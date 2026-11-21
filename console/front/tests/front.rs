//! The front as a person runs it, with parts planted on PATH.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FRONT: &str = env!("CARGO_BIN_EXE_tehas-front");

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A part that prints its words, one a line, and exits 5.
fn part(dir: &Path, name: &str) {
    let p = dir.join(name);
    fs::write(&p, "#!/bin/sh\nfor w; do echo \"$w\"; done\nexit 5\n").unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
}

fn tehas(dir: &Path, words: &[&str]) -> Output {
    Command::new(FRONT)
        .args(words)
        .env("PATH", format!("{}:/usr/bin:/bin", dir.display()))
        .output()
        .unwrap()
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn a_part_runs_with_its_words_and_status() {
    let dir = scratch("front-run");
    part(&dir, "tehas-industry");
    let o = tehas(&dir, &["industry", "factory", "list", "--json"]);
    assert_eq!(out(&o), "factory\nlist\n--json\n");
    assert_eq!(o.status.code(), Some(5));
    let o = tehas(&dir, &["help", "industry"]);
    assert_eq!(out(&o), "--help\n");
}

#[test]
fn the_usage_names_the_parts_and_says_when_there_is_none() {
    let dir = scratch("front-usage");
    for words in [&[][..], &["help"], &["--help"], &["help", "help"]] {
        let o = tehas(&dir, words);
        assert_eq!(o.status.code(), Some(0), "{words:?}");
        assert!(
            out(&o).ends_with("No part is installed on this machine.\n"),
            "{}",
            out(&o)
        );
    }
    part(&dir, "tehas-kaizen");
    part(&dir, "tehas-industry");
    let o = tehas(&dir, &[]);
    assert!(
        out(&o).ends_with("Parts on this machine: industry, kaizen\n"),
        "{}",
        out(&o)
    );
}

#[test]
fn a_word_no_part_has_is_64_and_runs_nothing() {
    let dir = scratch("front-unknown");
    part(&dir, "tehas-cycle");
    for words in [
        &["status"][..],
        &["cycle"],
        &["../x"],
        &["--json"],
        &["help", "status"],
    ] {
        let o = tehas(&dir, words);
        assert_eq!(o.status.code(), Some(64), "{words:?}");
        assert_eq!(out(&o), "", "{words:?}");
        let err = String::from_utf8_lossy(&o.stderr);
        assert!(err.contains("tehas: no part named"), "{err}");
    }
}
