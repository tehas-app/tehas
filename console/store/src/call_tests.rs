//! `call` against the program: the same line for the same arguments, and the
//! tests' answers file.

use crate::call_with;
use crate::verbs_tests::{home, scratch, SHA};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The program of this build (see `tests::store_binary`).
fn program() -> PathBuf {
    crate::tests::store_binary()
}

/// What the program prints on standard output for `args`, and its status.
fn run(home: &Path, args: &[&str]) -> (i32, String) {
    let out = Command::new(program())
        .args(args)
        .env_clear()
        .env("TEHAS_HOME", home)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
    )
}

fn call(home: &Path, args: &[&str], answers: Option<&Path>) -> Result<String, (u8, String)> {
    call_with(home, args, &mut std::io::empty(), answers)
}

const RECORD: [&str; 12] = [
    "approval-record",
    "7",
    "--at",
    "1700000000",
    "--by",
    "alice",
    "--how",
    "label",
    "--label",
    "tehas:ready",
    "--body-sha",
    SHA,
];

#[test]
fn a_write_and_a_read_answer_the_line_the_program_prints() {
    let ours = home("call-ours");
    let theirs = home("call-theirs");
    for args in [&RECORD[..], &["approval-current", "7"], &["check"]] {
        let line = call(&ours, args, None).unwrap();
        assert_eq!(run(&theirs, args), (0, format!("{line}\n")), "{args:?}");
    }
}

#[test]
fn a_refusal_is_the_line_the_program_prints_on_standard_error() {
    let home = home("call-refused");
    let (rc, line) = call(&home, &["approval-current", "x"], None).unwrap_err();
    assert_eq!(rc, 64);
    assert!(line.starts_with("store: usage approval-current"), "{line}");
    let (rc, line) = call(&home, &["no-such-verb"], None).unwrap_err();
    assert_eq!(rc, 64);
    assert!(line.starts_with("usage: store "), "{line}");
}

#[test]
fn a_home_that_is_no_directory_is_seventy_eight() {
    let (rc, line) = call(Path::new("relative"), &["check"], None).unwrap_err();
    assert_eq!(
        (rc, line.as_str()),
        (78, "store: TEHAS_HOME is not set to a directory")
    );
}

#[test]
fn an_answers_line_refuses_the_call_and_its_extensions_without_running() {
    let home = home("call-answers");
    let dir = scratch("call-answers-file");
    let answers = dir.join("answers");
    fs::write(&answers, "approval-record 7|1|store: busy #7: locked\n").unwrap();
    let no = call(&home, &RECORD, Some(&answers)).unwrap_err();
    assert_eq!(no, (1, "store: busy #7: locked".to_string()));
    // Nothing was written: the store has no approval for #7.
    let current = call(&home, &["approval-current", "7"], Some(&answers)).unwrap();
    assert!(current.contains("\"approval\":null"), "{current}");
    // `approval-record 70` is not `approval-record 7` and a space.
    let other = call(
        &home,
        &["approval-record", "70", "--at", "x"],
        Some(&answers),
    );
    assert_eq!(other.unwrap_err().0, 64);
    // No file is no answers.
    let none = answers.with_file_name("none");
    assert!(call(&home, &["approval-current", "7"], Some(&none)).is_ok());
}

#[test]
fn an_answer_of_zero_is_the_line_and_the_newest_line_answers() {
    let home = home("call-zero");
    let dir = scratch("call-zero-file");
    let answers = dir.join("answers");
    fs::write(
        &answers,
        "check|78|store: corrupt: no\ncheck|0|{\"api\":1,\"verb\":\"check\",\"ok\":true}\n",
    )
    .unwrap();
    assert_eq!(
        call(&home, &["check"], Some(&answers)).unwrap(),
        "{\"api\":1,\"verb\":\"check\",\"ok\":true}"
    );
}

#[test]
fn times_limit_the_answer_and_the_calls_are_counted() {
    let home = home("call-times");
    let dir = scratch("call-times-file");
    let answers = dir.join("answers");
    fs::write(&answers, "check|1|store: busy: held|2\n").unwrap();
    for _ in 0..2 {
        assert_eq!(call(&home, &["check"], Some(&answers)).unwrap_err().0, 1);
    }
    assert!(call(&home, &["check"], Some(&answers)).is_ok());
    let n = fs::read_to_string(answers.with_file_name("answers.n")).unwrap();
    assert_eq!(n.trim(), "3");
}

#[test]
fn a_batch_is_answered_when_any_call_in_it_matches() {
    let home = home("call-batch");
    let dir = scratch("call-batch-file");
    let answers = dir.join("answers");
    fs::write(&answers, "park-show 7|1|store: busy #7: locked\n").unwrap();
    let batch =
        |request: &str| call_with(&home, &["batch"], &mut request.as_bytes(), Some(&answers));
    let no = batch(r#"[["plan-show","5"],["park-show","7"]]"#).unwrap_err();
    assert_eq!(no, (1, "store: busy #7: locked".to_string()));
    let line = batch(r#"[["plan-show","5"],["park-show","8"]]"#).unwrap();
    assert!(line.starts_with("{\"api\":1,\"verb\":\"batch\""), "{line}");
}
