//! The fixtures of before 2.0 answered a question by ticking one of its
//! boxes. A box no longer answers (#2782): a reply from the same account
//! naming the same choice does. `as_reply` turns a body with one box ticked
//! into the same body with none, and the reply that makes that choice, at the
//! time the tick was saved; a body with more or fewer ticks stays as it is.

use serde_json::{json, Value};

/// The reply's comment id: after the question's 4001.
pub const REPLY_ID: u64 = 4002;

/// The body with its one tick taken off, and the reply of `by` that names
/// the choice it ticked, when exactly one box was ticked and `by` is someone.
pub fn as_reply(body: &str, by: &str, by_id: Option<u64>) -> (String, Option<Value>) {
    let boxes: Vec<&str> = body
        .lines()
        .filter(|l| l.starts_with("- [") && matches!(l.get(3..6), Some(" ] " | "x] " | "X] ")))
        .collect();
    let ticked: Vec<usize> = boxes
        .iter()
        .enumerate()
        .filter(|(_, l)| matches!(l.get(3..4), Some("x" | "X")))
        .map(|(i, _)| i)
        .collect();
    if ticked.len() != 1 || by.is_empty() {
        return (body.to_string(), None);
    }
    let i = ticked[0];
    let label = &boxes[i][6..];
    let words = if body.contains("kind=plan") {
        (i + 1).to_string()
    } else {
        match tehas_core::question::retry_of(label) {
            Some(true) => "retry".to_string(),
            Some(false) => "retry-text".to_string(),
            None => "park".to_string(),
        }
    };
    let unticked: String = body
        .split('\n')
        .map(|l| match l.get(..6) {
            Some("- [x] " | "- [X] ") => format!("- [ ] {}", &l[6..]),
            _ => l.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut author = json!({"login": by});
    if let Some(id) = by_id {
        author["databaseId"] = json!(id);
    }
    let reply = json!({"databaseId": REPLY_ID, "author": author, "body": words,
        "createdAt": "2026-01-01T01:00:00Z", "lastEditedAt": null, "editor": null,
        "userContentEdits": {"totalCount": 0, "nodes": []}});
    (unticked, Some(reply))
}

/// `later` with the reply put where its time puts it, among the comments
/// written after the question.
pub fn in_order(reply: Option<Value>, later: Vec<Value>) -> Vec<Value> {
    let Some(reply) = reply else {
        return later;
    };
    let at = reply["createdAt"].as_str().unwrap_or("").to_string();
    let k = later
        .iter()
        .position(|c| c["createdAt"].as_str().is_some_and(|t| t > at.as_str()))
        .unwrap_or(later.len());
    let mut out = later;
    out.insert(k, reply);
    out
}
