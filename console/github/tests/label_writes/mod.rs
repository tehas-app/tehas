//! The label writes as the adapter asks them: the endpoints they are asked
//! of, the 404 that calls a removal done, and the refusal of a name no label
//! can have. Over a runner that answers from fixtures: no gh, no token, no
//! network, no repository. These tests reach the crate's `pub(crate)` label
//! writes, so `console/github/src/forge.rs` holds them as a `cfg(test)`
//! module; the file sits in the adapter's tests tree, where the adapter's
//! own label words are kept (`architecture.toml`, `label-words`).

use super::*;
use std::sync::{Arc, Mutex};
use tehas_core::forge::GhRunner;

/// The words of a fixture's failure. A runner gives a reason that carries no
/// separator; the line of the error adds `": "`, so a fixture reads as gh's
/// words do and no case depends on where the colon is put.
fn reason(words: &str) -> String {
    words.trim_start_matches(": ").to_string()
}

/// A runner that answers from a list and writes down every call it was asked
/// to make. Each entry is what one call returns: its answer or the reason it
/// failed. A call past the list fails by its number, so an adapter that asks
/// GitHub more than a test expected cannot pass quietly.
struct Stub {
    answers: Vec<Result<String, String>>,
    asked: Arc<Mutex<Vec<Vec<String>>>>,
}

impl GhRunner for Stub {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        let index = {
            let mut asked = self.asked.lock().unwrap();
            asked.push(args.iter().map(|a| (*a).to_string()).collect());
            asked.len() - 1
        };
        match self.answers.get(index) {
            Some(a) => a.clone(),
            None => Err(reason(&format!(": no fixture for call {index}"))),
        }
    }
}

fn forge(answers: Vec<Result<String, String>>) -> (GitHub, Arc<Mutex<Vec<Vec<String>>>>) {
    let stub = Stub {
        answers,
        asked: Arc::new(Mutex::new(Vec::new())),
    };
    let asked = Arc::clone(&stub.asked);
    (GitHub::new("acme/widget", Box::new(stub)), asked)
}

/// One event of the label events as the REST list gives it, as one JSON
/// line. A `None` time leaves `created_at` out, as an answer that says
/// nothing of it does; an empty actor is the login GitHub omits for an event
/// whose account is gone.
fn event(kind: &str, label: &str, actor: &str, at: Option<&str>) -> String {
    serde_json::json!({
        "event": kind,
        "label": {"name": label},
        "actor": {"login": actor},
        "created_at": at,
    })
    .to_string()
}

/// A REST page of events, as gh's answer holds it.
fn page(events: &[String]) -> String {
    format!("[{}]", events.join(","))
}

#[test]
fn a_removal_of_what_is_not_there_is_the_state_wanted() {
    let (mut f, asked) = forge(vec![Err(reason(": Not Found (HTTP 404)"))]);
    assert!(f.remove_label(7, "tehas:running").is_ok());
    // The path named the issue and the label, so the 404 was the label's.
    assert_eq!(
        asked.lock().unwrap()[0],
        [
            "api".to_string(),
            "-X".to_string(),
            "DELETE".to_string(),
            "repos/acme/widget/issues/7/labels/tehas%3Arunning".to_string(),
        ]
    );
}

#[test]
fn only_a_reason_of_not_found_or_a_status_of_404_calls_a_removal_done() {
    // The words the bash matches, and the status read whole: a gateway error
    // whose body happens to carry the digits 404 is no removal, and a caller
    // that believed it would leave a label on the issue that still carries it.
    let cases = [
        (reason(": Not Found (HTTP 404)"), true),
        (reason(": HTTP 404"), true),
        (reason(": not found"), true),
        (reason(": HTTP 502 upstream said 404 earlier"), false),
        (reason(": HTTP 403"), false),
        (reason(": gh ran out of its 30s bound"), false),
        // The words inside a reason that says something else are no 404: a
        // gh that could not start, or a proxy whose host was not found, left
        // the label where it was.
        (reason(": /not found/gh could not be run"), false),
        (reason(": proxy says upstream not found"), false),
        (reason(": Not Found but connection reset"), false),
    ];
    for (why, gone) in cases {
        assert_eq!(gh::says_not_found(&why), gone, "says_not_found({why:?})");
        let (mut f, _) = forge(vec![Err(why.clone())]);
        assert_eq!(
            f.remove_label(7, "tehas:running").is_ok(),
            gone,
            "remove_label of {why:?}"
        );
    }
    // A transport failure is not a removal whatever the rest of the reason
    // holds, and a 404 is a 404 beside other words.
    assert!(gh::says_not_found("HTTP 404: Not Found"));
    assert!(!gh::says_not_found("connection reset"));
}

#[test]
fn a_removal_that_really_failed_is_an_error() {
    let (mut f, _) = forge(vec![Err(reason(": HTTP 502"))]);
    assert_eq!(
        f.remove_label(7, "tehas:running").unwrap_err().why,
        "HTTP 502"
    );
}

#[test]
fn the_writes_are_asked_of_the_rest_endpoints() {
    // The list endpoint, not `gh issue edit`, which needs a GraphQL answer
    // a bot's token may not give (issue #120).
    let (mut f, asked) = forge(vec![Ok("[]".to_string()), Ok("[]".to_string())]);
    f.add_label(7, "tehas:running").unwrap();
    f.remove_label(8, "tehas:ready").unwrap();
    let asked = asked.lock().unwrap();
    assert_eq!(
        asked[0],
        [
            "api".to_string(),
            "-X".to_string(),
            "POST".to_string(),
            "repos/acme/widget/issues/7/labels".to_string(),
            "-f".to_string(),
            "labels[]=tehas:running".to_string(),
        ]
    );
    assert_eq!(
        asked[1],
        [
            "api".to_string(),
            "-X".to_string(),
            "DELETE".to_string(),
            "repos/acme/widget/issues/8/labels/tehas%3Aready".to_string(),
        ]
    );
}

#[test]
fn a_label_that_is_no_label_name_is_refused_before_the_call() {
    // A write names a label of tehas. Text read back from GitHub, or a caller
    // that guessed, is refused before the request is built: no DELETE moves to
    // another endpoint and gh is handed no flag of its own.
    for bad in [
        "",
        "..%2F..%2Fetc",
        "a/b",
        "has?query=1",
        "has#fragment",
        "has\nnewline --paginate",
        "has space",
        "with.dot",
        "with/slash",
        &"x".repeat(51),
    ] {
        let (mut f, asked) = forge(vec![Ok("[]".to_string())]);
        let e = f
            .remove_label(7, bad)
            .expect_err(&format!("{bad:?} was accepted by remove_label"));
        assert_eq!(e.call, "the label change", "{bad:?} was accepted");
        assert!(!e.why.is_empty(), "{bad:?} answered no reason");
        assert!(asked.lock().unwrap().is_empty(), "{bad:?} reached gh");
        let (mut f, asked) = forge(vec![Ok("[]".to_string())]);
        assert_eq!(
            f.add_label(7, bad).unwrap_err().call,
            "the label change",
            "{bad:?} was accepted"
        );
        assert!(asked.lock().unwrap().is_empty(), "{bad:?} reached gh");
    }
    // The last segment of a removal's path is encoded, so a name of any
    // characters stays inside it and moves the DELETE nowhere: a `:` becomes
    // %3A and the path keeps its segments. The field of an addition needs no
    // encoding, and reads as the label is written.
    let (mut f, asked) = forge(vec![Ok("[]".to_string()), Ok("[]".to_string())]);
    f.remove_label(7, "tehas:ready").unwrap();
    f.add_label(7, "tehas:ready").unwrap();
    let asked = asked.lock().unwrap();
    assert_eq!(
        asked[0][3],
        "repos/acme/widget/issues/7/labels/tehas%3Aready"
    );
    assert_eq!(*asked[1].last().unwrap(), "labels[]=tehas:ready");
    // A read takes any string, because a repository's own labels are asked
    // about with the characters the site answers them with.
    let (mut f, _) = forge(vec![Ok(page(&[event(
        "labeled",
        "needs human attention",
        "alice",
        Some("2026-01-01T00:00:00Z"),
    )]))]);
    assert_eq!(
        f.label_state(7, "needs human attention")
            .unwrap()
            .map(|e| e.actor),
        Some("alice".to_string())
    );
}

#[test]
fn a_call_past_the_fixtures_is_an_error_not_an_answer() {
    // Two terms on one adapter: the second has no fixture, so it fails by
    // name rather than reusing the first answer or answering empty.
    let (mut f, _) = forge(vec![Ok("[]".to_string())]);
    assert!(f.approval(7).unwrap().is_none());
    let e = f.add_label(7, "tehas:running").unwrap_err();
    assert_eq!(e.call, "the label change");
    assert_eq!(e.why, "no fixture for call 1");
}
