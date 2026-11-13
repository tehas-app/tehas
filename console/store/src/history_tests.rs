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

/// A refresh of `issue` that ran as an easy merge and ended at `at`. The
/// approval of its start carries `--easy`, which is where `history-show` reads
/// the mark from (#1751).
fn easy_refresh(home: &Path, issue: i64, at: i64) {
    let at_s = (at - 10).to_string();
    ok_at(
        home,
        at - 10,
        &[
            "approval-record",
            &issue.to_string(),
            "--at",
            &at_s,
            "--by",
            "alice",
            "--how",
            "refresh",
            "--body-sha",
            SHA,
            "--refresh-pr",
            "40",
            "--refresh-head",
            "0123456789abcdef0123456789abcdef01234567",
            "--easy",
        ],
    );
    let (_l, a) = {
        let lock = hold(home, issue);
        let v = ok_at(home, at - 5, &["cycle-start", &issue.to_string()]);
        (lock, v["attempt"].as_i64().unwrap())
    };
    end(home, issue, a, "failed");
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
               "provider":true,"parked":false,"easy":false,"seq":3})
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
fn history_show_keeps_the_easy_mark_of_a_pruned_approval() {
    // The read of #1751. A refresh that ran as an easy merge keeps its mark on the
    // row `history-show` answers even after the row of its approval is gone: a
    // second refresh on the same base is what makes it go, because the keeping that
    // follows a start keeps the base and the starts on it and deletes the rest. The
    // start that merged leaves no row of its own -- an easy merge runs no container --
    // so the newest row of such a record is the row of the merge, and the rules of
    // #1031 read the mark off it to hold a second easy merge back.
    let home = home("history-easy-pruned");
    start(&home, 31);
    easy_refresh(&home, 31, NOW + 40);
    let marked = rows(&home, &["31", "--kind", "cycle"]);
    assert_eq!(
        marked
            .iter()
            .filter(|r| r["how"] == "refresh" && r["easy"] == json!(true))
            .count(),
        1,
        "the row of the easy merge says so: {marked:?}"
    );
    // The next refresh of the same base, whose start's keeping deletes the approval
    // of the start of the easy merge.
    easy_refresh(&home, 31, NOW + 200);
    let after = rows(&home, &["31", "--kind", "cycle"]);
    assert_eq!(
        after
            .iter()
            .filter(|r| r["how"] == "refresh" && r["easy"] == json!(true))
            .count(),
        2,
        "both rows of the easy merges still say so after the keeping: {after:?}"
    );
}

#[test]
fn history_show_keeps_the_easy_mark_past_a_cycle_that_never_ran() {
    // The same read, on the other path that deletes used approvals: a cycle whose
    // runner started no container ends `--never-ran`, which takes every used approval
    // of the issue with it so no later start rests on an approval that started nothing
    // -- the easy merge's row among them, and that row is what the row of its cycle
    // answers `easy` off. With it gone the history would answer a plain `false` for a
    // merge that ran, and the poll, reading no easy merge on the record, would choose
    // that very merge again (#1751). A merge is one merge whether the container it
    // asked for ever ran.
    let home = home("history-easy-never-ran");
    // The easy merge, and the start of the refresh after it, which the runner never
    // started. `--never-ran` is what the unit passes for such a run, with the exit of
    // the runner that made no container.
    start(&home, 33);
    easy_refresh(&home, 33, NOW + 40);
    let (l, a) = {
        let lock = hold(&home, 33);
        // The row of this start is written with its own time and its own head: the
        // helpers share one `at`, and `cycle-start` takes the newest unused row, so a
        // row of a stale `at` would overwrite the approval of the easy merge whose
        // mark this case reads.
        let at = (NOW + 200).to_string();
        let head = "b".repeat(40);
        ok_at(
            &home,
            NOW + 200,
            &[
                "approval-record",
                "33",
                "--at",
                &at,
                "--by",
                "alice",
                "--how",
                "refresh",
                "--body-sha",
                SHA,
                "--refresh-pr",
                "40",
                "--refresh-head",
                &head,
            ],
        );
        let v = ok_at(&home, NOW + 205, &["cycle-start", "33"]);
        assert_eq!(
            one::<i64>(&home, "SELECT started_easy FROM approvals WHERE issue = 33"),
            0,
            "this start is not the easy merge: its row is its own"
        );
        (lock, v["attempt"].as_i64().unwrap())
    };
    ok_at(
        &home,
        NOW + 210,
        &[
            "cycle-end",
            "33",
            "--attempt",
            &a.to_string(),
            "--status",
            "unknown",
            "--rc",
            "4",
            "--parked",
            "--never-ran",
        ],
    );
    drop(l);
    let after = rows(&home, &["33", "--kind", "cycle"]);
    assert_eq!(
        after
            .iter()
            .filter(|r| r["how"] == "refresh" && r["easy"] == json!(true))
            .count(),
        1,
        "the row of the merge that ran says so although a later end deleted the \
         used rows: {after:?}"
    );
    // Of the used approvals the end leaves one: the marked row. The record still holds
    // no base for a later start to rest on, which is what `--never-ran` is for.
    assert_eq!(
        count(&home, "approvals WHERE issue = 33 AND used_by IS NOT NULL"),
        1,
        "the approval of the easy merge is the only used row the end leaves"
    );
}

#[test]
fn history_show_makes_no_issues_row() {
    let home = home("history-reads");
    rows(&home, &["12"]);
    assert_eq!(count(&home, "issues"), 0);
}
