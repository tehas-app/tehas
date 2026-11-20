//! Unpack an archive attached to an issue into a directory. Anyone who may
//! attach to the repository wrote it, so nothing in it is trusted: not its
//! names, not its file types, not the sizes it claims.
//!
//! Every entry goes through these rules, in this order:
//!   0. at most 200 entries are read
//!   1. only regular files, and entries with no Unix file type, which zip
//!      libraries write for plain files; a directory is passed over, a link or
//!      any other kind is left out, and so is an encrypted file
//!   2. no path that starts with /, has a .. part, has a backslash, or is more
//!      than six levels deep
//!   3. each part of the path cleaned as protolab cleans a name; a part that
//!      ends up empty leaves the entry out
//!   4. only the types protolab hosts, and md
//!   5. at most --max-files files
//!   6. at most --max-bytes in all, counted on what is inflated and not on
//!      what the archive says, a damaged entry's bytes included; after the
//!      first file over it, nothing more is unpacked and the rest are left out
//!      for the same reason. An entry that claims more than is left is over
//!      it before a byte is inflated. An entry that inflates past the size it
//!      claims, or whose data shares bytes with another entry's, is damaged
//!   7. nothing written through a link: not when the directory is one, nor
//!      when a part of the entry's path that already stands under it is one,
//!      and no file over one already there
//!
//! Files are made new, mode 644; no link is ever made.

use crate::sniff::Sniffer;
use crate::zip::{Archive, Entry, ReadError, MAX_ENTRIES};
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

const OUTSIDE: &str = "A path outside the archive.";
const UNUSABLE: &str = "A name the lab cannot use.";
const FULL: &str = "Over the 10 MB limit of a ticket.";

/// The kind protolab hosts a file of this extension as; None for one it does
/// not. The same table as `kind_of` in console/src/protolab/build.rs, and md.
pub fn kind(ext: &str) -> Option<&'static str> {
    match ext {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "avif" => Some("image"),
        "mp4" | "webm" | "mov" => Some("video"),
        "html" | "htm" => Some("app"),
        "css" | "js" | "mjs" | "json" | "txt" | "md" => Some("part"),
        _ => None,
    }
}

/// One part of a path as protolab cleans a name: every byte but A-Z a-z 0-9
/// . _ - made '-', leading dots and dashes dropped, at most 80 bytes.
pub fn clean_part(part: &[u8]) -> String {
    clean_bytes(part).into_iter().map(char::from).collect()
}

// clean_part as ASCII bytes. Bytes, not chars: the Kani proof below cannot
// afford decoding.
fn clean_bytes(part: &[u8]) -> Vec<u8> {
    let mapped: Vec<u8> = part
        .iter()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-' => *b,
            _ => b'-',
        })
        .collect();
    let lead = mapped
        .iter()
        .take_while(|b| matches!(b, b'.' | b'-'))
        .count();
    let end = mapped.len().min(lead + 80);
    mapped[lead..end].to_vec()
}

/// The name of an entry as it is said: the archive's own, cut at 200
/// characters.
fn said(name: &[u8]) -> String {
    String::from_utf8_lossy(name).chars().take(200).collect()
}

fn skip(name: &str, reason: &str) -> Value {
    json!({"skip": name, "reason": reason})
}

// The first `n` spans of the array, as (start, end) in the name.
type Parts = ([(usize, usize); 6], usize);

/// The parts of a name, as bash reads them split on '/': one slash at the end
/// ends the last part, and two in a row hold an empty one. OUTSIDE for a name
/// that leaves the destination. By index, not slice::split, which the Kani
/// proof below cannot afford.
fn parts(name: &[u8]) -> Result<Parts, &'static str> {
    if name.starts_with(b"/") || name.contains(&b'\\') {
        return Err(OUTSIDE);
    }
    let body = name.strip_suffix(b"/").unwrap_or(name);
    let mut parts = [(0, 0); 6];
    let mut n = 0;
    for (i, b) in body.iter().enumerate() {
        if *b == b'/' {
            // A seventh part.
            if n == parts.len() - 1 {
                return Err(OUTSIDE);
            }
            parts[n].1 = i;
            n += 1;
            parts[n].0 = i + 1;
        }
    }
    parts[n].1 = body.len();
    n += 1;
    if parts[..n].iter().any(|&(s, e)| &body[s..e] == b"..") {
        return Err(OUTSIDE);
    }
    Ok((parts, n))
}

/// Why the path of an entry is not taken, or its cleaned path.
fn clean_path(name: &[u8]) -> Result<String, &'static str> {
    let (parts, n) = parts(name)?;
    let mut rel = Vec::with_capacity(n);
    for &(start, end) in &parts[..n] {
        let clean = clean_part(&name[start..end]);
        if clean.is_empty() {
            return Err(UNUSABLE);
        }
        rel.push(clean);
    }
    Ok(rel.join("/"))
}

fn is_link(p: &Path) -> bool {
    fs::symlink_metadata(p)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

/// Make the folders of `rel` under `dest`, one at a time; false when a part
/// is there and is not a folder.
fn make_parents(dest: &Path, rel: &str) -> bool {
    let mut at = dest.to_path_buf();
    let mut parts: Vec<&str> = rel.split('/').collect();
    parts.pop();
    for part in parts {
        at.push(part);
        match fs::symlink_metadata(&at) {
            Ok(m) if m.is_dir() => continue,
            Ok(_) => return false,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                if fs::create_dir(&at).is_err() {
                    return false;
                }
            }
            Err(_) => return false,
        }
    }
    true
}

struct Tee<'a> {
    file: &'a mut fs::File,
    sniff: &'a mut Sniffer,
}

impl Write for Tee<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file.write_all(buf)?;
        self.sniff.feed(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

enum Written {
    File(Value),
    Full,
    // What it inflated before it failed.
    Bad(u64),
}

fn write_entry(archive: &Archive, entry: &Entry, path: &Path, left: u64) -> Written {
    let mut file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o644)
        .open(path)
    {
        Ok(f) => f,
        Err(_) => return Written::Bad(0),
    };
    let mut sniff = Sniffer::default();
    let mut inflated = 0;
    let got = archive.read(
        entry,
        left,
        &mut Tee {
            file: &mut file,
            sniff: &mut sniff,
        },
        &mut inflated,
    );
    let ok = got.is_ok()
        && file.sync_all().is_ok()
        && fs::set_permissions(path, fs::Permissions::from_mode(0o644)).is_ok();
    drop(file);
    match got {
        Ok(_) if ok => {
            let s = sniff.finish();
            Written::File(json!({
                "bytes": s.bytes,
                "sha256": s.sha256,
                "image": s.image,
                "text": s.text,
            }))
        }
        Err(ReadError::Over) => {
            let _ = fs::remove_file(path);
            Written::Full
        }
        _ => {
            let _ = fs::remove_file(path);
            Written::Bad(inflated)
        }
    }
}

/// Unpack `archive` into `dest`, which is made when it is not there. One value
/// per entry, in the archive's order: {"name","kind","bytes","sha256","image",
/// "text"} for a file written, {"skip","reason"} for one left out. Err when
/// `dest` cannot be made.
pub fn unpack(
    archive: &Archive,
    dest: &Path,
    max_bytes: u64,
    max_files: i64,
) -> io::Result<Vec<Value>> {
    fs::create_dir_all(dest)?;
    let mut out = Vec::new();
    let mut left = max_bytes;
    let mut files = max_files;
    let mut full = false;
    for entry in &archive.entries {
        let name = said(&entry.name);
        if entry.is_dir() {
            continue;
        }
        if entry.unix_type().is_some_and(|t| t != 0o100000) {
            out.push(skip(&name, "A link or a special file."));
            continue;
        }
        if entry.encrypted {
            out.push(skip(&name, "A locked file."));
            continue;
        }
        let rel = match clean_path(&entry.name) {
            Ok(rel) => rel,
            Err(why) => {
                out.push(skip(&name, why));
                continue;
            }
        };
        let last = rel.rsplit('/').next().unwrap_or(&rel);
        let ext = last
            .rsplit_once('.')
            .map(|(_, e)| e.to_ascii_lowercase())
            .unwrap_or_default();
        let Some(kind) = kind(&ext) else {
            out.push(skip(&name, "Not a type the lab hosts."));
            continue;
        };
        if files <= 0 {
            out.push(skip(&name, &format!("More than {max_files} files.")));
            continue;
        }
        if full {
            out.push(skip(&name, FULL));
            continue;
        }
        let mut probe = dest.to_path_buf();
        let mut link = is_link(dest);
        for part in rel.split('/') {
            probe.push(part);
            link |= is_link(&probe);
        }
        if link {
            out.push(skip(&name, OUTSIDE));
            continue;
        }
        let path: PathBuf = dest.join(&rel);
        if fs::symlink_metadata(&path).is_ok() {
            out.push(skip(&name, "A second file of that name."));
            continue;
        }
        if !make_parents(dest, &rel) {
            out.push(skip(&name, UNUSABLE));
            continue;
        }
        match write_entry(archive, entry, &path, left) {
            Written::File(mut v) => {
                let bytes = v["bytes"].as_u64().unwrap_or(0);
                left -= bytes.min(left);
                files -= 1;
                v["name"] = json!(rel);
                v["kind"] = json!(kind);
                out.push(v);
            }
            Written::Full => {
                full = true;
                out.push(skip(&name, FULL));
            }
            // A damaged entry is charged what it inflated, so damaged
            // entries cannot cost more than the room given.
            Written::Bad(inflated) => {
                left -= inflated.min(left);
                out.push(skip(&name, "Could not be unpacked."));
            }
        }
    }
    if archive.more {
        out.push(skip(
            "...",
            &format!("More than {MAX_ENTRIES} entries in the zip."),
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_path_reads_parts_as_bash_does() {
        assert_eq!(clean_path(b"a b/c.txt"), Ok("a-b/c.txt".into()));
        assert_eq!(clean_path(b"1/2/3/4/5/6/"), Ok("1/2/3/4/5/6".into()));
        assert_eq!(clean_path(b"1/2/3/4/5/6/7"), Err(OUTSIDE));
        assert_eq!(clean_path(b"a///////"), Err(OUTSIDE));
        // Outside wins over an empty part, wherever each is.
        assert_eq!(clean_path(b"a//../x"), Err(OUTSIDE));
        assert_eq!(clean_path(b"../a//"), Err(OUTSIDE));
        assert_eq!(clean_path(b"a/.."), Err(OUTSIDE));
        assert_eq!(clean_path(b"a/../"), Err(OUTSIDE));
        assert_eq!(clean_path(b"a/.../b"), Err(UNUSABLE));
        assert_eq!(clean_path(b"a//b"), Err(UNUSABLE));
        assert_eq!(clean_path(b""), Err(UNUSABLE));
        assert_eq!(clean_path(b"a//"), Err(UNUSABLE));
    }
}

#[cfg(kani)]
mod proofs {
    use super::*;

    // A cleaned part holds only A-Z a-z 0-9 . _ - and starts with neither a
    // dot nor a dash: no slash, no backslash, never "..". With clean_path's
    // check that no part comes out empty and parts_stay_inside, every path
    // clean_path takes stays under the destination. A fixed 16 bytes, which
    // the solver takes far faster than a length it picks: a shorter part
    // cleans to the start of what it cleans to with any bytes after it, so
    // the proof holds for it too.
    #[kani::proof]
    #[kani::unwind(18)]
    fn clean_part_keeps_safe_bytes() {
        let buf: [u8; 16] = kani::any();
        let len = buf.len();
        let b = clean_bytes(&buf);
        assert!(b.len() <= len);
        assert!(b
            .iter()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-')));
        assert!(!b.starts_with(b".") && !b.starts_with(b"-"));
        kani::cover!(b.len() == 16);
        kani::cover!(!b.is_empty() && b.len() < len);
    }

    // A name taken has 1 to 6 parts, in order, each inside the name, none
    // holding a slash or a backslash, none "..", and no slash before the
    // first. Names of up to 10 bytes, which reach 6 parts and a seventh;
    // 16 take the solver minutes.
    #[kani::proof]
    #[kani::unwind(12)]
    fn parts_stay_inside() {
        let buf: [u8; 10] = kani::any();
        let len: usize = kani::any();
        kani::assume(len <= buf.len());
        let name = &buf[..len];
        if let Ok((parts, n)) = parts(name) {
            assert!((1..=6).contains(&n));
            assert!(parts[0].0 == 0 && name.first() != Some(&b'/'));
            let k: usize = kani::any();
            kani::assume(k < n);
            let (start, end) = parts[k];
            assert!(start <= end && end <= name.len());
            if k + 1 < n {
                assert!(name[end] == b'/' && parts[k + 1].0 == end + 1);
            }
            let i: usize = kani::any();
            if start <= i && i < end {
                assert!(name[i] != b'/' && name[i] != b'\\');
            }
            assert!(&name[start..end] != b"..");
            kani::cover!(n == 6);
            kani::cover!(n == 2 && name.ends_with(b"/"));
        }
    }
}
