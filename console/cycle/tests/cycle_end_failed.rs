//! The record of a cycle whose last GitHub step failed,
//! state/cycles/<issue>.end-failed.json: the step and whether the failure was
//! GitHub's. The poll that finishes it is console/dispatch/tests/dispatch_auto_finish.rs.

mod common;

use common::{has, said as said_of, site_of, World};
use serde_json::{json, Value};
use std::fs;
use tehas_cycle::end_failed::{self, Failure, Left};
use tehas_ports::fake::{BlueprintsOp, FakeSite, Fault, Faulty, Op, TicketsOp};

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
    // The park's question and the comment in its place both fail 503, and
    // the read after each finds no comment.
    let site = site_of(&w);
    let t = w.report_through(
        &site,
        &Faulty::new(FakeSite::new(&site)).on(
            Op::Tickets(TicketsOp::Note),
            Fault::Server { applied: false },
        ),
    );
    // The question once, then the plain comment with the handoff, three
    // tries; none of them is on the issue.
    let notes = |ws: &[tehas_ports::Write]| {
        ws.iter()
            .filter(|w| matches!(w, tehas_ports::Write::TicketNote { .. }))
            .count()
    };
    assert_eq!(notes(&t.sent), 4);
    assert_eq!(notes(&t.applied), 0);
    has(
        &said_of(&t.sent),
        "note: <!-- pi-rukas:handoff issue=7 cap=verify-timeout:develop -->",
    );
    assert!(!t.written.handed_off);
    let failed = t.written.handoff_failed.expect("the handoff failed");
    // What tehas-cycle keeps of it: a failure of GitHub's at the handoff,
    // in the words of the site's answer.
    let Failure::Answered(answer) = &failed else {
        panic!("{failed:?}")
    };
    let result = fs::read_to_string(w.state().join("7.result.json")).unwrap();
    let left = Left {
        issue: 7,
        attempt: 1,
        result: Some(&result),
        progress: None,
        pr: false,
        handoff_failed: Some(&failed),
    };
    let r = end_failed::of(&left, 1).expect("on record");
    assert_eq!(
        json!([r.step, r.github, r.host, r.why, r.tries, r.stopped]),
        json!(["handoff", true, true, answer.line(), [], ""])
    );
}

#[test]
fn a_handoff_comment_that_may_have_been_posted_is_no_failed_end() {
    let w = World::new("ef-maybe");
    handoff(&w);
    // The question with the handoff fails 503 and no read tells whether
    // GitHub took it: it is not sent again, nor followed by the comment.
    let site = site_of(&w);
    let t = w.report_through(
        &site,
        &Faulty::new(FakeSite::new(&site))
            .on(
                Op::Tickets(TicketsOp::Note),
                Fault::Server { applied: false },
            )
            .on(Op::Blueprints(BlueprintsOp::Comments), Fault::Unreachable),
    );
    has(&w.report_log(), "WARN: a write on #7 may have been posted");
    assert!(!t.written.handed_off);
    assert!(t.written.handoff_maybe);
    assert!(t.written.handoff_failed.is_none());
    let result = fs::read_to_string(w.state().join("7.result.json")).unwrap();
    let left = Left {
        issue: 7,
        attempt: 1,
        result: Some(&result),
        progress: None,
        pr: false,
        handoff_failed: t.written.handoff_failed.as_ref(),
    };
    assert!(end_failed::of(&left, 1).is_none());
}
