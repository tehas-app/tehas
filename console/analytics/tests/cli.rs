//! The program as the bash commands call it: its exit statuses.

use std::process::Command;
use tehas_core::home::{Home, State};
use tehas_core::testdir::TestDir;

const ANALYTICS: &str = env!("CARGO_BIN_EXE_analytics");

/// A home of this test process's own: two runs of the tests at once, as a
/// loaded gate makes, never share one.
fn scratch(name: &str) -> TestDir {
    TestDir::new(&format!("analytics-cli-{name}"))
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
        &["run", "3"],
        &["plan", "3", "--rc", "0"],
        &["event", "3", "shipped"],
        &["report"],
    ] {
        let (code, err) = status(Some(h), args);
        assert_eq!(code, 64, "{args:?}");
        assert!(err.starts_with("usage: analytics"), "{args:?}");
    }
    assert!(!Home::new(home.path()).state_dir().exists());
}

#[test]
fn an_event_makes_the_database_and_a_run_without_history_is_65() {
    let home = scratch("event");
    let h = home.to_str().unwrap();
    assert_eq!(
        status(Some(h), &["event", "3", "ready"]),
        (0, String::new())
    );
    assert!(Home::new(home.path()).state(State::AnalyticsDb).is_file());
    let facts = home.join("facts.json");
    std::fs::write(&facts, r#"{"line":null,"attempt":0}"#).unwrap();
    let f = facts.to_str().unwrap();
    let (code, err) = status(Some(h), &["run", "3", "--facts", f]);
    assert_eq!(code, 65);
    assert_eq!(
        err,
        "analytics: no history of issue 3, so no run to record\n"
    );
    let missing = home.join("missing.json");
    let (code, err) = status(Some(h), &["run", "3", "--facts", missing.to_str().unwrap()]);
    assert_eq!(code, 65);
    assert!(
        err.starts_with("analytics: no facts of the run of issue 3"),
        "{err}"
    );
}
