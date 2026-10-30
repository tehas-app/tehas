//! The clock sample: the (wall, monotonic) pair of a call, and the newest one
//! on record (#1043).
//!
//! A host restored from a save wakes with the wall clock some time ahead of
//! where it left, while every monotonic clock kept time. A rule that judges an
//! age on the wall clock then reads every heartbeat as older than its limit.
//! The store keeps one pair of its own readings so the rules of a later call
//! can tell the two clocks apart: `Now::read` takes the pair next to the
//! `now` of the call (main.rs reads `now` once per process, and the pair is
//! read in the same instant) and compares it with the newest pair on record.
//!
//! `mono` is a reading of the process's own instant clock, so it means nothing
//! across processes: only the difference between two of the store's readings
//! is used, both taken with the same program. A sample a call read is kept,
//! and the next call measures against it; the newest pair is what `publish`
//! writes to live/clock.json, where the health report reads it.

use crate::verbs::Q;
use rusqlite::{params, Connection};
use std::sync::OnceLock;
use std::time::Instant;

/// The instant this process began: the zero of the monotonic clock of every
/// pair it takes, so the readings of one program are on one clock.
static PROCESS_BEGAN: OnceLock<Instant> = OnceLock::new();

/// The largest difference between the two clocks' advance that still counts as
/// the clocks agreeing. A host's wall clock is corrected in whole seconds and
/// the two clocks of one process are read microseconds apart, so a step is
/// never found near this figure.
pub const TOLERANCE: i64 = 5;
/// How long after a step the rules that judge an age stay held off: one full
/// heartbeat interval (#1043). It is LOST_AFTER, the shortest age any of these
/// rules judges, so holding a rule for it cannot notice a real death earlier
/// than the rule itself would have noticed it.
pub const HOLD_AFTER_STEP: i64 = crate::live::LOST_AFTER;

/// The pair one call read: the wall clock the verb's `now` comes from, and the
/// monotonic clock of the same instant, whole seconds since this process read
/// its first pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Sample {
    /// What the host's clock said when the pair was taken.
    pub wall: i64,
    /// What the store's own clock said at the same instant: whole seconds since
    /// the process that took the pair began.
    pub mono: i64,
    /// The wall time of the call that took the pair: what a reader counts the
    /// age of the pair from, since the wall reading of the pair itself is the
    /// time the host's clock said.
    pub taken_at: i64,
}

/// The pair a call takes: its wall reading is the verb's `now`, and `mono` is
/// the store's own clock at the instant of the call, whole seconds since the
/// program began. The monotonic reading means nothing across processes -- only
/// two readings of one program are comparable -- so it is read by the caller
/// that owns the process (`mono` of this module) and given to the pair; the
/// pair is kept so the hold that follows a step can be counted on a clock the
/// host cannot step.
pub fn pair(wall: i64, mono: i64) -> Sample {
    Sample {
        wall,
        mono,
        taken_at: wall,
    }
}

/// The store's monotonic clock: whole seconds since the process began. A host's
/// wall clock steps; this one does not, which is what makes the two of use
/// together. It means nothing across processes -- only two readings of one program
/// are comparable -- which is why a pair is kept rather than inferred.
pub fn mono() -> i64 {
    Instant::now()
        .duration_since(*PROCESS_BEGAN.get_or_init(Instant::now))
        .as_secs() as i64
}

/// What a call knows of the clocks, read once, in the verb's own transaction.
#[derive(Debug, Clone, Copy, Default)]
pub struct Now {
    /// The pair the store holds, as this call found it.
    pub before: Option<Sample>,
    /// The pair of this call; None for a process that read no pair.
    pub got: Option<Sample>,
    /// The seconds the wall clock stepped against the monotonic clock, or
    /// None when it did not.
    pub stepped: Option<i64>,
    /// The monotonic reading the step was measured from: the older of the two
    /// pairs a step was found between. The hold that follows is counted on the
    /// store's own clock from this reading, so a pair left by a program that ran
    /// on another day holds no rule off for its own age -- the reading of one
    /// process says nothing to another, which is why `seen` of a row written
    /// beside this call's pair is the reading of the row or of the call,
    /// whichever of the two ran first, and never a sum of them.
    pub seen: Option<i64>,
}

impl Now {
    /// The pair of the store `held` and the pair of this call, which is None
    /// for a process that has read no pair. `read` is the normal route; this
    /// one is for a verb that reads its rows on a connection of its own, away
    /// from the write lock it later takes, and for a call with no pair at all
    /// -- `Now::default()` is the reading of a call that read none.
    pub fn of(before: Option<Sample>, got: Option<Sample>) -> Now {
        let stepped = step(before, got);
        // The step is measured from the older of the two pairs, and the hold that
        // follows is counted from there on the clock of the call that found it --
        // which is why `seen` is the smaller of the two readings and never the
        // reading of a row written by some other run.
        let seen = match (stepped, before, got) {
            (Some(_), Some(was), Some(g)) => Some(was.mono.min(g.mono)),
            _ => None,
        };
        Now {
            before,
            got,
            stepped,
            seen,
        }
    }

    /// Whether a rule that judges an age may act on this call: false while one
    /// is held off for `HOLD_AFTER_STEP` of the monotonic clock after the step
    /// was seen. A call that read no pair cannot place the step, so it acts.
    pub fn holds(&self) -> bool {
        !may_act(self.got.map_or(0, |g| g.mono), self.seen)
    }

    /// Keep the pair this call read, when it read one. Each sample replaces the
    /// one before it, so the store holds the newest pair of the host.
    pub fn record(&self, conn: &Connection, now: i64) -> Result<(), crate::verbs::Fail> {
        match self.got {
            Some(got) => record(conn, &got, now),
            None => Ok(()),
        }
    }

    /// The pair to publish: this call's, or the one the store holds when the
    /// call read none.
    pub fn keep(&self) -> Option<Sample> {
        self.got.or(self.before)
    }
    /// The clock's entry of live/clock.json, or None when there is no pair to
    /// say anything of. `publish` writes it with the files it already
    /// publishes, and `console health report` reads it from there.
    ///
    ///   {"at":<wall of the newest pair>,"wall":<wall>,"mono":<mono>,
    ///    "step":<seconds>|null,"seen":<the monotonic it was measured from>|null}
    ///
    /// `at` is when the newest pair was taken, so a reader can tell a pair of
    /// this minute from one of last night; `step` is what this call found, and
    /// `seen` is where the step was measured from, so the reader that prints
    /// the line can tell when the step stops being news.
    pub fn json(&self) -> Option<String> {
        let keep = self.keep()?;
        Some(crate::verbs::obj(&[
            ("at", crate::verbs::j(keep.taken_at)),
            ("wall", crate::verbs::j(keep.wall)),
            ("mono", crate::verbs::j(keep.mono)),
            ("step", crate::verbs::j(self.stepped)),
            ("seen", crate::verbs::j(self.seen)),
        ]))
    }
}

/// The newest pair on record.
///
/// A pair means nothing except as a pair of the process that took it: its two
/// readings are of that process's clocks, and only the difference between the
/// readings of two calls of one process is a measure of anything. The row is kept
/// across processes for its wall reading and its `taken_at`, which a reader outside
/// the store uses; a step is only ever found between a row this call -- or the call
/// before it, of the same process -- wrote, which is what `Now::of` is given.
pub fn kept(conn: &Connection, _now: i64) -> Option<Sample> {
    conn.query_row(
        "SELECT wall, mono, taken_at FROM clock_sample WHERE id = 1",
        [],
        |r| {
            Ok(Sample {
                wall: r.get(0)?,
                mono: r.get(1)?,
                taken_at: r.get(2)?,
            })
        },
    )
    .ok()
}

/// The row of `got`, taken next to `now`, the wall time the verb judged by.
pub fn record(conn: &Connection, got: &Sample, now: i64) -> Result<(), crate::verbs::Fail> {
    conn.execute(
        "INSERT OR REPLACE INTO clock_sample (id, wall, mono, taken_at) VALUES (1, ?1, ?2, ?3)",
        params![got.wall, got.mono, now],
    )
    .q()?;
    Ok(())
}

/// What `got` says about the clocks against `before`: None when there is no
/// pair to compare with or the two clocks agree, and otherwise the seconds the
/// wall clock stepped by, positive forward.
pub fn step(before: Option<Sample>, got: Option<Sample>) -> Option<i64> {
    let (was, got) = match (before, got) {
        (Some(was), Some(got)) => (was, got),
        _ => return None,
    };
    // The advance of the wall clock and of the store's own between the pair on
    // record and this call's pair. A step is a difference between the two.
    let drift = (got.wall.checked_sub(was.wall)? - got.mono.checked_sub(was.mono)?).abs();
    (drift > TOLERANCE).then_some(got.wall - was.wall)
}

/// Whether a rule that judges an age may act at the monotonic reading `mono`:
/// true when no step is held against it, false for `HOLD_AFTER_STEP` seconds
/// of the monotonic clock after the step at `seen`.
pub fn may_act(mono: i64, seen: Option<i64>) -> bool {
    match seen {
        None => true,
        Some(seen) => mono - HOLD_AFTER_STEP >= seen,
    }
}
