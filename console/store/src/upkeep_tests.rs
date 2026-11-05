//! backup, prune and sweep-locks.

use crate::upkeep::{date, history_delete, HistoryTable, KEEP};
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
    n: usize,
    start: i64,
    cycle_end: i64,
    ended: Option<i64>,
}

/// What the plan of `issue` ran: `m` turns of one session from `plan_start` to
/// `plan_end`.
struct Plan {
    m: usize,
    plan_start: i64,
    plan_end: i64,
}

/// The issue of a factory that ran the cycle and the plan it is given. Rows are
/// written by the verb that writes them in a factory, except where a case names
/// a divergence on purpose: the `ended` instant a builder names, which the verbs
/// leave to the end that ran, and what `row` writes. The cycle rows are
/// dated between the two cycle instants and the plan rows between the two
/// planning ones, so the newest row of either table is the one a test names by
/// the attempt or the turn it is.
fn history(home: &Path, issue: i64, cycle: Option<Cycle>, plan: Plan) {
    let start = match &cycle {
        Some(c) => c.start,
        None => plan.plan_start,
    };
    let n_s = issue.to_string();
    conn(home)
        .execute(
            "INSERT OR IGNORE INTO issues (issue, kind, kind_at, checked_at) VALUES (?1, 'closed', ?2, 0)",
            rusqlite::params![issue, start],
        )
        .unwrap();
    let ended = match cycle {
        Some(Cycle {
            n,
            start,
            cycle_end,
            ended,
        }) => {
            if n > 0 {
                let lock = hold(home, issue);
                // The approval the first start is made from; a later attempt is
                // started by hand, which needs none.
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
                let n = i64::try_from(n).unwrap();
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
            ended
        }
        // No cycle on record: nothing to date, nothing to end.
        None => None,
    };
    let Plan {
        m,
        plan_start,
        plan_end,
    } = plan;
    conn(home)
        .execute(
            "UPDATE cycles SET ended_at = ?1 WHERE issue = ?2",
            rusqlite::params![ended, issue],
        )
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
        let m = i64::try_from(m).unwrap();
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
fn row(home: &Path, name: HistoryTable, issue: i64, n: i64, at: i64) {
    let (table, column) = table(name);
    // One history's own writer gives the row its `seq`, its status or outcome
    // and its place among the 50 newest; here only the `at` is the test's.
    let (last, status) = match name {
        HistoryTable::CycleHistory => ("status", "closed"),
        HistoryTable::PlanHistory => ("outcome", "finished"),
    };
    conn(home)
        .execute(
            &format!("INSERT INTO {table} (issue, {column}, at, {last}) VALUES (?1, ?2, ?3, ?4)"),
            rusqlite::params![issue, n, at, status],
        )
        .unwrap();
}

/// The issue's last end is `ago` seconds before the window's edge, with `n`
/// cycle ends and `m` planning turns. The keep-rule of an issue that ended long
/// ago applies to it.
fn ended_long_ago(home: &Path, issue: i64, before_window: i64, n: usize, m: usize) {
    let end = NOW - WINDOW - before_window;
    history(
        home,
        issue,
        Some(Cycle {
            n,
            start: end - 40,
            cycle_end: end,
            ended: Some(end),
        }),
        Plan {
            m,
            plan_start: end - 40,
            plan_end: end,
        },
    );
}

/// Like `ended_long_ago`, but the cycle wrote no history row of its own, as
/// `cycle-end --stopped` or `--refused` leaves it. The issue's planning rows are
/// all the store holds of it.
fn ended_without_history(home: &Path, issue: i64, m: usize) {
    let end = NOW - WINDOW - 10;
    history(
        home,
        issue,
        Some(Cycle {
            n: 0,
            start: end - 40,
            cycle_end: end,
            ended: Some(end),
        }),
        Plan {
            m,
            plan_start: end - 40,
            plan_end: end,
        },
    );
}

/// Its cycle ended long ago, its planning turned inside the window, and one row
/// of each table is older than its last end, as a row of an old store can be.
/// `row` writes the two old rows, so the pruner is asked about a row older than
/// the issue's own end.
fn ended_with_rows_of_the_window(home: &Path, issue: i64) {
    let old = NOW - 100 * DAY;
    history(
        home,
        issue,
        Some(Cycle {
            n: 4,
            start: old - 40,
            cycle_end: old - 30,
            ended: Some(old),
        }),
        Plan {
            m: 3,
            plan_start: old - 40,
            plan_end: NOW - DAY,
        },
    );
    row(home, HistoryTable::CycleHistory, issue, 5, old - 35);
    row(home, HistoryTable::PlanHistory, issue, 6, old - 39);
}

/// Its cycle ended `ago` seconds ago, after the window opened, with `n` ends and
/// `m` turns. Nothing of it is old enough to go.
fn ended_recently(home: &Path, issue: i64, ago: i64, n: usize, m: usize) {
    history(
        home,
        issue,
        Some(Cycle {
            n,
            start: NOW - ago - 20,
            cycle_end: NOW - ago,
            ended: Some(NOW - ago),
        }),
        Plan {
            m,
            plan_start: NOW - ago - 20,
            plan_end: NOW - ago,
        },
    );
}

/// A cycle of `n` ends that is still running, so its cycle row has no end. Every
/// row stays whatever its age.
fn still_running(home: &Path, issue: i64, n: usize) {
    history(
        home,
        issue,
        Some(Cycle {
            n,
            start: NOW - 140 * DAY,
            cycle_end: NOW - 120 * DAY,
            ended: None,
        }),
        Plan {
            m: 0,
            plan_start: NOW - 120 * DAY,
            plan_end: NOW - 120 * DAY,
        },
    );
}

/// The planning of an issue the store has no cycle row for: nothing dates it, so
/// nothing of it goes.
fn no_cycle_row(home: &Path, issue: i64, m: usize) {
    history(
        home,
        issue,
        None,
        Plan {
            m,
            plan_start: NOW - 140 * DAY,
            plan_end: NOW - 110 * DAY,
        },
    );
}

/// The window of the `prune --days 90` the tests ask for.
const WINDOW: i64 = 90 * DAY;

/// The counts of the two histories `prune` answers with.
fn pruned_rows(home: &Path) -> (i64, i64) {
    let v = ok(home, &["prune", "--days", "90"]);
    let of = |key: &str| {
        v[key]
            .as_i64()
            .unwrap_or_else(|| panic!("prune answer has no {key} number: {v}"))
    };
    (of("cycle_history"), of("plan_history"))
}

/// One of the two histories, in the words a test names it by.
fn table(name: HistoryTable) -> (&'static str, &'static str) {
    match name {
        HistoryTable::CycleHistory => ("cycle_history", "attempt"),
        HistoryTable::PlanHistory => ("plan_history", "turn"),
    }
}

/// How many rows one history of `issue` holds.
fn rows(home: &Path, name: HistoryTable, issue: i64) -> i64 {
    let (table, _) = table(name);
    count(home, &format!("{table} WHERE issue = {issue}"))
}

/// What one history of `issue` still holds, as (attempt or turn, `at`) in the
/// order the rows were written, so a case names the rows it expects to survive.
fn kept(home: &Path, name: HistoryTable, issue: i64) -> Vec<(i64, i64)> {
    let (table, column) = table(name);
    let c = conn(home);
    let mut q = c
        .prepare(&format!(
            "SELECT {column}, at FROM {table} WHERE issue = ?1 ORDER BY seq"
        ))
        .unwrap();
    q.query_map([issue], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// How many lines `tehas history` still answers with for `issue`.
fn history_lines(home: &Path, issue: i64) -> usize {
    ok(home, &["history-show", &issue.to_string()])["rows"]
        .as_array()
        .unwrap_or_else(|| panic!("history-show answers with no rows array for {issue}"))
        .len()
}

#[test]
fn prune_keeps_the_last_row_of_each_history_of_an_issue_that_ended_long_ago() {
    let home = home("prune-long-ago");
    ended_long_ago(&home, 30, 10, 30, 12);

    // 29 of the 30 cycle rows and 11 of the 12 plan rows; the last of each stays.
    assert_eq!(pruned_rows(&home), (29, 11));
    assert_eq!(rows(&home, HistoryTable::CycleHistory, 30), 1);
    assert_eq!(rows(&home, HistoryTable::PlanHistory, 30), 1);
    assert_eq!(
        one::<i64>(&home, "SELECT attempt FROM cycle_history WHERE issue = 30"),
        30
    );
    assert_eq!(
        one::<i64>(&home, "SELECT turn FROM plan_history WHERE issue = 30"),
        12
    );
    assert_eq!(history_lines(&home, 30), 2);
}

#[test]
fn prune_keeps_every_plan_row_of_an_issue_whose_cycle_wrote_no_history() {
    let home = home("prune-no-history");
    // `cycle-end --stopped` or `--refused`: the plan rows are the only record of
    // how the plan of the issue went.
    ended_without_history(&home, 31, 4);

    assert_eq!(pruned_rows(&home), (0, 0));
    assert_eq!(rows(&home, HistoryTable::CycleHistory, 31), 0);
    assert_eq!(rows(&home, HistoryTable::PlanHistory, 31), 4);
}

/// The plan rows of an issue whose cycle ended long ago with no cycle history
/// of its own, and one cycle history row of another issue: the guard is the only
/// thing between those rows and the delete, so it is asked about with the rest
/// of the table's rows eligible.
#[test]
fn prune_takes_no_plan_row_of_an_issue_whose_cycle_wrote_no_history() {
    let home = home("prune-plan-record");
    ended_without_history(&home, 31, 4);
    // Another issue of the same store, ended long ago with its history on record.
    ended_long_ago(&home, 30, 10, 5, 3);

    let (cycle_history, plan_history) = pruned_rows(&home);
    assert_eq!(cycle_history, 4);
    // None of the four plan rows of issue 31 is among them, though its cycle
    // ended as long ago as issue 30's.
    assert_eq!(plan_history, 2);
    assert_eq!(rows(&home, HistoryTable::PlanHistory, 31), 4);
}

#[test]
fn prune_keeps_the_last_row_and_the_last_row_before_the_window() {
    let home = home("prune-rows-of-window");
    ended_with_rows_of_the_window(&home, 32);

    // Of the five cycle rows the last of the issue's ends is kept and the other
    // four go, the row older than its end among them. Of the four plan rows the
    // last of the issue's turns is kept and the three older ones go.
    assert_eq!(pruned_rows(&home), (4, 3));
    assert_eq!(rows(&home, HistoryTable::CycleHistory, 32), 1);
    assert_eq!(rows(&home, HistoryTable::PlanHistory, 32), 1);
    assert_eq!(
        kept(&home, HistoryTable::CycleHistory, 32),
        vec![(4, NOW - 100 * DAY - 30)]
    );
    assert_eq!(
        kept(&home, HistoryTable::PlanHistory, 32),
        vec![(3, NOW - DAY)]
    );
}

#[test]
fn prune_keeps_every_row_of_a_recent_or_running_issue_or_a_plan_of_no_cycle() {
    let home = home("prune-kept");
    ended_recently(&home, 33, DAY, 6, 4);
    still_running(&home, 34, 6);
    no_cycle_row(&home, 35, 5);

    assert_eq!(pruned_rows(&home), (0, 0));
    assert_eq!(rows(&home, HistoryTable::CycleHistory, 33), 6);
    assert_eq!(rows(&home, HistoryTable::PlanHistory, 33), 4);
    assert_eq!(rows(&home, HistoryTable::CycleHistory, 34), 6);
    assert_eq!(rows(&home, HistoryTable::PlanHistory, 34), 0);
    assert_eq!(rows(&home, HistoryTable::PlanHistory, 35), 5);
}

#[test]
fn prune_of_the_six_issues_takes_their_old_rows_once_and_then_nothing() {
    let home = home("prune-history");
    ended_long_ago(&home, 30, 10, 30, 12);
    ended_without_history(&home, 31, 4);
    ended_with_rows_of_the_window(&home, 32);
    ended_recently(&home, 33, DAY, 6, 4);
    still_running(&home, 34, 6);
    no_cycle_row(&home, 35, 5);

    // 30 plus 5 cycle rows and 12 plus 4 plan rows to start with, of which the
    // keeps leave 1 + 2 cycle rows and 1 + 1 plan rows: issue 31 wrote no cycle
    // history and keeps all four of its plan rows, and nothing of issues 33, 34
    // and 35 is old enough to ask about.
    let (cycle_history, plan_history) = pruned_rows(&home);
    assert_eq!(cycle_history, 33);
    assert_eq!(plan_history, (12 + 4 + 4) - (1 + 1 + 4));

    // A repeat of the verb takes nothing more.
    assert_eq!(pruned_rows(&home), (0, 0));
}

#[test]
fn prune_keeps_the_row_park_show_and_history_show_still_read() {
    let home = home("prune-readers");
    // The intent park of a cycle a label started, as `park_tests` makes one, of
    // an issue whose cycle ended long ago: `park-show` reads the park reason from
    // the issue's newest cycle history row by `max(id)`, and derives `refine`
    // from it, so both rest on that row surviving prune.
    intent_parked(&home, 36, NOW - WINDOW - 10);
    let before = ok(&home, &["park-show", "36"]);
    assert_eq!(before["park"]["park_reason"], "underspecified");
    assert_eq!(before["park"]["refine"], true);

    ok(&home, &["prune", "--days", "90"]);

    // `park-show` still reads its park reason from the row prune kept, so `refine`
    // still says the poll owes one, and `history-show` still names an end of it.
    let after = ok(&home, &["park-show", "36"]);
    assert_eq!(after["park"]["park_reason"], "underspecified");
    assert_eq!(after["park"]["refine"], true);
    assert_eq!(
        kept(&home, HistoryTable::CycleHistory, 36),
        vec![(1, NOW - WINDOW - 10)]
    );
    assert_eq!(history_lines(&home, 36), 1);
}

#[test]
fn upkeep_prunes_the_history_and_history_still_answers() {
    let home = home("upkeep-history");
    ended_long_ago(&home, 40, 30, 20, 8);
    // A recent issue: its cycle ended within the window, so all its rows stay.
    ended_recently(&home, 41, DAY, 3, 2);
    let v = ok(&home, &["upkeep"]);
    assert_eq!(v["prune"]["cycle_history"], 19);
    assert_eq!(v["prune"]["plan_history"], 7);
    // `tehas history` still answers for both: the two kept ends of the pruned
    // issue, and all five lines of the recent one.
    assert_eq!(history_lines(&home, 40), 2);
    assert_eq!(history_lines(&home, 41), 5);
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

/// The intent park of a cycle of `issue` started at `start` and ended there
/// parked with its reason underspecified, as the cycle's own attempt:
/// `park-show` reads that reason from the issue's newest cycle history row and
/// derives `refine` from it.
fn intent_parked(home: &Path, issue: i64, start: i64) {
    let (_lock, attempt) = start_by(home, issue, "alice", "label", start);
    let n = issue.to_string();
    let a = attempt.to_string();
    ok_at(
        home,
        start,
        &[
            "cycle-end",
            &n,
            "--attempt",
            &a,
            "--status",
            "parked",
            "--cap",
            "intent-park",
            "--park-reason",
            "underspecified",
            "--parked",
        ],
    );
    ok(
        home,
        &["park", &n, "--attempt", &a, "--reason", "intent-park"],
    );
}

#[test]
fn the_prune_delete_names_its_table_and_its_four_clauses() {
    // The shape of the DELETE, pinned for both tables: what it deletes from, the
    // two issue guards and the three keeps. A change here is a change to the
    // keep-rule, not a reformatting.
    for (table, name) in [
        (HistoryTable::CycleHistory, "cycle_history"),
        (HistoryTable::PlanHistory, "plan_history"),
    ] {
        let sql = history_delete(table);
        let said = |what: &str| assert!(sql.contains(what), "{name}: no {what} in\n{sql}");
        said(&format!("DELETE FROM {name} WHERE"));
        said("issue IN (SELECT issue FROM cycles WHERE ended_at < ?1)");
        // `cycle-end --stopped` and `--refused` write no cycle_history row, so
        // this guard is the only thing keeping those issues' plan rows.
        said("issue IN (SELECT issue FROM cycle_history)");
        said(&format!(
            "(at, id) NOT IN\n             (SELECT at, id FROM {name} h WHERE h.issue = {name}.issue\n                ORDER BY at DESC, id DESC LIMIT 1)"
        ));
        said("FROM cycle_history h WHERE h.issue = ");
        said("AND h.at < ?1");
        // The last three clauses are the keeps: own, cycle, cycle before window.
        assert_eq!(sql.matches("(at, id) NOT IN").count(), 3, "{name}:\n{sql}");
        assert_eq!(sql.matches("ORDER BY at DESC, id DESC LIMIT 1").count(), 3);
    }
}
