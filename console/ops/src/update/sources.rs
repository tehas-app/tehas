//! The checksum of the source each program of the workspace is built from,
//! which install.sh keeps in `<program>.stamp` and compares to decide what to
//! build again.
//!
//!   console update sources <workspace> <package>...
//!
//! One line per package, `<package> <sha256>`, in the order asked. The source of
//! a package is the files of its own directory, the workspace's `Cargo.toml` and
//! `Cargo.lock`, and the source of every package it takes by `path` among its
//! dependencies and build dependencies, all the way down. Left out of each
//! directory: `target/`, `tests/` and `src/tests.rs`, which no release build
//! reads, and for the root package the directories of the members. A
//! dev-dependency is not followed. So the members of the workspace and their
//! manifests are the one list: a new crate needs no new rule here.
//!
//! The checksum is the one `find | sort | xargs sha256sum | sha256sum` gives
//! for the same files: each file's `sha256sum` line, `./` and the path from the
//! workspace, sorted by path, and the checksum of those lines.
//!
//! Exit: 0, 1 a manifest or a file cannot be read or a package is not in the
//! workspace, 64 usage.

use super::{fault, say, tell};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

/// A package of the workspace: its directory from the root ("" for the root),
/// and the directories of the packages it takes by path.
pub(super) struct Package {
    dir: String,
    deps: Vec<String>,
}

/// Each package by name.
pub(super) type Workspace = BTreeMap<String, Package>;

pub fn verb(given: &[String]) -> u8 {
    let [root, packages @ ..] = given else {
        tell("console update sources: usage: <workspace> <package>...");
        return 64;
    };
    if packages.is_empty() {
        tell("console update sources: usage: <workspace> <package>...");
        return 64;
    }
    let root = Path::new(root);
    let lines = workspace(root).and_then(|ws| {
        packages
            .iter()
            .map(|name| Ok(format!("{name} {}", source_sum(root, &ws, name)?)))
            .collect::<Result<Vec<_>, String>>()
    });
    match lines {
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

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

/// The packages of the workspace at `root`: the root's own and each member's.
pub(super) fn workspace(root: &Path) -> Result<Workspace, String> {
    let manifest = root.join("Cargo.toml");
    let text = read(&manifest)?;
    let mut ws = Workspace::new();
    let mut dirs = vec![String::new()];
    for member in members(&text).ok_or_else(|| format!("{}: no members", manifest.display()))? {
        if member.contains(['*', '?', '[']) {
            return Err(format!("{}: member {member} is a glob", manifest.display()));
        }
        dirs.push(within("", &member).ok_or_else(|| {
            format!(
                "{}: member {member} is outside the workspace",
                manifest.display()
            )
        })?);
    }
    for dir in dirs {
        let manifest = root.join(&dir).join("Cargo.toml");
        let text = read(&manifest)?;
        let name = package_name(&text)
            .ok_or_else(|| format!("{}: no package name", manifest.display()))?;
        let mut deps = Vec::new();
        for path in path_deps(&text) {
            deps.push(within(&dir, &path).ok_or_else(|| {
                format!(
                    "{}: dependency {path} is outside the workspace",
                    manifest.display()
                )
            })?);
        }
        ws.insert(name, Package { dir, deps });
    }
    Ok(ws)
}

/// `rel` from the directory `base`, both relative to the root, as a path of
/// normal parts joined by `/`; None when it leaves the root.
fn within(base: &str, rel: &str) -> Option<String> {
    let mut parts: Vec<String> = base
        .split('/')
        .filter(|p| !p.is_empty())
        .map(String::from)
        .collect();
    for part in Path::new(rel).components() {
        match part {
            Component::Normal(p) => parts.push(p.to_str()?.to_string()),
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop()?;
            }
            _ => return None,
        }
    }
    Some(parts.join("/"))
}

/// The strings of `[workspace] members = [...]`, which may span lines.
fn members(manifest: &str) -> Option<Vec<String>> {
    let mut section = String::new();
    let mut list: Option<String> = None;
    for line in manifest.lines() {
        let line = uncomment(line);
        if let Some(open) = list.as_mut() {
            open.push_str(line);
            open.push('\n');
        } else if let Some(name) = header(line) {
            section = name;
            continue;
        } else if section == "workspace" {
            if let Some((key, value)) = line.split_once('=') {
                if key.trim() == "members" {
                    list = Some(format!("{value}\n"));
                }
            }
        }
        if let Some(open) = &list {
            if open.contains(']') {
                let inner = open.split_once('[')?.1.split_once(']')?.0;
                return Some(quoted(inner));
            }
        }
    }
    None
}

/// The strings in double quotes, in order.
fn quoted(text: &str) -> Vec<String> {
    text.split('"')
        .skip(1)
        .step_by(2)
        .map(String::from)
        .collect()
}

/// A line without its comment. A `#` inside a string is not looked for: the
/// manifests here have none.
fn uncomment(line: &str) -> &str {
    line.split_once('#').map_or(line, |(code, _)| code).trim()
}

/// The table a `[name]` or `[[name]]` line opens.
fn header(line: &str) -> Option<String> {
    let inner = line.strip_prefix('[')?.strip_suffix(']')?;
    let inner = inner
        .strip_prefix('[')
        .and_then(|i| i.strip_suffix(']'))
        .unwrap_or(inner);
    Some(inner.trim().to_string())
}

fn package_name(manifest: &str) -> Option<String> {
    let mut section = String::new();
    for line in manifest.lines() {
        let line = uncomment(line);
        if let Some(name) = header(line) {
            section = name;
        } else if section == "package" {
            if let Some((key, value)) = line.split_once('=') {
                if key.trim() == "name" {
                    return quoted(value).into_iter().next();
                }
            }
        }
    }
    None
}

/// A table whose entries a release build follows: dependencies and build
/// dependencies, a target's too, and one dependency's own table. Not
/// dev-dependencies.
fn follows(section: &str) -> bool {
    let kinds = ["dependencies", "build-dependencies"];
    section
        .split('.')
        .enumerate()
        .any(|(i, part)| kinds.contains(&part) && (i == 0 || section.starts_with("target.")))
}

/// The `path = "..."` of every dependency a release build follows.
fn path_deps(manifest: &str) -> Vec<String> {
    let mut section = String::new();
    let mut paths = Vec::new();
    for line in manifest.lines() {
        let line = uncomment(line);
        if let Some(name) = header(line) {
            section = name;
            continue;
        }
        if !follows(&section) {
            continue;
        }
        let mut rest = line;
        while let Some(at) = rest.find("path") {
            let before = rest[..at].chars().next_back();
            let after = rest[at + 4..].trim_start();
            rest = &rest[at + 4..];
            if before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-') {
                continue;
            }
            if let Some(value) = after.strip_prefix('=') {
                if let Some(path) = quoted(value).into_iter().next() {
                    paths.push(path);
                }
            }
        }
    }
    paths
}

/// The package's directories, its own first, then those it takes by path.
fn closure(ws: &Workspace, name: &str) -> Result<BTreeSet<String>, String> {
    let by_dir: BTreeMap<&str, &Package> = ws.values().map(|p| (p.dir.as_str(), p)).collect();
    let first = ws
        .get(name)
        .ok_or_else(|| format!("no package {name} in the workspace"))?;
    let mut seen = BTreeSet::new();
    let mut todo = vec![first.dir.clone()];
    while let Some(dir) = todo.pop() {
        if !seen.insert(dir.clone()) {
            continue;
        }
        let pkg = by_dir.get(dir.as_str()).ok_or_else(|| {
            format!("{name} takes {dir} by path, and it is no member of the workspace")
        })?;
        todo.extend(pkg.deps.iter().cloned());
    }
    Ok(seen)
}

/// The files of the package in `dir`, as paths from the root. The root's own
/// leave out every member's directory.
fn package_files(root: &Path, ws: &Workspace, dir: &str) -> Result<Vec<String>, String> {
    let mut skip: BTreeSet<String> = ["target", "tests"].iter().map(|d| join(dir, d)).collect();
    if dir.is_empty() {
        skip.extend(ws.values().map(|p| p.dir.clone()).filter(|d| !d.is_empty()));
    }
    let tests_rs = join(dir, "src/tests.rs");
    let mut files = Vec::new();
    walk(root, dir, &skip, &mut files)?;
    files.retain(|f| *f != tests_rs);
    Ok(files)
}

fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// The regular files under `dir`, as find -type f lists them: a link is not
/// followed and not listed.
fn walk(
    root: &Path,
    dir: &str,
    skip: &BTreeSet<String>,
    files: &mut Vec<String>,
) -> Result<(), String> {
    let path = root.join(dir);
    let entries = match fs::read_dir(&path) {
        Ok(entries) => entries,
        // A member's directory a tree does not have holds no source.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !dir.is_empty() => return Ok(()),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    for entry in entries {
        let entry = entry.map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let Some(name) = entry.file_name().to_str().map(String::from) else {
            return Err(format!("{}: a file name that is not UTF-8", path.display()));
        };
        let rel = join(dir, &name);
        let kind = entry
            .file_type()
            .map_err(|e| format!("cannot read {}: {e}", root.join(&rel).display()))?;
        if kind.is_dir() {
            if !skip.contains(&rel) {
                walk(root, &rel, skip, files)?;
            }
        } else if kind.is_file() {
            files.push(rel);
        }
    }
    Ok(())
}

/// What sha256sum prints for one file. A name with a backslash or a new line
/// is escaped as sha256sum escapes it.
fn sum_line(hex: &str, rel: &str) -> String {
    let name = format!("./{rel}");
    if name.contains(['\\', '\n']) {
        let escaped = name.replace('\\', "\\\\").replace('\n', "\\n");
        format!("\\{hex}  {escaped}\n")
    } else {
        format!("{hex}  {name}\n")
    }
}

pub(super) fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub(super) fn source_sum(root: &Path, ws: &Workspace, name: &str) -> Result<String, String> {
    let mut files: BTreeSet<String> = ["Cargo.toml", "Cargo.lock"].map(String::from).into();
    for dir in closure(ws, name)? {
        files.extend(package_files(root, ws, &dir)?);
    }
    let mut lines = String::new();
    // sort -z sorts the bytes of "./<path>", which is the order of the paths.
    for rel in &files {
        let path: PathBuf = root.join(rel);
        let bytes = fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        lines.push_str(&sum_line(&hex(&bytes), rel));
    }
    Ok(hex(lines.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::super::testing::{dir, Dir};
    use super::*;
    use std::process::Command;

    const ROOT: &str = r#"
[workspace]
members = ["lib", # the shared one
  "app",
  "other"]

[package]
name = "main"

[dependencies]
serde = "1"
shared = { path = "lib", features = ["x"] }

[dev-dependencies]
app = { path = "app" }
"#;

    fn tree(name: &str) -> Dir {
        let d = dir(name);
        d.file("Cargo.toml", ROOT);
        d.file("Cargo.lock", "lock\n");
        d.file("src/main.rs", "fn main() {}\n");
        d.file("src/tests.rs", "// a test\n");
        d.file("tests/it.rs", "// a test\n");
        d.file("target/release/main", "built\n");
        d.file("lib/Cargo.toml", "[package]\nname = \"shared\"\n");
        d.file("lib/src/lib.rs", "// lib\n");
        d.file(
            "app/Cargo.toml",
            "[package]\nname = \"app\"\n\n[[bin]]\nname = \"app\"\npath = \"src/main.rs\"\n\n[target.'cfg(unix)'.dependencies]\nshared = { path = \"../lib\" }\n",
        );
        d.file("app/src/main.rs", "fn main() {}\n");
        d.file("app/src/tests.rs", "// a test\n");
        d.file("app/tests/it.rs", "// a test\n");
        d.file("app/target/x", "built\n");
        d.file("other/Cargo.toml", "[package]\nname = \"other\"\n");
        d.file("other/src/main.rs", "fn main() {}\n");
        d
    }

    fn sums(d: &Dir) -> BTreeMap<String, String> {
        let ws = workspace(d.0.path()).unwrap();
        ["main", "shared", "app", "other"]
            .iter()
            .map(|n| (n.to_string(), source_sum(d.0.path(), &ws, n).unwrap()))
            .collect()
    }

    /// What install.sh computed before this verb, for the same files.
    fn shell_sum(root: &Path, files: &[&str]) -> String {
        let list = files
            .iter()
            .map(|f| format!("./{f}"))
            .collect::<Vec<_>>()
            .join("\n");
        let out = Command::new("bash")
            .arg("-c")
            .arg("printf '%s\\n' \"$1\" | tr '\\n' '\\0' | LC_ALL=C sort -z | xargs -0 sha256sum | sha256sum | cut -d' ' -f1")
            .arg("-")
            .arg(list)
            .current_dir(root)
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    #[test]
    fn the_sum_is_the_one_sha256sum_gives_for_the_same_files() {
        let d = tree("sources-shell");
        let ws = workspace(d.0.path()).unwrap();
        assert_eq!(
            source_sum(d.0.path(), &ws, "app").unwrap(),
            shell_sum(
                d.0.path(),
                &[
                    "Cargo.toml",
                    "Cargo.lock",
                    "app/Cargo.toml",
                    "app/src/main.rs",
                    "lib/Cargo.toml",
                    "lib/src/lib.rs"
                ]
            )
        );
        assert_eq!(
            source_sum(d.0.path(), &ws, "main").unwrap(),
            shell_sum(
                d.0.path(),
                &[
                    "Cargo.toml",
                    "Cargo.lock",
                    "src/main.rs",
                    "lib/Cargo.toml",
                    "lib/src/lib.rs"
                ]
            )
        );
    }

    #[test]
    fn the_members_and_their_path_dependencies_come_from_the_manifests() {
        let ws = workspace(tree("sources-members").0.path()).unwrap();
        assert_eq!(
            ws.keys().collect::<Vec<_>>(),
            ["app", "main", "other", "shared"]
        );
        assert_eq!(ws["main"].deps, ["lib"]);
        assert_eq!(ws["app"].deps, ["lib"]);
        assert!(ws["other"].deps.is_empty());
        assert_eq!(ws["main"].dir, "");
    }

    #[test]
    fn a_change_to_a_package_moves_its_sum_and_the_sums_of_those_that_take_it() {
        let d = tree("sources-change");
        let before = sums(&d);
        d.file("lib/src/lib.rs", "// changed\n");
        let after = sums(&d);
        for moved in ["main", "shared", "app"] {
            assert_ne!(before[moved], after[moved], "{moved}");
        }
        assert_eq!(before["other"], after["other"]);

        let before = after;
        d.file("other/src/main.rs", "// changed\n");
        let after = sums(&d);
        assert_ne!(before["other"], after["other"]);
        for kept in ["main", "shared", "app"] {
            assert_eq!(before[kept], after[kept], "{kept}");
        }
    }

    #[test]
    fn the_shared_manifest_and_lock_move_every_sum() {
        let d = tree("sources-lock");
        let before = sums(&d);
        d.file("Cargo.lock", "lock 2\n");
        let after = sums(&d);
        for name in before.keys() {
            assert_ne!(before[name], after[name], "{name}");
        }
    }

    #[test]
    fn tests_builds_and_a_dev_dependency_move_no_sum() {
        let d = tree("sources-tests");
        let before = sums(&d);
        for file in [
            "src/tests.rs",
            "tests/it.rs",
            "tests/new.rs",
            "target/release/main",
            "app/src/tests.rs",
            "app/tests/it.rs",
            "app/target/y",
        ] {
            d.file(file, "// changed\n");
        }
        assert_eq!(sums(&d), before);
        // The root takes app as a dev-dependency only, and none of its files.
        d.file("app/src/main.rs", "// changed\n");
        assert_eq!(sums(&d)["main"], before["main"]);
    }

    #[test]
    fn a_new_file_in_the_roots_own_tree_moves_the_roots_sum_alone() {
        let d = tree("sources-new");
        let before = sums(&d);
        d.file("src/new/mod.rs", "// new\n");
        let after = sums(&d);
        assert_ne!(before["main"], after["main"]);
        for kept in ["shared", "app", "other"] {
            assert_eq!(before[kept], after[kept], "{kept}");
        }
    }

    #[test]
    fn a_link_is_not_followed_and_not_summed() {
        let d = tree("sources-link");
        let before = sums(&d);
        std::os::unix::fs::symlink("/etc", d.0.join("src/etc")).unwrap();
        std::os::unix::fs::symlink("main.rs", d.0.join("src/again.rs")).unwrap();
        assert_eq!(sums(&d), before);
    }

    #[test]
    fn an_unknown_package_and_a_member_without_a_manifest_are_refused() {
        let d = tree("sources-refused");
        let ws = workspace(d.0.path()).unwrap();
        assert!(source_sum(d.0.path(), &ws, "nothing")
            .unwrap_err()
            .contains("no package nothing"));
        fs::remove_file(d.0.join("other/Cargo.toml")).unwrap();
        assert!(workspace(d.0.path())
            .err()
            .unwrap()
            .contains("other/Cargo.toml"));
    }

    #[test]
    fn a_path_outside_the_workspace_or_a_glob_is_refused() {
        let d = tree("sources-outside");
        d.file(
            "other/Cargo.toml",
            "[package]\nname = \"other\"\n[dependencies]\nx = { path = \"../../x\" }\n",
        );
        assert!(workspace(d.0.path())
            .err()
            .unwrap()
            .contains("outside the workspace"));
        d.file(
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/*\"]\n[package]\nname = \"main\"\n",
        );
        assert!(workspace(d.0.path()).err().unwrap().contains("glob"));
    }

    #[test]
    fn the_verb_prints_one_line_per_package_in_the_order_asked() {
        let d = tree("sources-verb");
        let root = d.0.path().to_str().unwrap().to_string();
        assert_eq!(verb(&[root.clone(), "other".into(), "main".into()]), 0);
        assert_eq!(verb(&[root.clone(), "nothing".into()]), 1);
        assert_eq!(verb(&[root]), 64);
        assert_eq!(verb(&[]), 64);
    }

    #[test]
    fn the_tables_a_release_build_follows() {
        for yes in [
            "dependencies",
            "build-dependencies",
            "dependencies.shared",
            "target.'cfg(unix)'.dependencies",
            "target.x86_64-unknown-linux-musl.build-dependencies",
        ] {
            assert!(follows(yes), "{yes}");
        }
        for no in [
            "dev-dependencies",
            "dev-dependencies.x",
            "package",
            "bin",
            "target.x.dev-dependencies",
            "workspace.dependencies",
        ] {
            assert!(!follows(no), "{no}");
        }
        assert_eq!(
            path_deps("[dependencies.shared]\npath = \"../lib\"\n[dependencies]\na = { version = \"1\", path = \"a\" }\nxpath = { git = \"y\" }\n"),
            ["../lib", "a"]
        );
    }

    #[test]
    fn this_workspace_reads_and_every_program_install_sh_builds_is_in_it() {
        let root = &Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let ws = workspace(root).unwrap();
        for prog in [
            "console",
            "analytics",
            "tehas-mcp",
            "store",
            "attach",
            "tehas-dispatch",
            "tehas-cycle",
        ] {
            assert!(ws.contains_key(prog), "{prog}");
        }
        // The attachment checker takes the shared library, as its manifest says.
        assert!(ws["attach"].deps.contains(&"core".to_string()));
        assert_eq!(ws["console"].deps, ["store", "core", "cycle", "ops"]);
    }
}
