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
fn history_show_lists_every_column_in_the_order_written() {
    let home = home("history-order");
    cycle(&home, 11, "label", NOW + 30);
    turn(&home, 10, NOW + 20);
    cycle(&home, 10, "label", NOW + 20);
    cycle(&home, 10, "fixer", NOW + 40);
    let all = rows(&home, &[]);
    // In the order the ends were written, whatever their times and issues:
    // one sequence across both tables.
    assert_eq!(
        keys(&all),
        vec![
            ("cycle".to_string(), 11, NOW + 30),
            ("plan".to_string(), 10, NOW + 20),
            ("cycle".to_string(), 10, NOW + 20),
            ("cycle".to_string(), 10, NOW + 40),
        ]
    );
    let seqs: Vec<i64> = all.iter().map(|r| r["seq"].as_i64().unwrap()).collect();
    assert_eq!(seqs, vec![1, 2, 3, 4]);
    let c = all[2].clone();
    let id = c["id"].clone();
    assert_eq!(
        c,
        json!({"kind":"cycle","id":id,"issue":10,"attempt":1,"at":NOW + 20,"status":"failed",
               "step":"review","by":"alice","how":"label","pr":40,"started_at":NOW + 10,"rc":3,
               "cap":null,"park_reason":null,"failure_kind":null,"failure_cause":null,
               "provider":true,"parked":false,"seq":3})
    );
    let p = all[1].clone();
    let id = p["id"].clone();
    assert_eq!(
        p,
        json!({"kind":"plan","id":id,"issue":10,"turn":1,"at":NOW + 20,"outcome":"finished",
               "session":"s1","started_at":NOW + 10,"rc":0,"by":"bob","label":"tehas:refine","seq":2,
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
fn history_show_takes_the_millwright_under_both_words() {
    let home = home("history-millwright-both");
    // Three rows, each at a time of its own: a person's label, a start of this
    // release's word, and a third made as a millwright's start and then read
    // back as the row a host wrote before the rename, which still says `fixer`.
    // A read for either millwright word keeps the last two and no other; a read
    // of another word keeps its own alone.
    cycle(&home, 12, "label", NOW);
    cycle(&home, 12, "millwright", NOW + 30);
    let (l, a) = start_by(&home, 12, "tehas-fixer", "millwright", NOW + 50);
    ok_at(
        &home,
        NOW + 60,
        &[
            "cycle-end",
            "12",
            "--attempt",
            &a.to_string(),
            "--status",
            "failed",
            "--step",
            "review",
        ],
    );
    drop(l);
    // The UPDATE names that one row by the time `cycle-end` wrote it: two rows
    // already share `at` NOW + 30, so the attempt alone is not enough.
    let c = conn(&home);
    c.execute(
        "UPDATE cycle_history SET how = 'fixer' WHERE issue = 12 AND at = ?1",
        [NOW + 60],
    )
    .unwrap();
    drop(c);
    for word in ["millwright", "fixer"] {
        assert_eq!(
            keys(&rows(&home, &["12", "--kind", "cycle", "--how", word])),
            vec![
                ("cycle".to_string(), 12, NOW + 30),
                ("cycle".to_string(), 12, NOW + 60),
            ],
            "--how {word}"
        );
    }
    // A row of another word is kept by its own filter and not by the pair's:
    // the refresh row is there for `--how refresh` to find, and neither
    // millwright word above took it.
    cycle(&home, 12, "refresh", NOW + 90);
    assert_eq!(
        keys(&rows(&home, &["12", "--kind", "cycle", "--how", "refresh"])),
        vec![("cycle".to_string(), 12, NOW + 90)],
        "the refresh row is its own, and no millwright word reaches it"
    );
    assert_eq!(
        keys(&rows(&home, &["12", "--kind", "cycle", "--how", "hand"])),
        Vec::<(String, i64, i64)>::new(),
        "a word with no row of its own keeps none"
    );
    // The label row is there, and a filter of `label` keeps it alone: the pair
    // did not take it.
    assert_eq!(
        keys(&rows(&home, &["12", "--kind", "cycle", "--how", "label"])),
        vec![("cycle".to_string(), 12, NOW)]
    );
    assert_eq!(
        keys(&rows(&home, &["12", "--kind", "cycle"])),
        vec![
            ("cycle".to_string(), 12, NOW),
            ("cycle".to_string(), 12, NOW + 30),
            ("cycle".to_string(), 12, NOW + 60),
            ("cycle".to_string(), 12, NOW + 90),
        ]
    );
}

#[test]
fn history_show_answers_the_old_word_alone_when_only_that_word_was_written() {
    // The one case the pair's filter must answer on its own: a row whose `how`
    // is only the old word, and no row of the new one. Were the alias bound to
    // another word, the read of `fixer` here would come back empty.
    let home = home("history-fixer-only");
    cycle(&home, 13, "label", NOW);
    let (l, a) = start_by(&home, 13, "tehas-fixer", "millwright", NOW + 50);
    ok_at(
        &home,
        NOW + 60,
        &[
            "cycle-end",
            "13",
            "--attempt",
            &a.to_string(),
            "--status",
            "failed",
            "--step",
            "review",
        ],
    );
    drop(l);
    let c = conn(&home);
    c.execute(
        "UPDATE cycle_history SET how = 'fixer' WHERE issue = 13 AND at = ?1",
        [NOW + 60],
    )
    .unwrap();
    drop(c);
    assert_eq!(
        keys(&rows(&home, &["13", "--kind", "cycle", "--how", "fixer"])),
        vec![("cycle".to_string(), 13, NOW + 60)]
    );
    assert_eq!(
        keys(&rows(
            &home,
            &["13", "--kind", "cycle", "--how", "millwright"]
        )),
        vec![("cycle".to_string(), 13, NOW + 60)]
    );
    assert_eq!(
        keys(&rows(&home, &["13", "--kind", "cycle", "--how", "label"])),
        vec![("cycle".to_string(), 13, NOW)]
    );
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
