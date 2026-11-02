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
             INSERT INTO polls (at, lock_taken_at) VALUES (1, 1), (2, 2), ({recent}, {recent});
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
    // Poll 2 goes; poll 1 stays while its action 2 is unsettled, and the
    // recent one is not old enough.
    assert_eq!(v["polls"], 1);
    assert_eq!(v["polls_kept"], 1);
    assert_eq!(count(&home, "polls"), 2);
    assert_eq!(count(&home, "endings"), 1);
    assert_eq!(count(&home, "actions"), 2);
    assert_eq!(count(&home, "journal"), 2);
    assert_eq!(count(&home, "heartbeats WHERE issue = 8"), 0);
    assert_eq!(count(&home, "heartbeats WHERE issue = 9"), 1);
    // The cycles stay.
    assert_eq!(count(&home, "cycles"), 2);
}

/// What a cycle of `issue` ran: `n` ends from its start at `start` to
/// `cycle_end`, its cycle row left ending at `ended` (null for one still
/// running).
struct Cycle {
    n: i64,
    start: i64,
    cycle_end: i64,
    ended: Option<i64>,
}

/// What the plan of `issue` ran: `m` turns of one session from `plan_start` to
/// `plan_end`.
struct Plan {
    m: i64,
    plan_start: i64,
    plan_end: i64,
}

/// The issue of a factory that ran the cycle and the plan it is given. Every row
/// is written by the verb that writes it in a factory, so each carries the `at`,
/// the `seq` and the place among the 50 newest of the issue that verb gives it:
/// the cycle rows are dated between the two cycle instants and the plan rows
/// between the two planning ones, so the newest row of either table is the one a
/// test names by the attempt or the turn it is. A `cycle` of no ends is the issue
/// whose cycle ended without a history row (`cycle-end --stopped` or `--refused`),
/// and no `cycle` at all, given a start in the past that no verb wrote, is the
/// issue the store has no cycle row for.
fn history(home: &Path, issue: i64, cycle: Option<Cycle>, plan: Plan) {
    let (n, start, cycle_end, ended) = match &cycle {
        None => (0, plan.plan_start, plan.plan_start, None),
        Some(c) => (c.n, c.start, c.cycle_end, c.ended),
    };
    let Plan {
        m,
        plan_start,
        plan_end,
    } = plan;
    let n_s = issue.to_string();
    conn(home)
        .execute_batch(&format!(
            "INSERT OR IGNORE INTO issues (issue, kind, kind_at, checked_at) VALUES ({issue}, 'closed', {start}, 0);"
        ))
        .unwrap();
    if n > 0 {
        let lock = hold(home, issue);
        // The approval the first start is made from; a later attempt is started
        // by hand, which needs none.
        let at = (start - 6).to_string();
        ok_at(
            home,
            start - 6,
            &[
                "approval-record",
                &n_s,
                "--at",
                &at,
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
        ok_at(home, start, &["cycle-start", &n_s]);
        let span = cycle_end - start;
        for attempt in 1..=n {
            let a = attempt.to_string();
            ok_at(
                home,
                start + attempt * span / n,
                &["cycle-end", &n_s, "--attempt", &a, "--status", "closed"],
            );
            if attempt < n {
                let next = attempt + 1;
                ok_at(
                    home,
                    start + next * span / n - 1,
                    &["cycle-start", &n_s, "--by-hand"],
                );
            }
        }
        drop(lock);
    }
    let ended = match ended {
        None => "NULL".to_string(),
        Some(e) => e.to_string(),
    };
    conn(home)
        .execute_batch(&format!(
            "UPDATE cycles SET ended_at = {ended} WHERE issue = {issue};"
        ))
        .unwrap();
    if m > 0 {
        let b = write_file(home, "brief.md", b"the brief")
            .to_str()
            .unwrap()
            .to_string();
        ok_at(
            home,
            plan_start,
            &[
                "plan-begin",
                &n_s,
                "--by",
                "alice",
                "--label",
                "tehas:plan",
                "--brief-file",
                &b,
            ],
        );
        let span = plan_end - plan_start;
        for turn in 1..=m {
            let t = turn.to_string();
            let at = plan_start + turn * span / m;
            if turn > 1 {
                ok_at(
                    home,
                    at,
                    &["plan-answer", &n_s, "--turn", &t, "--by", "alice"],
                );
            }
            ok_at(
                home,
                at,
                &[
                    "plan-end",
                    &n_s,
                    "--turn",
                    &t,
                    "--outcome",
                    "finished",
                    "--rc",
                    "0",
                    "--session",
                    "s1",
                ],
            );
        }
    }
}

/// A row of one of the two histories, written with the `at` the test asks for:
/// the row of an end the store recorded before this release could date it, so the
/// pruner is asked about a row older than the issue's own end. `seq` is the
/// store's to give, as the schema's trigger shows.
fn row(home: &Path, cycle: bool, issue: i64, n: i64, at: i64) {
    let sql = if cycle {
        format!("INSERT INTO cycle_history (issue, attempt, at, status) VALUES ({issue}, {n}, {at}, 'closed');")
    } else {
        format!("INSERT INTO plan_history (issue, turn, at, outcome) VALUES ({issue}, {n}, {at}, 'finished');")
    };
    conn(home).execute_batch(&sql).unwrap();
}

#[test]
fn prune_removes_the_history_of_issues_that_ended_long_ago_and_keeps_one_row() {
    let home = home("prune-history");
    let old = NOW - 100 * DAY;
    let recent = NOW - DAY;
    // Ended long ago: the old rows go and the last row of each table stays.
    history(
        &home,
        30,
        Some(Cycle {
            n: 30,
            start: old - 40,
            cycle_end: old,
            ended: Some(old),
        }),
        Plan {
            m: 12,
            plan_start: old - 40,
            plan_end: old,
        },
    );
    // Its cycle ended long ago and wrote no history of its own (`cycle-end
    // --stopped` or `--refused`), an end a report reads from the issue's cycle row
    // and its `endings` row: its plan rows are all the issue has, so they stay.
    history(
        &home,
        31,
        Some(Cycle {
            n: 0,
            start: old - 40,
            cycle_end: old,
            ended: Some(old),
        }),
        Plan {
            m: 4,
            plan_start: old - 40,
            plan_end: old,
        },
    );
    // Ended long ago, but its planning turned after the window, and one row of
    // each table is older than its end, as a row of an old store can be: the last
    // row of each table is kept, and the rows older than the window go.
    history(
        &home,
        32,
        Some(Cycle {
            n: 4,
            start: old - 40,
            cycle_end: old - 30,
            ended: Some(old),
        }),
        Plan {
            m: 3,
            plan_start: old - 40,
            plan_end: recent,
        },
    );
    row(&home, true, 32, 5, old - 35);
    row(&home, false, 32, 6, old - 39);
    // Ended since the window: every row stays whatever its age.
    history(
        &home,
        33,
        Some(Cycle {
            n: 6,
            start: old - 40,
            cycle_end: old - 20,
            ended: Some(recent),
        }),
        Plan {
            m: 4,
            plan_start: old - 40,
            plan_end: old - 10,
        },
    );
    // Still running: every row stays.
    history(
        &home,
        34,
        Some(Cycle {
            n: 6,
            start: old - 40,
            cycle_end: old - 20,
            ended: None,
        }),
        Plan {
            m: 0,
            plan_start: old,
            plan_end: old,
        },
    );
    // A plan of an issue with no cycle on record: nothing dates it, so nothing of
    // it goes.
    history(
        &home,
        35,
        None,
        Plan {
            m: 5,
            plan_start: old - 40,
            plan_end: old - 10,
        },
    );

    let v = ok(&home, &["prune", "--days", "90"]);
    // The 29 old cycle rows of issue 30, the four of issue 32, and the three old
    // plan rows of issue 31, whose cycle wrote no history of its own.
    assert_eq!(v["cycle_history"], 33);
    assert_eq!(v["plan_history"], 14);
    assert_eq!(count(&home, "cycle_history WHERE issue = 30"), 1);
    assert_eq!(count(&home, "plan_history WHERE issue = 30"), 1);
    assert_eq!(
        one::<i64>(&home, "SELECT attempt FROM cycle_history WHERE issue = 30"),
        30
    );
    assert_eq!(
        one::<i64>(&home, "SELECT turn FROM plan_history WHERE issue = 30"),
        12
    );
    // The issue whose cycle wrote no history keeps all four of its plan rows.
    assert_eq!(count(&home, "cycle_history WHERE issue = 31"), 0);
    assert_eq!(count(&home, "plan_history WHERE issue = 31"), 4);
    // Its planning row of the window is the one kept of that table, and its four
    // old cycle rows went. Of those the oldest is kept too, being the end the
    // issue's `endings` row names: prune takes that row from its table in the same
    // call, so the two keep saying the same thing about the issue's last end.
    assert_eq!(count(&home, "cycle_history WHERE issue = 32"), 1);
    assert_eq!(count(&home, "plan_history WHERE issue = 32"), 1);
    assert_eq!(
        one::<i64>(&home, "SELECT at FROM cycle_history WHERE issue = 32"),
        old - 30
    );
    assert_eq!(
        one::<i64>(&home, "SELECT turn FROM plan_history WHERE issue = 32"),
        3
    );
    // Not pruned: the issue that ended recently, the running cycle, and the issue
    // with no cycle on record.
    assert_eq!(count(&home, "cycle_history WHERE issue = 33"), 6);
    assert_eq!(count(&home, "plan_history WHERE issue = 33"), 4);
    assert_eq!(count(&home, "cycle_history WHERE issue = 34"), 6);
    assert_eq!(count(&home, "plan_history WHERE issue = 35"), 5);

    // The kept rows are the ones `history-show` answers with, and a repeat of the
    // verb takes nothing more.
    assert_eq!(
        ok(&home, &["history-show", "30"])["rows"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let again = ok(&home, &["prune", "--days", "90"]);
    assert_eq!(again["cycle_history"], 0);
    assert_eq!(again["plan_history"], 0);
}

#[test]
fn upkeep_prunes_the_history_and_history_still_answers() {
    let home = home("upkeep-history");
    let old = NOW - 120 * DAY;
    history(
        &home,
        40,
        Some(Cycle {
            n: 20,
            start: old - 40,
            cycle_end: old,
            ended: Some(old),
        }),
        Plan {
            m: 8,
            plan_start: old - 40,
            plan_end: old,
        },
    );
    // A recent issue: its cycle ended within the window, so all its rows stay.
    history(
        &home,
        41,
        Some(Cycle {
            n: 3,
            start: NOW - 2 * DAY,
            cycle_end: NOW - DAY,
            ended: Some(NOW - DAY),
        }),
        Plan {
            m: 2,
            plan_start: NOW - 2 * DAY,
            plan_end: NOW - DAY,
        },
    );
    let v = ok(&home, &["upkeep"]);
    assert_eq!(v["prune"]["cycle_history"], 19);
    assert_eq!(v["prune"]["plan_history"], 7);
    // `tehas history` still answers for both: the two kept ends of the pruned
    // issue, and all five lines of the recent one.
    assert_eq!(
        ok(&home, &["history-show", "40"])["rows"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        ok(&home, &["history-show", "41"])["rows"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
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
        "review-261001-000000.json",
        "review-261001-000000.jsonl",
        "review.log",
        "dispatch.log",
        "clean.log",
        "work-5-261013-000000.jsonl",
    ] {
        fs::write(logs.join(name), b"x").unwrap();
        aged(&logs.join(name), NOW - 8 * DAY);
    }
    aged(&logs.join("work-5-261013-000000.jsonl"), NOW - DAY);
    let v = ok(&home, &["upkeep", "--dry-run"]);
    assert_eq!(v["logs"].as_array().unwrap().len(), 9);
    assert_eq!(v.get("backup"), None);
    assert_eq!(names(&logs).len(), 13);
    assert!(!home.join("state/backup").exists());
    let v = ok(&home, &["upkeep"]);
    assert_eq!(v["logs"].as_array().unwrap().len(), 9);
    assert_eq!(v["backup"]["made"], true);
    assert_eq!(v["prune"]["before"], NOW - 90 * DAY);
    assert_eq!(
        names(&logs),
        vec![
            "clean.log",
            "dispatch.log",
            "review.log",
            "work-5-261013-000000.jsonl"
        ]
    );
}

#[test]
fn locks_held_names_the_held_locks_among_a_thousand_old_ones() {
    // No database: whether a cycle runs is the lock's to say.
    let home = scratch("locks-held");
    let locks = home.join("state/cycles");
    fs::create_dir_all(&locks).unwrap();
    for n in 1..=1000 {
        File::create(locks.join(format!("{n}.lock"))).unwrap();
    }
    let _a = hold(&home, 17);
    let _b = hold(&home, 1001);
    File::create(locks.join("notes.txt")).unwrap();
    let v = ok(&home, &["locks-held"]);
    assert_eq!(v["held"], serde_json::json!([17, 1001]));
    assert_eq!(names(&locks).len(), 1002);
}

#[test]
fn locks_held_counts_a_lock_it_cannot_probe_and_reads_no_directory_as_none() {
    let home = scratch("locks-held-odd");
    assert_eq!(ok(&home, &["locks-held"])["held"], serde_json::json!([]));
    let locks = home.join("state/cycles");
    fs::create_dir_all(locks.join("4.lock")).unwrap();
    assert_eq!(ok(&home, &["locks-held"])["held"], serde_json::json!([4]));
    assert!(usage(&["locks-held", "--all"]).contains("locks-held"));
}

#[test]
fn copy_writes_the_store_to_the_named_file_and_changes_nothing_else() {
    let home = home("copy-to");
    let (_lock, _) = start(&home, 7);
    let to = home.join("elsewhere.db");
    let v = ok(&home, &["copy", "--to", to.to_str().unwrap()]);
    assert_eq!(v["file"], to.to_str().unwrap());
    assert_eq!(mode(&to), 0o600);
    let c = Connection::open(&to).unwrap();
    let n: i64 = c
        .query_row("SELECT count(*) FROM cycles WHERE issue = 7", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(n, 1);
    assert!(!home.join("state/backup").exists());
    // A name that is taken is not overwritten.
    let (code, _) = refused(&home, &["copy", "--to", to.to_str().unwrap()]);
    assert_ne!(code, 0);
}

#[test]
fn copy_needs_an_absolute_name() {
    assert!(usage(&["copy"]).contains("--to"));
    assert!(usage(&["copy", "--to", "here.db"]).contains("absolute"));
}

#[test]
fn copy_holds_a_row_that_is_still_in_the_wal() {
    let home = home("copy-wal");
    // A reader of its own keeps SQLite from folding the WAL into the file.
    let db = home.join("state/tehas.db");
    let reader = Connection::open(&db).unwrap();
    reader
        .query_row("SELECT count(*) FROM cycles", [], |r| r.get::<_, i64>(0))
        .unwrap();
    let (_lock, _) = start(&home, 9);
    let wal = fs::metadata(home.join("state/tehas.db-wal")).unwrap().len();
    assert!(wal > 0, "the row is in the WAL");
    let to = home.join("state/wal-copy.db");
    ok(&home, &["copy", "--to", to.to_str().unwrap()]);
    let c = Connection::open(&to).unwrap();
    let n: i64 = c
        .query_row("SELECT count(*) FROM cycles WHERE issue = 9", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(n, 1);
    drop(reader);
}
