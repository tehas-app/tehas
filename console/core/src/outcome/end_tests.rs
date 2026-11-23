//! The end word: each row of its table, each sample line of the events spec,
//! and every combination the analytics' tests ran before the move.

use super::*;
use serde_json::json;

fn row(outcome: &'static str, status: i64, change: bool, parked: bool) -> Line<'static> {
    Line {
        outcome: Some(outcome),
        status: Some(status),
        change,
        parked,
        failure_kind: None,
    }
}

#[test]
fn each_row_of_the_table_decides_its_end() {
    // Each case matches its row and no row above it.
    let cases: [(u8, Line, &str); 16] = [
        (1, row("merged", 0, true, false), "merged"),
        (2, row("closed", 10, false, false), "stopped"),
        (2, row("interrupted", 21, true, false), "stopped"),
        (2, row("stopped", 10, false, false), "stopped"),
        (3, row("refused", 78, false, false), "refused"),
        (3, row("refused", 10, true, true), "refused"),
        (4, row("running", 3, true, false), "refused"),
        (4, row("handoff", 40, false, true), "refused"),
        (5, row("handoff", 10, true, false), "offered"),
        (5, row("running", 30, true, false), "offered"),
        (6, row("handoff", 10, true, true), "handoff"),
        (6, row("handoff", 10, false, false), "handoff"),
        (7, row("lost", 10, false, false), "failed"),
        (
            7,
            Line {
                failure_kind: Some("verify-failed"),
                ..row("running", 10, false, true)
            },
            "failed",
        ),
        (
            8,
            Line {
                failure_kind: Some("cap"),
                ..row("running", 10, false, true)
            },
            "parked",
        ),
        (8, row("parked", 10, false, true), "parked"),
    ];
    for (n, l, want) in cases {
        assert_eq!(end(&l), want, "row {n}: {l:?}");
    }
    for w in [
        "killed", "stalled", "timeout", "aborted", "unknown", "failed", "lost",
    ] {
        assert_eq!(end(&row(w, 10, false, true)), "failed", "{w}");
    }
    assert_eq!(end(&Line::default()), "failed", "no status word is unknown");
}

#[test]
fn each_sample_line_of_the_spec_ends_as_it_says() {
    let samples = [
        json!({"cycle": "7-1", "outcome": "merged", "status": 0, "parked": false, "change": 9, "end": "merged"}),
        json!({"cycle": "12-4", "outcome": "closed", "status": 10, "parked": false, "end": "stopped"}),
        json!({"cycle": "14-2", "outcome": "refused", "status": 78, "parked": false, "end": "refused"}),
        json!({"cycle": "15-1", "outcome": "handoff", "status": 10, "parked": false, "change": 21, "end": "offered"}),
        json!({"cycle": "16-3", "outcome": "handoff", "status": 10, "parked": true, "change": 22, "end": "handoff"}),
        json!({"cycle": "12-3", "outcome": "failed", "status": 10, "failureKind": "verify-failed", "parked": true, "end": "failed"}),
        json!({"cycle": "18-1", "outcome": "running", "status": 10, "cap": "step-failed:develop", "failureKind": "cap", "parked": true, "end": "parked"}),
    ];
    let mut seen = std::collections::BTreeSet::new();
    for s in &samples {
        let got = end(&Line {
            outcome: s["outcome"].as_str(),
            status: s["status"].as_i64(),
            change: s.get("change").is_some(),
            parked: s["parked"].as_bool().unwrap_or(false),
            failure_kind: s["failureKind"].as_str(),
        });
        assert_eq!(got, s["end"], "{s}");
        seen.insert(got);
    }
    assert_eq!(seen.len(), OUTCOMES.len(), "one sample per word");
}

// The decision as the analytics made it before the move, in its own words
// (`pr` for `offered`). It stays here only to pin which ends moved.
fn before(l: &Line) -> &'static str {
    let status = l.outcome.unwrap_or(UNKNOWN);
    let kind = l.failure_kind.unwrap_or("");
    match status {
        "merged" => return "merged",
        "closed" | "interrupted" => return "stopped",
        "refused" => return "refused",
        _ => {}
    }
    if matches!(l.status, Some(3 | 4 | 5 | 40 | 64 | 78)) {
        return "refused";
    }
    if l.change && !l.parked {
        return "pr";
    }
    if status == "handoff" {
        return "handoff";
    }
    let failed = ["killed", "stalled", "timeout", "aborted", UNKNOWN, "failed"];
    if failed.contains(&status) || (!kind.is_empty() && kind != "cap") {
        return "failed";
    }
    "parked"
}

/// Which of the four moves a changed end is, or None for one that should
/// not have moved.
fn moved(l: &Line, was: &str, now: &str) -> Option<&'static str> {
    let outcome = l.outcome.unwrap_or(UNKNOWN);
    match (outcome, was, now) {
        ("stopped", _, "stopped") => Some("tehas stop ends stopped"),
        ("lost", "parked", "failed") => Some("a lost cycle ends failed"),
        ("merged", "merged", _) if l.parked => Some("a parked merged line ends by the later rows"),
        ("closed" | "interrupted", "stopped", _) if l.parked => {
            Some("a parked closed or interrupted line ends by the later rows")
        }
        _ => None,
    }
}

#[test]
fn every_combination_the_analytics_ran_ends_as_before_or_by_one_of_the_four_moves() {
    let mut moves = std::collections::BTreeSet::new();
    for outcome in STATUSES.iter().copied().map(Some).chain([None]) {
        for status in [None, Some(0), Some(3), Some(30)] {
            for change in [false, true] {
                for failure_kind in [None, Some("cap"), Some("model")] {
                    for parked in [false, true] {
                        let l = Line {
                            outcome,
                            status,
                            change,
                            parked,
                            failure_kind,
                        };
                        let now = end(&l);
                        assert!(OUTCOMES.contains(&now), "{now} from {l:?}");
                        if parked {
                            assert!(STOPS.contains(&now), "a parked line ends {now}: {l:?}");
                        }
                        let was = match before(&l) {
                            "pr" => "offered",
                            w => w,
                        };
                        if was != now {
                            let why = moved(&l, was, now);
                            assert!(why.is_some(), "{l:?} was {was}, is {now}");
                            moves.extend(why);
                        }
                    }
                }
            }
        }
    }
    assert_eq!(moves.len(), 4, "each of the four moves is seen: {moves:?}");
}
