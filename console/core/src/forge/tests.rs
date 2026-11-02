//! The tests of the forge: the four cases the issue names first, then the
//! shape of the calls and the reading of an answer that is no answer.
//!
//! Every case runs the adapter against a [`GhRunner`] that answers from
//! fixtures, so no case needs gh, a token, a network or a repository: what a
//! case decides is the rule, not the site.

use super::gh;
use super::{Approval, Forge, GhRunner, GitHub, LabelEvent, APPROVAL_LABEL};
use serde_json::Value;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;

/// The words of a fixture's failure. A runner gives a reason that carries no
/// separator; the line of the error adds `": "`, so a fixture reads as gh's
/// words do and no case depends on where the colon is put.
fn reason(words: &str) -> String {
    words.trim_start_matches(": ").to_string()
}

/// A runner that answers from a list and writes down every call it was asked
/// to make. Each entry is what one call returns: its answer or the reason it
/// failed. A call past the list fails by its number, so an adapter that asks
/// GitHub more than a test expected cannot pass quietly. The runner holds the
/// program, so what a case asserts is the words after it.
struct Stub {
    answers: Vec<Result<String, String>>,
    asked: Rc<RefCell<Vec<Vec<String>>>>,
}

impl GhRunner for Stub {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        let index = {
            let mut asked = self.asked.borrow_mut();
            asked.push(args.iter().map(|a| (*a).to_string()).collect());
            asked.len() - 1
        };
        match self.answers.get(index) {
            Some(a) => a.clone(),
            None => Err(reason(&format!(": no fixture for call {index}"))),
        }
    }
}

fn forge(answers: Vec<Result<String, String>>) -> (GitHub, Rc<RefCell<Vec<Vec<String>>>>) {
    let stub = Stub {
        answers,
        asked: Rc::new(RefCell::new(Vec::new())),
    };
    let asked = Rc::clone(&stub.asked);
    (GitHub::new("acme/widget", Box::new(stub)), asked)
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
        Some(Approval {
            actor: "dana".to_string(),
            at: "2026-01-03T00:00:00Z".to_string(),
        })
    );
    let asked = asked.borrow();
    assert_eq!(
        asked[0],
        [
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
    // order and the kind decide, as `console label` reads them.
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
    // Two applications in one second: the stamp cannot separate them, so the
    // LAST one in the page answers, as `tehas_label_actor` and the
    // dispatcher's `label_actor` read it. The state term does not separate
    // them either, so it answers the same.
    let (mut f, _) = forge(vec![Ok(page(&[
        event("labeled", APPROVAL_LABEL, "alice", at),
        event("labeled", APPROVAL_LABEL, "bob", at),
    ]))]);
    assert_eq!(f.approval(7).unwrap().unwrap().actor, "bob");
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
        Some(Approval {
            actor: "alice".to_string(),
            at: "2026-01-01T00:00:00Z".to_string(),
        })
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
    assert_eq!(f.approval(7).unwrap(), None);
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
        assert_eq!(
            f.approval(7).unwrap().unwrap().actor,
            "alice",
            "order {order}"
        );
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
    let (mut f, _) = forge(vec![Err(reason(": HTTP 403"))]);
    let e = f.approval(7).unwrap_err();
    assert_eq!(e.call, "the issue's label events");
    assert_eq!(e.why, "HTTP 403");
    assert_eq!(e.line(), "ERROR: the issue's label events failed: HTTP 403");
    // The separator is the line's, not the reason's: a reason that came with
    // one still reads as one sentence, and one that has nothing to say leaves
    // no dangling colon.
    assert_eq!(
        super::ForgeError::new("the issue", reason(": HTTP 403")).line(),
        "ERROR: the issue failed: HTTP 403"
    );
    assert_eq!(
        super::ForgeError::new("the issue", "").line(),
        "ERROR: the issue failed"
    );
    // The same line through Display, so a caller can format it or `?` it.
    assert_eq!(
        format!(
            "{}",
            super::ForgeError::new("the issue", reason(": HTTP 403"))
        ),
        "ERROR: the issue failed: HTTP 403"
    );
    // Nothing was answered. The caller holds an error, not an approval by
    // nobody, which is the difference between a poll that stops and one
    // that says the issue is unapproved and starts nothing.
}

#[test]
fn a_removal_of_what_is_not_there_is_the_state_wanted() {
    let (mut f, asked) = forge(vec![Err(reason(": Not Found (HTTP 404)"))]);
    assert!(f.remove_label(7, "tehas:running").is_ok());
    // The path named the issue and the label, so the 404 was the label's.
    assert_eq!(
        asked.borrow()[0],
        [
            "api".to_string(),
            "-X".to_string(),
            "DELETE".to_string(),
            "repos/acme/widget/issues/7/labels/tehas%3Arunning".to_string(),
        ]
    );
}

#[test]
fn only_the_words_not_found_or_a_status_of_404_call_a_removal_done() {
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
    let asked = asked.borrow();
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
        assert!(asked.borrow().is_empty(), "{bad:?} reached gh");
        let (mut f, asked) = forge(vec![Ok("[]".to_string())]);
        assert_eq!(
            f.add_label(7, bad).unwrap_err().call,
            "the label change",
            "{bad:?} was accepted"
        );
        assert!(asked.borrow().is_empty(), "{bad:?} reached gh");
    }
    // The last segment of a removal's path is encoded, so a name of any
    // characters stays inside it and moves the DELETE nowhere: a `:` becomes
    // %3A and the path keeps its segments. The field of an addition needs no
    // encoding, and reads as the label is written.
    let (mut f, asked) = forge(vec![Ok("[]".to_string()), Ok("[]".to_string())]);
    f.remove_label(7, "tehas:ready").unwrap();
    f.add_label(7, "tehas:ready").unwrap();
    let asked = asked.borrow();
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
fn any_label_can_be_asked_of_the_forge() {
    // The approval term is one label of the general one, so the same read
    // answers who applied `tehas:urgent`, which is what status and tidy ask.
    let events = page(&[
        event(
            "labeled",
            "tehas:urgent",
            "alice",
            Some("2026-01-01T00:00:00Z"),
        ),
        event(
            "labeled",
            "tehas:urgent",
            "bob",
            Some("2026-01-02T00:00:00Z"),
        ),
        event(
            "unlabeled",
            "tehas:urgent",
            "carol",
            Some("2026-01-03T00:00:00Z"),
        ),
    ]);
    // Two terms of one issue read the events twice, one call each: the caller
    // that wants both holds what it read, as the poll caches them.
    let second = events.clone();
    let third = events.clone();
    let (mut f, _) = forge(vec![Ok(events), Ok(second), Ok(third)]);
    assert_eq!(
        f.applied_by(7, "tehas:urgent").unwrap(),
        Some(Approval {
            actor: "bob".to_string(),
            at: "2026-01-02T00:00:00Z".to_string(),
        })
    );
    // Each term reads the events for itself: the caller that wants both holds
    // what it read, as the poll caches them.
    assert_eq!(
        f.label_state(7, "tehas:urgent").unwrap().map(|e| e.kind),
        Some("unlabeled".to_string())
    );
    // The general term answers the approval's label too, and an issue nothing
    // approved answers None, not a pair of empty strings.
    let removed = page(&[event(
        "unlabeled",
        APPROVAL_LABEL,
        "bot",
        Some("2026-01-03T00:00:00Z"),
    )]);
    let (mut f, _) = forge(vec![Ok(removed.clone()), Ok(removed)]);
    assert_eq!(f.applied_by(7, APPROVAL_LABEL).unwrap(), None);
    assert_eq!(f.approval(7).unwrap(), None);
}

#[test]
fn the_labels_of_an_issue_come_from_its_rest_read() {
    let (mut f, asked) = forge(vec![Ok(
        r#"{"number":7,"labels":[{"name":"tehas:ready"},{"name":"bug"}]}"#.to_string(),
    )]);
    assert_eq!(f.labels(7).unwrap(), ["tehas:ready", "bug"]);
    assert_eq!(
        asked.borrow()[0],
        ["api".to_string(), "repos/acme/widget/issues/7".to_string()]
    );
    // An entry of the list that names no label is no label and is dropped, as
    // the dispatcher's `snapshot::names` reads it: one odd entry of one issue
    // stops no other label of it.
    let (mut f, _) = forge(vec![Ok(
        r#"{"number":7,"labels":[{"name":"bug"},7,{"other":1}]}"#.to_string(),
    )]);
    assert_eq!(f.labels(7).unwrap(), ["bug"]);
    // An answer that names no labels at all is still no answer.
    let (mut f, _) = forge(vec![Ok(r#"{"number":7}"#.to_string())]);
    assert_eq!(f.labels(7).unwrap_err().call, "the issue");
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
    assert_eq!(
        f.approval(7).unwrap(),
        Some(Approval {
            actor: String::new(),
            at: String::new(),
        })
    );
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
    assert_eq!(f.approval(7).unwrap().unwrap().actor, "alice");
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
    assert_eq!(f.approval(7).unwrap(), None);
}

#[test]
fn an_event_whose_stamp_is_no_text_is_no_answer() {
    // A stamp that is there and is no text cannot be ordered: jq sorts a
    // number before every string, so reading it as no stamp would hand the
    // malformed entry a tie the reference reader gives to the stamped event.
    // The read says so rather than answering.
    let of = |stamp: Value| {
        page(&[
            serde_json::json!({
                "event": "labeled",
                "label": {"name": APPROVAL_LABEL},
                "actor": {"login": "mallory"},
                "created_at": stamp,
            }),
            event(
                "labeled",
                APPROVAL_LABEL,
                "alice",
                Some("2026-01-01T00:00:00Z"),
            ),
        ])
    };
    for stamp in [
        serde_json::json!(1),
        serde_json::json!({"$date": 1}),
        serde_json::json!([1]),
    ] {
        let (mut f, _) = forge(vec![Ok(of(stamp.clone()))]);
        let e = f
            .approval(7)
            .expect_err(&format!("{stamp} was read as a stamp"));
        assert_eq!(e.call, "the issue's label events");
        assert!(!e.why.is_empty(), "{stamp} answered no reason");
        let (mut f, _) = forge(vec![Ok(of(stamp.clone()))]);
        assert_eq!(
            f.label_state(7, APPROVAL_LABEL).unwrap_err().call,
            "the issue's label events"
        );
    }
    // An answer that says nothing of the time, or says it as JSON null, is the
    // site's own and reads: an unstamped event is the oldest of the page, so
    // the stamped one of alice answers, as `console label` reads it.
    let unstamped_of = |with_null: bool| {
        let mut mallory = serde_json::json!({
            "event": "labeled",
            "label": {"name": APPROVAL_LABEL},
            "actor": {"login": "mallory"},
        });
        if with_null {
            mallory["created_at"] = Value::Null;
        }
        page(&[
            mallory,
            event(
                "labeled",
                APPROVAL_LABEL,
                "alice",
                Some("2026-01-01T00:00:00Z"),
            ),
        ])
    };
    for with_null in [false, true] {
        let (mut f, _) = forge(vec![Ok(unstamped_of(with_null))]);
        assert_eq!(
            f.approval(7).unwrap().map(|a| a.actor),
            Some("alice".to_string()),
            "an absent stamp was read as the newest one"
        );
    }
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
    assert_eq!(f.approval(7).unwrap().unwrap().actor, "bob");
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
    // The first line that says anything is the one quoted, and within it the
    // LEFTMOST word of the list, as grep -oE and the dispatcher's regex
    // answer: the first entry of the list would report the plain limit for a
    // line whose only match is the secondary one later on.
    assert_eq!(gh::why("rate limit", Some(1)), ": rate limit");
    assert_eq!(
        gh::why("rate limit exceeded; see secondary rate limit", Some(1)),
        ": rate limit"
    );
    assert_eq!(
        gh::why("You have hit a secondary rate limit", Some(1)),
        ": secondary rate limit"
    );
    // Where two words start alike, the longer one wins, so a secondary limit
    // is never reported as a plain one.
    assert_eq!(
        gh::why("secondary rate limit", Some(1)),
        ": secondary rate limit"
    );
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
    assert_eq!(gh::why("gh: http 404x", Some(1)), ": HTTP 404");
    // The Kelvin sign lowercases to two bytes and the dotted capital I to
    // two as well, so the offsets of the search are the offsets of the
    // lower-cased line and not of the line gh printed.
    assert_eq!(gh::why("\u{212a} rate limit", Some(1)), ": rate limit");
    assert_eq!(gh::why("\u{0130}I timeout", Some(1)), ": timeout");
    assert_eq!(gh::why("\u{212a}HTTP 404", Some(1)), ": HTTP 404");
    // The words the bash matches, and the status whole: the digits of a
    // message that only happen to read 404 are no 404.
    assert!(gh::says_not_found(": Not Found (HTTP 404)"));
    assert!(gh::says_not_found(": HTTP 404"));
    assert!(!gh::says_not_found(": 404"));
    assert!(!gh::says_not_found(": HTTP 403"));
    assert!(!gh::says_not_found(": HTTP 502 with 404 in the body"));
    // The words of the list survive a line whose lower case is longer than
    // itself: lowercasing is not byte preserving for every scalar.
    assert_eq!(gh::why("\u{212a} rate limit", Some(1)), ": rate limit");
    assert_eq!(gh::why("\u{0130}I timeout", Some(1)), ": timeout");
    assert_eq!(gh::why("\u{212a}HTTP 404", Some(1)), ": HTTP 404");
}

#[test]
fn a_runner_that_cannot_be_run_names_the_program() {
    // A host whose gh is missing must say so, not answer that the issue
    // carries no labels.
    let (mut f, _) = forge(vec![Err(reason(": gh: no such file or directory"))]);
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
    assert!(f.approval(7).unwrap().is_none());
    let e = f.add_label(7, "tehas:running").unwrap_err();
    assert_eq!(e.call, "the label change");
    assert_eq!(e.why, "no fixture for call 1");
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
