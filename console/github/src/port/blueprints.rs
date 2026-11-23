//! The blueprints port's reads, `Blueprints`. Answered: one blueprint to
//! a depth (`blueprint`, `several`), its comments whole (`comments`), and a
//! search of the project's issues (`search`), the relay's way to settle an
//! open.
//!
//! A blueprint is the issue's REST read, the same read `edit` makes, and
//! its comments' REST pages as far as the depth asks: none at
//! `Depth::Cut { notes: 0 }`. Each text goes through `markers::read`, so
//! this item's markers come back apart and another item's words stay. The
//! revision is `blueprint_writes::revision` over the raw answer, markers
//! in, so an edit at it holds on an unchanged issue. GitHub's REST answer
//! names no editor of a body or a comment and no time the body was edited:
//! `editor` is None, and so is the body's `edited`. No read is retried: the
//! adapter has no shared retry yet.

use std::collections::HashSet;
use std::num::NonZeroU32;

use super::blueprint_writes::{ends_with_folded, read_issue, revision};
use super::unsupported;
use crate::ticket::CUT_TEXT;
use crate::tickets::account;
use crate::{gh, markers, GitHub};
use serde_json::Value;
use tehas_ports::{
    At, Attachment, AttachmentId, Blueprint, BlueprintDelta, BlueprintHit, BlueprintNo,
    Blueprints, Bound, ChangeToken, Comment, CommentId, Cursor, Depth, Did, Edit, Got, Outcome,
    Pool, Refusal, Revision, Took, Unread, Whole, Within, ALL_MAX,
};

/// The comments one page of an issue's comments asks for, the most GitHub
/// gives.
const COMMENTS_PER_PAGE: usize = 100;

/// The most pages of comments one read takes: past it, `Unread(Cut)`.
const COMMENT_PAGES_MAX: usize = 100;

/// The hits one page of a search asks for, the most GitHub gives.
const PER_PAGE: usize = 100;

/// The most hits GitHub gives of one search, over all its pages.
const REACH: usize = 1000;

/// The longest text of words GitHub takes in a search, qualifiers aside.
const WORDS_MAX: usize = 256;

impl Blueprints for GitHub {
    /// The issue's REST read, then the pages of its comments the depth
    /// takes, counted by the issue's own count of them. A pull request's
    /// number is `NotFound`, with no request after the issue's.
    fn blueprint(&mut self, no: BlueprintNo, depth: Depth) -> Got<Blueprint> {
        let (mut blueprint, count) = self.issue(no)?;
        blueprint.comments = match depth {
            Depth::Cut { notes } => {
                let mut took = self.last_comments(no, count, u32::from(notes))?;
                for c in &mut took.items {
                    c.body = c.body.chars().take(CUT_TEXT).collect();
                }
                took
            }
            Depth::Last { notes } => self.last_comments(no, count, notes.get())?,
            Depth::All => {
                let all = self.whole_comments(no, count, ALL_MAX)?;
                let mut took = Took::default();
                took.items = all.into_items();
                took
            }
        };
        Ok(blueprint)
    }

    /// One `blueprint` a number, in the order asked.
    fn several(&mut self, nos: &[BlueprintNo], depth: Depth) -> Vec<(BlueprintNo, Got<Blueprint>)> {
        nos.iter().map(|n| (*n, self.blueprint(*n, depth))).collect()
    }

    /// The issue's REST read for its count of comments, then every page of
    /// them; more than `bound` is `Unread(Cut)`, with no page read.
    fn comments(&mut self, no: BlueprintNo, bound: Bound) -> Got<Whole<Comment>> {
        let (_, count) = self.issue(no)?;
        self.whole_comments(no, count, bound)
    }

    fn comments_by_id(&mut self, ids: &[CommentId]) -> Vec<(CommentId, Got<Comment>)> {
        ids.iter().map(|i| (i.clone(), unsupported())).collect()
    }

    fn body_edits(&mut self, _: BlueprintNo, _: Bound) -> Got<Whole<Edit>> {
        unsupported()
    }

    fn comment_edits(&mut self, _: &CommentId, _: Bound) -> Got<Whole<Edit>> {
        unsupported()
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
            return Ok(Took {
                items: Vec::new(),
                more: false,
            });
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
                    kept.push(BlueprintHit {
                        no: hit.no,
                        title: hit.title,
                        open: hit.open,
                    });
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
        Ok(Took { items: kept, more })
    }

    fn attachments(&mut self, _: BlueprintNo) -> Got<Whole<Attachment>> {
        unsupported()
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
    fn issue(&mut self, no: BlueprintNo) -> Got<(Blueprint, usize)> {
        let out = self
            .issue_answer(no.get())
            .map_err(|e| self.outcome(&e))?;
        let issue = read_issue(&out, &self.repo)?;
        if issue.no != no {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        let malformed = Outcome::Unread(Unread::Malformed);
        let at = revision(&out).ok_or(malformed.clone())?;
        let mut v: Value = serde_json::from_str(out.trim()).map_err(|_| malformed.clone())?;
        tehas_core::accounts::vouch(&mut v);
        let open = match v.get("state").and_then(Value::as_str) {
            Some("open") => true,
            Some("closed") => false,
            _ => return Err(malformed),
        };
        let created = time(&v, "created_at")?;
        let count = v
            .get("comments")
            .and_then(Value::as_u64)
            .ok_or(malformed)? as usize;
        let read = markers::read(&issue.body, Some(no));
        let mut blueprint = Blueprint::new(no, created, at, issue.link);
        blueprint.title = issue.title;
        blueprint.body = read.text;
        blueprint.open = open;
        blueprint.author = author(&v);
        blueprint.markers = read.markers;
        blueprint.comments.more = count > 0;
        Ok((blueprint, count))
    }

    /// The newest `n` of the issue's `count` comments, oldest first, with
    /// `more` when it holds more than `n`.
    fn last_comments(&mut self, no: BlueprintNo, count: usize, n: u32) -> Got<Took<Comment>> {
        let from = count.saturating_sub(n as usize);
        let mut took = Took::default();
        took.items = self.comments_from(no, count, from)?;
        took.more = from > 0;
        Ok(took)
    }

    /// Every one of the issue's `count` comments, or `Unread(Cut)` past
    /// `bound` with no page read.
    fn whole_comments(&mut self, no: BlueprintNo, count: usize, bound: Bound) -> Got<Whole<Comment>> {
        if count > bound.get() as usize {
            return Err(Outcome::Unread(Unread::Cut));
        }
        Ok(Whole::new(self.comments_from(no, count, 0)?))
    }

    /// The issue's comments from the `from`th (0 the oldest) to the last of
    /// `count`, read a page at a time from the page that holds the first
    /// one asked. Every page holds exactly what `count` says it holds, each
    /// comment once and on issue `no` of this project, or the read is
    /// `Unread(Inconsistent)`: the issue changed under the read, or the
    /// answer is not this issue's.
    fn comments_from(&mut self, no: BlueprintNo, count: usize, from: usize) -> Got<Vec<Comment>> {
        if from >= count {
            return Ok(Vec::new());
        }
        let first = from / COMMENTS_PER_PAGE + 1;
        let last = (count - 1) / COMMENTS_PER_PAGE + 1;
        if last - first + 1 > COMMENT_PAGES_MAX {
            return Err(Outcome::Unread(Unread::Cut));
        }
        let mut seen = HashSet::new();
        let mut all = Vec::new();
        for page in first..=last {
            let out = self.comments_page(no, page)?;
            let held = (count - (page - 1) * COMMENTS_PER_PAGE).min(COMMENTS_PER_PAGE);
            let comments = read_comments(&out, &self.repo, no)?;
            if comments.len() != held {
                return Err(Outcome::Unread(Unread::Inconsistent));
            }
            for c in comments {
                if !seen.insert(c.id.clone()) {
                    return Err(Outcome::Unread(Unread::Inconsistent));
                }
                all.push(c);
            }
        }
        Ok(all.split_off(from - (first - 1) * COMMENTS_PER_PAGE))
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

/// The time of the field `k` of `v`, or `Unread(Malformed)`.
fn time(v: &Value, k: &str) -> Got<At> {
    v.get(k)
        .and_then(Value::as_str)
        .and_then(tehas_core::time::from_iso)
        .map(At::new)
        .ok_or(Outcome::Unread(Unread::Malformed))
}

/// The account of the `user` of an issue or a comment, None where it names
/// nobody: a deleted account's is `null`.
fn author(v: &Value) -> Option<tehas_ports::Account> {
    let u = v.get("user")?;
    let facts = gh::Facts {
        id: u.get("id").and_then(Value::as_u64),
        kind: u.get("type").and_then(Value::as_str).map(str::to_string),
    };
    account(u.get("login")?.as_str()?, &facts)
}

/// One page of an issue's comments, each read: its text through
/// `markers::read` as a text of blueprint `no`, its revision the sum of
/// its raw text, markers in. A page that does not parse, or a comment that
/// lacks a field every comment has, is `Unread(Malformed)`; a comment whose
/// link is not one on issue `no` of this project is
/// `Unread(Inconsistent)`.
fn read_comments(out: &str, repo: &str, no: BlueprintNo) -> Got<Vec<Comment>> {
    let malformed = Outcome::Unread(Unread::Malformed);
    let mut v: Value = serde_json::from_str(out.trim()).map_err(|_| malformed.clone())?;
    tehas_core::accounts::vouch(&mut v);
    let items = v.as_array().ok_or(malformed.clone())?;
    let mut comments = Vec::with_capacity(items.len());
    for c in items {
        let id = c.get("id").and_then(Value::as_u64).ok_or(malformed.clone())?;
        let url = c
            .get("html_url")
            .and_then(Value::as_str)
            .ok_or(malformed.clone())?;
        if !ends_with_folded(url, &format!("/{repo}/issues/{}#issuecomment-{id}", no.get())) {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        let link = tehas_ports::WebLink::new(url).map_err(|_| malformed.clone())?;
        let raw = match c.get("body") {
            None | Some(Value::Null) => "",
            Some(Value::String(s)) => s.as_str(),
            Some(_) => return Err(malformed),
        };
        let created = time(c, "created_at")?;
        let updated = time(c, "updated_at")?;
        let read = markers::read(raw, Some(no));
        let sum = Revision::new(tehas_core::approval::body_sum(raw));
        let mut comment = Comment::new(CommentId::new(id.to_string()), created, sum, link);
        comment.author = author(c);
        // REST keeps no edit time apart: a comment changes only by an edit.
        comment.edited = (updated != created).then_some(updated);
        comment.body = read.text;
        comment.markers = read.markers;
        comments.push(comment);
    }
    Ok(comments)
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
