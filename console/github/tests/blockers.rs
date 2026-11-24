//! `Tickets::blockers` as the adapter answers it: an issue's blocked-by
//! links, replayed from the recordings of the fixture project under
//! `tests/recorded/issue-<n>/blockers.*`, which hold what the issue's
//! ticket read holds of them, and the law that ties the list to
//! `Ticket.blockers`. The cases a recording cannot hold run against a
//! runner that answers from a list.

mod replay;

use replay::{adapter_of, PROJECT};
use std::num::NonZeroU64;
use std::sync::{Arc, Mutex};
use tehas_core::forge::GhRunner;
use tehas_github::GitHub;
use tehas_ports::laws;
use tehas_ports::{
    Blocker, BlockerState, Item, ItemName, Outcome, ProjectName, Ref, Refusal, TicketKey, Tickets,
    Unread,
};

const INCONSISTENT: Outcome = Outcome::Unread(Unread::Inconsistent);
const MALFORMED: Outcome = Outcome::Unread(Unread::Malformed);
const CUT: Outcome = Outcome::Unread(Unread::Cut);

fn key(n: u64) -> TicketKey {
    TicketKey::Number(NonZeroU64::new(n).unwrap())
}

#[test]
fn an_issue_waits_on_what_its_links_name() {
    let (mut gh, asked) = adapter_of(PROJECT);
    let got = gh.blockers(&key(20)).unwrap().into_items();
    assert_eq!(
        got,
        [Blocker::new(
            Ref::Here(Item::Ticket(key(23))),
            BlockerState::Open
        )]
    );
    assert_eq!(asked.get(), 1);
    assert!(gh.blockers(&key(25)).unwrap().into_items().is_empty());
}

#[test]
fn the_list_is_the_tickets_own_at_every_depth() {
    let (mut gh, _) = adapter_of(PROJECT);
    // 21 is a part of 20, whose parent this adapter does not read yet.
    assert_eq!(
        laws::tickets::a_parent_holds_its_parts(&mut gh, &[key(20), key(25)]),
        Ok(())
    );
}

#[test]
fn a_failed_read_answers_as_the_parts_read_does() {
    // Not there, out of time, rate limited, forbidden, a change's number.
    for n in [9, 10, 12, 14, 30] {
        let (mut gh, _) = adapter_of(PROJECT);
        let blockers = gh.blockers(&key(n)).map(|_| ());
        let (mut gh, _) = adapter_of(PROJECT);
        let parts = gh.parts(&key(n)).map(|_| ());
        assert!(blockers.is_err(), "{n}");
        assert_eq!(blockers, parts, "{n}");
    }
    let (mut gh, _) = adapter_of(PROJECT);
    assert_eq!(gh.blockers(&key(9)), Err(Outcome::NotFound));
    assert_eq!(gh.blockers(&key(30)), Err(Outcome::NotFound));
}

#[test]
fn a_key_that_is_no_number_asks_nothing() {
    let (mut gh, asked) = adapter_of(PROJECT);
    let named = TicketKey::Key(tehas_ports::KeyText::new("PROJ-1").unwrap());
    assert_eq!(gh.blockers(&named), Err(Outcome::Refused(Refusal::Invalid)));
    assert_eq!(asked.get(), 0);
}

type Asked = Arc<Mutex<Vec<String>>>;

struct Stub {
    answers: Vec<String>,
    asked: Asked,
}

impl GhRunner for Stub {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        let mut asked = self.asked.lock().unwrap();
        asked.push(args.join(" "));
        self.answers
            .get(asked.len() - 1)
            .cloned()
            .ok_or_else(|| "no fixture".to_string())
    }
}

fn blockers(inner: &str) -> (Result<Vec<Blocker>, Outcome>, Asked) {
    let asked = Asked::default();
    let answer = format!(
        r#"{{"data":{{"repository":{{"databaseId":7001,"issue":{{"number":7{inner}}}}}}}}}"#
    );
    let stub = Stub {
        answers: vec![answer],
        asked: Arc::clone(&asked),
    };
    let mut gh = GitHub::new(PROJECT, Box::new(stub));
    (gh.blockers(&key(7)).map(|w| w.into_items()), asked)
}

/// An issue another names, its repository known by its id: this
/// project's for `acme/widget`.
fn named(n: &str, state: &str, repo: &str) -> String {
    let id = if repo == PROJECT { 7001 } else { 7100 };
    format!(
        r#"{{"number":{n},"state":"{state}","repository":{{"databaseId":{id},"nameWithOwner":"{repo}"}}}}"#
    )
}

fn links(total: u64, nodes: &[String]) -> String {
    format!(
        r#","blockedBy":{{"totalCount":{total},"nodes":[{}]}}"#,
        nodes.join(",")
    )
}

#[test]
fn a_link_of_another_project_is_named_and_a_closed_one_is_closed() {
    let (got, asked) = blockers(&links(
        2,
        &[
            named("4", "CLOSED", "acme/widget"),
            named("5", "OPEN", "octo/tools"),
        ],
    ));
    assert_eq!(
        got.unwrap(),
        [
            Blocker::new(Ref::Here(Item::Ticket(key(4))), BlockerState::Closed),
            Blocker::new(
                Ref::Elsewhere {
                    project: ProjectName::new("octo/tools"),
                    item: ItemName::new("5"),
                },
                BlockerState::Open
            ),
        ]
    );
    let asked = asked.lock().unwrap();
    assert_eq!(asked.len(), 1);
    assert!(asked[0].contains("blockedBy(first: 100, after: $cursor) { totalCount nodes"));
    assert!(asked[0].contains(" -F number=7 "));
}

#[test]
fn a_list_that_does_not_add_up_is_unread() {
    // More than the page holds.
    assert_eq!(
        blockers(&links(101, &[named("4", "OPEN", "acme/widget")])).0,
        Err(CUT)
    );
    // A null entry, one with no number, one that does not say its state.
    assert_eq!(
        blockers(&links(1, &["null".to_string()])).0,
        Err(INCONSISTENT)
    );
    let nameless =
        r#"{"state":"OPEN","repository":{"databaseId":7001,"nameWithOwner":"acme/widget"}}"#
            .to_string();
    assert_eq!(blockers(&links(1, &[nameless])).0, Err(INCONSISTENT));
    assert_eq!(
        blockers(&links(1, &[named("4", "MERGED", "acme/widget")])).0,
        Err(MALFORMED)
    );
    // No count, no list at all.
    assert_eq!(
        blockers(r#","blockedBy":{"nodes":[]}"#).0,
        Err(INCONSISTENT)
    );
    assert_eq!(blockers("").0, Err(MALFORMED));
}
