//! The events of tehas: one vocabulary of facts in the ports' words, the
//! envelope a plugin, a page and another program read, and a small
//! in-process dispatcher. Every event is pull first: a fact the factory API
//! prints, raised in the process that records it, after the record commits.
//!
//! Names are `<subject>.<what-happened>`, lower case, past tense. Each name
//! is written once, in `Name::as_str` and the serde names below. Elsewhere
//! code names `Name::CycleEnded`, never the string.

use serde::{Deserialize, Serialize};

use crate::At;

mod dispatch;
mod gate;
mod line;
mod payload;
pub mod rfc3339;
mod samples;
mod words;

pub use dispatch::{Builder, Dispatcher, Fault, Heard, Raised, Refused, LOG};
pub use gate::{Ask, GateAnswer, GatePayload, StartAsk, Verdict, Why};
pub use payload::{
    ChangeMerged, ChangeOffered, CycleEnded, CycleParked, CycleStarted, LimitChanged,
    QuestionAsked, TicketActed, TicketClosed,
};
pub use samples::{gate_samples, samples, GateSample, Sample};
pub use words::{
    CycleId, End, How, Key, LimitState, QuestionChoice, QuestionNo, RunnerWord, Source, Stop,
};

/// The events' contract version, the envelope's `api`.
pub const API: u32 = 1;

/// Declares the names once: `Name` with its strings, `Event` with one
/// variant per name, and the reader of `Event`.
macro_rules! names {
    ($($(#[$doc:meta])* $name:ident($payload:ident) = $text:literal,)+) => {
        /// The name of an event. May grow: a name this copy does not know
        /// reads as `Unknown`.
        #[non_exhaustive]
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
        pub enum Name {
            $($(#[$doc])* #[serde(rename = $text)] $name,)+
            /// A name this copy does not know.
            #[serde(other)]
            Unknown,
        }

        impl Name {
            /// Every name, in the order of the spec's first list.
            pub const ALL: [Name; 9] = [$(Name::$name),+];

            /// `cycle.started` and so on; `unknown` for a name this copy
            /// does not know.
            pub fn as_str(self) -> &'static str {
                match self {
                    $(Name::$name => $text,)+
                    Name::Unknown => "unknown",
                }
            }
        }

        /// One variant per name, the payload as `data`.
        #[non_exhaustive]
        #[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
        #[serde(tag = "event", content = "data")]
        pub enum Event {
            $($(#[$doc])* #[serde(rename = $text)] $name($payload),)+
            /// A name this copy does not know, or a name of its own whose
            /// `data` does not read, so an older reader never fails an
            /// envelope, and any event of an envelope whose `api` is not
            /// this copy's. Read, never raised; writing it is an error.
            #[serde(
                untagged,
                deserialize_with = "read_unknown",
                serialize_with = "write_unknown"
            )]
            Unknown,
        }

        impl Event {
            /// The event's name.
            pub fn name(&self) -> Name {
                match self {
                    $(Event::$name(_) => Name::$name,)+
                    Event::Unknown => Name::Unknown,
                }
            }
        }
    };
}

names! {
    /// A cycle's start is on record.
    CycleStarted(CycleStarted) = "cycle.started",
    /// A cycle's history line says it parked.
    CycleParked(CycleParked) = "cycle.parked",
    /// A question is recorded.
    QuestionAsked(QuestionAsked) = "question.asked",
    /// A cycle's history line is written.
    CycleEnded(CycleEnded) = "cycle.ended",
    /// A cycle left a change for review.
    ChangeOffered(ChangeOffered) = "change.offered",
    /// A write changed the factory's limit.
    FactoryLimitChanged(LimitChanged) = "factory.limit-changed",
    /// An allowed account's act is recorded.
    TicketActed(TicketActed) = "ticket.acted",
    /// A ticket the factory follows is recorded closed.
    TicketClosed(TicketClosed) = "ticket.closed",
    /// A change linked to a blueprint is recorded merged.
    ChangeMerged(ChangeMerged) = "change.merged",
}

// The ports' one read rule for an enum that may grow, on the tag `event`.
fn read_unknown<'de, D: serde::Deserializer<'de>>(d: D) -> Result<(), D::Error> {
    crate::unknown::read_any_at(d, "event")
}

// Unknown is never written: an envelope of it says nothing a reader can use.
fn write_unknown<S: serde::Serializer>(_: S) -> Result<S::Ok, S::Error> {
    Err(serde::ser::Error::custom(
        "an unknown event is read, never written",
    ))
}

impl Name {
    /// Whether the point has a `before` side that is raised. None is in
    /// 2.0: the first gate, the `before` side of `cycle.started`, comes in
    /// 2.1 with its raise point.
    pub fn gate(self) -> bool {
        false
    }
}

/// The side of a point: `after` a fact, or `before` an act that waits.
/// Exhaustive.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum When {
    /// A notify event: the fact happened, nothing waits.
    After,
    /// A gate: the act waits for an answer.
    Before,
}

/// A typed payload. The key is the payload's to make, so tehas and a
/// consumer that builds the event from a witness make the same key.
pub trait Payload: Serialize + Clone + Into<Event> + 'static {
    /// The payload's name.
    const NAME: Name;
    /// The event's identity within its name.
    fn key(&self) -> Key;
    /// When the fact happened. Never when it was raised.
    fn at(&self) -> At;
    /// The payload of an event of this name.
    fn peek(e: &Event) -> Option<&Self>;
}

/// An event as JSON carries it: `api`, `source`, `event`, `when`, `key`,
/// `at` and the payload as `data`. An envelope of another `api` reads with
/// its event as `Unknown`: this copy knows the payloads of its own alone.
/// serde writes the fields as `api`, `source`, `event`, `data`, `when`,
/// `key`, `at`, as `event` is flattened: compare envelopes as values.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(from = "EnvelopeWire")]
pub struct Envelope {
    /// The events' contract version, `API`.
    pub api: u32,
    /// The factory the event is from, set by a consumer that relays many
    /// factories' events, left out by tehas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    /// The name and its payload.
    #[serde(flatten)]
    pub event: Event,
    /// The side of the point.
    pub when: When,
    /// The event's identity within its name.
    pub key: Key,
    /// When the fact happened.
    #[serde(with = "rfc3339")]
    pub at: At,
}

#[derive(Deserialize)]
struct EnvelopeWire {
    api: u32,
    #[serde(default)]
    source: Option<Source>,
    #[serde(flatten)]
    event: Event,
    when: When,
    key: Key,
    #[serde(with = "rfc3339")]
    at: At,
}

impl From<EnvelopeWire> for Envelope {
    fn from(w: EnvelopeWire) -> Self {
        Envelope {
            event: if w.api == API {
                w.event
            } else {
                Event::Unknown
            },
            api: w.api,
            source: w.source,
            when: w.when,
            key: w.key,
            at: w.at,
        }
    }
}

impl Envelope {
    /// The `after` envelope of a payload: its key and time, no source.
    pub fn new<P: Payload>(p: P) -> Self {
        Envelope {
            api: API,
            source: None,
            key: p.key(),
            at: p.at(),
            when: When::After,
            event: p.into(),
        }
    }

    /// The same envelope, relayed from the factory `source`. Nothing else
    /// changes.
    pub fn from(self, source: Source) -> Self {
        Envelope {
            source: Some(source),
            ..self
        }
    }

    /// The event's name.
    pub fn name(&self) -> Name {
        self.event.name()
    }

    /// The log's line of the envelope: `event` and the envelope as compact
    /// JSON, or `event`, its name and key when it does not write, as an
    /// `Unknown` does not.
    pub fn line(&self) -> String {
        match line::json(self) {
            Ok(json) => format!("event {json}"),
            Err(_) => format!("event {} {}", self.name().as_str(), self.key.as_str()),
        }
    }
}
