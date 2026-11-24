//! The question on a parked issue: retry and leave it parked, as the poll
//! writes them to GitHub and the store. The decisions are fixtures in
//! tests/fixtures/park.json.

mod common;

use common::{has, lacks, park_body, World};
use serde_json::{json, Value};
use std::fs;
use std::process::{Command, Stdio};
use tehas_ports::{Act, Mark, State};

/// The issues a cycle was started for, one per line.
fn started(w: &World) -> String {
    w.calls("fake-start").trim_end().to_string()
}

/// jq -r of `filter` over `input`.
fn jqr(filter: &str, input: &str) -> String {
    use std::io::Write;
    let mut c = Command::new("jq")
        .args(["-r", filter])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    c.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let out = c.wait_with_output().unwrap();
    String::from_utf8_lossy(&out.stdout)
        .trim_end_matches('\n')
        .to_string()
}

/// t_row: a read verb of the store through a jq filter.
fn row(w: &World, args: &[&str], filter: &str) -> String {
    jqr(filter, &w.row(args).to_string())
}

/// #7's current park through `filter`, as a string.
fn park_of(w: &World, filter: &str) -> String {
    jqr(&format!("{filter} | tostring"), &w.park_of().to_string())
}

/// #7 parked with its question ticked by `editor`.
fn ticked(w: &World, boxes: &[&str], editor: &str) {
    w.parked("failed", "alice");
    w.comments(7, &park_body(boxes), editor, json!([]));
}

#[test]
fn a_retry_ticked_by_an_allowed_account_starts_a_cycle() {
    let w = World::new("park-retry");
    ticked(&w, &["x", " "], "alice");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(started(&w), "7");
    assert!(
        w.on(7, Mark::State(State::Running)) && w.off(7, Mark::State(State::Question)),
        "{:?}",
        w.writes_of(7)
    );
    // The cycle's --restart takes it off, through pi-rukas.
    lacks(&w.gh(), "--remove-label needs-human-attention");
    has(&w.gh(), "authorized by @alice's reply");
    has(&r.out, "start #7 (retry answered by alice)");
}

/// A retry tick on an issue that also carries tehas:ready and the escalation
/// label, one poll.
fn retry_with_labels(w: &World) {
    w.parked("failed", "alice");
    w.fx(
        "asking.json",
        r#"[{"number":7,"labels":[{"name":"needs-human-attention"},{"name":"tehas:question"},{"name":"tehas:ready"},{"name":"tehas:millwright-escalated"}]}]"#,
    );
    w.comments(7, &park_body(&["x", " "]), "alice", json!([]));
    w.poll();
}

#[test]
fn a_retry_tick_takes_tehas_ready_and_the_escalation_label_off() {
    let w = World::new("park-labels");
    retry_with_labels(&w);
    assert_eq!(started(&w), "7");
    assert!(
        w.off(7, Mark::State(State::Question))
            && w.off(7, Mark::Act(Act::Approve))
            && w.off(7, Mark::State(State::Escalated)),
        "{:?}",
        w.writes_of(7)
    );
}

#[test]
fn a_retry_tick_writes_the_approval_record_with_the_ticks_time_before_the_label_changes() {
    let w = World::new("park-record");
    retry_with_labels(&w);
    assert_eq!(
        w.at_mark("[.by, .how, .at, .comment_id] | tostring"),
        r#"["alice","reply","2026-01-01T01:00:00Z",4001]"#
    );
}

#[test]
fn the_question_that_was_answered_is_used_up_by_the_start() {
    let w = World::new("park-used");
    retry_with_labels(&w);
    assert_eq!(w.park_q(7), "none");
    assert_eq!(
        row(
            &w,
            &["question-known", "7", "--id", "4001"],
            "[.known, .recorded] | tostring"
        ),
        "[false,false]"
    );
}

#[test]
fn a_tick_counts_once_the_next_poll_finds_its_question_off_the_record_and_starts_nothing() {
    let w = World::new("park-once");
    ticked(&w, &["x", " "], "alice");
    w.touch("patch-fails");
    w.poll();
    assert_eq!(started(&w), "7");
    w.clear_calls();
    let r = w.poll();
    assert_eq!(started(&w), "");
    has(
        &r.out,
        "skip #7: a choice was made in a question this host has no record of",
    );
    assert_eq!(row(&w, &["cycle-show", "7"], ".cycle.attempt"), "2");
}

#[test]
fn a_retry_waits_while_every_slot_is_taken_and_the_tick_stays() {
    let w = World::new("park-slots");
    ticked(&w, &["x", " "], "alice");
    w.hold(9);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(started(&w), "");
    lacks(&w.gh(), "issue edit");
    assert_eq!(w.marks(None), 0);
    lacks(&w.gh(), "api -X PATCH");
    assert_eq!(w.park_q(7), "4001");
    let r = w.dispatch(&["--dry-run"]);
    has(&r.out, "holding: 1 cycles are running (limit 1)");
    w.release();
}

/// A retry whose cycle does not start: one poll.
fn start_fails(w: &World) -> common::Run {
    ticked(w, &["x", " "], "alice");
    w.touch("start-fails");
    w.poll()
}

#[test]
fn a_retry_whose_cycle_does_not_start_is_handed_back_exit_1() {
    let w = World::new("park-start-fails");
    let r = start_fails(&w);
    assert_eq!(r.rc, 1, "{}", r.out);
    assert!(
        w.off(7, Mark::State(State::Running)) && w.on(7, Mark::State(State::NeedsPerson)),
        "{:?}",
        w.writes_of(7)
    );
    has(&w.gh(), "Tehas could not start the cycle for this issue");
    // The start took the question off the record and spent the park's retry
    // before it marked the issue: the park asks for nothing.
    assert_eq!(park_of(&w, "[.question, .retries]"), "[null,1]");
}

#[test]
fn the_comments_are_read_a_hundred_at_a_time() {
    let w = World::new("park-hundred");
    start_fails(&w);
    has(&w.gh(), "comments(last: 100)");
}

#[test]
fn a_dry_run_says_what_the_tick_would_do_and_changes_nothing() {
    let w = World::new("park-dry");
    ticked(&w, &["x", " "], "alice");
    let r = w.dispatch(&["--dry-run"]);
    has(&r.out, "would run a cycle for #7 (retry answered by alice)");
    assert_eq!(started(&w), "");
    lacks(&w.gh(), "issue edit");
    assert_eq!(w.marks(None), 0);
    lacks(&w.gh(), "api -X PATCH");
    assert_eq!(w.park_q(7), "4001");
}

#[test]
fn a_retry_answered_by_the_bot_or_a_stranger_starts_nothing() {
    let w = World::new("park-stranger");
    for other in ["tehas-bot", "mallory"] {
        ticked(&w, &["x", " "], other);
        w.poll();
        assert_eq!(started(&w), "", "{other}");
        lacks(&w.gh(), "api -X PATCH");
    }
}

#[test]
fn a_retry_answered_by_a_listed_login_with_another_user_id_starts_nothing() {
    let w = World::new("park-user-id");
    for id in [json!(99), Value::Null] {
        ticked(&w, &["x", " "], "alice");
        let path = w.fx.join("comments-7.json");
        let mut v: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        v["data"]["repository"]["issue"]["comments"]["nodes"][1]["author"]["databaseId"] =
            id.clone();
        w.fx_json("comments-7.json", &v);
        let r = w.poll();
        assert_eq!(started(&w), "", "{id}");
        has(&r.out, "does not count");
    }
}

/// #2782: a box ticked in a comment of before 2.0 does nothing at all: no
/// cycle starts, no answer is recorded, and nothing is posted.
#[test]
fn a_box_ticked_on_an_old_comment_does_nothing() {
    let w = World::new("park-tick");
    // Two boxes ticked, or one with nobody's reply: neither is an answer.
    for boxes in [&["x", " ", "x"][..], &["x", " ", " "]] {
        w.parked("failed", "alice");
        w.comments(7, &park_body(boxes), "", json!([]));
        let r = w.poll();
        assert_eq!(r.rc, 0, "{}", r.out);
        assert_eq!(started(&w), "", "{boxes:?}");
        let gh = w.gh();
        assert_eq!(w.notes_any(), 0);
        lacks(&gh, "issue edit");
        assert_eq!(w.marks(None), 0);
        lacks(&gh, "api -X PATCH");
        assert_eq!(w.park_q(7), "4001");
        assert_eq!(
            row(
                &w,
                &["question-known", "7", "--id", "4001"],
                "[.answered, .answer] | tostring"
            ),
            "[false,null]"
        );
    }
}

#[test]
fn the_issue_alone_reply_by_an_allowed_account_starts_a_cycle_with_how_reply() {
    let w = World::new("park-alone");
    ticked(&w, &[" ", "x", " "], "alice");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(started(&w), "7");
    assert_eq!(
        w.at_mark("[.by, .how, .at, .comment_id] | tostring"),
        r#"["alice","reply","2026-01-01T01:00:00Z",4001]"#
    );
    assert!(
        w.on(7, Mark::State(State::Running)) && w.off(7, Mark::State(State::Question)),
        "{:?}",
        w.writes_of(7)
    );
    has(&w.gh(), "authorized by @alice's reply");
    has(&r.out, "start #7 (retry answered by alice)");
    assert_eq!(w.park_q(7), "none");
}

#[test]
fn the_issue_alone_answer_by_the_bot_or_a_stranger_starts_nothing() {
    let w = World::new("park-alone-stranger");
    for other in ["tehas-bot", "mallory"] {
        ticked(&w, &[" ", "x", " "], other);
        w.poll();
        assert_eq!(started(&w), "", "{other}");
        lacks(&w.gh(), "api -X PATCH");
    }
}

#[test]
fn both_retry_boxes_ticked_answer_nothing() {
    let w = World::new("park-both-retry");
    ticked(&w, &["x", "x"], "alice");
    w.poll();
    assert_eq!(started(&w), "");
    assert_eq!(w.notes_any(), 0);
    lacks(&w.gh(), "api -X PATCH");
}

#[test]
fn a_retry_tick_of_the_issue_alone_waits_while_every_slot_is_taken_and_the_tick_stays() {
    let w = World::new("park-alone-slots");
    ticked(&w, &[" ", "x", " "], "alice");
    w.hold(9);
    w.poll();
    assert_eq!(started(&w), "");
    lacks(&w.gh(), "issue edit");
    assert_eq!(w.marks(None), 0);
    assert_eq!(w.park_q(7), "4001");
    w.release();
}

#[test]
fn a_dry_run_says_what_the_issue_alone_tick_would_do_and_changes_nothing() {
    let w = World::new("park-alone-dry");
    ticked(&w, &[" ", "x", " "], "alice");
    let r = w.dispatch(&["--dry-run"]);
    has(&r.out, "would run a cycle for #7 (retry answered by alice)");
    assert_eq!(started(&w), "");
    lacks(&w.gh(), "issue edit");
    assert_eq!(w.marks(None), 0);
    assert_eq!(w.park_q(7), "4001");
}

#[test]
fn a_park_comment_of_the_old_two_boxes_still_starts_a_clean_start_when_it_is_ticked() {
    let w = World::new("park-old");
    ticked(&w, &["x", " "], "alice");
    w.poll();
    assert_eq!(started(&w), "7");
    assert_eq!(w.at_mark(".how"), "reply");
}

#[test]
fn and_its_leave_box_still_takes_the_question_away() {
    let w = World::new("park-old-leave");
    ticked(&w, &[" ", "x"], "alice");
    w.poll();
    assert_eq!(started(&w), "");
    assert!(
        w.off(7, Mark::State(State::Question)),
        "{:?}",
        w.writes_of(7)
    );
    assert_eq!(w.park_q(7), "none");
}

#[test]
fn leave_it_parked_takes_the_question_away_and_keeps_the_issue_parked() {
    let w = World::new("park-leave");
    ticked(&w, &[" ", " ", "x"], "alice");
    w.poll();
    assert_eq!(started(&w), "");
    assert!(
        w.off(7, Mark::State(State::Question)),
        "{:?}",
        w.writes_of(7)
    );
    lacks(&w.gh(), "needs-human-attention");
    assert_eq!(w.park_q(7), "none");
    // Answered by the person who ticked it, and on record as such.
    assert_eq!(
        row(
            &w,
            &["question-known", "7", "--id", "4001"],
            "[.known, .recorded, .answered] | tostring"
        ),
        "[false,true,true]"
    );
}

#[test]
fn leave_it_parked_keeps_the_question_on_record_while_its_label_cannot_be_removed() {
    let w = World::new("park-leave-label");
    ticked(&w, &[" ", " ", "x"], "alice");
    w.touch("edit-fails");
    let r = w.poll();
    assert_eq!(w.park_q(7), "4001");
    has(
        &r.out,
        "WARN: label change on #7 failed: --remove-label tehas:question",
    );
}

#[test]
fn a_leave_it_parked_whose_answer_the_store_would_not_take_is_said_in_the_log() {
    let w = World::new("park-leave-store");
    ticked(&w, &[" ", " ", "x"], "alice");
    w.store_refuses(
        "question-answered 7",
        1,
        "store: busy #7: the database is locked",
    );
    let r = w.poll();
    assert!(
        w.off(7, Mark::State(State::Question)),
        "{:?}",
        w.writes_of(7)
    );
    has(
        &r.out,
        "WARN: could not record the answer to the question of #7 (busy)",
    );
}

#[test]
fn a_dry_run_of_leave_it_parked_changes_nothing() {
    let w = World::new("park-leave-dry");
    ticked(&w, &[" ", " ", "x"], "alice");
    w.dispatch(&["--dry-run"]);
    lacks(&w.gh(), "issue edit");
    assert_eq!(w.marks(None), 0);
    assert_eq!(w.park_q(7), "4001");
}

/// One comment after the question, as GitHub's GraphQL gives it.
macro_rules! later_comment {
    ($id:expr, $by:expr, $body:expr, $at:expr $(,)?) => {{
        let mut c = json!({"databaseId": $id, "author": {"login": $by, "databaseId": common::uid($by)},
            "body": $body, "lastEditedAt": null, "editor": null,
            "userContentEdits": {"totalCount": 0, "nodes": []}});
        c[common::replies::CREATED] = json!($at);
        c
    }};
}

/// A newer reply that names a choice the stop does not offer (`retry`, where
/// only the text-only retry is offered) is declined with the note and acts
/// on nothing; the older reply's choice does not act in its place. Once the
/// note follows them, neither is read again.
#[test]
fn a_reply_naming_a_choice_the_stop_does_not_offer_is_declined_and_nothing_starts() {
    let w = World::new("park-unoffered");
    w.parked("failed", "alice");
    let body = tehas_core::question::q_render(
        tehas_core::question::Kind::Park,
        7,
        1,
        &json!({"text": "Tehas cycle stopped.", "comments": false}),
        "",
    );
    let replies = vec![
        later_comment!(4002, "alice", "retry-text", "2026-01-01T01:00:00Z"),
        later_comment!(4003, "alice", "retry", "2026-01-01T02:00:00Z"),
    ];
    w.comments(7, &body, "", json!(replies));
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(started(&w), "");
    has(
        &r.out,
        "skip #7: alice answered a choice its question does not offer",
    );
    let note = "The reply names a choice this question does not offer, so nothing was started.";
    has(&w.gh(), note);
    let mark = tehas_dispatch::questions::declined_marker("4001");
    has(&w.gh(), &mark);
    assert_eq!(w.park_q(7), "4001");
    // The next poll finds the note after both replies: nothing is said again.
    let mut replies = replies;
    replies.push(later_comment!(
        4004,
        "tehas-bot",
        &format!("{note}\n\n{mark}"),
        "2026-01-01T03:00:00Z",
    ));
    w.comments(7, &body, "", json!(replies));
    w.clear_calls();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(started(&w), "");
    assert_eq!(w.notes(7), 0);
    lacks(&r.out, "does not offer");
}

/// A park question whose sum on record is of before T4.7, the comment as it
/// was posted with its markers in, is adopted on the poll's first read (the
/// lead's ruling): the store keeps the sum of its visible text, the poll says
/// so once, nothing is written to the code host and nothing asked again,
/// and the answer in it is read as it is.
#[test]
fn a_question_summed_before_2_0_is_adopted_on_its_first_read() {
    use tehas_core::question::{q_text, question_sum};
    let w = World::new("park-adopt");
    for ticks in [[" ", " "], ["x", " "]] {
        // 1.x's park records its question with the sum of its comment.
        let body = park_body(&ticks);
        let raw = tehas_core::approval::body_sum(&park_body(&[" ", " "]));
        let args = ["--comment-id", "4001", "--body-sha", &raw];
        w.parked_with("failed", "alice", "-", &args);
        let known = || w.row(&["question-known", "7", "--id", "4001"]);
        assert_eq!(known()["sum_form"], "rendered");
        w.comments(7, &body, "alice", json!([]));
        let r = w.poll();
        assert_eq!(r.rc, 0, "{}", r.out);
        has(
            &r.out,
            "INFO: the question of #7 (comment 4001) had its sum of before 2.0",
        );
        if ticks[0] == "x" {
            // The tick counts in the same poll, which uses the question up.
            assert_eq!(started(&w), "7");
        } else {
            assert_eq!(known()["sum_form"], "visible");
            let sum = question_sum(&q_text(&body, 7).unwrap());
            assert_eq!(
                w.row(&["question-show", "7"])["question"]["body_sha256"],
                json!(sum)
            );
            lacks(&w.gh(), "api -X PATCH");
            assert_eq!(started(&w), "");
            assert!(w.writes_of(7).is_empty(), "{:?}", w.writes_of(7));
            // The next poll finds it adopted and says nothing.
            w.clear_calls();
            let r = w.poll();
            assert_eq!(r.rc, 0, "{}", r.out);
            lacks(&r.out, "had its sum of before 2.0");
        }
    }
}
