//! A drafted plan made an issue's text: what a tick on Update this issue
//! prints through `console plan update`, and what the dispatcher writes for a
//! plan with no open points, which needs no tick.
//!
//! Neither writes text that says nothing of the issue. [`made`] asks
//! [`tehas_core::textcheck`] of the draft's title and spec and answers with a
//! [`Refusal`] naming the rule, so the writer that calls it can say the reason
//! to the person and change nothing. `libexec/plan` cannot fail the turn for
//! a refused draft: the session stays open and the person replies to change
//! the plan, as with an open point.

use crate::textcheck::{check_body, check_title, Refusal};
use serde_json::Value;

/// A GitHub comment holds 65,536 characters; the plan is cut well before.
pub const CLIP: usize = 40_000;

/// What the comment says when a person still applies the label.
pub const APPLY_READY: &str = "This issue now holds the plan. Apply `tehas:ready` to build it.";

/// What a comment says when a drafted plan was refused: the line a writer
/// puts before the reason, for a person who reads the reason on the issue.
pub const REFUSED: &str = "Tehas did not make the drafted plan this issue's text";

/// What the comment says when the drafted plan's own text was refused. A
/// writer that only has to stop uses it for the reason it reads back from
/// [`made`], and never prints it.
pub const APPLY_REFUSED: &str = "Tehas did not make the drafted plan this issue's text.";

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

/// What [`made`] gives a person when the draft's text says nothing of the
/// issue: the reason, and what the person can do next.
#[derive(Debug, PartialEq, Eq)]
pub struct Refused {
    /// The rule the text failed, in the words `textcheck` gives, which quote
    /// the refused title when there is room for it.
    pub reason: String,
    /// What to tell the person to do: the plan stands as shown, and a reply
    /// naming the change drafts it again.
    pub next: String,
}

impl Refused {
    /// The line for the comment: what was not done, the rule, and what to do
    /// next. One line, so the comment that says it holds one.
    pub fn line(&self) -> String {
        format!("{REFUSED}: {}. {}", self.reason, self.next)
    }
}

impl From<Refusal> for Refused {
    fn from(refused: Refusal) -> Refused {
        Refused {
            reason: refused.line,
            next: "The plan stands as shown and nothing was written. Reply to \
                   change the title or the text of the plan, and the next turn \
                   drafts it again."
                .to_string(),
        }
    }
}

/// `said`, then the text the plan replaces, folded and quoted. `draft` is the
/// {title, spec} a turn kept, `was` the issue's {title, body}.
///
/// The draft's own title and spec are what the check reads, and the reason is
/// a [`Refused`], not a line of prose: a writer that only has to stop answers
/// with the reason in the comment it posts. A title the draft does not name is
/// the issue's own, which is the text a person wrote and approved with their
/// label, so nothing written here refuses it.
pub fn made(draft: &Value, was: &Value, said: &str) -> Result<Made, Refused> {
    let spec = text_of(draft.get("spec"));
    if spec.is_empty() {
        return Err(Refused {
            reason: "the drafted plan has no text".into(),
            next: "Reply to change the plan, and the next turn drafts it again. \
                   A plan of nothing has nothing to write."
                .to_string(),
        });
    }
    // The draft's own title is the only title this function can put on the
    // issue, so it is the only one the check reads: a draft that names none
    // leaves the issue the title it already holds, which a person wrote and
    // approved with the label that started the session.
    let named = text_of(draft.get("title"));
    if !named.is_empty() {
        check_title(&named, &spec).map_err(Refused::from)?;
    }
    check_body(&spec).map_err(Refused::from)?;
    let title = if named.is_empty() {
        text_of(was.get("title"))
    } else {
        named
    };
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
        let m = made(&json!({"title": "", "spec": "Do it."}), &was, "Said.").unwrap();
        assert_eq!(m.title, "search");
        assert_eq!(m.body, "Do it.");
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
        // A spec of nothing is refused before the title is read: there is
        // nothing to write whichever way the title reads.
        assert!(made(&json!({"title": "", "spec": ""}), &was, "Said.").is_err());
    }

    #[test]
    fn a_draft_whose_title_says_nothing_makes_nothing_and_says_why() {
        let was = json!({"title": "search", "body": "fast"});
        let refused = made(&json!({"title": "EPIC: EPIC", "spec": "Do it."}), &was, "Said.")
            .expect_err("a title of nothing but prefixes");
        assert_eq!(refused.reason, "the title is nothing but prefixes: epic epic");
        assert!(refused.next.contains("nothing was written"), "{}", refused.next);
        // The issue keeps the title it has, which the draft did not name, so
        // the draft cannot refuse it: it is what a person wrote.
        assert!(made(&json!({"title": "", "spec": "Do it."}), &was, "Said.").is_ok());
        assert!(made(
            &json!({"title": "faster search", "spec": "Do it."}),
            &was,
            "Said."
        )
        .is_ok());
    }

    #[test]
    fn a_draft_whose_text_is_the_brief_makes_nothing_and_says_why() {
        let was = json!({"title": "search", "body": "fast"});
        let refused = made(
            &json!({"title": "faster search",
                    "spec": "Descriptor: what the issue is\n\n## Prior context inventory\n\n- the file"}),
            &was,
            "Said.",
        )
        .expect_err("a body that is the brief");
        assert!(
            refused.reason.starts_with("the body is the planner's brief"),
            "{}",
            refused.reason
        );
        assert!(refused.next.contains("Reply"), "{}", refused.next);
    }
}
