//! The forge as a command: what `libexec/status` and
//! `console label` ask of GitHub, asked through `tehas_core::forge`, so that
//! bash holds no gh call, no REST path and no rule of its own (#800 part 2).
//!
//!   console forge labels <issue>
//!     The labels the issue carries, one per line. It is the site's list, so a
//!     name it carries twice appears twice; an issue that carries none prints
//!     nothing and exits 0.
//!   console forge label-state <issue> <label>
//!     `<kind>\t<account>\t<time>` of the newest `labeled` or `unlabeled` event
//!     of the label. Account and time are empty when the event names none.
//!     For a label no event carries this prints nothing at all.
//!     Authorisation stays the caller's (AGENTS.md rule 5): this says who,
//!     never whether that account may start anything.
//!   console forge approval <issue>
//!     `<account>\t<time>` of the newest application of `tehas:ready`, the pair
//!     `tehas_label_actor` prints, or nothing when no event applied it. A label
//!     taken off answers no approval, whoever took it off. The word is the
//!     forge's own: a caller cannot ask whether a label that approves nothing
//!     approved the work.
//!   console forge add-label <issue> <label>
//!   console forge remove-label <issue> <label>
//!     Put the label on, take it off. An issue that carries the label already
//!     is the state wanted, and one that does not carry a label taken off is
//!     already in it, so either verb may be asked twice. A name no label of
//!     tehas can have is a bad argument and never reaches gh. For these two the
//!     exit status and not the empty output is the thing to branch on: a label
//!     put on is on the issue whether or not this run was heard before it ended.
//!
//! `<issue>` is digits and nothing else: a number that is not one is no
//! question, and 64 stops it before anything is asked.
//!
//! The repository is `TEHAS_REPO`, `owner/name`, as `lib/config.sh` exports it,
//! and the call names no repository of its own, so a poll has no way to write
//! where its instance did not choose. The program is the instance's
//! `$TEHAS_HOME/shim/gh` when it is there, so the write is the shim's write,
//! with the bot's token and the line in `state/github-writes.log`; with no
//! shim it is gh found on `PATH`, where `host/env.sh` puts that shim first
//! anyway. The command holds no token of its own: the shim takes one, or the
//! caller of a whole poll holds `TEHAS_GH_TOKEN` for every call of it (#938).
//! One call runs under a bound of its own, TERM then KILL, as `timeout -k`
//! does: the bound covers the child, and a further 5 seconds of grace before
//! the kill, so a poll holding its lock budgets 35 seconds for a gh that never
//! returned.
//!
//! Exit: 0 the call answered, and what was asked of it is printed; 1 gh failed,
//! with one line on standard error naming the call and gh's reason, which holds
//! no host and no token; 64 a bad argument, before anything is asked; 78 the
//! instance cannot be reached: `TEHAS_REPO` is unset or not `owner/name`, or no
//! gh can be run, either way before the first gh call; 1 as well when the
//! answer itself could not be written.

use std::env;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;
#[cfg(test)]
use tehas_core::forge::GhRunner;
use tehas_core::forge::{is_label, Approval, Bounded, Forge, ForgeError, GitHub, LabelEvent};

/// The longest repository path part this command accepts. Its own bound and not
/// `lib/config.sh`'s rule: that checks the characters of `owner/name` and bounds
/// no length, and the label ceiling of the adapter is no rule about a
/// repository. A longer setting is a setting that cannot name one.
const REPO_PART_MAX: usize = 100;

/// The largest issue number a call may be asked about, the ceiling
/// `console shipped answer` holds: past it two spellings name the same issue,
/// which no caller may be allowed to ask about. `console store` and analytics
/// keep their numbers to nine digits, well inside it.
const ISSUE_MAX: u64 = 9_007_199_254_740_992;

/// The bound of one call of gh: the thirty seconds the dispatcher's reads give
/// it, so the two readers of one issue wait the same time for an answer. The
/// same number is in `console/dispatch/src/read/run.rs`; both are 30 and
/// neither reads the other's.
pub(crate) const CALL_BOUND: Duration = Duration::from_secs(30);

/// What one verb asks of the forge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Term {
    Labels,
    State,
    Approval,
    Add,
    Remove,
}

/// The five verbs: the word a caller types, the term it asks, the words it takes
/// after the verb, and the words of the run when it took one too few or one too
/// many. One table, so the usage line a caller is shown and the verbs the command
/// has cannot drift apart. The approval names no label, because the label that
/// lets a cycle start is the forge's.
const VERBS: [(&str, Term, usize, &str); 5] = [
    ("labels", Term::Labels, 1, "one issue number"),
    (
        "label-state",
        Term::State,
        2,
        "one issue number and one label",
    ),
    ("approval", Term::Approval, 1, "one issue number"),
    ("add-label", Term::Add, 2, "one issue number and one label"),
    (
        "remove-label",
        Term::Remove,
        2,
        "one issue number and one label",
    ),
];

/// The words of one run, and the term they ask: the issue as the caller wrote
/// it, the number `run` parses from it once, and the label of the verbs that
/// take one. A term with a word missing and a term with one too many are the
/// same to a caller: it did not say what it wanted.
struct Asking<'a> {
    verb: &'static str,
    /// The issue word as it was given, for the line that refuses it.
    word: &'a str,
    /// The number of the call: zero until `run` has parsed the word, so no
    /// term is asked of the site before its issue is a number.
    issue: u64,
    term: Term,
    label: Option<&'a str>,
}

/// The words a caller types, and the term they ask: None when they ask none,
/// which is usage.
fn asking(given: &[String]) -> Option<Asking<'_>> {
    let (verb, term, words, _) = VERBS
        .iter()
        .find(|(name, _, _, _)| given.first().is_some_and(|a| a == name))?;
    let rest = &given[1..];
    if rest.len() != *words {
        return None;
    }
    Some(Asking {
        verb,
        word: rest[0].as_str(),
        issue: 0,
        term: *term,
        label: (*words == 2).then(|| rest[1].as_str()),
    })
}

/// The words of the five verbs after one word of the command, so a usage line
/// cannot go stale when a sixth verb is added.
fn shapes() -> String {
    VERBS
        .iter()
        .map(|(name, _, words, _)| {
            let label = if *words == 2 { " <label>" } else { "" };
            format!("{name} <issue>{label}")
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

/// The line of a run that stopped before it asked anything: the prefix the
/// poll's log reads, and never a line on standard output, which a caller reads
/// as an answer.
fn stop(rc: u8, line: String) -> (u8, String, String) {
    (rc, String::new(), format!("console forge: {line}\n"))
}

/// The words a caller is shown when its arguments ask no term: which word was
/// left out, named from the same table as the shapes below it, and then those
/// shapes, so one line of a log is enough to fix the call.
fn usage(given: &[String]) -> String {
    let why = match given.first().map(String::as_str) {
        None => "what is wanted of the forge".to_string(),
        Some(verb) => match VERBS.iter().find(|(name, _, _, _)| *name == verb) {
            Some((_, _, _, words)) => (*words).to_string(),
            None => VERBS
                .iter()
                .map(|(name, _, _, _)| *name)
                .collect::<Vec<_>>()
                .join(", "),
        },
    };
    format!("console forge: {why}\nusage: console forge {}\n", shapes())
}

/// Whether `<issue>` is a number: digits and nothing else, non-empty. Anything
/// else is no question and never reaches the site, which is also what keeps
/// `repos/<repo>/issues/<issue>` to one path segment.
fn digits(issue: &str) -> bool {
    !issue.is_empty() && issue.bytes().all(|b| b.is_ascii_digit())
}

/// The issue of a call: digits, one number and not several spellings of one
/// (a leading zero names no issue), non-zero, and no bigger than a read answers
/// as one number.
fn issue_of(word: &str) -> Option<u64> {
    if !digits(word) || (word.len() > 1 && word.starts_with('0')) {
        return None;
    }
    let asked = word.parse::<u64>().ok()?;
    (asked > 0 && asked <= ISSUE_MAX).then_some(asked)
}

/// Whether a repository is `owner/name`: the shape `lib/config.sh` checks the
/// setting for, under this command's own ceiling for a part. A setting that is
/// no repository is no repository to ask, which is the 78 of a run that cannot
/// reach GitHub and never a request aimed at something else.
pub(crate) fn repo_shaped(repo: &str) -> bool {
    let part = |p: &str| {
        !p.is_empty()
            && p.len() <= REPO_PART_MAX
            && p.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    };
    let Some((owner, name)) = repo.split_once('/') else {
        return false;
    };
    // One slash and no more: a third part is no `owner/name`.
    !name.contains('/') && part(owner) && part(name)
}

/// `TEHAS_REPO`, trimmed: the repository of the instance, or None when the name
/// is unset, empty, or nothing but space. Nothing here asks GitHub, runs a
/// program or writes a file.
pub(crate) fn repo() -> Option<String> {
    match env::var("TEHAS_REPO") {
        Ok(v) => {
            let v = v.trim();
            (!v.is_empty()).then(|| v.to_string())
        }
        Err(_) => None,
    }
}

/// Whether `file` is something that may be run: a regular file with a bit for
/// someone to execute it. The same test `clean`'s `which` and the dispatcher's
/// readers make of a program.
pub(crate) fn runnable(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Where `program` may be run, as `command -v` answers: the first directory of
/// `path` that holds it, or None.
fn which(program: &str, path: &str) -> Option<String> {
    path.split(':').find_map(|dir| {
        let p = Path::new(dir).join(program);
        runnable(&p).then(|| p.display().to_string())
    })
}

/// The program of a run: the instance's shim when it is installed, and gh of
/// `PATH` otherwise. The shim is named rather than inherited from an ambient
/// `PATH`, because it is the program that takes the bot's token and keeps the
/// write log the poll's cost record reads (#649): a run that reached GitHub
/// past it would be unmeasured and still exit 0.
pub(crate) fn gh_program(home: Option<&str>, path: &str) -> Option<String> {
    if let Some(home) = home.filter(|h| !h.is_empty()) {
        let shim = Path::new(home).join("shim").join("gh");
        if runnable(&shim) {
            return Some(shim.display().to_string());
        }
    }
    which("gh", path)
}

/// What a term answered, in the shape its own term asked for. Each answer
/// carries the term it belongs to, so a reading that found nothing and a write
/// that is now in the state wanted are different answers and never one catch-all.
enum Answer {
    Names(Term, Vec<String>),
    Event(Term, LabelEvent),
    Approval(Term, Approval),
    /// Nothing of the site answers the term, or the write is done: the state is
    /// as asked, which is all the empty line says.
    Done(Term),
}

/// The line of one answer: the fields of an event with tabs between them, the
/// labels one to a line, and nothing at all for the answer of a term the site had
/// nothing to say about. A tab and not a space: a caller reads the line the way
/// `console label` writes one, with `IFS=$'\t' read`. Every arm names the pair it
/// prints, so a new term or a new answer fails to compile rather than print
/// nothing.
fn line_of(answer: &Answer) -> String {
    match answer {
        Answer::Names(Term::Labels, names) => {
            // No blank line for the empty list: a caller reads one line per label.
            if names.is_empty() {
                String::new()
            } else {
                format!("{}\n", names.join("\n"))
            }
        }
        Answer::Event(Term::State, event) => {
            format!("{}\t{}\t{}\n", event.kind, event.actor, event.at)
        }
        Answer::Approval(Term::Approval, approval) => {
            format!("{}\t{}\n", approval.actor, approval.at)
        }
        // The state read found no event, the approval found no application, and
        // either write left the issue as asked: the same empty line, for four
        // different reasons the caller does not have to tell apart.
        Answer::Done(Term::State)
        | Answer::Done(Term::Approval)
        | Answer::Done(Term::Add)
        | Answer::Done(Term::Remove) => String::new(),
        // The remaining pairs ask a term for an answer of another kind. `ask`
        // builds every answer from the term it was handed, so none of them is
        // reachable, and a new term or answer has to be named here to compile.
        _ => unreachable!("a term prints the answer of its own kind"),
    }
}

/// One verb against one forge. Taken as a function of a [`Forge`] so that a case
/// hands it a stub in place of gh, and the command and the case run the one
/// mapping from an answer to a line and a status.
fn ask(forge: &mut dyn Forge, asking: &Asking) -> Result<Answer, ForgeError> {
    let term = asking.term;
    let issue = asking.issue;
    let label = asking.label.unwrap_or_default();
    match term {
        Term::Labels => forge.labels(issue).map(|n| Answer::Names(term, n)),
        Term::State => forge
            .label_state(issue, label)
            .map(|e| event_answer(term, e)),
        Term::Approval => forge.approval(issue).map(|a| approval_answer(term, a)),
        Term::Add => forge.add_label(issue, label).map(|()| Answer::Done(term)),
        Term::Remove => forge
            .remove_label(issue, label)
            .map(|()| Answer::Done(term)),
    }
}

/// The answer of a state read: the event, or the term's own nothing-found, which
/// is an answer and never a failed call.
fn event_answer(term: Term, event: Option<LabelEvent>) -> Answer {
    match event {
        Some(event) => Answer::Event(term, event),
        None => Answer::Done(term),
    }
}

/// The answer of an approval read, in the same two shapes.
fn approval_answer(term: Term, approval: Option<Approval>) -> Answer {
    match approval {
        Some(approval) => Answer::Approval(term, approval),
        None => Answer::Done(term),
    }
}

/// One verb of one issue, from its words to its status and its two streams: the
/// whole of a run, so `main` is one call and a case sees what a caller sees.
/// Everything a run reads from outside itself is given (the setting, the
/// program, and one call of gh), so a case holds them and a host supplies its
/// own, and neither reaches into the other's.
///
/// The two 78s are made before `call` is asked anything, so a run that cannot
/// reach GitHub pays no token and none of its bound, and prints nothing rather
/// than the empty answer a caller would read as "the issue carries no labels".
fn run(
    given: &[String],
    repository: Option<&str>,
    gh: bool,
    mut call: impl FnMut(&Asking, &str) -> Result<Answer, ForgeError>,
) -> (u8, String, String) {
    // The words first, and all of them: a bad one is 64 and asks GitHub nothing,
    // so a poll spends no call, no token and none of its bound on a typo.
    let Some(mut asking) = asking(given) else {
        return stop(64, usage(given));
    };
    // Digits and nothing else, non-zero, and no bigger than a read answers as one
    // number: each says the question was not asked properly, which is 64 before
    // the site is asked. The number parsed here is the only number the call
    // carries, so the words of the argument cannot reach gh.
    let Some(number) = issue_of(asking.word) else {
        let word = echo(asking.word);
        return stop(64, format!("{}: {word} is no issue number", asking.verb));
    };
    asking.issue = number;
    // A term that names a label checks the name it was given: one with a space, a
    // newline or a slash in it is no label of tehas, and never one argument of a
    // request. `labels` and `approval` take none, and a read of a state takes any
    // name, so a repository's own label can still be asked about.
    if let Some(label) = asking.label {
        if !is_label(label) {
            let name = echo(label);
            return stop(64, format!("{}: {name} is no label name", asking.verb));
        }
    }
    // The setting, trimmed, and then its shape: the `owner/name` of a host's
    // instance.env may carry the space or a newline a file writes with, and that
    // is no reason to stop a poll. Anything else is no repository to ask. The
    // name goes to the call, so the reading is asked of the repository the
    // setting named and of no other.
    let Some(repository) = repository.map(str::trim).filter(|r| repo_shaped(r)) else {
        return stop(78, format!("{}: TEHAS_REPO is not set", asking.verb));
    };
    if !gh {
        return stop(
            78,
            format!("{}: gh could not be run: it is not in PATH", asking.verb),
        );
    }
    match call(&asking, repository) {
        // An answer prints its line, which may be the empty one: the site said
        // there is no such event, and that is an answer and never a failed call.
        Ok(answer) => (0, line_of(&answer), String::new()),
        // One line: the term that stopped and gh's own words for it, made from
        // the forge's fixed list, so nothing of an answer that may name a host or
        // carry a token is repeated here.
        Err(e) => stop(1, e.line()),
    }
}

/// A word the caller wrote, as it may be repeated on standard error: control
/// characters gone, so one bad argument cannot forge a line of a poll's log or
/// put an escape on a terminal, and short enough for that line. The words of a
/// refused argument are refused, not trusted.
fn echo(word: &str) -> String {
    let said: String = word.chars().filter(|c| !c.is_control()).collect();
    if said.chars().count() <= 120 {
        return said;
    }
    let cut = said.chars().take(120).collect::<String>();
    format!("{cut}…")
}

/// One run's words to the streams of the process: what a term answered on
/// standard output, and the line of a run that stopped on standard error. False
/// when the answer could not be written, which `main` answers with 1: a run that
/// was not heard is not a run that answered.
fn emit(out: &str, err: &str) -> bool {
    let o = std::io::stdout();
    let mut o = o.lock();
    let written = o.write_all(out.as_bytes()).and_then(|()| o.flush()).is_ok();
    if !err.is_empty() {
        let e = std::io::stderr();
        let mut e = e.lock();
        let _ = e.write_all(err.as_bytes()).and_then(|()| e.flush());
    }
    written
}

/// The one call a run makes of the forge: the adapter of the repository it was
/// asked of, of the program named at the start of the run, under the bound of
/// one call. Held across the terms of a run so that the program decision, the
/// token it brings and the bound are made once and not per verb.
fn ask_the_forge(program: String) -> impl FnMut(&Asking, &str) -> Result<Answer, ForgeError> {
    let mut forge: Option<GitHub> = None;
    move |asking, repository| {
        let held = forge.get_or_insert_with(|| {
            GitHub::new(
                repository,
                Box::new(Bounded::new(program.clone(), CALL_BOUND)),
            )
        });
        ask(held, asking)
    }
}

/// The run of the command: the setting and the program it reads from its
/// environment, then [`run`], then the two streams. Taken apart from `main` so a
/// case can hold a directory for them and leave the process's own environment
/// alone.
fn execute(given: &[String]) -> u8 {
    // The setting is read once and kept, so the name a poll's log records is the
    // name this run asked, whatever the environment holds later.
    let repository = repo();
    // The instance's shim when it is installed, so the token and the write log
    // are the shim's and not some other gh's.
    let program = gh_program(
        env::var("TEHAS_HOME").ok().as_deref(),
        &env::var("PATH").unwrap_or_default(),
    );
    let (rc, out, err) = run(
        given,
        repository.as_deref(),
        program.is_some(),
        ask_the_forge(program.unwrap_or_default()),
    );
    if emit(&out, &err) {
        rc
    } else {
        1
    }
}

pub fn main(given: &[String]) -> u8 {
    execute(given)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// The repository a plain case runs under, as a host's setting gives it.
    const REPO: &str = "acme/widget";

    /// What a run asked of the site: each call's term, issue, label and the
    /// repository it was asked of, in the order the run asked them.
    type Calls = Rc<RefCell<Vec<(Term, u64, String, String)>>>;

    /// The name the adapter gives the call of a term, so a case that makes a run
    /// fail reads the line the adapter's `GitHub` would have written: the words of
    /// the failed call are the adapter's and no place of this module invents one.
    fn call_name_of(term: Term) -> &'static str {
        match term {
            Term::Labels => "the issue",
            Term::State | Term::Approval => "the issue's label events",
            Term::Add | Term::Remove => "the label change",
        }
    }

    /// One run, and the calls it made of the site: the term, the issue, the
    /// label and the repository of each, in the order the run asked them, so a
    /// case that expects no call reads no call, and a case of a verb reads the
    /// words the call carried.
    fn one(given: &[&str], answer: Result<&str, &str>) -> Run {
        with(given, Some(REPO), true, answer)
    }

    /// One run with the setting and the program of the case's own: the two
    /// things a host's run reads from its environment, given here so a case holds
    /// them instead of reaching into the process.
    fn with(given: &[&str], repo: Option<&str>, gh: bool, answer: Result<&str, &str>) -> Run {
        let given: Vec<String> = given.iter().map(|s| s.to_string()).collect();
        let answer = answer.map(str::to_string).map_err(str::to_string);
        let calls: Calls = Rc::new(RefCell::new(Vec::new()));
        let held = Rc::clone(&calls);
        let (rc, out, err) = run(&given, repo, gh, move |asking, site| {
            held.borrow_mut().push((
                asking.term,
                asking.issue,
                asking.label.unwrap_or_default().to_string(),
                site.to_string(),
            ));
            match &answer {
                Ok(text) => Ok(answer_of(asking, text)),
                Err(why) => Err(ForgeError::new(call_name_of(asking.term), why.clone())),
            }
        });
        let taken = calls.borrow().clone();
        Run {
            rc,
            out,
            err,
            calls: taken,
        }
    }

    struct Run {
        rc: u8,
        out: String,
        err: String,
        calls: Vec<(Term, u64, String, String)>,
    }

    impl Run {
        /// The verdict and the two streams, as the caller of a poll has them.
        fn said(&self) -> (u8, &str, &str) {
            (self.rc, self.out.as_str(), self.err.as_str())
        }
    }

    /// The fixture as the term's answer: `none` is the answer of a term the site
    /// had nothing to say about, a line of tab-separated fields is the event, and
    /// the labels verb reads one name to a line. Each term builds the answer of
    /// its own kind, so no case has to invent one for a term that answers none.
    fn answer_of(asking: &Asking, text: &str) -> Answer {
        let term = asking.term;
        if text == "none" {
            // The labels verb is answered by the site's empty list, which is a
            // shape of its own; the readings and the writes answer the term's
            // nothing.
            return if term == Term::Labels {
                Answer::Names(term, Vec::new())
            } else {
                Answer::Done(term)
            };
        }
        match term {
            Term::Labels => Answer::Names(
                term,
                text.lines()
                    .filter(|l| !l.is_empty())
                    .map(str::to_string)
                    .collect(),
            ),
            Term::State => {
                let f = fields(text, 3);
                Answer::Event(
                    term,
                    LabelEvent {
                        kind: f[0].clone(),
                        actor: f[1].clone(),
                        at: f[2].clone(),
                    },
                )
            }
            Term::Approval => {
                let f = fields(text, 2);
                Answer::Approval(
                    term,
                    Approval {
                        actor: f[0].clone(),
                        at: f[1].clone(),
                    },
                )
            }
            Term::Add | Term::Remove => Answer::Done(term),
        }
    }

    /// The fields of a fixture, the missing ones filled in: an event that names no
    /// account or no time has the empty field the site answers with.
    fn fields(text: &str, n: usize) -> Vec<String> {
        let mut f: Vec<String> = text.splitn(n, '\t').map(str::to_string).collect();
        f.resize(n, String::new());
        f
    }

    #[test]
    fn a_failed_call_is_named_by_the_adapter_and_not_by_this_command() {
        // A run of a host makes its errors through the adapter, whose own words
        // name the call. `run` is given no place to invent one: the line a case
        // reads is built from the terms the adapter's `GitHub` uses, so a verb
        // cannot be shown failing at something the site was never asked.
        for (given, call) in [
            (vec!["labels", "7"], "the issue"),
            (
                vec!["label-state", "7", "tehas:ready"],
                "the issue's label events",
            ),
            (vec!["approval", "7"], "the issue's label events"),
            (vec!["add-label", "7", "tehas:ready"], "the label change"),
            (vec!["remove-label", "7", "tehas:ready"], "the label change"),
        ] {
            let asked = Rc::new(RefCell::new(Vec::new()));
            let given: Vec<String> = given.iter().map(|s| s.to_string()).collect();
            let (rc, out, err) = run(&given, Some(REPO), true, {
                let told = Rc::clone(&asked);
                move |asking, site| {
                    let mut forge = GitHub::new(
                        site,
                        Box::new(Failing {
                            asked: told.clone(),
                        }),
                    );
                    ask(&mut forge, asking)
                }
            });
            assert_eq!(rc, 1, "{given:?} failed at nothing");
            assert_eq!(out, "", "{given:?} printed an answer");
            assert!(
                err.contains(&format!("ERROR: {call} failed")),
                "{given:?}: {err}"
            );
            // The words asked are the words of that verb, so the failing read is
            // the read the verb makes.
            let words = asked.borrow().clone();
            assert!(words.contains(&"api".to_string()), "{given:?}: {words:?}");
        }
    }

    /// A runner that answers nothing: the whole of the failure line comes from the
    /// name the adapter gives the call it was making.
    struct Failing {
        asked: Rc<RefCell<Vec<String>>>,
    }

    impl GhRunner for Failing {
        fn run(&mut self, args: &[&str]) -> Result<String, String> {
            *self.asked.borrow_mut() = args.iter().map(|a| a.to_string()).collect();
            Err("the call did not answer".to_string())
        }
    }

    #[test]
    fn labels_prints_the_names_one_to_a_line() {
        let run = one(&["labels", "7"], Ok("tehas:ready\nbug"));
        assert_eq!(run.said(), (0, "tehas:ready\nbug\n", ""));
        assert_eq!(
            run.calls[0],
            (Term::Labels, 7, String::new(), REPO.to_string())
        );
    }

    #[test]
    fn an_issue_that_carries_no_labels_prints_nothing_and_answers_zero() {
        // The empty list is the site's answer, so it is 0 and no line: a caller
        // that read it as a failure would clear labels nobody took off.
        let run = one(&["labels", "7"], Ok("none"));
        assert_eq!(run.said(), (0, "", ""));
        assert_eq!(run.calls.len(), 1, "the read was not made");
        // A name the site gave twice is the list it holds, printed twice: the
        // answer is a list and not a set.
        let run = one(&["labels", "7"], Ok("bug\nbug"));
        assert_eq!(run.said(), (0, "bug\nbug\n", ""));
    }

    #[test]
    fn the_state_line_is_kind_account_and_time() {
        // Tabs between them: a caller reads the line with IFS=tab.
        let run = one(
            &["label-state", "7", "tehas:running"],
            Ok("unlabeled\tbot\t2026-01-02T00:00:00Z"),
        );
        assert_eq!(
            run.said(),
            (0, "unlabeled\tbot\t2026-01-02T00:00:00Z\n", "")
        );
        assert_eq!(
            run.calls[0],
            (
                Term::State,
                7,
                "tehas:running".to_string(),
                REPO.to_string()
            )
        );
    }

    #[test]
    fn an_event_that_names_nothing_leaves_its_field_empty() {
        // The account of an event whose actor is gone, and the time of one that
        // was never stamped: each prints the empty field it has, so a caller
        // compares an empty name with its allowed list and decides nobody was
        // asked, which is the rule of AGENTS.md rule 5 kept where the site put it.
        let run = one(&["label-state", "7", "tehas:ready"], Ok("labeled\t\t"));
        assert_eq!(run.said(), (0, "labeled\t\t\n", ""));
        let run = one(&["approval", "7"], Ok("alice\t"));
        assert_eq!(run.said(), (0, "alice\t\n", ""));
    }

    #[test]
    fn a_label_the_issue_never_carried_prints_nothing_and_answers_zero() {
        let run = one(&["label-state", "7", "tehas:ready"], Ok("none"));
        assert_eq!(run.said(), (0, "", ""));
        assert_eq!(run.calls.len(), 1, "a state read that was not made");
        // The same for the approval: no application is an answer and not a read
        // that failed, and the caller compares nobody with the allowed list.
        let run = one(&["approval", "7"], Ok("none"));
        assert_eq!(run.said(), (0, "", ""));
    }

    #[test]
    fn the_approval_line_is_the_line_tehas_label_actor_prints() {
        // The pair `tehas_label_actor` prints, in that order and with the tab of
        // that line between them: the account that applied the label that lets a
        // cycle start, and the moment it was applied.
        let run = one(&["approval", "7"], Ok("alice\t2026-01-01T00:00:00Z"));
        assert_eq!(run.said(), (0, "alice\t2026-01-01T00:00:00Z\n", ""));
        // The one label that approves work, of the issue the caller named. The
        // word is the forge's own, so no argument of the call named another.
        assert_eq!(
            run.calls[0],
            (Term::Approval, 7, String::new(), REPO.to_string())
        );
    }

    #[test]
    fn a_write_prints_nothing_and_answers_zero() {
        for verb in ["add-label", "remove-label"] {
            let run = one(&[verb, "7", "tehas:ready"], Ok(""));
            assert_eq!(run.said(), (0, "", ""), "{verb}");
            assert_eq!(run.calls.len(), 1, "{verb} was asked twice");
            assert_eq!(run.calls[0].2, "tehas:ready", "{verb} lost its label");
        }
    }

    #[test]
    fn a_failed_call_is_one_line_on_standard_error_and_exits_one() {
        // The line is the forge's own: the term that stopped and gh's reason, made
        // from its fixed list. Nothing reaches standard output, so a line a caller
        // reads there is always an answer and never a complaint.
        let run = one(&["labels", "7"], Err("HTTP 403"));
        assert_eq!(
            run.said(),
            (1, "", "console forge: ERROR: the issue failed: HTTP 403\n")
        );
        // A call that gave no words still says what stopped it, with no colon left
        // hanging at the end of the line.
        let run = one(&["approval", "7"], Err(""));
        let said = run.said();
        assert_eq!(said.0, 1);
        assert_eq!(said.1, "");
        assert_eq!(
            said.2,
            "console forge: ERROR: the issue's label events failed\n"
        );
    }

    #[test]
    fn a_bad_issue_number_is_refused_before_the_site_is_asked() {
        for issue in [
            "",
            "0",
            "0000007",
            "0x7",
            "7 ",
            "-7",
            "7.0",
            "abc",
            "7\n",
            "+7",
            "\u{7c7}",
            "99999999999999999999999",
            // Past the number a read answers as one: two spellings of it name one
            // issue, which is what `console shipped answer` refuses too.
            "9007199254740993",
            "18446744073709551616",
            "18446744073709551615",
        ] {
            for verb in [
                "labels",
                "label-state",
                "approval",
                "add-label",
                "remove-label",
            ] {
                let args: Vec<&str> = if verb == "labels" || verb == "approval" {
                    vec![verb, issue]
                } else {
                    vec![verb, issue, "tehas:ready"]
                };
                let run = one(&args, Ok("none"));
                assert_eq!(run.rc, 64, "{args:?} was accepted");
                assert!(
                    run.err.contains("is no issue number"),
                    "{args:?}: {}",
                    run.err
                );
                assert!(run.calls.is_empty(), "{args:?} reached the site");
            }
        }
        // Every digit the caller wrote is asked, and the number of the call is the
        // number of the argument, whole. The ceiling itself is a number.
        let run = one(&["approval", "1234567890"], Ok("none"));
        assert_eq!(run.rc, 0);
        assert_eq!(
            run.calls[0],
            (Term::Approval, 1234567890, String::new(), REPO.to_string())
        );
        let run = one(&["labels", "9007199254740992"], Ok("none"));
        assert_eq!(run.rc, 0, "{}", run.err);
        assert_eq!(run.calls[0].1, ISSUE_MAX);
    }

    #[test]
    fn a_refused_argument_cannot_forge_a_line_of_the_log() {
        // The words of a bad argument come back, so a caller can see what it
        // typed: without a control character, and bounded, because a poll stamps
        // one line per call and a terminal reads it.
        let run = one(&["labels", "7\nforged 1970-01-01"], Ok("none"));
        assert_eq!(run.rc, 64);
        assert_eq!(
            run.err,
            "console forge: labels: 7forged 1970-01-01 is no issue number\n"
        );
        let long = "7".repeat(4000);
        let run = one(&["labels", &long], Ok("none"));
        assert_eq!(run.err.lines().count(), 1, "{}", run.err);
        assert!(
            run.err.chars().count() < 200,
            "{} bytes echoed back",
            run.err.chars().count()
        );
    }

    #[test]
    fn a_name_that_is_no_label_is_refused_before_the_site_is_asked() {
        // A name with a space, a newline or a slash in it is no label of tehas: 64
        // here, and never one argument of a request.
        for name in [
            "",
            "has space",
            "a\nb",
            "x/1",
            "tehas:ready?",
            "\u{fc}nicode",
        ] {
            for verb in ["label-state", "add-label", "remove-label"] {
                let run = one(&[verb, "7", name], Ok("none"));
                assert_eq!(run.rc, 64, "{verb} {name:?} was accepted");
                assert!(
                    run.err.contains("is no label name"),
                    "{verb} {name:?}: {}",
                    run.err
                );
                assert!(run.calls.is_empty(), "{verb} {name:?} reached the site");
            }
        }
        // The set the writes accept, and the ceiling GitHub gives a name.
        for name in ["tehas:ready", "bug", "needs-human-attention", "a_b9"] {
            assert!(is_label(name), "{name:?} was refused");
        }
        let long = "t".repeat(50);
        assert!(is_label(&long), "the longest label was refused");
        assert!(!is_label(&format!("{long}x")));
    }

    #[test]
    fn words_that_ask_no_term_are_a_bad_argument() {
        let cases: Vec<(Vec<&str>, &str)> = vec![
            (vec![], "what is wanted of the forge"),
            (vec!["labels"], "one issue number"),
            (vec!["labels", "7", "extra"], "one issue number"),
            (vec!["approval"], "one issue number"),
            (vec!["approval", "7", "tehas:ready"], "one issue number"),
            (vec!["label-state", "7"], "one issue number and one label"),
            (vec!["add-label"], "one issue number and one label"),
            (
                vec!["remove-label", "7", "bug", "extra"],
                "one issue number and one label",
            ),
            (vec!["comment", "7"], "labels, label-state, approval"),
            (vec!["Labels", "7"], "labels, label-state, approval"),
            (vec!["labels", "--repo"], "is no issue number"),
        ];
        for (args, want) in cases {
            let run = one(&args, Ok("none"));
            assert_eq!(run.rc, 64, "{args:?} was not usage");
            assert_eq!(run.out, "", "{args:?} printed an answer");
            assert!(run.err.contains(want), "{args:?}: {}", run.err);
            // A word that is no number is told as such; every other shape of a bad
            // call is shown the line of the five verbs.
            if want != "is no issue number" {
                assert!(
                    run.err.contains("usage: console forge"),
                    "{args:?}: {}",
                    run.err
                );
            }
            assert!(run.calls.is_empty(), "{args:?} reached the site");
        }
        // The usage line names each verb with the words it takes, so a caller that
        // reads it once can fix the call rather than guess it.
        assert!(one(&[], Ok("none"))
            .err
            .contains("label-state <issue> <label>"));
    }

    #[test]
    fn without_the_repository_nothing_is_asked_and_the_status_is_seventy_eight() {
        // The case the issue names: 78 before the first gh call, said on standard
        // error, and never printed as the empty answer a caller would read as "the
        // issue carries no labels".
        for name in [
            None,
            Some(""),
            Some("   "),
            Some("not a repository"),
            Some("only"),
        ] {
            for verb in [
                ["labels", "7"].as_slice(),
                ["approval", "7"].as_slice(),
                ["add-label", "7", "tehas:ready"].as_slice(),
            ] {
                let run = with(verb, name, true, Ok("none"));
                assert_eq!(run.rc, 78, "{name:?} {verb:?}");
                assert_eq!(run.out, "", "{name:?} {verb:?} printed an answer");
                assert!(
                    run.err.contains("TEHAS_REPO is not set"),
                    "{name:?} {verb:?}: {}",
                    run.err
                );
                assert!(run.calls.is_empty(), "{name:?} {verb:?} reached the site");
            }
        }
        // The setting of a host is `owner/name`, and a value a file wrote with
        // space or a newline about it is still the repository it names: the name is
        // trimmed before its shape is read, so a trailing newline of an
        // instance.env is no reason to stop the poll.
        let run = with(&["labels", "7"], Some(" acme/widget\n"), true, Ok("none"));
        assert_eq!(run.rc, 0, "{}", run.err);
        assert_eq!(run.calls[0].3, REPO);
    }

    #[test]
    fn without_gh_nothing_is_asked_and_the_status_is_seventy_eight() {
        // A program that is not there is unusable and not a read that failed, so
        // the status is the 78 of a command that cannot be used at all, and no call
        // of a term is made to discover it.
        for verb in [
            ["labels", "7"].as_slice(),
            ["remove-label", "7", "tehas:ready"].as_slice(),
        ] {
            let run = with(verb, Some(REPO), false, Ok("none"));
            assert_eq!(run.rc, 78, "{verb:?}");
            assert_eq!(run.out, "", "{verb:?} printed an answer");
            assert!(
                run.err.contains("gh could not be run"),
                "{verb:?}: {}",
                run.err
            );
            assert!(run.calls.is_empty(), "{verb:?} reached the site");
        }
    }

    #[test]
    fn a_program_must_be_a_file_of_path_that_may_be_run() {
        // The name a host runs is its instance's shim or plain gh: a directory that
        // holds no gh, a name that is a directory, and a file no one may run are
        // all "no gh".
        let dir = scratch("path");
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let gh = bin.join("gh");
        let empty = dir.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        // PATH is a list of directories, so the case builds one that holds the
        // program and one that holds nothing, and asks them in both orders.
        let both = |first: &str, second: &str| {
            format!(
                "{}:{}",
                dir.join(first).display(),
                dir.join(second).display()
            )
        };
        let here = gh.display().to_string();
        std::fs::write(&gh, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&gh, PermissionsExt::from_mode(0o755)).unwrap();
        assert_eq!(
            which("gh", &both("bin", "empty")).as_deref(),
            Some(here.as_str())
        );
        assert_eq!(
            which("gh", &both("empty", "bin")).as_deref(),
            Some(here.as_str()),
            "a gh of a later directory"
        );
        std::fs::remove_file(&gh).unwrap();
        std::fs::create_dir_all(&gh).unwrap();
        assert_eq!(
            which("gh", &both("bin", "empty")),
            None,
            "a directory is no gh"
        );
        std::fs::remove_dir_all(&gh).unwrap();
        std::fs::write(&gh, "not a program\n").unwrap();
        assert_eq!(
            which("gh", &both("bin", "empty")),
            None,
            "a file no one may run is no gh"
        );
        std::fs::set_permissions(&gh, PermissionsExt::from_mode(0o700)).unwrap();
        assert!(
            which("gh", &both("bin", "empty")).is_some(),
            "a gh of its owner alone is gh"
        );
        assert_eq!(which("gh", ""), None, "an empty PATH holds no gh");
        assert_eq!(
            which("gh", &dir.join("nowhere").display().to_string()),
            None
        );
    }

    #[test]
    fn the_instance_shim_is_the_program_when_it_is_installed() {
        // The shim of the instance takes the bot's token and keeps the write log
        // the poll's cost record reads, so a run that reached gh by name alone
        // would write unmeasured and exit 0. Where there is no shim, gh of PATH
        // answers, and where there is neither, the run is the 78 of no program.
        let dir = scratch("shim");
        let shim = dir.join("shim");
        std::fs::create_dir_all(&shim).unwrap();
        let gh = shim.join("gh");
        std::fs::write(&gh, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&gh, PermissionsExt::from_mode(0o755)).unwrap();
        let home = dir.display().to_string();
        let named = gh.display().to_string();
        // A stripped PATH: only the shim can be the program.
        assert_eq!(gh_program(Some(&home), "").as_deref(), Some(named.as_str()));
        // A shim that is not runnable is no shim, and neither is a directory.
        std::fs::set_permissions(&gh, PermissionsExt::from_mode(0o644)).unwrap();
        assert_eq!(gh_program(Some(&home), ""), None, "a shim no one may run");
        std::fs::remove_file(&gh).unwrap();
        std::fs::create_dir_all(&gh).unwrap();
        assert_eq!(gh_program(Some(&home), ""), None, "a directory is no shim");
        std::fs::remove_dir_all(&shim).unwrap();
        // No shim: the program is the gh of PATH, and none when PATH holds no gh.
        assert_eq!(gh_program(Some(&home), ""), None);
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("gh"), "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(bin.join("gh"), PermissionsExt::from_mode(0o755)).unwrap();
        let path = bin.display().to_string();
        assert_eq!(
            gh_program(Some(&home), &path).as_deref(),
            Some(bin.join("gh").display().to_string().as_str())
        );
        // No home at all is no shim either: PATH decides.
        assert_eq!(
            gh_program(None, &path).as_deref(),
            Some(bin.join("gh").display().to_string().as_str())
        );
    }

    #[test]
    fn the_repository_of_the_setting_and_no_other() {
        // The words of the setting decide which repository a run asks, and a name
        // that is no `owner/name` is refused before a call: no reading, and no
        // write, leaves the instance's own repository.
        let wide = format!("acme/{}", "w".repeat(REPO_PART_MAX));
        let wider = format!("acme/{}", "w".repeat(REPO_PART_MAX + 1));
        for (given, ok) in [
            ("acme/widget", true),
            ("acme-widget/example.product", true),
            ("acme/widget/extra", false),
            ("acme/", false),
            ("/widget", false),
            ("widget", false),
            ("acme/wid get", false),
            ("acme/..%2fetc", false),
            ("", false),
            // The ceiling is this command's own, not the label ceiling of the
            // adapter: a long name of a repository is still a repository.
            (wide.as_str(), true),
            (wider.as_str(), false),
        ] {
            let run = with(&["labels", "7"], Some(given), true, Ok("none"));
            if ok {
                assert_eq!(run.rc, 0, "{given:?}: {}", run.err);
                assert_eq!(run.calls.len(), 1, "{given:?} asked twice");
                // The reading is asked of the trimmed setting, whatever space a
                // file wrote about it.
                assert_eq!(run.calls[0].3, given.trim(), "{given:?}");
            } else {
                assert_eq!(run.rc, 78, "{given:?} reached the site");
                assert!(run.calls.is_empty(), "{given:?} reached the site");
            }
        }
    }

    #[test]
    fn a_term_prints_only_the_answer_of_its_own_kind() {
        // The pairing is in the type: each answer carries its term, and the arms
        // of `line_of` name the pairs that print. A foreign pair cannot be built
        // without a term of the wrong kind reaching the arm, which `ask` cannot
        // do: it builds each answer from the term it was given.
        let event = Answer::Event(
            Term::State,
            LabelEvent {
                kind: "labeled".to_string(),
                actor: "bot".to_string(),
                at: "2026-01-01T00:00:00Z".to_string(),
            },
        );
        assert_eq!(line_of(&event), "labeled\tbot\t2026-01-01T00:00:00Z\n");
        let approval = Answer::Approval(
            Term::Approval,
            Approval {
                actor: "alice".to_string(),
                at: "2026-01-01T00:00:00Z".to_string(),
            },
        );
        assert_eq!(line_of(&approval), "alice\t2026-01-01T00:00:00Z\n");
        assert_eq!(line_of(&Answer::Names(Term::Labels, Vec::new())), "");
        assert_eq!(
            line_of(&Answer::Names(
                Term::Labels,
                vec!["bug".to_string(), "bug".to_string()]
            )),
            "bug\nbug\n"
        );
        // The four empty answers, one per term that prints nothing.
        for term in [Term::State, Term::Approval, Term::Add, Term::Remove] {
            assert_eq!(line_of(&Answer::Done(term)), "", "{term:?}");
        }
    }

    #[test]
    fn each_verb_asks_the_adapter_the_words_it_should() {
        // The production path from a word of the command to one argv: `run` with
        // the closure `main` gives it, over a `GitHub` whose runner is a fixture.
        // The label and the repository have different places in every path, so a
        // call that mixed them up would name the wrong issue, field or endpoint.
        let cases: [(&[&str], &[&str]); 5] = [
            (&["labels", "7"], &["api", "repos/acme/widget/issues/7"]),
            (
                &["label-state", "7", "tehas:running"],
                &[
                    "api",
                    "repos/acme/widget/issues/7/events?per_page=100",
                    "--paginate",
                ],
            ),
            (
                &["approval", "7"],
                &[
                    "api",
                    "repos/acme/widget/issues/7/events?per_page=100",
                    "--paginate",
                ],
            ),
            (
                &["add-label", "7", "tehas:ready"],
                &[
                    "api",
                    "-X",
                    "POST",
                    "repos/acme/widget/issues/7/labels",
                    "-f",
                    "labels[]=tehas:ready",
                ],
            ),
            (
                &["remove-label", "7", "tehas:ready"],
                &[
                    "api",
                    "-X",
                    "DELETE",
                    "repos/acme/widget/issues/7/labels/tehas%3Aready",
                ],
            ),
        ];
        for (given, want) in cases {
            let asked = Rc::new(RefCell::new(Vec::new()));
            let fixture = Rc::new(RefCell::new(Fixture::new()));
            let given: Vec<String> = given.iter().map(|s| s.to_string()).collect();
            let (rc, out, err) = run(&given, Some(REPO), true, {
                let told = Rc::clone(&asked);
                let answers = Rc::clone(&fixture);
                move |asking, site| {
                    // The closure of `execute`, with the adapter's fixture runner
                    // in place of gh: the same call `main` makes.
                    let mut forge = GitHub::new(
                        site,
                        Box::new(Recorded {
                            asked: told.clone(),
                            answers: answers.clone(),
                        }),
                    );
                    ask(&mut forge, asking)
                }
            });
            let words = asked.borrow().clone();
            assert_eq!(words, want.to_vec(), "{given:?}: {err}");
            assert_eq!(rc, 0, "{given:?}: {err}");
            assert_eq!(out, "", "{given:?} printed an answer");
        }
    }

    /// What a fixture runner answers to a read of one issue's timeline: no event
    /// of any label, which is the answer each verb of the case is asked about.
    struct Fixture {
        events: String,
    }

    impl Fixture {
        fn new() -> Fixture {
            Fixture {
                events: "[]".to_string(),
            }
        }
    }

    /// A runner that keeps the words it was asked for and answers with the shape
    /// GitHub gives the read those words name.
    struct Recorded {
        asked: Rc<RefCell<Vec<String>>>,
        answers: Rc<RefCell<Fixture>>,
    }

    impl GhRunner for Recorded {
        fn run(&mut self, args: &[&str]) -> Result<String, String> {
            *self.asked.borrow_mut() = args.iter().map(|a| a.to_string()).collect();
            // A path of `/events` is the timeline read, answered from the fixture;
            // a read of the issue answers the empty list of labels, which is the
            // site's answer and not a read that could not be read; a write answers
            // what the site answers to it.
            if args.iter().any(|a| a.contains("/events")) {
                return Ok(self.answers.borrow().events.clone());
            }
            if args.contains(&"api") && !args.contains(&"-X") {
                return Ok("{\"number\":7,\"labels\":[]}".to_string());
            }
            Ok(String::new())
        }
    }

    #[test]
    fn an_adapter_of_one_run_is_asked_by_each_term_it_is_given() {
        // One adapter for a run, so the terms of one run share its program and
        // its bound; a caller that asks a state and an approval of one issue pays
        // for the pages of the events once each and builds no second gh.
        let asks = Rc::new(RefCell::new(Vec::new()));
        let told = Rc::clone(&asks);
        let given: Vec<String> = ["label-state", "7", "tehas:ready"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let mut forge: Option<GitHub> = None;
        let mut once = |asking: &Asking, repository: &str| {
            let held = forge.get_or_insert_with(|| {
                GitHub::new(
                    repository,
                    Box::new(Recorded {
                        asked: Rc::clone(&told),
                        answers: Rc::new(RefCell::new(Fixture::new())),
                    }),
                )
            });
            ask(held, asking)
        };
        let asking = asking(&given).unwrap();
        let asking = Asking { issue: 7, ..asking };
        let first = once(&asking, REPO).unwrap();
        let second = once(&asking, REPO).unwrap();
        assert!(matches!(first, Answer::Done(_)));
        assert!(matches!(second, Answer::Done(_)));
        // The words of the last call: the events read is paginated, so the path
        // is one word and the flag the next. That the second term of the run
        // reached the same path names the adapter's repository once for both.
        let words = asks.borrow().clone();
        assert_eq!(
            words.get(1).map(String::as_str),
            Some("repos/acme/widget/issues/7/events?per_page=100")
        );
        assert!(words.contains(&"--paginate".to_string()));
    }

    /// A scratch directory of the case, under cargo's own place for test files.
    fn scratch(name: &str) -> std::path::PathBuf {
        let base = env::var("CARGO_TARGET_TMPDIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| env::temp_dir());
        let dir = base.join(format!("forge-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
