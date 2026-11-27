//! `CodeHostWrites::open_change` and `CodeHost::remote` as the adapter
//! answers them, over hand-made samples of GitHub's answers in
//! `tests/samples/open-change/` (no secret, no private name) and the
//! failure samples of `tests/samples/code-host-writes/`, given to the
//! adapter as `Bounded` gives them. No test calls GitHub.

use serde::Deserialize;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tehas_core::forge::GhRunner;
use tehas_github::{GitHub, GitHubSite, MakeRunner};
use tehas_ports::{
    Access, Actor, BlueprintNo, Body, Branch, Caller, CodeHost, CodeHostWrites, NewChange, Outcome,
    Passing, Pool, Refusal, Sent, Sha256, Site, Sum, Unread,
};

const PROJECT: &str = "acme/widget";
const BOUND: &str = "gh ran out of its 30s bound";
const NO_INPUT: &str = "this runner takes no input";
const HEAD: &str = "tehas/finish-7";

fn sample(name: &str) -> String {
    let at = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/samples/open-change");
    std::fs::read_to_string(at.join(name)).unwrap()
}

fn failed(name: &str) -> String {
    let at = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/samples/code-host-writes");
    tehas_core::forge::why(&std::fs::read_to_string(at.join(name)).unwrap(), Some(1))
}

fn now() -> i64 {
    1_795_000_000
}

#[derive(Clone, Debug, PartialEq)]
struct Call {
    args: Vec<String>,
    input: Option<String>,
}

type Calls = Arc<Mutex<Vec<Call>>>;

/// Answers each call from the list in turn and writes every call down. A
/// call past the list fails. With a `site`, the question of the
/// repository's name is answered from it, as GitHub answers it for the
/// fixture project, and the list is kept for the other calls.
struct Script {
    answers: VecDeque<Result<String, String>>,
    calls: Calls,
    site: Option<&'static str>,
}

impl Script {
    fn next(&mut self, args: &[&str], input: Option<&str>) -> Result<String, String> {
        self.calls.lock().unwrap().push(Call {
            args: args.iter().map(|a| a.to_string()).collect(),
            input: input.map(str::to_string),
        });
        if let Some(site) = self.site.filter(|_| input.is_none() && is_name(args)) {
            return Ok(site.to_string());
        }
        self.answers
            .pop_front()
            .unwrap_or_else(|| Err("no answer".into()))
    }
}

/// The question of the repository's name, as the adapter asks it.
fn is_name(args: &[&str]) -> bool {
    args.get(1) == Some(&"graphql")
        && args
            .last()
            .is_some_and(|q| q.ends_with("{ databaseId nameWithOwner } }"))
}

/// GitHub's answer to the question of the name for the fixture project:
/// its database id and the name it has.
const HERE: &str = r#"{"data":{"repository":{"databaseId":7001,"nameWithOwner":"acme/widget"}}}"#;

impl GhRunner for Script {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        self.next(args, None)
    }

    fn run_input(&mut self, args: &[&str], input: &str) -> Result<String, String> {
        self.next(args, Some(input))
    }
}

/// The adapter of `project` over a script that answers every call from the
/// list, the question of the name too.
fn adapter_of(project: &str, answers: Vec<Result<String, String>>) -> (GitHub, Calls) {
    script_of(project, None, answers)
}

fn script_of(
    project: &str,
    site: Option<&'static str>,
    answers: Vec<Result<String, String>>,
) -> (GitHub, Calls) {
    let calls = Calls::default();
    let script = Script {
        answers: answers.into(),
        calls: Arc::clone(&calls),
        site,
    };
    (GitHub::new(project, Box::new(script)).on_clock(now), calls)
}

fn adapter(answers: Vec<Result<String, String>>) -> (GitHub, Calls) {
    script_of(PROJECT, Some(HERE), answers)
}

/// The factory's bot account, as the site is given it.
const BOT_ID: u64 = 100001;

/// A code host writes handle the site makes for the factory's own account,
/// knowing the bot's user id `bot_id`, over a runner answering `answers`.
fn ours_of(
    bot_id: Option<u64>,
    answers: Vec<Result<String, String>>,
) -> (Box<dyn CodeHostWrites + Send>, Calls) {
    let calls = Calls::default();
    let kept = Arc::new(Mutex::new(Some(Script {
        answers: answers.into(),
        calls: Arc::clone(&calls),
        site: Some(HERE),
    })));
    let make: MakeRunner = Arc::new(move |_: &Actor| {
        let script = kept.lock().unwrap().take().expect("one handle");
        Ok(Box::new(script) as Box<dyn GhRunner>)
    });
    let site = GitHubSite::new(PROJECT, "tehas-bot[bot]", bot_id, make).on_clock(now);
    let h = site
        .code_host_writes(
            &Actor::Ours(tehas_ports::Scope::Full),
            Caller::new("test.open"),
        )
        .unwrap();
    (h, calls)
}

fn ours(answers: Vec<Result<String, String>>) -> (Box<dyn CodeHostWrites + Send>, Calls) {
    ours_of(Some(BOT_ID), answers)
}

fn taken(calls: &Calls) -> Vec<Call> {
    calls.lock().unwrap().clone()
}

fn args(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| w.to_string()).collect()
}

const READ: [&str; 2] = [
    "api",
    "repos/acme/widget/pulls?state=open&head=acme:tehas%2Ffinish-7&per_page=100",
];
const POST: [&str; 6] = [
    "api",
    "-X",
    "POST",
    "repos/acme/widget/pulls",
    "--input",
    "-",
];

/// The title 1.x's finish gives its draft (`finish/plan.rs`, `title`).
const TITLE: &str = "Finish issue #7 from a stopped cycle";

/// The body 1.x's finish gives its draft (`finish/plan.rs`, `body`), for
/// issue 7, base `main` and a short reason.
const BODY_1X: &str = "Fixes #7.\n\n\
The factory opened this pull request from a cycle that stopped: the cap ran out\n\n\
Its gate did not pass. What is here is the branch the stopped cycle left, rebased onto `main` and pushed as it stood, so the cap, the failing checks and the review decide on it as they would on any other pull request. Nothing here merged on its own.\n\n\
Opened by tehas-cycle finish for issue #7.\n";

fn bp(n: u64) -> BlueprintNo {
    BlueprintNo::new(n).unwrap()
}

/// What the finish asks: `gh pr create --base main --head <pushed> --draft
/// --title <title> --body-file <body>`, and closes the issue.
fn finish_change(body: &str, closes: Vec<BlueprintNo>) -> NewChange {
    NewChange {
        head: Branch::new(HEAD).unwrap(),
        base: Branch::new("main").unwrap(),
        title: TITLE.to_string(),
        body: Body::Text(body.to_string()),
        draft: true,
        closes,
    }
}

/// What the `POST` sent on standard input: these fields and no other.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    title: String,
    head: String,
    base: String,
    body: String,
    draft: bool,
}

fn input(c: &Call) -> Input {
    serde_json::from_str(c.input.as_deref().unwrap()).unwrap()
}

fn posts(calls: &[Call]) -> usize {
    calls.iter().filter(|c| c.args == args(&POST)).count()
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

fn spent(pool: Pool) -> Outcome {
    Outcome::Spent {
        pool,
        until: tehas_ports::At::new(now()),
    }
}

// The request.

#[test]
fn an_open_reads_the_head_then_posts_what_the_finishs_gh_create_sent() {
    let (mut gh, calls) = adapter(vec![
        Ok(sample("pulls-of-head-none.json")),
        Ok(sample("pull-41-created.json")),
    ]);
    let posted = gh
        .open_change(&finish_change(BODY_1X, vec![bp(7)]))
        .unwrap();
    assert_eq!(posted.id.get(), 41);
    assert_eq!(
        posted.link.as_str(),
        "https://github.com/acme/widget/pull/41"
    );
    let calls = taken(&calls);
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0].args, args(&READ));
    assert_eq!(calls[0].input, None);
    assert!(is_name_question(&calls[1]));
    assert_eq!(calls[2].args, args(&POST));
    assert_eq!(
        input(&calls[2]),
        Input {
            title: TITLE.into(),
            head: HEAD.into(),
            base: "main".into(),
            // 1.x's body closes 7 already: no second line.
            body: BODY_1X.into(),
            draft: true,
        }
    );
}

#[test]
fn a_200_kib_body_goes_on_standard_input_alone() {
    let mut body = String::new();
    let mut i = 0;
    while body.len() < 200 * 1024 {
        body.push_str(&format!(
            "A line only this draft's body holds, number {i}.\n"
        ));
        i += 1;
    }
    let (mut gh, calls) = adapter(vec![
        Ok(sample("pulls-of-head-none.json")),
        Ok(sample("pull-41-created.json")),
    ]);
    gh.open_change(&finish_change(&body, vec![])).unwrap();
    let calls = taken(&calls);
    for a in calls.iter().flat_map(|c| &c.args) {
        assert!(!a.contains("A line only this draft"), "{a:.80}");
    }
    assert_eq!(input(&calls[2]).body, body);
}

// What the change closes.

#[test]
fn closes_is_written_after_the_body_as_1x_wrote_it() {
    let (mut gh, calls) = adapter(vec![
        Ok(sample("pulls-of-head-none.json")),
        Ok(sample("pull-41-created.json")),
    ]);
    gh.open_change(&finish_change("The work.", vec![bp(7), bp(9), bp(7)]))
        .unwrap();
    // `Fixes #7.`, byte for byte the first line of 1.x's finish body.
    assert_eq!(
        input(&taken(&calls)[2]).body,
        "The work.\n\nFixes #7.\nFixes #9.\n"
    );
}

#[test]
fn closes_is_never_doubled_nor_dropped() {
    for (body, closes, want) in [
        (BODY_1X, vec![bp(7)], BODY_1X.to_string()),
        (
            BODY_1X,
            vec![bp(7), bp(8)],
            format!("{BODY_1X}\nFixes #8.\n"),
        ),
        ("", vec![bp(7)], "Fixes #7.\n".to_string()),
        (
            "Closes #70.",
            vec![bp(7)],
            "Closes #70.\n\nFixes #7.\n".to_string(),
        ),
        ("Body.\n", vec![], "Body.\n".to_string()),
    ] {
        let (mut gh, calls) = adapter(vec![
            Ok(sample("pulls-of-head-none.json")),
            Ok(sample("pull-41-created.json")),
        ]);
        gh.open_change(&finish_change(body, closes.clone()))
            .unwrap();
        assert_eq!(input(&taken(&calls)[2]).body, want, "{body:?} {closes:?}");
    }
}

// Refused before any request.

#[test]
fn a_title_or_body_that_holds_a_marker_is_refused_before_any_request() {
    for (title, body) in [
        (TITLE, "Text <!-- tehas:park issue=7 --> more"),
        ("A <!-- tehas:question 7 --> title", "Body."),
        (TITLE, "An open wrapper alone <!-- TEHAS:"),
    ] {
        let mut new = finish_change(body, vec![]);
        new.title = title.to_string();
        let (mut gh, calls) = adapter(vec![]);
        assert_eq!(
            gh.open_change(&new),
            Err(Outcome::Refused(Refusal::Invalid)),
            "{title} {body}"
        );
        assert!(taken(&calls).is_empty());
    }
}

#[test]
fn a_pi_rukas_handoff_line_in_the_body_is_no_marker() {
    let body = "<!-- pi-rukas:handoff issue=7 cap=budget -->\nThe work.";
    let (mut gh, calls) = adapter(vec![
        Ok(sample("pulls-of-head-none.json")),
        Ok(sample("pull-41-created.json")),
    ]);
    assert!(gh.open_change(&finish_change(body, vec![])).is_ok());
    assert_eq!(input(&taken(&calls)[2]).body, body);
}

#[test]
fn a_summed_body_or_a_project_that_would_aim_elsewhere_is_refused_before_any_request() {
    let mut summed = finish_change("", vec![]);
    summed.body = Body::Summed(Sum {
        sha256: Sha256::new("a".repeat(64)).unwrap(),
        len: 3,
    });
    let (mut gh, calls) = adapter(vec![]);
    assert_eq!(
        gh.open_change(&summed),
        Err(Outcome::Refused(Refusal::Invalid))
    );
    for project in ["acme", "acme/../x", "acme/widget/pulls", "ac me/w"] {
        let (mut gh2, c2) = adapter_of(project, vec![]);
        assert_eq!(
            gh2.open_change(&finish_change("B", vec![])),
            Err(Outcome::Refused(Refusal::Invalid)),
            "{project}"
        );
        assert!(taken(&c2).is_empty());
    }
    assert!(taken(&calls).is_empty());
}

// One already there.

#[test]
fn an_open_change_of_the_same_head_is_exists_and_nothing_is_posted() {
    let (mut gh, calls) = adapter(vec![Ok(sample("pulls-of-head-41.json"))]);
    assert_eq!(
        gh.open_change(&finish_change(BODY_1X, vec![bp(7)])),
        Err(Outcome::Refused(Refusal::Exists))
    );
    assert_eq!(taken(&calls).len(), 1);
}

#[test]
fn a_422_with_a_change_of_the_head_now_there_is_exists_and_any_other_is_invalid() {
    // Opened by someone else between the read and the POST.
    let (mut gh, calls) = adapter(vec![
        Ok(sample("pulls-of-head-none.json")),
        Err(failed("validation.stderr")),
        Ok(sample("pulls-of-head-41.json")),
    ]);
    assert_eq!(
        gh.open_change(&finish_change("B", vec![])),
        Err(Outcome::Refused(Refusal::Exists))
    );
    let calls = taken(&calls);
    assert_eq!(posts(&calls), 1);
    assert_eq!(calls[3].args, args(&READ));
    // No commits between, a branch not there, or a draft the plan refuses:
    // invalid, and never a ready pull request instead.
    for after in [
        Ok(sample("pulls-of-head-none.json")),
        Err(BOUND.to_string()),
    ] {
        let (mut gh, calls) = adapter(vec![
            Ok(sample("pulls-of-head-none.json")),
            Err(failed("validation.stderr")),
            after,
        ]);
        assert_eq!(
            gh.open_change(&finish_change("B", vec![])),
            Err(Outcome::Refused(Refusal::Invalid))
        );
        let calls = taken(&calls);
        assert_eq!(posts(&calls), 1);
        assert!(calls
            .iter()
            .filter_map(|c| c.input.as_deref())
            .all(|i| i.contains(r#""draft":true"#)));
    }
}

#[test]
fn a_draft_answered_as_ready_is_inconsistent() {
    let (mut gh, _) = adapter(vec![
        Ok(sample("pulls-of-head-none.json")),
        Ok(sample("pull-41-created-ready.json")),
    ]);
    assert_eq!(
        gh.open_change(&finish_change("B", vec![])),
        Err(Outcome::Unread(Unread::Inconsistent))
    );
}

// Failures.

#[test]
fn each_failure_of_the_read_before_sends_nothing() {
    for (name, why, want) in [
        ("not found", failed("not-found.stderr"), Outcome::NotFound),
        ("forbidden", failed("forbidden.stderr"), Outcome::Forbidden),
        (
            "signed out",
            failed("signed-out.stderr"),
            Outcome::SignedOut,
        ),
        (
            "rate limit",
            failed("rate-limited.stderr"),
            spent(Pool::Calls),
        ),
        (
            "secondary",
            failed("secondary-limit.stderr"),
            spent(Pool::Calls),
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
    ] {
        let (mut gh, calls) = adapter(vec![Err(why)]);
        assert_eq!(
            gh.open_change(&finish_change("B", vec![])),
            Err(want),
            "{name}"
        );
        assert_eq!(taken(&calls).len(), 1, "{name}");
    }
    for (name, out, want) in [
        (
            "not json",
            "<html>".to_string(),
            Outcome::Unread(Unread::Malformed),
        ),
        (
            "no list",
            "{}".to_string(),
            Outcome::Unread(Unread::Malformed),
        ),
        (
            "other head",
            sample("pulls-of-head-other-head.json"),
            Outcome::Unread(Unread::Inconsistent),
        ),
    ] {
        let (mut gh, calls) = adapter(vec![Ok(out)]);
        assert_eq!(
            gh.open_change(&finish_change("B", vec![])),
            Err(want),
            "{name}"
        );
        assert_eq!(taken(&calls).len(), 1, "{name}");
    }
}

#[test]
fn each_failure_of_the_post_is_its_outcome_and_it_is_never_sent_again() {
    for (name, why, want) in [
        ("not found", failed("not-found.stderr"), Outcome::NotFound),
        ("gone", failed("gone.stderr"), Outcome::NotFound),
        ("forbidden", failed("forbidden.stderr"), Outcome::Forbidden),
        (
            "signed out",
            failed("signed-out.stderr"),
            Outcome::SignedOut,
        ),
        (
            "rate limit",
            failed("rate-limited.stderr"),
            spent(Pool::Writes),
        ),
        (
            "secondary",
            failed("secondary-limit.stderr"),
            spent(Pool::Writes),
        ),
        (
            "unreachable",
            failed("unreachable.stderr"),
            not_sent(Passing::Unreachable),
        ),
        (
            "no input",
            NO_INPUT.to_string(),
            Outcome::Refused(Refusal::Unsupported),
        ),
    ] {
        let (mut gh, calls) = adapter(vec![Ok(sample("pulls-of-head-none.json")), Err(why)]);
        assert_eq!(
            gh.open_change(&finish_change("B", vec![])),
            Err(want),
            "{name}"
        );
        let calls = taken(&calls);
        assert_eq!(calls.len(), 3, "{name}");
        assert_eq!(posts(&calls), 1, "{name}");
    }
}

#[test]
fn a_post_that_may_have_landed_is_settled_by_one_read_of_the_head() {
    // The read finds the one opened: done.
    for why in [BOUND.to_string(), failed("server-error.stderr")] {
        let (mut gh, calls) = ours(vec![
            Ok(sample("pulls-of-head-none.json")),
            Err(why),
            Ok(sample("pulls-of-head-41.json")),
        ]);
        let posted = gh.open_change(&finish_change("B", vec![])).unwrap();
        assert_eq!(posted.id.get(), 41);
        let calls = taken(&calls);
        assert_eq!(calls.len(), 4);
        assert_eq!(posts(&calls), 1);
    }
    // None, two, or the read fails: it may land yet.
    for after in [
        Ok(sample("pulls-of-head-none.json")),
        Ok(sample("pulls-of-head-two.json")),
        Err(failed("server-error.stderr")),
    ] {
        let (mut gh, calls) = ours(vec![
            Ok(sample("pulls-of-head-none.json")),
            Err(BOUND.to_string()),
            after,
        ]);
        assert_eq!(
            gh.open_change(&finish_change("B", vec![])),
            Err(maybe(Passing::Timeout))
        );
        assert_eq!(posts(&taken(&calls)), 1);
    }
}

#[test]
fn another_pull_request_of_the_head_found_by_the_settle_leaves_maybe() {
    // Opened by someone else between the read and the POST: another base
    // and ready, another title, or ready alone.
    for found in [
        "pulls-of-head-41-another-ready.json",
        "pulls-of-head-41-other-title.json",
        "pulls-of-head-41-other-ready.json",
    ] {
        let (mut gh, calls) = ours(vec![
            Ok(sample("pulls-of-head-none.json")),
            Err(BOUND.to_string()),
            Ok(sample(found)),
        ]);
        assert_eq!(
            gh.open_change(&finish_change("B", vec![])),
            Err(maybe(Passing::Timeout)),
            "{found}"
        );
        assert_eq!(posts(&taken(&calls)), 1, "{found}");
    }
    // Ours: the base, title and draft sent.
    let (mut gh, _) = ours(vec![
        Ok(sample("pulls-of-head-none.json")),
        Err(BOUND.to_string()),
        Ok(sample("pulls-of-head-41.json")),
    ]);
    assert_eq!(
        gh.open_change(&finish_change("B", vec![]))
            .unwrap()
            .id
            .get(),
        41
    );
}

#[test]
fn the_settle_takes_only_a_pull_request_the_factorys_bot_opened() {
    // A person's pull request on our head with our base, title and draft,
    // and one by a user whose id is the bot's but who is no bot: Maybe.
    for found in [
        "pulls-of-head-41-by-a-person.json",
        "pulls-of-head-41-user-with-the-bots-id.json",
    ] {
        let (mut gh, calls) = ours(vec![
            Ok(sample("pulls-of-head-none.json")),
            Err(BOUND.to_string()),
            Ok(sample(found)),
        ]);
        assert_eq!(
            gh.open_change(&finish_change("B", vec![])),
            Err(maybe(Passing::Timeout)),
            "{found}"
        );
        assert_eq!(posts(&taken(&calls)), 1, "{found}");
    }
    // Ours, by the bot's id: done.
    let (mut gh, _) = ours(vec![
        Ok(sample("pulls-of-head-none.json")),
        Err(BOUND.to_string()),
        Ok(sample("pulls-of-head-41.json")),
    ]);
    assert_eq!(
        gh.open_change(&finish_change("B", vec![]))
            .unwrap()
            .id
            .get(),
        41
    );
    // No bot id given, so nothing is ours: it may land yet, from a site
    // handle and from a bare one alike.
    let (mut gh, _) = ours_of(
        None,
        vec![
            Ok(sample("pulls-of-head-none.json")),
            Err(BOUND.to_string()),
            Ok(sample("pulls-of-head-41.json")),
        ],
    );
    assert_eq!(
        gh.open_change(&finish_change("B", vec![])),
        Err(maybe(Passing::Timeout))
    );
    let (mut gh, _) = adapter(vec![
        Ok(sample("pulls-of-head-none.json")),
        Err(BOUND.to_string()),
        Ok(sample("pulls-of-head-41.json")),
    ]);
    assert_eq!(
        gh.open_change(&finish_change("B", vec![])),
        Err(maybe(Passing::Timeout))
    );
}

#[test]
fn a_closing_word_github_does_not_read_still_gets_the_line() {
    for (body, want) in [
        // In fenced code, an HTML comment, a quote, indented code, a code
        // span: GitHub closes nothing, so the line is added.
        ("```\nFixes #7.\n```", "```\nFixes #7.\n```\n\nFixes #7.\n"),
        ("<!-- Fixes #7 -->", "<!-- Fixes #7 -->\n\nFixes #7.\n"),
        ("> Fixes #7.", "> Fixes #7.\n\nFixes #7.\n"),
        ("    Fixes #7.", "    Fixes #7.\n\nFixes #7.\n"),
        ("See `Fixes #7`.", "See `Fixes #7`.\n\nFixes #7.\n"),
        // Plain text closes it: no second line.
        ("Fixes #7.", "Fixes #7."),
        ("Work.\n\nFixes #7.\n", "Work.\n\nFixes #7.\n"),
        // #70 does not name 7.
        ("Fixes #70.", "Fixes #70.\n\nFixes #7.\n"),
        // Every other form gets the line: another project's form, another
        // word, the lenient forms a looser reading would take, and a
        // closing line in a table after a paragraph.
        (
            "Fixes acme/widget#7.",
            "Fixes acme/widget#7.\n\nFixes #7.\n",
        ),
        ("Closes #7.", "Closes #7.\n\nFixes #7.\n"),
        ("Fixes\n\n#7", "Fixes\n\n#7\n\nFixes #7.\n"),
        ("fixed.#7", "fixed.#7\n\nFixes #7.\n"),
        ("Fixes:#7", "Fixes:#7\n\nFixes #7.\n"),
        ("Work. Fixes #7.", "Work. Fixes #7.\n\nFixes #7.\n"),
        (
            "Work.\n| a |\n| - |\n| Fixes #7 |",
            "Work.\n| a |\n| - |\n| Fixes #7 |\n\nFixes #7.\n",
        ),
        // The exact line, with or without its dot, among other lines.
        ("Work.\n  Fixes #7  \nMore.", "Work.\n  Fixes #7  \nMore."),
    ] {
        let (mut gh, calls) = adapter(vec![
            Ok(sample("pulls-of-head-none.json")),
            Ok(sample("pull-41-created.json")),
        ]);
        gh.open_change(&finish_change(body, vec![bp(7)])).unwrap();
        assert_eq!(input(&taken(&calls)[2]).body, want, "{body:?}");
    }
}

#[test]
fn an_answer_after_a_2xx_that_does_not_read_is_unread() {
    for (name, out, want) in [
        ("empty", String::new(), Outcome::Unread(Unread::Malformed)),
        (
            "other project",
            sample("pull-41-created-other-project.json"),
            Outcome::Unread(Unread::Inconsistent),
        ),
        (
            "other head",
            sample("pull-41-created-other-head.json"),
            Outcome::Unread(Unread::Inconsistent),
        ),
    ] {
        let (mut gh, calls) = adapter(vec![Ok(sample("pulls-of-head-none.json")), Ok(out)]);
        assert_eq!(
            gh.open_change(&finish_change("B", vec![])),
            Err(want),
            "{name}"
        );
        assert_eq!(posts(&taken(&calls)), 1, "{name}");
    }
}

// The remote.

#[test]
fn the_remote_is_1xs_https_url_with_no_credential_and_no_request() {
    for access in [Access::Fetch, Access::Push] {
        let (mut gh, calls) = adapter(vec![]);
        let remote = gh.remote(access).unwrap();
        // 1.x's finish: format!("https://github.com/{}.git", repo).
        assert_eq!(remote.url.as_str(), "https://github.com/acme/widget.git");
        assert!(remote.with.is_none());
        assert!(taken(&calls).is_empty());
    }
    let (mut gh, _) = adapter_of("Acme-Labs/widget_2.0", vec![]);
    assert_eq!(
        gh.remote(Access::Push).unwrap().url.as_str(),
        "https://github.com/Acme-Labs/widget_2.0.git"
    );
}

#[test]
fn a_project_name_that_is_not_owner_and_name_has_no_remote() {
    for project in [
        "",
        "acme",
        "acme/",
        "/widget",
        "acme/widget/x",
        "acme/..",
        "acme/.",
        "../widget",
        "acme/wid get",
        " acme/widget",
        "acme/widget\n",
        "-acme/widget",
        "acme/-widget",
        "acme--x/widget",
        "acme/widget.git",
        "user:pass@acme/widget",
        "acme/widget?x=1",
    ] {
        let (mut gh, calls) = adapter_of(project, vec![]);
        assert_eq!(
            gh.remote(Access::Push).map(|r| r.url.as_str().to_string()),
            Err(Outcome::Refused(Refusal::Invalid)),
            "{project:?}"
        );
        assert!(taken(&calls).is_empty());
    }
}

/// The question of the name, as the adapter asks it.
fn is_name_question(c: &Call) -> bool {
    c.input.is_none()
        && c.args[..2] == args(&["api", "graphql"])
        && c.args
            .iter()
            .any(|a| a.contains("databaseId nameWithOwner"))
}

/// With no open pull request of the head, the open confirms the
/// repository by the question of its name before its post; the next open
/// asks nothing.
#[test]
fn an_open_confirms_the_repository_before_its_post_and_the_next_asks_nothing() {
    let (mut gh, calls) = adapter_of(
        PROJECT,
        vec![
            Ok(sample("pulls-of-head-none.json")),
            Ok(HERE.into()),
            Ok(sample("pull-41-created.json")),
            Ok(sample("pulls-of-head-none.json")),
            Ok(sample("pull-41-created.json")),
        ],
    );
    for _ in 0..2 {
        let posted = gh
            .open_change(&finish_change(BODY_1X, vec![bp(7)]))
            .unwrap();
        assert_eq!(posted.id.get(), 41);
    }
    let calls = taken(&calls);
    assert_eq!(calls.len(), 5);
    assert_eq!(calls[0].args, args(&READ));
    assert!(is_name_question(&calls[1]));
    assert_eq!(calls[2].args, args(&POST));
    assert_eq!(calls.iter().filter(|c| is_name_question(c)).count(), 1);
}

/// A repository under another owner gets no pull request.
#[test]
fn an_open_on_a_repository_under_another_owner_is_refused_with_nothing_posted() {
    let moved = HERE.replace("acme/widget", "mallory/widget");
    let (mut gh, calls) = adapter_of(
        PROJECT,
        vec![
            Ok(sample("pulls-of-head-none.json")),
            Ok(moved),
            Ok(sample("pull-41-created.json")),
        ],
    );
    assert_eq!(
        gh.open_change(&finish_change(BODY_1X, vec![bp(7)])),
        Err(Outcome::Unread(Unread::Inconsistent))
    );
    let calls = taken(&calls);
    assert_eq!(calls.len(), 2);
    assert_eq!(posts(&calls), 0, "nothing posted");
    assert!(is_name_question(&calls[1]));
}

/// A failure of the question is its outcome, with nothing sent.
#[test]
fn an_open_whose_repository_question_fails_sends_nothing() {
    let (mut gh, calls) = adapter_of(
        PROJECT,
        vec![
            Ok(sample("pulls-of-head-none.json")),
            Err(BOUND.into()),
            Ok(sample("pull-41-created.json")),
        ],
    );
    assert_eq!(
        gh.open_change(&finish_change(BODY_1X, vec![bp(7)])),
        Err(not_sent(Passing::Timeout))
    );
    assert_eq!(posts(&taken(&calls)), 0, "nothing posted");
}
