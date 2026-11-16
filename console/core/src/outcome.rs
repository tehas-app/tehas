//! The one vocabulary of how a cycle stands and how a run ended. The store
//! keeps a cycle's status and end reason in these words, the MCP server
//! promises them in its schemas, and analytics counts runs by the outcomes.

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

/// How a run ended, in the words of the runs table:
/// `pr` an open pull request, not parked; `merged`; `handoff` to a person;
/// `parked`; `failed`; `refused`, no cycle ran; `stopped`, closed or
/// interrupted.
pub const OUTCOMES: [&str; 7] = [
    "pr", "merged", "handoff", "parked", "failed", "refused", "stopped",
];

/// The outcomes of a run that stopped for a person: the ones a park comment
/// follows.
pub const STOPS: [&str; 4] = ["handoff", "parked", "failed", "refused"];

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
}
