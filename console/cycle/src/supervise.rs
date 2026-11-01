//! `tehas-cycle cycle`: a cycle's start and its supervision. libexec/cycle holds
//! the issue's lock as fd 9 and execs this, so the unit's stop reaches it as the
//! main process. fd 9 stays open: Rust's children inherit it, and a cycle killed
//! outright keeps its place until tehas work and the docker client are gone too.
//!
//! The start goes on record under the lock. The new attempt is what the
//! dispatcher waits for to know its unit came up, and a cycle that goes on to
//! refuse its number still came up. From the unit the start uses the approval
//! the dispatcher recorded; a run by hand is on record as one. A unit is never
//! taken for a run by hand: a start the store refuses runs nothing.
//!
//! A TERM, INT or HUP before tehas work starts ends this process. Once it runs
//! they are passed on as TERM, and the report is then made with one bounded try
//! for each call to GitHub. The report is made even if a stop arrives during it:
//! leaving tehas:running on the issue would park it for good.
//!
//! The report is `apply` where the instance sets TEHAS_CYCLE_REPORT=rust. Else
//! libexec/cycle is run again with what it needs in TEHAS_CYCLE_ENDED. It goes by
//! the settings of this environment and loads none: instance.env may have changed.

use crate::after::log;
use crate::apply;
use crate::host::{self, Refused, Start};
use crate::runner;
use crate::start::attempt_shaped;
use serde_json::Value;
use std::fs::{self, OpenOptions};
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tehas_core::time;

/// The notes of an earlier cycle, killed outright before it could report.
const NOTES: [&str; 13] = [
    "oom",
    "forks",
    "stalled",
    "timed",
    "stopped",
    "stopped.new",
    "result.json",
    "progress.json",
    "nostart",
    "lean-refused",
    "done",
    "runner_exit",
    "status",
];

/// What libexec/cycle gives this.
pub struct Given {
    pub issue: u64,
    pub work: String,
    pub status_cmd: String,
    pub console: String,
    /// libexec/cycle, for the report in bash.
    pub again: String,
}

/// What a number is, as GitHub answers for it. The factory works on open issues
/// only: a closed issue is never taken, and a pull request is never taken, open,
/// merged or closed. A number is never taken on a guess.
#[derive(Debug, PartialEq, Eq)]
pub enum Number {
    Open,
    Pr,
    Closed,
    Unreadable,
}

pub fn number(answer: &str) -> Number {
    let values = runner::values(answer).unwrap_or_default();
    let [one] = values.as_slice() else {
        return Number::Unreadable;
    };
    if !one.is_object() {
        return Number::Unreadable;
    }
    if !matches!(one.get("pull_request"), None | Some(Value::Null)) {
        return Number::Pr;
    }
    let state = match one.get("state") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => "",
        Some(Value::String(s)) => s,
        _ => return Number::Unreadable,
    };
    match state.to_ascii_lowercase().as_str() {
        "open" => Number::Open,
        "closed" => Number::Closed,
        _ => Number::Unreadable,
    }
}

/// An issue a handoff left with needs-human-attention starts again only on a
/// person's answer: --restart tells the runner so, and takes the label off.
pub fn wants_restart(answer: &str) -> bool {
    let Some(v) = runner::first_value(answer) else {
        return false;
    };
    let is_it = |l: &Value| l.get("name").and_then(Value::as_str) == Some("needs-human-attention");
    match v.get("labels") {
        Some(Value::Array(a)) => a.iter().any(is_it),
        Some(Value::Object(o)) => o.values().any(is_it),
        _ => false,
    }
}

/// The attempt of a start the store took after it timed out: one on record since
/// this one began, under this lock, is its own.
pub fn took_late(shown: &Value, began: i64) -> Option<String> {
    let c = shown.get("cycle").filter(|c| c.is_object())?;
    let open = matches!(c.get("ended_at"), None | Some(Value::Null));
    let since = c.get("started_at").and_then(Value::as_f64)? >= began as f64;
    let attempt = c.get("attempt")?.to_string();
    (open && since && attempt_shaped(&attempt)).then_some(attempt)
}

fn refused(f: Value) -> Refused {
    let word = |k: &str| f.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    Refused {
        word: word("word"),
        line: word("line"),
    }
}

fn rc_of(f: &Value) -> i64 {
    f.get("rc").and_then(Value::as_i64).unwrap_or(78)
}

fn by_hand() -> bool {
    let var = |k: &str| std::env::var(k).unwrap_or_default();
    var("TEHAS_CYCLE_FROM") != "unit" && var("INVOCATION_ID").is_empty()
}

/// The pi-rukas commit of the image a cycle started now runs.
fn rukas_commit() -> Option<String> {
    let image = std::env::var("PI_ENSEMBLE_IMAGE").unwrap_or_default();
    let format =
        "{{index .Config.Labels \"tehas.rukas\"}}|{{index .Config.Labels \"tehas.rukas.images\"}}";
    let out = Command::new("docker")
        .args(["image", "inspect", "--format", format, &image])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let commit = text.lines().next()?.split('|').next()?;
    let shaped = (7..=40).contains(&commit.len())
        && commit
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    shaped.then(|| commit.to_string())
}

/// The start on record, or the status to exit with.
fn start_on_record(home: &Path, n: &str) -> Result<Value, i32> {
    let rukas = rukas_commit();
    let mut args = vec!["cycle-start", n];
    if let Some(r) = &rukas {
        args.extend(["--rukas", r]);
    }
    if by_hand() {
        args.push("--by-hand");
    }
    // A busy store committed nothing, so the start is asked again. A store that
    // timed out may have committed it. It is then ended as refused and nothing
    // runs.
    let began = time::now();
    let mut failed = None;
    for attempt in 1..=3 {
        let failure = match host::store_value(home, &args) {
            Ok(v) => return Ok(v),
            Err(f) => f,
        };
        let (rc, said) = (rc_of(&failure), refused(failure));
        match (rc, said.word.as_str()) {
            (1, "busy") => {}
            (1, "timeout") => match host::store_try(home, 3, &["cycle-show", n]) {
                Err(r) => {
                    log(&format!("refuse #{n}: the state store did not answer the start in time, and whether it took it is not known ({}): {}", r.word, said.line));
                    return Err(75);
                }
                Ok(shown) => {
                    if let Some(took) = took_late(&shown, began) {
                        let end = ["cycle-end", n, "--attempt", &took, "--refused"];
                        if let Err(r) = host::store_try(home, 5, &end) {
                            log(&format!("ERROR: the start of #{n} the store took late is not ended ({}): cycle-end {n} --attempt {took} --refused", r.word));
                        }
                        log(&format!(
                            "refuse #{n}: the state store took the start only after it timed out: {}",
                            said.line
                        ));
                        return Err(75);
                    }
                }
            },
            _ => {
                failed = Some((rc, said));
                break;
            }
        }
        failed = Some((rc, said));
        if attempt < 3 {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    match failed {
        Some((1, s)) if matches!(s.word.as_str(), "busy" | "timeout") => {
            log(&format!(
                "refuse #{n}: the state store did not take the start ({}): {}",
                s.word, s.line
            ));
            Err(75)
        }
        Some((1, s)) => {
            log(&format!(
                "refuse #{n}: the state store refused the start: {}",
                s.line
            ));
            Err(77)
        }
        Some((_, s)) => {
            log(&format!(
                "ERROR: #{n}: the state store cannot be used: {}",
                s.line
            ));
            Err(78)
        }
        None => Err(78),
    }
}

/// A start that refuses its number ends its record, so the next start of the
/// issue is not one that replaces an open row.
fn end_refused(home: &Path, n: &str, attempt: &str) {
    let end = ["cycle-end", n, "--attempt", attempt, "--refused"];
    if let Err(r) = host::store_try(home, 5, &end) {
        log(&format!(
            "ERROR: the refused start of #{n} is not on record ({}): cycle-end {n} --attempt {attempt} --refused",
            r.word
        ));
    }
}

/// The answer for the number. A call that only failed is tried again rather
/// than refusing a cycle that was already authorized.
fn ask_github(repo: &str, n: &str) -> Option<String> {
    let wait: u64 = std::env::var("TEHAS_CYCLE_RETRY_WAIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    for attempt in 1..=3 {
        let out = Command::new("gh")
            .args(["api", &format!("repos/{repo}/issues/{n}")])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output();
        if let Ok(o) = out {
            if o.status.success() {
                return Some(String::from_utf8_lossy(&o.stdout).into_owned());
            }
        }
        if attempt < 3 {
            std::thread::sleep(Duration::from_secs(wait));
        }
    }
    None
}

fn exit_code(s: ExitStatus) -> i32 {
    s.code()
        .or_else(|| s.signal().map(|n| 128 + n))
        .unwrap_or(127)
}

fn term(pid: u32) {
    let _ = Command::new("kill")
        .args(["-TERM", "--", &pid.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// tehas work to its end, whatever signal comes: its exit status, and whether
/// one came. The signals are taken before it starts, so none falls between.
fn supervise(
    home: &Path,
    g: &Given,
    attempt: &str,
    restart: bool,
    stop: &AtomicBool,
) -> (i32, bool) {
    let log = home.join(format!("logs/dispatch-work-{}.log", g.issue));
    let out = OpenOptions::new().create(true).append(true).open(&log);
    let Ok(out) = out else {
        return (1, stop.load(Ordering::SeqCst));
    };
    let Ok(err) = out.try_clone() else {
        return (1, stop.load(Ordering::SeqCst));
    };
    let mut work = Command::new(&g.work);
    work.arg(g.issue.to_string());
    if restart {
        work.arg("--restart");
    }
    let child = work
        .env("TEHAS_CYCLE_ATTEMPT", attempt)
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .spawn();
    let Ok(mut child) = child else {
        return (127, stop.load(Ordering::SeqCst));
    };
    let mut signalled = false;
    loop {
        if stop.swap(false, Ordering::SeqCst) {
            signalled = true;
            term(child.id());
        }
        match child.try_wait() {
            Ok(Some(s)) => return (exit_code(s), signalled || stop.load(Ordering::SeqCst)),
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => return (127, signalled),
        }
    }
}

/// The report in bash, with this process ignoring what comes while it runs.
fn report_in_bash(g: &Given, s: &Start) -> i32 {
    let ended = [
        s.attempt.to_string(),
        s.rc.to_string(),
        s.started.to_string(),
        u8::from(s.signalled).to_string(),
        u8::from(s.requeued).to_string(),
        s.by.clone(),
        s.how.clone(),
        s.approval_at.clone(),
    ]
    .join("|");
    let done = Command::new(&g.again)
        .arg(g.issue.to_string())
        .env("TEHAS_CYCLE_ENDED", ended)
        .stdin(Stdio::null())
        .status();
    done.map(exit_code).unwrap_or(127)
}

/// A cycle's start, its supervision and its report. Its exit status.
pub fn run(home: &Path, g: &Given) -> i32 {
    let n = g.issue.to_string();
    let answer = match start_on_record(home, &n) {
        Ok(a) => a,
        Err(rc) => return rc,
    };
    let text = |k: &str| {
        answer
            .get(k)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    let attempt = answer
        .get("attempt")
        .map_or(String::new(), Value::to_string);
    if !attempt_shaped(&attempt) {
        log(&format!(
            "ERROR: #{n}: the state store answered the start with no attempt"
        ));
        return 78;
    }
    let approval_at = match answer.get("approval_at") {
        Some(Value::Number(x)) => x.to_string(),
        _ => String::new(),
    };
    let repo = std::env::var("TEHAS_REPO").unwrap_or_default();
    let Some(issue) = ask_github(&repo, &n) else {
        log(&format!(
            "refuse #{n}: GitHub could not be asked what the number is"
        ));
        end_refused(home, &n, &attempt);
        return 65;
    };
    match number(&issue) {
        Number::Open => {}
        kind => {
            log(&match kind {
                Number::Pr => {
                    format!("refuse #{n}: it is a pull request, not an issue the factory works on")
                }
                Number::Closed => format!("refuse #{n}: the issue is closed"),
                _ => format!("refuse #{n}: GitHub's answer for it could not be read"),
            });
            end_refused(home, &n, &attempt);
            return 65;
        }
    }
    let started = time::now();
    let restart = wants_restart(&issue);
    let cycles = home.join("state/cycles");
    for note in NOTES {
        let _ = fs::remove_file(cycles.join(format!("{n}.{note}")));
    }
    let stop = Arc::new(AtomicBool::new(false));
    for sig in [
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGHUP,
    ] {
        if signal_hook::flag::register(sig, Arc::clone(&stop)).is_err() {
            log(&format!("ERROR: #{n}: cannot listen for signal {sig}"));
            end_refused(home, &n, &attempt);
            return 78;
        }
    }
    let (rc, signalled) = supervise(home, g, &attempt, restart, &stop);
    let start = Start {
        issue: g.issue,
        attempt: attempt.parse().unwrap_or(0),
        rc: i64::from(rc),
        started,
        signalled,
        by: text("started_by"),
        how: text("how"),
        approval_at,
        requeued: answer.get("requeued") == Some(&Value::Bool(true)),
        now: None,
        console: g.console.clone(),
        status_cmd: g.status_cmd.clone(),
    };
    if std::env::var("TEHAS_CYCLE_REPORT").as_deref() == Ok("rust") {
        apply::run(home, &start)
    } else {
        report_in_bash(g, &start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_number_is_what_github_answers_for_it() {
        for (answer, want) in [
            (r#"{"number":7,"state":"open"}"#, Number::Open),
            (r#"{"number":7,"state":"OPEN"}"#, Number::Open),
            (r#"{"number":7,"state":"closed"}"#, Number::Closed),
            (r#"{"number":7,"state":"CLOSED"}"#, Number::Closed),
            (r#"{"state":"open","pull_request":{"url":"x"}}"#, Number::Pr),
            (r#"{"state":"closed","pull_request":{}}"#, Number::Pr),
            (r#"{"state":"open","pull_request":null}"#, Number::Open),
            (r#"{"state":"merged"}"#, Number::Unreadable),
            (r#"{"state":7}"#, Number::Unreadable),
            (r#"{}"#, Number::Unreadable),
            ("[]", Number::Unreadable),
            ("", Number::Unreadable),
            ("not json", Number::Unreadable),
            (r#"{"state":"open"}{"state":"open"}"#, Number::Unreadable),
        ] {
            assert_eq!(number(answer), want, "{answer}");
        }
    }

    #[test]
    fn a_restart_is_a_label_of_the_issue() {
        assert!(wants_restart(
            r#"{"labels":[{"name":"x"},{"name":"needs-human-attention"}]}"#
        ));
        assert!(!wants_restart(r#"{"labels":[{"name":"x"}]}"#));
        assert!(!wants_restart(r#"{"labels":null}"#));
        assert!(!wants_restart(r#"{}"#));
        assert!(!wants_restart("not json"));
    }

    #[test]
    fn a_late_start_is_an_open_row_since_this_start_began() {
        let row = |at: i64, ended: Value, attempt: Value| json!({"cycle": {"attempt": attempt, "started_at": at, "ended_at": ended}});
        assert_eq!(
            took_late(&row(100, Value::Null, json!(4)), 100).as_deref(),
            Some("4")
        );
        assert_eq!(took_late(&row(99, Value::Null, json!(4)), 100), None);
        assert_eq!(took_late(&row(100, json!(101), json!(4)), 100), None);
        assert_eq!(took_late(&row(100, Value::Null, json!(0)), 100), None);
        assert_eq!(took_late(&row(100, Value::Null, json!("4")), 100), None);
        assert_eq!(took_late(&json!({"cycle": null}), 100), None);
        assert_eq!(took_late(&json!({}), 100), None);
    }
}
