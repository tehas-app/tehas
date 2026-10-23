//! approval-record and approval-current.

use crate::verbs_tests::*;
use serde_json::{json, Value};
use std::path::Path;

fn record(home: &Path, issue: i64, at: i64, by: &str, how: &str) -> Value {
    let (n, a) = (issue.to_string(), at.to_string());
    let mut args = vec![
        "approval-record",
        &n,
        "--at",
        &a,
        "--by",
        by,
        "--how",
        how,
        "--body-sha",
        SHA,
    ];
    if how == "label" || how == "fixer" {
        args.extend(["--label", "tehas:ready"]);
    }
    if how == "refresh" {
        args.extend([
            "--refresh-pr",
            "40",
            "--refresh-head",
            "0123456789abcdef0123456789abcdef01234567",
        ]);
    }
    ok(home, &args)
}

fn current(home: &Path, issue: i64) -> Value {
    ok(home, &["approval-current", &issue.to_string()])
}

#[test]
fn approval_record_stores_every_field() {
    let home = home("approval");
    let v = ok(
        &home,
        &[
            "approval-record",
            "10",
            "--at",
            "1791000000",
            "--by",
            "alice",
            "--how",
            "label",
            "--label",
            "tehas:ready",
            "--body-sha",
            SHA,
            "--title-sha",
            SHA_B,
            "--comment-id",
            "123456",
        ],
    );
    assert_eq!(
        v["approval"],
        json!({"issue":10,"at":1791000000,"by":"alice","how":"label","label":"tehas:ready",
               "body_sha256":SHA,"title_sha256":SHA_B,"comment_id":123456,"refresh_pr":null,
               "refresh_head":null,"used_at":null,"used_by":null})
    );
    let v = record(&home, 11, 5, "bob", "refresh");
    assert_eq!(v["approval"]["refresh_pr"], 40);
    assert_eq!(v["approval"]["label"], "refresh");
    assert_eq!(
        record(&home, 12, 5, "bob", "retry-tick")["approval"]["label"],
        "retry-tick"
    );
    assert_eq!(
        record(&home, 13, 5, "bob", "outage-retry")["approval"]["label"],
        "outage-retry"
    );
    assert_eq!(
        record(&home, 14, 5, "bob", "fixer")["approval"]["label"],
        "tehas:ready"
    );
    assert_eq!(count(&home, "issues"), 5);
}

#[test]
fn approval_record_refuses_bad_arguments() {
    let base = [
        "approval-record",
        "1",
        "--at",
        "5",
        "--by",
        "a",
        "--body-sha",
        SHA,
    ];
    let with = |extra: &[&str]| {
        let mut v = base.to_vec();
        v.extend(extra);
        usage(&v)
    };
    assert!(with(&["--how", "label"]).contains("--label is required with --how label"));
    assert!(
        with(&["--how", "retry-tick", "--label", "tehas:ready"]).contains("--label is not taken")
    );
    assert!(
        with(&["--how", "label", "--label", "ready"]).contains("--label is not a tehas:* label")
    );
    assert!(with(&["--how", "refresh"]).contains("--refresh-pr and --refresh-head"));
    assert!(with(&["--how", "refresh", "--refresh-pr", "4"]).contains("--refresh-head is required"));
    assert!(with(&[
        "--how",
        "label",
        "--label",
        "tehas:ready",
        "--refresh-pr",
        "4",
        "--refresh-head",
        &"a".repeat(40)
    ])
    .contains("exactly with --how refresh"));
    assert!(with(&[
        "--how",
        "fixer",
        "--label",
        "tehas:ready",
        "--comment-id",
        "0"
    ])
    .contains("--comment-id"));
    assert!(with(&[
        "--how",
        "fixer",
        "--label",
        "tehas:ready",
        "--title-sha",
        "x"
    ])
    .contains("--title-sha"));
    for args in [
        &[
            "approval-record",
            "1",
            "--by",
            "a",
            "--how",
            "label",
            "--label",
            "tehas:ready",
            "--body-sha",
            SHA,
        ][..],
        &[
            "approval-record",
            "1",
            "--at",
            "5",
            "--how",
            "label",
            "--label",
            "tehas:ready",
            "--body-sha",
            SHA,
        ][..],
        &[
            "approval-record",
            "1",
            "--at",
            "5",
            "--by",
            "a",
            "--how",
            "label",
            "--label",
            "tehas:ready",
        ][..],
        &[
            "approval-record",
            "1",
            "--at",
            "5",
            "--by",
            "a",
            "--label",
            "tehas:ready",
            "--body-sha",
            SHA,
        ][..],
        &["approval-record", "1", "--undo"][..],
        &["approval-record", "1", "--undo", "--at", "5", "--by", "a"][..],
        &["approval-current"][..],
        &["approval-current", "1", "--at", "5"][..],
    ] {
        assert!(usage(args).starts_with("usage approval-"), "{args:?}");
    }
}

#[test]
fn approval_record_keeps_one_unused_row() {
    let home = home("approval-one");
    record(&home, 20, 5, "alice", "label");
    record(&home, 20, 9, "bob", "label");
    assert_eq!(count(&home, "approvals WHERE issue = 20"), 1);
    assert_eq!(
        one::<String>(&home, "SELECT by_account FROM approvals"),
        "bob"
    );
    // The same at again replaces its fields.
    record(&home, 20, 9, "carol", "label");
    assert_eq!(
        one::<String>(&home, "SELECT by_account FROM approvals"),
        "carol"
    );
}

#[test]
fn approval_record_over_a_used_approval_is_refused() {
    let home = home("approval-used");
    let (_l, _) = start(&home, 21);
    let at = one::<i64>(&home, "SELECT at FROM approvals WHERE issue = 21").to_string();
    assert_eq!(
        refused(
            &home,
            &[
                "approval-record",
                "21",
                "--at",
                &at,
                "--by",
                "bob",
                "--how",
                "label",
                "--label",
                "tehas:ready",
                "--body-sha",
                SHA
            ]
        ),
        (1, "used".into())
    );
    assert_eq!(
        one::<String>(&home, "SELECT by_account FROM approvals WHERE issue = 21"),
        "alice"
    );
}

#[test]
fn approval_undo_removes_only_the_unused_row() {
    let home = home("approval-undo");
    let (_l, _) = start(&home, 22);
    let used_at = one::<i64>(&home, "SELECT at FROM approvals WHERE issue = 22");
    record(&home, 22, 5, "bob", "label");
    assert_eq!(
        ok(
            &home,
            &[
                "approval-record",
                "22",
                "--undo",
                "--at",
                &used_at.to_string()
            ]
        )["undone"],
        false
    );
    assert_eq!(
        ok(&home, &["approval-record", "22", "--undo", "--at", "5"])["undone"],
        true
    );
    assert_eq!(
        ok(&home, &["approval-record", "22", "--undo", "--at", "5"])["undone"],
        false
    );
    assert_eq!(count(&home, "approvals WHERE issue = 22"), 1);
    assert_eq!(current(&home, 22)["current"]["at"], used_at);
}

#[test]
fn approval_current_approval_is_null_once_used() {
    let home = home("approval-current");
    assert_eq!(
        current(&home, 23),
        json!({"api":1,"verb":"approval-current","ok":true,"approval":null,"current":null,
               "rests_on":null,"fixer_starts":0,"refreshes":0})
    );
    approve(&home, 23, "alice", "label");
    let v = current(&home, 23);
    assert_eq!(v["approval"]["by"], "alice");
    assert_eq!(v["current"], Value::Null);
    let _lock = hold(&home, 23);
    ok(&home, &["cycle-start", "23"]);
    let v = current(&home, 23);
    assert_eq!(v["approval"], Value::Null);
    assert_eq!(v["current"]["used_by"], 1);
    assert_eq!(v["rests_on"]["by"], "alice");
    assert_eq!(v["rests_on"]["used_by"], 1);
}

#[test]
fn approval_current_rests_only_on_used_rows() {
    let home = home("approval-rests");
    let (l, a) = start_by(&home, 24, "alice", "label", NOW);
    end(&home, 24, a, "failed");
    drop(l);
    let (l, b) = start_by(&home, 24, "fixerbot", "fixer", NOW);
    end(&home, 24, b, "failed");
    drop(l);
    let (_l, c) = start_by(&home, 24, "alice", "refresh", NOW);
    let v = current(&home, 24);
    assert_eq!(v["current"]["how"], "refresh");
    assert_eq!(v["current"]["used_by"], c);
    assert_eq!(v["rests_on"]["how"], "label");
    assert_eq!(v["rests_on"]["used_by"], a);
    assert_eq!(v["fixer_starts"], 1);
    assert_eq!(v["refreshes"], 1);
}

#[test]
fn an_unused_label_approval_is_never_a_base() {
    let home = home("approval-unused-base");
    let (l, a) = start_by(&home, 25, "fixerbot", "fixer", NOW);
    end(&home, 25, a, "failed");
    drop(l);
    approve(&home, 25, "mallory", "label");
    let v = current(&home, 25);
    assert_eq!(v["rests_on"], Value::Null);
    assert_eq!(v["approval"]["by"], "mallory");
    assert_eq!(v["fixer_starts"], 1);
}

#[test]
fn approvals_keep_only_the_base_and_its_starts() {
    let home = home("approval-retain");
    for (by, how) in [
        ("alice", "label"),
        ("fixerbot", "fixer"),
        ("bob", "label"),
        ("fixerbot", "fixer"),
        ("alice", "refresh"),
    ] {
        let (l, a) = start_by(&home, 26, by, how, NOW);
        end(&home, 26, a, "failed");
        drop(l);
    }
    // What stays: bob's label (used_by 3) and the fixer and refresh on it.
    let rows: Vec<(i64, String)> = {
        let c = conn(&home);
        let mut q = c
            .prepare("SELECT used_by, how FROM approvals WHERE issue = 26 ORDER BY used_by")
            .unwrap();
        q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert_eq!(
        rows,
        vec![
            (3, "label".into()),
            (4, "fixer".into()),
            (5, "refresh".into())
        ]
    );
    let v = current(&home, 26);
    assert_eq!(v["rests_on"]["by"], "bob");
    assert_eq!(
        (v["fixer_starts"].clone(), v["refreshes"].clone()),
        (json!(1), json!(1))
    );
    // A start with no base keeps only itself.
    let (l, a) = start_by(&home, 27, "fixerbot", "fixer", NOW);
    end(&home, 27, a, "failed");
    drop(l);
    let (_l, _) = start_by(&home, 27, "fixerbot", "fixer", NOW);
    assert_eq!(count(&home, "approvals WHERE issue = 27"), 1);
}
