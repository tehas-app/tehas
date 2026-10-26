//! history-show: the rows of `cycle_history` and `plan_history`, the ends of
//! cycles and planning turns that cycle-end and plan-end wrote. Read only.

use crate::verbs::{self, j, line, list, obj, row_json, Args, Fail, Kind, Q};
use rusqlite::Connection;
use serde_json::Value;
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub struct Call {
    pub issue: Option<i64>,
    pub kind: Option<&'static str>,
    pub how: Option<&'static str>,
}

/// The `how` words a cycle row can hold: `cycles.how`, in the words of this
/// release, in the order a refusal prints them. Each pair is one word and its
/// old spelling: `fixer` is matched and never printed, and a filter of either
/// word keeps the rows of both. #686: part 5 drops the alias of the pair.
pub const HOWS: [(&str, &str); 6] = [
    (crate::approval::MILLWRIGHT, crate::approval::FIXER),
    ("label", "label"),
    ("refresh", "refresh"),
    ("retry-tick", "retry-tick"),
    ("outage-retry", "outage-retry"),
    ("hand", "hand"),
];

pub fn parse(name: &str, rest: &[String]) -> Option<Result<Call, String>> {
    (name == "history-show").then(|| args(rest))
}

fn args(rest: &[String]) -> Result<Call, String> {
    let a = Args::parse(
        "history-show",
        rest,
        &[("--kind", Kind::Value), ("--how", Kind::Value)],
        true,
    )?;
    let issue = match a.issue {
        Some(_) => Some(a.issue()?),
        None => None,
    };
    let kind = a.word("--kind", &[("cycle", "cycle"), ("plan", "plan")])?;
    let how = a.word("--how", &HOWS)?;
    if how.is_some() && kind == Some("plan") {
        return Err(a.usage("--how", "is not taken with --kind plan"));
    }
    Ok(Call { issue, kind, how })
}

/// The words a history read filters on. `HOWS` pairs each word with the
/// spelling it replaces, so a filter of either word of the millwright pair
/// keeps the rows of both: a row written before the rename still says `fixer`.
/// A filter of any other word is the one word given, and a read with no filter
/// is None.
fn how_words(how: Option<&'static str>) -> Option<[Option<&'static str>; 2]> {
    match how {
        Some(w) if crate::approval::is_millwright(w) => Some([
            Some(crate::approval::MILLWRIGHT),
            Some(crate::approval::FIXER),
        ]),
        Some(w) => Some([Some(w), Some(w)]),
        None => None,
    }
}

const CYCLE_NAMES: [&str; 18] = [
    "id",
    "issue",
    "attempt",
    "at",
    "status",
    "step",
    "by",
    "how",
    "pr",
    "started_at",
    "rc",
    "cap",
    "park_reason",
    "failure_kind",
    "failure_cause",
    "provider?",
    "parked?",
    "seq",
];

const PLAN_NAMES: [&str; 11] = [
    "id",
    "issue",
    "turn",
    "at",
    "outcome",
    "session",
    "started_at",
    "rc",
    "by",
    "label",
    "seq",
];

/// One row as answered, with what orders it: its `seq`, the order the ends
/// were written in.
type Row = (i64, String);

pub fn run(home: &Path, call: &Call) -> Result<String, Fail> {
    verbs::look(home, |c| {
        let mut rows: Vec<Row> = Vec::new();
        if call.kind != Some("plan") {
            rows.extend(cycles(c, call.issue, call.how)?);
        }
        // A plan row has no `how`, so a filter on it keeps none.
        if call.kind != Some("cycle") && call.how.is_none() {
            rows.extend(plans(c, call.issue)?);
        }
        rows.sort_by_key(|r| r.0);
        let rows: Vec<String> = rows.into_iter().map(|r| r.1).collect();
        Ok(line("history-show", &[("rows", list(&rows))]))
    })
}

fn cycles(c: &Connection, issue: Option<i64>, how: Option<&'static str>) -> Result<Vec<Row>, Fail> {
    // NULL is the filter of no word: no row's `how` is NULL, and the empty
    // string is no word of `HOWS`, so neither stands for a filter it does not
    // mean.
    let [first, second] = how_words(how).unwrap_or([None, None]);
    let mut q = c
        .prepare(
            "SELECT id, issue, attempt, at, status, step, by_account, how, pr, started_at, rc, cap,
               park_reason, failure_kind, failure_cause, provider, parked, seq
             FROM cycle_history
             WHERE (?1 IS NULL OR issue = ?1)
               AND (?2 IS NULL OR how = ?2 OR how = ?3)",
        )
        .q()?;
    let rows = q
        .query_map(rusqlite::params![issue, first, second], |r| {
            Ok((r.get(CYCLE_NAMES.len() - 1)?, row_json(r, &CYCLE_NAMES)?))
        })
        .q()?;
    let mut out = Vec::new();
    for row in rows {
        let (seq, mut p) = row.q()?;
        p.insert(0, ("kind", j("cycle")));
        out.push((seq, obj(&p)));
    }
    Ok(out)
}

fn plans(c: &Connection, issue: Option<i64>) -> Result<Vec<Row>, Fail> {
    let mut q = c
        .prepare(
            "SELECT id, issue, turn, at, outcome, session, started_at, rc, by_account, label, seq,
               filed_json, auto
             FROM plan_history WHERE (?1 IS NULL OR issue = ?1)",
        )
        .q()?;
    let rows = q
        .query_map([issue], |r| {
            Ok((
                r.get(PLAN_NAMES.len() - 1)?,
                row_json(r, &PLAN_NAMES)?,
                r.get::<_, Option<String>>(PLAN_NAMES.len())?,
                r.get::<_, bool>(PLAN_NAMES.len() + 1)?,
            ))
        })
        .q()?;
    let mut out = Vec::new();
    for row in rows {
        let (seq, mut p, filed, auto) = row.q()?;
        // plan-end writes the kept URLs as a JSON list; anything else is none.
        let filed = filed
            .and_then(|f| serde_json::from_str::<Value>(&f).ok())
            .filter(Value::is_array)
            .unwrap_or(Value::Array(Vec::new()));
        p.insert(0, ("kind", j("plan")));
        p.push(("filed", filed.to_string()));
        p.push(("auto", j(auto)));
        out.push((seq, obj(&p)));
    }
    Ok(out)
}
