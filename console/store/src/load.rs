//! `store load --from <file>`: a copy made by `store copy` loaded into a store
//! that holds no history, the inverse of the copy.
//!
//! The copy is read first, from a file of its own beside the store: SQLite's
//! header, PRAGMA integrity_check, and its schema, which is refused when it is
//! newer than this binary's and brought forward by the steps of `migrate` when
//! it is older. A copy that fails any of these is refused before the store is
//! opened. Then, in one transaction under the write lock, the store must be at
//! this binary's schema, no cycle lock may be held and no table of HISTORY may
//! hold a row; every table is emptied and filled from the copy. A refusal or a
//! failure rolls the transaction back, so the store is as it was. The store's
//! own file is never replaced: the readers of a drained factory hold it open,
//! and a file renamed under them would meet the old one's -wal.
//!
//! Answers the copy's schema as it came, its rows but meta's, and the ms it
//! took. The refusals are 1 with a word: `no-copy`, `bad-copy`, `newer-copy`,
//! `running` and `history`.

use crate::db::{self, COUNTED_TABLES, SCHEMA_VERSION};
use crate::lock;
use crate::verbs::{self, j, line, Fail, Q};
use rusqlite::{Connection, OpenFlags};
use std::fs::{self, OpenOptions};
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// The tables that record what the factory did. A store with a row in any of
/// them holds history and is never loaded over. The five counted tables beside
/// these -- `meta`, `issues`, `polls`, `poll_cost` and `sweep_log` -- are the
/// installation's record, the numbers the poll reads from GitHub and its own
/// log, which a drained factory may still write and the copy's rows replace.
pub const HISTORY: [&str; 18] = [
    "cycles",
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
    "actions",
    "journal",
];

/// The copy's own file beside the store, and SQLite's files beside it, removed
/// when dropped.
struct Beside(PathBuf);

impl Drop for Beside {
    fn drop(&mut self) {
        for end in ["", "-wal", "-shm", "-journal"] {
            let mut name = self.0.clone().into_os_string();
            name.push(end);
            let _ = fs::remove_file(PathBuf::from(name));
        }
    }
}

fn bad(from: &str, why: impl std::fmt::Display) -> Fail {
    (1, format!("bad-copy: the copy {from} {why}"))
}

/// The copy's bytes in a new file of mode 0600 beside the store.
fn place(from: &str, store: &Path) -> Result<Beside, Fail> {
    match fs::metadata(from) {
        Ok(m) if m.is_file() => {}
        Ok(_) => return Err(bad(from, "is not a file")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err((1, format!("no-copy: the copy {from} is not there")))
        }
        Err(e) => return Err(bad(from, format!("cannot be read: {e}"))),
    }
    let dir = store.parent().unwrap_or(Path::new("/"));
    let side = Beside(dir.join(format!(".tehas.db.load-{}", std::process::id())));
    drop(Beside(side.0.clone()));
    let unwritable = |e: io::Error| {
        (
            1,
            format!("cannot write {} beside the store: {e}", side.0.display()),
        )
    };
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&side.0)
        .map_err(unwritable)?;
    let mut src = fs::File::open(from).map_err(|e| bad(from, format!("cannot be read: {e}")))?;
    io::copy(&mut src, &mut out).map_err(unwritable)?;
    out.sync_all().map_err(unwritable)?;
    Ok(side)
}

/// The copy checked and at this binary's schema: the schema it came at and its
/// rows but meta's.
fn ready(from: &str, side: &Path, now: i64) -> Result<(i64, i64), Fail> {
    if !db::is_database(side) {
        return Err(bad(from, "is not an SQLite database"));
    }
    let mut c = Connection::open_with_flags(
        side,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| bad(from, format!("cannot be opened: {e}")))?;
    let unreadable = |e: rusqlite::Error| bad(from, format!("cannot be read: {e}"));
    let mut said = Vec::new();
    {
        let mut q = c.prepare("PRAGMA integrity_check").map_err(unreadable)?;
        let rows = q
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(unreadable)?;
        for row in rows {
            said.push(row.map_err(unreadable)?);
        }
    }
    if said != ["ok"] {
        return Err(bad(from, format!("does not check: {}", said.join("; "))));
    }
    let schema: i64 = c
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(unreadable)?;
    if schema > SCHEMA_VERSION {
        return Err((
            1,
            format!(
                "newer-copy: the copy {from} holds schema {schema}, this release reads schema {SCHEMA_VERSION} and older"
            ),
        ));
    }
    if schema < 1 {
        return Err(bad(from, "holds no schema"));
    }
    // The installation's values are written only by a migration from 0, which
    // was refused above.
    let unused = db::Settings {
        version: "load".to_string(),
        commit: "load".to_string(),
        instance: "load".to_string(),
        repo: "load".to_string(),
    };
    db::migrate(&mut c, side, schema, now, &unused).map_err(|e| {
        let why = e.detail().unwrap_or_else(|| e.word().to_string());
        bad(from, format!("did not migrate from schema {schema}: {why}"))
    })?;
    let mut rows = 0;
    for table in COUNTED_TABLES.iter().filter(|t| **t != "meta") {
        // The name is one of COUNTED_TABLES, never a caller's word.
        rows += c
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| {
                r.get::<_, i64>(0)
            })
            .map_err(unreadable)?;
    }
    Ok((schema, rows))
}

/// The columns of a table of the store, quoted, in its order.
fn columns(conn: &Connection, table: &str) -> Result<String, Fail> {
    let mut q = conn
        .prepare(&format!("PRAGMA main.table_info({table})"))
        .q()?;
    let names = q.query_map([], |r| r.get::<_, String>(1)).q()?;
    let mut out = Vec::new();
    for n in names {
        out.push(format!("\"{}\"", n.q()?.replace('"', "\"\"")));
    }
    Ok(out.join(", "))
}

/// The refusals, and the store emptied and filled from the attached copy.
fn fill(conn: &Connection, home: &Path) -> Result<(), Fail> {
    verbs::schema(conn)?;
    let issues = lock::listed(home).map_err(|e| (1, format!("running: {e}")))?;
    if let Some(n) = issues
        .into_iter()
        .find(|n| lock::probe_file(&lock::path(home, *n)).is_some_and(|l| l.rc != 0))
    {
        return Err((1, format!("running: a cycle is running for #{n}")));
    }
    for table in HISTORY {
        let n: i64 = conn
            .query_row(&format!("SELECT count(*) FROM main.{table}"), [], |r| {
                r.get(0)
            })
            .q()?;
        if n > 0 {
            return Err((
                1,
                format!(
                    "history: the state store holds history, {n} rows in {table}; a copy loads only into a new or empty store"
                ),
            ));
        }
    }
    // Checked at the commit, when every table holds the copy's rows.
    conn.execute_batch("PRAGMA defer_foreign_keys = ON").q()?;
    for table in COUNTED_TABLES {
        conn.execute_batch(&format!("DELETE FROM main.{table}"))
            .q()?;
    }
    for table in COUNTED_TABLES {
        let cols = columns(conn, table)?;
        conn.execute_batch(&format!(
            "INSERT INTO main.{table} ({cols}) SELECT {cols} FROM copy.{table}"
        ))
        .q()?;
    }
    Ok(())
}

pub fn load(home: &Path, from: &str, now: i64) -> Result<String, Fail> {
    let began = Instant::now();
    let store = db::path(home);
    let side = place(from, &store)?;
    let (schema, rows) = ready(from, &side.0, now)?;
    let conn = db::open(&store, db::Purpose::Write).map_err(crate::line)?;
    let name = side
        .0
        .to_str()
        .ok_or_else(|| (1, format!("{} is not UTF-8", side.0.display())))?;
    // SQLite attaches only outside a transaction.
    conn.execute("ATTACH DATABASE ?1 AS copy", [name]).q()?;
    conn.execute_batch("BEGIN IMMEDIATE").q()?;
    let done = fill(&conn, home).and_then(|()| conn.execute_batch("COMMIT").q());
    if done.is_err() {
        let _ = conn.execute_batch("ROLLBACK");
    }
    let _ = conn.execute_batch("DETACH DATABASE copy");
    done?;
    drop(conn);
    let ms = i64::try_from(began.elapsed().as_millis()).unwrap_or(i64::MAX);
    Ok(line(
        "load",
        &[
            ("file", j(from)),
            ("schema", j(schema)),
            ("rows", j(rows)),
            ("ms", j(ms)),
        ],
    ))
}
