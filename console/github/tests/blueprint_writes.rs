//! The blueprints port's writes as the adapter answers them: `open`,
//! `edit` and `comment`, over samples of GitHub's answers in
//! `tests/samples/blueprint-writes/` (hand-made from GitHub's documented
//! shapes, no secret and no private name). A failure sample is gh's error
//! stream, given to the adapter as `Bounded` would give it. No test calls
//! GitHub.

use serde::Deserialize;
use serde_json::Value;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use tehas_core::forge::GhRunner;
use tehas_github::GitHub;
use tehas_ports::laws;
use tehas_ports::{
    At, BlueprintNo, BlueprintWrites, Body, CommentId, Got, Lost, Marker, MarkerEdit, NewBlueprint,
    Outcome, Passing, Pool, Refusal, Revision, Sent, Sha256, Sum, Unread, WebLink,
};

const PROJECT: &str = "acme/widget";

/// The words `Bounded` gives for a gh it stopped at its bound.
const BOUND: &str = "gh ran out of its 30s bound";

fn sample(name: &str) -> String {
    let at = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/samples/blueprint-writes");
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

fn no(n: u64) -> BlueprintNo {
    BlueprintNo::new(n).unwrap()
}

/// One call the runner was given: its arguments, and its standard input.
#[derive(Clone, Debug, PartialEq)]
struct Call {
    args: Vec<String>,
    input: Option<String>,
}

type Calls = Arc<Mutex<Vec<Call>>>;

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

fn adapter(answers: Vec<Result<String, String>>) -> (GitHub, Calls) {
    let calls = Calls::default();
    let script = Script {
        answers: answers.into(),
        calls: Arc::clone(&calls),
    };
    let gh = GitHub::new(PROJECT, Box::new(script)).on_clock(now);
    (gh, calls)
}

/// A body of 200 KiB, past what one argument may hold, and unlike any
/// other text of a test.
fn big_body() -> String {
    let line = "A line of the plan that only this body holds, number ";
    let mut body = String::new();
    let mut i = 0;
    while body.len() < 200 * 1024 {
        body.push_str(&format!("{line}{i}.\n"));
        i += 1;
    }
    body
}

const TITLE: &str = "Plan: the title only standard input carries";

/// No argument holds the text, nor a 64-byte piece of it.
fn kept_out_of_argv(calls: &[Call], text: &str) {
    let probe = &text[..text.len().min(64)];
    for c in calls {
        for a in &c.args {
            assert!(!a.contains(probe), "an argument holds the text: {a:.80}");
        }
    }
}

/// What a write sent on standard input: nothing but these fields.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    title: Option<String>,
    body: Option<String>,
}

fn input(c: &Call) -> Input {
    serde_json::from_str(c.input.as_deref().unwrap()).unwrap()
}

fn sent(title: Option<&str>, body: Option<&str>) -> Input {
    Input {
        title: title.map(str::to_string),
        body: body.map(str::to_string),
    }
}

/// An issue's answer with its title and body set.
fn issue_7(title: &str, body: &str) -> String {
    let mut v: Value = serde_json::from_str(&sample("issue-7.json")).unwrap();
    v["title"] = title.into();
    v["body"] = body.into();
    v.to_string()
}

/// The revision the relay reads today (`GitHub::text_revision`) of an
/// answer.
fn revision_of(answer: &str) -> Revision {
    let (mut gh, _) = adapter(vec![Ok(answer.to_string())]);
    Revision::new(gh.text_revision(7).unwrap())
}

fn new_blueprint(title: &str, body: &str) -> NewBlueprint {
    NewBlueprint {
        title: title.into(),
        body: Body::Text(body.into()),
        markers: Vec::new(),
    }
}

fn args(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| w.to_string()).collect()
}

#[test]
fn an_open_sends_a_200_kib_body_and_its_title_on_standard_input_alone() {
    let body = big_body();
    let (mut gh, calls) = adapter(vec![Ok(sample("issue-created.json"))]);
    let got = gh.open(&new_blueprint(TITLE, &body)).unwrap();
    assert_eq!(got.id, no(42));
    // The issue's own link, as the container's gh prints it.
    assert_eq!(
        got.link.as_str(),
        "https://github.com/acme/widget/issues/42"
    );
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].args,
        args(&[
            "api",
            "-X",
            "POST",
            "repos/acme/widget/issues",
            "--input",
            "-"
        ])
    );
    kept_out_of_argv(&calls, &body);
    kept_out_of_argv(&calls, TITLE);
    assert_eq!(input(&calls[0]), sent(Some(TITLE), Some(&body)));
}

#[test]
fn a_comment_sends_a_200_kib_body_on_standard_input_and_answers_the_comments_link() {
    let body = big_body();
    let (mut gh, calls) = adapter(vec![
        Ok(sample("issue-7.json")),
        Ok(sample("comment-created.json")),
    ]);
    let got = gh.comment(no(7), &body, &[]).unwrap();
    assert_eq!(got.id, CommentId::new("2501234567"));
    // The comment's link, as `gh issue comment` prints it.
    assert_eq!(
        got.link.as_str(),
        "https://github.com/acme/widget/issues/7#issuecomment-2501234567"
    );
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    // The issue is read first, so a pull request's number gets no comment.
    assert_eq!(
        calls[0],
        Call {
            args: args(&["api", "repos/acme/widget/issues/7"]),
            input: None
        }
    );
    assert_eq!(
        calls[1].args,
        args(&[
            "api",
            "-X",
            "POST",
            "repos/acme/widget/issues/7/comments",
            "--input",
            "-"
        ])
    );
    kept_out_of_argv(&calls, &body);
    assert_eq!(input(&calls[1]), sent(None, Some(&body)));
}

#[test]
fn an_edit_reads_the_issue_then_sends_a_200_kib_body_and_title_on_standard_input() {
    let body = big_body();
    let read = sample("issue-7.json");
    let since = revision_of(&read);
    let (mut gh, calls) = adapter(vec![Ok(read), Ok(issue_7(TITLE, &body))]);
    let got = gh
        .edit(
            no(7),
            Some(TITLE),
            Some(&body),
            &MarkerEdit::default(),
            &since,
        )
        .unwrap();
    assert_eq!(got.as_str(), "https://github.com/acme/widget/issues/7");
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[0],
        Call {
            args: args(&["api", "repos/acme/widget/issues/7"]),
            input: None
        }
    );
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
    kept_out_of_argv(&calls, &body);
    kept_out_of_argv(&calls, TITLE);
    assert_eq!(input(&calls[1]), sent(Some(TITLE), Some(&body)));
}

#[test]
fn an_edit_sends_only_what_it_is_given() {
    let read = sample("issue-7.json");
    let since = revision_of(&read);
    let (mut gh, calls) = adapter(vec![Ok(read.clone()), Ok(read)]);
    gh.edit(
        no(7),
        None,
        Some("New body."),
        &MarkerEdit::default(),
        &since,
    )
    .unwrap();
    assert_eq!(
        input(&calls.lock().unwrap()[1]),
        sent(None, Some("New body."))
    );
}

#[test]
fn an_edit_of_an_issue_changed_since_its_revision_is_a_conflict_and_sends_nothing() {
    let read = sample("issue-7.json");
    let since = revision_of(&read);
    let v: Value = serde_json::from_str(&read).unwrap();
    let title = v["title"].as_str().unwrap();
    let body = v["body"].as_str().unwrap();
    for changed in [
        issue_7(title, "Another account's body."),
        issue_7("Retitled", body),
    ] {
        let (mut gh, calls) = adapter(vec![Ok(changed)]);
        let got = gh.edit(no(7), Some("x"), Some("y"), &MarkerEdit::default(), &since);
        assert_eq!(got, Err(Outcome::Conflict(Lost::Changed)));
        assert_eq!(calls.lock().unwrap().len(), 1, "only the read");
    }
}

#[test]
fn a_label_or_a_comment_does_not_move_the_revision() {
    let read = sample("issue-7.json");
    let since = revision_of(&read);
    let mut v: Value = serde_json::from_str(&read).unwrap();
    v["labels"] = serde_json::json!([]);
    v["comments"] = 3.into();
    v["updated_at"] = "2026-11-23T11:00:00Z".into();
    let (mut gh, calls) = adapter(vec![Ok(v.to_string()), Ok(read)]);
    let got = gh.edit(no(7), Some("x"), None, &MarkerEdit::default(), &since);
    assert!(got.is_ok(), "{got:?}");
    assert_eq!(calls.lock().unwrap().len(), 2);
}

#[test]
fn the_read_before_an_edit_or_a_comment_fails_with_no_write_and_says_it_was_not_sent() {
    let since = revision_of(&sample("issue-7.json"));
    let no_sent = |why| Outcome::Unavailable {
        why,
        sent: Sent::No,
    };
    let cases = [
        // The relay's runner's reasons: gh's status and the bound's 124.
        ("gh exited 1: HTTP 404".to_string(), Outcome::NotFound),
        ("gh exited 1: HTTP 410".to_string(), Outcome::NotFound),
        ("gh exited 1: HTTP 403".to_string(), Outcome::Forbidden),
        ("gh exited 1: HTTP 401".to_string(), Outcome::SignedOut),
        (
            "gh exited 1: HTTP 502".to_string(),
            no_sent(Passing::Server),
        ),
        ("gh exited 124".to_string(), no_sent(Passing::Timeout)),
        (failed("not-found.stderr"), Outcome::NotFound),
        (failed("forbidden.stderr"), Outcome::Forbidden),
        (
            failed("server-error.stderr"),
            Outcome::Unavailable {
                why: Passing::Server,
                sent: Sent::No,
            },
        ),
        (
            BOUND.to_string(),
            Outcome::Unavailable {
                why: Passing::Timeout,
                sent: Sent::No,
            },
        ),
        (
            failed("rate-limited.stderr"),
            Outcome::Spent {
                pool: Pool::Calls,
                until: At::new(now()),
            },
        ),
    ];
    for (why, want) in cases {
        let (mut gh, calls) = adapter(vec![Err(why.clone())]);
        let got = gh.edit(
            no(7),
            Some(TITLE),
            Some("b"),
            &MarkerEdit::default(),
            &since,
        );
        assert_eq!(got, Err(want.clone()), "edit: {why}");
        assert_eq!(calls.lock().unwrap().len(), 1, "edit: {why}");
        let (mut gh, calls) = adapter(vec![Err(why.clone())]);
        let got = gh.comment(no(7), "b", &[]).map(|_| ());
        assert_eq!(got, Err(want), "comment: {why}");
        assert_eq!(calls.lock().unwrap().len(), 1, "comment: {why}");
    }
}

#[test]
fn an_issue_answer_with_no_title_is_unread_not_a_conflict() {
    let mut v: Value = serde_json::from_str(&sample("issue-7.json")).unwrap();
    v.as_object_mut().unwrap().remove("title");
    let since = revision_of(&sample("issue-7.json"));
    let (mut gh, calls) = adapter(vec![Ok(v.to_string())]);
    let got = gh.edit(no(7), Some("x"), None, &MarkerEdit::default(), &since);
    assert_eq!(got, Err(Outcome::Unread(Unread::Malformed)));
    assert_eq!(calls.lock().unwrap().len(), 1);
}

#[test]
fn a_comment_on_a_pull_requests_number_is_not_found_and_sends_nothing() {
    let (mut gh, calls) = adapter(vec![Ok(sample("pull-8.json"))]);
    let got = gh.comment(no(8), "b", &[]);
    assert_eq!(got, Err(Outcome::NotFound));
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert!(calls[0].input.is_none(), "no POST");
}

#[test]
fn an_edit_of_a_pull_request_is_not_found_and_sends_nothing() {
    let read = sample("pull-8.json");
    let since = revision_of(&read);
    let (mut gh, calls) = adapter(vec![Ok(read)]);
    let got = gh.edit(no(8), Some("x"), None, &MarkerEdit::default(), &since);
    assert_eq!(got, Err(Outcome::NotFound));
    assert_eq!(calls.lock().unwrap().len(), 1);
}

#[test]
fn an_edit_that_would_drop_a_marker_is_refused_and_a_title_alone_is_made() {
    let marked = issue_7(
        "Plan",
        "The plan.\n\n<!-- tehas:park issue=7 step=work kind=stopped provider=0 sig=0000000000000000000000000000000000000000000000000000000000000000 -->",
    );
    let since = revision_of(&marked);
    let (mut gh, calls) = adapter(vec![Ok(marked.clone())]);
    let got = gh.edit(no(7), None, Some("New."), &MarkerEdit::default(), &since);
    assert_eq!(got, Err(Outcome::Refused(Refusal::Unsupported)));
    assert_eq!(calls.lock().unwrap().len(), 1, "only the read");

    let (mut gh, calls) = adapter(vec![Ok(marked.clone()), Ok(marked)]);
    let got = gh.edit(
        no(7),
        Some("Retitled"),
        None,
        &MarkerEdit::default(),
        &since,
    );
    assert!(got.is_ok(), "{got:?}");
    assert_eq!(
        input(&calls.lock().unwrap()[1]),
        sent(Some("Retitled"), None)
    );
}

#[test]
fn an_edit_with_nothing_to_change_reads_and_writes_nothing() {
    let read = sample("issue-7.json");
    let since = revision_of(&read);
    let (mut gh, calls) = adapter(vec![Ok(read)]);
    let got = gh.edit(no(7), None, None, &MarkerEdit::default(), &since);
    assert_eq!(
        got.unwrap().as_str(),
        "https://github.com/acme/widget/issues/7"
    );
    assert_eq!(calls.lock().unwrap().len(), 1);
}

#[test]
fn markers_and_marked_texts_are_refused_before_any_request() {
    let since = Revision::new("r");
    let marker = Marker::HoldCleared;
    let set = MarkerEdit {
        set: vec![marker.clone()],
        drop: Vec::new(),
    };
    let marked = "Text <!-- tehas:hold-cleared --> here";
    let (mut gh, calls) = adapter(Vec::new());
    let mut with_marker = new_blueprint("t", "b");
    with_marker.markers = vec![marker.clone()];
    let unsupported = Err(Outcome::Refused(Refusal::Unsupported));
    let invalid = Err(Outcome::Refused(Refusal::Invalid));
    assert_eq!(gh.open(&with_marker).map(|_| ()), unsupported);
    assert_eq!(gh.comment(no(7), "b", &[marker]).map(|_| ()), unsupported);
    assert_eq!(
        gh.edit(no(7), None, Some("b"), &set, &since).map(|_| ()),
        unsupported
    );
    assert_eq!(gh.open(&new_blueprint("t", marked)).map(|_| ()), invalid);
    assert_eq!(gh.open(&new_blueprint(marked, "b")).map(|_| ()), invalid);
    assert_eq!(gh.comment(no(7), marked, &[]).map(|_| ()), invalid);
    let edit = gh.edit(no(7), None, Some(marked), &MarkerEdit::default(), &since);
    assert_eq!(edit.map(|_| ()), invalid);
    let summed = NewBlueprint {
        title: "t".into(),
        body: Body::Summed(Sum {
            sha256: Sha256::new("a".repeat(64)).unwrap(),
            len: 1,
        }),
        markers: Vec::new(),
    };
    assert_eq!(gh.open(&summed).map(|_| ()), invalid);
    assert!(calls.lock().unwrap().is_empty());
}

#[test]
fn a_repository_that_is_no_owner_and_name_gets_no_write() {
    for repo in [
        "acme",
        "acme/widget/issues",
        "acme/..",
        "../widget",
        "acme/wid get",
        "acme/w?x=1",
        "/widget",
    ] {
        let calls = Calls::default();
        let script = Script {
            answers: VecDeque::new(),
            calls: Arc::clone(&calls),
        };
        let mut gh = GitHub::new(repo, Box::new(script));
        let invalid = Err(Outcome::Refused(Refusal::Invalid));
        assert_eq!(
            gh.open(&new_blueprint("t", "b")).map(|_| ()),
            invalid,
            "{repo}"
        );
        assert_eq!(gh.comment(no(7), "b", &[]).map(|_| ()), invalid, "{repo}");
        let edit = gh.edit(
            no(7),
            Some("t"),
            None,
            &MarkerEdit::default(),
            &Revision::new("r"),
        );
        assert_eq!(edit.map(|_| ()), invalid, "{repo}");
        assert!(calls.lock().unwrap().is_empty(), "{repo}");
    }
}

/// Each write of the three, run once against `answers` after its own read
/// where it has one: what it answered, and the calls it made.
fn each_write(fail: &Result<String, String>) -> Vec<(&'static str, Got<()>, Vec<Call>)> {
    let read = sample("issue-7.json");
    let since = revision_of(&read);
    let (mut gh, calls) = adapter(vec![fail.clone()]);
    let open = gh.open(&new_blueprint(TITLE, &big_body())).map(|_| ());
    let open_calls = calls.lock().unwrap().clone();
    let (mut gh, calls) = adapter(vec![Ok(read.clone()), fail.clone()]);
    let comment = gh.comment(no(7), &big_body(), &[]).map(|_| ());
    let comment_calls = calls.lock().unwrap().clone();
    let (mut gh, calls) = adapter(vec![Ok(read), fail.clone()]);
    let edit = gh
        .edit(
            no(7),
            Some(TITLE),
            Some(&big_body()),
            &MarkerEdit::default(),
            &since,
        )
        .map(|_| ());
    let edit_calls = calls.lock().unwrap().clone();
    vec![
        ("open", open, open_calls),
        ("comment", comment, comment_calls),
        ("edit", edit, edit_calls),
    ]
}

#[test]
fn every_failure_of_a_write_is_its_named_outcome_sent_once_and_echoes_no_input() {
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
        (failed("secondary-limit.stderr"), spent.clone()),
        (failed("rate-limited.stderr"), spent.clone()),
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
        // The relay's runner's reasons, gh's words after its exit status.
        ("gh exited 1: HTTP 404".to_string(), Outcome::NotFound),
        ("gh exited 1: HTTP 410".to_string(), Outcome::NotFound),
        ("gh exited 1: HTTP 403".to_string(), Outcome::Forbidden),
        ("gh exited 1: HTTP 401".to_string(), Outcome::SignedOut),
        (
            "gh exited 1: secondary rate limit".to_string(),
            spent.clone(),
        ),
        ("gh exited 1: HTTP 429".to_string(), spent.clone()),
        (
            "gh exited 1: HTTP 422".to_string(),
            Outcome::Refused(Refusal::Invalid),
        ),
        ("gh exited 1: HTTP 502".to_string(), maybe(Passing::Server)),
        ("gh exited 124".to_string(), maybe(Passing::Timeout)),
        (
            "gh exited 1: no such host".to_string(),
            Outcome::Unavailable {
                why: Passing::Unreachable,
                sent: Sent::No,
            },
        ),
    ];
    let body = big_body();
    for (why, want) in cases {
        for (op, got, calls) in each_write(&Err(why.clone())) {
            assert_eq!(got, Err(want.clone()), "{op}: {why}");
            // Sent once: the only call with an input. An edit that may have
            // landed reads once more to settle, and sends nothing again.
            let writes = calls.iter().filter(|c| c.input.is_some()).count();
            assert_eq!(writes, 1, "{op}: {why}");
            let said = format!("{got:?} {}", got.as_ref().unwrap_err().line());
            assert!(
                !said.contains(&body[..64]) && !said.contains(TITLE),
                "{op}: {said}"
            );
        }
    }
}

#[test]
fn an_answer_that_does_not_read_is_unread_never_a_success() {
    let created = sample("issue-created.json");
    let cut = created[..created.len() / 2].to_string();
    for answer in ["not json".to_string(), String::new(), "{}".to_string(), cut] {
        for (op, got, _) in each_write(&Ok(answer.clone())) {
            assert_eq!(
                got,
                Err(Outcome::Unread(Unread::Malformed)),
                "{op}: {answer:.40}"
            );
        }
    }
}

#[test]
fn a_link_that_is_not_what_was_written_is_inconsistent() {
    let other =
        sample("issue-created.json").replace("acme/widget/issues/42", "acme/other/issues/42");
    let (mut gh, _) = adapter(vec![Ok(other)]);
    assert_eq!(
        gh.open(&new_blueprint("t", "b")),
        Err(Outcome::Unread(Unread::Inconsistent))
    );
    // A comment answered on another issue, or on a pull request.
    for (from, to) in [("issues/7#", "issues/9#"), ("issues/7#", "pull/7#")] {
        let answer = sample("comment-created.json").replace(from, to);
        let (mut gh, _) = adapter(vec![Ok(sample("issue-7.json")), Ok(answer)]);
        assert_eq!(
            gh.comment(no(7), "b", &[]),
            Err(Outcome::Unread(Unread::Inconsistent)),
            "{to}"
        );
    }
    // An edit answered with another issue.
    let read = sample("issue-7.json");
    let since = revision_of(&read);
    let (mut gh, _) = adapter(vec![Ok(read), Ok(sample("issue-created.json"))]);
    assert_eq!(
        gh.edit(no(7), Some("t"), None, &MarkerEdit::default(), &since),
        Err(Outcome::Unread(Unread::Inconsistent))
    );
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
fn a_runner_that_takes_no_input_sends_nothing_and_never_puts_the_text_in_an_argument() {
    let calls = Calls::default();
    let runner = NoInput {
        calls: Arc::clone(&calls),
    };
    let mut gh = GitHub::new(PROJECT, Box::new(runner));
    let unsupported = Err(Outcome::Refused(Refusal::Unsupported));
    assert_eq!(gh.open(&new_blueprint(TITLE, "b")).map(|_| ()), unsupported);
    assert_eq!(gh.comment(no(7), "b", &[]).map(|_| ()), unsupported);
    // The comment's read alone: no write went out.
    assert_eq!(
        *calls.lock().unwrap(),
        [Call {
            args: args(&["api", "repos/acme/widget/issues/7"]),
            input: None
        }]
    );
}

#[test]
fn the_unbuilt_writes_are_refused_before_any_request() {
    let (mut gh, calls) = adapter(Vec::new());
    let unsupported = Err(Outcome::Refused(Refusal::Unsupported));
    let id = CommentId::new("1");
    let edit = gh.edit_comment(&id, "b", &MarkerEdit::default(), &Revision::new("r"));
    assert_eq!(edit.map(|_| ()), unsupported);
    assert_eq!(gh.close(no(7), true), unsupported);
    assert!(calls.lock().unwrap().is_empty());
}

/// Issue 7 as a site keeps it: a read answers its title and body, a
/// `PATCH` changes them.
#[derive(Default)]
struct Kept {
    title: String,
    body: String,
}

struct Site {
    kept: Arc<Mutex<Kept>>,
}

impl GhRunner for Site {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        // The read's question of the body's last edit: never edited.
        if args.starts_with(&["api", "graphql"]) && args.contains(&"number=7") {
            return Ok(
                r#"{"data":{"repository":{"issue":{"lastEditedAt":null,"editor":null}}}}"#.into(),
            );
        }
        if args != ["api", "repos/acme/widget/issues/7"] {
            return Err(format!("no recording of {args:?}"));
        }
        let k = self.kept.lock().unwrap();
        Ok(issue_7(&k.title, &k.body))
    }

    fn run_input(&mut self, args: &[&str], input: &str) -> Result<String, String> {
        if args
            != [
                "api",
                "-X",
                "PATCH",
                "repos/acme/widget/issues/7",
                "--input",
                "-",
            ]
        {
            return Err(format!("no recording of {args:?}"));
        }
        let sent: Input = serde_json::from_str(input).map_err(|_| "no JSON".to_string())?;
        let mut k = self.kept.lock().unwrap();
        if let Some(t) = sent.title {
            k.title = t;
        }
        if let Some(b) = sent.body {
            k.body = b;
        }
        Ok(issue_7(&k.title, &k.body))
    }
}

#[test]
fn the_law_an_edit_moves_the_revision_holds_over_the_adapters_edit() {
    let kept = Arc::new(Mutex::new(Kept {
        title: "Plan: the widget's settings page".into(),
        body: "One page for every setting.".into(),
    }));
    let site = Site {
        kept: Arc::clone(&kept),
    };
    let mut h = GitHub::new(PROJECT, Box::new(site)).on_clock(now);
    laws::blueprints::an_edit_moves_the_revision(&mut h, no(7)).unwrap();
    assert_eq!(
        kept.lock().unwrap().body,
        "One page for every setting.\n\nEdited."
    );
}

/// An edit whose `PATCH` failed with `why`, then settled by one read that
/// answers `again`: what it answered, and its calls.
fn settled(why: &str, again: Result<String, String>) -> (Got<WebLink>, Vec<Call>) {
    let read = sample("issue-7.json");
    let since = revision_of(&read);
    let (mut gh, calls) = adapter(vec![Ok(read), Err(why.to_string()), again]);
    let got = gh.edit(
        no(7),
        None,
        Some("New body."),
        &MarkerEdit::default(),
        &since,
    );
    let calls = calls.lock().unwrap().clone();
    (got, calls)
}

#[test]
fn an_edit_that_may_have_landed_is_settled_by_one_read_and_never_sent_again() {
    let read = sample("issue-7.json");
    let v: Value = serde_json::from_str(&read).unwrap();
    let title = v["title"].as_str().unwrap();
    let maybe = |why| Outcome::Unavailable {
        why,
        sent: Sent::Maybe,
    };
    let not_sent = |why| Outcome::Unavailable {
        why,
        sent: Sent::No,
    };
    for (why, passing) in [
        (failed("server-error.stderr"), Passing::Server),
        (BOUND.to_string(), Passing::Timeout),
        ("gh exited 124".to_string(), Passing::Timeout),
    ] {
        // At the revision sent: it landed. The title not given is the old one.
        let (got, calls) = settled(&why, Ok(issue_7(title, "New body.")));
        assert_eq!(
            got.unwrap().as_str(),
            "https://github.com/acme/widget/issues/7",
            "{why}"
        );
        assert_eq!(calls.len(), 3, "{why}");
        assert_eq!(calls[2].args, args(&["api", "repos/acme/widget/issues/7"]));
        assert_eq!(calls.iter().filter(|c| c.input.is_some()).count(), 1);
        // Still at the old revision: it was not sent.
        let (got, _) = settled(&why, Ok(read.clone()));
        assert_eq!(got, Err(not_sent(passing)), "{why}");
        // Changed to something else: it may have been sent.
        let (got, _) = settled(&why, Ok(issue_7(title, "A third body.")));
        assert_eq!(got, Err(maybe(passing)), "{why}");
        // The settling read fails: still maybe.
        let (got, calls) = settled(&why, Err(failed("server-error.stderr")));
        assert_eq!(got, Err(maybe(passing)), "{why}");
        assert_eq!(calls.len(), 3, "{why}");
    }
}

#[test]
fn an_edit_refused_for_sure_is_not_settled() {
    for why in [failed("forbidden.stderr"), failed("validation.stderr")] {
        let (got, calls) = settled(&why, Ok(sample("issue-7.json")));
        assert!(got.is_err(), "{why}");
        assert_eq!(calls.len(), 2, "no settling read: {why}");
    }
}

#[test]
fn an_open_or_a_comment_that_may_have_landed_stays_maybe() {
    let maybe = Err(Outcome::Unavailable {
        why: Passing::Server,
        sent: Sent::Maybe,
    });
    for (op, got, calls) in each_write(&Err(failed("server-error.stderr"))) {
        if op != "edit" {
            assert_eq!(got, maybe, "{op}");
            assert!(calls.last().unwrap().input.is_some(), "{op}: no read after");
        }
    }
}

/// A tickets port read of issue 7 answered `why`: the adapter's one reading
/// of a failed call, `GitHub::outcome`.
fn read_outcome(why: &str) -> Outcome {
    let (mut gh, _) = adapter(vec![Err(why.to_string())]);
    let key = tehas_ports::TicketKey::Number(std::num::NonZeroU64::new(7).unwrap());
    tehas_ports::Tickets::several(&mut gh, &[key])
        .remove(0)
        .1
        .unwrap_err()
}

#[test]
fn every_reason_bounded_gives_maps_as_before_the_relays_runner() {
    let maybe = |why| Outcome::Unavailable {
        why,
        sent: Sent::Maybe,
    };
    let unreachable = Outcome::Unavailable {
        why: Passing::Unreachable,
        sent: Sent::No,
    };
    let spent = Outcome::Spent {
        pool: Pool::Calls,
        until: At::new(now()),
    };
    let cases = [
        ("not found", Outcome::NotFound),
        (": HTTP 401", Outcome::SignedOut),
        (": HTTP 403", Outcome::Forbidden),
        (": HTTP 404", Outcome::NotFound),
        (": HTTP 410", Outcome::NotFound),
        // GitHub refusing the input made nothing (T4.7): refused, never a
        // write that may have been sent.
        (": HTTP 422", Outcome::Refused(Refusal::Invalid)),
        (": HTTP 429", spent.clone()),
        (": HTTP 500", maybe(Passing::Server)),
        (": HTTP 502", maybe(Passing::Server)),
        (": rate limit", spent.clone()),
        (": secondary rate limit", spent),
        (": timed out", maybe(Passing::Timeout)),
        (": timeout", maybe(Passing::Timeout)),
        (BOUND, maybe(Passing::Timeout)),
        (": connection refused", unreachable.clone()),
        (": no such host", unreachable.clone()),
        (": connection reset", maybe(Passing::Unreachable)),
        // Bounded lower-cases its words, which the reading's "unexpected
        // EOF" does not match: as before, a server failure.
        (": unexpected eof", maybe(Passing::Server)),
        (": gh exited 1", maybe(Passing::Server)),
        (": gh exited 124", maybe(Passing::Server)),
        (": gh was stopped by a signal", maybe(Passing::Server)),
        ("gh's answer could not be read", maybe(Passing::Server)),
        (
            "gh could not be run: No such file or directory",
            unreachable,
        ),
    ];
    for (why, want) in cases {
        assert_eq!(read_outcome(why), want, "{why}");
    }
}

/// A body holding a marker that reads and one that does not (a field with
/// a leading zero).
const MARKED: &str = "The plan.\n<!-- tehas:hold-cleared -->\n<!-- tehas:park issue=07 step=work kind=stopped provider=0 sig=0000000000000000000000000000000000000000000000000000000000000000 -->";

#[test]
fn the_law_an_unreadable_marker_is_kept_answers_unsupported_and_writes_nothing() {
    // Today's honest answer: with no marker writer, an edit of a body that
    // holds markers is refused, nothing is written, and the markers stay.
    // The marker writer will replace the refusal, and the law then holds.
    let kept = Arc::new(Mutex::new(Kept {
        title: "Plan".into(),
        body: MARKED.into(),
    }));
    let site = Site {
        kept: Arc::clone(&kept),
    };
    let mut h = GitHub::new(PROJECT, Box::new(site)).on_clock(now);
    let law = laws::markers::an_unreadable_marker_is_kept(&mut h, no(7));
    assert_eq!(
        law,
        Err(laws::Broken {
            law: "markers::an_unreadable_marker_is_kept".into(),
            what: format!(
                "the edit: {}",
                Outcome::Refused(Refusal::Unsupported).line()
            ),
        })
    );
    assert_eq!(kept.lock().unwrap().body, MARKED);
}

#[test]
fn the_revision_is_over_the_raw_body_markers_in() {
    let raw = issue_7("Plan", MARKED);
    let sums = format!(
        "{}:{}",
        tehas_core::approval::title_sum(Some("Plan")).unwrap(),
        tehas_core::approval::body_sum(MARKED)
    );
    assert_eq!(revision_of(&raw), Revision::new(sums));
    // A revision over the body without its markers is another revision,
    // and an edit at it is a conflict: `blueprint` must answer the raw one.
    let stripped = revision_of(&issue_7("Plan", "The plan."));
    assert_ne!(stripped, revision_of(&raw));
    let (mut gh, _) = adapter(vec![Ok(raw.clone())]);
    let got = gh.edit(no(7), Some("x"), None, &MarkerEdit::default(), &stripped);
    assert_eq!(got, Err(Outcome::Conflict(Lost::Changed)));
    let (mut gh, _) = adapter(vec![Ok(raw.clone()), Ok(raw.clone())]);
    let got = gh.edit(
        no(7),
        Some("x"),
        None,
        &MarkerEdit::default(),
        &revision_of(&raw),
    );
    assert!(got.is_ok(), "{got:?}");
}

/// What each of the three writes answers when its write call fails with
/// `why` (after a good read where it reads first), and the edit's read when
/// that read fails with it, with the calls of each.
fn every_way(why: &str) -> Vec<(&'static str, Got<()>, Vec<Call>)> {
    let mut all = each_write(&Err(why.to_string()));
    let since = revision_of(&sample("issue-7.json"));
    let (mut gh, calls) = adapter(vec![Err(why.to_string())]);
    let read = gh
        .edit(no(7), Some(TITLE), None, &MarkerEdit::default(), &since)
        .map(|_| ());
    all.push(("the edit's read", read, calls.lock().unwrap().clone()));
    all
}

#[test]
fn a_runner_that_gave_up_before_starting_gh_sent_nothing_and_nothing_is_settled() {
    let not_sent = Err(Outcome::Unavailable {
        why: Passing::Timeout,
        sent: Sent::No,
    });
    for (op, got, calls) in every_way("not started: out of time") {
        assert_eq!(got, not_sent, "{op}");
        // No read after the write to settle it: nothing was sent.
        let last = calls.last().unwrap();
        assert!(op == "the edit's read" || last.input.is_some(), "{op}");
        assert!(
            calls.iter().filter(|c| c.input.is_some()).count() <= 1,
            "{op}"
        );
    }
    // Only that exact form: another sentence with the same start is not it.
    for (op, got, _) in every_way("not started: something else") {
        assert_ne!(got, not_sent, "{op}");
    }
}

#[test]
fn the_relay_runners_network_failures_keep_their_meaning() {
    // gh's own network time limit: the request may have gone out.
    for (op, got, _) in every_way("gh exited 1: timed out") {
        let sent = if op == "the edit's read" {
            Sent::No
        } else {
            Sent::Maybe
        };
        assert_eq!(
            got,
            Err(Outcome::Unavailable {
                why: Passing::Timeout,
                sent
            }),
            "{op}"
        );
    }
    // Refused at the connection: nothing went out.
    for (op, got, _) in every_way("gh exited 1: connection refused") {
        assert_eq!(
            got,
            Err(Outcome::Unavailable {
                why: Passing::Unreachable,
                sent: Sent::No
            }),
            "{op}"
        );
    }
    // The relay's bound stopped gh: it may have sent.
    for (op, got, _) in every_way("gh exited 124") {
        let sent = if op == "the edit's read" {
            Sent::No
        } else {
            Sent::Maybe
        };
        assert_eq!(
            got,
            Err(Outcome::Unavailable {
                why: Passing::Timeout,
                sent
            }),
            "{op}"
        );
    }
}

#[test]
fn what_today_answers_for_the_relays_own_sentence() {
    // The relay's own words are no form the adapter knows: read as a server
    // failure that may have been sent. The relay returns
    // "not started: out of time" instead.
    for (op, got, _) in every_way("the relay's 100 s are up") {
        let sent = if op == "the edit's read" {
            Sent::No
        } else {
            Sent::Maybe
        };
        assert_eq!(
            got,
            Err(Outcome::Unavailable {
                why: Passing::Server,
                sent
            }),
            "{op}"
        );
    }
}
