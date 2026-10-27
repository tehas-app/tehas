//! `factory_outcomes`: how the factory has done further back than the 30-day
//! tools go, from the counts file the analytics program writes for the host.
//!
//! It reads one file and copies only the keys named here. Counts only: the
//! file holds no issue number, title, failure cause or log content, and this
//! tool would not serve one if it did.

use crate::tools::{self, Dirs, ToolError};
use regex::Regex;
use serde_json::{json, Map, Value};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use tehas_core::time::{civil_from_days, days_from_civil};

/// The shape of the file, which is the shape of the tool's answer. The
/// analytics program writes it and checks it against the same numbers.
const API: i64 = 1;
/// How far back the counts reach: the retention of the factory's analytics.
const DAYS: i64 = 365;
const DAY: i64 = 86_400;
/// The name of the file, which the header of the file itself repeats.
const FILE: &str = "outcomes.jsonl";
/// A day the tool has not heard of for this long is a day the host has not
/// written the file: the writer runs once a day, so a late hour is no stale.
const FILE_STALE_SECS: i64 = 2 * 86_400;

/// The seconds of a bound: a day, `YYYY-MM-DD`, at its start, or a full UTC
/// time. `BOUND` has already said which of the two shapes the string has.
fn seconds(s: &str) -> Option<i64> {
    let parts: Vec<&str> = s.split('T').collect();
    let date = parts[0];
    let p: Vec<i64> = date.split('-').filter_map(|x| x.parse().ok()).collect();
    if p.len() != 3 {
        return None;
    }
    let (y, mo, d) = (p[0], p[1], p[2]);
    // The ranges of the reader in core, and the day must be a day the
    // calendar holds: 2026-02-30 counts on into March and is no date.
    if !(1970..=2100).contains(&y) || !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return None;
    }
    let days = days_from_civil(y, mo, d);
    let (ry, rm, rd) = civil_from_days(days);
    if (ry, rm, rd) != (y, mo, d) {
        return None;
    }
    let time = match parts.get(1) {
        None | Some(&"") if parts.len() == 1 => 0,
        Some(t) => {
            let t = t.strip_suffix('Z')?;
            let h: Vec<i64> = t.split(':').filter_map(|x| x.parse().ok()).collect();
            if h.len() != 3 || h[0] > 23 || h[1] > 59 || h[2] > 60 {
                return None;
            }
            h[0] * 3600 + h[1] * 60 + h[2]
        }
        _ => return None,
    };
    days.checked_mul(DAY)?.checked_add(time)
}

/// The seconds of the Monday that opens the week named by a `YYYY-MM-DD`, or
/// None for a week that is no date. The weeks are the analytics program's own:
/// UTC, and beginning on Monday.
fn week_start(week: &str) -> Option<i64> {
    let days = seconds(week)? / DAY;
    // 1970-01-01 was a Thursday, so four days on from a Thursday is a Monday.
    let monday = days - (days + 4).rem_euclid(7);
    Some(monday * DAY)
}

/// The groupings, in the order the tool names them.
pub const BYS: [&str; 2] = ["version", "week"];

/// A version is matched exactly, as the session tools match theirs, so the
/// same shape of word is accepted here.
pub static WORD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9._+-]{1,64}$").unwrap());
/// A bound: a day, `YYYY-MM-DD`, or a full UTC time as the live files give it.
pub static BOUND: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}(T[0-9]{2}:[0-9]{2}:[0-9]{2}Z)?$").unwrap()
});

fn bad(why: &str) -> ToolError {
    ToolError::BadArgs(why.to_string())
}

fn word(args: &Value, key: &str) -> Result<Option<String>, ToolError> {
    match &args[key] {
        Value::Null => Ok(None),
        Value::String(s) if WORD.is_match(s) => Ok(Some(s.clone())),
        _ => Err(bad(&format!("{key} is 1 to 64 of A-Za-z0-9._+-"))),
    }
}

fn bound(args: &Value, key: &str) -> Result<Option<i64>, ToolError> {
    let why = || bad(&format!("{key} is YYYY-MM-DD or YYYY-MM-DDTHH:MM:SSZ"));
    match &args[key] {
        Value::Null => Ok(None),
        Value::String(s) if BOUND.is_match(s) => seconds(s).map(Some).ok_or_else(why),
        _ => Err(why()),
    }
}

/// The header's own stale bound: the host writes the file once a day, so a
/// day without it is a host that has not written it.
fn file_stale(generated_at: &str, now: i64) -> bool {
    tools::epoch(generated_at).is_none_or(|t| now - t > FILE_STALE_SECS)
}

/// The counts file the host writes to ~/tehas/mcp/outcomes.jsonl: a header
/// line, then one entry per line. Never published.
pub struct File_ {
    pub by: String,
    pub generated_at: String,
    /// The host has not written it for a day.
    pub stale: bool,
    path: PathBuf,
}

fn bad_file(why: &str) -> ToolError {
    ToolError::File {
        file: FILE.into(),
        why: why.into(),
    }
}

impl File_ {
    /// Reads and checks the header only; the entries are read when asked for.
    pub fn open(dir: &Path, now: i64) -> Result<File_, ToolError> {
        let path = dir.join(FILE);
        let f = File::open(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                bad_file("not written")
            } else {
                bad_file("could not be read")
            }
        })?;
        let mut first = String::new();
        BufReader::new(f)
            .read_line(&mut first)
            .map_err(|_| bad_file("could not be read"))?;
        let head: Value = serde_json::from_str(&first).map_err(|_| bad_file("has no header"))?;
        if head["api"] != API {
            return Err(bad_file("has a header of another api"));
        }
        // The header names the file it was written for. A reader that does not
        // check it would serve an old or foreign file as if it were the counts.
        if head["file"] != FILE {
            return Err(bad_file("has a header for another file"));
        }
        let by = head["by"]
            .as_str()
            .filter(|b| BYS.contains(b))
            .ok_or_else(|| bad_file("has a header without a grouping"))?
            .to_string();
        let generated_at = head["generatedAt"].as_str().unwrap_or("").to_string();
        let stale = file_stale(&generated_at, now);
        Ok(File_ {
            by,
            generated_at,
            stale,
            path,
        })
    }

    /// The entries, one line at a time. A line that is not a JSON object is
    /// skipped, as in the snapshot; a read error ends the entries.
    pub fn entries(&self) -> impl Iterator<Item = Value> {
        File::open(&self.path)
            .ok()
            .into_iter()
            // Bytes, not lines of text: a line that is not UTF-8 is skipped
            // like any other bad line instead of ending the reading.
            .flat_map(|f| BufReader::new(f).split(b'\n').skip(1))
            .map_while(Result::ok)
            .filter_map(|l| serde_json::from_slice::<Value>(&l).ok())
            .filter(Value::is_object)
    }
}

pub(crate) fn call(dirs: &Dirs, args: &Value) -> Result<Map<String, Value>, ToolError> {
    let by = match &args["by"] {
        Value::Null => "version",
        Value::String(s) if BYS.contains(&s.as_str()) => s.as_str(),
        _ => return Err(bad(&format!("by is one of {}", BYS.join(", ")))),
    };
    let tehas = word(args, "tehasVersion")?;
    let rukas = word(args, "rukasVersion")?;
    let since = bound(args, "since")?;
    let until = bound(args, "until")?;

    let file = File_::open(&dirs.outcomes, tools::now())?;
    // The host writes the file grouped one way and the header says which. An
    // argument that asks for the other cannot be answered from it, so it says
    // so rather than answering with nothing.
    if file.by != by {
        return Err(bad(&format!(
            "the host writes its counts grouped by {}, not {by}",
            file.by
        )));
    }
    let spec = crate::outcomes_schema::spec(&file.by);
    let entries: Vec<Value> = file
        .entries()
        .filter(|e| {
            let named = |key: &str, want: &Option<String>| {
                want.as_ref()
                    .is_none_or(|w| e[key].as_str() == Some(w.as_str()))
            };
            named("tehasVersion", &tehas)
                && named("rukasVersion", &rukas)
                && in_window(e, since, until)
        })
        .map(|e| match tools::pick(&e, &spec) {
            Some(Value::Object(m)) => Value::Object(m),
            _ => Value::Null,
        })
        .filter(Value::is_object)
        .collect();
    let mut out = Map::new();
    out.insert("by".into(), json!(file.by));
    out.insert("days".into(), json!(DAYS));
    out.insert("generatedAt".into(), json!(file.generated_at));
    out.insert("stale".into(), json!(file.stale));
    out.insert("entries".into(), Value::Array(entries));
    Ok(out)
}

/// Whether an entry falls in the window, which is given in whole days. A
/// week's entry stands for seven days and passes when any of them is in it.
fn in_window(e: &Value, since: Option<i64>, until: Option<i64>) -> bool {
    let (first, last) = entry_days(e);
    since.is_none_or(|s| last >= day(s)) && until.is_none_or(|u| first <= day(u))
}

fn day(t: i64) -> i64 {
    t.div_euclid(DAY)
}

/// The first and last day an entry stands for. A version pair holds runs of
/// every day the file reaches, and so does an entry that names no week or
/// names one that is no date: it is kept, as a filter cannot be answered from
/// a count of what it leaves out.
fn entry_days(e: &Value) -> (i64, i64) {
    match e["week"].as_str().and_then(week_start) {
        Some(start) => (day(start), day(start) + 6),
        None => (i64::MIN, i64::MAX),
    }
}
