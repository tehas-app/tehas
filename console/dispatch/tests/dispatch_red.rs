//! No new cycle while the check TEHAS_BRANCH_CHECK names failed on the head
//! commit of the branch: what the poll asks GitHub and what it runs. The
//! decisions are fixtures in tests/fixtures/red.json.

mod common;

use common::{has, lacks, plan_body, synthetic, World};
use serde_json::{json, Value};
use std::process::Command;

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

/// The one run of the check on the head: in the one question, and as
/// GitHub lists every run for the long way.
fn checks(w: &World, status: &str, conclusion: Option<&str>) {
    question(w, &[(1, 1, status, conclusion)]);
    w.fx_json(
        "checks.json",
        &json!({"total_count": 1, "check_runs": [run(1, 1, status, conclusion)]}),
    );
}

/// The one question's answer, `tests/synthetic/branch-checks/verify-passed.json`
/// with its one suite given for each `(suite, workflow, status, conclusion)`:
/// the suite and its workflow by these ids, its run of `verify` by the
/// suite's id and in this state.
fn question(w: &World, suites: &[(u64, u64, &str, Option<&str>)]) {
    w.fx_json("checks-question.json", &answer(suites));
}

fn answer(suites: &[(u64, u64, &str, Option<&str>)]) -> Value {
    let text = std::fs::read_to_string(synthetic("branch-checks/verify-passed.json")).unwrap();
    let mut v: Value = serde_json::from_str(&text).unwrap();
    let all = &mut v["data"]["repository"]["ref"]["target"]["checkSuites"];
    let one = all["nodes"][0].clone();
    let nodes: Vec<Value> = suites
        .iter()
        .map(|&(suite, workflow, status, conclusion)| {
            let mut s = one.clone();
            s["databaseId"] = json!(suite);
            s["workflowRun"]["workflow"]["databaseId"] = json!(workflow);
            let r = &mut s["checkRuns"]["nodes"][0];
            r["databaseId"] = json!(suite);
            r["status"] = json!(status.to_ascii_uppercase());
            r["conclusion"] = json!(conclusion.map(str::to_ascii_uppercase));
            s
        })
        .collect();
    *all = json!({"totalCount": nodes.len(), "nodes": nodes});
    v
}

/// A run of `verify` on the head, by GitHub Actions in check suite `suite`.
fn run(id: u64, suite: u64, status: &str, conclusion: Option<&str>) -> serde_json::Value {
    json!({"id": id, "name": "verify", "head_sha": SHA, "status": status,
        "conclusion": conclusion, "app": {"id": 15368, "slug": "github-actions"}, "check_suite": {"id": suite}})
}

/// The read of the check, as the code host adapter asks it: one question
/// for the branch's head and every run of the name on it.
const QUESTION: &str =
    "-F owner=acme -F name=widget -f ref=refs/heads/main -f check=verify -f query=query(";

fn red_instance(w: &World) {
    w.slots(2, &["TEHAS_BRANCH_CHECK=verify"]);
}

/// The calls that read the check: the one question, and the REST reads of
/// the long way.
fn check_calls(w: &World) -> usize {
    w.gh()
        .lines()
        .filter(|l| l.contains("checkSuites") || l.contains("/check-runs?"))
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
    has(&w.gh(), QUESTION);
    lacks(&w.gh(), "--add-label tehas:running");
}

/// Two workflows' jobs of one name on the head: the failed one is not hidden
/// behind the newer pass of the other, and the poll holds (Decided 129).
#[test]
fn red_two_workflows_jobs_of_one_name_are_both_read_and_the_failed_one_holds() {
    let w = World::new("red-two-workflows");
    red_instance(&w);
    w.ready(&[7, 8]);
    question(
        &w,
        &[
            (20, 2, "completed", Some("success")),
            (10, 1, "completed", Some("failure")),
        ],
    );
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.calls("fake-start"), "");
    assert_eq!(w.dispatch_state()["why"], "red");
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
    // A poll held by an unknown conclusion is an idle poll: one question for
    // the branch's head, the check's runs, their workflows and the commit's
    // statuses, no start. More is the signal to keep the answer by head commit.
    assert_eq!(check_calls(&w), 1);
    let gh = w.gh();
    assert_eq!(gh.lines().filter(|l| l.contains(QUESTION)).count(), 1);
    assert_eq!(
        gh.lines().filter(|l| l.contains("qualifiedName")).count(),
        1
    );
    for long in ["/check-runs?", "/status?", "/actions/runs?"] {
        lacks(&gh, long);
    }
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
            "nonsense" => w.fx("checks-question.json", "not json\n"),
            // A failed run with no id: no answer to fold.
            _ => {
                let mut v = answer(&[(1, 1, "completed", Some("failure"))]);
                v["data"]["repository"]["ref"]["target"]["checkSuites"]["nodes"][0]["checkRuns"]
                    ["nodes"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("databaseId");
                w.fx_json("checks-question.json", &v);
            }
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
        // A read that failed but was not refused keeps its own line alone.
        lacks(&r.out, "the code host refused");
    }
}

/// The code host refusing the read (the site's own words, in
/// tests/common/refused.txt), of the workflow runs as for an App without
/// actions: read, or of the check runs: the check is unreadable, never the
/// pass the runs would read as, and the poll says once, of the branch, that
/// the factory's account may lack a permission and where the setup page
/// says which.
#[test]
fn red_a_refused_read_is_unreadable_and_says_the_permission_once() {
    let w = World::new("red-refused");
    let line = "the code host refused to say the checks of main; the factory's account may lack a permission (host/docs/github-app.md#the-permission-to-read-the-checks)";
    for refused in ["runs-refused", "checks-refused"] {
        w.reset();
        red_instance(&w);
        w.ready(&[7, 8]);
        checks(&w, "completed", Some("success"));
        w.fx(refused, include_str!("common/refused.txt"));
        let r = w.poll();
        assert_eq!(r.rc, 0, "{refused}: {}", r.out);
        assert_eq!(r.out.matches(line).count(), 1, "{refused}: {}", r.out);
        has(
            &r.out,
            "the check verify on main could not be read: cycles start",
        );
        assert_ne!(w.dispatch_state()["why"], "red");
        // The next poll says it again, once: once for each poll and branch.
        let again = w.poll();
        assert_eq!(
            again.out.matches(line).count(),
            1,
            "{refused}: {}",
            again.out
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
