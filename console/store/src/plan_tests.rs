//! plan-begin, plan-answer, plan-end and plan-show.

use crate::verbs_tests::*;
use serde_json::{json, Value};
use std::fs::File;
use std::path::Path;

fn brief(home: &Path, text: &str) -> String {
    write_file(home, "brief.md", text.as_bytes())
        .to_str()
        .unwrap()
        .to_string()
}

fn begin(home: &Path, issue: i64) -> Value {
    let b = brief(home, "the brief");
    ok(
        home,
        &[
            "plan-begin",
            &issue.to_string(),
            "--by",
            "alice",
            "--label",
            "tehas:plan",
            "--brief-file",
            &b,
        ],
    )
}

fn turn_end(home: &Path, issue: i64, turn: i64, extra: &[&str]) -> Result<Value, (u8, String)> {
    let (n, t) = (issue.to_string(), turn.to_string());
    let mut args = vec!["plan-end", &n, "--turn", &t];
    args.extend(extra);
    call(home, &args)
}

fn finished(home: &Path, issue: i64, turn: i64) -> Value {
    turn_end(
        home,
        issue,
        turn,
        &["--outcome", "finished", "--rc", "0", "--session", "s1"],
    )
    .unwrap()
}

fn answer(home: &Path, issue: i64, turn: i64, by: &str) -> Result<Value, (u8, String)> {
    call(
        home,
        &[
            "plan-answer",
            &issue.to_string(),
            "--turn",
            &turn.to_string(),
            "--by",
            by,
        ],
    )
}

fn show(home: &Path, issue: i64) -> Value {
    ok(home, &["plan-show", &issue.to_string()])["planning"].clone()
}

#[test]
fn plan_begin_opens_a_running_first_turn() {
    let home = home("plan-begin");
    let v = begin(&home, 10);
    assert_eq!(v["replaced"], "none");
    assert_eq!(
        v["planning"],
        json!({"issue":10,"session":null,"turns":0,"options":[],"pending":false,"pending_outcome":null,
               "failed":false,"running":true,"started_at":NOW,"comment_id":null,"lost":false,
               "by":"alice","label":"tehas:plan","brief":"the brief","turn_rc":null,"opened_at":NOW,
               "ended_at":null,"auto":false,"answers":[],"filed":[],"answer":null,
               "notes":{"fixer_refused":[],"held":[]}})
    );
    assert_eq!(count(&home, "issues WHERE issue = 10"), 1);
}

#[test]
fn plan_begin_states_its_preconditions_and_arguments() {
    let home = home("plan-begin-pre");
    let b = brief(&home, "x");
    for args in [
        &["plan-begin", "11", "--by", "alice", "--label", "tehas:plan"][..],
        &["plan-begin", "11", "--by", "alice", "--brief-file", &b][..],
        &[
            "plan-begin",
            "11",
            "--label",
            "tehas:plan",
            "--brief-file",
            &b,
        ][..],
        &[
            "plan-begin",
            "11",
            "--by",
            "alice",
            "--label",
            "tehas:plan",
            "--brief-file",
            "brief.md",
        ][..],
        &["plan-begin", "11", "--undo", "--by", "alice"][..],
    ] {
        assert!(usage(args).starts_with("usage plan-begin: "), "{args:?}");
    }
    let link = home.join("brief-link.md");
    std::os::unix::fs::symlink(&b, &link).unwrap();
    let big = home.join("big.md");
    File::create(&big).unwrap().set_len((1 << 20) + 1).unwrap();
    let bad = write_file(&home, "bad.md", &[0xff, 0xfe]);
    for f in [link, big, bad, home.join("none.md")] {
        assert_eq!(
            refused(
                &home,
                &[
                    "plan-begin",
                    "11",
                    "--by",
                    "a",
                    "--label",
                    "tehas:plan",
                    "--brief-file",
                    f.to_str().unwrap()
                ]
            ),
            (1, "brief".into())
        );
    }
    assert_eq!(count(&home, "planning"), 0);
}

#[test]
fn plan_begin_refuses_a_running_session() {
    let home = home("plan-running");
    begin(&home, 12);
    assert_eq!(
        refused(
            &home,
            &[
                "plan-begin",
                "12",
                "--by",
                "bob",
                "--label",
                "tehas:refine",
                "--brief-file",
                &brief(&home, "y")
            ]
        ),
        (1, "running".into())
    );
    assert_eq!(show(&home, 12)["by"], "alice");
}

#[test]
fn plan_begin_refuses_a_lost_session_not_yet_closed() {
    let home = home("plan-lost");
    begin(&home, 13);
    turn_end(&home, 13, 1, &["--outcome", "lost"]).unwrap();
    assert_eq!(
        refused(
            &home,
            &[
                "plan-begin",
                "13",
                "--by",
                "bob",
                "--label",
                "tehas:plan",
                "--brief-file",
                &brief(&home, "y")
            ]
        ),
        (1, "running".into())
    );
    ok(&home, &["plan-end", "13", "--close"]);
    assert_eq!(begin(&home, 13)["replaced"], "ended");
}

#[test]
fn plan_begin_replaces_an_ended_session_and_its_children() {
    let home = home("plan-replace-ended");
    begin(&home, 14);
    turn_end(
        &home,
        14,
        1,
        &[
            "--outcome",
            "finished",
            "--rc",
            "0",
            "--session",
            "s1",
            "--filed",
            "https://github.com/o/r/issues/5",
        ],
    )
    .unwrap();
    answer(&home, 14, 2, "bob").unwrap();
    ok(&home, &["plan-end", "14", "--close"]);
    let v = begin(&home, 14);
    assert_eq!(v["replaced"], "ended");
    assert_eq!(v["planning"]["answers"], json!([]));
    assert_eq!(v["planning"]["filed"], json!([]));
    assert_eq!(count(&home, "planning_answers"), 0);
    assert_eq!(count(&home, "planning_filed"), 0);
}

#[test]
fn plan_begin_replaces_an_open_session_that_is_not_running() {
    let home = home("plan-replace-open");
    begin(&home, 15);
    finished(&home, 15, 1);
    ok(
        &home,
        &["question-asked", "15", "--id", "150", "--kind", "plan"],
    );
    // A park question of the issue is withdrawn too.
    let (_l, a) = start(&home, 15);
    end(&home, 15, a, "failed");
    ok(
        &home,
        &[
            "park",
            "15",
            "--attempt",
            &a.to_string(),
            "--reason",
            "x",
            "--comment-id",
            "151",
            "--body-sha",
            SHA,
        ],
    );
    let v = begin(&home, 15);
    assert_eq!(v["replaced"], "open");
    assert_eq!(v["planning"]["session"], Value::Null);
    assert_eq!(count(&home, "questions"), 0);
}

#[test]
fn plan_begin_undo_removes_only_a_first_turn() {
    let home = home("plan-begin-undo");
    assert_eq!(
        refused(&home, &["plan-begin", "16", "--undo"]),
        (1, "noplan".into())
    );
    begin(&home, 16);
    assert_eq!(ok(&home, &["plan-begin", "16", "--undo"])["undone"], true);
    assert_eq!(count(&home, "planning"), 0);
    begin(&home, 16);
    finished(&home, 16, 1);
    assert_eq!(
        refused(&home, &["plan-begin", "16", "--undo"]),
        (1, "turn".into())
    );
}

#[test]
fn plan_answer_keeps_the_session_and_brief() {
    let home = home("plan-answer");
    begin(&home, 17);
    finished(&home, 17, 1);
    let v = ok_at(
        &home,
        NOW + 30,
        &["plan-answer", "17", "--turn", "2", "--by", "bob", "--fixer"],
    );
    assert_eq!(v["turn"], 2);
    let p = &v["planning"];
    assert_eq!(p["session"], "s1");
    assert_eq!(p["brief"], "the brief");
    assert_eq!(p["running"], true);
    assert_eq!(p["pending"], false);
    assert_eq!(p["started_at"], NOW + 30);
    assert_eq!(p["answers"], json!([{"turn":2,"by":"bob","fixer":true}]));
}

#[test]
fn plan_answer_closes_the_question_it_answers() {
    let home = home("plan-answer-q");
    begin(&home, 18);
    finished(&home, 18, 1);
    ok(
        &home,
        &["question-asked", "18", "--id", "180", "--kind", "plan"],
    );
    ok_at(
        &home,
        NOW + 5,
        &["plan-answer", "18", "--turn", "2", "--by", "bob"],
    );
    assert_eq!(
        one::<String>(
            &home,
            "SELECT answered_at || answered_by FROM questions WHERE id = 180"
        ),
        format!("{}bob", NOW + 5)
    );
    assert_eq!(show(&home, 18)["comment_id"], Value::Null);
    assert_eq!(
        ok(&home, &["question-known", "18", "--id", "180"])["known"],
        false
    );
}

#[test]
fn plan_answer_undo_reopens_the_question_it_closed() {
    let home = home("plan-answer-undo");
    begin(&home, 19);
    finished(&home, 19, 1);
    ok(
        &home,
        &["question-asked", "19", "--id", "190", "--kind", "plan"],
    );
    ok_at(
        &home,
        NOW + 5,
        &["plan-answer", "19", "--turn", "2", "--by", "bob"],
    );
    let v = ok(&home, &["plan-answer", "19", "--turn", "2", "--undo"]);
    assert_eq!(v["planning"]["running"], false);
    assert_eq!(v["planning"]["comment_id"], 190);
    assert_eq!(v["planning"]["answers"], json!([]));
    assert_eq!(
        ok(&home, &["question-known", "19", "--id", "190"])["known"],
        true
    );
    assert_eq!(
        refused(&home, &["plan-answer", "19", "--turn", "2", "--undo"]),
        (1, "turn".into())
    );
}

#[test]
fn plan_answer_undo_never_reopens_an_older_question() {
    let home = home("plan-answer-older");
    begin(&home, 20);
    finished(&home, 20, 1);
    ok(
        &home,
        &["question-asked", "20", "--id", "200", "--kind", "plan"],
    );
    ok_at(
        &home,
        NOW + 5,
        &["plan-answer", "20", "--turn", "2", "--by", "bob"],
    );
    // Turn 2 fails with no question posted; turn 3 is answered by the same
    // person, and its undo must not reopen turn 1's question.
    turn_end(&home, 20, 2, &["--outcome", "failed", "--rc", "1"]).unwrap();
    ok_at(
        &home,
        NOW + 9,
        &["plan-answer", "20", "--turn", "3", "--by", "bob"],
    );
    let v = ok(&home, &["plan-answer", "20", "--turn", "3", "--undo"]);
    assert_eq!(v["planning"]["comment_id"], Value::Null);
    assert_eq!(
        one::<String>(&home, "SELECT answered_by FROM questions WHERE id = 200"),
        "bob"
    );
}

#[test]
fn plan_answer_states_its_preconditions_and_arguments() {
    let home = home("plan-answer-pre");
    assert_eq!(answer(&home, 21, 2, "bob").unwrap_err().0, 1);
    assert_eq!(
        refused(&home, &["plan-answer", "21", "--turn", "2", "--by", "bob"]),
        (1, "noplan".into())
    );
    begin(&home, 21);
    assert_eq!(
        refused(&home, &["plan-answer", "21", "--turn", "1", "--by", "bob"]),
        (1, "turn".into())
    );
    // A finished turn without a session leaves the record ended: no answer.
    turn_end(&home, 21, 1, &["--outcome", "interrupted"]).unwrap();
    assert_eq!(
        refused(&home, &["plan-answer", "21", "--turn", "2", "--by", "bob"]),
        (1, "noplan".into())
    );
    begin(&home, 22);
    finished(&home, 22, 1);
    assert_eq!(
        refused(&home, &["plan-answer", "22", "--turn", "3", "--by", "bob"]),
        (1, "turn".into())
    );
    answer(&home, 22, 2, "bob").unwrap();
    assert_eq!(
        refused(&home, &["plan-answer", "22", "--turn", "2", "--by", "bob"]),
        (1, "turn".into())
    );
    turn_end(&home, 22, 2, &["--outcome", "lost"]).unwrap();
    assert_eq!(
        refused(&home, &["plan-answer", "22", "--turn", "3", "--by", "bob"]),
        (1, "turn".into())
    );
    for args in [
        &["plan-answer", "22", "--by", "bob"][..],
        &["plan-answer", "22", "--turn", "2"][..],
        &["plan-answer", "22", "--turn", "2", "--undo", "--by", "bob"][..],
        &["plan-answer", "22", "--turn", "2", "--undo", "--fixer"][..],
    ] {
        assert!(usage(args).starts_with("usage plan-answer: "), "{args:?}");
    }
}

#[test]
fn a_finished_turn_records_its_session_options_filed_and_outcome_file() {
    let home = home("plan-end");
    begin(&home, 23);
    let out = home.join("state").join("plan-23.json.out");
    let v = turn_end(
        &home,
        23,
        1,
        &[
            "--outcome",
            "finished",
            "--rc",
            "0",
            "--session",
            "s-23",
            "--option",
            "Yes",
            "--option",
            "No",
            "--filed",
            "https://github.com/o/r/issues/7",
            "--out",
            out.to_str().unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(v["outcome"], "finished");
    assert_eq!(v["cleaned"], json!([]));
    let p = &v["planning"];
    assert_eq!(p["session"], "s-23");
    assert_eq!(p["turns"], 1);
    assert_eq!(p["options"], json!(["Yes", "No"]));
    assert_eq!(p["pending"], true);
    assert_eq!(p["pending_outcome"], "state/plan-23.json.out");
    assert_eq!(p["running"], false);
    assert_eq!(p["failed"], false);
    assert_eq!(p["turn_rc"], 0);
    assert_eq!(p["filed"], json!(["https://github.com/o/r/issues/7"]));
    let h: (i64, i64, String, String, i64, i64, String, String, String) = conn(&home)
        .query_row(
            "SELECT id, turn, outcome, session, started_at, rc, by_account, label, filed_json FROM plan_history",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?)),
        )
        .unwrap();
    assert_eq!(v["history_id"], h.0);
    assert_eq!(
        (
            h.1,
            h.2.as_str(),
            h.3.as_str(),
            h.4,
            h.5,
            h.6.as_str(),
            h.7.as_str(),
            h.8.as_str()
        ),
        (
            1,
            "finished",
            "s-23",
            NOW,
            0,
            "alice",
            "tehas:plan",
            r#"["https://github.com/o/r/issues/7"]"#
        )
    );
    // Posted: the outcome file is no longer pending.
    let v = ok(&home, &["plan-end", "23", "--posted"]);
    assert_eq!(v["planning"]["pending"], false);
    assert_eq!(v["planning"]["pending_outcome"], Value::Null);
    assert_eq!(
        ok(&home, &["plan-end", "23", "--posted"])["planning"]["pending"],
        false
    );
    assert_eq!(
        ok(&home, &["plan-end", "99", "--posted"])["planning"],
        Value::Null
    );
}

#[test]
fn filed_keeps_the_url_of_another_repository() {
    let home = home("plan-filed");
    begin(&home, 24);
    turn_end(
        &home,
        24,
        1,
        &[
            "--outcome",
            "finished",
            "--rc",
            "0",
            "--session",
            "s",
            "--filed",
            "https://github.com/a/b/issues/3",
            "--filed",
            "https://github.com/c/d/issues/3",
            "--filed",
            "https://github.com/a/b/issues/3",
        ],
    )
    .unwrap();
    assert_eq!(
        show(&home, 24)["filed"],
        json!([
            "https://github.com/a/b/issues/3",
            "https://github.com/c/d/issues/3"
        ])
    );
    assert_eq!(
        one::<i64>(&home, "SELECT min(filed_issue) FROM planning_filed"),
        3
    );
}

#[test]
fn plan_session_brief_and_filed_survive_a_failed_turn() {
    let home = home("plan-failed");
    begin(&home, 25);
    turn_end(
        &home,
        25,
        1,
        &[
            "--outcome",
            "finished",
            "--rc",
            "0",
            "--session",
            "s",
            "--filed",
            "https://github.com/o/r/issues/9",
        ],
    )
    .unwrap();
    answer(&home, 25, 2, "bob").unwrap();
    let v = turn_end(
        &home,
        25,
        2,
        &["--outcome", "failed", "--rc", "3", "--option", "ignored"],
    )
    .unwrap();
    let p = &v["planning"];
    assert_eq!(p["session"], "s");
    assert_eq!(p["brief"], "the brief");
    assert_eq!(p["filed"], json!(["https://github.com/o/r/issues/9"]));
    assert_eq!(p["failed"], true);
    assert_eq!(p["pending"], true);
    assert_eq!(p["options"], json!([]));
    assert_eq!(p["turns"], 2);
    assert_eq!(p["turn_rc"], 3);
    assert_eq!(p["answers"], json!([{"turn":2,"by":"bob","fixer":false}]));
    assert_eq!(p["ended_at"], Value::Null);
}

#[test]
fn a_finished_turn_without_a_session_is_a_failed_turn() {
    let home = home("plan-no-session");
    begin(&home, 26);
    finished(&home, 26, 1);
    answer(&home, 26, 2, "bob").unwrap();
    let v = turn_end(&home, 26, 2, &["--outcome", "finished", "--rc", "0"]).unwrap();
    assert_eq!(v["planning"]["failed"], true);
    assert_eq!(v["planning"]["session"], "s1");
    // A first turn with no session at all ends the record.
    begin(&home, 27);
    let v = turn_end(
        &home,
        27,
        1,
        &[
            "--outcome",
            "finished",
            "--rc",
            "0",
            "--session",
            "bad session",
        ],
    )
    .unwrap();
    assert_eq!(v["cleaned"], json!(["--session"]));
    assert_eq!(v["planning"]["ended_at"], NOW);
    assert_eq!(v["planning"]["running"], false);
    assert_eq!(v["planning"]["turns"], 1);
}

#[test]
fn a_lost_session_is_marked_lost_with_its_own_outcome() {
    let home = home("plan-lost-mark");
    begin(&home, 28);
    finished(&home, 28, 1);
    answer(&home, 28, 2, "bob").unwrap();
    let v = turn_end(&home, 28, 2, &["--outcome", "lost", "--rc", "137"]).unwrap();
    assert_eq!(v["outcome"], "lost");
    assert_eq!(v["planning"]["lost"], true);
    assert_eq!(v["planning"]["turn_rc"], 137);
    assert_eq!(v["planning"]["turns"], 1);
    assert_eq!(
        one::<String>(&home, "SELECT outcome FROM plan_history WHERE turn = 2"),
        "lost"
    );
}

#[test]
fn a_lost_session_stays_running_until_closed() {
    let home = home("plan-lost-running");
    begin(&home, 29);
    turn_end(&home, 29, 1, &["--outcome", "lost"]).unwrap();
    let v = ok(&home, &["plan-show", "--open"]);
    assert_eq!(v["sessions"][0]["running"], true);
    assert_eq!(v["sessions"][0]["lost"], true);
    // A second lost end of the same turn is allowed: the poll tries again.
    turn_end(&home, 29, 1, &["--outcome", "lost"]).unwrap();
    let v = ok_at(&home, NOW + 3, &["plan-end", "29", "--close"]);
    assert_eq!(v["closed"], true);
    assert_eq!(v["planning"]["running"], false);
    assert_eq!(v["planning"]["ended_at"], NOW + 3);
    assert_eq!(ok(&home, &["plan-end", "29", "--close"])["closed"], false);
}

#[test]
fn close_withdraws_the_open_question() {
    let home = home("plan-close");
    begin(&home, 30);
    finished(&home, 30, 1);
    ok(
        &home,
        &["question-asked", "30", "--id", "300", "--kind", "plan"],
    );
    ok(&home, &["plan-end", "30", "--close"]);
    assert_eq!(count(&home, "questions"), 0);
    assert_eq!(show(&home, 30)["comment_id"], Value::Null);
}

#[test]
fn cleaned_options_never_fail_a_turn() {
    let home = home("plan-options");
    begin(&home, 31);
    let long = format!("a\nb{}", "é".repeat(200));
    let mut args: Vec<String> = [
        "--outcome",
        "finished",
        "--rc",
        "0",
        "--session",
        "s",
        "--option",
        &long,
        "--option",
        "\u{7}",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for i in 0..23 {
        args.push("--option".into());
        args.push(format!("o{i}"));
    }
    args.extend(["--filed".into(), "http://github.com/o/r/issues/1".into()]);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let v = turn_end(&home, 31, 1, &refs).unwrap();
    assert_eq!(v["cleaned"], json!(["--option", "--filed"]));
    let options = v["planning"]["options"].as_array().unwrap();
    assert_eq!(options.len(), 20);
    let first = options[0].as_str().unwrap();
    assert!(first.len() <= 300 && first.len() >= 299, "{}", first.len());
    assert!(first.starts_with("ab"));
    assert_eq!(options[1], "-");
    assert_eq!(v["planning"]["filed"], json!([]));
}

#[test]
fn plan_end_states_its_preconditions_and_arguments() {
    let home = home("plan-end-pre");
    assert_eq!(
        refused(
            &home,
            &["plan-end", "32", "--turn", "1", "--outcome", "interrupted"]
        ),
        (1, "noplan".into())
    );
    begin(&home, 32);
    assert_eq!(
        refused(
            &home,
            &["plan-end", "32", "--turn", "2", "--outcome", "interrupted"]
        ),
        (1, "turn".into())
    );
    finished(&home, 32, 1);
    assert_eq!(
        refused(
            &home,
            &["plan-end", "32", "--turn", "2", "--outcome", "interrupted"]
        ),
        (1, "turn".into())
    );
    let outside = scratch("elsewhere").join("x.out");
    let (code, line) = turn_end(
        &home,
        32,
        1,
        &[
            "--outcome",
            "interrupted",
            "--out",
            outside.to_str().unwrap(),
        ],
    )
    .unwrap_err();
    assert_eq!(code, 64, "{line}");
    let inside_dots = format!("{}/state/../x.out", home.display());
    for args in [
        &["plan-end", "32", "--outcome", "interrupted"][..],
        &["plan-end", "32", "--turn", "1"][..],
        &["plan-end", "32", "--turn", "1", "--outcome", "finished"][..],
        &[
            "plan-end",
            "32",
            "--turn",
            "1",
            "--outcome",
            "finished",
            "--rc",
            "1",
        ][..],
        &["plan-end", "32", "--turn", "1", "--outcome", "failed"][..],
        &[
            "plan-end",
            "32",
            "--turn",
            "1",
            "--outcome",
            "failed",
            "--rc",
            "0",
        ][..],
        &[
            "plan-end",
            "32",
            "--turn",
            "1",
            "--outcome",
            "interrupted",
            "--out",
            "rel.out",
        ][..],
        &[
            "plan-end",
            "32",
            "--turn",
            "1",
            "--outcome",
            "interrupted",
            "--out",
            &inside_dots,
        ][..],
        &["plan-end", "32", "--posted", "--close"][..],
        &["plan-end", "32", "--posted", "--turn", "1"][..],
        &["plan-end", "32", "--close", "--outcome", "lost"][..],
        &["plan-end", "--close"][..],
    ] {
        assert!(usage(args).starts_with("usage plan-end: "), "{args:?}");
    }
}

#[test]
fn history_keeps_fifty_turns_per_issue() {
    let home = home("plan-history");
    conn(&home)
        .execute_batch(
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 55)
             INSERT INTO plan_history (issue, turn, at, outcome) SELECT 33, i, i, 'finished' FROM n;
             WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 55)
             INSERT INTO plan_history (issue, turn, at, outcome) SELECT 34, i, i, 'finished' FROM n;",
        )
        .unwrap();
    begin(&home, 33);
    let v = finished(&home, 33, 1);
    assert_eq!(count(&home, "plan_history WHERE issue = 33"), 50);
    assert_eq!(
        one::<i64>(
            &home,
            "SELECT min(turn) FROM plan_history WHERE issue = 33 AND at < 100"
        ),
        7
    );
    let h = v["history_id"].as_i64().unwrap();
    assert_eq!(
        one::<String>(
            &home,
            &format!("SELECT issue || ' ' || turn || ' ' || at FROM plan_history WHERE id = {h}")
        ),
        format!("33 1 {NOW}")
    );
    assert_eq!(count(&home, "plan_history WHERE issue = 34"), 55);
}

#[test]
fn plan_show_open_lists_only_open_sessions() {
    let home = home("plan-show");
    assert_eq!(ok(&home, &["plan-show", "34"])["planning"], Value::Null);
    begin(&home, 36);
    begin(&home, 34);
    begin(&home, 35);
    ok(&home, &["plan-end", "35", "--close"]);
    let v = ok(&home, &["plan-show", "--open"]);
    let issues: Vec<i64> = v["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["issue"].as_i64().unwrap())
        .collect();
    assert_eq!(issues, vec![34, 36]);
    assert_eq!(show(&home, 35)["ended_at"], NOW);
    for args in [&["plan-show"][..], &["plan-show", "1", "--open"][..]] {
        assert!(usage(args).starts_with("usage plan-show: "), "{args:?}");
    }
}

// --- plan-note ---------------------------------------------------------------------

fn note(home: &Path, issue: i64, kind: &str, id: i64) -> Result<Value, (u8, String)> {
    call(
        home,
        &[
            "plan-note",
            &issue.to_string(),
            "--kind",
            kind,
            "--id",
            &id.to_string(),
        ],
    )
}

#[test]
fn plan_note_says_once_and_plan_show_lists_the_notes() {
    let home = home("plan-note");
    begin(&home, 60);
    finished(&home, 60, 1);
    let v = note(&home, 60, "held", 900).unwrap();
    assert_eq!(v["said"], true);
    assert_eq!(
        v["planning"]["notes"],
        json!({"fixer_refused":[],"held":[900]})
    );
    assert_eq!(note(&home, 60, "held", 900).unwrap()["said"], false);
    note(&home, 60, "fixer-refused", 902).unwrap();
    note(&home, 60, "fixer-refused", 901).unwrap();
    // The same id under the other kind is another note.
    assert_eq!(note(&home, 60, "fixer-refused", 900).unwrap()["said"], true);
    assert_eq!(
        show(&home, 60)["notes"],
        json!({"fixer_refused":[902,901,900],"held":[900]})
    );
    let open = ok(&home, &["plan-show", "--open"]);
    assert_eq!(open["sessions"][0]["notes"]["held"], json!([900]));
}

#[test]
fn plan_note_needs_an_open_session() {
    let home = home("plan-note-open");
    assert_eq!(
        note(&home, 61, "held", 5).unwrap_err().1.split(' ').next(),
        Some("noplan")
    );
    begin(&home, 61);
    ok(&home, &["plan-end", "61", "--close"]);
    let (code, line) = note(&home, 61, "held", 5).unwrap_err();
    assert_eq!(code, 1);
    assert!(line.starts_with("noplan #61"), "{line}");
    assert_eq!(count(&home, "planning_notes"), 0);
}

#[test]
fn plan_notes_go_with_the_session() {
    let home = home("plan-note-cascade");
    // Replaced: plan-begin over an open session that is not running.
    begin(&home, 62);
    finished(&home, 62, 1);
    note(&home, 62, "held", 1).unwrap();
    begin(&home, 62);
    assert_eq!(count(&home, "planning_notes"), 0);
    assert_eq!(
        show(&home, 62)["notes"],
        json!({"fixer_refused":[],"held":[]})
    );
    // Closed.
    finished(&home, 62, 1);
    note(&home, 62, "fixer-refused", 2).unwrap();
    ok(&home, &["plan-end", "62", "--close"]);
    assert_eq!(count(&home, "planning_notes"), 0);
    // Ended by a turn that left no session.
    begin(&home, 63);
    note(&home, 63, "held", 3).unwrap();
    turn_end(&home, 63, 1, &["--outcome", "not-started"]).unwrap();
    assert!(show(&home, 63)["ended_at"].is_number());
    assert_eq!(count(&home, "planning_notes"), 0);
    // Undone in its first turn.
    begin(&home, 64);
    note(&home, 64, "held", 4).unwrap();
    ok(&home, &["plan-begin", "64", "--undo"]);
    assert_eq!(count(&home, "planning_notes"), 0);
}

#[test]
fn plan_note_usage() {
    for (args, says) in [
        (
            &["plan-note", "1", "--kind", "seen", "--id", "5"][..],
            "--kind is not one of fixer-refused, held: \"seen\"",
        ),
        (&["plan-note", "1", "--id", "5"][..], "--kind is required"),
        (
            &["plan-note", "1", "--kind", "held"][..],
            "--id is required",
        ),
        (
            &["plan-note", "1", "--kind", "held", "--id", "0"][..],
            "--id is not a comment id",
        ),
        (
            &["plan-note", "--kind", "held", "--id", "5"][..],
            "<issue> is required",
        ),
        (
            &["plan-note", "1", "--kind", "held", "--id", "5", "--by", "a"][..],
            "unknown flag",
        ),
    ] {
        let why = usage(args);
        assert!(why.starts_with("usage plan-note: "), "{args:?}: {why}");
        assert!(why.contains(says), "{args:?}: {why}");
    }
}

// --- the answer text -------------------------------------------------------------------

fn answer_with(home: &Path, issue: i64, turn: i64, text: &Path) -> Result<Value, (u8, String)> {
    call(
        home,
        &[
            "plan-answer",
            &issue.to_string(),
            "--turn",
            &turn.to_string(),
            "--by",
            "bob",
            "--text-file",
            text.to_str().unwrap(),
        ],
    )
}

#[test]
fn plan_answer_keeps_its_text_and_plan_show_returns_it() {
    let home = home("plan-answer-text");
    begin(&home, 65);
    // The first turn answers nothing.
    assert_eq!(show(&home, 65)["answer"], Value::Null);
    finished(&home, 65, 1);
    assert_eq!(show(&home, 65)["answer"], Value::Null);
    let text = write_file(
        &home,
        "answer.md",
        "Use the second option, ünïcode.\n".as_bytes(),
    );
    let v = answer_with(&home, 65, 2, &text).unwrap();
    assert_eq!(v["planning"]["answer"], "Use the second option, ünïcode.\n");
    // Running, then pending: the answer of that turn.
    assert_eq!(
        show(&home, 65)["answer"],
        "Use the second option, ünïcode.\n"
    );
    finished(&home, 65, 2);
    assert_eq!(
        show(&home, 65)["answer"],
        "Use the second option, ünïcode.\n"
    );
    // Posted: no turn is running or pending.
    ok(&home, &["plan-end", "65", "--posted"]);
    assert_eq!(show(&home, 65)["answer"], Value::Null);
    // An answer given without a text has none.
    answer(&home, 65, 3, "bob").unwrap();
    assert_eq!(show(&home, 65)["answer"], Value::Null);
}

#[test]
fn plan_answer_undo_takes_the_text_with_it() {
    let home = home("plan-answer-text-undo");
    begin(&home, 66);
    finished(&home, 66, 1);
    let text = write_file(&home, "answer.md", b"yes");
    answer_with(&home, 66, 2, &text).unwrap();
    ok(&home, &["plan-answer", "66", "--turn", "2", "--undo"]);
    assert_eq!(count(&home, "planning_answers"), 0);
    assert_eq!(show(&home, 66)["answer"], Value::Null);
}

#[test]
fn plan_answer_refuses_a_text_file_it_cannot_take() {
    let home = home("plan-answer-text-refused");
    begin(&home, 67);
    finished(&home, 67, 1);
    let good = write_file(&home, "good.md", b"yes");
    let link = home.join("link.md");
    std::os::unix::fs::symlink(&good, &link).unwrap();
    let big = home.join("big.md");
    std::fs::write(&big, vec![b'a'; (1 << 20) + 1]).unwrap();
    for (file, says) in [
        (link, "not a regular file"),
        (home.join("none.md"), "not a regular file"),
        (big.clone(), "over 1 MiB"),
        (write_file(&home, "bytes.md", &[0xff, 0xfe]), "not UTF-8"),
    ] {
        let (code, line) = answer_with(&home, 67, 2, &file).unwrap_err();
        assert_eq!(code, 1, "{line}");
        assert!(line.starts_with("answer #67: "), "{line}");
        assert!(line.contains(says), "{line}");
    }
    assert_eq!(count(&home, "planning_answers"), 0);
    assert_eq!(show(&home, 67)["running"], false);
    std::fs::write(&big, vec![b'a'; 1 << 20]).unwrap();
    let v = answer_with(&home, 67, 2, &big).unwrap();
    assert_eq!(v["planning"]["answer"].as_str().unwrap().len(), 1 << 20);
}

#[test]
fn plan_answer_takes_the_longest_reply_github_allows() {
    let home = home("plan-answer-text-github");
    begin(&home, 68);
    finished(&home, 68, 1);
    // 65,536 characters of four bytes each, with the prefix the caller adds.
    let text = format!(
        "The operator replied on the issue:\n\n{}",
        "\u{1F600}".repeat(65_536)
    );
    let file = write_file(&home, "long.md", text.as_bytes());
    let v = answer_with(&home, 68, 2, &file).unwrap();
    assert_eq!(v["planning"]["answer"].as_str().unwrap(), text);
}

#[test]
fn plan_answer_text_file_usage() {
    let base = ["plan-answer", "1", "--turn", "2", "--by", "bob"];
    let with = |extra: &[&str]| {
        let mut v = base.to_vec();
        v.extend(extra);
        usage(&v)
    };
    assert!(with(&["--text-file", "answer.md"]).contains("--text-file is not an absolute path"));
    assert!(with(&["--text-file", "/a", "--text-file", "/b"]).contains("--text-file given twice"));
    assert!(usage(&[
        "plan-answer",
        "1",
        "--turn",
        "2",
        "--undo",
        "--text-file",
        "/a"
    ])
    .contains("--text-file is not taken with --undo"));
}

#[test]
fn an_automatic_refine_is_auto_on_the_session_and_each_turn_of_it() {
    let home = home("plan-auto");
    let b = brief(&home, "the brief");
    let v = ok(
        &home,
        &[
            "plan-begin",
            "30",
            "--by",
            "alice",
            "--label",
            "tehas:refine",
            "--brief-file",
            &b,
            "--auto",
        ],
    );
    assert_eq!(v["planning"]["auto"], true);
    finished(&home, 30, 1);
    let rows = ok(&home, &["history-show", "30", "--kind", "plan"])["rows"].clone();
    assert_eq!(rows[0]["auto"], true);
    // A session a label started is not.
    begin(&home, 31);
    finished(&home, 31, 1);
    assert_eq!(show(&home, 31)["auto"], false);
    let rows = ok(&home, &["history-show", "31", "--kind", "plan"])["rows"].clone();
    assert_eq!(rows[0]["auto"], false);
}

#[test]
fn plan_begin_takes_auto_with_a_refine_only() {
    let why = usage(&[
        "plan-begin",
        "1",
        "--by",
        "alice",
        "--label",
        "tehas:plan",
        "--brief-file",
        "/b",
        "--auto",
    ]);
    assert!(why.starts_with("usage plan-begin: --auto"), "{why}");
    assert!(usage(&["plan-begin", "1", "--undo", "--auto"]).starts_with("usage plan-begin: "));
}
