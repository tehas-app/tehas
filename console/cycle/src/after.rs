//! What follows a cycle's record: the live view, the run and its end in the
//! analytics, the improvement record, the build output of its workspace, the
//! status, and the next poll. Each step's failure is a WARN line. None changes
//! how the cycle ended, and every step runs whatever the one before it did.
//!
//! The approval is the one step before the end: `cycle-end` removes the
//! record of a start that never ran, so `approved` reads it first.

use crate::host;
use crate::improve::{self, Ended};
use crate::jq::J;
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tehas_core::time::{iso, now};

/// A call of the analytics may take this long: a stopped cycle has
/// TimeoutStopSec for its whole report, and two calls are part of it.
const ANALYTICS_LIMIT: u32 = 5;

/// A line of the cycle's log, as its bash writes one.
pub fn log(line: &str) {
    let _ = writeln!(std::io::stdout(), "{} {line}", iso(now()));
}

#[derive(Clone, Debug, Default)]
pub struct After {
    pub ended: Ended,
    pub attempt: u64,
    pub park_reason: String,
    pub failure_kind: String,
    pub failure_cause: String,
    pub provider: bool,
    pub parked: bool,
    pub by: String,
    pub how: String,
    /// What `approved` printed.
    pub approved: String,
    pub image_built: String,
    pub forks: String,
    pub signalled: bool,
    pub workdir: String,
    pub console: String,
    pub status_cmd: String,
}

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}

fn analytics_bin(home: &Path) -> PathBuf {
    home.join("libexec/analytics")
}

fn analytics_on(home: &Path) -> bool {
    let bin = analytics_bin(home);
    env("TEHAS_ANALYTICS") == "on"
        && fs::metadata(bin).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

fn analytics_wait() -> u32 {
    let wait = env("TEHAS_ANALYTICS_WAIT");
    let digits = (1..=3).contains(&wait.len())
        && !wait.starts_with('0')
        && wait.bytes().all(|b| b.is_ascii_digit());
    if digits {
        wait.parse().unwrap_or(ANALYTICS_LIMIT)
    } else {
        ANALYTICS_LIMIT
    }
}

/// tehas_analytics: one call, bounded, its failure one line.
fn analytics(home: &Path, args: &[&str]) {
    if !analytics_on(home) {
        return;
    }
    let got = Command::new("timeout")
        .args(["-k", "1", &analytics_wait().to_string()])
        .arg(analytics_bin(home))
        .args(args)
        .env("TEHAS_HOME", home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .status();
    let rc = got.map_or(127, |s| s.code().unwrap_or(137));
    if rc != 0 {
        log(&format!(
            "WARN analytics {}: exit {rc}",
            args.first().copied().unwrap_or("")
        ));
    }
}

/// `{at, how, driver}` of the approval an attempt used, null when there is
/// none or it cannot be read: of the store's answer to approval-current.
pub fn approval_of(answer: &Value, attempt: u64) -> J {
    let time = |v: &Value| v.as_f64().map(|t| iso(t.floor() as i64));
    let Some(c) = answer.get("current").filter(|c| !c.is_null()) else {
        return J::null();
    };
    if c.get("used_by").and_then(Value::as_u64) != Some(attempt) || !c.is_object() {
        return J::null();
    }
    let rests = answer.get("rests_on").filter(|r| !r.is_null());
    let (Some(mut at), rests_at) = (
        c.get("at").and_then(time),
        rests.map(|r| r.get("at").and_then(time)),
    ) else {
        return J::null();
    };
    if rests_at == Some(None) {
        return J::null();
    }
    let how = match c.get("how") {
        Some(Value::String(h)) if h == "millwright" => J::str("fixer"),
        Some(h) => J::from(h),
        None => J::null(),
    };
    if how == J::str("refresh") {
        if let Some(Some(base)) = rests_at {
            at = base;
        }
    }
    let driver = c.pointer("/input/driver").map_or(J::null(), J::from);
    J::obj(vec![("at", J::Str(at)), ("how", how), ("driver", driver)])
}

/// What `tehas-cycle approved` prints.
pub fn approved(home: &Path, issue: u64, attempt: u64) -> String {
    if !analytics_on(home) {
        return "null".into();
    }
    match host::store(home, &["approval-current", &issue.to_string()]) {
        Ok(a) => approval_of(&a, attempt).compact(),
        Err(_) => "null".into(),
    }
}

/// The analytics' line of the run. A field with nothing in it is left out.
pub fn history_line(a: &After, at: &str) -> J {
    let e = &a.ended;
    let mut f: Vec<(String, J)> = vec![
        ("at".into(), J::str(at)),
        ("started".into(), J::int(e.started)),
        ("rc".into(), J::int(e.rc)),
    ];
    let mut text = |k: &str, v: &str| {
        if !v.is_empty() {
            f.push((k.into(), J::str(v)));
        }
    };
    text("status", &e.status);
    text("step", &e.step);
    text("cap", &e.cap);
    text("park_reason", &a.park_reason);
    text("failure_kind", &a.failure_kind);
    text("failure_cause", &a.failure_cause);
    f.push(("provider".into(), J::bool(a.provider)));
    f.push(("parked".into(), J::bool(a.parked)));
    let mut text = |k: &str, v: &str| {
        if !v.is_empty() {
            f.push((k.into(), J::str(v)));
        }
    };
    text("pr", &e.pr);
    text("by", &a.by);
    text("how", &a.how);
    J::Obj(f)
}

/// The facts of a run: {line, attempt, approval, settings, rukas}. The
/// settings and the commit are of this attempt's record, null when the store
/// has none or could not answer.
pub fn run_facts(line: &J, attempt: Option<u64>, approved: &str, record: Option<&Value>) -> J {
    let cycle = record.and_then(|r| r.get("cycle")).filter(|c| {
        !c.is_null() && attempt.is_some() && c.get("attempt").and_then(Value::as_u64) == attempt
    });
    let settings = cycle
        .and_then(improve::settings)
        .map_or(J::null(), |(c, h, n)| {
            J::obj(vec![("container", c), ("host", h), ("concurrentCycles", n)])
        });
    let rukas = cycle
        .and_then(|c| c.get("rukas_commit"))
        .map_or(J::null(), J::from);
    J::obj(vec![
        ("line", line.clone()),
        ("attempt", attempt.map_or(J::null(), |n| J::int(n as i64))),
        ("approval", J::parse(approved).unwrap_or(J::null())),
        ("settings", settings),
        ("rukas", rukas),
    ])
}

/// The analytics' run: the facts in a new 0600 file for the call.
fn analytics_run(home: &Path, a: &After, record: &Result<Value, String>) {
    if !analytics_on(home) {
        return;
    }
    let e = &a.ended;
    let at = iso(now());
    let facts = run_facts(
        &history_line(a, &at),
        Some(a.attempt),
        &a.approved,
        record.as_ref().ok(),
    );
    let dir = home.join("tmp");
    if !dir.exists() {
        let _ = fs::DirBuilder::new().mode(0o700).create(&dir);
    }
    let real_dir = fs::symlink_metadata(&dir).is_ok_and(|m| m.is_dir());
    let file = real_dir
        .then(|| improve::private_file(&dir, "analytics-facts.", &facts.compact()))
        .flatten();
    let Some(file) = file else {
        log("WARN analytics run: no facts written");
        return;
    };
    let path = file.to_string_lossy().into_owned();
    let n = e.issue.to_string();
    let mut args: Vec<&str> = vec!["run", &n, "--facts", &path];
    if !e.image.is_empty() {
        args.extend(["--image", &e.image]);
    }
    if !a.image_built.is_empty() {
        args.extend(["--image-built", &a.image_built]);
    }
    if !e.rukas.is_empty() {
        args.extend(["--rukas-image", &e.rukas]);
    }
    args.extend(["--oom", &e.kills, "--forks", &a.forks]);
    analytics(home, &args);
    let _ = fs::remove_file(&file);
}

/// The steps, in their order.
pub fn after(home: &Path, a: &After) {
    let e = &a.ended;
    let n = e.issue;
    if let Err(f) = host::store_try(home, 3, &["publish"]) {
        log(&format!(
            "WARN: the live view is not published after the end of #{n} ({}: {})",
            f.word, f.line
        ));
    }
    let wanted = analytics_on(home) || !env("TEHAS_IMPROVE_REPO").is_empty();
    let record = if wanted {
        host::store(home, &["cycle-show", &n.to_string()])
    } else {
        Err("not read".to_string())
    };
    analytics_run(home, a, &record);
    analytics(home, &["event", &n.to_string(), "ended"]);
    if !env("TEHAS_IMPROVE_REPO").is_empty() {
        if let Err(said) = improve::write(home, e, &record, now()) {
            if let Some(line) = said {
                eprintln!("{line}");
            }
            log(&format!("WARN: no record of #{n} for the improvement loop"));
        }
    }
    clean(home, a);
    if !a.signalled {
        let _ = Command::new(&a.status_cmd).status();
    }
    next_poll(home);
}

/// Nothing builds in a kept workspace again, and the cycle still holds its
/// lock.
fn clean(home: &Path, a: &After) {
    let scratch = match (env("TEHAS_SCRATCH_DIR"), a.workdir.rsplit('/').next()) {
        (dir, Some(name)) if !dir.is_empty() => format!("{dir}/{name}"),
        _ => String::new(),
    };
    let clean_log = home.join("logs/clean.log").to_string_lossy().into_owned();
    let cleaned = Command::new("timeout")
        .args(["-k", "5", "300", &a.console, "clean", "ended", &a.workdir])
        .args([&env("TEHAS_CLEAN_DIRS"), &clean_log, &scratch])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if !cleaned.is_ok_and(|s| s.success()) {
        log(&format!(
            "WARN: the build output of {} was not removed",
            a.workdir
        ));
    }
}

/// The slot is free now: fill it without waiting for the timer. systemd takes
/// a start asked while a poll runs as that poll, which may have counted this
/// cycle as running, so the file is left too: tehas-dispatch.path starts the
/// next poll when that one ends. The start is for a host where the path unit
/// is not on. systemctl does not get the cycle's lock, fd 9: closing a
/// descriptor Rust did not open is unsafe, so sh does it.
fn next_poll(home: &Path) {
    let again = home.join("state/dispatch.again");
    if fs::write(&again, "").is_err() {
        log(&format!("WARN: could not write {}", again.display()));
    }
    let started = Command::new("sh")
        .args(["-c", "exec \"$@\" 9>&-", "sh"])
        .args(["timeout", "-k", "5", "10", "systemctl", "--user", "start"])
        .args(["--no-block", "tehas-dispatch.service"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if !started.is_ok_and(|s| s.success()) {
        log("WARN: could not start a poll of the dispatcher");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_approval_of_an_attempt_is_read_as_the_store_shape_gave_it() {
        let a = json!({"current": {"at": 1792497600, "how": "millwright", "used_by": 2, "input": {"driver": "lean"}}});
        assert_eq!(
            approval_of(&a, 2).compact(),
            r#"{"at":"2026-10-20T12:00:00Z","how":"fixer","driver":"lean"}"#
        );
        assert_eq!(approval_of(&a, 3), J::null());
        assert_eq!(approval_of(&json!({"current": null}), 1), J::null());
    }

    #[test]
    fn a_refresh_is_on_record_at_the_time_of_the_approval_it_rests_on() {
        let a = json!({"current": {"at": 1792497600, "how": "refresh", "used_by": 1},
                       "rests_on": {"at": 1792411200, "by": "alice"}});
        assert_eq!(
            approval_of(&a, 1).compact(),
            r#"{"at":"2026-10-19T12:00:00Z","how":"refresh","driver":null}"#
        );
    }

    #[test]
    fn a_time_that_is_no_number_is_no_approval() {
        let a = json!({"current": {"at": "soon", "how": "label", "used_by": 1}});
        assert_eq!(approval_of(&a, 1), J::null());
        let b =
            json!({"current": {"at": 1, "how": "label", "used_by": 1}, "rests_on": {"at": "x"}});
        assert_eq!(approval_of(&b, 1), J::null());
    }

    fn ended() -> After {
        After {
            ended: Ended {
                started: 100,
                rc: 2,
                status: "failed".into(),
                step: "develop".into(),
                ..Ended::default()
            },
            provider: true,
            by: "alice".into(),
            ..After::default()
        }
    }

    #[test]
    fn the_line_of_a_run_keeps_the_order_and_leaves_out_what_is_empty() {
        assert_eq!(
            history_line(&ended(), "T").compact(),
            r#"{"at":"T","started":100,"rc":2,"status":"failed","step":"develop","provider":true,"parked":false,"by":"alice"}"#
        );
    }

    #[test]
    fn an_attempt_that_is_no_number_makes_every_fact_of_the_record_null() {
        let rec = json!({"cycle": {"attempt": 1, "rukas_commit": "abcdef1", "settings": {"TEHAS_X": "1"}}});
        let line = history_line(&ended(), "T");
        assert_eq!(
            run_facts(&line, None, "null", Some(&rec)).compact(),
            format!(
                r#"{{"line":{},"attempt":null,"approval":null,"settings":null,"rukas":null}}"#,
                line.compact()
            )
        );
    }

    #[test]
    fn a_line_longer_than_one_argument_may_be_is_given_whole() {
        let mut a = ended();
        a.failure_cause = "x".repeat(200_000);
        let facts = run_facts(&history_line(&a, "T"), Some(1), "null", None).compact();
        let back: Value = serde_json::from_str(&facts).unwrap();
        assert_eq!(back["line"]["failure_cause"].as_str().unwrap().len(), 200_000);
    }

    #[test]
    fn the_record_of_another_attempt_gives_no_facts() {
        let rec = json!({"cycle": {"attempt": 1, "rukas_commit": "abcdef1", "settings": {"TEHAS_X": "1"}}});
        let line = J::parse("{}").unwrap();
        let other = run_facts(&line, Some(2), "null", Some(&rec)).compact();
        assert_eq!(
            other,
            r#"{"line":{},"attempt":2,"approval":null,"settings":null,"rukas":null}"#
        );
        let this = run_facts(&line, Some(1), "null", Some(&rec)).compact();
        assert_eq!(
            this,
            r#"{"line":{},"attempt":1,"approval":null,"settings":{"container":{},"host":{"TEHAS_X":"1"},"concurrentCycles":null},"rukas":"abcdef1"}"#
        );
    }

    #[test]
    fn the_wait_of_the_analytics_is_a_small_number_or_the_limit() {
        std::env::set_var("TEHAS_ANALYTICS_WAIT", "soon");
        assert_eq!(analytics_wait(), 5);
        std::env::set_var("TEHAS_ANALYTICS_WAIT", "7");
        assert_eq!(analytics_wait(), 7);
        std::env::remove_var("TEHAS_ANALYTICS_WAIT");
    }
}
