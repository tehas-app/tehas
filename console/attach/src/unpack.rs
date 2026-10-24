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
//!   6. at most --max-bytes in all, counted on what is written and not on what
//!      the archive says; after the first file over it, nothing more is
//!      unpacked and the rest are left out for the same reason
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
/// not. The same table as tehas_attachment_kind in lib/attachments.sh, and md.
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
    let mapped: String = part
        .iter()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-' => *b as char,
            _ => '-',
        })
        .collect();
    let mut kept = mapped.trim_start_matches(['.', '-']).to_string();
    kept.truncate(80);
    kept
}

/// The name of an entry as it is said: the archive's own, cut at 200
/// characters.
fn said(name: &[u8]) -> String {
    String::from_utf8_lossy(name).chars().take(200).collect()
}

fn skip(name: &str, reason: &str) -> Value {
    json!({"skip": name, "reason": reason})
}

/// The parts of a name, as bash reads them split on '/': one slash at the end
/// ends the last part, and two in a row hold an empty one.
fn parts(name: &[u8]) -> Vec<&[u8]> {
    let name = name.strip_suffix(b"/").unwrap_or(name);
    name.split(|b| *b == b'/').collect()
}

/// Why the path of an entry is not taken, or its cleaned path.
fn clean_path(name: &[u8]) -> Result<String, &'static str> {
    let parts = parts(name);
    if name.starts_with(b"/")
        || name.contains(&b'\\')
        || parts.iter().any(|p| *p == b"..")
        || parts.len() > 6
    {
        return Err(OUTSIDE);
    }
    let mut rel = Vec::with_capacity(parts.len());
    for part in parts {
        let clean = clean_part(part);
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
    Bad,
}

fn write_entry(archive: &Archive, entry: &Entry, path: &Path, left: u64) -> Written {
    let mut file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o644)
        .open(path)
    {
        Ok(f) => f,
        Err(_) => return Written::Bad,
    };
    let mut sniff = Sniffer::default();
    let got = archive.read(
        entry,
        left,
        &mut Tee {
            file: &mut file,
            sniff: &mut sniff,
        },
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
            Written::Bad
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
            Written::Bad => out.push(skip(&name, "Could not be unpacked.")),
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
