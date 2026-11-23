//! The forge: what tehas asks of the site its issues live on, in tehas terms.
//!
//! tehas keeps an issue's state in labels and decides from the account that
//! put a label on (AGENTS.md rule 5). The trait here says what tehas wants to
//! know, in its own terms: an issue's labels, the newest event of one label,
//! who applied a label, and putting a label on or taking it off. Whether a
//! term is backed by a label or by a native field is the adapter's choice and
//! never a caller's. Whether a label is on the issue now is no term of its
//! own, because a caller that asks for one and not the other is how a stale
//! read decides something.
//!
//! The GitHub adapter that answers the trait is `GitHub` in the `tehas-github`
//! crate (`console/github/`). What stays here, until the ports replace the
//! trait and the last caller of the runner moves, is the trait with its
//! values, [`GhRunner`] and [`Bounded`], the runner a host uses, the
//! reading of a failed call's reason that the runner makes, and the rule a
//! label name keeps, [`is_label`], while `console/src/labels.rs` reads it.
//!
//! Errors are values. A call that fails names the call and gh's reason for
//! it, made from a fixed list of error words, so a caller can log why it
//! stopped without holding gh's whole answer, which may name a host or carry a
//! token. Nothing here panics.

/// The characters a GitHub label name is built from in tehas. The `:` of
/// tehas's labels is one of them; a space, a `/` and a `?` are not.
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

    /// Every label event of the issue, for a rule of the caller's to read, as
    /// [`crate::events::pause_holds`] reads them.
    fn label_events(&mut self, issue: u64) -> Result<Vec<crate::events::Event>, ForgeError>;

    /// The newest application of `label`, or None when no event applied it.
    /// Only an application answers it: a label taken off does not, whoever
    /// took it off, because authorisation is the account that applied the
    /// label and a removal would hand the caller the remover to compare with
    /// the allowed list.
    fn applied_by(&mut self, issue: u64, label: &str) -> Result<Option<Approval>, ForgeError>;

    /// Who applied the label that approves work most recently, and when, for
    /// the caller to compare with the allowed list. None when nothing applied
    /// it: the same answer as [`Forge::applied_by`] of that label, which the
    /// adapter names.
    fn approval(&mut self, issue: u64) -> Result<Option<Approval>, ForgeError>;

    /// The names of the issue types an issue of the repository can be given.
    /// The empty list is an owner with none, as a personal account is.
    fn issue_types(&mut self) -> Result<Vec<String>, ForgeError>;

    /// Who gave the issue the type `name` it holds now, and when, for the
    /// caller to compare with the allowed list, as [`Forge::applied_by`]
    /// answers of a label. None when it holds another type or none. An empty
    /// `actor` when the newest change of its type is not the one that set
    /// `name`, or names nobody.
    fn typed_by(&mut self, issue: u64, name: &str) -> Result<Option<Approval>, ForgeError>;

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
/// a token or a network: a test hands the adapter a stub that answers from
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

/// Given to every gh call of tehas's own, by `Bounded` and by `call`'s
/// runners: the host's shim then sends a rate-limited write once, so a
/// call's bound is not spent in the shim's minute of waiting. An operator's
/// or a plugin's gh keeps the retry.
pub const GH_NO_RETRY: (&str, &str) = ("TEHAS_GH_RETRY", "0");

/// A program is gh by its file name: `gh` on the PATH, or a shim's path.
pub fn is_gh(program: &str) -> bool {
    std::path::Path::new(program).file_name() == Some("gh".as_ref())
}

/// The runner a host uses: gh of its own, in a bound of its own. One call per
/// [`GhRunner::run`], so nothing one call set is seen by the next. The call
/// has the shim's retry off ([`GH_NO_RETRY`]).
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
            .env(GH_NO_RETRY.0, GH_NO_RETRY.1)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()
            .map_err(|e| format!("{} could not be run: {e}", self.program))?;
        // Both pipes are drained on their own thread: a child that fills one
        // while the other is still being read would never get to its end.
        let out = child.stdout.take().expect("piped");
        // None when the stream could not be read to its end: an empty answer
        // is then no answer of gh's.
        let drain = |mut s: Box<dyn Read + Send>| {
            std::thread::spawn(move || {
                let mut text = Vec::new();
                s.read_to_end(&mut text).ok()?;
                Some(String::from_utf8_lossy(&text).into_owned())
            })
        };
        let reading = drain(Box::new(out));
        let reading_err = drain(Box::new(child.stderr.take().expect("piped")));

        let code = wait(&mut child, self.bound, self.grace);
        let out = reading.join().ok().flatten();
        let err = reading_err.join().ok().flatten().unwrap_or_default();
        match code {
            Some(0) => out.ok_or_else(|| format!("{}'s answer could not be read", self.program)),
            // Outlived its bound: a call that says nothing of itself still
            // says what stopped it.
            None => Err(format!(
                "{} ran out of its {}s bound",
                self.program,
                self.bound.as_secs()
            )),
            Some(c) => Err(why(&err, Some(c))),
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

/// Whether a failed removal answers that the label was not there, which is the
/// state wanted. The status is read whole out of the digits `find_http`
/// located, never as "404" anywhere in the text: a gateway error whose body
/// happens to carry those three digits is no removal. gh's own words
/// `Not Found` count only as the whole reason, never inside one that says
/// something else: a gh that could not start, or a proxy whose host was not
/// found, left the label where it was.
pub fn says_not_found(why: &str) -> bool {
    let w = why.to_ascii_lowercase();
    if w.strip_prefix(": ").unwrap_or(&w).trim() == "not found" {
        return true;
    }
    find_http(&w).is_some_and(|i| &w.as_bytes()[i + 5..i + 8] == b"404")
}

/// What a failed gh call's answer says of its error: ": " and the first of a
/// fixed list of words in what gh printed, else the exit status. The list is
/// the one the bash dispatcher's `gh_json` grepped and the dispatcher's
/// `gh_why` holds, so a caller logs the reason the poll logs. Only these words, or a
/// status: the rest of gh's answer is written by a host and may name one, or
/// carry a token.
pub fn why(err: &str, code: Option<i32>) -> String {
    // Longest first where two could match the same word, so a secondary limit
    // is not reported as a plain one where they start alike.
    const WORDS: [&str; 8] = [
        "secondary rate limit",
        "unexpected EOF",
        "connection reset",
        "connection refused",
        "rate limit",
        "timed out",
        "timeout",
        "no such host",
    ];
    for line in err.lines() {
        let low = line.to_ascii_lowercase();
        // The first line that says anything is the one quoted, and within it
        // the LEFTMOST word of the list, as grep -oE and the dispatcher's
        // regex answer: the first entry of the list would report "rate limit"
        // for a line whose only match is a secondary limit later on. Only
        // these words, so the rest of gh's answer - a host's address, a token
        // - stays out. The text is cut from `low`, the string that was
        // searched, so the byte offsets of `find` are its own.
        if let Some((i, w)) = WORDS
            .iter()
            .filter_map(|w| low.find(w).map(|i| (i, *w)))
            .min_by_key(|(i, w)| (*i, std::cmp::Reverse(w.len())))
        {
            return format!(": {}", &low[i..i + w.len()]);
        }
        // An HTTP status is gh's own word for the state and is kept whole.
        if let Some(i) = find_http(&low) {
            return format!(": HTTP {}", &low[i + 5..i + 8]);
        }
    }
    match code {
        Some(c) => format!(": gh exited {c}"),
        None => ": gh was stopped by a signal".to_string(),
    }
}

/// Where a lower-case line names an HTTP status: "http " and the digits after
/// it. The digits are checked so that a line that only holds the word is not
/// read as a status.
fn find_http(line: &str) -> Option<usize> {
    let b = line.as_bytes();
    let head = b"http ";
    let mut i = 0;
    while i + head.len() + 3 <= b.len() {
        if b[i..i + head.len()] == head[..] && b[i + 5..i + 8].iter().all(u8::is_ascii_digit) {
            return Some(i);
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_gh_call_of_call_turns_the_shims_retry_off_and_another_program_is_left_alone() {
        use crate::call::{run, run_grace_env, GRACE};
        let d = crate::testdir::TestDir::new("gh-no-retry");
        let says = "#!/bin/sh\necho \"${TEHAS_GH_RETRY-unset}\"\n";
        let (gh, other) = (d.join("gh"), d.join("not-gh"));
        crate::testing::write_exe(&gh, says).unwrap();
        crate::testing::write_exe(&other, says).unwrap();
        let bound = std::time::Duration::from_secs(10);
        assert_eq!(run(gh.to_str().unwrap(), &[], bound).out, "0\n");
        assert_eq!(run(other.to_str().unwrap(), &[], bound).out, "unset\n");
        let kept = d.join("out");
        let f = std::fs::File::create(&kept).unwrap();
        let rc = run_grace_env(gh.to_str().unwrap(), &[], Some(f), bound, GRACE, &[]);
        assert_eq!(rc, Some(0));
        assert_eq!(std::fs::read_to_string(&kept).unwrap(), "0\n");
    }
}
