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
            .contains("--result is not one of ok, empty, read-failed, timed-out, unknown")
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

/// #1754: a write GitHub may have taken (a timeout, a 5xx) is not settled. The
/// row stays open as `unknown`, with no time and no result, until a read says
/// what happened.
#[test]
fn an_unknown_write_stays_open_until_a_read_settles_it() {
    let home = home("journal-unknown");
    let id = intend(&home, 8000, "7").to_string();
    // Only a call has an unknown end, and a call is made once attempted.
    assert_eq!(
        refused(
            &home,
            &["journal-settle", "--action", &id, "--result", "unknown"]
        ),
        (1, "action".into())
    );
    ok_at(&home, 8001, &["journal-attempt", "--action", &id]);
    let u = ok_at(
        &home,
        8002,
        &["journal-settle", "--action", &id, "--result", "unknown"],
    );
    assert_eq!(u["state"], "unknown");
    assert_eq!(u["result"], json!(null));
    assert_eq!(
        count(
            &home,
            "actions WHERE state = 'unknown' AND settled_at IS NULL AND result IS NULL"
        ),
        1
    );
    let open = ok(&home, &["journal-open"]);
    assert_eq!(open["actions"][0]["state"], "unknown");
    assert_eq!(open["actions"][0]["attempted_at"], 8001);
    // Unknown is said once; the next answer is a read's.
    assert_eq!(
        refused(
            &home,
            &["journal-settle", "--action", &id, "--result", "unknown"]
        ),
        (1, "action".into())
    );
    let d = ok_at(
        &home,
        8100,
        &["journal-settle", "--action", &id, "--result", "ok"],
    );
    assert_eq!(d["state"], "done");
    assert_eq!(ok(&home, &["journal-open"])["actions"], json!([]));
    assert_eq!(
        one::<String>(
            &home,
            "SELECT group_concat(detail, ',') FROM journal WHERE step = 'settle' ORDER BY id"
        ),
        "unknown,ok"
    );
    // A read that finds it did not happen abandons it.
    let other = intend(&home, 8200, "8").to_string();
    ok(&home, &["journal-attempt", "--action", &other]);
    ok(
        &home,
        &["journal-settle", "--action", &other, "--result", "unknown"],
    );
    assert_eq!(
        ok(
            &home,
            &["journal-settle", "--action", &other, "--abandoned"]
        )["state"],
        "abandoned"
    );
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

#[test]
fn the_kinds_of_a_planning_sessions_end_are_taken() {
    let home = home("journal-kinds");
    ok_at(&home, 7000, &["poll-begin"]);
    for kind in ["issue-edit", "sub-issue-add", "issue-close"] {
        let v = ok_at(
            &home,
            7000,
            &[
                "journal-record",
                "--poll",
                "7000",
                "--kind",
                kind,
                "--issue",
                "7",
                "--args",
                "{}",
                "--before",
                "6990",
            ],
        );
        assert!(v["action"].as_i64().is_some(), "{kind}");
    }
    assert_eq!(count(&home, "actions"), 3);
}

#[test]
fn open_marks_a_row_whose_arguments_are_not_an_object() {
    // The write path takes only a JSON object, so such a row comes from a
    // directly-written or older store. It is said, not read as null: a reader
    // cannot tell an unreadable blob from an action that took no arguments.
    let home = home("journal-args-unreadable");
    ok_at(&home, 8000, &["poll-begin"]);
    let readable = intend(&home, 8000, "7");
    for (i, body) in ["not JSON", "[1,2]", "null", r#""a string""#]
        .into_iter()
        .enumerate()
    {
        conn(&home)
            .execute(
                "INSERT INTO actions (poll_at, kind, issue, args_json, before_snapshot,
                    state, intended_at)
                 VALUES (8000, 'comment-post', 7, ?1, 7990, 'intended', ?2)",
                rusqlite::params![body, 8001 + i as i64],
            )
            .unwrap();
    }
    let open = ok(&home, &["journal-open"]);
    let actions = open["actions"].as_array().unwrap();
    assert_eq!(actions.len(), 5);
    let good = actions.iter().find(|a| a["id"] == readable).unwrap();
    assert_eq!(good["args"]["repo"], "example-owner/example-product");
    assert_eq!(good["args_unreadable"], serde_json::Value::Null);
    for a in actions.iter().filter(|a| a["id"] != readable) {
        assert_eq!(a["args_unreadable"], json!(true), "{a}");
        assert_eq!(a["args"], serde_json::Value::Null, "{a} has args");
    }
}

/// A write as `Journal::send` records it, by its JSON.
fn write_json(w: &tehas_ports::Write) -> String {
    serde_json::to_string(w).unwrap()
}

fn ticket(n: u64) -> tehas_ports::TicketKey {
    tehas_ports::TicketKey::Number(std::num::NonZeroU64::new(n).unwrap())
}

fn record_write(home: &std::path::Path, now: i64, write: &str, args: &str) {
    ok_at(home, now, &["poll-begin"]);
    ok_at(
        home,
        now,
        &[
            "journal-record",
            "--poll",
            &now.to_string(),
            "--write",
            write,
            "--args",
            args,
            "--before",
            &(now - 5).to_string(),
        ],
    );
}

/// T4.4: a write row keeps its `Write` as the `write` field of its
/// arguments, beside what the caller gave; its kind is the write's word and
/// its issue the ticket it names.
#[test]
fn a_write_is_recorded_by_its_word_and_the_number_it_names() {
    let home = home("journal-write");
    let reopen = tehas_ports::Write::Reopen { ticket: ticket(7) };
    let args = r#"{"repo":"example-owner/example-product","cycle":{"issue":7,"attempt":2}}"#;
    record_write(&home, 1000, &write_json(&reopen), args);
    let row = &ok(&home, &["journal-open"])["actions"][0];
    assert_eq!(row["kind"], "ticket-reopen");
    assert_eq!(row["issue"], 7);
    assert_eq!(row["args"]["repo"], "example-owner/example-product");
    assert_eq!(row["args"]["cycle"]["attempt"], 2);
    let kept: tehas_ports::Write = serde_json::from_value(row["args"]["write"].clone()).unwrap();
    assert_eq!(kept, reopen);

    // A write that names no ticket, change or blueprint is on issue 0.
    record_write(&home, 1001, &write_json(&tehas_ports::Write::Prepare), "{}");
    let row = &ok(&home, &["journal-open"])["actions"][1];
    assert_eq!(row["kind"], "marks-prepare");
    assert_eq!(row["issue"], 0);
}

#[test]
fn a_write_row_takes_no_kind_no_issue_and_no_body_in_text() {
    let reopen = write_json(&tehas_ports::Write::Reopen { ticket: ticket(7) });
    let rec = |extra: &[&str], write: &str, args: &str| {
        let mut a = vec!["journal-record", "--poll", "1", "--write", write];
        a.extend_from_slice(extra);
        a.extend(["--args", args, "--before", "1"]);
        usage(&a)
    };
    assert!(
        rec(&["--kind", "label-add"], &reopen, "{}").contains("--kind is not taken with --write")
    );
    assert!(rec(&["--issue", "7"], &reopen, "{}").contains("--issue is not taken with --write"));
    assert!(rec(&[], "[1]", "{}").contains("--write is not a write the journal records"));
    assert!(rec(&[], r#"{"type":"Paint"}"#, "{}")
        .contains("--write is not a write the journal records"));
    // The row keeps a body by its sum alone.
    let note = write_json(&tehas_ports::Write::TicketNote {
        ticket: ticket(7),
        body: tehas_ports::Body::Text("text".into()),
        markers: vec![],
    });
    assert!(rec(&[], &note, "{}").contains("--write is not a write the journal records"));
    assert!(rec(&[], &reopen, r#"{"write":{}}"#).contains("--args holds a write already"));
}

/// Every word a write is recorded by is one of `KINDS`, and no two kinds
/// share one but the two notes and the two edits, as 1.x wrote them. The
/// words of a change's mark and edit are B3's, so those kinds are not here.
#[test]
fn each_write_kind_has_its_own_word_in_kinds() {
    use tehas_ports::WriteKind as K;
    let all = [
        K::Mark,
        K::Unmark,
        K::Close,
        K::Reopen,
        K::TicketNote,
        K::FileTicket,
        K::EditTicket,
        K::SetKind,
        K::SetPriority,
        K::WaitOn,
        K::StopWaiting,
        K::Assign,
        K::AddPart,
        K::RemovePart,
        K::NewTarget,
        K::EditTarget,
        K::SetTarget,
        K::Prepare,
        K::EnsureMark,
        K::OpenBlueprint,
        K::EditBlueprint,
        K::Comment,
        K::EditComment,
        K::CloseBlueprint,
        K::OpenChange,
        K::CommentChange,
        K::CloseChange,
        K::Merge,
        K::Propose,
        K::Tag,
        K::SetStatus,
        K::NewRelease,
        K::Upload,
        K::PublishRelease,
    ];
    let mut words: Vec<&str> = all
        .iter()
        .map(|k| crate::journal::word(*k).unwrap_or_else(|| panic!("{k:?} has no word")))
        .collect();
    for w in &words {
        assert!(crate::journal::KINDS.contains(w), "{w}");
    }
    assert_eq!(
        crate::journal::word(K::TicketNote),
        crate::journal::word(K::Comment)
    );
    assert_eq!(
        crate::journal::word(K::EditTicket),
        crate::journal::word(K::EditBlueprint)
    );
    words.sort_unstable();
    words.dedup();
    assert_eq!(words.len(), all.len() - 2);
    assert_eq!(crate::journal::word(K::Unknown), None);
}
