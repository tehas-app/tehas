//! The finish step, an ask on the issue with a marked comment and
//! tehas:finish, run by a real poll with tehas-cycle stubbed. The rules of
//! each ask are the Rust tests of src/finish.

mod common;

use common::*;
use serde_json::json;

/// Issue 7 carries tehas:finish from `who`, and `who` wrote the marked
/// comment a minute before. Its body is "Words.", and alice approved a cycle
/// of it. The finish prints the URL, or the line of finish-refuses.
fn asked(who: &str) -> World {
    let w = World::new("finish-ask");
    w.stub(
        "fake-finish",
        r#"if [ -s "$T/fx/finish-refuses" ]; then
  echo "tehas-cycle: finish: #$2: $(cat "$T/fx/finish-refuses")" >&2; exit 1; fi
echo "https://github.com/acme/widget/pull/90""#,
    );
    w.env("TEHAS_FINISH_CMD", "fake-finish");
    w.slots(1, &[]);
    w.fx("ready.json", "[]");
    w.fx(
        "work-7.json",
        r#"{"number":7,"state":"open","labels":[{"name":"tehas:finish"}]}"#,
    );
    words_of_seven(&w, "Words.");
    w.fx_json(
        "events-7.json",
        &json!([{"event": "labeled", "label": {"name": "tehas:finish"}, "actor": {"login": who},
            "created_at": "2026-10-02T00:00:00Z"}]),
    );
    w.fx_json(
        "icomments-7.json",
        &json!([icomment(71, who, "2026-10-01T23:59:00Z", "2026-10-01T23:59:00Z",
            "<!-- tehas:millwright issue=7 action=finish -->\nThe flaky test in other.rs failed, not the change.")]),
    );
    w.past(7, "alice", "label", "2026-10-01T00:00:00Z", &[]);
    w
}

fn words_of_seven(w: &World, body: &str) {
    w.fx_json(
        "edit-7.json",
        &json!({"data": {"repository": {"issue": {"lastEditedAt": null, "editor": null,
            "body": body, "title": "Seven"}}}}),
    );
}

#[test]
fn an_allowed_ask_runs_the_finish_takes_the_label_off_and_comments_with_the_url() {
    let w = asked("alice");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &w.calls("fake-finish"),
        &format!(
            "finish 7 --home {} --repo acme/widget --default main --reason The flaky test in other.rs failed, not the change.",
            w.home.display()
        ),
    );
    has(
        &w.gh(),
        "issue edit 7 --repo acme/widget --remove-label tehas:finish",
    );
    has(&w.gh(), "<!-- tehas:finish issue=7 result=opened -->");
    has(&w.gh(), "https://github.com/acme/widget/pull/90");
    has(&r.out, "finish #7: opened https://github.com/acme/widget/pull/90, asked by alice (label): The flaky test in other.rs failed, not the change.");
    assert_eq!(w.calls("fake-start"), "");
}

#[test]
fn the_millwright_asks_within_its_grant() {
    let w = asked("tehas-bot");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&w.calls("fake-finish"), "finish 7 ");
    has(&r.out, "asked by tehas-bot (millwright)");
}

#[test]
fn a_refusal_of_tehas_cycle_finish_takes_the_label_off_and_comments_with_its_line() {
    let w = asked("alice");
    w.fx("finish-refuses", "no workspace is kept for it\n");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&w.gh(), "--remove-label tehas:finish");
    has(&w.gh(), "<!-- tehas:finish issue=7 result=refused -->");
    has(&w.gh(), "finished nothing: no workspace is kept for it.");
    has(
        &r.out,
        "finish #7: refused, asked by alice (label): no workspace is kept for it",
    );
}

#[test]
fn a_rule_not_met_takes_the_label_off_and_runs_nothing() {
    let w = asked("tehas-bot");
    words_of_seven(&w, "Other words.");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.calls("fake-finish"), "");
    has(&w.gh(), "--remove-label tehas:finish");
    has(
        &w.gh(),
        "the millwright may not ask it: its text is not the text alice approved",
    );
}

#[test]
fn an_ask_from_an_account_that_is_not_allowed_is_ignored() {
    let w = asked("mallory");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.calls("fake-finish"), "");
    lacks(&w.gh(), "tehas:finish");
    has(
        &r.out,
        "ignore #7: tehas:finish was applied by 'mallory', not an allowed account",
    );
}

#[test]
fn a_paused_factory_runs_no_finish() {
    let w = asked("alice");
    w.fx("control.json", r#"{"labels":[{"name":"tehas:pause"}]}"#);
    w.poll();
    assert_eq!(w.calls("fake-finish"), "");
    lacks(&w.gh(), "tehas:finish");
}

#[test]
fn a_dry_run_runs_no_finish() {
    let w = asked("alice");
    w.dispatch(&["--dry-run"]);
    assert_eq!(w.calls("fake-finish"), "");
    lacks(&w.gh(), "--remove-label tehas:finish");
}
