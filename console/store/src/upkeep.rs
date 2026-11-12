//! The store's upkeep, run by tehasbot clean and the poll's sweep:
//!
//!   backup [--show]   one copy a day, state/backup/tehas-<YYYYMMDD>.db, by
//!                     VACUUM INTO; the newest 7 kept. --show only reads: the
//!                     newest copy and its age
//!   copy --to <file>  one consistent copy of the store at the absolute name
//!                     given, by VACUUM INTO; reads the store, writes only that
//!                     file, and refuses a name that is taken, a directory
//!                     that is not there and one under live/. Answers the
//!                     copy's schema, its rows but meta's, and the ms it took
//!   load --from <file>
//!                     a copy of `copy` loaded into a store that holds no
//!                     history, in one transaction: src/load.rs
//!   prune --days N    endings, settled actions with their journal, the
//!                     heartbeats of ended cycles, and the history of issues
//!                     ended more than N days ago, each issue keeping its last
//!                     row of each history. A cycle's row is never removed
//!   sweep-locks [--days N] [--max N] [--dry-run]
//!                     removes state/cycles/<n>.lock when no one holds it and
//!                     nothing has used the issue for N days: its cycle on
//!                     record ended that long ago, or none is on record and the
//!                     file is that old. Each removal is a sweep_log row
//!   locks-held        the issues whose state/cycles/<n>.lock is held, or could
//!                     not be probed, as "held": [n, ...]. One process probes
//!                     every file; the database is not opened, so a cycle the
//!                     store does not know of is still named
//!   upkeep [--dry-run]
//!                     what tehasbot clean asks once an hour: the logs of
//!                     cycles, planning turns, index runs and the analyst
//!                     older than 7 days, the four long-lived logs rolled past
//!                     64 MiB, then backup and prune --days 90. A dry run
//!                     names the logs and touches nothing

use crate::db;
use crate::lock;
use crate::verbs::{self, j, line, list, obj, Args, Fail, Kind, Q};
use rusqlite::{params, OptionalExtension};
use std::fs::{self, File, TryLockError};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[derive(Debug, PartialEq, Eq)]
pub enum Call {
    Backup {
        show: bool,
    },
    Copy {
        to: String,
    },
    Load {
        from: String,
    },
    Prune {
        days: i64,
    },
    SweepLocks {
        days: i64,
        max: i64,
        dry_run: bool,
    },
    LocksHeld,
    /// `cap` is the size a long-lived log may reach before the run cuts it. It
    /// is a field and not `LOG_CAP` at the call so that a test proves the
    /// verb's whole answer on a log it can write; the parse gives `LOG_CAP`,
    /// which is what a host gets.
    Upkeep {
        dry_run: bool,
        cap: u64,
    },
}

/// The daily copies kept.
pub const KEEP: usize = 7;
const DAY: i64 = 86_400;

pub fn parse(name: &str, rest: &[String]) -> Option<Result<Call, String>> {
    Some(match name {
        "backup" => {
            Args::parse("backup", rest, &[("--show", Kind::Switch)], false).map(|a| Call::Backup {
                show: a.has("--show"),
            })
        }
        "copy" => Args::parse("copy", rest, &[("--to", Kind::Value)], false).and_then(|a| {
            let to = a.need("--to")?;
            if !to.starts_with('/') {
                return Err(a.usage("--to", "is an absolute name"));
            }
            Ok(Call::Copy { to: to.to_string() })
        }),
        "load" => Args::parse("load", rest, &[("--from", Kind::Value)], false).and_then(|a| {
            let from = a.need("--from")?;
            if !from.starts_with('/') {
                return Err(a.usage("--from", "is an absolute name"));
            }
            Ok(Call::Load {
                from: from.to_string(),
            })
        }),
        "prune" => Args::parse("prune", rest, &[("--days", Kind::Value)], false).and_then(|a| {
            Ok(Call::Prune {
                days: a.strict_need("--days", "a count of days", days)?,
            })
        }),
        "sweep-locks" => Args::parse(
            "sweep-locks",
            rest,
            &[
                ("--days", Kind::Value),
                ("--max", Kind::Value),
                ("--dry-run", Kind::Switch),
            ],
            false,
        )
        .and_then(|a| {
            Ok(Call::SweepLocks {
                days: a.strict("--days", "a count of days", days)?.unwrap_or(30),
                max: a.strict("--max", "a count", max)?.unwrap_or(3),
                dry_run: a.has("--dry-run"),
            })
        }),
        "locks-held" => Args::parse("locks-held", rest, &[], false).map(|_| Call::LocksHeld),
        "upkeep" => Args::parse("upkeep", rest, &[("--dry-run", Kind::Switch)], false).map(|a| {
            Call::Upkeep {
                dry_run: a.has("--dry-run"),
                cap: LOG_CAP,
            }
        }),
        _ => return None,
    })
}

fn days(s: &str) -> Option<i64> {
    verbs::number(s).filter(|n| (1..=3650).contains(n))
}

fn max(s: &str) -> Option<i64> {
    verbs::number(s).filter(|n| (1..=100).contains(n))
}

pub fn run(home: &Path, call: &Call, now: i64) -> Result<String, Fail> {
    match call {
        Call::Backup { show } => backup(home, *show, now),
        Call::Copy { to } => copy(home, to),
        Call::Load { from } => crate::load::load(home, from, now),
        Call::Prune { days } => prune(home, *days, now),
        Call::SweepLocks { days, max, dry_run } => sweep_locks(home, *days, *max, *dry_run, now),
        Call::Upkeep { dry_run, cap } => upkeep(home, *dry_run, *cap, now),
        Call::LocksHeld => locks_held(home),
    }
}

// --- backup -------------------------------------------------------------------

/// The UTC date of `now` as YYYYMMDD.
pub fn date(now: i64) -> String {
    // Howard Hinnant's days-to-civil.
    let z = now.div_euclid(DAY) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}{m:02}{d:02}")
}

/// `tehas-<8 digits>.db`, a daily copy's name.
fn daily(name: &str) -> bool {
    name.len() == 17
        && name.starts_with("tehas-")
        && name.ends_with(".db")
        && name[6..14].bytes().all(|b| b.is_ascii_digit())
}

/// The daily copies in `dir`, newest name first.
fn copies(dir: &Path) -> Result<Vec<PathBuf>, Fail> {
    let mut found = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(found),
        Err(e) => return Err((1, format!("unreadable {}: {e}", dir.display()))),
    };
    for e in entries {
        let e = e.map_err(|e| (1, format!("unreadable {}: {e}", dir.display())))?;
        if daily(&e.file_name().to_string_lossy()) {
            found.push(e.path());
        }
    }
    found.sort();
    found.reverse();
    Ok(found)
}

fn newest(dir: &Path, now: i64) -> Result<String, Fail> {
    let Some(p) = copies(dir)?.into_iter().next() else {
        return Ok("null".to_string());
    };
    let at = fs::metadata(&p)
        .map_err(|e| (1, format!("unreadable {}: {e}", p.display())))?
        .mtime();
    Ok(obj(&[
        ("file", j(p.to_string_lossy().as_ref())),
        ("at", j(at)),
        ("age_seconds", j((now - at).max(0))),
    ]))
}

fn copy(home: &Path, to: &str) -> Result<String, Fail> {
    verbs::look(home, |_| Ok(()))?;
    let target = Path::new(to);
    if fs::symlink_metadata(target).is_ok() {
        return Err((
            1,
            format!("exists: the target {to} already holds a file; name a new one"),
        ));
    }
    let dir = target.parent().unwrap_or(Path::new("/"));
    if !fs::metadata(dir).is_ok_and(|m| m.is_dir()) {
        return Err((
            1,
            format!("no-directory: the directory of the target {to} is not there"),
        ));
    }
    // Caddy serves live/: a copy there would be the store on the web.
    let live = home.join("live");
    let served = |d: &Path| {
        d.canonicalize()
            .is_ok_and(|d| live.canonicalize().is_ok_and(|l| d.starts_with(l)))
    };
    if served(dir) {
        return Err((
            1,
            format!("served: the target {to} is under live/, which Caddy serves"),
        ));
    }
    let began = std::time::Instant::now();
    // The reader's connection is enough: VACUUM INTO reads one snapshot, so a
    // row still in the WAL is in the copy and no writer waits for it.
    let conn = db::open_look(&db::path(home)).map_err(crate::line)?;
    let copied = db::copy_to(&conn, target).map_err(crate::line)?;
    let ms = i64::try_from(began.elapsed().as_millis()).unwrap_or(i64::MAX);
    Ok(line(
        "copy",
        &[
            ("file", j(to)),
            ("schema", j(copied.schema)),
            ("rows", j(copied.rows)),
            ("ms", j(ms)),
        ],
    ))
}

fn backup(home: &Path, show: bool, now: i64) -> Result<String, Fail> {
    let dir = home.join("state").join("backup");
    // The store is checked as every verb checks it, before anything is made.
    verbs::look(home, |_| Ok(()))?;
    if show {
        return Ok(line("backup", &[("newest", newest(&dir, now)?)]));
    }
    let (made, removed, newest) = make_backup(home, now)?;
    Ok(line(
        "backup",
        &[
            ("made", j(made)),
            ("removed", list(&removed)),
            ("newest", newest),
        ],
    ))
}

/// Today's copy when there is none, and the old ones removed: whether it was
/// made, what was removed and the newest.
fn make_backup(home: &Path, now: i64) -> Result<(bool, Vec<String>, String), Fail> {
    let dir = db::backup_dir(home, &home.join("state").join("backup")).map_err(crate::line)?;
    let copy = dir.join(format!("tehas-{}.db", date(now)));
    let made = !copy.exists();
    if made {
        // Written under a name no reader takes for a copy and renamed once
        // whole: a run killed partway leaves no copy of today behind.
        let part = dir.join(format!("tehas-{}.db.part", date(now)));
        let _ = fs::remove_file(&part);
        // VACUUM INTO reads a snapshot and runs outside any transaction, so a
        // reader's connection is enough and no writer waits for it.
        let conn = db::open_look(&db::path(home)).map_err(crate::line)?;
        db::backup_to(&conn, &part).map_err(crate::line)?;
        fs::rename(&part, &copy).map_err(|e| (1, format!("write {}: {e}", copy.display())))?;
    }
    let mut removed = Vec::new();
    for old in copies(&dir)?.into_iter().skip(KEEP) {
        fs::remove_file(&old).map_err(|e| (1, format!("write {}: {e}", old.display())))?;
        // What a person's look at the copy left beside it goes with it.
        for side in ["-wal", "-shm"] {
            let mut p = old.clone().into_os_string();
            p.push(side);
            let _ = fs::remove_file(p);
        }
        removed.push(j(old.to_string_lossy().as_ref()));
    }
    Ok((made, removed, newest(&dir, now)?))
}

// --- prune --------------------------------------------------------------------

fn prune(home: &Path, days: i64, now: i64) -> Result<String, Fail> {
    Ok(line("prune", &pruned(home, days, now)?))
}

/// One of the two histories `pruned` deletes from. The type is the whole
/// argument: nothing else is a table prune knows, and the keep-rules below hold
/// only for these two names.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HistoryTable {
    CycleHistory,
    PlanHistory,
}

impl HistoryTable {
    fn name(self) -> &'static str {
        match self {
            HistoryTable::CycleHistory => "cycle_history",
            HistoryTable::PlanHistory => "plan_history",
        }
    }
}

/// The DELETE of one history table, as `pruned` runs it. What goes is a row of
/// an issue whose cycle ended before `?1`, and never the row a reader is asking
/// about: `history-show` lists the last row of each table, and `park-show`'s
/// `PARK_SELECT` takes `park_reason` and its `refine` from the parked attempt's
/// newest cycle history row. `upkeep_tests` prunes each of those stores and
/// reads them back, so a keep-rule that stops holding is a failing test there.
/// Rows are told apart by `at` and `id`, the order the writers and their
/// per-issue cap of the 50 newest use, not by `seq`, which each new end is given
/// beyond the rows kept.
pub fn history_delete(table: HistoryTable) -> String {
    let name = table.name();
    // The issue's newest row of one table by `at`, of all its rows or of those
    // the window has passed.
    let last = |from: &str, older: bool| {
        let where_ = if older { " AND h.at < ?1" } else { "" };
        format!(
            "(at, id) NOT IN
             (SELECT at, id FROM {from} h WHERE h.issue = {name}.issue{where_}
                ORDER BY at DESC, id DESC LIMIT 1)"
        )
    };
    // The newest row of the issue's own table, and of its cycle history: a plan
    // row is kept while it is the last end of the issue's cycle on record.
    let own_last = last(name, false);
    let cycle_last = last("cycle_history", false);
    // The newest row of the issue's cycle history that the window has passed, so
    // the row prune keeps is the end still in `endings` when `cycles.ended_at`
    // names the same end. The two tables are written by the same `now` on the
    // end path; a hand-repair that moves one of them apart is the case this is
    // here for.
    let cycle_last_before_window = last("cycle_history", true);
    // `cycle-end --stopped` and `--refused` write no cycle_history row, so this
    // is the only thing keeping the planning rows of those issues, and the only
    // record of how their plan went. Dropping it as redundant would erase it.
    let has_cycle_history = "issue IN (SELECT issue FROM cycle_history)";
    format!(
        "DELETE FROM {name} WHERE
           issue IN (SELECT issue FROM cycles WHERE ended_at < ?1)
           AND {has_cycle_history}
           AND {own_last} AND {cycle_last} AND {cycle_last_before_window}"
    )
}

fn pruned(home: &Path, days: i64, now: i64) -> Result<Vec<(&'static str, String)>, Fail> {
    let before = now - days * DAY;
    verbs::write(home, |c| {
        let endings = c
            .execute("DELETE FROM endings WHERE ended_at < ?1", [before])
            .q()?;
        let settled = "SELECT id FROM actions WHERE settled_at IS NOT NULL AND settled_at < ?1
                       AND state IN ('done','failed','abandoned')";
        let journal = c
            .execute(
                &format!("DELETE FROM journal WHERE action IN ({settled})"),
                [before],
            )
            .q()?;
        let actions = c
            .execute(
                &format!("DELETE FROM actions WHERE id IN ({settled})"),
                [before],
            )
            .q()?;
        let heartbeats = c
            .execute(
                "DELETE FROM heartbeats WHERE at < ?1 AND issue IN
                   (SELECT issue FROM cycles WHERE ended_at IS NOT NULL)",
                [before],
            )
            .q()?;
        let sweep_log = c
            .execute("DELETE FROM sweep_log WHERE at < ?1", [before])
            .q()?;
        let poll_cost = c
            .execute("DELETE FROM poll_cost WHERE at < ?1", [before])
            .q()?;
        // A poll's row goes once no action names it: an action intended and
        // never settled keeps its poll (section 5), and says so by count.
        let polls = c
            .execute(
                "DELETE FROM polls WHERE at < ?1
                   AND NOT EXISTS (SELECT 1 FROM actions WHERE poll_at = polls.at)",
                [before],
            )
            .q()?;
        let polls_kept: i64 = c
            .query_row("SELECT count(*) FROM polls WHERE at < ?1", [before], |r| {
                r.get(0)
            })
            .q()?;
        // The history of an issue that ended long ago goes with its endings row:
        // the cap of the 50 newest per issue, in cycle-end and plan-end, bounds
        // one issue but not the factory, so the two tables otherwise grow with the
        // issues the factory has ever run.
        let cycle_history = c
            .execute(&history_delete(HistoryTable::CycleHistory), [before])
            .q()?;
        let plan_history = c
            .execute(&history_delete(HistoryTable::PlanHistory), [before])
            .q()?;
        Ok(vec![
            ("before", j(before)),
            ("endings", j(endings)),
            ("actions", j(actions)),
            ("journal", j(journal)),
            ("heartbeats", j(heartbeats)),
            ("sweep_log", j(sweep_log)),
            ("poll_cost", j(poll_cost)),
            ("polls", j(polls)),
            ("polls_kept", j(polls_kept)),
            ("cycle_history", j(cycle_history)),
            ("plan_history", j(plan_history)),
        ])
    })
}

// --- sweep-locks --------------------------------------------------------------

/// Whether nothing has used the issue since `before`: its cycle on record
/// ended before then, or there is none and the lock file is that old.
fn unused(c: &rusqlite::Connection, file: &Path, issue: i64, before: i64) -> Result<bool, Fail> {
    let row: Option<(i64, Option<i64>)> = c
        .query_row(
            "SELECT started_at, ended_at FROM cycles WHERE issue = ?1",
            [issue],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .q()?;
    Ok(match row {
        Some((_, None)) => false,
        Some((_, Some(ended))) => ended < before,
        None => fs::symlink_metadata(file).is_ok_and(|m| m.mtime() < before),
    })
}

/// Removes the lock file while holding it, when it is still the file that was
/// probed. False when someone holds it or it changed. tehasbot cycle opens its
/// lock with `exec 9>`, which touches the file before it waits for the lock:
/// a file touched in the last minute may have a waiter on it, and removing it
/// would leave that waiter holding a lock no one else can see.
fn remove_held(file: &Path) -> Result<bool, String> {
    let before = fs::symlink_metadata(file).map_err(|e| e.to_string())?;
    if !before.file_type().is_file() {
        return Ok(false);
    }
    let f = File::open(file).map_err(|e| e.to_string())?;
    match f.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => return Ok(false),
        Err(TryLockError::Error(e)) => return Err(e.to_string()),
    }
    let m = f.metadata().map_err(|e| e.to_string())?;
    if m.dev() != before.dev() || m.ino() != before.ino() {
        return Ok(false);
    }
    let touched = m.modified().map_err(|e| e.to_string())?;
    if touched + Duration::from_secs(60) > SystemTime::now() {
        return Ok(false);
    }
    fs::remove_file(file).map_err(|e| e.to_string())?;
    Ok(true)
}

fn sweep_locks(home: &Path, days: i64, max: i64, dry_run: bool, now: i64) -> Result<String, Fail> {
    let before = now - days * DAY;
    let issues = lock::listed(home).map_err(|e| (1, format!("unreadable state/cycles: {e}")))?;
    let mut picked = Vec::new();
    verbs::look(home, |c| {
        for issue in issues {
            if picked.len() as i64 >= max {
                break;
            }
            let file = lock::path(home, issue);
            if lock::probe_file(&file).is_none_or(|l| l.rc != 0) {
                continue;
            }
            if unused(c, &file, issue, before)? {
                picked.push((issue, file));
            }
        }
        Ok(())
    })?;
    let mut removed = Vec::new();
    let mut kept = Vec::new();
    for (issue, file) in &picked {
        if dry_run {
            removed.push(*issue);
            continue;
        }
        match remove_held(file) {
            Ok(true) => removed.push(*issue),
            Ok(false) => kept.push(*issue),
            Err(e) => return Err((1, format!("write state/cycles/{issue}.lock: {e}"))),
        }
    }
    if !dry_run && !removed.is_empty() {
        verbs::write(home, |c| {
            for issue in &removed {
                c.execute(
                    "INSERT OR REPLACE INTO sweep_log (issue, at, action, what, reason)
                     VALUES (?1, ?2, 'removed', 'lock', ?3)",
                    params![issue, now, format!("unused for {days} days")],
                )
                .q()?;
            }
            Ok(())
        })?;
    }
    let ids = |v: &[i64]| list(&v.iter().map(|n| j(*n)).collect::<Vec<_>>());
    Ok(line(
        "sweep-locks",
        &[
            ("dry_run", j(dry_run)),
            ("removed", ids(&removed)),
            ("kept", ids(&kept)),
        ],
    ))
}

/// A lock that cannot be probed counts as held, as a failed `flock -n` did.
fn locks_held(home: &Path) -> Result<String, Fail> {
    let issues = lock::listed(home).map_err(|e| (1, format!("unreadable state/cycles: {e}")))?;
    let held: Vec<String> = issues
        .into_iter()
        .filter(|n| lock::probe_file(&lock::path(home, *n)).is_some_and(|l| l.rc != 0))
        .map(j)
        .collect();
    Ok(line("locks-held", &[("held", list(&held))]))
}

// --- upkeep -------------------------------------------------------------------

/// The logs upkeep removes: a cycle's, a planning turn's, an index run's, a
/// review's and the analyst's. The four long-lived logs are not among them:
/// `roll` keeps those under the cap by copy and truncate instead, because the
/// units hold them open for append.
fn old_log(name: &str) -> bool {
    name.starts_with("work-")
        || name.starts_with("dispatch-work-")
        || name.starts_with("index-")
        || (name.starts_with("improve-analyst") && name.ends_with(".log"))
        || (name.starts_with("plan-") && name.ends_with(".jsonl"))
        || (name.starts_with("dispatch-plan-") && name.ends_with(".log"))
        || (name.starts_with("review-") && (name.ends_with(".json") || name.ends_with(".jsonl")))
}

const LOG_DAYS: i64 = 7;
const PRUNE_DAYS: i64 = 90;

/// The size one long-lived log may reach before upkeep rolls it: one figure for
/// the four, no setting for it, and nothing fsynced. The cap is a function
/// argument all the way to `upkeep`, and the parse of the verb gives this, so a
/// test proves the same rule on a log it can write instead of 64 MiB.
pub const LOG_CAP: u64 = 64 * 1024 * 1024;

/// The long-lived logs the units and the shim append to, by the fixed names of
/// the tools that read them: the dispatcher's and the refresh's in `logs/`, the
/// review timer's, and the `gh` shim's record of every GitHub write in `state/`.
/// The four names are written out, never found by a pattern, and a rotated
/// `.1` is never a candidate for another cut.
pub const LONG_LIVED: [&str; 4] = [
    "logs/dispatch.log",
    "logs/review-timer.log",
    "logs/refresh.log",
    "state/github-writes.log",
];

/// Whether one long-lived log is due to be cut: a plain file over `cap`, so
/// never a link, a directory or a fifo.
fn rollable(m: &fs::Metadata, cap: u64) -> bool {
    m.file_type().is_file() && m.len() > cap
}

/// Cut one long-lived log past `cap` by copy and truncate: the whole file is
/// copied to `<name>.1`, replacing an older `.1`, and the file it was read from
/// is then emptied. Nothing is renamed, because the dispatcher's unit and every
/// cycle's hold the file open for append and would go on writing to a renamed
/// one. The two steps are not one move, so the lines an open writer adds
/// between them are lost: they are the price of a truncate no writer notices.
/// False for a log that is not there, is not a plain file, or is not over the
/// cap. The verb passes `LOG_CAP`.
pub fn roll(home: &Path, name: &str, cap: u64) -> Result<bool, Fail> {
    let path = home.join(name);
    let written = |e: std::io::Error| (1, format!("write {name}: {e}"));
    let m = match fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err((1, format!("unreadable {name}: {e}"))),
    };
    if !rollable(&m, cap) {
        return Ok(false);
    }
    let copy = {
        let mut copy = path.clone().into_os_string();
        copy.push(".1");
        PathBuf::from(copy)
    };
    // A copy, not a rename: the writer holds the file, not the name. An older
    // `.1` is replaced, so one generation is kept, as the MCP tail reads. It is
    // removed first and the copy made new, so a `.1` that is a link is replaced
    // and nothing is written through it.
    match fs::remove_file(&copy) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(written(e)),
    }
    let mut to = File::options()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&copy)
        .map_err(written)?;
    std::io::copy(&mut File::open(&path).map_err(written)?, &mut to).map_err(written)?;
    // Truncate the file that was measured, not whatever holds the name now.
    let log = File::options().write(true).open(&path).map_err(written)?;
    let now = log.metadata().map_err(written)?;
    if now.dev() != m.dev() || now.ino() != m.ino() {
        return Ok(false);
    }
    log.set_len(0).map_err(written)?;
    Ok(true)
}

/// The long-lived logs over `cap`, cut to a `.1` and an empty file: the names
/// of the ones that were, in `LONG_LIVED`'s order. A cap no file is over leaves
/// every log as it is, which is how a dry run reports without writing.
fn roll_logs(home: &Path, cap: u64) -> Result<Vec<String>, Fail> {
    let mut rolled = Vec::new();
    for name in LONG_LIVED {
        if roll(home, name, cap)? {
            rolled.push(j(name));
        }
    }
    Ok(rolled)
}

fn upkeep(home: &Path, dry_run: bool, cap: u64, now: i64) -> Result<String, Fail> {
    verbs::look(home, |_| Ok(()))?;
    let logs = home.join("logs");
    let mut gone = Vec::new();
    let mut names: Vec<(String, PathBuf)> = match fs::read_dir(&logs) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
            .filter(|(n, _)| old_log(n))
            .collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err((1, format!("unreadable logs: {e}"))),
    };
    names.sort();
    for (name, path) in names {
        let Ok(m) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !m.file_type().is_file() || m.mtime() >= now - LOG_DAYS * DAY {
            continue;
        }
        if !dry_run {
            fs::remove_file(&path).map_err(|e| (1, format!("write logs/{name}: {e}")))?;
        }
        gone.push(j(name));
    }
    // The long-lived logs are cut after the old ones are removed, so one answer
    // says both what went and what was cut.
    let rolled = roll_logs(home, if dry_run { u64::MAX } else { cap })?;
    let mut pairs = vec![
        ("dry_run", j(dry_run)),
        ("logs", list(&gone)),
        ("rolled", list(&rolled)),
    ];
    if !dry_run {
        let (made, removed, newest) = make_backup(home, now)?;
        pairs.push((
            "backup",
            obj(&[
                ("made", j(made)),
                ("removed", list(&removed)),
                ("newest", newest),
            ]),
        ));
        pairs.push(("prune", obj(&pruned(home, PRUNE_DAYS, now)?)));
    }
    Ok(line("upkeep", &pairs))
}
