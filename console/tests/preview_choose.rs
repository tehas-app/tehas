//! `console preview choose` as a process: what it prints and how it exits for
//! a page it cannot read, an empty list and a wrong argument. The rules of the
//! choice are tested beside it, in src/preview/choose_tests.rs.

use std::io::Write;
use std::process::{Command, Stdio};

/// The console's preview choose with these arguments and this standard input:
/// its exit status, standard output and standard error.
fn choose(args: &[&str], given: &str) -> (i32, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_console"))
        .args(["preview", "choose"])
        .args(args)
        .env_remove("TEHAS_HOME")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(given.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).trim_end().to_string(),
        String::from_utf8_lossy(&out.stderr).trim_end().to_string(),
    )
}

#[test]
fn a_page_that_is_not_a_page_chooses_nothing() {
    for given in ["not json at all", r#"{"number":"#, "["] {
        let (rc, out, _) = choose(&["acme/widget", "5"], given);
        assert_eq!(out, "nothing read", "{given}");
        assert_eq!(rc, 0, "{given}");
    }
}

#[test]
fn an_empty_list_chooses_nothing_and_says_nothing() {
    // A page with no pull request in it is a list that was read, which
    // chooses nothing and says nothing. Bytes that are no page at all are a
    // list that cannot be read, which says so.
    // The first is the page of none the run hands on, the others pages as
    // GitHub answers them.
    for given in ["[]\n", "[]\n", "[]\n[]\n"] {
        let (rc, out, _) = choose(&["acme/widget", "5"], given);
        assert_eq!(out, "", "{given:?}");
        assert_eq!(rc, 0, "{given:?}");
    }
    let (_, out, _) = choose(&["acme/widget", "5"], "");
    assert_eq!(out, "nothing read");
}

#[test]
fn a_missing_argument_or_a_max_that_is_not_a_number_is_usage() {
    assert_eq!(
        choose(&[], ""),
        (
            2,
            String::new(),
            "console preview choose: usage: <repo> [max] <pull requests".to_string()
        )
    );
    assert_eq!(
        choose(&["acme/widget", "many"], ""),
        (
            2,
            String::new(),
            "console preview choose: <max> is not a whole number: many".to_string()
        )
    );
}
