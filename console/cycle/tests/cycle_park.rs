//! A park by a provider outage or a failure, the marker of a park, the park
//! question, and what is reduced before it is posted.

mod common;

use common::{has, lacks, World};
use regex::Regex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

const OUTAGE: &str = r#"{"kind":"dispatch-failed-provider","step":"adversarial","at":2,"providerMessage":"Connection error."}"#;
const LOG: &str = r#"echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl""#;
/// 2026-01-01T00:00:00Z.
const NEW_YEAR: u64 = 1_767_225_600;
/// 2026-10-24T12:00:00Z.
const DECIDED: u64 = 1_792_843_200;

/// A handoff at `cap`, and the failure the runner named before it, which
/// tehas work wrote into the progress as a schema 5 run does.
fn capped(w: &World, cap: &str, failure: &str) {
    w.ended("handoff", "handoff", "feature/issue-7-x", "");
    w.entry(|e| e["cap"] = json!(cap));
    let f: Value = serde_json::from_str(failure).unwrap();
    let reason = f["providerMessage"]
        .as_str()
        .or(f["errorTail"].as_str())
        .unwrap_or("");
    let kind = match f["kind"].as_str() {
        Some("dispatch-failed-provider") => json!("provider"),
        _ => f["kind"].clone(),
    };
    let mut fail = serde_json::Map::new();
    for (k, v) in [
        ("kind", kind),
        ("step", f["step"].clone()),
        ("role", f["role"].clone()),
        ("cause", f["cause"].clone()),
    ] {
        if !v.is_null() {
            fail.insert(k.into(), v);
        }
    }
    w.progress(json!({"roles": [], "reason": reason, "failure": fail,
        "from": {"roles": "result", "failure": "result"}}));
}

/// The unit's start, on alice's approval.
fn from_unit(w: &mut World, how: &str, at: u64, extra: &[&str]) {
    w.env("TEHAS_CYCLE_FROM", "unit");
    w.approve("alice", how, at, extra);
}

/// The park's fields named, as one array.
fn park_of(w: &World, keys: &[&str]) -> Value {
    let p = w.park();
    Value::Array(keys.iter().map(|k| p[*k].clone()).collect())
}

fn history_last(w: &World) -> Value {
    let rows = w.row(&["history-show", "7"])["rows"].clone();
    rows.as_array()
        .unwrap()
        .last()
        .cloned()
        .unwrap_or(Value::Null)
}

fn count(text: &str, needle: &str) -> usize {
    text.lines().filter(|l| l.contains(needle)).count()
}

fn starts(text: &str, prefix: &str) -> usize {
    text.lines().filter(|l| l.starts_with(prefix)).count()
}

/// The edits of the issue.
fn edits(w: &World) -> String {
    w.gh()
        .lines()
        .filter(|l| l.starts_with("issue edit"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A handoff no pull request carries.
fn handed_off(w: &World, cap: &str, failure: &str) {
    capped(w, cap, failure);
    w.entry(|e| {
        e["handoff"] = json!({"channel": "host",
            "body": "<!-- pi-rukas:handoff issue=7 cap=fix-pass-park -->\n## Cap hit\nThe branch is consolidated locally, not pushed."})
    });
}

/// The failure of a schema 5 run as tehas work writes it into the progress.
fn failed(w: &World, cap: &str, kind: &str, reason: &str) {
    w.ended("handoff", "handoff", "feature/issue-7-x", "");
    w.entry(|e| e["cap"] = json!(cap));
    w.progress(json!({"roles": [], "reason": reason,
        "failure": {"kind": kind, "step": "develop", "role": "developer", "cause": ""},
        "from": {"roles": "result", "failure": "result"}}));
}

/// A park whose failure the result gives: step develop, its kind and cause.
fn park_failed(w: &World, cap: &str, reason: &str) {
    w.ended("handoff", "develop", "feature/x", cap);
    w.entry(|e| e["parkReason"] = json!(reason));
    w.fx(
        "progress.json",
        r#"{"roles":[],"reason":"3 tests failed","failure":{"kind":"verify-failed","step":"develop","role":"developer","cause":"exit-1"},"from":{"roles":"result","failure":"result"}}"#,
    );
}

fn park_markers_in(text: &str) -> Vec<String> {
    text.lines()
        .filter(|l| l.starts_with("<!-- tehas:park "))
        .map(str::to_string)
        .collect()
}

fn park_markers(w: &World) -> Vec<String> {
    park_markers_in(&w.gh())
}

fn park_sig(w: &World) -> String {
    let re = Regex::new(r" sig=([0-9a-f]+) -->$").unwrap();
    park_markers(w)
        .iter()
        .filter_map(|m| re.captures(m).map(|c| c[1].to_string()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn sha(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

/// The sig of the park in the history line, from its fields as recorded.
fn history_sig(w: &World) -> String {
    let r = history_last(w);
    let f: Vec<String> = [
        "step",
        "cap",
        "park_reason",
        "failure_kind",
        "failure_cause",
    ]
    .iter()
    .map(|k| match &r[*k] {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        v => v.to_string(),
    })
    .collect();
    sha(&f.join("|"))
}

/// A park of the issue's previous end, left on the store's history.
fn parked_end(w: &World, step: &str, cap: &str, kind: &str, cause: &str) {
    w.t_ended(
        "alice",
        "label",
        &[
            "--status",
            "failed",
            "--rc",
            "10",
            "--step",
            step,
            "--cap",
            cap,
            "--failure-kind",
            kind,
            "--failure-cause",
            cause,
            "--parked",
        ],
    );
}

/// The comment markers posted, less a park's own marker in its exact form.
fn markers(w: &World) -> usize {
    let own = Regex::new(
        r"^<!-- tehas:park issue=7 step=[a-z0-9-]{0,40} kind=[a-z0-9-]{0,40} provider=[01] sig=[0-9a-f]{64} -->$",
    )
    .unwrap();
    w.gh()
        .lines()
        .filter(|l| !own.is_match(l))
        .filter(|l| l.contains("<!--"))
        .count()
}

fn boxes(w: &World) -> usize {
    starts(&w.gh(), "- [")
}

/// The usage numbers come from files too: a string there must not reach a
/// comment.
fn sly_usage(w: &World) {
    w.ended("handoff", "commit-pr", "feature/x", "");
    let sly = "1\n- [x] Retry from a clean start\n<!-- tehas:question kind=park issue=7 turn=1 -->";
    w.progress(json!({"reviewHeld": false, "roles": [
        {"role": "planner", "dispatches": 2, "turns": sly, "input": 7, "output": null, "cacheRead": {"a": 1}},
        {"role": "coder", "dispatches": 1, "turns": 2, "input": 3, "output": 4, "cacheRead": 5}]}));
    w.entry(|e| {
        e["usage"] = json!({"dispatches": 3, "turns": 7, "input": 10, "output": 4, "cacheRead": 5})
    });
}

/// A runner that refused to start (tehas work exit 4).
fn never_ran(w: &mut World, approved: bool) {
    w.fx("rc", "4\n");
    if approved {
        from_unit(w, "label", NEW_YEAR, &[]);
    }
}

/// tehas work refused a lean start (78).
fn lean_refused(w: &mut World) {
    w.fx("rc", "78\n");
    w.work(r#": >"$TEHAS_HOME/state/cycles/7.lean-refused""#);
    let input = w.t().join("lean-input.json");
    fs::write(&input, r#"{"links":[],"driver":"lean"}"#).unwrap();
    let input = input.to_string_lossy().to_string();
    from_unit(w, "label", NEW_YEAR, &["--input-file", &input]);
}

/// The question of #7's current park, "none" without one. The id is a
/// number, which `World::park_q` reads as none.
fn park_q(w: &World) -> String {
    match &w.park()["question"] {
        Value::Null => "none".into(),
        Value::String(s) => s.clone(),
        v => v.to_string(),
    }
}

fn approval(w: &World) -> Value {
    w.row(&["approval-current", "7"])
}

// --- A cap that a provider outage gives ---------------------------------------

#[test]
fn a_step_that_failed_on_a_provider_error_is_parked_by_a_provider_outage_and_waits_for_a_person() {
    let mut w = World::new("park-outage");
    capped(&w, "step-failed:adversarial", OUTAGE);
    w.work(LOG);
    from_unit(&mut w, "label", 1_760_000_000, &[]);
    let r = w.run7();
    assert_eq!(r.rc, 0);
    let gh = w.gh();
    has(
        &gh,
        "Tehas cycle parked by a provider outage: status `handoff`",
    );
    has(&gh, "cap `step-failed:adversarial`: Connection error.");
    has(&gh, "Only the cycle's own result names the outage, so tehas does not start it again on its own: it waits for a person.");
    lacks(&gh, "starts it again once");
    lacks(&gh, "stopped without a pull request");
    has(
        &gh,
        "--remove-label tehas:running --add-label needs-human-attention",
    );
    let p = w.park();
    assert_eq!(
        park_of(
            &w,
            &[
                "reason",
                "step",
                "automatic",
                "retries",
                "started_by",
                "question"
            ]
        ),
        json!([
            "step-failed:adversarial",
            "handoff",
            false,
            0,
            "alice",
            4002
        ])
    );
    assert!(p["started_at"].is_number());
    let h = history_last(&w);
    assert_eq!(
        json!([h["cap"], h["provider"], h["parked"]]),
        json!(["step-failed:adversarial", true, true])
    );
}

#[test]
fn a_failed_dispatch_is_no_outage_whatever_its_words_say() {
    for f in [
        r#"{"kind":"dispatch-failed","at":2,"errorTail":"request timed out after 600s"}"#,
        r#"{"kind":"dispatch-failed","at":2,"errorTail":"502 Bad Gateway"}"#,
    ] {
        let w = World::new("park-dispatch");
        capped(&w, "step-failed:develop", f);
        w.run7();
        assert_eq!(w.park()["reason"], "step-failed:develop", "{f}");
    }
}

#[test]
fn an_outage_of_a_cycle_started_by_hand_owes_no_retry_and_says_it_waits_for_a_person() {
    let w = World::new("park-hand");
    capped(&w, "step-failed:adversarial", OUTAGE);
    w.run7();
    let gh = w.gh();
    has(&gh, "Tehas cycle parked by a provider outage");
    has(&gh, "it waits for a person.");
    lacks(&gh, "starts it again once");
    assert_eq!(
        park_of(&w, &["reason", "automatic", "started_by"]),
        json!(["step-failed:adversarial", false, null])
    );
}

#[test]
fn the_retry_tehas_made_on_its_own_parked_by_an_outage_again_waits_for_a_person() {
    let mut w = World::new("park-retry");
    capped(&w, "step-failed:adversarial", OUTAGE);
    from_unit(&mut w, "outage-retry", 1_760_000_000, &[]);
    w.run7();
    let gh = w.gh();
    has(&gh, "Tehas cycle parked by a provider outage");
    has(&gh, "it waits for a person.");
    lacks(&gh, "starts it again once");
    assert_eq!(
        park_of(&w, &["reason", "automatic"]),
        json!(["step-failed:adversarial", false])
    );
    assert_eq!(w.row(&["cycle-show", "7"])["cycle"]["requeued"], true);
}

#[test]
fn a_step_that_failed_for_another_reason_is_parked_as_before_and_no_retry() {
    let w = World::new("park-other");
    capped(
        &w,
        "step-failed:develop",
        r#"{"kind":"dispatch-failed","at":2,"errorTail":"3 assertions failed in test/x"}"#,
    );
    w.work(LOG);
    w.run7();
    let gh = w.gh();
    has(&gh, "Tehas cycle stopped without a pull request");
    lacks(&gh, "provider outage");
    assert_eq!(
        park_of(&w, &["reason", "automatic"]),
        json!(["step-failed:develop", false])
    );
    has(&gh, "3 assertions failed in test/x");
}

#[test]
fn a_handoff_no_pull_request_carries_is_posted_once_by_the_host() {
    let long_cause = "verify-timeout verify command bash scripts/verify.sh exceeded its 30-min timeout on the CONSOLIDATED tree at /w/tehas (PI_ENSEMBLE_VERIFY_SUITE_TIMEOUT_MS=1800000), so the run was killed at the bound; the gate was still in test/cycle_test.sh";
    let w = World::new("park-handoff");
    let f = json!({"kind": "dispatch-failed", "at": 2, "errorTail": long_cause}).to_string();
    handed_off(&w, "verify-timeout:develop", &f);
    w.work(LOG);
    let r = w.run7();
    assert_eq!(r.rc, 0);
    let gh = w.gh();
    assert_eq!(count(&gh, "pi-rukas:handoff issue=7 cap="), 1);
    has(&gh, "The branch is consolidated locally, not pushed.");
    lacks(&r.out, "reached no comment");
    assert_eq!(history_last(&w)["cap"], "verify-timeout:develop");
}

#[test]
fn a_handoff_that_reached_no_comment_is_said_in_the_log() {
    let w = World::new("park-lost");
    handed_off(
        &w,
        "verify-timeout:develop",
        r#"{"kind":"dispatch-failed","at":2,"errorTail":"exit 1"}"#,
    );
    w.work(LOG);
    w.touch("post-fails");
    w.touch("comment-fails");
    let r = w.run7();
    has(
        &r.out,
        "WARN: the handoff of #7 reached no comment: it is only in the run log",
    );
}

#[test]
fn a_cycle_with_no_handoff_warns_of_no_lost_handoff() {
    let w = World::new("park-nohandoff");
    capped(
        &w,
        "step-failed:develop",
        r#"{"kind":"dispatch-failed","at":2,"errorTail":"exit 1"}"#,
    );
    w.work(LOG);
    let r = w.run7();
    lacks(&r.out, "reached no comment");
}

#[test]
fn a_cap_whose_progress_names_no_failure_is_no_outage_and_asks_no_reason() {
    let w = World::new("park-noreason");
    capped(
        &w,
        "step-failed:develop",
        r#"{"kind":"dispatch-failed","at":2}"#,
    );
    let r = w.run7();
    lacks(&r.out, "no reason was found");
    assert_eq!(w.park()["reason"], "step-failed:develop");
    let w = World::new("park-noreason2");
    capped(
        &w,
        "step-failed:develop",
        r#"{"kind":"dispatch-failed","at":2,"errorTail":"exit 1"}"#,
    );
    let r = w.run7();
    lacks(&r.out, "no reason was found");
}

#[test]
fn a_failure_of_kind_provider_is_said_as_an_outage_whatever_the_cap_and_owes_no_retry() {
    let w = World::new("park-provider");
    failed(&w, "spawn-backstop", "provider", "rate limited");
    w.run7();
    let gh = w.gh();
    has(&gh, "Tehas cycle parked by a provider outage");
    has(&gh, "cap `spawn-backstop`: rate limited");
    assert_eq!(
        park_of(&w, &["reason", "automatic"]),
        json!(["spawn-backstop", false])
    );
    let h = history_last(&w);
    assert_eq!(
        json!([h["cap"], h["provider"]]),
        json!(["spawn-backstop", true])
    );
}

#[test]
fn a_failure_of_another_kind_is_no_outage_even_when_its_words_look_like_one() {
    for k in ["process", "timeout", "verify", "cap", "something-new"] {
        let w = World::new("park-kind");
        failed(
            &w,
            "step-failed:develop",
            k,
            "502 Bad Gateway, connection timed out",
        );
        let r = w.run7();
        assert_eq!(w.park()["reason"], "step-failed:develop", "{k}");
        lacks(&w.gh(), "provider outage");
        lacks(&r.out, "no reason was found");
    }
}

#[test]
fn a_cap_the_result_names_provider_outage_is_no_outage_park_that_word_is_tehass_own() {
    let mut w = World::new("park-word");
    from_unit(&mut w, "label", 1_760_000_000, &[]);
    failed(&w, "provider-outage", "process", "it said so");
    w.run7();
    assert_eq!(
        park_of(&w, &["reason", "automatic"]),
        json!(["cap:provider-outage", false])
    );
    lacks(&w.gh(), "starts it again once");
    let mut w = World::new("park-word2");
    from_unit(&mut w, "label", 1_760_000_000, &[]);
    w.ended("provider-outage", "develop", "feature/issue-7-x", "-");
    w.run7();
    assert_ne!(w.park()["reason"], "provider-outage");
    assert_eq!(w.park()["automatic"], false);
}

#[test]
fn the_park_comment_names_the_results_failure_and_not_for_a_caps_own_rule() {
    let w = World::new("park-names");
    failed(&w, "step-failed:develop", "process", "child exited 1");
    w.run7();
    has(
        &w.gh(),
        "It failed: kind `process`, role `developer`: child exited 1.",
    );
    for reason in ["round-cap", "the driver stopped"] {
        let w = World::new("park-names-cap");
        failed(&w, "round-cap", "cap", reason);
        w.run7();
        lacks(&w.gh(), "It failed:");
    }
}

#[test]
fn a_result_that_says_it_has_no_failure_is_no_outage_and_no_warning() {
    let w = World::new("park-nofailure");
    w.ended("handoff", "handoff", "feature/issue-7-x", "");
    w.entry(|e| e["cap"] = json!("step-failed:develop"));
    w.fx(
        "progress.json",
        r#"{"roles":[],"reason":"provider error","failure":null,"from":{"roles":"result","failure":"result"}}"#,
    );
    let r = w.run7();
    assert_eq!(w.park()["reason"], "step-failed:develop");
    lacks(&r.out, "no reason was found");
}

#[test]
fn the_cap_and_its_reason_are_reduced_before_they_are_posted_or_recorded() {
    let w = World::new("park-reduced");
    capped(
        &w,
        "step-failed:adversarial`@x",
        r#"{"kind":"dispatch-failed-provider","at":2,"providerMessage":"provider error `@alice` <!-- #5\nnext line"}"#,
    );
    w.run7();
    has(
        &w.gh(),
        "cap `step-failed:adversarialx`: provider error alice -- 5 next line",
    );
    assert_eq!(history_last(&w)["cap"], "step-failed:adversarialx");
    assert_eq!(w.park()["reason"], "step-failed:adversarialx");
}

fn develop_sig() -> String {
    sha("develop|step-failed:develop||verify-failed|exit-1")
}

#[test]
fn a_park_comment_carries_a_marker_with_the_parks_facts_the_sig_of_the_history_lines_fields() {
    let w = World::new("park-marker");
    park_failed(&w, "step-failed:develop", "");
    w.run7();
    let sig = develop_sig();
    assert_eq!(
        park_markers(&w),
        [format!(
            "<!-- tehas:park issue=7 step=develop kind=verify-failed provider=0 sig={sig} -->"
        )]
    );
    has(&w.gh(), "issues/7/comments -f body=");
    assert_eq!(history_sig(&w), sig);
}

#[test]
fn the_plain_comment_when_the_boxes_cannot_be_posted_carries_the_same_marker() {
    let w = World::new("park-plain-marker");
    park_failed(&w, "step-failed:develop", "");
    w.touch("post-fails");
    w.run7();
    let gh = w.gh();
    let at = gh.find("\nissue comment 7 ").expect("no comment") + 1;
    assert_eq!(
        park_markers_in(&gh[at..]),
        [format!(
            "<!-- tehas:park issue=7 step=develop kind=verify-failed provider=0 sig={} -->",
            develop_sig()
        )]
    );
}

#[test]
fn a_park_that_repeats_the_issues_previous_end_says_so_with_the_step_and_the_cap() {
    let w = World::new("park-repeats");
    park_failed(&w, "step-failed:develop", "");
    w.run7();
    assert_eq!(count(&w.gh(), "repeats the one before it"), 0);
    // The park before this one, left on the same store by the cycle above.
    w.run7();
    let gh = w.gh();
    assert_eq!(count(&gh, "repeats the one before it"), 1);
    has(&gh, "number 1 of the parks in a row that stopped at step `develop` on the cap `step-failed:develop`");
    has(&gh, "change the issue first");
}

#[test]
fn a_park_that_does_not_repeat_the_end_before_it_says_nothing_of_a_run() {
    let w = World::new("park-norun");
    park_failed(&w, "verify-failed:develop", "");
    w.run7();
    park_failed(&w, "step-failed:develop", "");
    w.run7();
    // Another cap before: no run, and the count starts again.
    assert_eq!(count(&w.gh(), "repeats the one before it"), 0);
    park_failed(&w, "step-failed:develop", "");
    w.run7();
    assert_eq!(count(&w.gh(), "repeats the one before it"), 1);
    // A merge between two parks alike breaks the run.
    let w = World::new("park-norun-merge");
    park_failed(&w, "step-failed:develop", "");
    w.run7();
    w.ended("merged", "review", "feature/x", "");
    w.merged_pr();
    w.fx("rc", "0\n");
    w.run7();
    park_failed(&w, "step-failed:develop", "");
    w.run7();
    assert_eq!(count(&w.gh(), "repeats the one before it"), 0);
}

#[test]
fn a_park_of_a_provider_outage_never_says_it_repeats_a_run() {
    let w = World::new("park-outage-run");
    capped(&w, "step-failed:adversarial", OUTAGE);
    parked_end(&w, "handoff", "step-failed:adversarial", "", "");
    w.run7();
    assert_eq!(count(&w.gh(), "repeats the one before it"), 0);
}

#[test]
fn a_history_that_cannot_be_read_says_nothing_of_a_run_and_the_park_goes_on() {
    let w = World::new("park-nohistory");
    park_failed(&w, "step-failed:develop", "");
    parked_end(
        &w,
        "develop",
        "step-failed:develop",
        "verify-failed",
        "exit-1",
    );
    w.store_answers("history-show", 1, "refused", 999);
    w.run7();
    assert_eq!(count(&w.gh(), "repeats the one before it"), 0);
    w.store_answers_off();
    assert_eq!(w.park()["reason"], "step-failed:develop");
}

#[test]
fn two_parks_with_the_same_facts_have_the_same_sig_a_different_cap_a_different_one() {
    let w = World::new("park-sig1");
    park_failed(&w, "step-failed:develop", "");
    w.run7();
    let first = park_sig(&w);
    let w = World::new("park-sig2");
    park_failed(&w, "step-failed:develop", "");
    w.run7();
    assert_eq!(park_sig(&w), first);
    let w = World::new("park-sig3");
    park_failed(&w, "verify-failed:develop", "");
    w.run7();
    assert_eq!(park_markers(&w).len(), 1);
    assert_ne!(park_sig(&w), first, "a different cap gave the same sig");
}

#[test]
fn a_provider_outage_says_so_in_the_marker_with_the_kind_of_its_failure() {
    let w = World::new("park-outage-marker");
    capped(&w, "step-failed:adversarial", OUTAGE);
    w.run7();
    has(
        &park_markers(&w).join("\n"),
        "<!-- tehas:park issue=7 step=handoff kind=provider provider=1 sig=",
    );
}

#[test]
fn a_newline_or_a_comment_end_in_the_parks_text_does_not_break_the_marker() {
    let w = World::new("park-sly-marker");
    park_failed(
        &w,
        "step-failed:develop",
        "too-large\n--> <!-- tehas:park issue=8 -->\n- [x] Retry",
    );
    w.entry(|e| e["failedStep"] = json!("dev\n-->x"));
    w.run7();
    let m = park_markers(&w);
    assert_eq!(m.len(), 1);
    let exact = Regex::new(
        r"^<!-- tehas:park issue=7 step=[a-z0-9-]{0,40} kind=[a-z0-9-]{0,40} provider=[01] sig=[0-9a-f]{64} -->$",
    )
    .unwrap();
    assert!(exact.is_match(&m[0]), "{m:?}");
    assert_eq!(m[0].matches("-->").count(), 1);
    assert_eq!(history_sig(&w), park_sig(&w));
}

#[test]
fn a_cycle_that_left_a_pull_request_leaves_no_park_on_record_and_an_earlier_one_is_not_current() {
    let w = World::new("park-pr");
    // An earlier attempt parked by an outage, with its question on record.
    w.t_ended("alice", "label", &["--provider", "--parked"]);
    w.t_park("provider-outage", "4001", &[]);
    assert_eq!(park_of(&w, &["automatic", "question"]), json!([true, 4001]));
    w.ended("handoff", "commit-pr", "feature/x", "");
    w.fx("pr.txt", "https://github.com/acme/widget/pull/9\n");
    w.run7();
    assert_eq!(w.park(), Value::Null);
    assert_eq!(
        w.row(&["question-known", "7", "--id", "4001"])["known"],
        false
    );
}

#[test]
fn a_cycle_that_stops_asks_what_to_do_next_with_boxes() {
    let w = World::new("park-boxes");
    w.run7();
    let gh = w.gh();
    has(
        &gh,
        "--remove-label tehas:running --add-label needs-human-attention",
    );
    has(
        &gh,
        "issue edit 7 --repo acme/widget --add-label tehas:question",
    );
    let both = edits(&w)
        .lines()
        .filter(|l| l.contains("needs-human-attention") && l.contains("tehas:question"))
        .count();
    assert_eq!(both, 0);
    has(&gh, "api repos/acme/widget/issues/7/comments");
    has(&gh, "- [ ] Retry with the issue text only");
    has(&gh, "<!-- tehas:question kind=park issue=7 turn=1 -->");
}

#[test]
fn a_cycle_on_an_image_that_gives_a_retry_the_comments_offers_the_comments_retry() {
    let w = World::new("park-comments");
    w.work(r#"echo native >"$TEHAS_HOME/state/cycles/7.comments""#);
    w.run7();
    let gh = w.gh();
    has(&gh, "- [ ] Retry with the issue and all its comments");
    has(&gh, "- [ ] Retry with the issue text only");
    has(&gh, "Both retries start over in a new workspace.");
    let w = World::new("park-comments2");
    w.run7();
    lacks(&w.gh(), "Retry with the issue and all its comments");
}

#[test]
fn the_host_keeps_the_id_of_the_park_question_it_posted_as_the_question_of_this_park() {
    let w = World::new("park-keeps-id");
    w.run7();
    assert_eq!(park_q(&w), "4002");
    let q = w.row(&["question-known", "7", "--id", "4002"]);
    assert_eq!(
        json!([q["known"], q["kind"], q["answered"]]),
        json!([true, "park", false])
    );
    assert_eq!(park_of(&w, &["attempt", "reason"]), json!([1, "unknown"]));
}

#[test]
fn the_body_of_the_question_goes_when_it_is_posted() {
    let w = World::new("park-body-goes");
    w.run7();
    let left = w
        .files(&w.home.join("tmp"))
        .into_iter()
        .filter(|f| f.starts_with("park-question."))
        .count();
    assert_eq!(left, 0);
}

#[test]
fn when_the_question_cannot_be_posted_the_old_comment_is_used_and_no_question_label_is_added() {
    let w = World::new("park-nopost");
    w.touch("post-fails");
    w.run7();
    let gh = w.gh();
    has(&gh, "issue comment 7");
    has(&gh, "Tehas cycle stopped without a pull request");
    has(&gh, "To retry, apply");
    lacks(&edits(&w), "tehas:question");
    assert_eq!(park_q(&w), "none");
    // The park is on record all the same: it asks nothing.
    assert_eq!(w.park()["reason"], "unknown");
}

#[test]
fn a_question_github_answered_with_a_5xx_and_took_is_read_back_and_asked() {
    let w = World::new("park-5xx-took");
    w.fx("post-fails", "HTTP 502: Bad Gateway\n");
    w.touch("post-taken");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    let gh = w.gh();
    assert_eq!(starts(&gh, "api repos/acme/widget/issues/7/comments "), 1);
    lacks(&gh, "issue comment 7");
    lacks(&r.out, "may have been posted");
    assert_eq!(park_q(&w), "4002");
    has(&edits(&w), "tehas:question");
}

#[test]
fn a_question_github_answered_with_a_5xx_and_did_not_take_gets_the_plain_comment() {
    let w = World::new("park-5xx-not");
    w.fx("post-fails", "HTTP 502: Bad Gateway\n");
    w.fx("comments.json", "[]\n");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    let gh = w.gh();
    assert_eq!(starts(&gh, "api repos/acme/widget/issues/7/comments "), 1);
    assert_eq!(starts(&gh, "issue comment 7 "), 1);
    lacks(&r.out, "may have been posted");
    assert_eq!(park_q(&w), "none");
}

#[test]
fn a_question_github_answered_with_a_5xx_that_no_read_settles_is_not_posted_again_nor_followed_by_the_plain_comment(
) {
    let w = World::new("park-5xx-unknown");
    w.fx("post-fails", "HTTP 502: Bad Gateway\n");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    let gh = w.gh();
    assert_eq!(starts(&gh, "api repos/acme/widget/issues/7/comments "), 1);
    lacks(&gh, "issue comment 7");
    has(&r.out, "may have been posted");
    // The row stays open for the poll's read to settle.
    let open = w.store_ok(&["journal-open"]);
    let rows: Vec<Value> = open["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| json!([a["kind"], a["state"]]))
        .collect();
    assert_eq!(rows, [json!(["comment-post", "unknown"])]);
}

#[test]
fn a_park_the_store_would_not_take_asks_no_question_the_plain_comment_and_no_tehasquestion() {
    let w = World::new("park-busy");
    w.store_answers("park", 1, "store: busy #7: the database is locked", 0);
    let r = w.run7();
    assert_eq!(r.rc, 0);
    has(&r.out, "WARN: could not record the park of #7 (busy: store: busy #7: the database is locked), so it asks no question");
    assert_eq!(w.store_answered(), 3);
    let gh = w.gh();
    lacks(&gh, "api repos/acme/widget/issues/7/comments");
    has(&gh, "issue comment 7 --repo acme/widget --body");
    has(&gh, "Tehas cycle stopped without a pull request");
    has(&gh, "To retry, apply `tehas:ready`.");
    lacks(&gh, "- [ ] Retry with the issue text only");
    lacks(&edits(&w), "tehas:question");
    has(
        &gh,
        "--remove-label tehas:running --add-label needs-human-attention",
    );
    w.store_answers_off();
    assert_eq!(w.park(), Value::Null);
    assert_eq!(history_last(&w)["parked"], true);
}

#[test]
fn and_so_does_a_store_that_cannot_be_used() {
    let w = World::new("park-corrupt");
    w.store_answers("park", 78, "store: corrupt: the file is not a database", 0);
    let r = w.run7();
    has(&r.out, "WARN: could not record the park of #7 (corrupt");
    let gh = w.gh();
    lacks(&gh, "api repos/acme/widget/issues/7/comments");
    has(&gh, "To retry, apply `tehas:ready`.");
    lacks(&edits(&w), "tehas:question");
}

#[test]
fn an_outage_park_the_store_would_not_take_does_not_say_tehas_starts_it_again() {
    let mut w = World::new("park-outage-busy");
    capped(&w, "step-failed:adversarial", OUTAGE);
    from_unit(&mut w, "label", 1_760_000_000, &[]);
    w.store_answers("park", 1, "store: busy #7: the database is locked", 0);
    w.run7();
    let gh = w.gh();
    has(&gh, "Tehas cycle parked by a provider outage");
    lacks(&gh, "starts it again once");
    has(
        &gh,
        "so tehas does not start it again on its own: it waits for a person.",
    );
    w.store_answers_off();
    assert_eq!(w.park(), Value::Null);
}

#[test]
fn a_question_the_store_would_not_take_is_posted_and_the_log_says_its_boxes_will_not_work() {
    let w = World::new("park-q-busy");
    w.store_answers(
        "question-asked",
        1,
        "store: busy #7: the database is locked",
        0,
    );
    let r = w.run7();
    has(&w.gh(), "- [ ] Retry with the issue text only");
    has(
        &r.out,
        "WARN: could not record the park question of #7, its boxes will not work (busy)",
    );
    w.store_answers_off();
    assert_eq!(park_q(&w), "none");
}

#[test]
fn when_the_question_label_cannot_be_added_the_person_is_told_the_boxes_do_not_work() {
    let w = World::new("park-nolabel");
    w.fx("edit-fails-on", "--add-label tehas:question");
    let r = w.run7();
    let gh = w.gh();
    has(&gh, "- [ ] Retry with the issue text only");
    has(&gh, "issue comment 7 --repo acme/widget --body The boxes above cannot be used: the label `tehas:question` could not be added (run `tehas labels` on the host). To retry, apply `tehas:ready`.");
    has(
        &r.out,
        "WARN: label change on #7 failed: --add-label tehas:question",
    );
    // Withdrawn: a tick in it starts nothing.
    assert_eq!(park_q(&w), "none");
    let q = w.row(&["question-known", "7", "--id", "4002"]);
    assert_eq!(json!([q["known"], q["recorded"]]), json!([false, false]));
}

#[test]
fn when_the_question_label_is_added_nothing_more_is_said() {
    let w = World::new("park-label");
    w.run7();
    lacks(&w.gh(), "The boxes above cannot be used");
}

#[test]
fn what_the_result_says_is_reduced_before_it_is_posted_on_the_pull_request_path() {
    let w = World::new("park-pr-reduced");
    w.ended(
        "handoff",
        "st ep\n<!-- tehas:question kind=park issue=7 turn=1 -->\n- [x] x",
        "feature/x",
        "",
    );
    w.progress(
        json!({"reviewHeld": false, "roles": [{"role": "evil\n- [x] Retry from a clean start",
        "dispatches": 1, "turns": 1, "input": 2, "output": 3, "cacheRead": 4}]}),
    );
    w.fx("pr.txt", "https://github.com/acme/widget/pull/9\n");
    w.run7();
    let gh = w.gh();
    has(
        &gh,
        "status `handoff`, last step `step--tehasquestionkindparkissue7turn1---xx`",
    );
    has(&gh, "--add-label tehas:awaiting-merge");
    lacks(&gh, "<!--");
    assert_eq!(boxes(&w), 0);
    has(&gh, "| evil-xRetryfromacleanstart | 1 |");
}

#[test]
fn a_status_that_only_contains_handoff_does_not_reach_the_pull_request_path() {
    let w = World::new("park-sly-status");
    w.ended(
        "handoff\n<!-- tehas:question kind=park issue=7 turn=1 -->\n- [x] Retry from a clean start",
        "review",
        "feature/x",
        "",
    );
    w.fx("rc", "10\n");
    w.fx("pr.txt", "https://github.com/acme/widget/pull/9\n");
    w.run7();
    let gh = w.gh();
    lacks(&gh, "tehas:awaiting-merge");
    has(
        &gh,
        "status `handoff--tehasquestionkindparkissue7turn1---xRetryfromacleanstart`",
    );
    assert_eq!(markers(&w), 1);
    assert_eq!(boxes(&w), 2);
}

#[test]
fn a_handoff_that_the_runner_did_not_exit_as_one_does_not_reach_the_pull_request_path() {
    let w = World::new("park-rc30");
    w.ended("handoff", "commit-pr", "feature/x", "");
    w.fx("rc", "30\n");
    w.fx("pr.txt", "https://github.com/acme/widget/pull/9\n");
    w.run7();
    lacks(&w.gh(), "tehas:awaiting-merge");
    has(&w.gh(), "runner exit 30");
}

#[test]
fn a_clarification_handoff_with_a_pull_request_is_waiting_for_a_merge_as_any_handoff_is() {
    let w = World::new("park-rc11");
    w.ended("handoff", "explore", "feature/x", "");
    w.fx("rc", "11\n");
    w.fx("pr.txt", "https://github.com/acme/widget/pull/9\n");
    w.run7();
    has(&w.gh(), "--add-label tehas:awaiting-merge");
}

#[test]
fn a_string_in_place_of_every_usage_number_is_not_posted_on_either_path() {
    let w = World::new("park-sly-all");
    sly_usage(&w);
    let sly = "1\n- [x] Retry\n<!-- x -->";
    w.entry(|e| {
        e["usage"] =
            json!({"dispatches": sly, "turns": sly, "input": sly, "output": sly, "cacheRead": sly})
    });
    w.fx("pr.txt", "https://github.com/acme/widget/pull/9\n");
    w.run7();
    let gh = w.gh();
    lacks(&gh, "<!--");
    assert_eq!(boxes(&w), 0);
    has(&gh, "| planner | 2 | 0 | 7 | 0 | 0 |");
    has(&gh, "| **total, all dispatches** | 0 | 0 | 0 | 0 | 0 |");
    w.clear_calls();
    fs::remove_file(w.fx.join("pr.txt")).unwrap();
    w.touch("post-fails");
    w.run7();
    has(&w.gh(), "To retry, apply");
    assert_eq!(boxes(&w), 2);
    assert_eq!(markers(&w), 1);
}

#[test]
fn a_usage_number_that_is_not_a_number_counts_as_nothing_on_the_pull_request_path() {
    let w = World::new("park-sly-pr");
    sly_usage(&w);
    w.fx("pr.txt", "https://github.com/acme/widget/pull/9\n");
    w.run7();
    let gh = w.gh();
    has(&gh, "pr comment https://github.com/acme/widget/pull/9");
    lacks(&gh, "<!--");
    assert_eq!(boxes(&w), 0);
    has(&gh, "| planner | 2 | 0 | 7 | 0 | 0 |");
    has(&gh, "| coder | 1 | 2 | 3 | 4 | 5 |");
    has(&gh, "| **total, all dispatches** | 3 | 7 | 10 | 4 | 5 |");
}

#[test]
fn and_in_the_park_question_where_only_the_factorys_own_boxes_and_marker_are_posted() {
    let w = World::new("park-sly-q");
    sly_usage(&w);
    w.fx("rc", "30\n");
    w.run7();
    has(
        &w.gh(),
        "| **total, all dispatches** | 3 | 7 | 10 | 4 | 5 |",
    );
    assert_eq!(boxes(&w), 2);
    assert_eq!(markers(&w), 1);
}

#[test]
fn and_in_the_comment_that_is_used_when_the_question_cannot_be_posted() {
    let w = World::new("park-sly-plain");
    sly_usage(&w);
    w.fx("rc", "30\n");
    w.touch("post-fails");
    w.run7();
    let gh = w.gh();
    has(&gh, "To retry, apply");
    assert_eq!(
        count(&gh, "| **total, all dispatches** | 3 | 7 | 10 | 4 | 5 |"),
        2
    );
    // The question that could not be posted is in the record of calls too.
    assert_eq!(boxes(&w), 2);
    assert_eq!(markers(&w), 1);
}

#[test]
fn a_lean_start_tehas_work_refused_is_parked_with_what_to_do_and_its_approval_goes() {
    let mut w = World::new("park-lean");
    lean_refused(&mut w);
    let r = w.run7();
    assert_eq!(r.rc, 0);
    let gh = w.gh();
    has(&gh, "Tehas started no cycle: this start's driver is `lean`, and the sandbox image cannot run it as this start asks (runner exit 78). It does not run classic in its place, and nothing of it ran. The log names what the image lacks. To keep it on lean, rebuild the sandbox image on a pi-rukas that supports this, then apply `tehas:ready` again. To run it on classic instead, which changes its driver, take `tehas:lean` off, then retry;");
    has(
        &gh,
        "--remove-label tehas:running --add-label needs-human-attention",
    );
    has(
        &gh,
        "To run it on classic instead, take `tehas:lean` off this issue, then tick a box.",
    );
    // One line names the refusal, and no warning of a missing result.
    has(&r.out, "#7: tehas work refused the start before its runner ran (exit 78): the sandbox image cannot run lean as this start asks");
    lacks(
        &r.out,
        "holds nothing, so its step is read from its run log",
    );
    let live: Value =
        serde_json::from_str(&fs::read_to_string(w.home.join("live/running.json")).unwrap())
            .unwrap();
    assert_eq!(
        live["last"]["statusText"],
        "the sandbox image cannot run lean as this start asks"
    );
    has(&gh, "- [ ] Retry with the issue text only");
    lacks(&gh, "stopped without a pull request");
    lacks(&gh, "--add-label tehas:ready");
    assert_eq!(w.park()["reason"], "lean-refused");
    let a = approval(&w);
    assert_eq!(
        json!([a["approval"], a["current"], a["rests_on"]]),
        json!([null, null, null])
    );
    let h = w.history0();
    assert_eq!(
        json!([h["rc"], h["how"], h["by"]]),
        json!([78, "label", "alice"])
    );
    assert_eq!(w.park()["automatic"], false);
}

#[test]
fn a_78_on_a_classic_start_stays_a_stopped_cycle_with_its_approval_on_record() {
    let mut w = World::new("park-78-classic");
    w.fx("rc", "78\n");
    from_unit(&mut w, "label", NEW_YEAR, &[]);
    w.run7();
    has(&w.gh(), "stopped without a pull request");
    lacks(&w.gh(), "tehas:lean");
    assert_eq!(approval(&w)["current"]["by"], "alice");
}

#[test]
fn a_78_on_a_lean_start_that_tehas_work_did_not_say_was_leans_is_a_stopped_cycle_with_its_approval_on_record(
) {
    let mut w = World::new("park-78-lean");
    lean_refused(&mut w);
    fs::remove_file(w.fx.join("work.sh")).unwrap();
    w.run7();
    has(&w.gh(), "stopped without a pull request");
    lacks(&w.gh(), "tehas:lean");
    assert_eq!(approval(&w)["current"]["by"], "alice");
}

#[test]
fn a_refused_lean_start_of_an_earlier_cycle_says_nothing_of_this_one() {
    let mut w = World::new("park-lean-earlier");
    lean_refused(&mut w);
    fs::remove_file(w.fx.join("work.sh")).unwrap();
    fs::create_dir_all(w.state()).unwrap();
    fs::write(w.state().join("7.lean-refused"), "").unwrap();
    w.run7();
    has(&w.gh(), "stopped without a pull request");
}

#[test]
fn a_start_that_never_ran_is_parked_as_one_that_never_ran_named_from_the_record() {
    let mut w = World::new("park-never");
    never_ran(&mut w, true);
    w.run7();
    let gh = w.gh();
    has(&gh, "Tehas started no cycle: tehas work refused the start before its runner ran (runner exit 4). It refuses one, for example, when the issue changed after its approval, when the approval or the issue cannot be read, or when an image or file the issue links cannot be given to the cycle.");
    has(&gh, "alice approved this start at 2026-01-01T00:00:00Z");
    has(&gh, "- [ ] Retry with the issue text only");
    lacks(&gh, "stopped without a pull request");
    lacks(&gh, "status `unknown`");
    lacks(&gh, "last step");
    assert_eq!(
        park_of(&w, &["reason", "automatic", "body_sha256"]),
        json!(["unknown", false, null])
    );
}

#[test]
fn a_refresh_that_never_ran_names_the_persons_approval_it_rests_on_not_the_time_the_poll_decided_it(
) {
    let mut w = World::new("park-never-refresh");
    never_ran(&mut w, false);
    w.t_ended("alice", "label", &["--status", "handoff"]);
    let head = "a".repeat(40);
    from_unit(
        &mut w,
        "refresh",
        DECIDED,
        &["--refresh-pr", "50", "--refresh-head", &head],
    );
    w.run7();
    has(&w.gh(), "alice approved this start at 2023-11-14T22:13:20Z");
    lacks(&w.gh(), "2026-10-24T12:00:00Z");
}

#[test]
fn the_retry_after_an_outage_that_never_ran_names_no_time_of_approval_the_poll_decided_it() {
    let mut w = World::new("park-never-outage");
    never_ran(&mut w, false);
    from_unit(&mut w, "outage-retry", DECIDED, &[]);
    w.run7();
    has(&w.gh(), "alice approved this start;");
    lacks(&w.gh(), "2026-10-24T12:00:00Z");
}

#[test]
fn a_start_that_never_ran_leaves_no_approval_record_and_its_line_of_history_still_names_how_it_was_authorised(
) {
    let mut w = World::new("park-never-record");
    never_ran(&mut w, true);
    w.run7();
    let a = approval(&w);
    assert_eq!(json!([a["approval"], a["current"]]), json!([null, null]));
    let h = w.history0();
    assert_eq!(
        json!([h["rc"], h["status"], h["step"], h["how"], h["parked"]]),
        json!([4, "refused", "-", "label", true])
    );
}

#[test]
fn a_cycle_that_ran_keeps_its_approval_record() {
    let mut w = World::new("park-ran-keeps");
    from_unit(&mut w, "label", NEW_YEAR, &[]);
    w.ended("handoff", "commit-pr", "feature/x", "");
    w.fx("pr.txt", "https://github.com/acme/widget/pull/9\n");
    w.run7();
    let a = approval(&w);
    assert_eq!(
        json!([a["current"]["by"], a["current"]["used_by"]]),
        json!(["alice", 1])
    );
}

#[test]
fn a_start_that_never_ran_with_no_record_on_this_host_says_so_and_still_asks_for_the_label() {
    let mut w = World::new("park-never-norecord");
    never_ran(&mut w, false);
    w.run7();
    has(
        &w.gh(),
        "No approval of this start is recorded on this host: a fresh `tehas:ready` is needed.",
    );
    has(&w.gh(), "--add-label needs-human-attention");
    assert_eq!(park_of(&w, &["attempt", "question"]), json!([1, 4002]));
}

#[test]
fn the_park_carries_the_body_of_the_approval_it_started_on_and_a_park_of_a_cycle_started_by_hand_names_no_body(
) {
    // The shell file's two cases "the park carries the body of the approval it
    // started on" and "a park of a cycle started by hand names no body, not
    // that of an earlier start", which runs on the state of the first.
    let sum = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let mut w = World::new("park-body");
    from_unit(&mut w, "label", NEW_YEAR, &["--body-sha", sum]);
    w.fx("rc", "30\n");
    w.run7();
    assert_eq!(w.park()["body_sha256"], sum);
    // By hand: the earlier start's approval is still the last one used.
    w.env("TEHAS_CYCLE_FROM", "");
    w.run7();
    assert_eq!(w.park()["body_sha256"], Value::Null);
    let c = w.row(&["cycle-show", "7"])["cycle"].clone();
    assert_eq!(json!([c["attempt"], c["how"]]), json!([2, "hand"]));
}

// --- A develop timeout (#832, #833) --------------------------------------------

/// A cycle stopped at its developer's bound, the driver's work-state in the
/// workspace, and the issue's body.
fn develop_timeout(w: &World, body: &str) {
    w.ended(
        "aborted",
        "develop",
        "feature/issue-7-x",
        "child-timeout:develop",
    );
    w.entry(|e| {
        e["failure"] = json!({"errorTail": "[pi-rukas] killed after 5400000ms child-bound"})
    });
    let ws = w.workdir().join(".pi/work-state");
    fs::create_dir_all(&ws).unwrap();
    fs::write(
        ws.join("7.json"),
        json!({"pipelineState": {"normalisedSpec": {"deliverables": [{"id": "d1", "paths": ["libexec/drain"]}]}}}).to_string(),
    )
    .unwrap();
    w.fx(
        "issue-7.json",
        &json!({"number": 7, "state": "open", "body": body}).to_string(),
    );
}

/// The body the first timeout writes on `body`.
fn noted(w: &World, body: &str) -> String {
    develop_timeout(w, body);
    let mut child = Command::new(env!("CARGO_BIN_EXE_tehas-cycle"))
        .arg("last-attempt")
        .arg("7")
        .arg(w.fx.join("result.json"))
        .arg(w.workdir())
        .env("TEHAS_HOME", &w.home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(json!({"body": body}).to_string().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    v["body"].as_str().unwrap().to_string()
}

#[test]
fn a_develop_timeout_leaves_a_note_of_the_attempt_in_the_issues_body_and_still_offers_a_retry() {
    let w = World::new("park-timeout");
    develop_timeout(&w, "Fix the drain.");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    let gh = w.gh();
    has(&gh, "api -X PATCH repos/acme/widget/issues/7 -f body=Fix the drain.\n\n<!-- tehas:last-attempt base=");
    has(&gh, "the developer ran past its time bound of 90 minutes");
    has(&gh, "  - `libexec/drain`");
    has(&gh, "- [ ] Retry with the issue text only");
    assert_eq!(w.park()["reason"], "child-timeout:develop");
}

#[test]
fn a_second_develop_timeout_on_the_same_text_writes_nothing_offers_no_retry_and_names_tehasplan() {
    let w = World::new("park-timeout-twice");
    let body = noted(&w, "Fix the drain.");
    develop_timeout(&w, &body);
    w.run7();
    let gh = w.gh();
    lacks(&gh, "api -X PATCH");
    lacks(&gh, "Retry with the issue text only");
    has(&gh, "- [ ] Leave it parked");
    has(
        &gh,
        "ran out of time in develop twice on the same text: split it with `tehas:plan`",
    );
    assert_eq!(park_q(&w), "4002");
}

#[test]
fn a_timeout_after_the_text_changed_outside_the_note_writes_a_fresh_note_and_offers_a_retry() {
    let w = World::new("park-timeout-changed");
    let body = noted(&w, "Fix the drain.");
    develop_timeout(
        &w,
        &body.replacen("Fix the drain.", "Fix only the drain's check.", 1),
    );
    w.run7();
    has(
        &w.gh(),
        "api -X PATCH repos/acme/widget/issues/7 -f body=Fix only the drain's check.",
    );
    has(&w.gh(), "- [ ] Retry with the issue text only");
}

#[test]
fn a_note_github_will_not_take_is_a_warning_and_the_park_is_posted_as_before() {
    let w = World::new("park-timeout-patch");
    develop_timeout(&w, "Fix the drain.");
    w.touch("patch-fails");
    let r = w.run7();
    has(
        &r.out,
        "WARN: could not leave the last attempt's note in the body of #7",
    );
    has(&w.gh(), "- [ ] Retry with the issue text only");
    assert_eq!(park_q(&w), "4002");
}

#[test]
fn another_cap_leaves_the_body_alone() {
    let w = World::new("park-timeout-cap");
    develop_timeout(&w, "Fix the drain.");
    w.entry(|e| e["cap"] = json!("child-timeout:explore"));
    w.run7();
    lacks(&w.gh(), "api -X PATCH");
    has(&w.gh(), "- [ ] Retry with the issue text only");
}
