//! Issue #2214: a planning turn whose issue was closed is dropped.

mod common;

use common::{has, lacks, plan_body, World};
use serde_json::json;
use std::fs;

/// A planning turn that ended and waits to be posted, and an empty queue: the
/// poll has one thing to do, post that turn or drop it. The record is set
/// pending here, as a turn on a running or closed-out session is not
/// scheduled for the post.
fn turn_pending(w: &World) {
    w.reset();
    for f in ["ready.json", "plans.json", "asking.json"] {
        w.fx(f, "[]");
    }
    w.t_plan(
        7,
        json!({"session": "abc-123", "turns": 1, "pending": true, "comment": null}),
    );
    // The outcome the turn handed back, where the post reads it.
    let record = w.record().to_string_lossy().into_owned();
    fs::copy(w.fx.join("plan-result.json"), format!("{record}.out")).unwrap();
    fs::write(format!("{record}.rc"), "0\n").unwrap();
    w.clear_calls();
}

#[test]
fn a_turn_that_ends_after_its_issue_was_closed_posts_nothing_and_its_session_ends() {
    let w = World::new("closed-issue");
    turn_pending(&w);
    w.fx("state-7.json", r#"{"state":"CLOSED"}"#);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    // Nothing of the turn reached GitHub: no outcome, no boxes, no question label.
    lacks(&w.gh(), "kind=plan issue=7 turn=1");
    lacks(&w.gh(), "--add-label tehas:question");
    has(&r.out, "planning #7 dropped: the issue is closed");
    assert_eq!(w.rec_open(), "none");
}

#[test]
fn a_waiting_turn_of_a_closed_issue_is_dropped_with_no_line_of_its_own() {
    let w = World::new("closed-issue");
    turn_pending(&w);
    // The question of the first turn went out and the issue carries its
    // label: what remains is the write of the plan's boxes.
    w.fx(
        "asking.json",
        r#"[{"number":7,"labels":[{"name":"tehas:question"}]}]"#,
    );
    w.comments_of(7, 4001, &plan_body(" ", "x", " ", 1, ""), "tehas-bot");
    w.fx("state-7.json", r#"{"state":"CLOSED"}"#);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    lacks(&w.gh(), "issue edit 7 --repo acme/widget --body");
    has(&r.out, "planning #7 dropped: the issue is closed");
    assert_eq!(w.rec_open(), "none");
}

#[test]
fn a_turn_that_ends_while_its_issue_is_open_posts_as_before() {
    let w = World::new("closed-issue");
    turn_pending(&w);
    w.fx("state-7.json", r#"{"state":"OPEN"}"#);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&w.gh(), "<!-- tehas:question kind=plan issue=7 turn=1 -->");
    lacks(&r.out, "dropped: the issue is closed");
}
