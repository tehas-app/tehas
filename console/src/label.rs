//! Who applied a label. Authorisation is the account behind the label event,
//! not that the label is present (AGENTS.md rule 5), so every command that lets
//! a label decide something reads the same events the same way.
//!
//!   console label actor <label>
//!     stdin: one issue's events, as gh api repos/<repo>/issues/<n>/events
//!     ?per_page=100 --paginate answers them, one JSON array per page. stdout:
//!     "<login>\t<time>" of the labeled event carrying <label> with the latest
//!     time: newest by its own stamp, not by the order GitHub handed the pages
//!     back, except that events sharing one stamp keep that page order and an
//!     event with no stamp is older than any stamped one. An empty pair when no
//!     event does: an event that names no account or no time leaves that field
//!     empty. 1 when the events are not what this expects, which is never the
//!     same as having no event.
//!   console label allowed <issue> <label>
//!     asks GitHub for the issue's events in TEHAS_REPO: 0 when the account
//!     that applied <label> most recently is one of TEHAS_ALLOWED_LABELERS, 1
//!     when it is not or no event applies it, 2 when the events cannot be read.
//!   console label pause-holds
//!     stdin: the events of the control issue, as above. The pause label works
//!     one way: anyone who can label may stop the factory, only an allowed
//!     account may start it again. 0 when the tehas:pause event with the latest
//!     stamp holds the host paused: a removal by an account outside
//!     TEHAS_ALLOWED_LABELERS or by an event that names no account, or an
//!     application no event has taken back. The label is gone from the issue
//!     all the same when the label object was deleted or renamed, and no
//!     account was asked to lift it. stdout is "<event>\t<account>". 1 when
//!     nothing the events say holds the host paused: no tehas:pause event, or
//!     its last removal was by an allowed account. 2 when the events are not
//!     what this expects. Whether the label is on the issue now is not this
//!     command's to see: the caller reads the issue.
//!
//! Which issue to ask about, and what to do when its events cannot be read, is
//! the caller's. 64 is bad words.

use crate::forge::{gh_program, repo, repo_shaped, CALL_BOUND};
use std::env;
use std::io::Read;
use tehas_core::events::{allowed_in, label_actor, pause_holds, Event};
use tehas_core::forge::{read_events, Bounded, Forge, GitHub};

const USAGE: &str = "usage: console label actor <label> | allowed <issue> <label> | pause-holds";

fn allowed(login: &str) -> bool {
    allowed_in(
        &env::var("TEHAS_ALLOWED_LABELERS").unwrap_or_default(),
        login,
    )
}

// Standard input as jq -s reads it: nothing at all is no events.
fn events_given() -> Option<Vec<Event>> {
    let mut text = String::new();
    std::io::stdin().read_to_string(&mut text).ok()?;
    if text.trim().is_empty() {
        return Some(Vec::new());
    }
    read_events(&text).ok()
}

/// `console label allowed`, for a command of this program.
pub fn allowed_issue(issue: &str, label: &str) -> u8 {
    let Ok(number) = issue.parse::<u64>() else {
        return 2;
    };
    let Some(repository) = repo().filter(|r| repo_shaped(r)) else {
        return 2;
    };
    let path = env::var("PATH").unwrap_or_default();
    let home = env::var("TEHAS_HOME").ok();
    let Some(program) = gh_program(home.as_deref(), &path) else {
        return 2;
    };
    let runner = Box::new(Bounded::new(program, CALL_BOUND));
    match GitHub::new(repository, runner).applied_by(number, label) {
        Ok(Some(applied)) if allowed(&applied.actor) => 0,
        Ok(_) => 1,
        Err(_) => 2,
    }
}

fn holds(events: Option<Vec<Event>>) -> (u8, String) {
    let Some(events) = events else {
        return (2, String::new());
    };
    match pause_holds(&events, &allowed) {
        Some((event, by)) => (0, format!("{event}\t{by}\n")),
        None => (1, String::new()),
    }
}

fn actor(events: Option<Vec<Event>>, label: &str) -> (u8, String) {
    let Some(events) = events else {
        return (1, String::new());
    };
    let (who, at) = label_actor(&events, label);
    (0, format!("{who}\t{at}\n"))
}

pub fn main(given: &[String]) -> u8 {
    let words: Vec<&str> = given.iter().map(String::as_str).collect();
    let (rc, out) = match words.as_slice() {
        ["actor", label] => actor(events_given(), label),
        ["allowed", issue, label] => (allowed_issue(issue, label), String::new()),
        ["pause-holds"] => holds(events_given()),
        _ => {
            eprintln!("{USAGE}");
            return 64;
        }
    };
    print!("{out}");
    rc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn events(text: &str) -> Option<Vec<Event>> {
        read_events(text).ok()
    }

    #[test]
    fn the_actor_is_the_latest_stamp_whatever_order_the_pages_came_in() {
        let late =
            r#"{"event":"labeled","label":{"name":"l"},"actor":{"login":"new"},"created_at":"2"}"#;
        let early =
            r#"{"event":"labeled","label":{"name":"l"},"actor":{"login":"old"},"created_at":"1"}"#;
        let (rc, out) = actor(events(&format!("[{late}][{early}]")), "l");
        assert_eq!((rc, out.as_str()), (0, "new\t2\n"));
    }

    #[test]
    fn a_removal_does_not_answer_who_applied_a_label() {
        let on =
            r#"{"event":"labeled","label":{"name":"l"},"actor":{"login":"a"},"created_at":"1"}"#;
        let off =
            r#"{"event":"unlabeled","label":{"name":"l"},"actor":{"login":"b"},"created_at":"2"}"#;
        let (_, out) = actor(events(&format!("[{on},{off}]")), "l");
        assert_eq!(out, "a\t1\n");
    }

    #[test]
    fn no_event_is_an_empty_pair_and_unreadable_events_are_a_failure() {
        assert_eq!(actor(events("[]"), "l"), (0, "\t\n".to_string()));
        assert_eq!(actor(events(r#"{"message":"no"}"#), "l").0, 1);
        assert_eq!(actor(events("[][1]"), "l").0, 1);
    }

    #[test]
    fn the_pause_status_reads_the_events_or_says_it_could_not() {
        assert_eq!(holds(events("[]")).0, 1);
        assert_eq!(holds(events(r#"{"message":"no"}"#)).0, 2);
        let on = r#"[{"event":"labeled","label":{"name":"tehas:pause"},"actor":{"login":"a"}}]"#;
        assert_eq!(holds(events(on)), (0, "labeled\ta\n".to_string()));
    }
}
