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
//!
//! [`try_lock`] is the other side: a holder that takes a lock now or not at
//! all, and creates its file to do so. What it holds is a [`Held`], which
//! gives the lock back by name when it drops.

use std::fs::{self, File, TryLockError};
use std::io::ErrorKind;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::Duration;

/// A lock this process holds, given back when the value drops.
///
/// An flock belongs to the open file description, and a child that any
/// thread of this process starts holds a copy of every descriptor until its
/// exec closes it. Closing the file alone can then leave the lock held for a
/// moment after the drop, which the next taker reads as a lock still held.
/// So the drop unlocks first, which frees the lock for every copy at once.
/// Two locks are handed down to children on purpose and must never be a
/// `Held`, whose drop would free them under the children that still run:
/// a cycle's lock and the code index's (`tehas-cycle`'s `lock::take` and
/// `lock::hand_down`), and the update's lock (`tehas-ops`'s `update::lock`,
/// `Held::inherit`).
#[derive(Debug)]
pub struct Held(File);

impl Held {
    /// The lock of `file`, taken now; else why not, with the file back to
    /// try again (`WouldBlock` when another holds it).
    pub fn try_take(file: File) -> Result<Held, (TryLockError, File)> {
        match file.try_lock() {
            Ok(()) => Ok(Held(file)),
            Err(e) => Err((e, file)),
        }
    }

    /// The lock of `file`, waited for.
    pub fn take(file: File) -> std::io::Result<Held> {
        file.lock()?;
        Ok(Held(file))
    }

    /// A shared lock of `file`, waited for.
    pub fn take_shared(file: File) -> std::io::Result<Held> {
        file.lock_shared()?;
        Ok(Held(file))
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

/// A lock taken now, or None when another holds it.
pub fn try_lock(path: &Path) -> Result<Option<Held>, String> {
    let mut f = File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    // A lock just given back by another process can still be held a moment
    // by a child it forked while it was taken, until that child's exec
    // closes it.
    for _ in 0..10 {
        match Held::try_take(f) {
            Ok(held) => return Ok(Some(held)),
            Err((TryLockError::WouldBlock, back)) => {
                f = back;
                std::thread::sleep(Duration::from_millis(50));
            }
            Err((TryLockError::Error(e), _)) => return Err(format!("{}: {e}", path.display())),
        }
    }
    Ok(None)
}

/// The names of the `*.lock` files in `dir` that are held, in the order of a
/// glob. One that cannot be read counts as held: nothing may guess that nobody
/// holds it.
pub fn held_in(dir: &Path) -> Vec<String> {
    let mut held: Vec<String> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let name = path
                .file_name()?
                .to_str()?
                .strip_suffix(".lock")?
                .to_string();
            (!name.is_empty() && !matches!(probe(&path), Probe::Free | Probe::Absent))
                .then_some(name)
        })
        .collect();
    held.sort();
    held
}

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
    use crate::testdir::TestDir;
    use std::io::BufRead;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    /// The reason of an unknown answer.
    fn reason(p: &Probe) -> String {
        match p {
            Probe::Unknown(why) => why.clone(),
            other => panic!("expected unknown, found {other:?}"),
        }
    }

    /// A file of the issue's own name, made empty.
    fn lock_file(d: &TestDir, n: u64) -> PathBuf {
        let p = d.join(format!("{n}.lock"));
        fs::write(&p, b"").unwrap();
        p
    }

    #[test]
    fn a_file_no_one_holds_is_free() {
        let d = TestDir::new("free");
        let p = lock_file(&d, 1);
        assert_eq!(probe(&p), Probe::Free);
        // The probe gave it back: the same file answers free again.
        assert_eq!(probe(&p), Probe::Free);
    }

    #[test]
    fn a_missing_file_is_absent() {
        let d = TestDir::new("absent");
        assert_eq!(probe(&d.join("nope.lock")), Probe::Absent);
        // Absent, though the directory of it is not there either.
        assert_eq!(probe(&d.join("gone").join("nope.lock")), Probe::Absent);
    }

    #[test]
    fn the_probe_never_makes_a_file() {
        let d = TestDir::new("creates");
        assert_eq!(probe(&d.join("7.lock")), Probe::Absent);
        assert!(!d.join("7.lock").exists());
        assert_eq!(fs::read_dir(d.path()).unwrap().count(), 0);
    }

    #[test]
    fn a_lock_held_elsewhere_is_not_taken_and_one_given_back_is() {
        let d = TestDir::new("lock-try");
        let f = d.join("x.lock");
        let held = try_lock(&f).unwrap().expect("a free lock is taken");
        assert_eq!(probe(&f), Probe::Held);
        assert!(try_lock(&f).unwrap().is_none());
        drop(held);
        assert!(try_lock(&f).unwrap().is_some());
        let missing = d.join("no/x.lock");
        assert!(try_lock(&missing)
            .unwrap_err()
            .starts_with(&missing.display().to_string()));
    }

    #[test]
    fn a_held_lock_is_free_at_once_while_other_threads_start_children() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        let d = TestDir::new("lock-held-drop");
        let f = d.join("x.lock");
        // Children started all the while: each holds a copy of every open
        // descriptor of this process until its exec. A drop that only closed
        // the file left the next take refused about one time in six.
        let stop = Arc::new(AtomicBool::new(false));
        let spawners: Vec<_> = (0..4)
            .map(|_| {
                let stop = Arc::clone(&stop);
                std::thread::spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        let _ = std::process::Command::new("true").status();
                    }
                })
            })
            .collect();
        let mut refused = 0;
        for _ in 0..2000 {
            match Held::try_take(File::create(&f).unwrap()) {
                Ok(held) => drop(held),
                Err(_) => refused += 1,
            }
        }
        stop.store(true, Ordering::Relaxed);
        for s in spawners {
            s.join().unwrap();
        }
        assert_eq!(refused, 0, "a dropped Held was still held");
    }

    #[test]
    fn a_lock_another_process_holds_is_held() {
        let d = TestDir::new("held");
        let p = lock_file(&d, 4);
        // The holder is a process of its own. A lock this process took would
        // belong to its open file description, which a child another test forks
        // in the meantime shares until it execs, so the lock could outlive its
        // drop here. The holder prints when it holds, waits for a line, and
        // gives the lock up when the pipe closes.
        let mut child = std::process::Command::new("flock")
            .arg("-x")
            .arg(&p)
            .arg("-c")
            .arg("echo held; read _")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut said = String::new();
        std::io::BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut said)
            .unwrap();
        assert_eq!(said.trim(), "held", "the holder never took the lock");
        // Contention is the one answer with no reason beside it: another
        // process holds the lock, which is what was asked.
        assert_eq!(probe(&p), Probe::Held);
        drop(child.stdin.take().unwrap());
        child.wait().unwrap();
        assert_eq!(probe(&p), Probe::Free);
    }

    #[test]
    fn a_lock_that_is_a_directory_is_unknown() {
        let d = TestDir::new("directory");
        let p = d.join("6.lock");
        fs::create_dir(&p).unwrap();
        assert_eq!(
            probe(&p),
            Probe::Unknown("6.lock is not a regular file".to_string())
        );
    }

    #[test]
    fn a_lock_that_is_a_link_to_a_free_file_is_unknown() {
        let d = TestDir::new("link");
        let target = d.join("elsewhere.lock");
        fs::write(&target, b"").unwrap();
        assert_eq!(probe(&target), Probe::Free);
        let p = d.join("8.lock");
        std::os::unix::fs::symlink(&target, &p).unwrap();
        assert_eq!(
            probe(&p),
            Probe::Unknown("8.lock is not a regular file".to_string())
        );
    }

    #[test]
    fn a_dangling_link_is_unknown() {
        let d = TestDir::new("dangling");
        let p = d.join("9.lock");
        std::os::unix::fs::symlink(d.join("nothing"), &p).unwrap();
        assert_eq!(
            probe(&p),
            Probe::Unknown("9.lock is not a regular file".to_string())
        );
    }

    #[test]
    fn a_lock_of_mode_000_is_unknown_when_the_mode_keeps_us_out() {
        let d = TestDir::new("mode");
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
        let d = TestDir::new("reason");
        let p = d.join("12.lock");
        fs::create_dir(&p).unwrap();
        assert!(reason(&probe(&p)).starts_with("12.lock "));
        std::os::unix::fs::symlink(d.join("nothing"), d.join("13.lock")).unwrap();
        assert!(reason(&probe(&d.join("13.lock"))).starts_with("13.lock "));
    }
}
