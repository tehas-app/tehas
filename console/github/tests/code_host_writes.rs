//! The code host port's writes as the adapter answers them:
//! `comment_change`, `mark_change`, `unmark_change` and `set_status`, over samples of
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
    Act, ChangeNo, CodeHostWrites, CommitId, Mark, MarkName, Marker, MarkerKind, Outcome, Passing,
    Pool, Refusal, Sent, State, TagName, Unread,
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

/// The question of this project's name as the adapter asks it, once, when
/// an answer names another repository.
const NAME_QUESTION: [&str; 8] = [
    "api",
    "graphql",
    "-f",
    "owner=acme",
    "-f",
    "name=widget",
    "-f",
    "query=query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { databaseId nameWithOwner } }",
];

/// Each call as `name` where it is exactly that question, else `call`.
fn kinds(calls: &[Call]) -> Vec<&'static str> {
    calls
        .iter()
        .map(|c| match c.input.is_none() && c.args == NAME_QUESTION {
            true => "name",
            false => "call",
        })
        .collect()
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

/// An instance that still names the project's old name comments on its
/// pull request as here, by the name GitHub gives with the repository's id.
#[test]
fn a_renamed_project_still_gets_its_comment_on_a_change_by_its_repository_id() {
    let renamed = r#"{"data":{"repository":{"databaseId":7001,"nameWithOwner":"acme/widget"}}}"#;
    let (mut gh, calls) = adapter_of(
        "acme/widget-before",
        vec![
            Ok(sample("pull-8-open.json")),
            Ok(renamed.into()),
            Ok(sample("comment-8-created.json")),
        ],
    );
    let posted = gh.comment_change(no(8), "b", &[]).unwrap();
    assert_eq!(posted.id.as_str(), "2502345678");
    let calls = taken(&calls);
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[1].args[..2], args(&["api", "graphql"]));
    let other = sample("pull-8-open.json").replace("acme/widget/pull/8", "acme/other/pull/8");
    let (mut gh, calls) = adapter_of("acme/widget-before", vec![Ok(other), Ok(renamed.into())]);
    assert_eq!(
        gh.comment_change(no(8), "b", &[]),
        Err(Outcome::Unread(Unread::Inconsistent))
    );
    assert_eq!(taken(&calls).len(), 2, "nothing posted");
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

/// The handoff line pi-rukas starts a cycle's comment with: not a marker of
/// tehas's.
const HANDOFF: &str = "<!-- pi-rukas:handoff issue=7 cap=budget -->";

/// The comment `body` is posted once, and the JSON sent holds it byte for
/// byte.
fn posted_once(body: &str) {
    let (mut gh, calls) = adapter(vec![
        Ok(sample("pull-8-open.json")),
        Ok(sample("comment-8-created.json")),
    ]);
    assert!(gh.comment_change(no(8), body, &[]).is_ok(), "{body}");
    let calls = taken(&calls);
    let posts: Vec<_> = calls.iter().filter(|c| c.args == args(&POST)).collect();
    assert_eq!(posts.len(), 1);
    let sent: Input = serde_json::from_str(posts[0].input.as_deref().unwrap()).unwrap();
    assert_eq!(sent.body, body);
}

#[test]
fn a_comment_that_starts_with_pi_rukass_handoff_line_is_posted_as_given() {
    posted_once(&format!(
        "{HANDOFF}\nThe cycle handed off: the budget ran out."
    ));
}

#[test]
fn a_foreign_comment_that_names_tehas_later_inside_it_is_posted_as_given() {
    posted_once("<!-- pi-rukas:handoff issue=7 cap=budget, see tehas: the factory -->\nDone.");
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
        assert_eq!(kinds(&taken(&calls)), ["call", "call"], "{name}");
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
        let want: &[&str] = match name {
            "another project" => &["call", "name"],
            _ => &["call"],
        };
        assert_eq!(kinds(&taken(&calls)), want, "{name}");
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
        let want: &[&str] = match name {
            "another project" => &["call", "call", "name"],
            _ => &["call", "call"],
        };
        assert_eq!(kinds(&taken(&calls)), want, "{name}");
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
            let asked: &[&str] = match name {
                "another project" => &["call", "name"],
                _ => &["call"],
            };
            assert_eq!(kinds(&taken(&calls)), asked, "{name}, unmark {unmark}");
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

// What is not answered yet: the reads are code_host.rs's.

#[test]
fn every_other_write_is_unsupported_before_any_request() {
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
    assert!(taken(&calls).is_empty());
}

// --- set_status ---------------------------------------------------------------

const HEAD: &str = "6dcb09b5b57875f334f61aebed695e2e4193db5e";

fn status(state: tehas_ports::StatusState, about: &str) -> tehas_ports::Status {
    tehas_ports::Status {
        name: "tehas/plugin/review".into(),
        state,
        about: about.into(),
        link: Some(tehas_ports::WebLink::new("https://example.com/runs/1").unwrap()),
    }
}

/// The sample answer with `state`, the answer to a status of that state.
fn created(state: &str) -> String {
    sample("status-created.json").replace("\"success\"", &format!("\"{state}\""))
}

fn head() -> CommitId {
    CommitId::new(HEAD).unwrap()
}

#[test]
fn a_status_is_one_post_to_the_commit_its_state_in_githubs_word_and_its_text_as_input() {
    use tehas_ports::StatusState as S;
    for (state, word) in [
        (S::Pending, "pending"),
        (S::Passed, "success"),
        (S::Failed, "failure"),
        (S::Errored, "error"),
    ] {
        let (mut gh, calls) = adapter(vec![Ok(created(word))]);
        assert_eq!(gh.set_status(&head(), &status(state, "Passed.")), Ok(()));
        let taken = taken(&calls);
        assert_eq!(taken.len(), 1, "{word}: sent once");
        assert_eq!(
            taken[0].args,
            [
                "api",
                "-X",
                "POST",
                &format!("repos/acme/widget/statuses/{HEAD}"),
                "--input",
                "-"
            ]
        );
        let sent: serde_json::Value =
            serde_json::from_str(taken[0].input.as_deref().unwrap()).unwrap();
        assert_eq!(
            sent,
            serde_json::json!({
                "state": word,
                "context": "tehas/plugin/review",
                "description": "Passed.",
                "target_url": "https://example.com/runs/1",
            })
        );
    }
}

#[test]
fn a_status_with_no_link_sends_none_and_its_about_is_cut_to_what_github_keeps() {
    use tehas_ports::StatusState as S;
    let mut s = status(S::Passed, &"é".repeat(200));
    s.link = None;
    let (mut gh, calls) = adapter(vec![Ok(created("success"))]);
    assert_eq!(gh.set_status(&head(), &s), Ok(()));
    let sent: serde_json::Value =
        serde_json::from_str(taken(&calls)[0].input.as_deref().unwrap()).unwrap();
    assert_eq!(sent.get("target_url"), None);
    assert_eq!(sent["description"], "é".repeat(140));
}

#[test]
fn a_name_that_is_no_one_line_an_about_with_a_marker_or_a_project_that_is_no_name_sends_nothing() {
    use tehas_ports::StatusState as S;
    let invalid = Err(Outcome::Refused(Refusal::Invalid));
    let mut cases = Vec::new();
    for name in ["", "  ", "a\nb", "a\rb"] {
        let mut s = status(S::Passed, "Passed.");
        s.name = name.into();
        cases.push((PROJECT, s));
    }
    cases.push((
        PROJECT,
        status(S::Passed, "See <!-- tehas:question 7 --> here"),
    ));
    cases.push(("acme/widget/x", status(S::Passed, "Passed.")));
    cases.push(("acme/..", status(S::Passed, "Passed.")));
    for (project, s) in cases {
        let (mut gh, calls) = adapter_of(project, vec![Ok(created("success"))]);
        assert_eq!(gh.set_status(&head(), &s), invalid, "{project} {s:?}");
        assert_eq!(taken(&calls), vec![], "{project} {s:?}");
    }
}

#[test]
fn each_failure_of_the_post_is_its_outcome_and_it_is_never_sent_twice() {
    use tehas_ports::StatusState as S;
    for (what, why, want) in write_failures() {
        let (mut gh, calls) = adapter(vec![Err(why)]);
        assert_eq!(
            gh.set_status(&head(), &status(S::Failed, "Failed.")),
            Err(want),
            "{what}"
        );
        assert_eq!(taken(&calls).len(), 1, "{what}");
    }
}

#[test]
fn an_answer_that_is_not_the_status_sent_is_unread() {
    use tehas_ports::StatusState as S;
    let other = sample("status-created.json").replace("tehas/plugin/review", "ci/other");
    for (what, answer, want) in [
        ("not json", "<html>".to_string(), Unread::Malformed),
        (
            "no state",
            r#"{"context":"tehas/plugin/review"}"#.to_string(),
            Unread::Malformed,
        ),
        ("another name", other, Unread::Inconsistent),
        ("another state", created("pending"), Unread::Inconsistent),
    ] {
        let (mut gh, _) = adapter(vec![Ok(answer)]);
        assert_eq!(
            gh.set_status(&head(), &status(S::Passed, "Passed.")),
            Err(Outcome::Unread(want)),
            "{what}"
        );
    }
}

#[test]
fn a_runner_that_takes_no_input_sends_no_status() {
    let calls = Calls::default();
    let runner = NoInput {
        answers: VecDeque::new(),
        calls: Arc::clone(&calls),
    };
    let mut gh = GitHub::new(PROJECT, Box::new(runner)).on_clock(now);
    assert_eq!(
        gh.set_status(
            &head(),
            &status(tehas_ports::StatusState::Passed, "Passed.")
        ),
        Err(Outcome::Refused(Refusal::Unsupported))
    );
}
