//! `analytics report --json`: the ten-day report the dashboard shows, as one
//! JSON object. libexec/status writes it to live/analytics.json, which is
//! public, so it holds aggregates only: counts, durations, token sums,
//! version strings and the vocabulary words of stop reasons and steps. No
//! issue number, title, failure cause, account, model or provider.
//!
//! The window is ten UTC days: from the midnight nine days before `to`'s
//! day, to `to`, the time of the report. Prune keeps 365 days back from
//! now, so all of it is in the database.
//!
//! Every figure carries its count `n`. A value that cannot be worked out,
//! such as a share of nothing, is left out, never null. Work runs (kind
//! work) that ended in the window make the run figures; planning turns are
//! only counted, in planningTurns. Durations are seconds.
//!
//! `refine` follows the issues a refining session filed (tehas:refine), read
//! from state/refined.jsonl beside the database, next to the other issues
//! that were made ready in the window: how many started and shipped, and the
//! lead time of the refined ones. Counts only, no issue number.
//!
//! `sizes` says how issues of each size fare, read from state/sizes.jsonl
//! (lib/sizing.sh) beside the database: the issues measured in the window in
//! three buckets of the file count, how many of them shipped and the median
//! lead time of those. Counts only, no issue number: the sizes are coarse and
//! say nothing of what the issue asked for.
//!
//! `gate` says what the runs of the product's gate cost, and is the one section
//! of the report a `tehasbot status` poll does not publish: `libexec/status`
//! names the keys it copies and this is not among them. Its figures are the
//! median and p90 of one run of the gate, how many a work run made and their
//! share of the runs' time, over the runs that recorded them, from the two gate
//! columns `run.rs` fills. Counts and durations only.
//!
//! `drivers` compares the drivers on the same work runs: for each of them the
//! runs, the share that ended in a pull request a person could merge, the share
//! parked, minutes, tokens, and of those pull requests, one per issue, how many
//! were merged by now. A run with no driver on record is classic. Lean does not
//! merge, so the figure is the pull requests', not the driver's own merges. Like
//! `gate`, `tehasbot status` does not publish it.

use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use tehas_core::time::civil_from_days;

pub const REPORT_VERSION: i64 = 1;
pub const DAYS: i64 = 10;
const DAY: i64 = 86_400;
/// The longest stretch one sample stands for. A longer gap between samples
/// is a dispatcher that was not polling: it is not counted as available,
/// nor as one long draw of the sample's watts.
pub const SAMPLE_SPAN: i64 = 600;
/// A version pair with fewer work runs shows its count only.
pub const VERSION_FLOOR: usize = 3;
use crate::run::DRIVERS;
const STOP_REASONS: usize = 5;
const STEPS: usize = 12;
const VERSIONS: usize = 10;
/// The size buckets, in the order the report lists them. `size_bucket` returns
/// one and `name` gives the word of it, so no bucket the report prints is one
/// the counting does not know.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Bucket {
    None,
    Small,
    Medium,
    Large,
}

impl Bucket {
    /// The word of the bucket, the one the report and the manual name it by.
    fn name(self) -> &'static str {
        match self {
            Bucket::None => "none",
            Bucket::Small => "1-2",
            Bucket::Medium => "3-4",
            Bucket::Large => "5+",
        }
    }
}

const SIZE_BUCKETS: [Bucket; 4] = [Bucket::None, Bucket::Small, Bucket::Medium, Bucket::Large];
/// Bytes of a jsonl ledger the report reads: `LEDGER_MAX_BYTES` of the crate's
/// root, which `sessions` reads the ledgers under too.
const LEDGER_MAX_BYTES: u64 = crate::LEDGER_MAX_BYTES;
/// A ledger line of this many bytes or more is a paste, not a record: it is
/// skipped rather than parsed, so one pathological line costs the read that
/// line and not the file. `LEDGER_MAX_LINE` of the crate's root.
const LEDGER_MAX_LINE: usize = crate::LEDGER_MAX_LINE;
/// The bounds a ledger timestamp may fall inside, as what `iso_ms` turns them
/// into: one epoch year back and one forward, as milliseconds. The parse bounds
/// the year it will hold, and this bounds what the report will answer for: a
/// stamp outside them is a date no window of the report holds.
const LEDGER_AT_MIN: i64 = 1_735_689_600_000; // 2025-01-01T00:00:00Z
const LEDGER_AT_MAX: i64 = 4_102_444_800_000; // 2100-01-01T00:00:00Z
/// The greatest `files` count a ledger line may hold. Above it the bucket is
/// "5+" whatever the number, and `config.sh` refuses a `TEHAS_SIZE_MAX_FILES`
/// of more than nine digits, so a count of more holds no figure the buckets
/// can answer for.
const SIZE_MAX_FILES: i64 = 999_999_999;
/// The greatest gate share the report prints: a run of the gate that outlasted
/// the run it was in is a figure of two clocks, not a share above the whole.
const MAX_SHARE: f64 = 1.0;
/// The two ledgers beside the database, named once: `sessions` reads
/// `refined.jsonl` too, and both readers must mean the same file.
const SIZES_LEDGER: &str = "sizes.jsonl";
pub(crate) const REFINED_LEDGER: &str = "refined.jsonl";

/// A word of a published vocabulary, or "other": a value of any other shape
/// is never published as it is.
fn vocab(s: &str) -> String {
    let ok = (1..=60).contains(&s.len())
        && s.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b':' | b'_' | b'-')
        });
    if ok {
        s.to_string()
    } else {
        "other".to_string()
    }
}

/// A version as published: left out when unknown, "other" when of another
/// shape than a version or a commit.
fn named(s: &str) -> Option<Value> {
    if s.is_empty() {
        return None;
    }
    let ok = s.len() <= 40
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'+' | b'-'));
    Some(json!(if ok { s } else { "other" }))
}

fn share(part: usize, whole: usize) -> Option<f64> {
    (whole > 0).then(|| round3(part as f64 / whole as f64))
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

/// The median of sorted values: the middle one, or the mean of the two
/// middle ones, rounded to a whole number.
fn median(sorted: &[i64]) -> Option<i64> {
    let n = sorted.len();
    if n == 0 {
        return None;
    }
    if n % 2 == 1 {
        Some(sorted[n / 2])
    } else {
        Some(((sorted[n / 2 - 1] + sorted[n / 2]) as f64 / 2.0).round() as i64)
    }
}

/// The 90th percentile of sorted values by nearest rank: the smallest value
/// at or above which lies 90 % of them.
fn p90(sorted: &[i64]) -> Option<i64> {
    let n = sorted.len();
    (n > 0).then(|| sorted[((n * 9).div_ceil(10)).max(1) - 1])
}

/// The day of days since the epoch, as YYYY-MM-DD (proleptic Gregorian).
fn date(days: i64) -> String {
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

fn obj(pairs: Vec<(&str, Option<Value>)>) -> Value {
    let mut m = Map::new();
    for (k, v) in pairs {
        if let Some(v) = v {
            m.insert(k.to_string(), v);
        }
    }
    Value::Object(m)
}

struct WorkRun {
    id: String,
    issue: i64,
    driver: String,
    started: i64,
    ended: i64,
    seconds: Option<i64>,
    outcome: String,
    tokens: Option<(i64, i64, i64)>,
    failure_kind: Option<String>,
    cap: Option<String>,
    park_reason: Option<String>,
    last_step: Option<String>,
    failed_step: Option<String>,
    tehas: String,
    rukas: String,
    // The run's gate figures as the report reads them. `gate_runs` is the
    // count of the runs of the gate the stream held; `gate_seconds` is their
    // milliseconds on the row brought to seconds, with the whole second a
    // printed duration needs.
    gate_runs: Option<i64>,
    gate_seconds: Option<i64>,
}

/// The two gate figures of one run, bound together: a run that holds one holds
/// both, so the four figures of the `gate` section provably count the same runs.
struct GateFigures {
    runs: i64,
    seconds: i64,
}

impl GateFigures {
    /// One run of the gate of this run: its gate time shared across the runs of
    /// it, the way `spread` reads a figure of a run's whole time: a whole
    /// second, and never none. The count is never 0, so no default is needed.
    fn each(&self) -> i64 {
        ((self.seconds as f64) / self.runs as f64).round() as i64
    }
}

impl WorkRun {
    fn tokens(&self) -> i64 {
        self.tokens.map_or(0, |(i, o, c)| i + o + c)
    }

    /// The gate's time as a share of this run's own time, which is the figure
    /// the report's `share` is the mean of. A run of no recorded length tells
    /// nothing of a share, and a gate that outlasted the run it was in is no
    /// share above 1: the two are read from two clocks, the run's own history
    /// line and the runner's own timings.
    fn gate_share(&self) -> Option<f64> {
        let gate = self.gate_seconds?;
        let secs = self.seconds.filter(|s| *s > 0)?;
        Some(round3((gate as f64 / secs as f64).min(MAX_SHARE)))
    }
    // Why a run that did not reach a pull request stopped: the failure kind,
    // the cap for a failure that is a cap, else the park reason, else how it
    // ended.
    fn stop_reason(&self) -> String {
        let kind = self.failure_kind.as_deref().filter(|k| !k.is_empty());
        let cap = self.cap.as_deref().filter(|k| !k.is_empty());
        let park = self.park_reason.as_deref().filter(|k| !k.is_empty());
        let word = match (kind, cap) {
            (Some("cap"), Some(c)) => c,
            (Some(k), _) => k,
            (None, Some(c)) => c,
            (None, None) => park.unwrap_or(&self.outcome),
        };
        vocab(word)
    }
}

/// The report at `to`.
pub fn build(conn: &Connection, to: i64) -> rusqlite::Result<Value> {
    let today = to.div_euclid(DAY);
    let from = (today - (DAYS - 1)) * DAY;
    let runs = work_runs(conn, from, to)?;
    let shipped = shipped(conn, from, to)?;
    let samples = samples(conn, from, to)?;
    let planning_turns: i64 = conn.query_row(
        "SELECT count(*) FROM runs WHERE kind = 'plan' AND ended >= ?1 AND ended <= ?2",
        params![from, to],
        |r| r.get(0),
    )?;

    let n_shipped = shipped.len();
    let per_shipped = |total: i64| {
        (n_shipped > 0).then(|| json!((total as f64 / n_shipped as f64).round() as i64))
    };

    let mut report = Map::new();
    report.insert("report".into(), json!(REPORT_VERSION));
    report.insert("from".into(), json!(from));
    report.insert("to".into(), json!(to));
    report.insert("shipped".into(), json!({"n": n_shipped}));
    if let Some(p) = planned(&samples, from) {
        report.insert(PLANNED.into(), p);
    }

    let count = |o: &str| runs.iter().filter(|r| r.outcome == o).count();
    let mut split = Map::new();
    split.insert("n".into(), json!(runs.len()));
    for o in [
        "pr", "merged", "handoff", "parked", "failed", "refused", "stopped",
    ] {
        split.insert(o.into(), json!(count(o)));
    }
    report.insert("runs".into(), Value::Object(split));
    report.insert("planningTurns".into(), json!({"n": planning_turns}));
    // Runs that ended needing a person: handed off or parked.
    report.insert(
        "handoffRate".into(),
        obj(vec![
            ("n", Some(json!(runs.len()))),
            (
                "share",
                share(count("handoff") + count("parked"), runs.len()).map(Value::from),
            ),
        ]),
    );

    // Shipped issues with at least one work run here: one whose runs were
    // all pruned says nothing about how many it took.
    let counted: Vec<&Shipped> = shipped.iter().filter(|s| s.runs > 0).collect();
    let first = counted.iter().filter(|s| s.runs == 1).count();
    report.insert(
        "firstPassYield".into(),
        obj(vec![
            ("n", Some(json!(counted.len()))),
            ("share", share(first, counted.len()).map(Value::from)),
        ]),
    );
    let mut lead: Vec<i64> = shipped
        .iter()
        .filter_map(|s| s.lead)
        .filter(|l| *l >= 0)
        .collect();
    lead.sort_unstable();
    report.insert("leadTime".into(), spread(&lead));

    let mut secs: Vec<i64> = runs.iter().filter_map(|r| r.seconds).collect();
    secs.sort_unstable();
    report.insert("cycleTime".into(), spread(&secs));
    let total_secs: i64 = secs.iter().sum();
    report.insert(
        "machineTime".into(),
        obj(vec![
            ("n", Some(json!(secs.len()))),
            ("seconds", Some(json!(total_secs))),
            ("perShipped", per_shipped(total_secs)),
        ]),
    );

    let used: Vec<(i64, i64, i64)> = runs.iter().filter_map(|r| r.tokens).collect();
    let (input, output, cache_read) = used
        .iter()
        .fold((0, 0, 0), |a, t| (a.0 + t.0, a.1 + t.1, a.2 + t.2));
    let total_tokens = input + output + cache_read;
    report.insert(
        "tokens".into(),
        obj(vec![
            ("n", Some(json!(used.len()))),
            ("total", Some(json!(total_tokens))),
            ("input", Some(json!(input))),
            ("output", Some(json!(output))),
            ("cacheRead", Some(json!(cache_read))),
            ("perShipped", per_shipped(total_tokens)),
        ]),
    );

    report.insert("availability".into(), availability(&samples, to));
    if let Some(g) = gate(&runs) {
        report.insert(GATE.into(), g);
    }
    if let Some(e) = energy(&samples, to, n_shipped) {
        report.insert(ENERGY.into(), e);
    }
    report.insert("daily".into(), daily(&runs, &shipped, from));
    report.insert("stopReasons".into(), stop_reasons(&runs));
    report.insert("stations".into(), stations(conn, &runs, from, to)?);
    report.insert("versions".into(), versions(&runs));
    report.insert("drivers".into(), drivers(conn, &runs, to)?);
    Ok(Value::Object(report))
}

/// Each entry is one section whose whole row `text` prints only when the key
/// is there: its name in the report and the label of its row. `text` names its
/// rows from this list and a test holds the list against the keys the report
/// actually publishes, so a section that gains a key without a row, or a row
/// without a producer, fails a test rather than going unprinted. The rows that
/// stand whatever the report holds -- `Shipped`, `Runs`, the times -- come from
/// `build`, which always publishes them, and are not listed here. Each of the
/// five holds an object of its own when it holds anything at all, and prints no
/// row when it does not: `planned` and `energy` with a sample that counted
/// them, `refine` and `sizes` with the ledgers beside the database, in
/// `complete`, and `gate` with a run that recorded the gate.
pub const SECTIONS: [(&str, &str); 5] = [
    (PLANNED, "Planned"),
    (ENERGY, "Energy"),
    (REFINE, "Refine"),
    (SIZES, "Size"),
    (GATE, "Gate"),
];

/// The keys of the five sections of `SECTIONS`, named once so `text` and the
/// report's own writers cannot spell them apart.
const PLANNED: &str = "planned";
const ENERGY: &str = "energy";
pub(crate) const REFINE: &str = "refine";
pub(crate) const SIZES: &str = "sizes";
/// `gate` is the section of a run that recorded the gate: absent with no such
/// run, as the other four are absent with nothing to count, which is what
/// leaves `text` no row to print.
const GATE: &str = "gate";

/// The label of one section's row, the word `text` prints it by. The five
/// labels live in `SECTIONS` alone, so the renderer and the test of the
/// contract read one list. A key of no section has no row, and prints none:
/// this is called only for the five, and the empty word renders a row of a
/// screen the test then finds has no label, which is the failure it is.
pub(crate) fn label(key: &str) -> &'static str {
    SECTIONS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, l)| *l)
        .unwrap_or_default()
}

/// One line of state/refined.jsonl (lib/planning.sh): when a refining session
/// filed an issue, and that issue's number.
#[derive(Debug, Clone, PartialEq)]
pub struct Refined {
    pub at: i64,
    pub filed: i64,
}

fn read_ledger(home: &Path, name: &str) -> Option<String> {
    crate::run::read_ledger(home, name, LEDGER_MAX_BYTES)
}

/// A ledger line's `at` in Unix seconds, or nothing: an ISO time as the writers
/// print it, and only a time the report can answer for. The bound is on the
/// milliseconds, where they are already a number; `iso_ms` itself refuses a
/// year it cannot hold, so a stamp that reaches here is a date and this only
/// asks whether the report keeps that year.
pub(crate) fn ledger_at(v: &Value) -> Option<i64> {
    let ms = v
        .get("at")
        .and_then(Value::as_str)
        .and_then(crate::run::iso_ms)?;
    (LEDGER_AT_MIN..=LEDGER_AT_MAX)
        .contains(&ms)
        .then_some(ms / 1000)
}

/// The ledger of refined issues. A line that is not an object with a whole time
/// and a positive number is skipped, and so is a line of `LEDGER_MAX_LINE`
/// bytes or more; no file is no refined issue.
pub fn read_refined(home: &Path) -> Vec<Refined> {
    ledger_lines(home, REFINED_LEDGER)
        .into_iter()
        .filter_map(|v| {
            let at = ledger_at(&v)?;
            let filed = v.get("filed").and_then(Value::as_i64).filter(|n| *n > 0)?;
            Some(Refined { at, filed })
        })
        .collect()
}

/// The lines of one ledger beside the database, each parsed as a JSON value and
/// none of a shape it cannot be: what the file is not (no file, a link, a
/// device, too long, not text) is no lines at all.
fn ledger_lines(home: &Path, name: &str) -> Vec<Value> {
    let Some(text) = read_ledger(home, name) else {
        return Vec::new();
    };
    text.lines()
        .filter(|l| l.len() < LEDGER_MAX_LINE)
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .collect()
}

/// The refine figures of the window that ends at `to`: refining sessions
/// started (first turns with how `refine`), issues they filed, of those how
/// many started a cycle and how many shipped, with the lead time of the
/// shipped ones from their ready event; and, to compare, the other issues
/// made ready in the window, how many started and shipped.
pub fn refine(conn: &Connection, refined: &[Refined], to: i64) -> rusqlite::Result<Value> {
    let from = (to.div_euclid(DAY) - (DAYS - 1)) * DAY;
    let sessions: i64 = conn.query_row(
        "SELECT count(*) FROM runs WHERE kind = 'plan' AND how = 'refine' AND started >= ?1 AND started <= ?2",
        params![from, to],
        |r| r.get(0),
    )?;
    let filed: BTreeSet<i64> = refined
        .iter()
        .filter(|r| r.at >= from && r.at <= to)
        .map(|r| r.filed)
        .collect();
    let has_event = |issue: i64, event: &str| -> rusqlite::Result<bool> {
        conn.query_row(
            "SELECT count(*) FROM issue_events WHERE issue = ?1 AND event = ?2",
            params![issue, event],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n > 0)
    };
    let lead_of = |issue: i64| -> rusqlite::Result<Option<i64>> {
        let mut st = conn.prepare("SELECT lead_seconds FROM v_lead WHERE issue = ?1")?;
        let mut rows = st.query(params![issue])?;
        Ok(match rows.next()? {
            Some(row) => Some(row.get::<_, Option<i64>>(0)?.unwrap_or(-1)),
            None => None,
        })
    };
    let (mut started, mut shipped, mut lead) = (0usize, 0usize, Vec::new());
    for &issue in &filed {
        if has_event(issue, "started")? {
            started += 1;
        }
        if let Some(l) = lead_of(issue)? {
            shipped += 1;
            if l >= 0 {
                lead.push(l);
            }
        }
    }
    lead.sort_unstable();
    let mut st = conn.prepare(
        "SELECT issue FROM issue_events WHERE event = 'ready' AND at >= ?1 AND at <= ?2",
    )?;
    let ready: Vec<i64> = st
        .query_map(params![from, to], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let others: Vec<i64> = ready.into_iter().filter(|i| !filed.contains(i)).collect();
    let (mut o_started, mut o_shipped) = (0usize, 0usize);
    for &issue in &others {
        if has_event(issue, "started")? {
            o_started += 1;
        }
        if lead_of(issue)?.is_some() {
            o_shipped += 1;
        }
    }
    Ok(obj(vec![
        ("sessions", Some(json!(sessions))),
        ("n", Some(json!(filed.len()))),
        ("started", Some(json!(started))),
        ("shipped", Some(json!(shipped))),
        ("shippedShare", share(shipped, filed.len()).map(Value::from)),
        ("leadTime", Some(spread(&lead))),
        (
            "others",
            Some(obj(vec![
                ("n", Some(json!(others.len()))),
                ("started", Some(json!(o_started))),
                ("shipped", Some(json!(o_shipped))),
                (
                    "shippedShare",
                    share(o_shipped, others.len()).map(Value::from),
                ),
            ])),
        ),
    ]))
}

/// One line of state/sizes.jsonl (lib/sizing.sh): what an approved start
/// measured of its issue's body, keyed with the time and the issue number.
#[derive(Debug, Clone, PartialEq)]
pub struct Size {
    pub at: i64,
    pub issue: i64,
    pub files: i64,
}

/// The ledger of measured starts. The last line of an issue wins, by the
/// `at` it carries, so a line replayed out of order cannot answer for a later
/// one; returned in issue order. Of what the line holds only `at`, `issue`
/// and `files` are read, so nothing of the body's own words can reach the
/// report. A line that is not an object with a whole `at` inside the year the
/// report keeps, a positive whole-numbered `issue` and a whole `files` count of
/// no more than `SIZE_MAX_FILES` is skipped, and a line of a hundred thousand
/// bytes or more is skipped with it, so one pathological line costs the read
/// that line and not the file. A ledger that is a directory, a link or a device,
/// one of another shape than text, or one past `LEDGER_MAX_BYTES`, is no
/// measured issue at all, as no file is.
pub fn read_sizes(home: &Path) -> Vec<Size> {
    let mut latest: BTreeMap<i64, Size> = BTreeMap::new();
    for v in ledger_lines(home, SIZES_LEDGER) {
        let Some(at) = ledger_at(&v) else {
            continue;
        };
        let Some(issue) = whole(&v, "issue").filter(|n| *n > 0) else {
            continue;
        };
        let Some(files) = whole(&v, "files").filter(|n| *n >= 0 && *n <= SIZE_MAX_FILES) else {
            continue;
        };
        match latest.get(&issue) {
            Some(prev) if prev.at >= at => {}
            _ => {
                latest.insert(issue, Size { at, issue, files });
            }
        }
    }
    latest.into_values().collect()
}

/// A field of a ledger line as a whole number: a JSON integer, never a float.
/// The writer's jq prints integers, and a measure of 2.5 files is no measure.
fn whole(v: &Value, key: &str) -> Option<i64> {
    v.get(key).filter(|v| v.is_i64()).and_then(Value::as_i64)
}

/// The size of one issue as a bucket: the file counts 1-2, 3-4 and 5+, or
/// "none" for the bodies that named no file at all. "5+" is the catch-all for
/// every count above 4, however large: the reader admits any whole count up to
/// what `TEHAS_SIZE_MAX_FILES` may be set to, and the bucket says where the
/// threshold pays, not how wide an issue was.
fn size_bucket(files: i64) -> Bucket {
    match files {
        ..=0 => Bucket::None,
        1..=2 => Bucket::Small,
        3..=4 => Bucket::Medium,
        _ => Bucket::Large,
    }
}

/// What one bucket counted: the starts measured in the window, the ships among
/// them, and the lead times of the ships that had one.
#[derive(Default)]
struct BucketCount {
    n: usize,
    shipped: usize,
    leads: Vec<i64>,
}

/// The shipped issues of the ledger, read once: the number to its merge and its
/// lead time. `v_lead` holds one row per shipped issue, keyed by the number
/// alone, which is the earliest merge the issue ever had, matched with its first
/// accepted ready event. The merge is bounded by `from`, the first day of the
/// window, because `sizes` credits only a merge at or after a measured start and
/// a start of the window cannot predate it. The lead is what the row's own
/// window gives, which is why `leadTime.n` can fall short of `shipped`.
fn leads(conn: &Connection, from: i64) -> rusqlite::Result<BTreeMap<i64, (i64, Option<i64>)>> {
    let mut q =
        conn.prepare("SELECT issue, merged, lead_seconds FROM v_lead WHERE merged >= ?1")?;
    let rows = q.query_map(params![from], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))?;
    rows.collect()
}

/// The `sizes` figures of the window that ends at `to`: of the issues whose
/// start was measured in it, by the bucket of its `files` count, how many
/// there were, how many shipped and the median lead time of those.
///
/// Two windows meet in one figure. `n` counts the starts measured in it, from
/// the ledger; a ship is the issue's `v_lead` row, whose merge is the earliest
/// the issue ever had, so only a merge at or after the measured start counts,
/// and an issue whose first ready lies before the window ships without a lead
/// time. `leadTime.n` is that smaller count: where it falls short of
/// `shipped`, a ship brought no lead. The view is read once for the whole
/// ledger, bounded by the window's own first day.
/// Counts only, so the threshold of the dispatcher's hold (`TEHAS_SIZE_MAX_FILES`)
/// can be set from what the outcomes say.
pub fn sizes(conn: &Connection, ledger: &[Size], to: i64) -> rusqlite::Result<Value> {
    let from = (to.div_euclid(DAY) - (DAYS - 1)) * DAY;
    let leads = leads(conn, from)?;
    let mut acc: BTreeMap<Bucket, BucketCount> = SIZE_BUCKETS
        .iter()
        .map(|b| (*b, BucketCount::default()))
        .collect();
    for s in ledger.iter().filter(|s| s.at >= from && s.at <= to) {
        let b = size_bucket(s.files);
        let a = acc
            .get_mut(&b)
            .expect("a bucket of SIZE_BUCKETS, which the map was seeded from");
        a.n += 1;
        if let Some((merged, lead)) = leads.get(&s.issue) {
            // The view's merge is the earliest the issue ever had, so a merge
            // before this measured start belongs to an earlier cycle of it:
            // the window shipped nothing.
            if *merged >= s.at {
                a.shipped += 1;
                // The view's lead is its own `merged - ready`, so it is never
                // negative; a row with no ready behind it holds none at all.
                if let Some(l) = lead {
                    a.leads.push(*l);
                }
            }
        }
    }
    for a in acc.values_mut() {
        a.leads.sort_unstable();
    }
    let row = |b: Bucket| {
        // The leads of each bucket were sorted once, before the rows.
        let a = acc
            .get(&b)
            .expect("a bucket of SIZE_BUCKETS, which the map was seeded from");
        obj(vec![
            ("n", Some(json!(a.n))),
            ("shipped", Some(json!(a.shipped))),
            ("leadTime", Some(spread(&a.leads))),
        ])
    };
    let mut out = Map::new();
    for b in SIZE_BUCKETS {
        out.insert(b.name().to_string(), row(b));
    }
    Ok(Value::Object(out))
}

/// The whole report a person or the dashboard sees: the figures of `build`,
/// and the two sections that come from the ledgers beside the database. The
/// keys of one section are its own; a section that fails is the error, named
/// for itself, so a defect in one ledger says which.
pub fn complete(conn: &Connection, home: &Path, now: i64) -> Result<Value, (u8, String)> {
    let report = build(conn, now).map_err(|e| (70, format!("the report failed: {e}")))?;
    let refine = refine(conn, &read_refined(home), now)
        .map_err(|e| (70, format!("the refine figures failed: {e}")))?;
    let sizes = sizes(conn, &read_sizes(home), now)
        .map_err(|e| (70, format!("the size figures failed: {e}")))?;
    // `build` returns an object on every path; a report that is not one is
    // published as it is rather than ending the command over its shape.
    let Some(o) = report.as_object() else {
        return Ok(report);
    };
    let mut o = o.clone();
    o.insert(REFINE.into(), refine);
    o.insert(SIZES.into(), sizes);
    Ok(Value::Object(o))
}

/// The gate figures of the window: of the `n` work runs that recorded a run of
/// the product's gate, the median and p90 seconds one run of it took, the mean
/// number of them per run, and `share`, the mean of those of the `n` that
/// recorded a length of their own. `n` is the denominator of the count figures;
/// `share` is the mean of the shares of the runs that had one. A run that
/// recorded no gate run is in none of them, as a run without usage is in no
/// token figure, and with no such run the section is absent: the producer
/// returns `None` rather than a count of nothing, which is what leaves `text`
/// no row to print. `seconds` is their gate's whole time, the figure
/// #868's goal is read against.
///
/// One run of the gate of a run that made several is the run's time divided
/// across them, because the stream gives no wall time of its own: what the
/// runner reports is the gate's own milliseconds, and a run of it that waited
/// for another to free the container's CPU share does not count that wait.
fn gate(runs: &[WorkRun]) -> Option<Value> {
    let with: Vec<(&WorkRun, GateFigures)> = runs
        .iter()
        .filter_map(|r| {
            Some((
                r,
                GateFigures {
                    runs: r.gate_runs?,
                    seconds: r.gate_seconds?,
                },
            ))
        })
        .collect();
    let n = with.len();
    if n == 0 {
        return None;
    }
    let mut each: Vec<i64> = with.iter().map(|(_, g)| g.each()).collect();
    each.sort_unstable();
    let gate_seconds: i64 = with.iter().map(|(_, g)| g.seconds).sum();
    let per_run = round3(with.iter().map(|(_, g)| g.runs).sum::<i64>() as f64 / n as f64);
    let shares: Vec<f64> = with.iter().filter_map(|(r, _)| r.gate_share()).collect();
    let share =
        (!shares.is_empty()).then(|| round3(shares.iter().sum::<f64>() / shares.len() as f64));
    Some(obj(vec![
        ("n", Some(json!(n))),
        ("seconds", Some(json!(gate_seconds))),
        ("median", median(&each).map(Value::from)),
        ("p90", p90(&each).map(Value::from)),
        ("perRun", Some(json!(per_run))),
        ("share", share.map(Value::from)),
    ]))
}

fn spread(sorted: &[i64]) -> Value {
    obj(vec![
        ("n", Some(json!(sorted.len()))),
        ("median", median(sorted).map(Value::from)),
        ("p90", p90(sorted).map(Value::from)),
    ])
}

/// One column of the runs table as a whole number: a cell that is not an
/// integer is no figure, which keeps a stray REAL out of the report rather
/// than ending it. The report publishes aggregates, so a column it cannot read
/// leaves its figure absent, as any figure it cannot work out is absent.
fn integer(r: &rusqlite::Row<'_>, n: usize) -> Option<i64> {
    match r.get_ref(n) {
        Ok(rusqlite::types::ValueRef::Integer(v)) => Some(v),
        _ => None,
    }
}

fn work_runs(conn: &Connection, from: i64, to: i64) -> rusqlite::Result<Vec<WorkRun>> {
    let mut q = conn.prepare(
        "SELECT id, started, ended, seconds, coalesce(outcome, 'failed'), input, output, cache_read,
                failure_kind, cap, park_reason, last_step, failed_step,
                coalesce(tehas_version, ''), coalesce(rukas_version, rukas_commit, ''),
                gate_runs, gate_ms, issue, coalesce(driver, 'classic')
         FROM runs WHERE kind = 'work' AND ended >= ?1 AND ended <= ?2 ORDER BY ended",
    )?;
    let rows = q.query_map(params![from, to], |r| {
        let (i, o, c): (Option<i64>, Option<i64>, Option<i64>) = (r.get(5)?, r.get(6)?, r.get(7)?);
        let tokens = (i.is_some() || o.is_some() || c.is_some())
            .then(|| (i.unwrap_or(0), o.unwrap_or(0), c.unwrap_or(0)));
        Ok(WorkRun {
            id: r.get(0)?,
            issue: r.get(17)?,
            driver: r.get(18)?,
            started: r.get(1)?,
            ended: r.get(2)?,
            seconds: r.get::<_, Option<i64>>(3)?.filter(|s| *s >= 0),
            outcome: r.get(4)?,
            tokens,
            failure_kind: r.get(8)?,
            cap: r.get(9)?,
            park_reason: r.get(10)?,
            last_step: r.get(11)?,
            failed_step: r.get(12)?,
            tehas: r.get(13)?,
            rukas: r.get(14)?,
            gate_runs: integer(r, 15).filter(|n| *n > 0 && *n <= crate::run::MAX_GATE_RUNS as i64),
            gate_seconds: integer(r, 16)
                .filter(|m| *m > 0 && *m <= crate::run::MAX_GATE_MS)
                .map(|m| (m / 1000).max(1)),
        })
    })?;
    rows.collect()
}

struct Shipped {
    merged: i64,
    lead: Option<i64>,
    runs: i64,
}

// The issues v_lead counts as shipped whose merge is in the window.
fn shipped(conn: &Connection, from: i64, to: i64) -> rusqlite::Result<Vec<Shipped>> {
    let mut q = conn.prepare(
        "SELECT merged, lead_seconds, runs FROM v_lead WHERE merged >= ?1 AND merged <= ?2",
    )?;
    let rows = q.query_map(params![from, to], |r| {
        Ok(Shipped {
            merged: r.get(0)?,
            lead: r.get(1)?,
            runs: r.get(2)?,
        })
    })?;
    rows.collect()
}

struct Sample {
    at: i64,
    planned: Option<(i64, Option<i64>)>,
    available: bool,
    watts: Option<f64>,
}

fn samples(conn: &Connection, from: i64, to: i64) -> rusqlite::Result<Vec<Sample>> {
    let mut q = conn.prepare(
        "SELECT at, in_line, planning, paused, held, watts, running FROM samples
         WHERE at >= ?1 AND at <= ?2 ORDER BY at",
    )?;
    let rows = q.query_map(params![from, to], |r| {
        let in_line: Option<i64> = r.get(1)?;
        let planning: Option<i64> = r.get(2)?;
        let paused: Option<i64> = r.get(3)?;
        let held: Option<String> = r.get(4)?;
        let running: Option<i64> = r.get(6)?;
        Ok(Sample {
            at: r.get(0)?,
            planned: in_line.map(|n| (n, planning)),
            // Known not to be paused, and either nothing held the next
            // start or cycles were running: full slots are not down time.
            available: paused == Some(0)
                && (held.as_deref().unwrap_or("").is_empty() || running.is_some_and(|n| n > 0)),
            watts: r.get(5)?,
        })
    })?;
    rows.collect()
}

// How long each sample stands for: until the next one, or until `to` for
// the last, at most SAMPLE_SPAN.
fn spans(samples: &[Sample], to: i64) -> impl Iterator<Item = (&Sample, i64)> {
    samples.iter().enumerate().map(move |(i, s)| {
        let next = samples.get(i + 1).map_or(to, |n| n.at);
        (s, (next - s.at).clamp(0, SAMPLE_SPAN))
    })
}

// Planned work now, from the latest sample that counted the line, and one
// value a day, the day's last such sample.
fn planned(samples: &[Sample], from: i64) -> Option<Value> {
    let with: Vec<(i64, i64, Option<i64>)> = samples
        .iter()
        .filter_map(|s| s.planned.map(|(l, p)| (s.at, l, p)))
        .collect();
    let &(_, in_line, planning) = with.last()?;
    let mut per_day: BTreeMap<i64, i64> = BTreeMap::new();
    for (at, l, p) in &with {
        per_day.insert((at - from) / DAY, l + p.unwrap_or(0));
    }
    let trend: Vec<Value> = (0..DAYS)
        .map(|d| {
            obj(vec![
                ("day", Some(json!(date(from / DAY + d)))),
                ("n", per_day.get(&d).map(|n| json!(n))),
            ])
        })
        .collect();
    Some(obj(vec![
        ("n", Some(json!(in_line + planning.unwrap_or(0)))),
        ("inLine", Some(json!(in_line))),
        ("planning", planning.map(Value::from)),
        ("trend", Some(Value::Array(trend))),
    ]))
}

// The share of the time since the first sample in the window that the
// factory could take work. Each sample stands for the time to the next, at
// most SAMPLE_SPAN, so a stretch with no samples counts as not available.
fn availability(samples: &[Sample], to: i64) -> Value {
    let Some(first) = samples.first() else {
        return json!({"n": 0});
    };
    let span = to - first.at;
    let up: i64 = spans(samples, to)
        .filter(|(s, _)| s.available)
        .map(|(_, d)| d)
        .sum();
    obj(vec![
        ("n", Some(json!(samples.len()))),
        ("seconds", Some(json!(span))),
        (
            "share",
            (span > 0).then(|| json!(round3((up as f64 / span as f64).min(1.0)))),
        ),
    ])
}

// kWh over the window: each sample's watts for the time it stands for.
// Left out where no sample has watts.
fn energy(samples: &[Sample], to: i64, shipped: usize) -> Option<Value> {
    let n = samples.iter().filter(|s| s.watts.is_some()).count();
    if n == 0 {
        return None;
    }
    let joules: f64 = spans(samples, to)
        .filter_map(|(s, d)| s.watts.map(|w| w * d as f64))
        .sum();
    let kwh = joules / 3_600_000.0;
    Some(obj(vec![
        ("n", Some(json!(n))),
        ("kwh", Some(json!(round3(kwh)))),
        (
            "perShipped",
            (shipped > 0).then(|| json!(round3(kwh / shipped as f64))),
        ),
    ]))
}

fn daily(runs: &[WorkRun], shipped: &[Shipped], from: i64) -> Value {
    let mut days: Vec<(i64, i64, i64, i64)> = vec![(0, 0, 0, 0); DAYS as usize];
    let at = |t: i64| {
        usize::try_from((t - from) / DAY)
            .ok()
            .filter(|d| *d < DAYS as usize)
    };
    for s in shipped {
        if let Some(d) = at(s.merged) {
            days[d].0 += 1;
        }
    }
    for r in runs {
        if let Some(d) = at(r.ended) {
            days[d].1 += 1;
            days[d].2 += i64::from(r.outcome == "failed");
            days[d].3 += r.seconds.unwrap_or(0);
        }
    }
    Value::Array(
        days.iter()
            .enumerate()
            .map(|(d, (shipped, runs, failed, secs))| {
                json!({"day": date(from / DAY + d as i64), "shipped": shipped, "runs": runs,
                       "failed": failed, "minutes": (*secs as f64 / 60.0).round() as i64})
            })
            .collect(),
    )
}

// The runs that did not reach a pull request, by why they stopped, the five
// most common. A run a person stopped by closing the issue is not counted.
fn stop_reasons(runs: &[WorkRun]) -> Value {
    let mut by: BTreeMap<String, usize> = BTreeMap::new();
    let mut n = 0;
    for r in runs {
        if matches!(r.outcome.as_str(), "pr" | "merged" | "stopped") {
            continue;
        }
        n += 1;
        *by.entry(r.stop_reason()).or_default() += 1;
    }
    let mut top: Vec<(String, usize)> = by.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    top.truncate(STOP_REASONS);
    json!({"n": n, "top": top.into_iter().map(|(reason, n)| json!({"reason": reason, "n": n})).collect::<Vec<_>>()})
}

// Per step: of the runs that reached it, the share that got through it. A
// run that reached a pull request got through every step it ran; a run
// handed off, parked or failed stopped in its failed step, else its last;
// refused and stopped runs are not the line's doing and are left out.
// Steps are in the order runs reach them: by their mean start in the run.
fn stations(conn: &Connection, runs: &[WorkRun], from: i64, to: i64) -> rusqlite::Result<Value> {
    let mut q = conn.prepare(
        "SELECT s.run_id, s.step, s.started FROM run_steps s JOIN runs r ON r.id = s.run_id
         WHERE r.kind = 'work' AND r.ended >= ?1 AND r.ended <= ?2",
    )?;
    let mut reached: BTreeMap<&str, BTreeMap<String, i64>> = BTreeMap::new();
    let by_id: BTreeMap<&str, &WorkRun> = runs.iter().map(|r| (r.id.as_str(), r)).collect();
    let rows = q.query_map(params![from, to], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<i64>>(2)?,
        ))
    })?;
    for row in rows {
        let (id, step, started) = row?;
        let Some((id, run)) = by_id.get_key_value(id.as_str()) else {
            continue;
        };
        let offset = started.map_or(0, |s| (s - run.started).max(0));
        let at = reached
            .entry(id)
            .or_default()
            .entry(vocab(&step))
            .or_insert(offset);
        *at = (*at).min(offset);
    }
    // step -> (reached, passed, sum of offsets)
    let mut steps: BTreeMap<String, (usize, usize, i64)> = BTreeMap::new();
    let mut n = 0;
    for (id, ran) in &reached {
        let run = by_id[id];
        let stop = match run.outcome.as_str() {
            "pr" | "merged" => None,
            "handoff" | "parked" | "failed" => Some(
                run.failed_step
                    .as_deref()
                    .or(run.last_step.as_deref())
                    .map(vocab)
                    .unwrap_or_else(|| {
                        ran.iter()
                            .max_by_key(|(_, o)| **o)
                            .map(|(s, _)| s.clone())
                            .unwrap_or_default()
                    }),
            ),
            _ => continue,
        };
        n += 1;
        for (step, offset) in ran {
            let e = steps.entry(step.clone()).or_default();
            e.0 += 1;
            e.1 += usize::from(stop.as_deref() != Some(step.as_str()));
            e.2 += offset;
        }
    }
    let mut order: Vec<(String, (usize, usize, i64))> = steps.into_iter().collect();
    order.sort_by(|a, b| {
        let mean = |e: &(usize, usize, i64)| e.2 as f64 / e.0 as f64;
        mean(&a.1)
            .total_cmp(&mean(&b.1))
            .then_with(|| a.0.cmp(&b.0))
    });
    order.truncate(STEPS);
    let list: Vec<Value> = order
        .into_iter()
        .map(|(step, (reached, passed, _))| {
            json!({"step": step, "n": reached, "passed": passed,
                   "share": round3(passed as f64 / reached as f64)})
        })
        .collect();
    Ok(json!({"n": n, "steps": list}))
}

// One row per tehas and pi-rukas version pair, the pair that ran last
// first. A pair with VERSION_FLOOR runs or more also has the share that
// reached a pull request and the median seconds, and the median tokens of the
// runs that reported usage.
fn versions(runs: &[WorkRun]) -> Value {
    let mut pairs: Vec<((String, String), Vec<&WorkRun>)> = Vec::new();
    for r in runs {
        let key = (r.tehas.clone(), r.rukas.clone());
        match pairs.iter_mut().find(|(k, _)| *k == key) {
            Some((_, v)) => v.push(r),
            None => pairs.push((key, vec![r])),
        }
    }
    let last = |v: &[&WorkRun]| v.iter().map(|r| r.ended).max().unwrap_or(0);
    pairs.sort_by(|a, b| last(&b.1).cmp(&last(&a.1)).then_with(|| a.0.cmp(&b.0)));
    pairs.truncate(VERSIONS);
    Value::Array(
        pairs
            .into_iter()
            .map(|((tv, rv), v)| {
                let n = v.len();
                let enough = n >= VERSION_FLOOR;
                let pr = v
                    .iter()
                    .filter(|r| matches!(r.outcome.as_str(), "pr" | "merged"))
                    .count();
                let mut secs: Vec<i64> = v.iter().map(|r| r.seconds.unwrap_or(0)).collect();
                secs.sort_unstable();
                let mut tokens = used_tokens(&v);
                tokens.sort_unstable();
                let pick = |o: Option<Value>| if enough { o } else { None };
                obj(vec![
                    ("tehas", named(&tv)),
                    ("rukas", named(&rv)),
                    ("n", Some(json!(n))),
                    ("prShare", pick(share(pr, n).map(Value::from))),
                    ("medianSeconds", pick(median(&secs).map(Value::from))),
                    ("medianTokens", pick(median(&tokens).map(Value::from))),
                ])
            })
            .collect(),
    )
}

/// Each issue's last merge up to `to`: a merged event, or a work run that
/// ended merged, the two `v_lead` reads. The view keeps the first, and a merge
/// before a run started is of an earlier cycle of the issue.
fn last_merges(conn: &Connection, to: i64) -> rusqlite::Result<BTreeMap<i64, i64>> {
    let mut q = conn.prepare(
        "SELECT issue, max(at) FROM (
           SELECT issue, at FROM issue_events WHERE event = 'merged'
           UNION ALL
           SELECT issue, ended FROM runs WHERE kind = 'work' AND outcome = 'merged'
         ) WHERE at <= ?1 GROUP BY issue",
    )?;
    let rows = q.query_map(params![to], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

/// The tokens of the runs that reported usage: the rest have no figure, not 0.
fn used_tokens(runs: &[&WorkRun]) -> Vec<i64> {
    runs.iter()
        .filter(|r| r.tokens.is_some())
        .map(|r| r.tokens())
        .collect()
}

/// `n`, the sum and the median of sorted values in units of `per`, the last two
/// left out with no value to work them from: nothing reported is no figure, not 0.
fn tally(sorted: &[i64], per: i64) -> Value {
    let whole = |x: i64| json!((x as f64 / per as f64).round() as i64);
    obj(vec![
        ("n", Some(json!(sorted.len()))),
        (
            "total",
            (!sorted.is_empty()).then(|| whole(sorted.iter().sum())),
        ),
        ("median", median(sorted).map(whole)),
    ])
}

/// The runs of each driver in the window. A run that reported no usage is in no
/// token figure and one of no length in no minutes figure. A pull request counts
/// once per issue, because a merge is on record per issue and the runs that
/// follow up one pull request are many, and as merged when the issue's last
/// merge is at or after the first run of the driver that reached it.
fn drivers(conn: &Connection, runs: &[WorkRun], to: i64) -> rusqlite::Result<Value> {
    let merges = last_merges(conn, to)?;
    let mut out = Map::new();
    for driver in DRIVERS {
        let of: Vec<&WorkRun> = runs.iter().filter(|r| r.driver == driver).collect();
        let n = of.len();
        let reached: Vec<&&WorkRun> = of
            .iter()
            .filter(|r| matches!(r.outcome.as_str(), "pr" | "merged"))
            .collect();
        let mut pulls: BTreeMap<i64, i64> = BTreeMap::new();
        for r in &reached {
            let first = pulls.entry(r.issue).or_insert(r.started);
            *first = (*first).min(r.started);
        }
        let merged = pulls
            .iter()
            .filter(|(issue, first)| merges.get(issue).is_some_and(|m| m >= first))
            .count();
        let parked = of.iter().filter(|r| r.outcome == "parked").count();
        let mut secs: Vec<i64> = of.iter().filter_map(|r| r.seconds).collect();
        secs.sort_unstable();
        let mut tokens = used_tokens(&of);
        tokens.sort_unstable();
        out.insert(
            driver.to_string(),
            obj(vec![
                ("n", Some(json!(n))),
                ("prShare", share(reached.len(), n).map(Value::from)),
                ("parkedShare", share(parked, n).map(Value::from)),
                ("minutes", Some(tally(&secs, 60))),
                ("tokens", Some(tally(&tokens, 1))),
                (
                    "pullRequests",
                    Some(obj(vec![
                        ("n", Some(json!(pulls.len()))),
                        ("merged", Some(json!(merged))),
                        ("share", share(merged, pulls.len()).map(Value::from)),
                    ])),
                ),
            ]),
        );
    }
    Ok(Value::Object(out))
}

/// The report as `analytics report` prints it for a person: the figures of
/// `build`, one to a line, then the days, the stop reasons, the steps, the
/// versions and the drivers. A figure that is left out of the JSON is shown as "-".
pub fn text(report: &Value) -> String {
    let int = |v: &Value| v.as_i64();
    let at = |path: &[&str]| path.iter().fold(report, |v, k| &v[*k]).clone();
    let n = |key: &str| format!("(n={})", int(&report[key]["n"]).unwrap_or(0));
    let pct = |v: &Value| v.as_f64().map_or("-".to_string(), percent);
    let dur = |v: &Value| int(v).map_or("-".to_string(), duration);
    let num = |v: &Value| int(v).map_or("-".to_string(), |x| x.to_string());
    // A figure rounded to three places, printed as the three it holds: an f64
    // of its own would print 1 as "1" and a small mean as 0.
    let dec = |v: &Value| v.as_f64().map_or("-".to_string(), |x| format!("{x:.3}"));
    let from = int(&report["from"]).unwrap_or(0);
    let to = int(&report["to"]).unwrap_or(0);

    let mut lines = vec![
        format!(
            "Last {DAYS} days, {} to {} (UTC)",
            date(from.div_euclid(DAY)),
            date(to.div_euclid(DAY))
        ),
        String::new(),
    ];
    let mut row = |name: &str, value: String, count: String| {
        let line = format!("{name:<18} {value}");
        lines.push(if count.is_empty() {
            line
        } else {
            format!("{line}  {count}")
        });
    };
    row(
        "Shipped",
        format!("{} issues", num(&at(&["shipped", "n"]))),
        String::new(),
    );
    // The five conditional rows take their labels from `SECTIONS`, so the
    // renderer and the test of the contract read one list: a section that gains
    // a key without a row, or a row without a section, fails a test.
    let planned = &report[PLANNED];
    row(
        label(PLANNED),
        if planned.is_object() {
            format!(
                "{} now: {} in line, {} in planning",
                num(&planned["n"]),
                num(&planned["inLine"]),
                num(&planned["planning"])
            )
        } else {
            "-".to_string()
        },
        String::new(),
    );
    let runs = &report["runs"];
    let split: Vec<String> = [
        "pr", "merged", "handoff", "parked", "failed", "refused", "stopped",
    ]
    .iter()
    .map(|o| format!("{} {o}", num(&runs[*o])))
    .collect();
    row(
        "Runs",
        format!("{}: {}", num(&runs["n"]), split.join(", ")),
        String::new(),
    );
    row(
        "Planning turns",
        num(&at(&["planningTurns", "n"])),
        String::new(),
    );
    let refine = &report[REFINE];
    if refine.as_object().is_some() {
        row(
            label(REFINE),
            format!(
                "{} sessions, {} issues filed: {} started, {} shipped ({}), lead median {}; other issues {} shipped (n={})",
                num(&refine["sessions"]),
                num(&refine["n"]),
                num(&refine["started"]),
                num(&refine["shipped"]),
                pct(&refine["shippedShare"]),
                dur(&refine["leadTime"]["median"]),
                pct(&refine["others"]["shippedShare"]),
                num(&refine["others"]["n"]),
            ),
            String::new(),
        );
    }
    let sizes = &report[SIZES];
    if let Some(s) = sizes.as_object() {
        // Each bucket is read as the three counts `sizes` publishes; a bucket
        // that is not an object of them prints as nothing rather than a row of
        // dashes, so producer and consumer cannot drift unseen.
        let bucket = |b: Bucket| {
            let e = &s[b.name()];
            format!(
                "{}: {} shipped {}, lead {}",
                b.name(),
                num(&e["n"]),
                num(&e["shipped"]),
                dur(&e["leadTime"]["median"]),
            )
        };
        row(
            label(SIZES),
            SIZE_BUCKETS
                .iter()
                .map(|b| bucket(*b))
                .collect::<Vec<_>>()
                .join(" | "),
            String::new(),
        );
    }
    row(
        "Handoff rate",
        pct(&at(&["handoffRate", "share"])),
        n("handoffRate"),
    );
    row(
        "First-pass yield",
        pct(&at(&["firstPassYield", "share"])),
        n("firstPassYield"),
    );
    for (name, key) in [("Lead time", "leadTime"), ("Cycle time", "cycleTime")] {
        row(
            name,
            format!(
                "median {}, p90 {}",
                dur(&report[key]["median"]),
                dur(&report[key]["p90"])
            ),
            n(key),
        );
    }
    let per = |v: &Value, f: &dyn Fn(&Value) -> String| match v {
        Value::Null => String::new(),
        v => format!(", {} per shipped issue", f(v)),
    };
    let machine = &report["machineTime"];
    row(
        "Machine time",
        format!(
            "{}{}",
            dur(&machine["seconds"]),
            per(&machine["perShipped"], &dur)
        ),
        n("machineTime"),
    );
    let tokens = &report["tokens"];
    row(
        "Tokens",
        format!(
            "{}: {} input, {} output, {} cache read{}",
            num(&tokens["total"]),
            num(&tokens["input"]),
            num(&tokens["output"]),
            num(&tokens["cacheRead"]),
            per(&tokens["perShipped"], &num)
        ),
        n("tokens"),
    );
    let gate = &report[GATE];
    if gate.is_object() {
        // The row of the four figures `gate` works out. Its whole time is not
        // one of them: it is read from the same object for the count.
        row(
            label(GATE),
            format!(
                "median {}, p90 {}, {} runs per run, {} of run time",
                dur(&gate["median"]),
                dur(&gate["p90"]),
                dec(&gate["perRun"]),
                pct(&gate["share"]),
            ),
            format!(
                "(n={}, {})",
                int(&gate["n"]).unwrap_or(0),
                dur(&gate["seconds"])
            ),
        );
    }
    let avail = &report["availability"];
    row(
        "Availability",
        match int(&avail["seconds"]) {
            Some(s) => format!("{} of {}", pct(&avail["share"]), duration(s)),
            None => "-".to_string(),
        },
        n("availability"),
    );
    let energy = &report[ENERGY];
    if energy.is_object() {
        let kwh = |v: &Value| v.as_f64().map_or("-".to_string(), |k| format!("{k} kWh"));
        row(
            label(ENERGY),
            format!(
                "{}{}",
                kwh(&energy["kwh"]),
                per(&energy["perShipped"], &kwh)
            ),
            n(ENERGY),
        );
    }

    lines.push(String::new());
    lines.push(format!(
        "{:<12}{:>8}{:>6}{:>8}{:>9}",
        "Day", "Shipped", "Runs", "Failed", "Minutes"
    ));
    for d in report["daily"].as_array().into_iter().flatten() {
        lines.push(format!(
            "{:<12}{:>8}{:>6}{:>8}{:>9}",
            d["day"].as_str().unwrap_or("-"),
            num(&d["shipped"]),
            num(&d["runs"]),
            num(&d["failed"]),
            num(&d["minutes"])
        ));
    }

    lines.push(String::new());
    lines.push(format!("Stop reasons  {}", n("stopReasons")));
    let top = report["stopReasons"]["top"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if top.is_empty() {
        lines.push("  none".to_string());
    }
    for r in &top {
        lines.push(format!(
            "  {:<30}{:>5}",
            r["reason"].as_str().unwrap_or("-"),
            num(&r["n"])
        ));
    }

    lines.push(String::new());
    lines.push(format!("Steps passed  {}", n("stations")));
    let steps = report["stations"]["steps"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if steps.is_empty() {
        lines.push("  none".to_string());
    }
    for s in &steps {
        lines.push(format!(
            "  {:<30}{:>5} of {:<5}{:>6}",
            s["step"].as_str().unwrap_or("-"),
            num(&s["passed"]),
            num(&s["n"]),
            pct(&s["share"])
        ));
    }

    lines.push(String::new());
    lines.push(format!(
        "Versions  (fewer than {VERSION_FLOOR} runs: the count only)"
    ));
    let versions = report["versions"].as_array().cloned().unwrap_or_default();
    if versions.is_empty() {
        lines.push("  none".to_string());
    } else {
        lines.push(format!(
            "  {:<20}{:<20}{:>5}{:>9}{:>8}{:>10}",
            "tehas", "pi-rukas", "runs", "to a PR", "median", "tokens"
        ));
    }
    for v in &versions {
        let name = |k: &str| v[k].as_str().unwrap_or("-").to_string();
        let mut line = format!(
            "  {:<20}{:<20}{:>5}",
            name("tehas"),
            name("rukas"),
            num(&v["n"])
        );
        if v["prShare"].is_number() {
            line.push_str(&format!(
                "{:>9}{:>8}{:>10}",
                pct(&v["prShare"]),
                dur(&v["medianSeconds"]),
                num(&v["medianTokens"])
            ));
        }
        lines.push(line);
    }

    lines.push(String::new());
    lines.push("Drivers".to_string());
    lines.push(format!(
        "  {:<10}{:>5}{:>9}{:>8}{:>10}{:>14}{:>5}{:>8}",
        "", "runs", "to a PR", "parked", "minutes", "tokens", "PRs", "merged"
    ));
    for driver in DRIVERS {
        let d = &report["drivers"][driver];
        let pulls = &d["pullRequests"];
        // A count short of the runs says the tokens are not all of them.
        let tokens = match (int(&d["tokens"]["n"]), int(&d["n"])) {
            (Some(some), Some(all)) if some < all => {
                format!("{} (n={some})", num(&d["tokens"]["total"]))
            }
            _ => num(&d["tokens"]["total"]),
        };
        lines.push(format!(
            "  {:<10}{:>5}{:>9}{:>8}{:>10}{:>14}{:>5}{:>8}",
            driver,
            num(&d["n"]),
            pct(&d["prShare"]),
            pct(&d["parkedShare"]),
            num(&d["minutes"]["total"]),
            tokens,
            num(&pulls["n"]),
            num(&pulls["merged"])
        ));
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// A share as a percentage: whole where it is whole, else one decimal.
fn percent(share: f64) -> String {
    let p = (share * 1000.0).round() / 10.0;
    if p.fract() == 0.0 {
        format!("{p:.0}%")
    } else {
        format!("{p:.1}%")
    }
}

/// Seconds for a person: 45s, 12m30s, 3h05m, 2d04h.
fn duration(secs: i64) -> String {
    let s = secs.max(0);
    match s {
        0..=59 => format!("{s}s"),
        60..=3599 if s % 60 == 0 => format!("{}m", s / 60),
        60..=3599 => format!("{}m{:02}s", s / 60, s % 60),
        3600..=86_399 => format!("{}h{:02}m", s / 3600, s % 3600 / 60),
        _ => format!("{}d{:02}h", s / DAY, s % DAY / 3600),
    }
}
