//! The pure state machine of one report: `next`, with `Notice`, `Health`,
//! `Word` and the `Notices` sink it reports to. An outage is the host's; a
//! spent pool and a sign-out are one holder's on that host. Only a change
//! of state raises a notice, and the adapter raises it only when its
//! `Ledger::swap_health` wins, so many processes report one change once.

use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{unknown, At, HealthState, HostId, Outcome, Passing, Pool, Realm, Whose};

/// The next state of `whose` on `host`, from the state it `was` in and the
/// outcome a call has just `seen`, and the notice due, if any. Only a
/// change of state raises one.
///
/// The host's own state reads `Unavailable` as an outage and any answer of
/// the site as the host up. A holder's state reads `Spent` and `SignedOut`
/// as its own; any other answer is a good call that clears a sign-out, and
/// clears a spent pool from `until` on, since a call before then was in
/// another pool. `Unread` tells neither anything. A state of the other
/// kind (a holder's state given for the host, or the host's for a holder)
/// is read as `Healthy`.
pub fn next(
    host: &HostId,
    whose: &Whose,
    realm: Realm,
    now: At,
    was: &HealthState,
    seen: &Result<(), Outcome>,
) -> (HealthState, Option<Notice>) {
    let was = own(whose, *was);
    let new = match whose {
        Whose::Host => host_next(now, was, seen),
        Whose::Holder(_) => holder_next(now, was, seen),
    };
    let what = changed(&was, &new).then_some(match new {
        HealthState::Healthy => Health::Recovered,
        HealthState::Failing { why, .. } => Health::Unavailable(why),
        HealthState::Spent { pool, until } => Health::Spent { pool, until },
        HealthState::SignedOut { .. } => Health::SignedOut,
    });
    let notice = what.map(|what| Notice {
        host: host.clone(),
        whose: whose.clone(),
        realm,
        what,
        at: now,
    });
    (new, notice)
}

fn own(whose: &Whose, was: HealthState) -> HealthState {
    match (whose, was) {
        (Whose::Host, HealthState::Spent { .. } | HealthState::SignedOut { .. })
        | (Whose::Holder(_), HealthState::Failing { .. }) => HealthState::Healthy,
        (_, was) => was,
    }
}

fn host_next(now: At, was: HealthState, seen: &Result<(), Outcome>) -> HealthState {
    match (was, seen) {
        (_, Err(Outcome::Unread(_))) => was,
        (HealthState::Failing { .. }, Err(Outcome::Unavailable { .. })) => was,
        (_, Err(Outcome::Unavailable { why, .. })) => HealthState::Failing {
            since: now,
            why: *why,
        },
        _ => HealthState::Healthy,
    }
}

fn holder_next(now: At, was: HealthState, seen: &Result<(), Outcome>) -> HealthState {
    match (was, seen) {
        (_, Err(Outcome::Unread(_) | Outcome::Unavailable { .. })) => was,
        (_, Err(Outcome::Spent { pool, until })) => HealthState::Spent {
            pool: *pool,
            until: *until,
        },
        (HealthState::SignedOut { .. }, Err(Outcome::SignedOut)) => was,
        (_, Err(Outcome::SignedOut)) => HealthState::SignedOut { since: now },
        (HealthState::Spent { until, .. }, _) if now < until => was,
        _ => HealthState::Healthy,
    }
}

/// A change of state: another kind, or a spent pool of another pool. A new
/// `until` of the same pool, or a new `since` or `why`, is not one.
fn changed(was: &HealthState, new: &HealthState) -> bool {
    match (was, new) {
        (HealthState::Spent { pool: a, .. }, HealthState::Spent { pool: b, .. }) => a != b,
        _ => std::mem::discriminant(was) != std::mem::discriminant(new),
    }
}

/// One change of health, to report once.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Notice {
    /// The host it is about.
    pub host: HostId,
    /// The host's own health, or one holder's.
    pub whose: Whose,
    /// The realm of the site that saw it.
    pub realm: Realm,
    /// What changed.
    pub what: Health,
    /// When it was seen.
    pub at: At,
}

/// What a notice says. `TokenNotDropped`: a refused token could not be
/// dropped from the `TokenStore`, so another process may still read it
/// until it expires. `Unknown`: a value of a newer copy was read (see
/// "Enums that may grow").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
#[non_exhaustive]
pub enum Health {
    /// The host is out, with a passing failure.
    Unavailable(Passing),
    /// The holder's pool is out until then.
    Spent {
        /// The pool that is out.
        pool: Pool,
        /// When it has budget again.
        until: At,
    },
    /// The holder's credential is no longer good.
    SignedOut,
    /// The host or the holder is healthy again.
    Recovered,
    /// The ledger could not be read or written, so a change may be
    /// reported twice.
    LedgerDown,
    /// A refused token could not be dropped, so another process may still
    /// read it until it expires.
    TokenNotDropped,
    /// A value of a newer copy was read, of the enum the word names. A
    /// notice this copy does not know reads as `Unknown(Word::Health)`.
    #[serde(
        untagged,
        deserialize_with = "health_unknown",
        serialize_with = "health_unknown_tag"
    )]
    Unknown(Word),
}

/// `Unknown` from any tagged value: its own word where the tag is
/// `Unknown` and its content a word, else `Word::Health`, for a notice
/// this copy does not know. A value with no tag stays an error.
fn health_unknown<'de, D: Deserializer<'de>>(d: D) -> Result<Word, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Content {
        Word(Word),
        Other(serde::de::IgnoredAny),
    }
    #[derive(Deserialize)]
    struct Tagged {
        r#type: String,
        #[serde(default)]
        value: Option<Content>,
    }
    let tagged = Tagged::deserialize(d)?;
    Ok(match (tagged.r#type.as_str(), tagged.value) {
        ("Unknown", Some(Content::Word(word))) => word,
        _ => Word::Health,
    })
}

fn health_unknown_tag<S: Serializer>(word: &Word, s: S) -> Result<S::Ok, S::Error> {
    let mut map = s.serialize_map(Some(2))?;
    map.serialize_entry(unknown::TAG, "Unknown")?;
    map.serialize_entry("value", word)?;
    map.end()
}

/// The enums that may grow, by name, for `Health::Unknown`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Word {
    /// `Mark`.
    Mark,
    /// `Act`.
    Act,
    /// `State`.
    State,
    /// `Choice`.
    Choice,
    /// `Kind`.
    Kind,
    /// `MarkerKind`.
    MarkerKind,
    /// `Scope`.
    Scope,
    /// `Happened`.
    Happened,
    /// `ChangeHappened`.
    ChangeHappened,
    /// `Marker`.
    Marker,
    /// `QuestionKind`.
    QuestionKind,
    /// `Write`.
    Write,
    /// `WriteKind`.
    WriteKind,
    /// `Receipt`.
    Receipt,
    /// `Refusal`.
    Refusal,
    /// `Unread`.
    Unread,
    /// `Pool`.
    Pool,
    /// `NotMergeable`.
    NotMergeable,
    /// `Conclusion`.
    Conclusion,
    /// `Verdict`.
    Verdict,
    /// `Takeover`.
    Takeover,
    /// `Health`.
    Health,
    /// `PortName`.
    PortName,
    /// A word this copy does not know.
    #[serde(other)]
    Unknown,
}

/// Where notices go. tehas's sink writes each as one fixed line in our
/// words to its process's log; the consumer's sink is its own.
pub trait Notices: Send + Sync {
    /// Report one notice.
    fn notice(&self, n: Notice);
}
