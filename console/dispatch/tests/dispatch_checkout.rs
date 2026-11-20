//! The checkout and the code index, edits after the label, pull requests that
//! close the issue, and the branch. The pure rules of edits and renames are
//! the fixtures of tests/fixtures/approval.json.

mod common;

use common::*;
use serde_json::{json, Value};
use std::fs::OpenOptions;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

fn count_exact(text: &str, line: &str) -> usize {
    text.lines().filter(|l| *l == line).count()
}

#[test]
fn a_real_poll_puts_the_checkout_on_main_and_fast_forwards_it() {
    let w = World::new("co-ff");
    git(
        &w.upstream(),
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "second",
        ],
    );
    git(
        &w.checkout(),
        &["checkout", "-q", "-b", "feature/left-over"],
    );
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(git(&w.checkout(), &["branch", "--show-current"]), "main");
    assert_eq!(
        git(&w.checkout(), &["rev-parse", "HEAD"]),
        git(&w.upstream(), &["rev-parse", "main"])
    );
    lacks(&r.out, "WARN");
    assert_eq!(w.started(), "7 ");
    has(
        &w.gh(),
        "issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:ready",
    );
}

/// A poll whose fetch hangs, bounded at a second, and how long it took.
fn hung_fetch(w: &World) -> (Run, u64) {
    let pack = w.t().join("slow-upload-pack");
    tehas_core::testing::write_exe(&pack, "#!/bin/sh\nexec sleep 30 2>/dev/null\n").unwrap();
    git(
        &w.checkout(),
        &[
            "config",
            "remote.origin.uploadpack",
            &pack.to_string_lossy(),
        ],
    );
    w.env("TEHAS_FETCH_WAIT", "1");
    let began = Instant::now();
    let r = w.poll();
    let took = began.elapsed().as_secs();
    git(
        &w.checkout(),
        &["config", "--unset", "remote.origin.uploadpack"],
    );
    (r, took)
}

#[test]
fn a_fetch_that_hangs_is_given_up_after_its_bound_said_and_the_cycle_still_starts() {
    let w = World::new("co-fetch-hangs");
    let (r, took) = hung_fetch(&w);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        &format!("WARN: could not fast-forward {}", w.checkout().display()),
    );
    assert_eq!(w.started(), "7 ");
    assert!(took < 20);
}

#[test]
fn the_code_index_is_refreshed_after_the_fetch_and_before_the_cycle() {
    let w = World::new("co-index-order");
    hung_fetch(&w);
    assert_eq!(w.calls("fake-index").trim_end(), "--before-cycle");
    assert_eq!(
        w.calls("index.head").trim_end(),
        git(&w.upstream(), &["rev-parse", "main"])
    );
    assert!(!w.t().join("calls/index.late").exists());
}

#[test]
fn an_index_that_cannot_be_refreshed_is_a_warning_and_the_cycle_still_runs() {
    let w = World::new("co-index-fails");
    w.touch("index-fails");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "WARN: the code index was not refreshed");
    assert_eq!(w.started(), "7 ");
}

/// A poll that starts the index as its own unit.
fn index_unit(w: &World) -> Run {
    w.touch("index-runs");
    let r = w.poll();
    w.rm_fx("index-runs");
    r
}

#[test]
fn an_index_that_runs_as_its_own_unit_holds_the_cycles_until_the_poll_after_it() {
    let w = World::new("co-index-unit");
    let r = index_unit(&w);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "holding: the code index is refreshed first, as tehas-index.service; the cycles start on the poll after it",
    );
    assert_eq!(w.started(), "");
    let h = w.home.display();
    has(
        &w.calls("fake-index-start"),
        &format!("{h}/logs/dispatch-index.log {h}/state/index.rc {h}/state/dispatch.again"),
    );
    lacks(&w.gh(), "issue edit");
    lacks(&w.gh(), "issue comment");
}

#[test]
fn the_poll_after_the_index_run_says_how_it_ended_and_starts_the_cycles() {
    let w = World::new("co-index-after");
    index_unit(&w);
    let base = git(&w.checkout(), &["rev-parse", "origin/main"]);
    w.home_file("state/index/widget.db", "");
    w.home_file("state/index.json", &json!({"commit": base}).to_string());
    w.home_file("state/index.rc", "1\n");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        &format!(
            "WARN: the code index was not refreshed, see {}/logs/dispatch-index.log",
            w.home.display()
        ),
    );
    assert_eq!(w.calls("fake-index"), "");
    assert_eq!(w.started(), "7 ");
    assert!(!w.home.join("state/index.rc").exists());
}

#[test]
fn an_index_run_that_left_its_exit_status_has_ended_though_systemd_still_shows_its_unit() {
    let w = World::new("co-index-rc");
    w.home_file("state/index.rc", "0\n");
    w.stub(
        "systemctl",
        r#"case "$1 $2" in
  "show tehas.slice") cat "$T/fx/slice.show" ;;
  "--user show") echo active ;;
esac"#,
    );
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    lacks(&r.out, "holding: the cycles wait for the code index");
    assert_eq!(w.started(), "7 ");
}

#[test]
fn an_index_the_indexer_failed_at_this_tip_is_not_run_again_by_a_poll() {
    let w = World::new("co-index-failed");
    let tip = git(&w.checkout(), &["rev-parse", "origin/main"]);
    w.home_file("state/index.failed", &format!("{tip}\n"));
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.calls("fake-index"), "");
    assert_eq!(w.started(), "7 ");
}

#[test]
fn while_an_index_run_holds_its_lock_no_cycle_starts_and_the_poll_says_why() {
    let w = World::new("co-index-lock");
    let lock = OpenOptions::new()
        .create(true)
        .append(true)
        .open(w.home.join("state/index.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    let r = w.poll();
    drop(lock);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "holding: the cycles wait for the code index, which tehas-index.service is refreshing",
    );
    assert_eq!(w.calls("fake-index"), "");
    assert_eq!(w.started(), "");
    lacks(&w.gh(), "issue edit");
}

#[test]
fn a_dry_run_refreshes_no_index() {
    let w = World::new("co-index-dry");
    w.dispatch(&["--dry-run"]);
    assert_eq!(w.calls("fake-index"), "");
}

#[test]
fn a_real_poll_does_not_start_an_issue_the_bot_labelled_and_takes_that_label_off() {
    let w = World::new("co-bot-label");
    w.fx(
        "events-7.json",
        r#"[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"}},
       {"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"the-bot"}}]"#,
    );
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    lacks(&w.gh(), "--add-label tehas:running");
    assert_eq!(
        count_exact(
            &w.gh(),
            "issue edit 7 --repo acme/widget --remove-label tehas:ready"
        ),
        1
    );
    assert_eq!(count_lines(&w.gh(), "issue comment "), 1);
}

#[test]
fn a_cycle_does_not_start_when_the_issue_cannot_be_marked_as_running() {
    let w = World::new("co-mark-fails");
    w.touch("edit-fails");
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "could not mark #7 as running");
    assert_eq!(w.started(), "");
}

const LABELLED: &str = r#"[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-02T10:00:00Z"}]"#;

#[test]
fn an_outside_edit_after_the_label_is_not_approved_though_an_allowed_account_edited_after_it() {
    let w = World::new("co-edits");
    w.fx("events-7.json", LABELLED);
    w.fx(
        "edit-7.json",
        r#"{"data":{"repository":{"issue":{"lastEditedAt":"2026-01-02T10:05:00Z","editor":{"login":"alice"}}}}}"#,
    );
    w.fx(
        "edits-7.json",
        r#"{"totalCount":2,"nodes":[{"editedAt":"2026-01-02T10:01:00Z","editor":{"login":"mallory"}},{"editedAt":"2026-01-02T10:05:00Z","editor":{"login":"alice"}}]}"#,
    );
    let r = w.dispatch(&["--dry-run"]);
    has(
        &r.out,
        "skip #7: its text was changed by 'mallory' after alice applied tehas:ready",
    );
    has(&w.gh(), "userContentEdits(last: 100)");
    // A history longer than the poll read holds it too.
    w.fx(
        "edits-7.json",
        r#"{"totalCount":101,"nodes":[{"editedAt":"2026-01-02T10:05:00Z","editor":{"login":"alice"}}]}"#,
    );
    let r = w.dispatch(&["--dry-run"]);
    has(
        &r.out,
        "skip #7: its text was changed by 'unknown' after alice applied tehas:ready",
    );
    // A history GitHub does not give ends the poll.
    w.rm_fx("edits-7.json");
    w.touch("edits-fail");
    let r = w.dispatch(&["--dry-run"]);
    w.rm_fx("edits-fail");
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "ERROR: unreadable edit history of #7");
}

// The list may name a GitHub account called unknown: a change by no account is
// still nobody's. The rename variant is a fixture of approval.json.
#[test]
fn with_the_account_unknown_on_the_list_a_change_by_no_account_is_not_approved_edit_variant() {
    let w = World::new("co-unknown-edit");
    let mut lines = MINIMAL.to_vec();
    lines.extend([
        "TEHAS_MAX_CYCLES=1",
        r#"TEHAS_ALLOWED_LABELERS="alice unknown""#,
    ]);
    w.instance(&lines);
    w.fx("events-7.json", LABELLED);
    w.fx(
        "edit-7.json",
        r#"{"data":{"repository":{"issue":{"lastEditedAt":"2026-01-02T10:05:00Z","editor":null}}}}"#,
    );
    let r = w.dispatch(&["--dry-run"]);
    has(&r.out, "skip #7: its text was changed by 'unknown'");
    lacks(&r.out, "would run a cycle");
    has(&w.gh(), "userContentEdits(last: 100)");
}

#[test]
fn an_issue_never_edited_asks_for_no_edit_history() {
    let w = World::new("co-never-edited");
    w.fx("events-7.json", LABELLED);
    let r = w.dispatch(&["--dry-run"]);
    has(&r.out, "would run a cycle for #7");
    lacks(
        &w.gh(),
        "userContentEdits(last: 100) { totalCount nodes { editedAt",
    );
}

/// The one rename the poll's own read can differ on: no time. No edit record
/// either, so the rename alone has to hold the start.
fn unstamped_rename(w: &World, actor: Value) -> Run {
    w.fx(
        "edit-7.json",
        r#"{"data":{"repository":{"issue":{"lastEditedAt":null,"editor":null}}}}"#,
    );
    w.fx_json(
        "events-7.json",
        &json!([{"event": "labeled", "label": {"name": "tehas:ready"}, "actor": {"login": "alice"},
            "created_at": "2026-01-02T10:00:00Z"}, {"event": "renamed", "actor": actor}]),
    );
    w.dispatch(&["--dry-run"])
}

#[test]
fn a_title_rename_with_no_time_after_the_label_is_not_approved() {
    let w = World::new("co-rename-no-time");
    let r = unstamped_rename(&w, json!({"login": "mallory"}));
    has(&r.out, "skip #7: its text was changed by 'mallory'");
    lacks(&r.out, "would run a cycle");
}

#[test]
fn a_title_rename_with_no_time_and_no_account_is_not_approved() {
    let w = World::new("co-rename-nobody");
    let r = unstamped_rename(&w, Value::Null);
    has(&r.out, "skip #7: its text was changed by 'unknown'");
    lacks(&r.out, "would run a cycle");
}

/// The pull requests GitHub says close the issue, as (url, state).
fn closing_prs(w: &World, n: u64, prs: &[(&str, &str)]) {
    let nodes: Vec<Value> = prs
        .iter()
        .map(|(u, s)| json!({"url": u, "state": s}))
        .collect();
    w.fx_json(
        &format!("prs-{n}.json"),
        &json!({"data": {"repository": {"issue": {"closedByPullRequestsReferences": {"nodes": nodes}}}}}),
    );
}

/// The open pull requests of the queue's page, each
/// "<number>|<title>|<head>|<author>|<label>|<body>[|<head repository>]". A
/// body of `-` is the null GitHub gives for no description.
fn open_pulls(w: &World, pulls: &[&str]) {
    let list: Vec<Value> = pulls
        .iter()
        .map(|p| {
            let f: Vec<&str> = p.split('|').collect();
            let head = f.get(6).copied().unwrap_or("acme/widget");
            let labels: Vec<Value> = f
                .get(4)
                .filter(|l| !l.is_empty())
                .map(|l| json!({"name": l}))
                .into_iter()
                .collect();
            let body = match f.get(5).copied() {
                Some("-") => Value::Null,
                b => json!(b.unwrap_or("")),
            };
            json!({"number": f[0].parse::<u64>().unwrap(), "title": f[1], "headRefName": f[2],
                "author": {"login": f[3]}, "headRepository": {"nameWithOwner": head},
                "isCrossRepository": head != "acme/widget", "labels": {"nodes": labels}, "body": body})
        })
        .collect();
    w.fx_json("pulls.json", &json!(list));
}

const PR90: &str = "https://github.com/acme/widget/pull/90";

#[test]
fn an_issue_with_an_open_pull_request_that_closes_it_starts_nothing_and_loses_tehas_ready() {
    let w = World::new("co-pr-open");
    closing_prs(&w, 7, &[(PR90, "OPEN")]);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    has(
        &r.out,
        &format!("skip #7: pull request {PR90} is open and closes it"),
    );
    has(
        &w.gh(),
        "issue edit 7 --repo acme/widget --remove-label tehas:ready",
    );
    lacks(&w.gh(), "--add-label tehas:running");
    assert_eq!(count_lines(&w.gh(), "issue comment 7 "), 1);
    has(
        &w.gh(),
        &format!("issue comment 7 --repo acme/widget --body Tehas took `tehas:ready` off this issue and started nothing: {PR90} is open and closes it."),
    );
}

#[test]
fn a_closed_or_merged_pull_request_that_closed_it_holds_nothing_back() {
    let w = World::new("co-pr-closed");
    closing_prs(
        &w,
        7,
        &[
            (PR90, "CLOSED"),
            ("https://github.com/acme/widget/pull/91", "MERGED"),
        ],
    );
    w.poll();
    assert_eq!(w.started(), "7 ");
    has(&w.gh(), "closedByPullRequestsReferences");
}

#[test]
fn an_open_pull_request_into_another_branch_whose_body_closes_it_starts_nothing_and_loses_tehas_ready(
) {
    // No closing reference: GitHub records one only for a pull request into
    // the default branch, and this one is stacked on another.
    let w = World::new("co-pr-stacked");
    closing_prs(&w, 7, &[]);
    open_pulls(
        &w,
        &["90|Stacked work|feature/issue-7-stack|tehas-bot||Closes #7"],
    );
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    has(
        &r.out,
        &format!("skip #7: pull request {PR90} is open and closes it"),
    );
    has(
        &w.gh(),
        "issue edit 7 --repo acme/widget --remove-label tehas:ready",
    );
    lacks(&w.gh(), "--add-label tehas:running");
    has(
        &w.gh(),
        &format!("issue comment 7 --repo acme/widget --body Tehas took `tehas:ready` off this issue and started nothing: {PR90} is open and closes it."),
    );
}

/// A poll that starts #7 and says no pull request holds it.
fn starts_7(w: &World) {
    let r = w.poll();
    assert_eq!(w.started(), "7 ", "{}", r.out);
    lacks(&r.out, "is open and closes it");
}

#[test]
fn a_pull_request_from_a_fork_or_by_an_outsider_that_closes_issues_holds_none_of_them() {
    let w = World::new("co-pr-fork");
    closing_prs(&w, 7, &[]);
    open_pulls(
        &w,
        &[
            "90|Fork|feature/issue-7|mallory||Closes #7, #8, #9|mallory/widget",
            "91|Outsider on a branch|feature/x|mallory||Fixes #7",
            "92|Allowed account fork|feature/y|alice||Resolves #7|alice/widget",
        ],
    );
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
    lacks(&r.out, "is open and closes it");
    lacks(&w.gh(), "started nothing");
}

#[test]
fn a_linked_pull_request_from_a_fork_that_closes_the_issue_holds_nothing_back() {
    let w = World::new("co-pr-linked-fork");
    w.fx_json(
        "prs-7.json",
        &json!({"data": {"repository": {"issue": {"closedByPullRequestsReferences": {"nodes": [{
            "url": PR90, "state": "OPEN", "isCrossRepository": true, "author": {"login": "mallory"},
            "headRepository": {"nameWithOwner": "mallory/widget"}}]}}}}}),
    );
    starts_7(&w);
}

#[test]
fn a_body_of_another_pull_request_that_names_no_closing_keyword_starts_the_issue() {
    let w = World::new("co-pr-no-keyword");
    closing_prs(&w, 7, &[]);
    open_pulls(
        &w,
        &[
            "90|Someone else work|feature/other|alice||See issue #7 and #70 for background",
            "91|Nothing to close|feature/other-2|alice||",
        ],
    );
    starts_7(&w);
}

#[test]
fn a_pull_request_whose_body_closes_70_does_not_hold_issue_7() {
    let w = World::new("co-pr-70");
    closing_prs(&w, 7, &[]);
    open_pulls(&w, &["90|Another issue|feature/other|alice||Closes #70"]);
    starts_7(&w);
}

#[test]
fn a_closed_pull_request_of_the_page_whose_body_closes_it_holds_nothing_back() {
    // The page answers only the open ones: a closed pull request is absent.
    let w = World::new("co-pr-page-closed");
    closing_prs(&w, 7, &[]);
    open_pulls(&w, &["91|Another issue|feature/other|alice||Fixes #8"]);
    starts_7(&w);
}

#[test]
fn a_pull_request_whose_page_answers_no_body_holds_nothing_back() {
    let w = World::new("co-pr-null-body");
    closing_prs(&w, 7, &[]);
    open_pulls(
        &w,
        &["90|No description|feature/issue-7-stack|tehas-bot||-"],
    );
    starts_7(&w);
}

#[test]
fn a_dry_run_of_a_pull_request_into_another_branch_says_it_would_take_the_label_off() {
    let w = World::new("co-pr-stacked-dry");
    closing_prs(&w, 7, &[]);
    open_pulls(
        &w,
        &["90|Stacked work|feature/issue-7-stack|tehas-bot||fixes: #7."],
    );
    let r = w.dispatch(&["--dry-run"]);
    has(
        &r.out,
        &format!("skip #7: pull request {PR90} is open and closes it"),
    );
    lacks(&r.out, "would run a cycle");
    lacks(&w.gh(), "issue edit");
    lacks(&w.gh(), "issue comment");
}

#[test]
fn an_open_pull_request_does_not_hold_up_the_issue_behind_it() {
    let w = World::new("co-pr-behind");
    w.ready(&[7, 8]);
    closing_prs(&w, 7, &[(PR90, "OPEN")]);
    w.poll();
    assert_eq!(w.started(), "8 ");
}

#[test]
fn when_the_label_cannot_be_taken_off_nothing_is_said_and_the_next_poll_tries_again() {
    let w = World::new("co-pr-edit-fails");
    closing_prs(&w, 7, &[(PR90, "OPEN")]);
    w.fx("edit-fails-on", "remove-label tehas:ready\n");
    let r = w.poll();
    assert_eq!(w.started(), "");
    has(
        &r.out,
        "WARN: label change on #7 failed: --remove-label tehas:ready",
    );
    lacks(&w.gh(), "issue comment 7");
}

#[test]
fn a_dry_run_says_it_would_take_the_label_off_and_changes_nothing() {
    let w = World::new("co-pr-dry");
    closing_prs(&w, 7, &[(PR90, "OPEN")]);
    let r = w.dispatch(&["--dry-run"]);
    has(
        &r.out,
        &format!("skip #7: pull request {PR90} is open and closes it"),
    );
    lacks(&r.out, "would run a cycle");
    lacks(&w.gh(), "issue edit");
    lacks(&w.gh(), "issue comment");
}

#[test]
fn a_pause_answer_that_cannot_be_understood_dispatches_nothing() {
    let w = World::new("co-pause-null");
    w.fx("control.json", r#"{"labels":null}"#);
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    assert_eq!(w.started(), "");
}

#[test]
fn tehas_branch_overrides_the_checkouts_default_branch() {
    let w = World::new("co-branch");
    git(&w.checkout(), &["branch", "-q", "release", "main"]);
    git(&w.upstream(), &["branch", "-q", "release", "main"]);
    git(&w.checkout(), &["fetch", "-q", "origin"]);
    let mut lines = MINIMAL.to_vec();
    lines.push("TEHAS_BRANCH=release");
    w.instance(&lines);
    w.poll();
    assert_eq!(git(&w.checkout(), &["branch", "--show-current"]), "release");
}

#[test]
fn a_checkout_without_origin_head_stops_before_any_label_changes() {
    let w = World::new("co-no-head");
    git(&w.checkout(), &["remote", "set-head", "origin", "-d"]);
    let r = w.poll();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "default branch");
    lacks(&w.gh(), "issue edit");
}

#[test]
fn init_labels_is_tehas_labels_now_here_it_is_an_unknown_argument() {
    let w = World::new("co-init-labels");
    let r = w.dispatch(&["--init-labels"]);
    assert_eq!(r.rc, 64, "{}", r.out);
    has(&r.out, "tehas dispatch: unknown argument: --init-labels");
    assert_eq!(w.gh(), "");
}
