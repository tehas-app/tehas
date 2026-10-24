//! `analytics sessions`: the runs of the last 30 days as JSON Lines, for the
//! MCP server. Line 1 is the header, then one session per line, newest
//! first. A session is a run with its steps, its roles and the names of the
//! log files it left in $TEHAS_HOME/logs. A key with no value is left out.

use crate::run::iso_ms;
use rusqlite::types::ValueRef;
use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::Path;
use tehas_core::time::iso;

pub const SESSIONS_DAYS: i64 = 30;
const DAY: i64 = 86_400;
/// Bytes of a jsonl ledger the sessions read, and the width one line may
/// have: the same bounds the report's ledger reader works within, from the
/// crate's root.
const LEDGER_MAX_BYTES: u64 = crate::LEDGER_MAX_BYTES;
const LEDGER_MAX_LINE: usize = crate::LEDGER_MAX_LINE;
/// How long after a planning turn ended a ledger line may still be its.
const FILED_AFTER: i64 = 120;
/// How long before a run's start its log's stamp may be: the stamp is taken
/// as the cycle begins, the run's start a little before or after.
const STAMP_BEFORE: i64 = 120;

#[derive(Clone, Copy)]
enum Kind {
    Text,
    Int,
    Real,
    Time,
    Bool,
}
use Kind::{Bool, Int, Real, Text, Time};

const RUN: [(&str, &str, Kind); 38] = [
    ("id", "id", Text),
    ("kind", "kind", Text),
    ("issue", "issue", Int),
    ("attempt", "attempt", Int),
    ("how", "how", Text),
    ("started", "started", Time),
    ("ended", "ended", Time),
    ("seconds", "seconds", Int),
    ("queue_wait", "queueWait", Int),
    ("outcome", "outcome", Text),
    ("exit_code", "exitCode", Int),
    ("status", "status", Text),
    ("last_step", "lastStep", Text),
    ("failed_step", "failedStep", Text),
    ("cap", "cap", Text),
    ("park_reason", "parkReason", Text),
    ("review_held", "reviewHeld", Bool),
    ("failure_kind", "failureKind", Text),
    ("failure_cause", "failureCause", Text),
    ("provider_outage", "providerOutage", Bool),
    ("endpoint", "endpoint", Text),
    ("pr", "pr", Int),
    ("tehas_version", "tehasVersion", Text),
    ("tehas_commit", "tehasCommit", Text),
    ("rukas_version", "rukasVersion", Text),
    ("rukas_commit", "rukasCommit", Text),
    ("image", "image", Text),
    ("result_schema", "resultSchema", Int),
    ("input", "input", Int),
    ("output", "output", Int),
    ("cache_read", "cacheRead", Int),
    ("cache_write", "cacheWrite", Int),
    ("turns", "turns", Int),
    ("dispatches", "dispatches", Int),
    ("cost", "cost", Real),
    ("runner_ms", "runnerMs", Int),
    ("oom_kills", "oomKills", Int),
    ("forks_refused", "forksRefused", Int),
];
const RUN_LAST: [(&str, &str, Kind); 1] = [("concurrent", "concurrent", Int)];

const STEP: [(&str, &str, Kind); 5] = [
    ("step", "step", Text),
    ("round", "round", Int),
    ("started", "started", Time),
    ("ended", "ended", Time),
    ("seconds", "seconds", Int),
];

const ROLE: [(&str, &str, Kind); 10] = [
    ("role", "role", Text),
    ("model", "model", Text),
    ("provider", "provider", Text),
    ("dispatches", "dispatches", Int),
    ("turns", "turns", Int),
    ("input", "input", Int),
    ("output", "output", Int),
    ("cache_read", "cacheRead", Int),
    ("cache_write", "cacheWrite", Int),
    ("cost", "cost", Real),
];
const ROLE_LAST: [(&str, &str, Kind); 1] = [("ms", "ms", Int)];

fn columns(
    parts: &[&[(&'static str, &'static str, Kind)]],
) -> Vec<(&'static str, &'static str, Kind)> {
    parts.iter().flat_map(|p| p.iter().copied()).collect()
}

fn select(table: &str, cols: &[(&str, &str, Kind)], tail: &str) -> String {
    let names: Vec<&str> = cols.iter().map(|c| c.0).collect();
    format!("SELECT {} FROM {table} {tail}", names.join(", "))
}

// One row as an object: camelCase keys, NULLs left out.
fn object(
    row: &rusqlite::Row,
    cols: &[(&str, &str, Kind)],
) -> rusqlite::Result<Map<String, Value>> {
    let mut o = Map::new();
    for (i, (_, key, kind)) in cols.iter().enumerate() {
        let v = match (row.get_ref(i)?, kind) {
            (ValueRef::Null, _) => continue,
            (ValueRef::Integer(n), Time) => json!(iso(n)),
            (ValueRef::Integer(n), Bool) => json!(n != 0),
            (ValueRef::Integer(n), _) => json!(n),
            (ValueRef::Real(f), _) => json!(f),
            (ValueRef::Text(t), _) => json!(String::from_utf8_lossy(t)),
            (ValueRef::Blob(_), _) => continue,
        };
        o.insert((*key).to_string(), v);
    }
    Ok(o)
}

/// The lines to print: the header, then the sessions of the last
/// SESSIONS_DAYS days.
pub fn lines(conn: &Connection, home: &Path, now: i64) -> rusqlite::Result<Vec<String>> {
    let run_cols = columns(&[&RUN, &RUN_LAST]);
    let step_cols = STEP.to_vec();
    let role_cols = columns(&[&ROLE, &ROLE_LAST]);
    let mut out = vec![
        json!({"api": 1, "sessions": 1, "days": SESSIONS_DAYS, "generatedAt": iso(now)})
            .to_string(),
    ];

    let mut runs = conn.prepare(&select(
        "runs",
        &run_cols,
        "WHERE started >= ?1 ORDER BY started DESC, id DESC",
    ))?;
    let mut steps = conn.prepare(&select(
        "run_steps",
        &step_cols,
        "WHERE run_id = ?1 ORDER BY started, step, round",
    ))?;
    let mut roles = conn.prepare(&select(
        "run_roles",
        &role_cols,
        "WHERE run_id = ?1 ORDER BY role",
    ))?;
    let found = runs
        .query_map(params![now - SESSIONS_DAYS * DAY], |r| object(r, &run_cols))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let ledger = ledger(home);
    let logs_dir = home.join("logs");
    let names = log_names(&logs_dir);
    for mut run in found {
        let id = run["id"].as_str().unwrap_or_default().to_string();
        let child = |stmt: &mut rusqlite::Statement, cols: &[(&str, &str, Kind)]| {
            stmt.query_map(params![id], |r| object(r, cols).map(Value::Object))?
                .collect::<rusqlite::Result<Vec<_>>>()
        };
        run.insert("steps".into(), Value::Array(child(&mut steps, &step_cols)?));
        run.insert("roles".into(), Value::Array(child(&mut roles, &role_cols)?));
        let kind = run["kind"].as_str().unwrap_or_default().to_string();
        let issue = run["issue"].as_i64().unwrap_or_default();
        let started = secs(&run, "started").unwrap_or_default();
        let logs = log_files(
            &logs_dir,
            &names,
            &kind,
            issue,
            started,
            secs(&run, "ended"),
            now,
        );
        let logs: Vec<Value> = logs
            .into_iter()
            .map(|(word, name, bytes)| json!({"word": word, "name": name, "bytes": bytes}))
            .collect();
        run.insert("logs".into(), Value::Array(logs));
        if kind == "plan" {
            let upto = secs(&run, "ended").unwrap_or(now) + FILED_AFTER;
            let filed: Vec<Value> = ledger
                .iter()
                .filter(|l| l.source == issue && l.at >= started && l.at <= upto)
                .map(|l| json!({"issue": l.filed, "url": l.url}))
                .collect();
            if !filed.is_empty() {
                run.insert("filed".into(), Value::Array(filed));
            }
        }
        out.push(Value::Object(run).to_string());
    }
    Ok(out)
}

struct Filed {
    at: i64,
    source: i64,
    filed: i64,
    url: String,
}

// state/refined.jsonl, as lib/planning.sh writes it, read by the same bounded
// plain-file rule the report's own ledger reader uses: anything unreadable, a
// link, or a line of another shape is no filed issue.
fn ledger(home: &Path) -> Vec<Filed> {
    let text = crate::run::read_ledger(home, crate::report::REFINED_LEDGER, LEDGER_MAX_BYTES)
        .unwrap_or_default();
    text.lines()
        .filter(|l| l.len() < LEDGER_MAX_LINE)
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter_map(|v| {
            let url = v.get("url")?.as_str()?;
            Some(Filed {
                at: crate::report::ledger_at(&v)?,
                source: v.get("source")?.as_i64()?,
                filed: v.get("filed")?.as_i64().filter(|n| *n >= 1)?,
                url: url.starts_with("https://").then(|| url.to_string())?,
            })
        })
        .collect()
}

fn secs(run: &Map<String, Value>, key: &str) -> Option<i64> {
    iso_ms(run.get(key)?.as_str()?).map(|ms| ms / 1000)
}

// 261020-100000 as Unix seconds, UTC.
fn stamp_secs(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let ok = b.len() == 13
        && b[6] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 6 || c.is_ascii_digit());
    ok.then_some(())?;
    let t = format!(
        "20{}-{}-{}T{}:{}:{}Z",
        &s[0..2],
        &s[2..4],
        &s[4..6],
        &s[7..9],
        &s[9..11],
        &s[11..13]
    );
    iso_ms(&t).map(|ms| ms / 1000)
}

fn regular_size(p: &Path) -> Option<u64> {
    fs::symlink_metadata(p)
        .ok()
        .filter(|m| m.is_file())
        .map(|m| m.len())
}

/// The names in the logs directory, read once for all runs.
pub fn log_names(logs: &Path) -> Vec<String> {
    fs::read_dir(logs)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .collect()
}

/// The log files of a run that exist as regular files: (word, file name,
/// bytes). A log belongs to the run when the stamp in its name, taken when
/// the cycle began, is no more than STAMP_BEFORE seconds before the run's
/// start and not after its end (now for a run that has not ended). When
/// several stamps fall in that window only the latest is the run's: an
/// earlier one is a short run of the same issue that ended just before.
pub fn log_files(
    logs: &Path,
    names: &[String],
    kind: &str,
    issue: i64,
    started: i64,
    ended: Option<i64>,
    now: i64,
) -> Vec<(String, String, u64)> {
    let (own, tails): (&str, &[(&str, &str)]) = match kind {
        "work" => (
            "work",
            &[
                ("transcript", ".jsonl"),
                ("err", ".jsonl.err"),
                ("preflight", ".jsonl.preflight"),
                ("console", ".jsonl.console"),
                ("result", ".jsonl.result.json"),
            ],
        ),
        "plan" => ("plan", &[("progress", ".jsonl"), ("result", ".json")]),
        _ => return Vec::new(),
    };
    let prefix = format!("{own}-{issue}-");
    let upto = ended.unwrap_or(now);
    let mut stamps: Vec<&str> = names
        .iter()
        .filter_map(|n| n.strip_prefix(&prefix)?.get(..13))
        .filter(|s| stamp_secs(s).is_some_and(|t| t >= started - STAMP_BEFORE && t <= upto))
        .collect();
    stamps.sort_unstable();
    stamps.dedup();
    let mut found = Vec::new();
    for stamp in stamps.iter().rev() {
        for (word, tail) in tails {
            let name = format!("{prefix}{stamp}{tail}");
            if let Some(bytes) = regular_size(&logs.join(&name)) {
                found.push(((*word).to_string(), name, bytes));
            }
        }
        if !found.is_empty() {
            break;
        }
    }
    let dispatch = format!("dispatch-{own}-{issue}.log");
    if let Some(bytes) = regular_size(&logs.join(&dispatch)) {
        found.push(("dispatch".to_string(), dispatch, bytes));
    }
    found
}
