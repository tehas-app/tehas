//! What tehas reads of pi-rukas's headless runner for a cycle's report, as
//! lib/headless.sh and libexec/cycle read it: the issue's entry of the result
//! (tehas_headless_outcome), the steps of a run log (tehas_headless_run), the
//! fields of the progress summary, and the table of token usage.
//!
//! Where jq would stop on a value of the wrong type, these stop too: an
//! outcome jq cannot make is none, and a progress jq stops in gives the fields
//! it had printed before it stopped.

use regex::Regex;
use serde_json::{json, Map, Value};

/// The result schema this reads (tehas_headless_result).
const RESULT_SCHEMA: u64 = 5;

/// The first JSON value of a text, as `jq -n input` reads it.
pub fn first_value(text: &str) -> Option<Value> {
    serde_json::Deserializer::from_str(text)
        .into_iter::<Value>()
        .next()
        .and_then(Result::ok)
}

/// Every JSON value of a text, or None when one cannot be parsed.
fn values(text: &str) -> Option<Vec<Value>> {
    serde_json::Deserializer::from_str(text)
        .into_iter::<Value>()
        .collect::<Result<Vec<_>, _>>()
        .ok()
}

/// A string with only the bytes of `keep`, as gsub("[^...]"; "") leaves it.
pub fn kept(s: &str, keep: fn(char) -> bool) -> String {
    s.chars().filter(|c| keep(*c)).collect()
}

/// The first `n` characters.
pub fn cut(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
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
fn hides(v: Option<&Value>) -> bool {
    !matches!(v, None | Some(Value::Null) | Some(Value::Object(_)))
}

/// tehas_headless_outcome: the issue's entry of a schema 5 result, its text
/// fields reduced to plain characters. None when there is none.
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
    let e = match o.get("issues") {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(Value::as_object)
            .find(|e| e.get("issue").and_then(Value::as_f64) == Some(issue as f64))?,
        Some(Value::Object(m)) => m
            .values()
            .filter_map(Value::as_object)
            .find(|e| e.get("issue").and_then(Value::as_f64) == Some(issue as f64))?,
        _ => return None,
    };
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
        "branch": plain(e.get("branch")), "prUrl": plain(e.get("prUrl")),
        "resumable": e.get("resumable") == Some(&Value::Bool(true)),
        "exitCode": match e.get("exitCode") { Some(Value::Number(x)) => Value::Number(x.clone()), _ => Value::Null },
        "usage": {"input": n(usage.get("input")), "output": n(usage.get("output")),
                  "cacheRead": n(usage.get("cacheRead")), "turns": n(usage.get("turns")),
                  "dispatches": n(usage.get("dispatches"))},
        "buildCommit": plain(o.get("buildCommit")), "buildVersion": plain(o.get("buildVersion")),
    }))
}

/// tehas_branch_pattern: a branch pi-rukas makes for the issue.
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

/// What tehas_headless_run says of a run log.
#[derive(Debug, Default, PartialEq)]
pub struct Run {
    pub step: String,
    pub branch: String,
    /// `whole`, or `truncated` for a log over 200 KB.
    pub read: String,
}

fn step_name(s: &str) -> bool {
    (1..=100).contains(&s.len())
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_./:-".contains(&c))
}

/// The stable lines of a stream: each line that is a JSON object with v 1
/// and not marked unstable.
fn stable(text: &str) -> impl Iterator<Item = Value> + '_ {
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

/// tehas_headless_run: the last step a log says was started, the branch of
/// its last commit-pr when it is the issue's, and whether it was read whole.
pub fn run(text: &str, issue: u64) -> Run {
    let read = if text.len() <= 200_000 {
        "whole"
    } else {
        "truncated"
    }
    .to_string();
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
        read,
    }
}

/// The fields libexec/cycle reads of the progress summary, in its order:
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
    f[1] = keep(
        &reason,
        |c| c.is_ascii_alphanumeric() || " _.,:;/()=+-".contains(c),
        200,
    );
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
fn num(v: &Value) -> String {
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

/// The table of token usage the comments carry, as libexec/cycle makes it,
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

    #[test]
    fn an_outcome_is_the_issues_entry() {
        let r = r#"{"schemaVersion":5,"buildCommit":"abc","issues":[{"issue":8},{"issue":7,"status":"handoff","steps":[{"step":"a"},{"step":"b x"}],"usage":{"input":3}}]}"#;
        let o = outcome(r, 7).unwrap();
        assert_eq!(o["status"], "handoff");
        assert_eq!(o["lastStep"], "bx");
        assert_eq!(o["usage"]["input"], 3);
        assert_eq!(o["usage"]["output"], 0);
        assert_eq!(o["buildCommit"], "abc");
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
    fn a_log_names_its_last_step_and_the_branch_of_its_last_push() {
        let l = |s: &str| {
            format!("{{\"v\":1,\"kind\":\"step-started\",\"event\":{{\"step\":\"{s}\"}}}}\n")
        };
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
                read: "whole".into()
            }
        );
        assert_eq!(run(&(l("commit-pr") + &l("feature/issue-8")), 7).branch, "");
        assert_eq!(run(&l("commit-pr"), 7).branch, "");
        let bad = l("plan") + "{\"v\":1,\"kind\":\"step-started\",\"event\":\"x\"}\n";
        assert_eq!(run(&bad, 7).step, "");
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
    fn a_body_closes_only_its_own_number() {
        assert!(names_issue(&json!({"body": "Fixes #7."}), 7));
        assert!(!names_issue(&json!({"body": "Closes #70"}), 7));
        assert!(names_issue(&json!({"headRefName": "feature/issue-7-x"}), 7));
        assert!(!names_issue(&json!({"headRefName": "feature/issue-70"}), 7));
    }
}
