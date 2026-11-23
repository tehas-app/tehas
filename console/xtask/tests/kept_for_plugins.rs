//! The crates 2.0 keeps with no caller of its own, for the 2.1 plugins
//! (docs/architecture.md, "Kept for the 2.1 plugins"). A dead-code pass of
//! 2.0.x must not take one out: each leaves only with a change of the plugin
//! specification that drops its use.

use std::fs;
use std::path::{Path, PathBuf};

/// Each kept crate, its folder under console/, and what in 2.1 uses it.
const KEPT: [(&str, &str); 8] = [
    (
        "items",
        "the lab plugin's issue feed and page writer (plugin spec 1.4.0, T14)",
    ),
    (
        "attach",
        "the `files:` grant and the plugin mode of attachments (1.4.0, 1.5.1)",
    ),
    (
        "describe",
        "the `given` plugin's image descriptions (1.5.0, section 7)",
    ),
    ("review-render", "the `review` plugin's issue body (1.5.0)"),
    ("wiki-collect", "the `history:` grant's raw sources (1.5.0)"),
    (
        "wiki-render",
        "the `wiki` plugin's check, render and keep rules (1.5.0)",
    ),
    ("mesh-feed", "the host-made `feed:mesh` (1.5.0, section 3)"),
    ("host-feed", "the host-made `feed:host` (1.5.0, section 3)"),
];

fn console() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// The quoted names of the workspace's `key = [...]` line.
fn listed(manifest: &str, key: &str) -> Vec<String> {
    let line = manifest
        .lines()
        .find(|l| l.trim_start().starts_with(&format!("{key} = [")))
        .unwrap_or_else(|| panic!("console/Cargo.toml has no {key} list"));
    line.split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

/// Whether any Rust file under `dir` holds a test.
fn has_tests(dir: &Path) -> bool {
    fs::read_dir(dir).into_iter().flatten().flatten().any(|e| {
        let path = e.path();
        if path.is_dir() {
            return path.file_name().is_some_and(|n| n != "target") && has_tests(&path);
        }
        path.extension().is_some_and(|x| x == "rs")
            && fs::read_to_string(&path).is_ok_and(|t| t.contains("#[test]"))
    })
}

fn why(name: &str, reason: &str) -> String {
    format!(
        "console/{name} is kept for the 2.1 plugins: {reason}. It has no 2.0 caller on \
         purpose (docs/architecture.md, \"Kept for the 2.1 plugins\"). Take it out of \
         KEPT only with the plugin specification change that drops its use"
    )
}

#[test]
fn every_crate_kept_for_the_plugins_stays_a_tested_member_of_the_workspace() {
    let manifest = fs::read_to_string(console().join("Cargo.toml")).unwrap();
    let members = listed(&manifest, "members");
    let defaults = listed(&manifest, "default-members");
    for (name, reason) in KEPT {
        assert!(members.iter().any(|m| m == name), "{}", why(name, reason));
        assert!(defaults.iter().any(|m| m == name), "{}", why(name, reason));
        let dir = console().join(name);
        assert!(dir.join("Cargo.toml").is_file(), "{}", why(name, reason));
        assert!(
            has_tests(&dir),
            "{} (and it has no test)",
            why(name, reason)
        );
    }
}

#[test]
fn the_architecture_page_names_every_kept_crate() {
    let page = fs::read_to_string(console().join("../docs/architecture.md")).unwrap();
    let section = page
        .split("## Kept for the 2.1 plugins")
        .nth(1)
        .expect("docs/architecture.md has no section \"Kept for the 2.1 plugins\"");
    let section = section.split("\n## ").next().unwrap_or("");
    for (name, _) in KEPT {
        assert!(
            section.contains(&format!("`console/{name}`")),
            "docs/architecture.md's section does not name console/{name}"
        );
    }
}
