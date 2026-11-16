//! `--say`: the lines a person reads in place of the JSON answer, for `init`
//! and `migrate`. install.sh runs the store this way and prints what it says
//! as it stands, so no shell reads the store's JSON.
//!
//! - `init` says `store: made <db>`, and `migrate` says
//!   `store: <db> at schema <n>, backup <file>`. A failure is the verb's own
//!   `store: <word> ...` line and exit.

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

/// The arguments without `--say`, and whether it was given. Only the two
/// verbs take it; given twice, the second stays and the verb refuses it.
pub fn strip(args: &[String]) -> (Vec<String>, bool) {
    let takes = matches!(args.first().map(String::as_str), Some("init" | "migrate"));
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
pub fn say(home: &Path, verb: &Verb, answer: Result<String, Fail>) -> Said {
    match (verb, answer) {
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
