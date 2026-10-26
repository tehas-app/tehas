//! What a poll cost: poll-record, which libexec/dispatch calls as each poll
//! that held the lock ends, and poll-show, which `tehas status` reads.
//!
//!   store poll-record --started <unix[.frac]> --times <file> --tally <file>
//!                     --writes-log <file> [--starts <n>]
//!     One row of poll_cost. The wall time is from --started to now. The CPU
//!     time is the four figures of bash's `times` in <file>, the poll's and
//!     its children's. The gh calls are the bytes of the tally, one newline
//!     per call. The writes are the lines of the host shim's write log
//!     (host/shim/gh) dated from the second of --started to now, by kind. A
//!     tally that is not there is 0 calls; a write log that is not there
//!     counts nothing, and the writes are null. --starts is the number of
//!     cycles the poll started, 0 when it started none; without it the row
//!     keeps no answer for that.
//!   store poll-show
//!     The last poll's row, the sums of the last hour, and the line `tehas
//!     status` prints, null when no poll is on record.
//!   store poll-idle --since <seconds>
//!     The median wall, CPU and write cost of the polls of the last
//!     <seconds> that started no cycle, as one object: {"polls": n, "cpu_ms":
//!     m, "wall_ms": m, "writes": m}. The writes are left out when none of
//!     those polls had a write log; {"polls": 0} is the answer when none is on
//!     record. A row with no --starts on it is unknown and is left out.

use crate::verbs::{self, j, line, obj, Args, Fail, Kind, Q};
use rusqlite::{params, OptionalExtension};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use tehas_core::time::days_from_civil;

#[derive(Debug, PartialEq, Eq)]
pub enum Call {
    Record(Record),
    Show,
    Idle(Idle),
}

#[derive(Debug, PartialEq, Eq)]
pub struct Record {
    pub started_ms: i64,
    pub times: PathBuf,
    pub tally: PathBuf,
    pub writes_log: PathBuf,
    /// The cycles this poll started; None when the caller said nothing.
    pub starts: Option<i64>,
}

/// `poll-idle --since <seconds>`: the polls of the last `since` that started
/// no cycle.
#[derive(Debug, PartialEq, Eq)]
pub struct Idle {
    pub since: i64,
}

/// The most of the write log read: its end, where a poll's lines are.
const LOG_TAIL: u64 = 1 << 20;
const TIMES_BOUND: u64 = 4096;
const HOUR: i64 = 3600;

pub fn parse(name: &str, rest: &[String]) -> Option<Result<Call, String>> {
    Some(match name {
        "poll-record" => Args::parse(
            "poll-record",
            rest,
            &[
                ("--started", Kind::Value),
                ("--times", Kind::Value),
                ("--tally", Kind::Value),
                ("--writes-log", Kind::Value),
                ("--starts", Kind::Value),
            ],
            false,
        )
        .and_then(|a| {
            let path = |flag: &str| a.path(flag)?.ok_or_else(|| a.usage(flag, "is required"));
            Ok(Call::Record(Record {
                started_ms: a.strict_need("--started", "a Unix time", started)?,
                times: path("--times")?,
                tally: path("--tally")?,
                writes_log: path("--writes-log")?,
                starts: a.strict("--starts", "a number of cycles", verbs::count)?,
            }))
        }),
        "poll-show" => Args::parse("poll-show", rest, &[], false).map(|_| Call::Show),
        "poll-idle" => {
            Args::parse("poll-idle", rest, &[("--since", Kind::Value)], false).and_then(|a| {
                Ok(Call::Idle(Idle {
                    since: a.strict_need("--since", "a number of seconds", verbs::count)?,
                }))
            })
        }
        _ => return None,
    })
}

/// A Unix time with an optional fraction, as bash's EPOCHREALTIME gives it,
/// in milliseconds.
pub fn started(s: &str) -> Option<i64> {
    let (whole, frac) = s.split_once(['.', ',']).unwrap_or((s, ""));
    Some(verbs::unix(whole)? * 1000 + millis(frac)?)
}

/// The digits after the point, as milliseconds: `5` is 500.
fn millis(frac: &str) -> Option<i64> {
    if frac.len() > 9 || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    format!("{frac:0<3}")[..3].parse().ok()
}

/// The four times of bash's `times` builtin, `0m0.012s` each, summed in
/// milliseconds. None when the text holds anything else or not four.
pub fn cpu_ms(text: &str) -> Option<i64> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() != 4 {
        return None;
    }
    let mut ms = 0;
    for w in words {
        let (min, sec) = w.strip_suffix('s')?.split_once('m')?;
        let min = verbs::count(min)?;
        let (s, frac) = sec.split_once(['.', ',']).unwrap_or((sec, ""));
        ms += (min * 60 + verbs::count(s)?) * 1000 + millis(frac)?;
    }
    Some(ms)
}

/// The writes of the log's lines dated from `from` to `to`, Unix seconds.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Writes {
    pub label: i64,
    pub comment: i64,
    pub edit: i64,
    pub other: i64,
}

impl Writes {
    pub fn total(&self) -> i64 {
        self.label + self.comment + self.edit + self.other
    }
}

/// `2026-10-26T06:00:00Z` in Unix seconds.
fn iso_secs(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() != 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[19] != b'Z' {
        return None;
    }
    let n = |r: std::ops::Range<usize>| {
        let v = s.get(r)?;
        v.bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| v.parse().ok())?
    };
    let days = days_from_civil(n(0..4)?, n(5..7)?, n(8..10)?);
    Some(days * 86_400 + n(11..13)? * 3600 + n(14..16)? * 60 + n(17..19)?)
}

/// The kinds the shim writes: `label create`, `issue comment`, `pr edit`,
/// `api POST`, ... Its first word for a label call, its second else.
pub fn count_writes(log: &str, from: i64, to: i64) -> Writes {
    let mut w = Writes::default();
    for l in log.lines() {
        let mut f = l.split('\t');
        let (Some(at), Some(kind)) = (f.next().and_then(iso_secs), f.next()) else {
            continue;
        };
        if at < from || at > to {
            continue;
        }
        match kind.split_once(' ') {
            Some(("label", _)) => w.label += 1,
            Some((_, "comment")) => w.comment += 1,
            Some((_, "edit")) => w.edit += 1,
            _ => w.other += 1,
        }
    }
    w
}

/// The end of the write log, from its first whole line. None when there is no
/// regular file there.
fn log_tail(path: &Path) -> Option<String> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let mut f = std::fs::File::open(path).ok()?;
    let skip = meta.len().saturating_sub(LOG_TAIL);
    f.seek(SeekFrom::Start(skip)).ok()?;
    let mut bytes = Vec::new();
    f.take(LOG_TAIL).read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    Some(match (skip, text.split_once('\n')) {
        (0, _) | (_, None) => text,
        (_, Some((_, rest))) => rest.to_string(),
    })
}

/// Seconds as the status line says them: one decimal under ten.
pub fn secs(ms: i64) -> String {
    if ms < 9_950 {
        let tenths = (ms + 50) / 100;
        format!("{}.{}", tenths / 10, tenths % 10)
    } else {
        ((ms + 500) / 1000).to_string()
    }
}

fn plural(n: i64, one: &str) -> String {
    format!("{n} {one}{}", if n == 1 { "" } else { "s" })
}

/// The line under `factory` in `tehas status`.
pub fn status_line(last: &Last, polls: i64, cpu: i64, writes: Option<i64>) -> String {
    let w = |n: Option<i64>| n.map(|n| plural(n, "write"));
    let mut s = format!(
        "poll      last {} s wall, {} s CPU, {} gh",
        secs(last.wall_ms),
        secs(last.cpu_ms),
        last.gh
    );
    if let Some(n) = w(last.writes) {
        s += &format!(" ({n})");
    }
    s += &format!("; last hour {}, {} s CPU", plural(polls, "poll"), secs(cpu));
    if let Some(n) = w(writes) {
        s += &format!(", {n}");
    }
    s
}

#[derive(Debug, PartialEq, Eq)]
pub struct Last {
    pub at: i64,
    pub wall_ms: i64,
    pub cpu_ms: i64,
    pub gh: i64,
    pub writes: Option<i64>,
    /// The cycles that poll started; None when the row keeps no answer.
    pub starts: Option<i64>,
    pub kinds: Option<[i64; 4]>,
}

pub fn run(home: &Path, call: &Call, now: i64) -> Result<String, Fail> {
    match call {
        Call::Record(r) => record(home, r, now, now_ms()),
        Call::Show => show(home, now),
        Call::Idle(i) => idle(home, i, now),
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn garbled(what: &str) -> Fail {
    (1, format!("garbled: {what}"))
}

/// `now` dates the row; `real_ms` is the clock the wall time and the write
/// log are read against, which a test cannot set.
pub fn record(home: &Path, r: &Record, now: i64, real_ms: i64) -> Result<String, Fail> {
    let times = match verbs::read_file(&r.times, TIMES_BOUND) {
        Ok((bytes, _)) => String::from_utf8(bytes).map_err(|_| garbled("--times is not UTF-8"))?,
        Err(_) => return Err(garbled("--times is not a regular file")),
    };
    let cpu = cpu_ms(&times).ok_or_else(|| garbled("--times is not what bash's times writes"))?;
    let gh = match std::fs::symlink_metadata(&r.tally) {
        Ok(m) if m.is_file() => m.len() as i64,
        _ => 0,
    };
    let wall = (real_ms - r.started_ms).max(0);
    let writes = log_tail(&r.writes_log).map(|log| {
        count_writes(
            &log,
            r.started_ms.div_euclid(1000),
            real_ms.div_euclid(1000),
        )
    });
    let total = writes.as_ref().map(Writes::total);
    let k = |f: fn(&Writes) -> i64| writes.as_ref().map(f);
    verbs::write(home, |c| {
        c.execute(
            "INSERT INTO poll_cost (at, wall_ms, cpu_ms, gh, writes, label, comment, edit, other, starts)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                now,
                wall,
                cpu,
                gh,
                total,
                k(|w| w.label),
                k(|w| w.comment),
                k(|w| w.edit),
                k(|w| w.other),
                r.starts
            ],
        )
        .q()
    })?;
    Ok(line(
        "poll-record",
        &[
            ("at", j(now)),
            ("wall_ms", j(wall)),
            ("cpu_ms", j(cpu)),
            ("gh", j(gh)),
            ("writes", j(total)),
            ("starts", j(r.starts)),
        ],
    ))
}

/// The middle of sorted figures: the one in the middle of an odd count, the
/// mean of the two of an even one, rounded. None for nothing at all. The
/// analytics program's own median is private to that crate, so the store's is
/// written here beside the verb that asks for it.
pub fn median(sorted: &[i64]) -> Option<i64> {
    let n = sorted.len();
    if n == 0 {
        return None;
    }
    if n % 2 == 1 {
        return Some(sorted[n / 2]);
    }
    Some(((sorted[n / 2 - 1] + sorted[n / 2]) as f64 / 2.0).round() as i64)
}

/// The polls of the last `i.since` seconds that started no cycle. A row with
/// no `starts` on it is unknown and is not one of them.
fn idle(home: &Path, i: &Idle, now: i64) -> Result<String, Fail> {
    verbs::look(home, |c| {
        let mut q = c
            .prepare(
                "SELECT wall_ms, cpu_ms, writes FROM poll_cost
                 WHERE starts = 0 AND at > ?1 AND at <= ?2",
            )
            .q()?;
        let rows = q
            .query_map(params![now - i.since, now], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, Option<i64>>(2)?,
                ))
            })
            .q()?;
        let (mut wall, mut cpu, mut writes) = (Vec::new(), Vec::new(), Vec::new());
        for row in rows {
            let (w, c_, n) = row.q()?;
            wall.push(w);
            cpu.push(c_);
            if let Some(n) = n {
                writes.push(n);
            }
        }
        let polls = wall.len() as i64;
        if polls == 0 {
            return Ok(line("poll-idle", &[("polls", j(0))]));
        }
        for v in [&mut wall, &mut cpu] {
            v.sort_unstable();
        }
        let mut pairs = vec![
            ("polls", j(polls)),
            ("cpu_ms", j(median(&cpu))),
            ("wall_ms", j(median(&wall))),
        ];
        // The writes only when one of the polls had a log to count: with no
        // write log on the host the figure would say nothing.
        if !writes.is_empty() {
            writes.sort_unstable();
            pairs.push(("writes", j(median(&writes))));
        }
        Ok(line("poll-idle", &pairs))
    })
}

fn show(home: &Path, now: i64) -> Result<String, Fail> {
    verbs::look(home, |c| {
        let last = c
            .query_row(
                "SELECT at, wall_ms, cpu_ms, gh, writes, label, comment, edit, other, starts
                 FROM poll_cost ORDER BY at DESC, id DESC LIMIT 1",
                [],
                |r| {
                    let kinds: [Option<i64>; 4] = [r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?];
                    Ok(Last {
                        at: r.get(0)?,
                        wall_ms: r.get(1)?,
                        cpu_ms: r.get(2)?,
                        gh: r.get(3)?,
                        writes: r.get(4)?,
                        starts: r.get(9)?,
                        kinds: match kinds {
                            [Some(a), Some(b), Some(c), Some(d)] => Some([a, b, c, d]),
                            _ => None,
                        },
                    })
                },
            )
            .optional()
            .q()?;
        let (polls, cpu, writes): (i64, i64, Option<i64>) = c
            .query_row(
                "SELECT count(*), coalesce(sum(cpu_ms), 0), sum(writes)
                 FROM poll_cost WHERE at > ?1 AND at <= ?2",
                params![now - HOUR, now],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .q()?;
        let hour = obj(&[
            ("polls", j(polls)),
            ("cpu_ms", j(cpu)),
            ("writes", j(writes)),
        ]);
        let Some(last) = last else {
            return Ok(line(
                "poll-show",
                &[
                    ("last", "null".into()),
                    ("hour", hour),
                    ("line", "null".into()),
                ],
            ));
        };
        let said = status_line(&last, polls, cpu, writes);
        let kinds = match last.kinds {
            Some([l, c, e, o]) => obj(&[
                ("label", j(l)),
                ("comment", j(c)),
                ("edit", j(e)),
                ("other", j(o)),
            ]),
            None => "null".into(),
        };
        let shown = obj(&[
            ("at", j(last.at)),
            ("wall_ms", j(last.wall_ms)),
            ("cpu_ms", j(last.cpu_ms)),
            ("gh", j(last.gh)),
            ("writes", j(last.writes)),
            ("starts", j(last.starts)),
            ("kinds", kinds),
        ]);
        Ok(line(
            "poll-show",
            &[("last", shown), ("hour", hour), ("line", j(said))],
        ))
    })
}
