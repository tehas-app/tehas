//! Every write and its answer, as the journal records it: `Write`,
//! `WriteKind`, `Receipt`, `Posted`, `Writes`. One variant for every write
//! method of every port, each one request to the site but `Prepare` and
//! `Propose`. `TicketWrites::apply` is no write of its own: it is its parts.

use serde::{Deserialize, Serialize};

use crate::mark::unknown_tag;
use crate::{
    unknown, AccountId, At, Blobs, BlueprintNo, BlueprintWrites, Body, ChangeNo, CloseReason,
    CodeHostWrites, CommentId, CommitId, Did, FileName, Got, Item, Kind, Mark, MarkSpec, Marker,
    MarkerEdit, MergeHow, NewBlueprint, NewChange, NewRelease, NewTicket, Outcome, Priority,
    Proposal, Proposed, Refusal, ReleaseId, Revision, Sha256, Status, Sum, TagName, TargetEdit,
    TargetId, TicketKey, TicketWrites, WebLink,
};

/// One write, as the journal records it before it is sent. A row names its
/// files by sum and keeps its bodies summed, so it holds no bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
#[non_exhaustive]
pub enum Write {
    /// Put a mark on a ticket.
    Mark {
        /// The ticket.
        ticket: TicketKey,
        /// The mark.
        mark: Mark,
    },
    /// Take a mark off a ticket.
    Unmark {
        /// The ticket.
        ticket: TicketKey,
        /// The mark.
        mark: Mark,
    },
    /// Close a ticket.
    Close {
        /// The ticket.
        ticket: TicketKey,
        /// Why.
        reason: CloseReason,
    },
    /// Open a closed ticket again.
    Reopen {
        /// The ticket.
        ticket: TicketKey,
    },
    /// Write a note on a ticket.
    TicketNote {
        /// The ticket.
        ticket: TicketKey,
        /// The note's text.
        body: Body,
        /// Our markers, beside it.
        markers: Vec<Marker>,
    },
    /// File a new ticket.
    FileTicket(NewTicket),
    /// Edit a ticket's own title and text, if still at `since`.
    EditTicket {
        /// The ticket.
        ticket: TicketKey,
        /// The new title, if it changes.
        title: Option<String>,
        /// The new text, if it changes.
        text: Option<Body>,
        /// How its markers change.
        markers: MarkerEdit,
        /// The revision it must still be at.
        since: Revision,
    },
    /// Set a ticket's type, or take it off.
    SetKind {
        /// The ticket.
        ticket: TicketKey,
        /// The type.
        kind: Option<Kind>,
    },
    /// Set a ticket's priority.
    SetPriority {
        /// The ticket.
        ticket: TicketKey,
        /// The priority.
        priority: Priority,
    },
    /// A ticket begins to wait on a ticket or a change.
    WaitOn {
        /// The ticket.
        ticket: TicketKey,
        /// What it waits on.
        on: Item,
    },
    /// A ticket stops waiting on a ticket or a change.
    StopWaiting {
        /// The ticket.
        ticket: TicketKey,
        /// What it waited on.
        on: Item,
    },
    /// Assign an account to a ticket.
    Assign {
        /// The ticket.
        ticket: TicketKey,
        /// The account.
        who: AccountId,
    },
    /// Make a ticket a part of another.
    AddPart {
        /// The ticket that gains a part.
        parent: TicketKey,
        /// The part.
        child: TicketKey,
    },
    /// Take a part out of a ticket.
    RemovePart {
        /// The ticket that loses a part.
        parent: TicketKey,
        /// The part.
        child: TicketKey,
    },
    /// Make a target.
    NewTarget {
        /// Its title.
        title: String,
        /// When it is due, if it has a date.
        due: Option<At>,
    },
    /// Change a target.
    EditTarget {
        /// The target.
        id: TargetId,
        /// The change.
        edit: TargetEdit,
    },
    /// Set a ticket's target, or take it off.
    SetTarget {
        /// The ticket.
        ticket: TicketKey,
        /// The target.
        target: Option<TargetId>,
    },
    /// Make sure the site can carry every mark of the factory.
    Prepare,
    /// Make sure a project's own mark exists.
    EnsureMark(MarkSpec),
    /// Open a new blueprint.
    OpenBlueprint(NewBlueprint),
    /// Edit a blueprint's title and body, if still at `since`.
    EditBlueprint {
        /// The blueprint.
        no: BlueprintNo,
        /// The new title, if it changes.
        title: Option<String>,
        /// The new body, if it changes.
        body: Option<Body>,
        /// How its markers change.
        markers: MarkerEdit,
        /// The revision it must still be at.
        since: Revision,
    },
    /// Write a comment on a blueprint.
    Comment {
        /// The blueprint.
        on: BlueprintNo,
        /// The comment's text.
        body: Body,
        /// Our markers, beside it.
        markers: Vec<Marker>,
    },
    /// Edit a comment, if still at `since`.
    EditComment {
        /// The comment.
        id: CommentId,
        /// The new text.
        body: Body,
        /// How its markers change.
        markers: MarkerEdit,
        /// The revision it must still be at.
        since: Revision,
    },
    /// Close a blueprint.
    CloseBlueprint {
        /// The blueprint.
        no: BlueprintNo,
        /// Its work is done.
        done: bool,
    },
    /// Open a change.
    OpenChange(NewChange),
    /// Edit a change's title and body.
    EditChange {
        /// The change.
        no: ChangeNo,
        /// The new title, if it changes.
        title: Option<String>,
        /// The new body, if it changes.
        body: Option<Body>,
        /// How its markers change.
        markers: MarkerEdit,
    },
    /// Put a mark on a change.
    MarkChange {
        /// The change.
        no: ChangeNo,
        /// The mark.
        mark: Mark,
    },
    /// Take a mark off a change.
    UnmarkChange {
        /// The change.
        no: ChangeNo,
        /// The mark.
        mark: Mark,
    },
    /// Write a comment on a change.
    CommentChange {
        /// The change.
        no: ChangeNo,
        /// The comment's text.
        body: Body,
        /// Our markers, beside it.
        markers: Vec<Marker>,
    },
    /// Close a change unmerged.
    CloseChange {
        /// The change.
        no: ChangeNo,
    },
    /// Merge a change, if its head is still `head`.
    Merge {
        /// The change.
        no: ChangeNo,
        /// The head it must still be at.
        head: CommitId,
        /// How it merges.
        how: MergeHow,
    },
    /// Commit files on a branch and open a change for them.
    Propose(Proposal),
    /// Tag a commit.
    Tag {
        /// The tag.
        name: TagName,
        /// The commit.
        at: CommitId,
    },
    /// Post a status on a commit.
    SetStatus {
        /// The commit.
        at: CommitId,
        /// The status.
        status: Status,
    },
    /// Make a release of a tag.
    NewRelease(NewRelease),
    /// Add a file to a release, its bytes by sum.
    Upload {
        /// The release.
        release: ReleaseId,
        /// The file's name.
        name: FileName,
        /// The file, by its sum.
        file: Sum,
    },
    /// Make a draft release public.
    PublishRelease {
        /// The release.
        release: ReleaseId,
    },
    /// A row this copy does not know: never sent.
    #[serde(
        untagged,
        deserialize_with = "unknown::read_any",
        serialize_with = "unknown_tag"
    )]
    Unknown,
}

/// A write's answer: what it made, and where a person opens it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Posted<T> {
    /// What it made.
    pub id: T,
    /// Where a person opens it.
    pub link: WebLink,
}

impl<T> Posted<T> {
    /// What a write made, and where a person opens it.
    pub fn new(id: T, link: WebLink) -> Self {
        Self { id, link }
    }
}

/// What a write made, with where a person opens it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
#[non_exhaustive]
pub enum Receipt {
    /// Done, with nothing made.
    Done,
    /// A note or a comment.
    Comment(Posted<CommentId>),
    /// A ticket.
    Ticket(Posted<TicketKey>),
    /// A blueprint.
    Blueprint(Posted<BlueprintNo>),
    /// A change.
    Change(Posted<ChangeNo>),
    /// A `Propose`: the change and the head it opened at.
    Proposed(Proposed),
    /// A target.
    Target(TargetId),
    /// A release.
    Release(Posted<ReleaseId>),
    /// An edit, and where a person opens what it edited.
    Edited(WebLink),
    /// An answer this copy does not know.
    #[serde(
        untagged,
        deserialize_with = "unknown::read_any",
        serialize_with = "unknown_tag"
    )]
    Unknown,
}

/// The kind of a write, in our words, one per variant. The store maps each
/// to the word its column of kinds holds; this crate holds no store word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum WriteKind {
    /// A mark put on a ticket.
    Mark,
    /// A mark taken off a ticket.
    Unmark,
    /// A ticket closed.
    Close,
    /// A ticket opened again.
    Reopen,
    /// A note on a ticket.
    TicketNote,
    /// A ticket filed.
    FileTicket,
    /// A ticket's title and text edited.
    EditTicket,
    /// A ticket's type set.
    SetKind,
    /// A ticket's priority set.
    SetPriority,
    /// A ticket made to wait.
    WaitOn,
    /// A ticket's wait taken off.
    StopWaiting,
    /// An account assigned to a ticket.
    Assign,
    /// A part added to a ticket.
    AddPart,
    /// A part taken out of a ticket.
    RemovePart,
    /// A target made.
    NewTarget,
    /// A target changed.
    EditTarget,
    /// A ticket's target set.
    SetTarget,
    /// The factory's marks made ready.
    Prepare,
    /// A project's own mark made sure of.
    EnsureMark,
    /// A blueprint opened.
    OpenBlueprint,
    /// A blueprint's title and body edited.
    EditBlueprint,
    /// A comment on a blueprint.
    Comment,
    /// A comment edited.
    EditComment,
    /// A blueprint closed.
    CloseBlueprint,
    /// A change opened.
    OpenChange,
    /// A change's title and body edited.
    EditChange,
    /// A mark put on a change.
    MarkChange,
    /// A mark taken off a change.
    UnmarkChange,
    /// A comment on a change.
    CommentChange,
    /// A change closed unmerged.
    CloseChange,
    /// A change merged.
    Merge,
    /// Files proposed as a change.
    Propose,
    /// A commit tagged.
    Tag,
    /// A status posted on a commit.
    SetStatus,
    /// A release made.
    NewRelease,
    /// A file added to a release.
    Upload,
    /// A release made public.
    PublishRelease,
    /// A kind this copy does not know.
    #[serde(other)]
    Unknown,
}

impl Write {
    /// The write's kind, one per variant.
    pub fn kind(&self) -> WriteKind {
        match self {
            Write::Mark { .. } => WriteKind::Mark,
            Write::Unmark { .. } => WriteKind::Unmark,
            Write::Close { .. } => WriteKind::Close,
            Write::Reopen { .. } => WriteKind::Reopen,
            Write::TicketNote { .. } => WriteKind::TicketNote,
            Write::FileTicket(_) => WriteKind::FileTicket,
            Write::EditTicket { .. } => WriteKind::EditTicket,
            Write::SetKind { .. } => WriteKind::SetKind,
            Write::SetPriority { .. } => WriteKind::SetPriority,
            Write::WaitOn { .. } => WriteKind::WaitOn,
            Write::StopWaiting { .. } => WriteKind::StopWaiting,
            Write::Assign { .. } => WriteKind::Assign,
            Write::AddPart { .. } => WriteKind::AddPart,
            Write::RemovePart { .. } => WriteKind::RemovePart,
            Write::NewTarget { .. } => WriteKind::NewTarget,
            Write::EditTarget { .. } => WriteKind::EditTarget,
            Write::SetTarget { .. } => WriteKind::SetTarget,
            Write::Prepare => WriteKind::Prepare,
            Write::EnsureMark(_) => WriteKind::EnsureMark,
            Write::OpenBlueprint(_) => WriteKind::OpenBlueprint,
            Write::EditBlueprint { .. } => WriteKind::EditBlueprint,
            Write::Comment { .. } => WriteKind::Comment,
            Write::EditComment { .. } => WriteKind::EditComment,
            Write::CloseBlueprint { .. } => WriteKind::CloseBlueprint,
            Write::OpenChange(_) => WriteKind::OpenChange,
            Write::EditChange { .. } => WriteKind::EditChange,
            Write::MarkChange { .. } => WriteKind::MarkChange,
            Write::UnmarkChange { .. } => WriteKind::UnmarkChange,
            Write::CommentChange { .. } => WriteKind::CommentChange,
            Write::CloseChange { .. } => WriteKind::CloseChange,
            Write::Merge { .. } => WriteKind::Merge,
            Write::Propose(_) => WriteKind::Propose,
            Write::Tag { .. } => WriteKind::Tag,
            Write::SetStatus { .. } => WriteKind::SetStatus,
            Write::NewRelease(_) => WriteKind::NewRelease,
            Write::Upload { .. } => WriteKind::Upload,
            Write::PublishRelease { .. } => WriteKind::PublishRelease,
            Write::Unknown => WriteKind::Unknown,
        }
    }

    /// The same write with every `Body::Text` as `Body::Summed`: what the
    /// journal's row keeps. The ports never hash; the caller gives the sum:
    /// `sha256` answers the sha-256 of a body's text, as an adapter computes
    /// it at the edge, and the length is the text's. A body whose digest it
    /// does not give (None) makes the write `Refused(Invalid)`. A body
    /// already summed asks for none.
    pub fn summed(&self, mut sha256: impl FnMut(&str) -> Option<Sha256>) -> Got<Write> {
        let mut w = self.clone();
        let body = match &mut w {
            Write::TicketNote { body, .. }
            | Write::Comment { body, .. }
            | Write::EditComment { body, .. }
            | Write::CommentChange { body, .. }
            | Write::EditTicket {
                text: Some(body), ..
            }
            | Write::EditBlueprint {
                body: Some(body), ..
            }
            | Write::EditChange {
                body: Some(body), ..
            }
            | Write::FileTicket(NewTicket { text: body, .. })
            | Write::OpenBlueprint(NewBlueprint { body, .. })
            | Write::OpenChange(NewChange { body, .. })
            | Write::Propose(Proposal {
                change: NewChange { body, .. },
                ..
            })
            | Write::NewRelease(NewRelease { notes: body, .. }) => body,
            _ => return Ok(w),
        };
        *body = body
            .summed(&mut sha256)
            .ok_or(Outcome::Refused(Refusal::Invalid))?;
        Ok(w)
    }
}

/// Sends one write to its port.
pub trait Writes {
    /// Send one write. A `Body::Summed` is `Refused(Invalid)`: the journal
    /// sends the write it was given and records its summed form. Files come
    /// from `files` by sum.
    fn send(&mut self, w: &Write, files: &dyn Blobs) -> Got<Receipt>;
    /// True for a read-only host's writer.
    fn read_only(&self) -> bool;
}

/// Sends each variant to its port's write method. Owns its handles.
pub struct Writer {
    /// The tickets write handle.
    pub tickets: Box<dyn TicketWrites + Send>,
    /// The blueprints write handle.
    pub blueprints: Box<dyn BlueprintWrites + Send>,
    /// The code host write handle.
    pub code_host: Box<dyn CodeHostWrites + Send>,
}

/// The text of a body as a port method takes it. A `Body::Summed` is
/// `Refused(Invalid)`: a summed row is never sent.
fn text(body: &Body) -> Got<&str> {
    match body {
        Body::Text(t) => Ok(t),
        Body::Summed(_) => Err(Outcome::Refused(Refusal::Invalid)),
    }
}

fn opt_text(body: &Option<Body>) -> Got<Option<&str>> {
    body.as_ref().map(text).transpose()
}

impl Writes for Writer {
    /// An unknown write is `Refused(Invalid)` before any request. A write
    /// that makes nothing answers `Receipt::Done`; an edit, where a person
    /// opens what it edited.
    fn send(&mut self, w: &Write, files: &dyn Blobs) -> Got<Receipt> {
        let done = |r: Did| r.map(|()| Receipt::Done);
        let t = &mut self.tickets;
        let b = &mut self.blueprints;
        let c = &mut self.code_host;
        match w {
            Write::Mark { ticket, mark } => done(t.mark(ticket, mark)),
            Write::Unmark { ticket, mark } => done(t.unmark(ticket, mark)),
            Write::Close { ticket, reason } => done(t.close(ticket, reason)),
            Write::Reopen { ticket } => done(t.reopen(ticket)),
            Write::TicketNote {
                ticket,
                body,
                markers,
            } => t.note(ticket, text(body)?, markers).map(Receipt::Comment),
            Write::FileTicket(new) => t.file(new).map(Receipt::Ticket),
            Write::EditTicket {
                ticket,
                title,
                text: body,
                markers,
                since,
            } => t
                .edit(ticket, title.as_deref(), opt_text(body)?, markers, since)
                .map(Receipt::Edited),
            Write::SetKind { ticket, kind } => done(t.set_kind(ticket, kind.as_ref())),
            Write::SetPriority { ticket, priority } => done(t.set_priority(ticket, *priority)),
            Write::WaitOn { ticket, on } => done(t.wait_on(ticket, on)),
            Write::StopWaiting { ticket, on } => done(t.stop_waiting(ticket, on)),
            Write::Assign { ticket, who } => done(t.assign(ticket, who)),
            Write::AddPart { parent, child } => done(t.add_part(parent, child)),
            Write::RemovePart { parent, child } => done(t.remove_part(parent, child)),
            Write::NewTarget { title, due } => t.new_target(title, *due).map(Receipt::Target),
            Write::EditTarget { id, edit } => done(t.edit_target(id, edit)),
            Write::SetTarget { ticket, target } => done(t.set_target(ticket, target.as_ref())),
            Write::Prepare => done(t.prepare()),
            Write::EnsureMark(spec) => done(t.ensure_mark(spec)),
            Write::OpenBlueprint(new) => b.open(new).map(Receipt::Blueprint),
            Write::EditBlueprint {
                no,
                title,
                body,
                markers,
                since,
            } => b
                .edit(*no, title.as_deref(), opt_text(body)?, markers, since)
                .map(Receipt::Edited),
            Write::Comment { on, body, markers } => {
                b.comment(*on, text(body)?, markers).map(Receipt::Comment)
            }
            Write::EditComment {
                id,
                body,
                markers,
                since,
            } => b
                .edit_comment(id, text(body)?, markers, since)
                .map(Receipt::Edited),
            Write::CloseBlueprint { no, done: d } => done(b.close(*no, *d)),
            Write::OpenChange(new) => c.open_change(new).map(Receipt::Change),
            Write::EditChange {
                no,
                title,
                body,
                markers,
            } => c
                .edit_change(*no, title.as_deref(), opt_text(body)?, markers)
                .map(Receipt::Edited),
            Write::MarkChange { no, mark } => done(c.mark_change(*no, mark)),
            Write::UnmarkChange { no, mark } => done(c.unmark_change(*no, mark)),
            Write::CommentChange { no, body, markers } => c
                .comment_change(*no, text(body)?, markers)
                .map(Receipt::Comment),
            Write::CloseChange { no } => done(c.close_change(*no)),
            Write::Merge { no, head, how } => done(c.merge(*no, head, *how)),
            Write::Propose(p) => match c.propose(p, files) {
                Ok(proposed) => Ok(Receipt::Proposed(proposed)),
                Err(applied) => Err(applied
                    .stopped
                    .map_or(Outcome::Unread(crate::Unread::Inconsistent), |(_, o)| o)),
            },
            Write::Tag { name, at } => done(c.tag(name, at)),
            Write::SetStatus { at, status } => done(c.set_status(at, status)),
            Write::NewRelease(new) => c.new_release(new).map(Receipt::Release),
            Write::Upload {
                release,
                name,
                file,
            } => done(c.upload(release, name, file, files)),
            Write::PublishRelease { release } => done(c.publish_release(release)),
            Write::Unknown => Err(Outcome::Refused(Refusal::Invalid)),
        }
    }

    /// False: a `Writer` sends every write. A read-only host's writer is
    /// the guard's, above it.
    fn read_only(&self) -> bool {
        false
    }
}
