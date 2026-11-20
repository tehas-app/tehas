//! The planning session on the state store: what survives a turn (B8), which
//! question an answer counts in (F5), an open session holding its issue out
//! of other work (A13), a session replaced (B4), and a store that does not
//! answer (D2).

mod common;

use common::*;
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::path::Path;

fn v(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{e}: {text}"))
}

/// `filter` of #7's session on record, through jq.
fn rec_jq(w: &World, filter: &str) -> Value {
    v(&jq(filter, &w.rec().to_string()))
}

/// `filter` of what a read verb of the store answers, through jq.
fn row_jq(w: &World, args: &[&str], filter: &str) -> Value {
    v(&jq(filter, &w.row(args).to_string()))
}

fn known(w: &World) -> Value {
    let q = w.row(&["question-known", "7", "--id", "4001"]);
    json!([q["known"], q["answered"]])
}

/// The files under `dir`, at any depth, whose names pass `pick`.
fn found(dir: &Path, pick: &dyn Fn(&str) -> bool) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| {
            let p = e.path();
            usize::from(pick(&e.file_name().to_string_lossy()))
                + if p.is_dir() { found(&p, pick) } else { 0 }
        })
        .sum()
}

/// The real store again, on the same database.
fn store_back(w: &World) {
    let _ = fs::remove_file(w.t().join("store-answers"));
}

/// How many questions naming `turn` this run posted.
fn posted_turn(w: &World, turn: u64) -> usize {
    let mark = format!("<!-- tehas:question kind=plan issue=7 turn={turn} -->");
    w.gh().lines().filter(|l| l.contains(&mark)).count()
}

fn said(w: &World, text: &str) -> usize {
    w.gh().lines().filter(|l| l.contains(text)).count()
}

fn record_file(w: &World, ext: &str) -> std::path::PathBuf {
    let mut p = w.record().into_os_string();
    p.push(ext);
    p.into()
}

fn answer_line(w: &World) -> String {
    format!(
        "--session abc-123 --answer {}/answer.md",
        w.keep().display()
    )
}

#[test]
fn b8_the_brief_the_session_and_what_it_filed_survive_a_failed_turn() {
    let w = World::new("plan-store");
    w.planning();
    w.t_plan(
        7,
        json!({"turns": 1, "options": ["Adopt all", "Change some"], "comment": 4001,
            "filed": ["https://github.com/acme/widget/issues/41"]}),
    );
    w.comments(7, &plan_body(" ", "x", " ", 1, ""), "alice", json!([]));
    w.fx("plan-fails", "{\"session\":\"abc-123\"}\n");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&w.gh(), "This planning turn did not finish");
    assert_eq!(
        rec_jq(&w, "[.session, .turns, .failed, .filed]"),
        json!([
            "abc-123",
            2,
            true,
            ["https://github.com/acme/widget/issues/41"]
        ])
    );
    assert_eq!(w.rec_brief().lines().next(), Some("# Faster search"));
    assert_eq!(
        row_jq(
            &w,
            &["history-show", "7", "--kind", "plan"],
            ".rows[-1] | [.turn, .outcome, .rc, .session]"
        ),
        json!([2, "failed", 2, "abc-123"])
    );
}

#[test]
fn b8_a_turn_that_exits_0_with_no_session_in_its_outcome_is_a_failed_turn_of_the_session_on_record()
{
    let w = World::new("plan-store");
    w.planning();
    w.comments(7, &plan_body(" ", "x", " ", 1, ""), "alice", json!([]));
    w.fx(
        "plan-result.json",
        "{\"text\":\"Two ways.\",\"question\":null}\n",
    );
    w.poll();
    has(
        &w.gh(),
        "This planning turn did not finish: it exited with status 0.",
    );
    assert_eq!(
        rec_jq(&w, "[.session, .turns, .failed, .running, .options]"),
        json!(["abc-123", 2, true, false, []])
    );
    assert_eq!(w.rec_open(), "open");
}

#[test]
fn b8_a_session_pi_rukas_lost_ends_with_outcome_lost_and_stays_open_until_its_labels_come_off() {
    let w = World::new("plan-store");
    w.planning();
    w.comments(7, &plan_body("x", " ", " ", 1, ""), "alice", json!([]));
    w.fx("plan-fails", "{\"session\":\"abc-123\",\"lost\":true}\n");
    w.fx(
        "edit-fails-on",
        "--remove-label tehas:question --remove-label tehas:planning\n",
    );
    w.poll();
    let history = ["history-show", "7", "--kind", "plan"];
    assert_eq!(
        row_jq(&w, &history, ".rows[-1] | [.turn, .outcome]"),
        json!([2, "lost"])
    );
    assert_eq!(rec_jq(&w, "[.lost, .running]"), json!([true, true]));
    assert_eq!(w.rec_open(), "open");
    w.clear_calls();
    w.rm_fx("edit-fails-on");
    w.rm_fx("plan-fails");
    w.poll();
    has(&w.gh(), "pi-rukas has no record of it on");
    assert_eq!(w.rec_open(), "none");
    // One end on record for the one turn.
    assert_eq!(
        row_jq(&w, &history, "[.rows[] | select(.turn == 2)] | length"),
        json!(1)
    );
}

#[test]
fn b8_a_lost_session_whose_close_the_store_would_not_take_is_said_once_by_the_poll_that_closes_it()
{
    let w = World::new("plan-store");
    w.planning();
    w.comments(7, &plan_body("x", " ", " ", 1, ""), "alice", json!([]));
    w.fx("plan-fails", "{\"session\":\"abc-123\",\"lost\":true}\n");
    w.store_refuses(
        "plan-end 7 --close",
        1,
        "store: busy #7: the database is locked",
    );
    w.poll();
    lacks(&w.gh(), "pi-rukas has no record of it on");
    assert_eq!(w.rec_open(), "open");
    let mut n = 0;
    for _ in 0..2 {
        store_back(&w);
        w.clear_calls();
        w.rm_fx("plan-fails");
        w.poll();
        n += said(&w, "pi-rukas has no record of it on");
    }
    assert_eq!(n, 1);
    assert_eq!(w.rec_open(), "none");
}

#[test]
fn f5_once_a_turn_begins_the_question_it_answered_is_answered_on_record_and_a_tick_in_it_runs_no_second_turn(
) {
    let w = World::new("plan-store");
    w.planning();
    w.comments(7, &plan_body(" ", "x", " ", 1, ""), "alice", json!([]));
    w.fx("plan-runs", "7\n");
    w.poll();
    assert_eq!(w.plan_calls(), "");
    has(&w.calls("fake-plan-start"), "--session abc-123");
    assert_eq!(known(&w), json!([false, true]));
    w.rm_fx("plan-runs");
    w.clear_calls();
    // The turn is still on record as running: the same tick is read again.
    assert_eq!(w.rec()["running"], json!(true));
    let lock = OpenOptions::new()
        .create(true)
        .append(true)
        .open(record_file(&w, ".lock"))
        .unwrap();
    lock.try_lock().unwrap();
    let r = w.poll();
    drop(lock);
    assert_eq!(w.calls("fake-plan-start"), "");
    has(
        &r.out,
        "skip #7: the question answered is not the one this session is waiting on",
    );
    assert_eq!(w.rec()["turns"], json!(1));
}

#[test]
fn a13_a_turn_running_on_record_is_not_begun_again_by_a_new_tehas_plan_and_the_poll_starts_nothing()
{
    let w = World::new("plan-store");
    w.to_plan();
    w.fx(
        "ready.json",
        r#"[{"number":8,"labels":[{"name":"tehas:ready"}]}]"#,
    );
    w.fx(
        "events-8.json",
        r#"[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"}}]"#,
    );
    w.t_plan(7, json!({"turns": 0, "running": true}));
    // The first turn was cut off, and its issue cannot be handed to a person yet.
    w.fx("edit-fails-on", "--add-label needs-human-attention");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "skip #7: its planning session could not begin: store: running",
    );
    assert_eq!(w.plan_calls(), "");
    lacks(&w.gh(), "--add-label tehas:planning");
    assert_eq!(w.calls("fake-start"), "");
    assert_eq!(w.rec()["running"], json!(true));
    assert_eq!(
        found(&w.home.join("tmp"), &|f| f.starts_with("plan-7.brief.")),
        0
    );
}

#[test]
fn a13_open_sessions_that_cannot_be_read_hold_every_start_busy_is_exit_1_a_store_that_cannot_be_used_78(
) {
    let w = World::new("plan-store");
    for (rc, line) in [
        (1, "store: busy #0: the database is locked"),
        (78, "store: corrupt: the file is not a database"),
    ] {
        w.reset();
        w.store_refuses("plan-show --open", rc, line);
        let r = w.poll();
        assert_eq!(r.rc, rc, "{}", r.out);
        has(
            &r.out,
            "ERROR: the state store could not list the open planning sessions",
        );
        assert_eq!(w.calls("fake-start"), "");
        lacks(&w.gh(), "issue edit");
    }
}

#[test]
fn a13_when_the_open_sessions_cannot_be_read_again_after_a_planning_write_no_issue_starts() {
    let w = World::new("plan-store");
    // #9's session is over: its issue no longer asks, so the poll ends it, and
    // reads the open sessions again before #7 could start.
    w.open_read_fails_again();
    w.poll();
    assert!(w.row(&["plan-show", "9"])["planning"]["ended_at"].is_number());
}

#[test]
fn the_keep_directory_a_turn_is_given_is_beside_the_workspace_not_in_the_state_directory() {
    let w = World::new("plan-store");
    w.to_plan();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &w.plan_calls(),
        &format!("--keep {}/work/widget-plan-7.keep ", w.home.display()),
    );
    lacks(&w.plan_calls(), "/state/plan-7.json.keep");
    assert_eq!(found(&w.home.join("state"), &|f| f.ends_with(".keep")), 0);
}

#[test]
fn a_later_turn_writes_no_answer_into_a_work_directory_tehas_did_not_make_and_its_record_is_put_back(
) {
    let w = World::new("plan-store");
    w.planning();
    w.comments(7, &plan_body(" ", "x", " ", 1, ""), "alice", json!([]));
    let work = w.home.join("work");
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).unwrap();
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    assert_eq!(w.plan_calls(), "");
    has(&r.out, "work exists and was not made by tehas");
    assert!(!w.keep().exists());
    assert_eq!(rec_jq(&w, "[.turns, .running]"), json!([1, false]));
    fs::remove_dir_all(&work).unwrap();
    w.poll();
    has(&w.plan_calls(), &answer_line(&w));
    assert!(work.join(".tehas-workspaces").is_file());
}

#[test]
fn b4_a_new_tehas_plan_replaces_a_session_with_its_answers_what_it_filed_and_its_notes() {
    let w = World::new("plan-store");
    w.to_plan();
    // The session waits for an answer nobody has given.
    w.fx("asking.json", r#"[{"number":7}]"#);
    w.comments(7, &plan_body(" ", " ", " ", 2, ""), "", json!([]));
    w.t_plan(
        7,
        json!({"turns": 2, "options": ["Adopt all"], "comment": 4001,
            "answers": [{"turn": 2, "by": "bob", "fixer": false}],
            "filed": ["https://github.com/acme/widget/issues/41"], "held": [4001],
            "fixer_refused": [4003]}),
    );
    assert_eq!(
        rec_jq(
            &w,
            "[.held, .fixer_refused, (.filed | length), (.answers | length)]"
        ),
        json!([[4001], [4003], 1, 1])
    );
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        rec_jq(
            &w,
            "[.turns, .answers, .filed, .held, .fixer_refused, .comment]"
        ),
        json!([1, [], null, null, null, 4002])
    );
    // The question of the session it replaced is off the record.
    assert_eq!(
        w.row(&["question-known", "7", "--id", "4001"])["recorded"],
        json!(false)
    );
}

#[test]
fn b4_a_session_ended_and_then_asked_for_again_starts_with_nothing_of_the_old_one() {
    let w = World::new("plan-store");
    w.planning();
    w.t_plan(
        7,
        json!({"turns": 1, "options": ["Adopt all"], "comment": 4001,
            "filed": ["https://github.com/acme/widget/issues/41"]}),
    );
    w.store_ok(&["plan-end", "7", "--close"]);
    w.fx("asking.json", "[]");
    w.fx(
        "plans.json",
        r#"[{"number":7,"labels":[{"name":"tehas:plan"}]}]"#,
    );
    w.fx(
        "events-7.json",
        r#"[{"event":"labeled","label":{"name":"tehas:plan"},"actor":{"login":"alice"}}]"#,
    );
    w.fx(
        "issue-7.json",
        r#"{"title":"Faster search","body":"Make it fast."}"#,
    );
    w.poll();
    assert_eq!(
        rec_jq(&w, "[.turns, .filed, .answers]"),
        json!([1, null, []])
    );
}

#[test]
fn d2_an_end_of_a_turn_the_store_will_not_take_keeps_the_exit_status_and_the_next_poll_posts_it_once(
) {
    let w = World::new("plan-store");
    w.planning();
    w.fx("asking.json", "[]");
    w.t_plan(
        7,
        json!({"turns": 1, "options": ["Adopt all", "Change some"], "running": true}),
    );
    fs::copy(w.fx.join("plan-result.json"), record_file(&w, ".out")).unwrap();
    fs::write(record_file(&w, ".rc"), "0\n").unwrap();
    w.store_refuses(
        "plan-end 7 --turn 2",
        1,
        "store: busy #7: the database is locked",
    );
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "WARN: the end of the planning turn of #7 could not be recorded (busy); the next poll tries again",
    );
    assert_eq!(
        fs::read_to_string(record_file(&w, ".rc"))
            .unwrap_or_default()
            .trim_end(),
        "0"
    );
    assert_eq!(posted_turn(&w, 2), 0);
    assert_eq!(w.rec()["running"], json!(true));
    store_back(&w);
    w.clear_calls();
    w.poll();
    assert_eq!(posted_turn(&w, 2), 1);
    assert!(!record_file(&w, ".rc").exists());
    assert_eq!(
        rec_jq(&w, "[.turns, .running, .pending]"),
        json!([2, false, false])
    );
    w.clear_calls();
    w.poll();
    assert_eq!(posted_turn(&w, 2), 0);
}

#[test]
fn d2_an_answer_whose_turn_the_store_will_not_begin_marks_nothing_and_leaves_the_answer_to_the_next_poll(
) {
    let w = World::new("plan-store");
    w.planning();
    w.comments(7, &plan_body(" ", "x", " ", 1, ""), "alice", json!([]));
    w.store_refuses("plan-answer 7", 1, "store: busy #7: the database is locked");
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "ERROR: could not write the record of #7, not starting a turn (busy)",
    );
    assert_eq!(w.plan_calls(), "");
    lacks(&w.gh(), "--add-label tehas:planning");
    assert_eq!(
        found(&w.home.join("tmp"), &|f| f.starts_with("plan-answer.")),
        0
    );
    store_back(&w);
    w.poll();
    has(&w.plan_calls(), &answer_line(&w));
}

// The decision is a fixture case too; this holds what a store that cannot be
// used does to the poll.
#[test]
fn f5_a_question_whose_record_could_not_be_read_is_neither_answered_nor_unticked() {
    let w = World::new("plan-store");
    w.planning();
    w.comments(7, &plan_body("x", "x", " ", 1, ""), "alice", json!([]));
    w.store_refuses("plan-show 7", 1, "store: busy #7: the database is locked");
    let r = w.poll();
    assert_eq!(w.plan_calls(), "");
    has(
        &r.out,
        "skip #7: its question on record could not be read (busy)",
    );
    lacks(&w.gh(), "api -X PATCH");
    w.store_refuses(
        "question-known 7",
        78,
        "store: corrupt: the file is not a database",
    );
    let r = w.poll();
    assert_eq!(r.rc, 78, "{}", r.out);
    lacks(&w.gh(), "api -X PATCH");
}
