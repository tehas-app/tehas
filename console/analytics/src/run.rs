//! `analytics run <issue> --facts <file>`: one work run, from the facts its
//! caller gives and what the cycle left on the host, and its insert.
//!
//! The facts are one JSON object that lib/analytics.sh writes for the call, a
//! claim read as the files below are:
//!
//! - line: tehas's line of history of this run, written by libexec/cycle
//!   just before this program is called: at, started, rc, status, step, cap,
//!   park_reason, failure_kind, failure_cause, provider, parked, pr, how.
//!   tehas's own view of the run: its status says killed, stalled or timeout
//!   where tehas stopped it. The run is not recorded without it.
//! - attempt: how many runs of the issue there have been, this one with them.
//! - approval: {at, how} of the approval the run started on, or null: how,
//!   and at, when tehas:ready was accepted (ISO 8601).
//! - settings: {container: {PI_ENSEMBLE_*: value}, host: {TEHAS_*: value},
//!   concurrentCycles} as libexec/work snapshot them for this run, values
//!   already reduced to safe words, or null. The model and provider of each
//!   role, and how many cycles held their lock as this one started, this one
//!   with them.
//! - rukas: the pi-rukas commit of the image's tehas.rukas label as the cycle
//!   started, or null.
//!
//! Every file is read as tehas wrote it, and none through a link. Where a
//! file is missing its columns are NULL. Under $TEHAS_HOME:
//!
//! - state/cycles/<n>.result.json (libexec/work): the headless runner's raw
//!   result, schema 3 or 4, {schemaVersion, buildVersion, buildCommit,
//!   exitCode, issues: [{issue, status, durationMs, steps: [{step,
//!   startedAt, endedAt}], exitCode, usage: {input, output, cacheRead,
//!   cacheWrite, cost, turns, dispatches, byRole: {<role>: {...}}}, cap,
//!   failedStep, parkReason, failure: {kind, step, role, cause}}]}. Read
//!   whole: lib/headless.sh reads schema 4 only, but the fields read here are
//!   the same in both. libexec/cycle removes it as the next cycle starts.
//! - state/cycles/<n>.progress.json (libexec/work, tehas_headless_progress):
//!   {roles: [{role, dispatches, turns, input, output, cacheRead}], reason,
//!   failure, reviewHeld, from}. reviewHeld, and the roles where the result
//!   has no byRole (v0.12.80).
//!
//! The image, its tehas.rukas commit and the counts of processes killed for
//! memory and of refused forks are given on the command line (Given):
//! libexec/cycle reads the image and its commit from the cycle's record on
//! the state store, and reads and removes .oom and .forks, so they are not
//! read here.
//! - logs/work-<n>-<stamp>.jsonl (libexec/work): the runner's NDJSON progress,
//!   the newest one written since the run started. Its stable step-started
//!   lines give the steps where the result has none.
//! - logs/work-<n>-<stamp>.jsonl.preflight (libexec/work): the push check's
//!   log, with a line "model endpoint: provider <p> is on its backup" for
//!   each provider that was on its backup endpoint.
//! - docs/version.txt (install.sh): one line, "vX.Y.Z+<sha7>" or "<sha7>".
//!   `analytics plan` reads it too.

use regex::Regex;
use rusqlite::{params, Connection};
use serde_json::{Map, Value};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::UNIX_EPOCH;
use tehas_core::grammar::is_hex;
use tehas_core::mask;
use tehas_core::time::days_from_civil;

// A file bigger than this is not read: none that tehas writes comes near it.
const MAX_FILE: u64 = 16 << 20;
// The progress stream can be long; only its step lines are read.
const MAX_STREAM: u64 = 256 << 20;
const CAUSE_CHARS: usize = 200;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Run {
    pub id: String,
    pub issue: i64,
    pub attempt: Option<i64>,
    pub how: Option<String>,
    pub started: i64,
    pub ended: Option<i64>,
    pub seconds: Option<i64>,
    pub queue_wait: Option<i64>,
    pub outcome: Option<String>,
    pub status: Option<String>,
    pub exit_code: Option<i64>,
    pub last_step: Option<String>,
    pub failed_step: Option<String>,
    pub cap: Option<String>,
    pub park_reason: Option<String>,
    pub review_held: Option<bool>,
    pub failure_kind: Option<String>,
    pub failure_cause: Option<String>,
    pub provider_outage: Option<bool>,
    pub endpoint: Option<String>,
    pub pr: Option<i64>,
    pub tehas_version: Option<String>,
    pub tehas_commit: Option<String>,
    pub rukas_version: Option<String>,
    pub rukas_commit: Option<String>,
    pub image: Option<String>,
    pub input: Option<i64>,
    pub output: Option<i64>,
    pub cache_read: Option<i64>,
    pub cache_write: Option<i64>,
    pub turns: Option<i64>,
    pub dispatches: Option<i64>,
    pub cost: Option<f64>,
    pub oom_kills: Option<i64>,
    pub forks_refused: Option<i64>,
    pub concurrent: Option<i64>,
    pub runner_ms: Option<i64>,
    pub result_schema: Option<i64>,
    pub steps: Vec<Step>,
    pub roles: Vec<Role>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Step {
    pub step: String,
    pub round: i64,
    pub started: Option<i64>,
    pub ended: Option<i64>,
    pub seconds: Option<i64>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Role {
    pub role: String,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub dispatches: Option<i64>,
    pub turns: Option<i64>,
    pub input: Option<i64>,
    pub output: Option<i64>,
    pub cache_read: Option<i64>,
    pub cache_write: Option<i64>,
    pub cost: Option<f64>,
    pub ms: Option<i64>,
}

/// The result schemas whose fields this reads.
const RESULT_SCHEMAS: [i64; 2] = [3, 4];

/// What libexec/cycle passes on: the facts file, the image the container
/// started from, the pi-rukas commit of its label, and its counts of kills and
/// refused forks.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Given {
    pub image: Option<String>,
    pub rukas_image: Option<String>,
    pub oom: Option<i64>,
    pub forks: Option<i64>,
    pub facts: Option<PathBuf>,
}

/// The facts file, read as a host file is: a JSON value, not through a link,
/// bounded. None when it is not there or not of that shape.
pub(crate) fn read_facts(given: Option<&Path>) -> Option<Value> {
    read_json(given?)
}

/// Reads the run of `issue` that ended last. Err when its facts cannot be
/// read or hold no history line.
pub fn read(home: &Path, issue: i64, given: &Given) -> Result<Run, String> {
    let cycles = home.join("state").join("cycles");
    let note = |ext: &str| cycles.join(format!("{issue}.{ext}"));

    let facts = read_facts(given.facts.as_deref())
        .and_then(|f| match f {
            Value::Object(o) => Some(o),
            _ => None,
        })
        .ok_or_else(|| format!("no facts of the run of issue {issue}, so no run to record"))?;
    let last = facts
        .get("line")
        .and_then(Value::as_object)
        .ok_or_else(|| format!("no history of issue {issue}, so no run to record"))?;
    let started = last
        .get("started")
        .and_then(Value::as_i64)
        .filter(|s| *s > 0)
        .ok_or_else(|| format!("the last history line of issue {issue} has no start"))?;

    let mut run = Run {
        id: format!("work-{issue}-{started}"),
        issue,
        started,
        attempt: facts
            .get("attempt")
            .and_then(Value::as_i64)
            .filter(|n| *n > 0),
        ..Run::default()
    };
    run.ended = text(last, "at")
        .and_then(|at| iso_ms(&at))
        .map(|ms| ms / 1000);
    run.seconds = run.ended.map(|e| (e - started).max(0));
    run.status = word(last, "status", 40);
    run.exit_code = last.get("rc").and_then(Value::as_i64);
    run.last_step = word(last, "step", 80).filter(|s| s != "-");
    run.cap = word(last, "cap", 80);
    run.park_reason = word(last, "park_reason", 40);
    run.failure_kind = word(last, "failure_kind", 40);
    run.failure_cause = text(last, "failure_cause").map(|c| cause(&c));
    run.provider_outage = last.get("provider").and_then(Value::as_bool);
    run.pr = text(last, "pr").and_then(|u| pr_number(&u));
    run.how = word(last, "how", 40);
    let parked = last.get("parked").and_then(Value::as_bool).unwrap_or(false);

    if let Some(approved) = facts.get("approval").and_then(Value::as_object) {
        if let Some(how) = word(approved, "how", 40) {
            run.how = Some(how);
        }
        if let Some(at) = approved.get("at").and_then(Value::as_str).and_then(iso_ms) {
            let wait = started - at / 1000;
            run.queue_wait = (wait >= 0).then_some(wait);
        }
    }

    let result = read_json(&note("result.json"));
    let schema = result
        .as_ref()
        .and_then(|r| r.get("schemaVersion"))
        .and_then(Value::as_i64);
    let result = result.filter(|_| schema.is_some_and(|s| RESULT_SCHEMAS.contains(&s)));
    let entry = result.as_ref().and_then(|r| {
        r.get("issues")?
            .as_array()?
            .iter()
            .find(|e| e.get("issue").and_then(Value::as_i64) == Some(issue))
            .and_then(Value::as_object)
    });
    if let Some(r) = &result {
        run.result_schema = schema;
        run.rukas_version = r
            .get("buildVersion")
            .and_then(Value::as_str)
            .and_then(plain40);
        run.rukas_commit = r
            .get("buildCommit")
            .and_then(Value::as_str)
            .filter(|c| is_hex(c, 7, 40))
            .map(str::to_string);
        if run.exit_code.is_none() {
            run.exit_code = r.get("exitCode").and_then(Value::as_i64);
        }
    }
    if let Some(e) = entry {
        read_entry(&mut run, e);
    }

    let progress = read_json(&note("progress.json"));
    if let Some(p) = &progress {
        run.review_held = p.get("reviewHeld").and_then(Value::as_bool);
    }
    let settings = facts.get("settings");
    let container = settings
        .and_then(|s| s.get("container"))
        .and_then(Value::as_object);
    if let Some(n) = settings
        .and_then(|s| s.get("concurrentCycles"))
        .and_then(Value::as_i64)
    {
        run.concurrent = Some((n - 1).max(0));
    }
    if run.roles.is_empty() {
        if let Some(roles) = progress
            .as_ref()
            .and_then(|p| p.get("roles"))
            .and_then(Value::as_array)
        {
            run.roles = roles
                .iter()
                .filter_map(Value::as_object)
                .filter_map(|r| role(word(r, "role", 80)?, r))
                .collect();
        }
    }
    for r in &mut run.roles {
        let suffix: String = r
            .role
            .chars()
            .map(|c| {
                if c == '-' {
                    '_'
                } else {
                    c.to_ascii_uppercase()
                }
            })
            .collect();
        let pick = |own: &str, all: &str| {
            container.and_then(|c| {
                c.get(&format!("{own}{suffix}"))
                    .or_else(|| c.get(all))
                    .and_then(Value::as_str)
                    .and_then(plain_model)
            })
        };
        r.model = pick("PI_ENSEMBLE_MODEL_", "PI_ENSEMBLE_SUBAGENT_MODEL");
        r.provider = pick("PI_ENSEMBLE_PROVIDER_", "PI_ENSEMBLE_SUBAGENT_PROVIDER");
    }

    // The image's label as the container started, else as the cycle began.
    if run.rukas_commit.is_none() {
        run.rukas_commit = given.rukas_image.clone().or_else(|| {
            facts
                .get("rukas")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|c| is_hex(c, 7, 40))
                .map(str::to_string)
        });
    }
    run.image = given.image.clone();
    run.oom_kills = given.oom;
    run.forks_refused = given.forks;

    (run.tehas_version, run.tehas_commit) = tehas_version(home);

    let stream = work_log(home, issue, started);
    if let Some(log) = &stream {
        let preflight = PathBuf::from(format!("{}.preflight", log.display()));
        if let Some(text) = read_plain(&preflight, MAX_FILE) {
            let backup = text.lines().any(|l| {
                l.starts_with("model endpoint: provider ") && l.ends_with(" is on its backup")
            });
            run.endpoint = Some(if backup { "backup" } else { "main" }.to_string());
        }
        if run.steps.is_empty() {
            run.steps = stream_steps(log, issue, run.ended);
        }
    }

    run.outcome = Some(outcome(&run, parked).to_string());
    Ok(run)
}

/// The tehas version and commit of docs/version.txt.
pub fn tehas_version(home: &Path) -> (Option<String>, Option<String>) {
    let Some(v) = first_line(&home.join("docs").join("version.txt")) else {
        return (None, None);
    };
    let (version, commit) = match v.split_once('+') {
        Some((tag, sha)) => (Some(tag), sha),
        None => (None, v.as_str()),
    };
    (
        version.and_then(plain40),
        Some(commit)
            .filter(|c| is_hex(c, 7, 40))
            .map(str::to_string),
    )
}

/// The issue's entry of the result: what tehas_headless_outcome drops too.
fn read_entry(run: &mut Run, e: &Map<String, Value>) {
    if run.status.is_none() || run.status.as_deref() == Some("unknown") {
        run.status = word(e, "status", 40).or(run.status.take());
    }
    if let Some(code) = e.get("exitCode").and_then(Value::as_i64) {
        run.exit_code = Some(code);
    }
    run.runner_ms = e
        .get("durationMs")
        .and_then(Value::as_i64)
        .filter(|n| *n >= 0);
    run.failed_step = word(e, "failedStep", 80);
    if run.cap.is_none() {
        run.cap = word(e, "cap", 80);
    }
    if run.park_reason.is_none() {
        run.park_reason = word(e, "parkReason", 40);
    }
    if let Some(f) = e.get("failure").and_then(Value::as_object) {
        if run.failure_kind.is_none() {
            run.failure_kind = word(f, "kind", 40);
        }
        // The history's cause is reduced already; the raw one only without it.
        if run.failure_cause.is_none() {
            run.failure_cause = text(f, "cause").map(|c| cause(&c));
        }
    }
    if let Some(u) = e.get("usage").and_then(Value::as_object) {
        run.input = number(u, "input");
        run.output = number(u, "output");
        run.cache_read = number(u, "cacheRead");
        run.cache_write = number(u, "cacheWrite");
        run.turns = number(u, "turns");
        run.dispatches = number(u, "dispatches");
        run.cost = u.get("cost").and_then(Value::as_f64).filter(|c| *c >= 0.0);
        if let Some(by) = u.get("byRole").and_then(Value::as_object) {
            run.roles = by
                .iter()
                .filter_map(|(name, v)| {
                    let name = plain(name, 80).unwrap_or_else(|| "driver".to_string());
                    role(name, v.as_object()?)
                })
                .collect();
        }
    }
    if let Some(steps) = e.get("steps").and_then(Value::as_array) {
        let mut seen: Vec<String> = Vec::new();
        for s in steps.iter().filter_map(Value::as_object) {
            let Some(name) = word(s, "step", 80) else {
                continue;
            };
            seen.push(name.clone());
            let round = seen.iter().filter(|n| **n == name).count() as i64;
            let from = text(s, "startedAt").and_then(|t| iso_ms(&t));
            let to = text(s, "endedAt").and_then(|t| iso_ms(&t));
            run.steps.push(Step {
                step: name,
                round,
                started: from.map(|ms| ms / 1000),
                ended: to.map(|ms| ms / 1000),
                seconds: from.zip(to).map(|(a, b)| ((b - a) / 1000).max(0)),
            });
        }
        if let Some(last) = run.steps.last() {
            run.last_step = Some(last.step.clone());
        }
    }
}

fn role(name: String, r: &Map<String, Value>) -> Option<Role> {
    Some(Role {
        role: name,
        dispatches: number(r, "dispatches"),
        turns: number(r, "turns"),
        input: number(r, "input"),
        output: number(r, "output"),
        cache_read: number(r, "cacheRead"),
        cache_write: number(r, "cacheWrite"),
        cost: r.get("cost").and_then(Value::as_f64).filter(|c| *c >= 0.0),
        ms: number(r, "ms").or_else(|| number(r, "durationMs")),
        ..Role::default()
    })
}

/// How the run ended, in the words of the runs table:
/// merged    the issue's pull request was merged
/// stopped   the issue was closed while it ran, or it was interrupted
/// refused   no cycle ran: the runner refused (40), or tehas work started
///           none (3 cannot push, 4 attachments, 5 already running, 64, 78)
/// pr        it left an open pull request and was not parked
/// handoff   the runner handed off to a person
/// failed    tehas stopped or lost it (killed, stalled, timeout), the runner
///           crashed or aborted, or a failure other than a cap stopped it
/// parked    any other end that needs a person, such as a cap
fn outcome(run: &Run, parked: bool) -> &'static str {
    let status = run.status.as_deref().unwrap_or("unknown");
    let kind = run.failure_kind.as_deref().unwrap_or("");
    match status {
        "merged" => return "merged",
        "closed" | "interrupted" => return "stopped",
        "refused" => return "refused",
        _ => {}
    }
    if matches!(run.exit_code, Some(3 | 4 | 5 | 40 | 64 | 78)) {
        return "refused";
    }
    if run.pr.is_some() && !parked {
        return "pr";
    }
    if status == "handoff" {
        return "handoff";
    }
    if matches!(
        status,
        "killed" | "stalled" | "timeout" | "crash" | "aborted" | "unknown" | "failed"
    ) || (!kind.is_empty() && kind != "cap")
    {
        return "failed";
    }
    "parked"
}

/// Inserts the run with its steps and roles, in one transaction. A run
/// recorded again replaces what was recorded of it.
pub fn insert(conn: &Connection, run: &Run) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM runs WHERE id = ?1", [&run.id])?;
    tx.execute(
        "INSERT INTO runs (id, kind, issue, attempt, how, started, ended, seconds, queue_wait,
           outcome, status, exit_code, last_step, failed_step, cap, park_reason, review_held,
           failure_kind, failure_cause, provider_outage, endpoint, pr, tehas_version,
           tehas_commit, rukas_version, rukas_commit, image, input, output, cache_read,
           cache_write, turns, dispatches, cost, oom_kills, forks_refused, concurrent,
           runner_ms, result_schema)
         VALUES (?1, 'work', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
           ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32,
           ?33, ?34, ?35, ?36, ?37, ?38)",
        params![
            run.id,
            run.issue,
            run.attempt,
            run.how,
            run.started,
            run.ended,
            run.seconds,
            run.queue_wait,
            run.outcome,
            run.status,
            run.exit_code,
            run.last_step,
            run.failed_step,
            run.cap,
            run.park_reason,
            run.review_held,
            run.failure_kind,
            run.failure_cause,
            run.provider_outage,
            run.endpoint,
            run.pr,
            run.tehas_version,
            run.tehas_commit,
            run.rukas_version,
            run.rukas_commit,
            run.image,
            run.input,
            run.output,
            run.cache_read,
            run.cache_write,
            run.turns,
            run.dispatches,
            run.cost,
            run.oom_kills,
            run.forks_refused,
            run.concurrent,
            run.runner_ms,
            run.result_schema,
        ],
    )?;
    for s in &run.steps {
        tx.execute(
            "INSERT OR REPLACE INTO run_steps (run_id, step, round, started, ended, seconds)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![run.id, s.step, s.round, s.started, s.ended, s.seconds],
        )?;
    }
    for r in &run.roles {
        tx.execute(
            "INSERT OR REPLACE INTO run_roles (run_id, role, model, provider, dispatches, turns,
               input, output, cache_read, cache_write, cost, ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                run.id,
                r.role,
                r.model,
                r.provider,
                r.dispatches,
                r.turns,
                r.input,
                r.output,
                r.cache_read,
                r.cache_write,
                r.cost,
                r.ms
            ],
        )?;
    }
    tx.commit()
}

/// The steps from the stable step-started lines of the progress stream: each
/// step ends where the next begins, the last where the run ended.
fn stream_steps(log: &Path, issue: i64, ended: Option<i64>) -> Vec<Step> {
    let Some(file) = plain_file(log) else {
        return Vec::new();
    };
    let mut starts: Vec<(String, i64)> = Vec::new();
    for line in BufReader::new(file.take(MAX_STREAM)).lines() {
        let Ok(line) = line else { break };
        let Ok(Value::Object(o)) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let stable = o.get("v").and_then(Value::as_i64) == Some(1)
            && !o.get("unstable").and_then(Value::as_bool).unwrap_or(false);
        if !stable || o.get("kind").and_then(Value::as_str) != Some("step-started") {
            continue;
        }
        if o.get("issue")
            .and_then(Value::as_i64)
            .is_some_and(|n| n != issue)
        {
            continue;
        }
        let Some(ev) = o.get("event").and_then(Value::as_object) else {
            continue;
        };
        let (Some(step), Some(at)) = (word(ev, "step", 80), ev.get("at").and_then(Value::as_i64))
        else {
            continue;
        };
        starts.push((step, at));
    }
    let mut steps = Vec::new();
    for (i, (step, at)) in starts.iter().enumerate() {
        let round = starts[..=i].iter().filter(|(s, _)| s == step).count() as i64;
        let to = starts
            .get(i + 1)
            .map(|(_, next)| *next)
            .or(ended.map(|e| e * 1000));
        steps.push(Step {
            step: step.clone(),
            round,
            started: Some(at / 1000),
            ended: to.map(|t| t / 1000),
            seconds: to.map(|t| ((t - at) / 1000).max(0)),
        });
    }
    steps
}

/// The newest logs/work-<issue>-*.jsonl written since the run started, as
/// lib/improve.sh finds a cycle's log.
fn work_log(home: &Path, issue: i64, started: i64) -> Option<PathBuf> {
    let prefix = format!("work-{issue}-");
    let mut found: Vec<PathBuf> = fs::read_dir(home.join("logs"))
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&prefix) && n.ends_with(".jsonl"))
        })
        .filter(|p| written_since(p, started))
        .collect();
    found.sort();
    found.pop()
}

fn written_since(p: &Path, started: i64) -> bool {
    fs::symlink_metadata(p)
        .ok()
        .filter(|m| m.is_file())
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .is_some_and(|d| d.as_secs() as i64 >= started - 5)
}

fn plain_file(p: &Path) -> Option<File> {
    let meta = fs::symlink_metadata(p).ok()?;
    if !meta.is_file() {
        return None;
    }
    File::open(p).ok()
}

/// A ledger beside the database: `state/<name>`, read by the one rule this
/// program has for a host file it did not write -- never through a link, never
/// a device or a fifo, and none over `limit` bytes. Both state ledgers are read
/// here, so "a ledger is a bounded plain file" is held in one place.
pub(crate) fn read_ledger(home: &Path, name: &str, limit: u64) -> Option<String> {
    read_plain(&home.join("state").join(name), limit)
}

/// A plain file's text, never through a link, and none over `limit` bytes. The
/// one way this program reads a host file it did not write.
fn read_plain(p: &Path, limit: u64) -> Option<String> {
    let file = plain_file(p)?;
    if file.metadata().ok()?.len() > limit {
        return None;
    }
    let mut s = String::new();
    file.take(limit).read_to_string(&mut s).ok()?;
    Some(s)
}

pub(crate) fn read_json(p: &Path) -> Option<Value> {
    serde_json::from_str(&read_plain(p, MAX_FILE)?).ok()
}

fn first_line(p: &Path) -> Option<String> {
    let s = read_plain(p, 4096)?;
    let line = s.lines().next()?.trim().to_string();
    (!line.is_empty()).then_some(line)
}

fn text(o: &Map<String, Value>, key: &str) -> Option<String> {
    o.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// A field as a word of the published vocabularies: [A-Za-z0-9_.:-], cut.
fn word(o: &Map<String, Value>, key: &str, max: usize) -> Option<String> {
    plain(o.get(key)?.as_str()?, max)
}

fn plain(s: &str, max: usize) -> Option<String> {
    let w: String = s
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-'))
        .take(max)
        .collect();
    (!w.is_empty()).then_some(w)
}

fn plain40(s: &str) -> Option<String> {
    plain(s, 40)
}

// A model or provider as libexec/work's settings snapshot holds it, which may
// be "[withheld]".
fn plain_model(s: &str) -> Option<String> {
    if s == "[withheld]" {
        return Some(s.to_string());
    }
    plain(s, 80)
}

fn number(o: &Map<String, Value>, key: &str) -> Option<i64> {
    o.get(key).and_then(Value::as_i64).filter(|n| *n >= 0)
}

fn pattern(p: &str) -> Regex {
    Regex::new(p).expect("a pattern of the cause's mask is not a regular expression")
}

// What the console's mask leaves and a failure cause may hold: the secret
// shapes lib/improve.sh knows (tehas_improve_secret), JWTs, addresses with a
// user, the host of a URL, IPv4 and IPv6 addresses, and host names. So no
// column of the database names a host or holds a token.
static PRIVATE_KEY: LazyLock<Regex> = LazyLock::new(|| {
    pattern(r"-----BEGIN [A-Z ]*PRIVATE KEY-----.*?(?:-----END [A-Z ]*PRIVATE KEY-----|$)")
});
static API_KEY: LazyLock<Regex> = LazyLock::new(|| {
    pattern(
        r"(^|[^A-Za-z0-9])(sk-)[A-Za-z0-9_-]{6,}|(xox[abprs]-)[A-Za-z0-9-]{4,}|(AKIA)[0-9A-Z]{16}",
    )
});
static JWT: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"eyJ[A-Za-z0-9_-]{4,}(?:\.[A-Za-z0-9_-]+)*"));
static URL_HOST: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?i)\b([a-z][a-z0-9+.-]*://)[^/\s]+"));
static USER_AT_HOST: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*"));
static IPV4: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"\b[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\b"));
static IPV6: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?i)\b[0-9a-f]{1,4}(?::[0-9a-f]{0,4}){2,7}\b"));
static HOST_NAME: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"\b[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+(?::[0-9]+)?\b"));

// The last part of a file name: a dotted word ending in one of these is
// a file, not a host.
const FILE_EXTENSIONS: [&str; 30] = [
    "json", "jsonl", "md", "txt", "log", "yaml", "yml", "toml", "lock", "html", "css", "js", "mjs",
    "ts", "tsx", "rs", "sh", "py", "go", "sql", "db", "gz", "zip", "tar", "png", "jpg", "svg",
    "csv", "diff", "patch",
];

/// Whether a dotted word is a host name: it has a port, or its last part is
/// letters that are no file extension. A version such as v0.7.22 ends in
/// digits and is kept.
fn is_host(found: &str) -> bool {
    if found.contains(':') {
        return true;
    }
    let last = found.rsplit('.').next().unwrap_or("");
    last.len() >= 2
        && last.bytes().all(|b| b.is_ascii_alphabetic())
        && !FILE_EXTENSIONS.contains(&last.to_ascii_lowercase().as_str())
}

/// An IPv6 address, not a time: two non-empty groups at least, and :: or
/// three colons. 18:38:44 has two.
fn is_ipv6(found: &str) -> bool {
    let groups = found.split(':').filter(|g| !g.is_empty()).count();
    groups >= 2 && (found.contains("::") || found.matches(':').count() >= 3)
}

/// The failure's cause masked as the console masks and as above, then cut,
/// so a cut never leaves part of a secret the mask would have known whole.
pub fn cause(c: &str) -> String {
    let one_line: String = c
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    let s = PRIVATE_KEY.replace_all(&one_line, "[private key]");
    let s = API_KEY.replace_all(&s, "${1}${2}${3}${4}***");
    let s = JWT.replace_all(&s, "[jwt]");
    let s = mask::mask(&s);
    let s = URL_HOST.replace_all(&s, "${1}[host]");
    let s = USER_AT_HOST.replace_all(&s, "[address]");
    let s = IPV4.replace_all(&s, "[address]");
    let s = IPV6.replace_all(&s, |m: &regex::Captures| {
        if is_ipv6(&m[0]) {
            "[address]".to_string()
        } else {
            m[0].to_string()
        }
    });
    let s = HOST_NAME.replace_all(&s, |m: &regex::Captures| {
        if is_host(&m[0]) {
            "[host]".to_string()
        } else {
            m[0].to_string()
        }
    });
    s.chars().take(CAUSE_CHARS).collect()
}

/// The number of a pull request from its address, .../pull/<n>.
fn pr_number(url: &str) -> Option<i64> {
    let (_, n) = url.trim_end_matches('/').rsplit_once("/pull/")?;
    n.parse().ok().filter(|n: &i64| *n > 0)
}

/// Milliseconds since the epoch of an ISO 8601 time in UTC, as GitHub and
/// the runner write them: 2026-10-15T18:38:44Z or 2026-10-15T18:38:44.024Z.
pub fn iso_ms(s: &str) -> Option<i64> {
    let s = s.strip_suffix('Z')?;
    let (date, time) = s.split_once('T')?;
    let mut d = date.splitn(3, '-').map(|p| p.parse::<i64>().ok());
    let (y, mo, da) = (d.next()??, d.next()??, d.next()??);
    let (hms, frac) = match time.split_once('.') {
        Some((hms, frac)) => (hms, frac),
        None => (time, ""),
    };
    let mut t = hms.splitn(3, ':').map(|p| p.parse::<i64>().ok());
    let (h, mi, se) = (t.next()??, t.next()??, t.next()??);
    // The year is bounded before the arithmetic is asked to hold it: an
    // unbounded one overflows the days-to-milliseconds below, and no host file
    // of tehas' own making carries a stamp outside these two.
    if !(1970..=2100).contains(&y)
        || !(1..=12).contains(&mo)
        || !(1..=31).contains(&da)
        || h > 23
        || mi > 59
        || se > 60
    {
        return None;
    }
    if !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let ms = format!("{frac:0<3}")[..3].parse::<i64>().ok()?;
    let days = days_from_civil(y, mo, da);
    Some(((days * 24 + h) * 60 + mi) * 60_000 + se * 1000 + ms)
}
