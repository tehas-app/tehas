//! The tickets port: tickets, their marks, history, blockers, parts and
//! targets, and its two traits, `Tickets` and `TicketWrites`.

use std::num::{NonZeroU32, NonZeroU64};

use serde::{Deserialize, Serialize};

use crate::mark::unknown_tag;
use crate::{
    unknown, Account, AccountId, Applied, At, Body, Bound, ChangeDigest, ChangeNo, ChangeToken,
    Cites, Comment, CommentId, CommitId, Cursor, Depth, Did, Edit, Got, Item, Kind, Mark, MarkId,
    MarkName, MarkSpec, Marker, MarkerEdit, MarkerKey, MarkerKind, Outcome, Posted, Priority, Ref,
    Reports, Revision, TargetId, TicketKey, Took, WebLink, Whole, Window, Within,
};

/// Whether a ticket is open, and why it closed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TicketState {
    /// Open.
    Open,
    /// Closed, for this reason.
    Closed(CloseReason),
}

/// Why a ticket closed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CloseReason {
    /// Its work is done.
    Done,
    /// It will not be done.
    NotPlanned,
    /// Another ticket asks the same.
    Duplicate(TicketKey),
}

/// A ticket's fields without its text, notes or history.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TicketDigest {
    /// The ticket.
    pub key: TicketKey,
    /// Its title.
    pub title: String,
    /// Open, or closed and why.
    pub state: TicketState,
    /// The site's type of ticket, if it has one.
    pub kind: Option<Kind>,
    /// How soon it is to be worked on.
    pub priority: Priority,
    /// The marks it carries now.
    pub marks: Vec<Mark>,
    /// The target, with its title as the site names it now.
    pub target: Option<TargetName>,
    /// Who filed it; None where the site names nobody.
    pub author: Option<Account>,
    /// The ticket it is a part of: P exactly when `Tickets::parts(P)`
    /// holds this ticket. None when it is a part of none. Missing, it reads
    /// as None.
    #[serde(default)]
    pub parent: Option<Ref<TicketKey>>,
    /// When it was filed.
    pub created: At,
    /// When the site last saw the ticket change, as it records it: a
    /// caller compares it with a time it stored.
    pub updated: At,
    /// Equal revisions mean the ticket did not change.
    pub revision: Revision,
    /// Where a person opens it.
    pub link: WebLink,
}

new!(
    TicketDigest {
        key: TicketKey,
        state: TicketState,
        created: At,
        updated: At,
        revision: Revision,
        link: WebLink
    },
    title,
    kind,
    priority,
    marks,
    target,
    author,
    parent
);

/// A target by its id, with its title as the site names it now.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TargetName {
    /// The target.
    pub id: TargetId,
    /// Its title.
    pub title: String,
}

new!(TargetName {
    id: TargetId,
    title: String
});

/// A ticket, read whole to the depth asked.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Ticket {
    /// Its fields.
    pub digest: TicketDigest,
    /// The ticket's own text, without our markers. Where ticket and
    /// blueprint are one item, the blueprint's body; a caller reads it from
    /// the port it means.
    pub text: String,
    /// When it closed, if it is closed.
    pub closed: Option<At>,
    /// What closed it, from the newest close in its history.
    pub closer: Option<Closer>,
    /// Its parts, whole at every `Depth`, `Depth::Cut` included: as
    /// `Tickets::parts` answers. Past the read's bound the ticket is
    /// `Unread(Cut)`, never a short list.
    pub parts: Vec<Part>,
    /// What it waits on, whole at every `Depth` as `parts` is: as
    /// `Tickets::blockers` answers.
    pub blockers: Vec<Blocker>,
    /// The accounts it is assigned to.
    pub assignees: Vec<Account>,
    /// Our markers in the ticket's own text, apart from `text`.
    pub markers: Vec<Marker>,
    /// Changes that name this ticket in their text, as the site records it.
    pub mentioned_by: Vec<Ref<ChangeNo>>,
    /// Oldest first. Of two events in one second an addition sorts before a
    /// removal; of two of one kind the later in the site's answer wins.
    /// As much as the read asked: the window its `Depth` gives
    /// (`Depth::history`), or the one given beside it. `Depth::Cut` takes
    /// none: empty, with `more`.
    pub history: Took<TicketEvent>,
}

new!(
    Ticket {
        digest: TicketDigest
    },
    text,
    closed,
    closer,
    parts,
    blockers,
    assignees,
    markers,
    mentioned_by,
    history
);

/// A part of a ticket, and whether it is open.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Part {
    /// The part.
    pub key: Ref<TicketKey>,
    /// It is open.
    pub open: bool,
}

new!(Part { key: Ref<TicketKey>, open: bool });

/// A blocker may be a change where the site lets a ticket wait on one.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Blocker {
    /// What the ticket waits on.
    pub on: Ref<Item>,
    /// Its state.
    pub state: BlockerState,
}

new!(Blocker { on: Ref<Item>, state: BlockerState });

/// The state of what a ticket waits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BlockerState {
    /// Open.
    Open,
    /// Closed.
    Closed,
    /// A change, merged.
    Merged,
}

/// Who or what closed a ticket.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Closer {
    /// A merged change that said it closes it.
    Change {
        /// The change.
        no: ChangeNo,
        /// It was merged.
        merged: bool,
    },
    /// A commit that said it closes it.
    Commit(CommitId),
    /// An account, by hand.
    Account(Account),
}

/// One event of a ticket's history.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TicketEvent {
    /// What happened.
    pub what: Happened,
    /// None when the site names nobody, as for a deleted account.
    pub by: Option<Account>,
    /// When, if the site says.
    pub at: Option<At>,
    /// For a mark that went on or came off, the site's id of that mark,
    /// as `Tickets::marks` names it: a mark renamed to one of ours keeps
    /// its old id, so it is not ours. None where the site gave no id: the
    /// caller then compares `Tickets::marks`' id for that mark with its
    /// pin, and refuses the mark when they differ or the mark has no pin.
    /// The ticket's current marks carry no ids and prove nothing. Missing,
    /// it reads as None.
    #[serde(default)]
    pub mark_id: Option<MarkId>,
}

new!(TicketEvent { what: Happened }, by, at, mark_id);

/// What happened to a ticket.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
#[non_exhaustive]
pub enum Happened {
    /// A mark went on.
    Marked(Mark),
    /// A mark came off.
    Unmarked(Mark),
    /// The type was set, or taken off.
    Kind(Option<Kind>),
    /// The priority was set.
    Priority(Priority),
    /// It began to wait on this.
    BlockerAdded(Ref<Item>),
    /// It stopped waiting on this.
    BlockerRemoved(Ref<Item>),
    /// An account was assigned.
    Assigned(Account),
    /// An account was unassigned.
    Unassigned(Account),
    /// The target was set, or taken off.
    Targeted(Option<TargetId>),
    /// The title changed.
    Retitled {
        /// The title before.
        from: String,
        /// The title after.
        to: String,
    },
    /// It closed.
    Closed {
        /// Why.
        reason: CloseReason,
        /// What closed it, if the site says.
        closer: Option<Closer>,
    },
    /// It was reopened.
    Reopened,
    /// An event this copy does not know.
    #[serde(
        untagged,
        deserialize_with = "unknown::read_any",
        serialize_with = "unknown_tag"
    )]
    Unknown,
}

impl Happened {
    /// The kind of this event, as a history read's filter names it.
    pub fn kind(&self) -> HappenedKind {
        match self {
            Happened::Marked(_) => HappenedKind::Marked,
            Happened::Unmarked(_) => HappenedKind::Unmarked,
            Happened::Kind(_) => HappenedKind::Kind,
            Happened::Priority(_) => HappenedKind::Priority,
            Happened::BlockerAdded(_) => HappenedKind::BlockerAdded,
            Happened::BlockerRemoved(_) => HappenedKind::BlockerRemoved,
            Happened::Assigned(_) => HappenedKind::Assigned,
            Happened::Unassigned(_) => HappenedKind::Unassigned,
            Happened::Targeted(_) => HappenedKind::Targeted,
            Happened::Retitled { .. } => HappenedKind::Retitled,
            Happened::Closed { .. } => HappenedKind::Closed,
            Happened::Reopened => HappenedKind::Reopened,
            Happened::Unknown => HappenedKind::Unknown,
        }
    }
}

/// The kind of a `Happened`, without its content: what a history read
/// takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum HappenedKind {
    /// A mark went on.
    Marked,
    /// A mark came off.
    Unmarked,
    /// The type was set, or taken off.
    Kind,
    /// The priority was set.
    Priority,
    /// It began to wait on something.
    BlockerAdded,
    /// It stopped waiting on something.
    BlockerRemoved,
    /// An account was assigned.
    Assigned,
    /// An account was unassigned.
    Unassigned,
    /// The target was set, or taken off.
    Targeted,
    /// The title changed.
    Retitled,
    /// It closed.
    Closed,
    /// It was reopened.
    Reopened,
    /// An event this copy does not know: it takes the events read as
    /// `Happened::Unknown`.
    #[serde(other)]
    Unknown,
}

/// A target: a set of tickets with a title and maybe a due date.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Target {
    /// The target.
    pub id: TargetId,
    /// Its title.
    pub title: String,
    /// When it is due, if it has a date.
    pub due: Option<At>,
    /// When it was made.
    pub created: At,
    /// It is open.
    pub open: bool,
    /// How many of its tickets are open.
    pub open_count: u32,
    /// How many of its tickets are closed.
    pub closed_count: u32,
}

new!(
    Target {
        id: TargetId,
        title: String,
        created: At
    },
    due,
    open,
    open_count,
    closed_count
);

/// A change to a target.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TargetEdit {
    /// A new title.
    Retitle(String),
    /// A new due date, or none.
    Due(Option<At>),
    /// Close it.
    Close,
    /// Open it again.
    Reopen,
}

/// Which tickets a list takes. Every field narrows; the default takes every
/// ticket.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Filter {
    /// Open or closed tickets alone.
    pub open: Option<bool>,
    /// Tickets carrying every one of these marks.
    pub carrying: Vec<Mark>,
    /// Tickets carrying at least one of these marks. Empty narrows
    /// nothing. Beside `carrying`, a ticket must meet both. Missing, it
    /// reads as empty.
    #[serde(default)]
    pub carrying_any: Vec<Mark>,
    /// Tickets of this type.
    pub kind: Option<Kind>,
    /// Tickets of this target.
    pub target: Option<TargetId>,
}

/// A ticket to file.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NewTicket {
    /// Its title.
    pub title: String,
    /// Its text.
    pub text: Body,
    /// Our markers, beside the text.
    pub markers: Vec<Marker>,
    /// The marks it carries.
    pub marks: Vec<Mark>,
    /// Its type, if any.
    pub kind: Option<Kind>,
    /// How soon it is to be worked on.
    pub priority: Priority,
    /// Its target, if any.
    pub target: Option<TargetId>,
    /// The ticket it is a part of, if any.
    pub parent: Option<TicketKey>,
}

/// The parts of one press or one decision, applied in this order. The one
/// path by which marks go on and off a ticket, acts included. The note part
/// is written when `note` or `markers` is given: its text, and its markers
/// beside it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Step {
    /// The note's text.
    pub note: Option<String>,
    /// Our markers, beside the note.
    pub markers: Vec<Marker>,
    /// The marks to take off.
    pub unmark: Vec<Mark>,
    /// The marks to put on.
    pub mark: Vec<Mark>,
    /// Close the ticket, for this reason.
    pub close: Option<CloseReason>,
}

/// One part of a `Step`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StepPart {
    /// The note, with its markers.
    Note,
    /// A mark taken off.
    Unmark(Mark),
    /// A mark put on.
    Mark(Mark),
    /// The close.
    Close,
}

/// A ticket that closed, with what closed it and its size of work.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ClosedTicket {
    /// Its fields.
    pub digest: TicketDigest,
    /// When it closed.
    pub closed: At,
    /// What closed it, if the site says.
    pub closer: Option<Closer>,
    /// How many parts it has.
    pub part_count: u32,
    /// The merged changes that closed it, with their size. Empty where
    /// tickets are not on the code host: `Board::closed_work` joins them.
    pub merged: Vec<ChangeDigest>,
}

new!(
    ClosedTicket {
        digest: TicketDigest,
        closed: At
    },
    closer,
    part_count,
    merged
);

/// A ticket's change since a cursor: its digest, its text and its close.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TicketChange {
    /// Its fields.
    pub digest: TicketDigest,
    /// Its text, without our markers.
    pub text: String,
    /// When it closed, if it is closed.
    pub closed: Option<At>,
}

new!(
    TicketChange {
        digest: TicketDigest
    },
    text,
    closed
);

/// Tickets and notes changed since a cursor, and where to read on from.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TicketDelta {
    /// The tickets that changed.
    pub tickets: Vec<TicketChange>,
    /// The notes that changed, each with its ticket.
    pub notes: Vec<(TicketKey, Comment)>,
    /// Where the next delta starts: after the last change it holds.
    pub next: Cursor,
    /// More changes wait past `next`: the bound cut this delta. Missing,
    /// it reads as false, as `new` leaves it.
    #[serde(default)]
    pub more: bool,
}

new!(TicketDelta { next: Cursor }, tickets, notes, more);

/// A ticket an account filed, as a lost press finds it again.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Filed {
    /// The ticket.
    pub key: TicketKey,
    /// Its title.
    pub title: String,
    /// The first line of its text.
    pub first_line: String,
    /// Where a person opens it.
    pub link: WebLink,
}

new!(
    Filed {
        key: TicketKey,
        link: WebLink
    },
    title,
    first_line
);

/// A ticket's digest with what closed it, as `Tickets::several_with_closers`
/// answers it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct WithCloser {
    /// The digest `several` gives.
    pub digest: TicketDigest,
    /// What closed it: None while it is open or where the site names
    /// nothing.
    #[serde(default)]
    pub closer: Option<Closer>,
}

new!(
    WithCloser {
        digest: TicketDigest
    },
    closer
);

/// A mark the site holds, with its id, as `Tickets::marks` answers it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct MarkHeld {
    /// The mark, by the name it has now.
    pub mark: Mark,
    /// The site's id of it, which a rename keeps.
    pub id: MarkId,
}

new!(MarkHeld {
    mark: Mark,
    id: MarkId
});

/// The tickets port's reads. A handle is bound to one project, one actor
/// and one caller name.
pub trait Tickets: Reports + Cites {
    /// One ticket, to the depth asked, in one read. Its history is the
    /// window `depth` gives (`Depth::history`), or `history` when given.
    fn ticket(&mut self, key: &TicketKey, depth: Depth, history: Option<Window>) -> Got<Ticket>;
    /// Whether a number names a ticket or a change, where they share one
    /// number space. `NotFound` where neither.
    fn what_is(&mut self, number: NonZeroU64) -> Got<Item>;
    /// `what_is` for several numbers, in one question where the site allows.
    fn what_are(&mut self, numbers: &[NonZeroU64]) -> Vec<(NonZeroU64, Got<Item>)>;
    /// The tickets the filter takes, whole. In list order: oldest filed
    /// first, ties broken by key.
    fn list(&mut self, filter: &Filter, bound: Bound) -> Got<Whole<TicketDigest>>;
    /// Several tickets' digests. A key whose number names a change, where
    /// they share one number space, is `NotFound`: it is no ticket. A
    /// caller that may hold either asks `what_are` first; an adapter may
    /// answer both from one question and keep the answer for the pass.
    fn several(&mut self, keys: &[TicketKey]) -> Vec<(TicketKey, Got<TicketDigest>)>;
    /// `several`, each digest with what closed the ticket, in the same
    /// request: the closer `Ticket.closer` gives, from the newest close in
    /// its history, and None while it is open or where the site names
    /// nothing. A key that names a change is `NotFound`, as in `several`.
    fn several_with_closers(&mut self, keys: &[TicketKey]) -> Vec<(TicketKey, Got<WithCloser>)>;
    /// A ticket's history, oldest first: the events of the kinds in
    /// `only`, or every event for None. The window counts the events taken.
    fn history(
        &mut self,
        key: &TicketKey,
        window: Window,
        only: Option<&[HappenedKind]>,
    ) -> Got<Took<TicketEvent>>;
    /// Notes on the ticket itself, oldest first, as much as `window` takes.
    /// Where ticket and blueprint are one item, empty: the item's comments
    /// are the blueprint's.
    fn notes(&mut self, key: &TicketKey, window: Window) -> Got<Took<Comment>>;
    /// The edits of a ticket note, oldest first: how a link or a hold is
    /// vouched. Where ticket and blueprint are one item, the same answer as
    /// `Blueprints::comment_edits` for that comment.
    fn note_edits(&mut self, id: &CommentId, bound: Bound) -> Got<Whole<Edit>>;
    /// What a ticket waits on.
    fn blockers(&mut self, key: &TicketKey) -> Got<Whole<Blocker>>;
    /// A ticket's parts.
    fn parts(&mut self, key: &TicketKey) -> Got<Whole<Part>>;
    /// The tickets closed since a time, in list order.
    fn closed_since(&mut self, since: At, bound: Bound) -> Got<Whole<ClosedTicket>>;
    /// The first `first` hits in the site's order of relevance, with
    /// `more`: a search is never a whole list, and a common word is not
    /// `Unread(Cut)`. `within` asks for titles alone or everything.
    /// `words` are words, never the site's query syntax: the adapter quotes
    /// or escapes every qualifier its site has, so no caller's words can
    /// widen the search past this project or this port's items.
    fn search(
        &mut self,
        words: &str,
        within: Within,
        open: Option<bool>,
        first: NonZeroU32,
    ) -> Got<Took<TicketDigest>>;
    /// Tickets an account filed since a time: a lost press found again.
    fn filed_by(&mut self, by: &AccountId, since: At, bound: Bound) -> Got<Whole<Filed>>;
    /// Tickets carrying `mark` with our marker of `kind` and `key` in any
    /// place a ticket marker can be: the ticket's own text, its notes, and,
    /// where ticket and blueprint are one item, the item's body and
    /// comments. `open` takes open or closed tickets alone, None both: a
    /// caller that must not file a report twice asks None.
    fn find_marked(
        &mut self,
        mark: &Mark,
        kind: &MarkerKind,
        key: &MarkerKey,
        open: Option<bool>,
    ) -> Got<Whole<TicketKey>>;
    /// The site's types of ticket.
    fn kinds(&mut self) -> Got<Whole<Kind>>;
    /// The marks the site holds for the project, each with its id, as the
    /// site names them now: what a caller pins, so a mark renamed to one
    /// of ours is told apart by its id. The id is the site's own and a
    /// rename keeps it: never one made from the name. Each mark once; a
    /// site word this copy does not know is left out.
    fn marks(&mut self, bound: Bound) -> Got<Whole<MarkHeld>>;
    /// The project's targets, open and closed, whole.
    fn targets(&mut self, bound: Bound) -> Got<Whole<Target>>;
    /// The tickets of a target, in list order.
    fn members(&mut self, target: &TargetId, bound: Bound) -> Got<Whole<TicketDigest>>;
    /// Has anything changed since this token. Free on a site that answers
    /// conditional requests or pushes events.
    fn changed(&mut self, since: Option<&ChangeToken>) -> Got<(bool, ChangeToken)>;
    /// Tickets and notes changed since the cursor, with their text and
    /// close, each list oldest first. `bound` counts both lists together.
    /// Past it, a page of the first changes with `more`, and a `next` that
    /// resumes after the last change of either list: never `Unread(Cut)`,
    /// so a long history spreads over passes. A page with `more` holds at
    /// most `bound` changes and never none; it may hold fewer.
    fn delta(&mut self, from: Option<&Cursor>, bound: Bound) -> Got<TicketDelta>;
    /// A link a visitor opens to file a ticket with this text filled in.
    fn new_ticket_link(&self, title: &str, text: &str) -> WebLink;
}

/// What `prepare` could not do.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Unprepared {
    /// The first mark it could not make; None when the site failed before
    /// it tried one.
    pub mark: Option<MarkName>,
    /// Why.
    pub why: Outcome,
}

new!(Unprepared { why: Outcome }, mark);

impl From<Outcome> for Unprepared {
    fn from(why: Outcome) -> Unprepared {
        Unprepared { mark: None, why }
    }
}

/// The tickets port's writes.
pub trait TicketWrites: Tickets {
    /// The parts of one step, in order, with how far it got, and the note's
    /// answer when the note part was applied. The history then names this
    /// handle's account. The journal sends a step part by part instead;
    /// this is for a caller with no journal.
    fn apply(
        &mut self,
        key: &TicketKey,
        step: &Step,
    ) -> (Applied<StepPart>, Option<Posted<CommentId>>);
    /// Make sure the site can carry every mark of the factory. A mark that
    /// cannot be made stops none of the others; the first that failed is
    /// the answer.
    fn prepare(&mut self) -> Result<(), Unprepared>;
    /// Make sure a project's own mark exists. Already there is done. Its
    /// colour and text are made to match where the site has them and the
    /// spec gives them; a site without them ignores both, and that is done
    /// too.
    fn ensure_mark(&mut self, spec: &MarkSpec) -> Did;
    /// Put a mark on.
    fn mark(&mut self, key: &TicketKey, mark: &Mark) -> Did;
    /// Take a mark off.
    fn unmark(&mut self, key: &TicketKey, mark: &Mark) -> Did;
    /// Close the ticket.
    fn close(&mut self, key: &TicketKey, reason: &CloseReason) -> Did;
    /// Write a note on the ticket, its markers beside it.
    fn note(&mut self, key: &TicketKey, body: &str, markers: &[Marker]) -> Got<Posted<CommentId>>;
    /// File a new ticket.
    fn file(&mut self, new: &NewTicket) -> Got<Posted<TicketKey>>;
    /// The ticket's own title and text. `since`: write only if the ticket is
    /// still at this revision, else `Conflict(Changed)`. Markers not named in
    /// `markers` are kept.
    fn edit(
        &mut self,
        key: &TicketKey,
        title: Option<&str>,
        text: Option<&str>,
        markers: &MarkerEdit,
        since: &Revision,
    ) -> Got<WebLink>;
    /// Open a closed ticket again.
    fn reopen(&mut self, key: &TicketKey) -> Did;
    /// Set the ticket's type, or take it off.
    fn set_kind(&mut self, key: &TicketKey, kind: Option<&Kind>) -> Did;
    /// Set the ticket's priority.
    fn set_priority(&mut self, key: &TicketKey, p: Priority) -> Did;
    /// `key` waits on `on`, a ticket or, where the site lets a ticket wait on
    /// one, a change. `Refused(Cycle)` when the site sees a loop;
    /// `Refused(Unsupported)` for a change on a site that cannot hold it.
    fn wait_on(&mut self, key: &TicketKey, on: &Item) -> Did;
    /// `key` no longer waits on `on`.
    fn stop_waiting(&mut self, key: &TicketKey, on: &Item) -> Did;
    /// Assign an account of the ticket realm. `Refused(Invalid)` when the
    /// site will not assign it here; the dispatcher's claim then stands
    /// without it.
    fn assign(&mut self, key: &TicketKey, who: &AccountId) -> Did;
    /// Make `child` a part of `parent`.
    fn add_part(&mut self, parent: &TicketKey, child: &TicketKey) -> Did;
    /// Take `child` out of `parent`'s parts.
    fn remove_part(&mut self, parent: &TicketKey, child: &TicketKey) -> Did;
    /// Make a target.
    fn new_target(&mut self, title: &str, due: Option<At>) -> Got<TargetId>;
    /// Change a target.
    fn edit_target(&mut self, id: &TargetId, edit: &TargetEdit) -> Did;
    /// Set the ticket's target, or take it off.
    fn set_target(&mut self, key: &TicketKey, target: Option<&TargetId>) -> Did;
}
