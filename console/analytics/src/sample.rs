//! `analytics sample`: one row of samples, read from the live files of the
//! dispatcher's poll. Under $TEHAS_HOME:
//!
//! - live/status.json (tehasbot status, run at the start of every poll):
//!   {generatedAt, paused, queued: [..], running: [..], awaiting: [..],
//!   stopped: [..], planning, ...}. in_line, running, awaiting and stopped
//!   are the lengths of those lists; planning is the count of open issues
//!   labelled tehas:plan or tehas:planning, left out by a status.json from
//!   before it was kept; paused is 1 or 0.
//! - live/dispatch.json (tehas-dispatch, report_hold, on a poll that gets
//!   as far as rule 3): the report of `tehas_core::hold::Report`, {time, why,
//!   count, free, limitRoom, ...}, whose keys the sampler names by that type.
//!   held is
//!   why: the one word of the hold, empty when nothing holds. cycle_limit is
//!   the cycles allowed at once: count plus free (TEHAS_MAX_CYCLES,
//!   or the count the poll derived from the host when that setting is
//!   `auto`: the dispatcher writes the count it decided by into free), or
//!   count plus limitRoom where the limit of tehas limit is lower.
//!
//! A file that is missing, a link, not JSON, or too old by its own time
//! gives NULL columns, and so does a field of the wrong type. Too old is ten
//! minutes, and for dispatch.json two and a half: a poll runs every two
//! minutes, and a paused one writes none, so an older one is an earlier
//! poll's.

use crate::loose::Loose;
use crate::run::{iso_ms, read_json};
use rusqlite::{params, Connection};
use serde::de::IgnoredAny;
use serde::Deserialize;
use std::path::Path;
use tehas_core::home::{Home, Live};

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
}

/// `live/status.json`, as tehasbot status writes it. The list keys are those
/// of `tehas_core::queue_lists` and the time key `generatedAt`: a field of
/// another shape reads as nothing, and the tests that write a status.json by
/// the core's names keep these ones true.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct StatusFile {
    #[serde(rename = "generatedAt")]
    generated_at: Loose<String>,
    queued: Loose<Vec<IgnoredAny>>,
    running: Loose<Vec<IgnoredAny>>,
    awaiting: Loose<Vec<IgnoredAny>>,
    stopped: Loose<Vec<IgnoredAny>>,
    planning: Loose<i64>,
    paused: Loose<bool>,
}

/// `live/dispatch.json`, the report of `tehas_core::hold::Report`. The keys
/// are the core's, named by the tests that write a report through the type.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct DispatchFile {
    time: Loose<String>,
    why: Loose<String>,
    count: Loose<i64>,
    free: Loose<i64>,
    limit_room: Loose<i64>,
}

pub fn read(home: &Path, now: i64) -> Sample {
    let home = Home::new(home);
    let mut s = Sample::default();
    if let Some(st) = read_json::<StatusFile>(&home.live(Live::Status)).filter(|st| {
        st.generated_at
            .get()
            .and_then(|at| iso_ms(at))
            .is_some_and(|ms| now - ms / 1000 <= STALE)
    }) {
        let len = |v: &Loose<Vec<IgnoredAny>>| v.get().map(|a| a.len() as i64);
        s.in_line = len(&st.queued);
        s.running = len(&st.running);
        s.awaiting = len(&st.awaiting);
        s.stopped = len(&st.stopped);
        s.paused = st.paused.0;
        s.planning = st.planning.whole();
    }
    if let Some(d) = read_json::<DispatchFile>(&home.live(Live::Dispatch)).filter(|d| {
        d.time
            .get()
            .and_then(|at| iso_ms(at))
            .is_some_and(|ms| now - ms / 1000 <= STALE_DISPATCH)
    }) {
        s.held = d.why.get().and_then(|w| {
            // The word is open by shape, not by the core's list: a row
            // with a word a future poll added is a held poll, and dropping
            // it would read as no hold at all.
            let ok = w.len() <= 40 && w.bytes().all(|b| b.is_ascii_lowercase() || b == b'-');
            ok.then(|| w.to_string())
        });
        if let (Some(count), Some(free)) = (d.count.0, d.free.0) {
            let mut limit = count + free;
            if let Some(room) = d.limit_room.0 {
                limit = limit.min(count + room);
            }
            s.cycle_limit = Some(limit.max(0));
        }
    }
    s
}

/// One row at `at`. A second sample in the same second replaces the first.
pub fn insert(conn: &Connection, at: i64, s: &Sample) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO samples
           (at, in_line, running, awaiting, stopped, planning, cycle_limit, paused, held)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            at,
            s.in_line,
            s.running,
            s.awaiting,
            s.stopped,
            s.planning,
            s.cycle_limit,
            s.paused,
            s.held
        ],
    )?;
    Ok(())
}
