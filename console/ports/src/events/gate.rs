//! The gate's JSON: the `before` side of a point, the ask a listener gets
//! and the answer it gives. 2.0 ships the shape; the raise, `ask` and
//! `Gated` come in 2.1 with the first gate's raise point.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::words::key_of;
use super::{rfc3339, How, Key, Name, Source, When, API};
use crate::ids::checked;
use crate::{At, BlueprintNo, Login, TicketKey};

/// The payload of a gate: the `data` of an `Ask`.
pub trait GatePayload: Serialize + DeserializeOwned + Clone + 'static {
    /// The point's name.
    const NAME: Name;
    /// The ask's identity within its name.
    fn key(&self) -> Key;
    /// The time the ask is about.
    fn at(&self) -> At;
}

/// The `before` side of `cycle.started`: a start is decided and its
/// container has not started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartAsk {
    /// The blueprint.
    pub blueprint: BlueprintNo,
    /// The ticket, left out when it is the blueprint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket: Option<TicketKey>,
    /// What starts it.
    pub how: How,
    /// Who starts it, left out for a cycle run by hand.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<Login>,
    /// The approval's time.
    #[serde(with = "rfc3339")]
    pub approval: At,
}

impl GatePayload for StartAsk {
    const NAME: Name = Name::CycleStarted;

    fn key(&self) -> Key {
        let approval =
            rfc3339::text(self.approval).unwrap_or_else(|| self.approval.secs().to_string());
        key_of(&[&self.blueprint.get().to_string(), &approval])
    }

    fn at(&self) -> At {
        self.approval
    }
}

/// A gate's envelope: the fields of `Envelope`, `when` always `before`,
/// `event` the payload's name. It reads only as that.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "AskWire<P>", bound = "P: GatePayload")]
pub struct Ask<P: GatePayload> {
    /// The events' contract version, `API`.
    pub api: u32,
    /// The factory, set by a consumer that relays many.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    /// The point's name.
    pub event: Name,
    /// The ask.
    pub data: P,
    /// Always `before`.
    pub when: When,
    /// The ask's identity within its name.
    pub key: Key,
    /// The time the ask is about.
    #[serde(with = "rfc3339")]
    pub at: At,
}

#[derive(Deserialize)]
#[serde(bound = "P: GatePayload")]
struct AskWire<P> {
    api: u32,
    #[serde(default)]
    source: Option<Source>,
    event: Name,
    data: P,
    when: When,
    key: Key,
    #[serde(with = "rfc3339")]
    at: At,
}

impl<P: GatePayload> TryFrom<AskWire<P>> for Ask<P> {
    type Error = crate::NotValid;
    fn try_from(w: AskWire<P>) -> Result<Self, Self::Error> {
        if w.event != P::NAME || w.when != When::Before {
            return Err(crate::NotValid {
                what: "gate ask, which is before its own point",
            });
        }
        Ok(Ask {
            api: w.api,
            source: w.source,
            event: w.event,
            data: w.data,
            when: w.when,
            key: w.key,
            at: w.at,
        })
    }
}

impl<P: GatePayload> Ask<P> {
    /// The ask of a payload: its key and time, no source.
    pub fn new(p: P) -> Self {
        Ask {
            api: API,
            source: None,
            event: P::NAME,
            key: p.key(),
            at: p.at(),
            when: When::Before,
            data: p,
        }
    }
}

checked!(
    /// Why a gate's listener refused: 1 to 140 characters, no control
    /// character.
    Why,
    "gate answer's why",
    why_ok
);

fn why_ok(s: &str) -> bool {
    (1..=140).contains(&s.chars().count()) && !s.chars().any(char::is_control)
}

/// A gate listener's word. Exhaustive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    /// The act goes on.
    Allow,
    /// The act is held.
    Refuse,
}

/// What a gate's listener answers, `/out/_tehas/answer.json`:
/// `{"api": 1, "answer": "allow" | "refuse", "why": "..."}`. A refusal
/// says why; an allow may.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "AnswerWire")]
pub struct GateAnswer {
    /// The events' contract version, `API`.
    pub api: u32,
    /// Allow or refuse.
    pub answer: Verdict,
    /// Why, which a refusal has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<Why>,
}

#[derive(Deserialize)]
struct AnswerWire {
    api: u32,
    answer: Verdict,
    #[serde(default)]
    why: Option<Why>,
}

impl TryFrom<AnswerWire> for GateAnswer {
    type Error = crate::NotValid;
    fn try_from(w: AnswerWire) -> Result<Self, Self::Error> {
        if w.answer == Verdict::Refuse && w.why.is_none() {
            return Err(crate::NotValid {
                what: "gate answer, whose refusal says why",
            });
        }
        Ok(GateAnswer {
            api: w.api,
            answer: w.answer,
            why: w.why,
        })
    }
}

impl GateAnswer {
    /// The act goes on.
    pub fn allow() -> Self {
        GateAnswer {
            api: API,
            answer: Verdict::Allow,
            why: None,
        }
    }

    /// The act is held, for `why`.
    pub fn refuse(why: Why) -> Self {
        GateAnswer {
            api: API,
            answer: Verdict::Refuse,
            why: Some(why),
        }
    }
}
