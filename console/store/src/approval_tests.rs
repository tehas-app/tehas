//! approval-record and approval-current.

use crate::verbs_tests::*;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

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
    if how == "label" || crate::approval::is_millwright(how) {
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
fn a_plan_with_no_open_points_is_recorded_as_the_tehas_plan_that_approved_it() {
    let home = home("approval-planned");
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
            "tehas:plan",
            "--body-sha",
            SHA,
        ],
    );
    assert_eq!(v["approval"]["label"], "tehas:plan");
    assert_eq!(v["approval"]["by"], "alice");
    assert_eq!(v["approval"]["how"], "label");
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
               "refresh_head":null,"used_at":null,"used_by":null,"input":null,"easy":false})
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
fn a_millwright_start_is_written_under_the_new_word_and_counted_by_both() {
    let home = home("approval-millwright");
    // The dispatcher of this release still sends the old word; part 5 sends the
    // new one. Both rows say `millwright`, and both need the person's label.
    let v = record(&home, 40, 5, "tehas-fixer", "fixer");
    assert_eq!(v["approval"]["how"], "millwright");
    assert_eq!(v["approval"]["label"], "tehas:ready");
    let v = record(&home, 41, 5, "tehas-fixer", "millwright");
    assert_eq!(v["approval"]["how"], "millwright");
    assert_eq!(v["approval"]["label"], "tehas:ready");
    // A row written before the rename still counts toward the starts.
    ok(
        &home,
        &[
            "approval-record",
            "42",
            "--at",
            "4",
            "--by",
            "oldbot",
            "--how",
            "label",
            "--label",
            "tehas:ready",
            "--body-sha",
            SHA,
        ],
    );
    let c = conn(&home);
    c.execute(
        "UPDATE approvals SET how = 'fixer', used_at = at, used_by = 1
         WHERE issue = 42",
        [],
    )
    .unwrap();
    drop(c);
    let v = current(&home, 42);
    assert_eq!(v["millwright_starts"], 1, "an old fixer row is a start");
    assert_eq!(v["fixer_starts"], 1, "the answer keeps the old key too");
}

#[test]
fn the_two_spellings_of_the_millwright_are_the_same_starts() {
    // The starts of both spellings count together, above the base.
    let counted = home("approval-millwright-counts");
    let (l, a) = start_by(&counted, 43, "alice", "label", NOW);
    end(&counted, 43, a, "failed");
    drop(l);
    let (l, b) = start_by(&counted, 43, "tehas-fixer", "fixer", NOW);
    end(&counted, 43, b, "failed");
    drop(l);
    let (_l, c) = start_by(&counted, 43, "tehas-fixer", "millwright", NOW);
    end(&counted, 43, c, "failed");
    let v = current(&counted, 43);
    assert_eq!(
        (v["millwright_starts"].clone(), v["fixer_starts"].clone()),
        (json!(2), json!(2)),
        "the two spellings are the same starts"
    );
    assert_eq!(v["rests_on"]["by"], "alice");
}

#[test]
fn a_millwright_start_rests_on_the_person_and_is_a_base_of_its_own() {
    // The row's own word is the new one, so rests_on still steps over it.
    let home = home("approval-millwright-base");
    let (l, a) = start_by(&home, 44, "alice", "label", NOW);
    end(&home, 44, a, "failed");
    drop(l);
    let (l, b) = start_by(&home, 44, "tehas-fixer", "millwright", NOW);
    end(&home, 44, b, "failed");
    drop(l);
    let (_l, _) = start_by(&home, 44, "alice", "refresh", NOW);
    let v = current(&home, 44);
    assert_eq!(v["current"]["how"], "refresh");
    assert_eq!(v["rests_on"]["by"], "alice");
    assert_eq!(v["rests_on"]["used_by"], a);
    assert_eq!(v["millwright_starts"], 1);
    assert_eq!(b, a + 1);
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
               "rests_on":null,"millwright_starts":0,"fixer_starts":0,"refreshes":0,"easy":false})
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
            (4, "millwright".into()),
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

// --- inputs ----------------------------------------------------------------------

const INPUT: &str = r#"{"links":["https://github.com/user-attachments/assets/1"],"comment_links":[{"link":"https://example.test/a","source":"comment:5","by":"alice"}],"retry_context":[{"source":"pr-comment:9","text":"fix the test"}],"refresh_of":{"pr":40,"head_sha":"0123456789abcdef0123456789abcdef01234567","base_sha":"89abcdef0123456789abcdef0123456789abcdef","branch":"feature/issue-30"}}"#;

fn record_with(home: &Path, issue: i64, at: i64, input: &Path) -> Result<Value, (u8, String)> {
    call(
        home,
        &[
            "approval-record",
            &issue.to_string(),
            "--at",
            &at.to_string(),
            "--by",
            "alice",
            "--how",
            "label",
            "--label",
            "tehas:ready",
            "--body-sha",
            SHA,
            "--input-file",
            input.to_str().unwrap(),
        ],
    )
}

#[test]
fn approval_record_keeps_its_inputs_and_current_returns_them() {
    let home = home("approval-input");
    let file = write_file(&home, "input.json", INPUT.as_bytes());
    let want: Value = serde_json::from_str(INPUT).unwrap();
    let v = record_with(&home, 30, 5, &file).unwrap();
    assert_eq!(v["approval"]["input"], want);
    let v = current(&home, 30);
    assert_eq!(v["approval"]["input"], want);
    let _lock = hold(&home, 30);
    ok(&home, &["cycle-start", "30"]);
    let v = current(&home, 30);
    assert_eq!(v["approval"], Value::Null);
    assert_eq!(v["current"]["input"], want);
    // An approval given without inputs has none, and the next one replaces
    // the unused row with its own.
    let v = record(&home, 30, 6, "bob", "label");
    assert_eq!(v["approval"]["input"], Value::Null);
    assert_eq!(current(&home, 30)["current"]["input"], want);
    // A record made again at the same time takes the inputs it is given now.
    record_with(&home, 31, 7, &file).unwrap();
    assert_eq!(
        record(&home, 31, 7, "alice", "label")["approval"]["input"],
        Value::Null
    );
    let empty = write_file(&home, "empty.json", b"{}");
    assert_eq!(
        record_with(&home, 32, 7, &empty).unwrap()["approval"]["input"],
        json!({})
    );
}

#[test]
fn approval_record_refuses_an_input_file_it_cannot_take() {
    let home = home("approval-input-refused");
    let good = write_file(&home, "good.json", INPUT.as_bytes());
    let link = home.join("link.json");
    std::os::unix::fs::symlink(&good, &link).unwrap();
    let big = home.join("big.json");
    // Within the bound by whitespace alone, then one byte over it.
    let mut text = br#"{"links":[]}"#.to_vec();
    text.resize(1 << 20, b' ');
    std::fs::write(&big, &text).unwrap();
    assert!(record_with(&home, 40, 5, &big).is_ok());
    text.push(b' ');
    std::fs::write(&big, &text).unwrap();
    let cases: Vec<(PathBuf, &str)> = vec![
        (link, "not a regular file"),
        (home.join("none.json"), "not a regular file"),
        (home.clone(), "not a regular file"),
        (big, "over 1 MiB"),
        (write_file(&home, "bytes.json", &[0xff, 0xfe]), "not UTF-8"),
        (write_file(&home, "text.json", b"links"), "not JSON"),
        (write_file(&home, "list.json", b"[]"), "not a JSON object"),
        (
            write_file(&home, "key.json", br#"{"links":[],"branch":"main"}"#),
            "holds a key it may not: \"branch\"",
        ),
        (
            write_file(&home, "type.json", br#"{"links":"a"}"#),
            "holds links of another type",
        ),
        (
            write_file(&home, "of.json", br#"{"refresh_of":[]}"#),
            "holds refresh_of of another type",
        ),
        (
            write_file(&home, "driver.json", br#"{"driver":"classic"}"#),
            "holds driver of another type",
        ),
        (
            write_file(&home, "number.json", br#"{"driver":1}"#),
            "holds driver of another type",
        ),
    ];
    for (file, says) in cases {
        let (code, line) = record_with(&home, 41, 5, &file).unwrap_err();
        assert_eq!(code, 1, "{}: {line}", file.display());
        assert!(line.starts_with("input #41: "), "{line}");
        assert!(line.contains(says), "{}: {line}", file.display());
    }
    assert_eq!(count(&home, "approvals WHERE issue = 41"), 0);
    assert_eq!(count(&home, "issues WHERE issue = 41"), 0);
    let null_of = write_file(&home, "null.json", br#"{"refresh_of":null}"#);
    assert!(record_with(&home, 41, 5, &null_of).is_ok());
}

#[test]
fn approval_record_input_file_usage() {
    let base = [
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
        "--body-sha",
        SHA,
    ];
    let with = |extra: &[&str]| {
        let mut v = base.to_vec();
        v.extend(extra);
        usage(&v)
    };
    assert!(with(&["--input-file", "input.json"]).contains("--input-file is not an absolute path"));
    assert!(
        with(&["--input-file", "/a", "--input-file", "/b"]).contains("--input-file given twice")
    );
    assert!(with(&["--input-file"]).contains("--input-file needs a value"));
    assert!(usage(&[
        "approval-record",
        "1",
        "--undo",
        "--at",
        "5",
        "--input-file",
        "/a"
    ])
    .contains("--input-file is not taken with --undo"));
}

/// The driver lives in the inputs, so no schema changes: a record with none is
/// classic and the store answers it as it always has.
#[test]
fn the_driver_is_kept_with_the_inputs_and_a_record_without_one_names_none() {
    let home = home("approval-driver");
    let lean = write_file(&home, "lean.json", br#"{"links":[],"driver":"lean"}"#);
    record_with(&home, 43, 5, &lean).unwrap();
    assert_eq!(current(&home, 43)["approval"]["input"]["driver"], "lean");
    let classic = write_file(&home, "classic.json", br#"{"links":[]}"#);
    record_with(&home, 44, 5, &classic).unwrap();
    assert_eq!(current(&home, 44)["approval"]["input"].get("driver"), None);
    record(&home, 45, 5, "alice", "label");
    assert_eq!(current(&home, 45)["approval"]["input"], Value::Null);
}

#[test]
fn approval_undo_removes_the_inputs_with_the_row() {
    let home = home("approval-input-undo");
    let file = write_file(&home, "input.json", INPUT.as_bytes());
    record_with(&home, 42, 5, &file).unwrap();
    assert_eq!(count(&home, "approvals WHERE input_json IS NOT NULL"), 1);
    let v = ok(&home, &["approval-record", "42", "--undo", "--at", "5"]);
    assert_eq!(v["undone"], true);
    assert_eq!(count(&home, "approvals"), 0);
    assert_eq!(current(&home, 42)["approval"], Value::Null);
}

/// A refresh start recorded as an easy merge: the row keeps the mark, and
/// approval-current answers the dispatcher's question, "has this approval had
/// its one easy merge".
#[test]
fn an_easy_refresh_start_is_marked_and_counted() {
    let home = home("approval-easy");
    let n = "51";
    ok(
        &home,
        &[
            "approval-record",
            n,
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
        ],
    );
    let lock = hold(&home, 51);
    ok(&home, &["cycle-start", n]);
    end(&home, 51, 1, "failed");
    drop(lock);

    ok(
        &home,
        &[
            "approval-record",
            n,
            "--at",
            "1791000001",
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
    let lock = hold(&home, 51);
    ok_at(&home, NOW + 1, &["cycle-start", n]);
    let v = current(&home, 51);
    assert_eq!(v["current"]["how"], "refresh", "{v}");
    assert_eq!(v["easy"], true, "{v}");
    assert_eq!(v["refreshes"], 1, "{v}");
    end(&home, 51, 2, "failed");
    drop(lock);

    // A start on the same base that is not a refresh does not spend it again,
    // and a refresh of a later base answers for its own base.
    let v = current(&home, 51);
    assert_eq!(v["easy"], true, "{v}");
}

/// The mark belongs to a refresh alone, and a row without it answers `easy`
/// false: what a refresh of an older store ran was a full cycle.
#[test]
fn the_easy_mark_is_a_refresh_switch_and_defaults_to_no() {
    let home = home("approval-easy-usage");
    approve(&home, 52, "alice", "label");
    let v = current(&home, 52);
    assert_eq!(v["approval"]["easy"], false);

    let (n, a) = ("53".to_string(), "1791000002".to_string());
    assert!(usage(&[
        "approval-record",
        &n,
        "--at",
        &a,
        "--by",
        "alice",
        "--how",
        "label",
        "--label",
        "tehas:ready",
        "--body-sha",
        SHA,
        "--easy",
    ])
    .contains("--easy is given only with --how refresh"));
    assert_eq!(
        count(&home, "approvals WHERE issue = 53"),
        0,
        "a refused row was written"
    );

    // A refresh row recorded without the switch keeps 0, which reads as "no
    // easy merge spent".
    record(&home, 54, 5, "alice", "refresh");
    let _lock = hold(&home, 54);
    ok(&home, &["cycle-start", "54"]);
    let v = current(&home, 54);
    assert_eq!(v["current"]["how"], "refresh");
    assert_eq!(v["easy"], false, "{v}");
}
