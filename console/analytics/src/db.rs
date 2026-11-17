//! The database file: opening it, the schema, events, pruning and the copy.

use rusqlite::{params, Connection, OpenFlags, Transaction, TransactionBehavior};
use std::fs::{self, OpenOptions};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tehas_core::file::side_file;

/// PRAGMA user_version of the schema in schema.sql. v0.30.0 wrote 4 already,
/// so a database of any other schema is moved aside whole under its version.
pub const SCHEMA_VERSION: i64 = 4;
pub const SCHEMA: &str = include_str!("schema.sql");
pub const RETENTION_DAYS: i64 = 365;

/// The columns added within schema 4, after v0.30.0 wrote it: table, column
/// and type. A file a release before them wrote gains each as it is opened,
/// last in its table, where schema.sql has it too.
pub const ADDED: [(&str, &str, &str); 1] = [("runs", "contractor_id", "TEXT")];
const DAY: i64 = 86_400;

/// How long to keep trying the journal switch, and how often, when another
/// opener holds the lock it needs.
const WAL_WAIT: Duration = Duration::from_secs(5);
// Past the busy timeout, or a second try never starts.
const OPEN_WAIT: Duration = Duration::from_secs(10);
const WAL_RETRY: Duration = Duration::from_millis(50);

pub const EVENTS: [&str; 7] = [
    "ready", "started", "ended", "awaiting", "merged", "closed", "stopped",
];

pub fn path(home: &Path) -> PathBuf {
    home.join("state").join("analytics.db")
}

/// Opens the database, making it when there is none. A file that is not a
/// database is moved to analytics.db.broken and a new one is made: the data
/// is at most RETENTION_DAYS deep.
/// One of any other schema version is moved, with its -wal and -shm, to
/// analytics.db.schema-<version>, where a reader can still fetch it, and
/// prune_aside deletes it RETENTION_DAYS after its last write. Any other
/// failure, a full disk, a file it may not write, a lock still held after
/// OPEN_WAIT, is an error and leaves the file as it is. The second value is where a file was
/// moved, if one was.
pub fn open(file: &Path, now: i64) -> Result<(Connection, Option<PathBuf>), String> {
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("cannot make {}: {e}", dir.display()))?;
    }
    let until = Instant::now() + OPEN_WAIT;
    let opened = loop {
        match try_open(file, now) {
            Err(Unusable::Busy(_)) if Instant::now() < until => std::thread::sleep(WAL_RETRY),
            opened => break opened,
        }
    };
    let (to, why, keep_sides) = match opened {
        Ok(conn) => return Ok((conn, None)),
        Err(Unusable::Failed(e) | Unusable::Busy(e)) => return Err(e),
        Err(Unusable::Broken(why)) => (file.with_extension("db.broken"), why, false),
        Err(Unusable::Foreign(version, why)) => (
            file.with_extension(format!("db.schema-{version}")),
            why,
            true,
        ),
    };
    fs::rename(file, &to).map_err(|e| format!("{why}, and it cannot be moved aside: {e}"))?;
    for side in ["-wal", "-shm"] {
        let from = side_file(file, side);
        if keep_sides && from.exists() {
            let _ = fs::rename(&from, side_file(&to, side));
        } else {
            let _ = fs::remove_file(&from);
        }
    }
    let conn = try_open(file, now).map_err(|e| match e {
        Unusable::Failed(e) | Unusable::Busy(e) | Unusable::Broken(e) | Unusable::Foreign(_, e) => {
            e
        }
    })?;
    Ok((conn, Some(to)))
}

/// Deletes the databases of other schema versions, and their side files,
/// whose last write is more than 365 days old: by then every row in them is.
/// Returns the expired files it could not delete.
pub fn prune_aside(file: &Path, now: i64) -> Vec<PathBuf> {
    let mut stuck = Vec::new();
    let (Some(dir), Some(name)) = (file.parent(), file.file_name().and_then(|n| n.to_str())) else {
        return stuck;
    };
    let prefix = format!("{name}.schema-");
    let Ok(entries) = fs::read_dir(dir) else {
        return stuck;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let ours = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with(&prefix));
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        let written = meta
            .modified()
            .ok()
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);
        if ours && meta.is_file() && written.is_some_and(|w| w < now - RETENTION_DAYS * DAY) {
            match fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => stuck.push(path),
                _ => {}
            }
        }
    }
    stuck
}

pub(crate) enum Unusable {
    // The file may be a good database: it is left alone.
    Failed(String),
    // Another program holds the file for now: try again, then as Failed.
    Busy(String),
    // The file is not a database of this schema.
    Broken(String),
    // A database of another schema version: kept aside, not discarded.
    Foreign(i64, String),
}

fn classify(e: rusqlite::Error) -> Unusable {
    match e.sqlite_error_code() {
        Some(rusqlite::ErrorCode::NotADatabase) | Some(rusqlite::ErrorCode::DatabaseCorrupt) => {
            Unusable::Broken(format!("the database is not usable: {e}"))
        }
        _ if is_busy(&e) => Unusable::Busy(format!("the database could not be opened: {e}")),
        _ => Unusable::Failed(format!("the database could not be opened: {e}")),
    }
}

fn try_open(file: &Path, now: i64) -> Result<Connection, Unusable> {
    // Made 0600 before SQLite sees it; its -wal and -shm take the same mode.
    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(file)
        .map_err(|e| Unusable::Failed(format!("cannot open {}: {e}", file.display())))?;
    let _ = fs::set_permissions(file, fs::Permissions::from_mode(0o600));
    let mut conn = Connection::open_with_flags(
        file,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(classify)?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(classify)?;
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
    settle(&mut conn, version, now)?;
    add_missing(&mut conn)?;
    // Only now: a file that is moved aside is moved as it was found.
    set_wal(&mut conn)?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(classify)?;
    Ok(conn)
}

/// Puts the database in WAL mode. Reading it first matters when several
/// programs open the same new file at once: the switch needs an exclusive
/// lock and SQLite can answer SQLITE_BUSY or SQLITE_LOCKED at once there
/// instead of waiting out the busy timeout, so a switch that is refused is
/// tried again until the one that holds the lock has made it.
fn set_wal(conn: &mut Connection) -> Result<(), Unusable> {
    let mut deadline = None;
    loop {
        let mode: String = conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .map_err(classify)?;
        if mode.eq_ignore_ascii_case("wal") {
            return Ok(());
        }
        match conn.pragma_update(None, "journal_mode", "WAL") {
            Ok(()) => return Ok(()),
            Err(e) if is_busy(&e) => {
                let now = Instant::now();
                let until = *deadline.get_or_insert(now + WAL_WAIT);
                if now >= until {
                    return Err(classify(e));
                }
                std::thread::sleep(WAL_RETRY);
            }
            Err(e) => return Err(classify(e)),
        }
    }
}

fn is_busy(e: &rusqlite::Error) -> bool {
    matches!(
        e.sqlite_error_code(),
        Some(rusqlite::ErrorCode::DatabaseBusy) | Some(rusqlite::ErrorCode::DatabaseLocked)
    )
}

/// Makes the schema of a database whose user_version read `seen`. The version
/// is read again under a write lock: another program may have done it since,
/// and its database is not a broken one.
pub(crate) fn settle(conn: &mut Connection, seen: i64, now: i64) -> Result<(), Unusable> {
    if seen == SCHEMA_VERSION {
        return Ok(());
    }
    if seen != 0 {
        return Err(foreign(seen));
    }
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(classify)?;
    let version: i64 = tx
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(classify)?;
    match version {
        SCHEMA_VERSION => return Ok(()),
        0 => {
            let tables: i64 = tx
                .query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get(0))
                .map_err(classify)?;
            if tables != 0 {
                return Err(Unusable::Broken(
                    "the database has tables but no schema version".to_string(),
                ));
            }
            create(&tx, now).map_err(classify)?;
        }
        other => return Err(foreign(other)),
    }
    tx.commit().map_err(classify)
}

/// The columns of [`ADDED`] the file does not hold yet. Read first without a
/// lock, so an open of a file that has them all writes nothing, and again
/// under the write lock, where another opener may have added them since.
fn add_missing(conn: &mut Connection) -> Result<(), Unusable> {
    if lacking(conn).map_err(classify)?.is_empty() {
        return Ok(());
    }
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(classify)?;
    for (table, column, kind) in lacking(&tx).map_err(classify)? {
        tx.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {kind}"))
            .map_err(classify)?;
    }
    tx.commit().map_err(classify)
}

fn lacking(conn: &Connection) -> rusqlite::Result<Vec<(&'static str, &'static str, &'static str)>> {
    let mut out = Vec::new();
    for (table, column, kind) in ADDED {
        let there: i64 = conn.query_row(
            "SELECT count(*) FROM pragma_table_info(?1) WHERE name = ?2",
            params![table, column],
            |r| r.get(0),
        )?;
        if there == 0 {
            out.push((table, column, kind));
        }
    }
    Ok(out)
}

// A schema this program does not write is kept aside.
fn foreign(version: i64) -> Unusable {
    Unusable::Foreign(
        version,
        format!("the database has schema {version}, this program writes {SCHEMA_VERSION}"),
    )
}

fn create(tx: &Transaction, now: i64) -> rusqlite::Result<()> {
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
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)
}

/// Records an issue's event, the first time only. Returns whether it was new.
pub fn event(conn: &Connection, issue: i64, event: &str, at: i64) -> rusqlite::Result<bool> {
    let n = conn.execute(
        "INSERT OR IGNORE INTO issue_events (at, issue, event) VALUES (?1, ?2, ?3)",
        params![at, issue, event],
    )?;
    Ok(n == 1)
}

/// Keeps when an image was built, as `image:<id>` in meta rather than a
/// column of runs. An id names one build: the first
/// time recorded stands, and every run of the image reads it, also one
/// recorded before it was known.
pub fn image_built(
    conn: &Connection,
    image: Option<&str>,
    built: Option<i64>,
) -> rusqlite::Result<()> {
    if let (Some(image), Some(built)) = (image, built) {
        conn.execute(
            "INSERT OR IGNORE INTO meta (key, value) VALUES ('image:' || ?1, ?2)",
            params![image, built.to_string()],
        )?;
    }
    Ok(())
}

/// Deletes what is older than 365 days. A run is as old as its end. The
/// events of an issue that started and has not ended stay past the cutoff
/// while the start is less than a day older than it: no cycle runs a day, so
/// an issue whose end was never recorded does not keep its events for ever.
pub fn prune(conn: &Connection, now: i64) -> rusqlite::Result<()> {
    let cutoff = now - RETENTION_DAYS * DAY;
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM runs WHERE coalesce(ended, started) < ?1",
        [cutoff],
    )?;
    tx.execute(
        "DELETE FROM issue_events WHERE at < ?1 AND issue NOT IN (
           SELECT issue FROM issue_events WHERE event = 'started' AND at >= ?2
           EXCEPT SELECT issue FROM issue_events WHERE event = 'ended')",
        [cutoff, cutoff - DAY],
    )?;
    tx.execute("DELETE FROM samples WHERE at < ?1", [cutoff])?;
    tx.execute(
        "DELETE FROM meta WHERE key LIKE 'image:%'
           AND substr(key, 7) NOT IN (SELECT image FROM runs WHERE image IS NOT NULL)",
        [],
    )?;
    // A database made when retention was shorter says what it is now.
    tx.execute(
        "UPDATE meta SET value = ?1 WHERE key = 'retention_days' AND value <> ?1",
        [RETENTION_DAYS.to_string()],
    )?;
    tx.commit()
}

/// Writes a consistent copy to `to`, mode 0600. `to` must not exist.
pub fn export(conn: &Connection, to: &Path) -> Result<(), String> {
    // Before the file is made: a path SQLite cannot be given leaves none.
    let name = to.to_str().ok_or("the export path is not UTF-8")?;
    // SQLite's VACUUM INTO takes an empty file, so it is made here first, with
    // its mode, and never replaces anything.
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(to)
        .map_err(|e| format!("cannot create {}: {e}", to.display()))?;
    conn.execute("VACUUM INTO ?1", [name]).map_err(|e| {
        let _ = fs::remove_file(to);
        format!("the export failed: {e}")
    })?;
    fs::set_permissions(to, fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("cannot set the mode of {}: {e}", to.display()))
}

/// What an export holds, read from the copy itself, as the one line
/// `analytics copy --json` prints: {"api":1,"schema":N,"from":T,"to":T,
/// "runs":N}. from is the oldest start of a run and to the newest end (a
/// start where a run has none); both are left out when there is no run.
pub fn copy_summary(copy: &Path) -> Result<String, String> {
    let conn = Connection::open_with_flags(copy, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("cannot open the copy {}: {e}", copy.display()))?;
    let read = |e: rusqlite::Error| format!("cannot read the copy {}: {e}", copy.display());
    let schema: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(read)?;
    let (runs, from, to): (i64, Option<i64>, Option<i64>) = conn
        .query_row(
            "SELECT count(*), min(started), max(coalesce(ended, started)) FROM runs",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(read)?;
    let mut line = format!("{{\"api\":1,\"schema\":{schema}");
    if let (Some(from), Some(to)) = (from, to) {
        line.push_str(&format!(",\"from\":{from},\"to\":{to}"));
    }
    line.push_str(&format!(",\"runs\":{runs}}}"));
    Ok(line)
}
