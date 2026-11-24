//! What tehas reads of pi-rukas's headless runner: the issue's entry of the
//! result (`tehas-cycle runner outcome`), the steps of a run log
//! (`tehas-cycle runner run`), the fields of the progress summary, and the
//! table of token usage. Each is read into its type here, at the edge, by the
//! rules of `crate::loose`.
//!
//! Where jq would stop on a value of the wrong type, these stop too: an
//! outcome jq cannot make is none, and a progress jq stops in gives the fields
//! it had printed before it stopped.

use crate::loose::{self, Field, Loose, Many, Node};
use serde::{Deserialize, Serialize};
use serde_json::Number;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// The result schema this reads.
pub const RESULT_SCHEMA: u64 = 5;

/// One JSON value of a result file, as tehas reads it. A file a re-run
/// appended to holds more than one.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RunResult {
    pub schema_version: Loose<Number>,
    pub build_commit: Loose<String>,
    pub build_version: Loose<String>,
    pub issues: Many<Node<Entry>>,
    /// The runner's word on the comments it was given, for the run as a whole.
    pub comments: Node<Comments>,
    pub error: Node<RunError>,
}

impl RunResult {
    /// Whether the value names the schema this reads, as jq compares numbers.
    pub fn schema_5(&self) -> bool {
        self.schema_version.get().and_then(Number::as_f64) == Some(RESULT_SCHEMA as f64)
    }
}

/// The error of a result that holds one.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct RunError {
    pub code: Loose<String>,
}

/// An issue's entry of a result: the fields tehas reads of it.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Entry {
    pub issue: Loose<u64>,
    pub status: Loose<String>,
    pub cap: Loose<String>,
    pub failed_step: Loose<String>,
    pub park_reason: Loose<String>,
    pub pr_state: Loose<String>,
    pub branch: Loose<String>,
    pub pr_url: Loose<String>,
    pub steps: Many<Node<Step>>,
    pub handoff: Node<Handoff>,
    pub resumable: Loose<bool>,
    pub exit_code: Loose<Number>,
    pub usage: Node<EntryUsage>,
    pub failure: Node<EntryFailure>,
    pub comments: Node<Comments>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Step {
    pub step: Loose<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Handoff {
    pub body: Loose<String>,
    pub channel: Loose<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EntryUsage {
    pub input: Loose<Number>,
    pub output: Loose<Number>,
    pub cache_read: Loose<Number>,
    pub turns: Loose<Number>,
    pub dispatches: Loose<Number>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EntryFailure {
    pub step: Loose<String>,
    pub message: Loose<String>,
    pub provider_message: Loose<String>,
    pub cause: Loose<String>,
    pub error_tail: Loose<String>,
}

/// What the runner says of the comments a comments retry gave it.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Comments {
    pub read: Loose<bool>,
    pub count: Loose<u64>,
    pub dropped: Loose<u64>,
    pub reason: Loose<String>,
}

/// Every JSON value of a result's text, each an object or not, or None when
/// one cannot be parsed.
pub fn results(text: &str) -> Option<Vec<Node<RunResult>>> {
    loose::values(text)
}

/// The issue's entry of one result value: the first object of its `issues`,
/// a list or an object, whose `issue` is that whole number. Every reader of
/// an entry goes through here, so they agree on the entry they describe.
pub fn issue_entry(result: &Node<RunResult>, issue: u64) -> Option<&Entry> {
    result
        .object()?
        .issues
        .items()
        .iter()
        .filter_map(Node::object)
        .find(|e| e.issue.get() == Some(&issue))
}

/// The issue's entry in the last of a result's values that has one: a file
/// a re-run appended to describes the newest run.
pub fn last_issue_entry(results: &[Node<RunResult>], issue: u64) -> Option<&Entry> {
    results.iter().rev().find_map(|r| issue_entry(r, issue))
}

/// A string with only the bytes of `keep`, as gsub("[^...]"; "") leaves it.
pub fn kept(s: &str, keep: fn(char) -> bool) -> String {
    s.chars().filter(|c| keep(*c)).collect()
}

/// The first `n` characters.
pub fn cut(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// The most characters of the reason a cycle ended with that its end keeps,
/// as many as the condensed stream keeps of a text (#718).
const REASON_MOST: usize = 2000;
const REASON_CUT: &str = " (cut)";

/// A reason of at most REASON_MOST characters, ending ` (cut)` when it was
/// longer. Cutting one already cut changes nothing.
pub fn cut_reason(s: &str) -> String {
    if s.chars().nth(REASON_MOST).is_none() {
        return s.to_string();
    }
    cut(s, REASON_MOST - REASON_CUT.len()) + REASON_CUT
}

/// What a cycle's record says of a cap the driver gave no words for.
const NO_EVIDENCE: &str = "no evidence from the driver";

/// The reason a record gives for its cap. A reason that is empty or only
/// repeats the cap word is no evidence, and says so. Memory kills in the
/// container are said, with the killed processes' names where they are known,
/// so a memory plan that is too tight is not read as a failed change.
pub fn cap_reason(cap: &str, reason: &str, kills: u64, names: &[String]) -> String {
    if cap.is_empty() {
        return reason.to_string();
    }
    let said = reason.trim();
    let word = cap.split(':').next().unwrap_or(cap);
    let echo = said.is_empty() || said.eq_ignore_ascii_case(cap) || said.eq_ignore_ascii_case(word);
    let mut out = if echo {
        NO_EVIDENCE.to_string()
    } else {
        said.to_string()
    };
    if kills > 0 && !out.contains("killed for memory") {
        out += &if kills == 1 {
            "; 1 process in its container was killed for memory".to_string()
        } else {
            format!("; {kills} processes in its container were killed for memory")
        };
        if !names.is_empty() {
            out += &format!(" ({})", names.join(", "));
        }
    }
    cut_reason(&out)
}

fn plain(v: &Loose<String>) -> String {
    kept(v.str(), |c| {
        c.is_ascii_alphanumeric() || "_.:/-".contains(c)
    })
}

/// `n`: a number of at least 0, else 0.
fn n(v: Option<&Number>) -> Number {
    match v {
        Some(x) if x.as_f64().is_some_and(|f| f >= 0.0) => x.clone(),
        _ => Number::from(0),
    }
}

/// The issue's entry of a schema 5 result, as `tehas-cycle runner outcome`
/// prints it: its keys in this order, which is the sorted order of the jq it
/// replaced.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub branch: String,
    pub build_commit: String,
    pub build_version: String,
    pub cap: String,
    pub exit_code: Option<Number>,
    pub failed_step: String,
    pub handoff_body: String,
    pub handoff_channel: String,
    pub last_step: String,
    pub park_reason: String,
    pub pr_state: String,
    pub pr_url: String,
    pub resumable: bool,
    pub status: String,
    pub usage: Usage,
}

/// Token usage: the five counts, each a number of at least 0.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub cache_read: Number,
    pub dispatches: Number,
    pub input: Number,
    pub output: Number,
    pub turns: Number,
}

impl Default for Usage {
    fn default() -> Self {
        let zero = || Number::from(0);
        Usage {
            cache_read: zero(),
            dispatches: zero(),
            input: zero(),
            output: zero(),
            turns: zero(),
        }
    }
}

impl Usage {
    /// The counts in the order of the usage table.
    fn five(&self) -> [&Number; 5] {
        [
            &self.dispatches,
            &self.turns,
            &self.input,
            &self.output,
            &self.cache_read,
        ]
    }
}

/// The issue's entry of a schema 5 result, its text fields reduced to plain
/// characters, its exitCode null unless a number. handoffBody is the handoff's
/// markdown as the runner rendered it, cut at 30000 characters and not
/// cleaned: it is for a comment, through q_clean. None when there is none.
pub fn outcome(text: &str, issue: u64) -> Option<Outcome> {
    let r = results(text)?;
    // jq -e answers for its last output; a file of several results has the
    // entry of each, and the last is the one read.
    r.iter().rev().find_map(|r| outcome_of(r, issue))
}

fn outcome_of(r: &Node<RunResult>, issue: u64) -> Option<Outcome> {
    let o = r.object()?;
    if !o.schema_5() {
        return None;
    }
    let e = issue_entry(r, issue)?;
    // `.handoff.body?` of a handoff that is no object makes no value, and so
    // no object at all; a usage that is no object stops jq.
    if e.handoff.hides() {
        return None;
    }
    let usage = match &e.usage {
        Node::Absent | Node::Null | Node::False => EntryUsage::default(),
        Node::Object(u) => u.clone(),
        Node::Other => return None,
    };
    let last_step = match e.steps.items().iter().rfind(|s| !s.hides()) {
        Some(Node::Object(s)) => plain(&s.step),
        _ => String::new(),
    };
    let handoff = e.handoff.object().cloned().unwrap_or_default();
    Some(Outcome {
        status: plain(&e.status),
        cap: plain(&e.cap),
        failed_step: plain(&e.failed_step),
        last_step,
        park_reason: plain(&e.park_reason),
        handoff_body: cut(handoff.body.str(), 30000),
        handoff_channel: plain(&handoff.channel),
        pr_state: plain(&e.pr_state),
        branch: plain(&e.branch),
        pr_url: plain(&e.pr_url),
        resumable: e.resumable.get() == Some(&true),
        exit_code: e.exit_code.get().cloned(),
        usage: Usage {
            input: n(usage.input.get()),
            output: n(usage.output.get()),
            cache_read: n(usage.cache_read.get()),
            turns: n(usage.turns.get()),
            dispatches: n(usage.dispatches.get()),
        },
        build_commit: plain(&o.build_commit),
        build_version: plain(&o.build_version),
    })
}

/// The one grammar for a branch pi-rukas makes for an issue: feature/issue-<n>
/// with an optional slug of up to 80 characters.
pub fn branch_of(issue: u64, b: &str) -> bool {
    let rest = match b.strip_prefix(&format!("feature/issue-{issue}")) {
        Some(r) => r,
        None => return false,
    };
    rest.is_empty()
        || rest.strip_prefix('-').is_some_and(|s| {
            (1..=80).contains(&s.len())
                && s.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        })
}

/// The most of a run log tehas reads. Of a longer one it reads its end, which
/// is where the step a runner died in and the branch of its last push are.
pub const LOG_TAIL: u64 = 200_000;

/// What the step-started lines of a run log say of the run.
#[derive(Debug, Default, PartialEq)]
pub struct Run {
    pub step: String,
    pub branch: String,
    /// `whole`, or `truncated` for a log over `LOG_TAIL`. Said by whoever read
    /// the log, which is the only one that knows whether it held all of it.
    pub read: String,
}

fn step_name(s: &str) -> bool {
    (1..=100).contains(&s.len())
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_./:-".contains(&c))
}

/// One line of a runner's stream: a JSON object of version 1. The fields are
/// those tehas reads of it.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Line {
    pub v: Loose<f64>,
    pub kind: Loose<String>,
    pub unstable: Field,
    pub key: Field,
    pub event: Node<Event>,
}

/// The event of a stream's line.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Event {
    pub step: Loose<String>,
    pub branch: Loose<String>,
    pub role: Field,
    pub usage: Node<EventUsage>,
    pub evidence: Loose<String>,
    pub cap: Loose<String>,
    pub next_step: Loose<String>,
    pub provider_message: Loose<String>,
    pub error_tail: Loose<String>,
}

/// The usage of a dispatch, each count as the runner wrote it.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EventUsage {
    pub turns: Field,
    pub input: Field,
    pub output: Field,
    pub cache_read: Field,
}

impl Line {
    /// The line, when it is a JSON object with v 1.
    pub fn parse(l: &str) -> Option<Line> {
        let line = serde_json::from_str::<Node<Line>>(l).ok()?.into_object()?;
        (line.v.get() == Some(&1.0)).then_some(line)
    }

    /// Not marked unstable: no `unstable`, or null or false.
    pub fn stable(&self) -> bool {
        self.unstable.nothing()
    }

    /// Marked unstable, with `true` itself.
    pub fn unstable(&self) -> bool {
        self.unstable == Field::Bool(true)
    }

    pub fn is(&self, kind: &str) -> bool {
        self.kind.get().map(String::as_str) == Some(kind)
    }

    /// The event, when it is an object.
    pub fn event(&self) -> Option<&Event> {
        self.event.object()
    }
}

/// The stable lines of a stream: each line that is a JSON object with v 1
/// and not marked unstable.
pub(crate) fn stable(text: &str) -> impl Iterator<Item = Line> + '_ {
    text.split('\n')
        .filter_map(Line::parse)
        .filter(Line::stable)
}

/// A run log at `path`, or None when `issue` is not a number or the path is no
/// regular file. A log whose runner died is all tehas has of the step it was
/// in and the branch it pushed. Of a log longer than `LOG_TAIL` only its last
/// `LOG_TAIL` bytes are read, and `read` says `truncated` for that.
pub fn run_log(path: &Path, issue: &str) -> Option<Run> {
    if !issue.starts_with(|c: char| ('1'..='9').contains(&c))
        || !issue.bytes().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    let n = issue.parse().ok()?;
    let (bytes, whole) = log_tail(path)?;
    let mut r = run(&String::from_utf8_lossy(&bytes), n);
    r.read = read_word(whole).to_string();
    Some(r)
}

/// The bytes of the run log at `path` to read, and whether they are the whole
/// of it: None when it is no regular file or cannot be read, and of a log over
/// `LOG_TAIL` only its last `LOG_TAIL`, sought rather than read past.
pub fn log_tail(path: &Path) -> Option<(Vec<u8>, bool)> {
    let m = path.symlink_metadata().ok()?;
    if !m.is_file() {
        return None;
    }
    if m.len() <= LOG_TAIL {
        return Some((std::fs::read(path).ok()?, true));
    }
    let mut f = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    f.seek(SeekFrom::Start(m.len() - LOG_TAIL)).ok()?;
    f.take(LOG_TAIL).read_to_end(&mut bytes).ok()?;
    Some((bytes, false))
}

/// How a run log was read, in the one word the report and the helper's third
/// line say it in: `whole`, or `truncated` for a log read only from its end.
pub fn read_word(whole: bool) -> &'static str {
    if whole {
        "whole"
    } else {
        "truncated"
    }
}

/// The last step a log's text says was started and the branch of its last
/// commit-pr when it is the issue's. Of a text that is the end of a longer
/// log, this finds only what that end says: `Run::read` is said by whoever
/// read the log.
pub fn run(text: &str, issue: u64) -> Run {
    let mut steps = vec![];
    for v in stable(text) {
        if !v.is("step-started") {
            continue;
        }
        // `.event.step` of an event that is no object stops jq: nothing of
        // the log's steps is said.
        if v.event.hides() {
            steps.clear();
            break;
        }
        if let Some(s) = v.event().and_then(|e| e.step.get()) {
            if step_name(s) {
                steps.push(s.clone());
            }
        }
    }
    let n = steps.len();
    let at = steps.iter().rposition(|s| s == "commit-pr");
    let mut branch = match at {
        Some(i) if i + 1 < n => steps[i + 1].clone(),
        _ => String::new(),
    };
    if branch == "commit-pr" || !branch_of(issue, &branch) {
        branch.clear();
    }
    if branch.is_empty() && at.is_some() {
        branch = branch_said_after_push(text, issue);
    }
    Run {
        step: steps.last().cloned().unwrap_or_default(),
        branch,
        ..Run::default()
    }
}

/// The last branch of this issue an event after the last commit-pr names in its
/// `branch` field, unstable events too. A driver that names no branch as the
/// step after commit-pr names it there. It only says which branch to ask GitHub
/// about: the pull request is still GitHub's answer.
fn branch_said_after_push(text: &str, issue: u64) -> String {
    let mut pushed = false;
    let mut branch = String::new();
    for v in text.split('\n').filter_map(Line::parse) {
        let Some(e) = v.event() else {
            continue;
        };
        if v.is("step-started") {
            if e.step.str() == "commit-pr" {
                pushed = true;
                branch.clear();
            }
            continue;
        }
        if let Some(b) = e.branch.get() {
            if pushed && branch_of(issue, b) {
                branch = b.to_string();
            }
        }
    }
    branch
}

/// The progress summary, `<issue>.progress.json`, as tehas reads it back: the
/// fields of [`progress_fields`], the failure the end's record reads and the
/// rows of the usage table. `crate::progress::summary` writes it.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Progress {
    pub review_held: Loose<bool>,
    pub reason: Loose<String>,
    from: Node<ProgressFrom>,
    pub failure: Node<ProgressFailure>,
    pub roles: Many<Node<Row>>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct ProgressFrom {
    pub failure: Loose<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct ProgressFailure {
    pub kind: Loose<String>,
    pub step: Loose<String>,
    pub role: Loose<String>,
    pub cause: Loose<String>,
}

/// A row of the progress's roles, each count as it was written.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Row {
    pub role: Field,
    pub dispatches: Field,
    pub turns: Field,
    pub input: Field,
    pub output: Field,
    pub cache_read: Field,
}

impl Row {
    fn five(&self) -> [&Field; 5] {
        [
            &self.dispatches,
            &self.turns,
            &self.input,
            &self.output,
            &self.cache_read,
        ]
    }
}

impl Progress {
    /// The summary of a whole text, or None when it is no JSON object.
    pub fn of(text: &str) -> Option<Progress> {
        serde_json::from_str::<Node<Progress>>(text)
            .ok()?
            .into_object()
    }
}

/// The fields tehas-cycle reads of the progress summary, in its order:
/// reviewHeld, reason, from.failure, failure.kind, failure.role and
/// failure.cause. Each is empty past the point where jq would stop, and
/// reviewHeld is empty too when it is not a boolean.
pub fn progress_fields(text: Option<&str>) -> [String; 6] {
    let mut f: [String; 6] = Default::default();
    // `.a.b` of an object or null; anything else stops jq at once.
    let v = match text.and_then(|t| loose::first_value::<Node<Progress>>(&cut_bytes(t, 1_000_000)))
    {
        Some(Node::Object(v)) => v,
        Some(Node::Null) => Progress::default(),
        _ => return f,
    };
    let keep = |v: &Loose<String>, set: fn(char) -> bool, most: usize| match v.get() {
        Some(s) => cut(&kept(s, set), most),
        None => String::new(),
    };
    // reviewHeld is `true`, `false`, or empty when the progress does not say:
    // a field that is not there is not a review that held nothing.
    if let Some(b) = v.review_held.get() {
        f[0] = b.to_string();
    }
    let reason = Loose(v.reason.get().map(|s| s.replace('\n', " ")));
    f[1] = cut_reason(&keep(
        &reason,
        |c| c.is_ascii_alphanumeric() || " _.,:;/()=+-".contains(c),
        usize::MAX,
    ));
    let from = match &v.from {
        Node::Object(from) => from.failure.get().map(|s| s.replace('\0', "")),
        Node::Absent | Node::Null => None,
        Node::False | Node::Other => return f,
    };
    f[2] = from.unwrap_or_default();
    let failure = match &v.failure {
        Node::Object(failure) => failure.clone(),
        Node::Absent | Node::Null => ProgressFailure::default(),
        Node::False | Node::Other => return f,
    };
    f[3] = keep(&failure.kind, |c| c.is_ascii_lowercase() || c == '-', 40);
    f[4] = keep(
        &failure.role,
        |c| c.is_ascii_alphanumeric() || "_.-".contains(c),
        40,
    );
    f[5] = keep(
        &failure.cause,
        |c| c.is_ascii_alphanumeric() || "_.:-".contains(c),
        60,
    );
    f
}

/// The first `n` bytes, as `head -c` gives them.
fn cut_bytes(t: &str, n: usize) -> String {
    let b = &t.as_bytes()[..t.len().min(n)];
    String::from_utf8_lossy(b).into_owned()
}

/// A count as jq prints it in a string: an integer without a point.
pub fn num(x: &Number) -> String {
    if x.is_i64() || x.is_u64() {
        return x.to_string();
    }
    let f = x.as_f64().unwrap_or(0.0);
    if f.fract() == 0.0 && f.abs() < 1e17 {
        format!("{}", f as i64)
    } else {
        format!("{f}")
    }
}

/// A count of a row: a number of at least 0, else 0.
fn count(v: &Field) -> Number {
    match v {
        Field::Num(x) => n(Some(x)),
        _ => n(None),
    }
}

/// `[.roles[]? | objects]` of a progress read as one value, or None where
/// jq stops (a progress that is no object or null, or none at all).
fn rows_of(v: Option<Node<Progress>>) -> Option<Vec<Row>> {
    match v? {
        Node::Object(p) => Some(
            p.roles
                .items()
                .iter()
                .filter_map(Node::object)
                .cloned()
                .collect(),
        ),
        Node::Null => Some(vec![]),
        _ => None,
    }
}

/// The rows of the progress, or None when jq stops: a progress of other than
/// one value.
fn rows(progress: Option<&str>) -> Option<Vec<Row>> {
    let mut v = loose::values::<Node<Progress>>(progress?)?;
    // jq -c over the file prints one array per value; the table reads one.
    if v.len() != 1 {
        return None;
    }
    rows_of(v.pop())
}

/// The table of token usage the comments carry, as tehas-cycle makes it,
/// and whether the progress's rows could not be read (its WARN line).
pub fn usage(outcome: Option<&Outcome>, progress: Option<&str>) -> (String, bool) {
    let own = outcome.map(|o| o.usage.clone()).unwrap_or_default();
    let said = own.five().iter().any(|v| **v != Number::from(0));
    let rows = rows(progress);
    let unreadable = rows.is_none();
    // The rows' sum, when every row is a record of five numbers.
    let total = rows.as_ref().and_then(|r| {
        let told = !r.is_empty()
            && r.iter()
                .all(|row| row.five().iter().all(|v| matches!(v, Field::Num(_))));
        told.then(|| {
            let sum = |k: fn(&Row) -> &Field| {
                let s: f64 = r
                    .iter()
                    .map(|row| count(k(row)).as_f64().unwrap_or(0.0))
                    .sum();
                Number::from_f64(s).unwrap_or_else(|| Number::from(0))
            };
            Usage {
                dispatches: sum(|r| &r.dispatches),
                turns: sum(|r| &r.turns),
                input: sum(|r| &r.input),
                output: sum(|r| &r.output),
                cache_read: sum(|r| &r.cache_read),
            }
        })
    });
    // The table itself reads the progress as `head -c 1000000 | jq`.
    let table_rows = rows_of(
        progress.and_then(|p| loose::first_value::<Node<Progress>>(&cut_bytes(p, 1_000_000))),
    );
    let Some(r) = table_rows else {
        return (String::new(), unreadable);
    };
    let t = match total {
        Some(t) if !said => t,
        _ => own,
    };
    let d = n(Some(&t.dispatches));
    if r.is_empty() && num(&d) == "0" {
        return (String::new(), unreadable);
    }
    let mut lines = vec![
        "| Role | Dispatches | Turns | Input | Output | Cache read |".to_string(),
        "|---|---|---|---|---|---|".to_string(),
    ];
    for row in &r {
        let role = row
            .role
            .str()
            .map_or_else(|| row.role.json(), str::to_string);
        let role = kept(&role, |c| c.is_ascii_alphanumeric() || "_.-".contains(c));
        let cells: Vec<String> = row.five().iter().map(|k| num(&count(k))).collect();
        lines.push(format!("| {role} | {} |", cells.join(" | ")));
    }
    let cells: Vec<String> = t.five().iter().map(|k| num(&n(Some(k)))).collect();
    lines.push(format!(
        "| **total, all dispatches** | {} |",
        cells.join(" | ")
    ));
    (lines.join("\n"), unreadable)
}

/// `mine` of tehas_names_issue: the one rule, which finish and the dispatcher
/// ask too.
pub use tehas_core::issue_pull::names_issue;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tehas_core::testdir::TestDir;

    /// One step-started line of a run log.
    fn started(s: &str) -> String {
        format!("{{\"v\":1,\"kind\":\"step-started\",\"event\":{{\"step\":\"{s}\"}}}}\n")
    }

    /// At least `n` bytes of what a runner writes between its steps: one line,
    /// so the steps after it are still whole lines.
    fn written(n: usize) -> String {
        "y".repeat(n) + "\n"
    }

    #[test]
    fn a_cap_reason_is_never_the_bare_cap_word_and_says_memory_kills() {
        let none: &[String] = &[];
        for echo in [
            "",
            "  ",
            "verify-failed:develop",
            "verify-failed",
            "VERIFY-FAILED:develop",
        ] {
            assert_eq!(
                cap_reason("verify-failed:develop", echo, 0, none),
                NO_EVIDENCE,
                "{echo:?}"
            );
        }
        assert_eq!(cap_reason("round-cap", "round-cap", 0, none), NO_EVIDENCE);
        assert_eq!(cap_reason("turns", "out of turns", 0, none), "out of turns");
        assert_eq!(cap_reason("", "", 3, none), "", "no cap, no reason to give");
        assert_eq!(
            cap_reason(
                "verify-failed:develop",
                "verify-failed:develop",
                1,
                &["node".to_string()]
            ),
            "no evidence from the driver; 1 process in its container was killed for memory (node)"
        );
        assert_eq!(
            cap_reason("round-cap", "3 rounds", 2, none),
            "3 rounds; 2 processes in its container were killed for memory"
        );
        let said = "the container was killed; 4 processes in it were killed for memory";
        assert_eq!(
            cap_reason("container-killed", said, 4, none),
            said,
            "said once"
        );
        let long = "r".repeat(REASON_MOST + 10);
        assert!(cap_reason("turns", &long, 1, none).ends_with(REASON_CUT));
    }

    #[test]
    fn an_outcome_is_the_issues_entry() {
        let r = r#"{"schemaVersion":5,"buildCommit":"abc","issues":[{"issue":8},{"issue":7,"status":"handoff","steps":[{"step":"a"},{"step":"b x"}],"usage":{"input":3}}]}"#;
        let o = outcome(r, 7).unwrap();
        assert_eq!(o.status, "handoff");
        assert_eq!(o.last_step, "bx");
        assert_eq!(o.usage.input, Number::from(3));
        assert_eq!(o.usage.output, Number::from(0));
        assert_eq!(o.build_commit, "abc");
        assert_eq!(o.pr_state, "");
        let lean = r#"{"schemaVersion":5,"issues":[{"issue":7,"prState":"draft","handoff":{"channel":"host","body":"b"}}]}"#;
        let o = outcome(lean, 7).unwrap();
        assert_eq!((&*o.pr_state, &*o.handoff_channel), ("draft", "host"));
        assert!(outcome(r, 9).is_none());
        assert!(outcome(&r.replace(":5,", ":4,"), 7).is_none());
        assert!(outcome(
            r#"{"schemaVersion":5,"issues":[{"issue":7,"usage":"x"}]}"#,
            7
        )
        .is_none());
        assert!(outcome(
            r#"{"schemaVersion":5,"issues":[{"issue":7,"handoff":"x"}]}"#,
            7
        )
        .is_none());
    }

    #[test]
    fn an_outcome_prints_as_the_jq_it_replaced() {
        let r = r#"{"schemaVersion":5.0,"buildVersion":"1.2.3","issues":{"x":{"issue":7,"status":"park ed","cap":"turns","exitCode":2.0,"resumable":true,"prUrl":"https://h/p/1","steps":[{"step":"a"},null],"handoff":{"body":"line\nnext","channel":7},"usage":{"input":3.5,"output":-1,"cacheRead":"4","turns":0.0,"dispatches":12}}}}"#;
        let o = outcome(r, 7).unwrap();
        let printed = serde_json::json!(o).to_string();
        assert_eq!(
            printed,
            r#"{"branch":"","buildCommit":"","buildVersion":"1.2.3","cap":"turns","exitCode":2.0,"failedStep":"","handoffBody":"line\nnext","handoffChannel":"","lastStep":"","parkReason":"","prState":"","prUrl":"https://h/p/1","resumable":true,"status":"parked","usage":{"cacheRead":0,"dispatches":12,"input":3.5,"output":0,"turns":0.0}}"#
        );
        // The type holds its keys in that order too.
        assert_eq!(serde_json::to_string(&o).unwrap(), printed);
    }

    #[test]
    fn every_reader_takes_the_same_entry_of_a_result() {
        let cases = [
            (r#"{"issues":[{"issue":8},{"issue":7,"status":"1"}]}"#, Some(1)),
            (r#"{"issues":{"a":{"issue":7,"status":"1"}}}"#, Some(1)),
            (
                r#"{"issues":[{"issue":7,"status":"1"},{"issue":7,"status":"2"}]}"#,
                Some(1),
            ),
            (r#"{"issues":[{"issue":7.0,"status":"1"}]}"#, None),
            (r#"{"issues":[{"issue":"7","status":"1"}]}"#, None),
            (r#"{"issues":[7,{"issue":7,"status":"1"}]}"#, Some(1)),
            (r#"{"issues":7}"#, None),
            (
                "{\"issues\":[{\"issue\":7,\"status\":\"1\"}]}\n{\"issues\":[{\"issue\":7,\"status\":\"2\"}]}",
                Some(2),
            ),
            (
                "{\"issues\":[{\"issue\":7,\"status\":\"1\"}]}\n{\"issues\":[{\"issue\":8}]}",
                Some(1),
            ),
        ];
        for (text, n) in cases {
            let all = results(text).unwrap();
            let got = last_issue_entry(&all, 7).map(|e| e.status.str().parse::<u64>().unwrap());
            assert_eq!(got, n, "{text}");
            let schema5 = text.replace("{\"issues\"", "{\"schemaVersion\":5,\"issues\"");
            assert_eq!(outcome(&schema5, 7).is_some(), n.is_some(), "{text}");
            let end = crate::end_failed::entry(text, 7);
            assert_eq!(
                end.map(|e| e.status.str().parse::<u64>().unwrap()),
                n,
                "{text}"
            );
        }
    }

    #[test]
    fn a_branch_of_an_issue_is_its_number_and_a_slug_of_at_most_80() {
        let slug = |n: usize| "y".repeat(n);
        for b in [
            "feature/issue-7".to_string(),
            "feature/issue-7-a".to_string(),
            "feature/issue-7-A.b_c-d".to_string(),
            format!("feature/issue-7-{}", slug(80)),
        ] {
            assert!(branch_of(7, &b), "{b}");
        }
        for b in [
            "feature/issue-70".to_string(),
            "feature/issue-8-a".to_string(),
            "feature/issue-7-".to_string(),
            "feature/issue-7-a/b".to_string(),
            "feature/issue-7-a b".to_string(),
            "feature/issue-7x".to_string(),
            "x/feature/issue-7".to_string(),
            format!("feature/issue-7-{}", slug(81)),
        ] {
            assert!(!branch_of(7, &b), "{b}");
        }
    }

    #[test]
    fn a_log_names_its_last_step_and_the_branch_of_its_last_push() {
        let l = started;
        let log = [
            l("plan"),
            l("commit-pr"),
            l("feature/issue-7-a"),
            l("review"),
            l("commit-pr"),
            l("feature/issue-7-b"),
            l("ci"),
        ]
        .concat();
        assert_eq!(
            run(&log, 7),
            Run {
                step: "ci".into(),
                branch: "feature/issue-7-b".into(),
                ..Run::default()
            }
        );
        assert_eq!(run(&(l("commit-pr") + &l("feature/issue-8")), 7).branch, "");
        assert_eq!(run(&l("commit-pr"), 7).branch, "");
        let bad = l("plan") + "{\"v\":1,\"kind\":\"step-started\",\"event\":\"x\"}\n";
        assert_eq!(run(&bad, 7).step, "");
    }

    #[test]
    fn a_log_with_no_branch_step_after_commit_pr_names_the_branch_a_later_event_carries() {
        let l = started;
        let said = |b: &str| {
            format!("{{\"v\":1,\"kind\":\"lens-residuals-posted\",\"unstable\":true,\"event\":{{\"prNumber\":9,\"branch\":\"{b}\"}}}}\n")
        };
        let log = [
            l("commit-pr"),
            l("lens-review"),
            said("feature/issue-7-a"),
            l("lens-fix"),
        ]
        .concat();
        assert_eq!(run(&log, 7).branch, "feature/issue-7-a");
        assert_eq!(run(&log, 7).step, "lens-fix");
        // Only a branch of this issue, only after a push, and the branch step
        // after commit-pr comes first.
        let other = l("commit-pr") + &said("feature/issue-8-a");
        assert_eq!(run(&other, 7).branch, "");
        let before = said("feature/issue-7-a") + &l("commit-pr") + &l("lens-review");
        assert_eq!(run(&before, 7).branch, "");
        let both = l("commit-pr") + &l("feature/issue-7-b") + &said("feature/issue-7-a");
        assert_eq!(run(&both, 7).branch, "feature/issue-7-b");
    }

    #[test]
    fn a_run_log_of_at_most_the_bound_is_read_whole_and_says_whole() {
        let d = TestDir::new("runner-log-whole");
        let p = d.join("work-7-261101-093921.jsonl");
        let log = started("explore") + &started("commit-pr") + &started("feature/issue-7-checks");
        fs::write(&p, &log).unwrap();
        assert_eq!(
            run_log(&p, "7"),
            Some(Run {
                step: "feature/issue-7-checks".into(),
                branch: "feature/issue-7-checks".into(),
                read: "whole".into(),
            })
        );
    }

    #[test]
    fn a_run_log_past_the_bound_is_read_from_its_end_and_says_truncated() {
        let d = TestDir::new("runner-log-tail");
        let p = d.join("work-7-261101-093921.jsonl");
        let log = written(LOG_TAIL as usize + 1)
            + &started("commit-pr")
            + &started("feature/issue-7-late");
        fs::write(&p, &log).unwrap();
        assert!(p.metadata().unwrap().len() > LOG_TAIL);
        assert_eq!(
            run_log(&p, "7"),
            Some(Run {
                step: "feature/issue-7-late".into(),
                branch: "feature/issue-7-late".into(),
                read: "truncated".into(),
            })
        );
    }

    #[test]
    fn a_step_only_in_the_head_of_a_long_run_log_is_not_said() {
        let d = TestDir::new("runner-log-head");
        let p = d.join("work-7-261101-093921.jsonl");
        // explore and plan are past the end tehas reads, so of this run only
        // its last push is known.
        let log = started("explore")
            + &started("plan")
            + &written(LOG_TAIL as usize)
            + &started("commit-pr")
            + &started("feature/issue-7-late");
        fs::write(&p, &log).unwrap();
        let r = run_log(&p, "7").unwrap();
        assert_eq!(r.step, "feature/issue-7-late");
        assert_eq!(r.branch, "feature/issue-7-late");
        assert_eq!(r.read, "truncated");
    }

    #[test]
    fn the_progress_fields_stop_where_jq_stops() {
        let f = progress_fields(Some(r#"{"reviewHeld":true,"reason":"a\nb!","from":"x"}"#));
        assert_eq!(f[0], "true");
        assert_eq!(f[1], "a b");
        assert_eq!(f[2], "");
        let f = progress_fields(Some(
            r#"{"from":{"failure":"result"},"failure":{"kind":"provider","role":"coder","cause":"x y"}}"#,
        ));
        assert_eq!(f[2..], ["result", "provider", "coder", "xy"]);
        assert_eq!(progress_fields(Some("[1]"))[0], "");
        // Only a boolean says the review held something or nothing.
        assert_eq!(progress_fields(Some(r#"{"reviewHeld":false}"#))[0], "false");
        assert_eq!(progress_fields(Some(r#"{"reason":"x"}"#))[0], "");
        assert_eq!(progress_fields(Some(r#"{"reviewHeld":"false"}"#))[0], "");
    }

    #[test]
    fn a_long_reason_is_kept_whole_up_to_its_bound_and_says_where_it_was_cut() {
        // #718: a cause of 180 characters was cut mid-word at 200.
        let long = "verify-timeout verify command bash scripts/verify.sh exceeded its 30-min timeout on the CONSOLIDATED tree at /w/tehas (PI_ENSEMBLE_VERIFY_SUITE_TIMEOUT_MS=1800000), so the run was killed at the bound; the gate was still running test/cycle_test.sh";
        assert!(long.len() > 200);
        let f = progress_fields(Some(&format!(r#"{{"reason":"{long}"}}"#)));
        assert_eq!(f[1], long);
        let huge = "x".repeat(REASON_MOST + 1);
        let f = progress_fields(Some(&format!(r#"{{"reason":"{huge}"}}"#)));
        assert_eq!(f[1].chars().count(), REASON_MOST);
        assert!(f[1].ends_with("x (cut)"), "{}", &f[1][1980..]);
        assert_eq!(cut_reason(&f[1]), f[1]);
        assert_eq!(
            cut_reason(&"y".repeat(REASON_MOST)),
            "y".repeat(REASON_MOST)
        );
    }

    #[test]
    fn a_body_closes_only_its_own_number() {
        assert!(names_issue(&serde_json::json!({"body": "Fixes #7."}), 7));
        assert!(!names_issue(&serde_json::json!({"body": "Closes #70"}), 7));
        assert!(names_issue(
            &serde_json::json!({"headRefName": "feature/issue-7-x"}),
            7
        ));
        assert!(!names_issue(
            &serde_json::json!({"headRefName": "feature/issue-70"}),
            7
        ));
    }
}
