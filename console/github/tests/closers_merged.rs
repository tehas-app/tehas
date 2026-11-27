//! `Tickets::several_with_closers` over a live recording: issue 947 of a
//! real project, closed completed by a commit while one merged change of
//! the project is listed as closing it. The recording under
//! `tests/recorded/closers/b52-several-947.json` is what gh answered to
//! the adapter's question and is never edited; the closed-since answer
//! beside it, `b52-since-947.json`, is that recording's own node with its
//! close time, so the two reads of the law answer the same issue. The
//! hand-made derived `b52-open-951.json` is that node reopened: an open
//! issue with a listed-changes answer, for the state gate of
//! `with_closer`.

use std::num::NonZeroU64;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tehas_core::forge::GhRunner;
use tehas_github::GitHub;
use tehas_ports::{
    laws, Bound, ChangeNo, ChangeState, CloseReason, Closer, CommitId, TicketKey, TicketState,
    Tickets,
};

fn key(n: u64) -> TicketKey {
    TicketKey::Number(NonZeroU64::new(n).unwrap())
}

/// Answers the two questions of the law from the recordings: the several
/// question of 947 and the closed-since page made from its node. It keeps
/// the queries it was asked.
struct B52 {
    queries: Arc<Mutex<Vec<String>>>,
}

impl GhRunner for B52 {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        let query = args
            .iter()
            .find_map(|a| a.strip_prefix("query="))
            .unwrap_or_default()
            .to_string();
        self.queries.lock().unwrap().push(query);
        let asked = self.queries.lock().unwrap();
        let last = asked.last().map(String::as_str).unwrap_or_default();
        let file = if last.contains("i947: issue(number: 947)") {
            "b52-several-947.json"
        } else if last.contains("i951: issue(number: 951)") {
            "b52-open-951.json"
        } else if last.contains("since: $since") {
            "b52-since-947.json"
        } else {
            return Err(format!("no recording of {args:?}"));
        };
        std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/recorded/closers")
                .join(file),
        )
        .map_err(|e| format!("no recording {file}: {e}"))
    }
}

/// The adapter over the recordings, and the log of the queries it asks.
fn adapter() -> (GitHub, Arc<Mutex<Vec<String>>>) {
    let queries = Arc::new(Mutex::new(Vec::new()));
    let gh = GitHub::new(
        "acme/widget",
        Box::new(B52 {
            queries: Arc::clone(&queries),
        }),
    )
    .on_clock(|| 1_800_000_000);
    (gh, queries)
}

/// The law, on the GitHub adapter over the recording.
#[test]
fn several_lists_merged_as_closed_since_over_the_recording() {
    let (mut gh, _) = adapter();
    let keys = [key(947)];
    assert_eq!(
        laws::tickets::several_lists_merged_as_closed_since(
            &mut gh,
            &keys,
            Bound::new(10).unwrap(),
        ),
        Ok(())
    );
}

/// What the recording says about 947, as the adapter reads it: closed
/// completed, its closer the commit it was closed by, the merged change
/// GitHub lists beside it in `merged`.
#[test]
fn the_recording_closes_947_by_a_commit_beside_a_merged_change() {
    let (mut gh, _) = adapter();
    let got = gh.several_with_closers(&[key(947)]);
    let (k, answer) = &got[0];
    assert_eq!(*k, key(947));
    let with = answer.as_ref().unwrap();
    assert_eq!(with.digest.state, TicketState::Closed(CloseReason::Done));
    assert_eq!(
        with.closer,
        Some(Closer::Commit(
            CommitId::new("b9bd2e108c28c40bd77090bdeb9668b828e6b8aa").unwrap()
        ))
    );
    assert_eq!(with.merged.len(), 1, "the one change listed and merged");
    let change = &with.merged[0];
    assert_eq!(change.no, ChangeNo::new(950).unwrap());
    assert!(matches!(change.state, ChangeState::Merged { .. }));
    let size = change.size.as_ref().unwrap();
    assert_eq!((size.files, size.added, size.deleted), (6, 1622, 0));
}

/// An open ticket keeps the list empty without reading it, as the port
/// docs and the fake hold it: GitHub lists a merged change as closing an
/// issue reopened after it. The hand-made derived `b52-open-951.json`
/// lists two changes and holds one, so reading the list at all would
/// fail the read; the answer is `Ok` with the list empty and no closer.
#[test]
fn an_open_ticket_reads_no_merged_changes() {
    let (mut gh, _) = adapter();
    let got = gh.several_with_closers(&[key(951)]);
    let (k, answer) = &got[0];
    assert_eq!(*k, key(951));
    let with = answer.as_ref().unwrap();
    assert_eq!(with.digest.state, TicketState::Open);
    assert_eq!(with.closer, None, "a reopened issue is not closed");
    assert!(with.merged.is_empty(), "open keeps the list empty");
}

/// The question the recording answers is the question the adapter asks:
/// the same fields, so the answer holds what the read parses.
#[test]
fn the_question_asked_is_the_one_recorded() {
    let (mut gh, queries) = adapter();
    let _ = gh.several_with_closers(&[key(947)]);
    let asked = queries.lock().unwrap();
    assert_eq!(asked.len(), 1, "one question for one ticket");
    let q = &asked[0];
    assert!(q.contains("i947: issue(number: 947)"));
    assert!(q.contains("closedByPullRequestsReferences(first: 20)"));
    assert!(q.contains("closingIssuesReferences(first: 50)"));
    assert!(q.contains("baseRepository { databaseId nameWithOwner }"));
}
