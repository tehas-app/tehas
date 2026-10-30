//! The store against scratch homes: the mode of the file, the refusals that
//! come before SQLite is asked anything, the four classes of failure, and what
//! `init` and `check` answer.

use crate::db;
use crate::live;
use crate::review_issue;
use crate::{execute, parse, ReviewFile, ReviewIssue, Settings, Verb};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

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
        "schema=9",
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
    assert!(err.contains("schema 10"), "{err}");
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
    assert!(line.contains("\"schema\":9"), "{line}");
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
fn the_installed_bounds_are_the_ones_the_spec_states() {
    // As every helper that makes a home does: the last two assertions are about
    // the wait this thread is under, and a process that never asked keeps the
    // installed pair, which the test below shows.
    crate::verbs_tests::short_bounds();
    // The tests run with a short busy and lock bound of their own, so the
    // installed program's pair is pinned here rather than by a test that waits
    // it out: five seconds for a connection that writes, a second for one that
    // reads, and five for the publisher's file lock.
    assert_eq!(db::BUSY_TIMEOUT, Duration::from_secs(5));
    assert_eq!(db::READ_WAIT, Duration::from_secs(1));
    assert_eq!(live::LOCK_WAIT, Duration::from_secs(5));
    // The short pair is what the tests above actually wait: long enough that
    // the millisecond polling is seen, short enough not to be the installed
    // bound, and inside it by a margin no slow host closes. The tests' own
    // thread asked for it through `verbs_tests::short_bounds`; a thread that did
    // not, as the test below shows, still waits the pair above.
    assert!(db::TEST_BUSY_TIMEOUT < db::BUSY_TIMEOUT);
    assert!(db::TEST_READ_WAIT < db::TEST_BUSY_TIMEOUT);
    assert!(live::LOCK_WAIT > db::TEST_BUSY_TIMEOUT);
    assert_eq!(db::write_bound(), db::TEST_BUSY_TIMEOUT);
    assert_eq!(db::read_bound(), db::TEST_READ_WAIT);
}

#[test]
fn publish_waits_the_lock_bound_the_spec_states() {
    // The constant above is only half the pin: `live::take_lock` reads its bound
    // from `db::lock_wait`, so a thread that never asked for the short pair has
    // to be answered with the five seconds publish really waits. A test that
    // held state/publish.lock would have to sit through that five seconds, which
    // is the cost this issue set out to remove; the bound is proved here instead,
    // and the short pair's effect on the same read is shown by
    // `live_tests::a_held_publish_lock_is_busy`.
    assert_eq!(db::lock_wait(), live::LOCK_WAIT);
    assert_eq!(db::lock_wait(), Duration::from_secs(5));
}

#[test]
fn the_bound_a_connection_gets_is_the_installed_constant() {
    // On a thread that never asked for the short pair, the bound each busy handler
    // takes and the constant it comes from are one value, in the build the
    // assertion is compiled into. The child process of the test below reports its
    // own write pair the same way, so neither end of that comparison is a copy the
    // other has to be kept in step with by hand. The reader's bound is read here
    // too: `prepare` is the only place it is handed to SQLite, and a pin of
    // `READ_WAIT` alone would not notice the handler being given another value.
    let (bound, constant) = db::bound_check_answer(db::Purpose::Write);
    assert_eq!(bound, constant);
    assert_eq!(bound, db::BUSY_TIMEOUT);
    let (bound, constant) = db::bound_check_answer(db::Purpose::Read);
    assert_eq!(bound, constant);
    assert_eq!(bound, db::READ_WAIT);
}

#[test]
fn a_thread_that_asked_shortens_the_bound_a_connection_gets() {
    // The short pair moves the bound a connection is given, not only the constant
    // a test compares an elapsed time with: on a thread that asked, the bound is
    // the short one while the constant stays where the spec puts it, and the
    // difference is exactly what the bound check reports.
    crate::verbs_tests::short_bounds();
    let (bound, constant) = db::bound_check_answer(db::Purpose::Write);
    assert_eq!(bound, db::TEST_BUSY_TIMEOUT);
    assert_eq!(constant, db::BUSY_TIMEOUT);
    assert_ne!(bound, constant);
}

#[test]
fn a_thread_that_asked_shortens_the_publish_bound_too() {
    // The three reads of the short pair go together: the two waits of the
    // database and the file lock of the live view are shortened by one call, so
    // a test cannot end up waiting the installed lock bound between two waits it
    // shortened. The thread that did not ask is pinned above, where a hoist of
    // `db::lock_wait` out of `live::take_lock`'s retry loop would leave this one
    // and the test above agreeing at the installed pair while publish gave up at
    // another.
    crate::verbs_tests::short_bounds();
    assert_eq!(db::lock_wait(), db::TEST_BUSY_TIMEOUT);
}

#[test]
fn a_store_the_tests_did_not_shorten_waits_the_installed_bounds() {
    // The short pair is a setting of the test binary and of nothing else: a
    // process that never asked for it waits out `BUSY_TIMEOUT` on a lock it
    // cannot get and says `busy` when that is gone. The store
    // answers `--test-store-bound-check` with the wait it met, so the proof is
    // made by the program itself rather than by a constant, and the short pair
    // cannot quietly shorten the installed one. A thread of this binary that
    // never called `db::install_test_bounds` is the same case; the store is the
    // stronger of the two, because it is what a host runs.
    let store = std::env::var("CARGO_BIN_EXE_store")
        .map(PathBuf::from)
        .unwrap_or_else(|_| store_binary());
    let writer_met_the_bound = || {
        let home = crate::verbs_tests::scratch("installed-bounds");
        init(&home, false).unwrap();
        // The holder waits for nothing, so the store cannot win the race and has
        // to meet its bound: a write lock held by another connection is what a
        // connection that writes wants.
        let holder = Connection::open(db::path(&home)).unwrap();
        holder
            .pragma_update(None, "locking_mode", "EXCLUSIVE")
            .unwrap();
        holder
            .execute("INSERT INTO meta (key, value) VALUES ('held', '1')", [])
            .unwrap();
        let child = std::process::Command::new(&store)
            .args([crate::BOUND_CHECK])
            .arg(db::path(&home))
            .env_clear()
            .output()
            .unwrap_or_else(|e| panic!("{}: {e}", store.display()));
        drop(holder);
        assert!(
            child.status.success(),
            "{:?} {}",
            child.status,
            String::from_utf8_lossy(&child.stderr)
        );
        let said: serde_json::Value = serde_json::from_slice(&child.stdout).unwrap();
        assert_eq!(said["busy"], true, "{said}");
        (
            said["waited_ms"].as_i64().unwrap(),
            said["bound_ms"].as_i64().unwrap(),
            said["installed_ms"].as_i64().unwrap(),
            home,
        )
    };

    // Five seconds, where the tests' own bound is half a second: this is the
    // pair of the program a host runs and not the short one. The child names the
    // bound it applied and the constant that bound comes from, both read off the
    // child's own compilation, so the three are one quantity rather than three
    // that have to be kept in step: a wait that fell short of the bound the child
    // was given fails, a bound that drifted from the child's constant fails, and
    // so does a child whose constant no longer matches the one the parent pins.
    let (waited, bound, constant, home) = writer_met_the_bound();
    assert_eq!(
        bound,
        db::BUSY_TIMEOUT.as_millis() as i64,
        "the child did not wait the bound its constants state"
    );
    assert_eq!(
        constant,
        db::BUSY_TIMEOUT.as_millis() as i64,
        "the child's bound and the parent's constant are not the same value"
    );
    assert!(
        waited >= bound,
        "a writer waited {waited} ms, not its bound of {bound} ms"
    );
    assert!(check(&home).is_ok());

    // The reader's bound is the business of
    // `a_reader_waits_a_second_for_a_lock_and_not_the_writers_five`, which
    // compares a reader's wait with both bounds of the run: it is a second in
    // what a host runs, and the short pair's two hundred milliseconds here.
}

/// The store of this build, for the one test that must run the program rather
/// than call into it. cargo names it for an integration test; the unit tests of
/// the binary find it beside the harness it was built with.
fn store_binary() -> PathBuf {
    let deps = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let here = deps.join("store");
    if here.exists() {
        return here;
    }
    deps.parent().unwrap().join("store")
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
    // The store has to meet the writer's bound rather than beat the holder, so
    // this thread asks for the short pair.
    crate::verbs_tests::short_bounds();
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
    let waited = asked.elapsed();
    // The bound of this run: the handler is asked again until the wait it was
    // given is gone, and `the_installed_bounds_are_the_ones_the_spec_states`
    // pins the pair the program a host runs is given.
    assert!(
        waited >= db::write_bound(),
        "waited {waited:?} on a bound of {:?}",
        db::write_bound()
    );
    drop(tx);
    // Once the lock is gone the same call answers, so the word is the lock's
    // and not the file's.
    assert!(db::open_write(&db::path(&home)).is_ok());
    assert!(check(&home).is_ok());
}

#[test]
fn a_reader_waits_a_second_for_a_lock_and_not_the_writers_five() {
    // The reader has to meet its own bound rather than beat the holder, so the
    // thread asks for the short pair; the assertion below is between the two
    // bounds of the run either way.
    crate::verbs_tests::short_bounds();
    //
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
    // A reader waits its own bound and not the writer's, whichever pair the run
    // is under.
    assert!(
        waited >= db::read_bound() && waited < db::write_bound(),
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
fn a_call_that_meets_no_lock_counts_no_retry() {
    // The count the concurrency test reads is of waits on a lock, not of
    // connections: `check` opens the store, reads it and closes it, and none of
    // that is a wait, so the debug line it would print is no line at all.
    crate::verbs_tests::short_bounds();
    let home = scratch("busy-count-clean");
    init(&home, false).unwrap();
    check(&home).unwrap();
    assert_eq!(db::busy_retries(), 0);
}

#[test]
fn a_call_that_waits_on_a_lock_counts_the_wait_it_made() {
    // The other half of the gate: the handler's counter is what the notice is
    // judged against, so a wait that happened must be visible in it and a call
    // that met nothing must leave it where it was. The holder asks for nothing,
    // so the reader meets its bound and the count is not a race about whether
    // the lock came free.
    crate::verbs_tests::short_bounds();
    let home = scratch("busy-count-held");
    init(&home, false).unwrap();
    let holder = crate::verbs_tests::exclusive_lock(&home);
    let err = db::open_read_only(&db::path(&home)).unwrap_err();
    drop(holder);
    assert_eq!(err, db::DbError::Busy, "{err:?}");
    assert!(
        db::busy_retries() >= 1,
        "a wait on a lock left no count behind"
    );
}

#[test]
fn the_busy_report_is_one_line_per_wait_and_says_nothing_by_default() {
    // The variable is the whole of the gate: with no value, which is what every
    // caller of the store gives it, a thread that waited on a lock prints
    // nothing. The line is asked for by the test's own environment and read by
    // no caller, so a host's store is silent whatever it holds.
    let home = scratch("busy-report");
    init(&home, false).unwrap();
    let store = store_binary();
    let held = |given: &str| {
        let mut cmd = std::process::Command::new(&store);
        cmd.args([crate::BOUND_CHECK, db::path(&home).to_str().unwrap()])
            .env_clear()
            .env("TEHAS_HOME", &home)
            .env(db::TEST_BUSY_REPORT, given);
        let out = cmd
            .output()
            .unwrap_or_else(|e| panic!("{}: {e}", store.display()));
        assert_eq!(out.status.code(), Some(0), "{}: {given}", store.display());
        let said: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        // The lock was really held: `--test-store-bound-check` answers whether
        // it met `busy` on its way to the write lock. Without this the silence
        // of no lock and the silence of the variable look the same.
        assert_eq!(said["busy"], true, "{said}");
        String::from_utf8_lossy(&out.stderr).to_string()
    };
    // Nothing asked for: the store is silent, as it is on a host. The lock is
    // held for this call too, so silence is the variable's doing and not the
    // absence of a wait.
    let holder = crate::verbs_tests::exclusive_lock(&home);
    let quiet = held("0");
    let said = held("1");
    drop(holder);
    assert!(
        !said.is_empty(),
        "a wait met printed nothing, though one line was asked for"
    );
    assert!(
        quiet.is_empty(),
        "a wait printed a line that was not asked for: {quiet:?}"
    );
    for line in said.lines() {
        let seen = "store debug: busy retry ";
        assert!(
            line.strip_prefix(seen)
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())),
            "{line:?} is not the notice"
        );
    }
    // A wait of the bound polls a thousand times, and a line per poll would be a
    // thousand lines: a bound of 1 asks for the line of each wait and no more.
    assert!(
        said.lines().count() < 10,
        "the notice is per poll, not per wait: {said:?}"
    );
    // A value that is no count is said so, and loudly: a test that mistyped the
    // value would else read the silence of no wait as the silence of no lock.
    let bad = std::process::Command::new(&store)
        .args([crate::BOUND_CHECK, db::path(&home).to_str().unwrap()])
        .env_clear()
        .env("TEHAS_HOME", &home)
        .env(db::TEST_BUSY_REPORT, "soon")
        .output()
        .unwrap();
    assert_eq!(bad.status.code(), Some(64));
    assert_eq!(
        String::from_utf8_lossy(&bad.stderr).trim_end(),
        format!("store: {} is not a count: soon", db::TEST_BUSY_REPORT)
    );
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

    // The same second again, as two verbs of one update reach it on a fast
    // host: the copy that is there is not opened for writing, and the verb
    // writes the next free name instead of failing a migration that is fine.
    let same = migrate_at(&home, &state, 1_792_000_000).unwrap();
    let next = PathBuf::from(
        serde_json::from_str::<serde_json::Value>(&same).unwrap()["backup"]
            .as_str()
            .unwrap(),
    );
    assert_eq!(next, db::backup_name_at(&state, 9, 1_792_000_000, 1));
    assert!(db::is_database(&next));
    assert_eq!(mode(&next), 0o600);
    assert_eq!(fs::read(&file).unwrap(), before);

    // Every name of that second taken: the copy cannot be made, and the verb
    // says so. Chosen over a directory that cannot be written because no mode
    // of a directory says it: a caller that may read one may write it too,
    // whatever its bits say, and root is not stopped by 0500.
    let first_copy = fs::read(&copy).unwrap();
    for n in 2..db::BACKUP_NAMES {
        fs::write(db::backup_name_at(&state, 9, 1_792_000_000, n), b"taken").unwrap();
    }
    let err = migrate_at(&home, &state, 1_792_000_000)
        .expect_err("a backup with no free name is refused, not overwritten");
    assert_eq!(err.0, 78, "{}", err.1);
    assert!(err.1.starts_with("unreadable "), "{}", err.1);
    assert!(err.1.contains("cannot make the backup"), "{}", err.1);
    // The whole point: no schema was touched, so a reader still sees the version
    // the file came with, and the copy the verb could not make left nothing
    // behind to be mistaken for one.
    let conn = db::open_read_only(&file).unwrap();
    assert_eq!(one::<i64>(&conn, "PRAGMA user_version"), db::SCHEMA_VERSION);
    assert_eq!(
        one::<String>(&conn, "SELECT value FROM meta WHERE key = 'schema'"),
        "9"
    );
    drop(conn);
    assert_eq!(fs::read(&file).unwrap(), before, "a failed backup migrated");
    let mut allowed: Vec<std::ffi::OsString> = ["tehas.db", "tehas.db-shm", "tehas.db-wal"]
        .iter()
        .map(std::ffi::OsString::from)
        .collect();
    for n in 0..db::BACKUP_NAMES {
        allowed.push(
            db::backup_name_at(&state, 9, 1_792_000_000, n)
                .file_name()
                .unwrap()
                .to_os_string(),
        );
    }
    for e in fs::read_dir(&state).unwrap() {
        let name = e.unwrap().file_name();
        assert!(allowed.contains(&name), "state/ holds {name:?}");
    }
    assert_eq!(
        fs::read(&copy).unwrap(),
        first_copy,
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
fn migrate_names_its_backup_in_json_whatever_its_path_holds() {
    // A home with a control character, a quote and a letter outside ASCII in
    // its path: JSON escapes the first two its own way, and Rust's Debug form
    // is not JSON.
    let home = scratch("migrate-odd-name").join("ho\u{1b}me \"x\" \u{e4}");
    fs::create_dir(&home).unwrap();
    init(&home, false).unwrap();
    let state = home.join("state");
    let line = migrate(&home, &state).unwrap();
    let value: serde_json::Value = serde_json::from_str(&line).expect(&line);
    let copy = PathBuf::from(value["backup"].as_str().unwrap());
    assert_eq!(copy.parent().unwrap(), state);
    assert!(line.contains("\\u001b"), "{line}");
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
    assert_eq!(value["to"], 9, "{line}");
    let file = db::path(&home);
    let conn = db::open_read_only(&file).unwrap();
    assert_eq!(one::<i64>(&conn, "PRAGMA user_version"), 9);
    assert_eq!(
        one::<String>(&conn, "SELECT value FROM meta WHERE key = 'schema'"),
        "9"
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
            "schema=9",
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
    assert_eq!(report["schema"], 9);
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
    let made = format!("tehas-9-{}.db", 1_792_000_000);
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
        db::backup_name(&state, 9, 1_792_000_000),
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
    // Every name of `tehas-9-<now>` held by a file the verb did not make is the
    // copy it cannot write, and the verb must then change nothing at all.
    let home = scratch("migrate-after");
    fs::create_dir_all(home.join("state")).unwrap();
    fs::write(db::path(&home), b"").unwrap();
    let state = home.join("state");
    for n in 0..db::BACKUP_NAMES {
        fs::write(
            db::backup_name_at(&state, 9, 1_792_000_000, n),
            b"not a backup",
        )
        .unwrap();
    }

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
    // The foreign files are untouched: the verb neither opened one for writing
    // nor replaced it with a copy of its own.
    for n in 0..db::BACKUP_NAMES {
        let foreign = db::backup_name_at(&state, 9, 1_792_000_000, n);
        assert_eq!(fs::read(&foreign).unwrap(), b"not a backup");
    }
}

#[test]
fn migrate_reports_the_three_fields_and_a_database_at_9_stays_at_9() {
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
    assert_eq!(value["from"], 9, "{line}");
    assert_eq!(value["to"], 9, "{line}");
    assert!(value["backup"].is_string(), "{line}");
    for field in ["api", "verb", "ok", "from", "to", "backup"] {
        assert!(value.get(field).is_some(), "{field} is missing from {line}");
    }
    // Already at 1: nothing to run, and the same answer shape. The backup is
    // still written, because the verb promises a copy before it touches the
    // file and has no way to know it will not need it until it has looked.
    assert_eq!(value["from"], 9, "{line}");
    assert_eq!(value["to"], 9, "{line}");
    assert!(db::is_database(&PathBuf::from(
        value["backup"].as_str().unwrap()
    )));
    let file = db::path(&home);
    checkpoint(&file);
    let before = fs::read(&file).unwrap();
    // The same instant again: the copy of the first call is on the disk under
    // that name, and a verb that read it back and called it done would report a
    // backup it did not make. It makes a new copy under the next name instead.
    let first = PathBuf::from(value["backup"].as_str().unwrap());
    let same = migrate(&home, &state).unwrap();
    let next = PathBuf::from(
        serde_json::from_str::<serde_json::Value>(&same).unwrap()["backup"]
            .as_str()
            .unwrap(),
    );
    assert_ne!(
        next, first,
        "a second call in the same second reused a copy"
    );
    assert!(first.exists() && db::is_database(&next), "{same}");
    // A minute later it is a different instant and a different file, and the
    // store itself is the file it was: the verb had nothing to run.
    let again = migrate_at(&home, &state, 1_792_000_060).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&again).unwrap()["from"],
        9,
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
    assert_eq!(copies.len(), 3, "three calls, three copies: {copies:?}");
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
        conn.pragma_update(None, "user_version", 10).unwrap();
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
            .any(|n| n.to_string_lossy().starts_with("tehas-9-")),
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
    // The verb has to meet the writer's bound rather than beat the holder.
    crate::verbs_tests::short_bounds();
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

// --- the record writer's slow notices ---------------------------------------
// The record of a cycle keeps the driver's `dispatch-slow` lines so a stuck
// child is read without the console (tehas_core::slow, lib/improve.sh).

fn slow(home: &Path, stream: &str, name: &str) -> Result<String, (u8, String)> {
    let dir = scratch(name);
    let given = dir.join("work-7.jsonl");
    let out = dir.join("slow.jsonl");
    fs::write(&given, stream).unwrap();
    let r = execute(
        home,
        &Verb::Slow {
            file: given,
            out: out.clone(),
        },
        1_792_000_000,
    );
    r.map(|line| match fs::read_to_string(&out) {
        Ok(kept) => format!("{line}\n{kept}"),
        // No file: the answer stands alone, which is what a cycle with no
        // notice gives the record.
        Err(_) => line,
    })
}

#[test]
fn slow_keeps_the_notices_of_a_cycle_and_answers_with_their_count() {
    let home = scratch("slow-home");
    let stream = concat!(
        "{\"v\":1,\"type\":\"agent_start\"}\n",
        "{\"v\":1,\"kind\":\"dispatch-slow\",\"step\":\"lens-review\",\"turns\":257}\n",
        "{\"v\":1,\"kind\":\"dispatch-slow\",\"command\":\"bash -c probe\",\"count\":24,\"identicalResultCount\":24}\n",
    );
    let answer = slow(&home, stream, "slow-kept").unwrap();
    let (line, kept) = answer.split_once('\n').unwrap();
    assert!(line.contains("\"verb\":\"slow\""), "{line}");
    assert!(line.contains("\"lines\":2"), "{line}");
    assert_eq!(kept.lines().count(), 2, "{kept}");
    assert!(kept.contains("\"identicalResultCount\":24"), "{kept}");
}

#[test]
fn slow_leaves_no_file_when_the_cycle_had_no_notice() {
    let home = scratch("slow-none");
    let answer = slow(
        &home,
        "{\"v\":1,\"kind\":\"dispatch-completed\",\"ok\":true}\n",
        "slow-none",
    )
    .unwrap();
    assert!(
        answer.starts_with("{\"api\":1,\"verb\":\"slow\",\"ok\":true,\"lines\":0}"),
        "{answer}"
    );
    // Nothing after the answer: the caller's file was never made.
    assert!(!answer.contains('\n'), "{answer}");
}

#[test]
fn slow_refuses_a_file_it_cannot_read() {
    let home = scratch("slow-link");
    let dir = scratch("slow-link-files");
    let real = dir.join("elsewhere");
    fs::write(&real, "{\"kind\":\"dispatch-slow\"}\n").unwrap();
    let link = dir.join("work-7.jsonl");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let out = dir.join("slow.jsonl");
    let (code, why) = execute(
        &home,
        &Verb::Slow {
            file: link,
            out: out.clone(),
        },
        1_792_000_000,
    )
    .unwrap_err();
    assert_eq!(code, 1, "{why}");
    assert!(!out.exists(), "a refusal wrote the record's file");
}

#[test]
fn slow_names_a_file_argument_and_refuses_any_other_shape() {
    assert_eq!(
        parse(&["slow".into(), "/a".into(), "--out".into(), "/b".into()]),
        Some(Verb::Slow {
            file: PathBuf::from("/a"),
            out: PathBuf::from("/b"),
        })
    );
    for args in [
        vec!["slow".to_string()],
        vec!["slow".to_string(), "/a".to_string()],
        vec!["slow".to_string(), "/a".to_string(), "--out".to_string()],
        vec![
            "slow".to_string(),
            "/a".to_string(),
            "--into".to_string(),
            "/b".to_string(),
        ],
    ] {
        assert_eq!(parse(&args), None, "{args:?}");
    }
}

// --- the review issue -------------------------------------------------------
// The title and body of the issue tehasbot review files for triage, rendered
// from the document of one run (tehas_core::review).

/// The verb's three arguments, given as words.
fn review_args(args: &[&str]) -> Vec<String> {
    let mut v = vec!["review-issue".to_string()];
    v.extend(args.iter().map(|s| s.to_string()));
    v
}

/// The document of a run, written to a file of its own and handed to the verb
/// with the two names the title is built from.
fn review_issue(home: &Path, name: &str, document: &str) -> Result<String, (u8, String)> {
    let dir = scratch(name);
    let given = dir.join("review-2026-10-27.json");
    fs::write(&given, document).unwrap();
    execute(
        home,
        &Verb::ReviewIssue(ReviewIssue {
            path: given,
            branch: "main".to_string(),
            commit: "cb6912a".to_string(),
        }),
        1_792_000_000,
    )
}

/// The same verb at a file the caller names, for the refusals that are about
/// the file rather than its contents.
fn review_at(path: PathBuf) -> Result<String, (u8, String)> {
    execute(
        &scratch("review-at"),
        &Verb::ReviewIssue(ReviewIssue {
            path,
            branch: "main".to_string(),
            commit: "cb6912a".to_string(),
        }),
        1_792_000_000,
    )
}

#[test]
fn review_issue_answers_the_title_and_body_as_one_line() {
    let home = scratch("review-home");
    let document = concat!(
        "{\"exitCode\":0,\"verdict\":\"ISSUES_FOUND\",\"noReview\":false,\"findings\":[",
        "{\"severity\":\"CRITICAL\",\"lens\":\"security\",\"path\":\"lib/config.sh\",\"line\":4,",
        "\"title\":\"a token is printed\",\"description\":\"at length\",\"suggestion\":\"hide it\"},",
        "{\"severity\":\"MEDIUM\",\"lens\":\"simplicity\",\"path\":\"console/src/render.rs\",\"line\":2,",
        "\"title\":\"a branch that repeats\"},",
        "{\"severity\":\"LOW\",\"lens\":\"performance\",\"path\":\"console/core/src/slow.rs\",\"line\":9,",
        "\"title\":\"a read of the whole file\"}]}",
    );
    let answer = review_issue(&home, "review-answered", document).unwrap();
    assert_eq!(
        answer.lines().count(),
        1,
        "one object is one line: {answer}"
    );
    assert!(
        answer.starts_with("{\"api\":1,\"verb\":\"review-issue\",\"ok\":true,"),
        "{answer}"
    );
    // The body is model text with newlines and quotes in it, so the test of
    // what the answer means is the parser the shell's reader uses.
    let line: serde_json::Value = serde_json::from_str(&answer).unwrap();
    assert_eq!(
        line["title"],
        "Six-lens review: 3 findings on main since cb6912a"
    );
    assert_eq!(
        line["body"],
        "### Critical\n- [security] lib/config.sh:4: a token is printed\n\n\
         ### Medium\n- [simplicity] console/src/render.rs:2: a branch that repeats\n\n\
         Low: 1 more findings not listed."
    );
    // What a finding says at length stays in the log of the run.
    assert!(!answer.contains("at length"), "{answer}");
}

#[test]
fn review_issue_escapes_an_answer_whose_body_holds_newlines_and_quotes() {
    // The words of a finding are the model's. The answer is still one line,
    // and parses back with the newlines and the quotes of the body preserved:
    // what the store's reader in lib/store.sh requires of every verb.
    let home = scratch("review-escaped");
    let document = "{\"findings\":[{\"severity\":\"HIGH\",\"lens\":\"security\",\
                    \"path\":\"a\\\"b.rs\",\"line\":7,\"title\":\"says \\\"no\\\" and \\\\n more\"}]}";
    let answer = review_issue(&home, "review-escape", document).unwrap();
    assert_eq!(
        answer.lines().count(),
        1,
        "one object is one line: {answer}"
    );
    let line: serde_json::Value = serde_json::from_str(&answer).unwrap();
    assert_eq!(
        line["body"],
        "### High\n- [security] a\"b.rs:7: says \"no\" and \\n more"
    );
    assert!(line["body"].as_str().unwrap().contains('\n'));
}

#[test]
fn review_issue_answers_a_title_that_quotes_a_token_masked_and_as_one_line() {
    // The words of a finding are the model's and may hold a secret it saw. The
    // verb answers with the body masked -- the same tehas_core::mask the
    // console's log of the run goes through -- and one line the store's reader
    // parses back: what reaches the half that files it is safe to post.
    let home = scratch("review-token");
    // Synthetic filler, the shape the workspace uses for token fixtures
    // (console/src/export.rs): a real token pattern with no token in it, so no
    // secret scan fires on this file.
    let secret = format!("ghp_{}", "A".repeat(36));
    let document = format!(
        concat!(
            "{{\"findings\":[{{\"severity\":\"CRITICAL\",\"lens\":\"security\",",
            "\"path\":\"instances/example/settings.sh\",\"line\":7,",
            "\"title\":\"a token is printed: {}\"}}]}}",
        ),
        secret
    );
    let answer = review_issue(&home, "review-token-file", &document).unwrap();
    assert_eq!(
        answer.lines().count(),
        1,
        "one object is one line: {answer}"
    );
    let line: serde_json::Value = serde_json::from_str(&answer).unwrap();
    assert_eq!(
        line["body"],
        "### Critical\n- [security] instances/example/settings.sh:7: a token is printed: ghp_***"
    );
}

#[test]
fn review_issue_of_no_findings_answers_the_empty_body() {
    let home = scratch("review-none");
    let answer = review_issue(
        &home,
        "review-no-findings",
        "{\"exitCode\":0,\"findings\":[]}",
    )
    .unwrap();
    let line: serde_json::Value = serde_json::from_str(&answer).unwrap();
    assert_eq!(line["body"], "");
    assert_eq!(
        line["title"],
        "Six-lens review: 0 findings on main since cb6912a"
    );
}

#[test]
fn the_empty_document_of_a_run_that_wrote_nothing_is_a_filing_of_nothing() {
    // The console makes the document before the container fills it, so a run
    // whose container said nothing leaves a file of no bytes. It is no JSON,
    // and it is not a refusal either: it is the document of a run that left no
    // findings, and the verb that files renders it as one. GitHub is asked
    // nothing, the answer is the line of a filing of nothing, and the record
    // of the range moves on as it does for a run that found none.
    let dir = scratch("file-empty-doc");
    let gh = stub_gh(&dir);
    let f = filing("empty-doc", EMPTY_DOC, &[]);
    fs::write(&f.doc, "").unwrap();
    let line = filed(&f, &gh);
    assert_eq!(line["action"], "nothing");
    assert_eq!(line["findings"], 0);
    assert_eq!(line["cost"], 0);
    assert!(
        line["said"]
            .as_str()
            .unwrap()
            .starts_with("nothing to file:"),
        "{line}"
    );
    assert!(
        gh_calls(&gh).is_empty(),
        "GitHub was asked: {:?}",
        gh_calls(&gh)
    );
}

#[test]
fn review_issue_refuses_a_document_that_is_not_json() {
    let home = scratch("review-malformed");
    // An array, a string and a number are JSON but no document of a run, and
    // bytes that are no UTF-8 are no JSON: all of them are refused. An empty
    // file is not among them: it is a run whose container wrote nothing, and
    // `the_empty_document_of_a_run_that_wrote_nothing` holds what it answers.
    for document in [
        "not json at all",
        "{\"findings\":",
        "\u{ff}\u{fe} bytes no UTF-8 decoder takes",
        "[]",
        "\"a string\"",
        "42",
    ] {
        let (code, why) = review_issue(&home, "review-bad", document).unwrap_err();
        assert_eq!(code, 64, "{document:?} answered {why}");
        assert!(why.starts_with("review-issue: "), "{document:?} said {why}");
    }
}

#[test]
fn review_issue_refuses_a_file_it_cannot_read() {
    // A link, a file over the bound and a file that is not there: each is
    // refused at 64 with the name of the argument that was refused, and the
    // line main prints carries the verb's own word first.
    let dir = scratch("review-file-args");

    let real = dir.join("elsewhere.json");
    fs::write(&real, "{\"findings\":[]}").unwrap();
    let link = dir.join("review.json");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let (code, why) = review_at(link).unwrap_err();
    assert_eq!(code, 64, "{why}");
    assert!(
        why.starts_with("review-issue: ") && why.ends_with(" is not a plain file"),
        "{why}"
    );

    let over = dir.join("over.json");
    fs::write(&over, vec![b'x'; (1 << 20) + 1]).unwrap();
    let (code, why) = review_at(over).unwrap_err();
    assert_eq!(code, 64, "{why}");
    assert!(
        why.starts_with("review-issue: ") && why.ends_with(" is over 1 MiB"),
        "{why}"
    );

    let missing = dir.join("gone.json");
    let (code, why) = review_at(missing).unwrap_err();
    assert_eq!(code, 64, "{why}");
    assert!(
        why.starts_with("review-issue: ") && why.ends_with(" is not a plain file"),
        "{why}"
    );
    // The document of the same directory that is a plain file is read: the
    // refusals above are about the file, not about where it sits.
    review_at(real).unwrap();
}

#[test]
fn review_issue_names_three_arguments_and_refuses_any_other_shape() {
    assert_eq!(
        parse(&review_args(&["/a.json", "main", "cb6912a"])),
        Some(Verb::ReviewIssue(ReviewIssue {
            path: PathBuf::from("/a.json"),
            branch: "main".to_string(),
            commit: "cb6912a".to_string(),
        }))
    );
    assert_eq!(
        parse(&review_args(&[
            "/a.json",
            "feature/issue-883-review",
            &"a".repeat(40)
        ])),
        Some(Verb::ReviewIssue(ReviewIssue {
            path: PathBuf::from("/a.json"),
            branch: "feature/issue-883-review".to_string(),
            commit: "a".repeat(40),
        })),
        "the branch of a worktree is a name the verb takes"
    );
    // Any other shape, and either name outside its grammar, is refused by
    // parse: main prints the usage and exits 64 without reading the document.
    for bad in [
        vec![],
        vec!["/a.json"],
        vec!["/a.json", "main"],
        vec!["/a.json", "main", "cb6912a", "extra"],
        vec!["/a.json", "", "cb6912a"],
        vec!["/a.json", " main", "cb6912a"],
        vec!["/a.json", "-main", "cb6912a"],
        vec!["/a.json", &format!("a{}", "b".repeat(80)), "cb6912a"],
        vec!["/a.json", "main;rm", "cb6912a"],
        vec!["/a.json", "main", "cb6912"],
        vec!["/a.json", "main", "cb6912z"],
        vec!["/a.json", "main", "CB6912A"],
        vec!["/a.json", "main", ""],
        vec!["--json", "/a.json", "main", "cb6912a"],
    ] {
        assert_eq!(parse(&review_args(&bad)), None, "{bad:?}");
    }
}

#[test]
fn review_issue_writes_nothing_to_the_home() {
    // The verb reads the document it was given and answers: it is not one of
    // the verbs that open the store, so the home of a call is left as it was.
    let home = scratch("review-quiet");
    let before: Vec<String> = names(&home);
    review_issue(&home, "review-kept", "{\"findings\":[]}").unwrap();
    assert_eq!(names(&home), before, "the verb wrote into the home");
    assert!(!home.join("state").exists(), "no database was opened");
}

fn names(home: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(home)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    v.sort();
    v
}

// --- store review-file ------------------------------------------------------

/// The findings of one run, as the document of a run that found two holds
/// them, with one word of a finding that the mask takes out and one that the
/// host's own name is meant to.
const REVIEW_DOC: &str = concat!(
    "{\"exitCode\":10,\"verdict\":\"ISSUES_FOUND\",\"noReview\":false,\"findings\":[",
    "{\"severity\":\"CRITICAL\",\"lens\":\"security\",\"path\":\"lib/config.sh\",\"line\":4,",
    "\"title\":\"a token is printed\"},",
    "{\"severity\":\"MEDIUM\",\"lens\":\"simplicity\",\"path\":\"libexec/review\",\"line\":2,",
    "\"title\":\"a branch that repeats\"}]}",
);

/// What one filing was given, written to a document of its own, with the two
/// names of the title and the command's own line. `gh` is looked for on the
/// path of the call, which is where the cases of the filing put a stub of
/// their own; `private` is the host's names, one per entry.
struct Filing {
    home: PathBuf,
    doc: PathBuf,
    ask: review_issue::Ask,
}

fn filing(name: &str, document: &str, private: &[&str]) -> Filing {
    let home = scratch(&format!("file-{name}-home"));
    // A home of the store's own shape, so the verb's own guard on TEHAS_HOME is
    // met as it is on a host: the filing writes its body under state/.
    fs::create_dir_all(home.join("state")).unwrap();
    fs::write(home.join(".tehas-home"), "tehas\n").unwrap();
    let dir = scratch(&format!("file-{name}-doc"));
    let doc = dir.join("review-2026-10-28.json");
    fs::write(&doc, document).unwrap();
    Filing {
        home,
        doc: doc.clone(),
        ask: review_issue::Ask {
            repo: "acme/backlog".to_string(),
            doc,
            branch: "main".to_string(),
            commit: "cb6912a".repeat(6),
            ask: "Apply `tehas:refine` to plan this into parts.".to_string(),
            private: private.iter().map(|s| s.to_string()).collect(),
            gh: PathBuf::from("gh"),
        },
    }
}

/// The filing of a case, asked of the `gh` of the case: the verb is given the
/// stub by name, so a case of the filing runs no GitHub and needs no token. The
/// stub answers each kind of call from a file of the case and records the call,
/// which the case reads back with `gh_calls`. The answer is the store's line.
fn filed(f: &Filing, gh: &Path) -> Value {
    let mut ask = f.ask.clone();
    ask.gh = gh.to_path_buf();
    let body = review_issue::file(&ask, &f.home).expect("the filing answered");
    assert_eq!(body.lines().count(), 1, "one object is one line: {body}");
    // The body of the filing is gone when the verb ends: what gh was given was
    // the run's words, not a record of the host.
    assert!(
        !review_issue::body_file(&f.home).exists(),
        "the body of the filing was left in the home"
    );
    let line: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(line["verb"], "review-file");
    assert_eq!(line["ok"], true);
    line
}

/// The calls the stub recorded, one per line, each ending with the kind the
/// stub read out of it.
fn gh_calls(gh: &Path) -> Vec<String> {
    fs::read_to_string(gh.with_extension("calls"))
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

/// The `gh` of a case: a program of the case's own, which the verb finds on
/// the path. One line per call goes into `<gh>.calls`; the arguments after the
/// verb's two words are written as flags, so a body file is not mistaken for a
/// call. A call of no kind the case named, and a kind with no file, answer as
/// GitHub does when there is nothing of it; a file holding `fail` refuses.
fn stub_gh(dir: &Path) -> PathBuf {
    let bin = dir.join("gh");
    let body = r#"#!/bin/sh
# The gh of the store's own tests: one line per call in gh.calls, each ending
# with the kind read from the arguments, and an answer for the list and for the
# two calls of the filing from a file of the case (list.answer, create.answer,
# comment.answer). A kind with no file answers as GitHub does when there is
# nothing of it; a file that holds the word fail refuses the call.
set -u
dir=$(dirname "$0")
args="$*"
kind=other
i=0
for a in "$@"; do
  i=$((i + 1))
  case "$a" in
    --body-file)
      # The third word of a comment and the fifth of a create: the number of
      # the issue tells them apart, not a guess at the position.
      [ "$2" = comment ] && kind=comment || kind=create
      j=0
      for b in "$@"; do
        j=$((j + 1))
        [ "$b" = --body-file ] || continue
        k=$((j + 1))
        n=0
        for c in "$@"; do
          n=$((n + 1))
          [ "$n" = "$k" ] && cp "$c" "$dir/last.body" 2>/dev/null
        done
      done
      ;;
    --state) kind=list ;;
    --json) [ "$kind" = other ] && kind=list ;;
    --title) kind=create ;;
    --label) ;;
  esac
done
case "$args" in
  "issue list "*) kind=list ;;
  "issue create "*) kind=create ;;
  "issue comment "*) kind=comment ;;
esac
printf '%s %s\n' "$args" "$kind" >>"$dir/gh.calls"
ans=$dir/$kind.answer
if [ -f "$ans" ]; then
  [ "$(cat "$ans")" != fail ] || exit 1
  [ "$kind" = list ] && { cat "$ans"; exit 0; }
  rm -f "$dir/last.body"
  exit 0
fi
[ "$kind" = list ] && { echo '[]'; exit 0; }
exit 0
"#;
    fs::write(&bin, body).unwrap();
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

/// The answer a case gives for one kind of call, in the directory of the stub.
fn gh_answer(dir: &Path, kind: &str, answer: &str) {
    fs::write(dir.join(format!("{kind}.answer")), format!("{answer}\n")).unwrap();
}

/// The findings of a run that found none: the document answers a body of
/// nothing, and a filing of nothing asks GitHub nothing.
const EMPTY_DOC: &str = "{\"exitCode\":0,\"findings\":[]}";

#[test]
fn review_file_files_the_findings_as_one_issue_of_the_label() {
    let dir = scratch("file-new");
    let gh = stub_gh(&dir);
    let f = filing("new", REVIEW_DOC, &[]);
    let line = filed(&f, &gh);
    assert_eq!(line["findings"], 2);
    assert_eq!(line["action"], "issue");
    assert_eq!(line["number"], Value::Null);
    assert_eq!(
        line["said"], "2 findings are filed in acme/backlog",
        "{line}"
    );
    assert_eq!(line["why"], Value::Null);
    assert_eq!(line["cost"], 2, "the list, then the issue: {line}");
    let calls = gh_calls(&gh);
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert!(calls[0].ends_with(" list"), "{}", calls[0]);
    assert!(
        calls[0].contains("--label tehas:improvement"),
        "{}",
        calls[0]
    );
    assert!(calls[0].contains("--json number,title"), "{}", calls[0]);
    assert!(calls[1].ends_with(" create"), "{}", calls[1]);
    assert!(
        calls[1].contains("Six-lens review: 2 findings on main since cb6912a"),
        "{}",
        calls[1]
    );
    assert!(
        calls[1].contains("--label tehas:improvement"),
        "{}",
        calls[1]
    );
    // The body gh was given: the two findings, masked, and the line of the
    // command last of all, as the command wrote it and not as a finding.
    let body = fs::read_to_string(dir.join("last.body")).expect("the body file");
    assert_eq!(
        body,
        "### Critical\n- [security] lib/config.sh:4: a token is printed\n\n\
         ### Medium\n- [simplicity] libexec/review:2: a branch that repeats\n\
         Apply `tehas:refine` to plan this into parts.",
        "{body}"
    );
}

#[test]
fn review_file_comments_on_the_open_issue_of_the_review_and_files_no_other() {
    let dir = scratch("file-comment");
    let gh = stub_gh(&dir);
    gh_answer(
        &dir,
        "list",
        "[{\"number\":41,\"title\":\"Six-lens review: 2 findings on main since 1111111\"}]",
    );
    let f = filing("comment", REVIEW_DOC, &[]);
    let line = filed(&f, &gh);
    assert_eq!(line["action"], "comment");
    assert_eq!(line["number"], 41);
    assert_eq!(line["findings"], 2);
    assert_eq!(
        line["said"],
        "2 findings are a comment on #41 of acme/backlog"
    );
    assert_eq!(line["cost"], 2, "{line}");
    let calls = gh_calls(&gh);
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert!(calls[1].starts_with("issue comment 41"), "{}", calls[1]);
    assert!(!calls.iter().any(|c| c.contains("create")), "{calls:?}");
}

#[test]
fn review_file_of_an_open_issue_of_another_kind_files_a_new_one() {
    let dir = scratch("file-other");
    let gh = stub_gh(&dir);
    gh_answer(&dir, "list", "[{\"number\":41,\"title\":\"Slow tests\"}]");
    let f = filing("other", REVIEW_DOC, &[]);
    let line = filed(&f, &gh);
    assert_eq!(line["action"], "issue");
    assert_eq!(line["number"], Value::Null);
    let calls = gh_calls(&gh);
    assert!(calls.iter().any(|c| c.ends_with(" create")), "{calls:?}");
    assert!(!calls.iter().any(|c| c.ends_with(" comment")), "{calls:?}");
}

#[test]
fn review_file_of_nothing_to_file_asks_the_host_nothing() {
    // The two shapes of a run that left no findings: a document of no findings
    // at all, and one the store will not read as a review document. The first
    // files nothing and asks nothing; the second is refused at 64 and the
    // command's own line says the run filed nothing.
    let dir = scratch("file-none");
    let gh = stub_gh(&dir);
    let f = filing("none", EMPTY_DOC, &[]);
    let line = filed(&f, &gh);
    assert_eq!(line["findings"], 0);
    assert_eq!(line["action"], "nothing");
    assert_eq!(line["cost"], 0, "{line}");
    assert_eq!(
        line["said"], "nothing to file: cb6912a left no findings",
        "{line}"
    );
    assert!(
        gh_calls(&gh).is_empty(),
        "GitHub was asked: {:?}",
        gh_calls(&gh)
    );

    // A document that is no document: the filing is refused before GitHub is
    // asked anything, with the store's own word for the file.
    let dir = scratch("file-bad");
    let gh = stub_gh(&dir);
    let f = filing("bad", "{\"findings\":\"not an array\"}", &[]);
    // The findings of such a document are none, so the filing asks nothing and
    // says the run left none, as the command has always said it.
    let line = review_issue::file(&f.ask, &f.home).unwrap();
    let line: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(line["findings"], 0);
    assert!(
        gh_calls(&gh).is_empty(),
        "GitHub was asked: {:?}",
        gh_calls(&gh)
    );
}

#[test]
fn review_file_refuses_a_document_it_cannot_read_and_asks_nothing() {
    let dir = scratch("file-unreadable");
    let gh = stub_gh(&dir);
    let f = filing("unreadable", EMPTY_DOC, &[]);
    fs::write(&f.doc, "not json at all").unwrap();
    let (code, why) = review_issue::file(&f.ask, &f.home).unwrap_err();
    assert_eq!(code, 64, "{why}");
    assert!(why.starts_with("review-file: "), "{why}");
    assert!(
        gh_calls(&gh).is_empty(),
        "GitHub was asked: {:?}",
        gh_calls(&gh)
    );
}

#[test]
fn review_file_asks_no_list_of_a_run_that_will_not_be_filed() {
    // The list is the first call of a filing that has something to file, and
    // never the first of one that has nothing: the order is what cost 0 and
    // cost 2 tell apart.
    let dir = scratch("file-order");
    let gh = stub_gh(&dir);
    let f = filing("order", REVIEW_DOC, &[]);
    filed(&f, &gh);
    assert_eq!(gh_calls(&gh).len(), 2);
}

#[test]
fn review_file_says_why_a_filing_github_refused_did_not_happen() {
    let dir = scratch("file-refused");
    let gh = stub_gh(&dir);
    gh_answer(&dir, "create", "fail");
    let f = filing("refused", REVIEW_DOC, &[]);
    let line = filed(&f, &gh);
    assert_eq!(line["action"], "nothing");
    assert_eq!(line["said"], "");
    assert_eq!(line["why"], "GitHub refused the issue in acme/backlog");
    assert_eq!(line["cost"], 2, "{line}");

    // The comment of an open issue, refused, names the issue it tried.
    let dir = scratch("file-comment-refused");
    let gh = stub_gh(&dir);
    gh_answer(
        &dir,
        "list",
        "[{\"number\":41,\"title\":\"Six-lens review: 1 findings on main since cb6912a\"}]",
    );
    gh_answer(&dir, "comment", "fail");
    let f = filing("comment-refused", REVIEW_DOC, &[]);
    let line = filed(&f, &gh);
    assert_eq!(line["action"], "nothing");
    assert_eq!(line["number"], 41);
    assert_eq!(
        line["why"], "GitHub refused the comment on #41 of acme/backlog",
        "{line}"
    );
}

#[test]
fn review_file_of_a_list_that_will_not_answer_files_the_findings_anyway() {
    // A gh that refuses the list is asked nothing more than the list would
    // have decided: the findings are filed as new, because a duplicate is read
    // by a person and a lost finding is not.
    let dir = scratch("file-list-refused");
    let gh = stub_gh(&dir);
    gh_answer(&dir, "list", "fail");
    let f = filing("list-refused", REVIEW_DOC, &[]);
    let line = filed(&f, &gh);
    assert_eq!(line["action"], "issue");
    assert_eq!(line["cost"], 2, "{line}");
    let calls = gh_calls(&gh);
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert!(calls[1].ends_with(" create"), "{}", calls[1]);
}

#[test]
fn review_file_masks_the_words_of_a_finding_before_gh_sees_them() {
    // The third finding of the fixture holds a comment marker and a
    // token-shaped string. The mask is tehas_core::mask, the one the console's
    // log of the same run goes through; the title of the issue is the
    // renderer's and holds no word of a finding that the mask would take.
    let dir = scratch("file-mask");
    let gh = stub_gh(&dir);
    let token = format!("ghp_{}", "A".repeat(36));
    let document = format!(
        "{{\"findings\":[{{\"severity\":\"HIGH\",\"lens\":\"security\",\"path\":\"a.rs\",\"line\":1,\"title\":\"gives <!-- sly --> and {token}\"}}]}}"
    );
    let f = filing("mask", &document, &[]);
    filed(&f, &gh);
    let body = fs::read_to_string(dir.join("last.body")).expect("the body file");
    assert!(!body.contains("<!-- sly -->"), "{body}");
    assert!(body.contains("<\\!-- sly"), "{body}");
    assert!(!body.contains(&format!("ghp_{}", "A".repeat(36))), "{body}");
    assert!(body.contains("ghp_***"), "{body}");
    // The command's own line, put on after the masking, is there as written.
    assert!(body.ends_with("Apply `tehas:refine` to plan this into parts."));
}

/// The names of the host, among the findings. The mask keeps the first call's
/// names for the process (the OnceLock of tehas_core::mask), and a program
/// that masks two runs with two lists would keep the first; the console sets
/// the names once, before its one run writes either of its two files, and the
/// filing does the same. So this is the one case of the filing that names any,
/// and the one above gives none.
#[test]
fn review_file_hides_the_names_the_command_gave_it() {
    // What belongs to the host and nobody else: the aliases, the endpoints,
    // the bot's email. The command hands the list to the verb, which masks them
    // among the findings as the console masks them in the log of the run. The
    // names are a OnceLock of the process, the first call's, as in the console,
    // so this is the one case of the filing that names any.
    let dir = scratch("file-private");
    let gh = stub_gh(&dir);
    let document = "{\"findings\":[{\"severity\":\"HIGH\",\"lens\":\"security\",\"path\":\"a.rs\",\"line\":1,\"title\":\"the host is tehas-host-alias\"}]}";
    let f = filing("private", document, &["tehas-host-alias"]);
    let mut ask = f.ask.clone();
    ask.gh = gh.clone();
    // The names go to the mask as the command's own entry does: once, before
    // the verb is run (main.rs's ReviewFile).
    tehas_core::mask::set_private(ask.private_names()).unwrap();
    review_issue::file(&ask, &f.home).unwrap();
    let body = fs::read_to_string(dir.join("last.body")).expect("the body file");
    // The name is a name of the host as the mask sees one: four characters at
    // the least, and not a word the mask's own output writes.
    assert!(
        body.contains("[private name]"),
        "the name of the host is hidden: {body}"
    );
    assert!(!body.contains("tehas-host-alias"), "{body}");
}

#[test]
fn review_file_writes_the_body_where_no_other_account_reads_it() {
    // The body is the model's words, and gh, a program of the host, is given
    // the file. It is written 0600 in the directory of 0700 the store's own
    // file lives in, and it goes when the verb ends.
    let dir = scratch("file-mode");
    let gh = stub_gh(&dir);
    let f = filing("mode", REVIEW_DOC, &[]);
    let mut ask = f.ask.clone();
    ask.gh = gh.clone();
    let line = review_issue::file(&ask, &f.home).unwrap();
    assert!(line.contains("review-file"), "{line}");
    assert!(!review_issue::body_file(&f.home).exists());
    // The body the stub was handed is the run's words, and no file of the
    // filing is left in the home: what gh read is gone with the verb.
    let body = fs::read_to_string(dir.join("last.body")).expect("the body file");
    assert!(body.contains("a branch that repeats"), "{body}");
    assert!(
        !review_issue::body_file(&f.home).exists(),
        "state/ holds no body of the filing"
    );
}

#[test]
fn review_file_writes_nothing_else_to_the_home() {
    let dir = scratch("file-quiet");
    let gh = stub_gh(&dir);
    let f = filing("quiet", REVIEW_DOC, &[]);
    let before: Vec<String> = names(&f.home.join("state"));
    filed(&f, &gh);
    // The body the calls were made with is gone, and nothing else was written:
    // the filing opens no database and keeps no record of its own.
    assert_eq!(names(&f.home.join("state")), before, "{:?}", before);
    assert!(!review_issue::body_file(&f.home).exists());
    assert!(
        !f.home.join("state").join("tehas.db").exists(),
        "the filing opened the database"
    );
}

#[test]
fn review_file_names_its_arguments_and_refuses_any_other_shape() {
    let good = |extra: &[&str]| {
        let mut v = vec![
            "review-file".to_string(),
            "--repo".to_string(),
            "acme/backlog".to_string(),
            "--doc".to_string(),
            "/a.json".to_string(),
            "--branch".to_string(),
            "main".to_string(),
            "--commit".to_string(),
            "cb6912a".to_string(),
            "--ask".to_string(),
            "Apply it.".to_string(),
        ];
        v.extend(extra.iter().map(|s| s.to_string()));
        v
    };
    assert_eq!(
        parse(&good(&[])),
        Some(Verb::ReviewFile(ReviewFile {
            repo: "acme/backlog".to_string(),
            doc: PathBuf::from("/a.json"),
            branch: "main".to_string(),
            commit: "cb6912a".to_string(),
            ask: "Apply it.".to_string(),
            private: vec![],
            gh: PathBuf::from("gh"),
        }))
    );
    assert_eq!(
        parse(&good(&["--json", "--private", "tehas-host"])).map(|v| match v {
            Verb::ReviewFile(r) => r.private,
            _ => panic!("not a filing"),
        }),
        Some(vec!["tehas-host".to_string()])
    );
    // Any name outside its grammar, a flag twice, a flag of no value, a flag
    // the verb does not know, or a name the command did not write: all refused
    // by parse, so main prints the usage and GitHub is asked nothing.
    for bad in [
        vec![],
        good(&["--ask", "twice"]),
        good(&["--repo", "other/backlog"]),
        good(&["--private"]),
        good(&["--repo"]),
        good(&["--other", "x"]),
        good(&["/extra"]),
        good(&["--ask", "two\nlines"]),
        good(&["--ask", ""]),
        good(&["--private", "short\u{1}name"]),
        good(&["--repo", "acme/"]),
        good(&["--repo", "/backlog"]),
        good(&["--repo", "acme/backlog/extra"]),
        good(&["--repo", "acme/back\u{e5}log"]),
        good(&["--branch", " main"]),
        good(&["--branch", "main;rm"]),
        good(&["--commit", "cb6912"]),
        good(&["--commit", "CB6912A"]),
    ] {
        assert_eq!(parse(&bad), None, "{bad:?}");
    }
}

#[test]
fn the_title_prefix_is_the_renderers() {
    // The filing recognises its own issues by the prefix of the title, and the
    // renderer makes the title: the two are one string, and a change to either
    // alone would file a second issue for a range that is already filed.
    let dir = scratch("file-prefix");
    let answer = review_issue(
        &dir,
        "file-prefix",
        "{\"findings\":[{\"severity\":\"HIGH\",\"lens\":\"security\",\"path\":\"a.rs\",\"line\":1,\"title\":\"t\"}]}",
    )
    .unwrap();
    let line: Value = serde_json::from_str(&answer).unwrap();
    assert!(
        line["title"]
            .as_str()
            .unwrap()
            .starts_with(review_issue::TITLE_PREFIX),
        "{}",
        line["title"]
    );
    assert_eq!(review_issue::TITLE_PREFIX, "Six-lens review:");
}

// --- store batch ------------------------------------------------------------

/// The rows of every table, as one number each, so a batch that wrote is seen
/// whatever it wrote and wherever it wrote it.
fn table_counts(home: &Path) -> Vec<(String, i64)> {
    let conn = Connection::open(db::path(home)).unwrap();
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    tables
        .into_iter()
        .map(|t| {
            let n: i64 = conn
                .query_row(&format!("SELECT count(*) FROM \"{t}\""), [], |r| r.get(0))
                .unwrap();
            (t, n)
        })
        .collect()
}

/// One `store batch` of the given calls, run as the command runs it: the
/// request on standard input, the answers on standard output. The verb's own
/// exit and the two lines it may print are given back with the answers.
fn batch(home: &Path, request: &str) -> (i32, Vec<Value>, String) {
    batch_at(home, crate::verbs_tests::NOW, request)
}

fn batch_at(home: &Path, now: i64, request: &str) -> (i32, Vec<Value>, String) {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new(store_binary())
        .arg("batch")
        .env("TEHAS_HOME", home)
        .env(crate::TEST_NOW, now.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the store of this build runs");
    child
        .stdin
        .take()
        .expect("the pipe is ours")
        .write_all(request.as_bytes())
        .expect("the request is written");
    let out = child.wait_with_output().expect("the store answered");
    let said = String::from_utf8_lossy(&out.stdout).to_string();
    let answers = serde_json::from_str::<Value>(said.trim())
        .unwrap_or_else(|e| panic!("{e} of {said:?}"))
        .get("answers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| panic!("no answers in {said:?}"));
    (
        out.status.code().unwrap_or(-1),
        answers,
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

/// What a call's answer printed, as the poll reads it: the text is the verb's
/// own line, and the store answers nothing else, so it parses.
fn said(a: &Value) -> Value {
    let text = a["out"]
        .as_str()
        .unwrap_or_else(|| panic!("the answer of a read is not text: {a}"));
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{e} of {text:?}"))
}

/// The four reads of one issue, in the order the poll asks them.
fn four(n: i64, qid: i64) -> Vec<Vec<String>> {
    let a = |x: &[&str]| x.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let (n, q) = (n.to_string(), qid.to_string());
    vec![
        a(&["plan-show", &n]),
        a(&["park-show", &n]),
        a(&["question-known", &n, "--id", &q]),
        a(&["approval-current", &n]),
    ]
}

fn request(calls: &[Vec<String>]) -> String {
    serde_json::to_string(calls).unwrap()
}

/// A cycle started by a label, ended and parked with a question of its own on
/// record: what the poll reads of an issue that waits for a person. Each issue
/// is approved and parked for a body of its own, so two issues of one test
/// differ by a name the answers carry: a batch that answered one issue with
/// another's rows is seen rather than assumed away.
fn asked(home: &Path, issue: i64, id: i64) {
    use crate::verbs_tests::{end, hold, next_at, ok, SHA_B};
    let lock = hold(home, issue);
    let bound = format!("{:064x}", 1_000_000 + issue * 1_000 + 7);
    let (n, at) = (issue.to_string(), next_at().to_string());
    ok(
        home,
        &[
            "approval-record",
            &n,
            "--at",
            &at,
            "--by",
            "alice",
            "--how",
            "label",
            "--label",
            "tehas:ready",
            "--body-sha",
            &bound,
        ],
    );
    let attempt = ok(home, &["cycle-start", &n])["attempt"].as_i64().unwrap();
    end(home, issue, attempt, "failed");
    let (a, q) = (attempt.to_string(), id.to_string());
    ok(
        home,
        &[
            "park",
            &n,
            "--attempt",
            &a,
            "--reason",
            "provider-outage",
            "--comment-id",
            &q,
            "--body-sha",
            SHA_B,
        ],
    );
    drop(lock);
}

#[test]
fn batch_answers_three_reads_in_the_order_it_was_given() {
    let home = crate::verbs_tests::home("batch-order");
    asked(&home, 12, 4001);
    let calls = vec![
        vec!["plan-show".into(), "12".into()],
        vec!["park-show".into(), "12".into()],
        vec![
            "question-known".into(),
            "12".into(),
            "--id".into(),
            "4001".into(),
        ],
    ];
    let (rc, answers, err) = batch(&home, &request(&calls));
    assert_eq!(rc, 0, "{err}");
    assert_eq!(err, "", "the verb wrote its failure on the error stream");
    assert_eq!(answers.len(), 3, "{answers:?}");
    for a in &answers {
        assert_eq!(a["rc"], 0, "{a}");
    }
    // Each out is the very line the single verb prints, and in the order of
    // the request: the poll reads an answer by its position, not its name.
    let single = |args: &[&str]| crate::verbs_tests::ok(&home, args);
    assert_eq!(said(&answers[0]), single(&["plan-show", "12"]));
    assert_eq!(said(&answers[1]), single(&["park-show", "12"]));
    assert_eq!(
        said(&answers[2]),
        single(&["question-known", "12", "--id", "4001"])
    );
    // The park's question is the one on record, so the read says it is known.
    assert_eq!(said(&answers[2])["known"], true);
    // The park read is of the same issue the plan read asked of: no answer
    // belongs to another call's arguments.
    assert_eq!(said(&answers[1]), single(&["park-show", "12"]));
}

#[test]
fn batch_reads_the_four_verbs_and_the_writes_are_refused_with_64() {
    let home = crate::verbs_tests::home("batch-refuses");
    asked(&home, 12, 4001);
    let before = table_counts(&home);
    let calls = vec![
        vec!["approval-current".into(), "12".into()],
        vec!["park-retry".into(), "12".into()],
        vec!["plan-begin".into(), "12".into()],
        vec!["journal-open".into()],
        vec!["poll-begin".into()],
        vec!["cycle-end".into(), "12".into()],
        vec!["not-a-verb".into()],
    ];
    let (rc, answers, err) = batch(&home, &request(&calls));
    assert_eq!(rc, 0);
    assert_eq!(err, "", "the verb wrote its failure on the error stream");
    assert_eq!(answers[0]["rc"], 0, "{}", answers[0]);
    assert_eq!(said(&answers[0])["verb"], "approval-current");
    // A call this verb does not run is refused before the store is opened, and
    // the exit says why: 64 for a call that stops at its arguments or names a
    // verb the store does not have at all, and 1 for a verb of the store that
    // this part does not answer, which is the exit a read of the store gives.
    let rc: Vec<i64> = answers[1..]
        .iter()
        .map(|a| a["rc"].as_i64().unwrap())
        .collect();
    // Each answer of a call this verb refuses is the line the single call prints
    // on its error stream, and the exit the single command gives with it: the
    // poll reads the same word for a read of a batch and for the read made alone.
    assert_eq!(rc, vec![1, 1, 1, 1, 1, 64], "{answers:?}");
    // The word a write is refused with names the verb, so a caller that logs
    // the answer knows which call it was.
    assert_eq!(answers[1]["out"], "park-retry #12: not a read of a batch");
    assert_eq!(answers[6]["out"], "usage not-a-verb: unknown verb");
    // A verb outside the four is refused by its name alone, before its other
    // arguments are asked anything: the exit of a read of the store and a line
    // naming the verb and the issue, whether its arguments would have parsed
    // or not. A name the store does not have at all keeps the usage line and
    // the 64 the command gives it.
    let (rc, refused, _) = batch(&home, &request(&[vec!["park".into(), "12".into()]]));
    assert_eq!(rc, 0, "{refused:?}");
    assert_eq!(refused[0]["rc"], 1, "{}", refused[0]);
    assert_eq!(refused[0]["out"], "park #12: not a read of a batch");
    let (_, refused, _) = batch(&home, &request(&[vec!["not-a-verb".into(), "12".into()]]));
    assert_eq!(refused[0]["rc"], 64, "{}", refused[0]);
    assert_eq!(refused[0]["out"], "usage not-a-verb: unknown verb");
    // Nothing was written: every row of the store is the row it was.
    assert_eq!(table_counts(&home), before);
}

#[test]
fn a_batch_of_only_refusals_never_opens_the_store() {
    // The one case a poll cannot answer for: a request that holds no read at
    // all is answered without the database, so a batch cannot be made to open
    // a store it was not given.
    let home = crate::verbs_tests::home("batch-no-store");
    let file = db::path(&home);
    fs::remove_file(&file).unwrap();
    let calls = vec![vec!["park-retry".into(), "12".into()]];
    let (rc, answers, err) = batch(&home, &request(&calls));
    assert_eq!(rc, 0);
    assert_eq!(err, "", "the verb wrote its failure on the error stream");
    assert_eq!(answers[0]["rc"], 1);
    assert_eq!(answers[0]["out"], "park-retry #12: not a read of a batch");
    assert!(!file.exists(), "a refusal made the store's file");
}

#[test]
fn a_batch_that_is_not_a_request_is_64_and_writes_nothing() {
    let home = crate::verbs_tests::home("batch-usage");
    asked(&home, 12, 4001);
    let before = table_counts(&home);
    let too_many: Vec<Vec<String>> =
        vec![vec!["park-show".to_string(), "12".to_string()]; crate::batch::MAX_CALLS + 1];
    let too_many = serde_json::to_string(&too_many).unwrap();
    for given in [
        "",
        "not json",
        "{\"plan-show\":\"12\"}",
        "[\"park-show\",\"12\"]",
        "[[\"park-retry\",\"12\"],[42]]",
        "[[]]",
        "[[\"park-show\",\"12\",\"extra\",\"1\",\"2\",\"3\"]]",
        &too_many,
    ] {
        let child = {
            use std::io::Write;
            use std::process::{Command, Stdio};
            let mut c = Command::new(store_binary())
                .arg("batch")
                .env("TEHAS_HOME", &home)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            c.stdin.take().unwrap().write_all(given.as_bytes()).unwrap();
            c.wait_with_output().unwrap()
        };
        assert_eq!(child.status.code(), Some(64), "{given:?}");
        assert!(child.stdout.is_empty(), "{given:?} answered JSON");
    }
    assert_eq!(table_counts(&home), before, "a bad request wrote");
}

#[test]
fn a_batch_refuses_every_write_the_store_has() {
    // The allowlist of the batch is four names wide and the store has about
    // forty verbs. Every verb the store's own parser accepts is asked of
    // `batch::parse` here, and anything that is not one of the four reads must
    // come back refused, whatever its arguments: were `READS` widened without
    // an audit of a verb's body, this is the case that says so.
    let verb_of = |call: &str| call.split(' ').next().unwrap_or("").to_string();
    for given in [
        "cycle-start 12",
        "cycle-stop 12",
        "cycle-end 12 --attempt 1 --status failed",
        "cycle-show 12",
        "cycle-show --running",
        "cycle-note 12 --text x",
        "park 12 --attempt 1 --reason provider-outage --body-sha aaaaaaaa",
        "park-retry 12",
        "park-told 12",
        "park-show --automatic",
        "park-show --refine",
        "question-asked 12 --id 4001 --kind park --body-sha aaaaaaaa",
        "question-answered 12 --id 4001",
        "plan-begin 12 --by alice --label r --brief-file /dev/null",
        "plan-answer 12 --turn 1 --text x",
        "plan-end 12 --close",
        "plan-show --open",
        "plan-note 12 --kind brief --id 1",
        "approval-record 12 --at 1 --by alice --how label --label r --body-sha aaaaaaaa",
        "approval-undo 12 --at 1",
        "live-open 12",
        "upkeep-backup",
        "locks-held",
        "poll-begin",
        "poll-show",
        "poll-idle",
        "journal-open",
        "journal-attempt --action x",
        "history-show 12 --kind cycle",
        "not-a-verb",
    ] {
        let text = serde_json::to_string(&[vec![verb_of(given)]]).unwrap();
        let asked = crate::batch::parse(&text).unwrap_or_else(|e| panic!("{given}: {e}"));
        match &asked[0] {
            crate::batch::Asked::Refused(_, _) => {}
            crate::batch::Asked::Call(call) => {
                panic!("{given} reached the store as a read of a batch: {call:?}")
            }
        }
        // The four reads are the only calls that parse into one.
        let four = matches!(
            verb_of(given).as_str(),
            "plan-show" | "park-show" | "question-known" | "approval-current"
        );
        assert_eq!(
            four,
            crate::batch::is_read(&verb_of(given)),
            "{given} is outside the four the batch names"
        );
    }
}

#[test]
fn a_batch_refuses_a_verb_whose_name_is_a_read_but_is_not() {
    // A name of the four is answered by the grammar of that verb alone, so the
    // one call of the store's own read arms is the one a batch makes: `--open`
    // and `--automatic` are the switches of the same verb that read the whole
    // store rather than the issue, and a batch of them is refused, not run.
    let home = crate::verbs_tests::home("batch-switches");
    let before = table_counts(&home);
    for call in [
        vec!["plan-show".to_string(), "--open".to_string()],
        vec!["park-show".to_string(), "--automatic".to_string()],
        vec!["park-show".to_string(), "--refine".to_string()],
        vec!["park-show".to_string()],
    ] {
        let one = [call.as_slice()];
        let text = serde_json::to_string(&one).unwrap();
        let (rc, answers, err) = batch(&home, &text);
        assert_eq!(rc, 0, "{call:?}: {err}");
        assert_eq!(answers[0]["rc"], 64, "{call:?}: {}", answers[0]);
        assert!(
            answers[0]["out"]
                .as_str()
                .unwrap_or_default()
                .starts_with(&format!("usage {}: ", call[0])),
            "{call:?}: {}",
            answers[0]
        );
    }
    assert_eq!(table_counts(&home), before, "a refused switch wrote");
}

#[test]
fn a_batch_with_no_home_refuses_before_it_reads_its_request() {
    // The guard every verb passes: the home is asked for before a byte of the
    // request is read, so a caller with no usable home cannot make the store
    // take its input into memory at all. The request is never written: there is
    // 1 MiB of calls the store must not have to hold.
    use std::process::{Command, Stdio};
    let mut child = Command::new(store_binary())
        .arg("batch")
        .env_remove("TEHAS_HOME")
        .env(crate::TEST_NOW, crate::verbs_tests::NOW.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the store of this build runs");
    // The pipe is closed with nothing in it, so a store that read its request
    // first would answer 64 for an empty one rather than 78 for no home.
    drop(child.stdin.take());
    let out = child.wait_with_output().expect("the store answered");
    assert_eq!(
        out.status.code(),
        Some(78),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty(), "a call with no home answered JSON");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("TEHAS_HOME"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn a_batch_reads_one_snapshot_of_the_store() {
    // The point of the one opening: two calls of one batch cannot see two
    // states of the store, whatever another writer does while the batch runs.
    // The batch's own reads show it: the second read of one issue is the same
    // answer as the first, and both are read after a writer has been held out.
    let home = crate::verbs_tests::home("batch-snapshot");
    asked(&home, 12, 4001);
    // A writer holds the store's write lock: a batch that opened the store
    // twice would pay for the wait twice, and a batch that read without a
    // transaction would see whatever it committed in between.
    let holder = crate::verbs_tests::writer_lock(&home);
    let calls = vec![
        vec!["park-show".into(), "12".into()],
        vec!["park-show".into(), "12".into()],
    ];
    let (rc, answers, err) = batch(&home, &request(&calls));
    drop(holder);
    assert_eq!(rc, 0);
    assert_eq!(err, "", "the verb wrote its failure on the error stream");
    assert_eq!(answers[0], answers[1]);
    assert_eq!(said(&answers[0]), said(&answers[1]));
    assert_eq!(said(&answers[0])["verb"], "park-show");
}

#[test]
fn a_read_of_a_batch_is_the_answer_of_the_same_call_alone() {
    // The contract the poll depends on: `out` is what the verb prints, parsed.
    // Every field of the four answers, and the refusal of a call whose issue
    // is not a number, are what the single call gives.
    let home = crate::verbs_tests::home("batch-same");
    asked(&home, 12, 4001);
    let calls = four(12, 4001);
    let (rc, answers, err) = batch(&home, &request(&calls));
    assert_eq!(rc, 0);
    assert_eq!(err, "", "the verb wrote its failure on the error stream");
    let alone = [
        crate::verbs_tests::ok(&home, &["plan-show", "12"]),
        crate::verbs_tests::ok(&home, &["park-show", "12"]),
        crate::verbs_tests::ok(&home, &["question-known", "12", "--id", "4001"]),
        crate::verbs_tests::ok(&home, &["approval-current", "12"]),
    ];
    for (a, b) in answers.iter().zip(alone) {
        assert_eq!(a["rc"], 0, "{a}");
        assert_eq!(said(a), b, "the batch's answer is not the single call's");
    }
    // An issue that is not a number is the single verb's 64, kept as this
    // call's answer: the line the verb prints, and its exit.
    let (rc, answers, _) = batch(&home, &request(&[vec!["park-show".into(), "x".into()]]));
    assert_eq!(rc, 0);
    assert_eq!(answers[0]["rc"], 64, "{}", answers[0]);
    assert!(
        answers[0]["out"]
            .as_str()
            .unwrap_or_default()
            .starts_with("usage park-show: "),
        "{}",
        answers[0]
    );
}

#[test]
fn a_batch_writes_nothing_however_its_calls_are_mixed() {
    // The claim the whole design rests on: a batch reads. The refusals are
    // answered without the store, the reads run inside one transaction that is
    // committed with nothing in it, and a call whose own row is damaged does not
    // make its neighbours answer differently. `table_counts` is the proof a write
    // cannot pass unseen: a row the batch wrote and COMMIT discarded would still
    // not show, so the case asks for the writes of the store by name -- a batch
    // that reached one would answer it as a read and be seen here.
    let home = crate::verbs_tests::home("batch-no-write");
    asked(&home, 12, 4001);
    let before = table_counts(&home);
    let calls = vec![
        vec!["park-retry".to_string(), "12".to_string()],
        vec!["park-told".to_string(), "12".to_string()],
        vec![
            "question-answered".to_string(),
            "12".to_string(),
            "--id".to_string(),
        ],
        vec!["park-show".to_string(), "12".to_string()],
        vec!["plan-show".to_string(), "12".to_string()],
    ];
    let (rc, answers, err) = batch(&home, &request(&calls));
    assert_eq!(rc, 0, "{err}");
    // The three refusals carry their own lines and no answer of a write; the two
    // reads answer as the single call does, and the read of the issue that has a
    // park is undamaged by the calls beside it. A verb outside the four is
    // refused by its name, whatever its arguments were meant to be.
    assert_eq!(answers[0]["out"], "park-retry #12: not a read of a batch");
    assert_eq!(answers[1]["out"], "park-told #12: not a read of a batch");
    assert_eq!(
        answers[2]["out"],
        "question-answered #12: not a read of a batch"
    );
    assert_eq!(
        said(&answers[3]),
        crate::verbs_tests::ok(&home, &["park-show", "12"])
    );
    assert_eq!(table_counts(&home), before, "a batch wrote");
}

#[test]
fn a_batch_of_13_issues_answers_each_issue_it_was_asked_about() {
    // The poll's own shape: 13 issues, the four reads of each. Every answer is
    // the answer of the issue its own arguments named, so a batch that answered
    // one issue's call with another's rows is seen.
    let home = crate::verbs_tests::home("batch-13");
    let mut calls: Vec<Vec<String>> = Vec::new();
    for i in 0..13i64 {
        let n = 100 + i;
        asked(&home, n, 4000 + i);
        calls.extend(four(n, 4000 + i));
    }
    let (rc, answers, err) = batch(&home, &request(&calls));
    assert_eq!(rc, 0, "{err}");
    assert_eq!(err, "", "the verb wrote its failure on the error stream");
    assert_eq!(answers.len(), 52);
    for (i, a) in answers.iter().enumerate() {
        assert_eq!(a["rc"], 0, "{i}: {a}");
    }
    for i in 0..13i64 {
        let n = 100 + i;
        let at = (i * 4) as usize;
        // The park and the approval name the issue they were read for; the
        // question of the issue's own attempt is the one the read says is
        // known; and the plan read of an issue with no session is the null the
        // single verb answers.
        assert_eq!(said(&answers[at])["verb"], "plan-show");
        assert_eq!(said(&answers[at])["planning"], Value::Null, "{n}");
        assert_eq!(said(&answers[at + 1])["park"]["issue"], json!(n), "{n}");
        assert_eq!(said(&answers[at + 2])["known"], json!(true), "{n}");
        assert_eq!(said(&answers[at + 3])["current"]["issue"], json!(n), "{n}");
        // Each of the four is the answer the same call alone gives, so the
        // order of the batch changed nothing but how often the store opened.
        let alone = |args: &[&str]| crate::verbs_tests::ok(&home, args);
        let q = (4000 + i).to_string();
        let n = n.to_string();
        assert_eq!(said(&answers[at]), alone(&["plan-show", &n]), "{n} plan");
        assert_eq!(
            said(&answers[at + 1]),
            alone(&["park-show", &n]),
            "{n} park"
        );
        assert_eq!(
            said(&answers[at + 2]),
            alone(&["question-known", &n, "--id", &q]),
            "{n} question"
        );
        assert_eq!(
            said(&answers[at + 3]),
            alone(&["approval-current", &n]),
            "{n} approval"
        );
    }
}
