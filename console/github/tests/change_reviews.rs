//! The review bodies of a change, the notes reviewers left on files, its
//! conversation and the answer of whether an account may write. The path of
//! every request is built from the handle's own project, and each test
//! pins the whole command line sent: a handle reads no repository but the
//! one it was made for. That is this lane's security pass. The laws of
//! `tehas_ports::laws` check the same answers.

mod answers;

use std::sync::{Arc, Mutex};

use tehas_core::forge::{Failed, GhRunner};
use tehas_core::time::from_iso;
use tehas_github::GitHub;
use tehas_ports::laws;
use tehas_ports::{At, Bound, ChangeNo, CodeHost, Login, Outcome, Refusal, Unread, ALL_MAX};

type Asked = Arc<Mutex<Vec<String>>>;

/// The state the answers are recorded under: the handle reads the project
/// `acme/widget` and no other.
const PROJECT: &str = "acme/widget";

/// A runner that answers each REST call from `tests/*\/change-reviews/<state>/`,
/// its file named by the path past `repos/acme/widget/`, every byte outside
/// `[A-Za-z0-9=-]` written `_`; a `.fail` file makes the call fail.
struct Rec {
    asked: Asked,
    state: String,
}

fn read_name(args: &[&str]) -> Option<String> {
    let path = args.get(1)?.strip_prefix("repos/acme/widget/")?;
    Some(
        path.chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect(),
    )
}

impl GhRunner for Rec {
    fn run_answer(&mut self, args: &[&str]) -> Result<String, Failed> {
        let answer = read_name(args).and_then(|read| {
            let at = answers::at(
                "change-reviews",
                &self.state,
                &format!("{read}.answer.json"),
            );
            std::fs::read_to_string(at).ok()
        });
        self.run(args).map_err(|why| Failed { why, answer })
    }

    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        self.asked.lock().unwrap().push(args.join(" "));
        let read = read_name(args).ok_or_else(|| format!("no path in {}", args.join(" ")))?;
        std::fs::read_to_string(answers::at(
            "change-reviews",
            &self.state,
            &format!("{read}.json"),
        ))
        .or_else(|_| {
            std::fs::read_to_string(answers::at(
                "change-reviews",
                &self.state,
                &format!("{read}.fail"),
            ))
            .map(|w| w.trim_end().to_string())
            .map_err(|_| format!("no recording of {read}"))
        })
    }
}

fn adapter(state: &str) -> (GitHub, Asked) {
    let asked = Asked::default();
    let rec = Rec {
        asked: Arc::clone(&asked),
        state: state.to_string(),
    };
    let gh = GitHub::new(PROJECT, Box::new(rec)).on_clock(|| 1_767_225_600);
    (gh, asked)
}

fn at(iso: &str) -> At {
    At::new(from_iso(iso).expect("a time"))
}

fn cn(n: u64) -> ChangeNo {
    ChangeNo::new(n).expect("a number")
}

fn ok(law: laws::Law) {
    if let Err(b) = law {
        panic!("broken law: {b:?}");
    }
}

#[test]
fn reviews_are_the_submitted_ones_and_the_path_is_the_handles_own() {
    let (mut gh, asked) = adapter("widget");
    let got = gh.reviews(cn(7), ALL_MAX).expect("reviews");
    assert_eq!(
        asked.lock().unwrap().as_slice(),
        ["api repos/acme/widget/pulls/7/reviews?per_page=100&page=1"]
    );
    let reviews = got.into_items();
    assert_eq!(reviews.len(), 2, "the pending review is left out");
    assert_eq!(reviews[0].id, 11);
    assert_eq!(reviews[0].at, at("2026-10-26T09:20:00Z"));
    assert_eq!(reviews[0].body, "looks well done");
    assert_eq!(reviews[0].by.as_ref().unwrap().login.as_str(), "alice");
    assert_eq!(
        reviews[0].link.as_str(),
        "https://github.com/acme/widget/pull/7#pullrequestreview-11"
    );
    // Past the bound the list is cut, not given short.
    let (mut gh, _asked) = adapter("widget");
    assert_eq!(
        gh.reviews(cn(7), Bound::new(1).unwrap()),
        Err(Outcome::Unread(Unread::Cut))
    );
}

#[test]
fn the_laws_answer_reviews_and_notes_over_the_recordings() {
    let (mut gh, _asked) = adapter("widget");
    ok(laws::code_host::a_change_gives_its_reviews_and_notes(
        &mut gh,
        cn(7),
        &[
            (11, "alice", at("2026-10-26T09:20:00Z"), "looks well done"),
            (13, "bob", at("2026-10-26T09:40:00Z"), "one thing to fix"),
        ],
        &[
            (
                21,
                "alice",
                at("2026-10-26T09:25:00Z"),
                "use the port here",
                "src/main.rs",
                7,
            ),
            (
                22,
                "bob",
                at("2026-10-26T09:45:00Z"),
                "and name it once",
                "src/lib.rs",
                3,
            ),
        ],
    ));
    let (mut gh, _asked) = adapter("widget");
    ok(laws::code_host::a_change_without_reviews_gives_none(
        &mut gh,
        cn(9),
    ));
    let (mut gh, _asked) = adapter("widget");
    ok(
        laws::code_host::may_write_never_guesses_a_name_it_does_not_know(
            &mut gh,
            &Login::new("alice"),
            &Login::new("dave"),
        ),
    );
}

#[test]
fn a_review_of_another_pull_request_is_inconsistent() {
    let (mut gh, _asked) = adapter("wrong");
    assert_eq!(
        gh.reviews(cn(7), ALL_MAX),
        Err(Outcome::Unread(Unread::Inconsistent))
    );
}

#[test]
fn notes_name_their_file_and_merge_the_line_the_site_gives() {
    let (mut gh, asked) = adapter("widget");
    let notes = gh
        .review_comments(cn(7), ALL_MAX)
        .expect("notes")
        .into_items();
    assert_eq!(
        asked.lock().unwrap().as_slice(),
        ["api repos/acme/widget/pulls/7/comments?per_page=100&page=1"]
    );
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0].id, 21);
    assert_eq!(notes[0].path.as_ref().unwrap().as_str(), "src/main.rs");
    assert_eq!(notes[0].line, Some(7), "the line the file stood on");
    assert_eq!(notes[0].edited, None);
    assert_eq!(
        notes[0].link.as_str(),
        "https://github.com/acme/widget/pull/7#discussion_r21"
    );
    assert_eq!(notes[1].line, Some(3));
    assert_eq!(notes[1].edited, Some(at("2026-10-26T10:00:00Z")));
}

#[test]
fn the_conversation_keeps_the_text_as_it_was_read() {
    let (mut gh, asked) = adapter("widget");
    let comments = gh.conversation(cn(7), ALL_MAX).expect("talk").into_items();
    assert_eq!(
        asked.lock().unwrap().as_slice(),
        ["api repos/acme/widget/issues/7/comments?per_page=100&page=1"]
    );
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0].id.as_str(), "1");
    assert_eq!(comments[0].raw.as_deref(), Some("go on"));
    assert_eq!(comments[0].author.as_ref().unwrap().login.as_str(), "alice");
    assert_eq!(comments[0].edited, None);
    assert_eq!(comments[1].edited, Some(at("2026-10-26T09:30:00Z")));
    assert_eq!(
        comments[1].link.as_str(),
        "https://github.com/acme/widget/pull/7#issuecomment-2"
    );
}

#[test]
fn the_write_answer_is_asked_of_the_handles_own_project() {
    let (mut gh, asked) = adapter("widget");
    assert_eq!(gh.may_write(&Login::new("alice")), Ok(true));
    assert_eq!(gh.may_write(&Login::new("bob")), Ok(false));
    assert_eq!(
        asked.lock().unwrap().as_slice(),
        [
            "api repos/acme/widget/collaborators/alice/permission",
            "api repos/acme/widget/collaborators/bob/permission",
        ]
    );
    // A name the recording says is not there fails; it is never answered
    // as a guess.
    let (mut gh, _asked) = adapter("widget");
    assert!(gh.may_write(&Login::new("carol")).is_err());
    // A name outside GitHub's login shape never reaches the site.
    let (mut gh, asked) = adapter("widget");
    assert_eq!(
        gh.may_write(&Login::new("a b/../..")),
        Err(Outcome::Refused(Refusal::Invalid))
    );
    assert!(asked.lock().unwrap().is_empty(), "no request was sent");
}
