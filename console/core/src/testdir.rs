//! The scratch directory of a test. Every test of the workspace makes its
//! directories here: under one root per test process,
//! `$TMPDIR/tehas-cargo-<pid>/`, so a killed run leaves one directory. The
//! guard removes its directory when it drops, which is also when its test
//! panics, and the root with the last one.

use std::fs;
use std::io;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// Held while the root is changed: a directory dropped, or a program shared.
pub(crate) static ROOT: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Where `testing::link_exe` keeps the one copy of each program it shares.
pub(crate) const SHARED: &str = "shared-exe";

/// The root of this process's test directories.
pub fn root() -> PathBuf {
    std::env::temp_dir().join(format!("tehas-cargo-{}", std::process::id()))
}

/// A directory that exists until the value drops.
#[derive(Debug)]
pub struct TestDir {
    path: PathBuf,
}

impl TestDir {
    /// A new, empty directory whose name starts with `name`.
    pub fn new(name: &str) -> TestDir {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = root().join(format!("{name}-{n}"));
        let _held = ROOT.lock().unwrap_or_else(|e| e.into_inner());
        // Another guard may remove the root between the two creates.
        let mut tries = 0;
        loop {
            match fs::create_dir_all(&path) {
                Ok(()) => break,
                Err(e) if tries < 10 && e.kind() == io::ErrorKind::NotFound => tries += 1,
                Err(e) => panic!("cannot create {}: {e}", path.display()),
            }
        }
        TestDir { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// A path in the directory.
    pub fn join(&self, p: impl AsRef<Path>) -> PathBuf {
        self.path.join(p)
    }
}

impl Deref for TestDir {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for TestDir {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _held = ROOT.lock().unwrap_or_else(|e| e.into_inner());
        let _ = fs::remove_dir_all(&self.path);
        // The last directory takes the shared programs with it; while another
        // test's directory is still there the root stays.
        let only_shared = fs::read_dir(root())
            .is_ok_and(|mut d| d.all(|e| e.is_ok_and(|e| e.file_name() == SHARED)));
        if only_shared {
            let _ = fs::remove_dir_all(root());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_removed_on_drop_and_on_panic() {
        let d = TestDir::new("drop");
        fs::write(d.join("f"), "x").unwrap();
        let p = d.path().to_path_buf();
        assert!(p.starts_with(root()));
        drop(d);
        assert!(!p.exists());

        let held = std::thread::spawn(|| {
            let d = TestDir::new("panic");
            let p = d.path().to_path_buf();
            std::panic::catch_unwind(move || {
                let _d = d;
                panic!("the case fails");
            })
            .unwrap_err();
            p
        })
        .join()
        .unwrap();
        assert!(!held.exists());
    }

    #[test]
    fn names_are_unique() {
        let a = TestDir::new("same");
        let b = TestDir::new("same");
        assert_ne!(a.path(), b.path());
    }
}
