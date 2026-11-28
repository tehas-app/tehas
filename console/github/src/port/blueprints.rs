//! The blueprints port's reads, `Blueprints`. Answered: one blueprint to
//! a depth (`blueprint`, `several`), its comments whole (`comments`), the
//! edit histories of bodies and comments (`body_edits`, `comment_edits` and
//! their plurals, in `edits`), a search of the project's issues (`search`),
//! the relay's way to settle an open, and the files a blueprint's body or
//! any text links to (`attachments`, `attachments_in`).
//!
//! A blueprint is the issue's REST read, the same read `edit` makes, and
//! its comments' REST pages as far as the depth asks: none at
//! `Depth::Cut { notes: 0 }`. Each text goes through `markers::read`, so
//! this item's markers come back apart and another item's words stay. The
//! revision is `blueprint_writes::revision` over the raw answer, markers
//! in, so an edit at it holds on an unchanged issue.
//!
//! REST names no editor and no time the body was edited, so a text that
//! may have been edited is asked of GraphQL: the body when the issue's
//! `updated_at` is not its `created_at`, and on each page of comments one
//! question for those whose `updated_at` moved. `editor` None means
//! unedited, never unread: a failed question fails the read. A read of
//! comments that comes out `Unread(Inconsistent)` is made once more; no
//! other failure is retried here.

use std::collections::HashSet;
use std::num::{NonZeroU32, NonZeroU64};

mod edits;

use super::blueprint_writes::{comment_revision, ends_with_folded, read_issue, revision};
use super::unsupported;
use crate::graphql::priced;
use crate::ticket::CUT_TEXT;
use crate::tickets::{account, Bot};
use crate::{gh, markers, GitHub};
use serde::Deserialize;
use serde_json::Value;
use tehas_ports::{
    At, Attachment, AttachmentId, Blueprint, BlueprintDelta, BlueprintHit, BlueprintNo, Blueprints,
    Bound, ChangeToken, Comment, CommentId, Cursor, Depth, Did, Edit, Got, Outcome, Pool, Refusal,
    TicketDigest, TicketKey, Took, Unread, Whole, Window, Within, ALL_MAX,
};

/// The comments one page of an issue's comments asks for, the most GitHub
/// gives.
const COMMENTS_PER_PAGE: usize = 100;

/// The most pages of comments one read takes, 10,000 comments: past it,
/// `Unread(Cut)`, even where `Depth::Last` or the caller's bound asks for
/// more. The spec bounds neither; a read that large is not one a caller
/// should make in one pass.
const COMMENT_PAGES_MAX: usize = 100;

/// The hits one page of a search asks for, the most GitHub gives.
const PER_PAGE: usize = 100;

/// The most hits GitHub gives of one search, over all its pages.
const REACH: usize = 1000;

/// The longest text of words GitHub takes in a search, qualifiers aside.
const WORDS_MAX: usize = 256;

impl Blueprints for GitHub {
    /// The issue's REST read, then the pages of its comments the depth
    /// takes, counted by the issue's own count of them: at
    /// `Depth::Cut { notes: 0 }` the one read alone. A pull request's number
    /// is `NotFound`, with no request after the issue's. A read of comments
    /// that comes out `Unread(Inconsistent)` is made once more, whole.
    fn blueprint(&mut self, no: BlueprintNo, depth: Depth) -> Got<Blueprint> {
        let read = |gh: &mut GitHub| -> Got<Blueprint> {
            let (mut blueprint, stamp) = gh.issue(no)?;
            gh.edit_of(&mut blueprint, &stamp)?;
            blueprint.comments = match depth {
                Depth::Cut { notes } => {
                    let mut took = gh.last_comments(no, &stamp, u32::from(notes))?;
                    for c in &mut took.items {
                        c.body = c.body.chars().take(CUT_TEXT).collect();
                    }
                    took
                }
                Depth::Last { notes } => gh.last_comments(no, &stamp, notes.get())?,
                Depth::All => {
                    let all = gh.whole_comments(no, &stamp, ALL_MAX)?;
                    let mut took = Took::default();
                    took.items = all.into_items();
                    took
                }
            };
            Ok(blueprint)
        };
        match depth.notes() {
            None => read(self),
            Some(_) => self.twice(read),
        }
    }

    /// One `blueprint` a number, in the order asked.
    fn several(&mut self, nos: &[BlueprintNo], depth: Depth) -> Vec<(BlueprintNo, Got<Blueprint>)> {
        nos.iter()
            .map(|n| (*n, self.blueprint(*n, depth)))
            .collect()
    }

    /// The issue's REST read for its count of comments, then every page of
    /// them; more than `bound` is `Unread(Cut)`, with no page read. A read
    /// that comes out `Unread(Inconsistent)` is made once more, whole.
    fn comments(&mut self, no: BlueprintNo, bound: Bound) -> Got<Whole<Comment>> {
        self.twice(|gh| {
            let (_, stamp) = gh.issue(no)?;
            gh.whole_comments(no, &stamp, bound)
        })
    }

    fn comments_by_id(&mut self, ids: &[CommentId]) -> Vec<(CommentId, Got<Comment>)> {
        ids.iter().map(|i| (i.clone(), unsupported())).collect()
    }

    /// `body_edits_of` of the one number.
    fn body_edits(&mut self, no: BlueprintNo, bound: Bound) -> Got<Whole<Edit>> {
        one(self.body_edits_of(&[no], bound))
    }

    /// `comment_edits_of` of the one id.
    fn comment_edits(&mut self, id: &CommentId, bound: Bound) -> Got<Whole<Edit>> {
        one(self.comment_edits_of(std::slice::from_ref(id), bound))
    }

    /// One GraphQL question for each `edits::CHUNK` numbers, each number
    /// asked once (`edits`).
    fn body_edits_of(
        &mut self,
        nos: &[BlueprintNo],
        bound: Bound,
    ) -> Vec<(BlueprintNo, Got<Whole<Edit>>)> {
        self.bodies_edits(nos, bound).0
    }

    /// As `body_edits_of`, a question that fails whole failing the read.
    fn body_edits_together(
        &mut self,
        nos: &[BlueprintNo],
        bound: Bound,
    ) -> Got<Vec<(BlueprintNo, Got<Whole<Edit>>)>> {
        match self.bodies_edits(nos, bound) {
            (_, Some(o)) => Err(o),
            (each, None) => Ok(each),
        }
    }

    /// One GraphQL question for each `edits::CHUNK` comments, each asked
    /// once, after a REST read of each comment whose node id no read of
    /// comments on this handle found (`edits`).
    fn comment_edits_of(
        &mut self,
        ids: &[CommentId],
        bound: Bound,
    ) -> Vec<(CommentId, Got<Whole<Edit>>)> {
        self.comments_edits(ids, bound)
    }

    /// The files `text` links to on GitHub's store, by tehas's one link
    /// rule (`crate::attachments::read`), with no request. `attachments`
    /// reads a blueprint's body without its markers through the same
    /// function.
    fn attachments_in(&self, text: &str) -> Vec<Attachment> {
        crate::attachments::read(text)
    }

    /// GitHub's issue search, its pages read in turn until one hit past
    /// `first` or the end. Each word is quoted, so none is a qualifier,
    /// and a hit is kept only when it is an issue of this project that
    /// holds every word, ASCII case folded, in its title or, searching
    /// everything, its title or body: the words alone decide a hit.
    fn search(
        &mut self,
        words: &str,
        within: Within,
        open: Option<bool>,
        first: NonZeroU32,
    ) -> Got<Took<BlueprintHit>> {
        let words: Vec<String> = words
            .split_whitespace()
            .map(str::to_ascii_lowercase)
            .collect();
        if words.is_empty() {
            return Ok(Took::default());
        }
        let q = query(&self.repo, &words, within, open)?;
        let want = first.get() as usize;
        let mut kept = Vec::new();
        let mut seen = HashSet::new();
        let mut total = None;
        let mut page = 1;
        while kept.len() <= want {
            let out = self.search_page(&q, page)?;
            let read = read_page(&out, &self.repo)?;
            if *total.get_or_insert(read.total) != read.total {
                return Err(Outcome::Unread(Unread::Inconsistent));
            }
            let short = read.hits.len() < PER_PAGE;
            for hit in read.hits {
                if !seen.insert(hit.no) {
                    return Err(Outcome::Unread(Unread::Inconsistent));
                }
                let found = match within {
                    Within::Titles => hit.title.clone(),
                    Within::Everything => format!("{}\n{}", hit.title, hit.body),
                };
                if hit.issue
                    && hit.here
                    && open.is_none_or(|o| o == hit.open)
                    && holds(&found, &words)
                {
                    let mut found = BlueprintHit::new(hit.no);
                    found.title = hit.title;
                    found.open = hit.open;
                    kept.push(found);
                }
            }
            let held = read.total.min(REACH);
            match seen.len().cmp(&held) {
                std::cmp::Ordering::Greater => return Err(Outcome::Unread(Unread::Inconsistent)),
                std::cmp::Ordering::Equal => break,
                std::cmp::Ordering::Less if short => {
                    return Err(Outcome::Unread(Unread::Inconsistent))
                }
                std::cmp::Ordering::Less => page += 1,
            }
        }
        // Past GitHub's reach, a hit that is not read may be one more.
        if kept.len() <= want && total.is_some_and(|t| t > REACH) {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        let more = kept.len() > want;
        kept.truncate(want);
        let mut took = Took::default();
        took.items = kept;
        took.more = more;
        Ok(took)
    }

    /// The issue's REST read, the one read `blueprint` makes at
    /// `Depth::Cut { notes: 0 }`, and its body without its markers through
    /// [`Blueprints::attachments_in`]'s function, so the ids are the same
    /// for the same text. No comment and no edit is read: only the body
    /// is the blueprint's text. A pull request's number is `NotFound`.
    fn attachments(&mut self, no: BlueprintNo) -> Got<Whole<Attachment>> {
        let (blueprint, _) = self.issue(no)?;
        Ok(Whole::new(crate::attachments::read(&blueprint.body)))
    }

    fn fetch(&mut self, _: &AttachmentId, _: u64, _: &mut dyn std::io::Write) -> Did {
        unsupported()
    }

    fn changed(&mut self, _: Option<&ChangeToken>) -> Got<(bool, ChangeToken)> {
        unsupported()
    }

    fn delta(&mut self, _: Option<&Cursor>, _: Bound) -> Got<BlueprintDelta> {
        unsupported()
    }
}

impl GitHub {
    /// The blueprint `no` with no comment, and how many comments the issue
    /// says it holds. A failed read is its outcome; a pull request is
    /// `NotFound`; an answer of another issue or project is
    /// `Unread(Inconsistent)`; one that lacks a field every issue has is
    /// `Unread(Malformed)`.
    fn issue(&mut self, no: BlueprintNo) -> Got<(Blueprint, Stamp)> {
        let out = self.issue_answer(no.get()).map_err(|e| self.outcome(&e))?;
        self.issue_of(no, &out)
    }

    /// The blueprint `no` as `blueprint` reads it at
    /// `Depth::Cut { notes: 0 }`, and its ticket's digest, from one REST
    /// read of the issue: where ticket and blueprint are one item, both
    /// are of the one answer. A pull request is `NotFound`.
    pub(super) fn blueprint_and_digest(
        &mut self,
        no: BlueprintNo,
    ) -> Got<(Blueprint, TicketDigest)> {
        let out = self.issue_answer(no.get()).map_err(|e| self.outcome(&e))?;
        let (mut blueprint, stamp) = self.issue_of(no, &out)?;
        let key = NonZeroU64::new(no.get())
            .map(TicketKey::Number)
            .ok_or(Outcome::Unread(Unread::Inconsistent))?;
        let digest = self.digest_of(&key, &out)?;
        self.edit_of(&mut blueprint, &stamp)?;
        Ok((blueprint, digest))
    }

    /// The body's last edit, asked only of an issue changed since it was
    /// opened: unedited otherwise.
    fn edit_of(&mut self, blueprint: &mut Blueprint, stamp: &Stamp) -> Did {
        if stamp.touched {
            let (edited, editor) = self.body_edit(blueprint.no)?;
            blueprint.edited = edited;
            blueprint.editor = editor;
        }
        Ok(())
    }

    /// [`GitHub::issue`] of an answer read already.
    fn issue_of(&mut self, no: BlueprintNo, out: &str) -> Got<(Blueprint, Stamp)> {
        let issue = read_issue(out, &self.repo)?;
        if issue.no != no {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        let malformed = Outcome::Unread(Unread::Malformed);
        let at = revision(out).ok_or(malformed.clone())?;
        let mut v: Value = serde_json::from_str(out.trim()).map_err(|_| malformed.clone())?;
        tehas_core::accounts::vouch(&mut v);
        let f: IssueFields = serde_json::from_value(v).map_err(|_| malformed.clone())?;
        let open = match f.state.as_deref() {
            Some("open") => true,
            Some("closed") => false,
            _ => return Err(malformed),
        };
        let created = time(f.created_at.as_deref())?;
        let count = f.comments.ok_or(malformed.clone())? as usize;
        let updated = f.updated_at.ok_or(malformed)?;
        let touched = f.created_at != Some(updated.clone());
        let read = markers::read(&issue.body, Some(no));
        let mut blueprint = Blueprint::new(no, created, at, issue.link);
        blueprint.title = issue.title;
        blueprint.body = read.text;
        blueprint.open = open;
        blueprint.author = author(f.user.as_ref(), self.bot.as_ref());
        blueprint.markers = read.markers;
        blueprint.comments.more = count > 0;
        Ok((
            blueprint,
            Stamp {
                count,
                updated,
                touched,
            },
        ))
    }

    /// `read`, and once more when it comes out `Unread(Inconsistent)`: the
    /// issue changed under it. The spec's read retry; a transient failure
    /// is not retried here.
    fn twice<T>(&mut self, mut read: impl FnMut(&mut GitHub) -> Got<T>) -> Got<T> {
        match read(self) {
            Err(Outcome::Unread(Unread::Inconsistent)) => read(self),
            other => other,
        }
    }

    /// The newest `n` of the issue's `count` comments, oldest first, with
    /// `more` when it holds more than `n`.
    fn last_comments(&mut self, no: BlueprintNo, stamp: &Stamp, n: u32) -> Got<Took<Comment>> {
        let from = stamp.count.saturating_sub(n as usize);
        let mut took = Took::default();
        took.items = self.comments_from(no, stamp, from)?;
        took.more = from > 0;
        Ok(took)
    }

    /// The ticket's notes: the issue's comments, oldest first, as much as
    /// `window` takes. An issue and its blueprint are the one item here,
    /// so these are the same pages `comments` reads. A read that comes out
    /// `Unread(Inconsistent)` is made once more, whole.
    pub(super) fn ticket_notes(&mut self, no: BlueprintNo, window: Window) -> Got<Took<Comment>> {
        self.twice(|gh| {
            let (_, stamp) = gh.issue(no)?;
            match window {
                Window::Whole(bound) => {
                    let mut took = Took::default();
                    took.items = gh.whole_comments(no, &stamp, bound)?.into_items();
                    Ok(took)
                }
                Window::Last(notes) => gh.last_comments(no, &stamp, notes.get()),
            }
        })
    }

    /// [`Blueprints::comment_edits`] of the one id, for the tickets port.
    pub(super) fn note_edits_one(&mut self, id: &CommentId, bound: Bound) -> Got<Whole<Edit>> {
        one(self.comments_edits(std::slice::from_ref(id), bound))
    }

    /// Every one of the issue's `count` comments, or `Unread(Cut)` past
    /// `bound` with no page read.
    fn whole_comments(
        &mut self,
        no: BlueprintNo,
        stamp: &Stamp,
        bound: Bound,
    ) -> Got<Whole<Comment>> {
        if stamp.count > bound.get() as usize {
            return Err(Outcome::Unread(Unread::Cut));
        }
        Ok(Whole::new(self.comments_from(no, stamp, 0)?))
    }

    /// The issue's comments from the `from`th (0 the oldest) to the last of
    /// the `stamp.count` it held when read, a page at a time from the page
    /// that holds the first one asked. The read is `Unread(Inconsistent)`
    /// unless every page holds exactly what the count says, each comment
    /// once, on issue `no` of this project, its id and time rising from the
    /// one before. Over more than one page a comment deleted and another
    /// posted between two pages keeps the count and skips one, so the issue
    /// is read again after the last page and must still hold the count and
    /// the `updated_at` it was read with. One page is one request and needs
    /// no read after it.
    fn comments_from(&mut self, no: BlueprintNo, stamp: &Stamp, from: usize) -> Got<Vec<Comment>> {
        let count = stamp.count;
        if from >= count {
            return Ok(Vec::new());
        }
        let inconsistent = Outcome::Unread(Unread::Inconsistent);
        let first = from / COMMENTS_PER_PAGE + 1;
        let last = (count - 1) / COMMENTS_PER_PAGE + 1;
        if last - first + 1 > COMMENT_PAGES_MAX {
            return Err(Outcome::Unread(Unread::Cut));
        }
        let mut all: Vec<Comment> = Vec::new();
        let mut before: Option<(u64, At)> = None;
        for page in first..=last {
            let out = self.comments_page(no, page)?;
            let held = (count - (page - 1) * COMMENTS_PER_PAGE).min(COMMENTS_PER_PAGE);
            let (mut comments, touched) = read_comments(&out, &self.repo, no, self.bot.as_ref())?;
            if comments.len() != held {
                return Err(inconsistent);
            }
            for (at, node) in &touched {
                if let Ok(id) = comments[*at].id.as_str().parse() {
                    self.comment_nodes.insert(id, (node.clone(), no));
                }
            }
            self.fill_editors(&mut comments, &touched)?;
            for c in comments {
                let id: u64 = c.id.as_str().parse().map_err(|_| inconsistent.clone())?;
                if before.is_some_and(|(i, at)| id <= i || c.created < at) {
                    return Err(inconsistent);
                }
                before = Some((id, c.created));
                all.push(c);
            }
        }
        if last > first && self.issue(no)?.1 != *stamp {
            return Err(inconsistent);
        }
        Ok(all.split_off(from - (first - 1) * COMMENTS_PER_PAGE))
    }

    /// The body's last edit of issue `no`: when, and by whom, both None when
    /// unedited. One GraphQL question; its failure is the read's.
    fn body_edit(&mut self, no: BlueprintNo) -> Got<(Option<At>, Option<tehas_ports::Account>)> {
        let out = self
            .ask("the issue's editor", BODY_EDIT, Some(no.get()), None)
            .map_err(|e| priced(self.outcome(&e)))?;
        let edit: Edited = answer(&out, "/data/repository/issue", (self.now)())?;
        edit.read(self.bot.as_ref())
    }

    /// The last edit of each comment of a page whose `updated_at` moved,
    /// `touched` their indexes and node ids: one GraphQL question for the
    /// page, none when none moved. Every one asked must be answered as an
    /// issue comment, or the read is `Unread(Inconsistent)`.
    fn fill_editors(&mut self, comments: &mut [Comment], touched: &[Touched]) -> Did {
        if touched.is_empty() {
            return Ok(());
        }
        let ids: Vec<String> = touched.iter().map(|(_, id)| format!("\"{id}\"")).collect();
        let question = COMMENT_EDITS.replace("IDS", &ids.join(", "));
        let out = self
            .ask("the comments' editors", &question, None, None)
            .map_err(|e| priced(self.outcome(&e)))?;
        let nodes: Vec<Option<Edited>> = answer(&out, "/data/nodes", (self.now)())?;
        if nodes.len() != touched.len() {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        for ((at, id), node) in touched.iter().zip(nodes) {
            let node = node
                .filter(|n| n.id.as_deref() == Some(id.as_str()))
                .ok_or(Outcome::Unread(Unread::Inconsistent))?;
            let (edited, editor) = node.read(self.bot.as_ref())?;
            comments[*at].edited = edited;
            comments[*at].editor = editor;
        }
        Ok(())
    }

    /// gh's answer to one page of an issue's comments, oldest first, or
    /// its outcome.
    fn comments_page(&mut self, no: BlueprintNo, page: usize) -> Got<String> {
        let path = format!(
            "repos/{}/issues/{}/comments?per_page={COMMENTS_PER_PAGE}&page={page}",
            self.repo,
            no.get()
        );
        self.call("the issue's comments", &["api", &path])
            .map_err(|e| self.outcome(&e))
    }

    /// gh's answer to one page of the issue search, or its outcome. A
    /// spent budget is the search pool's. A 422 is a project the site will
    /// not search, gone or not visible: the words were checked before.
    fn search_page(&mut self, q: &str, page: usize) -> Got<String> {
        let q = format!("q={q}");
        let per_page = format!("per_page={PER_PAGE}");
        let page = format!("page={page}");
        let args = [
            "api",
            "-X",
            "GET",
            "search/issues",
            "-f",
            &q,
            "-f",
            &per_page,
            "-f",
            &page,
        ];
        self.call("the search", &args).map_err(|e| {
            if e.why.contains("HTTP 422") {
                return Outcome::NotFound;
            }
            match self.outcome(&e) {
                Outcome::Spent { until, .. } => Outcome::Spent {
                    pool: Pool::Search,
                    until,
                },
                other => other,
            }
        })
    }
}

/// What an issue's read says of its comments: how many, and when the issue
/// last changed. A comment posted or deleted moves one or the other.
/// `touched`: it changed since it was opened, so its body may have been
/// edited.
#[derive(PartialEq, Eq)]
struct Stamp {
    count: usize,
    updated: String,
    touched: bool,
}

/// The question of an issue's last body edit.
const BODY_EDIT: &str = "query($owner: String!, $name: String!, $number: Int!) { repository(owner: $owner, name: $name) { issue(number: $number) { lastEditedAt editor { __typename login ...on User { databaseId } ...on Bot { databaseId } } } } }";

/// The question of the last edit of comments by node id, `IDS` their
/// quoted ids. It names the repository so the adapter's one way to ask
/// GitHub, which always gives it, asks no variable it does not use.
const COMMENT_EDITS: &str = "query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { id } nodes(ids: [IDS]) { __typename ...on IssueComment { id lastEditedAt editor { __typename login ...on User { databaseId } ...on Bot { databaseId } } } } }";

/// A text's last edit as GraphQL answers it.
#[derive(Deserialize)]
pub(super) struct Edited {
    id: Option<String>,
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

impl Edited {
    /// When the text was last edited, and by whom; both None when it was
    /// never edited. An edit by an account GitHub no longer names (deleted:
    /// its `ghost`) is answered as the account `ghost` of an unknown kind,
    /// so an edited text never reads as unedited.
    pub(super) fn read(self, bot: Option<&Bot>) -> Got<(Option<At>, Option<tehas_ports::Account>)> {
        let Some(at) = self.last_edited_at else {
            return Ok((None, None));
        };
        let at = time(Some(&at))?;
        Ok((Some(at), Some(editor(self.editor, bot)?)))
    }
}

/// The account GraphQL names as an editor. A deleted account is GitHub's
/// `ghost`, with no type and no id.
fn editor(actor: Option<Actor>, bot: Option<&Bot>) -> Got<tehas_ports::Account> {
    let (login, facts) = match actor {
        None => (
            "ghost".to_string(),
            gh::Facts {
                id: None,
                kind: None,
            },
        ),
        Some(a) => (
            a.login.ok_or(Outcome::Unread(Unread::Malformed))?,
            gh::Facts {
                id: a.id,
                kind: a.kind,
            },
        ),
    };
    account(&login, &facts, bot).ok_or(Outcome::Unread(Unread::Malformed))
}

/// The one answer of a read of one item.
fn one<K>(mut several: Vec<(K, Got<Whole<Edit>>)>) -> Got<Whole<Edit>> {
    several
        .pop()
        .map_or(Err(Outcome::Unread(Unread::Inconsistent)), |(_, got)| got)
}

/// The part of a GraphQL answer at `at`, its logins vouched. An answer
/// that carries errors is no whole answer: a spent priced budget when
/// GitHub says the rate is limited, `Unread(Inconsistent)` for any other
/// (a comment deleted since its page was read among them).
fn answer<T: serde::de::DeserializeOwned>(out: &str, at: &str, now: i64) -> Got<T> {
    crate::graphql::part(out, at, now)
}

/// The fields of an issue's REST answer the read takes beside
/// `read_issue`'s; each missing one is None.
#[derive(Deserialize)]
struct IssueFields {
    state: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
    comments: Option<u64>,
    user: Option<User>,
}

/// One comment of a page of an issue's comments. A body that is no text
/// fails the page.
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

/// The account GitHub names as an issue's or a comment's `user`.
#[derive(Deserialize)]
struct User {
    login: Option<String>,
    id: Option<u64>,
    #[serde(rename = "type")]
    kind: Option<String>,
}

/// A time of GitHub's, or `Unread(Malformed)`.
fn time(iso: Option<&str>) -> Got<At> {
    iso.and_then(tehas_core::time::from_iso)
        .map(At::new)
        .ok_or(Outcome::Unread(Unread::Malformed))
}

/// The account of an issue's or a comment's `user`, None where it names
/// nobody: a deleted account's is `null`.
fn author(user: Option<&User>, bot: Option<&Bot>) -> Option<tehas_ports::Account> {
    let u = user?;
    let facts = gh::Facts {
        id: u.id,
        kind: u.kind.clone(),
    };
    account(u.login.as_deref()?, &facts, bot)
}

/// One page of an issue's comments, each read: its text through
/// `markers::read` as a text of blueprint `no`, its revision the sum of
/// its raw text, markers in. A page that does not parse, or a comment that
/// lacks a field every comment has, is `Unread(Malformed)`; a comment whose
/// link is not one on issue `no` of this project is
/// `Unread(Inconsistent)`.
fn read_comments(
    out: &str,
    repo: &str,
    no: BlueprintNo,
    bot: Option<&Bot>,
) -> Got<(Vec<Comment>, Vec<Touched>)> {
    let malformed = Outcome::Unread(Unread::Malformed);
    let mut v: Value = serde_json::from_str(out.trim()).map_err(|_| malformed.clone())?;
    tehas_core::accounts::vouch(&mut v);
    let items: Vec<CommentFields> = serde_json::from_value(v).map_err(|_| malformed.clone())?;
    let mut comments = Vec::with_capacity(items.len());
    let mut touched = Vec::new();
    for c in items {
        let id = c.id.ok_or(malformed.clone())?;
        let url = c.html_url.as_deref().ok_or(malformed.clone())?;
        if !ends_with_folded(
            url,
            &format!("/{repo}/issues/{}#issuecomment-{id}", no.get()),
        ) {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        let link = tehas_ports::WebLink::new(url).map_err(|_| malformed.clone())?;
        let raw = c.body.as_deref().unwrap_or_default();
        let created = time(c.created_at.as_deref())?;
        let updated = time(c.updated_at.as_deref())?;
        let read = markers::read(raw, Some(no));
        let sum = comment_revision(raw);
        let mut comment = Comment::new(CommentId::new(id.to_string()), created, sum, link);
        comment.author = author(c.user.as_ref(), bot);
        comment.raw = Some(raw.to_string());
        if updated != created {
            let node = c
                .node_id
                .filter(|n| is_node_id(n))
                .ok_or(malformed.clone())?;
            touched.push((comments.len(), node));
        }
        comment.body = read.text;
        comment.markers = read.markers;
        comments.push(comment);
    }
    Ok((comments, touched))
}

/// A comment of a page whose `updated_at` moved: where it stands in the
/// page, and its node id.
type Touched = (usize, String);

/// A node id GitHub gives: letters, digits, `_` and `-`, so it goes in a
/// question as it is.
fn is_node_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 100
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// The search's question: this project's issues, each word quoted, and
/// the state asked. A `"` is no part of a word GitHub can quote, so it
/// splits one; the hit is still held to the word whole. Words GitHub cannot
/// search, longer than it takes or of quotes alone, are `Refused(Invalid)`,
/// with no request: a question with no word would read every issue.
fn query(repo: &str, words: &[String], within: Within, open: Option<bool>) -> Got<String> {
    let quoted: Vec<String> = words
        .iter()
        .flat_map(|w| w.split('"'))
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{w}\""))
        .collect();
    let said = quoted.join(" ");
    if said.is_empty() || said.len() > WORDS_MAX {
        return Err(Outcome::Refused(Refusal::Invalid));
    }
    let within = match within {
        Within::Titles => "in:title",
        Within::Everything => "in:title,body",
    };
    let state = match open {
        None => "",
        Some(true) => " is:open",
        Some(false) => " is:closed",
    };
    Ok(format!("repo:{repo} is:issue {within} {said}{state}")
        .trim_end()
        .to_string())
}

/// Every word is in `text`, ASCII case folded.
fn holds(text: &str, words: &[String]) -> bool {
    let text = text.to_ascii_lowercase();
    words.iter().all(|w| text.contains(w.as_str()))
}

/// One page of a search, read.
struct Page {
    /// How many items GitHub says the search holds.
    total: usize,
    hits: Vec<Hit>,
}

/// One item of a page, as the search's filter reads it.
struct Hit {
    no: BlueprintNo,
    title: String,
    body: String,
    open: bool,
    /// An issue, not a pull request.
    issue: bool,
    /// Of this project.
    here: bool,
}

/// A page of gh's answer. An answer that does not parse, or lacks a field
/// every page or item has, is `Unread(Malformed)`; an item with no number,
/// or a search GitHub says it did not finish, is `Unread(Inconsistent)`.
fn read_page(out: &str, repo: &str) -> Got<Page> {
    let malformed = Outcome::Unread(Unread::Malformed);
    let v: Value = serde_json::from_str(out.trim()).map_err(|_| malformed.clone())?;
    if v.get("incomplete_results").and_then(Value::as_bool) != Some(false) {
        return Err(match v.get("incomplete_results") {
            Some(Value::Bool(true)) => Outcome::Unread(Unread::Inconsistent),
            _ => malformed,
        });
    }
    let total = v
        .get("total_count")
        .and_then(Value::as_u64)
        .ok_or(malformed.clone())? as usize;
    let items = v
        .get("items")
        .and_then(Value::as_array)
        .ok_or(malformed.clone())?;
    let ours = format!("/repos/{repo}").to_ascii_lowercase();
    let mut hits = Vec::with_capacity(items.len());
    for item in items {
        let text = |k: &str| item.get(k).and_then(Value::as_str);
        let no = item
            .get("number")
            .and_then(Value::as_u64)
            .and_then(BlueprintNo::new)
            .ok_or(Outcome::Unread(Unread::Inconsistent))?;
        let open = match text("state") {
            Some("open") => true,
            Some("closed") => false,
            _ => return Err(malformed),
        };
        let here = text("repository_url")
            .ok_or(malformed.clone())?
            .to_ascii_lowercase()
            .ends_with(&ours);
        let body = match item.get("body") {
            None | Some(Value::Null) => String::new(),
            Some(Value::String(s)) => s.clone(),
            Some(_) => return Err(malformed),
        };
        hits.push(Hit {
            no,
            title: text("title").ok_or(malformed.clone())?.to_string(),
            body,
            open,
            issue: item.get("pull_request").is_none_or(Value::is_null),
            here,
        });
    }
    Ok(Page { total, hits })
}
