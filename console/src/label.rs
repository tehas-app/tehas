//! Who applied a label. Authorisation is the account behind the label event,
//! not that the label is present (AGENTS.md rule 5), so every command that lets
//! a label decide something reads the same events the same way.
//!
//!   console label allowed <issue> <label>
//!     asks GitHub for the issue's events in TEHAS_REPO: 0 when the account
//!     that applied <label> most recently is one of TEHAS_ALLOWED_LABELERS, 1
//!     when it is not or no event applies it, 2 when the events cannot be read.
//!
//! Which issue to ask about, and what to do when its events cannot be read, is
//! the caller's. 64 is bad words.

use crate::forge::{gh_program, repo, repo_shaped, CALL_BOUND};
use std::env;
use tehas_core::events::allowed_in;
use tehas_core::forge::{Bounded, Forge, GitHub};

const USAGE: &str = "usage: console label allowed <issue> <label>";

fn allowed(login: &str) -> bool {
    allowed_in(
        &env::var("TEHAS_ALLOWED_LABELERS").unwrap_or_default(),
        login,
    )
}

fn is_allowed(issue: &str, label: &str) -> u8 {
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

pub fn main(given: &[String]) -> u8 {
    let words: Vec<&str> = given.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["allowed", issue, label] => is_allowed(issue, label),
        _ => {
            eprintln!("{USAGE}");
            64
        }
    }
}
