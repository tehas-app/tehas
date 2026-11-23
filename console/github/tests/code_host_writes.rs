//! The code host port's writes as the adapter answers them:
//! `comment_change`, `mark_change` and `unmark_change`, over samples of
//! GitHub's answers in `tests/samples/code-host-writes/` (hand-made from
//! GitHub's documented shapes, no secret and no private name). A failure
//! sample is gh's error stream, given to the adapter as `Bounded` would
//! give it. No test calls GitHub.
//!
//! The failure cases assume the base's reading of a runner's reason
//! (`tickets.rs`'s `outcome`): an HTTP status at its start, a time limit by
//! `Bounded`'s words.

use serde::Deserialize;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tehas_core::forge::GhRunner;
use tehas_github::{table, GitHub};
use tehas_ports::{
    Act, Bound, ChangeNo, CodeHost, CodeHostWrites, CommitId, Mark, MarkName, Marker, MarkerKind,
    Outcome, Passing, Pool, Refusal, Sent, State, TagName, Unread,
};

const PROJECT: &str = "acme/widget";

/// The words `Bounded` gives for a gh it stopped at its bound.
const BOUND: &str = "gh ran out of its 30s bound";

/// What `GhRunner::run_input`'s default answers.
const NO_INPUT: &str = "this runner takes no input";

fn sample(name: &str) -> String {
    let at = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/samples/code-host-writes");
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

fn no(n: u64) -> ChangeNo {
    ChangeNo::new(n).unwrap()
}

fn refreshing() -> Mark {
    Mark::State(State::Refreshing)
}

/// The label of a mark, from the adapter's table.
fn label(mark: &Mark) -> String {
    table::label(mark).unwrap()
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

/// A runner that keeps `GhRunner::run_input`'s default: it takes no input.
struct NoInput {
    answers: VecDeque<Result<String, String>>,
    calls: Calls,
}

impl GhRunner for NoInput {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        self.calls.lock().unwrap().push(Call {
            args: args.iter().map(|a| a.to_string()).collect(),
            input: None,
        });
        self.answers
            .pop_front()
            .unwrap_or_else(|| Err("no answer".into()))
    }
}

fn adapter(answers: Vec<Result<String, String>>) -> (GitHub, Calls) {
    adapter_of(PROJECT, answers)
}

fn adapter_of(project: &str, answers: Vec<Result<String, String>>) -> (GitHub, Calls) {
    let calls = Calls::default();
    let script = Script {
        answers: answers.into(),
        calls: Arc::clone(&calls),
    };
    let gh = GitHub::new(project, Box::new(script)).on_clock(now);
    (gh, calls)
}

fn taken(calls: &Calls) -> Vec<Call> {
    calls.lock().unwrap().clone()
}

fn args(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| w.to_string()).collect()
}

const READ: [&str; 2] = ["api", "repos/acme/widget/pulls/8"];
const POST: [&str; 6] = [
    "api",
    "-X",
    "POST",
    "repos/acme/widget/issues/8/comments",
    "--input",
    "-",
];

fn add(label: &str) -> Vec<String> {
    args(&[
        "api",
        "-X",
        "POST",
        "repos/acme/widget/issues/8/labels",
        "-f",
        &format!("labels[]={label}"),
    ])
}

fn remove(encoded: &str) -> Vec<String> {
    args(&[
        "api",
        "-X",
        "DELETE",
        &format!("repos/acme/widget/issues/8/labels/{encoded}"),
    ])
}

/// A body of 200 KiB, past what one argument may hold, and unlike any
/// other text of a test.
fn big_body() -> String {
    let line = "A line of the refresh report that only this body holds, number ";
    let mut body = String::new();
    let mut i = 0;
    while body.len() < 200 * 1024 {
        body.push_str(&format!("{line}{i}.\n"));
        i += 1;
    }
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

/// What a comment sent on standard input: its body and nothing else.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    body: String,
}

fn maybe(why: Passing) -> Outcome {
    Outcome::Unavailable {
        why,
        sent: Sent::Maybe,
    }
}

fn not_sent(why: Passing) -> Outcome {
    Outcome::Unavailable {
        why,
        sent: Sent::No,
    }
}

fn spent_writes() -> Outcome {
    Outcome::Spent {
        pool: Pool::Writes,
        until: tehas_ports::At::new(now()),
    }
}

fn spent_calls() -> Outcome {
    Outcome::Spent {
        pool: Pool::Calls,
        until: tehas_ports::At::new(now()),
    }
}

/// Each failure of a write, as the runner gives it, and its outcome.
fn write_failures() -> Vec<(&'static str, String, Outcome)> {
    vec![
        ("not found", failed("not-found.stderr"), Outcome::NotFound),
        ("gone", failed("gone.stderr"), Outcome::NotFound),
        ("forbidden", failed("forbidden.stderr"), Outcome::Forbidden),
        (
            "signed out",
            failed("signed-out.stderr"),
            Outcome::SignedOut,
        ),
        ("rate limit", failed("rate-limited.stderr"), spent_writes()),
        (
            "secondary limit",
            failed("secondary-limit.stderr"),
            spent_writes(),
        ),
        (
            "422",
            failed("validation.stderr"),
            Outcome::Refused(Refusal::Invalid),
        ),
        ("5xx", failed("server-error.stderr"), maybe(Passing::Server)),
        ("timeout", BOUND.to_string(), maybe(Passing::Timeout)),
        (
            "unreachable",
            failed("unreachable.stderr"),
            not_sent(Passing::Unreachable),
        ),
    ]
}

/// Each failure of the read before a write, and its outcome: nothing was
/// sent.
fn read_failures() -> Vec<(&'static str, String, Outcome)> {
    vec![
        ("not found", failed("not-found.stderr"), Outcome::NotFound),
        ("gone", failed("gone.stderr"), Outcome::NotFound),
        ("forbidden", failed("forbidden.stderr"), Outcome::Forbidden),
        (
            "signed out",
            failed("signed-out.stderr"),
            Outcome::SignedOut,
        ),
        ("rate limit", failed("rate-limited.stderr"), spent_calls()),
        (
            "secondary limit",
            failed("secondary-limit.stderr"),
            spent_calls(),
        ),
        (
            "5xx",
            failed("server-error.stderr"),
            not_sent(Passing::Server),
        ),
        ("timeout", BOUND.to_string(), not_sent(Passing::Timeout)),
        (
            "unreachable",
            failed("unreachable.stderr"),
            not_sent(Passing::Unreachable),
        ),
    ]
}

/// Answers of the read that are no pull request of this project.
fn unread_pulls() -> Vec<(&'static str, String, Outcome)> {
    vec![
        (
            "not json",
            "<html>".to_string(),
            Outcome::Unread(Unread::Malformed),
        ),
        (
            "no labels",
            sample("pull-8-hollow.json"),
            Outcome::Unread(Unread::Malformed),
        ),
        (
            "another project",
            sample("pull-8-other-project.json"),
            Outcome::Unread(Unread::Inconsistent),
        ),
        (
            "another number",
            sample("pull-8-open.json").replace(r#""number":8"#, r#""number":9"#),
            Outcome::Unread(Unread::Inconsistent),
        ),
    ]
}

// The handle the cycle's writer holds.

#[test]
fn the_adapter_is_a_code_host_writes_handle_a_thread_can_take() {
    fn send<T: Send>(_: &T) {}
    let (gh, _) = adapter(vec![]);
    let handle: Box<dyn CodeHostWrites + Send> = Box::new(gh);
    send(&handle);
}

// comment_change

#[test]
fn a_comment_sends_a_200_kib_body_on_standard_input_and_answers_its_id_and_link() {
    let body = big_body();
    let (mut gh, calls) = adapter(vec![
        Ok(sample("pull-8-open.json")),
        Ok(sample("comment-8-created.json")),
    ]);
    let posted = gh.comment_change(no(8), &body, &[]).unwrap();
    assert_eq!(posted.id.as_str(), "2502345678");
    assert_eq!(
        posted.link.as_str(),
        "https://github.com/acme/widget/pull/8#issuecomment-2502345678"
    );
    let calls = taken(&calls);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].args, args(&READ));
    assert_eq!(calls[0].input, None);
    assert_eq!(calls[1].args, args(&POST));
    kept_out_of_argv(&calls, &body);
    let sent: Input = serde_json::from_str(calls[1].input.as_deref().unwrap()).unwrap();
    assert_eq!(sent, Input { body });
}

#[test]
fn a_comment_on_a_closed_or_merged_change_is_posted() {
    let (mut gh, calls) = adapter(vec![
        Ok(sample("pull-8-merged.json")),
        Ok(sample("comment-8-created.json")),
    ]);
    assert!(gh.comment_change(no(8), "Merged.", &[]).is_ok());
    assert_eq!(taken(&calls).len(), 2);
}

#[test]
fn a_comment_on_an_issues_number_or_none_is_not_found_and_nothing_is_posted() {
    // GitHub's pulls read answers 404 for an issue's number.
    let (mut gh, calls) = adapter(vec![Err(failed("not-found.stderr"))]);
    assert_eq!(
        gh.comment_change(no(7), "A note.", &[]),
        Err(Outcome::NotFound)
    );
    let calls = taken(&calls);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].args, args(&["api", "repos/acme/widget/pulls/7"]));
}

#[test]
fn a_comment_with_markers_is_refused_before_any_request() {
    let (mut gh, calls) = adapter(vec![]);
    let marker = Marker::Unreadable {
        kind: MarkerKind::Question,
    };
    assert_eq!(
        gh.comment_change(no(8), "A note.", &[marker]),
        Err(Outcome::Refused(Refusal::Unsupported))
    );
    assert_eq!(
        gh.comment_change(no(8), "Text <!-- tehas:question 7 --> more", &[]),
        Err(Outcome::Refused(Refusal::Invalid))
    );
    assert!(taken(&calls).is_empty());
}

#[test]
fn each_failure_of_a_comments_post_is_its_outcome_and_it_is_never_sent_again() {
    let mut cases = write_failures();
    cases.push((
        "no input",
        NO_INPUT.to_string(),
        Outcome::Refused(Refusal::Unsupported),
    ));
    for (name, why, want) in cases {
        let body = "The refresh is done; a body this test alone holds.";
        let (mut gh, calls) = adapter(vec![Ok(sample("pull-8-open.json")), Err(why)]);
        assert_eq!(gh.comment_change(no(8), body, &[]), Err(want), "{name}");
        assert_eq!(taken(&calls).len(), 2, "{name}");
    }
}

#[test]
fn a_runner_that_takes_no_input_sends_no_comment() {
    let calls = Calls::default();
    let runner = NoInput {
        answers: vec![Ok(sample("pull-8-open.json"))].into(),
        calls: Arc::clone(&calls),
    };
    let mut gh = GitHub::new(PROJECT, Box::new(runner)).on_clock(now);
    assert_eq!(
        gh.comment_change(no(8), "A note.", &[]),
        Err(Outcome::Refused(Refusal::Unsupported))
    );
    assert_eq!(taken(&calls).len(), 1);
}

#[test]
fn each_failure_of_the_read_before_a_comment_sends_nothing() {
    for (name, why, want) in read_failures() {
        let (mut gh, calls) = adapter(vec![Err(why)]);
        assert_eq!(
            gh.comment_change(no(8), "A note.", &[]),
            Err(want),
            "{name}"
        );
        assert_eq!(taken(&calls).len(), 1, "{name}");
    }
    for (name, out, want) in unread_pulls() {
        let (mut gh, calls) = adapter(vec![Ok(out)]);
        assert_eq!(
            gh.comment_change(no(8), "A note.", &[]),
            Err(want),
            "{name}"
        );
        assert_eq!(taken(&calls).len(), 1, "{name}");
    }
}

#[test]
fn a_comments_answer_that_does_not_read_is_unread() {
    for (name, out, want) in [
        (
            "not json",
            "".to_string(),
            Outcome::Unread(Unread::Malformed),
        ),
        (
            "no id",
            sample("comment-8-created.json").replace(r#""id":2502345678,"#, ""),
            Outcome::Unread(Unread::Malformed),
        ),
        (
            "another project",
            sample("comment-8-other-project.json"),
            Outcome::Unread(Unread::Inconsistent),
        ),
        (
            "another change",
            sample("comment-8-created.json").replace("/pull/8#", "/pull/9#"),
            Outcome::Unread(Unread::Inconsistent),
        ),
    ] {
        let (mut gh, calls) = adapter(vec![Ok(sample("pull-8-open.json")), Ok(out)]);
        assert_eq!(
            gh.comment_change(no(8), "A note.", &[]),
            Err(want),
            "{name}"
        );
        assert_eq!(taken(&calls).len(), 2, "{name}");
    }
}

#[test]
fn a_project_name_that_would_aim_elsewhere_is_refused_before_any_request() {
    for project in [
        "acme",
        "acme/../other",
        "acme/widget/issues",
        "ac me/widget",
    ] {
        let (mut gh, calls) = adapter_of(project, vec![]);
        assert_eq!(
            gh.comment_change(no(8), "A note.", &[]),
            Err(Outcome::Refused(Refusal::Invalid)),
            "{project}"
        );
        assert_eq!(
            gh.mark_change(no(8), &refreshing()),
            Err(Outcome::Refused(Refusal::Invalid)),
            "{project}"
        );
        assert!(taken(&calls).is_empty(), "{project}");
    }
}

// mark_change and unmark_change

#[test]
fn a_mark_is_its_tables_label_added_to_the_open_change() {
    let mark = refreshing();
    let (mut gh, calls) = adapter(vec![
        Ok(sample("pull-8-open.json")),
        Ok(sample("labels-8-added.json")),
    ]);
    assert_eq!(gh.mark_change(no(8), &mark), Ok(()));
    let calls = taken(&calls);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].args, args(&READ));
    assert_eq!(calls[1].args, add(&label(&mark)));
    assert_eq!(calls[1].input, None);
}

#[test]
fn a_mark_the_change_carries_is_done_with_nothing_sent() {
    let (mut gh, calls) = adapter(vec![Ok(sample("pull-8-refreshing.json"))]);
    assert_eq!(gh.mark_change(no(8), &refreshing()), Ok(()));
    assert_eq!(taken(&calls).len(), 1);
}

#[test]
fn a_mark_on_a_merged_change_is_refused_with_nothing_sent() {
    let (mut gh, calls) = adapter(vec![Ok(sample("pull-8-merged.json"))]);
    assert_eq!(
        gh.mark_change(no(8), &Mark::Act(Act::Pause)),
        Err(Outcome::Refused(Refusal::Invalid))
    );
    assert_eq!(taken(&calls).len(), 1);
}

#[test]
fn a_mark_no_label_carries_is_refused_before_any_request() {
    let (mut gh, calls) = adapter(vec![]);
    assert_eq!(
        gh.mark_change(no(8), &Mark::Unknown),
        Err(Outcome::Refused(Refusal::Invalid))
    );
    assert_eq!(
        gh.unmark_change(no(8), &Mark::Unknown),
        Err(Outcome::Refused(Refusal::Invalid))
    );
    if let Ok(spaced) = MarkName::new("two words") {
        assert_eq!(
            gh.mark_change(no(8), &Mark::Project(spaced)),
            Err(Outcome::Refused(Refusal::Invalid))
        );
    }
    assert!(taken(&calls).is_empty());
}

#[test]
fn a_projects_own_mark_is_its_name() {
    let mark = Mark::Project(MarkName::new("area-ui").unwrap());
    let (mut gh, calls) = adapter(vec![Ok(sample("pull-8-open.json")), Ok("[]".into())]);
    assert_eq!(gh.mark_change(no(8), &mark), Ok(()));
    assert_eq!(taken(&calls)[1].args, add("area-ui"));
}

#[test]
fn a_mark_or_unmark_of_an_issues_number_is_not_found_and_nothing_is_sent() {
    for unmark in [false, true] {
        let (mut gh, calls) = adapter(vec![Err(failed("not-found.stderr"))]);
        let got = if unmark {
            gh.unmark_change(no(7), &refreshing())
        } else {
            gh.mark_change(no(7), &refreshing())
        };
        assert_eq!(got, Err(Outcome::NotFound), "unmark {unmark}");
        assert_eq!(taken(&calls).len(), 1, "unmark {unmark}");
    }
}

#[test]
fn an_unmark_takes_the_tables_label_off_the_change_open_or_not() {
    for pull in ["pull-8-refreshing.json", "pull-8-merged.json"] {
        let (mut gh, calls) = adapter(vec![Ok(sample(pull)), Ok(String::new())]);
        assert_eq!(gh.unmark_change(no(8), &refreshing()), Ok(()), "{pull}");
        let calls = taken(&calls);
        assert_eq!(calls.len(), 2, "{pull}");
        let encoded = label(&refreshing()).replace(':', "%3A");
        assert_eq!(calls[1].args, remove(&encoded), "{pull}");
    }
}

#[test]
fn removing_a_mark_the_change_does_not_carry_is_done() {
    // Not on the change: nothing is sent.
    let (mut gh, calls) = adapter(vec![Ok(sample("pull-8-open.json"))]);
    assert_eq!(gh.unmark_change(no(8), &refreshing()), Ok(()));
    assert_eq!(taken(&calls).len(), 1);
    // Gone between the read and the removal: the 404 is the state wanted.
    let (mut gh, calls) = adapter(vec![
        Ok(sample("pull-8-refreshing.json")),
        Err(failed("not-found.stderr")),
    ]);
    assert_eq!(gh.unmark_change(no(8), &refreshing()), Ok(()));
    assert_eq!(taken(&calls).len(), 2);
}

#[test]
fn each_failure_of_a_marks_write_is_its_outcome_and_it_is_never_sent_again() {
    for (name, why, want) in write_failures() {
        // A label's write that may have landed is settled by one read; here
        // the read fails too, so the write's own outcome stands.
        let settles = matches!(
            want,
            Outcome::Unavailable {
                sent: Sent::Maybe,
                ..
            }
        );
        let mut answers = vec![Ok(sample("pull-8-open.json")), Err(why.clone())];
        if settles {
            answers.push(Err(failed("server-error.stderr")));
        }
        let (mut gh, calls) = adapter(answers);
        assert_eq!(
            gh.mark_change(no(8), &refreshing()),
            Err(want.clone()),
            "mark {name}"
        );
        let calls = taken(&calls);
        let writes = calls.iter().filter(|c| c.args.contains(&"POST".into()));
        assert_eq!(writes.count(), 1, "mark {name}");
        assert_eq!(calls.len(), 2 + settles as usize, "mark {name}");

        if name == "not found" {
            // A removal's 404 is done; above.
            continue;
        }
        let mut answers = vec![Ok(sample("pull-8-refreshing.json")), Err(why)];
        if settles {
            answers.push(Err(failed("server-error.stderr")));
        }
        let (mut gh, calls) = adapter(answers);
        assert_eq!(
            gh.unmark_change(no(8), &refreshing()),
            Err(want),
            "unmark {name}"
        );
        let calls = taken(&calls);
        let writes = calls.iter().filter(|c| c.args.contains(&"DELETE".into()));
        assert_eq!(writes.count(), 1, "unmark {name}");
    }
}

#[test]
fn each_failure_of_the_read_before_a_mark_sends_nothing() {
    for (name, why, want) in read_failures().into_iter().chain(
        unread_pulls()
            .into_iter()
            .map(|(n, out, o)| (n, format!("ok:{out}"), o)),
    ) {
        for unmark in [false, true] {
            let answer = match why.strip_prefix("ok:") {
                Some(out) => Ok(out.to_string()),
                None => Err(why.clone()),
            };
            let (mut gh, calls) = adapter(vec![answer]);
            let got = if unmark {
                gh.unmark_change(no(8), &refreshing())
            } else {
                gh.mark_change(no(8), &refreshing())
            };
            assert_eq!(got, Err(want.clone()), "{name}, unmark {unmark}");
            assert_eq!(taken(&calls).len(), 1, "{name}, unmark {unmark}");
        }
    }
}

#[test]
fn a_mark_that_may_have_landed_is_settled_by_one_read_and_never_sent_again() {
    // Timed out, and the read shows the label on: done.
    let (mut gh, calls) = adapter(vec![
        Ok(sample("pull-8-open.json")),
        Err(BOUND.into()),
        Ok(sample("pull-8-refreshing.json")),
    ]);
    assert_eq!(gh.mark_change(no(8), &refreshing()), Ok(()));
    let calls = taken(&calls);
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[2].args, args(&READ));
    // Timed out, and the read shows it still off: it may land yet.
    let (mut gh, calls) = adapter(vec![
        Ok(sample("pull-8-open.json")),
        Err(BOUND.into()),
        Ok(sample("pull-8-open.json")),
    ]);
    assert_eq!(
        gh.mark_change(no(8), &refreshing()),
        Err(maybe(Passing::Timeout))
    );
    assert_eq!(taken(&calls).len(), 3);
}

#[test]
fn an_unmark_that_may_have_landed_is_settled_by_one_read_and_never_sent_again() {
    let (mut gh, calls) = adapter(vec![
        Ok(sample("pull-8-refreshing.json")),
        Err(failed("server-error.stderr")),
        Ok(sample("pull-8-open.json")),
    ]);
    assert_eq!(gh.unmark_change(no(8), &refreshing()), Ok(()));
    assert_eq!(taken(&calls).len(), 3);
    let (mut gh, calls) = adapter(vec![
        Ok(sample("pull-8-refreshing.json")),
        Err(failed("server-error.stderr")),
        Ok(sample("pull-8-refreshing.json")),
    ]);
    assert_eq!(
        gh.unmark_change(no(8), &refreshing()),
        Err(maybe(Passing::Server))
    );
    assert_eq!(taken(&calls).len(), 3);
}

#[test]
fn a_comment_that_may_have_landed_stays_maybe_with_no_read_and_no_resend() {
    let (mut gh, calls) = adapter(vec![Ok(sample("pull-8-open.json")), Err(BOUND.into())]);
    assert_eq!(
        gh.comment_change(no(8), "A note.", &[]),
        Err(maybe(Passing::Timeout))
    );
    assert_eq!(taken(&calls).len(), 2);
}

#[test]
fn a_label_compares_without_case_as_github_does() {
    let upper = sample("pull-8-refreshing.json").replace(
        &format!(r#""name":"{}""#, label(&refreshing())),
        &format!(r#""name":"{}""#, label(&refreshing()).to_uppercase()),
    );
    let (mut gh, calls) = adapter(vec![Ok(upper)]);
    assert_eq!(gh.mark_change(no(8), &refreshing()), Ok(()));
    assert_eq!(taken(&calls).len(), 1);
}

#[test]
fn no_outcome_line_holds_the_input_or_githubs_words() {
    let body = "Only-this-test's body text";
    let mut seen = Vec::new();
    for (_, why, _) in write_failures() {
        let (mut gh, _) = adapter(vec![Ok(sample("pull-8-open.json")), Err(why)]);
        seen.push(gh.comment_change(no(8), body, &[]).unwrap_err());
    }
    for o in seen {
        let line = o.line();
        for word in [body, "HTTP", "gh", "Not Found", "GitHub", "acme"] {
            assert!(!line.contains(word), "{line}");
        }
    }
}

// What is not answered yet.

#[test]
fn every_other_write_and_every_read_is_unsupported_before_any_request() {
    let (mut gh, calls) = adapter(vec![]);
    let unsupported = Err(Outcome::Refused(Refusal::Unsupported));
    let head = CommitId::new("e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1").unwrap();
    assert_eq!(gh.close_change(no(8)), unsupported);
    assert_eq!(
        gh.merge(no(8), &head, tehas_ports::MergeHow::Squash),
        unsupported
    );
    let tag = TagName::new("v1.0.0").unwrap();
    assert_eq!(gh.tag(&tag, &head), unsupported);
    assert_eq!(
        gh.change(no(8), Bound::new(10).unwrap()).map(|_| ()),
        unsupported
    );
    assert_eq!(gh.facts().map(|_| ()), unsupported);
    assert_eq!(gh.resolve_tag(&tag).map(|_| ()), unsupported);
    assert!(taken(&calls).is_empty());
}
