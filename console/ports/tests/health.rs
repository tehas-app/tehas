//! `health::next` as a table: each state and outcome, for the host and for
//! a holder; a notice only on a change of state; an outage and its end,
//! a spent pool and a sign-out, each reported once for every project on
//! the host.

use tehas_ports::*;

fn host() -> HostId {
    HostId::new("code-host")
}

fn ours() -> Whose {
    Whose::Holder(Holder::Ours(HolderId::new("grant-1")))
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

fn down(why: Passing) -> Result<(), Outcome> {
    Err(Outcome::Unavailable {
        why,
        sent: Sent::No,
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
    let priced = spent(Pool::Priced, NOW + 60);
    let past = spent(Pool::Priced, NOW);
    let gone = signed_out(NOW - 30);
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
        (
            up,
            Err(Outcome::SignedOut),
            signed_out(NOW),
            Some(Health::SignedOut),
        ),
        // An outage is the host's, not the holder's.
        (up, down(Passing::Timeout), up, None),
        (up, Err(Outcome::Unread(Unread::Cut)), up, None),
        // Spent: a good call before `until` was in another pool.
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
        // The same pool again moves `until`, with no notice; another pool
        // is a change.
        (
            priced,
            out_of(Pool::Priced, NOW + 120),
            spent(Pool::Priced, NOW + 120),
            None,
        ),
        (
            priced,
            out_of(Pool::Search, NOW + 30),
            spent(Pool::Search, NOW + 30),
            Some(Health::Spent {
                pool: Pool::Search,
                until: at(NOW + 30),
            }),
        ),
        (
            priced,
            Err(Outcome::SignedOut),
            signed_out(NOW),
            Some(Health::SignedOut),
        ),
        (priced, down(Passing::Server), priced, None),
        // Signed out: it never blocks; the next good call clears it.
        (gone, Err(Outcome::SignedOut), gone, None),
        (gone, Ok(()), up, Some(Health::Recovered)),
        (gone, Err(Outcome::Forbidden), up, Some(Health::Recovered)),
        (
            gone,
            out_of(Pool::Calls, NOW + 9),
            spent(Pool::Calls, NOW + 9),
            Some(Health::Spent {
                pool: Pool::Calls,
                until: at(NOW + 9),
            }),
        ),
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
fn a_state_of_the_other_kind_reads_as_healthy() {
    for seen in every_seen() {
        let (host_new, _) = step(&Whose::Host, &HealthState::Healthy, &seen);
        for was in [spent(Pool::Priced, NOW + 60), signed_out(NOW - 1)] {
            assert_eq!(
                step(&Whose::Host, &was, &seen),
                step(&Whose::Host, &HealthState::Healthy, &seen),
                "{was:?} {seen:?}"
            );
            assert_ne!(host_new, was);
        }
        assert_eq!(
            step(&ours(), &failing(NOW - 1), &seen),
            step(&ours(), &HealthState::Healthy, &seen),
            "{seen:?}"
        );
    }
}

#[test]
fn a_notice_is_raised_only_on_a_change_of_state() {
    let states = [
        HealthState::Healthy,
        failing(NOW - 30),
        spent(Pool::Priced, NOW + 60),
        spent(Pool::Priced, NOW - 1),
        signed_out(NOW - 30),
    ];
    for whose in [Whose::Host, ours()] {
        for was in states {
            for seen in every_seen() {
                let (new, notice) = step(&whose, &was, &seen);
                let was = match (&whose, was) {
                    (Whose::Host, HealthState::Spent { .. } | HealthState::SignedOut { .. })
                    | (Whose::Holder(_), HealthState::Failing { .. }) => HealthState::Healthy,
                    (_, was) => was,
                };
                let change = match (was, new) {
                    (HealthState::Spent { pool: a, .. }, HealthState::Spent { pool: b, .. }) => {
                        a != b
                    }
                    _ => std::mem::discriminant(&was) != std::mem::discriminant(&new),
                };
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
    let read = *kept;
    let mut raised = Vec::new();
    for project in 0..racing {
        let realm = if project % 2 == 0 {
            Realm::CodeHost
        } else {
            Realm::Tickets
        };
        let (new, notice) = next(&host(), whose, realm, at(NOW), &read, seen);
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
    let mut kept = HealthState::Healthy;
    let raised = run(&mut kept, &ours(), &out_of(Pool::Priced, NOW + 60), 2);
    assert_eq!(
        raised.iter().map(|n| n.what).collect::<Vec<_>>(),
        [Health::Spent {
            pool: Pool::Priced,
            until: at(NOW + 60)
        }]
    );
    // A write in another pool before `until` goes on and clears nothing.
    assert!(run(&mut kept, &ours(), &Ok(()), 2).is_empty());
    assert_eq!(kept, spent(Pool::Priced, NOW + 60));
    // Another holder on the host is not held.
    let person = Whose::Holder(Holder::Person(AccountId::new("42")));
    let mut theirs = HealthState::Healthy;
    assert!(run(&mut theirs, &person, &Ok(()), 1).is_empty());
    assert_eq!(theirs, HealthState::Healthy);
    // Past `until`, the first good call ends it, once.
    let (new, notice) = next(
        &host(),
        &ours(),
        Realm::CodeHost,
        at(NOW + 60),
        &kept,
        &Ok(()),
    );
    assert_eq!(new, HealthState::Healthy);
    assert_eq!(notice.map(|n| n.what), Some(Health::Recovered));
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
    for whose in [Whose::Host, ours()] {
        for was in [HealthState::Healthy, failing(1), spent(Pool::Writes, 5)] {
            for seen in every_seen() {
                let a = next(&host(), &whose, Realm::Tickets, at(NOW), &was, &seen);
                let b = next(&host(), &whose, Realm::Tickets, at(NOW), &was, &seen);
                assert_eq!(a, b);
            }
        }
    }
}
