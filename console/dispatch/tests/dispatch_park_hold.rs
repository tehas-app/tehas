//! A relabelled start held on the body its last cycle parked on, and the retry
//! after a provider outage: what the poll tells, records and starts. The
//! decisions are fixtures in tests/fixtures/park.json.

mod common;

use common::{count_lines, has, icomment, lacks, park_body, Run, World, MINIMAL};
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

fn iso(epoch: i64) -> String {
    let out = Command::new("date")
        .args(["-u", "-d", &format!("@{epoch}"), "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
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
    // The hold of no resources is asked before a candidate is read.
    w.mesh(Some(r#"{"data":[{"id":"test-model"}]}"#));
    w.mesh_curl();
    park_comments(w, json!([]));
    issue_body(w, body);
    w.fx_json(
        "work-7.json",
        &json!({"number": 7, "state": "open", "body": body,
            "labels": [{"name": "needs-human-attention"}, {"name": "tehas:question"}, {"name": "tehas:ready"}]}),
    );
    w.fx(
        "ready.json",
        r#"[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"needs-human-attention"},{"name":"tehas:question"}]}]"#,
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
    has(&w.gh(), "issue comment 7 --repo acme/widget --body Not started: the last cycle parked on `intent-park` and the issue text has not changed since. Edit the body, or tick Retry in the park comment.");
    assert_eq!(park_of(&w, ".told_at != null"), "true");
    assert_eq!(count_lines(&w.gh(), "issue comment 7"), 1);
    assert_eq!(
        w.dispatch_state()["held"],
        json!([{"clear": "edit the body, or tick Retry in the park comment", "issue": 7,
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
    lacks(&w.gh(), "issue comment 7");
}

#[test]
fn the_millwrights_tehas_ready_on_the_body_its_last_cycle_parked_on_is_held_too() {
    // A second develop timeout on unchanged text offers no retry box, and
    // leaves the millwright's label as the one way round it.
    let w = World::new("hold-millwright");
    parked_words(&w);
    w.instance(MINIMAL);
    w.fx(
        "events-7.json",
        r#"[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"tehas-bot"},"created_at":"2026-10-02T00:00:00Z"}]"#,
    );
    let r = holds(&w);
    has(&r.out, HOLD);
}

// --- The millwright's guidance comment lifts the hold -------------------------

/// The park of #7 on "Words." with the millwright's tehas:ready of two minutes
/// ago on it and alice's approval behind it, with these settings
/// [TEHAS_WORK_FILES=1]. The label's epoch.
fn millwright_parked(w: &World, settings: &[&str]) -> i64 {
    parked_words(w);
    let at = tehas_core::time::now() - 120;
    let mut lines = MINIMAL.to_vec();
    lines.extend_from_slice(if settings.is_empty() {
        &["TEHAS_WORK_FILES=1"]
    } else {
        settings
    });
    w.instance(&lines);
    ready_by(w, "tehas-bot", at);
    at
}

/// tehas:ready on #7 by `login` at `at`.
fn ready_by(w: &World, login: &str, at: i64) {
    w.fx_json(
        "events-7.json",
        &json!([{"event": "labeled", "label": {"name": "tehas:ready"},
            "actor": {"login": login}, "created_at": iso(at)}]),
    );
}

/// A change note of the millwright.
fn change_note(id: u64, login: &str, at: &str, text: &str) -> Value {
    icomment(
        id,
        login,
        at,
        at,
        &format!("<!-- tehas:millwright issue=7 action=change -->\n{text}"),
    )
}

fn with_notes(w: &World, notes: &[Value]) {
    park_comments(w, Value::Array(notes.to_vec()));
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
fn the_millwrights_action_change_comment_of_the_ten_minutes_before_its_label_lifts_the_hold_and_the_record_says_so(
) {
    let w = World::new("hold-lift");
    let at = millwright_parked(&w, &[]);
    let note_at = iso(at - 180);
    with_notes(
        &w,
        &[change_note(
            81,
            "tehas-bot",
            &note_at,
            "Do not run the whole suite.",
        )],
    );
    let r = lifts(&w);
    has(&r.out, "lift #7: its last cycle parked on intent-park and the body has not changed since, lifted on the fixer's guidance comment");
    has(&r.out, "start #7 (labelled by the fixer by tehas-bot)");
    assert_eq!(
        w.approval("[.by, .how, .fixer_starts, .rests_on.by, .rests_on.how]", 7),
        r#"["tehas-bot","fixer",1,"alice","label"]"#
    );
    // The lift is its own field: rests_on keeps naming alice's approval, and
    // the note is the comment the cycle is given.
    let lifted: Value = serde_json::from_str(&w.approval(".park_lifted", 7)).unwrap();
    assert_eq!(
        lifted,
        json!({"source": "millwright:81", "by": "tehas-bot", "at": note_at,
            "text": "<!-- tehas:millwright issue=7 action=change -->\nDo not run the whole suite."})
    );
    assert_eq!(
        w.approval(".rests_on.body_sha256 == .body_sha256", 7),
        "true"
    );
    assert_eq!(
        w.approval(".retry_context", 7),
        w.approval("[.park_lifted]", 7)
    );
}

#[test]
fn the_lift_needs_the_instance_to_give_a_cycle_the_note_images_lift_it_neither_setting_holds() {
    let w = World::new("hold-lift-settings");
    let at = millwright_parked(&w, &["TEHAS_WORK_IMAGES=1"]);
    with_notes(
        &w,
        &[change_note(
            81,
            "tehas-bot",
            &iso(at - 180),
            "Look at the shot.",
        )],
    );
    lifts(&w);
    let at = millwright_parked(&w, &["TEHAS_WORK_IMAGES=0", "TEHAS_WORK_FILES=0"]);
    with_notes(
        &w,
        &[change_note(
            81,
            "tehas-bot",
            &iso(at - 180),
            "Reaches no cycle.",
        )],
    );
    let r = holds(&w);
    has(&r.out, HOLD);
    lacks(&r.out, "lift #7");
    assert_eq!(park_of(&w, ".told_at != null"), "true");
}

#[test]
fn an_action_retry_comment_of_the_same_window_lifts_nothing_that_ask_is_the_run_again() {
    let w = World::new("hold-retry-note");
    let at = millwright_parked(&w, &[]);
    let t = iso(at - 180);
    with_notes(
        &w,
        &[icomment(
            81,
            "tehas-bot",
            &t,
            &t,
            "<!-- tehas:millwright issue=7 action=retry -->\nOnce more, the same.",
        )],
    );
    let r = holds(&w);
    has(&r.out, HOLD);
    lacks(&r.out, "lift #7");
}

#[test]
fn two_change_notes_of_the_window_the_newest_of_them_is_the_one_the_record_names() {
    let w = World::new("hold-two-notes");
    let at = millwright_parked(&w, &[]);
    with_notes(
        &w,
        &[
            change_note(
                81,
                "tehas-bot",
                &iso(at - 300),
                "First the tests, then the build.",
            ),
            change_note(82, "tehas-bot", &iso(at - 60), "Run only the named tests."),
        ],
    );
    lifts(&w);
    assert_eq!(w.approval(".park_lifted.source", 7), "millwright:82");
}

#[test]
fn a_change_note_outside_the_ten_minutes_or_none_at_all_lifts_nothing() {
    let w = World::new("hold-old-note");
    let at = millwright_parked(&w, &[]);
    with_notes(
        &w,
        &[change_note(
            81,
            "tehas-bot",
            &iso(at - 900),
            "Out of its time.",
        )],
    );
    let r = holds(&w);
    has(&r.out, HOLD);
    lacks(&r.out, "lift #7");
}

#[test]
fn a_change_note_of_another_account_or_one_saved_after_the_label_lifts_nothing() {
    let w = World::new("hold-other-note");
    let at = millwright_parked(&w, &[]);
    with_notes(
        &w,
        &[
            change_note(81, "mallory", &iso(at - 180), "Not the millwright."),
            icomment(
                82,
                "tehas-bot",
                &iso(at - 120),
                &iso(at + 3600),
                "<!-- tehas:millwright issue=7 action=change -->\nSaved after the label.",
            ),
        ],
    );
    let r = holds(&w);
    has(&r.out, HOLD);
}

#[test]
fn a_newer_action_retry_comment_takes_the_lift_away_the_note_on_record_would_ask_for_the_same_run()
{
    let w = World::new("hold-newer-retry");
    let at = millwright_parked(&w, &[]);
    let t = iso(at - 60);
    with_notes(
        &w,
        &[
            change_note(81, "tehas-bot", &iso(at - 180), "Guidance."),
            icomment(
                82,
                "tehas-bot",
                &t,
                &t,
                "<!-- tehas:millwright issue=7 action=retry -->\nOnce more.",
            ),
        ],
    );
    let r = holds(&w);
    lacks(&r.out, "lift #7");
}

#[test]
fn a_change_note_whose_marker_names_another_issue_lifts_nothing() {
    let w = World::new("hold-note-other-issue");
    let at = millwright_parked(&w, &[]);
    let t = iso(at - 180);
    with_notes(
        &w,
        &[icomment(
            81,
            "tehas-bot",
            &t,
            &t,
            "<!-- tehas:millwright issue=77 action=change -->\nAnother issue.",
        )],
    );
    let r = holds(&w);
    lacks(&r.out, "lift #7");
}

#[test]
fn a_parked_issue_that_asks_no_question_has_its_comments_read_and_the_note_lifts_the_hold() {
    let w = World::new("hold-no-question");
    let at = millwright_parked(&w, &[]);
    with_notes(
        &w,
        &[change_note(81, "tehas-bot", &iso(at - 180), "Guidance.")],
    );
    no_question(&w);
    lifts(&w);
}

#[test]
fn a_change_note_saved_again_before_the_label_lifts_only_where_the_poll_can_see_its_own_account_did(
) {
    // GraphQL names the editor.
    let w = World::new("hold-saved-again");
    let at = millwright_parked(&w, &[]);
    with_notes(
        &w,
        &[icomment(
            81,
            "tehas-bot",
            &iso(at - 180),
            &iso(at - 60),
            "<!-- tehas:millwright issue=7 action=change -->\nSecond thoughts.",
        )],
    );
    lifts(&w);
}

#[test]
fn comments_that_cannot_be_read_keep_the_hold_untold_and_the_next_poll_asks_again() {
    let w = World::new("hold-unreadable");
    let at = millwright_parked(&w, &[]);
    with_notes(
        &w,
        &[change_note(81, "tehas-bot", &iso(at - 180), "Guidance.")],
    );
    // With the question open that read ends the poll before any start is
    // weighed.
    no_question(&w);
    for f in ["comments-7", "icomments-7"] {
        fs::rename(
            w.fx.join(format!("{f}.json")),
            w.fx.join(format!("{f}.kept")),
        )
        .unwrap();
    }
    let r = holds(&w);
    has(&r.out, "the fixer's comments could not be read");
    lacks(&w.gh(), "Not started: the last cycle parked on");
    assert_eq!(park_of(&w, ".told_at != null"), "false");
    for f in ["comments-7", "icomments-7"] {
        fs::rename(
            w.fx.join(format!("{f}.kept")),
            w.fx.join(format!("{f}.json")),
        )
        .unwrap();
    }
    lifts(&w);
}

#[test]
fn a_persons_tehas_ready_on_the_parked_body_is_held_whatever_comments_exist() {
    let w = World::new("hold-person");
    let at = millwright_parked(&w, &[]);
    with_notes(
        &w,
        &[change_note(81, "tehas-bot", &iso(at - 180), "Guidance.")],
    );
    // The lift belongs to the millwright's own label.
    ready_by(&w, "alice", at);
    let r = holds(&w);
    has(&r.out, HOLD);
    lacks(&r.out, "lift #7");
}

#[test]
fn a_start_the_lift_let_through_counts_against_the_cap_of_retries_and_a_second_one_is_refused() {
    let w = World::new("hold-lift-cap");
    let at = millwright_parked(
        &w,
        &["TEHAS_WORK_FILES=1", "TEHAS_MILLWRIGHT_MAX_RETRIES=1"],
    );
    with_notes(
        &w,
        &[change_note(81, "tehas-bot", &iso(at - 180), "Guidance.")],
    );
    lifts(&w);
    assert_eq!(w.approval(".fixer_starts", 7), "1");
    // The cycle that lift started parks on the same body: the millwright's
    // next label is at the cap, and a second note lifts nothing.
    w.t_park(7, "intent-park", "4009", &[]);
    w.clear_calls();
    let r = w.poll();
    assert_eq!(started(&w), "");
    has(
        &r.out,
        "the fixer has started 1 cycles on it since alice approved it, the limit",
    );
    lacks(&r.out, "lift #7");
}

#[test]
fn a_dry_run_of_the_lift_says_it_and_changes_nothing_and_the_park_keeps_its_hold() {
    let w = World::new("hold-lift-dry");
    let at = millwright_parked(&w, &[]);
    with_notes(
        &w,
        &[change_note(81, "tehas-bot", &iso(at - 180), "Guidance.")],
    );
    w.clear_calls();
    let r = w.dispatch(&["--dry-run"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "lift #7: its last cycle parked on intent-park and the body has not changed since, lifted on the fixer's guidance comment");
    has(&r.out, "would run a cycle for #7");
    assert_eq!(started(&w), "");
    lacks(&w.gh(), "issue edit 7");
    assert_eq!(park_of(&w, "[.retries, .told_at] | tostring"), "[0,null]");
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
    lacks(&w.gh(), "issue comment 7");
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
    assert_eq!(count_lines(&w.gh(), "issue comment 7"), 1);
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
    w.t_approve(7, "bob", "label", 1_767_300_000, &[]);
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
    has(&r.out, "lift #7: its last cycle parked on intent-park and the body has not changed since, lifted on the Retry alice ticked");
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
    has(&r.out, "lift #7: its last cycle parked on intent-park and the body has not changed since, lifted as its park has no question on record to tick");
}
