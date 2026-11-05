//! The locks of a cycle, of tehas work and of the code index, taken where the
//! command runs and handed down to every program it starts.
//!
//! A cycle holds state/cycles/<issue>.lock and the index state/index.lock for as
//! long as anything they started runs: the dispatcher and the store count a held
//! lock as a running cycle, and a docker client that outlives its command still
//! writes the workspace or the index. Safe Rust opens every file close-on-exec,
//! so [`hand_down`] clears that flag with rustix (#1587), and each child inherits
//! the descriptor that holds the lock.
//!
//! A lock is flock(2), as `flock -n` and core's probe ask it. tehas work run by
//! tehasbot cycle finds the issue's lock held by its parent on a descriptor it
//! inherited: [`held_here`] says so, from /proc/self/fdinfo, which lists the
//! locks of each open file description.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

/// Open `path`, made if it is not there, and take its lock, trying until `wait`
/// has passed. `Ok(None)` when another holds it then; `Err` when the file cannot
/// be opened.
pub fn take(path: &Path, wait: Duration) -> Result<Option<File>, String> {
    let f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let until = Instant::now() + wait;
    loop {
        match f.try_lock() {
            Ok(()) => return Ok(Some(f)),
            // Any refusal is a lock not taken, as flock -n reads it.
            Err(TryLockError::WouldBlock | TryLockError::Error(_)) => {}
        }
        if Instant::now() >= until {
            return Ok(None);
        }
        sleep(Duration::from_millis(100));
    }
}

/// Every program this process starts from now on inherits `f`.
pub fn hand_down(f: &File) -> Result<(), String> {
    rustix::io::fcntl_setfd(f, rustix::io::FdFlags::empty())
        .map_err(|e| format!("the lock cannot be handed down: {e}"))
}

/// Whether a descriptor of this process holds the lock of `path`: one this
/// process inherited, as tehas work does from tehasbot cycle.
pub fn held_here(path: &Path) -> bool {
    held_in(path, Path::new("/proc/self"))
}

fn held_in(path: &Path, proc: &Path) -> bool {
    let Ok(want) = fs::metadata(path) else {
        return false;
    };
    let Ok(fds) = fs::read_dir(proc.join("fd")) else {
        return false;
    };
    fds.flatten().any(|fd| {
        let same = fs::metadata(fd.path())
            .is_ok_and(|m| m.is_file() && m.dev() == want.dev() && m.ino() == want.ino());
        same && fs::read_to_string(proc.join("fdinfo").join(fd.file_name())).is_ok_and(|info| {
            info.lines()
                .any(|l| l.starts_with("lock:") && l.contains(" FLOCK "))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tehas_core::testdir::TestDir;

    #[test]
    fn a_free_lock_is_taken_and_made_when_missing() {
        let dir = TestDir::new("lock-free");
        let path = dir.join("7.lock");
        let f = take(&path, Duration::ZERO).unwrap();
        assert!(f.is_some());
        assert!(path.is_file());
        assert!(held_here(&path));
    }

    #[test]
    fn a_held_lock_is_refused_after_the_wait() {
        let dir = TestDir::new("lock-held");
        let path = dir.join("7.lock");
        let _mine = take(&path, Duration::ZERO).unwrap().unwrap();
        let began = Instant::now();
        assert!(take(&path, Duration::ZERO).unwrap().is_none());
        assert!(take(&path, Duration::from_millis(300)).unwrap().is_none());
        assert!(began.elapsed() >= Duration::from_millis(300));
    }

    #[test]
    fn a_lock_let_go_during_the_wait_is_taken() {
        let dir = TestDir::new("lock-later");
        let path = dir.join("7.lock");
        let mine = take(&path, Duration::ZERO).unwrap().unwrap();
        let t = std::thread::spawn(move || {
            sleep(Duration::from_millis(200));
            drop(mine);
        });
        assert!(take(&path, Duration::from_secs(5)).unwrap().is_some());
        t.join().unwrap();
    }

    #[test]
    fn a_file_that_cannot_be_opened_is_an_error() {
        let dir = TestDir::new("lock-error");
        assert!(take(&dir.join("no/such/7.lock"), Duration::ZERO).is_err());
    }

    #[test]
    fn a_descriptor_open_on_the_file_without_its_lock_is_not_held() {
        let dir = TestDir::new("lock-open");
        let path = dir.join("7.lock");
        let _open = File::create(&path).unwrap();
        assert!(!held_here(&path));
        assert!(!held_here(&dir.join("8.lock")));
        let other = dir.join("8.lock");
        let _theirs = take(&other, Duration::ZERO).unwrap().unwrap();
        assert!(!held_here(&path));
    }

    // A child that the descriptor reached finds the lock held there, and
    // `flock -n` on the path is refused while the child lives.
    #[test]
    fn a_handed_down_lock_reaches_a_child_and_one_kept_back_does_not() {
        let dir = TestDir::new("lock-child");
        let path = dir.join("7.lock");
        let ask = |path: &Path| {
            Command::new("bash")
                .args(["-c", r#"for f in /proc/$$/fd/*; do [ "$f" -ef "$0" ] && grep -q FLOCK "/proc/$$/fdinfo/${f##*/}" && exit 0; done; exit 1"#])
                .arg(path)
                .status()
                .unwrap()
                .success()
        };
        let f = take(&path, Duration::ZERO).unwrap().unwrap();
        assert!(!ask(&path));
        hand_down(&f).unwrap();
        assert!(ask(&path));
        drop(f);
        assert!(!ask(&path));
    }
}
