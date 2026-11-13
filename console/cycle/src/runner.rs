//! What tehas reads of pi-rukas's headless runner: the issue's entry of the
//! result (`tehas-cycle runner outcome`), the steps of a run log
//! (`tehas-cycle runner run`), the fields of the progress summary, and the
//! table of token usage.
//!
//! Where jq would stop on a value of the wrong type, these stop too: an
//! outcome jq cannot make is none, and a progress jq stops in gives the fields
//! it had printed before it stopped.

use regex::Regex;
use serde_json::{json, Map, Value};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// The result schema this reads.
pub const RESULT_SCHEMA: u64 = 5;

/// The first JSON value of a text, as `jq -n input` reads it.
pub fn first_value(text: &str) -> Option<Value> {
    serde_json::Deserializer::from_str(text)
        .into_iter::<Value>()
        .next()
        .and_then(Result::ok)
}

/// Every JSON value of a text, or None when one cannot be parsed.
pub fn values(text: &str) -> Option<Vec<Value>> {
    serde_json::Deserializer::from_str(text)
        .into_iter::<Value>()
        .collect::<Result<Vec<_>, _>>()
        .ok()
}

/// The issue's entry of one result value: the first object of its `issues`,
/// a list or an object, whose `issue` is that whole number. Every reader of
/// an entry goes through here, so they agree on the entry they describe.
pub fn issue_entry(result: &Value, issue: u64) -> Option<&Value> {
    let entries: Box<dyn Iterator<Item = &Value>> = match result.get("issues")? {
        Value::Array(a) => Box::new(a.iter()),
        Value::Object(o) => Box::new(o.values()),
        _ => return None,
    };
    entries
        .filter(|e| e.is_object())
        .find(|e| e.get("issue").and_then(Value::as_u64) == Some(issue))
}

/// The issue's entry in the last of a result's values that has one: a file
/// a re-run appended to describes the newest run.
pub fn last_issue_entry(results: &[Value], issue: u64) -> Option<&Value> {
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
pub const REASON_MOST: usize = 2000;
const REASON_CUT: &str = " (cut)";

/// A reason of at most REASON_MOST characters, ending ` (cut)` when it was
/// longer. Cutting one already cut changes nothing.
pub fn cut_reason(s: &str) -> String {
    if s.chars().nth(REASON_MOST).is_none() {
        return s.to_string();
    }
    cut(s, REASON_MOST - REASON_CUT.len()) + REASON_CUT
}

fn plain(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => kept(s, |c| c.is_ascii_alphanumeric() || "_.:/-".contains(c)),
        _ => String::new(),
    }
}

/// `n`: a number of at least 0, else 0.
fn n(v: Option<&Value>) -> Value {
    match v {
        Some(Value::Number(x)) if x.as_f64().is_some_and(|f| f >= 0.0) => Value::Number(x.clone()),
        _ => json!(0),
    }
}

/// Whether `.a.b?` of jq makes no value at all: `.a` is neither an object
/// nor null.
pub(crate) fn hides(v: Option<&Value>) -> bool {
    !matches!(v, None | Some(Value::Null) | Some(Value::Object(_)))
}

/// The issue's entry of a schema 5 result, its text fields reduced to plain
/// characters, its exitCode null unless a number. handoffBody is the handoff's
/// markdown as the runner rendered it, cut at 30000 characters and not
/// cleaned: it is for a comment, through q_clean. None when there is none.
pub fn outcome(text: &str, issue: u64) -> Option<Value> {
    let r = values(text)?;
    // jq -e answers for its last output; a file of several results has the
    // entry of each, and the last is the one read.
    r.iter().rev().find_map(|r| outcome_of(r, issue))
}

fn outcome_of(r: &Value, issue: u64) -> Option<Value> {
    let o = r.as_object()?;
    if o.get("schemaVersion").and_then(Value::as_f64) != Some(RESULT_SCHEMA as f64) {
        return None;
    }
    let e = issue_entry(r, issue)?;
    // `.handoff.body?` of a handoff that is no object makes no value, and so
    // no object at all; a usage that is no object stops jq.
    if hides(e.get("handoff")) {
        return None;
    }
    let usage = match e.get("usage") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => Map::new(),
        Some(Value::Object(u)) => u.clone(),
        _ => return None,
    };
    let steps: Vec<String> = match e.get("steps") {
        Some(Value::Array(a)) => a
            .iter()
            .filter(|s| matches!(s, Value::Object(_) | Value::Null))
            .map(|s| plain(s.get("step")))
            .collect(),
        Some(Value::Object(m)) => m
            .values()
            .filter(|s| matches!(s, Value::Object(_) | Value::Null))
            .map(|s| plain(s.get("step")))
            .collect(),
        _ => vec![],
    };
    let body = match e.get("handoff").and_then(|h| h.get("body")) {
        Some(Value::String(b)) => cut(b, 30000),
        _ => String::new(),
    };
    Some(json!({
        "status": plain(e.get("status")), "cap": plain(e.get("cap")),
        "failedStep": plain(e.get("failedStep")),
        "lastStep": steps.last().cloned().unwrap_or_default(),
        "parkReason": plain(e.get("parkReason")),
        "handoffBody": body,
        "handoffChannel": plain(e.get("handoff").and_then(|h| h.get("channel"))),
        "prState": plain(e.get("prState")),
        "branch": plain(e.get("branch")), "prUrl": plain(e.get("prUrl")),
        "resumable": e.get("resumable") == Some(&Value::Bool(true)),
        "exitCode": match e.get("exitCode") { Some(Value::Number(x)) => Value::Number(x.clone()), _ => Value::Null },
        "usage": {"input": n(usage.get("input")), "output": n(usage.get("output")),
                  "cacheRead": n(usage.get("cacheRead")), "turns": n(usage.get("turns")),
                  "dispatches": n(usage.get("dispatches"))},
        "buildCommit": plain(o.get("buildCommit")), "buildVersion": plain(o.get("buildVersion")),
    }))
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

/// The stable lines of a stream: each line that is a JSON object with v 1
/// and not marked unstable.
pub(crate) fn stable(text: &str) -> impl Iterator<Item = Value> + '_ {
    text.split('\n').filter_map(|l| {
        let v: Value = serde_json::from_str(l).ok()?;
        let ok = v.is_object()
            && v.get("v").and_then(Value::as_f64) == Some(1.0)
            && matches!(
                v.get("unstable"),
                None | Some(Value::Null) | Some(Value::Bool(false))
            );
        ok.then_some(v)
    })
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
        if v.get("kind").and_then(Value::as_str) != Some("step-started") {
            continue;
        }
        // `.event.step` of an event that is no object stops jq: nothing of
        // the log's steps is said.
        if hides(v.get("event")) {
            steps.clear();
            break;
        }
        if let Some(Value::String(s)) = v.get("event").and_then(|e| e.get("step")) {
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
    Run {
        step: steps.last().cloned().unwrap_or_default(),
        branch,
        ..Run::default()
    }
}

/// The fields tehas-cycle reads of the progress summary, in its order:
/// reviewHeld, reason, from.failure, failure.kind, failure.role and
/// failure.cause. Each is empty past the point where jq would stop.
pub fn progress_fields(text: Option<&str>) -> [String; 6] {
    let mut f: [String; 6] = Default::default();
    let Some(v) = text.and_then(|t| first_value(&cut_bytes(t, 1_000_000))) else {
        return f;
    };
    let Some(get) = Getter::of(&v) else {
        return f;
    };
    let keep = |v: &Value, set: fn(char) -> bool, most: usize| match v {
        Value::String(s) => cut(&kept(s, set), most),
        _ => String::new(),
    };
    f[0] = (get.at(&v, &["reviewHeld"]) == Some(Value::Bool(true))).to_string();
    let reason = match get.at(&v, &["reason"]).unwrap_or(Value::Null) {
        Value::Null | Value::Bool(false) => Value::String(String::new()),
        Value::String(s) => Value::String(s.replace('\n', " ")),
        o => o,
    };
    f[1] = cut_reason(&keep(
        &reason,
        |c| c.is_ascii_alphanumeric() || " _.,:;/()=+-".contains(c),
        usize::MAX,
    ));
    let Some(from) = get.at(&v, &["from", "failure"]) else {
        return f;
    };
    f[2] = match from {
        Value::String(s) => s.replace('\0', ""),
        _ => String::new(),
    };
    for (i, (k, set, most)) in [
        (
            "kind",
            (|c: char| c.is_ascii_lowercase() || c == '-') as fn(char) -> bool,
            40,
        ),
        (
            "role",
            |c: char| c.is_ascii_alphanumeric() || "_.-".contains(c),
            40,
        ),
        (
            "cause",
            |c: char| c.is_ascii_alphanumeric() || "_.:-".contains(c),
            60,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let Some(x) = get.at(&v, &["failure", k]) else {
            return f;
        };
        f[3 + i] = keep(&x, set, most);
    }
    f
}

/// The first `n` bytes, as `head -c` gives them.
fn cut_bytes(t: &str, n: usize) -> String {
    let b = &t.as_bytes()[..t.len().min(n)];
    String::from_utf8_lossy(b).into_owned()
}

/// jq's path reads: `.a.b` of an object or null, and an error, None, of
/// anything else.
struct Getter;

impl Getter {
    fn of(v: &Value) -> Option<Getter> {
        matches!(v, Value::Object(_) | Value::Null).then_some(Getter)
    }

    fn at(&self, v: &Value, path: &[&str]) -> Option<Value> {
        let mut cur = v.clone();
        for k in path {
            cur = match cur {
                Value::Object(o) => o.get(*k).cloned().unwrap_or(Value::Null),
                Value::Null => Value::Null,
                _ => return None,
            };
        }
        // `.k // ""`: null and false are nothing.
        Some(match cur {
            Value::Bool(false) => Value::Null,
            x => x,
        })
    }
}

/// A count as jq prints it in a string: an integer without a point.
pub fn num(v: &Value) -> String {
    match v {
        Value::Number(x) if x.is_i64() || x.is_u64() => x.to_string(),
        Value::Number(x) => {
            let f = x.as_f64().unwrap_or(0.0);
            if f.fract() == 0.0 && f.abs() < 1e17 {
                format!("{}", f as i64)
            } else {
                format!("{f}")
            }
        }
        _ => "0".into(),
    }
}

fn count(v: Option<&Value>) -> Value {
    n(v)
}

const FIVE: [&str; 5] = ["dispatches", "turns", "input", "output", "cacheRead"];

/// The rows of the progress, `[.roles[]? | objects]`, or None when jq stops
/// (a progress that is no object, or none at all).
fn rows(progress: Option<&str>) -> Option<Vec<Map<String, Value>>> {
    let v = values(progress?)?;
    // jq -c over the file prints one array per value; the table reads one.
    let v = match v.as_slice() {
        [one] => one.clone(),
        _ => return None,
    };
    let roles = match &v {
        Value::Object(o) => o.get("roles").cloned().unwrap_or(Value::Null),
        Value::Null => Value::Null,
        _ => return None,
    };
    Some(match roles {
        Value::Array(a) => a.iter().filter_map(|r| r.as_object().cloned()).collect(),
        Value::Object(m) => m.values().filter_map(|r| r.as_object().cloned()).collect(),
        _ => vec![],
    })
}

/// The table of token usage the comments carry, as tehas-cycle makes it,
/// and whether the progress's rows could not be read (its WARN line).
pub fn usage(outcome: &Value, progress: Option<&str>) -> (String, bool) {
    let own = outcome.get("usage").cloned().unwrap_or(json!({}));
    let said = own.is_object()
        && FIVE
            .iter()
            .any(|k| own.get(*k).is_some_and(|v| !v.is_null() && v != &json!(0)));
    let rows = rows(progress);
    let unreadable = rows.is_none();
    // The rows' sum, when every row is a record of five numbers.
    let total = rows.as_ref().and_then(|r| {
        let told = !r.is_empty()
            && r.iter().all(|row| {
                FIVE.iter()
                    .all(|k| row.get(*k).is_some_and(Value::is_number))
            });
        told.then(|| {
            let mut t = Map::new();
            for k in FIVE {
                let s: f64 = r
                    .iter()
                    .map(|row| count(row.get(k)).as_f64().unwrap_or(0.0))
                    .sum();
                t.insert(k.to_string(), json!(s));
            }
            Value::Object(t)
        })
    });
    // The table itself reads the progress as `head -c 1000000 | jq`.
    let table_rows = progress
        .and_then(|p| first_value(&cut_bytes(p, 1_000_000)))
        .and_then(|v| match v {
            Value::Object(o) => Some(match o.get("roles") {
                Some(Value::Array(a)) => a.iter().filter_map(|r| r.as_object().cloned()).collect(),
                Some(Value::Object(m)) => {
                    m.values().filter_map(|r| r.as_object().cloned()).collect()
                }
                _ => vec![],
            }),
            Value::Null => Some(vec![]),
            _ => None,
        });
    let Some(r) = table_rows else {
        return (String::new(), unreadable);
    };
    let t = match total {
        Some(t) if !said => t,
        _ => own,
    };
    let d = count(t.get("dispatches"));
    if r.is_empty() && num(&d) == "0" {
        return (String::new(), unreadable);
    }
    let mut lines = vec![
        "| Role | Dispatches | Turns | Input | Output | Cache read |".to_string(),
        "|---|---|---|---|---|---|".to_string(),
    ];
    for row in &r {
        let role = match row.get("role") {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Null) | None => "null".into(),
            Some(v) => v.to_string(),
        };
        let role = kept(&role, |c| c.is_ascii_alphanumeric() || "_.-".contains(c));
        let cells: Vec<String> = FIVE.iter().map(|k| num(&count(row.get(*k)))).collect();
        lines.push(format!("| {role} | {} |", cells.join(" | ")));
    }
    let cells: Vec<String> = FIVE.iter().map(|k| num(&count(t.get(*k)))).collect();
    lines.push(format!(
        "| **total, all dispatches** | {} |",
        cells.join(" | ")
    ));
    (lines.join("\n"), unreadable)
}

/// `mine` of tehas_names_issue: a pull request whose head is the issue's
/// branch, or whose body closes the issue.
pub fn names_issue(pr: &Value, issue: u64) -> bool {
    let text = |k: &str| match pr.get(k) {
        None | Some(Value::Null) | Some(Value::Bool(false)) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
    };
    let head = text("headRefName");
    if head == format!("feature/issue-{issue}")
        || head.starts_with(&format!("feature/issue-{issue}-"))
    {
        return true;
    }
    let re = Regex::new(&format!(
        r"(?i)\b(close[sd]?|fix(e[sd])?|resolve[sd]?):?\s+#{issue}([^0-9]|$)"
    ))
    .unwrap();
    re.is_match(&text("body"))
}

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
    fn an_outcome_is_the_issues_entry() {
        let r = r#"{"schemaVersion":5,"buildCommit":"abc","issues":[{"issue":8},{"issue":7,"status":"handoff","steps":[{"step":"a"},{"step":"b x"}],"usage":{"input":3}}]}"#;
        let o = outcome(r, 7).unwrap();
        assert_eq!(o["status"], "handoff");
        assert_eq!(o["lastStep"], "bx");
        assert_eq!(o["usage"]["input"], 3);
        assert_eq!(o["usage"]["output"], 0);
        assert_eq!(o["buildCommit"], "abc");
        assert_eq!(o["prState"], "");
        let lean = r#"{"schemaVersion":5,"issues":[{"issue":7,"prState":"draft","handoff":{"channel":"host","body":"b"}}]}"#;
        let o = outcome(lean, 7).unwrap();
        assert_eq!(
            (&o["prState"], &o["handoffChannel"]),
            (&json!("draft"), &json!("host"))
        );
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
    fn every_reader_takes_the_same_entry_of_a_result() {
        let cases = [
            (r#"{"issues":[{"issue":8},{"issue":7,"n":1}]}"#, Some(1)),
            (r#"{"issues":{"a":{"issue":7,"n":1}}}"#, Some(1)),
            (
                r#"{"issues":[{"issue":7,"n":1},{"issue":7,"n":2}]}"#,
                Some(1),
            ),
            (r#"{"issues":[{"issue":7.0,"n":1}]}"#, None),
            (r#"{"issues":[{"issue":"7","n":1}]}"#, None),
            (r#"{"issues":[7,{"issue":7,"n":1}]}"#, Some(1)),
            (r#"{"issues":7}"#, None),
            (
                "{\"issues\":[{\"issue\":7,\"n\":1}]}\n{\"issues\":[{\"issue\":7,\"n\":2}]}",
                Some(2),
            ),
            (
                "{\"issues\":[{\"issue\":7,\"n\":1}]}\n{\"issues\":[{\"issue\":8}]}",
                Some(1),
            ),
        ];
        for (text, n) in cases {
            let all = values(text).unwrap();
            let got = last_issue_entry(&all, 7).map(|e| e["n"].as_u64().unwrap());
            assert_eq!(got, n, "{text}");
            let schema5 = text.replace("{\"issues\"", "{\"schemaVersion\":5,\"issues\"");
            assert_eq!(outcome(&schema5, 7).is_some(), n.is_some(), "{text}");
            let end = crate::end_failed::entry(text, 7);
            assert_eq!(end["n"].as_u64(), n, "{text}");
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
        assert!(names_issue(&json!({"body": "Fixes #7."}), 7));
        assert!(!names_issue(&json!({"body": "Closes #70"}), 7));
        assert!(names_issue(&json!({"headRefName": "feature/issue-7-x"}), 7));
        assert!(!names_issue(&json!({"headRefName": "feature/issue-70"}), 7));
    }
}
