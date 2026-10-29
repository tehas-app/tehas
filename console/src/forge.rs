//! The forge as a command: what `libexec/tidy`, `libexec/status` and
//! `lib/labels.sh` ask of GitHub, asked through `tehas_core::forge`, so that
//! bash holds no gh call, no REST path and no rule of its own (#800 part 2).
//!
//!   console forge labels <issue>
//!     The labels the issue carries, one per line. It is the site's list, so a
//!     name it carries twice appears twice; an issue that carries none prints
//!     nothing and exits 0.
//!   console forge label-state <issue> <label>
//!     `<kind>\t<account>\t<time>` of the newest `labeled` or `unlabeled` event
//!     of the label, which is the line `tehas_label_state` of `lib/labels.sh`
//!     prints, or nothing when no event of the issue carries it. Account and
//!     time are empty when the event names none. Authorisation stays the
//!     caller's (AGENTS.md rule 5): this says who, never whether that account
//!     may start anything.
//!   console forge approval <issue>
//!     `<account>\t<time>` of the newest application of `tehas:ready`, the line
//!     `tehas_label_actor` prints, or nothing when no event applied it. A label
//!     taken off answers no approval, whoever took it off. The word is the
//!     forge's own: a caller cannot ask whether a label that approves nothing
//!     approved the work.
//!   console forge add-label <issue> <label>
//!   console forge remove-label <issue> <label>
//!     Put the label on, take it off. An issue that carries the label already
//!     is the state wanted, and one that does not carry a label taken off is
//!     already in it, so either verb may be asked twice. A name no label of
//!     tehas can have is a bad argument and never reaches gh.
//!
//! `<issue>` is digits and nothing else: a number that is not one is no
//! question, and 64 stops it before anything is asked.
//!
//! The repository is `TEHAS_REPO`, `owner/name`, as `lib/config.sh` exports it,
//! and the call names no repository of its own, so a poll has no way to write
//! where its instance did not choose. `gh` is run from `PATH`, where
//! `host/env.sh` puts the instance's shim first, and the command holds no token
//! of its own: the shim takes one, or the caller of a whole poll holds
//! `TEHAS_GH_TOKEN` for every call of it (#938). One call runs under a bound of
//! its own, TERM then KILL, as `timeout -k` does: a poll holds its lock for the
//! whole read, and a gh that never returned would hold it forever.
//!
//! Exit: 0 the call answered, and what was asked of it is printed; 1 gh failed,
//! with one line on standard error naming the call and gh's reason, which holds
//! no host and no token; 64 a bad argument, before anything is asked; 78
//! `TEHAS_REPO` is not set, before the first gh call.

use std::env;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;
use tehas_core::forge::{Bounded, Forge, ForgeError, GitHub};

/// The longest label GitHub accepts, the ceiling `is_label` of the adapter
/// holds. Checked here too, so a name that cannot be a label answers 64 as a
/// bad argument rather than 1 as a call that failed.
const LABEL_MAX: usize = 50;

/// The bound of one call of gh: the thirty seconds the dispatcher's reads give
/// it, so the two readers of one issue wait the same time for an answer.
const CALL_BOUND: Duration = Duration::from_secs(30);

/// What one verb asks of the forge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Term {
    Labels,
    State,
    Approval,
    Add,
    Remove,
}

/// The five verbs, the term each asks, and the words each takes after the verb:
/// one table, so the usage line a caller is shown and the verbs the command has
/// cannot drift apart. The approval names no label, because the label that lets
/// a cycle start is the forge's.
const VERBS: [(&str, Term, usize); 5] = [
    ("labels", Term::Labels, 1),
    ("label-state", Term::State, 2),
    ("approval", Term::Approval, 1),
    ("add-label", Term::Add, 2),
    ("remove-label", Term::Remove, 2),
];

/// The words of one run, and the term they ask: None when they ask none, which
/// is usage. A term with a word missing and a term with one too many are the
/// same to a caller: it did not say what it wanted.
fn verb_of(given: &[String]) -> Option<(&'static str, &str, &str, Term)> {
    let (verb, term, words) = VERBS
        .iter()
        .find(|(name, _, _)| given.first().is_some_and(|a| a == name))?;
    let rest = &given[1..];
    if rest.len() != *words {
        return None;
    }
    let label = if *words == 2 { rest[1].as_str() } else { "" };
    Some((*verb, rest[0].as_str(), label, *term))
}

/// The words a caller is shown when its arguments ask no term: which word was
/// left out, and then the shape of all five verbs, so one line of a log is
/// enough to fix the call.
fn usage(given: &[String]) -> String {
    let why = match given.first().map(String::as_str) {
        None => "what is wanted of the forge".to_string(),
        Some(verb) => match VERBS.iter().find(|(name, _, _)| *name == verb) {
            Some((_, _, 2)) => "one issue number and one label".to_string(),
            Some(_) => "one issue number".to_string(),
            None => "labels, label-state, approval, add-label or remove-label".to_string(),
        },
    };
    let of = VERBS
        .iter()
        .map(|(name, _, words)| {
            let words = if *words == 2 { " <label>" } else { "" };
            format!("{name} <issue>{words}")
        })
        .collect::<Vec<_>>()
        .join(" | ");
    format!("console forge: {why}\nusage: console forge {of}\n")
}

/// The characters a label of tehas is built from, the set `is_label` accepts.
fn label_shaped(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= LABEL_MAX
        && label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'-' | b'_'))
}

/// Whether `<issue>` is a number: digits and nothing else, non-empty. Anything
/// else is no question and never reaches the site, which is also what keeps
/// `repos/<repo>/issues/<issue>` to one path segment.
fn digits(issue: &str) -> bool {
    !issue.is_empty() && issue.bytes().all(|b| b.is_ascii_digit())
}

/// Whether a repository is `owner/name`: the shape `lib/config.sh` checks the
/// setting for. A setting that is no repository is no repository to ask, which
/// is the 78 of a run that cannot reach GitHub and never a request aimed at
/// something else.
fn repo_shaped(repo: &str) -> bool {
    let mut parts = repo.split('/');
    let (Some(owner), Some(name), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    [owner, name].iter().all(|part| {
        !part.is_empty()
            && part.len() <= LABEL_MAX
            && part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    })
}

/// `TEHAS_REPO`, trimmed: the repository of the instance, or None when the name
/// is unset, empty, or nothing but space. Nothing here asks GitHub, runs a
/// program or writes a file.
fn repo() -> Option<String> {
    match env::var("TEHAS_REPO") {
        Ok(v) => {
            let v = v.trim();
            (!v.is_empty()).then(|| v.to_string())
        }
        Err(_) => None,
    }
}

/// Whether `program` is where `PATH` says it is and may be run, as `command -v`
/// answers. `clean`'s `which` walks `PATH` the same way.
fn on_path(program: &str, path: &str) -> bool {
    path.split(':').any(|dir| {
        std::fs::metadata(Path::new(dir).join(program))
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    })
}

/// What a term answered: the three readings of the forge, and the two writes,
/// which say only that the state is now as asked.
enum Answer {
    Names(Vec<String>),
    Event(String, String, String),
    Approval(String, String),
    Done,
}

/// The line of one answer: the fields of an event with tabs between them, the
/// labels one to a line, and nothing at all for the empty answer of a term the
/// site had nothing to say about. A tab and not a space: a caller reads the line
/// the way `lib/labels.sh` writes one, with `IFS=$'\t' read`.
fn line_of(term: Term, answer: &Answer) -> String {
    match (term, answer) {
        (Term::Labels, Answer::Names(names)) if !names.is_empty() => {
            format!("{}\n", names.join("\n"))
        }
        (Term::State, Answer::Event(kind, actor, at)) => format!("{kind}\t{actor}\t{at}\n"),
        (Term::Approval, Answer::Approval(actor, at)) => format!("{actor}\t{at}\n"),
        _ => String::new(),
    }
}

/// One verb against one forge. Taken as a function of a [`Forge`] so that a case
/// hands it a stub in place of gh, and the command and the case run the one
/// mapping from an answer to a line and a status.
fn ask(forge: &mut dyn Forge, term: Term, issue: u64, label: &str) -> Result<Answer, ForgeError> {
    match term {
        Term::Labels => forge.labels(issue).map(Answer::Names),
        Term::State => forge.label_state(issue, label).map(|e| match e {
            Some(e) => Answer::Event(e.kind, e.actor, e.at),
            None => Answer::Done,
        }),
        Term::Approval => forge.approval(issue).map(|a| match a {
            Some(a) => Answer::Approval(a.actor, a.at),
            None => Answer::Done,
        }),
        Term::Add => forge.add_label(issue, label).map(|()| Answer::Done),
        Term::Remove => forge.remove_label(issue, label).map(|()| Answer::Done),
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
    mut call: impl FnMut(Term, u64, &str, &str) -> Result<Answer, ForgeError>,
) -> (u8, String, String) {
    // The words first, and all of them: a bad one is 64 and asks GitHub nothing,
    // so a poll spends no call, no token and none of its bound on a typo.
    let Some((verb, issue, label, term)) = verb_of(given) else {
        return (64, String::new(), usage(given));
    };
    // Digits and nothing else, and a number the term can hold: either says the
    // question was not asked properly, which is 64 before the site is asked. A
    // string of digits longer than any issue is the second case.
    if !digits(issue) {
        return (
            64,
            String::new(),
            format!("console forge: {verb}: {issue} is no issue number\n"),
        );
    }
    let Ok(issue) = issue.parse::<u64>() else {
        return (
            64,
            String::new(),
            format!("console forge: {verb}: {issue} is no issue number\n"),
        );
    };
    // A term that names a label checks the name it was given: one with a space, a
    // newline or a slash in it is no label of tehas, and never one argument of a
    // request. The forge's own label is not asked about, so it is not checked.
    if matches!(term, Term::State | Term::Add | Term::Remove) && !label_shaped(label) {
        return (
            64,
            String::new(),
            format!("console forge: {verb}: {label} is no label name\n"),
        );
    }
    // The setting, trimmed, and then its shape: the `owner/name` of a host's
    // instance.env may carry the space or a newline a file writes with, and that
    // is no reason to stop a poll. Anything else is no repository to ask. The
    // name goes to the call, so the reading is asked of the repository the
    // setting named and of no other.
    let Some(repository) = repository.map(str::trim).filter(|r| repo_shaped(r)) else {
        return (
            78,
            String::new(),
            format!("console forge: {verb}: TEHAS_REPO is not set\n"),
        );
    };
    if !gh {
        return (
            78,
            String::new(),
            format!("console forge: {verb}: gh could not be run: it is not in PATH\n"),
        );
    }
    match call(term, issue, label, repository) {
        // An answer prints its line, which may be the empty one: the site said
        // there is no such event, and that is an answer and never a failed call.
        Ok(answer) => (0, line_of(term, &answer), String::new()),
        // One line: the term that stopped and gh's own words for it, made from
        // the forge's fixed list, so nothing of an answer that may name a host or
        // carry a token is repeated here.
        Err(e) => (1, String::new(), format!("console forge: {}\n", e.line())),
    }
}

/// One run's words to the streams of the process: what a term answered on
/// standard output, and the line of a run that stopped on standard error. The
/// bytes are written and flushed here, so a run that cannot be heard says
/// nothing extra and still ends with the status it promised.
fn emit(out: &str, err: &str) {
    let o = std::io::stdout();
    let mut o = o.lock();
    let _ = o.write_all(out.as_bytes()).and_then(|()| o.flush());
    if !err.is_empty() {
        let e = std::io::stderr();
        let mut e = e.lock();
        let _ = e.write_all(err.as_bytes()).and_then(|()| e.flush());
    }
}

pub fn main(given: &[String]) -> u8 {
    // The setting is read once and kept, so the name a poll's log records is the
    // name this run asked, whatever the environment holds later.
    let repository = repo();
    // The program by name, so the shim of the instance answers where host/env.sh
    // put its directory first, and plain gh where there is no shim.
    let gh = on_path("gh", &env::var("PATH").unwrap_or_default());
    let (rc, out, err) = run(
        given,
        repository.as_deref(),
        gh,
        |term, issue, label, repo| {
            // The adapter of the repository the setting named, and the runner of one
            // call: the bound belongs to the runner, so a gh that never returned is
            // stopped and the poll's lock is let go.
            let mut forge = GitHub::new(repo, Box::new(Bounded::new("gh", CALL_BOUND)));
            ask(&mut forge, term, issue, label)
        },
    );
    emit(&out, &err);
    rc
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
        let (rc, out, err) = run(&given, repo, gh, move |term, issue, label, site| {
            held.borrow_mut()
                .push((term, issue, label.to_string(), site.to_string()));
            match &answer {
                Ok(text) => Ok(answer_of(term, text)),
                Err(why) => Err(ForgeError::new(call_of(term), why.clone())),
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

    /// The call a term makes, as the adapter names it, so the line a case reads
    /// is the line a host's run logs.
    fn call_of(term: Term) -> &'static str {
        match term {
            Term::Labels => "the issue",
            Term::State | Term::Approval => "the issue's label events",
            Term::Add | Term::Remove => "the label change",
        }
    }

    /// The fixture as the term's answer: `none` is the empty answer of a term the
    /// site had nothing to say about, a line of tab-separated fields is the
    /// event, and the labels verb reads one name to a line.
    fn answer_of(term: Term, text: &str) -> Answer {
        if text == "none" {
            return Answer::Done;
        }
        match term {
            Term::Labels => Answer::Names(
                text.lines()
                    .filter(|l| !l.is_empty())
                    .map(str::to_string)
                    .collect(),
            ),
            Term::State => {
                let f = fields(text, 3);
                Answer::Event(f[0].clone(), f[1].clone(), f[2].clone())
            }
            Term::Approval => {
                let f = fields(text, 2);
                Answer::Approval(f[0].clone(), f[1].clone())
            }
            _ => Answer::Done,
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
    fn the_state_line_is_the_line_tehas_label_state_prints() {
        // Kind, account and time with tabs between them, as the jq of
        // lib/labels.sh writes them: a caller reads the line with IFS=tab.
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
            "0x7",
            "7 ",
            "-7",
            "7.0",
            "abc",
            "7\n",
            "+7",
            "\u{7c7}",
            "99999999999999999999999",
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
        // number of the argument, whole.
        let run = one(&["approval", "1234567890"], Ok("none"));
        assert_eq!(run.rc, 0);
        assert_eq!(
            run.calls[0],
            (Term::Approval, 1234567890, String::new(), REPO.to_string())
        );
        assert_eq!(one(&["labels", "0"], Ok("none")).rc, 0);
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
            assert!(label_shaped(name), "{name:?} was refused");
        }
        let long = "t".repeat(LABEL_MAX);
        assert!(label_shaped(&long), "the longest label was refused");
        assert!(!label_shaped(&format!("{long}x")));
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
        std::fs::write(&gh, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&gh, PermissionsExt::from_mode(0o755)).unwrap();
        assert!(on_path("gh", &both("bin", "empty")));
        assert!(
            on_path("gh", &both("empty", "bin")),
            "a gh of a later directory"
        );
        std::fs::remove_file(&gh).unwrap();
        std::fs::create_dir_all(&gh).unwrap();
        assert!(
            !on_path("gh", &both("bin", "empty")),
            "a directory is no gh"
        );
        std::fs::remove_dir_all(&gh).unwrap();
        std::fs::write(&gh, "not a program\n").unwrap();
        assert!(
            !on_path("gh", &both("bin", "empty")),
            "a file no one may run is no gh"
        );
        std::fs::set_permissions(&gh, PermissionsExt::from_mode(0o700)).unwrap();
        assert!(
            on_path("gh", &both("bin", "empty")),
            "a gh of its owner alone is gh"
        );
        assert!(!on_path("gh", ""), "an empty PATH holds no gh");
        assert!(!on_path("gh", &dir.join("nowhere").display().to_string()));
    }

    #[test]
    fn the_repository_of_the_setting_and_no_other() {
        // The words of the setting decide which repository a run asks, and a name
        // that is no `owner/name` is refused before a call: no reading, and no
        // write, leaves the instance's own repository.
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

    /// A scratch directory of the case, under cargo's own place for test files.
    fn scratch(name: &str) -> std::path::PathBuf {
        let base = std::env::var("CARGO_TARGET_TMPDIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir());
        let dir = base.join(format!("forge-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
