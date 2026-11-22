//! The fixtures of before 2.0 answered a question by ticking one of its
//! boxes. A box no longer answers (#2782): a reply from the same account
//! naming the same choice does. `as_reply` turns a body with one box ticked
//! into the same body with none, and the reply that makes that choice, at the
//! time the tick was saved; a body with more or fewer ticks stays as it is.

use serde_json::{json, Value};

/// The reply's comment id: after the question's 4001.
pub const REPLY_ID: u64 = 4002;

/// The key of a comment's time, as GitHub's GraphQL names it.
pub const CREATED: &str = "createdAt";

/// The body with its one tick taken off, and the comments written after the
/// question with the reply of `by` that names the choice it ticked among them
/// at its time, when exactly one box was ticked and `by` is someone; the
/// body and the comments as they were otherwise.
pub fn as_reply(
    body: &str,
    by: &str,
    by_id: Option<u64>,
    later: Vec<Value>,
) -> (String, Vec<Value>, bool) {
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
        return (body.to_string(), later, false);
    }
    let i = ticked[0];
    let label = &boxes[i][6..];
    // The end of a planning session is named by its word: a bare number
    // names only one of the planner's own options.
    let words = if body.contains("kind=plan") && i + 1 == boxes.len() {
        "end".to_string()
    } else if body.contains("kind=plan") {
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
    let mut reply = json!({"databaseId": REPLY_ID, "author": author, "body": words,
        "lastEditedAt": null, "editor": null,
        "userContentEdits": {"totalCount": 0, "nodes": []}});
    reply[CREATED] = json!("2026-01-01T01:00:00Z");
    let k = later
        .iter()
        .position(|c| {
            c[CREATED]
                .as_str()
                .is_some_and(|t| t > "2026-01-01T01:00:00Z")
        })
        .unwrap_or(later.len());
    let mut later = later;
    later.insert(k, reply);
    (unticked, later, true)
}
