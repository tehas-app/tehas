//! `Tickets::closed_since` and `Tickets::several_with_closers` as the
//! adapter answers them, replayed from hand-made recordings under
//! `tests/recorded/closed/` and `tests/recorded/closers/`.

mod replay;

use std::num::NonZeroU64;
use tehas_github::GitHub;
use tehas_ports::{
    laws, At, Bound, ChangeNo, ChangeState, CloseReason, Closer, CommitId, ItemName, KeyText,
    Outcome, Passing, ProjectName, Refusal, Sent, TicketKey, TicketState, Tickets, Unread, WebLink,
};

const MALFORMED: Outcome = Outcome::Unread(Unread::Malformed);
const INCONSISTENT: Outcome = Outcome::Unread(Unread::Inconsistent);
const CUT: Outcome = Outcome::Unread(Unread::Cut);
const SERVER: Outcome = Outcome::Unavailable {
    why: Passing::Server,
    sent: Sent::Maybe,
};

fn adapter() -> GitHub {
    replay::adapter_of(replay::PROJECT).0
}

fn key(n: u64) -> TicketKey {
    TicketKey::Number(NonZeroU64::new(n).unwrap())
}

fn at(iso: &str) -> At {
    At::new(tehas_core::time::from_iso(iso).unwrap())
}

fn bound(n: u32) -> Bound {
    Bound::new(n).unwrap()
}

fn numbers(keys: &[TicketKey]) -> Vec<u64> {
    keys.iter()
        .map(|k| match k {
            TicketKey::Number(n) => n.get(),
            TicketKey::Key(_) => 0,
        })
        .collect()
}

#[test]
fn several_carries_each_closer_as_the_ticket_does() {
    // 20 is closed by a merged change of this project, 21 and 25 are open.
    let mut gh = adapter();
    let keys = [key(20), key(21), key(25)];
    assert_eq!(
        laws::tickets::several_carries_each_closer(&mut gh, &keys),
        Ok(())
    );
    let got = gh.several_with_closers(&keys);
    let closer = got[0].1.as_ref().unwrap().closer.clone();
    assert_eq!(
        closer,
        Some(Closer::Change {
            no: ChangeNo::new(31).unwrap(),
            merged: true
        })
    );
    assert_eq!(got[1].1.as_ref().unwrap().closer, None);
}

#[test]
fn several_with_closers_is_one_question_in_the_order_asked() {
    let (mut gh, asked) = replay::adapter_of(replay::PROJECT);
    let keys = [key(25), key(20), key(25), key(21)];
    let got = gh.several_with_closers(&keys);
    assert_eq!(asked.get(), 1);
    let order: Vec<TicketKey> = got.iter().map(|(k, _)| k.clone()).collect();
    assert_eq!(order, keys);
    assert_eq!(got[0].1, got[2].1);
    assert_eq!(
        numbers(&[got[1].1.as_ref().unwrap().digest.key.clone()]),
        [20]
    );
}

#[test]
fn a_number_that_is_no_issue_is_not_found_and_the_rest_are_read() {
    // 3 names a pull request: GitHub answers null and NOT_FOUND at its
    // alias, gh exits 1 and the answer beside it is read.
    let mut gh = adapter();
    let got = gh.several_with_closers(&[key(3), key(20)]);
    assert_eq!(got[0].1, Err(Outcome::NotFound));
    let twenty = got[1].1.as_ref().unwrap();
    assert_eq!(twenty.digest.state, TicketState::Closed(CloseReason::Done));
    assert!(twenty.closer.is_some());
}

#[test]
fn an_error_of_the_question_leaves_no_ticket_sure() {
    let mut gh = adapter();
    let got = gh.several_with_closers(&[key(4), key(20)]);
    assert_eq!(got[0].1, Err(INCONSISTENT));
    assert_eq!(got[1].1, Err(INCONSISTENT));
}

#[test]
fn a_failed_question_is_its_outcome_for_every_ticket() {
    let mut gh = adapter();
    let got = gh.several_with_closers(&[key(5), key(20)]);
    assert_eq!(got[0].1, Err(SERVER));
    assert_eq!(got[1].1, Err(SERVER));
}

#[test]
fn an_answer_of_another_number_does_not_add_up() {
    let mut gh = adapter();
    assert_eq!(gh.several_with_closers(&[key(6)])[0].1, Err(INCONSISTENT));
}

#[test]
fn a_closed_issue_with_no_close_read_is_malformed() {
    let mut gh = adapter();
    assert_eq!(gh.several_with_closers(&[key(8)])[0].1, Err(MALFORMED));
}

#[test]
fn a_key_that_is_no_number_is_refused_without_a_question() {
    let (mut gh, asked) = replay::adapter_of(replay::PROJECT);
    let k = TicketKey::Key(KeyText::new("ABC-1").unwrap());
    let got = gh.several_with_closers(&[k]);
    assert_eq!(got[0].1, Err(Outcome::Refused(Refusal::Invalid)));
    assert_eq!(asked.get(), 0);
}

#[test]
fn closed_since_holds_each_ticket_closed_since_in_list_order() {
    let (mut gh, asked) = replay::adapter_of(replay::PROJECT);
    let got = gh
        .closed_since(at("2026-02-02T00:00:00Z"), bound(10))
        .unwrap()
        .into_items();
    assert_eq!(asked.get(), 2);
    let keys: Vec<TicketKey> = got.iter().map(|c| c.digest.key.clone()).collect();
    // Oldest filed first, ties by number; 42 closed before the time asked.
    assert_eq!(numbers(&keys), [44, 40, 43, 47]);

    let hand = &got[0];
    assert_eq!(hand.closed, at("2026-02-02T00:00:00Z"));
    assert_eq!(
        hand.digest.state,
        TicketState::Closed(CloseReason::NotPlanned)
    );
    assert!(matches!(hand.closer, Some(Closer::Account(_))));
    assert_eq!(hand.part_count, 2);
    assert!(hand.merged.is_empty(), "an open change is no merge");

    let merged = &got[1];
    assert_eq!(
        merged.closer,
        Some(Closer::Change {
            no: ChangeNo::new(41).unwrap(),
            merged: true
        })
    );
    // The merged change of another project has no number here.
    let nos: Vec<u64> = merged.merged.iter().map(|c| c.no.get()).collect();
    assert_eq!(nos, [41]);
    assert!(matches!(merged.merged[0].state, ChangeState::Merged { .. }));
    let size = merged.merged[0].size.as_ref().unwrap();
    assert_eq!((size.files, size.added, size.deleted), (2, 12, 3));

    let commit = &got[2];
    assert_eq!(
        commit.closer,
        Some(Closer::Commit(
            CommitId::new(format!("c0ffee{}", "0".repeat(34))).unwrap()
        ))
    );
    let nos: Vec<u64> = commit.merged.iter().map(|c| c.no.get()).collect();
    assert_eq!(nos, [46], "the change GitHub lists beside a commit's close");

    // A merged change of another project closed 47: it is named there with
    // its link, and is no merged change here.
    let elsewhere = &got[3];
    assert_eq!(
        elsewhere.closer,
        Some(Closer::change_elsewhere(
            ProjectName::new("other/tools"),
            ItemName::new("7"),
            WebLink::new("https://github.com/other/tools/pull/7").unwrap()
        ))
    );
    assert!(elsewhere.merged.is_empty());
}

#[test]
fn closed_since_past_its_bound_is_cut() {
    let mut gh = adapter();
    assert_eq!(
        gh.closed_since(at("2026-02-02T00:00:00Z"), bound(2)),
        Err(CUT)
    );
}

#[test]
fn closed_since_with_more_closing_changes_than_one_read_names_is_cut() {
    let mut gh = adapter();
    assert_eq!(
        gh.closed_since(at("2026-03-01T00:00:00Z"), bound(10)),
        Err(CUT)
    );
}

#[test]
fn closed_since_reads_no_missing_count_as_none() {
    let mut gh = adapter();
    assert_eq!(
        gh.closed_since(at("2026-03-02T00:00:00Z"), bound(10)),
        Err(INCONSISTENT)
    );
}

#[test]
fn closed_since_with_a_closed_issue_of_no_close_time_is_malformed() {
    let mut gh = adapter();
    assert_eq!(
        gh.closed_since(at("2026-03-03T00:00:00Z"), bound(10)),
        Err(MALFORMED)
    );
}

#[test]
fn closed_since_that_github_refused_is_its_outcome() {
    let mut gh = adapter();
    assert_eq!(
        gh.closed_since(at("2026-03-04T00:00:00Z"), bound(10)),
        Err(SERVER)
    );
}
