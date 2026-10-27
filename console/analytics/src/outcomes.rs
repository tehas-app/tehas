//! `analytics outcomes`: the counts behind the MCP tool `factory_outcomes`,
//! as far back as the database holds. Line 1 is the header, then one entry
//! per line: `runs` and the three count maps of one group, and nothing else.
//!
//! Counts only. No issue number, title, failure cause or log content: those
//! stay in the 30-day tools, which read the sessions snapshot. A group is
//! named by a word of the database's own vocabularies, so no free text
//! reaches the file.
//!
//! `tehasbot status` asks for the file with `outcomes --out <directory>`,
//! which writes `<directory>/outcomes.jsonl` here rather than printing it:
//! what is checked before the file is put in place is what this module wrote,
//! so the host needs no second reader of it. Without `--out` the same lines
//! go to standard output, for a person. The directory the host names is
//! `~/tehas/mcp`, the one the MCP container is given read only.

use crate::db;
use crate::run::iso_ms;
use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::Path;
use tehas_core::time::{civil_from_days, days_from_civil, iso, week_start_of_day};

/// The shape of the entries.
pub const OUTCOMES: i64 = 1;
/// Where `tehasbot status` asks for the file, under `~/tehas/mcp`. The name
/// the reader opens. It is JSON Lines; the name says what the file is, not
/// how it is cut into lines, as `sessions.jsonl` is named for its sessions.
pub const FILE: &str = "outcomes.jsonl";
/// The most one word of the database may be to name a group or a count. A
/// longer value counts as unknown. The reader of the MCP server accepts the
/// same alphabet, less the `:` a word of the database may hold: see the pair
/// of them kept together in the analytics tests.
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
/// Only [`filter`] builds one, and what it built is already valid: the words
/// are words and the bounds are moments, so a hand-made Filter can serve
/// nothing and say nothing. The version words ask for one release each and
/// group with them, so under a week grouping they answer to nothing.
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
/// least one letter or digit. Anything else is no word. `filter` is its only
/// caller of production; the tests name the shape it accepts, and the reader
/// of the MCP server keeps its own alphabet beside it.
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

/// What an entry stands for: a pair of releases (None for one the run
/// recorded not at all), or the Monday that opens a UTC week.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Group {
    Versions(Option<String>, Option<String>),
    Week(String),
}

fn bump(counts: &mut BTreeMap<String, u64>, key: &str) {
    *counts.entry(key.to_string()).or_default() += 1;
}

/// The week a moment belongs to: `YYYY-MM-DD` of its Monday.
pub fn week_of(t: i64) -> String {
    let (y, m, d) = civil_from_days(week_start_of_day(t.div_euclid(DAY)));
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
/// or a bound that is not a time. Each of the four is None when the caller did
/// not give it, `Some(None)` when they gave it and it reads as neither a word
/// nor a moment - which is no filter at all, because an answer of nothing
/// would read as an empty week rather than a bad argument - and `Some(Some(v))`
/// for a value this module accepts.
pub fn filter(
    tehas: Option<Option<&str>>,
    rukas: Option<Option<&str>>,
    since: Option<Option<&str>>,
    until: Option<Option<&str>>,
    by: &str,
) -> Option<Filter> {
    let by = match by {
        "version" => By::Version,
        "week" => By::Week,
        _ => return None,
    };
    let words = |asked: Option<Option<&str>>| -> Option<Option<String>> {
        match asked {
            None => Some(None),
            Some(None) => None,
            Some(Some(v)) => word(v).map(Some),
        }
    };
    let bounds = |asked: Option<Option<&str>>| -> Option<Option<i64>> {
        match asked {
            None => Some(None),
            Some(None) => None,
            Some(Some(v)) => bound(v).map(Some),
        }
    };
    Some(Filter {
        tehas: words(tehas)?,
        rukas: words(rukas)?,
        since: bounds(since)?,
        until: bounds(until)?,
        by,
    })
}

/// The lines to print: the header, then the entries. A version's entries come
/// the most runs first; a week's newest week first.
pub fn lines(conn: &Connection, now: i64, f: &Filter) -> rusqlite::Result<Vec<String>> {
    // Whole days, as the weeks are: a run of the day a bound names belongs to
    // that bound, whichever of the two shapes it was given in, so neither end
    // of the window cuts a day in half. The default lower bound is the whole
    // retention, which is what makes this tool more than the 30-day ones.
    let from = f.since.unwrap_or(now - db::RETENTION_DAYS * DAY);
    let to = f.until.unwrap_or(now);
    let rows = rows(conn, from, to)?;
    // One row per group leaves SQLite, not one per run: the window is the
    // whole retention, and the poll asks for these lines on every pass. Rust
    // then puts the words of the database's vocabularies through `word` - a
    // value the database holds that is no word counts as none of one - and
    // re-buckets what SQLite answered. An ordered map, so the lookup of a
    // group is no scan of those seen so far.
    let mut grouped: BTreeMap<Group, Row> = BTreeMap::new();
    for r in rows {
        // The words of the database's own vocabularies, read as the module
        // reads them: a value that is no word counts as none of one, so no
        // free text reaches the file. A run of no version belongs to no
        // release, so it answers to no version filter and joins no entry of a
        // filtered call.
        let tehas = column(&r.tehas);
        let rukas = column(&r.rukas_version).or_else(|| column(&r.rukas_commit));
        let wanted =
            |asked: &Option<String>, value: &Option<String>| asked.is_none() || asked == value;
        if f.by == By::Version && !(wanted(&f.tehas, &tehas) && wanted(&f.rukas, &rukas)) {
            continue;
        }
        let group = match f.by {
            By::Version => Group::Versions(tehas, rukas),
            By::Week => Group::Week(week_of(r.at)),
        };
        let row = grouped.entry(group).or_default();
        // One group of the window's rows, which is how many runs it holds.
        row.runs += r.runs.max(0) as u64;
        let outcome = column(&r.outcome).unwrap_or_else(|| UNKNOWN.to_string());
        bump(&mut row.outcomes, &outcome);
        if let Some(k) = column(&r.kind) {
            bump(&mut row.kinds, &k);
        }
        if let Some(p) = column(&r.park) {
            bump(&mut row.parks, &p);
        }
    }
    // The most runs first, and the name of the group the other way for a tie;
    // a week's newest week first.
    let mut grouped: Vec<(Group, Row)> = grouped.into_iter().collect();
    match f.by {
        By::Version => grouped.sort_by_key(|(group, row)| {
            (
                std::cmp::Reverse(row.runs),
                std::cmp::Reverse(group.clone()),
            )
        }),
        By::Week => grouped.sort_by_key(|(group, _)| std::cmp::Reverse(group.clone())),
    }
    let mut out = vec![json!({
        "api": 1,
        "outcomes": OUTCOMES,
        "file": FILE,
        "days": db::RETENTION_DAYS,
        "by": match f.by { By::Version => "version", By::Week => "week" },
        "generatedAt": iso(now),
    })
    .to_string()];
    for (group, row) in grouped {
        let mut o = Map::new();
        match group {
            Group::Versions(tehas, rukas) => {
                if let Some(v) = tehas {
                    o.insert("tehasVersion".into(), json!(v));
                }
                if let Some(v) = rukas {
                    o.insert("rukasVersion".into(), json!(v));
                }
            }
            Group::Week(week) => {
                o.insert("week".into(), json!(week));
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

/// The counts of the window, written to `<dir>/outcomes.jsonl` for the MCP
/// server. See `crate::publish::put` for how the file is made.
pub fn write(dir: &Path, conn: &Connection, now: i64, f: &Filter) -> Result<(), String> {
    let lines = lines(conn, now, f).map_err(|e| format!("the counts could not be read: {e}"))?;
    crate::publish::put(dir, FILE, &lines)
}

/// One group of `runs` as the counts read it: the key it is grouped under,
/// the count it brings, and the words of the two maps that name a reason.
struct Record {
    tehas: Option<String>,
    rukas_version: Option<String>,
    rukas_commit: Option<String>,
    outcome: Option<String>,
    kind: Option<String>,
    park: Option<String>,
    at: i64,
    runs: i64,
}

// The groups of the window, counted by SQLite: a run is counted on its end,
// and a run that never ended on its start, as the prune counts it. The
// grouping is the database's so the row volume leaving it is one row per
// group of the window, not one per run of it.
fn rows(conn: &Connection, from: i64, to: i64) -> rusqlite::Result<Vec<Record>> {
    // SQLite groups nulls together, which is what a version no run recorded
    // needs: it is one group of runs that name no release.
    let mut stmt = conn.prepare(
        "SELECT tehas_version, rukas_version, rukas_commit, outcome, failure_kind, \
         park_reason, coalesce(ended, started), count(*) FROM runs \
         WHERE coalesce(ended, started) >= ?1 AND coalesce(ended, started) <= ?2 \
         GROUP BY 1, 2, 3, 4, 5, 6, 7",
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
                runs: r.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(found)
}
