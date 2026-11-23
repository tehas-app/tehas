//! answer-record and answer-drop, and the answer question-known gives.

use crate::verbs_tests::*;
use serde_json::{json, Value};
use std::path::Path;

/// The answer question-known gives for question `id` of the issue, as JSON
/// text.
fn known(home: &Path, issue: i64, id: i64) -> String {
    let (n, id) = (issue.to_string(), id.to_string());
    ok(home, &["question-known", &n, "--id", &id])["answer"].to_string()
}

/// #40 parked, its question 41 on record.
fn parked(home: &Path) {
    parked_as(home, "41");
}

/// #40 parked, the question of comment `comment` on record.
fn parked_as(home: &Path, comment: &str) {
    let (_l, a) = start_by(home, 40, "alice", "approved", NOW);
    end(home, 40, a, "failed");
    let a = a.to_string();
    ok(
        home,
        &[
            "park",
            "40",
            "--attempt",
            &a,
            "--reason",
            "failed",
            "--comment-id",
            comment,
            "--body-sha",
            &"a".repeat(64),
        ],
    );
}

/// #42 in planning, its question 43 waiting with the options given.
fn planning(home: &Path, options: &[&str]) {
    crate::park_tests::planned_with(home, 42, options);
    ok(
        home,
        &["question-asked", "42", "--id", "43", "--kind", "plan"],
    );
}

/// The arguments of an answer-record of the issue for bob.
fn answer<'a>(issue: &'a str, extra: &[&'a str]) -> Vec<&'a str> {
    let mut args = vec!["answer-record", issue, "--by", "bob"];
    args.extend(extra);
    args
}

#[test]
fn a_park_question_takes_a_retry_and_question_known_gives_it() {
    let home = home("answer-park");
    parked(&home);
    let v = call_at(
        &home,
        NOW + 60,
        &["answer-record", "40", "--by", "bob", "--choice", "retry"],
    )
    .unwrap();
    assert_eq!(
        v["answer"],
        json!({"question": 1, "comment": 41, "issue": 40, "kind": "park", "choice": "retry",
            "option": null, "by": "bob", "at": NOW + 60, "text": null, "how": "answer"})
    );
    assert_eq!(v["replaced"], false);
    assert_eq!(known(&home, 40, 41), v["answer"].to_string());
    // park-show gives it with the park, for a poll that reads no question.
    let park = ok(&home, &["park-show", "40"]);
    assert_eq!(park["park"]["answer"], v["answer"]);
    // A second answer takes the place of the first.
    let v = call(&home, &answer("40", &["--choice", "park"])).unwrap();
    assert_eq!(v["replaced"], true);
    assert!(known(&home, 40, 41).contains(r#""choice":"park""#));
    assert_eq!(count(&home, "answers"), 1);
}

#[test]
fn an_answer_needs_the_question_of_its_kind() {
    let home = home("answer-unasked");
    assert_eq!(
        refused(
            &home,
            &["answer-record", "40", "--by", "bob", "--choice", "retry"]
        ),
        (1, "unasked".into())
    );
    parked(&home);
    for c in [
        &["--choice", "end"][..],
        &["--choice", "option", "--option", "1"],
    ] {
        assert_eq!(call(&home, &answer("40", c)).unwrap_err().0, 1, "{c:?}");
    }
    planning(&home, &["Yes"]);
    assert_eq!(
        refused(
            &home,
            &["answer-record", "42", "--by", "bob", "--choice", "retry"]
        ),
        (1, "unasked".into())
    );
    assert_eq!(count(&home, "answers"), 0);
}

#[test]
fn a_planning_question_takes_an_option_update_end_or_words() {
    let home = home("answer-plan");
    planning(&home, &["Adopt all", "Update this issue"]);
    let v = call(
        &home,
        &answer("42", &["--choice", "option", "--option", "1"]),
    )
    .unwrap();
    assert_eq!(v["answer"]["option"], 1);
    assert_eq!(v["answer"]["kind"], "plan");
    // update is kept as the number of the option that says it.
    let v = call(&home, &answer("42", &["--choice", "update"])).unwrap();
    assert_eq!(
        (v["answer"]["choice"].clone(), v["answer"]["option"].clone()),
        (json!("update"), json!(2))
    );
    let v = call(&home, &answer("42", &["--choice", "end"])).unwrap();
    assert_eq!(v["answer"]["option"], Value::Null);
    let file = write_file(&home, "reply.md", b"Split it in two.\n");
    let text = ["--choice", "text", "--text-file", file.to_str().unwrap()];
    let v = call(&home, &answer("42", &text)).unwrap();
    assert_eq!(v["answer"]["text"], "Split it in two.\n");
    assert!(known(&home, 42, 43).contains(r#""choice":"text""#));
}

#[test]
fn an_option_the_question_does_not_offer_is_refused() {
    let home = home("answer-unoffered");
    planning(&home, &["Adopt all"]);
    assert_eq!(
        refused(
            &home,
            &[
                "answer-record",
                "42",
                "--by",
                "bob",
                "--choice",
                "option",
                "--option",
                "2"
            ]
        ),
        (1, "unoffered".into())
    );
    assert_eq!(
        refused(
            &home,
            &["answer-record", "42", "--by", "bob", "--choice", "update"]
        ),
        (1, "unoffered".into())
    );
    let empty = write_file(&home, "empty.md", b" \n");
    assert_eq!(
        refused(
            &home,
            &[
                "answer-record",
                "42",
                "--by",
                "bob",
                "--choice",
                "text",
                "--text-file",
                empty.to_str().unwrap()
            ]
        ),
        (1, "answer".into())
    );
    assert_eq!(count(&home, "answers"), 0);
}

/// A withdrawn question keeps its row and its answer (schema 30), and neither
/// is on record: the answer is given no more, and a new question takes the
/// next number.
#[test]
fn the_answer_goes_with_its_question() {
    let home = home("answer-gone");
    parked(&home);
    call(&home, &answer("40", &["--choice", "retry"])).unwrap();
    ok(
        &home,
        &["question-answered", "40", "--id", "41", "--withdraw"],
    );
    assert_eq!(count(&home, "answers"), 1);
    assert_eq!(count(&home, "questions WHERE withdrawn_at IS NOT NULL"), 1);
    assert_eq!(known(&home, 40, 41), "null");
    assert_eq!(
        ok(&home, &["park-show", "40"])["park"]["answer"],
        Value::Null
    );
    // An answer to a question no longer on record is not given.
    parked_as(&home, "44");
    let v = call(&home, &answer("40", &["--choice", "retry"])).unwrap();
    assert_eq!(
        (
            v["answer"]["question"].clone(),
            v["answer"]["comment"].clone()
        ),
        (json!(2), json!(44))
    );
    ok(
        &home,
        &["question-answered", "40", "--id", "44", "--by", "alice"],
    );
    assert_eq!(known(&home, 40, 44), "null");
}

#[test]
fn answer_drop_takes_the_answer_away_once() {
    let home = home("answer-drop");
    parked(&home);
    call(&home, &answer("40", &["--choice", "retry-text"])).unwrap();
    assert_eq!(
        ok(&home, &["answer-drop", "40", "--id", "41"])["dropped"],
        true
    );
    assert_eq!(
        ok(&home, &["answer-drop", "40", "--id", "41"])["dropped"],
        false
    );
    assert_eq!(known(&home, 40, 41), "null");
}

#[test]
fn answer_record_refuses_what_it_cannot_take() {
    for args in [
        &["answer-record", "40", "--choice", "retry"][..],
        &["answer-record", "40", "--by", "bob"],
        &["answer-record", "40", "--by", "b o b", "--choice", "retry"],
        &["answer-record", "40", "--by", "bob", "--choice", "tick"],
        &["answer-record", "40", "--by", "bob", "--choice", "option"],
        &[
            "answer-record",
            "40",
            "--by",
            "bob",
            "--choice",
            "option",
            "--option",
            "0",
        ],
        &[
            "answer-record",
            "40",
            "--by",
            "bob",
            "--choice",
            "option",
            "--option",
            "21",
        ],
        &[
            "answer-record",
            "40",
            "--by",
            "bob",
            "--choice",
            "end",
            "--option",
            "1",
        ],
        &["answer-record", "40", "--by", "bob", "--choice", "text"],
        &[
            "answer-record",
            "40",
            "--by",
            "bob",
            "--choice",
            "retry",
            "--text-file",
            "/x",
        ],
        &[
            "answer-record",
            "40",
            "--by",
            "bob",
            "--choice",
            "text",
            "--text-file",
            "x",
        ],
    ] {
        assert!(usage(args).starts_with("usage answer-record: "), "{args:?}");
    }
    assert!(usage(&["answer-drop", "40"]).starts_with("usage answer-drop: "));
}
