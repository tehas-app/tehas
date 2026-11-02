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
        let _ = fs::remove_dir_all(&self.path);
        // Fails while another test's directory is still there.
        let _ = fs::remove_dir(root());
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
