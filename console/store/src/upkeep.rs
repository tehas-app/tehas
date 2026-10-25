//! The store's upkeep, run by tehasbot clean and the poll's sweep:
//!
//!   backup [--show]   one copy a day, state/backup/tehas-<YYYYMMDD>.db, by
//!                     VACUUM INTO; the newest 7 kept. --show only reads: the
//!                     newest copy and its age
//!   prune --days N    endings, settled actions with their journal, and the
//!                     heartbeats of ended cycles, older than N days. A cycle's
//!                     row is never removed
//!   sweep-locks [--days N] [--max N] [--dry-run]
//!                     removes state/cycles/<n>.lock when no one holds it and
//!                     nothing has used the issue for N days: its cycle on
//!                     record ended that long ago, or none is on record and the
//!                     file is that old. Each removal is a sweep_log row
//!   upkeep [--dry-run]
//!                     what tehasbot clean asks once an hour: the logs of
//!                     cycles, planning turns, index runs and the analyst
//!                     older than 7 days, then backup and prune --days 90. A
//!                     dry run names the logs and touches nothing

use crate::db;
use crate::lock;
use crate::verbs::{self, j, line, list, obj, Args, Fail, Kind, Q};
use rusqlite::{params, OptionalExtension};
use std::fs::{self, File, TryLockError};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[derive(Debug, PartialEq, Eq)]
pub enum Call {
    Backup { show: bool },
    Prune { days: i64 },
    SweepLocks { days: i64, max: i64, dry_run: bool },
    Upkeep { dry_run: bool },
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
        "upkeep" => Args::parse("upkeep", rest, &[("--dry-run", Kind::Switch)], false).map(|a| {
            Call::Upkeep {
                dry_run: a.has("--dry-run"),
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
        Call::Prune { days } => prune(home, *days, now),
        Call::SweepLocks { days, max, dry_run } => sweep_locks(home, *days, *max, *dry_run, now),
        Call::Upkeep { dry_run } => upkeep(home, *dry_run, now),
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
        Ok(vec![
            ("before", j(before)),
            ("endings", j(endings)),
            ("actions", j(actions)),
            ("journal", j(journal)),
            ("heartbeats", j(heartbeats)),
            ("sweep_log", j(sweep_log)),
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

// --- upkeep -------------------------------------------------------------------

/// The logs upkeep removes: a cycle's, a planning turn's, an index run's, a
/// review's and the analyst's. Long-lived logs such as dispatch.log are not among them.
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

fn upkeep(home: &Path, dry_run: bool, now: i64) -> Result<String, Fail> {
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
    let mut pairs = vec![("dry_run", j(dry_run)), ("logs", list(&gone))];
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
