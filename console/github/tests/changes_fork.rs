//! `CodeHost::changes` says for each change whether its head is a fork,
//! and who opened it: the finish decides from both whether an open change
//! is the factory's own. A change is from a fork unless GitHub says it is
//! not and its head repository is this one: a deleted head repository
//! fails closed. Each case is one hand-made GraphQL page.

use std::sync::{Arc, Mutex};
use tehas_core::forge::GhRunner;
use tehas_github::GitHub;
use tehas_ports::{AccountId, AccountKind, Bound, ChangeFilter, CodeHost, Login, Outcome, Unread};

struct Page {
    out: String,
    asked: Arc<Mutex<u32>>,
}

impl GhRunner for Page {
    fn run(&mut self, _: &[&str]) -> Result<String, String> {
        *self.asked.lock().unwrap() += 1;
        Ok(self.out.clone())
    }
}

/// An open pull request of `acme/widget` (database id 7001), with its
/// `isCrossRepository` and `headRepository` as given.
fn node(no: u64, cross: bool, head_repo: &str, author: &str) -> String {
    format!(
        r#"{{"number":{no},"title":"Change {no}","state":"OPEN","isDraft":false,"isCrossRepository":{cross}{head_repo},"headRefName":"topic-{no}","headRefOid":"{sha}","baseRefName":"main","baseRefOid":"{sha}","createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-02T00:00:00Z","mergedAt":null,"closedAt":null,"lastEditedAt":null,"additions":1,"deletions":0,"changedFiles":1,"mergeable":"UNKNOWN","mergeStateStatus":"UNKNOWN","url":"https://github.com/acme/widget/pull/{no}","author":{author},"editor":null,"mergedBy":null,"mergeCommit":null,"labels":{{"totalCount":0,"nodes":[]}},"closingIssuesReferences":{{"totalCount":0,"nodes":[]}}}}"#,
        sha = "ab".repeat(20)
    )
}

fn page(nodes: &[String]) -> String {
    format!(
        r#"{{"data":{{"repository":{{"databaseId":7001,"pullRequests":{{"totalCount":{},"pageInfo":{{"hasNextPage":false,"endCursor":null}},"nodes":[{}]}}}}}}}}"#,
        nodes.len(),
        nodes.join(",")
    )
}

const ALICE: &str = r#"{"__typename":"User","login":"Alice","databaseId":101}"#;
const BOT: &str = r#"{"__typename":"Bot","login":"tehas-bot","databaseId":100001}"#;

fn read(out: String) -> tehas_ports::Got<Vec<tehas_ports::ChangeDigest>> {
    let asked = Arc::new(Mutex::new(0));
    let mut gh = GitHub::new(
        "acme/widget",
        Box::new(Page {
            out,
            asked: Arc::clone(&asked),
        }),
    );
    let got = gh
        .changes(&ChangeFilter::default(), Bound::new(10).unwrap())
        .map(|w| w.items().to_vec());
    assert_eq!(*asked.lock().unwrap(), 1);
    got
}

#[test]
fn a_change_is_from_a_fork_unless_its_head_is_this_repository() {
    let here = r#","headRepository":{"databaseId":7001}"#;
    let fork = r#","headRepository":{"databaseId":8001}"#;
    let gone = r#","headRepository":null"#;
    let got = read(page(&[
        node(1, false, here, ALICE),
        node(2, true, fork, ALICE),
        // GitHub says same repository, but names another head repository.
        node(3, false, fork, ALICE),
        // The head repository was deleted.
        node(4, true, gone, ALICE),
        node(5, false, gone, ALICE),
    ]))
    .unwrap();
    let forks: Vec<(u64, bool)> = got.iter().map(|d| (d.no.get(), d.from_fork)).collect();
    assert_eq!(
        forks,
        [(1, false), (2, true), (3, true), (4, true), (5, true)]
    );
}

#[test]
fn a_change_whose_answer_lacks_its_head_repository_is_unread() {
    assert_eq!(
        read(page(&[node(1, false, "", ALICE)])),
        Err(Outcome::Unread(Unread::Malformed))
    );
}

#[test]
fn each_change_names_its_author_with_kind_login_and_id() {
    let here = r#","headRepository":{"databaseId":7001}"#;
    let got = read(page(&[
        node(1, false, here, ALICE),
        node(2, false, here, BOT),
    ]))
    .unwrap();
    let alice = got[0].author.as_ref().unwrap();
    assert_eq!(alice.login, Login::new("alice"));
    assert_eq!(alice.kind, AccountKind::Person);
    assert_eq!(alice.id, Some(AccountId::new("101")));
    let bot = got[1].author.as_ref().unwrap();
    assert_eq!(bot.kind, AccountKind::Bot);
    assert_eq!(bot.id, Some(AccountId::new("100001")));
}
