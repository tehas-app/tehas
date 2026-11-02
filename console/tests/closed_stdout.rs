//! A program whose stdout's reader has gone ends 141 and says nothing, as a
//! shell command SIGPIPE killed does, not with a panic (#1407).

use std::process::{Command, Stdio};

#[test]
fn a_closed_stdout_ends_141_without_a_panic() {
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    let out = Command::new(env!("CARGO_BIN_EXE_console"))
        .args(["shipped", "page"])
        .stdin(Stdio::null())
        .stdout(writer)
        .output()
        .unwrap();
    let said = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(141), "console shipped page: {said}");
    assert!(said.is_empty(), "console shipped page: {said}");
}
