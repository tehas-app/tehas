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
        starts: None,
    }
}

fn line(at: i64, kind: &str) -> String {
    format!("{}\t{kind}\tacme/widget\t7\t0\t-\n", iso(at))
}

/// `poll-idle --since <seconds>` as the health check calls it.
fn idle(home: &Path, at: i64, since: &str) -> Value {
    ok_at(home, at, &["poll-idle", "--since", since])
}

/// A record of the poll's own files, with the row's `starts` as given: `now`
/// dates the row and `real_ms` is the clock the wall time and the write log
/// are read against.
fn record(
    home: &Path,
    at: i64,
    r: &Record,
    started_ms: i64,
    real_ms: i64,
    starts: Option<i64>,
) -> Value {
    let r = Record {
        started_ms,
        times: r.times.clone(),
        tally: r.tally.clone(),
        writes_log: r.writes_log.clone(),
        starts,
    };
    serde_json::from_str(&poll::record(home, &r, at, real_ms).unwrap()).unwrap()
}

/// One row written through the verb, which is how `--starts` arrives. The four
/// paths are the ones `files` wrote.
fn record_verb(home: &Path, at: i64, started: &str, starts: &str) -> Value {
    let state = home.join("state");
    let path = |n: &str| state.join(n).to_string_lossy().into_owned();
    ok_at(
        home,
        at,
        &[
            "poll-record",
            "--started",
            started,
            "--times",
            &path("poll.times"),
            "--tally",
            &path("poll-gh.tally"),
            "--writes-log",
            &path("github-writes.log"),
            "--starts",
            starts,
        ],
    )
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
    assert!(usage(&[
        "poll-record",
        "--started",
        "1",
        "--times",
        "/t",
        "--tally",
        "/g",
        "--writes-log",
        "/l",
        "--starts",
        "x"
    ])
    .contains("--starts is not a number of cycles"));
    assert!(usage(&["poll-idle"]).contains("--since is required"));
    assert!(usage(&["poll-idle", "--since", "-1"]).contains("--since is not a number of seconds"));
    assert!(usage(&["poll-idle", "--since", "3600", "--last"]).contains("unknown flag"));
}

#[test]
fn the_median_is_the_middle_figure_or_the_mean_of_the_two() {
    assert_eq!(poll::median(&[]), None);
    assert_eq!(poll::median(&[7]), Some(7));
    assert_eq!(poll::median(&[1, 2, 9]), Some(2));
    assert_eq!(poll::median(&[1, 2, 3, 10]), Some(3));
    // The mean of the two rounds rather than truncating.
    assert_eq!(poll::median(&[1, 2]), Some(2));
    assert_eq!(poll::median(&[2, 3]), Some(3));
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
    let v = record(&home, NOW, &r, real - 24_400, real, None);
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
    let v = record(&home, NOW, &r, 1_000, 3_000, None);
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
    record(&home, NOW - 7200, &r, real - 1000, real, None);
    record(&home, NOW - 3600, &r, real - 1000, real, None);
    record(&home, NOW - 1800, &r, real - 1000, real, None);
    fs::write(&r.tally, "\n").unwrap();
    record(&home, NOW - 60, &r, real - 2000, real, None);
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

#[test]
fn starts_is_on_the_row_and_says_the_cycles_the_poll_started() {
    let home = home("poll-starts");
    let r = files(&home, 2, None);
    let v = record_verb(&home, NOW, &NOW.to_string(), "3");
    assert_eq!(v["starts"], 3);
    assert_eq!(one::<i64>(&home, "SELECT starts FROM poll_cost"), 3);
    // Without the flag the row keeps no answer, which is not the same as 0.
    let v = record(&home, NOW + 1, &r, NOW * 1000 - 1000, NOW * 1000, None);
    assert!(v["starts"].is_null(), "{v}");
    assert_eq!(count(&home, "poll_cost WHERE starts IS NULL"), 1);
}

#[test]
fn poll_idle_gives_the_median_of_the_polls_that_started_nothing() {
    let home = home("poll-idle");
    let r = files(&home, 1, None);
    let real = 1_792_000_010_000;
    // Three idle polls of 10, 20 and 90 s of wall, and one that started a
    // cycle. The rows are written at the verb's own clock, so the poll-idle
    // window is named in that clock too.
    record(&home, NOW - 300, &r, real - 10_000, real, Some(0));
    record(&home, NOW - 240, &r, real - 20_000, real, Some(0));
    record(&home, NOW - 180, &r, real - 90_000, real, Some(0));
    record(&home, NOW - 120, &r, real - 30_000, real, Some(1));
    let v = idle(&home, NOW, "3600");
    assert_eq!(v["polls"], 3);
    assert_eq!(
        (v["wall_ms"].as_i64(), v["cpu_ms"].as_i64()),
        (Some(20_000), Some(1500))
    );
    // No write log on this host, so no poll has a write figure to take a
    // median of, and the verb says so with no figure at all.
    assert!(v["writes"].is_null(), "{v}");
}

#[test]
fn poll_idle_takes_the_mean_of_the_two_middle_rows_of_an_even_number() {
    let home = home("poll-idle-even");
    let r = files(&home, 1, None);
    let real = 1_792_000_010_000;
    for (n, secs) in [(1, 10), (2, 20), (3, 30), (4, 100)] {
        record(&home, NOW - 60 * n, &r, real - secs * 1000, real, Some(0));
    }
    assert_eq!(idle(&home, NOW, "3600")["wall_ms"], 25_000);
}

#[test]
fn poll_idle_counts_the_writes_it_knows_and_leaves_out_the_row_without_them() {
    let home = home("poll-idle-writes");
    // The rows of `record` below are written against `real`, so the log's one
    // line is dated a second before it.
    let log = [line(NOW + 10, "issue comment")].concat();
    let r = files(&home, 1, Some(&log));
    let real = 1_792_000_010_000;
    // Two rows with one write each, and one with the log gone: its writes are
    // null, and its poll is still one of the three.
    record(&home, NOW - 300, &r, real - 1000, real, Some(0));
    record(&home, NOW - 240, &r, real - 2000, real, Some(0));
    fs::remove_file(&r.writes_log).unwrap();
    record(&home, NOW - 180, &r, real - 3000, real, Some(0));
    let v = idle(&home, NOW, "3600");
    assert_eq!(
        (v["polls"].as_i64(), v["writes"].as_i64()),
        (Some(3), Some(1))
    );
}

#[test]
fn no_idle_poll_is_no_polls() {
    // Two stores: one whose rows say otherwise, and one with no poll at all.
    let home = home("poll-idle-none");
    let r = files(&home, 1, None);
    let real = 1_792_000_001_000;
    // One that started a cycle, and one from before the change: neither is idle.
    record(&home, NOW - 60, &r, real - 1000, real, Some(2));
    record(&home, NOW - 30, &r, real - 1000, real, None);
    let v = idle(&home, NOW, "3600");
    assert_eq!(v["polls"], 0);
    assert_eq!(v.as_object().unwrap().len(), 4, "{v}");
    // The same answer with no poll on record at all.
    let empty = crate::verbs_tests::home("poll-idle-empty");
    assert_eq!(idle(&empty, NOW, "3600")["polls"], 0);
}

#[test]
fn poll_idle_looks_only_as_far_back_as_it_was_asked() {
    let home = home("poll-idle-since");
    let r = files(&home, 1, None);
    let real = 1_792_000_010_000;
    // The rows are dated `now`, and the window is named against `now` too.
    record(&home, NOW, &r, real - 4000, real, Some(0));
    record(&home, NOW - 7200, &r, real - 8000, real, Some(0));
    assert_eq!(idle(&home, NOW, "3600")["polls"], 1);
    assert_eq!(idle(&home, NOW, "3600")["wall_ms"], 4000);
    assert_eq!(idle(&home, NOW, "86400")["polls"], 2);
    assert_eq!(idle(&home, NOW, "86400")["wall_ms"], 6000);
    // One dated after the call is not in the window at all, and one dated
    // past a minute ago is outside a window of 60 s.
    assert_eq!(idle(&home, NOW - 30, "3600")["polls"], 0);
    assert_eq!(idle(&home, NOW, "60")["polls"], 1);
    assert_eq!(idle(&home, NOW - 120, "60")["polls"], 0);
}

#[test]
fn poll_idle_prunes_with_the_other_records_of_the_poll() {
    let home = home("poll-idle-prune");
    let r = files(&home, 1, None);
    let real = 1_792_000_001_000;
    record(&home, NOW, &r, real - 1000, real, Some(0));
    assert_eq!(idle(&home, NOW, "3600")["polls"], 1);
    ok_at(&home, NOW + 2 * 86_400, &["prune", "--days", "1"]);
    assert_eq!(idle(&home, NOW + 2 * 86_400, "86400")["polls"], 0);
}
