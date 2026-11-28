//! The pure state machine of one report: `next`, with `Notice`, `Health`,
//! `Word` and the `Notices` sink it reports to. `observe` is owned here
//! too: the rule of what an answer does to the ledger and the sink, called
//! by the fake's handle and the adapter's anonymous handle alike. An
//! outage is the host's; a
//! sign-out is one holder's on that host, and a spent pool one pool's of
//! that holder. Only a change of state raises a notice, and the adapter
//! raises it only when its `Ledger::swap_health` wins, so many processes
//! report one change once.

use std::sync::atomic::{AtomicBool, Ordering};

use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    unknown, At, Failure, HealthState, Holder, HostId, Ledger, Outcome, Passing, Pool, Realm,
    SiteId, Whose,
};

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

/// How many seconds of failures a handle asks the ledger to keep.
pub const FAILURES_KEPT: u32 = 600;

/// Is this outcome a failure of the supplier, kept for the health check:
/// an outage, a spent pool, or the factory's own sign-out. An odd answer,
/// a failed check run (`Broken`) and another holder's sign-out are not.
pub fn kept_failure(o: &Outcome, holder: &Holder) -> bool {
    match o {
        Outcome::Unavailable { why, .. } => *why != Passing::Broken,
        Outcome::Spent { .. } => true,
        Outcome::SignedOut => matches!(holder, Holder::Ours(_)),
        _ => false,
    }
}

/// Tell `Health::LedgerDown` for `host`, once: `told_down` answers whether
/// it has been told, and the first caller tells it. A ledger that fails
/// fails toward reporting: the answer still goes to the caller, and a
/// change reported without the ledger may be reported again by the next
/// process to hold it.
pub fn ledger_down(
    sink: &dyn Notices,
    told_down: &AtomicBool,
    host: &HostId,
    realm: Realm,
    at: At,
) {
    if told_down.swap(true, Ordering::Relaxed) {
        return;
    }
    sink.notice(Notice {
        host: host.clone(),
        whose: Whose::Host,
        realm,
        what: Health::LedgerDown,
        at,
    });
}

/// What a handle gives `observe`: its ledger and sink, the once-flag of
/// `LedgerDown`, and the words each notice it tells will name.
pub struct Observer<'a> {
    /// Where states are read, swapped and kept.
    pub ledger: &'a dyn Ledger,
    /// Where notices go.
    pub sink: &'a dyn Notices,
    /// Whether this handle has told `LedgerDown` already.
    pub told_down: &'a AtomicBool,
    /// The host the notices name.
    pub host: &'a HostId,
    /// The holder the states of `keys` hang under.
    pub holder: &'a Holder,
    /// The realm the notices name.
    pub realm: Realm,
    /// The project a kept failure is for.
    pub site: &'a SiteId,
    /// When the answer was seen.
    pub at: At,
}

/// The one rule of what an answer does to the ledger's health, owned here
/// and called by the fake's handle and the adapter's anonymous handle
/// alike: give `seen` to each state `keys` names (the host, the holder,
/// the pools `pools` names, and a `Spent`'s own pool), advance it with
/// `next`, and swap it in the ledger; tell each notice whose swap wins,
/// and tell none whose swap loses. A ledger read or swap that fails fails
/// toward reporting: the notice of a read that failed is told from the
/// healthy base, and `ledger_down` tells once over `told_down`. A kept
/// failure (see `kept_failure`) is put in the ledger with `failed`, whose
/// failure is told the same way. Effects stay with the ledger and the
/// sink; the rule of their order is here alone.
pub fn observe(o: &Observer<'_>, seen: &Result<(), Outcome>, pools: &[Pool]) {
    for whose in keys(o.holder, seen, pools) {
        match o.ledger.health(o.host, &whose) {
            Ok(was) => {
                let (new, notice) = next(o.host, &whose, o.realm, o.at, &was, seen);
                if new == was {
                    continue;
                }
                match o.ledger.swap_health(o.host, &whose, &was, &new) {
                    Ok(true) => notice.into_iter().for_each(|n| o.sink.notice(n)),
                    Ok(false) => {}
                    Err(_) => {
                        notice.into_iter().for_each(|n| o.sink.notice(n));
                        ledger_down(o.sink, o.told_down, o.host, o.realm, o.at);
                    }
                }
            }
            Err(_) => {
                let healthy = HealthState::Healthy;
                let (_, notice) = next(o.host, &whose, o.realm, o.at, &healthy, seen);
                notice.into_iter().for_each(|n| o.sink.notice(n));
                ledger_down(o.sink, o.told_down, o.host, o.realm, o.at);
            }
        }
    }
    if let Err(err) = seen {
        if kept_failure(err, o.holder) {
            let f = Failure::new(o.at, o.site.clone(), err.clone());
            if o.ledger.failed(o.host, f, FAILURES_KEPT).is_err() {
                ledger_down(o.sink, o.told_down, o.host, o.realm, o.at);
            }
        }
    }
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
#[non_exhaustive]
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

#[cfg(test)]
mod tests {
    use super::{kept_failure, ledger_down, observe, Observer, FAILURES_KEPT};
    use crate::{
        At, Budget, Failure, Health, HealthState, Holder, HolderId, HostId, Ledger, LedgerDown,
        Notice, Notices, Outcome, Passing, Pool, Realm, Sent, SiteId, Whose,
    };
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    struct Sink(Mutex<Vec<Notice>>);

    impl Sink {
        fn new() -> Sink {
            Sink(Mutex::new(Vec::new()))
        }

        /// Whose and what of each notice told, in order.
        fn told(&self) -> Vec<(Whose, Health)> {
            self.0
                .lock()
                .unwrap()
                .iter()
                .map(|n| (n.whose.clone(), n.what))
                .collect()
        }
    }

    impl Notices for Sink {
        fn notice(&self, n: Notice) {
            self.0.lock().unwrap().push(n);
        }
    }

    fn outage() -> Result<(), Outcome> {
        Err(Outcome::Unavailable {
            why: Passing::Server,
            sent: Sent::Maybe,
        })
    }

    fn host() -> HostId {
        HostId::new("example.test")
    }

    fn holder() -> Holder {
        Holder::Anonymous(HolderId::new("caller"))
    }

    fn at() -> At {
        At::new(1000)
    }

    /// A ledger whose every arm answers as the fns answer.
    struct Script {
        health: fn() -> Result<HealthState, LedgerDown>,
        swap: fn() -> Result<bool, LedgerDown>,
        failed: fn() -> Result<(), LedgerDown>,
    }

    fn sound() -> Script {
        Script {
            health: || Ok(HealthState::Healthy),
            swap: || Ok(true),
            failed: || Ok(()),
        }
    }

    impl Ledger for Script {
        fn budget(&self, _: &HostId, _: &Holder, _: Pool) -> Result<Option<Budget>, LedgerDown> {
            Ok(None)
        }
        fn put_budget(&self, _: &HostId, _: &Holder, _: Pool, _: Budget) -> Result<(), LedgerDown> {
            Ok(())
        }
        fn health(&self, _: &HostId, _: &Whose) -> Result<HealthState, LedgerDown> {
            (self.health)()
        }
        fn swap_health(
            &self,
            _: &HostId,
            _: &Whose,
            _: &HealthState,
            _: &HealthState,
        ) -> Result<bool, LedgerDown> {
            (self.swap)()
        }
        fn failed(&self, _: &HostId, _: Failure, _: u32) -> Result<(), LedgerDown> {
            (self.failed)()
        }
        fn failures(&self, _: &HostId, _: At) -> Result<Vec<Failure>, LedgerDown> {
            Ok(Vec::new())
        }
    }

    fn run(s: &Script, sink: &Sink, told_down: &AtomicBool, seen: &Result<(), Outcome>) {
        let (h, d, st) = (host(), holder(), SiteId::new("someone/there"));
        let o = Observer {
            ledger: s,
            sink,
            told_down,
            host: &h,
            holder: &d,
            realm: Realm::CodeHost,
            site: &st,
            at: at(),
        };
        observe(&o, seen, &[Pool::Calls]);
    }

    /// A change whose swap loses says nothing: the winner tells it.
    #[test]
    fn a_swap_that_loses_says_nothing() {
        let s = Script {
            swap: || Ok(false),
            ..sound()
        };
        let sink = Sink::new();
        let told_down = AtomicBool::new(false);
        run(&s, &sink, &told_down, &outage());
        assert_eq!(sink.told(), [], "the loser tells the host nothing");
        assert!(!told_down.load(Ordering::Relaxed));
    }

    /// A change whose swap fails is still told, and the ledger is told
    /// down once.
    #[test]
    fn a_swap_that_fails_raises_and_fails_toward_reporting() {
        let s = Script {
            swap: || Err(LedgerDown),
            ..sound()
        };
        let sink = Sink::new();
        let told_down = AtomicBool::new(false);
        run(&s, &sink, &told_down, &outage());
        assert_eq!(
            sink.told(),
            [
                (Whose::Host, Health::Unavailable(Passing::Server)),
                (Whose::Host, Health::LedgerDown),
            ],
            "the change is told from the state read, then the ledger down"
        );
    }

    /// A ledger that cannot be read tells each change from the healthy
    /// base, and says the ledger down once, however many keys failed.
    #[test]
    fn a_ledger_read_that_fails_raises_from_the_healthy_base() {
        let s = Script {
            health: || Err(LedgerDown),
            ..sound()
        };
        let sink = Sink::new();
        let told_down = AtomicBool::new(false);
        run(&s, &sink, &told_down, &outage());
        assert_eq!(
            sink.told(),
            [
                (Whose::Host, Health::Unavailable(Passing::Server)),
                (Whose::Host, Health::LedgerDown),
            ],
            "only the host's change is due from the base, and the ledger is told once"
        );
    }

    /// When every state is already the outage seen, only the failure
    /// record fails: that alone says the ledger down.
    #[test]
    fn only_the_failure_record_failing_says_the_ledger_down() {
        let s = Script {
            health: || {
                Ok(HealthState::Failing {
                    since: at(),
                    why: Passing::Server,
                })
            },
            failed: || Err(LedgerDown),
            ..sound()
        };
        let sink = Sink::new();
        let told_down = AtomicBool::new(false);
        run(&s, &sink, &told_down, &outage());
        assert_eq!(
            sink.told(),
            [(Whose::Host, Health::LedgerDown)],
            "no change was due, so only the ledger is told"
        );
    }

    /// The ledger is told down once however many arms fail, and once in
    /// all over the `told_down` it was given.
    #[test]
    fn the_ledger_is_told_down_once() {
        let s = Script {
            health: || Err(LedgerDown),
            ..sound()
        };
        let sink = Sink::new();
        let told_down = AtomicBool::new(false);
        run(&s, &sink, &told_down, &outage());
        run(&s, &sink, &told_down, &outage());
        ledger_down(&sink, &told_down, &host(), Realm::CodeHost, at());
        assert_eq!(
            sink.told(),
            [
                (Whose::Host, Health::Unavailable(Passing::Server)),
                (Whose::Host, Health::LedgerDown),
                (Whose::Host, Health::Unavailable(Passing::Server)),
            ],
            "each call tells its change again, but the ledger is told down once"
        );
    }

    /// The kept set: an outage, a spent pool and our own sign-out; an odd
    /// answer, a broken check run and another caller's sign-out are not.
    #[test]
    fn kept_failures_are_the_suppliers_own() {
        let ours = Holder::Ours(HolderId::new("ours"));
        let out = |why: Passing| Outcome::Unavailable {
            why,
            sent: Sent::Maybe,
        };
        assert!(kept_failure(&out(Passing::Server), &holder()));
        assert!(!kept_failure(&out(Passing::Broken), &holder()));
        assert!(kept_failure(
            &Outcome::Spent {
                pool: Pool::Calls,
                until: at(),
            },
            &holder()
        ));
        assert!(kept_failure(&Outcome::SignedOut, &ours));
        assert!(!kept_failure(&Outcome::SignedOut, &holder()));
        assert!(!kept_failure(&Outcome::NotFound, &ours));
    }

    /// The window is what the health check reads back.
    #[test]
    fn the_failure_window_is_the_health_check_s() {
        assert_eq!(FAILURES_KEPT, 600);
    }
}
