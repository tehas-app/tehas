//! `Tickets::what_is` and `Tickets::what_are` as the adapter answers them,
//! replayed from its recordings under `tests/recorded/what/<state>/`: what
//! each number names, a ticket or a change, asked in one question an alias
//! a number. `live/` is a live read-only answer of a public repository; the
//! rest are made by hand. Answers GitHub cannot be made to give on demand
//! are under `tests/synthetic/what/` (`tests/synthetic/README.md`).

mod answers;

use std::num::NonZeroU64;
use std::sync::{Arc, Mutex};
use tehas_core::forge::{Failed, GhRunner};
use tehas_github::GitHub;
use tehas_ports::{
    At, ChangeNo, Got, Item, Outcome, Passing, Pool, Sent, TicketKey, Tickets, Unread, ALL_MAX,
};

const PROJECT: &str = "acme/widget";
const MALFORMED: Outcome = Outcome::Unread(Unread::Malformed);
const INCONSISTENT: Outcome = Outcome::Unread(Unread::Inconsistent);
const CUT: Outcome = Outcome::Unread(Unread::Cut);
const SERVER: Outcome = Outcome::Unavailable {
    why: Passing::Server,
    sent: Sent::Maybe,
};

type Asked = Arc<Mutex<Vec<Vec<String>>>>;

fn field<'a>(args: &'a [&str], name: &str) -> Option<&'a str> {
    args.windows(2)
        .filter(|w| w[0] == "-f" || w[0] == "-F")
        .find_map(|w| w[1].strip_prefix(&format!("{name}=")))
}

/// The question `live/from-1` answers, whole: GitHub's `locations` in it
/// put `n999999999` at column 289, where this text has it.
const LIVE_QUERY: &str = "query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { n1: issueOrPullRequest(number: 1) { __typename ... on Issue { number } ... on PullRequest { number } } n2: issueOrPullRequest(number: 2) { __typename ... on Issue { number } ... on PullRequest { number } } n999999999: issueOrPullRequest(number: 999999999) { __typename ... on Issue { number } ... on PullRequest { number } } } }";

/// The question of `numbers` a hand-made recording answers, whole.
fn question_of(numbers: &[u64]) -> String {
    let mut q = String::from(
        "query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) {",
    );
    for n in numbers {
        q.push_str(&format!(" n{n}: issueOrPullRequest(number: {n}) {{ __typename ... on Issue {{ number }} ... on PullRequest {{ number }} }}"));
    }
    q + " } }"
}

/// Each recording of `state` and the whole question it answers.
fn recorded(state: &str) -> Vec<(&'static str, String)> {
    let made = |file, numbers: &[u64]| (file, question_of(numbers));
    match state {
        "live" => vec![("from-1", LIVE_QUERY.to_string())],
        "one" => vec![
            made("from-1", &[1]),
            made("from-2", &[2]),
            made("from-999999999", &[999_999_999]),
        ],
        "many" => vec![
            made("from-1", &(1..=100).collect::<Vec<_>>()),
            made("from-101", &(101..=150).collect::<Vec<_>>()),
        ],
        "null-no-error" | "missing-alias" => vec![made("from-3", &[3, 5])],
        "other-error" | "two-errors" | "inner-not-found" | "error-elsewhere"
        | "found-and-error" | "question-error" => vec![made("from-3", &[3, 6])],
        // A failure with no answer is the call's, of any question.
        "not-found" => vec![made("from-3", &[3, 4]), made("from-3", &[3])],
        _ => vec![made("from-3", &[3, 4])],
    }
}

/// A runner that answers a question only from the recording of one state
/// that answers that whole question (`recorded`), and panics on any other.
/// `.json` is gh's answer, `.fail` the reason it failed, and `.answer.json`
/// beside a `.fail` what gh printed as it failed.
struct Replay {
    state: String,
    asked: Asked,
}

impl GhRunner for Replay {
    fn run(&mut self, _: &[&str]) -> Result<String, String> {
        panic!("the items' question is asked with its answer kept")
    }

    fn run_answer(&mut self, args: &[&str]) -> Result<String, Failed> {
        self.asked
            .lock()
            .unwrap()
            .push(args.iter().map(|a| a.to_string()).collect());
        assert_eq!(&args[..2], ["api", "graphql"]);
        assert_eq!(field(args, "owner"), Some("acme"));
        assert_eq!(field(args, "name"), Some("widget"));
        let query = field(args, "query").expect("a query");
        let Some((file, _)) = recorded(&self.state).into_iter().find(|(_, q)| q == query) else {
            panic!("no recording in {} of the question {query}", self.state)
        };
        let read =
            |name: String| std::fs::read_to_string(answers::at("what", &self.state, &name)).ok();
        if let Some(out) = read(format!("{file}.json")) {
            return Ok(out);
        }
        match read(format!("{file}.fail")) {
            Some(why) => Err(Failed {
                why: why.trim_end().to_string(),
                answer: read(format!("{file}.answer.json")),
            }),
            None => panic!("no {file} in {}", self.state),
        }
    }
}

fn adapter(state: &str) -> (GitHub, Asked) {
    let asked = Asked::default();
    let replay = Replay {
        state: state.to_string(),
        asked: Arc::clone(&asked),
    };
    let gh = GitHub::new(PROJECT, Box::new(replay)).on_clock(|| 1_767_225_600);
    (gh, asked)
}

fn no(n: u64) -> NonZeroU64 {
    NonZeroU64::new(n).unwrap()
}

fn ticket(n: u64) -> Got<Item> {
    Ok(Item::Ticket(TicketKey::Number(no(n))))
}

fn change(n: u64) -> Got<Item> {
    Ok(Item::Change(ChangeNo::from(no(n))))
}

/// What the numbers name in one state, and how many questions it took.
fn what(state: &str, numbers: &[u64]) -> (Vec<(u64, Got<Item>)>, usize) {
    let (mut gh, asked) = adapter(state);
    let numbers: Vec<NonZeroU64> = numbers.iter().map(|n| no(*n)).collect();
    let got = gh
        .what_are(&numbers)
        .into_iter()
        .map(|(n, a)| (n.get(), a))
        .collect();
    let calls = asked.lock().unwrap().len();
    (got, calls)
}

#[test]
fn a_live_answer_names_a_change_a_ticket_and_nothing() {
    let (got, calls) = what("live", &[1, 2, 999_999_999]);
    assert_eq!(
        got,
        [
            (1, change(1)),
            (2, ticket(2)),
            (999_999_999, Err(Outcome::NotFound))
        ]
    );
    assert_eq!(calls, 1);
}

#[test]
fn the_numbers_go_in_one_question_an_alias_each() {
    let (mut gh, asked) = adapter("live");
    gh.what_are(&[no(1), no(2), no(999_999_999)]);
    let asked = asked.lock().unwrap();
    let query = asked[0]
        .iter()
        .find_map(|a| a.strip_prefix("query="))
        .unwrap();
    for n in [1, 2, 999_999_999] {
        assert!(
            query.contains(&format!(
                " n{n}: issueOrPullRequest(number: {n}) {{ __typename "
            )),
            "{query}"
        );
    }
}

#[test]
fn a_question_gh_answered_whole_answers_each_number() {
    let (got, _) = what("there", &[3, 4]);
    assert_eq!(got, [(3, ticket(3)), (4, change(4))]);
}

#[test]
fn what_is_is_what_are_of_one_number() {
    for (n, want) in [
        (1, change(1)),
        (2, ticket(2)),
        // GitHub does not know it: gh's answer kept says so.
        (999_999_999, Err(Outcome::NotFound)),
    ] {
        let (mut gh, asked) = adapter("one");
        assert_eq!(gh.what_is(no(n)), want, "{n}");
        assert_eq!(asked.lock().unwrap().len(), 1, "{n}");
    }
}

#[test]
fn a_repeated_number_is_asked_once_and_answered_each_time() {
    let (got, calls) = what("one", &[1, 1]);
    assert_eq!(got, [(1, change(1)), (1, change(1))]);
    assert_eq!(calls, 1);
}

#[test]
fn no_number_asks_nothing() {
    let (got, calls) = what("live", &[]);
    assert!(got.is_empty());
    assert_eq!(calls, 0);
}

/// Decided 121: a null node with exactly one `NOT_FOUND` error whose path is
/// that alias is `NotFound` (`live/`); every other shape is unread.
#[test]
fn a_node_that_does_not_add_up_is_unread_and_spoils_only_itself() {
    for (state, other, want) in [
        // Null with no error that says why.
        ("null-no-error", 5, INCONSISTENT),
        // Not in the answer at all.
        ("missing-alias", 5, MALFORMED),
        // Another number than the one asked, or none.
        ("wrong-number", 4, INCONSISTENT),
        ("no-number", 4, INCONSISTENT),
        // Neither an issue nor a pull request.
        ("unknown-type", 4, INCONSISTENT),
        // An error of another kind than not found.
        ("other-error", 6, INCONSISTENT),
        // Not found said twice, or of a field inside the node.
        ("two-errors", 6, INCONSISTENT),
        ("inner-not-found", 6, INCONSISTENT),
        // A node and an error both.
        ("found-and-error", 6, INCONSISTENT),
    ] {
        let (got, _) = what(state, &[3, other]);
        assert_eq!(got, [(3, ticket(3)), (other, Err(want))], "{state}");
    }
}

/// An error at an alias not asked is the question's.
#[test]
fn an_error_at_an_alias_not_asked_leaves_no_number_sure() {
    let (got, _) = what("error-elsewhere", &[3, 6]);
    assert_eq!(got, [(3, Err(INCONSISTENT)), (6, Err(INCONSISTENT))]);
}

#[test]
fn an_error_of_the_question_leaves_no_number_sure() {
    let (got, _) = what("question-error", &[3, 6]);
    assert_eq!(got, [(3, Err(INCONSISTENT)), (6, Err(INCONSISTENT))]);
}

/// An error that names no alias is the question's: a limited rate spends
/// the priced budget for every number, any other leaves none sure, the
/// repository's own not found among them.
#[test]
fn an_error_that_names_no_alias_is_every_numbers() {
    // Held a minute out: GitHub's reset is not read.
    let limited = Outcome::Spent {
        pool: Pool::Priced,
        until: At::new(1_767_225_600 + 60),
    };
    for (state, want) in [("limited", limited), ("no-repository", INCONSISTENT)] {
        let (got, calls) = what(state, &[3, 4]);
        assert_eq!(got, [(3, Err(want.clone())), (4, Err(want))], "{state}");
        assert_eq!(calls, 1, "{state}");
    }
}

#[test]
fn an_answer_that_is_no_answer_fails_every_number() {
    for (state, want) in [
        ("garbled", MALFORMED),
        // gh failed: its answer is read only by its errors.
        ("failed-no-errors", SERVER),
        ("failed-garbled", SERVER),
    ] {
        let (got, _) = what(state, &[3, 4]);
        assert_eq!(got, [(3, Err(want.clone())), (4, Err(want))], "{state}");
    }
}

#[test]
fn a_failed_question_is_its_outcome_for_every_number() {
    for (state, want) in [
        ("server-error", SERVER),
        // A GraphQL question draws on the priced pool.
        (
            "rate-limited",
            Outcome::Spent {
                pool: Pool::Priced,
                until: At::new(1_767_225_600),
            },
        ),
        (
            "timeout",
            Outcome::Unavailable {
                why: Passing::Timeout,
                sent: Sent::Maybe,
            },
        ),
        ("forbidden", Outcome::Forbidden),
        ("signed-out", Outcome::SignedOut),
        // Not found of the whole question names no one number of several.
        ("not-found", INCONSISTENT),
    ] {
        let (got, calls) = what(state, &[3, 4]);
        assert_eq!(got, [(3, Err(want.clone())), (4, Err(want))], "{state}");
        assert_eq!(calls, 1, "{state}");
    }
    // Of one number too: with no answer, GitHub named no number.
    let (got, _) = what("not-found", &[3]);
    assert_eq!(got, [(3, Err(INCONSISTENT))]);
}

#[test]
fn more_than_a_hundred_numbers_take_a_question_a_hundred() {
    let numbers: Vec<u64> = (1..=150).rev().collect();
    let (got, calls) = what("many", &numbers);
    assert_eq!(calls, 2);
    assert_eq!(got.len(), 150);
    for (n, a) in got {
        let want = if n % 2 == 1 { ticket(n) } else { change(n) };
        assert_eq!(a, want, "{n}");
    }
}

#[test]
fn more_numbers_than_all_max_are_cut_with_no_question() {
    let past = ALL_MAX.get() as u64 + 1;
    let numbers: Vec<u64> = (1..=past).collect();
    let (got, calls) = what("many", &numbers);
    assert_eq!(calls, 0);
    assert_eq!(got.len(), past as usize);
    assert!(got.iter().all(|(_, a)| *a == Err(CUT)));
    // Repeats count once: as many asks of fewer numbers are no cut.
    let mut repeated: Vec<u64> = (1..=150).collect();
    repeated.extend((1..=past).map(|n| 1 + n % 150));
    let (got, calls) = what("many", &repeated);
    assert_eq!(calls, 2);
    assert!(got.iter().all(|(_, a)| a.is_ok()));
}
