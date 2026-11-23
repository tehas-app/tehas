//! The payloads, one per name: each makes its own key and time.

use serde::{Deserialize, Serialize};

use super::words::key_of;
use super::{rfc3339, CycleId, End, Event, How, Key, LimitState, Name, Payload, QuestionChoice};
use super::{QuestionNo, RunnerWord, Stop};
use crate::{Account, Act, At, BlueprintNo, ChangeNo, CloseReason, Login, QuestionKind, TicketKey};

/// A cycle's start is on record. Witness: `running[]` of `status --json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CycleStarted {
    /// The cycle.
    pub cycle: CycleId,
    /// The blueprint, the factory API's `issue`.
    pub blueprint: BlueprintNo,
    /// The ticket, left out when it is the blueprint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket: Option<TicketKey>,
    /// `running[].since`.
    #[serde(with = "rfc3339")]
    pub started: At,
    /// `running[].how`.
    pub how: How,
    /// Who started it, left out for a cycle run by hand.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<Login>,
}

/// A cycle's history line is written. Witness: a work line of
/// `history --json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CycleEnded {
    /// The cycle.
    pub cycle: CycleId,
    /// The blueprint.
    pub blueprint: BlueprintNo,
    /// The ticket, left out when it is the blueprint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket: Option<TicketKey>,
    /// When it started.
    #[serde(with = "rfc3339")]
    pub started: At,
    /// When it ended: for a lost line, when it was last heard from.
    #[serde(with = "rfc3339")]
    pub ended: At,
    /// When a line written later than `ended` was written.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "rfc3339::option"
    )]
    pub recorded: Option<At>,
    /// The end word.
    pub end: End,
    /// History's `outcome`, the store's status word.
    pub outcome: String,
    /// History's `status`, the exit status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u8>,
    /// Whether it parked.
    pub parked: bool,
    /// Whether a signal stopped it, left out when false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub signalled: bool,
    /// The change the line names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change: Option<ChangeNo>,
}

/// A cycle's history line says it parked. Witness: such a work line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CycleParked {
    /// The cycle.
    pub cycle: CycleId,
    /// The blueprint.
    pub blueprint: BlueprintNo,
    /// The ticket, left out when it is the blueprint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket: Option<TicketKey>,
    /// The end's time.
    #[serde(with = "rfc3339")]
    pub at: At,
    /// The end, one that stops for a person.
    pub end: Stop,
    /// The step it stopped at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<RunnerWord>,
    /// The cap that stopped it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap: Option<RunnerWord>,
    /// Why it parked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub park_reason: Option<RunnerWord>,
    /// The kind of failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_kind: Option<RunnerWord>,
    /// The failure's cause.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_cause: Option<RunnerWord>,
    /// Whether the model provider was asked about it.
    pub provider: bool,
    /// How many times the same park repeated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat: Option<std::num::NonZeroU32>,
    /// The change the line names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change: Option<ChangeNo>,
}

/// A question is recorded. Witness: `tehas question --json --after <id>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionAsked {
    /// The store's number, `tehas question`'s `id`.
    pub question: QuestionNo,
    /// A park's question or a plan's.
    pub kind: QuestionKind,
    /// The blueprint.
    pub blueprint: BlueprintNo,
    /// The cycle that parked, for a park's question only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycle: Option<CycleId>,
    /// `askedAt`.
    #[serde(with = "rfc3339")]
    pub asked_at: At,
    /// The choices, in order.
    pub choices: Vec<QuestionChoice>,
}

/// A cycle left a change for review. Witness: a work line with `change`,
/// no `signalled`, and an end other than `refused` or `merged`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeOffered {
    /// The change.
    pub change: ChangeNo,
    /// The blueprint.
    pub blueprint: BlueprintNo,
    /// The cycle.
    pub cycle: CycleId,
    /// The cycle's end.
    pub end: End,
    /// The end's time, the line's `ended`.
    #[serde(with = "rfc3339")]
    pub at: At,
}

/// An allowed account's act is recorded. Witness: the act in the tickets
/// port's history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketActed {
    /// The ticket.
    pub ticket: TicketKey,
    /// What the account did.
    pub act: Act,
    /// Who did it.
    pub by: Account,
    /// When, in the supplier's history.
    #[serde(with = "rfc3339")]
    pub at: At,
}

/// A ticket the factory follows is recorded closed. Witness: `Closed` in
/// the tickets port's history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketClosed {
    /// The ticket.
    pub ticket: TicketKey,
    /// Why it closed.
    pub reason: CloseReason,
    /// When.
    #[serde(with = "rfc3339")]
    pub at: At,
}

/// A change linked to a blueprint is recorded merged. Witness: `Merged` in
/// the code host port's activity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeMerged {
    /// The change.
    pub change: ChangeNo,
    /// The blueprint it is linked to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blueprint: Option<BlueprintNo>,
    /// When.
    #[serde(with = "rfc3339")]
    pub at: At,
}

/// `tehas limit --json` without `api`, after a write that changed the
/// limit. The envelope's key and `at` are `set`, or for a clear `cleared`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitChanged {
    /// When the limit was written, left out with no limit.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "rfc3339::option"
    )]
    pub set: Option<At>,
    /// How the limit stands.
    pub state: LimitState,
    /// The cycles it allows at once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycles: Option<u32>,
    /// Whether planning runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<bool>,
    /// When it ends.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "rfc3339::option"
    )]
    pub until: Option<At>,
    /// Why it was set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// A clear's own time, on a clear alone, which has no `set`. tehas keys
    /// the clear on it; a consumer that sees the clear in a poll keys it on
    /// its poll's time.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "rfc3339::option"
    )]
    pub cleared: Option<At>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

macro_rules! payload {
    ($t:ident, $name:ident, |$p:ident| $key:expr, |$q:ident| $at:expr) => {
        impl From<$t> for Event {
            fn from(p: $t) -> Event {
                Event::$name(p)
            }
        }

        impl Payload for $t {
            const NAME: Name = Name::$name;

            fn key(&self) -> Key {
                let $p = self;
                $key
            }

            fn at(&self) -> At {
                let $q = self;
                $at
            }

            fn peek(e: &Event) -> Option<&Self> {
                match e {
                    Event::$name(p) => Some(p),
                    _ => None,
                }
            }
        }
    };
}

payload!(
    CycleStarted,
    CycleStarted,
    |p| key_of(&[&p.cycle.to_string()]),
    |p| p.started
);
payload!(
    CycleParked,
    CycleParked,
    |p| key_of(&[&p.cycle.to_string()]),
    |p| p.at
);
payload!(
    QuestionAsked,
    QuestionAsked,
    |p| key_of(&[&p.question.get().to_string()]),
    |p| p.asked_at
);
payload!(
    CycleEnded,
    CycleEnded,
    |p| key_of(&[&p.cycle.to_string()]),
    |p| p.ended
);
payload!(
    ChangeOffered,
    ChangeOffered,
    |p| key_of(&[&p.cycle.to_string()]),
    |p| p.at
);
payload!(
    LimitChanged,
    FactoryLimitChanged,
    |p| key_of(&[&time(p.set.or(p.cleared))]),
    |p| p.set.or(p.cleared).unwrap_or(At::new(0))
);
payload!(
    TicketActed,
    TicketActed,
    |p| key_of(&[
        &super::words::ticket_text(&p.ticket),
        act_word(p.act),
        &time(Some(p.at))
    ]),
    |p| p.at
);
payload!(
    TicketClosed,
    TicketClosed,
    |p| key_of(&[&super::words::ticket_text(&p.ticket), &time(Some(p.at))]),
    |p| p.at
);
payload!(
    ChangeMerged,
    ChangeMerged,
    |p| key_of(&[&p.change.get().to_string()]),
    |p| p.at
);

// A time in a key: its RFC 3339 text, or its seconds where it has none.
fn time(at: Option<At>) -> String {
    let at = at.unwrap_or(At::new(0));
    rfc3339::text(at).unwrap_or_else(|| at.secs().to_string())
}

// An act in a key: its name as the ports' serde writes it, `Approve`. A
// test pins that the two agree.
pub(crate) fn act_word(a: Act) -> &'static str {
    match a {
        Act::Approve => "Approve",
        Act::Plan => "Plan",
        Act::Refine => "Refine",
        Act::Finish => "Finish",
        Act::Block => "Block",
        Act::Pause => "Pause",
        _ => "Unknown",
    }
}
