//! `--say`: the lines a person reads in place of the JSON answer, for `init`,
//! `migrate` and `import`. install.sh runs the store this way and prints what
//! it says as it stands, so no shell reads the store's JSON.
//!
//! - `init` says `store: made <db>`, `migrate` says
//!   `store: <db> at schema <n>, backup <file>`, and `import` says
//!   `store: imported <table> <n>, ...; moved <n> files to <dir>/` when it
//!   moved any. A failure is the verb's own `store: <word> ...` line and exit.
//! - `import --dry-run` says nothing when the install may go on. It is the
//!   gate before install.sh's first copy, so its refusals are install.sh's own
//!   lines and all of them exit 1: a cycle or planning turn that runs, files the
//!   import would hold, or a store that could not judge them.

use crate::import::{self, TABLES};
use crate::verbs::Fail;
use crate::{db, Verb};
use serde_json::Value;
use std::path::Path;

pub const FLAG: &str = "--say";

/// What one call prints and the status it exits with.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Said {
    pub code: u8,
    pub out: Vec<String>,
    pub err: Vec<String>,
}

/// The arguments without `--say`, and whether it was given. Only the three
/// verbs take it; given twice, the second stays and the verb refuses it.
pub fn strip(args: &[String]) -> (Vec<String>, bool) {
    let takes = matches!(
        args.first().map(String::as_str),
        Some("init" | "migrate" | "import")
    );
    match args.iter().position(|a| a == FLAG) {
        Some(i) if takes => {
            let mut rest = args.to_vec();
            rest.remove(i);
            (rest, true)
        }
        _ => (args.to_vec(), false),
    }
}

/// The lines of one verb's answer.
pub fn say(home: &Path, verb: &Verb, answer: Result<String, Fail>, now: i64) -> Said {
    let state = home.join("state");
    match (verb, answer) {
        (Verb::Import { dry_run: true }, Ok(_)) => Said::default(),
        (Verb::Import { dry_run: true }, Err((_, why))) => {
            let err = if let Some(run) = why.strip_prefix("live ") {
                let issue = run.split(':').next().unwrap_or(run);
                vec![format!(
                    "tehas: a cycle or planning turn runs ({issue}); run tehas drain --wait and let it end, then install again; nothing was installed"
                )]
            } else if why.starts_with("held ") {
                let mut err: Vec<String> =
                    format!("store: {why}").lines().map(String::from).collect();
                err.push(format!(
                    "tehas: the old records listed above cannot be imported; move each one out of {} or mend it, then install again; nothing was installed",
                    state.display()
                ));
                err
            } else {
                let lines: Vec<String> =
                    format!("store: {why}").lines().map(String::from).collect();
                let mut err = lines[lines.len().saturating_sub(5)..].to_vec();
                err.push(format!(
                    "tehas: the store could not judge the old records under {}; nothing was installed",
                    state.display()
                ));
                err
            };
            Said {
                code: 1,
                out: vec![],
                err,
            }
        }
        (_, Err((code, why))) => Said {
            code,
            out: vec![],
            err: vec![format!("store: {why}")],
        },
        (verb, Ok(line)) => {
            let v: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
            let file = db::path(home);
            let out = match verb {
                Verb::Init { .. } => Some(format!("store: made {}", file.display())),
                Verb::Migrate { .. } => Some(format!(
                    "store: {} at schema {}, backup {}",
                    file.display(),
                    shown(&v["to"]),
                    shown(&v["backup"])
                )),
                Verb::Import { .. } => imported(&v).map(|counts| {
                    format!(
                        "store: imported {counts}; moved {} files to {}/imported-{}/",
                        v["moved"],
                        state.display(),
                        import::day(now)
                    )
                }),
                _ => Some(line),
            };
            Said {
                code: 0,
                out: out.into_iter().collect(),
                err: vec![],
            }
        }
    }
}

/// A value of the answer as text, `?` when it is not there.
fn shown(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => "?".to_string(),
    }
}

/// The kinds of row an import made, in the store's order, when it moved any file.
fn imported(v: &Value) -> Option<String> {
    if v["moved"].as_u64().unwrap_or(0) == 0 {
        return None;
    }
    let made: Vec<String> = TABLES
        .iter()
        .filter_map(|t| {
            let n = v["imported"][*t].as_u64().unwrap_or(0);
            (n > 0).then(|| format!("{t} {n}"))
        })
        .collect();
    Some(made.join(", "))
}
