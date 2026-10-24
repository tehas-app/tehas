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

/// A row in every table of schema 1, so the step is seen to keep each. The
/// issue, its cycle and the rows that hang off them are the shared ones, so a
/// fixture that needs the cycle of issue 7 asks for one list and not a copy.
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

/// One row of a table read through `keyed_rows`: its primary key and the values
/// that go with it, named so that a caller cannot read a key as a value or the
/// other way about.
type Row = (Vec<Sql>, Vec<Sql>);

/// The two tables the step from 2 to 3 is about, each read at once with its
/// rows keyed by their own primary key, so a comparison cannot line the rows of
/// one up with the columns of the other or with another row's values: the step
/// rebuilds `approvals` and renames a column of `planning_answers`, and neither
/// is allowed to move a value the store already held. `pk` is the table's own
/// key, which both dumps are sorted by, and `value_cols` names the values that
/// follow it.
struct Held {
    cols: Vec<String>,
    rows: Vec<Row>,
    pk: Vec<String>,
    value_cols: Vec<String>,
}

fn dump_two(c: &Connection) -> (Held, Held) {
    let ac = columns(c, "approvals");
    let (a, pk_a) = keyed_rows(c, "approvals", &ac);
    let pc = columns(c, "planning_answers");
    let (p, pk_p) = keyed_rows(c, "planning_answers", &pc);
    (
        Held {
            cols: ac.clone(),
            rows: a,
            value_cols: value_cols(&pk_a, &ac),
            pk: pk_a,
        },
        Held {
            cols: pc.clone(),
            rows: p,
            value_cols: value_cols(&pk_p, &pc),
            pk: pk_p,
        },
    )
}

/// The columns of the values part of a keyed row: the key's own columns lead the
/// row and are not values, and the values that follow them are named by `cols`,
/// the columns of the table read in order.
fn value_cols(pk: &[String], cols: &[String]) -> Vec<String> {
    cols.iter().filter(|c| !pk.contains(c)).cloned().collect()
}

/// The columns of a keyed row that are not the named one, each with the name it
/// is under: the rest of a row is compared by name rather than by position,
/// because the position a dump gives a column says nothing about which name it
/// was read under.
fn held_by_name<'a>(
    vcols: &'a [String],
    row: &'a [Sql],
    skip: &str,
) -> Vec<(&'a str, &'a Sql)> {
    vcols
        .iter()
        .zip(row)
        .filter(|(c, _)| c.as_str() != skip)
        .map(|(c, v)| (c.as_str(), v))
        .collect()
}

/// The value of one column of a keyed row, asked by name. Asking for the column
/// by name rather than at an offset is what keeps the answer's own column, whose
/// name the step changes, from being read out of the middle of the values by
/// position.
fn value_of<'a>(vcols: &'a [String], row: &'a [Sql], name: &str) -> &'a Sql {
    vcols
        .iter()
        .zip(row)
        .find(|(c, _)| c.as_str() == name)
        .map(|(_, v)| v)
        .unwrap_or_else(|| panic!("the row has no column {name}: {vcols:?}"))
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

/// How the two old fixtures reach the version they are the fixture of: the DDL
/// that makes the store, the number `user_version` carries, and the word of the
/// `schema` row of `meta` that says the same thing to a reader of the table.
enum At {
    /// The DDL of schema 1, as released: the store main's install.sh made.
    One,
    /// The same DDL with db::STEP_2 over it, which is a store at 2.
    Two,
}

impl At {
    /// The two spellings of this version, which the step moves together.
    fn held(&self) -> (i64, &'static str) {
        match self {
            At::One => (1, "1"),
            At::Two => (2, "2"),
        }
    }
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
    let (version, schema_row) = at.held();
    if let At::Two = at {
        // The step's own words, laid over the schema 1 fixture: a fixture of 2
        // and the step that starts at 2 cannot drift apart.
        c.execute_batch(db::step_sql(1).unwrap()).unwrap();
    }
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
        c.execute(
            "UPDATE meta SET value = ?1 WHERE key = 'schema'",
            [schema_row],
        )
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
/// schema 2 has it. The issues and the planning session of PLAN_ROWS, the cycle
/// of CYCLE_ROW and the three records of V2_ROWS -- whose two notes are of the
/// two kinds a store of schema 2 could already say -- are in it, which is what
/// the step from 2 is asked of and what the verbs read back afterwards.
fn v2_home(name: &str, rows: &str) -> PathBuf {
    old_home(
        name,
        At::Two,
        &[PLAN_ROWS, CYCLE_ROW, V2_ROWS, rows].join("\n"),
    )
}

/// Rows enough to hold the old words, not the new ones: no record made before
/// schema 3 can carry either, and a store whose note is of the kind of 3 before
/// the step is a store that never was. The kind of a note is the one of the two
/// the store itself refuses: `approvals.how` carries no CHECK at all, so the
/// words of a start are the verb's list rather than a constraint's, and no
/// constraint could keep either word out of a v2 store.
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

/// The three places the name of the answer's column is written: schema.sql, the
/// rebuild of the step from 2, and what import writes. The step renames the
/// column and the import writes it, so a rename that reached two of the three
/// would fail here rather than as an SQL error at import time.
#[test]
fn the_name_of_the_answer_s_column_is_the_one_the_schema_holds() {
    assert_eq!(crate::import::ANSWER_COLUMN, "millwright");
    assert!(
        db::SCHEMA.contains("millwright   INTEGER NOT NULL DEFAULT 0"),
        "schema.sql does not write the column the store names"
    );
    assert!(
        db::step_sql(2).is_some_and(|sql| sql.contains("RENAME COLUMN fixer TO millwright")),
        "the step from 2 does not rename the column to the name the store names"
    );
    // The JSON key of an answer stays `fixer` until the verbs speak the new word
    // (#686), and two readers outside the store depend on it: `tehas_fixer_answer`
    // in lib/fixer.sh reads `.fixer` of each answer and refuses the grant when the
    // key is not a boolean, and console/analytics/src/plan.rs counts the answers
    // whose `fixer` is true. Neither can be renamed here; the key moves with them.
    let h = v2_home("answer-key", "");
    migrate(&h).unwrap();
    let p = ok(&h, &["plan-show", "7"])["planning"].clone();
    assert_eq!(
        p["answers"],
        serde_json::json!([{"turn": 2, "by": "bob", "fixer": true}]),
        "{p}"
    );
}

/// The word list of the CHECK of a table's column, as the store's own text/// holds it. A CHECK cannot be changed in place, so the two of this step are
/// written twice: in schema.sql, which `init` runs, and in the rebuild of
/// `db::STEP_3`, which the step from 2 runs. The same list is written by hand
/// there, so both ways to schema 3 are asked the same words here: a word in one
/// and not the other is the drift the shape test cannot see, since it compares a
/// migrated store with an `init`-made one and both would be wrong together.
/// The list is taken from between the parentheses of the CHECK by finding the
/// opening list and its closing parenthesis, so no offset has to be counted.
fn check_words(c: &Connection, table: &str, column: &str) -> Vec<String> {
    let sql: String = c
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |r| r.get(0),
        )
        .unwrap();
    // The CHECK of the column, found by the words that open its list of them and
    // read to the parenthesis that closes the list -- not to the last opening
    // parenthesis of the CREATE, which belongs to the table's PRIMARY KEY. The
    // two CHECKs of this step are written by hand in two files, so the text is
    // asked for rather than matched exactly, and the spacing of the file it came
    // from is left alone.
    let opened = sql
        .find(&format!("CHECK ({column} IN ("))
        .unwrap_or_else(|| panic!("{table}.{column} has no CHECK of its own: {sql}"));
    let list = &sql[opened + "CHECK (".len() + column.len() + " IN (".len()..];
    let closed = list
        .find(')')
        .unwrap_or_else(|| panic!("{table}.{column}: no list of words: {sql}"));
    list[..closed]
        .split(',')
        .map(|w| w.trim().trim_matches('\'').to_string())
        .collect()
}

/// The words each CHECK of the step must hold at schema 3. These two lists and
/// the two of schema.sql and of the rebuild are one contract written in four
/// places, so they are named here once and asked of every store at 3: a word in
/// one copy and not another is the drift the shape test cannot see, because it
/// compares a migrated store with an `init`-made one and both would be wrong
/// together. Note the third word: `plan-note` does not take it yet, which is the
/// next part (#686); the store's shape accepts it and the verb does not.
const NOTE_KINDS: [&str; 3] = ["fixer-refused", "millwright-refused", "held"];
const QUESTION_KINDS: [&str; 2] = ["park", "plan"];

/// The CHECKs of the two tables the step is about, and the name of the answer's
/// column, read back from a store at 3: the words the old store could not say
/// are sayable, the words it could say are still sayable, and one word more is
/// still refused.
/// Everything it writes is of issue 9, which no fixture of either step uses, so
/// the helper asks the same questions of a store `init` made, of one brought
/// from 1 and of one brought from 2. Issue 9 has no cycle, so its approval is
/// recorded as a start owed and not as one made.
fn says_the_words_of_schema_3(home: &Path) {
    // The two lists of the step are the two named above, in whichever store this
    // is: the one `init` made from schema.sql or the one the step rebuilt.
    let made = conn(home);
    assert_eq!(
        check_words(&made, "planning_notes", "kind"),
        NOTE_KINDS,
        "the CHECK of a note's kind is not the list of the step"
    );
    assert_eq!(
        check_words(&made, "questions", "kind"),
        QUESTION_KINDS,
        "the CHECK of a question's kind is not the list of the schema"
    );
    drop(made);
    let mut c = db::write_ask(&db::path(home)).unwrap();
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
    // A CHECK belongs to the table: `approvals.how` carries none, in schema.sql
    // or in the rebuild, so the words of a start are the list of
    // `approval::HOWS` and a verb of the store refuses one outside it with exit
    // 64 before a row is written. What is refused below is a kind no table takes
    // and a kind of a question outside its own list.
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
    // The two notes the helper wrote, read back under their own kinds: the words
    // the store of schema 2 could not say and the one it could, all three in the
    // table the step rebuilt.
    let kinds: Vec<String> = asker
        .prepare("SELECT kind FROM planning_notes WHERE issue = 9 ORDER BY comment_id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        kinds,
        [
            "millwright-refused".to_string(),
            "fixer-refused".to_string(),
        ],
        "{kinds:?}"
    );
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

/// The rows of a pragma that describes one table, as the values they are rather
/// than as formatted text: a NULL and the word NULL are two things, and a shape
/// comparison between a migrated store and an `init`-made one should not rest on
/// how Debug happens to spell either.
///
/// `table_info` is the one pragma whose rows have an order that means something,
/// `cid`, the column's place in the table -- and it is the thing the two ways to
/// a schema are required to agree on, so it is what the rows are sorted by.
/// SQLite answers the pragma by way of an index of its own otherwise. The other
/// pragmas here answer in the order of their own rows, which is stable, and are
/// left as they came.
fn described(c: &Connection, pragma: &str, table: &str) -> Vec<Vec<Sql>> {
    let mut q = c
        .prepare(&format!("SELECT * FROM pragma_{pragma}('{table}')"))
        .unwrap();
    let n = q.column_count();
    let mut rows = q
        .query_map([], |r| {
            (0..n)
                .map(|i| r.get::<_, Sql>(i))
                .collect::<Result<Vec<_>, _>>()
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    drop(q);
    if pragma == "table_info" {
        // `cid` is the first column of the pragma's own row.
        rows.sort_by_key(|r| match r.first() {
            Some(Sql::Integer(cid)) => *cid,
            _ => i64::MIN,
        });
    }
    rows
}

/// Every row of a table keyed by its own primary key, which is what a row of
/// these tables is known by. The step's tests compare a table across a rebuild
/// and a rename: a row of the rebuilt `approvals` and of the renamed
/// `planning_answers` is the row whose key it was, so the key is read with the
/// values and carried through the comparison instead of a row being matched to
/// its counterpart by the position a dump happened to give it. The key's own
/// names come back with the rows, so the caller can name the values that follow
/// them, and the rows come back in one order the two dumps share.
fn keyed_rows(c: &Connection, table: &str, cols: &[String]) -> (Vec<Row>, Vec<String>) {
    // A table whose key is a column of its own -- `actions` has an INTEGER
    // PRIMARY KEY, which is the rowid and carries no index row of origin 'pk' --
    // is keyed by that column, named in the table's own pragma.
    let pk: Vec<String> = {
        let mut q = c
            .prepare(&format!(
                "SELECT name FROM pragma_index_list('{table}') WHERE origin = 'pk'"
            ))
            .unwrap();
        let mut names = q
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<String>, _>>()
            .unwrap();
        drop(q);
        match names.pop() {
            Some(name) => {
                let mut q = c
                    .prepare(&format!(
                        "SELECT name FROM pragma_index_info('{name}') ORDER BY seqno"
                    ))
                    .unwrap();
                q.query_map([], |r| r.get::<_, String>(0))
                    .unwrap()
                    .collect::<Result<Vec<String>, _>>()
                    .unwrap()
            }
            // No index row of origin 'pk': the key is one column of its own,
            // which is the rowid -- `actions` and the rest of the INTEGER PRIMARY
            // KEY tables. The column is named in the table's own pragma.
            None => {
                let mut q = c
                    .prepare(&format!(
                        "SELECT name FROM pragma_table_info('{table}') WHERE pk = 1 ORDER BY cid"
                    ))
                    .unwrap();
                let single = q
                    .query_map([], |r| r.get::<_, String>(0))
                    .unwrap()
                    .collect::<Result<Vec<String>, _>>()
                    .unwrap();
                assert_eq!(single.len(), 1, "{table} has no key to key a row by");
                single
            }
        }
    };
    key_by_columns(c, table, cols, pk)
}

/// Every row of `table` as its key and its values, the two read in one statement
/// so that a row's key and its values are the same row's on both sides of a
/// comparison. The key's own names come back with the rows, so the caller can
/// name the values that follow them.
fn key_by_columns(
    c: &Connection,
    table: &str,
    cols: &[String],
    pk: Vec<String>,
) -> (Vec<Row>, Vec<String>) {
    // The key columns lead the row, then the columns that are not part of it:
    // a primary key of these tables is made of ordinary columns, so naming the
    // key twice in one SELECT would ask for the same column two positions over
    // and read a row's values against the wrong names.
    let mut all: Vec<String> = pk.clone();
    all.extend(cols.iter().filter(|c| !pk.contains(c)).cloned());
    let quoted: Vec<String> = all.iter().map(|c| format!("\"{c}\"")).collect();
    let mut q = c
        .prepare(&format!("SELECT {} FROM {table}", quoted.join(", ")))
        .unwrap();
    let mut rows = q
        .query_map([], |r| {
            (0..all.len())
                .map(|i| r.get::<_, Sql>(i))
                .collect::<Result<Vec<Sql>, _>>()
        })
        .unwrap()
        .collect::<Result<Vec<Vec<Sql>>, _>>()
        .unwrap();
    drop(q);
    let pk_len = pk.len();
    let mut out: Vec<Row> = rows
        .drain(..)
        .map(|r| {
            let k = r[..pk_len].to_vec();
            let v = r[pk_len..].to_vec();
            (k, v)
        })
        .collect();
    // The order of the rows is SQLite's to choose and is not what is being
    // asserted -- only something both dumps must agree on for the two to be
    // compared row against row. `Sql` has no order of its own, so each key is
    // made into one text of its values, which is stable for one store and so the
    // same on both sides of a comparison.
    out.sort_by_key(|(k, _)| key_text(k));
    (out, pk)
}

/// One key as a single text, for ordering two dumps of a table by their keys.
fn key_text(k: &[Sql]) -> String {
    k.iter()
        .map(|v| match v {
            Sql::Null => "~".to_string(),
            Sql::Integer(n) => format!("I{n:020}"),
            Sql::Real(r) => format!("R{r:020}"),
            Sql::Text(s) => format!("T{s}"),
            Sql::Blob(b) => format!("B{}", b.len()),
        })
        .collect::<Vec<_>>()
        .join("|")
}

/// The columns of a table in the order the table holds them, `cid` by `cid`.
/// Every dump below lines a row's values up with this list, so it is the order a
/// reader of the store sees and the one thing this helper must not get wrong:
/// SQLite answers `pragma_table_info` by way of an index of its own unless the
/// rows are ordered, which left a migrated `planning_answers` answering with its
/// renamed column before the old ones.
fn columns(c: &Connection, table: &str) -> Vec<String> {
    let mut q = c
        .prepare(&format!(
            "SELECT name FROM pragma_table_info('{table}') ORDER BY cid"
        ))
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
    // The rows of `meta` are keyed by `key`: a name is never read against
    // another row's value.
    let meta = keyed_rows(&before, "meta", &columns(&before, "meta")).0;
    let mut held = Vec::new();
    for t in &names {
        if t == "meta" {
            continue;
        }
        let cols = columns(&before, t);
        let (rows, _) = keyed_rows(&before, t, &cols);
        assert!(!rows.is_empty(), "the fixture has no row in {t}");
        held.push((t.clone(), cols, rows));
    }
    drop(before);
    // The two tables the step is about, read out of the store as it was before it
    // ran and not after: the columns of `planning_answers` are asked here, while
    // the answer's column still answers to `fixer`, because the rebuild and the
    // rename are compared with these below.
    let was = conn(&home);
    let rebuilt = columns(&was, "approvals");
    let (was_approvals, _) = keyed_rows(&was, "approvals", &rebuilt);
    let renamed = columns(&was, "planning_answers");
    let (was_answers, answers_pk) = keyed_rows(&was, "planning_answers", &renamed);
    assert!(
        renamed.contains(&"fixer".to_string()),
        "the store was not at 2 before the step: {renamed:?}"
    );
    drop(was);
    let line = migrate(&home).unwrap();
    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!((v["from"].as_i64(), v["to"].as_i64()), (Some(1), Some(3)));
    let after = conn(&home);
    for (t, cols, rows) in &held {
        // The two tables the step is about are asked of by name below, where
        // the one thing it did to each of them can be named and everything else
        // about them is compared with the store as it was.
        if t == "approvals" || t == "planning_answers" {
            continue;
        }
        assert_eq!(&keyed_rows(&after, t, cols).0, rows, "{t}");
    }
    // `approvals` is the rebuild: keyed by its own (issue, at), its one row is
    // the row of schema 1 -- `how` of 'label', the approval the fixture gave, and
    // the column of input_json the first step added.
    assert_eq!(
        keyed_rows(&after, "approvals", &rebuilt).0,
        was_approvals
    );
    assert_eq!(count(&home, "approvals WHERE how = 'label'"), 1);
    assert_eq!(count(&home, "approvals WHERE input_json IS NULL"), 1);
    // `planning_answers` is the rename: the row of schema 1 never gave a value,
    // and at 3 it is the 0 of a column nobody filled, under the new name. The
    // columns the step is not allowed to touch are the ones both the store it
    // starts from and the store it leaves hold -- `text` is the first step's, so
    // the fixture of schema 1 has none of it -- and naming them keeps "no value
    // of an existing column moved" from becoming an assertion that schema 1 was
    // missing a column of schema 3's.
    let now = columns(&after, "planning_answers");
    assert!(now.contains(&"millwright".to_string()), "{now:?}");
    assert!(!now.contains(&"fixer".to_string()), "{now:?}");
    let is = keyed_rows(&after, "planning_answers", &now);
    assert_eq!(is.1, answers_pk, "the step changed the key of a row");
    let was_cols = value_cols(&answers_pk, &renamed);
    let now_cols = value_cols(&is.1, &now);
    assert_eq!(is.0.len(), was_answers.len());
    for ((k_was, was), (k_is, is)) in was_answers.iter().zip(&is.0) {
        assert_eq!(k_was, k_is, "the step moved a row's key");
        // The columns of the row the step is not allowed to move, each under the
        // name it carries in the store it is read out of.
        fn kept<'x>(cols: &'x [String], row: &'x [Sql], skip: &str) -> Vec<(&'x str, &'x Sql)> {
            held_by_name(cols, row, skip)
                .into_iter()
                // `text` is the column of the step to 2, so the fixture of
                // schema 1 has none of it: the columns the step from 2 to 3 is
                // not allowed to move are the ones the two stores hold in common.
                .filter(|(c, _)| *c == "by_account" || *c == "millwright" || *c == "fixer")
                .filter(|(c, _)| *c != skip)
                .collect()
        }
        assert_eq!(
            kept(&was_cols, was, "fixer"),
            kept(&now_cols, is, "millwright")
        );
        // The answer's own value, which the line above leaves out: the row of
        // schema 1 never gave one, so at 3 it is the 0 of a column nobody filled,
        // under the name the step gave it.
        assert_eq!(*value_of(&was_cols, was, "fixer"), Sql::Integer(0));
        assert_eq!(*value_of(&now_cols, is, "millwright"), Sql::Integer(0));
    }
    // Where the two ways to schema 3 put the columns: `millwright` is the column
    // the step of 3 renames and `text` the one the step of 2 adds, and a store
    // brought forward and one `init` made hold them in the same places. The
    // rename keeps `fixer`'s own place and ADD COLUMN puts `text` last, which is
    // where schema.sql writes it: the claim the steps' docs make, and the one an
    // `assert!(contains)` could not fail.
    assert_eq!(
        now,
        [
            "issue".to_string(),
            "turn".to_string(),
            "by_account".to_string(),
            "millwright".to_string(),
            "text".to_string(),
        ],
        "{now:?}"
    );
    assert_eq!(count(&home, "planning_answers WHERE millwright = 0"), 1);
    assert_eq!(count(&home, "planning_answers WHERE text IS NULL"), 1);
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), 3);
    assert_eq!(
        one::<String>(&home, "SELECT value FROM meta WHERE key = 'schema'"),
        "3"
    );
    // meta keeps its rows; only `schema` moves with the version. The rows are
    // keyed by `key`, so a name is never read against another row's value.
    let meta_after = keyed_rows(&after, "meta", &columns(&after, "meta")).0;
    assert_eq!(meta_after.len(), meta.len());
    for ((k_was, was), (k_is, is)) in meta.iter().zip(&meta_after) {
        assert_eq!(k_was, k_is);
        if k_was[0] == Sql::Text("schema".to_string()) {
            assert_eq!(is[0], Sql::Text("3".to_string()));
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
    // The three records the step is about, read out of the store before anything
    // runs: the step is allowed to move a row and not to change one, and the two
    // dumps of a row are compared apart from the one column it renames -- which
    // is why they are asked of the store while it is still at 2 and its answer's
    // column answers to `fixer`.
    let home = v2_home("step-2-rows", "");
    let before = conn(&home);
    holds_no_new_word(&before);
    let (approvals, answers) = dump_two(&before);
    let note_cols = columns(&before, "planning_notes");
    let notes = keyed_rows(&before, "planning_notes", &note_cols).0;
    // The fixture is a store of schema 2: its answer's column answers to
    // `fixer`, which is the name the step takes away.
    assert!(
        answers.cols.contains(&"fixer".to_string()),
        "the fixture is not a store of schema 2: {:?}",
        answers.cols
    );
    assert_eq!(answers.rows.len(), 1, "the fixture has no answer");
    assert_eq!(approvals.rows.len(), 1, "the fixture has no approval");
    assert_eq!(
        *value_of(&approvals.value_cols, &approvals.rows[0].1, "how"),
        Sql::Text("fixer".to_string())
    );
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
    assert_eq!(
        keyed_rows(&after, "approvals", &approvals.cols).0,
        approvals.rows
    );
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
    assert_eq!(
        keyed_rows(&after, "planning_notes", &note_cols).0,
        notes,
        "a note moved or changed"
    );
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
    let held = keyed_rows(&after, "planning_answers", &later);
    let later_cols = value_cols(&held.1, &later);
    assert_eq!(held.1.len(), answers.pk.len());
    assert_eq!(held.0.len(), answers.rows.len());
    for ((k_was, was), (k_is, is)) in answers.rows.iter().zip(&held.0) {
        // The same row, under the same key.
        assert_eq!(k_was, k_is, "the step moved a row's key");
        // Every column of the row but the answer's own is the same value under
        // the same name, whichever way `pragma_table_info` orders them.
        assert_eq!(
            held_by_name(&answers.value_cols, was, "fixer"),
            held_by_name(&later_cols, is, "millwright")
        );
        // The answer's own value, read from under the name each dump gives it:
        // the step may change that name and not the yes it carries.
        assert_eq!(
            *value_of(&answers.value_cols, was, "fixer"),
            *value_of(&later_cols, is, "millwright"),
            "the answer's value moved with the name of its column"
        );
    }
    // The same claim as the step from 1 makes: the columns of the renamed table
    // are those of schema.sql, in its order, with the answer's own between the
    // actor and the text.
    assert_eq!(
        later,
        [
            "issue".to_string(),
            "turn".to_string(),
            "by_account".to_string(),
            "millwright".to_string(),
            "text".to_string(),
        ],
        "{later:?}"
    );
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


