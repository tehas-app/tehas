//! `tehas <part>` as bin/tehas runs it: `console front` finds the part's
//! program on PATH and runs it with the words, in place of itself.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const CONSOLE: &str = env!("CARGO_BIN_EXE_console");

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    for d in ["home/libexec", "home/bin", "parts"] {
        fs::create_dir_all(dir.join(d)).unwrap();
    }
    dir
}

/// A part that prints its name and words, one a line, and exits 7.
fn part(dir: &Path, name: &str, says: &str) {
    let p = dir.join(name);
    fs::write(
        &p,
        format!("#!/bin/sh\necho {says}\nfor w; do echo \"$w\"; done\nexit 7\n"),
    )
    .unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
}

fn tehas(dir: &Path, words: &[&str]) -> Output {
    let home = dir.join("home");
    let path = format!(
        "{}:{}:{}:/usr/bin:/bin",
        home.join("libexec").display(),
        home.join("bin").display(),
        dir.join("parts").display()
    );
    Command::new(CONSOLE)
        .arg("front")
        .arg(home.join("libexec"))
        .args(["1000", "/bin/bash", "tehas"])
        .args(words)
        .env("PATH", path)
        .output()
        .unwrap()
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn a_part_runs_with_the_words_and_its_status_is_tehas_status() {
    let dir = scratch("parts-run");
    part(&dir.join("parts"), "tehas-industry", "industry");
    let o = tehas(&dir, &["industry", "status", "--json", "a b"]);
    assert_eq!(out(&o), "industry\nstatus\n--json\na b\n");
    assert_eq!(o.status.code(), Some(7));
}

#[test]
fn help_and_a_help_flag_go_to_the_part() {
    let dir = scratch("parts-help");
    part(&dir.join("parts"), "tehas-kaizen", "kaizen");
    for words in [&["help", "kaizen"][..], &["kaizen", "--help"]] {
        let o = tehas(&dir, words);
        assert_eq!(out(&o), "kaizen\n--help\n", "{words:?}");
    }
}

#[test]
fn tehas_own_programs_and_directories_are_not_parts() {
    let dir = scratch("parts-own");
    let home = dir.join("home");
    part(&home.join("libexec"), "tehas-industry", "libexec");
    part(&home.join("bin"), "tehas-kata", "bin");
    part(&dir.join("parts"), "tehas-cycle", "cycle");
    for word in ["industry", "kata", "cycle"] {
        let o = tehas(&dir, &[word]);
        assert_eq!(o.status.code(), Some(64), "{word}");
        assert_eq!(out(&o), "", "{word}");
        let err = String::from_utf8_lossy(&o.stderr);
        assert!(
            err.contains(&format!("tehas: unknown command: {word}")),
            "{err}"
        );
    }
}

#[test]
fn a_command_of_tehas_wins_over_a_part_of_its_name() {
    let dir = scratch("parts-verb");
    part(&dir.join("parts"), "tehas-help", "part");
    part(&dir.join("parts"), "tehas-status", "part");
    let o = tehas(&dir, &["help"]);
    assert!(
        out(&o).starts_with("tehas: the command a person types"),
        "{}",
        out(&o)
    );
    let o = tehas(&dir, &["help", "status"]);
    assert!(!out(&o).starts_with("part"), "{}", out(&o));
}

#[test]
fn the_usage_names_the_parts_this_machine_has() {
    let dir = scratch("parts-usage");
    let o = tehas(&dir, &[]);
    assert!(!out(&o).contains("Parts on this machine"), "{}", out(&o));
    part(&dir.join("parts"), "tehas-industry", "industry");
    part(&dir.join("parts"), "tehas-kaizen", "kaizen");
    let o = tehas(&dir, &[]);
    assert!(
        out(&o).ends_with("\nParts on this machine: industry, kaizen\n"),
        "{}",
        out(&o)
    );
}
