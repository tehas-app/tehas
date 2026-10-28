//! tehas-cycle: a cycle's report in Rust, beside libexec/cycle. It acts on
//! nothing on GitHub or in the state store.
//!
//! Usage:
//!   tehas-cycle report <facts.json>   print the report of an ended cycle,
//!                                     as JSON, from the facts in the file
//!                                     (src/facts.rs) and the answers of
//!                                     GitHub it holds under "github"
//!   tehas-cycle shadow <issue> --attempt <n> --rc <n> --started <s>
//!       --signalled 0|1 --by <login> --how <how> --approval-at <s>
//!       --requeued 0|1
//!                                     just before libexec/cycle reports:
//!                                     decide the report from the host and
//!                                     keep it in state/cycles/shadow-<issue>.json
//!   tehas-cycle compare <issue>       after the report: set the bash's
//!                                     beside it and add one line to
//!                                     logs/cycle-shadow.log
//!   tehas-cycle last-attempt <issue> <result.json> <workspace>
//!                                     the issue as GitHub's REST API answers
//!                                     it on standard input: what a cycle
//!                                     that ended at child-timeout:develop
//!                                     does with its body, as JSON
//!                                     {"action": "none"|"edit"|"repeat",
//!                                     "body": <the body to write>,
//!                                     "say": <a repeat's words for the park>}
//!                                     (src/last_attempt.rs)
//!   tehas-cycle -h|--help             print this text
//!
//! shadow and compare do nothing while $TEHAS_HOME/state/cycle-shadow.on is
//! not there. They read $TEHAS_HOME, TEHAS_REPO and TEHAS_WORK_DIR from the
//! environment, as tehas_load leaves them, and gh and the store.
//!
//! last-attempt reads what it is given and changes nothing.
//!
//! Exit: 0 it printed or kept its answer; 1 what it reads could not be
//! read, said on standard error; 64 an argument it does not know.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tehas_cycle::host::Start;
use tehas_cycle::last_attempt::{self, Action};
use tehas_cycle::report::{to_json, Given};
use tehas_cycle::{report, shadow, Facts};

fn usage() -> String {
    include_str!("main.rs")
        .lines()
        .skip(2)
        .take_while(|l| l.starts_with("//!"))
        .map(|l| l.trim_start_matches("//!").strip_prefix(' ').unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

fn wrong(args: &[String]) -> ! {
    eprintln!(
        "tehas-cycle: unknown invocation: {}\n\n{}",
        args.join(" "),
        usage()
    );
    std::process::exit(64);
}

fn fail(e: String) -> ! {
    eprintln!("tehas-cycle: {e}");
    std::process::exit(1);
}

fn issue(s: &str, args: &[String]) -> u64 {
    s.parse()
        .ok()
        .filter(|n| *n > 0)
        .unwrap_or_else(|| wrong(args))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let home = PathBuf::from(std::env::var("TEHAS_HOME").unwrap_or_default());
    let on = || home.join("state/cycle-shadow.on").exists();
    match a.as_slice() {
        ["-h" | "--help"] => println!("{}", usage()),
        ["report", file] => print_report(file),
        ["last-attempt", n, result, workspace] => {
            last_attempt(issue(n, &args), result, Path::new(workspace))
        }
        ["shadow", n, rest @ ..] => {
            let n = issue(n, &args);
            let mut start = Start {
                issue: n,
                attempt: 0,
                rc: 0,
                started: 0,
                signalled: false,
                by: String::new(),
                how: String::new(),
                approval_at: String::new(),
                requeued: false,
            };
            let mut it = rest.iter();
            while let Some(flag) = it.next() {
                let v = it.next().unwrap_or_else(|| wrong(&args)).to_string();
                let num = |v: &str| v.parse::<i64>().unwrap_or_else(|_| wrong(&args));
                match *flag {
                    "--attempt" => {
                        start.attempt = num(&v).try_into().unwrap_or_else(|_| wrong(&args))
                    }
                    "--rc" => start.rc = num(&v),
                    "--started" => start.started = num(&v),
                    "--signalled" => start.signalled = v == "1",
                    "--by" => start.by = v,
                    "--how" => start.how = v,
                    "--approval-at" => start.approval_at = v,
                    "--requeued" => start.requeued = v == "1",
                    _ => wrong(&args),
                }
            }
            if start.attempt == 0 {
                wrong(&args);
            }
            if on() {
                shadow::begin(&home, &start).unwrap_or_else(|e| fail(e));
            }
        }
        ["compare", n] => {
            let n = issue(n, &args);
            if on() {
                if let Some(line) = shadow::compare(&home, n).unwrap_or_else(|e| fail(e)) {
                    println!("{line}");
                }
            }
        }
        _ => wrong(&args),
    }
}

fn print_report(file: &str) {
    let v: Value = std::fs::read_to_string(file)
        .map_err(|e| e.to_string())
        .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))
        .unwrap_or_else(|e| fail(format!("the facts in {file} cannot be read: {e}")));
    let facts = Facts::from_json(&v).unwrap_or_else(|e| fail(e));
    let mut gh = Given(v.get("github").cloned().unwrap_or(json!({})));
    let r = report(&facts, &mut gh);
    println!("{}", to_json(&r, facts.issue));
}

fn last_attempt(n: u64, result: &str, workspace: &Path) {
    let mut stdin = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut stdin)
        .unwrap_or_else(|e| fail(format!("the issue cannot be read: {e}")));
    let issue: Value = serde_json::from_str(&stdin)
        .unwrap_or_else(|e| fail(format!("the issue cannot be read: {e}")));
    let r: Value = std::fs::read_to_string(result)
        .map_err(|e| e.to_string())
        .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))
        .unwrap_or_else(|e| fail(format!("the result in {result} cannot be read: {e}")));
    let entry = r
        .get("issues")
        .and_then(Value::as_array)
        .and_then(|l| {
            l.iter()
                .find(|e| e.get("issue").and_then(Value::as_u64) == Some(n))
        })
        .cloned()
        .unwrap_or(Value::Null);
    let body = match issue.get("body") {
        Some(Value::String(b)) => b.as_str(),
        Some(Value::Null) => "",
        _ => fail("the issue has no body".to_string()),
    };
    let action = last_attempt::decide(body, &entry, last_attempt::state(workspace, n).as_ref());
    println!(
        "{}",
        match action {
            Action::Nothing => json!({"action": "none"}),
            Action::Edit(body) => json!({"action": "edit", "body": body}),
            Action::Repeat => json!({"action": "repeat", "say": last_attempt::REPEAT}),
        }
    );
}
