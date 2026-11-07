//! load: a copy into a store that holds no history, and every refusal leaving
//! the store as it was.

use crate::db;
use crate::verbs_tests::*;
use rusqlite::Connection;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// A copy of a store with a cycle that ended and one that runs no more.
fn a_copy(name: &str) -> (TestDir, PathBuf) {
    let from = home(name);
    let (lock, attempt) = start(&from, 7);
    drop(lock);
    end(&from, 7, attempt, "closed");
    let to = from.join("copy.db");
    let v = ok(&from, &["copy", "--to", to.to_str().unwrap()]);
    assert!(v["rows"].as_i64().unwrap() > 0);
    (from, to)
}

fn load(home: &Path, from: &Path) -> Result<Value, (u8, String)> {
    call(home, &["load", "--from", from.to_str().unwrap()])
}

/// The store's own file, byte for byte, and no file left beside it.
fn store(home: &Path) -> (Vec<u8>, Vec<String>) {
    let mut beside: Vec<String> = fs::read_dir(home.join("state"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("load"))
        .collect();
    beside.sort();
    // What is in the WAL is in the file once it is checkpointed.
    conn(home)
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
        .unwrap();
    (fs::read(db::path(home)).unwrap(), beside)
}

fn dump(c: &Connection, table: &str) -> Vec<String> {
    let mut q = c
        .prepare(&format!("SELECT * FROM {table} ORDER BY 1"))
        .unwrap();
    let n = q.column_count();
    q.query_map([], |r| {
        Ok((0..n)
            .map(|i| format!("{:?}", r.get::<_, rusqlite::types::Value>(i).unwrap()))
            .collect::<Vec<_>>()
            .join("|"))
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

#[test]
fn load_fills_a_new_store_with_every_row_of_the_copy() {
    let (from, copy) = a_copy("load-from");
    let into = home("load-new");
    let v = load(&into, &copy).unwrap();
    assert_eq!(v["file"], copy.to_str().unwrap());
    assert_eq!(v["schema"], db::SCHEMA_VERSION);
    let made = ok(
        &from,
        &["copy", "--to", from.join("again.db").to_str().unwrap()],
    );
    assert_eq!(v["rows"], made["rows"]);
    assert!(v["ms"].as_i64().is_some_and(|ms| ms >= 0));
    let (a, b) = (conn(&from), conn(&into));
    for table in db::COUNTED_TABLES {
        assert_eq!(dump(&a, table), dump(&b, table), "{table}");
    }
    assert_eq!(count(&into, "cycles WHERE issue = 7"), 1);
    let (_, beside) = store(&into);
    assert!(beside.is_empty(), "{beside:?}");
}

#[test]
fn load_replaces_the_samples_a_drained_poll_wrote() {
    let (_from, copy) = a_copy("load-samples-from");
    let into = home("load-samples");
    conn(&into)
        .execute_batch(
            "INSERT INTO polls (at, lock_taken_at) VALUES (5, 4);
             INSERT INTO health_samples (at, ok) VALUES (5, 1);
             INSERT INTO issues (issue, kind, kind_at, checked_at) VALUES (99, 'issue', 1, 1);",
        )
        .unwrap();
    load(&into, &copy).unwrap();
    assert_eq!(count(&into, "polls WHERE at = 5"), 0);
    assert_eq!(count(&into, "issues WHERE issue = 99"), 0);
    assert_eq!(count(&into, "cycles WHERE issue = 7"), 1);
}

#[test]
fn load_refuses_a_store_that_holds_history_and_leaves_it() {
    let (_from, copy) = a_copy("load-history-from");
    let into = home("load-history");
    let (lock, attempt) = start(&into, 9);
    drop(lock);
    end(&into, 9, attempt, "closed");
    let before = store(&into);
    let (code, why) = load(&into, &copy).expect_err("refused");
    assert_eq!(code, 1);
    assert!(why.starts_with("history: "), "{why}");
    assert!(why.contains("rows in cycles"), "{why}");
    assert!(!why.contains('\n'));
    assert_eq!(store(&into), before);
    assert_eq!(count(&into, "cycles WHERE issue = 7"), 0);
}

#[test]
fn load_refuses_while_a_cycle_lock_is_held() {
    let (_from, copy) = a_copy("load-running-from");
    let into = home("load-running");
    let held = hold(&into, 12);
    let before = store(&into);
    let (code, why) = load(&into, &copy).expect_err("refused");
    assert_eq!(
        (code, why.as_str()),
        (1, "running: a cycle is running for #12")
    );
    assert_eq!(store(&into), before);
    drop(held);
    load(&into, &copy).unwrap();
}

#[test]
fn load_refuses_a_copy_of_a_newer_schema_naming_both() {
    let (_from, copy) = a_copy("load-newer-from");
    Connection::open(&copy)
        .unwrap()
        .pragma_update(None, "user_version", db::SCHEMA_VERSION + 1)
        .unwrap();
    let into = home("load-newer");
    let before = store(&into);
    let (code, why) = load(&into, &copy).expect_err("refused");
    assert_eq!(code, 1);
    assert_eq!(
        why,
        format!(
            "newer-copy: the copy {} holds schema {}, this release reads schema {} and older",
            copy.display(),
            db::SCHEMA_VERSION + 1,
            db::SCHEMA_VERSION
        )
    );
    assert_eq!(store(&into), before);
}

#[test]
fn an_unreadable_copy_leaves_the_store_untouched() {
    let (from, copy) = a_copy("load-bad-from");
    let into = home("load-bad");
    let before = store(&into);
    let refused = |p: &Path| {
        let (code, why) = load(&into, p).expect_err("refused");
        assert_eq!(code, 1, "{why}");
        assert!(!why.contains('\n'));
        why
    };
    let gone = from.join("gone.db");
    assert!(refused(&gone).starts_with("no-copy: "));
    let text = write_file(&from, "text.db", b"not a store");
    assert!(refused(&text).starts_with("bad-copy: "));
    assert!(refused(&from.join("state")).starts_with("bad-copy: "));
    // A copy cut short, and one with a page of its rows overwritten.
    let bytes = fs::read(&copy).unwrap();
    let short = write_file(&from, "short.db", &bytes[..bytes.len() / 2]);
    assert!(refused(&short).starts_with("bad-copy: "));
    let c = Connection::open(&copy).unwrap();
    let page = db::pages::page_of(&c, "cycles").unwrap();
    drop(c);
    let at = db::pages::page_offset(&copy, page).unwrap() as usize;
    let mut damaged = bytes.clone();
    damaged[at..at + 64].fill(0xff);
    let damaged = write_file(&from, "damaged.db", &damaged);
    assert!(refused(&damaged).starts_with("bad-copy: "));
    let none = write_file(&from, "none.db", b"");
    assert!(refused(&none).starts_with("bad-copy: "));
    assert_eq!(store(&into), before);
}

#[test]
fn load_needs_an_absolute_name() {
    assert!(usage(&["load"]).contains("--from"));
    assert!(usage(&["load", "--from", "copy.db"]).contains("absolute"));
}
