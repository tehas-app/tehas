//! The events a cycle raises (docs/specs/2026-11-22-events.md, T11.8): one
//! log line each, in order, and each the fact the store then holds.

mod common;

use common::World;
use tehas_ports::events::{Envelope, Event, Name};

/// The envelopes the cycle's log listener wrote, in order.
fn raised(out: &str) -> Vec<Envelope> {
    out.lines()
        .filter_map(|l| {
            l.split_once(" event {")
                .map(|(_, rest)| format!("{{{rest}"))
        })
        .map(|json| serde_json::from_str(&json).unwrap_or_else(|e| panic!("{json}: {e}")))
        .collect()
}

fn names(all: &[Envelope]) -> Vec<Name> {
    all.iter().map(Envelope::name).collect()
}

#[test]
fn a_park_raises_its_start_its_park_its_question_and_its_end_in_order() {
    let w = World::new("events-park");
    let r = w.run7();
    let all = raised(&r.out);
    assert_eq!(
        names(&all),
        [
            Name::CycleStarted,
            Name::CycleParked,
            Name::QuestionAsked,
            Name::CycleEnded
        ],
        "{}",
        r.out
    );
    let line = w.history0();
    let q = w.row(&["question-list", "--after", "0"])["questions"][0].clone();
    for e in &all {
        match &e.event {
            Event::CycleStarted(p) => {
                assert_eq!(p.started.secs(), line["started_at"].as_i64().unwrap());
            }
            Event::CycleParked(p) => assert_eq!(p.at.secs(), line["at"].as_i64().unwrap()),
            Event::QuestionAsked(p) => {
                assert_eq!(p.question.get(), q["id"].as_u64().unwrap());
                assert_eq!(p.asked_at.secs(), q["posted_at"].as_i64().unwrap());
                assert_eq!(e.key.as_str(), q["id"].to_string());
            }
            Event::CycleEnded(p) => {
                assert!(p.parked);
                assert_eq!(p.ended.secs(), line["at"].as_i64().unwrap());
                assert_eq!(p.outcome, line["status"].as_str().unwrap());
            }
            other => panic!("{other:?}"),
        }
        if e.name() != Name::QuestionAsked {
            assert_eq!(e.key.as_str(), "7-1");
        }
    }
}

#[test]
fn a_change_left_for_review_is_offered_after_the_end() {
    let w = World::new("events-offered");
    w.ended("handoff", "commit-pr", "feature/x", "");
    w.fx("pr.txt", "https://github.com/acme/widget/pull/9\n");
    let r = w.run7();
    assert_eq!(r.rc, 0, "{}", r.out);
    let all = raised(&r.out);
    assert_eq!(
        names(&all),
        [Name::CycleStarted, Name::CycleEnded, Name::ChangeOffered],
        "{}",
        r.out
    );
    let Event::ChangeOffered(offer) = &all[2].event else {
        unreachable!()
    };
    assert_eq!(offer.change.get(), 9);
    assert_eq!(w.history0()["pr"], 9);
}

#[test]
fn a_merged_change_ends_and_is_not_offered() {
    let w = World::new("events-merged");
    w.ended("merged", "", "feature/x", "-");
    w.merged_pr();
    let r = w.run7();
    let all = raised(&r.out);
    assert_eq!(
        names(&all),
        [Name::CycleStarted, Name::CycleEnded],
        "{}",
        r.out
    );
}
