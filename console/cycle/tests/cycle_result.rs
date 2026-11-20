//! A merge, the result the cycle reads, the pull requests of the branch, and
//! the workspace it removes or keeps.

mod common;

use common::{has, lacks, World};
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// The mark tehas work leaves on the directory of its workspaces.
fn mark(w: &World) {
    let work = w.home.join("work");
    fs::create_dir_all(&work).unwrap();
    fs::write(work.join(".tehas-workspaces"), "").unwrap();
}

/// The copy of the workspace an earlier stopped cycle left.
fn before_retry(w: &World) -> PathBuf {
    let mut p = w.workdir().into_os_string();
    p.push(".before-retry");
    PathBuf::from(p)
}

fn there(p: &Path) -> bool {
    p.symlink_metadata().is_ok()
}

/// A state file of the shape pi-rukas wrote once, under `dir`.
fn work_state(dir: &Path) {
    let d = dir.join(".pi/work-state");
    fs::create_dir_all(&d).unwrap();
    fs::write(
        d.join("7.json"),
        r#"{"pipelineState":{"status":"merged","currentStep":"done","branchName":"feature/x"}}"#,
    )
    .unwrap();
}

fn pr9(w: &World) {
    w.fx("pr.txt", "https://github.com/acme/widget/pull/9\n");
}

fn kept(w: &World) -> String {
    format!("The workspace is kept at `{}`", w.kept_here())
}

#[test]
fn a_merged_cycle_only_loses_its_running_label() {
    let w = World::new("res-merged");
    w.ended("merged", "done", "feature/x", "");
    w.merged_pr();
    w.run7();
    let gh = w.gh();
    has(
        &gh,
        "issue edit 7 --repo acme/widget --remove-label tehas:running",
    );
    lacks(&gh, "--add-label");
    has(
        &gh,
        "The change was merged: https://github.com/acme/widget/pull/9",
    );
}

#[test]
fn a_merged_state_with_no_merged_pull_request_is_a_claim_parked_and_nothing_is_removed() {
    let w = World::new("res-claim");
    w.ended("merged", "done", "feature/x", "");
    mark(&w);
    fs::create_dir_all(before_retry(&w)).unwrap();
    w.fx("image.label", &format!("{}\n", "e".repeat(40)));
    let r = w.run7();
    assert_eq!(r.rc, 0);
    let gh = w.gh();
    has(
        &gh,
        "pr list --repo acme/widget --head feature/x --author tehas-bot[bot] --state all",
    );
    has(
        &gh,
        "--remove-label tehas:running --add-label needs-human-attention",
    );
    has(&gh, "Tehas cycle stopped: its state says `merged`, but the bot has no merged pull request for this issue on `feature/x`");
    lacks(&gh, "The change was merged");
    assert!(there(&w.workdir()) && there(&before_retry(&w)));
    assert!(!there(&w.home.join("state/rukas-proven")));
    // park_q reads a string; the store keeps the id as a number.
    assert_eq!(w.park()["question"], 4002);
}

#[test]
fn a_merged_state_whose_branch_has_only_an_open_pull_request_is_parked_and_the_comment_names_it() {
    let w = World::new("res-open");
    w.ended("merged", "done", "feature/x", "");
    pr9(&w);
    w.run7();
    let gh = w.gh();
    has(&gh, "--add-label needs-human-attention");
    has(
        &gh,
        "Its branch has an open pull request: https://github.com/acme/widget/pull/9",
    );
    lacks(&gh, "tehas:awaiting-merge");
}

#[test]
fn a_merged_pull_request_of_another_issue_does_not_make_a_merge() {
    let w = World::new("res-other-merged");
    w.ended("merged", "done", "feature/issue-9-other", "");
    w.merged_pr();
    w.fx("pr-body", "Fixes #9\n");
    w.run7();
    let gh = w.gh();
    has(&gh, "--add-label needs-human-attention");
    has(
        &gh,
        "has a pull request of the bot that does not name this issue",
    );
    lacks(&gh, "The change was merged");
}

#[test]
fn a_handoff_whose_branch_is_another_issues_pull_request_is_parked_and_nothing_is_posted_on_it() {
    let w = World::new("res-other-handoff");
    w.ended("handoff", "commit-pr", "feature/issue-9-other", "");
    pr9(&w);
    w.fx("pr-body", "Fixes #9 and mentions #7\n");
    w.run7();
    let gh = w.gh();
    lacks(&gh, "tehas:awaiting-merge");
    lacks(&gh, "pr comment");
    has(&gh, "--add-label needs-human-attention");
    has(&gh, "The branch `feature/issue-9-other` has a pull request of the bot that does not name this issue");
    lacks(&gh, "Its branch has an open pull request");
}

#[test]
fn a_pull_request_whose_head_is_the_issues_branch_names_it_whatever_its_body_says() {
    let w = World::new("res-head");
    w.ended("handoff", "commit-pr", "feature/issue-7-the-change", "");
    pr9(&w);
    w.fx("pr-body", "No keyword here.\n");
    w.run7();
    has(
        &w.gh(),
        "--remove-label tehas:running --add-label tehas:awaiting-merge",
    );
}

#[test]
fn a_body_that_closes_a_longer_number_does_not_name_this_issue() {
    let w = World::new("res-longer");
    w.ended("handoff", "commit-pr", "feature/x", "");
    pr9(&w);
    w.fx("pr-body", "Closes #70\n");
    w.run7();
    lacks(&w.gh(), "tehas:awaiting-merge");
}

#[test]
fn a_result_that_is_a_link_is_not_read() {
    let w = World::new("res-link");
    let elsewhere = w.t().join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    fs::write(
        elsewhere.join("7.json"),
        json!({"schemaVersion": 5, "issues": [{"issue": 7, "status": "merged",
            "failedStep": "done", "branch": "feature/x"}]})
        .to_string(),
    )
    .unwrap();
    w.work(r#"ln -sf "$T/elsewhere/7.json" "$TEHAS_HOME/state/cycles/7.result.json""#);
    w.merged_pr();
    let r = w.run7();
    has(&r.out, "WARN: the result of #7 is a link, it is not read");
    let gh = w.gh();
    has(&gh, "status `unknown`");
    lacks(&gh, "pr list");
    lacks(&gh, "The change was merged");
    // The end is on record all the same, and the store keeps nothing through
    // the link (F7).
    has(&r.out, "WARN: the result of #7 is not on record: refused");
    let c = &w.row(&["cycle-show", "7"])["cycle"];
    assert!(!c["ended_at"].is_null());
    assert_eq!(c["end_reason"], "unknown");
    assert_eq!(
        w.row(&["history-show", "7"])["rows"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn a_progress_older_than_the_cycles_start_is_not_on_record_as_its_own_g3() {
    let w = World::new("res-stale");
    w.ended("handoff", "commit-pr", "feature/x", "");
    w.work(r#"touch -d "@$(($(date +%s) - 3600))" "$TEHAS_HOME/state/cycles/7.progress.json""#);
    let r = w.run7();
    has(&r.out, "WARN: the progress of #7 is not on record: stale");
    lacks(&r.out, "the result of #7 is not on record");
}

#[test]
fn a_state_file_the_cycle_left_in_its_workspace_is_not_read_only_the_runners_result_is() {
    let w = World::new("res-workstate");
    work_state(&w.workdir());
    w.merged_pr();
    w.run7();
    let gh = w.gh();
    has(&gh, "status `unknown`");
    lacks(&gh, "pr list");
}

#[test]
fn a_result_of_another_schema_or_of_another_issue_is_no_result() {
    let changes: [fn(&mut Value); 2] = [
        |r| r["schemaVersion"] = json!(3),
        |r| r["issues"][0]["issue"] = json!(8),
    ];
    for change in changes {
        let w = World::new("res-schema");
        w.ended("merged", "done", "feature/x", "");
        w.result(change);
        w.merged_pr();
        w.run7();
        has(&w.gh(), "status `unknown`");
    }
}

#[test]
fn a_result_left_by_an_earlier_cycle_is_not_this_cycles() {
    let w = World::new("res-earlier");
    fs::create_dir_all(w.state()).unwrap();
    fs::write(
        w.state().join("7.result.json"),
        json!({"schemaVersion": 5, "issues": [{"issue": 7, "status": "merged", "branch": "feature/x"}]})
            .to_string(),
    )
    .unwrap();
    w.run7();
    has(&w.gh(), "status `unknown`");
    assert!(!there(&w.state().join("7.result.json")));
}

#[test]
fn a_pull_request_a_person_merged_while_the_cycle_ran_ends_it_as_merged_no_park_no_question() {
    let w = World::new("res-person");
    w.ended("running", "handoff", "feature/x", "");
    w.merged_pr();
    mark(&w);
    w.fx("image.label", &format!("{}\n", "e".repeat(40)));
    w.work("exit 2");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    let gh = w.gh();
    let edits: Vec<&str> = gh.lines().filter(|l| l.starts_with("issue edit")).collect();
    assert_eq!(
        edits,
        ["issue edit 7 --repo acme/widget --remove-label tehas:running"]
    );
    has(&gh, "Tehas cycle ended: its pull request https://github.com/acme/widget/pull/9 was merged while the cycle ran (last step `handoff`");
    has(&gh, "runner exit 2");
    lacks(&gh, "stopped without a pull request");
    lacks(&gh, "issues/7/comments");
    assert!(w.park()["question"].is_null());
    has(&r.out, "done #7 rc=0 status=merged");
    has(&r.out, "pr=https://github.com/acme/widget/pull/9");
    assert!(there(&w.workdir()));
    has(&gh, &kept(&w));
    assert!(!there(&w.home.join("state/rukas-proven")));
}

#[test]
fn a_runner_stopped_at_lens_fix_after_its_pull_request_merged_ends_the_cycle_as_merged_with_0_its_own_exit_kept(
) {
    let w = World::new("res-lensfix");
    w.ended("interrupted", "lens-fix", "feature/x", "-");
    w.merged_pr();
    w.work(
        r#"mkdir -p "$TEHAS_HOME/state/cycles"; echo 21 >"$TEHAS_HOME/state/cycles/7.runner_exit""#,
    );
    let r = w.run7();
    assert_eq!(r.rc, 0);
    has(&r.out, "done #7 rc=0 status=merged step=lens-fix");
    has(&w.gh(), "runner exit 21");
    assert_eq!(w.row(&["cycle-show", "7"])["cycle"]["runner_exit"], 21);
    assert!(w.park()["question"].is_null());
}

#[test]
fn a_pull_request_closed_without_a_merge_parks_the_issue_without_the_retry_boxes() {
    let w = World::new("res-closed");
    w.ended("running", "review", "feature/x", "");
    pr9(&w);
    w.fx("pr-state", "CLOSED\n");
    mark(&w);
    w.work("exit 2");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    let gh = w.gh();
    has(&gh, "issue edit 7 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention");
    has(&gh, "Tehas cycle stopped: its pull request https://github.com/acme/widget/pull/9 was closed without being merged");
    has(&gh, &kept(&w));
    has(&gh, "To retry, apply `tehas:ready`.");
    lacks(&gh, "issues/7/comments");
    lacks(&gh, "tehas:question");
    assert!(w.park()["question"].is_null());
    assert!(there(&w.workdir()));
}

#[test]
fn an_open_pull_request_beside_a_closed_one_is_the_cycles() {
    let w = World::new("res-beside");
    w.ended("handoff", "commit-pr", "feature/x", "");
    let out = std::process::Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .unwrap();
    let at = String::from_utf8(out.stdout).unwrap().trim().to_string();
    w.fx(
        "pr.json",
        &json!([
            {"url": "https://github.com/acme/widget/pull/8", "state": "CLOSED", "body": "Fixes #7",
             "headRefName": "feature/x", "createdAt": at},
            {"url": "https://github.com/acme/widget/pull/9", "state": "OPEN", "body": "Fixes #7",
             "headRefName": "feature/x", "createdAt": at}])
        .to_string(),
    );
    w.run7();
    let gh = w.gh();
    has(
        &gh,
        "--remove-label tehas:running --add-label tehas:awaiting-merge",
    );
    has(
        &gh,
        "Pull request waiting for a human: https://github.com/acme/widget/pull/9",
    );
}

#[test]
fn a_state_file_in_the_product_checkout_is_not_the_cycles_the_container_never_writes_there() {
    let w = World::new("res-checkout");
    work_state(&w.home.join("repos/widget"));
    w.run7();
    has(
        &w.gh(),
        "--remove-label tehas:running --add-label needs-human-attention",
    );
}

#[test]
fn the_workspace_of_a_cycle_that_left_a_pull_request_is_removed() {
    let w = World::new("res-rm-pr");
    w.ended("handoff", "commit-pr", "feature/x", "");
    mark(&w);
    pr9(&w);
    w.run7();
    assert!(!there(&w.workdir()));
}

#[test]
fn and_with_it_the_one_kept_from_an_earlier_stopped_cycle_of_the_issue() {
    let w = World::new("res-rm-before");
    w.ended("handoff", "commit-pr", "feature/x", "");
    mark(&w);
    fs::create_dir_all(before_retry(&w)).unwrap();
    pr9(&w);
    w.run7();
    assert!(!there(&w.workdir()) && !there(&before_retry(&w)));
}

#[test]
fn so_is_the_workspace_of_a_merged_cycle() {
    let w = World::new("res-rm-merged");
    w.ended("merged", "done", "feature/x", "");
    w.merged_pr();
    mark(&w);
    w.run7();
    assert!(!there(&w.workdir()));
}

#[test]
fn the_workspace_of_a_cycle_that_stopped_is_kept_for_a_person_to_read() {
    let w = World::new("res-keep");
    w.ended("aborted", "develop", "feature/x", "");
    mark(&w);
    w.run7();
    assert!(there(&w.workdir()));
}

#[test]
fn and_so_is_the_one_kept_from_the_cycle_before_it() {
    let w = World::new("res-keep-before");
    w.ended("aborted", "develop", "feature/x", "");
    mark(&w);
    fs::create_dir_all(before_retry(&w)).unwrap();
    w.run7();
    assert!(there(&w.workdir()) && there(&before_retry(&w)));
}

// A merge someone else made while the cycle still reviewed is not the cycle's
// own merge: what it had not pushed is only on the host.
#[test]
fn the_workspace_of_a_cycle_a_merge_interrupted_is_kept_and_its_comment_names_it() {
    let w = World::new("res-interrupted");
    w.ended("running", "review", "feature/x", "");
    w.merged_pr();
    mark(&w);
    w.work("exit 2");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert!(there(&w.workdir()));
    let gh = w.gh();
    has(&gh, "was merged while the cycle ran");
    has(&gh, &kept(&w));
    lacks(&gh, &w.home.join("work").display().to_string());
    lacks(&gh, "needs-human-attention");
}

#[test]
fn and_so_is_the_one_kept_from_an_earlier_stopped_cycle_of_an_interrupted_merge() {
    let w = World::new("res-interrupted-before");
    w.ended("running", "review", "feature/x", "");
    w.merged_pr();
    mark(&w);
    fs::create_dir_all(before_retry(&w)).unwrap();
    w.work("exit 2");
    w.run7();
    assert!(there(&w.workdir()) && there(&before_retry(&w)));
    has(&w.gh(), &kept(&w));
}

#[test]
fn a_cycle_the_dispatcher_stopped_for_a_closed_issue_names_the_workspace_it_kept() {
    let w = World::new("res-closed-issue");
    w.ended("running", "adversarial", "feature/x", "");
    mark(&w);
    w.work(r#"mkdir -p "$TEHAS_HOME/state/cycles"; echo closed >"$TEHAS_HOME/state/cycles/7.done"; exit 2"#);
    w.run7();
    assert!(there(&w.workdir()));
    let gh = w.gh();
    has(&gh, "this issue was closed while it ran");
    has(&gh, &kept(&w));
}

// A run that never cloned a workspace has no directory for its comment to name.
#[test]
fn a_cycle_that_made_no_workspace_is_not_said_to_have_kept_one() {
    let w = World::new("res-noworkspace");
    w.ended("interrupted", "review", "feature/x", "");
    fs::remove_dir_all(w.workdir()).unwrap();
    w.run7();
    assert!(!there(&w.workdir()));
    let gh = w.gh();
    has(&gh, "Tehas cycle stopped without a pull request");
    has(&gh, "This cycle left no workspace on this host");
    lacks(&gh, &kept(&w));
}

// A run that moved an earlier attempt aside and then failed before it cloned
// leaves only that copy.
#[test]
fn a_cycle_that_cloned_no_workspace_names_the_copy_an_earlier_attempt_left() {
    let w = World::new("res-copy");
    w.ended("interrupted", "review", "feature/x", "");
    fs::remove_dir_all(w.workdir()).unwrap();
    fs::create_dir_all(before_retry(&w)).unwrap();
    w.run7();
    assert!(!there(&w.workdir()));
    let gh = w.gh();
    has(&gh, "Tehas cycle stopped without a pull request");
    has(
        &gh,
        &format!("The workspace is kept at `{}.before-retry`", w.kept_here()),
    );
    lacks(&gh, "This cycle left no workspace on this host");
}

#[test]
fn a_directory_tehas_work_did_not_mark_is_never_removed() {
    let w = World::new("res-unmarked");
    w.ended("merged", "done", "feature/x", "");
    w.merged_pr();
    let r = w.run7();
    assert!(there(&w.workdir()));
    has(
        &r.out,
        &format!(
            "WARN: {} is not marked by tehas work, leaving the workspace {}",
            w.home.join("work").display(),
            w.workdir().display()
        ),
    );
}

#[test]
fn a_workspace_that_cannot_be_removed_is_said_so() {
    let w = World::new("res-stuck");
    w.ended("merged", "done", "feature/x", "");
    w.merged_pr();
    mark(&w);
    let work = w.home.join("work");
    fs::set_permissions(&work, fs::Permissions::from_mode(0o555)).unwrap();
    let r = w.run7();
    fs::set_permissions(&work, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(there(&w.workdir()));
    has(
        &r.out,
        &format!(
            "WARN: could not remove the workspace {}",
            w.workdir().display()
        ),
    );
}

#[test]
fn another_issues_workspace_is_left_alone() {
    let w = World::new("res-other-ws");
    w.ended("merged", "done", "feature/x", "");
    w.merged_pr();
    mark(&w);
    let other = w.home.join("work/widget-9");
    fs::create_dir_all(&other).unwrap();
    w.run7();
    assert!(there(&other));
}
