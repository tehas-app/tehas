//! Which GitHub step of a cycle's end failed, and whether GitHub was at fault
//! (#1726): the record `tehas_core::end_fault` keeps, made from what the
//! cycle left and what its report could not post.
//!
//! The push and the pull request are pi-rukas's, in its `commit-pr` step: a
//! cycle that stopped there with no pull request, on words that name a push,
//! a pull request or GitHub, failed one of them. The handoff comment is the
//! host's: a handoff that reached no comment failed it. The words are read
//! only for the fixed list of `end_fault::words`, so nothing the agent wrote
//! goes on record. The result is the container's, so a push or a pull request
//! it names is on record as one the host did not see fail, and the poll
//! finishes nothing on it.

use serde_json::Value;
use tehas_core::end_fault::EndFault;

/// The step pi-rukas pushes and opens its pull request in.
const COMMIT_PR: &str = "commit-pr";

fn s<'a>(v: &'a Value, p: &str) -> &'a str {
    v.pointer(p).and_then(Value::as_str).unwrap_or("")
}

/// The issue's entry of a result, as the runner wrote it.
fn entry(result: &str, issue: u64) -> Value {
    crate::runner::values(result)
        .unwrap_or_default()
        .iter()
        .rev()
        .find_map(|r| {
            let list: Vec<&Value> = match &r["issues"] {
                Value::Array(a) => a.iter().collect(),
                Value::Object(o) => o.values().collect(),
                _ => return None,
            };
            list.into_iter()
                .find(|e| e["issue"].as_u64() == Some(issue))
                .cloned()
        })
        .unwrap_or(Value::Null)
}

/// What a cycle left: its result, its progress summary and whether it has a
/// pull request; and what the report's handoff comment failed with, when it
/// failed.
pub struct Left<'a> {
    pub issue: u64,
    pub attempt: u64,
    pub result: Option<&'a str>,
    pub progress: Option<&'a str>,
    pub pr: bool,
    pub handoff_failed: Option<&'a str>,
}

/// The record of a failed end, or None for an end with no failed GitHub step.
pub fn of(l: &Left, now: i64) -> Option<EndFault> {
    if !l.pr {
        if let Some((step, said)) = commit_pr(l) {
            return Some(EndFault::new(l.issue, l.attempt, step, &said, now));
        }
    }
    l.handoff_failed.map(|said| EndFault {
        host: true,
        ..EndFault::new(l.issue, l.attempt, "handoff", said, now)
    })
}

/// The push or the pull request of `commit-pr`, and what it said.
fn commit_pr(l: &Left) -> Option<(&'static str, String)> {
    let e = l.result.map_or(Value::Null, |r| entry(r, l.issue));
    let p: Value = l
        .progress
        .and_then(|t| serde_json::from_str(t).ok())
        .unwrap_or(Value::Null);
    let there = s(&e, "/failedStep") == COMMIT_PR
        || s(&e, "/failure/step") == COMMIT_PR
        || s(&e, "/cap") == format!("step-failed:{COMMIT_PR}")
        || s(&p, "/failure/step") == COMMIT_PR;
    if !there {
        return None;
    }
    let said: String = [
        s(&e, "/failure/message"),
        s(&e, "/failure/providerMessage"),
        s(&e, "/failure/cause"),
        s(&p, "/reason"),
        s(&p, "/failure/cause"),
    ]
    .iter()
    .filter(|t| !t.is_empty())
    .copied()
    .collect::<Vec<_>>()
    .join("\n");
    let low = said.to_ascii_lowercase();
    let pr = ["pull request", "pr create", "createpullrequest", "/pulls"]
        .iter()
        .any(|w| low.contains(w));
    let push = low.contains("push") || low.contains("returned error");
    if push {
        Some(("push", said))
    } else if pr {
        Some(("pr", said))
    } else if low.contains("github") || !tehas_core::end_fault::words(&said).is_empty() {
        // A failure of GitHub that names neither: the push comes first.
        Some(("push", said))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(failure: &str) -> String {
        format!(
            r#"{{"schemaVersion":5,"issues":[{{"issue":7,"status":"handoff","failedStep":"commit-pr","failure":{failure}}}]}}"#
        )
    }

    fn left<'a>(result: &'a str, pr: bool, handoff: Option<&'a str>) -> Left<'a> {
        Left {
            issue: 7,
            attempt: 2,
            result: Some(result),
            progress: None,
            pr,
            handoff_failed: handoff,
        }
    }

    #[test]
    fn a_push_that_failed_on_githubs_side_is_on_record_on_the_results_word() {
        let r = result(
            r#"{"kind":"tooling","step":"commit-pr","message":"git push failed: The requested URL returned error: 502"}"#,
        );
        let f = of(&left(&r, false, None), 100).unwrap();
        assert_eq!(
            (f.step.as_str(), f.github, f.why.as_str(), f.at, f.attempt),
            ("push", true, "returned error: 502", 100, 2)
        );
        // The host did not see it fail, so the poll does not finish it.
        assert!(!f.host);
        assert!(matches!(f.due(101), tehas_core::end_fault::Due::Never(_)));
    }

    #[test]
    fn a_pull_request_refused_is_the_requests_own() {
        let r = result(
            r#"{"kind":"tooling","message":"gh pr create: HTTP 422: Validation Failed (a pull request already exists)"}"#,
        );
        let f = of(&left(&r, false, None), 100).unwrap();
        assert_eq!((f.step.as_str(), f.github), ("pr", false));
        let r = result(r#"{"message":"gh pr create: HTTP 429: Too Many Requests"}"#);
        let f = of(&left(&r, false, None), 100).unwrap();
        assert_eq!((f.step.as_str(), f.github), ("pr", true));
    }

    #[test]
    fn a_step_that_is_not_githubs_is_no_record() {
        let r = result(r#"{"message":"cargo test failed in other.rs"}"#);
        assert_eq!(of(&left(&r, false, None), 100), None);
        let r = r#"{"schemaVersion":5,"issues":[{"issue":7,"failedStep":"develop","failure":{"message":"HTTP 502"}}]}"#;
        assert_eq!(of(&left(r, false, None), 100), None);
    }

    #[test]
    fn a_cycle_with_its_pull_request_failed_only_what_came_after() {
        let r = result(r#"{"message":"git push: HTTP 502"}"#);
        assert_eq!(of(&left(&r, true, None), 100), None);
        let f = of(&left(&r, true, Some(": HTTP 503")), 100).unwrap();
        assert_eq!((f.step.as_str(), f.github, f.host), ("handoff", true, true));
    }

    #[test]
    fn the_progress_names_the_step_when_the_result_does_not() {
        let l = Left {
            issue: 7,
            attempt: 1,
            result: None,
            progress: Some(
                r#"{"reason":"push rejected: HTTP 503","failure":{"step":"commit-pr"}}"#,
            ),
            pr: false,
            handoff_failed: None,
        };
        let f = of(&l, 5).unwrap();
        assert_eq!((f.step.as_str(), f.github), ("push", true));
    }
}
