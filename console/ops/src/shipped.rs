//! What closed an issue: the test that a merged pull request did, for
//! `console tidy run` and `tehasbot status`. The acceptance rule has one
//! statement here. Two commands act on it: tidy (src/tidy.rs) puts tehas:shipped
//! on in the edit that takes the state labels off, and tehasbot status
//! (console/src/queue.rs) lists the shipped issues and gives them the label, and the
//! two must not disagree.
//!
//!   console shipped page
//!     The page: how many issues one read may name, and the ceiling of the
//!     list a caller builds, in one number so they cannot drift apart.
//!
//! The rule is `by_merge`, over the closer the tickets port names and the
//! merged changes of this project it lists as closing the ticket. A closer
//! that is a change of this project speaks for itself, so `merged` false is
//! no merge whatever is listed. Any other closer, or none, lets the merged
//! changes listed decide: GitHub sometimes records the close with a commit
//! as the closer while it lists the merged pull request as closing the
//! issue (#947). tehasbot status reads with `since`, over
//! `Tickets::closed_since`; tidy reads by number with
//! `Tickets::several_with_closers`. Both carry the same closer and the same
//! merged changes, so the two answer alike for one ticket; a poll can still
//! disagree with a poll, since each reads at its own instant.
//!
//! A change of another project never decides: the port names a closer there
//! as `Closer::ChangeElsewhere`, with no `merged`, and lists only this
//! project's merged changes, so tehas:shipped means a change of this
//! project merged (Decided 79).
//!
//! Exit: 0, or 2 for usage.

use std::num::NonZeroU32;

use tehas_ports::{
    At, Bound, ChangeDigest, ChangeState, ClosedTicket, Closer, Got, Mark, State, TicketKey,
    Tickets,
};

// How many issues one read names: the bound of `since` and the most tidy
// asks of the closers' read, in the one number.
pub const PAGE: usize = 100;

// `PAGE` as the bound of a port's read.
const BOUND: NonZeroU32 = match NonZeroU32::new(PAGE as u32) {
    Some(n) => n,
    None => panic!("PAGE is above 0"),
};

// The label of a merge: the name tidy puts on in the edit that takes the
// state labels off, and the name tehasbot status gives from its own read. It is
// named here because both the comparison that earns it and the edit that writes
// it read this one word.
pub const SHIPPED: &str = "tehas:shipped";

/// The mark of a merge, in tehas's words: what `SHIPPED` is on GitHub.
pub const MARK: Mark = Mark::State(State::Shipped);

/// Whether a merged change of this project closed the ticket. A closer that
/// is a change of this project speaks for itself, so one not merged is no
/// merge whatever else is listed. Any other closer, or none, lets the
/// merged changes the site lists as closing it decide.
pub fn by_merge(closer: Option<&Closer>, listed: &[ChangeDigest]) -> bool {
    match closer {
        Some(Closer::Change { merged, .. }) => *merged,
        _ => listed
            .iter()
            .any(|c| matches!(c.state, ChangeState::Merged { .. })),
    }
}

/// The tickets closed at or after `since` that a merged change closed, most
/// recently closed first, every one of `controls` left out. Two closed at
/// the same instant keep the port's order. More closed tickets than `PAGE`
/// since then is the port's `Unread(Cut)`, and any read that failed is its
/// outcome: no answer, never an empty list.
pub fn since(
    tickets: &mut dyn Tickets,
    since: At,
    controls: &[TicketKey],
) -> Got<Vec<ClosedTicket>> {
    let mut kept: Vec<ClosedTicket> = tickets
        .closed_since(since, Bound::from(BOUND))?
        .into_items()
        .into_iter()
        .filter(|t| by_merge(t.closer.as_ref(), &t.merged))
        .filter(|t| !controls.contains(&t.digest.key))
        .collect();
    kept.sort_by_key(|t| std::cmp::Reverse(t.closed));
    Ok(kept)
}

/// The keys of the shipped tickets that do not carry `MARK`, in their order.
pub fn lacking_mark(shipped: &[ClosedTicket]) -> Vec<TicketKey> {
    shipped
        .iter()
        .filter(|t| !t.digest.marks.contains(&MARK))
        .map(|t| t.digest.key.clone())
        .collect()
}

pub fn main(given: &[String]) -> u8 {
    if given.iter().map(String::as_str).eq(["page"]) {
        tehas_core::outln!("{PAGE}");
        return 0;
    }
    eprintln!("console shipped: usage: page");
    2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_is_one_number_below_the_ceiling_of_a_read() {
        assert!(
            (1..=256).contains(&PAGE),
            "a read names no more issues than that"
        );
        assert_eq!(BOUND.get() as usize, PAGE);
    }

    #[test]
    fn the_verb_is_the_page() {
        let given = |w: &[&str]| w.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(main(&given(&["page"])), 0);
        assert_eq!(main(&given(&[])), 2);
        assert_eq!(main(&given(&["page", "x"])), 2);
        assert_eq!(main(&given(&["query"])), 2);
    }

    mod ports {
        //! The rule and the read on the ports, over the ports' fake: a
        //! ticket a merged change closed is shipped, by its closer first and
        //! by the merged changes listed as closing it when the closer names
        //! no change of this project.

        use super::super::*;
        use tehas_ports::fake::{FakeSite, Fault, Faulty, Op, TicketsOp, World};
        use tehas_ports::{
            Actor, Caller, ChangeNo, CloseReason, CommitId, ItemName, Outcome, ProjectName, Site,
            Unread, WebLink,
        };

        fn merged_state(w: &World) -> ChangeState {
            ChangeState::Merged {
                at: w.now(),
                by: None,
                commit: None,
            }
        }

        /// Ticket `n` closed by hand now, and change `n + 1` merged that
        /// closes it.
        fn shipped_at(w: World, n: u64) -> World {
            let state = merged_state(&w);
            w.ticket(n, |t| t.closed(CloseReason::Done))
                .change(n + 1, &format!("work/{n}"), |c| c.closes(n).state(state))
        }

        fn read(site: &impl Site, since: At, controls: &[TicketKey]) -> Got<Vec<ClosedTicket>> {
            let mut t = site
                .tickets(&Actor::Operator, Caller::new("shipped"))
                .unwrap();
            super::super::since(&mut *t, since, controls)
        }

        fn keys(got: &[ClosedTicket]) -> Vec<TicketKey> {
            got.iter().map(|t| t.digest.key.clone()).collect()
        }

        /// The merged changes the fake lists as closing a shipped ticket,
        /// to give the rule as a site gives them.
        fn listed() -> Vec<ChangeDigest> {
            let w = shipped_at(World::new(), 7);
            let got = read(&FakeSite::new(&w), At::new(0), &[]).unwrap();
            assert_eq!(got.len(), 1);
            assert!(!got[0].merged.is_empty());
            got[0].merged.clone()
        }

        #[test]
        fn a_change_closer_speaks_for_itself_and_any_other_lets_the_list_decide() {
            let listed = listed();
            let no = ChangeNo::new(8).unwrap();
            let merged = Closer::Change { no, merged: true };
            let unmerged = Closer::Change { no, merged: false };
            assert!(by_merge(Some(&merged), &[]));
            assert!(!by_merge(Some(&unmerged), &listed));
            assert!(!by_merge(Some(&unmerged), &[]));
            let commit = Closer::Commit(CommitId::new("abc1234").unwrap());
            let elsewhere = Closer::change_elsewhere(
                ProjectName::new("acme/other"),
                ItemName::new("3"),
                WebLink::new("https://example.test/acme/other/pull/3").unwrap(),
            );
            for closer in [
                None,
                Some(&commit),
                Some(&elsewhere),
                Some(&Closer::Unknown),
            ] {
                assert!(by_merge(closer, &listed), "{closer:?} with a merge listed");
                assert!(!by_merge(closer, &[]), "{closer:?} with none listed");
            }
            // A change listed that is not merged is no merge.
            let mut open = listed.clone();
            open[0].state = ChangeState::Open;
            assert!(!by_merge(None, &open));
            let mut closed = listed;
            closed[0].state = ChangeState::Closed { at: At::new(0) };
            assert!(!by_merge(None, &closed));
        }

        #[test]
        fn the_read_keeps_the_shipped_most_recently_closed_first_without_the_controls() {
            let w = shipped_at(World::new(), 1);
            let since = w.now();
            let w = shipped_at(w, 7);
            w.advance(10);
            let w = w.ticket(9, |t| t.closed(CloseReason::Done));
            w.advance(10);
            let w = shipped_at(w, 11);
            let site = FakeSite::new(&w);
            let control = w.key(1);
            assert_eq!(
                keys(&read(&site, since, std::slice::from_ref(&control)).unwrap()),
                vec![w.key(11), w.key(7)]
            );
            assert_eq!(
                keys(&read(&site, since, &[]).unwrap()),
                vec![w.key(11), w.key(1), w.key(7)]
            );
        }

        #[test]
        fn a_ticket_closed_before_the_time_asked_is_left_out() {
            let w = shipped_at(World::new(), 7);
            w.advance(10);
            let since = w.now();
            let w = shipped_at(w, 11);
            assert_eq!(
                keys(&read(&FakeSite::new(&w), since, &[]).unwrap()),
                vec![w.key(11)]
            );
        }

        #[test]
        fn a_read_that_failed_is_its_outcome_and_never_an_empty_list() {
            let w = shipped_at(World::new(), 7);
            let site = Faulty::new(FakeSite::new(&w))
                .on(Op::Tickets(TicketsOp::ClosedSince), Fault::Unreachable);
            let got = read(&site, At::new(0), &[]);
            assert!(matches!(got, Err(Outcome::Unavailable { .. })), "{got:?}");
        }

        #[test]
        fn more_closed_than_a_page_is_cut_and_not_a_shorter_list() {
            let mut w = World::new();
            for n in 1..=PAGE as u64 + 1 {
                w = w.ticket(n, |t| t.closed(CloseReason::Done));
            }
            assert_eq!(
                read(&FakeSite::new(&w), At::new(0), &[]),
                Err(Outcome::Unread(Unread::Cut))
            );
        }

        #[test]
        fn the_tickets_lacking_the_mark_are_the_shipped_without_it() {
            let state = merged_state(&World::new());
            let w = World::new()
                .ticket(7, |t| t.mark(MARK).closed(CloseReason::Done))
                .change(8, "work/7", |c| c.closes(7).state(state));
            let w = shipped_at(w, 11);
            let got = read(&FakeSite::new(&w), At::new(0), &[]).unwrap();
            assert_eq!(keys(&got).len(), 2);
            assert_eq!(lacking_mark(&got), vec![w.key(11)]);
            assert_eq!(lacking_mark(&[]), Vec::<TicketKey>::new());
        }
    }
}
