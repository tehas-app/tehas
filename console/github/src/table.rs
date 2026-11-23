//! The one module of the adapter that writes a label or a marker: the label
//! of each of tehas's words, its colour and text, the rule a label name
//! keeps, and each marker's text. The rest of the adapter names a word or a
//! marker, never its text. The labels are `host/labels.tsv`, read when the
//! adapter is built, and a test of the adapter holds the words here to that
//! file. The markers are `host/markers.tsv`, compiled in the same way.

use tehas_ports::{
    Act, Choice, Colour, FinishResult, Mark, MarkName, MarkSpec, Marker, MillwrightAction,
    QuestionKind, Ref, State, TicketKey,
};

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

/// `Site::mark_name`: a mark as GitHub spells it, its label, for terminal,
/// log and MCP text. `""` for a mark no label is, which is never written.
pub fn mark_name(mark: &Mark) -> String {
    label(mark).unwrap_or_default()
}

/// `Cite::Mark`: a mark's label as GitHub shows code, so a reader knows it
/// for a label. `""` for a mark no label is.
pub fn cite_mark(mark: &Mark) -> String {
    code(&mark_name(mark))
}

/// A text as GitHub shows code: inline for one line, a fenced block for
/// more. A fence is longer than any run of backticks inside it. `""` stays
/// `""`.
fn code(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    if text.contains('\n') {
        let fence = "`".repeat((longest + 1).max(3));
        format!("{fence}\n{text}\n{fence}")
    } else {
        let fence = "`".repeat(longest + 1);
        format!("{fence}{text}{fence}")
    }
}

/// The vendored table of markers: one row per kind but `Project`, a tab
/// between the kind, the open text, the close text and the fields.
const MARKERS: &str = include_str!("../../../host/markers.tsv");

/// A marker's row of the table, its fields by name with their values, and
/// the text a block encloses.
type Filled<'a> = (
    &'static str,
    Vec<(&'static str, Option<String>)>,
    Option<&'a str>,
);

/// A marker's text, from its row of the table: `{name}` is that field's
/// value, and an optional field with no value is left out with the space
/// before it. A block is its open line, its text and its close line. None
/// for a marker that is not writable, and for a project's own marker, whose
/// text is the consumer's.
pub fn marker_text(m: &Marker) -> Option<String> {
    if !m.writable() {
        return None;
    }
    let yes = |b: bool| if b { "1" } else { "0" }.to_string();
    let (kind, fields, note): Filled<'_> = match m {
        Marker::Question {
            kind,
            blueprint,
            turn,
        } => {
            let kind = match kind {
                QuestionKind::Plan => "plan",
                QuestionKind::Park => "park",
                _ => return None,
            };
            (
                "Question",
                vec![
                    ("kind", Some(kind.into())),
                    ("issue", Some(blueprint.get().to_string())),
                    ("turn", Some(turn.get().to_string())),
                ],
                None,
            )
        }
        Marker::Blocked { by } => {
            let numbers: Option<Vec<String>> = by
                .iter()
                .map(|t| match t {
                    Ref::Here(TicketKey::Number(n)) => Some(n.get().to_string()),
                    _ => None,
                })
                .collect();
            ("Blocked", vec![("by", Some(numbers?.join(",")))], None)
        }
        Marker::HoldCleared => ("HoldCleared", vec![], None),
        Marker::Park(p) => (
            "Park",
            vec![
                ("issue", Some(p.blueprint.get().to_string())),
                ("step", Some(p.step.as_str().into())),
                ("kind", Some(p.kind.as_str().into())),
                ("provider", Some(yes(p.provider))),
                ("sig", Some(p.sig.as_str().into())),
            ],
            None,
        ),
        Marker::Finish(f) => {
            let result = match f.result {
                FinishResult::Opened => "opened",
                FinishResult::Refused => "refused",
                FinishResult::Stopped => "stopped",
            };
            (
                "Finish",
                vec![
                    ("issue", Some(f.blueprint.get().to_string())),
                    ("result", Some(result.into())),
                    ("by", f.by_poll.then(|| "poll".into())),
                ],
                None,
            )
        }
        Marker::LastAttempt(l) => (
            "LastAttempt",
            vec![("base", Some(l.base.as_str().into()))],
            Some(l.note.as_str()),
        ),
        Marker::Millwright(w) => {
            let action = match w.action {
                MillwrightAction::Retry => "retry",
                MillwrightAction::Change => "change",
                MillwrightAction::Answer => "answer",
                MillwrightAction::Finish => "finish",
                MillwrightAction::Person => "person",
                MillwrightAction::Ask => "ask",
            };
            (
                "Millwright",
                vec![
                    ("issue", Some(w.blueprint.get().to_string())),
                    ("action", Some(action.into())),
                ],
                None,
            )
        }
        Marker::TicketOf(Ref::Here(key)) => {
            let key = match key {
                TicketKey::Number(n) => n.get().to_string(),
                TicketKey::Key(k) => k.as_str().to_string(),
            };
            ("TicketOf", vec![("ticket", Some(key))], None)
        }
        Marker::Link { blueprint, remove } => (
            "Link",
            vec![
                ("blueprint", Some(blueprint.get().to_string())),
                ("remove", remove.then(|| "1".into())),
            ],
            None,
        ),
        _ => return None,
    };
    let row = MARKERS
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| l.split('\t').collect::<Vec<_>>())
        .find(|cols| cols.first() == Some(&kind) && cols.len() >= 3)?;
    let mut text = row[1].to_string();
    for (name, value) in fields {
        let slot = format!("{{{name}}}");
        text = match value {
            Some(v) => text.replace(&slot, &v),
            None => text.replace(&format!(" {name}={slot}"), ""),
        };
    }
    Some(match note {
        Some(note) => format!("{text}\n{note}\n{}", row[2]),
        None => text,
    })
}

/// `Cite::Marker`: the marker's text as GitHub shows code, so it is no live
/// marker. `""` for a marker that is not writable.
pub fn cite_marker(m: &Marker) -> String {
    marker_text(m).map(|text| code(&text)).unwrap_or_default()
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
