//! The tickets port's `note` and `edit` as the adapter answers them: the
//! issue read, then one comment posted with its markers, or one edit of
//! the issue's title and body at its revision, over samples of GitHub's
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
        assert_eq!(taken(&calls).len(), 1, "nothing posted");
    }
    // A project named in another case is the same project.
    let answer = sample("comment-posted.json").replace("acme/widget", "Acme/Widget");
    assert!(note_against(Ok(answer)).0.is_ok());
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

#[test]
fn a_cited_or_quoted_marker_is_text_and_is_posted() {
    let text = "Write `<!-- tehas:millwright issue=7 action=finish -->` first.\n> <!-- tehas:hold-cleared -->";
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

/// The revision of the issue's sample, the time GitHub last saw it change:
/// what a read of it gives as `TicketDigest.revision`.
fn since() -> Revision {
    Revision::new("2026-11-22T14:30:00Z")
}

/// The issue's sample with its title and body set, and its revision
/// `updated`.
fn issue_7(title: &str, body: &str, updated: &str) -> String {
    let mut v: Value = serde_json::from_str(&sample("issue-7.json")).unwrap();
    v["title"] = title.into();
    v["body"] = body.into();
    v["updated_at"] = updated.into();
    v.to_string()
}

const TITLE: &str = "Plan: the title only standard input carries";
const EDITED: &str = "2026-11-23T10:20:00Z";

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
    let mut l = LastAttemptMarker::new(Sha256::new("0".repeat(64)).unwrap());
    l.note = note.to_string();
    Marker::LastAttempt(l)
}

/// A ticket's body as 1.x left it: the text, a hold, and a last attempt's
/// note.
fn marked_body() -> String {
    format!(
        "One page for every setting.\r\n\r\n<!-- tehas:blocked by=3 -->\n<!-- tehas:last-attempt base={} -->\nIt ran out of time.\n<!-- /tehas:last-attempt -->",
        "0".repeat(64)
    )
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
fn an_edit_of_an_issue_changed_since_its_revision_is_a_conflict_and_sends_nothing() {
    let changed = issue_7("Plan: the widget's settings page", "Same.", EDITED);
    let (mut gh, calls) = adapter(vec![Ok(changed)]);
    let got = gh.edit(&key(7), Some("x"), None, &MarkerEdit::default(), &since());
    assert_eq!(got, Err(Outcome::Conflict(Lost::Changed)));
    assert_eq!(taken(&calls).len(), 1, "only the read");
    // An answer with no revision is unread, not a conflict.
    let mut v: Value = serde_json::from_str(&sample("issue-7.json")).unwrap();
    v.as_object_mut().unwrap().remove("updated_at");
    let (mut gh, calls) = adapter(vec![Ok(v.to_string())]);
    let got = gh.edit(&key(7), Some("x"), None, &MarkerEdit::default(), &since());
    assert_eq!(got, Err(Outcome::Unread(Unread::Malformed)));
    assert_eq!(taken(&calls).len(), 1);
}

#[test]
fn an_edit_keeps_every_marker_it_is_not_told_to_change_in_its_order() {
    let read = issue_7("Plan", &marked_body(), "2026-11-22T14:30:00Z");
    let (mut gh, calls) = adapter(vec![Ok(read), Ok(issue_7("Plan", "x", EDITED))]);
    gh.edit(
        &key(7),
        None,
        Some("New text."),
        &MarkerEdit::default(),
        &since(),
    )
    .unwrap();
    let body = edited(&taken(&calls)[1]).body.unwrap();
    assert_eq!(
        body,
        format!(
            "New text.\n\n<!-- tehas:blocked by=3 -->\n<!-- tehas:last-attempt base={} -->\nIt ran out of time.\n<!-- /tehas:last-attempt -->",
            "0".repeat(64)
        )
    );
    let back = markers::read(&body, BlueprintNo::new(7));
    assert_eq!(back.text, "New text.");
    assert_eq!(
        back.markers,
        [blocked(3), last_attempt("It ran out of time.")]
    );
}

#[test]
fn an_edit_sets_and_drops_markers_and_keeps_the_text_when_given_none() {
    let read = issue_7("Plan", &marked_body(), "2026-11-22T14:30:00Z");
    let (mut gh, calls) = adapter(vec![Ok(read), Ok(issue_7("Plan", "x", EDITED))]);
    let edit = MarkerEdit {
        set: vec![last_attempt("Learned more."), Marker::HoldCleared],
        drop: vec![(MarkerKind::Blocked, None)],
    };
    gh.edit(&key(7), None, None, &edit, &since()).unwrap();
    let sent = edited(&taken(&calls)[1]);
    assert_eq!(sent.title, None);
    let back = markers::read(&sent.body.unwrap(), BlueprintNo::new(7));
    assert_eq!(back.text, "One page for every setting.");
    assert_eq!(
        back.markers,
        [last_attempt("Learned more."), Marker::HoldCleared]
    );
}

#[test]
fn a_last_attempts_edit_writes_the_bytes_1x_wrote() {
    // last_attempt.rs's decide: the body, a blank line, the open line, the
    // note, the close line.
    let note = "### The last attempt ran out of time\n\nThe last cycle on this text was stopped in develop: the developer ran past its time bound. Its work was not pushed, and the next cycle starts from a clean checkout.\n\nPlan a smaller change: finish one part well, or say in this issue what to leave out. This section is the factory's; edit or delete it as you like.";
    let written_1x = format!(
        "Fix it.\n\n<!-- tehas:last-attempt base={} -->\n{note}\n<!-- /tehas:last-attempt -->",
        "0".repeat(64)
    );
    let read = issue_7("Fix", "Fix it.\r\n\r\n", "2026-11-22T14:30:00Z");
    let (mut gh, calls) = adapter(vec![Ok(read), Ok(issue_7("Fix", "x", EDITED))]);
    let edit = MarkerEdit {
        set: vec![last_attempt(note)],
        drop: Vec::new(),
    };
    gh.edit(&key(7), None, None, &edit, &since()).unwrap();
    assert_eq!(edited(&taken(&calls)[1]).body.unwrap(), written_1x);
}

#[test]
fn a_body_with_a_marker_that_does_not_read_is_not_rewritten_but_its_title_is() {
    for body in [
        "Text.\n\n<!-- tehas:blocked by=03 -->",
        "Text.\n\ntehas:blocked by=3",
        "Text.\n\n<!-- tehas:nonsense -->",
    ] {
        let read = issue_7("Plan", body, "2026-11-22T14:30:00Z");
        let (mut gh, calls) = adapter(vec![Ok(read.clone())]);
        let got = gh.edit(
            &key(7),
            None,
            Some("New."),
            &MarkerEdit::default(),
            &since(),
        );
        assert_eq!(got, Err(Outcome::Refused(Refusal::Unsupported)), "{body}");
        assert_eq!(taken(&calls).len(), 1, "only the read");
        let (mut gh, calls) = adapter(vec![Ok(read.clone()), Ok(read)]);
        let got = gh.edit(
            &key(7),
            Some("Retitled"),
            None,
            &MarkerEdit::default(),
            &since(),
        );
        assert!(got.is_ok(), "{got:?}");
        assert_eq!(
            edited(&taken(&calls)[1]),
            Edited {
                title: Some("Retitled".into()),
                body: None
            }
        );
    }
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
fn an_edit_of_a_pull_request_is_not_found_and_sends_nothing() {
    let (mut gh, calls) = adapter(vec![Ok(sample("pull-8.json"))]);
    let got = gh.edit(&key(8), Some("x"), None, &MarkerEdit::default(), &since());
    assert_eq!(got, Err(Outcome::NotFound));
    assert_eq!(taken(&calls).len(), 1);
}

#[test]
fn what_an_edits_guards_refuse_is_refused_before_any_request() {
    let invalid = Err(Outcome::Refused(Refusal::Invalid));
    let (mut gh, calls) = adapter(Vec::new());
    let set = |m: Marker| MarkerEdit {
        set: vec![m],
        drop: Vec::new(),
    };
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
    let k = TicketKey::Key(KeyText::new("PROJ-1").unwrap());
    assert_eq!(gh.edit(&k, Some("t"), None, &none, &s), invalid);
    assert!(taken(&calls).is_empty());
    // A marker of another item reads nowhere: refused after the read, with
    // nothing sent.
    let (mut gh, calls) = adapter(vec![Ok(sample("issue-7.json"))]);
    let other = Marker::Question {
        kind: QuestionKind::Park,
        blueprint: BlueprintNo::new(8).unwrap(),
        turn: NonZeroU32::new(1).unwrap(),
    };
    assert_eq!(gh.edit(&key(7), None, None, &set(other), &s), invalid);
    assert_eq!(taken(&calls).len(), 1);
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
