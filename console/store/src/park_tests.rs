//! park, park-show, park-retry, question-asked, question-known and
//! question-answered.

use crate::verbs_tests::*;
use serde_json::{json, Value};
use std::path::Path;

fn park(
    home: &Path,
    issue: i64,
    attempt: i64,
    reason: &str,
    extra: &[&str],
) -> Result<Value, (u8, String)> {
    let (n, a) = (issue.to_string(), attempt.to_string());
    let mut args = vec!["park", &n, "--attempt", &a, "--reason", reason];
    args.extend(extra);
    call(home, &args)
}

fn known(home: &Path, issue: i64, id: i64) -> Value {
    ok(
        home,
        &[
            "question-known",
            &issue.to_string(),
            "--id",
            &id.to_string(),
        ],
    )
}

/// A started, ended and parked cycle.
fn parked(home: &Path, issue: i64, by: &str, how: &str, reason: &str) -> i64 {
    let (_l, a) = start_by(home, issue, by, how, NOW);
    end(home, issue, a, "failed");
    park(home, issue, a, reason, &[]).unwrap();
    a
}

/// A planning session in its second turn, waiting for an answer.
fn planned(home: &Path, issue: i64) {
    let brief = write_file(home, &format!("brief-{issue}.md"), b"brief");
    let n = issue.to_string();
    ok(
        home,
        &[
            "plan-begin",
            &n,
            "--by",
            "alice",
            "--label",
            "tehas:plan",
            "--brief-file",
            brief.to_str().unwrap(),
        ],
    );
    ok(
        home,
        &[
            "plan-end",
            &n,
            "--turn",
            "1",
            "--outcome",
            "finished",
            "--rc",
            "0",
            "--session",
            "s1",
        ],
    );
}

#[test]
fn park_records_the_park_and_its_question() {
    let home = home("park");
    let (_l, a) = start(&home, 10);
    let v = park(
        &home,
        10,
        a,
        "provider-outage",
        &[
            "--step",
            "review",
            "--pr",
            "12",
            "--comment-id",
            "5001",
            "--body-sha",
            SHA,
        ],
    )
    .unwrap();
    assert_eq!(v["issue"], 10);
    assert_eq!(v["question"], 5001);
    assert_eq!(v["cleaned"], json!([]));
    assert_eq!(
        v["park"],
        json!({"issue":10,"attempt":1,"at":NOW,"reason":"provider-outage","step":"review","pr":12,
               "retries":0,"started_at":NOW,"started_by":"alice","automatic":true,"question":5001})
    );
    assert_eq!(
        one::<String>(
            &home,
            "SELECT id || issue || attempt || kind || posted_at || body_sha256 FROM questions"
        ),
        format!("5001101park{NOW}{SHA}")
    );
    // The same three again change nothing.
    let again = park(
        &home,
        10,
        a,
        "provider-outage",
        &["--step", "review", "--pr", "12"],
    )
    .unwrap();
    assert_eq!(again["park"], v["park"]);
    assert_eq!(count(&home, "parks"), 1);
}

#[test]
fn park_cleans_its_reason_and_step() {
    let home = home("park-clean");
    let (_l, a) = start(&home, 11);
    let v = park(&home, 11, a, "cap hit!", &["--step", "a step"]).unwrap();
    assert_eq!(v["park"]["reason"], "caphit");
    assert_eq!(v["park"]["step"], "astep");
    assert_eq!(v["cleaned"], json!(["--reason", "--step"]));
    let (_l, b) = start(&home, 12);
    assert_eq!(
        park(&home, 12, b, "!!", &[]).unwrap()["park"]["reason"],
        "unknown"
    );
}

#[test]
fn park_states_its_preconditions_and_arguments() {
    let home = home("park-pre");
    assert_eq!(
        refused(&home, &["park", "13", "--attempt", "1", "--reason", "x"]),
        (1, "nocycle".into())
    );
    let (_l, a) = start(&home, 13);
    assert_eq!(park(&home, 13, a + 1, "x", &[]).unwrap_err().0, 1);
    assert_eq!(
        refused(&home, &["park", "13", "--attempt", "2", "--reason", "x"]),
        (1, "attempt".into())
    );
    park(&home, 13, a, "x", &[]).unwrap();
    assert_eq!(
        refused(&home, &["park", "13", "--attempt", "1", "--reason", "y"]),
        (1, "parked".into())
    );
    for args in [
        &["park", "13", "--attempt", "1"][..],
        &["park", "13", "--reason", "x"][..],
        &[
            "park",
            "13",
            "--attempt",
            "1",
            "--reason",
            "x",
            "--comment-id",
            "5",
        ][..],
        &[
            "park",
            "13",
            "--attempt",
            "1",
            "--reason",
            "x",
            "--body-sha",
            SHA,
        ][..],
        &["park", "13", "--attempt", "1", "--reason", "x", "--pr", "x"][..],
    ] {
        assert!(usage(args).starts_with("usage park: "), "{args:?}");
    }
}

#[test]
fn park_refuses_an_id_that_is_a_plan_question() {
    let home = home("park-taken");
    planned(&home, 14);
    ok(
        &home,
        &["question-asked", "14", "--id", "700", "--kind", "plan"],
    );
    let (_l, a) = start(&home, 14);
    let (code, word) = {
        let (n, at) = (14.to_string(), a.to_string());
        refused(
            &home,
            &[
                "park",
                &n,
                "--attempt",
                &at,
                "--reason",
                "x",
                "--comment-id",
                "700",
                "--body-sha",
                SHA,
            ],
        )
    };
    assert_eq!((code, word.as_str()), (1, "taken"));
    // Another issue's park question is taken too.
    let (_l2, b) = start(&home, 15);
    park(
        &home,
        15,
        b,
        "x",
        &["--comment-id", "800", "--body-sha", SHA],
    )
    .unwrap();
    let (_l3, c) = start(&home, 16);
    assert_eq!(
        park(
            &home,
            16,
            c,
            "x",
            &["--comment-id", "800", "--body-sha", SHA]
        )
        .unwrap_err()
        .0,
        1
    );
    assert_eq!(count(&home, "parks WHERE issue IN (14, 16)"), 0);
}

#[test]
fn park_show_with_nothing_is_null_and_0() {
    let home = home("park-show-null");
    let v = ok(&home, &["park-show", "17"]);
    assert_eq!(v, json!({"api":1,"verb":"park-show","ok":true,"park":null}));
    assert_eq!(ok(&home, &["park-show", "--automatic"])["parks"], json!([]));
    for args in [&["park-show"][..], &["park-show", "1", "--automatic"][..]] {
        assert!(usage(args).starts_with("usage park-show: "), "{args:?}");
    }
}

#[test]
fn park_show_names_the_account_that_started_the_parked_cycle() {
    let home = home("park-f6");
    parked(&home, 18, "alice", "label", "provider-outage");
    approve(&home, 18, "bob", "label");
    let v = ok(&home, &["park-show", "18"]);
    assert_eq!(v["park"]["started_by"], "alice");
    assert_eq!(v["park"]["automatic"], true);
}

#[test]
fn park_retry_spends_the_automatic_retry_once() {
    let home = home("park-retry");
    parked(&home, 19, "alice", "label", "provider-outage");
    let v = ok(&home, &["park-retry", "19"]);
    assert_eq!(
        (v["retries"].clone(), v["automatic"].clone()),
        (json!(1), json!(true))
    );
    let v = ok(&home, &["park-retry", "19"]);
    assert_eq!(
        (v["retries"].clone(), v["automatic"].clone()),
        (json!(2), json!(false))
    );
    assert_eq!(ok(&home, &["park-show", "19"])["park"]["automatic"], false);
    assert_eq!(
        refused(&home, &["park-retry", "20"]),
        (1, "unparked".into())
    );
    assert!(usage(&["park-retry"]).starts_with("usage park-retry: "));
}

#[test]
fn a_requeued_cycle_s_outage_park_owes_no_automatic_retry() {
    let home = home("park-requeued");
    parked(&home, 21, "alice", "outage-retry", "provider-outage");
    assert_eq!(ok(&home, &["park-show", "21"])["park"]["automatic"], false);
}

#[test]
fn a_run_by_hand_owes_no_automatic_retry() {
    let home = home("park-hand");
    let _l = hold(&home, 22);
    ok(&home, &["cycle-start", "22", "--by-hand"]);
    end(&home, 22, 1, "failed");
    park(&home, 22, 1, "provider-outage", &[]).unwrap();
    let v = ok(&home, &["park-show", "22"]);
    assert_eq!(v["park"]["automatic"], false);
    assert_eq!(v["park"]["started_by"], Value::Null);
}

#[test]
fn park_show_automatic_lists_only_owed_retries() {
    let home = home("park-auto");
    parked(&home, 23, "alice", "label", "provider-outage");
    parked(&home, 24, "alice", "label", "failed");
    parked(&home, 25, "alice", "label", "provider-outage");
    ok(&home, &["park-retry", "25"]);
    parked(&home, 26, "alice", "label", "provider-outage");
    // A new start leaves the old park behind.
    let (_l, _) = start(&home, 26);
    let v = ok(&home, &["park-show", "--automatic"]);
    let issues: Vec<i64> = v["parks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["issue"].as_i64().unwrap())
        .collect();
    assert_eq!(issues, vec![23]);
    assert_eq!(ok(&home, &["park-show", "26"])["park"], Value::Null);
    assert_eq!(
        refused(&home, &["park-retry", "26"]),
        (1, "unparked".into())
    );
}

#[test]
fn question_asked_records_park_and_plan_questions() {
    let home = home("asked");
    let a = parked(&home, 30, "alice", "label", "failed");
    let v = ok(
        &home,
        &[
            "question-asked",
            "30",
            "--id",
            "900",
            "--kind",
            "park",
            "--body-sha",
            SHA,
        ],
    );
    assert_eq!(
        v["question"],
        json!({"id":900,"issue":30,"attempt":a,"kind":"park","posted_at":NOW,"body_sha256":SHA,
               "answered_at":null,"answered_by":null})
    );
    assert_eq!(ok(&home, &["park-show", "30"])["park"]["question"], 900);
    // The same again is unchanged.
    let again = ok_at(
        &home,
        NOW + 9,
        &["question-asked", "30", "--id", "900", "--kind", "park"],
    );
    assert_eq!(again["question"], v["question"]);
    planned(&home, 31);
    let v = ok(
        &home,
        &[
            "question-asked",
            "31",
            "--id",
            "901",
            "--kind",
            "plan",
            "--attempt",
            "1",
        ],
    );
    assert_eq!(v["question"]["attempt"], 1);
    assert_eq!(
        ok(&home, &["plan-show", "31"])["planning"]["comment_id"],
        901
    );
}

#[test]
fn question_asked_states_its_preconditions_and_arguments() {
    let home = home("asked-pre");
    assert_eq!(
        refused(
            &home,
            &["question-asked", "32", "--id", "1", "--kind", "park"]
        ),
        (1, "unparked".into())
    );
    assert_eq!(
        refused(
            &home,
            &["question-asked", "32", "--id", "1", "--kind", "plan"]
        ),
        (1, "noplan".into())
    );
    parked(&home, 32, "alice", "label", "failed");
    assert_eq!(
        refused(
            &home,
            &[
                "question-asked",
                "32",
                "--id",
                "1",
                "--kind",
                "park",
                "--attempt",
                "2"
            ]
        ),
        (1, "attempt".into())
    );
    planned(&home, 33);
    ok(
        &home,
        &["question-asked", "33", "--id", "2", "--kind", "plan"],
    );
    assert_eq!(
        refused(
            &home,
            &["question-asked", "32", "--id", "2", "--kind", "park"]
        ),
        (1, "taken".into())
    );
    planned(&home, 32);
    ok(
        &home,
        &["question-asked", "32", "--id", "3", "--kind", "park"],
    );
    assert_eq!(
        refused(
            &home,
            &["question-asked", "32", "--id", "3", "--kind", "plan"]
        ),
        (1, "taken".into())
    );
    for args in [
        &["question-asked", "32", "--id", "1"][..],
        &["question-asked", "32", "--kind", "park"][..],
        &["question-asked", "32", "--id", "x", "--kind", "park"][..],
        &[
            "question-asked",
            "32",
            "--id",
            "1",
            "--kind",
            "park",
            "--body-sha",
            "x",
        ][..],
    ] {
        assert!(
            usage(args).starts_with("usage question-asked: "),
            "{args:?}"
        );
    }
}

#[test]
fn question_known_is_true_only_for_the_question_on_record() {
    let home = home("known");
    parked(&home, 40, "alice", "label", "failed");
    ok(
        &home,
        &["question-asked", "40", "--id", "41", "--kind", "park"],
    );
    assert_eq!(
        known(&home, 40, 41),
        json!({"api":1,"verb":"question-known","ok":true,"known":true,"kind":"park","recorded":true,"answered":false})
    );
    planned(&home, 42);
    ok(
        &home,
        &["question-asked", "42", "--id", "43", "--kind", "plan"],
    );
    assert_eq!(known(&home, 42, 43)["known"], true);
    assert_eq!(known(&home, 42, 43)["kind"], "plan");
    assert!(usage(&["question-known", "42"]).starts_with("usage question-known: "));
}

#[test]
fn question_known_is_false_for_an_id_never_asked() {
    let home = home("known-never");
    parked(&home, 44, "alice", "label", "failed");
    let v = known(&home, 44, 45);
    assert_eq!(
        (v["known"].clone(), v["recorded"].clone(), v["kind"].clone()),
        (json!(false), json!(false), Value::Null)
    );
}

#[test]
fn question_known_is_false_for_an_id_of_another_issue() {
    let home = home("known-other");
    parked(&home, 46, "alice", "label", "failed");
    ok(
        &home,
        &["question-asked", "46", "--id", "47", "--kind", "park"],
    );
    parked(&home, 48, "alice", "label", "failed");
    let v = known(&home, 48, 47);
    assert_eq!(
        (v["known"].clone(), v["recorded"].clone()),
        (json!(false), json!(false))
    );
}

#[test]
fn question_known_is_false_for_an_answered_question() {
    let home = home("known-answered");
    parked(&home, 49, "alice", "label", "failed");
    ok(
        &home,
        &["question-asked", "49", "--id", "50", "--kind", "park"],
    );
    let v = ok(
        &home,
        &["question-answered", "49", "--id", "50", "--by", "bob"],
    );
    assert_eq!(v["question"]["answered_by"], "bob");
    assert_eq!(v["question"]["answered_at"], NOW);
    let v = known(&home, 49, 50);
    assert_eq!(
        (
            v["known"].clone(),
            v["recorded"].clone(),
            v["answered"].clone()
        ),
        (json!(false), json!(true), json!(true))
    );
    // A second answer changes nothing.
    let v = ok_at(
        &home,
        NOW + 5,
        &["question-answered", "49", "--id", "50", "--by", "carol"],
    );
    assert_eq!(v["question"]["answered_by"], "bob");
}

#[test]
fn a_park_question_is_not_known_after_a_new_cycle_start() {
    let home = home("known-new-start");
    parked(&home, 51, "alice", "label", "failed");
    ok(
        &home,
        &["question-asked", "51", "--id", "52", "--kind", "park"],
    );
    let (_l, _) = start(&home, 51);
    let v = known(&home, 51, 52);
    assert_eq!(
        (v["known"].clone(), v["recorded"].clone()),
        (json!(false), json!(true))
    );
}

#[test]
fn a_withdrawn_question_is_not_known() {
    let home = home("known-withdrawn");
    parked(&home, 53, "alice", "label", "failed");
    ok(
        &home,
        &["question-asked", "53", "--id", "54", "--kind", "park"],
    );
    assert_eq!(
        ok(
            &home,
            &["question-answered", "53", "--id", "54", "--withdraw"]
        )["withdrawn"],
        true
    );
    let v = known(&home, 53, 54);
    assert_eq!(
        (v["known"].clone(), v["recorded"].clone()),
        (json!(false), json!(false))
    );
    // A withdrawn plan question takes the session's comment id with it.
    planned(&home, 55);
    ok(
        &home,
        &["question-asked", "55", "--id", "56", "--kind", "plan"],
    );
    ok(
        &home,
        &["question-answered", "55", "--id", "56", "--withdraw"],
    );
    assert_eq!(
        ok(&home, &["plan-show", "55"])["planning"]["comment_id"],
        Value::Null
    );
    assert_eq!(known(&home, 55, 56)["known"], false);
}

#[test]
fn question_answered_states_its_preconditions_and_arguments() {
    let home = home("answered-pre");
    assert_eq!(
        refused(
            &home,
            &["question-answered", "57", "--id", "58", "--by", "bob"]
        ),
        (1, "unknown".into())
    );
    parked(&home, 57, "alice", "label", "failed");
    ok(
        &home,
        &["question-asked", "57", "--id", "58", "--kind", "park"],
    );
    assert_eq!(
        refused(
            &home,
            &["question-answered", "59", "--id", "58", "--withdraw"]
        ),
        (1, "unknown".into())
    );
    for args in [
        &["question-answered", "57", "--id", "58"][..],
        &[
            "question-answered",
            "57",
            "--id",
            "58",
            "--by",
            "bob",
            "--withdraw",
        ][..],
        &["question-answered", "57", "--by", "bob"][..],
        &["question-answered", "57", "--id", "58", "--by", "bad_name"][..],
    ] {
        assert!(
            usage(args).starts_with("usage question-answered: "),
            "{args:?}"
        );
    }
}

#[test]
fn a_park_replayed_with_a_question_records_it() {
    let home = home("park-replay");
    let (_l, a) = start(&home, 17);
    let q = ["--comment-id", "5101", "--body-sha", SHA];
    assert_eq!(
        park(&home, 17, a, "provider-outage", &[]).unwrap()["question"],
        Value::Null
    );
    let v = park(&home, 17, a, "provider-outage", &q).unwrap();
    assert_eq!(v["question"], 5101);
    assert_eq!(v["park"]["question"], 5101);
    assert_eq!(known(&home, 17, 5101)["known"], true);
    // A replay after a lost answer, with or without the id, names it again.
    assert_eq!(
        park(&home, 17, a, "provider-outage", &q).unwrap()["question"],
        5101
    );
    assert_eq!(
        park(&home, 17, a, "provider-outage", &[]).unwrap()["question"],
        5101
    );
    assert_eq!(count(&home, "questions"), 1);
    assert_eq!(count(&home, "parks"), 1);
}

#[test]
fn park_refuses_an_id_of_an_older_attempt() {
    let home = home("park-older");
    let q = ["--comment-id", "5201", "--body-sha", SHA];
    let (l, a) = start(&home, 18);
    end(&home, 18, a, "failed");
    park(&home, 18, a, "failed", &q).unwrap();
    drop(l);
    let (_l, b) = start(&home, 18);
    end(&home, 18, b, "failed");
    let (code, line) = park(&home, 18, b, "failed", &q).unwrap_err();
    assert_eq!(code, 1);
    assert!(line.starts_with("attempt #18: "), "{line}");
    assert_eq!(count(&home, &format!("parks WHERE attempt = {b}")), 0);
    assert_eq!(
        one::<i64>(&home, "SELECT attempt FROM questions WHERE id = 5201"),
        a
    );
}

#[test]
fn only_the_newest_park_question_is_on_record() {
    let home = home("park-newest");
    parked(&home, 70, "alice", "label", "failed");
    ok_at(
        &home,
        NOW,
        &["question-asked", "70", "--id", "100", "--kind", "park"],
    );
    ok_at(
        &home,
        NOW + 1,
        &["question-asked", "70", "--id", "101", "--kind", "park"],
    );
    assert_eq!(ok(&home, &["park-show", "70"])["park"]["question"], 101);
    assert_eq!(known(&home, 70, 101)["known"], true);
    let v = known(&home, 70, 100);
    assert_eq!(
        (v["known"].clone(), v["recorded"].clone()),
        (json!(false), json!(true))
    );
}
