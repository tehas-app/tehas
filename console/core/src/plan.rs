//! A drafted plan made an issue's text: what a tick on Update this issue
//! prints through `console plan update`, and what the dispatcher writes for a
//! plan with no open points, which needs no tick.

use serde_json::Value;

/// A GitHub comment holds 65,536 characters; the plan is cut well before.
pub const CLIP: usize = 40_000;

/// What the comment says when a person still applies the label.
pub const APPLY_READY: &str = "This issue now holds the plan. Apply `tehas:ready` to build it.";

pub fn clip(s: &str, what: &str) -> String {
    match s.char_indices().nth(CLIP) {
        Some((at, _)) => format!(
            "{}\n\n(cut: {what} is longer than a comment holds)",
            &s[..at]
        ),
        None => s.to_string(),
    }
}

/// A value as jq's `(. // "") | tostring` gives it.
pub fn text_of(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) | Some(Value::Bool(false)) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
    }
}

/// The issue's new title and body, and the comment that ends the session.
#[derive(Debug, PartialEq, Eq)]
pub struct Made {
    pub title: String,
    pub body: String,
    pub text: String,
}

/// `said`, then the text the plan replaces, folded and quoted. `draft` is the
/// {title, spec} a turn kept, `was` the issue's {title, body}.
pub fn made(draft: &Value, was: &Value, said: &str) -> Result<Made, String> {
    let spec = text_of(draft.get("spec"));
    if spec.is_empty() {
        return Err("the drafted plan has no text".into());
    }
    let mut title = text_of(draft.get("title"));
    if title.is_empty() {
        title = text_of(was.get("title"));
    }
    let old = format!(
        "# {}\n\n{}",
        text_of(was.get("title")),
        text_of(was.get("body"))
    );
    let quoted = clip(&old, "the text")
        .split('\n')
        .map(|l| format!("> {l}"))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Made {
        title,
        body: clip(&spec, "the plan"),
        text: format!(
            "{said}\n\n<details><summary>The text it replaced</summary>\n\n{quoted}\n\n</details>"
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_plan_with_no_title_keeps_the_issues() {
        let was = json!({"title": "search", "body": "fast"});
        let m = made(&json!({"title": "", "spec": "x"}), &was, "Said.").unwrap();
        assert_eq!(m.title, "search");
        assert_eq!(m.body, "x");
        assert_eq!(
            m.text,
            "Said.\n\n<details><summary>The text it replaced</summary>\n\n> # search\n> \n> fast\n\n</details>"
        );
    }

    #[test]
    fn a_plan_with_no_text_makes_nothing() {
        let was = json!({"title": "search", "body": "fast"});
        assert!(made(&json!({"title": "t", "spec": ""}), &was, "Said.").is_err());
        assert!(made(&json!({"title": "t"}), &was, "Said.").is_err());
    }
}
