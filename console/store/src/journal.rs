//! The action journal of section 5 of the state store's spec: what a poll
//! meant to write on GitHub or the host, written before the call and settled
//! after it, so a poll that died between two writes can be read afterwards.
//! The dispatcher in Rust is the one caller (#254).
//!
//!   store poll-begin
//!     The poll's row in polls, dated now: what an action names as the poll
//!     that intended it. A second poll in the same second shares the row.
//!   store journal-record --poll <at> --kind <kind> --issue <n> --args <json>
//!                        --before <at>
//!     One action, intended: the whole call in --args (a JSON object), the
//!     read it rests on in --before. {"action":N}.
//!   store journal-attempt --action N
//!     The step just before the call: intended becomes attempted.
//!   store journal-settle --action N (--result <result> | --abandoned)
//!                        [--evidence <json>]
//!     After the call: ok or empty is done, read-failed or timed-out is
//!     failed. --abandoned is the settling poll's word for an action whose
//!     fresh read says it did not happen and need not.
//!   store journal-open
//!     The actions intended or attempted and not settled, oldest first.

use crate::verbs::{self, j, line, list, obj, refuse, Args, Fail, Kind, Q};
use rusqlite::{params, OptionalExtension};
use serde_json::Value;
use std::path::Path;

/// The kinds of section 5's table.
pub const KINDS: [&str; 9] = [
    "label-add",
    "label-remove",
    "label-remove-only",
    "comment-post",
    "comment-edit",
    "container-start",
    "unit-stop",
    "refresh-begin",
    "refresh-end",
];

/// The typed results of every external call (section 3).
const RESULTS: [&str; 4] = ["ok", "empty", "read-failed", "timed-out"];

/// The most of --args or --evidence taken: a comment's body is 65,536
/// characters at most on GitHub, and four bytes a character at worst.
const JSON_BOUND: usize = 300_000;

#[derive(Debug, PartialEq, Eq)]
pub enum Call {
    Begin,
    Record(Record),
    Attempt(i64),
    Settle(Settle),
    Open,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Record {
    pub poll: i64,
    pub kind: &'static str,
    pub issue: i64,
    pub args: String,
    pub before: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Settle {
    pub action: i64,
    /// None for --abandoned.
    pub result: Option<&'static str>,
    pub evidence: Option<String>,
}

/// A JSON object, written back compact.
fn object(s: &str) -> Option<String> {
    if s.len() > JSON_BOUND {
        return None;
    }
    match serde_json::from_str::<Value>(s) {
        Ok(v @ Value::Object(_)) => Some(v.to_string()),
        _ => None,
    }
}

fn one_of(list: &'static [&'static str]) -> impl Fn(&str) -> Option<&'static str> {
    move |s| list.iter().find(|k| **k == s).copied()
}

pub fn parse(name: &str, rest: &[String]) -> Option<Result<Call, String>> {
    Some(match name {
        "poll-begin" => Args::parse("poll-begin", rest, &[], false).map(|_| Call::Begin),
        "journal-record" => Args::parse(
            "journal-record",
            rest,
            &[
                ("--poll", Kind::Value),
                ("--kind", Kind::Value),
                ("--issue", Kind::Value),
                ("--args", Kind::Value),
                ("--before", Kind::Value),
            ],
            false,
        )
        .and_then(|a| {
            Ok(Call::Record(Record {
                poll: a.strict_need("--poll", "a Unix time", verbs::unix)?,
                kind: a.strict_need(
                    "--kind",
                    &format!("one of {}", KINDS.join(", ")),
                    one_of(&KINDS),
                )?,
                issue: a.strict_need("--issue", "an issue number", verbs::number)?,
                args: a.strict_need("--args", "a JSON object", object)?,
                before: a.strict_need("--before", "a Unix time", verbs::unix)?,
            }))
        }),
        "journal-attempt" => {
            Args::parse("journal-attempt", rest, &[("--action", Kind::Value)], false).and_then(
                |a| {
                    Ok(Call::Attempt(a.strict_need(
                        "--action",
                        "an action number",
                        verbs::number,
                    )?))
                },
            )
        }
        "journal-settle" => Args::parse(
            "journal-settle",
            rest,
            &[
                ("--action", Kind::Value),
                ("--result", Kind::Value),
                ("--abandoned", Kind::Switch),
                ("--evidence", Kind::Value),
            ],
            false,
        )
        .and_then(|a| {
            let result = a.strict(
                "--result",
                &format!("one of {}", RESULTS.join(", ")),
                one_of(&RESULTS),
            )?;
            match (result.is_some(), a.has("--abandoned")) {
                (true, true) => return Err(a.usage("--abandoned", "is not taken with --result")),
                (false, false) => return Err(a.usage("--result", "or --abandoned is required")),
                _ => {}
            }
            Ok(Call::Settle(Settle {
                action: a.strict_need("--action", "an action number", verbs::number)?,
                result,
                evidence: a.strict("--evidence", "a JSON object", object)?,
            }))
        }),
        "journal-open" => Args::parse("journal-open", rest, &[], false).map(|_| Call::Open),
        _ => return None,
    })
}

pub fn run(home: &Path, call: &Call, now: i64) -> Result<String, Fail> {
    match call {
        Call::Begin => begin(home, now),
        Call::Record(r) => record(home, r, now),
        Call::Attempt(id) => attempt(home, *id, now),
        Call::Settle(s) => settle(home, s, now),
        Call::Open => open(home),
    }
}

fn begin(home: &Path, now: i64) -> Result<String, Fail> {
    verbs::write(home, |c| {
        c.execute(
            "INSERT OR IGNORE INTO polls (at, lock_taken_at) VALUES (?1, ?1)",
            [now],
        )
        .q()
    })?;
    Ok(line("poll-begin", &[("at", j(now))]))
}

fn record(home: &Path, r: &Record, now: i64) -> Result<String, Fail> {
    let id = verbs::write(home, |c| {
        let known: bool = c
            .query_row("SELECT count(*) FROM polls WHERE at = ?1", [r.poll], |x| {
                x.get::<_, i64>(0)
            })
            .q()?
            > 0;
        if !known {
            return Err(refuse(
                "poll",
                r.poll,
                "is not on record; run poll-begin first",
            ));
        }
        c.execute(
            "INSERT INTO actions (poll_at, kind, issue, args_json, before_snapshot, state, intended_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'intended', ?6)",
            params![r.poll, r.kind, r.issue, r.args, r.before, now],
        )
        .q()?;
        let id = c.last_insert_rowid();
        c.execute(
            "INSERT INTO journal (action, at, step) VALUES (?1, ?2, 'intended')",
            params![id, now],
        )
        .q()?;
        Ok(id)
    })?;
    Ok(line("journal-record", &[("action", j(id))]))
}

/// The state of an action, or the refusal for one that is not on record.
fn state(c: &rusqlite::Connection, id: i64) -> Result<String, Fail> {
    c.query_row("SELECT state FROM actions WHERE id = ?1", [id], |r| {
        r.get(0)
    })
    .optional()
    .q()?
    .ok_or_else(|| refuse("action", id, "is not on record"))
}

fn attempt(home: &Path, id: i64, now: i64) -> Result<String, Fail> {
    verbs::write(home, |c| {
        let was = state(c, id)?;
        if was != "intended" {
            return Err(refuse("action", id, &format!("is {was}, not intended")));
        }
        c.execute(
            "UPDATE actions SET state = 'attempted', attempted_at = ?2 WHERE id = ?1",
            params![id, now],
        )
        .q()?;
        c.execute(
            "INSERT INTO journal (action, at, step) VALUES (?1, ?2, 'attempt')",
            params![id, now],
        )
        .q()
    })?;
    Ok(line(
        "journal-attempt",
        &[("action", j(id)), ("state", j("attempted"))],
    ))
}

fn settle(home: &Path, s: &Settle, now: i64) -> Result<String, Fail> {
    let to = match s.result {
        Some("ok" | "empty") => "done",
        Some(_) => "failed",
        None => "abandoned",
    };
    verbs::write(home, |c| {
        let was = state(c, s.action)?;
        if !matches!(was.as_str(), "intended" | "attempted") {
            return Err(refuse(
                "action",
                s.action,
                &format!("is settled already ({was})"),
            ));
        }
        // A result is a call's, and a call is made only once attempted.
        if was == "intended" && s.result.is_some() {
            return Err(refuse(
                "action",
                s.action,
                "was never attempted, so it has no result; --abandoned settles it",
            ));
        }
        c.execute(
            "UPDATE actions SET state = ?2, settled_at = ?3, result = ?4,
               evidence_json = coalesce(?5, evidence_json)
             WHERE id = ?1",
            params![s.action, to, now, s.result, s.evidence],
        )
        .q()?;
        c.execute(
            "INSERT INTO journal (action, at, step, detail) VALUES (?1, ?2, 'settle', ?3)",
            params![s.action, now, s.result.unwrap_or("abandoned")],
        )
        .q()
    })?;
    Ok(line(
        "journal-settle",
        &[
            ("action", j(s.action)),
            ("state", j(to)),
            ("result", j(s.result)),
        ],
    ))
}

fn open(home: &Path) -> Result<String, Fail> {
    verbs::look(home, |c| {
        let mut q = c
            .prepare(
                "SELECT id, poll_at, kind, issue, args_json, before_snapshot, state,
                        intended_at, attempted_at
                 FROM actions WHERE state IN ('intended','attempted')
                 ORDER BY intended_at, id",
            )
            .q()?;
        let rows = q
            .query_map([], |r| {
                let args: String = r.get(4)?;
                Ok(obj(&[
                    ("id", j(r.get::<_, i64>(0)?)),
                    ("poll_at", j(r.get::<_, i64>(1)?)),
                    ("kind", j(r.get::<_, String>(2)?)),
                    ("issue", j(r.get::<_, i64>(3)?)),
                    (
                        "args",
                        serde_json::from_str::<Value>(&args)
                            .unwrap_or(Value::Null)
                            .to_string(),
                    ),
                    ("before", j(r.get::<_, i64>(5)?)),
                    ("state", j(r.get::<_, String>(6)?)),
                    ("intended_at", j(r.get::<_, i64>(7)?)),
                    ("attempted_at", j(r.get::<_, Option<i64>>(8)?)),
                ]))
            })
            .q()?
            .collect::<Result<Vec<_>, _>>()
            .q()?;
        Ok(line("journal-open", &[("actions", list(&rows))]))
    })
}
