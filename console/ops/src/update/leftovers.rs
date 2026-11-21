//! Files an earlier run left in the home that nothing installs.
//!
//!   console update leftovers [--check] <home>
//!   console update cycles <home>
//!
//! `leftovers` finds the two names a sandbox build stages beside the installed
//! build script, `sandbox/sandbox-uid.Dockerfile` and its `mktemp` temporary
//! `sandbox/sandbox-uid.Dockerfile.XXXXXX`, as plain files. A build that ended
//! takes both back, so one found here is a leftover of a run a signal took off.
//! For each it prints `changed sandbox/<name>`, as `update files` names a
//! change, then `stale: <path>` with --check, else removes it and prints
//! `removed: <path>`, or the reason on standard error for one it could not
//! remove. Exit 0, 64 usage.
//!
//! `cycles` removes the marker a cycle used to leave, `state/cycles/<issue>`:
//! a plain file named by digits alone. The lock, the taken mark and every file
//! with a suffix stay. It prints `removed: <path>` for each, as `leftovers`
//! does. Exit 0, 64 usage.

use super::{say, tell};
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

const STAGED: &str = "sandbox-uid.Dockerfile";

pub fn leftovers_verb(given: &[String]) -> u8 {
    let (check, home) = match given {
        [flag, home] if flag == "--check" => (true, home),
        [home] if home != "--check" => (false, home),
        _ => {
            tell("console update leftovers: usage: [--check] <home>");
            return 64;
        }
    };
    for line in leftovers(Path::new(home), check) {
        say(&line);
    }
    0
}

fn leftovers(home: &Path, check: bool) -> Vec<String> {
    let mut lines = Vec::new();
    for path in plain_files(&home.join("sandbox"), staged_name) {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        lines.push(format!("changed sandbox/{name}"));
        if check {
            lines.push(format!("stale: {}", path.display()));
        } else if let Some(line) = remove(&path) {
            lines.push(line);
        }
    }
    lines
}

/// The plain files in `dir` whose names `keep` takes, in byte order.
fn plain_files(dir: &Path, keep: fn(&[u8]) -> bool) -> Vec<PathBuf> {
    let mut found: Vec<_> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.file_name())
        .filter(|name| keep(name.as_bytes()))
        .collect();
    found.sort();
    found.into_iter().map(|name| dir.join(name)).collect()
}

/// `removed: <path>`, or None with the reason on standard error, as `rm -f` says it.
fn remove(path: &Path) -> Option<String> {
    match fs::remove_file(path) {
        Ok(()) => Some(format!("removed: {}", path.display())),
        Err(e) => {
            tell(&format!(
                "console update: cannot remove {}: {e}",
                path.display()
            ));
            None
        }
    }
}

fn staged_name(name: &[u8]) -> bool {
    match name.strip_prefix(STAGED.as_bytes()) {
        Some(b"") => true,
        Some(rest) => rest
            .strip_prefix(b".")
            .is_some_and(|s| s.len() == 6 && s.iter().all(u8::is_ascii_alphanumeric)),
        None => false,
    }
}

fn marker_name(name: &[u8]) -> bool {
    !name.is_empty() && name.iter().all(u8::is_ascii_digit)
}

pub fn cycles_verb(given: &[String]) -> u8 {
    let [home] = given else {
        tell("console update cycles: usage: <home>");
        return 64;
    };
    for line in cycles(Path::new(home)) {
        say(&line);
    }
    0
}

fn cycles(home: &Path) -> Vec<String> {
    plain_files(&home.join("state/cycles"), marker_name)
        .iter()
        .filter_map(|path| remove(path))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::testing::dir;
    use super::*;
    use std::os::unix::fs::symlink;

    fn names(dir: &Path) -> Vec<String> {
        let mut found: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().into_string().unwrap())
            .collect();
        found.sort();
        found
    }

    #[test]
    fn the_copy_a_build_left_in_the_sandbox_directory_is_named_then_removed() {
        let d = dir("leftovers");
        let home = d.0.join("home");
        let sandbox = home.join("sandbox");
        fs::create_dir_all(&sandbox).unwrap();
        for name in [
            "sandbox-uid.Dockerfile",
            "sandbox-uid.Dockerfile.abcdef",
            "sandbox-uid.Dockerfile.toolong",
            "sandbox-uid.Dockerfilex",
            "Dockerfile",
            "build.sh",
        ] {
            fs::write(sandbox.join(name), "FROM nowhere\n").unwrap();
        }
        let outside = d.0.join("outside-named");
        fs::write(&outside, "x\n").unwrap();
        symlink(&outside, sandbox.join("sandbox-uid.Dockerfile.link01")).unwrap();
        fs::create_dir(sandbox.join("sandbox-uid.Dockerfile.dir001")).unwrap();

        let both = vec![
            sandbox.join("sandbox-uid.Dockerfile"),
            sandbox.join("sandbox-uid.Dockerfile.abcdef"),
        ];
        assert_eq!(plain_files(&sandbox, staged_name), both);
        let said = |word: &str| {
            vec![
                "changed sandbox/sandbox-uid.Dockerfile".to_string(),
                format!("{word}: {}", both[0].display()),
                "changed sandbox/sandbox-uid.Dockerfile.abcdef".to_string(),
                format!("{word}: {}", both[1].display()),
            ]
        };
        // --check names them and removes nothing.
        assert_eq!(leftovers(&home, true), said("stale"));
        assert_eq!(plain_files(&sandbox, staged_name), both);
        assert_eq!(leftovers(&home, false), said("removed"));
        assert!(plain_files(&sandbox, staged_name).is_empty());
        // The shapes that are not a build's two names are left for their owner,
        // as the installed files beside them are.
        assert_eq!(
            names(&sandbox),
            [
                "Dockerfile",
                "build.sh",
                "sandbox-uid.Dockerfile.dir001",
                "sandbox-uid.Dockerfile.link01",
                "sandbox-uid.Dockerfile.toolong",
                "sandbox-uid.Dockerfilex",
            ]
        );
        assert!(outside.exists());
    }

    #[test]
    fn a_home_with_no_sandbox_directory_has_no_leftovers() {
        let d = dir("leftovers-none");
        assert!(leftovers(&d.0, true).is_empty());
        assert!(leftovers(&d.0, false).is_empty());
    }

    #[test]
    fn the_marker_a_cycle_used_to_leave_is_removed_and_its_neighbours_stay() {
        let d = dir("cycles");
        let markers = d.0.join("state/cycles");
        fs::create_dir_all(&markers).unwrap();
        for n in ["5", "23", "25", "26"] {
            fs::write(markers.join(n), "").unwrap();
        }
        for name in [
            "5.lock",
            "5.taken",
            "5.park",
            "5.result.json",
            "5x",
            "notanumber",
            ".hidden",
        ] {
            fs::write(markers.join(name), "keep\n").unwrap();
        }
        fs::create_dir(markers.join("77")).unwrap();
        symlink("/etc/passwd", markers.join("99")).unwrap();
        let gone = |n: &str| format!("removed: {}", markers.join(n).display());
        assert_eq!(
            cycles(&d.0),
            [gone("23"), gone("25"), gone("26"), gone("5")]
        );
        assert_eq!(
            names(&markers),
            [
                ".hidden",
                "5.lock",
                "5.park",
                "5.result.json",
                "5.taken",
                "5x",
                "77",
                "99",
                "notanumber",
            ]
        );
    }

    #[test]
    fn a_leftover_that_cannot_be_removed_is_still_named_and_not_said_removed() {
        use std::os::unix::fs::PermissionsExt;
        let d = dir("leftovers-kept");
        let sandbox = d.0.join("sandbox");
        fs::create_dir_all(&sandbox).unwrap();
        fs::write(sandbox.join(STAGED), "").unwrap();
        fs::set_permissions(&sandbox, fs::Permissions::from_mode(0o500)).unwrap();
        let said = leftovers(&d.0, false);
        let kept = sandbox.join(STAGED).exists();
        fs::set_permissions(&sandbox, fs::Permissions::from_mode(0o700)).unwrap();
        // root removes it anyway.
        if kept {
            assert_eq!(said, ["changed sandbox/sandbox-uid.Dockerfile"]);
        }
    }

    #[test]
    fn a_usage_is_64() {
        assert_eq!(leftovers_verb(&[]), 64);
        assert_eq!(leftovers_verb(&["--check".into()]), 64);
        assert_eq!(leftovers_verb(&["--x".into(), "a".into()]), 64);
        assert_eq!(cycles_verb(&[]), 64);
    }
}
