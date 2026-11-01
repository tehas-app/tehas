//! The forge: what tehas asks of the site its issues live on, in tehas terms.
//!
//! tehas keeps an issue's state in labels and decides from the account that
//! put a label on (AGENTS.md rule 5). Today about twenty scripts ask GitHub
//! for those things themselves, each in its own shape, so the forge is welded
//! in. This module is the first half of the seam: one trait that says what
//! tehas wants to know, and one adapter that answers it from GitHub's labels
//! the way `lib/labels.sh` and `libexec/dispatch` do today. Nothing calls it
//! yet; the next issues of #800 give bash a command for it and move
//! `libexec/status` and `lib/labels.sh` onto that command.
//!
//! The terms are tehas's, not GitHub's: an issue's labels, the newest event of
//! one label, who applied the label that lets a cycle start, and putting a
//! label on or taking it off. Whether a term is backed by a label or, later,
//! by a native field, is the adapter's choice and never a caller's: a caller
//! that named a GitHub field would still be writing GitHub. Whether a label is
//! on the issue now is no term of its own, because a caller that asks for one
//! and not the other is how a stale read decides something.
//!
//! Errors are values. A call that fails names the call and gh's reason for
//! it, made from a fixed list of error words, so a caller can log why it
//! stopped without holding gh's whole answer, which may name a host or carry a
//! token. Nothing here panics.
//!
//! The adapter sits with the rules because the rules are only true of GitHub
//! when read through the calls that adapter makes: the 404 that means a label
//! was not there, the `--paginate` read that is the whole of an issue's
//! events, the REST endpoints whose argv the poll's write log and the action
//! journal of #800 record. Split the two and the copy of the rule and the copy
//! of the call drift apart, which is what the module exists to stop. It is a
//! library with no caller yet, and it holds no token: [`Bounded`] is the only
//! thing here that starts a process, and it is what the next issues of #800
//! build once, with the instance's shim as its program.
//!
//! The reading of gh's answers, and the rules of `lib/labels.sh` that decide
//! which of an issue's events answer a term, are in [`gh`].

mod gh;

/// The label whose latest application approves work. Named once in this
/// module: the same string is still spelled out in `console/dispatch`,
/// `console/cycle` and `libexec/labels`, and the issue that consolidates
/// tehas's label names (#810) is where the other copies are gathered.
const APPROVAL_LABEL: &str = "tehas:ready";

/// The characters a GitHub label name is built from in tehas. The `:` of
/// `tehas:ready` is one of them; a space, a `/` and a `?` are not.
const LABEL_BYTES: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789:-_";

/// The longest label name GitHub accepts.
const LABEL_MAX: usize = 50;

/// Whether `label` is a name the writes will accept: non-empty, no longer than
/// GitHub allows, and built from the characters in [`LABEL_BYTES`]. A write
/// refuses a name outside it, because such a name is no label of tehas and the
/// request that carries it is aimed at something else - a newline, or a `-f`,
/// would otherwise reach gh as part of the one argument it is given. A read of
/// a term takes any string, so a repository's own label can still be asked
/// about. The path segment of a removal is encoded as well, so a name that did
/// reach a write could not leave the labels of its issue.
pub fn is_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= LABEL_MAX
        && label.as_bytes().iter().all(|b| LABEL_BYTES.contains(b))
}

/// `s` as jq's `@uri` writes it, so a value cannot carry a path or a query of
/// its own past the segment it sits in. Applied to the one part of a path the
/// site's own answer fills: the label of a removal. The dispatcher's
/// `read::build::uri` holds the same function, for the branch of a query.
fn uri(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// A term the forge could not answer, as a value rather than a panic.
///
/// `call` names the term that stopped, so one line says what was attempted,
/// and `why` is gh's reason, which the runner made from its fixed list of
/// error words and never from the whole of its answer: a caller does not hold
/// what may name a host or carry a token. An error is never an empty answer,
/// and an empty answer is never an error.
#[derive(Clone, Debug)]
pub struct ForgeError {
    pub call: String,
    /// The reason, without its `": "`: the line adds that. Empty when the
    /// call gave none.
    pub why: String,
}

impl ForgeError {
    /// The error of one call, with its reason, which may be empty. A reason is
    /// what a runner said, not gh's own words: only the fixed list of them
    /// belongs in a log line.
    pub fn new(call: &str, why: impl Into<String>) -> ForgeError {
        ForgeError {
            call: call.to_string(),
            why: why.into(),
        }
    }

    /// The line a caller logs, shaped as the poll's lines are: what failed,
    /// and gh's own words when it had any. The separator is made here, so a
    /// reason never has to carry it.
    pub fn line(&self) -> String {
        if self.why.is_empty() {
            format!("ERROR: {} failed", self.call)
        } else {
            format!("ERROR: {} failed: {}", self.call, self.why)
        }
    }
}

impl std::fmt::Display for ForgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.line())
    }
}

impl std::error::Error for ForgeError {}

/// Who applied a label, and when, as the approval term answers. `None` is the
/// answer of an issue nothing applied; an empty `actor` inside `Some` is a fact
/// and not a refusal, the event's account being gone. The caller compares the
/// account with the allowed list, which is what keeps authorisation where the
/// site put it (AGENTS.md rule 5): a [`Forge`] answers who, and never whether
/// that account may start anything.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Approval {
    /// The account that applied the label, empty when the event names none.
    pub actor: String,
    /// The application's stamp, empty when it has none.
    pub at: String,
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
/// says so, and the caller decides what a missing answer is worth. An empty
/// value is the site's answer and not a read that failed: the labels of an
/// issue that carries none are the empty list, and an answer that could not
/// be read is an error, which is what keeps a poll from clearing a label
/// nobody took off.
///
/// Every term makes its own call. A caller that wants more than one answer of
/// one issue holds what it read itself, as the poll caches a label's events:
/// the adapter keeps nothing between terms.
pub trait Forge {
    /// The labels the issue carries, in the order the site gave them. The
    /// empty list is the site's answer that it carries none; a read that could
    /// not be read is an error.
    fn labels(&mut self, issue: u64) -> Result<Vec<String>, ForgeError>;

    /// The newest `labeled` or `unlabeled` event of `label`, or None when no
    /// event of the issue carries it.
    fn label_state(&mut self, issue: u64, label: &str) -> Result<Option<LabelEvent>, ForgeError>;

    /// The newest application of `label`, or None when no event applied it.
    /// Only an application answers it: a label taken off does not, whoever
    /// took it off, because authorisation is the account that applied the
    /// label and a removal would hand the caller the remover to compare with
    /// the allowed list.
    fn applied_by(&mut self, issue: u64, label: &str) -> Result<Option<Approval>, ForgeError>;

    /// Who applied the label that approves work most recently, and when, for
    /// the caller to compare with the allowed list. None when nothing applied
    /// it: the same answer as [`Forge::applied_by`] of [`APPROVAL_LABEL`].
    fn approval(&mut self, issue: u64) -> Result<Option<Approval>, ForgeError> {
        self.applied_by(issue, APPROVAL_LABEL)
    }

    /// Put `label` on the issue. An issue that carries it already is the
    /// state wanted, not an error. A name no label can have is refused
    /// without a call.
    fn add_label(&mut self, issue: u64, label: &str) -> Result<(), ForgeError>;

    /// Take `label` off the issue. An issue that does not carry it is already
    /// in the state wanted, not an error. A name no label can have is refused
    /// without a call.
    fn remove_label(&mut self, issue: u64, label: &str) -> Result<(), ForgeError>;
}

/// gh, behind a trait of its own, so the adapter's rules are read without gh,
/// a token or a network: a test hands [`GitHub`] a stub that answers from
/// fixtures, and what it decides is the rules alone.
///
/// A runner runs one gh call and owns how: it holds the program, gives the
/// call its bound, and reads its standard output. `args` are the words after
/// the program, which the runner does not repeat, and the reason an adapter
/// holds is the runner's own for the failure, not what gh printed.
pub trait GhRunner {
    /// Run gh with these arguments: its standard output, or the reason it
    /// gave for failing.
    fn run(&mut self, args: &[&str]) -> Result<String, String>;
}

/// The GitHub adapter: every term backed by labels, as the bash backs them
/// today. It holds the repository it asks and the runner that asks GitHub, and
/// reads nothing else.
///
/// The ordering of the events that answers [`Forge::label_state`] and
/// [`Forge::approval`] is the rules of [`gh`].
pub struct GitHub {
    /// `owner/name`, as `TEHAS_REPO` gives it.
    pub repo: String,
    runner: Box<dyn GhRunner>,
}

impl GitHub {
    /// The adapter of one repository, asking GitHub through `runner`. The
    /// runner is the one that runs gh: on a host, [`Bounded`], built with the
    /// path of the instance's `host/shim/gh`, which takes the bot's token from
    /// `TEHAS_GH_TOKEN`. Where the program, its token and the bound come from
    /// is the caller's setting to read; the next issues of #800 say which
    /// command owns it.
    pub fn new(repo: impl Into<String>, runner: Box<dyn GhRunner>) -> GitHub {
        GitHub {
            repo: repo.into(),
            runner,
        }
    }

    /// Ask gh, keeping the reason a failed call gave. `args` are the words
    /// after gh; `call` is what a [`ForgeError`] names.
    fn call(&mut self, call: &str, args: &[&str]) -> Result<String, ForgeError> {
        match self.runner.run(args) {
            Ok(out) => Ok(out),
            Err(why) => Err(ForgeError::new(call, why)),
        }
    }

    /// The reason of a read that is no answer, named after the read the
    /// adapter was making: the reader's words, kept whole.
    fn reason_of(why: String) -> ForgeError {
        ForgeError::new("the issue's label events", why)
    }

    /// The issue's label events, whole: the REST list with `--paginate`, so an
    /// issue with more events than one page holds is read to the end rather
    /// than to its first hundred.
    fn events(&mut self, issue: u64) -> Result<Vec<gh::Event>, ForgeError> {
        let path = format!("repos/{}/issues/{issue}/events?per_page=100", self.repo);
        let out = self.call("the issue's label events", &["api", &path, "--paginate"])?;
        gh::parse_events(&out).map_err(Self::reason_of)
    }
}

impl Forge for GitHub {
    fn labels(&mut self, issue: u64) -> Result<Vec<String>, ForgeError> {
        let path = format!("repos/{}/issues/{issue}", self.repo);
        let out = self.call("the issue", &["api", &path])?;
        // The answer is read as an issue with labels or not at all: an answer
        // that is no issue is not an issue with no labels.
        gh::parse_labels(&out).ok_or_else(|| ForgeError::new("the issue", "no labels to read"))
    }

    fn label_state(&mut self, issue: u64, label: &str) -> Result<Option<LabelEvent>, ForgeError> {
        Ok(gh::latest_event(&self.events(issue)?, label))
    }

    fn applied_by(&mut self, issue: u64, label: &str) -> Result<Option<Approval>, ForgeError> {
        // The applications alone, as `tehas_label_actor` reads them: the
        // newest event of the label is not the newest application of it, and
        // a removal would hand the caller the remover's account to compare
        // with the allowed list.
        Ok(
            gh::latest_applied(&self.events(issue)?, label).map(|e| Approval {
                actor: e.actor,
                at: e.at,
            }),
        )
    }

    fn add_label(&mut self, issue: u64, label: &str) -> Result<(), ForgeError> {
        if !is_label(label) {
            return Err(ForgeError::new(
                "the label change",
                format!("{label} is no label name to write: an empty one, a long one, or one outside the characters tehas's labels use"),
            ));
        }
        let path = format!("repos/{}/issues/{issue}/labels", self.repo);
        // The label goes as a field of the request, as the bash and the
        // dispatcher's writer send it, and not into a path: it keeps the
        // characters of the site's answer. The REST list endpoint, not
        // `gh issue edit`, which asks GitHub a
        // GraphQL question a bot's token may not answer (issue #120): a mark
        // made that way fails on every issue. Adding a label GitHub already
        // has is not an error, so this is safe to ask twice. The whole call is
        // one argv, so the words here are the words the poll's write log
        // records, and the action journal of #800 records the same call.
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
        if !is_label(label) {
            return Err(ForgeError::new(
                "the label change",
                format!("{label} is no label name to write: an empty one, a long one, or one outside the characters tehas's labels use"),
            ));
        }
        // The name is encoded, so a character outside the tehas vocabulary -
        // a `:`, a space - stays inside the last segment of the path and moves
        // the DELETE nowhere else. The repository is the settings' own
        // `owner/name`, as every reader of it passes it to this path.
        let path = format!("repos/{}/issues/{issue}/labels/{}", self.repo, uri(label));
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

/// The runner a host uses: gh of its own, in a bound of its own. One call per
/// [`GhRunner::run`], so nothing one call set is seen by the next.
///
/// The child leads a process group and the group is signalled, as `timeout -k`
/// does: TERM first, KILL after the grace, so gh and what it started are
/// gone. A poll holds `state/dispatch.lock` for its whole read, so a gh that
/// never returns would hold the lock for as long as it lives, which is why the
/// bound belongs here and not in the caller's head. The dispatcher's
/// `read::run::run_input_env` is the same bound and the same signals; it is
/// `tehas-dispatch`'s, so the wait is held here where the forge is, and the two
/// go into one with the callers of #800.
pub struct Bounded {
    /// The program to run. A host runs its shim, `host/shim/gh`, which takes
    /// the token from `TEHAS_GH_TOKEN` when the caller holds one.
    pub program: String,
    /// How long one call may take before it is stopped.
    pub bound: std::time::Duration,
    /// The grace between TERM and KILL, as `timeout -k` gives.
    pub grace: std::time::Duration,
}

impl Bounded {
    /// The runner of one program under one bound, with the 5-second grace
    /// `timeout -k` uses by default. A poll gives a call of its own gh the
    /// same bound the dispatcher's reads use: thirty seconds.
    pub fn new(program: impl Into<String>, bound: std::time::Duration) -> Bounded {
        Bounded {
            program: program.into(),
            bound,
            grace: std::time::Duration::from_secs(5),
        }
    }
}

impl GhRunner for Bounded {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        use std::io::Read;
        use std::os::unix::process::CommandExt;
        use std::process::{Command, Stdio};

        let mut child = Command::new(&self.program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()
            .map_err(|e| format!("{} could not be run: {e}", self.program))?;
        // Both pipes are drained on their own thread: a child that fills one
        // while the other is still being read would never get to its end.
        let mut out = child.stdout.take().expect("piped");
        let reading = std::thread::spawn(move || {
            let mut text = String::new();
            let _ = out.read_to_string(&mut text);
            text
        });
        let mut err = child.stderr.take().expect("piped");
        let reading_err = std::thread::spawn(move || {
            let mut text = String::new();
            let _ = err.read_to_string(&mut text);
            text
        });

        let code = wait(&mut child, self.bound, self.grace);
        let out = reading.join().unwrap_or_default();
        let err = reading_err.join().unwrap_or_default();
        match code {
            Some(0) => Ok(out),
            // Outlived its bound: a call that says nothing of itself still
            // says what stopped it.
            None => Err(format!(
                "{} ran out of its {}s bound",
                self.program,
                self.bound.as_secs()
            )),
            Some(c) => Err(gh::why(&err, Some(c))),
        }
    }
}

/// The child's exit status, the signal as 128 plus its number, and None when
/// it outlived its bound and was stopped. The child leads a group of its own,
/// so the signals go to it and to what it started, as `timeout -k` sends them.
fn wait(
    child: &mut std::process::Child,
    bound: std::time::Duration,
    grace: std::time::Duration,
) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    use std::time::{Duration, Instant};

    let group = child.id().to_string();
    let began = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(s)) => return s.code().or_else(|| s.signal().map(|g| 128 + g)),
            Ok(None) if began.elapsed() < bound => std::thread::sleep(Duration::from_millis(20)),
            _ => break,
        }
    }
    signal("-TERM", &group);
    let term = Instant::now();
    while term.elapsed() < grace {
        if let Ok(Some(_)) = child.try_wait() {
            // Stopped by TERM: timeout answers 124.
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    signal("-KILL", &group);
    let _ = child.kill();
    let _ = child.wait();
    // One that had to be killed: timeout -k answers 137.
    Some(137)
}

/// One signal to the group, in a call that can neither print nor block.
fn signal(which: &str, group: &str) {
    use std::process::Stdio;

    let _ = std::process::Command::new("kill")
        .args([which, "--", group])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(test)]
mod tests;
