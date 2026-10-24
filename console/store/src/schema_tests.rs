//! Schema 2: what init writes, and the step from a schema 1 store, which must
//! keep every row. The v1 DDL is kept as it was released to main, in
//! tests/schema_v1.sql.

use crate::db;
use crate::verbs_tests::*;
use crate::{execute, Settings, Verb};
use rusqlite::types::Value as Sql;
use rusqlite::Connection;
use std::fs::{self, OpenOptions};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::Path;
use std::path::PathBuf;

const V1: &str = include_str!("../tests/schema_v1.sql");

/// A row in every table of schema 1, so the step is seen to keep each.
const V1_ROWS: &str = "
INSERT INTO issues (issue, kind, kind_at, checked_at) VALUES (7, 'issue', 1, 1);
INSERT INTO cycles (issue, attempt, started_at, started_by, how) VALUES (7, 2, 100, 'alice', 'label');
INSERT INTO cycle_locks (issue, held, seen_at, probe_rc) VALUES (7, 1, 100, 1);
INSERT INTO heartbeats (issue, at, running) VALUES (7, 100, 1);
INSERT INTO endings (issue, started_at, ended_at, status) VALUES (7, 90, 95, 'closed');
INSERT INTO approvals (issue, at, by_account, how, label, body_sha256, used_at, used_by)
  VALUES (7, 50, 'alice', 'label', 'tehas:ready',
          'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 60, 2);
INSERT INTO planning (issue, session, turns, running, pending, label, opened_at)
  VALUES (7, 's1', 1, 1, 0, 'tehas:plan', 80);
INSERT INTO planning_answers (issue, turn, by_account) VALUES (7, 2, 'bob');
INSERT INTO planning_filed (issue, url, filed_issue, at)
  VALUES (7, 'https://github.com/o/r/issues/9', 9, 81);
INSERT INTO cycle_settings (issue, key, value) VALUES (7, 'K', 'V');
INSERT INTO cycle_notes (issue, kind, said_for, at) VALUES (7, 'quiet', 'x', 85);
INSERT INTO cycle_results (issue, received_at, schema_version, payload) VALUES (7, 101, 5, '{}');
INSERT INTO cycle_progress (issue, received_at, written_at) VALUES (7, 101, 100);
INSERT INTO parks (issue, attempt, at, reason) VALUES (7, 2, 102, 'failed');
INSERT INTO questions (id, issue, attempt, kind, posted_at) VALUES (900, 7, 2, 'park', 103);
INSERT INTO cycle_history (issue, attempt, at, status) VALUES (7, 1, 70, 'failed');
INSERT INTO plan_history (issue, turn, at, outcome, filed_json) VALUES (7, 1, 71, 'finished', '[]');
INSERT INTO refined (issue, at) VALUES (7, 72);
INSERT INTO polls (at, lock_taken_at) VALUES (200, 199);
INSERT INTO poll_demand (at, waiting) VALUES (200, 0);
INSERT INTO queue_snapshot (at) VALUES (200);
INSERT INTO health_samples (at, ok) VALUES (200, 1);
INSERT INTO energy_samples (at, ok) VALUES (200, 1);
INSERT INTO platform_state (key, value) VALUES ('k', 'v');
INSERT INTO pause_flag (id, at, reason, by_account) VALUES (1, 200, 'r', 'alice');
INSERT INTO limits (id, set_at) VALUES (1, 200);
INSERT INTO branch_checks (name, head_sha, at, state) VALUES ('main', 'abc', 200, 'done');
INSERT INTO check_product (at, ok) VALUES (200, 1);
INSERT INTO refresh_requests (issue, at, head_sha, pr, by_account) VALUES (7, 200, 'abc', 4, 'alice');
INSERT INTO refresh_hold (id, reason, since_at) VALUES (1, 'r', 200);
INSERT INTO endpoints (id, main, settled_at) VALUES (1, 'main', 200);
INSERT INTO index_state (\"commit\", state) VALUES ('abc', 'current');
INSERT INTO release_history (tag, at) VALUES ('v1', 200);
INSERT INTO edge_takeover (id, made_at) VALUES (1, 200);
INSERT INTO improve_cap (id, cap, set_at) VALUES (1, 3, 200);
INSERT INTO sweep_log (issue, at, action, what, reason) VALUES (7, 200, 'kept', 'ws', 'r');
INSERT INTO actions (id, poll_at, kind, issue, args_json, before_snapshot, state, intended_at)
  VALUES (1, 200, 'k', 7, '{}', 0, 'done', 200);
INSERT INTO journal (action, at, step) VALUES (1, 200, 's');
";

fn settings() -> Settings {
    Settings {
        version: "v0.11.0".to_string(),
        commit: "cb6912a".to_string(),
        instance: "tehas-tehas".to_string(),
        repo: "example-owner/example-product".to_string(),
    }
}

/// A home whose store is the one main's install.sh made: schema 1, with
/// `rows` written into it.
fn v1_home(name: &str, rows: &str) -> PathBuf {
    let home = scratch(name);
    let state = home.join("state");
    fs::DirBuilder::new().mode(0o700).create(&state).unwrap();
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(db::path(&home))
        .unwrap();
    let c = Connection::open(db::path(&home)).unwrap();
    c.pragma_update(None, "journal_mode", "WAL").unwrap();
    c.execute_batch(V1).unwrap();
    c.execute_batch(
        "INSERT INTO meta (key, value) VALUES ('schema', '1'), ('created_at', '1791000000'),
           ('tehas_version', 'v0.11.0'), ('tehas_commit', 'cb6912a'),
           ('instance', 'tehas-tehas'), ('repo', 'example-owner/example-product');",
    )
    .unwrap();
    c.execute_batch(rows).unwrap();
    c.pragma_update(None, "user_version", 1).unwrap();
    home
}

fn migrate(home: &Path) -> Result<String, (u8, String)> {
    execute(
        home,
        &Verb::Migrate {
            backup: home.join("state").join("backup"),
            settings: settings(),
        },
        NOW,
    )
}

fn tables(c: &Connection) -> Vec<String> {
    let mut q = c
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .unwrap();
    q.query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// Rows of a pragma that describes one table, as text.
fn described(c: &Connection, pragma: &str, table: &str) -> Vec<String> {
    let mut q = c
        .prepare(&format!("SELECT * FROM pragma_{pragma}('{table}')"))
        .unwrap();
    let n = q.column_count();
    let rows = q
        .query_map([], |r| {
            (0..n)
                .map(|i| r.get::<_, Sql>(i).map(|v| format!("{v:?}")))
                .collect::<Result<Vec<_>, _>>()
        })
        .unwrap();
    rows.map(|r| r.unwrap().join("|")).collect()
}

/// Every row of a table, the named columns only, in rowid order.
fn dump(c: &Connection, table: &str, columns: &[String]) -> Vec<Vec<Sql>> {
    let cols: Vec<String> = columns.iter().map(|c| format!("\"{c}\"")).collect();
    let mut q = c
        .prepare(&format!(
            "SELECT {} FROM {table} ORDER BY rowid",
            cols.join(", ")
        ))
        .unwrap();
    let rows = q
        .query_map([], |r| {
            (0..columns.len())
                .map(|i| r.get::<_, Sql>(i))
                .collect::<Result<Vec<_>, _>>()
        })
        .unwrap();
    rows.map(Result::unwrap).collect()
}

fn columns(c: &Connection, table: &str) -> Vec<String> {
    let mut q = c
        .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
        .unwrap();
    q.query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn init_writes_schema_2() {
    assert_eq!(db::SCHEMA_VERSION, 2);
    let home = home("schema-2");
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), 2);
    assert_eq!(
        one::<String>(&home, "SELECT value FROM meta WHERE key = 'schema'"),
        "2"
    );
    assert_eq!(count(&home, "planning_notes"), 0);
}

#[test]
fn the_step_from_1_keeps_every_row_of_every_table() {
    let home = v1_home("step-rows", V1_ROWS);
    let before = conn(&home);
    let names = tables(&before);
    assert_eq!(names.len(), db::COUNTED_TABLES.len() - 1);
    let meta = dump(&before, "meta", &columns(&before, "meta"));
    let mut held = Vec::new();
    for t in &names {
        if t == "meta" {
            continue;
        }
        let cols = columns(&before, t);
        let rows = dump(&before, t, &cols);
        assert!(!rows.is_empty(), "the fixture has no row in {t}");
        held.push((t.clone(), cols, rows));
    }
    drop(before);
    let line = migrate(&home).unwrap();
    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!((v["from"].as_i64(), v["to"].as_i64()), (Some(1), Some(2)));
    let after = conn(&home);
    for (t, cols, rows) in &held {
        assert_eq!(&dump(&after, t, cols), rows, "{t}");
    }
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), 2);
    assert_eq!(
        one::<String>(&home, "SELECT value FROM meta WHERE key = 'schema'"),
        "2"
    );
    // meta keeps its rows; only `schema` moves with the version.
    let meta_after = dump(&after, "meta", &columns(&after, "meta"));
    assert_eq!(meta_after.len(), meta.len());
    for (was, is) in meta.iter().zip(&meta_after) {
        if was[0] == Sql::Text("schema".to_string()) {
            assert_eq!(is[1], Sql::Text("2".to_string()));
        } else {
            assert_eq!(was, is);
        }
    }
    // The new columns are empty on the old rows, and the new table is there.
    assert_eq!(count(&home, "approvals WHERE input_json IS NULL"), 1);
    assert_eq!(count(&home, "parks WHERE told_at IS NULL"), 1);
    assert_eq!(count(&home, "planning_answers WHERE text IS NULL"), 1);
    assert_eq!(count(&home, "planning_notes"), 0);
    // Both copies: the one the answer names and the store as it was found.
    let found = home.join("state/backup").join(format!("tehas-1-{NOW}.db"));
    let reported = home.join("state/backup").join(format!("tehas-2-{NOW}.db"));
    assert_eq!(v["backup"], reported.to_str().unwrap());
    for copy in [&found, &reported] {
        let c = Connection::open(copy).unwrap();
        let v: i64 = c
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, 1, "{}", copy.display());
    }
    let report = execute(&home, &Verb::Check, NOW).unwrap();
    assert!(
        report.contains("\"schema\":2,\"integrity\":\"ok\""),
        "{report}"
    );
}

#[test]
fn a_migrated_store_has_the_schema_init_writes() {
    let migrated = v1_home("step-shape", "");
    migrate(&migrated).unwrap();
    let made = home("init-shape");
    let (a, b) = (conn(&migrated), conn(&made));
    assert_eq!(tables(&a), tables(&b));
    for t in tables(&b) {
        for pragma in ["table_info", "index_list", "foreign_key_list"] {
            assert_eq!(
                described(&a, pragma, &t),
                described(&b, pragma, &t),
                "{pragma} of {t}"
            );
        }
    }
    let index_sql = |c: &Connection| -> Vec<Option<String>> {
        let mut q = c
            .prepare("SELECT sql FROM sqlite_master WHERE type = 'index' ORDER BY name")
            .unwrap();
        q.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert_eq!(index_sql(&a), index_sql(&b));
}

#[test]
fn a_step_that_fails_leaves_the_store_at_1_with_its_rows() {
    // A table of the step's own name already there: the step's CREATE fails
    // after its ALTERs ran, and the transaction takes them back.
    let home = v1_home(
        "step-fails",
        "INSERT INTO issues (issue, kind, kind_at, checked_at) VALUES (7, 'issue', 1, 1);
         CREATE TABLE planning_notes (a);",
    );
    let (code, line) = migrate(&home).unwrap_err();
    assert_eq!(code, 78, "{line}");
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), 1);
    assert_eq!(
        one::<String>(&home, "SELECT value FROM meta WHERE key = 'schema'"),
        "1"
    );
    assert!(!columns(&conn(&home), "approvals").contains(&"input_json".to_string()));
    assert_eq!(count(&home, "issues"), 1);
}

#[test]
fn the_verbs_refuse_a_store_at_1_until_it_is_migrated() {
    let home = v1_home("verbs-at-1", V1_ROWS);
    let (code, line) = call(&home, &["plan-show", "7"]).unwrap_err();
    assert_eq!(code, 78);
    assert!(line.starts_with("unreadable "), "{line}");
    assert!(
        line.contains("the store is at schema 1, this store writes 2; run store migrate"),
        "{line}"
    );
    assert_eq!(refused(&home, &["park-told", "7"]).0, 78);
    let report = execute(&home, &Verb::Check, NOW).unwrap();
    assert!(
        report.contains("\"schema\":1,\"integrity\":\"ok\""),
        "{report}"
    );
    assert!(!report.contains("planning_notes"), "{report}");
    migrate(&home).unwrap();
    // The rows of schema 1 read and write as rows of schema 2.
    let v = ok(&home, &["approval-current", "7"]);
    assert_eq!(v["current"]["used_by"], 2);
    assert_eq!(v["current"]["input"], serde_json::Value::Null);
    let v = ok(&home, &["park-told", "7"]);
    assert_eq!(v["said"], true);
    assert_eq!(v["park"]["body_sha256"], SHA);
    let p = ok(&home, &["plan-show", "7"])["planning"].clone();
    assert_eq!(p["answer"], serde_json::Value::Null);
    assert_eq!(p["answers"][0]["by"], "bob");
}

#[test]
fn a_store_above_2_is_future() {
    let home = home("schema-3");
    conn(&home)
        .pragma_update(None, "user_version", db::SCHEMA_VERSION + 1)
        .unwrap();
    let (code, line) = migrate(&home).unwrap_err();
    assert_eq!(code, 78);
    assert!(line.starts_with("future "), "{line}");
}

#[test]
fn the_test_clock_is_read_from_its_variable() {
    use crate::test_clock;
    use std::ffi::OsStr;
    assert_eq!(test_clock(None), Ok(None));
    assert_eq!(
        test_clock(Some(OsStr::new("1792000123"))),
        Ok(Some(1_792_000_123))
    );
    for bad in ["", "0", "-5", "12x", "017", "123456789012"] {
        let err = test_clock(Some(OsStr::new(bad))).unwrap_err();
        assert!(err.contains("TEHAS_STORE_TEST_NOW"), "{bad}: {err}");
    }
}
