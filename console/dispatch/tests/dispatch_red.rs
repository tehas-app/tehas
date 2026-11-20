//! No new cycle while the check TEHAS_BRANCH_CHECK names failed on the head
//! commit of the branch: what the poll asks GitHub and what it runs. The
//! decisions are fixtures in tests/fixtures/red.json.

mod common;

use common::{has, lacks, plan_body, World};
use serde_json::json;
use std::process::Command;

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

/// The one run of the check on the head.
fn checks(w: &World, status: &str, conclusion: Option<&str>) {
    w.fx_json(
        "checks.json",
        &json!({"total_count": 1, "check_runs": [{"name": "verify", "head_sha": SHA,
            "status": status, "conclusion": conclusion}]}),
    );
}

fn red_instance(w: &World) {
    w.slots(2, &["TEHAS_BRANCH_CHECK=verify"]);
}

fn check_calls(w: &World) -> usize {
    w.gh()
        .lines()
        .filter(|l| l.contains("commits/main/check-runs"))
        .count()
}

fn unknown_hold(name: &str) -> String {
    format!("holding: the check \"{name}\" on main at 0123456789ab finished without saying it passed: cycles start")
}

fn now() -> i64 {
    tehas_core::time::now()
}

/// jq -r of `filter` over `input`.
fn jqr(filter: &str, input: &str) -> String {
    use std::io::Write;
    let mut c = Command::new("jq")
        .args(["-r", filter])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    c.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let out = c.wait_with_output().unwrap();
    String::from_utf8_lossy(&out.stdout)
        .trim_end_matches('\n')
        .to_string()
}

#[test]
fn red_a_failed_check_on_the_head_starts_nothing_and_says_so() {
    let w = World::new("red-failed");
    red_instance(&w);
    w.ready(&[7, 8]);
    checks(&w, "completed", Some("failure"));
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.calls("fake-start"), "");
    let d = w.dispatch_state();
    assert_eq!(d["why"], "red");
    assert_eq!(d["hold"], "holding: main is red at 0123456789ab");
    has(&r.out, "holding: main is red at 0123456789ab");
    assert_eq!(check_calls(&w), 1);
    has(
        &w.gh(),
        "api repos/acme/widget/commits/main/check-runs?check_name=verify&filter=latest",
    );
    lacks(&w.gh(), "--add-label tehas:running");
}

#[test]
fn red_said_on_every_poll_while_a_runs_conclusion_says_nothing_at_the_cost_of_one_read() {
    let w = World::new("red-unknown-cost");
    red_instance(&w);
    w.ready(&[7, 8]);
    checks(&w, "completed", Some("cancelled"));
    w.poll();
    w.clear_calls();
    let r = w.poll();
    has(&r.out, &unknown_hold("verify"));
    assert_eq!(w.dispatch_state()["why"], "unknown-gate");
    // A poll held by an unknown conclusion is an idle poll: one read of the
    // check, no start. More is the signal to keep the answer by head commit.
    assert_eq!(check_calls(&w), 1);
    let cost = w.store_at(now(), &["poll-show"]);
    assert_eq!(jqr(".last.starts", &cost.out), "0");
}

#[test]
fn red_a_call_that_fails_hangs_or_answers_nonsense_starts_cycles_and_says_why() {
    let w = World::new("red-unreadable");
    for kind in ["fails", "hangs", "nonsense", "badsha"] {
        w.reset();
        red_instance(&w);
        match kind {
            "fails" => w.touch("checks-fails"),
            "hangs" => w.touch("checks-hangs"),
            "nonsense" => w.fx("checks.json", "not json\n"),
            _ => w.fx(
                "checks.json",
                r#"{"check_runs":[{"head_sha":"<b>","status":"completed","conclusion":"failure"}]}"#,
            ),
        }
        w.env("TEHAS_CHECK_WAIT", "1");
        let r = w.poll();
        assert_eq!(r.rc, 0, "{kind}: {}", r.out);
        assert_eq!(w.started(), "7 ", "{kind}");
        assert!(
            r.out
                .contains("the check verify on main could not be read: cycles start"),
            "{kind}: the log does not say why: {}",
            r.out
        );
    }
}

#[test]
fn red_no_check_named_github_is_not_asked() {
    let w = World::new("red-unnamed");
    w.slots(2, &[]);
    checks(&w, "completed", Some("failure"));
    w.poll();
    assert_eq!(w.started(), "7 ");
    assert_eq!(check_calls(&w), 0);
}

#[test]
fn red_a_planning_session_still_starts_and_an_answer_still_runs_a_turn() {
    let w = World::new("red-planning");
    w.to_plan();
    red_instance(&w);
    checks(&w, "completed", Some("failure"));
    w.poll();
    has(
        &w.plan_calls(),
        &format!("7 --out {}.out", w.record().display()),
    );
    w.planning();
    red_instance(&w);
    checks(&w, "completed", Some("failure"));
    w.comments(7, &plan_body(" ", "x", " ", 1, ""), "alice", json!([]));
    w.poll();
    has(
        &w.plan_calls(),
        &format!(
            "--session abc-123 --answer {}/answer.md",
            w.keep().display()
        ),
    );
}
