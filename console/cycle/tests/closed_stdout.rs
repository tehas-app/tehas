//! A program whose stdout's reader has gone ends 141 and says nothing, as a
//! shell command SIGPIPE killed does, not with a panic (#1407).

use std::process::{Command, Stdio};
use tehas_core::testdir::TestDir;

#[test]
fn a_closed_stdout_ends_141_without_a_panic() {
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    let out = Command::new(env!("CARGO_BIN_EXE_tehas-cycle"))
        .args(["--help"])
        .stdin(Stdio::null())
        .stdout(writer)
        .output()
        .unwrap();
    let said = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(141), "tehas-cycle --help: {said}");
    assert!(said.is_empty(), "tehas-cycle --help: {said}");
}

#[test]
fn a_cycle_report_printed_to_a_closed_stdout_ends_141_without_a_panic() {
    // `cycle` pipes the report of an ended cycle into what reads it.
    let d = TestDir::new("cycle-closed-stdout");
    let facts = d.join("facts.json");
    std::fs::write(&facts, r#"{"issue":7,"start":{"attempt":1}}"#).unwrap();
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    let out = Command::new(env!("CARGO_BIN_EXE_tehas-cycle"))
        .args(["report", facts.to_str().unwrap()])
        .stdin(Stdio::null())
        .stdout(writer)
        .output()
        .unwrap();
    let said = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(141), "tehas-cycle report: {said}");
    assert!(said.is_empty(), "tehas-cycle report: {said}");
}
