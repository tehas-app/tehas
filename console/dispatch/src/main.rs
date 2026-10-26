//! tehas-dispatch: the dispatcher's decisions in Rust, beside the bash poll.
//! It reads what one poll would read and prints it, or what it would decide.
//! It writes nothing: no label, no comment, no file, no record, no unit.
//!
//! Usage:
//!   tehas-dispatch snapshot             print the snapshot, as JSON
//!   tehas-dispatch decide [--dry-run]   print the decision of a poll on it:
//!                                       {exit, log, actions, picked, needs,
//!                                       report, demand, gh_requests, seconds}
//!   tehas-dispatch -h|--help            print this text
//!
//! Reads the instance's settings from the environment, as lib/config.sh
//! leaves them (tehasbot loads them), $TEHAS_HOME, and gh, systemctl and
//! $TEHAS_HOME/libexec/store from the PATH and the home.
//!
//! Exit: 0 it printed its answer, whatever the decision says; 1 a read it
//! cannot do without failed, said on standard error; 64 an argument it does
//! not know; 78 TEHAS_HOME or TEHAS_REPO is not set.

use serde_json::{json, Map, Value};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tehas_dispatch::read::build::{fill, snapshot, Sources};
use tehas_dispatch::read::run::Calls;
use tehas_dispatch::{decide, Settings, Snapshot};

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
    let (print_snapshot, dry) = match a.as_slice() {
        ["-h" | "--help"] => {
            println!("{}", usage());
            return;
        }
        ["snapshot"] => (true, false),
        ["decide"] => (false, false),
        ["decide", "--dry-run"] => (false, true),
        _ => {
            eprintln!(
                "tehas-dispatch: unknown invocation: {}\n\n{}",
                args.join(" "),
                usage()
            );
            std::process::exit(64);
        }
    };
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
    env["libexec"] = json!(format!("{home}/libexec"));
    let settings = Settings::parse(&env);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let began = Instant::now();
    let home = PathBuf::from(home);
    let mut src = Sources {
        home: &home,
        calls: Calls {
            bound: Duration::from_secs(30),
            store_bound: Duration::from_secs(
                std::env::var("tehas_store_bound")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(20),
            ),
            gh: "gh".into(),
            store: home.join("libexec/store").to_string_lossy().into_owned(),
            gh_count: 0,
        },
        systemctl: "systemctl".into(),
        now,
        models: tehas_dispatch::read::models::Models::read(),
        endpoint: (String::new(), String::new()),
    };
    let snap = match snapshot(&mut src, &settings) {
        Ok(s) => s,
        Err(f) => {
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
        "needs": d.needs, "report": d.report, "demand": d.demand,
        "gh_requests": src.calls.gh_count,
        "seconds": (began.elapsed().as_millis() as f64 / 1000.0),
    });
    println!("{out}");
}
