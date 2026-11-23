//! The one vocabulary of how a cycle stands and how a run ended, and the one
//! decision of a work line's end. The store keeps a cycle's status and end
//! reason in these words, the MCP server promises them in its schemas, and
//! analytics counts runs by the ends `end` decides.

use crate::exit;

/// The word for a status no writer of this vocabulary said.
pub const UNKNOWN: &str = "unknown";

/// The runner's status of an issue in its result: pi-rukas's pipeline states,
/// and `not-started` for a group member it never ran.
pub const RUNNER: [&str; 6] = [
    "running",
    "merged",
    "handoff",
    "aborted",
    "interrupted",
    "not-started",
];

/// tehas's own words: for a cycle it stopped (`killed`, `stalled`,
/// `timeout`, `stopped`), whose issue closed under it, that never ran
/// (`refused`), or that died without saying so (`lost`).
pub const TEHAS: [&str; 7] = [
    "killed", "stalled", "timeout", "stopped", "closed", "refused", "lost",
];

/// What `cycles.end_reason` can hold: the CHECK list of the store's schema.
/// A status word outside it ends as `unknown`.
pub const END_REASONS: [&str; 12] = [
    "closed",
    "merged",
    "handoff",
    "parked",
    "failed",
    "stopped",
    "timeout",
    "killed",
    "lost",
    "interrupted",
    "stalled",
    UNKNOWN,
];

/// Every status word a cycle can carry: the runner's, tehas's, the end
/// reasons the store keeps, and `unknown`.
pub const STATUSES: [&str; 16] = [
    "running",
    "merged",
    "handoff",
    "aborted",
    "interrupted",
    "not-started",
    "killed",
    "stalled",
    "timeout",
    "stopped",
    "closed",
    "refused",
    "lost",
    "parked",
    "failed",
    UNKNOWN,
];

/// The word itself when a cycle can carry it, `unknown` for any other.
pub fn status(word: &str) -> &str {
    if STATUSES.contains(&word) {
        word
    } else {
        UNKNOWN
    }
}

/// The end reason of a status word: the word when `cycles.end_reason` takes
/// it, `unknown` for any other.
pub fn end_reason(word: &str) -> &str {
    if END_REASONS.contains(&word) {
        word
    } else {
        UNKNOWN
    }
}

/// How a run ended, in the words of the runs table: `offered` an open
/// change, not parked; `merged`; `handoff` to a person; `parked`; `failed`;
/// `refused`, no cycle ran; `stopped`, closed, interrupted or stopped. `end`
/// decides which. The set is closed: a new word is a major change.
pub const OUTCOMES: [&str; 7] = [
    "offered", "merged", "handoff", "parked", "failed", "refused", "stopped",
];

/// The outcomes of a run that stopped for a person: the ones a park comment
/// follows.
pub const STOPS: [&str; 4] = ["handoff", "parked", "failed", "refused"];

/// The runner's exit status when it refused the issue.
pub const RUNNER_REFUSED: u8 = 40;

/// The exit statuses of a start that ran no cycle: the runner refused, or
/// `tehas work` started none.
pub const REFUSED_EXITS: [u8; 6] = [
    exit::work::NO_PUSH,
    exit::work::NOT_STARTED,
    exit::work::BUSY,
    RUNNER_REFUSED,
    exit::USAGE,
    exit::SETTINGS,
];

/// The status words of a cycle that failed: tehas stopped it (`killed`,
/// `stalled`, `timeout`), the runner aborted, it said nothing readable, or
/// it died without a word (`lost`).
pub const FAILED: [&str; 7] = [
    "killed", "stalled", "timeout", "aborted", UNKNOWN, "failed", "lost",
];

/// What decides a work line's end. `outcome` is the status word, `None`
/// when the line has none, which counts as `unknown`; `status` the exit
/// status; `change` whether the line names a change.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Line<'a> {
    pub outcome: Option<&'a str>,
    pub status: Option<i64>,
    pub change: bool,
    pub parked: bool,
    pub failure_kind: Option<&'a str>,
}

/// The end of a work line, a word of `OUTCOMES`: the first row that
/// matches.
///
/// 1. `merged`, not parked: `merged`
/// 2. `closed`, `interrupted` or `stopped`, not parked: `stopped`
/// 3. `refused`: `refused`
/// 4. an exit status of `REFUSED_EXITS`: `refused`
/// 5. a change, not parked: `offered`
/// 6. `handoff`: `handoff`
/// 7. a word of `FAILED`, or a failure kind other than `cap`: `failed`
/// 8. any other line: `parked`
///
/// A parked line never ends `merged`, `stopped` or `offered`, so every
/// parked line ends with a word of `STOPS`.
pub fn end(line: &Line) -> &'static str {
    let outcome = line.outcome.unwrap_or(UNKNOWN);
    let free = !line.parked;
    if outcome == "merged" && free {
        "merged"
    } else if matches!(outcome, "closed" | "interrupted" | "stopped") && free {
        "stopped"
    } else if outcome == "refused"
        || line
            .status
            .is_some_and(|s| REFUSED_EXITS.iter().any(|&r| i64::from(r) == s))
    {
        "refused"
    } else if line.change && free {
        "offered"
    } else if outcome == "handoff" {
        "handoff"
    } else if FAILED.contains(&outcome)
        || line
            .failure_kind
            .is_some_and(|k| !k.is_empty() && k != "cap")
    {
        "failed"
    } else {
        "parked"
    }
}

#[cfg(test)]
mod end_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn set(words: &[&'static str]) -> BTreeSet<&'static str> {
        let s: BTreeSet<_> = words.iter().copied().collect();
        assert_eq!(s.len(), words.len(), "a word twice in {words:?}");
        s
    }

    #[test]
    fn the_statuses_are_the_runner_tehas_and_the_end_reasons() {
        let mut want = set(&RUNNER);
        want.extend(set(&TEHAS));
        want.extend(set(&END_REASONS));
        want.insert(UNKNOWN);
        assert_eq!(set(&STATUSES), want);
    }

    #[test]
    fn a_word_outside_is_unknown() {
        for w in STATUSES {
            assert_eq!(status(w), w);
        }
        for w in ["", "completed", "crash", "Merged", "not an end", "landed"] {
            assert_eq!(status(w), UNKNOWN, "{w}");
        }
    }

    #[test]
    fn an_end_reason_is_a_status_and_the_rest_end_as_unknown() {
        for w in END_REASONS {
            assert!(STATUSES.contains(&w), "{w}");
            assert_eq!(end_reason(w), w);
        }
        for w in ["running", "aborted", "refused", "not-started", ""] {
            assert_eq!(end_reason(w), UNKNOWN, "{w}");
        }
    }

    #[test]
    fn the_stops_are_outcomes() {
        set(&OUTCOMES);
        for w in STOPS {
            assert!(OUTCOMES.contains(&w), "{w}");
        }
    }

    // A new outcome word is a stop or not by a decision, not by being left out
    // of the stops: the outcomes that are no stop are exactly these three.
    #[test]
    fn the_outcomes_that_are_no_stop_are_an_offer_a_merge_and_stopped() {
        let rest: BTreeSet<_> = set(&OUTCOMES).difference(&set(&STOPS)).copied().collect();
        assert_eq!(rest, set(&["offered", "merged", "stopped"]));
    }

    #[test]
    fn every_failed_word_is_a_status() {
        for w in set(&FAILED) {
            assert!(STATUSES.contains(&w), "{w}");
        }
    }
}
