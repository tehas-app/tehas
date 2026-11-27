//! The events a cycle raises (docs/specs/2026-11-22-events.md, T11.8): one
//! log line each, in order, and each the fact the store then holds.

mod common;

use common::{site_of, World};
use std::sync::Arc;
use tehas_core::raise::Writer;
use tehas_cycle::{after, events};
use tehas_ports::events::{Envelope, Event, Name};
use tehas_ports::fake::FakeSite;

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
    // The binary: its start first, then the events of its end. The stub gh
    // takes no note the adapter sends, so its question is shown in process
    // below.
    let w = World::new("events-park");
    let r = w.run7();
    let all = raised(&r.out);
    let but_asked: Vec<Name> = names(&all)
        .into_iter()
        .filter(|n| *n != Name::QuestionAsked)
        .collect();
    assert_eq!(
        but_asked,
        [Name::CycleStarted, Name::CycleParked, Name::CycleEnded],
        "{}",
        r.out
    );
    let line = w.history0();
    for e in &all {
        match &e.event {
            Event::CycleStarted(p) => {
                assert_eq!(p.started.secs(), line["started_at"].as_i64().unwrap());
            }
            Event::CycleParked(p) => assert_eq!(p.at.secs(), line["at"].as_i64().unwrap()),
            Event::CycleEnded(p) => {
                assert!(p.parked);
                assert_eq!(p.ended.secs(), line["at"].as_i64().unwrap());
                assert_eq!(p.outcome, line["status"].as_str().unwrap());
            }
            Event::QuestionAsked(_) => {}
            other => panic!("{other:?}"),
        }
        if e.name() != Name::QuestionAsked {
            assert_eq!(e.key.as_str(), "7-1");
        }
    }
    // In process: the report asks its question on the fake, its end goes on
    // record as tehas-cycle records it, and the end raises the park, the
    // question and the end, in that order.
    let w = World::new("events-park-asked");
    let t = w.report_through(&site_of(&w), &FakeSite::new(&site_of(&w)));
    let end: Vec<&str> = t.end.iter().map(String::as_str).collect();
    w.store_locked(&end);
    let d = events::wiring(
        &w.home,
        Arc::new(|_| String::new()),
        Arc::new(|_| panic!("no code host handle is made while plugins are off")),
    );
    let ((), logged) =
        after::holding(|| events::raise_ended(&d, &w.home, 7, 1, Writer::Report, &mut |_| None));
    let all = raised(&logged);
    assert_eq!(
        names(&all),
        [Name::CycleParked, Name::QuestionAsked, Name::CycleEnded],
        "{logged}"
    );
    let line = w.history0();
    let q = w.row(&["question-list", "--after", "0"])["questions"][0].clone();
    assert_eq!(w.park_q(), "4002");
    for e in &all {
        match &e.event {
            Event::CycleParked(p) => assert_eq!(p.at.secs(), line["at"].as_i64().unwrap()),
            Event::QuestionAsked(p) => {
                assert_eq!(p.question.get(), q["id"].as_u64().unwrap());
                assert_eq!(p.asked_at.secs(), q["posted_at"].as_i64().unwrap());
                assert_eq!(e.key.as_str(), q["id"].to_string());
            }
            Event::CycleEnded(p) => {
                assert!(p.parked);
                assert_eq!(p.ended.secs(), line["at"].as_i64().unwrap());
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
fn a_merged_change_ends_and_raises_its_merge_once() {
    // The line the report ended `merged` is the fact of the merge: the
    // cycle raises `change.merged` with it, once, keyed on the change
    // number, and offers that change again to nobody. The merge time is
    // asked of the code-host port at the raise; here the stub gh answers
    // the question for nothing, so the event carries the line's end: the
    // ask is an ask, and its failure changes nothing else.
    let w = World::new("events-merged");
    w.ended("merged", "", "feature/x", "-");
    w.merged_pr();
    let r = w.run7();
    let all = raised(&r.out);
    assert_eq!(
        names(&all),
        [Name::CycleStarted, Name::CycleEnded, Name::ChangeMerged],
        "{}",
        r.out
    );
    let line = w.history0();
    assert_eq!(line["status"].as_str().unwrap(), "merged");
    assert_eq!(line["pr"].as_u64().unwrap(), 9);
    let Event::CycleEnded(p) = &all[1].event else {
        unreachable!()
    };
    assert!(!p.parked);
    assert_eq!(p.ended.secs(), line["at"].as_i64().unwrap());
    let Event::ChangeMerged(m) = &all[2].event else {
        unreachable!()
    };
    assert_eq!(m.change.get(), 9);
    assert_eq!(m.at.secs(), line["at"].as_i64().unwrap());
    assert_eq!(all[2].key.as_str(), "9");
}

// A start refused after it is on record wrote its own line, with no report:
// it raises its end itself, once (T11.9).
#[test]
fn a_start_refused_before_its_runner_raises_its_start_and_its_end_once() {
    let w = World::new("events-refused");
    w.fx("issue-7.json", r#"{"number":7,"state":"closed"}"#);
    let r = w.run7();
    assert_eq!(r.rc, 65, "{}", r.out);
    let all = raised(&r.out);
    assert_eq!(
        names(&all),
        [Name::CycleStarted, Name::CycleEnded],
        "{}",
        r.out
    );
    let Event::CycleEnded(p) = &all[1].event else {
        unreachable!()
    };
    assert_eq!(all[1].key.as_str(), "7-1");
    assert_eq!(p.end, tehas_ports::events::End::Refused);
    assert_eq!((p.outcome.as_str(), p.status), ("refused", Some(65)));
    let line = w.history0();
    assert_eq!(line["reported"], false);
    assert_eq!(p.ended.secs(), line["at"].as_i64().unwrap());
}

// A start that ends an open record as lost wrote that record's line: it
// raises that end before its own start, and its own end after its report.
#[test]
fn a_start_that_ended_an_open_record_as_lost_raises_that_end_first() {
    let w = World::new("events-replaced");
    w.store_locked(&["cycle-start", "7", "--by-hand"]);
    let r = w.run7();
    let all = raised(&r.out);
    let ends: Vec<(&str, Name)> = all
        .iter()
        .map(|e| (e.key.as_str(), e.name()))
        .filter(|(_, n)| matches!(n, Name::CycleEnded | Name::CycleStarted))
        .collect();
    assert_eq!(
        ends,
        [
            ("7-1", Name::CycleEnded),
            ("7-2", Name::CycleStarted),
            ("7-2", Name::CycleEnded)
        ],
        "{}",
        r.out
    );
    let Event::CycleEnded(lost) = &all[0].event else {
        unreachable!()
    };
    assert_eq!(
        (lost.outcome.as_str(), lost.end),
        ("lost", tehas_ports::events::End::Failed)
    );
}
