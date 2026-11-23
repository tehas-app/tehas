//! The one module of the adapter that writes a label: the label of each of
//! tehas's words, its colour and text, and the rule a label name keeps. The
//! rest of the adapter names a word, never its label. The table is
//! `host/labels.tsv`, read when the adapter is built, and a test of the
//! adapter holds the words here to that file.

use tehas_ports::{Act, Choice, Colour, Mark, MarkName, MarkSpec, State};

/// The characters a GitHub label name is built from in tehas. The `:` of
/// tehas's labels is one of them; a space, a `/` and a `?` are not.
const LABEL_BYTES: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789:-_";

/// The longest label name GitHub accepts.
const LABEL_MAX: usize = 50;

/// Whether `label` is a name the writes will accept: non-empty, no longer than
/// GitHub allows, and built from the characters in [`LABEL_BYTES`]. A write
/// refuses a name outside it, because such a name is no label of tehas and the
/// request that carries it is aimed at something else - a newline, or a `-f`,
/// would otherwise reach gh as part of the one argument it is given. A read of
/// a term takes any string, so a repository's own label can still be asked
/// about. The path segment of a removal is encoded as well, so a name that did
/// reach a write could not leave the labels of its issue.
pub fn is_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= LABEL_MAX
        && label.as_bytes().iter().all(|b| LABEL_BYTES.contains(b))
}

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

/// `host/labels.tsv` as the adapter was built with it. Nothing reads the
/// installed copy at run time.
const FILE: &str = include_str!("../../../host/labels.tsv");

/// The name, colour and text of each label of the table, in its order, as
/// `prepare` makes them. A line the file does not give whole is left out;
/// the table's test holds the file to none.
pub fn specs() -> Vec<MarkSpec> {
    FILE.lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let (name, colour, about) = (fields.next()?, fields.next()?, fields.next()?);
            Some(MarkSpec {
                name: MarkName::new(name).ok()?,
                colour: Some(Colour::new(colour).ok()?),
                about: Some(about.to_string()).filter(|a| !a.is_empty()),
            })
        })
        .collect()
}
