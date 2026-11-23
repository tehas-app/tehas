//! The blueprints port: what a cycle works from, its text, comments with
//! their editors, edit histories and attachments, and its two traits,
//! `Blueprints` and `BlueprintWrites`.

use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};

use crate::{
    Account, At, AttachmentId, BlueprintNo, Body, Bound, ChangeToken, Cites, CommentId, Cursor,
    Depth, Did, Got, Marker, MarkerEdit, Posted, Reports, Revision, Took, WebLink, Whole, Within,
};

/// A blueprint, read to the depth asked.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Blueprint {
    /// The blueprint.
    pub no: BlueprintNo,
    /// Its title.
    pub title: String,
    /// Without our markers: they are in `markers`.
    pub body: String,
    /// It is open.
    pub open: bool,
    /// Who opened it; None where the site names nobody.
    pub author: Option<Account>,
    /// The last account to edit the body; None when unedited.
    pub editor: Option<Account>,
    /// When it was opened.
    pub created: At,
    /// When the body was last edited; None when unedited.
    pub edited: Option<At>,
    /// What it looked like when read.
    pub revision: Revision,
    /// Our markers in the body, as the adapter read them.
    pub markers: Vec<Marker>,
    /// Oldest first, newest last. `Depth::Last` gives the newest n whole,
    /// with `more`; `Depth::Cut` the last few, cut; `Depth::All` every one,
    /// `more` false.
    pub comments: Took<Comment>,
    /// Where a person opens it.
    pub link: WebLink,
}

new!(
    Blueprint {
        no: BlueprintNo,
        created: At,
        revision: Revision,
        link: WebLink
    },
    title,
    body,
    open,
    author,
    editor,
    edited,
    markers,
    comments
);

/// A comment, or a note on a ticket, with its author and last editor.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Comment {
    /// The comment.
    pub id: CommentId,
    /// Who wrote it; None where the site names nobody.
    pub author: Option<Account>,
    /// None when unedited. An answer or a marker counts from this account.
    pub editor: Option<Account>,
    /// When it was written.
    pub created: At,
    /// When it was last edited; None when unedited.
    pub edited: Option<At>,
    /// Without our markers: they are in `markers`.
    pub body: String,
    /// Our markers in it, as the adapter read them.
    pub markers: Vec<Marker>,
    /// What it looked like when read.
    pub revision: Revision,
    /// Where a person opens it.
    pub link: WebLink,
}

new!(
    Comment {
        id: CommentId,
        created: At,
        revision: Revision,
        link: WebLink
    },
    author,
    editor,
    edited,
    body,
    markers
);

/// One edit of a body or a comment: who, when, the text after it without
/// our markers, and the markers after it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Edit {
    /// Who edited; None where the site names nobody.
    pub by: Option<Account>,
    /// When.
    pub at: At,
    /// The text after the edit, without our markers.
    pub after: String,
    /// Our markers after the edit.
    pub markers: Vec<Marker>,
}

new!(Edit { at: At }, by, after, markers);

/// A file a blueprint's text links to on the site's own store.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Attachment {
    /// The attachment.
    pub id: AttachmentId,
    /// The link in the text, as it stands there: what a manifest records
    /// and how a changed attachment is told apart.
    pub link: WebLink,
    /// Its name, as the site gives it. None for a file pasted into the
    /// text that the site only ever stores as an image: the caller types it
    /// as an image, not by an extension.
    pub name: Option<String>,
    /// Its size in bytes, if the site says.
    pub size: Option<u64>,
}

new!(
    Attachment {
        id: AttachmentId,
        link: WebLink
    },
    name,
    size
);

/// A blueprint to open.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NewBlueprint {
    /// Its title.
    pub title: String,
    /// Its body.
    pub body: Body,
    /// Our markers, beside the body.
    pub markers: Vec<Marker>,
}

/// One hit of a search of blueprints.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct BlueprintHit {
    /// The blueprint.
    pub no: BlueprintNo,
    /// Its title.
    pub title: String,
    /// It is open.
    pub open: bool,
}

new!(BlueprintHit { no: BlueprintNo }, title, open);

/// A blueprint's change since a cursor, whole but for its comments.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct BlueprintChange {
    /// The blueprint.
    pub no: BlueprintNo,
    /// Its title.
    pub title: String,
    /// Its body, without our markers.
    pub body: String,
    /// It is open.
    pub open: bool,
    /// When it closed, if it is closed.
    pub closed: Option<At>,
    /// What it looked like when read.
    pub revision: Revision,
}

new!(
    BlueprintChange {
        no: BlueprintNo,
        revision: Revision
    },
    title,
    body,
    open,
    closed
);

/// Blueprints and comments changed since a cursor, and where to read on
/// from.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct BlueprintDelta {
    /// The blueprints that changed.
    pub blueprints: Vec<BlueprintChange>,
    /// The comments that changed, each with its blueprint.
    pub comments: Vec<(BlueprintNo, Comment)>,
    /// Where the next delta starts: after the last change it holds.
    pub next: Cursor,
    /// More changes wait past `next`: the bound cut this delta. Missing,
    /// it reads as false, as `new` leaves it.
    #[serde(default)]
    pub more: bool,
}

new!(BlueprintDelta { next: Cursor }, blueprints, comments, more);

/// The blueprints port's reads. A handle is bound to one project, one
/// actor and one caller name.
pub trait Blueprints: Reports + Cites {
    /// One blueprint, to the depth asked. A number that names a change,
    /// where they share one number space, is `NotFound`, never a
    /// `Blueprint`: a caller that must tell the two apart asks
    /// `CodeHost::change` after.
    fn blueprint(&mut self, no: BlueprintNo, depth: Depth) -> Got<Blueprint>;
    /// Several blueprints, each with its own answer; a change's number is
    /// `NotFound`, as in `blueprint`.
    fn several(&mut self, nos: &[BlueprintNo], depth: Depth) -> Vec<(BlueprintNo, Got<Blueprint>)>;
    /// A blueprint's comments, whole.
    fn comments(&mut self, no: BlueprintNo, bound: Bound) -> Got<Whole<Comment>>;
    /// Comments by id, each with its own answer.
    fn comments_by_id(&mut self, ids: &[CommentId]) -> Vec<(CommentId, Got<Comment>)>;
    /// The edits of a body, oldest first: the approval of a changed text.
    fn body_edits(&mut self, no: BlueprintNo, bound: Bound) -> Got<Whole<Edit>>;
    /// The edits of a comment, oldest first.
    fn comment_edits(&mut self, id: &CommentId, bound: Bound) -> Got<Whole<Edit>>;
    /// The first `first` hits by relevance, with `more`, words only,
    /// escaped by the adapter, as `Tickets::search`.
    fn search(
        &mut self,
        words: &str,
        within: Within,
        open: Option<bool>,
        first: NonZeroU32,
    ) -> Got<Took<BlueprintHit>>;
    /// The files the blueprint's text links to on the site's own store, by
    /// the site's rule for its links.
    fn attachments(&mut self, no: BlueprintNo) -> Got<Whole<Attachment>>;
    /// One attachment into `into`, at most `limit` bytes.
    fn fetch(&mut self, a: &AttachmentId, limit: u64, into: &mut dyn std::io::Write) -> Did;
    /// Has anything changed since this token.
    fn changed(&mut self, since: Option<&ChangeToken>) -> Got<(bool, ChangeToken)>;
    /// Blueprints and comments changed since the cursor, each list oldest
    /// first. `bound` counts both lists together. Past it, a page of the
    /// first changes with `more`, and a `next` that resumes after the last
    /// change of either list: never `Unread(Cut)`. A page with `more` holds
    /// at most `bound` changes and never none; it may hold fewer.
    fn delta(&mut self, from: Option<&Cursor>, bound: Bound) -> Got<BlueprintDelta>;
}

/// The blueprints port's writes.
pub trait BlueprintWrites: Blueprints {
    /// Open a new blueprint.
    fn open(&mut self, new: &NewBlueprint) -> Got<Posted<BlueprintNo>>;
    /// The title and body. `Conflict(Changed)` if no longer at `since`.
    /// Markers not named in `markers` are kept.
    fn edit(
        &mut self,
        no: BlueprintNo,
        title: Option<&str>,
        body: Option<&str>,
        markers: &MarkerEdit,
        since: &Revision,
    ) -> Got<WebLink>;
    /// Write a comment, its markers beside it.
    fn comment(
        &mut self,
        no: BlueprintNo,
        body: &str,
        markers: &[Marker],
    ) -> Got<Posted<CommentId>>;
    /// Compare then write. Where the site cannot do both in one step, a
    /// window of a moment remains; the adapter's contract test states it.
    /// Markers not named in `markers` are kept: an answer's tick keeps the
    /// question's marker.
    fn edit_comment(
        &mut self,
        id: &CommentId,
        body: &str,
        markers: &MarkerEdit,
        since: &Revision,
    ) -> Got<WebLink>;
    /// Close the blueprint, done or not.
    fn close(&mut self, no: BlueprintNo, done: bool) -> Did;
}
