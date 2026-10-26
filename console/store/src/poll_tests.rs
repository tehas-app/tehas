//! poll-record and poll-show.

use crate::poll::{self, count_writes, cpu_ms, secs, started, Record};
use crate::verbs_tests::*;
use serde_json::Value;
use std::fs;
use std::path::Path;
use tehas_core::time::iso;

const TIMES: &str = "0m0.120s 0m0.030s\n0m1.200s 0m0.150s\n";

fn files(home: &Path, tally: usize, log: Option<&str>) -> Record {
    let state = home.join("state");
    fs::write(state.join("poll.times"), TIMES).unwrap();
    fs::write(state.join("poll-gh.tally"), "\n".repeat(tally)).unwrap();
    let writes_log = state.join("github-writes.log");
    if let Some(l) = log {
        fs::write(&writes_log, l).unwrap();
    }
    Record {
        started_ms: 0,
        times: state.join("poll.times"),
        tally: state.join("poll-gh.tally"),
        writes_log,
    }
}

fn line(at: i64, kind: &str) -> String {
    format!("{}\t{kind}\tacme/widget\t7\t0\t-\n", iso(at))
}

fn record(home: &Path, at: i64, r: &Record, started_ms: i64, real_ms: i64) -> Value {
    let r = Record {
        started_ms,
        times: r.times.clone(),
        tally: r.tally.clone(),
        writes_log: r.writes_log.clone(),
    };
    serde_json::from_str(&poll::record(home, &r, at, real_ms).unwrap()).unwrap()
}

#[test]
fn the_arguments_are_strict() {
    assert_eq!(started("1792000000.123456"), Some(1_792_000_000_123));
    assert_eq!(started("1792000000"), Some(1_792_000_000_000));
    assert_eq!(started("1792000000,5"), Some(1_792_000_000_500));
    assert_eq!(started("17920x"), None);
    assert_eq!(started("-1"), None);
    assert!(usage(&["poll-record", "--started", "1"]).contains("--times is required"));
    assert!(usage(&[
        "poll-record",
        "--started",
        "x",
        "--times",
        "/t",
        "--tally",
        "/g",
        "--writes-log",
        "/l"
    ])
    .contains("--started is not a Unix time"));
    assert!(usage(&[
        "poll-record",
        "--started",
        "1",
        "--times",
        "t",
        "--tally",
        "/g",
        "--writes-log",
        "/l"
    ])
    .contains("--times is not an absolute path"));
    assert!(usage(&["poll-show", "--last"]).contains("unknown flag"));
}

#[test]
fn bash_times_sum_to_milliseconds() {
    assert_eq!(cpu_ms(TIMES), Some(1500));
    assert_eq!(cpu_ms("1m2,5s 0m0s\n0m0.001s 0m0.000s\n"), Some(62_501));
    assert_eq!(cpu_ms("0m0.1s 0m0.1s\n"), None);
    assert_eq!(cpu_ms("real 0m0.1s 0m0.1s 0m0.1s"), None);
}

#[test]
fn the_write_log_is_counted_between_the_two_times_by_kind() {
    let log = [
        line(99, "issue edit"),
        line(100, "issue edit"),
        line(100, "pr edit"),
        line(101, "issue comment"),
        line(102, "label create"),
        line(102, "api POST"),
        line(103, "pr merge"),
        "garbled line\n".to_string(),
        line(110, "issue comment"),
    ]
    .concat();
    let w = count_writes(&log, 100, 103);
    assert_eq!((w.label, w.comment, w.edit, w.other), (1, 1, 2, 2));
    assert_eq!(w.total(), 6);
}

#[test]
fn seconds_read_as_the_status_line_says_them() {
    assert_eq!(secs(0), "0.0");
    assert_eq!(secs(3_140), "3.1");
    assert_eq!(secs(9_960), "10");
    assert_eq!(secs(24_400), "24");
    assert_eq!(secs(41_600), "42");
}

#[test]
fn a_poll_records_wall_cpu_calls_and_writes() {
    let home = home("poll-record");
    let real = 1_792_000_100_000;
    let log = [
        line(1_792_000_080, "issue edit"),
        line(1_792_000_095, "issue comment"),
        line(1_792_000_101, "issue edit"),
    ]
    .concat();
    let r = files(&home, 12, Some(&log));
    let v = record(&home, NOW, &r, real - 24_400, real);
    assert_eq!(v["wall_ms"], 24_400);
    assert_eq!(v["cpu_ms"], 1500);
    assert_eq!(v["gh"], 12);
    // The line of 101 is after the end, and one before 075 is not given.
    assert_eq!(v["writes"], 2);
    let s = ok_at(&home, NOW, &["poll-show"]);
    assert_eq!(s["last"]["kinds"]["comment"], 1);
    assert_eq!(s["last"]["kinds"]["edit"], 1);
    assert_eq!(s["last"]["kinds"]["label"], 0);
    assert_eq!(
        s["line"],
        "poll      last 24 s wall, 1.5 s CPU, 12 gh (2 writes); last hour 1 poll, 1.5 s CPU, 2 writes"
    );
}

#[test]
fn no_write_log_is_no_writes_on_record() {
    let home = home("poll-nolog");
    let r = files(&home, 0, None);
    fs::remove_file(&r.tally).unwrap();
    let v = record(&home, NOW, &r, 1_000, 3_000);
    assert_eq!((v["gh"].as_i64(), v["writes"].is_null()), (Some(0), true));
    let s = ok_at(&home, NOW, &["poll-show"]);
    assert_eq!(
        s["line"],
        "poll      last 2.0 s wall, 1.5 s CPU, 0 gh; last hour 1 poll, 1.5 s CPU"
    );
}

#[test]
fn garbled_times_record_nothing() {
    let home = home("poll-garbled");
    let r = files(&home, 1, Some(""));
    fs::write(&r.times, "nothing").unwrap();
    let r2 = Record { started_ms: 0, ..r };
    let (code, why) = poll::record(&home, &r2, NOW, 1_000).unwrap_err();
    assert_eq!(code, 1, "{why}");
    assert!(why.starts_with("garbled"), "{why}");
    assert_eq!(count(&home, "poll_cost"), 0);
}

#[test]
fn the_hour_sums_the_last_hour_and_ignores_older_polls() {
    let home = home("poll-hour");
    let log = line(1_792_000_000, "label create");
    let r = files(&home, 3, Some(&log));
    let real = 1_792_000_001_000;
    record(&home, NOW - 7200, &r, real - 1000, real);
    record(&home, NOW - 3600, &r, real - 1000, real);
    record(&home, NOW - 1800, &r, real - 1000, real);
    fs::write(&r.tally, "\n").unwrap();
    record(&home, NOW - 60, &r, real - 2000, real);
    let s = ok_at(&home, NOW, &["poll-show"]);
    assert_eq!(s["last"]["at"], NOW - 60);
    assert_eq!(s["last"]["gh"], 1);
    assert_eq!(s["hour"]["polls"], 2);
    assert_eq!(s["hour"]["cpu_ms"], 3000);
    assert_eq!(s["hour"]["writes"], 2);
    assert_eq!(
        s["line"],
        "poll      last 2.0 s wall, 1.5 s CPU, 1 gh (1 write); last hour 2 polls, 3.0 s CPU, 2 writes"
    );
    // Pruned with the other records of the poll.
    ok_at(&home, NOW + 2 * 86_400, &["prune", "--days", "1"]);
    assert_eq!(count(&home, "poll_cost"), 0);
}

#[test]
fn no_poll_on_record_is_no_line() {
    let home = home("poll-none");
    let s = ok(&home, &["poll-show"]);
    assert!(s["last"].is_null() && s["line"].is_null(), "{s}");
    assert_eq!(s["hour"]["polls"], 0);
}
