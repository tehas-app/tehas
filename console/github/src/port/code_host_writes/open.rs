//! `CodeHostWrites::open_change`: a pull request opened from a branch of
//! this repository.
//!
//! The open pull requests of the head are read first,
//! `repos/<repo>/pulls?state=open&head=<owner>:<branch>`: one there is
//! `Refused(Exists)` with nothing sent, as the fake answers, so a change is
//! never opened twice. Then one `POST repos/<repo>/pulls`, its title, head,
//! base, body and `draft` as JSON on gh's standard input, sent once, never
//! again.
//!
//! What the change closes is written after the body, one line per
//! blueprint, in the words 1.x's finish wrote at the top of its draft:
//! `Fixes #<n>.` (`console/cycle/src/finish/plan.rs`, `body`). A blueprint
//! the body already closes in GitHub's syntax gets no second line.
//!
//! GitHub answers 422 for a head that already has an open pull request, a
//! head with no commit past the base, a branch that is not there, and a
//! draft the repository's plan does not allow; gh gives the adapter the
//! status alone. One read of the head tells the first apart:
//! `Refused(Exists)`. Every other 422 is `Refused(Invalid)`: a draft is
//! never opened as a ready pull request instead.
//!
//! A `POST` that may have landed (a time limit, a 5xx) is settled by one
//! read of the head, as the spec's "`Sent::Maybe` is settled in the
//! adapter first" asks: one open pull request of the head, where the read
//! before found none, is the one opened. Else it stays `sent: Maybe`.

use super::{ends_with_folded, project, unsent};
use crate::forge::uri;
use crate::{markers, GitHub};
use serde::Deserialize;
use tehas_ports::{
    BlueprintNo, Body, ChangeNo, Got, NewChange, Outcome, Posted, Refusal, Sent, Unread, WebLink,
};

impl GitHub {
    pub(super) fn open_pull(&mut self, new: &NewChange) -> Got<Posted<ChangeNo>> {
        let Body::Text(body) = &new.body else {
            return Err(Outcome::Refused(Refusal::Invalid));
        };
        if markers::foreign_text(&new.title) || markers::foreign_text(body) {
            return Err(Outcome::Refused(Refusal::Invalid));
        }
        let path = format!("{}/pulls", project(&self.repo)?);
        let head = new.head.as_str();
        let before = self.pulls_of(head).map_err(unsent)?;
        if !before.is_empty() {
            return Err(Outcome::Refused(Refusal::Exists));
        }
        let input = serde_json::json!({
            "title": new.title,
            "head": head,
            "base": new.base.as_str(),
            "body": with_closes(body, &new.closes),
            "draft": new.draft,
        })
        .to_string();
        let args = ["api", "-X", "POST", &path, "--input", "-"];
        let failed = match self.call_input("the change", &args, &input) {
            Ok(out) => return read_opened(&out, &self.repo, head, new.draft),
            Err(e) => self.written(&e),
        };
        match failed {
            Outcome::Refused(Refusal::Invalid) => match self.pulls_of(head) {
                Ok(there) if !there.is_empty() => Err(Outcome::Refused(Refusal::Exists)),
                _ => Err(failed),
            },
            Outcome::Unavailable {
                sent: Sent::Maybe, ..
            } => match self.pulls_of(head) {
                Ok(mut there) if there.len() == 1 => Ok(there.remove(0)),
                _ => Err(failed),
            },
            _ => Err(failed),
        }
    }

    /// The open pull requests of `head` in this repository. An answer that
    /// is no list is `Unread(Malformed)`; one of another head or another
    /// repository is `Unread(Inconsistent)`.
    fn pulls_of(&mut self, head: &str) -> Got<Vec<Posted<ChangeNo>>> {
        let path = project(&self.repo)?;
        let owner = self.repo.split_once('/').map_or("", |(o, _)| o);
        let ask = format!(
            "{path}/pulls?state=open&head={}:{}&per_page=100",
            uri(owner),
            uri(head)
        );
        let out = self
            .call("the change's head", &["api", &ask])
            .map_err(|e| self.outcome(&e))?;
        let pulls: Vec<Pull> = serde_json::from_str(out.trim()).map_err(|_| MALFORMED)?;
        pulls
            .into_iter()
            .map(|p| read_pull(p, &self.repo, head))
            .collect()
    }
}

const MALFORMED: Outcome = Outcome::Unread(Unread::Malformed);

/// `body`, then one closing line per blueprint `closes` names, each once,
/// after a blank line. A blueprint the body already closes in GitHub's
/// syntax, as 1.x's finish body does (`Fixes #7.` at its top), gets no
/// second line: the reference is never dropped and never doubled.
fn with_closes(body: &str, closes: &[BlueprintNo]) -> String {
    let mut text = body.to_string();
    let mut said: Vec<BlueprintNo> = Vec::new();
    for b in closes {
        if said.contains(b) || tehas_core::issue_pull::closes_issue(body, b.get()) {
            continue;
        }
        if said.is_empty() && !text.is_empty() {
            if !text.ends_with('\n') {
                text.push('\n');
            }
            text.push('\n');
        }
        text.push_str(&format!("Fixes #{}.\n", b.get()));
        said.push(*b);
    }
    text
}

/// A pull request as a REST answer gives the fields read here.
#[derive(Deserialize)]
struct Pull {
    number: u64,
    html_url: String,
    head: Head,
    draft: Option<bool>,
}

#[derive(Deserialize)]
struct Head {
    #[serde(rename = "ref")]
    name: String,
    repo: Option<HeadRepo>,
}

#[derive(Deserialize)]
struct HeadRepo {
    full_name: String,
}

/// The pull request a `POST` answered: this repository's, of `head`, a
/// draft when one was asked.
fn read_opened(out: &str, repo: &str, head: &str, draft: bool) -> Got<Posted<ChangeNo>> {
    let pull: Pull = serde_json::from_str(out.trim()).map_err(|_| MALFORMED)?;
    let is_draft = pull.draft.ok_or(MALFORMED)?;
    let posted = read_pull(pull, repo, head)?;
    if draft && !is_draft {
        return Err(Outcome::Unread(Unread::Inconsistent));
    }
    Ok(posted)
}

/// One pull request of a REST answer, its number and link: its link must
/// be this repository's pull request of that number, and its head `head`
/// in this repository.
fn read_pull(pull: Pull, repo: &str, head: &str) -> Got<Posted<ChangeNo>> {
    let no = ChangeNo::new(pull.number).ok_or(MALFORMED)?;
    let here = ends_with_folded(&pull.html_url, &format!("/{repo}/pull/{}", no.get()))
        && pull.head.name == head
        && pull
            .head
            .repo
            .is_some_and(|r| r.full_name.eq_ignore_ascii_case(repo));
    if !here {
        return Err(Outcome::Unread(Unread::Inconsistent));
    }
    Ok(Posted::new(
        no,
        WebLink::new(pull.html_url).map_err(|_| MALFORMED)?,
    ))
}
