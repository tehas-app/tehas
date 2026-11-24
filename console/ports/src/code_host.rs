//! The code host port: changes, branches, files, checks, merges, tags,
//! releases and the git remote, and its two traits, `CodeHost` and
//! `CodeHostWrites`.

use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};

use crate::mark::unknown_tag;
use crate::{
    unknown, Account, AccountId, Applied, At, Blobs, BlueprintNo, Body, Bound, Branch, ChangeNo,
    ChangeToken, Cites, Comment, CommentId, CommitId, Cursor, Did, FileName, GitRemote, Got, Mark,
    Marker, MarkerEdit, Posted, ProjectName, Ref, Relation, ReleaseId, RepoPath, Reports, Rev,
    Secret, Sum, TagName, Token, Took, WebLink, Whole, Window,
};

/// What the code host says of the project. `tracker_on`: the code host's
/// own tracker is on, so it can hold blueprints (and tickets, where they
/// come from it).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Facts {
    /// The project's current name, after any rename or move. Where the
    /// site holds it to be the project the handle was made for, it is the
    /// configured spelling, so a name that differs means the project
    /// moved. The adapter folds the site's case.
    pub name: ProjectName,
    /// Anyone may read it.
    pub public: bool,
    /// The code host's own tracker is on.
    pub tracker_on: bool,
    /// It is archived.
    pub archived: bool,
    /// The branch changes go into by default.
    pub default_branch: Branch,
}

new!(
    Facts {
        name: ProjectName,
        default_branch: Branch
    },
    public,
    tracker_on,
    archived
);

/// Whether a change can merge now. A conflict wins: a change whose
/// branch conflicts with its base is `No(Conflict)`, whatever else holds
/// it. While the site has not worked out whether it conflicts, the answer
/// is `Unknown`, never another reason, so a conflicting draft never reads
/// as `No(Draft)` and is asked again.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mergeable {
    /// It can.
    Yes,
    /// It cannot, for this reason; `Conflict` before every other.
    No(NotMergeable),
    /// The site has not worked it out yet, a conflict among it. The
    /// default.
    #[default]
    Unknown,
}

/// Why a change cannot merge now, in the words both code hosts have.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum NotMergeable {
    /// Its branch conflicts with the base.
    Conflict,
    /// Its branch is behind the base.
    Behind,
    /// Its checks fail.
    ChecksFailing,
    /// It wants a review.
    ReviewWanted,
    /// It is a draft.
    Draft,
    /// A rule of the site holds it.
    Blocked,
    /// A reason this copy does not know.
    #[serde(other)]
    Unknown,
}

/// Whether a change is open, merged or closed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChangeState {
    /// Open.
    Open,
    /// Merged.
    Merged {
        /// When.
        at: At,
        /// Who merged it, if the site says.
        by: Option<Account>,
        /// The commit it merged as. None when the site holds no merge
        /// commit for it (an old or rewritten history): a reader that
        /// needs a base to diff fails closed.
        commit: Option<CommitId>,
    },
    /// Closed unmerged.
    Closed {
        /// When.
        at: At,
    },
}

/// The size of a change.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Size {
    /// How many files it changes.
    pub files: u32,
    /// How many lines it adds.
    pub added: u32,
    /// How many lines it deletes.
    pub deleted: u32,
}

/// A change's fields without its files, reviews or comments.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ChangeDigest {
    /// The change.
    pub no: ChangeNo,
    /// Its title.
    pub title: String,
    /// Open, merged or closed.
    pub state: ChangeState,
    /// The branch it merges from.
    pub head: Branch,
    /// The commit at its head.
    pub head_commit: CommitId,
    /// The branch it merges into.
    pub base: Branch,
    /// The base commit the site records for the change, from the same
    /// answer as `head_commit`: what a hook names as the change's base. It
    /// may lag the tip of `base`, which `CodeHost::branch` gives.
    pub base_commit: CommitId,
    /// Who opened it; None where the site names nobody.
    pub author: Option<Account>,
    /// How its author stands to the project, as the site records it with
    /// the change; None where the site does not say. Missing, it reads as
    /// None.
    #[serde(default)]
    pub author_relation: Option<Relation>,
    /// Its head is in another project.
    pub from_fork: bool,
    /// It is a draft.
    pub draft: bool,
    /// Whether it can merge now.
    pub merge_state: Mergeable,
    /// The marks it carries.
    pub marks: Vec<Mark>,
    /// The blueprints this change says it closes, as the site records it.
    /// A site may record none for a change into a branch other than the
    /// default: `says_closes` reads the text.
    pub closes: Vec<Ref<BlueprintNo>>,
    /// The blueprints the change's text names with the site's closing
    /// syntax, whatever its base. Given when the filter asks for bodies,
    /// empty otherwise. The adapter owns the syntax. Missing, it reads as
    /// empty.
    #[serde(default)]
    pub says_closes: Vec<Ref<BlueprintNo>>,
    /// Its size, if the site says.
    pub size: Option<Size>,
    /// Given when the filter asks for bodies. Without our markers, as every
    /// body.
    pub body: Option<String>,
    /// When it was opened.
    pub created: At,
    /// When the site last saw the change change, as it records it: at or
    /// after `created`, `edited` and its merge or close. A caller compares
    /// it with a time it stored.
    pub updated: At,
    /// When its title or body was last edited; None when unedited.
    pub edited: Option<At>,
    /// The last account to edit the change's title or body; None when
    /// unedited, or where the site names nobody.
    pub editor: Option<Account>,
    /// Where a person opens it.
    pub link: WebLink,
}

new!(
    ChangeDigest {
        no: ChangeNo,
        state: ChangeState,
        head: Branch,
        head_commit: CommitId,
        base: Branch,
        base_commit: CommitId,
        created: At,
        updated: At,
        link: WebLink
    },
    title,
    author,
    author_relation,
    from_fork,
    draft,
    merge_state,
    marks,
    closes,
    says_closes,
    size,
    body,
    edited,
    editor
);

/// A change, read whole.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Change {
    /// Its fields.
    pub digest: ChangeDigest,
    /// Its body, without our markers.
    pub body: String,
    /// The files it changes.
    pub files: Vec<ChangedFile>,
    /// Every review, not the latest of each reviewer: a consumer takes the
    /// latest per reviewer and reads `Verdict::Unknown` as no approval
    /// (Decided 112).
    pub reviews: Vec<Review>,
    /// Its comments.
    pub comments: Vec<Comment>,
}

new!(
    Change {
        digest: ChangeDigest
    },
    body,
    files,
    reviews,
    comments
);

/// One file a change changes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ChangedFile {
    /// The file.
    pub path: RepoPath,
    /// How many lines it adds.
    pub added: u32,
    /// How many lines it deletes.
    pub deleted: u32,
}

new!(ChangedFile { path: RepoPath }, added, deleted);

/// A review of a change.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Review {
    /// Who reviewed; None where the site names nobody.
    pub by: Option<Account>,
    /// What the review says.
    pub verdict: Verdict,
    /// When.
    pub at: At,
    /// Its notes.
    pub notes: Vec<ReviewNote>,
}

new!(
    Review {
        verdict: Verdict,
        at: At
    },
    by,
    notes
);

/// What a review says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Verdict {
    /// It approves the change.
    Approved,
    /// It asks for changes.
    ChangesAsked,
    /// It comments only.
    Commented,
    /// A verdict this copy does not know, and a review the site dismissed.
    /// Never an approval.
    #[serde(other)]
    Unknown,
}

/// One note of a review, on a file or on the whole change.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ReviewNote {
    /// Who wrote it; None where the site names nobody.
    pub by: Option<Account>,
    /// The file it is on, if any.
    pub path: Option<RepoPath>,
    /// Its text.
    pub body: String,
}

/// One event of a change.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ChangeEvent {
    /// What happened.
    pub what: ChangeHappened,
    /// Who; None where the site names nobody.
    pub by: Option<Account>,
    /// When, as the site records it. For a push whose own time the site
    /// does not record, the newest commit's time, as `committed`: so `at`
    /// does not order events. The list's order does (Decided 126).
    pub at: At,
    /// For a push, when its newest commit was made, which may be long before.
    pub committed: Option<At>,
}

new!(
    ChangeEvent {
        what: ChangeHappened,
        at: At
    },
    by,
    committed
);

/// What happened to a change.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
#[non_exhaustive]
pub enum ChangeHappened {
    /// Commits were pushed.
    Pushed,
    /// A comment was written.
    Commented,
    /// A review was given.
    Reviewed,
    /// A mark went on.
    Marked(Mark),
    /// A mark came off.
    Unmarked(Mark),
    /// It was merged.
    Merged,
    /// It was closed.
    Closed,
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

/// Which states of change a filter takes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChangeStates {
    /// Open changes.
    #[default]
    Open,
    /// Merged changes.
    Merged,
    /// Changes closed unmerged.
    Closed,
    /// Every change.
    All,
}

/// Whose changes a filter takes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Author {
    /// This handle's account.
    Me,
    /// This account.
    Account(AccountId),
}

/// Every field narrows; the default is every open change.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChangeFilter {
    /// The states taken.
    pub states: ChangeStates,
    /// Changes by this author.
    pub author: Option<Author>,
    /// Changes from this branch.
    pub head: Option<Branch>,
    /// Changes carrying every one of these marks. Empty narrows nothing,
    /// as in tickets' `Filter`. Missing, it reads as empty.
    #[serde(default)]
    pub carrying: Vec<Mark>,
    /// Changes that close one of these blueprints. Empty narrows nothing.
    pub closing: Vec<BlueprintNo>,
    /// Changes merged since a time.
    pub merged_since: Option<At>,
    /// Changes updated since a time.
    pub updated_since: Option<At>,
    /// These changes alone, in one question where the site allows. Empty
    /// narrows nothing.
    pub numbers: Vec<ChangeNo>,
    /// Give each digest its body.
    pub bodies: bool,
}

/// Changes and their comments changed since a cursor, with their bodies.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ChangeDelta {
    /// The changes that changed.
    pub changes: Vec<ChangeDigest>,
    /// The comments on changes written or edited since the cursor, each
    /// with its change, beside the tickets' notes of `TicketDelta`: the
    /// conversation of a change, read with the change's own cursor. A
    /// comment of a ticket or a blueprint is never here. Missing, it reads
    /// as empty.
    #[serde(default)]
    pub comments: Vec<(ChangeNo, Comment)>,
    /// Where the next delta starts: after the last change it holds.
    pub next: Cursor,
    /// More changes wait past `next`: the bound cut this delta. Missing,
    /// it reads as false, as `new` leaves it.
    #[serde(default)]
    pub more: bool,
}

new!(ChangeDelta { next: Cursor }, changes, comments, more);

/// The checks on a commit, and what they add up to: its check runs and
/// its status contexts, and `Overall::of` every one of them.
///
/// The runs are the newest of each App and name, as a site's "latest"
/// filter gives them: runs of different Apps are never collapsed,
/// so two Apps' runs of one name are both held. A `Check` names no App, so
/// a name may stand more than once here.
///
/// A status context is held as one run named by its context, from its
/// newest state: success is `Done(Passed)`, failure and error are
/// `Done(Failed)`, and pending is not done (`Running`). It is never
/// collapsed with a check run of the same name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Checks {
    /// Each check.
    pub runs: Vec<Check>,
    /// What they add up to.
    pub overall: Overall,
}

/// One check on a commit: a check run, or a status context held as a
/// run named by its context (see `Checks`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Check {
    /// Its name: a run's name, or a status's context. Runs of different
    /// Apps may share it.
    pub name: String,
    /// How far it is.
    pub status: CheckStatus,
}

new!(Check {
    name: String,
    status: CheckStatus
});

/// How far a check is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CheckStatus {
    /// Waiting to run.
    Queued,
    /// Running.
    Running,
    /// Done, with this conclusion.
    Done(Conclusion),
}

/// The words both code hosts have. The adapter folds its site's into
/// these: a success or a neutral end is `Passed`; a skip is `Skipped`; a
/// failure, a time out and a run that could not start are `Failed`; a
/// cancel is `Cancelled`; anything else, and a finished run that names no
/// conclusion, is `Unknown`, never a pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Conclusion {
    /// It passed.
    Passed,
    /// It failed.
    Failed,
    /// It was cancelled.
    Cancelled,
    /// It was skipped.
    Skipped,
    /// A conclusion this copy does not know.
    #[serde(other)]
    Unknown,
}

/// What the checks on a commit add up to, by `Overall::of`. Never `Pass`
/// from an answer that could not be read: that is `Unread`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Overall {
    /// Every check passed or was skipped.
    Pass,
    /// A check failed, whatever the others are.
    Fail,
    /// None failed and a check has not finished.
    Pending,
    /// There are no checks.
    #[default]
    None,
    /// Every check finished, none failed, and not every one passed: one
    /// was cancelled or ended in a word the site does not fold. What ended
    /// them is not known, so it is no pass.
    Unknown,
}

impl Overall {
    /// What the runs add up to, over every one given: the newest run of
    /// each App and name and each status context's newest state. None is
    /// `None`; a `Failed` one is `Fail`; every one `Passed` or `Skipped` is
    /// `Pass`; every one done otherwise is `Unknown`; else `Pending`.
    pub fn of(runs: &[Check]) -> Overall {
        if runs.is_empty() {
            return Overall::None;
        }
        let done = |r: &Check| match r.status {
            CheckStatus::Done(c) => Some(c),
            CheckStatus::Queued | CheckStatus::Running => None,
        };
        if runs.iter().any(|r| done(r) == Some(Conclusion::Failed)) {
            Overall::Fail
        } else if runs
            .iter()
            .all(|r| matches!(done(r), Some(Conclusion::Passed | Conclusion::Skipped)))
        {
            Overall::Pass
        } else if runs.iter().all(|r| done(r).is_some()) {
            Overall::Unknown
        } else {
            Overall::Pending
        }
    }
}

/// A status tehas posts on a commit, as a plugin's hook does. `link` is
/// the one `WebLink` a port takes: shown beside the status, never called.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Status {
    /// Its name. The newest status of a name on a commit wins.
    pub name: String,
    /// Its state.
    pub state: StatusState,
    /// One line about it.
    pub about: String,
    /// A link shown beside it, never called.
    pub link: Option<WebLink>,
}

/// The state of a status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StatusState {
    /// Not finished.
    Pending,
    /// Passed.
    Passed,
    /// Failed.
    Failed,
    /// Ended in error.
    Errored,
}

/// How one commit stands to another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Compared {
    /// The same commit.
    Identical,
    /// The head is this many commits behind the base.
    Behind(u32),
    /// The head is this many commits ahead of the base.
    Ahead(u32),
    /// Each has commits the other has not.
    Diverged,
}

/// A change to open.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NewChange {
    /// The branch it merges from.
    pub head: Branch,
    /// The branch it merges into.
    pub base: Branch,
    /// Its title.
    pub title: String,
    /// Its body.
    pub body: Body,
    /// Open it as a draft.
    pub draft: bool,
    /// The blueprints this change closes when it merges. The adapter writes
    /// them in its site's syntax, apart from `body`, and keeps them when an
    /// `edit_change` replaces the body.
    pub closes: Vec<BlueprintNo>,
}

/// How a change merges. It may grow: an adapter's match on it has an arm
/// for the rest, and that arm refuses the merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum MergeHow {
    /// A merge commit.
    Merge,
    /// The change's commits as one commit.
    Squash,
    /// Its commits on top of the base.
    Rebase,
    /// The way the site's settings for the project make the default: a
    /// caller that keeps no rule of its own leaves it to the project.
    Site,
    /// A way this copy does not know: `Refused(Invalid)` before any
    /// request, never written.
    #[serde(other)]
    Unknown,
}

/// Files committed on a fresh branch from `base`, and a change opened for
/// them. Asked twice with the same inputs, it answers the same change while
/// that change is open or merged. Once it was closed unmerged, the next ask
/// moves `branch` to a fresh commit of the same files from `base` and opens
/// a new change; the closed one is never reopened or answered again. The
/// branch is the proposal's own then, so `takeover` is not asked. The
/// files are named by sum; their bytes come from the caller's `Blobs`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Proposal {
    /// The commit the branch starts from.
    pub base: CommitId,
    /// The branch.
    pub branch: Branch,
    /// The files, each by its sum.
    pub files: Vec<(RepoPath, Sum)>,
    /// The commit's message, written as given, with one line end after it.
    /// One commit, with `base` its one parent.
    pub message: String,
    /// The change to open.
    pub change: NewChange,
    /// What to do when `branch` is already there and is not this proposal.
    pub takeover: Takeover,
}

/// One part of a `Proposal`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProposalPart {
    /// The branch.
    Branch,
    /// The commit.
    Commit,
    /// The change.
    Change,
}

/// The caller's rule for a branch that is already there, applied by the
/// adapter as written, so the adapter decides nothing of its own.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
#[non_exhaustive]
pub enum Takeover {
    /// Never move it: `Refused(Exists)`.
    Never,
    /// Move it when every commit on it past `base`'s history is by this
    /// handle's account and no open change uses it: an earlier proposal of
    /// the caller's own that was left. Else `Refused(Exists)`.
    OwnWithoutChange,
    /// Move it when exactly one commit on it is past `base`'s history, as
    /// `OwnWithoutChange` counts them, that commit has one parent, its
    /// message is `message`, and no open change uses it, whoever wrote the
    /// commit: a proposal left by one account and pressed again by another.
    /// `base` may have moved on since that commit was made: its parent is
    /// then an older commit of `base`'s history. The two messages compare
    /// as bytes once white space at the end of each (spaces, tabs, line
    /// ends) is taken off; nothing else is folded, not case, not inner
    /// white space, not a trailer. A caller that passes its
    /// `Proposal.message` matches the commit `propose` left. Else
    /// `Refused(Exists)`.
    SameShape {
        /// The message the one commit must have.
        message: String,
    },
    /// A rule this copy does not know: `Refused(Invalid)` before any request.
    #[serde(
        untagged,
        deserialize_with = "unknown::read_any",
        serialize_with = "unknown_tag"
    )]
    Unknown,
}

/// What `propose` made: the change, and the head commit it opened at, so a
/// caller merges at that head without a read that races an outside push.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Proposed {
    /// The change.
    pub change: Posted<ChangeNo>,
    /// The commit it opened at.
    pub head: CommitId,
}

new!(Proposed { change: Posted<ChangeNo>, head: CommitId });

/// A tag of the project.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Tag {
    /// Its name.
    pub name: TagName,
    /// The commit it names, peeled.
    pub commit: CommitId,
    /// When the tag itself was made, if the site says: an annotated tag's
    /// own date, never its commit's; None for a tag the site keeps no time
    /// for (Decided 128).
    pub at: Option<At>,
    /// It is an annotated tag.
    pub annotated: bool,
}

new!(
    Tag {
        name: TagName,
        commit: CommitId
    },
    at,
    annotated
);

/// A release of a tag.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Release {
    /// The release.
    pub id: ReleaseId,
    /// The tag it is of.
    pub tag: TagName,
    /// Its title.
    pub title: String,
    /// It is a draft.
    pub draft: bool,
    /// It is a prerelease.
    pub prerelease: bool,
    /// When it was published; None for a draft.
    pub published: Option<At>,
    /// Its notes, without our markers; `""` when it has none. Missing, it
    /// reads as `""`.
    #[serde(default)]
    pub notes: String,
    /// Its files.
    pub files: Vec<ReleaseFile>,
    /// Where a person opens it.
    pub link: WebLink,
}

new!(
    Release {
        id: ReleaseId,
        tag: TagName,
        link: WebLink
    },
    title,
    draft,
    prerelease,
    published,
    notes,
    files
);

/// One file of a release.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ReleaseFile {
    /// Its name.
    pub name: FileName,
    /// Its size in bytes.
    pub size: u64,
}

new!(ReleaseFile {
    name: FileName,
    size: u64
});

/// A release to make.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NewRelease {
    /// The tag it is of.
    pub tag: TagName,
    /// Its title.
    pub title: String,
    /// Its notes.
    pub notes: Body,
    /// Make it as a draft.
    pub draft: bool,
    /// Mark it as a prerelease.
    pub prerelease: bool,
}

/// What git is to do at a remote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Access {
    /// Fetch.
    Fetch,
    /// Push.
    Push,
}

/// Where git fetches or pushes, and a credential narrowed to it. None for an
/// anonymous fetch. Holds a credential, so it is no data type.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Remote {
    /// The remote.
    pub url: GitRemote,
    /// The credential for it, if any.
    pub with: Option<GitCredential>,
}

new!(Remote { url: GitRemote }, with);

/// A token for an `https` remote, or a deploy key for an `ssh` one, as the
/// actor's credential is.
#[derive(Debug, Clone)]
pub enum GitCredential {
    /// A token, for `https`.
    Token(Token),
    /// A private key, for `ssh`.
    Key(Secret),
}

/// The code host port's reads. A handle is bound to one project, one actor
/// and one caller name.
pub trait CodeHost: Reports + Cites {
    /// What the code host says of the project.
    fn facts(&mut self) -> Got<Facts>;
    /// The commit a branch points at; None when there is no such branch.
    fn branch(&mut self, name: &Branch) -> Got<Option<CommitId>>;
    /// How `head` stands to `base`.
    fn compare(&mut self, base: &CommitId, head: &CommitId) -> Got<Compared>;
    /// A file's bytes at a revision, at most `limit`.
    fn file(&mut self, path: &RepoPath, at: &Rev, limit: u64) -> Got<Vec<u8>>;
    /// The changes the filter takes, whole.
    fn changes(&mut self, filter: &ChangeFilter, bound: Bound) -> Got<Whole<ChangeDigest>>;
    /// One change, whole.
    fn change(&mut self, no: ChangeNo, bound: Bound) -> Got<Change>;
    /// The files a change changes, whole.
    fn changed_files(&mut self, no: ChangeNo, bound: Bound) -> Got<Whole<ChangedFile>>;
    /// A change's events, oldest first in the site's order: every kind the
    /// site records. An adapter that cannot give a kind refuses the read
    /// `Refused(Unsupported)`; it never leaves one out (Decided 124). `at`
    /// does not order them; `more` may be true where only items the
    /// adapter leaves out remain (Decided 126).
    fn activity(&mut self, no: ChangeNo, window: Window) -> Got<Took<ChangeEvent>>;
    /// The checks on a commit, or those of this name: its check runs, the
    /// newest of each App and name, so an old failure beside a passing
    /// re-run is not read, and its status contexts, each one's newest state
    /// held as a run named by its context. Runs of different Apps, or of one
    /// App in different suites (on GitHub, workflows), are never collapsed:
    /// two such runs of one name are both given (Decided 130). `overall` is
    /// `Overall::of` all of them, so a failed status beside a passing run
    /// is `Fail`. `Checks` gives the fold of a status.
    fn checks(&mut self, at: &CommitId, name: Option<&str>) -> Got<Checks>;
    /// The project's tags, whole, in the site's order: no order is
    /// promised, so a caller that wants the newest asks `recent_tags`.
    fn tags(&mut self, bound: Bound) -> Got<Whole<Tag>>;
    /// The newest `first` tags, with `more`: newest first by the time each
    /// was made, ties and tags with no time in the site's order. A caller
    /// that reads one page of the newest asks this, never `tags`.
    fn recent_tags(&mut self, first: NonZeroU32) -> Got<Took<Tag>>;
    /// The commit a tag names, an annotated tag peeled.
    fn resolve_tag(&mut self, name: &TagName) -> Got<CommitId>;
    /// The project's releases, whole, drafts among them, in the site's
    /// order: no order is promised, as in `tags`.
    fn releases(&mut self, bound: Bound) -> Got<Whole<Release>>;
    /// The newest `first` releases, drafts among them, with `more`: newest
    /// first by when the site made each, a draft too, not by when it was
    /// published.
    fn recent_releases(&mut self, first: NonZeroU32) -> Got<Took<Release>>;
    /// The newest published release, if any.
    fn latest_release(&mut self) -> Got<Option<Release>>;
    /// The release of a tag.
    fn release(&mut self, tag: &TagName) -> Got<Release>;
    /// One release file into `into`, within a size and time limit. The
    /// credential goes to the site's own hosts only.
    fn download(
        &mut self,
        tag: &TagName,
        file: &FileName,
        limit: u64,
        into: &mut dyn std::io::Write,
    ) -> Did;
    /// The source at a commit as a tarball.
    fn archive(&mut self, at: &CommitId, limit: u64, into: &mut dyn std::io::Write) -> Did;
    /// Has anything changed since this token.
    fn changed(&mut self, since: Option<&ChangeToken>) -> Got<(bool, ChangeToken)>;
    /// Changes and their comments changed since the cursor, with bodies,
    /// each list oldest first. `bound` counts both lists together. Past
    /// it, a page of the first changes with `more`, and a `next` that
    /// resumes after the last change of either list: never `Unread(Cut)`.
    /// A page with `more` holds at most `bound` changes and never none; it
    /// may hold fewer.
    fn delta(&mut self, from: Option<&Cursor>, bound: Bound) -> Got<ChangeDelta>;
    /// The remote for git, with a credential for this handle's actor.
    fn remote(&mut self, access: Access) -> Got<Remote>;
}

/// The code host port's writes.
pub trait CodeHostWrites: CodeHost {
    /// Open a change.
    fn open_change(&mut self, new: &NewChange) -> Got<Posted<ChangeNo>>;
    /// The title and body. Keeps `closes`, and the markers not named in
    /// `markers`.
    fn edit_change(
        &mut self,
        no: ChangeNo,
        title: Option<&str>,
        body: Option<&str>,
        markers: &MarkerEdit,
    ) -> Got<WebLink>;
    /// Put a mark on a change.
    fn mark_change(&mut self, no: ChangeNo, mark: &Mark) -> Did;
    /// Take a mark off a change.
    fn unmark_change(&mut self, no: ChangeNo, mark: &Mark) -> Did;
    /// Write a comment on a change, its markers beside it.
    fn comment_change(
        &mut self,
        no: ChangeNo,
        body: &str,
        markers: &[Marker],
    ) -> Got<Posted<CommentId>>;
    /// Close a change unmerged.
    fn close_change(&mut self, no: ChangeNo) -> Did;
    /// Merge only if the head is still `head`: `Conflict(HeadMoved)` if not.
    fn merge(&mut self, no: ChangeNo, head: &CommitId, how: MergeHow) -> Did;
    /// The whole bump, idempotent by its inputs while its change is open
    /// or merged (see `Proposal`). A branch already there is handled by
    /// `p.takeover`. Answers the change and its head.
    fn propose(
        &mut self,
        p: &Proposal,
        files: &dyn Blobs,
    ) -> Result<Proposed, Applied<ProposalPart>>;
    /// Same tag on the same commit is done; elsewhere `Conflict(Elsewhere)`.
    fn tag(&mut self, name: &TagName, at: &CommitId) -> Did;
    /// The newest status of this name on a commit wins.
    fn set_status(&mut self, at: &CommitId, status: &Status) -> Did;
    /// A release of a tag, as a draft or published. The same tag twice is
    /// the same release.
    fn new_release(&mut self, new: &NewRelease) -> Got<Posted<ReleaseId>>;
    /// One file of a release, its bytes from `files` by its sum. The same
    /// name with the same length is done; with another, `Refused(Exists)`.
    fn upload(
        &mut self,
        release: &ReleaseId,
        name: &FileName,
        file: &Sum,
        files: &dyn Blobs,
    ) -> Did;
    /// A draft made public. Already published is done.
    fn publish_release(&mut self, release: &ReleaseId) -> Did;
}
