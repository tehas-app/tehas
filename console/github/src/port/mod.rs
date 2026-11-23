//! The ports of `tehas-ports`, answered by [`GitHub`] over the forge's
//! terms: one module a trait, so each is built and reviewed apart.
//! `tickets.rs` holds the tickets port's reads and `writes.rs` its writes;
//! `identity.rs`, `code_host.rs`, `board.rs` and `blueprints.rs` hold their
//! ports' answers as each is built. What every handle answers, its reports
//! and the names it cites by, is here. A method no caller asks yet answers
//! `Refused(Unsupported)` before any request.

mod blueprints;
mod board;
mod code_host;
mod identity;
mod tickets;
mod writes;

use crate::{table, GitHub};
use tehas_ports::{Cite, Cites, Got, Mark, Outcome, Ref, Refusal, Report, Reports, TicketKey};

/// The answer of a method no caller asks of this adapter yet.
pub(super) fn unsupported<T>() -> Got<T> {
    Err(Outcome::Refused(Refusal::Unsupported))
}

/// The number of a ticket that keeps its rule, or `Refused(Invalid)`.
pub(super) fn number(key: &TicketKey) -> Got<u64> {
    match key {
        TicketKey::Number(n) => Ok(n.get()),
        TicketKey::Key(_) => Err(Outcome::Refused(Refusal::Invalid)),
    }
}

/// The label of a mark this adapter writes, or `Refused(Invalid)` for one
/// no label can carry.
pub(super) fn label_of(mark: &Mark) -> Got<String> {
    table::label(mark)
        .filter(|l| table::is_label(l))
        .ok_or(Outcome::Refused(Refusal::Invalid))
}

impl Reports for GitHub {
    /// The adapter keeps no report of its own yet: a failed call answers its
    /// outcome to the caller.
    fn take_reports(&mut self) -> Vec<Report> {
        Vec::new()
    }
}

impl Cites for GitHub {
    fn cite(&self, what: &Cite) -> String {
        fn named<T>(r: &Ref<T>, here: impl Fn(&T) -> String) -> String {
            match r {
                Ref::Here(t) => format!("#{}", here(t)),
                Ref::Elsewhere { project, item } => {
                    format!("{}#{}", project.as_str(), item.as_str())
                }
            }
        }
        match what {
            Cite::Ticket(r) => named(r, |k| match k {
                TicketKey::Number(n) => n.get().to_string(),
                TicketKey::Key(k) => k.as_str().to_string(),
            }),
            Cite::Blueprint(r) => named(r, |n| n.get().to_string()),
            Cite::Change(r) => named(r, |n| n.get().to_string()),
            Cite::Account(login) => format!("@{}", login.as_str()),
            Cite::Marker(m) => table::cite_marker(m),
            Cite::Mark(m) => table::cite_mark(m),
            // A cite this copy does not know names nothing rather than a
            // wrong item.
            _ => String::new(),
        }
    }
}
