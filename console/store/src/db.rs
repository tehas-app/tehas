//! The database file: where it is, opening it, and the schema it must hold.
//!
//! ADR 0003 is the reason this is not the analytics program's `open`: the
//! store never moves a damaged file aside and carries on. Its rows are the
//! host's only record of an approval, of a park question id and of an
//! unsettled journaled action, and quarantining any of that erases the record
//! the store exists to keep. Every failure below stops with 78 and one word on
//! stderr and leaves the file exactly where a person can read it.

use rusqlite::{params, Connection, ErrorCode, OpenFlags, OptionalExtension};
use std::cell::Cell;
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::live;

/// PRAGMA user_version of the schema in schema.sql.
pub const SCHEMA_VERSION: i64 = 8;
pub const SCHEMA: &str = include_str!("schema.sql");

/// The steps from schema 1 to 2 and from 2 to 3: columns and a table added,
/// nothing changed or removed, so no row of an older store can be lost. Each
/// column comes last in its table in schema.sql too, so init and migrate make
/// the same schema.
const STEP_2: &str = "
ALTER TABLE approvals ADD COLUMN input_json TEXT;
ALTER TABLE parks ADD COLUMN told_at INTEGER;
ALTER TABLE planning_answers ADD COLUMN text TEXT;
CREATE TABLE planning_notes (
  issue INTEGER NOT NULL REFERENCES planning (issue) ON DELETE CASCADE,
  kind  TEXT NOT NULL CHECK (kind IN ('fixer-refused','held')),
  comment_id INTEGER NOT NULL,
  at INTEGER NOT NULL,
  PRIMARY KEY (issue, kind, comment_id)
);
";
const STEP_3: &str = "
ALTER TABLE heartbeats ADD COLUMN text TEXT;
";

/// The step from 3 to 4: `cycles.how` takes 'millwright' and `planning_notes.kind`
/// takes 'millwright-refused', each beside the old word, and
/// `planning_answers.fixer` is named `millwright`. SQLite cannot change a CHECK
/// in place, so the two tables are rebuilt as schema.sql writes them and every
/// row is copied. See `step_4` for why foreign keys are off while it runs.
///
/// #686: these two CHECK lists hold both spellings, and are the schema's part of
/// the transition: part 5 drops the old word from them, with `schema.sql` and
/// the aliases `Args::word` takes. The migration keeps the rows a host has.
pub(crate) const STEP_4: &str = "
CREATE TABLE cycles_new (
  issue          INTEGER PRIMARY KEY REFERENCES issues (issue) ON DELETE CASCADE,
  attempt        INTEGER NOT NULL DEFAULT 1,
  started_at     INTEGER NOT NULL,
  started_by     TEXT,
  how            TEXT NOT NULL CHECK (how IN ('label','refresh','retry-tick','outage-retry','fixer','millwright','hand')),
  requeued       INTEGER NOT NULL DEFAULT 0,
  rukas_commit   TEXT,
  image          TEXT,
  branch         TEXT,
  end_reason     TEXT CHECK (end_reason IN ('closed','merged','handoff','parked','failed','stopped','timeout','killed','lost','interrupted','stalled','unknown')),
  ended_at       INTEGER,
  oom_kills      INTEGER,
  forks_refused  INTEGER,
  stalled_seconds INTEGER,
  timed_out      INTEGER NOT NULL DEFAULT 0,
  runner_exit    INTEGER,
  watched        INTEGER NOT NULL DEFAULT 0,
  host_saw       TEXT CHECK (host_saw IN ('killed')),
  stop_asked_at  INTEGER,
  stop_said_times INTEGER NOT NULL DEFAULT 0
);
INSERT INTO cycles_new SELECT * FROM cycles;
DROP TABLE cycles;
ALTER TABLE cycles_new RENAME TO cycles;
CREATE INDEX cycles_running ON cycles (ended_at, started_at);
CREATE TABLE planning_notes_new (
  issue INTEGER NOT NULL REFERENCES planning (issue) ON DELETE CASCADE,
  kind  TEXT NOT NULL CHECK (kind IN ('fixer-refused','millwright-refused','held')),
  comment_id INTEGER NOT NULL,
  at INTEGER NOT NULL,
  PRIMARY KEY (issue, kind, comment_id)
);
INSERT INTO planning_notes_new SELECT * FROM planning_notes;
DROP TABLE planning_notes;
ALTER TABLE planning_notes_new RENAME TO planning_notes;
ALTER TABLE planning_answers RENAME COLUMN fixer TO millwright;
";

/// The step from 4 to 5: `auto` added to `planning` and `plan_history`, last in
/// each as in schema.sql. Every row is kept, as a refine a person asked for.
const STEP_5: &str = "
ALTER TABLE planning ADD COLUMN auto INTEGER NOT NULL DEFAULT 0;
ALTER TABLE plan_history ADD COLUMN auto INTEGER NOT NULL DEFAULT 0;
";

/// The step from 5 to 6: `seq` added to `cycle_history` and `plan_history`,
/// last in each as in schema.sql, with its indexes and the triggers that fill
/// it. The rows there are numbered in the order history-show listed them before,
/// the end, then the issue, then the row, a cycle before a planning turn.
pub const STEP_6: &str = "
ALTER TABLE cycle_history ADD COLUMN seq INTEGER;
ALTER TABLE plan_history ADD COLUMN seq INTEGER;
CREATE TEMP TABLE history_seq AS
  SELECT kind, id, row_number() OVER (ORDER BY at, issue, id, kind) AS seq
  FROM (SELECT 0 AS kind, id, at, issue FROM cycle_history
        UNION ALL SELECT 1, id, at, issue FROM plan_history);
UPDATE cycle_history SET seq = (SELECT seq FROM history_seq h WHERE h.kind = 0 AND h.id = cycle_history.id);
UPDATE plan_history SET seq = (SELECT seq FROM history_seq h WHERE h.kind = 1 AND h.id = plan_history.id);
DROP TABLE history_seq;
CREATE UNIQUE INDEX cycle_history_seq ON cycle_history (seq);
CREATE UNIQUE INDEX plan_history_seq ON plan_history (seq);
CREATE TRIGGER cycle_history_order AFTER INSERT ON cycle_history WHEN NEW.seq IS NULL BEGIN
  UPDATE cycle_history SET seq = 1 + max(coalesce((SELECT max(seq) FROM cycle_history), 0),
    coalesce((SELECT max(seq) FROM plan_history), 0)) WHERE id = NEW.id;
END;
CREATE TRIGGER plan_history_order AFTER INSERT ON plan_history WHEN NEW.seq IS NULL BEGIN
  UPDATE plan_history SET seq = 1 + max(coalesce((SELECT max(seq) FROM cycle_history), 0),
    coalesce((SELECT max(seq) FROM plan_history), 0)) WHERE id = NEW.id;
END;
";

/// The step from 6 to 7: the table of what each poll cost.
pub const STEP_7: &str = "
CREATE TABLE poll_cost (
  id INTEGER PRIMARY KEY,
  at INTEGER NOT NULL,
  wall_ms INTEGER NOT NULL, cpu_ms INTEGER NOT NULL, gh INTEGER NOT NULL,
  writes INTEGER, label INTEGER, comment INTEGER, edit INTEGER, other INTEGER
);
CREATE INDEX poll_cost_at ON poll_cost (at);
";

/// The step from 7 to 8: whether a poll started a cycle, last in `poll_cost` as
/// in schema.sql. A row of an older store keeps it null, which is unknown and
/// not 0: `poll-idle` counts only the polls known to have started none.
pub const STEP_8: &str = "
ALTER TABLE poll_cost ADD COLUMN starts INTEGER;
";

/// How long SQLite waits for a lock another connection holds before giving
/// up, as the spec's section 4 states it. A database still busy then is exit
/// 1 with the word `busy`. A test that only needs to meet a busy database asks
/// for the short pair of `install_test_bounds`; what a host runs waits this.
pub const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// How long a reader waits. In WAL mode a writer's lock never stops a reader;
/// what does is SQLite's own brief exclusive steps, the recovery of the -shm
/// and the checkpoint of the last connection to close, which are over in
/// milliseconds. Without any wait, tests/concurrency.rs saw a `check` answer
/// `busy` during a recovery.
pub const READ_WAIT: Duration = Duration::from_secs(1);

/// The waits a test may install for its thread in place of the installed pair:
/// `busy`, but arrived at in milliseconds rather than seconds, so a test that
/// proves the word is the lock's does not sit through the installed bound. The
/// pair is a tenth of it -- far enough below that a test which means to beat a
/// lock it holds itself (verbs_tests::a_reader_waits_out_a_short_lock) keeps its
/// margin, and far enough above a millisecond of polling that a test which means
/// to meet the bound still sees the handler run.
/// `tests::the_installed_bounds_are_the_ones_the_spec_states` is the test that
/// pins the installed pair, and
/// `tests::a_store_the_tests_did_not_shorten_waits_the_installed_bounds` the one
/// that shows the program a host runs is not on this pair.
#[cfg(test)]
pub const TEST_BUSY_TIMEOUT: Duration = Duration::from_millis(500);

/// See `TEST_BUSY_TIMEOUT`.
#[cfg(test)]
pub const TEST_READ_WAIT: Duration = Duration::from_millis(200);

// The pair `install_test_bounds` puts on the waits, None for the program a host
// runs. Read by every connection of the thread and set only by the tests, so one
// run waits one pair and nothing changes it under a test already waiting -- the
// tests run in parallel, so a bound one test could set at will would be a race
// rather than a setting.
#[cfg(test)]
thread_local! {
    static TEST: std::cell::Cell<Option<(Duration, Duration)>> = const {
        std::cell::Cell::new(None)
    };
}

/// How often a waiting connection asks again. SQLite's own busy timeout backs
/// off to 100 ms between tries, so among writers that keep the lock busy the
/// one that has waited longest asks least often, and it can lose every turn
/// until its five seconds are gone: tests/concurrency.rs saw that with eight
/// writers. Asking every millisecond gives each waiter the same chance.
const BUSY_POLL: Duration = Duration::from_millis(1);

/// A busy handler: true to try again. `tries` is 0 at the start of each wait,
/// which is when the clock starts. A process of the store waits on one
/// connection at a time, so the start is kept per thread. The count is part of
/// SQLite's handler signature and is not read: the limit is a duration, so only
/// the clock says when a wait has run out.
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
    busy_wait(tries, write_bound())
}

fn read_wait(tries: i32) -> bool {
    busy_wait(tries, read_bound())
}

/// The two waits of this program for a connection of `purpose`: the bound its
/// busy handler takes, and the constant that bound is meant to come from.
///
/// Both are answered, in the build that answers the bound check at all, so the
/// one test that runs the store as a child can compare them. A debug binary is a
/// separate compilation from the test harness, so the child's constant and the
/// parent's are not the same read: reported together with the wait that met the
/// bound, they close the gap where a bound drifted from its constant on either
/// side and the elapsed time still agreed with the parent's copy. See
/// `tests::a_store_the_tests_did_not_shorten_waits_the_installed_bounds`.
#[cfg(debug_assertions)]
pub(crate) fn bound_check_answer(purpose: Purpose) -> (Duration, Duration) {
    // Read on a thread that never installed the short pair, which is what
    // `bound_check` does, so the two are the installed bound and its constant.
    match purpose {
        Purpose::Write => (write_bound(), installed_constants().0),
        Purpose::Read => (read_bound(), installed_constants().1),
    }
}

/// The bound of a connection that writes, of one that reads, and of publish's
/// wait for state/publish.lock: the installed `BUSY_TIMEOUT`, `READ_WAIT` and
/// `live::LOCK_WAIT`, unless this thread installed the tests' short pair. The
/// three are the only places the short pair is applied, so the two waits of the
/// database and the one file lock of the live view are shortened together, and a
/// build a host runs has no `install_test_bounds` to call and waits the spec.
///
/// None of the three ends on a constant. The one test that cannot ask for the
/// short pair runs the store as a child process, and a child of a test run is
/// built with `cfg(test)` on without asking, so its bound is read from here too.
/// Answering from one place keeps the bound a child was given and the bound its
/// parent asserts against on the same read, so a bound that drifted from the
/// constant is measured rather than read past.
pub(crate) fn write_bound() -> Duration {
    #[cfg(test)]
    if let Some((write, _)) = installed() {
        return write;
    }
    installed_constants().0
}

/// See `write_bound`.
pub(crate) fn read_bound() -> Duration {
    #[cfg(test)]
    if let Some((_, read)) = installed() {
        return read;
    }
    installed_constants().1
}

/// See `write_bound`.
pub(crate) fn lock_wait() -> Duration {
    #[cfg(test)]
    if let Some((write, _)) = installed() {
        return write;
    }
    installed_constants().2
}

/// The three waits of the program a host runs: the busy handler of a connection
/// that writes or reads, and publish's retry loop. Each of the three reads its
/// value here, so the bound a child process was given and the bound its parent
/// asserts against come from one place. The constants themselves are pinned by
/// `tests::the_installed_bounds_are_the_ones_the_spec_states`.
fn installed_constants() -> (Duration, Duration, Duration) {
    (BUSY_TIMEOUT, READ_WAIT, live::LOCK_WAIT)
}

/// The pair this thread installed, if any. `install_test_bounds` puts it there;
/// nothing else can, and a build a host runs has no such code at all.
#[cfg(test)]
fn installed() -> Option<(Duration, Duration)> {
    TEST.with(|bound| bound.get())
}

/// The tests' short pair, installed for this thread. `verbs_tests::short_bounds`
/// calls it before the first connection of a test, and every thread that opens a
/// store asks first, so one test waits one pair and a test that compares an
/// elapsed time with `write_bound` is told the truth. The store binary answers
/// `--test-store-bound-check` without this call, which is how the tests show the
/// installed pair is untouched. Returns the pair, so the caller can say what it
/// asked for.
#[cfg(test)]
pub fn install_test_bounds() -> (Duration, Duration) {
    let pair = (TEST_BUSY_TIMEOUT, TEST_READ_WAIT);
    TEST.with(|bound| bound.set(Some(pair)));
    pair
}

/// The indexes of the schema by name, so the test that pins the schema can name
/// them. Every CREATE INDEX of schema.sql appears here exactly once, which that
/// test is what checks.
#[cfg(test)]
pub const COUNTED_INDEXES: [&str; 14] = [
    "issues_checked",
    "cycles_running",
    "heartbeats_at",
    "endings_recent",
    "approvals_open",
    "questions_issue",
    "cycle_history_issue",
    "plan_history_issue",
    "cycle_history_seq",
    "plan_history_seq",
    "polls_recent",
    "poll_cost_at",
    "actions_open",
    "journal_action",
];

/// The tables `check` counts. Every table of schema.sql appears here exactly
/// once, which the test every_counted_table_exists pins, so a table that goes
/// missing from the schema cannot go missing from the report too.
pub const COUNTED_TABLES: [&str; 41] = [
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
    "planning_notes",
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
    "poll_cost",
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
    // Anything above the version this binary writes is a database this
    // command must not touch. A file made but not yet written has 0 and is the
    // store's own to fill; one below is `migrate`'s to bring forward.
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

pub use tehas_core::file::side_file;

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

/// Writes the schema into the connection and the row of `meta` that says when
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

/// The steps this binary holds. From schema 0, an SQLite file with no tables
/// and a `user_version` of 0 (what SQLite makes of an empty file),
/// `write_schema` is the whole of it: it lays schema.sql at the version this
/// binary writes, writes the rows of `meta` -- `schema` among them -- and sets
/// `user_version`, all in one transaction. From an older schema it is each step
/// up from there in turn, each its own transaction. A database already at
/// SCHEMA_VERSION has nothing to run.
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
        // need one.
        v if v == SCHEMA_VERSION => Ok(()),
        // Each step is asked for outside any transaction of the caller's,
        // which is why `migrate` gives its own lock back before it comes here:
        // SQLite will not run `VACUUM INTO` from within one, and the two would
        // not nest.
        0 => write_schema(
            conn,
            now,
            &settings.version,
            &settings.commit,
            &settings.instance,
            &settings.repo,
        ),
        1 => step(conn, 2, STEP_2)
            .and_then(|()| step(conn, 3, STEP_3))
            .and_then(|()| step_4(conn))
            .and_then(|()| step(conn, 5, STEP_5))
            .and_then(|()| step(conn, 6, STEP_6))
            .and_then(|()| step(conn, 7, STEP_7))
            .and_then(|()| step(conn, 8, STEP_8)),
        2 => step(conn, 3, STEP_3)
            .and_then(|()| step_4(conn))
            .and_then(|()| step(conn, 5, STEP_5))
            .and_then(|()| step(conn, 6, STEP_6))
            .and_then(|()| step(conn, 7, STEP_7))
            .and_then(|()| step(conn, 8, STEP_8)),
        3 => step_4(conn)
            .and_then(|()| step(conn, 5, STEP_5))
            .and_then(|()| step(conn, 6, STEP_6))
            .and_then(|()| step(conn, 7, STEP_7))
            .and_then(|()| step(conn, 8, STEP_8)),
        4 => step(conn, 5, STEP_5)
            .and_then(|()| step(conn, 6, STEP_6))
            .and_then(|()| step(conn, 7, STEP_7))
            .and_then(|()| step(conn, 8, STEP_8)),
        5 => step(conn, 6, STEP_6)
            .and_then(|()| step(conn, 7, STEP_7))
            .and_then(|()| step(conn, 8, STEP_8)),
        6 => step(conn, 7, STEP_7).and_then(|()| step(conn, 8, STEP_8)),
        7 => step(conn, 8, STEP_8),
        v => Err(DbError::Corrupt(format!(
            "no migration from schema {v} of {}, this binary writes {SCHEMA_VERSION}",
            file.display()
        ))),
    }
}

/// One schema step to `to`, in one transaction with `meta.schema` and
/// `user_version`: a failure commits nothing and leaves the store where it
/// was with every row.
fn step(conn: &mut Connection, to: i64, sql: &str) -> Result<(), DbError> {
    let from = to - 1;
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(classify)?;
    // Read again under the lock: another migrate may have run in between.
    let held: i64 = tx
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(classify)?;
    if held != from {
        return Err(DbError::Corrupt(format!(
            "the step from schema {from} found schema {held}"
        )));
    }
    tx.execute_batch(sql).map_err(classify)?;
    // A rebuilt table runs with foreign keys off, so check them before commit.
    let orphan: Option<String> = tx
        .query_row("PRAGMA foreign_key_check", [], |r| r.get(0))
        .optional()
        .map_err(classify)?;
    if let Some(table) = orphan {
        return Err(DbError::Corrupt(format!(
            "the step to schema {to} leaves a row of {table} without its parent"
        )));
    }
    let n = tx
        .execute(
            "UPDATE meta SET value = ?1 WHERE key = 'schema'",
            [to.to_string()],
        )
        .map_err(classify)?;
    if n != 1 {
        return Err(DbError::Corrupt(format!(
            "the store at schema {from} has no schema row in meta"
        )));
    }
    tx.pragma_update(None, "user_version", to)
        .map_err(classify)?;
    tx.commit().map_err(classify)
}

/// The step to 4. Dropping `cycles` with foreign keys on would delete every row
/// that references it, so they are off for the step, which checks them before
/// it commits. They can only be set outside a transaction. The VACUUM after the
/// commit gives back the pages of the dropped tables; if it fails the store is
/// at 4 with its rows, and the caller hears the error.
fn step_4(conn: &mut Connection) -> Result<(), DbError> {
    conn.pragma_update(None, "foreign_keys", "OFF")
        .map_err(classify)?;
    let stepped = step(conn, 4, STEP_4);
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(classify)?;
    stepped?;
    conn.execute_batch("VACUUM").map_err(classify)
}

/// How many names a backup tries before it is refused: `tehas-<v>-<now>.db`,
/// then `-1` to `-99` before the `.db`.
pub const BACKUP_NAMES: u32 = 100;

/// The name of a backup this verb writes, given the version it came from and
/// the instant it was asked for.
pub fn backup_name(dir: &Path, from: i64, now: i64) -> PathBuf {
    backup_name_at(dir, from, now, 0)
}

/// The `n`th name of that backup: two verbs of one update can land in the same
/// second, and the second takes the next name rather than fail.
pub fn backup_name_at(dir: &Path, from: i64, now: i64, n: u32) -> PathBuf {
    if n == 0 {
        dir.join(format!("tehas-{from}-{now}.db"))
    } else {
        dir.join(format!("tehas-{from}-{now}-{n}.db"))
    }
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
/// `state` is `state/` or `state/backup/` of the home the command was pointed
/// at, settled by `backup_dir` below: a directory the store may write a copy of
/// itself into.
///
/// The caller holds the write lock and has ended the transaction it was taken
/// in: SQLite will not copy a database that a transaction is open on, so a verb
/// that locks and copies asks for the lock and gives it back before it asks for
/// the copy. Nothing else is writing while it does, because a verb of the store
/// that changes rows takes the same lock in its own transaction and the busy
/// timeout is what another waits.
pub fn backup(conn: &Connection, state: &Path, from: i64, now: i64) -> Result<PathBuf, DbError> {
    // A name held by anything, a dangling link included, is skipped; the lock
    // the caller holds keeps another verb from taking the one chosen.
    let copy = (0..BACKUP_NAMES)
        .map(|n| backup_name_at(state, from, now, n))
        .find(|c| c.symlink_metadata().is_err())
        .ok_or_else(|| {
            DbError::Unreadable(format!(
                "cannot make the backup {}: its {BACKUP_NAMES} names are taken",
                backup_name(state, from, now).display()
            ))
        })?;
    backup_to(conn, &copy)?;
    Ok(copy)
}

/// `backup` to a name the caller chose, with the same guarantees.
pub fn backup_to(conn: &Connection, copy: &Path) -> Result<(), DbError> {
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
        .open(copy)
        .map(|_| ())
        .map_err(|e| {
            DbError::Unreadable(format!("cannot make the backup {}: {e}", copy.display()))
        })?;
    conn.execute("VACUUM INTO ?1", [name]).map_err(|e| {
        // SQLite may have filled part of the file before it gave up, and a
        // reader cannot tell that copy from a good one. The name is taken away
        // so that the only backup the directory holds is one that finished.
        let _ = fs::remove_file(copy);
        classify(e)
    })?;
    // The copy is written in the rollback journal mode, which the store's
    // readers cannot open: a copy restored over the store must be one the
    // store opens as it is.
    let wal = Connection::open(copy)
        .and_then(|c| c.query_row("PRAGMA journal_mode = WAL", [], |r| r.get::<_, String>(0)))
        .map_err(|e| {
            let _ = fs::remove_file(copy);
            classify(e)
        })?;
    if wal != "wal" {
        let _ = fs::remove_file(copy);
        return Err(DbError::Unreadable(format!(
            "the backup {} took journal mode {wal}",
            copy.display()
        )));
    }
    set_mode(copy, 0o600)
}

/// The directory `migrate --backup` was given, settled to one of the two places
/// the verb may write: `state/` of the home the command was pointed at, or
/// `state/backup/` below it, resolved.
///
/// A backup is a copy of the store, so it is readable by nobody but the tehas
/// user, and the mode that buys is the one on the directory: `state/` is 0700
/// and is the store's own directory, made so by `create`. A `<dir>` off the
/// command line is a person's -- it could be a link, it could walk out of the
/// home with a `..`, it could be 0755 and hold a copy of every approval the host
/// ever recorded -- and the store was never given leave to write one there. The
/// resolution comes first so that a link is answered as the directory it is,
/// and the comparison of the two comes before the name: the verb asks once.
///
/// `state/backup/` is the store's too: it is made here, 0700, when it is not
/// there, and kept 0700 when it is, so the copies `install.sh` asks for stay
/// out of the way of the files beside the store without leaving its directory.
pub fn backup_dir(home: &Path, dir: &Path) -> Result<PathBuf, DbError> {
    let own = home.join("state").join("backup");
    if dir == own {
        match fs::symlink_metadata(dir) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                fs::DirBuilder::new().mode(0o700).create(dir).map_err(|e| {
                    DbError::Unreadable(format!("cannot make {}: {e}", dir.display()))
                })?;
            }
            _ => {}
        }
    }
    let state = dir
        .canonicalize()
        .map_err(|e| DbError::Unreadable(format!("the backup directory {}: {e}", dir.display())))?;
    // Resolved too, so a home reached through a link compares like for like.
    let home = home
        .canonicalize()
        .map_err(|e| DbError::Unreadable(format!("the home {}: {e}", home.display())))?;
    let own = home.join("state").join("backup");
    if state == own {
        set_mode(&state, 0o700)?;
        return Ok(state);
    }
    if state != home.join("state") {
        return Err(DbError::Unreadable(format!(
            "the backup goes in the state/ or state/backup/ of the store's home, not {}",
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
        // A store not yet migrated lacks the tables of the later versions; it
        // is reported as it is. At the version this binary writes, a missing
        // table is the error the count below gives.
        if schema < SCHEMA_VERSION {
            let there: bool = conn
                .query_row(
                    "SELECT count(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |r| r.get(0),
                )
                .map_err(classify)?;
            if !there {
                continue;
            }
        }
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
