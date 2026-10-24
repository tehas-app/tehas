//! A reader of the zip format, as much of it as an attachment needs: the end
//! of central directory record, the central directory, and the data of an
//! entry stored or deflated. No zip64, no archive split over disks, no other
//! compression method: an archive of a few megabytes needs none of them.
//!
//! Every offset and length comes from the archive, so every one is checked
//! against the bytes before it is used. Nothing here panics on any input.

use flate2::read::DeflateDecoder;
use flate2::Crc;
use std::io::{self, Read, Write};

/// The most entries read of one archive. One more and the rest are not read.
pub const MAX_ENTRIES: usize = 200;

const EOCD_SIG: u32 = 0x0605_4b50;
const CENTRAL_SIG: u32 = 0x0201_4b50;
const LOCAL_SIG: u32 = 0x0403_4b50;
const EOCD_LEN: usize = 22;
const CENTRAL_LEN: usize = 46;
const LOCAL_LEN: usize = 30;

/// One entry of the central directory.
#[derive(Debug, Clone)]
pub struct Entry {
    pub name: Vec<u8>,
    /// The Unix mode, for an entry written on Unix with mode bits set.
    pub unix_mode: Option<u32>,
    /// The MS-DOS directory attribute.
    pub dos_dir: bool,
    pub encrypted: bool,
    method: u16,
    crc: u32,
    compressed: u64,
    size: u64,
    local_offset: u64,
}

impl Entry {
    /// A directory: by its Unix type, or, with none, by its attribute or a
    /// name that ends in a slash.
    pub fn is_dir(&self) -> bool {
        match self.unix_type() {
            Some(t) => t == 0o040000,
            None => self.dos_dir || self.name.ends_with(b"/"),
        }
    }

    /// The file type bits of the Unix mode; None when there are none.
    pub fn unix_type(&self) -> Option<u32> {
        self.unix_mode.map(|m| m & 0o170000).filter(|t| *t != 0)
    }
}

pub struct Archive<'a> {
    data: &'a [u8],
    pub entries: Vec<Entry>,
    /// The archive holds more than MAX_ENTRIES entries.
    pub more: bool,
}

/// Why the bytes of an entry were not given.
#[derive(Debug)]
pub enum ReadError {
    /// More bytes than the limit.
    Over,
    /// Damaged, locked, or of a method this reader does not know.
    Bad,
    /// Writing them failed.
    Write,
}

fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    let b = d.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([b[0], b[1]]))
}

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    let b = d.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

impl<'a> Archive<'a> {
    /// The central directory of `data`, or why it cannot be read.
    pub fn open(data: &'a [u8]) -> Result<Archive<'a>, &'static str> {
        let eocd = find_eocd(data).ok_or("no end of central directory record")?;
        let disk = u16_at(data, eocd + 4).ok_or("short")?;
        let cd_disk = u16_at(data, eocd + 6).ok_or("short")?;
        let on_disk = u16_at(data, eocd + 8).ok_or("short")?;
        let total = u16_at(data, eocd + 10).ok_or("short")?;
        let cd_size = u32_at(data, eocd + 12).ok_or("short")?;
        let cd_offset = u32_at(data, eocd + 16).ok_or("short")?;
        if disk != 0 || cd_disk != 0 || on_disk != total {
            return Err("an archive split over disks");
        }
        if total == 0xffff || cd_size == 0xffff_ffff || cd_offset == 0xffff_ffff {
            return Err("a zip64 archive");
        }
        let start = cd_offset as usize;
        let end = start
            .checked_add(cd_size as usize)
            .filter(|e| *e <= eocd)
            .ok_or("the central directory is outside the archive")?;
        let dir = &data[start..end];
        let want = (total as usize).min(MAX_ENTRIES);
        let mut entries = Vec::with_capacity(want);
        let mut at = 0usize;
        for _ in 0..want {
            let (entry, next) = central_entry(dir, at).ok_or("a damaged central directory")?;
            entries.push(entry);
            at = next;
        }
        Ok(Archive {
            data,
            entries,
            more: total as usize > MAX_ENTRIES,
        })
    }

    /// Write the bytes of `entry` to `out`, at most `limit` of them. Returns
    /// how many were written. The checksum is checked at the end.
    pub fn read(&self, entry: &Entry, limit: u64, out: &mut dyn Write) -> Result<u64, ReadError> {
        if entry.encrypted {
            return Err(ReadError::Bad);
        }
        let local = usize::try_from(entry.local_offset).map_err(|_| ReadError::Bad)?;
        if u32_at(self.data, local) != Some(LOCAL_SIG) {
            return Err(ReadError::Bad);
        }
        let name_len = u16_at(self.data, local + 26).ok_or(ReadError::Bad)? as usize;
        let extra_len = u16_at(self.data, local + 28).ok_or(ReadError::Bad)? as usize;
        let start = local + LOCAL_LEN + name_len + extra_len;
        let len = usize::try_from(entry.compressed).map_err(|_| ReadError::Bad)?;
        let end = start.checked_add(len).ok_or(ReadError::Bad)?;
        let packed = self.data.get(start..end).ok_or(ReadError::Bad)?;
        match entry.method {
            0 => {
                if entry.compressed != entry.size {
                    return Err(ReadError::Bad);
                }
                copy(packed, entry, limit, out)
            }
            8 => copy(DeflateDecoder::new(packed), entry, limit, out),
            _ => Err(ReadError::Bad),
        }
    }
}

fn find_eocd(data: &[u8]) -> Option<usize> {
    let last = data.len().checked_sub(EOCD_LEN)?;
    // The record ends the archive but for a comment of at most 65535 bytes.
    let first = last.saturating_sub(0xffff);
    (first..=last).rev().find(|&at| {
        u32_at(data, at) == Some(EOCD_SIG)
            && u16_at(data, at + 20).map(|c| at + EOCD_LEN + c as usize) == Some(data.len())
    })
}

fn central_entry(dir: &[u8], at: usize) -> Option<(Entry, usize)> {
    if u32_at(dir, at)? != CENTRAL_SIG {
        return None;
    }
    let made_by = u16_at(dir, at + 4)?;
    let flags = u16_at(dir, at + 8)?;
    let method = u16_at(dir, at + 10)?;
    let crc = u32_at(dir, at + 16)?;
    let compressed = u32_at(dir, at + 20)?;
    let size = u32_at(dir, at + 24)?;
    let name_len = u16_at(dir, at + 28)? as usize;
    let extra_len = u16_at(dir, at + 30)? as usize;
    let comment_len = u16_at(dir, at + 32)? as usize;
    let external = u32_at(dir, at + 38)?;
    let local_offset = u32_at(dir, at + 42)?;
    let name_at = at + CENTRAL_LEN;
    let name = dir.get(name_at..name_at + name_len)?.to_vec();
    let next = name_at + name_len + extra_len + comment_len;
    if next > dir.len() {
        return None;
    }
    // 3 is Unix; the high half of the external attributes is then its mode.
    let unix_mode = (made_by >> 8 == 3)
        .then_some(external >> 16)
        .filter(|m| *m != 0);
    Some((
        Entry {
            name,
            unix_mode,
            dos_dir: external & 0x10 != 0,
            encrypted: flags & 1 != 0,
            method,
            crc,
            compressed: compressed as u64,
            size: size as u64,
            local_offset: local_offset as u64,
        },
        next,
    ))
}

fn copy(
    mut from: impl Read,
    entry: &Entry,
    limit: u64,
    out: &mut dyn Write,
) -> Result<u64, ReadError> {
    let mut crc = Crc::new();
    let mut buf = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let n = match from.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(ReadError::Bad),
        };
        total += n as u64;
        if total > limit {
            return Err(ReadError::Over);
        }
        crc.update(&buf[..n]);
        out.write_all(&buf[..n]).map_err(|_| ReadError::Write)?;
    }
    if crc.sum() != entry.crc || total != entry.size {
        return Err(ReadError::Bad);
    }
    Ok(total)
}
