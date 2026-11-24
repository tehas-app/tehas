//! The tickets port's writes, `TicketWrites`. Answered: the factory's
//! labels made (`prepare`), or one of them (`prepare_one`), a project's own
//! label (`ensure_mark`), a mark
//! put on or taken off, a note with its markers (`note`), and the
//! ticket's own title and text edited at a revision (`edit`).
//!
//! A write that carries text is one request, sent once and never again:
//! the text goes as JSON on gh's standard input, never in an argument,
//! which holds at most 128 KiB and is seen by every process on the host.
//! It reads the issue first, so a pull request's number is `NotFound` with
//! nothing sent.

use super::blueprint_writes::{ends_with_folded, read_comment, revision, text_revision, unsent};
use super::{label_of, number, unsupported};
use crate::{markers, table, GitHub};
use serde_json::Value;
use tehas_ports::{
    Applied, At, CloseReason, CommentId, Did, Got, Item, Kind, Lost, Mark, MarkSpec, Marker,
    MarkerEdit, NewTicket, Outcome, Posted, Priority, Refusal, Revision, Sent, Step, StepPart,
    TargetEdit, TargetId, TicketKey, TicketWrites, Unprepared, Unread, WebLink,
};

impl TicketWrites for GitHub {
    fn apply(
        &mut self,
        _: &TicketKey,
        step: &Step,
    ) -> (Applied<StepPart>, Option<Posted<CommentId>>) {
        let first = if step.note.is_some() || !step.markers.is_empty() {
            Some(StepPart::Note)
        } else if let Some(m) = step.unmark.first() {
            Some(StepPart::Unmark(m.clone()))
        } else if let Some(m) = step.mark.first() {
            Some(StepPart::Mark(m.clone()))
        } else if step.close.is_some() {
            Some(StepPart::Close)
        } else {
            None
        };
        let mut applied = Applied::default();
        applied.stopped = first.map(|p| (p, Outcome::Refused(Refusal::Unsupported)));
        (applied, None)
    }

    /// Each label of the factory's table, made or brought back to its colour
    /// and text, one call each.
    fn prepare(&mut self) -> Result<(), Unprepared> {
        let mut first = Ok(());
        for spec in table::specs() {
            let made = self.ensure_mark(&spec);
            if let (Ok(()), Err(why)) = (&first, made) {
                let mut unprepared = Unprepared::new(why);
                unprepared.mark = Some(spec.name.clone());
                first = Err(unprepared);
            }
        }
        first
    }

    /// The label of one factory mark read, then made or brought to the
    /// table's colour and text as `prepare` makes each, only where it is
    /// not so already: a label that is right is not written. A project's
    /// own mark, and a mark no label is, are `Refused(Invalid)` with no
    /// request.
    fn prepare_one(&mut self, mark: &Mark) -> Did {
        if matches!(mark, Mark::Project(_)) {
            return Err(Outcome::Refused(Refusal::Invalid));
        }
        let label = label_of(mark)?;
        let spec = table::specs()
            .into_iter()
            .find(|s| s.name.as_str() == label)
            .ok_or(Outcome::Refused(Refusal::Invalid))?;
        match self.label_held(&label)? {
            Some(held) if held.is(&spec) => Ok(()),
            _ => self.ensure_mark(&spec),
        }
    }

    /// `gh label create --force`: made, or its colour and text put back.
    fn ensure_mark(&mut self, spec: &MarkSpec) -> Did {
        let name = spec.name.as_str();
        if !table::is_label(name) {
            return Err(Outcome::Refused(Refusal::Invalid));
        }
        let repo = self.repo.clone();
        let mut args = vec!["label", "create", name, "--repo", &repo];
        if let Some(c) = &spec.colour {
            args.extend(["--color", c.as_str()]);
        }
        if let Some(about) = &spec.about {
            args.extend(["--description", about]);
        }
        args.push("--force");
        self.call("the label", &args)
            .map(|_| ())
            .map_err(|e| self.outcome(&e))
    }

    fn mark(&mut self, key: &TicketKey, mark: &Mark) -> Did {
        let (issue, label) = (number(key)?, label_of(mark)?);
        self.add_label(issue, &label).map_err(|e| self.outcome(&e))
    }

    fn unmark(&mut self, key: &TicketKey, mark: &Mark) -> Did {
        let (issue, label) = (number(key)?, label_of(mark)?);
        self.remove_label(issue, &label)
            .map_err(|e| self.outcome(&e))
    }

    fn close(&mut self, _: &TicketKey, _: &CloseReason) -> Did {
        unsupported()
    }

    /// The issue read, then a comment on it: one `POST` of the text and
    /// its markers as [`markers::write`] makes them. Its id is the
    /// comment's REST id and its link the issue's with
    /// `#issuecomment-<id>`. A time limit or a server's error after the
    /// `POST` went out is `sent: Maybe`, and stays so: the adapter has no
    /// read that tells its comment apart, so the caller settles it from the
    /// item's comments (`Blueprints::comments`).
    fn note(&mut self, key: &TicketKey, body: &str, markers: &[Marker]) -> Got<Posted<CommentId>> {
        let issue = number(key)?;
        let text = markers::write(body, markers, key.same_item())?;
        let path = format!("{}/issues/{issue}/comments", self.issues()?);
        read_first(self, issue)?;
        let input = serde_json::json!({ "body": text }).to_string();
        let out = self.send("POST", &path, &input)?;
        let no = key.same_item().ok_or(Outcome::Refused(Refusal::Invalid))?;
        read_comment(&out, &self.repo, no)
    }

    fn file(&mut self, _: &NewTicket) -> Got<Posted<TicketKey>> {
        unsupported()
    }

    /// The issue read, its revision compared with `since`, then one
    /// `PATCH` of what changes. The revision is the sum of the issue's title
    /// and body as GitHub holds them, markers in (`blueprint_writes::revision`),
    /// the one rule of both ports: a label or a comment moves nothing, a
    /// change of the title or of any byte of the body does, but a title
    /// changed by CRs or LFs alone, which the title's sum drops. GitHub has no
    /// conditional update of an issue, so the moment between the read and
    /// the write stays open: another account's edit in it is overwritten.
    ///
    /// The body changes as [`markers::edit`] says, every other byte where it
    /// was. A text or title given that holds our wrapper anywhere, or a bare
    /// marker, is `Refused(Invalid)` before any request. A title equal to
    /// the one there, and a body that would not change, are not sent; with
    /// nothing to send the answer is the issue's link.
    ///
    /// A `PATCH` that may have landed is settled by one read, never sent
    /// again: at the revision of what was sent it is done, still at `since`
    /// it was not sent, else it stays `sent: Maybe`.
    fn edit(
        &mut self,
        key: &TicketKey,
        title: Option<&str>,
        text: Option<&str>,
        markers: &MarkerEdit,
        since: &Revision,
    ) -> Got<WebLink> {
        let issue = number(key)?;
        markers.check()?;
        if title.into_iter().chain(text).any(markers::foreign_text) {
            return Err(Outcome::Refused(Refusal::Invalid));
        }
        let path = format!("{}/issues/{issue}", self.issues()?);
        let now = read_first(self, issue)?;
        if now.revision.as_ref() != Some(since) {
            return match now.revision {
                Some(_) => Err(Outcome::Conflict(Lost::Changed)),
                None => Err(Outcome::Unread(Unread::Malformed)),
            };
        }
        let body = match text.is_some() || *markers != MarkerEdit::default() {
            true => markers::edit(&now.body, text, markers, key.same_item())?,
            false => None,
        };
        let title = title.filter(|t| *t != now.title);
        let mut fields = serde_json::Map::new();
        if let Some(t) = title {
            fields.insert("title".into(), t.into());
        }
        if let Some(b) = &body {
            fields.insert("body".into(), b.as_str().into());
        }
        if fields.is_empty() {
            return Ok(now.link);
        }
        let failed = match self.send("PATCH", &path, &Value::Object(fields).to_string()) {
            Ok(out) => {
                let edited = issue_of(&out, &self.repo)?;
                if edited.number != issue {
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
        let sent = text_revision(
            title.unwrap_or(&now.title),
            body.as_deref().unwrap_or(&now.body),
        );
        match read_first(self, issue) {
            Ok(again) if again.revision.is_some() && again.revision == sent => Ok(again.link),
            Ok(again) if again.revision.as_ref() == Some(since) => Err(Outcome::Unavailable {
                why,
                sent: Sent::No,
            }),
            _ => Err(failed),
        }
    }

    fn reopen(&mut self, _: &TicketKey) -> Did {
        unsupported()
    }

    fn set_kind(&mut self, _: &TicketKey, _: Option<&Kind>) -> Did {
        unsupported()
    }

    fn set_priority(&mut self, _: &TicketKey, _: Priority) -> Did {
        unsupported()
    }

    fn wait_on(&mut self, _: &TicketKey, _: &Item) -> Did {
        unsupported()
    }

    fn stop_waiting(&mut self, _: &TicketKey, _: &Item) -> Did {
        unsupported()
    }

    fn assign(&mut self, _: &TicketKey, _: &tehas_ports::AccountId) -> Did {
        unsupported()
    }

    fn add_part(&mut self, _: &TicketKey, _: &TicketKey) -> Did {
        unsupported()
    }

    fn remove_part(&mut self, _: &TicketKey, _: &TicketKey) -> Did {
        unsupported()
    }

    fn new_target(&mut self, _: &str, _: Option<At>) -> Got<TargetId> {
        unsupported()
    }

    fn edit_target(&mut self, _: &TargetId, _: &TargetEdit) -> Did {
        unsupported()
    }

    fn set_target(&mut self, _: &TicketKey, _: Option<&TargetId>) -> Did {
        unsupported()
    }
}

/// The issue's REST read before a write, held to be this project's issue
/// `issue`. A failure of the read is its outcome, with nothing sent; a pull
/// request is `NotFound`.
fn read_first(gh: &mut GitHub, issue: u64) -> Got<Issue> {
    let out = gh.issue_answer(issue).map_err(|e| unsent(gh.outcome(&e)))?;
    let read = issue_of(&out, &gh.repo)?;
    if read.number != issue {
        return Err(Outcome::Unread(Unread::Inconsistent));
    }
    Ok(read)
}

/// An issue of this project as a read or a write answers it.
struct Issue {
    number: u64,
    title: String,
    body: String,
    link: WebLink,
    /// Its revision, None for an answer with no title.
    revision: Option<Revision>,
}

/// The issue in gh's answer. An answer that does not parse or lacks a
/// field every issue has is `Unread(Malformed)`; a pull request is
/// `NotFound`, no ticket; a link that is not this project's issue of that
/// number is `Unread(Inconsistent)`.
fn issue_of(out: &str, repo: &str) -> Got<Issue> {
    let malformed = Outcome::Unread(Unread::Malformed);
    let v: Value = serde_json::from_str(out.trim()).map_err(|_| malformed.clone())?;
    let number = v
        .get("number")
        .and_then(Value::as_u64)
        .filter(|n| *n > 0)
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
    let text = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    if v.get("pull_request").is_some_and(|p| !p.is_null()) {
        return Err(Outcome::NotFound);
    }
    if !ends_with_folded(url, &format!("/{repo}/issues/{number}")) {
        return Err(Outcome::Unread(Unread::Inconsistent));
    }
    Ok(Issue {
        number,
        title: text("title").unwrap_or_default(),
        body,
        link: WebLink::new(url).map_err(|_| malformed)?,
        revision: revision(out),
    })
}
