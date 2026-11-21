//! The reader of what a plugin's run left: the tar stream of `docker cp
//! <container>:/out -` (docs/specs/2026-11-21-plugins-security.md, "Reading
//! /out and /state"). It takes regular files and directories only, with
//! names that stay under the directory it writes, and nothing a run left is
//! opened as a path on the host. ustar, GNU long names and pax headers are
//! read; anything else in the stream fails it.

use std::collections::BTreeSet;
use std::fs::{DirBuilder, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::Path;

/// The most entries of a view's `/out`.
pub const ENTRIES: usize = 5000;
/// The most bytes of a view's files together.
pub const BYTES: u64 = 256 << 20;
/// The most bytes of a long name or of a pax header.
const META_MAX: u64 = 64 << 10;
const BLOCK: usize = 512;

/// How much a stream may hold.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub entries: usize,
    pub bytes: u64,
}

/// What a stream that was taken held.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Taken {
    pub files: usize,
    pub dirs: usize,
    pub bytes: u64,
}

/// The kind a type flag, or the type bits of a mode, names.
fn kind(flag: u8, mode: u64) -> Result<bool, &'static str> {
    let by_flag = match flag {
        b'0' | 0 => Ok(false),
        b'5' => Ok(true),
        b'1' => Err("a hard link"),
        b'2' => Err("a symlink"),
        b'3' => Err("a character device"),
        b'4' => Err("a block device"),
        b'6' => Err("a fifo"),
        b'K' => Err("a link's long name"),
        _ => Err("an entry of a kind this reader does not take"),
    }?;
    match mode & 0o170000 {
        0 => Ok(by_flag),
        0o100000 if !by_flag => Ok(false),
        0o040000 if by_flag => Ok(true),
        0o140000 => Err("a socket"),
        0o120000 => Err("a symlink"),
        0o010000 => Err("a fifo"),
        0o020000 => Err("a character device"),
        0o060000 => Err("a block device"),
        _ => Err("an entry whose mode and type disagree"),
    }
}

/// An octal field: digits, then spaces or NULs. A base-256 size, which only
/// a file past any limit here needs, is refused.
fn octal(field: &[u8]) -> Option<u64> {
    let text = field
        .iter()
        .skip_while(|b| **b == b' ')
        .take_while(|b| **b != 0 && **b != b' ')
        .copied()
        .collect::<Vec<u8>>();
    let rest_blank = field
        .iter()
        .skip_while(|b| **b == b' ')
        .skip(text.len())
        .all(|b| *b == 0 || *b == b' ');
    if !rest_blank || text.iter().any(|b| !(b'0'..=b'7').contains(b)) {
        return None;
    }
    if text.is_empty() {
        return Some(0);
    }
    u64::from_str_radix(std::str::from_utf8(&text).ok()?, 8).ok()
}

/// A NUL-ended field.
fn field(f: &[u8]) -> &[u8] {
    let end = f.iter().position(|b| *b == 0).unwrap_or(f.len());
    &f[..end]
}

fn checksum_ok(h: &[u8; BLOCK]) -> bool {
    let sum: u64 = h
        .iter()
        .enumerate()
        .map(|(i, b)| {
            if (148..156).contains(&i) {
                32
            } else {
                *b as u64
            }
        })
        .sum();
    octal(&h[148..156]) == Some(sum)
}

/// The path keys of a pax header: `path` and `size`; a link's is refused.
fn pax(data: &[u8], path: &mut Option<Vec<u8>>, size: &mut Option<u64>) -> Result<(), String> {
    let mut rest = data;
    while !rest.is_empty() {
        let sp = rest
            .iter()
            .position(|b| *b == b' ')
            .ok_or("a pax header that cannot be read")?;
        let len: usize = std::str::from_utf8(&rest[..sp])
            .ok()
            .and_then(|n| n.parse().ok())
            .filter(|n| *n > sp + 1 && *n <= rest.len())
            .ok_or("a pax header that cannot be read")?;
        let record = &rest[sp + 1..len];
        let record = record
            .strip_suffix(b"\n")
            .ok_or("a pax header that cannot be read")?;
        let eq = record
            .iter()
            .position(|b| *b == b'=')
            .ok_or("a pax header that cannot be read")?;
        let (key, value) = (&record[..eq], &record[eq + 1..]);
        match key {
            b"path" => *path = Some(value.to_vec()),
            b"size" => {
                *size = Some(
                    std::str::from_utf8(value)
                        .ok()
                        .and_then(|v| v.parse().ok())
                        .ok_or("a pax size that cannot be read")?,
                )
            }
            b"linkpath" => return Err("a link's name in a pax header".to_string()),
            _ if key.starts_with(b"GNU.sparse") => return Err("a sparse file".to_string()),
            _ => {}
        }
        rest = &rest[len..];
    }
    Ok(())
}

/// The parts of a name that stays under the target: valid UTF-8, no
/// control character, not absolute, no `.` or `..`, nothing empty but a
/// directory's last slash. Err says what is wrong with it.
fn parts(name: &[u8], dir: bool) -> Result<Vec<String>, String> {
    let name = std::str::from_utf8(name).map_err(|_| "a name that is not UTF-8".to_string())?;
    let shown = name.escape_debug().to_string();
    if name.chars().any(char::is_control) {
        return Err(format!("{shown}: a control character in a name"));
    }
    if name.starts_with('/') {
        return Err(format!("{shown}: an absolute name"));
    }
    let name = if dir {
        name.strip_suffix('/').unwrap_or(name)
    } else {
        name
    };
    let list: Vec<&str> = name.split('/').collect();
    if list.contains(&"..") {
        return Err(format!("{shown}: a name with .."));
    }
    if list.iter().any(|p| p.is_empty() || *p == ".") {
        return Err(format!("{shown}: a name with an empty part or ."));
    }
    Ok(list.into_iter().map(str::to_string).collect())
}

/// Bytes of the stream, exactly, or why it ended.
fn take(r: &mut impl Read, buf: &mut [u8]) -> Result<(), String> {
    r.read_exact(buf)
        .map_err(|e| format!("the stream ends inside an entry: {e}"))
}

/// The data of an entry and its padding, at most `max` bytes, read whole.
fn data(r: &mut impl Read, size: u64, max: u64) -> Result<Vec<u8>, String> {
    if size > max {
        return Err(format!("a header of {size} bytes, more than {max}"));
    }
    let mut buf = vec![0u8; padded(size)];
    take(r, &mut buf)?;
    buf.truncate(size as usize);
    Ok(buf)
}

fn padded(size: u64) -> usize {
    (size as usize).div_ceil(BLOCK) * BLOCK
}

/// Read the stream `r` into `target`, which it creates and which must not
/// exist. Every name must be `top` or under `top/`; what is under it is
/// written under `target`. On Err, the reason, `target` is removed.
pub fn read(r: impl Read, target: &Path, top: &str, limits: Limits) -> Result<Taken, String> {
    DirBuilder::new()
        .mode(0o755)
        .create(target)
        .map_err(|e| format!("cannot create {}: {e}", target.display()))?;
    let got = unpack(r, target, top, limits);
    if got.is_err() {
        let _ = std::fs::remove_dir_all(target);
    }
    got
}

fn unpack(mut r: impl Read, target: &Path, top: &str, limits: Limits) -> Result<Taken, String> {
    let mut taken = Taken::default();
    let mut made: BTreeSet<Vec<String>> = BTreeSet::new();
    made.insert(Vec::new());
    let mut entries = 0usize;
    let mut long_name: Option<Vec<u8>> = None;
    let mut pax_path: Option<Vec<u8>> = None;
    let mut pax_size: Option<u64> = None;
    let mut h = [0u8; BLOCK];
    loop {
        take(&mut r, &mut h).map_err(|_| "the stream ends before its end".to_string())?;
        if h.iter().all(|b| *b == 0) {
            if long_name.is_some() || pax_path.is_some() || pax_size.is_some() {
                return Err("the stream ends after a header of no entry".to_string());
            }
            return Ok(taken);
        }
        if !checksum_ok(&h) {
            return Err("a header whose checksum is wrong".to_string());
        }
        let flag = h[156];
        let size = octal(&h[124..136]).ok_or("a size that cannot be read")?;
        match flag {
            b'L' => {
                long_name = Some(field(&data(&mut r, size, META_MAX)?).to_vec());
                continue;
            }
            b'x' => {
                pax(&data(&mut r, size, META_MAX)?, &mut pax_path, &mut pax_size)?;
                continue;
            }
            b'g' => return Err("a global pax header".to_string()),
            _ => {}
        }
        entries += 1;
        if entries > limits.entries {
            return Err(format!("more than {} entries", limits.entries));
        }
        let mode = octal(&h[100..108]).ok_or("a mode that cannot be read")?;
        let size = pax_size.take().unwrap_or(size);
        let ustar = &h[257..263] == b"ustar\0";
        let name = match (pax_path.take(), long_name.take()) {
            (Some(p), _) | (None, Some(p)) => p,
            (None, None) if ustar && h[345] != 0 => {
                let mut n = field(&h[345..500]).to_vec();
                n.push(b'/');
                n.extend_from_slice(field(&h[..100]));
                n
            }
            (None, None) => field(&h[..100]).to_vec(),
        };
        let shown = String::from_utf8_lossy(&name).escape_debug().to_string();
        let dir = kind(flag, mode).map_err(|what| format!("{shown}: {what}"))?;
        let mut list = parts(&name, dir)?;
        if list.first().map(String::as_str) != Some(top) {
            return Err(format!("{shown}: a name outside {top}/"));
        }
        list.remove(0);
        if dir {
            if size != 0 {
                return Err(format!("{shown}: a directory with data"));
            }
            make_dirs(target, &list, &mut made)?;
            taken.dirs += 1;
            continue;
        }
        if list.is_empty() {
            return Err(format!("{shown}: {top} itself as a file"));
        }
        if taken.bytes + size > limits.bytes {
            return Err(format!("more than {} bytes", limits.bytes));
        }
        make_dirs(target, &list[..list.len() - 1], &mut made)?;
        let path = list
            .iter()
            .fold(target.to_path_buf(), |p, part| p.join(part));
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o644)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&path)
            .map_err(|e| format!("{shown}: cannot write it: {e}"))?;
        let mut left = size;
        let mut buf = [0u8; 64 * BLOCK];
        while left > 0 {
            let n = left.min(buf.len() as u64) as usize;
            take(&mut r, &mut buf[..n])?;
            f.write_all(&buf[..n])
                .map_err(|e| format!("{shown}: cannot write it: {e}"))?;
            left -= n as u64;
        }
        let pad = padded(size) - size as usize;
        take(&mut r, &mut buf[..pad])?;
        taken.files += 1;
        taken.bytes += size;
    }
}

/// Each directory of `list` under `target`, from the top, made new unless
/// this read made it already.
fn make_dirs(
    target: &Path,
    list: &[String],
    made: &mut BTreeSet<Vec<String>>,
) -> Result<(), String> {
    for n in 1..=list.len() {
        let key = list[..n].to_vec();
        if made.contains(&key) {
            continue;
        }
        let path = key
            .iter()
            .fold(target.to_path_buf(), |p, part| p.join(part));
        DirBuilder::new()
            .mode(0o755)
            .create(&path)
            .map_err(|e| format!("{}: cannot make it: {e}", key.join("/")))?;
        made.insert(key);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdir::TestDir;
    use std::fs;

    const ALL: Limits = Limits {
        entries: ENTRIES,
        bytes: BYTES,
    };

    /// One ustar header.
    fn header(name: &[u8], flag: u8, size: usize, mode: u32) -> [u8; BLOCK] {
        let mut h = [0u8; BLOCK];
        h[..name.len()].copy_from_slice(name);
        h[100..108].copy_from_slice(format!("{mode:07o}\0").as_bytes());
        h[108..116].copy_from_slice(b"0023421\0");
        h[116..124].copy_from_slice(b"0023421\0");
        h[124..136].copy_from_slice(format!("{size:011o}\0").as_bytes());
        h[136..148].copy_from_slice(b"00000000000\0");
        h[156] = flag;
        h[257..263].copy_from_slice(b"ustar\0");
        h[263..265].copy_from_slice(b"00");
        h[148..156].copy_from_slice(b"        ");
        let sum: u32 = h.iter().map(|b| *b as u32).sum();
        h[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
        h
    }

    /// A tar of entries `(name, flag, data, mode)`, ended by two zero blocks.
    #[derive(Default)]
    struct Tar(Vec<u8>);

    impl Tar {
        fn entry(mut self, name: &[u8], flag: u8, data: &[u8], mode: u32) -> Self {
            self.0
                .extend_from_slice(&header(name, flag, data.len(), mode));
            self.0.extend_from_slice(data);
            self.0.resize(self.0.len().div_ceil(BLOCK) * BLOCK, 0);
            self
        }
        fn dir(self, name: &str) -> Self {
            self.entry(name.as_bytes(), b'5', b"", 0o755)
        }
        fn file(self, name: &str, data: &[u8]) -> Self {
            self.entry(name.as_bytes(), b'0', data, 0o644)
        }
        fn end(mut self) -> Vec<u8> {
            self.0.extend_from_slice(&[0u8; 2 * BLOCK]);
            self.0
        }
    }

    fn pax_record(key: &str, value: &str) -> Vec<u8> {
        let body = format!(" {key}={value}\n");
        let mut n = body.len() + 1;
        while format!("{n}{body}").len() != n {
            n += 1;
        }
        format!("{n}{body}").into_bytes()
    }

    fn unpack_into(bytes: &[u8], limits: Limits) -> (TestDir, Result<Taken, String>) {
        let d = TestDir::new("plugin-out");
        let got = read(bytes, &d.join("view"), "out", limits);
        (d, got)
    }

    fn refused(bytes: &[u8]) -> String {
        let (d, got) = unpack_into(bytes, ALL);
        assert!(!d.join("view").exists(), "the target stays after a refusal");
        got.unwrap_err()
    }

    #[test]
    fn files_and_directories_are_written_under_the_target() {
        let tar = Tar::default()
            .dir("out/")
            .file("out/index.html", b"<p>hi</p>")
            .dir("out/a/")
            .file("out/a/b.json", &[7u8; 1500])
            .file("out/c/d.txt", b"made its directory")
            .end();
        let (d, got) = unpack_into(&tar, ALL);
        assert_eq!(
            got,
            Ok(Taken {
                files: 3,
                dirs: 2,
                bytes: 9 + 1500 + 18
            })
        );
        let view = d.join("view");
        assert_eq!(fs::read(view.join("index.html")).unwrap(), b"<p>hi</p>");
        assert_eq!(fs::read(view.join("a/b.json")).unwrap(), vec![7u8; 1500]);
        assert_eq!(
            fs::read(view.join("c/d.txt")).unwrap(),
            b"made its directory"
        );
    }

    #[test]
    fn a_target_that_exists_is_not_written() {
        let d = TestDir::new("plugin-out");
        fs::create_dir(d.join("view")).unwrap();
        let tar = Tar::default().dir("out/").end();
        let got = read(&tar[..], &d.join("view"), "out", ALL);
        assert!(got.unwrap_err().starts_with("cannot create "));
    }

    #[test]
    fn each_kind_but_file_and_directory_is_refused() {
        for (flag, what) in [
            (b'1', "a hard link"),
            (b'2', "a symlink"),
            (b'3', "a character device"),
            (b'4', "a block device"),
            (b'6', "a fifo"),
            (b'7', "an entry of a kind this reader does not take"),
            (b'S', "an entry of a kind this reader does not take"),
        ] {
            let tar = Tar::default()
                .dir("out/")
                .entry(b"out/x", flag, b"", 0o644)
                .end();
            assert_eq!(refused(&tar), format!("out/x: {what}"), "{}", flag as char);
        }
    }

    #[test]
    fn a_socket_or_a_link_in_the_mode_is_refused() {
        for (mode, what) in [(0o140755, "a socket"), (0o120777, "a symlink")] {
            let tar = Tar::default().entry(b"out/s", b'0', b"", mode).end();
            assert_eq!(refused(&tar), format!("out/s: {what}"));
        }
        let tar = Tar::default().entry(b"out/d", b'5', b"", 0o100644).end();
        assert_eq!(
            refused(&tar),
            "out/d: an entry whose mode and type disagree"
        );
    }

    #[test]
    fn a_name_that_leaves_the_target_is_refused() {
        for (name, why) in [
            ("/etc/passwd", "/etc/passwd: an absolute name"),
            ("out/../x", "out/../x: a name with .."),
            ("out/a/../../x", "out/a/../../x: a name with .."),
            ("out//x", "out//x: a name with an empty part or ."),
            ("out/./x", "out/./x: a name with an empty part or ."),
            ("other/x", "other/x: a name outside out/"),
            ("out", "out: out itself as a file"),
        ] {
            let tar = Tar::default().file(name, b"x").end();
            assert_eq!(refused(&tar), why);
        }
    }

    #[test]
    fn a_name_not_utf8_or_with_a_control_character_is_refused() {
        let tar = Tar::default().entry(b"out/\xff", b'0', b"", 0o644).end();
        assert_eq!(refused(&tar), "a name that is not UTF-8");
        let tar = Tar::default().entry(b"out/a\nb", b'0', b"", 0o644).end();
        assert_eq!(refused(&tar), "out/a\\nb: a control character in a name");
        let tar = Tar::default().entry(b"out/\x1b[2J", b'0', b"", 0o644).end();
        assert!(refused(&tar).ends_with("a control character in a name"));
    }

    #[test]
    fn a_name_twice_is_refused() {
        let tar = Tar::default().file("out/a", b"1").file("out/a", b"2").end();
        assert!(refused(&tar).starts_with("out/a: cannot write it: "));
        let tar = Tar::default().file("out/a", b"1").dir("out/a/").end();
        assert!(refused(&tar).starts_with("a: cannot make it: "));
    }

    #[test]
    fn more_entries_or_bytes_than_the_limits_are_refused() {
        let tar = Tar::default()
            .dir("out/")
            .file("out/a", b"1")
            .file("out/b", b"2")
            .end();
        let few = Limits {
            entries: 2,
            bytes: BYTES,
        };
        let (_d, got) = unpack_into(&tar, few);
        assert_eq!(got.unwrap_err(), "more than 2 entries");
        let small = Limits {
            entries: ENTRIES,
            bytes: 1,
        };
        let (d, got) = unpack_into(&tar, small);
        assert_eq!(got.unwrap_err(), "more than 1 bytes");
        assert!(!d.join("view").exists());
        let exact = Limits {
            entries: 3,
            bytes: 2,
        };
        assert!(unpack_into(&tar, exact).1.is_ok());
    }

    #[test]
    fn the_limits_of_a_view_are_the_specs() {
        assert_eq!(ENTRIES, 5000);
        assert_eq!(BYTES, 256 * 1024 * 1024);
    }

    #[test]
    fn a_gnu_long_name_and_a_ustar_prefix_are_read() {
        let long = format!("out/{}/f", "d".repeat(120));
        let mut tar = Tar::default()
            .dir("out/")
            .entry(
                b"././@LongLink",
                b'L',
                format!("{long}\0").as_bytes(),
                0o644,
            )
            .file("out/truncated", b"long")
            .end();
        tar.truncate(tar.len() - 2 * BLOCK);
        let mut h = header(b"f2", b'0', 2, 0o644);
        h[345..350].copy_from_slice(b"out/p");
        h[148..156].copy_from_slice(b"        ");
        let sum: u32 = h.iter().map(|b| *b as u32).sum();
        h[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
        tar.extend_from_slice(&h);
        tar.extend_from_slice(b"ok");
        tar.resize(tar.len().div_ceil(BLOCK) * BLOCK, 0);
        tar.extend_from_slice(&[0u8; 2 * BLOCK]);
        let (d, got) = unpack_into(&tar, ALL);
        assert_eq!(got.unwrap().files, 2);
        let view = d.join("view");
        assert_eq!(fs::read(view.join(&long[4..])).unwrap(), b"long");
        assert_eq!(fs::read(view.join("p/f2")).unwrap(), b"ok");
        assert!(!view.join("truncated").exists());
    }

    #[test]
    fn a_pax_path_and_size_are_read_and_a_pax_link_is_refused() {
        let mut rec = pax_record("path", "out/é.txt");
        rec.extend(pax_record("mtime", "1700000000.5"));
        let tar = Tar::default()
            .entry(b"PaxHeaders/x", b'x', &rec, 0o644)
            .file("out/ignored", b"data")
            .end();
        let (d, got) = unpack_into(&tar, ALL);
        assert!(got.is_ok(), "{got:?}");
        assert_eq!(fs::read(d.join("view/é.txt")).unwrap(), b"data");

        let rec = pax_record("path", "../x");
        let tar = Tar::default()
            .entry(b"PaxHeaders/x", b'x', &rec, 0o644)
            .file("out/x", b"")
            .end();
        assert_eq!(refused(&tar), "../x: a name with ..");

        let rec = pax_record("linkpath", "/etc/passwd");
        let tar = Tar::default()
            .entry(b"PaxHeaders/x", b'x', &rec, 0o644)
            .file("out/x", b"")
            .end();
        assert_eq!(refused(&tar), "a link's name in a pax header");

        let tar = Tar::default()
            .entry(b"PaxHeaders/x", b'x', b"99 path=x\n", 0o644)
            .file("out/x", b"")
            .end();
        assert_eq!(refused(&tar), "a pax header that cannot be read");

        let tar = Tar::default().entry(b"g", b'g', b"", 0o644).end();
        assert_eq!(refused(&tar), "a global pax header");
    }

    #[test]
    fn a_stream_cut_short_or_damaged_is_refused() {
        let tar = Tar::default().file("out/a", &[1u8; 600]).end();
        assert!(refused(&tar[..BLOCK + 100]).starts_with("the stream ends inside an entry"));
        assert_eq!(refused(&tar[..BLOCK * 3]), "the stream ends before its end");
        let mut bad = tar.clone();
        bad[0] = b'x';
        assert_eq!(refused(&bad), "a header whose checksum is wrong");
        let tar = Tar::default().entry(b"L", b'L', b"out/a\0", 0o644).end();
        assert_eq!(refused(&tar), "the stream ends after a header of no entry");
        let tar = Tar::default()
            .entry(b"L", b'L', &vec![b'a'; (META_MAX + 1) as usize], 0o644)
            .end();
        assert!(refused(&tar).starts_with("a header of 65537 bytes"));
    }

    #[test]
    fn a_directory_with_data_is_refused() {
        let tar = Tar::default().entry(b"out/d/", b'5', b"xx", 0o755).end();
        assert_eq!(refused(&tar), "out/d/: a directory with data");
    }
}
