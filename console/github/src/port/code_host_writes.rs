//! The code host port's writes, `CodeHostWrites`. Answered: a change opened
//! (`open_change`, in `open.rs`), a comment on a change (`comment_change`),
//! a mark put on a change or taken off it (`mark_change`,
//! `unmark_change`), and a status on a commit (`set_status`). Every other write answers
//! `Refused(Unsupported)` before any request.
//!
//! Each write to a change reads the pull request first, `repos/<repo>/pulls/<n>`, which
//! answers 404 for an issue's number: an issue, or a number that is
//! nothing, is `NotFound` with nothing sent. Each write is then one request
//! to this adapter's own repository, sent once, never again. A comment's
//! body goes as JSON on gh's standard input, never in an argument. A mark is
//! its label from `table.rs`, written through the issues path as the
//! tickets port's marks are; adding a label creates it when the repository
//! lacks it, and `prepare` gives the factory's their colour and text.
//!
//! A mark put on or taken off that may have landed is settled by one read
//! of the change's labels, as the ports spec's "`Sent::Maybe` is settled in
//! the adapter first" asks; a comment is not, and stays `sent: Maybe`.
//!
//! A comment carries no marker: this adapter has no writer of markers
//! beside a body yet, so one named is `Refused(Unsupported)` and a body
//! that holds one, as `markers::foreign_text` finds it, is
//! `Refused(Invalid)`.

mod open;

use super::{label_of, unsupported};
use crate::{markers, ForgeError, GitHub};
use serde_json::Value;
use tehas_ports::{
    Applied, Blobs, ChangeNo, CodeHostWrites, CommentId, CommitId, Did, FileName, Got, Mark,
    Marker, MarkerEdit, MergeHow, NewChange, NewRelease, Outcome, Pool, Posted, Proposal,
    ProposalPart, Proposed, Refusal, ReleaseId, Sent, Status, StatusState, Sum, TagName, Unread,
    WebLink,
};

/// What `GhRunner::run_input`'s default answers: the runner sends nothing.
const NO_INPUT: &str = "this runner takes no input";
/// The most characters of a status's description GitHub keeps.
const STATUS_ABOUT_MAX: usize = 140;

impl CodeHostWrites for GitHub {
    /// See [`open`]: a pull request opened from a branch of this
    /// repository, once.
    fn open_change(&mut self, new: &NewChange) -> Got<Posted<ChangeNo>> {
        self.open_pull(new)
    }

    fn edit_change(
        &mut self,
        _: ChangeNo,
        _: Option<&str>,
        _: Option<&str>,
        _: &MarkerEdit,
    ) -> Got<WebLink> {
        unsupported()
    }

    /// The label added to the open pull request. One it carries is done
    /// with nothing sent; a closed or merged one is `Refused(Invalid)`.
    fn mark_change(&mut self, no: ChangeNo, mark: &Mark) -> Did {
        let label = label_of(mark)?;
        let pull = self.pull_before(no)?;
        if !pull.open {
            return Err(Outcome::Refused(Refusal::Invalid));
        }
        if pull.carries(&label) {
            return Ok(());
        }
        match self.add_label(no.get(), &label) {
            Ok(()) => Ok(()),
            Err(e) => {
                let failed = self.written(&e);
                self.settle(no, failed, |p| p.carries(&label))
            }
        }
    }

    /// The label taken off the pull request, open or not. One it does not
    /// carry is done with nothing sent, and so is a 404 on the removal.
    fn unmark_change(&mut self, no: ChangeNo, mark: &Mark) -> Did {
        let label = label_of(mark)?;
        let pull = self.pull_before(no)?;
        if !pull.carries(&label) {
            return Ok(());
        }
        match self.remove_label(no.get(), &label) {
            Ok(()) => Ok(()),
            Err(e) => {
                let failed = self.written(&e);
                self.settle(no, failed, |p| !p.carries(&label))
            }
        }
    }

    /// One `POST` of the body on the pull request's conversation. Its id is
    /// the REST comment id, its link the pull request's with
    /// `#issuecomment-<id>`.
    fn comment_change(
        &mut self,
        no: ChangeNo,
        body: &str,
        markers: &[Marker],
    ) -> Got<Posted<CommentId>> {
        if !markers.is_empty() {
            return unsupported();
        }
        if markers::foreign_text(body) {
            return Err(Outcome::Refused(Refusal::Invalid));
        }
        let path = format!("{}/issues/{}/comments", project(&self.repo)?, no.get());
        let pull = self.pull_before(no)?;
        let input = serde_json::json!({ "body": body }).to_string();
        let args = ["api", "-X", "POST", &path, "--input", "-"];
        let out = self
            .call_input("the change's comment", &args, &input)
            .map_err(|e| self.written(&e))?;
        read_comment(&out, &self.repo, no, &pull.link)
    }

    fn close_change(&mut self, _: ChangeNo) -> Did {
        unsupported()
    }

    fn merge(&mut self, _: ChangeNo, _: &CommitId, _: MergeHow) -> Did {
        unsupported()
    }

    fn propose(&mut self, _: &Proposal, _: &dyn Blobs) -> Result<Proposed, Applied<ProposalPart>> {
        let mut applied = Applied::default();
        applied.stopped = Some((ProposalPart::Branch, Outcome::Refused(Refusal::Unsupported)));
        Err(applied)
    }

    fn tag(&mut self, _: &TagName, _: &CommitId) -> Did {
        unsupported()
    }

    /// One status on commit `at` of this repository, sent once: GitHub
    /// keeps the newest of a name, so the same status sent again is the
    /// same answer, and a row left unknown is settled by sending it again.
    /// Its about is cut to the characters GitHub keeps. A name that is
    /// empty or more than one line, or an about that holds a marker as
    /// `markers::foreign_text` finds one, is
    /// `Refused(Invalid)` with nothing sent.
    fn set_status(&mut self, at: &CommitId, status: &Status) -> Did {
        let path = format!("{}/statuses/{}", project(&self.repo)?, at.as_str());
        if status.name.trim().is_empty() || status.name.contains(['\n', '\r']) {
            return Err(Outcome::Refused(Refusal::Invalid));
        }
        if markers::foreign_text(&status.about) {
            return Err(Outcome::Refused(Refusal::Invalid));
        }
        let state = status_state(status.state);
        let about: String = status.about.chars().take(STATUS_ABOUT_MAX).collect();
        let mut body = serde_json::json!({
            "state": state,
            "context": status.name,
            "description": about,
        });
        if let Some(link) = &status.link {
            body["target_url"] = link.as_str().into();
        }
        let args = ["api", "-X", "POST", &path, "--input", "-"];
        let out = self
            .call_input("the commit's status", &args, &body.to_string())
            .map_err(|e| self.written(&e))?;
        read_status(&out, &status.name, state)
    }

    fn new_release(&mut self, _: &NewRelease) -> Got<Posted<ReleaseId>> {
        unsupported()
    }

    fn upload(&mut self, _: &ReleaseId, _: &FileName, _: &Sum, _: &dyn Blobs) -> Did {
        unsupported()
    }

    fn publish_release(&mut self, _: &ReleaseId) -> Did {
        unsupported()
    }
}

impl GitHub {
    /// The pull request `no` of this repository, read before a write or to
    /// settle one. A failure of the read is its outcome with nothing sent;
    /// an issue's number answers 404, `NotFound`.
    fn pull_before(&mut self, no: ChangeNo) -> Got<Pull> {
        let path = format!("{}/pulls/{}", project(&self.repo)?, no.get());
        let out = self
            .call("the change", &["api", &path])
            .map_err(|e| unsent(self.outcome(&e)))?;
        read_pull(&out, &self.repo, no)
    }

    /// The named outcome of a write that failed. A 422 is GitHub refusing
    /// the input, a spent budget is the writes' pool, and a runner that
    /// takes no input sent nothing.
    fn written(&self, e: &ForgeError) -> Outcome {
        if e.why == NO_INPUT {
            return Outcome::Refused(Refusal::Unsupported);
        }
        if e.why.contains("HTTP 422") {
            return Outcome::Refused(Refusal::Invalid);
        }
        match self.outcome(e) {
            Outcome::Spent { until, .. } => Outcome::Spent {
                pool: Pool::Writes,
                until,
            },
            other => other,
        }
    }

    /// A mark's write that may have landed, settled by one read of the
    /// change: done when `landed` holds, else the write's own failure.
    /// Never sent again.
    fn settle(&mut self, no: ChangeNo, failed: Outcome, landed: impl Fn(&Pull) -> bool) -> Did {
        if !matches!(
            failed,
            Outcome::Unavailable {
                sent: Sent::Maybe,
                ..
            }
        ) {
            return Err(failed);
        }
        match self.pull_before(no) {
            Ok(p) if landed(&p) => Ok(()),
            _ => Err(failed),
        }
    }
}

/// The REST path of this adapter's repository, the one place its writes
/// go. A name that is no `owner/name` of GitHub's characters is
/// `Refused(Invalid)`: it would aim the request somewhere else.
fn project(repo: &str) -> Got<String> {
    let (owner, name) = repo
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

/// A pull request of this repository as its read answers it.
struct Pull {
    open: bool,
    labels: Vec<String>,
    link: WebLink,
}

impl Pull {
    /// It carries `label`. GitHub compares label names without case.
    fn carries(&self, label: &str) -> bool {
        self.labels.iter().any(|l| l.eq_ignore_ascii_case(label))
    }
}

/// The pull request in gh's answer. An answer that does not parse or lacks
/// a field every pull request has is `Unread(Malformed)`; one of another
/// number or another repository is `Unread(Inconsistent)`.
fn read_pull(out: &str, repo: &str, no: ChangeNo) -> Got<Pull> {
    let malformed = Outcome::Unread(Unread::Malformed);
    let v: Value = serde_json::from_str(out.trim()).map_err(|_| malformed.clone())?;
    let number = v
        .get("number")
        .and_then(Value::as_u64)
        .ok_or(malformed.clone())?;
    let url = v
        .get("html_url")
        .and_then(Value::as_str)
        .ok_or(malformed.clone())?;
    let open = match v.get("state").and_then(Value::as_str) {
        Some("open") => true,
        Some("closed") => false,
        _ => return Err(malformed),
    };
    let labels = v
        .get("labels")
        .and_then(Value::as_array)
        .ok_or(malformed.clone())?
        .iter()
        .map(|l| l.get("name").and_then(Value::as_str).map(str::to_string))
        .collect::<Option<Vec<_>>>()
        .ok_or(malformed.clone())?;
    if number != no.get() || !ends_with_folded(url, &format!("/{repo}/pull/{number}")) {
        return Err(Outcome::Unread(Unread::Inconsistent));
    }
    let link = WebLink::new(url).map_err(|_| malformed)?;
    Ok(Pull { open, labels, link })
}

/// The comment in gh's answer: its id, and its link, which must be the
/// comment's on this repository's pull request `no`. The link answered is
/// the pull request's read link with `#issuecomment-<id>`.
fn read_comment(out: &str, repo: &str, no: ChangeNo, pull: &WebLink) -> Got<Posted<CommentId>> {
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
    let tail = format!("/{repo}/pull/{}#issuecomment-{id}", no.get());
    if !ends_with_folded(url, &tail) {
        return Err(Outcome::Unread(Unread::Inconsistent));
    }
    let link =
        WebLink::new(format!("{}#issuecomment-{id}", pull.as_str())).map_err(|_| malformed)?;
    Ok(Posted::new(CommentId::new(id.to_string()), link))
}

/// GitHub's word for a status's state.
fn status_state(state: StatusState) -> &'static str {
    match state {
        StatusState::Pending => "pending",
        StatusState::Passed => "success",
        StatusState::Failed => "failure",
        StatusState::Errored => "error",
    }
}

/// The status in gh's answer, which must be the one sent: its name and its
/// state. An answer that does not parse is `Unread(Malformed)`; one of
/// another name or state is `Unread(Inconsistent)`.
fn read_status(out: &str, name: &str, state: &str) -> Did {
    let v: Value =
        serde_json::from_str(out.trim()).map_err(|_| Outcome::Unread(Unread::Malformed))?;
    let field = |k: &str| v.get(k).and_then(Value::as_str);
    match (field("context"), field("state")) {
        (Some(c), Some(s)) if c == name && s == state => Ok(()),
        (Some(_), Some(_)) => Err(Outcome::Unread(Unread::Inconsistent)),
        _ => Err(Outcome::Unread(Unread::Malformed)),
    }
}

/// `text` ends with `tail`, ASCII case folded: GitHub names a project in
/// any case.
fn ends_with_folded(text: &str, tail: &str) -> bool {
    text.len() >= tail.len()
        && text.is_char_boundary(text.len() - tail.len())
        && text[text.len() - tail.len()..].eq_ignore_ascii_case(tail)
}
