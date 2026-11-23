//! `health::next` as a table: each state and outcome, for the host, a
//! holder and a pool; a notice only on a change of state; an outage and its
//! end, a spent pool and a sign-out, each reported once for every project
//! on the host.

use tehas_ports::*;

fn host() -> HostId {
    HostId::new("code-host")
}

fn grant() -> Holder {
    Holder::Ours(HolderId::new("grant-1"))
}

fn ours() -> Whose {
    Whose::Holder(grant())
}

fn pool(p: Pool) -> Whose {
    Whose::Pool(grant(), p)
}

const NOW: i64 = 1_000;

/// A state, the outcome seen, and the state and notice `next` gives.
type Case = (
    HealthState,
    Result<(), Outcome>,
    HealthState,
    Option<Health>,
);

fn at(secs: i64) -> At {
    At::new(secs)
}

fn failing(since: i64) -> HealthState {
    HealthState::Failing {
        since: at(since),
        why: Passing::Timeout,
    }
}

fn spent(pool: Pool, until: i64) -> HealthState {
    HealthState::Spent {
        pool,
        until: at(until),
    }
}

fn signed_out(since: i64) -> HealthState {
    HealthState::SignedOut { since: at(since) }
}

/// A failure an attempt to reach the site met. One that could not connect
/// never went out; any other did, and no answer came. A passed deadline's
/// `Timeout` with `sent: No` is made without trying and never given.
fn down(why: Passing) -> Result<(), Outcome> {
    let sent = match why {
        Passing::Unreachable => Sent::No,
        _ => Sent::Maybe,
    };
    Err(Outcome::Unavailable { why, sent })
}

/// The site's answer that the work asked about errored.
fn broken() -> Result<(), Outcome> {
    Err(Outcome::Unavailable {
        why: Passing::Broken,
        sent: Sent::Maybe,
    })
}

fn out_of(pool: Pool, until: i64) -> Result<(), Outcome> {
    Err(Outcome::Spent {
        pool,
        until: at(until),
    })
}

/// Every outcome a call can meet, one of each kind.
fn every_seen() -> Vec<Result<(), Outcome>> {
    vec![
        Ok(()),
        Err(Outcome::NotFound),
        Err(Outcome::Conflict(Lost::Changed)),
        Err(Outcome::Refused(Refusal::Invalid)),
        Err(Outcome::Forbidden),
        Err(Outcome::SignedOut),
        out_of(Pool::Priced, NOW + 60),
        down(Passing::Timeout),
        Err(Outcome::Unavailable {
            why: Passing::Server,
            sent: Sent::Maybe,
        }),
        broken(),
        Err(Outcome::Unread(Unread::Malformed)),
    ]
}

fn step(
    whose: &Whose,
    was: &HealthState,
    seen: &Result<(), Outcome>,
) -> (HealthState, Option<Health>) {
    let (new, notice) = next(&host(), whose, Realm::CodeHost, at(NOW), was, seen);
    if let Some(n) = &notice {
        assert_eq!(n.host, host());
        assert_eq!(&n.whose, whose);
        assert_eq!(n.realm, Realm::CodeHost);
        assert_eq!(n.at, at(NOW));
    }
    (new, notice.map(|n| n.what))
}

#[test]
fn the_hosts_table() {
    let up = HealthState::Healthy;
    let out = failing(NOW - 30);
    let new_out = HealthState::Failing {
        since: at(NOW),
        why: Passing::Timeout,
    };
    let cases: Vec<Case> = vec![
        // Up: an outage begins on a passing failure, at now.
        (up, Ok(()), up, None),
        (
            up,
            down(Passing::Timeout),
            new_out,
            Some(Health::Unavailable(Passing::Timeout)),
        ),
        (
            up,
            down(Passing::Unreachable),
            HealthState::Failing {
                since: at(NOW),
                why: Passing::Unreachable,
            },
            Some(Health::Unavailable(Passing::Unreachable)),
        ),
        // Any answer of the site is the host up: not the host's outage.
        (up, Err(Outcome::Forbidden), up, None),
        (up, Err(Outcome::SignedOut), up, None),
        (up, out_of(Pool::Priced, NOW + 60), up, None),
        (up, Err(Outcome::Unread(Unread::Cut)), up, None),
        // A job that errored is the site's answer: the host is up.
        (up, broken(), up, None),
        // Out: more failures change nothing; the first answer ends it.
        (out, down(Passing::Server), out, None),
        (out, Ok(()), up, Some(Health::Recovered)),
        (out, Err(Outcome::NotFound), up, Some(Health::Recovered)),
        (
            out,
            Err(Outcome::Refused(Refusal::Exists)),
            up,
            Some(Health::Recovered),
        ),
        (
            out,
            out_of(Pool::Search, NOW + 5),
            up,
            Some(Health::Recovered),
        ),
        (out, broken(), up, Some(Health::Recovered)),
        // An answer that does not read tells nothing.
        (out, Err(Outcome::Unread(Unread::Malformed)), out, None),
    ];
    for (was, seen, new, notice) in cases {
        assert_eq!(
            step(&Whose::Host, &was, &seen),
            (new, notice),
            "{was:?} {seen:?}"
        );
    }
}

#[test]
fn a_holders_table() {
    let up = HealthState::Healthy;
    let gone = signed_out(NOW - 30);
    let cases: Vec<Case> = vec![
        (up, Ok(()), up, None),
        (up, Err(Outcome::Forbidden), up, None),
        (
            up,
            Err(Outcome::SignedOut),
            signed_out(NOW),
            Some(Health::SignedOut),
        ),
        // A spent pool is the pool's, an outage the host's.
        (up, out_of(Pool::Priced, NOW + 60), up, None),
        (up, down(Passing::Timeout), up, None),
        (up, Err(Outcome::Unread(Unread::Cut)), up, None),
        // Signed out: it never blocks; the next good call clears it.
        (gone, Err(Outcome::SignedOut), gone, None),
        (gone, Ok(()), up, Some(Health::Recovered)),
        (gone, Err(Outcome::Forbidden), up, Some(Health::Recovered)),
        (gone, out_of(Pool::Calls, NOW + 9), gone, None),
        (gone, Err(Outcome::Unread(Unread::Malformed)), gone, None),
        (gone, down(Passing::Unreachable), gone, None),
    ];
    for (was, seen, new, notice) in cases {
        assert_eq!(
            step(&ours(), &was, &seen),
            (new, notice),
            "{was:?} {seen:?}"
        );
    }
}

#[test]
fn a_pools_table() {
    let up = HealthState::Healthy;
    let priced = spent(Pool::Priced, NOW + 60);
    let past = spent(Pool::Priced, NOW);
    let cases: Vec<Case> = vec![
        (up, Ok(()), up, None),
        (up, Err(Outcome::Forbidden), up, None),
        (
            up,
            out_of(Pool::Priced, NOW + 60),
            priced,
            Some(Health::Spent {
                pool: Pool::Priced,
                until: at(NOW + 60),
            }),
        ),
        // Another pool's refusal, a sign-out and an outage are not this
        // pool's.
        (up, out_of(Pool::Search, NOW + 30), up, None),
        (up, Err(Outcome::SignedOut), up, None),
        (up, down(Passing::Timeout), up, None),
        (up, Err(Outcome::Unread(Unread::Cut)), up, None),
        // Spent: a good call before `until` changes nothing.
        (priced, Ok(()), priced, None),
        (priced, Err(Outcome::NotFound), priced, None),
        // From `until` on, a good call ends it.
        (past, Ok(()), up, Some(Health::Recovered)),
        (
            spent(Pool::Priced, NOW - 1),
            Err(Outcome::Conflict(Lost::Gone)),
            up,
            Some(Health::Recovered),
        ),
        // The same pool again moves `until`, with no notice.
        (
            priced,
            out_of(Pool::Priced, NOW + 120),
            spent(Pool::Priced, NOW + 120),
            None,
        ),
        (priced, out_of(Pool::Search, NOW + 30), priced, None),
        (past, out_of(Pool::Search, NOW + 30), past, None),
        (priced, Err(Outcome::SignedOut), priced, None),
        (past, Err(Outcome::SignedOut), past, None),
        (priced, down(Passing::Server), priced, None),
        (
            priced,
            Err(Outcome::Unread(Unread::Malformed)),
            priced,
            None,
        ),
    ];
    for (was, seen, new, notice) in cases {
        assert_eq!(
            step(&pool(Pool::Priced), &was, &seen),
            (new, notice),
            "{was:?} {seen:?}"
        );
    }
}

/// The states `whose` does not keep, which `next` reads as `Healthy`.
fn not_its_own(whose: &Whose) -> Vec<HealthState> {
    match whose {
        Whose::Host => vec![spent(Pool::Priced, NOW + 60), signed_out(NOW - 1)],
        Whose::Holder(_) => vec![failing(NOW - 1), spent(Pool::Priced, NOW + 60)],
        Whose::Pool(..) => vec![
            failing(NOW - 1),
            signed_out(NOW - 1),
            spent(Pool::Search, NOW + 60),
        ],
    }
}

/// Whether `whose` keeps `state` as its own.
fn keeps(whose: &Whose, state: &HealthState) -> bool {
    match (whose, state) {
        (_, HealthState::Healthy)
        | (Whose::Host, HealthState::Failing { .. })
        | (Whose::Holder(_), HealthState::SignedOut { .. }) => true,
        (Whose::Pool(_, mine), HealthState::Spent { pool, .. }) => pool == mine,
        _ => false,
    }
}

fn every_whose() -> Vec<Whose> {
    vec![Whose::Host, ours(), pool(Pool::Priced)]
}

#[test]
fn a_state_of_the_other_kind_reads_as_healthy() {
    for whose in every_whose() {
        for seen in every_seen() {
            let healthy = step(&whose, &HealthState::Healthy, &seen);
            for was in not_its_own(&whose) {
                assert_eq!(
                    step(&whose, &was, &seen),
                    healthy,
                    "{whose:?} {was:?} {seen:?}"
                );
                assert_ne!(healthy.0, was);
            }
        }
    }
}

#[test]
fn a_notice_is_raised_only_on_a_change_of_state() {
    let states = [
        HealthState::Healthy,
        failing(NOW - 30),
        spent(Pool::Priced, NOW + 60),
        spent(Pool::Priced, NOW - 1),
        spent(Pool::Search, NOW + 60),
        signed_out(NOW - 30),
    ];
    for whose in every_whose() {
        for was in states {
            for seen in every_seen() {
                let (new, notice) = step(&whose, &was, &seen);
                let was = if keeps(&whose, &was) {
                    was
                } else {
                    HealthState::Healthy
                };
                let change = std::mem::discriminant(&was) != std::mem::discriminant(&new);
                assert_eq!(notice.is_some(), change, "{whose:?} {was:?} {seen:?}");
                // Asked again from the new state, the same outcome raises
                // none.
                let again = step(&whose, &new, &seen);
                assert_eq!(again.1, None, "{whose:?} {new:?} {seen:?} twice");
                if let Some(what) = notice {
                    let expected = match new {
                        HealthState::Healthy => Health::Recovered,
                        HealthState::Failing { why, .. } => Health::Unavailable(why),
                        HealthState::Spent { pool, until } => Health::Spent { pool, until },
                        HealthState::SignedOut { .. } => Health::SignedOut,
                    };
                    assert_eq!(what, expected, "{whose:?} {was:?} {seen:?}");
                }
            }
        }
    }
}

/// The ledger's `swap_health` in one process's terms: written only if the
/// state is still what the caller read.
fn swap(kept: &mut HealthState, expect: &HealthState, new: &HealthState) -> bool {
    if kept == expect {
        *kept = *new;
        true
    } else {
        false
    }
}

/// Several short processes, each reading the kept state, asking `next`,
/// and raising its notice only when its swap wins. `racing` processes read
/// the state before any of them writes.
fn run(
    kept: &mut HealthState,
    whose: &Whose,
    seen: &Result<(), Outcome>,
    racing: usize,
) -> Vec<Notice> {
    run_at(NOW, kept, whose, seen, racing)
}

/// `run` at `now`.
fn run_at(
    now: i64,
    kept: &mut HealthState,
    whose: &Whose,
    seen: &Result<(), Outcome>,
    racing: usize,
) -> Vec<Notice> {
    let read = *kept;
    let mut raised = Vec::new();
    for project in 0..racing {
        let realm = if project % 2 == 0 {
            Realm::CodeHost
        } else {
            Realm::Tickets
        };
        let (new, notice) = next(&host(), whose, realm, at(now), &read, seen);
        if new != read && swap(kept, &read, &new) {
            raised.extend(notice);
        }
    }
    raised
}

#[test]
fn an_outage_and_its_end_are_reported_once_for_every_project_on_the_host() {
    let mut kept = HealthState::Healthy;
    // Three projects on one host see the outage at once; one swap wins.
    let raised = run(&mut kept, &Whose::Host, &down(Passing::Timeout), 3);
    assert_eq!(raised.len(), 1);
    assert_eq!(raised[0].what, Health::Unavailable(Passing::Timeout));
    assert_eq!(raised[0].whose, Whose::Host);
    // Later processes read the outage and raise nothing more.
    for _ in 0..4 {
        assert!(run(&mut kept, &Whose::Host, &down(Passing::Server), 2).is_empty());
    }
    let raised = run(&mut kept, &Whose::Host, &Ok(()), 3);
    assert_eq!(raised.len(), 1);
    assert_eq!(raised[0].what, Health::Recovered);
    assert!(run(&mut kept, &Whose::Host, &Ok(()), 3).is_empty());
    assert_eq!(kept, HealthState::Healthy);
}

#[test]
fn a_spent_pool_holds_every_project_its_holder_serves_and_reports_once() {
    let priced = pool(Pool::Priced);
    let mut kept = HealthState::Healthy;
    let raised = run(&mut kept, &priced, &out_of(Pool::Priced, NOW + 60), 2);
    assert_eq!(
        raised.iter().map(|n| n.what).collect::<Vec<_>>(),
        [Health::Spent {
            pool: Pool::Priced,
            until: at(NOW + 60)
        }]
    );
    assert!(raised.iter().all(|n| n.whose == priced));
    // A good call in the pool before `until` clears nothing.
    assert!(run(&mut kept, &priced, &Ok(()), 2).is_empty());
    assert_eq!(kept, spent(Pool::Priced, NOW + 60));
    // Another holder's pool on the host is not held.
    let person = Whose::Pool(Holder::Person(AccountId::new("42")), Pool::Priced);
    let mut theirs = HealthState::Healthy;
    assert!(run(&mut theirs, &person, &Ok(()), 1).is_empty());
    assert_eq!(theirs, HealthState::Healthy);
    // Past `until`, the first good call ends it, once.
    let raised = run_at(NOW + 60, &mut kept, &priced, &Ok(()), 3);
    assert_eq!(
        raised.iter().map(|n| n.what).collect::<Vec<_>>(),
        [Health::Recovered]
    );
    assert_eq!(kept, HealthState::Healthy);
}

#[test]
fn two_pools_of_one_holder_are_held_and_recover_apart() {
    let (priced, writes) = (pool(Pool::Priced), pool(Pool::Writes));
    let mut kept_priced = HealthState::Healthy;
    let mut kept_writes = HealthState::Healthy;
    let what = |raised: Vec<Notice>| raised.into_iter().map(|n| n.what).collect::<Vec<_>>();
    // Priced is spent until 100, then Writes until 50. Each pool's state
    // reads only its own refusal.
    let seen = out_of(Pool::Priced, 100);
    assert_eq!(
        what(run_at(10, &mut kept_priced, &priced, &seen, 2)),
        [Health::Spent {
            pool: Pool::Priced,
            until: at(100)
        }]
    );
    assert!(run_at(10, &mut kept_writes, &writes, &seen, 2).is_empty());
    let seen = out_of(Pool::Writes, 50);
    assert_eq!(
        what(run_at(20, &mut kept_writes, &writes, &seen, 2)),
        [Health::Spent {
            pool: Pool::Writes,
            until: at(50)
        }]
    );
    assert!(run_at(20, &mut kept_priced, &priced, &seen, 2).is_empty());
    assert_eq!(kept_priced, spent(Pool::Priced, 100));
    // A good Writes call at 60 recovers Writes alone.
    assert_eq!(
        what(run_at(60, &mut kept_writes, &writes, &Ok(()), 2)),
        [Health::Recovered]
    );
    assert_eq!(kept_writes, HealthState::Healthy);
    assert_eq!(kept_priced, spent(Pool::Priced, 100));
    // Priced stays held to 100; a good Priced call then recovers it.
    assert!(run_at(99, &mut kept_priced, &priced, &Ok(()), 2).is_empty());
    assert_eq!(
        what(run_at(100, &mut kept_priced, &priced, &Ok(()), 2)),
        [Health::Recovered]
    );
    assert_eq!(kept_priced, HealthState::Healthy);
}

#[test]
fn refusals_of_two_pools_in_turn_do_not_flap_one_state() {
    let (priced, writes) = (pool(Pool::Priced), pool(Pool::Writes));
    let mut kept_priced = HealthState::Healthy;
    let mut kept_writes = HealthState::Healthy;
    let mut raised = Vec::new();
    for round in 0..4 {
        for (p, until) in [
            (Pool::Priced, NOW + 60 + round),
            (Pool::Writes, NOW + 30 + round),
        ] {
            let seen = out_of(p, until);
            raised.extend(run(&mut kept_priced, &priced, &seen, 1));
            raised.extend(run(&mut kept_writes, &writes, &seen, 1));
        }
    }
    // One notice for each pool, on its first refusal; later ones move
    // `until` alone.
    assert_eq!(
        raised
            .iter()
            .map(|n| (&n.whose, n.what))
            .collect::<Vec<_>>(),
        [
            (
                &priced,
                Health::Spent {
                    pool: Pool::Priced,
                    until: at(NOW + 60)
                }
            ),
            (
                &writes,
                Health::Spent {
                    pool: Pool::Writes,
                    until: at(NOW + 30)
                }
            ),
        ]
    );
    assert_eq!(kept_priced, spent(Pool::Priced, NOW + 63));
    assert_eq!(kept_writes, spent(Pool::Writes, NOW + 33));
}

#[test]
fn a_sign_out_is_untouched_by_a_pools_refusal() {
    let mut kept = signed_out(NOW - 30);
    for p in [Pool::Calls, Pool::Priced, Pool::Search, Pool::Writes] {
        assert!(run(&mut kept, &ours(), &out_of(p, NOW + 60), 2).is_empty());
    }
    assert_eq!(kept, signed_out(NOW - 30));
}

#[test]
fn a_sign_out_and_its_end_are_reported_once() {
    let mut kept = HealthState::Healthy;
    let raised = run(&mut kept, &ours(), &Err(Outcome::SignedOut), 3);
    assert_eq!(raised.len(), 1);
    assert_eq!(raised[0].what, Health::SignedOut);
    assert_eq!(kept, signed_out(NOW));
    assert!(run(&mut kept, &ours(), &Err(Outcome::SignedOut), 3).is_empty());
    let raised = run(&mut kept, &ours(), &Ok(()), 3);
    assert_eq!(raised.len(), 1);
    assert_eq!(raised[0].what, Health::Recovered);
}

#[test]
fn a_ledger_that_cannot_be_read_still_gets_its_outcomes_notice() {
    // The ledger answers `LedgerDown`, so the adapter takes the state as
    // healthy and raises the notice its outcome calls for, as if it had won
    // the swap: an outage is never left unreported.
    let (_, notice) = next(
        &host(),
        &Whose::Host,
        Realm::CodeHost,
        at(NOW),
        &HealthState::Healthy,
        &down(Passing::Unreachable),
    );
    assert_eq!(
        notice.map(|n| n.what),
        Some(Health::Unavailable(Passing::Unreachable))
    );
    let (_, notice) = next(
        &host(),
        &ours(),
        Realm::CodeHost,
        at(NOW),
        &HealthState::Healthy,
        &Err(Outcome::SignedOut),
    );
    assert_eq!(notice.map(|n| n.what), Some(Health::SignedOut));
}

#[test]
fn next_is_a_function_of_its_inputs() {
    for whose in every_whose() {
        for was in [
            HealthState::Healthy,
            failing(1),
            spent(Pool::Writes, 5),
            spent(Pool::Priced, 5),
            signed_out(1),
        ] {
            for seen in every_seen() {
                let a = next(&host(), &whose, Realm::Tickets, at(NOW), &was, &seen);
                let b = next(&host(), &whose, Realm::Tickets, at(NOW), &was, &seen);
                assert_eq!(a, b);
            }
        }
    }
}
