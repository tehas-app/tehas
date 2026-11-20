//! The record of a cycle whose last GitHub step failed,
//! state/cycles/<issue>.end-failed.json: the step and whether the failure was
//! GitHub's. The poll that finishes it is test/dispatch_auto_finish_test.sh.

mod common;

use common::{has, World};
use serde_json::{json, Value};
use std::fs;

/// A cycle that stopped in commit-pr with no pull request, on `message`.
fn in_commit_pr(w: &World, message: &str) {
    w.ended(
        "handoff",
        "commit-pr",
        "feature/issue-7",
        "step-failed:commit-pr",
    );
    w.entry(|e| e["failure"] = json!({"kind": "tooling", "step": "commit-pr", "message": message}));
}

fn record(w: &World) -> Option<Value> {
    let text = fs::read_to_string(w.state().join("7.end-failed.json")).ok()?;
    Some(serde_json::from_str(&text).unwrap())
}

/// What the record says: its step, whether GitHub failed, why, its tries
/// and how it stopped.
fn said(w: &World) -> Value {
    let r = record(w).expect("no record");
    json!([r["step"], r["github"], r["why"], r["tries"], r["stopped"]])
}

/// A park whose handoff is pi-rukas's, which the host posts.
fn handoff(w: &World) {
    w.ended(
        "handoff",
        "develop",
        "feature/issue-7",
        "verify-timeout:develop",
    );
    w.entry(|e| {
        e["handoff"] = json!({"channel": "host",
            "body": "<!-- pi-rukas:handoff issue=7 cap=verify-timeout -->\nNot pushed."})
    });
}

#[test]
fn a_push_that_failed_502_is_on_record_as_githubs() {
    let w = World::new("ef-502");
    in_commit_pr(
        &w,
        "git push origin feature/issue-7: fatal: unable to access: The requested URL returned error: 502",
    );
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert_eq!(
        said(&w),
        json!(["push", true, "returned error: 502", [], ""])
    );
    assert_eq!(record(&w).unwrap()["issue"], 7);
    has(
        &r.out,
        "#7: its push failed on GitHub's side (returned error: 502): on record",
    );
}

#[test]
fn a_pull_request_refused_422_is_on_record_as_the_requests_own() {
    let w = World::new("ef-422");
    in_commit_pr(&w, "gh pr create: HTTP 422: Validation Failed");
    w.run7();
    assert_eq!(said(&w), json!(["pr", false, "HTTP 422", [], ""]));
}

#[test]
fn a_step_that_failed_for_anything_but_github_is_no_record_and_the_record_an_earlier_cycle_left_goes(
) {
    let w = World::new("ef-other");
    fs::create_dir_all(w.state()).unwrap();
    fs::write(
        w.state().join("7.end-failed.json"),
        r#"{"issue":7,"attempt":1,"step":"push","github":true,"why":"HTTP 502","at":1,"tries":[],"stopped":"","said":false}"#,
    )
    .unwrap();
    in_commit_pr(&w, "cargo test failed in other.rs");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert!(record(&w).is_none());
}

#[test]
fn a_handoff_comment_that_failed_503_is_on_record_as_githubs() {
    let w = World::new("ef-503");
    handoff(&w);
    // The park's question and the comment in its place both fail, and the
    // read after each 503 finds no comment.
    w.touch("post-fails");
    w.fx("comment-fails", "HTTP 503: Service Unavailable\n");
    w.fx("comments.json", "[]\n");
    let r = w.run7();
    has(&r.out, "WARN: the handoff of #7 reached no comment");
    assert_eq!(said(&w), json!(["handoff", true, "HTTP 503", [], ""]));
}

#[test]
fn a_handoff_comment_that_may_have_been_posted_is_no_failed_end() {
    let w = World::new("ef-maybe");
    handoff(&w);
    // The comment fails 503 and no read tells whether GitHub took it.
    w.touch("post-fails");
    w.fx("comment-fails", "HTTP 503: Service Unavailable\n");
    let r = w.run7();
    has(
        &r.out,
        "WARN: the handoff of #7 may not have reached a comment",
    );
    assert!(record(&w).is_none());
}
