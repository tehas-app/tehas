//! Labels on parked and planning issues, planning turns run as units, and a
//! bot account in the allowed list.

mod common;

use common::*;
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn v(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{e}: {text}"))
}

/// `filter` of #7's session on record, through jq.
fn rec_jq(w: &World, filter: &str) -> Value {
    v(&jq(filter, &w.rec().to_string()))
}

fn beside(w: &World, ext: &str) -> PathBuf {
    let mut p = w.record().into_os_string();
    p.push(ext);
    p.into()
}

fn holding(text: &str, want: &str) -> usize {
    text.lines().filter(|l| l.contains(want)).count()
}

fn exactly(text: &str, want: &str) -> usize {
    text.lines().filter(|l| *l == want).count()
}

/// The instance lists the bot among the accounts that may label.
fn bot_allowed(w: &World) {
    let mut lines = MINIMAL.to_vec();
    lines.push("TEHAS_ALLOWED_LABELERS='alice tehas-bot'");
    w.instance(&lines);
}

/// Whether something other than this test holds the lock of #7's turn.
fn turn_lock_held(w: &World) -> bool {
    let f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(beside(w, ".lock"))
        .unwrap();
    f.try_lock().is_err()
}

/// Waits up to five seconds for `done`.
fn wait_for(done: impl Fn() -> bool) {
    for _ in 0..50 {
        if done() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// A later turn of #7 that a restart cut off.
fn cut_off_later_turn(w: &World) {
    w.planning();
    w.fx("asking.json", "[]");
    w.t_plan(
        7,
        json!({"turns": 1, "options": ["Adopt all", "Change some"], "running": true}),
    );
}

// The decision of this one and of the two below marked so is a fixture case
// too; these hold what the poll writes for it.
#[test]
fn a_parked_issue_retried_by_its_labels_starts_a_cycle_and_its_old_question_goes() {
    let w = World::new("turns");
    w.parked("failed", "alice");
    w.comments(7, &park_body(&[" ", " "]), "", json!([]));
    w.fx(
        "asking.json",
        r#"[{"number":7,"labels":[{"name":"tehas:question"}]}]"#,
    );
    w.fx(
        "ready.json",
        r#"[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"tehas:question"}]}]"#,
    );
    w.poll();
    assert_eq!(w.calls("fake-start").trim_end(), "7");
    assert_eq!(
        exactly(
            &w.gh(),
            "issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:ready --remove-label tehas:question"
        ),
        1
    );
    assert_eq!(w.park_q(7), "none");
}

#[test]
fn a_question_that_posted_but_could_not_be_labelled_is_not_lost() {
    let w = World::new("turns");
    w.planning();
    w.comments(7, &plan_body(" ", "x", " ", 1, ""), "alice", json!([]));
    w.fx("edit-fails-on", "--add-label tehas:question");
    w.poll();
    assert_eq!(rec_jq(&w, "[.comment, .pending]"), json!([4002, true]));
    assert!(fs::metadata(beside(&w, ".out"))
        .map(|m| m.len() > 0)
        .unwrap_or(false));
    w.clear_calls();
    w.rm_fx("edit-fails-on");
    w.fx("asking.json", "[]");
    w.poll();
    assert_eq!(holding(&w.gh(), "issues/7/comments"), 0);
    has(
        &w.gh(),
        "--remove-label tehas:planning --add-label tehas:question",
    );
    assert_eq!(w.rec()["pending"], json!(false));
    assert!(!beside(&w, ".out").exists());
}

#[test]
fn a_later_turn_cut_off_by_a_restart_is_reported_and_can_be_answered() {
    let w = World::new("turns");
    cut_off_later_turn(&w);
    w.poll();
    has(&w.gh(), "This planning turn did not finish: it was cut off");
    has(
        &w.gh(),
        "--add-label tehas:question --add-label needs-human-attention",
    );
    assert_eq!(
        rec_jq(&w, "[.session, .turns, .failed, .running]"),
        json!(["abc-123", 2, true, false])
    );
    assert_eq!(w.plan_calls(), "");
}

#[test]
fn the_container_of_a_cut_off_turn_is_removed_before_the_turn_is_reported() {
    let w = World::new("turns");
    cut_off_later_turn(&w);
    w.poll();
    has(&w.calls("docker"), "rm -f tehas-plan-7");
}

/// A first turn of #7 that runs as its unit, through one poll.
fn turn_runs(w: &World) -> Run {
    w.to_plan();
    w.fx("plan-runs", "7\n");
    w.poll()
}

// A fixture case too.
#[test]
fn a_poll_does_not_wait_for_a_planning_turn_a_ready_issue_starts_while_it_runs() {
    let w = World::new("turns");
    let r = turn_runs(&w);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "planning #7: turn 1 runs as tehas-plan-7.service");
    assert_eq!(w.rec()["running"], json!(true));
    lacks(&w.gh(), "kind=plan");
    w.fx(
        "ready.json",
        r#"[{"number":8,"labels":[{"name":"tehas:ready"}]}]"#,
    );
    w.fx(
        "events-8.json",
        r#"[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"}}]"#,
    );
    w.clear_calls();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.calls("fake-start").trim_end(), "8");
    assert_eq!(w.rec()["running"], json!(true));
    lacks(&w.gh(), "kind=plan");
    assert_eq!(w.plan_calls(), "");
}

#[test]
fn the_poll_after_a_turn_ends_posts_its_outcome_once_though_systemd_still_shows_its_unit() {
    let w = World::new("turns");
    turn_runs(&w);
    // The first poll took tehas:plan off the issue.
    w.fx("plans.json", "[]");
    fs::copy(w.fx.join("plan-result.json"), beside(&w, ".out")).unwrap();
    fs::write(beside(&w, ".rc"), "0\n").unwrap();
    w.clear_calls();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&w.gh(), "<!-- tehas:question kind=plan issue=7 turn=1 -->");
    assert_eq!(
        rec_jq(&w, "[.session, .turns, .running]"),
        json!(["abc-123", 1, false])
    );
    assert!(!beside(&w, ".rc").exists());
    w.rm_fx("plan-runs");
    w.clear_calls();
    w.poll();
    lacks(&w.gh(), "kind=plan");
    assert_eq!(w.plan_calls(), "");
}

#[test]
fn a_turn_s_exit_status_other_than_0_is_reported_as_a_failed_turn() {
    let w = World::new("turns");
    cut_off_later_turn(&w);
    fs::write(beside(&w, ".rc"), "2\n").unwrap();
    w.poll();
    has(
        &w.gh(),
        "This planning turn did not finish: it exited with status 2",
    );
    assert_eq!(
        rec_jq(&w, "[.turns, .failed, .running]"),
        json!([2, true, false])
    );
    lacks(&w.calls("docker"), "rm -f tehas-plan-7");
}

#[test]
fn a_turn_whose_unit_cannot_start_is_reported_as_a_failed_turn() {
    let w = World::new("turns");
    w.planning();
    w.comments(7, &plan_body(" ", "x", " ", 1, ""), "alice", json!([]));
    w.stub("fake-plan-start", "exit 1");
    let r = w.poll();
    has(&r.out, "ERROR: could not start the planning turn of #7");
    has(
        &w.gh(),
        "This planning turn did not finish: it could not be started as a unit",
    );
    assert_eq!(rec_jq(&w, "[.failed, .running]"), json!([true, false]));
}

/// A first turn that waits for fx/plan-done, run in the background as
/// systemd-run leaves a unit running, through one poll.
fn turn_that_waits(w: &World) {
    w.to_plan();
    w.stub(
        "fake-plan",
        "out=\"\"\nwhile [ \"$#\" -gt 0 ]; do [ \"$1\" = --out ] && out=$2; shift; done\n\
         [ ! -e \"/proc/$$/fd/9\" ] || : >\"$T/calls/plan.fd9\"\nn=0\n\
         while [ ! -e \"$T/fx/plan-done\" ] && [ $((n += 1)) -le 3000 ]; do sleep 0.1; done\n\
         cp \"$T/fx/plan-result.json\" \"$out\"",
    );
    w.stub(
        "fake-plan-start",
        "n=$1; shift\nsetsid \"$@\" </dev/null >/dev/null 2>&1 &",
    );
    w.rm_fx("plan-done");
    let began = Instant::now();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert!(began.elapsed() < Duration::from_secs(20));
    has(&r.out, "planning #7: turn 1 runs as tehas-plan-7.service");
    wait_for(|| turn_lock_held(w));
    assert!(turn_lock_held(w), "the turn does not hold its lock");
    let rec = w.rec();
    assert_eq!(rec["running"], json!(true));
    let at = rec["startedAt"].as_str().unwrap_or_default();
    assert!(
        regex::Regex::new(r"^[0-9-]+T[0-9:]+Z$")
            .unwrap()
            .is_match(at),
        "{at}"
    );
    // The first poll took tehas:plan off; a label put back by hand finds the
    // turn running.
    w.clear_calls();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.calls("fake-plan-start"), "");
    lacks(&w.gh(), "kind=plan");
    assert_eq!(w.rec()["running"], json!(true));
}

/// The turn of turn_that_waits() ends.
fn turn_ends(w: &World) {
    let again = w.home.join("state/dispatch.again");
    let _ = fs::remove_file(&again);
    w.touch("plan-done");
    wait_for(|| beside(w, ".rc").exists() && again.exists());
}

#[test]
fn a_turn_that_does_not_end_the_poll_returns_the_turn_holds_its_lock_and_a_second_poll_starts_no_second_turn(
) {
    let w = World::new("turns");
    turn_that_waits(&w);
    turn_ends(&w);
}

#[test]
fn a_turn_that_ends_writes_its_status_and_asks_for_a_poll_which_posts_its_outcome() {
    let w = World::new("turns");
    turn_that_waits(&w);
    turn_ends(&w);
    assert_eq!(
        fs::read_to_string(beside(&w, ".rc"))
            .unwrap_or_default()
            .trim_end(),
        "0"
    );
    assert!(w.home.join("state/dispatch.again").exists());
    assert_eq!(w.calls("plan.fd9"), "");
    w.fx("plans.json", "[]");
    w.clear_calls();
    w.poll();
    has(&w.gh(), "<!-- tehas:question kind=plan issue=7 turn=1 -->");
    assert_eq!(rec_jq(&w, "[.turns, .running]"), json!([1, false]));
}

// A fixture case too.
#[test]
fn a_first_turn_cut_off_by_a_restart_ends_visibly() {
    let w = World::new("turns");
    w.planning();
    w.fx("asking.json", "[]");
    w.t_plan(7, json!({"turns": 0, "running": true}));
    w.poll();
    has(
        &w.gh(),
        "--remove-label tehas:planning --add-label needs-human-attention",
    );
    has(&w.gh(), "could not start");
    assert_eq!(w.rec_open(), "none");
}

#[test]
fn a_first_turn_cut_off_by_a_restart_says_nothing_while_its_label_change_fails() {
    let w = World::new("turns");
    w.planning();
    w.fx("asking.json", "[]");
    w.t_plan(7, json!({"turns": 0, "running": true}));
    w.touch("edit-fails");
    let rows = |w: &World| w.row(&["history-show", "7", "--kind", "plan"])["rows"].clone();
    let lines = rows(&w).as_array().map(Vec::len);
    w.poll();
    lacks(&w.gh(), "issue comment");
    assert_eq!(w.rec()["running"], json!(true));
    // No end is on record: the next poll finds the same turn.
    assert_eq!(rows(&w).as_array().map(Vec::len), lines);
}

#[test]
fn a_record_of_a_turn_in_flight_or_waiting_to_be_posted_is_not_cleaned_up() {
    let w = World::new("turns");
    cut_off_later_turn(&w);
    w.dispatch(&["--dry-run"]);
    assert_eq!(w.rec_open(), "open");
    w.planning();
    w.fx("asking.json", "[]");
    w.t_plan(
        7,
        json!({"turns": 1, "options": ["Adopt all", "Change some"], "pending": true}),
    );
    fs::copy(w.fx.join("plan-result.json"), beside(&w, ".out")).unwrap();
    w.touch("post-fails");
    w.poll();
    assert_eq!(w.rec_open(), "open");
    assert_eq!(w.rec()["pending"], json!(true));
}

#[test]
fn the_end_of_a_session_is_said_once() {
    let w = World::new("turns");
    w.planning();
    w.comments(7, &plan_body(" ", " ", "x", 1, ""), "alice", json!([]));
    w.fx("edit-fails-on", "--remove-label needs-human-attention");
    w.poll();
    lacks(&w.gh(), "issue comment");
    assert_eq!(w.rec_open(), "open");
    w.clear_calls();
    w.rm_fx("edit-fails-on");
    w.poll();
    assert_eq!(holding(&w.gh(), "Planning session ended by alice."), 1);
    has(&w.gh(), "--remove-label needs-human-attention");
    assert_eq!(w.rec_open(), "none");
}

#[test]
fn a_session_whose_record_is_gone_is_not_announced_while_its_labels_cannot_be_removed() {
    let w = World::new("turns");
    w.planning();
    w.store_ok(&["plan-end", "7", "--close"]);
    w.comments(7, &plan_body("x", " ", " ", 1, ""), "alice", json!([]));
    w.touch("edit-fails");
    w.poll();
    lacks(&w.gh(), "issue comment");
}

#[test]
fn the_bot_is_refused_as_an_allowed_labeller_even_with_no_question_open() {
    let w = World::new("turns");
    bot_allowed(&w);
    let r = w.poll();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "the bot account tehas-bot[bot] is in TEHAS_ALLOWED_LABELERS",
    );
    assert_eq!(w.calls("fake-start"), "");
    lacks(&w.gh(), "issue edit");
}

#[test]
fn a_bot_login_that_is_not_kept_ends_the_poll_before_anything_is_written() {
    let w = World::new("turns");
    w.fx("marked.json", r#"[{"number":9}]"#);
    fs::remove_file(w.home.join("state/github/login")).unwrap();
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "ERROR: the bot's own login could not be read");
    lacks(&w.gh(), "issue edit");
    lacks(&w.gh(), "issue comment");
    assert_eq!(w.calls("fake-start"), "");
}

#[test]
fn the_bot_s_login_is_the_one_the_app_kept_and_get_user_is_not_asked() {
    let w = World::new("turns");
    let r = w.poll();
    lacks(&r.out, "the bot's own login could not be read");
    lacks(&w.gh(), "api user");
}
