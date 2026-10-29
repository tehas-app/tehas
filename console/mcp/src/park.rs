//! `factory_park`: why an issue's last stopped run stopped. The step, kind,
//! cap and signature of its park marker come from the sessions snapshot, the
//! fields the cycle signed; the reason and the failing checks from the run's
//! result log, masked and cut.

use crate::logs::{self, Unopened};
use crate::snapshot::Snapshot;
use crate::tools::{self, Dirs, ToolError};
use regex::Regex;
use serde_json::{json, Map, Value};
use std::io::Read;
use std::sync::LazyLock;
use tehas_core::mask::mask;
use tehas_core::question::park_sig;

/// The outcomes of a run that stopped for a person: the ones a park comment
/// follows.
pub const STOPS: [&str; 4] = ["handoff", "parked", "failed", "refused"];
/// The largest issue number: the most digits a session id holds.
const ISSUE_MAX: u64 = 999_999_999;
/// A larger result log is not read. A real one is some kilobytes.
pub const RESULT_MAX: u64 = 1024 * 1024;
/// The most failing check lines an answer holds.
pub const CHECKS_MAX: usize = 20;
/// The most characters of one line or text of the result.
pub const TEXT_MAX: usize = 400;

/// A line of the runner's evidence that names a failing check.
static CHECK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bFAIL(ED)?\b|^\s*not ok\b|\bfailed checks\b").unwrap());
static CAUSE_CLASS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z-]{1,40}$").unwrap());

pub fn tool() -> Value {
    let int = |d: &str| json!({"type": "integer", "description": d});
    let text = |d: &str| json!({"type": "string", "description": d});
    let flag = |d: &str| json!({"type": "boolean", "description": d});
    json!({
        "name": "factory_park",
        "description": "Why an issue's last stopped work run of the last 30 days stopped: its outcome, the step, the failure kind, the cap, the park reason and cause, and sig, the signature its park comment's tehas:park marker carries, from the fields the run recorded. From the run's result log: the runner's message, cause and what to do, and the failing check lines, at most 20 of 400 characters each, all masked. An issue with no stopped run in the window is a tool error.",
        "inputSchema": {"type": "object", "properties": {
            "issue": {"type": "integer", "minimum": 1, "maximum": ISSUE_MAX, "description": "The issue number."},
        }, "required": ["issue"], "additionalProperties": false},
        "outputSchema": {"type": "object", "properties": {
            "api": int("The API version, 1."),
            "days": int("The days the snapshot covers."),
            "stale": flag("True when the snapshot is more than ten minutes old."),
            "issue": int("The issue number."),
            "id": text("The session id of the run, to give factory_session or factory_log."),
            "started": text("When it started, UTC."),
            "ended": text("When it ended, UTC."),
            "latest": flag("True when no later work run of the issue is in the window."),
            "outcome": {"type": "string", "enum": STOPS, "description": "How it ended: handoff, parked, failed or refused."},
            "status": text("The status word the cycle ended with."),
            "exitCode": int("The exit status of the cycle."),
            "step": text("The step it stopped at, as the park marker names it. Left out when none was recorded."),
            "failedStep": text("The step the runner said failed."),
            "kind": text("The kind of failure, as the park marker names it."),
            "cap": text("The cap that stopped it."),
            "reason": text("The park reason."),
            "cause": text("The cause of the failure."),
            "provider": flag("True when a model provider outage stopped it."),
            "sig": text("The signature of the park: sha256 of step, cap, reason, kind and cause, as the tehas:park marker of its park comment carries it."),
            "result": {"type": "string", "enum": ["read", "none", "gone", "unreadable"], "description": "Its result log: read, none listed, gone, or unreadable (not a plain file, over 1 MiB, not JSON, or no entry of this issue)."},
            "message": text("The runner's failure message, one line, masked."),
            "causeClass": text("What the runner blamed: change, environment or another word of its own."),
            "causeReason": text("The runner's reason for the cause, one line, masked."),
            "userAction": text("What the runner says to do, one line, masked."),
            "checks": {"type": "array", "items": {"type": "string"}, "description": "The failing check lines of the runner's evidence, masked, each one line of at most 400 characters, at most 20."},
            "checksCut": flag("True when there were more than 20 lines."),
        }, "required": ["api"]},
    })
}

fn bad(why: &str) -> ToolError {
    ToolError::BadArgs(why.to_string())
}

fn issue_arg(args: &Value) -> Result<u64, ToolError> {
    let map = args
        .as_object()
        .ok_or_else(|| bad("arguments are an object with issue"))?;
    if let Some(k) = map.keys().find(|k| *k != "issue") {
        return Err(bad(&format!("{k} is not an argument: issue is")));
    }
    args["issue"]
        .as_u64()
        .filter(|n| (1..=ISSUE_MAX).contains(n))
        .ok_or_else(|| bad(&format!("issue is an integer from 1 to {ISSUE_MAX}")))
}

/// Masked first, then one line, then cut: a cut never leaves part of a
/// secret the mask would have known whole.
fn line(s: &str) -> String {
    let one: String = mask(s)
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    one.trim().chars().take(TEXT_MAX).collect()
}

fn text_of(v: &Value) -> Option<String> {
    v.as_str().map(line).filter(|s| !s.is_empty())
}

/// The issue's entry of the result log, or the word to say why there is none.
fn entry(dirs: &Dirs, s: &Value, issue: u64) -> Result<Value, &'static str> {
    let listed = s["logs"].as_array().into_iter().flatten();
    let names: Vec<&str> = listed
        .filter(|l| l["word"] == "result")
        .filter_map(|l| l["name"].as_str())
        .collect();
    let name = match names.as_slice() {
        [] => return Err("none"),
        [one] => *one,
        _ => return Err("unreadable"),
    };
    if !logs::name_ok("work", issue, "result", name) {
        return Err("unreadable");
    }
    let (f, _) = logs::open_plain(&dirs.logs.join(name), RESULT_MAX).map_err(|u| match u {
        Unopened::Gone => "gone",
        _ => "unreadable",
    })?;
    let mut bytes = Vec::new();
    f.take(RESULT_MAX + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "unreadable")?;
    if bytes.len() as u64 > RESULT_MAX {
        return Err("unreadable");
    }
    // A file of several results: the last entry of this issue is the run's.
    let mut found = None;
    for v in serde_json::Deserializer::from_slice(&bytes).into_iter::<Value>() {
        let v = v.map_err(|_| "unreadable")?;
        let entries: Vec<&Value> = match &v["issues"] {
            Value::Array(a) => a.iter().collect(),
            Value::Object(m) => m.values().collect(),
            _ => vec![],
        };
        if let Some(e) = entries
            .into_iter()
            .rev()
            .find(|e| e["issue"].as_u64() == Some(issue))
        {
            found = Some(e.clone());
        }
    }
    found.filter(Value::is_object).ok_or("unreadable")
}

/// The runner's own extract first, then the lines of its evidence that name
/// a failing check, each once.
fn checks(e: &Value) -> (Vec<String>, bool) {
    let mut out: Vec<String> = Vec::new();
    let mut cut = false;
    let own = e["causeEvidence"].as_array().into_iter().flatten();
    let evidence = e["failure"]["evidence"].as_str().unwrap_or("");
    let found = own
        .filter_map(Value::as_str)
        .map(line)
        .chain(evidence.lines().filter(|l| CHECK.is_match(l)).map(line));
    for l in found {
        if l.is_empty() || out.contains(&l) {
            continue;
        }
        if out.len() == CHECKS_MAX {
            cut = true;
            break;
        }
        out.push(l);
    }
    (out, cut)
}

pub(crate) fn call(dirs: &Dirs, args: &Value) -> Result<Map<String, Value>, ToolError> {
    let issue = issue_arg(args)?;
    let snap = Snapshot::open(&dirs.sessions, tools::now())?;
    let mut latest = true;
    let mut stop = None;
    for s in snap.sessions() {
        if s["issue"].as_u64() != Some(issue) || s["kind"] != "work" {
            continue;
        }
        if s["outcome"].as_str().is_some_and(|o| STOPS.contains(&o)) {
            stop = Some(s);
            break;
        }
        latest = false;
    }
    let s = stop.ok_or_else(|| {
        bad(&format!(
            "issue {issue} has no stopped work run in the last {} days",
            snap.days
        ))
    })?;

    let word = |k: &str| s[k].as_str().unwrap_or("");
    let step = match word("lastStep") {
        "" => "-",
        w => w,
    };
    let sig = park_sig(
        step,
        word("cap"),
        word("parkReason"),
        word("failureKind"),
        word("failureCause"),
    );

    let mut out = Map::new();
    out.insert("days".into(), json!(snap.days));
    out.insert("stale".into(), json!(snap.stale));
    out.insert("issue".into(), json!(issue));
    out.insert("latest".into(), json!(latest));
    for (from, to) in [
        ("id", "id"),
        ("started", "started"),
        ("ended", "ended"),
        ("outcome", "outcome"),
        ("status", "status"),
        ("exitCode", "exitCode"),
        ("lastStep", "step"),
        ("failedStep", "failedStep"),
        ("failureKind", "kind"),
        ("cap", "cap"),
        ("parkReason", "reason"),
        ("failureCause", "cause"),
        ("providerOutage", "provider"),
    ] {
        let v = match &s[from] {
            Value::String(t) => Some(json!(mask(t))),
            v @ (Value::Number(_) | Value::Bool(_)) => Some(v.clone()),
            _ => None,
        };
        if let Some(v) = v {
            out.insert(to.into(), v);
        }
    }
    out.insert("sig".into(), json!(sig));

    match entry(dirs, &s, issue) {
        Err(why) => {
            out.insert("result".into(), json!(why));
        }
        Ok(e) => {
            out.insert("result".into(), json!("read"));
            for (k, v) in [
                ("message", &e["failure"]["message"]),
                ("causeReason", &e["causeReason"]),
                ("userAction", &e["userAction"]),
            ] {
                if let Some(t) = text_of(v) {
                    out.insert(k.into(), json!(t));
                }
            }
            if let Some(c) = e["causeClass"].as_str().filter(|c| CAUSE_CLASS.is_match(c)) {
                out.insert("causeClass".into(), json!(c));
            }
            let (lines, cut) = checks(&e);
            if !lines.is_empty() {
                out.insert("checks".into(), json!(lines));
            }
            if cut {
                out.insert("checksCut".into(), json!(true));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_is_masked_before_it_is_cut() {
        let token = format!("ghp_{}", "a".repeat(30));
        let s = format!("{}{token}", "x".repeat(TEXT_MAX - 10));
        let l = line(&s);
        assert!(!l.contains("ghp_aaaa"), "{l}");
        assert!(l.chars().count() <= TEXT_MAX);
        assert_eq!(line("a\nb\tc\r"), "a b c");
    }

    #[test]
    fn a_check_line_names_a_failure() {
        for l in [
            "FAIL: a case",
            "FAILED: test/x.sh: a case",
            "not ok 3 - a case",
            "test.sh: failed checks (1), not run (0)",
            "FAIL  shellcheck and the shell tests",
        ] {
            assert!(CHECK.is_match(l), "{l}");
        }
        for l in [
            "ok    the workspace's tests",
            "work_test.sh: 40 cases, 0 failed",
            "FAILURES are none",
            "",
        ] {
            assert!(!CHECK.is_match(l), "{l}");
        }
    }
}
