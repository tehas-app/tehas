//! The sessions snapshot across the crates: this program writes it with the
//! command the host runs, and the MCP server's reader reads that same file.
//! Every field of the contract in `tehas_core::snapshot` is emitted here and
//! copied there, with the type the contract names, so a renamed or new field
//! fails this test instead of vanishing from every answer. The reader checks
//! the header's api itself, so the header written here is one it accepts.

use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const ANALYTICS: &str = env!("CARGO_BIN_EXE_analytics");

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    for sub in ["state/cycles", "logs", "docs"] {
        fs::create_dir_all(dir.join(sub)).unwrap();
    }
    dir
}

// A log's stamp: the time a cycle puts in the name of the log it opens.
fn stamp(t: i64) -> String {
    let out = Command::new("date")
        .args(["-u", "-d", &format!("@{t}"), "+%y%m%d-%H%M%S"])
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

// The JSON type of a value, spelled as the contract spells a shape.
fn json_type(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::String(_) => "string",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::Array(_) | Value::Object(_) => "composite",
    }
}

// A home with one run that has every column of a session, its step, its role,
// the build time of its image and one log: as much of the contract as a row
// can hold. The database is made by the program itself, then filled here.
fn home_with_a_run(name: &str) -> (PathBuf, String) {
    let home = scratch(name);
    let now = tehas_core::time::now();
    let started = now - 3600;
    let id = format!("work-7-{started}");
    let run = Command::new(ANALYTICS)
        .args(["event", "7", "started"])
        .env("TEHAS_HOME", &home)
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(0), "{run:?}");

    let conn = rusqlite::Connection::open(home.join("state/analytics.db")).unwrap();
    conn.execute(
        &format!(
            "INSERT INTO runs (id, kind, issue, attempt, how, started, ended, seconds,
                queue_wait, outcome, status, exit_code, last_step, failed_step, cap,
                park_reason, review_held, failure_kind, failure_cause, provider_outage,
                endpoint, pr, tehas_version, tehas_commit, rukas_version, rukas_commit,
                image, input, output, cache_read, cache_write, turns, dispatches, cost,
                runner_ms, oom_kills, forks_refused, concurrent, result_schema)
             VALUES ('{id}', 'work', 7, 2, 'label', {started}, {started} + 600, 600,
                5, 'failed', 'failed', 3, 'review', 'review', 'time', 'loop', 1,
                'model', 'the model said no', 0, 'main', 8, 'v0.7.31', 'c1', 'v0.14.0',
                'c2', 'sha256:ab', 1, 2, 3, 4, 9, 6, 0.5, 700, 0, 1, 2, 4)"
        ),
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO meta (key, value) VALUES ('image:sha256:ab', ?1)",
        rusqlite::params![started - 1000],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO run_steps (run_id, step, round, started, ended, seconds) VALUES (?1, 'build', 1, ?2, ?2 + 60, 60)",
        rusqlite::params![id, started],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO run_roles (run_id, role, model, provider, dispatches, turns, input, output, cache_read, cache_write, cost, ms) VALUES (?1, 'developer', 'm', 'p', 6, 9, 1, 2, 3, 4, 0.5, 700)",
        rusqlite::params![id],
    )
    .unwrap();
    fs::write(
        home.join("logs")
            .join(format!("work-7-{}.jsonl", stamp(started))),
        "abc",
    )
    .unwrap();
    fs::create_dir_all(home.join("live")).unwrap();
    (home, id)
}

// The snapshot of that home, as `tehasbot status` asks for it.
fn write_snapshot(home: &PathBuf) {
    let out = Command::new(ANALYTICS)
        .args(["sessions", "--out", home.join("mcp").to_str().unwrap()])
        .env("TEHAS_HOME", home)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(home.join("mcp/sessions.jsonl").is_file());
}

fn answer(dirs: &mcp::tools::Dirs, name: &str, args: Value) -> Value {
    match mcp::tools::call(dirs, name, &args).unwrap() {
        mcp::tools::Output::Json(v) => v,
        other => panic!("{name} gave {other:?}"),
    }
}

#[test]
fn the_mcp_reader_copies_every_field_the_sessions_writer_emits() {
    let (home, id) = home_with_a_run("sessions-contract");
    write_snapshot(&home);
    let dirs = mcp::tools::Dirs {
        live: home.join("live"),
        sessions: home.join("mcp"),
        logs: home.join("logs"),
    };

    let session = answer(&dirs, "factory_session", json!({"id": id}))["session"].clone();
    for (key, shape, _) in tehas_core::snapshot::RUN_FIELDS {
        assert_eq!(json_type(&session[key]), shape.json_type(), "{key}");
    }
    for (fields, rows) in [
        (
            tehas_core::snapshot::STEP_FIELDS.as_slice(),
            &session["steps"],
        ),
        (
            tehas_core::snapshot::ROLE_FIELDS.as_slice(),
            &session["roles"],
        ),
    ] {
        let row = &rows[0];
        for &(key, shape, _) in fields {
            assert_eq!(json_type(&row[key]), shape.json_type(), "{key}");
        }
    }
    // A log's file name is not what the reader answers with.
    assert_eq!(session["logs"], json!([{"file": "transcript", "bytes": 3}]));

    // A summary names no field the writer did not emit, beyond the link the
    // tool puts in from outside the record.
    let listed = answer(&dirs, "factory_sessions", json!({"limit": 1}))["sessions"][0].clone();
    for key in listed.as_object().unwrap().keys() {
        assert!(
            tehas_core::snapshot::RUN_FIELDS.iter().any(|f| f.0 == *key)
                || key == "issueUrl"
                || key == "prUrl",
            "{key}"
        );
    }
}
