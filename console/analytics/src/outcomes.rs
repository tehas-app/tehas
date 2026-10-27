//! `analytics outcomes`: the counts behind the MCP tool `factory_outcomes`,
//! as far back as the database holds. Line 1 is the header, then one entry
//! per line.
//!
//! Counts only. No issue number, title, failure cause or log content: those
//! stay in the 30-day tools, which read the sessions snapshot. A group is
//! named by a word of the database's own vocabularies, so no free text
//! reaches the file.
//!
//! `tehasbot status` asks for the file with `outcomes --out <file>`, which
//! writes it here rather than printing it: what is checked before the file is
//! put in place is what this module wrote, so the host needs no second reader
//! of it. Without `--out` the same lines go to standard output, for a person.
//! The directory the host names is `~/tehas/mcp`, the one the MCP container
//! is given read only.

use crate::run::iso_ms;
use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use tehas_core::time::{civil_from_days, days_from_civil, iso};

/// The file's `api`.
pub const API: i64 = 1;
/// The shape of the entries.
pub const OUTCOMES: i64 = 1;
/// Where `tehasbot status` asks for the file, under `~/tehas/mcp`. The name
/// the reader opens. It is JSON Lines; the name says what the file is, not
/// how it is cut into lines, as `sessions.jsonl` is named for its sessions.
pub const FILE: &str = "outcomes.jsonl";
/// How far back the counts go when the caller names no bound. The whole
/// retention is what makes this tool more than the 30-day ones.
pub const MAX_DAYS: i64 = 365;
/// The most one word of the database may be to name a group or a count. A
/// longer value counts as unknown.
const WORD_MAX: usize = 64;
const DAY: i64 = 86_400;
/// Outcomes with no word in `runs.outcome`.
const UNKNOWN: &str = "unknown";
/// How the entries are grouped.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum By {
    #[default]
    Version,
    Week,
}

/// What the caller asked for: the rows served and how they are grouped.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Filter {
    pub tehas: Option<String>,
    pub rukas: Option<String>,
    /// The first moment served, and the last: a run is counted on its end, a
    /// run with no end on its start.
    pub since: Option<i64>,
    pub until: Option<i64>,
    pub by: By,
}

/// A group's name or a count's name: a word of the database's own
/// vocabularies, at most `WORD_MAX` of letters, digits and `. _ - :`, and at
/// least one letter or digit. Anything else is no word.
pub fn word(w: &str) -> Option<String> {
    (!w.is_empty()
        && w.len() <= WORD_MAX
        && w.contains(char::is_alphanumeric)
        && w.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':')))
    .then(|| w.to_string())
}

/// The same for a column read from the database, where no value at all is
/// also no word.
fn column(v: &Option<String>) -> Option<String> {
    word(v.as_deref()?)
}

/// The counts of one group, in the order the entry gives them.
#[derive(Default)]
struct Row {
    runs: u64,
    outcomes: BTreeMap<String, u64>,
    kinds: BTreeMap<String, u64>,
    parks: BTreeMap<String, u64>,
}

fn bump(counts: &mut BTreeMap<String, u64>, key: &str) {
    *counts.entry(key.to_string()).or_default() += 1;
}

/// Days since 1970-01-01 of the Monday of the week `day` is in. Weeks are UTC
/// and begin on Monday, and 1970-01-01 was a Thursday, so a day's Monday is
/// that many days back.
fn monday(day: i64) -> i64 {
    day - (day - 4).rem_euclid(7)
}

/// The week a moment belongs to: `YYYY-MM-DD` of its Monday.
pub fn week_of(t: i64) -> String {
    let (y, m, d) = civil_from_days(monday(t.div_euclid(DAY)));
    format!("{y:04}-{m:02}-{d:02}")
}

/// A calendar day, `YYYY-MM-DD`, as days since 1970-01-01, or None: the parts
/// within the ranges `iso_ms` allows its own, and the date one the calendar
/// holds, which it may not be -- 2026-02-30 counts on into March there, as
/// `civil_from_days` of the day it gave would say.
fn day_of_date(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let well_formed = b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit());
    well_formed.then_some(())?;
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, m, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    if !(1970..=2100).contains(&y) || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let days = days_from_civil(y, m, d);
    // What the same reader answers for: a date that counted on into another
    // month is no date, and neither is one whose seconds would not fit.
    let (ry, rm, rd) = civil_from_days(days);
    (ry == y && rm == m && rd == d && days.checked_mul(DAY)?.checked_add(DAY).is_some())
        .then_some(days)
}

/// One bound as Unix seconds: `YYYY-MM-DD` for a day, at its start, or a full
/// UTC time as the snapshot's lines give it.
pub fn bound(s: &str) -> Option<i64> {
    match day_of_date(s) {
        Some(d) => Some(d * DAY),
        None => iso_ms(s).map(|ms| ms / 1000),
    }
}

/// The words and bounds a caller gave, or None for a word that is not a word
/// or a bound that is not a time. `asked` says which of the four were given
/// at all: one that was not is no bound and no word, which is its own answer,
/// while one that was given and reads as neither is no filter at all.
pub fn filter(given: [bool; 4], values: [Option<&str>; 4], by: &str) -> Option<Filter> {
    let by = match by {
        "version" => By::Version,
        "week" => By::Week,
        _ => return None,
    };
    let [tehas_on, rukas_on, since_on, until_on] = given;
    let [tehas, rukas, since, until] = values;
    // An option the caller did not give is no word and no bound, which is its
    // own answer. One it gave and this cannot read is no filter at all: the
    // caller asked for something the file cannot hold, and an answer of
    // nothing would read as an empty week rather than a bad argument.
    let word_of = |on: bool, v: Option<&str>| -> Option<Option<String>> {
        match (on, v) {
            (false, _) => Some(None),
            (true, Some(v)) => word(v).map(Some),
            (true, None) => None,
        }
    };
    let bound_of = |on: bool, v: Option<&str>| -> Option<Option<i64>> {
        match (on, v) {
            (false, _) => Some(None),
            (true, Some(v)) => bound(v).map(Some),
            (true, None) => None,
        }
    };
    let tehas = word_of(tehas_on, tehas)?;
    let rukas = word_of(rukas_on, rukas)?;
    let since = bound_of(since_on, since)?;
    let until = bound_of(until_on, until)?;
    Some(Filter {
        tehas,
        rukas,
        since,
        until,
        by,
    })
}

/// The lines to print: the header, then the entries. A version's entries come
/// the most runs first; a week's newest week first.
pub fn lines(conn: &Connection, now: i64, f: &Filter) -> rusqlite::Result<Vec<String>> {
    // Whole days, as the weeks are: a run of the day a bound names belongs to
    // that bound, whichever of the two shapes it was given in, so neither end
    // of the window cuts a day in half. The default lower bound is the same
    // span the sessions of a snapshot reach.
    let from = f.since.unwrap_or(now - MAX_DAYS * DAY);
    let to = f.until.unwrap_or(now);
    let rows = rows(conn, from, to)?;
    let mut grouped: Vec<(Vec<String>, Row)> = Vec::new();
    for r in rows {
        let at = r.at;
        let tehas = column(&r.tehas);
        let rukas = column(&r.rukas_version).or_else(|| column(&r.rukas_commit));
        // A run of no version belongs to no release, so it answers to no
        // version filter and joins no entry of a filtered call.
        let wanted =
            |asked: &Option<String>, value: &Option<String>| asked.is_none() || asked == value;
        if f.by == By::Version && !(wanted(&f.tehas, &tehas) && wanted(&f.rukas, &rukas)) {
            continue;
        }
        let week = (f.by == By::Week).then(|| week_of(at));
        let key: Vec<String> = match f.by {
            By::Version => vec![
                tehas.clone().unwrap_or_default(),
                rukas.clone().unwrap_or_default(),
            ],
            By::Week => vec![week.clone().unwrap_or_default()],
        };
        let row = match grouped.iter_mut().position(|(k, _)| *k == key) {
            Some(i) => &mut grouped[i].1,
            None => {
                grouped.push((key, Row::default()));
                &mut grouped.last_mut().expect("just pushed").1
            }
        };
        row.runs += 1;
        if let Some(k) = column(&r.outcome) {
            bump(&mut row.outcomes, &k);
        } else {
            bump(&mut row.outcomes, UNKNOWN);
        }
        if let Some(k) = column(&r.kind) {
            bump(&mut row.kinds, &k);
        }
        if let Some(p) = column(&r.park) {
            bump(&mut row.parks, &p);
        }
    }
    match f.by {
        By::Version => grouped.sort_by(|a, b| match b.1.runs.cmp(&a.1.runs) {
            std::cmp::Ordering::Equal => b.0.cmp(&a.0),
            other => other,
        }),
        By::Week => grouped.sort_by(|a, b| b.0.cmp(&a.0)),
    }
    let mut out = vec![json!({
        "api": API,
        "outcomes": OUTCOMES,
        "file": FILE,
        "by": match f.by { By::Version => "version", By::Week => "week" },
        "generatedAt": iso(now),
    })
    .to_string()];
    for (key, row) in grouped {
        let mut o = Map::new();
        match f.by {
            By::Version => {
                if !key[0].is_empty() {
                    o.insert("tehasVersion".into(), json!(&key[0]));
                }
                if !key[1].is_empty() {
                    o.insert("rukasVersion".into(), json!(&key[1]));
                }
            }
            By::Week => {
                if !key[0].is_empty() {
                    o.insert("week".into(), json!(&key[0]));
                }
            }
        }
        o.insert("runs".into(), json!(row.runs));
        o.insert("outcomes".into(), json!(row.outcomes));
        o.insert("failureKinds".into(), json!(row.kinds));
        o.insert("parkReasons".into(), json!(row.parks));
        out.push(Value::Object(o).to_string());
    }
    Ok(out)
}

/// The private file `<dir>/<name>`, written from `lines`: through a name of
/// this run, mode 600, and moved into place so a reader never sees half a
/// file. What is written is what the caller was given, so the host that asks
/// for the file needs no reader of its own. The directory is made mode 700 if
/// it is not there.
pub fn put(dir: &Path, name: &str, lines: &[String]) -> Result<(), String> {
    let why = |what: &str, e: std::io::Error| format!("{what}: {e}");
    // The directory is the private one of the MCP server, mode 700. Make it
    // with that mode rather than 755 and a later chmod, which would leave it
    // readable in between.
    fs::create_dir_all(dir).map_err(|e| why("the directory could not be made", e))?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
        .map_err(|e| why("the directory could not be given its mode", e))?;
    let to = dir.join(name);
    // A name of this run, so two writers of the same file cannot meet in it:
    // a cycle's end and a poll may both ask for the snapshot at once.
    let temp = to.with_extension(format!("{}.new", std::process::id()));
    let made = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)
        .map_err(|e| why("could not be written", e))?;
    let mut out = std::io::BufWriter::new(made);
    let body = format!("{}\n", lines.join("\n"));
    out.write_all(body.as_bytes())
        .and_then(|()| out.flush())
        .and_then(|()| out.get_mut().sync_all())
        .map_err(|e| {
            let _ = fs::remove_file(&temp);
            why("could not be written", e)
        })?;
    // The mode of a file made by create_new is the one asked for, masked by
    // the umask, which a service may widen. Set it again.
    fs::set_permissions(&temp, fs::Permissions::from_mode(0o600)).map_err(|e| {
        let _ = fs::remove_file(&temp);
        why("could not be given its mode", e)
    })?;
    fs::rename(&temp, &to).map_err(|e| {
        let _ = fs::remove_file(&temp);
        why("could not be put in place", e)
    })
}

/// The counts of the window, written to `<dir>/outcomes.jsonl` for the MCP
/// server. See `put` for how the file is made.
pub fn write(dir: &Path, conn: &Connection, now: i64, f: &Filter) -> Result<(), String> {
    let lines = lines(conn, now, f).map_err(|e| format!("the counts could not be read: {e}"))?;
    put(dir, FILE, &lines)
}

/// One row of `runs` as the counts read it.
struct Record {
    tehas: Option<String>,
    rukas_version: Option<String>,
    rukas_commit: Option<String>,
    outcome: Option<String>,
    kind: Option<String>,
    park: Option<String>,
    at: i64,
}

// The rows whose day is served. A run is counted on its end, and a run that
// never ended on its start, as the prune counts it.
fn rows(conn: &Connection, from: i64, to: i64) -> rusqlite::Result<Vec<Record>> {
    let mut stmt = conn.prepare(
        "SELECT tehas_version, rukas_version, rukas_commit, outcome, failure_kind, \
         park_reason, coalesce(ended, started) FROM runs \
         WHERE coalesce(ended, started) >= ?1 AND coalesce(ended, started) <= ?2",
    )?;
    let found = stmt
        .query_map(params![from, to], |r| {
            Ok(Record {
                tehas: r.get(0)?,
                rukas_version: r.get(1)?,
                rukas_commit: r.get(2)?,
                outcome: r.get(3)?,
                kind: r.get(4)?,
                park: r.get(5)?,
                at: r.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(found)
}
