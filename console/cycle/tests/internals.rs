//! tehas reads pi-rukas only through the contract its headless runner
//! publishes: the --capabilities answer, the --json result and the stable
//! NDJSON lines (src/progress.rs). Not its state file, its worktree layout,
//! its event log or its source. This test fails when code that is installed
//! or run names one of those again. Tests, the manual in host/docs/ and the
//! records in docs/ may: a test writes a state file to show it is not read,
//! and the manual tells a person where to look.

use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// What a host must not read, as regular expressions.
const INTERNALS: &[&str] = &[
    "extension/src",
    "/opt/pi-rukas/extension",
    "work-state",
    r"\.worktrees",
    "eventLog",
    "pipelineState",
    "currentStep",
    r"\.stateFile",
    "work-driver-",
    r"roles\.ts",
];

/// Naming a path in .git/info/exclude keeps pi-rukas's files out of a commit;
/// it reads nothing, so such a line is not a hit.
const NOT_READ: &str = "info/exclude";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every tracked file that is installed or run.
fn code() -> Vec<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root())
        .args(["ls-files", "--"])
        .args([
            "bin",
            "libexec",
            "lib",
            "host",
            "console/src",
            "console/ops/src",
            "console/core",
            "install.sh",
            "bootstrap.sh",
            "scripts",
            ":!host/docs",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "git ls-files failed");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

/// The lines of a file a pattern finds, each as `<n>:<line>`, but those that
/// name the exclude file.
fn hits(text: &str, re: &Regex) -> Vec<String> {
    text.lines()
        .enumerate()
        .filter(|(_, l)| re.is_match(l) && !l.contains(NOT_READ))
        .map(|(i, l)| format!("{}:{l}", i + 1))
        .collect()
}

#[test]
fn no_code_reads_pirukas_internals() {
    let res: Vec<Regex> = INTERNALS.iter().map(|r| Regex::new(r).unwrap()).collect();
    let mut found = String::new();
    for f in code() {
        // A tracked file gone from the tree reads as nothing.
        let Ok(bytes) = fs::read(root().join(&f)) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        for re in &res {
            let h = hits(&text, re);
            if !h.is_empty() {
                found += &format!("{f}: {}\n", h.join("\n"));
            }
        }
    }
    assert_eq!(found, "");
}

#[test]
fn the_list_covers_the_code_that_is_installed_and_run() {
    let code = code();
    for f in [
        "libexec/setup",
        "libexec/update",
        "host/sandbox/build.sh",
        "host/sandbox/Dockerfile",
        "console/src/render.rs",
        "console/core/src/mask.rs",
    ] {
        assert!(code.iter().any(|c| c.contains(f)), "{f} is not in the list");
    }
}

#[test]
fn each_pattern_finds_what_it_is_for() {
    let sample = r#"jq .pipelineState.status "$w/.pi/work-state/7.json"
cd "$w/.worktrees/issue-7-a"
test -e /opt/pi-rukas/extension/src/work-driver-issue-images.ts
jq ".eventLog[]"
"#;
    for re in INTERNALS {
        if matches!(*re, "currentStep" | r"\.stateFile" | r"roles\.ts") {
            continue;
        }
        assert!(
            Regex::new(re).unwrap().is_match(sample),
            "the pattern {re} finds nothing in the sample"
        );
    }
}
