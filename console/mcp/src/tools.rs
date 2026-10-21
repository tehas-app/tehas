//! The tools: what each copies from the live files.
//!
//! Every tool reads files under live/ by fixed names and copies only the keys
//! its spec below names. A key the dashboard's files gain later is not served
//! until it is named here. Null values are left out.

use crate::sessions;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

/// The most of a console a call returns, and the default.
pub const CONSOLE_MAX: u64 = 65536;
/// A poll older than this is stale, as on the dashboard.
pub(crate) const STALE_SECS: i64 = 600;

/// The directories the server reads, each mounted read only.
pub struct Dirs {
    /// What the dashboard publishes.
    pub live: PathBuf,
    /// The sessions snapshot, never published.
    pub sessions: PathBuf,
    pub logs: PathBuf,
}

/// What a call returns: JSON, or JSON that describes a file sent with it.
#[derive(Debug)]
pub enum Output {
    Json(Value),
    Blob {
        meta: Value,
        uri: String,
        mime: &'static str,
        /// The file's bytes in base64.
        blob: String,
    },
}

#[derive(Debug)]
pub enum ToolError {
    Unknown(String),
    BadArgs(String),
    File { file: String, why: String },
}

const STAGES: [&str; 6] = [
    "queued",
    "planning",
    "running",
    "awaiting",
    "stopped",
    "completed",
];

/// The tools, in a fixed order.
pub fn list() -> Value {
    let none = json!({"type": "object", "properties": {}, "additionalProperties": false});
    let out =
        json!({"type": "object", "properties": {"api": {"type": "integer"}}, "required": ["api"]});
    let mut tools = json!([
        {
            "name": "factory_status",
            "description": "Whether the factory is working, paused or failing, its queue counts, the running cycles and their steps, the demand waiting, and the platform commit. Stale is true when the dispatcher has not reported for ten minutes.",
            "inputSchema": none,
            "outputSchema": out,
        },
        {
            "name": "factory_issues",
            "description": "The issues the dashboard lists, by stage: queued, planning, running, awaiting review, stopped and completed, each with number, title, link and times. Planning gives a count and issue numbers only.",
            "inputSchema": {
                "type": "object",
                "properties": {"stage": {"type": "string", "enum": STAGES}},
                "additionalProperties": false,
            },
            "outputSchema": out,
        },
        {
            "name": "factory_endings",
            "description": "The cycles that ended last, with their status and the step they ended in, and which of them failed.",
            "inputSchema": none,
            "outputSchema": out,
        },
        {
            "name": "factory_console",
            "description": "One running cycle's step, status and the end of its console as the dashboard shows it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "issue": {"type": "integer", "minimum": 1},
                    "bytes": {"type": "integer", "minimum": 1, "maximum": CONSOLE_MAX},
                },
                "required": ["issue"],
                "additionalProperties": false,
            },
            "outputSchema": out,
        },
        {
            "name": "factory_stats",
            "description": "The last ten days as the dashboard reports them: runs by outcome, cycle and lead time, first pass yield, handoff rate, availability, machine time, tokens, energy, stop reasons, versions, the refine figures and the daily rows.",
            "inputSchema": none,
            "outputSchema": out,
        },
    ]);
    if let Value::Array(a) = &mut tools {
        a.extend(sessions::tools());
    }
    tools
}

/// The structured content of one call, with "api": 1.
pub fn call(dirs: &Dirs, name: &str, args: &Value) -> Result<Output, ToolError> {
    let live = dirs.live.as_path();
    let mut out = match name {
        "factory_status" => status(live)?,
        "factory_issues" => issues(live, args)?,
        "factory_endings" => endings(live)?,
        "factory_console" => console(live, args)?,
        "factory_stats" => stats(live)?,
        "factory_sessions" => sessions::list(dirs, args)?,
        "factory_session" => sessions::one(dirs, args)?,
        "factory_failures" => sessions::failures(dirs)?,
        _ => return Err(ToolError::Unknown(name.to_string())),
    };
    out.insert("api".into(), json!(1));
    Ok(Output::Json(Value::Object(out)))
}

fn read(live: &Path, file: &str) -> Result<Value, ToolError> {
    let err = |why: &str| ToolError::File {
        file: file.to_string(),
        why: why.to_string(),
    };
    let bytes = std::fs::read(live.join(file)).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            err("not published")
        } else {
            err("could not be read")
        }
    })?;
    serde_json::from_slice(&bytes).map_err(|_| err("is not complete JSON"))
}

/// The parts of `src` that `spec` names: `true` a scalar, an object its named
/// keys, `[spec]` each element of an array. None for null or a missing value.
pub(crate) fn pick(src: &Value, spec: &Value) -> Option<Value> {
    match (spec, src) {
        (_, Value::Null) => None,
        (Value::Bool(true), Value::Object(_) | Value::Array(_)) => None,
        (Value::Bool(true), v) => Some(v.clone()),
        (Value::Object(s), Value::Object(m)) => {
            let mut out = Map::new();
            for (k, sub) in s {
                if let Some(v) = m.get(k).and_then(|v| pick(v, sub)) {
                    out.insert(k.clone(), v);
                }
            }
            Some(Value::Object(out))
        }
        (Value::Array(s), Value::Array(a)) => {
            let sub = s.first()?;
            Some(Value::Array(
                a.iter().filter_map(|v| pick(v, sub)).collect(),
            ))
        }
        _ => None,
    }
}

fn put(out: &mut Map<String, Value>, key: &str, v: Option<Value>) {
    if let Some(v) = v {
        out.insert(key.to_string(), v);
    }
}

fn merge(out: &mut Map<String, Value>, v: Option<Value>) {
    if let Some(Value::Object(m)) = v {
        out.extend(m);
    }
}

fn status(live: &Path) -> Result<Map<String, Value>, ToolError> {
    let health = read(live, "health.json")?;
    let st = read(live, "status.json")?;
    let checks = &health["checks"];
    let mut out = Map::new();
    merge(
        &mut out,
        pick(
            &health,
            &json!({"status": true, "output": true, "time": true}),
        ),
    );
    merge(
        &mut out,
        pick(
            &st,
            &json!({"generatedAt": true, "repo": true, "paused": true, "planning": true}),
        ),
    );
    if let Some(t) = health["time"].as_str().and_then(epoch) {
        out.insert("stale".into(), json!(now() - t > STALE_SECS));
    }
    put(
        &mut out,
        "limit",
        pick(&checks["limit"], &json!({"state": true})),
    );
    put(
        &mut out,
        "queue",
        pick(
            &checks["queue"],
            &json!({"inLine": true, "building": true, "review": true, "stopped": true, "time": true}),
        ),
    );
    put(
        &mut out,
        "cycles",
        pick(
            &checks["cycles"],
            &json!([{"issue": true, "running": true, "status": true, "step": true, "startedAt": true, "time": true}]),
        ),
    );
    put(
        &mut out,
        "demand",
        pick(
            &checks["demand"],
            &json!({"waiting": true, "oldest": true, "time": true}),
        ),
    );
    put(
        &mut out,
        "platform",
        pick(
            &checks["platform"],
            &json!({"commit": true, "behind": true, "upstream": true, "fetchedAt": true}),
        ),
    );
    Ok(out)
}

fn issues(live: &Path, args: &Value) -> Result<Map<String, Value>, ToolError> {
    let want = match &args["stage"] {
        Value::Null => None,
        Value::String(s) if STAGES.contains(&s.as_str()) => Some(s.as_str()),
        _ => {
            return Err(ToolError::BadArgs(format!(
                "stage is one of {}",
                STAGES.join(", ")
            )))
        }
    };
    let st = read(live, "status.json")?;
    let item =
        json!([{"number": true, "title": true, "url": true, "updatedAt": true, "closedAt": true}]);
    let mut out = Map::new();
    put(
        &mut out,
        "generatedAt",
        pick(&st["generatedAt"], &json!(true)),
    );
    for stage in STAGES {
        if want.is_some_and(|w| w != stage) {
            continue;
        }
        if stage == "planning" {
            let health = read(live, "health.json")?;
            let numbers: Vec<Value> = health["checks"]["plans"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|p| pick(&p["issue"], &json!(true)))
                .collect();
            let mut p = Map::new();
            put(&mut p, "count", pick(&st["planning"], &json!(true)));
            p.insert("issues".into(), Value::Array(numbers));
            out.insert("planning".into(), Value::Object(p));
        } else {
            put(&mut out, stage, pick(&st[stage], &item));
        }
    }
    Ok(out)
}

fn endings(live: &Path) -> Result<Map<String, Value>, ToolError> {
    let ends = read(live, "endings.json")?;
    let health = read(live, "health.json")?;
    let mut out = Map::new();
    put(
        &mut out,
        "endings",
        pick(
            &ends,
            &json!([{"issue": true, "status": true, "step": true, "startedAt": true, "updatedAt": true}]),
        ),
    );
    put(
        &mut out,
        "failed",
        pick(
            &health["checks"]["failed"],
            &json!([{"issue": true, "status": true, "step": true, "startedAt": true, "time": true}]),
        ),
    );
    Ok(out)
}

fn console(live: &Path, args: &Value) -> Result<Map<String, Value>, ToolError> {
    let issue = args["issue"]
        .as_u64()
        .filter(|n| *n > 0)
        .ok_or_else(|| ToolError::BadArgs("issue is a positive integer".into()))?;
    let bytes = match &args["bytes"] {
        Value::Null => CONSOLE_MAX,
        v => v
            .as_u64()
            .filter(|n| (1..=CONSOLE_MAX).contains(n))
            .ok_or_else(|| ToolError::BadArgs(format!("bytes is from 1 to {CONSOLE_MAX}")))?,
    };
    let running = read(live, "running.json")?;
    let cycle = running["cycles"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|c| c["issue"].as_u64() == Some(issue))
        .ok_or_else(|| ToolError::BadArgs(format!("no running cycle for issue {issue}")))?;
    let mut out = Map::new();
    merge(
        &mut out,
        pick(
            cycle,
            &json!({"issue": true, "step": true, "status": true, "statusText": true, "startedAt": true, "updatedAt": true}),
        ),
    );
    // The only file name this opens, and only when running.json gives it.
    let name = format!("console-{issue}.txt");
    if cycle["console"].as_str() == Some(name.as_str()) {
        if let Some(text) = read_tail(&live.join(&name), bytes) {
            out.insert("console".into(), json!(text));
        }
    }
    Ok(out)
}

/// The last `n` bytes of a file, cut forward to the start of a character.
/// Only those bytes are read: a console only grows.
fn read_tail(path: &Path, n: u64) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(n))).ok()?;
    let mut text = Vec::with_capacity(n.min(len) as usize);
    f.take(n).read_to_end(&mut text).ok()?;
    let start = text
        .iter()
        .position(|b| b & 0b1100_0000 != 0b1000_0000)
        .unwrap_or(text.len());
    Some(String::from_utf8_lossy(&text[start..]).into_owned())
}

fn stats(live: &Path) -> Result<Map<String, Value>, ToolError> {
    let a = read(live, "analytics.json")?;
    let spec = json!({
        "from": true, "to": true, "report": true,
        "runs": {"n": true, "merged": true, "pr": true, "handoff": true, "parked": true,
                 "failed": true, "refused": true, "stopped": true},
        "shipped": {"n": true},
        "cycleTime": {"n": true, "median": true, "p90": true},
        "leadTime": {"n": true, "median": true, "p90": true},
        "firstPassYield": {"n": true, "share": true},
        "handoffRate": {"n": true, "share": true},
        "availability": {"n": true, "seconds": true, "share": true},
        "machineTime": {"n": true, "seconds": true, "perShipped": true},
        "energy": {"n": true, "kwh": true, "perShipped": true},
        "tokens": {"n": true, "input": true, "output": true, "cacheRead": true, "total": true,
                   "perShipped": true},
        "planned": {"n": true, "inLine": true, "planning": true,
                    "trend": [{"day": true, "n": true}]},
        "planningTurns": {"n": true},
        "refine": {"sessions": true, "n": true, "started": true, "shipped": true,
                   "shippedShare": true,
                   "leadTime": {"n": true, "median": true, "p90": true},
                   "others": {"n": true, "started": true, "shipped": true,
                              "shippedShare": true}},
        "stations": {"n": true, "steps": [{"step": true, "n": true, "passed": true, "share": true}]},
        "stopReasons": {"n": true, "top": [{"reason": true, "n": true}]},
        "versions": [{"tehas": true, "rukas": true, "n": true, "medianSeconds": true,
                      "medianTokens": true, "prShare": true}],
        "daily": [{"day": true, "runs": true, "shipped": true, "failed": true, "minutes": true}],
    });
    match pick(&a, &spec) {
        Some(Value::Object(m)) => Ok(m),
        _ => Err(ToolError::File {
            file: "analytics.json".into(),
            why: "is not an object".into(),
        }),
    }
}

pub(crate) fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Seconds since 1970 for "YYYY-MM-DDTHH:MM:SSZ", the form the live files use.
pub(crate) fn epoch(t: &str) -> Option<i64> {
    let b = t.as_bytes();
    if b.len() != 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[19] != b'Z' {
        return None;
    }
    let n = |r: std::ops::Range<usize>| t.get(r)?.parse::<i64>().ok();
    let (y, m, d) = (n(0..4)?, n(5..7)?, n(8..10)?);
    let (hh, mm, ss) = (n(11..13)?, n(14..16)?, n(17..19)?);
    // Days from the civil date (Howard Hinnant's algorithm).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + hh * 3600 + mm * 60 + ss)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_of_a_live_file_time() {
        assert_eq!(epoch("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(epoch("2026-10-20T08:46:07Z"), Some(1792485967));
        assert_eq!(epoch("2026-10-20 08:46:07"), None);
    }
}
