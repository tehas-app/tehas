//! The tickets port's `note` and `edit` as the adapter answers them: the
//! issue read, then one comment posted with its markers, or one edit of
//! the issue's title and body at its revision; and its `close`, `assign`
//! and `add_part`, each read first; over samples of GitHub's
//! answers in `tests/samples/ticket-writes/` (hand-made from GitHub's
//! documented shapes, no secret and no private name). A failure sample is
//! gh's error stream, given to the adapter as `Bounded` would give it. No
//! test calls GitHub.

use serde::Deserialize;
use serde_json::Value;
use std::collections::VecDeque;
use std::num::{NonZeroU32, NonZeroU64};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tehas_core::forge::GhRunner;
use tehas_github::markers;
use tehas_github::GitHub;
use tehas_ports::{
    At, BlueprintNo, CommentId, Got, KeyText, LastAttemptMarker, Lost, MarkName, Marker,
    MarkerEdit, MarkerKind, MarkerWord, Outcome, ParkMarker, Passing, Pool, Posted, QuestionKind,
    Ref, Refusal, Revision, Sent, Sha256, TicketKey, TicketWrites, Tickets, Unread, WebLink,
};

const PROJECT: &str = "acme/widget";

/// The words `Bounded` gives for a gh it stopped at its bound.
const BOUND: &str = "gh ran out of its 30s bound";

/// The words a runner gives when its time ran out before it started gh.
const NOT_STARTED: &str = "not started: out of time";

fn sample(name: &str) -> String {
    let at = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/samples/ticket-writes");
    std::fs::read_to_string(at.join(name)).unwrap()
}

/// What the adapter is given for a failure sample: the reason `Bounded`
/// makes of gh's error stream and exit status.
fn failed(name: &str) -> String {
    tehas_core::forge::why(&sample(name), Some(1))
}

fn now() -> i64 {
    1_795_000_000
}

fn key(n: u64) -> TicketKey {
    TicketKey::Number(NonZeroU64::new(n).unwrap())
}

/// One call the runner was given: its arguments, and its standard input.
#[derive(Clone, Debug, PartialEq)]
struct Call {
    args: Vec<String>,
    input: Option<String>,
}

type Calls = Arc<Mutex<Vec<Call>>>;

fn taken(calls: &Calls) -> Vec<Call> {
    calls.lock().unwrap().clone()
}

/// Answers each call from the list in turn and writes every call down. A
/// call past the list fails.
struct Script {
    answers: VecDeque<Result<String, String>>,
    calls: Calls,
}

impl Script {
    fn next(&mut self, args: &[&str], input: Option<&str>) -> Result<String, String> {
        self.calls.lock().unwrap().push(Call {
            args: args.iter().map(|a| a.to_string()).collect(),
            input: input.map(str::to_string),
        });
        self.answers
            .pop_front()
            .unwrap_or_else(|| Err("no answer".into()))
    }
}

impl GhRunner for Script {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        self.next(args, None)
    }

    fn run_input(&mut self, args: &[&str], input: &str) -> Result<String, String> {
        self.next(args, Some(input))
    }
}

fn adapter_of(repo: &str, answers: Vec<Result<String, String>>) -> (GitHub, Calls) {
    let calls = Calls::default();
    let script = Script {
        answers: answers.into(),
        calls: Arc::clone(&calls),
    };
    (GitHub::new(repo, Box::new(script)).on_clock(now), calls)
}

fn adapter(answers: Vec<Result<String, String>>) -> (GitHub, Calls) {
    adapter_of(PROJECT, answers)
}

/// A body of 200 KiB, past what one argument may hold, and unlike any
/// other text of a test. It ends with no line end, as a text beside a
/// marker must.
fn big_body() -> String {
    let line = "A line of the report that only this note holds, number ";
    let mut body = String::new();
    let mut i = 0;
    while body.len() < 200 * 1024 {
        body.push_str(&format!("{line}{i}.\n"));
        i += 1;
    }
    body.truncate(body.trim_end().len());
    body
}

/// No argument holds the text, nor a 64-byte piece of it.
fn kept_out_of_argv(calls: &[Call], text: &str) {
    let probe = &text[..text.len().min(64)];
    for c in calls {
        for a in &c.args {
            assert!(!a.contains(probe), "an argument holds the text: {a:.80}");
        }
    }
}

/// What the note sent on standard input: its body and nothing else.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    body: String,
}

fn sent_body(c: &Call) -> String {
    serde_json::from_str::<Input>(c.input.as_deref().unwrap())
        .unwrap()
        .body
}

/// The calls but the question of this project's name, which an answer
/// naming another repository asks once.
fn but_the_name(calls: &[Call]) -> usize {
    calls
        .iter()
        .filter(|c| c.args.get(1).map(String::as_str) != Some("graphql"))
        .count()
}

fn args(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| w.to_string()).collect()
}

fn read_7() -> Call {
    Call {
        args: args(&["api", "repos/acme/widget/issues/7"]),
        input: None,
    }
}

fn post_7() -> Vec<String> {
    args(&[
        "api",
        "-X",
        "POST",
        "repos/acme/widget/issues/7/comments",
        "--input",
        "-",
    ])
}

fn question(turn: u32) -> Marker {
    Marker::Question {
        kind: QuestionKind::Park,
        blueprint: BlueprintNo::new(7).unwrap(),
        turn: NonZeroU32::new(turn).unwrap(),
    }
}

fn park() -> Marker {
    let mut p = ParkMarker::new(
        BlueprintNo::new(7).unwrap(),
        Sha256::new("a".repeat(64)).unwrap(),
    );
    p.step = MarkerWord::new("review").unwrap();
    Marker::Park(p)
}

#[test]
fn a_note_reads_the_issue_then_posts_a_200_kib_body_on_standard_input_alone() {
    let body = big_body();
    let (mut gh, calls) = adapter(vec![
        Ok(sample("issue-7.json")),
        Ok(sample("comment-posted.json")),
    ]);
    let got = gh.note(&key(7), &body, &[]).unwrap();
    // The REST id the cycle records the park question by, and the comment's
    // own link.
    assert_eq!(got.id, CommentId::new("2507654321"));
    assert_eq!(
        got.link.as_str(),
        "https://github.com/acme/widget/issues/7#issuecomment-2507654321"
    );
    let calls = taken(&calls);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0], read_7());
    assert_eq!(calls[1].args, post_7());
    kept_out_of_argv(&calls, &body);
    assert_eq!(sent_body(&calls[1]), body);
}

#[test]
fn a_notes_markers_are_written_after_its_text_and_read_back_as_given() {
    let text = big_body();
    let given = [park(), question(1)];
    let (mut gh, calls) = adapter(vec![
        Ok(sample("issue-7.json")),
        Ok(sample("comment-posted.json")),
    ]);
    gh.note(&key(7), &text, &given).unwrap();
    let calls = taken(&calls);
    let stored = sent_body(&calls[1]);
    kept_out_of_argv(&calls, &stored);
    let back = markers::read(&stored, BlueprintNo::new(7));
    assert_eq!(back.markers, given);
    assert_eq!(back.text, text);
    assert!(stored.ends_with(
        "\n\n<!-- tehas:park issue=7 step=review kind= provider=0 sig=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa -->\n<!-- tehas:question kind=park issue=7 turn=1 -->"
    ));
}

#[test]
fn a_park_question_with_its_handoff_posts_the_bytes_1x_posted() {
    // 1.x's q_render for #7's park with pi-rukas's handoff, as question.rs's
    // test a_park_question_lists_its_choices_in_words_and_both_markers has it.
    let ask = "Reply in a comment with the word of one of these:";
    let only = "Only an answer from an account that may authorise work counts.";
    let text = format!("<!-- pi-rukas:handoff issue=7 cap=x -->\nTehas cycle stopped.\n\n{ask}\n\n- retry: Retry with the issue and all its comments. A new cycle starts over in a new workspace, given what allowed accounts wrote on this issue and on its pull requests.\n- retry-text: Retry with the issue text only. A new cycle starts over in a new workspace, given the title and body and nothing else.\n- park: Leave it parked. Nothing more happens until someone answers.\n\n{only}");
    let posted_1x = format!("{text}\n\n<!-- tehas:park issue=7 step=review kind= provider=0 sig=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa -->\n<!-- tehas:question kind=park issue=7 turn=1 -->");
    let (mut gh, calls) = adapter(vec![
        Ok(sample("issue-7.json")),
        Ok(sample("comment-posted.json")),
    ]);
    gh.note(&key(7), &text, &[park(), question(1)]).unwrap();
    assert_eq!(sent_body(&taken(&calls)[1]), posted_1x);
}

#[test]
fn a_note_on_a_pull_requests_number_is_not_found_and_posts_nothing() {
    let (mut gh, calls) = adapter(vec![Ok(sample("pull-8.json"))]);
    let got = gh.note(&key(8), "b", &[Marker::HoldCleared]);
    assert_eq!(got, Err(Outcome::NotFound));
    let calls = taken(&calls);
    assert_eq!(calls.len(), 1);
    assert!(calls[0].input.is_none(), "no POST");
}

#[test]
fn the_read_before_a_note_fails_with_nothing_posted_and_says_it_was_not_sent() {
    let no_sent = |why| Outcome::Unavailable {
        why,
        sent: Sent::No,
    };
    let spent = Outcome::Spent {
        pool: Pool::Calls,
        until: At::new(now()),
    };
    let cases = [
        (failed("not-found.stderr"), Outcome::NotFound),
        (failed("gone.stderr"), Outcome::NotFound),
        (failed("forbidden.stderr"), Outcome::Forbidden),
        (failed("signed-out.stderr"), Outcome::SignedOut),
        (failed("rate-limited.stderr"), spent.clone()),
        (failed("secondary-limit.stderr"), spent),
        (failed("server-error.stderr"), no_sent(Passing::Server)),
        (BOUND.to_string(), no_sent(Passing::Timeout)),
        (NOT_STARTED.to_string(), no_sent(Passing::Timeout)),
        (failed("unreachable.stderr"), no_sent(Passing::Unreachable)),
        ("gh exited 1: HTTP 404".to_string(), Outcome::NotFound),
        ("gh exited 124".to_string(), no_sent(Passing::Timeout)),
    ];
    for (why, want) in cases {
        let (mut gh, calls) = adapter(vec![Err(why.clone())]);
        let got = gh.note(&key(7), "b", &[]).map(|_| ());
        assert_eq!(got, Err(want), "{why}");
        assert_eq!(taken(&calls).len(), 1, "{why}");
    }
}

/// `note` of a 200 KiB body with markers, its read answered and its POST
/// given `post`: what it answered and the calls it made.
fn note_against(post: Result<String, String>) -> (Got<Posted<CommentId>>, Vec<Call>) {
    let (mut gh, calls) = adapter(vec![Ok(sample("issue-7.json")), post]);
    let got = gh.note(&key(7), &big_body(), &[park(), question(1)]);
    (got, taken(&calls))
}

#[test]
fn every_failure_of_the_post_is_its_named_outcome_sent_once_and_echoes_no_input() {
    let maybe = |why| Outcome::Unavailable {
        why,
        sent: Sent::Maybe,
    };
    let spent = Outcome::Spent {
        pool: Pool::Writes,
        until: At::new(now()),
    };
    let no_sent = |why| Outcome::Unavailable {
        why,
        sent: Sent::No,
    };
    let cases = [
        (failed("not-found.stderr"), Outcome::NotFound),
        (failed("gone.stderr"), Outcome::NotFound),
        (failed("forbidden.stderr"), Outcome::Forbidden),
        (failed("signed-out.stderr"), Outcome::SignedOut),
        (failed("secondary-limit.stderr"), spent.clone()),
        (failed("rate-limited.stderr"), spent.clone()),
        (
            failed("validation.stderr"),
            Outcome::Refused(Refusal::Invalid),
        ),
        // A note the adapter cannot settle: it may be there, and stays so.
        (failed("server-error.stderr"), maybe(Passing::Server)),
        (BOUND.to_string(), maybe(Passing::Timeout)),
        (NOT_STARTED.to_string(), no_sent(Passing::Timeout)),
        (failed("unreachable.stderr"), no_sent(Passing::Unreachable)),
        // The relay's runner's reasons, gh's words after its exit status.
        ("gh exited 1: HTTP 404".to_string(), Outcome::NotFound),
        ("gh exited 1: HTTP 410".to_string(), Outcome::NotFound),
        ("gh exited 1: HTTP 403".to_string(), Outcome::Forbidden),
        ("gh exited 1: HTTP 401".to_string(), Outcome::SignedOut),
        ("gh exited 1: HTTP 429".to_string(), spent),
        (
            "gh exited 1: HTTP 422".to_string(),
            Outcome::Refused(Refusal::Invalid),
        ),
        ("gh exited 1: HTTP 503".to_string(), maybe(Passing::Server)),
        ("gh exited 124".to_string(), maybe(Passing::Timeout)),
    ];
    let body = big_body();
    for (why, want) in cases {
        let (got, calls) = note_against(Err(why.clone()));
        assert_eq!(got, Err(want), "{why}");
        // Sent once, and nothing read or sent after it.
        assert_eq!(calls.len(), 2, "{why}");
        assert_eq!(calls.iter().filter(|c| c.input.is_some()).count(), 1);
        let said = format!("{got:?} {}", got.as_ref().unwrap_err().line());
        assert!(!said.contains(&body[..64]), "{why}: {said}");
        assert!(!said.contains("tehas:"), "{why}: {said}");
    }
}

#[test]
fn an_answer_that_does_not_read_is_unread_never_a_success() {
    let posted = sample("comment-posted.json");
    let cut = posted[..posted.len() / 2].to_string();
    let mut no_id: Value = serde_json::from_str(&posted).unwrap();
    no_id.as_object_mut().unwrap().remove("id");
    for answer in [
        "not json".to_string(),
        String::new(),
        "{}".to_string(),
        cut,
        no_id.to_string(),
    ] {
        let (got, _) = note_against(Ok(answer.clone()));
        assert_eq!(got, Err(Outcome::Unread(Unread::Malformed)), "{answer:.40}");
    }
    // The read before it, likewise: nothing is posted.
    for answer in ["not json", "{}", "{\"number\":7}"] {
        let (mut gh, calls) = adapter(vec![Ok(answer.to_string())]);
        assert_eq!(
            gh.note(&key(7), "b", &[]),
            Err(Outcome::Unread(Unread::Malformed)),
            "{answer}"
        );
        assert_eq!(taken(&calls).len(), 1);
    }
}

#[test]
fn an_answer_for_another_repository_or_issue_is_inconsistent() {
    for (from, to) in [
        ("acme/widget/issues/7#", "acme/other/issues/7#"),
        ("issues/7#", "issues/9#"),
        ("issues/7#", "pull/7#"),
        ("#issuecomment-2507654321", "#issuecomment-1"),
    ] {
        let answer = sample("comment-posted.json").replace(from, to);
        let (got, _) = note_against(Ok(answer));
        assert_eq!(got, Err(Outcome::Unread(Unread::Inconsistent)), "{to}");
    }
    // The read answered another repository's issue, or another number.
    for (from, to) in [
        (
            "github.com/acme/widget/issues/7",
            "github.com/acme/other/issues/7",
        ),
        ("\"number\":7", "\"number\":9"),
    ] {
        let read = sample("issue-7.json").replace(from, to);
        let (mut gh, calls) = adapter(vec![Ok(read)]);
        assert_eq!(
            gh.note(&key(7), "b", &[]),
            Err(Outcome::Unread(Unread::Inconsistent)),
            "{to}"
        );
        assert_eq!(but_the_name(&taken(&calls)), 1, "nothing posted");
    }
    // A project named in another case is the same project.
    let answer = sample("comment-posted.json").replace("acme/widget", "Acme/Widget");
    assert!(note_against(Ok(answer)).0.is_ok());
}

/// GitHub's answer for the repository the configured name resolves to:
/// its database id and the name it has now.
const RENAMED: &str =
    r#"{"data":{"repository":{"databaseId":7001,"nameWithOwner":"acme/widget"}}}"#;

/// An instance that still names the project's old name writes it as here:
/// the name is asked once, with the repository's id, and every link after
/// is read against it.
#[test]
fn a_renamed_project_is_still_written_as_here_by_its_repository_id() {
    let (mut gh, calls) = adapter_of(
        "acme/widget-before",
        vec![
            Ok(sample("issue-7.json")),
            Ok(RENAMED.into()),
            Ok(sample("comment-posted.json")),
        ],
    );
    let got = gh.note(&key(7), "b", &[]).unwrap();
    assert_eq!(got.id, CommentId::new("2507654321"));
    let calls = taken(&calls);
    assert_eq!(calls.len(), 3, "the name is asked once");
    assert_eq!(calls[1].args[..2], args(&["api", "graphql"]));
    assert!(calls[1].args.contains(&"owner=acme".to_string()));
    assert!(calls[1].args.contains(&"name=widget-before".to_string()));
    assert!(calls[2].input.is_some(), "then the note is posted");
}

/// A link to another repository is not here, whatever name was learned;
/// with no name learned, the answer stays unread.
#[test]
fn another_repositorys_link_is_not_here_for_a_renamed_project() {
    let read = sample("issue-7.json").replace(
        "github.com/acme/widget/issues/7",
        "github.com/acme/other/issues/7",
    );
    let (mut gh, calls) = adapter_of("acme/widget-before", vec![Ok(read), Ok(RENAMED.into())]);
    assert_eq!(
        gh.note(&key(7), "b", &[]),
        Err(Outcome::Unread(Unread::Inconsistent))
    );
    assert!(
        taken(&calls).iter().all(|c| c.input.is_none()),
        "nothing posted"
    );
    let no_name = RENAMED.replace(r#","nameWithOwner":"acme/widget""#, "");
    for name in [Ok(no_name), Err("no answer".to_string())] {
        let (mut gh, calls) =
            adapter_of("acme/widget-before", vec![Ok(sample("issue-7.json")), name]);
        assert_eq!(
            gh.note(&key(7), "b", &[]),
            Err(Outcome::Unread(Unread::Inconsistent))
        );
        assert_eq!(taken(&calls).len(), 2, "nothing posted");
    }
}

/// A repository moved to another owner is not here, even where GitHub
/// gives its new name with this project's id and every link names it: the
/// account that now owns it is not the configured one.
#[test]
fn a_project_transferred_to_another_owner_is_not_here() {
    let moved = |s: String| s.replace("acme/widget", "mallory/widget");
    let (mut gh, calls) = adapter(vec![
        Ok(moved(sample("issue-7.json"))),
        Ok(moved(RENAMED.into())),
        Ok(moved(sample("comment-posted.json"))),
    ]);
    assert_eq!(
        gh.note(&key(7), "b", &[]),
        Err(Outcome::Unread(Unread::Inconsistent))
    );
    let calls = taken(&calls);
    assert_eq!(calls.len(), 2, "nothing posted");
    assert_eq!(calls[1].args[..2], args(&["api", "graphql"]));
    // A rename within the owner, named in another case, is still here.
    let (mut gh, _) = adapter_of(
        "ACME/widget-before",
        vec![
            Ok(sample("issue-7.json")),
            Ok(RENAMED.into()),
            Ok(sample("comment-posted.json")),
        ],
    );
    assert!(gh.note(&key(7), "b", &[]).is_ok());
}

/// A runner of the trait's default, which takes no input.
struct NoInput {
    calls: Calls,
}

impl GhRunner for NoInput {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        self.calls.lock().unwrap().push(Call {
            args: args.iter().map(|a| a.to_string()).collect(),
            input: None,
        });
        Ok(sample("issue-7.json"))
    }
}

#[test]
fn a_runner_that_takes_no_input_posts_nothing_and_never_puts_the_text_in_an_argument() {
    let calls = Calls::default();
    let runner = NoInput {
        calls: Arc::clone(&calls),
    };
    let mut gh = GitHub::new(PROJECT, Box::new(runner));
    let body = big_body();
    assert_eq!(
        gh.note(&key(7), &body, &[]).map(|_| ()),
        Err(Outcome::Refused(Refusal::Unsupported))
    );
    let calls = taken(&calls);
    assert_eq!(calls, [read_7()]);
    kept_out_of_argv(&calls, &body);
}

#[test]
fn what_the_guards_refuse_is_refused_before_any_request() {
    let invalid = Err(Outcome::Refused(Refusal::Invalid));
    let (mut gh, calls) = adapter(Vec::new());
    // A text that holds a marker, wrapped or bare, of this item or another.
    for text in [
        "Done.\n<!-- tehas:hold-cleared -->",
        "<!-- tehas:millwright issue=8 action=retry -->",
        "<!-- tehas:nonsense -->",
        "tehas:blocked by=12",
    ] {
        assert_eq!(gh.note(&key(7), text, &[]).map(|_| ()), invalid, "{text}");
    }
    // A marker that is not writable, or of another item.
    for m in [
        Marker::Unknown,
        Marker::Blocked { by: Vec::new() },
        Marker::Question {
            kind: QuestionKind::Park,
            blueprint: BlueprintNo::new(8).unwrap(),
            turn: NonZeroU32::new(1).unwrap(),
        },
    ] {
        assert_eq!(gh.note(&key(7), "b", &[m]).map(|_| ()), invalid);
    }
    // A ticket's key is no issue of GitHub's.
    let k = TicketKey::Key(KeyText::new("PROJ-1").unwrap());
    assert_eq!(gh.note(&k, "b", &[]).map(|_| ()), invalid);
    // A project's own marker has no words in the table.
    let own = Marker::Project {
        name: MarkName::new("acme-hold").unwrap(),
        key: None,
        fields: Vec::new(),
    };
    assert_eq!(
        gh.note(&key(7), "b", &[own]).map(|_| ()),
        Err(Outcome::Refused(Refusal::Unsupported))
    );
    assert!(taken(&calls).is_empty());
}

/// A note's text with our wrapper in a code span: no body carries a cited
/// marker (the spec's Decided 83), since 1.x's readers find one in code.
const CITED: &str = "See `<!-- tehas:millwright issue=7 action=finish -->` here";
/// A note's text with our wrapper in a quote, which 1.x's readers find too.
const QUOTED: &str = "> <!-- tehas:blocked by=3 -->";

#[test]
fn a_note_with_a_cited_marker_is_refused_and_posts_nothing() {
    let (mut gh, calls) = adapter(Vec::new());
    assert_eq!(
        gh.note(&key(7), CITED, &[]).map(|_| ()),
        Err(Outcome::Refused(Refusal::Invalid))
    );
    assert!(taken(&calls).is_empty());
}

#[test]
fn a_note_with_a_quoted_marker_is_refused_and_posts_nothing() {
    let (mut gh, calls) = adapter(Vec::new());
    assert_eq!(
        gh.note(&key(7), QUOTED, &[question(1)]).map(|_| ()),
        Err(Outcome::Refused(Refusal::Invalid))
    );
    assert!(taken(&calls).is_empty());
}

#[test]
fn another_tools_marker_line_is_text_and_is_posted() {
    let text = "<!-- pi-rukas:handoff issue=7 cap=budget -->\nTehas cycle stopped.";
    let (mut gh, calls) = adapter(vec![
        Ok(sample("issue-7.json")),
        Ok(sample("comment-posted.json")),
    ]);
    gh.note(&key(7), text, &[]).unwrap();
    assert_eq!(sent_body(&taken(&calls)[1]), text);
}

#[test]
fn a_repository_that_is_no_owner_and_name_gets_no_note() {
    for repo in [
        "acme",
        "acme/widget/issues",
        "acme/..",
        "../widget",
        "acme/wid get",
        "acme/w?x=1",
        "/widget",
    ] {
        let (mut gh, calls) = adapter_of(repo, Vec::new());
        assert_eq!(
            gh.note(&key(7), "b", &[]).map(|_| ()),
            Err(Outcome::Refused(Refusal::Invalid)),
            "{repo}"
        );
        assert!(taken(&calls).is_empty(), "{repo}");
    }
}

/// The issue's sample with its title and body set, and its time `updated`.
fn issue_7(title: &str, body: &str, updated: &str) -> String {
    let mut v: Value = serde_json::from_str(&sample("issue-7.json")).unwrap();
    v["title"] = title.into();
    v["body"] = body.into();
    v["updated_at"] = updated.into();
    v.to_string()
}

/// An issue's revision as the blueprints port's writes read it
/// (`GitHub::text_revision`, the sum of its title and body): the one rule of
/// both ports.
fn rev_of(answer: &str) -> Revision {
    let (mut gh, _) = adapter(vec![Ok(answer.to_string())]);
    Revision::new(gh.text_revision(7).unwrap())
}

/// The revision of the issue's sample.
fn since() -> Revision {
    rev_of(&sample("issue-7.json"))
}

const TITLE: &str = "Plan: the title only standard input carries";
const EDITED: &str = "2026-11-23T10:20:00Z";
const BASE: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// What an edit sent on standard input: nothing but these fields.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Edited {
    title: Option<String>,
    body: Option<String>,
}

fn edited(c: &Call) -> Edited {
    serde_json::from_str(c.input.as_deref().unwrap()).unwrap()
}

fn patch_7() -> Vec<String> {
    args(&[
        "api",
        "-X",
        "PATCH",
        "repos/acme/widget/issues/7",
        "--input",
        "-",
    ])
}

fn blocked(n: u64) -> Marker {
    Marker::Blocked {
        by: vec![Ref::Here(key(n))],
    }
}

fn last_attempt(note: &str) -> Marker {
    let mut l = LastAttemptMarker::new(Sha256::new(BASE).unwrap());
    l.note = note.to_string();
    Marker::LastAttempt(l)
}

fn set(m: Marker) -> MarkerEdit {
    MarkerEdit {
        set: vec![m],
        drop: Vec::new(),
    }
}

/// The body an edit of #7, whose body is `body`, sends for `text` and
/// `markers`; None when it sends no PATCH. The read is the only other call.
fn edit_of(body: &str, text: Option<&str>, markers: &MarkerEdit) -> Got<Option<String>> {
    let read = issue_7("Plan", body, "2026-11-22T14:30:00Z");
    let at = rev_of(&read);
    let (mut gh, calls) = adapter(vec![Ok(read), Ok(issue_7("Plan", "x", EDITED))]);
    gh.edit(&key(7), None, text, markers, &at)?;
    let calls = taken(&calls);
    Ok(match calls.len() {
        1 => None,
        _ => Some(edited(&calls[1]).body.unwrap()),
    })
}

#[test]
fn an_edit_reads_the_issue_then_sends_a_200_kib_text_and_its_title_on_standard_input_alone() {
    let body = big_body();
    let (mut gh, calls) = adapter(vec![
        Ok(sample("issue-7.json")),
        Ok(issue_7(TITLE, &body, EDITED)),
    ]);
    let got = gh
        .edit(
            &key(7),
            Some(TITLE),
            Some(&body),
            &MarkerEdit::default(),
            &since(),
        )
        .unwrap();
    assert_eq!(got.as_str(), "https://github.com/acme/widget/issues/7");
    let calls = taken(&calls);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0], read_7());
    assert_eq!(calls[1].args, patch_7());
    kept_out_of_argv(&calls, &body);
    kept_out_of_argv(&calls, TITLE);
    assert_eq!(
        edited(&calls[1]),
        Edited {
            title: Some(TITLE.into()),
            body: Some(body)
        }
    );
}

#[test]
fn the_revision_a_read_gives_is_the_one_an_edit_is_held_to() {
    let (mut gh, _) = adapter(vec![
        Ok(sample("issue-7.json")),
        Ok(r#"{"data":{"repository":{"issue":{"number":7,"parent":null}}}}"#.to_string()),
        Ok(sample("issue-7.json")),
        Ok(issue_7("t", "New.", EDITED)),
    ]);
    let read = gh.several(&[key(7)]).pop().unwrap().1.unwrap();
    assert_eq!(read.revision, since());
    let got = gh.edit(
        &key(7),
        None,
        Some("New."),
        &MarkerEdit::default(),
        &read.revision,
    );
    assert!(got.is_ok(), "{got:?}");
}

#[test]
fn a_tickets_revision_is_the_blueprint_writes_revision_of_the_same_answer() {
    // Blueprints::blueprint is not answered yet: blueprint_writes' revision
    // is the same function, which GitHub::text_revision gives.
    for body in [
        "",
        "Plain.",
        "With <!-- tehas:blocked by=3 --> a marker.\r\n",
    ] {
        let answer = issue_7("A title", body, "2026-11-22T14:30:00Z");
        let (mut gh, _) = adapter(vec![
            Ok(answer.clone()),
            Ok(r#"{"data":{"repository":{"issue":{"number":7,"parent":null}}}}"#.to_string()),
        ]);
        let digest = gh.several(&[key(7)]).pop().unwrap().1.unwrap();
        assert_eq!(digest.revision, rev_of(&answer), "{body:?}");
        assert_eq!(digest.updated, At::new(1_795_357_800));
    }
}

#[test]
fn a_label_a_comment_or_a_new_time_between_the_read_and_the_edit_is_no_conflict() {
    let read = sample("issue-7.json");
    let mut v: Value = serde_json::from_str(&read).unwrap();
    v["labels"] = serde_json::json!([]);
    v["comments"] = 3.into();
    v["updated_at"] = "2026-11-23T11:00:00Z".into();
    let (mut gh, calls) = adapter(vec![Ok(v.to_string()), Ok(read)]);
    let got = gh.edit(&key(7), Some("x"), None, &MarkerEdit::default(), &since());
    assert!(got.is_ok(), "{got:?}");
    assert_eq!(taken(&calls).len(), 2);
}

#[test]
fn a_title_body_or_marker_change_since_the_read_is_a_conflict_and_sends_nothing() {
    let v: Value = serde_json::from_str(&sample("issue-7.json")).unwrap();
    let title = v["title"].as_str().unwrap();
    let body = v["body"].as_str().unwrap();
    for changed in [
        issue_7("Retitled", body, "2026-11-22T14:30:00Z"),
        issue_7(title, "Another account's body.", "2026-11-22T14:30:00Z"),
        // A marker alone added to the body moves it too.
        issue_7(
            title,
            &format!("{body}\n\n<!-- tehas:hold-cleared -->"),
            "2026-11-22T14:30:00Z",
        ),
    ] {
        let (mut gh, calls) = adapter(vec![Ok(changed)]);
        let got = gh.edit(&key(7), Some("x"), None, &MarkerEdit::default(), &since());
        assert_eq!(got, Err(Outcome::Conflict(Lost::Changed)));
        assert_eq!(taken(&calls).len(), 1, "only the read");
    }
    // An answer with no title has no revision: unread, not a conflict.
    let mut v = v;
    v.as_object_mut().unwrap().remove("title");
    let (mut gh, calls) = adapter(vec![Ok(v.to_string())]);
    let got = gh.edit(&key(7), Some("x"), None, &MarkerEdit::default(), &since());
    assert_eq!(got, Err(Outcome::Unread(Unread::Malformed)));
    assert_eq!(taken(&calls).len(), 1);
}

#[test]
fn an_edit_keeps_every_other_byte_and_every_marker_it_is_not_told_to_change_where_it_was() {
    // A new text in place of the old, its CRs and the markers around it
    // kept.
    let body =
        "<!-- tehas:finish issue=7 result=opened -->\nOld text.\r\n\r\n<!-- tehas:blocked by=3 -->";
    assert_eq!(
        edit_of(body, Some("New text."), &MarkerEdit::default()).unwrap(),
        Some("<!-- tehas:finish issue=7 result=opened -->\nNew text.\r\n\r\n<!-- tehas:blocked by=3 -->".into())
    );
    // A marker set in place of one of its kind, where it stood.
    let body = "Do it.\n\n<!-- tehas:blocked by=3 -->\nMore text.\r\n";
    assert_eq!(
        edit_of(body, None, &set(blocked(5))).unwrap(),
        Some("Do it.\n\n<!-- tehas:blocked by=5 -->\nMore text.\r\n".into())
    );
    // A marker added after the text, the markers there kept in place.
    let body = "Do it.\n\n<!-- tehas:blocked by=3 -->\nMore.\n\n<!-- tehas:hold-cleared -->\n";
    assert_eq!(
        edit_of(body, None, &set(last_attempt("Note."))).unwrap(),
        Some(format!("Do it.\n\n<!-- tehas:blocked by=3 -->\nMore.\n\n<!-- tehas:hold-cleared -->\n<!-- tehas:last-attempt base={BASE} -->\nNote.\n<!-- /tehas:last-attempt -->"))
    );
}

#[test]
fn a_marker_dropped_takes_its_line_or_its_words_and_one_space() {
    let drop = |kind| MarkerEdit {
        set: Vec::new(),
        drop: vec![(kind, None)],
    };
    assert_eq!(
        edit_of(
            "A.\n<!-- tehas:blocked by=3 -->\nB.",
            None,
            &drop(MarkerKind::Blocked)
        )
        .unwrap(),
        Some("A.\nB.".into())
    );
    assert_eq!(
        edit_of(
            "A. <!-- tehas:blocked by=3 --> B.",
            None,
            &drop(MarkerKind::Blocked)
        )
        .unwrap(),
        Some("A. B.".into())
    );
    assert_eq!(
        edit_of(
            "A.\r\n\r\n<!-- tehas:hold-cleared -->",
            None,
            &drop(MarkerKind::HoldCleared)
        )
        .unwrap(),
        Some("A.\r\n".into())
    );
}

#[test]
fn a_last_attempts_edit_writes_the_bytes_1x_wrote() {
    // console/cycle/src/last_attempt.rs line 96 (decide) and line 214
    // (note): the body without white space at its end, a blank line, the
    // open line, the note, the close line.
    let note = "### The last attempt ran out of time\n\nThe last cycle on this text was stopped in develop: the developer ran past its time bound. Its work was not pushed, and the next cycle starts from a clean checkout.\n\nPlan a smaller change: finish one part well, or say in this issue what to leave out. This section is the factory's; edit or delete it as you like.";
    let written_1x = format!(
        "Fix it.\n\n<!-- tehas:last-attempt base={BASE} -->\n{note}\n<!-- /tehas:last-attempt -->"
    );
    assert_eq!(
        edit_of("Fix it.\r\n\r\n", None, &set(last_attempt(note))).unwrap(),
        Some(written_1x)
    );
}

#[test]
fn a_last_attempt_set_goes_at_the_end_in_place_of_an_older_one_and_keeps_the_text() {
    let body = format!(
        "Fix it.\r\n\r\n- one\r\n- two\n\n<!-- tehas:last-attempt base={BASE} -->\nOld note.\n<!-- /tehas:last-attempt -->\n<!-- tehas:blocked by=3 -->"
    );
    assert_eq!(
        edit_of(&body, None, &set(last_attempt("New note."))).unwrap(),
        Some(format!(
            "Fix it.\r\n\r\n- one\r\n- two\n\n<!-- tehas:blocked by=3 -->\n<!-- tehas:last-attempt base={BASE} -->\nNew note.\n<!-- /tehas:last-attempt -->"
        ))
    );
}

#[test]
fn markers_that_do_not_read_and_another_items_markers_stay_as_they_are() {
    let la = set(last_attempt("Note."));
    let block =
        format!("<!-- tehas:last-attempt base={BASE} -->\nNote.\n<!-- /tehas:last-attempt -->");
    // After a line that reads as a marker of ours, the block follows on the
    // next line; after text, a blank line comes between.
    for (body, between) in [
        ("Text.\n\n<!-- tehas:blocked by=03 -->", "\n"),
        ("Text.\n\n<!-- tehas:nonsense -->", "\n"),
        ("Text.\n\ntehas:blocked by=3", "\n\n"),
        (
            "Split from #8.\n\n<!-- tehas:millwright issue=8 action=retry -->",
            "\n\n",
        ),
        ("See `<!-- tehas:hold-cleared -->` in a code span.", "\n\n"),
    ] {
        let got = edit_of(body, None, &la).unwrap();
        assert_eq!(got, Some(format!("{body}{between}{block}")), "{body}");
    }
}

#[test]
fn an_edit_that_changes_nothing_sends_no_patch() {
    let drop = MarkerEdit {
        set: Vec::new(),
        drop: vec![(MarkerKind::HoldCleared, None)],
    };
    assert_eq!(
        edit_of("<!-- tehas:blocked by=3 -->\nText\n", None, &drop),
        Ok(None)
    );
    assert_eq!(
        edit_of(
            "Text.\n\n<!-- tehas:blocked by=3 -->",
            None,
            &set(blocked(3))
        ),
        Ok(None)
    );
    // A title equal to the one there, alone: the read and nothing else.
    let v: Value = serde_json::from_str(&sample("issue-7.json")).unwrap();
    let (mut gh, calls) = adapter(vec![Ok(sample("issue-7.json"))]);
    let got = gh.edit(
        &key(7),
        v["title"].as_str(),
        None,
        &MarkerEdit::default(),
        &since(),
    );
    assert_eq!(
        got.unwrap().as_str(),
        "https://github.com/acme/widget/issues/7"
    );
    assert_eq!(taken(&calls).len(), 1);
}

#[test]
fn an_edit_with_nothing_to_change_reads_and_writes_nothing() {
    let (mut gh, calls) = adapter(vec![Ok(sample("issue-7.json"))]);
    let got = gh.edit(&key(7), None, None, &MarkerEdit::default(), &since());
    assert_eq!(
        got.unwrap().as_str(),
        "https://github.com/acme/widget/issues/7"
    );
    assert_eq!(taken(&calls).len(), 1);
}

#[test]
fn a_marker_to_change_is_found_where_the_reader_finds_it_not_by_its_words() {
    // A copy of its words in a code span is text, and stays.
    let body = "Do `<!-- tehas:blocked by=3 -->` this.\n\n<!-- tehas:blocked by=3 -->";
    assert_eq!(
        edit_of(body, None, &set(blocked(5))),
        Ok(Some(
            "Do `<!-- tehas:blocked by=3 -->` this.\n\n<!-- tehas:blocked by=5 -->".into()
        ))
    );
}

#[test]
fn an_edit_of_a_pull_request_is_not_found_and_sends_nothing() {
    let read = sample("pull-8.json");
    let (mut gh, calls) = adapter(vec![Ok(read.clone())]);
    let got = gh.edit(
        &key(8),
        Some("x"),
        None,
        &MarkerEdit::default(),
        &rev_of(&read),
    );
    assert_eq!(got, Err(Outcome::NotFound));
    assert_eq!(taken(&calls).len(), 1);
}

#[test]
fn an_edit_with_a_cited_marker_is_refused_before_any_request() {
    let (mut gh, calls) = adapter(Vec::new());
    let none = MarkerEdit::default();
    let invalid = Err(Outcome::Refused(Refusal::Invalid));
    assert_eq!(
        gh.edit(&key(7), None, Some(CITED), &none, &since()),
        invalid
    );
    assert_eq!(
        gh.edit(&key(7), Some(CITED), None, &none, &since()),
        invalid
    );
    assert!(taken(&calls).is_empty());
}

#[test]
fn an_edit_with_a_quoted_marker_is_refused_before_any_request() {
    let (mut gh, calls) = adapter(Vec::new());
    let got = gh.edit(
        &key(7),
        None,
        Some(QUOTED),
        &MarkerEdit::default(),
        &since(),
    );
    assert_eq!(got, Err(Outcome::Refused(Refusal::Invalid)));
    assert!(taken(&calls).is_empty());
}

#[test]
fn what_an_edits_guards_refuse_is_refused_before_any_request() {
    let invalid = Err(Outcome::Refused(Refusal::Invalid));
    let (mut gh, calls) = adapter(Vec::new());
    let none = MarkerEdit::default();
    let s = since();
    assert_eq!(
        gh.edit(&key(7), None, None, &set(Marker::Unknown), &s),
        invalid
    );
    let empty = Marker::Blocked { by: Vec::new() };
    assert_eq!(gh.edit(&key(7), None, None, &set(empty), &s), invalid);
    let marked = "Text <!-- tehas:hold-cleared --> here";
    assert_eq!(gh.edit(&key(7), None, Some(marked), &none, &s), invalid);
    assert_eq!(gh.edit(&key(7), Some(marked), None, &none, &s), invalid);
    assert_eq!(
        gh.edit(&key(7), None, Some("tehas:blocked by=3"), &none, &s),
        invalid
    );
    let k = TicketKey::Key(KeyText::new("PROJ-1").unwrap());
    assert_eq!(gh.edit(&k, Some("t"), None, &none, &s), invalid);
    assert!(taken(&calls).is_empty());
    // A marker of another item reads nowhere: refused after the read, with
    // nothing sent.
    let other = Marker::Question {
        kind: QuestionKind::Park,
        blueprint: BlueprintNo::new(8).unwrap(),
        turn: NonZeroU32::new(1).unwrap(),
    };
    assert_eq!(
        edit_of("Text.", None, &set(other)),
        Err(Outcome::Refused(Refusal::Unsupported))
    );
}

#[test]
fn the_read_before_an_edit_fails_with_nothing_sent() {
    let no_sent = |why| Outcome::Unavailable {
        why,
        sent: Sent::No,
    };
    for (why, want) in [
        (failed("not-found.stderr"), Outcome::NotFound),
        (failed("forbidden.stderr"), Outcome::Forbidden),
        (failed("signed-out.stderr"), Outcome::SignedOut),
        (failed("server-error.stderr"), no_sent(Passing::Server)),
        (BOUND.to_string(), no_sent(Passing::Timeout)),
        (failed("unreachable.stderr"), no_sent(Passing::Unreachable)),
    ] {
        let (mut gh, calls) = adapter(vec![Err(why.clone())]);
        let got = gh.edit(
            &key(7),
            Some("t"),
            Some("b"),
            &MarkerEdit::default(),
            &since(),
        );
        assert_eq!(got, Err(want), "{why}");
        assert_eq!(taken(&calls).len(), 1, "{why}");
    }
}

/// `edit` of a 200 KiB text and a title, its read answered, its PATCH
/// given `patch`, and a read after it given `after` when there is one.
fn edit_against(
    patch: Result<String, String>,
    after: Option<Result<String, String>>,
) -> (Got<WebLink>, Vec<Call>) {
    let mut answers = vec![Ok(sample("issue-7.json")), patch];
    answers.extend(after);
    let (mut gh, calls) = adapter(answers);
    let got = gh.edit(
        &key(7),
        Some(TITLE),
        Some(&big_body()),
        &MarkerEdit::default(),
        &since(),
    );
    (got, taken(&calls))
}

#[test]
fn every_failure_of_the_edit_is_its_named_outcome_sent_once_and_echoes_no_input() {
    let maybe = |why| Outcome::Unavailable {
        why,
        sent: Sent::Maybe,
    };
    let spent = Outcome::Spent {
        pool: Pool::Writes,
        until: At::new(now()),
    };
    let cases = [
        (failed("not-found.stderr"), Outcome::NotFound),
        (failed("gone.stderr"), Outcome::NotFound),
        (failed("forbidden.stderr"), Outcome::Forbidden),
        (failed("signed-out.stderr"), Outcome::SignedOut),
        (failed("secondary-limit.stderr"), spent.clone()),
        (failed("rate-limited.stderr"), spent),
        (
            failed("validation.stderr"),
            Outcome::Refused(Refusal::Invalid),
        ),
        (failed("server-error.stderr"), maybe(Passing::Server)),
        (BOUND.to_string(), maybe(Passing::Timeout)),
        (
            failed("unreachable.stderr"),
            Outcome::Unavailable {
                why: Passing::Unreachable,
                sent: Sent::No,
            },
        ),
    ];
    let body = big_body();
    for (why, want) in cases {
        // The read that would settle a Maybe fails too.
        let (got, calls) = edit_against(Err(why.clone()), None);
        assert_eq!(got, Err(want), "{why}");
        assert_eq!(calls.iter().filter(|c| c.input.is_some()).count(), 1);
        let said = format!("{got:?} {}", got.as_ref().unwrap_err().line());
        assert!(
            !said.contains(&body[..64]) && !said.contains(TITLE),
            "{said}"
        );
    }
}

#[test]
fn an_edit_that_may_have_landed_is_settled_by_one_read() {
    let body = big_body();
    let timeout = Err(BOUND.to_string());
    // The read shows what was sent: done.
    let (got, calls) = edit_against(timeout.clone(), Some(Ok(issue_7(TITLE, &body, EDITED))));
    assert_eq!(
        got.unwrap().as_str(),
        "https://github.com/acme/widget/issues/7"
    );
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[2], read_7());
    // Still at its revision: it was not sent.
    let (got, calls) = edit_against(timeout.clone(), Some(Ok(sample("issue-7.json"))));
    assert_eq!(
        got,
        Err(Outcome::Unavailable {
            why: Passing::Timeout,
            sent: Sent::No
        })
    );
    assert_eq!(calls.iter().filter(|c| c.input.is_some()).count(), 1);
    // Changed, and not to what was sent: it stays unknown.
    let (got, _) = edit_against(
        Err(failed("server-error.stderr")),
        Some(Ok(issue_7("Another title", "Another body.", EDITED))),
    );
    assert_eq!(
        got,
        Err(Outcome::Unavailable {
            why: Passing::Server,
            sent: Sent::Maybe
        })
    );
}

#[test]
fn an_edits_answer_that_does_not_read_or_names_another_issue_is_never_a_success() {
    for answer in ["not json", "", "{}"] {
        let (got, _) = edit_against(Ok(answer.to_string()), None);
        assert_eq!(got, Err(Outcome::Unread(Unread::Malformed)), "{answer}");
    }
    let other = issue_7(TITLE, "b", EDITED)
        .replace("\"number\":7", "\"number\":9")
        .replace("widget/issues/7", "widget/issues/9");
    let (got, _) = edit_against(Ok(other), None);
    assert_eq!(got, Err(Outcome::Unread(Unread::Inconsistent)));
    let elsewhere = issue_7(TITLE, "b", EDITED).replace("acme/widget/issues", "acme/other/issues");
    let (got, _) = edit_against(Ok(elsewhere), None);
    assert_eq!(got, Err(Outcome::Unread(Unread::Inconsistent)));
}

#[test]
fn a_runner_that_takes_no_input_edits_nothing() {
    let calls = Calls::default();
    let runner = NoInput {
        calls: Arc::clone(&calls),
    };
    let mut gh = GitHub::new(PROJECT, Box::new(runner));
    let got = gh.edit(&key(7), Some(TITLE), None, &MarkerEdit::default(), &since());
    assert_eq!(got, Err(Outcome::Refused(Refusal::Unsupported)));
    assert_eq!(taken(&calls), [read_7()]);
}

// The layouts the cycle's writes need, byte for byte: the store keeps the
// sum of the whole body a question posts.

const PARK_LINE: &str = "<!-- tehas:park issue=7 step=review kind= provider=0 sig=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa -->";

/// The body `note` posts for `text` and `markers` on #7.
fn posted(text: &str, markers: &[Marker]) -> String {
    let (mut gh, calls) = adapter(vec![
        Ok(sample("issue-7.json")),
        Ok(sample("comment-posted.json")),
    ]);
    gh.note(&key(7), text, markers).unwrap();
    sent_body(&taken(&calls)[1])
}

#[test]
fn a_park_question_posts_exactly_the_body_core_s_q_render_makes() {
    use tehas_core::question::{q_parts, q_render, Kind};
    let Marker::Park(p) = park() else {
        unreachable!()
    };
    for (marker, line) in [
        ("<!-- pi-rukas:handoff issue=7 cap=x -->", ""),
        ("", "The host's own line."),
    ] {
        let out = |park: &str| {
            serde_json::json!({"text": "Tehas cycle stopped.\n\nIt ran out of time.", "question": null,
                "marker": marker, "park": park, "retry": true, "comments": true})
        };
        let with_park = q_render(Kind::Park, 7, 1, &out(PARK_LINE), line);
        let without = q_render(Kind::Park, 7, 1, &out(""), line);
        let (text, markers) = q_parts(&without, Kind::Park, 7, 1, Some(p.clone())).unwrap();
        assert_eq!(markers, [park(), question(1)]);
        assert_eq!(posted(&text, &markers), with_park);
    }
}

#[test]
fn a_planning_question_posts_exactly_the_body_core_s_q_render_makes() {
    use tehas_core::question::{q_parts, q_render, Kind};
    let out = serde_json::json!({"text": "- [ ] a plan", "question": {"question": "Which?",
        "options": [{"label": "Adopt all", "description": "Apply all five"}, {"label": "Change some"}]}});
    let rendered = q_render(Kind::Plan, 7, 2, &out, "");
    let (text, markers) = q_parts(&rendered, Kind::Plan, 7, 2, None).unwrap();
    assert_eq!(
        markers,
        [Marker::Question {
            kind: QuestionKind::Plan,
            blueprint: BlueprintNo::new(7).unwrap(),
            turn: NonZeroU32::new(2).unwrap(),
        }]
    );
    assert_eq!(posted(&text, &markers), rendered);
}

#[test]
fn a_parks_comment_is_its_words_a_blank_line_and_the_park_line() {
    // console/cycle/src/report.rs Park::comment: the handoff's marker, the
    // words (q_clean, no line end at their end), then "\n\n" and the park's
    // marker.
    let words = tehas_core::question::q_clean(
        "Tehas cycle stopped. To retry, apply `tehas:ready`.\n\nUsage: 3 calls.\n",
        60000,
    );
    let text = format!("<!-- pi-rukas:handoff issue=7 cap=x -->\n{words}");
    assert_eq!(
        posted(&text, &[park()]),
        format!("<!-- pi-rukas:handoff issue=7 cap=x -->\nTehas cycle stopped. To retry, apply `tehas:ready`.\n\nUsage: 3 calls.\n\n{PARK_LINE}")
    );
}

#[test]
fn a_notes_id_is_github_s_numeric_rest_id_never_its_node_id() {
    let (mut gh, _) = adapter(vec![
        Ok(sample("issue-7.json")),
        Ok(sample("comment-posted.json")),
    ]);
    let got = gh.note(&key(7), "b", &[]).unwrap();
    let v: Value = serde_json::from_str(&sample("comment-posted.json")).unwrap();
    assert_eq!(got.id.as_str().parse::<u64>().ok(), v["id"].as_u64());
    assert_ne!(got.id.as_str(), v["node_id"].as_str().unwrap());
    // An id GitHub gives as a string is no REST id: unread.
    let mut s = v.clone();
    s["id"] = "IC_kwDOAbCdEf6VeQ8x".into();
    assert_eq!(
        note_against(Ok(s.to_string())).0,
        Err(Outcome::Unread(Unread::Malformed))
    );
}

/// gh's answers for issue 21 of the recordings, its REST read with `body`
/// and `updated` in place of its own.
fn recorded_21(body: Option<&str>, updated: Option<&str>) -> String {
    let at = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/recorded/issue-21/issue.json");
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(at).unwrap()).unwrap();
    if let Some(b) = body {
        v["body"] = b.into();
    }
    if let Some(u) = updated {
        v["updated_at"] = u.into();
    }
    v.to_string()
}

fn recorded_21_cut() -> String {
    let at =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/recorded/issue-21/ticket-cut.json");
    std::fs::read_to_string(at).unwrap()
}

#[test]
fn a_ticket_read_after_a_last_attempts_edit_gives_the_text_and_the_block_back() {
    use tehas_ports::Depth;
    let k = key(21);
    let text = "Fix the importer.\r\n\r\nIt drops   the last row.  \r\n";
    let note = "### The last attempt ran out of time\n\n- Steps: develop 90m00s";
    let la = last_attempt(note);
    // The read the cycle makes first: its revision is the one the edit is
    // held to.
    let (mut gh, calls) = adapter(vec![
        Ok(recorded_21(Some(text), None)),
        Ok(recorded_21_cut()),
    ]);
    let before = gh.ticket(&k, Depth::Cut { notes: 0 }, None).unwrap();
    assert_eq!(taken(&calls).len(), 2);
    assert_eq!(before.text, text);
    let edit = MarkerEdit {
        set: vec![la.clone()],
        drop: Vec::new(),
    };
    let (mut gh, calls) = adapter(vec![
        Ok(recorded_21(Some(text), None)),
        Ok(recorded_21(Some("x"), Some(EDITED))),
    ]);
    gh.edit(&k, None, None, &edit, &before.digest.revision)
        .unwrap();
    let stored = serde_json::from_str::<Edited>(taken(&calls)[1].input.as_deref().unwrap())
        .unwrap()
        .body
        .unwrap();
    // The issue's text, its white space at the end alone taken off, then the
    // block.
    assert_eq!(
        stored,
        format!(
            "Fix the importer.\r\n\r\nIt drops   the last row.\n\n<!-- tehas:last-attempt base={} -->\n{note}\n<!-- /tehas:last-attempt -->",
            "0".repeat(64)
        )
    );
    let (mut gh, _) = adapter(vec![
        Ok(recorded_21(Some(&stored), Some(EDITED))),
        Ok(recorded_21_cut()),
    ]);
    let after = gh.ticket(&k, Depth::Cut { notes: 0 }, None).unwrap();
    assert_eq!(after.text, text.trim_end());
    assert_eq!(after.text.trim(), before.text.trim());
    assert_eq!(after.markers, [la]);
    assert_eq!(
        after.digest.revision,
        rev_of(
            &recorded_21(Some(&stored), Some(EDITED))
                .replace("/issues/21", "/issues/7")
                .replace("\"number\":21", "\"number\":7")
        )
    );
}

// After the second read.

const MARKED_NOTE: &str = "<!-- tehas:millwright issue=7 action=retry -->";

#[test]
fn an_edit_setting_a_block_whose_note_holds_a_marker_is_refused_and_sends_nothing() {
    let read = issue_7("Plan", "Fix it.", "2026-11-22T14:30:00Z");
    let at = rev_of(&read);
    let (mut gh, calls) = adapter(vec![Ok(read), Ok(issue_7("Plan", "x", EDITED))]);
    let got = gh.edit(&key(7), None, None, &set(last_attempt(MARKED_NOTE)), &at);
    assert_eq!(got, Err(Outcome::Refused(Refusal::Invalid)));
    assert!(taken(&calls).iter().all(|c| c.input.is_none()), "no PATCH");
}

#[test]
fn a_block_s_close_words_in_a_note_or_a_text_are_refused() {
    let close = "a <!-- /tehas:last-attempt --> b";
    assert_eq!(
        edit_of("Fix it.", None, &set(last_attempt(close))),
        Err(Outcome::Refused(Refusal::Invalid))
    );
    let (mut gh, calls) = adapter(Vec::new());
    let got = gh.edit(&key(7), None, Some(close), &MarkerEdit::default(), &since());
    assert_eq!(got, Err(Outcome::Refused(Refusal::Invalid)));
    assert_eq!(
        gh.note(&key(7), close, &[]).map(|_| ()),
        Err(Outcome::Refused(Refusal::Invalid))
    );
    assert!(taken(&calls).is_empty());
}

/// A body as GitHub's web editor saves it, CRLF, with a last attempt's
/// block.
fn crlf_body() -> String {
    format!("Line one.\r\nLine two.\r\n\r\n<!-- tehas:last-attempt base={BASE} -->\r\nOld note.\r\n<!-- /tehas:last-attempt -->")
}

#[test]
fn a_crlf_block_is_replaced_and_the_body_s_other_bytes_stay_as_found() {
    assert_eq!(
        edit_of(&crlf_body(), None, &set(last_attempt("New note."))),
        Ok(Some(format!(
            "Line one.\r\nLine two.\n\n<!-- tehas:last-attempt base={BASE} -->\nNew note.\n<!-- /tehas:last-attempt -->"
        )))
    );
}

#[test]
fn setting_the_crlf_block_that_is_there_sends_nothing() {
    assert_eq!(
        edit_of(&crlf_body(), None, &set(last_attempt("Old note."))),
        Ok(None)
    );
}

#[test]
fn an_edit_leaves_no_stray_space_no_leading_blank_lines_and_no_third_line_end() {
    let drop = MarkerEdit {
        set: Vec::new(),
        drop: vec![(MarkerKind::Blocked, None)],
    };
    assert_eq!(
        edit_of("A. <!-- tehas:blocked by=3 -->\nB.", None, &drop),
        Ok(Some("A.\nB.".into()))
    );
    assert_eq!(
        edit_of("A.\n<!-- tehas:blocked by=3 --> B.", None, &drop),
        Ok(Some("A.\nB.".into()))
    );
    // An empty text leaves the markers alone.
    assert_eq!(
        edit_of(
            "Old.\n\n<!-- tehas:blocked by=3 -->",
            Some(""),
            &MarkerEdit::default()
        ),
        Ok(Some("<!-- tehas:blocked by=3 -->".into()))
    );
    // A body of markers alone keeps its CRs when a text comes before them.
    assert_eq!(
        edit_of(
            "<!-- tehas:blocked by=3 -->\r\n<!-- tehas:hold-cleared -->",
            Some("New."),
            &MarkerEdit::default()
        ),
        Ok(Some(
            "New.\n\n<!-- tehas:blocked by=3 -->\r\n<!-- tehas:hold-cleared -->".into()
        ))
    );
    // A block moved to the end leaves one blank line where it was, not two.
    let body = format!("Text.\n\n<!-- tehas:last-attempt base={BASE} -->\nOld.\n<!-- /tehas:last-attempt -->\n\n<!-- tehas:blocked by=3 -->");
    assert_eq!(
        edit_of(&body, None, &set(last_attempt("New."))),
        Ok(Some(format!("Text.\n\n<!-- tehas:blocked by=3 -->\n<!-- tehas:last-attempt base={BASE} -->\nNew.\n<!-- /tehas:last-attempt -->")))
    );
}

#[test]
fn a_revision_from_the_list_read_is_held_by_an_edit_of_the_unchanged_issue() {
    use tehas_ports::{Bound, Filter};
    for body in ["One page.", "One page.\r\n\r\n- read them\r\n- save them"] {
        let node = serde_json::json!({"number": 7, "title": "Plan", "body": body, "state": "OPEN",
            "stateReason": null, "url": "https://github.com/acme/widget/issues/7",
            "createdAt": "2026-11-20T09:00:00Z", "updatedAt": "2026-11-22T14:30:00Z",
            "author": null, "issueType": null, "milestone": null,
            "labels": {"totalCount": 0, "nodes": []}, "parent": null});
        let page = serde_json::json!({"data": {"repository": {"issues": {
            "pageInfo": {"hasNextPage": false, "endCursor": null}, "nodes": [node]}}}});
        let (mut gh, calls) = adapter(vec![
            Ok(page.to_string()),
            Ok(issue_7("Plan", body, "2026-11-23T09:00:00Z")),
            Ok(issue_7("Plan", "New.", EDITED)),
        ]);
        let listed = gh.list(&Filter::default(), Bound::new(5).unwrap()).unwrap();
        let revision = listed.items()[0].revision.clone();
        let got = gh.edit(
            &key(7),
            None,
            Some("New."),
            &MarkerEdit::default(),
            &revision,
        );
        assert!(got.is_ok(), "{body:?}: {got:?}");
        assert_eq!(taken(&calls).len(), 3);
    }
}

/// The JSON a write sent on standard input, as its text.
fn input_of(c: &Call) -> String {
    c.input.clone().unwrap()
}

fn call(words: &[&str]) -> Call {
    Call {
        args: args(words),
        input: None,
    }
}

/// An open issue is closed by one `PATCH` of its state and the reason
/// GitHub keeps, after the read.
#[test]
fn close_reads_the_issue_then_patches_its_state_and_reason() {
    use tehas_ports::CloseReason;
    let (mut gh, calls) = adapter(vec![
        Ok(sample("issue-7.json")),
        Ok(sample("issue-7-closed-not-planned.json")),
    ]);
    gh.close(&key(7), &CloseReason::NotPlanned).unwrap();
    let calls = taken(&calls);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0], read_7());
    assert_eq!(
        calls[1].args,
        args(&[
            "api",
            "-X",
            "PATCH",
            "repos/acme/widget/issues/7",
            "--input",
            "-"
        ])
    );
    assert_eq!(
        input_of(&calls[1]),
        serde_json::json!({"state": "closed", "state_reason": "not_planned"}).to_string()
    );
}

/// Closing a closed issue is done, and nothing is sent.
#[test]
fn closing_a_closed_issue_is_done_with_nothing_sent() {
    use tehas_ports::CloseReason;
    let (mut gh, calls) = adapter(vec![Ok(sample("issue-7-closed.json"))]);
    gh.close(&key(7), &CloseReason::Done).unwrap();
    assert_eq!(taken(&calls), vec![read_7()]);
}

/// The close answers the site's own time of it, from the answer that found
/// the issue closed: the `PATCH` answer of a close sent here, and the read
/// of one already closed. A site that names no time answers none.
#[test]
fn a_close_answers_the_sites_time_of_it() {
    use tehas_ports::{At, CloseReason};
    // The close sent here: the PATCH's answer names the time.
    let (mut gh, calls) = adapter(vec![
        Ok(sample("issue-7.json")),
        Ok(sample("issue-7-closed.json")),
    ]);
    assert_eq!(
        gh.close_at(&key(7), &CloseReason::Done),
        Ok(Some(At::new(1795428000)))
    );
    assert_eq!(taken(&calls).len(), 2);
    // One found closed by its read: the read names it, nothing sent.
    let (mut gh, calls) = adapter(vec![Ok(sample("issue-7-closed.json"))]);
    assert_eq!(
        gh.close_at(&key(7), &CloseReason::Done),
        Ok(Some(At::new(1795428000)))
    );
    assert_eq!(taken(&calls).len(), 1);
    // The same answer with no `closed_at` in it: closed, no time.
    let mut v: serde_json::Value = serde_json::from_str(&sample("issue-7-closed.json")).unwrap();
    v.as_object_mut().unwrap().remove("closed_at");
    let (mut gh, _) = adapter(vec![Ok(sample("issue-7.json")), Ok(v.to_string())]);
    assert_eq!(gh.close_at(&key(7), &CloseReason::Done), Ok(None));
}

/// A pull request's number is no ticket, and a refused input made nothing.
#[test]
fn close_refuses_a_pull_request_and_reads_a_422_as_refused() {
    use tehas_ports::CloseReason;
    let (mut gh, calls) = adapter(vec![Ok(sample("pull-8.json"))]);
    assert_eq!(
        gh.close(&key(8), &CloseReason::Done),
        Err(Outcome::NotFound)
    );
    assert_eq!(taken(&calls).len(), 1);
    let (mut gh, _) = adapter(vec![
        Ok(sample("issue-7.json")),
        Err(failed("validation.stderr")),
    ]);
    assert_eq!(
        gh.close(&key(7), &CloseReason::Done),
        Err(Outcome::Refused(Refusal::Invalid))
    );
}

/// The login of the account is read by its id, then posted to the
/// issue's assignees, and the answer is read for it.
#[test]
fn assign_posts_the_accounts_login_and_reads_it_back() {
    let (mut gh, calls) = adapter(vec![
        Ok(sample("user-904.json")),
        Ok(sample("assignees-taken.json")),
    ]);
    gh.assign(&key(7), &tehas_ports::AccountId::new("904"))
        .unwrap();
    let calls = taken(&calls);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0], call(&["api", "user/904"]));
    assert_eq!(
        calls[1].args,
        args(&[
            "api",
            "-X",
            "POST",
            "repos/acme/widget/issues/7/assignees",
            "--input",
            "-"
        ])
    );
    assert_eq!(
        input_of(&calls[1]),
        serde_json::json!({"assignees": ["carol"]}).to_string()
    );
}

/// GitHub answers 201 and drops a login it cannot assign: the answer is
/// read, and ours missing from it is refused, never done.
#[test]
fn an_assignee_github_dropped_is_refused() {
    let (mut gh, _) = adapter(vec![
        Ok(sample("user-904.json")),
        Ok(sample("assignees-dropped.json")),
    ]);
    assert_eq!(
        gh.assign(&key(7), &tehas_ports::AccountId::new("904")),
        Err(Outcome::Refused(Refusal::Invalid))
    );
}

/// A bot holds no assignment, and an id GitHub never gives is no account:
/// both refused, nothing posted.
#[test]
fn assign_refuses_a_bot_and_an_id_that_is_no_githubs() {
    let (mut gh, calls) = adapter(vec![Ok(sample("user-100001.json"))]);
    assert_eq!(
        gh.assign(&key(7), &tehas_ports::AccountId::new("100001")),
        Err(Outcome::Refused(Refusal::Invalid))
    );
    assert_eq!(taken(&calls).len(), 1);
    let (mut gh, calls) = adapter(vec![]);
    assert_eq!(
        gh.assign(&key(7), &tehas_ports::AccountId::new("carol")),
        Err(Outcome::Refused(Refusal::Invalid))
    );
    assert!(taken(&calls).is_empty());
}

/// A child with no parent: its id read, then posted to the parent's
/// sub-issues.
#[test]
fn add_part_reads_the_childs_parent_and_id_then_posts_it() {
    let (mut gh, calls) = adapter(vec![
        Err(failed("not-found.stderr")),
        Ok(sample("issue-13.json")),
        Ok(sample("sub-issue-added.json")),
    ]);
    gh.add_part(&key(12), &key(13)).unwrap();
    let calls = taken(&calls);
    assert_eq!(calls.len(), 3);
    assert_eq!(
        calls[0],
        call(&["api", "repos/acme/widget/issues/13/parent"])
    );
    assert_eq!(calls[1], call(&["api", "repos/acme/widget/issues/13"]));
    assert_eq!(
        calls[2].args,
        args(&[
            "api",
            "-X",
            "POST",
            "repos/acme/widget/issues/12/sub_issues",
            "--input",
            "-"
        ])
    );
    assert_eq!(
        input_of(&calls[2]),
        serde_json::json!({"sub_issue_id": 3001230013u64}).to_string()
    );
}

/// Already this parent's part is done with nothing sent; another
/// parent's is refused and never moved; a ticket is no part of itself.
#[test]
fn add_part_is_done_for_its_own_parent_and_refused_for_another() {
    let (mut gh, calls) = adapter(vec![Ok(sample("issue-12.json"))]);
    gh.add_part(&key(12), &key(13)).unwrap();
    assert_eq!(taken(&calls).len(), 1);
    let (mut gh, calls) = adapter(vec![Ok(sample("issue-14.json"))]);
    assert_eq!(
        gh.add_part(&key(12), &key(13)),
        Err(Outcome::Refused(Refusal::Invalid))
    );
    assert_eq!(taken(&calls).len(), 1);
    let (mut gh, calls) = adapter(vec![]);
    assert_eq!(
        gh.add_part(&key(12), &key(12)),
        Err(Outcome::Refused(Refusal::Cycle))
    );
    assert!(taken(&calls).is_empty());
}

/// A 422 is GitHub refusing the input: a mark it refused made nothing, so
/// it is refused, never a write that may have been sent.
#[test]
fn a_mark_github_refuses_with_422_is_refused_not_maybe_sent() {
    let (mut gh, _) = adapter(vec![Err(failed("validation.stderr"))]);
    assert_eq!(
        gh.mark(
            &key(7),
            &tehas_ports::Mark::State(tehas_ports::State::Running)
        ),
        Err(Outcome::Refused(Refusal::Invalid))
    );
}

/// A child whose parent is an issue of another repository has another
/// parent: refused, never moved.
#[test]
fn a_parent_of_another_repository_is_another_parent() {
    let (mut gh, calls) = adapter(vec![Ok(sample("issue-12-other-repo.json"))]);
    assert_eq!(
        gh.add_part(&key(12), &key(13)),
        Err(Outcome::Refused(Refusal::Invalid))
    );
    assert_eq!(but_the_name(&taken(&calls)), 1);
}
