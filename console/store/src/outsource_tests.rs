//! The outsource state: the job machine of "Job states", its history rows, the
//! three losses of "Durability", the attempt number, the retry record, the
//! one-time codes, and the guarantee that a factory with the feature off has
//! no table of it.

use crate::db;
use crate::verbs_tests::*;
use serde_json::Value;
use std::path::Path;

const KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const KEY_B: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
/// The hash of a code, as the command that holds the code passes it. The store
/// is given the hash and never the code, so this is the value the row of
/// `join_codes` holds. The code a test prints for a host is the unhashed one
/// below: what the store is never given, and what no row of it may hold.
const CODE: &str = "6b5586591612048389628461405e114d36619b1383e06f18f1d7e49fa9c75a02";
const CODE_B: &str = "d8451e7750f1e01e2e9e5c3a67a0e2e34db21a70e21d9a11c6f59a2e2b41f5f0";

/// A code as it is printed for a host, and the secret of the invite. The tests
/// pass the hash of a code to the store, so these two are the strings a
/// contractor's host holds and no byte of a store may hold. They are not the
/// hashes of the two above: the store is never given either, and a scan of the
/// file for them is the scan of a store that has leaked one.
const PRINTED: &str = "9f2c41b7ae0d385c6e1f0b9a4d73e2c8518ba6f4d0c39e7b2a15864df0e73c21";
const PRINTED_B: &str = "41b79f2c85c6e1f0d3852c41b7ae0d389f4d73e2c8518ba6f4d0c39e7b2a1586";

fn settings() -> crate::Settings {
    crate::Settings {
        version: "v0.10.2".to_string(),
        commit: "cb6912a".to_string(),
        instance: "tehas-tehas".to_string(),
        repo: "example-owner/example-product".to_string(),
    }
}

/// A store made with the feature on, as the command that loaded
/// `TEHAS_OUTSOURCE=1` makes it.
fn on(name: &str) -> TestDir {
    made(name, true)
}

/// Whether a home's store holds the five tables, read the way every read verb
/// of the store reads it. A test that opened the file itself with the plain
/// `Connection::open` would read only the file: with WAL on, the master record
/// of a store that was just widened is in the -wal beside it, and the plain
/// open says the tables are not there when they are.
fn holds(home: &Path) -> bool {
    let conn = db::open_read_only(&db::path(home)).unwrap();
    db::has_outsource(&conn).unwrap()
}

/// A store made by `store init`, with the feature or without it.
fn made(name: &str, outsource: bool) -> TestDir {
    let home = scratch(name);
    short_bounds();
    crate::execute(
        &home,
        &crate::Verb::Init {
            force: false,
            settings: settings(),
            outsource,
        },
        NOW,
    )
    .unwrap();
    home
}

/// A store made by a command that loaded nothing: the store of a factory that
/// does not outsource, which holds no table of the feature.
fn off(name: &str) -> TestDir {
    off_at(name, NOW)
}

/// `off` with the time the init row of meta is to carry.
fn off_at(name: &str, now: i64) -> TestDir {
    let home = scratch(name);
    short_bounds();
    crate::execute(
        &home,
        &crate::Verb::Init {
            force: false,
            settings: settings(),
            outsource: false,
        },
        now,
    )
    .unwrap();
    home
}

/// `store migrate` as the command that loaded the instance's settings runs it.
/// The store of a host is made before it is brought forward, so the verb is
/// asked of a home that holds one.
/// `store migrate` as the command that loaded the instance's settings runs it:
/// `--outsource` when the instance has TEHAS_OUTSOURCE=1, and nothing when it
/// has not. `now` is the second of the call, which the verb puts in the name of
/// the copy it leaves: two migrations of one store must not be asked of one
/// second, or the second copy is refused as a file that is there already.
fn migrate_at(home: &Path, outsource: bool, now: i64) -> Value {
    crate::execute(
        home,
        &crate::Verb::Migrate {
            backup: home.join("state").join("backup"),
            settings: settings(),
            outsource,
        },
        now,
    )
    .unwrap()
    .parse()
    .unwrap()
}

fn add(home: &Path, name: &str) -> Value {
    out(home, &["contractor-add", "--name", name, "--key", KEY])
}

/// The words the verbs of outsource.rs answer with: what happened, which a
/// person or a later part reads, rather than the verb that was asked. A verb
/// that writes has its own word for each outcome, and every verb of the module
/// answers `refused` too, which is the one word of a store that holds no table
/// of the feature.
const SAID: &[(&str, &[&str])] = &[
    ("contractor-add", &["added", "used"]),
    ("contractor-list", &["contractors"]),
    ("contractor-seen", &["seen"]),
    ("contractor-rest", &["resting"]),
    ("contractor-drain", &["draining"]),
    ("contractor-revoke", &["revoked"]),
    ("invite-new", &["invited"]),
    ("invite-list", &["invites"]),
    ("invite-use", &["joined", "expired", "unknown", "used"]),
    ("invite-expire", &["expired"]),
    ("job-offer", &["offered", "refused"]),
    ("job-check-in", &["checked-in", "refused", "stale"]),
    ("job-finish", &["finished", "refused", "stale"]),
    ("job-release", &["released", "refused", "stale"]),
    ("job-stop", &["stopping", "refused", "stale"]),
    ("job-lose", &["lost", "refused"]),
    ("job-show", &["job"]),
    ("jobs-sweep", &["swept"]),
    ("job-attempts", &["attempts"]),
];

/// The words one verb may answer with, its own and the one of the feature
/// being off.
fn said_of(verb: &str) -> Vec<&'static str> {
    SAID.iter()
        .find(|(v, _)| *v == verb)
        .unwrap_or_else(|| panic!("no verb of the feature named {verb}"))
        .1
        .iter()
        .copied()
        .chain(std::iter::once(REFUSED))
        .collect()
}

/// The one word of a store that holds no table of the feature.
const REFUSED: &str = "refused";

/// A call of one of these verbs: the answer is one of the words the verb
/// states, which is what a caller of the feature reads.
fn out(home: &Path, args: &[&str]) -> Value {
    out_at(home, NOW, args)
}

/// `out` at a second the test names.
fn out_at(home: &Path, now: i64, args: &[&str]) -> Value {
    let v = answer(home, now, args).unwrap_or_else(|e| panic!("{args:?}: {e:?}"));
    // The -wal holds the rows of a run that has not checkpointed, so it is read
    // the same way. The two strings no store of a factory may hold are looked
    // for in both files, whatever the call was.
    for suffix in ["", "-wal", "-journal"] {
        let side = db::side_file(&db::path(home), suffix);
        if let Ok(file) = std::fs::read(&side) {
            let text = String::from_utf8_lossy(&file);
            for word in [PRINTED, PRINTED_B] {
                for needle in [word.to_string(), split_of(word)] {
                    assert!(
                        !text.contains(&needle),
                        "{} holds the code {word}",
                        side.display()
                    );
                }
            }
        }
    }
    let said = v["verb"].as_str().unwrap_or_default().to_string();
    assert!(
        said_of(args[0]).contains(&said.as_str()),
        "{} answered {said}: {v}",
        args[0]
    );
    v
}

/// The same string with a NUL between its halves, which is how a page of a
/// database can hold a value that has been spilled and copied.
fn split_of(a: &str) -> String {
    let (a, b) = a.split_at(a.len() / 2);
    format!("{a}\0{b}")
}

/// The exit and the word of a call of the feature that failed.
fn refused_out(home: &Path, args: &[&str]) -> (u8, String) {
    answer(home, NOW, args)
        .err()
        .unwrap_or_else(|| panic!("{args:?} was answered"))
}

/// The rows of one key of an answer, each of them an object the verb rendered.
fn objects(v: &Value, key: &str) -> Vec<Value> {
    v[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} of {v} is no array"))
        .iter()
        .map(|row| match row {
            Value::String(text) => text
                .parse()
                .unwrap_or_else(|e| panic!("{key}: {e}: {text}")),
            other => panic!("{key}: a row that is not an object: {other}"),
        })
        .collect()
}

/// The first row of a list verb, read as the object it is.
fn first_row(home: &Path, verb: &str) -> Value {
    first_row_at(home, NOW, verb)
}

fn first_row_at(home: &Path, now: i64, verb: &str) -> Value {
    let all = call_at_any(home, now, &[verb]);
    let rows = all["rows"]
        .as_array()
        .unwrap_or_else(|| panic!("{verb}: {all}"));
    match &rows[0] {
        Value::String(text) => text
            .parse()
            .unwrap_or_else(|e| panic!("{verb}: {e}: {text}")),
        other => panic!("{verb}: a row that is not an object: {other}"),
    }
}

/// The rows of one of the two list verbs, read as the objects they are.
fn rows_of(home: &Path, verb: &str) -> Vec<Value> {
    out(home, &[verb])["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| match row {
            Value::String(text) => text
                .parse()
                .unwrap_or_else(|e| panic!("{verb}: {e}: {text}")),
            other => panic!("{verb}: a row that is not an object: {other}"),
        })
        .collect()
}

/// A call as main() makes it, with no word asked of the answer.
fn call_at_any(home: &Path, now: i64, args: &[&str]) -> Value {
    answer(home, now, args).unwrap_or_else(|e| panic!("{args:?}: {e:?}"))
}

/// The line one call wrote, or the exit and the word it failed with.
fn answer(home: &Path, now: i64, args: &[&str]) -> Result<Value, (u8, String)> {
    let call = crate::verbs::parse(&strings(args))
        .unwrap_or_else(|| panic!("{args:?} is no record verb"))
        .map_err(|why| (64, why))?;
    crate::execute(home, &crate::Verb::Record(call), now)
        .map(|line| {
            assert_eq!(line.lines().count(), 1, "{line}");
            line.parse().unwrap()
        })
        .map_err(|(code, line)| {
            // The word the caller reads is the first of the line after the
            // command's own prefix: `nojob`, `unknown`, `conflict`, or the
            // class of the database.
            let why = line.strip_prefix("store: ").unwrap_or(&line);
            (
                code,
                why.split([' ', ':']).next().unwrap_or(why).to_string(),
            )
        })
}

/// A cycle of `issue` that ran and ended, so a job may refer to it.
fn issue(home: &Path, n: i64) {
    let (lock, attempt) = start(home, n);
    end(home, n, attempt, "closed");
    // The cycle's lock is the store's way of saying a cycle runs: a job of the
    // issue is given out only when no cycle of it holds the lock, so the test
    // lets go of it here rather than at the end of its own scope.
    drop(lock);
}

/// The first job of `name`, given for an issue the store holds and whose own
/// cycle has ended. The name is registered if the store does not hold it yet,
/// and a machine that still holds a job of the test's own giving is asked to
/// hand it back first: a contractor takes one job at a time, so a second offer
/// into the same hand is refused as `busy`.
fn offered(home: &Path, issue: i64, name: &str) -> Value {
    if !rows(
        home,
        &format!("SELECT name FROM contractors WHERE name = '{name}'"),
    )
    .iter()
    .any(|n| n == name)
    {
        add(home, name);
    }
    if let Some(id) = open_of_name(home, name) {
        out(home, &["job-release", "--job", &id.to_string()]);
    }
    offer(home, issue, name)
}

/// The job a contractor still holds, by name rather than by the store's idea of
/// what is open: a row past its own bound is kept until the sweep loses it, and
/// a test that wants the machine back releases the row it knows of.
fn open_of_name(home: &Path, name: &str) -> Option<i64> {
    conn(home)
        .query_row(
            "SELECT id FROM jobs WHERE contractor = ?1
               AND state IN ('offered','running','stopping') ORDER BY id",
            rusqlite::params![name],
            |r| r.get::<_, i64>(0),
        )
        .ok()
}

/// `job-offer` of one issue to one contractor, which the store refuses when the
/// machine of that contractor holds another job.
fn offer(home: &Path, issue: i64, name: &str) -> Value {
    out(
        home,
        &[
            "job-offer",
            "--issue",
            &issue.to_string(),
            "--contractor",
            name,
        ],
    )
}

fn rows(home: &Path, sql: &str) -> Vec<String> {
    let c = conn(home);
    let mut q = c.prepare(sql).unwrap();
    q.query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

fn held(home: &Path, table: &str) -> i64 {
    count(home, table)
}

/// The names of the tables of a store.
fn tables(home: &Path) -> Vec<String> {
    rows(
        home,
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
    )
}

// --- the feature's tables ----------------------------------------------------

#[test]
fn a_store_with_the_feature_off_holds_no_table_of_it_and_refuses_its_verbs() {
    let home = off("off");
    for table in db::OUTSOURCE_TABLES {
        assert!(
            !tables(&home).iter().any(|n| n == table),
            "{table} is there"
        );
    }
    // The verbs answer the one refusal, and write nothing: the store is not
    // even opened for a row.
    let v = out(&home, &["contractor-add", "--name", "box1", "--key", KEY]);
    assert_eq!(v["verb"], "refused");
    assert_eq!(v["said"], "outsource is off");
    for args in [
        vec!["contractor-list"],
        vec!["invite-list"],
        vec!["job-attempts", "--issue", "1"],
        vec!["jobs-sweep", "--timeout", "60"],
        vec!["job-show", "--job", "1"],
        vec!["job-check-in", "--job", "1"],
    ] {
        assert_eq!(out(&home, &args)["verb"], "refused", "{args:?}");
    }
    // The counts of `check` name the same tables they named before the feature.
    let report = crate::execute(&home, &crate::Verb::Check, NOW)
        .unwrap()
        .parse::<Value>()
        .unwrap();
    for table in db::OUTSOURCE_TABLES {
        assert!(report["rows"].get(table).is_none(), "{table} is counted");
    }
    assert_eq!(
        report["rows"].as_object().unwrap().len(),
        db::COUNTED_TABLES.len()
    );
    assert_eq!(report["schema"], db::SCHEMA_VERSION);
}

#[test]
fn the_init_of_an_off_call_writes_no_word_of_the_feature_to_the_file() {
    // The store's one guard is the tables, and a row cannot exist without one,
    // but a call of this issue's guarantee is made against the bytes as well:
    // a store made by an init that says the instance does not outsource holds
    // no name of the feature at all, so a reader of a host's file cannot tell
    // it from a store that predates the feature.
    let home = off("off-quiet");
    for file in ["", "-wal"] {
        let name = db::side_file(&db::path(&home), file);
        if !name.exists() {
            continue;
        }
        let text = String::from_utf8_lossy(&std::fs::read(&name).unwrap()).to_lowercase();
        for word in [
            "contractor",
            "join_codes",
            "job_",
            "jobs",
            "issue_attempts",
            "outsource",
        ] {
            assert!(!text.contains(word), "{name:?} says {word}");
        }
    }
    // The counts of `check` are the 24 of the release before the feature, and
    // no call of the feature adds one: it is refused before it reads a row.
    let report = crate::execute(&home, &crate::Verb::Check, NOW)
        .unwrap()
        .parse::<Value>()
        .unwrap();
    assert_eq!(
        report["rows"].as_object().unwrap().len(),
        db::COUNTED_TABLES.len()
    );
    let before = report["rows"].clone();
    out(&home, &["contractor-add", "--name", "box1", "--key", KEY]);
    out(
        &home,
        &["job-offer", "--issue", "1", "--contractor", "box1"],
    );
    let after = crate::execute(&home, &crate::Verb::Check, NOW)
        .unwrap()
        .parse::<Value>()
        .unwrap();
    assert_eq!(after["rows"], before, "a refused call wrote a row");
}

#[test]
fn a_store_with_the_feature_on_holds_the_five_tables_and_counts_them() {
    let home = on("on");
    for table in db::OUTSOURCE_TABLES {
        assert!(
            tables(&home).iter().any(|n| n == table),
            "{table} is missing"
        );
    }
    assert_eq!(
        tables(&home).len(),
        db::COUNTED_TABLES.len() + db::OUTSOURCE_TABLES.len()
    );
    let report = crate::execute(&home, &crate::Verb::Check, NOW)
        .unwrap()
        .parse::<Value>()
        .unwrap();
    for table in db::OUTSOURCE_TABLES {
        assert_eq!(report["rows"][table], 0, "{table}");
    }
    assert_eq!(
        report["rows"].as_object().unwrap().len(),
        db::COUNTED_TABLES.len() + db::OUTSOURCE_TABLES.len()
    );
}

#[test]
fn a_migrate_lays_the_tables_for_an_on_call_and_never_for_an_off_one() {
    let home = made("migrate-on", true);
    assert!(holds(&home), "init with the feature on made no table");
    migrate_at(&home, false, NOW);
    // A store that holds them keeps them: a migrate that says the feature off
    // does not take the feature away, which would be a person's rows.
    assert!(holds(&home));
    // And a store that does not hold them is not widened by a call that says
    // nothing, nor by a second call; a call that says the feature on lays them.
    let off = off_at("migrate-off", NOW - 100);
    assert!(!holds(&off), "init with the feature off made a table");
    migrate_at(&off, false, NOW);
    assert!(!holds(&off));
    migrate_at(&off, false, NOW + 1);
    assert!(!holds(&off));
    migrate_at(&off, true, NOW + 2);
    assert!(holds(&off));
}

#[test]
fn a_store_brought_forward_from_the_release_before_keeps_its_rows_and_gains_the_tables() {
    // The tables arrive with the step to 15, so a store of the release before
    // them comes forward with every row it holds. The store of 14 is the one
    // of the release before this binary: made without the tables, which is
    // what a host that never turned the feature on holds today.
    let home = off_at("forward", NOW - 100);
    let (_lock, attempt) = start(&home, 4);
    end(&home, 4, attempt, "closed");
    let before = count(&home, "cycle_history");
    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), db::SCHEMA_VERSION);
    assert!(!holds(&home));

    migrate_at(&home, true, NOW);

    assert_eq!(one::<i64>(&home, "PRAGMA user_version"), db::SCHEMA_VERSION);
    assert_eq!(
        count(&home, "cycle_history"),
        before,
        "the step to the tables lost a row"
    );
    for table in db::OUTSOURCE_TABLES {
        assert!(tables(&home).iter().any(|n| n == table), "{table}");
    }
    // The store of the release before the tables is one the feature reads: a
    // job of an issue it already holds is a job of that issue.
    add(&home, "box1");
    let v = out(
        &home,
        &["job-offer", "--issue", "4", "--contractor", "box1"],
    );
    assert_eq!(v["issue"], 4);
    assert_eq!(v["attempt"], 1);
    let id = v["job"].as_i64().unwrap();
    assert_eq!(
        out(&home, &["job-show", "--job", &id.to_string()])["state"],
        "offered"
    );
}

// --- the contractors ---------------------------------------------------------

#[test]
fn a_contractor_is_added_once_and_holds_only_its_public_key() {
    let home = on("add");
    let v = add(&home, "box1");
    assert_eq!(v["verb"], "added");
    // Re-adding the same name and key answers the same and writes one row.
    let v = add(&home, "box1");
    assert_eq!(v["verb"], "added");
    assert_eq!(v["again"], 1);
    assert_eq!(held(&home, "contractors"), 1);
    // Another key for the same name is refused: the registered key is the
    // allowlist, so a stolen name cannot swap it.
    let v = out(&home, &["contractor-add", "--name", "box1", "--key", KEY_B]);
    assert_eq!(v["verb"], "used");
    assert_eq!(rows(&home, "SELECT public_key FROM contractors"), vec![KEY]);
    // Who said so is kept for the line a person reads.
    out(
        &home,
        &[
            "contractor-add",
            "--name",
            "box2",
            "--key",
            KEY_B,
            "--by",
            "alice",
        ],
    );
    let by: Option<String> = conn(&home)
        .query_row(
            "SELECT added_by FROM contractors WHERE name = 'box2'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(by.as_deref(), Some("alice"));
}

#[test]
fn a_contractors_list_says_what_a_person_asks_of_it() {
    let home = on("list");
    add(&home, "box2");
    add(&home, "box1");
    out(&home, &["contractor-seen", "--name", "box1"]);
    out(&home, &["contractor-rest", "--name", "box1", "--for", "60"]);
    out(&home, &["contractor-drain", "--name", "box2"]);
    let rows = rows_of(&home, "contractor-list");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["name"], "box1");
    assert_eq!(rows[0]["last_seen_at"], NOW);
    assert_eq!(rows[0]["resting"], true);
    assert_eq!(rows[0]["resting_until"], NOW + 60);
    assert_eq!(rows[0]["draining"], false);
    assert_eq!(rows[0]["revoked"], false);
    assert_eq!(rows[1]["name"], "box2");
    assert_eq!(rows[1]["draining"], true);
    assert_eq!(rows[1]["resting"], false);
    assert_eq!(rows[1]["last_seen_at"], Value::Null);
}

#[test]
fn a_name_the_store_does_not_hold_is_the_one_refusal() {
    let home = on("unknown");
    for args in [
        vec!["contractor-seen", "--name", "ghost"],
        vec!["contractor-rest", "--name", "ghost", "--for", "60"],
        vec!["contractor-drain", "--name", "ghost"],
        vec!["contractor-revoke", "--name", "ghost"],
    ] {
        let (code, why) = refused_out(&home, &args);
        assert_eq!((code, why.as_str()), (1, "unknown"), "{args:?}");
    }
}

#[test]
fn drain_is_said_both_ways_and_a_rest_period_holds_a_job_back() {
    let home = on("drain");
    issue(&home, 2);
    add(&home, "box1");
    assert_eq!(
        out(&home, &["contractor-drain", "--name", "box1"])["draining"],
        true
    );
    assert_eq!(
        out(&home, &["contractor-drain", "--name", "box1", "--on", "0"])["draining"],
        false
    );
    // A job waits for a contractor whose rest has not passed, and is given to
    // one whose rest has.
    out(&home, &["contractor-rest", "--name", "box1", "--for", "10"]);
    let v = out(
        &home,
        &["job-offer", "--issue", "2", "--contractor", "box1"],
    );
    assert_eq!(v["verb"], "refused");
    assert_eq!(v["said"], "resting");
    let v = out_at(
        &home,
        NOW + 11,
        &["job-offer", "--issue", "2", "--contractor", "box1"],
    );
    assert_eq!(v["verb"], "offered");
    // A draining contractor takes nothing.
    out(&home, &["contractor-drain", "--name", "box1"]);
    let v = out_at(
        &home,
        NOW + 12,
        &["job-offer", "--issue", "2", "--contractor", "box1"],
    );
    assert_eq!(v["said"], "draining");
    // The job went to no one, so the issue has spent nothing.
    assert_eq!(out(&home, &["job-attempts", "--issue", "2"])["given"], 1);
}

#[test]
fn a_revoked_contractor_loses_its_jobs_and_takes_no_new_one() {
    let home = on("revoke");
    issue(&home, 7);
    let id = offered(&home, 7, "box1")["job"].as_i64().unwrap();
    let v = out(&home, &["contractor-revoke", "--name", "box1"]);
    assert_eq!(v["verb"], "revoked");
    assert_eq!(v["lost"], 1);
    let v = out(&home, &["job-show", "--job", &id.to_string()]);
    assert_eq!(v["state"], "lost");
    assert_eq!(v["lost_reason"], "revoked");
    let v = out(
        &home,
        &["job-offer", "--issue", "7", "--contractor", "box1"],
    );
    assert_eq!(v["verb"], "refused");
    assert_eq!(v["said"], "revoked");
    // A second revocation is the answer it already is: nothing left to lose.
    assert_eq!(
        out(&home, &["contractor-revoke", "--name", "box1"])["lost"],
        0
    );
    // The sweep of a factory that comes back finds nothing more to lose.
    let v = out_at(
        &home,
        NOW + 60,
        &["jobs-sweep", "--timeout", "600", "--now"],
    );
    assert_eq!(v["lost"], 0);
    assert_eq!(v["revoked"], 0);
    assert_eq!(v["codes"], 0);
}

// --- the invite codes --------------------------------------------------------

#[test]
fn a_code_works_once_expires_and_a_wrong_one_is_unknown() {
    let home = on("codes");
    let v = out(
        &home,
        &[
            "invite-new",
            "--name",
            "box1",
            "--code",
            CODE,
            "--ttl",
            "100",
        ],
    );
    assert_eq!(v["verb"], "invited");
    assert_eq!(v["expires_at"], NOW + 100);
    // The right hash joins, and the name arrives with the key it gave.
    let v = out(
        &home,
        &["invite-use", "--code", CODE, "--name", "box1", "--key", KEY],
    );
    assert_eq!(v["verb"], "joined");
    assert_eq!(held(&home, "contractors"), 1);
    // The same code again is spent, whatever the caller says of it.
    let v = out(
        &home,
        &[
            "invite-use",
            "--code",
            CODE,
            "--name",
            "box9",
            "--key",
            KEY_B,
        ],
    );
    assert_eq!(v["verb"], "expired");
    assert_eq!(held(&home, "contractors"), 1);
    // A hash the store does not hold is nothing at all.
    let v = out(
        &home,
        &[
            "invite-use",
            "--code",
            CODE_B,
            "--name",
            "box2",
            "--key",
            KEY_B,
        ],
    );
    assert_eq!(v["verb"], "unknown");
    assert_eq!(held(&home, "contractors"), 1);
}

#[test]
fn a_code_past_its_expiry_joins_nothing_and_is_closed_out_by_the_sweep() {
    let home = on("expire");
    out(
        &home,
        &[
            "invite-new",
            "--name",
            "box1",
            "--code",
            CODE,
            "--ttl",
            "10",
        ],
    );
    assert_eq!(first_row(&home, "invite-list")["live"], true);
    assert_eq!(first_row_at(&home, NOW + 11, "invite-list")["live"], false);
    let v = out_at(
        &home,
        NOW + 11,
        &["invite-use", "--code", CODE, "--name", "box1", "--key", KEY],
    );
    assert_eq!(v["verb"], "expired");
    assert_eq!(held(&home, "contractors"), 0);
    let v = out_at(&home, NOW + 12, &["jobs-sweep", "--timeout", "60", "--now"]);
    assert_eq!(v["codes"], 1);
    // The row is kept: a code that expired is the record that a name was
    // invited and never came.
    assert_eq!(held(&home, "join_codes"), 1);
    assert_eq!(out(&home, &["invite-expire"])["codes"], 0);
}

#[test]
fn a_new_invite_replaces_the_code_that_was_never_used() {
    let home = on("replace");
    out(
        &home,
        &[
            "invite-new",
            "--name",
            "box1",
            "--code",
            CODE,
            "--ttl",
            "100",
        ],
    );
    out(
        &home,
        &[
            "invite-new",
            "--name",
            "box1",
            "--code",
            CODE_B,
            "--ttl",
            "100",
        ],
    );
    // One row, one live code: the first printing cannot be joined with.
    assert_eq!(held(&home, "join_codes"), 1);
    let v = out(
        &home,
        &["invite-use", "--code", CODE, "--name", "box1", "--key", KEY],
    );
    assert_eq!(v["verb"], "unknown");
    let v = out(
        &home,
        &[
            "invite-use",
            "--code",
            CODE_B,
            "--name",
            "box1",
            "--key",
            KEY,
        ],
    );
    assert_eq!(v["verb"], "joined");
}

#[test]
fn the_store_holds_no_code_and_no_private_key_of_any_kind() {
    let home = on("secret");
    out(
        &home,
        &[
            "invite-new",
            "--name",
            "box1",
            "--code",
            CODE,
            "--ttl",
            "100",
        ],
    );
    out(
        &home,
        &["invite-use", "--code", CODE, "--name", "box1", "--key", KEY],
    );
    let c = conn(&home);
    let names: Vec<String> = {
        let mut q = c.prepare("PRAGMA table_info(join_codes)").unwrap();
        q.query_map([], |r| r.get(1))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    };
    // The one column a code could be kept in is named for its hash.
    assert_eq!(
        names,
        ["name", "code_sha256", "created_at", "expires_at", "used_at"]
    );
    let keys: Vec<String> = {
        let mut q = c.prepare("PRAGMA table_info(contractors)").unwrap();
        q.query_map([], |r| r.get(1))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    };
    assert!(keys.contains(&"public_key".to_string()), "{keys:?}");
    assert!(!keys.iter().any(|k| k.contains("private")), "{keys:?}");
    // Nothing of the file holds the code that was joined with. A value written
    // into a row is there as its own bytes, so the string is looked for both
    // whole and with SQLite's own byte between them, the way a scan of a
    // store's file is made by hand.
    let whole = std::fs::read(db::path(&home)).unwrap();
    let text = String::from_utf8_lossy(&whole);
    for printed in [PRINTED, PRINTED_B] {
        for needle in [printed.to_string(), split_of(printed)] {
            assert!(!text.contains(&needle), "the store holds the code");
        }
    }
}

// --- the job machine ---------------------------------------------------------

#[test]
fn each_move_of_the_machine_writes_its_history_row() {
    let home = on("machine");
    issue(&home, 3);
    let v = offered(&home, 3, "box1");
    let id = v["job"].as_i64().unwrap();
    assert_eq!(v["verb"], "offered");
    assert_eq!(v["attempt"], 1);
    assert_eq!(v["contractor"], "box1");

    assert_eq!(
        out(&home, &["job-check-in", "--job", &id.to_string()])["verb"],
        "checked-in"
    );
    assert_eq!(
        out(&home, &["job-stop", "--job", &id.to_string()])["verb"],
        "stopping"
    );
    let v = out(
        &home,
        &["job-finish", "--job", &id.to_string(), "--sha256", CODE],
    );
    assert_eq!(v["verb"], "finished");
    assert_eq!(v["state"], "finished");

    let v = out(&home, &["job-show", "--job", &id.to_string()]);
    assert_eq!(v["state"], "finished");
    assert_eq!(v["result_sha256"], CODE);
    let history = objects(&v, "history");
    let said: Vec<(Option<&str>, &str, &str)> = history
        .iter()
        .map(|r| {
            (
                r["from"].as_str(),
                r["to"].as_str().unwrap(),
                r["by"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        said,
        vec![
            (None, "offered", "factory"),
            (Some("offered"), "running", "factory"),
            (Some("running"), "stopping", "factory"),
            (Some("stopping"), "finished", "factory"),
        ]
    );
    // The history rows are the moves: one each, in the order they were said.
    assert_eq!(
        rows(&home, "SELECT to_state FROM job_history ORDER BY at, id"),
        ["offered", "running", "stopping", "finished"]
    );
    assert_eq!(held(&home, "job_history"), 4);
    // The contractor that finished it is counted as one that did.
    assert_eq!(first_row(&home, "contractor-list")["jobs_done"], 1);
}

#[test]
fn a_release_is_not_a_failure_and_costs_the_contractor_nothing() {
    let home = on("release");
    issue(&home, 5);
    let id = offered(&home, 5, "box1")["job"].as_i64().unwrap();
    out(&home, &["job-check-in", "--job", &id.to_string()]);
    let v = out(&home, &["job-release", "--job", &id.to_string()]);
    assert_eq!(v["verb"], "released");
    assert_eq!(v["state"], "released");
    let v = first_row(&home, "contractor-list").clone();
    assert_eq!(v["jobs_done"], 0);
    assert_eq!(v["jobs_lost"], 0);
    // An offer may be handed back before it is taken, and the number has risen.
    let id = offered(&home, 5, "box1")["job"].as_i64().unwrap();
    assert_eq!(
        out(&home, &["job-show", "--job", &id.to_string()])["attempt"],
        2
    );
    assert_eq!(
        out(&home, &["job-release", "--job", &id.to_string()])["state"],
        "released"
    );
    // A job that has ended moves no further.
    let v = out(&home, &["job-release", "--job", &id.to_string()]);
    assert_eq!(v["verb"], "refused");
    assert_eq!(v["state"], "released");
}

#[test]
fn a_move_the_machine_does_not_have_is_said_and_changes_nothing() {
    let home = on("illegal");
    issue(&home, 6);
    let id = offered(&home, 6, "box1")["job"].as_i64().unwrap();
    // offered does not finish, and does not stop.
    for verb in ["job-finish", "job-stop"] {
        let v = out(&home, &[verb, "--job", &id.to_string()]);
        assert_eq!(v["verb"], "refused", "{verb}");
        assert_eq!(v["state"], "offered");
    }
    assert_eq!(held(&home, "job_history"), 1);
    out(&home, &["job-check-in", "--job", &id.to_string()]);
    out(&home, &["job-release", "--job", &id.to_string()]);
    // A job that released does not come back.
    for verb in ["job-check-in", "job-finish", "job-stop", "job-release"] {
        let v = out(&home, &[verb, "--job", &id.to_string()]);
        assert_eq!(v["verb"], "refused", "{verb}");
        assert_eq!(v["state"], "released");
    }
    assert_eq!(held(&home, "job_history"), 3);
}

#[test]
fn a_job_is_shown_by_its_number_or_by_its_issue_and_a_missing_one_is_one() {
    let home = on("show");
    issue(&home, 8);
    let id = offered(&home, 8, "box1")["job"].as_i64().unwrap();
    let v = out(&home, &["job-show", "--issue", "8"]);
    assert_eq!(v["job"], id);
    assert_eq!(v["issue"], 8);
    assert_eq!(
        refused_out(&home, &["job-show", "--job", "404"]),
        (1, "nojob".to_string())
    );
    assert_eq!(
        refused_out(&home, &["job-show", "--issue", "99"]),
        (1, "nojob".to_string())
    );
    // The current job of an issue is the open one of its highest attempt. The
    // first is handed back before the second is asked for, as a machine with
    // the one checkout does.
    out(&home, &["job-release", "--job", &id.to_string()]);
    let second = out(
        &home,
        &["job-offer", "--issue", "8", "--contractor", "box1"],
    )["job"]
        .as_i64()
        .unwrap();
    assert_eq!(out(&home, &["job-show", "--issue", "8"])["job"], second);
    out(&home, &["job-release", "--job", &second.to_string()]);
    assert_eq!(
        refused_out(&home, &["job-show", "--issue", "8"]),
        (1, "nojob".to_string())
    );
}

// --- the three losses --------------------------------------------------------

#[test]
fn a_silent_contractor_loses_its_job_past_the_timeout() {
    let home = on("silent");
    issue(&home, 9);
    let id = offered(&home, 9, "box1")["job"].as_i64().unwrap();
    out(&home, &["job-check-in", "--job", &id.to_string()]);
    // The timeout has not passed: the job is still the factory's business.
    assert_eq!(
        out_at(&home, NOW + 300, &["jobs-sweep", "--timeout", "600"])["lost"],
        0
    );
    assert_eq!(
        out(&home, &["job-show", "--job", &id.to_string()])["state"],
        "running"
    );
    // Past it, the sweep that every poll runs loses it.
    let v = out_at(&home, NOW + 601, &["jobs-sweep", "--timeout", "600"]);
    assert_eq!(v["verb"], "swept");
    assert_eq!(v["lost"], 1);
    let v = out(&home, &["job-show", "--job", &id.to_string()]);
    assert_eq!(v["state"], "lost");
    assert_eq!(v["lost_reason"], "timeout");
    // The contractor that lost it is counted; the issue has spent an attempt
    // and lost it.
    assert_eq!(first_row(&home, "contractor-list")["jobs_lost"], 1);
    let v = out(&home, &["job-attempts", "--issue", "9"]);
    assert_eq!(v["given"], 1);
    assert_eq!(v["lost"], 1);
    assert_eq!(v["open"], false);
    // The sweep is told twice and counts once.
    assert_eq!(
        out_at(&home, NOW + 700, &["jobs-sweep", "--timeout", "600"])["lost"],
        0
    );
    assert_eq!(held(&home, "job_history"), 3);
}

#[test]
fn a_job_never_taken_is_lost_from_the_second_it_was_offered() {
    let home = on("never");
    issue(&home, 10);
    add(&home, "box1");
    out(
        &home,
        &[
            "job-offer",
            "--issue",
            "10",
            "--contractor",
            "box1",
            "--expires",
            &(NOW + 300).to_string(),
        ],
    );
    assert_eq!(
        out_at(&home, NOW + 299, &["jobs-sweep", "--timeout", "600"])["lost"],
        0
    );
    assert_eq!(
        out_at(&home, NOW + 300, &["jobs-sweep", "--timeout", "600"])["lost"],
        1
    );
    assert_eq!(
        refused_out(&home, &["job-show", "--issue", "10"]),
        (1, "nojob".to_string())
    );
    let v = out(&home, &["job-show", "--job", "1"]);
    assert_eq!(v["state"], "lost");
    assert_eq!(v["lost_reason"], "expired");
}

#[test]
fn a_stopping_job_is_lost_by_the_timeout_like_a_running_one() {
    let home = on("stopping");
    issue(&home, 11);
    let id = offered(&home, 11, "box1")["job"].as_i64().unwrap();
    out(&home, &["job-check-in", "--job", &id.to_string()]);
    out(&home, &["job-stop", "--job", &id.to_string()]);
    assert_eq!(
        out_at(&home, NOW + 601, &["jobs-sweep", "--timeout", "600"])["lost"],
        1
    );
    let v = out(&home, &["job-show", "--job", &id.to_string()]);
    assert_eq!(v["state"], "lost");
    assert_eq!(v["lost_reason"], "timeout");
}

#[test]
fn a_job_that_never_checked_in_is_measured_from_its_offer() {
    let home = on("never-checked");
    issue(&home, 12);
    let id = offered(&home, 12, "box1")["job"].as_i64().unwrap();
    out(&home, &["job-check-in", "--job", &id.to_string()]);
    // The row of a job whose contractor's timer never reached the factory:
    // the timeout is counted from the second it was offered.
    conn(&home)
        .execute(
            "UPDATE jobs SET last_check_in_at = NULL WHERE id = ?1",
            rusqlite::params![id],
        )
        .unwrap();
    assert_eq!(
        out_at(&home, NOW + 599, &["jobs-sweep", "--timeout", "600"])["lost"],
        0
    );
    assert_eq!(
        out_at(&home, NOW + 600, &["jobs-sweep", "--timeout", "600"])["lost"],
        1
    );
}

#[test]
fn a_late_contractor_is_refused_by_its_attempt_number() {
    let home = on("late");
    issue(&home, 13);
    let first = offered(&home, 13, "box1")["job"].as_i64().unwrap();
    out(&home, &["job-check-in", "--job", &first.to_string()]);
    // The first job goes quiet and the sweep loses it; a second is given out.
    // The sweep that loses it says so, and says nothing more: a row it lost is
    // never lost twice, so a second call is the answer it already is.
    assert_eq!(
        out_at(&home, NOW + 601, &["jobs-sweep", "--timeout", "600"])["lost"],
        1
    );
    assert_eq!(
        out_at(&home, NOW + 602, &["jobs-sweep", "--timeout", "600"])["lost"],
        0
    );
    // The quiet job of the first machine has been lost by the sweep, so the
    // second job of the issue goes out on the same machine.
    let second = {
        let v = offer(&home, 13, "box1");
        assert_eq!(v["verb"], "offered", "{v}");
        v["job"].as_i64().unwrap()
    };
    assert_eq!(
        out(&home, &["job-show", "--job", &second.to_string()])["attempt"],
        2
    );
    // The first contractor comes back with its result. The store refuses it:
    // the attempt that replaced it is the one a result is honoured for.
    let v = out(
        &home,
        &["job-finish", "--job", &first.to_string(), "--sha256", CODE],
    );
    assert_eq!(v["verb"], "stale");
    assert_eq!(v["attempt"], 1);
    let v = out(&home, &["job-show", "--job", &first.to_string()]);
    assert_eq!(v["state"], "lost");
    assert_eq!(v["result_sha256"], Value::Null);
    // The row holds the moves it lived through and nothing of the calls that
    // came too late: offered, taken up, then lost by the sweep. The calls below
    // write no row.
    assert_eq!(
        objects(&v, "history")
            .iter()
            .map(|h| h["to"].as_str().unwrap().to_string())
            .collect::<Vec<_>>(),
        ["offered", "running", "lost"]
    );
    // A check-in and a stop of the old attempt are refused the same way, and
    // neither writes a history row. A release is not asked for here: the row is
    // lost already, and a job that is closed out has nothing to give back.
    for verb in ["job-check-in", "job-stop"] {
        assert_eq!(
            out(&home, &[verb, "--job", &first.to_string()])["verb"],
            "stale",
            "{verb}"
        );
    }
    // The job still open is given back, and the store says whose attempt it was.
    let v = out(&home, &["job-release", "--job", &second.to_string()]);
    assert_eq!(v["verb"], "released");
    // The current attempt is honoured: a fresh job of the issue, given after
    // the last was given back, is checked in and finished.
    let third = offered(&home, 13, "box3")["job"].as_i64().unwrap();
    out(&home, &["job-check-in", "--job", &third.to_string()]);
    assert_eq!(
        out(&home, &["job-finish", "--job", &third.to_string()])["state"],
        "finished"
    );
}

#[test]
fn the_attempt_number_rises_for_every_job_given_for_one_issue() {
    let home = on("attempts");
    issue(&home, 14);
    add(&home, "box1");
    for expect in 1..=4 {
        // Each job is given up before the next is asked for: one contractor
        // holds one job, so the attempts of an issue go out one at a time.
        if let Some(id) = open_of_name(&home, "box1") {
            out(&home, &["job-release", "--job", &id.to_string()]);
        }
        let v = out(
            &home,
            &["job-offer", "--issue", "14", "--contractor", "box1"],
        );
        assert_eq!(v["attempt"], expect);
        assert_eq!(v["issue"], 14);
    }
    let v = out(&home, &["job-attempts", "--issue", "14"]);
    assert_eq!(v["given"], 4);
    assert_eq!(v["lost"], 0);
    assert_eq!(v["open"], true);
    assert_eq!(v["last_given_at"], NOW);
    // An issue that was never given out has spent nothing.
    let v = out(&home, &["job-attempts", "--issue", "15"]);
    assert_eq!(v["given"], 0);
    assert_eq!(v["lost"], 0);
    assert_eq!(v["open"], false);
    assert_eq!(v["last_given_at"], Value::Null);
}

#[test]
fn the_retry_cap_is_reachable_and_reads_back_after_the_sweep() {
    // The number of lost jobs the instance allows one issue is its setting, so
    // what the store owes a later part is the count that word is compared
    // against, kept apart from the cycles' own attempt numbers and not lost
    // with the rows of the jobs themselves.
    let home = on("cap");
    issue(&home, 16);
    issue(&home, 17);
    // Two machines: the quiet job of one issue and the taken job of the other
    // are held at once, and a contractor holds one job.
    add(&home, "box1");
    add(&home, "box2");
    for round in 1..=3 {
        let at = NOW + 3600 * round;
        // The round is one quiet job of issue 17, and the job of issue 16 that
        // is taken up and finished on a machine of its own: the count read back
        // below cannot be one issue's seen twice, and only the quiet job is lost.
        let v = out_at(
            &home,
            at,
            &["job-offer", "--issue", "17", "--contractor", "box1"],
        );
        let id = v["job"]
            .as_i64()
            .unwrap_or_else(|| panic!("round {round}: {v}"));
        out_at(&home, at + 1, &["job-check-in", "--job", &id.to_string()]);
        let other = out_at(
            &home,
            at,
            &["job-offer", "--issue", "16", "--contractor", "box2"],
        );
        let other_id = other["job"]
            .as_i64()
            .unwrap_or_else(|| panic!("round {round}: {other}"));
        out_at(
            &home,
            at + 1,
            &["job-check-in", "--job", &other_id.to_string()],
        );
        out_at(
            &home,
            at + 300,
            &[
                "job-finish",
                "--job",
                &other_id.to_string(),
                "--sha256",
                CODE,
            ],
        );
        let swept = out_at(&home, at + 601, &["jobs-sweep", "--timeout", "600"]);
        assert_eq!(swept["lost"], 1, "round {round} of {swept}");
    }
    // Three jobs given for it and none lost: each was taken up and finished, so
    // no row of it is open now.
    let v = out(&home, &["job-attempts", "--issue", "16"]);
    assert_eq!(v["given"], 3);
    assert_eq!(v["lost"], 0);
    assert_eq!(v["open"], false);
    let v = out(&home, &["job-attempts", "--issue", "17"]);
    assert_eq!(v["given"], 3);
    assert_eq!(v["lost"], 3);
    assert_eq!(v["open"], false);
    // The record is the issue's own table: nothing of the jobs is needed to
    // read it back.
    let (given, lost): (i64, i64) = conn(&home)
        .query_row(
            "SELECT given, lost FROM issue_attempts WHERE issue = 17",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((given, lost), (3, 3));
    // An issue that never went out reads as nothing.
    assert_eq!(out(&home, &["job-attempts", "--issue", "18"])["given"], 0);
}

#[test]
fn a_loss_starts_the_rest_period_the_call_named() {
    let home = on("rest");
    issue(&home, 18);
    let id = offered(&home, 18, "box1")["job"].as_i64().unwrap();
    out(&home, &["job-check-in", "--job", &id.to_string()]);
    let v = out(
        &home,
        &[
            "job-lose",
            "--job",
            &id.to_string(),
            "--reason",
            "timeout",
            "--rest",
            "1800",
        ],
    );
    assert_eq!(v["verb"], "lost");
    assert_eq!(v["said"], "counted");
    let v = first_row(&home, "contractor-list").clone();
    assert_eq!(v["jobs_lost"], 1);
    assert_eq!(v["resting"], true);
    assert_eq!(v["resting_until"], NOW + 1800);
    // A loss of a job lost already moves nothing again.
    let v = out(
        &home,
        &[
            "job-lose",
            "--job",
            &id.to_string(),
            "--reason",
            "revoked",
            "--rest",
            "0",
        ],
    );
    assert_eq!(v["said"], "again");
    assert_eq!(first_row(&home, "contractor-list")["jobs_lost"], 1);
    assert_eq!(out(&home, &["job-attempts", "--issue", "18"])["lost"], 1);
}

#[test]
fn the_sweep_sorts_out_everything_that_died_while_the_factory_was_down() {
    // The one call a restart runs: an offered job nobody took, a running job
    // gone quiet, a stopping job gone quiet, and the codes nobody joined with.
    let home = on("restart");
    for n in 20..=22 {
        issue(&home, n);
    }
    // Three jobs that died while the factory was down, in the three states a
    // job dies in, on the three machines that held them.
    let offered_id = offered(&home, 20, "box1")["job"].as_i64().unwrap();
    let running_id = offered(&home, 21, "box2")["job"].as_i64().unwrap();
    let stopping_id = offered(&home, 22, "box3")["job"].as_i64().unwrap();
    for id in [running_id, stopping_id] {
        out(&home, &["job-check-in", "--job", &id.to_string()]);
    }
    out(&home, &["job-stop", "--job", &stopping_id.to_string()]);
    out(
        &home,
        &[
            "invite-new",
            "--name",
            "box3",
            "--code",
            CODE,
            "--ttl",
            "60",
        ],
    );
    let v = out_at(
        &home,
        NOW + 3600,
        &["jobs-sweep", "--timeout", "600", "--now"],
    );
    assert_eq!(v["lost"], 3);
    assert_eq!(v["codes"], 1);
    assert_eq!(
        rows(&home, "SELECT state FROM jobs ORDER BY id"),
        ["lost", "lost", "lost"]
    );
    assert_eq!(
        rows(&home, "SELECT lost_reason FROM jobs ORDER BY id"),
        ["expired", "timeout", "timeout"]
    );
    // Nothing is left open, so a second sweep says the same with nothing to do.
    let v = out_at(
        &home,
        NOW + 3601,
        &["jobs-sweep", "--timeout", "600", "--now"],
    );
    assert_eq!(v["lost"], 0);
    assert_eq!(v["revoked"], 0);
    assert_eq!(v["codes"], 0);
    // The history of one of them is the whole of its life, in order.
    assert_eq!(
        objects(
            &out(&home, &["job-show", "--job", &offered_id.to_string()]),
            "history"
        )
        .len(),
        2
    );
}

// --- the arguments -----------------------------------------------------------

#[test]
fn a_value_outside_its_grammar_is_64_and_writes_nothing() {
    let home = on("usage");
    let bad: Vec<Vec<&str>> = vec![
        vec!["contractor-add", "--name", "box1"],
        vec!["contractor-add", "--name", "", "--key", KEY],
        vec!["contractor-add", "--name", "../etc", "--key", KEY],
        vec!["contractor-add", "--name", "box1", "--key", "nothex"],
        vec!["contractor-add", "--name", "box1", "--key", &KEY[..63]],
        vec![
            "contractor-add",
            "--name",
            "box1",
            "--key",
            KEY,
            "--what",
            "x",
        ],
        vec!["invite-new", "--name", "box1", "--code", "zz"],
        vec!["invite-new", "--name", "box1", "--code", CODE, "--ttl", "0"],
        vec!["job-offer", "--issue", "0", "--contractor", "box1"],
        vec!["job-offer", "--issue", "x", "--contractor", "box1"],
        vec![
            "job-offer",
            "--issue",
            "1",
            "--contractor",
            "box1",
            "--expires",
            "-5",
        ],
        vec!["job-check-in"],
        vec!["job-check-in", "--job", "0"],
        vec!["job-finish", "--job", "1", "--sha256", "nope"],
        vec!["job-lose", "--job", "1", "--reason", "gone"],
        vec![
            "job-lose", "--job", "1", "--reason", "timeout", "--rest", "-1",
        ],
        vec!["job-show"],
        vec!["job-show", "--job", "1", "--issue", "1"],
        vec!["jobs-sweep"],
        vec!["jobs-sweep", "--timeout", "0"],
        vec!["job-attempts"],
    ];
    for args in bad {
        assert!(usage(&args).starts_with("usage "), "{}", args.join(" "));
    }
    // None of them reached the store.
    assert_eq!(held(&home, "contractors"), 0);
    assert_eq!(held(&home, "jobs"), 0);
    assert_eq!(held(&home, "join_codes"), 0);
}

#[test]
fn a_long_count_and_a_name_of_the_wrong_shape_are_refused() {
    // The grammars the store holds itself, so a caller that loaded nothing
    // cannot widen them.
    assert!(crate::verbs::parse(&strings(&[
        "contractor-rest",
        "--name",
        "box1",
        "--for",
        "99999999999999"
    ]))
    .unwrap()
    .is_err());
    assert!(crate::verbs::parse(&strings(&[
        "contractor-add",
        "--name",
        &"a".repeat(65),
        "--key",
        KEY
    ]))
    .unwrap()
    .is_err());
    assert!(crate::verbs::parse(&strings(&[
        "contractor-add",
        "--name",
        "box-1_a.host",
        "--key",
        KEY
    ]))
    .unwrap()
    .is_ok());
    assert!(crate::verbs::parse(&strings(&[
        "contractor-add",
        "--name",
        "box 1",
        "--key",
        KEY
    ]))
    .unwrap()
    .is_err());
}

/// A store whose tables are there but whose rows must never be moved by a
/// second writer: the record verbs of another module and these share the file.
#[test]
fn a_job_refers_to_an_issue_the_store_holds_and_the_row_is_the_record() {
    let home = on("record");
    issue(&home, 24);
    let v = offered(&home, 24, "box1");
    let id = v["job"].as_i64().unwrap();
    let c = conn(&home);
    let (issue, attempt, state): (i64, i64, String) = c
        .query_row(
            "SELECT issue, attempt, state FROM jobs WHERE id = ?1",
            rusqlite::params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((issue, attempt, state.as_str()), (24, 1, "offered"));
    // The state column holds one of the six words and no other.
    let sql: String = c
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'jobs'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    for state in crate::outsource::STATES {
        assert!(sql.contains(state), "{state} is not in the CHECK");
    }
    assert!(sql.contains("lost_reason"), "{sql}");
}
