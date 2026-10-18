//! The program against the headless runner's fixtures, in scratch homes.

use crate::db;
use crate::run::{self, iso_ms, Given};
use crate::{execute, parse, Cmd};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

const DAY: i64 = 86_400;

fn scratch(name: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "analytics-test-{}-{}-{name}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("state/cycles")).unwrap();
    fs::create_dir_all(dir.join("logs")).unwrap();
    fs::create_dir_all(dir.join("docs")).unwrap();
    dir
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/fixtures")
        .join(name)
}

fn secs(iso: &str) -> i64 {
    iso_ms(iso).unwrap() / 1000
}

fn iso(t: i64) -> String {
    let out = std::process::Command::new("date")
        .args(["-u", "-d", &format!("@{t}"), "+%FT%TZ"])
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

// An address written out here, so no private one stands in the source.
fn ip(octets: [u8; 4]) -> String {
    octets.map(|o| o.to_string()).join(".")
}

fn put(home: &Path, name: &str, text: &str) {
    let p = home.join(name);
    fs::write(&p, text).unwrap();
}

fn history(home: &Path, issue: i64, lines: &[Value]) {
    let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
    put(home, &format!("state/cycles/{issue}.history.jsonl"), &text);
}

fn record(home: &Path, issue: i64, now: i64) -> Connection {
    execute(home, &Cmd::Run(issue, Given::default()), now).unwrap();
    Connection::open(db::path(home)).unwrap()
}

fn one<T: rusqlite::types::FromSql>(conn: &Connection, sql: &str) -> T {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn steps(conn: &Connection) -> Vec<(String, i64, i64, i64, i64)> {
    let mut q = conn
        .prepare("SELECT step, round, started, ended, seconds FROM run_steps ORDER BY started, step")
        .unwrap();
    q.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

// A cycle of issue 7 from a fixture, as libexec/cycle and libexec/work leave
// it: the result, the progress summary, the settings, the approval, the
// image's commit and the version, and the history line `line`.
fn cycle(home: &Path, name: &str, started: i64, line: Value) {
    let dir = fixture(name);
    fs::copy(dir.join("result.json"), home.join("state/cycles/7.result.json")).unwrap();
    fs::copy(
        dir.join("progress.jsonl"),
        home.join("logs/work-7-261015-183834.jsonl"),
    )
    .unwrap();
    put(
        home,
        "logs/work-7-261015-183834.jsonl.preflight",
        "Everything up-to-date\nprovider retry: 3 tries\n",
    );
    put(
        home,
        "state/cycles/7.progress.json",
        r#"{"roles":[],"reason":"","failure":null,"reviewHeld":false,"from":{"roles":"progress","failure":"progress"}}"#,
    );
    put(
        home,
        "state/cycles/7.settings",
        &json!({"container": {"PI_ENSEMBLE_SUBAGENT_MODEL": "sub-model",
                              "PI_ENSEMBLE_SUBAGENT_PROVIDER": "sub-provider",
                              "PI_ENSEMBLE_MODEL_OPS": "ops-model",
                              "PI_ENSEMBLE_PROVIDER_OPS": "ops-provider"},
                "host": {"TEHAS_WORK_TIMEOUT": "21600"}, "concurrentCycles": 2})
        .to_string(),
    );
    put(
        home,
        "state/cycles/7.approved.json",
        &json!({"at": iso(started - 90), "by": "someone", "how": "label"}).to_string(),
    );
    put(home, "state/cycles/7.rukas", "abcdef1\n");
    put(home, "docs/version.txt", "v0.7.22+1234abc\n");
    history(home, 7, &[line]);
}

#[test]
fn each_fixture_result_is_stored_with_what_the_outcome_drops() {
    // started and the end of the last step of each fixture, the runner's
    // durationMs, and the status, cap and exit tehas wrote: for schema 3
    // lib/headless.sh reads no outcome, so the history says unknown.
    let cases = [
        ("headless-0.12.80", "2026-10-15T18:38:44Z", 64974, 3, 61, "unknown"),
        ("headless-1ca933e", "2026-10-15T20:59:05Z", 64061, 11, 52, "unknown"),
        ("headless-316134e", "2026-10-18T10:16:49Z", 8108, 8, 0, "handoff"),
    ];
    for (name, first, ms, explore, handoff, status) in cases {
        let home = scratch(name);
        let started = secs(first) - 10;
        let ended = started + 80;
        let line = if status == "unknown" {
            json!({"at": iso(ended), "started": started, "rc": 10, "status": "unknown",
                   "step": "-", "parked": true, "provider": false, "how": "label"})
        } else {
            json!({"at": iso(ended), "started": started, "rc": 10, "status": "handoff",
                   "step": "explore", "cap": "explore-bodies-empty", "failure_kind": "cap",
                   "parked": true, "provider": false, "how": "label"})
        };
        cycle(&home, name, started, line);
        let conn = record(&home, 7, ended + 60);
        let row: (String, String, i64, i64, i64, i64, i64, String, String, i64) = conn
            .query_row(
                "SELECT id, outcome, attempt, started, ended, seconds, queue_wait, status, cap,
                        exit_code FROM runs",
                [],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                        r.get(7)?,
                        r.get(8)?,
                        r.get(9)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            row,
            (
                format!("work-7-{started}"),
                "handoff".to_string(),
                1,
                started,
                ended,
                80,
                90,
                "handoff".to_string(),
                "explore-bodies-empty".to_string(),
                10
            ),
            "{name}"
        );
        let more: (i64, i64, i64, f64, i64, String, String, String) = conn
            .query_row(
                "SELECT runner_ms, cache_write, dispatches, cost, result_schema, last_step,
                        failed_step, endpoint FROM runs",
                [],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                        r.get(7)?,
                    ))
                },
            )
            .unwrap();
        let schema = if name == "headless-316134e" { 4 } else { 3 };
        let dispatches = if schema == 4 { 0 } else { 1 };
        assert_eq!(
            more,
            (
                ms,
                0,
                dispatches,
                0.0,
                schema,
                "handoff".into(),
                "explore".into(),
                "main".into()
            ),
            "{name}"
        );
        let versions: (String, String, String, String, i64) = conn
            .query_row(
                "SELECT tehas_version, tehas_commit, rukas_version, rukas_commit, concurrent FROM runs",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();
        let build: Value =
            serde_json::from_str(&fs::read_to_string(fixture(name).join("result.json")).unwrap())
                .unwrap();
        assert_eq!(
            versions,
            (
                "v0.7.22".into(),
                "1234abc".into(),
                "0.12.80".into(),
                build["buildCommit"].as_str().unwrap().to_string(),
                1
            ),
            "{name}"
        );
        let s = steps(&conn);
        let first = secs(first);
        assert_eq!(s.len(), 2, "{name}");
        assert_eq!(s[0], ("explore".into(), 1, first, first + explore, explore), "{name}");
        assert_eq!((s[1].0.as_str(), s[1].1, s[1].4), ("handoff", 1, handoff), "{name}");
        let roles: Vec<(String, String, String, i64)> = conn
            .prepare("SELECT role, model, provider, dispatches FROM run_roles")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        let want: Vec<(String, String, String, i64)> = if name == "headless-1ca933e" {
            vec![("ops".into(), "ops-model".into(), "ops-provider".into(), 1)]
        } else {
            vec![]
        };
        assert_eq!(roles, want, "{name}");
    }
}

#[test]
fn cache_write_cost_and_role_usage_are_kept() {
    let home = scratch("usage");
    let started = secs("2026-10-18T10:16:39Z");
    cycle(
        &home,
        "headless-316134e",
        started,
        json!({"at": iso(started + 30), "started": started, "rc": 10, "status": "handoff",
               "parked": true}),
    );
    let path = home.join("state/cycles/7.result.json");
    let mut r: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    r["issues"][0]["usage"] = json!({"input": 1000, "output": 200, "cacheRead": 5000,
        "cacheWrite": 300, "cost": 0.25, "turns": 9, "dispatches": 3,
        "byRole": {"code-review-specialist": {"input": 600, "output": 100, "cacheRead": 4000,
                   "cacheWrite": 200, "cost": 0.125, "turns": 5, "dispatches": 2},
                   "ops": {"input": 400, "output": 100, "cacheRead": 1000, "cacheWrite": 100,
                   "cost": 0.125, "turns": 4, "dispatches": 1}}});
    fs::write(&path, r.to_string()).unwrap();
    let conn = record(&home, 7, started + 60);
    let usage: (i64, i64, i64, i64, i64, i64, f64) = conn
        .query_row(
            "SELECT input, output, cache_read, cache_write, turns, dispatches, cost FROM runs",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(usage, (1000, 200, 5000, 300, 9, 3, 0.25));
    let roles: Vec<(String, String, i64, i64, f64)> = conn
        .prepare("SELECT role, model, input, cache_write, cost FROM run_roles ORDER BY role")
        .unwrap()
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    // A role without a model of its own has the subagent's.
    assert_eq!(
        roles,
        vec![
            ("code-review-specialist".into(), "sub-model".into(), 600, 200, 0.125),
            ("ops".into(), "ops-model".into(), 400, 100, 0.125),
        ]
    );
}

#[test]
fn roles_come_from_the_progress_and_steps_from_the_stream_when_the_result_has_none() {
    let home = scratch("fallback");
    let started = secs("2026-10-15T18:38:34Z");
    let ended = secs("2026-10-15T18:39:50Z");
    cycle(
        &home,
        "headless-0.12.80",
        started,
        json!({"at": iso(ended), "started": started, "rc": 10, "status": "unknown", "parked": true}),
    );
    let path = home.join("state/cycles/7.result.json");
    let mut r: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    r["issues"][0].as_object_mut().unwrap().remove("steps");
    fs::write(&path, r.to_string()).unwrap();
    put(
        &home,
        "state/cycles/7.progress.json",
        r#"{"roles":[{"role":"ops","dispatches":2,"turns":3,"input":10,"output":20,"cacheRead":30}],"reason":"","failure":null,"reviewHeld":true}"#,
    );
    let conn = record(&home, 7, ended);
    assert_eq!(one::<i64>(&conn, "SELECT review_held FROM runs"), 1);
    assert_eq!(
        one::<String>(&conn, "SELECT role || ' ' || model || ' ' || turns || ' ' || cache_read FROM run_roles"),
        "ops ops-model 3 30"
    );
    // The stream's step-started lines: explore at 18:38:44.024, handoff at
    // 18:38:47.386, which ends where the run did.
    let first = secs("2026-10-15T18:38:44Z");
    assert_eq!(
        steps(&conn),
        vec![
            ("explore".into(), 1, first, first + 3, 3),
            ("handoff".into(), 1, first + 3, ended, 62),
        ]
    );
}

#[test]
fn a_run_with_no_result_is_stored_with_tehas_s_status_and_times() {
    for (status, rc) in [("killed", 137), ("stalled", 2), ("timeout", 2)] {
        let home = scratch(status);
        let started = 1_792_000_000;
        let cap = match status {
            "stalled" => "stall",
            "timeout" => "wall-clock",
            _ => "",
        };
        history(
            &home,
            12,
            &[json!({"at": iso(started + 3600), "started": started, "rc": rc, "status": status,
                     "step": "implement", "cap": cap, "parked": true, "provider": false})],
        );
        // What libexec/cycle passes on. The files it removed are not read.
        put(&home, "state/cycles/12.oom", "9\n");
        put(&home, "state/cycles/12.image", &format!("sha256:{} 1111111\n", "e".repeat(64)));
        let given = Given {
            image: Some(format!("sha256:{}", "a".repeat(64))),
            rukas_image: Some("abc1234".into()),
            oom: Some(4),
            forks: Some(2),
        };
        execute(&home, &Cmd::Run(12, given), started + 3700).unwrap();
        let conn = Connection::open(db::path(&home)).unwrap();
        assert_eq!(
            one::<String>(&conn, "SELECT image || ' ' || rukas_commit || ' ' || forks_refused FROM runs"),
            format!("sha256:{} abc1234 2", "a".repeat(64))
        );
        let row: (String, i64, i64, i64, i64, String, String, Option<String>, i64) = conn
            .query_row(
                "SELECT status, exit_code, started, ended, seconds, outcome, last_step, cap,
                        oom_kills FROM runs",
                [],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                        r.get(7)?,
                        r.get(8)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            row,
            (
                status.to_string(),
                rc,
                started,
                started + 3600,
                3600,
                "failed".to_string(),
                "implement".to_string(),
                (!cap.is_empty()).then(|| cap.to_string()),
                4
            )
        );
        assert_eq!(
            one::<Option<i64>>(&conn, "SELECT result_schema FROM runs"),
            None
        );
        assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM run_steps"), 0);
    }
}

#[test]
fn attempt_counts_earlier_runs_and_how_is_kept() {
    let home = scratch("attempt");
    let t = 1_792_000_000;
    let line = |started: i64, how: &str| {
        json!({"at": iso(started + 600), "started": started, "rc": 10, "status": "handoff",
               "parked": true, "how": how})
    };
    history(
        &home,
        9,
        &[line(t, "label"), line(t + 1000, "retry-tick"), line(t + 2000, "fixer")],
    );
    put(
        &home,
        "state/cycles/9.approved.json",
        &json!({"at": iso(t + 1900), "by": "a", "how": "fixer"}).to_string(),
    );
    let conn = record(&home, 9, t + 3000);
    assert_eq!(
        one::<String>(&conn, "SELECT attempt || ' ' || how || ' ' || queue_wait FROM runs"),
        "3 fixer 100"
    );
    // Recorded again, it replaces itself.
    execute(&home, &Cmd::Run(9, Given::default()), t + 3000).unwrap();
    assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM runs"), 1);
}

#[test]
fn outcomes_follow_how_the_cycle_ended() {
    let home = scratch("outcomes");
    let t = 1_792_000_000;
    let cases = [
        (json!({"status": "merged", "rc": 0, "pr": "https://github.com/o/r/pull/41"}), "merged", Some(41)),
        (json!({"status": "handoff", "rc": 10, "pr": "https://github.com/o/r/pull/42", "parked": false}), "pr", Some(42)),
        (json!({"status": "handoff", "rc": 10, "pr": "https://github.com/o/r/pull/43", "parked": true}), "handoff", Some(43)),
        (json!({"status": "closed", "rc": 10}), "stopped", None),
        (json!({"status": "unknown", "rc": 4, "parked": true}), "refused", None),
        (json!({"status": "aborted", "rc": 20, "cap": "deadline", "parked": true}), "failed", None),
        (json!({"status": "handoff", "rc": 10, "failure_kind": "provider", "provider": true, "parked": true}), "handoff", None),
        (json!({"status": "parked", "rc": 10, "cap": "intent-park", "failure_kind": "cap", "parked": true}), "parked", None),
    ];
    for (i, (fields, want, pr)) in cases.into_iter().enumerate() {
        let issue = 100 + i as i64;
        let mut line = json!({"at": iso(t + 60), "started": t});
        line.as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        history(&home, issue, &[line]);
        let conn = record(&home, issue, t + 100);
        let got: (String, Option<i64>) = conn
            .query_row(
                "SELECT outcome, pr FROM runs WHERE issue = ?1",
                [issue],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(got, (want.to_string(), pr), "case {i}");
    }
}

#[test]
fn a_failure_cause_is_masked_and_cut() {
    let home = scratch("cause");
    let started = secs("2026-10-18T10:16:39Z");
    cycle(
        &home,
        "headless-316134e",
        started,
        json!({"at": iso(started + 30), "started": started, "rc": 10, "status": "handoff",
               "parked": true}),
    );
    let path = home.join("state/cycles/7.result.json");
    let mut r: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    let long = "x".repeat(400);
    r["issues"][0]["failure"] = json!({"kind": "provider", "step": "implement",
        "cause": format!("connect {}:8080 refused token=ghp_abcdefghijklmnopqrstuvwxyz123456\n{long}", ip([192, 168, 1, 20]))});
    fs::write(&path, r.to_string()).unwrap();
    let conn = record(&home, 7, started + 60);
    let cause: String = one(&conn, "SELECT failure_cause FROM runs");
    assert!(cause.starts_with("connect [private address]:8080 refused token=***"), "{cause}");
    assert_eq!(cause.chars().count(), 200);
    assert_eq!(one::<String>(&conn, "SELECT failure_kind FROM runs"), "provider");
}

#[test]
fn the_history_s_cause_is_taken_before_the_result_s() {
    let home = scratch("historycause");
    let started = secs("2026-10-18T10:16:39Z");
    cycle(
        &home,
        "headless-316134e",
        started,
        json!({"at": iso(started + 30), "started": started, "rc": 10, "status": "handoff",
               "failure_kind": "provider", "failure_cause": "ECONNREFUSED"}),
    );
    let path = home.join("state/cycles/7.result.json");
    let mut r: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    r["issues"][0]["failure"] = json!({"kind": "provider", "cause": "a long raw text"});
    fs::write(&path, r.to_string()).unwrap();
    let conn = record(&home, 7, started + 60);
    assert_eq!(one::<String>(&conn, "SELECT failure_cause FROM runs"), "ECONNREFUSED");
}

#[test]
fn a_raw_cause_loses_its_hosts_addresses_and_keys() {
    let cases = [
        ("connect ECONNREFUSED models.example:8000", "connect ECONNREFUSED [host]"),
        ("git@host.tld:org/repo not found", "[address]:org/repo not found"),
        (
            "Incorrect API key provided: sk-abc123def456ghi789jkl0",
            "Incorrect API key provided: sk-***",
        ),
        ("slack xoxb-1234-abcdefgh said no", "slack xoxb-*** said no"),
        ("aws AKIAABCDEFGHIJKLMNOP denied", "aws AKIA*** denied"),
        (
            "jwt eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.c2lnbmF0dXJl expired",
            "jwt [jwt] expired",
        ),
        (
            "key -----BEGIN RSA PRIVATE KEY-----\nMIIBOgIBAAJB\n-----END RSA PRIVATE KEY----- leaked",
            "key [private key] leaked",
        ),
        ("dial [2001:db8::1]:443 failed", "dial [[address]]:443 failed"),
        ("GET https://api.example.com/v1 503", "GET https://[host]/v1 503"),
        ("lookup api.example.com: no such host", "lookup [host]: no such host"),
        // Not hosts: a version, a file, a time.
        (
            "tehas v0.7.22 could not read result.json at 18:38:44",
            "tehas v0.7.22 could not read result.json at 18:38:44",
        ),
    ];
    for (raw, want) in cases {
        assert_eq!(run::cause(raw), want, "{raw}");
    }
}

#[test]
fn a_backup_endpoint_is_read_from_the_preflight_log() {
    let home = scratch("backup");
    let started = secs("2026-10-18T10:16:39Z");
    cycle(
        &home,
        "headless-316134e",
        started,
        json!({"at": iso(started + 30), "started": started, "rc": 10, "status": "handoff"}),
    );
    put(
        &home,
        "logs/work-7-261015-183834.jsonl.preflight",
        "Everything up-to-date\nmodel endpoint: provider fleet is on its backup\n",
    );
    let conn = record(&home, 7, started + 60);
    assert_eq!(one::<String>(&conn, "SELECT endpoint FROM runs"), "backup");
}

#[test]
fn an_issue_without_history_records_nothing() {
    let home = scratch("nohistory");
    assert_eq!(execute(&home, &Cmd::Run(5, Given::default()), 1_792_000_000).unwrap_err().0, 65);
    assert!(!db::path(&home).exists());
    // A link is not read.
    let target = home.join("elsewhere.jsonl");
    fs::write(&target, "{\"started\": 1, \"at\": \"2026-10-18T10:16:39Z\"}\n").unwrap();
    std::os::unix::fs::symlink(&target, home.join("state/cycles/5.history.jsonl")).unwrap();
    assert_eq!(execute(&home, &Cmd::Run(5, Given::default()), 1_792_000_000).unwrap_err().0, 65);
}

#[test]
fn prune_removes_rows_older_than_ten_days_and_keeps_a_running_run_s_events() {
    let home = scratch("prune");
    let now = 1_792_000_000;
    let old = now - 11 * DAY;
    let (conn, _) = db::open(&db::path(&home), now).unwrap();
    for (id, issue, ended) in [("old", 1, old), ("new", 2, now - DAY)] {
        conn.execute(
            "INSERT INTO runs (id, kind, issue, started, ended) VALUES (?1, 'work', ?2, ?3, ?3)",
            rusqlite::params![id, issue, ended],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO run_steps (run_id, step, round) VALUES (?1, 'explore', 1)",
            [id],
        )
        .unwrap();
        conn.execute("INSERT INTO run_roles (run_id, role) VALUES (?1, 'ops')", [id])
            .unwrap();
    }
    // Issue 1 ended long ago; issue 3 started eleven days ago and runs still.
    for (issue, event, at) in [
        (1, "ready", old),
        (1, "started", old),
        (1, "ended", old),
        (3, "ready", old),
        (3, "started", old),
        // Started a day and more before the cutoff, never ended: no cycle
        // runs that long, so it goes like any other.
        (6, "ready", now - 12 * DAY),
        (6, "started", now - 11 * DAY - 1),
        (2, "ready", now - DAY),
    ] {
        db::event(&conn, issue, event, at).unwrap();
    }
    conn.execute("INSERT INTO samples (at) VALUES (?1), (?2)", [old, now - 60])
        .unwrap();
    execute(&home, &Cmd::Prune, now).unwrap();
    assert_eq!(one::<String>(&conn, "SELECT group_concat(id) FROM runs"), "new");
    assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM run_steps"), 1);
    assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM run_roles"), 1);
    assert_eq!(
        one::<String>(
            &conn,
            "SELECT group_concat(issue || ':' || event, ' ') FROM (SELECT * FROM issue_events ORDER BY issue, event)"
        ),
        "2:ready 3:ready 3:started"
    );
    assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM samples"), 1);
    // Once it has ended, the next prune takes it.
    db::event(&conn, 3, "ended", now).unwrap();
    execute(&home, &Cmd::Prune, now).unwrap();
    assert_eq!(
        one::<String>(&conn, "SELECT group_concat(issue || ':' || event, ' ') FROM issue_events"),
        "2:ready 3:ended"
    );
}

#[test]
fn a_database_another_program_made_meanwhile_is_not_moved_aside() {
    let home = scratch("race");
    let file = db::path(&home);
    let now = 1_792_000_000;
    // Both read user_version 0; the other made the schema before this one
    // took the write lock.
    let mut late = Connection::open(&file).unwrap();
    db::open(&file, now).unwrap();
    assert!(db::settle(&mut late, 0, now + 5).is_ok());
    assert_eq!(one::<String>(&late, "SELECT value FROM meta WHERE key = 'created'"), "1792000000");
    assert!(!home.join("state/analytics.db.broken").exists());
}

#[test]
fn programs_that_open_a_new_database_at_once_all_get_it() {
    let home = scratch("concurrent");
    let file = db::path(&home);
    let openers: Vec<_> = (0..8)
        .map(|i| {
            let file = file.clone();
            std::thread::spawn(move || {
                let (conn, moved) = db::open(&file, 1_792_000_000).unwrap();
                db::event(&conn, i, "ready", 1_792_000_000).unwrap();
                moved
            })
        })
        .collect();
    for o in openers {
        assert!(o.join().unwrap().is_none(), "a valid database was moved aside");
    }
    assert!(!home.join("state/analytics.db.broken").exists());
    let conn = Connection::open(&file).unwrap();
    assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM issue_events"), 8);
    assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM meta"), 3);
}

#[test]
fn a_database_that_cannot_be_written_is_an_error_and_stays() {
    let now = 1_792_000_000;
    // A file of its own that may not be written.
    let home = scratch("readonly-file");
    let file = db::path(&home);
    execute(&home, &Cmd::Event(1, "ready".into(), None), now).unwrap();
    let before = fs::read(&file).unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o400)).unwrap();
    assert_eq!(execute(&home, &Cmd::Event(2, "ready".into(), None), now).unwrap_err().0, 70);
    assert!(fs::read(&file).unwrap() == before);
    assert!(!home.join("state/analytics.db.broken").exists());
    // A directory it may not write in, with the database in it.
    let home = scratch("readonly-dir");
    let file = db::path(&home);
    execute(&home, &Cmd::Event(1, "ready".into(), None), now).unwrap();
    let state = home.join("state");
    fs::set_permissions(&state, fs::Permissions::from_mode(0o500)).unwrap();
    let err = execute(&home, &Cmd::Event(2, "ready".into(), None), now);
    fs::set_permissions(&state, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(err.unwrap_err().0, 70);
    assert!(file.is_file());
    assert!(!home.join("state/analytics.db.broken").exists());
}

#[test]
fn a_write_prunes_too() {
    let home = scratch("writeprune");
    let now = 1_792_000_000;
    execute(&home, &Cmd::Event(4, "ready".into(), None), now - 11 * DAY).unwrap();
    execute(&home, &Cmd::Event(5, "ready".into(), None), now).unwrap();
    let conn = Connection::open(db::path(&home)).unwrap();
    assert_eq!(one::<String>(&conn, "SELECT group_concat(issue) FROM issue_events"), "5");
}

#[test]
fn each_event_is_stored_once_per_issue() {
    let home = scratch("events");
    let now = 1_792_000_000;
    execute(&home, &Cmd::Event(8, "ready".into(), None), now).unwrap();
    execute(&home, &Cmd::Event(8, "ready".into(), None), now + 100).unwrap();
    execute(&home, &Cmd::Event(8, "started".into(), None), now + 200).unwrap();
    execute(&home, &Cmd::Event(9, "ready".into(), None), now + 300).unwrap();
    let conn = Connection::open(db::path(&home)).unwrap();
    assert_eq!(
        one::<String>(
            &conn,
            "SELECT group_concat(issue || ':' || event || ':' || (at - 1792000000), ' ')
             FROM (SELECT * FROM issue_events ORDER BY at)"
        ),
        "8:ready:0 8:started:200 9:ready:300"
    );
}

#[test]
fn a_new_database_is_0600_in_wal_mode_with_its_schema_version() {
    let home = scratch("new");
    execute(&home, &Cmd::Prune, 1_792_000_000).unwrap();
    let file = db::path(&home);
    assert_eq!(fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
    let conn = Connection::open(&file).unwrap();
    assert_eq!(one::<i64>(&conn, "PRAGMA user_version"), db::SCHEMA_VERSION);
    assert_eq!(one::<String>(&conn, "PRAGMA journal_mode"), "wal");
    assert_eq!(
        one::<String>(&conn, "SELECT group_concat(key || '=' || value, ' ') FROM (SELECT * FROM meta ORDER BY key)"),
        "created=1792000000 retention_days=10 schema=1"
    );
}

#[test]
fn a_broken_file_is_moved_aside_and_a_new_database_made() {
    for (name, content) in [
        ("garbage", b"this is not a database at all, but it is long enough to be read as a header. ".repeat(20)),
        ("tables", Vec::new()),
    ] {
        let home = scratch(name);
        let file = db::path(&home);
        match name {
            "tables" => {
                let c = Connection::open(&file).unwrap();
                c.execute_batch("CREATE TABLE other (x)").unwrap();
            }
            _ => fs::write(&file, &content).unwrap(),
        }
        let before = fs::read(&file).unwrap();
        execute(&home, &Cmd::Event(1, "ready".into(), None), 1_792_000_000).unwrap();
        let aside = fs::read(home.join("state/analytics.db.broken")).unwrap();
        assert!(aside == before, "{name}: the file moved aside is not the one found");
        let conn = Connection::open(&file).unwrap();
        assert_eq!(one::<i64>(&conn, "PRAGMA user_version"), db::SCHEMA_VERSION, "{name}");
        assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM issue_events"), 1, "{name}");
    }
}

#[test]
fn a_database_of_another_schema_is_kept_aside_under_its_version() {
    let home = scratch("foreign");
    let file = db::path(&home);
    {
        let c = Connection::open(&file).unwrap();
        c.execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA wal_autocheckpoint = 0;
             CREATE TABLE runs (x); INSERT INTO runs VALUES (42); PRAGMA user_version = 99",
        )
        .unwrap();
        // The row is still in the -wal when the file is moved.
        std::mem::forget(c);
    }
    execute(&home, &Cmd::Event(1, "ready".into()), 1_792_000_000).unwrap();
    assert!(!home.join("state/analytics.db.broken").exists());
    let kept = Connection::open(home.join("state/analytics.db.schema-99")).unwrap();
    assert_eq!(one::<i64>(&kept, "PRAGMA user_version"), 99);
    assert_eq!(one::<i64>(&kept, "SELECT x FROM runs"), 42);
    let conn = Connection::open(&file).unwrap();
    assert_eq!(one::<i64>(&conn, "PRAGMA user_version"), db::SCHEMA_VERSION);
    assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM issue_events"), 1);
}

#[test]
fn prune_deletes_a_kept_schema_ten_days_after_its_last_write() {
    let home = scratch("aside");
    let state = home.join("state");
    let now = 1_792_000_000;
    let at = |secs: i64| std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs as u64);
    for (name, written) in [
        ("analytics.db.schema-7", now - 11 * 86_400),
        ("analytics.db.schema-7-wal", now - 11 * 86_400),
        ("analytics.db.schema-8", now - 9 * 86_400),
        ("analytics.db.broken", now - 30 * 86_400),
        ("other.schema-1", now - 30 * 86_400),
    ] {
        fs::create_dir_all(&state).unwrap();
        let f = fs::File::create(state.join(name)).unwrap();
        f.set_modified(at(written)).unwrap();
    }
    execute(&home, &Cmd::Prune, now).unwrap();
    let mut left: Vec<String> = fs::read_dir(&state)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| !["analytics.db", "analytics.db-wal", "analytics.db-shm", "cycles"].contains(&n.as_str()))
        .collect();
    left.sort();
    assert_eq!(left, ["analytics.db.broken", "analytics.db.schema-8", "other.schema-1"]);
}

#[test]
fn export_writes_a_copy_that_opens_with_the_same_version_mode_0600_and_nothing_private() {
    let home = scratch("export");
    let started = secs("2026-10-18T10:16:39Z");
    cycle(
        &home,
        "headless-316134e",
        started,
        json!({"at": iso(started + 30), "started": started, "rc": 10, "status": "handoff",
               "parked": true, "pr": "https://github.com/o/r/pull/3"}),
    );
    let path = home.join("state/cycles/7.result.json");
    let mut r: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    r["issues"][0]["failure"] = json!({"kind": "provider",
        "cause": format!("GET https://user:hunter2@models.example/v1 from {} and {} with Bearer abcdefghijklmnopqrstuvwxyz; \
            connect ECONNREFUSED models.example:8000; git@host.tld:org/repo; \
            Incorrect API key provided: sk-abc123def456ghi789jkl0; dial fe80::1%eth0 and 2001:db8::7",
            ip([10, 0, 0, 7]), ip([203, 0, 113, 9]))});
    fs::write(&path, r.to_string()).unwrap();
    let now = started + 60;
    execute(&home, &Cmd::Run(7, Given::default()), now).unwrap();
    execute(&home, &Cmd::Event(7, "ready".into(), None), now).unwrap();
    let out = home.join("copy.db");
    execute(&home, &Cmd::Export(out.clone()), now).unwrap();
    assert_eq!(fs::metadata(&out).unwrap().permissions().mode() & 0o777, 0o600);
    let copy = Connection::open(&out).unwrap();
    assert_eq!(one::<i64>(&copy, "PRAGMA user_version"), db::SCHEMA_VERSION);
    assert_eq!(one::<i64>(&copy, "SELECT count(*) FROM runs"), 1);
    assert_eq!(one::<i64>(&copy, "SELECT count(*) FROM run_steps"), 2);
    // Every text value of every table: no token, no URL with a host, no
    // address.
    let token = regex::Regex::new(
        r"gh[pousr]_[A-Za-z0-9]{8}|github_pat_|(?i)bearer [A-Za-z0-9]{8}|:[^@/\s]+@|sk-[A-Za-z0-9]{6}|@",
    )
    .unwrap();
    let url = regex::Regex::new(r"(?i)[a-z]+://[A-Za-z0-9.-]+|models\.example|host\.tld").unwrap();
    let address =
        regex::Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b|(?i)[0-9a-f]+::[0-9a-f]+").unwrap();
    let tables: Vec<String> = copy
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let mut seen = 0;
    for t in tables {
        let mut q = copy.prepare(&format!("SELECT * FROM {t}")).unwrap();
        let n = q.column_count();
        let mut rows = q.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            for i in 0..n {
                if let Ok(Some(v)) = row.get::<_, Option<String>>(i) {
                    seen += 1;
                    for re in [&token, &url, &address] {
                        assert!(!re.is_match(&v), "{t}: {v}");
                    }
                }
            }
        }
    }
    assert!(seen > 10);
    // An export never replaces a file.
    assert_eq!(execute(&home, &Cmd::Export(out), now).unwrap_err().0, 73);
}

#[test]
fn the_views_answer_for_a_known_data_set() {
    let home = scratch("views");
    let day0 = 1_792_022_400; // 2026-10-15T00:00:00Z
    let (conn, _) = db::open(&db::path(&home), day0).unwrap();
    // Issue 1: two runs, merged on the second. Issue 2: one run, merged.
    // Issue 3: one failed run. All on one version pair but the last.
    let runs = [
        ("a", 1, day0 + 100, 600, "handoff", 1000, "v1"),
        ("b", 1, day0 + 2000, 1200, "merged", 3000, "v1"),
        ("c", 2, day0 + 3000, 1800, "merged", 2000, "v1"),
        ("d", 3, day0 + DAY + 10, 300, "failed", 500, "v2"),
    ];
    for (id, issue, started, secs, outcome, tokens, tv) in runs {
        conn.execute(
            "INSERT INTO runs (id, kind, issue, started, ended, seconds, outcome, input,
                               tehas_version, rukas_version)
             VALUES (?1, 'work', ?2, ?3, ?4, ?5, ?6, ?7, ?8, '0.13.0')",
            rusqlite::params![id, issue, started, started + secs, secs, outcome, tokens, tv],
        )
        .unwrap();
    }
    db::event(&conn, 1, "ready", day0).unwrap();
    db::event(&conn, 2, "ready", day0 + 2900).unwrap();
    let lead: Vec<(i64, i64, i64)> = conn
        .prepare("SELECT issue, lead_seconds, runs FROM v_lead ORDER BY issue")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(lead, vec![(1, 3200, 2), (2, 1900, 1)]);
    let daily: Vec<(String, i64, i64, i64, i64, i64)> = conn
        .prepare("SELECT day, shipped, runs, failed, minutes, tokens FROM v_daily ORDER BY day")
        .unwrap()
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        daily,
        vec![
            ("2026-10-15".into(), 2, 3, 0, 60, 6000),
            ("2026-10-16".into(), 0, 1, 1, 5, 500),
        ]
    );
    let versions: Vec<(String, i64, f64, f64, f64, f64)> = conn
        .prepare(
            "SELECT tehas_version, runs, pr_share, handoff_share, median_minutes, median_tokens
             FROM v_versions ORDER BY tehas_version",
        )
        .unwrap()
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        versions,
        vec![
            ("v1".into(), 3, 0.667, 0.333, 20.0, 2000.0),
            ("v2".into(), 1, 0.0, 0.0, 5.0, 500.0),
        ]
    );
}

#[test]
fn iso_times_are_read_as_utc() {
    assert_eq!(iso_ms("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(iso_ms("2026-10-15T18:38:44.024Z"), Some(1_792_089_524_024));
    assert_eq!(iso_ms("2000-03-01T00:00:01.5Z"), Some(951_868_801_500));
    assert_eq!(iso_ms("2026-10-15 18:38:44"), None);
    assert_eq!(iso_ms("2026-13-15T18:38:44Z"), None);
}

#[test]
fn the_arguments_are_checked() {
    let args = |s: &str| s.split(' ').map(str::to_string).collect::<Vec<_>>();
    assert_eq!(parse(&args("run 12")), Some(Cmd::Run(12, Given::default())));
    let image = format!("sha256:{}", "0f".repeat(32));
    assert_eq!(
        parse(&args(&format!(
            "run 12 --oom 0 --image {image} --forks 3 --rukas-image 1234abc"
        ))),
        Some(Cmd::Run(
            12,
            Given {
                image: Some(image.clone()),
                rukas_image: Some("1234abc".into()),
                oom: Some(0),
                forks: Some(3),
            }
        ))
    );
    for bad in [
        "run 12 --oom".to_string(),
        "run 12 --oom -1".into(),
        "run 12 --oom 01".into(),
        "run 12 --forks 1234567890".into(),
        "run 12 --forks x".into(),
        "run 12 --image sha256:abc".into(),
        format!("run 12 --image {}", "0f".repeat(32)),
        format!("run 12 --image sha256:{}", "0F".repeat(32)),
        "run 12 --rukas-image 123456".into(),
        "run 12 --rukas-image xyz1234".into(),
        "run 12 --oom 1 --oom 2".into(),
        "run 12 --other 1".into(),
    ] {
        assert_eq!(parse(&args(&bad)), None, "{bad}");
    }
    assert_eq!(parse(&args("event 12 merged")), Some(Cmd::Event(12, "merged".into(), None)));
    assert_eq!(parse(&args("prune")), Some(Cmd::Prune));
    assert_eq!(parse(&args("sample")), Some(Cmd::Sample));
    assert_eq!(
        parse(&args("event 12 ready --at 1792000000")),
        Some(Cmd::Event(12, "ready".into(), Some(1_792_000_000)))
    );
    assert_eq!(parse(&args("event 12 ready --at 0")), Some(Cmd::Event(12, "ready".into(), Some(0))));
    for bad in [
        "event 12 ready --at",
        "event 12 ready --at -5",
        "event 12 ready --at 01",
        "event 12 ready --at 12.5",
        "event 12 ready --at 1234567890123",
        "event 12 ready --at 1 --at 2",
        "event 12 ready --when 1",
        "event 12 ready 1792000000",
    ] {
        assert_eq!(parse(&args(bad)), None, "{bad}");
    }
    assert_eq!(parse(&args("export /tmp/x.db")), Some(Cmd::Export("/tmp/x.db".into())));
    for bad in ["run", "run 0", "run 012", "run -1", "run x", "event 12 shipped", "prune now", "sample now", "report", "export"] {
        assert_eq!(parse(&args(bad)), None, "{bad}");
    }
}

// The schema as SQLite holds it, compared with schema.pinned. A change to
// schema.sql fails here until user_version is raised and the pinned copy is
// replaced with what this prints.
#[test]
fn schema_is_pinned() {
    let home = scratch("pinned");
    let (conn, _) = db::open(&db::path(&home), 0).unwrap();
    let mut dump = format!(
        "user_version {}\n",
        one::<i64>(&conn, "PRAGMA user_version")
    );
    let mut q = conn
        .prepare("SELECT type, name, sql FROM sqlite_master WHERE sql IS NOT NULL ORDER BY type, name")
        .unwrap();
    let rows = q
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))
        .unwrap();
    for row in rows {
        let (kind, name, sql) = row.unwrap();
        dump.push_str(&format!("\n-- {kind} {name}\n{sql};\n"));
    }
    let pinned = include_str!("schema.pinned");
    if dump != pinned {
        let version_line = |s: &str| s.lines().next().unwrap_or("").to_string();
        if version_line(&dump) == version_line(pinned) {
            panic!(
                "the schema changed and user_version did not: raise SCHEMA_VERSION in db.rs, \
                 then replace src/schema.pinned with:\n{dump}"
            );
        }
        panic!("user_version changed: replace src/schema.pinned with:\n{dump}");
    }
}

#[test]
fn run_reads_nothing_through_a_link() {
    let home = scratch("links");
    let started = secs("2026-10-18T10:16:39Z");
    cycle(
        &home,
        "headless-316134e",
        started,
        json!({"at": iso(started + 30), "started": started, "rc": 10, "status": "handoff"}),
    );
    let result = home.join("state/cycles/7.result.json");
    let moved = home.join("result.json");
    fs::rename(&result, &moved).unwrap();
    std::os::unix::fs::symlink(&moved, &result).unwrap();
    let r = run::read(&home, 7, &Given::default()).unwrap();
    assert_eq!(r.result_schema, None);
    assert_eq!(r.runner_ms, None);
}

// The live files of one poll, as libexec/status, libexec/dispatch and
// libexec/energy write them, at `at`.
fn live(home: &Path, at: i64, why: &str, limit_room: Option<i64>) {
    fs::create_dir_all(home.join("live")).unwrap();
    let issue = |n: i64| json!({"number": n, "title": "t", "url": "u"});
    put(
        home,
        "live/status.json",
        &json!({"generatedAt": iso(at), "repo": "acme/widget", "paused": true,
                "running": [issue(1), issue(2)], "queued": [issue(3), issue(4), issue(5)],
                "awaiting": [issue(6)], "stopped": [], "completed": [issue(9)]})
        .to_string(),
    );
    let mut d = json!({"time": iso(at), "why": why, "hold": "holding: ...", "count": 2,
                       "room": 1, "committed": 0, "max": 0, "reserve": "2g", "free": 3,
                       "awaitRoom": null});
    if let Some(r) = limit_room {
        d["limitRoom"] = json!(r);
    }
    put(home, "live/dispatch.json", &d.to_string());
    put(
        home,
        "live/energy.json",
        &json!({"generatedAt": iso(at), "totals": {"watts": 412.5, "hosts": 2}}).to_string(),
    );
}

fn sample_row(home: &Path) -> String {
    let conn = Connection::open(db::path(home)).unwrap();
    one::<String>(
        &conn,
        "SELECT group_concat(row, ' | ') FROM (SELECT at || ' ' || coalesce(in_line, '-') || ' ' ||
           coalesce(running, '-') || ' ' || coalesce(awaiting, '-') || ' ' || coalesce(stopped, '-') || ' ' ||
           coalesce(planning, '-') || ' ' || coalesce(cycle_limit, '-') || ' ' || coalesce(paused, '-') || ' ' ||
           coalesce('[' || held || ']', '-') || ' ' || coalesce(watts, '-') AS row FROM samples ORDER BY at)",
    )
}

#[test]
fn a_sample_reads_the_live_files_of_the_poll() {
    let home = scratch("sample");
    let now = 1_792_000_000;
    live(&home, now - 30, "awaiting", None);
    execute(&home, &Cmd::Sample, now).unwrap();
    // Five slots: two running and three free. Planning stays NULL until
    // status.json counts it.
    assert_eq!(sample_row(&home), format!("{now} 3 2 1 0 - 5 1 [awaiting] 412.5"));

    // The limit of tehas limit allows fewer; nothing holds.
    live(&home, now + 90, "", Some(1));
    execute(&home, &Cmd::Sample, now + 100).unwrap();
    assert!(sample_row(&home).ends_with(&format!(" | {} 3 2 1 0 - 3 1 [] 412.5", now + 100)));
}

#[test]
fn a_live_file_that_is_missing_stale_a_link_or_malformed_gives_null_columns() {
    let now = 1_792_000_000;
    let nothing = |at: i64| format!("{at} - - - - - - - - -");

    let home = scratch("sample-none");
    execute(&home, &Cmd::Sample, now).unwrap();
    assert_eq!(sample_row(&home), nothing(now));

    // An earlier poll's files: a paused poll writes no dispatch.json.
    let home = scratch("sample-stale");
    live(&home, now - 601, "count", None);
    execute(&home, &Cmd::Sample, now).unwrap();
    assert_eq!(sample_row(&home), nothing(now));
    live(&home, now - 30, "count", None);
    // A poll runs every two minutes: a dispatch.json older than that is an
    // earlier poll's, though status.json still counts.
    put(&home, "live/dispatch.json", &json!({"time": iso(now - 200), "why": "count", "count": 1, "free": 0}).to_string());
    execute(&home, &Cmd::Sample, now + 1).unwrap();
    assert!(sample_row(&home).ends_with(&format!(" | {} 3 2 1 0 - - 1 - 412.5", now + 1)));

    let home = scratch("sample-links");
    live(&home, now - 30, "count", None);
    for name in ["status.json", "dispatch.json", "energy.json"] {
        let p = home.join("live").join(name);
        let moved = home.join(name);
        fs::rename(&p, &moved).unwrap();
        std::os::unix::fs::symlink(&moved, &p).unwrap();
    }
    execute(&home, &Cmd::Sample, now).unwrap();
    assert_eq!(sample_row(&home), nothing(now));

    let home = scratch("sample-bad");
    fs::create_dir_all(home.join("live")).unwrap();
    put(&home, "live/status.json", "not json");
    put(
        &home,
        "live/dispatch.json",
        &json!({"time": "yesterday", "why": "count", "count": 1, "free": 0}).to_string(),
    );
    put(
        &home,
        "live/energy.json",
        &json!({"generatedAt": iso(now), "totals": {"watts": null}}).to_string(),
    );
    execute(&home, &Cmd::Sample, now).unwrap();
    assert_eq!(sample_row(&home), nothing(now));

    // Fields of the wrong type, and a hold that is not one word.
    let home = scratch("sample-types");
    fs::create_dir_all(home.join("live")).unwrap();
    put(
        &home,
        "live/status.json",
        &json!({"generatedAt": iso(now), "paused": "no", "queued": 3, "running": [], "awaiting": {}, "stopped": null})
            .to_string(),
    );
    put(
        &home,
        "live/dispatch.json",
        &json!({"time": iso(now), "why": "holding: <b>", "count": "2", "free": 1}).to_string(),
    );
    execute(&home, &Cmd::Sample, now).unwrap();
    assert_eq!(sample_row(&home), format!("{now} - 0 - - - - - - -"));
}

#[test]
fn a_sample_prunes_too_and_two_in_one_second_keep_the_last() {
    let home = scratch("sample-prune");
    let now = 1_792_000_000;
    execute(&home, &Cmd::Event(4, "ready".into(), None), now - 11 * DAY).unwrap();
    execute(&home, &Cmd::Sample, now - 11 * DAY).unwrap();
    live(&home, now, "count", None);
    execute(&home, &Cmd::Sample, now).unwrap();
    live(&home, now, "", None);
    execute(&home, &Cmd::Sample, now).unwrap();
    assert_eq!(sample_row(&home), format!("{now} 3 2 1 0 - 5 1 [] 412.5"));
    let conn = Connection::open(db::path(&home)).unwrap();
    assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM issue_events"), 0);
}

#[test]
fn an_event_is_stored_at_the_time_given_and_one_ahead_of_now_is_refused() {
    let home = scratch("event-at");
    let now = 1_792_000_000;
    execute(&home, &Cmd::Event(8, "ready".into(), Some(now - 3600)), now).unwrap();
    execute(&home, &Cmd::Event(8, "started".into(), None), now).unwrap();
    execute(&home, &Cmd::Event(9, "ready".into(), Some(now + 60)), now).unwrap();
    let err = execute(&home, &Cmd::Event(10, "ready".into(), Some(now + 61)), now).unwrap_err();
    assert_eq!(err.0, 64);
    let conn = Connection::open(db::path(&home)).unwrap();
    assert_eq!(
        one::<String>(
            &conn,
            "SELECT group_concat(issue || ':' || event || ':' || (at - 1792000000), ' ')
             FROM (SELECT * FROM issue_events ORDER BY issue, event)"
        ),
        "8:ready:-3600 8:started:0 9:ready:60"
    );
}
