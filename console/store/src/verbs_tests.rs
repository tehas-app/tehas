//! What the record verbs share, and the helpers the other *_tests files use:
//! a store per test, a call made the way main() makes it, a held cycle lock.

use crate::db;
use crate::verbs::{self, shown};
use crate::{execute, Settings, Verb};
use rusqlite::Connection;
use serde_json::Value;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::time::Duration;

pub const NOW: i64 = 1_792_000_000;
pub const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
pub const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
pub const IMAGE: &str = "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

/// This process's directory under one root in the temp dir. The first call
/// removes the directories of test processes that have ended, so scratch homes
/// do not pile up.
pub fn scratch_root() -> PathBuf {
    static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        let root = std::env::temp_dir().join("tehas-store-tests");
        if let Ok(entries) = fs::read_dir(&root) {
            for e in entries.flatten() {
                let pid = e.file_name().to_string_lossy().into_owned();
                if pid.parse::<u32>().is_ok() && !Path::new("/proc").join(&pid).exists() {
                    let _ = fs::remove_dir_all(e.path());
                }
            }
        }
        root.join(std::process::id().to_string())
    })
    .clone()
}

pub fn scratch(name: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = scratch_root().join(format!("verbs-{}-{name}", N.fetch_add(1, Ordering::SeqCst)));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A home with an initialised store.
pub fn home(name: &str) -> PathBuf {
    let home = scratch(name);
    execute(
        &home,
        &Verb::Init {
            force: false,
            settings: Settings {
                version: "v0.10.2".to_string(),
                commit: "cb6912a".to_string(),
                instance: "tehas-tehas".to_string(),
                repo: "example-owner/example-product".to_string(),
            },
        },
        NOW,
    )
    .unwrap();
    home
}

pub fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| s.to_string()).collect()
}

/// A call as main() makes it: a usage error is 64 with its line.
pub fn call_at(home: &Path, now: i64, args: &[&str]) -> Result<Value, (u8, String)> {
    match verbs::parse(&strings(args)) {
        Some(Ok(c)) => execute(home, &Verb::Record(c), now).map(|line| {
            assert_eq!(line.lines().count(), 1, "{line}");
            let v: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(v["api"], 1);
            assert_eq!(v["verb"], args[0]);
            assert_eq!(v["ok"], true);
            v
        }),
        Some(Err(why)) => Err((64, why)),
        None => panic!("not a record verb: {args:?}"),
    }
}

pub fn call(home: &Path, args: &[&str]) -> Result<Value, (u8, String)> {
    call_at(home, NOW, args)
}

pub fn ok(home: &Path, args: &[&str]) -> Value {
    call(home, args).unwrap_or_else(|e| panic!("{args:?}: {e:?}"))
}

pub fn ok_at(home: &Path, now: i64, args: &[&str]) -> Value {
    call_at(home, now, args).unwrap_or_else(|e| panic!("{args:?}: {e:?}"))
}

/// The exit and the first word of the stderr line.
pub fn refused(home: &Path, args: &[&str]) -> (u8, String) {
    let (code, line) = call(home, args).expect_err("the call was refused");
    let word = line.split([' ', ':']).next().unwrap_or("").to_string();
    (code, word)
}

/// The 64 line of a call that stops at its arguments.
pub fn usage(args: &[&str]) -> String {
    match verbs::parse(&strings(args)) {
        Some(Err(why)) => why,
        other => panic!("{args:?} was not refused: {other:?}"),
    }
}

pub fn conn(home: &Path) -> Connection {
    Connection::open(db::path(home)).unwrap()
}

pub fn one<T: rusqlite::types::FromSql>(home: &Path, sql: &str) -> T {
    conn(home).query_row(sql, [], |r| r.get(0)).unwrap()
}

pub fn count(home: &Path, sql: &str) -> i64 {
    one(home, &format!("SELECT count(*) FROM {sql}"))
}

/// The cycle lock of an issue, held by this process until the file is dropped.
pub fn hold(home: &Path, issue: i64) -> File {
    let dir = home.join("state").join("cycles");
    fs::create_dir_all(&dir).unwrap();
    let f = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(format!("{issue}.lock")))
        .unwrap();
    f.lock().unwrap();
    f
}

/// A unique `--at` for each approval a test records.
pub fn next_at() -> i64 {
    static AT: AtomicI64 = AtomicI64::new(1_700_000_000);
    AT.fetch_add(1, Ordering::SeqCst)
}

pub fn approve(home: &Path, issue: i64, by: &str, how: &str) -> i64 {
    let at = next_at();
    let n = issue.to_string();
    let at_s = at.to_string();
    let mut args = vec![
        "approval-record",
        &n,
        "--at",
        &at_s,
        "--by",
        by,
        "--how",
        how,
        "--body-sha",
        SHA,
    ];
    if how == "label" || how == "fixer" {
        args.extend(["--label", "tehas:ready"]);
    }
    if how == "refresh" {
        args.extend([
            "--refresh-pr",
            "40",
            "--refresh-head",
            "0123456789abcdef0123456789abcdef01234567",
        ]);
    }
    ok(home, &args);
    at
}

/// A started cycle: an approval by `by`, the lock held, cycle-start.
pub fn start_by(home: &Path, issue: i64, by: &str, how: &str, now: i64) -> (File, i64) {
    approve(home, issue, by, how);
    let lock = hold(home, issue);
    let v = ok_at(home, now, &["cycle-start", &issue.to_string()]);
    (lock, v["attempt"].as_i64().unwrap())
}

pub fn start(home: &Path, issue: i64) -> (File, i64) {
    start_by(home, issue, "alice", "label", NOW)
}

/// Ends the cycle of `issue` with `status`.
pub fn end(home: &Path, issue: i64, attempt: i64, status: &str) -> Value {
    ok(
        home,
        &[
            "cycle-end",
            &issue.to_string(),
            "--attempt",
            &attempt.to_string(),
            "--status",
            status,
        ],
    )
}

/// A second connection that holds the write lock until dropped.
pub fn writer_lock(home: &Path) -> Connection {
    let c = Connection::open(db::path(home)).unwrap();
    c.pragma_update(None, "busy_timeout", 0).unwrap();
    c.execute_batch("BEGIN IMMEDIATE; INSERT INTO meta (key, value) VALUES ('held', '1');")
        .unwrap();
    c
}

/// A connection that keeps every other connection out, readers included.
pub fn exclusive_lock(home: &Path) -> Connection {
    let c = Connection::open(db::path(home)).unwrap();
    c.execute_batch(
        "PRAGMA locking_mode = EXCLUSIVE; BEGIN EXCLUSIVE; INSERT INTO meta (key, value) VALUES ('held', '1');",
    )
    .unwrap();
    c
}

pub fn write_file(dir: &Path, name: &str, body: &[u8]) -> PathBuf {
    let p = dir.join(name);
    fs::write(&p, body).unwrap();
    p
}

/// Whether this process can be kept out of a directory by its mode. Root
/// cannot.
pub fn modes_bind() -> bool {
    use std::os::unix::fs::PermissionsExt;
    let d = scratch("mode-probe");
    fs::set_permissions(&d, fs::Permissions::from_mode(0o000)).unwrap();
    let bound = fs::read_dir(&d).is_err();
    fs::set_permissions(&d, fs::Permissions::from_mode(0o700)).unwrap();
    bound
}

// --- grammars and arguments ----------------------------------------------------

#[test]
fn grammars_hold_their_edges() {
    assert_eq!(verbs::number("1"), Some(1));
    assert_eq!(verbs::number("9999999999"), Some(9_999_999_999));
    for bad in ["0", "01", "+1", "-1", "", "1 ", "99999999999", "1e3"] {
        assert_eq!(verbs::number(bad), None, "{bad}");
    }
    assert_eq!(verbs::count("0"), Some(0));
    assert_eq!(verbs::count("999999999"), Some(999_999_999));
    assert_eq!(verbs::count("1000000000"), None);
    assert_eq!(verbs::count("00"), None);
    assert_eq!(verbs::unix("99999999999"), Some(99_999_999_999));
    assert_eq!(verbs::unix("0"), None);
    assert_eq!(
        verbs::comment_id("9007199254740991"),
        Some(9_007_199_254_740_991)
    );
    assert_eq!(verbs::comment_id("9007199254740992"), None);
    assert!(verbs::account(&"a".repeat(39)).is_some());
    for bad in ["", "a_b", "a b", &"a".repeat(40)] {
        assert!(verbs::account(bad).is_none(), "{bad}");
    }
    assert!(verbs::sha256(SHA).is_some());
    assert!(verbs::sha256(&SHA.to_uppercase()).is_none());
    assert!(verbs::commit("abcdef0").is_some());
    assert!(verbs::commit("abcdef").is_none());
    assert!(verbs::head_sha(&"a".repeat(40)).is_some());
    assert!(verbs::head_sha(&"a".repeat(39)).is_none());
    assert!(verbs::image(IMAGE).is_some());
    assert!(verbs::image(SHA).is_none());
}

#[test]
fn every_64_names_the_flag_and_an_enumerated_word() {
    let long = "x".repeat(200);
    for (args, want) in [
        (&["cycle-start"][..], "usage cycle-start: <issue> is required"),
        (
            &["cycle-start", "0"][..],
            "usage cycle-start: <issue> is not a number: \"0\"",
        ),
        (&["cycle-start", "1", "2"][..], "usage cycle-start: \"2\" unexpected"),
        (
            &["cycle-start", "1", "--bogus"][..],
            "usage cycle-start: \"--bogus\" unknown flag",
        ),
        (
            &["cycle-start", "1", "--rukas"][..],
            "usage cycle-start: --rukas needs a value",
        ),
        (
            &["cycle-start", "1", "--by-hand", "--by-hand"][..],
            "usage cycle-start: --by-hand given twice",
        ),
        (
            &["cycle-show", "1", "--json", "--json"][..],
            "usage cycle-show: --json given twice",
        ),
        (
            &[
                "approval-record", "1", "--at", "5", "--by", "a", "--how", "bogus", "--body-sha", SHA,
            ][..],
            "usage approval-record: --how is not one of label, refresh, retry-tick, outage-retry, fixer: \"bogus\"",
        ),
        (
            &[
                "approval-record", "1", "--at", "5", "--by", "a", "--how", "label", "--label",
                "Bad Word", "--body-sha", SHA,
            ][..],
            "usage approval-record: --label is not a tehas:* label: \"Bad Word\"",
        ),
        (
            &["cycle-note", "1", "--kind", "loud", "--said-for", "x"][..],
            "usage cycle-note: --kind is not one of quiet: \"loud\"",
        ),
        (
            &["cycle-end", "1", "--attempt", "1", "--status", "x", "--host-saw", "seen"][..],
            "usage cycle-end: --host-saw is not one of killed: \"seen\"",
        ),
        (
            &["plan-end", "1", "--turn", "1", "--outcome", "done"][..],
            "usage plan-end: --outcome is not one of finished, failed, interrupted, not-started, not-readable, lost: \"done\"",
        ),
        (
            &["plan-begin", "1", "--by", "a", "--label", "tehas:ready", "--brief-file", "/b"][..],
            "usage plan-begin: --label is not one of tehas:plan, tehas:refine: \"tehas:ready\"",
        ),
        (
            &["question-asked", "1", "--id", "5", "--kind", "both"][..],
            "usage question-asked: --kind is not one of park, plan: \"both\"",
        ),
    ] {
        assert_eq!(usage(args), want, "{args:?}");
    }
    // A long word is cut to 64 bytes of its escaped form.
    let line = usage(&["cycle-note", "1", "--kind", &long, "--said-for", "x"]);
    assert!(line.ends_with(&shown(&long)), "{line}");
    assert_eq!(shown(&long).len(), 64);
    let line = usage(&["cycle-start", "1", "--\u{7}bell"]);
    assert!(line.contains("\\u{7}"), "{line}");
}

#[test]
fn the_main_parse_takes_record_verbs_and_refuses_their_usage_errors() {
    assert!(matches!(
        crate::parse(&strings(&["cycle-start", "12"])),
        Some(Verb::Record(_))
    ));
    assert_eq!(crate::parse(&strings(&["cycle-start"])), None);
    assert_eq!(crate::parse(&strings(&["cycle-heartbeat", "12"])), None);
    assert!(verbs::parse(&strings(&["check"])).is_none());
    // A usage error opens nothing: no store is made where there was none.
    let empty = scratch("usage-opens-nothing");
    assert!(verbs::parse(&strings(&["park", "1"])).unwrap().is_err());
    assert!(!empty.join("state").exists());
}

#[test]
fn a_flag_value_that_starts_with_a_dash_is_taken_as_given() {
    let home = home("dash");
    let (_lock, attempt) = start(&home, 3);
    let a = attempt.to_string();
    let v = ok(
        &home,
        &[
            "cycle-end",
            "3",
            "--attempt",
            &a,
            "--status",
            "closed",
            "--step",
            "-",
        ],
    );
    assert_eq!(v["cycle"]["end_reason"], "closed");
    assert_eq!(one::<String>(&home, "SELECT step FROM cycle_history"), "-");
    assert_eq!(v["cleaned"], serde_json::json!([]));
    // A value flag takes the next argument even when it looks like a flag.
    assert_eq!(
        verbs::parse(&strings(&[
            "cycle-note",
            "3",
            "--kind",
            "quiet",
            "--said-for",
            "--json",
            "--json"
        ])),
        Some(Ok(verbs::Call::Cycle(crate::cycle::Call::Note {
            issue: 3,
            said_for: "--json".to_string()
        })))
    );
}

// --- the store -------------------------------------------------------------------

/// One valid call of every verb, against issue 1. `brief` is a brief file,
/// also taken as an answer's text, and `input` an approval's inputs.
fn samples(brief: &str, input: &str) -> Vec<Vec<String>> {
    let raw: Vec<Vec<&str>> = vec![
        vec!["cycle-start", "1"],
        vec!["cycle-image", "1", "--attempt", "1", "--image", IMAGE],
        vec!["cycle-end", "1", "--attempt", "1", "--status", "closed"],
        vec!["cycle-end", "1", "--attempt", "1", "--refused"],
        vec!["cycle-end", "1", "--attempt", "1", "--stopped"],
        vec!["cycle-show", "1"],
        vec!["cycle-show", "--running"],
        vec![
            "cycle-note",
            "1",
            "--kind",
            "quiet",
            "--said-for",
            "1791999000",
        ],
        vec!["cycle-stop-asked", "1"],
        vec![
            "cycle-heartbeat",
            "1",
            "--running",
            "--status",
            "running",
            "--step",
            "build",
        ],
        vec!["publish"],
        vec!["park", "1", "--attempt", "1", "--reason", "failed"],
        vec!["park-show", "1"],
        vec!["park-show", "--automatic"],
        vec!["park-retry", "1"],
        vec!["park-told", "1"],
        vec!["question-asked", "1", "--id", "77", "--kind", "park"],
        vec!["question-known", "1", "--id", "77"],
        vec!["question-answered", "1", "--id", "77", "--by", "alice"],
        vec![
            "approval-record",
            "1",
            "--at",
            "5",
            "--by",
            "alice",
            "--how",
            "label",
            "--label",
            "tehas:ready",
            "--body-sha",
            SHA,
        ],
        vec![
            "approval-record",
            "1",
            "--at",
            "6",
            "--by",
            "alice",
            "--how",
            "retry-tick",
            "--body-sha",
            SHA,
            "--input-file",
            input,
        ],
        vec!["approval-record", "1", "--undo", "--at", "5"],
        vec!["approval-current", "1"],
        vec![
            "plan-begin",
            "1",
            "--by",
            "alice",
            "--label",
            "tehas:plan",
            "--brief-file",
            brief,
        ],
        vec!["plan-begin", "1", "--undo"],
        vec!["plan-answer", "1", "--turn", "2", "--by", "alice"],
        vec![
            "plan-answer",
            "1",
            "--turn",
            "2",
            "--by",
            "alice",
            "--text-file",
            brief,
        ],
        vec!["plan-answer", "1", "--turn", "2", "--undo"],
        vec!["plan-end", "1", "--turn", "1", "--outcome", "interrupted"],
        vec!["plan-end", "1", "--posted"],
        vec!["plan-end", "1", "--close"],
        vec!["plan-show", "1"],
        vec!["plan-show", "--open"],
        vec!["plan-note", "1", "--kind", "held", "--id", "5"],
        vec!["history-show"],
        vec!["backup"],
        vec!["backup", "--show"],
        vec!["prune", "--days", "90"],
        vec!["sweep-locks"],
        vec!["upkeep"],
        vec!["history-show", "1", "--kind", "cycle", "--how", "label"],
    ];
    raw.into_iter().map(|v| strings(&v)).collect()
}

const READERS: [&str; 9] = [
    "cycle-show",
    "backup",
    "sweep-locks",
    "publish",
    "park-show",
    "question-known",
    "approval-current",
    "plan-show",
    "history-show",
];

fn run_strings(home: &Path, args: &[String]) -> Result<String, (u8, String)> {
    match verbs::parse(args) {
        Some(Ok(c)) => execute(home, &Verb::Record(c), NOW),
        other => panic!("{args:?}: {other:?}"),
    }
}

#[test]
fn every_verb_says_78_and_its_word_when_the_store_cannot_be_used() {
    let files = scratch("briefs");
    let brief = write_file(&files, "brief.md", b"the brief");
    let brief = brief.to_str().unwrap();
    let input = write_file(&files, "input.json", b"{}");
    let input = input.to_str().unwrap();
    let missing = scratch("78-missing");
    let corrupt = scratch("78-corrupt");
    fs::create_dir_all(corrupt.join("state")).unwrap();
    fs::write(db::path(&corrupt), vec![b'x'; 8192]).unwrap();
    // A file a touch made, never migrated.
    let touched = scratch("78-touched");
    fs::create_dir_all(touched.join("state")).unwrap();
    fs::write(db::path(&touched), b"").unwrap();
    // An SQLite file with no schema.
    let bare = scratch("78-bare");
    fs::create_dir_all(bare.join("state")).unwrap();
    {
        let c = Connection::open(db::path(&bare)).unwrap();
        c.execute_batch("PRAGMA journal_mode = WAL; CREATE TABLE t (a); DROP TABLE t;")
            .unwrap();
    }
    let future = home("78-future");
    conn(&future)
        .pragma_update(None, "user_version", db::SCHEMA_VERSION + 1)
        .unwrap();
    for (dir, word) in [
        (&missing, "missing"),
        (&corrupt, "corrupt"),
        (&touched, "unreadable"),
        (&bare, "unreadable"),
        (&future, "future"),
    ] {
        for args in samples(brief, input) {
            let (code, line) = run_strings(dir, &args).expect_err("refused");
            assert_eq!(code, 78, "{args:?} {line}");
            assert!(line.starts_with(&format!("{word} ")), "{args:?}: {line}");
        }
    }
    for dir in [&touched, &bare] {
        let (_, line) = run_strings(dir, &strings(&["plan-show", "1"])).unwrap_err();
        assert!(
            line.contains("the store holds no schema; run store migrate"),
            "{line}"
        );
    }
    assert!(!db::path(&missing).exists());
    assert_eq!(fs::metadata(db::path(&touched)).unwrap().len(), 0);
}

#[test]
fn every_writer_says_busy_and_commits_nothing_while_the_lock_is_held() {
    let files = scratch("busy-briefs");
    let brief = write_file(&files, "brief.md", b"the brief");
    let brief = brief.to_str().unwrap().to_string();
    let input = write_file(&files, "input.json", b"{}");
    let input = input.to_str().unwrap().to_string();
    let handles: Vec<_> = samples(&brief, &input)
        .into_iter()
        .filter(|a| !READERS.contains(&a[0].as_str()))
        .map(|args| {
            std::thread::spawn(move || {
                let home = home("busy");
                let _lock = hold(&home, 1);
                let holder = writer_lock(&home);
                let (code, line) = run_strings(&home, &args).expect_err("busy");
                drop(holder);
                assert_eq!(code, 1, "{args:?}");
                assert!(line.starts_with("busy "), "{args:?}: {line}");
                assert_eq!(count(&home, "issues"), 0, "{args:?}");
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn every_reader_says_busy_when_it_cannot_look() {
    let handles: Vec<_> = samples("/unused", "/unused")
        .into_iter()
        .filter(|a| READERS.contains(&a[0].as_str()))
        .map(|args| {
            std::thread::spawn(move || {
                let home = home("busy-reader");
                let holder = exclusive_lock(&home);
                let (code, line) = run_strings(&home, &args).expect_err("busy");
                drop(holder);
                assert_eq!(code, 1, "{args:?}");
                assert!(line.starts_with("busy "), "{args:?}: {line}");
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn a_reader_waits_out_a_short_lock() {
    let home = home("short-lock");
    let holder = exclusive_lock(&home);
    let t = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        drop(holder);
    });
    let v = ok(&home, &["cycle-show", "1"]);
    t.join().unwrap();
    assert_eq!(v["lock"]["probe_rc"], 0);
    assert_eq!(v["cycle"], Value::Null);
}

#[test]
fn a_refused_verb_leaves_no_issues_row() {
    let home = home("no-stub");
    let _lock = hold(&home, 4);
    assert_eq!(
        refused(&home, &["cycle-start", "4"]),
        (1, "unapproved".into())
    );
    assert_eq!(
        refused(&home, &["park", "4", "--attempt", "1", "--reason", "x"]),
        (1, "nocycle".into())
    );
    assert_eq!(
        refused(
            &home,
            &["question-asked", "4", "--id", "9", "--kind", "plan"]
        ),
        (1, "noplan".into())
    );
    assert_eq!(count(&home, "issues"), 0);
    // `used` needs a used approval, so its issue is there before the call.
    let (_l, _) = start(&home, 5);
    let at = one::<i64>(&home, "SELECT at FROM approvals WHERE issue = 5").to_string();
    let before = count(&home, "issues");
    assert_eq!(
        refused(
            &home,
            &[
                "approval-record",
                "5",
                "--at",
                &at,
                "--by",
                "bob",
                "--how",
                "label",
                "--label",
                "tehas:ready",
                "--body-sha",
                SHA
            ]
        ),
        (1, "used".into())
    );
    assert_eq!(count(&home, "issues"), before);
}

#[test]
fn a_constraint_failure_is_exit_1_conflict_not_78() {
    let home = home("conflict");
    let (_lock, attempt) = start(&home, 6);
    conn(&home)
        .execute_batch(
            "CREATE TRIGGER no_parks BEFORE INSERT ON parks BEGIN SELECT RAISE(ABORT, 'no parks'); END;",
        )
        .unwrap();
    let a = attempt.to_string();
    let (code, line) = call(&home, &["park", "6", "--attempt", &a, "--reason", "x"]).unwrap_err();
    assert_eq!(code, 1);
    assert!(line.starts_with("conflict: "), "{line}");
    assert!(line.contains("no parks"), "{line}");
    assert_eq!(count(&home, "parks"), 0);
}

#[test]
fn files_are_read_before_the_write_lock() {
    let home = home("files-first");
    let (_lock, attempt) = start(&home, 7);
    let result = write_file(&home, "result.json", b"not json");
    let holder = writer_lock(&home);
    // While the verb waits for the lock the file becomes a good result. The
    // verb judged it before it asked for the lock, so the old bytes count.
    let path = result.clone();
    let t = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        fs::write(&path, br#"{"schemaVersion":3}"#).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        drop(holder);
    });
    let a = attempt.to_string();
    let v = ok(
        &home,
        &[
            "cycle-end",
            "7",
            "--attempt",
            &a,
            "--status",
            "closed",
            "--result",
            result.to_str().unwrap(),
        ],
    );
    t.join().unwrap();
    assert_eq!(v["result"], "unreadable");
    assert_eq!(count(&home, "cycle_results"), 0);
    // A brief that cannot be used is refused without waiting for the lock.
    let _holder = writer_lock(&home);
    let started = std::time::Instant::now();
    let missing = home.join("no-brief.md");
    assert_eq!(
        refused(
            &home,
            &[
                "plan-begin",
                "8",
                "--by",
                "a",
                "--label",
                "tehas:plan",
                "--brief-file",
                missing.to_str().unwrap()
            ]
        ),
        (1, "brief".into())
    );
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn a_file_argument_is_read_only_when_it_is_a_regular_file_within_its_bound() {
    use verbs::FileErr;
    let dir = scratch("read-file");
    let f = write_file(&dir, "f", b"12345");
    assert_eq!(verbs::read_file(&f, 5).unwrap().0, b"12345");
    assert_eq!(verbs::read_file(&f, 4), Err(FileErr::TooLarge));
    let link = dir.join("link");
    std::os::unix::fs::symlink(&f, &link).unwrap();
    assert_eq!(verbs::read_file(&link, 5), Err(FileErr::Refused));
    assert_eq!(verbs::read_file(&dir, 5), Err(FileErr::Refused));
    assert_eq!(
        verbs::read_file(&dir.join("none"), 5),
        Err(FileErr::Refused)
    );
}

#[test]
fn a_look_reads_one_snapshot() {
    use crate::verbs::Q;
    let home = home("look-snapshot");
    let issues = |c: &Connection| {
        c.query_row("SELECT count(*) FROM issues", [], |r| r.get::<_, i64>(0))
            .q()
    };
    let seen = verbs::look(&home, |c| {
        let before = issues(c)?;
        verbs::stub(&conn(&home), 1)?;
        Ok((before, issues(c)?))
    })
    .unwrap();
    assert_eq!(seen, (0, 0));
    assert_eq!(count(&home, "issues"), 1);
}
