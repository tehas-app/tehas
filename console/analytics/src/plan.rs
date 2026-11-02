//! `analytics plan <issue> --rc <rc> --facts <file>`: one planning turn, from
//! its record as the poll leaves it when the turn has ended and before the
//! poll rewrites or removes it.
//!
//! The facts are that record, which the poll writes for the call and
//! which is read as a host file is: {session, turns, running, startedAt,
//! label, answers: [{turn, by, millwright}], ...} (an older record says
//! fixer). A turn in flight is running,
//! with turns the turns done before it and startedAt when it started (ISO
//! 8601). Nothing is recorded without both. Under $TEHAS_HOME:
//!
//! - state/plan-<n>.json.out (libexec/plan): {session, lost, ...}, what the
//!   turn left: the session on exit 0, lost when Pi no longer had it.
//! - docs/version.txt, as for a work run.
//!
//! The row has kind plan, and turns is the turn's number in its session, not
//! the model's turns: pi's rpc mode reports no usage, so no usage column is
//! set. The turn ended when the poll collected it, which is now.

use crate::run::{iso_ms, read_facts, read_json, tehas_version, Run};
use rusqlite::{params, Connection};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// What the poll passes: how the turn's unit ended (its exit status, interrupted, not-started or not-readable), the
/// sandbox image's id, build time and pi-rukas commit, and the facts file.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Given {
    pub rc: String,
    pub rukas_image: Option<String>,
    pub image: Option<String>,
    pub image_built: Option<i64>,
    pub facts: Option<PathBuf>,
}

pub const RC_WORDS: [&str; 3] = ["interrupted", "not-started", "not-readable"];

/// The words a planning answer is marked with: `millwright` and the `fixer`
/// a record written before the rename still carries. The pair is the one in
/// `tehas_core::grammar`, which the store's rows and the MCP server's filter
/// read too, so no two of them spell the class apart; the case named after this
/// function in `tests.rs` asserts this reader reads those two.
#[cfg(test)]
pub const MILLWRIGHT_MARKS: [&str; 2] = tehas_core::grammar::MILLWRIGHT_WORDS;
#[cfg(not(test))]
const MILLWRIGHT_MARKS: [&str; 2] = tehas_core::grammar::MILLWRIGHT_WORDS;

pub fn read(home: &Path, issue: i64, given: &Given, now: i64) -> Result<Run, String> {
    let state = home.join("state");
    let none = || format!("no planning turn of issue {issue} in flight, so none to record");
    let rec = read_facts(given.facts.as_deref()).ok_or_else(none)?;
    let rec = rec.as_object().ok_or_else(none)?;
    if rec.get("running").and_then(Value::as_bool) != Some(true) {
        return Err(none());
    }
    let started = rec
        .get("startedAt")
        .and_then(Value::as_str)
        .and_then(iso_ms)
        .map(|ms| ms / 1000)
        .ok_or_else(none)?;
    let before = rec
        .get("turns")
        .and_then(Value::as_i64)
        .filter(|n| *n >= 0)
        .unwrap_or(0);
    let turn = before + 1;
    let out = read_json(&state.join(format!("plan-{issue}.json.out")));
    let lost = out
        .as_ref()
        .and_then(|o| o.get("lost"))
        .and_then(Value::as_bool)
        == Some(true);
    let session = out
        .as_ref()
        .and_then(|o| o.get("session"))
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty());
    let code = given.rc.parse::<i64>().ok();

    let mut run = Run {
        id: format!("plan-{issue}-{started}"),
        issue,
        started,
        ended: Some(now),
        seconds: Some((now - started).max(0)),
        turns: Some(turn),
        exit_code: code,
        rukas_commit: given.rukas_image.clone(),
        image: given.image.clone(),
        image_built: given.image_built,
        ..Run::default()
    };
    run.how = Some(how(rec, turn).to_string());
    run.status = if code != Some(0) && lost {
        Some("lost".to_string())
    } else if code.is_none() {
        Some(given.rc.clone())
    } else {
        None
    };
    run.outcome = Some(outcome(code, &given.rc, lost, session).to_string());
    (run.tehas_version, run.tehas_commit) = tehas_version(home);
    Ok(run)
}

/// Who started the turn: the label the first (`refine` when tehas:refine
/// started the session, else `label`), an answer each later one, or the
/// millwright's answer. The mark on record is `millwright`; a record from
/// before it was named so says `fixer`, and the two are the same person. The
/// mark alone decides: the `by` account of the answer is not consulted.
fn how(rec: &serde_json::Map<String, Value>, turn: i64) -> &'static str {
    if turn == 1 {
        return match rec.get("label").and_then(Value::as_str) {
            Some("tehas:refine") => "refine",
            _ => "label",
        };
    }
    let millwright = rec
        .get("answers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter(|a| a.get("turn").and_then(Value::as_i64) == Some(turn))
        .any(|a| {
            MILLWRIGHT_MARKS
                .iter()
                .any(|k| a.get(*k).and_then(Value::as_bool) == Some(true))
        });
    if millwright {
        "millwright"
    } else {
        "answer"
    }
}

/// How the turn ended, in the words of the runs table:
/// handoff   it finished (exit 0 with a session) and asked its question: a
///           person answers next
/// stopped   Pi no longer had the session, which is then ended
/// refused   no turn ran: its unit could not start, or libexec/plan ran
///           nothing (4 attachments, 64 usage, 78 settings)
/// failed    any other end: a turn that did not settle (2), was cut off, or
///           left an exit status that cannot be read
fn outcome(code: Option<i64>, rc: &str, lost: bool, session: bool) -> &'static str {
    match code {
        Some(0) if session => "handoff",
        Some(0) => "failed",
        _ if lost => "stopped",
        Some(4 | 64 | 78) => "refused",
        None if rc == "not-started" => "refused",
        _ => "failed",
    }
}

/// Inserts the turn. A turn recorded again keeps what was recorded first:
/// plan_finish runs again for a turn whose end it could not post.
pub fn insert(conn: &Connection, run: &Run) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO runs (id, kind, issue, how, started, ended, seconds, outcome,
           status, exit_code, turns, tehas_version, tehas_commit, rukas_commit, image)
         VALUES (?1, 'plan', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            run.id,
            run.issue,
            run.how,
            run.started,
            run.ended,
            run.seconds,
            run.outcome,
            run.status,
            run.exit_code,
            run.turns,
            run.tehas_version,
            run.tehas_commit,
            run.rukas_commit,
            run.image,
        ],
    )?;
    crate::db::image_built(conn, run.image.as_deref(), run.image_built)
}
