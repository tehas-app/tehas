//! `factory_park`: why an issue's last stopped run stopped. The step, kind,
//! cap and signature of its park marker come from the sessions snapshot, the
//! fields the cycle signed; the reason and the failing checks from the run's
//! result log, masked and cut.

use crate::args::{Args, Shape};
use crate::logs::{self, Unopened};
use crate::snapshot::{Loose, Many, Scalar, Session, Snapshot};
use crate::tools::{self, Dirs, ToolError};
use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::io::Read;
use std::sync::LazyLock;
use tehas_core::mask::mask;
use tehas_core::question::park_sig;

use tehas_core::outcome::{OUTCOMES, STOPS};
/// The largest issue number: the most digits a session id holds.
/// A larger result log is not read. A real one is some kilobytes.
pub const RESULT_MAX: u64 = tehas_core::file::READ_BOUND;
/// The most failing check lines an answer holds.
pub const CHECKS_MAX: usize = 20;
/// The most characters of one line or text of the result.
pub const TEXT_MAX: usize = 400;

/// A line of the runner's evidence that names a failing check.
static CHECK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bFAIL(ED)?\b|^\s*not ok\b|\bfailed checks\b").unwrap());
/// One word of the runner's own: a cause class or an error code.
static WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z-]{1,40}$").unwrap());

pub fn tool() -> Value {
    let int = |d: &str| json!({"type": "integer", "description": d});
    let text = |d: &str| json!({"type": "string", "description": d});
    let flag = |d: &str| json!({"type": "boolean", "description": d});
    json!({
        "name": "factory_park",
        "description": "Why an issue's last stopped work run of the last 30 days stopped: its outcome, the step, the failure kind, the cap, the park reason and cause, and sig, the signature its park comment's tehas:park marker carries, from the fields the run recorded. From the run's result log: the runner's message, cause and what to do, and the failing check lines, at most 20 of 400 characters each, all masked. An issue with no stopped run in the window is a tool error.",
        "inputSchema": {"type": "object", "properties": {
            "issue": {"type": "integer", "minimum": 1, "maximum": tools::ISSUE_MAX, "description": "The issue number."},
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
            "latestOutcome": {"type": "string", "enum": OUTCOMES, "description": "When latest is false: the outcome of the issue's newest work run, such as stopped for one interrupted or whose issue closed under it. Left out when that run recorded no outcome word."},
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
            "forksRefused": int("Starts of a process its container refused at its limit. Left out when none."),
            "oomKills": int("Processes killed for memory. Left out when none."),
            "sig": text("The signature of the park: sha256 of step, cap, reason, kind and cause, as the tehas:park marker of its park comment carries it."),
            "result": {"type": "string", "enum": ["read", "none", "gone", "unreadable"], "description": "Its result log: read, none listed, gone, or unreadable (not a plain file, over 1 MiB, not JSON, or no entry of this issue)."},
            "message": text("The runner's failure message, one line, masked."),
            "errorCode": text("The runner's error code when the runner itself stopped, such as crash."),
            "error": text("The runner's error message when the runner itself stopped, one line, masked."),
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

/// What `factory_park` takes: the issue's number, and nothing else.
#[derive(Debug)]
pub struct ParkArgs {
    issue: u64,
}

impl ParkArgs {
    pub(crate) fn read(args: &Args) -> Result<Self, ToolError> {
        issue_arg(args).map(|issue| ParkArgs { issue })
    }
}

fn issue_arg(args: &Args) -> Result<u64, ToolError> {
    if args.shape() != Shape::Object {
        return Err(bad("arguments are an object with issue"));
    }
    if let Some(k) = args.keys().find(|k| *k != "issue") {
        return Err(bad(&format!("{k} is not an argument: issue is")));
    }
    tools::issue_arg(&args.get("issue")).map(u64::from)
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

fn text_of(w: Option<&str>) -> Option<String> {
    w.map(line).filter(|s| !s.is_empty())
}

/// One entry of the result log, of the issue asked for: what the answer
/// serves of the runner's own extract. A field of another shape than the
/// word or the list of words it names reads as nothing, as indexing a
/// document read it.
#[derive(Deserialize, Default)]
pub(crate) struct ResultEntry {
    #[serde(default)]
    issue: Loose<u64>,
    #[serde(default)]
    failure: Loose<Failure>,
    #[serde(rename = "causeClass", default)]
    cause_class: Loose<String>,
    #[serde(rename = "causeReason", default)]
    cause_reason: Loose<String>,
    #[serde(rename = "userAction", default)]
    user_action: Loose<String>,
    #[serde(rename = "causeEvidence", default)]
    cause_evidence: Many<Loose<String>>,
}

/// The part of the runner's failure the answer serves.
#[derive(Deserialize, Default)]
pub(crate) struct Failure {
    #[serde(default)]
    message: Loose<String>,
    #[serde(default)]
    evidence: Loose<String>,
}

/// The error of the result that holds the entry: what the answer serves of
/// the runner itself when it stopped.
#[derive(Deserialize, Default)]
pub(crate) struct RunnerError {
    #[serde(default)]
    code: Loose<String>,
    #[serde(default)]
    message: Loose<String>,
}

/// The issue's entry of the result log and the error of the result that
/// holds it, or the word to say why there is none.
fn entry(dirs: &Dirs, s: &Session, issue: u64) -> Result<(ResultEntry, RunnerError), &'static str> {
    let listed = s.logs();
    let names: Vec<&str> = listed
        .iter()
        .filter(|l| l.word() == Some("result"))
        .filter_map(|l| l.name())
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
    // The stream is read as the file wrote it; only the entry and the error
    // that answer are kept as their types.
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
            found = Some((
                serde_json::from_value::<ResultEntry>(e.clone()).unwrap_or_default(),
                serde_json::from_value::<RunnerError>(v["error"].clone()).unwrap_or_default(),
            ));
        }
    }
    // Only an object can hold the issue it was found by, as the entry the
    // answer serves always did.
    found
        .filter(|(e, _)| e.issue.get() == Some(&issue))
        .ok_or("unreadable")
}

/// One word of the runner's, masked first: a private name fails the pattern.
fn word_of(w: Option<&str>) -> Option<String> {
    w.map(mask).filter(|w| WORD.is_match(w))
}

/// The runner's own extract first, then the lines of its evidence that name
/// a failing check. A line held in another, as the runner cuts or quotes
/// one, is given once, the longer kept.
fn checks(e: &ResultEntry) -> (Vec<String>, bool) {
    let mut out: Vec<String> = Vec::new();
    let mut cut = false;
    let evidence = e
        .failure
        .get()
        .and_then(|f| f.evidence.get().map(String::as_str))
        .unwrap_or("");
    let found = e
        .cause_evidence
        .list()
        .iter()
        .filter_map(|w| w.get().map(String::as_str))
        .map(line)
        .chain(evidence.lines().filter(|l| CHECK.is_match(l)).map(line));
    for l in found {
        if l.is_empty() || out.iter().any(|o| o.contains(l.as_str())) {
            continue;
        }
        if let Some(i) = out.iter().position(|o| l.contains(o.as_str())) {
            out.retain(|o| !l.contains(o.as_str()));
            out.insert(i, l);
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

pub(crate) fn call(dirs: &Dirs, asked: &ParkArgs) -> Result<Map<String, Value>, ToolError> {
    let issue = asked.issue;
    let snap = Snapshot::open(&dirs.sessions, tools::now())?;
    let mut latest = true;
    let mut newest: Option<String> = None;
    let mut stop = None;
    for s in snap.sessions() {
        if s.whole("issue") != Some(issue) || s.text("kind") != Some("work") {
            continue;
        }
        if s.text("outcome").is_some_and(|o| STOPS.contains(&o)) {
            stop = Some(s);
            break;
        }
        if latest {
            newest = s
                .text("outcome")
                .filter(|o| OUTCOMES.contains(o))
                .map(str::to_string);
        }
        latest = false;
    }
    let s = stop.ok_or_else(|| {
        bad(&format!(
            "issue {issue} has no stopped work run in the last {} days",
            snap.days
        ))
    })?;

    let word = |k: &str| s.text(k).unwrap_or("");
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
    if let Some(o) = newest {
        out.insert("latestOutcome".into(), json!(o));
    }
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
        // The word masked; a number or a flag travels as the line wrote it.
        let v = match s.field(from) {
            Some(Scalar::Text(t)) => Some(json!(mask(t))),
            Some(
                f @ (Scalar::Count(_) | Scalar::Big(_) | Scalar::Decimal(..) | Scalar::Flag(_)),
            ) => Some(serde_json::to_value(f).expect("a scalar serialises")),
            None => None,
        };
        if let Some(v) = v {
            out.insert(to.into(), v);
        }
    }
    // What the park comment states of the container, when it happened.
    for k in ["forksRefused", "oomKills"] {
        if let Some(n) = s.whole(k).filter(|n| *n > 0) {
            out.insert(k.into(), json!(n));
        }
    }
    out.insert("sig".into(), json!(sig));

    match entry(dirs, &s, issue) {
        Err(why) => {
            out.insert("result".into(), json!(why));
        }
        Ok((e, error)) => {
            out.insert("result".into(), json!("read"));
            if let Some(c) = word_of(error.code.get().map(String::as_str)) {
                out.insert("errorCode".into(), json!(c));
            }
            if let Some(t) = text_of(error.message.get().map(String::as_str)) {
                out.insert("error".into(), json!(t));
            }
            let failure = e.failure.get();
            for (k, t) in [
                (
                    "message",
                    failure.and_then(|f| f.message.get().map(String::as_str)),
                ),
                ("causeReason", e.cause_reason.get().map(String::as_str)),
                ("userAction", e.user_action.get().map(String::as_str)),
            ] {
                if let Some(t) = text_of(t) {
                    out.insert(k.into(), json!(t));
                }
            }
            if let Some(c) = word_of(e.cause_class.get().map(String::as_str)) {
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
