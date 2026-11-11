//! Which compiled programs install.sh installs, and the stamp it leaves beside
//! each one.
//!
//!   console update programs <home> release <tree> <program>...
//!   console update programs <home> build <workspace> <program>...
//!   console update programs stamp <installed program> release|<source sum>
//!
//! `release` judges the programs a release tree carries in `<tree>/libexec`,
//! `build` the ones built from the workspace. A program is due when
//! `<home>/libexec/<program>` is not executable or its `.stamp` is not the one
//! this tree would leave. One line per due program, in the order asked:
//! `missing <program> <stamp>` when nothing is installed under that name,
//! `differs <program> <stamp>` when something is. `<stamp>` is `release`, or
//! the checksum of the program's source (src/update/sources.rs). A release that
//! does not carry a program prints `absent <program>`.
//!
//! `stamp` writes `<installed program>.stamp` once the program is copied: for
//! a release `release <checksum of the program>`, for a build `<source sum>
//! <checksum of the program>`. These are the lines an older install.sh wrote
//! and compares, so either reads the other's.
//!
//! Exit: 0, 1 a file cannot be read or written or a package is not in the
//! workspace, 64 usage.

use super::sources::{hex, source_sum, workspace};
use super::{fault, say, tell};
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tehas_core::file::{write_atomic, Temp};

const USAGE: &str = "console update programs: usage: <home> release|build <tree> <program>... | stamp <installed program> release|<source sum>";

fn executable(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

fn file_sum(path: &Path) -> Result<String, String> {
    fs::read(path)
        .map(|bytes| hex(&bytes))
        .map_err(|e| format!("cannot read {}: {e}", path.display()))
}

fn stamp_path(program: &Path) -> PathBuf {
    let mut name = program.as_os_str().to_owned();
    name.push(".stamp");
    PathBuf::from(name)
}

/// The stamp beside an installed program, as `$(cat ...)` read it: no
/// trailing new lines, and nothing when there is none.
fn stamp_of(program: &Path) -> String {
    let text = fs::read(stamp_path(program)).unwrap_or_default();
    String::from_utf8_lossy(&text)
        .trim_end_matches('\n')
        .to_string()
}

/// The line for one program, or nothing when the one installed is current.
/// `want` is the stamp this tree leaves, given the installed program's
/// checksum.
fn judge(
    to: &Path,
    name: &str,
    stamp: &str,
    want: impl FnOnce() -> Result<String, String>,
) -> Result<Option<String>, String> {
    if executable(to) && stamp_of(to) == want()? {
        return Ok(None);
    }
    let state = if to.is_file() { "differs" } else { "missing" };
    Ok(Some(format!("{state} {name} {stamp}")))
}

fn due(home: &Path, kind: &str, tree: &Path, names: &[String]) -> Result<Vec<String>, String> {
    let libexec = home.join("libexec");
    let mut lines = Vec::new();
    match kind {
        "release" => {
            for name in names {
                let from = tree.join("libexec").join(name);
                if !executable(&from) {
                    lines.push(format!("absent {name}"));
                    continue;
                }
                let sum = file_sum(&from)?;
                let to = libexec.join(name);
                lines.extend(judge(&to, name, "release", || {
                    Ok(format!("release {sum}"))
                })?);
            }
        }
        _ => {
            let ws = workspace(tree)?;
            for name in names {
                let sum = source_sum(tree, &ws, name)?;
                let to = libexec.join(name);
                let want = || Ok(format!("{sum} {}", file_sum(&to)?));
                lines.extend(judge(&to, name, &sum, want)?);
            }
        }
    }
    Ok(lines)
}

fn is_sum(word: &str) -> bool {
    word.len() == 64 && word.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn stamp(program: &Path, source: &str) -> Result<(), String> {
    let sum = file_sum(program)?;
    let line = if source == "release" {
        format!("release {sum}\n")
    } else {
        format!("{source} {sum}\n")
    };
    let path = stamp_path(program);
    let mode = fs::metadata(&path)
        .map(|m| m.permissions().mode() & 0o7777)
        .unwrap_or(0o644);
    write_atomic(&path, line.as_bytes(), mode, &Temp::Predictable)
        .map_err(|e: io::Error| format!("could not write {}: {e}", path.display()))
}

pub fn verb(given: &[String]) -> u8 {
    let done = match given {
        [w, program, source] if w == "stamp" => {
            if source != "release" && !is_sum(source) {
                tell(USAGE);
                return 64;
            }
            stamp(Path::new(program), source).map(|()| Vec::new())
        }
        [home, kind, tree, names @ ..]
            if (kind == "release" || kind == "build") && !names.is_empty() =>
        {
            due(Path::new(home), kind, Path::new(tree), names)
        }
        _ => {
            tell(USAGE);
            return 64;
        }
    };
    match done {
        Ok(lines) => {
            for line in lines {
                say(&line);
            }
            0
        }
        Err(e) => {
            fault(&e);
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{dir, Dir};
    use super::*;
    use std::process::Command;

    fn exe(d: &Dir, name: &str, text: &str) {
        let path = d.file(name, text);
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn workspace_tree(d: &Dir) {
        d.file(
            "src/console/Cargo.toml",
            "[workspace]\nmembers = [\"store\"]\n\n[package]\nname = \"console\"\n",
        );
        d.file("src/console/Cargo.lock", "lock\n");
        d.file("src/console/src/main.rs", "fn main() {}\n");
        d.file(
            "src/console/store/Cargo.toml",
            "[package]\nname = \"store\"\n",
        );
        d.file("src/console/store/src/main.rs", "fn main() {}\n");
    }

    fn sum_of(d: &Dir, name: &str) -> String {
        let root = d.0.join("src/console");
        source_sum(&root, &workspace(&root).unwrap(), name).unwrap()
    }

    fn build(d: &Dir) -> Vec<String> {
        due(
            &d.0.join("home"),
            "build",
            &d.0.join("src/console"),
            &names(&["console", "store"]),
        )
        .unwrap()
    }

    fn release(d: &Dir, list: &[&str]) -> Vec<String> {
        due(
            &d.0.join("home"),
            "release",
            &d.0.join("tree"),
            &names(list),
        )
        .unwrap()
    }

    /// Both installed and stamped by this verb, as a run that went well leaves them.
    fn installed(d: &Dir) {
        for name in ["console", "store"] {
            exe(
                d,
                &format!("home/libexec/{name}"),
                &format!("{name} built\n"),
            );
            let sum = sum_of(d, name);
            stamp(&d.0.join(format!("home/libexec/{name}")), &sum).unwrap();
        }
    }

    #[test]
    fn a_checkout_installs_what_is_not_there() {
        let d = dir("programs-fresh");
        workspace_tree(&d);
        let (c, s) = (sum_of(&d, "console"), sum_of(&d, "store"));
        assert_eq!(
            build(&d),
            [format!("missing console {c}"), format!("missing store {s}")]
        );
    }

    #[test]
    fn a_checkout_with_nothing_changed_has_nothing_due() {
        let d = dir("programs-same");
        workspace_tree(&d);
        installed(&d);
        assert!(build(&d).is_empty());
    }

    #[test]
    fn a_changed_source_builds_that_program_alone() {
        let d = dir("programs-source");
        workspace_tree(&d);
        installed(&d);
        d.file("src/console/store/src/main.rs", "fn main() { 1; }\n");
        assert_eq!(
            build(&d),
            [format!("differs store {}", sum_of(&d, "store"))]
        );
        // The workspace's lock is in every program's source.
        d.file("src/console/Cargo.lock", "lock 2\n");
        assert_eq!(build(&d).len(), 2);
    }

    #[test]
    fn a_replaced_program_is_due_again() {
        let d = dir("programs-replaced");
        workspace_tree(&d);
        installed(&d);
        exe(&d, "home/libexec/console", "another console\n");
        assert_eq!(
            build(&d),
            [format!("differs console {}", sum_of(&d, "console"))]
        );
        // One that is there but cannot be run is due too.
        installed(&d);
        fs::set_permissions(
            d.0.join("home/libexec/store"),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        assert_eq!(
            build(&d),
            [format!("differs store {}", sum_of(&d, "store"))]
        );
    }

    #[test]
    fn a_release_tree_installs_what_it_carries() {
        let d = dir("programs-release");
        exe(&d, "tree/libexec/console", "console 1\n");
        exe(&d, "tree/libexec/store", "store 1\n");
        let list = ["console", "store"];
        assert_eq!(
            release(&d, &list),
            ["missing console release", "missing store release"]
        );
        for name in list {
            exe(&d, &format!("home/libexec/{name}"), &format!("{name} 1\n"));
            stamp(&d.0.join(format!("home/libexec/{name}")), "release").unwrap();
        }
        assert!(release(&d, &list).is_empty());
        exe(&d, "tree/libexec/store", "store 2\n");
        assert_eq!(release(&d, &list), ["differs store release"]);
    }

    #[test]
    fn a_build_from_a_checkout_is_not_mistaken_for_the_release() {
        let d = dir("programs-release-build");
        workspace_tree(&d);
        installed(&d);
        exe(&d, "tree/libexec/console", "console built\n");
        assert_eq!(release(&d, &["console"]), ["differs console release"]);
    }

    #[test]
    fn a_release_with_no_store_says_so() {
        let d = dir("programs-no-store");
        exe(&d, "tree/libexec/console", "console 1\n");
        d.file("tree/libexec/store", "not a program\n");
        assert_eq!(
            release(&d, &["console", "store"]),
            ["missing console release", "absent store"]
        );
    }

    /// The lines an older install.sh wrote and compares.
    #[test]
    fn the_stamp_is_the_one_an_older_install_reads() {
        let d = dir("programs-stamp");
        exe(&d, "home/libexec/store", "store 1\n");
        let program = d.0.join("home/libexec/store");
        let shell = |line: &str| {
            let out = Command::new("bash")
                .arg("-c")
                .arg("[ \"$(cat \"$1.stamp\")\" = \"$2 $(sha256sum <\"$1\" | cut -d' ' -f1)\" ]")
                .arg("-")
                .arg(&program)
                .arg(line)
                .status()
                .unwrap();
            out.success()
        };
        stamp(&program, "release").unwrap();
        assert!(shell("release"));
        let src = "a".repeat(64);
        stamp(&program, &src).unwrap();
        assert!(shell(&src));
        // And a stamp that an older install.sh wrote is read as current.
        fs::write(
            d.0.join("home/libexec/store.stamp"),
            format!("release {}\n", file_sum(&program).unwrap()),
        )
        .unwrap();
        assert_eq!(
            stamp_of(&program),
            format!("release {}", file_sum(&program).unwrap())
        );
    }

    #[test]
    fn usage_is_refused() {
        assert_eq!(verb(&names(&["stamp", "/x", "nope"])), 64);
        assert_eq!(verb(&names(&["/home", "other", "/tree", "console"])), 64);
        assert_eq!(verb(&names(&["/home", "build", "/tree"])), 64);
    }

    #[test]
    fn an_unreadable_workspace_stops() {
        let d = dir("programs-no-workspace");
        let given = names(&[
            &d.0.join("home").to_string_lossy(),
            "build",
            &d.0.join("none").to_string_lossy(),
            "console",
        ]);
        assert_eq!(verb(&given), 1);
    }
}
