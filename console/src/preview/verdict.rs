//! What GitHub says about one pull request by itself, read as the sweep reads
//! it. A removal goes by this, so an answer that does not say is `Unclear`.

use serde_json::Value;
use std::process::{Command, Stdio};

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Pull {
    /// Open, its head in this repository, opened by one of ours.
    Keep,
    Closed,
    /// No such pull request.
    Gone,
    /// Open with a head that is not in this repository, a deleted fork included.
    Elsewhere,
    /// Open with a head here, and the answer names someone who is not ours.
    Outside,
    /// An answer that must not be acted on.
    Unclear,
}

/// Only "closed" is closed. An answer that does not say who opened it, or one
/// about another number, is unclear.
pub fn of_answer(answer: &Value, n: u64, repo: &str) -> Pull {
    let (Some(state), Some(number)) = (
        answer.get("state").and_then(Value::as_str),
        answer.get("number").filter(|v| v.is_number()),
    ) else {
        return Pull::Unclear;
    };
    if number.as_f64() != Some(n as f64) {
        return Pull::Unclear;
    }
    match state {
        "closed" => return Pull::Closed,
        "open" => {}
        _ => return Pull::Unclear,
    }
    let Some(head) = answer.get("head").filter(|head| head.is_object()) else {
        return Pull::Unclear;
    };
    let no_repo = matches!(head.get("repo"), None | Some(Value::Null));
    let full = match head.get("repo") {
        None | Some(Value::Null) => None,
        Some(Value::Object(repo)) => repo.get("full_name"),
        Some(_) => return Pull::Unclear,
    };
    let name = match full {
        Some(Value::String(name)) => name.as_str(),
        None | Some(Value::Null) | Some(Value::Bool(false)) => "",
        Some(_) => return Pull::Unclear,
    };
    if name.eq_ignore_ascii_case(repo) {
        return match answer.get("author_association").and_then(Value::as_str) {
            Some("OWNER" | "MEMBER" | "COLLABORATOR") => Pull::Keep,
            Some(_) => Pull::Outside,
            None => Pull::Unclear,
        };
    }
    if no_repo || matches!(full, Some(Value::String(_))) {
        Pull::Elsewhere
    } else {
        Pull::Unclear
    }
}

/// A call that worked is read; a failure is `Gone` only on GitHub's own 404.
/// The sweep checks that GitHub shows the repository at all before it reads a
/// 404 this way, and the run got an answer from repos/<repo>/pulls a moment ago.
fn of_call(worked: bool, stdout: &str, stderr: &str, n: u64, repo: &str) -> Pull {
    if !worked {
        return if stderr.contains("HTTP 404") {
            Pull::Gone
        } else {
            Pull::Unclear
        };
    }
    serde_json::from_str(stdout).map_or(Pull::Unclear, |answer| of_answer(&answer, n, repo))
}

pub fn ask(n: u64, repo: &str) -> Pull {
    let out = Command::new("gh")
        .args(["api", &format!("repos/{repo}/pulls/{n}")])
        .stdin(Stdio::null())
        .output();
    let Ok(out) = out else {
        return Pull::Unclear;
    };
    of_call(
        out.status.success(),
        &String::from_utf8_lossy(&out.stdout),
        &String::from_utf8_lossy(&out.stderr),
        n,
        repo,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const REPO: &str = "acme/widget";

    fn open() -> Value {
        json!({"number": 12, "state": "open", "author_association": "MEMBER",
               "head": {"repo": {"full_name": "Acme/Widget"}}})
    }

    fn with(key: &str, value: Value) -> Value {
        let mut answer = open();
        answer[key] = value;
        answer
    }

    fn read(answer: &Value) -> Pull {
        of_answer(answer, 12, REPO)
    }

    #[test]
    fn an_open_pull_request_of_ours_is_kept_in_any_capitals() {
        for who in ["OWNER", "MEMBER", "COLLABORATOR"] {
            assert_eq!(read(&with("author_association", json!(who))), Pull::Keep);
        }
    }

    #[test]
    fn only_closed_is_closed() {
        assert_eq!(read(&with("state", json!("closed"))), Pull::Closed);
        // Whatever else it says or does not: head and author are not asked.
        assert_eq!(
            read(&json!({"number": 12, "state": "closed"})),
            Pull::Closed
        );
        assert_eq!(read(&with("state", json!("weird"))), Pull::Unclear);
    }

    #[test]
    fn a_head_elsewhere_is_elsewhere_whoever_opened_it() {
        let fork = json!({"repo": {"full_name": "stranger/widget"}});
        assert_eq!(read(&with("head", fork)), Pull::Elsewhere);
        for head in [json!({"repo": null}), json!({"sha": "x"})] {
            assert_eq!(read(&with("head", head)), Pull::Elsewhere);
        }
        let mut quiet = with("head", json!({"repo": null}));
        quiet.as_object_mut().unwrap().remove("author_association");
        assert_eq!(read(&quiet), Pull::Elsewhere);
        let empty = json!({"repo": {"full_name": ""}});
        assert_eq!(read(&with("head", empty)), Pull::Elsewhere);
    }

    #[test]
    fn an_outsider_of_a_head_here_is_outside() {
        for who in ["NONE", "CONTRIBUTOR", "member", ""] {
            assert_eq!(read(&with("author_association", json!(who))), Pull::Outside);
        }
    }

    #[test]
    fn an_answer_that_does_not_say_is_unclear() {
        for answer in [
            json!({}),
            json!(null),
            json!([]),
            json!("open"),
            json!(12),
            json!({"number": 12}),
            json!({"state": "open"}),
            json!({"number": 51, "state": "closed"}),
            json!({"number": "12", "state": "closed"}),
            json!({"number": 12, "state": true}),
            json!({"number": 12, "state": "open"}),
            with("head", json!("main")),
            with("head", json!(null)),
            with("head", json!({"repo": {}})),
            with("head", json!({"repo": "acme/widget"})),
            with("head", json!({"repo": false})),
            with("head", json!({"repo": {"full_name": 12}})),
            with("head", json!({"repo": {"full_name": false}})),
            with("head", json!({"repo": {"full_name": true}})),
            with("author_association", json!(null)),
            with("author_association", json!(["OWNER"])),
            with("author_association", json!(true)),
        ] {
            assert_eq!(read(&answer), Pull::Unclear, "{answer}");
        }
    }

    #[test]
    fn a_number_is_compared_as_a_number() {
        assert_eq!(read(&with("number", json!(12.0))), Pull::Keep);
        assert_eq!(read(&with("number", json!(12.5))), Pull::Unclear);
    }

    #[test]
    fn a_call_is_gone_only_on_a_404() {
        let answer = open().to_string();
        assert_eq!(of_call(true, &answer, "", 12, REPO), Pull::Keep);
        assert_eq!(of_call(true, "", "", 12, REPO), Pull::Unclear);
        assert_eq!(of_call(true, "not json", "", 12, REPO), Pull::Unclear);
        assert_eq!(
            of_call(true, "{}\n{}\n", "", 12, REPO),
            Pull::Unclear,
            "two answers are not one"
        );
        for said in ["gh: Not Found (HTTP 404)", "HTTP 404\nmore\n"] {
            assert_eq!(of_call(false, "", said, 12, REPO), Pull::Gone);
        }
        for said in ["gh: rate limit (HTTP 403)", "connection refused", ""] {
            assert_eq!(of_call(false, "", said, 12, REPO), Pull::Unclear);
        }
    }
}
