//! The one module of the adapter that writes a label or a marker: the label
//! of each of tehas's words, its colour and text, the rule a label name
//! keeps, and each marker's text. The rest of the adapter names a word or a
//! marker, never its text. The labels are `host/labels.tsv`, read when the
//! adapter is built, and a test of the adapter holds the words here to that
//! file. The markers are `host/markers.tsv`, compiled in the same way:
//! `markers.rs` reads a text by these rows and writes none of their words.

use tehas_ports::{
    Act, Choice, Colour, FinishResult, Mark, MarkName, MarkSpec, Marker, MarkerKind,
    MillwrightAction, QuestionKind, Ref, State, TicketKey,
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

/// What stands between a marker's `<!--` and its kind's word.
pub(crate) const OURS: &str = "tehas:";

/// A marker's wrapper as the table writes it, up to the kind's word.
pub(crate) const WRAP: &str = "<!-- tehas:";

/// The 1.x word of a millwright's marker: it reads as an unreadable one.
pub(crate) const OLD_MILLWRIGHT: &str = "fixer";

/// What a field of a marker's row takes, as the table's header names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rule {
    /// The number of the item the marker is read from.
    Blueprint,
    /// A decimal number of at most this many digits.
    Number(usize),
    /// `min` to `max` numbers of at most `digits` digits, commas between.
    Numbers {
        min: usize,
        max: usize,
        digits: usize,
    },
    /// One of these words.
    Words(Vec<String>),
    /// At most this many of `a-z`, `0-9` and `-`, maybe none.
    Word(usize),
    /// Exactly this many lower-case hex digits.
    Hex(usize),
    /// 1 to this many characters of a ticket's key.
    Key(usize),
    /// A block's enclosed text.
    Text,
}

/// One field of a marker's row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    /// It may be left out, with the space before it.
    pub optional: bool,
    pub rule: Rule,
}

/// One row of `host/markers.tsv`: a kind, its word after [`OURS`], the
/// close line of a block, and its fields in the order they are written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkerRow {
    pub kind: MarkerKind,
    pub word: String,
    pub close: Option<String>,
    pub fields: Vec<Field>,
}

/// Every row of the markers' table, in its order. A line the file does not
/// give whole is left out; the table's test holds the file to none.
pub fn marker_rows() -> Vec<MarkerRow> {
    MARKERS
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(marker_row)
        .collect()
}

fn marker_row(line: &str) -> Option<MarkerRow> {
    let mut cols = line.split('\t');
    let (kind, open, close, fields) = (cols.next()?, cols.next()?, cols.next()?, cols.next()?);
    let kind = match kind {
        "Question" => MarkerKind::Question,
        "Blocked" => MarkerKind::Blocked,
        "HoldCleared" => MarkerKind::HoldCleared,
        "Park" => MarkerKind::Park,
        "Finish" => MarkerKind::Finish,
        "LastAttempt" => MarkerKind::LastAttempt,
        "Millwright" => MarkerKind::Millwright,
        "TicketOf" => MarkerKind::TicketOf,
        "Link" => MarkerKind::Link,
        _ => return None,
    };
    let word = open.strip_prefix(WRAP)?.split(' ').next()?.to_string();
    let fields = fields
        .split(';')
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .map(field)
        .collect::<Option<Vec<Field>>>()?;
    Some(MarkerRow {
        kind,
        word,
        close: Some(close.to_string()).filter(|c| !c.is_empty()),
        fields,
    })
}

fn field(text: &str) -> Option<Field> {
    let (name, rule) = text.split_once(' ')?;
    let (name, optional) = match name.strip_suffix('?') {
        Some(n) => (n, true),
        None => (name, false),
    };
    let (word, arg) = match rule.split_once('(') {
        Some((w, a)) => (w, a.strip_suffix(')')?),
        None => (rule, ""),
    };
    let rule = match word {
        "blueprint" => Rule::Blueprint,
        "number" => Rule::Number(arg.parse().ok()?),
        "numbers" => {
            let (range, digits) = arg.split_once(',')?;
            let (min, max) = range.split_once('-')?;
            Rule::Numbers {
                min: min.parse().ok()?,
                max: max.parse().ok()?,
                digits: digits.parse().ok()?,
            }
        }
        "words" => Rule::Words(arg.split('|').map(str::to_string).collect()),
        "word" => Rule::Word(arg.parse().ok()?),
        "hex" => Rule::Hex(arg.parse().ok()?),
        "key" => Rule::Key(arg.parse().ok()?),
        "text" => Rule::Text,
        _ => return None,
    };
    Some(Field {
        name: name.to_string(),
        optional,
        rule,
    })
}
