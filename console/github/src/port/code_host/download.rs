//! `CodeHost::download`: one release file, by gh's `release download` into
//! a directory of the adapter's own, then into the caller's writer.
//!
//! gh's runner gives text, and a release file is bytes, so gh writes the
//! file and the adapter reads it back. The release is read first: a file the
//! release does not list is `NotFound`, and one it lists as larger than the
//! limit is `Unread(TooLarge)`, both before any download. The file written is
//! held to the size the release lists and to the limit again. gh keeps the
//! credential; nothing of it is in an argument. The directory is removed
//! whatever happens.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::GitHub;
use tehas_ports::{Did, FileName, Outcome, TagName, Unread};

/// Directories made by this process, so two downloads never share one.
static MADE: AtomicU64 = AtomicU64::new(0);

/// A directory of the adapter's own under the system's temporary directory,
/// removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Option<Scratch> {
        let n = MADE.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("tehas-github-download-{}-{n}", std::process::id()));
        fs::create_dir(&dir).ok()?;
        Some(Scratch(dir))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A file name as gh's pattern takes it whole: its glob characters
/// escaped, so it names that one file.
fn pattern(name: &str) -> String {
    name.chars()
        .flat_map(|c| match c {
            '*' | '?' | '[' | ']' => vec!['\\', c],
            c => vec![c],
        })
        .collect()
}

impl GitHub {
    /// See the module.
    pub(super) fn download_to(
        &mut self,
        tag: &TagName,
        file: &FileName,
        limit: u64,
        into: &mut dyn Write,
    ) -> Did {
        let release = self.release_of(tag)?;
        let listed = release
            .files
            .iter()
            .find(|f| f.name == *file)
            .ok_or(Outcome::NotFound)?
            .size;
        if listed > limit {
            return Err(Outcome::Unread(Unread::TooLarge));
        }
        let scratch = Scratch::new().ok_or(Outcome::Unread(Unread::Unknown))?;
        let out = scratch.0.join("file");
        let (repo, pat, output) = (
            format!("--repo={}", self.repo),
            format!("--pattern={}", pattern(file.as_str())),
            format!("--output={}", out.display()),
        );
        let args = [
            "release",
            "download",
            &repo,
            &pat,
            &output,
            "--",
            tag.as_str(),
        ];
        self.call("the release file", &args)
            .map_err(|e| self.outcome(&e))?;
        copy(&out, listed, limit, into)
    }
}

/// The file gh wrote, held to the size the release lists and to `limit`,
/// into `into`. A size that is not the listed one is
/// `Unread(Inconsistent)`; a file that is not there, or a writer that
/// fails, `Unread(Malformed)`, as the ports' fake answers a writer that
/// fails.
fn copy(path: &Path, listed: u64, limit: u64, into: &mut dyn Write) -> Did {
    let malformed = Outcome::Unread(Unread::Malformed);
    let mut f = fs::File::open(path).map_err(|_| malformed.clone())?;
    let size = f.metadata().map_err(|_| malformed.clone())?.len();
    if size > limit {
        return Err(Outcome::Unread(Unread::TooLarge));
    }
    if size != listed {
        return Err(Outcome::Unread(Unread::Inconsistent));
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut f)
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| malformed.clone())?;
    if bytes.len() as u64 != size {
        return Err(Outcome::Unread(Unread::Inconsistent));
    }
    into.write_all(&bytes).map_err(|_| malformed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_one_file_to_gh_s_pattern() {
        assert_eq!(pattern("tehas-v1.tar.gz"), "tehas-v1.tar.gz");
        assert_eq!(pattern("a*b?[c]"), "a\\*b\\?\\[c\\]");
    }

    #[test]
    fn a_scratch_directory_is_its_own_and_goes() {
        let a = Scratch::new().unwrap();
        let b = Scratch::new().unwrap();
        assert_ne!(a.0, b.0);
        let at = a.0.clone();
        assert!(at.is_dir());
        drop(a);
        assert!(!at.exists());
    }
}
