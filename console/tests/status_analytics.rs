//! The two private files tehasbot status asks the analytics program to write
//! under mcp/ for the MCP server: their modes, when they are asked for, and
//! what happens when analytics is off, missing or failing.

mod common;

use common::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// A gh with nothing to say: no issues, no labels, no releases.
const GH_QUIET: &str = r#"[ ! -e "$T/github-down" ] || exit 1
case "$*" in
  "issue list"*) echo "[]" ;;
  "issue view"*) echo "" ;;
  "api repos/"*) echo "[]" ;;
  "api graphql"*"filterBy: {states: [CLOSED], since: "*) echo "{\"data\":{\"repository\":{\"databaseId\":7001,\"issues\":{\"pageInfo\":{\"hasNextPage\":false,\"endCursor\":null},\"nodes\":[]}}}}" ;;
  "api graphql"*subIssues*) echo "{\"data\":{\"repository\":{\"issues\":{\"nodes\":[]}}}}" ;;
  "api graphql"*) echo "{\"data\":{\"search\":{\"nodes\":[]}}}" ;;
esac"#;

const SESSIONS: &str = "{\"api\":1,\"days\":30,\"generatedAt\":\"2026-10-21T00:00:00Z\",\"sessions\":1}\n{\"id\":\"a\"}\n";

/// The counts file as the analytics program writes it: a header, then one
/// entry a line, counts only.
const OUTCOMES: &str = "{\"api\":1,\"outcomes\":1,\"file\":\"outcomes.jsonl\",\"days\":365,\"by\":\"version\",\"generatedAt\":\"2026-10-21T00:00:00Z\"}\n{\"tehasVersion\":\"v1\",\"runs\":3,\"outcomes\":{\"merged\":3},\"failureKinds\":{},\"parkReasons\":{}}\n";

/// The stub stands for the program: given --out it writes the file named by
/// the option after it, as sessions.rs and outcomes.rs do on a real host.
const ANALYTICS: &str = r#"[ "$1" != sessions ] || { mkdir -p "$3" && chmod 700 "$3" && cat "$T/sessions.jsonl" >"$3/sessions.jsonl"; }
    [ "$1" != outcomes ] || { mkdir -p "$3" && chmod 700 "$3" && cat "$T/outcomes.jsonl" >"$3/outcomes.jsonl"; }"#;

/// A host as this file's reset() had it: the quiet gh, the stub program and
/// no store.
fn world(name: &str) -> World {
    let w = World::new(&format!("analytics-{name}"));
    let libexec = w.home.join("libexec");
    fs::remove_dir_all(&libexec).unwrap();
    fs::create_dir(libexec).unwrap();
    w.gh_stub(GH_QUIET);
    w.stub_analytics(ANALYTICS);
    w.fx("sessions.jsonl", SESSIONS);
    w.fx("outcomes.jsonl", OUTCOMES);
    w
}

fn snap(w: &World) -> PathBuf {
    w.home.join("mcp/sessions.jsonl")
}

fn counts(w: &World) -> PathBuf {
    w.home.join("mcp/outcomes.jsonl")
}

fn read(p: &Path) -> String {
    fs::read_to_string(p).unwrap_or_default()
}

/// The mtime of `p` is `secs` ago.
fn aged(p: &Path, secs: u64) {
    fs::File::options()
        .write(true)
        .open(p)
        .unwrap()
        .set_modified(SystemTime::now() - Duration::from_secs(secs))
        .unwrap();
}

/// The names under `dir`, at any depth, that `pred` takes.
fn found(dir: &Path, pred: &dyn Fn(&str) -> bool) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| {
            let p = e.path();
            let me = usize::from(pred(&e.file_name().to_string_lossy()));
            me + if p.is_dir() { found(&p, pred) } else { 0 }
        })
        .sum()
}

fn news(dir: &Path) -> usize {
    found(dir, &|n| n.ends_with(".new"))
}

fn mode(p: &Path) -> u32 {
    fs::metadata(p).unwrap().permissions().mode() & 0o7777
}

fn mcp_calls(w: &World) -> String {
    let mcp = w.home.join("mcp");
    format!("sessions --out {0}\noutcomes --out {0}\n", mcp.display())
}

#[test]
fn a_run_asks_for_the_snapshot_and_the_counts_and_writes_nothing_under_live() {
    let w = world("calls");
    let r = w.status();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(r.out, "");
    assert_eq!(w.calls("analytics"), mcp_calls(&w));
    assert!(!w.live_file("analytics.json").exists());
    assert_eq!(news(&w.live), 0);
}

#[test]
fn the_files_are_asked_for_after_the_runs_events_so_they_hold_them() {
    let w = world("events");
    w.status();
    w.fx(
        "issues.json",
        r#"[{"number":5,"title":"t","url":"u","state":"OPEN","labels":[{"name":"tehas:awaiting-merge"}],"updatedAt":"2026-01-01T00:00:00Z","closedAt":null}]"#,
    );
    w.gh_stub(
        r#"case "$*" in
  "issue list"*--search*) cat "$T/issues.json" ;;
  "issue list"*) echo "[]" ;;
  "issue view"*) echo "" ;;
  "api repos/"*) echo "[]" ;;
  "api graphql"*"filterBy: {states: [CLOSED], since: "*) echo "{\"data\":{\"repository\":{\"databaseId\":7001,\"issues\":{\"pageInfo\":{\"hasNextPage\":false,\"endCursor\":null},\"nodes\":[]}}}}" ;;
  "api graphql"*subIssues*) echo "{\"data\":{\"repository\":{\"issues\":{\"nodes\":[]}}}}" ;;
  "api graphql"*) echo "{\"data\":{\"search\":{\"nodes\":[]}}}" ;;
esac"#,
    );
    let _ = fs::remove_file(w.t().join("calls/analytics"));
    w.status();
    assert_eq!(
        w.calls("analytics"),
        format!("event 5 awaiting\n{}", mcp_calls(&w))
    );
}

#[test]
fn a_run_that_ends_early_because_github_does_not_answer_still_writes_the_snapshot() {
    let w = world("down");
    w.touch("github-down");
    let r = w.status();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(read(&snap(&w)), SESSIONS);
}

#[test]
fn a_run_that_is_told_to_stop_starts_no_read_and_still_writes_the_snapshot() {
    let w = world("stop");
    w.gh_stub(": >\"$T/gh-started\"\nsleep 1\necho \"[]\"");
    let r = w.sh(&format!(
        r#"{} & p=$!
n=0
until [ -e "$T/gh-started" ] || ! kill -0 $p 2>/dev/null || [ $((n += 1)) -gt 300 ]; do sleep 0.1; done
kill -TERM $p
wait $p"#,
        w.status_cmd()
    ));
    assert_eq!(r.rc, 143, "{}", r.out);
    assert_eq!(w.gh().lines().count(), 1, "{}", w.gh());
    assert!(snap(&w).exists());
    assert!(!w.live_file("status.json").exists());
}

#[test]
fn tehas_analytics_off_asks_the_program_for_nothing() {
    let w = world("off");
    w.status();
    let _ = fs::remove_file(w.t().join("calls/analytics"));
    w.minimal(&["TEHAS_ANALYTICS=off"]);
    let r = w.status();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.calls("analytics"), "");
}

#[test]
fn a_host_without_the_program_has_no_file() {
    let w = world("missing");
    w.status();
    fs::remove_file(w.home.join("libexec/analytics")).unwrap();
    let r = w.status();
    assert_eq!(r.out, "");
    assert!(!snap(&w).exists());
    assert!(!counts(&w).exists());
}

#[test]
fn the_sessions_snapshot_is_private_mode_600_in_a_700_directory_never_under_live() {
    let w = world("snap-private");
    let r = w.status();
    assert_eq!(r.rc, 0, "{}", r.out);
    let s = snap(&w);
    assert_eq!(read(&s), SESSIONS);
    assert_eq!(mode(&s), 0o600);
    assert_eq!(mode(s.parent().unwrap()), 0o700);
    assert_eq!(news(s.parent().unwrap()), 0);
    assert_eq!(found(&w.live, &|n| n.to_lowercase().contains("session")), 0);
}

#[test]
fn the_snapshot_is_written_by_the_program_to_the_directory_status_names() {
    let w = world("snap-program");
    // The stub writes the file it was asked for, by the name it makes from
    // the directory it was given, and nothing else.
    w.stub_analytics(
        r#"[ "$1" != sessions ] || { [ "$2" = --out ] || printf 'STUB %s\n' "sessions without --out" >&2; mkdir -p "$3" && chmod 700 "$3" && cat "$T/sessions.jsonl" >"$3/sessions.jsonl."$$".new"; chmod 600 "$3/sessions.jsonl."$$".new"; mv -f "$3/sessions.jsonl."$$".new" "$3/sessions.jsonl"; }"#,
    );
    let r = w.status();
    assert_eq!(r.out, "");
    let s = snap(&w);
    assert_eq!(read(&s), SESSIONS);
    // The program leaves no name of its run behind.
    assert_eq!(news(s.parent().unwrap()), 0);
}

#[test]
fn tehas_analytics_off_removes_the_snapshot() {
    let w = world("snap-off");
    w.status();
    assert!(snap(&w).exists());
    w.minimal(&["TEHAS_ANALYTICS=off"]);
    w.status();
    assert!(!snap(&w).exists());
}

#[test]
fn a_snapshot_the_program_cannot_write_keeps_a_snapshot_younger_than_ten_minutes_and_removes_an_older_one(
) {
    let w = world("snap-failed");
    w.status();
    let s = snap(&w);
    let keep = read(&s);
    // A program that cannot read its database ends without the file it was
    // asked to write: the print of the last run stands, until it is too old.
    w.stub_analytics(r#"[ "$1" != sessions ] || exit 70"#);
    aged(&s, 540);
    let r = w.status();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "WARN analytics sessions: exit 70");
    assert_eq!(read(&s), keep);
    assert_eq!(news(s.parent().unwrap()), 0);
    aged(&s, 600);
    w.status();
    assert!(!s.exists());
}

#[test]
fn a_failing_sessions_run_does_not_stop_the_counts() {
    let w = world("snap-fails");
    w.stub_analytics(
        r#"[ "$1" != sessions ] || exit 70
  [ "$1" != outcomes ] || { mkdir -p "$3" && chmod 700 "$3" && cat "$T/outcomes.jsonl" >"$3/outcomes.jsonl"; }"#,
    );
    w.status();
    assert!(!snap(&w).exists());
    assert!(counts(&w).exists());
}

// The counts of how the runs ended, further back than the snapshot reaches:
// the same private file, the same keeping.

#[test]
fn the_outcomes_counts_are_private_mode_600_in_a_700_directory_never_under_live() {
    let w = world("counts-private");
    let r = w.status();
    assert_eq!(r.rc, 0, "{}", r.out);
    let c = counts(&w);
    assert_eq!(read(&c), OUTCOMES);
    assert_eq!(mode(&c), 0o600);
    assert_eq!(mode(c.parent().unwrap()), 0o700);
    assert_eq!(news(c.parent().unwrap()), 0);
    assert_eq!(found(&w.live, &|n| n.to_lowercase().contains("outcome")), 0);
}

#[test]
fn the_counts_go_through_the_programs_own_name() {
    let w = world("counts-name");
    w.stub_analytics(
        r#"[ "$1" != outcomes ] || { [ "$2" = --out ] || printf 'STUB %s\n' "outcomes without --out" >&2; mkdir -p "$3" && chmod 700 "$3" && cat "$T/outcomes.jsonl" >"$3/outcomes.jsonl."$$".new"; chmod 600 "$3/outcomes.jsonl."$$".new"; mv -f "$3/outcomes.jsonl."$$".new" "$3/outcomes.jsonl"; }"#,
    );
    let r = w.status();
    assert_eq!(r.out, "");
    let c = counts(&w);
    assert_eq!(read(&c), OUTCOMES);
    // The program leaves no name of its run behind.
    assert_eq!(news(c.parent().unwrap()), 0);
}

/// The whole-year scan is asked for on every poll, so the poll binds it: a
/// publish that runs past the limit is a WARN and an old file kept a while.
#[test]
fn the_outcomes_publish_is_asked_for_under_the_analytics_time_limit() {
    let w = world("counts-limit");
    w.status();
    assert_eq!(count_lines(&w.calls("analytics"), "outcomes --out "), 1);
    w.stub_analytics(
        r#"[ "$1" != outcomes ] || { echo "$*" >"$T/outcomes-argv"; exit 124; }
  [ "$1" != sessions ] || { mkdir -p "$3" && chmod 700 "$3" && cat "$T/sessions.jsonl" >"$3/sessions.jsonl"; }"#,
    );
    aged(&counts(&w), 540);
    let r = w.status();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "WARN analytics outcomes: exit 124");
    assert!(counts(&w).exists());
}

#[test]
fn tehas_analytics_off_removes_the_counts() {
    let w = world("counts-off");
    w.status();
    assert!(counts(&w).exists());
    w.minimal(&["TEHAS_ANALYTICS=off"]);
    w.status();
    assert!(!counts(&w).exists());
}

#[test]
fn a_failed_outcomes_run_keeps_counts_younger_than_ten_minutes_and_removes_older_ones() {
    let w = world("counts-failed");
    w.status();
    let c = counts(&w);
    let keep = read(&c);
    w.stub_analytics(r#"[ "$1" != outcomes ] || exit 70"#);
    aged(&c, 540);
    let r = w.status();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "WARN analytics outcomes: exit 70");
    assert_eq!(read(&c), keep);
    assert_eq!(news(c.parent().unwrap()), 0);
    aged(&c, 600);
    w.status();
    assert!(!c.exists());
}

#[test]
fn a_failing_outcomes_run_does_not_stop_the_snapshot() {
    let w = world("counts-fails");
    w.stub_analytics(
        r#"[ "$1" != outcomes ] || exit 70
  [ "$1" != sessions ] || { mkdir -p "$3" && chmod 700 "$3" && cat "$T/sessions.jsonl" >"$3/sessions.jsonl"; }"#,
    );
    w.status();
    assert!(snap(&w).exists());
    assert!(!counts(&w).exists());
}
