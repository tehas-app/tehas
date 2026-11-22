//! The schema this binary writes, and the steps from the stores of the
//! releases 1.0 updates from (0.30.x, 0.31.x and 0.32.x), which must keep
//! every row. A store older than v0.30.0 wrote is refused. The stores are the
//! ones those releases made, kept as SQL in tests/fixtures/store-v<n>.sql.

use crate::db;
use crate::verbs_tests::*;
use crate::{execute, Settings, Verb};
use rusqlite::types::Value as Sql;
use rusqlite::Connection;
use std::fs::{self, OpenOptions};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::Path;
use tehas_core::home::{Home, State};

/// The store each tagged release 1.0 updates from makes, by the schema it is
/// at: v0.30.0 wrote 12, v0.30.1 13, v0.30.2 to v0.30.6 14 with one
/// schema.sql, v0.31.0 16 and v0.32.0 17.
const RELEASES: [(&str, i64, &str); 6] = [
    (
        "v0.30.0",
        12,
        include_str!("../tests/fixtures/store-v0.30.0.sql"),
    ),
    (
        "v0.30.1",
        13,
        include_str!("../tests/fixtures/store-v0.30.1.sql"),
    ),
    (
        "v0.30.2",
        14,
        include_str!("../tests/fixtures/store-v0.30.2.sql"),
    ),
    (
        "v0.31.0",
        16,
        include_str!("../tests/fixtures/store-v0.31.0.sql"),
    ),
    (
        "v0.32.0",
        17,
        include_str!("../tests/fixtures/store-v0.32.0.sql"),
    ),
    (
        "v1.4.0",
        23,
        include_str!("../tests/fixtures/store-v1.4.0.sql"),
    ),
];

fn settings() -> Settings {
    Settings {
        version: "v1.0.0".to_string(),
        commit: "cb6912a".to_string(),
        instance: "tehas-tehas".to_string(),
        repo: "example-owner/example-product".to_string(),
    }
}

/// A home whose store is the one a release made, from its SQL.
fn release_home(name: &str, sql: &str) -> TestDir {
    let home = scratch(name);
    let state = Home::new(home.path()).state_dir();
    fs::DirBuilder::new().mode(0o700).create(&state).unwrap();
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(db::path(&home))
        .unwrap();
    let c = Connection::open(db::path(&home)).unwrap();
    c.pragma_update(None, "journal_mode", "WAL").unwrap();
    c.execute_batch(sql).unwrap();
    home
}

/// The store of a release at `schema`, and that schema.
fn release(name: &str, schema: i64) -> TestDir {
    let (_, _, sql) = RELEASES.iter().find(|r| r.1 == schema).unwrap();
    release_home(name, sql)
}

/// A store at 15: the last release's, brought forward by this binary's steps
/// to 15 and no further, as a host on a pre-release of 1.0 holds it.
fn at_15(name: &str) -> TestDir {
    let home = release(name, 14);
    let mut c = conn(&home);
    for (to, run) in db::STEPS {
        if *to == 15 {
            run(&mut c).unwrap();
        }
    }
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), 15);
    home
}

/// A store at 16: the last release's, brought forward to 16 and no further,
/// as a host on a pre-release of 1.0 before schema 17 holds it.
fn at_16(name: &str) -> TestDir {
    let home = release(name, 14);
    let mut c = conn(&home);
    for (to, run) in db::STEPS {
        if *to == 15 || *to == 16 {
            run(&mut c).unwrap();
        }
    }
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), 16);
    home
}

/// A store at 17: the last release's, brought forward to 17 and no further,
/// as a host on a pre-release of 1.0 before schema 18 holds it.
fn at_17(name: &str) -> TestDir {
    let home = release(name, 14);
    let mut c = conn(&home);
    for (to, run) in db::STEPS {
        if (15..=17).contains(to) {
            run(&mut c).unwrap();
        }
    }
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), 17);
    home
}

/// A store at 22: the last release's, brought forward to 22 and no further,
/// as a host that ran plugins before a tool was called holds it.
fn at_22(name: &str) -> TestDir {
    let home = release(name, 17);
    let mut c = conn(&home);
    for (to, run) in db::STEPS {
        if (18..=22).contains(to) {
            run(&mut c).unwrap();
        }
    }
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), 22);
    home
}

/// The SQL of the step to `to`, as one transaction runs it.
fn step_sql(to: i64) -> &'static str {
    match to {
        13 => db::STEP_13,
        14 => db::STEP_14,
        15 => db::STEP_15,
        16 => db::STEP_16,
        17 => db::STEP_17,
        18 => db::STEP_18,
        19 => db::STEP_19,
        20 => db::STEP_20,
        21 => db::STEP_21,
        22 => db::STEP_22,
        23 => db::STEP_23,
        24 => db::STEP_24,
        25 => db::STEP_25,
        26 => db::STEP_26,
        27 => db::STEP_27,
        db::DROP_OUTSOURCE_STEP => db::DROP_OUTSOURCE,
        _ => unreachable!("no step to {to}"),
    }
}

/// Every table but meta, plugin_runs, plugin_records and answers, with its
/// rows, in rowid order. The three are new at 19, 20 and 24, so no store a
/// step starts from holds them.
fn rows(c: &Connection) -> Vec<(String, usize)> {
    tables(c)
        .into_iter()
        .filter(|t| !["meta", "plugin_runs", "plugin_records", "answers"].contains(&t.as_str()))
        .map(|t| {
            let n = dump(c, &t, &columns(c, &t)).len();
            (t, n)
        })
        .collect()
}

/// The rows of every table but the outsource ones, which a store of 15 to 27
/// has and no other does.
fn host_rows(c: &Connection) -> Vec<(String, usize)> {
    rows(c)
        .into_iter()
        .filter(|(t, _)| !db::DROPPED_TABLES.contains(&t.as_str()))
        .collect()
}

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
    migrate_at(home, NOW)
}

/// `store migrate` at `now`: two migrations of one store in one second would
/// name the same copy.
fn migrate_at(home: &Path, now: i64) -> Result<String, (u8, String)> {
    execute(
        home,
        &Verb::Migrate {
            backup: Home::new(home).state(State::Backups),
            settings: settings(),
        },
        now,
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
fn init_writes_schema_28() {
    assert_eq!(db::SCHEMA_VERSION, 28);
    let home = home("schema-28");
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), 28);
    assert_eq!(
        one::<String>(&home, "SELECT value FROM meta WHERE key = 'schema'"),
        "28"
    );
    // Schema 27: a question asked again in 2.0's words.
    assert!(columns(&conn(&home), "questions").contains(&"reask".to_string()));
    // Schema 25: the choices a park's question offers.
    assert!(columns(&conn(&home), "questions").contains(&"offers".to_string()));
    // Outsource was dropped, and init lays none of its tables.
    for t in db::DROPPED_TABLES {
        assert!(!tables(&conn(&home)).iter().any(|n| n == t), "{t}");
    }
    assert_eq!(count(&home, "meta WHERE key = 'outsource'"), 0);
    assert_eq!(count(&home, "planning_notes"), 0);
    let c = conn(&home);
    assert!(columns(&c, "heartbeats").contains(&"text".to_string()));
    assert!(columns(&c, "planning_answers").contains(&"millwright".to_string()));
    assert!(!columns(&c, "planning_answers").contains(&"fixer".to_string()));
    for t in ["planning", "plan_history"] {
        assert!(columns(&c, t).contains(&"auto".to_string()), "{t}");
    }
    // Schema 19: a run of a plugin. Schema 23: the size of a call's answer.
    assert_eq!(
        columns(&c, "plugin_runs"),
        [
            "id",
            "plugin",
            "plugin_commit",
            "kind",
            "how",
            "started_by",
            "head",
            "started_at",
            "ended_at",
            "result",
            "tokens",
            "size"
        ]
    );
    assert_eq!(count(&home, "plugin_runs"), 0);
    // Schema 20: a plugin's records, none until a plugin keeps one.
    assert_eq!(count(&home, "plugin_records"), 0);
    // Schema 21: who filed a record's issue.
    assert!(columns(&c, "plugin_records").contains(&"filed_by".to_string()));
    assert!(columns(&c, "plugin_records").contains(&"filed_by_id".to_string()));
    // Schema 24: the answers of `tehas answer`, none until one is given.
    assert_eq!(
        columns(&c, "answers"),
        [
            "question",
            "issue",
            "at",
            "by_account",
            "choice",
            "option_key",
            "text"
        ]
    );
    assert_eq!(count(&home, "answers"), 0);
    let made = tables(&c);
    for t in DROPPED {
        assert!(!made.contains(&t.to_string()), "{t}");
    }
}

/// What a store migrated here must be: the schema init writes, table for
/// table, index for index and CHECK for CHECK.
fn same_shape_as_init(migrated: &Path, made: &Connection, from: &str) {
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
    let a = conn(migrated);
    assert_eq!(tables(&a), tables(made), "from {from}");
    for t in tables(made) {
        for pragma in ["table_info", "index_list", "foreign_key_list"] {
            assert_eq!(
                described(&a, pragma, &t),
                described(made, pragma, &t),
                "{pragma} of {t} from {from}"
            );
        }
        assert_eq!(
            checks(&a, &t),
            checks(made, &t),
            "CHECKs of {t} from {from}"
        );
    }
    assert_eq!(index_sql(&a), index_sql(made), "from {from}");
}

#[test]
fn the_fixtures_are_the_stores_of_the_releases() {
    for (version, schema, _) in RELEASES {
        let home = release(&format!("fixture-{version}"), schema);
        assert_eq!(one::<i64>(&home, "PRAGMA user_version"), schema);
        assert_eq!(
            one::<String>(&home, "SELECT value FROM meta WHERE key = 'schema'"),
            schema.to_string()
        );
        assert_eq!(
            one::<String>(&home, "SELECT value FROM meta WHERE key = 'tehas_version'"),
            version
        );
        // A row in every table a host writes, so a step is seen to keep each.
        for (t, n) in rows(&conn(&home)) {
            if !db::DROPPED_TABLES.contains(&t.as_str()) {
                assert!(n > 0, "{version} has no row in {t}");
            }
        }
    }
}

#[test]
fn a_store_of_each_release_1_0_updates_from_comes_to_18_with_every_row() {
    let made = home("init-shape");
    let b = conn(&made);
    for (version, schema, _) in RELEASES {
        let home = release(&format!("release-{version}"), schema);
        let before = host_rows(&conn(&home));
        let meta = count(&home, "meta");
        let v: serde_json::Value = serde_json::from_str(&migrate(&home).unwrap()).unwrap();
        assert_eq!(
            (v["from"].as_i64(), v["to"].as_i64()),
            (Some(schema), Some(db::SCHEMA_VERSION)),
            "{version}"
        );
        assert_eq!(one::<i64>(&home, "PRAGMA user_version"), db::SCHEMA_VERSION);
        assert_eq!(
            one::<String>(&home, "SELECT value FROM meta WHERE key = 'schema'"),
            db::SCHEMA_VERSION.to_string()
        );
        assert_eq!(count(&home, "meta"), meta, "{version}");
        // Every row kept; the outsource tables are gone.
        let after = host_rows(&conn(&home));
        assert_eq!(after, before, "{version}");
        for t in db::DROPPED_TABLES {
            assert!(
                !tables(&conn(&home)).iter().any(|n| n == t),
                "{t} of {version}"
            );
        }
        assert_eq!(count(&home, "meta WHERE key = 'outsource'"), 0);
        same_shape_as_init(&home, &b, version);
        // The millwright's old word is gone from the rows (schema 16).
        assert_eq!(count(&home, "cycles WHERE how = 'fixer'"), 0);
        assert_eq!(count(&home, "cycles WHERE how = 'millwright'"), 1);
        assert_eq!(
            count(&home, "planning_notes WHERE kind = 'fixer-refused'"),
            0
        );
        // The verbs read what the release wrote.
        assert_eq!(
            ok(&home, &["approval-current", "7"])["current"]["by"],
            "alice"
        );
        let p = ok(&home, &["plan-show", "7"])["planning"].clone();
        assert_eq!(p["answers"][0]["by"], "bob", "{version}");
        assert_eq!(p["notes"]["millwright_refused"], serde_json::json!([55]));
        let h = ok(&home, &["history-show", "8", "--kind", "cycle"]);
        assert_eq!(h["rows"][0]["how"], "millwright", "{version}");
        let open = ok(&home, &["journal-open"]);
        assert_eq!(open["actions"][0]["state"], "attempted", "{version}");
        // The copy the migrate took is the store as the release left it.
        let copy = Home::new(home.path())
            .state(State::Backups)
            .join(format!("tehas-{schema}-{NOW}.db"));
        let c = Connection::open(&copy).unwrap();
        let at: i64 = c
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(at, schema, "{}", copy.display());
        let report = execute(&home, &Verb::Check, NOW).unwrap();
        assert!(
            report.contains(&format!(
                "\"schema\":{},\"integrity\":\"ok\"",
                db::SCHEMA_VERSION
            )),
            "{report}"
        );
    }
    let home = at_15("release-15");
    migrate(&home).unwrap();
    same_shape_as_init(&home, &b, "15");
    let home = at_16("release-16");
    migrate(&home).unwrap();
    same_shape_as_init(&home, &b, "16");
    let home = at_17("release-17");
    migrate(&home).unwrap();
    same_shape_as_init(&home, &b, "17");
}

/// Plan 6.7: an update stopped at any step, then run again. A step is one
/// transaction, so a stop between two steps leaves the store at the last one,
/// and a stop inside a step leaves it where the step began. Either way the
/// next migrate goes on from there to the schema init writes, with every row.
#[test]
fn a_migrate_stopped_at_any_step_comes_to_18_when_it_runs_again() {
    let made = home("stopped-shape");
    let b = conn(&made);
    for (version, schema, _) in RELEASES {
        for stop in schema + 1..=db::SCHEMA_VERSION {
            let name = format!("stopped-{version}-at-{stop}");
            let home = release(&name, schema);
            let before = host_rows(&conn(&home));
            let mut c = conn(&home);
            for (to, run) in db::STEPS {
                if *to > schema && *to < stop {
                    run(&mut c).unwrap();
                }
            }
            // The step to `stop` begun and its work done, then the process
            // gone before the commit.
            {
                c.pragma_update(None, "foreign_keys", "OFF").unwrap();
                let tx = c.transaction().unwrap();
                tx.execute_batch(step_sql(stop)).unwrap();
                drop(tx);
                c.pragma_update(None, "foreign_keys", "ON").unwrap();
            }
            drop(c);
            assert_eq!(one::<i64>(&home, "PRAGMA user_version"), stop - 1, "{name}");
            assert_eq!(
                one::<String>(&home, "SELECT value FROM meta WHERE key = 'schema'"),
                (stop - 1).to_string(),
                "{name}"
            );
            let v: serde_json::Value = serde_json::from_str(&migrate(&home).unwrap()).unwrap();
            assert_eq!(
                (v["from"].as_i64(), v["to"].as_i64()),
                (Some(stop - 1), Some(db::SCHEMA_VERSION)),
                "{name}"
            );
            let after = host_rows(&conn(&home));
            assert_eq!(after, before, "{name}");
            same_shape_as_init(&home, &b, &name);
        }
    }
}

/// The step that drops outsource, run over a store at the schema before it:
/// the five tables go with whatever rows a development store held, and the
/// feature's row of meta, and every other row stays.
#[test]
fn the_step_that_drops_outsource_takes_its_tables_and_keeps_every_other_row() {
    let made = home("drop-outsource-init");
    let home = release("drop-outsource", 23);
    let mut c = conn(&home);
    for (to, run) in db::STEPS {
        if (24..db::DROP_OUTSOURCE_STEP).contains(to) {
            run(&mut c).unwrap();
        }
    }
    drop(c);
    assert_eq!(
        one::<i64>(&home, "PRAGMA user_version"),
        db::DROP_OUTSOURCE_STEP - 1
    );
    let key = "0".repeat(64);
    conn(&home)
        .execute_batch(&format!(
            "INSERT OR IGNORE INTO issues (issue, kind, kind_at, checked_at) VALUES (7, 'issue', 1, 1);
             INSERT INTO contractors (name, public_key, added_at) VALUES ('box1', '{key}', 100);
             INSERT INTO jobs (issue, contractor, attempt, state, offered_at, expires_at, updated_at)
               VALUES (7, 'box1', 1, 'offered', 100, 200, 100);
             INSERT INTO job_history (job, at, to_state, said_by) VALUES (1, 100, 'offered', 'factory');
             INSERT INTO issue_attempts (issue, given) VALUES (7, 1);
             INSERT INTO meta (key, value) VALUES ('outsource', '1');"
        ))
        .unwrap();
    let before = host_rows(&conn(&home));
    let meta = count(&home, "meta");
    migrate(&home).unwrap();
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), db::SCHEMA_VERSION);
    for t in db::DROPPED_TABLES {
        assert!(!tables(&conn(&home)).iter().any(|n| n == t), "{t}");
    }
    assert_eq!(count(&home, "meta WHERE key = 'outsource'"), 0);
    assert_eq!(count(&home, "meta"), meta - 1);
    assert_eq!(host_rows(&conn(&home)), before);
    same_shape_as_init(&home, &conn(&made), "the schema before the drop");
}

#[test]
fn a_step_that_fails_leaves_the_store_where_it_was_with_its_rows() {
    // A table of the step's own name already there: the step to 15 fails, and
    // the store is at 14 with every row, ready for a migrate once it is gone.
    let home = release("step-fails", 14);
    conn(&home)
        .execute_batch("CREATE TABLE contractors (a);")
        .unwrap();
    let before = rows(&conn(&home));
    let (code, line) = migrate(&home).unwrap_err();
    assert_eq!(code, 78, "{line}");
    assert!(
        line.contains("the step from schema 14 to 15 failed"),
        "{line}"
    );
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), 14);
    assert_eq!(rows(&conn(&home)), before);
    conn(&home)
        .execute_batch("DROP TABLE contractors;")
        .unwrap();
    migrate_at(&home, NOW + 1).unwrap();
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), db::SCHEMA_VERSION);
}

/// A store older than v0.30.0 wrote: an earlier release's. Its rows are those
/// of the v0.30.0 store, and its version an older one.
fn pre_0_30(name: &str, schema: i64) -> TestDir {
    let home = release(name, 12);
    let c = conn(&home);
    c.execute(
        "UPDATE meta SET value = ?1 WHERE key = 'schema'",
        [schema.to_string()],
    )
    .unwrap();
    c.pragma_update(None, "user_version", schema).unwrap();
    home
}

#[test]
fn a_store_older_than_v0_30_is_refused_and_left_as_it_was() {
    for schema in [1, 6, 10, 11] {
        let home = pre_0_30(&format!("pre-0.30-{schema}"), schema);
        let before = rows(&conn(&home));
        let (code, line) = migrate(&home).unwrap_err();
        assert_eq!(code, 78, "{line}");
        assert!(line.starts_with("unreadable "), "{line}");
        assert!(
            line.contains(&format!(
                "is at schema {schema}, older than v0.30.0 wrote: update to a v0.30 release first, then to this one"
            )),
            "{line}"
        );
        assert_eq!(one::<i64>(&home, "PRAGMA user_version"), schema);
        assert_eq!(rows(&conn(&home)), before);
        // Refused before the copy: no backup of it is written.
        let backups = Home::new(home.path()).state(State::Backups);
        let kept = fs::read_dir(&backups).map_or(0, |d| d.count());
        assert_eq!(kept, 0, "{}", backups.display());
        // The dry run, which install.sh asks before it copies anything,
        // refuses it the same way.
        let (code, line) = execute(&home, &Verb::MigrateDry { backup: None }, NOW).unwrap_err();
        assert_eq!(code, 78, "{line}");
        assert!(line.contains("update to a v0.30 release first"), "{line}");
        // The verbs say the same.
        let (code, line) = call(&home, &["plan-show", "7"]).unwrap_err();
        assert_eq!(code, 78);
        assert!(line.contains("update to a v0.30 release first"), "{line}");
        // A copy of it does not load either.
        let into = crate::verbs_tests::home(&format!("pre-0.30-into-{schema}"));
        let copy = db::path(&home);
        let (code, line) = call(&into, &["load", "--from", copy.to_str().unwrap()]).unwrap_err();
        assert_eq!(code, 1, "{line}");
        assert!(line.contains("older than v0.30.0 wrote"), "{line}");
    }
}

#[test]
fn the_dry_run_of_migrate_judges_a_release_store_and_writes_nothing() {
    for schema in [12, 13, 14, 16, 17, db::SCHEMA_VERSION] {
        let home = release(&format!("dry-{schema}"), schema.min(17));
        if schema == db::SCHEMA_VERSION {
            migrate(&home).unwrap();
        }
        let file = db::path(&home);
        let before = fs::read(&file).unwrap();
        let said = execute(&home, &Verb::MigrateDry { backup: None }, NOW).unwrap();
        let v: serde_json::Value = serde_json::from_str(&said).unwrap();
        assert_eq!(v["ok"], true, "{said}");
        assert_eq!(v["dry_run"], true, "{said}");
        assert_eq!(v["from"], schema, "{said}");
        assert_eq!(v["to"], db::SCHEMA_VERSION, "{said}");
        assert_eq!(fs::read(&file).unwrap(), before);
        assert_eq!(one::<i64>(&home, "PRAGMA user_version"), schema);
    }
}

#[test]
fn the_dry_run_of_migrate_takes_a_backup_directory_and_no_other_word() {
    let p = |a: &[&str]| crate::parse(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    let dry = |backup: Option<&str>| {
        Some(Verb::MigrateDry {
            backup: backup.map(std::path::PathBuf::from),
        })
    };
    assert_eq!(p(&["migrate", "--dry-run"]), dry(None));
    assert_eq!(p(&["migrate", "--dry-run", "--json"]), dry(None));
    assert_eq!(
        p(&["migrate", "--dry-run", "--backup", "/x"]),
        dry(Some("/x"))
    );
    assert_eq!(
        p(&["migrate", "--backup", "/x", "--dry-run"]),
        dry(Some("/x"))
    );
    assert_eq!(p(&["migrate", "--dry-run", "--backup", "x"]), None);
    assert_eq!(p(&["migrate", "--dry-run", "--backup"]), None);
    assert_eq!(
        p(&["migrate", "--dry-run", "--backup", "/x", "--backup", "/y"]),
        None
    );
    assert_eq!(p(&["migrate", "--dry-run", "--dry-run"]), None);
}

fn rehearse(home: &Path, backup: &Path) -> Result<String, (u8, String)> {
    execute(
        home,
        &Verb::MigrateDry {
            backup: Some(backup.to_path_buf()),
        },
        NOW,
    )
}

// install.sh asks it before its first copy, so what stops the real migration
// before its first step stops the install while nothing is installed.
#[test]
fn the_dry_run_with_a_backup_writes_the_copies_and_takes_them_away() {
    for schema in [17, db::SCHEMA_VERSION] {
        let home = release(&format!("rehearse-{schema}"), 17);
        if schema == db::SCHEMA_VERSION {
            migrate(&home).unwrap();
            for copy in fs::read_dir(Home::new(home.path()).state(State::Backups)).unwrap() {
                fs::remove_file(copy.unwrap().path()).unwrap();
            }
        }
        let file = db::path(&home);
        let before = fs::read(&file).unwrap();
        let backup = Home::new(home.path()).state(State::Backups);
        let said = rehearse(&home, &backup).unwrap();
        let v: serde_json::Value = serde_json::from_str(&said).unwrap();
        assert_eq!(v["dry_run"], true, "{said}");
        assert_eq!(v["from"], schema, "{said}");
        assert_eq!(v["to"], db::SCHEMA_VERSION, "{said}");
        let made = backup.canonicalize().unwrap();
        assert_eq!(v["backup"], made.to_str().unwrap(), "{said}");
        assert_eq!(fs::read_dir(&backup).unwrap().count(), 0);
        assert_eq!(fs::read(&file).unwrap(), before);
        assert_eq!(one::<i64>(&home, "PRAGMA user_version"), schema);
    }
}

#[test]
fn the_dry_run_with_a_backup_refuses_a_directory_out_of_the_home() {
    let home = release("rehearse-out", 17);
    let elsewhere = home.join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, Home::new(home.path()).state(State::Backups)).unwrap();
    let file = db::path(&home);
    let before = fs::read(&file).unwrap();
    // The plain dry run cannot see it, which is why install.sh asks with it.
    execute(&home, &Verb::MigrateDry { backup: None }, NOW).unwrap();
    let (code, line) = rehearse(&home, &Home::new(home.path()).state(State::Backups)).unwrap_err();
    assert_eq!(code, 78, "{line}");
    assert!(
        line.contains("the backup goes in the state/ or state/backup/ of the store's home"),
        "{line}"
    );
    assert_eq!(fs::read_dir(&elsewhere).unwrap().count(), 0);
    assert_eq!(fs::read(&file).unwrap(), before);
    // The real migration is refused the same way.
    let (code, _) = migrate(&home).unwrap_err();
    assert_eq!(code, 78);
}

#[test]
fn the_verbs_refuse_a_store_of_a_0_30_release_until_it_is_migrated() {
    let home = release("verbs-at-12", 12);
    let (code, line) = call(&home, &["plan-show", "7"]).unwrap_err();
    assert_eq!(code, 78);
    assert!(
        line.contains(&format!(
            "the store is at schema 12, this store writes {}; run store migrate",
            db::SCHEMA_VERSION
        )),
        "{line}"
    );
    migrate(&home).unwrap();
    assert_eq!(
        ok(&home, &["approval-current", "7"])["current"]["used_by"],
        2
    );
}

/// #1967: the step from 13 keeps every cycle and every row that references one,
/// and a cycle may then say it was started from the host.
#[test]
fn the_step_from_13_keeps_the_cycles_and_takes_host() {
    let home = release("step-from-13", 13);
    let children = ["heartbeats", "cycle_settings", "cycle_notes"];
    let before: Vec<i64> = children.iter().map(|t| count(&home, t)).collect();
    assert!(before.iter().all(|&n| n > 0), "{before:?}");
    let cycles = count(&home, "cycles");
    let insert = "INSERT OR IGNORE INTO issues (issue, kind, kind_at, checked_at) VALUES (90, 'issue', 1, 1);
         INSERT INTO cycles (issue, started_at, started_by, how) VALUES (90, 400, 'host', 'host');";
    assert!(conn(&home).execute_batch(insert).is_err());
    migrate(&home).unwrap();
    let after: Vec<i64> = children.iter().map(|t| count(&home, t)).collect();
    assert_eq!(after, before);
    assert_eq!(count(&home, "cycles"), cycles);
    conn(&home).execute_batch(insert).unwrap();
    assert_eq!(count(&home, "cycles WHERE how = 'host'"), 1);
}

/// #1754: the step from 15 keeps every action and its journal, and an action
/// may then be `unknown`.
#[test]
fn the_step_from_15_keeps_the_journal_and_takes_unknown() {
    let home = at_15("step-from-15");
    let unknown =
        "INSERT INTO actions (poll_at, kind, issue, args_json, before_snapshot, state, intended_at)
         VALUES (1790000300, 'comment-post', 7, '{}', 0, 'unknown', 200);";
    assert!(conn(&home).execute_batch(unknown).is_err());
    let (actions, journal) = (count(&home, "actions"), count(&home, "journal"));
    assert!(actions > 0 && journal > 0);
    migrate(&home).unwrap();
    assert_eq!(count(&home, "actions"), actions);
    assert_eq!(count(&home, "journal"), journal);
    conn(&home).execute_batch(unknown).unwrap();
    assert_eq!(count(&home, "actions WHERE state = 'unknown'"), 1);
}

/// The step to 16 writes the millwright's old word as its name in every row
/// that kept it, so no answer of the store says `fixer` again.
#[test]
fn the_step_from_15_names_the_millwright_in_every_old_row() {
    let home = at_15("step-16-words");
    conn(&home)
        .execute_batch(
            "INSERT INTO approvals (issue, at, by_account, how, label, body_sha256, started_how)
               VALUES (8, 1790000600, 'tehas-fixer', 'fixer', 'tehas:ready',
                       'cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc', 'fixer');
             INSERT INTO planning_notes (issue, kind, comment_id, at) VALUES (7, 'fixer-refused', 57, 1789999700);
             INSERT INTO planning_notes (issue, kind, comment_id, at) VALUES (7, 'millwright-refused', 57, 1789999800);",
        )
        .unwrap();
    let kept: Vec<i64> = ["cycles", "cycle_history", "approvals"]
        .iter()
        .map(|t| count(&home, t))
        .collect();
    migrate(&home).unwrap();
    for (t, n) in ["cycles", "cycle_history", "approvals"].iter().zip(kept) {
        assert_eq!(count(&home, t), n, "{t}");
        assert_eq!(count(&home, &format!("{t} WHERE how = 'fixer'")), 0, "{t}");
    }
    assert_eq!(count(&home, "cycle_history WHERE how = 'millwright'"), 1);
    assert_eq!(
        count(
            &home,
            "approvals WHERE how = 'millwright' AND started_how = 'millwright'"
        ),
        1
    );
    // One note a comment: a comment noted under both words is kept once.
    assert_eq!(
        count(&home, "planning_notes WHERE kind = 'fixer-refused'"),
        0
    );
    assert_eq!(
        count(&home, "planning_notes WHERE kind = 'millwright-refused'"),
        2
    );
}

/// The step to 23 keeps every run of a plugin, with no size, and a row may
/// then be a call.
#[test]
fn the_step_from_22_keeps_the_plugin_runs_and_takes_a_call() {
    let home = at_22("step-23-runs");
    let (c40, h40) = ("0".repeat(40), "1".repeat(40));
    conn(&home)
        .execute_batch(&format!(
            "INSERT INTO plugin_runs (plugin, plugin_commit, kind, how, started_by, head, started_at, ended_at, result, tokens)
               VALUES ('floorplan', '{c40}', 'refresh', 'timer', 'timer', '{h40}', 100, 160, 'done', 30);
             INSERT INTO plugin_runs (plugin, plugin_commit, kind, how, started_by, head, started_at)
               VALUES ('floorplan', '{c40}', 'hook', 'cycle', 'host', '{h40}', 200);"
        ))
        .unwrap();
    let call = format!(
        "INSERT INTO plugin_runs (plugin, plugin_commit, kind, how, started_by, head, started_at)
           VALUES ('floorplan', '{c40}', 'call', 'chat', 'chat', '{c40}', 300);"
    );
    assert!(conn(&home).execute_batch(&call).is_err());
    let kept = |home: &Path| {
        let cols = [
            "id",
            "plugin",
            "plugin_commit",
            "kind",
            "how",
            "started_by",
            "head",
            "started_at",
            "ended_at",
            "result",
            "tokens",
        ]
        .map(str::to_string);
        dump(&conn(home), "plugin_runs", &cols)
    };
    let before = kept(&home);
    assert_eq!(before.len(), 2);
    migrate(&home).unwrap();
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), db::SCHEMA_VERSION);
    assert_eq!(kept(&home), before);
    assert_eq!(count(&home, "plugin_runs WHERE size IS NULL"), 2);
    conn(&home).execute_batch(&call).unwrap();
    assert_eq!(count(&home, "plugin_runs WHERE kind = 'call'"), 1);
    let made = crate::verbs_tests::home("step-23-shape");
    same_shape_as_init(&home, &conn(&made), "22");
}

/// The step to 24 keeps every row and adds the answers, empty, which go with
/// their question.
#[test]
fn the_step_from_23_adds_the_answers_and_an_answer_goes_with_its_question() {
    let home = at_22("step-24-answers");
    let mut c = conn(&home);
    for (to, run) in db::STEPS {
        if *to == 23 {
            run(&mut c).unwrap();
        }
    }
    drop(c);
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), 23);
    let before = host_rows(&conn(&home));
    migrate(&home).unwrap();
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), db::SCHEMA_VERSION);
    assert_eq!(host_rows(&conn(&home)), before);
    assert_eq!(count(&home, "answers"), 0);
    let c = conn(&home);
    c.execute_batch("PRAGMA foreign_keys = ON").unwrap();
    let id: i64 = c
        .query_row("SELECT id FROM questions LIMIT 1", [], |r| r.get(0))
        .unwrap();
    c.execute(
        "INSERT INTO answers (question, issue, at, by_account, choice) VALUES (?1, 7, 100, 'alice', 'retry')",
        [id],
    )
    .unwrap();
    assert!(c
        .execute(
            "INSERT INTO answers (question, issue, at, by_account, choice) VALUES (?1, 7, 100, 'alice', 'tick')",
            [id + 1],
        )
        .is_err());
    c.execute("DELETE FROM questions WHERE id = ?1", [id])
        .unwrap();
    assert_eq!(count(&home, "answers"), 0);
    // And a cycle's start may say `answer`, every cycle kept.
    c.execute("UPDATE cycles SET how = 'answer'", []).unwrap();
    assert_eq!(
        count(&home, "cycles WHERE how = 'answer'"),
        count(&home, "cycles")
    );
    let made = crate::verbs_tests::home("step-24-shape");
    same_shape_as_init(&home, &conn(&made), "23");
}

/// The step to 25 keeps every question and gives none the choices it
/// offered: they were not kept before.
#[test]
fn the_step_from_24_adds_the_offers_of_a_question_and_keeps_every_row() {
    let home = at_22("step-25-offers");
    let mut c = conn(&home);
    for (to, run) in db::STEPS {
        if (23..=24).contains(to) {
            run(&mut c).unwrap();
        }
    }
    drop(c);
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), 24);
    let before = host_rows(&conn(&home));
    migrate(&home).unwrap();
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), db::SCHEMA_VERSION);
    assert_eq!(host_rows(&conn(&home)), before);
    assert_eq!(count(&home, "questions WHERE offers IS NOT NULL"), 0);
    let made = crate::verbs_tests::home("step-25-shape");
    same_shape_as_init(&home, &conn(&made), "24");
}

/// The step to 27 from a store of v1.4.0 with a retry ticked and two
/// questions open: every `retry-tick` is `reply` (cycles, approvals and the
/// history), `cycles.how` takes `retry-tick` no more, and each open question
/// is marked to be asked again, which question-known says.
#[test]
fn the_step_to_27_rewrites_retry_tick_and_marks_the_open_questions() {
    let sql = format!(
        "{}\n{}",
        include_str!("../tests/fixtures/store-v1.4.0.sql"),
        include_str!("../tests/fixtures/store-v1.4.0-open.sql")
    );
    let home = release_home("step-27-reask", &sql);
    migrate(&home).unwrap();
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), db::SCHEMA_VERSION);
    for t in ["cycles", "approvals", "cycle_history"] {
        assert_eq!(
            count(&home, &format!("{t} WHERE how = 'retry-tick'")),
            0,
            "{t}"
        );
        assert_eq!(
            count(&home, &format!("{t} WHERE issue = 21 AND how = 'reply'")),
            1,
            "{t}"
        );
    }
    assert_eq!(count(&home, "approvals WHERE label = 'retry-tick'"), 0);
    assert_eq!(
        count(&home, "approvals WHERE started_how = 'retry-tick'"),
        0
    );
    assert_eq!(
        count(
            &home,
            "approvals WHERE issue = 21 AND started_how = 'reply'"
        ),
        1
    );
    assert!(conn(&home)
        .execute("UPDATE cycles SET how = 'retry-tick' WHERE issue = 21", [])
        .is_err());
    // The open questions, and only they, are marked.
    assert_eq!(
        count(&home, "questions WHERE reask = 1"),
        count(&home, "questions WHERE answered_at IS NULL")
    );
    for (issue, id) in [("21", "2101"), ("22", "2201")] {
        let v = ok(&home, &["question-known", issue, "--id", id]);
        assert_eq!(
            (v["known"].clone(), v["reask"].clone()),
            (serde_json::json!(true), serde_json::json!(true)),
            "{v}"
        );
    }
    let made = crate::verbs_tests::home("step-27-shape");
    same_shape_as_init(&home, &conn(&made), "v1.4.0");
}

/// The step to 26 keeps every cycle, and a cycle's start may then say
/// `reply`, the how of a retry a person's reply asked for.
#[test]
fn the_step_from_25_lets_a_cycle_say_reply_and_keeps_every_row() {
    let home = at_22("step-26-reply");
    let mut c = conn(&home);
    for (to, run) in db::STEPS {
        if (23..=25).contains(to) {
            run(&mut c).unwrap();
        }
    }
    assert!(c.execute("UPDATE cycles SET how = 'reply'", []).is_err());
    drop(c);
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), 25);
    let before = host_rows(&conn(&home));
    migrate(&home).unwrap();
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), db::SCHEMA_VERSION);
    assert_eq!(host_rows(&conn(&home)), before);
    let c = conn(&home);
    c.execute("UPDATE cycles SET how = 'reply'", []).unwrap();
    assert_eq!(
        count(&home, "cycles WHERE how = 'reply'"),
        count(&home, "cycles")
    );
    let made = crate::verbs_tests::home("step-26-shape");
    same_shape_as_init(&home, &conn(&made), "25");
}

/// The CHECK of `answers.choice` is the words of the choices, which the
/// command, the verb and the dispatcher read in tehas_core::question.
#[test]
fn the_answers_take_the_words_of_the_choices() {
    let home = home("answers-check");
    let words: Vec<String> = tehas_core::question::CHOICES
        .iter()
        .map(|c| format!("'{}'", c.word()))
        .collect();
    let want = format!("CHECK (choice IN ({}))", words.join(","));
    let got = checks(&conn(&home), "answers");
    assert!(got.contains(&want), "{got:?} does not hold {want}");
}

#[test]
fn the_steps_are_the_ladder_from_v0_30_up_to_the_schema_this_binary_writes() {
    // The list migrate walks is one entry per schema, in order, each once: a
    // step put after a later one, or left out, or doubled, is caught here.
    let targets: Vec<i64> = db::STEPS.iter().map(|(to, _)| *to).collect();
    assert_eq!(
        targets,
        (db::OLDEST + 1..=db::SCHEMA_VERSION).collect::<Vec<_>>()
    );
    assert_eq!(db::OLDEST, RELEASES[0].1);
}

#[test]
fn a_store_above_this_schema_is_future() {
    let home = home("schema-future");
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

#[test]
fn load_brings_a_copy_of_a_0_30_store_forward_as_migrate_does() {
    for (name, old) in [
        ("12", release("load-old-12", 12)),
        ("15", at_15("load-old-15")),
        ("16", at_16("load-old-16")),
        ("17", at_17("load-old-17")),
        ("v0.31.0", release("load-old-v0.31.0", 16)),
        ("v0.32.0", release("load-old-v0.32.0", 17)),
    ] {
        let from = one::<i64>(&old, "PRAGMA user_version");
        let into = home(&format!("load-old-into-{name}"));
        let copy = db::path(&old);
        let v = call(&into, &["load", "--from", copy.to_str().unwrap()]).unwrap();
        assert_eq!(v["schema"], from, "{v}");
        assert!(v["rows"].as_i64().unwrap() > 0, "{v}");
        assert_eq!(one::<i64>(&into, "PRAGMA user_version"), db::SCHEMA_VERSION);
        assert_eq!(count(&into, "cycles WHERE issue = 7"), 1);
        assert_eq!(count(&into, "plan_history WHERE issue = 7"), 1);
        // The copy itself is read, never changed.
        assert_eq!(one::<i64>(&old, "PRAGMA user_version"), from);
    }
}

/// The sources of the verbs, without their tests.
const VERB_SOURCES: [(&str, &str); 22] = [
    ("answer.rs", include_str!("answer.rs")),
    ("approval.rs", include_str!("approval.rs")),
    ("batch.rs", include_str!("batch.rs")),
    ("call.rs", include_str!("call.rs")),
    ("cycle.rs", include_str!("cycle.rs")),
    ("db.rs", include_str!("db.rs")),
    ("history.rs", include_str!("history.rs")),
    ("journal.rs", include_str!("journal.rs")),
    ("lib.rs", include_str!("lib.rs")),
    ("live.rs", include_str!("live.rs")),
    ("load.rs", include_str!("load.rs")),
    ("lock.rs", include_str!("lock.rs")),
    ("main.rs", include_str!("main.rs")),
    ("order.rs", include_str!("order.rs")),
    ("park.rs", include_str!("park.rs")),
    ("plan.rs", include_str!("plan.rs")),
    ("plugin.rs", include_str!("plugin.rs")),
    ("poll.rs", include_str!("poll.rs")),
    ("record.rs", include_str!("record.rs")),
    ("say.rs", include_str!("say.rs")),
    ("upkeep.rs", include_str!("upkeep.rs")),
    ("verbs.rs", include_str!("verbs.rs")),
];

/// Whether `src` holds an INSERT or UPDATE of `table` written out by name.
fn writes(src: &str, table: &str) -> bool {
    [
        "INSERT INTO ",
        "INSERT OR IGNORE INTO ",
        "INSERT OR REPLACE INTO ",
        "UPDATE ",
    ]
    .iter()
    .any(|kw| {
        src.match_indices(&format!("{kw}{table}")).any(|(at, m)| {
            !src[at + m.len()..].starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_')
        })
    })
}

/// A table nobody writes is a table to drop (#1770): every counted table has
/// a verb that writes it.
#[test]
fn every_counted_table_has_a_writer() {
    for t in db::COUNTED_TABLES {
        assert!(
            VERB_SOURCES.iter().any(|(_, s)| writes(s, t)),
            "no verb writes {t}: drop it, or name its writer"
        );
    }
    for t in DROPPED {
        assert!(!db::COUNTED_TABLES.contains(&t), "{t}");
    }
}

#[test]
fn the_writer_test_sees_every_verb_source() {
    let mut files: Vec<String> = fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.ends_with(".rs") && !n.ends_with("_tests.rs"))
        // `test-write` is no host's writer.
        .filter(|n| n != "tests.rs" && n != "exercise.rs")
        .collect();
    files.sort();
    let named: Vec<&str> = VERB_SOURCES.iter().map(|(n, _)| *n).collect();
    assert_eq!(files, named);
}

/// The tables schema 11 drops (#1770): no verb wrote them and nothing read them.
const DROPPED: [&str; 18] = [
    "branch_checks",
    "check_product",
    "cycle_locks",
    "edge_takeover",
    "endpoints",
    "energy_samples",
    "health_samples",
    "improve_cap",
    "index_state",
    "limits",
    "pause_flag",
    "platform_state",
    "poll_demand",
    "queue_snapshot",
    "refresh_hold",
    "refresh_requests",
    "refined",
    "release_history",
];

/// No dropped table comes back: no step and no init lays one.
#[test]
fn the_dropped_tables_stay_dropped_after_a_migrate() {
    let home = release("dropped-stay", 12);
    migrate(&home).unwrap();
    migrate_at(&home, NOW + 1).unwrap();
    let left = tables(&conn(&home));
    for t in DROPPED {
        assert!(!left.contains(&t.to_string()), "{t}");
        assert!(
            !include_str!("schema.sql").contains(&format!("CREATE TABLE {t} ")),
            "{t}"
        );
    }
}
