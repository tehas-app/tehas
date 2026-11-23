//! The named outcomes: `Outcome` and its parts, `Got`, `Did`, `Whole`,
//! `Took`, `Applied`. A site's status, error text and field names never
//! leave the adapter.

use serde::{Deserialize, Serialize};

use crate::At;

/// A value, or the one outcome that stopped it.
pub type Got<T> = Result<T, Outcome>;
/// Done, or the one outcome that stopped it.
pub type Did = Result<(), Outcome>;

/// A list the adapter vouches is all there is.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Whole<T>(Vec<T>);

impl<T> Default for Whole<T> {
    /// Nothing.
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<T> Whole<T> {
    /// The whole list. Only an adapter or a fake that read it all says so.
    pub fn new(items: Vec<T>) -> Self {
        Self(items)
    }

    /// The items.
    pub fn items(&self) -> &[T] {
        &self.0
    }

    /// The items, owned.
    pub fn into_items(self) -> Vec<T> {
        self.0
    }
}

/// The first n of a list in the site's order, or the newest n, each item
/// whole. `more` is true exactly when the site holds more than n; a site
/// that cannot tell answers `Unread(Inconsistent)`, never a guess. Read with
/// `Window::Whole` or `Depth::All`, `more` is false.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Took<T> {
    /// The items taken.
    pub items: Vec<T>,
    /// The site holds more than were taken.
    pub more: bool,
}

impl<T> Default for Took<T> {
    /// Nothing taken, and no more.
    fn default() -> Self {
        Self {
            items: Vec::new(),
            more: false,
        }
    }
}

/// What stopped a call. Exhaustive on purpose: a new outcome is a major
/// change of the ports.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Outcome {
    /// Not there, or not visible to this actor.
    NotFound,
    /// A conditional write lost: read again, then decide.
    Conflict(Lost),
    /// The site will not take this write until an input changes. Remember
    /// it; do not retry.
    Refused(Refusal),
    /// This actor may not do this. Never widen the actor in reply.
    Forbidden,
    /// The credential is no longer good.
    SignedOut,
    /// The budget of this holder and pool is out until then. Calls of the
    /// same holder in the pool fail at once, with no request, until `until`.
    Spent {
        /// The pool that is out.
        pool: Pool,
        /// When it has budget again.
        until: At,
    },
    /// A passing failure. For a write, whether it may have reached the site.
    Unavailable {
        /// What passed.
        why: Passing,
        /// Whether the request went out.
        sent: Sent,
    },
    /// The answer could not be read whole. Fail closed: read as changed,
    /// held or unknown, never as empty.
    Unread(Unread),
}

/// How a conditional write lost. `Gone` only for the thing a conditional
/// write named (the comment, the branch), deleted between the read and the
/// write. A write to a thing that was never there, or is not visible, is
/// `NotFound`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Lost {
    /// It changed since it was read.
    Changed,
    /// It was deleted since it was read.
    Gone,
    /// The branch's head moved since it was read.
    HeadMoved,
    /// It moved to another project.
    Elsewhere,
}

/// Why the site will not take a write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Refusal {
    /// An input does not keep its rule.
    Invalid,
    /// It would make a cycle of links.
    Cycle,
    /// The change cannot be merged as it stands.
    NotMergeable,
    /// It is there already, in another form.
    Exists,
    /// The site or this adapter does not do it.
    Unsupported,
    /// A read-only host's handle made no request.
    ReadOnly,
    /// A refusal this copy does not know.
    #[serde(other)]
    Unknown,
}

/// Which budget a call draws on. Which pool a call draws on is the
/// adapter's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Pool {
    /// Reads counted one per call.
    Calls,
    /// Reads priced by how much they ask.
    Priced,
    /// Searches.
    Search,
    /// Writes.
    Writes,
    /// A pool this copy does not know.
    #[serde(other)]
    Unknown,
}

/// A failure that passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Passing {
    /// No answer in time.
    Timeout,
    /// The site could not be reached.
    Unreachable,
    /// The site failed on its side.
    Server,
    /// The site answered, readably, that the work the call asked about
    /// ended in error by the site's own word: a check run or a job that
    /// errored on the site's side. Not a fault of the answer, which reads
    /// whole: an answer that does not parse is `Unread(Malformed)`.
    Broken,
}

/// Whether a request went out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Sent {
    /// It did not go out.
    No,
    /// The request went out and no answer came. Never sent again on this
    /// answer; a read settles it.
    Maybe,
}

/// Why an answer could not be read whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Unread {
    /// More than the bound.
    Cut,
    /// Parts of the answer disagree, or the site cannot say if there is
    /// more.
    Inconsistent,
    /// An item too large to give whole.
    TooLarge,
    /// An answer that does not parse, or was cut off in transit. Never
    /// `Unavailable`: it fails closed.
    Malformed,
    /// A reason this copy does not know. Handled as any `Unread`.
    #[serde(other)]
    Unknown,
}

impl Outcome {
    /// One fixed line in our words for the report. Never the site's text,
    /// which may name a host or carry a token.
    pub fn line(&self) -> &'static str {
        match self {
            Outcome::NotFound => "not found, or not visible to this actor",
            Outcome::Conflict(Lost::Changed) => "changed since it was read",
            Outcome::Conflict(Lost::Gone) => "deleted since it was read",
            Outcome::Conflict(Lost::HeadMoved) => "the branch moved since it was read",
            Outcome::Conflict(Lost::Elsewhere) => "moved to another project",
            Outcome::Refused(r) => match r {
                Refusal::Invalid => "refused: an input is not valid",
                Refusal::Cycle => "refused: it would make a cycle",
                Refusal::NotMergeable => "refused: the change cannot be merged",
                Refusal::Exists => "refused: it is there already",
                Refusal::Unsupported => "refused: not supported here",
                Refusal::ReadOnly => "refused: this host is read-only",
                Refusal::Unknown => "refused",
            },
            Outcome::Forbidden => "this actor may not do this",
            Outcome::SignedOut => "the credential is no longer good",
            Outcome::Spent { .. } => "the budget is spent until later",
            Outcome::Unavailable { why, sent } => match (why, sent) {
                (Passing::Timeout, Sent::No) => "timed out before sending",
                (Passing::Timeout, Sent::Maybe) => "timed out; it may have been sent",
                (Passing::Unreachable, _) => "the site could not be reached",
                (Passing::Server, Sent::No) => "the site failed",
                (Passing::Server, Sent::Maybe) => "the site failed; it may have been sent",
                (Passing::Broken, _) => "the work asked about ended in error",
            },
            Outcome::Unread(u) => match u {
                Unread::Cut => "the answer is longer than its bound",
                Unread::Inconsistent => "the answer does not agree with itself",
                Unread::TooLarge => "an item is too large to read whole",
                Unread::Malformed => "the answer does not read",
                Unread::Unknown => "the answer could not be read whole",
            },
        }
    }
}

/// A write of several parts, applied in order, that says how far it got.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Applied<P> {
    /// The parts applied, in order.
    pub done: Vec<P>,
    /// The part that stopped it, and why. None when every part was applied.
    pub stopped: Option<(P, Outcome)>,
}

impl<P> Default for Applied<P> {
    /// Nothing applied, and nothing stopped it.
    fn default() -> Self {
        Self {
            done: Vec::new(),
            stopped: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_outcome_has_its_own_line() {
        let all = [
            Outcome::NotFound,
            Outcome::Conflict(Lost::Changed),
            Outcome::Conflict(Lost::Gone),
            Outcome::Conflict(Lost::HeadMoved),
            Outcome::Conflict(Lost::Elsewhere),
            Outcome::Refused(Refusal::Invalid),
            Outcome::Refused(Refusal::Cycle),
            Outcome::Refused(Refusal::NotMergeable),
            Outcome::Refused(Refusal::Exists),
            Outcome::Refused(Refusal::Unsupported),
            Outcome::Refused(Refusal::ReadOnly),
            Outcome::Forbidden,
            Outcome::SignedOut,
            Outcome::Spent {
                pool: Pool::Calls,
                until: At::new(0),
            },
            Outcome::Unavailable {
                why: Passing::Timeout,
                sent: Sent::No,
            },
            Outcome::Unavailable {
                why: Passing::Timeout,
                sent: Sent::Maybe,
            },
            Outcome::Unavailable {
                why: Passing::Broken,
                sent: Sent::No,
            },
            Outcome::Unread(Unread::Cut),
            Outcome::Unread(Unread::Malformed),
        ];
        let lines: Vec<&str> = all.iter().map(Outcome::line).collect();
        for (i, l) in lines.iter().enumerate() {
            assert!(!l.is_empty());
            assert!(!lines[..i].contains(l), "{l} twice");
        }
    }
}
