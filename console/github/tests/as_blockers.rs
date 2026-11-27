//! `Board::as_blockers` as the adapter answers it: what each number names,
//! a ticket or a change, with its state, in one GraphQL question an alias a
//! number (Decided 122). Replayed from `tests/recorded/blockers/live/`, the
//! live answers of `tehas-app/tehas-rec-blockers` (kit run #1, 2026-11-26):
//! issue 1 open, issue 2 closed, pull request 3 open, 4 merged and 5
//! closed, and the same question with 1005, a number that never was.
//! `tests/synthetic/blockers/` holds the faults GitHub cannot give.

mod answers;

use std::num::NonZeroU64;
use std::sync::{Arc, Mutex};
use tehas_core::forge::{Failed, GhRunner};
use tehas_github::GitHub;
use tehas_ports::{
    laws, Blocker, BlockerState, Board, ChangeNo, Got, Item, Outcome, Passing, Ref, Sent,
    TicketKey, Unread, ALL_MAX,
};

const PROJECT: &str = "tehas-app/tehas-rec-blockers";
const MALFORMED: Outcome = Outcome::Unread(Unread::Malformed);

type Asked = Arc<Mutex<Vec<String>>>;

/// The question kit run #1 asked of 1 to 5, as its `calls.log` line 21
/// gives it: answered by `as-blockers-found`.
const FOUND_QUERY: &str = r#"query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { n1: issueOrPullRequest(number: 1) { __typename ... on Issue { number state } ... on PullRequest { number state } } n2: issueOrPullRequest(number: 2) { __typename ... on Issue { number state } ... on PullRequest { number state } } n3: issueOrPullRequest(number: 3) { __typename ... on Issue { number state } ... on PullRequest { number state } } n4: issueOrPullRequest(number: 4) { __typename ... on Issue { number state } ... on PullRequest { number state } } n5: issueOrPullRequest(number: 5) { __typename ... on Issue { number state } ... on PullRequest { number state } } } }"#;

/// The question kit run #1 asked of 1 to 5 and 1005, as its `calls.log`
/// line 22 gives it: answered by `as-blockers`.
const MISSING_QUERY: &str = r#"query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { n1: issueOrPullRequest(number: 1) { __typename ... on Issue { number state } ... on PullRequest { number state } } n2: issueOrPullRequest(number: 2) { __typename ... on Issue { number state } ... on PullRequest { number state } } n3: issueOrPullRequest(number: 3) { __typename ... on Issue { number state } ... on PullRequest { number state } } n4: issueOrPullRequest(number: 4) { __typename ... on Issue { number state } ... on PullRequest { number state } } n5: issueOrPullRequest(number: 5) { __typename ... on Issue { number state } ... on PullRequest { number state } } n1005: issueOrPullRequest(number: 1005) { __typename ... on Issue { number state } ... on PullRequest { number state } } } }"#;

fn field<'a>(args: &'a [&str], name: &str) -> Option<&'a str> {
    args.windows(2)
        .filter(|w| w[0] == "-f" || w[0] == "-F")
        .find_map(|w| w[1].strip_prefix(&format!("{name}=")))
}

/// The numbers a question asks, read from its aliases.
fn numbers_of(query: &str) -> Vec<u64> {
    query
        .split(": issueOrPullRequest(number: ")
        .skip(1)
        .filter_map(|r| r.split(')').next()?.parse().ok())
        .collect()
}

/// A runner that answers the blockers' question from the answers of one
/// state, when the whole question is one the run recorded:
/// `as-blockers-found` for `FOUND_QUERY`, `as-blockers` for
/// `MISSING_QUERY`. `.json` is gh's answer, `.fail` the reason it
/// failed and `.answer.json` beside it what gh printed as it failed.
struct Replay {
    state: &'static str,
    asked: Asked,
}

impl GhRunner for Replay {
    fn run(&mut self, _: &[&str]) -> Result<String, String> {
        panic!("the blockers' question is asked with its answer kept")
    }

    fn run_answer(&mut self, args: &[&str]) -> Result<String, Failed> {
        assert_eq!(&args[..2], ["api", "graphql"]);
        assert_eq!(field(args, "owner"), Some("tehas-app"));
        assert_eq!(field(args, "name"), Some("tehas-rec-blockers"));
        let query = field(args, "query").expect("a query");
        self.asked.lock().unwrap().push(query.to_string());
        let name = match query {
            FOUND_QUERY => "as-blockers-found",
            MISSING_QUERY => "as-blockers",
            other => panic!("no recording of the question {other}"),
        };
        let read = |ext: &str| {
            std::fs::read_to_string(answers::at(
                "blockers",
                self.state,
                &format!("{name}.{ext}"),
            ))
            .ok()
        };
        if let Some(out) = read("json") {
            return Ok(out);
        }
        match read("fail") {
            Some(why) => Err(Failed {
                why: why.trim_end().to_string(),
                answer: read("answer.json"),
            }),
            None => panic!("no recording of {name} in {}", self.state),
        }
    }
}

fn adapter(state: &'static str) -> (GitHub, Asked) {
    let asked = Asked::default();
    let replay = Replay {
        state,
        asked: Arc::clone(&asked),
    };
    (GitHub::new(PROJECT, Box::new(replay)), asked)
}

/// A runner whose every question fails with `failed`, its questions kept.
struct Failing {
    failed: fn() -> Failed,
    asked: Asked,
}

impl GhRunner for Failing {
    fn run(&mut self, _: &[&str]) -> Result<String, String> {
        panic!("the blockers' question is asked with its answer kept")
    }

    fn run_answer(&mut self, args: &[&str]) -> Result<String, Failed> {
        let query = field(args, "query").expect("a query");
        self.asked.lock().unwrap().push(query.to_string());
        Err((self.failed)())
    }
}

fn failing(failed: fn() -> Failed) -> (GitHub, Asked) {
    let asked = Asked::default();
    let runner = Failing {
        failed,
        asked: Arc::clone(&asked),
    };
    (GitHub::new(PROJECT, Box::new(runner)), asked)
}

fn timed_out() -> Failed {
    Failed {
        why: "gh ran out of its 30s bound".to_string(),
        answer: None,
    }
}

fn no(n: u64) -> NonZeroU64 {
    NonZeroU64::new(n).unwrap()
}

fn ticket(n: u64, state: BlockerState) -> Got<Blocker> {
    Ok(Blocker::new(
        Ref::Here(Item::Ticket(TicketKey::Number(no(n)))),
        state,
    ))
}

fn change(n: u64, state: BlockerState) -> Got<Blocker> {
    Ok(Blocker::new(
        Ref::Here(Item::Change(ChangeNo::from(no(n)))),
        state,
    ))
}

#[test]
fn the_law_holds_on_the_adapter_in_one_question() {
    let (mut gh, asked) = adapter("live");
    let want = [
        (no(2), ticket(2, BlockerState::Closed)),
        (no(3), change(3, BlockerState::Open)),
        (no(1), ticket(1, BlockerState::Open)),
        (no(4), change(4, BlockerState::Merged)),
        (no(1005), Err(Outcome::NotFound)),
        (no(5), change(5, BlockerState::Closed)),
        (no(1), ticket(1, BlockerState::Open)),
    ];
    assert_eq!(
        laws::board::as_blockers_name_each_number(&mut gh, &want),
        Ok(())
    );
    assert_eq!(asked.lock().unwrap().len(), 1);
}

#[test]
fn the_law_holds_on_the_adapter_when_every_number_is_there() {
    let (mut gh, asked) = adapter("live");
    let want = [
        (no(5), change(5, BlockerState::Closed)),
        (no(4), change(4, BlockerState::Merged)),
        (no(3), change(3, BlockerState::Open)),
        (no(2), ticket(2, BlockerState::Closed)),
        (no(1), ticket(1, BlockerState::Open)),
    ];
    assert_eq!(
        laws::board::as_blockers_name_each_number(&mut gh, &want),
        Ok(())
    );
    assert_eq!(asked.lock().unwrap().len(), 1);
}

#[test]
fn the_adapter_asks_the_question_the_run_recorded() {
    let (mut gh, asked) = adapter("live");
    gh.as_blockers(&[no(1), no(2), no(3), no(4), no(5)]);
    gh.as_blockers(&[no(1005), no(5), no(4), no(3), no(2), no(1)]);
    assert_eq!(*asked.lock().unwrap(), [FOUND_QUERY, MISSING_QUERY]);
}

#[test]
fn no_number_asks_nothing() {
    let (mut gh, asked) = adapter("live");
    assert!(gh.as_blockers(&[]).is_empty());
    assert!(asked.lock().unwrap().is_empty());
}

#[test]
fn a_hundred_numbers_go_in_a_question_and_past_the_most_none_is_asked() {
    let (mut gh, asked) = failing(timed_out);
    let numbers: Vec<NonZeroU64> = (1..=150).map(no).collect();
    assert_eq!(gh.as_blockers(&numbers).len(), 150);
    let asked = asked.lock().unwrap();
    let chunks: Vec<Vec<u64>> = asked.iter().map(|q| numbers_of(q)).collect();
    assert_eq!(
        chunks,
        [(1..=100).collect::<Vec<u64>>(), (101..=150).collect()]
    );

    let (mut gh, asked) = failing(timed_out);
    let numbers: Vec<NonZeroU64> = (1..=u64::from(ALL_MAX.get()) + 1).map(no).collect();
    let got = gh.as_blockers(&numbers);
    assert!(got
        .iter()
        .all(|(_, a)| *a == Err(Outcome::Unread(Unread::Cut))));
    assert!(asked.lock().unwrap().is_empty());
}

#[test]
fn a_state_not_of_its_kind_is_malformed_and_spoils_only_itself() {
    let (mut gh, _) = adapter("malformed");
    assert_eq!(
        gh.as_blockers(&[no(1), no(2), no(3), no(4), no(5)]),
        [
            (no(1), Err(MALFORMED)),
            (no(2), ticket(2, BlockerState::Closed)),
            (no(3), Err(MALFORMED)),
            (no(4), change(4, BlockerState::Merged)),
            (no(5), Err(MALFORMED)),
        ]
    );
}

#[test]
fn a_failed_question_is_its_outcome_for_every_number() {
    let (mut gh, asked) = failing(timed_out);
    let timeout = Outcome::Unavailable {
        why: Passing::Timeout,
        sent: Sent::Maybe,
    };
    assert_eq!(
        gh.as_blockers(&[no(1), no(2)]),
        [(no(1), Err(timeout.clone())), (no(2), Err(timeout))]
    );
    assert_eq!(asked.lock().unwrap().len(), 1);
}
