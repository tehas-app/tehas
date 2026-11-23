//! The one module of the adapter that writes a label: the label of each of
//! tehas's words, its colour and text, and the rule a label name keeps. The
//! rest of the adapter names a word, never its label. The table is
//! `host/labels.tsv`, read when the adapter is built, and a test of the
//! adapter holds the words here to that file. The texts of tehas's markers
//! are here too, as `host/markers.tsv` gives them: `markers.rs` reads a
//! text by these rows and writes none of their words.

use tehas_ports::{Act, Choice, Colour, Mark, MarkName, MarkSpec, MarkerKind, State};

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

/// What stands between a marker's `<!--` and its kind's word.
pub(crate) const OURS: &str = "tehas:";

/// A marker's wrapper as the table writes it, up to the kind's word.
pub(crate) const WRAP: &str = "<!-- tehas:";

/// The 1.x word of a millwright's marker: it reads as an unreadable one.
pub(crate) const OLD_MILLWRIGHT: &str = "fixer";

/// `host/markers.tsv` as the adapter was built with it.
const MARKERS: &str = include_str!("../../../host/markers.tsv");

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
