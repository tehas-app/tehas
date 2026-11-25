//! `analytics sessions`: the runs of the last 30 days as JSON Lines, for the
//! MCP server. Line 1 is the header, then one session per line, newest
//! first. A session is a run with its steps, its roles and the names of the
//! log files it left in $TEHAS_HOME/logs. A key with no value is left out.
//!
//! The fields are the contract in `tehas_core::snapshot`, which the MCP
//! server's reader reads too: the names, their order and their shapes are
//! spelled once there, so a field cannot be renamed on this side and quietly
//! missing from every answer.

use crate::run::iso_ms;
use rusqlite::types::ValueRef;
use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};
use std::path::Path;
pub use tehas_core::snapshot::{log_files, log_names};
use tehas_core::snapshot::{
    Field, Shape, API, ROLE_FIELDS, RUN_FIELDS, SESSIONS_SHAPE, STEP_FIELDS,
};
use tehas_core::time::iso;
use Shape::{Bool, Time};

pub const SESSIONS_DAYS: i64 = 30;
/// The name of the private file, which the reader of the MCP server opens.
pub const FILE: &str = "sessions.jsonl";
const DAY: i64 = 86_400;
/// How long after a planning turn ended a ledger line may still be its.
const FILED_AFTER: i64 = 120;

// The column of a field in its table: the file's name with an underscore
// before each capital, which is how the schema names every column but one.
fn column(key: &str) -> String {
    let mut out = String::with_capacity(key.len() + 4);
    for c in key.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

// The one field that is not a column of the run's row: an image's build time,
// which the store keeps in meta under the image's id.
const META: [(&str, &str); 1] = [(
    "imageBuilt",
    "(SELECT CAST(value AS INTEGER) FROM meta WHERE key = 'image:' || runs.image)",
)];

fn select(table: &str, fields: &[Field], tail: &str) -> String {
    let names: Vec<String> = fields
        .iter()
        .map(|f| {
            META.iter()
                .find(|(key, _)| *key == f.0)
                .map_or_else(|| column(f.0), |(_, sql)| sql.to_string())
        })
        .collect();
    format!("SELECT {} FROM {table} {tail}", names.join(", "))
}

// One row as an object: camelCase keys, NULLs left out.
fn object(row: &rusqlite::Row, fields: &[Field]) -> rusqlite::Result<Map<String, Value>> {
    let mut o = Map::new();
    for (i, (key, kind, _)) in fields.iter().enumerate() {
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

/// The private file of the MCP server under `<dir>`, written from `lines`:
/// through a name of this run, mode 600, and moved into place so a reader
/// never sees half a file. The host asks for the snapshot this way, rather
/// than catching what the command prints and moving it itself.
pub fn write(dir: &Path, lines: &[String]) -> Result<(), String> {
    crate::publish::put(dir, FILE, lines)
}

/// The lines to print: the header, then the sessions of the last
/// SESSIONS_DAYS days.
pub fn lines(conn: &Connection, home: &Path, now: i64) -> rusqlite::Result<Vec<String>> {
    let mut out = vec![
        json!({"api": API, "sessions": SESSIONS_SHAPE, "days": SESSIONS_DAYS, "generatedAt": iso(now)})
            .to_string(),
    ];

    let mut runs = conn.prepare(&select(
        "runs",
        &RUN_FIELDS,
        "WHERE started >= ?1 ORDER BY started DESC, id DESC",
    ))?;
    let mut steps = conn.prepare(&select(
        "run_steps",
        &STEP_FIELDS,
        "WHERE run_id = ?1 ORDER BY started, step, round",
    ))?;
    let mut roles = conn.prepare(&select(
        "run_roles",
        &ROLE_FIELDS,
        "WHERE run_id = ?1 ORDER BY role",
    ))?;
    let found = runs
        .query_map(params![now - SESSIONS_DAYS * DAY], |r| {
            object(r, &RUN_FIELDS)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let ledger = ledger(home);
    let logs_dir = home.join("logs");
    let names = log_names(&logs_dir);
    for mut run in found {
        let id = run["id"].as_str().unwrap_or_default().to_string();
        let child = |stmt: &mut rusqlite::Statement, fields: &[Field]| {
            stmt.query_map(params![id], |r| object(r, fields).map(Value::Object))?
                .collect::<rusqlite::Result<Vec<_>>>()
        };
        run.insert(
            "steps".into(),
            Value::Array(child(&mut steps, &STEP_FIELDS)?),
        );
        run.insert(
            "roles".into(),
            Value::Array(child(&mut roles, &ROLE_FIELDS)?),
        );
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

/// A line of state/refined.jsonl as the poll writes it.
#[derive(serde::Deserialize)]
pub(crate) struct FiledLine {
    at: String,
    source: i64,
    filed: i64,
    url: String,
}

// state/refined.jsonl under the bounded plain-file rule: anything
// unreadable, a link, or a line of another shape is no filed issue.
fn ledger(home: &Path) -> Vec<Filed> {
    ledger_lines(home)
        .into_iter()
        .filter_map(|l: FiledLine| {
            Some(Filed {
                at: ledger_at(&l.at)?,
                source: l.source,
                filed: Some(l.filed).filter(|n| *n >= 1)?,
                url: l.url.starts_with("https://").then_some(l.url)?,
            })
        })
        .collect()
}

/// The bounds a ledger timestamp may fall inside, in milliseconds.
const LEDGER_AT_MIN: i64 = 1_735_689_600_000; // 2025-01-01T00:00:00Z
const LEDGER_AT_MAX: i64 = 4_102_444_800_000; // 2100-01-01T00:00:00Z

/// A ledger line's `at` in Unix seconds, or nothing: an ISO time as the
/// writer prints it, inside the bounds above.
fn ledger_at(at: &str) -> Option<i64> {
    let ms = iso_ms(at)?;
    (LEDGER_AT_MIN..=LEDGER_AT_MAX)
        .contains(&ms)
        .then_some(ms / 1000)
}

/// The lines of refined.jsonl, each read as a `FiledLine`; a line of another
/// shape, or of `LEDGER_MAX_LINE` bytes or more, is skipped. What the file is
/// not (no file, a link, a device, too long, not text) is no lines at all.
pub(crate) fn ledger_lines(home: &Path) -> Vec<FiledLine> {
    let Some(text) = crate::run::read_ledger(
        home,
        tehas_core::home::State::Refined,
        crate::LEDGER_MAX_BYTES,
    ) else {
        return Vec::new();
    };
    text.lines()
        .filter(|l| l.len() < crate::LEDGER_MAX_LINE)
        // An object only: serde would read a struct from an array too.
        .filter(|l| l.trim_start().starts_with('{'))
        .filter_map(|l| serde_json::from_str::<FiledLine>(l).ok())
        .collect()
}

fn secs(run: &Map<String, Value>, key: &str) -> Option<i64> {
    iso_ms(run.get(key)?.as_str()?).map(|ms| ms / 1000)
}
