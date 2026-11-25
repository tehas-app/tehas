//! The words of the events: `Key`, `CycleId`, `End`, `Stop`, `How`,
//! `Source`, `RunnerWord`, `QuestionNo`, `QuestionChoice`, `LimitState`,
//! and the text of a ticket in a key.

use std::fmt;
use std::num::{NonZeroU32, NonZeroU64};

use serde::{Deserialize, Serialize};

use crate::ids::checked;
use crate::{BlueprintNo, TicketKey};

checked!(
    /// An event's identity within its name, made from the fact by the
    /// payload's own code: no white space, 1 to 200 characters. Its parts
    /// are joined with `@`, which no part holds.
    Key,
    "event key",
    key_ok
);

fn key_ok(s: &str) -> bool {
    (1..=200).contains(&s.chars().count()) && !s.chars().any(char::is_whitespace)
}

checked!(
    /// A factory's name, as a consumer of many factories names it: 1 to 64
    /// lower-case letters, digits and `-`. tehas never sets it.
    Source,
    "source",
    source_ok
);

fn source_ok(s: &str) -> bool {
    (1..=64).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

checked!(
    /// A runner's word as the store keeps it: cleaned, 1 to 80 characters,
    /// no control character. The runner owns the set; tehas shows it and
    /// decides nothing on it.
    RunnerWord,
    "runner word",
    runner_word_ok
);

fn runner_word_ok(s: &str) -> bool {
    (1..=80).contains(&s.chars().count()) && !s.chars().any(char::is_control)
}

/// The identity of one cycle, `<blueprint>-<attempt>`: `12-3`. Each part is
/// 1 to 9 digits with no leading zero. Unique within one store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CycleId {
    /// The blueprint the cycle worked.
    pub blueprint: BlueprintNo,
    /// The attempt, from 1 for each blueprint.
    pub attempt: NonZeroU32,
}

const CYCLE_PART: u64 = 999_999_999;

impl CycleId {
    /// The id, or None when a part has more than nine digits.
    pub fn new(blueprint: BlueprintNo, attempt: NonZeroU32) -> Option<Self> {
        (blueprint.get() <= CYCLE_PART && u64::from(attempt.get()) <= CYCLE_PART)
            .then_some(Self { blueprint, attempt })
    }

    /// The id a text names, if it is `<blueprint>-<attempt>`.
    pub fn read(s: &str) -> Option<Self> {
        let (b, a) = s.split_once('-')?;
        let part = |p: &str| -> Option<u64> {
            let ok = (1..=9).contains(&p.len())
                && !p.starts_with('0')
                && p.bytes().all(|c| c.is_ascii_digit());
            ok.then(|| p.parse().ok()).flatten()
        };
        let blueprint = BlueprintNo::new(part(b)?)?;
        let attempt = NonZeroU32::new(u32::try_from(part(a)?).ok()?)?;
        Self::new(blueprint, attempt)
    }
}

impl fmt::Display for CycleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.blueprint.get(), self.attempt)
    }
}

impl TryFrom<String> for CycleId {
    type Error = crate::NotValid;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::read(&s).ok_or(crate::NotValid { what: "cycle id" })
    }
}

impl From<CycleId> for String {
    fn from(c: CycleId) -> String {
        c.to_string()
    }
}

/// The closed set of words a cycle ended with,
/// `tehas_core::outcome::OUTCOMES`. Exhaustive: a new word is a major
/// change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum End {
    /// It left a change for review and did not park.
    Offered,
    /// Its change was merged within the cycle.
    Merged,
    /// The runner handed off to a person.
    Handoff,
    /// Any other end that needs a person.
    Parked,
    /// It failed.
    Failed,
    /// No cycle ran.
    Refused,
    /// Closed, interrupted or stopped.
    Stopped,
}

impl End {
    /// Every end, in the order of `OUTCOMES`.
    pub const ALL: [End; 7] = [
        End::Offered,
        End::Merged,
        End::Handoff,
        End::Parked,
        End::Failed,
        End::Refused,
        End::Stopped,
    ];

    /// The stop this end is, if it stops for a person.
    pub fn stop(self) -> Option<Stop> {
        match self {
            End::Handoff => Some(Stop::Handoff),
            End::Parked => Some(Stop::Parked),
            End::Failed => Some(Stop::Failed),
            End::Refused => Some(Stop::Refused),
            End::Offered | End::Merged | End::Stopped => None,
        }
    }
}

/// The ends that stop for a person, `tehas_core::outcome::STOPS`.
/// Exhaustive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stop {
    /// The runner handed off to a person.
    Handoff,
    /// Any other end that needs a person.
    Parked,
    /// It failed.
    Failed,
    /// No cycle ran.
    Refused,
}

impl From<Stop> for End {
    fn from(s: Stop) -> End {
        match s {
            Stop::Handoff => End::Handoff,
            Stop::Parked => End::Parked,
            Stop::Failed => End::Failed,
            Stop::Refused => End::Refused,
        }
    }
}

/// What started a cycle, `cycles.how`. May grow. `approved` is 1.x's
/// `label`, renamed in 2.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum How {
    /// An allowed account approved the ticket.
    Approved,
    /// Its change was brought up to date.
    Refresh,
    /// A retry after the model provider's outage.
    OutageRetry,
    /// The millwright asked for it.
    Millwright,
    /// The host started it.
    Host,
    /// A person's reply asked for a retry.
    Reply,
    /// `tehas answer` asked for it.
    Answer,
    /// A person ran it by hand.
    Hand,
    /// A word this copy does not know.
    #[serde(other)]
    Unknown,
}

/// A question's number in the factory's store, as `tehas question` prints
/// it: the store's own, given in order, never a supplier's comment id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct QuestionNo(NonZeroU64);

impl QuestionNo {
    /// The number, or None for 0.
    pub fn new(n: u64) -> Option<Self> {
        NonZeroU64::new(n).map(Self)
    }

    /// The number.
    pub fn get(self) -> u64 {
        self.0.get()
    }
}

/// One choice of a question, in the words `tehas answer` takes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct QuestionChoice {
    /// The word a reply or `tehas answer` gives.
    pub choice: String,
    /// What the choice does, for a person.
    pub label: String,
}

new!(QuestionChoice {
    choice: String,
    label: String
});

/// How the factory's limit stands, as `tehas limit --json` says.
/// Exhaustive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LimitState {
    /// No limit.
    None,
    /// A limit holds.
    Ok,
    /// A limit whose `until` has passed.
    Expired,
    /// A limit file that does not read.
    Unreadable,
}

/// The text of a ticket in a key: a number's digits, or the key as it is.
pub(crate) fn ticket_text(t: &TicketKey) -> String {
    match t {
        TicketKey::Number(n) => n.to_string(),
        TicketKey::Key(k) => k.as_str().to_string(),
    }
}

/// A key of parts joined with `@`. A payload makes each part from words
/// that hold no white space and fit the length, so no payload makes a key
/// that breaks the rule; one that would is `-`.
pub(crate) fn key_of(parts: &[&str]) -> Key {
    let text = parts.join("@");
    Key::new(text).unwrap_or_else(|_| Key("-".to_string()))
}
