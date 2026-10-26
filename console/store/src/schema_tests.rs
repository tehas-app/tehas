//! Schema 4: what init writes, and the steps from an older store, which must
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

/// The DDL of the released steps to 2 and to 3, as a store at that version holds
/// them.
const TO_2: &str = "
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
const TO_3: &str = "ALTER TABLE heartbeats ADD COLUMN text TEXT;";
// The step to 4 is the rebuild db.rs runs, as released.
const TO_4: &str = db::STEP_4;
const TO_5: &str = "
ALTER TABLE planning ADD COLUMN auto INTEGER NOT NULL DEFAULT 0;
ALTER TABLE plan_history ADD COLUMN auto INTEGER NOT NULL DEFAULT 0;
";
const TO_6: &str = db::STEP_6;
// The step to 8 is the ALTER db.rs runs, as released.
const TO_7: &str = db::STEP_7;

/// A home whose store is at schema `at`, 1 to 7, with `rows` of schema 1
/// written into it and `later` written after the steps.
fn old_home(name: &str, at: i64, rows: &str, later: &str) -> PathBuf {
    let home = v1_home(name, rows);
    let c = conn(&home);
    for (to, sql) in [
        (2, TO_2),
        (3, TO_3),
        (4, TO_4),
        (5, TO_5),
        (6, TO_6),
        (7, TO_7),
    ] {
        if to <= at {
            c.execute_batch(sql).unwrap();
        }
    }
    c.execute_batch(later).unwrap();
    c.execute(
        "UPDATE meta SET value = ?1 WHERE key = 'schema'",
        [at.to_string()],
    )
    .unwrap();
    c.pragma_update(None, "user_version", at).unwrap();
    home
}

/// The words of the fixer as a store at 3 holds them: a start, an answer and a
/// note.
const FIXER_ROWS: &str = "
INSERT INTO issues (issue, kind, kind_at, checked_at) VALUES (8, 'issue', 1, 1);
INSERT INTO cycles (issue, started_at, started_by, how) VALUES (8, 300, 'tehas-fixer', 'fixer');
INSERT INTO planning_answers (issue, turn, by_account, fixer, text) VALUES (7, 3, 'tehas-fixer', 1, 'b');
INSERT INTO planning_notes (issue, kind, comment_id, at) VALUES (7, 'fixer-refused', 55, 301);
";

/// The CHECK clauses of a table's CREATE, in order: a rebuilt table's text is
/// quoted differently, and ADD COLUMN changes its spacing, but its CHECKs are
/// written as schema.sql writes them.
fn checks(c: &Connection, table: &str) -> Vec<String> {
    let sql: String = c
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |r| r.get(0),
        )
        .unwrap();
    let mut out = Vec::new();
    let mut rest = sql.as_str();
    while let Some(i) = rest.find("CHECK (") {
        let tail = &rest[i..];
        let mut depth = 0;
        let end = tail
            .char_indices()
            .find(|&(_, ch)| {
                match ch {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
                ch == ')' && depth == 0
            })
            .map(|(j, _)| j + 1)
            .unwrap();
        out.push(tail[..end].to_string());
        rest = &tail[end..];
    }
    out
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
fn init_writes_schema_8() {
    assert_eq!(db::SCHEMA_VERSION, 8);
    let home = home("schema-8");
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), 8);
    assert_eq!(
        one::<String>(&home, "SELECT value FROM meta WHERE key = 'schema'"),
        "8"
    );
    assert_eq!(count(&home, "planning_notes"), 0);
    let c = conn(&home);
    assert!(columns(&c, "heartbeats").contains(&"text".to_string()));
    assert!(columns(&c, "planning_answers").contains(&"millwright".to_string()));
    assert!(!columns(&c, "planning_answers").contains(&"fixer".to_string()));
    for t in ["planning", "plan_history"] {
        assert!(columns(&c, t).contains(&"auto".to_string()), "{t}");
    }
}

#[test]
fn the_step_from_1_keeps_every_row_of_every_table() {
    let home = v1_home("step-rows", V1_ROWS);
    let before = conn(&home);
    let names = tables(&before);
    assert_eq!(names.len(), db::COUNTED_TABLES.len() - 2);
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
    assert_eq!((v["from"].as_i64(), v["to"].as_i64()), (Some(1), Some(8)));
    let after = conn(&home);
    for (t, cols, rows) in &held {
        // The one column the steps rename.
        let cols: Vec<String> = cols
            .iter()
            .map(|c| match (t.as_str(), c.as_str()) {
                ("planning_answers", "fixer") => "millwright".to_string(),
                _ => c.clone(),
            })
            .collect();
        assert_eq!(&dump(&after, t, &cols), rows, "{t}");
    }
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), 8);
    assert_eq!(
        one::<String>(&home, "SELECT value FROM meta WHERE key = 'schema'"),
        "8"
    );
    // meta keeps its rows; only `schema` moves with the version.
    let meta_after = dump(&after, "meta", &columns(&after, "meta"));
    assert_eq!(meta_after.len(), meta.len());
    for (was, is) in meta.iter().zip(&meta_after) {
        if was[0] == Sql::Text("schema".to_string()) {
            assert_eq!(is[1], Sql::Text("8".to_string()));
        } else {
            assert_eq!(was, is);
        }
    }
    // The new columns are empty on the old rows, and the new table is there.
    assert_eq!(count(&home, "approvals WHERE input_json IS NULL"), 1);
    assert_eq!(count(&home, "parks WHERE told_at IS NULL"), 1);
    assert_eq!(count(&home, "planning_answers WHERE text IS NULL"), 1);
    assert_eq!(count(&home, "planning_notes"), 0);
    assert_eq!(
        count(&home, "heartbeats WHERE text IS NULL"),
        count(&home, "heartbeats")
    );
    assert_eq!(
        count(&home, "planning WHERE auto = 0"),
        count(&home, "planning")
    );
    assert_eq!(
        count(&home, "plan_history WHERE auto = 0"),
        count(&home, "plan_history")
    );
    // Both copies: the one the answer names and the store as it was found.
    let found = home.join("state/backup").join(format!("tehas-1-{NOW}.db"));
    let reported = home.join("state/backup").join(format!("tehas-8-{NOW}.db"));
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
        report.contains("\"schema\":8,\"integrity\":\"ok\""),
        "{report}"
    );
}

#[test]
fn the_steps_from_2_add_the_heartbeat_s_text() {
    let home = old_home("step-from-2", 2, V1_ROWS, "");
    let rows = count(&home, "heartbeats");
    let line = migrate(&home).unwrap();
    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!((v["from"].as_i64(), v["to"].as_i64()), (Some(2), Some(8)));
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), 8);
    assert_eq!(count(&home, "heartbeats WHERE text IS NULL"), rows);
}

#[test]
fn the_step_from_3_keeps_the_fixer_s_rows_and_takes_the_millwright() {
    let home = old_home("step-from-3", 3, V1_ROWS, FIXER_ROWS);
    let children = [
        "cycle_locks",
        "heartbeats",
        "cycle_settings",
        "cycle_notes",
        "cycle_results",
        "cycle_progress",
    ];
    let before: Vec<i64> = children.iter().map(|t| count(&home, t)).collect();
    let line = migrate(&home).unwrap();
    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!((v["from"].as_i64(), v["to"].as_i64()), (Some(3), Some(8)));
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), 8);
    // Dropping `cycles` deletes nothing that references it.
    let after: Vec<i64> = children.iter().map(|t| count(&home, t)).collect();
    assert_eq!(after, before);
    assert!(before.iter().all(|&n| n > 0), "{before:?}");
    assert_eq!(count(&home, "cycles"), 2);
    assert_eq!(count(&home, "cycles WHERE issue = 8 AND how = 'fixer'"), 1);
    assert_eq!(
        count(&home, "planning_answers WHERE turn = 3 AND millwright = 1"),
        1
    );
    assert_eq!(
        count(&home, "planning_notes WHERE kind = 'fixer-refused'"),
        1
    );
    // The pages of the dropped tables are given back.
    assert_eq!(one::<i64>(&home, "PRAGMA freelist_count"), 0);
    // Both copies are of the store at 3.
    for v in [3, 8] {
        let copy = home.join(format!("state/backup/tehas-{v}-{NOW}.db"));
        let c = Connection::open(&copy).unwrap();
        let at: i64 = c
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(at, 3, "{}", copy.display());
    }
    // The verbs read the renamed column and keep the key `fixer`.
    let p = ok(&home, &["plan-show", "7"])["planning"].clone();
    assert_eq!(p["answers"][1]["by"], "tehas-fixer");
    assert_eq!(p["answers"][1]["fixer"], true);
    // The new words are accepted.
    let c = conn(&home);
    c.execute_batch(
        "INSERT INTO issues (issue, kind, kind_at, checked_at) VALUES (9, 'issue', 1, 1);
         INSERT INTO cycles (issue, started_at, started_by, how)
           VALUES (9, 400, 'tehas-millwright', 'millwright');
         INSERT INTO planning_notes (issue, kind, comment_id, at)
           VALUES (7, 'millwright-refused', 56, 401);",
    )
    .unwrap();
    assert!(c
        .execute(
            "INSERT INTO planning_notes (issue, kind, comment_id, at) VALUES (7, 'other', 57, 402)",
            [],
        )
        .is_err());
}

#[test]
fn a_step_to_4_that_fails_leaves_the_store_at_3_with_its_rows() {
    // A table of the step's own name already there: the step's CREATE fails
    // and nothing of it is kept.
    let home = old_home(
        "step-4-fails",
        3,
        V1_ROWS,
        "CREATE TABLE planning_notes_new (a);",
    );
    let (code, line) = migrate(&home).unwrap_err();
    assert_eq!(code, 78, "{line}");
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), 3);
    assert_eq!(
        one::<String>(&home, "SELECT value FROM meta WHERE key = 'schema'"),
        "3"
    );
    let c = conn(&home);
    assert!(columns(&c, "planning_answers").contains(&"fixer".to_string()));
    assert!(!checks(&c, "cycles")[0].contains("millwright"));
    assert_eq!(count(&home, "cycles"), 1);
    assert_eq!(count(&home, "heartbeats"), 1);
}

#[test]
fn the_step_from_4_adds_auto_to_planning() {
    let home = old_home("step-from-4", 4, V1_ROWS, "");
    let (sessions, turns) = (count(&home, "planning"), count(&home, "plan_history"));
    assert!(sessions > 0 && turns > 0);
    let line = migrate(&home).unwrap();
    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!((v["from"].as_i64(), v["to"].as_i64()), (Some(4), Some(8)));
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), 8);
    assert_eq!(count(&home, "planning WHERE auto = 0"), sessions);
    assert_eq!(count(&home, "plan_history WHERE auto = 0"), turns);
}

#[test]
fn the_step_from_5_numbers_the_ends_in_the_order_they_were_listed() {
    let home = old_home(
        "step-from-5",
        5,
        "",
        "INSERT INTO cycle_history (issue, attempt, at, status) VALUES (7, 1, 70, 'failed');
         INSERT INTO plan_history (issue, turn, at, outcome) VALUES (7, 1, 70, 'finished');
         INSERT INTO cycle_history (issue, attempt, at, status) VALUES (3, 1, 70, 'failed');
         INSERT INTO plan_history (issue, turn, at, outcome) VALUES (2, 1, 69, 'finished');",
    );
    let line = migrate(&home).unwrap();
    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!((v["from"].as_i64(), v["to"].as_i64()), (Some(5), Some(8)));
    let order = |c: &Connection| -> Vec<(String, i64, i64)> {
        let mut q = c
            .prepare(
                "SELECT 'cycle', issue, seq FROM cycle_history
                 UNION ALL SELECT 'plan', issue, seq FROM plan_history ORDER BY 3",
            )
            .unwrap();
        q.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    let c = conn(&home);
    // The end, then the issue, then a cycle before a planning turn.
    let s = |k: &str, i, n| (k.to_string(), i, n);
    assert_eq!(
        order(&c),
        vec![
            s("plan", 2, 1),
            s("cycle", 3, 2),
            s("cycle", 7, 3),
            s("plan", 7, 4)
        ]
    );
    // A row written after the step takes the next number, whatever its table.
    c.execute_batch(
        "INSERT INTO plan_history (issue, turn, at, outcome) VALUES (1, 2, 10, 'finished');
         INSERT INTO cycle_history (issue, attempt, at, status) VALUES (1, 2, 10, 'failed');",
    )
    .unwrap();
    assert_eq!(order(&c)[4..], [s("plan", 1, 5), s("cycle", 1, 6)]);
}

#[test]
fn a_migrated_store_has_the_schema_init_writes() {
    let made = home("init-shape");
    let b = conn(&made);
    let index_sql = |c: &Connection| -> Vec<Option<String>> {
        let mut q = c
            .prepare(
                "SELECT sql FROM sqlite_master WHERE type IN ('index', 'trigger') ORDER BY name",
            )
            .unwrap();
        q.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    for from in 1..db::SCHEMA_VERSION {
        let migrated = old_home(&format!("step-shape-{from}"), from, "", "");
        migrate(&migrated).unwrap();
        let a = conn(&migrated);
        assert_eq!(tables(&a), tables(&b), "from {from}");
        for t in tables(&b) {
            for pragma in ["table_info", "index_list", "foreign_key_list"] {
                assert_eq!(
                    described(&a, pragma, &t),
                    described(&b, pragma, &t),
                    "{pragma} of {t} from {from}"
                );
            }
            assert_eq!(checks(&a, &t), checks(&b, &t), "CHECKs of {t} from {from}");
        }
        assert_eq!(index_sql(&a), index_sql(&b), "from {from}");
    }
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
        line.contains("the store is at schema 1, this store writes 8; run store migrate"),
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
    // The rows of schema 1 read and write as rows of schema 8.
    let v = ok(&home, &["approval-current", "7"]);
    assert_eq!(v["current"]["used_by"], 2);
    assert_eq!(v["current"]["input"], serde_json::Value::Null);
    let v = ok(&home, &["park-told", "7"]);
    assert_eq!(v["said"], true);
    assert_eq!(v["park"]["body_sha256"], SHA);
    let p = ok(&home, &["plan-show", "7"])["planning"].clone();
    assert_eq!(p["answer"], serde_json::Value::Null);
    assert_eq!(p["answers"][0]["by"], "bob");
    assert_eq!(p["auto"], false);
}

#[test]
fn the_step_from_6_adds_the_cost_of_a_poll() {
    let home = old_home("step-from-6", 6, V1_ROWS, "");
    assert!(!tables(&conn(&home)).contains(&"poll_cost".to_string()));
    let line = migrate(&home).unwrap();
    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!((v["from"].as_i64(), v["to"].as_i64()), (Some(6), Some(8)));
    assert_eq!(count(&home, "poll_cost"), 0);
    assert_eq!(
        count(&home, "cycle_history WHERE seq IS NOT NULL"),
        count(&home, "cycle_history")
    );
}

#[test]
fn a_store_above_7_is_future() {
    let home = home("schema-8");
    conn(&home)
        .pragma_update(None, "user_version", db::SCHEMA_VERSION + 1)
        .unwrap();
    let (code, line) = migrate(&home).unwrap_err();
    assert_eq!(code, 78);
    assert!(line.starts_with("future "), "{line}");
}

#[cfg(debug_assertions)]
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
