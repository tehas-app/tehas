//! The store against scratch homes: the mode of the file, the refusals that
//! come before SQLite is asked anything, the four classes of failure, and what
//! `init` and `check` answer.

use crate::db;
use crate::{execute, parse, Settings, Verb};
use rusqlite::Connection;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

fn scratch(name: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = crate::verbs_tests::scratch_root()
        .join(format!("test-{}-{name}", N.fetch_add(1, Ordering::SeqCst)));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn settings() -> Settings {
    Settings {
        version: "v0.10.2".to_string(),
        commit: "cb6912a".to_string(),
        instance: "tehas-tehas".to_string(),
        repo: "example-owner/example-product".to_string(),
    }
}

fn init(home: &Path, force: bool) -> Result<String, (u8, String)> {
    execute(
        home,
        &Verb::Init {
            force,
            settings: settings(),
        },
        1_792_000_000,
    )
}

fn check(home: &Path) -> Result<String, (u8, String)> {
    execute(home, &Verb::Check, 1_792_000_000)
}

fn one<T: rusqlite::types::FromSql>(conn: &Connection, sql: &str) -> T {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn init_makes_a_0600_file_in_a_0700_state_with_its_side_files() {
    let home = scratch("mode");
    // The umask of the process is what state/ would take without the store's
    // own chmod, so the file is only 0700 because the command said so.
    let line = init(&home, false).unwrap();
    assert!(line.contains("\"created\":true"), "{line}");
    let file = db::path(&home);
    assert_eq!(mode(&file), 0o600);
    // state/ is 0700 whatever the umask of the caller was: the store made the
    // directory itself and set the mode on it.
    assert_eq!(mode(&home.join("state")), 0o700);
    // The two siblings SQLite makes beside the file are the store's data until
    // the next checkpoint, so they carry the file's mode too. A database with
    // no rows to checkpoint has no -wal and no -shm at all once its last
    // reader has gone, so the mode of the siblings is asked of one that has
    // them, in the test of the same name below.
    let conn = Connection::open(&file).unwrap();
    assert_eq!(one::<i64>(&conn, "PRAGMA user_version"), db::SCHEMA_VERSION);
    assert_eq!(one::<String>(&conn, "PRAGMA journal_mode"), "wal");
    // synchronous and foreign_keys answer as integers: 2 is FULL and 1 is ON.
    assert_eq!(one::<i64>(&conn, "PRAGMA synchronous"), 2);
    assert_eq!(one::<i64>(&conn, "PRAGMA foreign_keys"), 1);
}

#[test]
fn the_wal_and_shm_siblings_take_the_file_s_mode() {
    let home = scratch("sides");
    init(&home, false).unwrap();
    // Rows, so the write goes through the -wal and it is still there for the
    // next reader to see. The connection is held to the end of the test: when
    // the last one closes, SQLite checkpoints and removes both siblings, and
    // then there is no sibling left to ask the mode of.
    let _held = seed(&home);
    let file = db::path(&home);
    for suffix in ["-wal", "-shm"] {
        let sibling = side(&file, suffix);
        assert!(sibling.exists(), "{suffix} was not made");
        assert_eq!(mode(&sibling), 0o600, "{suffix}");
    }
}

#[test]
fn init_writes_the_row_of_meta_and_the_caller_s_settings() {
    let home = scratch("meta");
    init(&home, false).unwrap();
    let conn = Connection::open(db::path(&home)).unwrap();
    let got = one::<String>(
        &conn,
        "SELECT group_concat(key || '=' || value, ' ') FROM (SELECT * FROM meta ORDER BY key)",
    );
    let mut want = [
        "created_at=1792000000",
        "instance=tehas-tehas",
        "repo=example-owner/example-product",
        "schema=3",
        "tehas_commit=cb6912a",
        "tehas_version=v0.10.2",
    ]
    .to_vec();
    want.sort_unstable();
    assert_eq!(got.split(' ').collect::<Vec<_>>(), want);
}

#[test]
fn init_refuses_an_existing_file_and_writes_nothing() {
    let home = scratch("exists");
    init(&home, false).unwrap();
    let file = db::path(&home);
    let before = fs::read(&file).unwrap();
    // The refusal is the verb's precondition, not the database's, so it is
    // exit 1 and the word names the file the caller must decide about.
    let err = init(&home, false).unwrap_err();
    assert_eq!(err.0, 1, "{}", err.1);
    assert!(
        err.1.starts_with("the database exists already"),
        "{}",
        err.1
    );
    assert_eq!(fs::read(&file).unwrap(), before);
    // Nothing was added beside it either.
    assert_eq!(fs::read_dir(home.join("state")).unwrap().count(), 1);
}

#[test]
fn force_replaces_an_empty_file_and_refuses_one_with_rows() {
    let home = scratch("force");
    let file = db::path(&home);
    fs::create_dir_all(home.join("state")).unwrap();
    // An empty file is the store's to fill.
    fs::write(&file, b"").unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
    let line = init(&home, true).unwrap();
    assert!(line.contains("\"created\":false"), "{line}");
    assert_eq!(mode(&file), 0o600);
    // The fill went through the log, so the log and its shared-memory sibling
    // hold the schema until the next checkpoint and carry the file's mode: the
    // umask of the caller is not allowed to decide that of them.
    let _held = seed(&home);
    for suffix in ["-wal", "-shm"] {
        let sibling = side(&file, suffix);
        assert!(sibling.exists(), "{suffix} was not made");
        assert_eq!(mode(&sibling), 0o600, "{suffix} of a file --force filled");
    }
    drop(_held);
    // A file with rows in it is not.
    let (code, err) = init(&home, true).unwrap_err();
    assert_eq!(code, 1);
    assert!(err.contains("holds tables"), "{err}");
    let before = fs::read(&file).unwrap();
    assert!(init(&home, false).is_err());
    assert_eq!(fs::read(&file).unwrap(), before);
}

#[test]
fn force_refuses_a_file_that_is_not_a_database_and_keeps_it() {
    let home = scratch("force-garbage");
    let file = db::path(&home);
    fs::create_dir_all(home.join("state")).unwrap();
    let garbage = b"not a database, and long enough to be read as a header. ".repeat(20);
    fs::write(&file, &garbage).unwrap();
    let (code, err) = init(&home, true).unwrap_err();
    assert_eq!(code, 78);
    assert!(err.starts_with("corrupt "), "{err}");
    // ADR 0003: the damaged file stays exactly where a person can read it.
    assert_eq!(fs::read(&file).unwrap(), garbage);
    assert!(!home.join("state/tehas.db.broken").exists());
    assert!(!home.join("state/tehas.db.schema-1").exists());
    assert_eq!(fs::read_dir(home.join("state")).unwrap().count(), 1);
}

#[test]
fn a_link_at_the_path_is_refused_and_its_target_untouched() {
    let home = scratch("link");
    let elsewhere = scratch("link-target");
    let real = elsewhere.join("elsewhere.db");
    fs::write(&real, b"somebody else's file").unwrap();
    fs::create_dir_all(home.join("state")).unwrap();
    std::os::unix::fs::symlink(&real, db::path(&home)).unwrap();
    for force in [false, true] {
        let (code, err) = init(&home, force).unwrap_err();
        assert_eq!(code, 78, "a link is refused, not followed");
        assert!(err.starts_with("corrupt "), "{err}");
        assert!(err.contains("is a link"), "{err}");
    }
    assert_eq!(fs::read(&real).unwrap(), b"somebody else's file");
    let (code, err) = check(&home).unwrap_err();
    assert_eq!(code, 78);
    assert!(err.starts_with("corrupt "), "{err}");
    // A link for state/ itself moves the store outside the home.
    let moved = scratch("link-dir");
    let elsewhere = scratch("link-dir-target");
    std::os::unix::fs::symlink(&elsewhere, moved.join("state")).unwrap();
    let (code, err) = init(&moved, true).unwrap_err();
    assert_eq!(code, 78);
    assert!(err.starts_with("unreadable "), "{err}");
    assert!(err.contains("is a link"), "{err}");
    assert!(!elsewhere.join("tehas.db").exists());
}

/// Checks that a path outside the home is refused before SQLite is asked
/// anything, and says which tree it sits in.
fn guard_path(home: &Path, rel: &str, said: &str) {
    let file = home.join(rel).join("tehas.db");
    let err = db::open(&file, db::Purpose::Read).unwrap_err();
    assert_eq!(err, db::DbError::Unreadable(said.to_string()));
    assert!(!file.exists());
}

#[test]
fn a_path_under_live_or_state_index_is_refused() {
    let served = scratch("under-live");
    fs::create_dir_all(served.join("live")).unwrap();
    guard_path(
        &served,
        "live",
        "the path is under live/, which Caddy serves",
    );
    let indexed = scratch("under-index");
    fs::create_dir_all(indexed.join("state/index")).unwrap();
    guard_path(
        &indexed,
        "state/index",
        "the path is under state/index, which the indexing container writes",
    );
    // A directory that merely has the name is not the tree the rule is about.
    let named = scratch("named-live");
    fs::create_dir_all(named.join("state/livelier")).unwrap();
    assert_eq!(
        db::open(&named.join("state/livelier/tehas.db"), db::Purpose::Read).unwrap_err(),
        db::DbError::Missing
    );
}

#[test]
fn each_of_the_four_classes_exits_78_with_its_word() {
    // missing: no file at all.
    let home = scratch("missing");
    let (code, err) = check(&home).unwrap_err();
    assert_eq!(code, 78);
    assert_eq!(err, "missing state/tehas.db");

    // corrupt: a file whose bytes are not a database.
    let home = scratch("corrupt");
    fs::create_dir_all(home.join("state")).unwrap();
    fs::write(
        db::path(&home),
        b"not a database, and long enough to be read as a header. ".repeat(20),
    )
    .unwrap();
    let (code, err) = check(&home).unwrap_err();
    assert_eq!(code, 78);
    assert!(err.starts_with("corrupt the database"), "{err}");

    // corrupt: a good database cut short by one whole page, so the count at its
    // front names a page the bytes of the file do not reach. SQLite calls that
    // a malformed image and the spec's table calls it `corrupt`. A cut that
    // only shortens the last page is not the same case: the count stays honest
    // about what can still be read and the checks answer ok.
    let home = scratch("truncated");
    init(&home, false).unwrap();
    drop(seed(&home));
    let file = db::path(&home);
    checkpoint(&file);
    let bytes = fs::read(&file).unwrap();
    assert!(bytes.len() > 4 * 4096, "the seeded file has pages to lose");
    fs::write(&file, &bytes[..bytes.len() - 4096]).unwrap();
    let (code, err) = check(&home).unwrap_err();
    assert_eq!(code, 78, "a truncated file is corrupt");
    assert!(err.starts_with("corrupt "), "{err}");

    // unreadable: there, but no permission on it.
    let home = scratch("unreadable");
    init(&home, false).unwrap();
    checkpoint(&db::path(&home));
    fs::set_permissions(db::path(&home), fs::Permissions::from_mode(0o000)).unwrap();
    let (code, err) = check(&home).unwrap_err();
    fs::set_permissions(db::path(&home), fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(code, 78);
    assert!(err.starts_with("unreadable the database"), "{err}");

    // unreadable: a directory where the file should be.
    let home = scratch("directory");
    fs::create_dir_all(home.join("state/tehas.db")).unwrap();
    let (code, err) = check(&home).unwrap_err();
    assert_eq!(code, 78);
    assert!(err.starts_with("unreadable the database"), "{err}");

    // future: user_version ahead of what this binary writes.
    let home = scratch("future");
    init(&home, false).unwrap();
    let conn = Connection::open(db::path(&home)).unwrap();
    conn.pragma_update(None, "user_version", db::SCHEMA_VERSION + 1)
        .unwrap();
    drop(conn);
    checkpoint(&db::path(&home));
    let (code, err) = check(&home).unwrap_err();
    assert_eq!(code, 78);
    assert!(err.starts_with("future the database"), "{err}");
    assert!(err.contains("schema 4"), "{err}");
}

#[test]
fn the_four_words_are_the_ones_the_spec_names() {
    let cases: [(db::DbError, &str); 5] = [
        (db::DbError::Missing, "missing"),
        (db::DbError::Corrupt("x".into()), "corrupt"),
        (db::DbError::Unreadable("x".into()), "unreadable"),
        (db::DbError::Future(2), "future"),
        (db::DbError::Busy, "busy"),
    ];
    for (err, word) in cases {
        assert_eq!(err.word(), word);
    }
}

#[test]
fn check_reports_its_schema_and_an_integrity_of_ok() {
    let home = scratch("check");
    init(&home, false).unwrap();
    let line = check(&home).unwrap();
    assert!(
        line.starts_with("{\"api\":1,\"verb\":\"check\",\"ok\":true"),
        "{line}"
    );
    assert!(line.contains("\"schema\":3"), "{line}");
    assert!(line.contains("\"integrity\":\"ok\""), "{line}");
    assert!(line.contains("\"meta\":6"), "{line}");
    assert!(line.contains("\"issues\":0"), "{line}");
    assert!(line.ends_with('}'), "{line}");
    // One object on stdout, one line, valid JSON.
    assert_eq!(line.lines().count(), 1);
    let value: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(value["api"], 1);
    assert_eq!(value["verb"], "check");
    assert_eq!(value["ok"], true);
    let rows = value["rows"].as_object().unwrap();
    assert_eq!(rows.len(), db::COUNTED_TABLES.len());
    for table in db::COUNTED_TABLES {
        assert!(rows.contains_key(table), "{table} is not counted");
    }
}

#[test]
fn every_counted_table_exists_and_every_table_of_the_schema_is_counted() {
    let home = scratch("tables");
    init(&home, false).unwrap();
    let conn = Connection::open(db::path(&home)).unwrap();
    let mut q = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .unwrap();
    let names: Vec<String> = q
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    let mut counted: Vec<&str> = db::COUNTED_TABLES.to_vec();
    counted.sort_unstable();
    let mut counted: Vec<String> = counted.iter().map(|s| s.to_string()).collect();
    counted.sort_unstable();
    assert_eq!(names, counted, "the schema and the counts have diverged");
    assert_eq!(
        counted.len(),
        db::COUNTED_TABLES.len(),
        "a table is counted twice"
    );
    // The indexes by name too, so the test that damages one of them names a
    // page that exists.
    let mut q = conn
        .prepare("SELECT name FROM sqlite_schema WHERE type = 'index' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .unwrap();
    let names: Vec<String> = q
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    let mut counted: Vec<String> = db::COUNTED_INDEXES.iter().map(|s| s.to_string()).collect();
    counted.sort_unstable();
    assert_eq!(
        names, counted,
        "the schema and the named indexes have diverged"
    );
    assert_eq!(
        counted.len(),
        db::COUNTED_INDEXES.len(),
        "an index is named twice"
    );
}

#[test]
fn check_says_what_failed_and_exits_1_without_moving_the_file() {
    let home = scratch("integrity");
    init(&home, false).unwrap();
    let file = db::path(&home);
    checkpoint(&file);
    // Rows of its own, filled to the width of a title a host really holds: the
    // seed of the other tests is shorter, and at its width the pages pack so
    // thin that the first leaf the walk below reaches holds no rows at all.
    {
        let conn = db::open(&file, db::Purpose::Write).unwrap();
        for n in 0..500 {
            conn.execute(
                "INSERT INTO issues (issue, kind, kind_at, title, checked_at) VALUES (?1, 'issue', ?2, ?3, ?2)",
                rusqlite::params![n, 1_792_000_000_i64, format!("row {n} padded out to make pages")],
            )
            .unwrap();
        }
    }
    checkpoint(&file);
    // What is damaged is one entry of the cell pointer array at the end of a
    // leaf: 13 is the page type that holds rows themselves, and the pointer of
    // the last cell sits at the very last two bytes of the page. Set to an
    // offset the page has no bytes at, that row is gone from the table and its
    // entries in issues_checked remain -- the one disagreement quick_check does
    // not compare and integrity_check does. The file then opens, answers, and
    // is ill, and no byte of it reads as malformed: the case the exit of 1 is
    // for, as distinct from the 78 of a file no open gets through. Most leaves
    // of the file answer for their own damage at the quick_check of every open
    // -- a fragment or a freeblock the page never counted, which is the shape
    // of the file and the word `corrupt` -- so each leaf is asked in turn and
    // the file is left with the first whose fault only integrity_check reports.
    // The store reports it and leaves the file exactly where it is.
    let page_len =
        usize::try_from(db::pages::file_page_size(&file).expect("the file states its page size"))
            .unwrap();
    let base = fs::read(&file).unwrap();
    let mut bytes = base.clone();
    // The pages of the other tables are passed over: their own damage is a
    // fragment or a freeblock the checks of every open read first, and those
    // are the shape of the file and the word `corrupt`. The walk asks each leaf
    // of the file in turn, and the first one whose damage only integrity_check
    // reports is the one the file is left with.
    let mut damaged = None;
    for page in 1.. {
        let offset = match db::pages::page_offset(&file, page) {
            Some(o) => usize::try_from(o).unwrap(),
            None => break,
        };
        if base[offset] != 0x0d {
            continue;
        }
        // More than a few cells, so the page is one that really holds rows and
        // the pointer at its end is a row's and not SQLite's own.
        if db::pages::cell_count(&file, page).unwrap_or(0) <= 3 {
            continue;
        }
        let entry = offset + page_len - 2;
        let mut tried = base.clone();
        tried[entry] = 0x01;
        tried[entry + 1] = 0x01;
        fs::write(&file, &tried).unwrap();
        match check(&home) {
            Err((1, err)) => {
                assert!(err.starts_with("integrity "), "{err}");
                assert_ne!(err, "integrity ok", "{err}");
                bytes = tried;
                damaged = Some(page);
                break;
            }
            // Some other structure of the page answered first, or nothing did:
            // put the page back as it was and ask the next one.
            Err((78, _)) | Ok(_) => fs::write(&file, &base).unwrap(),
            Err((code, err)) => panic!("unexpected {code}: {err}"),
        }
    }
    assert!(
        damaged.is_some(),
        "no leaf of the file holds a row whose loss only integrity_check sees"
    );
    // The bytes of the winner are back in the file: the loop left them there,
    // and this writes them once more so that what the check below reads is the
    // file and not a lucky leftover of the search.
    fs::write(&file, &bytes).unwrap();
    match check(&home) {
        Err((1, err)) => {
            assert!(err.starts_with("integrity "), "{err}");
            assert_ne!(err, "integrity ok", "{err}");
        }
        Err((code, err)) => panic!("unexpected {code}: {err}"),
        Ok(line) => panic!("the damage went unnoticed: {line}"),
    }
    assert_eq!(fs::read(&file).unwrap(), bytes);
    // Nothing moved aside and nothing quarantined: the store of ADR 0003 keeps
    // the file exactly where it is. Only the two names SQLite gives a database
    // with a log are allowed beside it, and no third -- a `.broken` of the
    // analytics program's habit would be one, and so is any copy of the file
    // under another name.
    let mut kept = fs::read_dir(home.join("state"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect::<Vec<_>>();
    kept.sort();
    let allowed = ["tehas.db", "tehas.db-shm", "tehas.db-wal"];
    let allowed: Vec<std::ffi::OsString> = allowed.iter().map(std::ffi::OsString::from).collect();
    for name in &kept {
        assert!(
            allowed.contains(name),
            "state/ holds {name:?}, which the verb put there"
        );
    }
    assert!(
        kept.contains(&std::ffi::OsString::from("tehas.db")),
        "the file is gone"
    );
}

#[test]
fn a_page_of_an_empty_table_holds_nothing_to_damage() {
    // Why the test above damages a page with rows on it: a byte of the page of
    // a table with none is SQLite's own header, and changing it answers the
    // same as leaving it.
    let home = scratch("empty-page");
    init(&home, false).unwrap();
    let file = db::path(&home);
    checkpoint(&file);
    let conn = db::open_read_only(&file).unwrap();
    let page = db::pages::page_of(&conn, "issues").unwrap();
    drop(conn);
    let offset = db::pages::page_offset(&file, page).unwrap() as usize;
    let mut bytes = fs::read(&file).unwrap();
    bytes[offset + 0x20] ^= 0xff;
    fs::write(&file, &bytes).unwrap();
    assert_eq!(fs::read(&file).unwrap(), bytes);
    assert!(check(&home).is_ok());
}

#[test]
fn a_locked_database_is_exit_1_and_the_word_busy() {
    let home = scratch("busy");
    init(&home, false).unwrap();
    // A second connection holds the write lock, and the store is made to want
    // it: opening a database reads no page, so the contention has to be asked
    // for rather than hoped for. The holder waits for nothing, so the store
    // cannot win the race and must report it.
    let mut holder = Connection::open(db::path(&home)).unwrap();
    holder.pragma_update(None, "busy_timeout", 0).unwrap();
    let tx = holder
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    tx.execute("INSERT INTO meta (key, value) VALUES ('held', '1')", [])
        .unwrap();
    let asked = std::time::Instant::now();
    let err = db::open_write(&db::path(&home)).unwrap_err();
    assert_eq!(err, db::DbError::Busy, "{err:?}");
    assert_eq!(err.word(), "busy");
    assert!(asked.elapsed() >= db::BUSY_TIMEOUT, "{:?}", asked.elapsed());
    drop(tx);
    // Once the lock is gone the same call answers, so the word is the lock's
    // and not the file's.
    assert!(db::open_write(&db::path(&home)).is_ok());
    assert!(check(&home).is_ok());
}

#[test]
fn a_reader_waits_a_second_for_a_lock_and_not_the_writers_five() {
    // A writer's lock does not stop a reader in WAL mode, so the holder takes
    // the one lock that does: the whole file, as locking_mode EXCLUSIVE keeps
    // it after a write.
    let home = scratch("read-wait");
    init(&home, false).unwrap();
    let holder = Connection::open(db::path(&home)).unwrap();
    holder
        .pragma_update(None, "locking_mode", "EXCLUSIVE")
        .unwrap();
    holder
        .execute("INSERT INTO meta (key, value) VALUES ('held', '1')", [])
        .unwrap();
    let asked = std::time::Instant::now();
    let err = db::open_read_only(&db::path(&home)).unwrap_err();
    let waited = asked.elapsed();
    assert_eq!(err, db::DbError::Busy, "{err:?}");
    assert!(
        waited >= db::READ_WAIT && waited < db::BUSY_TIMEOUT,
        "{waited:?}"
    );
    drop(holder);
    assert!(check(&home).is_ok());
}

#[test]
fn a_store_connection_waits_with_its_own_handler_not_sqlite_s_timeout() {
    // Setting a busy handler clears SQLite's busy_timeout, so a connection that
    // still reports 5000 or 1000 waits with SQLite's backoff of up to 100 ms.
    let home = scratch("handler");
    init(&home, false).unwrap();
    let file = db::path(&home);
    let write = db::open(&file, db::Purpose::Write).unwrap();
    assert_eq!(one::<i64>(&write, "PRAGMA busy_timeout"), 0);
    let read = db::open_read_only(&file).unwrap();
    assert_eq!(one::<i64>(&read, "PRAGMA busy_timeout"), 0);
}

#[test]
fn the_busy_handler_polls_until_its_limit_and_restarts_with_each_wait() {
    use std::time::{Duration, Instant};
    let limit = Duration::from_millis(50);
    let asked = Instant::now();
    assert!(db::busy_wait(0, limit));
    assert!(db::busy_wait(1, limit));
    // Two tries at BUSY_POLL each, well inside SQLite's own 100 ms steps.
    assert!(
        asked.elapsed() < Duration::from_millis(40),
        "{:?}",
        asked.elapsed()
    );
    std::thread::sleep(limit);
    assert!(!db::busy_wait(2, limit));
    // A new wait starts a new clock.
    assert!(db::busy_wait(0, limit));
    assert!(db::busy_wait(1, limit));
}

#[test]
fn check_can_leave_the_wal_and_shm_beside_the_file_at_its_mode() {
    let home = scratch("check-sides");
    init(&home, false).unwrap();
    let file = db::path(&home);
    for suffix in ["-wal", "-shm"] {
        assert!(!side(&file, suffix).exists(), "{suffix} before check");
    }
    check(&home).unwrap();
    for suffix in ["-wal", "-shm"] {
        let sibling = side(&file, suffix);
        assert!(sibling.exists(), "{suffix} after check");
        assert_eq!(mode(&sibling), 0o600, "{suffix}");
    }
}

#[test]
fn migrate_writes_its_backup_before_it_changes_anything() {
    // The order is the deliverable, seen from the one thing that can stop it:
    // the copy. It is what the verb does first, it is what the verb names in its
    // answer, and a verb that cannot make it has changed nothing -- no schema,
    // no version, no row -- because it never got past the copy.
    let home = scratch("migrate-first");
    init(&home, false).unwrap();
    let file = db::path(&home);
    checkpoint(&file);
    // What the file holds once its log is in it: the bytes the verb is not
    // allowed to change, taken now and asked of again after every call below.
    let before = fs::read(&file).unwrap();
    let state = home.join("state");

    // The copy itself: beside the file it came from, at the store's mode, and a
    // database a reader can open.
    let line = migrate(&home, &state).unwrap();
    let value: serde_json::Value = serde_json::from_str(&line).unwrap();
    let copy = PathBuf::from(value["backup"].as_str().unwrap());
    assert_eq!(copy.parent().unwrap(), state);
    assert_eq!(
        mode(&copy),
        0o600,
        "a copy of the store is as unreadable as the store"
    );
    assert!(db::is_database(&copy));
    // A copy is a database of its own, compacted by VACUUM rather than handed
    // over page for page, so it is asked what it holds rather than how it reads.
    // It holds the database as the verb found it, and the store's own bytes are
    // unchanged by the verb that made it.
    let copy_conn = Connection::open(&copy).unwrap();
    assert_eq!(
        one::<i64>(&copy_conn, "PRAGMA user_version"),
        db::SCHEMA_VERSION,
        "the copy is of the database as it was"
    );
    assert_eq!(one::<i64>(&copy_conn, "SELECT count(*) FROM meta"), 6);
    drop(copy_conn);
    assert_eq!(
        fs::read(&file).unwrap(),
        before,
        "a verb with nothing to run wrote"
    );

    // The same name, asked for again: the copy that is there is not opened for
    // writing, and the verb says so. This is the failure that would otherwise
    // leave a person with one copy where they counted on two -- a copy of a
    // later database under the name of an earlier one -- and a host reaches it
    // on its own, with two verbs of one update in the same second. Chosen over a
    // directory that cannot be written because no mode of a directory says it: a
    // caller that may read one may write it too, whatever its bits say, and root
    // is not stopped by 0500.
    let err = migrate_at(&home, &state, 1_792_000_000)
        .expect_err("a backup whose name is taken is refused, not overwritten");
    assert_eq!(err.0, 78, "{}", err.1);
    assert!(err.1.starts_with("unreadable "), "{}", err.1);
    // The whole point: no schema was touched, so a reader still sees the version
    // the file came with, and the copy the verb could not make left nothing
    // behind to be mistaken for one.
    let conn = db::open_read_only(&file).unwrap();
    assert_eq!(one::<i64>(&conn, "PRAGMA user_version"), db::SCHEMA_VERSION);
    assert_eq!(
        one::<String>(&conn, "SELECT value FROM meta WHERE key = 'schema'"),
        "3"
    );
    drop(conn);
    assert_eq!(fs::read(&file).unwrap(), before, "a failed backup migrated");
    let mut left = fs::read_dir(&state)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect::<Vec<_>>();
    left.sort();
    let allowed: Vec<std::ffi::OsString> = [
        "tehas.db",
        "tehas.db-shm",
        "tehas.db-wal",
        &format!("tehas-3-{}.db", 1_792_000_000),
    ]
    .iter()
    .map(std::ffi::OsString::from)
    .collect();
    for name in &left {
        assert!(allowed.contains(name), "state/ holds {name:?}");
    }
    assert_eq!(
        fs::read(&copy).unwrap().len(),
        fs::metadata(&copy).unwrap().len() as usize,
        "the refused verb left the first copy as it wrote it"
    );

    // A minute later it is a different instant and a different file: an older
    // copy kept under the name of a newer one is the failure the time in the
    // name exists to prevent, and a verb that read the first copy back and
    // called it done would report a backup it never made.
    let again = migrate_at(&home, &state, 1_792_000_060).unwrap();
    let second = PathBuf::from(
        serde_json::from_str::<serde_json::Value>(&again).unwrap()["backup"]
            .as_str()
            .unwrap(),
    );
    assert_ne!(second, copy);
    assert!(copy.exists() && second.exists(), "both copies are kept");
    assert_eq!(fs::read(&file).unwrap(), before);

    // And a directory that is not the state/ of this home is refused before the
    // copy: the store keeps its copies where the 0700 of state/ keeps them
    // readable by nobody but the tehas user, and a person's directory is not the
    // store's to write one into.
    let elsewhere = scratch("migrate-elsewhere");
    let (code, err) = migrate(&home, &elsewhere).unwrap_err();
    assert_eq!(code, 78, "{err}");
    assert!(err.starts_with("unreadable "), "{err}");
    assert!(err.contains("state/"), "{err}");
    assert_eq!(fs::read(&file).unwrap(), before, "a refused backup wrote");
    assert!(fs::read_dir(&elsewhere).unwrap().next().is_none());
}

#[test]
fn migrate_writes_into_state_backup_made_0700_and_refuses_a_link_there() {
    // state/backup/ is the store's own: the verb makes it when it is not there,
    // keeps it 0700 when it is, and writes the copy into it rather than beside
    // the store.
    let home = scratch("migrate-backup-dir");
    init(&home, false).unwrap();
    let own = home.join("state").join("backup");
    let line = migrate(&home, &own).unwrap();
    let copy = PathBuf::from(
        serde_json::from_str::<serde_json::Value>(&line).unwrap()["backup"]
            .as_str()
            .unwrap(),
    );
    assert_eq!(copy.parent().unwrap(), own, "{line}");
    assert_eq!(mode(&own), 0o700);
    assert_eq!(mode(&copy), 0o600);
    assert!(db::is_database(&copy));
    fs::set_permissions(&own, fs::Permissions::from_mode(0o755)).unwrap();
    migrate_at(&home, &own, 1_792_000_060).unwrap();
    assert_eq!(mode(&own), 0o700, "a state/backup/ at 0755 is settled");

    // A link at that name is answered as the directory it points at, which is
    // not the store's.
    let home = scratch("migrate-backup-link");
    init(&home, false).unwrap();
    let elsewhere = scratch("migrate-backup-elsewhere");
    std::os::unix::fs::symlink(&elsewhere, home.join("state").join("backup")).unwrap();
    let (code, err) = migrate(&home, &home.join("state").join("backup")).unwrap_err();
    assert_eq!(code, 78, "{err}");
    assert!(err.starts_with("unreadable "), "{err}");
    assert!(fs::read_dir(&elsewhere).unwrap().next().is_none());
}

#[test]
fn migrate_takes_a_backup_dir_in_a_home_reached_through_a_link() {
    // A TEHAS_HOME whose path goes through a link, as a home moved to another
    // disk is: the store's own state/ and state/backup/ are still its own.
    let real = scratch("linked-home-real");
    let home = scratch("linked-home").join("tehas");
    std::os::unix::fs::symlink(&real, &home).unwrap();
    init(&home, false).unwrap();
    let own = home.join("state").join("backup");
    let line = migrate(&home, &own).unwrap();
    let copy = PathBuf::from(
        serde_json::from_str::<serde_json::Value>(&line).unwrap()["backup"]
            .as_str()
            .unwrap(),
    );
    assert_eq!(copy.parent().unwrap(), real.join("state").join("backup"));
    assert_eq!(mode(&real.join("state").join("backup")), 0o700);
    migrate_at(&home, &home.join("state"), 1_792_000_060).unwrap();
}

#[test]
fn migrate_brings_an_empty_file_to_the_schema_and_the_rows_it_lacks() {
    // An empty file is SQLite's own shape for nothing: no header, no tables and
    // a user_version of 0. `init` would make one at 1, so this is the state the
    // one migration of schema 1 exists for, made here rather than by a verb --
    // and made the way a host makes one, with the umask's mode on the file and
    // on the directory, which is the state `install.sh` and `libexec/update`
    // leave behind and the reason a migration settles both modes on the way in.
    let home = scratch("migrate-zero");
    fs::create_dir_all(home.join("state")).unwrap();
    fs::write(db::path(&home), b"").unwrap();
    fs::set_permissions(db::path(&home), fs::Permissions::from_mode(0o644)).unwrap();
    fs::set_permissions(home.join("state"), fs::Permissions::from_mode(0o755)).unwrap();
    let line = migrate(&home, &home.join("state")).unwrap();
    let value: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(value["from"], 0, "{line}");
    assert_eq!(value["to"], 3, "{line}");
    let file = db::path(&home);
    let conn = db::open_read_only(&file).unwrap();
    assert_eq!(one::<i64>(&conn, "PRAGMA user_version"), 3);
    assert_eq!(
        one::<String>(&conn, "SELECT value FROM meta WHERE key = 'schema'"),
        "3"
    );
    // The migration writes the rows a database at 1 holds and has no way to
    // get: the four the caller gave, and the instant.
    let got = one::<String>(
        &conn,
        "SELECT group_concat(key || '=' || value, ' ') FROM (SELECT * FROM meta ORDER BY key)",
    );
    assert_eq!(
        got.split(' ').collect::<Vec<_>>(),
        [
            "created_at=1792000000",
            "instance=tehas-tehas",
            "repo=example-owner/example-product",
            "schema=3",
            "tehas_commit=cb6912a",
            "tehas_version=v0.10.2",
        ]
    );
    // And it is a database of the store's own making: the file arrived at the
    // umask's mode and is one the store filled, so it leaves at 0600, and the
    // directory that holds it -- with its log, its backups and every approval
    // the host ever recorded -- leaves at 0700. With WAL on, a readable `state/`
    // is a readable copy of the store whatever the file's own bits say, so the
    // directory is the mode that matters and `migrate` is the verb that meets a
    // `state/` some `mkdir -p` made. The mode of the log and its shared-memory
    // sibling is not asked of a verb that has already closed its connection:
    // SQLite takes the mode of a file it makes itself from the umask and may
    // make the two only as the commit ends, and by the time this test looks the
    // last connection has closed and SQLite has taken both away. The test that
    // holds a connection open to ask is `the_wal_and_shm_siblings_take_the_file_s_mode`.
    drop(conn);
    assert_eq!(mode(&file), 0o600, "the migrated file");
    assert_eq!(mode(&home.join("state")), 0o700, "state/");
    // Every table of the schema is there, so the migrated file is one the other
    // verbs read. The count is what the schema holds, counted from the schema
    // the store was given rather than from a number written here: the schema of
    // the epic grows, and a table added to it is not a reason for this test to
    // say a migration went wrong.
    let report = check(&home).unwrap();
    let report: serde_json::Value = serde_json::from_str(&report).unwrap();
    assert_eq!(report["schema"], 3);
    assert_eq!(report["integrity"], "ok");
    assert_eq!(
        report["rows"].as_object().unwrap().len(),
        db::COUNTED_TABLES.len()
    );
    let conn = db::open_read_only(&file).unwrap();
    let tables: i64 = one(
        &conn,
        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
    );
    let indexes: i64 = one(
        &conn,
        "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND name NOT LIKE 'sqlite_%'",
    );
    drop(conn);
    assert_eq!(tables, db::COUNTED_TABLES.len() as i64);
    assert_eq!(indexes, db::COUNTED_INDEXES.len() as i64);
    // What a migration leaves in `state/`, and what each of the two copies
    // holds. A verb that changed the version leaves two files: the copy the
    // answer gives, which is the database as the verb found it and the copy that
    // had to exist before the step was allowed to run, and the copy of the file
    // under the version it came with, which is its twin. The reported name is
    // the one a person reaches for, so it is the one taken first: a verb that
    // could not write it has run nothing.
    let state = home.join("state");
    let copies: Vec<String> = fs::read_dir(&state)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("tehas-"))
        .collect();
    let found = format!("tehas-0-{}.db", 1_792_000_000);
    let made = format!("tehas-3-{}.db", 1_792_000_000);
    assert!(
        copies.contains(&found),
        "the copy of the file as it was found is not in state/: {copies:?}"
    );
    assert!(
        copies.contains(&made),
        "the copy the verb reports is not in state/: {copies:?}"
    );
    // The copy the verb reports is a database of its own, of the version the
    // answer says the store ends at, and it is the state before the step: at 0,
    // with no table of the schema in it. That is what makes it the restore
    // point. The store itself is what the step made, and the test of the copy
    // that could not be written is what holds the verb to the order.
    let copy = PathBuf::from(value["backup"].as_str().unwrap());
    assert_eq!(
        copy,
        db::backup_name(&state, 3, 1_792_000_000),
        "{line} reports a copy other than the one taken first"
    );
    assert!(copy.exists(), "{line} names a copy that is not there");
    let after = Connection::open(&copy).unwrap();
    assert_eq!(
        one::<i64>(&after, "PRAGMA user_version"),
        0,
        "the reported copy is not the state before the step"
    );
    assert_eq!(
        one::<i64>(
            &after,
            "SELECT count(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'",
        ),
        0,
        "the reported copy holds part of the schema the step laid"
    );
    drop(after);
    // The other copy is the same bytes under the version the file came with:
    // SQLite's VACUUM INTO writes a header for a file of no bytes, so it is a
    // copy of an empty database rather than a copy of nothing.
    let found = db::backup_name(&state, 0, 1_792_000_000);
    assert!(
        found.exists(),
        "the copy of the file as it was found is missing"
    );
    let was = Connection::open(&found).unwrap();
    assert_eq!(one::<i64>(&was, "PRAGMA user_version"), 0);
    assert_eq!(
        one::<i64>(
            &was,
            "SELECT count(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'",
        ),
        0,
        "the copy of the file as it was found holds no part of the schema"
    );
}

#[test]
fn a_reported_copy_that_cannot_be_written_leaves_the_step_unrun() {
    // The name the answer carries is the name the caller opens, so it is the
    // name that must be written before the step can commit -- not after it.
    // Two verbs of one update land in the same second, so `tehas-3-<now>.db`
    // can be held by a file the verb did not make, and the verb must then
    // change nothing at all.
    let home = scratch("migrate-after");
    fs::create_dir_all(home.join("state")).unwrap();
    fs::write(db::path(&home), b"").unwrap();
    let state = home.join("state");
    let foreign = db::backup_name(&state, 3, 1_792_000_000);
    fs::write(&foreign, b"not a backup").unwrap();

    let (code, err) = migrate(&home, &state).unwrap_err();
    assert_eq!(code, 78, "{err}");
    assert!(err.starts_with("unreadable "), "{err}");
    assert!(err.contains("cannot make the backup"), "{err}");

    // The store is the empty file it was: no schema, no version, no rows.
    let conn = Connection::open(db::path(&home)).unwrap();
    assert_eq!(
        one::<i64>(&conn, "PRAGMA user_version"),
        0,
        "a copy the verb could not make still migrated the store"
    );
    assert_eq!(
        one::<i64>(
            &conn,
            "SELECT count(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'",
        ),
        0,
        "a copy the verb could not make still laid the schema"
    );
    drop(conn);
    // The foreign file is untouched: the verb neither opened it for writing nor
    // replaced it with a copy of its own.
    assert_eq!(fs::read(&foreign).unwrap(), b"not a backup");
}

#[test]
fn migrate_reports_the_three_fields_and_a_database_at_2_stays_at_2() {
    let home = scratch("migrate-json");
    init(&home, false).unwrap();
    let state = home.join("state");
    let line = migrate(&home, &state).unwrap();
    // One object, one line, valid JSON, and the fields the verb states: api
    // first as every answer of tehas writes it, then the verb and its ok, then
    // the three of the migration.
    assert_eq!(line.lines().count(), 1);
    let value: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert!(
        line.starts_with("{\"api\":1,\"verb\":\"migrate\",\"ok\":true"),
        "{line}"
    );
    assert_eq!(value["api"], 1, "{line}");
    assert_eq!(value["verb"], "migrate", "{line}");
    assert_eq!(value["ok"], true, "{line}");
    assert_eq!(value["from"], 3, "{line}");
    assert_eq!(value["to"], 3, "{line}");
    assert!(value["backup"].is_string(), "{line}");
    for field in ["api", "verb", "ok", "from", "to", "backup"] {
        assert!(value.get(field).is_some(), "{field} is missing from {line}");
    }
    // Already at 1: nothing to run, and the same answer shape. The backup is
    // still written, because the verb promises a copy before it touches the
    // file and has no way to know it will not need it until it has looked.
    assert_eq!(value["from"], 3, "{line}");
    assert_eq!(value["to"], 3, "{line}");
    assert!(db::is_database(&PathBuf::from(
        value["backup"].as_str().unwrap()
    )));
    let file = db::path(&home);
    checkpoint(&file);
    let before = fs::read(&file).unwrap();
    // The same instant again: the copy of the first call is on the disk under
    // that name, and a verb that read it back and called it done would report a
    // backup it did not make. The name is made afresh from the instant and the
    // name that is taken is asked for by name, so a run of the verb twice in one
    // second says so rather than quietly answering with the older file.
    let (code, err) = migrate(&home, &state).unwrap_err();
    assert_eq!(
        code, 78,
        "a second call in the same second overwrote a copy: {code} {err}"
    );
    assert!(err.starts_with("unreadable "), "{err}");
    // A minute later it is a different instant and a different file, and the
    // store itself is the file it was: the verb had nothing to run.
    let again = migrate_at(&home, &state, 1_792_000_060).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&again).unwrap()["from"],
        3,
        "{again}"
    );
    assert_eq!(
        fs::read(&file).unwrap(),
        before,
        "a verb with nothing to run wrote"
    );
    let copies: Vec<_> = fs::read_dir(&state)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .filter(|n| n.to_string_lossy().starts_with("tehas-"))
        .collect();
    assert_eq!(copies.len(), 2, "two calls, two copies: {copies:?}");
}

#[test]
fn migrate_refuses_a_schema_it_has_no_step_for_and_a_future_one() {
    // A version between 0 and what this binary writes is not a database this
    // verb can bring forward, and it is not one it may overwrite with schema 1
    // either: the file keeps its bytes and the verb says which version it gave
    // up on. The backup is still there, because the copy is what a person needs
    // most when the step cannot be run.
    let home = scratch("migrate-unknown");
    init(&home, false).unwrap();
    let file = db::path(&home);
    let state = home.join("state");
    {
        let conn = Connection::open(&file).unwrap();
        conn.pragma_update(None, "user_version", 4).unwrap();
    }
    checkpoint(&file);
    let before = fs::read(&file).unwrap();
    let (code, err) = migrate(&home, &state).unwrap_err();
    assert_eq!(code, 78, "{err}");
    // The version is read on the connection, and a version above what this
    // binary writes is the one class every way into the store already gives:
    // `future`, met before the copy, so a database this command must not touch
    // is not copied either.
    assert!(err.starts_with("future "), "{err}");
    assert_eq!(fs::read(&file).unwrap(), before);
    let copies: Vec<_> = fs::read_dir(&state)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .filter(|n| n.to_string_lossy().starts_with("tehas-"))
        .collect();
    assert!(
        copies.is_empty(),
        "a future database was copied: {copies:?}"
    );
    // A version this binary does not write and does not exceed -- a step of a
    // later release, seen by a command that has no step for it -- is reached by
    // the copy and refused by the step: the file keeps its bytes, and the copy
    // the verb did make is what a person restores from. The name in the refusal
    // is the copy that exists, not one the verb hoped to write -- and it exists
    // twice, because the copy the answer would have carried is taken first, under
    // the version the step was going to write, and the copy of the file under its
    // own version follows it. Both predate the refusal. (Two below 1 is
    // the shape; nothing in the world of schema 1 writes one, so it is made
    // here, and a version above 1 is the `future` case just above.)
    {
        let conn = Connection::open(&file).unwrap();
        conn.pragma_update(None, "user_version", -1).unwrap();
    }
    checkpoint(&file);
    let before = fs::read(&file).unwrap();
    let (code, err) = migrate(&home, &state).unwrap_err();
    assert_eq!(code, 78, "{err}");
    assert!(err.starts_with("corrupt "), "{err}");
    assert!(err.contains("no migration from schema -1"), "{err}");
    assert!(err.contains(&format!("{}", db::SCHEMA_VERSION)), "{err}");
    assert_eq!(fs::read(&file).unwrap(), before);
    let copies: Vec<_> = fs::read_dir(&state)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .filter(|n| n.to_string_lossy().starts_with("tehas-"))
        .collect();
    assert_eq!(
        copies.len(),
        2,
        "the copies of the refused database: {copies:?}"
    );
    assert!(
        copies
            .iter()
            .any(|n| n.to_string_lossy().starts_with("tehas--1-")),
        "the copy of the file as it was found: {copies:?}"
    );
    assert!(
        copies
            .iter()
            .any(|n| n.to_string_lossy().starts_with("tehas-3-")),
        "the copy the verb reports: {copies:?}"
    );
    // A file with tables and no version is the same case seen from the other
    // side: 0 with rows in it is not an empty file and is not migrated over.
    let home = scratch("migrate-tables-no-version");
    init(&home, false).unwrap();
    let file = db::path(&home);
    {
        let conn = Connection::open(&file).unwrap();
        conn.pragma_update(None, "user_version", 0).unwrap();
    }
    checkpoint(&file);
    let before = fs::read(&file).unwrap();
    let (code, err) = migrate(&home, &home.join("state")).unwrap_err();
    assert_eq!(code, 78, "{err}");
    assert!(err.starts_with("corrupt "), "{err}");
    assert_eq!(fs::read(&file).unwrap(), before);
}

#[test]
fn migrate_waits_on_a_lock_the_way_a_write_verb_does() {
    // A migration is a write: it takes the write lock when it opens, so a
    // caller that already holds it is met before the verb reads a version or
    // copies a file. The holder waits for nothing, so the verb cannot win the
    // race and must report it: `busy` is exit 1, and no copy was made.
    let home = scratch("migrate-busy");
    init(&home, false).unwrap();
    let state = home.join("state");
    let mut holder = Connection::open(db::path(&home)).unwrap();
    holder.pragma_update(None, "busy_timeout", 0).unwrap();
    let tx = holder
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    tx.execute("INSERT INTO meta (key, value) VALUES ('held', '1')", [])
        .unwrap();
    let (code, err) = migrate(&home, &state).unwrap_err();
    assert_eq!(code, 1, "{err}");
    assert!(err.starts_with("busy "), "{err}");
    let copies: Vec<_> = fs::read_dir(&state)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .filter(|n| n.to_string_lossy().starts_with("tehas-"))
        .collect();
    assert!(
        copies.is_empty(),
        "a locked database was copied: {copies:?}"
    );
    drop(tx);
    drop(holder);
    // Once the lock is gone the same call answers, and the copy it was asked
    // for is the file the verb names.
    let line = migrate(&home, &state).unwrap();
    let copy = PathBuf::from(
        serde_json::from_str::<serde_json::Value>(&line).unwrap()["backup"]
            .as_str()
            .unwrap(),
    );
    assert!(copy.exists(), "{line} names a copy that is not there");
    assert!(db::is_database(&copy));
}

#[test]
fn the_arguments_migrate_accepts_are_the_ones_it_states() {
    let dir = "/tmp/store-migrate-dir";
    let full: &[&str] = &[
        "--backup",
        dir,
        "--version",
        "v0.10.2",
        "--commit",
        "cb6912a",
        "--instance",
        "tehas-tehas",
        "--repo",
        "example-owner/example-product",
    ];
    assert_eq!(
        parse(&migrate_args(full)),
        Some(Verb::Migrate {
            backup: PathBuf::from(dir),
            settings: settings(),
        })
    );
    let mut jsoned = vec!["--json"];
    jsoned.extend_from_slice(full);
    assert!(matches!(
        parse(&migrate_args(&jsoned)),
        Some(Verb::Migrate { .. })
    ));
    for bad in [
        vec![],
        vec!["--backup"],
        vec!["--backup", dir, "--backup", dir],
        vec!["--backup", "relative/dir"],
        vec!["--backup", "--json"],
        vec!["--backup", dir, "--json", "--json"],
        vec!["--backup", dir, "--force"],
        vec!["--backup", dir, "extra"],
        vec!["--backup", dir, "--version", "v0.10.2"],
        vec!["--backup", dir, "--repo", "not a repo"],
        vec!["--backup", dir, "--commit", "zzzzzzz"],
    ] {
        assert_eq!(parse(&migrate_args(&bad)), None, "{bad:?}");
    }
}

/// The arguments of migrate, with these flags.
fn migrate_args(args: &[&str]) -> Vec<String> {
    let mut v = vec!["migrate".to_string()];
    v.extend(args.iter().map(|s| s.to_string()));
    v
}

fn migrate(home: &Path, dir: &Path) -> Result<String, (u8, String)> {
    migrate_at(home, dir, 1_792_000_000)
}

fn migrate_at(home: &Path, dir: &Path, now: i64) -> Result<String, (u8, String)> {
    execute(
        home,
        &Verb::Migrate {
            backup: dir.to_path_buf(),
            settings: settings(),
        },
        now,
    )
}

#[test]
fn an_unknown_verb_or_argument_is_64() {
    let home = scratch("usage");
    for args in [
        &[][..],
        &["migrate"][..],
        &["state-audit"][..],
        &["cycle-heartbeat", "12"][..],
        &["check", "--json", "--json"][..],
        &["check", "--force"][..],
        &["init", "--json", "--json"][..],
        &["init", "--force", "--force"][..],
        &["init", "--version"][..],
        &["init", "--repo", "not a repo"][..],
        &["init", "--repo", "two/slashes/here"][..],
        &["init", "--commit", "zzzzzzz"][..],
        &["init", "--instance", ""][..],
        &["init", "extra"][..],
        &["--json"][..],
    ] {
        assert_eq!(
            parse(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>()),
            None,
            "{args:?}"
        );
    }
    // Nothing was written by a call that stopped at its arguments.
    assert!(!home.join("state").exists());
}

/// The arguments of init, with these settings.
fn init_args(extra: &[&str]) -> Vec<String> {
    let mut v = vec!["init".to_string()];
    v.extend(extra.iter().map(|s| s.to_string()));
    v
}

#[test]
fn the_arguments_init_accepts_are_the_ones_it_states() {
    let _home = scratch("accepted");
    assert_eq!(parse(&init_args(&[])), None, "the settings are required");
    assert!(
        parse(&init_args(&["--repo", "a/b"])).is_none(),
        "one setting is not enough"
    );
    let full: &[&str] = &[
        "--version",
        "v0.10.2",
        "--commit",
        "cb6912a",
        "--instance",
        "tehas-tehas",
        "--repo",
        "example-owner/example-product",
    ];
    assert_eq!(
        parse(&init_args(full)),
        Some(Verb::Init {
            force: false,
            settings: settings()
        })
    );
    let mut forced = vec!["--force", "--json"];
    forced.extend_from_slice(full);
    assert_eq!(
        parse(&init_args(&forced)),
        Some(Verb::Init {
            force: true,
            settings: settings()
        })
    );
    assert_eq!(parse(&init_args(&["--json"])), None);
    assert_eq!(
        parse(&["check".to_string(), "--json".to_string()]),
        Some(Verb::Check)
    );
}

#[test]
fn the_schema_holds_every_table_of_the_spec_with_its_checks() {
    let home = scratch("schema");
    init(&home, false).unwrap();
    let conn = Connection::open(db::path(&home)).unwrap();
    // A value outside a CHECK is refused rather than stored.
    for (sql, what) in [
        (
            "INSERT INTO issues (issue, kind, kind_at, checked_at) VALUES (1, 'thing', 1, 1)",
            "issues.kind",
        ),
        (
            "INSERT INTO cycles (issue, started_at, started_by, how) VALUES (1, 1, 'a', 'because')",
            "cycles.how",
        ),
        (
            "INSERT INTO questions (id, issue, attempt, kind, posted_at) VALUES (1, 1, 1, 'blockers', 1)",
            "questions.kind",
        ),
        (
            "INSERT INTO improve_cap (id, cap, set_at) VALUES (1, 9, 1)",
            "improve_cap.cap",
        ),
        (
            "INSERT INTO pause_flag (id, at, reason, by_account) VALUES (2, 1, 'r', 'a')",
            "pause_flag holds at most one row",
        ),
    ] {
        assert!(conn.execute(sql, []).is_err(), "{what} accepted a bad row");
    }
    // foreign_keys is on for the connection, so a cycle with no issue goes no
    // further than the key.
    assert!(conn
        .execute(
            "INSERT INTO cycles (issue, started_at, started_by, how) VALUES (7, 1, 'a', 'label')",
            []
        )
        .is_err());
    conn.execute(
        "INSERT INTO issues (issue, kind, kind_at, checked_at) VALUES (7, 'issue', 1, 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO cycles (issue, started_at, started_by, how) VALUES (7, 1, 'a', 'label')",
        [],
    )
    .unwrap();
    // Deleting the issue takes the cycle's row with it, which is what the keys
    // of section 3 mean.
    conn.execute("DELETE FROM issues WHERE issue = 7", [])
        .unwrap();
    assert_eq!(
        one::<i64>(&conn, "SELECT count(*) FROM cycles WHERE issue = 7"),
        0
    );
}

#[test]
fn the_open_connections_carry_the_pragmas_a_reader_must_not_forget() {
    // foreign_keys is per-connection and never stored in the file, so a
    // connection the store opened has it set and a bare one does not.
    let home = scratch("pragmas");
    init(&home, false).unwrap();
    let store = db::open(&db::path(&home), db::Purpose::Read).unwrap();
    assert_eq!(one::<i64>(&store, "PRAGMA foreign_keys"), 1);
    assert_eq!(one::<i64>(&store, "PRAGMA synchronous"), 2);
    // The other half of the same fact: the pragma is the store's to set and
    // nobody else's, so what a connection it did not open would see is
    // settled by the file's own bytes rather than by a live connection. A new
    // connection of a database that still has a -wal is handed the state the
    // last one left there, which would answer for the store rather than for
    // the file.
    let bytes = fs::read(db::path(&home)).unwrap();
    assert!(
        !bytes.windows(13).any(|w| w == b"foreign_keys="),
        "the pragma belongs to the connection, not to the file"
    );
    // And what the key buys: through the store's own connection a cycle with no
    // issue of its own is refused, so the line in prepare is not decoration.
    let store = db::open(&db::path(&home), db::Purpose::Write).unwrap();
    assert!(store
        .execute(
            "INSERT INTO cycles (issue, started_at, started_by, how) VALUES (4242, 1, 'a', 'label')",
            []
        )
        .is_err());
}

#[test]
fn an_open_refuses_a_file_that_is_not_a_database_at_all() {
    // The check every open runs is what turns SQLite's answer into the word
    // `corrupt`: a file that is not a database is refused before any verb reads
    // a byte of it, and is left where it is.
    let home = scratch("not-a-database");
    fs::create_dir_all(home.join("state")).unwrap();
    let file = db::path(&home);
    fs::write(&file, b"#!/bin/sh\necho not a database\n".repeat(60)).unwrap();
    for purpose in [db::Purpose::Read, db::Purpose::Write] {
        let err = db::open(&file, purpose).unwrap_err();
        assert_eq!(err.word(), "corrupt", "{err:?}");
    }
    assert_eq!(
        fs::read(&file).unwrap(),
        b"#!/bin/sh\necho not a database\n".repeat(60),
        "the refused file is not touched"
    );
}

#[test]
fn every_way_into_the_store_refuses_a_malformed_image() {
    // missing, corrupt, unreadable and future are the answer of every way in,
    // not of `check` alone: an open is what reads the file, so a verb cannot
    // get further than it. The truncated file stands for the malformed image.
    let home = scratch("all-opens-refuse");
    init(&home, false).unwrap();
    drop(seed(&home));
    let file = db::path(&home);
    checkpoint(&file);
    let bytes = fs::read(&file).unwrap();
    fs::write(&file, &bytes[..bytes.len() - 4096]).unwrap();
    for purpose in [db::Purpose::Read, db::Purpose::Write] {
        let err = db::open(&file, purpose).unwrap_err();
        assert_eq!(err.word(), "corrupt", "{err:?}");
    }
    let err = db::open_read_only(&file).unwrap_err();
    assert_eq!(err.word(), "corrupt", "{err:?}");
    assert_eq!(
        fs::read(&file).unwrap().len(),
        bytes.len() - 4096,
        "the file is not repaired or moved"
    );
}

#[test]
fn the_command_s_exit_and_its_line_carry_the_class_of_the_failure() {
    // The unit tests above call the verbs; this is the shape of what a caller
    // gets: the word as the first word of the line on standard error, the exit
    // the spec's table gives it, and one object on standard output for a verb
    // that answered. It is what line() builds, so it is tested here rather than
    // by watching a process.
    let home = scratch("exit-codes");
    init(&home, false).unwrap();
    let file = db::path(&home);

    // The success of check, and the object's first field.
    let ok = check(&home).unwrap();
    assert!(ok.starts_with("{\"api\":1,\"verb\":\"check\""), "{ok}");
    assert_eq!(ok.lines().count(), 1, "one object is one line");

    // The four classes, each with its word first and 78 behind it.
    let cases: [(db::DbError, u8, &str); 4] = [
        (db::DbError::Missing, 78, "missing state/tehas.db"),
        (
            db::DbError::Corrupt("disk image".into()),
            78,
            "corrupt the database: disk image",
        ),
        (
            db::DbError::Unreadable("no permission".into()),
            78,
            "unreadable the database: no permission",
        ),
        (
            db::DbError::Future(2),
            78,
            "future the database: the database has schema 2",
        ),
    ];
    for (err, code, want) in cases {
        assert_eq!(crate::line(err), (code, want.to_string()));
    }
    // A lock is not one of the four: it is exit 1 with its own word.
    let (code, msg) = crate::line(db::DbError::Busy);
    assert_eq!(code, 1);
    assert!(msg.starts_with("busy "), "{msg}");

    // The same words through the verbs themselves, at files that are really so.
    let missing = scratch("no-file");
    let (code, err) = check(&missing).unwrap_err();
    assert_eq!((code, err.as_str()), (78, "missing state/tehas.db"));
    drop(file);
}

// The WAL siblings are named by appending to the database's name.
fn side(file: &Path, suffix: &str) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Rows enough that the file has pages of its own to damage or truncate: at a
/// few dozen the whole table still sits inside the single page it is reached
/// from, and a file with nothing in it but its schema has no page of a table at
/// all. The connection is returned rather than dropped, because SQLite removes
/// the -wal and -shm only when the last connection closes and the rows are the
/// file's data until the next checkpoint.
const SEED_ROWS: i64 = 1000;

fn seed(home: &Path) -> Connection {
    let conn = db::open(&db::path(home), db::Purpose::Write).unwrap();
    for n in 0..SEED_ROWS {
        conn.execute(
            "INSERT INTO issues (issue, kind, kind_at, title, checked_at) VALUES (?1, 'issue', ?2, ?3, ?2)",
            rusqlite::params![n, 1_792_000_000_i64, format!("the issue number {n} and its title")],
        )
        .unwrap();
    }
    conn
}

/// Puts the rows in the file proper, so what is on disk is what a reader meets.
/// A file the store will not open -- the one of the future case, whose
/// user_version is ahead of this binary -- is asked in the plain way instead,
/// which is enough: the caller has already decided what is wrong with it and
/// only wants the bytes of it on disk rather than in a log beside it.
fn checkpoint(file: &Path) {
    match db::open(file, db::Purpose::Write) {
        Ok(conn) => conn
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
            .unwrap(),
        Err(_) => {
            let conn = Connection::open(file).unwrap();
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
                .unwrap();
        }
    }
    // When the last connection to a database closes with nothing in its log,
    // SQLite checkpoints and removes both siblings itself; there is nothing
    // left to take away, so this only clears what is still there.
    for suffix in ["-wal", "-shm"] {
        let sibling = side(file, suffix);
        if sibling.exists() {
            fs::remove_file(&sibling).unwrap();
        }
    }
}
