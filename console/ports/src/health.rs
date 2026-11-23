//! The pure state machine of one report: `next`, with `Notice`, `Health`,
//! `Word` and the `Notices` sink it reports to. An outage is the host's; a
//! sign-out is one holder's on that host, and a spent pool one pool's of
//! that holder. Only a change of state raises a notice, and the adapter
//! raises it only when its `Ledger::swap_health` wins, so many processes
//! report one change once.

use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{unknown, At, HealthState, Holder, HostId, Outcome, Passing, Pool, Realm, Whose};

/// The next state of `whose` on `host`, from the state it `was` in and the
/// outcome a call has just `seen`, and the notice due, if any. Only a
/// change of state raises one: another variant.
///
/// A caller keeps `Failing` under `Whose::Host`, `SignedOut` under
/// `Whose::Holder` and `Spent` of a pool under `Whose::Pool` of that pool,
/// and gives a call's outcome to `next` once for each key `keys` returns:
/// the host, the holder, each pool the call drew on, and the pool a
/// `Spent` names. `seen` is the outcome of every
/// attempt the adapter made to reach the site, one that could not connect
/// included; never an answer it made without trying (a held pool's
/// `Spent`, a passed deadline's `Unavailable(Timeout)`, a kept or given
/// token, an expired kept or given token's `SignedOut`, an out-of-set
/// `Forbidden`).
///
/// The host reads `Unavailable` as an outage and any answer of the site as
/// the host up, `Unavailable(Broken)` among them: the site answered that a
/// check run or job errored. A holder reads `SignedOut` as its own and any
/// other answer but `Spent` as a good call that clears it. A pool reads
/// `Spent` of itself as its own and ignores `Spent` of another pool, which
/// `keys` gives to that pool. A good call clears a pool only from `until`
/// on, and `SignedOut` tells it nothing. `Unread` and `Unavailable` tell
/// neither holder nor pool anything, but `Unavailable(Broken)`, which
/// answers the holder's request, clears a holder's `SignedOut`. A state not
/// its own (a holder's or a
/// pool's given for the host, `Spent` of another pool) is read as
/// `Healthy`.
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
        Whose::Pool(_, pool) => pool_next(*pool, now, was, seen),
    };
    let changed = std::mem::discriminant(&was) != std::mem::discriminant(&new);
    let what = changed.then_some(match new {
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

/// The states an answer is given to, in this order: `Whose::Host`, then
/// `Whose::Holder(holder)`, then `Whose::Pool(holder, p)` for each pool
/// `drew` names, then, on a `Spent`, the pool it names, each key once. The
/// caller gives the answer to `next` once for each. A `Spent` belongs to the
/// pool the answer names, which need not be one the call drew on.
pub fn keys(holder: &Holder, seen: &Result<(), Outcome>, drew: &[Pool]) -> Vec<Whose> {
    let named = match seen {
        Err(Outcome::Spent { pool, .. }) => Some(*pool),
        _ => None,
    };
    let mut keys = vec![Whose::Host, Whose::Holder(holder.clone())];
    for pool in drew.iter().copied().chain(named) {
        let key = Whose::Pool(holder.clone(), pool);
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys
}

fn own(whose: &Whose, was: HealthState) -> HealthState {
    match (whose, was) {
        (_, HealthState::Healthy)
        | (Whose::Host, HealthState::Failing { .. })
        | (Whose::Holder(_), HealthState::SignedOut { .. }) => was,
        (Whose::Pool(_, mine), HealthState::Spent { pool, .. }) if pool == *mine => was,
        _ => HealthState::Healthy,
    }
}

fn host_next(now: At, was: HealthState, seen: &Result<(), Outcome>) -> HealthState {
    match (was, seen) {
        (_, Err(Outcome::Unread(_))) => was,
        (
            _,
            Err(Outcome::Unavailable {
                why: Passing::Broken,
                ..
            }),
        ) => HealthState::Healthy,
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
        // A failed check run is the site answering the holder's request: the
        // sign-in works, as with any other good answer.
        (
            _,
            Err(Outcome::Unavailable {
                why: Passing::Broken,
                ..
            }),
        ) => HealthState::Healthy,
        (_, Err(Outcome::Unread(_) | Outcome::Unavailable { .. } | Outcome::Spent { .. })) => was,
        (HealthState::SignedOut { .. }, Err(Outcome::SignedOut)) => was,
        (_, Err(Outcome::SignedOut)) => HealthState::SignedOut { since: now },
        _ => HealthState::Healthy,
    }
}

fn pool_next(mine: Pool, now: At, was: HealthState, seen: &Result<(), Outcome>) -> HealthState {
    match (was, seen) {
        (_, Err(Outcome::Spent { pool, until })) if *pool == mine => HealthState::Spent {
            pool: mine,
            until: *until,
        },
        (
            _,
            Err(
                Outcome::Spent { .. }
                | Outcome::Unread(_)
                | Outcome::Unavailable { .. }
                | Outcome::SignedOut,
            ),
        ) => was,
        // An answer to a request sent before the spend was kept can arrive
        // after it.
        (HealthState::Spent { until, .. }, _) if now < until => was,
        _ => HealthState::Healthy,
    }
}

/// One change of health, to report once. Its `until`, where it has one, is
/// the first estimate; the ledger and each call's outcome carry the
/// current one.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Notice {
    /// The host it is about.
    pub host: HostId,
    /// The host's own health, a holder's, or one pool's.
    pub whose: Whose,
    /// The realm of the site that saw it.
    pub realm: Realm,
    /// What changed.
    pub what: Health,
    /// When it was seen.
    pub at: At,
}

new!(Notice {
    host: HostId,
    whose: Whose,
    realm: Realm,
    what: Health,
    at: At
});

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
        /// When it has budget again, as first estimated.
        until: At,
    },
    /// The holder's credential is no longer good.
    SignedOut,
    /// The host, the holder or the pool is healthy again.
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
    /// `HappenedKind`.
    HappenedKind,
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
    /// `events::Name`.
    EventName,
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
