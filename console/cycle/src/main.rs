//! tehas-cycle: a cycle's report in Rust, beside libexec/cycle. It decides
//! and writes nothing.
//!
//! Usage:
//!   tehas-cycle report <facts.json>   print the report of an ended cycle,
//!                                     as JSON, from the facts in the file
//!                                     (src/facts.rs) and the answers of
//!                                     GitHub it holds under "github"
//!   tehas-cycle -h|--help             print this text
//!
//! Exit: 0 it printed the report; 1 the facts could not be read, said on
//! standard error; 64 an argument it does not know.

use serde_json::{json, Value};
use tehas_cycle::report::{Action, Given};
use tehas_cycle::{report, Facts};

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
    let file = match a.as_slice() {
        ["-h" | "--help"] => {
            println!("{}", usage());
            return;
        }
        ["report", file] => *file,
        _ => {
            eprintln!(
                "tehas-cycle: unknown invocation: {}\n\n{}",
                args.join(" "),
                usage()
            );
            std::process::exit(64);
        }
    };
    let v: Value = match std::fs::read_to_string(file)
        .map_err(|e| e.to_string())
        .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))
    {
        Ok(v) => v,
        Err(e) => {
            eprintln!("tehas-cycle: the facts in {file} cannot be read: {e}");
            std::process::exit(1);
        }
    };
    let facts = match Facts::from_json(&v) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("tehas-cycle: {e}");
            std::process::exit(1);
        }
    };
    let mut gh = Given(v.get("github").cloned().unwrap_or(json!({})));
    let r = report(&facts, &mut gh);
    println!("{}", to_json(&r, facts.issue));
}

/// The report as JSON. A park is shown with its question, and with the
/// comment it posts when the question is not posted or the park not taken.
fn to_json(r: &tehas_cycle::Report, issue: u64) -> Value {
    let actions: Vec<Value> = r
        .actions
        .iter()
        .map(|a| match a {
            Action::Relabel { remove, add } => json!({"relabel": {"remove": remove, "add": add}}),
            Action::Comment { ok, stuck } => json!({"comment": {"ok": ok, "stuck": stuck}}),
            Action::Park(p) => json!({"park": {
                "args": p.args,
                "question": p.question(issue, false).1,
                "comment": p.comment(false, true),
                "comment_unrecorded": p.comment(false, false),
            }}),
            Action::PrComment { pr, body } => json!({"pr_comment": {"pr": pr, "body": body}}),
            Action::Tidy { remove, warn } => json!({"tidy": {"remove": remove, "warn": warn}}),
            Action::RefreshOff { pr } => json!({"refresh_off": pr}),
        })
        .collect();
    json!({
        "log": r.log, "actions": actions, "done": r.done, "end": r.end, "files": r.files,
        "history": r.history, "analytics": r.analytics, "improve": r.improve,
        "status_run": r.status_run, "asked": r.asked,
    })
}
