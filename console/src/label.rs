//! Who applied a label. Authorisation is the account behind the label event,
//! not that the label is present (AGENTS.md rule 5), so every command that lets
//! a label decide something reads the same events the same way.
//!
//!   console label allowed <issue> <label>
//!     asks GitHub for the issue's events in TEHAS_REPO: 0 when <label> is on
//!     and the account whose application stands is one of
//!     TEHAS_ALLOWED_LABELERS, 1 when it is not, the label was taken off
//!     since, or no event applies it, 2 when the events cannot be read.
//!
//! Which issue to ask about, and what to do when its events cannot be read, is
//! the caller's. 64 is bad words.

use crate::forge::{gh_program, repo, repo_shaped, CALL_BOUND};
use std::env;
use std::num::NonZeroU64;
use tehas_core::events::{allowed_folded, mark_actor};
use tehas_ports::{HappenedKind, Mark, TicketKey, Tickets, Window, ALL_MAX};

const USAGE: &str = "usage: console label allowed <issue> <label>";

/// Whether an account of `list` put on `mark` as it stands on `issue`: 0
/// when one did, 1 when another account did, the mark was taken off since,
/// or none put it on, 2 when the issue is no number or its history could not
/// be read. A mark of None is one no event can carry.
pub fn allowed(tickets: &mut dyn Tickets, issue: &str, mark: Option<&Mark>, list: &str) -> u8 {
    let Some(key) = issue
        .parse::<u64>()
        .ok()
        .and_then(NonZeroU64::new)
        .map(TicketKey::Number)
    else {
        return 2;
    };
    let Some(mark) = mark else {
        return 1;
    };
    let only = [HappenedKind::Marked, HappenedKind::Unmarked];
    let read = tickets.history(&key, Window::Whole(ALL_MAX), Some(&only));
    let Ok(read) = read else {
        return 2;
    };
    match mark_actor(&read.items, mark) {
        Some(by) if allowed_folded(list, by) => 0,
        _ => 1,
    }
}

/// The tickets handle of this host: the repository's, through the
/// instance's gh. None when either is missing.
fn handle() -> Option<Box<dyn tehas_ports::TicketWrites>> {
    let repository = repo().filter(|r| repo_shaped(r))?;
    let path = env::var("PATH").unwrap_or_default();
    let home = env::var("TEHAS_HOME").ok();
    let program = gh_program(home.as_deref(), &path)?;
    Some(crate::wiring::tickets(&repository, &program, CALL_BOUND))
}

/// `console label allowed`, for a command of this program.
pub fn allowed_issue(issue: &str, mark: Option<&Mark>) -> u8 {
    if issue.parse::<u64>().is_err() {
        return 2;
    }
    let Some(mut tickets) = handle() else {
        return 2;
    };
    let list = env::var("TEHAS_ALLOWED_LABELERS").unwrap_or_default();
    allowed(&mut *tickets, issue, mark, &list)
}

pub fn main(given: &[String]) -> u8 {
    let words: Vec<&str> = given.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["allowed", issue, label] => allowed_issue(issue, crate::wiring::mark_of(label).as_ref()),
        _ => {
            eprintln!("{USAGE}");
            64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tehas_ports::fake::{FakeSite, Fault, Faulty, Op, TicketsOp, World};
    use tehas_ports::{Act, Actor, At, Caller, Happened, Scope, Site};

    fn ready() -> Mark {
        Mark::Act(Act::Approve)
    }

    fn ask(world: &World, issue: &str, mark: Option<&Mark>) -> u8 {
        let site = FakeSite::new(world);
        let mut t = site
            .tickets(&Actor::Ours(Scope::Full), Caller::new("label"))
            .unwrap();
        allowed(&mut *t, issue, mark, "alice bob")
    }

    /// Issue 7 with `ready` put on by each login in turn, a second apart.
    fn labelled(logins: &[&str]) -> World {
        World::new().ticket(7, |mut t| {
            for (i, login) in logins.iter().enumerate() {
                let at = At::new(1_760_000_000 + i as i64);
                t = t.event(Happened::Marked(ready()), Some(login), Some(at));
            }
            t
        })
    }

    #[test]
    fn the_label_counts_when_an_allowed_account_applied_it_most_recently() {
        assert_eq!(ask(&labelled(&["mallory", "bob"]), "7", Some(&ready())), 0);
    }

    #[test]
    fn it_does_not_when_the_last_account_is_not_allowed_or_no_event_applies_it() {
        assert_eq!(
            ask(&labelled(&["alice", "mallory"]), "7", Some(&ready())),
            1
        );
        assert_eq!(ask(&labelled(&[]), "7", Some(&ready())), 1);
        let other = Mark::Act(Act::Plan);
        assert_eq!(ask(&labelled(&["alice"]), "7", Some(&other)), 1);
        assert_eq!(ask(&labelled(&["alice"]), "7", None), 1);
    }

    #[test]
    fn a_login_of_the_list_counts_in_any_case() {
        let site = FakeSite::new(&labelled(&["mallory", "alice"]));
        let mut t = site
            .tickets(&Actor::Ours(Scope::Full), Caller::new("label"))
            .unwrap();
        assert_eq!(allowed(&mut *t, "7", Some(&ready()), "Alice bob"), 0);
        assert_eq!(allowed(&mut *t, "7", Some(&ready()), "ALICE"), 0);
        assert_eq!(allowed(&mut *t, "7", Some(&ready()), "alic"), 1);
    }

    #[test]
    fn a_removal_never_answers_who_applied_it() {
        let w = World::new().ticket(7, |t| {
            t.event(Happened::Marked(ready()), Some("mallory"), Some(At::new(1)))
                .event(Happened::Unmarked(ready()), Some("alice"), Some(At::new(2)))
        });
        assert_eq!(ask(&w, "7", Some(&ready())), 1);
    }

    /// The label an allowed account applied and someone took off since is
    /// on no more: it counts for nobody (T4.7). 1.4.x answered its applier.
    #[test]
    fn a_label_taken_off_since_counts_for_nobody() {
        let w = World::new().ticket(7, |t| {
            t.event(Happened::Marked(ready()), Some("alice"), Some(At::new(1)))
                .event(
                    Happened::Unmarked(ready()),
                    Some("mallory"),
                    Some(At::new(2)),
                )
        });
        assert_eq!(ask(&w, "7", Some(&ready())), 1);
    }

    #[test]
    fn an_event_that_names_no_account_is_no_allowed_one() {
        let w = World::new().ticket(7, |t| {
            t.event(Happened::Marked(ready()), None, Some(At::new(1)))
        });
        assert_eq!(ask(&w, "7", Some(&ready())), 1);
    }

    #[test]
    fn events_that_cannot_be_read_are_2_not_a_refusal() {
        let w = labelled(&["alice"]);
        let site =
            Faulty::new(FakeSite::new(&w)).on(Op::Tickets(TicketsOp::History), Fault::Timeout);
        let mut t = site
            .tickets(&Actor::Ours(Scope::Full), Caller::new("label"))
            .unwrap();
        assert_eq!(allowed(&mut *t, "7", Some(&ready()), "alice"), 2);
        assert_eq!(ask(&w, "8", Some(&ready())), 2);
        for bad in ["", "0", "seven", "-1"] {
            assert_eq!(ask(&w, bad, Some(&ready())), 2, "{bad}");
        }
    }
}
