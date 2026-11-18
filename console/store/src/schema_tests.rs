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

/// The store each tagged release 1.0 updates from makes, by the schema it is
/// at: v0.30.0 wrote 12, v0.30.1 13, v0.30.2 to v0.30.6 14 with one
/// schema.sql, v0.31.0 16 and v0.32.0 17.
const RELEASES: [(&str, i64, &str); 5] = [
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

/// The columns of a contractor's report, which schema 17 adds.
const REPORT_COLUMNS: [&str; 5] = [
    "report_image",
    "report_rukas",
    "report_mem_mb",
    "report_disk_kb",
    "report_at",
];

/// The SQL of the step to `to`, as one transaction runs it.
fn step_sql(to: i64) -> &'static str {
    match to {
        13 => db::STEP_13,
        14 => db::STEP_14,
        15 => db::OUTSOURCE_SCHEMA,
        16 => db::STEP_16,
        17 => db::STEP_17,
        18 => db::STEP_18,
        _ => unreachable!("no step to {to}"),
    }
}

/// Every table but meta, with its rows, in rowid order.
fn rows(c: &Connection) -> Vec<(String, usize)> {
    tables(c)
        .into_iter()
        .filter(|t| t != "meta")
        .map(|t| {
            let n = dump(c, &t, &columns(c, &t)).len();
            (t, n)
        })
        .collect()
}

/// The rows of every table but the outsource ones, which a store of 15 or
/// later has and an older one does not.
fn host_rows(c: &Connection) -> Vec<(String, usize)> {
    rows(c)
        .into_iter()
        .filter(|(t, _)| !db::OUTSOURCE_TABLES.contains(&t.as_str()))
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
    migrate_with(home, false, NOW)
}

/// `store migrate` with or without `--outsource`, at `now`: two migrations of
/// one store in one second would name the same copy.
fn migrate_with(home: &Path, outsource: bool, now: i64) -> Result<String, (u8, String)> {
    execute(
        home,
        &Verb::Migrate {
            backup: home.join("state").join("backup"),
            settings: settings(),
            outsource,
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
fn init_writes_schema_18() {
    assert_eq!(db::SCHEMA_VERSION, 18);
    let home = home("schema-18");
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), 18);
    assert_eq!(
        one::<String>(&home, "SELECT value FROM meta WHERE key = 'schema'"),
        "18"
    );
    // The five tables of the feature are laid by every init, and stay empty
    // while it is off.
    for t in db::OUTSOURCE_TABLES {
        assert!(tables(&conn(&home)).iter().any(|n| n == t), "{t}");
        assert_eq!(count(&home, t), 0, "{t}");
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
    // Schema 17: a contractor keeps its last report. Schema 18: a job keeps
    // the hash of its token of each scope.
    let jobs = columns(&c, "jobs");
    for col in ["model_token_sha256", "job_token_sha256"] {
        assert!(jobs.contains(&col.to_string()), "{col}");
    }
    assert!(!jobs.contains(&"token_sha256".to_string()));
    for col in REPORT_COLUMNS {
        assert!(
            columns(&c, "contractors").contains(&col.to_string()),
            "{col}"
        );
    }
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
            if !db::OUTSOURCE_TABLES.contains(&t.as_str()) {
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
        // Every row kept; the outsource tables are new and empty.
        let after = host_rows(&conn(&home));
        assert_eq!(after, before, "{version}");
        for t in db::OUTSOURCE_TABLES {
            assert_eq!(count(&home, t), 0, "{t} of {version}");
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
        let copy = home
            .join("state/backup")
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
    migrate_with(&home, false, NOW + 1).unwrap();
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
        let backups = home.join("state").join("backup");
        let kept = fs::read_dir(&backups).map_or(0, |d| d.count());
        assert_eq!(kept, 0, "{}", backups.display());
        // The dry run, which install.sh asks before it copies anything,
        // refuses it the same way.
        let (code, line) = execute(&home, &Verb::MigrateDry, NOW).unwrap_err();
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
        let said = execute(&home, &Verb::MigrateDry, NOW).unwrap();
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
fn the_dry_run_of_migrate_takes_no_other_word() {
    let p = |a: &[&str]| crate::parse(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    assert_eq!(p(&["migrate", "--dry-run"]), Some(Verb::MigrateDry));
    assert_eq!(
        p(&["migrate", "--dry-run", "--json"]),
        Some(Verb::MigrateDry)
    );
    assert_eq!(p(&["migrate", "--dry-run", "--backup", "/x"]), None);
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

/// The step to 17 keeps every outsource row and adds its columns empty: a job
/// given out before it has no token's hash, a contractor no report.
#[test]
fn the_step_from_16_keeps_the_outsource_rows_and_adds_its_columns_empty() {
    let home = at_16("step-17-rows");
    conn(&home)
        .execute_batch(&format!(
            "INSERT OR IGNORE INTO issues (issue, kind, kind_at, checked_at) VALUES (7, 'issue', 1, 1);
             INSERT INTO contractors (name, public_key, added_at, last_seen_at) VALUES ('box1', '{KEY}', 100, 200);
             INSERT INTO jobs (issue, contractor, attempt, state, offered_at, expires_at, updated_at)
               VALUES (7, 'box1', 1, 'running', 100, 4000, 100);
             INSERT INTO job_history (job, at, to_state, said_by) VALUES (1, 100, 'offered', 'factory');
             INSERT INTO issue_attempts (issue, given) VALUES (7, 1);"
        ))
        .unwrap();
    assert!(conn(&home)
        .execute_batch("UPDATE jobs SET token_sha256 = 'x'")
        .is_err());
    let before = rows(&conn(&home));
    migrate(&home).unwrap();
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), db::SCHEMA_VERSION);
    assert_eq!(rows(&conn(&home)), before);
    assert_eq!(
        count(
            &home,
            "jobs WHERE model_token_sha256 IS NULL AND job_token_sha256 IS NULL"
        ),
        1
    );
    for col in REPORT_COLUMNS {
        assert_eq!(
            count(&home, &format!("contractors WHERE {col} IS NULL")),
            1,
            "{col}"
        );
    }
    assert_eq!(
        one::<i64>(
            &home,
            "SELECT last_seen_at FROM contractors WHERE name = 'box1'"
        ),
        200
    );
}

/// The step to 18 keeps a job's token hash as the hash of its job scope and
/// adds the model scope empty.
#[test]
fn the_step_from_17_keeps_the_token_hash_as_the_job_scope() {
    let home = at_17("step-18-rows");
    let sha = "1".repeat(64);
    conn(&home)
        .execute_batch(&format!(
            "INSERT OR IGNORE INTO issues (issue, kind, kind_at, checked_at) VALUES (7, 'issue', 1, 1);
             INSERT INTO contractors (name, public_key, added_at) VALUES ('box1', '{KEY}', 100);
             INSERT INTO jobs (issue, contractor, attempt, state, offered_at, expires_at, updated_at, token_sha256)
               VALUES (7, 'box1', 1, 'running', 100, 4000, 100, '{sha}');
             INSERT INTO jobs (issue, contractor, attempt, state, offered_at, expires_at, updated_at)
               VALUES (7, 'box1', 2, 'offered', 100, 4000, 100);"
        ))
        .unwrap();
    let before = rows(&conn(&home));
    migrate(&home).unwrap();
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), 18);
    assert_eq!(rows(&conn(&home)), before);
    assert_eq!(
        count(
            &home,
            &format!(
                "jobs WHERE id = 1 AND job_token_sha256 = '{sha}' AND model_token_sha256 IS NULL"
            )
        ),
        1
    );
    assert_eq!(
        count(
            &home,
            "jobs WHERE id = 2 AND model_token_sha256 IS NULL AND job_token_sha256 IS NULL"
        ),
        1
    );
    assert!(!columns(&conn(&home), "jobs").contains(&"token_sha256".to_string()));
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

#[test]
fn a_store_of_14_migrated_with_the_feature_on_takes_its_writes() {
    let home = release("release-14-on", 14);
    let cycles = count(&home, "cycles");
    migrate_with(&home, true, NOW).unwrap();
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), db::SCHEMA_VERSION);
    assert_eq!(
        count(&home, "meta WHERE key = 'outsource' AND value = '1'"),
        1
    );
    let args = ["contractor-add", "--name", "box1", "--key", KEY];
    call_word_at(&home, NOW, &args, "added").unwrap();
    assert_eq!(count(&home, "contractors"), 1);
    assert_eq!(count(&home, "cycles"), cycles);
}

#[test]
fn a_migrated_store_with_the_feature_off_writes_no_outsource_row() {
    let home = release("release-14-off", 14);
    migrate(&home).unwrap();
    let args = ["contractor-add", "--name", "box1", "--key", KEY];
    call_word_at(&home, NOW, &args, "refused").unwrap();
    assert_eq!(count(&home, "contractors"), 0);
}

/// A public key as a contractor gives it.
const KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// The sources of the verbs, without their tests.
const VERB_SOURCES: [(&str, &str); 21] = [
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
    ("outsource.rs", include_str!("outsource.rs")),
    ("park.rs", include_str!("park.rs")),
    ("plan.rs", include_str!("plan.rs")),
    ("poll.rs", include_str!("poll.rs")),
    ("review_issue.rs", include_str!("review_issue.rs")),
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
    migrate_with(&home, false, NOW + 1).unwrap();
    let left = tables(&conn(&home));
    for t in DROPPED {
        assert!(!left.contains(&t.to_string()), "{t}");
        assert!(
            !include_str!("schema.sql").contains(&format!("CREATE TABLE {t} ")),
            "{t}"
        );
    }
}
