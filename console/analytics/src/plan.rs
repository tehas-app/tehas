//! `analytics plan <issue> --rc <rc> --facts <file>`: one planning turn, from
//! its record as the poll leaves it when the turn has ended and before the
//! poll rewrites or removes it.
//!
//! The facts are that record, which the poll writes for the call and
//! which is read as a host file is: {session, turns, running, startedAt,
//! label, answers: [{turn, by, millwright}], ...}. A turn in flight is running,
//! with turns the turns done before it and startedAt when it started (ISO
//! 8601). Nothing is recorded without both. Under $TEHAS_HOME:
//!
//! - state/plan-<n>.json.out (`tehas plan`): {session, lost, ...}, what the
//!   turn left: the session on exit 0, lost when Pi no longer had it.
//! - docs/version.txt, as for a work run.
//!
//! The row has kind plan, and turns is the turn's number in its session, not
//! the model's turns: the turn's answer holds no usage, so no usage column is
//! set. The turn ended when the poll collected it, which is now.

use crate::loose::{Loose, Many};
use crate::run::{iso_ms, read_facts, read_json, tehas_version, Run};
use rusqlite::{params, Connection};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tehas_core::home::Home;

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

/// The word a planning answer is marked with: the one in `tehas_core::grammar`,
/// which the store's rows and the MCP server's filter read too; the case named
/// after this function in `tests.rs` asserts this reader reads it.
#[cfg(test)]
pub const MILLWRIGHT_MARK: &str = tehas_core::grammar::MILLWRIGHT;
#[cfg(not(test))]
const MILLWRIGHT_MARK: &str = tehas_core::grammar::MILLWRIGHT;

/// The poll's record of the planning turn in flight. Every field is
/// [`Loose`] or [`Many`]: one of another shape reads as nothing, as the
/// reader has always done.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct PlanRecord {
    running: Loose<bool>,
    started_at: Loose<String>,
    turns: Loose<i64>,
    label: Loose<String>,
    answers: Many<Answer>,
}

/// One answer on the record. The `millwright` key is the mark of
/// [`MILLWRIGHT_MARK`]: the test of the mark reads an answer marked with it,
/// so a renamed word in the core fails there.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Answer {
    turn: Loose<i64>,
    #[serde(rename = "millwright")]
    millwright: Loose<bool>,
}

/// What `tehas plan` left beside the record: the session on exit 0, lost
/// when Pi no longer had it.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct OutFile {
    session: Loose<String>,
    lost: Loose<bool>,
}

pub fn read(home: &Path, issue: i64, given: &Given, now: i64) -> Result<Run, String> {
    let none = || format!("no planning turn of issue {issue} in flight, so none to record");
    let rec = read_facts::<Loose<PlanRecord>>(given.facts.as_deref())
        .and_then(|r| r.0)
        .ok_or_else(none)?;
    if rec.running.0 != Some(true) {
        return Err(none());
    }
    let started = rec
        .started_at
        .get()
        .and_then(|at| iso_ms(at))
        .map(|ms| ms / 1000)
        .ok_or_else(none)?;
    let before = rec.turns.whole().unwrap_or(0);
    let turn = before + 1;
    let out = read_json::<Loose<OutFile>>(&Home::new(home).plan_file(issue as u64, ".out"))
        .and_then(|o| o.0);
    let lost = out.as_ref().and_then(|o| o.lost.0) == Some(true);
    let session = out
        .as_ref()
        .and_then(|o| o.session.get())
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
    run.how = Some(how(&rec, turn).to_string());
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
/// started the session, else `approved`), an answer each later one, or the
/// millwright's answer, marked `millwright` on record. The mark alone decides: the `by` account of the answer is not consulted.
fn how(rec: &PlanRecord, turn: i64) -> &'static str {
    if turn == 1 {
        return match rec.label.get().map(String::as_str) {
            Some("tehas:refine") => "refine",
            _ => "approved",
        };
    }
    // serde cannot name a field by a constant: `Answer` spells the mark and
    // the test of the mark reads an answer marked with it.
    debug_assert_eq!("millwright", MILLWRIGHT_MARK);
    let millwright = rec
        .answers
        .items()
        .iter()
        .any(|a| a.turn.0 == Some(turn) && a.millwright.0 == Some(true));
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
/// refused   no turn ran: its unit could not start, or `tehas plan` ran
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
