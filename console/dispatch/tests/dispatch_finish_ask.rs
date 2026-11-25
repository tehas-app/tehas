//! The finish step, an ask on the issue with a marked comment and
//! tehas:finish, run by a real poll with tehas-cycle stubbed. The rules of
//! each ask are the Rust tests of src/finish.

mod common;

use common::*;
use serde_json::json;
use tehas_ports::{Act, FinishResult, Mark, Marker, Write};

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
    w.past(7, "alice", "approved", "2026-10-01T00:00:00Z", &[]);
    w
}

/// The finish replies the poll sent on #7, by the result of the marker
/// typed beside each text, with the state of its row.
fn replies(w: &World) -> Vec<(FinishResult, String)> {
    w.sent(7)
        .into_iter()
        .filter_map(|(write, state)| match write {
            Write::TicketNote { markers, .. } => match markers.as_slice() {
                [Marker::Finish(m)] if !m.by_poll => Some((m.result, state)),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// Whether a text the site was given for #7 holds `text`.
fn said(w: &World, text: &str) -> bool {
    w.said(7).iter().any(|b| b.contains(text))
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
    assert!(w.off(7, Mark::Act(Act::Finish)), "{:?}", w.writes_of(7));
    assert_eq!(replies(&w), [(FinishResult::Opened, "done".to_string())]);
    assert!(
        said(&w, "https://github.com/acme/widget/pull/90"),
        "{:?}",
        w.said(7)
    );
    has(&r.out, "finish #7: opened https://github.com/acme/widget/pull/90, asked by alice (label): The flaky test in other.rs failed, not the change.");
    assert_eq!(w.calls("fake-start"), "");
}

/// The wiring of a finish's reply: one POST of the issue's comments through
/// the adapter, and one note of the ports, done. Methods and counts only:
/// what it says is the finish's tests'.
#[test]
fn a_finish_reply_is_one_note_through_the_adapter() {
    let w = asked("alice");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    let posts = w
        .gh()
        .lines()
        .filter(|l| l.starts_with("api -X POST") && l.contains("/issues/7/comments"))
        .count();
    assert_eq!(posts, 1, "{}", w.gh());
    assert_eq!(w.notes(7), 1);
    assert_eq!(replies(&w).len(), 1);
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
    assert!(w.off(7, Mark::Act(Act::Finish)), "{:?}", w.writes_of(7));
    assert_eq!(replies(&w), [(FinishResult::Refused, "done".to_string())]);
    assert!(
        said(&w, "finished nothing: no workspace is kept for it."),
        "{:?}",
        w.said(7)
    );
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
    assert!(w.off(7, Mark::Act(Act::Finish)), "{:?}", w.writes_of(7));
    assert_eq!(replies(&w), [(FinishResult::Refused, "done".to_string())]);
    assert!(
        said(
            &w,
            "the millwright may not ask it: its text is not the text alice approved"
        ),
        "{:?}",
        w.said(7)
    );
}

#[test]
fn an_ask_from_an_account_that_is_not_allowed_is_ignored() {
    let w = asked("mallory");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.calls("fake-finish"), "");
    assert!(!w.off(7, Mark::Act(Act::Finish)), "{:?}", w.writes_of(7));
    assert!(replies(&w).is_empty(), "{:?}", w.writes_of(7));
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
    assert!(!w.off(7, Mark::Act(Act::Finish)), "{:?}", w.writes_of(7));
    assert!(replies(&w).is_empty(), "{:?}", w.writes_of(7));
}

#[test]
fn a_dry_run_runs_no_finish() {
    let w = asked("alice");
    w.dispatch(&["--dry-run"]);
    assert_eq!(w.calls("fake-finish"), "");
    assert!(!w.off(7, Mark::Act(Act::Finish)), "{:?}", w.writes_of(7));
    assert!(replies(&w).is_empty(), "{:?}", w.writes_of(7));
}
