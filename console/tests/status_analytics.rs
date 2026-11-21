//! live/analytics.json, the ten-day report tehasbot status writes from
//! `analytics report --json` for the dashboard, and the two private files it
//! asks the program to write under mcp/ for the MCP server: their keys and
//! their modes, and what happens when analytics is off, missing or failing.

mod common;

use common::*;
use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// A gh with nothing to say: no issues, no labels, no releases.
const GH_QUIET: &str = r#"[ ! -e "$T/github-down" ] || exit 1
case "$*" in
  "issue list"*) echo "[]" ;;
  "issue view"*) echo "" ;;
  "api repos/"*) echo "[]" ;;
  "api graphql"*subIssues*) echo "{\"data\":{\"repository\":{\"issues\":{\"nodes\":[]}}}}" ;;
  "api graphql"*) echo "{\"data\":{\"search\":{\"nodes\":[]}}}" ;;
esac"#;

/// A report with every key the program prints, the optional planned and
/// energy among them, and two it never prints: neither reaches the page.
const REPORT: &str = r#"{"report":1,"from":1792022400,"to":1792843200,"shipped":{"n":3},
 "planned":{"n":6,"inLine":5,"planning":1,"trend":[]},
 "runs":{"n":6,"pr":1,"merged":1,"handoff":1,"parked":0,"failed":2,"refused":1,"stopped":0},
 "planningTurns":{"n":1},"handoffRate":{"n":6,"share":0.167},
 "firstPassYield":{"n":2,"share":0.5},"leadTime":{"n":2,"median":43400,"p90":83600},
 "cycleTime":{"n":6,"median":750,"p90":1800},"machineTime":{"n":6,"seconds":4800,"perShipped":1600},
 "tokens":{"n":4,"total":6650,"input":6100,"output":500,"cacheRead":50,"perShipped":2217},
 "availability":{"n":5,"seconds":734400,"share":0.002},"energy":{"n":4,"kwh":0.075,"perShipped":0.025},
 "daily":[],"stopReasons":{"n":0,"top":[]},"stations":{"n":0,"steps":[]},"versions":[],
 "refine":{"sessions":2,"n":1,"started":1,"shipped":0,"shippedShare":0.0,
  "leadTime":{"n":0},"others":{"n":3,"started":2,"shipped":1,"shippedShare":0.333}},
 "sizes":{"none":{"n":0,"shipped":0,"leadTime":{"n":0}},
  "1-2":{"n":2,"shipped":1,"leadTime":{"n":1,"median":7200}},
  "3-4":{"n":0,"shipped":0,"leadTime":{"n":0}},
  "5+":{"n":1,"shipped":0,"leadTime":{"n":0}}},
 "gate":{"n":3,"seconds":1200,"median":300,"p90":540,"perRun":1.333,"share":0.467},
 "failureCause":"a cause that may not be public","issues":[12,13]}
"#;

/// The report of the snapshot and counts cases.
const SMALL: &str = "{\"report\":1,\"from\":1,\"to\":2}\n";

const KEYS: &str = "availability,cycleTime,daily,energy,firstPassYield,from,handoffRate,leadTime,machineTime,planned,planningTurns,refine,report,runs,shipped,stations,stopReasons,to,tokens,versions";

const SESSIONS: &str = "{\"api\":1,\"days\":30,\"generatedAt\":\"2026-10-21T00:00:00Z\",\"sessions\":1}\n{\"id\":\"a\"}\n";

/// The counts file as the analytics program writes it: a header, then one
/// entry a line, counts only.
const OUTCOMES: &str = "{\"api\":1,\"outcomes\":1,\"file\":\"outcomes.jsonl\",\"days\":365,\"by\":\"version\",\"generatedAt\":\"2026-10-21T00:00:00Z\"}\n{\"tehasVersion\":\"v1\",\"runs\":3,\"outcomes\":{\"merged\":3},\"failureKinds\":{},\"parkReasons\":{}}\n";

/// The stub stands for the program: given --out it writes the file named by
/// the option after it, as sessions.rs and outcomes.rs do on a real host.
const ANALYTICS: &str = r#"[ "$1" != report ] || cat "$T/report.json"
    [ "$1" != sessions ] || { mkdir -p "$3" && chmod 700 "$3" && cat "$T/sessions.jsonl" >"$3/sessions.jsonl"; }
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
    w.fx("report.json", REPORT);
    w.fx("sessions.jsonl", SESSIONS);
    w.fx("outcomes.jsonl", OUTCOMES);
    w
}

fn file(w: &World) -> PathBuf {
    w.live_file("analytics.json")
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

fn json(p: &Path) -> Value {
    serde_json::from_str(&read(p)).unwrap_or(Value::Null)
}

/// The keys of a JSON file, sorted and joined as jq's `keys | join(",")`.
fn keys(p: &Path) -> String {
    let mut k: Vec<String> = json(p)
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    k.sort();
    k.join(",")
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
    format!(
        "report --json\nsessions --out {0}\noutcomes --out {0}\n",
        mcp.display()
    )
}

#[test]
fn the_report_is_written_with_the_keys_that_may_be_published_and_no_other() {
    let w = world("keys");
    let r = w.status();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(r.out, "");
    assert_eq!(w.calls("analytics"), mcp_calls(&w));
    let f = file(&w);
    assert_eq!(keys(&f), KEYS);
    // The size and gate figures belong to the terminal, not the page: the
    // report has the keys, and the publisher's allowlist drops them.
    let report: Value = serde_json::from_str(REPORT).unwrap();
    assert!(report.get("sizes").is_some());
    assert!(json(&f).get("sizes").is_none());
    assert!(report.get("gate").is_some());
    assert!(json(&f).get("gate").is_none());
    let text = read(&f);
    lacks(&text, "may not be public");
    assert_eq!(json(&f)["runs"], report["runs"]);
    assert_eq!(json(&f)["refine"], report["refine"]);
    assert_eq!(text.matches('\n').count(), 1, "{text}");
    assert_eq!(news(&w.live), 0);
    // Readable by the docs server, as status.json is.
    assert_eq!(mode(&f), mode(&w.live_file("status.json")));
}

#[test]
fn the_report_goes_through_names_of_this_run_a_cycles_end_and_a_poll_may_run_status_at_once() {
    let w = world("names");
    w.stub_analytics(
        r#"[ "$1" != report ] || { readlink /proc/$$/fd/1 >"$T/report-to"; cat "$T/report.json"; }"#,
    );
    fs::create_dir_all(&w.live).unwrap();
    let partial = w.live_file("analytics.json.1.raw.new");
    fs::write(&partial, "partial\n").unwrap();
    let r = w.status();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(keys(&file(&w)), KEYS);
    let to = read(&w.t().join("report-to"));
    let to = to.trim_end();
    let prefix = format!("{}/analytics.json.", w.live.display());
    let pid = to
        .strip_prefix(&prefix)
        .and_then(|s| s.strip_suffix(".raw.new"))
        .unwrap_or_else(|| panic!("the report went to {to}"));
    assert!(
        !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()),
        "the report went to {to}"
    );
    assert_eq!(read(&partial).trim_end(), "partial");
}

#[test]
fn the_report_is_asked_for_after_the_runs_events_so_it_holds_them() {
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
fn a_run_that_ends_early_because_github_does_not_answer_still_writes_the_report() {
    let w = world("down");
    w.touch("github-down");
    let r = w.status();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert!(file(&w).exists());
    assert_eq!(json(&file(&w))["report"], 1);
}

#[test]
fn a_run_that_is_told_to_stop_starts_no_read_and_still_writes_the_report() {
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
    assert!(file(&w).exists());
    assert!(!w.live_file("status.json").exists());
}

#[test]
fn tehas_analytics_off_removes_the_file_and_asks_for_no_report() {
    let w = world("off");
    w.status();
    assert!(file(&w).exists());
    let _ = fs::remove_file(w.t().join("calls/analytics"));
    w.minimal(&["TEHAS_ANALYTICS=off"]);
    let r = w.status();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert!(!file(&w).exists());
    assert_eq!(w.calls("analytics"), "");
}

#[test]
fn a_host_without_the_program_has_no_file() {
    let w = world("missing");
    w.status();
    fs::remove_file(w.home.join("libexec/analytics")).unwrap();
    let r = w.status();
    assert_eq!(r.out, "");
    assert!(!file(&w).exists());
}

#[test]
fn a_failed_report_keeps_a_file_younger_than_ten_minutes_and_removes_an_older_one() {
    let w = world("failed");
    w.status();
    let keep = read(&file(&w));
    w.stub_analytics(r#"[ "$1" != report ] || { echo "{\"report\":1,\"half"; exit 70; }"#);
    aged(&file(&w), 540);
    let r = w.status();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "WARN analytics report: exit 70");
    assert_eq!(read(&file(&w)), keep);
    assert_eq!(news(&w.live), 0);
    aged(&file(&w), 600);
    w.status();
    assert!(!file(&w).exists());
}

#[test]
fn a_report_that_is_not_one_is_a_warning_and_is_not_published() {
    let w = world("not-one");
    w.status();
    for printed in ["not json", r#"{"report":2}"#, "[1,2]", ""] {
        w.fx("report.json", &format!("{printed}\n"));
        aged(&file(&w), 60);
        let r = w.status();
        assert_eq!(r.rc, 0, "{}", r.out);
        has(&r.out, "WARN analytics report: not a report");
        assert_eq!(keys(&file(&w)), KEYS, "after {printed:?}");
    }
    aged(&file(&w), 601);
    w.status();
    assert!(!file(&w).exists());
}

#[test]
fn a_report_that_hangs_costs_one_time_limit() {
    let w = world("hangs");
    w.stub_analytics(r#"[ "$1" != report ] || exec sleep 30"#);
    w.set_env(&["TEHAS_ANALYTICS_WAIT=1"]);
    let before = Instant::now();
    let r = w.status();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert!(before.elapsed() < Duration::from_secs(5));
    has(&r.out, "WARN analytics report: exit 124");
    assert!(!file(&w).exists());
}

/// The world of the snapshot and counts cases, with the small report.
fn private(name: &str) -> World {
    let w = world(name);
    w.fx("report.json", SMALL);
    w
}

#[test]
fn the_sessions_snapshot_is_private_mode_600_in_a_700_directory_never_under_live() {
    let w = private("snap-private");
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
    let w = private("snap-program");
    // The stub writes the file it was asked for, by the name it makes from
    // the directory it was given, and nothing else.
    w.stub_analytics(
        r#"[ "$1" != report ] || cat "$T/report.json"
  [ "$1" != sessions ] || { [ "$2" = --out ] || printf 'STUB %s\n' "sessions without --out" >&2; mkdir -p "$3" && chmod 700 "$3" && cat "$T/sessions.jsonl" >"$3/sessions.jsonl."$$".new"; chmod 600 "$3/sessions.jsonl."$$".new"; mv -f "$3/sessions.jsonl."$$".new" "$3/sessions.jsonl"; }"#,
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
    let w = private("snap-off");
    w.status();
    assert!(snap(&w).exists());
    w.minimal(&["TEHAS_ANALYTICS=off"]);
    w.status();
    assert!(!snap(&w).exists());
}

#[test]
fn a_snapshot_the_program_cannot_write_keeps_a_snapshot_younger_than_ten_minutes_and_removes_an_older_one(
) {
    let w = private("snap-failed");
    w.status();
    let s = snap(&w);
    let keep = read(&s);
    // A program that cannot read its database ends without the file it was
    // asked to write: the print of the last run stands, until it is too old.
    w.stub_analytics(
        r#"[ "$1" != sessions ] || exit 70
  [ "$1" != report ] || cat "$T/report.json""#,
    );
    aged(&s, 540);
    let r = w.status();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "WARN analytics sessions: exit 70");
    assert_eq!(read(&s), keep);
    assert_eq!(news(s.parent().unwrap()), 0);
    aged(&s, 600);
    w.status();
    assert!(!s.exists());
    assert!(file(&w).exists());
}

#[test]
fn a_failing_sessions_run_does_not_stop_analytics_json_or_the_counts() {
    let w = private("snap-fails");
    w.stub_analytics(
        r#"[ "$1" != sessions ] || exit 70
  [ "$1" != report ] || cat "$T/report.json"
  [ "$1" != outcomes ] || { mkdir -p "$3" && chmod 700 "$3" && cat "$T/outcomes.jsonl" >"$3/outcomes.jsonl"; }"#,
    );
    w.status();
    assert_eq!(json(&file(&w))["report"], 1);
    assert!(!snap(&w).exists());
    assert!(counts(&w).exists());
}

// The counts of how the runs ended, further back than the snapshot reaches:
// the same private file, the same keeping.

#[test]
fn the_outcomes_counts_are_private_mode_600_in_a_700_directory_never_under_live() {
    let w = private("counts-private");
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
    let w = private("counts-name");
    w.stub_analytics(
        r#"[ "$1" != report ] || cat "$T/report.json"
  [ "$1" != outcomes ] || { [ "$2" = --out ] || printf 'STUB %s\n' "outcomes without --out" >&2; mkdir -p "$3" && chmod 700 "$3" && cat "$T/outcomes.jsonl" >"$3/outcomes.jsonl."$$".new"; chmod 600 "$3/outcomes.jsonl."$$".new"; mv -f "$3/outcomes.jsonl."$$".new" "$3/outcomes.jsonl"; }"#,
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
    let w = private("counts-limit");
    w.status();
    assert_eq!(count_lines(&w.calls("analytics"), "outcomes --out "), 1);
    w.stub_analytics(
        r#"[ "$1" != outcomes ] || { echo "$*" >"$T/outcomes-argv"; exit 124; }
  [ "$1" != report ] || cat "$T/report.json"
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
    let w = private("counts-off");
    w.status();
    assert!(counts(&w).exists());
    w.minimal(&["TEHAS_ANALYTICS=off"]);
    w.status();
    assert!(!counts(&w).exists());
}

#[test]
fn a_failed_outcomes_run_keeps_counts_younger_than_ten_minutes_and_removes_older_ones() {
    let w = private("counts-failed");
    w.status();
    let c = counts(&w);
    let keep = read(&c);
    w.stub_analytics(
        r#"[ "$1" != outcomes ] || exit 70
  [ "$1" != report ] || cat "$T/report.json""#,
    );
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
fn a_failing_outcomes_run_does_not_stop_the_snapshot_or_analytics_json() {
    let w = private("counts-fails");
    w.stub_analytics(
        r#"[ "$1" != outcomes ] || exit 70
  [ "$1" != report ] || cat "$T/report.json"
  [ "$1" != sessions ] || { mkdir -p "$3" && chmod 700 "$3" && cat "$T/sessions.jsonl" >"$3/sessions.jsonl"; }"#,
    );
    w.status();
    assert_eq!(json(&file(&w))["report"], 1);
    assert!(snap(&w).exists());
    assert!(!counts(&w).exists());
}
