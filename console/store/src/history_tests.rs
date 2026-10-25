//! history-show.

use crate::verbs_tests::*;
use serde_json::{json, Value};
use std::path::Path;

fn rows(home: &Path, args: &[&str]) -> Vec<Value> {
    let mut all = vec!["history-show"];
    all.extend(args);
    ok(home, &all)["rows"].as_array().unwrap().clone()
}

/// `(kind, issue, at)` of each row, in the order answered.
fn keys(rows: &[Value]) -> Vec<(String, i64, i64)> {
    rows.iter()
        .map(|r| {
            (
                r["kind"].as_str().unwrap().to_string(),
                r["issue"].as_i64().unwrap(),
                r["at"].as_i64().unwrap(),
            )
        })
        .collect()
}

/// A cycle of `issue` started by `how` and ended at `at`.
fn cycle(home: &Path, issue: i64, how: &str, at: i64) {
    let (_l, a) = start_by(home, issue, "alice", how, at - 10);
    ok_at(
        home,
        at,
        &[
            "cycle-end",
            &issue.to_string(),
            "--attempt",
            &a.to_string(),
            "--status",
            "failed",
            "--step",
            "review",
            "--rc",
            "3",
            "--pr",
            "40",
            "--provider",
        ],
    );
}

/// A planning turn of `issue` that finished at `at`, filing one issue.
fn turn(home: &Path, issue: i64, at: i64) {
    let brief = write_file(home, &format!("brief-{issue}.md"), b"brief");
    ok_at(
        home,
        at - 10,
        &[
            "plan-begin",
            &issue.to_string(),
            "--by",
            "bob",
            "--label",
            "tehas:refine",
            "--brief-file",
            brief.to_str().unwrap(),
        ],
    );
    ok_at(
        home,
        at,
        &[
            "plan-end",
            &issue.to_string(),
            "--turn",
            "1",
            "--outcome",
            "finished",
            "--rc",
            "0",
            "--session",
            "s1",
            "--filed",
            "https://github.com/o/r/issues/9",
        ],
    );
}

#[test]
fn history_show_with_nothing_is_an_empty_list() {
    let home = home("history-empty");
    let v = ok(&home, &["history-show"]);
    assert_eq!(
        v,
        json!({"api":1,"verb":"history-show","ok":true,"rows":[]})
    );
    assert_eq!(rows(&home, &["5"]), Vec::<Value>::new());
}

#[test]
fn history_show_lists_every_column_oldest_first() {
    let home = home("history-order");
    cycle(&home, 11, "label", NOW + 30);
    turn(&home, 10, NOW + 20);
    cycle(&home, 10, "label", NOW + 20);
    cycle(&home, 10, "fixer", NOW + 40);
    let all = rows(&home, &[]);
    // At the same time and issue the lower id first: the plan row is the
    // first of its table, the cycle row the second of its own.
    assert_eq!(
        keys(&all),
        vec![
            ("plan".to_string(), 10, NOW + 20),
            ("cycle".to_string(), 10, NOW + 20),
            ("cycle".to_string(), 11, NOW + 30),
            ("cycle".to_string(), 10, NOW + 40),
        ]
    );
    let c = all[1].clone();
    let id = c["id"].clone();
    assert_eq!(
        c,
        json!({"kind":"cycle","id":id,"issue":10,"attempt":1,"at":NOW + 20,"status":"failed",
               "step":"review","by":"alice","how":"label","pr":40,"started_at":NOW + 10,"rc":3,
               "cap":null,"park_reason":null,"failure_kind":null,"failure_cause":null,
               "provider":true,"parked":false})
    );
    let p = all[0].clone();
    let id = p["id"].clone();
    assert_eq!(
        p,
        json!({"kind":"plan","id":id,"issue":10,"turn":1,"at":NOW + 20,"outcome":"finished",
               "session":"s1","started_at":NOW + 10,"rc":0,"by":"bob","label":"tehas:refine",
               "filed":["https://github.com/o/r/issues/9"],"auto":false})
    );
}

#[test]
fn history_show_takes_an_issue_a_kind_and_a_how() {
    let home = home("history-filter");
    cycle(&home, 10, "label", NOW + 20);
    cycle(&home, 10, "refresh", NOW + 30);
    cycle(&home, 11, "refresh", NOW + 35);
    turn(&home, 10, NOW + 25);
    assert_eq!(rows(&home, &["10"]).len(), 3);
    assert_eq!(rows(&home, &["11"]).len(), 1);
    assert_eq!(
        keys(&rows(&home, &["10", "--kind", "plan"])),
        vec![("plan".to_string(), 10, NOW + 25)]
    );
    assert_eq!(
        keys(&rows(&home, &["10", "--kind", "cycle"])),
        vec![
            ("cycle".to_string(), 10, NOW + 20),
            ("cycle".to_string(), 10, NOW + 30)
        ]
    );
    // --how keeps the cycles started that way; a plan row has no how.
    assert_eq!(
        keys(&rows(&home, &["--how", "refresh"])),
        vec![
            ("cycle".to_string(), 10, NOW + 30),
            ("cycle".to_string(), 11, NOW + 35)
        ]
    );
    assert_eq!(
        keys(&rows(&home, &["10", "--kind", "cycle", "--how", "refresh"])),
        vec![("cycle".to_string(), 10, NOW + 30)]
    );
    assert_eq!(rows(&home, &["10", "--how", "fixer"]), Vec::<Value>::new());
}

#[test]
fn history_show_refuses_bad_arguments() {
    for (args, says) in [
        (&["history-show", "x"][..], "<issue> is not a number"),
        (&["history-show", "0"][..], "<issue> is not a number"),
        (&["history-show", "1", "2"][..], "\"2\" unexpected"),
        (
            &["history-show", "--kind", "run"][..],
            "--kind is not one of cycle, plan: \"run\"",
        ),
        (
            &["history-show", "--how", "ready"][..],
            "--how is not one of",
        ),
        (
            &["history-show", "--kind", "plan", "--how", "label"][..],
            "--how is not taken with --kind plan",
        ),
        (&["history-show", "--kind"][..], "--kind needs a value"),
        (
            &["history-show", "--kind", "cycle", "--kind", "cycle"][..],
            "--kind given twice",
        ),
        (&["history-show", "--all"][..], "unknown flag"),
    ] {
        let why = usage(args);
        assert!(why.starts_with("usage history-show: "), "{args:?}: {why}");
        assert!(why.contains(says), "{args:?}: {why}");
    }
}

#[test]
fn history_show_makes_no_issues_row() {
    let home = home("history-reads");
    rows(&home, &["12"]);
    assert_eq!(count(&home, "issues"), 0);
}
