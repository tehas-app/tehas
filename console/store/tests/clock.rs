//! The test clock: a build with debug assertions takes its time from
//! TEHAS_STORE_TEST_NOW, so the shell tests can say when a cycle started
//! without touching a file's mtime. A release build never reads it, and this
//! file is not built for one.

#![cfg(debug_assertions)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const STORE: &str = env!("CARGO_BIN_EXE_store");

fn store(home: &Path, now: Option<&str>, args: &[&str]) -> Output {
    let mut cmd = Command::new(STORE);
    cmd.args(args).env_clear().env("TEHAS_HOME", home);
    if let Some(t) = now {
        cmd.env("TEHAS_STORE_TEST_NOW", t);
    }
    cmd.output().unwrap()
}

fn home(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("clock-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const INIT: [&str; 9] = [
    "init",
    "--version",
    "v0.11.0",
    "--commit",
    "cb6912a",
    "--instance",
    "tehas-tehas",
    "--repo",
    "example-owner/example-product",
];

fn seen_at(out: &Output) -> i64 {
    assert!(out.status.success(), "{out:?}");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v["lock"]["seen_at"].as_i64().unwrap()
}

#[test]
fn a_debug_build_reads_its_clock_from_the_variable() {
    let home = home("read");
    assert!(store(&home, Some("1700000123"), &INIT).status.success());
    let c = rusqlite::Connection::open(home.join("state/tehas.db")).unwrap();
    let made: String = c
        .query_row("SELECT value FROM meta WHERE key = 'created_at'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(made, "1700000123");
    let out = store(&home, Some("1700000456"), &["cycle-show", "1"]);
    assert_eq!(seen_at(&out), 1_700_000_456);
    // Without it, the real clock.
    let out = store(&home, None, &["cycle-show", "1"]);
    assert!(seen_at(&out) > 1_790_000_000);
}

#[test]
fn a_clock_that_is_not_a_unix_time_stops_the_command() {
    let home = home("bad");
    assert!(store(&home, None, &INIT).status.success());
    for bad in ["", "soon", "-1", "0"] {
        let out = store(&home, Some(bad), &["cycle-show", "1"]);
        assert_eq!(out.status.code(), Some(78), "{bad:?}");
        assert!(out.stdout.is_empty());
        let err = String::from_utf8_lossy(&out.stderr);
        assert_eq!(
            err.trim_end(),
            "store: TEHAS_STORE_TEST_NOW is not a Unix time",
            "{bad:?}"
        );
    }
}
