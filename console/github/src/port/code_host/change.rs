//! `CodeHost::change`: one pull request read whole. The port defines no
//! depth for it: a change is always its digest, its body, its files, its
//! reviews with their notes and its comments, each list whole up to the
//! caller's bound or `Unread(Cut)`.
//!
//! The requests, in order: the pull request's REST read; the pages of its
//! issue comments, 100 a page, as many as its count of them says, with one
//! GraphQL question for a page after the first that holds a comment whose
//! `updated_at` moved; the pages of its files, as many as its count says
//! (past GitHub's 3000, `Unread(Cut)` with no page read); the pages of its
//! reviews, to the first short page; the pages of its review comments, as
//! many as its count says; and last one GraphQL question for what REST
//! does not give: what it closes, whether its head is a fork, whether it
//! can merge, the last edit of its body and of each comment of the first
//! page whose `updated_at` moved, and its head and `updatedAt` again. With
//! no comment, no file and no review comment that is three requests.
//!
//! One moment: the last question's head and `updatedAt` must be the first
//! read's. GitHub moves a pull request's `updatedAt` on a comment, a
//! review, an edit and a push, so equal at both ends means nothing moved
//! between. Over more than one page of any list the pull request is also
//! read again and its counts must be as they were. A comment's edit later
//! than the `updated_at` its page gave is of a body the page did not hold.
//! Each of these is `Unread(Inconsistent)`, and such a read is made once
//! more.
//!
//! A change's comments are its issue comments, the conversation's. A
//! review's own text is its note with no path; its comments on the diff
//! are its other notes. A review still pending, and its notes, are no
//! review given and are left out.

use std::collections::BTreeSet;

use super::super::blueprint_writes::{comment_revision, ends_with_folded};
use crate::graphql::priced;
use crate::tickets::{account, Bot};
use crate::{gh, markers, GitHub};
use serde::Deserialize;
use serde_json::Value;
use tehas_ports::{
    Account, AccountKind, At, BlueprintNo, Bound, Branch, Change, ChangeDigest, ChangeNo,
    ChangeState, ChangedFile, Comment, CommentId, CommitId, Got, ItemName, Login, Mergeable,
    Outcome, ProjectName, Realm, Ref, Relation, RepoPath, Review, ReviewNote, Size, Unread,
    Verdict, WebLink,
};

/// What one page of a REST list asks for, the most GitHub gives.
const PER_PAGE: usize = 100;

/// The most files GitHub lists of a pull request: past them the list is
/// not whole, `Unread(Cut)`, with no page read.
const FILES_MAX: usize = 3000;

/// The most pages of review comments one read takes: they have no bound of
/// the caller's, and past these the read is `Unread(Cut)`.
const NOTE_PAGES_MAX: usize = 30;

impl GitHub {
    /// The change `no`, read whole, and once more when the read comes out
    /// `Unread(Inconsistent)`: the pull request changed under it. No other
    /// failure is retried here.
    pub(super) fn change_whole(&mut self, no: ChangeNo, bound: Bound) -> Got<Change> {
        match self.change_once(no, bound) {
            Err(Outcome::Unread(Unread::Inconsistent)) => self.change_once(no, bound),
            other => other,
        }
    }

    fn change_once(&mut self, no: ChangeNo, bound: Bound) -> Got<Change> {
        let item = BlueprintNo::new(no.get()).ok_or(Outcome::Unread(Unread::Malformed))?;
        let (pull, stamp) = self.pull(no)?;
        let bound = bound.get() as usize;
        if stamp.comments > bound || stamp.files > bound {
            return Err(Outcome::Unread(Unread::Cut));
        }
        if stamp.files > FILES_MAX {
            return Err(Outcome::Unread(Unread::Cut));
        }
        let mut spread = false;
        let (mut comments, first) = self.change_comments(no, item, &stamp, &mut spread)?;
        let files = self.change_files(no, &stamp, &mut spread)?;
        let mut reviews = self.reviews(no, item, bound, &mut spread)?;
        self.notes(no, item, &stamp, &mut reviews, &mut spread)?;
        if spread && self.pull(no)?.1 != stamp {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        // Asked last: its head and time must still be the first read's, so
        // every part of the answer is of one moment.
        let ids: Vec<String> = first.iter().map(|(_, id, _)| id.clone()).collect();
        let (node, edits) = self.ask_change(no, &ids, true)?;
        let node = node.ok_or(MALFORMED)?;
        if node.head != stamp.head || Some(node.updated) != time(Some(&stamp.updated)).ok() {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        edited(&mut comments, &first, edits)?;
        let mut digest = pull.digest;
        digest.marks = self.marks_of(&pull.labels);
        digest.from_fork = node.fork;
        digest.merge_state = node.merge_state;
        digest.closes = node.closes;
        digest.edited = node.edited;
        digest.editor = node.editor;
        let read = markers::read(&pull.raw, Some(item));
        digest.says_closes = says_closes(&read.text);
        digest.body = Some(read.text.clone());
        let mut change = Change::new(digest);
        change.body = read.text;
        change.files = files;
        change.reviews = reviews.into_iter().map(|(_, r)| r).collect();
        change.comments = comments;
        Ok(change)
    }

    /// The pull request's REST read: its digest with no marks and none of
    /// what GraphQL gives, its labels and raw body, and its stamp. A failed
    /// read is its outcome: an issue's number is `NotFound`, with no
    /// request after it. An answer of another number or project is
    /// `Unread(Inconsistent)`; one that lacks a field every pull request
    /// has, `Unread(Malformed)`.
    fn pull(&mut self, no: ChangeNo) -> Got<(Pull, Stamp)> {
        let path = format!("repos/{}/pulls/{}", self.repo, no.get());
        let out = self
            .call("the pull request", &["api", &path])
            .map_err(|e| self.outcome(&e))?;
        read_pull(&out, &self.repo, no, self.bot.as_ref())
    }

    /// Every issue comment of the pull request, oldest first, and those of
    /// the first page whose `updated_at` moved: their edits are asked in the
    /// read's last question. A later page with a moved comment asks its own
    /// question at once. Each page holds exactly
    /// what the count says, each comment once, on this pull request, its id
    /// rising and its time never falling from the one before; else
    /// `Unread(Inconsistent)`.
    fn change_comments(
        &mut self,
        no: ChangeNo,
        item: BlueprintNo,
        stamp: &Stamp,
        spread: &mut bool,
    ) -> Got<(Vec<Comment>, Vec<Touched>)> {
        let count = stamp.comments;
        let mut all: Vec<Comment> = Vec::with_capacity(count);
        let mut first = Vec::new();
        let mut before: Option<(u64, At)> = None;
        for page in 1..=count.div_ceil(PER_PAGE) {
            *spread |= page > 1;
            let path = format!(
                "repos/{}/issues/{}/comments?per_page={PER_PAGE}&page={page}",
                self.repo,
                no.get()
            );
            let out = self
                .call("the pull request's comments", &["api", &path])
                .map_err(|e| self.outcome(&e))?;
            let held = (count - (page - 1) * PER_PAGE).min(PER_PAGE);
            let (mut comments, touched) =
                read_comments(&out, &self.repo, no, item, self.bot.as_ref())?;
            if comments.len() != held {
                return Err(Outcome::Unread(Unread::Inconsistent));
            }
            if page == 1 {
                first = touched;
            } else if !touched.is_empty() {
                let ids: Vec<String> = touched.iter().map(|(_, id, _)| id.clone()).collect();
                let (_, edits) = self.ask_change(no, &ids, false)?;
                edited(&mut comments, &touched, edits)?;
            }
            for c in comments {
                let id: u64 =
                    c.id.as_str()
                        .parse()
                        .map_err(|_| Outcome::Unread(Unread::Malformed))?;
                if before.is_some_and(|(i, at)| id <= i || c.created < at) {
                    return Err(Outcome::Unread(Unread::Inconsistent));
                }
                before = Some((id, c.created));
                all.push(c);
            }
        }
        Ok((all, first))
    }

    /// One GraphQL question: with `pull`, what the pull request's REST read
    /// does not give; and the last edit of each comment of `ids`, by node
    /// id, answered in their order. Every comment asked must be answered as
    /// an issue comment of that id, or the read is `Unread(Inconsistent)`.
    /// A failed question is the read's failure, a spent budget the priced
    /// pool's.
    fn ask_change(
        &mut self,
        no: ChangeNo,
        ids: &[String],
        pull: bool,
    ) -> Got<(Option<Node>, Vec<Edit>)> {
        let question = change_question(ids, pull);
        let number = pull.then_some(no.get());
        let out = self
            .ask("the pull request's question", &question, number, None)
            .map_err(|e| priced(self.outcome(&e)))?;
        let answer: Answer = answer(&out, (self.now)())?;
        let node = match pull {
            false => None,
            true => Some(
                answer
                    .repository
                    .ok_or(MALFORMED)?
                    .read(no, self.bot.as_ref())?,
            ),
        };
        let nodes = answer.nodes.unwrap_or_default();
        if nodes.len() != ids.len() {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        let mut edits = Vec::with_capacity(ids.len());
        for (id, n) in ids.iter().zip(nodes) {
            let n = n
                .filter(|n| n.id.as_deref() == Some(id.as_str()))
                .filter(|n| n.kind.as_deref() == Some("IssueComment"))
                .ok_or(Outcome::Unread(Unread::Inconsistent))?;
            edits.push(n.edit.read(self.bot.as_ref())?);
        }
        Ok((node, edits))
    }

    /// Every file of the pull request, as many as its count says, each
    /// once; else `Unread(Inconsistent)`.
    fn change_files(
        &mut self,
        no: ChangeNo,
        stamp: &Stamp,
        spread: &mut bool,
    ) -> Got<Vec<ChangedFile>> {
        let count = stamp.files;
        let mut files = Vec::with_capacity(count);
        let mut seen = BTreeSet::new();
        for page in 1..=count.div_ceil(PER_PAGE) {
            *spread |= page > 1;
            let out = self.page(no, "files", page, "the pull request's files")?;
            let read: Vec<FileFields> = parse(&out)?;
            let held = (count - (page - 1) * PER_PAGE).min(PER_PAGE);
            if read.len() != held {
                return Err(Outcome::Unread(Unread::Inconsistent));
            }
            for f in read {
                let name = f.filename.ok_or(MALFORMED)?;
                if !seen.insert(name.clone()) {
                    return Err(Outcome::Unread(Unread::Inconsistent));
                }
                let mut file = ChangedFile::new(RepoPath::new(name).map_err(|_| MALFORMED)?);
                file.added = small(f.additions)?;
                file.deleted = small(f.deletions)?;
                files.push(file);
            }
        }
        Ok(files)
    }

    /// Every review given on the pull request, oldest first, with its id
    /// and its text as its one note so far; to the first page short of
    /// 100. More than `bound` given is `Unread(Cut)`. Each review once, on
    /// this pull request, its id rising; else `Unread(Inconsistent)`.
    fn reviews(
        &mut self,
        no: ChangeNo,
        item: BlueprintNo,
        bound: usize,
        spread: &mut bool,
    ) -> Got<Vec<(u64, Review)>> {
        let mut reviews: Vec<(u64, Review)> = Vec::new();
        let mut last = 0;
        for page in 1.. {
            if page > bound / PER_PAGE + 2 {
                return Err(Outcome::Unread(Unread::Cut));
            }
            *spread |= page > 1;
            let out = self.page(no, "reviews", page, "the pull request's reviews")?;
            let read: Vec<ReviewFields> = parse(&out)?;
            let short = read.len() < PER_PAGE;
            for r in read {
                let id = r.id.ok_or(MALFORMED)?;
                if id <= last || !on_pull(r.pull_request_url.as_deref(), &self.repo, no)? {
                    return Err(Outcome::Unread(Unread::Inconsistent));
                }
                last = id;
                let verdict = match r.state.as_deref().ok_or(MALFORMED)? {
                    "PENDING" => continue,
                    "APPROVED" => Verdict::Approved,
                    "CHANGES_REQUESTED" => Verdict::ChangesAsked,
                    "COMMENTED" => Verdict::Commented,
                    _ => Verdict::Unknown,
                };
                let mut review = Review::new(verdict, time(r.submitted_at.as_deref())?);
                review.by = author(r.user.as_ref(), self.bot.as_ref());
                let text = markers::read(r.body.as_deref().unwrap_or_default(), Some(item)).text;
                if !text.is_empty() {
                    let mut note = ReviewNote::default();
                    note.by = review.by.clone();
                    note.body = text;
                    review.notes.push(note);
                }
                reviews.push((id, review));
                if reviews.len() > bound {
                    return Err(Outcome::Unread(Unread::Cut));
                }
            }
            if short {
                break;
            }
        }
        Ok(reviews)
    }

    /// The review comments of the pull request, as many as its count says,
    /// each a note of the review it belongs to, in the order of their ids.
    /// One of a review not given (pending) is left out with it. Each page
    /// holds what the count says, each comment once, on this pull request,
    /// its id rising; else `Unread(Inconsistent)`.
    fn notes(
        &mut self,
        no: ChangeNo,
        item: BlueprintNo,
        stamp: &Stamp,
        reviews: &mut [(u64, Review)],
        spread: &mut bool,
    ) -> Got<()> {
        let count = stamp.notes;
        let last_page = count.div_ceil(PER_PAGE);
        if last_page > NOTE_PAGES_MAX {
            return Err(Outcome::Unread(Unread::Cut));
        }
        let mut last = 0;
        for page in 1..=last_page {
            *spread |= page > 1;
            let out = self.page(no, "comments", page, "the pull request's review comments")?;
            let read: Vec<NoteFields> = parse(&out)?;
            let held = (count - (page - 1) * PER_PAGE).min(PER_PAGE);
            if read.len() != held {
                return Err(Outcome::Unread(Unread::Inconsistent));
            }
            for n in read {
                let id = n.id.ok_or(MALFORMED)?;
                if id <= last || !on_pull(n.pull_request_url.as_deref(), &self.repo, no)? {
                    return Err(Outcome::Unread(Unread::Inconsistent));
                }
                last = id;
                let Some((_, review)) = reviews
                    .iter_mut()
                    .find(|(r, _)| Some(*r) == n.pull_request_review_id)
                else {
                    continue;
                };
                let mut note = ReviewNote::default();
                note.by = author(n.user.as_ref(), self.bot.as_ref());
                note.path = match n.path {
                    Some(p) => Some(RepoPath::new(p).map_err(|_| MALFORMED)?),
                    None => None,
                };
                note.body = markers::read(n.body.as_deref().unwrap_or_default(), Some(item)).text;
                review.notes.push(note);
            }
        }
        Ok(())
    }

    /// gh's answer to one page of a REST list of the pull request's, or its
    /// outcome.
    fn page(&mut self, no: ChangeNo, list: &str, page: usize, call: &str) -> Got<String> {
        let path = format!(
            "repos/{}/pulls/{}/{list}?per_page={PER_PAGE}&page={page}",
            self.repo,
            no.get()
        );
        self.call(call, &["api", &path])
            .map_err(|e| self.outcome(&e))
    }
}

const MALFORMED: Outcome = Outcome::Unread(Unread::Malformed);

/// What a pull request's read says of the lists the change reads: a
/// comment, a review comment or a push moves one of these. Compared after
/// a read of more than one page.
#[derive(PartialEq, Eq)]
struct Stamp {
    comments: usize,
    notes: usize,
    files: usize,
    updated: String,
    head: String,
}

/// The pull request's REST read, before GraphQL's answer.
struct Pull {
    digest: ChangeDigest,
    labels: Vec<String>,
    /// The body as GitHub keeps it, our markers in.
    raw: String,
}

#[derive(Deserialize)]
struct PullFields {
    number: Option<u64>,
    html_url: Option<String>,
    title: Option<String>,
    body: Option<String>,
    state: Option<String>,
    draft: Option<bool>,
    merged: Option<bool>,
    merged_at: Option<String>,
    closed_at: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
    #[serde(deserialize_with = "crate::ticket::present")]
    merge_commit_sha: Option<String>,
    merged_by: Option<User>,
    user: Option<User>,
    author_association: Option<String>,
    head: Option<Side>,
    base: Option<Side>,
    labels: Option<Vec<Label>>,
    comments: Option<u64>,
    review_comments: Option<u64>,
    additions: Option<u64>,
    deletions: Option<u64>,
    changed_files: Option<u64>,
}

/// A pull request's head or base.
#[derive(Deserialize)]
struct Side {
    #[serde(rename = "ref")]
    branch: Option<String>,
    sha: Option<String>,
}

#[derive(Deserialize)]
struct Label {
    name: Option<String>,
}

/// The account GitHub's REST names as a `user`.
#[derive(Deserialize)]
struct User {
    login: Option<String>,
    id: Option<u64>,
    #[serde(rename = "type")]
    kind: Option<String>,
}

#[derive(Deserialize)]
struct CommentFields {
    id: Option<u64>,
    html_url: Option<String>,
    body: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
    node_id: Option<String>,
    user: Option<User>,
}

#[derive(Deserialize)]
struct FileFields {
    filename: Option<String>,
    additions: Option<u64>,
    deletions: Option<u64>,
}

#[derive(Deserialize)]
struct ReviewFields {
    id: Option<u64>,
    state: Option<String>,
    body: Option<String>,
    submitted_at: Option<String>,
    pull_request_url: Option<String>,
    user: Option<User>,
}

#[derive(Deserialize)]
struct NoteFields {
    id: Option<u64>,
    pull_request_review_id: Option<u64>,
    pull_request_url: Option<String>,
    path: Option<String>,
    body: Option<String>,
    user: Option<User>,
}

/// The pull request's REST answer read. See [`GitHub::pull`].
fn read_pull(out: &str, repo: &str, no: ChangeNo, bot: Option<&Bot>) -> Got<(Pull, Stamp)> {
    let f: PullFields = parse(out)?;
    if f.number != Some(no.get()) {
        return Err(Outcome::Unread(Unread::Inconsistent));
    }
    let url = f.html_url.ok_or(MALFORMED)?;
    if !ends_with_folded(&url, &format!("/{repo}/pull/{}", no.get())) {
        return Err(Outcome::Unread(Unread::Inconsistent));
    }
    let side = |s: Option<&Side>| -> Got<(Branch, CommitId)> {
        let s = s.ok_or(MALFORMED)?;
        Ok((
            Branch::new(s.branch.as_deref().ok_or(MALFORMED)?).map_err(|_| MALFORMED)?,
            CommitId::new(s.sha.as_deref().ok_or(MALFORMED)?).map_err(|_| MALFORMED)?,
        ))
    };
    let (head, head_commit) = side(f.head.as_ref())?;
    let (base, base_commit) = side(f.base.as_ref())?;
    let state = match (f.state.as_deref(), f.merged) {
        (Some("open"), _) => ChangeState::Open,
        (Some("closed"), Some(true)) => ChangeState::Merged {
            at: time(f.merged_at.as_deref())?,
            by: author(f.merged_by.as_ref(), bot),
            commit: f
                .merge_commit_sha
                .as_deref()
                .map(|c| CommitId::new(c).map_err(|_| MALFORMED))
                .transpose()?,
        },
        (Some("closed"), Some(false)) => ChangeState::Closed {
            at: time(f.closed_at.as_deref())?,
        },
        _ => return Err(MALFORMED),
    };
    let updated = f.updated_at.ok_or(MALFORMED)?;
    let mut digest = ChangeDigest::new(
        no,
        state,
        head,
        head_commit.clone(),
        base,
        base_commit,
        time(f.created_at.as_deref())?,
        time(Some(&updated))?,
        WebLink::new(url).map_err(|_| MALFORMED)?,
    );
    digest.title = f.title.ok_or(MALFORMED)?;
    digest.author = author(f.user.as_ref(), bot);
    digest.author_relation = f.author_association.as_deref().map(relation);
    digest.draft = f.draft.ok_or(MALFORMED)?;
    let mut size = Size::default();
    size.files = small(f.changed_files)?;
    size.added = small(f.additions)?;
    size.deleted = small(f.deletions)?;
    digest.size = Some(size);
    let labels = f
        .labels
        .ok_or(MALFORMED)?
        .into_iter()
        .map(|l| l.name)
        .collect::<Option<Vec<_>>>()
        .ok_or(MALFORMED)?;
    let stamp = Stamp {
        comments: count(f.comments)?,
        notes: count(f.review_comments)?,
        files: size.files as usize,
        updated,
        head: head_commit.as_str().to_string(),
    };
    let pull = Pull {
        digest,
        labels,
        raw: f.body.unwrap_or_default(),
    };
    Ok((pull, stamp))
}

/// One page of the pull request's issue comments, each read: its text
/// through `markers::read` as a text of item `no`, its revision the sum of
/// its raw text, markers in; and the comments whose `updated_at` moved,
/// by their place and node id. A comment whose link is not one on this
/// pull request is `Unread(Inconsistent)`.
fn read_comments(
    out: &str,
    repo: &str,
    no: ChangeNo,
    item: BlueprintNo,
    bot: Option<&Bot>,
) -> Got<(Vec<Comment>, Vec<Touched>)> {
    let items: Vec<CommentFields> = parse(out)?;
    let mut comments = Vec::with_capacity(items.len());
    let mut touched = Vec::new();
    for c in items {
        let id = c.id.ok_or(MALFORMED)?;
        let url = c.html_url.ok_or(MALFORMED)?;
        if !ends_with_folded(
            &url,
            &format!("/{repo}/pull/{}#issuecomment-{id}", no.get()),
        ) {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        let raw = c.body.as_deref().unwrap_or_default();
        let created = time(c.created_at.as_deref())?;
        let updated = time(c.updated_at.as_deref())?;
        let read = markers::read(raw, Some(item));
        let sum = comment_revision(raw);
        let link = WebLink::new(url).map_err(|_| MALFORMED)?;
        let mut comment = Comment::new(CommentId::new(id.to_string()), created, sum, link);
        comment.author = author(c.user.as_ref(), bot);
        if updated != created {
            let node = c.node_id.filter(|n| is_node_id(n)).ok_or(MALFORMED)?;
            touched.push((comments.len(), node, updated));
        }
        comment.body = read.text;
        comment.markers = read.markers;
        comments.push(comment);
    }
    Ok((comments, touched))
}

/// A comment of a page whose `updated_at` moved: its place in the page,
/// its node id, and its `updated_at`.
type Touched = (usize, String, At);

/// The edits GraphQL answered set on the comments of a page that moved. An
/// edit later than the comment's `updated_at` is one its body, read before
/// it, does not hold: `Unread(Inconsistent)`.
fn edited(comments: &mut [Comment], touched: &[Touched], edits: Vec<Edit>) -> Got<()> {
    for ((at, _, updated), (when, editor)) in touched.iter().zip(edits) {
        if when.is_some_and(|w| w > *updated) {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        comments[*at].edited = when;
        comments[*at].editor = editor;
    }
    Ok(())
}

/// The GraphQL question of [`GitHub::ask_change`]. It always names the
/// repository, since the adapter's one way to ask gives its owner and name.
fn change_question(ids: &[String], pull: bool) -> String {
    const WHO: &str = "{ __typename login ...on User { databaseId } ...on Bot { databaseId } }";
    let (vars, pull) = match pull {
        true => (
            ", $number: Int!",
            format!(" pullRequest(number: $number) {{ number headRefOid updatedAt isCrossRepository mergeable mergeStateStatus lastEditedAt editor {WHO} closingIssuesReferences(first: 100) {{ totalCount nodes {{ number repository {{ databaseId nameWithOwner }} }} }} }}"),
        ),
        false => ("", String::new()),
    };
    let nodes = match ids.is_empty() {
        true => String::new(),
        false => {
            let quoted: Vec<String> = ids.iter().map(|i| format!("\"{i}\"")).collect();
            format!(
                " nodes(ids: [{}]) {{ __typename ...on IssueComment {{ id lastEditedAt editor {WHO} }} }}",
                quoted.join(", ")
            )
        }
    };
    format!("query($owner: String!, $name: String!{vars}) {{ repository(owner: $owner, name: $name) {{ databaseId{pull} }}{nodes} }}")
}

/// The question's answer.
#[derive(Deserialize)]
struct Answer {
    repository: Option<RepoNode>,
    nodes: Option<Vec<Option<EditedNode>>>,
}

#[derive(Deserialize)]
struct RepoNode {
    #[serde(rename = "databaseId")]
    id: Option<u64>,
    #[serde(rename = "pullRequest")]
    pull: Option<PullNode>,
}

#[derive(Deserialize)]
struct PullNode {
    number: Option<u64>,
    #[serde(rename = "headRefOid")]
    head: Option<String>,
    #[serde(rename = "updatedAt")]
    updated: Option<String>,
    #[serde(rename = "isCrossRepository")]
    fork: Option<bool>,
    mergeable: Option<String>,
    #[serde(rename = "mergeStateStatus")]
    merge_state: Option<String>,
    #[serde(flatten)]
    edit: Edited,
    #[serde(rename = "closingIssuesReferences")]
    closing: Option<Closing>,
}

#[derive(Deserialize)]
struct Closing {
    #[serde(rename = "totalCount")]
    total: Option<u64>,
    nodes: Option<Vec<Closed>>,
}

#[derive(Deserialize)]
struct Closed {
    number: Option<u64>,
    repository: Option<ClosedRepo>,
}

#[derive(Deserialize)]
struct ClosedRepo {
    #[serde(rename = "databaseId")]
    id: Option<u64>,
    #[serde(rename = "nameWithOwner")]
    name: Option<String>,
}

#[derive(Deserialize)]
struct EditedNode {
    #[serde(rename = "__typename")]
    kind: Option<String>,
    id: Option<String>,
    #[serde(flatten)]
    edit: Edited,
}

/// A text's last edit as GraphQL answers it.
#[derive(Deserialize)]
struct Edited {
    #[serde(rename = "lastEditedAt")]
    last_edited_at: Option<String>,
    editor: Option<Actor>,
}

/// An account GraphQL names.
#[derive(Deserialize)]
struct Actor {
    #[serde(rename = "__typename")]
    kind: Option<String>,
    login: Option<String>,
    #[serde(rename = "databaseId")]
    id: Option<u64>,
}

/// When a text was last edited and by whom, both None when unedited.
type Edit = (Option<At>, Option<Account>);

/// What GraphQL gives of the pull request, read.
struct Node {
    /// The head and the time GitHub last saw the pull request change, as
    /// this answer gives them.
    head: String,
    updated: At,
    fork: bool,
    merge_state: Mergeable,
    closes: Vec<Ref<BlueprintNo>>,
    edited: Option<At>,
    editor: Option<Account>,
}

impl RepoNode {
    /// The pull request's part, which must be of `no`, else
    /// `Unread(Inconsistent)`. Closing references past one page are
    /// `Unread(Cut)`.
    fn read(self, no: ChangeNo, bot: Option<&Bot>) -> Got<Node> {
        let here = self.id.ok_or(MALFORMED)?;
        let pull = self.pull.ok_or(Outcome::Unread(Unread::Inconsistent))?;
        if pull.number != Some(no.get()) {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        let closing = pull.closing.ok_or(MALFORMED)?;
        let nodes = closing.nodes.ok_or(MALFORMED)?;
        match closing.total.ok_or(MALFORMED)? {
            t if t == nodes.len() as u64 => {}
            t if t > nodes.len() as u64 => return Err(Outcome::Unread(Unread::Cut)),
            _ => return Err(Outcome::Unread(Unread::Inconsistent)),
        }
        let mut closes = Vec::with_capacity(nodes.len());
        for c in nodes {
            let number = c.number.and_then(BlueprintNo::new).ok_or(MALFORMED)?;
            let repo = c.repository.ok_or(MALFORMED)?;
            closes.push(match repo.id.ok_or(MALFORMED)? {
                id if id == here => Ref::Here(number),
                _ => Ref::Elsewhere {
                    project: ProjectName::new(repo.name.ok_or(MALFORMED)?),
                    item: ItemName::new(number.get().to_string()),
                },
            });
        }
        let (edited, editor) = pull.edit.read(bot)?;
        Ok(Node {
            head: pull.head.ok_or(MALFORMED)?,
            updated: time(pull.updated.as_deref())?,
            fork: pull.fork.ok_or(MALFORMED)?,
            merge_state: super::mergeable(
                pull.mergeable.as_deref().ok_or(MALFORMED)?,
                pull.merge_state.as_deref().ok_or(MALFORMED)?,
            ),
            closes,
            edited,
            editor,
        })
    }
}

impl Edited {
    /// When the text was last edited, and by whom; both None when it was
    /// never edited. An edit by an account GitHub no longer names is the
    /// account `ghost` of an unknown kind, so an edited text never reads as
    /// unedited.
    fn read(self, bot: Option<&Bot>) -> Got<Edit> {
        let Some(at) = self.last_edited_at else {
            return Ok((None, None));
        };
        let at = time(Some(&at))?;
        let editor = match self.editor {
            None => Account::new(Realm::CodeHost, Login::new("ghost"), AccountKind::Unknown),
            Some(a) => {
                let facts = gh::Facts {
                    id: a.id,
                    kind: a.kind,
                };
                account(&a.login.ok_or(MALFORMED)?, &facts, bot).ok_or(MALFORMED)?
            }
        };
        Ok((Some(at), Some(editor)))
    }
}

/// The blueprints of this project the text names with GitHub's closing
/// syntax: each `#<n>` the core's reading of a closing line takes, in the
/// order they first stand in the text. `text` is the body without this
/// item's markers. A link (`closes https://github.com/<o>/<r>/issues/5`)
/// and `<o>/<r>#5`, which GitHub counts too, are not read: this reads
/// fewer, never more. The list (`changes` with bodies) reads it the same.
pub(super) fn says_closes(text: &str) -> Vec<Ref<BlueprintNo>> {
    let mut said: Vec<Ref<BlueprintNo>> = Vec::new();
    for (i, _) in text.match_indices('#') {
        let digits: String = text[i + 1..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        let Some(n) = digits.parse().ok().and_then(BlueprintNo::new) else {
            continue;
        };
        let here = Ref::Here(n);
        if !said.contains(&here) && tehas_core::issue_pull::closes_issue(text, n.get()) {
            said.push(here);
        }
    }
    said
}

/// The part of a GraphQL answer under `data`, its logins vouched. An answer
/// that carries errors is no whole answer: a spent priced budget when
/// GitHub says the rate is limited, `Unread(Inconsistent)` for any other.
pub(super) fn answer<T: serde::de::DeserializeOwned>(out: &str, now: i64) -> Got<T> {
    crate::graphql::part(out, "/data", now)
}

/// A REST answer, its logins vouched, or `Unread(Malformed)`.
fn parse<T: serde::de::DeserializeOwned>(out: &str) -> Got<T> {
    let mut v: Value = serde_json::from_str(out.trim()).map_err(|_| MALFORMED)?;
    tehas_core::accounts::vouch(&mut v);
    serde_json::from_value(v).map_err(|_| MALFORMED)
}

/// Whether a review's or a review comment's pull request is `no` of this
/// project. A missing link is `Unread(Malformed)`.
fn on_pull(url: Option<&str>, repo: &str, no: ChangeNo) -> Got<bool> {
    let url = url.ok_or(MALFORMED)?;
    Ok(ends_with_folded(
        url,
        &format!("/repos/{repo}/pulls/{}", no.get()),
    ))
}

/// How an author stands to the project, from GitHub's `author_association`:
/// its owner, a member of the owning organisation, a collaborator, or one
/// of GitHub's words for none of these. A word this adapter does not know
/// is `Unknown`, which counts as none of these too.
fn relation(word: &str) -> Relation {
    match word {
        "OWNER" => Relation::Owner,
        "MEMBER" => Relation::Member,
        "COLLABORATOR" => Relation::Collaborator,
        "CONTRIBUTOR" | "FIRST_TIME_CONTRIBUTOR" | "FIRST_TIMER" | "NONE" | "MANNEQUIN" => {
            Relation::Outside
        }
        _ => Relation::Unknown,
    }
}

/// A time of GitHub's, or `Unread(Malformed)`.
fn time(iso: Option<&str>) -> Got<At> {
    iso.and_then(tehas_core::time::from_iso)
        .map(At::new)
        .ok_or(MALFORMED)
}

/// A count GitHub gives, or `Unread(Malformed)`.
fn count(n: Option<u64>) -> Got<usize> {
    n.and_then(|n| usize::try_from(n).ok()).ok_or(MALFORMED)
}

/// A count that fits the port's `u32`, or `Unread(Malformed)`.
fn small(n: Option<u64>) -> Got<u32> {
    n.and_then(|n| u32::try_from(n).ok()).ok_or(MALFORMED)
}

/// The account of a `user`, None where it names nobody: a deleted
/// account's is `null`.
fn author(user: Option<&User>, bot: Option<&Bot>) -> Option<Account> {
    let u = user?;
    let facts = gh::Facts {
        id: u.id,
        kind: u.kind.clone(),
    };
    account(u.login.as_deref()?, &facts, bot)
}

/// A node id GitHub gives: letters, digits, `_` and `-`, so it goes in a
/// question as it is.
fn is_node_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 100
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
