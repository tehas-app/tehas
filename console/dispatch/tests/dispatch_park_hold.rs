//! A relabelled start held on the body its last cycle parked on, and the retry
//! after a provider outage: what the poll tells, records and starts. The
//! decisions are fixtures in tests/fixtures/park.json.

mod common;

use common::{has, lacks, park_body, Run, World, READY_MARK};
use serde_json::{json, Value};
use std::fs;
use std::process::{Command, Stdio};

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

/// #7's current park through `filter`, as a string.
fn park_of(w: &World, filter: &str) -> String {
    jqr(&format!("{filter} | tostring"), &w.park_of().to_string())
}

// --- A relabelled start of an issue whose last cycle parked ------------------

/// The text the poll reads for #7, in the answer that carries the edit
/// record.
fn issue_body(w: &World, body: &str) {
    w.fx_json(
        "edit-7.json",
        &json!({"data": {"repository": {"issue": {"lastEditedAt": null, "editor": null, "body": body}}}}),
    );
}

/// The comments of the parked issue: the bot's park question and `notes`, a
/// REST list, in both answers a poll may read.
fn park_comments(w: &World, notes: Value) {
    w.comments(7, &park_body(&[" ", " "]), "", notes.clone());
    let bot = json!({"id": 4001, "user": {"login": "tehas-bot", "id": common::uid("tehas-bot")},
        "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z",
        "body": park_body(&[" ", " "])});
    let mut all = vec![bot];
    all.extend(notes.as_array().cloned().unwrap_or_default());
    w.fx_json("icomments-7.json", &Value::Array(all));
}

/// The park of the cycle that just stopped: alice's label (or a start by hand
/// for "-") on "Words.", parked on `reason`. #7 carries tehas:ready again,
/// with `body`.
fn parked_again(w: &World, body: &str, reason: &str, by: &str) {
    w.parked(reason, by);
    // The repository's marks, as a running host has them: the marks read
    // answers with the ready mark's node id, so its applications keep
    // counting once the pins of state/labels are kept.
    w.fx_json(
        "labels.json",
        &json!([{"id": "LA_ready", "name": READY_MARK}]),
    );
    // The hold of no resources is asked before a candidate is read.
    w.mesh(Some(r#"{"data":[{"id":"test-model"}]}"#));
    w.mesh_curl();
    park_comments(w, json!([]));
    issue_body(w, body);
    w.fx_json(
        "work-7.json",
        &json!({"number": 7, "state": "open", "body": body,
            "labels": [{"name": "needs-human-attention"}, {"name": "tehas:question"}, {"name": READY_MARK}]}),
    );
    w.fx_json(
        "ready.json",
        &json!([{"number": 7, "labels": [{"name": READY_MARK},
            {"name": "needs-human-attention"}, {"name": "tehas:question"}]}]),
    );
}

fn parked_words(w: &World) {
    parked_again(w, "Words.", "intent-park", "alice");
}

/// One poll on an issue whose park holds: labels, park and boxes stay.
#[track_caller]
fn holds(w: &World) -> Run {
    w.clear_calls();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(started(w), "");
    lacks(&w.gh(), "issue edit 7");
    assert_eq!(w.marks(Some(7)), 0);
    assert_eq!(park_of(w, "[.attempt, .retries, .question]"), "[1,0,4001]");
    r
}

/// One poll on a lifted park: the cycle starts, and nothing of the hold is
/// said or told.
#[track_caller]
fn lifts(w: &World) -> Run {
    w.clear_calls();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(started(w), "7");
    lacks(&r.out, "hold #7: its last cycle parked on");
    lacks(&w.gh(), "Not started: the last cycle parked on");
    assert_eq!(park_of(w, "."), "null");
    r
}

const HOLD: &str =
    "hold #7: its last cycle parked on intent-park and the body has not changed since";

#[test]
fn a_relabelled_start_on_the_body_its_last_cycle_parked_on_is_held_said_once_on_the_issue() {
    let w = World::new("hold-said");
    parked_words(&w);
    let r = holds(&w);
    has(&r.out, HOLD);
    assert!(w.said_on(7, "Not started: the last cycle parked on `intent-park` and the issue text has not changed since. Edit the body, or reply to the park comment with the word of one of its retries."), "{:?}", w.said(7));
    assert_eq!(park_of(&w, ".told_at != null"), "true");
    assert_eq!(w.notes(7), 1);
    assert_eq!(
        w.dispatch_state()["held"],
        json!([{"clear": "edit the body, or reply to the park comment with the word of one of its retries", "issue": 7,
            "text": "its last cycle parked on `intent-park` and the body has not changed since",
            "why": "parked"}])
    );
}

#[test]
fn and_the_next_poll_of_the_same_park_only_logs_with_no_comment() {
    let w = World::new("hold-next");
    parked_words(&w);
    holds(&w);
    let r = holds(&w);
    has(&r.out, HOLD);
    assert_eq!(w.notes(7), 0);
}

/// The park's question taken off #7, as leaving it parked does.
fn no_question(w: &World) {
    w.fx(
        "ready.json",
        r#"[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"needs-human-attention"}]}]"#,
    );
    w.fx("asking.json", "[]");
    let p = w.fx.join("work-7.json");
    let mut v: Value = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
    v["labels"]
        .as_array_mut()
        .unwrap()
        .retain(|l| l["name"] != "tehas:question");
    w.fx_json("work-7.json", &v);
}

#[test]
fn a_dry_run_of_the_hold_says_it_and_changes_nothing_not_its_park() {
    let w = World::new("hold-dry");
    parked_words(&w);
    w.clear_calls();
    let r = w.dispatch(&["--dry-run"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, HOLD);
    assert_eq!(started(&w), "");
    lacks(&w.gh(), "issue edit 7");
    assert_eq!(w.marks(Some(7)), 0);
    assert_eq!(w.notes(7), 0);
    lacks(&w.gh(), "api -X");
    assert_eq!(park_of(&w, ".told_at"), "null");
}

#[test]
fn a_hold_whose_tell_the_store_would_not_take_tells_it_again_on_the_next_poll_and_says_so() {
    let w = World::new("hold-tell-refused");
    parked_words(&w);
    w.store_refuses("park-told 7", 1, "store: busy #7: the database is locked");
    let r = holds(&w);
    has(
        &r.out,
        "WARN: could not record the hold on the park of #7 (busy)",
    );
    holds(&w);
    assert_eq!(w.notes(7), 1);
    assert_eq!(park_of(&w, ".told_at"), "null");
}

// The hold is of the poll, where its kind is settled, not of the order.
#[test]
fn an_issue_first_in_the_start_order_parked_on_its_own_body_is_held_too() {
    let w = World::new("hold-order");
    parked_words(&w);
    w.store_ok(&["order-set", "--issues", "7"]);
    holds(&w);
}

#[test]
fn a_changed_body_starts_and_the_park_of_the_held_go_is_not_current_after_the_start() {
    let w = World::new("hold-changed");
    parked_again(&w, "Words, and more.", "intent-park", "alice");
    w.clear_calls();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(started(&w), "7");
    lacks(&r.out, "its last cycle parked on");
    assert_eq!(park_of(&w, "."), "null");
}

#[test]
fn a_ticked_retry_on_the_body_its_last_cycle_parked_on_starts_all_the_same() {
    let w = World::new("hold-ticked");
    parked_words(&w);
    w.comments(7, &park_body(&["x", " "]), "alice", json!([]));
    w.clear_calls();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(started(&w), "7");
    lacks(&r.out, "its last cycle parked on");
    lacks(&w.gh(), "Not started: the last cycle parked on");
    assert_eq!(park_of(&w, "."), "null");
}

/// #7's last cycle, started by `by` (or by hand for "-"), parked by a
/// provider outage on "Words.". No label of a person's queues it.
fn outage_parked(w: &World, by: &str) {
    parked_again(w, "Words.", "provider-outage", by);
    w.fx("ready.json", "[]");
    w.fx(
        "issue-7.json",
        r#"{"state":"OPEN","labels":[{"name":"needs-human-attention"},{"name":"tehas:question"}]}"#,
    );
    w.clear_calls();
}

const OWED: &str =
    "skip #7: its retry after a provider outage is not started; it waits for a person";

#[test]
fn a_park_of_a_provider_outage_is_retried_as_today_on_its_own_body() {
    // The start rests on the account that started the parked cycle, and the
    // issue carries no tehas:ready.
    let w = World::new("hold-outage");
    outage_parked(&w, "alice");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(started(&w), "7");
    has(&r.out, "start #7 (requeued by alice)");
    lacks(&r.out, "its last cycle parked on");
    lacks(&w.gh(), "Not started: the last cycle parked on");
    assert_eq!(park_of(&w, "."), "null");
    assert_eq!(
        w.at_mark("[.by, .how] | tostring"),
        r#"["alice","outage-retry"]"#
    );
    assert_eq!(
        jqr(
            "[.cycle.started_by, .cycle.requeued] | tostring",
            &w.row(&["cycle-show", "7"]).to_string()
        ),
        r#"["alice",true]"#
    );
}

#[test]
fn the_retry_rests_on_who_started_the_parked_cycle_not_on_a_later_approval_of_another_account() {
    let w = World::new("hold-outage-bob");
    outage_parked(&w, "alice");
    // bob's approval, later and never used: the retry is alice's.
    w.t_approve(7, "bob", "approved", 1_767_300_000, &[]);
    let r = w.poll();
    assert_eq!(started(&w), "7");
    has(&r.out, "start #7 (requeued by alice)");
    assert_eq!(w.at_mark(".by"), "alice");
}

#[test]
fn a_retry_whose_account_may_no_longer_label_spends_it_and_starts_nothing() {
    let w = World::new("hold-outage-carol");
    outage_parked(&w, "carol");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(started(&w), "");
    has(&r.out, OWED);
    assert_eq!(park_of(&w, "[.retries, .automatic]"), "[1,false]");
    w.clear_calls();
    let r = w.poll();
    lacks(&r.out, "its retry after a provider outage");
}

#[test]
fn a_dry_run_of_that_leaves_the_retry_owed() {
    let w = World::new("hold-outage-dry");
    outage_parked(&w, "carol");
    let r = w.dispatch(&["--dry-run"]);
    has(&r.out, OWED);
    assert_eq!(park_of(&w, "[.retries, .automatic]"), "[0,true]");
}

#[test]
fn an_open_planning_session_holds_the_retry_after_an_outage_and_spends_it_a13() {
    let w = World::new("hold-outage-plan");
    outage_parked(&w, "alice");
    w.t_plan(7, json!({"turns": 1, "comment": 4002}));
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(started(&w), "");
    has(&r.out, OWED);
    assert_eq!(park_of(&w, "[.retries, .automatic]"), "[1,false]");
}

#[test]
fn one_retry_after_an_outage_then_a_person_the_retrys_own_outage_park_owes_none() {
    let w = World::new("hold-outage-once");
    outage_parked(&w, "alice");
    w.poll();
    assert_eq!(started(&w), "7");
    // The retry's cycle (attempt 2, requeued) is parked by an outage too.
    let out = Command::new("flock")
        .args(["-w", "5"])
        .arg(w.cycles_dir().join("7.lock"))
        .arg(w.t().join("store-bin"))
        .args([
            "cycle-end",
            "7",
            "--attempt",
            "2",
            "--status",
            "handoff",
            "--provider",
            "--parked",
        ])
        .env("TEHAS_HOME", &w.home)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success(), "cycle-end: {out:?}");
    w.t_park(7, "provider-outage", "4009", &[]);
    assert_eq!(park_of(&w, "[.attempt, .automatic]"), "[2,false]");
    w.clear_calls();
    let r = w.poll();
    assert_eq!(started(&w), "");
    lacks(&r.out, "its retry after a provider outage");
}

#[test]
fn an_outage_park_of_a_cycle_started_by_hand_owes_no_retry() {
    let w = World::new("hold-outage-by-hand");
    outage_parked(&w, "-");
    assert_eq!(park_of(&w, ".automatic"), "false");
    let r = w.poll();
    assert_eq!(started(&w), "");
    lacks(&r.out, "provider outage");
}

#[test]
fn a_retry_whose_unit_never_comes_up_leaves_no_retry_behind() {
    let w = World::new("hold-outage-start-fails");
    outage_parked(&w, "alice");
    w.touch("start-fails");
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    assert_eq!(park_of(&w, "[.retries, .automatic]"), "[1,false]");
    w.rm_fx("start-fails");
    w.clear_calls();
    w.poll();
    assert_eq!(started(&w), "");
}

#[test]
fn a_retry_after_an_outage_whose_issue_cannot_be_marked_is_still_owed_and_the_next_poll_starts_it()
{
    let w = World::new("hold-outage-mark");
    outage_parked(&w, "alice");
    w.fx("edit-fails-on", "--add-label tehas:running");
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "ERROR: could not mark #7 as running, not starting a cycle",
    );
    assert_eq!(started(&w), "");
    assert_eq!(park_of(&w, "[.retries, .automatic]"), "[0,true]");
    w.rm_fx("edit-fails-on");
    w.clear_calls();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(started(&w), "7");
    assert_eq!(park_of(&w, "."), "null");
}

#[test]
fn a_busy_store_keeps_the_retry_after_an_outage_for_the_next_poll_nothing_starts_the_park_stays() {
    let w = World::new("hold-outage-busy");
    outage_parked(&w, "alice");
    w.store_refuses(
        "park-show --automatic",
        1,
        "store: busy: the database is locked",
    );
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    assert_eq!(started(&w), "");
    has(
        &r.out,
        "ERROR: the state store could not list the parks owed a retry (busy)",
    );
    assert_eq!(park_of(&w, "[.retries, .automatic]"), "[0,true]");
    lacks(&w.gh(), "issue edit 7");
    assert_eq!(w.marks(Some(7)), 0);
}

#[test]
fn a_park_of_a_cycle_started_by_hand_names_no_body_and_starts_as_today() {
    // Nothing bound a body for a run by hand: no reason to hold.
    let w = World::new("hold-by-hand");
    parked_again(&w, "Words.", "intent-park", "-");
    assert_eq!(park_of(&w, ".body_sha256"), "null");
    w.clear_calls();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(started(&w), "7");
    lacks(&r.out, "its last cycle parked on");
    lacks(&w.gh(), "Not started: the last cycle parked on");
}

#[test]
fn a_retry_ticked_in_the_park_comment_lifts_the_hold_once_the_question_label_is_off_1829() {
    // A person ticks Retry, then swaps the labels for tehas:ready by hand
    // before a slot frees: the hold must see it.
    let w = World::new("hold-ticked-lift");
    parked_words(&w);
    no_question(&w);
    w.comments(7, &park_body(&[" ", "x", " "]), "alice", json!([]));
    let r = lifts(&w);
    has(&r.out, "lift #7: its last cycle parked on intent-park and the body has not changed since, lifted on the retry alice answered");
}

#[test]
fn a_retry_answered_through_tehas_answer_lifts_the_hold_as_a_tick_does() {
    let w = World::new("hold-answered-lift");
    parked_words(&w);
    no_question(&w);
    w.comments(7, &park_body(&[" ", " ", " "]), "", json!([]));
    w.store_ok(&[
        "answer-record",
        "7",
        "--by",
        "alice",
        "--choice",
        "retry-text",
    ]);
    let r = lifts(&w);
    has(&r.out, "lifted on the retry alice answered");
}

#[test]
fn a_retry_ticked_by_an_account_that_may_not_authorise_work_lifts_nothing() {
    let w = World::new("hold-ticked-stranger");
    parked_words(&w);
    no_question(&w);
    w.comments(7, &park_body(&[" ", "x", " "]), "mallory", json!([]));
    let r = holds(&w);
    has(&r.out, HOLD);
    lacks(&r.out, "lift #7");
}

#[test]
fn a_park_whose_question_is_no_longer_on_record_does_not_hold_a_new_tehas_ready() {
    // No box of that park starts anything now.
    let w = World::new("hold-no-record");
    parked_words(&w);
    w.store_ok(&["question-answered", "7", "--id", "4001", "--withdraw"]);
    let r = lifts(&w);
    has(&r.out, "lift #7: its last cycle parked on intent-park and the body has not changed since, lifted as its park has no question on record to answer");
}

/// A develop timeout on the same text twice offered no retry and asked
/// nothing: its park has no question on record, and a new `tehas:ready` on
/// the unchanged text is held all the same, said once, until the body changes.
#[test]
fn a_park_that_offered_no_retry_holds_a_new_tehas_ready_until_the_body_changes() {
    let w = World::new("hold-no-retry");
    parked_again(&w, "Words.", "child-timeout:develop", "alice");
    w.store_ok(&["question-answered", "7", "--id", "4001", "--withdraw"]);
    w.clear_calls();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(started(&w), "");
    has(&r.out, "hold #7: its last cycle parked on child-timeout:develop twice on the same text, and the body has not changed since");
    let edit = tehas_core::hold::HELD_PARKED_EDIT;
    assert!(w.said_on(7, &format!("Not started: the last cycle ran out of time in develop twice on this text, so it offers no retry: {edit}.")), "{:?}", w.said(7));
    assert_eq!(
        w.dispatch_state()["held"],
        json!([{"clear": edit, "issue": 7,
            "text": "its last cycle parked on `child-timeout:develop` and the body has not changed since",
            "why": "parked"}])
    );
    w.clear_calls();
    w.poll();
    assert_eq!(w.notes(7), 0);
    assert_eq!(started(&w), "");
    parked_body_changed(&w);
}

/// The same park once the body changed: the label starts the cycle.
fn parked_body_changed(w: &World) {
    issue_body(w, "Smaller words.");
    let work = fs::read_to_string(w.fx.join("work-7.json")).unwrap();
    w.fx(
        "work-7.json",
        &work.replace("\"Words.\"", "\"Smaller words.\""),
    );
    w.clear_calls();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(started(w), "7");
}
