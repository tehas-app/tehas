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
use tehas_dispatch::read::build::{snapshot, Sources};
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
    let repo = env.get("TEHAS_REPO").and_then(Value::as_str).unwrap_or("");
    if home.is_empty() || repo.is_empty() {
        eprintln!("tehas-dispatch: TEHAS_HOME and TEHAS_REPO must be set (tehasbot loads them)");
        std::process::exit(78);
    }
    let mut env = Value::Object(env);
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
            gh: "gh".into(),
            store: home.join("libexec/store").to_string_lossy().into_owned(),
            gh_count: 0,
        },
        systemctl: "systemctl".into(),
        now,
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
    let d = decide(&Snapshot::parse(&snap), &settings, dry);
    let out = json!({
        "exit": d.exit, "log": d.log, "actions": d.actions, "picked": d.picked,
        "needs": d.needs, "report": d.report, "demand": d.demand,
        "gh_requests": src.calls.gh_count,
        "seconds": (began.elapsed().as_millis() as f64 / 1000.0),
    });
    println!("{out}");
}
