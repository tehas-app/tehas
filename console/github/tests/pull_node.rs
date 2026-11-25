//! `CodeHost::changes` and `Board::open_changes` read a pull request node
//! with one reader: the same node gives the same digest to both, and each
//! reads past what a question asks as its port's samples say.

use std::sync::{Arc, Mutex};
use tehas_core::forge::GhRunner;
use tehas_github::GitHub;
use tehas_ports::{
    Board, Bound, ChangeAsk, ChangeDigest, ChangeFilter, CodeHost, Got, Outcome, Unread,
};

/// A runner that answers every question with one page.
struct Page {
    out: String,
    asked: Arc<Mutex<Vec<String>>>,
}

impl GhRunner for Page {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        self.asked.lock().unwrap().push(args.join(" "));
        Ok(self.out.clone())
    }
}

/// An open pull request of `acme/widget` (database id 7001), from its own
/// branch, carrying `labels` and closing `closes` of its issues, each list
/// whole. It holds what either question asks.
fn node(labels: usize, closes: usize) -> String {
    let labels: Vec<String> = (0..labels)
        .map(|i| format!(r#"{{"name":"theirs-{i}"}}"#))
        .collect();
    let closes: Vec<String> = (1..=closes)
        .map(|i| {
            format!(
                r#"{{"number":{i},"repository":{{"databaseId":7001,"nameWithOwner":"acme/widget"}}}}"#
            )
        })
        .collect();
    format!(
        r#"{{"number":5,"title":"Change 5","state":"OPEN","isDraft":true,"isCrossRepository":false,"headRepository":{{"databaseId":7001}},"headRefName":"topic-5","headRefOid":"{sha}","baseRefName":"main","baseRefOid":"{sha}","createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-03T00:00:00Z","mergedAt":null,"closedAt":null,"lastEditedAt":"2026-01-02T00:00:00Z","additions":3,"deletions":1,"changedFiles":2,"mergeable":"MERGEABLE","mergeStateStatus":"CLEAN","url":"https://github.com/acme/widget/pull/5","author":{{"__typename":"User","login":"Alice","databaseId":101}},"editor":{{"__typename":"User","login":"bob","databaseId":102}},"mergedBy":null,"mergeCommit":null,"renames":{{"nodes":[]}},"labels":{{"totalCount":{},"nodes":[{}]}},"closingIssuesReferences":{{"totalCount":{},"nodes":[{}]}}}}"#,
        labels.len(),
        labels.join(","),
        closes.len(),
        closes.join(","),
        sha = "ab".repeat(20)
    )
}

fn adapter(node: &str) -> (GitHub, Arc<Mutex<Vec<String>>>) {
    let out = format!(
        r#"{{"data":{{"repository":{{"databaseId":7001,"pullRequests":{{"totalCount":1,"pageInfo":{{"hasNextPage":false,"endCursor":null}},"nodes":[{node}]}}}}}}}}"#
    );
    let asked = Arc::new(Mutex::new(Vec::new()));
    let gh = GitHub::new(
        "acme/widget",
        Box::new(Page {
            out,
            asked: Arc::clone(&asked),
        }),
    );
    (gh, asked)
}

fn bound(n: u32) -> Bound {
    Bound::new(n).unwrap()
}

fn listed(node: &str) -> Got<Vec<ChangeDigest>> {
    let (mut gh, _) = adapter(node);
    gh.changes(&ChangeFilter::default(), bound(10))
        .map(|w| w.items().to_vec())
}

fn boarded(node: &str) -> Got<Vec<ChangeDigest>> {
    let (mut gh, _) = adapter(node);
    let ask = ChangeAsk {
        filter: ChangeFilter::default(),
        files: None,
        closing: false,
        siblings: None,
        activity: None,
    };
    gh.open_changes(&ask, bound(10))
        .map(|w| w.items().iter().map(|i| i.change.clone()).collect())
}

#[test]
fn both_ports_read_one_node_to_one_digest() {
    let node = node(2, 3);
    let listed = listed(&node).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed, boarded(&node).unwrap());
    let d = &listed[0];
    assert!(d.draft && !d.from_fork);
    assert_eq!(d.closes.len(), 3);
    assert_eq!(d.marks.len(), 2);
    assert_eq!(d.editor.as_ref().map(|a| a.login.as_str()), Some("bob"));
}

#[test]
fn each_question_asks_the_one_field_list_with_its_own_parts() {
    let node = node(0, 0);
    let (mut gh, asked) = adapter(&node);
    gh.changes(&ChangeFilter::default(), bound(10)).unwrap();
    let list = asked.lock().unwrap()[0].clone();
    assert!(list.contains(" headRepository { databaseId }"), "{list}");
    assert!(!list.contains("renames:"), "{list}");
    assert!(
        list.contains("closingIssuesReferences(first: 50)"),
        "{list}"
    );
}

#[test]
fn past_what_the_question_asks_is_each_ports_own_outcome() {
    // 51 closed issues, past the 50 a question asks: the code host's
    // samples say Cut, the board's TooLarge.
    let closes = node(0, 51);
    assert_eq!(listed(&closes), Err(Outcome::Unread(Unread::Cut)));
    assert_eq!(boarded(&closes), Err(Outcome::Unread(Unread::TooLarge)));
    let labels = node(101, 0);
    assert_eq!(listed(&labels), Err(Outcome::Unread(Unread::Cut)));
    assert_eq!(boarded(&labels), Err(Outcome::Unread(Unread::TooLarge)));
}

#[test]
fn the_list_fails_closed_on_a_head_repository_it_asked_and_was_not_given() {
    let node = node(0, 0).replace(r#""headRepository":{"databaseId":7001},"#, "");
    assert_eq!(listed(&node), Err(Outcome::Unread(Unread::Malformed)));
    // The board asks no head repository, and reads GitHub's word.
    assert!(!boarded(&node).unwrap()[0].from_fork);
}
