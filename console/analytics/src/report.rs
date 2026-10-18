//! `analytics report --json`: the ten-day report the dashboard shows, as one
//! JSON object. libexec/status writes it to live/analytics.json, which is
//! public, so it holds aggregates only: counts, durations, token sums,
//! version strings and the vocabulary words of stop reasons and steps. No
//! issue number, title, failure cause, account, model or provider.
//!
//! The window is ten UTC days: from the midnight nine days before `to`'s
//! day, to `to`, the time of the report. Prune keeps ten days back from now,
//! so all of it is in the database.
//!
//! Every figure carries its count `n`. A value that cannot be worked out,
//! such as a share of nothing, is left out, never null. Work runs (kind
//! work) that ended in the window make the run figures; planning turns are
//! only counted, in planningTurns. Durations are seconds.

use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

pub const REPORT_VERSION: i64 = 1;
pub const DAYS: i64 = 10;
const DAY: i64 = 86_400;
/// The longest stretch one sample stands for. A longer gap between samples
/// is a dispatcher that was not polling: it is not counted as available,
/// nor as one long draw of the sample's watts.
pub const SAMPLE_SPAN: i64 = 600;
/// A version pair with fewer work runs shows its count only.
pub const VERSION_FLOOR: usize = 3;
const STOP_REASONS: usize = 5;
const STEPS: usize = 12;
const VERSIONS: usize = 10;

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
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
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
}

impl WorkRun {
    fn tokens(&self) -> i64 {
        self.tokens.map_or(0, |(i, o, c)| i + o + c)
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
        report.insert("planned".into(), p);
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
    if let Some(e) = energy(&samples, to, n_shipped) {
        report.insert("energy".into(), e);
    }
    report.insert("daily".into(), daily(&runs, &shipped, from));
    report.insert("stopReasons".into(), stop_reasons(&runs));
    report.insert("stations".into(), stations(conn, &runs, from, to)?);
    report.insert("versions".into(), versions(&runs));
    Ok(Value::Object(report))
}

fn spread(sorted: &[i64]) -> Value {
    obj(vec![
        ("n", Some(json!(sorted.len()))),
        ("median", median(sorted).map(Value::from)),
        ("p90", p90(sorted).map(Value::from)),
    ])
}

fn work_runs(conn: &Connection, from: i64, to: i64) -> rusqlite::Result<Vec<WorkRun>> {
    let mut q = conn.prepare(
        "SELECT id, started, ended, seconds, coalesce(outcome, 'failed'), input, output, cache_read,
                failure_kind, cap, park_reason, last_step, failed_step,
                coalesce(tehas_version, ''), coalesce(rukas_version, rukas_commit, '')
         FROM runs WHERE kind = 'work' AND ended >= ?1 AND ended <= ?2 ORDER BY ended",
    )?;
    let rows = q.query_map(params![from, to], |r| {
        let (i, o, c): (Option<i64>, Option<i64>, Option<i64>) = (r.get(5)?, r.get(6)?, r.get(7)?);
        let tokens = (i.is_some() || o.is_some() || c.is_some())
            .then(|| (i.unwrap_or(0), o.unwrap_or(0), c.unwrap_or(0)));
        Ok(WorkRun {
            id: r.get(0)?,
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
        "SELECT at, in_line, planning, paused, held, watts FROM samples
         WHERE at >= ?1 AND at <= ?2 ORDER BY at",
    )?;
    let rows = q.query_map(params![from, to], |r| {
        let in_line: Option<i64> = r.get(1)?;
        let planning: Option<i64> = r.get(2)?;
        let paused: Option<i64> = r.get(3)?;
        let held: Option<String> = r.get(4)?;
        Ok(Sample {
            at: r.get(0)?,
            planned: in_line.map(|n| (n, planning)),
            // Could take work: known not to be paused, and nothing held it.
            available: paused == Some(0) && held.as_deref().unwrap_or("").is_empty(),
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
// reached a pull request and the median seconds and tokens.
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
                let mut tokens: Vec<i64> = v.iter().map(|r| r.tokens()).collect();
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
