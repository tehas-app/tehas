//! The notes a cycle leaves beside its number, `state/cycles/<issue>.<name>`,
//! in bare names. Every place that clears them takes its names from here, so
//! a note is added in one place and no clearing step can miss it.
//!
//! `<issue>.end-failed.json` is not one of them: the report writes or removes
//! it and the poll finishes it after the cycle has ended (`end_fault`). The
//! cycle's record, `<issue>.json` and `.manifest.json`, is not one either.

/// Held while a cycle runs: its lock and its container's id.
pub const RUNNING: [&str; 2] = ["lock", "cid"];

/// How the container ended, as the runner wrote it. A stop keeps it with the
/// cycle's record.
pub const RECORD: [&str; 2] = ["runner_exit", "status"];

/// What the run said of itself and what parks or holds the issue.
pub const SAID: [&str; 17] = [
    "oom",
    "forks",
    "stalled",
    "timed",
    "signal",
    "disk",
    "stopped",
    "done",
    "nostart",
    "lean-refused",
    "refused",
    "refresh-conflict",
    "comments",
    "result.json",
    "progress.json",
    "freeze",
    "frozen",
];

/// A note written beside its name and moved into place: a step that clears
/// the note clears a write that never finished.
pub const NEW: [&str; 4] = [
    "stopped.new",
    "nostart.new",
    "runner_exit.new",
    "status.new",
];

/// The notes that a cycle's end clears once it has reported. The rest stay
/// for what reads them after the end: the result and the progress for the
/// record, a freeze, a park's reasons and the comments mode for the next
/// step on the issue.
pub const ENDED: [&str; 12] = [
    "oom",
    "forks",
    "stalled",
    "timed",
    "signal",
    "disk",
    "stopped",
    "nostart",
    "refused",
    "done",
    "runner_exit",
    "status",
];

/// Every note.
pub fn all() -> impl Iterator<Item = &'static str> {
    RUNNING.into_iter().chain(RECORD).chain(SAID).chain(NEW)
}

/// What `tehas stop` takes: every note but the runner's record, and but
/// `stopped.new`, which goes with the marker the stop writes.
pub fn stopped() -> impl Iterator<Item = &'static str> {
    all().filter(|n| !RECORD.contains(n) && *n != "stopped.new")
}

/// What a cycle clears before it starts, of the one before it killed outright:
/// every note but what the running cycle holds.
pub fn started() -> impl Iterator<Item = &'static str> {
    all().filter(|n| !RUNNING.contains(n))
}

/// Whether `name` is a note.
pub fn is_note(name: &str) -> bool {
    all().any(|n| n == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn each_name_is_bare_and_once() {
        let mut seen = HashSet::new();
        for n in all() {
            assert!(!n.starts_with('.') && !n.is_empty(), "{n}");
            assert!(seen.insert(n), "twice: {n}");
        }
    }

    #[test]
    fn the_end_clears_notes_only() {
        for n in ENDED {
            assert!(is_note(n), "{n}");
        }
    }

    #[test]
    fn a_stop_takes_what_says_a_cycle_runs_and_keeps_the_record() {
        let s: Vec<_> = stopped().collect();
        for n in ["lock", "cid", "disk", "comments", "freeze", "nostart.new"] {
            assert!(s.contains(&n), "{n}");
        }
        for n in ["runner_exit", "status", "stopped.new"] {
            assert!(!s.contains(&n), "{n}");
        }
    }

    #[test]
    fn a_start_keeps_its_own_lock_and_clears_the_rest() {
        let s: Vec<_> = started().collect();
        assert_eq!(s.len(), all().count() - RUNNING.len());
        for n in ["freeze", "frozen", "disk", "comments", "status.new"] {
            assert!(s.contains(&n), "{n}");
        }
        assert!(!s.contains(&"lock") && !s.contains(&"cid"));
    }

    #[test]
    fn the_failed_end_is_not_a_note() {
        assert!(!is_note("end-failed.json"));
    }
}
