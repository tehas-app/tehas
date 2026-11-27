//! What GitHub says about one pull request by itself, read as the sweep reads
//! it. A removal goes by this, so an answer that does not say is `Unclear`.
//! The one question goes on the code-host port: the sweep reads the change's
//! typed digest, never a field name of the supplier.

use tehas_ports::{
    Bound, ChangeDigest, ChangeFilter, ChangeNo, ChangeState, ChangeStates, CodeHost, Relation,
};

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Pull {
    /// Open, its head in this repository, opened by one of ours.
    Keep,
    Closed,
    /// No such pull request.
    Gone,
    /// Open with a head that is not in this repository, a deleted fork included.
    Elsewhere,
    /// Open with a head here, and the answer names someone who is not ours.
    Outside,
    /// An answer that must not be acted on.
    Unclear,
}

/// The one question of the sweep, asked of the code-host port: the change
/// answers it, the sweep reads the digest. A read that fails, or one whose
/// answer is no whole answer, is `Unclear`: it keeps the preview, as no
/// answer from GitHub did before the port.
pub fn ask(host: &mut Option<Box<dyn CodeHost + Send>>, n: u64, bot: &str) -> Pull {
    let Some(host) = host.as_mut() else {
        return Pull::Unclear;
    };
    let Some(no) = ChangeNo::new(n) else {
        return Pull::Unclear;
    };
    let filter = ChangeFilter {
        states: ChangeStates::All,
        numbers: vec![no],
        ..Default::default()
    };
    let Some(bound) = Bound::new(1) else {
        return Pull::Unclear;
    };
    match host.changes(&filter, bound) {
        Ok(digests) => match digests.into_items().into_iter().find(|d| d.no == no) {
            Some(d) => decide(&d, bot),
            // The site answered and names no such change: it is gone. The
            // sweep has just checked that GitHub shows the repository at
            // all, so this is not a token that lost the project.
            None => Pull::Gone,
        },
        Err(_) => Pull::Unclear,
    }
}

/// Closed and merged are closed: the REST read this replaces saw GitHub's
/// `closed` state, which a merged pull request answers with too, and swept
/// it. An open one of ours is kept; a head in another project,
/// `Elsewhere`, whoever opened it.
fn decide(d: &ChangeDigest, bot: &str) -> Pull {
    judge(
        &d.state,
        d.from_fork,
        d.author.as_ref().map(|a| a.login.as_str()),
        d.author_relation,
        bot,
    )
}

/// The digest read down to what the verdict asks: `judge` is what the sweep
/// decides, and the table below holds it directly.
fn judge(
    state: &ChangeState,
    from_fork: bool,
    author: Option<&str>,
    relation: Option<Relation>,
    bot: &str,
) -> Pull {
    match state {
        ChangeState::Open => {}
        _ => return Pull::Closed,
    }
    if from_fork {
        return Pull::Elsewhere;
    }
    // The factory's own App, as `by_us` in preview.rs reads it.
    if let Some(a) = author {
        if tehas_core::accounts::factorys(a, bot) {
            return Pull::Keep;
        }
    }
    match relation {
        Some(Relation::Owner | Relation::Member | Relation::Collaborator) => Pull::Keep,
        Some(_) => Pull::Outside,
        None => Pull::Unclear,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tehas_ports::fake::{CodeHostOp, FakeSite, Fault, Faulty, Op, World};
    use tehas_ports::{Actor, At, Caller, Relation, Scope, Site};

    const BOT: &str = "";

    /// An open change's verdict from the four fields that decide it.
    fn open(from_fork: bool, author: Option<&str>, relation: Option<Relation>) -> Pull {
        judge(&ChangeState::Open, from_fork, author, relation, BOT)
    }

    #[test]
    fn a_closed_and_a_merged_change_are_both_closed() {
        assert_eq!(
            judge(
                &ChangeState::Closed { at: At::new(9) },
                false,
                None,
                None,
                BOT
            ),
            Pull::Closed
        );
        // GitHub answers a merged pull request with its `closed` state, and
        // the REST read this replaces swept it: a merge is closed.
        assert_eq!(
            judge(
                &ChangeState::Merged {
                    at: At::new(9),
                    by: None,
                    commit: None
                },
                false,
                None,
                None,
                BOT
            ),
            Pull::Closed
        );
    }

    #[test]
    fn an_open_change_of_ours_is_kept_by_relation_or_by_the_apps_login() {
        for relation in [Relation::Owner, Relation::Member, Relation::Collaborator] {
            assert_eq!(open(false, Some("alice"), Some(relation)), Pull::Keep);
        }
        // The factory's own App, whatever its relation says.
        assert_eq!(
            judge(
                &ChangeState::Open,
                false,
                Some("acme-app[bot]"),
                Some(Relation::Outside),
                "acme-app[bot]"
            ),
            Pull::Keep
        );
        // Only the factory's App, not another bot.
        assert_eq!(
            judge(
                &ChangeState::Open,
                false,
                Some("acme-app[bot]"),
                Some(Relation::Outside),
                "other-app[bot]"
            ),
            Pull::Outside
        );
    }

    #[test]
    fn an_outsider_is_outside_and_no_relation_is_unclear() {
        for relation in [Relation::Outside, Relation::Unknown] {
            assert_eq!(open(false, Some("bob"), Some(relation)), Pull::Outside);
        }
        assert_eq!(open(false, Some("bob"), None), Pull::Unclear);
        assert_eq!(open(false, None, None), Pull::Unclear);
    }

    #[test]
    fn a_head_elsewhere_is_elsewhere_whoever_opened_it() {
        assert_eq!(open(true, None, None), Pull::Elsewhere);
        // The App's, but from a fork: never ours. The fake never answers a
        // fork, so the judge holds this line and the adapter's own tests
        // read isCrossRepository into it.
        assert_eq!(
            judge(
                &ChangeState::Open,
                true,
                Some("acme-app[bot]"),
                Some(Relation::Member),
                "acme-app[bot]"
            ),
            Pull::Elsewhere
        );
    }

    fn asked(world: &World) -> Pull {
        let host = FakeSite::new(world)
            .code_host(&Actor::Ours(Scope::Full), Caller::new("preview"))
            .unwrap();
        ask(&mut Some(host), 12, BOT)
    }

    #[test]
    fn the_site_answers_the_one_question_and_its_absence_is_gone() {
        let member =
            World::new()
                .relation("alice", Relation::Member)
                .change(12, "feature/x", |c| c.author("alice"));
        assert_eq!(asked(&member), Pull::Keep);
        let outsider = World::new().change(12, "feature/x", |c| c.author("bob"));
        assert_eq!(asked(&outsider), Pull::Outside);
        let closed = World::new().change(12, "feature/x", |c| {
            c.state(ChangeState::Closed { at: At::new(9) })
        });
        assert_eq!(asked(&closed), Pull::Closed);
        let merged = World::new().change(12, "feature/x", |c| {
            c.state(ChangeState::Merged {
                at: At::new(9),
                by: None,
                commit: None,
            })
        });
        assert_eq!(asked(&merged), Pull::Closed);
        // The site answered and named no such change.
        assert_eq!(asked(&World::new()), Pull::Gone);
    }

    #[test]
    fn a_failed_read_and_a_host_that_cannot_be_wired_both_keep() {
        let world = World::new()
            .relation("alice", Relation::Member)
            .change(12, "feature/x", |c| c.author("alice"));
        let site = Faulty::new(FakeSite::new(&world))
            .on(Op::CodeHost(CodeHostOp::Changes), Fault::Unreachable);
        let host = site
            .code_host(&Actor::Ours(Scope::Full), Caller::new("preview"))
            .unwrap();
        assert_eq!(ask(&mut Some(host), 12, BOT), Pull::Unclear);
        // No handles at all: the run keeps what it cannot ask about.
        assert_eq!(ask(&mut None, 12, BOT), Pull::Unclear);
    }
}
