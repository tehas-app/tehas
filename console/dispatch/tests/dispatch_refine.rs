//! A cycle parked as underspecified or too-large is refined once on its own
//! (rule 5a), and what a refine that files does to the original issue and the
//! filed one.

mod common;

use common::Part::{Off, On};
use common::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use tehas_core::home::{self, Home};
use tehas_ports::{Act, Mark, State};

/// Issue 7's cycle was started on `who`'s tehas:ready of `body` and parked at
/// explore as intent-park, its question 4001 on record and unanswered. The
/// text has not been edited since. Nothing else is queued.
fn refine_park(w: &World, reason: &str, extra: &str, body: &str, who: &str) {
    w.reset();
    w.fx("ready.json", "[]");
    let labels: Vec<Value> = ["needs-human-attention", "tehas:question", extra]
        .iter()
        .filter(|l| !l.is_empty())
        .map(|l| json!({"name": l}))
        .collect();
    w.fx_json("asking.json", &json!([{"number": 7, "labels": labels}]));
    w.fx_json(
        "issue-7.json",
        &json!({"title": "Faster search", "body": body, "state": "OPEN", "labels": labels}),
    );
    let sum = format!("{:x}", Sha256::digest(body.as_bytes()));
    w.t_approve(7, who, "approved", 1_700_000_001, &["--body-sha", &sum]);
    w.t_started(7, None, &[]);
    let r = w.sh(&format!(
        "flock -w 5 \"$T_CYCLES/7.lock\" \"$T/store-bin\" cycle-end 7 --attempt 1 --status parked \
         --cap intent-park --park-reason {reason} --parked"
    ));
    assert_eq!(r.rc, 0, "refine_park: the store refused the end: {}", r.out);
    w.t_park(7, "intent-park", "4001", &[]);
    w.comments(7, &park_body(&[" ", " "]), "", json!([]));
}

fn parked_underspecified(w: &World) {
    refine_park(w, "underspecified", "", "Make it good.", "alice");
}

/// The refines spent on the park.
fn spent(w: &World) -> Value {
    w.row(&["park-show", "7"])["park"]["retries"].clone()
}

/// An ended planning session of #7 on record, of `status`, its end at `end`
/// (t_plan_end).
fn plan_ended(w: &World, end: i64, status: &str) {
    let brief = w.t().join("plan-brief.md");
    fs::write(&brief, "# Faster search\n").unwrap();
    let brief = brief.to_string_lossy().into_owned();
    let begun = w.store_at(
        end - 60,
        &[
            "plan-begin",
            "7",
            "--by",
            "alice",
            "--label",
            "tehas:plan",
            "--brief-file",
            &brief,
        ],
    );
    assert_eq!(begun.rc, 0, "{}", begun.out);
    let ended = w.store_at(end, &["plan-end", "7", "--turn", "1", "--outcome", status]);
    assert_eq!(ended.rc, 0, "{}", ended.out);
}

fn holding(text: &str, want: &str) -> usize {
    text.lines().filter(|l| l.contains(want)).count()
}

fn brief_line(w: &World) -> String {
    format!(
        "7 --out {}.out --keep {} --brief {}/tmp/plan-7.brief.",
        w.record().display(),
        w.keep().display(),
        w.home.display()
    )
}

fn refine_brief() -> String {
    fs::read_to_string(root().join("lib/refine-brief.md"))
        .unwrap()
        .trim_end_matches('\n')
        .to_string()
}

/// Issue 7 carries alice's tehas:refine, and the session's first turn files
/// issue 12 (and whatever else is named), as tehas plan reports a confirm.
fn filed_by_refine(w: &World, body: &str, urls: &[&str]) {
    w.to_refine();
    w.fx_json("issue-12.json", &json!({"body": body}));
    w.fx("work-12.json", r#"{"number":12,"id":5012,"state":"open"}"#);
    w.fx_json(
        "plan-result.json",
        &json!({"session": "abc-123", "text": format!("Filed {}: Faster search, part 1.", urls[0]),
            "question": null, "filed": urls}),
    );
}

const ONE_PART: &str =
    "Search answers in 50 ms.\n\n## Deliverables\n1. ...\n\n## Later parts\nNone.";
const MORE_PARTS: &str = "Search answers in 50 ms.\n\n## Later parts\n- Cache the index\n- Warm it on start\n\n## Evidence\nx - lib/a.sh:1";
const I12: &str = "https://github.com/acme/widget/issues/12";

// The decisions of the next two are fixture cases too; these hold what the
// poll writes for them.
#[test]
fn a_cycle_parked_as_underspecified_is_refined_with_the_refine_brief_on_the_word_of_its_starter() {
    let w = World::new("refine");
    parked_underspecified(&w);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&w.plan_calls(), &brief_line(&w));
    has(&w.plan_calls(), " --kind refine");
    let brief = w.rec_brief();
    assert_eq!(
        brief.lines().take(3).collect::<Vec<_>>().join("\n"),
        "# Faster search\n\nMake it good."
    );
    has(&brief, &refine_brief());
    assert_eq!(
        brief.lines().last(),
        Some("This request is issue #7 itself. The issue you file replaces #7: #7 is not a duplicate of this plan.")
    );
    has(
        &r.out,
        "start refining #7 (its cycle parked as underspecified, started by alice)",
    );
    let p = &w.row(&["plan-show", "7"])["planning"];
    assert_eq!(
        json!([p["by"], p["label"], p["auto"]]),
        json!(["alice", "tehas:refine", true])
    );
    has(&w.gh(), "its cycle parked as `underspecified`, and the start of that cycle by @alice authorizes one refine.");
    assert!(
        w.changes(
            7,
            &[
                On(Mark::State(State::Planning)),
                Off(Mark::Act(Act::Refine)),
                Off(Mark::State(State::Question)),
                Off(Mark::State(State::NeedsPerson))
            ]
        ),
        "{:?}",
        w.writes_of(7)
    );
    assert_eq!(spent(&w), json!(1));
    // The session's start took the park's question off the record.
    assert_eq!(w.park_q(7), "none");
    let h = w.row(&["history-show", "7", "--kind", "plan"]);
    let last = h["rows"]
        .as_array()
        .and_then(|r| r.last())
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        json!([last["label"], last["auto"]]),
        json!(["tehas:refine", true])
    );
}

#[test]
fn a_cycle_parked_as_too_large_is_refined_and_the_comment_names_the_reason() {
    let w = World::new("refine");
    refine_park(&w, "too-large", "", "Make it good.", "alice");
    w.poll();
    has(&w.gh(), "its cycle parked as `too-large`");
}

#[test]
fn a_park_of_another_reason_is_not_refined() {
    let w = World::new("refine");
    refine_park(&w, "premise-unsound", "", "Make it good.", "alice");
    let r = w.poll();
    assert_eq!(w.plan_calls(), "");
    lacks(&r.out, "refin");
}

#[test]
fn an_issue_that_had_a_planning_session_before_is_not_refined() {
    let w = World::new("refine");
    parked_underspecified(&w);
    plan_ended(&w, 1_700_000_100, "not-started");
    let r = w.poll();
    assert_eq!(w.plan_calls(), "");
    lacks(&r.out, "refin");
}

#[test]
fn a_refine_whose_first_label_change_fails_opens_no_session_and_is_not_tried_again() {
    let w = World::new("refine");
    parked_underspecified(&w);
    w.fx("edit-fails-on", "--add-label tehas:planning\n");
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "could not mark #7 as planning");
    assert_eq!(w.rec_open(), "none");
    assert_eq!(spent(&w), json!(1));
    w.clear_calls();
    w.rm_fx("edit-fails-on");
    w.poll();
    assert_eq!(w.plan_calls(), "");
}

#[test]
fn a_paused_factory_refines_nothing_and_refines_once_it_is_resumed() {
    let w = World::new("refine");
    parked_underspecified(&w);
    w.pause_flag(r#"{"at":"2026-01-01T00:00:00Z","by":"alice","reason":"test"}"#);
    w.poll();
    assert_eq!(w.plan_calls(), "");
    assert_eq!(spent(&w), json!(0));
    fs::remove_file(Home::new(&w.home).state(home::State::Pause)).unwrap();
    w.clear_calls();
    let r = w.poll();
    has(&r.out, "start refining #7");
}

#[test]
fn a_refine_that_files_one_issue_links_both_ways_and_closes_the_original_as_replaced() {
    let w = World::new("refine");
    filed_by_refine(&w, ONE_PART, &[I12]);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    let gh = w.gh();
    assert!(
        w.wrote(12, None, "Replaces #7\n\nSearch answers in 50 ms."),
        "{:?}",
        w.patched(12)
    );
    assert!(w.parted(7, 12));
    assert!(
        w.said_on(
            7,
            &format!("Replaced by #12.\n\nFiled {I12}: Faster search, part 1.")
        ),
        "{:?}",
        w.said(7)
    );
    assert!(
        w.changes(
            7,
            &[
                Off(Mark::State(State::Question)),
                Off(Mark::State(State::Planning)),
                Off(Mark::State(State::Refined)),
                On(Mark::State(State::Replaced))
            ]
        ),
        "{:?}",
        w.writes_of(7)
    );
    assert!(w.closed(7), "{:?}", w.writes_of(7));
    lacks(&gh, "tehas:question kind=plan");
    assert!(
        !w.changes(7, &[On(Mark::State(State::Question))]),
        "{:?}",
        w.writes_of(7)
    );
    assert_eq!(w.rec_open(), "none");
    let mut out = w.record().into_os_string();
    out.push(".out");
    assert!(!std::path::Path::new(&out).exists());
    has(
        &r.out,
        &format!("planning #7 ended: tehas:replaced by {I12}"),
    );
}

#[test]
fn a_refine_whose_plan_lists_later_parts_keeps_the_original_open_as_refined() {
    let w = World::new("refine");
    filed_by_refine(&w, MORE_PARTS, &[I12]);
    w.poll();
    assert!(
        w.changes(
            7,
            &[
                Off(Mark::State(State::Planning)),
                Off(Mark::State(State::Replaced)),
                On(Mark::State(State::Refined))
            ]
        ),
        "{:?}",
        w.writes_of(7)
    );
    assert!(
        !w.changes(7, &[On(Mark::State(State::Replaced))]),
        "{:?}",
        w.writes_of(7)
    );
    assert!(!w.closed(7));
    assert_eq!(w.rec_open(), "none");
}

#[test]
fn a_filed_body_that_cannot_be_read_keeps_the_original_open() {
    let w = World::new("refine");
    filed_by_refine(&w, "x", &[I12]);
    w.fx("issue-12.json", "{\"title\":\"no body here\"\n");
    let r = w.poll();
    has(&r.out, "WARN: could not read #12, which refining #7 filed");
    assert!(
        w.changes(7, &[On(Mark::State(State::Refined))]),
        "{:?}",
        w.writes_of(7)
    );
    assert!(!w.closed(7));
}

#[test]
fn a_filed_body_that_already_names_the_original_is_not_written_again() {
    let w = World::new("refine");
    filed_by_refine(&w, &format!("Replaces #7\n\n{ONE_PART}"), &[I12]);
    w.poll();
    assert!(w.patched(12).is_empty());
    assert_eq!(w.marks(Some(12)), 0);
    assert!(w.closed(7), "{:?}", w.writes_of(7));
}

#[test]
fn a_sub_issue_link_that_fails_is_a_warning_and_the_rest_goes_on() {
    let w = World::new("refine");
    filed_by_refine(&w, ONE_PART, &[I12]);
    w.touch("part-fails");
    let r = w.poll();
    has(&r.out, "WARN: could not link #12 as a sub-issue of #7");
    assert!(w.closed(7), "{:?}", w.writes_of(7));
}

#[test]
fn a_label_change_that_fails_is_repeated_by_the_next_poll_and_replaced_by_is_said_once() {
    let w = World::new("refine");
    filed_by_refine(&w, ONE_PART, &[I12]);
    w.fx("edit-fails-on", "--add-label tehas:replaced\n");
    let r = w.poll();
    has(
        &r.out,
        "could not mark #7 tehas:replaced; the next poll replaces it",
    );
    lacks(&w.gh(), "Replaced by");
    assert!(!w.closed(7));
    assert_eq!(w.rec_open(), "open");
    w.clear_calls();
    w.rm_fx("edit-fails-on");
    w.fx("refines.json", "[]");
    w.poll();
    assert!(
        w.changes(7, &[On(Mark::State(State::Replaced))]),
        "{:?}",
        w.writes_of(7)
    );
    assert_eq!(holding(&w.gh(), "Replaced by"), 1);
    assert!(w.closed(7), "{:?}", w.writes_of(7));
    assert_eq!(w.rec_open(), "none");
}

#[test]
fn an_original_that_carries_the_mark_already_is_only_closed_nothing_is_said_or_linked_again() {
    let w = World::new("refine");
    filed_by_refine(&w, &format!("Replaces #7\n\n{ONE_PART}"), &[I12]);
    w.fx(
        "issue-7.json",
        r#"{"title":"Faster search","body":"Make it fast."}"#,
    );
    // The marks the replace reads now stand in the ticket read, where
    // the labels of the issue's view used to answer it.
    w.fx_json("labels-7.json", &json!([{"name": "tehas:replaced"}]));
    w.poll();
    assert!(w.closed(7), "{:?}", w.writes_of(7));
    assert!(w.patched(12).is_empty());
    assert_eq!(w.marks(Some(12)), 0);
    assert!(!w.parted(7, 12));
    lacks(&w.gh(), "Replaced by");
    assert_eq!(w.rec_open(), "none");
}

#[test]
fn a_mark_an_earlier_refine_left_marks_nothing_the_new_filed_issue_is_linked_and_named() {
    let w = World::new("refine");
    filed_by_refine(&w, ONE_PART, &[I12]);
    w.fx(
        "issue-7.json",
        r#"{"title":"Faster search","body":"Make it fast.","labels":[{"name":"tehas:refined"}]}"#,
    );
    w.poll();
    let gh = w.gh();
    assert!(w.wrote(12, None, "Replaces #7"), "{:?}", w.patched(12));
    assert!(w.parted(7, 12));
    assert!(
        w.changes(
            7,
            &[
                Off(Mark::State(State::Refined)),
                On(Mark::State(State::Replaced))
            ]
        ),
        "{:?}",
        w.writes_of(7)
    );
    has(&gh, "Replaced by #12.");
    assert!(w.closed(7), "{:?}", w.writes_of(7));
}

#[test]
fn a_filed_issue_whose_replaces_line_cannot_be_written_is_not_marked_and_the_next_poll_tries_again()
{
    let w = World::new("refine");
    filed_by_refine(&w, ONE_PART, &[I12]);
    w.fx("edit-fails-on", "issue edit 12\n");
    let r = w.poll();
    has(
        &r.out,
        "could not say on #12 that it replaces #7; the next poll replaces it",
    );
    assert!(
        !w.changes(7, &[On(Mark::State(State::Replaced))]),
        "{:?}",
        w.writes_of(7)
    );
    lacks(&w.gh(), "Replaced by");
    assert_eq!(w.rec_open(), "open");
    w.clear_calls();
    w.rm_fx("edit-fails-on");
    w.fx("refines.json", "[]");
    w.poll();
    assert!(w.wrote(12, None, "Replaces #7"), "{:?}", w.patched(12));
    assert!(
        w.changes(7, &[On(Mark::State(State::Replaced))]),
        "{:?}",
        w.writes_of(7)
    );
    assert_eq!(w.rec_open(), "none");
}

#[test]
fn a_filed_address_that_is_not_an_issue_address_is_left_out_and_the_session_still_ends() {
    let w = World::new("refine");
    filed_by_refine(
        &w,
        ONE_PART,
        &[
            I12,
            "https://github.com/acme/widget/issues/12/",
            "https://github.com/acme/widget/issues/13#x",
            "@mallory",
        ],
    );
    w.poll();
    let gh = w.gh();
    has(&gh, "Replaced by #12.");
    lacks(&gh, "mallory");
    lacks(&gh, "issue view 13");
    assert!(w.closed(7), "{:?}", w.writes_of(7));
    assert_eq!(w.rec_open(), "none");
}

#[test]
fn a_filed_issue_of_another_repository_gets_no_links_and_is_named_the_original_stays_open() {
    let w = World::new("refine");
    filed_by_refine(
        &w,
        ONE_PART,
        &[I12, "https://github.com/other/repo/issues/3"],
    );
    w.poll();
    let gh = w.gh();
    has(
        &gh,
        "Replaced by #12, https://github.com/other/repo/issues/3.",
    );
    assert!(w.patched(3).is_empty());
    assert_eq!(w.marks(Some(3)), 0);
    assert!(w.parted(7, 12));
    assert!(
        w.changes(7, &[On(Mark::State(State::Refined))]),
        "{:?}",
        w.writes_of(7)
    );
}

#[test]
fn the_repository_is_compared_without_its_case() {
    let w = World::new("refine");
    filed_by_refine(&w, ONE_PART, &["https://github.com/Acme/Widget/issues/12"]);
    w.poll();
    has(&w.gh(), "Replaced by #12.");
    assert!(w.closed(7), "{:?}", w.writes_of(7));
}

#[test]
fn a_plan_session_that_files_is_posted_as_before() {
    let w = World::new("refine");
    w.to_plan();
    w.fx_json(
        "plan-result.json",
        &json!({"session": "abc-123", "text": format!("Filed {I12}."), "question": null,
            "filed": [I12]}),
    );
    w.poll();
    has(&w.gh(), &format!("Filed in this turn: {I12}"));
    lacks(&w.gh(), "Replaced by");
    assert!(!w.closed(7));
}

#[test]
fn a_refine_turn_that_filed_nothing_asks_as_before() {
    let w = World::new("refine");
    w.to_refine();
    w.poll();
    has(&w.gh(), "<!-- tehas:question kind=plan issue=7 turn=1 -->");
    lacks(&w.gh(), "Replaced by");
}
