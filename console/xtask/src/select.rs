//! Name the checks a change needs, from the paths it touched. It is the
//! selection behind `scripts/verify.sh --changed`, the gate a developer runs
//! while working. The full gate stays what the driver and CI run.
//!
//! Each path is relative to the root of the checkout. One line per check:
//!
//!   full <why>    the whole gate is needed; it is then the only line
//!   test <file>   a shell test file, test/*_test.sh
//!   node <file>   a front page test file, test/*_test.mjs
//!   cargo         the whole workspace's tests; then there is no crate line
//!   crate <name>  the tests of one crate of the workspace
//!   omit <file>   a heavy file left to the full gate (see below)
//!
//! How a path maps:
//!
//!   - scripts/test.sh, scripts/verify.sh, scripts/select-tests.sh, this crate
//!     and test/lib.sh: full.
//!   - A test file: itself.
//!   - console/: the crate the path is in and the crates that depend on it, and
//!     the shell tests that name console/ or build the store or the attachment
//!     checker (t_store, t_attach). The dependencies are those of `cargo
//!     metadata` on the tree as it stands, plus the list in `RUNS` below.
//!   - cargo instead of crates when the path is in console/core/, is a manifest
//!     or the lock, is in no crate, or cargo cannot say what the crates are.
//!   - A helper under test/ or a fixture under test/fixtures/<name>/: the test
//!     files that name it.
//!   - Any other path: the test files that name its file name as a word, or its
//!     directory when that has a slash in it (host/docs). A helper that names it
//!     brings in the test files that name the helper.
//!   - Nothing names it: no check for docs/ and the top-level *.md files, which
//!     are not installed, and full for anything else.
//!
//! The heavy integration files, test/install_*, test/update_*,
//! test/pack_release_test.sh and test/bootstrap_test.sh, are left to the full
//! gate unless the change touches what they test directly: the file itself, a
//! helper it uses, or a path whose name starts its name (install.sh and
//! host/release/install for install_*, libexec/update for update_*,
//! scripts/pack-release.sh, bootstrap.sh). Reached only through a shared
//! library such as lib/config.sh, they are omitted and named in an omit line.
//!
//! When unsure it asks for more, never less. A test that reaches a file by a
//! name built at run time is missed here; the full gate the driver and CI run
//! still has it.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::process::Command;

/// <crate>:<crate of a program>. The tests of the first run the program of the
/// second, found beside the test binary. No manifest says so, so metadata
/// cannot: a change to the program needs those tests, and those tests need the
/// program built by the same cargo command, or they run a stale one.
/// test/select_test.sh holds this list to the crates that look beside their
/// own executable for a program.
pub const RUNS: &[(&str, &str)] = &[("console", "store"), ("tehas-dispatch", "store")];

/// `cargo metadata` of the tree's workspace. --no-deps needs neither the lock
/// nor the registry, and lists dependencies of every kind.
pub fn cargo_metadata(root: &Path) -> Option<String> {
    let out = Command::new("cargo")
        .args([
            "metadata",
            "--offline",
            "--no-deps",
            "--format-version",
            "1",
        ])
        .args(["--manifest-path", "console/Cargo.toml"])
        .current_dir(root)
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

/// The workspace as metadata gives it: the crate of each directory, and the
/// crates that depend on each crate.
struct Crates {
    at: HashMap<String, String>,
    users: HashMap<String, Vec<String>>,
}

fn read_crates(json: &str) -> Option<Crates> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let root = v.get("workspace_root")?.as_str()?;
    let mut at = HashMap::new();
    let mut known = HashSet::new();
    let mut users: HashMap<String, Vec<String>> = HashMap::new();
    for pkg in v.get("packages")?.as_array()? {
        let name = pkg.get("name")?.as_str()?;
        if name.is_empty() {
            return None;
        }
        let manifest = pkg.get("manifest_path")?.as_str()?;
        let rel = manifest.strip_prefix(root).unwrap_or(manifest);
        let rel = rel.strip_suffix("/Cargo.toml").unwrap_or(rel);
        at.insert(format!("console{rel}"), name.to_string());
        known.insert(name.to_string());
        for dep in pkg.get("dependencies")?.as_array()? {
            if !dep.get("path").is_some_and(|p| !p.is_null() && p != false) {
                continue;
            }
            let d = dep.get("name")?.as_str()?;
            users
                .entry(d.to_string())
                .or_default()
                .push(name.to_string());
        }
    }
    if !at.contains_key("console") {
        return None;
    }
    for (user, program) in RUNS {
        if !known.contains(*user) || !known.contains(*program) {
            return None;
        }
        users
            .entry(program.to_string())
            .or_default()
            .push(user.to_string());
    }
    Some(Crates { at, users })
}

/// The crate a path under console/ is in, or None when only the whole
/// workspace will do. The root package owns console/ itself, so only its two
/// source directories count as in it.
fn crate_of<'a>(crates: &'a Crates, p: &str) -> Option<&'a String> {
    let rest = p.strip_prefix("console/")?;
    if rest.starts_with("core/") || (rest.contains('/') && rest.ends_with("/Cargo.toml")) {
        return None;
    }
    if rest.starts_with("src/") || rest.starts_with("tests/") {
        return crates.at.get("console");
    }
    let (first, _) = rest.split_once('/')?;
    crates.at.get(&format!("console/{first}"))
}

fn want(crates: &Crates, wanted: &mut BTreeSet<String>, name: &str) {
    if !wanted.insert(name.to_string()) {
        return;
    }
    for d in crates.users.get(name).into_iter().flatten() {
        want(crates, wanted, d);
    }
}

fn base_name(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

/// A file name's stem as a test file's name starts with it: up to the first
/// dot, dashes as underscores.
fn stem(base: &str) -> String {
    base.split('.').next().unwrap_or("").replace('-', "_")
}

/// The shell pattern <start>*<end>.
fn between(file: &str, start: &str, end: &str) -> bool {
    file.len() >= start.len() + end.len() && file.starts_with(start) && file.ends_with(end)
}

/// <stem>_test.sh or <stem>_*_test.sh.
fn named_for(file: &str, stem: &str) -> bool {
    file == format!("{stem}_test.sh") || between(file, &format!("{stem}_"), "_test.sh")
}

fn heavy(f: &str) -> bool {
    let b = base_name(f);
    matches!(
        b,
        "update_test.sh" | "pack_release_test.sh" | "bootstrap_test.sh"
    ) || between(b, "install_", "_test.sh")
        || between(b, "update_", "_test.sh")
}

fn find(hay: &[u8], needle: &[u8]) -> Vec<usize> {
    if needle.is_empty() {
        return (0..=hay.len()).collect();
    }
    hay.windows(needle.len())
        .enumerate()
        .filter(|(_, w)| *w == needle)
        .map(|(i, _)| i)
        .collect()
}

fn word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// grep -wF: an occurrence with no word character on either side of it.
fn has_word(hay: &[u8], needle: &[u8]) -> bool {
    find(hay, needle).into_iter().any(|i| {
        let j = i + needle.len();
        (i == 0 || !word_byte(hay[i - 1])) && (j == hay.len() || !word_byte(hay[j]))
    })
}

fn has(hay: &[u8], needle: &str) -> bool {
    !find(hay, needle.as_bytes()).is_empty()
}

/// `tehas <base>` or `tehasbot <base>`, then no [a-z_-] on the line.
fn has_command(hay: &[u8], base: &str) -> bool {
    ["tehas ", "tehasbot "].iter().any(|run| {
        let needle = format!("{run}{base}");
        find(hay, needle.as_bytes())
            .into_iter()
            .any(|i| match hay.get(i + needle.len()) {
                None | Some(b'\n') => true,
                Some(b) => !(b.is_ascii_lowercase() || *b == b'_' || *b == b'-'),
            })
    })
}

/// The test files and the helpers under test/, read once each.
struct Suite<'r> {
    root: &'r Path,
    tests: Vec<String>,
    helpers: Vec<String>,
    text: HashMap<String, Vec<u8>>,
}

impl<'r> Suite<'r> {
    fn new(root: &'r Path) -> Suite<'r> {
        let mut names: Vec<String> = fs::read_dir(root.join("test"))
            .map(|d| {
                d.filter_map(|e| e.ok())
                    .filter(|e| e.path().is_file())
                    .filter_map(|e| e.file_name().into_string().ok())
                    .filter(|n| !n.starts_with('.'))
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        let tests = names
            .iter()
            .filter(|n| n.ends_with("_test.sh") || n.ends_with("_test.mjs"))
            .map(|n| format!("test/{n}"))
            .collect();
        let helpers = names
            .iter()
            .filter(|n| n.ends_with(".sh") && *n != "lib.sh" && !n.ends_with("_test.sh"))
            .map(|n| format!("test/{n}"))
            .collect();
        Suite {
            root,
            tests,
            helpers,
            text: HashMap::new(),
        }
    }

    fn text(&mut self, f: &str) -> &[u8] {
        if !self.text.contains_key(f) {
            let t = fs::read(self.root.join(f)).unwrap_or_default();
            self.text.insert(f.to_string(), t);
        }
        &self.text[f]
    }

    /// The files of <files> that name <p>. A command's bare name is a common
    /// word (install, update, work), so it counts only after its directory
    /// (libexec/update), after the command that runs it (tehas update), or as
    /// the start of a test file's name (update_release_test.sh). A file name
    /// with a dot in it is specific enough to count as a word on its own.
    fn names(&mut self, p: &str, files: &[String]) -> Vec<String> {
        let base = base_name(p);
        let stem = stem(base);
        let parent = p
            .rsplit_once('/')
            .map(|(dir, _)| format!("{}/{base}", base_name(dir)));
        let command = p.starts_with("libexec/") || p.starts_with("bin/");
        let mut hit = Vec::new();
        for f in files {
            let t = self.text(f);
            if (base.contains('.') && has_word(t, base.as_bytes()))
                || parent.as_deref().is_some_and(|n| has(t, n))
                || (command
                    && (has_command(t, base) || has(t, "libexec/\"") || has(t, "libexec/$")))
            {
                hit.push(f.clone());
                continue;
            }
            let name = base_name(f);
            if named_for(name, &stem) || name == format!("{stem}_test.mjs") {
                hit.push(f.clone());
            }
        }
        hit
    }

    /// The test files that name <p>, and those that name a helper that names
    /// it.
    fn naming(&mut self, p: &str) -> Vec<String> {
        let tests = self.tests.clone();
        let helpers = self.helpers.clone();
        let mut hit = self.names(p, &tests);
        for h in self.names(p, &helpers) {
            let word = base_name(&h).to_string();
            for f in &tests {
                if has_word(self.text(f), word.as_bytes()) {
                    hit.push(f.clone());
                }
            }
        }
        hit
    }
}

fn is_gate(p: &str) -> bool {
    matches!(
        p,
        "scripts/test.sh" | "scripts/verify.sh" | "scripts/select-tests.sh" | "test/lib.sh"
    ) || p.starts_with("console/xtask/")
}

/// The lines for <paths>, as the module's head says. <metadata> is asked at
/// most once, for the first path under console/.
pub fn select(
    root: &Path,
    paths: &[String],
    metadata: impl FnOnce() -> Option<String>,
) -> Vec<String> {
    let mut suite = Suite::new(root);
    let mut picked: BTreeSet<String> = BTreeSet::new();
    let mut direct: HashSet<String> = HashSet::new();
    let mut wanted: BTreeSet<String> = BTreeSet::new();
    let mut metadata = Some(metadata);
    let mut crates: Option<Crates> = None;
    let mut cargo = false;
    let full = |why: String| vec![format!("full {why}")];

    for p in paths {
        let p = p.strip_prefix("./").unwrap_or(p);
        if is_gate(p) {
            return full(format!("{p} is part of the gate itself"));
        }
        if p.starts_with("test/") && (p.ends_with("_test.sh") || p.ends_with("_test.mjs")) {
            if root.join(p).is_file() {
                picked.insert(p.to_string());
            }
            direct.insert(p.to_string());
            continue;
        }
        if p.starts_with("console/") {
            if !cargo {
                if let Some(read) = metadata.take() {
                    crates = read().as_deref().and_then(read_crates);
                }
                match crates.as_ref().and_then(|c| crate_of(c, p).map(|n| (c, n))) {
                    Some((c, name)) => want(c, &mut wanted, name),
                    None => cargo = true,
                }
            }
            let tests = suite.tests.clone();
            for f in tests {
                let t = suite.text(&f);
                if has(t, "console/") || has(t, "t_store") || has(t, "t_attach") {
                    picked.insert(f);
                }
            }
            continue;
        }
        let p = match p.strip_prefix("test/fixtures/") {
            Some(rest) => format!("test/fixtures/{}", rest.split('/').next().unwrap_or("")),
            None => p.to_string(),
        };

        // The heavy files whose name starts with the path's, which it tests
        // directly.
        let own = stem(base_name(&p));
        for f in &suite.tests {
            if named_for(base_name(f), &own) {
                direct.insert(f.clone());
            }
        }
        let mut hit = suite.naming(&p);
        if p.starts_with("test/") && p.ends_with(".sh") {
            direct.extend(hit.iter().cloned());
        }
        if let Some((dir, _)) = p.rsplit_once('/') {
            if dir.contains('/') {
                hit.extend(suite.naming(dir));
            }
        }
        if !hit.is_empty() {
            picked.extend(hit);
            continue;
        }
        if p.starts_with("docs/") || (p.ends_with(".md") && !p.contains('/')) {
            continue;
        }
        return full(format!("no test names {p}"));
    }

    let mut lines = Vec::new();
    if cargo {
        lines.push("cargo".to_string());
    } else if !wanted.is_empty() {
        for (user, program) in RUNS {
            if wanted.contains(*user) {
                wanted.insert(program.to_string());
            }
        }
        lines.extend(wanted.iter().map(|c| format!("crate {c}")));
    }
    for f in picked {
        let kind = if heavy(&f) && !direct.contains(&f) {
            "omit"
        } else if f.ends_with(".mjs") {
            "node"
        } else {
            "test"
        };
        lines.push(format!("{kind} {f}"));
    }
    lines
}

#[cfg(test)]
mod tests;
