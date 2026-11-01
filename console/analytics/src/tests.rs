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

/// These records, one per line: how a jsonl ledger is written.
fn ledger(records: &[Value]) -> String {
    records.iter().map(|l| format!("{l}\n")).collect()
}

/// The four buckets of nothing, the shape the manual names: the report of a
/// ledger that holds nothing, or that the report will not read.
fn empty_buckets() -> Value {
    let nothing = || json!({"n": 0, "shipped": 0, "leadTime": {"n": 0}});
    json!({"none": nothing(), "1-2": nothing(), "3-4": nothing(), "5+": nothing()})
}

/// Where a test's facts of the issue's run or turn are, as lib/analytics.sh
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

// A cycle of issue 7 from a fixture, as libexec/cycle and libexec/work leave
// it: the result, the progress summary and the version on disk, and the facts
// of the settings, the approval, the image's commit and the history line
// `line`.
fn cycle(home: &Path, name: &str, started: i64, line: Value) {
    let dir = fixture(name);
    fs::copy(
        dir.join("result.json"),
        home.join("state/cycles/7.result.json"),
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
    put(
        home,
        "state/cycles/7.progress.json",
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
        json!({"at": iso(started - 90), "how": "label"}),
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
            (
                "code-review-specialist".into(),
                "sub-model".into(),
                600,
                200,
                0.125
            ),
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
        // What libexec/cycle passes on. The files it removed are not read.
        put(&home, "state/cycles/12.oom", "9\n");
        put(
            &home,
            "state/cycles/12.image",
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
    let path = home.join("state/cycles/7.result.json");
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
            line(t, "label"),
            line(t + 1000, "retry-tick"),
            line(t + 2000, "fixer"),
        ],
    );
    fact(
        &home,
        9,
        "approval",
        json!({"at": iso(t + 1900), "how": "fixer"}),
    );
    let conn = record(&home, 9, t + 3000);
    assert_eq!(
        one::<String>(
            &conn,
            "SELECT attempt || ' ' || how || ' ' || queue_wait FROM runs"
        ),
        "3 fixer 100"
    );
    // Recorded again, it replaces itself.
    execute(&home, &Cmd::Run(9, given(&home, 9)), t + 3000).unwrap();
    assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM runs"), 1);
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
            "pr",
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
    let path = home.join("state/cycles/7.result.json");
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
    put(
        &home,
        "state/cycles/7.history.jsonl",
        &format!(
            "{}\n",
            json!({"at": iso(started + 9), "started": started - 5000, "rc": 1, "status": "failed"})
        ),
    );
    put(
        &home,
        "state/cycles/7.approved.json",
        &json!({"at": iso(started - 9000), "how": "fixer"}).to_string(),
    );
    put(
        &home,
        "state/cycles/7.settings",
        &json!({"container": {"PI_ENSEMBLE_SUBAGENT_MODEL": "file-model"}, "concurrentCycles": 9})
            .to_string(),
    );
    put(&home, "state/cycles/7.rukas", "fedcba9\n");
    put(
        &home,
        "state/cycles/7.progress.json",
        r#"{"roles":[{"role":"ops","dispatches":1}],"reviewHeld":false}"#,
    );
    history(
        &home,
        7,
        &[
            json!({"at": iso(started + 60), "started": started, "rc": 0, "status": "merged",
                 "how": "label"}),
        ],
    );
    fact(&home, 7, "attempt", json!(4));
    fact(
        &home,
        7,
        "approval",
        json!({"at": iso(started - 30), "how": "retry-tick"}),
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
        format!("work-7-{started} 4 retry-tick 30 merged 60 2 1234abc")
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
                 "how": "label"}),
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
        "label - - - 1"
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
    assert!(!home.join("state/analytics.db.broken").exists());
}

#[test]
fn programs_that_open_a_new_database_at_once_all_get_it() {
    // The journal switch is where the openers contend, so the eight-opener
    // run is repeated rather than run once: one pass can miss the lock race.
    for round in 0..20 {
        let home = scratch(&format!("concurrent-{round}"));
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
            assert!(
                o.join().unwrap().is_none(),
                "a valid database was moved aside"
            );
        }
        assert!(!home.join("state/analytics.db.broken").exists());
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
        let aside = fs::read(home.join("state/analytics.db.broken")).unwrap();
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
    execute(&home, &Cmd::Event(1, "ready".into(), None), 1_792_000_000).unwrap();
    assert!(!home.join("state/analytics.db.broken").exists());
    let kept = Connection::open(home.join("state/analytics.db.schema-99")).unwrap();
    assert_eq!(one::<i64>(&kept, "PRAGMA user_version"), 99);
    assert_eq!(one::<i64>(&kept, "SELECT x FROM runs"), 42);
    let conn = Connection::open(&file).unwrap();
    assert_eq!(one::<i64>(&conn, "PRAGMA user_version"), db::SCHEMA_VERSION);
    assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM issue_events"), 1);
}

/// The database as a schema 2 program made it, from that program's schema.sql
/// and its `create`, in WAL mode, with a finished run, its step and its role.
fn schema_2_database(file: &Path, now: i64) -> Connection {
    let c = Connection::open(file).unwrap();
    c.execute_batch(include_str!("../tests/fixtures/schema-2.sql"))
        .unwrap();
    for (key, value) in [
        ("schema", "2".to_string()),
        ("retention_days", "365".to_string()),
        ("created", now.to_string()),
    ] {
        c.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)",
            rusqlite::params![key, value],
        )
        .unwrap();
    }
    c.pragma_update(None, "user_version", 2).unwrap();
    c.pragma_update(None, "journal_mode", "WAL").unwrap();
    c.execute_batch(&format!(
        "INSERT INTO runs (id, kind, issue, started, ended, seconds, outcome, pr, input, gate_runs, gate_ms)
           VALUES ('work-7-1', 'work', 7, {s}, {e}, 60, 'pr', 70, 10, 1, 5000);
         INSERT INTO run_steps (run_id, step, round) VALUES ('work-7-1', 'implement', 1);
         INSERT INTO run_roles (run_id, role) VALUES ('work-7-1', 'developer');",
        s = now - 120,
        e = now - 60,
    ))
    .unwrap();
    c
}

// Every object as SQLite holds it, less the white space: ALTER TABLE puts the
// new column on the old table's last line.
fn stored_schema(conn: &Connection) -> Vec<(String, String)> {
    let mut q = conn
        .prepare("SELECT name, sql FROM sqlite_master WHERE sql IS NOT NULL ORDER BY type, name")
        .unwrap();
    let rows = q
        .query_map([], |r| {
            let sql: String = r.get(1)?;
            Ok((r.get(0)?, sql.split_whitespace().collect()))
        })
        .unwrap();
    rows.map(Result::unwrap).collect()
}

#[test]
fn a_schema_2_database_migrates_in_place_and_keeps_its_runs() {
    let home = scratch("migrate");
    let file = db::path(&home);
    let now = 1_792_000_000;
    drop(schema_2_database(&file, now));
    let (conn, moved) = db::open(&file, now).unwrap();
    assert_eq!(moved, None);
    assert_eq!(one::<i64>(&conn, "PRAGMA user_version"), db::SCHEMA_VERSION);
    assert_eq!(one::<String>(&conn, "PRAGMA journal_mode"), "wal");
    assert_eq!(
        one::<String>(
            &conn,
            "SELECT group_concat(key || '=' || value, ' ') FROM (SELECT * FROM meta ORDER BY key)"
        ),
        format!(
            "created={now} retention_days=365 schema={}",
            db::SCHEMA_VERSION
        )
    );
    assert_eq!(
        one::<String>(
            &conn,
            "SELECT id || ' ' || gate_ms || ' ' || coalesce(driver, 'none') FROM runs"
        ),
        "work-7-1 5000 none"
    );
    assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM run_steps"), 1);
    assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM run_roles"), 1);
    // Nothing went aside.
    let names: Vec<String> = fs::read_dir(home.join("state"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.contains(".schema-") || n.ends_with(".broken"))
        .collect();
    assert_eq!(names, Vec::<String>::new());
    // A second open has nothing left to do.
    let (again, moved) = db::open(&file, now + 1).unwrap();
    assert_eq!(moved, None);
    assert_eq!(one::<i64>(&again, "SELECT count(*) FROM runs"), 1);
}

#[test]
fn a_migrated_database_holds_what_a_new_one_does() {
    let home = scratch("migrated");
    let file = db::path(&home);
    drop(schema_2_database(&file, 1_792_000_000));
    let (migrated, _) = db::open(&file, 1_792_000_000).unwrap();
    let fresh = scratch("fresh");
    let (new, _) = db::open(&db::path(&fresh), 1_792_000_000).unwrap();
    assert_eq!(stored_schema(&migrated), stored_schema(&new));
    let driver_is_checked = migrated.execute("UPDATE runs SET driver = 'other'", []);
    assert!(driver_is_checked.is_err());
    // What schema.pinned pins of a run is what a migrated database takes.
    let run = run::Run {
        id: "work-8-1".into(),
        issue: 8,
        started: 1,
        driver: Some("lean".into()),
        ..run::Run::default()
    };
    run::insert(&migrated, &run).unwrap();
    assert_eq!(
        one::<String>(&migrated, "SELECT driver FROM runs WHERE issue = 8"),
        "lean"
    );
}

#[test]
fn programs_that_open_a_schema_2_database_at_once_all_get_it() {
    for round in 0..20 {
        let home = scratch(&format!("migrate-concurrent-{round}"));
        let file = db::path(&home);
        let now = 1_792_000_000;
        drop(schema_2_database(&file, now));
        let openers: Vec<_> = (0..8)
            .map(|i| {
                let file = file.clone();
                std::thread::spawn(move || {
                    let (conn, moved) = db::open(&file, now).unwrap();
                    db::event(&conn, i, "ready", now).unwrap();
                    moved
                })
            })
            .collect();
        for o in openers {
            assert_eq!(o.join().unwrap(), None, "a database was moved aside");
        }
        assert!(!home.join("state/analytics.db.schema-2").exists());
        let conn = Connection::open(&file).unwrap();
        assert_eq!(one::<i64>(&conn, "PRAGMA user_version"), db::SCHEMA_VERSION);
        assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM runs"), 1);
        assert_eq!(one::<i64>(&conn, "SELECT count(*) FROM issue_events"), 8);
    }
}

#[test]
fn a_database_another_program_migrated_meanwhile_is_left_as_it_is() {
    let home = scratch("migrate-race");
    let file = db::path(&home);
    let now = 1_792_000_000;
    drop(schema_2_database(&file, now));
    // This one read user_version 2; the other took the write lock first.
    let mut late = Connection::open(&file).unwrap();
    db::open(&file, now).unwrap();
    assert!(db::settle(&mut late, 2, now).is_ok());
    assert_eq!(one::<i64>(&late, "PRAGMA user_version"), db::SCHEMA_VERSION);
}

#[test]
fn a_connection_open_through_the_migration_goes_on_working() {
    let home = scratch("migrate-open");
    let file = db::path(&home);
    let now = 1_792_000_000;
    let old = schema_2_database(&file, now);
    // A reader in the middle of a read does not hold the migration up, and
    // goes on seeing the rows it was reading.
    let reader = Connection::open(&file).unwrap();
    reader.execute_batch("BEGIN").unwrap();
    assert_eq!(one::<i64>(&reader, "SELECT count(*) FROM runs"), 1);
    let (_conn, moved) = db::open(&file, now).unwrap();
    assert_eq!(moved, None);
    assert_eq!(one::<i64>(&reader, "SELECT count(*) FROM runs"), 1);
    reader.execute_batch("COMMIT").unwrap();
    assert_eq!(
        one::<i64>(
            &reader,
            "SELECT count(*) FROM pragma_table_info('runs') WHERE name = 'driver'"
        ),
        1
    );
    // A schema 2 program that had the file open writes on, by its columns.
    old.execute(
        "INSERT INTO runs (id, kind, issue, started, outcome) VALUES ('work-8-1', 'work', 8, 1, 'pr')",
        [],
    )
    .unwrap();
    assert_eq!(
        one::<i64>(&old, "SELECT count(*) FROM runs WHERE driver IS NULL"),
        2
    );
}

#[test]
fn a_write_lock_held_through_the_migration_is_an_error_and_changes_nothing() {
    let home = scratch("migrate-busy");
    let file = db::path(&home);
    let now = 1_792_000_000;
    let holder = schema_2_database(&file, now);
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    let mut late = Connection::open(&file).unwrap();
    late.busy_timeout(std::time::Duration::ZERO).unwrap();
    assert!(matches!(
        db::settle(&mut late, 2, now),
        Err(db::Unusable::Busy(_))
    ));
    holder.execute_batch("COMMIT").unwrap();
    assert_eq!(one::<i64>(&late, "PRAGMA user_version"), 2);
    assert!(db::settle(&mut late, 2, now).is_ok());
    assert_eq!(one::<i64>(&late, "PRAGMA user_version"), db::SCHEMA_VERSION);
}

#[test]
fn a_write_lock_held_past_the_busy_timeout_is_waited_out() {
    let home = scratch("migrate-long-lock");
    let file = db::path(&home);
    let now = 1_792_000_000;
    let holder = schema_2_database(&file, now);
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

#[test]
fn a_migration_that_fails_takes_nothing_with_it_and_moves_nothing() {
    let home = scratch("migrate-fails");
    let file = db::path(&home);
    let now = 1_792_000_000;
    let c = schema_2_database(&file, now);
    c.execute_batch("ALTER TABLE runs ADD COLUMN driver TEXT")
        .unwrap();
    drop(c);
    assert!(db::open(&file, now).is_err());
    let c = Connection::open(&file).unwrap();
    assert_eq!(one::<i64>(&c, "PRAGMA user_version"), 2);
    assert_eq!(
        one::<String>(&c, "SELECT value FROM meta WHERE key = 'schema'"),
        "2"
    );
    assert_eq!(one::<i64>(&c, "SELECT count(*) FROM runs"), 1);
    assert!(!home.join("state/analytics.db.schema-2").exists());
}

#[test]
fn a_database_of_schema_1_is_still_kept_aside() {
    let home = scratch("schema-1");
    let file = db::path(&home);
    {
        let c = Connection::open(&file).unwrap();
        c.execute_batch(
            "CREATE TABLE runs (x); INSERT INTO runs VALUES (42); PRAGMA user_version = 1",
        )
        .unwrap();
    }
    let (conn, moved) = db::open(&file, 1_792_000_000).unwrap();
    assert_eq!(
        moved,
        Some(home.join("state/analytics.db.schema-1")),
        "schema 1 has no migration"
    );
    assert_eq!(one::<i64>(&conn, "PRAGMA user_version"), db::SCHEMA_VERSION);
}

#[test]
fn a_run_stores_the_driver_its_approval_bound_and_nothing_else() {
    let home = scratch("driver");
    let started = secs("2026-10-18T10:16:39Z");
    for (issue, approval, want) in [
        (1, json!({"how": "label", "driver": "lean"}), Some("lean")),
        (
            2,
            json!({"how": "label", "driver": "classic"}),
            Some("classic"),
        ),
        (3, json!({"how": "label"}), None),
        (4, json!({"how": "label", "driver": null}), None),
        (5, json!({"how": "label", "driver": "fast"}), None),
        (6, json!({"how": "label", "driver": "LEAN"}), None),
        (7, json!({"how": "label", "driver": "le an"}), None),
        (8, json!({"how": "label", "driver": 1}), None),
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
    let state = home.join("state");
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
    execute(&home, &Cmd::Run(7, given(&home, 7)), now).unwrap();
    execute(&home, &Cmd::Event(7, "ready".into(), None), now).unwrap();
    let out = home.join("copy.db");
    execute(&home, &Cmd::Export(out.clone(), false), now).unwrap();
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
        execute(&home, &Cmd::Export(out.clone(), true), now)
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
            rusqlite::params![
                id,
                issue,
                started,
                started + secs,
                secs,
                outcome,
                tokens,
                tv
            ],
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
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
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
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
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
    // The year is bounded here, before the arithmetic that would overflow on a
    // large one: a ledger's stamp is read only within these two years, and
    // `report::ledger_at` asks only whether the report keeps that year.
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
        parse(&args("export /tmp/x.db")),
        Some(Cmd::Export("/tmp/x.db".into(), false))
    );
    assert_eq!(
        parse(&args("export /tmp/x.db --json")),
        Some(Cmd::Export("/tmp/x.db".into(), true))
    );
    assert_eq!(parse(&args("report --json")), Some(Cmd::Report));
    assert_eq!(parse(&args("report")), Some(Cmd::ReportText));
    for bad in [
        "run",
        "run 0",
        "run 012",
        "run -1",
        "run x",
        "event 12 shipped",
        "prune now",
        "sample now",
        "report --text",
        "report --json now",
        "export",
        "export --json",
        "export -x.db",
        "export x.db --yaml",
        "export x.db --json --json",
        "export x.db --json now",
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
    let r = run::read(&home, 7, &given(&home, 7)).unwrap();
    assert_eq!(r.result_schema, None);
    assert_eq!(r.runner_ms, None);
}

// The live files of one poll, as libexec/status, libexec/dispatch and
// console energy write them, at `at`.
fn live(home: &Path, at: i64, why: &str, limit_room: Option<i64>) {
    fs::create_dir_all(home.join("live")).unwrap();
    let issue = |n: i64| json!({"number": n, "title": "t", "url": "u"});
    put(
        home,
        "live/status.json",
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
    // Five slots: two running and three free.
    assert_eq!(
        sample_row(&home),
        format!("{now} 3 2 1 0 2 5 1 [awaiting] 412.5")
    );

    // The limit of tehas limit allows fewer; nothing holds.
    live(&home, now + 90, "", Some(1));
    execute(&home, &Cmd::Sample, now + 100).unwrap();
    assert!(sample_row(&home).ends_with(&format!(" | {} 3 2 1 0 2 3 1 [] 412.5", now + 100)));
}

#[test]
fn a_sample_reads_the_report_the_dispatcher_writes() {
    // The dispatcher's own type, taken through to_value and stamped with the
    // poll's time: the bytes libexec/dispatch publishes. The sampler reads its
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
        slots: Some(json!({"cycles": 6, "cyclesHow": "auto"})),
    };
    let mut v = r.to_value();
    v[tehas_core::hold::KEY_TIME] = json!(iso(now - 20));
    put(&home, "live/dispatch.json", &v.to_string());
    execute(&home, &Cmd::Sample, now).unwrap();
    // The limit of tehas limit is the lower ceiling: count plus limitRoom,
    // three, though count plus free would give seven.
    assert_eq!(
        sample_row(&home),
        format!("{now} 3 2 1 0 2 3 1 [limit] 412.5")
    );
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
    put(
        &home,
        "live/dispatch.json",
        &json!({"time": iso(now - 200), "why": "count", "count": 1, "free": 0}).to_string(),
    );
    execute(&home, &Cmd::Sample, now + 1).unwrap();
    assert!(sample_row(&home).ends_with(&format!(" | {} 3 2 1 0 2 - 1 - 412.5", now + 1)));

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
        &json!({"generatedAt": iso(now), "paused": "no", "queued": 3, "running": [], "awaiting": {}, "stopped": null,
                "planning": "2"})
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
        put(&home, "live/status.json", &s.to_string());
    };
    fs::create_dir_all(home.join("live")).unwrap();
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
    assert_eq!(sample_row(&home), format!("{now} 3 2 1 0 2 5 1 [] 412.5"));
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

// A planning turn of issue 7 in flight: its record as the poll leaves
// it when the turn has ended, as facts, and what libexec/plan wrote beside it.
fn turn(home: &Path, record: Value, out: Option<Value>) {
    fs::write(facts_path(home, 7), record.to_string()).unwrap();
    if let Some(o) = out {
        put(home, "state/plan-7.json.out", &o.to_string());
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
               "answers": [{"turn": 2, "by": "alice", "fixer": false}]}),
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
        "SELECT count(*) FROM runs WHERE coalesce(input, output, cache_read, cache_write, dispatches, cost,
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
    let fixer = json!([{"turn": 3, "by": "fixer-bot", "fixer": true}]);
    let millwright = json!([{"turn": 3, "by": "millwright-bot", "millwright": true}]);
    for (rc, record, out, outcome, status, how) in [
        (
            "0",
            rec(0, json!([])),
            Some(json!({"session": "s", "text": "t"})),
            "handoff",
            "-",
            "label",
        ),
        (
            "0",
            rec(0, json!([])),
            Some(json!({"text": "t"})),
            "failed",
            "-",
            "label",
        ),
        (
            "2",
            rec(2, fixer.clone()),
            Some(json!({"session": "s"})),
            "failed",
            "-",
            "millwright",
        ),
        // An answer marked `millwright` is the millwright's too, and a record
        // marked with neither word is an answer of a person.
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
        ("4", rec(0, json!([])), None, "refused", "-", "label"),
        ("64", rec(0, json!([])), None, "refused", "-", "label"),
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
fn plan_how_words_are_the_pair_of_the_core_grammar() {
    // Each reader of the class has a test of its own that its words are these
    // two: this is analytics', beside the MCP server's and the core pair's.
    assert_eq!(
        crate::plan::MILLWRIGHT_MARKS,
        tehas_core::grammar::MILLWRIGHT_WORDS
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
    // Each word of the pair marks the turn the millwright's and neither word
    // is an answer: were the marks anything else, one of these three disagrees.
    assert_eq!(
        mark(json!({"turn": 3, "by": "bot", "millwright": true})),
        "millwright"
    );
    assert_eq!(
        mark(json!({"turn": 3, "by": "bot", "fixer": true})),
        "millwright"
    );
    assert_eq!(
        mark(json!({"turn": 3, "by": "bot", "millwright": false, "fixer": false})),
        "answer"
    );
}

#[test]
fn a_refining_session_s_first_turn_is_how_refine_and_its_later_turns_are_answers() {
    let now = 1_792_000_000;
    let at = iso(now - 60);
    for (turns, label, how) in [
        (0, Some("tehas:refine"), "refine"),
        (0, Some("tehas:plan"), "label"),
        (0, None, "label"),
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
fn the_refine_figures_follow_the_refined_issues_and_compare_them_with_the_rest() {
    let home = scratch("report-refine");
    let day0 = 1_792_022_400; // 2026-10-15T00:00:00Z
    let to = day0 + 9 * DAY + 43_200;
    let (conn, _) = db::open(&db::path(&home), day0).unwrap();
    // Two refining sessions in the window, one before it, and a plain plan.
    for (issue, started, how) in [
        (1, day0 + 100, "refine"),
        (2, day0 + 200, "refine"),
        (3, day0 - DAY, "refine"),
        (4, day0 + 300, "label"),
    ] {
        conn.execute(
            "INSERT INTO runs (id, kind, issue, how, started, ended) VALUES (?1, 'plan', ?2, ?3, ?4, ?4)",
            rusqlite::params![format!("plan-{issue}-{started}"), issue, how, started],
        )
        .unwrap();
    }
    // Refined issues 10 (started and shipped after 2 h) and 11 (not started);
    // other issues 20 (shipped) and 21 (started only).
    let ev = |issue: i64, event: &str, at: i64| {
        conn.execute(
            "INSERT INTO issue_events (at, issue, event) VALUES (?1, ?2, ?3)",
            rusqlite::params![at, issue, event],
        )
        .unwrap();
    };
    ev(10, "ready", day0 + 1000);
    ev(10, "started", day0 + 1100);
    ev(10, "merged", day0 + 1000 + 7200);
    ev(11, "ready", day0 + 2000);
    ev(20, "ready", day0 + 3000);
    ev(20, "started", day0 + 3100);
    ev(20, "merged", day0 + 9000);
    ev(21, "ready", day0 + 4000);
    ev(21, "started", day0 + 4100);
    std::fs::create_dir_all(home.join("state")).unwrap();
    std::fs::write(
        home.join("state").join("refined.jsonl"),
        format!(
            "{}\n{}\nnot json\n{}\n",
            json!({"at": iso(day0 + 900), "source": 1, "filed": 10, "url": "u"}),
            json!({"at": iso(day0 + 1900), "source": 2, "filed": 11, "url": "u"}),
            json!({"at": iso(day0 + 1950), "source": 2, "filed": 0, "url": "u"}),
        ),
    )
    .unwrap();
    let refined = crate::report::read_refined(&home);
    assert_eq!(refined.len(), 2);
    let got = crate::report::refine(&conn, &refined, to).unwrap();
    assert_eq!(
        got,
        json!({"sessions": 2, "n": 2, "started": 1, "shipped": 1, "shippedShare": 0.5,
               "leadTime": {"n": 1, "median": 7200, "p90": 7200},
               "others": {"n": 2, "started": 2, "shipped": 1, "shippedShare": 0.5}})
    );
    let mut report = crate::report::build(&conn, to).unwrap();
    report["refine"] = got;
    let text = crate::report::text(&report);
    assert!(
        text.contains("Refine             2 sessions, 2 issues filed: 1 started, 1 shipped (50%), lead median 2h00m; other issues 50% shipped (n=2)"),
        "{text}"
    );
    // No ledger: no refined issue, and the report says so in numbers.
    assert!(crate::report::read_refined(&scratch("report-refine-none")).is_empty());
}

#[test]
fn the_size_figures_follow_the_measured_starts_of_the_window() {
    let home = scratch("report-sizes");
    let day0 = 1_792_022_400; // 2026-10-15T00:00:00Z
    let to = day0 + 9 * DAY + 43_200;
    let (conn, _) = db::open(&db::path(&home), day0).unwrap();
    let ev = |issue: i64, event: &str, at: i64| {
        conn.execute(
            "INSERT INTO issue_events (at, issue, event) VALUES (?1, ?2, ?3)",
            rusqlite::params![at, issue, event],
        )
        .unwrap();
    };
    // 1: two files, shipped after 2 h, and measured twice -- the later line wins.
    // 2: two files, measured before the window, and shipped.
    // 3: three files, shipped after 1 h. 4: five files, never shipped.
    // 5: no file, shipped after 3 h. 6: no file, started only.
    // 7: two files, merged before its measured start: an old merge, no ship.
    // 8: one file, merged with no ready event at all: shipped, no lead.
    // 9: four files, shipped after 4 h. 10: four files, never shipped.
    // 11: measured in the window, no line for it at all in the ledger.
    for (issue, ready, merged) in [
        (1, day0 + 1000, day0 + 8200),
        (2, day0 + 2000, day0 + 9_200),
        (3, day0 + 3000, day0 + 6600),
        (5, day0 + 4000, day0 + 14_800),
        (9, day0 + 7000, day0 + 21_400),
    ] {
        ev(issue, "ready", ready);
        ev(issue, "merged", merged);
    }
    // An issue of a previous cycle that merged long before this measurement.
    ev(7, "ready", day0 - 40 * DAY);
    ev(7, "merged", day0 - 39 * DAY);
    // A merge the view knows of with no ready event behind it.
    ev(8, "merged", day0 + 7200);
    for issue in [4, 6, 10] {
        ev(issue, "ready", day0 + 5000);
    }
    ev(6, "started", day0 + 5100);
    // The last line of an issue wins by its `at`, not by its place in the
    // file: issue 1's later measure stands first here and still answers.
    let mut wide = json!({"at": iso(day0 + 6100), "issue": 12, "files": 3});
    wide["filler"] = json!("x".repeat(100_000));
    put(
        &home,
        "state/sizes.jsonl",
        &ledger(&[
            json!({"at": iso(day0 + 1000), "issue": 1, "files": 2, "tests": 1,
                  "deliverables": 1, "done_when": 2}),
            json!({"at": iso(day0 + 900), "issue": 1, "files": 5, "tests": 0,
                   "deliverables": 1, "done_when": 0}),
            json!({"at": iso(day0 - DAY), "issue": 2, "files": 2}),
            json!({"at": iso(day0 + 2000), "issue": 3, "files": 3}),
            json!({"at": iso(day0 + 3000), "issue": 4, "files": 9}),
            json!("not json"),
            json!({"at": iso(day0 + 3100), "issue": 4, "files": "many"}),
            json!({"at": "not a time", "issue": 5, "files": 0}),
            json!({"at": iso(day0 + 4000), "issue": 5, "files": 0}),
            json!({"at": iso(day0 + 5000), "issue": 6, "files": 0}),
            // A merge of an earlier cycle, before this start: no ship.
            json!({"at": iso(day0 + 6000), "issue": 7, "files": 2}),
            // A merge with no ready behind it: a ship of no lead.
            json!({"at": iso(day0 + 6200), "issue": 8, "files": 1}),
            json!({"at": iso(day0 + 7000), "issue": 9, "files": 4}),
            json!({"at": iso(day0 + 7100), "issue": 10, "files": 4}),
            // Of the four counts of a line only `files` is read; the rest of
            // what the ledger holds cannot reach the report.
            json!({"at": iso(day0 + 7200), "issue": 10, "files": 4, "tests": 9,
                   "deliverables": "words", "done_when": ["a", "b"]}),
            // Of another shape than a record: no line.
            json!({"at": iso(day0 + 7300), "issue": 11, "files": null}),
            json!({"at": iso(day0 + 7400), "issue": 11, "files": 1.5}),
            json!(42),
            json!([{"at": iso(day0 + 7500), "issue": 11, "files": 1}]),
            // A line too wide to be a measure, and a stamp outside the years
            // the report keeps.
            wide,
            // A stamp outside the years the report keeps: a corrupt or hostile line,
            // and the year's own size is no reason to answer for it.
            json!({"at": "2263-01-01T00:00:00Z", "issue": 13, "files": 2}),
            json!({"at": "1990-01-01T00:00:00Z", "issue": 13, "files": 2}),
            json!({"at": iso(day0 + 8000), "issue": 13, "files": 2}),
        ]),
    );
    let ledger = crate::report::read_sizes(&home);
    // In issue order, the order the map's key gives it.
    let seen: Vec<(i64, i64, i64)> = ledger.iter().map(|s| (s.issue, s.files, s.at)).collect();
    assert_eq!(
        seen,
        [
            (1, 2, day0 + 1000),
            (2, 2, day0 - DAY),
            (3, 3, day0 + 2000),
            (4, 9, day0 + 3000),
            (5, 0, day0 + 4000),
            (6, 0, day0 + 5000),
            (7, 2, day0 + 6000),
            (8, 1, day0 + 6200),
            (9, 4, day0 + 7000),
            (10, 4, day0 + 7200),
            (13, 2, day0 + 8000),
        ]
    );
    let empty = json!({"n": 0, "shipped": 0, "leadTime": {"n": 0}});
    assert_eq!(empty_buckets()["3-4"], empty);
    let got = crate::report::sizes(&conn, &ledger, to).unwrap();
    assert_eq!(
        got,
        json!({
            "none": {"n": 2, "shipped": 1, "leadTime": {"n": 1, "median": 10800, "p90": 10800}},
            "1-2": {"n": 4, "shipped": 2, "leadTime": {"n": 1, "median": 7200, "p90": 7200}},
            "3-4": {"n": 3, "shipped": 2, "leadTime": {"n": 2, "median": 9000, "p90": 14400}},
            "5+": {"n": 1, "shipped": 0, "leadTime": {"n": 0}},
        })
    );
    // The window holds the starts measured in it: issue 2, measured nine days
    // before, is in none of the buckets although it shipped.
    assert_eq!(got["1-2"]["n"], json!(4));
    assert_eq!(got["1-2"]["shipped"], json!(2));
    // Issue 7 merged before its measured start, so the window shipped nothing
    // of it; issue 8 shipped with no ready event, so it counts as shipped and
    // brings no lead: where shipped outruns leadTime.n, a ship had no lead.
    assert_eq!(got["1-2"]["leadTime"]["n"], json!(1));
    // The boundaries of the buckets: one file is "1-2", four is "3-4", and
    // "5+" holds every count above them together.
    assert_eq!(got["3-4"]["n"], json!(3));
    assert_eq!(got["5+"]["n"], json!(1));
    // The same ledger with no work in the window: no bucket but "none".
    assert_eq!(crate::report::sizes(&conn, &[], to).unwrap()["3-4"], empty);
    // No ledger at all: empty buckets, not a missing key.
    assert_eq!(
        crate::report::sizes(
            &conn,
            &crate::report::read_sizes(&scratch("report-sizes-none")),
            to
        )
        .unwrap(),
        empty_buckets()
    );
    let text = crate::report::text(&crate::report::complete(&conn, &home, to).unwrap());
    // The Size line names each bucket by name, not by column width.
    assert!(
        text.contains("none: 2 shipped 1, lead 3h00m | 1-2: 4 shipped 2, lead 2h00m | 3-4: 3 shipped 2, lead 2h30m | 5+: 1 shipped 0, lead -"),
        "{text}"
    );
}

// The report command, not build(), adds the ledger figures: the same database
// and ledger reach the printed report through the command that reads them.
#[test]
fn the_report_command_adds_the_refine_and_size_figures() {
    let home = scratch("report-complete");
    let day0 = 1_792_022_400; // 2026-10-15T00:00:00Z
    let to = day0 + 9 * DAY + 43_200;
    let (conn, _) = db::open(&db::path(&home), day0).unwrap();
    conn.execute(
        "INSERT INTO issue_events (at, issue, event) VALUES (?1, ?2, 'ready'), (?1, ?2, 'started'), (?3, ?2, 'merged')",
        rusqlite::params![day0 + 1000, 1, day0 + 4600],
    )
    .unwrap();
    std::fs::create_dir_all(home.join("state")).unwrap();
    put(
        &home,
        "state/sizes.jsonl",
        &ledger(&[json!({"at": iso(day0 + 1000), "issue": 1, "files": 2})]),
    );
    let got = crate::report::complete(&conn, &home, to).unwrap();
    assert_eq!(
        got["sizes"]["1-2"],
        json!({"n": 1, "shipped": 1,
                   "leadTime": {"n": 1, "median": 3600, "p90": 3600}})
    );
    assert_eq!(
        got["sizes"]["none"],
        json!({"n": 0, "shipped": 0, "leadTime": {"n": 0}})
    );
    assert!(got["refine"].is_object());
    // build() alone holds neither: the keys belong to the command.
    let bare = crate::report::build(&conn, to).unwrap();
    assert!(bare.get("sizes").is_none() && bare.get("refine").is_none());
    // A ledger that is a link is read no more than a ledger that is no file:
    // the figures are four buckets of nothing.
    let moved = home.join("elsewhere.jsonl");
    fs::rename(home.join("state").join("sizes.jsonl"), &moved).unwrap();
    std::os::unix::fs::symlink(&moved, home.join("state").join("sizes.jsonl")).unwrap();
    assert!(crate::report::read_sizes(&home).is_empty());
    let got = crate::report::complete(&conn, &home, to).unwrap();
    assert_eq!(got["sizes"], empty_buckets());
    // The Size line says the buckets by name.
    assert!(
        crate::report::text(&got).contains("none: 0 shipped 0, lead - | 1-2: 0 shipped 0, lead - | 3-4: 0 shipped 0, lead - | 5+: 0 shipped 0, lead -"),
    );
}

// The published privacy claim of the size figures, held where it is made:
// one bucket is three counts and nothing else, whatever the ledger's own lines
// hold beside them.
#[test]
fn the_size_figures_hold_no_keys_but_their_own() {
    let home = scratch("report-sizes-keys");
    let day0 = 1_792_022_400;
    let to = day0 + 9 * DAY + 43_200;
    let (conn, _) = db::open(&db::path(&home), day0).unwrap();
    std::fs::create_dir_all(home.join("state")).unwrap();
    put(
        &home,
        "state/sizes.jsonl",
        &ledger(&[
            json!({"at": iso(day0 + 1000), "issue": 1, "files": 2, "tests": 1,
                   "deliverables": 3, "done_when": 4, "title": "what the issue asked for"}),
        ]),
    );
    let got = crate::report::sizes(&conn, &crate::report::read_sizes(&home), to).unwrap();
    assert_eq!(
        got.as_object().unwrap().keys().collect::<Vec<_>>(),
        ["1-2", "3-4", "5+", "none"]
    );
    for b in got.as_object().unwrap().values() {
        assert_eq!(
            b.as_object().unwrap().keys().collect::<Vec<_>>(),
            ["leadTime", "n", "shipped"]
        );
    }
    assert!(!got.to_string().contains("asked for"));
}

// The two whole-file bounds of a ledger, pinned in both directions: the reader
// takes a file of the writer's own making and refuses one that is a byte too
// long, and the size of a `files` count is bounded too.
#[test]
fn a_ledger_past_the_whole_file_bound_is_no_ledger_at_all() {
    let home = scratch("report-sizes-bounds");
    let day0 = 1_792_022_400;
    let to = day0 + 9 * DAY + 43_200;
    let (conn, _) = db::open(&db::path(&home), day0).unwrap();
    std::fs::create_dir_all(home.join("state")).unwrap();
    // The 500 lines the writers keep: read whole, and of one issue each.
    let starts: Vec<Value> = (1..=500)
        .map(|i| json!({"at": iso(day0 + 1000 + i), "issue": i, "files": 2}))
        .collect();
    put(&home, "state/sizes.jsonl", &ledger(&starts));
    assert_eq!(crate::report::read_sizes(&home).len(), 500);
    // A line one byte short of the width bound is a record; one at it is a
    // paste. Both sit beside a good line, so the wide one costs only itself.
    // The width is measured on the printed line, so the filler is solved for.
    let at = iso(day0 + 1000);
    let prefix_of = |issue: i64| {
        format!(
            "{},{}",
            json!({"at": at.clone(), "issue": issue, "files": 2}),
            "\"filler\":"
        )
    };
    let wide_of = |issue: i64, len: usize| {
        let mut w = json!({"at": at.clone(), "issue": issue, "files": 2});
        w["filler"] = json!("x".repeat(len - prefix_of(issue).len() - 2));
        assert_eq!(w.to_string().len(), len);
        w
    };
    // The printed line of a two-file start with a one-digit issue is 49
    // bytes; the filler is solved for below, and this pins it against that.
    assert_eq!(
        json!({"at": at, "issue": 900, "files": 2})
            .to_string()
            .len(),
        51
    );
    put(
        &home,
        "state/sizes.jsonl",
        &ledger(&[
            json!({"at": at.clone(), "issue": 900, "files": 2}),
            wide_of(901, 99_999),
        ]),
    );
    assert_eq!(crate::report::read_sizes(&home).len(), 2);
    put(
        &home,
        "state/sizes.jsonl",
        &ledger(&[
            json!({"at": at.clone(), "issue": 900, "files": 2}),
            wide_of(902, 100_000),
        ]),
    );
    assert_eq!(crate::report::read_sizes(&home).len(), 1);
    // One byte past the whole-file bound is no ledger: the figures are four
    // buckets of nothing, as they are for no file at all.
    let one_mib = 1usize << 20;
    let mut big = ledger(&starts);
    big.push_str(&"y".repeat(one_mib - big.len() + 1));
    assert_eq!(big.len(), one_mib + 1);
    put(&home, "state/sizes.jsonl", &big);
    assert!(crate::report::read_sizes(&home).is_empty());
    assert_eq!(
        crate::report::sizes(&conn, &crate::report::read_sizes(&home), to).unwrap(),
        empty_buckets()
    );
    // The last byte within the bound is read: the same file one byte shorter
    // answers for all 500 starts.
    put(&home, "state/sizes.jsonl", &big[..one_mib]);
    assert_eq!(crate::report::read_sizes(&home).len(), 500);
    // A `files` count of more than the nine digits the setting may hold is no
    // measure, whatever bucket it would have fallen in.
    put(
        &home,
        "state/sizes.jsonl",
        &ledger(&[
            json!({"at": iso(day0 + 1000), "issue": 903, "files": 999_999_999}),
            json!({"at": iso(day0 + 1000), "issue": 904, "files": 1_000_000_000}),
            json!({"at": iso(day0 + 1000), "issue": 905, "files": -1}),
        ]),
    );
    assert_eq!(
        crate::report::read_sizes(&home)
            .iter()
            .map(|s| s.issue)
            .collect::<Vec<_>>(),
        [903]
    );
    // A stamp of an eleven-digit year is refused at the parse, so neither
    // ledger can panic the report with one.
    for name in ["sizes.jsonl", "refined.jsonl"] {
        put(
            &home,
            &format!("state/{name}"),
            &ledger(&[
                json!({"at": "71167994112-01-01T00:00:00Z", "issue": 1, "files": 2,
                            "source": 1, "filed": 1, "url": "https://x/y/issues/1"}),
            ]),
        );
        assert!(crate::report::read_sizes(&home).is_empty(), "{name}");
        assert!(crate::report::read_refined(&home).is_empty(), "{name}");
    }
}

// The rows the terminal knows are exactly the sections the report command
// The rows the terminal prints conditionally are exactly the sections that
// carry them: `text` renders one row per entry of `SECTIONS`, and no row for a
// section the report does not hold.
#[test]
fn the_text_report_prints_one_row_for_every_section_of_the_report() {
    let home = scratch("report-text-sections");
    let day0 = 1_792_022_400;
    let to = day0 + 9 * DAY + 43_200;
    let (conn, _) = db::open(&db::path(&home), day0).unwrap();
    report_data(&conn, day0, to);
    let built = crate::report::build(&conn, to).unwrap();
    let full = crate::report::complete(&conn, &home, to).unwrap();
    let keys = |v: &Value| {
        v.as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<String>>()
    };
    // The five sections of the contract: each holds an object of its own and
    // prints the row `SECTIONS` names, so a section without a row and a row
    // without a section both fail here. The two of the ledgers come with
    // `complete`, the two of the database with `build`.
    let full_text = crate::report::text(&full);
    let built_text = crate::report::text(&built);
    let added = keys(&full)
        .into_iter()
        .filter(|k| !keys(&built).contains(k))
        .collect::<Vec<_>>();
    assert_eq!(added, [crate::report::REFINE, crate::report::SIZES]);
    for (key, label) in crate::report::SECTIONS {
        let from_ledger = key == crate::report::REFINE || key == crate::report::SIZES;
        let (report, text) = if from_ledger {
            (&full, &full_text)
        } else {
            (&built, &built_text)
        };
        assert!(report[key].is_object(), "{key} is a section of the report");
        let line = text
            .lines()
            .find(|l| l.starts_with(label))
            .unwrap_or_else(|| panic!("{label} is the row of the {key} section:\n{text}"));
        assert!(line.len() > label.len() + 1, "{key} prints a value: {line}");
        // And no row without its section.
        if from_ledger {
            assert!(!keys(&built).contains(&key.to_string()));
            assert!(
                !built_text.contains(label),
                "{label} prints without its section"
            );
        }
    }
}

// The size figures are read within the time the caller gives the report, the
// ledger path included: `leads` reads the view once for the whole ledger.
#[test]
fn the_size_figures_of_a_full_ledger_come_in_inside_the_time_limit() {
    let home = scratch("report-sizes-load");
    let to = 1_792_022_400 + 10 * DAY;
    let (mut conn, _) = db::open(&db::path(&home), to).unwrap();
    let tx = conn.transaction().unwrap();
    // Four hundred shipped issues, and a measured start for each of them.
    for i in 0..400_i64 {
        let started = to - 10 * DAY + i * 60;
        db::event(&tx, i, "ready", started).unwrap();
        db::event(&tx, i, "merged", started + 3600).unwrap();
    }
    tx.commit().unwrap();
    let starts: Vec<Value> = (0..400)
        .map(|i| json!({"at": iso(to - 3600), "issue": i, "files": i % 7}))
        .collect();
    put(&home, "state/sizes.jsonl", &ledger(&starts));
    let t = std::time::Instant::now();
    assert_eq!(execute(&home, &Cmd::Report, to), Ok(()));
    let took = t.elapsed();
    // The limit is 5 s; the ledger's join is one view read, not one per line.
    assert!(took.as_secs_f64() < 2.0, "the report took {took:?}");
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
    put(&home, "state/plan-7.json", &in_flight.to_string());
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

// The known data set of the report's tests: six work runs in the window on
// three version pairs, one before it, a planning turn, three shipped issues,
// six samples (one before the window) and the gate of three of the runs.
//
// `gate` is each run of the product's gate the runs made: its id, a text
// command, the tier word, the step word it ran in, whether it passed, and its
// own milliseconds. d and the run before the window hold none, so they are in
// no gate figure.
#[allow(clippy::type_complexity)]
const REPORT_GATES: [(&str, &str, &str, &str, i64, i64); 5] = [
    (
        "c",
        "scripts/verify.sh --fast",
        "fast",
        "develop",
        1,
        300_000,
    ),
    (
        "c",
        "scripts/verify.sh --fast",
        "fast",
        "review",
        1,
        300_000,
    ),
    ("e", "scripts/verify.sh", "full", "develop", 0, 540_000),
    (
        "f",
        "scripts/verify.sh --fast",
        "fast",
        "develop",
        1,
        60_000,
    ),
    // Outside the window, and the run it belongs to has no end for its `at`:
    // neither is a figure the report reads.
    (
        "old",
        "scripts/verify.sh --fast",
        "fast",
        "develop",
        1,
        45_000,
    ),
];

fn report_data(conn: &Connection, day0: i64, to: i64) {
    #[allow(clippy::type_complexity)]
    let runs: [(
        &str,
        i64,
        i64,
        i64,
        &str,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        Option<&str>,
        Option<&str>,
        Option<&str>,
        Option<&str>,
        &str,
        Option<&str>,
    ); 7] = [
        (
            "a",
            1,
            day0 + 100,
            600,
            "handoff",
            Some(1000),
            None,
            None,
            Some("cap"),
            Some("explore-bodies-empty"),
            Some("explore"),
            Some("handoff"),
            "v1",
            Some("0.13.0"),
        ),
        (
            "b",
            1,
            day0 + 2000,
            1200,
            "merged",
            Some(3000),
            None,
            None,
            None,
            None,
            None,
            Some("review"),
            "v1",
            Some("0.13.0"),
        ),
        (
            "c",
            2,
            day0 + 3000,
            1800,
            "pr",
            Some(2000),
            Some(500),
            None,
            None,
            None,
            None,
            Some("implement"),
            "v1",
            Some("0.13.0"),
        ),
        (
            "d",
            3,
            day0 + DAY + 10,
            300,
            "failed",
            None,
            None,
            None,
            Some("timeout"),
            None,
            None,
            Some("explore"),
            "v2",
            Some("0.13.0"),
        ),
        (
            "e",
            4,
            day0 + DAY + 1000,
            900,
            "failed",
            Some(100),
            None,
            Some(50),
            Some("Weird Cause!"),
            None,
            None,
            Some("implement"),
            "v2",
            None,
        ),
        (
            "f",
            5,
            day0 + 2 * DAY,
            0,
            "refused",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            "v1",
            Some("0.13.0"),
        ),
        (
            "old",
            9,
            day0 - 1000,
            100,
            "failed",
            Some(5),
            None,
            None,
            Some("timeout"),
            None,
            None,
            None,
            "v0",
            Some("0.12.0"),
        ),
    ];
    for (
        id,
        issue,
        started,
        secs,
        outcome,
        input,
        output,
        cache,
        kind,
        cap,
        failed,
        last,
        tv,
        rv,
    ) in runs
    {
        conn.execute(
            "INSERT INTO runs (id, kind, issue, started, ended, seconds, outcome, input, output,
                               cache_read, failure_kind, cap, failed_step, last_step,
                               tehas_version, rukas_version, failure_cause)
             VALUES (?1, 'work', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                     'a cause no page may show')",
            rusqlite::params![
                id,
                issue,
                started,
                started + secs,
                secs,
                outcome,
                input,
                output,
                cache,
                kind,
                cap,
                failed,
                last,
                tv,
                rv
            ],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO runs (id, kind, issue, started, ended, seconds, outcome)
         VALUES ('p', 'plan', 6, ?1, ?2, 400, 'handoff')",
        [day0 + 100, day0 + 500],
    )
    .unwrap();
    // Three of the runs recorded the gate, in four of its runs of the window:
    // two 5-minute fast tiers in c, one 9-minute full tier in e and one
    // minute of fast in f. `gate_ms` is what the runner's lines said, summed,
    // in milliseconds, and the columns of run_gate_runs are the text, the tier
    // and the step of each of them, the words of the published vocabularies.
    for (id, command, tier, step, ok, ms) in REPORT_GATES {
        conn.execute(
            "INSERT INTO run_gate_runs (run_id, seq, command, tier, step, ok, ms, at)
             SELECT id, (SELECT count(*) + 1 FROM run_gate_runs g WHERE g.run_id = r.id),
                    ?2, ?3, ?4, ?5, ?6, coalesce(ended, started)
             FROM runs r WHERE id = ?1",
            rusqlite::params![id, command, tier, step, ok, ms],
        )
        .unwrap();
    }
    for (id, runs, ms) in [
        ("c", 2, 600_000),
        ("e", 1, 540_000),
        ("f", 1, 60_000),
        ("old", 1, 45_000),
    ] {
        conn.execute(
            "UPDATE runs SET gate_runs = ?2, gate_ms = ?3 WHERE id = ?1",
            rusqlite::params![id, runs, ms],
        )
        .unwrap();
    }
    let steps = [
        ("a", "explore", 0),
        ("a", "handoff", 300),
        ("b", "explore", 0),
        ("b", "implement", 300),
        ("b", "review", 900),
        ("c", "explore", 0),
        ("c", "implement", 400),
        ("d", "explore", 0),
        ("e", "explore", 0),
        ("e", "implement", 200),
    ];
    for (id, step, offset) in steps {
        conn.execute(
            "INSERT INTO run_steps (run_id, step, round, started)
             SELECT id, ?2, 1, started + ?3 FROM runs WHERE id = ?1",
            rusqlite::params![id, step, offset],
        )
        .unwrap();
    }
    db::event(conn, 1, "ready", day0).unwrap();
    db::event(conn, 2, "ready", day0 + 2900).unwrap();
    db::event(conn, 2, "merged", day0 + DAY + 100).unwrap();
    db::event(conn, 7, "merged", day0 + 2 * DAY).unwrap();
    db::event(conn, 3, "stopped", day0 + DAY + 400).unwrap();
    #[allow(clippy::type_complexity)]
    let samples: [(
        i64,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        Option<&str>,
        Option<f64>,
    ); 6] = [
        (day0 - 60, Some(9), Some(9), Some(0), Some(""), Some(999.0)),
        (day0 + DAY, Some(1), Some(0), Some(0), Some(""), Some(100.0)),
        (to - 1800, Some(2), Some(1), Some(0), Some(""), Some(100.0)),
        (to - 1200, Some(3), None, Some(1), None, Some(200.0)),
        (to - 600, Some(4), Some(2), Some(0), Some("resources"), None),
        (to - 300, Some(5), Some(1), Some(0), Some(""), Some(100.0)),
    ];
    for (at, in_line, planning, paused, held, watts) in samples {
        conn.execute(
            "INSERT INTO samples (at, in_line, planning, paused, held, watts)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![at, in_line, planning, paused, held, watts],
        )
        .unwrap();
    }
}

#[test]
fn the_report_gives_the_figures_of_a_known_data_set() {
    let home = scratch("report");
    let day0 = 1_792_022_400; // 2026-10-15T00:00:00Z
    let to = day0 + 9 * DAY + 43_200;
    let (conn, _) = db::open(&db::path(&home), day0).unwrap();
    report_data(&conn, day0, to);
    let got = crate::report::build(&conn, to).unwrap();
    let day = |d: &str, shipped: i64, runs: i64, failed: i64, minutes: i64| json!({"day": d, "shipped": shipped, "runs": runs, "failed": failed, "minutes": minutes});
    let mut trend: Vec<Value> = (15..=24)
        .map(|d| json!({"day": format!("2026-10-{d}")}))
        .collect();
    trend[1] = json!({"day": "2026-10-16", "n": 1});
    trend[9] = json!({"day": "2026-10-24", "n": 6});
    let want = json!({
        "report": 1,
        "from": day0,
        "to": to,
        "shipped": {"n": 3},
        "planned": {"n": 6, "inLine": 5, "planning": 1, "trend": trend},
        "runs": {"n": 6, "pr": 1, "merged": 1, "handoff": 1, "parked": 0, "failed": 2,
                 "refused": 1, "stopped": 0},
        "planningTurns": {"n": 1},
        "handoffRate": {"n": 6, "share": 0.167},
        // Issues 1 (two runs) and 2 (one); issue 7 has no run here.
        "firstPassYield": {"n": 2, "share": 0.5},
        "leadTime": {"n": 2, "median": 43_400, "p90": 83_600},
        "cycleTime": {"n": 6, "median": 750, "p90": 1800},
        // c, e and f recorded the gate: 300 s and 300 s in c, 540 s in e and
        // 60 s in f. The median of the four runs of the gate is 300 s and
        // their p90 540 s; 1.333 of them per run; 1200 s of gate in the 2700 s
        // of the three runs, so a mean share of 0.467.
        "gate": {"n": 3, "seconds": 1200, "median": 300, "p90": 540, "perRun": 1.333,
                 "share": 0.467},
        "machineTime": {"n": 6, "seconds": 4800, "perShipped": 1600},
        "tokens": {"n": 4, "total": 6650, "input": 6100, "output": 500, "cacheRead": 50,
                   "shipped": 3, "perShipped": 2217},
        // 1500 seconds of 734400 since the first sample: 600 + 600 + 300.
        "availability": {"n": 5, "seconds": 734_400, "share": 0.002},
        // 100 W for 600 s, 100 W for 600 s, 200 W for 600 s, 100 W for 300 s.
        "energy": {"n": 4, "kwh": 0.075, "perShipped": 0.025},
        "daily": [
            day("2026-10-15", 1, 3, 0, 60),
            day("2026-10-16", 1, 2, 2, 20),
            day("2026-10-17", 1, 1, 0, 0),
            day("2026-10-18", 0, 0, 0, 0),
            day("2026-10-19", 0, 0, 0, 0),
            day("2026-10-20", 0, 0, 0, 0),
            day("2026-10-21", 0, 0, 0, 0),
            day("2026-10-22", 0, 0, 0, 0),
            day("2026-10-23", 0, 0, 0, 0),
            day("2026-10-24", 0, 0, 0, 0),
        ],
        "stopReasons": {"n": 4, "top": [
            {"reason": "explore-bodies-empty", "n": 1},
            {"reason": "other", "n": 1},
            {"reason": "refused", "n": 1},
            {"reason": "timeout", "n": 1},
        ]},
        "stations": {"n": 5, "steps": [
            {"step": "explore", "n": 5, "passed": 3, "share": 0.6},
            {"step": "handoff", "n": 1, "passed": 1, "share": 1.0},
            {"step": "implement", "n": 3, "passed": 2, "share": 0.667},
            {"step": "review", "n": 1, "passed": 1, "share": 1.0},
        ]},
        // Three runs or more show their figures; fewer, the count only.
        "versions": [
            {"tehas": "v1", "rukas": "0.13.0", "n": 4, "prShare": 0.5, "medianSeconds": 900,
             "medianTokens": 2500, "tokensN": 3},
            {"tehas": "v2", "n": 1},
            {"tehas": "v2", "rukas": "0.13.0", "n": 1},
        ],
        "drivers": {
            "classic": {"n": 6, "ran": 5, "prShare": 0.4, "parkedShare": 0.2,
                        "minutes": {"n": 5, "total": 80, "median": 15},
                        "tokens": {"n": 4, "total": 6650, "median": 1750},
                        "pullRequests": {"n": 2, "merged": 2, "share": 1.0}},
            "lean": {"n": 0, "ran": 0, "minutes": {"n": 0}, "tokens": {"n": 0},
                     "pullRequests": {"n": 0, "merged": 0}},
        },
    });
    assert_eq!(got, want, "{}", serde_json::to_string_pretty(&got).unwrap());
    let text = got.to_string();
    assert!(!text.contains("cause no page"));
    assert!(!text.contains("Weird"));
}

#[test]
fn the_gate_line_of_the_report_gives_its_figures_in_text_and_json() {
    let home = scratch("report-gate");
    let day0 = 1_792_022_400; // 2026-10-15T00:00:00Z
    let to = day0 + 9 * DAY + 43_200;
    let (conn, _) = db::open(&db::path(&home), day0).unwrap();
    report_data(&conn, day0, to);
    let report = crate::report::build(&conn, to).unwrap();
    // The four gate rows of the window, read back as the report worked them
    // out: 300 s and 300 s in c, 540 s in e and 60 s in f.
    assert_eq!(
        report["gate"],
        json!({"n": 3, "seconds": 1200, "median": 300, "p90": 540, "perRun": 1.333,
               "share": 0.467})
    );
    // The text line holds the same four figures, with the seconds of the two
    // other durations of the report as minutes.
    let text = crate::report::text(&report);
    assert!(
        text.contains(
            "\nGate               median 5m, p90 9m, 1.333 runs per run, 46.7% of run time  (n=3, 20m)\n"
        ),
        "{text}"
    );
    // The JSON the report command prints holds the same key as `build`'s.
    let json = crate::report::complete(&conn, &home, to).unwrap();
    assert_eq!(json["gate"], report["gate"]);
    // A second row of e's that carries no milliseconds of its own, and no
    // tier: the count of the gate's runs of it rises to two, and its 540 s are
    // read as two of 270 s each, which is what the run's stream said of them
    // together. The report's figures follow the rows it was given.
    conn.execute(
        "INSERT INTO run_gate_runs (run_id, seq, command, tier, ok, ms, at)
         VALUES ('e', 2, 'curl https://models.example/verify', NULL, NULL, NULL, ?1)",
        [to],
    )
    .unwrap();
    conn.execute("UPDATE runs SET gate_runs = 2 WHERE id = 'e'", [])
        .unwrap();
    let again = crate::report::build(&conn, to).unwrap();
    assert_eq!(again["gate"]["n"], json!(3));
    assert_eq!(again["gate"]["median"], json!(270));
    assert_eq!(again["gate"]["p90"], json!(300));
    assert_eq!(again["gate"]["seconds"], json!(1200));
    // Two runs of the gate in e now: the mean rises, and the row says so. The
    // run's own share of its time is its recorded milliseconds over its own
    // seconds, whatever the count of them is.
    assert_eq!(again["gate"]["perRun"], json!(1.667));
    assert_eq!(again["gate"]["share"], json!(0.467));
    let text = crate::report::text(&again);
    assert!(
        text.contains("median 4m30s, p90 5m, 1.667 runs per run, 46.7% of run time  (n=3, 20m)"),
        "{text}"
    );
}

/// A work run that ended in the report's window, as the columns the drivers
/// are read from.
fn driver_run(
    conn: &Connection,
    id: &str,
    issue: i64,
    started: i64,
    outcome: &str,
    tokens: Option<i64>,
    driver: Option<&str>,
) {
    conn.execute(
        "INSERT INTO runs (id, kind, issue, started, ended, seconds, outcome, input, driver)
         VALUES (?1, 'work', ?2, ?3, ?3 + 600, 600, ?4, ?5, ?6)",
        rusqlite::params![id, issue, started, outcome, tokens, driver],
    )
    .unwrap();
}

#[test]
fn the_report_splits_by_driver_and_counts_a_null_row_as_classic() {
    let home = scratch("report-drivers");
    let day0 = 1_792_022_400;
    let to = day0 + DAY;
    let (conn, _) = db::open(&db::path(&home), day0).unwrap();
    // Classic: two pull requests, one merged by the driver, one parked run.
    driver_run(&conn, "a", 1, day0, "merged", Some(100), None);
    driver_run(&conn, "b", 2, day0, "pr", Some(200), Some("classic"));
    driver_run(&conn, "c", 3, day0, "parked", Some(300), None);
    driver_run(&conn, "d", 4, day0, "failed", Some(400), None);
    // Lean: no usage yet. One pull request a person merged later, one not.
    driver_run(&conn, "e", 5, day0, "pr", None, Some("lean"));
    driver_run(&conn, "f", 6, day0, "pr", None, Some("lean"));
    driver_run(&conn, "g", 7, day0, "parked", None, Some("lean"));
    db::event(&conn, 5, "merged", day0 + 3600).unwrap();
    let got = crate::report::build(&conn, to).unwrap();
    assert_eq!(
        got["drivers"],
        json!({
            "classic": {"n": 4, "ran": 4, "prShare": 0.5, "parkedShare": 0.25,
                        "minutes": {"n": 4, "total": 40, "median": 10},
                        "tokens": {"n": 4, "total": 1000, "median": 250},
                        "pullRequests": {"n": 2, "merged": 1, "share": 0.5}},
            "lean": {"n": 3, "ran": 3, "prShare": 0.667, "parkedShare": 0.333,
                     "minutes": {"n": 3, "total": 30, "median": 10},
                     "tokens": {"n": 0},
                     "pullRequests": {"n": 2, "merged": 1, "share": 0.5}},
        })
    );
    let text = crate::report::text(&got);
    let row = |name: &str| {
        text.lines()
            .find(|l| l.trim_start().starts_with(name))
            .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap()
    };
    assert_eq!(row("classic"), "classic 4 4 50% 25% 40 1000 2 1");
    assert_eq!(row("lean"), "lean 3 3 66.7% 33.3% 30 - (n=0) 2 1");
}

#[test]
fn a_drivers_shares_are_of_its_runs_that_ran() {
    let home = scratch("report-driver-ran");
    let day0 = 1_792_022_400;
    let (conn, _) = db::open(&db::path(&home), day0).unwrap();
    driver_run(&conn, "a", 1, day0, "pr", None, Some("lean"));
    driver_run(&conn, "b", 2, day0, "failed", None, Some("lean"));
    driver_run(&conn, "c", 3, day0, "refused", None, Some("lean"));
    driver_run(&conn, "d", 4, day0, "stopped", None, Some("lean"));
    let got = crate::report::build(&conn, day0 + DAY).unwrap();
    let lean = &got["drivers"]["lean"];
    assert_eq!(lean["n"], 4);
    assert_eq!(lean["ran"], 2);
    assert_eq!(lean["prShare"], 0.5);
    assert_eq!(lean["parkedShare"], 0.0);
}

#[test]
fn a_driver_with_only_refused_runs_has_no_shares() {
    let home = scratch("report-driver-refused");
    let day0 = 1_792_022_400;
    let (conn, _) = db::open(&db::path(&home), day0).unwrap();
    driver_run(&conn, "a", 1, day0, "refused", None, Some("lean"));
    let got = crate::report::build(&conn, day0 + DAY).unwrap();
    let lean = &got["drivers"]["lean"];
    assert_eq!((&lean["n"], &lean["ran"]), (&json!(1), &json!(0)));
    assert_eq!(lean.get("prShare"), None);
    assert_eq!(lean.get("parkedShare"), None);
}

#[test]
fn a_run_handed_off_to_a_person_counts_as_parked_for_its_driver() {
    let home = scratch("report-driver-handoff");
    let day0 = 1_792_022_400;
    let (conn, _) = db::open(&db::path(&home), day0).unwrap();
    driver_run(&conn, "a", 1, day0, "handoff", None, Some("lean"));
    driver_run(&conn, "b", 2, day0, "parked", None, Some("lean"));
    driver_run(&conn, "c", 3, day0, "pr", None, Some("lean"));
    driver_run(&conn, "d", 4, day0, "pr", None, Some("lean"));
    let got = crate::report::build(&conn, day0 + DAY).unwrap();
    assert_eq!(got["drivers"]["lean"]["parkedShare"], 0.5);
    assert_eq!(got["handoffRate"]["share"], 0.5);
}

#[test]
fn a_shipped_issue_whose_runs_reported_no_usage_is_no_zero_in_tokens_per_shipped() {
    let home = scratch("report-per-shipped");
    let day0 = 1_792_022_400;
    let (conn, _) = db::open(&db::path(&home), day0).unwrap();
    let tokens =
        |conn: &Connection| crate::report::build(conn, day0 + DAY).unwrap()["tokens"].clone();
    driver_run(&conn, "a", 1, day0, "merged", Some(1_000_000), None);
    driver_run(&conn, "b", 2, day0, "pr", None, Some("lean"));
    db::event(&conn, 2, "merged", day0 + 3600).unwrap();
    assert_eq!(tokens(&conn)["perShipped"], 1_000_000);
    assert_eq!(tokens(&conn)["shipped"], 1);
    // One run with a figure is enough for the issue to count.
    driver_run(&conn, "c", 2, day0 + 1, "pr", Some(0), Some("lean"));
    assert_eq!(tokens(&conn)["perShipped"], 500_000);
    assert_eq!(tokens(&conn)["shipped"], 2);
    // No run here at all: no driver left a gap, so the issue counts as it did.
    db::event(&conn, 3, "merged", day0 + 3600).unwrap();
    assert_eq!(tokens(&conn)["shipped"], 3);
}

#[test]
fn a_driver_of_no_runs_has_counts_and_no_figures() {
    let home = scratch("report-no-drivers");
    let to = 1_792_022_400;
    let (conn, _) = db::open(&db::path(&home), to).unwrap();
    let none = json!({"n": 0, "ran": 0, "minutes": {"n": 0}, "tokens": {"n": 0},
                      "pullRequests": {"n": 0, "merged": 0}});
    assert_eq!(
        crate::report::build(&conn, to).unwrap()["drivers"],
        json!({"classic": none, "lean": none})
    );
}

#[test]
fn empty_tokens_are_left_out_of_a_drivers_sum_and_not_counted() {
    let home = scratch("report-part-tokens");
    let day0 = 1_792_022_400;
    let (conn, _) = db::open(&db::path(&home), day0).unwrap();
    driver_run(&conn, "a", 1, day0, "pr", Some(0), Some("lean"));
    driver_run(&conn, "b", 2, day0, "pr", Some(400), Some("lean"));
    driver_run(&conn, "c", 3, day0, "pr", None, Some("lean"));
    let got = crate::report::build(&conn, day0 + DAY).unwrap();
    // A reported 0 is a figure; an empty run is not in the median either.
    assert_eq!(
        got["drivers"]["lean"]["tokens"],
        json!({"n": 2, "total": 400, "median": 200})
    );
    assert!(crate::report::text(&got).contains("400 (n=2)"));
}

#[test]
fn a_pull_request_counts_once_and_only_a_merge_after_it_was_reached() {
    let home = scratch("report-pulls");
    let day0 = 1_792_022_400;
    let to = day0 + DAY;
    let (conn, _) = db::open(&db::path(&home), day0 - 9 * DAY).unwrap();
    // Two runs of one pull request, the second a follow-up by another how.
    driver_run(&conn, "a", 1, day0, "pr", None, Some("lean"));
    driver_run(&conn, "b", 1, day0 + 1000, "pr", None, Some("lean"));
    db::event(&conn, 1, "merged", day0 + 2000).unwrap();
    // Merged before this cycle of the issue: an earlier pull request's.
    db::event(&conn, 2, "merged", day0 - 8 * DAY).unwrap();
    driver_run(&conn, "c", 2, day0, "pr", None, Some("lean"));
    // Merged after the report's end: not yet, in this report.
    driver_run(&conn, "d", 3, day0, "pr", None, Some("lean"));
    db::event(&conn, 3, "merged", to + 1).unwrap();
    // No pull request, so it is in no count of them, merged or not.
    driver_run(&conn, "e", 4, day0, "failed", None, Some("lean"));
    db::event(&conn, 4, "merged", day0 + 2000).unwrap();
    assert_eq!(
        crate::report::build(&conn, to).unwrap()["drivers"]["lean"]["pullRequests"],
        json!({"n": 3, "merged": 1, "share": 0.333})
    );
}

#[test]
fn a_merge_is_of_the_driver_that_last_reached_a_pull_request_before_it() {
    let home = scratch("report-pulls-driver");
    let day0 = 1_792_022_400;
    let to = day0 + DAY;
    let (conn, _) = db::open(&db::path(&home), day0 - 9 * DAY).unwrap();
    // Lean's pull request was closed, classic's merged.
    driver_run(&conn, "a", 1, day0, "pr", None, Some("lean"));
    driver_run(&conn, "b", 1, day0 + 1000, "pr", None, None);
    db::event(&conn, 1, "merged", day0 + 2000).unwrap();
    // The other way round, and a classic run after the merge changes nothing.
    driver_run(&conn, "c", 2, day0, "pr", None, None);
    driver_run(&conn, "d", 2, day0 + 1000, "pr", None, Some("lean"));
    db::event(&conn, 2, "merged", day0 + 2000).unwrap();
    driver_run(&conn, "e", 2, day0 + 3000, "pr", None, None);
    let report = crate::report::build(&conn, to).unwrap();
    assert_eq!(
        report["drivers"]["lean"]["pullRequests"],
        json!({"n": 2, "merged": 1, "share": 0.5})
    );
    assert_eq!(
        report["drivers"]["classic"]["pullRequests"],
        json!({"n": 2, "merged": 1, "share": 0.5})
    );
}

#[test]
fn a_refused_run_is_in_no_minutes_of_its_driver() {
    let home = scratch("report-minutes-ran");
    let day0 = 1_792_022_400;
    let (conn, _) = db::open(&db::path(&home), day0 - 9 * DAY).unwrap();
    driver_run(&conn, "a", 1, day0, "pr", None, Some("lean"));
    driver_run(&conn, "b", 2, day0, "refused", None, Some("lean"));
    driver_run(&conn, "c", 3, day0, "stopped", None, Some("lean"));
    let report = crate::report::build(&conn, day0 + DAY).unwrap();
    assert_eq!(report["drivers"]["lean"]["n"], json!(3));
    assert_eq!(
        report["drivers"]["lean"]["minutes"],
        json!({"n": 1, "total": 10, "median": 10})
    );
}

#[test]
fn a_migrated_run_reads_classic_in_the_report() {
    let home = scratch("report-migrated");
    let file = db::path(&home);
    let now = 1_792_022_400;
    drop(schema_2_database(&file, now));
    let (conn, _) = db::open(&file, now).unwrap();
    let got = crate::report::build(&conn, now).unwrap();
    assert_eq!(got["drivers"]["classic"]["n"], 1);
    assert_eq!(got["drivers"]["classic"]["prShare"], 1.0);
    assert_eq!(got["drivers"]["lean"]["n"], 0);
}

#[test]
fn a_run_with_no_usage_is_no_zero_in_a_version_pairs_median() {
    let home = scratch("report-version-tokens");
    let day0 = 1_792_022_400;
    let (conn, _) = db::open(&db::path(&home), day0).unwrap();
    let pair = |conn: &Connection| {
        let got = crate::report::build(conn, day0 + DAY).unwrap();
        let row = &got["versions"][0];
        (row["medianTokens"].clone(), row["tokensN"].clone())
    };
    driver_run(&conn, "a", 1, day0, "pr", None, Some("lean"));
    driver_run(&conn, "b", 2, day0, "pr", None, Some("lean"));
    driver_run(&conn, "c", 3, day0, "pr", None, Some("lean"));
    assert_eq!(pair(&conn), (Value::Null, Value::Null));
    // The floor holds for the runs behind the figure, not the pair's.
    driver_run(&conn, "d", 4, day0, "pr", Some(500), None);
    driver_run(&conn, "e", 5, day0, "pr", Some(600), None);
    assert_eq!(pair(&conn), (Value::Null, Value::Null));
    driver_run(&conn, "f", 6, day0, "pr", Some(700), None);
    assert_eq!(pair(&conn), (json!(600), json!(3)));
}

#[test]
fn v_versions_leaves_runs_with_no_usage_out_of_its_median_tokens() {
    let median =
        |conn: &Connection| one::<Option<f64>>(conn, "SELECT median_tokens FROM v_versions");
    // On a new database and on one migrated from schema 2.
    let new = scratch("view-tokens-new");
    let (fresh, _) = db::open(&db::path(&new), 1_792_022_400).unwrap();
    let old = scratch("view-tokens-migrated");
    drop(schema_2_database(&db::path(&old), 1_792_022_400));
    let (migrated, _) = db::open(&db::path(&old), 1_792_022_400).unwrap();
    migrated.execute("DELETE FROM runs", []).unwrap();
    for conn in [&fresh, &migrated] {
        driver_run(conn, "a", 1, 1_792_022_400, "pr", None, Some("lean"));
        driver_run(conn, "b", 2, 1_792_022_400, "pr", None, Some("lean"));
        assert_eq!(median(conn), None);
        driver_run(conn, "c", 3, 1_792_022_400, "pr", Some(500), None);
        assert_eq!(median(conn), Some(500.0));
        // A reported 0 is a figure.
        driver_run(conn, "d", 4, 1_792_022_400, "pr", Some(0), None);
        assert_eq!(median(conn), Some(250.0));
    }
}

#[test]
fn a_version_pair_reaches_its_figures_at_three_runs() {
    let home = scratch("report-floor");
    let day0 = 1_792_022_400;
    let (conn, _) = db::open(&db::path(&home), day0).unwrap();
    let pair =
        |conn: &Connection| crate::report::build(conn, day0 + DAY).unwrap()["versions"][0].clone();
    for (i, outcome) in ["pr", "failed", "pr"].iter().enumerate() {
        conn.execute(
            "INSERT INTO runs (id, kind, issue, started, ended, seconds, outcome, input,
                               tehas_version, rukas_version)
             VALUES (?1, 'work', 1, ?2, ?3, 60, ?4, 10, 'v0.7.22', '0.13.0')",
            rusqlite::params![
                format!("r{i}"),
                day0 + i as i64,
                day0 + i as i64 + 60,
                outcome
            ],
        )
        .unwrap();
        let want = if i < 2 {
            json!({"tehas": "v0.7.22", "rukas": "0.13.0", "n": i + 1})
        } else {
            json!({"tehas": "v0.7.22", "rukas": "0.13.0", "n": 3, "prShare": 0.667,
                   "medianSeconds": 60, "medianTokens": 10, "tokensN": 3})
        };
        assert_eq!(pair(&conn), want);
    }
}

#[test]
fn an_empty_database_reports_counts_of_nothing_and_leaves_out_what_has_no_value() {
    let home = scratch("report-empty");
    let to = 1_792_022_400 + 3600;
    let (conn, _) = db::open(&db::path(&home), to).unwrap();
    let got = crate::report::build(&conn, to).unwrap();
    let keys: Vec<&str> = got
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "availability",
            "cycleTime",
            "daily",
            "drivers",
            "firstPassYield",
            "from",
            "handoffRate",
            "leadTime",
            "machineTime",
            "planningTurns",
            "report",
            "runs",
            "shipped",
            "stations",
            "stopReasons",
            "to",
            "tokens",
            "versions"
        ]
    );
    // refine and sizes come from the ledgers beside the database, so the
    // report command adds them, not build().
    assert!(got.get("refine").is_none());
    assert!(got.get("sizes").is_none());
    // The command's own report carries both keys, as `text` needs them.
    let full = crate::report::complete(&conn, &home, to).unwrap();
    assert!(full["sizes"].is_object() && full["refine"].is_object());
    assert_eq!(full["sizes"]["none"]["n"], json!(0));
    assert_eq!(execute(&home, &Cmd::Report, to), Ok(()));
    assert_eq!(got["availability"], json!({"n": 0}));
    assert_eq!(got["firstPassYield"], json!({"n": 0}));
    assert_eq!(got["leadTime"], json!({"n": 0}));
    // No run recorded the gate, so its section is absent, as the other four
    // are absent with nothing to count, and the terminal prints no row for it.
    assert!(got.get("gate").is_none());
    assert_eq!(got["machineTime"], json!({"n": 0, "seconds": 0}));
    assert_eq!(got["daily"].as_array().unwrap().len(), 10);
    assert_eq!(got["from"], json!(to - 3600 - 9 * DAY));
    assert!(!got.to_string().contains("null"));
}

#[test]
fn ten_days_of_data_report_well_inside_the_time_limit() {
    let home = scratch("report-load");
    let to = 1_792_022_400 + 10 * DAY;
    let (mut conn, _) = db::open(&db::path(&home), to).unwrap();
    let tx = conn.transaction().unwrap();
    // A sample every two minutes, and a run every fifteen with three steps.
    for i in 0..7200 {
        tx.execute(
            "INSERT INTO samples (at, in_line, planning, paused, held, watts)
             VALUES (?1, 3, 1, 0, '', 250.0)",
            [to - 10 * DAY + i * 120],
        )
        .unwrap();
    }
    for i in 0..960_i64 {
        let started = to - 10 * DAY + i * 900;
        let id = format!("work-{i}-{started}");
        tx.execute(
            "INSERT INTO runs (id, kind, issue, started, ended, seconds, outcome, input, output,
                               tehas_version, rukas_version, failure_kind, last_step)
             VALUES (?1, 'work', ?2, ?3, ?4, 600, ?5, 1000, 100, 'v1', ?6, 'timeout', 'review')",
            rusqlite::params![
                id,
                i / 2,
                started,
                started + 600,
                if i % 3 == 0 { "failed" } else { "pr" },
                format!("0.{}.0", i % 7)
            ],
        )
        .unwrap();
        for (k, step) in ["explore", "implement", "review"].iter().enumerate() {
            tx.execute(
                "INSERT INTO run_steps (run_id, step, round, started) VALUES (?1, ?2, 1, ?3)",
                rusqlite::params![id, step, started + k as i64 * 100],
            )
            .unwrap();
        }
        if i % 2 == 1 {
            db::event(&tx, i / 2, "ready", started - 300).unwrap();
            db::event(&tx, i / 2, "merged", started + 700).unwrap();
        }
    }
    tx.commit().unwrap();
    let t = std::time::Instant::now();
    let got = execute(&home, &Cmd::Report, to);
    let took = t.elapsed();
    assert_eq!(got, Ok(()));
    let report = crate::report::build(&conn, to).unwrap();
    assert!(report["runs"]["n"].as_i64().unwrap() > 800);
    assert!(report["shipped"]["n"].as_i64().unwrap() > 400);
    // The limit is 5 s; an unoptimised build on a busy machine is well
    // inside 2.
    assert!(took.as_secs_f64() < 2.0, "the report took {took:?}");
}

#[test]
fn an_export_of_no_runs_leaves_out_from_and_to() {
    let home = scratch("export-empty");
    let now = 1_792_022_400;
    let out = home.join("copy.db");
    execute(&home, &Cmd::Export(out.clone(), true), now).unwrap();
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
    execute(&home, &Cmd::Export(out.clone(), true), now).unwrap();
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
fn the_text_report_says_the_figures_with_their_counts() {
    let home = scratch("report-text");
    let day0 = 1_792_022_400; // 2026-10-15T00:00:00Z
    let to = day0 + 9 * DAY + 43_200;
    let (conn, _) = db::open(&db::path(&home), day0).unwrap();
    report_data(&conn, day0, to);
    let text = crate::report::text(&crate::report::build(&conn, to).unwrap());
    // The Size line comes only with the key, which the report command adds.
    assert!(!text.contains("Size"));
    for want in [
        "Last 10 days, 2026-10-15 to 2026-10-24 (UTC)\n",
        "\nShipped            3 issues\n",
        "\nPlanned            6 now: 5 in line, 1 in planning\n",
        "\nRuns               6: 1 pr, 1 merged, 1 handoff, 0 parked, 2 failed, 1 refused, 0 stopped\n",
        "\nHandoff rate       16.7%  (n=6)\n",
        "\nFirst-pass yield   50%  (n=2)\n",
        "\nLead time          median 12h03m, p90 23h13m  (n=2)\n",
        "\nCycle time         median 12m30s, p90 30m  (n=6)\n",
        "\nMachine time       1h20m, 26m40s per shipped issue  (n=6)\n",
        "\nTokens             6650: 6100 input, 500 output, 50 cache read, 2217 per shipped issue  (n=4)\n",
        "\nAvailability       0.2% of 8d12h  (n=5)\n",
        "\nEnergy             0.075 kWh, 0.025 kWh per shipped issue  (n=4)\n",
        "\nGate               median 5m, p90 9m, 1.333 runs per run, 46.7% of run time  (n=3, 20m)\n",
        "\n2026-10-16         1     2       2       20\n",
        "\nStop reasons  (n=4)\n  explore-bodies-empty              1\n",
        "\n  implement                         2 of 3     66.7%\n",
        "\n  v1                  0.13.0                  4      50%     15m      2500\n",
        "\n  v2                  -                       1\n",
    ] {
        assert!(text.contains(want), "{want:?} in\n{text}");
    }
    assert!(!text.contains("cause no page"));
}

#[test]
fn the_text_report_of_an_empty_database_says_none() {
    let home = scratch("report-text-empty");
    let to = 1_792_022_400 + 3600;
    let (conn, _) = db::open(&db::path(&home), to).unwrap();
    let text = crate::report::text(&crate::report::build(&conn, to).unwrap());
    for want in [
        "\nShipped            0 issues\n",
        "\nPlanned            -\n",
        "\nFirst-pass yield   -  (n=0)\n",
        "\nLead time          median -, p90 -  (n=0)\n",
        "\nMachine time       0s  (n=0)\n",
        "\nAvailability       -  (n=0)\n",
        "\nStop reasons  (n=0)\n  none\n",
        "\nVersions  (fewer than 3 runs: the count only)\n  none\n",
    ] {
        assert!(text.contains(want), "{want:?} in\n{text}");
    }
    assert!(!text.contains("Energy"));
}

#[test]
fn an_export_to_a_path_that_is_not_utf8_leaves_no_file() {
    use std::os::unix::ffi::OsStrExt;
    let home = scratch("export-utf8");
    let out = home.join(std::ffi::OsStr::from_bytes(b"copy-\xff.db"));
    let err = execute(&home, &Cmd::Export(out.clone(), false), 1_792_022_400).unwrap_err();
    assert_eq!(err.0, 73, "{err:?}");
    assert!(!out.exists());
}

#[test]
fn a_factory_whose_cycles_run_is_available_whatever_holds_the_next_start() {
    let home = scratch("report-available");
    let to = 1_792_022_400 + 10 * DAY;
    let (conn, _) = db::open(&db::path(&home), to).unwrap();
    // Ten minutes each: full slots, a hold with nothing running, a pause,
    // nothing held, and a hold with running unknown.
    for (i, (running, paused, held)) in [
        (Some(4), 0, "count"),
        (Some(0), 0, "awaiting"),
        (Some(2), 1, ""),
        (Some(0), 0, ""),
        (None, 0, "count"),
    ]
    .into_iter()
    .enumerate()
    {
        conn.execute(
            "INSERT INTO samples (at, running, paused, held) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![to - 3000 + i as i64 * 600, running, paused, held],
        )
        .unwrap();
    }
    let report = crate::report::build(&conn, to).unwrap();
    assert_eq!(
        report["availability"],
        json!({"n": 5, "seconds": 3000, "share": 0.4})
    );
}

fn add_run(conn: &Connection, id: &str, issue: i64, started: i64) {
    conn.execute(
        "INSERT INTO runs (id, kind, issue, started, ended, outcome) VALUES (?1, 'work', ?2, ?3, ?3 + 600, 'pr')",
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
        "UPDATE runs SET failure_kind = 'cap', rukas_version = 'v0.14.0', review_held = 0, provider_outage = 1, cost = 1.5 WHERE id = 'work-7-x'",
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

fn logs_dir(name: &str, files: &[&str]) -> PathBuf {
    let home = scratch(name);
    for f in files {
        fs::write(home.join("logs").join(f), "abc").unwrap();
    }
    home.join("logs")
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
    let logs = logs_dir(
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
    let logs = logs_dir(
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
    let logs = logs_dir("logs-links", &["work-77-261020-100000.jsonl", "target"]);
    std::os::unix::fs::symlink("target", logs.join("work-7-261020-100000.jsonl")).unwrap();
    let started = secs("2026-10-20T10:00:30Z");
    let got = run_logs(&logs, "work", 7, started, None, started + 3600);
    assert!(got.is_empty());
}

#[test]
fn plan_log_words() {
    let logs = logs_dir(
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
    ]
    .join("\n");
    put(&home, "state/refined.jsonl", &ledger);
    let session = &sessions_output(&home, now)[1];
    assert_eq!(session["how"], "refine");
    assert_eq!(
        session["filed"],
        json!([
            {"issue": 11, "url": "https://example.com/i/11"},
            {"issue": 13, "url": "https://example.com/i/13"},
        ])
    );
    fs::remove_file(home.join("state/refined.jsonl")).unwrap();
    assert!(sessions_output(&home, now)[1].get("filed").is_none());
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
    let out = entries(
        &conn,
        now,
        &ofilter(Some("v1"), None, None, None, "version"),
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["runs"], 4);
    assert_eq!(out[0]["outcomes"]["merged"], 1);
    assert_eq!(out[0]["outcomes"]["failed"], 1);
    assert_eq!(out[0]["outcomes"]["parked"], 1);
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
        "INSERT INTO runs (id, kind, issue, started, ended, outcome) VALUES (?1, ?2, 1, ?3, ?3, 'pr')",
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
fn home_of_one_ended_run(name: &str, at: i64) -> (PathBuf, PathBuf) {
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
    // A poll: the snapshot is asked for and then the counts, as libexec/status
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
