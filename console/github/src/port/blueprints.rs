//! The blueprints port's reads, `Blueprints`. Answered: a search of the
//! project's issues (`search`), the relay's way to settle an open. Nothing
//! that answers a text is answered yet: a blueprint's body and comments
//! carry markers, which no reader of this adapter takes out yet.

use std::collections::HashSet;
use std::num::NonZeroU32;

use super::unsupported;
use crate::GitHub;
use serde_json::Value;
use tehas_ports::{
    Attachment, AttachmentId, Blueprint, BlueprintDelta, BlueprintHit, BlueprintNo, Blueprints,
    Bound, ChangeToken, Comment, CommentId, Cursor, Depth, Did, Edit, Got, Outcome, Pool, Refusal,
    Took, Unread, Whole, Within,
};

/// The hits one page of a search asks for, the most GitHub gives.
const PER_PAGE: usize = 100;

/// The most hits GitHub gives of one search, over all its pages.
const REACH: usize = 1000;

/// The longest text of words GitHub takes in a search, qualifiers aside.
const WORDS_MAX: usize = 256;

impl Blueprints for GitHub {
    fn blueprint(&mut self, _: BlueprintNo, _: Depth) -> Got<Blueprint> {
        unsupported()
    }

    fn several(&mut self, nos: &[BlueprintNo], _: Depth) -> Vec<(BlueprintNo, Got<Blueprint>)> {
        nos.iter().map(|n| (*n, unsupported())).collect()
    }

    fn comments(&mut self, _: BlueprintNo, _: Bound) -> Got<Whole<Comment>> {
        unsupported()
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

    /// None yet: this adapter reads no text, so it finds no attachment in
    /// one.
    fn attachments_in(&self, _: &str) -> Vec<Attachment> {
        Vec::new()
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
