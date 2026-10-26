//! poll-begin and the journal verbs.

use crate::verbs_tests::*;
use serde_json::json;

const ARGS: &str = r#"{"repo":"example-owner/example-product","add":["tehas:running"]}"#;

fn intend(home: &std::path::Path, now: i64, issue: &str) -> i64 {
    ok_at(home, now, &["poll-begin"]);
    ok_at(
        home,
        now,
        &[
            "journal-record",
            "--poll",
            &now.to_string(),
            "--kind",
            "label-add",
            "--issue",
            issue,
            "--args",
            ARGS,
            "--before",
            &(now - 5).to_string(),
        ],
    )["action"]
        .as_i64()
        .unwrap()
}

#[test]
fn the_arguments_are_strict() {
    let rec = |kind: &str, args: &str| {
        usage(&[
            "journal-record",
            "--poll",
            "1",
            "--kind",
            kind,
            "--issue",
            "7",
            "--args",
            args,
            "--before",
            "1",
        ])
    };
    assert!(rec("label-paint", "{}").contains("--kind is not one of label-add"));
    assert!(rec("label-add", "[1]").contains("--args is not a JSON object"));
    assert!(rec("label-add", "{").contains("--args is not a JSON object"));
    assert!(usage(&["journal-record", "--kind", "label-add"]).contains("is required"));
    assert!(usage(&["journal-settle", "--action", "1"]).contains("--result or --abandoned"));
    assert!(usage(&[
        "journal-settle",
        "--action",
        "1",
        "--result",
        "ok",
        "--abandoned"
    ])
    .contains("--abandoned is not taken with --result"));
    assert!(
        usage(&["journal-settle", "--action", "1", "--result", "fine"])
            .contains("--result is not one of ok, empty, read-failed, timed-out")
    );
    assert!(usage(&["journal-attempt", "--action", "x"]).contains("is not an action number"));
    assert!(usage(&["poll-begin", "--at", "1"]).contains("unknown flag"));
}

#[test]
fn an_action_is_intended_attempted_and_settled_once() {
    let home = home("journal-life");
    let id = intend(&home, 1000, "7");
    let open = ok(&home, &["journal-open"]);
    assert_eq!(open["actions"][0]["id"], id);
    assert_eq!(open["actions"][0]["state"], "intended");
    assert_eq!(open["actions"][0]["poll_at"], 1000);
    assert_eq!(open["actions"][0]["before"], 995);
    assert_eq!(open["actions"][0]["args"]["add"], json!(["tehas:running"]));

    let a = ok_at(
        &home,
        1001,
        &["journal-attempt", "--action", &id.to_string()],
    );
    assert_eq!(a["state"], "attempted");
    assert_eq!(
        refused(&home, &["journal-attempt", "--action", &id.to_string()]),
        (1, "action".into())
    );

    let s = ok_at(
        &home,
        1002,
        &[
            "journal-settle",
            "--action",
            &id.to_string(),
            "--result",
            "ok",
            "--evidence",
            r#"{"labels":["tehas:running"]}"#,
        ],
    );
    assert_eq!(s["state"], "done");
    assert_eq!(ok(&home, &["journal-open"])["actions"], json!([]));
    assert_eq!(
        one::<String>(&home, "SELECT evidence_json FROM actions"),
        r#"{"labels":["tehas:running"]}"#
    );
    assert_eq!(
        one::<String>(
            &home,
            "SELECT group_concat(step, ',') FROM journal ORDER BY id"
        ),
        "intended,attempt,settle"
    );
    // Settled is settled: a second answer is refused, not written over.
    assert_eq!(
        refused(
            &home,
            &[
                "journal-settle",
                "--action",
                &id.to_string(),
                "--result",
                "timed-out"
            ]
        ),
        (1, "action".into())
    );
}

#[test]
fn a_failed_call_is_failed_and_an_abandoned_one_needs_no_attempt() {
    let home = home("journal-fail");
    let a = intend(&home, 2000, "8");
    let b = intend(&home, 2000, "9");
    ok(&home, &["journal-attempt", "--action", &a.to_string()]);
    let f = ok(
        &home,
        &[
            "journal-settle",
            "--action",
            &a.to_string(),
            "--result",
            "read-failed",
        ],
    );
    assert_eq!(f["state"], "failed");
    let g = ok(
        &home,
        &["journal-settle", "--action", &b.to_string(), "--abandoned"],
    );
    assert_eq!(g["state"], "abandoned");
    assert_eq!(g["result"], json!(null));
    assert_eq!(count(&home, "actions WHERE settled_at IS NOT NULL"), 2);
}

#[test]
fn an_action_names_a_poll_on_record_and_an_unknown_one_is_refused() {
    let home = home("journal-poll");
    assert_eq!(
        refused(
            &home,
            &[
                "journal-record",
                "--poll",
                "5",
                "--kind",
                "comment-post",
                "--issue",
                "7",
                "--args",
                "{}",
                "--before",
                "1"
            ]
        ),
        (1, "poll".into())
    );
    assert_eq!(count(&home, "actions"), 0);
    assert_eq!(
        refused(&home, &["journal-attempt", "--action", "41"]),
        (1, "action".into())
    );
    // Two polls in one second share their row.
    ok_at(&home, 3000, &["poll-begin"]);
    assert_eq!(ok_at(&home, 3000, &["poll-begin"])["at"], 3000);
    assert_eq!(count(&home, "polls"), 1);
}

#[test]
fn open_lists_the_unsettled_oldest_first() {
    let home = home("journal-order");
    let late = intend(&home, 5000, "3");
    let early = intend(&home, 4000, "4");
    let done = intend(&home, 4500, "5");
    ok(&home, &["journal-attempt", "--action", &done.to_string()]);
    ok(
        &home,
        &[
            "journal-settle",
            "--action",
            &done.to_string(),
            "--result",
            "ok",
        ],
    );
    ok(&home, &["journal-attempt", "--action", &late.to_string()]);
    let ids: Vec<i64> = ok(&home, &["journal-open"])["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, vec![early, late]);
}

#[test]
fn an_action_never_attempted_has_no_result_and_is_only_abandoned() {
    let home = home("journal-unattempted");
    let id = intend(&home, 6000, "6").to_string();
    assert_eq!(
        refused(
            &home,
            &["journal-settle", "--action", &id, "--result", "ok"]
        ),
        (1, "action".into())
    );
    assert_eq!(
        ok(&home, &["journal-settle", "--action", &id, "--abandoned"])["state"],
        "abandoned"
    );
}
