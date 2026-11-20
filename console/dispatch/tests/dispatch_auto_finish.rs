//! The poll finishes a cycle whose push or pull request failed on GitHub's
//! side, from the record the cycle left (console/cycle/tests/cycle_end_failed.rs),
//! with tehas-cycle finish stubbed. The rules of the tries are the Rust tests
//! of src/auto_finish.rs.

mod common;

use common::*;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

fn record(w: &World) -> PathBuf {
    w.cycles_dir().join("7.end-failed.json")
}

fn read_record(w: &World) -> Value {
    serde_json::from_str(&fs::read_to_string(record(w)).unwrap()).unwrap()
}

/// Issue 7 is parked, and its last cycle's `step` failed a minute ago with
/// `why`, GitHub's fault or not, as the host saw it fail when `host`. The
/// finish prints the URL, or the line of finish-says after that of
/// finish-said.
fn stopped(step: &str, github: bool, why: &str, host: bool) -> World {
    let w = World::new("auto-finish");
    w.stub(
        "fake-finish",
        r#"if [ -s "$T/fx/finish-says" ]; then
  if [ -s "$T/fx/finish-said" ]; then
    echo "tehas-cycle: finish: #$2: the command said: $(cat "$T/fx/finish-said")" >&2; fi
  echo "tehas-cycle: finish: #$2: $(cat "$T/fx/finish-says")" >&2; exit 1; fi
echo "https://github.com/acme/widget/pull/90""#,
    );
    w.env("TEHAS_FINISH_CMD", "fake-finish");
    w.slots(1, &[]);
    w.fx("ready.json", "[]");
    w.fx(
        "work-7.json",
        r#"{"number":7,"state":"open","labels":[{"name":"needs-human-attention"}]}"#,
    );
    fs::create_dir_all(w.cycles_dir()).unwrap();
    let at = tehas_core::time::now() - 60;
    let r = json!({"issue": 7, "attempt": 1, "step": step, "github": github, "host": host,
        "why": why, "at": at, "tries": [], "stopped": "", "said": false});
    fs::write(record(&w), format!("{r}\n")).unwrap();
    w
}

/// Eleven minutes pass: the failure and every try are moved back.
fn later(w: &World) {
    let mut r = read_record(w);
    r["at"] = json!(r["at"].as_i64().unwrap() - 660);
    let tries: Vec<Value> = r["tries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| json!(t.as_i64().unwrap() - 660))
        .collect();
    r["tries"] = json!(tries);
    fs::write(record(w), format!("{r}\n")).unwrap();
}

fn finishes(w: &World) -> usize {
    count_lines(&w.calls("fake-finish"), "finish 7 ")
}

#[test]
fn a_push_that_failed_502_once_is_finished_on_the_next_poll() {
    let w = stopped("push", true, "HTTP 502", true);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &w.calls("fake-finish"),
        &format!(
            "finish 7 --home {} --repo acme/widget --default main --reason The cycle stopped because its push failed on GitHub's side (HTTP 502). The poll finished it from the kept workspace on its own, try 1 of 3.",
            w.home.display()
        ),
    );
    has(
        &w.gh(),
        "<!-- tehas:finish issue=7 result=opened by=poll -->",
    );
    has(&w.gh(), "https://github.com/acme/widget/pull/90");
    has(&r.out, "auto-finish #7: opened https://github.com/acme/widget/pull/90, try 1 of 3: its push failed on GitHub's side (HTTP 502)");
    assert!(!record(&w).exists());
    assert_eq!(w.calls("fake-start"), "");
}

#[test]
fn a_pull_request_that_fails_429_three_times_stops_after_3_tries_and_is_left_for_the_ask() {
    let w = stopped("pr", true, "HTTP 429", true);
    w.fx(
        "finish-says",
        "could not open the draft: HTTP 429: Too Many Requests\n",
    );
    let r = w.poll();
    has(
        &r.out,
        "auto-finish #7: try 1 of 3 failed on GitHub's side: could not open the draft: HTTP 429",
    );
    assert_eq!(read_record(&w)["tries"].as_array().unwrap().len(), 1);
    // Too soon for the second.
    w.poll();
    assert_eq!(finishes(&w), 1);
    later(&w);
    w.poll();
    assert_eq!(finishes(&w), 2);
    lacks(&w.gh(), "result=stopped");
    later(&w);
    let r = w.poll();
    assert_eq!(finishes(&w), 3);
    let rec = read_record(&w);
    assert_eq!(rec["tries"].as_array().unwrap().len(), 3);
    assert_eq!(rec["said"], json!(true));
    assert!(rec["stopped"]
        .as_str()
        .unwrap()
        .starts_with("3 tries failed"));
    let stopped_once = |w: &World| {
        w.gh()
            .lines()
            .filter(|l| l.contains("result=stopped by=poll"))
            .count()
    };
    assert_eq!(stopped_once(&w), 1);
    has(&w.gh(), "It waits for `tehas:finish`");
    has(&r.out, "auto-finish #7: stopped: 3 tries failed");
    later(&w);
    w.poll();
    assert_eq!(finishes(&w), 3);
    assert_eq!(stopped_once(&w), 1);
}

#[test]
fn a_push_only_the_cycle_s_own_result_names_is_not_finished_and_nothing_is_said() {
    let w = stopped("push", true, "HTTP 502", false);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.calls("fake-finish"), "");
    lacks(&w.gh(), "tehas:finish issue=7");
    let rec = read_record(&w);
    assert_eq!(
        json!([rec["tries"], rec["stopped"], rec["said"]]),
        json!([[], "", false])
    );
}

#[test]
fn a_refusal_tells_the_issue_the_rule_s_words_and_the_log_what_the_command_said() {
    let w = stopped("push", true, "HTTP 502", true);
    w.fx("finish-says", "the rebase onto origin/main conflicts\n");
    w.fx(
        "finish-said",
        "error: could not apply 1a2b... @alice look here\n",
    );
    let r = w.poll();
    has(
        &w.gh(),
        "its finish was refused: the rebase onto origin/main conflicts",
    );
    lacks(&w.gh(), "@alice");
    has(
        &r.out,
        "the finish of #7 was refused, the command said: error: could not apply 1a2b... @alice look here",
    );
}

#[test]
fn a_422_is_not_retried() {
    let w = stopped("pr", false, "HTTP 422", true);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.calls("fake-finish"), "");
    lacks(&w.gh(), "tehas:finish issue=7");
    let rec = read_record(&w);
    assert_eq!(json!([rec["tries"], rec["stopped"]]), json!([[], ""]));
}

#[test]
fn a_finish_already_asked_for_is_not_run_twice() {
    let w = stopped("push", true, "HTTP 502", true);
    w.fx(
        "work-7.json",
        r#"{"number":7,"state":"open","labels":[{"name":"needs-human-attention"},{"name":"tehas:finish"}]}"#,
    );
    w.fx_json(
        "edit-7.json",
        &json!({"data": {"repository": {"issue": {"lastEditedAt": null, "editor": null,
            "body": "Words.", "title": "Seven"}}}}),
    );
    w.fx_json(
        "events-7.json",
        &json!([{"event": "labeled", "label": {"name": "tehas:finish"}, "actor": {"login": "alice"},
            "created_at": "2026-10-02T00:00:00Z"}]),
    );
    w.fx_json(
        "icomments-7.json",
        &json!([icomment(
            71,
            "alice",
            "2026-10-01T23:59:00Z",
            "2026-10-01T23:59:00Z",
            "<!-- tehas:millwright issue=7 action=finish -->\nGitHub failed the push."
        )]),
    );
    w.past(7, "alice", "label", "2026-10-01T00:00:00Z", &[]);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(finishes(&w), 1);
    has(
        &r.out,
        "finish #7: opened https://github.com/acme/widget/pull/90, asked by alice (label)",
    );
    lacks(&r.out, "auto-finish #7");
    assert!(!record(&w).exists());
}

#[test]
fn an_issue_a_person_took_on_again_has_its_record_removed_and_nothing_run() {
    let w = stopped("push", true, "HTTP 502", true);
    w.fx(
        "work-7.json",
        r#"{"number":7,"state":"closed","labels":[{"name":"needs-human-attention"}]}"#,
    );
    let r = w.poll();
    assert_eq!(w.calls("fake-finish"), "");
    has(&r.out, "auto-finish #7: dropped: the issue is closed");
    assert!(!record(&w).exists());
}
