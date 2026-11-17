//! tehas-dispatch: the dispatcher in Rust.
//!
//! Usage:
//!   tehas-dispatch poll [--dry-run]     one poll, as tehasbot dispatch runs it:
//!                                       it acts. 78 before it when models.json
//!                                       names a bad or unset key
//!   tehas-dispatch refresh              one run of tehas refresh: it
//!                                       writes the refresh requests
//!   tehas-dispatch -h|--help            print this text
//!
//! Reads the instance's settings from the environment, as lib/config.sh
//! leaves them (tehasbot loads them), $TEHAS_HOME, and gh, systemctl, curl,
//! docker and $TEHAS_HOME/libexec/store from the PATH and the home.
//!
//! Exit: poll ends as tehasbot dispatch does; 64 an argument it does not know;
//! 78 a setting tehas_load always leaves set is not set.
//! refresh has its own text and statuses: tehas refresh --help.

use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use tehas_dispatch::Settings;

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
    let dry = match a.as_slice() {
        ["-h" | "--help"] => {
            tehas_core::outln!("{}", usage());
            return;
        }
        ["poll"] => false,
        ["poll", "--dry-run"] => true,
        ["refresh", rest @ ..] => std::process::exit(tehas_dispatch::refresh::main(rest)),
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
    // The settings an instance has to set itself, and the three a load always
    // leaves set that a poll cannot act by without: a run with neither has no
    // settings to decide by, and stops before any call (rule 2). tehas_core
    // names both lists, so the poll holds no copy of either.
    let set = |k: &str| {
        env.get(k)
            .and_then(Value::as_str)
            .is_some_and(|v| !v.is_empty())
    };
    let missing: Vec<&str> = tehas_core::settings::INSTANCE_SETTINGS
        .into_iter()
        .chain(tehas_core::settings::DISPATCHER_ALSO)
        .filter(|k| !set(k))
        .collect();
    if !missing.is_empty() {
        eprintln!(
            "tehas-dispatch: {} not set: load the instance's settings first (lib/config.sh, tehas_load)",
            missing.join(", ")
        );
        std::process::exit(78);
    }
    if let Err(why) = tehas_core::models::host_keys_ok() {
        eprintln!("tehas: {why}");
        std::process::exit(78);
    }
    let mut env = Value::Object(env);
    if let Some(why) = Settings::refused(&env) {
        eprintln!("tehas-dispatch: {why}");
        std::process::exit(78);
    }
    // tehas_branch: the setting, or the checkout's origin/HEAD. tehas_core
    // holds the rule, with the bound on the git read and the line lib/config.sh
    // and tehas work give.
    if !set_in(&env, "TEHAS_BRANCH") {
        match tehas_core::settings::branch() {
            Ok(b) => env["TEHAS_BRANCH"] = json!(b),
            Err(line) => {
                eprintln!("{line}");
                std::process::exit(78);
            }
        }
    }
    // The poll's own libexec, which the refine brief's path is named by.
    env["libexec"] = json!(tehas_dispatch::poll::here(Path::new(&home)));
    let settings = Settings::parse(&env);
    std::process::exit(tehas_dispatch::poll::run_poll(
        &PathBuf::from(&home),
        &settings,
        dry,
    ));
}
