//! `tehas blockers [--json]`: the holds between issues the dispatcher's last
//! poll read, each cycle among them, and for each issue a chain holds, the
//! issues at its ends and what holds each. From state/starts.json
//! (tehas_core::starts); the graph is tehas_core::blockers. No network, and
//! it writes nothing.
//!
//! Exit: 0 shown; 1 the record cannot be read; 3 no poll has written one
//! yet; 64 an argument other than `--json`; 78 the settings are missing or
//! malformed.

use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::env;
use std::path::PathBuf;
use tehas_core::blockers::analyse;
use tehas_core::starts::{self, code, Edge, Reason, Record};

const USAGE: &str = "usage: tehas blockers [--json]";

/// Whether the arguments ask for `--json`, or 64 with the usage.
pub fn words(given: &[String]) -> Result<bool, u8> {
    match given {
        [] => Ok(false),
        [flag] if flag == "--json" => Ok(true),
        _ => {
            eprintln!("{USAGE}");
            Err(64)
        }
    }
}

/// What holds `n` at the end of a chain.
fn end_of(record: &Record, n: u64) -> Value {
    let links = record.edges.iter().find_map(|e| match e {
        Edge::Link { issue, open } if *issue == n => Some(*open),
        _ => None,
    });
    let reason = match (links, record.verdict(n)) {
        (Some(open), _) => Reason::new(
            code::BLOCKED_LINK,
            format!("{open} blocked-by links of it are open; which issues they name is not read"),
        ),
        (None, Some(None)) => return json!({"issue": n, "start": true}),
        (None, _) => record
            .hold_of(n)
            .cloned()
            .unwrap_or_else(|| Reason::new(code::OTHER, "the poll did not read what holds it")),
    };
    json!({"issue": n, "start": false, "reason": reason.to_value()})
}

/// The object `--json` prints of a record, `api` and `state` first.
pub fn object(at: &str, record: &Record) -> Value {
    let marked: Vec<(u64, u64)> = record
        .edges
        .iter()
        .filter_map(|e| match e {
            Edge::Marker { issue, on, .. } => Some((*issue, *on)),
            Edge::Link { .. } => None,
        })
        .collect();
    let linked: BTreeSet<u64> = record
        .edges
        .iter()
        .filter_map(|e| match e {
            Edge::Link { issue, .. } => Some(*issue),
            Edge::Marker { .. } => None,
        })
        .collect();
    let g = analyse(&marked, &linked);
    let chains: Vec<Value> = g
        .ends
        .iter()
        .map(|(n, ends)| {
            let ends: Vec<Value> = ends.iter().map(|e| end_of(record, *e)).collect();
            json!({"issue": n, "cycle": g.reach_cycle.contains(n), "ends": ends})
        })
        .collect();
    let edges: Vec<Value> = record.edges.iter().map(Edge::to_value).collect();
    json!({"api": 1, "state": "ok", "at": at, "edges": edges, "cycles": g.cycles, "chains": chains})
}

/// The same for a person, one line a chain.
fn said(v: &Value) -> String {
    let mut out = Vec::new();
    let nums = |a: &Value| -> String {
        a.as_array()
            .into_iter()
            .flatten()
            .map(|n| format!("#{n}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    for c in v["cycles"].as_array().into_iter().flatten() {
        out.push(format!(
            "cycle     {}: none of them starts until one is unblocked",
            nums(c)
        ));
    }
    for c in v["chains"].as_array().into_iter().flatten() {
        let ends: Vec<String> = c["ends"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|e| match e["reason"]["text"].as_str() {
                Some(t) if e["issue"] == c["issue"] => t.to_string(),
                Some(t) => format!("#{}: {t}", e["issue"]),
                None => format!("#{} starts", e["issue"]),
            })
            .collect();
        let mut line = format!("#{} waits", c["issue"]);
        if c["cycle"] == json!(true) {
            line.push_str(" on a cycle");
        }
        if !ends.is_empty() {
            line.push_str(&format!(" on {}", ends.join("; ")));
        }
        out.push(line);
    }
    if out.is_empty() {
        out.push("nothing holds one issue behind another".into());
    }
    out.push(format!(
        "as the poll of {} read it",
        v["at"].as_str().unwrap_or("?")
    ));
    out.join("\n")
}

pub fn main(given: &[String]) -> u8 {
    let json = match words(given) {
        Ok(j) => j,
        Err(rc) => return rc,
    };
    let home = PathBuf::from(env::var_os("TEHAS_HOME").unwrap_or_default());
    match starts::read(&home) {
        Ok(Some((at, record))) => {
            let v = object(&at, &record);
            if json {
                tehas_core::outln!("{v}");
            } else {
                tehas_core::outln!("{}", said(&v));
            }
            0
        }
        Ok(None) => {
            if json {
                tehas_core::outln!("{{\"api\":1,\"state\":\"none\"}}");
            } else {
                tehas_core::outln!("no poll has recorded its holds yet");
            }
            3
        }
        Err(_) => {
            eprintln!(
                "tehas blockers: {} cannot be read",
                starts::path(&home).display()
            );
            if json {
                tehas_core::outln!("{{\"api\":1,\"state\":\"unreadable\"}}");
            }
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Record {
        Record {
            issues: vec![
                (
                    21,
                    Some(Reason::new(
                        code::BLOCKED,
                        "tehas:blocked holds it: #11 is open",
                    )),
                ),
                (
                    22,
                    Some(Reason::new(
                        code::BLOCKED_LINK,
                        "a blocked-by link holds it: 2 blockers are open",
                    )),
                ),
                (
                    30,
                    Some(Reason::new(
                        code::BLOCKED,
                        "tehas:blocked holds it: #31 is open",
                    )),
                ),
                (
                    31,
                    Some(Reason::new(
                        code::BLOCKED,
                        "tehas:blocked holds it: #30 is open",
                    )),
                ),
            ],
            edges: vec![
                Edge::Marker {
                    issue: 21,
                    on: 11,
                    by: "alice".into(),
                },
                Edge::Link { issue: 22, open: 2 },
                Edge::Marker {
                    issue: 30,
                    on: 31,
                    by: "alice".into(),
                },
                Edge::Marker {
                    issue: 31,
                    on: 30,
                    by: "bob".into(),
                },
            ],
            holds: vec![(
                11,
                Reason::new(
                    code::QUESTION,
                    "it carries tehas:question: it waits for an answer",
                ),
            )],
        }
    }

    #[test]
    fn the_object_is_the_sample() {
        let file: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../api/blockers.sample.json"
        )))
        .unwrap();
        assert_eq!(object("2026-11-20T10:00:00Z", &sample()), file);
    }

    #[test]
    fn a_person_reads_a_line_a_chain() {
        let text = said(&object("2026-11-20T10:00:00Z", &sample()));
        assert!(
            text.contains("cycle     #30 #31: none of them starts"),
            "{text}"
        );
        assert!(
            text.contains("#21 waits on #11: it carries tehas:question"),
            "{text}"
        );
        assert!(
            text.contains("#22 waits on 2 blocked-by links of it are open"),
            "{text}"
        );
        assert!(text.contains("#30 waits on a cycle"), "{text}");
    }

    #[test]
    fn it_takes_json_alone() {
        assert_eq!(words(&[]), Ok(false));
        assert_eq!(words(&["--json".to_string()]), Ok(true));
        assert_eq!(words(&["x".to_string()]), Err(64));
    }
}
