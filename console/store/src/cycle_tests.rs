//! cycle-start, cycle-image, cycle-end, cycle-show, cycle-note and
//! cycle-stop-asked.

use crate::cycle::{secret, END_REASONS};
use crate::lock;
use crate::verbs_tests::*;
use serde_json::{json, Value};
use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn attempt_args(issue: i64, attempt: i64) -> (String, String) {
    (issue.to_string(), attempt.to_string())
}

// --- cycle-start ----------------------------------------------------------------

#[test]
fn cycle_start_records_the_approval_s_account_and_uses_it() {
    let home = home("start");
    let at = approve(&home, 10, "alice", "label");
    let _lock = hold(&home, 10);
    let v = ok(&home, &["cycle-start", "10", "--rukas", "abcdef0"]);
    assert_eq!(v["issue"], 10);
    assert_eq!(v["attempt"], 1);
    assert_eq!(v["started_at"], NOW);
    assert_eq!(v["started_by"], "alice");
    assert_eq!(v["how"], "label");
    assert_eq!(v["requeued"], false);
    assert_eq!(v["approval_at"], at);
    assert_eq!(v["replaced_unended"], false);
    let c = conn(&home);
    let row: (i64, i64, String, String, i64, Option<String>, Option<i64>) = c
        .query_row(
            "SELECT attempt, started_at, started_by, how, requeued, rukas_commit, ended_at FROM cycles WHERE issue = 10",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
        )
        .unwrap();
    assert_eq!(
        row,
        (
            1,
            NOW,
            "alice".into(),
            "label".into(),
            0,
            Some("abcdef0".into()),
            None
        )
    );
    let (used_at, used_by): (i64, i64) = c
        .query_row(
            "SELECT used_at, used_by FROM approvals WHERE issue = 10",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((used_at, used_by), (NOW, 1));
    // The issues stub is there and says nobody has read the issue.
    let (kind, kind_at, checked_at): (String, i64, i64) = c
        .query_row(
            "SELECT kind, kind_at, checked_at FROM issues WHERE issue = 10",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((kind.as_str(), kind_at, checked_at), ("unreadable", 0, 0));
}

#[test]
fn the_issues_stub_never_touches_a_real_row() {
    let home = home("stub-real");
    conn(&home)
        .execute(
            "INSERT INTO issues (issue, kind, kind_at, title, checked_at) VALUES (11, 'issue', 5, 'Title', 6)",
            [],
        )
        .unwrap();
    let (_lock, _) = start(&home, 11);
    assert_eq!(
        one::<String>(
            &home,
            "SELECT kind || title || kind_at || checked_at FROM issues WHERE issue = 11"
        ),
        "issueTitle56"
    );
}

#[test]
fn cycle_start_refuses_bad_arguments() {
    for args in [
        &["cycle-start"][..],
        &["cycle-start", "x"][..],
        &["cycle-start", "1", "--rukas", "ZZZZZZZ"][..],
        &["cycle-start", "1", "--rukas", "abc"][..],
        &["cycle-start", "1", "--by"][..],
        &["cycle-start", "1", "--image", IMAGE][..],
    ] {
        assert!(usage(args).starts_with("usage cycle-start: "), "{args:?}");
    }
}

#[test]
fn cycle_start_with_the_lock_free_leaves_the_approval_unused() {
    let home = home("start-free");
    approve(&home, 12, "alice", "label");
    assert_eq!(
        refused(&home, &["cycle-start", "12"]),
        (1, "unlocked".into())
    );
    // A lock file nobody holds is free too.
    let f = hold(&home, 12);
    drop(f);
    assert_eq!(
        refused(&home, &["cycle-start", "12"]),
        (1, "unlocked".into())
    );
    assert_eq!(count(&home, "approvals WHERE used_at IS NULL"), 1);
    assert_eq!(count(&home, "cycles"), 0);
}

#[test]
fn cycle_start_says_probe_when_the_lock_cannot_be_told() {
    let home = home("start-probe");
    approve(&home, 13, "alice", "label");
    fs::create_dir_all(lock::path(&home, 13)).unwrap();
    assert_eq!(refused(&home, &["cycle-start", "13"]), (1, "probe".into()));
    assert_eq!(count(&home, "approvals WHERE used_at IS NULL"), 1);
}

#[test]
fn cycle_start_marks_the_approval_used_by_its_attempt_in_one_transaction() {
    let home = home("start-attempts");
    let (lock1, a1) = start(&home, 14);
    end(&home, 14, a1, "closed");
    drop(lock1);
    let (_lock2, a2) = start(&home, 14);
    assert_eq!((a1, a2), (1, 2));
    assert_eq!(
        one::<i64>(
            &home,
            "SELECT used_by FROM approvals WHERE issue = 14 ORDER BY used_by DESC LIMIT 1"
        ),
        2
    );
    assert_eq!(
        one::<i64>(&home, "SELECT attempt FROM cycles WHERE issue = 14"),
        2
    );
}

#[test]
fn cycle_start_that_is_busy_uses_nothing() {
    // The verb has to meet the writer's bound rather than beat the holder, so
    // this thread asks for the short pair.
    short_bounds();
    let home = home("start-busy");
    approve(&home, 15, "alice", "label");
    let _lock = hold(&home, 15);
    let holder = writer_lock(&home);
    assert_eq!(refused(&home, &["cycle-start", "15"]), (1, "busy".into()));
    drop(holder);
    assert_eq!(count(&home, "approvals WHERE used_at IS NULL"), 1);
    assert_eq!(count(&home, "cycles"), 0);
}

#[test]
fn a_second_cycle_start_without_a_new_approval_is_unapproved() {
    let home = home("start-twice");
    let (_lock, a) = start(&home, 16);
    end(&home, 16, a, "closed");
    assert_eq!(
        refused(&home, &["cycle-start", "16"]),
        (1, "unapproved".into())
    );
    assert_eq!(
        one::<i64>(&home, "SELECT attempt FROM cycles WHERE issue = 16"),
        1
    );
}

#[test]
fn cycle_start_uses_the_unused_row_whatever_its_at() {
    let home = home("start-any-at");
    // An unused approval whose at is older than a used one.
    let (_lock, a) = start(&home, 17);
    end(&home, 17, a, "closed");
    ok(
        &home,
        &[
            "approval-record",
            "17",
            "--at",
            "1000",
            "--by",
            "bob",
            "--how",
            "label",
            "--label",
            "tehas:ready",
            "--body-sha",
            SHA,
        ],
    );
    let v = ok(&home, &["cycle-start", "17"]);
    assert_eq!(v["approval_at"], 1000);
    assert_eq!(v["started_by"], "bob");
}

#[test]
fn the_attempt_counts_past_history() {
    let home = home("start-history");
    conn(&home)
        .execute_batch(
            "INSERT INTO cycle_history (issue, attempt, at, status) VALUES (18, 6, 1, 'closed');",
        )
        .unwrap();
    let (_lock, a) = start(&home, 18);
    assert_eq!(a, 7);
}

#[test]
fn a_run_by_hand_keeps_its_records_and_touches_no_approval() {
    let home = home("by-hand");
    let at = approve(&home, 19, "alice", "label");
    let _lock = hold(&home, 19);
    let v = ok(&home, &["cycle-start", "19", "--by-hand"]);
    assert_eq!(v["how"], "hand");
    assert_eq!(v["started_by"], Value::Null);
    assert_eq!(v["approval_at"], Value::Null);
    assert_eq!(
        one::<Option<i64>>(
            &home,
            &format!("SELECT used_at FROM approvals WHERE issue = 19 AND at = {at}")
        ),
        None
    );
    // Without any approval at all.
    let _lock = hold(&home, 20);
    assert_eq!(ok(&home, &["cycle-start", "20", "--by-hand"])["attempt"], 1);
}

#[test]
fn an_outage_retry_start_is_requeued() {
    let home = home("requeued");
    let (_lock, _) = start_by(&home, 21, "alice", "outage-retry", NOW);
    assert_eq!(
        one::<i64>(&home, "SELECT requeued FROM cycles WHERE issue = 21"),
        1
    );
}

#[test]
fn a_start_over_an_unended_row_says_so_and_replaces_it() {
    let home = home("unended");
    let (_lock, a) = start(&home, 22);
    approve(&home, 22, "alice", "label");
    let v = ok(&home, &["cycle-start", "22"]);
    assert_eq!(v["replaced_unended"], true);
    assert_eq!(v["attempt"], a + 1);
    assert_eq!(count(&home, "cycles WHERE issue = 22"), 1);
}

#[test]
fn cycle_start_clears_the_previous_attempt_s_settings_notes_locks_results_and_progress() {
    let home = home("start-clears");
    let (_lock, a) = start(&home, 23);
    let c = conn(&home);
    c.execute_batch(
        "INSERT INTO cycle_settings VALUES (23, 'PI_ENSEMBLE_X', '1');
         INSERT INTO cycle_notes VALUES (23, 'quiet', 'x', 1);
         INSERT INTO cycle_locks VALUES (23, 1, 1, 1);
         INSERT INTO cycle_results VALUES (23, 1, 1, '{}');
         INSERT INTO cycle_progress VALUES (23, 1, NULL, NULL, NULL, NULL, 1);",
    )
    .unwrap();
    end(&home, 23, a, "closed");
    approve(&home, 23, "alice", "label");
    ok(&home, &["cycle-start", "23"]);
    for t in [
        "cycle_settings",
        "cycle_notes",
        "cycle_locks",
        "cycle_results",
        "cycle_progress",
    ] {
        assert_eq!(count(&home, &format!("{t} WHERE issue = 23")), 0, "{t}");
    }
}

#[test]
fn cycle_start_keeps_earlier_heartbeats_and_history() {
    let home = home("start-keeps");
    let (_lock, a) = start(&home, 24);
    end(&home, 24, a, "closed");
    approve(&home, 24, "alice", "label");
    ok_at(&home, NOW + 10, &["cycle-start", "24"]);
    assert_eq!(count(&home, "heartbeats WHERE issue = 24"), 1);
    assert_eq!(count(&home, "cycle_history WHERE issue = 24"), 1);
    assert_eq!(count(&home, "endings WHERE issue = 24"), 1);
}

#[test]
fn cycle_start_records_the_commit_it_was_given() {
    let home = home("start-commit");
    approve(&home, 25, "alice", "label");
    let _lock = hold(&home, 25);
    ok(
        &home,
        &[
            "cycle-start",
            "25",
            "--rukas",
            "0123456789abcdef0123456789abcdef01234567",
        ],
    );
    assert_eq!(
        one::<String>(&home, "SELECT rukas_commit FROM cycles WHERE issue = 25"),
        "0123456789abcdef0123456789abcdef01234567"
    );
}

// --- cycle-image ----------------------------------------------------------------

fn image(home: &Path, issue: i64, attempt: i64, extra: &[&str]) -> Result<Value, (u8, String)> {
    let (n, a) = attempt_args(issue, attempt);
    let mut args = vec!["cycle-image", &n, "--attempt", &a, "--image", IMAGE];
    args.extend(extra);
    call(home, &args)
}

fn settings_file(home: &Path, body: &str) -> String {
    write_file(home, "settings.json", body.as_bytes())
        .to_str()
        .unwrap()
        .to_string()
}

const GOOD_SETTINGS: &str = r#"{"container":{"PI_ENSEMBLE_MODEL":"m1","PI_ENSEMBLE_TOKEN":"[withheld]"},"host":{"TEHAS_CYCLES":"2"},"concurrentCycles":3}"#;

#[test]
fn cycle_image_records_image_commit_and_settings_at_container_start() {
    let home = home("image");
    let (_lock, a) = start(&home, 30);
    let s = settings_file(&home, GOOD_SETTINGS);
    let v = image(&home, 30, a, &["--rukas", "abcdef0", "--settings", &s]).unwrap();
    assert_eq!(v["image"], IMAGE);
    assert_eq!(v["rukas_commit"], "abcdef0");
    assert_eq!(v["settings"], 4);
    let show = ok(&home, &["cycle-show", "30"]);
    assert_eq!(show["cycle"]["image"], IMAGE);
    assert_eq!(
        show["cycle"]["settings"],
        json!({"PI_ENSEMBLE_MODEL":"m1","PI_ENSEMBLE_TOKEN":"[withheld]","TEHAS_CYCLES":"2","concurrentCycles":"3"})
    );
    // A second call replaces the settings rather than adding to them.
    let s = settings_file(&home, r#"{"container":{},"host":{"TEHAS_X":"1"}}"#);
    assert_eq!(
        image(&home, 30, a, &["--settings", &s]).unwrap()["settings"],
        1
    );
    assert_eq!(count(&home, "cycle_settings WHERE issue = 30"), 1);
    // Without --rukas the commit stays.
    assert_eq!(image(&home, 30, a, &[]).unwrap()["rukas_commit"], "abcdef0");
}

#[test]
fn cycle_image_records_the_image_when_settings_are_refused() {
    let home = home("image-refused");
    let (_lock, a) = start(&home, 31);
    for body in [
        "not json",
        "[]",
        r#"{"container":{"OTHER":"x"},"host":{}}"#,
        r#"{"container":{"PI_ENSEMBLE_X":1},"host":{}}"#,
        r#"{"container":{},"host":{"PATH":"x"}}"#,
        r#"{"container":{},"host":{},"extra":1}"#,
        r#"{"container":{},"host":{},"concurrentCycles":1.5}"#,
        r#"{"host":{}}"#,
    ] {
        let s = settings_file(&home, body);
        let v = image(&home, 31, a, &["--settings", &s]).unwrap();
        assert_eq!(v["settings"], "refused", "{body}");
        assert_eq!(v["image"], IMAGE);
    }
    let big = format!(
        r#"{{"container":{{"PI_ENSEMBLE_X":"{}"}},"host":{{}}}}"#,
        "x".repeat(4097)
    );
    let s = settings_file(&home, &big);
    assert_eq!(
        image(&home, 31, a, &["--settings", &s]).unwrap()["settings"],
        "refused"
    );
    let s = settings_file(&home, &" ".repeat(65 * 1024));
    assert_eq!(
        image(&home, 31, a, &["--settings", &s]).unwrap()["settings"],
        "refused"
    );
    assert_eq!(count(&home, "cycle_settings"), 0);
    assert_eq!(
        one::<String>(&home, "SELECT image FROM cycles WHERE issue = 31"),
        IMAGE
    );
}

#[test]
fn cycle_image_refuses_each_of_the_six_secret_shapes() {
    let home = home("image-secret");
    let (_lock, a) = start(&home, 32);
    let shapes = [
        format!("ghp_{}", "A".repeat(36)),
        format!("github_pat_{}", "a_".repeat(11)),
        format!("x sk-{}", "a".repeat(20)),
        format!("xoxb-{}", "1".repeat(10)),
        format!("AKIA{}", "ABCDEFGH12345678"),
        "-----BEGIN RSA PRIVATE KEY-----".to_string(),
    ];
    for shape in &shapes {
        assert!(secret(shape.as_bytes()), "{shape}");
        let body = format!(r#"{{"container":{{"PI_ENSEMBLE_X":"{shape}"}},"host":{{}}}}"#);
        let s = settings_file(&home, &body);
        assert_eq!(
            image(&home, 32, a, &["--settings", &s]).unwrap()["settings"],
            "secret",
            "{shape}"
        );
    }
    // An escaped secret is found in the decoded value.
    let s = settings_file(
        &home,
        r#"{"container":{"PI_ENSEMBLE_X":"\u0041KIAABCDEFGH12345678"},"host":{}}"#,
    );
    assert_eq!(
        image(&home, 32, a, &["--settings", &s]).unwrap()["settings"],
        "secret"
    );
    for near in [
        format!("ghp_{}", "A".repeat(35)),
        format!("xsk-{}", "a".repeat(20)),
        format!("AKIA{}", "abcdefgh12345678"),
        "-----BEGIN PUBLIC KEY-----".to_string(),
        "-----BEGIN PRIVATE KEY----".to_string(),
    ] {
        assert!(!secret(near.as_bytes()), "{near}");
    }
    assert!(secret(b"-----BEGIN PRIVATE KEY-----"));
    assert_eq!(count(&home, "cycle_settings"), 0);
}

#[test]
fn a_result_or_settings_file_that_is_a_link_is_refused() {
    let home = home("links");
    let (_lock, a) = start(&home, 33);
    let real = settings_file(&home, GOOD_SETTINGS);
    let link = home.join("settings-link.json");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let v = image(&home, 33, a, &["--settings", link.to_str().unwrap()]).unwrap();
    assert_eq!(v["settings"], "refused");
    let result = write_file(&home, "r.json", br#"{"schemaVersion":1}"#);
    let rlink = home.join("result-link.json");
    std::os::unix::fs::symlink(&result, &rlink).unwrap();
    let (n, at) = attempt_args(33, a);
    let v = ok(
        &home,
        &[
            "cycle-end",
            &n,
            "--attempt",
            &at,
            "--status",
            "closed",
            "--result",
            rlink.to_str().unwrap(),
        ],
    );
    assert_eq!(v["result"], "refused");
    assert_eq!(count(&home, "cycle_results"), 0);
}

#[test]
fn cycle_image_states_its_preconditions() {
    let home = home("image-pre");
    assert_eq!(image(&home, 34, 1, &[]).unwrap_err().0, 1);
    assert_eq!(
        refused(
            &home,
            &["cycle-image", "34", "--attempt", "1", "--image", IMAGE]
        ),
        (1, "nocycle".into())
    );
    let (lock34, a) = start(&home, 34);
    let (n, wrong) = attempt_args(34, a + 1);
    assert_eq!(
        refused(
            &home,
            &["cycle-image", &n, "--attempt", &wrong, "--image", IMAGE]
        ),
        (1, "attempt".into())
    );
    drop(lock34);
    let (_, at) = attempt_args(34, a);
    assert_eq!(
        refused(
            &home,
            &["cycle-image", &n, "--attempt", &at, "--image", IMAGE]
        ),
        (1, "unlocked".into())
    );
    fs::remove_file(lock::path(&home, 34)).unwrap();
    assert_eq!(
        refused(
            &home,
            &["cycle-image", &n, "--attempt", &at, "--image", IMAGE]
        ),
        (1, "probe".into())
    );
    let _l = hold(&home, 34);
    end(&home, 34, a, "closed");
    assert_eq!(
        refused(
            &home,
            &["cycle-image", &n, "--attempt", &at, "--image", IMAGE]
        ),
        (1, "ended".into())
    );
    for args in [
        &["cycle-image", "34", "--attempt", "1"][..],
        &["cycle-image", "34", "--image", IMAGE][..],
        &["cycle-image", "34", "--attempt", "1", "--image", SHA][..],
        &[
            "cycle-image",
            "34",
            "--attempt",
            "1",
            "--image",
            IMAGE,
            "--settings",
            "rel.json",
        ][..],
    ] {
        assert!(usage(args).starts_with("usage cycle-image: "), "{args:?}");
    }
}

// --- cycle-end ------------------------------------------------------------------

#[test]
fn cycle_end_records_the_end_its_history_ending_and_heartbeat() {
    let home = home("end");
    let (_lock, a) = start(&home, 40);
    let (n, at) = attempt_args(40, a);
    let v = ok_at(
        &home,
        NOW + 600,
        &[
            "cycle-end",
            &n,
            "--attempt",
            &at,
            "--status",
            "handoff",
            "--step",
            "ci",
            "--rc",
            "10",
            "--runner-exit",
            "0",
            "--host-saw",
            "killed",
            "--oom",
            "2",
            "--forks",
            "3",
            "--stalled",
            "0",
            "--timed",
            "--branch",
            "feature/issue-40-thing",
            "--pr",
            "99",
            "--cap",
            "awaiting-human-merge",
            "--park-reason",
            "x",
            "--failure-kind",
            "provider",
            "--failure-cause",
            "rate:limit",
            "--provider",
            "--parked",
        ],
    );
    let c = &v["cycle"];
    assert_eq!(c["end_reason"], "handoff");
    assert_eq!(c["ended_at"], NOW + 600);
    assert_eq!(c["oom_kills"], 2);
    assert_eq!(c["forks_refused"], 3);
    assert_eq!(c["stalled_seconds"], 0);
    assert_eq!(c["timed_out"], true);
    assert_eq!(c["runner_exit"], 0);
    assert_eq!(c["watched"], true);
    assert_eq!(c["host_saw"], "killed");
    assert_eq!(c["branch"], "feature/issue-40-thing");
    assert_eq!(v["already"], false);
    assert_eq!(v["result"], Value::Null);
    assert_eq!(v["progress"], Value::Null);
    assert_eq!(v["cleaned"], json!([]));
    let id: i64 = one(&home, "SELECT id FROM cycle_history WHERE issue = 40");
    assert_eq!(v["history_id"], id);
    let hist: String = one(
        &home,
        "SELECT concat_ws(' ', attempt, status, step, by_account, how, pr, started_at, rc, cap,
           park_reason, failure_kind, failure_cause, provider, parked)
         FROM cycle_history WHERE issue = 40",
    );
    assert_eq!(
        hist,
        format!(
            "1 handoff ci alice label 99 {NOW} 10 awaiting-human-merge x provider rate:limit 1 1"
        )
    );
    assert_eq!(
        one::<String>(&home, "SELECT started_at || ' ' || ended_at || ' ' || status || ' ' || step FROM endings WHERE issue = 40"),
        format!("{NOW} {} handoff ci", NOW + 600)
    );
    assert_eq!(
        one::<String>(&home, "SELECT at || ' ' || running || ' ' || step || ' ' || status FROM heartbeats WHERE issue = 40"),
        format!("{} 0 ci handoff", NOW + 600)
    );
}

#[test]
fn watched_follows_the_runner_exit() {
    let home = home("watched");
    let (_l, a) = start(&home, 41);
    let (n, at) = attempt_args(41, a);
    let v = ok(
        &home,
        &[
            "cycle-end",
            &n,
            "--attempt",
            &at,
            "--status",
            "closed",
            "--runner-exit",
            "3",
        ],
    );
    assert_eq!(v["cycle"]["watched"], true);
    let (_l2, b) = start(&home, 42);
    let (n, at) = attempt_args(42, b);
    let v = ok(
        &home,
        &["cycle-end", &n, "--attempt", &at, "--status", "closed"],
    );
    assert_eq!(v["cycle"]["watched"], false);
    let v = ok(&home, &["cycle-show", "41"]);
    assert_eq!(v["cycle"]["runner_exit"], 3);
}

#[test]
fn cycle_end_refuses_bad_arguments() {
    for args in [
        &["cycle-end", "1", "--attempt", "1"][..],
        &["cycle-end", "1", "--status", "closed"][..],
        &["cycle-end", "1", "--attempt", "x", "--status", "closed"][..],
        &[
            "cycle-end",
            "1",
            "--attempt",
            "1",
            "--status",
            "closed",
            "--rc",
            "-1",
        ][..],
        &[
            "cycle-end",
            "1",
            "--attempt",
            "1",
            "--status",
            "closed",
            "--rc",
            "1000000000",
        ][..],
        &[
            "cycle-end",
            "1",
            "--attempt",
            "1",
            "--status",
            "closed",
            "--oom",
            "x",
        ][..],
        &[
            "cycle-end",
            "1",
            "--attempt",
            "1",
            "--status",
            "closed",
            "--pr",
            "0",
        ][..],
        &[
            "cycle-end",
            "1",
            "--attempt",
            "1",
            "--status",
            "closed",
            "--result",
            "r.json",
        ][..],
        &[
            "cycle-end",
            "1",
            "--attempt",
            "1",
            "--status",
            "closed",
            "--refused",
        ][..],
        &["cycle-end", "1", "--attempt", "1", "--refused", "--stopped"][..],
        &["cycle-end", "1", "--attempt", "1", "--stopped", "--watched"][..],
        &[
            "cycle-end",
            "1",
            "--attempt",
            "1",
            "--status",
            "a",
            "--status",
            "b",
        ][..],
    ] {
        assert!(usage(args).starts_with("usage cycle-end: "), "{args:?}");
    }
}

#[test]
fn cycle_end_states_its_preconditions() {
    let home = home("end-pre");
    assert_eq!(
        refused(
            &home,
            &["cycle-end", "43", "--attempt", "1", "--status", "closed"]
        ),
        (1, "nocycle".into())
    );
    let (_l, a) = start(&home, 43);
    let (n, wrong) = attempt_args(43, a + 1);
    for mode in [
        &["--status", "closed"][..],
        &["--refused"][..],
        &["--stopped"][..],
    ] {
        let mut args = vec!["cycle-end", &n, "--attempt", &wrong];
        args.extend(mode);
        assert_eq!(refused(&home, &args), (1, "attempt".into()), "{mode:?}");
    }
}

#[test]
fn a_second_end_of_one_attempt_is_already_and_writes_nothing() {
    let home = home("end-twice");
    let (_l, a) = start(&home, 44);
    let first = end(&home, 44, a, "closed");
    let (n, at) = attempt_args(44, a);
    let again = ok_at(
        &home,
        NOW + 50,
        &["cycle-end", &n, "--attempt", &at, "--status", "failed"],
    );
    assert_eq!(again["already"], true);
    assert_eq!(again["cycle"], first["cycle"]);
    assert_eq!(count(&home, "cycle_history"), 1);
    assert_eq!(count(&home, "heartbeats"), 1);
    for mode in ["--refused", "--stopped"] {
        let v = ok(&home, &["cycle-end", &n, "--attempt", &at, mode]);
        assert_eq!(v["already"], true);
        assert_eq!(v["cycle"]["end_reason"], "closed");
    }
}

#[test]
fn an_unknown_status_word_ends_as_unknown() {
    let home = home("end-unknown");
    let (_l, a) = start(&home, 45);
    let v = end(&home, 45, a, "pr-closed");
    assert_eq!(v["cycle"]["end_reason"], "unknown");
    assert_eq!(
        one::<String>(&home, "SELECT status FROM cycle_history"),
        "pr-closed"
    );
    assert_eq!(
        one::<String>(&home, "SELECT status FROM endings"),
        "pr-closed"
    );
    assert_eq!(
        one::<String>(&home, "SELECT status FROM heartbeats"),
        "pr-closed"
    );
}

#[test]
fn every_word_libexec_cycle_assigns_is_in_the_end_reason_set() {
    // The words libexec/cycle sets as status, and the .done words that become
    // one (closed, merged).
    for word in [
        "closed",
        "merged",
        "handoff",
        "killed",
        "stalled",
        "timeout",
        "interrupted",
        "unknown",
    ] {
        assert!(END_REASONS.contains(&word), "{word}");
    }
    let home = home("end-words");
    for (i, word) in END_REASONS.iter().enumerate() {
        let issue = 100 + i as i64;
        let (_l, a) = start(&home, issue);
        assert_eq!(end(&home, issue, a, word)["cycle"]["end_reason"], *word);
    }
}

#[test]
fn cleaned_values_never_fail_an_end() {
    let home = home("end-cleaned");
    let (_l, a) = start(&home, 46);
    let (n, at) = attempt_args(46, a);
    let long_status = format!("{}!", "s".repeat(45));
    let cap = format!("CAP {}", "C".repeat(90));
    let v = ok(
        &home,
        &[
            "cycle-end",
            &n,
            "--attempt",
            &at,
            "--status",
            &long_status,
            "--step",
            &"t".repeat(41),
            "--cap",
            &cap,
            "--branch",
            "bad branch name",
            "--park-reason",
            "Parked Now",
            "--failure-kind",
            "Provider",
            "--failure-cause",
            "a cause with spaces",
        ],
    );
    assert_eq!(
        v["cleaned"],
        json!([
            "--status",
            "--step",
            "--branch",
            "--cap",
            "--park-reason",
            "--failure-kind",
            "--failure-cause"
        ])
    );
    let row: (
        String,
        String,
        String,
        Option<String>,
        String,
        String,
        String,
    ) = conn(&home)
        .query_row(
            "SELECT status, step, cap, (SELECT branch FROM cycles WHERE issue = 46), park_reason,
               failure_kind, failure_cause FROM cycle_history WHERE issue = 46",
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
                ))
            },
        )
        .unwrap();
    assert_eq!(row.0, "s".repeat(40));
    assert_eq!(row.1, "t".repeat(40));
    assert_eq!(row.2.len(), 80);
    assert!(row.2.starts_with("CAPCCC"));
    assert_eq!(row.3, None);
    assert_eq!(row.4, "arkedow");
    assert_eq!(row.5, "rovider");
    assert_eq!(row.6, "acausewithspaces");
    // A status that cleans to nothing is unknown.
    let (_l2, b) = start(&home, 47);
    let (n, at) = attempt_args(47, b);
    let v = ok(
        &home,
        &["cycle-end", &n, "--attempt", &at, "--status", "!!!"],
    );
    assert_eq!(v["cycle"]["end_reason"], "unknown");
    assert_eq!(v["cleaned"], json!(["--status"]));
}

#[test]
fn a_container_branch_of_a_plain_shape_is_kept() {
    let home = home("end-branch");
    let (_l, a) = start(&home, 48);
    let (n, at) = attempt_args(48, a);
    let v = ok(
        &home,
        &[
            "cycle-end",
            &n,
            "--attempt",
            &at,
            "--status",
            "closed",
            "--branch",
            "fix/thing.2",
        ],
    );
    assert_eq!(v["cycle"]["branch"], "fix/thing.2");
    assert!(crate::cycle::branch(48, "feature/issue-48"));
    assert!(!crate::cycle::branch(48, "-x"));
    assert!(!crate::cycle::branch(48, &format!("a{}", "b".repeat(80))));
}

#[test]
fn cycle_end_stores_a_result_of_an_unknown_schema_as_a_claim() {
    let home = home("end-result");
    let (_l, a) = start(&home, 50);
    let body = br#"{"schemaVersion":99,"status":"merged","extra":[1,2]}"#;
    let r = write_file(&home, "result.json", body);
    let (n, at) = attempt_args(50, a);
    let v = ok(
        &home,
        &[
            "cycle-end",
            &n,
            "--attempt",
            &at,
            "--status",
            "closed",
            "--result",
            r.to_str().unwrap(),
        ],
    );
    assert_eq!(v["result"], json!({"schema_version": 99}));
    assert_eq!(
        one::<String>(&home, "SELECT payload FROM cycle_results WHERE issue = 50"),
        String::from_utf8_lossy(body)
    );
    assert_eq!(
        one::<i64>(&home, "SELECT received_at FROM cycle_results"),
        NOW
    );
}

#[test]
fn cycle_end_does_not_store_a_result_that_is_not_json() {
    let home = home("end-result-bad");
    let dir = scratch("results");
    let cases: [(&[u8], &str); 4] = [
        (b"", "none"),
        (b"not json", "unreadable"),
        (b"[1]", "unreadable"),
        (br#"{"schemaVersion":"1"}"#, "unreadable"),
    ];
    for (i, (body, word)) in cases.iter().enumerate() {
        let issue = 51 + i as i64;
        let (_l, a) = start(&home, issue);
        let r = write_file(&dir, &format!("r{i}.json"), body);
        let (n, at) = attempt_args(issue, a);
        let v = ok(
            &home,
            &[
                "cycle-end",
                &n,
                "--attempt",
                &at,
                "--status",
                "closed",
                "--result",
                r.to_str().unwrap(),
            ],
        );
        assert_eq!(v["result"], *word, "{word}");
    }
    // Over 16 MiB, and a file that is not there.
    let big = dir.join("big.json");
    File::create(&big).unwrap().set_len((16 << 20) + 1).unwrap();
    let (_l, a) = start(&home, 56);
    let (n, at) = attempt_args(56, a);
    let v = ok(
        &home,
        &[
            "cycle-end",
            &n,
            "--attempt",
            &at,
            "--status",
            "closed",
            "--result",
            big.to_str().unwrap(),
            "--progress",
            dir.join("none.json").to_str().unwrap(),
        ],
    );
    assert_eq!(v["result"], "too-large");
    assert_eq!(v["progress"], "refused");
    assert_eq!(count(&home, "cycle_results"), 0);
    assert_eq!(v["cycle"]["end_reason"], "closed");
}

fn set_mtime(path: &Path, unix: i64) {
    let f = File::options().write(true).open(path).unwrap();
    f.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(unix as u64))
        .unwrap();
}

#[test]
fn cycle_end_stores_progress_written_after_the_start() {
    let home = home("end-progress");
    let (_l, a) = start(&home, 57);
    let p = write_file(
        &home,
        "progress.json",
        br#"{"roles":[{"role":"driver","turns":3}],"reason":"slow","reviewHeld":true}"#,
    );
    set_mtime(&p, NOW + 5);
    let (n, at) = attempt_args(57, a);
    let v = ok(
        &home,
        &[
            "cycle-end",
            &n,
            "--attempt",
            &at,
            "--status",
            "closed",
            "--progress",
            p.to_str().unwrap(),
        ],
    );
    assert_eq!(v["progress"], json!({"written_at": NOW + 5}));
    let row: (i64, String, i64, Option<String>, String, i64) = conn(&home)
        .query_row(
            "SELECT received_at, reason, review_held, steps_json, roles_json, written_at FROM cycle_progress",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .unwrap();
    assert_eq!(
        row,
        (
            NOW,
            "slow".into(),
            1,
            None,
            r#"[{"role":"driver","turns":3}]"#.into(),
            NOW + 5
        )
    );
}

#[test]
fn cycle_end_does_not_store_progress_older_than_the_start() {
    let home = home("end-stale");
    let (_l, a) = start(&home, 58);
    let p = write_file(&home, "progress.json", br#"{"reason":"old"}"#);
    set_mtime(&p, NOW - 1);
    let (n, at) = attempt_args(58, a);
    let v = ok(
        &home,
        &[
            "cycle-end",
            &n,
            "--attempt",
            &at,
            "--status",
            "closed",
            "--progress",
            p.to_str().unwrap(),
        ],
    );
    assert_eq!(v["progress"], "stale");
    assert_eq!(count(&home, "cycle_progress"), 0);
    // Too large and unreadable.
    let (_l2, b) = start(&home, 59);
    let big = home.join("big-progress.json");
    File::create(&big).unwrap().set_len((1 << 20) + 1).unwrap();
    let (n, at) = attempt_args(59, b);
    let v = ok(
        &home,
        &[
            "cycle-end",
            &n,
            "--attempt",
            &at,
            "--status",
            "closed",
            "--progress",
            big.to_str().unwrap(),
        ],
    );
    assert_eq!(v["progress"], "too-large");
    let (_l3, c) = start(&home, 60);
    let bad = write_file(&home, "bad-progress.json", b"{");
    let (n, at) = attempt_args(60, c);
    let v = ok(
        &home,
        &[
            "cycle-end",
            &n,
            "--attempt",
            &at,
            "--status",
            "closed",
            "--progress",
            bad.to_str().unwrap(),
        ],
    );
    assert_eq!(v["progress"], "unreadable");
}

#[test]
fn cycle_end_does_not_change_image_or_commit() {
    let home = home("end-image");
    approve(&home, 61, "alice", "label");
    let _l = hold(&home, 61);
    ok(&home, &["cycle-start", "61", "--rukas", "abcdef0"]);
    image(&home, 61, 1, &[]).unwrap();
    let v = end(&home, 61, 1, "closed");
    assert_eq!(v["cycle"]["image"], IMAGE);
    assert_eq!(v["cycle"]["rukas_commit"], "abcdef0");
}

#[test]
fn never_ran_removes_the_used_approvals_and_keeps_an_unused_one() {
    let home = home("never-ran");
    let (l, a) = start(&home, 62);
    end(&home, 62, a, "closed");
    drop(l);
    let (_l, b) = start(&home, 62);
    let unused_at = approve(&home, 62, "bob", "label");
    let (n, at) = attempt_args(62, b);
    ok(
        &home,
        &[
            "cycle-end",
            &n,
            "--attempt",
            &at,
            "--status",
            "failed",
            "--never-ran",
        ],
    );
    assert_eq!(
        count(&home, "approvals WHERE issue = 62 AND used_by IS NOT NULL"),
        0
    );
    assert_eq!(
        one::<i64>(&home, "SELECT at FROM approvals WHERE issue = 62"),
        unused_at
    );
}

#[test]
fn endings_keep_the_ten_newest() {
    let home = home("endings");
    for i in 0..12 {
        let issue = 200 + i;
        let (_l, a) = start(&home, issue);
        let (n, at) = attempt_args(issue, a);
        ok_at(
            &home,
            NOW + i,
            &["cycle-end", &n, "--attempt", &at, "--status", "closed"],
        );
    }
    assert_eq!(count(&home, "endings"), 10);
    assert_eq!(one::<i64>(&home, "SELECT min(issue) FROM endings"), 202);
}

#[test]
fn history_keeps_fifty_per_issue() {
    let home = home("history-50");
    conn(&home)
        .execute_batch(
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 55)
             INSERT INTO cycle_history (issue, attempt, at, status) SELECT 63, i, i, 'closed' FROM n;
             INSERT INTO cycle_history (issue, attempt, at, status) VALUES (64, 1, 1, 'closed');",
        )
        .unwrap();
    let (_l, a) = start(&home, 63);
    assert_eq!(a, 56);
    end(&home, 63, a, "closed");
    assert_eq!(count(&home, "cycle_history WHERE issue = 63"), 50);
    assert_eq!(
        one::<i64>(
            &home,
            "SELECT min(attempt) FROM cycle_history WHERE issue = 63"
        ),
        7
    );
    assert_eq!(count(&home, "cycle_history WHERE issue = 64"), 1);
}

#[test]
fn a_refused_start_ends_the_row_with_no_history() {
    let home = home("end-refused");
    let (lock_file, a) = start(&home, 65);
    let (n, at) = attempt_args(65, a);
    let v = ok(&home, &["cycle-end", &n, "--attempt", &at, "--refused"]);
    assert_eq!(v["already"], false);
    assert_eq!(v["cycle"]["ended_at"], NOW);
    assert_eq!(v["cycle"]["end_reason"], Value::Null);
    assert_eq!(count(&home, "cycle_history"), 0);
    assert_eq!(count(&home, "endings"), 0);
    assert_eq!(count(&home, "heartbeats"), 0);
    assert_eq!(count(&home, "approvals WHERE used_by IS NOT NULL"), 1);
    drop(lock_file);
    // Refused needs the caller to hold the lock.
    let (l2, b) = start(&home, 66);
    drop(l2);
    let (n, at) = attempt_args(66, b);
    assert_eq!(
        refused(&home, &["cycle-end", &n, "--attempt", &at, "--refused"]),
        (1, "unlocked".into())
    );
}

#[test]
fn a_stopped_end_needs_a_free_lock() {
    let home = home("end-stopped");
    let (l, a) = start(&home, 67);
    let (n, at) = attempt_args(67, a);
    assert_eq!(
        refused(&home, &["cycle-end", &n, "--attempt", &at, "--stopped"]),
        (1, "running".into())
    );
    drop(l);
    let v = ok(&home, &["cycle-end", &n, "--attempt", &at, "--stopped"]);
    assert_eq!(v["cycle"]["end_reason"], "stopped");
    assert_eq!(count(&home, "cycle_history"), 0);
    assert_eq!(count(&home, "endings"), 0);
    // With the lock file already removed the issue cannot be told.
    let (l2, b) = start(&home, 68);
    drop(l2);
    fs::remove_file(lock::path(&home, 68)).unwrap();
    let (n, at) = attempt_args(68, b);
    assert_eq!(
        refused(&home, &["cycle-end", &n, "--attempt", &at, "--stopped"]),
        (1, "probe".into())
    );
}

// --- cycle-show -----------------------------------------------------------------

#[test]
fn cycle_show_reports_held_while_the_test_holds_the_lock() {
    let home = home("show-held");
    let (l, _) = start(&home, 70);
    let v = ok(&home, &["cycle-show", "70"]);
    assert_eq!(v["issue"], 70);
    assert_eq!(v["lock"]["state"], "held");
    assert_eq!(v["lock"]["probe_rc"], 1);
    assert_eq!(v["lock"]["seen_at"], NOW);
    assert_eq!(v["cycle"]["attempt"], 1);
    assert_eq!(v["cycle"]["started_by"], "alice");
    assert_eq!(v["cycle"]["settings"], json!({}));
    assert_eq!(v["cycle"]["quiet"], Value::Null);
    drop(l);
    assert_eq!(ok(&home, &["cycle-show", "70"])["lock"]["state"], "free");
}

#[test]
fn cycle_show_writes_no_cycle_locks_row() {
    let home = home("show-writes");
    let (_l, _) = start(&home, 71);
    ok(&home, &["cycle-show", "71"]);
    ok(&home, &["cycle-show", "--running"]);
    assert_eq!(count(&home, "cycle_locks"), 0);
}

#[test]
fn cycle_show_without_a_lock_file_and_no_cycle_is_free() {
    let home = home("show-free");
    let v = ok(&home, &["cycle-show", "72"]);
    assert_eq!(
        v["lock"],
        json!({"state":"free","probe_rc":0,"file":false,"seen_at":NOW})
    );
    assert_eq!(v["cycle"], Value::Null);
}

#[test]
fn cycle_show_without_a_lock_file_for_a_running_cycle_is_probe_rc_2() {
    let home = home("show-nofile");
    let (l, _) = start(&home, 73);
    drop(l);
    fs::remove_file(lock::path(&home, 73)).unwrap();
    let v = ok(&home, &["cycle-show", "73"]);
    assert_eq!(v["lock"]["probe_rc"], 2);
    assert_eq!(
        v["lock"]["why"],
        "no lock file for a cycle on record as running"
    );
    // Once ended, no file is free again.
    end(&home, 73, 1, "closed");
    assert_eq!(ok(&home, &["cycle-show", "73"])["lock"]["probe_rc"], 0);
}

#[test]
fn cycle_show_of_a_link_or_directory_is_probe_rc_2() {
    let home = home("show-shapes");
    fs::create_dir_all(lock::path(&home, 74)).unwrap();
    assert_eq!(ok(&home, &["cycle-show", "74"])["lock"]["probe_rc"], 2);
    let target = home.join("t.lock");
    fs::write(&target, b"").unwrap();
    std::os::unix::fs::symlink(&target, lock::path(&home, 75)).unwrap();
    assert_eq!(ok(&home, &["cycle-show", "75"])["lock"]["probe_rc"], 2);
}

#[test]
fn cycle_show_running_lists_open_rows_and_held_locks() {
    let home = home("show-running");
    let (_held, _) = start(&home, 80);
    let (l81, a81) = start(&home, 81);
    end(&home, 81, a81, "closed");
    drop(l81);
    // A lock held with no row, and a free lock file with no row.
    let _stray = hold(&home, 82);
    drop(hold(&home, 83));
    let v = ok(&home, &["cycle-show", "--running"]);
    assert_eq!(v["probe_rc"], 0);
    assert!(v.get("why").is_none());
    let issues: Vec<i64> = v["cycles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["issue"].as_i64().unwrap())
        .collect();
    assert_eq!(issues, vec![80, 82]);
    assert_eq!(v["cycles"][1]["cycle"], Value::Null);
    assert_eq!(v["cycles"][0]["lock"]["state"], "held");
}

#[test]
fn cycle_show_running_is_probe_rc_2_when_any_entry_is() {
    let home = home("show-running-2");
    let (_held, _) = start(&home, 84);
    let (l, _) = start(&home, 85);
    drop(l);
    fs::remove_file(lock::path(&home, 85)).unwrap();
    let v = ok(&home, &["cycle-show", "--running"]);
    assert_eq!(v["probe_rc"], 2);
    assert!(v["why"].as_str().unwrap().contains("#85"));
    assert_eq!(v["cycles"].as_array().unwrap().len(), 2);
    // A missing state/cycles lists the open rows, each probed.
    fs::remove_dir_all(lock::dir(&home)).unwrap();
    let v = ok(&home, &["cycle-show", "--running"]);
    assert_eq!(v["probe_rc"], 2);
    assert_eq!(v["cycles"].as_array().unwrap().len(), 2);
}

#[test]
fn cycle_show_of_an_unlistable_directory_is_probe_rc_2() {
    let home = home("show-unlistable");
    fs::create_dir_all(lock::dir(&home)).unwrap();
    if modes_bind() {
        fs::set_permissions(lock::dir(&home), fs::Permissions::from_mode(0o000)).unwrap();
        let v = ok(&home, &["cycle-show", "--running"]);
        fs::set_permissions(lock::dir(&home), fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(v["probe_rc"], 2);
        assert!(v["why"].as_str().unwrap().starts_with("state/cycles"));
    }
    // state/cycles a file: not a directory.
    fs::remove_dir_all(lock::dir(&home)).unwrap();
    fs::write(lock::dir(&home), b"").unwrap();
    let v = ok(&home, &["cycle-show", "--running"]);
    assert_eq!(v["probe_rc"], 2);
}

#[test]
fn cycle_show_refuses_neither_or_both() {
    assert!(usage(&["cycle-show"]).starts_with("usage cycle-show: "));
    assert!(usage(&["cycle-show", "1", "--running"]).starts_with("usage cycle-show: "));
    assert!(usage(&["cycle-show", "--running", "--running"]).starts_with("usage cycle-show: "));
}

// --- cycle-note and cycle-stop-asked -----------------------------------------------

fn note(home: &Path, issue: i64, said_for: &str, now: i64) -> Value {
    ok_at(
        home,
        now,
        &[
            "cycle-note",
            &issue.to_string(),
            "--kind",
            "quiet",
            "--said-for",
            said_for,
        ],
    )
}

fn ask(home: &Path, issue: i64, now: i64, said_only: bool) -> Value {
    let n = issue.to_string();
    let mut args = vec!["cycle-stop-asked", &n];
    if said_only {
        args.push("--said-only");
    }
    ok_at(home, now, &args)
}

#[test]
fn cycle_note_says_once_per_episode() {
    let home = home("note-once");
    let (_l, _) = start(&home, 90);
    let v = note(&home, 90, "1791990000", NOW);
    assert_eq!(v["said"], true);
    assert_eq!(v["episode_at"], NOW);
    assert_eq!(v["times"], 0);
    assert_eq!(v["stop_asked_at"], Value::Null);
    let v = note(&home, 90, "1791990000", NOW + 60);
    assert_eq!(v["said"], false);
    assert_eq!(v["episode_at"], NOW);
    assert_eq!(count(&home, "cycle_notes"), 1);
    assert_eq!(
        ok(&home, &["cycle-show", "90"])["cycle"]["quiet"],
        json!({"said_for":"1791990000","at":NOW})
    );
}

#[test]
fn a_new_episode_counts_its_asks_from_one() {
    let home = home("note-episode");
    let (_l, _) = start(&home, 91);
    note(&home, 91, "e1", NOW);
    assert_eq!(ask(&home, 91, NOW + 1, false)["times"], 1);
    assert_eq!(ask(&home, 91, NOW + 2, false)["times"], 2);
    assert_eq!(note(&home, 91, "e1", NOW + 3)["times"], 2);
    // The cycle was heard from again: a new episode.
    let v = note(&home, 91, "e2", NOW + 10);
    assert_eq!(
        (v["said"].clone(), v["times"].clone()),
        (json!(true), json!(0))
    );
    assert_eq!(count(&home, "cycle_notes"), 1);
    assert_eq!(ask(&home, 91, NOW + 11, false)["times"], 1);
    assert_eq!(note(&home, 91, "e2", NOW + 12)["times"], 1);
}

#[test]
fn said_only_moves_the_time_and_not_the_count() {
    let home = home("note-said-only");
    let (_l, _) = start(&home, 92);
    note(&home, 92, "e1", NOW);
    ask(&home, 92, NOW + 1, false);
    let v = ask(&home, 92, NOW + 5, true);
    assert_eq!(
        v,
        json!({"api":1,"verb":"cycle-stop-asked","ok":true,"times":1,"asked_at":NOW + 5})
    );
    assert_eq!(
        one::<i64>(&home, "SELECT stop_asked_at FROM cycles WHERE issue = 92"),
        NOW + 5
    );
}

#[test]
fn cycle_note_and_stop_asked_state_their_preconditions() {
    let home = home("note-pre");
    assert_eq!(
        refused(
            &home,
            &["cycle-note", "93", "--kind", "quiet", "--said-for", "x"]
        ),
        (1, "nocycle".into())
    );
    assert_eq!(
        refused(&home, &["cycle-stop-asked", "93"]),
        (1, "nocycle".into())
    );
    let (_l, a) = start(&home, 93);
    assert_eq!(
        refused(&home, &["cycle-stop-asked", "93"]),
        (1, "nonote".into())
    );
    end(&home, 93, a, "closed");
    assert_eq!(
        refused(
            &home,
            &["cycle-note", "93", "--kind", "quiet", "--said-for", "x"]
        ),
        (1, "ended".into())
    );
    assert_eq!(
        refused(&home, &["cycle-stop-asked", "93"]),
        (1, "ended".into())
    );
    for args in [
        &["cycle-note", "93", "--said-for", "x"][..],
        &["cycle-note", "93", "--kind", "quiet"][..],
        &["cycle-note", "93", "--kind", "quiet", "--said-for", "a b"][..],
        &[
            "cycle-note",
            "93",
            "--kind",
            "quiet",
            "--said-for",
            &"x".repeat(65),
        ][..],
        &["cycle-stop-asked", "93", "--said-only", "--said-only"][..],
        &["cycle-stop-asked"][..],
    ] {
        assert!(usage(args).starts_with("usage cycle-"), "{args:?}");
    }
}
