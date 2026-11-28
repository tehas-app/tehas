//! `CodeHost::reviews`, `CodeHost::review_comments`, `CodeHost::conversation`
//! and `CodeHost::may_write`: the review bodies of a pull request, the notes
//! reviewers left on its files, its conversation, and whether a named
//! account may write to the project. Each list is GitHub's own REST page,
//! 100 a page, read to the first short page, whole up to the caller's
//! bound or `Unread(Cut)`.
//!
//! The path of every request is made from the handle's own repository: the
//! only caller word that stands in a path is the login of `may_write`, and
//! it is asked only of GitHub's own login shape, percent-escaped besides.
//! That is the confirmation these reads carry: a handle reads no project
//! but the one it was made for, and a test pins the arguments.

use serde::Deserialize;

use super::change::{author, on_pull, parse, time, User, PER_PAGE};
use crate::forge::uri;
use crate::markers;
use crate::port::blueprint_writes::{comment_revision, ends_with_folded};
use crate::GitHub;
use tehas_ports::{
    Bound, ChangeNo, ChangeReview, Comment, CommentId, Got, Login, Outcome, Refusal, RepoPath,
    ReviewComment, Unread, WebLink, Whole,
};

const MALFORMED: Outcome = Outcome::Unread(Unread::Malformed);

/// The most pages one of these lists takes: 100 a page, past them the
/// list is not whole, `Unread(Cut)`.
const PAGES_MAX: u32 = 10;

/// One review of a page. A review still pending has no `submitted_at` and
/// is not a review given.
#[derive(Deserialize)]
struct ReviewPage {
    id: Option<u64>,
    state: Option<String>,
    body: Option<String>,
    submitted_at: Option<String>,
    html_url: Option<String>,
    pull_request_url: Option<String>,
    user: Option<User>,
}

/// One review comment of a page, on a file of the pull request.
#[derive(Deserialize)]
struct NotePage {
    id: Option<u64>,
    path: Option<String>,
    line: Option<u64>,
    original_line: Option<u64>,
    body: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
    html_url: Option<String>,
    pull_request_url: Option<String>,
    user: Option<User>,
}

/// One conversation comment of a page. The pull request's conversation is
/// GitHub's issue-comment list of the same number.
#[derive(Deserialize)]
struct TalkPage {
    id: Option<u64>,
    body: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
    html_url: Option<String>,
    user: Option<User>,
}

/// A permission answer: `role_name` names maintain, which `permission`
/// folds into write.
#[derive(Deserialize)]
struct PermissionFields {
    permission: Option<String>,
    role_name: Option<String>,
}

/// Whether the login has GitHub's own shape, so it may stand in a path.
fn login_shape(who: &str) -> bool {
    !who.is_empty()
        && who.len() <= 39
        && who.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

impl GitHub {
    /// The review bodies, oldest first, their ids rising, each on this
    /// pull request. A pending review is left out: nothing was submitted.
    pub(super) fn reviews_of(&mut self, no: ChangeNo, bound: Bound) -> Got<Whole<ChangeReview>> {
        let mut all: Vec<ChangeReview> = Vec::new();
        let mut last = 0;
        for page in 1..=PAGES_MAX {
            let out = self.page(no, "reviews", page as usize, "the reviews")?;
            let read: Vec<ReviewPage> = parse(&out)?;
            let short = read.len() < PER_PAGE;
            for r in read {
                if r.submitted_at.as_deref().unwrap_or("").is_empty()
                    || r.state.as_deref() == Some("PENDING")
                {
                    continue;
                }
                let id = r.id.ok_or(MALFORMED.clone())?;
                if id <= last || !on_pull(r.pull_request_url.as_deref(), &self.repo, no)? {
                    return Err(Outcome::Unread(Unread::Inconsistent));
                }
                last = id;
                let at = time(r.submitted_at.as_deref())?;
                let url = r.html_url.as_deref().ok_or(MALFORMED.clone())?;
                let tail = format!("/{}/pull/{}#pullrequestreview-{id}", self.repo, no.get());
                if !ends_with_folded(url, &tail) {
                    return Err(Outcome::Unread(Unread::Inconsistent));
                }
                let mut review = ChangeReview::new(
                    id,
                    at,
                    r.body.unwrap_or_default(),
                    WebLink::new(url).map_err(|_| MALFORMED.clone())?,
                );
                review.by = author(r.user.as_ref(), self.bot.as_ref());
                all.push(review);
                if all.len() > bound.get() as usize {
                    return Err(Outcome::Unread(Unread::Cut));
                }
            }
            if short {
                break;
            }
        }
        Ok(Whole::new(all))
    }

    /// The notes reviewers left on files, oldest first, their ids rising,
    /// each on this pull request. A note of a review still pending stays:
    /// it was written, and its body is read here, not through the review.
    pub(super) fn review_notes_of(
        &mut self,
        no: ChangeNo,
        bound: Bound,
    ) -> Got<Whole<ReviewComment>> {
        let mut all: Vec<ReviewComment> = Vec::new();
        let mut last = 0;
        for page in 1..=PAGES_MAX {
            let out = self.page(no, "comments", page as usize, "the review comments")?;
            let read: Vec<NotePage> = parse(&out)?;
            let short = read.len() < PER_PAGE;
            for n in read {
                let id = n.id.ok_or(MALFORMED.clone())?;
                if id <= last || !on_pull(n.pull_request_url.as_deref(), &self.repo, no)? {
                    return Err(Outcome::Unread(Unread::Inconsistent));
                }
                last = id;
                let at = time(n.created_at.as_deref())?;
                let updated = time(n.updated_at.as_deref())?;
                let url = n.html_url.as_deref().ok_or(MALFORMED.clone())?;
                let tail = format!("/{}/pull/{}#discussion_r{id}", self.repo, no.get());
                if !ends_with_folded(url, &tail) {
                    return Err(Outcome::Unread(Unread::Inconsistent));
                }
                let mut note = ReviewComment::new(
                    id,
                    at,
                    n.body.unwrap_or_default(),
                    WebLink::new(url).map_err(|_| MALFORMED.clone())?,
                );
                note.by = author(n.user.as_ref(), self.bot.as_ref());
                note.edited = (updated != at).then_some(updated);
                note.path = match n.path {
                    Some(p) => Some(RepoPath::new(p).map_err(|_| MALFORMED.clone())?),
                    None => None,
                };
                note.line = n.line.or(n.original_line).map(|l| l as u32);
                all.push(note);
                if all.len() > bound.get() as usize {
                    return Err(Outcome::Unread(Unread::Cut));
                }
            }
            if short {
                break;
            }
        }
        Ok(Whole::new(all))
    }

    /// The conversation of the pull request: GitHub's issue-comment list
    /// of the same number, oldest first, its ids rising. The text is kept
    /// as it was read; no edit history is asked, as no reader of these
    /// judges an edit.
    pub(super) fn conversation_of(&mut self, no: ChangeNo, bound: Bound) -> Got<Whole<Comment>> {
        let mut all: Vec<Comment> = Vec::new();
        let mut last: u64 = 0;
        for page in 1..=PAGES_MAX {
            let path = format!(
                "repos/{}/issues/{}/comments?per_page={PER_PAGE}&page={page}",
                self.repo,
                no.get()
            );
            let out = self
                .call("the conversation", &["api", &path])
                .map_err(|e| self.outcome(&e))?;
            let read: Vec<TalkPage> = parse(&out)?;
            let short = read.len() < PER_PAGE;
            for c in read {
                let id = c.id.ok_or(MALFORMED.clone())?;
                if id <= last {
                    return Err(Outcome::Unread(Unread::Inconsistent));
                }
                last = id;
                let created = time(c.created_at.as_deref())?;
                let updated = time(c.updated_at.as_deref())?;
                let url = c.html_url.as_deref().ok_or(MALFORMED.clone())?;
                let on_issue = ends_with_folded(
                    url,
                    &format!("/{}/issues/{}#issuecomment-{id}", self.repo, no.get()),
                );
                let on_pull_page = ends_with_folded(
                    url,
                    &format!("/{}/pull/{}#issuecomment-{id}", self.repo, no.get()),
                );
                if !(on_issue || on_pull_page) {
                    return Err(Outcome::Unread(Unread::Inconsistent));
                }
                let raw = c.body.unwrap_or_default();
                let text = markers::read(&raw, None).text;
                let mut comment = Comment::new(
                    CommentId::new(id.to_string()),
                    created,
                    comment_revision(&raw),
                    WebLink::new(url).map_err(|_| MALFORMED.clone())?,
                );
                comment.author = author(c.user.as_ref(), self.bot.as_ref());
                comment.edited = (updated != created).then_some(updated);
                comment.raw = Some(raw);
                comment.body = text;
                all.push(comment);
                if all.len() > bound.get() as usize {
                    return Err(Outcome::Unread(Unread::Cut));
                }
            }
            if short {
                break;
            }
        }
        Ok(Whole::new(all))
    }

    /// Whether the account may write: an answer of `role_name`, else of
    /// `permission`; admin, maintain or write may. A name outside GitHub's
    /// login shape is `Refused(Invalid)` before any request: it stands in
    /// the path. The path is the handle's own repository.
    pub(super) fn may_write_of(&mut self, who: &Login) -> Got<bool> {
        let name = who.as_str();
        if !login_shape(name) {
            return Err(Outcome::Refused(Refusal::Invalid));
        }
        let path = format!("repos/{}/collaborators/{}/permission", self.repo, uri(name));
        let out = self
            .call("the permission", &["api", &path])
            .map_err(|e| self.outcome(&e))?;
        let p: PermissionFields = parse(&out)?;
        let role = p.role_name.as_deref().or(p.permission.as_deref());
        match role {
            Some("admin") | Some("maintain") | Some("write") => Ok(true),
            Some(_) => Ok(false),
            None => Err(MALFORMED),
        }
    }
}
