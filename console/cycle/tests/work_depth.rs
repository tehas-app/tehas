//! The history of a cycle's workspace, TEHAS_CLONE_DEPTH: the default branch
//! alone at the depth, all of it for 0, and a value that is no depth stopping
//! the start before anything is cloned. A refresh whose pull request started
//! beyond the depth has its workspace deepened to the commit the branch
//! started from, so the conflict check judges the merge and
//! `git merge-base --is-ancestor` answers there as in a full clone.

mod work;

use serde_json::{json, Value};
use std::fs;
use work::{absent, body_sha256, epoch, git_out, git_rc, has, World};

/// Twenty commits on main and a branch of the product beside it.
fn twenty(name: &str) -> World {
    let w = World::new(name);
    w.stub_github();
    for n in 1..=20 {
        w.commit_upstream(&format!("main {n}"), None);
    }
    git_out(&w.upstream, &["branch", "other", "HEAD~5"]);
    w.fetched();
    w
}

fn count(w: &World) -> String {
    git_out(&w.ws, &["rev-list", "--count", "HEAD"])
}

#[test]
fn a_new_cycles_workspace_holds_the_default_branch_alone_at_the_depth_the_instance_sets() {
    let w = twenty("depth-3");
    w.minimal(&["TEHAS_CLONE_DEPTH=3"]);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(
        git_out(&w.ws, &["rev-parse", "HEAD"]),
        git_out(&w.upstream, &["rev-parse", "main"])
    );
    assert_eq!(count(&w), "3");
    assert_eq!(
        git_out(&w.ws, &["rev-parse", "--is-shallow-repository"]),
        "true"
    );
    assert_eq!(
        git_out(&w.ws, &["for-each-ref", "--format=%(refname)"]),
        "refs/heads/main\nrefs/remotes/origin/HEAD\nrefs/remotes/origin/main"
    );
    assert_eq!(git_out(&w.ws, &["tag"]), "");
}

#[test]
fn the_default_depth_is_100_commits() {
    let w = twenty("depth-default");
    w.minimal(&[]);
    for n in 21..=110 {
        w.commit_upstream(&format!("main {n}"), None);
    }
    w.fetched();
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(count(&w), "100");
}

#[test]
fn tehas_clone_depth_0_gives_the_workspace_full_history() {
    let w = twenty("depth-0");
    for n in 21..=110 {
        w.commit_upstream(&format!("main {n}"), None);
    }
    w.fetched();
    let total = git_out(&w.upstream, &["rev-list", "--count", "main"]);
    w.minimal(&["TEHAS_CLONE_DEPTH=0"]);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(count(&w), total);
    assert_eq!(
        git_out(&w.ws, &["rev-parse", "--is-shallow-repository"]),
        "false"
    );
}

#[test]
fn a_depth_that_is_no_whole_number_stops_the_start_with_78_and_names_it_before_anything_is_cloned()
{
    let w = twenty("depth-bad");
    for bad in ["-1", "full", "01"] {
        w.clear_calls();
        let _ = fs::remove_dir_all(&w.ws);
        w.minimal(&[&format!("TEHAS_CLONE_DEPTH={bad}")]);
        let r = w.work7();
        assert_eq!(r.rc, 78, "{bad}: {}", r.out);
        has(
            &r.out,
            "TEHAS_CLONE_DEPTH is not valid: expected a whole number of 0 or more",
        );
        assert!(absent(&w.ws), "{bad}: the workspace was made");
        assert!(w.cycle_args().is_empty(), "{bad}: a container started");
    }
}

// --- A refresh of a pull request that started beyond the depth -------------

/// The approval of a refresh of pull request 50 at `head`, as the dispatcher
/// records it.
fn refresh_at(w: &World, head: &str) {
    w.issue_is("", "NONE", "");
    w.store_db();
    let input = w.path("refresh-input.json");
    let v = json!({"links": [], "refresh_of": {"pr": 50, "head_sha": head, "branch": "main"}});
    fs::write(&input, v.to_string()).unwrap();
    w.approve(
        7,
        "alice",
        "refresh",
        epoch("2026-10-01T00:00:00Z"),
        &[
            "--body-sha",
            &body_sha256(""),
            "--input-file",
            &input.to_string_lossy(),
            "--refresh-pr",
            "50",
            "--refresh-head",
            head,
        ],
    );
    w.started(7, &[]);
}

/// A pull request's branch off main of `n` commits, the first writing `file`,
/// then main moved on by 8 commits, the last writing `file` too for a
/// conflict. The branch's first head and its last.
fn old_pr(
    w: &World,
    branch: &str,
    file: &str,
    n: u32,
    line: &str,
    conflict: bool,
) -> (String, String) {
    let up = &w.upstream;
    git_out(up, &["checkout", "-q", "-b", branch, "main"]);
    w.commit_upstream("the pull request", Some((file, line)));
    let first = git_out(up, &["rev-parse", "HEAD"]);
    for k in 2..=n {
        w.commit_upstream(&format!("the pull request {k}"), None);
    }
    let last = git_out(up, &["rev-parse", "HEAD"]);
    git_out(up, &["checkout", "-q", "main"]);
    for k in 1..=7 {
        w.commit_upstream(&format!("main moves {k}"), None);
    }
    if conflict {
        w.commit_upstream("main moves 8", Some((file, "main moved on")));
    } else {
        w.commit_upstream("main moves 8", None);
    }
    w.fetched();
    (first, last)
}

fn refreshed(name: &str) -> World {
    let w = twenty(name);
    w.caps_continue();
    w.minimal(&["TEHAS_CLONE_DEPTH=3"]);
    w
}

#[test]
fn a_conflict_refresh_on_a_branch_that_started_beyond_the_depth_is_judged_and_names_the_file() {
    let w = refreshed("depth-conflict");
    let (_, head) = old_pr(
        &w,
        "feature/far",
        "far.txt",
        2,
        "the pull request's line",
        true,
    );
    let base = git_out(&w.upstream, &["merge-base", "main", &head]);
    refresh_at(&w, &head);
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    assert!(w.cycle_args().is_empty());
    has(
        &r.out,
        "pull request #50 conflicts with origin/main in 1 file(s)",
    );
    let note = w.home.join("state/cycles/7.refresh-conflict");
    let got: Value = serde_json::from_str(&fs::read_to_string(&note).unwrap()).unwrap();
    assert_eq!(
        got,
        json!({"branch": "main", "more": 0, "paths": ["far.txt"], "pr": 50})
    );
    // The workspace holds the start of the branch, and is still shallow below it.
    assert_eq!(git_out(&w.ws, &["merge-base", "origin/main", &head]), base);
    assert_eq!(
        git_out(&w.ws, &["rev-parse", "--is-shallow-repository"]),
        "true"
    );
}

#[test]
fn a_pull_request_picked_up_again_its_head_reviewed_earlier_beyond_the_depth_is_an_ancestor_of_its_head(
) {
    let w = refreshed("depth-again");
    let (first, head) = old_pr(
        &w,
        "feature/long",
        "long.txt",
        12,
        "a file of its own",
        false,
    );
    refresh_at(&w, &head);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    has(&w.cycle_args().join(" "), "--continue-pr");
    assert_eq!(
        git_rc(&w.ws, &["merge-base", "--is-ancestor", &first, &head]),
        0
    );
    assert_eq!(
        git_out(&w.ws, &["merge-base", "origin/main", &head]),
        git_out(&w.upstream, &["merge-base", "main", &head])
    );
}

#[test]
fn a_refresh_whose_head_the_checkout_does_not_hold_starts_no_container_78_and_says_why() {
    let w = refreshed("depth-unheld");
    refresh_at(&w, &"b".repeat(40));
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    assert!(w.cycle_args().is_empty());
    has(
        &r.out,
        "the workspace of the refresh of pull request #50 cannot hold the commit its branch started from",
    );
}
