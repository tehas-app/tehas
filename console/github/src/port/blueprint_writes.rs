//! The blueprints port's writes, `BlueprintWrites`. Answered: a new
//! blueprint (`open`), its title and body edited at a revision (`edit`), a
//! comment (`comment`) and a comment's text edited at its revision
//! (`edit_comment`). Each write is one request, sent once, never again: its
//! title and body go as JSON on gh's standard input, never in an argument,
//! which holds at most 128 KiB and is seen by every process on the host.
//! `edit` and `comment` read the issue first, `edit_comment` the comment,
//! and every write confirms the repository first, once a handle
//! (`GitHub::confirm_here`).
//!
//! Every write goes to this adapter's own repository, named by its typed
//! number or id. `open`, `edit` and `comment` carry no marker: a write that
//! names one, or whose text holds one, is refused before any request, and
//! an edit that would drop one is refused too. `edit_comment` keeps the
//! comment's markers and changes them as `markers::edit` says, and edits
//! only a comment the factory's own account wrote, known by its id.

use super::named::Named;
use super::{unsent, unsupported};
use crate::{gh, markers, GitHub};
use serde_json::Value;
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
        let issue = self.as_here(|repo| read_issue(&out, repo))?;
        Ok(Posted::new(issue.no, issue.link))
    }

    /// The issue read, its revision compared with `since`, then a `PATCH`
    /// of what is given. GitHub has no conditional update of an issue, so
    /// the moment between the read and the write stays open: another
    /// account's edit in it is overwritten. The revision is the issue's
    /// title and body, as [`revision`] makes it; labels and comments do not
    /// move it. A `PATCH` that may have landed is settled by one read, never
    /// sent again: at the revision sent it is done, at `since` it was not
    /// sent, else it stays `sent: Maybe`.
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
        let (now, at) = self.read_before(no)?;
        if at != *since {
            return Err(Outcome::Conflict(Lost::Changed));
        }
        // The body's markers cannot be read apart yet, so a new body would
        // lose them: any the reader finds, bare too, and our wrapper,
        // opening or closing, as 1.x's readers find it.
        if body.is_some() && markers::foreign_text(&now.body) {
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
        let failed = match self.send("PATCH", &path, &Value::Object(fields).to_string()) {
            Ok(out) => {
                let edited = self.as_here(|repo| read_issue(&out, repo))?;
                if edited.no != no {
                    return Err(Outcome::Unread(Unread::Inconsistent));
                }
                return Ok(edited.link);
            }
            Err(o) => o,
        };
        let Outcome::Unavailable {
            why,
            sent: Sent::Maybe,
        } = failed
        else {
            return Err(failed);
        };
        let sent = text_revision(title.unwrap_or(&now.title), body.unwrap_or(&now.body));
        match self.read_before(no) {
            Ok((again, r)) if Some(&r) == sent.as_ref() => Ok(again.link),
            Ok((_, r)) if r == at => Err(Outcome::Unavailable {
                why,
                sent: Sent::No,
            }),
            _ => Err(failed),
        }
    }

    /// The issue read first, as `edit` does, so a pull request's number is
    /// `NotFound` with nothing sent; then a comment on it, `POST` of its
    /// body. Its link is the comment's own, the issue's with
    /// `#issuecomment-<id>`.
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
        self.read_before(no)?;
        let input = serde_json::json!({ "body": body }).to_string();
        let out = self.send("POST", &path, &input)?;
        self.as_here(|repo| read_comment(&out, repo, no))
    }

    /// The comment read, its revision compared with `since`, then one
    /// `PATCH` of its body. The revision is [`comment_revision`] of its raw
    /// body, markers in, as `Blueprints::comments` gives it. GitHub has no
    /// conditional update of a comment, so the moment between the read and
    /// the `PATCH` stays open: another account's edit in it is overwritten.
    /// A caller that cares reads again after.
    ///
    /// Only the factory's own comment is edited, through the factory's own
    /// handle (the tehas ports owner's ruling). A handle a site made for
    /// any actor but `Actor::Ours`, the operator's gh, a person's or a
    /// given token's, is `Forbidden` before any request, the bot's comment
    /// as well, once the body is checked. Through the factory's handle the read's author must carry
    /// the user id of the site's bot account and be typed a bot, as
    /// `Bot::is` decides. Any other author, a person's, another bot's, or
    /// an author missing or unread, is `Forbidden` with nothing sent, and
    /// so is every comment through a handle that knows no bot id, one made
    /// with [`GitHub::new`]. The login decides nothing: a renamed App
    /// changes it and another account may hold it.
    ///
    /// `body` takes the place of the comment's text and its markers change
    /// as [`markers::edit`] says, the ones not named kept where they stand;
    /// the whole is read back before it is sent (Decided 116). A body that
    /// holds our wrapper anywhere, or a bare marker, is `Refused(Invalid)`
    /// before any request. A comment that is not there, or not on an issue
    /// of this project, is `NotFound` with nothing sent; one deleted
    /// between the read and the `PATCH` is `Conflict(Gone)`. A body that
    /// would not change is not sent: the answer is the comment's link.
    ///
    /// A `PATCH` that may have landed is settled by one read, never sent
    /// again: at the revision of what was sent it is done, still at `since`
    /// it was not sent, else it stays `sent: Maybe`.
    fn edit_comment(
        &mut self,
        id: &CommentId,
        body: &str,
        edit: &MarkerEdit,
        since: &Revision,
    ) -> Got<WebLink> {
        edit.check()?;
        if markers::foreign_text(body) {
            return Err(Outcome::Refused(Refusal::Invalid));
        }
        if !self.as_bot {
            return Err(Outcome::Forbidden);
        }
        let n = comment_number(id)?;
        let path = format!("{}/issues/comments/{n}", self.issues()?);
        let now = self.comment_before(&path, n)?;
        if !self.bot.as_ref().is_some_and(|b| b.is(&now.author)) {
            return Err(Outcome::Forbidden);
        }
        if now.revision != *since {
            return Err(Outcome::Conflict(Lost::Changed));
        }
        let Some(new) = markers::edit(&now.body, Some(body), edit, Some(now.no))? else {
            return Ok(now.link);
        };
        let input = serde_json::json!({ "body": new }).to_string();
        let failed = match self.send("PATCH", &path, &input) {
            Ok(out) => {
                let edited = self.as_here(|repo| read_comment(&out, repo, now.no))?;
                if edited.id != *id {
                    return Err(Outcome::Unread(Unread::Inconsistent));
                }
                return Ok(edited.link);
            }
            Err(Outcome::NotFound) => return Err(Outcome::Conflict(Lost::Gone)),
            Err(o) => o,
        };
        let Outcome::Unavailable {
            why,
            sent: Sent::Maybe,
        } = failed
        else {
            return Err(failed);
        };
        match self.comment_before(&path, n) {
            Ok(again) if again.revision == comment_revision(&new) => Ok(again.link),
            Ok(again) if again.revision == *since => Err(Outcome::Unavailable {
                why,
                sent: Sent::No,
            }),
            _ => Err(failed),
        }
    }

    fn close(&mut self, _: BlueprintNo, _: bool) -> Did {
        unsupported()
    }
}

impl GitHub {
    /// The REST path of this adapter's repository, the one place its writes
    /// go. A name that is no `owner/name` of GitHub's characters is
    /// `Refused(Invalid)`: it would aim the request somewhere else.
    pub(super) fn issues(&self) -> Got<String> {
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

    /// One write, its JSON on gh's standard input, sent once, after
    /// [`GitHub::confirm_here`]. A failure is
    /// its named outcome: a 422 is GitHub refusing the input, a spent
    /// budget is the writes' pool, and a runner that sends no input sent
    /// nothing. No word of gh's answer, or of the input, is kept.
    pub(super) fn send(&mut self, method: &str, path: &str, input: &str) -> Got<String> {
        self.confirm_here()?;
        let args = ["api", "-X", method, path, "--input", "-"];
        let e = match self.call_input("the write", &args, input) {
            Ok(out) => return Ok(out),
            Err(e) => e,
        };
        if e.why == NO_INPUT {
            return Err(Outcome::Refused(Refusal::Unsupported));
        }
        if e.why.contains("HTTP 422") {
            return Err(Outcome::Refused(Refusal::Invalid));
        }
        Err(match self.outcome(&e) {
            Outcome::Spent { until, .. } => Outcome::Spent {
                pool: Pool::Writes,
                until,
            },
            other => other,
        })
    }

    /// The issue `no` and its revision, read before a write or to settle
    /// one. A failure of the read is its outcome, with nothing sent; a pull
    /// request is `NotFound`; an answer with no title, or of another issue,
    /// is unread.
    fn read_before(&mut self, no: BlueprintNo) -> Got<(Issue, Revision)> {
        let out = self
            .issue_answer(no.get())
            .map_err(|e| unsent(self.outcome(&e)))?;
        let issue = self.as_here(|repo| read_issue(&out, repo))?;
        if issue.no != no {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        let at = revision(&out).ok_or(Outcome::Unread(Unread::Malformed))?;
        Ok((issue, at))
    }
}

impl GitHub {
    /// The comment `n` at `path`, read before a write or to settle one. A
    /// failure of the read is its outcome, with nothing sent.
    fn comment_before(&mut self, path: &str, n: u64) -> Got<Held> {
        let out = self
            .call("the comment", &["api", path])
            .map_err(|e| unsent(self.outcome(&e)))?;
        self.as_here(|repo| held(&out, repo, n))
    }
}

/// The revision of an issue's REST read: its title and body, as the
/// relay's read before an edit has it (`GitHub::text_revision`). None for
/// an answer with no title.
///
/// The contract `Blueprints::blueprint` keeps: it answers the body
/// without our markers, but its `revision` is this function over the RAW
/// answer, markers in. A revision over the stripped
/// body would never equal this one, and every edit at it would be
/// `Conflict(Changed)`.
pub(super) fn revision(out: &str) -> Option<Revision> {
    gh::text_revision(out).map(Revision::new)
}

/// The revision an issue of this title and body reads at, as [`revision`]
/// reads it.
pub(super) fn text_revision(title: &str, body: &str) -> Option<Revision> {
    revision(&serde_json::json!({ "title": title, "body": body }).to_string())
}

/// The revision of a comment: the sum of its raw body, markers in, as
/// every read of comments gives it.
pub(super) fn comment_revision(raw: &str) -> Revision {
    Revision::new(tehas_core::approval::body_sum(raw))
}

/// `Refused(Invalid)` when a text holds one of our markers: a body never
/// does, they travel beside it. As [`markers::foreign_text`] finds one: our
/// wrapper, opening or closing, or a marker the reader reads, bare too.
fn unmarked(texts: &[&str]) -> Did {
    if texts.iter().any(|t| markers::foreign_text(t)) {
        Err(Outcome::Refused(Refusal::Invalid))
    } else {
        Ok(())
    }
}

/// An issue of this project as a read or a write answers it.
pub(super) struct Issue {
    pub(super) no: BlueprintNo,
    pub(super) title: String,
    pub(super) body: String,
    pub(super) link: WebLink,
}

/// The issue in gh's answer. An answer that does not parse or lacks a
/// field every issue has is `Unread(Malformed)`; a pull request is
/// `NotFound`, no blueprint; a link that is not this project's issue of
/// that number is `Unread(Inconsistent)`.
pub(super) fn read_issue(out: &str, repo: Named) -> Got<Issue> {
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
    let title = v
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if v.get("pull_request").is_some_and(|p| !p.is_null()) {
        return Err(Outcome::NotFound);
    }
    if !repo.ends(url, "", &format!("/issues/{}", no.get())) {
        return Err(Outcome::Unread(Unread::Inconsistent));
    }
    let link = WebLink::new(url).map_err(|_| malformed)?;
    Ok(Issue {
        no,
        title,
        body,
        link,
    })
}

/// The comment in gh's answer: its id, and its link, which must be the
/// comment's on this project's issue `no`.
pub(super) fn read_comment(out: &str, repo: Named, no: BlueprintNo) -> Got<Posted<CommentId>> {
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
    if !repo.ends(url, "", &format!("/issues/{}#issuecomment-{id}", no.get())) {
        return Err(Outcome::Unread(Unread::Inconsistent));
    }
    Ok(Posted::new(
        CommentId::new(id.to_string()),
        WebLink::new(url).map_err(|_| malformed)?,
    ))
}

/// The REST id a comment's id is. Any other id names no comment here:
/// `NotFound`, with no request.
fn comment_number(id: &CommentId) -> Got<u64> {
    let text = id.as_str();
    match !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) {
        true => text
            .parse()
            .ok()
            .filter(|n| *n > 0)
            .ok_or(Outcome::NotFound),
        false => Err(Outcome::NotFound),
    }
}

/// A comment of this project's issue `no` as its REST read answers it.
struct Held {
    no: BlueprintNo,
    /// Its author's user id and type as GitHub gives them, each None where
    /// the answer names none or not as GitHub writes it.
    author: gh::Facts,
    body: String,
    link: WebLink,
    revision: Revision,
}

/// The comment `n` in gh's answer. An answer that does not parse or lacks
/// a field every comment has is `Unread(Malformed)`; a comment on a pull
/// request is `NotFound`, no blueprint's; another comment, or one of
/// another project, is `Unread(Inconsistent)`.
fn held(out: &str, repo: Named, n: u64) -> Got<Held> {
    let malformed = Outcome::Unread(Unread::Malformed);
    let v: Value = serde_json::from_str(out.trim()).map_err(|_| malformed.clone())?;
    if v.get("id").and_then(Value::as_u64) != Some(n) {
        return Err(match v.get("id") {
            Some(Value::Number(_)) => Outcome::Unread(Unread::Inconsistent),
            _ => malformed,
        });
    }
    let url = v
        .get("html_url")
        .and_then(Value::as_str)
        .ok_or(malformed.clone())?;
    let body = match v.get("body") {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(_) => return Err(malformed),
    };
    let no = comment_issue(repo, url, n)?;
    let user = v.get("user");
    let author = gh::Facts {
        id: user.and_then(|u| u.get("id")).and_then(Value::as_u64),
        kind: user
            .and_then(|u| u.get("type"))
            .and_then(Value::as_str)
            .map(str::to_string),
    };
    Ok(Held {
        no,
        author,
        revision: comment_revision(&body),
        body,
        link: WebLink::new(url).map_err(|_| malformed)?,
    })
}

/// The issue of this project a comment's link `url` names, the comment
/// `n`: `NotFound` for a comment on a pull request, `Unread(Inconsistent)`
/// for one of another project or another comment.
pub(super) fn comment_issue(repo: Named, url: &str, n: u64) -> Got<BlueprintNo> {
    let inconsistent = Outcome::Unread(Unread::Inconsistent);
    let head = url
        .strip_suffix(&format!("#issuecomment-{n}"))
        .ok_or(inconsistent.clone())?;
    let (at, no) = head.rsplit_once('/').ok_or(inconsistent.clone())?;
    let no = no
        .parse()
        .ok()
        .and_then(BlueprintNo::new)
        .ok_or(inconsistent.clone())?;
    if repo.ends(at, "", "/pull") {
        return Err(Outcome::NotFound);
    }
    if !repo.ends(at, "", "/issues") {
        return Err(inconsistent);
    }
    Ok(no)
}
