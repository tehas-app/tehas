//! `Mark`: the acts, the factory's states, the choices and the epic mark, in
//! our words; `Kind`, the site's type of ticket; `MarkSpec` for a project's
//! own mark. How a site writes each one is the adapter's.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::ids::checked;
use crate::{unknown, MarkName};

/// What a ticket or a change carries, in tehas's words. How a site writes
/// each one is the adapter's.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
#[non_exhaustive]
pub enum Mark {
    /// What a person does to ask the factory for work.
    Act(Act),
    /// The factory's state.
    State(State),
    /// A choice made beside an act.
    Choice(Choice),
    /// The epic mark, apart from the type `Kind::Epic`. Each has its own
    /// history; the port folds neither into the other.
    Epic,
    /// A project's own mark, carried through by name.
    Project(MarkName),
    /// A mark this copy does not know. It authorises nothing.
    #[serde(
        untagged,
        deserialize_with = "mark_unknown",
        serialize_with = "unknown_tag"
    )]
    Unknown,
}

/// The tags of `Mark` but `Unknown`.
const MARK_TAGS: &[&str] = &["Act", "State", "Choice", "Epic", "Project"];

fn mark_unknown<'de, D: Deserializer<'de>>(d: D) -> Result<(), D::Error> {
    unknown::read(d, MARK_TAGS)
}

pub(crate) fn unknown_tag<S: Serializer>(s: S) -> Result<S::Ok, S::Error> {
    unknown::write(s)
}

/// What a person does to ask the factory for work. Authorisation reads who.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Act {
    /// Approve the ticket for work.
    Approve,
    /// Ask for a plan first.
    Plan,
    /// Ask for the ticket to be refined.
    Refine,
    /// Ask for a stopped cycle's work to be finished.
    Finish,
    /// Start once every ticket its Blocked marker names is closed; hold
    /// until then.
    Block,
    /// Pause the factory.
    Pause,
    /// An act this copy does not know. It authorises nothing.
    #[serde(other)]
    Unknown,
}

/// The factory's state. Written by the factory alone; another account that
/// writes one is not the factory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum State {
    /// A cycle works on it.
    Running,
    /// A plan is being made.
    Planning,
    /// Its change is being brought up to date.
    Refreshing,
    /// Its change waits for a merge.
    AwaitingMerge,
    /// Its change is merged.
    Shipped,
    /// The factory asked a question and waits for the answer.
    Question,
    /// A person must look at it.
    NeedsPerson,
    /// Other tickets took its place.
    Replaced,
    /// It was refined.
    Refined,
    /// The millwright left it to a person; the factory does nothing on it.
    Escalated,
    /// A ticket the factory filed as a proposal, held until a person acts.
    Proposal,
    /// A state this copy does not know. It holds the ticket.
    #[serde(other)]
    Unknown,
}

/// A choice made beside an act: the driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Choice {
    /// The lean driver.
    Lean,
    /// The classic driver.
    Classic,
    /// A choice this copy does not know.
    #[serde(other)]
    Unknown,
}

/// The site's type of ticket, as the site names it. `Epic` is a type like
/// any other; the epic mark is `Mark::Epic`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
#[non_exhaustive]
pub enum Kind {
    /// A feature.
    Feature,
    /// A bug.
    Bug,
    /// A task.
    Task,
    /// An epic, as a type.
    Epic,
    /// Another type, by the site's name.
    Other(MarkName),
    /// A type this copy does not know.
    #[serde(
        untagged,
        deserialize_with = "kind_unknown",
        serialize_with = "unknown_tag"
    )]
    Unknown,
}

/// The tags of `Kind` but `Unknown`.
const KIND_TAGS: &[&str] = &["Feature", "Bug", "Task", "Epic", "Other"];

fn kind_unknown<'de, D: Deserializer<'de>>(d: D) -> Result<(), D::Error> {
    unknown::read(d, KIND_TAGS)
}

/// How soon a ticket is to be worked on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Priority {
    /// Before the rest.
    Urgent,
    /// In its turn.
    Normal,
}

/// A project's own mark, as a project asks for it to exist. `colour` and
/// `about` are best effort: an adapter whose site has no colour or text for
/// a mark ignores them, and the mark is still made. `name` alone is the
/// contract.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MarkSpec {
    /// The mark's name.
    pub name: MarkName,
    /// Its colour, if the site has one.
    pub colour: Option<Colour>,
    /// Its text, if the site has one.
    pub about: Option<String>,
}

checked!(
    /// A colour. Six hex digits, checked when made.
    Colour,
    "colour",
    colour_ok
);

fn colour_ok(s: &str) -> bool {
    s.len() == 6 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_colour_is_six_hex_digits() {
        assert!(Colour::new("0e8a16").is_ok());
        assert!(Colour::new("ABCDEF").is_ok());
        assert!(Colour::new("abcde").is_err());
        assert!(Colour::new("abcdefa").is_err());
        assert!(Colour::new("#abcde").is_err());
        assert!(Colour::new("abcdeg").is_err());
    }
}
