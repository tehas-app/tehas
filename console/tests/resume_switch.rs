//! `tehas resume`'s read of the other switch, `tehas:pause` on the control
//! issue, through the GitHub adapter the console's wiring gives it: who took
//! the label off decides (AGENTS.md rule 5), and an answer that cannot be read
//! is said on standard error.

use serde_json::json;
use tehas_core::forge::GhRunner;
use tehas_ops::holds;

struct Answers {
    labels: Result<String, String>,
    events: Result<String, String>,
}

impl GhRunner for Answers {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        if args[1].contains("/events") {
            self.events.clone()
        } else {
            self.labels.clone()
        }
    }
}

fn other(labels: Result<&str, &str>, events: Result<&str, &str>) -> Option<Result<String, String>> {
    let owned = |r: Result<&str, &str>| r.map(String::from).map_err(String::from);
    let runner = Answers {
        labels: owned(labels),
        events: owned(events),
    };
    let mut forge = tehas_github::GitHub::new("acme/widget", Box::new(runner));
    holds::other_switch(&mut forge, 1, &|who| who == "alice")
}

const NONE: &str = r#"{"labels":[]}"#;

fn pause_event(kind: &str, by: &str) -> String {
    json!([{"event": kind, "label": {"name": "tehas:pause"}, "actor": {"login": by},
            "created_at": "2026-01-02T00:00:00Z"}])
    .to_string()
}

#[test]
fn the_label_on_the_issue_holds_the_factory() {
    let on = r#"{"labels":[{"name":"bug"},{"name":"tehas:pause"}]}"#;
    assert_eq!(
        other(Ok(on), Err("not asked")),
        Some(Ok(
            "the factory stays paused: tehas:pause is still on issue #1".to_string()
        ))
    );
}

#[test]
fn who_took_the_label_off_decides() {
    let stranger = pause_event("unlabeled", "mallory");
    assert_eq!(
        other(Ok(NONE), Ok(&stranger)),
        Some(Ok("the factory stays paused: tehas:pause was taken off #1 by 'mallory', who may not authorise work here".to_string()))
    );
    let nobody = json!([{"event": "unlabeled", "label": {"name": "tehas:pause"}, "actor": null}]);
    assert_eq!(
        other(Ok(NONE), Ok(&nobody.to_string())),
        Some(Ok("the factory stays paused: tehas:pause was taken off #1 by 'unknown', who may not authorise work here".to_string()))
    );
    assert_eq!(
        other(Ok(NONE), Ok(&pause_event("unlabeled", "alice"))),
        None
    );
    assert_eq!(other(Ok(NONE), Ok("[]")), None);
}

#[test]
fn an_application_nothing_took_back_holds_the_factory_whoever_made_it() {
    for by in ["mallory", "alice"] {
        assert_eq!(
            other(Ok(NONE), Ok(&pause_event("labeled", by))),
            Some(Ok("the factory stays paused: tehas:pause is not on issue #1, but its last tehas:pause event only applied it".to_string()))
        );
    }
}

#[test]
fn an_answer_that_cannot_be_read_is_said_on_standard_error() {
    assert_eq!(
        other(Err("HTTP 502"), Ok("[]")),
        Some(Err("tehas resume: the labels of issue #1 could not be read: check tehas:pause there yourself".to_string()))
    );
    assert_eq!(
        other(Ok("{}"), Ok("[]")),
        Some(Err("tehas resume: the labels of issue #1 could not be read: check tehas:pause there yourself".to_string()))
    );
    assert_eq!(
        other(Ok(NONE), Err("HTTP 502")),
        Some(Err(
            "tehas resume: the label events of issue #1 could not be read".to_string()
        ))
    );
    assert_eq!(
        other(Ok(NONE), Ok("{}")),
        Some(Err(
            "tehas resume: the label events of issue #1 could not be read".to_string()
        ))
    );
}
