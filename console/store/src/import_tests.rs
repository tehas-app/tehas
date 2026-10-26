//! store import: today's record files into rows, each file moved once its rows
//! read back.

use crate::import::{day, iso};
use crate::verbs_tests::*;
use crate::{execute, Verb};
use serde_json::Value;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

const HEAD: &str = "0123456789abcdef0123456789abcdef01234567";

/// A home with a store and the marker install.sh leaves.
fn host(name: &str) -> PathBuf {
    let h = home(name);
    fs::write(h.join(".tehas-home"), "tehas\n").unwrap();
    h
}

fn t(s: &str) -> i64 {
    iso(s).unwrap()
}

/// A file under state/ with its mtime.
fn put(home: &Path, rel: &str, body: &str, mtime: i64) {
    let p = home.join("state").join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, body).unwrap();
    File::options()
        .write(true)
        .open(&p)
        .unwrap()
        .set_modified(UNIX_EPOCH + Duration::from_secs(mtime as u64))
        .unwrap();
}

fn there(home: &Path, rel: &str) -> bool {
    fs::symlink_metadata(home.join("state").join(rel)).is_ok()
}

fn moved(home: &Path, rel: &str) -> bool {
    there(home, &format!("imported-{}/{rel}", day(NOW)))
}

fn import(home: &Path, dry_run: bool) -> Result<Value, (u8, String)> {
    execute(home, &Verb::Import { dry_run }, NOW).map(|line| {
        assert_eq!(line.lines().count(), 1, "{line}");
        let v: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["api"], 1);
        assert_eq!(v["verb"], "import");
        assert_eq!(v["ok"], true);
        assert_eq!(v["dry_run"], dry_run);
        v
    })
}

fn import_ok(home: &Path) -> Value {
    import(home, false).unwrap_or_else(|e| panic!("{e:?}"))
}

fn rows(home: &Path) -> i64 {
    [
        "cycles",
        "approvals",
        "parks",
        "questions",
        "cycle_settings",
        "cycle_history",
        "planning",
        "plan_history",
    ]
    .iter()
    .map(|tb| count(home, tb))
    .sum()
}

fn label_record(at: &str, by: &str) -> String {
    format!(
        r#"{{"at":"{at}","by":"{by}","how":"label","body_sha256":"{SHA}","title_sha256":"{SHA_B}","links":["https://example.com/a.png"],"comment_links":[]}}"#
    )
}

fn line(at: &str, started: i64, status: &str) -> String {
    format!(
        r#"{{"at":"{at}","started":{started},"rc":0,"status":"{status}","step":"review","provider":false,"parked":false,"pr":"https://github.com/acme/widget/pull/41","by":"alice","how":"label"}}"#
    )
}

// --- the root and the live refusal ---------------------------------------------------

#[test]
fn an_unmarked_home_is_78_and_nothing_is_touched() {
    let h = home("unmarked");
    put(&h, "cycles/7.taken", "", NOW - 100);
    let (code, why) = import(&h, false).unwrap_err();
    assert_eq!(code, 78);
    assert!(why.starts_with("unmarked "), "{why}");
    assert!(there(&h, "cycles/7.taken"));
    assert!(!there(&h, &format!("imported-{}", day(NOW))));
    assert_eq!(rows(&h), 0);
    // A link named as the marker is not the marker.
    std::os::unix::fs::symlink("/etc/hostname", h.join(".tehas-home")).unwrap();
    assert_eq!(import(&h, true).unwrap_err().0, 78);
}

#[test]
fn nothing_to_import_answers_zero_counts() {
    let h = host("nothing");
    let v = import_ok(&h);
    assert!(
        v["imported"].as_object().unwrap().values().all(|n| n == 0),
        "{v}"
    );
    assert_eq!(v["moved"], 0);
    assert_eq!(v["held"], serde_json::json!([]));
    assert_eq!(count(&h, "meta WHERE key = 'imported_at'"), 0);
    // A fresh host has no database yet: the dry run install.sh makes first needs none.
    fs::remove_file(crate::db::path(&h)).unwrap();
    assert_eq!(import(&h, true).unwrap()["moved"], 0);
}

#[test]
fn a_held_cycle_or_plan_lock_refuses_with_live_and_writes_nothing() {
    let h = host("live");
    put(&h, "cycles/7.taken", "", NOW - 100);
    let lock = hold(&h, 9);
    let (code, why) = import(&h, false).unwrap_err();
    assert_eq!((code, why.split(' ').next().unwrap()), (1, "live"), "{why}");
    assert!(why.contains("#9"), "{why}");
    assert_eq!(import(&h, true).unwrap_err().0, 1);
    drop(lock);
    let plan_lock = h.join("state/plan-12.json.lock");
    let f = File::create(&plan_lock).unwrap();
    f.lock().unwrap();
    let (code, why) = import(&h, false).unwrap_err();
    assert_eq!(code, 1);
    assert!(why.starts_with("live #12: a planning turn"), "{why}");
    drop(f);
    assert!(there(&h, "cycles/7.taken"));
    assert_eq!(rows(&h), 0);
    // A lock with nothing to import refuses nothing.
    fs::remove_file(h.join("state/cycles/7.taken")).unwrap();
    let _again = hold(&h, 9);
    assert_eq!(import_ok(&h)["moved"], 0);
}

// --- the cycle's records ---------------------------------------------------------------

#[test]
fn a_cycle_its_approval_settings_and_history_become_rows_and_move() {
    let h = host("cycle");
    let start = t("2026-10-14T10:00:00Z");
    put(&h, "cycles/7.taken", "", start);
    put(&h, "cycles/7.by", "alice\n", start);
    put(&h, "cycles/7.rukas", "abcdef0\n", start);
    put(
        &h,
        "cycles/7.image",
        &format!("{IMAGE} 1234567\n"),
        start + 5,
    );
    put(
        &h,
        "cycles/7.branch",
        "feature/issue-7-thing\n",
        start + 900,
    );
    put(
        &h,
        "cycles/7.settings",
        r#"{"container":{"PI_ENSEMBLE_MODEL":"m"},"host":{"TEHAS_SLOTS":"2"},"concurrentCycles":1}"#,
        start + 5,
    );
    put(
        &h,
        "cycles/7.approved.json",
        &label_record("2026-10-14T09:59:00Z", "alice"),
        start - 1,
    );
    let history = [
        line("2026-10-13T12:00:00Z", t("2026-10-13T11:00:00Z"), "failed"),
        line("2026-10-14T11:00:00Z", start + 1, "handoff"),
    ]
    .join("\n");
    put(&h, "cycles/7.history.jsonl", &history, start + 3600);
    // Carriers stay where they are.
    put(&h, "cycles/7.result.json", "{}", start + 3600);
    put(&h, "refined.jsonl", "{}\n", start);

    let v = import_ok(&h);
    let got = &v["imported"];
    assert_eq!(got["cycles"], 1, "{v}");
    assert_eq!(got["approvals"], 1);
    assert_eq!(got["cycle_settings"], 3);
    assert_eq!(got["cycle_history"], 2);
    assert_eq!(v["moved"], 8);

    let c = conn(&h);
    type Row = (
        i64,
        i64,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        i64,
    );
    let row: Row = c
        .query_row(
            "SELECT attempt, started_at, started_by, how, rukas_commit, image, branch, end_reason, ended_at
             FROM cycles WHERE issue = 7",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        row,
        (
            2,
            start,
            Some("alice".into()),
            "label".into(),
            Some("abcdef0".into()),
            Some(IMAGE.into()),
            Some("feature/issue-7-thing".into()),
            "handoff".into(),
            t("2026-10-14T11:00:00Z")
        )
    );
    let cur = ok(&h, &["approval-current", "7"]);
    assert_eq!(cur["approval"], Value::Null, "the start used it");
    assert_eq!(cur["current"]["used_by"], 2);
    assert_eq!(cur["current"]["by"], "alice");
    assert_eq!(cur["current"]["label"], "tehas:ready");
    assert_eq!(cur["current"]["title_sha256"], SHA_B);
    assert_eq!(cur["rests_on"]["used_by"], 2);
    let input: String = c
        .query_row(
            "SELECT input_json FROM approvals WHERE issue = 7",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let input: Value = serde_json::from_str(&input).unwrap();
    assert_eq!(input["links"][0], "https://example.com/a.png");
    assert_eq!(input["comment_links"], serde_json::json!([]));
    let hist: Vec<(i64, String, Option<i64>, Option<String>)> = c
        .prepare(
            "SELECT attempt, status, pr, by_account FROM cycle_history WHERE issue = 7 ORDER BY at",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        hist,
        [
            (1, "failed".into(), Some(41), Some("alice".into())),
            (2, "handoff".into(), Some(41), Some("alice".into()))
        ]
    );
    let shown = ok(&h, &["cycle-show", "7"]);
    assert_eq!(shown["cycle"]["settings"]["PI_ENSEMBLE_MODEL"], "m");
    assert_eq!(shown["cycle"]["settings"]["concurrentCycles"], "1");
    for rel in [
        "cycles/7.taken",
        "cycles/7.by",
        "cycles/7.rukas",
        "cycles/7.image",
        "cycles/7.branch",
        "cycles/7.settings",
        "cycles/7.approved.json",
        "cycles/7.history.jsonl",
    ] {
        assert!(!there(&h, rel), "{rel} is still there");
        assert!(moved(&h, rel), "{rel} was not moved");
    }
    assert!(there(&h, "cycles/7.result.json"));
    assert!(there(&h, "refined.jsonl"));
    assert_eq!(
        one::<String>(&h, "SELECT value FROM meta WHERE key = 'imported_at'"),
        NOW.to_string()
    );
    // The next start is the attempt after the last one on record.
    let _lock = hold(&h, 7);
    approve(&h, 7, "alice", "label");
    assert_eq!(ok(&h, &["cycle-start", "7"])["attempt"], 3);
}

#[test]
fn a_start_with_no_line_after_it_ended_lost_and_a_run_by_hand_has_no_account() {
    let h = host("lost");
    let start = t("2026-10-14T10:00:00Z");
    put(&h, "cycles/7.taken", "", start);
    put(
        &h,
        "cycles/7.history.jsonl",
        &line("2026-10-13T12:00:00Z", start - 90_000, "failed"),
        start - 80_000,
    );
    put(&h, "cycles/8.taken", "", start);
    put(&h, "cycles/8.by", "alice\n", start);
    import_ok(&h);
    let c = conn(&h);
    let seven: (i64, String, i64, String) = c
        .query_row(
            "SELECT attempt, end_reason, ended_at, how FROM cycles WHERE issue = 7",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(seven, (2, "lost".into(), start, "hand".into()));
    let eight: (i64, Option<String>, String) = c
        .query_row(
            "SELECT attempt, started_by, how FROM cycles WHERE issue = 8",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        eight,
        (1, None, "hand".into()),
        "F6: no approval, no account"
    );
}

#[test]
fn a_fixer_start_keeps_its_base_and_its_count() {
    let h = host("fixer");
    let start = t("2026-10-14T10:00:00Z");
    put(&h, "cycles/7.taken", "", start);
    put(&h, "cycles/7.by", "fixerbot\n", start);
    let record = format!(
        r#"{{"at":"2026-10-14T09:59:00Z","by":"fixerbot","how":"fixer","body_sha256":"{SHA}","links":[],
            "rests_on":{{"at":"2026-10-13T08:00:00Z","by":"bob","how":"label","body_sha256":"{SHA}"}},"fixer_starts":2}}"#
    );
    put(&h, "cycles/7.approved.json", &record, start - 1);
    let v = import_ok(&h);
    assert_eq!(v["imported"]["approvals"], 3, "{v}");
    let cur = ok(&h, &["approval-current", "7"]);
    assert_eq!(
        cur["current"]["how"], "millwright",
        "imported under the new word"
    );
    assert_eq!(cur["current"]["used_by"], 1);
    assert_eq!(cur["rests_on"]["by"], "bob");
    assert_eq!(cur["rests_on"]["how"], "label");
    assert_eq!(cur["rests_on"]["at"], t("2026-10-13T08:00:00Z"));
    assert_eq!(cur["fixer_starts"], 2, "never fewer than the file said");
    assert_eq!(cur["millwright_starts"], 2, "the new key, the same number");
    assert_eq!(cur["refreshes"], 0);
    // The next start's retention keeps what approval-current reads.
    let _lock = hold(&h, 7);
    approve(&h, 7, "fixerbot", "fixer");
    ok(&h, &["cycle-start", "7"]);
    let cur = ok(&h, &["approval-current", "7"]);
    assert_eq!(cur["rests_on"]["by"], "bob");
    assert_eq!(cur["fixer_starts"], 3);
}

#[test]
fn a_millwright_record_imports_under_the_new_names() {
    // A record written after the rename: the new word, the new count key, the
    // new mark and the new note key.
    let h = host("millwright");
    let start = t("2026-10-14T10:00:00Z");
    put(&h, "cycles/7.taken", "", start);
    put(&h, "cycles/7.by", "tehas-millwright\n", start);
    let record = format!(
        r#"{{"at":"2026-10-14T09:59:00Z","by":"tehas-millwright","how":"millwright","body_sha256":"{SHA}","links":[],
            "rests_on":{{"at":"2026-10-13T08:00:00Z","by":"bob","how":"label","body_sha256":"{SHA}"}},"millwright_starts":2}}"#
    );
    put(&h, "cycles/7.approved.json", &record, start - 1);
    put(
        &h,
        "plan-7.json",
        r#"{"session":"sess-1","turns":2,"answers":[{"turn":2,"by":"tehas-millwright","millwright":true}],"millwright_refused":[55]}"#,
        start,
    );
    import_ok(&h);
    let cur = ok(&h, &["approval-current", "7"]);
    assert_eq!(cur["current"]["how"], "millwright");
    assert_eq!(cur["millwright_starts"], 2);
    assert_eq!(cur["fixer_starts"], 2, "the answer keeps the old key too");
    let p = &ok(&h, &["plan-show", "7"])["planning"];
    assert_eq!(
        p["answers"],
        serde_json::json!([{"turn":2,"by":"tehas-millwright","fixer":true}])
    );
    assert_eq!(p["notes"]["millwright_refused"], serde_json::json!([55]));
    assert_eq!(
        count(&h, "planning_notes WHERE kind = 'millwright-refused'"),
        1
    );
}

#[test]
fn a_record_that_names_its_millwright_starts_twice_is_held() {
    // Both spellings of the count in one record, with two numbers: the file
    // says two things, so nothing is imported and the file stays where it is.
    let h = host("starts-twice");
    let start = t("2026-10-14T10:00:00Z");
    put(&h, "cycles/7.taken", "", start);
    let record = format!(
        r#"{{"at":"2026-10-14T09:59:00Z","by":"tehas-millwright","how":"millwright","body_sha256":"{SHA}","links":[],
            "rests_on":{{"at":"2026-10-13T08:00:00Z","by":"bob","how":"label","body_sha256":"{SHA}"}},
            "millwright_starts":2,"fixer_starts":1}}"#
    );
    put(&h, "cycles/7.approved.json", &record, start - 1);
    let (code, why) = import(&h, false).unwrap_err();
    assert_eq!(code, 1, "{why}");
    assert!(
        why.contains(
            "state/cycles/7.approved.json: names its millwright starts twice, and differently"
        ),
        "{why}"
    );
    assert!(there(&h, "cycles/7.approved.json"), "the file stays");
    assert_eq!(count(&h, "approvals"), 0);
    // An undo of nothing changes nothing: the store still holds no row.
    ok(&h, &["approval-record", "7", "--undo", "--at", "1"]);
    assert_eq!(count(&h, "approvals"), 0);
}

#[test]
fn a_record_that_marks_its_answer_twice_is_held() {
    // The same answer marked under both names and unequally: the import cannot
    // know which mark the record meant, so the plan file is held.
    let h = host("mark-twice");
    let start = t("2026-10-14T10:00:00Z");
    put(
        &h,
        "plan-7.json",
        r#"{"session":"sess-1","turns":2,"answers":[{"turn":2,"by":"alice","millwright":true,"fixer":false}]}"#,
        start,
    );
    let (code, why) = import(&h, false).unwrap_err();
    assert_eq!(code, 1, "{why}");
    assert!(
        why.contains("state/plan-7.json: marks its answer twice, and differently"),
        "{why}"
    );
    assert!(there(&h, "plan-7.json"), "the file stays");
    assert_eq!(count(&h, "planning_answers"), 0);
}

#[test]
fn a_record_of_both_spellings_imports_the_notes_once_each() {
    // One list of refused comments under each name: the rows are the notes of
    // both, one per comment, and the answer's mark reads as the new field.
    let h = host("both-spellings");
    let start = t("2026-10-14T10:00:00Z");
    put(&h, "cycles/7.taken", "", start);
    let record = format!(
        r#"{{"at":"2026-10-14T09:59:00Z","by":"tehas-millwright","how":"millwright","body_sha256":"{SHA}","links":[],
            "rests_on":{{"at":"2026-10-13T08:00:00Z","by":"bob","how":"label","body_sha256":"{SHA}"}},
            "millwright_starts":2,"fixer_starts":2}}"#
    );
    put(&h, "cycles/7.approved.json", &record, start - 1);
    put(
        &h,
        "plan-7.json",
        r#"{"session":"sess-1","turns":2,"answers":[{"turn":2,"by":"alice","fixer":true}],
            "millwright_refused":[55],"fixer_refused":[56,55]}"#,
        start,
    );
    import_ok(&h);
    assert_eq!(
        count(&h, "planning_notes WHERE kind = 'millwright-refused'"),
        2,
        "the ids of both lists, 55 once"
    );
    let p = &ok(&h, &["plan-show", "7"])["planning"];
    assert_eq!(
        p["notes"]["millwright_refused"],
        serde_json::json!([55, 56])
    );
    let cur = ok(&h, &["approval-current", "7"]);
    assert_eq!(cur["current"]["how"], "millwright");
    assert_eq!(cur["millwright_starts"], 2, "both spellings, one number");
}

#[test]
fn a_refresh_start_keeps_its_base_its_fixer_starts_and_refreshes() {
    let h = host("refresh");
    let start = t("2026-10-14T10:00:00Z");
    put(&h, "cycles/7.taken", "", start);
    // A refresh's at is its base's, so the row is given a time of its own.
    let record = format!(
        r#"{{"at":"2026-10-13T08:00:00Z","by":"bob","how":"refresh","body_sha256":"{SHA}","links":[],
            "rests_on":{{"at":"2026-10-13T08:00:00Z","by":"bob","how":"label","body_sha256":"{SHA}"}},
            "fixer_starts":1,"refreshes":2,"refresh_of":{{"pr":41,"head_sha":"{HEAD}","base_sha":"{HEAD}","branch":"main"}}}}"#
    );
    put(&h, "cycles/7.approved.json", &record, start - 1);
    import_ok(&h);
    let cur = ok(&h, &["approval-current", "7"]);
    assert_eq!(cur["current"]["how"], "refresh");
    assert_eq!(cur["current"]["refresh_pr"], 41);
    assert_eq!(cur["current"]["refresh_head"], HEAD);
    assert_eq!(cur["current"]["at"], start - 1);
    assert_eq!(cur["rests_on"]["at"], t("2026-10-13T08:00:00Z"));
    assert_eq!(cur["rests_on"]["by"], "bob");
    assert_eq!(cur["fixer_starts"], 1);
    assert_eq!(cur["refreshes"], 2);
    assert_eq!(count(&h, "approvals WHERE issue = 7"), 4);
}

#[test]
fn a_park_and_its_question_become_rows() {
    let h = host("park");
    let start = t("2026-10-14T10:00:00Z");
    put(&h, "cycles/7.taken", "", start);
    put(&h, "cycles/7.by", "alice\n", start);
    put(
        &h,
        "cycles/7.approved.json",
        &label_record("2026-10-14T09:59:00Z", "alice"),
        start - 1,
    );
    put(
        &h,
        "cycles/7.history.jsonl",
        &line("2026-10-14T11:00:00Z", start + 1, "failed"),
        start + 3600,
    );
    put(
        &h,
        "cycles/7.park",
        &format!(
            r#"{{"cap":"","reason":"the model went away","provider":true,"retry":true,"started":{start},"body_sha256":"{SHA}","held":true}}"#
        ),
        start + 3601,
    );
    put(&h, "park-7.id", "12345\n", start + 3602);
    put(
        &h,
        "park-7.json",
        r#"{"text":"t","question":null}"#,
        start + 3602,
    );
    let v = import_ok(&h);
    assert_eq!(v["imported"]["parks"], 1, "{v}");
    assert_eq!(v["imported"]["questions"], 1);
    let p = ok(&h, &["park-show", "7"]);
    assert_eq!(p["park"]["reason"], "provider-outage");
    assert_eq!(p["park"]["retries"], 0);
    assert_eq!(p["park"]["automatic"], true);
    assert_eq!(p["park"]["question"], 12345);
    assert_eq!(p["park"]["step"], "review");
    assert_eq!(p["park"]["pr"], 41);
    assert_eq!(p["park"]["started_by"], "alice");
    assert_eq!(
        one::<i64>(&h, "SELECT told_at FROM parks WHERE issue = 7"),
        start + 3601
    );
    assert_eq!(
        ok(&h, &["question-known", "7", "--id", "12345"])["known"],
        true
    );
    assert!(moved(&h, "park-7.id"));
    assert!(moved(&h, "park-7.json"));
    assert!(moved(&h, "cycles/7.park"));
}

#[test]
fn a_park_with_no_retry_owes_none_and_an_id_with_no_park_is_no_question() {
    let h = host("park-no");
    let start = t("2026-10-14T10:00:00Z");
    put(&h, "cycles/7.taken", "", start);
    put(
        &h,
        "cycles/7.park",
        r#"{"cap":"needs:review","reason":"","provider":false,"retry":false,"started":1}"#,
        start + 10,
    );
    put(&h, "cycles/8.taken", "", start);
    put(&h, "park-8.id", "999\n", start + 10);
    import_ok(&h);
    let p = ok(&h, &["park-show", "7"]);
    assert_eq!(p["park"]["reason"], "needs:review");
    assert_eq!(p["park"]["retries"], 1);
    assert_eq!(p["park"]["automatic"], false);
    // F5: only an id of a park on record becomes a question.
    assert_eq!(count(&h, "questions"), 0);
    assert!(moved(&h, "park-8.id"));
}

#[test]
fn quiet_notes_and_secret_settings_move_without_rows() {
    let h = host("quiet");
    let start = t("2026-10-14T10:00:00Z");
    put(&h, "cycles/7.taken", "", start);
    put(&h, "cycles/7.quiet", "123\n", start + 10);
    put(
        &h,
        "cycles/7.settings",
        r#"{"container":{"PI_ENSEMBLE_KEY":"ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},"host":{},"concurrentCycles":null}"#,
        start + 10,
    );
    import_ok(&h);
    assert_eq!(count(&h, "cycle_settings"), 0);
    assert_eq!(count(&h, "cycle_notes"), 0);
    assert!(moved(&h, "cycles/7.quiet"));
    assert!(moved(&h, "cycles/7.settings"));
}

// --- what is held ---------------------------------------------------------------------

#[test]
fn a_link_is_held_and_stays_with_its_issue_s_files() {
    let h = host("link");
    let start = t("2026-10-14T10:00:00Z");
    let target = h.join("elsewhere");
    fs::write(&target, "alice\n").unwrap();
    put(&h, "cycles/7.taken", "", start);
    std::os::unix::fs::symlink(&target, h.join("state/cycles/7.by")).unwrap();
    put(&h, "cycles/8.taken", "", start);
    let (code, why) = import(&h, false).unwrap_err();
    assert_eq!(code, 1);
    assert!(why.starts_with("held 2 files"), "{why}");
    assert!(
        why.contains("state/cycles/7.by: is not a regular file"),
        "{why}"
    );
    assert!(
        why.contains("state/cycles/7.taken: held with cycles/7.by"),
        "{why}"
    );
    assert!(there(&h, "cycles/7.by"));
    assert!(there(&h, "cycles/7.taken"));
    assert_eq!(fs::read_to_string(&target).unwrap(), "alice\n");
    assert_eq!(count(&h, "cycles WHERE issue = 7"), 0);
    // The other issue is imported.
    assert_eq!(count(&h, "cycles WHERE issue = 8"), 1);
    assert!(moved(&h, "cycles/8.taken"));
    // A directory or an oversized file is held the same way.
    let h = host("dir");
    fs::create_dir_all(h.join("state/cycles/7.park")).unwrap();
    put(
        &h,
        "cycles/9.history.jsonl",
        &"x".repeat((1 << 20) + 1),
        start,
    );
    let (code, why) = import(&h, true).unwrap_err();
    assert_eq!(code, 1);
    assert!(
        why.contains("state/cycles/7.park: is not a regular file"),
        "{why}"
    );
    assert!(
        why.contains("state/cycles/9.history.jsonl: is over"),
        "{why}"
    );
}

#[test]
fn a_time_that_does_not_parse_holds_its_file() {
    let h = host("time");
    let start = t("2026-10-14T10:00:00Z");
    put(&h, "cycles/7.taken", "", start);
    put(
        &h,
        "cycles/7.approved.json",
        &label_record("2026-10-14 09:59:00", "alice"),
        start,
    );
    put(
        &h,
        "plan-8.json",
        r#"{"session":"s","turns":1,"startedAt":"yesterday"}"#,
        start,
    );
    let (code, why) = import(&h, false).unwrap_err();
    assert_eq!(code, 1);
    assert!(
        why.contains("state/cycles/7.approved.json: has a at that is not a %FT%TZ time"),
        "{why}"
    );
    assert!(why.contains("state/plan-8.json: has a startedAt"), "{why}");
    assert!(there(&h, "cycles/7.approved.json"));
    assert!(there(&h, "plan-8.json"));
    assert_eq!(rows(&h), 0);
    for bad in [
        "2026-02-30T00:00:00Z",
        "2026-10-14T24:00:00Z",
        "2026-10-14T10:00:00.5Z",
        "2026-10-14T10:00:00+00:00",
    ] {
        assert_eq!(iso(bad), None, "{bad}");
    }
    assert_eq!(iso("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(iso("2024-02-29T12:00:00Z"), Some(1_709_208_000));
    assert_eq!(day(1_709_208_000), "20240229");
}

// --- idempotence ------------------------------------------------------------------------

#[test]
fn a_second_run_is_a_no_op_and_a_crash_before_the_move_is_finished_by_the_next() {
    let h = host("twice");
    let start = t("2026-10-14T10:00:00Z");
    put(&h, "cycles/7.taken", "", start);
    put(
        &h,
        "cycles/7.approved.json",
        &label_record("2026-10-14T09:59:00Z", "alice"),
        start,
    );
    put(
        &h,
        "cycles/7.history.jsonl",
        &line("2026-10-14T11:00:00Z", start + 1, "handoff"),
        start + 10,
    );
    import_ok(&h);
    let before = rows(&h);
    let again = import_ok(&h);
    assert!(
        again["imported"]
            .as_object()
            .unwrap()
            .values()
            .all(|n| n == 0),
        "{again}"
    );
    assert_eq!(again["moved"], 0);
    // The rows were committed and the files not moved: put them back as they were.
    let dir = h.join(format!("state/imported-{}", day(NOW)));
    for rel in [
        "cycles/7.taken",
        "cycles/7.approved.json",
        "cycles/7.history.jsonl",
    ] {
        fs::copy(dir.join(rel), h.join("state").join(rel)).unwrap();
    }
    put(&h, "cycles/7.taken", "", start);
    let third = import_ok(&h);
    assert!(
        third["imported"]
            .as_object()
            .unwrap()
            .values()
            .all(|n| n == 0),
        "{third}"
    );
    assert_eq!(third["moved"], 3);
    assert_eq!(rows(&h), before);
    // Nothing is replaced under imported/: the second copy takes the next name.
    assert!(dir.join("cycles/7.taken.1").exists());
    assert!(dir.join("cycles/7.taken").exists());
}

#[test]
fn a_file_whose_rows_do_not_read_back_stays_with_its_group() {
    let h = host("readback");
    let start = t("2026-10-14T10:00:00Z");
    // The id is already another issue's question, so the park's is not written.
    conn(&h)
        .execute_batch(
            "INSERT INTO issues (issue, kind, kind_at, checked_at) VALUES (99, 'unreadable', 0, 0);
             INSERT INTO questions (id, issue, attempt, kind, posted_at) VALUES (12345, 99, 1, 'plan', 1);",
        )
        .unwrap();
    put(&h, "cycles/7.taken", "", start);
    put(
        &h,
        "cycles/7.park",
        r#"{"cap":"x","provider":false,"retry":false}"#,
        start + 10,
    );
    put(&h, "park-7.id", "12345\n", start + 10);
    put(&h, "cycles/8.taken", "", start);
    for _ in 0..2 {
        let (code, why) = import(&h, false).unwrap_err();
        assert_eq!(code, 1);
        assert!(
            why.contains("state/park-7.id: its rows did not read back"),
            "{why}"
        );
        assert!(
            why.contains("state/cycles/7.park: held with park-7.id"),
            "{why}"
        );
        // The whole group stays, so the next run plans the same files again.
        for rel in ["park-7.id", "cycles/7.park", "cycles/7.taken"] {
            assert!(there(&h, rel), "{rel}");
        }
        assert!(moved(&h, "cycles/8.taken"));
    }
    assert_eq!(count(&h, "cycles WHERE issue = 7"), 1);
    assert_eq!(count(&h, "parks WHERE issue = 7"), 1);
}

/// What a run that stopped after the cycle's rows, before the rest, leaves:
/// the store holds the cycle; the files of the later kinds are still there.
fn three_lines(h: &Path, start: i64) {
    put(h, "cycles/7.taken", "", start);
    put(h, "cycles/7.by", "alice\n", start);
    put(
        h,
        "cycles/7.approved.json",
        &label_record("2026-10-14T09:59:00Z", "alice"),
        start - 1,
    );
    let history = [
        line("2026-10-14T10:10:00Z", start + 5, "failed"),
        line("2026-10-14T10:20:00Z", start + 5, "failed"),
        line("2026-10-14T10:30:00Z", start + 5, "handoff"),
    ]
    .join("\n");
    put(h, "cycles/7.history.jsonl", &history, start + 3600);
}

fn attempts(h: &Path) -> (i64, i64, Vec<i64>, Vec<i64>) {
    let c = conn(h);
    let (attempt, started) = c
        .query_row(
            "SELECT attempt, started_at FROM cycles WHERE issue = 7",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let list = |sql: &str| -> Vec<i64> {
        c.prepare(sql)
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    (
        attempt,
        started,
        list("SELECT attempt FROM cycle_history WHERE issue = 7 ORDER BY at"),
        list("SELECT used_by FROM approvals WHERE issue = 7 ORDER BY at"),
    )
}

#[test]
fn a_run_after_one_that_wrote_only_the_cycle_keeps_its_attempts() {
    let start = t("2026-10-14T10:00:00Z");
    let want = (3, start, vec![1, 2, 3], vec![3]);
    for start_file_left in [true, false] {
        let h = host(&format!("partial-{start_file_left}"));
        three_lines(&h, start);
        import_ok(&h);
        assert_eq!(attempts(&h), want);
        // The cycle's row stays; the later kinds' rows are gone and their
        // files back, with or without the start file.
        conn(&h)
            .execute_batch("DELETE FROM approvals; DELETE FROM cycle_history;")
            .unwrap();
        let dir = h.join(format!("state/imported-{}", day(NOW)));
        let mut back = vec!["cycles/7.approved.json", "cycles/7.history.jsonl"];
        if start_file_left {
            back.push("cycles/7.taken");
        }
        for rel in back {
            fs::rename(dir.join(rel), h.join("state").join(rel)).unwrap();
        }
        if start_file_left {
            put(&h, "cycles/7.taken", "", start);
        }
        let v = import_ok(&h);
        assert_eq!(v["imported"]["cycles"], 0, "{v}");
        assert_eq!(v["imported"]["cycle_history"], 3);
        assert_eq!(v["imported"]["approvals"], 1);
        assert_eq!(attempts(&h), want, "start file left: {start_file_left}");
    }
}

#[test]
fn a_file_left_with_no_start_belongs_to_the_row_on_record() {
    let h = host("leftover");
    let start = t("2026-10-14T10:00:00Z");
    put(&h, "cycles/7.taken", "", start);
    put(
        &h,
        "cycles/7.park",
        r#"{"cap":"x","provider":false,"retry":true}"#,
        start + 10,
    );
    import_ok(&h);
    // The id of the park, left behind by a run that stopped while it moved files.
    put(&h, "park-7.id", "4242\n", start + 10);
    let v = import_ok(&h);
    assert_eq!(v["imported"]["questions"], 1, "{v}");
    assert_eq!(ok(&h, &["park-show", "7"])["park"]["question"], 4242);
    assert!(moved(&h, "park-7.id"));
    // An approval left the same way is the row's start's.
    put(
        &h,
        "cycles/7.approved.json",
        &label_record("2026-10-14T09:59:00Z", "alice"),
        start - 1,
    );
    import_ok(&h);
    assert_eq!(
        one::<i64>(&h, "SELECT used_by FROM approvals WHERE issue = 7"),
        1
    );
}

#[test]
fn a_dry_run_counts_and_writes_nothing() {
    let h = host("dry");
    let start = t("2026-10-14T10:00:00Z");
    put(&h, "cycles/7.taken", "", start);
    put(
        &h,
        "cycles/7.approved.json",
        &label_record("2026-10-14T09:59:00Z", "alice"),
        start,
    );
    put(
        &h,
        "plan-8.json",
        r#"{"session":"s","turns":1,"by":"alice","label":"tehas:plan"}"#,
        start,
    );
    let v = import(&h, true).unwrap();
    assert_eq!(v["imported"]["cycles"], 1, "{v}");
    assert_eq!(v["imported"]["approvals"], 1);
    assert_eq!(v["imported"]["planning"], 1);
    assert_eq!(v["moved"], 3);
    assert_eq!(rows(&h), 0);
    assert!(there(&h, "cycles/7.taken"));
    assert!(!there(&h, &format!("imported-{}", day(NOW))));
    assert_eq!(count(&h, "meta WHERE key = 'imported_at'"), 0);
}

#[test]
fn an_open_row_is_ended_by_a_newer_line_of_history() {
    let h = host("open");
    approve(&h, 7, "alice", "label");
    let lock = hold(&h, 7);
    ok(&h, &["cycle-start", "7"]);
    drop(lock);
    // The row is open; the files of an older tehas say the cycle ended after it began.
    put(
        &h,
        "cycles/7.history.jsonl",
        &line("2026-10-14T18:00:00Z", NOW - 60, "merged"),
        NOW + 600,
    );
    import_ok(&h);
    let (attempt, reason, ended): (i64, String, i64) = conn(&h)
        .query_row(
            "SELECT attempt, end_reason, ended_at FROM cycles WHERE issue = 7",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        (attempt, reason.as_str(), ended),
        (1, "merged", t("2026-10-14T18:00:00Z"))
    );
    assert_eq!(
        one::<i64>(&h, "SELECT attempt FROM cycle_history WHERE issue = 7"),
        1
    );
}

#[test]
fn a_later_start_of_an_older_tehas_replaces_the_row_on_record() {
    let h = host("later");
    let first = t("2026-10-14T10:00:00Z");
    put(&h, "cycles/7.taken", "", first);
    put(
        &h,
        "cycles/7.history.jsonl",
        &line("2026-10-14T11:00:00Z", first + 1, "failed"),
        first + 10,
    );
    import_ok(&h);
    // A cycle that ran the old code after the import left its files again.
    let second = t("2026-10-14T12:00:00Z");
    put(&h, "cycles/7.taken", "", second);
    put(
        &h,
        "cycles/7.history.jsonl",
        &line("2026-10-14T13:00:00Z", second + 1, "handoff"),
        second + 10,
    );
    import_ok(&h);
    let (attempt, started, reason): (i64, i64, String) = conn(&h)
        .query_row(
            "SELECT attempt, started_at, end_reason FROM cycles WHERE issue = 7",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((attempt, started, reason.as_str()), (2, second, "handoff"));
    assert_eq!(count(&h, "cycle_history WHERE issue = 7"), 2);
}

// --- planning ------------------------------------------------------------------------------

#[test]
fn a_planning_record_and_its_sidecars_become_rows() {
    let h = host("plan");
    let start = t("2026-10-14T10:00:00Z");
    let record = r#"{"session":"sess-1","turns":2,"options":["Yes","No"],"pending":true,"failed":false,
        "running":false,"startedAt":"2026-10-14T10:00:00Z","comment":777,"by":"alice","label":"tehas:plan",
        "answers":[{"turn":1,"by":"alice","fixer":false},{"turn":2,"by":"fixerbot","fixer":true}],
        "filed":["https://github.com/acme/other/issues/5","not a url"],"fixer_refused":[55],"held":[66]}"#;
    put(&h, "plan-8.json", record, start + 100);
    put(&h, "plan-8.json.brief", "# Title\n\nBody\n", start - 100);
    put(&h, "plan-8.json.answer", "Go with yes\n", start);
    put(&h, "plan-8.json.out", "{}", start + 50);
    put(&h, "plan-8.json.turn", "{}", start + 50);
    put(&h, "plan-8.json.lock", "", start);
    let v = import_ok(&h);
    assert_eq!(v["imported"]["planning"], 1, "{v}");
    assert_eq!(v["imported"]["planning_answers"], 2);
    assert_eq!(v["imported"]["planning_filed"], 1);
    assert_eq!(v["imported"]["planning_notes"], 2);
    assert_eq!(v["imported"]["questions"], 1);
    let p = &ok(&h, &["plan-show", "8"])["planning"];
    assert_eq!(p["session"], "sess-1");
    assert_eq!(p["turns"], 2);
    assert_eq!(p["options"], serde_json::json!(["Yes", "No"]));
    assert_eq!(p["pending"], true);
    assert_eq!(p["pending_outcome"], "state/plan-8.json.out");
    assert_eq!(p["started_at"], start);
    assert_eq!(p["comment_id"], 777);
    assert_eq!(p["by"], "alice");
    assert_eq!(p["label"], "tehas:plan");
    assert_eq!(p["brief"], "# Title\n\nBody\n");
    assert_eq!(p["opened_at"], start - 100);
    assert_eq!(p["ended_at"], Value::Null);
    assert_eq!(
        p["filed"],
        serde_json::json!(["https://github.com/acme/other/issues/5"])
    );
    assert_eq!(
        ok(&h, &["question-known", "8", "--id", "777"])["known"],
        true
    );
    assert_eq!(
        one::<String>(
            &h,
            "SELECT text FROM planning_answers WHERE issue = 8 AND turn = 2"
        ),
        "Go with yes\n"
    );
    assert_eq!(
        count(
            &h,
            "planning_answers WHERE issue = 8 AND turn = 1 AND text IS NULL"
        ),
        1
    );
    assert_eq!(
        count(
            &h,
            "planning_notes WHERE kind = 'millwright-refused' AND comment_id = 55"
        ),
        1
    );
    // The answer of the millwright is marked under the new column, and the old
    // record's `fixer` mark is read as the same thing.
    assert_eq!(
        count(
            &h,
            "planning_answers WHERE issue = 8 AND turn = 2 AND millwright = 1"
        ),
        1
    );
    assert_eq!(
        count(&h, "planning_notes WHERE kind = 'held' AND comment_id = 66"),
        1
    );
    for rel in [
        "plan-8.json",
        "plan-8.json.brief",
        "plan-8.json.answer",
        "plan-8.json.turn",
    ] {
        assert!(moved(&h, rel), "{rel}");
    }
    // The carriers stay.
    assert!(there(&h, "plan-8.json.out"));
    assert!(there(&h, "plan-8.json.lock"));
}

#[test]
fn plan_history_comes_from_both_paths_once() {
    let h = host("plan-history");
    let a = r#"{"kind":"plan","at":"2026-10-14T10:00:00Z","started":"1792000000","rc":"0","status":"finished","turn":1,"by":"alice","label":"tehas:plan","filed":["https://github.com/acme/widget/issues/9"]}"#;
    let b = r#"{"kind":"plan","at":"2026-10-14T11:00:00Z","rc":"not started","status":"not-started","turn":2,"by":"alice","label":"tehas:plan"}"#;
    put(
        &h,
        "cycles/plan-8.history.jsonl",
        &format!("{a}\n{b}\n"),
        NOW,
    );
    put(&h, "plan-8.history.jsonl", &format!("{a}\n"), NOW);
    let v = import_ok(&h);
    assert_eq!(v["imported"]["plan_history"], 2, "{v}");
    type Row = (i64, String, Option<i64>, Option<i64>, Option<String>);
    let got: Vec<Row> = conn(&h)
        .prepare(
            "SELECT turn, outcome, rc, started_at, filed_json FROM plan_history WHERE issue = 8 ORDER BY at",
        )
        .unwrap()
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        got,
        [
            (
                1,
                "finished".into(),
                Some(0),
                Some(1_792_000_000),
                Some(r#"["https://github.com/acme/widget/issues/9"]"#.into())
            ),
            (2, "not-started".into(), None, None, None)
        ]
    );
    assert!(moved(&h, "cycles/plan-8.history.jsonl"));
    assert!(moved(&h, "plan-8.history.jsonl"));
}

#[test]
fn planning_options_are_kept_as_plan_end_keeps_them() {
    let h = host("options");
    let long = "x".repeat(500);
    let mut options: Vec<Value> = vec![
        Value::String("a\u{1b}[31mred\nline".into()),
        Value::String(long),
        Value::Null,
        Value::String(String::new()),
    ];
    options.extend((0..30).map(|i| Value::String(format!("o{i}"))));
    let record = serde_json::json!({"session":"s","turns":1,"options":options,"by":"alice","label":"tehas:plan"});
    put(&h, "plan-8.json", &record.to_string(), NOW);
    import_ok(&h);
    let got = ok(&h, &["plan-show", "8"])["planning"]["options"].clone();
    let got: Vec<String> = serde_json::from_value(got).unwrap();
    assert_eq!(got.len(), 20);
    assert_eq!(got[0], "a[31mredline");
    assert_eq!(got[1], "x".repeat(300));
    assert_eq!((got[2].as_str(), got[3].as_str()), ("-", "-"));
    assert_eq!(got[19], "o15");
    assert!(moved(&h, "plan-8.json"));
}

fn at(i: i64) -> String {
    format!("2026-10-14T{:02}:{:02}:00Z", 10 + i / 60, i % 60)
}

#[test]
fn a_history_keeps_the_fifty_newest_rows() {
    let h = host("bound");
    let plan = |i: i64| {
        format!(
            r#"{{"at":"{}","rc":"0","status":"finished","turn":{},"by":"alice","label":"tehas:plan"}}"#,
            at(i),
            i + 1
        )
    };
    let older: Vec<String> = (0..50).map(plan).collect();
    let newer: Vec<String> = (50..100).map(plan).collect();
    put(&h, "plan-8.history.jsonl", &older.join("\n"), NOW);
    put(&h, "cycles/plan-8.history.jsonl", &newer.join("\n"), NOW);
    let lines: Vec<String> = (0..50)
        .map(|i| line(&at(i), t(&at(i)) - 30, "failed"))
        .collect();
    put(&h, "cycles/7.history.jsonl", &lines.join("\n"), NOW);
    let v = import_ok(&h);
    assert_eq!(v["imported"]["plan_history"], 50, "{v}");
    assert_eq!(count(&h, "plan_history WHERE issue = 8"), 50);
    assert_eq!(
        one::<i64>(&h, "SELECT min(turn) FROM plan_history WHERE issue = 8"),
        51
    );
    assert_eq!(count(&h, "cycle_history WHERE issue = 7"), 50);
    // A cycle that slipped through leaves one more line: still 50, the oldest gone.
    put(
        &h,
        "cycles/7.history.jsonl",
        &line(&at(70), t(&at(70)) - 30, "handoff"),
        NOW,
    );
    import_ok(&h);
    assert_eq!(count(&h, "cycle_history WHERE issue = 7"), 50);
    assert_eq!(
        count(
            &h,
            &format!("cycle_history WHERE issue = 7 AND at = {}", t(&at(0)))
        ),
        0
    );
    assert_eq!(
        one::<i64>(&h, "SELECT max(attempt) FROM cycle_history WHERE issue = 7"),
        51
    );
    for rel in [
        "plan-8.history.jsonl",
        "cycles/plan-8.history.jsonl",
        "cycles/7.history.jsonl",
    ] {
        assert!(moved(&h, rel), "{rel}");
    }
}

#[test]
fn the_verb_is_parsed_with_its_flags_only() {
    let parse = |args: &[&str]| crate::parse(&strings(args));
    assert_eq!(parse(&["import"]), Some(Verb::Import { dry_run: false }));
    assert_eq!(
        parse(&["import", "--dry-run", "--json"]),
        Some(Verb::Import { dry_run: true })
    );
    assert_eq!(parse(&["import", "--dry-run", "--dry-run"]), None);
    assert_eq!(parse(&["import", "--force"]), None);
    assert_eq!(parse(&["import", "7"]), None);
}
