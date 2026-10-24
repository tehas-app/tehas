//! Schema 3: what init writes, and the steps from a schema 1 store and a
//! schema 2 one, each of which must keep every row. The v1 DDL is kept as it was
//! released to main, in tests/schema_v1.sql; the v2 DDL is the same file with
//! the four additions of db::STEP_2, which is what a store on main today holds.

use crate::db;
use crate::verbs_tests::*;
use crate::{execute, Settings, Verb};
use rusqlite::types::Value as Sql;
use rusqlite::{params, Connection};
use std::fs::{self, OpenOptions};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::Path;
use std::path::PathBuf;

const V1: &str = include_str!("../tests/schema_v1.sql");

/// The v2 DDL of db::STEP_2: the three columns and the table that took the
/// store from 1 to 2, in the words the step itself uses. A schema 1 fixture
/// with these run over it is a store at 2, which is what the step from 2 is
/// asked of.
const V2_STEP: &str = db::STEP_2;

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

/// Rows a schema 3 fixture starts from: the issues the other rows hang on and
/// the planning sessions the plan verbs read back. Issue 9 is the fresh one of
/// `says_the_words_of_schema_3`, which records an approval and an answer of its
/// own once the store is at 3. The cycle of issue 7 is `CYCLE_ROW`, which only
/// the fixtures of the step from 2 carry: it is part of what makes the approval
/// of that issue a start.
const PLAN_ROWS: &str = "
INSERT INTO issues (issue, kind, kind_at, checked_at) VALUES (7, 'issue', 1, 1);
INSERT INTO issues (issue, kind, kind_at, checked_at) VALUES (9, 'issue', 1, 1);
INSERT INTO planning (issue, session, turns, running, pending, label, opened_at)
  VALUES (7, 's1', 2, 1, 0, 'tehas:plan', 80);
INSERT INTO planning (issue, session, turns, running, pending, label, opened_at)
  VALUES (9, 's9', 1, 0, 0, 'tehas:plan', 80);
";

/// The cycle of issue 7, the one an approval of that issue is a start of.
const CYCLE_ROW: &str =
    "INSERT INTO cycles (issue, attempt, started_at, started_by, how) VALUES (7, 2, 100, 'alice', 'label');";

/// The three records the step from 2 to 3 is about, each under the name it had
/// before the step: an approval of how 'fixer', an answer whose value is the
/// answer's yes, and a note of each kind the old store could say. The approval
/// of issue 7 is the one record a host's store of schema 2 holds whose `how`
/// comes from the old word, and its cycle is the row that makes it one. No row
/// of a store made before schema 3 holds either new word, which is what
/// `holds_no_new_word` asks of a fixture before it is migrated.
const V2_ROWS: &str = "
INSERT INTO approvals (issue, at, by_account, how, label, body_sha256, used_at, used_by, input_json)
  VALUES (7, 50, 'alice', 'fixer', 'tehas:ready',
          'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
          60, 2, '{}');
INSERT INTO planning_answers (issue, turn, by_account, fixer, text) VALUES (7, 2, 'bob', 1, 'yes');
INSERT INTO planning_notes (issue, kind, comment_id, at) VALUES (7, 'fixer-refused', 55, 82);
INSERT INTO planning_notes (issue, kind, comment_id, at) VALUES (7, 'held', 66, 82);
";

/// Every row of `approvals` and of `planning_answers` at once, so a fixture
/// that reads one of the two back can be compared with the other: the step
/// rebuilds the first and renames a column of the second, and neither is
/// allowed to move a value the store already held.
fn dump_two(c: &Connection) -> (Vec<String>, Vec<Vec<Sql>>, Vec<String>, Vec<Vec<Sql>>) {
    let ac = columns(c, "approvals");
    let a = dump(c, "approvals", &ac);
    let pc = columns(c, "planning_answers");
    let p = dump(c, "planning_answers", &pc);
    (ac, a, pc, p)
}

/// The text of every table and index of a store, each reduced to what it says
/// about the shape and nothing else: its own name, taken from its row of the
/// schema, is dropped along with the quoting a rename leaves on it, and a run
/// of whitespace is one space. A CHECK lives in the text of its CREATE, so two
/// stores hold the same schema only where the two sets agree -- and the words
/// of a CHECK are not touched, which is the point: the store brought from 2
/// must name the same kinds and the same ways as the one `init` made.
fn ddl(c: &Connection) -> Vec<(String, Option<String>)> {
    let mut q = c
        .prepare("SELECT name, sql FROM sqlite_master ORDER BY name")
        .unwrap();
    let rows = q
        .query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })
        .unwrap()
        .collect::<Result<Vec<(String, Option<String>)>, _>>()
        .unwrap();
    rows.into_iter()
        .map(|(name, sql)| {
            let sql = sql.map(|s| {
                s.replace('"', "")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            });
            (name, sql)
        })
        .collect()
}

/// How the two old fixtures reach the version they are the fixture of.
enum At {
    /// The DDL of schema 1, as released: the store main's install.sh made.
    One,
    /// The same DDL with db::STEP_2 over it, which is a store at 2.
    Two,
}

/// A home whose store is one of the two schemas the steps are asked of, with
/// `rows` written into it. The row of `meta` and the `user_version` say which.
fn old_home(name: &str, at: At, rows: &str) -> PathBuf {
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
    let version = match at {
        At::One => 1,
        At::Two => {
            c.execute_batch(V2_STEP).unwrap();
            2
        }
    };
    c.execute_batch(
        "INSERT INTO meta (key, value) VALUES ('schema', '1'), ('created_at', '1791000000'),
           ('tehas_version', 'v0.11.0'), ('tehas_commit', 'cb6912a'),
           ('instance', 'tehas-tehas'), ('repo', 'example-owner/example-product');",
    )
    .unwrap();
    c.execute_batch(rows).unwrap();
    // The row of meta and the pragma are set together, and the step is what
    // moves both, so the fixture states the version it is a store at.
    if version != 1 {
        c.execute("UPDATE meta SET value = '2' WHERE key = 'schema'", [])
            .unwrap();
    }
    c.pragma_update(None, "user_version", version).unwrap();
    home
}

/// A home whose store is the one main's install.sh made: schema 1, with
/// `rows` written into it.
fn v1_home(name: &str, rows: &str) -> PathBuf {
    old_home(name, At::One, rows)
}

/// A home whose store holds schema 2, as a host that installed the release of
/// schema 2 has it. The issues and the planning session of PLAN_ROWS, the three
/// records of V2_ROWS and the two notes of WORDS are in it, which is what the
/// step from 2 is asked of and what the verbs read back afterwards.
fn v2_home(name: &str, rows: &str) -> PathBuf {
    old_home(
        name,
        At::Two,
        &[PLAN_ROWS, CYCLE_ROW, V2_ROWS, rows].join("\n"),
    )
}

/// Rows enough to hold the old words, not the new ones: no record made before
/// schema 3 can carry either, and a store whose row says `millwright` before
/// the step is a store that never was.
fn holds_no_new_word(c: &Connection) {
    let said: i64 = c
        .query_row(
            "SELECT count(*) FROM approvals WHERE how = 'millwright'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(said, 0);
    let said: i64 = c
        .query_row(
            "SELECT count(*) FROM planning_notes WHERE kind = 'millwright-refused'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(said, 0);
}

/// The two CHECKs of the step, and the name of the answer's column, read back
/// from a store at 3: the words the old store could not say are sayable, the
/// words it could say are still sayable, and one word more is still refused.
/// Everything it writes is of issue 9, which no fixture of either step uses, so
/// the helper asks the same questions of a store `init` made, of one brought
/// from 1 and of one brought from 2. Issue 9 has no cycle, so its approval is
/// recorded as a start owed and not as one made.
fn says_the_words_of_schema_3(home: &Path) {
    let mut c = db::write_rows(&db::path(home)).unwrap();
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    // The issue and its session are there for a store whose fixture brought
    // them in, and made here for the one `init` made from nothing.
    tx.execute(
        "INSERT OR IGNORE INTO issues (issue, kind, kind_at, checked_at) VALUES (9, 'issue', 1, 1)",
        [],
    )
    .unwrap();
    tx.execute(
        "INSERT OR IGNORE INTO planning (issue, session, turns, running, pending, label, opened_at)
         VALUES (9, 's9', 1, 0, 0, 'tehas:plan', 80)",
        [],
    )
    .unwrap();
    tx.execute(
        "INSERT INTO approvals (issue, at, by_account, how, label, body_sha256, input_json)
         VALUES (9, 52, 'dave', 'millwright', 'tehas:ready', ?1, NULL)",
        params![SHA],
    )
    .expect("an approval of how 'millwright' is accepted at 3");
    tx.execute(
        "INSERT INTO approvals (issue, at, by_account, how, label, body_sha256, input_json)
         VALUES (9, 53, 'dave', 'fixer', 'tehas:ready', ?1, NULL)",
        params![SHA],
    )
    .expect("an approval of how 'fixer' is still accepted at 3");
    tx.execute(
        "INSERT INTO planning_answers (issue, turn, by_account, millwright, text)
         VALUES (9, 2, 'dave', 1, 'yes')",
        [],
    )
    .expect("the answer's column answers to 'millwright' at 3");
    tx.execute(
        "INSERT INTO planning_notes (issue, kind, comment_id, at) VALUES (9, 'millwright-refused', 57, 83)",
        [],
    )
    .expect("a note of kind 'millwright-refused' is accepted at 3");
    tx.execute(
        "INSERT INTO planning_notes (issue, kind, comment_id, at) VALUES (9, 'fixer-refused', 58, 83)",
        [],
    )
    .expect("a note of kind 'fixer-refused' is still accepted at 3");
    // The rows above are one transaction, given back before the refusals are
    // asked for: a statement run with one open is answered by the transaction,
    // and a CHECK that stops a statement inside it hurts the whole of it.
    tx.commit().unwrap();
    // The two refusals are asked for of a connection that may write and holds
    // no transaction of its own: a CHECK belongs to the table, and a statement
    // asked of a connection with a transaction open is answered by that
    // transaction before SQLite has read the statement.
    let asker = db::write_ask(&db::path(home)).unwrap();
    for (sql, what) in [
        // The CHECK of the step is the one of a note's kind. `approvals.how`
        // carries no CHECK in either schema: the words of a start are the
        // store's own, and a verb of the store refuses one outside them with
        // exit 64 before a row is written (approval.rs), so an INSERT of a
        // word outside that list is accepted here and is not a failure of the
        // step. What is refused is a kind no table will take.
        (
            "INSERT INTO planning_notes (issue, kind, comment_id, at) VALUES (9, 'nope', 59, 83)",
            "a kind outside the list",
        ),
        (
            "INSERT INTO questions (id, issue, attempt, kind, posted_at) VALUES (977, 9, 1, 'nope', 1)",
            "a kind of a question outside its list",
        ),
    ] {
        let err = asker
            .execute(sql, [])
            .err()
            .unwrap_or_else(|| panic!("{what} is accepted at 3"));
        assert!(
            err.to_string().contains("CHECK constraint failed"),
            "{what}: {err}"
        );
    }
    // `approvals.how` carries no CHECK at all, in schema.sql or in the rebuild:
    // the set of words is the store's own, and a verb of the store refuses one
    // outside it with exit 64 before a row is written (approval.rs). What the
    // step rebuilt is the column and every row of it, and the acceptance of
    // 'millwright' beside 'fixer' above is the half of the acceptance a CHECK
    // does hold; the kind of a note is the other, and it is refused above.
    let words: Vec<String> = asker
        .prepare("SELECT how FROM approvals WHERE issue = 9 ORDER BY at")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(words, ["millwright".to_string(), "fixer".to_string()]);
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
fn init_writes_schema_3() {
    assert_eq!(db::SCHEMA_VERSION, 3);
    let home = home("schema-3");
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), 3);
    assert_eq!(
        one::<String>(&home, "SELECT value FROM meta WHERE key = 'schema'"),
        "3"
    );
    assert_eq!(count(&home, "planning_notes"), 0);
    // The column of planning_answers is the new name, and the old one is gone.
    assert!(columns(&conn(&home), "planning_answers").contains(&"millwright".to_string()));
    assert!(!columns(&conn(&home), "planning_answers").contains(&"fixer".to_string()));
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
    assert_eq!((v["from"].as_i64(), v["to"].as_i64()), (Some(1), Some(3)));
    // The two tables the step is about, read out of the store as it was before
    // it ran: the rebuild and the rename are compared with these below.
    let rebuilt = columns(&conn(&home), "approvals");
    let was_approvals = dump(&conn(&home), "approvals", &rebuilt);
    let renamed = columns(&conn(&home), "planning_answers");
    let was_answers = dump(&conn(&home), "planning_answers", &renamed);
    let after = conn(&home);
    for (t, cols, rows) in &held {
        // The two tables the step is about are asked of by name below, where
        // the one thing it did to each of them can be named and everything else
        // about them is compared with the store as it was.
        if t == "approvals" || t == "planning_answers" {
            continue;
        }
        assert_eq!(&dump(&after, t, cols), rows, "{t}");
    }
    // `approvals` is the rebuild: read back through the columns of the store at
    // 3, its one row is the row of schema 1 -- `how` of 'label', the approval
    // the fixture gave, and the column of input_json the first step added.
    assert_eq!(dump(&after, "approvals", &rebuilt), was_approvals);
    assert_eq!(count(&home, "approvals WHERE how = 'label'"), 1);
    assert_eq!(count(&home, "approvals WHERE input_json IS NULL"), 1);
    // `planning_answers` is the rename: the row of schema 1 never gave a value,
    // and at 3 it is the 0 of a column nobody filled, under the new name. Its
    // other columns are the columns they were, in the order they were.
    let now = columns(&after, "planning_answers");
    assert!(now.contains(&"millwright".to_string()), "{now:?}");
    assert!(!now.contains(&"fixer".to_string()), "{now:?}");
    assert_eq!(
        was_answers[0][..3],
        dump(&after, "planning_answers", &now)[0][..3]
    );
    assert_eq!(
        was_answers[0][4..],
        dump(&after, "planning_answers", &now)[0][4..]
    );
    assert_eq!(count(&home, "planning_answers WHERE millwright = 0"), 1);
    assert_eq!(count(&home, "planning_answers WHERE text IS NULL"), 1);
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), 3);
    assert_eq!(
        one::<String>(&home, "SELECT value FROM meta WHERE key = 'schema'"),
        "3"
    );
    // meta keeps its rows; only `schema` moves with the version.
    let meta_after = dump(&after, "meta", &columns(&after, "meta"));
    assert_eq!(meta_after.len(), meta.len());
    for (was, is) in meta.iter().zip(&meta_after) {
        if was[0] == Sql::Text("schema".to_string()) {
            assert_eq!(is[1], Sql::Text("3".to_string()));
        } else {
            assert_eq!(was, is);
        }
    }
    // The new columns are empty on the old rows, and the new table is there.
    assert_eq!(count(&home, "parks WHERE told_at IS NULL"), 1);
    assert_eq!(count(&home, "planning_notes"), 0);
    // Both copies: the one the answer names and the store as it was found.
    let found = home.join("state/backup").join(format!("tehas-1-{NOW}.db"));
    let reported = home.join("state/backup").join(format!("tehas-3-{NOW}.db"));
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
        report.contains("\"schema\":3,\"integrity\":\"ok\""),
        "{report}"
    );
    says_the_words_of_schema_3(&home);
}

#[test]
fn the_step_from_2_keeps_the_approval_the_answer_and_the_note() {
    // The three rows the step is about: an approval whose how is the old word,
    // an answer whose value is the answer's yes, and a note of the old kind.
    // Every one of them is kept, and the answer's value is in `millwright`.
    // What the two rebuilt tables and the renamed one hold, column by column,
    // before anything runs: the step is allowed to move a row and not to change
    // one.
    let home = v2_home("step-2-rows", "");
    let before = conn(&home);
    holds_no_new_word(&before);
    let (approval_cols, approvals, answer_cols, answers) = dump_two(&before);
    // The fixture is a store of schema 2: its answer's column answers to
    // `fixer`, which is the name the step takes away.
    assert!(
        answer_cols.contains(&"fixer".to_string()),
        "the fixture is not a store of schema 2: {answer_cols:?}"
    );
    assert_eq!(approvals.len(), 1, "the fixture has no approval");
    assert_eq!(approvals[0][3], Sql::Text("fixer".to_string()));
    let note_cols = columns(&before, "planning_notes");
    let notes = dump(&before, "planning_notes", &note_cols);
    // The two notes of the fixture: the two of issue 7, each a kind the store
    // of schema 2 could already say.
    assert_eq!(notes.len(), 2, "the fixture has no note of each kind");
    let yes: i64 = before
        .query_row(
            "SELECT fixer FROM planning_answers WHERE issue = 7 AND turn = 2",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(yes, 1, "the fixture's answer is not a yes");
    drop(before);

    let line = migrate(&home).unwrap();
    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!((v["from"].as_i64(), v["to"].as_i64()), (Some(2), Some(3)));
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), 3);
    assert_eq!(
        one::<String>(&home, "SELECT value FROM meta WHERE key = 'schema'"),
        "3"
    );
    let after = conn(&home);
    // The approval is the row it was, `how` and all, and the index of the
    // rebuilt table still answers to the name it always had.
    assert_eq!(dump(&after, "approvals", &approval_cols), approvals);
    assert_eq!(count(&home, "approvals WHERE how = 'fixer'"), 1);
    let open: i64 = after
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND name = 'approvals_open'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(open, 1, "the index of the rebuilt approvals is not there");
    // The note of the old kind is the note it was, and so is the held one.
    assert_eq!(dump(&after, "planning_notes", &note_cols), notes);
    assert_eq!(
        count(&home, "planning_notes WHERE kind = 'fixer-refused'"),
        1
    );
    assert_eq!(count(&home, "planning_notes WHERE kind = 'held'"), 1);
    // The answer of the renamed table is the answer it was, under the new name
    // of the one column the step touched: the two dumps differ in that name.
    let later = columns(&after, "planning_answers");
    assert!(later.contains(&"millwright".to_string()), "{later:?}");
    assert!(!later.contains(&"fixer".to_string()), "{later:?}");
    let held = dump(&after, "planning_answers", &later);
    assert_eq!(held.len(), answers.len());
    for (was, is) in answers.iter().zip(&held) {
        // Every column but the answer's own is the same value in the same place.
        assert_eq!(&was[..3], &is[..3]);
        assert_eq!(&was[4..], &is[4..]);
    }
    let yes: i64 = after
        .query_row(
            "SELECT millwright FROM planning_answers WHERE issue = 7 AND turn = 2",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(yes, 1);
    drop(after);
    let report = execute(&home, &Verb::Check, NOW).unwrap();
    assert!(
        report.contains("\"schema\":3,\"integrity\":\"ok\""),
        "{report}"
    );
    says_the_words_of_schema_3(&home);
}

#[test]
fn the_two_copies_of_a_store_brought_from_2_are_of_the_store_as_it_was() {
    // A verb that changed the version leaves two copies, both of the database
    // as it was found: the one the answer names, under the version the store
    // ends at, and the one under the version it came with. Both predate the
    // step, so both hold schema 2 with the old name of the column in them.
    let home = v2_home("step-2-backup", "");
    let line = migrate(&home).unwrap();
    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
    let reported = home.join("state/backup").join(format!("tehas-3-{NOW}.db"));
    assert_eq!(v["backup"], reported.to_str().unwrap());
    let found = home.join("state/backup").join(format!("tehas-2-{NOW}.db"));
    for copy in [&found, &reported] {
        let c = Connection::open(copy).unwrap();
        let held: i64 = c
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(held, 2, "{}", copy.display());
        // The copy is the store a person restores, and it is the store before
        // the step: the column is still `fixer`, and its value is the 1 it had.
        let cols = columns(&c, "planning_answers");
        assert!(cols.contains(&"fixer".to_string()), "{cols:?}");
        assert!(!cols.contains(&"millwright".to_string()), "{cols:?}");
        let yes: i64 = c
            .query_row(
                "SELECT fixer FROM planning_answers WHERE turn = 2",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(yes, 1);
        drop(c);
    }
    // The store itself holds the new name and not the old one.
    let cols = columns(&conn(&home), "planning_answers");
    assert!(cols.contains(&"millwright".to_string()), "{cols:?}");
    assert!(!cols.contains(&"fixer".to_string()), "{cols:?}");
}

#[test]
fn a_migrated_store_has_the_schema_init_writes() {
    // Both ways to 3: the two steps of a store at 1, and the one step of a
    // store at 2. Each is asked of the store `init` made, table by table and
    // column by column, so a migrated store and one made whole cannot be told
    // apart by any reader that asks the schema what it holds.
    for (name, from) in [("step-shape", 1), ("step-2-shape", 2)] {
        let migrated = match from {
            1 => v1_home(name, ""),
            _ => v2_home(name, ""),
        };
        migrate(&migrated).unwrap();
        let made = home(name);
        let (a, b) = (conn(&migrated), conn(&made));
        assert_eq!(tables(&a), tables(&b), "{name}");
        for t in tables(&b) {
            for pragma in ["table_info", "index_list", "foreign_key_list"] {
                assert_eq!(
                    described(&a, pragma, &t),
                    described(&b, pragma, &t),
                    "{pragma} of {t} of {name}"
                );
            }
        }
        // The CHECKs of a table live in the text of its CREATE, so the two are
        // the same table only where the two lines agree; `ddl` takes back the
        // two differences a rename and a rebuild are allowed to leave.
        assert_eq!(ddl(&a), ddl(&b), "{name}");
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
fn a_step_that_fails_leaves_the_store_at_2_with_its_rows() {
    // The rebuild's own shape of the same case: the table the step means to
    // create is already there under that name, so its CREATE is refused. Every
    // row is where it was, the CHECKs are still the two of schema 2, and the
    // answer's column still answers to `fixer`.
    let home = v2_home("step-3-fails", "CREATE TABLE approvals_new (a);");
    let (code, line) = migrate(&home).unwrap_err();
    assert_eq!(code, 78, "{line}");
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), 2, "{line}");
    assert_eq!(
        one::<String>(&home, "SELECT value FROM meta WHERE key = 'schema'"),
        "2"
    );
    // Nothing of the step was committed: the rows are there, no half-built
    // table is, and the CHECK of 2 still refuses the word of 3.
    assert_eq!(count(&home, "approvals"), 1);
    assert_eq!(count(&home, "planning_notes"), 2);
    assert_eq!(count(&home, "approvals_new"), 0);
    let cols = columns(&conn(&home), "planning_answers");
    assert!(cols.contains(&"fixer".to_string()), "{cols:?}");
    assert!(!cols.contains(&"millwright".to_string()), "{cols:?}");
    let c = db::open(&db::path(&home), db::Purpose::Write).unwrap();
    let err = c
        .execute(
            "INSERT INTO planning_notes (issue, kind, comment_id, at) VALUES (7, 'millwright-refused', 57, 83)",
            [],
        )
        .expect_err("a store left at 2 holds the CHECK of 2");
    assert!(err.to_string().contains("CHECK constraint failed"), "{err}");
    drop(c);
    // The two copies the verb made before it failed are both there, of the
    // store as it was: one under the version the step was going to write, which
    // is the name the answer carries, and one under the version the store came
    // with. Both predate the step, so either is what a person restores from.
    assert!(home
        .join("state/backup")
        .join(format!("tehas-3-{NOW}.db"))
        .exists());
    assert!(home
        .join("state/backup")
        .join(format!("tehas-2-{NOW}.db"))
        .exists());
}

#[test]
fn the_verbs_refuse_a_store_at_1_until_it_is_migrated() {
    let home = v1_home("verbs-at-1", V1_ROWS);
    let (code, line) = call(&home, &["plan-show", "7"]).unwrap_err();
    assert_eq!(code, 78);
    assert!(line.starts_with("unreadable "), "{line}");
    assert!(
        line.contains("the store is at schema 1, this store writes 3; run store migrate"),
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
    // The rows of schema 1 read and write as rows of schema 3.
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
fn the_verbs_read_a_migrated_store_under_the_new_column_name() {
    // plan-show is the verb that reads the answer's column. The key it prints
    // stays `fixer` until the verbs speak the new word (#686), and it is the
    // value of `millwright` under it: a reader of the JSON cannot tell the two
    // stores apart, which is what the rename is for.
    let home = v2_home("verbs-at-2", "");
    let (code, line) = call(&home, &["plan-show", "7"]).unwrap_err();
    assert_eq!(code, 78, "{line}");
    assert!(line.starts_with("unreadable "), "{line}");
    assert!(
        line.contains("the store is at schema 2, this store writes 3; run store migrate"),
        "{line}"
    );
    let report = execute(&home, &Verb::Check, NOW).unwrap();
    assert!(
        report.contains("\"schema\":2,\"integrity\":\"ok\""),
        "{report}"
    );
    migrate(&home).unwrap();
    let p = ok(&home, &["plan-show", "7"])["planning"].clone();
    // The one answer of the fixture, whose value is 1 and whose text is the
    // reply of the person: the reader of plan.rs asks the table for `millwright`
    // and prints what it got under the key the verbs still answer to.
    assert_eq!(
        p["answers"],
        serde_json::json!([{"turn": 2, "by": "bob", "fixer": true}]),
        "{p}"
    );
    // The text the reader asks of the answer is still read from the same table.
    assert_eq!(count(&home, "planning_answers WHERE text IS NOT NULL"), 1);
}

#[test]
fn a_store_above_3_is_future() {
    let home = home("schema-4");
    conn(&home)
        .pragma_update(None, "user_version", db::SCHEMA_VERSION + 1)
        .unwrap();
    let (code, line) = migrate(&home).unwrap_err();
    assert_eq!(code, 78);
    assert!(line.starts_with("future "), "{line}");
}

#[test]
fn a_store_at_3_is_left_as_it_is() {
    // The verb asked of a store that needs nothing: it reports the version it
    // found, runs no step, and writes the copy it promised and nothing else.
    let home = home("at-3");
    let file = db::path(&home);
    let state = home.join("state").join("backup");
    let line = execute(
        &home,
        &Verb::Migrate {
            backup: state.clone(),
            settings: settings(),
        },
        NOW,
    )
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!((v["from"].as_i64(), v["to"].as_i64()), (Some(3), Some(3)));
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), 3);
    assert_eq!(
        one::<String>(&home, "SELECT value FROM meta WHERE key = 'schema'"),
        "3"
    );
    // The copy the answer names is the whole of what the verb wrote beside the
    // store: no second copy under an older version, because there was no older
    // version, and no step that could have changed a row.
    let named = PathBuf::from(v["backup"].as_str().unwrap());
    assert_eq!(named, db::backup_name(&state, 3, NOW));
    assert!(named.exists());
    let copies: Vec<String> = fs::read_dir(&state)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(copies, vec![format!("tehas-3-{NOW}.db")], "{copies:?}");
    let copy = Connection::open(&named).unwrap();
    let held: i64 = copy
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(held, 3);
    drop(copy);
    // The store is the file it was: a verb with nothing to run wrote nothing.
    let bytes = fs::read(&file).unwrap();
    execute(
        &home,
        &Verb::Migrate {
            backup: state.clone(),
            settings: settings(),
        },
        NOW + 60,
    )
    .unwrap();
    assert_eq!(fs::read(&file).unwrap(), bytes);
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
