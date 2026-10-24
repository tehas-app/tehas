//! backup, prune and sweep-locks.

use crate::upkeep::{date, KEEP};
use crate::verbs_tests::*;
use rusqlite::Connection;
use serde_json::Value;
use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

const DAY: i64 = 86_400;

fn mode(p: &Path) -> u32 {
    fs::metadata(p).unwrap().permissions().mode() & 0o777
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

fn aged(p: &Path, at: i64) {
    let f = File::options().write(true).open(p).unwrap();
    f.set_modified(UNIX_EPOCH + Duration::from_secs(at as u64))
        .unwrap();
}

#[test]
fn the_date_is_the_utc_day() {
    assert_eq!(date(0), "19700101");
    assert_eq!(date(NOW), "20261014");
    assert_eq!(date(1_709_164_800), "20240229");
    assert_eq!(date(1_709_164_800 - 1), "20240228");
}

#[test]
fn backup_makes_one_copy_a_day_that_reads_as_the_store() {
    let home = home("backup-day");
    let (_lock, _) = start(&home, 7);
    let v = ok(&home, &["backup"]);
    assert_eq!(v["made"], true);
    let dir = home.join("state/backup");
    let copy = dir.join("tehas-20261014.db");
    assert_eq!(v["newest"]["file"], copy.to_str().unwrap());
    assert_eq!(mode(&dir), 0o700);
    assert_eq!(mode(&copy), 0o600);
    let c = Connection::open(&copy).unwrap();
    let n: i64 = c
        .query_row("SELECT count(*) FROM cycles WHERE issue = 7", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(n, 1);
    // A copy the store opens as it is, once restored over it.
    let journal: String = c
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(journal, "wal");
    drop(c);
    // The same day again: the copy is kept as it is.
    let v = ok_at(&home, NOW + 60, &["backup"]);
    assert_eq!(v["made"], false);
    assert_eq!(names(&dir), vec!["tehas-20261014.db"]);
}

#[test]
fn backup_keeps_the_newest_seven_and_no_other_name_is_touched() {
    let home = home("backup-keep");
    let dir = home.join("state/backup");
    fs::create_dir_all(&dir).unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
    for d in 1..=8 {
        fs::write(dir.join(format!("tehas-202610{d:02}.db")), b"old").unwrap();
    }
    fs::write(dir.join("tehas-3-1792000000.db"), b"migrate's").unwrap();
    let v = ok(&home, &["backup"]);
    assert_eq!(v["removed"].as_array().unwrap().len(), 2);
    let left = names(&dir);
    assert_eq!(left.len(), KEEP + 1);
    assert!(left.contains(&"tehas-20261014.db".to_string()));
    assert!(left.contains(&"tehas-3-1792000000.db".to_string()));
    assert!(!left.contains(&"tehas-20261001.db".to_string()));
    assert!(!left.contains(&"tehas-20261002.db".to_string()));
}

#[test]
fn backup_show_reads_the_newest_and_its_age() {
    let home = home("backup-show");
    assert_eq!(ok(&home, &["backup", "--show"])["newest"], Value::Null);
    assert!(!home.join("state/backup").exists());
    ok(&home, &["backup"]);
    let copy = home.join("state/backup/tehas-20261014.db");
    aged(&copy, NOW - 3 * DAY);
    let v = ok(&home, &["backup", "--show"]);
    assert_eq!(v["newest"]["at"], NOW - 3 * DAY);
    assert_eq!(v["newest"]["age_seconds"], 3 * DAY);
}

#[test]
fn prune_removes_old_ends_settled_actions_and_old_heartbeats_of_ended_cycles() {
    let home = home("prune");
    let old = NOW - 100 * DAY;
    let recent = NOW - DAY;
    let (lock, attempt) = start_by(&home, 8, "alice", "label", old);
    ok_at(
        &home,
        old + 10,
        &["cycle-heartbeat", "8", "--running", "--status", "running"],
    );
    drop(lock);
    ok_at(
        &home,
        old + 20,
        &[
            "cycle-end",
            "8",
            "--attempt",
            &attempt.to_string(),
            "--status",
            "merged",
        ],
    );
    // A running cycle keeps every heartbeat, however old.
    let (_lock, _) = start_by(&home, 9, "alice", "label", old);
    ok_at(
        &home,
        old + 10,
        &["cycle-heartbeat", "9", "--running", "--status", "running"],
    );
    conn(&home)
        .execute_batch(&format!(
            "INSERT INTO endings (issue, started_at, ended_at, status) VALUES (20, 1, {recent}, 'merged');
             INSERT INTO polls (at, lock_taken_at) VALUES (1, 1);
             INSERT INTO actions (id, poll_at, kind, issue, args_json, before_snapshot, state, intended_at, settled_at)
               VALUES (1, 1, 'label', 8, '{{}}', 1, 'done', {old}, {old}),
                      (2, 1, 'label', 8, '{{}}', 1, 'intended', {old}, NULL),
                      (3, 1, 'label', 8, '{{}}', 1, 'done', {recent}, {recent});
             INSERT INTO journal (action, at, step)
               VALUES (1, {old}, 'done'), (2, {old}, 'intended'), (3, {recent}, 'done');"
        ))
        .unwrap();
    let v = ok(&home, &["prune", "--days", "90"]);
    assert_eq!(v["endings"], 1);
    assert_eq!(v["actions"], 1);
    assert_eq!(v["journal"], 1);
    assert_eq!(v["heartbeats"], 2);
    assert_eq!(count(&home, "endings"), 1);
    assert_eq!(count(&home, "actions"), 2);
    assert_eq!(count(&home, "journal"), 2);
    assert_eq!(count(&home, "heartbeats WHERE issue = 8"), 0);
    assert_eq!(count(&home, "heartbeats WHERE issue = 9"), 1);
    // The cycles stay.
    assert_eq!(count(&home, "cycles"), 2);
}

#[test]
fn prune_and_sweep_locks_refuse_bad_numbers() {
    assert!(usage(&["prune"]).contains("--days"));
    assert!(usage(&["prune", "--days", "0"]).contains("--days"));
    assert!(usage(&["sweep-locks", "--max", "0"]).contains("--max"));
}

/// A cycle of `issue` that ended `ago` seconds ago, its lock file left.
fn ended(home: &Path, issue: i64, ago: i64) {
    let (lock, attempt) = start_by(home, issue, "alice", "label", NOW - ago - 60);
    drop(lock);
    ok_at(
        home,
        NOW - ago,
        &[
            "cycle-end",
            &issue.to_string(),
            "--attempt",
            &attempt.to_string(),
            "--status",
            "merged",
        ],
    );
    // The file as old as its cycle: one touched in the last minute may have a
    // waiter on it.
    aged(&home.join(format!("state/cycles/{issue}.lock")), NOW - ago);
}

#[test]
fn sweep_locks_removes_the_free_locks_of_issues_unused_for_its_days() {
    let home = home("sweep-locks");
    let locks = home.join("state/cycles");
    ended(&home, 1, 40 * DAY);
    ended(&home, 2, 10 * DAY);
    ended(&home, 3, 40 * DAY);
    let _held = hold(&home, 3);
    let (_open, _) = start_by(&home, 4, "alice", "label", NOW - 40 * DAY);
    // A lock with no cycle on record, old and young.
    File::create(locks.join("5.lock")).unwrap();
    aged(&locks.join("5.lock"), NOW - 40 * DAY);
    File::create(locks.join("6.lock")).unwrap();
    aged(&locks.join("6.lock"), NOW - DAY);
    let v = ok(&home, &["sweep-locks", "--dry-run"]);
    assert_eq!(v["removed"], serde_json::json!([1, 5]));
    assert!(locks.join("1.lock").exists());
    assert_eq!(count(&home, "sweep_log"), 0);
    let v = ok(&home, &["sweep-locks"]);
    assert_eq!(v["removed"], serde_json::json!([1, 5]));
    assert_eq!(names(&locks), vec!["2.lock", "3.lock", "4.lock", "6.lock"]);
    assert_eq!(
        count(
            &home,
            "sweep_log WHERE action = 'removed' AND what = 'lock'"
        ),
        2
    );
}

#[test]
fn sweep_locks_takes_at_most_its_max() {
    let home = home("sweep-max");
    for n in 1..=4 {
        ended(&home, n, 40 * DAY);
    }
    let v = ok(&home, &["sweep-locks", "--max", "3"]);
    assert_eq!(v["removed"].as_array().unwrap().len(), 3);
    let v = ok(&home, &["sweep-locks"]);
    assert_eq!(v["removed"].as_array().unwrap().len(), 1);
}

#[test]
fn sweep_locks_leaves_a_lock_touched_in_the_last_minute() {
    let home = home("sweep-touched");
    ended(&home, 1, 40 * DAY);
    // tehasbot cycle opens its lock with `exec 9>` before it waits for it.
    File::create(home.join("state/cycles/1.lock")).unwrap();
    let v = ok(&home, &["sweep-locks"]);
    assert_eq!(v["removed"], serde_json::json!([]));
    assert_eq!(v["kept"], serde_json::json!([1]));
    assert!(home.join("state/cycles/1.lock").exists());
}

#[test]
fn a_copy_cut_short_is_no_copy_of_the_day() {
    let home = home("backup-part");
    let dir = home.join("state/backup");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("tehas-20261014.db.part"), b"half").unwrap();
    let v = ok(&home, &["backup"]);
    assert_eq!(v["made"], true);
    assert_eq!(names(&dir), vec!["tehas-20261014.db"]);
}

#[test]
fn upkeep_removes_old_logs_backs_up_and_prunes() {
    let home = home("upkeep");
    let logs = home.join("logs");
    fs::create_dir_all(&logs).unwrap();
    for name in [
        "work-3-261001-000000.jsonl",
        "work-3-261001-000000.jsonl.err",
        "dispatch-work-3.log",
        "index-261001-000000.log",
        "improve-analyst-261001-000000.log",
        "plan-4-261001-000000.jsonl",
        "dispatch-plan-4.log",
        "dispatch.log",
        "clean.log",
        "work-5-261013-000000.jsonl",
    ] {
        fs::write(logs.join(name), b"x").unwrap();
        aged(&logs.join(name), NOW - 8 * DAY);
    }
    aged(&logs.join("work-5-261013-000000.jsonl"), NOW - DAY);
    let v = ok(&home, &["upkeep", "--dry-run"]);
    assert_eq!(v["logs"].as_array().unwrap().len(), 7);
    assert_eq!(v.get("backup"), None);
    assert_eq!(names(&logs).len(), 10);
    assert!(!home.join("state/backup").exists());
    let v = ok(&home, &["upkeep"]);
    assert_eq!(v["logs"].as_array().unwrap().len(), 7);
    assert_eq!(v["backup"]["made"], true);
    assert_eq!(v["prune"]["before"], NOW - 90 * DAY);
    assert_eq!(
        names(&logs),
        vec!["clean.log", "dispatch.log", "work-5-261013-000000.jsonl"]
    );
}
