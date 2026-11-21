//! Executables a test writes and then runs. Cargo runs a crate's tests as
//! threads of one process, and a fork in any thread copies every open
//! descriptor of the process. A script the test process holds open for writing
//! can be copied that way into a child that holds it until its own exec, and
//! the kernel refuses to exec a file open for writing: ETXTBSY. So the test
//! process never opens the executable itself. It writes the body to a file
//! beside it that is never run, and `install` makes the executable in a child
//! of its own, which no thread of the test process forks from.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// Make `path` an executable (mode 0755) that holds `body`.
pub fn write_exe(path: impl AsRef<Path>, body: impl AsRef<[u8]>) -> io::Result<()> {
    let path = path.as_ref();
    let body_file = beside(path);
    fs::write(&body_file, body)?;
    let made = install(&body_file, path);
    let _ = fs::remove_file(&body_file);
    made
}

/// Make `to` an executable copy of the program `from`.
pub fn copy_exe(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    install(from.as_ref(), to.as_ref())
}

/// Make `to` the program `from` as a hard link of one copy this process
/// shares, kept under the test directories' root. A test host with many tests
/// at once holds each program once, not once per test, so a memory ceiling
/// that counts written files does not fill. Where no link can be made it is a
/// copy. A rewrite of `to` replaces it and leaves the shared copy as it was:
/// `install` unlinks the file it writes over.
pub fn link_exe(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    let (from, to) = (from.as_ref(), to.as_ref());
    let _held = crate::testdir::ROOT
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dir = crate::testdir::root().join(crate::testdir::SHARED);
    let key = from.to_string_lossy().replace('/', "%");
    let shared = dir.join(key);
    if !shared.is_file() {
        fs::create_dir_all(&dir)?;
        install(from, &shared)?;
    }
    let _ = fs::remove_file(to);
    fs::hard_link(&shared, to).or_else(|_| install(from, to))
}

fn beside(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map_or_else(Default::default, |n| n.to_string_lossy());
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!(".{name}.{}-{n}.body", std::process::id()))
}

fn install(from: &Path, to: &Path) -> io::Result<()> {
    let out = Command::new("install")
        .arg("-m")
        .arg("0755")
        .arg(from)
        .arg(to)
        .output()?;
    if out.status.success() {
        return Ok(());
    }
    Err(io::Error::other(format!(
        "install {} {}: {}",
        from.display(),
        to.display(),
        String::from_utf8_lossy(&out.stderr).trim()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdir::TestDir;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    #[test]
    fn a_written_exe_runs_while_other_threads_fork() {
        let d = TestDir::new("write-exe");
        let stop = Arc::new(AtomicBool::new(false));
        let forks: Vec<_> = (0..32)
            .map(|_| {
                let stop = Arc::clone(&stop);
                std::thread::spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        let _ = Command::new("true").status();
                    }
                })
            })
            .collect();
        let mut failed = Vec::new();
        for i in 0..200 {
            let p = d.join(format!("s{i}"));
            write_exe(&p, format!("#!/bin/sh\nexit {}\n", i % 7)).unwrap();
            match Command::new(&p).status() {
                Ok(s) if s.code() == Some(i % 7) => {}
                other => failed.push(format!("{i}: {other:?}")),
            }
        }
        stop.store(true, Ordering::Relaxed);
        for f in forks {
            f.join().unwrap();
        }
        assert!(failed.is_empty(), "runs that failed: {failed:?}");
        let left: Vec<_> = fs::read_dir(d.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".body"))
            .collect();
        assert!(left.is_empty(), "body files left: {left:?}");
    }

    #[test]
    fn a_copied_exe_runs_and_a_rewrite_replaces_it() {
        let d = TestDir::new("copy-exe");
        let a = d.join("a");
        write_exe(&a, "#!/bin/sh\necho one\n").unwrap();
        let b = d.join("b");
        copy_exe(&a, &b).unwrap();
        let out = Command::new(&b).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), "one\n");
        write_exe(&b, "#!/bin/sh\necho two\n").unwrap();
        let out = Command::new(&b).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), "two\n");
    }

    #[test]
    fn a_linked_exe_shares_one_copy_and_a_rewrite_leaves_it() {
        let d = TestDir::new("link-exe");
        let src = d.join("src");
        write_exe(&src, "#!/bin/sh\necho one\n").unwrap();
        let (a, b) = (d.join("a"), d.join("b"));
        link_exe(&src, &a).unwrap();
        link_exe(&src, &b).unwrap();
        use std::os::unix::fs::MetadataExt;
        assert_eq!(
            fs::metadata(&a).unwrap().ino(),
            fs::metadata(&b).unwrap().ino()
        );
        assert!(fs::metadata(&a).unwrap().nlink() >= 3);
        write_exe(&b, "#!/bin/sh\necho two\n").unwrap();
        let run = |p: &Path| {
            String::from_utf8_lossy(&Command::new(p).output().unwrap().stdout).into_owned()
        };
        assert_eq!(run(&a), "one\n");
        assert_eq!(run(&b), "two\n");
    }

    #[test]
    fn a_missing_source_is_an_error_with_the_reason() {
        let d = TestDir::new("copy-exe-missing");
        let e = copy_exe(d.join("none"), d.join("b")).unwrap_err();
        assert!(e.to_string().contains("install"), "{e}");
        assert!(!d.join("b").exists());
    }
}
