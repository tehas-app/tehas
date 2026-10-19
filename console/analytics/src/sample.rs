//! `analytics sample`: one row of samples, read from the live files of the
//! dispatcher's poll. Under $TEHAS_HOME:
//!
//! - live/status.json (libexec/status, run at the start of every poll):
//!   {generatedAt, paused, queued: [..], running: [..], awaiting: [..],
//!   stopped: [..], planning, ...}. in_line, running, awaiting and stopped
//!   are the lengths of those lists; planning is the count of open issues
//!   labelled tehas:plan or tehas:planning, left out by a status.json from
//!   before it was kept; paused is 1 or 0.
//! - live/dispatch.json (libexec/dispatch, report_hold, on a poll that gets
//!   as far as rule 3): {time, why, count, free, limitRoom, ...}. held is
//!   why: the one word of the hold, empty when nothing holds. cycle_limit is
//!   the cycles allowed at once: count plus free (TEHAS_TICKET_CONCURRENCY),
//!   or count plus limitRoom where the limit of tehas limit is lower.
//! - live/energy.json (libexec/energy): {generatedAt, totals: {watts}}.
//!   watts is the mesh's draw, where it reports one.
//!
//! A file that is missing, a link, not JSON, or too old by its own time
//! gives NULL columns, and so does a field of the wrong type. Too old is ten
//! minutes, and for dispatch.json two and a half: a poll runs every two
//! minutes, and a paused one writes none, so an older one is an earlier
//! poll's.

use crate::run::{iso_ms, read_json};
use rusqlite::{params, Connection};
use serde_json::Value;
use std::path::Path;

/// Seconds after which a live file no longer describes the poll.
const STALE: i64 = 600;
const STALE_DISPATCH: i64 = 150;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Sample {
    pub in_line: Option<i64>,
    pub running: Option<i64>,
    pub awaiting: Option<i64>,
    pub stopped: Option<i64>,
    pub planning: Option<i64>,
    pub cycle_limit: Option<i64>,
    pub paused: Option<bool>,
    pub held: Option<String>,
    pub watts: Option<f64>,
}

pub fn read(home: &Path, now: i64) -> Sample {
    let live = home.join("live");
    let fresh = |name: &str, key: &str, stale: i64| {
        let v = read_json(&live.join(name))?;
        let at = iso_ms(v.get(key)?.as_str()?)? / 1000;
        (now - at <= stale).then_some(v)
    };
    let mut s = Sample::default();
    if let Some(st) = fresh("status.json", "generatedAt", STALE) {
        let len = |k: &str| st.get(k).and_then(Value::as_array).map(|a| a.len() as i64);
        s.in_line = len("queued");
        s.running = len("running");
        s.awaiting = len("awaiting");
        s.stopped = len("stopped");
        s.paused = st.get("paused").and_then(Value::as_bool);
        s.planning = st
            .get("planning")
            .and_then(Value::as_i64)
            .filter(|n| *n >= 0);
    }
    if let Some(d) = fresh("dispatch.json", "time", STALE_DISPATCH) {
        let n = |k: &str| d.get(k).and_then(Value::as_i64);
        s.held = d.get("why").and_then(Value::as_str).and_then(|w| {
            let ok = w.len() <= 40 && w.bytes().all(|b| b.is_ascii_lowercase() || b == b'-');
            ok.then(|| w.to_string())
        });
        if let (Some(count), Some(free)) = (n("count"), n("free")) {
            let mut limit = count + free;
            if let Some(room) = n("limitRoom") {
                limit = limit.min(count + room);
            }
            s.cycle_limit = Some(limit.max(0));
        }
    }
    if let Some(e) = fresh("energy.json", "generatedAt", STALE) {
        s.watts = e
            .get("totals")
            .and_then(|t| t.get("watts"))
            .and_then(Value::as_f64)
            .filter(|w| *w >= 0.0);
    }
    s
}

/// One row at `at`. A second sample in the same second replaces the first.
pub fn insert(conn: &Connection, at: i64, s: &Sample) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO samples
           (at, in_line, running, awaiting, stopped, planning, cycle_limit, paused, held, watts)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            at,
            s.in_line,
            s.running,
            s.awaiting,
            s.stopped,
            s.planning,
            s.cycle_limit,
            s.paused,
            s.held,
            s.watts
        ],
    )?;
    Ok(())
}
