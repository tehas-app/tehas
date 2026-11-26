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

use super::blueprint_writes::{read_comment, revision, text_revision, unsent};
use super::named::Named;
use super::{label_of, number, unsupported};
use crate::forge::uri;
use crate::vocabulary::LabelHeld;
use crate::{markers, table, GitHub};
use serde_json::Value;
use tehas_ports::{
    Applied, At, CloseReason, Closed, CommentId, Did, Got, Item, Kind, Lost, Mark, MarkSpec,
    Marker, MarkerEdit, NewTicket, Outcome, Posted, Priority, Refusal, Revision, Sent, Step,
    StepPart, TargetEdit, TargetId, TicketKey, TicketWrites, Unprepared, Unread, WebLink,
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

    /// The repository's labels read once, then each label of the factory's
    /// table brought to its name, colour and text as `prepare_one` brings
    /// one ([`GitHub::bring_label`]): a label that is right is not written.
    /// A read that fails is the first failure, and each label is then made
    /// or put back by `ensure_mark`, as with no read.
    fn prepare(&mut self) -> Result<(), Unprepared> {
        let held = self.labels_held();
        let mut first = match &held {
            Ok(_) => Ok(()),
            Err(why) => Err(Unprepared::new(why.clone())),
        };
        for spec in table::specs() {
            let made = match &held {
                Ok(all) => {
                    let name = spec.name.as_str();
                    let of = all
                        .iter()
                        .find(|h| h.name() == name)
                        .or_else(|| all.iter().find(|h| h.name().eq_ignore_ascii_case(name)));
                    self.bring_label(of, &spec)
                }
                Err(_) => self.ensure_mark(&spec),
            };
            if let (Ok(()), Err(why)) = (&first, made) {
                let mut unprepared = Unprepared::new(why);
                unprepared.mark = Some(spec.name.clone());
                first = Err(unprepared);
            }
        }
        first
    }

    /// The label of one factory mark read, then brought to the table's
    /// name, colour and text as `prepare` brings each
    /// ([`GitHub::bring_label`]). A project's own mark, and a mark no label
    /// is, are `Refused(Invalid)` with no request.
    fn prepare_one(&mut self, mark: &Mark) -> Did {
        if matches!(mark, Mark::Project(_)) {
            return Err(Outcome::Refused(Refusal::Invalid));
        }
        let label = label_of(mark)?;
        let spec = table::specs()
            .into_iter()
            .find(|s| s.name.as_str() == label)
            .ok_or(Outcome::Refused(Refusal::Invalid))?;
        let held = self.label_held(&label)?;
        self.bring_label(held.as_ref(), &spec)
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

    fn close(&mut self, key: &TicketKey, reason: &CloseReason) -> Did {
        self.close_at(key, reason).map(|_| ())
    }

    /// The issue read, then one `PATCH` of its state and the reason GitHub
    /// keeps (`completed`, `not_planned`, `duplicate`; REST names no
    /// original of a duplicate). A closed issue is done with nothing sent and
    /// keeps the reason it has: closed is closed. The answer is the
    /// `closed_at` of whichever answer found the issue closed: the raiser
    /// of the fact keys on the site's moment, not its own.
    fn close_at(&mut self, key: &TicketKey, reason: &CloseReason) -> Closed {
        let issue = number(key)?;
        let path = format!("{}/issues/{issue}", self.issues()?);
        let out = self
            .issue_answer(issue)
            .map_err(|e| unsent(self.outcome(&e)))?;
        self.as_here(|repo| issue_of(&out, repo))?;
        if state_of(&out)? == "closed" {
            return closed_at_of(&out);
        }
        let why = match reason {
            CloseReason::Done => "completed",
            CloseReason::NotPlanned => "not_planned",
            CloseReason::Duplicate(_) => "duplicate",
        };
        let input = serde_json::json!({ "state": "closed", "state_reason": why }).to_string();
        let answer = self.send("PATCH", &path, &input)?;
        match state_of(&answer)? {
            "closed" => closed_at_of(&answer),
            _ => Err(Outcome::Unread(Unread::Inconsistent)),
        }
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
        self.as_here(|repo| read_comment(&out, repo, no))
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
                let edited = self.as_here(|repo| issue_of(&out, repo))?;
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

    /// The account's login read by its id, then one `POST` of it to the
    /// issue's assignees. GitHub answers 201 and drops a login it cannot
    /// assign without a word, so the answer's assignees are read: ours not
    /// among them is `Refused(Invalid)`, never done on the status alone. A
    /// bot or an organisation holds no assignment: `Refused(Invalid)` with
    /// nothing sent.
    fn assign(&mut self, key: &TicketKey, who: &tehas_ports::AccountId) -> Did {
        let issue = number(key)?;
        let id = super::identity::id_of(who)?;
        let path = format!("{}/issues/{issue}/assignees", self.issues()?);
        let login = self
            .login_of(id)
            .map_err(unsent)?
            .ok_or(Outcome::Refused(Refusal::Invalid))?;
        let input = serde_json::json!({ "assignees": [login] }).to_string();
        let out = self.send("POST", &path, &input)?;
        let v: Value = serde_json::from_str(out.trim()).map_err(|_| MALFORMED)?;
        let assignees = v
            .get("assignees")
            .and_then(Value::as_array)
            .ok_or(MALFORMED)?;
        if assignees
            .iter()
            .any(|a| a.get("id").and_then(Value::as_u64) == Some(id))
        {
            Ok(())
        } else {
            Err(Outcome::Refused(Refusal::Invalid))
        }
    }

    /// The child's parent read first: already this parent's part is done
    /// with nothing sent, another parent's, in this repository or another,
    /// is `Refused(Invalid)`, never moved. Then the child's own issue read for its id, which the
    /// sub-issues endpoint takes, and one `POST` of it to the parent's.
    fn add_part(&mut self, parent: &TicketKey, child: &TicketKey) -> Did {
        let (up, down) = (number(parent)?, number(child)?);
        if up == down {
            return Err(Outcome::Refused(Refusal::Cycle));
        }
        let issues = self.issues()?;
        let asked = format!("{issues}/issues/{down}/parent");
        match self.call("the parent", &["api", &asked]) {
            // A parent of another repository is another parent.
            Ok(out) => {
                return match self.as_here(|repo| issue_of(&out, repo)) {
                    Ok(p) if p.number == up => Ok(()),
                    Ok(_) | Err(Outcome::Unread(Unread::Inconsistent)) => {
                        Err(Outcome::Refused(Refusal::Invalid))
                    }
                    Err(o) => Err(o),
                }
            }
            Err(e) if crate::gh::says_not_found(&e.why) => {}
            Err(e) => return Err(unsent(self.outcome(&e))),
        }
        let out = self
            .issue_answer(down)
            .map_err(|e| unsent(self.outcome(&e)))?;
        self.as_here(|repo| issue_of(&out, repo))?;
        let id = serde_json::from_str::<Value>(out.trim())
            .ok()
            .and_then(|v| v.get("id").and_then(Value::as_u64))
            .ok_or(MALFORMED)?;
        let path = format!("{issues}/issues/{up}/sub_issues");
        let input = serde_json::json!({ "sub_issue_id": id }).to_string();
        self.send("POST", &path, &input).map(|_| ())
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
    let read = gh.as_here(|repo| issue_of(&out, repo))?;
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
fn issue_of(out: &str, repo: Named) -> Got<Issue> {
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
    if !repo.ends(url, "", &format!("/issues/{number}")) {
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

impl GitHub {
    /// The label of `spec` brought to the table from `held`, the label
    /// GitHub holds by that name in any case: left where it is right,
    /// renamed where its name is in another case, else made or put back by
    /// `ensure_mark`. The one rule of `prepare` and `prepare_one`.
    fn bring_label(&mut self, held: Option<&LabelHeld>, spec: &MarkSpec) -> Did {
        match held {
            Some(h) if h.is(spec) => Ok(()),
            Some(h) if h.name() != spec.name.as_str() => self.rename_label(h.name(), spec),
            _ => self.ensure_mark(spec),
        }
    }

    /// One `PATCH` of the label GitHub holds as `held` to the spec's name,
    /// colour and text. `gh label create --force` keeps the case of the
    /// name there, so a name in another case is renamed here; GitHub keeps
    /// the label's id across a rename (Decided 92).
    fn rename_label(&mut self, held: &str, spec: &MarkSpec) -> Did {
        let path = format!("{}/labels/{}", self.issues()?, uri(held));
        let mut input = serde_json::json!({ "new_name": spec.name.as_str() });
        if let Some(c) = &spec.colour {
            input["color"] = c.as_str().into();
        }
        if let Some(about) = &spec.about {
            input["description"] = about.as_str().into();
        }
        self.send("PATCH", &path, &input.to_string()).map(|_| ())
    }
}

/// An answer that does not read.
const MALFORMED: Outcome = Outcome::Unread(Unread::Malformed);

/// The `state` of an issue's answer: `open` or `closed`.
fn state_of(out: &str) -> Got<&'static str> {
    let v: Value = serde_json::from_str(out.trim()).map_err(|_| MALFORMED)?;
    match v.get("state").and_then(Value::as_str) {
        Some("open") => Ok("open"),
        Some("closed") => Ok("closed"),
        _ => Err(MALFORMED),
    }
}

/// The `closed_at` of an issue's answer, when it has one.
fn closed_at_of(out: &str) -> Got<Option<At>> {
    let v: Value = serde_json::from_str(out.trim()).map_err(|_| MALFORMED)?;
    match v.get("closed_at").and_then(Value::as_str) {
        Some(s) => crate::ticket::time(s).map(Some),
        None => Ok(None),
    }
}
