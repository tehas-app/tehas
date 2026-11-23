//! The one module of the adapter that writes a label: the label of each of
//! tehas's words, and the rule a label name keeps. The rest of the adapter
//! names a word, never its label. The table is `host/labels.tsv`, and a test
//! of the adapter holds this copy of it to that file.
//!
//! The rule, [`is_label`], stays in core while `console/src/labels.rs` reads
//! it there, and comes here when that caller moves behind the ports.

pub use tehas_core::forge::is_label;
use tehas_ports::{Act, Choice, Mark, State};

/// The label whose latest application approves work.
pub const APPROVAL_LABEL: &str = "tehas:ready";

/// Each of tehas's words and its label, in the order of `host/labels.tsv`.
const WORDS: [(&str, Mark); 20] = [
    (APPROVAL_LABEL, Mark::Act(Act::Approve)),
    ("tehas:running", Mark::State(State::Running)),
    ("tehas:awaiting-merge", Mark::State(State::AwaitingMerge)),
    ("tehas:shipped", Mark::State(State::Shipped)),
    ("tehas:pause", Mark::Act(Act::Pause)),
    ("needs-human-attention", Mark::State(State::NeedsPerson)),
    ("tehas:question", Mark::State(State::Question)),
    ("tehas:plan", Mark::Act(Act::Plan)),
    ("tehas:refine", Mark::Act(Act::Refine)),
    ("tehas:planning", Mark::State(State::Planning)),
    ("tehas:replaced", Mark::State(State::Replaced)),
    ("tehas:refined", Mark::State(State::Refined)),
    ("tehas:millwright-escalated", Mark::State(State::Escalated)),
    ("tehas:lean", Mark::Choice(Choice::Lean)),
    ("tehas:classic", Mark::Choice(Choice::Classic)),
    ("tehas:blocked", Mark::Act(Act::Block)),
    ("tehas:refreshing", Mark::State(State::Refreshing)),
    ("epic", Mark::Epic),
    ("tehas:finish", Mark::Act(Act::Finish)),
    ("tehas:improvement", Mark::State(State::Proposal)),
];

/// Every word of the table with its label, in the table's order.
pub fn words() -> Vec<(&'static str, Mark)> {
    WORDS.to_vec()
}

/// The word a label is, None for a label that is none of tehas's words.
pub fn word(label: &str) -> Option<Mark> {
    WORDS
        .iter()
        .find(|(l, _)| *l == label)
        .map(|(_, m)| m.clone())
}

/// The label of a mark. None for `Unknown`, which is never written.
pub fn label(mark: &Mark) -> Option<String> {
    match mark {
        Mark::Project(name) => Some(name.as_str().to_string()),
        _ => WORDS
            .iter()
            .find(|(_, m)| m == mark)
            .map(|(l, _)| l.to_string()),
    }
}
