//! A cycle's own copy of the code it runs (#989), and a planning turn's
//! (#991). The install's bin/, libexec/ and lib/ are hardlinked into
//! run/<kind>-<issue>-<pid>/ when it starts, and it runs from there. install.sh replaces a file by renaming a new
//! one over it, never by writing into it, so the copy keeps the file the cycle
//! started with, and an update needs no drain. Every script finds its siblings
//! relative to its own path, so the whole cycle stays on the copy.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The directories of the install a cycle runs from.
pub const TREES: [&str; 3] = ["bin", "libexec", "lib"];

/// Where the copies are, under the home.
pub fn run_dir(home: &Path) -> PathBuf {
    home.join("run")
}

/// What runs from a copy: a cycle or a planning turn.
pub const KINDS: [&str; 2] = ["cycle", "plan"];

/// The pid a copy is named for, from its name `<kind>-<issue>-<pid>`.
fn pid_of(name: &str) -> Option<u32> {
    let (kind, rest) = name.split_once('-')?;
    if !KINDS.contains(&kind) {
        return None;
    }
    let (issue, pid) = rest.split_once('-')?;
    issue.parse::<u64>().ok()?;
    pid.parse().ok()
}

/// Whether a process runs with this pid.
pub fn alive(pid: u32) -> bool {
    Path::new("/proc").join(pid.to_string()).exists()
}

/// Removes every copy whose process is gone; returns the ones it removed.
/// A copy is named for the process of its cycle, which keeps the pid through
/// the exec, so a copy of a process that is gone is used by nothing.
pub fn sweep(home: &Path, alive: impl Fn(u32) -> bool) -> Vec<PathBuf> {
    let mut gone = vec![];
    let Ok(entries) = fs::read_dir(run_dir(home)) else {
        return gone;
    };
    for e in entries.flatten() {
        let name = e.file_name();
        let Some(pid) = name.to_str().and_then(pid_of) else {
            continue;
        };
        if !alive(pid) && fs::remove_dir_all(e.path()).is_ok() {
            gone.push(e.path());
        }
    }
    gone
}

/// Makes the copy of the install for the `kind` of `issue` in process `pid`
/// and returns its path. A copy that cannot be made whole is removed.
pub fn make(home: &Path, kind: &str, issue: u64, pid: u32) -> Result<PathBuf, String> {
    let run = run_dir(home);
    fs::create_dir_all(&run).map_err(|e| format!("{}: {e}", run.display()))?;
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&run, fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("{}: {e}", run.display()))?;
    }
    let dir = run.join(format!("{kind}-{issue}-{pid}"));
    // Left by an earlier process of the same pid.
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let made = TREES.iter().try_for_each(|t| {
        let from = home.join(t);
        if !from.is_dir() {
            return Err(format!("{} is not a directory", from.display()));
        }
        link_tree(&from, &dir.join(t)).map_err(|e| format!("{}: {e}", from.display()))
    });
    if let Err(e) = made {
        let _ = fs::remove_dir_all(&dir);
        return Err(e);
    }
    Ok(dir)
}

/// Hardlinks every file under `from` into `to`, and copies each symlink as a
/// symlink. A `.new` file is install.sh's copy in the middle of being written,
/// and is left out.
fn link_tree(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from)? {
        let e = e?;
        let name = e.file_name();
        if name.to_string_lossy().ends_with(".new") {
            continue;
        }
        let (src, dst) = (e.path(), to.join(&name));
        let kind = e.file_type()?;
        if kind.is_symlink() {
            std::os::unix::fs::symlink(fs::read_link(&src)?, &dst)?;
        } else if kind.is_dir() {
            link_tree(&src, &dst)?;
        } else {
            fs::hard_link(&src, &dst)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    fn home(name: &str) -> PathBuf {
        let h = std::env::temp_dir().join(format!("tehas-pin-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&h);
        for t in TREES {
            fs::create_dir_all(h.join(t)).unwrap();
        }
        fs::write(h.join("bin/tehasbot"), "old tehasbot").unwrap();
        fs::write(h.join("libexec/work"), "old work").unwrap();
        fs::create_dir_all(h.join("lib/sub")).unwrap();
        fs::write(h.join("lib/sub/x.sh"), "x").unwrap();
        std::os::unix::fs::symlink("x.sh", h.join("lib/sub/y.sh")).unwrap();
        fs::write(h.join("libexec/cycle.new"), "half").unwrap();
        h
    }

    #[test]
    fn the_copy_is_hardlinks_of_the_install() {
        let h = home("links");
        let dir = make(&h, "cycle", 7, 42).unwrap();
        assert_eq!(dir, h.join("run/cycle-7-42"));
        let ino = |p: &Path| fs::metadata(p).unwrap().ino();
        assert_eq!(ino(&h.join("libexec/work")), ino(&dir.join("libexec/work")));
        assert_eq!(ino(&h.join("lib/sub/x.sh")), ino(&dir.join("lib/sub/x.sh")));
        assert_eq!(
            fs::read_link(dir.join("lib/sub/y.sh")).unwrap(),
            Path::new("x.sh")
        );
        assert!(!dir.join("libexec/cycle.new").exists());
        let mode = fs::metadata(h.join("run")).unwrap().mode() & 0o777;
        assert_eq!(mode, 0o700);
        fs::remove_dir_all(&h).unwrap();
    }

    #[test]
    fn a_file_the_install_replaces_stays_old_in_the_copy() {
        let h = home("rename");
        let dir = make(&h, "cycle", 7, 42).unwrap();
        // As install.sh's put does: beside it, then renamed over it.
        fs::write(h.join("libexec/work.new"), "new work").unwrap();
        fs::rename(h.join("libexec/work.new"), h.join("libexec/work")).unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("libexec/work")).unwrap(),
            "old work"
        );
        assert_eq!(
            fs::read_to_string(h.join("libexec/work")).unwrap(),
            "new work"
        );
        fs::remove_dir_all(&h).unwrap();
    }

    #[test]
    fn a_copy_of_a_process_that_is_gone_is_removed() {
        let h = home("sweep");
        make(&h, "cycle", 7, 41).unwrap();
        make(&h, "plan", 8, 42).unwrap();
        make(&h, "plan", 9, 43).unwrap();
        fs::create_dir_all(h.join("run/other")).unwrap();
        let gone = sweep(&h, |pid| pid == 42);
        let mut gone = gone;
        gone.sort();
        assert_eq!(
            gone,
            vec![h.join("run/cycle-7-41"), h.join("run/plan-9-43")]
        );
        assert!(h.join("run/plan-8-42").is_dir());
        assert!(h.join("run/other").is_dir());
        fs::remove_dir_all(&h).unwrap();
    }

    #[test]
    fn a_copy_that_cannot_be_made_is_not_left() {
        let h = home("partial");
        fs::remove_dir_all(h.join("lib")).unwrap();
        let e = make(&h, "cycle", 7, 42).unwrap_err();
        assert!(e.contains("lib is not a directory"), "{e}");
        assert!(!h.join("run/cycle-7-42").exists());
        fs::remove_dir_all(&h).unwrap();
    }

    #[test]
    fn a_copy_left_by_the_same_pid_is_made_again() {
        let h = home("again");
        let dir = make(&h, "cycle", 7, 42).unwrap();
        fs::write(dir.join("libexec/stray"), "s").unwrap();
        let dir = make(&h, "cycle", 7, 42).unwrap();
        assert!(!dir.join("libexec/stray").exists());
        fs::remove_dir_all(&h).unwrap();
    }

    #[test]
    fn only_the_names_of_copies_carry_a_pid() {
        assert_eq!(pid_of("cycle-7-42"), Some(42));
        assert_eq!(pid_of("cycle-x-42"), None);
        assert_eq!(pid_of("cycle-7"), None);
        assert_eq!(pid_of("plan-7-42"), Some(42));
        assert_eq!(pid_of("other-7-42"), None);
    }
}
