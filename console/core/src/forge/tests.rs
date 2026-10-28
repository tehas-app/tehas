//! The tests of the forge: the four cases the issue names first, then the
//! shape of the calls and the reading of an answer that is no answer.
//!
//! Every case runs the adapter against a [`GhRunner`] that answers from
//! fixtures, so no case needs gh, a token, a network or a repository: what a
//! case decides is the rule, not the site.

use super::gh;
use super::{Forge, GhRunner, GitHub, LabelEvent, APPROVAL_LABEL};
use serde_json::Value;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;

/// A runner that answers from a list and writes down every call it was asked
/// to make. Each entry is what one call returns: its answer or the reason it
/// failed. A call past the list fails by its number, so an adapter that asks
/// GitHub more than a test expected cannot pass quietly.
struct Stub {
    answers: Vec<Result<String, String>>,
    asked: Rc<RefCell<Vec<Vec<String>>>>,
}

impl GhRunner for Stub {
    fn run(&mut self, args: &[String]) -> Result<String, String> {
        let index = {
            let mut asked = self.asked.borrow_mut();
            asked.push(args.to_vec());
            asked.len() - 1
        };
        match self.answers.get(index) {
            Some(a) => a.clone(),
            None => Err(format!(": no fixture for call {index}")),
        }
    }
}

fn forge(answers: Vec<Result<String, String>>) -> (GitHub, Rc<RefCell<Vec<Vec<String>>>>) {
    let stub = Stub {
        answers,
        asked: Rc::new(RefCell::new(Vec::new())),
    };
    let asked = Rc::clone(&stub.asked);
    (GitHub::new("acme/widget", "gh", Box::new(stub)), asked)
}

/// One event as the REST list gives it. A `None` time leaves `created_at`
/// out, as an answer that says nothing of it does; an empty actor is the
/// login GitHub omits for an event whose account is gone.
fn event(kind: &str, label: &str, actor: &str, at: Option<&str>) -> Value {
    serde_json::json!({
        "event": kind,
        "label": {"name": label},
        "actor": {"login": actor},
        "created_at": at,
    })
}

fn page(events: &[Value]) -> String {
    Value::Array(events.to_vec()).to_string()
}

#[test]
fn an_approval_is_read_from_two_pages_of_events() {
    // The ready label was applied on the first page and applied again on
    // the second, so an adapter that read only the first page answers with
    // the wrong account. --paginate is what brings the second page at all.
    let one = page(&[
        event(
            "labeled",
            APPROVAL_LABEL,
            "alice",
            Some("2026-01-01T00:00:00Z"),
        ),
        event(
            "labeled",
            "tehas:urgent",
            "bob",
            Some("2026-01-01T00:00:00Z"),
        ),
    ]);
    let two = page(&[
        event(
            "unlabeled",
            APPROVAL_LABEL,
            "carol",
            Some("2026-01-02T00:00:00Z"),
        ),
        event(
            "labeled",
            APPROVAL_LABEL,
            "dana",
            Some("2026-01-03T00:00:00Z"),
        ),
    ]);
    let (mut f, asked) = forge(vec![Ok(format!("{one}\n{two}"))]);
    assert_eq!(
        f.approval(7).unwrap(),
        ("dana".to_string(), "2026-01-03T00:00:00Z".to_string())
    );
    let asked = asked.borrow();
    assert_eq!(
        asked[0],
        [
            "gh".to_string(),
            "api".to_string(),
            "repos/acme/widget/issues/7/events?per_page=100".to_string(),
            "--paginate".to_string(),
        ]
    );
}

#[test]
fn a_label_added_and_then_removed_reads_as_unlabeled() {
    let events = page(&[
        event(
            "labeled",
            "tehas:running",
            "alice",
            Some("2026-01-01T00:00:00Z"),
        ),
        event(
            "unlabeled",
            "tehas:running",
            "bot",
            Some("2026-01-02T00:00:00Z"),
        ),
    ]);
    let (mut f, _) = forge(vec![Ok(events)]);
    assert_eq!(
        f.label_state(7, "tehas:running").unwrap(),
        Some(LabelEvent {
            kind: "unlabeled".to_string(),
            actor: "bot".to_string(),
            at: "2026-01-02T00:00:00Z".to_string(),
        })
    );
}

#[test]
fn two_events_in_the_same_second_keep_their_order() {
    // Both in one second: the stamp cannot separate them, so the page's
    // order and the kind decide, as lib/labels.sh reads them.
    let at = Some("2026-01-01T00:00:00Z");
    // Of an application and a removal in one second the removal counts,
    // whichever order the page handed them back in.
    for order in [0, 1] {
        let events = if order == 0 {
            page(&[
                event("labeled", APPROVAL_LABEL, "alice", at),
                event("unlabeled", APPROVAL_LABEL, "mallory", at),
            ])
        } else {
            page(&[
                event("unlabeled", APPROVAL_LABEL, "mallory", at),
                event("labeled", APPROVAL_LABEL, "alice", at),
            ])
        };
        let (mut f, _) = forge(vec![Ok(events)]);
        let state = f.label_state(7, APPROVAL_LABEL).unwrap().unwrap();
        assert_eq!(
            (state.kind.as_str(), state.actor.as_str()),
            ("unlabeled", "mallory"),
            "order {order} was read as {state:?}"
        );
    }
    // Two applications in one second: the stamp cannot separate them and the
    // approval term sorts by the stamp alone, as `tehas_label_actor` and the
    // dispatcher's `label_actor` do, so the LAST one in the page answers. The
    // state term does not separate them either, so it answers the same.
    let (mut f, _) = forge(vec![Ok(page(&[
        event("labeled", APPROVAL_LABEL, "alice", at),
        event("labeled", APPROVAL_LABEL, "bob", at),
    ]))]);
    assert_eq!(f.approval(7).unwrap().0, "bob");
}

#[test]
fn an_approval_is_the_application_and_never_the_removal() {
    // The label was applied by an allowed account and taken off a second
    // later by an outside one. `tehas_label_actor` filters to `labeled`, so
    // it answers with the applier and its time; an adapter that read the
    // newest event of the label would answer with the remover, and the
    // caller would judge a remover against TEHAS_ALLOWED_LABELERS.
    let events = page(&[
        event(
            "labeled",
            APPROVAL_LABEL,
            "alice",
            Some("2026-01-01T00:00:00Z"),
        ),
        event(
            "unlabeled",
            APPROVAL_LABEL,
            "bot",
            Some("2026-01-02T00:00:00Z"),
        ),
    ]);
    let (mut f, _) = forge(vec![Ok(events)]);
    assert_eq!(
        f.approval(7).unwrap(),
        ("alice".to_string(), "2026-01-01T00:00:00Z".to_string())
    );
    // The state term still reads the removal, because it answers whether the
    // label is on the issue: the two terms differ on purpose.
    let (mut f, _) = forge(vec![Ok(page(&[
        event(
            "labeled",
            APPROVAL_LABEL,
            "alice",
            Some("2026-01-01T00:00:00Z"),
        ),
        event(
            "unlabeled",
            APPROVAL_LABEL,
            "bot",
            Some("2026-01-02T00:00:00Z"),
        ),
    ]))]);
    let state = f.label_state(7, APPROVAL_LABEL).unwrap().unwrap();
    assert_eq!(
        (state.kind.as_str(), state.actor.as_str()),
        ("unlabeled", "bot")
    );
    // A removal and nothing else approves nothing: no event applied the
    // label, so the pair is empty and the caller treats it as nobody.
    let (mut f, _) = forge(vec![Ok(page(&[event(
        "unlabeled",
        APPROVAL_LABEL,
        "bot",
        Some("2026-01-02T00:00:00Z"),
    )]))]);
    assert_eq!(f.approval(7).unwrap(), (String::new(), String::new()));
    // Two applications in one second: the stamp cannot separate them, so the
    // LAST one the page handed back answers - `alice` in both pages below.
    // That is `tehas_label_actor`'s rule: its jq sorts by the stamp alone and
    // jq's sort_by is stable, so equal stamps keep the page's order, and the
    // dispatcher's `label_actor` sorts the same way. Checked against both
    // readers rather than reasoned out: a rule about a tie is exactly the kind
    // that reads backwards in one's head.
    let at = Some("2026-01-01T00:00:00Z");
    for order in [0, 1] {
        let events = if order == 0 {
            vec![
                event("labeled", APPROVAL_LABEL, "bob", at),
                event("labeled", APPROVAL_LABEL, "alice", at),
            ]
        } else {
            vec![
                event("labeled", APPROVAL_LABEL, "mallory", at),
                event("labeled", APPROVAL_LABEL, "alice", at),
            ]
        };
        let (mut f, _) = forge(vec![Ok(page(&events))]);
        assert_eq!(f.approval(7).unwrap().0, "alice", "order {order}");
        // The state term separates a removal from an application of one
        // second, but it does not separate two applications, so it answers
        // this pair the same way.
        let (mut f, _) = forge(vec![Ok(page(&events))]);
        let state = f.label_state(7, APPROVAL_LABEL).unwrap().unwrap();
        assert_eq!(
            (state.kind.as_str(), state.actor.as_str()),
            ("labeled", "alice"),
            "order {order} was read as {state:?}"
        );
    }
}

#[test]
fn a_failed_gh_call_is_an_error_value() {
    let (mut f, _) = forge(vec![Err(": HTTP 403".to_string())]);
    let e = f.approval(7).unwrap_err();
    assert_eq!(e.call, "the issue's label events");
    assert_eq!(e.why, ": HTTP 403");
    assert_eq!(e.line(), "ERROR: the issue's label events failed: HTTP 403");
    // Nothing was answered. The caller holds an error, not an approval by
    // nobody, which is the difference between a poll that stops and one
    // that says the issue is unapproved and starts nothing.
}

#[test]
fn a_removal_of_what_is_not_there_is_the_state_wanted() {
    let (mut f, asked) = forge(vec![Err(": Not Found (HTTP 404)".to_string())]);
    assert!(f.remove_label(7, "tehas:running").is_ok());
    // The path named the issue and the label, so the 404 was the label's.
    assert_eq!(
        asked.borrow()[0],
        [
            "gh".to_string(),
            "api".to_string(),
            "-X".to_string(),
            "DELETE".to_string(),
            "repos/acme/widget/issues/7/labels/tehas:running".to_string(),
        ]
    );
}

#[test]
fn a_removal_that_really_failed_is_an_error() {
    let (mut f, _) = forge(vec![Err(": HTTP 502".to_string())]);
    assert_eq!(
        f.remove_label(7, "tehas:running").unwrap_err().why,
        ": HTTP 502"
    );
}

#[test]
fn the_writes_are_asked_of_the_rest_endpoints() {
    // The list endpoint, not `gh issue edit`, which needs a GraphQL answer
    // a bot's token may not give (issue #120).
    let (mut f, asked) = forge(vec![Ok("[]".to_string()), Ok("[]".to_string())]);
    f.add_label(7, "tehas:running").unwrap();
    f.remove_label(8, "tehas:ready").unwrap();
    let asked = asked.borrow();
    assert_eq!(
        asked[0],
        [
            "gh".to_string(),
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
            "gh".to_string(),
            "api".to_string(),
            "-X".to_string(),
            "DELETE".to_string(),
            "repos/acme/widget/issues/8/labels/tehas:ready".to_string(),
        ]
    );
}

#[test]
fn the_labels_of_an_issue_come_from_its_rest_read() {
    let (mut f, asked) = forge(vec![Ok(
        r#"{"number":7,"labels":[{"name":"tehas:ready"},{"name":"bug"}]}"#.to_string(),
    )]);
    assert_eq!(f.labels(7).unwrap(), ["tehas:ready", "bug"]);
    assert_eq!(
        asked.borrow()[0],
        [
            "gh".to_string(),
            "api".to_string(),
            "repos/acme/widget/issues/7".to_string()
        ]
    );
}

#[test]
fn an_answer_that_is_no_answer_is_an_error_and_not_no_labels() {
    // The empty list means "this issue carries no label": an answer that
    // cannot be read must not be allowed to look like it.
    // Answers that say nothing of the issue's events. Each must come back
    // as an error: an empty list is the site's answer that an issue
    // carries no label, and a poll that took one of these for it would
    // clear a label nobody took off.
    let cases = [
        ("not json", "no json at all"),
        ("null", "a null answer"),
        ("[]] [[", "two pages that break apart"),
        ("{\"event\":\"labeled\"}", "one object, no page"),
        (
            "[[{\"name\":\"tehas:ready\"}]]",
            "an entry that is an array, no object",
        ),
        ("[1]", "an entry that is a number, no object"),
        ("[] not json", "a page and then rubbish"),
        ("   ", "whitespace and nothing else"),
        ("[\"labeled\"]", "a page whose entry is no object"),
        ("true", "a value that is no list"),
    ];
    for (out, why_it_is_nothing) in cases {
        let (mut f, _) = forge(vec![Ok(out.to_string())]);
        let e = f.label_state(7, APPROVAL_LABEL).expect_err(&format!(
            "{out:?} ({why_it_is_nothing}) was read as an answer"
        ));
        assert_eq!(e.call, "the issue's label events", "{out:?}");
        assert!(!e.why.is_empty(), "{out:?} answered no reason");
    }
    // What an issue that really carries nothing answers, and nothing more:
    // an empty page is an answer, not a failure.
    let (mut f, _) = forge(vec![Ok("[]".to_string())]);
    assert_eq!(f.label_state(7, APPROVAL_LABEL).unwrap(), None);
    // A well-formed event that names no label is still a read of the
    // issue's events: the issue carries the label nothing names.
    let (mut f, _) = forge(vec![Ok("[{\"event\":\"labeled\"}]".to_string())]);
    assert_eq!(f.label_state(7, APPROVAL_LABEL).unwrap(), None);
    let (mut f, _) = forge(vec![Ok("{\"labels\":[]}".to_string())]);
    assert!(f.labels(7).unwrap().is_empty());
    // An issue read that is no issue says so too.
    let (mut f, _) = forge(vec![Ok("not json".to_string())]);
    assert_eq!(f.labels(7).unwrap_err().call, "the issue");
}

#[test]
fn an_event_naming_nobody_or_no_time_answers_empty() {
    // Reported as the site says it: an account that is gone is the
    // caller's to refuse, not a reason to hide the event. The approval
    // term and the state term read the same answer, so one call serves
    // both.
    let (mut f, _) = forge(vec![Ok(page(&[event(
        "labeled",
        APPROVAL_LABEL,
        "",
        None,
    )]))]);
    assert_eq!(f.approval(7).unwrap(), (String::new(), String::new()));
    let (mut f, _) = forge(vec![Ok(page(&[event(
        "labeled",
        APPROVAL_LABEL,
        "",
        None,
    )]))]);
    let state = f.label_state(7, APPROVAL_LABEL).unwrap().unwrap();
    assert_eq!((state.kind.as_str(), state.actor.as_str()), ("labeled", ""));
    assert!(state.at.is_empty());
}

#[test]
fn an_unstamped_event_is_older_than_a_stamped_one() {
    let (mut f, _) = forge(vec![Ok(page(&[
        event(
            "labeled",
            APPROVAL_LABEL,
            "alice",
            Some("2026-01-01T00:00:00Z"),
        ),
        event("labeled", APPROVAL_LABEL, "nobody", None),
    ]))]);
    assert_eq!(f.approval(7).unwrap().0, "alice");
}

#[test]
fn only_the_two_kinds_of_event_carry_a_label() {
    // An event that names the label in passing is not a label on the
    // issue: a subscribed entry approves nothing.
    let events = page(&[serde_json::json!({
        "event": "subscribed",
        "label": {"name": APPROVAL_LABEL},
        "actor": {"login": "alice"},
        "created_at": "2026-01-09T00:00:00Z",
    })]);
    let (mut f, _) = forge(vec![Ok(events)]);
    assert_eq!(f.approval(7).unwrap(), (String::new(), String::new()));
}

#[test]
fn pages_side_by_side_are_all_read() {
    // gh streams its pages, so two arrays may sit together with no
    // separator between them at all.
    let two = format!(
        "{}{}",
        page(&[event(
            "labeled",
            APPROVAL_LABEL,
            "alice",
            Some("2026-01-01T00:00:00Z"),
        )]),
        page(&[event(
            "labeled",
            APPROVAL_LABEL,
            "bob",
            Some("2026-01-02T00:00:00Z"),
        )]),
    );
    let (mut f, _) = forge(vec![Ok(two)]);
    assert_eq!(f.approval(7).unwrap().0, "bob");
}

#[test]
fn the_reason_keeps_only_the_words_it_may_quote() {
    // A token or a private address in gh's answer stays out of the line a
    // caller logs: only the fixed words, or the exit status, ever leave.
    // Each case is one line of gh's own, which is what the caller logs.
    assert_eq!(
        gh::why("secondary rate limit", Some(1)),
        ": secondary rate limit"
    );
    // The first line that says anything is the one quoted, and within it
    // the first word of the list that matches, so the words a caller reads
    // are never more of gh's answer than this list.
    assert_eq!(gh::why("rate limit", Some(1)), ": rate limit");
    assert_eq!(
        gh::why("Get \"x\": dial tcp: no such host", Some(1)),
        ": no such host"
    );
    assert_eq!(gh::why("HTTP 404: bad", Some(1)), ": HTTP 404");
    assert_eq!(gh::why("gh: something else", Some(1)), ": gh exited 1");
    assert_eq!(gh::why("", None), ": gh was stopped by a signal");
    // A line that only holds the word is no status, and no status is the
    // one a removal calls success.
    assert_eq!(gh::why("the http call failed", Some(1)), ": gh exited 1");
    assert!(gh::says_not_found(": Not Found (HTTP 404)"));
    assert!(gh::says_not_found(": 404"));
    assert!(!gh::says_not_found(": HTTP 403"));
}

#[test]
fn a_runner_that_cannot_be_run_names_the_program() {
    // A host whose gh is missing must say so, not answer that the issue
    // carries no labels.
    let (mut f, _) = forge(vec![Err(": gh: no such file or directory".to_string())]);
    let e = f.labels(7).unwrap_err();
    assert_eq!(e.call, "the issue");
    assert_eq!(
        e.line(),
        "ERROR: the issue failed: gh: no such file or directory"
    );
}

#[test]
fn a_call_past_the_fixtures_is_an_error_not_an_answer() {
    // Two terms on one adapter: the second has no fixture, so it fails by
    // name rather than reusing the first answer or answering empty.
    let (mut f, _) = forge(vec![Ok("[]".to_string())]);
    assert!(f.approval(7).unwrap().0.is_empty());
    let e = f.add_label(7, "tehas:running").unwrap_err();
    assert_eq!(e.call, "the label change");
    assert_eq!(e.why, ": no fixture for call 1");
}

#[test]
fn the_order_of_one_second_is_the_rule_once() {
    // The helper the adapter's sort and this test share: two events in one
    // second are ordered by kind alone, so a rewrite that drops the
    // tie-break is caught here as well as through the adapter.
    let at = Some("2026-01-01T00:00:00Z");
    let made = |kind: &str| gh::Event {
        event: kind.to_string(),
        label: APPROVAL_LABEL.to_string(),
        actor: "a".to_string(),
        at: at.map(str::to_string),
    };
    let (app, removal) = (made("labeled"), made("unlabeled"));
    assert_eq!(gh::event_order(&&removal, &&app), Ordering::Greater);
    assert_eq!(gh::event_order(&&app, &&app), Ordering::Equal);
    // An unstamped event is older than any stamped one, and a stamped
    // removal beats an unstamped application of the same label.
    let ghost = gh::Event {
        event: "labeled".to_string(),
        label: APPROVAL_LABEL.to_string(),
        actor: "a".to_string(),
        at: None,
    };
    assert_eq!(gh::event_order(&&ghost, &&app), Ordering::Less);
    assert_eq!(gh::event_order(&&removal, &&ghost), Ordering::Greater);
}
