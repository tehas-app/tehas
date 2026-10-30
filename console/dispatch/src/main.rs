//! tehas-dispatch: the dispatcher's decisions in Rust, beside the bash poll.
//! It reads what one poll would read and prints it, or what it would decide,
//! and writes nothing but the shadow's two files below. `poll` is the one
//! command that acts.
//!
//! Usage:
//!   tehas-dispatch snapshot             print the snapshot, as JSON
//!   tehas-dispatch decide [--dry-run]   print the decision of a poll on it:
//!                                       {exit, log, actions, picked, needs,
//!                                       report, demand, gh_requests, seconds}
//!   tehas-dispatch shadow               decide as a real poll would, just
//!                                       before one, and keep the decision in
//!                                       state/dispatch-shadow.json
//!   tehas-dispatch compare <lines> [--exit <status>]
//!                                       after the poll: set the decision kept
//!                                       beside the poll's lines (the file
//!                                       <lines>) and its exit status, and add
//!                                       one JSON line to
//!                                       logs/dispatch-shadow.log
//!   tehas-dispatch poll [--dry-run]     one poll, as libexec/dispatch runs it:
//!                                       it acts. Nothing chooses it yet
//!                                       (#254 parts 4b and 4c)
//!   tehas-dispatch -h|--help            print this text
//!
//! Reads the instance's settings from the environment, as lib/config.sh
//! leaves them (tehasbot loads them), $TEHAS_HOME, and gh, systemctl, curl,
//! docker and $TEHAS_HOME/libexec/store from the PATH and the home. While
//! state/dispatch-shadow.on is not there, shadow and compare do nothing: the
//! shadow is opt-in, for the host where it is being proven.
//!
//! Exit: 0 it printed or kept its answer, whatever the decision says; 1 a
//! read it cannot do without failed, said on standard error; 64 an argument
//! it does not know; 78 a setting tehas_load always leaves set is not set.

use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tehas_dispatch::read::build::{fill, snapshot, Sources};
use tehas_dispatch::read::run::Calls;
use tehas_dispatch::{decide, Settings, Snapshot};
use tehas_core::hold::Report;

fn usage() -> String {
    include_str!("main.rs")
        .lines()
        .skip(2)
        .take_while(|l| l.starts_with("//!"))
        .map(|l| l.trim_start_matches("//!").strip_prefix(' ').unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

fn set_in(env: &Value, k: &str) -> bool {
    env.get(k)
        .and_then(Value::as_str)
        .is_some_and(|v| !v.is_empty())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let (print_snapshot, dry, keep, poll) = match a.as_slice() {
        ["-h" | "--help"] => {
            println!("{}", usage());
            return;
        }
        ["snapshot"] => (true, false, false, false),
        ["decide"] => (false, false, false, false),
        ["decide", "--dry-run"] => (false, true, false, false),
        ["shadow"] => (false, false, true, false),
        ["poll"] => (false, false, false, true),
        ["poll", "--dry-run"] => (false, true, false, true),
        ["compare", lines] => return compare(lines, None),
        ["compare", lines, "--exit", rc] => return compare(lines, rc.parse().ok()),
        _ => {
            eprintln!(
                "tehas-dispatch: unknown invocation: {}\n\n{}",
                args.join(" "),
                usage()
            );
            std::process::exit(64);
        }
    };
    if keep && !on() {
        return;
    }
    let env: Map<String, Value> = std::env::vars().map(|(k, v)| (k, json!(v))).collect();
    let home = env
        .get("TEHAS_HOME")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    // What lib/config.sh's tehas_load always leaves set: a run without one
    // has no settings to decide by, and stops before any call (rule 2).
    let set = |k: &str| {
        env.get(k)
            .and_then(Value::as_str)
            .is_some_and(|v| !v.is_empty())
    };
    let missing: Vec<&str> = [
        "TEHAS_HOME",
        "TEHAS_REPO",
        "TEHAS_INSTANCE_DIR",
        "TEHAS_CONTROL_ISSUE",
        "TEHAS_ALLOWED_LABELERS",
        "TEHAS_TICKET_CONCURRENCY",
    ]
    .into_iter()
    .filter(|k| !set(k))
    .collect();
    if !missing.is_empty() {
        eprintln!(
            "tehas-dispatch: {} not set: load the instance's settings first (lib/config.sh, tehas_load)",
            missing.join(", ")
        );
        std::process::exit(78);
    }
    let mut env = Value::Object(env);
    // tehas_branch: the setting, or the checkout's origin/HEAD.
    if !set_in(&env, "TEHAS_BRANCH") {
        let checkout = env["TEHAS_WORK_REPO"]
            .as_str()
            .unwrap_or("/nonexistent")
            .to_string();
        let r = tehas_dispatch::read::run::run(
            "git",
            &[
                "-C".into(),
                checkout.clone(),
                "symbolic-ref".into(),
                "-q".into(),
                "--short".into(),
                "refs/remotes/origin/HEAD".into(),
            ],
            Duration::from_secs(30),
        );
        let b = r.out.trim().trim_start_matches("origin/").to_string();
        if !r.ok() || b.is_empty() {
            eprintln!(
                "tehas-dispatch: the default branch of {checkout} is unknown: run install.sh, or set TEHAS_BRANCH in {}/instance.env",
                env["TEHAS_INSTANCE_DIR"].as_str().unwrap_or("?")
            );
            std::process::exit(78);
        }
        env["TEHAS_BRANCH"] = json!(b);
    }
    // The poll's own libexec, which the refine brief's path is named by.
    env["libexec"] = json!(tehas_dispatch::poll::here(Path::new(&home)));
    let settings = Settings::parse(&env);
    if poll {
        std::process::exit(tehas_dispatch::poll::run_poll(
            &PathBuf::from(&home),
            &settings,
            dry,
        ));
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let began = Instant::now();
    let home = PathBuf::from(home);
    let mut src = Sources {
        home: &home,
        calls: Calls::idle(
            Duration::from_secs(
                std::env::var("tehas_store_bound")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(20),
            ),
            home.join("libexec/store").to_string_lossy().into_owned(),
        ),
        systemctl: "systemctl".into(),
        now,
        models: tehas_dispatch::read::models::Models::read(),
        endpoint: (String::new(), String::new()),
        gate: false,
    };
    let snap = match snapshot(&mut src, &settings) {
        Ok(s) => s,
        Err(f) => {
            if keep {
                // The decision of a poll whose read failed: that line, exit 1.
                save(
                    &home,
                    &json!({"exit": 1, "log": [f.0], "actions": [],
                    "gh_requests": src.calls.gh_count, "seconds": began.elapsed().as_secs_f64()}),
                );
                return;
            }
            eprintln!("{}", f.0);
            std::process::exit(1);
        }
    };
    if print_snapshot {
        println!("{snap}");
        return;
    }
    let mut snap = snap;
    let mut d = decide(&Snapshot::parse(&snap), &settings, dry);
    // What the decision asked for and the poll reads only when asked: read
    // it, and decide again.
    while fill(&src, &mut snap, &d.needs) {
        d = decide(&Snapshot::parse(&snap), &settings, dry);
    }
    let out = json!({
        "exit": d.exit, "log": d.log, "actions": d.actions, "picked": d.picked,
        "needs": d.needs, "report": d.report.as_ref().map(Report::to_value),
        "demand": d.demand,
        "gh_requests": src.calls.gh_count,
        "seconds": (began.elapsed().as_millis() as f64 / 1000.0),
    });
    if keep {
        save(&home, &out);
    } else {
        println!("{out}");
    }
}

/// $TEHAS_HOME, or ~/tehas as env.sh gives it.
fn shadow_home() -> PathBuf {
    match std::env::var("TEHAS_HOME") {
        Ok(h) if !h.is_empty() => PathBuf::from(h),
        _ => PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("tehas"),
    }
}

/// state/dispatch-shadow.on starts the shadow; without it, it costs nothing.
fn on() -> bool {
    shadow_home().join("state/dispatch-shadow.on").exists()
}

/// The decision kept for compare, written whole or not at all.
fn save(home: &std::path::Path, decision: &Value) {
    let path = home.join("state/dispatch-shadow.json");
    let new = path.with_extension("json.new");
    if std::fs::write(&new, decision.to_string()).is_ok() {
        let _ = std::fs::rename(&new, &path);
    }
}

/// compare: the decision kept, set beside the poll's lines, one line of the
/// shadow's log. The decision is used once.
fn compare(lines: &str, exit: Option<i64>) {
    if !on() {
        return;
    }
    let home = shadow_home();
    let kept = home.join("state/dispatch-shadow.json");
    let decision: Value = std::fs::read_to_string(&kept)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(
            || json!({"exit": null, "log": ["the shadow decided nothing before this poll"]}),
        );
    let _ = std::fs::remove_file(&kept);
    let poll = std::fs::read_to_string(lines).unwrap_or_default();
    let time = tehas_dispatch::decide::iso_now();
    let line = tehas_dispatch::shadow::compare(&decision, &poll, exit, &time);
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(home.join("logs/dispatch-shadow.log"))
    {
        let _ = writeln!(f, "{line}");
    }
}
