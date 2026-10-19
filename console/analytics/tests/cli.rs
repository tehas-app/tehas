//! The program as the bash commands call it: its exit statuses.

use std::path::PathBuf;
use std::process::Command;

const ANALYTICS: &str = env!("CARGO_BIN_EXE_analytics");

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn status(home: Option<&str>, args: &[&str]) -> (i32, String) {
    let mut cmd = Command::new(ANALYTICS);
    cmd.args(args).env_remove("TEHAS_HOME");
    if let Some(h) = home {
        cmd.env("TEHAS_HOME", h);
    }
    let out = cmd.output().unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn without_tehas_home_it_stops_with_78_and_writes_nothing() {
    let (code, err) = status(None, &["prune"]);
    assert_eq!(code, 78);
    assert_eq!(err, "analytics: TEHAS_HOME is not set to a directory\n");
    let dir = scratch("relative");
    let missing = dir.join("missing");
    assert_eq!(status(Some(missing.to_str().unwrap()), &["prune"]).0, 78);
    assert!(!missing.exists());
    assert_eq!(status(Some("relative/home"), &["prune"]).0, 78);
}

#[test]
fn a_wrong_argument_is_a_usage_error() {
    let home = scratch("usage");
    let h = home.to_str().unwrap();
    for args in [
        &[][..],
        &["run"],
        &["event", "3", "shipped"],
        &["report", "--text"],
    ] {
        let (code, err) = status(Some(h), args);
        assert_eq!(code, 64, "{args:?}");
        assert!(err.starts_with("usage: analytics"), "{args:?}");
    }
    assert!(!home.join("state").exists());
}

#[test]
fn an_event_makes_the_database_and_a_run_without_history_is_65() {
    let home = scratch("event");
    let h = home.to_str().unwrap();
    assert_eq!(
        status(Some(h), &["event", "3", "ready"]),
        (0, String::new())
    );
    assert!(home.join("state/analytics.db").is_file());
    let (code, err) = status(Some(h), &["run", "3"]);
    assert_eq!(code, 65);
    assert_eq!(
        err,
        "analytics: no history of issue 3, so no run to record\n"
    );
}

#[test]
fn report_prints_one_line_of_json_with_its_version() {
    let home = scratch("report");
    let out = Command::new(ANALYTICS)
        .args(["report", "--json"])
        .env("TEHAS_HOME", &home)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stderr.is_empty());
    let text = String::from_utf8(out.stdout).unwrap();
    assert_eq!(text.lines().count(), 1);
    assert!(
        text.starts_with('{') && text.contains("\"report\":1"),
        "{text}"
    );
}
