//! One probe answers for every reader of a lock file: the store's cycle probe,
//! the dispatcher's snapshot, the health page and the cleaner. A lock is what
//! `flock -n` is asked about, so a reader that kept rules of its own would
//! disagree with the others about the same file, and each would fail closed on
//! a case the next one calls free.
//!
//! The probe never creates the file and never reads or writes its contents. It
//! answers four things, and a reader maps them to what it does: [`Probe::Absent`]
//! for no file, [`Probe::Free`] for one it took and gave back, [`Probe::Held`]
//! for one another process holds, and [`Probe::Unknown`] for the cases no one
//! may read as either, each with the reason a log carries. A link, a directory
//! or any other name that is no regular file is `Unknown`, as is a file that
//! cannot be opened, one replaced while it was opened, and a lock error other
//! than contention: a lock is taken and released by descriptor, so a probe of a
//! name that is not the file that was locked proves nothing either way.

use std::fs::{self, File, TryLockError};
use std::io::ErrorKind;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

/// What one probe of one path found.
#[derive(Debug, PartialEq, Eq)]
pub enum Probe {
    /// No file at that path.
    Absent,
    /// A regular file no one holds: the probe took it and gave it back.
    Free,
    /// Another process holds it.
    Held,
    /// A name that is no regular file, a file that could not be opened or
    /// probed, or one replaced while it was opened. The string is the reason.
    Unknown(String),
}

/// Steps 2 to 4 of the probe of one lock file, by its path: [`Probe::Absent`]
/// when there is no such file. The cycle lock and the planning turn's lock
/// (`state/plan-<n>.json.lock`) are probed the same way.
pub fn probe(file: &Path) -> Probe {
    let name = file
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let before = match fs::symlink_metadata(file) {
        Ok(m) if m.file_type().is_file() => m,
        Ok(_) => return Probe::Unknown(format!("{name} is not a regular file")),
        Err(e) if e.kind() == ErrorKind::NotFound => return Probe::Absent,
        Err(e) => return Probe::Unknown(format!("{name}: {e}")),
    };
    let f = match File::open(file) {
        Ok(f) => f,
        Err(e) => return Probe::Unknown(format!("{name}: {e}")),
    };
    match f.metadata() {
        Ok(m) if m.dev() == before.dev() && m.ino() == before.ino() => {}
        Ok(_) => return Probe::Unknown(format!("{name} was replaced")),
        Err(e) => return Probe::Unknown(format!("{name}: {e}")),
    }
    match f.try_lock() {
        Ok(()) => {
            let _ = f.unlock();
            Probe::Free
        }
        Err(TryLockError::WouldBlock) => Probe::Held,
        Err(TryLockError::Error(e)) => Probe::Unknown(format!("{name}: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    struct Dir(PathBuf);

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn dir(name: &str) -> Dir {
        let d = std::env::temp_dir().join(format!("tehas-core-lock-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        Dir(d)
    }

    /// The reason of an unknown answer.
    fn reason(p: &Probe) -> String {
        match p {
            Probe::Unknown(why) => why.clone(),
            other => panic!("expected unknown, found {other:?}"),
        }
    }

    /// A file of the issue's own name, made empty.
    fn lock_file(d: &Dir, n: u64) -> PathBuf {
        let p = d.0.join(format!("{n}.lock"));
        fs::write(&p, b"").unwrap();
        p
    }

    #[test]
    fn a_file_no_one_holds_is_free() {
        let d = dir("free");
        let p = lock_file(&d, 1);
        assert_eq!(probe(&p), Probe::Free);
        // The probe gave it back: the same file answers free again.
        assert_eq!(probe(&p), Probe::Free);
    }

    #[test]
    fn a_missing_file_is_absent() {
        let d = dir("absent");
        assert_eq!(probe(&d.0.join("nope.lock")), Probe::Absent);
        // Absent, though the directory of it is not there either.
        assert_eq!(probe(&d.0.join("gone").join("nope.lock")), Probe::Absent);
    }

    #[test]
    fn the_probe_never_makes_a_file() {
        let d = dir("creates");
        assert_eq!(probe(&d.0.join("7.lock")), Probe::Absent);
        assert!(!d.0.join("7.lock").exists());
        assert_eq!(fs::read_dir(&d.0).unwrap().count(), 0);
    }

    #[test]
    fn a_lock_another_process_holds_is_held() {
        let d = dir("held");
        let p = lock_file(&d, 4);
        let f = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&p)
            .unwrap();
        f.lock().unwrap();
        // Contention is the one answer with no reason beside it: another
        // process holds the lock, which is what was asked.
        assert_eq!(probe(&p), Probe::Held);
        drop(f);
        assert_eq!(probe(&p), Probe::Free);
    }

    #[test]
    fn a_lock_that_is_a_directory_is_unknown() {
        let d = dir("directory");
        let p = d.0.join("6.lock");
        fs::create_dir(&p).unwrap();
        assert_eq!(
            probe(&p),
            Probe::Unknown("6.lock is not a regular file".to_string())
        );
    }

    #[test]
    fn a_lock_that_is_a_link_to_a_free_file_is_unknown() {
        let d = dir("link");
        let target = d.0.join("elsewhere.lock");
        fs::write(&target, b"").unwrap();
        assert_eq!(probe(&target), Probe::Free);
        let p = d.0.join("8.lock");
        std::os::unix::fs::symlink(&target, &p).unwrap();
        assert_eq!(
            probe(&p),
            Probe::Unknown("8.lock is not a regular file".to_string())
        );
    }

    #[test]
    fn a_dangling_link_is_unknown() {
        let d = dir("dangling");
        let p = d.0.join("9.lock");
        std::os::unix::fs::symlink(d.0.join("nothing"), &p).unwrap();
        assert_eq!(
            probe(&p),
            Probe::Unknown("9.lock is not a regular file".to_string())
        );
    }

    #[test]
    fn a_lock_of_mode_000_is_unknown_when_the_mode_keeps_us_out() {
        let d = dir("mode");
        let p = lock_file(&d, 11);
        fs::set_permissions(&p, fs::Permissions::from_mode(0o000)).unwrap();
        if File::open(&p).is_ok() {
            // Root, or a mode that binds nobody: the case cannot be made here.
            fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
            return;
        }
        let Probe::Unknown(got) = probe(&p) else {
            panic!("a lock no one can open is not free, held or absent");
        };
        fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(got.starts_with("11.lock: "), "{got}");
    }

    #[test]
    fn each_reason_names_the_lock() {
        let d = dir("reason");
        let p = d.0.join("12.lock");
        fs::create_dir(&p).unwrap();
        assert!(reason(&probe(&p)).starts_with("12.lock "));
        std::os::unix::fs::symlink(d.0.join("nothing"), d.0.join("13.lock")).unwrap();
        assert!(reason(&probe(&d.0.join("13.lock"))).starts_with("13.lock "));
    }
}
