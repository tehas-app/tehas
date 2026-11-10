//! Files: a host file read by the store's rule, a database's side files, a
//! file's bytes as base64, and the one write through a temporary name.

use std::fs::{self, File, Metadata, OpenOptions, Permissions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Standard base64 with padding.
pub fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The 1 MiB bound of a read: a file, a stream or a line one program writes
/// and another reads. Both sides of a seam name this one constant, so a change
/// to it cannot leave a writer producing what its reader refuses. A bound that
/// is deliberately different keeps its own constant and says why.
pub const READ_BOUND: u64 = 1 << 20;
/// `READ_BOUND` for a bound counted in a `usize`.
pub const READ_BOUND_LEN: usize = READ_BOUND as usize;

#[derive(Debug)]
pub enum ReadError {
    /// It could not be looked at, opened or read.
    Io(io::Error),
    /// A link, a directory, a device or a fifo.
    NotRegular,
    /// Another file was in its place when it was opened.
    Replaced,
    /// Over the bound.
    TooLarge,
}

/// The bytes of a regular file of at most `bound` bytes, and the metadata of
/// the file that was read. It is not read through a link, and it is the same
/// file after it is opened.
pub fn read_regular(path: &Path, bound: u64) -> Result<(Vec<u8>, Metadata), ReadError> {
    let before = std::fs::symlink_metadata(path).map_err(ReadError::Io)?;
    if !before.file_type().is_file() {
        return Err(ReadError::NotRegular);
    }
    let f = File::open(path).map_err(ReadError::Io)?;
    let m = f.metadata().map_err(ReadError::Io)?;
    if m.dev() != before.dev() || m.ino() != before.ino() || !m.is_file() {
        return Err(ReadError::Replaced);
    }
    if m.len() > bound {
        return Err(ReadError::TooLarge);
    }
    let mut bytes = Vec::new();
    f.take(bound + 1)
        .read_to_end(&mut bytes)
        .map_err(ReadError::Io)?;
    if bytes.len() as u64 > bound {
        return Err(ReadError::TooLarge);
    }
    Ok((bytes, m))
}

/// How `write_atomic` names the file in progress. Every form asks for the
/// temporary file with `create_new`, so a writer never opens a file another
/// writer holds, and none of them is served: the edge serves no `*.new` path.
pub enum Temp {
    /// `<name>.new`, the same name every writer asks for. A leftover file of
    /// that name is removed first, so a crashed run does not block the next.
    Predictable,
    /// `<name>.<pid>.new`, so two writers of one file cannot meet in it.
    Pid,
    /// `<name>.<hex>.new`, a name no other process holds. Made by a
    /// `create_new` retry loop, for a writer that cannot remove a name it
    /// does not own.
    Unique,
}

/// The temporary file of `path` for `temp`, made with `create_new` at `mode`:
/// the name and the handle of it together, so the writer of the bytes holds
/// the file it names. An error says no name of this form could be opened.
fn temp_path(path: &Path, temp: &Temp, mode: u32) -> io::Result<(PathBuf, File)> {
    match temp {
        Temp::Predictable => {
            let tmp = predictable(path)?;
            Ok((tmp.clone(), open_temp(&tmp, mode)?))
        }
        Temp::Pid => {
            // The name `with_extension("<pid>.new")` gives: a name of this run
            // beside the file, ending in `.new`.
            let mut name = path.as_os_str().to_owned();
            if let Some(ext) = path.extension() {
                name.push(".");
                name.push(ext);
            }
            name.push(format!(".{}.new", std::process::id()));
            let tmp = take(PathBuf::from(name))?;
            Ok((tmp.clone(), open_temp(&tmp, mode)?))
        }
        Temp::Unique => {
            // The time, this process and a count of it: two writers of one
            // file at once ask for different names, and a name some earlier
            // run left beside the file cannot stop this one. Each try makes a
            // new name, so the retry answers a name that is truly in the way.
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let pid = std::process::id() as u64;
            for _ in 0..64u32 {
                let tick = COUNTER.fetch_add(1, Ordering::Relaxed);
                let nanos = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos() as u64);
                let seed = (pid << 48) ^ nanos.rotate_left(11) ^ (tick << 1);
                let last = temp_name(path, seed);
                // The same rule as the other two forms, asked of a name this
                // writer made for itself: a name it cannot take is one it does
                // not own, and another is asked for instead.
                if take(last.clone()).is_err() {
                    continue;
                }
                match open_temp(&last, mode) {
                    Ok(f) => return Ok((last, f)),
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(e) => return Err(e),
                }
            }
            Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "every temporary name beside it was taken",
            ))
        }
    }
}

/// A temporary name beside the file that is ours to hold: a leftover of a run
/// that died is taken away, so it cannot stop the next write. A name is ours
/// only when it holds a regular file of this account - a directory, or
/// something planted under the name, is not.
fn take(tmp: PathBuf) -> io::Result<PathBuf> {
    match fs::symlink_metadata(&tmp) {
        Ok(m) if !owned_metadata(&m) => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the temporary name is not ours to remove",
        )),
        Ok(_) => {
            fs::remove_file(&tmp)?;
            Ok(tmp)
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(tmp),
        Err(e) => Err(e),
    }
}

/// The predictable temporary name, `<name>.new`.
fn predictable(path: &Path) -> io::Result<PathBuf> {
    let mut name = path.as_os_str().to_owned();
    name.push(".new");
    take(PathBuf::from(name))
}

/// The account of this run: one that owns nothing where it cannot be had, the
/// rule of the flags' own `owned_regular`, so a name planted under one of them
/// is never taken away. `id -u`, as the console's account module asks: core
/// keeps no dependency for the number, and AGENTS.md allows no `unsafe`.
fn own_uid() -> Option<u32> {
    static UID: OnceLock<Option<u32>> = OnceLock::new();
    *UID.get_or_init(|| {
        let out = Command::new("id")
            .arg("-u")
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success())?;
        String::from_utf8_lossy(&out.stdout).trim_end().parse().ok()
    })
}

/// Whether the metadata is of a regular file of this account.
fn owned_metadata(m: &Metadata) -> bool {
    m.file_type().is_file() && own_uid().is_some_and(|me| m.uid() == me)
}

/// The temporary file made new, with `mode`: the one way a temporary of any
/// form is opened.
fn open_temp(tmp: &Path, mode: u32) -> io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(tmp)
}

/// `<name>.<hex>.new`: the name of one try at a temporary file.
fn temp_name(path: &Path, seed: u64) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".{seed:x}.new"));
    PathBuf::from(name)
}

/// Write `bytes` to `path` as `mode`, through a temporary file named by
/// `temp` and a rename, so a reader of `path` never sees half of it. One
/// rule: write, flush, `sync_all` the temporary file, set `mode` again (the
/// umask may widen what `create_new` made), rename, then `sync_all` the
/// parent directory, best-effort. Whatever fails, the temporary file is
/// removed and the error returned.
pub fn write_atomic(path: &Path, bytes: &[u8], mode: u32, temp: &Temp) -> io::Result<()> {
    let (tmp, mut f) = temp_path(path, temp, mode)?;
    let mut done = f
        .write_all(bytes)
        .and_then(|()| f.flush())
        .and_then(|()| f.sync_all());
    // A file made by create_new has the mode asked for masked by the umask,
    // which a service may widen.
    done = done.and(fs::set_permissions(&tmp, Permissions::from_mode(mode)));
    done = done.and_then(|()| fs::rename(&tmp, path));
    if let Err(e) = done {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    // The directory holds the name the reader opens.
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        if let Ok(handle) = File::open(dir) {
            let _ = handle.sync_all();
        }
    }
    Ok(())
}

// SQLite names a database's side files by appending to its name.
pub fn side_file(file: &Path, side: &str) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(side);
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdir::TestDir;

    #[test]
    fn a_regular_file_is_read_whole() {
        let d = TestDir::new("whole");
        let p = d.join("f");
        std::fs::write(&p, "four").unwrap();
        let (bytes, m) = read_regular(&p, 4).unwrap();
        assert_eq!(bytes, b"four");
        assert_eq!(m.len(), 4);
    }

    #[test]
    fn a_file_over_the_bound_is_refused() {
        let d = TestDir::new("bound");
        let p = d.join("f");
        std::fs::write(&p, "five!").unwrap();
        assert!(matches!(read_regular(&p, 4), Err(ReadError::TooLarge)));
    }

    #[test]
    fn a_link_a_directory_and_a_missing_file_are_refused() {
        let d = TestDir::new("refused");
        let p = d.join("f");
        std::fs::write(&p, "x").unwrap();
        let link = d.join("link");
        std::os::unix::fs::symlink(&p, &link).unwrap();
        assert!(matches!(read_regular(&link, 9), Err(ReadError::NotRegular)));
        assert!(matches!(
            read_regular(d.path(), 9),
            Err(ReadError::NotRegular)
        ));
        assert!(matches!(
            read_regular(&d.join("gone"), 9),
            Err(ReadError::Io(e)) if e.kind() == io::ErrorKind::NotFound
        ));
    }

    #[test]
    fn a_side_file_is_named_after_its_database() {
        let f = side_file(Path::new("/x/state/tehas.db"), "-wal");
        assert_eq!(f, Path::new("/x/state/tehas.db-wal"));
    }

    #[test]
    fn the_read_bound_is_one_mib_in_both_counts() {
        assert_eq!(READ_BOUND, 1_048_576);
        assert_eq!(READ_BOUND_LEN as u64, READ_BOUND);
    }

    #[test]
    fn two_writers_of_one_name_leave_a_whole_file_each_time() {
        // The point of the temporary name: a reader of the file never sees the
        // half of one text with the tail of another.
        use std::thread;
        const RUNS: usize = 100;
        let d = TestDir::new("two-writers");
        let p = d.join("both");
        let (a, b) = ("a".repeat(4096), "b".repeat(4096));
        let (ta, tb) = (
            thread::spawn({
                let p = p.clone();
                let a = a.clone();
                move || {
                    for _ in 0..RUNS {
                        write_atomic(&p, a.as_bytes(), 0o600, &Temp::Unique).unwrap();
                    }
                }
            }),
            thread::spawn({
                let p = p.clone();
                let b = b.clone();
                move || {
                    for _ in 0..RUNS {
                        write_atomic(&p, b.as_bytes(), 0o600, &Temp::Unique).unwrap();
                    }
                }
            }),
        );
        ta.join().unwrap();
        tb.join().unwrap();
        for _ in 0..RUNS {
            let text = fs::read_to_string(&p).unwrap();
            assert!(text == a || text == b, "a mixed file was read");
        }
        let left: Vec<_> = fs::read_dir(d.path())
            .unwrap()
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|n| n.ends_with(".new"))
            .collect();
        assert!(left.is_empty(), "{left:?}");
    }

    #[test]
    fn each_form_names_its_temporary_and_gives_it_its_mode() {
        let d = TestDir::new("forms");
        let pid = std::process::id();
        for (i, (temp, want)) in [
            (Temp::Predictable, "f.new".to_string()),
            (Temp::Pid, format!("f.{pid}.new")),
        ]
        .iter()
        .enumerate()
        {
            let p = d.join("f");
            let hold = d.join(want);
            // The temporary name is the one above: write through it, and the
            // file gets `mode`. Both forms take a leftover of their own name
            // first, so a run that died mid-write cannot stop the next one.
            write_atomic(&p, b"x", 0o640, temp).unwrap();
            assert_eq!(
                fs::metadata(&p).unwrap().permissions().mode() & 0o777,
                0o640
            );
            fs::remove_file(&p).unwrap();
            fs::write(&hold, "a leftover of a run that died").unwrap();
            write_atomic(&p, b"whole", 0o640, temp).unwrap();
            assert_eq!(fs::read_to_string(&p).unwrap(), "whole");
            assert!(!hold.exists(), "{hold:?} was left behind");
            // A file under the temporary name that is not ours - here, a link
            // planted under it, which the write would follow - holds the write
            // off, and stays where it was left. A name is ours to remove only
            // when it holds a regular file of this account: a leftover of a run
            // that died, yes; a name someone planted under it, no.
            let keep = |name: &Path| {
                let e = write_atomic(&p, b"x", 0o640, temp).unwrap_err();
                assert_eq!(e.kind(), io::ErrorKind::PermissionDenied, "{e}");
                assert!(name.symlink_metadata().is_ok(), "{name:?} was taken away");
            };
            // A directory under the name is not a leftover of a run that
            // died: neither form removes it, and the write fails.
            fs::create_dir_all(&hold).unwrap();
            keep(&hold);
            fs::remove_dir_all(&hold).unwrap();
            let outside = d.join(format!("victim-{i}"));
            fs::write(&outside, "not ours").unwrap();
            std::os::unix::fs::symlink(&outside, &hold).unwrap();
            keep(&hold);
            assert_eq!(fs::read_to_string(&outside).unwrap(), "not ours");
            fs::remove_file(&hold).unwrap();
            // A directory under the target name cannot be written over, and the
            // file made beside it goes with the attempt.
            fs::remove_file(&p).unwrap();
            fs::create_dir(&p).unwrap();
            let e = write_atomic(&p, b"x", 0o640, temp).unwrap_err();
            assert!(
                matches!(
                    e.kind(),
                    io::ErrorKind::PermissionDenied | io::ErrorKind::IsADirectory
                ),
                "{e}"
            );
            // The file made beside it is gone with the attempt.
            let left: Vec<_> = fs::read_dir(d.path())
                .unwrap()
                .filter_map(|e| e.ok()?.file_name().into_string().ok())
                .filter(|n| n.starts_with("f.") && n.ends_with(".new"))
                .collect();
            assert!(left.is_empty(), "{left:?}");
            fs::remove_dir(&p).unwrap();
            write_atomic(&p, b"x", 0o640, temp).unwrap();
            assert_eq!(
                fs::metadata(&p).unwrap().permissions().mode() & 0o777,
                0o640
            );
        }
        // The unique name is `<name>.<hex>.new`, made by create_new.
        let p = d.join("u");
        let (tmp, f) = temp_path(&p, &Temp::Unique, 0o600).unwrap();
        let hex = tmp
            .file_name()
            .unwrap()
            .to_string_lossy()
            .trim_start_matches("u.")
            .trim_end_matches(".new")
            .to_string();
        assert!(!hex.is_empty(), "{hex}");
        assert!(hex.bytes().all(|c| c.is_ascii_hexdigit()), "{hex}");
        // While its writer holds the file it is there, whole and of `mode`.
        assert!(tmp.is_file());
        drop(f);
        let _ = fs::remove_file(&tmp);
        write_atomic(&p, b"x", 0o600, &Temp::Unique).unwrap();
        let left: Vec<_> = fs::read_dir(d.path())
            .unwrap()
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|n| n.starts_with("u.") && n.ends_with(".new"))
            .collect();
        assert!(left.is_empty(), "{left:?}");
        assert_eq!(
            fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn the_predictable_form_replaces_a_leftover_and_refuses_a_directory() {
        let d = TestDir::new("predictable");
        let p = d.join("f");
        let tmp = d.join("f.new");
        // A whole file left under the temporary name by a run that died is
        // removed first, so the next write is this one's bytes.
        fs::write(&tmp, "half a file from a run that did not finish").unwrap();
        write_atomic(&p, b"new", 0o640, &Temp::Predictable).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "new");
        assert_eq!(
            fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert!(!tmp.exists());
        // A directory under the temporary name is not ours to remove.
        fs::create_dir(&tmp).unwrap();
        assert!(write_atomic(&p, b"new", 0o600, &Temp::Predictable).is_err());
        assert!(tmp.is_dir());
        fs::remove_dir(&tmp).unwrap();
    }

    #[test]
    fn a_file_of_the_read_bound_is_read_and_one_byte_more_is_not() {
        let d = TestDir::new("read-bound");
        let p = d.join("f");
        std::fs::write(&p, vec![b'x'; READ_BOUND_LEN]).unwrap();
        assert_eq!(
            read_regular(&p, READ_BOUND).unwrap().0.len(),
            READ_BOUND_LEN
        );
        std::fs::write(&p, vec![b'x'; READ_BOUND_LEN + 1]).unwrap();
        assert!(matches!(
            read_regular(&p, READ_BOUND),
            Err(ReadError::TooLarge)
        ));
    }
}
