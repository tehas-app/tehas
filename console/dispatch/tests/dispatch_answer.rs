//! An answer `tehas answer` gave (#2782): the poll acts on it as on the box
//! of the same choice ticked by its account, and takes away one it cannot
//! take. The answers are given through the real store, as the command gives
//! them.

mod common;

use common::*;
use serde_json::json;
use std::fs;

/// The issues a cycle was started for, one per line.
fn started(w: &World) -> String {
    w.calls("fake-start").trim_end().to_string()
}

/// #7 answered through the store, for `by`.
fn answered(w: &World, by: &str, choice: &[&str]) {
    let mut args = vec!["answer-record", "7", "--by", by, "--choice"];
    args.extend(choice);
    w.store_ok(&args);
}

/// The choice of the answer on record of #7's question 4001, or "none".
fn waiting(w: &World) -> String {
    match &w.row(&["question-known", "7", "--id", "4001"])["answer"]["choice"] {
        serde_json::Value::String(c) => c.clone(),
        _ => "none".into(),
    }
}

/// What the planning turn was given.
fn turn_text(w: &World) -> String {
    fs::read_to_string(w.keep().join("answer.md"))
        .unwrap_or_default()
        .trim_end_matches('\n')
        .to_string()
}

/// #7 stopped, its question asked with no box ticked.
fn stopped(w: &World, boxes: &[&str]) {
    w.parked("failed", "alice");
    w.comments(7, &park_body(boxes), "", json!([]));
}

#[test]
fn an_answered_retry_starts_a_cycle_as_a_tick_does_and_counts_once() {
    let w = World::new("answer-retry");
    stopped(&w, &[" ", " "]);
    answered(&w, "alice", &["retry"]);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(started(&w), "7");
    has(&r.out, "start #7 (retry answered by alice)");
    has(&w.gh(), "authorized by @alice's answer");
    // No box was ticked, so none is unticked.
    lacks(&w.gh(), "PATCH");
    // The start is the retry of the question, by the account of the answer.
    assert_eq!(
        w.at_mark("[.by, .how, .comment_id] | tostring"),
        r#"["alice","answer",4001]"#
    );
    assert_eq!(w.park_q(7), "none");
    w.clear_calls();
    w.poll();
    assert_eq!(started(&w), "");
}

#[test]
fn an_answer_to_leave_it_parked_answers_the_question_and_starts_nothing() {
    let w = World::new("answer-park");
    stopped(&w, &[" ", " "]);
    answered(&w, "alice", &["park"]);
    w.poll();
    assert_eq!(started(&w), "");
    assert_eq!(w.park_q(7), "none");
    assert_eq!(
        w.row(&["question-known", "7", "--id", "4001"])["answered"],
        true
    );
}

#[test]
fn a_dry_run_says_what_the_answer_would_do_and_keeps_it() {
    let w = World::new("answer-dry");
    stopped(&w, &[" ", " "]);
    answered(&w, "alice", &["retry"]);
    let r = w.dispatch(&["--dry-run"]);
    has(&r.out, "would run a cycle for #7 (retry answered by alice)");
    assert_eq!(started(&w), "");
    assert_eq!(waiting(&w), "retry");
}

#[test]
fn a_retry_the_stop_does_not_offer_is_taken_away_and_said() {
    let w = World::new("answer-unoffered");
    // The stop of before #1492 offers one retry, with the comments.
    stopped(&w, &[" ", " "]);
    answered(&w, "alice", &["retry-text"]);
    let r = w.poll();
    assert_eq!(started(&w), "");
    has(&r.out, "skip #7: the answer of alice asks to retry with the issue text only, which its question does not offer");
    has(&w.gh(), "The answer given for this question was not taken");
    assert_eq!(waiting(&w), "none");
    assert_eq!(w.park_q(7), "4001");
}

#[test]
fn an_answer_for_an_account_that_may_not_authorise_work_starts_nothing() {
    let w = World::new("answer-stranger");
    stopped(&w, &[" ", " "]);
    // The command refuses such an account; one on record is refused again.
    answered(&w, "mallory", &["retry"]);
    let r = w.poll();
    assert_eq!(started(&w), "");
    has(
        &r.out,
        "the account may not authorise work for this factory",
    );
    assert_eq!(waiting(&w), "none");
}

#[test]
fn an_answer_on_an_issue_no_longer_stopped_is_taken_away() {
    let w = World::new("answer-not-parked");
    stopped(&w, &[" ", " "]);
    w.fx("asking.json", r#"[{"number":7,"labels":[]}]"#);
    answered(&w, "alice", &["retry"]);
    w.poll();
    assert_eq!(started(&w), "");
    has(&w.gh(), "this issue is not parked, so nothing was started");
    assert_eq!(waiting(&w), "none");
}

#[test]
fn an_answered_option_goes_to_the_planning_session_as_a_ticked_one() {
    let w = World::new("answer-option");
    w.planning();
    w.comments(7, &plan_body(" ", " ", " ", 1, ""), "", json!([]));
    answered(&w, "alice", &["option", "--option", "2"]);
    w.poll();
    assert_eq!(turn_text(&w), "The operator chose: Change some");
    assert_eq!(w.rec()["turns"], json!(2));
}

#[test]
fn an_answered_end_ends_the_planning_session() {
    let w = World::new("answer-end");
    w.planning();
    w.comments(7, &plan_body(" ", " ", " ", 1, ""), "", json!([]));
    answered(&w, "alice", &["end"]);
    w.poll();
    assert_eq!(w.plan_calls(), "");
    has(&w.gh(), "Planning session ended by alice.");
    assert_eq!(w.rec_open(), "none");
}

#[test]
fn an_answer_in_words_is_the_reply_the_turn_is_given() {
    let w = World::new("answer-text");
    w.planning();
    w.comments(7, &plan_body(" ", " ", " ", 1, ""), "", json!([]));
    let file = w.t().join("reply.md");
    fs::write(&file, "Split it in two.\n").unwrap();
    answered(
        &w,
        "alice",
        &["text", "--text-file", file.to_str().unwrap()],
    );
    w.poll();
    assert_eq!(
        turn_text(&w),
        "The operator replied on the issue:\n\nSplit it in two."
    );
}

#[test]
fn a_persons_tick_after_the_answer_stands_and_the_answer_goes() {
    let w = World::new("answer-later-tick");
    // The answer is given before the tick of the fixture (01:00).
    w.parked("failed", "alice");
    w.store_at(
        1767225600,
        &["answer-record", "7", "--by", "alice", "--choice", "park"],
    );
    w.comments(7, &park_body(&["x", " "]), "alice", json!([]));
    let r = w.poll();
    has(
        &r.out,
        "the answer of alice is no answer: alice answered the question after it",
    );
    assert_eq!(started(&w), "7");
    assert_eq!(waiting(&w), "none");
}

#[test]
fn a_box_ticked_before_the_answer_is_unticked_with_it() {
    let w = World::new("answer-earlier-tick");
    w.parked("failed", "alice");
    // The fixture's tick (01:00) is before an answer given now.
    w.comments(7, &park_body(&[" ", "x"]), "alice", json!([]));
    answered(&w, "alice", &["retry"]);
    w.poll();
    assert_eq!(started(&w), "7");
    has(&w.gh(), "PATCH");
    assert_eq!(waiting(&w), "none");
}

#[test]
fn a_refused_answer_leaves_a_valid_tick_to_count() {
    let w = World::new("answer-refused-tick");
    w.parked("failed", "alice");
    w.comments(7, &park_body(&["x", " "]), "alice", json!([]));
    answered(&w, "alice", &["retry-text"]);
    let r = w.poll();
    has(&r.out, "which its question does not offer");
    assert_eq!(started(&w), "7");
    has(&r.out, "start #7 (retry ticked by alice)");
}
