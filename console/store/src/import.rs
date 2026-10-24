//! `store import [--dry-run]`: the records an older tehas kept as files under
//! `state/`, made rows, and each file moved to `state/imported-<YYYYMMDD>/`
//! once its rows read back. Spec section 7, as the store-callers plan
//! (docs/plans/2026-10-24-store-callers.md, section 6) changes it: the import
//! runs again whenever old files are there, and nothing is deleted.
//!
//! - The home is proved first: `$TEHAS_HOME/.tehas-home` must be there, or the
//!   verb exits 78 `unmarked` and touches nothing (B6).
//! - Anything to import while a cycle lock or a planning turn's lock is held,
//!   or cannot be probed, is exit 1 `live`, nothing written.
//! - A candidate is a regular file of at most 1 MiB that parses. A link, a
//!   directory, a larger file, one that does not parse, and a time that is not
//!   `%FT%TZ` hold the file and every other file of its issue's group: they
//!   stay, and the verb exits 1 `held` with one line per file.
//! - One transaction per issue's group (its cycle files, or its planning
//!   files), the kinds in the spec's order; rows by natural key and `INSERT OR
//!   IGNORE`, so a second run changes nothing. The group's files move only
//!   after all its rows are read back from the store, the start files last; a
//!   file left by a run that stopped is planned against the row on record.
//! - A history keeps the 50 newest rows of an issue, as the verbs do.
//! - `--dry-run` reads the files and nothing else: it does not open the store,
//!   so it works before `init` and `migrate`, and it counts the rows the files
//!   hold, not the rows a run would add.
//!
//! The carriers (`result.json`, `progress.json`, `.runner_exit`, `.status`,
//! `.oom`, `.forks`, `.stalled`, `.timed`, `.nostart`, `.done`, `.stopped`,
//! `.cid`, `.manifest.json`, `.images.md`, `.retry-context.md`, a plan's
//! `.out`, `.rc`, `.lock`), `state/refined.jsonl` and the `.keep` directories
//! are not the store's: they stay where they are.

use crate::cycle::{self, SettingsIn, END_REASONS};
use crate::lock;
use crate::plan;
use crate::verbs::{self, cap_char, j, kebab_char, line, list, obj, stub, word_char, Fail, Q};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, DirBuilder, File};
use std::io::{ErrorKind, Read};
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Path, PathBuf};

/// The most a record file may hold.
const BOUND: u64 = 1 << 20;
/// The most rows a fixer or refresh record may ask the import to make up.
const MAX_STARTS: i64 = 100;

const MARKER: &str = ".tehas-home";

/// The kinds of row, in the order of the answer's `imported` object.
const TABLES: [&str; 11] = [
    "cycles",
    "approvals",
    "parks",
    "questions",
    "cycle_settings",
    "cycle_history",
    "planning",
    "planning_answers",
    "planning_filed",
    "planning_notes",
    "plan_history",
];

/// The kinds of row in the order of spec section 7, the order of the writes
/// in a group's transaction. `Moved` is the files that become no row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Step {
    Cycles,
    Approvals,
    Parks,
    Questions,
    Settings,
    CycleHistory,
    Planning,
    PlanHistory,
    Moved,
}

// --- times -----------------------------------------------------------------------

/// `%FT%TZ` as Unix seconds, or None. Nothing else is taken: no fraction, no
/// offset, no second 60.
pub fn iso(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let shape = b.len() == 20
        && b[4] == b'-'
        && b[7] == b'-'
        && b[10] == b'T'
        && b[13] == b':'
        && b[16] == b':'
        && b[19] == b'Z';
    if !shape {
        return None;
    }
    let num = |from: usize, to: usize| -> Option<i64> {
        let t = &s[from..to];
        if t.bytes().all(|c| c.is_ascii_digit()) {
            t.parse().ok()
        } else {
            None
        }
    };
    let (y, mo, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (h, mi, se) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if !(1..=12).contains(&mo)
        || d < 1
        || d > days[(mo - 1) as usize]
        || h > 23
        || mi > 59
        || se > 59
    {
        return None;
    }
    Some(days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + se)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The UTC day of a Unix second as YYYYMMDD.
pub fn day(t: i64) -> String {
    let z = t.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}{m:02}{d:02}")
}

// --- the files -------------------------------------------------------------------

/// What a file under `state/` is, by its name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum What {
    Taken,
    By,
    Requeued,
    Rukas,
    Image,
    Branch,
    Settings,
    Approved,
    Park,
    Quiet,
    History,
    ParkId,
    ParkBody,
    Plan,
    PlanBrief,
    PlanAnswer,
    PlanTurn,
    PlanHistory,
}

const CYCLE_SUFFIXES: [(&str, What); 11] = [
    (".approved.json", What::Approved),
    (".history.jsonl", What::History),
    (".taken", What::Taken),
    (".by", What::By),
    (".requeued", What::Requeued),
    (".rukas", What::Rukas),
    (".image", What::Image),
    (".branch", What::Branch),
    (".settings", What::Settings),
    (".park", What::Park),
    (".quiet", What::Quiet),
];

const STATE_NAMES: [(&str, &str, What); 7] = [
    ("park-", ".id", What::ParkId),
    ("park-", ".json", What::ParkBody),
    ("plan-", ".json", What::Plan),
    ("plan-", ".json.brief", What::PlanBrief),
    ("plan-", ".json.answer", What::PlanAnswer),
    ("plan-", ".json.turn", What::PlanTurn),
    ("plan-", ".history.jsonl", What::PlanHistory),
];

/// A file found by its name: the path, the path under `state/`, its issue.
#[derive(Clone, Debug)]
struct Found {
    path: PathBuf,
    rel: String,
    issue: i64,
    what: What,
}

impl Found {
    fn plan(&self) -> bool {
        matches!(
            self.what,
            What::Plan | What::PlanBrief | What::PlanAnswer | What::PlanTurn | What::PlanHistory
        )
    }
}

/// A file as it was read: its bytes, mtime and identity.
struct Got {
    bytes: Vec<u8>,
    mtime: i64,
    dev: u64,
    ino: u64,
}

fn names(dir: &Path) -> Result<Vec<String>, Fail> {
    let shown = dir.display();
    match fs::symlink_metadata(dir) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => {
            return Err((
                78,
                format!("unreadable {shown} is not a directory; nothing was imported"),
            ))
        }
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err((78, format!("unreadable {shown}: {e}; nothing was imported"))),
    }
    let mut out = Vec::new();
    let entries = fs::read_dir(dir)
        .map_err(|e| (78, format!("unreadable {shown}: {e}; nothing was imported")))?;
    for entry in entries {
        let entry =
            entry.map_err(|e| (78, format!("unreadable {shown}: {e}; nothing was imported")))?;
        if let Some(name) = entry.file_name().to_str() {
            out.push(name.to_string());
        }
    }
    out.sort();
    Ok(out)
}

/// Every candidate under `state/` and `state/cycles/`, by name alone.
fn scan(home: &Path) -> Result<Vec<Found>, Fail> {
    let state = home.join("state");
    let mut found = Vec::new();
    for name in names(&state.join("cycles"))? {
        let hit = name
            .strip_prefix("plan-")
            .and_then(|r| r.strip_suffix(".history.jsonl"))
            .and_then(verbs::number)
            .map(|n| (n, What::PlanHistory))
            .or_else(|| {
                CYCLE_SUFFIXES.iter().find_map(|(suffix, what)| {
                    name.strip_suffix(suffix)
                        .and_then(verbs::number)
                        .map(|n| (n, *what))
                })
            });
        if let Some((issue, what)) = hit {
            found.push(Found {
                path: state.join("cycles").join(&name),
                rel: format!("cycles/{name}"),
                issue,
                what,
            });
        }
    }
    for name in names(&state)? {
        let hit = STATE_NAMES.iter().find_map(|(prefix, suffix, what)| {
            name.strip_prefix(prefix)
                .and_then(|r| r.strip_suffix(suffix))
                .and_then(verbs::number)
                .map(|n| (n, *what))
        });
        if let Some((issue, what)) = hit {
            found.push(Found {
                path: state.join(&name),
                rel: name,
                issue,
                what,
            });
        }
    }
    Ok(found)
}

/// A regular file, the same one after it is opened, of at most BOUND bytes.
fn read(path: &Path) -> Result<Got, String> {
    let before = fs::symlink_metadata(path).map_err(|e| format!("cannot be read: {e}"))?;
    if !before.file_type().is_file() {
        return Err("is not a regular file".to_string());
    }
    let f = File::open(path).map_err(|e| format!("cannot be read: {e}"))?;
    let m = f.metadata().map_err(|e| format!("cannot be read: {e}"))?;
    if m.dev() != before.dev() || m.ino() != before.ino() || !m.is_file() {
        return Err("was replaced while it was read".to_string());
    }
    if m.len() > BOUND {
        return Err(format!("is over {BOUND} bytes"));
    }
    let mut bytes = Vec::new();
    f.take(BOUND + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("cannot be read: {e}"))?;
    if bytes.len() as u64 > BOUND {
        return Err(format!("is over {BOUND} bytes"));
    }
    Ok(Got {
        bytes,
        mtime: m.mtime(),
        dev: m.dev(),
        ino: m.ino(),
    })
}

// --- values ----------------------------------------------------------------------

fn text(g: &Got) -> Result<&str, String> {
    std::str::from_utf8(&g.bytes).map_err(|_| "is not UTF-8".to_string())
}

/// The first line, trimmed; empty is None.
fn first_line(g: &Got) -> Result<Option<String>, String> {
    let t = text(g)?;
    let l = t.lines().next().unwrap_or("").trim();
    Ok((!l.is_empty()).then(|| l.to_string()))
}

fn object(bytes: &[u8]) -> Result<Map<String, Value>, String> {
    match serde_json::from_slice::<Value>(bytes) {
        Ok(Value::Object(m)) => Ok(m),
        Ok(_) => Err("is not a JSON object".to_string()),
        Err(_) => Err("is not JSON".to_string()),
    }
}

fn clean(s: Option<&str>, keep: fn(u8) -> bool, max: usize) -> Option<String> {
    let mut v: String = s?
        .chars()
        .filter(|c| c.is_ascii() && keep(*c as u8))
        .collect();
    v.truncate(max);
    (!v.is_empty()).then_some(v)
}

/// A field that is absent or null is None; one of another type is the error.
fn opt_str<'a>(m: &'a Map<String, Value>, k: &str) -> Result<Option<&'a str>, String> {
    match m.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(format!("has a {k} that is not a string")),
    }
}

fn opt_bool(m: &Map<String, Value>, k: &str) -> Result<bool, String> {
    match m.get(k) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err(format!("has a {k} that is not true or false")),
    }
}

fn opt_int(m: &Map<String, Value>, k: &str) -> Result<Option<i64>, String> {
    match m.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => n
            .as_i64()
            .map(Some)
            .ok_or_else(|| format!("has a {k} that is not a whole number")),
        Some(_) => Err(format!("has a {k} that is not a number")),
    }
}

fn opt_time(m: &Map<String, Value>, k: &str) -> Result<Option<i64>, String> {
    match opt_str(m, k)? {
        None => Ok(None),
        Some(s) => iso(s)
            .map(Some)
            .ok_or_else(|| format!("has a {k} that is not a %FT%TZ time: {}", verbs::shown(s))),
    }
}

fn need_time(m: &Map<String, Value>, k: &str) -> Result<i64, String> {
    opt_time(m, k)?.ok_or_else(|| format!("has no {k}"))
}

fn need<T>(v: Option<T>, what: &str) -> Result<T, String> {
    v.ok_or_else(|| format!("has no {what} of its shape"))
}

fn account(m: &Map<String, Value>, k: &str) -> Result<Option<String>, String> {
    match opt_str(m, k)? {
        None | Some("") => Ok(None),
        Some(s) => verbs::account(s)
            .map(Some)
            .ok_or_else(|| format!("has a {k} that is not an account")),
    }
}

/// The number of a pull request of a GitHub URL.
fn pr_number(url: &str) -> Option<i64> {
    let rest = url.strip_prefix("https://github.com/")?;
    let parts: Vec<&str> = rest.split('/').collect();
    match parts.as_slice() {
        [owner, repo, "pull", n] if !owner.is_empty() && !repo.is_empty() => verbs::number(n),
        _ => None,
    }
}

fn label_of(how: &str) -> String {
    if how == "label" || how == "fixer" {
        "tehas:ready".to_string()
    } else {
        how.to_string()
    }
}

// --- the records as parsed -------------------------------------------------------

/// A line of `state/cycles/<n>.history.jsonl`.
#[derive(Clone, Debug)]
struct CycleLine {
    at: i64,
    started: Option<i64>,
    status: String,
    step: Option<String>,
    rc: Option<i64>,
    cap: Option<String>,
    park_reason: Option<String>,
    failure_kind: Option<String>,
    failure_cause: Option<String>,
    provider: bool,
    parked: bool,
    pr: Option<i64>,
    by: Option<String>,
    how: Option<String>,
}

impl CycleLine {
    fn key(&self) -> (i64, String, Option<i64>) {
        (self.at, self.status.clone(), self.started)
    }

    fn reason(&self) -> &str {
        if END_REASONS.contains(&self.status.as_str()) {
            &self.status
        } else {
            "unknown"
        }
    }
}

fn cycle_lines(g: &Got) -> Result<Vec<CycleLine>, String> {
    let mut out = Vec::new();
    for (i, l) in text(g)?.lines().enumerate() {
        if l.trim().is_empty() {
            continue;
        }
        let m = object(l.as_bytes()).map_err(|e| format!("line {}: {e}", i + 1))?;
        let at = need_time(&m, "at").map_err(|e| format!("line {}: {e}", i + 1))?;
        let s = |k: &str| opt_str(&m, k).map_err(|e| format!("line {}: {e}", i + 1));
        let started = opt_int(&m, "started").map_err(|e| format!("line {}: {e}", i + 1))?;
        let rc = opt_int(&m, "rc").map_err(|e| format!("line {}: {e}", i + 1))?;
        out.push(CycleLine {
            at,
            started,
            status: clean(s("status")?, word_char, 40).unwrap_or_else(|| "unknown".to_string()),
            step: clean(s("step")?, word_char, 40),
            rc: rc.filter(|n| *n >= 0),
            cap: clean(s("cap")?, cap_char, 80),
            park_reason: clean(s("park_reason")?, kebab_char, 40),
            failure_kind: clean(s("failure_kind")?, kebab_char, 40),
            failure_cause: clean(s("failure_cause")?, cap_char, 60),
            provider: opt_bool(&m, "provider").map_err(|e| format!("line {}: {e}", i + 1))?,
            parked: opt_bool(&m, "parked").map_err(|e| format!("line {}: {e}", i + 1))?,
            pr: s("pr")?.and_then(pr_number),
            by: s("by")?.and_then(verbs::account),
            how: clean(s("how")?, kebab_char, 40),
        });
    }
    Ok(out)
}

/// A line of a planning turn's history.
#[derive(Clone, Debug)]
struct PlanLine {
    at: i64,
    turn: i64,
    outcome: &'static str,
    started: Option<i64>,
    rc: Option<i64>,
    by: Option<String>,
    label: Option<String>,
    filed: Vec<String>,
}

fn plan_lines(g: &Got) -> Result<Vec<PlanLine>, String> {
    let mut out = Vec::new();
    for (i, l) in text(g)?.lines().enumerate() {
        if l.trim().is_empty() {
            continue;
        }
        let at_line = |e: String| format!("line {}: {e}", i + 1);
        let m = object(l.as_bytes()).map_err(at_line)?;
        let at = need_time(&m, "at").map_err(at_line)?;
        let status = opt_str(&m, "status").map_err(at_line)?.unwrap_or("");
        let outcome = plan::OUTCOMES
            .iter()
            .find(|o| **o == status)
            .copied()
            .ok_or_else(|| {
                at_line(format!(
                    "has an outcome it does not know: {}",
                    verbs::shown(status)
                ))
            })?;
        let turn = opt_int(&m, "turn")
            .map_err(at_line)?
            .filter(|n| *n >= 1)
            .ok_or_else(|| at_line("has no turn".to_string()))?;
        // tehas wrote started and rc as strings.
        let digits = |k: &str, grammar: fn(&str) -> Option<i64>| -> Result<Option<i64>, String> {
            match m.get(k) {
                None | Some(Value::Null) => Ok(None),
                Some(Value::Number(n)) => {
                    Ok(n.as_i64().filter(|n| grammar(&n.to_string()).is_some()))
                }
                Some(Value::String(s)) => Ok(grammar(s)),
                Some(_) => Err(at_line(format!("has a {k} that is not a number"))),
            }
        };
        let filed = match m.get("filed") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(a)) => a
                .iter()
                .filter_map(Value::as_str)
                .filter(|u| plan::filed_issue(u).is_some())
                .map(str::to_string)
                .collect(),
            Some(_) => return Err(at_line("has a filed that is not a list".to_string())),
        };
        let label = opt_str(&m, "label")
            .map_err(at_line)?
            .filter(|l| *l == "tehas:plan" || *l == "tehas:refine")
            .map(str::to_string);
        out.push(PlanLine {
            at,
            turn,
            outcome,
            started: digits("started", verbs::unix)?,
            rc: digits("rc", verbs::count)?,
            by: opt_str(&m, "by").map_err(at_line)?.and_then(verbs::account),
            label,
            filed,
        });
    }
    Ok(out)
}

/// `state/cycles/<n>.approved.json`.
#[derive(Clone, Debug)]
struct ApprovalIn {
    at: i64,
    by: String,
    how: &'static str,
    body_sha: String,
    title_sha: Option<String>,
    refresh: Option<(i64, String)>,
    input: Option<String>,
    /// The base a fixer or refresh start rests on, and the starts on it the
    /// record counts beside its own: (base, fixer starts, refreshes).
    rests: Option<(Base, i64, i64)>,
}

#[derive(Clone, Debug)]
struct Base {
    at: i64,
    by: String,
    how: String,
    body_sha: String,
}

fn approval_in(g: &Got) -> Result<ApprovalIn, String> {
    let m = object(&g.bytes)?;
    let at = need_time(&m, "at")?;
    let by = need(account(&m, "by")?, "by")?;
    let how_word = opt_str(&m, "how")?.unwrap_or("");
    let how = crate::approval::HOWS
        .iter()
        .find(|h| **h == how_word)
        .copied()
        .ok_or_else(|| format!("has a how it does not know: {}", verbs::shown(how_word)))?;
    let body_sha = need(
        opt_str(&m, "body_sha256")?.and_then(verbs::sha256),
        "body_sha256",
    )?;
    let title_sha = match opt_str(&m, "title_sha256")? {
        None => None,
        Some(s) => Some(verbs::sha256(s).ok_or("has a title_sha256 that is not a sha256")?),
    };
    let mut input = Map::new();
    for k in ["links", "comment_links", "retry_context", "refresh_of"] {
        if let Some(v) = m.get(k) {
            input.insert(k.to_string(), v.clone());
        }
    }
    let refresh = if how == "refresh" {
        let of = match m.get("refresh_of") {
            Some(Value::Object(o)) => o,
            _ => return Err("is a refresh with no refresh_of".to_string()),
        };
        let pr = need(opt_int(of, "pr")?.filter(|n| *n >= 1), "refresh_of.pr")?;
        let head = need(
            opt_str(of, "head_sha")?.and_then(verbs::head_sha),
            "refresh_of.head_sha",
        )?;
        Some((pr, head))
    } else {
        None
    };
    let rests = if how == "fixer" || how == "refresh" {
        let r = match m.get("rests_on") {
            Some(Value::Object(o)) => o,
            _ => return Err(format!("is a {how} start with no rests_on")),
        };
        let base_how = opt_str(r, "how")?.unwrap_or("");
        if !["label", "retry-tick", "outage-retry"].contains(&base_how) {
            return Err("rests on an approval that is not a person's".to_string());
        }
        let base = Base {
            at: need_time(r, "at")?,
            by: need(account(r, "by")?, "rests_on.by")?,
            how: base_how.to_string(),
            body_sha: need(
                opt_str(r, "body_sha256")?.and_then(verbs::sha256),
                "rests_on.body_sha256",
            )?,
        };
        let starts = opt_int(&m, "fixer_starts")?.unwrap_or(0);
        let refreshes = opt_int(&m, "refreshes")?.unwrap_or(0);
        let ok = if how == "fixer" {
            starts >= 1 && refreshes == 0
        } else {
            starts >= 0 && refreshes >= 1
        };
        if !ok || starts + refreshes > MAX_STARTS {
            return Err("does not say how many starts rest on its base".to_string());
        }
        Some((base, starts, refreshes))
    } else {
        None
    };
    Ok(ApprovalIn {
        at,
        by,
        how,
        body_sha,
        title_sha,
        refresh,
        input: (!input.is_empty()).then(|| Value::Object(input).to_string()),
        rests,
    })
}

/// `state/cycles/<n>.park`.
#[derive(Clone, Debug)]
struct ParkIn {
    at: i64,
    reason: Option<String>,
    retry: bool,
    told: bool,
}

fn park_in(g: &Got) -> Result<ParkIn, String> {
    let m = object(&g.bytes)?;
    let provider = opt_bool(&m, "provider")?;
    let cap = clean(opt_str(&m, "cap")?, cap_char, 80);
    opt_str(&m, "reason")?;
    opt_int(&m, "started")?;
    Ok(ParkIn {
        at: g.mtime,
        reason: if provider {
            Some("provider-outage".to_string())
        } else {
            cap
        },
        retry: opt_bool(&m, "retry")?,
        told: opt_bool(&m, "held")?,
    })
}

/// `state/plan-<n>.json`.
#[derive(Clone, Debug, Default)]
struct PlanIn {
    session: Option<String>,
    turns: i64,
    options: Vec<String>,
    pending: bool,
    failed: bool,
    running: bool,
    lost: bool,
    started_at: Option<i64>,
    comment: Option<i64>,
    by: Option<String>,
    label: Option<String>,
    answers: Vec<(i64, String, bool)>,
    filed: Vec<String>,
    notes: Vec<(&'static str, i64)>,
    mtime: i64,
}

fn ids(m: &Map<String, Value>, k: &str) -> Result<Vec<i64>, String> {
    match m.get(k) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| {
                v.as_i64()
                    .filter(|n| verbs::comment_id(&n.to_string()).is_some())
                    .ok_or_else(|| format!("has a {k} that is not a list of comment ids"))
            })
            .collect(),
        Some(_) => Err(format!("has a {k} that is not a list")),
    }
}

fn plan_in(g: &Got) -> Result<PlanIn, String> {
    let m = object(&g.bytes)?;
    let options = match m.get("options") {
        None | Some(Value::Null) => Vec::new(),
        // Labels the runner wrote, kept as plan-end keeps them; one that is not
        // text is `-`.
        Some(Value::Array(a)) => a
            .iter()
            .take(plan::MAX_OPTIONS)
            .map(|v| plan::option_label(v.as_str().unwrap_or("")))
            .collect(),
        Some(_) => return Err("has options that are not a list".to_string()),
    };
    let answers = match m.get("answers") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| {
                let o = v.as_object().ok_or("has an answer that is not an object")?;
                let turn = opt_int(o, "turn")?
                    .filter(|n| *n >= 1)
                    .ok_or("has an answer with no turn")?;
                let by = need(account(o, "by")?, "answer by")?;
                Ok((turn, by, opt_bool(o, "fixer")?))
            })
            .collect::<Result<_, String>>()?,
        Some(_) => return Err("has answers that are not a list".to_string()),
    };
    let filed = match m.get("filed") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(Value::as_str)
            .filter(|u| plan::filed_issue(u).is_some())
            .map(str::to_string)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        Some(_) => return Err("has a filed that is not a list".to_string()),
    };
    let label = match opt_str(&m, "label")? {
        None => None,
        Some(l @ ("tehas:plan" | "tehas:refine")) => Some(l.to_string()),
        Some(l) => return Err(format!("has a label it does not know: {}", verbs::shown(l))),
    };
    let comment = match opt_int(&m, "comment")? {
        None => None,
        Some(n) => Some(
            verbs::comment_id(&n.to_string()).ok_or("has a comment that is not a comment id")?,
        ),
    };
    let mut notes: Vec<(&'static str, i64)> = Vec::new();
    for id in ids(&m, "fixer_refused")? {
        notes.push(("fixer-refused", id));
    }
    for id in ids(&m, "held")? {
        notes.push(("held", id));
    }
    Ok(PlanIn {
        session: opt_str(&m, "session")?
            .filter(|s| plan::session_ok(s))
            .map(str::to_string),
        turns: opt_int(&m, "turns")?.unwrap_or(0).max(0),
        options,
        pending: opt_bool(&m, "pending")?,
        failed: opt_bool(&m, "failed")?,
        running: opt_bool(&m, "running")?,
        lost: opt_bool(&m, "lost")?,
        started_at: opt_time(&m, "startedAt")?,
        comment,
        by: account(&m, "by")?,
        label,
        answers,
        filed,
        notes,
        mtime: g.mtime,
    })
}

// --- the rows --------------------------------------------------------------------

#[derive(Clone, Debug)]
struct CycleRow {
    attempt: i64,
    started_at: i64,
    started_by: Option<String>,
    how: String,
    requeued: bool,
    rukas: Option<String>,
    image: Option<String>,
    branch: Option<String>,
    end_reason: String,
    ended_at: i64,
}

#[derive(Clone, Debug)]
struct ApprovalRow {
    at: i64,
    by: String,
    how: String,
    body_sha: String,
    title_sha: Option<String>,
    refresh: Option<(i64, String)>,
    used_at: i64,
    used_by: i64,
    input: Option<String>,
}

#[derive(Clone, Debug)]
struct PlanningRow {
    p: PlanIn,
    brief: Option<String>,
    pending_outcome: Option<String>,
    opened_at: i64,
}

/// One write of a step, and what reads it back.
#[derive(Clone, Debug)]
enum Op {
    CycleInsert(i64, CycleRow),
    /// A start the files know that is later than the row on record.
    CycleUpdate(i64, i64, CycleRow),
    /// The open row started at .1 ends at .2 with the word .3.
    CycleEnd(i64, i64, i64, String),
    Approval(i64, ApprovalRow),
    Park {
        issue: i64,
        attempt: i64,
        at: i64,
        reason: String,
        step: Option<String>,
        pr: Option<i64>,
        retries: i64,
        told_at: Option<i64>,
    },
    Question {
        id: i64,
        issue: i64,
        attempt: i64,
        kind: &'static str,
        posted_at: i64,
    },
    Setting(i64, String, String),
    CycleHistory(i64, i64, CycleLine),
    Planning(i64, Box<PlanningRow>),
    Answer(i64, i64, String, bool, Option<String>),
    Filed(i64, String, i64, i64),
    Note(i64, &'static str, i64, i64),
    PlanHistory(i64, PlanLine),
    /// The issue's history cut to the HISTORY newest rows, as the verbs keep it.
    TrimCycleHistory(i64),
    TrimPlanHistory(i64),
}

impl Op {
    /// The table the answer counts it under.
    fn table(&self) -> &'static str {
        match self {
            Op::CycleInsert(..) | Op::CycleUpdate(..) | Op::CycleEnd(..) => "cycles",
            Op::Approval(..) => "approvals",
            Op::Park { .. } => "parks",
            Op::Question { .. } => "questions",
            Op::Setting(..) => "cycle_settings",
            Op::CycleHistory(..) | Op::TrimCycleHistory(..) => "cycle_history",
            Op::Planning(..) => "planning",
            Op::Answer(..) => "planning_answers",
            Op::Filed(..) => "planning_filed",
            Op::Note(..) => "planning_notes",
            Op::PlanHistory(..) | Op::TrimPlanHistory(..) => "plan_history",
        }
    }

    /// Whether the answer counts it: a trim writes no row.
    fn counted(&self) -> bool {
        !matches!(self, Op::TrimCycleHistory(..) | Op::TrimPlanHistory(..))
    }

    /// Where it goes in its group's transaction: by step, and a history's
    /// lines newest first, so a line older than the newest HISTORY on record
    /// is never written; the trim last.
    fn order(&self, step: Step) -> (Step, std::cmp::Reverse<i64>) {
        let at = match self {
            Op::CycleHistory(_, _, l) => l.at,
            Op::PlanHistory(_, l) => l.at,
            Op::TrimCycleHistory(..) | Op::TrimPlanHistory(..) => i64::MIN,
            _ => 0,
        };
        (step, std::cmp::Reverse(at))
    }

    /// The rows it wrote: 0 when they were there already.
    fn apply(&self, c: &Connection) -> Result<usize, Fail> {
        Ok(match self {
            Op::CycleInsert(issue, r) => {
                stub(c, *issue)?;
                c.execute(
                    "INSERT OR IGNORE INTO cycles (issue, attempt, started_at, started_by, how,
                       requeued, rukas_commit, image, branch, end_reason, ended_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    params![
                        issue, r.attempt, r.started_at, r.started_by, r.how, r.requeued,
                        r.rukas, r.image, r.branch, r.end_reason, r.ended_at
                    ],
                )
                .q()?
            }
            Op::CycleUpdate(issue, was, r) => {
                let n = c
                    .execute(
                        "UPDATE cycles SET attempt = ?3, started_at = ?4, started_by = ?5, how = ?6,
                           requeued = ?7, rukas_commit = ?8, image = ?9, branch = ?10,
                           end_reason = ?11, ended_at = ?12, oom_kills = NULL,
                           forks_refused = NULL, stalled_seconds = NULL, timed_out = 0,
                           runner_exit = NULL, watched = 0, host_saw = NULL,
                           stop_asked_at = NULL, stop_said_times = 0
                         WHERE issue = ?1 AND started_at = ?2",
                        params![
                            issue, was, r.attempt, r.started_at, r.started_by, r.how,
                            r.requeued, r.rukas, r.image, r.branch, r.end_reason, r.ended_at
                        ],
                    )
                    .q()?;
                if n > 0 {
                    // What cycle-start clears at a start: the earlier attempt's.
                    for table in [
                        "cycle_settings",
                        "cycle_notes",
                        "cycle_locks",
                        "cycle_results",
                        "cycle_progress",
                    ] {
                        c.execute(&format!("DELETE FROM {table} WHERE issue = ?1"), [issue])
                            .q()?;
                    }
                }
                n
            }
            Op::CycleEnd(issue, started, at, reason) => c
                .execute(
                    "UPDATE cycles SET ended_at = ?3, end_reason = ?4
                     WHERE issue = ?1 AND started_at = ?2 AND ended_at IS NULL",
                    params![issue, started, at, reason],
                )
                .q()?,
            Op::Approval(issue, a) => {
                stub(c, *issue)?;
                let (pr, head) = match &a.refresh {
                    Some((pr, head)) => (Some(*pr), Some(head.as_str())),
                    None => (None, None),
                };
                c.execute(
                    "INSERT OR IGNORE INTO approvals (issue, at, by_account, how, label, body_sha256,
                       title_sha256, comment_id, refresh_pr, refresh_head, used_at, used_by, input_json)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?9, ?10, ?11, ?12)",
                    params![
                        issue, a.at, a.by, a.how, label_of(&a.how), a.body_sha, a.title_sha, pr,
                        head, a.used_at, a.used_by, a.input
                    ],
                )
                .q()?
            }
            Op::Park {
                issue,
                attempt,
                at,
                reason,
                step,
                pr,
                retries,
                told_at,
            } => {
                stub(c, *issue)?;
                c.execute(
                    "INSERT OR IGNORE INTO parks (issue, attempt, at, reason, step, pr, retries, told_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![issue, attempt, at, reason, step, pr, retries, told_at],
                )
                .q()?
            }
            Op::Question {
                id,
                issue,
                attempt,
                kind,
                posted_at,
            } => {
                stub(c, *issue)?;
                c.execute(
                    "INSERT OR IGNORE INTO questions (id, issue, attempt, kind, posted_at, body_sha256)
                     VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
                    params![id, issue, attempt, kind, posted_at],
                )
                .q()?
            }
            Op::Setting(issue, k, v) => c
                .execute(
                    "INSERT OR IGNORE INTO cycle_settings (issue, key, value) VALUES (?1, ?2, ?3)",
                    params![issue, k, v],
                )
                .q()?,
            Op::CycleHistory(issue, attempt, l) => c
                .execute(
                    "INSERT INTO cycle_history (issue, attempt, at, status, step, by_account, how, pr,
                       started_at, rc, cap, park_reason, failure_kind, failure_cause, provider, parked)
                     SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16
                     WHERE NOT EXISTS (SELECT 1 FROM cycle_history
                       WHERE issue = ?1 AND at = ?3 AND status = ?4 AND started_at IS ?9)
                     AND (SELECT count(*) FROM cycle_history WHERE issue = ?1 AND at >= ?3) < 50",
                    params![
                        issue, attempt, l.at, l.status, l.step, l.by, l.how, l.pr, l.started,
                        l.rc, l.cap, l.park_reason, l.failure_kind, l.failure_cause, l.provider,
                        l.parked
                    ],
                )
                .q()?,
            Op::Planning(issue, r) => {
                stub(c, *issue)?;
                let p = &r.p;
                c.execute(
                    "INSERT OR IGNORE INTO planning (issue, session, turns, options_json, pending,
                       pending_outcome, failed, running, started_at, comment_id, lost, by_account,
                       label, brief, turn_rc, opened_at, ended_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, NULL, ?15, NULL)",
                    params![
                        issue,
                        p.session,
                        p.turns,
                        j(p.options.clone()),
                        p.pending,
                        r.pending_outcome,
                        p.failed,
                        p.running,
                        p.started_at,
                        p.comment,
                        p.lost,
                        p.by,
                        p.label,
                        r.brief,
                        r.opened_at
                    ],
                )
                .q()?
            }
            Op::Answer(issue, turn, by, fixer, text) => {
                // The column of an answer is `millwright` from schema 3, the name
                // the step of db::STEP_3 gave it. What import writes keeps the old
                // name until the verbs speak the new one (#686); see that step for
                // the rename, its rows and its place last in the table.
                c.execute(
                    &format!(
                        "INSERT OR IGNORE INTO planning_answers (issue, turn, by_account, {}, text)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                        ANSWER_COLUMN
                    ),
                    params![issue, turn, by, fixer, text],
                )
                .q()?
            }
            Op::Filed(issue, url, filed, at) => c
                .execute(
                    "INSERT OR IGNORE INTO planning_filed (issue, url, filed_issue, at)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![issue, url, filed, at],
                )
                .q()?,
            Op::Note(issue, kind, id, at) => c
                .execute(
                    "INSERT OR IGNORE INTO planning_notes (issue, kind, comment_id, at)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![issue, kind, id, at],
                )
                .q()?,
            Op::PlanHistory(issue, l) => c
                .execute(
                    "INSERT INTO plan_history (issue, turn, at, outcome, session, started_at, rc,
                       by_account, label, filed_json)
                     SELECT ?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7, ?8, ?9
                     WHERE NOT EXISTS (SELECT 1 FROM plan_history
                       WHERE issue = ?1 AND turn = ?2 AND at = ?3 AND outcome = ?4)
                     AND (SELECT count(*) FROM plan_history WHERE issue = ?1 AND at >= ?3) < 50",
                    params![
                        issue,
                        l.turn,
                        l.at,
                        l.outcome,
                        l.started,
                        l.rc,
                        l.by,
                        l.label,
                        (!l.filed.is_empty()).then(|| j(l.filed.clone()))
                    ],
                )
                .q()?,
            Op::TrimCycleHistory(issue) => {
                c.execute(
                    "DELETE FROM cycle_history WHERE issue = ?1 AND id NOT IN
                       (SELECT id FROM cycle_history WHERE issue = ?1 ORDER BY at DESC, id DESC LIMIT 50)",
                    [issue],
                )
                .q()?;
                0
            }
            Op::TrimPlanHistory(issue) => {
                c.execute(
                    "DELETE FROM plan_history WHERE issue = ?1 AND id NOT IN
                       (SELECT id FROM plan_history WHERE issue = ?1 ORDER BY at DESC, id DESC LIMIT 50)",
                    [issue],
                )
                .q()?;
                0
            }
        })
    }

    /// Whether the store holds what the op wrote, read after its commit.
    fn holds(&self, c: &Connection) -> Result<bool, Fail> {
        let yes = |sql: &str, p: &[&dyn rusqlite::ToSql]| -> Result<bool, Fail> {
            c.query_row(&format!("SELECT count(*) > 0 FROM {sql}"), p, |r| r.get(0))
                .q()
        };
        match self {
            Op::CycleInsert(issue, r) | Op::CycleUpdate(issue, _, r) => yes(
                "cycles WHERE issue = ?1 AND started_at >= ?2",
                &[issue, &r.started_at],
            ),
            Op::CycleEnd(issue, started, ..) => yes(
                "cycles WHERE issue = ?1 AND (started_at > ?2 OR ended_at IS NOT NULL)",
                &[issue, started],
            ),
            Op::Approval(issue, a) => yes(
                "approvals WHERE issue = ?1 AND at = ?2 AND by_account = ?3 AND how = ?4
                   AND body_sha256 = ?5",
                &[issue, &a.at, &a.by, &a.how, &a.body_sha],
            ),
            Op::Park { issue, attempt, .. } => {
                yes("parks WHERE issue = ?1 AND attempt = ?2", &[issue, attempt])
            }
            Op::Question {
                id, issue, kind, ..
            } => yes(
                "questions WHERE id = ?1 AND issue = ?2 AND kind = ?3",
                &[id, issue, kind],
            ),
            Op::Setting(issue, k, v) => yes(
                "cycle_settings WHERE issue = ?1 AND key = ?2 AND value = ?3",
                &[issue, k, v],
            ),
            // A line is kept, or 50 rows as new or newer are.
            Op::CycleHistory(issue, _, l) => yes(
                "cycle_history WHERE issue = ?1 AND ((at = ?2 AND status = ?3 AND started_at IS ?4)
                   OR (SELECT count(*) FROM cycle_history WHERE issue = ?1 AND at >= ?2) >= 50)",
                &[issue, &l.at, &l.status, &l.started],
            ),
            Op::Planning(issue, r) => yes(
                "planning WHERE issue = ?1 AND turns = ?2 AND by_account IS ?3 AND label IS ?4
                   AND started_at IS ?5",
                &[issue, &r.p.turns, &r.p.by, &r.p.label, &r.p.started_at],
            ),
            Op::Answer(issue, turn, by, ..) => yes(
                // The issue and the turn are the key of the row, so the column
                // of the answer is not named: from schema 3 it answers to
                // `millwright` (db::STEP_3). Part 2 (#686) renames what import
                // writes.
                "planning_answers WHERE issue = ?1 AND turn = ?2 AND by_account = ?3",
                &[issue, turn, by],
            ),
            Op::Filed(issue, url, ..) => yes(
                "planning_filed WHERE issue = ?1 AND url = ?2",
                &[issue, url],
            ),
            Op::Note(issue, kind, id, _) => yes(
                "planning_notes WHERE issue = ?1 AND kind = ?2 AND comment_id = ?3",
                &[issue, kind, id],
            ),
            Op::PlanHistory(issue, l) => yes(
                "plan_history WHERE issue = ?1 AND ((turn = ?2 AND at = ?3 AND outcome = ?4)
                   OR (SELECT count(*) FROM plan_history WHERE issue = ?1 AND at >= ?3) >= 50)",
                &[issue, &l.turn, &l.at, &l.outcome],
            ),
            Op::TrimCycleHistory(..) | Op::TrimPlanHistory(..) => Ok(true),
        }
    }
}

/// A file, the step its rows belong to, and the writes that must read back
/// before it moves. No write: it moves with no row.
struct Item {
    found: Found,
    got: Got,
    step: Step,
    checks: Vec<Op>,
}

/// One issue's cycle group or planning group. Its writes commit in one
/// transaction, in the order of the steps; its files move only once every
/// write of the group reads back, and then the files that say when it started
/// (`.taken`, `plan-<n>.json`) move last. So a run that stops anywhere leaves
/// the next run either the same files or files that name the row on record.
#[derive(Default)]
struct Group {
    ops: Vec<(Step, Op)>,
    items: Vec<Item>,
}

impl Group {
    fn op(&mut self, step: Step, op: Op) {
        self.ops.push((step, op));
    }
}

// --- the store as it is ------------------------------------------------------------

/// What the store holds of one issue's cycle, read before the import plans it.
#[derive(Default)]
struct OnRecord {
    /// attempt, started_at, ended_at
    row: Option<(i64, i64, Option<i64>)>,
    /// Whether that row's attempt has a park on record.
    parked: bool,
    max_history: i64,
    keys: BTreeSet<(i64, String, Option<i64>)>,
}

fn on_record(c: &Connection, issue: i64) -> Result<OnRecord, Fail> {
    let row = c
        .query_row(
            "SELECT attempt, started_at, ended_at FROM cycles WHERE issue = ?1",
            [issue],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .q()?;
    let parked = match row {
        Some((attempt, ..)) => c
            .query_row(
                "SELECT count(*) > 0 FROM parks WHERE issue = ?1 AND attempt = ?2",
                params![issue, attempt],
                |r| r.get(0),
            )
            .q()?,
        None => false,
    };
    let max_history: i64 = c
        .query_row(
            "SELECT coalesce(max(attempt), 0) FROM cycle_history WHERE issue = ?1",
            [issue],
            |r| r.get(0),
        )
        .q()?;
    let mut keys = BTreeSet::new();
    let mut st = c
        .prepare("SELECT at, status, started_at FROM cycle_history WHERE issue = ?1")
        .q()?;
    let rows = st
        .query_map([issue], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .q()?;
    for r in rows {
        keys.insert(r.q()?);
    }
    Ok(OnRecord {
        row,
        parked,
        max_history,
        keys,
    })
}

// --- planning an issue -------------------------------------------------------------

type Files = Vec<(Found, Got)>;

fn take(files: &mut Files, what: What) -> Option<(Found, Got)> {
    let i = files.iter().position(|(f, _)| f.what == what)?;
    Some(files.remove(i))
}

/// The cycle group of one issue: its rows and the step each file waits for.
fn plan_cycle(issue: i64, mut files: Files, rec: &OnRecord, w: &mut Group) -> Result<(), String> {
    let rel = |f: &Option<(Found, Got)>| f.as_ref().map(|(x, _)| x.rel.clone());
    let taken = take(&mut files, What::Taken);
    let by = take(&mut files, What::By);
    let requeued = take(&mut files, What::Requeued);
    let rukas = take(&mut files, What::Rukas);
    let image = take(&mut files, What::Image);
    let branch = take(&mut files, What::Branch);
    let settings = take(&mut files, What::Settings);
    let approved = take(&mut files, What::Approved);
    let park = take(&mut files, What::Park);
    let history = take(&mut files, What::History);
    let park_id = take(&mut files, What::ParkId);

    let wrap = |f: &Option<(Found, Got)>, e: String| format!("{}: {e}", rel(f).unwrap_or_default());
    let by_v = match &by {
        Some((_, g)) => match first_line(g).map_err(|e| wrap(&by, e))? {
            None => None,
            Some(s) => {
                Some(verbs::account(&s).ok_or_else(|| wrap(&by, "is not an account".into()))?)
            }
        },
        None => None,
    };
    let rukas_v = match &rukas {
        Some((_, g)) => match first_line(g).map_err(|e| wrap(&rukas, e))? {
            None => None,
            Some(s) => {
                Some(verbs::commit(&s).ok_or_else(|| wrap(&rukas, "is not a commit".into()))?)
            }
        },
        None => None,
    };
    let image_v = match &image {
        Some((_, g)) => match first_line(g).map_err(|e| wrap(&image, e))? {
            None => None,
            Some(s) => {
                let mut words = s.split_whitespace();
                let id = words
                    .next()
                    .and_then(verbs::image)
                    .ok_or_else(|| wrap(&image, "does not name an image".into()))?;
                Some((id, words.next().and_then(verbs::commit)))
            }
        },
        None => None,
    };
    let branch_v = match &branch {
        Some((_, g)) => first_line(g)
            .map_err(|e| wrap(&branch, e))?
            .filter(|b| cycle::branch(issue, b)),
        None => None,
    };
    let settings_v = match &settings {
        Some((_, g)) => match cycle::settings_of(&g.bytes) {
            SettingsIn::Good(rows) => Some(rows),
            SettingsIn::Secret => None,
            SettingsIn::Refused => {
                return Err(wrap(
                    &settings,
                    "is not the shape of a settings file".into(),
                ))
            }
        },
        None => None,
    };
    let approval = match &approved {
        Some((_, g)) => Some(approval_in(g).map_err(|e| wrap(&approved, e))?),
        None => None,
    };
    let park_v = match &park {
        Some((_, g)) => Some(park_in(g).map_err(|e| wrap(&park, e))?),
        None => None,
    };
    let lines = match &history {
        Some((_, g)) => cycle_lines(g).map_err(|e| wrap(&history, e))?,
        None => Vec::new(),
    };
    let id_v = match &park_id {
        Some((_, g)) => match first_line(g).map_err(|e| wrap(&park_id, e))? {
            None => None,
            Some(s) => Some(
                verbs::comment_id(&s)
                    .ok_or_else(|| wrap(&park_id, "is not a comment id".into()))?,
            ),
        },
        None => None,
    };

    // The attempts of the lines the store does not hold yet, oldest first.
    let new: Vec<bool> = lines.iter().map(|l| !rec.keys.contains(&l.key())).collect();
    let base = rec.row.map_or(0, |r| r.0).max(rec.max_history);
    let mut attempts = vec![0; lines.len()];
    let mut next = base;
    for (i, fresh) in new.iter().enumerate() {
        if *fresh {
            next += 1;
            attempts[i] = next;
        }
    }
    let last = lines.len().checked_sub(1);
    let from_line = if taken.is_some() { None } else { last };
    let start = taken
        .as_ref()
        .map(|(_, g)| g.mtime)
        .or_else(|| from_line.map(|i| lines[i].started.unwrap_or(lines[i].at)));

    // The row on record the files are of: the same start, or the last line is
    // the one that ended it or one of its own. No start file left at all is
    // that row too, so a run after one that stopped partway plans the files it
    // left against the row it wrote.
    let matched = rec.row.filter(|&(_, started, ended)| match start {
        None => true,
        Some(s) => {
            s == started
                || from_line.is_some_and(|i| {
                    let l = &lines[i];
                    Some(l.at) == ended
                        || l.started
                            .is_some_and(|x| x >= started && ended.is_none_or(|e| x <= e))
                })
        }
    });

    // The start the other files belong to: its attempt and when it began.
    // None when the files tell of no start, or of one older than the store's.
    let mut current: Option<(i64, i64, Option<usize>)> = None;
    let mut cycle_op: Option<Op> = None;
    if let Some((attempt, started, ended)) = matched {
        let ender = last.filter(|&i| {
            let l = &lines[i];
            match ended {
                Some(e) => l.at == e,
                None => new[i] && l.at > started,
            }
        });
        if let Some(e) = ender {
            if new[e] {
                // The lines up to the ender are the attempts up to the row's.
                let fresh: Vec<usize> = (0..=e).filter(|i| new[*i]).collect();
                let n = fresh.len() as i64;
                for (k, i) in fresh.into_iter().enumerate() {
                    attempts[i] = (attempt - (n - 1 - k as i64)).max(1);
                }
            }
            if ended.is_none() {
                cycle_op = Some(Op::CycleEnd(
                    issue,
                    started,
                    lines[e].at,
                    lines[e].reason().to_string(),
                ));
            }
        }
        current = Some((attempt, started, ender));
    } else if let Some(s) = start {
        if rec.row.is_none_or(|r| s > r.1) {
            let ender = last.filter(|i| new[*i] && lines[*i].at >= s);
            let attempt = match ender {
                Some(i) => attempts[i],
                None => next + 1,
            };
            let (how, started_by) = match &approval {
                Some(a) => (a.how.to_string(), by_v.clone().or(Some(a.by.clone()))),
                None => ("hand".to_string(), None),
            };
            let (ended_at, end_reason) = match ender {
                Some(i) => (lines[i].at, lines[i].reason().to_string()),
                None => (s, "lost".to_string()),
            };
            let row = CycleRow {
                attempt,
                started_at: s,
                started_by,
                how,
                requeued: requeued.is_some(),
                rukas: rukas_v
                    .clone()
                    .or_else(|| image_v.as_ref().and_then(|i| i.1.clone())),
                image: image_v.as_ref().map(|i| i.0.clone()),
                branch: branch_v.clone(),
                end_reason,
                ended_at,
            };
            cycle_op = Some(match rec.row {
                None => Op::CycleInsert(issue, row),
                Some(r) => Op::CycleUpdate(issue, r.1, row),
            });
            current = Some((attempt, s, ender));
        } else if let Some((attempt, started, None)) = rec.row {
            // Files of an older start: no row of their own, but a newer line
            // ends the open row on record.
            if let Some(i) = last.filter(|i| new[*i] && lines[*i].at > started) {
                attempts[i] = attempt;
                cycle_op = Some(Op::CycleEnd(
                    issue,
                    started,
                    lines[i].at,
                    lines[i].reason().to_string(),
                ));
            }
        }
    }

    let start_files = [taken, by, requeued, rukas, image, branch];
    let cycle_checks: Vec<Op> = cycle_op.iter().cloned().collect();
    if let Some(op) = cycle_op {
        w.op(Step::Cycles, op);
    }
    for (f, g) in start_files.into_iter().flatten() {
        // Facts of a start the store already holds, or of none: no row of
        // their own, so nothing to read back but the start's.
        w.items.push(Item {
            found: f,
            got: g,
            step: Step::Cycles,
            checks: cycle_checks.clone(),
        });
    }

    let push = |w: &mut Group, file: Option<(Found, Got)>, step: Step, ops: Vec<Op>| {
        if let Some((f, g)) = file {
            for op in &ops {
                w.op(step, op.clone());
            }
            w.items.push(Item {
                found: f,
                got: g,
                step,
                checks: ops,
            });
        }
    };

    let mut approval_ops = Vec::new();
    let mut park_ops = Vec::new();
    let mut question_ops = Vec::new();
    let mut setting_ops = Vec::new();
    if let Some((attempt, s, ender)) = current {
        if let (Some(a), Some((_, g))) = (&approval, &approved) {
            approval_ops = approval_rows(a, attempt, s, g.mtime)
                .into_iter()
                .map(|r| Op::Approval(issue, r))
                .collect();
        }
        let on_record = rec.parked && rec.row.is_some_and(|r| r.0 == attempt);
        // Only an id of a park on record is a question (F5).
        if park_v.is_some() || on_record {
            if let (Some(id), Some((_, g))) = (id_v, &park_id) {
                question_ops.push(Op::Question {
                    id,
                    issue,
                    attempt,
                    kind: "park",
                    posted_at: g.mtime,
                });
            }
        }
        if let Some(p) = &park_v {
            let line = ender.map(|i| &lines[i]);
            park_ops.push(Op::Park {
                issue,
                attempt,
                at: p.at,
                reason: p
                    .reason
                    .clone()
                    .or_else(|| line.map(|l| l.status.clone()))
                    .unwrap_or_else(|| "unknown".to_string()),
                step: line.and_then(|l| l.step.clone()),
                pr: line.and_then(|l| l.pr),
                retries: if p.retry { 0 } else { 1 },
                told_at: p.told.then_some(p.at),
            });
        }
        if let Some(rows) = &settings_v {
            setting_ops = rows
                .iter()
                .map(|(k, v)| Op::Setting(issue, k.clone(), v.clone()))
                .collect();
        }
    }
    push(w, approved, Step::Approvals, approval_ops);
    push(w, park, Step::Parks, park_ops);
    push(w, park_id, Step::Questions, question_ops);
    push(w, settings, Step::Settings, setting_ops);
    let history_ops: Vec<Op> = lines
        .iter()
        .enumerate()
        .filter(|(i, _)| new[*i])
        .map(|(i, l)| Op::CycleHistory(issue, attempts[i], l.clone()))
        .collect();
    if !history_ops.is_empty() {
        w.op(Step::CycleHistory, Op::TrimCycleHistory(issue));
    }
    push(w, history, Step::CycleHistory, history_ops);
    // .quiet, the transient park body: moved, no rows.
    for (f, g) in files {
        w.items.push(Item {
            found: f,
            got: g,
            step: Step::Moved,
            checks: Vec::new(),
        });
    }
    Ok(())
}

/// The approvals a record of a start becomes: the start itself, used by
/// `attempt`; for a fixer or refresh start also the base it rests on and one
/// used row for each start the record counts besides itself, at times strictly
/// between, so approval-current never counts fewer than the file did.
fn approval_rows(a: &ApprovalIn, attempt: i64, used_at: i64, mtime: i64) -> Vec<ApprovalRow> {
    let mut rows = Vec::new();
    let mut at = a.at;
    if let Some((base, starts, refreshes)) = &a.rests {
        let (fixers, refreshes) = if a.how == "fixer" {
            (starts - 1, 0)
        } else {
            (*starts, refreshes - 1)
        };
        let k = fixers + refreshes;
        let first = attempt - k - 1;
        rows.push(ApprovalRow {
            at: base.at,
            by: base.by.clone(),
            how: base.how.clone(),
            body_sha: base.body_sha.clone(),
            title_sha: None,
            refresh: None,
            used_at: base.at,
            used_by: first,
            input: None,
        });
        for i in 1..=k {
            rows.push(ApprovalRow {
                at: base.at + i,
                by: a.by.clone(),
                how: if i <= fixers { "fixer" } else { "refresh" }.to_string(),
                body_sha: base.body_sha.clone(),
                title_sha: None,
                refresh: (i > fixers).then(|| a.refresh.clone()).flatten(),
                used_at: base.at + i,
                used_by: first + i,
                input: None,
            });
        }
        if at <= base.at + k {
            at = mtime.max(base.at + k + 1);
        }
    }
    rows.push(ApprovalRow {
        at,
        by: a.by.clone(),
        how: a.how.to_string(),
        body_sha: a.body_sha.clone(),
        title_sha: a.title_sha.clone(),
        refresh: a.refresh.clone(),
        used_at,
        used_by: attempt,
        input: a.input.clone(),
    });
    rows
}

/// The planning group of one issue.
fn plan_planning(home: &Path, issue: i64, mut files: Files, w: &mut Group) -> Result<(), String> {
    let record = take(&mut files, What::Plan);
    let brief = take(&mut files, What::PlanBrief);
    let answer = take(&mut files, What::PlanAnswer);
    let wrap = |f: &Option<(Found, Got)>, e: String| {
        format!(
            "{}: {e}",
            f.as_ref().map(|(x, _)| x.rel.clone()).unwrap_or_default()
        )
    };
    let p = match &record {
        Some((_, g)) => Some(plan_in(g).map_err(|e| wrap(&record, e))?),
        None => None,
    };
    let brief_v = match &brief {
        Some((_, g)) => Some(text(g).map_err(|e| wrap(&brief, e))?.to_string()),
        None => None,
    };
    let answer_v = match &answer {
        Some((_, g)) => Some(text(g).map_err(|e| wrap(&answer, e))?.to_string()),
        None => None,
    };
    let mut lines = Vec::new();
    let mut history = Vec::new();
    while let Some((f, g)) = take(&mut files, What::PlanHistory) {
        let got = plan_lines(&g).map_err(|e| format!("{}: {e}", f.rel))?;
        lines.push(got);
        history.push((f, g));
    }

    let mut planning_ops = Vec::new();
    if let Some(p) = p {
        let out = format!("state/plan-{issue}.json.out");
        let pending_outcome = fs::symlink_metadata(home.join(&out))
            .is_ok_and(|m| m.file_type().is_file())
            .then_some(out);
        let opened_at = [
            brief.as_ref().map(|(_, g)| g.mtime),
            p.started_at,
            Some(p.mtime),
        ]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(p.mtime);
        planning_ops.push(Op::Planning(
            issue,
            Box::new(PlanningRow {
                p: p.clone(),
                brief: brief_v,
                pending_outcome,
                opened_at,
            }),
        ));
        let last_turn = p.answers.iter().map(|a| a.0).max();
        for (turn, by, fixer) in &p.answers {
            let text = answer_v.clone().filter(|_| Some(*turn) == last_turn);
            planning_ops.push(Op::Answer(issue, *turn, by.clone(), *fixer, text));
        }
        for url in &p.filed {
            if let Some(n) = plan::filed_issue(url) {
                planning_ops.push(Op::Filed(issue, url.clone(), n, p.mtime));
            }
        }
        for (kind, id) in &p.notes {
            planning_ops.push(Op::Note(issue, kind, *id, p.mtime));
        }
        if let Some(id) = p.comment {
            planning_ops.push(Op::Question {
                id,
                issue,
                attempt: p.turns,
                kind: "plan",
                posted_at: p.mtime,
            });
        }
    }
    for op in &planning_ops {
        w.op(Step::Planning, op.clone());
    }
    let planned = !planning_ops.is_empty();
    for file in [record, brief, answer].into_iter().flatten() {
        // Without a record, a brief or an answer belongs to no session: it
        // moves with no row.
        w.items.push(Item {
            found: file.0,
            got: file.1,
            step: if planned { Step::Planning } else { Step::Moved },
            checks: planning_ops.clone(),
        });
    }
    if lines.iter().any(|ls| !ls.is_empty()) {
        w.op(Step::PlanHistory, Op::TrimPlanHistory(issue));
    }
    for ((f, g), ls) in history.into_iter().zip(lines) {
        let ops: Vec<Op> = ls.into_iter().map(|l| Op::PlanHistory(issue, l)).collect();
        for op in &ops {
            w.op(Step::PlanHistory, op.clone());
        }
        w.items.push(Item {
            found: f,
            got: g,
            step: Step::PlanHistory,
            checks: ops,
        });
    }
    for (f, g) in files {
        w.items.push(Item {
            found: f,
            got: g,
            step: Step::Moved,
            checks: Vec::new(),
        });
    }
    Ok(())
}

// --- the verb ----------------------------------------------------------------------

/// The first lock that is held or cannot be probed: the issue and what it is.
fn live(home: &Path) -> Result<Option<(i64, &'static str)>, Fail> {
    let unknown = |e: String| (1, format!("live: {e}; nothing was imported"));
    for n in lock::listed(home).map_err(unknown)? {
        if lock::probe_file(&lock::path(home, n)).is_some_and(|l| l.rc != 0) {
            return Ok(Some((n, "a cycle")));
        }
    }
    let state = home.join("state");
    for name in names(&state)? {
        let n = name
            .strip_prefix("plan-")
            .and_then(|r| r.strip_suffix(".json.lock"))
            .and_then(verbs::number);
        if let Some(n) = n {
            if lock::probe_file(&state.join(&name)).is_some_and(|l| l.rc != 0) {
                return Ok(Some((n, "a planning turn")));
            }
        }
    }
    Ok(None)
}

/// Moves a file to the same path under `state/imported-<day>/`, never over a
/// file that is there: a name taken gets `.1`, `.2` and so on. The file must
/// still be the one that was read.
fn move_file(home: &Path, it: &Item, day: &str) -> Result<(), String> {
    let now = fs::symlink_metadata(&it.found.path).map_err(|e| format!("cannot be moved: {e}"))?;
    if !now.file_type().is_file() || now.dev() != it.got.dev || now.ino() != it.got.ino {
        return Err("was replaced after it was read".to_string());
    }
    let dest = home
        .join("state")
        .join(format!("imported-{day}"))
        .join(&it.found.rel);
    let dir = dest.parent().expect("a path under state/ has a directory");
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(|e| format!("cannot be moved: {}: {e}", dir.display()))?;
    for n in 0..1000 {
        let to = if n == 0 {
            dest.clone()
        } else {
            PathBuf::from(format!("{}.{n}", dest.display()))
        };
        match fs::symlink_metadata(&to) {
            Err(e) if e.kind() == ErrorKind::NotFound => {
                return fs::rename(&it.found.path, &to)
                    .map_err(|e| format!("cannot be moved: {e}"));
            }
            Err(e) => return Err(format!("cannot be moved: {e}")),
            Ok(_) => {}
        }
    }
    Err("cannot be moved: every name under imported/ is taken".to_string())
}

/// The name of the column of an answer, from schema 3: the step of db::STEP_3
/// renamed it from `fixer`. What the store's own verbs write is written with this
/// name; what import writes keeps the old one until the verbs do too (#686).
pub const ANSWER_COLUMN: &str = "millwright";

pub fn run(home: &Path, dry_run: bool, now: i64) -> Result<String, Fail> {
    match fs::symlink_metadata(home.join(MARKER)) {
        Ok(m) if m.file_type().is_file() => {}
        _ => {
            return Err((
                78,
                format!(
                    "unmarked {}: there is no {MARKER}, so the home is not known to be tehas's; nothing was imported",
                    home.display()
                ),
            ))
        }
    }
    let found = scan(home)?;
    let mut counts: BTreeMap<&str, usize> = TABLES.iter().map(|t| (*t, 0)).collect();
    if found.is_empty() {
        return Ok(answer(&counts, 0, dry_run));
    }
    if let Some((n, what)) = live(home)? {
        return Err((
            1,
            format!("live #{n}: {what} runs or its lock cannot be probed; nothing was imported"),
        ));
    }

    // Each file read once; a group with any file that will not read is held whole.
    let mut cycles: BTreeMap<i64, Files> = BTreeMap::new();
    let mut plans: BTreeMap<i64, Files> = BTreeMap::new();
    let mut bad: BTreeMap<(bool, i64), (String, String)> = BTreeMap::new();
    let mut rels: BTreeMap<(bool, i64), Vec<String>> = BTreeMap::new();
    for f in found {
        let key = (f.plan(), f.issue);
        rels.entry(key).or_default().push(f.rel.clone());
        match read(&f.path) {
            Ok(g) => {
                let group = if f.plan() { &mut plans } else { &mut cycles };
                group.entry(f.issue).or_default().push((f, g));
            }
            Err(why) => {
                bad.entry(key).or_insert((f.rel.clone(), why));
            }
        }
    }

    let records: BTreeMap<i64, OnRecord> = if dry_run {
        BTreeMap::new()
    } else {
        let issues: Vec<i64> = cycles.keys().copied().collect();
        verbs::look(home, |c| {
            issues
                .iter()
                .map(|n| on_record(c, *n).map(|r| (*n, r)))
                .collect()
        })?
    };
    let none = OnRecord::default();
    let mut groups: BTreeMap<(bool, i64), Group> = BTreeMap::new();
    let mut held: Vec<(String, String)> = Vec::new();
    for (issue, files) in cycles {
        if bad.contains_key(&(false, issue)) {
            continue;
        }
        let rec = records.get(&issue).unwrap_or(&none);
        let mut g = Group::default();
        match plan_cycle(issue, files, rec, &mut g) {
            Ok(()) => {
                groups.insert((false, issue), g);
            }
            Err(why) => {
                let rel = why.split(':').next().unwrap_or("").to_string();
                bad.insert((false, issue), (rel, why));
            }
        }
    }
    for (issue, files) in plans {
        if bad.contains_key(&(true, issue)) {
            continue;
        }
        let mut g = Group::default();
        match plan_planning(home, issue, files, &mut g) {
            Ok(()) => {
                groups.insert((true, issue), g);
            }
            Err(why) => {
                let rel = why.split(':').next().unwrap_or("").to_string();
                bad.insert((true, issue), (rel, why));
            }
        }
    }
    for (key, (culprit, why)) in &bad {
        for rel in &rels[key] {
            let reason = if rel == culprit {
                why.strip_prefix(&format!("{rel}: "))
                    .unwrap_or(why)
                    .to_string()
            } else {
                format!("held with {culprit}")
            };
            held.push((rel.clone(), reason));
        }
    }

    // The cycle groups first, then the planning groups: the steps' order.
    let day = day(now);
    let mut moved = 0;
    for (_, mut g) in groups {
        g.ops.sort_by_key(|(step, op)| op.order(*step));
        if dry_run {
            for (_, op) in g.ops.iter().filter(|(_, op)| op.counted()) {
                *counts.entry(op.table()).or_default() += 1;
            }
            moved += g.items.len();
            continue;
        }
        if !g.ops.is_empty() {
            let written = verbs::write(home, |c| {
                let mut written: Vec<(&str, usize)> = Vec::new();
                for (_, op) in &g.ops {
                    written.push((op.table(), op.apply(c)?));
                }
                Ok(written)
            })?;
            for (table, n) in written {
                *counts.entry(table).or_default() += n;
            }
        }
        if g.items.is_empty() {
            continue;
        }
        let back: Vec<bool> = verbs::look(home, |c| {
            g.items
                .iter()
                .map(|it| {
                    for op in &it.checks {
                        if !op.holds(c)? {
                            return Ok(false);
                        }
                    }
                    Ok(true)
                })
                .collect()
        })?;
        if let Some(first) = back.iter().position(|b| !b) {
            let culprit = g.items[first].found.rel.clone();
            for (it, ok) in g.items.iter().zip(back) {
                let why = if ok {
                    format!("held with {culprit}")
                } else {
                    "its rows did not read back from the store".to_string()
                };
                held.push((it.found.rel.clone(), why));
            }
            continue;
        }
        g.items.sort_by_key(|it| {
            (
                matches!(it.found.what, What::Taken | What::Plan),
                std::cmp::Reverse(it.step),
            )
        });
        for it in &g.items {
            match move_file(home, it, &day) {
                Ok(()) => moved += 1,
                Err(why) => held.push((it.found.rel.clone(), why)),
            }
        }
    }
    if !dry_run && moved > 0 {
        verbs::write(home, |c| {
            c.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('imported_at', ?1)",
                [now.to_string()],
            )
            .q()
            .map(|_| ())
        })?;
    }
    if !held.is_empty() {
        held.sort();
        let mut msg = format!(
            "held {} files stay where they are, {} {}",
            held.len(),
            moved,
            if dry_run { "would move" } else { "moved" }
        );
        for (rel, why) in &held {
            msg.push_str(&format!("\n  state/{rel}: {why}"));
        }
        return Err((1, msg));
    }
    Ok(answer(&counts, moved, dry_run))
}

fn answer(counts: &BTreeMap<&str, usize>, moved: usize, dry_run: bool) -> String {
    let imported: Vec<(&str, String)> = TABLES
        .iter()
        .map(|t| (*t, j(counts.get(t).copied().unwrap_or(0))))
        .collect();
    line(
        "import",
        &[
            ("imported", obj(&imported)),
            ("moved", j(moved)),
            ("held", list(&[])),
            ("dry_run", j(dry_run)),
        ],
    )
}
