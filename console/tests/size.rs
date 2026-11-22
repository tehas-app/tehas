//! `console size measure` as a process: its stdin, its environment, its one
//! line. The counts themselves are tested in core/src/size.rs. Moved from
//! test/sizing_test.sh.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tehas_core::testdir::TestDir;

const ZEROS: &str = r#"{"files":0,"tests":0,"deliverables":0,"done_when":0}"#;

/// What the measure prints for `body` on its stdin, with only PATH, HOME and
/// `env` in its environment, run in `dir`: (exit code, stdout and stderr).
fn measure_in(dir: &Path, body: Option<&[u8]>, env: &[(&str, &str)]) -> (i32, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_console"));
    cmd.args(["size", "measure"])
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", dir)
        .envs(env.iter().copied())
        .current_dir(dir)
        .stdin(if body.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    if let Some(body) = body {
        // The measure stops reading at its bound on bytes, so the rest of a
        // long body may meet a closed pipe.
        let _ = child.stdin.take().unwrap().write_all(body);
    }
    let out = child.wait_with_output().unwrap();
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text.trim_end().to_string())
}

fn measure(body: Option<&[u8]>, env: &[(&str, &str)]) -> (i32, String) {
    let dir = TestDir::new("size-measure");
    measure_in(&dir, body, env)
}

/// A body of `n` lines, each naming one test path, as a build log's dump
/// looks to the measure.
fn sized(n: usize) -> Vec<u8> {
    (1..=n)
        .map(|i| format!("`test/f{i}.sh`\n"))
        .collect::<String>()
        .into_bytes()
}

fn counts(files: usize, tests: usize) -> String {
    format!(r#"{{"files":{files},"tests":{tests},"deliverables":0,"done_when":0}}"#)
}

#[test]
fn an_empty_body_is_four_zeros_and_so_is_no_body_at_all() {
    assert_eq!(measure(Some(b""), &[]), (0, ZEROS.to_string()));
    assert_eq!(measure(None, &[]), (0, ZEROS.to_string()));
    assert_eq!(measure(Some(&sized(0)), &[]), (0, ZEROS.to_string()));
}

#[test]
fn the_measure_reads_only_its_stdin_and_writes_nothing() {
    let dir = TestDir::new("size-measure-quiet");
    let tmp = dir.join("tmp");
    fs::create_dir(&tmp).unwrap();
    let (rc, _) = measure_in(&dir, Some(b"x"), &[("TMPDIR", tmp.to_str().unwrap())]);
    assert_eq!(rc, 0);
    assert_eq!(fs::read_dir(&*dir).unwrap().count(), 1);
    assert_eq!(fs::read_dir(&tmp).unwrap().count(), 0);
}

#[test]
fn it_needs_no_setting() {
    assert_eq!(measure(None, &[]), (0, ZEROS.to_string()));
}

#[test]
fn the_answer_is_one_line_of_four_whole_numbers_in_the_order_named() {
    let body = b"## The change\n\n1. `lib/a.sh`\n\n## Done when\n\n- `test/a_test.sh`\n";
    let (rc, out) = measure(Some(body), &[]);
    assert_eq!(rc, 0);
    assert_eq!(out.lines().count(), 1, "{out}");
    assert_eq!(
        out,
        r#"{"files":2,"tests":1,"deliverables":1,"done_when":1}"#
    );
}

/// A person who pastes a dump into an issue body must not delay the start of
/// that cycle.
#[test]
fn a_body_of_forty_thousand_lines_is_measured_in_the_time_a_start_can_wait() {
    let body = sized(40_000);
    let begun = Instant::now();
    assert_eq!(measure(Some(&body), &[]), (0, counts(1000, 1000)));
    assert!(
        begun.elapsed() < Duration::from_secs(5),
        "{:?}",
        begun.elapsed()
    );
}

#[test]
fn the_bound_on_lines_measures_a_body_from_its_front() {
    let body = sized(300);
    assert_eq!(
        measure(Some(&body), &[("TEHAS_SIZE_MAX_LINES", "100")]),
        (0, counts(100, 100))
    );
    assert_eq!(
        measure(Some(&body), &[("TEHAS_SIZE_MAX_LINES", "400")]),
        (0, counts(300, 300))
    );
}

#[test]
fn the_bound_on_bytes_cuts_a_body_of_one_long_line() {
    let body = format!(
        "{} `lib/a.sh` {} `lib/b.sh`\n",
        "x".repeat(400),
        "y".repeat(400)
    );
    let cut = |bytes: usize| {
        measure(
            Some(body.as_bytes()),
            &[("TEHAS_SIZE_MAX_BYTES", &bytes.to_string())],
        )
    };
    assert_eq!(cut(500), (0, counts(1, 0)));
    assert_eq!(cut(1200), (0, counts(2, 0)));
}
