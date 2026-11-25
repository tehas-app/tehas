//! The program against the headless runner's fixtures, in scratch homes.

use crate::db;
use crate::run::{self, iso_ms, Given};
use crate::{execute, parse, Cmd, AT_AHEAD};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tehas_core::home::{Home, Live, State};
use tehas_core::testdir::TestDir;

const DAY: i64 = 86_400;

fn scratch(name: &str) -> TestDir {
    let dir = TestDir::new(&format!("analytics-{name}"));
    fs::create_dir_all(h(&dir).cycles_dir()).unwrap();
    fs::create_dir_all(dir.join("logs")).unwrap();
    fs::create_dir_all(dir.join("docs")).unwrap();
    fs::create_dir_all(dir.join("tmp")).unwrap();
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
    tehas_core::time::iso(t)
}

// An address written out here, so no private one stands in the source.
fn ip(octets: [u8; 4]) -> String {
    octets.map(|o| o.to_string()).join(".")
}

fn h(home: &Path) -> Home {
    Home::new(home)
}

fn put_at(p: PathBuf, text: &str) {
    fs::write(p, text).unwrap();
}

fn put(home: &Path, name: &str, text: &str) {
    let p = home.join(name);
    fs::write(&p, text).unwrap();
}

/// Where a test's facts of the issue's run or turn are, as tehas-cycle
/// writes them for a call.
fn facts_path(home: &Path, issue: i64) -> PathBuf {
    home.join(format!("tmp/facts-{issue}.json"))
}

/// Sets one fact of the issue's run and keeps the others.
fn fact(home: &Path, issue: i64, key: &str, value: Value) {
    let p = facts_path(home, issue);
    let mut facts: Value = fs::read_to_string(&p)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| {
            json!({"line": null, "attempt": null, "approval": null, "settings": null, "rukas": null})
        });
    facts[key] = value;
    fs::write(&p, facts.to_string()).unwrap();
}

/// The run's facts with these history lines: the last one, and how many.
fn history(home: &Path, issue: i64, lines: &[Value]) {
    fact(
        home,
        issue,
        "line",
        lines.last().cloned().unwrap_or(Value::Null),
    );
    fact(home, issue, "attempt", json!(lines.len()));
}

fn given(home: &Path, issue: i64) -> Given {
    Given {
        facts: Some(facts_path(home, issue)),
        ..Given::default()
    }
}

fn record(home: &Path, issue: i64, now: i64) -> Connection {
    execute(home, &Cmd::Run(issue, given(home, issue)), now).unwrap();
    Connection::open(db::path(home)).unwrap()
}

fn one<T: rusqlite::types::FromSql>(conn: &Connection, sql: &str) -> T {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

// `one` with the offsets of an --at case bound, so a test compares `at` to the
// `now` it passed rather than to a literal epoch.
fn one_at<T: rusqlite::types::FromSql>(conn: &Connection, sql: &str, now: i64) -> T {
    conn.query_row(sql, rusqlite::params![now], |r| r.get(0))
        .unwrap()
}

fn steps(conn: &Connection) -> Vec<(String, i64, i64, i64, i64)> {
    let mut q = conn
        .prepare(
            "SELECT step, round, started, ended, seconds FROM run_steps ORDER BY started, step",
        )
        .unwrap();
    q.query_map([], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

// A cycle of issue 7 from a fixture, as tehas-cycle and its work leave
// it: the result, the progress summary and the version on disk, and the facts
// of the settings, the approval, the image's commit and the history line
// `line`.
fn cycle(home: &Path, name: &str, started: i64, line: Value) {
    let dir = fixture(name);
    fs::copy(
        dir.join("result.json"),
        h(home).cycle_note(7, "result.json"),
    )
    .unwrap();
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
    put_at(
        h(home).cycle_note(7, "progress.json"),
        r#"{"roles":[],"reason":"","failure":null,"reviewHeld":false,"from":{"roles":"progress","failure":"progress"}}"#,
    );
    fact(
        home,
        7,
        "settings",
        json!({"container": {"PI_ENSEMBLE_SUBAGENT_MODEL": "sub-model",
                             "PI_ENSEMBLE_SUBAGENT_PROVIDER": "sub-provider",
                             "PI_ENSEMBLE_MODEL_OPS": "ops-model",
                             "PI_ENSEMBLE_PROVIDER_OPS": "ops-provider"},
               "host": {"TEHAS_WORK_TIMEOUT": "21600"}, "concurrentCycles": 2}),
    );
    fact(
        home,
        7,
        "approval",
        json!({"at": iso(started - 90), "how": "approved"}),
    );
    fact(home, 7, "rukas", json!("abcdef1"));
    put(home, "docs/version.txt", "v0.7.22+1234abc\n");
    history(home, 7, &[line]);
}

#[test]
fn each_fixture_result_is_stored_with_what_the_outcome_drops() {
    // started and the end of the last step of each fixture, the runner's
    // durationMs, and the status, cap and exit tehas wrote: for schema 3
    // tehas-cycle reads no outcome, so the history says unknown.
    let cases = [
        (
            "headless-0.12.80",
            "2026-10-15T18:38:44Z",
            64974,
            3,
            61,
            "unknown",
        ),
        (
            "headless-1ca933e",
            "2026-10-15T20:59:05Z",
            64061,
            11,
            52,
            "unknown",
        ),
        (
            "headless-316134e",
            "2026-10-18T10:16:49Z",
            8108,
            8,
            0,
            "handoff",
        ),
    ];
    for (name, first, ms, explore, handoff, status) in cases {
        let home = scratch(name);
        let started = secs(first) - 10;
        let ended = started + 80;
        let line = if status == "unknown" {
            json!({"at": iso(ended), "started": started, "rc": 10, "status": "unknown",
                   "step": "-", "parked": true, "provider": false, "how": "approved"})
        } else {
            json!({"at": iso(ended), "started": started, "rc": 10, "status": "handoff",
                   "step": "explore", "cap": "explore-bodies-empty", "failure_kind": "cap",
                   "parked": true, "provider": false, "how": "approved"})
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
        let more: (i64, i64, i64, i64, String, String, String) = conn
            .query_row(
                "SELECT runner_ms, cache_write, dispatches, result_schema, last_step,
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
        assert_eq!(
            s[0],
            ("explore".into(), 1, first, first + explore, explore),
            "{name}"
        );
        assert_eq!(
            (s[1].0.as_str(), s[1].1, s[1].4),
            ("handoff", 1, handoff),
            "{name}"
        );
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
fn cache_write_and_role_usage_are_kept_and_the_runner_s_cost_is_not() {
    let home = scratch("usage");
    let started = secs("2026-10-18T10:16:39Z");
    cycle(
        &home,
        "headless-316134e",
        started,
        json!({"at": iso(started + 30), "started": started, "rc": 10, "status": "handoff",
               "parked": true}),
    );
    let path = h(&home).cycle_note(7, "result.json");
    let mut r: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    r["issues"][0]["usage"] = json!({"input": 1000, "output": 200, "cacheRead": 5000,
        "cacheWrite": 300, "cost": 0.25, "turns": 9, "dispatches": 3,
        "byRole": {"code-review-specialist": {"input": 600, "output": 100, "cacheRead": 4000,
                   "cacheWrite": 200, "cost": 0.125, "turns": 5, "dispatches": 2},
                   "ops": {"input": 400, "output": 100, "cacheRead": 1000, "cacheWrite": 100,
                   "cost": 0.125, "turns": 4, "dispatches": 1}}});
    fs::write(&path, r.to_string()).unwrap();
    let conn = record(&home, 7, started + 60);
    let usage: (i64, i64, i64, i64, i64, i64) = conn
        .query_row(
            "SELECT input, output, cache_read, cache_write, turns, dispatches FROM runs",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(usage, (1000, 200, 5000, 300, 9, 3));
    // The runner's cost is read past: no column holds it.
    assert!(!columns(&conn, "runs").contains(&"cost".to_string()));
    assert!(!columns(&conn, "run_roles").contains(&"cost".to_string()));
    let roles: Vec<(String, String, i64, i64)> = conn
        .prepare("SELECT role, model, input, cache_write FROM run_roles ORDER BY role")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    // A role without a model of its own has the subagent's.
    assert_eq!(
        roles,
        vec![
            (
                "code-review-specialist".into(),
                "sub-model".into(),
                600,
                200
            ),
            ("ops".into(), "ops-model".into(), 400, 100),
        ]
    );
}

#[test]
fn a_schema_5_result_is_read() {
    let home = scratch("schema-5");
    let started = secs("2026-10-18T10:16:39Z");
    cycle(
        &home,
        "headless-316134e",
        started,
        json!({"at": iso(started + 30), "started": started, "rc": 10, "status": "handoff",
               "parked": true}),
    );
    let path = h(&home).cycle_note(7, "result.json");
    let mut r: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    r["schemaVersion"] = json!(5);
    r["issues"][0]["usage"]["input"] = json!(1234);
    fs::write(&path, r.to_string()).unwrap();
    let conn = record(&home, 7, started + 60);
    assert_eq!(
        one::<String>(
            &conn,
            "SELECT result_schema || ' ' || input || ' ' || rukas_version FROM runs"
        ),
        "5 1234 0.12.80"
    );
}

#[test]
fn tool_counts_and_the_driver_that_ran_are_kept_and_absent_ones_stay_null() {
    let home = scratch("tool-counts");
    let started = secs("2026-10-18T10:16:39Z");
    let facts = json!({"at": iso(started + 30), "started": started, "rc": 10,
                       "status": "handoff", "parked": true});
    cycle(&home, "headless-316134e", started, facts.clone());
    let path = h(&home).cycle_note(7, "result.json");
    let mut r: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    r["schemaVersion"] = json!(5);
    r["issues"][0]["driver"] = json!("lean");
    r["issues"][0]["usage"]["toolCalls"] = json!(12);
    r["issues"][0]["usage"]["toolErrors"] = json!(3);
    r["issues"][0]["usage"]["byRole"] = json!({
        "developer": {"input": 1, "output": 1, "cacheRead": 0, "cacheWrite": 0, "cost": 0,
                      "turns": 1, "dispatches": 1, "toolCalls": 12, "toolErrors": 3}});
    fs::write(&path, r.to_string()).unwrap();
    let conn = record(&home, 7, started + 60);
    assert_eq!(
        one::<String>(
            &conn,
            "SELECT tool_calls || ' ' || tool_errors || ' ' || result_driver FROM runs"
        ),
        "12 3 lean"
    );
    assert_eq!(
        one::<String>(
            &conn,
            "SELECT role || ' ' || tool_calls || ' ' || tool_errors FROM run_roles"
        ),
        "developer 12 3"
    );

    // A result from before pi-rukas counted, or with a driver it does not
    // know, leaves the columns empty: no count, not none.
    let home = scratch("tool-counts-absent");
    cycle(&home, "headless-316134e", started, facts);
    let path = h(&home).cycle_note(7, "result.json");
    let mut r: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    r["issues"][0]["driver"] = json!("other");
    fs::write(&path, r.to_string()).unwrap();
    let conn = record(&home, 7, started + 60);
    assert_eq!(
        one::<i64>(
            &conn,
            "SELECT count(*) FROM runs
             WHERE tool_calls IS NULL AND tool_errors IS NULL AND result_driver IS NULL"
        ),
        1
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
    let path = h(&home).cycle_note(7, "result.json");
    let mut r: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    r["issues"][0].as_object_mut().unwrap().remove("steps");
    fs::write(&path, r.to_string()).unwrap();
    put_at(
        h(&home).cycle_note(7, "progress.json"),
        r#"{"roles":[{"role":"ops","dispatches":2,"turns":3,"input":10,"output":20,"cacheRead":30}],"reason":"","failure":null,"reviewHeld":true}"#,
    );
    let conn = record(&home, 7, ended);
    assert_eq!(one::<i64>(&conn, "SELECT review_held FROM runs"), 1);
    assert_eq!(
        one::<String>(
            &conn,
            "SELECT role || ' ' || model || ' ' || turns || ' ' || cache_read FROM run_roles"
        ),
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
            &[
                json!({"at": iso(started + 3600), "started": started, "rc": rc, "status": status,
                     "step": "implement", "cap": cap, "parked": true, "provider": false}),
            ],
        );
        // What tehas-cycle passes on. The files it removed are not read.
        put_at(h(&home).cycle_note(12, "oom"), "9\n");
        put_at(
            h(&home).cycle_note(12, "image"),
            &format!("sha256:{} 1111111\n", "e".repeat(64)),
        );
        let given = Given {
            image: Some(format!("sha256:{}", "a".repeat(64))),
            rukas_image: Some("abc1234".into()),
            image_built: None,
            oom: Some(4),
            forks: Some(2),
            facts: Some(facts_path(&home, 12)),
        };
        execute(&home, &Cmd::Run(12, given), started + 3700).unwrap();
        let conn = Connection::open(db::path(&home)).unwrap();
        assert_eq!(
            one::<String>(
                &conn,
                "SELECT image || ' ' || rukas_commit || ' ' || forks_refused FROM runs"
            ),
            format!("sha256:{} abc1234 2", "a".repeat(64))
        );
        let row: (
            String,
            i64,
            i64,
            i64,
            i64,
            String,
            String,
            Option<String>,
            i64,
        ) = conn
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
fn the_runs_of_the_gate_come_from_the_stream_and_are_stored_one_row_each() {
    let home = scratch("gate-runs");
    let started = secs("2026-10-15T18:38:34Z");
    let ended = secs("2026-10-15T18:39:50Z");
    cycle(
        &home,
        "headless-316134e",
        started,
        json!({"at": iso(ended), "started": started, "rc": 0, "status": "merged",
               "pr": "https://github.com/o/r/pull/3"}),
    );
    // Two runs of the gate, of different tiers, one passed and one not, and
    // four lines that are no run of it: a line of another kind, one the runner
    // marks unstable, one of another issue, and one that named no time.
    let at = ended * 1000;
    let gate = |tier: &str, ms: i64, ok: bool, at: i64| {
        json!({"v": 1, "kind": "verify-completed", "issue": 7,
               "event": {"kind": "verify-completed", "step": "develop",
                          "command": format!("scripts/verify.sh --{tier}"),
                          "target.kind": tier, "tier": tier, "ok": ok, "ms": ms, "at": at}})
    };
    let stream = [
        gate("fast", 120_000, true, at - 60_000).to_string(),
        // The runner marks the line unstable: tehas reads only the kinds the
        // runner declares stable, as the manual of the cycle says.
        "{\"v\":1,\"kind\":\"verify-completed\",\"unstable\":true,\"event\":{\"ms\":999999999}}"
            .to_string(),
        // Of another kind, whatever its milliseconds say.
        "{\"v\":1,\"kind\":\"dispatch-completed\",\"event\":{\"kind\":\"dispatch-completed\",\"ms\":1}}"
            .to_string(),
        // Another issue's gate is not this run's, and a run of it that named
        // no time is no figure at all.
        "{\"v\":1,\"kind\":\"verify-completed\",\"issue\":8,\"event\":{\"ms\":7}}".to_string(),
        "{\"v\":1,\"kind\":\"verify-completed\",\"issue\":7,\"event\":{\"tier\":\"full\"}}".to_string(),
        gate("full", 300_000, false, at - 20_000).to_string(),
    ]
    .join("\n");
    put(
        &home,
        "logs/work-7-261015-183834.jsonl",
        &format!("{stream}\n"),
    );
    let conn = record(&home, 7, ended);
    assert_eq!(
        one::<String>(&conn, "SELECT gate_runs || ' ' || gate_ms FROM runs"),
        "2 420000"
    );
    let rows: Vec<(i64, String, String, String, i64, i64, i64)> = conn
        .prepare("SELECT seq, command, tier, step, ok, ms, at FROM run_gate_runs ORDER BY seq")
        .unwrap()
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        rows,
        vec![
            (
                1,
                "scriptsverify.sh--fast".into(),
                "fast".into(),
                "develop".into(),
                1,
                120_000,
                ended - 60
            ),
            (
                2,
                "scriptsverify.sh--full".into(),
                "full".into(),
                "develop".into(),
                0,
                300_000,
                ended - 20
            ),
        ]
    );
    // The steps still come from the stream's own lines, and a run recorded
    // again replaces its gate rows rather than doubling them.
    assert_eq!(steps(&conn).len(), 2);
    record(&home, 7, ended);
    assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM run_gate_runs"), 2);
}

#[test]
fn a_run_whose_stream_held_no_gate_run_records_no_gate() {
    let home = scratch("gate-none");
    let started = secs("2026-10-15T18:38:34Z");
    let ended = secs("2026-10-15T18:39:50Z");
    cycle(
        &home,
        "headless-316134e",
        started,
        json!({"at": iso(ended), "started": started, "rc": 10, "status": "handoff",
               "parked": true}),
    );
    // The fixture's stream holds no verify-completed line at all.
    let conn = record(&home, 7, ended);
    assert_eq!(
        one::<Option<i64>>(&conn, "SELECT gate_runs FROM runs"),
        None
    );
    assert_eq!(one::<Option<i64>>(&conn, "SELECT gate_ms FROM runs"), None);
    assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM run_gate_runs"), 0);
}

#[test]
fn the_gate_rows_past_the_bound_are_not_kept_and_a_time_past_it_is_no_figure() {
    let home = scratch("gate-bounds");
    let started = secs("2026-10-15T18:38:34Z");
    let ended = secs("2026-10-15T18:39:50Z");
    cycle(
        &home,
        "headless-316134e",
        started,
        json!({"at": iso(ended), "started": started, "rc": 0, "status": "merged",
               "pr": "https://github.com/o/r/pull/3"}),
    );
    let at = ended * 1000;
    let line = |ms: i64| {
        json!({"v": 1, "kind": "verify-completed", "issue": 7,
               "event": {"kind": "verify-completed", "step": "develop",
                          "command": "scripts/verify.sh", "target.kind": "full",
                          "ok": true, "ms": ms, "at": at}})
        .to_string()
    };
    // One line of every millisecond the runner may claim, and one more than
    // the bound of rows: the runaway stream is bounded rather than kept. The
    // last two lines are the bound of milliseconds itself, which is a figure,
    // and a negative time, which is none.
    let mut stream: Vec<String> = (0..run::MAX_GATE_RUNS - 1).map(|_| line(60_000)).collect();
    stream.push(line(run::MAX_GATE_MS + 1));
    stream.push(line(run::MAX_GATE_MS));
    stream.push(line(-1));
    stream.push(line(60_000));
    put(
        &home,
        "logs/work-7-261015-183834.jsonl",
        &format!("{}\n", stream.join("\n")),
    );
    let conn = record(&home, 7, ended);
    // The bound of rows, no more: the runaway line is no row and no
    // milliseconds of its own, and the read stops at the bound.
    assert_eq!(
        one::<i64>(&conn, "SELECT count(*) FROM run_gate_runs"),
        run::MAX_GATE_RUNS as i64
    );
    assert_eq!(
        one::<Option<i64>>(&conn, "SELECT gate_runs FROM runs"),
        Some(run::MAX_GATE_RUNS as i64)
    );
    assert_eq!(
        one::<Option<i64>>(&conn, "SELECT gate_ms FROM runs"),
        Some((run::MAX_GATE_RUNS as i64 - 1) * 60_000 + run::MAX_GATE_MS)
    );
    assert_eq!(
        one::<Option<i64>>(&conn, "SELECT max(ms) FROM run_gate_runs"),
        Some(run::MAX_GATE_MS)
    );
}

#[test]
fn the_steps_after_the_gate_bound_are_still_read() {
    let home = scratch("gate-bound-steps");
    let started = secs("2026-10-15T18:38:34Z");
    let ended = secs("2026-10-15T18:39:50Z");
    cycle(
        &home,
        "headless-316134e",
        started,
        json!({"at": iso(ended), "started": started, "rc": 0, "status": "merged",
               "pr": "https://github.com/o/r/pull/3"}),
    );
    // The result names no steps, so the run's steps come from the stream.
    let path = h(&home).cycle_note(7, "result.json");
    let mut r: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    r["issues"][0].as_object_mut().unwrap().remove("steps");
    fs::write(&path, r.to_string()).unwrap();
    let at = ended * 1000;
    let gate = json!({"v": 1, "kind": "verify-completed", "issue": 7,
                      "event": {"kind": "verify-completed", "step": "develop",
                                 "command": "scripts/verify.sh", "ok": true,
                                 "ms": 60_000, "at": at}})
    .to_string();
    let step = |name: &str, at: i64| {
        json!({"v": 1, "kind": "step-started", "issue": 7,
               "event": {"kind": "step-started", "step": name, "at": at}})
        .to_string()
    };
    // The rows of the gate stop at their bound; a step that starts after it
    // is still a step of the run.
    let mut stream = vec![step("develop", started * 1000)];
    stream.extend((0..=run::MAX_GATE_RUNS).map(|_| gate.clone()));
    stream.push(step("lens-review", (started + 30) * 1000));
    put(
        &home,
        "logs/work-7-261015-183834.jsonl",
        &format!("{}\n", stream.join("\n")),
    );
    let conn = record(&home, 7, ended);
    assert_eq!(
        one::<i64>(&conn, "SELECT count(*) FROM run_gate_runs"),
        run::MAX_GATE_RUNS as i64
    );
    assert_eq!(
        one::<i64>(
            &conn,
            "SELECT count(*) FROM run_steps WHERE step = 'lens-review'"
        ),
        1
    );
}

#[test]
fn a_run_whose_gate_named_no_time_records_no_gate() {
    let home = scratch("gate-zero");
    let started = secs("2026-10-15T18:38:34Z");
    let ended = secs("2026-10-15T18:39:50Z");
    cycle(
        &home,
        "headless-316134e",
        started,
        json!({"at": iso(ended), "started": started, "rc": 0, "status": "merged",
               "pr": "https://github.com/o/r/pull/3"}),
    );
    // The runner timed nothing: the rows are kept, and the run holds no figure,
    // which is what the NULLs of the two columns mean.
    let lines = json!({"v": 1, "kind": "verify-completed", "issue": 7,
                       "event": {"kind": "verify-completed", "step": "develop",
                                  "command": "scripts/verify.sh", "ok": true, "ms": 0,
                                  "at": ended * 1000}})
    .to_string();
    put(
        &home,
        "logs/work-7-261015-183834.jsonl",
        &format!("{lines}\n"),
    );
    let conn = record(&home, 7, ended);
    assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM run_gate_runs"), 1);
    assert_eq!(
        one::<Option<i64>>(&conn, "SELECT gate_runs FROM runs"),
        None
    );
    assert_eq!(one::<Option<i64>>(&conn, "SELECT gate_ms FROM runs"), None);
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
        &[
            line(t, "approved"),
            line(t + 1000, "reply"),
            line(t + 2000, "millwright"),
        ],
    );
    fact(
        &home,
        9,
        "approval",
        json!({"at": iso(t + 1900), "how": "millwright"}),
    );
    let conn = record(&home, 9, t + 3000);
    assert_eq!(
        one::<String>(
            &conn,
            "SELECT attempt || ' ' || how || ' ' || queue_wait FROM runs"
        ),
        "3 millwright 100"
    );
    // Recorded again, it replaces itself.
    execute(&home, &Cmd::Run(9, given(&home, 9)), t + 3000).unwrap();
    assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM runs"), 1);
}

#[test]
fn every_outcome_of_a_run_is_an_outcome_word() {
    use tehas_core::outcome::{OUTCOMES, STATUSES};
    let statuses = STATUSES.iter().map(|s| Some(s.to_string())).chain([None]);
    for status in statuses {
        for exit_code in [None, Some(0), Some(3), Some(30)] {
            for pr in [None, Some(4)] {
                for kind in [None, Some("cap"), Some("model")] {
                    for parked in [false, true] {
                        let r = run::Run {
                            status: status.clone(),
                            exit_code,
                            pr,
                            failure_kind: kind.map(str::to_string),
                            ..run::Run::default()
                        };
                        let o = tehas_core::outcome::end(&r.line(parked));
                        assert!(OUTCOMES.contains(&o), "{o} from {status:?}");
                    }
                }
            }
        }
    }
}

#[test]
fn outcomes_follow_how_the_cycle_ended() {
    let home = scratch("outcomes");
    let t = 1_792_000_000;
    let cases = [
        (
            json!({"status": "merged", "rc": 0, "pr": "https://github.com/o/r/pull/41"}),
            "merged",
            Some(41),
        ),
        (
            json!({"status": "handoff", "rc": 10, "pr": "https://github.com/o/r/pull/42", "parked": false}),
            "offered",
            Some(42),
        ),
        (
            json!({"status": "handoff", "rc": 10, "pr": "https://github.com/o/r/pull/43", "parked": true}),
            "handoff",
            Some(43),
        ),
        (json!({"status": "closed", "rc": 10}), "stopped", None),
        (
            json!({"status": "unknown", "rc": 4, "parked": true}),
            "refused",
            None,
        ),
        (
            json!({"status": "aborted", "rc": 20, "cap": "deadline", "parked": true}),
            "failed",
            None,
        ),
        (
            json!({"status": "handoff", "rc": 10, "failure_kind": "provider", "provider": true, "parked": true}),
            "handoff",
            None,
        ),
        (
            json!({"status": "parked", "rc": 10, "cap": "intent-park", "failure_kind": "cap", "parked": true}),
            "parked",
            None,
        ),
        // The four ends that moved in 2.0.
        (json!({"status": "stopped", "rc": 143}), "stopped", None),
        (json!({"status": "lost", "rc": 10}), "failed", None),
        (
            json!({"status": "merged", "rc": 0, "cap": "merged-claimed", "parked": true}),
            "parked",
            None,
        ),
        (
            json!({"status": "interrupted", "rc": 21, "parked": true}),
            "parked",
            None,
        ),
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
    let path = h(&home).cycle_note(7, "result.json");
    let mut r: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    let long = "x".repeat(400);
    r["issues"][0]["failure"] = json!({"kind": "provider", "step": "implement",
        "cause": format!("connect {}:8080 refused token=ghp_abcdefghijklmnopqrstuvwxyz123456\n{long}", ip([192, 168, 1, 20]))});
    fs::write(&path, r.to_string()).unwrap();
    let conn = record(&home, 7, started + 60);
    let cause: String = one(&conn, "SELECT failure_cause FROM runs");
    assert!(
        cause.starts_with("connect [private address]:8080 refused token=***"),
        "{cause}"
    );
    assert_eq!(cause.chars().count(), 200);
    assert_eq!(
        one::<String>(&conn, "SELECT failure_kind FROM runs"),
        "provider"
    );
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
    let path = h(&home).cycle_note(7, "result.json");
    let mut r: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    r["issues"][0]["failure"] = json!({"kind": "provider", "cause": "a long raw text"});
    fs::write(&path, r.to_string()).unwrap();
    let conn = record(&home, 7, started + 60);
    assert_eq!(
        one::<String>(&conn, "SELECT failure_cause FROM runs"),
        "ECONNREFUSED"
    );
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
    let now = 1_792_000_000;
    let code = |home: &Path, given: Given| execute(home, &Cmd::Run(5, given), now).unwrap_err().0;
    // No facts given, and a facts file that is not there.
    assert_eq!(code(&home, Given::default()), 65);
    assert_eq!(code(&home, given(&home, 5)), 65);
    // Facts with no history line, or one with no start.
    history(&home, 5, &[]);
    assert_eq!(code(&home, given(&home, 5)), 65);
    history(&home, 5, &[json!({"at": "2026-10-18T10:16:39Z", "rc": 0})]);
    assert_eq!(code(&home, given(&home, 5)), 65);
    // Facts that are not an object, or not JSON.
    for text in ["[1, 2]", "{\"line\":", ""] {
        fs::write(facts_path(&home, 5), text).unwrap();
        assert_eq!(code(&home, given(&home, 5)), 65, "{text}");
    }
    assert!(!db::path(&home).exists());
}

#[test]
fn facts_are_read_as_a_claim_never_through_a_link_and_bounded() {
    let home = scratch("factslink");
    let now = 1_792_000_000;
    history(
        &home,
        5,
        &[json!({"at": iso(now - 60), "started": now - 600, "rc": 0, "status": "merged"})],
    );
    // A link to good facts is not read.
    let link = home.join("tmp/link.json");
    std::os::unix::fs::symlink(facts_path(&home, 5), &link).unwrap();
    let through = Given {
        facts: Some(link),
        ..Given::default()
    };
    assert_eq!(
        execute(&home, &Cmd::Run(5, through), now).unwrap_err().0,
        65
    );
    // Nor is a directory, or a file over the bound.
    let dir = Given {
        facts: Some(home.join("tmp")),
        ..Given::default()
    };
    assert_eq!(execute(&home, &Cmd::Run(5, dir), now).unwrap_err().0, 65);
    let mut big = fs::read_to_string(facts_path(&home, 5)).unwrap();
    big.push_str(&" ".repeat(16 << 20));
    fs::write(home.join("tmp/big.json"), big).unwrap();
    let over = Given {
        facts: Some(home.join("tmp/big.json")),
        ..Given::default()
    };
    assert_eq!(execute(&home, &Cmd::Run(5, over), now).unwrap_err().0, 65);
    assert!(!db::path(&home).exists());
    // The facts themselves are recorded.
    record(&home, 5, now);
}

#[test]
fn the_run_takes_its_history_approval_settings_and_commit_from_the_facts_alone() {
    let home = scratch("factsonly");
    let started = 1_792_000_000;
    // The files these facts once came from, saying otherwise: none is read.
    put_at(
        h(&home).cycle_note(7, "history.jsonl"),
        &format!(
            "{}\n",
            json!({"at": iso(started + 9), "started": started - 5000, "rc": 1, "status": "failed"})
        ),
    );
    put_at(
        h(&home).cycle_note(7, "approved.json"),
        &json!({"at": iso(started - 9000), "how": "millwright"}).to_string(),
    );
    put_at(
        h(&home).cycle_note(7, "settings"),
        &json!({"container": {"PI_ENSEMBLE_SUBAGENT_MODEL": "file-model"}, "concurrentCycles": 9})
            .to_string(),
    );
    put_at(h(&home).cycle_note(7, "rukas"), "fedcba9\n");
    put_at(
        h(&home).cycle_note(7, "progress.json"),
        r#"{"roles":[{"role":"ops","dispatches":1}],"reviewHeld":false}"#,
    );
    history(
        &home,
        7,
        &[
            json!({"at": iso(started + 60), "started": started, "rc": 0, "status": "merged",
                 "how": "approved"}),
        ],
    );
    fact(&home, 7, "attempt", json!(4));
    fact(
        &home,
        7,
        "approval",
        json!({"at": iso(started - 30), "how": "reply"}),
    );
    fact(
        &home,
        7,
        "settings",
        json!({"container": {"PI_ENSEMBLE_SUBAGENT_MODEL": "facts-model"}, "host": {},
               "concurrentCycles": 3}),
    );
    fact(&home, 7, "rukas", json!(" 1234abc \n"));
    let conn = record(&home, 7, started + 100);
    assert_eq!(
        one::<String>(
            &conn,
            "SELECT id || ' ' || attempt || ' ' || how || ' ' || queue_wait || ' ' || status || ' ' ||
               seconds || ' ' || concurrent || ' ' || rukas_commit FROM runs"
        ),
        format!("work-7-{started} 4 reply 30 merged 60 2 1234abc")
    );
    assert_eq!(
        one::<String>(&conn, "SELECT role || ' ' || model FROM run_roles"),
        "ops facts-model"
    );
    // No approval, settings or commit on record: their columns are NULL.
    let home = scratch("factsnull");
    history(
        &home,
        7,
        &[
            json!({"at": iso(started + 60), "started": started, "rc": 0, "status": "merged",
                 "how": "approved"}),
        ],
    );
    fact(&home, 7, "rukas", json!("not hex"));
    let conn = record(&home, 7, started + 100);
    assert_eq!(
        one::<String>(
            &conn,
            "SELECT how || ' ' || coalesce(queue_wait, '-') || ' ' || coalesce(concurrent, '-') ||
               ' ' || coalesce(rukas_commit, '-') || ' ' || attempt FROM runs"
        ),
        "approved - - - 1"
    );
}

#[test]
fn prune_removes_rows_older_than_365_days_and_keeps_a_running_run_s_events() {
    let home = scratch("prune");
    let now = 1_792_000_000;
    let old = now - 366 * DAY;
    let kept = now - 300 * DAY;
    let (conn, _) = db::open(&db::path(&home), now).unwrap();
    for (id, issue, ended) in [("old", 1, old), ("new", 2, kept)] {
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
        conn.execute(
            "INSERT INTO run_roles (run_id, role) VALUES (?1, 'ops')",
            [id],
        )
        .unwrap();
    }
    // Issue 1 ended long ago; issue 3 started a year and a day ago and runs still.
    for (issue, event, at) in [
        (1, "ready", old),
        (1, "started", old),
        (1, "ended", old),
        (3, "ready", old),
        (3, "started", old),
        // Started a day and more before the cutoff, never ended: no cycle
        // runs that long, so it goes like any other.
        (6, "ready", now - 367 * DAY),
        (6, "started", now - 366 * DAY - 1),
        (2, "ready", kept),
    ] {
        db::event(&conn, issue, event, at).unwrap();
    }
    conn.execute("INSERT INTO samples (at) VALUES (?1), (?2)", [old, kept])
        .unwrap();
    execute(&home, &Cmd::Prune, now).unwrap();
    assert_eq!(
        one::<String>(&conn, "SELECT group_concat(id) FROM runs"),
        "new"
    );
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
        one::<String>(
            &conn,
            "SELECT group_concat(issue || ':' || event, ' ') FROM issue_events"
        ),
        "2:ready 3:ended"
    );
}

#[test]
fn prune_brings_the_retention_of_an_older_database_up_to_date() {
    let home = scratch("meta-retention");
    let now = 1_792_000_000;
    let (conn, _) = db::open(&db::path(&home), now).unwrap();
    conn.execute(
        "UPDATE meta SET value = '10' WHERE key = 'retention_days'",
        [],
    )
    .unwrap();
    execute(&home, &Cmd::Prune, now).unwrap();
    assert_eq!(
        one::<String>(&conn, "SELECT value FROM meta WHERE key = 'retention_days'"),
        "365"
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
    assert_eq!(
        one::<String>(&late, "SELECT value FROM meta WHERE key = 'created'"),
        "1792000000"
    );
    assert!(!h(&home).state_dir().join("analytics.db.broken").exists());
}

#[test]
fn programs_that_open_a_new_database_at_once_all_get_it() {
    // The journal switch is where the openers contend, so the eight-opener
    // run is repeated rather than run once: one pass can miss the lock race.
    // A failure names every opener of its round: the step each was at, how
    // long it took, and the error, so a rare one says where the wait was.
    for round in 0..20 {
        let home = scratch(&format!("concurrent-{round}"));
        let file = db::path(&home);
        let openers: Vec<_> = (0..8)
            .map(|i| {
                let file = file.clone();
                std::thread::spawn(move || {
                    let start = std::time::Instant::now();
                    let ms = || start.elapsed().as_millis();
                    let (conn, moved) = db::open(&file, 1_792_000_000)
                        .map_err(|e| format!("opener {i}: open failed at {} ms: {e}", ms()))?;
                    let opened = ms();
                    db::event(&conn, i, "ready", 1_792_000_000).map_err(|e| {
                        format!("opener {i}: opened at {opened} ms, its first write failed at {} ms: {e}", ms())
                    })?;
                    Ok::<_, String>((format!("opener {i}: opened at {opened} ms, wrote at {} ms", ms()), moved))
                })
            })
            .collect();
        let ends: Vec<Result<_, String>> = openers.into_iter().map(|o| o.join().unwrap()).collect();
        if ends.iter().any(Result::is_err) {
            let lines: Vec<&str> = ends
                .iter()
                .map(|e| match e {
                    Ok((line, _)) => line.as_str(),
                    Err(line) => line.as_str(),
                })
                .collect();
            panic!("round {round}:\n{}", lines.join("\n"));
        }
        for (_, moved) in ends.into_iter().flatten() {
            assert!(moved.is_none(), "a valid database was moved aside");
        }
        assert!(!h(&home).state_dir().join("analytics.db.broken").exists());
        let conn = Connection::open(&file).unwrap();
        assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM issue_events"), 8);
        assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM meta"), 3);
    }
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
    assert_eq!(
        execute(&home, &Cmd::Event(2, "ready".into(), None), now)
            .unwrap_err()
            .0,
        70
    );
    assert!(fs::read(&file).unwrap() == before);
    assert!(!h(&home).state_dir().join("analytics.db.broken").exists());
    // A directory it may not write in, with the database in it.
    let home = scratch("readonly-dir");
    let file = db::path(&home);
    execute(&home, &Cmd::Event(1, "ready".into(), None), now).unwrap();
    let state = h(&home).state_dir();
    fs::set_permissions(&state, fs::Permissions::from_mode(0o500)).unwrap();
    let err = execute(&home, &Cmd::Event(2, "ready".into(), None), now);
    fs::set_permissions(&state, fs::Permissions::from_mode(0o700)).unwrap(); // a directory
    assert_eq!(err.unwrap_err().0, 70);
    assert!(file.is_file());
    assert!(!h(&home).state_dir().join("analytics.db.broken").exists());
}

#[test]
fn a_write_prunes_too() {
    let home = scratch("writeprune");
    let now = 1_792_000_000;
    execute(&home, &Cmd::Event(4, "ready".into(), None), now - 366 * DAY).unwrap();
    execute(&home, &Cmd::Event(5, "ready".into(), None), now).unwrap();
    let conn = Connection::open(db::path(&home)).unwrap();
    assert_eq!(
        one::<String>(&conn, "SELECT group_concat(issue) FROM issue_events"),
        "5"
    );
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
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let conn = Connection::open(&file).unwrap();
    assert_eq!(one::<i64>(&conn, "PRAGMA user_version"), db::SCHEMA_VERSION);
    assert_eq!(one::<String>(&conn, "PRAGMA journal_mode"), "wal");
    assert_eq!(
        one::<String>(
            &conn,
            "SELECT group_concat(key || '=' || value, ' ') FROM (SELECT * FROM meta ORDER BY key)"
        ),
        format!(
            "created=1792000000 retention_days=365 schema={}",
            db::SCHEMA_VERSION
        )
    );
}

#[test]
fn a_broken_file_is_moved_aside_and_a_new_database_made() {
    for (name, content) in [
        (
            "garbage",
            b"this is not a database at all, but it is long enough to be read as a header. "
                .repeat(20),
        ),
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
        let aside = fs::read(h(&home).state_dir().join("analytics.db.broken")).unwrap();
        assert!(
            aside == before,
            "{name}: the file moved aside is not the one found"
        );
        let conn = Connection::open(&file).unwrap();
        assert_eq!(
            one::<i64>(&conn, "PRAGMA user_version"),
            db::SCHEMA_VERSION,
            "{name}"
        );
        assert_eq!(
            one::<i64>(&conn, "SELECT count(*) FROM issue_events"),
            1,
            "{name}"
        );
    }
}

#[test]
fn a_database_of_another_schema_is_kept_aside_under_its_version() {
    let home = scratch("foreign");
    let file = db::path(&home);
    // Held open until the end, so the row is still in the -wal when the file
    // is moved: its close would checkpoint it into the file.
    let c = Connection::open(&file).unwrap();
    c.execute_batch(
        "PRAGMA journal_mode = WAL; PRAGMA wal_autocheckpoint = 0;
         CREATE TABLE runs (x); INSERT INTO runs VALUES (42); PRAGMA user_version = 99",
    )
    .unwrap();
    execute(&home, &Cmd::Event(1, "ready".into(), None), 1_792_000_000).unwrap();
    assert!(!h(&home).state_dir().join("analytics.db.broken").exists());
    let kept = Connection::open(h(&home).state_dir().join("analytics.db.schema-99")).unwrap();
    assert_eq!(one::<i64>(&kept, "PRAGMA user_version"), 99);
    assert_eq!(one::<i64>(&kept, "SELECT x FROM runs"), 42);
    let conn = Connection::open(&file).unwrap();
    assert_eq!(one::<i64>(&conn, "PRAGMA user_version"), db::SCHEMA_VERSION);
    assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM issue_events"), 1);
    // The moved file's last connection closes last.
    drop(conn);
    drop(kept);
    c.close().unwrap();
}

/// A file another program has made and not yet given a schema: what `settle`
/// meets while that program's `create` holds the write lock.
fn empty_database(file: &Path) -> Connection {
    let c = Connection::open(file).unwrap();
    c.execute_batch("PRAGMA journal_mode = WAL").unwrap();
    c
}

#[test]
fn a_write_lock_held_through_the_create_is_an_error_and_changes_nothing() {
    let home = scratch("create-busy");
    let file = db::path(&home);
    let now = 1_792_000_000;
    let holder = empty_database(&file);
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    let mut late = Connection::open(&file).unwrap();
    late.busy_timeout(std::time::Duration::ZERO).unwrap();
    assert!(matches!(
        db::settle(&mut late, 0, now),
        Err(db::Unusable::Busy(_))
    ));
    holder.execute_batch("COMMIT").unwrap();
    assert_eq!(one::<i64>(&late, "PRAGMA user_version"), 0);
    assert!(db::settle(&mut late, 0, now).is_ok());
    assert_eq!(one::<i64>(&late, "PRAGMA user_version"), db::SCHEMA_VERSION);
}

#[test]
fn a_write_lock_held_past_the_busy_timeout_is_waited_out() {
    let home = scratch("create-long-lock");
    let file = db::path(&home);
    let now = 1_792_000_000;
    let holder = empty_database(&file);
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    let release = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(6));
        holder.execute_batch("COMMIT").unwrap();
    });
    let (conn, moved) = db::open(&file, now).unwrap();
    release.join().unwrap();
    assert_eq!(moved, None);
    assert_eq!(one::<i64>(&conn, "PRAGMA user_version"), db::SCHEMA_VERSION);
}

/// v0.30.0 wrote schema 4 already, so a database of an older schema is no
/// 0.30 host's: it is kept aside under its version, as a newer one is.
#[test]
fn a_database_of_schema_1_to_3_or_9_is_kept_aside() {
    for v in [1, 2, 3, 9] {
        let home = scratch(&format!("schema-{v}"));
        let file = db::path(&home);
        {
            let c = Connection::open(&file).unwrap();
            c.execute_batch(&format!(
                "CREATE TABLE runs (x); INSERT INTO runs VALUES (42); PRAGMA user_version = {v}"
            ))
            .unwrap();
        }
        let (conn, moved) = db::open(&file, 1_792_000_000).unwrap();
        assert_eq!(
            moved,
            Some(
                h(&home)
                    .state_dir()
                    .join(format!("analytics.db.schema-{v}"))
            ),
            "schema {v} has no migration"
        );
        assert_eq!(one::<i64>(&conn, "PRAGMA user_version"), db::SCHEMA_VERSION);
    }
}

#[test]
fn a_run_stores_the_driver_its_approval_bound_and_nothing_else() {
    let home = scratch("driver");
    let started = secs("2026-10-18T10:16:39Z");
    for (issue, approval, want) in [
        (
            1,
            json!({"how": "approved", "driver": "lean"}),
            Some("lean"),
        ),
        (
            2,
            json!({"how": "approved", "driver": "classic"}),
            Some("classic"),
        ),
        (3, json!({"how": "approved"}), None),
        (4, json!({"how": "approved", "driver": null}), None),
        (5, json!({"how": "approved", "driver": "fast"}), None),
        (6, json!({"how": "approved", "driver": "LEAN"}), None),
        (7, json!({"how": "approved", "driver": "le an"}), None),
        (8, json!({"how": "approved", "driver": 1}), None),
        (9, Value::Null, None),
    ] {
        let line = json!({"at": "2026-10-18T10:20:00Z", "started": started, "rc": 0,
                          "status": "handoff"});
        history(&home, issue, &[line]);
        fact(&home, issue, "approval", approval);
        let conn = record(&home, issue, started + 300);
        assert_eq!(
            conn.query_row("SELECT driver FROM runs WHERE issue = ?1", [issue], |r| {
                r.get::<_, Option<String>>(0)
            })
            .unwrap()
            .as_deref(),
            want,
            "issue {issue}"
        );
    }
}

#[test]
fn prune_deletes_a_kept_schema_365_days_after_its_last_write() {
    let home = scratch("aside");
    let state = h(&home).state_dir();
    let now = 1_792_000_000;
    let at = |secs: i64| std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs as u64);
    for (name, written) in [
        ("analytics.db.schema-7", now - 366 * 86_400),
        ("analytics.db.schema-7-wal", now - 366 * 86_400),
        ("analytics.db.schema-8", now - 300 * 86_400),
        ("analytics.db.broken", now - 400 * 86_400),
        ("other.schema-1", now - 400 * 86_400),
    ] {
        fs::create_dir_all(&state).unwrap();
        let f = fs::File::create(state.join(name)).unwrap();
        f.set_modified(at(written)).unwrap();
    }
    execute(&home, &Cmd::Prune, now).unwrap();
    let mut left: Vec<String> = fs::read_dir(&state)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| {
            ![
                "analytics.db",
                "analytics.db-wal",
                "analytics.db-shm",
                "cycles",
            ]
            .contains(&n.as_str())
        })
        .collect();
    left.sort();
    assert_eq!(
        left,
        [
            "analytics.db.broken",
            "analytics.db.schema-8",
            "other.schema-1"
        ]
    );
}

#[test]
fn prune_aside_returns_the_expired_files_it_could_not_delete() {
    let home = scratch("aside-stuck");
    let state = h(&home).state_dir();
    fs::create_dir_all(&state).unwrap();
    let now = 1_792_000_000;
    let old = std::time::UNIX_EPOCH + std::time::Duration::from_secs((now - 366 * 86_400) as u64);
    for name in ["analytics.db.schema-7", "analytics.db.schema-7-wal"] {
        fs::File::create(state.join(name))
            .unwrap()
            .set_modified(old)
            .unwrap();
    }
    let file = db::path(&home);
    // Held open and read, so the database's -wal and -shm exist while the
    // directory is read-only.
    let (held, _) = db::open(&file, now).unwrap();
    let _: i64 = held
        .query_row("SELECT count(*) FROM runs", [], |r| r.get(0))
        .unwrap();
    fs::set_permissions(&state, fs::Permissions::from_mode(0o555)).unwrap();
    // Root writes into a read-only directory: the case cannot be made.
    let root = fs::File::create(state.join("probe")).is_ok();
    let mut stuck = db::prune_aside(&file, now);
    fs::set_permissions(&state, fs::Permissions::from_mode(0o755)).unwrap();
    if root {
        return;
    }
    stuck.sort();
    assert_eq!(
        stuck,
        [
            state.join("analytics.db.schema-7"),
            state.join("analytics.db.schema-7-wal")
        ]
    );
    assert!(state.join("analytics.db.schema-7").exists());
    assert!(state.join("analytics.db.schema-7-wal").exists());

    // Through the command: the rest of prune runs, then exit 73 naming them.
    fs::set_permissions(&state, fs::Permissions::from_mode(0o555)).unwrap();
    let got = execute(&home, &Cmd::Prune, now);
    fs::set_permissions(&state, fs::Permissions::from_mode(0o755)).unwrap();
    let (code, why) = got.unwrap_err();
    assert_eq!(code, 73, "{why}");
    assert!(why.starts_with("WARN "), "{why}");
    assert!(why.contains("analytics.db.schema-7-wal"), "{why}");
    assert!(!why.contains('\n'), "{why}");

    assert_eq!(db::prune_aside(&file, now), Vec::<PathBuf>::new());
    assert!(!state.join("analytics.db.schema-7").exists());
    assert!(!state.join("analytics.db.schema-7-wal").exists());
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
    let path = h(&home).cycle_note(7, "result.json");
    let mut r: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    r["issues"][0]["failure"] = json!({"kind": "provider",
        "cause": format!("GET https://user:hunter2@models.example/v1 from {} and {} with Bearer abcdefghijklmnopqrstuvwxyz; \
            connect ECONNREFUSED models.example:8000; git@host.tld:org/repo; \
            Incorrect API key provided: sk-abc123def456ghi789jkl0; dial fe80::1%eth0 and 2001:db8::7",
            ip([10, 0, 0, 7]), ip([203, 0, 113, 9]))});
    fs::write(&path, r.to_string()).unwrap();
    let now = started + 60;
    execute(&home, &Cmd::Run(7, given(&home, 7)), now).unwrap();
    execute(&home, &Cmd::Event(7, "ready".into(), None), now).unwrap();
    let out = home.join("copy.db");
    execute(&home, &Cmd::Copy(out.clone(), false), now).unwrap();
    assert_eq!(
        fs::metadata(&out).unwrap().permissions().mode() & 0o777,
        0o600
    );
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
        regex::Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b|(?i)[0-9a-f]+::[0-9a-f]+")
            .unwrap();
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
    assert_eq!(
        execute(&home, &Cmd::Copy(out.clone(), true), now)
            .unwrap_err()
            .0,
        73
    );
    // What the copy holds, read from the copy.
    assert_eq!(
        db::copy_summary(&out).unwrap(),
        format!(
            r#"{{"api":1,"schema":{},"from":{started},"to":{},"runs":1}}"#,
            db::SCHEMA_VERSION,
            started + 30
        )
    );
}

#[test]
fn iso_times_are_read_as_utc() {
    assert_eq!(iso_ms("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(iso_ms("2026-10-15T18:38:44.024Z"), Some(1_792_089_524_024));
    assert_eq!(iso_ms("2000-03-01T00:00:01.5Z"), Some(951_868_801_500));
    assert_eq!(iso_ms("2026-10-15 18:38:44"), None);
    assert_eq!(iso_ms("2026-13-15T18:38:44Z"), None);
    // The year is bounded here, before the arithmetic that would overflow on a
    // large one: a ledger's stamp is read only within these two years, and
    // the sessions' `ledger_at` bounds it further.
    assert_eq!(iso_ms("1969-12-31T23:59:59Z"), None);
    assert_eq!(iso_ms("2101-01-01T00:00:00Z"), None);
    assert_eq!(iso_ms("71167994112-01-01T00:00:00Z"), None);
    assert_eq!(iso_ms("2262-01-01T00:00:00Z"), None);
    assert_eq!(iso_ms("9999-12-31T23:59:59Z"), None);
    assert_eq!(iso_ms("2100-12-31T23:59:59Z"), Some(4_133_980_799_000));
}

#[test]
fn the_arguments_are_checked() {
    let args = |s: &str| s.split(' ').map(str::to_string).collect::<Vec<_>>();
    let facts = || Some(PathBuf::from("/f.json"));
    assert_eq!(
        parse(&args("run 12 --facts /f.json")),
        Some(Cmd::Run(
            12,
            Given {
                facts: facts(),
                ..Given::default()
            }
        ))
    );
    let image = format!("sha256:{}", "0f".repeat(32));
    assert_eq!(
        parse(&args(&format!(
            "run 12 --facts /f.json --oom 0 --image {image} --forks 3 --rukas-image 1234abc"
        ))),
        Some(Cmd::Run(
            12,
            Given {
                image: Some(image.clone()),
                rukas_image: Some("1234abc".into()),
                image_built: None,
                oom: Some(0),
                forks: Some(3),
                facts: facts(),
            }
        ))
    );
    for bad in [
        "run 12".to_string(),
        "run 12 --oom 0".into(),
        "run 12 --facts".into(),
        "run 12 --facts /f.json --facts /g.json".into(),
        "run 12 --facts /f.json --oom".into(),
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
    assert_eq!(
        parse(&args("event 12 merged")),
        Some(Cmd::Event(12, "merged".into(), None))
    );
    assert_eq!(parse(&args("prune")), Some(Cmd::Prune));
    assert_eq!(parse(&args("sample")), Some(Cmd::Sample));
    assert_eq!(
        parse(&args("event 12 ready --at 1792000000")),
        Some(Cmd::Event(12, "ready".into(), Some(1_792_000_000)))
    );
    assert_eq!(
        parse(&args("event 12 ready --at 0")),
        Some(Cmd::Event(12, "ready".into(), Some(0)))
    );
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
    assert_eq!(
        parse(&args("copy --to /tmp/x.db")),
        Some(Cmd::Copy("/tmp/x.db".into(), false))
    );
    for json in ["copy --to /tmp/x.db --json", "copy --json --to /tmp/x.db"] {
        assert_eq!(
            parse(&args(json)),
            Some(Cmd::Copy("/tmp/x.db".into(), true)),
            "{json}"
        );
    }
    for bad in [
        "run",
        "run 0",
        "run 012",
        "run -1",
        "run x",
        "event 12 shipped",
        "prune now",
        "sample now",
        "report",
        "report --json",
        "export /tmp/x.db",
        "export /tmp/x.db --json",
        "copy",
        "copy x.db",
        "copy --to",
        "copy --json",
        "copy --to -x.db",
        "copy --to x.db --yaml",
        "copy --to x.db --json --json",
        "copy --to x.db --json now",
    ] {
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
        .prepare(
            "SELECT type, name, sql FROM sqlite_master WHERE sql IS NOT NULL ORDER BY type, name",
        )
        .unwrap();
    let rows = q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
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
                "the schema changed and user_version did not: a column added within the \
                 version goes last in its table, anything else raises \
                 SCHEMA_VERSION in db.rs; then replace src/schema.pinned with:\n{dump}"
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
    let result = h(&home).cycle_note(7, "result.json");
    let moved = home.join("result.json");
    fs::rename(&result, &moved).unwrap();
    std::os::unix::fs::symlink(&moved, &result).unwrap();
    let r = run::read(&home, 7, &given(&home, 7)).unwrap();
    assert_eq!(r.result_schema, None);
    assert_eq!(r.runner_ms, None);
}

// The record the driver wrote in its workspace, which the cycle keeps beside the
// run: the driver's own steps, which stand for a lean run's in place of the
// stream's.
#[test]
fn a_kept_lean_record_gives_a_lean_run_its_steps() {
    let home = scratch("lean-record");
    let started = secs("2026-10-18T10:16:39Z");
    cycle(
        &home,
        "headless-316134e",
        started,
        json!({"at": iso(started + 30), "started": started, "rc": 0, "status": "merged"}),
    );
    let steps = |home: &Path, v: Value| {
        put_at(
            h(home).cycle_note(7, "lean-state.json"),
            &json!({"schema": 1, "issue": 7, "steps": v}).to_string(),
        );
    };
    let names = |r: &run::Run| -> Vec<(String, i64)> {
        r.steps.iter().map(|s| (s.step.clone(), s.round)).collect()
    };
    steps(
        &home,
        json!([{"state": "ready", "startedAt": 1}, {"state": "verify"},
               {"state": "fix"}, {"state": "verify"}, {"state": 3}, "verify"]),
    );
    // A classic run takes nothing of the file, whatever it holds: its steps
    // are the stream's.
    let r = run::read(&home, 7, &given(&home, 7)).unwrap();
    let stream = names(&r);
    assert_eq!(stream[0].0, "explore", "{stream:?}");
    assert!(stream.iter().all(|(s, _)| s != "verify"), "{stream:?}");
    // The approval bound the lean driver, which is what makes a record read. A
    // step run twice is its second round; an entry of another shape is none.
    fact(
        &home,
        7,
        "approval",
        json!({"at": iso(started - 90), "how": "approved", "driver": "lean"}),
    );
    let r = run::read(&home, 7, &given(&home, 7)).unwrap();
    assert_eq!(
        names(&r),
        [
            ("ready".to_string(), 1),
            ("verify".to_string(), 1),
            ("fix".to_string(), 1),
            ("verify".to_string(), 2)
        ]
    );
    // The bound keeps the first MAX_LEAN_STEPS of a record that holds more.
    let many: Vec<Value> = (0..run::MAX_LEAN_STEPS + 5)
        .map(|i| json!({"state": format!("s{i}")}))
        .collect();
    steps(&home, Value::Array(many));
    let r = run::read(&home, 7, &given(&home, 7)).unwrap();
    assert_eq!(r.steps.len(), run::MAX_LEAN_STEPS);
    assert_eq!(r.steps[0].step, "s0");
    // No step it can read, no JSON, or no file at all: the stream's steps.
    for body in [
        r#"{"steps": [{"state": 3}]}"#,
        r#"{"steps": 1}"#,
        "{not json",
    ] {
        put_at(h(&home).cycle_note(7, "lean-state.json"), body);
        let r = run::read(&home, 7, &given(&home, 7)).unwrap();
        assert_eq!(names(&r), stream, "{body}");
    }
    fs::remove_file(h(&home).cycle_note(7, "lean-state.json")).unwrap();
    let r = run::read(&home, 7, &given(&home, 7)).unwrap();
    assert_eq!(names(&r), stream);
}

// The live files of one poll, as tehasbot status and tehas-dispatch write
// them, at `at`, and the energy file a 1.4 host left, which no sample reads.
fn live(home: &Path, at: i64, why: &str, limit_room: Option<i64>) {
    fs::create_dir_all(h(home).live_dir()).unwrap();
    let issue = |n: i64| json!({"number": n, "title": "t", "url": "u"});
    put_at(
        h(home).live(Live::Status),
        &json!({"generatedAt": iso(at), "repo": "acme/widget", "paused": true,
                "running": [issue(1), issue(2)], "queued": [issue(3), issue(4), issue(5)],
                "awaiting": [issue(6)], "stopped": [], "planning": 2, "completed": [issue(9)]})
        .to_string(),
    );
    let mut d = json!({"time": iso(at), "why": why, "hold": "holding: ...", "count": 2,
                       "room": 1, "committed": 0, "max": 0, "reserve": "2g", "free": 3,
                       "awaitRoom": null});
    if let Some(r) = limit_room {
        d["limitRoom"] = json!(r);
    }
    put_at(h(home).live(Live::Dispatch), &d.to_string());
}

fn sample_row(home: &Path) -> String {
    let conn = Connection::open(db::path(home)).unwrap();
    one::<String>(
        &conn,
        "SELECT group_concat(row, ' | ') FROM (SELECT at || ' ' || coalesce(in_line, '-') || ' ' ||
           coalesce(running, '-') || ' ' || coalesce(awaiting, '-') || ' ' || coalesce(stopped, '-') || ' ' ||
           coalesce(planning, '-') || ' ' || coalesce(cycle_limit, '-') || ' ' || coalesce(paused, '-') || ' ' ||
           coalesce('[' || held || ']', '-') AS row FROM samples ORDER BY at)",
    )
}

#[test]
fn a_sample_reads_the_live_files_of_the_poll() {
    let home = scratch("sample");
    let now = 1_792_000_000;
    live(&home, now - 30, "awaiting", None);
    execute(&home, &Cmd::Sample, now).unwrap();
    // Five slots: two running and three free.
    assert_eq!(sample_row(&home), format!("{now} 3 2 1 0 2 5 1 [awaiting]"));

    // The limit of tehas limit allows fewer; nothing holds.
    live(&home, now + 90, "", Some(1));
    execute(&home, &Cmd::Sample, now + 100).unwrap();
    assert!(sample_row(&home).ends_with(&format!(" | {} 3 2 1 0 2 3 1 []", now + 100)));
}

#[test]
fn a_sample_reads_the_report_the_dispatcher_writes() {
    // The dispatcher's own type, taken through to_value and stamped with the
    // poll's time: the bytes tehas-dispatch publishes. The sampler reads its
    // held word and its cycle limit from those bytes by the core's key names.
    let home = scratch("sample-written");
    let now = 1_792_000_000;
    live(&home, now - 30, "awaiting", None);
    let r = tehas_core::hold::Report {
        why: Some(tehas_core::hold::Why::Limit),
        hold: "holding: the limit allows 3 cycles at once and 2 are running".into(),
        count: 2,
        room: 4,
        committed: 0,
        max: 0,
        reserve: "2g".into(),
        free: 5,
        await_room: None,
        limit_room: Some(1),
        refresh_room: Some(2),
        endpoint: Some("backup".into()),
        held: Vec::new(),
    };
    let mut v = r.to_value();
    v[tehas_core::hold::KEY_TIME] = json!(iso(now - 20));
    put_at(h(&home).live(Live::Dispatch), &v.to_string());
    execute(&home, &Cmd::Sample, now).unwrap();
    // The limit of tehas limit is the lower ceiling: count plus limitRoom,
    // three, though count plus free would give seven.
    assert_eq!(sample_row(&home), format!("{now} 3 2 1 0 2 3 1 [limit]"));
}

#[test]
fn a_live_file_that_is_missing_stale_a_link_or_malformed_gives_null_columns() {
    let now = 1_792_000_000;
    let nothing = |at: i64| format!("{at} - - - - - - - -");

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
    put_at(
        h(&home).live(Live::Dispatch),
        &json!({"time": iso(now - 200), "why": "count", "count": 1, "free": 0}).to_string(),
    );
    execute(&home, &Cmd::Sample, now + 1).unwrap();
    assert!(sample_row(&home).ends_with(&format!(" | {} 3 2 1 0 2 - 1 -", now + 1)));

    let home = scratch("sample-links");
    live(&home, now - 30, "count", None);
    for view in [Live::Status, Live::Dispatch] {
        let p = h(&home).live(view);
        let moved = home.join(p.file_name().unwrap());
        fs::rename(&p, &moved).unwrap();
        std::os::unix::fs::symlink(&moved, &p).unwrap();
    }
    execute(&home, &Cmd::Sample, now).unwrap();
    assert_eq!(sample_row(&home), nothing(now));

    let home = scratch("sample-bad");
    fs::create_dir_all(h(&home).live_dir()).unwrap();
    put_at(h(&home).live(Live::Status), "not json");
    put_at(
        h(&home).live(Live::Dispatch),
        &json!({"time": "yesterday", "why": "count", "count": 1, "free": 0}).to_string(),
    );
    execute(&home, &Cmd::Sample, now).unwrap();
    assert_eq!(sample_row(&home), nothing(now));

    // Fields of the wrong type, and a hold that is not one word.
    let home = scratch("sample-types");
    fs::create_dir_all(h(&home).live_dir()).unwrap();
    put_at(
        h(&home).live(Live::Status),
        &json!({"generatedAt": iso(now), "paused": "no", "queued": 3, "running": [], "awaiting": {}, "stopped": null,
                "planning": "2"})
            .to_string(),
    );
    put_at(
        h(&home).live(Live::Dispatch),
        &json!({"time": iso(now), "why": "holding: <b>", "count": "2", "free": 1}).to_string(),
    );
    execute(&home, &Cmd::Sample, now).unwrap();
    assert_eq!(sample_row(&home), format!("{now} - 0 - - - - - -"));
}

// The sampler finds each list under the name the writer gives it.
#[test]
fn the_sampler_reads_the_lists_by_the_names_status_json_writes() {
    use tehas_core::queue_lists::{self, AWAITING, QUEUED, RUNNING, STOPPED};
    let home = scratch("sample-lists");
    let now = 1_792_000_000;
    let mut st = json!({"generatedAt": iso(now)});
    for (i, name) in queue_lists::LISTS.iter().enumerate() {
        st[*name] = json!(vec![json!({"number": 1}); i + 1]);
    }
    fs::create_dir_all(h(&home).live_dir()).unwrap();
    put_at(h(&home).live(Live::Status), &st.to_string());
    let s = crate::sample::read(&home, now);
    let at = |k: &str| queue_lists::LISTS.iter().position(|n| *n == k).unwrap() as i64 + 1;
    assert_eq!(s.in_line, Some(at(QUEUED)));
    assert_eq!(s.running, Some(at(RUNNING)));
    assert_eq!(s.awaiting, Some(at(AWAITING)));
    assert_eq!(s.stopped, Some(at(STOPPED)));
}

// A status.json written before it counted planning has no such field.
#[test]
fn planning_is_the_count_status_json_gives_and_null_without_one() {
    let home = scratch("sample-planning");
    let now = 1_792_000_000;
    let status = |planning: Option<Value>| {
        let mut s = json!({"generatedAt": iso(now), "paused": false, "queued": [],
                           "running": [], "awaiting": [], "stopped": []});
        if let Some(p) = planning {
            s["planning"] = p;
        }
        put_at(h(&home).live(Live::Status), &s.to_string());
    };
    fs::create_dir_all(h(&home).live_dir()).unwrap();
    for (at, planning) in [
        (now, Some(json!(0))),
        (now + 1, Some(json!(4))),
        (now + 2, None),
        (now + 3, Some(json!(-1))),
        (now + 4, Some(json!(1.5))),
    ] {
        status(planning);
        execute(&home, &Cmd::Sample, at).unwrap();
    }
    let conn = Connection::open(db::path(&home)).unwrap();
    assert_eq!(
        one::<String>(
            &conn,
            "SELECT group_concat(coalesce(planning, '-'), ' ') FROM (SELECT planning FROM samples ORDER BY at)"
        ),
        "0 4 - - -"
    );
}

#[test]
fn a_sample_prunes_too_and_two_in_one_second_keep_the_last() {
    let home = scratch("sample-prune");
    let now = 1_792_000_000;
    execute(&home, &Cmd::Event(4, "ready".into(), None), now - 366 * DAY).unwrap();
    execute(&home, &Cmd::Sample, now - 366 * DAY).unwrap();
    live(&home, now, "count", None);
    execute(&home, &Cmd::Sample, now).unwrap();
    live(&home, now, "", None);
    execute(&home, &Cmd::Sample, now).unwrap();
    assert_eq!(sample_row(&home), format!("{now} 3 2 1 0 2 5 1 []"));
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
    execute(
        &home,
        &Cmd::Event(10, "ready".into(), Some(now + AT_AHEAD)),
        now,
    )
    .unwrap();
    let err = execute(
        &home,
        &Cmd::Event(11, "ready".into(), Some(now + AT_AHEAD + 1)),
        now,
    )
    .unwrap_err();
    assert_eq!(err.0, 64);
    let conn = Connection::open(db::path(&home)).unwrap();
    assert_eq!(
        one::<String>(
            &conn,
            "SELECT group_concat(issue || ':' || event || ':' || (at - 1792000000), ' ')
             FROM (SELECT * FROM issue_events ORDER BY issue, event)"
        ),
        "8:ready:-3600 8:started:0 9:ready:0 10:ready:0"
    );
}

// The clamp: an --at ahead of now within AT_AHEAD is recorded at now and said
// on standard error, and one beyond it is refused with the row left out. A
// sibling of the same issue in the past keeps its own seconds: the clamp is
// conditional on the sign, not on how near now+AT_AHEAD the value is.
#[test]
fn an_event_ahead_of_now_is_clamped_to_now_and_the_clamp_is_noted() {
    let home = scratch("event-at-clamp");
    let now = 1_792_000_000;
    // A few minutes ahead of now, beside one a few minutes behind it.
    execute(&home, &Cmd::Event(4, "ready".into(), Some(now + 240)), now).unwrap();
    execute(&home, &Cmd::Event(4, "closed".into(), Some(now - 240)), now).unwrap();
    let conn = Connection::open(db::path(&home)).unwrap();
    assert_eq!(
        one_at::<String>(
            &conn,
            "SELECT group_concat(event || ':' || (at - ?1), ' ')
             FROM (SELECT * FROM issue_events ORDER BY event)",
            now,
        ),
        "closed:-240 ready:0"
    );
    // One ahead by less than a minute, which the old rule accepted: the row
    // is kept too, and clamped.
    execute(&home, &Cmd::Event(4, "started".into(), Some(now + 30)), now).unwrap();
    assert_eq!(
        one_at::<String>(
            &conn,
            "SELECT event || ':' || (at - ?1) FROM issue_events WHERE issue = 4
             AND event = 'started'",
            now,
        ),
        "started:0"
    );
    // The whole window is a clamp and the second past it is a refusal whose
    // message names the window, and keeps no row.
    execute(
        &home,
        &Cmd::Event(5, "ready".into(), Some(now + AT_AHEAD)),
        now,
    )
    .unwrap();
    let err = execute(
        &home,
        &Cmd::Event(5, "merged".into(), Some(now + AT_AHEAD + 1)),
        now,
    )
    .unwrap_err();
    assert_eq!(err.0, 64);
    assert!(
        err.1
            .contains(&format!("more than {AT_AHEAD}s ahead of now")),
        "the refusal names the window: {}",
        err.1
    );
    assert_eq!(
        one_at::<String>(
            &conn,
            "SELECT event || ':' || (at - ?1) FROM issue_events WHERE issue = 5",
            now
        ),
        "ready:0"
    );
    assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM issue_events"), 4);
}

// A time that needs no clamp keeps its own seconds, and the whole window up
// to AT_AHEAD is a clamp. A sibling of one issue at the same distance either
// side of now is what tells clamp from keep.
#[test]
fn an_event_at_now_or_in_the_past_is_not_clamped_and_says_nothing() {
    let home = scratch("event-at-kept");
    let now = 1_792_000_000;
    execute(&home, &Cmd::Event(6, "ready".into(), Some(now)), now).unwrap();
    execute(&home, &Cmd::Event(6, "started".into(), Some(now - 1)), now).unwrap();
    execute(
        &home,
        &Cmd::Event(7, "ready".into(), Some(now - AT_AHEAD)),
        now,
    )
    .unwrap();
    execute(
        &home,
        &Cmd::Event(7, "closed".into(), Some(now + AT_AHEAD)),
        now,
    )
    .unwrap();
    let conn = Connection::open(db::path(&home)).unwrap();
    assert_eq!(
        one_at::<String>(
            &conn,
            "SELECT group_concat(issue || ':' || event || ':' || (at - ?1), ' ')
             FROM (SELECT * FROM issue_events ORDER BY issue, event)",
            now,
        ),
        "6:ready:0 6:started:-1 7:closed:0 7:ready:-3600"
    );
}

// A `now` that is no clock at all (tehas_core's now returns 0 for a clock
// before 1970) judges nothing: every --at is stored as its caller asked
// rather than refused, which is the loss the clamp exists to prevent.
#[test]
fn an_event_with_no_clock_keeps_the_time_given_rather_than_being_refused() {
    let home = scratch("event-at-no-clock");
    for (issue, wanted) in [
        (1, 0),
        (2, 1_792_003_601),
        (3, 1_792_000_000 - AT_AHEAD - 1),
    ] {
        for now in [0, -1] {
            execute(&home, &Cmd::Event(issue, "ready".into(), Some(wanted)), now).unwrap();
        }
    }
    let conn = Connection::open(db::path(&home)).unwrap();
    assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM issue_events"), 3);
    assert_eq!(
        one::<String>(
            &conn,
            "SELECT group_concat(issue || ':' || at, ' ')
             FROM (SELECT * FROM issue_events ORDER BY issue)"
        ),
        "1:0 2:1792003601 3:1791996399"
    );
}

// An --at far behind now is stored as given: the bound is on the ahead side
// only. The far row is older than prune's cutoff, so it is gone by the time
// the near row of the same call is written, which is what the counts say.
#[test]
fn an_event_far_in_the_past_is_stored_as_given() {
    let home = scratch("event-at-past");
    let now = 1_792_000_000;
    let far = now - db::RETENTION_DAYS * 86_400 - 1;
    execute(&home, &Cmd::Event(12, "ready".into(), Some(far)), now).unwrap();
    execute(
        &home,
        &Cmd::Event(13, "ready".into(), Some(now + AT_AHEAD)),
        now,
    )
    .unwrap();
    let conn = Connection::open(db::path(&home)).unwrap();
    assert_eq!(
        one_at::<String>(
            &conn,
            "SELECT group_concat(issue || ':' || (at - ?1), ' ') FROM (SELECT * FROM issue_events WHERE issue = 13 ORDER BY issue)",
            now,
        ),
        "13:0"
    );
    assert_eq!(
        one::<i64>(&conn, "SELECT count(*) FROM issue_events WHERE issue = 12"),
        0
    );
}

// A planning turn of issue 7 in flight: its record as the poll leaves
// it when the turn has ended, as facts, and what `tehas plan` wrote beside it.
fn turn(home: &Path, record: Value, out: Option<Value>) {
    fs::write(facts_path(home, 7), record.to_string()).unwrap();
    if let Some(o) = out {
        put_at(h(home).plan_file(7, ".out"), &o.to_string());
    }
    put(home, "docs/version.txt", "v0.7.22+1234abc\n");
}

fn plan_cmd(home: &Path, rc: &str) -> Cmd {
    Cmd::Plan(
        7,
        crate::plan::Given {
            rc: rc.into(),
            rukas_image: Some("abcdef1".into()),
            image: None,
            image_built: None,
            facts: Some(facts_path(home, 7)),
        },
    )
}

#[test]
fn plan_stores_the_turn_with_its_times_turn_and_outcome_and_no_usage() {
    let home = scratch("plan");
    let now = 1_792_000_000;
    let started = now - 300;
    turn(
        &home,
        json!({"session": "s1", "turns": 1, "options": [], "pending": false, "failed": false,
               "running": true, "startedAt": iso(started), "by": "alice",
               "answers": [{"turn": 2, "by": "alice", "millwright": false}]}),
        Some(json!({"session": "s1", "text": "Which one?", "question": null})),
    );
    execute(&home, &plan_cmd(&home, "0"), now).unwrap();
    let conn = Connection::open(db::path(&home)).unwrap();
    assert_eq!(
        one::<String>(
            &conn,
            "SELECT id || ' ' || kind || ' ' || issue || ' ' || how || ' ' || started || ' ' || ended || ' ' ||
               seconds || ' ' || outcome || ' ' || coalesce(status, '-') || ' ' || exit_code || ' ' || turns || ' ' ||
               tehas_version || ' ' || tehas_commit || ' ' || rukas_commit FROM runs"
        ),
        format!("plan-7-{started} plan 7 answer {started} {now} 300 handoff - 0 2 v0.7.22 1234abc abcdef1")
    );
    let usage: i64 = one(
        &conn,
        "SELECT count(*) FROM runs WHERE coalesce(input, output, cache_read, cache_write, dispatches,
           attempt, queue_wait) IS NOT NULL",
    );
    assert_eq!(usage, 0);
    // No account and nothing the agent said is kept.
    let dump: String = one(
        &conn,
        "SELECT group_concat(quote(r.id) || quote(r.how) || quote(r.status), '') FROM runs r",
    );
    assert!(!dump.contains("alice") && !dump.contains("s1"));

    // plan_finish again for the same turn keeps the first row.
    execute(&home, &plan_cmd(&home, "interrupted"), now + 600).unwrap();
    assert_eq!(
        one::<String>(&conn, "SELECT outcome || ' ' || ended FROM runs"),
        format!("handoff {now}")
    );
}

#[test]
fn plan_outcomes_follow_how_the_turn_ended_and_how_it_was_started() {
    let now = 1_792_000_000;
    let at = iso(now - 60);
    let rec = |turns: i64, answers: Value| json!({"session": "s", "turns": turns, "running": true, "startedAt": at, "answers": answers});
    let millwright = json!([{"turn": 3, "by": "millwright-bot", "millwright": true}]);
    for (rc, record, out, outcome, status, how) in [
        (
            "0",
            rec(0, json!([])),
            Some(json!({"session": "s", "text": "t"})),
            "handoff",
            "-",
            "approved",
        ),
        (
            "0",
            rec(0, json!([])),
            Some(json!({"text": "t"})),
            "failed",
            "-",
            "approved",
        ),
        // An answer marked `millwright` is the millwright's, and a record
        // not marked so is an answer of a person.
        (
            "2",
            rec(2, millwright.clone()),
            Some(json!({"session": "s"})),
            "failed",
            "-",
            "millwright",
        ),
        (
            "2",
            rec(2, json!([{"turn": 3, "by": "alice", "millwright": false}])),
            Some(json!({"session": "s"})),
            "failed",
            "-",
            "answer",
        ),
        (
            "2",
            rec(1, json!([])),
            Some(json!({"session": "s", "lost": true})),
            "stopped",
            "lost",
            "answer",
        ),
        ("4", rec(0, json!([])), None, "refused", "-", "approved"),
        ("64", rec(0, json!([])), None, "refused", "-", "approved"),
        (
            "not-started",
            rec(1, json!([])),
            None,
            "refused",
            "not-started",
            "answer",
        ),
        (
            "interrupted",
            rec(1, json!([])),
            None,
            "failed",
            "interrupted",
            "answer",
        ),
        (
            "not-readable",
            rec(1, json!([])),
            None,
            "failed",
            "not-readable",
            "answer",
        ),
    ] {
        let home = scratch("plan-outcome");
        turn(&home, record, out);
        execute(&home, &plan_cmd(&home, rc), now).unwrap();
        let conn = Connection::open(db::path(&home)).unwrap();
        assert_eq!(
            one::<String>(
                &conn,
                "SELECT outcome || ' ' || coalesce(status, '-') || ' ' || how FROM runs"
            ),
            format!("{outcome} {status} {how}"),
            "rc {rc}"
        );
    }
}

#[test]
fn plan_how_words_are_the_word_of_the_core_grammar() {
    // Each reader of the word has a test of its own that its word is this
    // one: this is analytics', beside the MCP server's and the core word's.
    assert_eq!(
        crate::plan::MILLWRIGHT_MARK,
        tehas_core::grammar::MILLWRIGHT
    );
    let now = 1_792_000_000;
    let at = iso(now - 60);
    let mark = |answer: Value| {
        let home = scratch("plan-mark");
        turn(
            &home,
            json!({"session": "s", "turns": 2, "running": true, "startedAt": at, "answers": [answer]}),
            Some(json!({"session": "s"})),
        );
        execute(&home, &plan_cmd(&home, "2"), now).unwrap();
        let conn = Connection::open(db::path(&home)).unwrap();
        one::<String>(&conn, "SELECT how FROM runs")
    };
    // The word marks the turn the millwright's, and the old one is no mark:
    // were the mark anything else, one of these three disagrees.
    assert_eq!(
        mark(json!({"turn": 3, "by": "bot", "millwright": true})),
        "millwright"
    );
    assert_eq!(
        mark(json!({"turn": 3, "by": "bot", "fixer": true})),
        "answer"
    );
    assert_eq!(
        mark(json!({"turn": 3, "by": "bot", "millwright": false})),
        "answer"
    );
}

#[test]
fn a_refining_session_s_first_turn_is_how_refine_and_its_later_turns_are_answers() {
    let now = 1_792_000_000;
    let at = iso(now - 60);
    for (turns, label, how) in [
        (0, Some("tehas:refine"), "refine"),
        (0, Some("tehas:plan"), "approved"),
        (0, None, "approved"),
        (1, Some("tehas:refine"), "answer"),
    ] {
        let mut record = json!({"session": "s", "turns": turns, "running": true, "startedAt": at, "answers": []});
        if let Some(l) = label {
            record["label"] = json!(l);
        }
        let home = scratch("plan-refine");
        turn(&home, record, Some(json!({"session": "s", "text": "t"})));
        execute(&home, &plan_cmd(&home, "0"), now).unwrap();
        let conn = Connection::open(db::path(&home)).unwrap();
        assert_eq!(
            one::<String>(&conn, "SELECT how FROM runs"),
            how,
            "{label:?} turn {}",
            turns + 1
        );
    }
}

#[test]
fn a_planning_turn_not_in_flight_records_nothing() {
    let now = 1_792_000_000;
    let home = scratch("plan-none");
    let err = execute(&home, &plan_cmd(&home, "0"), now).unwrap_err();
    assert_eq!(err.0, 65);
    assert!(!db::path(&home).exists());
    for record in [
        json!({"session": "s", "turns": 1, "running": false, "startedAt": iso(now - 60)}),
        json!({"session": "s", "turns": 1, "running": true}),
        json!([1, 2]),
        json!(null),
    ] {
        turn(&home, record, None);
        assert_eq!(
            execute(&home, &plan_cmd(&home, "0"), now).unwrap_err().0,
            65
        );
    }
    // The record on disk is not read, and facts through a link are not either.
    let in_flight =
        json!({"session": "s", "turns": 0, "running": true, "startedAt": iso(now - 60)});
    put_at(h(&home).plan_file(7, ""), &in_flight.to_string());
    let target = home.join("tmp/elsewhere.json");
    fs::write(&target, in_flight.to_string()).unwrap();
    fs::remove_file(facts_path(&home, 7)).unwrap();
    assert_eq!(
        execute(&home, &plan_cmd(&home, "0"), now).unwrap_err().0,
        65
    );
    std::os::unix::fs::symlink(&target, facts_path(&home, 7)).unwrap();
    assert_eq!(
        execute(&home, &plan_cmd(&home, "0"), now).unwrap_err().0,
        65
    );
    assert!(!db::path(&home).exists());
    fs::remove_file(facts_path(&home, 7)).unwrap();
    fs::copy(&target, facts_path(&home, 7)).unwrap();
    execute(&home, &plan_cmd(&home, "0"), now).unwrap();
}

#[test]
fn plan_s_arguments_are_checked() {
    let args = |s: &str| s.split(' ').map(str::to_string).collect::<Vec<_>>();
    assert_eq!(
        parse(&args("plan 7 --rc 0 --facts /f.json")),
        Some(Cmd::Plan(
            7,
            crate::plan::Given {
                rc: "0".into(),
                rukas_image: None,
                image: None,
                image_built: None,
                facts: Some("/f.json".into()),
            }
        ))
    );
    assert_eq!(
        parse(&args(
            "plan 7 --facts /f.json --rukas-image abcdef1 --rc not-started"
        )),
        Some(Cmd::Plan(
            7,
            crate::plan::Given {
                rc: "not-started".into(),
                rukas_image: Some("abcdef1".into()),
                image: None,
                image_built: None,
                facts: Some("/f.json".into()),
            }
        ))
    );
    for bad in [
        "plan 7",
        "plan 7 --rc 0",
        "plan 7 --facts /f.json",
        "plan 7 --rc 0 --facts",
        "plan 7 --rc 0 --facts /f.json --facts /g.json",
        "plan 7 --rc",
        "plan 7 --rc 01",
        "plan 7 --rc 1000",
        "plan 7 --rc -1",
        "plan 7 --rc gone",
        "plan 7 --rc 0 --rc 2",
        "plan 7 --rc 0 --rukas-image xyz",
        "plan 07 --rc 0",
        "plan 7 --rc 0 --oom 1",
    ] {
        assert_eq!(parse(&args(bad)), None, "{bad}");
    }
}

// The issue, the --at of an event and the --rc of plan are whole numbers under
// the one rule in tehas_core::grammar::digits: no leading zero, a cap on the
// digits, and "0" only where zero is a value. Each word is put in its place in
// a command that is otherwise right, so a refusal is the number's own.
#[test]
fn the_whole_numbers_of_the_arguments_follow_the_shared_rule() {
    let parsed = |words: &[&str]| parse(&words.iter().map(|w| w.to_string()).collect::<Vec<_>>());
    let issue = |n: &str| parsed(&["event", n, "merged"]);
    let at = |t: &str| parsed(&["event", "12", "ready", "--at", t]);
    let rc = |c: &str| parsed(&["plan", "7", "--rc", c, "--facts", "/f.json"]);
    assert_eq!(issue("1"), Some(Cmd::Event(1, "merged".into(), None)));
    assert_eq!(
        issue("999999999"),
        Some(Cmd::Event(999_999_999, "merged".into(), None))
    );
    for bad in ["", "0", "00", "012", "1234567890", "1a", "+1", " 1"] {
        assert_eq!(issue(bad), None, "issue {bad:?}");
    }
    for (t, secs) in [("0", 0), ("1", 1), ("999999999999", 999_999_999_999)] {
        assert_eq!(
            at(t),
            Some(Cmd::Event(12, "ready".into(), Some(secs))),
            "--at {t}"
        );
    }
    for bad in ["", "00", "012", "1234567890123", "1a", "+1"] {
        assert_eq!(at(bad), None, "--at {bad:?}");
    }
    for c in ["0", "1", "255", "999"] {
        assert!(
            matches!(rc(c), Some(Cmd::Plan(7, ref g)) if g.rc == c),
            "--rc {c}"
        );
    }
    for bad in ["", "00", "012", "1000", "1a", "+1"] {
        assert_eq!(rc(bad), None, "--rc {bad:?}");
    }
}

#[test]
fn an_export_of_no_runs_leaves_out_from_and_to() {
    let home = scratch("export-empty");
    let now = 1_792_022_400;
    let out = home.join("copy.db");
    execute(&home, &Cmd::Copy(out.clone(), true), now).unwrap();
    assert_eq!(
        db::copy_summary(&out).unwrap(),
        format!(r#"{{"api":1,"schema":{},"runs":0}}"#, db::SCHEMA_VERSION)
    );
    // From and to span the runs: the oldest start, the newest end, and the
    // start of a run that has no end.
    let (conn, _) = db::open(&db::path(&home), now).unwrap();
    for (id, started, ended) in [
        ("a", now - 500, Some(now - 400)),
        ("b", now - 900, None),
        ("c", now - 100, Some(now - 50)),
    ] {
        conn.execute(
            "INSERT INTO runs (id, kind, issue, started, ended) VALUES (?1, 'work', 1, ?2, ?3)",
            rusqlite::params![id, started, ended],
        )
        .unwrap();
    }
    let out = home.join("copy2.db");
    execute(&home, &Cmd::Copy(out.clone(), true), now).unwrap();
    assert_eq!(
        db::copy_summary(&out).unwrap(),
        format!(
            r#"{{"api":1,"schema":{},"from":{},"to":{},"runs":3}}"#,
            db::SCHEMA_VERSION,
            now - 900,
            now - 50
        )
    );
}

#[test]
fn an_export_to_a_path_that_is_not_utf8_leaves_no_file() {
    use std::os::unix::ffi::OsStrExt;
    let home = scratch("export-utf8");
    let out = home.join(std::ffi::OsStr::from_bytes(b"copy-\xff.db"));
    let err = execute(&home, &Cmd::Copy(out.clone(), false), 1_792_022_400).unwrap_err();
    assert_eq!(err.0, 73, "{err:?}");
    assert!(!out.exists());
}

fn add_run(conn: &Connection, id: &str, issue: i64, started: i64) {
    conn.execute(
        "INSERT INTO runs (id, kind, issue, started, ended, outcome) VALUES (?1, 'work', ?2, ?3, ?3 + 600, 'offered')",
        rusqlite::params![id, issue, started],
    )
    .unwrap();
}

fn sessions_output(home: &Path, now: i64) -> Vec<Value> {
    let conn = Connection::open(db::path(home)).unwrap();
    crate::sessions::lines(&conn, home, now)
        .unwrap()
        .iter()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn sessions_lists_thirty_days_newest_first() {
    let home = scratch("sessions-days");
    let now = 1_792_000_000;
    let (conn, _) = db::open(&db::path(&home), now).unwrap();
    for (id, days) in [("work-1-a", 1), ("work-2-b", 29), ("work-3-c", 31)] {
        add_run(&conn, id, 1, now - days * DAY);
    }
    let out = sessions_output(&home, now);
    assert_eq!(out.len(), 3);
    assert_eq!(out[0]["days"], 30);
    assert_eq!(out[0]["api"], 1);
    assert_eq!(out[0]["generatedAt"], iso(now));
    assert_eq!(out[1]["id"], "work-1-a");
    assert_eq!(out[2]["id"], "work-2-b");
}

#[test]
fn sessions_break_ties_by_id_descending() {
    let home = scratch("sessions-ties");
    let now = 1_792_000_000;
    let (conn, _) = db::open(&db::path(&home), now).unwrap();
    add_run(&conn, "work-1-a", 1, now - 100);
    add_run(&conn, "work-2-a", 2, now - 100);
    let out = sessions_output(&home, now);
    assert_eq!(out[1]["id"], "work-2-a");
}

#[test]
fn sessions_carry_steps_roles_and_camel_case() {
    let home = scratch("sessions-fields");
    let now = 1_792_000_000;
    let (conn, _) = db::open(&db::path(&home), now).unwrap();
    add_run(&conn, "work-7-x", 7, now - 3600);
    conn.execute(
        "UPDATE runs SET failure_kind = 'cap', rukas_version = 'v0.14.0', review_held = 0, provider_outage = 1 WHERE id = 'work-7-x'",
        [],
    )
    .unwrap();
    for (step, round) in [("plan", 1), ("build", 1)] {
        conn.execute(
            "INSERT INTO run_steps (run_id, step, round, started) VALUES ('work-7-x', ?1, ?2, ?3)",
            rusqlite::params![step, round, now - 3500],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO run_roles (run_id, role, model, turns) VALUES ('work-7-x', 'dev', 'm', 4)",
        [],
    )
    .unwrap();
    let line = &sessions_output(&home, now)[1];
    assert_eq!(line["failureKind"], "cap");
    assert_eq!(line["rukasVersion"], "v0.14.0");
    assert_eq!(line["reviewHeld"], false);
    assert_eq!(line["providerOutage"], true);
    assert_eq!(line["started"], iso(now - 3600));
    assert_eq!(line["steps"].as_array().unwrap().len(), 2);
    assert_eq!(line["roles"].as_array().unwrap().len(), 1);
    assert_eq!(line["roles"][0]["turns"], 4);
    assert_eq!(line["logs"], json!([]));
    assert!(!line.to_string().contains("null"));
}

fn logs_dir(name: &str, files: &[&str]) -> (TestDir, PathBuf) {
    let home = scratch(name);
    for f in files {
        fs::write(home.join("logs").join(f), "abc").unwrap();
    }
    let logs = home.join("logs");
    (home, logs)
}

fn run_logs(
    logs: &Path,
    kind: &str,
    issue: i64,
    started: i64,
    ended: Option<i64>,
    now: i64,
) -> Vec<(String, String, u64)> {
    let names = crate::sessions::log_names(logs);
    crate::sessions::log_files(logs, &names, kind, issue, started, ended, now)
}

#[test]
fn log_files_match_the_stamp_window() {
    let (_home, logs) = logs_dir(
        "logs-window",
        &[
            "work-7-261020-100000.jsonl",
            "work-7-261020-100000.jsonl.err",
            "work-7-261021-100000.jsonl",
        ],
    );
    let started = secs("2026-10-20T10:00:30Z");
    let ended = Some(secs("2026-10-20T11:00:00Z"));
    let got = run_logs(&logs, "work", 7, started, ended, 0);
    let want = |w: &str, n: &str| (w.to_string(), n.to_string(), 3);
    assert_eq!(
        got,
        vec![
            want("transcript", "work-7-261020-100000.jsonl"),
            want("err", "work-7-261020-100000.jsonl.err"),
        ]
    );
}

#[test]
fn log_files_take_only_the_latest_stamp_in_the_window() {
    let (_home, logs) = logs_dir(
        "logs-two-stamps",
        &[
            "work-7-261020-095900.jsonl",
            "work-7-261020-095900.jsonl.result.json",
            "work-7-261020-100000.jsonl",
            "dispatch-work-7.log",
        ],
    );
    let started = secs("2026-10-20T10:00:30Z");
    let ended = Some(secs("2026-10-20T11:00:00Z"));
    let got = run_logs(&logs, "work", 7, started, ended, 0);
    let want = |w: &str, n: &str| (w.to_string(), n.to_string(), 3);
    assert_eq!(
        got,
        vec![
            want("transcript", "work-7-261020-100000.jsonl"),
            want("dispatch", "dispatch-work-7.log"),
        ]
    );
}

#[test]
fn log_files_skip_links_and_other_issues() {
    let (_home, logs) = logs_dir("logs-links", &["work-77-261020-100000.jsonl", "target"]);
    std::os::unix::fs::symlink("target", logs.join("work-7-261020-100000.jsonl")).unwrap();
    let started = secs("2026-10-20T10:00:30Z");
    let got = run_logs(&logs, "work", 7, started, None, started + 3600);
    assert!(got.is_empty());
}

#[test]
fn plan_log_words() {
    let (_home, logs) = logs_dir(
        "logs-plan",
        &["plan-5-261020-100000.jsonl", "plan-5-261020-100000.json"],
    );
    let started = secs("2026-10-20T10:00:10Z");
    let words = |logs: &Path| -> Vec<String> {
        run_logs(logs, "plan", 5, started, Some(started + 60), 0)
            .into_iter()
            .map(|l| l.0)
            .collect()
    };
    assert_eq!(words(&logs), ["progress", "result"]);
    fs::write(logs.join("dispatch-plan-5.log"), "x").unwrap();
    assert_eq!(words(&logs), ["progress", "result", "dispatch"]);
}

#[test]
fn sessions_is_a_command_without_arguments() {
    assert_eq!(parse(&["sessions".to_string()]), Some(Cmd::Sessions(None)));
    assert_eq!(parse(&["sessions".to_string(), "x".to_string()]), None);
    // --out asks for the file instead of the print, as outcomes' does.
    assert_eq!(
        parse(&[
            "sessions".to_string(),
            "--out".to_string(),
            "/tmp/s".to_string()
        ]),
        Some(Cmd::Sessions(Some(std::path::PathBuf::from("/tmp/s"))))
    );
    for bad in [
        vec!["--out"],
        vec!["--out", ""],
        vec!["--out", "-x"],
        vec!["--out", "/a", "--out", "/b"],
    ] {
        let mut args = vec!["sessions".to_string()];
        args.extend(bad.iter().map(|s| s.to_string()));
        assert_eq!(parse(&args), None, "{bad:?}");
    }
}

#[test]
fn sessions_written_makes_its_own_private_directory() {
    // The host only names the directory: the program makes it mode 700 and the
    // file mode 600, so a poll of status writes nothing with its own hands.
    let home = scratch("sessions-written");
    let now = 1_792_000_000;
    let (conn, _) = db::open(&db::path(&home), now).unwrap();
    conn.execute(
        "INSERT INTO runs (id, kind, issue, how, started, ended) VALUES ('work-1-a', 'work', 1, 'agent', ?1, ?2)",
        rusqlite::params![now - 3600, now - 3000],
    )
    .unwrap();
    let lines = crate::sessions::lines(&conn, &home, now).unwrap();
    let dir = home.join("mcp");
    crate::sessions::write(&dir, &lines).unwrap();
    let file = dir.join(crate::sessions::FILE);
    assert_eq!(fs::read_to_string(&file).unwrap(), lines.join("\n") + "\n");
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
}

#[test]
fn plan_sessions_carry_filed() {
    let home = scratch("sessions-filed");
    let now = 1_792_000_000;
    let (conn, _) = db::open(&db::path(&home), now).unwrap();
    let started = now - 3600;
    conn.execute(
        "INSERT INTO runs (id, kind, issue, how, started, ended) VALUES ('plan-5-x', 'plan', 5, 'refine', ?1, ?2)",
        rusqlite::params![started, started + 600],
    )
    .unwrap();
    let line = |at: i64, source: i64, filed: i64| {
        json!({"at": iso(at), "source": source, "filed": filed, "url": format!("https://example.com/i/{filed}")})
            .to_string()
    };
    let ledger = [
        line(started + 100, 5, 11),
        line(started + 100, 6, 12),
        line(started + 700, 5, 13),
        line(started + 721, 5, 14),
        "not json".to_string(),
        line(started - 1, 5, 15),
        // An array, or a field given twice, is no line.
        format!("[\"{}\",5,16,\"https://example.com/i/16\"]", iso(started + 100)),
        format!(
            "{{\"at\":\"{}\",\"source\":5,\"filed\":17,\"filed\":18,\"url\":\"https://example.com/i/17\"}}",
            iso(started + 100)
        ),
    ]
    .join("\n");
    put_at(h(&home).state(State::Refined), &ledger);
    let session = &sessions_output(&home, now)[1];
    assert_eq!(session["how"], "refine");
    assert_eq!(
        session["filed"],
        json!([
            {"issue": 11, "url": "https://example.com/i/11"},
            {"issue": 13, "url": "https://example.com/i/13"},
        ])
    );
    fs::remove_file(h(&home).state(State::Refined)).unwrap();
    assert!(sessions_output(&home, now)[1].get("filed").is_none());
}

// The refined ledger is read under the ledger bounds: a line of LEDGER_MAX_LINE
// bytes or more costs only itself, and a file one byte past the whole-file
// bound is no ledger at all.
#[test]
fn a_ledger_past_the_whole_file_bound_is_no_ledger_at_all() {
    let home = scratch("ledger-bounds");
    fs::create_dir_all(h(&home).state_dir()).unwrap();
    let at = iso(1_792_000_000);
    let line = |filed: i64| json!({"at": at, "source": 5, "filed": filed, "url": "https://example.com/i/1"});
    let text = |lines: &[Value]| lines.iter().map(|l| format!("{l}\n")).collect::<String>();
    // The width is measured on the printed line, so the filler is solved for.
    let wide = |filed: i64, len: usize| {
        let mut w = line(filed);
        w["filler"] = json!("");
        let short = w.to_string().len();
        w["filler"] = json!("x".repeat(len - short));
        assert_eq!(w.to_string().len(), len);
        w
    };
    let file = h(&home).state(State::Refined);
    let max = crate::LEDGER_MAX_LINE;
    put_at(file.clone(), &text(&[line(1), wide(2, max - 1)]));
    assert_eq!(crate::sessions::ledger_lines(&home).len(), 2);
    put_at(file.clone(), &text(&[line(1), wide(2, max)]));
    assert_eq!(crate::sessions::ledger_lines(&home).len(), 1);

    let bound = crate::LEDGER_MAX_BYTES as usize;
    let mut big = text(&(1..=500).map(line).collect::<Vec<_>>());
    big.push_str(&"y".repeat(bound - big.len() + 1));
    assert_eq!(big.len(), bound + 1);
    put_at(file.clone(), &big);
    assert!(crate::sessions::ledger_lines(&home).is_empty());
    // The last byte within the bound is read.
    put_at(file, &big[..bound]);
    assert_eq!(crate::sessions::ledger_lines(&home).len(), 500);
}

// outcomes: the counts behind factory_outcomes, as far back as the database
// holds. A run is counted on its end, a run that never ended on its start.
// The dates these tests name are the ones the clock of the host carries: 2026
// makes a Tuesday of the 20th and a Monday of the 19th.

fn outcomes_output(conn: &Connection, now: i64, f: &crate::outcomes::Filter) -> Vec<Value> {
    crate::outcomes::lines(conn, now, f)
        .unwrap()
        .iter()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// A work run that ended `at`, with what the counts group it by.
#[allow(clippy::too_many_arguments)]
fn add_outcome_run(
    conn: &Connection,
    id: &str,
    at: i64,
    tehas: Option<&str>,
    rukas: Option<&str>,
    outcome: Option<&str>,
    kind: Option<&str>,
    park: Option<&str>,
) {
    conn.execute(
        "INSERT INTO runs (id, kind, issue, started, ended, tehas_version, rukas_version, \
         outcome, failure_kind, park_reason) \
         VALUES (?1, 'work', 1, ?2, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![id, at, tehas, rukas, outcome, kind, park],
    )
    .unwrap();
}

fn ofilter(
    tehas: Option<&str>,
    rukas: Option<&str>,
    since: Option<&str>,
    until: Option<&str>,
    by: &str,
) -> crate::outcomes::Filter {
    crate::outcomes::Filter {
        tehas: tehas.map(str::to_string),
        rukas: rukas.map(str::to_string),
        since: since.and_then(crate::outcomes::bound),
        until: until.and_then(crate::outcomes::bound),
        by: match by {
            "week" => crate::outcomes::By::Week,
            _ => crate::outcomes::By::Version,
        },
    }
}

fn entries(conn: &Connection, now: i64, f: &crate::outcomes::Filter) -> Vec<Value> {
    outcomes_output(conn, now, f)[1..].to_vec()
}

#[test]
fn outcomes_is_a_command_whose_options_are_each_given_once() {
    assert_eq!(
        parse(&["outcomes".to_string()]),
        Some(Cmd::Outcomes(crate::outcomes::Filter::default(), None))
    );
    let whole = parse(&[
        "outcomes".to_string(),
        "--out".to_string(),
        "/tmp/counts".to_string(),
        "--tehas".to_string(),
        "v0.14.2".to_string(),
        "--rukas".to_string(),
        "v0.14.0".to_string(),
        "--since".to_string(),
        "2026-01-01".to_string(),
        "--until".to_string(),
        "2026-02-01T08:00:00Z".to_string(),
        "--by".to_string(),
        "week".to_string(),
    ]);
    assert_eq!(
        whole,
        Some(Cmd::Outcomes(
            crate::outcomes::Filter {
                tehas: Some("v0.14.2".to_string()),
                rukas: Some("v0.14.0".to_string()),
                since: crate::outcomes::bound("2026-01-01"),
                until: crate::outcomes::bound("2026-02-01T08:00:00Z"),
                by: crate::outcomes::By::Week,
            },
            Some(std::path::PathBuf::from("/tmp/counts")),
        ))
    );
    // The options in any order, and any subset of them.
    let by_version = parse(&[
        "outcomes".to_string(),
        "--by".to_string(),
        "version".to_string(),
        "--since".to_string(),
        "2026-01-01".to_string(),
    ]);
    assert_eq!(
        by_version,
        Some(Cmd::Outcomes(
            crate::outcomes::Filter {
                since: crate::outcomes::bound("2026-01-01"),
                ..Default::default()
            },
            None,
        ))
    );
    // A word of another shape, a time that is no time, a grouping that is
    // neither of its two words, an option twice, a value that is another
    // option's shape: none of them is a command.
    for bad in [
        vec!["--tehas", ""],
        vec!["--tehas", "--rukas"],
        vec!["--since", "2026-01-01", "--by"],
        vec!["--tehas", "../x"],
        vec!["--rukas", "x/"],
        vec!["--since", "2026-13-01"],
        vec!["--since", "2026-01-01T24:00:00Z"],
        vec!["--until", "2026-01-01T00:00:00"],
        vec!["--until", "2026-02-30"],
        vec!["--since", "yesterday"],
        vec!["--by", "day"],
        vec!["--tehas", "v1", "--tehas", "v2"],
        vec!["--tehas"],
        vec!["--out"],
        vec!["--out", ""],
        vec!["--out", "--tehas"],
        vec!["--out", "/tmp/a", "--out", "/tmp/b"],
        vec!["extra"],
    ] {
        let mut args = vec!["outcomes".to_string()];
        args.extend(bad.iter().map(|s| s.to_string()));
        assert_eq!(parse(&args), None, "{bad:?}");
    }
}

#[test]
fn outcomes_words_of_another_shape_count_as_none() {
    // A word is a release's name or nothing: what a caller may ask for and
    // what the file may hold are the same shape.
    for bad in ["", "a b", "../x", "x/y", "v1\n", "v1\tx", &"v".repeat(65)] {
        assert_eq!(crate::outcomes::word(bad), None, "{bad:?}");
    }
    for good in ["v0.14.2", "abc1234", "0", "a-b_c.d", &"v".repeat(64)] {
        assert_eq!(
            crate::outcomes::word(good),
            Some(good.to_string()),
            "{good:?}"
        );
    }
    // The two ends of one contract, kept together here: a group the writer may
    // name must be a word the reader of the MCP server can be asked for, or a
    // filter for it answers nothing. The reader's pattern is A-Za-z0-9._+- of
    // 1 to 64; a word of the database may also hold a `:`, which is why a
    // version may not, and the reader's extra `+` is a shape the writer never
    // names, so every word this module accepts is one a filter can be given.
    let reader = regex::Regex::new(r"^[A-Za-z0-9._+-]{1,64}$").unwrap();
    for good in ["v0.14.2", "abc1234", "0", "a-b_c.d", &"v".repeat(64)] {
        assert_eq!(
            crate::outcomes::word(good),
            Some(good.to_string()),
            "{good:?}"
        );
        assert!(reader.is_match(good), "{good:?} is not a reader's word");
    }
    for bad in [
        "",
        "a b",
        "../x",
        "x/y",
        "v1\n",
        "v1\tx",
        "v1+build",
        &"v".repeat(65),
    ] {
        assert_eq!(crate::outcomes::word(bad), None, "{bad:?}");
    }
}

#[test]
fn a_day_bound_is_stricter_than_a_time_bound() {
    // The two shapes of a bound and the two rules they follow, said plainly:
    // a day is read by this module, which refuses a date the calendar does not
    // hold, and a full time by iso_ms of run.rs, which reads the parts it is
    // given and so counts 2026-02-30 on into March. The manual says a bound is
    // a day or a full UTC time; a date that is no date belongs to neither.
    assert_eq!(crate::outcomes::bound("2026-02-30"), None);
    assert_eq!(crate::outcomes::bound("2026-02-28"), Some(1772236800));
    assert!(
        crate::outcomes::bound("2026-02-30T00:00:00Z").is_some(),
        "a full time is read as its parts, as the snapshot's lines are"
    );
}

#[test]
fn outcomes_bounds_accept_a_day_or_a_full_time() {
    let day = crate::outcomes::bound("2026-01-01").unwrap();
    assert_eq!(day % DAY, 0, "a day is its own start");
    assert_eq!(
        crate::outcomes::bound("2026-02-01T08:00:00Z").unwrap() % DAY,
        8 * 3600
    );
    // Neither shape: no bound, and so no command.
    for bad in [
        "yesterday",
        "2026-13-01",
        "2026-02-30",
        "2026-01-01T24:00:00Z",
        "2026-01-01 00:00",
        "",
    ] {
        assert_eq!(crate::outcomes::bound(bad), None, "{bad:?}");
    }
}

#[test]
fn outcomes_weeks_begin_on_monday() {
    // 2026 makes a Tuesday of the 20th and a Monday of the 19th: the whole of
    // a week answers to the Monday that opens it.
    for (day, week) in [
        ("2026-10-19", "2026-10-19"),
        ("2026-10-20", "2026-10-19"),
        ("2026-10-25", "2026-10-19"),
        ("2026-10-26", "2026-10-26"),
    ] {
        let at = crate::outcomes::bound(&format!("{day}T12:00:00Z")).unwrap();
        assert_eq!(crate::outcomes::week_of(at), week.to_string(), "{day}");
    }
}

#[test]
fn outcomes_header_names_the_file_and_the_grouping() {
    let home = scratch("outcomes-header");
    let now = 1_792_000_000;
    let (conn, _) = db::open(&db::path(&home), now).unwrap();
    let head = outcomes_output(&conn, now, &ofilter(None, None, None, None, "version"))[0].clone();
    assert_eq!(head["api"], 1);
    assert_eq!(head["file"], "outcomes.jsonl");
    assert_eq!(head["by"], "version");
    assert_eq!(head["outcomes"], 1, "the shape of the counts");
    // The reach the reader answers with, named by the file that holds it, so
    // the two programs cannot each state a retention of their own.
    assert_eq!(head["days"], db::RETENTION_DAYS, "the reach of the counts");
    assert_eq!(head["generatedAt"], iso(now));
    let weeks = outcomes_output(&conn, now, &ofilter(None, None, None, None, "week"))[0].clone();
    assert_eq!(weeks["by"], "week");
}

#[test]
fn outcomes_per_version_pair_the_most_runs_first() {
    let home = scratch("outcomes-versions");
    let now = 1_792_000_000;
    let (conn, _) = db::open(&db::path(&home), now).unwrap();
    for n in 0..3 {
        add_outcome_run(
            &conn,
            &format!("work-1-{n}"),
            now - 2 * DAY,
            Some("v1"),
            Some("r1"),
            Some("merged"),
            None,
            None,
        );
    }
    add_outcome_run(
        &conn,
        "work-2-a",
        now - 3 * DAY,
        Some("v2"),
        Some("r1"),
        Some("merged"),
        None,
        None,
    );
    let out = entries(&conn, now, &ofilter(None, None, None, None, "version"));
    assert_eq!(out.len(), 2);
    assert_eq!(out[0]["tehasVersion"], "v1");
    assert_eq!(out[0]["runs"], 3, "the pair of the most runs leads");
    assert_eq!(out[1]["tehasVersion"], "v2");
    assert_eq!(out[1]["runs"], 1);
    assert_eq!(out[0]["rukasVersion"], "r1");
}

#[test]
fn outcomes_per_week_newest_first() {
    let home = scratch("outcomes-weeks");
    let now = 1_792_000_000;
    let (conn, _) = db::open(&db::path(&home), now).unwrap();
    add_outcome_run(
        &conn,
        "work-1-a",
        now - 3 * DAY,
        Some("v1"),
        None,
        Some("merged"),
        None,
        None,
    );
    add_outcome_run(
        &conn,
        "work-2-a",
        now - 20 * DAY,
        Some("v1"),
        None,
        Some("merged"),
        None,
        None,
    );
    add_outcome_run(
        &conn,
        "work-3-a",
        now - 16 * DAY,
        Some("v1"),
        None,
        Some("failed"),
        None,
        None,
    );
    let out = entries(&conn, now, &ofilter(None, None, None, None, "week"));
    assert_eq!(out.len(), 3, "three weeks, not three runs: {out:?}");
    assert!(
        out[0]["week"].as_str().unwrap() > out[1]["week"].as_str().unwrap(),
        "newest week first"
    );
    assert!(out[1]["week"].as_str().unwrap() > out[2]["week"].as_str().unwrap());
}

#[test]
fn outcomes_count_how_the_cycle_ended() {
    let home = scratch("outcomes-ended");
    let now = 1_792_000_000;
    let (conn, _) = db::open(&db::path(&home), now).unwrap();
    add_outcome_run(
        &conn,
        "work-1-a",
        now - 2 * DAY,
        Some("v1"),
        None,
        Some("merged"),
        None,
        None,
    );
    add_outcome_run(
        &conn,
        "work-2-a",
        now - 2 * DAY,
        Some("v1"),
        None,
        Some("failed"),
        Some("tests"),
        None,
    );
    add_outcome_run(
        &conn,
        "work-3-a",
        now - 2 * DAY,
        Some("v1"),
        None,
        Some("parked"),
        None,
        Some("needs-human"),
    );
    add_outcome_run(
        &conn,
        "work-4-a",
        now - 2 * DAY,
        Some("v1"),
        None,
        None,
        None,
        None,
    );
    // A second run like the first, in the same second: SQLite answers one
    // row for both, and each map counts the two of them.
    add_outcome_run(
        &conn,
        "work-5-a",
        now - 2 * DAY,
        Some("v1"),
        None,
        Some("merged"),
        None,
        None,
    );
    // A kind and a reason the guard refuses: counted under a word of the
    // code's own, never under what the column held.
    let long = "x".repeat(65);
    add_outcome_run(
        &conn,
        "work-6-a",
        now - 2 * DAY,
        Some("v1"),
        None,
        Some("failed"),
        Some("not a word!"),
        None,
    );
    add_outcome_run(
        &conn,
        "work-7-a",
        now - 2 * DAY,
        Some("v1"),
        None,
        Some("parked"),
        None,
        Some(&long),
    );
    let out = entries(
        &conn,
        now,
        &ofilter(Some("v1"), None, None, None, "version"),
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["runs"], 7);
    let sum = |map: &str| -> u64 {
        out[0][map]
            .as_object()
            .unwrap()
            .values()
            .map(|v| v.as_u64().unwrap())
            .sum()
    };
    assert_eq!(sum("outcomes"), 7, "every run is in outcomes");
    assert_eq!(sum("failureKinds"), 2, "both failed runs carry a kind");
    assert_eq!(sum("parkReasons"), 2, "both parked runs carry a reason");
    assert_eq!(out[0]["failureKinds"]["unrecognised"], 1);
    assert_eq!(out[0]["parkReasons"]["unrecognised"], 1);
    let text = out[0].to_string();
    assert!(
        !text.contains("not a word") && !text.contains(&long),
        "{text}"
    );
    assert_eq!(out[0]["outcomes"]["merged"], 2);
    assert_eq!(out[0]["outcomes"]["failed"], 2);
    assert_eq!(out[0]["outcomes"]["parked"], 2);
    assert_eq!(
        out[0]["outcomes"]["unknown"], 1,
        "a run of no outcome is counted, not dropped"
    );
    assert_eq!(out[0]["failureKinds"]["tests"], 1);
    assert_eq!(out[0]["parkReasons"]["needs-human"], 1);
}

#[test]
fn outcomes_counts_a_run_that_never_ended_on_its_start() {
    let home = scratch("outcomes-never-ended");
    let now = 1_792_000_000;
    let (conn, _) = db::open(&db::path(&home), now).unwrap();
    conn.execute(
        "INSERT INTO runs (id, kind, issue, started, tehas_version, outcome) \
         VALUES ('work-1-a', 'work', 1, ?1, 'v1', 'merged')",
        rusqlite::params![now - 3 * DAY],
    )
    .unwrap();
    let out = entries(&conn, now, &ofilter(None, None, None, None, "version"));
    assert_eq!(out.len(), 1, "a run of no end is still a run");
    assert_eq!(out[0]["runs"], 1);
}

#[test]
fn outcomes_reaches_past_the_thirty_day_window() {
    let home = scratch("outcomes-long");
    let now = 1_792_000_000;
    let (conn, _) = db::open(&db::path(&home), now).unwrap();
    add_outcome_run(
        &conn,
        "work-1-a",
        now - 100 * DAY,
        Some("v1"),
        None,
        Some("merged"),
        None,
        None,
    );
    add_outcome_run(
        &conn,
        "work-2-a",
        now - 200 * DAY,
        Some("v1"),
        None,
        Some("merged"),
        None,
        None,
    );
    let out = entries(&conn, now, &ofilter(None, None, None, None, "week"));
    assert_eq!(out.len(), 2, "two weeks, however far back: {out:?}");
    // The sessions of a snapshot reach thirty days; these reach a year.
    let old = iso(now - 200 * DAY);
    let out = entries(
        &conn,
        now,
        &ofilter(None, None, Some("2020-01-01"), None, "week"),
    );
    assert!(out
        .iter()
        .any(|e| e["week"] == crate::outcomes::week_of(crate::outcomes::bound(&old).unwrap())));
}

#[test]
fn outcomes_holds_no_issue_cause_or_log() {
    let home = scratch("outcomes-no-leak");
    let now = 1_792_000_000;
    let (conn, _) = db::open(&db::path(&home), now).unwrap();
    conn.execute(
        "INSERT INTO runs (id, kind, issue, started, ended, tehas_version, outcome, \
         failure_cause, failure_kind) \
         VALUES ('work-1-a', 'work', 42, ?1, ?1, 'v1', 'failed', \
         'the whole of a log line with a token sk-abc123', 'tests')",
        rusqlite::params![now - DAY],
    )
    .unwrap();
    let text = crate::outcomes::lines(&conn, now, &ofilter(None, None, None, None, "version"))
        .unwrap()
        .join("\n");
    for forbidden in ["sk-abc123", "log line", "\"cause\"", "\"issue\""] {
        assert!(
            !text.contains(forbidden),
            "the counts hold {forbidden:?}: {text}"
        );
    }
    assert!(text.contains("tests"), "the kinds are counted: {text}");
}

#[test]
fn outcomes_filters_by_version_and_window() {
    let home = scratch("outcomes-filters");
    let now = 1_792_000_000;
    let (conn, _) = db::open(&db::path(&home), now).unwrap();
    add_outcome_run(
        &conn,
        "work-1-a",
        now - 5 * DAY,
        Some("v1"),
        Some("r1"),
        Some("merged"),
        None,
        None,
    );
    add_outcome_run(
        &conn,
        "work-2-a",
        now - 5 * DAY,
        Some("v2"),
        Some("r1"),
        Some("merged"),
        None,
        None,
    );
    add_outcome_run(
        &conn,
        "work-3-a",
        now - 40 * DAY,
        None,
        None,
        Some("merged"),
        None,
        None,
    );
    let names = |f: crate::outcomes::Filter| {
        entries(&conn, now, &f)
            .iter()
            .map(|e| {
                (
                    e["tehasVersion"].as_str().unwrap_or("-").to_string(),
                    e["rukasVersion"].as_str().unwrap_or("-").to_string(),
                    e["runs"].as_u64().unwrap(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(ofilter(Some("v1"), None, None, None, "version")),
        vec![("v1".to_string(), "r1".to_string(), 1)]
    );
    // A pi-rukas version is matched as the sessions match theirs: the word the
    // run holds, its commit where it holds no version.
    assert_eq!(
        names(ofilter(None, Some("r1"), None, None, "version")).len(),
        2
    );
    // Nothing asked for: every entry, the pair of the run of no version among them.
    assert_eq!(names(ofilter(None, None, None, None, "version")).len(), 3);
    assert!(names(ofilter(Some("nope"), None, None, None, "version")).is_empty());
    // A day bound is its own start.
    assert_eq!(crate::outcomes::bound("2026-01-01").unwrap() % DAY, 0);
    // Both shapes of a bound name a moment, and both read the same way. The
    // runs of five days ago fell at 17:46 of that day; a stamp an hour later
    // is after them, so it leaves them out at the front of a window and keeps
    // them at the back. A whole day names its own start, which is before them,
    // so it opens the window on them and closes it before them. A bound is
    // read as far as itself, inclusive: the bound of the run's own second
    // keeps it, as the pair below shows.
    let stamp = iso(now - 5 * DAY + 3600);
    let day = iso(now - 5 * DAY).chars().take(10).collect::<String>();
    let n = |s: Option<&str>, u: Option<&str>| names(ofilter(None, None, s, u, "version")).len();
    assert_eq!(n(Some(&stamp), None), 0);
    assert_eq!(n(Some(&day), None), 2);
    assert_eq!(
        n(None, Some(&stamp)),
        3,
        "the hour after the pair, the older run with them"
    );
    // The day of the pair opens at midnight of it, which is before them, and
    // before the run of forty days ago: what is left is that one entry.
    assert_eq!(n(None, Some(&day)), 1);
    // An hour on either side of the runs, and the pair is out of the window
    // at one end and in at the other.
    let before = iso(now - 5 * DAY - 3600);
    let after = iso(now - 5 * DAY + 3600);
    // The hour before reaches them; the hour after has only the run of forty
    // days ago left out of a year's window, so the count is one and not nought.
    assert_eq!(n(Some(&before), None), 2);
    assert_eq!(n(Some(&after), None), 0);
    assert_eq!(n(None, Some(&before)), 1);
    assert_eq!(n(None, Some(&after)), 3);
    // A window the runs are outside of: nothing.
    assert!(names(ofilter(
        None,
        None,
        Some("1970-01-01"),
        Some("1970-01-02"),
        "version"
    ))
    .is_empty());
    // The weeks answer as whole weeks: a window inside a week keeps the whole
    // of it, so the two runs of five days ago stand for their Monday.
    let weeks = entries(&conn, now, &ofilter(None, None, Some(&day), None, "week"));
    assert_eq!(weeks.len(), 1, "{weeks:?}");
    assert_eq!(weeks[0]["runs"], 2);
}

#[test]
fn outcomes_written_to_a_directory_is_the_file_the_reader_opens() {
    let home = scratch("outcomes-written");
    let now = 1_792_000_000;
    let (conn, _) = db::open(&db::path(&home), now).unwrap();
    add_outcome_run(
        &conn,
        "work-1-a",
        now - 2 * DAY,
        Some("v1"),
        Some("r1"),
        Some("merged"),
        None,
        None,
    );
    add_outcome_run(
        &conn,
        "work-2-a",
        now - 2 * DAY,
        Some("v1"),
        Some("r1"),
        Some("failed"),
        Some("tests"),
        None,
    );
    // The directory is the program's to make: the host names it and asks for
    // the file, and writes nothing itself.
    let dir = home.join("mcp");
    crate::outcomes::write(
        &dir,
        &conn,
        now,
        &ofilter(None, None, None, None, "version"),
    )
    .unwrap();
    let file = dir.join(crate::outcomes::FILE);
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        crate::outcomes::lines(&conn, now, &ofilter(None, None, None, None, "version"))
            .unwrap()
            .join("\n")
            + "\n",
        "the file holds what the command prints"
    );
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
        0o700
    );
    // Written again, it replaces itself, and no name of a run is left behind.
    crate::outcomes::write(
        &dir,
        &conn,
        now + 60,
        &ofilter(None, None, None, None, "week"),
    )
    .unwrap();
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
    assert!(fs::read_to_string(&file)
        .unwrap()
        .contains("\"by\":\"week\""));
    // A name in the way where the directory should be: an error that says so,
    // and nothing written anywhere.
    let blocked = home.join("blocked");
    fs::write(&blocked, "").unwrap();
    let why = crate::outcomes::write(
        &blocked,
        &conn,
        now,
        &ofilter(None, None, None, None, "version"),
    )
    .unwrap_err();
    assert!(why.contains("the directory"), "{why}");
    assert_eq!(fs::read_to_string(&blocked).unwrap(), "");
}

#[test]
fn outcomes_out_is_a_directory_written_by_the_program_alone() {
    // The host runs no reader of its own: the shape it puts on disk is what
    // this wrote, so a call of the whole command writes the file and prints
    // nothing.
    let home = scratch("outcomes-out-command");
    let now = 1_792_000_000;
    let (conn, _) = db::open(&db::path(&home), now).unwrap();
    add_outcome_run(
        &conn,
        "work-1-a",
        now - DAY,
        Some("v1"),
        None,
        Some("parked"),
        None,
        Some("needs-human"),
    );
    drop(conn);
    let dir = home.join("mcp");
    let cmd = parse(&[
        "outcomes".to_string(),
        "--out".to_string(),
        dir.to_str().unwrap().to_string(),
    ])
    .unwrap();
    assert!(matches!(cmd, Cmd::Outcomes(_, Some(_))));
    execute(&home, &cmd, now).unwrap();
    let text = fs::read_to_string(dir.join(crate::outcomes::FILE)).unwrap();
    assert!(
        text.contains("\"parkReasons\":{\"needs-human\":1}"),
        "{text}"
    );
    // The unfiltered command prints the same and writes nothing new: the
    // directory holds the file and the line that says it was written, and no
    // third name.
    let printed = parse(&["outcomes".to_string()]).unwrap();
    execute(&home, &printed, now).unwrap();
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
}

// The build time of an image is kept once per image id, so every run of that
// image carries it, also one recorded before the time was known.
#[test]
fn a_build_time_reaches_every_run_of_its_image() {
    let home = scratch("image-built");
    let started = secs("2026-10-18T10:16:39Z");
    let now = started + 3700;
    let image = format!("sha256:{}", "a".repeat(64));
    let other = format!("sha256:{}", "b".repeat(64));
    let (conn, _) = db::open(&db::path(&home), now).unwrap();
    add_run(&conn, "work-1-old", 1, started - 7200);
    add_run(&conn, "work-2-other", 2, started - 3600);
    conn.execute(
        "UPDATE runs SET image = ?1 WHERE id = 'work-1-old'",
        [&image],
    )
    .unwrap();
    conn.execute(
        "UPDATE runs SET image = ?1 WHERE id = 'work-2-other'",
        [&other],
    )
    .unwrap();
    drop(conn);
    cycle(
        &home,
        "headless-316134e",
        started,
        json!({"at": iso(started + 30), "started": started, "rc": 10, "status": "handoff"}),
    );
    let built = secs("2026-10-17T08:00:00Z");
    let given = Given {
        image: Some(image.clone()),
        image_built: Some(built),
        facts: Some(facts_path(&home, 7)),
        ..Given::default()
    };
    execute(&home, &Cmd::Run(7, given), now).unwrap();
    let out = sessions_output(&home, now);
    assert_eq!(out.len(), 4);
    let by_id = |id: &str| out.iter().find(|s| s["id"] == id).unwrap().clone();
    let new = out.iter().find(|s| s["issue"] == 7).unwrap();
    for s in [new.clone(), by_id("work-1-old")] {
        assert_eq!(s["image"], image.as_str());
        assert_eq!(s["imageBuilt"], "2026-10-17T08:00:00Z");
    }
    let o = by_id("work-2-other");
    assert_eq!(o["image"], other.as_str());
    assert!(o.get("imageBuilt").is_none());

    // The first time recorded for an id stands.
    let again = Given {
        image: Some(image.clone()),
        image_built: Some(built + 99),
        facts: Some(facts_path(&home, 7)),
        ..Given::default()
    };
    execute(&home, &Cmd::Run(7, again), now).unwrap();
    let conn = Connection::open(db::path(&home)).unwrap();
    assert_eq!(
        one::<String>(
            &conn,
            &format!("SELECT value FROM meta WHERE key = 'image:{image}'")
        ),
        built.to_string()
    );
}

#[test]
fn a_build_time_without_an_image_is_not_kept() {
    let home = scratch("image-built-alone");
    let started = secs("2026-10-18T10:16:39Z");
    cycle(
        &home,
        "headless-316134e",
        started,
        json!({"at": iso(started + 30), "started": started, "rc": 10, "status": "handoff"}),
    );
    let given = Given {
        image_built: Some(started - 60),
        facts: Some(facts_path(&home, 7)),
        ..Given::default()
    };
    execute(&home, &Cmd::Run(7, given), started + 3700).unwrap();
    let conn = Connection::open(db::path(&home)).unwrap();
    assert_eq!(
        one::<i64>(&conn, "SELECT count(*) FROM meta WHERE key LIKE 'image:%'"),
        0
    );
}

#[test]
fn prune_drops_the_build_time_of_an_image_no_run_names() {
    let home = scratch("image-built-prune");
    let now = 1_792_000_000;
    let (conn, _) = db::open(&db::path(&home), now).unwrap();
    let kept = format!("sha256:{}", "c".repeat(64));
    let gone = format!("sha256:{}", "d".repeat(64));
    add_run(&conn, "work-1-a", 1, now - DAY);
    conn.execute("UPDATE runs SET image = ?1", [&kept]).unwrap();
    for i in [&kept, &gone] {
        db::image_built(&conn, Some(i), Some(now - 2 * DAY)).unwrap();
    }
    db::prune(&conn, now).unwrap();
    assert_eq!(
        one::<String>(
            &conn,
            "SELECT group_concat(key, ' ') FROM meta WHERE key LIKE 'image:%'"
        ),
        format!("image:{kept}")
    );
    assert_eq!(
        one::<i64>(
            &conn,
            "SELECT count(*) FROM meta WHERE key NOT LIKE 'image:%'"
        ),
        3
    );
}

#[test]
fn plan_records_its_image_and_build_time() {
    let home = scratch("plan-image");
    let now = 1_792_000_000;
    let started = now - 300;
    turn(
        &home,
        json!({"session": "s1", "turns": 0, "running": true, "startedAt": iso(started)}),
        Some(json!({"session": "s1"})),
    );
    let image = format!("sha256:{}", "e".repeat(64));
    let cmd = Cmd::Plan(
        7,
        crate::plan::Given {
            rc: "0".into(),
            rukas_image: Some("abcdef1".into()),
            image: Some(image.clone()),
            image_built: Some(now - DAY),
            facts: Some(facts_path(&home, 7)),
        },
    );
    execute(&home, &cmd, now).unwrap();
    let out = sessions_output(&home, now);
    assert_eq!(out[1]["kind"], "plan");
    assert_eq!(out[1]["image"], image.as_str());
    assert_eq!(out[1]["imageBuilt"], iso(now - DAY));
}

#[test]
fn the_build_time_options_are_read() {
    let args = |s: &str| s.split(' ').map(str::to_string).collect::<Vec<_>>();
    let image = format!("sha256:{}", "0f".repeat(32));
    let facts = || Some(PathBuf::from("/f.json"));
    assert_eq!(
        parse(&args(&format!(
            "run 12 --facts /f.json --image {image} --image-built 2026-10-29T14:44:16.123456789Z"
        ))),
        Some(Cmd::Run(
            12,
            Given {
                image: Some(image.clone()),
                image_built: Some(secs("2026-10-29T14:44:16Z")),
                facts: facts(),
                ..Given::default()
            }
        ))
    );
    // A time docker might print that cannot be read is left out: the run is
    // still recorded.
    for odd in [
        "yesterday",
        "2026-10-29T14:44:16+00:00",
        &format!("2026-10-29T14:44:16.{}Z", "1".repeat(40)),
    ] {
        assert_eq!(
            parse(&args(&format!(
                "run 12 --facts /f.json --image-built {odd}"
            ))),
            Some(Cmd::Run(
                12,
                Given {
                    facts: facts(),
                    ..Given::default()
                }
            )),
            "{odd}"
        );
    }
    assert_eq!(
        parse(&args(&format!(
            "plan 7 --rc 0 --facts /f.json --image {image} --image-built 2026-10-29T14:44:16Z"
        ))),
        Some(Cmd::Plan(
            7,
            crate::plan::Given {
                rc: "0".into(),
                rukas_image: None,
                image: Some(image.clone()),
                image_built: Some(secs("2026-10-29T14:44:16Z")),
                facts: facts(),
            }
        ))
    );
    for bad in [
        "run 12 --facts /f.json --image-built 2026-10-29T14:44:16Z --image-built 2026-10-29T14:44:16Z"
            .to_string(),
        "run 12 --facts /f.json --image-built".into(),
        "plan 7 --rc 0 --facts /f.json --image sha256:abc".into(),
        format!("plan 7 --rc 0 --facts /f.json --image {image} --image {image}"),
        "plan 7 --rc 0 --facts /f.json --image-built x --image-built y".into(),
    ] {
        assert_eq!(parse(&args(&bad)), None, "{bad}");
    }
}

// ---- the files the host is sent, and the line that says they were written ----

/// One ended run of the kind `id` names, as both commands read it. The id
/// carries the kind, so a test that runs both commands adds one run each.
fn one_ended_run(conn: &Connection, id: &str, at: i64) {
    conn.execute(
        "INSERT INTO runs (id, kind, issue, started, ended, outcome) VALUES (?1, ?2, 1, ?3, ?3, 'offered')",
        rusqlite::params![id, if id.starts_with("plan") { "plan" } else { "work" }, at],
    )
    .unwrap();
}

/// A run of the kind `id` names that has not ended.
fn one_live_run(conn: &Connection, id: &str, at: i64) {
    conn.execute(
        "INSERT INTO runs (id, kind, issue, started) VALUES (?1, ?2, 1, ?3)",
        rusqlite::params![
            id,
            if id.starts_with("plan") {
                "plan"
            } else {
                "work"
            },
            at
        ],
    )
    .unwrap();
}

/// The plain `sessions --out <home>/mcp` command.
fn sessions_cmd(home: &Path) -> Cmd {
    out_command(home, "sessions").0
}

/// The plain `outcomes --out <home>/mcp` command.
fn outcomes_cmd(home: &Path) -> Cmd {
    out_command(home, "outcomes").0
}

/// The command `<what> --out <home>/mcp`, and the directory it writes.
fn out_command(home: &Path, what: &str) -> (Cmd, PathBuf) {
    let dir = home.join("mcp");
    let cmd = parse(&[
        what.to_string(),
        "--out".to_string(),
        dir.to_str().unwrap().to_string(),
    ])
    .unwrap();
    (cmd, dir)
}

/// The name of the file `<what> --out` writes.
fn out_file(what: &str) -> &'static str {
    match what {
        "sessions" => crate::sessions::FILE,
        _ => crate::outcomes::FILE,
    }
}

/// The state of the two files of a skip's directory that belong to the command
/// `what`: the bytes of the file, and the line that says it was written - none,
/// when it is not there, which is what a live run leaves.
fn written(dir: &Path, what: &str) -> (Vec<u8>, Option<Vec<u8>>) {
    (
        fs::read(dir.join(out_file(what))).unwrap(),
        fs::read(dir.join(crate::publish::stamp_name(out_file(what)))).ok(),
    )
}

/// The directory of a skip's test: both commands are asked for in turn, so it
/// holds the two files and the two lines before a test looks at it. A test that
/// means to watch a missing file or a missing stamp takes it away itself.
fn out_setup(home: &Path, now: i64) -> PathBuf {
    let dir = home.join("mcp");
    for what in ["sessions", "outcomes"] {
        let (cmd, here) = out_command(home, what);
        assert_eq!(here, dir, "{what} was asked for another directory");
        execute(home, &cmd, now).unwrap();
    }
    dir
}

/// The bytes of a file and the time it was last written.
fn file_and_mtime(dir: &Path, name: &str) -> (Vec<u8>, std::time::SystemTime) {
    let file = dir.join(name);
    (
        fs::read(&file).unwrap(),
        fs::metadata(&file).unwrap().modified().unwrap(),
    )
}

/// A scratch home of one ended run, made at `at`, with both files and both
/// lines already written by a call of the same time.
fn home_of_one_ended_run(name: &str, at: i64) -> (TestDir, PathBuf) {
    let home = scratch(name);
    let (conn, _) = db::open(&db::path(&home), at).unwrap();
    one_ended_run(&conn, "work-1-a", at - 3600);
    drop(conn);
    let dir = out_setup(&home, at);
    (home, dir)
}

#[test]
fn out_writes_the_file_and_its_stamp_the_first_time() {
    let (home, dir) = home_of_one_ended_run("out-first", 1_792_000_000);
    let now = 1_792_000_000;
    // Neither file is there, so both are written, each with a line of its own
    // beside it: four names in a directory that held none.
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 4);
    for what in ["sessions", "outcomes"] {
        let (body, stamp) = written(&dir, what);
        assert!(!body.is_empty(), "{what} was written empty");
        assert!(
            stamp.is_some(),
            "{what} was written without a line to say so"
        );
        // The mode of both is the private one: the reader is the MCP server of
        // this host and no one else.
        assert_eq!(
            fs::metadata(dir.join(out_file(what)))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600,
            "{what} is readable by another account"
        );
        assert_eq!(
            fs::metadata(dir.join(crate::publish::stamp_name(out_file(what))))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600,
            "the line of {what} is readable by another account"
        );
    }
    // The line is the whole fingerprint and nothing else: the count of runs,
    // the latest end, and the day the window closes on.
    let conn = Connection::open(db::path(&home)).unwrap();
    assert_eq!(
        String::from_utf8(written(&dir, "sessions").1.unwrap()).unwrap(),
        format!(
            "{}\n",
            crate::publish::fingerprint(&conn, now).unwrap().unwrap()
        ),
        "the line is not the fingerprint of this database"
    );
}

/// A run whose old how an open rewrites changes no count and no end, and the
/// published sessions still follow it: the next call writes the file again
/// in the new word, for each old word.
#[test]
fn out_after_a_rewrite_of_old_hows_writes_the_sessions_again() {
    let now = 1_792_000_000i64 - 1_792_000_000i64.rem_euclid(86_400) + 43_200;
    for (old, new) in db::OLD_HOWS {
        let (home, dir) = home_of_one_ended_run(&format!("out-rewrite-{old}"), now);
        let before = written(&dir, "sessions");
        let conn = Connection::open(db::path(&home)).unwrap();
        conn.execute("UPDATE runs SET how = ?1", [old]).unwrap();
        drop(conn);
        execute(&home, &sessions_cmd(&home), now + 60).unwrap();
        let after = written(&dir, "sessions");
        assert_ne!(after.1, before.1, "{old}: the line did not move");
        let body = String::from_utf8(after.0).unwrap();
        assert!(
            body.contains(&format!("\"how\":\"{new}\"")),
            "{old}: {body}"
        );
        assert!(!body.contains(&format!("\"{old}\"")), "{old}: {body}");
        // Nothing more to rewrite: the next call writes nothing.
        execute(&home, &sessions_cmd(&home), now + 120).unwrap();
        assert_eq!(written(&dir, "sessions").1, after.1, "{old}");
    }
}

#[test]
fn out_of_two_calls_with_no_change_touches_nothing() {
    let (home, dir) = home_of_one_ended_run("out-unchanged", 1_792_000_000);
    // The middle of a UTC day, so that the minute added below stays inside the
    // day the line names. The call that crosses midnight is
    // out_of_a_new_day_writes_the_file_again.
    let now = 1_792_000_000i64 - 1_792_000_000i64.rem_euclid(86_400) + 43_200;
    let (conn, _) = db::open(&db::path(&home), now).unwrap();
    conn.execute("DELETE FROM runs", []).unwrap();
    one_ended_run(&conn, "work-1-a", now - 3600);
    drop(conn);
    // A poll: the snapshot is asked for and then the counts, as tehasbot status
    // asks them. Both are asked at a minute apart, so that an mtime of one poll
    // cannot be mistaken for that of the other.
    let poll = |at: i64| {
        execute(&home, &sessions_cmd(&home), at).unwrap();
        execute(&home, &outcomes_cmd(&home), at).unwrap();
    };
    let mtimes = || {
        (
            fs::metadata(dir.join(crate::sessions::FILE))
                .unwrap()
                .modified()
                .unwrap(),
            fs::metadata(dir.join(crate::outcomes::FILE))
                .unwrap()
                .modified()
                .unwrap(),
        )
    };
    poll(now);
    let before = (
        mtimes(),
        written(&dir, "sessions"),
        written(&dir, "outcomes"),
    );
    // The poll a minute on reads the whole database and writes neither file:
    // no run started or ended in between, and the day of the window has not
    // moved. The mtimes are the proof, because a file that was written would
    // carry the time of the call that wrote it, and the words of a snapshot
    // name the second it was read, so a rewrite of the same words would still
    // be seen there.
    poll(now + 60);
    assert_eq!(
        (
            mtimes(),
            written(&dir, "sessions"),
            written(&dir, "outcomes")
        ),
        before,
        "a poll of nothing new wrote one of the files"
    );
    // And the line of each says the runs of the database and this day, which
    // is what the next call compares and finds the same.
    for what in ["sessions", "outcomes"] {
        let conn = Connection::open(db::path(&home)).unwrap();
        assert_eq!(
            written(&dir, what).1.unwrap(),
            format!(
                "{}\n",
                crate::publish::fingerprint(&conn, now + 60)
                    .unwrap()
                    .unwrap()
            )
            .into_bytes(),
            "the line of {what} says other runs or another day than this holds"
        );
    }
    // A poll that does write moves them, so the pair above can fail: the same
    // poll of the next day finds another day in the line and writes both files
    // again, and neither is written a third time.
    poll(now + 60 + DAY);
    let moved = mtimes();
    assert_ne!(moved, before.0, "a poll of a new day left the files alone");
    poll(now + 120 + DAY);
    assert_eq!(
        mtimes(),
        moved,
        "a poll of nothing new wrote one of the files of the new day"
    );
}

#[test]
fn out_of_a_new_run_writes_the_file_again() {
    let (home, dir) = home_of_one_ended_run("out-new-run", 1_792_000_000);
    let now = 1_792_000_000;
    let before = [written(&dir, "sessions"), written(&dir, "outcomes")];
    let snapshot = fs::read(dir.join(crate::sessions::FILE)).unwrap();
    let conn = Connection::open(db::path(&home)).unwrap();
    one_ended_run(&conn, "work-2-a", now - 600);
    drop(conn);
    // One new run, and both files are written again: each answers to the runs
    // of the database, whatever it holds of them.
    execute(&home, &sessions_cmd(&home), now).unwrap();
    execute(&home, &outcomes_cmd(&home), now).unwrap();
    assert_ne!(
        written(&dir, "sessions"),
        before[0],
        "the snapshot skipped a new run"
    );
    assert_ne!(
        written(&dir, "outcomes"),
        before[1],
        "the counts skipped a new run"
    );
    // And the snapshot names it.
    let now_held = fs::read(dir.join(crate::sessions::FILE)).unwrap();
    assert_ne!(
        now_held, snapshot,
        "the snapshot of a new run holds the old only"
    );
    assert!(
        String::from_utf8(now_held).unwrap().contains("work-2-a"),
        "the new run is not in the snapshot"
    );
}

#[test]
fn out_of_a_live_run_leaves_no_line_and_writes_again() {
    // The logs of a running cycle grow with no row changing, so there is
    // nothing to compare and no line is left to compare with: every poll of a
    // live run writes both files again.
    let home = scratch("out-live-run");
    let now = 1_792_000_000;
    let (conn, _) = db::open(&db::path(&home), now).unwrap();
    one_live_run(&conn, "work-1-a", now - 600);
    drop(conn);
    let dir = home.join("mcp");
    execute(&home, &sessions_cmd(&home), now).unwrap();
    execute(&home, &outcomes_cmd(&home), now).unwrap();
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
    assert_eq!(written(&dir, "sessions").1, None, "a live run was stamped");
    assert_eq!(written(&dir, "outcomes").1, None, "a live run was stamped");
    let before = [
        fs::read(dir.join(crate::sessions::FILE)).unwrap(),
        fs::read(dir.join(crate::outcomes::FILE)).unwrap(),
    ];
    // The day moves under a run that never ends, and both files follow it.
    execute(&home, &sessions_cmd(&home), now + DAY).unwrap();
    execute(&home, &outcomes_cmd(&home), now + DAY).unwrap();
    assert_ne!(
        fs::read(dir.join(crate::sessions::FILE)).unwrap(),
        before[0],
        "a poll skipped a live run"
    );
    assert_ne!(
        fs::read(dir.join(crate::outcomes::FILE)).unwrap(),
        before[1],
        "a poll skipped a live run"
    );
}

#[test]
fn out_of_a_missing_output_file_writes_it_again() {
    // The reader opens the file by name, so a stamp that matches one the
    // directory no longer holds must not be believed.
    let (home, dir) = home_of_one_ended_run("out-file-gone", 1_792_000_000);
    let now = 1_792_000_000;
    for what in ["sessions", "outcomes"] {
        fs::remove_file(dir.join(out_file(what))).unwrap();
    }
    let before = fs::read_dir(&dir).unwrap().count();
    execute(&home, &sessions_cmd(&home), now).unwrap();
    execute(&home, &outcomes_cmd(&home), now).unwrap();
    assert!(
        dir.join(crate::sessions::FILE).is_file(),
        "the snapshot stayed away"
    );
    assert!(
        dir.join(crate::outcomes::FILE).is_file(),
        "the counts stayed away"
    );
    // The two lines were kept: a file written again needs no new line, whose
    // words are those of the stamp that was already believed.
    assert_eq!(fs::read_dir(&dir).unwrap().count(), before + 2);
}

#[test]
fn out_of_a_missing_stamp_writes_the_file_again() {
    let (home, dir) = home_of_one_ended_run("out-stamp-gone", 1_792_000_000);
    let now = 1_792_000_000;
    for what in ["sessions", "outcomes"] {
        fs::remove_file(dir.join(crate::publish::stamp_name(out_file(what)))).unwrap();
    }
    let before = [
        file_and_mtime(&dir, crate::sessions::FILE),
        file_and_mtime(&dir, crate::outcomes::FILE),
    ];
    execute(&home, &sessions_cmd(&home), now + 1).unwrap();
    execute(&home, &outcomes_cmd(&home), now + 2).unwrap();
    assert_ne!(
        file_and_mtime(&dir, crate::sessions::FILE),
        before[0],
        "the snapshot kept its mtime"
    );
    assert_ne!(
        file_and_mtime(&dir, crate::outcomes::FILE),
        before[1],
        "the counts kept their mtime"
    );
    // Each line is there again, and holds the runs of the database.
    for what in ["sessions", "outcomes"] {
        let line =
            fs::read_to_string(dir.join(crate::publish::stamp_name(out_file(what)))).unwrap();
        assert_eq!(
            line,
            format!(
                "{}\n",
                crate::publish::fingerprint(&Connection::open(db::path(&home)).unwrap(), now + 1)
                    .unwrap()
                    .unwrap()
            ),
            "the line of {what} says other runs than the database holds"
        );
    }
}

#[test]
fn out_of_a_stamp_of_the_other_command_leaves_the_file_too() {
    // One line each, so a stamp left for sessions never speaks for outcomes
    // and the other way round: the names keep them apart.
    let home = scratch("out-two-stamps");
    let now = 1_792_000_000;
    let (conn, _) = db::open(&db::path(&home), now).unwrap();
    // One run of the work the snapshot keeps and one of the outcome the
    // counts group by, so the two files hold different words at one and the
    // same time: which line was believed is then told by the file it was read
    // back from, and not only by the time of it.
    one_ended_run(&conn, "work-1-a", now - 3600);
    one_ended_run(&conn, "plan-2-a", now - 1800);
    drop(conn);
    let dir = home.join("mcp");
    execute(&home, &sessions_cmd(&home), now).unwrap();
    execute(&home, &outcomes_cmd(&home), now).unwrap();
    let sessions_stamp = dir.join(crate::publish::stamp_name(crate::sessions::FILE));
    let outcomes_stamp = dir.join(crate::publish::stamp_name(crate::outcomes::FILE));
    let line = fs::read(&sessions_stamp).unwrap();
    // The line the counts would write, held aside: what is put in front of them
    // next is that same line under the other name, and the file is held while
    // it is, so a match of the counts' own words - which is what the two lines
    // are - is proved not to be the reason the file stands. What separates a
    // line that belongs to a file from one that does not is its name, and the
    // name is what the two stamps are made of.
    let counts = fs::read(dir.join(crate::outcomes::FILE)).unwrap();
    assert_eq!(
        line,
        fs::read(&outcomes_stamp).unwrap(),
        "the two lines differ, so the test below would prove nothing"
    );
    fs::remove_file(&outcomes_stamp).unwrap();
    fs::write(&sessions_stamp, "not a line of ours").unwrap();
    let before = file_and_mtime(&dir, crate::outcomes::FILE);
    // The counts are asked for with the line of the snapshot in front of them,
    // and no line of their own: nothing of this directory speaks for them, so
    // the file is written again even though no run has changed. The time of it
    // is what tells, put moving it for a write and no write leaving it - the
    // words cannot, since no run came or went and a file names the second it
    // was read, which this call carries as the file it compares with does.
    execute(&home, &outcomes_cmd(&home), now).unwrap();
    assert_ne!(
        file_and_mtime(&dir, crate::outcomes::FILE),
        before,
        "the counts believed the line of the snapshot"
    );
    // And what they wrote is again the counts, not the snapshot the line came
    // from: the reader opens the file by name, so a line of another file may
    // cost a write and may not answer for what is in it.
    assert_eq!(
        fs::read(dir.join(crate::outcomes::FILE)).unwrap(),
        counts,
        "the counts were answered with the words of the snapshot"
    );
    // The line of the counts is there again, and the line of the snapshot was
    // left as it was found: no command writes another's line.
    assert_eq!(fs::read(&outcomes_stamp).unwrap(), line);
    assert_eq!(
        fs::read_to_string(&sessions_stamp).unwrap(),
        "not a line of ours",
        "the counts wrote the line of the snapshot"
    );
}

#[test]
fn out_of_a_stamp_of_other_words_writes_the_file_again() {
    // A line of any other shape is no line: not one with a second line after
    // it, not one with a mark taken away, not one of another day.
    let (home, dir) = home_of_one_ended_run("out-stamp-words", 1_792_000_000);
    let now = 1_792_000_000;
    let stamp = dir.join(crate::publish::stamp_name(crate::sessions::FILE));
    let held = fs::read_to_string(&stamp).unwrap();
    let conn = Connection::open(db::path(&home)).unwrap();
    let line = crate::publish::fingerprint(&conn, now).unwrap().unwrap();
    for bad in [
        held.trim_end().to_string(),
        format!("{held}extra\n"),
        format!("{}{held}", line),
        held.replace("runs=1", "runs=0"),
        String::new(),
    ] {
        fs::write(&stamp, &bad).unwrap();
        let before = file_and_mtime(&dir, crate::sessions::FILE);
        execute(&home, &sessions_cmd(&home), now).unwrap();
        assert_ne!(
            file_and_mtime(&dir, crate::sessions::FILE),
            before,
            "a line of {bad:?} was believed"
        );
    }
}

#[test]
fn out_of_a_new_day_writes_the_file_again() {
    // The window moves on its own, so the day is part of what is compared: a
    // snapshot of one day is not a snapshot of the next.
    let (home, dir) = home_of_one_ended_run("out-new-day", 1_792_000_000);
    let now = 1_792_000_000;
    let before = [written(&dir, "sessions"), written(&dir, "outcomes")];
    for what in ["sessions", "outcomes"] {
        let cmd = match what {
            "sessions" => sessions_cmd(&home),
            _ => outcomes_cmd(&home),
        };
        execute(&home, &cmd, now + DAY).unwrap();
        assert_ne!(
            written(&dir, what),
            before[if what == "sessions" { 0 } else { 1 }],
            "{what} kept yesterday's file"
        );
    }
    // The snapshot names the day it was read, which is the words a stamp
    // cannot tell the two days apart by.
    let header: Value = serde_json::from_str(
        String::from_utf8(file_and_mtime(&dir, crate::sessions::FILE).0)
            .unwrap()
            .lines()
            .next()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        header["generatedAt"],
        iso(now + DAY),
        "the snapshot of the new day still names the old one"
    );
}

/// The names of a table's columns, in their order.
fn columns(conn: &Connection, table: &str) -> Vec<String> {
    let mut q = conn
        .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
        .unwrap();
    let names = q.query_map([], |r| r.get::<_, String>(0)).unwrap();
    names.map(Result::unwrap).collect()
}

/// The tables' columns, the foreign keys, and the SQL of every index and view:
/// what makes two files the same schema whatever ALTER TABLE left in the
/// text of their tables.
fn shape(conn: &Connection) -> Vec<String> {
    let mut q = conn
        .prepare(
            "SELECT type, name, coalesce(sql, '') FROM sqlite_master
             WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name",
        )
        .unwrap();
    let rows: Vec<(String, String, String)> = q
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let mut out = Vec::new();
    for (kind, name, sql) in rows {
        if kind != "table" {
            out.push(format!("{kind} {name}: {sql}"));
            continue;
        }
        for pragma in ["table_info", "foreign_key_list"] {
            let mut q = conn
                .prepare(&format!("SELECT * FROM pragma_{pragma}('{name}')"))
                .unwrap();
            let n = q.column_count();
            let cells = q
                .query_map([], |r| {
                    (0..n)
                        .map(|i| {
                            r.get::<_, rusqlite::types::Value>(i)
                                .map(|v| format!("{v:?}"))
                        })
                        .collect::<rusqlite::Result<Vec<_>>>()
                })
                .unwrap();
            for c in cells {
                out.push(format!("{kind} {name} {pragma}: {}", c.unwrap().join(" ")));
            }
        }
    }
    out
}

/// A file of schema 4 as `fixture` wrote it, with rows in runs, run_roles
/// and samples, watts among them.
fn schema_4_file(file: &Path, fixture: &str) {
    let old = Connection::open(file).unwrap();
    old.execute_batch(fixture).unwrap();
    old.execute_batch(
        "INSERT INTO meta (key, value) VALUES ('schema', '4'), ('retention_days', '365'),
           ('created', '100');
         INSERT INTO runs (id, kind, issue, started, outcome, how)
           VALUES ('work-7-100', 'work', 7, 100, 'pr', 'label');
         INSERT INTO run_roles (run_id, role, model, input) VALUES ('work-7-100', 'worker', 'm', 12);
         INSERT INTO samples (at, in_line, running, awaiting, stopped, planning, cycle_limit,
           paused, held, watts) VALUES (150, 1, 2, 3, 4, 5, 6, 0, 'limit', 412.5),
           (160, NULL, NULL, NULL, NULL, NULL, NULL, 1, NULL, NULL);
         PRAGMA user_version = 4;",
    )
    .unwrap();
    assert!(columns(&old, "samples").contains(&"watts".to_string()));
}

const SCHEMA_4: [(&str, &str); 2] = [
    (
        "v1.4.0",
        include_str!("../tests/fixtures/schema-4-v1.4.0.sql"),
    ),
    (
        "v0.30.6",
        include_str!("../tests/fixtures/schema-4-v0.30.6.sql"),
    ),
];

#[test]
fn a_new_database_is_schema_8_with_no_watts_no_contractor_no_cost_and_no_view() {
    let home = scratch("schema-8-new");
    let (conn, _) = db::open(&db::path(&home), 0).unwrap();
    assert_eq!(one::<i64>(&conn, "PRAGMA user_version"), 8);
    assert!(!columns(&conn, "samples").contains(&"watts".to_string()));
    assert!(!columns(&conn, "runs").contains(&"contractor_id".to_string()));
    assert!(!columns(&conn, "runs").contains(&"cost".to_string()));
    assert!(!columns(&conn, "run_roles").contains(&"cost".to_string()));
    assert_eq!(
        one::<i64>(
            &conn,
            "SELECT count(*) FROM sqlite_master WHERE type = 'view'"
        ),
        0
    );
}

/// A file of schema 5 as the 2.0 branch wrote it before schema 6: the
/// v1.4.0 fixture without samples.watts, with a contractor on one run and
/// none on the other.
fn schema_5_file(file: &Path) {
    let old = Connection::open(file).unwrap();
    old.execute_batch(SCHEMA_4[0].1).unwrap();
    old.execute_batch(
        "ALTER TABLE samples DROP COLUMN watts;
         INSERT INTO meta (key, value) VALUES ('schema', '5'), ('retention_days', '365'),
           ('created', '100');
         INSERT INTO runs (id, kind, issue, started, outcome, contractor_id)
           VALUES ('work-7-100', 'work', 7, 100, 'pr', 'box-1'),
                  ('work-8-110', 'work', 8, 110, 'parked', NULL);
         INSERT INTO run_roles (run_id, role, model, input) VALUES ('work-7-100', 'worker', 'm', 12);
         INSERT INTO samples (at, in_line, running, awaiting, stopped, planning, cycle_limit,
           paused, held) VALUES (150, 1, 2, 3, 4, 5, 6, 0, 'limit');
         PRAGMA user_version = 5;",
    )
    .unwrap();
    assert!(columns(&old, "runs").contains(&"contractor_id".to_string()));
    assert!(!columns(&old, "samples").contains(&"watts".to_string()));
}

#[test]
fn a_schema_5_file_migrates_in_place_and_keeps_every_row_without_its_contractor() {
    let fresh = scratch("schema-8-fresh-from-5");
    let (made, _) = db::open(&db::path(&fresh), 0).unwrap();
    let home = scratch("schema-5");
    let file = db::path(&home);
    schema_5_file(&file);
    let (conn, moved) = db::open(&file, 200).unwrap();
    assert_eq!(moved, None, "a file of schema 5 is not moved aside");
    assert_eq!(one::<i64>(&conn, "PRAGMA user_version"), 8);
    assert_eq!(
        one::<String>(&conn, "SELECT value FROM meta WHERE key = 'schema'"),
        "8"
    );
    assert_eq!(shape(&conn), shape(&made));
    assert!(!columns(&conn, "runs").contains(&"contractor_id".to_string()));
    assert_eq!(
        one::<String>(
            &conn,
            "SELECT group_concat(row, ' | ') FROM (SELECT id || ' ' || issue || ' ' || outcome
               AS row FROM runs ORDER BY id)"
        ),
        "work-7-100 7 offered | work-8-110 8 parked"
    );
    assert_eq!(
        one::<String>(
            &conn,
            "SELECT o.model || ' ' || o.input FROM runs r JOIN run_roles o ON o.run_id = r.id"
        ),
        "m 12"
    );
    assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM samples"), 1);
    assert_eq!(
        one::<i64>(&conn, "SELECT count(*) FROM pragma_foreign_key_check"),
        0
    );
    drop(conn);
    // Opened again it is an 8 already: nothing is written.
    let before = fs::read(&file).unwrap();
    let (again, moved) = db::open(&file, 300).unwrap();
    assert_eq!(moved, None);
    drop(again);
    assert!(fs::read(&file).unwrap() == before, "a second open wrote");
}

#[test]
fn a_schema_4_file_of_either_age_migrates_in_place_to_the_shape_of_a_new_one() {
    let fresh = scratch("schema-8-fresh");
    let (made, _) = db::open(&db::path(&fresh), 0).unwrap();
    for (age, fixture) in SCHEMA_4 {
        let home = scratch(&format!("schema-4-{age}"));
        let file = db::path(&home);
        schema_4_file(&file, fixture);
        let (conn, moved) = db::open(&file, 200).unwrap();
        assert_eq!(moved, None, "{age}: a file of schema 4 is not moved aside");
        assert_eq!(one::<i64>(&conn, "PRAGMA user_version"), 8, "{age}");
        assert_eq!(shape(&conn), shape(&made), "{age}");
        assert_eq!(
            one::<String>(&conn, "SELECT value FROM meta WHERE key = 'schema'"),
            "8",
            "{age}"
        );
        assert_eq!(
            one::<String>(
                &conn,
                "SELECT group_concat(row, ' | ') FROM (SELECT at || ' ' || coalesce(in_line, '-')
                   || ' ' || coalesce(running, '-') || ' ' || coalesce(awaiting, '-') || ' ' ||
                   coalesce(stopped, '-') || ' ' || coalesce(planning, '-') || ' ' ||
                   coalesce(cycle_limit, '-') || ' ' || paused || ' ' || coalesce(held, '-') AS row
                   FROM samples ORDER BY at)"
            ),
            "150 1 2 3 4 5 6 0 limit | 160 - - - - - - 1 -",
            "{age}"
        );
        assert_eq!(
            one::<String>(
                &conn,
                "SELECT r.id || ' ' || r.outcome || ' ' || r.how || ' ' || o.model || ' ' || o.input
                   FROM runs r JOIN run_roles o ON o.run_id = r.id"
            ),
            "work-7-100 offered approved m 12",
            "{age}"
        );
        assert_eq!(
            one::<i64>(&conn, "SELECT count(*) FROM pragma_foreign_key_check"),
            0,
            "{age}"
        );
        drop(conn);
        // Opened again it is an 8 already: nothing is written.
        let before = fs::read(&file).unwrap();
        let (again, moved) = db::open(&file, 300).unwrap();
        assert_eq!(moved, None, "{age}");
        assert_eq!(shape(&again), shape(&made), "{age}");
        drop(again);
        assert!(
            fs::read(&file).unwrap() == before,
            "{age}: a second open wrote"
        );
    }
}

#[test]
fn a_schema_4_file_another_program_migrated_meanwhile_is_not_migrated_again() {
    let home = scratch("schema-4-race");
    let file = db::path(&home);
    schema_4_file(&file, SCHEMA_4[0].1);
    // Both read user_version 4; the other migrated before this one took the
    // write lock.
    let mut late = Connection::open(&file).unwrap();
    db::open(&file, 200).unwrap();
    assert!(db::settle(&mut late, 4, 300).is_ok());
    assert_eq!(one::<i64>(&late, "PRAGMA user_version"), 8);
    assert!(!columns(&late, "samples").contains(&"watts".to_string()));
    assert_eq!(one::<i64>(&late, "SELECT count(*) FROM samples"), 2);
}

#[test]
fn programs_that_open_a_schema_4_file_at_once_all_get_it_migrated() {
    for round in 0..10 {
        let home = scratch(&format!("schema-4-concurrent-{round}"));
        let file = db::path(&home);
        schema_4_file(&file, SCHEMA_4[round % 2].1);
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
            assert!(
                o.join().unwrap().is_none(),
                "a schema 4 file was moved aside"
            );
        }
        let conn = Connection::open(&file).unwrap();
        assert_eq!(one::<i64>(&conn, "PRAGMA user_version"), 8);
        assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM issue_events"), 8);
        assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM samples"), 2);
    }
}

/// A file of schema 6 as the 2.0 branch wrote it before schema 7, with runs
/// that ended `pr` and were started by `label`, and the steps and roles of
/// one of them.
fn schema_6_file(file: &Path) {
    let old = Connection::open(file).unwrap();
    old.execute_batch(include_str!("../tests/fixtures/schema-6.sql"))
        .unwrap();
    old.execute_batch(
        "INSERT INTO meta (key, value) VALUES ('schema', '6'), ('retention_days', '365'),
           ('created', '100');
         INSERT INTO runs (id, kind, issue, how, started, ended, seconds, outcome, tehas_version)
           VALUES ('work-7-100', 'work', 7, 'label', 100, 700, 600, 'pr', 'v1.4.0'),
                  ('work-8-110', 'work', 8, 'reply', 110, 710, 600, 'merged', 'v1.4.0'),
                  ('work-9-120', 'work', 9, 'label', 120, 720, 600, 'parked', 'v1.4.0'),
                  ('plan-5-130', 'plan', 5, 'label', 130, 190, 60, NULL, 'v1.4.0');
         INSERT INTO run_roles (run_id, role, model, input) VALUES ('work-7-100', 'worker', 'm', 12);
         INSERT INTO run_steps (run_id, step, round, seconds) VALUES ('work-7-100', 'develop', 1, 300);
         INSERT INTO run_gate_runs (run_id, seq, ok, ms) VALUES ('work-7-100', 1, 1, 900);
         PRAGMA user_version = 6;",
    )
    .unwrap();
}

/// Schema 6 to 7: `runs.outcome` takes `offered` and no longer `pr`, every
/// `pr` outcome and every `label` how is rewritten, every row of runs and of
/// its children stays, the file has the
/// shape of a new one, the rewrite is counted for the published sessions, and
/// a second open writes nothing. No `pr` outcome and no `label` how is left.
#[test]
fn a_schema_6_file_takes_the_words_of_2_0_and_keeps_every_row() {
    let fresh = scratch("schema-8-fresh-from-6");
    let (made, _) = db::open(&db::path(&fresh), 0).unwrap();
    let home = scratch("schema-6");
    let file = db::path(&home);
    schema_6_file(&file);
    let (conn, moved) = db::open(&file, 200).unwrap();
    assert_eq!(moved, None, "a file of schema 6 is not moved aside");
    assert_eq!(one::<i64>(&conn, "PRAGMA user_version"), 8);
    assert_eq!(
        one::<String>(&conn, "SELECT value FROM meta WHERE key = 'schema'"),
        "8"
    );
    assert_eq!(shape(&conn), shape(&made));
    assert_eq!(
        one::<String>(
            &conn,
            "SELECT group_concat(row, ' | ') FROM (SELECT id || ' ' || how || ' '
               || coalesce(outcome, '-') AS row FROM runs ORDER BY id)"
        ),
        "plan-5-130 approved - | work-7-100 approved offered | work-8-110 reply merged | \
         work-9-120 approved parked"
    );
    assert_eq!(
        one::<i64>(
            &conn,
            "SELECT count(*) FROM runs WHERE outcome = 'pr' OR how = 'label'"
        ),
        0
    );
    assert_eq!(
        one::<String>(
            &conn,
            "SELECT o.model || ' ' || o.input || ' ' || s.step || ' ' || g.ms FROM run_roles o
               JOIN run_steps s ON s.run_id = o.run_id JOIN run_gate_runs g ON g.run_id = o.run_id
               WHERE o.run_id = 'work-7-100'"
        ),
        "m 12 develop 900"
    );
    assert_eq!(
        one::<i64>(&conn, "SELECT count(*) FROM pragma_foreign_key_check"),
        0
    );
    assert!(conn
        .execute("UPDATE runs SET outcome = 'pr' WHERE id = 'work-7-100'", [])
        .is_err());
    assert_eq!(
        one::<String>(
            &conn,
            &format!(
                "SELECT value FROM meta WHERE key = '{}'",
                db::HOWS_REWRITTEN
            )
        ),
        "1"
    );
    drop(conn);
    // Opened again it is an 8 already: nothing is written.
    let before = fs::read(&file).unwrap();
    let (again, moved) = db::open(&file, 300).unwrap();
    assert_eq!(moved, None);
    drop(again);
    assert!(fs::read(&file).unwrap() == before, "a second open wrote");
}

/// A file of schema 7 as the 2.0 branch wrote it before schema 8: runs and
/// roles with a cost, steps and gates, and the three views over them.
fn schema_7_file(file: &Path) {
    let old = Connection::open(file).unwrap();
    old.execute_batch(include_str!("../tests/fixtures/schema-7.sql"))
        .unwrap();
    old.execute_batch(
        "INSERT INTO meta (key, value) VALUES ('schema', '7'), ('retention_days', '365'),
           ('created', '100');
         INSERT INTO runs (id, kind, issue, how, started, ended, seconds, outcome, input, cost,
             tehas_version, gate_runs, gate_ms)
           VALUES ('work-7-100', 'work', 7, 'approved', 100, 700, 600, 'offered', 40, 0.25,
                   'v2.0.0', 1, 900),
                  ('work-8-110', 'work', 8, 'reply', 110, 710, 600, 'merged', NULL, NULL,
                   'v2.0.0', NULL, NULL);
         INSERT INTO run_roles (run_id, role, model, input, cost, ms)
           VALUES ('work-7-100', 'worker', 'm', 12, 0.125, 300);
         INSERT INTO run_steps (run_id, step, round, seconds) VALUES ('work-7-100', 'develop', 1, 300);
         INSERT INTO run_gate_runs (run_id, seq, ok, ms) VALUES ('work-7-100', 1, 1, 900);
         INSERT INTO issue_events (at, issue, event) VALUES (50, 7, 'ready');
         INSERT INTO samples (at, running, paused, held) VALUES (150, 1, 0, '');
         PRAGMA user_version = 7;",
    )
    .unwrap();
    assert!(columns(&old, "runs").contains(&"cost".to_string()));
    assert_eq!(
        one::<i64>(
            &old,
            "SELECT count(*) FROM sqlite_master WHERE type = 'view'"
        ),
        3
    );
}

/// Schema 7 to 8: `runs.cost` and `run_roles.cost` go, and the views
/// `v_lead`, `v_daily` and `v_versions` with them. Every row of every table
/// stays with its other columns, the file has the shape of a new one, and a
/// second open writes nothing.
#[test]
fn a_schema_7_file_loses_cost_and_the_views_and_keeps_every_row() {
    let fresh = scratch("schema-8-fresh-from-7");
    let (made, _) = db::open(&db::path(&fresh), 0).unwrap();
    let home = scratch("schema-7");
    let file = db::path(&home);
    schema_7_file(&file);
    let (conn, moved) = db::open(&file, 200).unwrap();
    assert_eq!(moved, None, "a file of schema 7 is not moved aside");
    assert_eq!(one::<i64>(&conn, "PRAGMA user_version"), 8);
    assert_eq!(
        one::<String>(&conn, "SELECT value FROM meta WHERE key = 'schema'"),
        "8"
    );
    assert_eq!(shape(&conn), shape(&made));
    assert!(!columns(&conn, "runs").contains(&"cost".to_string()));
    assert!(!columns(&conn, "run_roles").contains(&"cost".to_string()));
    assert_eq!(
        one::<i64>(
            &conn,
            "SELECT count(*) FROM sqlite_master WHERE type = 'view'"
        ),
        0
    );
    assert_eq!(
        one::<String>(
            &conn,
            "SELECT group_concat(row, ' | ') FROM (SELECT id || ' ' || how || ' ' || outcome
               || ' ' || coalesce(input, '-') || ' ' || coalesce(gate_ms, '-') AS row
               FROM runs ORDER BY id)"
        ),
        "work-7-100 approved offered 40 900 | work-8-110 reply merged - -"
    );
    assert_eq!(
        one::<String>(
            &conn,
            "SELECT o.model || ' ' || o.input || ' ' || o.ms || ' ' || s.step || ' ' || g.ms
               FROM run_roles o JOIN run_steps s ON s.run_id = o.run_id
               JOIN run_gate_runs g ON g.run_id = o.run_id WHERE o.run_id = 'work-7-100'"
        ),
        "m 12 300 develop 900"
    );
    assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM issue_events"), 1);
    assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM samples"), 1);
    assert_eq!(
        one::<i64>(&conn, "SELECT count(*) FROM pragma_foreign_key_check"),
        0
    );
    drop(conn);
    // Opened again it is an 8 already: nothing is written.
    let before = fs::read(&file).unwrap();
    let (again, moved) = db::open(&file, 300).unwrap();
    assert_eq!(moved, None);
    drop(again);
    assert!(fs::read(&file).unwrap() == before, "a second open wrote");
}

/// The end word of schema 7 on: `offered`, never `pr`, in the schema itself.
#[test]
fn the_schema_says_offered_and_never_pr() {
    assert!(SCHEMA_TEXT.contains("'offered'"));
    assert!(!SCHEMA_TEXT.contains("'pr'"));
}

const SCHEMA_TEXT: &str = include_str!("schema.sql");

/// A store of before 2.0 holds runs a ticked retry started, `how`
/// `retry-tick`, and runs recorded under the millwright's old name, `fixer`.
/// Each open rewrites them to `reply` and `millwright` as data, on the schema
/// the file has, and an open with none left writes nothing; the sessions then
/// list those runs with the new words (#2782).
#[test]
fn an_open_rewrites_the_hows_of_before_2_0_once_and_the_sessions_list_them() {
    let home = scratch("old-hows");
    let now = 1_792_000_000;
    drop(db::open(&db::path(&home), now).unwrap());
    let conn = Connection::open(db::path(&home)).unwrap();
    add_run(&conn, "work-7-old", 7, now - 3600);
    add_run(&conn, "work-8-old", 8, now - 3600);
    conn.execute_batch(
        "UPDATE runs SET how = 'retry-tick' WHERE id = 'work-7-old';
         UPDATE runs SET how = 'fixer' WHERE id = 'work-8-old';",
    )
    .unwrap();
    let version: i64 = one(&conn, "PRAGMA user_version");
    drop(conn);
    for _ in 0..2 {
        let (c, _) = db::open(&db::path(&home), now).unwrap();
        assert_eq!(
            one::<i64>(&c, "SELECT count(*) FROM runs WHERE how = 'retry-tick'"),
            0
        );
        assert_eq!(
            one::<String>(&c, "SELECT how FROM runs WHERE id = 'work-7-old'"),
            "reply"
        );
        assert_eq!(
            one::<String>(&c, "SELECT how FROM runs WHERE id = 'work-8-old'"),
            "millwright"
        );
        assert_eq!(one::<i64>(&c, "PRAGMA user_version"), version);
        drop(c);
    }
    let listed = sessions_output(&home, now);
    let run = listed
        .iter()
        .find(|l| l["id"] == "work-7-old")
        .expect("the run is listed");
    assert_eq!(run["how"], "reply", "{run}");
    let run = listed
        .iter()
        .find(|l| l["id"] == "work-8-old")
        .expect("the run is listed");
    assert_eq!(run["how"], "millwright", "{run}");
}
