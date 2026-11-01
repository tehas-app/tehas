//! The snapshot behind `tehas top`: one JSON object from what the factory
//! already writes under live/, the user's tehas-* units, and two readings of
//! the host, see src/top.rs.
//!
//! The readings are those of `console top reading`, either of them empty. The
//! host section is computed when both are there. No GitHub
//! call, no Docker call, no settings, and nothing is written. A section whose
//! source is missing, unreadable or older than <max age> seconds is null; the
//! others are there all the same. Every string is cleaned of control
//! characters, here and when the frame reads it.

use crate::top;
use serde_json::{json, Map, Value};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use tehas_core::time;

// Where the two raw parts go, for the text to fill in.
const HOST: &str = "\"\\u0001host\"";
const READING: &str = "\"\\u0001reading\"";

pub fn snapshot(live: &Path, lines: usize, max_age: i64, reading: &str, earlier: &str) -> String {
    let now = time::now();
    let health = health(&live.join("health.json"));
    let factory = health.as_ref().map_or(Value::Null, |h| {
        let at = h.get("time").and_then(Value::as_str).and_then(epoch);
        let mut f = h.clone();
        f["ageSeconds"] = json!(now - at.unwrap_or(now));
        f["stale"] = json!(now - at.unwrap_or(0) > max_age);
        f
    });
    let counts = health
        .as_ref()
        .map_or(Value::Null, |h| h["checks"]["queue"].clone());
    let cycle = cycle(&live.join("running.json"));
    let console = console(live, &cycle, lines);
    let host = top::host_text(reading, earlier).unwrap_or_else(|| "null".to_string());
    let mut snapshot = safe(json!({
        "generatedAt": time::iso(now),
        "factory": factory,
        "queue": queue(&live.join("status.json"), counts),
        "cycle": cycle,
        "units": units().unwrap_or(Value::Null),
        "energy": energy(&live.join("energy.json"), now, max_age),
        "console": console,
    }));
    // The host section and the reading keep the text the host gave (1000.00
    // stays 1000.00); their names were cleaned when the host was read.
    snapshot["host"] = json!("\u{1}host");
    snapshot["reading"] = json!("\u{1}reading");
    snapshot.to_string().replacen(HOST, &host, 1).replacen(
        READING,
        if serde_json::from_str::<Value>(reading).is_ok_and(|r| r.is_object()) {
            reading.trim()
        } else {
            "null"
        },
        1,
    )
}

/// Seconds since the epoch of YYYY-MM-DDTHH:MM:SSZ, as fromdateiso8601 reads it.
pub fn epoch(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() != 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || b[19] != b'Z'
    {
        return None;
    }
    let n = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, m, d, hh, mm, ss) = (
        n(0..4)?,
        n(5..7)?,
        n(8..10)?,
        n(11..13)?,
        n(14..16)?,
        n(17..19)?,
    );
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || hh > 23 || mm > 59 || ss > 60 {
        return None;
    }
    Some(time::days_from_civil(y, m, d) * 86_400 + hh * 3600 + mm * 60 + ss)
}

/// The bytes of a file with the control characters a terminal could act on
/// taken out: tab and newline stay, and a C1 control goes only where it is
/// its own UTF-8 character (0xc2 0x80..0x9f).
fn sanitized(bytes: &[u8]) -> String {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == 0xc2 && bytes.get(i + 1).is_some_and(|n| (0x80..=0x9f).contains(n)) {
            i += 2;
            continue;
        }
        if !(b <= 0x08 || (0x0b..=0x1f).contains(&b) || b == 0x7f) {
            out.push(b);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A JSON file read through the cleaning above.
fn read_clean(path: &Path) -> Option<Value> {
    serde_json::from_str(&sanitized(&fs::read(path).ok()?)).ok()
}

/// Every string with the characters below 0x20 and from 0x7f to 0x9f taken
/// out, tab kept.
fn safe(v: Value) -> Value {
    match v {
        Value::String(s) => Value::String(
            s.chars()
                .filter(|c| matches!(*c as u32, 32..=126 | 9) || *c as u32 >= 160)
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.into_iter().map(safe).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, safe(v))).collect()),
        other => other,
    }
}

fn field(v: &Value, key: &str) -> Value {
    v.get(key).cloned().unwrap_or(Value::Null)
}

/// The health file: what the factory line and the queue counts are read from.
fn health(path: &Path) -> Option<Value> {
    let h = read_clean(path)?;
    h.get("status")?.as_str()?;
    let queue = h.get("checks").map_or(Value::Null, |c| field(c, "queue"));
    Some(json!({
        "status": field(&h, "status"),
        "time": field(&h, "time"),
        "output": field(&h, "output"),
        "checks": {"queue": queue},
    }))
}

/// The issues behind the counts: running first, then in line, in review and
/// stopped.
fn queue(path: &Path, counts: Value) -> Value {
    let Some(s) = read_clean(path).filter(Value::is_object) else {
        return Value::Null;
    };
    let mut issues = Vec::new();
    for (key, state) in [
        ("running", "running"),
        ("queued", "in line"),
        ("awaiting", "in review"),
        ("stopped", "stopped"),
    ] {
        for i in s.get(key).and_then(Value::as_array).into_iter().flatten() {
            issues.push(
                json!({"number": field(i, "number"), "state": state, "title": field(i, "title")}),
            );
        }
    }
    json!({"time": field(&s, "generatedAt"), "counts": counts, "issues": issues})
}

fn iso_of(v: &Value) -> Value {
    v.as_f64()
        .map_or(Value::Null, |t| json!(time::iso(t.floor() as i64)))
}

fn brief(c: &Value) -> Value {
    json!({
        "issue": field(c, "issue"),
        "running": c.get("running").filter(|r| !r.is_null() && **r != json!(false)).cloned().unwrap_or(json!(false)),
        "status": field(c, "status"),
        "step": field(c, "step"),
        "console": field(c, "console"),
        "startedAt": iso_of(&field(c, "startedAt")),
        "time": iso_of(&field(c, "updatedAt")),
    })
}

/// The cycles, as health.json gives them: one file with every cycle, or the
/// older shape of one cycle running or the last one.
fn cycle(path: &Path) -> Value {
    let Some(r) = read_clean(path).filter(Value::is_object) else {
        return Value::Null;
    };
    let (cycles, last) = if r.get("cycles").is_some() {
        (field(&r, "cycles"), field(&r, "last"))
    } else if r.get("running") == Some(&json!(true)) {
        (json!([r.clone()]), Value::Null)
    } else {
        (json!([]), r.clone())
    };
    let cycles: Vec<Value> = cycles.as_array().into_iter().flatten().map(brief).collect();
    json!({"cycles": cycles, "last": if last.is_null() { Value::Null } else { brief(&last) }})
}

/// The tail of the console of the cycle running last, or of the one that
/// ended last. The name comes out of a file, so only a plain name is opened.
fn console(live: &Path, cycle: &Value, lines: usize) -> Value {
    let cycles = cycle
        .get("cycles")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let ended = cycles.is_empty();
    let which = cycles
        .last()
        .cloned()
        .unwrap_or_else(|| field(cycle, "last"));
    if which.is_null() {
        return Value::Null;
    }
    let issue = match field(&which, "issue") {
        Value::String(s) => s,
        other => other.to_string(),
    };
    let name = match field(&which, "console") {
        Value::Null | Value::Bool(false) => format!("console-{issue}.txt"),
        Value::String(s) => s,
        other => other.to_string(),
    };
    let plain = !name.is_empty()
        && !name.starts_with('.')
        && !name.contains("..")
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    if !plain {
        return Value::Null;
    }
    let Ok(bytes) = fs::read(live.join(&name)) else {
        return Value::Null;
    };
    if bytes.is_empty() {
        return Value::Null;
    }
    let text = sanitized(&bytes);
    let all: Vec<&str> = text.split_inclusive('\n').collect();
    let tail: Vec<String> = all[all.len().saturating_sub(lines)..]
        .iter()
        .map(|l| l.trim_end_matches('\n').to_string())
        .filter(|l| !l.is_empty())
        .collect();
    json!({"issue": issue, "name": name, "ended": ended, "lines": tail})
}

/// The energy sample, when it is fresh: the totals and the model slots.
fn energy(path: &Path, now: i64, max_age: i64) -> Value {
    let Some(e) = fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .filter(Value::is_object)
    else {
        return Value::Null;
    };
    let at = e
        .get("generatedAt")
        .and_then(Value::as_str)
        .and_then(epoch)
        .unwrap_or(0);
    if at <= now - max_age {
        return Value::Null;
    }
    let models: Vec<Value> = match e.get("models") {
        Some(Value::Array(a)) => a.clone(),
        Some(Value::Object(o)) => o.values().cloned().collect(),
        _ => Vec::new(),
    };
    let sum = |key: &str| -> Value {
        let n: Vec<&Value> = models
            .iter()
            .filter_map(|m| m.get(key))
            .filter(|v| v.is_number())
            .collect();
        if n.is_empty() {
            return Value::Null;
        }
        if n.iter().all(|v| v.is_i64()) {
            json!(n.iter().filter_map(|v| v.as_i64()).sum::<i64>())
        } else {
            json!(n.iter().filter_map(|v| v.as_f64()).sum::<f64>())
        }
    };
    let totals = e.get("totals").cloned().unwrap_or(Value::Null);
    json!({
        "time": field(&e, "generatedAt"),
        "watts": field(&totals, "watts"),
        "hosts": field(&totals, "hosts"),
        "slots": sum("slots"),
        "busy": sum("busy"),
    })
}

fn systemctl(args: &[&str]) -> Option<String> {
    let out = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Every tehas-* timer and service of the user's systemd. Nothing when
/// systemd cannot be asked at all (no user bus, another account).
fn units() -> Option<Value> {
    let listed = systemctl(&[
        "list-units",
        "--all",
        "--full",
        "--plain",
        "--no-legend",
        "tehas-*",
    ]);
    let files = systemctl(&["list-unit-files", "--no-legend", "tehas-*"]);
    if listed.is_none() && files.is_none() {
        return None;
    }
    let mut names: Vec<String> = [listed.unwrap_or_default(), files.unwrap_or_default()]
        .iter()
        .flat_map(|t| {
            t.lines()
                .filter_map(|l| l.split_whitespace().next())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect();
    names.sort();
    names.dedup();
    if names.is_empty() {
        return Some(json!([]));
    }
    let mut args = vec![
        "show",
        "-p",
        "Id,LoadState,ActiveState,SubState,UnitFileState,NextElapseUSecRealtime",
    ];
    args.extend(names.iter().map(String::as_str));
    let props = systemctl(&args)?;
    // One block of Key=Value lines per unit; a new Id= begins the next one.
    let mut units = Vec::new();
    let mut block: Option<Map<String, Value>> = None;
    let emit = |b: Map<String, Value>, units: &mut Vec<Value>| {
        let get = |k: &str| b.get(k).cloned().unwrap_or(json!(""));
        units.push(json!({
            "id": get("Id"), "loadState": get("LoadState"), "activeState": get("ActiveState"),
            "subState": get("SubState"), "unitFileState": get("UnitFileState"), "next": get("next"),
        }));
    };
    for line in props.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        if k == "Id" {
            if let Some(b) = block.take() {
                emit(b, &mut units);
            }
            block = Some(Map::new());
        }
        let Some(b) = block.as_mut() else { continue };
        b.insert(k.to_string(), json!(v));
        if k == "NextElapseUSecRealtime" {
            if let Some(clock) = clock_in(v) {
                b.insert("next".to_string(), json!(clock));
            }
        }
    }
    if let Some(b) = block {
        emit(b, &mut units);
    }
    Some(Value::Array(units))
}

/// The first HH:MM:SS in a text.
fn clock_in(v: &str) -> Option<String> {
    let b = v.as_bytes();
    (0..b.len().saturating_sub(7)).find_map(|i| {
        let w = &b[i..i + 8];
        let d = |j: usize| w[j].is_ascii_digit();
        (d(0) && d(1) && w[2] == b':' && d(3) && d(4) && w[5] == b':' && d(6) && d(7))
            .then(|| v[i..i + 8].to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_bytes_go_and_text_in_any_language_stays() {
        assert_eq!(sanitized(b"a\x1b[31mb\tc\nd\x7f"), "a[31mb\tc\nd");
        assert_eq!(sanitized("x\u{85}y é ü".as_bytes()), "xy é ü");
        assert_eq!(safe(json!({"a": ["x\u{9b}y\nz"]})), json!({"a": ["xyz"]}));
    }

    #[test]
    fn the_byte_filter_keeps_a_euro_sign_whose_byte_is_in_the_c1_range() {
        assert_eq!(
            sanitized(b"a\x07b\x09c\xc2\x9bd \xc3\xa4\xc3\xb6 \xe2\x82\xac e\x7ff\n"),
            "ab\tcd \u{e4}\u{f6} \u{20ac} ef\n"
        );
    }

    #[test]
    fn times_read_as_fromdateiso8601_reads_them() {
        assert_eq!(epoch("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(epoch("2026-10-31T00:00:00Z"), Some(1_793_404_800));
        assert_eq!(epoch("2026-10-31 00:00:00"), None);
    }

    #[test]
    fn the_clock_of_a_timer() {
        assert_eq!(
            clock_in("Mon 2026-01-01 10:00:00 UTC"),
            Some("10:00:00".to_string())
        );
        assert_eq!(clock_in("n/a"), None);
    }
}
