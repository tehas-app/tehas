//! `MarkerKind`, `MarkerKey`, `Marker` with a typed payload per kind, and
//! `Marker::Unreadable`. How a marker is written in text is the vendored
//! table, read by the adapter: no marker text is written here.

use std::num::NonZeroU32;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::ids::checked;
use crate::mark::unknown_tag;
use crate::{unknown, BlueprintNo, Did, MarkName, Outcome, Ref, Refusal, TicketKey};

/// The most tickets one `Blocked` marker names.
pub const BLOCKED_MAX: usize = 20;

/// The largest number the vendored table writes: `number(9)` and
/// `blueprint` are at most 9 digits.
const NUMBER_MAX: u64 = 999_999_999;

/// The version of the `Spec` block this copy reads.
pub const SPEC_VERSION: u32 = 1;

/// The kinds of tehas's markers: machine-read notes inside a comment or a
/// body. One kind per marker tehas writes or reads today, and the two that
/// 2.0 adds.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
#[non_exhaustive]
pub enum MarkerKind {
    /// A question of the factory's.
    Question,
    /// The tickets a ticket waits on: tehas's hold.
    Blocked,
    /// The factory's word that a hold was cleared.
    HoldCleared,
    /// A stopped cycle's park.
    Park,
    /// What a finish of a stopped cycle did.
    Finish,
    /// A block: an open line, the text it encloses, a close line.
    Spec,
    /// A block, as `Spec` is.
    LastAttempt,
    /// The millwright's marker on its comment. Once named `Fixer`.
    Millwright,
    /// The line back from a blueprint to its ticket, for people. New in 2.0.
    TicketOf,
    /// An allowed account's ask to link a ticket and a blueprint. New in 2.0.
    Link,
    /// A consumer's own marker kind, carried through by name.
    Project(MarkName),
    /// A kind this copy does not know. It holds what it sits on.
    #[serde(
        untagged,
        deserialize_with = "marker_kind_unknown",
        serialize_with = "unknown_tag"
    )]
    Unknown,
}

fn marker_kind_unknown<'de, D: Deserializer<'de>>(d: D) -> Result<(), D::Error> {
    unknown::read_any(d)
}

checked!(
    /// What a marker is found by: letters, digits, `-`, `=`, `.`, at most
    /// 128, checked when made.
    MarkerKey,
    "marker key",
    marker_key_ok
);

fn marker_key_ok(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'=' | b'.'))
}

checked!(
    /// A word of a marker: at most 40 of `a-z`, `0-9` and `-`, maybe empty,
    /// checked when made.
    MarkerWord,
    "marker word",
    marker_word_ok
);

fn marker_word_ok(s: &str) -> bool {
    s.len() <= 40
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

checked!(
    /// A sha-256 as 64 lower-case hex digits, checked when made.
    Sha256,
    "sha-256",
    sha256_ok
);

fn sha256_ok(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// One marker, as the adapter reads it from a comment or a body, with a
/// typed payload per kind. The comment's author and last editor are the
/// comment's own fields. A text's markers are in the order they stand in
/// it, the first first. Read through serde, it is [`Marker::checked`], and
/// a known tag whose content does not read, such as a word of a newer
/// `MillwrightAction`, is `Unreadable` of that tag's kind, as the text's
/// reading has it. The tags `Project`, `Unreadable` and `Bare` name no one
/// kind: theirs is `Unreadable` of `MarkerKind::Unknown`. A tag this copy
/// does not know is `Unknown`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
// `remote = "Self"` makes the derives inherent functions, so the trait impls
// below can check what is read.
#[serde(remote = "Self", tag = "type", content = "value")]
#[non_exhaustive]
pub enum Marker {
    /// A question of the factory's, on the blueprint it asks about.
    Question {
        /// What the question is for.
        kind: QuestionKind,
        /// The blueprint it asks about.
        blueprint: BlueprintNo,
        /// Which turn of questions it is.
        turn: NonZeroU32,
    },
    /// The tickets this one waits on. At most 20; more does not read. A
    /// write names ticket numbers of this realm and project alone.
    Blocked {
        /// The tickets, 1 to 20.
        by: Vec<Ref<TicketKey>>,
    },
    /// The factory's word that a hold was cleared.
    HoldCleared,
    /// A stopped cycle's park.
    Park(ParkMarker),
    /// What a finish of a stopped cycle did.
    Finish(FinishMarker),
    /// The spec a change was built to.
    Spec(SpecMarker),
    /// What a cycle stopped for time learned.
    LastAttempt(LastAttemptMarker),
    /// A millwright's decision.
    Millwright(MillwrightMarker),
    /// The ticket a blueprint is for, for people. A write names a ticket of
    /// this realm and project alone.
    TicketOf(Ref<TicketKey>),
    /// An allowed account's ask to link a blueprint to the ticket it sits
    /// on, or to take the link off.
    Link {
        /// The blueprint.
        blueprint: BlueprintNo,
        /// Take the link off instead.
        remove: bool,
    },
    /// A consumer's own marker, carried through by name, its fields as
    /// written. Only a project's own markers are untyped.
    Project {
        /// The kind's name.
        name: MarkName,
        /// What it is found by, if anything.
        key: Option<MarkerKey>,
        /// Its fields, names and values, as written.
        fields: Vec<(String, String)>,
    },
    /// Our wrapper is there, with this kind's word, and what it holds does
    /// not read: a field that does not parse, a number with leading zeros,
    /// a list over its limit, a field missing, repeated, out of order or not
    /// of the row, a value outside its set, a block with no close line.
    /// Kept, never dropped, and never written. It holds what it sits on, as
    /// an unknown marker does.
    Unreadable {
        /// The kind it was meant to be.
        kind: MarkerKind,
    },
    /// This kind's words and its first field without our wrapper, outside a
    /// quote and outside code. No marker to obey, and not absent: it holds
    /// as `Unreadable` does. Its words stay in the body, as prose: the
    /// adapter reports them here as well, and never writes one.
    Bare {
        /// The kind it names.
        kind: MarkerKind,
        /// Its payload when the rest of its line reads as the wrapped form's
        /// would: never one to hold or start on; a reader may use it to keep
        /// a fact it would otherwise drop.
        read: Option<Box<Marker>>,
    },
    /// A kind this copy does not know. It holds what it sits on.
    #[serde(
        untagged,
        deserialize_with = "marker_unknown",
        serialize_with = "unknown_tag"
    )]
    Unknown,
}

impl Serialize for Marker {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        Marker::serialize(self, s)
    }
}

impl<'de> Deserialize<'de> for Marker {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match MarkerRead::deserialize(d)? {
            MarkerRead::Read(m) => m.checked(),
            MarkerRead::Tag { r#type } => match tag_kind(&r#type) {
                Some(kind) => Marker::Unreadable { kind },
                None => Marker::Unknown,
            },
        })
    }
}

/// A marker as read: whole, or else its tag alone.
#[derive(Deserialize)]
#[serde(untagged)]
enum MarkerRead {
    Read(#[serde(deserialize_with = "Marker::deserialize")] Marker),
    Tag { r#type: String },
}

/// The kind a known tag names, `MarkerKind::Unknown` for a tag that names no
/// one kind; None for a tag this copy does not know.
fn tag_kind(tag: &str) -> Option<MarkerKind> {
    Some(match tag {
        "Question" => MarkerKind::Question,
        "Blocked" => MarkerKind::Blocked,
        "HoldCleared" => MarkerKind::HoldCleared,
        "Park" => MarkerKind::Park,
        "Finish" => MarkerKind::Finish,
        "Spec" => MarkerKind::Spec,
        "LastAttempt" => MarkerKind::LastAttempt,
        "Millwright" => MarkerKind::Millwright,
        "TicketOf" => MarkerKind::TicketOf,
        "Link" => MarkerKind::Link,
        _ if MARKER_TAGS.contains(&tag) => MarkerKind::Unknown,
        _ => return None,
    })
}

/// The tags of `Marker` but `Unknown`.
const MARKER_TAGS: &[&str] = &[
    "Question",
    "Blocked",
    "HoldCleared",
    "Park",
    "Finish",
    "Spec",
    "LastAttempt",
    "Millwright",
    "TicketOf",
    "Link",
    "Project",
    "Unreadable",
    "Bare",
];

fn marker_unknown<'de, D: Deserializer<'de>>(d: D) -> Result<(), D::Error> {
    unknown::read(d, MARKER_TAGS)
}

/// What a question of the factory's is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum QuestionKind {
    /// A question of a parked cycle.
    Park,
    /// A question of planning.
    Plan,
    /// A question this copy does not know.
    #[serde(other)]
    Unknown,
}

/// A stopped cycle's park, for the millwright to decide on.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ParkMarker {
    /// The blueprint that parked. A park of another blueprint than the item
    /// it is read from does not read.
    pub blueprint: BlueprintNo,
    /// The step it stopped at, empty when none was recorded.
    pub step: MarkerWord,
    /// The kind of failure, empty when none was recorded.
    pub kind: MarkerWord,
    /// The failure was the model provider's.
    pub provider: bool,
    /// The sha-256 of the park's fields as the store keeps them, its free
    /// text in the hash only: what tells one park from its repeat.
    pub sig: Sha256,
}

/// A millwright's decision on a blueprint.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MillwrightMarker {
    /// The blueprint it decides on. One of another blueprint than the item
    /// it is read from does not read.
    pub blueprint: BlueprintNo,
    /// What the millwright decides.
    pub action: MillwrightAction,
}

/// What the millwright decides. A word outside this set does not read: it
/// is `Unreadable { kind: Millwright }`, a millwright's marker and no
/// decision. Not `#[non_exhaustive]`: a new word is a new row and a new
/// variant at a tag, and an older copy reads it as `Unreadable`, which
/// holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MillwrightAction {
    /// Start it again, the same way.
    Retry,
    /// Start it again, working differently, as the comment says.
    Change,
    /// An answer to the factory's question, the comment's text.
    Answer,
    /// Open the draft change of a stopped cycle, for the reason the comment
    /// gives.
    Finish,
    /// Leave it to a person. The consumer's.
    Person,
    /// Ask a person a question. The consumer's.
    Ask,
}

/// What a finish of a stopped cycle did.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FinishMarker {
    /// The blueprint it finished. One of another blueprint than the item it
    /// is read from does not read.
    pub blueprint: BlueprintNo,
    /// What came of it.
    pub result: FinishResult,
    /// The poll finished it on its own, with no one's ask.
    pub by_poll: bool,
}

/// What came of a finish. A word outside this set does not read, as for
/// `MillwrightAction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FinishResult {
    /// The draft change is open; its link is in the comment's text.
    Opened,
    /// Nothing was finished; the reason is in the comment's text.
    Refused,
    /// The poll's own finish stopped; the reason is in the comment's text.
    Stopped,
}

/// The spec a change was built to, and one line per requirement. A block:
/// what it encloses is its payload, never the body's.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpecMarker {
    /// The block's version. This copy reads 1; another reads as
    /// `Unreadable { kind: Spec }`.
    pub version: NonZeroU32,
    /// The spec's link as written, one word. Whether it may be fetched is
    /// tehas's link rule, not the port's.
    pub link: String,
    /// The spec's sha-256.
    pub sha256: Sha256,
    /// At least one, each id once.
    pub requirements: Vec<Requirement>,
}

/// One requirement of a spec, and whether the change meets it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Requirement {
    /// The requirement's id: not empty, no backquote.
    pub id: String,
    /// The change meets it.
    pub met: bool,
    /// Why, never empty.
    pub note: String,
}

/// What a cycle stopped for time learned, for the next cycle on the same
/// text. A block: what it encloses is its payload, never the body's.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LastAttemptMarker {
    /// The sha-256 of the text it sits in, without this block. The same
    /// text with the same `base` is a repeat.
    pub base: Sha256,
    /// The enclosed text, kept out of the body a person or a caller reads.
    pub note: String,
}

impl Marker {
    /// The marker's kind. `Unreadable` and `Bare` answer the kind they name.
    pub fn kind(&self) -> MarkerKind {
        match self {
            Marker::Question { .. } => MarkerKind::Question,
            Marker::Blocked { .. } => MarkerKind::Blocked,
            Marker::HoldCleared => MarkerKind::HoldCleared,
            Marker::Park(_) => MarkerKind::Park,
            Marker::Finish(_) => MarkerKind::Finish,
            Marker::Spec(_) => MarkerKind::Spec,
            Marker::LastAttempt(_) => MarkerKind::LastAttempt,
            Marker::Millwright(_) => MarkerKind::Millwright,
            Marker::TicketOf(_) => MarkerKind::TicketOf,
            Marker::Link { .. } => MarkerKind::Link,
            Marker::Project { name, .. } => MarkerKind::Project(name.clone()),
            Marker::Unreadable { kind } | Marker::Bare { kind, .. } => kind.clone(),
            Marker::Unknown => MarkerKind::Unknown,
        }
    }

    /// What the marker is found by, from its payload: a question's
    /// blueprint and turn, a park's blueprint, a project marker's own key.
    /// None for a kind found by kind alone, and for `Unreadable` and `Bare`.
    pub fn key(&self) -> Option<MarkerKey> {
        let key = match self {
            Marker::Question {
                blueprint, turn, ..
            } => format!("blueprint={}.turn={turn}", blueprint.get()),
            Marker::Park(park) => format!("blueprint={}", park.blueprint.get()),
            Marker::Project { key, .. } => return key.clone(),
            _ => return None,
        };
        MarkerKey::new(key).ok()
    }

    /// The marker as a reader may act on it: itself when its payload keeps
    /// the port's limits, else `Unreadable` of its kind. A `Blocked` names 1
    /// to 20 tickets. A `Spec` is of version 1, its link one word, with at
    /// least one requirement, each id once, not empty and with no
    /// backquote, each note not empty.
    pub fn checked(self) -> Marker {
        if self.keeps_limits() {
            self
        } else {
            Marker::Unreadable { kind: self.kind() }
        }
    }

    /// True for a marker a write may set: one that keeps the port's limits,
    /// that the vendored table can write, and that is not `Unreadable`,
    /// `Bare` or `Unknown`. A `Question` of `QuestionKind::Unknown` is not
    /// written. The table writes a number of at most 9 digits,
    /// and a `TicketOf` or `Blocked` names only a ticket of this realm and
    /// project: a ticket `Elsewhere` is not written as a marker. A
    /// `Blocked` names ticket numbers alone.
    pub fn writable(&self) -> bool {
        !matches!(
            self,
            Marker::Unreadable { .. } | Marker::Bare { .. } | Marker::Unknown
        ) && self.keeps_limits()
            && self.table_writes()
    }

    fn table_writes(&self) -> bool {
        let fits = |n: u64| n <= NUMBER_MAX;
        match self {
            Marker::Question {
                kind,
                blueprint,
                turn,
            } => {
                *kind != QuestionKind::Unknown
                    && fits(blueprint.get())
                    && fits(u64::from(turn.get()))
            }
            Marker::Blocked { by } => by
                .iter()
                .all(|t| matches!(t, Ref::Here(TicketKey::Number(n)) if fits(n.get()))),
            Marker::Park(park) => fits(park.blueprint.get()),
            Marker::Finish(finish) => fits(finish.blueprint.get()),
            Marker::Millwright(m) => fits(m.blueprint.get()),
            Marker::TicketOf(ticket) => matches!(ticket, Ref::Here(_)),
            Marker::Link { blueprint, .. } => fits(blueprint.get()),
            _ => true,
        }
    }

    fn keeps_limits(&self) -> bool {
        match self {
            Marker::Blocked { by } => (1..=BLOCKED_MAX).contains(&by.len()),
            Marker::Spec(spec) => spec_reads(spec),
            _ => true,
        }
    }
}

fn spec_reads(spec: &SpecMarker) -> bool {
    let ids_ok = spec.requirements.iter().enumerate().all(|(i, r)| {
        !r.id.is_empty()
            && !r.id.contains('`')
            && !r.note.is_empty()
            && !spec.requirements[..i].iter().any(|o| o.id == r.id)
    });
    spec.version.get() == SPEC_VERSION
        && !spec.link.is_empty()
        && !spec.link.chars().any(char::is_whitespace)
        && !spec.requirements.is_empty()
        && ids_ok
}

/// How an edit changes the markers of what it edits. A marker in `set`
/// replaces the one of the same kind and key, or is added; one named in
/// `drop` is taken off; every other marker is kept where it stands, an
/// `Unreadable` one included, and a block whole, from its open line to its
/// close line. The default changes none. A `set` that holds a marker that
/// is not [`Marker::writable`] is `Refused(Invalid)` before any request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MarkerEdit {
    /// The markers to set.
    pub set: Vec<Marker>,
    /// The markers to take off, by kind and key.
    pub drop: Vec<(MarkerKind, Option<MarkerKey>)>,
}

impl MarkerEdit {
    /// `Refused(Invalid)` when a marker in `set` may not be written.
    pub fn check(&self) -> Did {
        if self.set.iter().all(Marker::writable) {
            Ok(())
        } else {
            Err(Outcome::Refused(Refusal::Invalid))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha(c: char) -> Sha256 {
        Sha256::new(c.to_string().repeat(64)).unwrap()
    }

    fn no(n: u64) -> BlueprintNo {
        BlueprintNo::new(n).unwrap()
    }

    fn ticket(n: u64) -> Ref<TicketKey> {
        Ref::Here(TicketKey::Number(std::num::NonZeroU64::new(n).unwrap()))
    }

    fn spec(requirements: Vec<Requirement>) -> Marker {
        Marker::Spec(SpecMarker {
            version: NonZeroU32::new(1).unwrap(),
            link: "https://example.test/spec.md".into(),
            sha256: sha('a'),
            requirements,
        })
    }

    fn req(id: &str, note: &str) -> Requirement {
        Requirement {
            id: id.into(),
            met: true,
            note: note.into(),
        }
    }

    #[test]
    fn a_marker_key_is_up_to_128_of_its_set() {
        assert!(MarkerKey::new("blueprint=7.turn=2").is_ok());
        assert!(MarkerKey::new("a".repeat(128)).is_ok());
        assert!(MarkerKey::new("a".repeat(129)).is_err());
        assert!(MarkerKey::new("").is_err());
        assert!(MarkerKey::new("a b").is_err());
        assert!(MarkerKey::new("a:b").is_err());
    }

    #[test]
    fn a_marker_word_is_up_to_40_lower_case_maybe_empty() {
        assert!(MarkerWord::new("").is_ok());
        assert!(MarkerWord::new("model-timeout-2").is_ok());
        assert!(MarkerWord::new("a".repeat(40)).is_ok());
        assert!(MarkerWord::new("a".repeat(41)).is_err());
        assert!(MarkerWord::new("Upper").is_err());
        assert!(MarkerWord::new("a_b").is_err());
    }

    #[test]
    fn a_sha256_is_64_lower_case_hex() {
        assert!(Sha256::new("0123456789abcdef".repeat(4)).is_ok());
        assert!(Sha256::new("A".repeat(64)).is_err());
        assert!(Sha256::new("a".repeat(63)).is_err());
        assert!(Sha256::new("a".repeat(65)).is_err());
        assert!(Sha256::new("g".repeat(64)).is_err());
    }

    #[test]
    fn blocked_names_one_to_twenty() {
        let twenty: Vec<_> = (1..=20).map(ticket).collect();
        let ok = Marker::Blocked { by: twenty.clone() };
        assert_eq!(ok.clone().checked(), ok);
        let mut over = twenty;
        over.push(ticket(21));
        assert_eq!(
            Marker::Blocked { by: over }.checked(),
            Marker::Unreadable {
                kind: MarkerKind::Blocked
            }
        );
        assert_eq!(
            Marker::Blocked { by: vec![] }.checked(),
            Marker::Unreadable {
                kind: MarkerKind::Blocked
            }
        );
    }

    #[test]
    fn a_spec_keeps_its_rules() {
        let unreadable = Marker::Unreadable {
            kind: MarkerKind::Spec,
        };
        let good = spec(vec![req("R1", "done"), req("R2", "also")]);
        assert_eq!(good.clone().checked(), good);
        assert_eq!(spec(vec![]).checked(), unreadable);
        assert_eq!(
            spec(vec![req("R1", "a"), req("R1", "b")]).checked(),
            unreadable
        );
        assert_eq!(spec(vec![req("", "a")]).checked(), unreadable);
        assert_eq!(spec(vec![req("R`1", "a")]).checked(), unreadable);
        assert_eq!(spec(vec![req("R1", "")]).checked(), unreadable);
        let Marker::Spec(mut v2) = good.clone() else {
            unreachable!()
        };
        v2.version = NonZeroU32::new(2).unwrap();
        assert_eq!(Marker::Spec(v2).checked(), unreadable);
        let Marker::Spec(mut spaced) = good else {
            unreachable!()
        };
        spaced.link = "two words".into();
        assert_eq!(Marker::Spec(spaced).checked(), unreadable);
    }

    #[test]
    fn kind_and_key_come_from_the_payload() {
        let q = Marker::Question {
            kind: QuestionKind::Park,
            blueprint: no(7),
            turn: NonZeroU32::new(2).unwrap(),
        };
        assert_eq!(q.kind(), MarkerKind::Question);
        assert_eq!(q.key().unwrap().as_str(), "blueprint=7.turn=2");
        let park = Marker::Park(ParkMarker {
            blueprint: no(9),
            step: MarkerWord::new("").unwrap(),
            kind: MarkerWord::new("timeout").unwrap(),
            provider: false,
            sig: sha('b'),
        });
        assert_eq!(park.key().unwrap().as_str(), "blueprint=9");
        assert_eq!(Marker::HoldCleared.key(), None);
        let bare = Marker::Bare {
            kind: MarkerKind::Blocked,
            read: Some(Box::new(Marker::Blocked {
                by: vec![ticket(1)],
            })),
        };
        assert_eq!(bare.kind(), MarkerKind::Blocked);
        assert_eq!(bare.key(), None);
        let unreadable = Marker::Unreadable {
            kind: MarkerKind::Park,
        };
        assert_eq!(unreadable.kind(), MarkerKind::Park);
        assert_eq!(unreadable.key(), None);
        let name = MarkName::new("hold").unwrap();
        let project = Marker::Project {
            name: name.clone(),
            key: Some(MarkerKey::new("k1").unwrap()),
            fields: vec![("by".into(), "someone".into())],
        };
        assert_eq!(project.kind(), MarkerKind::Project(name));
        assert_eq!(project.key().unwrap().as_str(), "k1");
        assert_eq!(Marker::Unknown.kind(), MarkerKind::Unknown);
    }

    fn elsewhere() -> Ref<TicketKey> {
        Ref::Elsewhere {
            project: crate::ProjectName::new("other"),
            item: crate::ItemName::new("other 1"),
        }
    }

    fn check_one(m: Marker) -> Did {
        MarkerEdit {
            set: vec![m],
            drop: vec![],
        }
        .check()
    }

    const REFUSED: Did = Err(Outcome::Refused(Refusal::Invalid));

    #[test]
    fn a_ticket_elsewhere_is_never_written_as_a_marker() {
        let key = Ref::Here(TicketKey::Key(crate::KeyText::new("ABC-1").unwrap()));
        for ok in [
            Marker::TicketOf(ticket(1)),
            Marker::TicketOf(key.clone()),
            Marker::Blocked {
                by: vec![ticket(1), ticket(2)],
            },
        ] {
            assert!(ok.writable(), "{ok:?}");
            assert_eq!(check_one(ok), Ok(()));
        }
        for bad in [
            Marker::TicketOf(elsewhere()),
            Marker::Blocked {
                by: vec![ticket(1), elsewhere()],
            },
            Marker::Blocked { by: vec![key] },
        ] {
            assert!(!bad.writable(), "{bad:?}");
            assert_eq!(check_one(bad), REFUSED);
        }
        // Read, it is still what it says: the table limits a write.
        let read = Marker::Blocked {
            by: vec![elsewhere()],
        };
        assert_eq!(read.clone().checked(), read);
    }

    #[test]
    fn a_write_holds_numbers_of_at_most_nine_digits() {
        const MAX: u64 = 999_999_999;
        let all = |n: u64| {
            let turn = NonZeroU32::new(u32::try_from(n).unwrap()).unwrap();
            vec![
                Marker::Question {
                    kind: QuestionKind::Plan,
                    blueprint: no(n),
                    turn: NonZeroU32::new(1).unwrap(),
                },
                Marker::Question {
                    kind: QuestionKind::Plan,
                    blueprint: no(1),
                    turn,
                },
                Marker::Blocked {
                    by: vec![ticket(1), ticket(n)],
                },
                Marker::Park(ParkMarker {
                    blueprint: no(n),
                    step: MarkerWord::new("").unwrap(),
                    kind: MarkerWord::new("").unwrap(),
                    provider: false,
                    sig: sha('a'),
                }),
                Marker::Finish(FinishMarker {
                    blueprint: no(n),
                    result: FinishResult::Opened,
                    by_poll: false,
                }),
                Marker::Millwright(MillwrightMarker {
                    blueprint: no(n),
                    action: MillwrightAction::Retry,
                }),
                Marker::Link {
                    blueprint: no(n),
                    remove: false,
                },
            ]
        };
        for m in all(MAX) {
            assert!(m.writable(), "{m:?}");
            assert_eq!(check_one(m), Ok(()));
        }
        for m in all(MAX + 1) {
            assert!(!m.writable(), "{m:?}");
            assert_eq!(check_one(m.clone()), REFUSED);
            assert_eq!(m.clone().checked(), m);
        }
        let big = Marker::TicketOf(ticket(MAX + 1));
        assert!(big.writable());
    }

    #[test]
    fn an_edit_never_sets_what_does_not_read() {
        assert_eq!(MarkerEdit::default().check(), Ok(()));
        let good = MarkerEdit {
            set: vec![Marker::HoldCleared],
            drop: vec![(MarkerKind::Blocked, None)],
        };
        assert_eq!(good.check(), Ok(()));
        for bad in [
            Marker::Unreadable {
                kind: MarkerKind::Blocked,
            },
            Marker::Bare {
                kind: MarkerKind::Blocked,
                read: None,
            },
            Marker::Unknown,
            Marker::Blocked { by: vec![] },
        ] {
            let edit = MarkerEdit {
                set: vec![Marker::HoldCleared, bad],
                drop: vec![],
            };
            assert_eq!(edit.check(), Err(Outcome::Refused(Refusal::Invalid)));
        }
    }

    #[test]
    fn a_question_of_an_unknown_kind_is_never_written() {
        let q = |kind| Marker::Question {
            kind,
            blueprint: no(7),
            turn: NonZeroU32::new(1).unwrap(),
        };
        assert!(q(QuestionKind::Plan).writable());
        assert!(q(QuestionKind::Park).writable());
        let unknown = q(QuestionKind::Unknown);
        assert!(!unknown.writable());
        assert_eq!(check_one(unknown.clone()), REFUSED);
        assert_eq!(unknown.clone().checked(), unknown);
    }
}
