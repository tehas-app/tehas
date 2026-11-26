//! `CodeHost::branch_checks` as the adapter answers it: one GraphQL
//! question for the branch's head and its checks of a name, folded as
//! `checks` folds them, and the long way (`branch`, then `checks`) where
//! the one answer may not be whole or GitHub answers it with errors.
//!
//! The live answers are kit run #1's step f (2026-11-26), on
//! tehas-app/tehas-rec-branch-checks: the one question
//! (`recorded/branch-checks/live/`) and the long way's REST reads
//! (`recorded/code-host/tehas-rec-branch-checks/`). The other cases are
//! the live answer with its suites and statuses set in the test. The App
//! without `actions: read` (step f4) was not recorded in run #1: its case
//! is hand-made, pending.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tehas_core::forge::{Failed, GhRunner};
use tehas_github::GitHub;
use tehas_ports::laws;
use tehas_ports::{
    Branch, CheckStatus, CodeHost, CommitId, Conclusion, Outcome, Overall, Pool, Unread,
};

type Asked = Arc<Mutex<Vec<String>>>;

/// A runner that answers each call in turn, writing each down: `Ok` is
/// gh's output, `Err((why, answer))` a failure and what gh printed. A call
/// past them fails.
struct Script {
    answers: Vec<Result<String, (String, Option<String>)>>,
    asked: Asked,
}

impl GhRunner for Script {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        self.run_answer(args).map_err(|f| f.why)
    }

    fn run_answer(&mut self, args: &[&str]) -> Result<String, Failed> {
        let mut asked = self.asked.lock().unwrap();
        asked.push(args.join(" "));
        match self.answers.get(asked.len() - 1).cloned() {
            Some(Ok(out)) => Ok(out),
            Some(Err((why, answer))) => Err(Failed { why, answer }),
            None => Err(Failed {
                why: "no answer".into(),
                answer: None,
            }),
        }
    }
}

fn scripted(answers: Vec<Result<String, (String, Option<String>)>>) -> (GitHub, Asked) {
    let asked = Asked::default();
    let runner = Script {
        answers,
        asked: Arc::clone(&asked),
    };
    (
        GitHub::new("tehas-app/tehas-rec-branch-checks", Box::new(runner)),
        asked,
    )
}

/// The head of `main` in the live recordings.
const SHA: &str = "a09d79baac4a6fa80120c3dc5e599b4e4eba40ad";

/// A recording under `tests/recorded/`.
fn recorded(path: &str) -> String {
    let at = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/recorded")
        .join(path);
    std::fs::read_to_string(&at).unwrap_or_else(|e| panic!("{}: {e}", at.display()))
}

/// The live answer of the one question for `main`.
fn live() -> String {
    recorded("branch-checks/live/question-main.json")
}

/// A live REST read of the long way, of the head `SHA`.
fn rest(read: &str) -> String {
    recorded(&format!("code-host/tehas-rec-branch-checks/{read}"))
}

fn main() -> Branch {
    Branch::new("main").unwrap()
}

/// A run of the question: `(id, name, status, conclusion, started)`.
type R<'a> = (u64, &'a str, &'a str, Option<&'a str>, Option<&'a str>);

/// A suite of the question: its id, whether GitHub Actions made it, its
/// workflow and event, and its runs.
fn suite(id: u64, actions: bool, workflow: Option<(u64, &str)>, runs: &[R]) -> String {
    let app = if actions {
        json!({"databaseId": 15368, "slug": "github-actions"})
    } else {
        json!({"databaseId": 9, "slug": "other-ci"})
    };
    let nodes: Vec<Value> = runs
        .iter()
        .map(|(id, name, status, conclusion, started)| {
            json!({"databaseId": id, "name": name, "status": status,
                "conclusion": conclusion, "startedAt": started})
        })
        .collect();
    json!({"databaseId": id, "app": app,
        "workflowRun": workflow.map(|(w, e)| json!({"event": e, "workflow": {"databaseId": w}})),
        "checkRuns": {"totalCount": nodes.len(), "nodes": nodes}})
    .to_string()
}

/// The question's answer: the live answer for `main` with these suites and
/// status contexts in place of its own.
fn answer(suites: &[String], contexts: &[(&str, &str)]) -> String {
    let suites: Vec<Value> = suites
        .iter()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    let contexts: Vec<Value> = contexts
        .iter()
        .map(|(c, s)| json!({"context": c, "state": s}))
        .collect();
    let mut v: Value = serde_json::from_str(&live()).unwrap();
    let target = &mut v["data"]["repository"]["ref"]["target"];
    target["checkSuites"] = json!({"totalCount": suites.len(), "nodes": suites});
    target["status"] = json!({"contexts": contexts});
    v.to_string()
}

/// The long way's answers: the branch's head (the live answer for `main`,
/// which holds every field the branch question asks), then one page of
/// the runs and one of the statuses.
fn long_way(runs: &[String]) -> Vec<Result<String, (String, Option<String>)>> {
    let runs: Vec<Value> = runs
        .iter()
        .map(|r| serde_json::from_str(r).unwrap())
        .collect();
    vec![
        Ok(live()),
        Ok(json!({"total_count": runs.len(), "check_runs": runs}).to_string()),
        Ok(json!({"total_count": 0, "statuses": []}).to_string()),
    ]
}

fn done(c: Conclusion) -> CheckStatus {
    CheckStatus::Done(c)
}

#[test]
fn the_head_and_its_checks_are_one_question() {
    let (mut gh, asked) = scripted(vec![Ok(answer(
        &[suite(
            1,
            true,
            Some((5, "push")),
            &[(11, "verify", "COMPLETED", Some("SUCCESS"), None)],
        )],
        &[],
    ))]);
    let (head, checks) = gh.branch_checks(&main(), Some("verify")).unwrap().unwrap();
    assert_eq!(head, CommitId::new(SHA).unwrap());
    assert_eq!(checks.overall, Overall::Pass);
    let asked = asked.lock().unwrap();
    assert_eq!(asked.len(), 1);
    assert!(
        asked[0].starts_with("api graphql -F owner=tehas-app -F name=tehas-rec-branch-checks -f ref=refs/heads/main -f check=verify -f query=query("),
        "{}",
        asked[0]
    );
}

/// Two workflows' jobs of one name are both held: the failed one is not
/// hidden by the other's pass (Decided 129).
#[test]
fn two_workflows_jobs_of_one_name_are_both_held() {
    let (mut gh, _) = scripted(vec![Ok(answer(
        &[
            suite(
                1,
                true,
                Some((5, "push")),
                &[(11, "verify", "COMPLETED", Some("FAILURE"), None)],
            ),
            suite(
                2,
                true,
                Some((6, "push")),
                &[(12, "verify", "COMPLETED", Some("SUCCESS"), None)],
            ),
        ],
        &[],
    ))]);
    let (_, checks) = gh.branch_checks(&main(), Some("verify")).unwrap().unwrap();
    assert_eq!(checks.runs.len(), 2);
    assert_eq!(checks.overall, Overall::Fail);
}

/// A newer run of one workflow and event, a re-run in a suite of its own,
/// supersedes the older failure; a run of another event stays apart
/// (amended 2026-11-25).
#[test]
fn a_newer_run_of_one_workflow_and_event_supersedes_and_another_event_stays() {
    let old = (
        11,
        "verify",
        "COMPLETED",
        Some("FAILURE"),
        Some("2026-11-25T10:00:00Z"),
    );
    let new = (
        12,
        "verify",
        "COMPLETED",
        Some("SUCCESS"),
        Some("2026-11-25T11:00:00Z"),
    );
    let (mut gh, _) = scripted(vec![Ok(answer(
        &[
            suite(1, true, Some((5, "push")), &[old]),
            suite(2, true, Some((5, "push")), &[new]),
        ],
        &[],
    ))]);
    let (_, checks) = gh.branch_checks(&main(), Some("verify")).unwrap().unwrap();
    assert_eq!(checks.runs.len(), 1);
    assert_eq!(checks.overall, Overall::Pass);
    let (mut gh, _) = scripted(vec![Ok(answer(
        &[
            suite(1, true, Some((5, "pull_request")), &[old]),
            suite(2, true, Some((5, "push")), &[new]),
        ],
        &[],
    ))]);
    let (_, checks) = gh.branch_checks(&main(), Some("verify")).unwrap().unwrap();
    assert_eq!(checks.overall, Overall::Fail);
}

/// The site's words fold as REST's do: a run not done is pending, a
/// cancel is no pass, and a status of the name fails beside a passing run
/// while one of another name is not held.
#[test]
fn the_words_fold_as_the_checks_read_folds_them() {
    let cases = [
        ("IN_PROGRESS", None, Overall::Pending),
        ("QUEUED", None, Overall::Pending),
        ("COMPLETED", Some("NEUTRAL"), Overall::Pass),
        ("COMPLETED", Some("SKIPPED"), Overall::Pass),
        ("COMPLETED", Some("TIMED_OUT"), Overall::Fail),
        ("COMPLETED", Some("STARTUP_FAILURE"), Overall::Fail),
        ("COMPLETED", Some("CANCELLED"), Overall::Unknown),
        ("COMPLETED", Some("STALE"), Overall::Unknown),
    ];
    for (status, conclusion, overall) in cases {
        let (mut gh, _) = scripted(vec![Ok(answer(
            &[suite(
                1,
                false,
                None,
                &[(11, "verify", status, conclusion, None)],
            )],
            &[],
        ))]);
        let (_, checks) = gh.branch_checks(&main(), Some("verify")).unwrap().unwrap();
        assert_eq!(checks.overall, overall, "{status} {conclusion:?}");
    }
    let pass = (11, "verify", "COMPLETED", Some("SUCCESS"), None);
    let (mut gh, _) = scripted(vec![Ok(answer(
        &[suite(1, false, None, &[pass])],
        &[("verify", "ERROR"), ("other", "FAILURE")],
    ))]);
    let (_, checks) = gh.branch_checks(&main(), Some("verify")).unwrap().unwrap();
    assert_eq!(
        checks.runs.iter().map(|r| r.status).collect::<Vec<_>>(),
        [done(Conclusion::Passed), done(Conclusion::Failed)]
    );
    assert_eq!(checks.overall, Overall::Fail);
}

#[test]
fn no_such_branch_is_none_in_one_question() {
    let none = recorded("branch-checks/live/question-no-such-branch-here.json");
    let (mut gh, asked) = scripted(vec![Ok(none)]);
    assert_eq!(gh.branch_checks(&main(), Some("verify")), Ok(None));
    assert_eq!(asked.lock().unwrap().len(), 1);
}

/// An answer GitHub gives with errors, as where the App may not read the
/// workflow runs, is asked the long way, whose outcome it gives.
#[test]
fn an_answer_with_errors_is_asked_the_long_way() {
    let refused = json!({"data": null, "errors": [{"type": "FORBIDDEN"}]}).to_string();
    let run = json!({"id": 11, "name": "verify", "status": "completed", "conclusion": "failure",
        "app": {"id": 9, "slug": "other-ci"}, "check_suite": {"id": 1}})
    .to_string();
    for first in [
        Ok(refused.clone()),
        Err(("gh exited 1".to_string(), Some(refused.clone()))),
    ] {
        let mut answers = vec![first];
        answers.extend(long_way(std::slice::from_ref(&run)));
        let (mut gh, asked) = scripted(answers);
        let (_, checks) = gh.branch_checks(&main(), Some("verify")).unwrap().unwrap();
        assert_eq!(checks.overall, Overall::Fail);
        let asked = asked.lock().unwrap();
        assert_eq!(asked.len(), 4, "{asked:?}");
        assert!(asked[2].contains("/check-runs?"), "{asked:?}");
    }
}

/// A list the one question may not give whole (a count past the nodes, or
/// a page of 100) is asked the long way, never folded from a part.
#[test]
fn a_list_that_may_not_be_whole_is_asked_the_long_way() {
    let pass = (11, "verify", "COMPLETED", Some("SUCCESS"), None);
    let mut short: Value =
        serde_json::from_str(&answer(&[suite(1, false, None, &[pass])], &[])).unwrap();
    short["data"]["repository"]["ref"]["target"]["checkSuites"]["totalCount"] = json!(2);
    let mut runs_short = short.clone();
    runs_short["data"]["repository"]["ref"]["target"]["checkSuites"]["totalCount"] = json!(1);
    runs_short["data"]["repository"]["ref"]["target"]["checkSuites"]["nodes"][0]["checkRuns"]
        ["totalCount"] = json!(2);
    let contexts: Vec<(String, &str)> = (0..100).map(|i| (format!("c{i}"), "SUCCESS")).collect();
    let contexts: Vec<(&str, &str)> = contexts.iter().map(|(c, s)| (c.as_str(), *s)).collect();
    let many = answer(&[suite(1, false, None, &[pass])], &contexts);
    for first in [short.to_string(), runs_short.to_string(), many] {
        let mut answers = vec![Ok(first)];
        answers.extend(long_way(&[]));
        let (mut gh, asked) = scripted(answers);
        let (_, checks) = gh.branch_checks(&main(), Some("verify")).unwrap().unwrap();
        assert_eq!(checks.overall, Overall::None);
        assert_eq!(asked.lock().unwrap().len(), 4);
    }
}

#[test]
fn a_rate_limited_question_is_the_priced_budget_spent_and_asks_no_more() {
    let limited = json!({"data": null, "errors": [{"type": "RATE_LIMITED"}]}).to_string();
    for first in [
        Ok(limited.clone()),
        Err(("gh exited 1".to_string(), Some(limited.clone()))),
    ] {
        let (mut gh, asked) = scripted(vec![first]);
        let got = gh.branch_checks(&main(), Some("verify"));
        assert!(
            matches!(
                got,
                Err(Outcome::Spent {
                    pool: Pool::Priced,
                    ..
                })
            ),
            "{got:?}"
        );
        assert_eq!(asked.lock().unwrap().len(), 1);
    }
}

#[test]
fn a_run_or_suite_missing_a_field_every_one_has_is_malformed() {
    let pass = (11, "verify", "COMPLETED", Some("SUCCESS"), None);
    let good: Value =
        serde_json::from_str(&answer(&[suite(1, true, Some((5, "push")), &[pass])], &[])).unwrap();
    for path in [
        "/databaseId",
        "/app",
        "/workflowRun/event",
        "/checkRuns/nodes/0/databaseId",
        "/checkRuns/nodes/0/status",
    ] {
        let mut v = good.clone();
        let (parent, key) = path.rsplit_once('/').unwrap();
        v["data"]["repository"]["ref"]["target"]["checkSuites"]["nodes"][0]
            .pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(key);
        let (mut gh, _) = scripted(vec![Ok(v.to_string())]);
        assert_eq!(
            gh.branch_checks(&main(), Some("verify")),
            Err(Outcome::Unread(Unread::Malformed)),
            "{path}"
        );
    }
}

/// Asked for every check, the long way: the question asks by name.
#[test]
fn asked_for_every_check_it_is_the_long_way() {
    let (mut gh, asked) = scripted(long_way(&[]));
    let (_, checks) = gh.branch_checks(&main(), None).unwrap().unwrap();
    assert_eq!(checks.overall, Overall::None);
    let asked = asked.lock().unwrap();
    assert_eq!(asked.len(), 3);
    assert!(
        !asked.iter().any(|a| a.contains("checkSuites")),
        "{asked:?}"
    );
}

/// The query text the one question asks, from the call written down.
fn query_of(call: &str) -> &str {
    call.split_once(" -f query=").map(|(_, q)| q).unwrap()
}

/// The arguments of `field(...)` in a query text.
fn args_of<'q>(query: &'q str, field: &str) -> &'q str {
    let at = query.find(&format!("{field}(")).unwrap() + field.len() + 1;
    &query[at..at + query[at..].find(')').unwrap()]
}

/// GitHub's `checkName` on `checkSuites` filters by the suite's name, and a
/// workflow's suite is named for the workflow, not its jobs: live, that
/// filter answered no suites and no error, so a failed check read as
/// missing and held nothing. The suites are asked unfiltered; the runs by
/// their name, of every type.
#[test]
fn the_question_filters_the_runs_by_the_name_and_never_the_suites() {
    let (mut gh, asked) = scripted(vec![Ok(answer(&[], &[]))]);
    gh.branch_checks(&main(), Some("verify")).unwrap();
    let asked = asked.lock().unwrap();
    let query = query_of(&asked[0]);
    let suites = args_of(query, "checkSuites");
    assert!(!suites.contains("checkName"), "{suites}");
    assert!(!suites.contains("filterBy"), "{suites}");
    let runs = args_of(query, "checkRuns");
    assert!(runs.contains("checkName: $check"), "{runs}");
    assert!(runs.contains("checkType: ALL"), "{runs}");
}

/// A suite of GitHub Actions holding a run of the name, whose workflow run
/// the answer does not give (null, as GitHub may answer an App without
/// `actions: read` without an error): the workflow is not known, so the
/// answer is not folded. It is asked the long way, whose read of the
/// workflow runs, refused, makes the check `Forbidden` and says why; and
/// where the long way reads them, the older failure of one workflow stays
/// beside the newer pass of another. Hand-made, pending: kit step f4
/// (`branch-checks/live/no-actions-read/`) was skipped in run #1.
#[test]
fn a_suite_of_actions_without_its_workflow_run_is_asked_the_long_way() {
    let old = (
        11,
        "verify",
        "COMPLETED",
        Some("FAILURE"),
        Some("2026-11-25T10:00:00Z"),
    );
    let new = (
        12,
        "verify",
        "COMPLETED",
        Some("SUCCESS"),
        Some("2026-11-25T11:00:00Z"),
    );
    let question = answer(
        &[
            suite(1, true, Some((5, "push")), &[old]),
            suite(2, true, None, &[new]),
        ],
        &[],
    );
    let rest = |id: u64, suite: u64, conclusion: &str, started: &str| {
        json!({"id": id, "name": "verify", "status": "completed", "conclusion": conclusion,
            "started_at": started, "app": {"id": 15368, "slug": "github-actions"},
            "check_suite": {"id": suite}})
        .to_string()
    };
    let runs = [
        rest(11, 1, "failure", "2026-11-25T10:00:00Z"),
        rest(12, 2, "success", "2026-11-25T11:00:00Z"),
    ];
    let mut answers = vec![Ok(question.clone())];
    answers.extend(long_way(&runs));
    answers.push(Err((
        "gh: Resource not accessible by integration (HTTP 403)".to_string(),
        None,
    )));
    let (mut gh, asked) = scripted(answers);
    assert_eq!(
        gh.branch_checks(&main(), Some("verify")),
        Err(Outcome::Forbidden)
    );
    assert_eq!(gh.take_lines(), [tehas_github::WORKFLOW_RUNS_FORBIDDEN]);
    assert_eq!(asked.lock().unwrap().len(), 5);

    let mut answers = vec![Ok(question)];
    answers.extend(long_way(&runs));
    answers.push(Ok(json!({"total_count": 2, "workflow_runs": [
        {"id": 101, "check_suite_id": 1, "workflow_id": 5, "event": "push"},
        {"id": 102, "check_suite_id": 2, "workflow_id": 6, "event": "push"}]})
    .to_string()));
    let (mut gh, _) = scripted(answers);
    let (_, checks) = gh.branch_checks(&main(), Some("verify")).unwrap().unwrap();
    assert_eq!(checks.runs.len(), 2);
    assert_eq!(checks.overall, Overall::Fail);
}

/// A suite of GitHub Actions without a run of the name needs no workflow:
/// the answer is whole without it.
#[test]
fn a_suite_without_a_run_of_the_name_needs_no_workflow_run() {
    let pass = (11, "verify", "COMPLETED", Some("SUCCESS"), None);
    let (mut gh, asked) = scripted(vec![Ok(answer(
        &[
            suite(1, true, Some((5, "push")), &[pass]),
            suite(2, true, None, &[]),
        ],
        &[],
    ))]);
    let (_, checks) = gh.branch_checks(&main(), Some("verify")).unwrap().unwrap();
    assert_eq!(checks.overall, Overall::Pass);
    assert_eq!(asked.lock().unwrap().len(), 1);
}

/// The law of the port on the live answers: `main` at `SHA`, whose
/// workflow Gate's job `verify` failed beside workflow Lint's suite with no
/// run of the name, and whose statuses are `verify` success and `other`
/// failure; and a missing branch as GitHub answered the one question live.
#[test]
fn the_law_holds_on_the_live_answers() {
    let answers = vec![
        Ok(live()),
        Ok(rest(&format!("checks-{SHA}-1-name=verify.json"))),
        Ok(rest(&format!("statuses-{SHA}-1.json"))),
        Ok(rest(&format!("runs-{SHA}-1.json"))),
        Ok(live()),
        Ok(recorded(
            "branch-checks/live/question-no-such-branch-here.json",
        )),
    ];
    let (mut gh, asked) = scripted(answers);
    assert_eq!(
        laws::code_host::branch_checks_are_the_branch_then_its_checks(
            &mut gh,
            &main(),
            &Branch::new("no-such-branch-here").unwrap(),
            "verify",
        ),
        Ok(())
    );
    assert_eq!(asked.lock().unwrap().len(), 6);
}

/// The live answer folds in one call: run 139972228367 of `verify` failed
/// and the status `verify` passed, so the check fails; `other` is not held.
#[test]
fn the_live_answer_is_the_failed_run_and_the_passed_status() {
    let (mut gh, asked) = scripted(vec![Ok(live())]);
    let (head, checks) = gh.branch_checks(&main(), Some("verify")).unwrap().unwrap();
    assert_eq!(head, CommitId::new(SHA).unwrap());
    assert_eq!(
        checks.runs.iter().map(|r| r.status).collect::<Vec<_>>(),
        [done(Conclusion::Failed), done(Conclusion::Passed)]
    );
    assert_eq!(checks.overall, Overall::Fail);
    assert_eq!(asked.lock().unwrap().len(), 1);
}

/// The first question filtered the suites by `checkName`. Asked live of the
/// same commit, it answered no suites and no error: the failed run of
/// `verify` was gone, and the passed status alone read as a pass. The
/// question asked now holds the failure.
#[test]
fn the_suites_filtered_by_the_check_name_answer_none_live() {
    let first = recorded("branch-checks/live/first-question-main.json");
    let v: Value = serde_json::from_str(&first).unwrap();
    let target = &v["data"]["repository"]["ref"]["target"];
    assert_eq!(target["oid"], SHA);
    assert_eq!(target["checkSuites"]["totalCount"], 0);
    let (mut gh, _) = scripted(vec![Ok(first)]);
    let (_, checks) = gh.branch_checks(&main(), Some("verify")).unwrap().unwrap();
    assert_eq!(checks.overall, Overall::Pass);
    let (mut gh, _) = scripted(vec![Ok(live())]);
    let (_, checks) = gh.branch_checks(&main(), Some("verify")).unwrap().unwrap();
    assert_eq!(checks.overall, Overall::Fail);
}
