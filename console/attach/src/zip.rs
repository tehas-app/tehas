//! A reader of the zip format, as much of it as an attachment needs: the end
//! of central directory record, the central directory, and the data of an
//! entry stored or deflated. No zip64, no archive split over disks, no other
//! compression method: an archive of a few megabytes needs none of them.
//!
//! Every offset and length comes from the archive, so every one is checked
//! against the bytes before it is used. Nothing here panics on any input.

use flate2::read::DeflateDecoder;
#[cfg(not(kani))]
use flate2::Crc;
#[cfg(kani)]
use proofs::Crc;
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
    // Its data shares bytes with another entry's: never read.
    overlaps: bool,
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
        let (mut entries, more) = directory(data, eocd)?;
        mark_overlaps(data, &mut entries);
        Ok(Archive {
            data,
            entries,
            more,
        })
    }

    /// Write the bytes of `entry` to `out`, at most `limit` of them and never
    /// more than the entry says it holds. Returns how many were written. The
    /// checksum is checked at the end. `inflated` counts every byte made,
    /// also of an entry that fails, so a caller can charge it.
    pub fn read(
        &self,
        entry: &Entry,
        limit: u64,
        out: &mut dyn Write,
        inflated: &mut u64,
    ) -> Result<u64, ReadError> {
        if entry.encrypted || entry.overlaps {
            return Err(ReadError::Bad);
        }
        // The size is the central directory's: one over the limit is refused
        // before a byte of it is inflated or written.
        if entry.size > limit {
            return Err(ReadError::Over);
        }
        let local = usize::try_from(entry.local_offset).map_err(|_| ReadError::Bad)?;
        if u32_at(self.data, local) != Some(LOCAL_SIG) {
            return Err(ReadError::Bad);
        }
        let name_len = u16_at(self.data, local.checked_add(26).ok_or(ReadError::Bad)?)
            .ok_or(ReadError::Bad)? as usize;
        let extra_len = u16_at(self.data, local.checked_add(28).ok_or(ReadError::Bad)?)
            .ok_or(ReadError::Bad)? as usize;
        let start = local
            .checked_add(LOCAL_LEN + name_len + extra_len)
            .ok_or(ReadError::Bad)?;
        let len = usize::try_from(entry.compressed).map_err(|_| ReadError::Bad)?;
        let end = start.checked_add(len).ok_or(ReadError::Bad)?;
        let packed = self.data.get(start..end).ok_or(ReadError::Bad)?;
        match entry.method {
            0 => {
                if entry.compressed != entry.size {
                    return Err(ReadError::Bad);
                }
                copy(packed, entry, limit, out, inflated)
            }
            8 => copy(DeflateDecoder::new(packed), entry, limit, out, inflated),
            _ => Err(ReadError::Bad),
        }
    }
}

// The entries of the central directory named by the record at `eocd`, which
// find_eocd found, and whether there are more than MAX_ENTRIES.
fn directory(data: &[u8], eocd: usize) -> Result<(Vec<Entry>, bool), &'static str> {
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
    Ok((entries, total as usize > MAX_ENTRIES))
}

/// Mark every entry whose local record and data share a byte with another's.
/// One stream named by many entries would otherwise be inflated once each.
/// An entry whose span cannot be worked out is marked too, and left out of
/// the comparison: its local record is not in the archive.
fn mark_overlaps(data: &[u8], entries: &mut [Entry]) {
    let span = |e: &Entry| -> Option<(u64, u64)> {
        let at = e.local_offset;
        let l = usize::try_from(at).ok()?;
        let head =
            u16_at(data, l.checked_add(26)?)? as u64 + u16_at(data, l.checked_add(28)?)? as u64;
        let end = at
            .checked_add(LOCAL_LEN as u64)?
            .checked_add(head)?
            .checked_add(e.compressed)?;
        Some((at, end))
    };
    // One slot per entry, None sorted first and passed over: a vector as long
    // as the entries keeps the sort within reach of the Kani proof.
    let mut spans: Vec<Option<(u64, u64, usize)>> = entries
        .iter_mut()
        .enumerate()
        .map(|(i, e)| {
            let s = span(e).map(|(a, b)| (a, b, i));
            e.overlaps |= s.is_none();
            s
        })
        .collect();
    spans.sort_unstable();
    let mut reach: Option<(u64, usize)> = None;
    for &(start, end, i) in spans.iter().flatten() {
        if let Some((far, j)) = reach {
            if start < far {
                entries[i].overlaps = true;
                entries[j].overlaps = true;
            }
        }
        if reach.is_none_or(|(far, _)| end > far) {
            reach = Some((end, i));
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
            overlaps: false,
        },
        next,
    ))
}

fn copy(
    mut from: impl Read,
    entry: &Entry,
    limit: u64,
    out: &mut dyn Write,
    inflated: &mut u64,
) -> Result<u64, ReadError> {
    let mut crc = Crc::new();
    // Small under Kani, which cannot solve a buffer this size.
    let mut buf = [0u8; if cfg!(kani) { 64 } else { 64 * 1024 }];
    let mut total = 0u64;
    loop {
        let n = match from.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(ReadError::Bad),
        };
        total += n as u64;
        *inflated += n as u64;
        if total > entry.size {
            return Err(ReadError::Bad);
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(local_offset: u64, compressed: u64) -> Entry {
        Entry {
            name: b"a.txt".to_vec(),
            unix_mode: None,
            dos_dir: false,
            encrypted: false,
            method: 0,
            crc: 0,
            compressed,
            size: compressed,
            local_offset,
            overlaps: false,
        }
    }

    // A local record of `name_len` name bytes at 0, then `data` bytes.
    fn local(name_len: u16, data: usize) -> Vec<u8> {
        let mut d = vec![0u8; LOCAL_LEN];
        d[..4].copy_from_slice(&LOCAL_SIG.to_le_bytes());
        d[26..28].copy_from_slice(&name_len.to_le_bytes());
        d.resize(LOCAL_LEN + name_len as usize + data, b'x');
        d
    }

    #[test]
    fn an_entry_whose_span_overflows_is_marked_and_refused() {
        let data = local(1, 4);
        let mut entries = vec![entry(u64::MAX - 8, 4), entry(0, 4)];
        mark_overlaps(&data, &mut entries);
        assert!(entries[0].overlaps);
        assert!(!entries[1].overlaps);
        let archive = Archive {
            data: &data,
            entries: entries.clone(),
            more: false,
        };
        let mut inflated = 0;
        assert!(matches!(
            archive.read(&entries[0], 100, &mut Vec::new(), &mut inflated),
            Err(ReadError::Bad)
        ));
    }

    #[test]
    fn an_entry_whose_local_record_is_past_the_end_is_marked() {
        let data = local(1, 4);
        let mut entries = vec![entry(data.len() as u64 - 2, 4), entry(0, 4)];
        mark_overlaps(&data, &mut entries);
        assert!(entries[0].overlaps);
        assert!(!entries[1].overlaps);
    }

    #[test]
    fn entries_that_share_bytes_are_both_marked() {
        let data = local(1, 100);
        let mut entries = vec![entry(0, 4), entry(4, 4)];
        mark_overlaps(&data, &mut entries);
        assert!(entries[0].overlaps && entries[1].overlaps);
    }
}

#[cfg(kani)]
mod proofs {
    use super::*;

    // Any bytes, up to N of them.
    fn bytes<const N: usize>(buf: &[u8; N]) -> &[u8] {
        let len: usize = kani::any();
        kani::assume(len <= N);
        &buf[..len]
    }

    // The checksum asks for CPU features Kani cannot model. Any sum stands in
    // for it, so a proof holds whether the check passes or fails.
    pub struct Crc;

    impl Crc {
        pub fn new() -> Crc {
            Crc
        }
        pub fn update(&mut self, _: &[u8]) {}
        pub fn sum(&self) -> u32 {
            kani::any()
        }
    }

    fn any_entry() -> Entry {
        Entry {
            name: Vec::new(),
            unix_mode: None,
            dos_dir: false,
            encrypted: kani::any(),
            method: kani::any(),
            crc: kani::any(),
            compressed: kani::any(),
            size: kani::any(),
            local_offset: kani::any(),
            overlaps: kani::any(),
        }
    }

    fn eocd_found_in(data: &[u8]) {
        if let Some(at) = find_eocd(data) {
            assert!(at + EOCD_LEN <= data.len());
            assert_eq!(u32_at(data, at), Some(EOCD_SIG));
            kani::cover!(at == 0);
            kani::cover!(at > 0);
        }
    }

    // The scan costs the solver about the square of the length, and more for
    // a length it picks: every length up to 26, and 64 bytes, which hold a
    // comment of up to 42.
    #[kani::proof]
    #[kani::unwind(28)]
    fn find_eocd_never_panics_up_to_26() {
        let buf: [u8; 26] = kani::any();
        eocd_found_in(bytes(&buf));
    }

    #[kani::proof]
    #[kani::unwind(46)]
    fn find_eocd_never_panics_at_64() {
        let buf: [u8; 64] = kani::any();
        eocd_found_in(&buf);
    }

    #[kani::proof]
    #[kani::unwind(4)]
    fn central_entry_never_panics() {
        let buf: [u8; 128] = kani::any();
        let dir = bytes(&buf);
        let at: usize = kani::any();
        if let Some((entry, next)) = central_entry(dir, at) {
            assert!(next >= at + CENTRAL_LEN + entry.name.len());
            assert!(next <= dir.len());
            kani::cover!(!entry.name.is_empty());
        }
    }

    // open is find_eocd, directory and mark_overlaps, each proven here.
    // directory reads no byte past the record, so the record ends the bytes,
    // and the directory lies anywhere in the 106 before it. Its loop calls
    // central_entry, proven for any offset, so one pass of it is enough and
    // keeps the solver to under a minute.
    #[kani::proof]
    #[kani::unwind(2)]
    fn directory_never_panics() {
        let buf: [u8; 128] = kani::any();
        let data = &buf[..];
        let eocd = data.len() - EOCD_LEN;
        kani::assume(u16_at(data, eocd + 10).is_some_and(|n| n <= 1));
        if let Ok((entries, more)) = directory(data, eocd) {
            assert!(!more && entries.len() <= 1);
            kani::cover!(entries.len() == 1);
        }
    }

    // Two entries left unmarked span bytes that do not meet. Three take the
    // solver minutes.
    #[kani::proof]
    #[kani::unwind(3)]
    fn mark_overlaps_never_panics_and_leaves_no_overlap() {
        let buf: [u8; 64] = kani::any();
        let data = bytes(&buf);
        let mut entries = [any_entry(), any_entry()];
        for e in entries.iter_mut() {
            e.overlaps = false;
        }
        mark_overlaps(data, &mut entries);
        let span = |e: &Entry| {
            let l = e.local_offset as usize;
            let head = u16_at(data, l + 26).unwrap() as u64 + u16_at(data, l + 28).unwrap() as u64;
            (
                e.local_offset,
                e.local_offset + LOCAL_LEN as u64 + head + e.compressed,
            )
        };
        for i in 0..entries.len() {
            for j in 0..entries.len() {
                if i != j && !entries[i].overlaps && !entries[j].overlaps {
                    let (a, b) = span(&entries[i]);
                    let (c, d) = span(&entries[j]);
                    assert!(b <= c || d <= a);
                }
            }
        }
        kani::cover!(!entries[0].overlaps && !entries[1].overlaps);
        kani::cover!(entries[0].overlaps && !entries[1].overlaps);
    }

    // Inflate is left out: the method is fixed to stored, or the solver walks
    // into the decoder. What is written is the entry's bytes in the archive.
    #[kani::proof]
    #[kani::unwind(3)]
    fn read_of_a_stored_entry_gives_only_archive_bytes() {
        let buf: [u8; 48] = kani::any();
        let data = bytes(&buf);
        let mut entry = any_entry();
        entry.method = 0;
        let archive = Archive {
            data,
            entries: Vec::new(),
            more: false,
        };
        let limit: u64 = kani::any();
        let mut out = Vec::new();
        let mut inflated = 0;
        if let Ok(n) = archive.read(&entry, limit, &mut out, &mut inflated) {
            assert!(n <= limit && n == entry.size && n == inflated);
            assert_eq!(out.len() as u64, n);
            let l = entry.local_offset as usize;
            let start = l
                + LOCAL_LEN
                + u16_at(data, l + 26).unwrap() as usize
                + u16_at(data, l + 28).unwrap() as usize;
            let k: usize = kani::any();
            if k < out.len() {
                assert_eq!(out[k], data[start + k]);
            }
            kani::cover!(n > 0);
        }
    }
}
