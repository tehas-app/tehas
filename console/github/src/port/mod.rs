//! The ports of `tehas-ports`, answered by [`GitHub`] over the forge's
//! terms: one module a trait, so each is built and reviewed apart.
//! `tickets.rs` holds the tickets port's reads, `closed.rs` those of
//! closed tickets, and `writes.rs` its writes;
//! `identity.rs`, `code_host.rs`, `board.rs` and `blueprints.rs` hold their
//! ports' answers as each is built. `site.rs` is the `Site` that makes the
//! handles, and `anonymous.rs` the code host handle of `Actor::Anonymous`,
//! which reaches GitHub through `curl.rs` and never through gh. What every handle answers, its reports
//! and the names it cites by, is here. A method no caller asks yet answers
//! `Refused(Unsupported)` before any request.

mod anonymous;
mod blueprint_writes;
mod blueprints;
mod board;
mod closed;
mod code_host;
mod code_host_writes;
mod identity;
mod mergeable;
mod named;
mod people;
mod site;
mod tickets;
mod writes;

pub use code_host::{project_of, WORKFLOW_RUNS_FORBIDDEN};
pub(crate) use named::Named;
pub(crate) use people::People;
pub use people::{PeopleClient, Person, Trade};
pub use site::{GitHubSite, MakeRunner};

use crate::{table, GitHub};
use tehas_ports::{Cite, Cites, Got, Mark, Outcome, Ref, Refusal, Report, Reports, TicketKey};

/// The answer of a method no caller asks of this adapter yet.
pub(super) fn unsupported<T>() -> Got<T> {
    Err(Outcome::Refused(Refusal::Unsupported))
}

/// A failure of the read before a write: the write was not sent.
pub(crate) fn unsent(o: Outcome) -> Outcome {
    match o {
        Outcome::Unavailable { why, .. } => Outcome::Unavailable {
            why,
            sent: tehas_ports::Sent::No,
        },
        other => other,
    }
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
        cite(what)
    }

    fn link(&self, what: &Cite) -> Option<tehas_ports::WebLink> {
        link(&self.repo, what)
    }
}

/// The text that names `what` in a body written to GitHub.
pub(super) fn cite(what: &Cite) -> String {
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

/// The page GitHub shows an item of `repo` on, built with no request as
/// its read's `html_url` gives it: an issue's for a ticket or a blueprint,
/// a pull request's for a change. A key that is no number, an item of
/// another project, a mark and a marker link nothing, and so does every
/// cite of a repository that is not exactly `owner/name`. An account links
/// nothing either: a login does not say whether it is a person's or an
/// App's, whose page is `apps/<slug>`, so `github.com/<login>` could be
/// another account's, until a cite carries the account's kind.
pub(super) fn link(repo: &str, what: &Cite) -> Option<tehas_ports::WebLink> {
    let (owner, name) = code_host::parts(repo)?;
    let page = match what {
        Cite::Ticket(Ref::Here(TicketKey::Number(n))) => {
            format!("{owner}/{name}/issues/{}", n.get())
        }
        Cite::Blueprint(Ref::Here(n)) => format!("{owner}/{name}/issues/{}", n.get()),
        Cite::Change(Ref::Here(n)) => format!("{owner}/{name}/pull/{}", n.get()),
        _ => return None,
    };
    tehas_ports::WebLink::new(format!("https://github.com/{page}")).ok()
}
