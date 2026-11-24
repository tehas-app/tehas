//! Whether a pull request can merge, from GitHub's two words for it: the
//! one reading of `mergeable` and `mergeStateStatus` for every port of the
//! adapter that gives a change's `Mergeable`.

use tehas_ports::{Mergeable, NotMergeable};

/// The port's `Mergeable` of GitHub's `mergeable` and `mergeStateStatus`.
/// A conflict wins, whatever the other word says. While GitHub has not
/// worked out whether the branch conflicts (`UNKNOWN`, or a `mergeable`
/// word this adapter does not know), the answer is `Unknown`, never another
/// reason, so a conflicting draft is asked again rather than read as a
/// draft. A mergeable branch is held by being behind, a rule or a draft;
/// checks that fail but do not hold the merge leave it mergeable. A state
/// word this adapter does not know, on a branch GitHub says does not
/// conflict, is `No(Unknown)`.
pub(crate) fn mergeable(mergeable: &str, status: &str) -> Mergeable {
    match (mergeable, status) {
        ("CONFLICTING", _) | (_, "DIRTY") => Mergeable::No(NotMergeable::Conflict),
        ("MERGEABLE", "UNKNOWN") => Mergeable::Unknown,
        ("MERGEABLE", "CLEAN" | "HAS_HOOKS" | "UNSTABLE") => Mergeable::Yes,
        ("MERGEABLE", "BEHIND") => Mergeable::No(NotMergeable::Behind),
        ("MERGEABLE", "BLOCKED") => Mergeable::No(NotMergeable::Blocked),
        ("MERGEABLE", "DRAFT") => Mergeable::No(NotMergeable::Draft),
        ("MERGEABLE", _) => Mergeable::No(NotMergeable::Unknown),
        _ => Mergeable::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::mergeable;
    use tehas_ports::{Mergeable, NotMergeable};

    const STATES: [&str; 8] = [
        "BEHIND",
        "BLOCKED",
        "CLEAN",
        "DIRTY",
        "DRAFT",
        "HAS_HOOKS",
        "UNKNOWN",
        "UNSTABLE",
    ];

    #[test]
    fn a_conflict_wins_whatever_the_state_says() {
        for status in STATES {
            assert_eq!(
                mergeable("CONFLICTING", status),
                Mergeable::No(NotMergeable::Conflict),
                "{status}"
            );
        }
        for word in ["MERGEABLE", "UNKNOWN", "NEW"] {
            assert_eq!(
                mergeable(word, "DIRTY"),
                Mergeable::No(NotMergeable::Conflict),
                "{word}"
            );
        }
    }

    #[test]
    fn a_conflict_not_worked_out_is_unknown_never_another_reason() {
        for status in STATES.into_iter().filter(|s| *s != "DIRTY") {
            assert_eq!(mergeable("UNKNOWN", status), Mergeable::Unknown, "{status}");
            assert_eq!(mergeable("NEW", status), Mergeable::Unknown, "{status}");
        }
        assert_eq!(mergeable("MERGEABLE", "UNKNOWN"), Mergeable::Unknown);
    }

    #[test]
    fn a_mergeable_branch_reads_what_holds_it() {
        for status in ["CLEAN", "HAS_HOOKS", "UNSTABLE"] {
            assert_eq!(mergeable("MERGEABLE", status), Mergeable::Yes, "{status}");
        }
        assert_eq!(
            mergeable("MERGEABLE", "BEHIND"),
            Mergeable::No(NotMergeable::Behind)
        );
        assert_eq!(
            mergeable("MERGEABLE", "BLOCKED"),
            Mergeable::No(NotMergeable::Blocked)
        );
        assert_eq!(
            mergeable("MERGEABLE", "DRAFT"),
            Mergeable::No(NotMergeable::Draft)
        );
    }

    #[test]
    fn a_state_word_not_known_on_a_branch_that_does_not_conflict_is_no_for_a_reason_not_known() {
        assert_eq!(
            mergeable("MERGEABLE", "NEW"),
            Mergeable::No(NotMergeable::Unknown)
        );
    }
}
