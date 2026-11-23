//! The repository's name: tehas-app/tehas-factory since 2.0, the same without
//! `-factory` before. Nothing tracked names the old one as the repository but
//! the places listed here, each with its reason. Another repository of the
//! family, tehas-app/tehas-<name>, is not the old name. This file spells the
//! old name in two halves, so it is no place of its own.

mod common;

use common::root;
use regex::bytes::Regex;
use std::fs;
use std::process::Command;

/// Where the old name stays, and why. A path ending in `/` is a directory.
const KEPT: [(&str, &str); 10] = [
    ("docs/", "records of what was true when they were written"),
    ("CHANGELOG.md", "the notes of past releases"),
    ("changelog.d/v2-t10-rename.md", "the note of the rename itself"),
    (
        "host/docs/contract.md",
        "Changed in 2.0 names the old name once",
    ),
    ("host/docs/tools-update.md", "the line tehas update says"),
    ("console/core/src/release.rs", "the one rule that knows it"),
    ("console/tests/update_release.rs", "the test of that rule"),
    (
        "test/fixtures/install-0.30/versions.env",
        "what a 0.30.x release left on a host",
    ),
    (
        "test/fixtures/install-0.32/versions.env",
        "what a 0.32.x release left on a host",
    ),
    (
        "instances/tehas-tehas/",
        "a real instance: a person changes it",
    ),
];

/// Whether the old name may stay in this tracked path.
fn kept(path: &str) -> bool {
    KEPT.iter().any(|(kept, _)| path.starts_with(kept))
}

fn tracked() -> Vec<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root())
        .arg("ls-files")
        .output()
        .unwrap();
    assert!(out.status.success(), "git ls-files");
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(String::from)
        .collect()
}

fn old_name() -> Regex {
    Regex::new(&format!("{}/{}([^-A-Za-z0-9_]|$)", "tehas-app", "tehas")).unwrap()
}

#[test]
fn the_old_name_is_the_repository_and_a_longer_name_is_another() {
    let re = old_name();
    for (old, name) in [
        ("tehas-app", "tehas"),
        ("github.com/tehas-app", "tehas.git"),
        ("tehas-app", "tehas/api/x.schema.json"),
        ("tehas-app", "tehas#159"),
        ("(tehas-app", "tehas)"),
        ("\"tehas-app", "tehas\""),
        ("tehas-app", "tehas\nnext"),
    ] {
        let text = format!("{old}/{name}");
        assert!(re.is_match(text.as_bytes()), "{text}");
    }
    for other in ["factory", "industry", "obeya", "plugin-voice", "settings"] {
        let text = format!("tehas-app/tehas-{other}");
        assert!(!re.is_match(text.as_bytes()), "{text}");
    }
}

#[test]
fn an_unreleased_note_is_no_record_and_names_the_new_repository() {
    assert!(kept("changelog.d/v2-t10-rename.md"));
    assert!(kept("CHANGELOG.md"));
    for note in ["changelog.d/fix-x.md", "changelog.d/v2-t10-rename-more.md"] {
        assert!(!kept(note), "{note} becomes the next release's notes");
    }
}

#[test]
fn nothing_names_the_old_repository_outside_the_kept_places() {
    let re = old_name();
    let files = tracked();
    assert!(files.len() > 1000, "git ls-files lists the tree");
    let named: Vec<&String> = files
        .iter()
        .filter(|f| !kept(f))
        .filter(|f| fs::read(root().join(f)).is_ok_and(|bytes| re.is_match(&bytes)))
        .collect();
    assert!(
        named.is_empty(),
        "these name the old repository, tehas-app/tehas-factory is its name: {named:?}"
    );
}
