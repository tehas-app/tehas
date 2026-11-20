//! Starting a planning turn, its record, and the turns that fail or are cut
//! off.

mod common;

use common::*;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

/// `filter` of #7's session on record, through jq.
fn rec_jq(w: &World, filter: &str) -> Value {
    v(&jq(filter, &w.rec().to_string()))
}

fn v(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{e}: {text}"))
}

/// The record of #7 as the fake gh saw it at the mark, or the fake plan at
/// the turn.
fn seen(w: &World, name: &str) -> Value {
    let text = fs::read_to_string(w.t().join("calls").join(name)).unwrap_or_default();
    serde_json::from_str(&text).unwrap_or(Value::Null)
}

fn known(w: &World) -> Value {
    let q = w.row(&["question-known", "7", "--id", "4001"]);
    json!([q["known"], q["answered"]])
}

fn briefs(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| {
            let p = e.path();
            let own = usize::from(e.file_name().to_string_lossy().starts_with("plan-7.brief."));
            own + if p.is_dir() { briefs(&p) } else { 0 }
        })
        .sum()
}

fn turn_line(w: &World) -> String {
    let record = w.record().display().to_string();
    let keep = w.keep().display().to_string();
    format!("7 --out {record}.out --keep {keep} --session abc-123 --answer {keep}/answer.md")
}

#[test]
fn a_session_does_not_start_when_the_issue_cannot_be_marked_as_planning() {
    let w = World::new("plan-turns");
    w.to_plan();
    w.touch("edit-fails");
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    assert_eq!(w.plan_calls(), "");
    lacks(&w.gh(), "issue comment");
    has(
        &r.out,
        "ERROR: could not mark #7 as planning, not starting a turn",
    );
    assert_eq!(w.rec(), Value::Null);
    assert_eq!(briefs(&w.home.join("tmp")), 0);
}

#[test]
fn the_turn_is_on_record_as_running_before_the_issue_is_marked() {
    let w = World::new("plan-turns");
    w.to_plan();
    w.poll();
    let m = seen(&w, "record.at-mark")["planning"].clone();
    assert_eq!(
        json!([
            m["session"],
            m["turns"],
            m["options"],
            m["pending"],
            m["failed"],
            m["running"],
            m["by"],
            m["label"],
            m["comment_id"]
        ]),
        json!([null, 0, [], false, false, true, "alice", "tehas:plan", null])
    );
    assert!(m["started_at"].is_number(), "{m}");
}

#[test]
fn a_session_does_not_start_when_its_record_cannot_be_written_and_the_issue_is_not_marked() {
    let w = World::new("plan-turns");
    for (rc, line) in [
        (1, "store: busy #7: the database is locked"),
        (78, "store: corrupt: the file is not a database"),
    ] {
        w.to_plan();
        w.store_refuses("plan-begin 7", rc, line);
        let r = w.poll();
        assert_eq!(r.rc, rc, "{}", r.out);
        if rc == 1 {
            has(
                &r.out,
                "ERROR: could not write the record of #7, not starting a turn (busy)",
            );
        } else {
            has(
                &r.out,
                "ERROR: the state store cannot be used: store: corrupt",
            );
        }
        assert_eq!(w.plan_calls(), "");
        lacks(&w.gh(), "issue edit");
        lacks(&w.gh(), "issue comment");
    }
}

#[test]
fn a_later_turn_that_cannot_be_marked_leaves_the_record_as_it_was_still_answerable() {
    let w = World::new("plan-turns");
    w.planning();
    w.comments(7, &plan_body(" ", "x", " ", 1, ""), "alice", json!([]));
    let before = rec_jq(&w, "del(.startedAt)");
    w.touch("edit-fails");
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    assert_eq!(w.plan_calls(), "");
    assert_eq!(rec_jq(&w, "del(.startedAt)"), before);
    assert_eq!(
        seen(&w, "record.at-mark")["planning"]["running"],
        json!(true)
    );
    // The question is open again: the answer it had is taken back.
    assert_eq!(known(&w), json!([true, false]));
    w.clear_calls();
    w.rm_fx("edit-fails");
    w.poll();
    has(&w.plan_calls(), &turn_line(&w));
}

#[test]
fn while_a_later_turn_runs_the_record_names_no_question() {
    let w = World::new("plan-turns");
    w.planning();
    w.comments(7, &plan_body(" ", "x", " ", 1, ""), "alice", json!([]));
    w.poll();
    let t = seen(&w, "record.at-turn")["planning"].clone();
    assert_eq!(
        json!([
            t["session"],
            t["turns"],
            t["options"],
            t["pending"],
            t["failed"],
            t["running"],
            t["comment_id"],
            t["answers"]
        ]),
        json!(["abc-123", 1, ["Adopt all", "Change some"], false, false, true, null,
            [{"turn": 2, "by": "alice", "millwright": false}]])
    );
    assert!(t["started_at"].is_number(), "{t}");
    // The question answered is answered on record by whoever answered it.
    assert_eq!(known(&w), json!([false, true]));
}

#[test]
fn who_started_a_session_and_who_answered_each_turn_stay_on_record_after_the_turn() {
    let w = World::new("plan-turns");
    w.planning();
    w.t_plan(
        7,
        json!({"by": "alice", "turns": 2, "options": ["Adopt all", "Change some"], "comment": 4001,
            "answers": [{"turn": 2, "by": "bob", "fixer": false}]}),
    );
    w.comments(7, &plan_body(" ", "x", " ", 2, ""), "alice", json!([]));
    w.poll();
    assert_eq!(
        rec_jq(&w, "[.turns, .by, .answers]"),
        v(
            r#"[3,"alice",[{"turn":2,"by":"bob","fixer":false},{"turn":3,"by":"alice","fixer":false}]]"#
        )
    );
}

#[test]
fn a_turn_that_fails_keeps_them_too() {
    let w = World::new("plan-turns");
    w.planning();
    w.comments(7, &plan_body(" ", "x", " ", 1, ""), "alice", json!([]));
    w.fx("plan-fails", "{\"session\":\"abc-123\"}\n");
    w.poll();
    assert_eq!(
        rec_jq(&w, "[.failed, .by, .answers]"),
        v(r#"[true,"alice",[{"turn":2,"by":"alice","fixer":false}]]"#)
    );
}

#[test]
fn a_turn_cut_off_before_the_issue_was_marked_is_reported_like_any_cut_off_turn() {
    let w = World::new("plan-turns");
    w.planning();
    w.t_plan(
        7,
        json!({"turns": 1, "options": ["Adopt all", "Change some"], "running": true}),
    );
    // The mark never happened: the issue still asks, and the old answer is there.
    w.comments(7, &plan_body(" ", "x", " ", 1, ""), "alice", json!([]));
    w.poll();
    has(&w.gh(), "This planning turn did not finish: it was cut off");
    has(
        &w.gh(),
        "--remove-label tehas:planning --add-label tehas:question --add-label needs-human-attention",
    );
    assert_eq!(
        rec_jq(
            &w,
            "[.session, .turns, .options, .pending, .failed, .running, .comment]"
        ),
        v(r#"["abc-123",2,[],false,true,false,4002]"#)
    );
    assert_eq!(w.plan_calls(), "");
}

#[test]
fn a_first_turn_cut_off_before_the_issue_was_marked_ends_visibly_and_takes_the_plan_label_off() {
    let w = World::new("plan-turns");
    w.to_plan();
    w.t_plan(7, json!({"turns": 0, "running": true}));
    // What GitHub answers once the label is off.
    w.fx("plans.json", "[]");
    w.poll();
    has(
        &w.gh(),
        "issue edit 7 --repo acme/widget --remove-label tehas:plan --remove-label tehas:planning --add-label needs-human-attention",
    );
    has(&w.gh(), "could not start");
    assert_eq!(w.rec_open(), "none");
    assert_eq!(w.plan_calls(), "");
}

#[test]
fn a_first_turn_that_fails_parks_the_issue_and_leaves_no_session_open() {
    let w = World::new("plan-turns");
    w.to_plan();
    w.touch("plan-fails");
    w.poll();
    has(
        &w.gh(),
        "--remove-label tehas:planning --add-label needs-human-attention",
    );
    has(&w.gh(), "could not start");
    assert_eq!(w.rec_open(), "none");
    assert!(!w.keep().exists());
}

// The session ends, but stays readable until the next tehas:plan of the
// issue, so the archive of a turn that made no session is not the line of
// history alone.
#[test]
fn a_first_turn_that_fails_leaves_its_ended_session_readable_for_an_export() {
    let w = World::new("plan-turns");
    w.to_plan();
    w.touch("plan-fails");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        v(&jq(
            ".planning | [.session, .turns, .running, (.ended_at | type), .brief != null]",
            &w.row(&["plan-show", "7"]).to_string()
        )),
        v(r#"[null,1,false,"number",true]"#)
    );
    assert_eq!(
        v(&jq(
            "[.rows[] | [.turn, .outcome, .rc]]",
            &w.row(&["history-show", "7", "--kind", "plan"]).to_string()
        )),
        v(r#"[[1,"failed",2]]"#)
    );
}

#[test]
fn a_turn_of_a_session_that_went_on_keeps_the_session_open_on_record() {
    let w = World::new("plan-turns");
    w.to_plan();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.rec()["session"], json!("abc-123"));
    assert_eq!(w.rec_open(), "open");
}

/// A first turn that fails once its session exists.
fn first_turn_fails_with_a_session(w: &World) {
    w.to_plan();
    w.fx("plan-fails", "{\"session\":\"abc-123\"}\n");
    w.poll();
}

#[test]
fn a_first_turn_that_fails_after_its_session_came_into_being_keeps_the_session() {
    let w = World::new("plan-turns");
    first_turn_fails_with_a_session(&w);
    has(
        &w.gh(),
        "This planning turn did not finish: it exited with status 2.",
    );
    has(&w.gh(), "Reply in a comment to run the turn again");
    lacks(&w.gh(), "could not start");
    has(
        &w.gh(),
        "--remove-label tehas:planning --add-label tehas:question --add-label needs-human-attention",
    );
    assert_eq!(
        rec_jq(&w, "del(.startedAt)"),
        json!({"session": "abc-123", "turns": 1, "options": [], "pending": false, "failed": true,
            "running": false, "comment": 4002, "by": "alice", "label": "tehas:plan", "answers": []})
    );
}

#[test]
fn and_a_reply_runs_the_turn_again_in_that_session() {
    let w = World::new("plan-turns");
    first_turn_fails_with_a_session(&w);
    w.clear_calls();
    w.rm_fx("plan-fails");
    w.fx(
        "asking.json",
        r#"[{"number":7,"labels":[{"name":"needs-human-attention"},{"name":"tehas:question"}]}]"#,
    );
    w.fx("plans.json", "[]");
    w.fx_json(
        "comments-7.json",
        &json!({"data": {"repository": {"issue": {"comments": {"nodes": [
            {"databaseId": 4002, "author": {"login": "tehas-bot", "databaseId": uid("tehas-bot")},
             "createdAt": "2026-01-01T00:00:00Z", "lastEditedAt": null, "editor": null,
             "body": "This planning turn did not finish.\n\n- [ ] End this planning session\n\n<!-- tehas:question kind=plan issue=7 turn=1 -->"},
            {"databaseId": 4003, "author": {"login": "alice"}, "body": "go on",
             "createdAt": "2026-01-01T02:00:00Z"}]}}}}}),
    );
    w.poll();
    has(
        &w.plan_calls(),
        &format!("{} --comment 4003", turn_line(&w)),
    );
    let answer = fs::read_to_string(w.keep().join("answer.md")).unwrap_or_default();
    assert_eq!(
        answer.trim_end_matches('\n'),
        "The operator replied on the issue:\n\ngo on"
    );
}
