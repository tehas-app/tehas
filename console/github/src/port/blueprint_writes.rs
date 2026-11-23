//! The blueprints port's writes, `BlueprintWrites`. Answered: a new
//! blueprint (`open`), its title and body edited at a revision (`edit`) and
//! a comment (`comment`). Each write is one request, sent once: its title
//! and body go as JSON on gh's standard input, never in an argument, which
//! holds at most 128 KiB and is seen by every process on the host.
//!
//! Every write goes to this adapter's own repository, named by its typed
//! number, and carries no marker: this adapter has no writer of markers
//! yet, so a write that names one, or whose text holds one, is refused
//! before any request, and an edit that would drop one is refused too.

use super::unsupported;
use crate::{gh, ForgeError, GitHub};
use serde_json::Value;
use tehas_core::text::has_marker;
use tehas_ports::{
    BlueprintNo, BlueprintWrites, Body, CommentId, Did, Got, Lost, Marker, MarkerEdit,
    NewBlueprint, Outcome, Pool, Posted, Refusal, Revision, Sent, Unread, WebLink,
};

/// What `GhRunner::run_input`'s default answers: the runner sends nothing.
const NO_INPUT: &str = "this runner takes no input";

impl BlueprintWrites for GitHub {
    /// A new issue, `POST` of its title and body.
    fn open(&mut self, new: &NewBlueprint) -> Got<Posted<BlueprintNo>> {
        let Body::Text(body) = &new.body else {
            return Err(Outcome::Refused(Refusal::Invalid));
        };
        if !new.markers.is_empty() {
            return unsupported();
        }
        unmarked(&[&new.title, body])?;
        let path = format!("{}/issues", self.issues()?);
        let input = serde_json::json!({ "title": new.title, "body": body }).to_string();
        let out = self.send("POST", &path, &input)?;
        let issue = read_issue(&out, &self.repo)?;
        Ok(Posted {
            id: issue.no,
            link: issue.link,
        })
    }

    /// The issue read, its revision compared with `since`, then a `PATCH`
    /// of what is given. GitHub has no conditional update of an issue, so
    /// the moment between the read and the write stays open: another
    /// account's edit in it is overwritten. The revision is the issue's
    /// title and body, as [`revision`] makes it; labels and comments do not
    /// move it.
    fn edit(
        &mut self,
        no: BlueprintNo,
        title: Option<&str>,
        body: Option<&str>,
        markers: &MarkerEdit,
        since: &Revision,
    ) -> Got<WebLink> {
        if *markers != MarkerEdit::default() {
            return unsupported();
        }
        let given: Vec<&str> = title.into_iter().chain(body).collect();
        unmarked(&given)?;
        let path = format!("{}/issues/{}", self.issues()?, no.get());
        // The read `blueprint(no, Depth::Cut { notes: 0 })` makes: the issue
        // alone, no comment.
        let out = self
            .issue_answer(no.get())
            .map_err(|e| unsent(self.outcome(&e)))?;
        let now = read_issue(&out, &self.repo)?;
        if now.no != no {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        if revision(&out).as_ref() != Some(since) {
            return Err(Outcome::Conflict(Lost::Changed));
        }
        // The body's markers cannot be read apart yet, so a new body would
        // lose them.
        if body.is_some() && has_marker(&now.body) {
            return unsupported();
        }
        let mut fields = serde_json::Map::new();
        if let Some(t) = title {
            fields.insert("title".into(), t.into());
        }
        if let Some(b) = body {
            fields.insert("body".into(), b.into());
        }
        if fields.is_empty() {
            return Ok(now.link);
        }
        let out = self.send("PATCH", &path, &Value::Object(fields).to_string())?;
        let edited = read_issue(&out, &self.repo)?;
        if edited.no != no {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        Ok(edited.link)
    }

    /// A comment on the issue, `POST` of its body. Its link is the
    /// comment's own, the issue's with `#issuecomment-<id>`.
    fn comment(
        &mut self,
        no: BlueprintNo,
        body: &str,
        markers: &[Marker],
    ) -> Got<Posted<CommentId>> {
        if !markers.is_empty() {
            return unsupported();
        }
        unmarked(&[body])?;
        let path = format!("{}/issues/{}/comments", self.issues()?, no.get());
        let input = serde_json::json!({ "body": body }).to_string();
        let out = self.send("POST", &path, &input)?;
        read_comment(&out, &self.repo, no)
    }

    fn edit_comment(
        &mut self,
        _: &CommentId,
        _: &str,
        _: &MarkerEdit,
        _: &Revision,
    ) -> Got<WebLink> {
        unsupported()
    }

    fn close(&mut self, _: BlueprintNo, _: bool) -> Did {
        unsupported()
    }
}

impl GitHub {
    /// The REST path of this adapter's repository, the one place its writes
    /// go. A name that is no `owner/name` of GitHub's characters is
    /// `Refused(Invalid)`: it would aim the request somewhere else.
    fn issues(&self) -> Got<String> {
        let (owner, name) = self
            .repo
            .split_once('/')
            .ok_or(Outcome::Refused(Refusal::Invalid))?;
        let ok = |s: &str, max: usize, extra: &[u8]| {
            !s.is_empty()
                && s.len() <= max
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || extra.contains(&b))
        };
        if ok(owner, 39, b"") && ok(name, 100, b"._") && name != "." && name != ".." {
            Ok(format!("repos/{owner}/{name}"))
        } else {
            Err(Outcome::Refused(Refusal::Invalid))
        }
    }

    /// One write, its JSON on gh's standard input, sent once. A failure is
    /// its named outcome: a 422 is GitHub refusing the input, a spent
    /// budget is the writes' pool, and a runner that sends no input sent
    /// nothing. No word of gh's answer, or of the input, is kept.
    fn send(&mut self, method: &str, path: &str, input: &str) -> Got<String> {
        let args = ["api", "-X", method, path, "--input", "-"];
        let why = match self.runner.run_input(&args, input) {
            Ok(out) => return Ok(out),
            Err(why) => why,
        };
        if why == NO_INPUT {
            return Err(Outcome::Refused(Refusal::Unsupported));
        }
        if why.contains("HTTP 422") {
            return Err(Outcome::Refused(Refusal::Invalid));
        }
        Err(match self.outcome(&ForgeError::new("the write", why)) {
            Outcome::Spent { until, .. } => Outcome::Spent {
                pool: Pool::Writes,
                until,
            },
            other => other,
        })
    }
}

/// The revision of an issue's REST read: its title and body, as the
/// relay's read before an edit has it (`GitHub::text_revision`). None for
/// an answer with no title.
pub(super) fn revision(out: &str) -> Option<Revision> {
    gh::text_revision(out).map(Revision::new)
}

/// A failure of the read before a write: the write was not sent.
fn unsent(o: Outcome) -> Outcome {
    match o {
        Outcome::Unavailable { why, .. } => Outcome::Unavailable {
            why,
            sent: Sent::No,
        },
        other => other,
    }
}

/// `Refused(Invalid)` when a text holds one of our markers: a body never
/// does, they travel beside it.
fn unmarked(texts: &[&str]) -> Did {
    if texts.iter().any(|t| has_marker(t)) {
        Err(Outcome::Refused(Refusal::Invalid))
    } else {
        Ok(())
    }
}

/// An issue of this project as a read or a write answers it.
struct Issue {
    no: BlueprintNo,
    body: String,
    link: WebLink,
}

/// The issue in gh's answer. An answer that does not parse or lacks a
/// field every issue has is `Unread(Malformed)`; a pull request is
/// `NotFound`, no blueprint; a link that is not this project's issue of
/// that number is `Unread(Inconsistent)`.
fn read_issue(out: &str, repo: &str) -> Got<Issue> {
    let malformed = Outcome::Unread(Unread::Malformed);
    let v: Value = serde_json::from_str(out.trim()).map_err(|_| malformed.clone())?;
    let no = v
        .get("number")
        .and_then(Value::as_u64)
        .and_then(BlueprintNo::new)
        .ok_or(malformed.clone())?;
    let url = v
        .get("html_url")
        .and_then(Value::as_str)
        .ok_or(malformed.clone())?;
    let body = match v.get("body") {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(_) => return Err(malformed),
    };
    if v.get("pull_request").is_some_and(|p| !p.is_null()) {
        return Err(Outcome::NotFound);
    }
    if !ends_with_folded(url, &format!("/{repo}/issues/{}", no.get())) {
        return Err(Outcome::Unread(Unread::Inconsistent));
    }
    let link = WebLink::new(url).map_err(|_| malformed)?;
    Ok(Issue { no, body, link })
}

/// The comment in gh's answer: its id, and its link, which must be the
/// comment's on this project's issue `no`.
fn read_comment(out: &str, repo: &str, no: BlueprintNo) -> Got<Posted<CommentId>> {
    let malformed = Outcome::Unread(Unread::Malformed);
    let v: Value = serde_json::from_str(out.trim()).map_err(|_| malformed.clone())?;
    let id = v
        .get("id")
        .and_then(Value::as_u64)
        .ok_or(malformed.clone())?;
    let url = v
        .get("html_url")
        .and_then(Value::as_str)
        .ok_or(malformed.clone())?;
    if !ends_with_folded(
        url,
        &format!("/{repo}/issues/{}#issuecomment-{id}", no.get()),
    ) {
        return Err(Outcome::Unread(Unread::Inconsistent));
    }
    Ok(Posted {
        id: CommentId::new(id.to_string()),
        link: WebLink::new(url).map_err(|_| malformed)?,
    })
}

/// `text` ends with `tail`, ASCII case folded: GitHub names a project in
/// any case.
fn ends_with_folded(text: &str, tail: &str) -> bool {
    text.len() >= tail.len()
        && text.is_char_boundary(text.len() - tail.len())
        && text[text.len() - tail.len()..].eq_ignore_ascii_case(tail)
}
