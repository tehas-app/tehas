//! `CodeHost::branch_checks` as the adapter answers it: one GraphQL
//! question for the branch's head and its checks of a name, folded as
//! `checks` folds them, and the long way (`branch`, then `checks`) where
//! the one answer may not be whole or GitHub answers it with errors.

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
    (GitHub::new("acme/widget", Box::new(runner)), asked)
}

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn main() -> Branch {
    Branch::new("main").unwrap()
}

/// A run of the question: `(id, name, status, conclusion, started)`.
type R<'a> = (u64, &'a str, &'a str, Option<&'a str>, Option<&'a str>);

/// A suite of the question: its id, whether GitHub Actions made it, its
/// workflow and event, and its runs.
fn suite(id: u64, actions: bool, workflow: Option<(u64, &str)>, runs: &[R]) -> Value {
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
}

/// The question's answer: the branch `main` at `SHA` with these suites and
/// status contexts.
fn answer(suites: &[Value], contexts: &[(&str, &str)]) -> String {
    let contexts: Vec<Value> = contexts
        .iter()
        .map(|(c, s)| json!({"context": c, "state": s}))
        .collect();
    json!({"data": {"repository": {"ref": {"name": "main", "prefix": "refs/heads/",
        "target": {"__typename": "Commit", "oid": SHA,
            "checkSuites": {"totalCount": suites.len(), "nodes": suites},
            "status": {"contexts": contexts}}}}}})
    .to_string()
}

/// The long way's answers: the branch's head, then one page of the runs
/// and one of the statuses.
fn long_way(runs: &[Value]) -> Vec<Result<String, (String, Option<String>)>> {
    vec![
        Ok(
            json!({"data": {"repository": {"ref": {"name": "main", "prefix": "refs/heads/",
            "target": {"__typename": "Commit", "oid": SHA}}}}})
            .to_string(),
        ),
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
        asked[0].starts_with("api graphql -F owner=acme -F name=widget -f ref=refs/heads/main -f check=verify -f query=query("),
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
    let none = json!({"data": {"repository": {"ref": null}}}).to_string();
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
        "app": {"id": 9, "slug": "other-ci"}, "check_suite": {"id": 1}});
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

/// The law of the port: the one question answers what `branch` then
/// `checks` answer.
#[test]
fn the_one_question_keeps_the_law_of_the_branch_then_its_checks() {
    let rest = json!({"id": 11, "name": "verify", "status": "completed", "conclusion": "failure",
        "app": {"id": 9, "slug": "other-ci"}, "check_suite": {"id": 1}});
    let mut answers = long_way(&[rest]);
    answers.push(Ok(answer(
        &[suite(
            1,
            false,
            None,
            &[(11, "verify", "COMPLETED", Some("FAILURE"), None)],
        )],
        &[],
    )));
    answers.push(Ok(
        json!({"data": {"repository": {"ref": null}}}).to_string()
    ));
    let (mut gh, asked) = scripted(answers);
    assert_eq!(
        laws::code_host::branch_checks_are_the_branch_then_its_checks(
            &mut gh,
            &main(),
            &Branch::new("gone").unwrap(),
            "verify",
        ),
        Ok(())
    );
    assert_eq!(asked.lock().unwrap().len(), 5);
}
