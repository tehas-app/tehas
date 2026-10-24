//! The database file: where it is, opening it, and the schema it must hold.
//!
//! ADR 0003 is the reason this is not the analytics program's `open`: the
//! store never moves a damaged file aside and carries on. Its rows are the
//! host's only record of an approval, of a park question id and of an
//! unsettled journaled action, and quarantining any of that erases the record
//! the store exists to keep. Every failure below stops with 78 and one word on
//! stderr and leaves the file exactly where a person can read it.

use rusqlite::{params, Connection, ErrorCode, OpenFlags};
use std::cell::Cell;
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// PRAGMA user_version of the schema in schema.sql.
pub const SCHEMA_VERSION: i64 = 1;
pub const SCHEMA: &str = include_str!("schema.sql");

/// How long SQLite waits for a lock another connection holds before giving
/// up, as the spec's section 4 states it. A database still busy then is exit
/// 1 with the word `busy`.
pub const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// How long a reader waits. In WAL mode a writer's lock never stops a reader;
/// what does is SQLite's own brief exclusive steps, the recovery of the -shm
/// and the checkpoint of the last connection to close, which are over in
/// milliseconds. Without any wait, tests/concurrency.rs saw a `check` answer
/// `busy` during a recovery.
pub const READ_WAIT: Duration = Duration::from_secs(1);

/// How often a waiting connection asks again. SQLite's own busy timeout backs
/// off to 100 ms between tries, so among writers that keep the lock busy the
/// one that has waited longest asks least often, and it can lose every turn
/// until its five seconds are gone: tests/concurrency.rs saw that with eight
/// writers. Asking every millisecond gives each waiter the same chance.
const BUSY_POLL: Duration = Duration::from_millis(1);

/// A busy handler: true to try again. `tries` is 0 at the start of each wait,
/// which is when the clock starts. A process of the store waits on one
/// connection at a time, so the start is kept per thread.
pub(crate) fn busy_wait(tries: i32, limit: Duration) -> bool {
    thread_local! {
        static SINCE: Cell<Option<Instant>> = const { Cell::new(None) };
    }
    let now = Instant::now();
    if tries == 0 {
        SINCE.set(Some(now));
    }
    if now.duration_since(SINCE.get().unwrap_or(now)) >= limit {
        return false;
    }
    std::thread::sleep(BUSY_POLL);
    true
}

fn write_wait(tries: i32) -> bool {
    busy_wait(tries, BUSY_TIMEOUT)
}

fn read_wait(tries: i32) -> bool {
    busy_wait(tries, READ_WAIT)
}

/// The indexes of schema v1 by name, so the test that pins the schema can name
/// them. Every CREATE INDEX of schema.sql appears here exactly once, which that
/// test is what checks.
#[cfg(test)]
pub const COUNTED_INDEXES: [&str; 11] = [
    "issues_checked",
    "cycles_running",
    "heartbeats_at",
    "endings_recent",
    "approvals_open",
    "questions_issue",
    "cycle_history_issue",
    "plan_history_issue",
    "polls_recent",
    "actions_open",
    "journal_action",
];

/// The tables `check` counts. Every table of schema.sql appears here exactly
/// once, which the test every_counted_table_exists pins, so a table that goes
/// missing from the schema cannot go missing from the report too.
pub const COUNTED_TABLES: [&str; 39] = [
    "meta",
    "issues",
    "cycles",
    "cycle_locks",
    "heartbeats",
    "endings",
    "approvals",
    "planning",
    "planning_answers",
    "planning_filed",
    "cycle_settings",
    "cycle_notes",
    "cycle_results",
    "cycle_progress",
    "parks",
    "questions",
    "cycle_history",
    "plan_history",
    "refined",
    "polls",
    "poll_demand",
    "queue_snapshot",
    "health_samples",
    "energy_samples",
    "platform_state",
    "pause_flag",
    "limits",
    "branch_checks",
    "check_product",
    "refresh_requests",
    "refresh_hold",
    "endpoints",
    "index_state",
    "release_history",
    "edge_takeover",
    "improve_cap",
    "sweep_log",
    "actions",
    "journal",
];

/// The four classes the store tells apart, each with its own word for stderr
/// (spec section 4). All four are exit 78: the store cannot answer and nothing
/// was written. A caller reads 78 with any of them as "could not look", never
/// as "no rows".
#[derive(Debug, PartialEq, Eq)]
pub enum DbError {
    /// No file at all.
    Missing,
    /// Not a database, malformed, a failed quick_check, a truncated file, or
    /// a non-file at state/tehas.db.
    Corrupt(String),
    /// There, but it cannot be opened: no permission, or an unreadable or
    /// torn -wal/-shm pair.
    Unreadable(String),
    /// user_version newer than this binary knows.
    Future(i64),
    /// The lock was held past the busy timeout. Exit 1 with the word `busy`.
    Busy,
}

impl DbError {
    /// The one word the caller reads on standard error.
    pub fn word(&self) -> &'static str {
        match self {
            DbError::Missing => "missing",
            DbError::Corrupt(_) => "corrupt",
            DbError::Unreadable(_) => "unreadable",
            DbError::Future(_) => "future",
            DbError::Busy => "busy",
        }
    }

    /// What went wrong, for the line on standard error. The word comes first
    /// so a caller can grep for it.
    pub fn detail(&self) -> Option<String> {
        match self {
            DbError::Corrupt(s) | DbError::Unreadable(s) => Some(s.clone()),
            DbError::Future(v) => Some(format!("the database has schema {v}")),
            DbError::Missing | DbError::Busy => None,
        }
    }
}

/// The path of the store: `$TEHAS_HOME/state/tehas.db`. Never under `live/`
/// and never under `state/index`, which `guard` refuses.
pub fn path(home: &Path) -> PathBuf {
    home.join("state").join("tehas.db")
}

/// What the caller said about the installation, for the rows of `meta` that a
/// migration from schema 0 writes. Each value arrives from the command that
/// loaded the instance's settings, which is the shape the verb states: the rules
/// of a valid setting stay in lib/config.sh, and `write_schema` refuses a value
/// that came empty rather than writing a row that says nothing.
#[derive(Debug, PartialEq, Eq)]
pub struct Settings {
    pub version: String,
    pub commit: String,
    pub instance: String,
    pub repo: String,
}

/// The checks that run before anything opens the file, so a bad path stops the
/// command before SQLite is given it: the file and its directory must be real,
/// not links, and the path must not sit under a tree a container writes.
fn guard_result(file: &Path) -> Result<(), DbError> {
    // A directory of the path, not a name that happens to contain the word:
    // state/live-is-not is not the served live/ of the host, and the rule is
    // about that directory.
    let sits_under = |bad: &str| {
        file.parent().is_some_and(|p| {
            p.components()
                .any(|c| c.as_os_str() == std::ffi::OsStr::new(bad))
        })
    };
    if sits_under("live") {
        return Err(DbError::Unreadable(
            "the path is under live/, which Caddy serves".to_string(),
        ));
    }
    if sits_under("index") {
        return Err(DbError::Unreadable(
            "the path is under state/index, which the indexing container writes".to_string(),
        ));
    }
    // The parent must be a real directory: state/ holds the file, and a link
    // there moves the store outside the home.
    if let Some(dir) = file.parent() {
        match fs::symlink_metadata(dir) {
            Ok(m) if m.is_symlink() => {
                return Err(DbError::Unreadable(format!("{} is a link", dir.display())))
            }
            Ok(m) if !m.is_dir() => {
                return Err(DbError::Unreadable(format!(
                    "{} is not a directory",
                    dir.display()
                )))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(DbError::Unreadable(format!("{}: {e}", dir.display()))),
            Ok(_) => {}
        }
    }
    match fs::symlink_metadata(file) {
        Ok(m) if m.is_symlink() => Err(DbError::Corrupt(format!("{} is a link", file.display()))),
        Ok(m) if !m.is_file() => Err(DbError::Unreadable(format!(
            "{} is not a file",
            file.display()
        ))),
        _ => Ok(()),
    }
}

/// Whether the bytes at the start of the file are SQLite's own header. SQLite
/// calls a file whose first bytes are not these `file is not a database`; this
/// is the same test made without a connection, and the header of a real
/// database of any schema version, which is what `init` asks of a file it was
/// told to replace.
pub fn is_database(file: &Path) -> bool {
    const HEADER: &[u8] = b"SQLite format 3\0";
    let Ok(mut f) = File::open(file) else {
        return false;
    };
    let mut buf = [0u8; 16];
    f.read_exact(&mut buf).is_ok() && buf == HEADER
}

/// The readers of the file's own shape that a test of a damaged store asks
/// for: the page of a table, the byte offset of a page, and what a page says
/// about itself. They read the file rather than run a query because the point
/// of the test is what the bytes say. No verb asks for them, so they are built
/// only for the tests.
#[cfg(test)]
pub mod pages {
    use super::*;
    // The file's own bytes are what these read, so they seek rather than ask
    // SQLite.
    use std::io::{Seek, SeekFrom};

    /// The page of a table of the store, counted the way SQLite counts them: one
    /// greater than the root page the schema holds for it. The page of a table with
    /// no rows holds only its own header, which is why changing a byte of it
    /// changes no answer; the page of a table with rows holds them, so it is the
    /// page a test of the integrity check damages.
    pub fn page_of(conn: &Connection, table: &str) -> Option<u64> {
        // The name comes from COUNTED_TABLES, never from a caller's word. COUNTED_*
        // and sqlite_master do not always agree -- a test of one cannot be allowed
        // to make the other answer for it -- so SQLite's own list is taken here
        // and compared with the caller's.
        if !COUNTED_TABLES.contains(&table) {
            return None;
        }
        let root: i64 = conn
            .query_row(
                "SELECT rootpage FROM sqlite_schema WHERE type = 'table' AND name = ?1",
                [table],
                |r| r.get(0),
            )
            .ok()?;
        (root > 0).then(|| root as u64 + 1)
    }

    /// The file handle of one page of the database, the way SQLite numbers pages:
    /// `(n - 1) * page_size` bytes into the file.
    pub fn page_offset(file: &Path, page: u64) -> Option<u64> {
        let size = page_size(file)?;
        let offset = (page - 1) * size;
        (offset < fs::metadata(file).ok()?.len()).then_some(offset)
    }

    /// The page size of the file, for the caller that counts bytes of a page from
    /// its end.
    pub fn file_page_size(file: &Path) -> Option<u64> {
        page_size(file)
    }

    /// How many cells a page of the file holds, read from the two bytes at offset
    /// 3 of it. The cell pointer array of that length sits at the end of the page,
    /// which is the structure to aim at when a row and its index entries are to
    /// stop agreeing with each other.
    pub fn cell_count(file: &Path, page: u64) -> Option<u16> {
        let offset = page_offset(file, page)?;
        let mut f = File::open(file).ok()?;
        f.seek(SeekFrom::Start(offset + 3)).ok()?;
        let mut buf = [0u8; 2];
        f.read_exact(&mut buf).ok()?;
        Some(u16::from_be_bytes(buf))
    }

    /// The page size the file was made with, read from its header rather than
    /// assumed: SQLite takes it from the CREATE DATABASE of the writer, so a
    /// database made by another build of SQLite need not hold 4096.
    fn page_size(file: &Path) -> Option<u64> {
        if !is_database(file) {
            return None;
        }
        let mut f = File::open(file).ok()?;
        let mut buf = [0u8; 2];
        f.seek(SeekFrom::Start(16)).ok()?;
        f.read_exact(&mut buf).ok()?;
        let size = u16::from_be_bytes(buf);
        // SQLite stores 1 for 65536, which no page size field of two bytes can hold.
        Some(u64::from(if size == 1 { u16::MAX } else { size }))
    }
}

/// The two kinds of call section 4 states: a verb that writes waits up to the
/// busy timeout for a lock another connection holds, and one that only reads
/// waits at most READ_WAIT, for SQLite's own brief steps. The wait is
/// per-connection, so a reader cannot sit on a poll's lock for five seconds:
/// that is the difference between `busy` soon and `busy` after a wait a status
/// page cannot afford.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    Read,
    Write,
}

/// Opens the store for reading and writing: the path is guarded, the file must
/// already be there, and every connection carries the pragmas of section 3.
/// A file that exists but cannot be opened is `unreadable`, and the store does
/// not move it (ADR 0003).
pub fn open(file: &Path, purpose: Purpose) -> Result<Connection, DbError> {
    guard_result(file)?;
    if !file.exists() {
        return Err(DbError::Missing);
    }
    // WAL needs the -shm beside the file, which it makes by writing in the
    // directory: a directory nobody may write gives up before the busy timeout
    // has anything to wait for.
    let dir_mode = file
        .parent()
        .and_then(|d| fs::metadata(d).ok())
        .map(|m| m.permissions().mode());
    let conn = Connection::open_with_flags(
        file,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| match dir_mode {
        // Readable, but nobody may write: not contention, and not the file's
        // fault either.
        Some(m) if m & 0o200 == 0 => DbError::Unreadable(format!(
            "{}: the directory of the store is not writable: {e}",
            file.display()
        )),
        _ => classify(e),
    })?;
    prepare(&conn, purpose)?;
    Ok(conn)
}

/// The write lock, taken and handed back: a verb that writes wants to meet a
/// lock before it has read anything and decided, and SQLite will not answer
/// SQLITE_BUSY for a read-only open. The lock is given back before the
/// connection is returned, so the caller holds the store's attention and not
/// its write lock: SQLite will not copy or vacuum a database that one is open
/// on, so a verb that locks, reads, copies and then writes has to be between
/// the two statements rather than inside a transaction. `lock` is what a verb
/// that only writes takes, and holds.
#[cfg(test)]
pub fn open_write(file: &Path) -> Result<Connection, DbError> {
    let conn = lock(file)?;
    settle(&conn)?;
    Ok(conn)
}

/// `open` for a verb that writes, with the write lock left held. A verb that
/// takes the lock and never ends it holds a write lock with no transaction,
/// which the next caller's busy timeout then pays for: `settle` below, or the
/// commit of a step, is the statement that ends it.
pub fn lock(file: &Path) -> Result<Connection, DbError> {
    let conn = open(file, Purpose::Write)?;
    conn.execute("BEGIN IMMEDIATE", []).map_err(classify)?;
    Ok(conn)
}

/// The statement that gives the write lock back, for the verb that looked at
/// the store and has nothing to write: nothing was changed, so there is nothing
/// to commit, and SQLite asks for the word all the same.
pub fn settle(conn: &Connection) -> Result<(), DbError> {
    conn.execute("COMMIT", []).map(|_| ()).map_err(classify)
}

/// The same checks and the same connection as `open`, for the reader that must
/// not disturb a writer's lock at all: read-only, so it waits at most READ_WAIT
/// and writes no row. It can still make the -wal and -shm beside the file when
/// they are absent, at the file's mode. `check` is the verb that reads, and
/// section 4 says so.
pub fn open_read_only(file: &Path) -> Result<Connection, DbError> {
    guard_result(file)?;
    if !file.exists() {
        return Err(DbError::Missing);
    }
    read_only(file, Purpose::Read)
}

/// The data verbs that only read open as `check` does. READ_WAIT covers the
/// moment the last connection to close locks the file to checkpoint.
pub fn open_look(file: &Path) -> Result<Connection, DbError> {
    open_read_only(file)
}

fn read_only(file: &Path, purpose: Purpose) -> Result<Connection, DbError> {
    let conn = Connection::open_with_flags(
        file,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(classify)?;
    prepare(&conn, purpose)?;
    Ok(conn)
}

/// Opens the store with nothing in it yet: the path is guarded, the file is
/// made mode 0600 and empty, and the caller decides what to write into it.
/// Unlike `open` this creates the file, so `init` uses it and nothing else.
pub fn create(file: &Path) -> Result<Connection, DbError> {
    guard_result(file)?;
    if file.exists() {
        // The mode is put on the file before SQLite is given it, as `open`
        // would have: a file that came from elsewhere with 0644 on it is the
        // store's file now. The verb that reaches here with a file that is
        // already there is `init --force` over an empty one and a migration of
        // one the host made with `touch`: either is filled with the store's
        // rows, so neither keeps the mode it arrived with.
        set_mode(file, 0o600)?;
    } else if let Some(dir) = file.parent() {
        // state/ is made 0700 and kept that way: with WAL on, a readable
        // state/ is a readable copy of the store whatever the file's own mode
        // says, and today ordinary mkdir leaves it at the umask.
        fs::create_dir_all(dir)
            .map_err(|e| DbError::Unreadable(format!("cannot make {}: {e}", dir.display())))?;
        settle_state_dir(dir)?;
        // Made mode 0600 before SQLite sees it, so the -wal and -shm siblings
        // it makes beside it take the same mode. O_EXCL: `init` has already
        // refused a file that exists, so one that appears here is another
        // process's and is not overwritten.
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(file)
            .map(|_| ())
            .or_else(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    Ok(())
                } else {
                    Err(DbError::Unreadable(format!(
                        "cannot make {}: {e}",
                        file.display()
                    )))
                }
            })?;
    }
    let conn = Connection::open_with_flags(
        file,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(classify)?;
    prepare(&conn, Purpose::Write)?;
    Ok(conn)
}

/// What every connection sets, in the one place: the journal mode, the busy
/// handler, the durability and the foreign keys. The handler asks again every
/// BUSY_POLL, for up to BUSY_TIMEOUT (5 s) on a connection that writes and
/// READ_WAIT (1 s) on one that reads, then the word `busy`. It replaces
/// SQLite's busy_timeout, which then reads 0.
fn prepare(conn: &Connection, purpose: Purpose) -> Result<(), DbError> {
    match purpose {
        Purpose::Write => conn.busy_handler(Some(write_wait)),
        Purpose::Read => conn.busy_handler(Some(read_wait)),
    }
    .map_err(classify)?;
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .map_err(classify)?;
    if !mode.eq_ignore_ascii_case("wal") {
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(classify)?;
    }
    conn.pragma_update(None, "synchronous", "FULL")
        .map_err(classify)?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(classify)?;
    let check: String = conn
        .query_row("PRAGMA quick_check", [], |r| r.get(0))
        .map_err(classify)?;
    if check != "ok" {
        return Err(DbError::Corrupt(format!(
            "the database failed its check: {check}"
        )));
    }
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(classify)?;
    // This binary writes one version and has no migration: anything above it
    // is a database this command must not touch. A file made but not yet
    // written has 0 and is the store's own to fill.
    if version > SCHEMA_VERSION {
        return Err(DbError::Future(version));
    }
    Ok(())
}

/// The mode of the store's own file: 0600, before SQLite is given it. Named for
/// the verbs that write and did not make the file -- `create` sets it on the one
/// it made, and `migrate` sets it on the empty one a host made with `touch`
/// before calling the verb.
pub fn set_file_mode(file: &Path) -> Result<(), DbError> {
    set_mode(file, 0o600)
}

/// The directory of the store, settled to the mode the store's own data needs:
/// 0700, whatever made the directory or what mode it was made with. `create`
/// calls it for the directory it made and every verb that writes calls it for
/// the one it was handed, because a `state/` that `libexec/update` made with
/// `mkdir -p` is at the umask and holds the store, its `-wal` and every backup
/// the store ever wrote.
///
/// A directory that is not there yet is left alone: making it is `create`'s
/// work and the verbs that only read the store must not leave a directory
/// behind, and one that turns out to be something else is what `guard` answers
/// for, not this.
pub fn settle_state_dir(dir: &Path) -> Result<(), DbError> {
    match fs::symlink_metadata(dir) {
        Ok(m) if m.is_dir() => set_mode(dir, 0o700),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(DbError::Unreadable(format!("{}: {e}", dir.display()))),
    }
}

fn set_mode(path: &Path, mode: u32) -> Result<(), DbError> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .map_err(|e| DbError::Unreadable(format!("cannot set the mode of {}: {e}", path.display())))
}

// SQLite names a database's side files by appending to its name.
pub fn side_file(file: &Path, side: &str) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(side);
    PathBuf::from(name)
}

/// SQLite says what is wrong in its own words; the four classes say what the
/// caller acts on. Anything SQLite could not answer for a reason that says
/// nothing about the file's shape is `unreadable`, and the file is left alone.
pub fn classify(e: rusqlite::Error) -> DbError {
    if matches!(
        e.sqlite_error_code(),
        Some(ErrorCode::DatabaseBusy) | Some(ErrorCode::DatabaseLocked)
    ) {
        return DbError::Busy;
    }
    match e.sqlite_error_code() {
        Some(ErrorCode::NotADatabase) | Some(ErrorCode::DatabaseCorrupt) => {
            DbError::Corrupt(format!("{e}"))
        }
        Some(ErrorCode::ReadOnly) | Some(ErrorCode::PermissionDenied) => {
            DbError::Unreadable(format!("{e}"))
        }
        _ => match e {
            rusqlite::Error::SqliteFailure(_, Some(msg))
                if msg.contains("unable to open") || msg.contains("disk I/O") =>
            {
                DbError::Unreadable(msg)
            }
            rusqlite::Error::SqliteFailure(_, msg) => DbError::Unreadable(msg.unwrap_or_default()),
            _ => DbError::Unreadable(format!("{e}")),
        },
    }
}

/// The same four classes for what the operating system says about the path,
/// so a caller that looked at the file itself before opening it answers the
/// way `open` would. A file that is not there is `missing` here too: `init`
/// makes one, `check` does not.
pub fn classify_io(e: &std::io::Error) -> DbError {
    match e.kind() {
        std::io::ErrorKind::NotFound => DbError::Missing,
        std::io::ErrorKind::PermissionDenied => DbError::Unreadable(format!("{e}")),
        _ => DbError::Unreadable(format!("{e}")),
    }
}

/// The guard on its own, for the caller that means to open the file itself and
/// wants the same answer for a link or a directory that `open` gives.
pub fn guard(file: &Path) -> DbError {
    guard_result(file)
        .err()
        .unwrap_or_else(|| DbError::Unreadable(format!("{} passed every check", file.display())))
}

/// Writes schema v1 into the connection and the row of `meta` that says when
/// it was made and by which binary. One transaction: a failure commits nothing
/// and leaves the file as empty as it was. An empty file is not the same thing
/// as a file holding a schema: SQLite gives a file of no bytes at all its own
/// header and its user_version of 0, so both are asked for.
pub fn write_schema(
    conn: &mut Connection,
    now: i64,
    version: &str,
    commit: &str,
    instance: &str,
    repo: &str,
) -> Result<(), DbError> {
    // The two are the caller's words and main.rs has already refused one of
    // another shape; the guard here is that a caller which did not ask main.rs
    // cannot write a row that says nothing about the installation.
    if version.is_empty() || commit.is_empty() || instance.is_empty() || repo.is_empty() {
        return Err(DbError::Unreadable(
            "a value of the installation arrived empty".to_string(),
        ));
    }
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(classify)?;
    let tables: i64 = tx
        .query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get(0))
        .map_err(classify)?;
    let held: i64 = tx
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(classify)?;
    if tables != 0 || held != 0 {
        return Err(DbError::Corrupt(
            "the file already holds a schema".to_string(),
        ));
    }
    tx.execute_batch(SCHEMA).map_err(classify)?;
    let rows = [
        ("schema", SCHEMA_VERSION.to_string()),
        ("created_at", now.to_string()),
        ("tehas_version", version.to_string()),
        ("tehas_commit", commit.to_string()),
        ("instance", instance.to_string()),
        ("repo", repo.to_string()),
    ];
    for (key, value) in rows {
        tx.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)",
            params![key, value],
        )
        .map_err(classify)?;
    }
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)
        .map_err(classify)?;
    tx.commit().map_err(classify)
}

/// The one migration this binary holds: from schema 0, an SQLite file with no
/// tables and a `user_version` of 0 (what SQLite makes of an empty file), to
/// schema 1. `write_schema` is the whole of it: it lays schema.sql, writes the
/// rows of `meta` -- `schema` among them -- and sets `user_version`, all in one
/// transaction, so the version and the `meta` row move together. A database
/// `init` made is already at 1 and has nothing to run. A later part of the epic
/// adds a step for each version above 1.
///
/// `file` is named for the refusal alone, so that the line the caller reads
/// carries the path of the database the step gave up on.
pub fn migrate(
    conn: &mut Connection,
    file: &Path,
    from: i64,
    now: i64,
    settings: &Settings,
) -> Result<(), DbError> {
    match from {
        // Already what this binary writes: the verb reports it and runs
        // nothing, so a migration is safe to call on a database that does not
        // need one -- which is what a person does next, because `init` made
        // this file at 1 and nothing in the world of schema 1 moves it.
        v if v == SCHEMA_VERSION => return Ok(()),
        0 => {}
        v => {
            return Err(DbError::Corrupt(format!(
                "no migration from schema {v} of {}, this binary writes {SCHEMA_VERSION}",
                file.display()
            )))
        }
    }
    // The step is `write_schema`'s own transaction: a failure in it commits
    // nothing and leaves the file as empty as it was. It is asked for outside
    // any transaction of the caller's, which is why `migrate` gives its own lock
    // back before it comes here -- SQLite will not run `VACUUM INTO` from within
    // one, and the two would not nest.
    write_schema(
        conn,
        now,
        &settings.version,
        &settings.commit,
        &settings.instance,
        &settings.repo,
    )
}

/// The name of a backup this verb writes, given the version it came from and
/// the instant it was asked for.
pub fn backup_name(dir: &Path, from: i64, now: i64) -> PathBuf {
    dir.join(format!("tehas-{from}-{now}.db"))
}

/// A consistent copy of the database, mode 0600, written by SQLite itself.
///
/// This is the whole of what `migrate` does before it changes a byte of the
/// store: the copy is made first, and when it cannot be made the verb stops and
/// the database is as it was. The analytics program's `export` is the precedent
/// for the shape: the path is refused before the file is made, the file is made
/// 0600 and never replaces one that is there, and a `VACUUM INTO` that fails
/// partway leaves no half-written copy behind.
///
/// `state` is `state/` of the home the command was pointed at, settled by
/// `backup_dir` below: a directory the store may write a copy of itself into.
///
/// The caller holds the write lock and has ended the transaction it was taken
/// in: SQLite will not copy a database that a transaction is open on, so a verb
/// that locks and copies asks for the lock and gives it back before it asks for
/// the copy. Nothing else is writing while it does, because a verb of the store
/// that changes rows takes the same lock in its own transaction and the busy
/// timeout is what another waits.
pub fn backup(conn: &Connection, state: &Path, from: i64, now: i64) -> Result<PathBuf, DbError> {
    let copy = backup_name(state, from, now);
    let name = copy.to_str().ok_or_else(|| {
        DbError::Unreadable(format!("the backup path {} is not UTF-8", copy.display()))
    })?;
    // Made here, with its mode, because SQLite's VACUUM INTO wants a file that
    // is there and empty, and takes the mode of one it makes itself from the
    // umask. `create_new` is what makes a name that exists an error rather than
    // an overwrite.
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&copy)
        .map(|_| ())
        .map_err(|e| {
            DbError::Unreadable(format!("cannot make the backup {}: {e}", copy.display()))
        })?;
    conn.execute("VACUUM INTO ?1", [name]).map_err(|e| {
        // SQLite may have filled part of the file before it gave up, and a
        // reader cannot tell that copy from a good one. The name is taken away
        // so that the only backup the directory holds is one that finished.
        let _ = fs::remove_file(&copy);
        classify(e)
    })?;
    set_mode(&copy, 0o600)?;
    Ok(copy)
}

/// The directory `migrate --backup` was given, settled to the one place the
/// verb may write: `state/` of the home the command was pointed at, resolved.
///
/// A backup is a copy of the store, so it is readable by nobody but the tehas
/// user, and the mode that buys is the one on the directory: `state/` is 0700
/// and is the store's own directory, made so by `create`. A `<dir>` off the
/// command line is a person's -- it could be a link, it could walk out of the
/// home with a `..`, it could be 0755 and hold a copy of every approval the host
/// ever recorded -- and the store was never given leave to write one there. The
/// resolution comes first so that a link is answered as the directory it is,
/// and the comparison of the two comes before the name: the verb asks once.
pub fn backup_dir(home: &Path, dir: &Path) -> Result<PathBuf, DbError> {
    let state = dir
        .canonicalize()
        .map_err(|e| DbError::Unreadable(format!("the backup directory {}: {e}", dir.display())))?;
    if state != home.join("state") {
        return Err(DbError::Unreadable(format!(
            "the backup goes in the state/ of the store's home, not {}",
            state.display()
        )));
    }
    home_of_state_dir(&state)?;
    Ok(state)
}

/// The home above a directory named `state`: the store's home, checked against
/// what `open` and `create` were told. The name is what is compared, not the
/// whole path, because the caller has already settled which directory it means
/// and this answers whether that one belongs to this store.
fn home_of_state_dir(state: &Path) -> Result<PathBuf, DbError> {
    if state.file_name().and_then(|n| n.to_str()) != Some("state") {
        return Err(DbError::Unreadable(format!(
            "{} is not the state/ of the store's home",
            state.display()
        )));
    }
    state
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| DbError::Unreadable(format!("{} has no home", state.display())))
}

/// What `check` answers: the schema version, the integrity check, and the rows
/// of every counted table.
pub struct Report {
    pub schema: i64,
    pub integrity: String,
    pub rows: Vec<(&'static str, i64)>,
}

/// Reads only, as section 4's `check` does: the version from PRAGMA
/// user_version, PRAGMA integrity_check, and a count of each table. The
/// divergence of section 7 is the import's to report and is #251's, so it is
/// not asked here.
pub fn check(conn: &Connection) -> Result<Report, DbError> {
    let schema: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(classify)?;
    if schema > SCHEMA_VERSION {
        return Err(DbError::Future(schema));
    }
    let mut integrity = Vec::new();
    let mut q = conn.prepare("PRAGMA integrity_check").map_err(classify)?;
    let rows = q
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(classify)?;
    for row in rows {
        integrity.push(row.map_err(classify)?);
    }
    let integrity = match integrity.as_slice() {
        [only] if only == "ok" => "ok".to_string(),
        others => others.join("; "),
    };
    let mut counts = Vec::with_capacity(COUNTED_TABLES.len());
    for table in COUNTED_TABLES {
        // The name is one of COUNTED_TABLES, never a caller's word.
        let n: i64 = conn
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .map_err(classify)?;
        counts.push((table, n));
    }
    Ok(Report {
        schema,
        integrity,
        rows: counts,
    })
}
