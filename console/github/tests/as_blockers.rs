//! `Board::as_blockers` as the adapter answers it: what each number names,
//! a ticket or a change, with its state, in one GraphQL question an alias a
//! number (Decided 122). The answers are written here, in the shape
//! `tests/recorded/what/live/` recorded of `what_are`'s question, with the
//! state beside each node.

use std::num::NonZeroU64;
use std::sync::{Arc, Mutex};
use tehas_core::forge::{Failed, GhRunner};
use tehas_github::GitHub;
use tehas_ports::{
    laws, Blocker, BlockerState, Board, ChangeNo, Got, Item, Outcome, Passing, Ref, Sent,
    TicketKey, Unread, ALL_MAX,
};

const PROJECT: &str = "acme/widget";
const MALFORMED: Outcome = Outcome::Unread(Unread::Malformed);

type Asked = Arc<Mutex<Vec<String>>>;

/// A runner that answers each question with `answer` of the numbers it
/// asks, read from its aliases: Ok for an answer gh exited 0 on, Err with
/// what gh printed as it exited 1.
struct Stub {
    answer: fn(&[u64]) -> Result<String, Failed>,
    asked: Asked,
}

impl GhRunner for Stub {
    fn run(&mut self, _: &[&str]) -> Result<String, String> {
        panic!("the blockers' question is asked with its answer kept")
    }

    fn run_answer(&mut self, args: &[&str]) -> Result<String, Failed> {
        let query = args
            .iter()
            .find_map(|a| a.strip_prefix("query="))
            .expect("a query");
        self.asked.lock().unwrap().push(query.to_string());
        let numbers: Vec<u64> = query
            .split(": issueOrPullRequest(number: ")
            .skip(1)
            .filter_map(|r| r.split(')').next()?.parse().ok())
            .collect();
        (self.answer)(&numbers)
    }
}

fn adapter(answer: fn(&[u64]) -> Result<String, Failed>) -> (GitHub, Asked) {
    let asked = Asked::default();
    let stub = Stub {
        answer,
        asked: Arc::clone(&asked),
    };
    (GitHub::new(PROJECT, Box::new(stub)), asked)
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

/// The node of number `n` of the fixture project: issues 1 (open) and 2
/// (closed), pull requests 50 (open), 51 (merged) and 52 (closed); any
/// other number names nothing.
fn node(n: u64) -> Option<String> {
    let (is, state) = match n {
        1 => ("Issue", "OPEN"),
        2 => ("Issue", "CLOSED"),
        50 => ("PullRequest", "OPEN"),
        51 => ("PullRequest", "MERGED"),
        52 => ("PullRequest", "CLOSED"),
        _ => return None,
    };
    Some(format!(
        r#"{{"__typename":"{is}","number":{n},"state":"{state}"}}"#
    ))
}

/// GitHub's answer of the fixture project: each node, null with a
/// `NOT_FOUND` error at its alias for a number it does not know, and gh
/// then exits 1 with the answer printed.
fn project(numbers: &[u64]) -> Result<String, Failed> {
    let mut nodes = Vec::new();
    let mut errors = Vec::new();
    for n in numbers {
        match node(*n) {
            Some(v) => nodes.push(format!(r#""n{n}":{v}"#)),
            None => {
                nodes.push(format!(r#""n{n}":null"#));
                errors.push(format!(
                    r#"{{"type":"NOT_FOUND","path":["repository","n{n}"],"message":"Could not resolve to an issue or pull request with the number of {n}."}}"#
                ));
            }
        }
    }
    let data = format!(r#"{{"repository":{{{}}}}}"#, nodes.join(","));
    if errors.is_empty() {
        return Ok(format!(r#"{{"data":{data}}}"#));
    }
    Err(Failed {
        why: ": gh exited 1".to_string(),
        answer: Some(format!(
            r#"{{"data":{data},"errors":[{}]}}"#,
            errors.join(",")
        )),
    })
}

#[test]
fn the_law_holds_on_the_adapter_in_one_question() {
    let (mut gh, asked) = adapter(project);
    let want = [
        (no(2), ticket(2, BlockerState::Closed)),
        (no(50), change(50, BlockerState::Open)),
        (no(1), ticket(1, BlockerState::Open)),
        (no(51), change(51, BlockerState::Merged)),
        (no(999), Err(Outcome::NotFound)),
        (no(52), change(52, BlockerState::Closed)),
        (no(1), ticket(1, BlockerState::Open)),
    ];
    assert_eq!(
        laws::board::as_blockers_name_each_number(&mut gh, &want),
        Ok(())
    );
    assert_eq!(asked.lock().unwrap().len(), 1);
}

#[test]
fn each_number_is_asked_its_state_under_its_own_alias() {
    let (mut gh, asked) = adapter(project);
    gh.as_blockers(&[no(1), no(50)]);
    let asked = asked.lock().unwrap();
    for n in [1, 50] {
        assert!(
            asked[0].contains(&format!(
                " n{n}: issueOrPullRequest(number: {n}) {{ __typename ... on Issue {{ number state }} ... on PullRequest {{ number state }} }}"
            )),
            "{}",
            asked[0]
        );
    }
}

#[test]
fn no_number_asks_nothing() {
    let (mut gh, asked) = adapter(project);
    assert!(gh.as_blockers(&[]).is_empty());
    assert!(asked.lock().unwrap().is_empty());
}

#[test]
fn a_hundred_numbers_go_in_a_question_and_past_the_most_none_is_asked() {
    let (mut gh, asked) = adapter(project);
    let numbers: Vec<NonZeroU64> = (1..=150).map(no).collect();
    let got = gh.as_blockers(&numbers);
    assert_eq!(got.len(), 150);
    assert_eq!(got[0], (no(1), ticket(1, BlockerState::Open)));
    assert_eq!(got[2], (no(3), Err(Outcome::NotFound)));
    assert_eq!(asked.lock().unwrap().len(), 2);

    let (mut gh, asked) = adapter(project);
    let numbers: Vec<NonZeroU64> = (1..=u64::from(ALL_MAX.get()) + 1).map(no).collect();
    let got = gh.as_blockers(&numbers);
    assert!(got
        .iter()
        .all(|(_, a)| *a == Err(Outcome::Unread(Unread::Cut))));
    assert!(asked.lock().unwrap().is_empty());
}

#[test]
fn a_state_not_of_its_kind_is_malformed_and_spoils_only_itself() {
    let (mut gh, _) = adapter(|numbers| {
        let nodes: Vec<String> = numbers
            .iter()
            .map(|n| {
                let v = match n {
                    3 => r#"{"__typename":"Issue","number":3,"state":"MERGED"}"#,
                    4 => r#"{"__typename":"PullRequest","number":4,"state":"DRAFT"}"#,
                    5 => r#"{"__typename":"Issue","number":5}"#,
                    _ => r#"{"__typename":"Issue","number":6,"state":"OPEN"}"#,
                };
                format!(r#""n{n}":{v}"#)
            })
            .collect();
        Ok(format!(
            r#"{{"data":{{"repository":{{{}}}}}}}"#,
            nodes.join(",")
        ))
    });
    assert_eq!(
        gh.as_blockers(&[no(3), no(4), no(5), no(6)]),
        [
            (no(3), Err(MALFORMED)),
            (no(4), Err(MALFORMED)),
            (no(5), Err(MALFORMED)),
            (no(6), ticket(6, BlockerState::Open)),
        ]
    );
}

#[test]
fn a_failed_question_is_its_outcome_for_every_number() {
    let (mut gh, asked) = adapter(|_| {
        Err(Failed {
            why: "gh ran out of its 30s bound".to_string(),
            answer: None,
        })
    });
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
