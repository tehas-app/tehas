//! The forge: what tehas asks of the site its issues live on, in tehas terms.
//!
//! tehas keeps an issue's state in labels and decides from the account that
//! put a label on (AGENTS.md rule 5). Today about twenty scripts ask GitHub
//! for those things themselves, each in its own shape, so the forge is welded
//! in. This module is the first half of the seam: one trait that says what
//! tehas wants to know, and one adapter that answers it from GitHub's labels
//! the way `lib/labels.sh` and `libexec/dispatch` do today. Nothing calls it
//! yet; the next issues of #800 give bash a command for it and move
//! `libexec/tidy`, `libexec/status` and `lib/labels.sh` onto that command.
//!
//! The terms are tehas's, not GitHub's: an issue's labels, the newest event of
//! one label, who applied the label that lets a cycle start, and putting a
//! label on or taking it off. Whether a term is backed by a label or, later,
//! by a native field, is the adapter's choice and never a caller's: a caller
//! that named a GitHub field would still be writing GitHub. Whether a label is
//! on the issue now is no term of its own, because a caller that asks for one
//! and not the other is how a stale read decides something.
//!
//! Errors are values. A call that fails names the call and the first of gh's
//! error words, so a caller can log why it stopped without holding gh's whole
//! answer, which may name a host or carry a token. Nothing here panics.
//!
//! The reading of gh's answers, and the rules of `lib/labels.sh` that decide
//! which of an issue's events answer a term, are in [`gh`].

mod gh;

/// The label whose latest application approves work. The term below names it
/// once, so a caller asks for an approval and never for this string.
const APPROVAL_LABEL: &str = "tehas:ready";

/// A term the forge could not answer, as a value rather than a panic.
///
/// `call` names the term that stopped, so one line says what was attempted,
/// and `why` is the first of gh's error words: empty when gh printed none of
/// them, which is why the line is built rather than held. An error is never
/// an empty answer, and an empty answer is never an error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeError {
    pub call: String,
    pub why: String,
}

impl ForgeError {
    /// The error of one call, with gh's reason, which may be empty.
    pub fn new(call: &str, why: impl Into<String>) -> ForgeError {
        ForgeError {
            call: call.to_string(),
            why: why.into(),
        }
    }

    /// The line a caller logs, shaped as the poll's lines are: what failed,
    /// and gh's own words when it had any.
    pub fn line(&self) -> String {
        if self.why.is_empty() {
            format!("ERROR: {} failed", self.call)
        } else {
            format!("ERROR: {} failed{}", self.call, self.why)
        }
    }
}

/// The latest `labeled` or `unlabeled` event of one label: its kind, the
/// account behind it, and its time. A field is empty when the event names
/// none, which is what GitHub answers for an event whose actor is gone; an
/// event with no time is read as older than any stamped one.
///
/// An empty `actor` is a fact, not a refusal: the caller compares it with the
/// allowed list and decides that nobody was asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabelEvent {
    /// `labeled` or `unlabeled`.
    pub kind: String,
    /// The account that changed the label, empty when the event names none.
    pub actor: String,
    /// The event's stamp, empty when it has none.
    pub at: String,
}

/// What tehas asks of the site its issues live on. `issue` is the issue's
/// number in every term.
///
/// The trait holds only what tehas needs and names no endpoint, no field and
/// no JSON shape, because a second forge would be written against it. Every
/// term answers a value or a [`ForgeError`]: a term that cannot be answered
/// says so, and the caller decides what a missing answer is worth.
pub trait Forge {
    /// The labels the issue carries, in the order the site gave them.
    fn labels(&mut self, issue: u64) -> Result<Vec<String>, ForgeError>;

    /// The newest `labeled` or `unlabeled` event of `label`, or None when no
    /// event of the issue carries it.
    fn label_state(&mut self, issue: u64, label: &str) -> Result<Option<LabelEvent>, ForgeError>;

    /// Who applied the label that approves work most recently, and when: the
    /// account that let a cycle start, for the caller to compare with the
    /// allowed list. Only an application answers it: a label taken off does
    /// not, whoever took it off. Empty fields when no event applied it.
    fn approval(&mut self, issue: u64) -> Result<(String, String), ForgeError>;

    /// Put `label` on the issue. An issue that carries it already is the
    /// state wanted, not an error.
    fn add_label(&mut self, issue: u64, label: &str) -> Result<(), ForgeError>;

    /// Take `label` off the issue. An issue that does not carry it is already
    /// in the state wanted, not an error.
    fn remove_label(&mut self, issue: u64, label: &str) -> Result<(), ForgeError>;
}

/// gh, behind a trait of its own, so the adapter's rules are read without gh,
/// a token or a network: a test hands [`GitHub`] a stub that answers from
/// fixtures, and what it decides is the rules alone.
///
/// `run` is one gh call: the words after `gh`, and either what it printed on
/// its standard output or why it did not. A runner that runs gh for real gives
/// the call a bound of its own; this says nothing about how long it may take.
pub trait GhRunner {
    /// Run gh with these arguments: its standard output, or the reason it
    /// gave for failing.
    fn run(&mut self, args: &[String]) -> Result<String, String>;
}

/// The GitHub adapter: every term backed by labels, as the bash backs them
/// today. It holds the repository it asks and the runner that asks GitHub, and
/// reads nothing else.
///
/// The reading of an answer, and the ordering of the events that answers
/// `label_state` and `approval`, are the rules of [`gh`]: the state term
/// reads both kinds of label event, the approval term only applications.
pub struct GitHub {
    /// `owner/name`, as `TEHAS_REPO` gives it.
    pub repo: String,
    /// The program gh runs as. A host runs its shim, `host/shim/gh`, which
    /// takes the token from `TEHAS_GH_TOKEN` when the caller holds one.
    pub gh: String,
    pub runner: Box<dyn GhRunner>,
}

impl GitHub {
    /// The adapter of one repository, asking GitHub through `runner`.
    pub fn new(
        repo: impl Into<String>,
        gh: impl Into<String>,
        runner: Box<dyn GhRunner>,
    ) -> GitHub {
        GitHub {
            repo: repo.into(),
            gh: gh.into(),
            runner,
        }
    }

    /// Ask gh, keeping the reason a failed call gave. The words after `gh` are
    /// what the adapter built; `call` is what a [`ForgeError`] names.
    fn call(&mut self, call: &str, args: &[&str]) -> Result<String, ForgeError> {
        let mut owned = Vec::with_capacity(args.len() + 1);
        owned.push(self.gh.clone());
        for a in args {
            owned.push((*a).to_string());
        }
        match self.runner.run(&owned) {
            Ok(out) => Ok(out),
            Err(why) => Err(ForgeError::new(call, why)),
        }
    }

    /// The issue's label events, whole: the REST list with `--paginate`, so an
    /// issue with more events than one page holds is read to the end rather
    /// than to its first hundred.
    fn events(&mut self, issue: u64) -> Result<Vec<gh::Event>, ForgeError> {
        let path = format!("repos/{}/issues/{issue}/events?per_page=100", self.repo);
        let out = self.call("the issue's label events", &["api", &path, "--paginate"])?;
        gh::parse_events(&out).map_err(|why| ForgeError::new("the issue's label events", why))
    }
}

impl Forge for GitHub {
    fn labels(&mut self, issue: u64) -> Result<Vec<String>, ForgeError> {
        let path = format!("repos/{}/issues/{issue}", self.repo);
        let out = self.call("the issue", &["api", &path])?;
        // The answer is read as an issue with labels or not at all: an answer
        // that is no issue is not an issue with no labels.
        gh::parse_labels(&out).ok_or_else(|| ForgeError::new("the issue", ": no labels to read"))
    }

    fn label_state(&mut self, issue: u64, label: &str) -> Result<Option<LabelEvent>, ForgeError> {
        Ok(gh::latest_event(&self.events(issue)?, label))
    }

    fn approval(&mut self, issue: u64) -> Result<(String, String), ForgeError> {
        // The applications alone, as `tehas_label_actor` reads them: the
        // newest event of the label is not the newest application of it, and
        // a removal would hand the caller the remover's account to compare
        // with the allowed list.
        Ok(gh::latest_applied(&self.events(issue)?, APPROVAL_LABEL)
            .map(|e| (e.actor, e.at))
            .unwrap_or_default())
    }

    fn add_label(&mut self, issue: u64, label: &str) -> Result<(), ForgeError> {
        let path = format!("repos/{}/issues/{issue}/labels", self.repo);
        // The REST list endpoint, not `gh issue edit`, which asks GitHub a
        // GraphQL question a bot's token may not answer (issue #120): a mark
        // made that way fails on every issue. Adding a label GitHub already
        // has is not an error, so this is safe to ask twice.
        self.call(
            "the label change",
            &[
                "api",
                "-X",
                "POST",
                &path,
                "-f",
                &format!("labels[]={label}"),
            ],
        )?;
        Ok(())
    }

    fn remove_label(&mut self, issue: u64, label: &str) -> Result<(), ForgeError> {
        let path = format!("repos/{}/issues/{issue}/labels/{label}", self.repo);
        // A label that is not on the issue answers 404, which is the state
        // wanted: as the poll's refresh_unmark reads that answer, it is
        // success. Any other failure is the caller's to log.
        match self.call("the label change", &["api", "-X", "DELETE", &path]) {
            Ok(_) => Ok(()),
            Err(e) if gh::says_not_found(&e.why) => Ok(()),
            Err(e) => Err(e),
        }
    }
}

/// The real runner: gh with its standard input closed, and its answer or its
/// first error words. The caller that runs gh for real gives this a bound of
/// its own.
impl GhRunner for std::process::Command {
    fn run(&mut self, args: &[String]) -> Result<String, String> {
        let out = self
            .args(args)
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|e| format!(": {}: {e}", self.get_program().to_string_lossy()))?;
        if out.status.success() {
            return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
        }
        Err(gh::why(
            &String::from_utf8_lossy(&out.stderr),
            out.status.code(),
        ))
    }
}

#[cfg(test)]
mod tests;
