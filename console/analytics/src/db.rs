//! The database file: opening it, the schema, events, pruning and the export.

use rusqlite::{params, Connection, OpenFlags};
use std::fs::{self, OpenOptions};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// PRAGMA user_version of the schema in schema.sql.
pub const SCHEMA_VERSION: i64 = 1;
pub const SCHEMA: &str = include_str!("schema.sql");
pub const RETENTION_DAYS: i64 = 10;
const DAY: i64 = 86_400;

pub const EVENTS: [&str; 7] = [
    "ready", "started", "ended", "awaiting", "merged", "closed", "stopped",
];

pub fn path(home: &Path) -> PathBuf {
    home.join("state").join("analytics.db")
}

/// Opens the database, making it when there is none. A file that is not a
/// database of this schema is moved to analytics.db.broken and a new one is
/// made: the data is only ten days deep. The second value says whether that
/// happened.
pub fn open(file: &Path, now: i64) -> Result<(Connection, bool), String> {
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("cannot make {}: {e}", dir.display()))?;
    }
    match try_open(file, now) {
        Ok(conn) => Ok((conn, false)),
        Err(Unusable::Locked(e)) => Err(e),
        Err(Unusable::Broken(why)) => {
            let broken = file.with_extension("db.broken");
            fs::rename(file, &broken)
                .map_err(|e| format!("{why}, and it cannot be moved aside: {e}"))?;
            for side in ["db-wal", "db-shm"] {
                let _ = fs::remove_file(file.with_extension(side));
            }
            let conn = try_open(file, now).map_err(|e| match e {
                Unusable::Locked(e) | Unusable::Broken(e) => e,
            })?;
            Ok((conn, true))
        }
    }
}

enum Unusable {
    // Busy or locked: another writer has it. Not a reason to move it aside.
    Locked(String),
    Broken(String),
}

fn classify(e: rusqlite::Error) -> Unusable {
    match e.sqlite_error_code() {
        Some(rusqlite::ErrorCode::DatabaseBusy) | Some(rusqlite::ErrorCode::DatabaseLocked) => {
            Unusable::Locked(format!("the database is busy: {e}"))
        }
        _ => Unusable::Broken(format!("the database is not usable: {e}")),
    }
}

fn try_open(file: &Path, now: i64) -> Result<Connection, Unusable> {
    // Made 0600 before SQLite sees it; its -wal and -shm take the same mode.
    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(file)
        .map_err(|e| Unusable::Locked(format!("cannot open {}: {e}", file.display())))?;
    let _ = fs::set_permissions(file, fs::Permissions::from_mode(0o600));
    let conn = Connection::open_with_flags(
        file,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(classify)?;
    conn.busy_timeout(Duration::from_secs(5)).map_err(classify)?;
    let check: String = conn
        .query_row("PRAGMA quick_check", [], |r| r.get(0))
        .map_err(classify)?;
    if check != "ok" {
        return Err(Unusable::Broken(format!(
            "the database failed its check: {check}"
        )));
    }
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(classify)?;
    if version == 0 {
        let tables: i64 = conn
            .query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get(0))
            .map_err(classify)?;
        if tables != 0 {
            return Err(Unusable::Broken(
                "the database has tables but no schema version".to_string(),
            ));
        }
    } else if version != SCHEMA_VERSION {
        // No migration exists yet: a schema this program does not know is
        // moved aside, as a broken file is.
        return Err(Unusable::Broken(format!(
            "the database has schema {version}, this program writes {SCHEMA_VERSION}"
        )));
    }
    // Only now: a file that is moved aside is moved as it was found.
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(classify)?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(classify)?;
    if version == 0 {
        create(&conn, now).map_err(classify)?;
    }
    Ok(conn)
}

fn create(conn: &Connection, now: i64) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(SCHEMA)?;
    for (key, value) in [
        ("schema", SCHEMA_VERSION.to_string()),
        ("retention_days", RETENTION_DAYS.to_string()),
        ("created", now.to_string()),
    ] {
        tx.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
    }
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit()
}

/// Records an issue's event, the first time only. Returns whether it was new.
pub fn event(conn: &Connection, issue: i64, event: &str, at: i64) -> rusqlite::Result<bool> {
    let n = conn.execute(
        "INSERT OR IGNORE INTO issue_events (at, issue, event) VALUES (?1, ?2, ?3)",
        params![at, issue, event],
    )?;
    Ok(n == 1)
}

/// Deletes what is older than ten days. A run is as old as its end. The
/// events of an issue that started and has not ended stay until it ends.
pub fn prune(conn: &Connection, now: i64) -> rusqlite::Result<()> {
    let cutoff = now - RETENTION_DAYS * DAY;
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM runs WHERE coalesce(ended, started) < ?1",
        [cutoff],
    )?;
    tx.execute(
        "DELETE FROM issue_events WHERE at < ?1 AND issue NOT IN (
           SELECT issue FROM issue_events WHERE event = 'started'
           EXCEPT SELECT issue FROM issue_events WHERE event = 'ended')",
        [cutoff],
    )?;
    tx.execute("DELETE FROM samples WHERE at < ?1", [cutoff])?;
    tx.commit()
}

/// Writes a consistent copy to `to`, mode 0600. `to` must not exist.
pub fn export(conn: &Connection, to: &Path) -> Result<(), String> {
    // SQLite's VACUUM INTO takes an empty file, so it is made here first, with
    // its mode, and never replaces anything.
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(to)
        .map_err(|e| format!("cannot create {}: {e}", to.display()))?;
    let name = to.to_str().ok_or("the export path is not UTF-8")?;
    conn.execute("VACUUM INTO ?1", [name]).map_err(|e| {
        let _ = fs::remove_file(to);
        format!("the export failed: {e}")
    })?;
    fs::set_permissions(to, fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("cannot set the mode of {}: {e}", to.display()))
}
