//! The world of the refresh tests (rule 4b): issue 7 waits for a merge of
//! pull request 50, which conflicts with main. Its body is "Words.", which
//! alice approved when she labelled its last cycle, and tehasbot refresh asked
//! for a refresh. The image takes --continue-pr (features.continuePr).

use crate::common::*;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tehas_core::home::{Home, State};

pub const RHEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
pub const RBASE: &str = "cccccccccccccccccccccccccccccccccccccccc";

/// The REST call of rule 4b that takes the mark off a pull request.
/// Whether the poll took the refresh mark off pull request `n`, as a write
/// of the code host.
pub fn unmarked_change(w: &World, n: u64) -> bool {
    w.writes_of(n).contains(&tehas_ports::Write::UnmarkChange {
        no: tehas_ports::ChangeNo::new(n).unwrap(),
        mark: tehas_ports::Mark::State(tehas_ports::State::Refreshing),
    })
}

pub fn refresh_dir(w: &World) -> PathBuf {
    Home::new(&w.home).state(State::Refreshes)
}

/// Pull request 50 as GitHub answers for it now, after `change`.
pub fn pull(w: &World, change: impl FnOnce(&mut Value)) {
    let mut p = json!({"state": "OPEN", "number": 50,
        "url": "https://github.com/acme/widget/pull/50", "isDraft": false, "isCrossRepository": false,
        "headRefOid": RHEAD, "baseRefName": "main", "baseRefOid": RBASE, "mergeable": "CONFLICTING",
        "author": {"login": "tehas-bot"}, "headRepository": {"nameWithOwner": "acme/widget"},
        "labels": {"nodes": []},
        "closingIssuesReferences": {"totalCount": 1, "nodes": [{"number": 7, "state": "OPEN",
            "labels": {"nodes": [{"name": "tehas:awaiting-merge"}]},
            "closedByPullRequestsReferences": {"nodes": [{"number": 50, "state": "OPEN"}]}}]}});
    change(&mut p);
    w.fx_json(
        "pull-50.json",
        &json!({"data": {"repository": {"pullRequest": p}}}),
    );
}

/// What GitHub says issue 7 carries now: tehas:awaiting-merge and `labels`.
pub fn issue7(w: &World, labels: &[&str]) {
    work7(w, "tehas:awaiting-merge", labels);
}

fn work7(w: &World, first: &str, labels: &[&str]) {
    let all: Vec<Value> = std::iter::once(first)
        .chain(labels.iter().copied())
        .map(|l| json!({"name": l}))
        .collect();
    w.fx_json(
        "work-7.json",
        &json!({"number": 7, "state": "open", "labels": all}),
    );
}

/// The request of #7 as tehasbot refresh writes it, asked at `at`.
pub fn request(w: &World, at: &str) {
    w.home_file(
        Home::new(&w.home).refresh_request(7),
        &json!({"at": at, "pr": 50, "head_sha": RHEAD, "base_sha": RBASE}).to_string(),
    );
}

/// The world of the refresh tests, with more instance settings.
pub fn refresh_setup(w: &World, more: &[&str]) {
    w.reset();
    let _ = fs::remove_dir_all(refresh_dir(w));
    for f in [
        "refreshing.json",
        "caps.json",
        "mark-fails",
        "mark-fails-times",
        "unmark-fails",
        "label-missing",
        "pull-50.json",
    ] {
        w.rm_fx(f);
    }
    w.slots(2, more);
    w.fx("ready.json", "[]");
    w.fx("events-7.json", "[]");
    w.fx("awaiting.json", r#"[{"number":7}]"#);
    w.fx(
        "edit-7.json",
        r#"{"data":{"repository":{"issue":{"lastEditedAt":null,"editor":null,"body":"Words."}}}}"#,
    );
    w.fx("icomments-7.json", "[]");
    issue7(w, &[]);
    pull(w, |_| {});
    w.fx(
        "caps.json",
        r#"{"verbs":{"work":{"flags":["--json","--timeout","--restart","--continue-pr"]}},"features":{"continuePr":true}}"#,
    );
    fs::create_dir_all(w.cycles_dir()).unwrap();
    request(w, "2026-10-17T09:00:00Z");
    // The marks read answers with the ready mark's id, as on a host.
    w.fx_json(
        "labels.json",
        &json!([{"id": "LA_ready", "name": READY_MARK}]),
    );
    // alice's label started the last cycle, which waits for a merge.
    w.past(7, "alice", "approved", "2026-10-01T00:00:00Z", &[]);
}

/// A new world of the refresh tests.
pub fn refresh_world(name: &str, more: &[&str]) -> World {
    let w = World::new(name);
    refresh_setup(&w, more);
    w
}

/// The store, under issue `n`'s lock.
pub fn locked_store(w: &World, n: u64, args: &[&str]) {
    let out = Command::new("flock")
        .args(["-w", "5"])
        .arg(w.cycles_dir().join(format!("{n}.lock")))
        .arg(w.t().join("store-bin"))
        .args(args)
        .env("TEHAS_HOME", &w.home)
        .output()
        .unwrap();
    assert!(out.status.success(), "store {args:?}: {out:?}");
}

/// `k` refreshes on alice's approval that ran and ended, the last one parked
/// when asked.
pub fn refresh_record(w: &World, k: i64, parked: bool) {
    for i in 1..=k {
        let a = w.attempt(7);
        w.t_approve(
            7,
            "alice",
            "refresh",
            epoch("2026-10-02T00:00:00Z") + i,
            &["--refresh-pr", "50", "--refresh-head", RHEAD],
        );
        w.t_started(7, None, &[]);
        let next = (a + 1).to_string();
        let mut end = vec!["cycle-end", "7", "--attempt", &next, "--status", "handoff"];
        if i == k && parked {
            end.push("--parked");
        }
        locked_store(w, 7, &end);
    }
}

/// A running refresh of issue `n`: an open cycle the store says started as
/// one, on alice's approval of an earlier cycle.
pub fn running_refresh(w: &World, n: u64) {
    w.past(n, "alice", "approved", "2026-09-30T00:00:00Z", &[]);
    w.t_approve(
        n,
        "alice",
        "refresh",
        epoch("2026-10-01T00:00:00Z"),
        &["--refresh-pr", "50", "--refresh-head", RHEAD],
    );
    w.t_started(n, None, &[]);
    w.hold(n);
    // A running cycle's issue is marked running, as on a host.
    let mut marked: Value =
        serde_json::from_str(&fs::read_to_string(w.fx.join("marked.json")).unwrap()).unwrap();
    marked
        .as_array_mut()
        .unwrap()
        .push(json!({"number": n, "labels": [{"name": "tehas:running"}]}));
    w.fx_json("marked.json", &marked);
}

/// Whether #7's request is still there: 1 or 0.
pub fn requests(w: &World) -> usize {
    usize::from(refresh_dir(w).join("7.json").exists())
}

/// The reaper of rule 2: the pull requests marked tehas:refreshing.
pub fn marked_refreshing(w: &World) {
    w.fx(
        "refreshing.json",
        r#"[{"number":50,"labels":[{"name":"tehas:refreshing"}]}]"#,
    );
}

/// The comments on #7 that say tehas does not refresh it.
pub fn said_no_refresh(w: &World) -> usize {
    w.said(7)
        .iter()
        .filter(|b| b.contains("does not refresh"))
        .count()
}
