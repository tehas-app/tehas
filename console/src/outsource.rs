//! `tehas outsource`: the contractors of this factory, as a person and a
//! program that manages factories both see them (#2274). The text a person
//! reads is src/usage/outsource.txt.
//!
//! Every subcommand asks the feature registry first: with outsource off it
//! says so and exits 78 before it reads or writes anything. The rows are the
//! state store's outsource verbs.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::env;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use tehas_core::{features, outln, time};

pub const USAGE: &str = include_str!("usage/outsource.txt");

#[derive(Debug, PartialEq, Eq)]
pub enum Asked {
    Invite { name: Option<String> },
    List,
    Status,
    Revoke(String),
    Drain(String),
}

/// The words of a run: the subcommand and whether it answers in JSON.
/// `Ok(None)` is help.
pub fn words(given: &[String]) -> Result<Option<(Asked, bool)>, u8> {
    let bad = |why: &str| {
        eprintln!("tehas outsource: {why}");
        eprint!("{USAGE}");
        Err(64)
    };
    if given.iter().any(|w| w == "-h" || w == "--help") {
        return Ok(None);
    }
    let json = given.iter().any(|w| w == "--json");
    let rest: Vec<&str> = given
        .iter()
        .map(String::as_str)
        .filter(|w| *w != "--json")
        .collect();
    let asked = match rest[..] {
        ["invite"] => Asked::Invite { name: None },
        ["invite", "--name", name] if contractor_name(name) => Asked::Invite {
            name: Some(name.to_string()),
        },
        ["invite", "--name", _] => {
            return bad("a name is a host's name: letters, digits, '.', '-' and '_'")
        }
        ["list"] => Asked::List,
        ["status"] => Asked::Status,
        ["revoke", name] if contractor_name(name) => Asked::Revoke(name.to_string()),
        ["drain", name] if contractor_name(name) => Asked::Drain(name.to_string()),
        [] => return bad("which subcommand?"),
        _ => return bad(&format!("not a subcommand: {}", rest.join(" "))),
    };
    Ok(Some((asked, json)))
}

/// A contractor's name, the shape the store takes.
fn contractor_name(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= 64
        && b[0].is_ascii_alphanumeric()
        && b.iter()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(c))
}

fn sha256(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn code() -> Option<String> {
    let mut bytes = [0u8; 24];
    File::open("/dev/urandom")
        .ok()?
        .read_exact(&mut bytes)
        .ok()?;
    Some(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn setting(name: &str) -> String {
    env::var(name).unwrap_or_default()
}

/// The status word of one row of `contractor-list`, as `tehas outsource list`
/// and the spec's "Visibility" name them.
pub fn status_of(row: &Value, now: i64, timeout: i64) -> &'static str {
    if row["revoked"] == true {
        return "revoked";
    }
    match row["job_state"].as_str() {
        Some("stopping") => return "stopping",
        Some(_) => return "working",
        None => {}
    }
    if row["draining"] == true {
        return "draining";
    }
    if row["resting"] == true {
        return "resting";
    }
    match row["last_seen_at"].as_i64() {
        Some(seen) if now - seen <= timeout => "idle",
        _ => "silent",
    }
}

/// The rows of `contractor-list`, each an object.
fn rows(said: &Value) -> Vec<Value> {
    said["rows"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|r| match r {
            Value::String(t) => serde_json::from_str(&t).ok(),
            v @ Value::Object(_) => Some(v),
            _ => None,
        })
        .collect()
}

/// The contractors as `list --json` gives them.
pub fn listed(said: &Value, now: i64, timeout: i64) -> Vec<Value> {
    rows(said)
        .iter()
        .map(|r| {
            json!({
                "name": r["name"],
                "status": status_of(r, now, timeout),
                "issue": r["job_issue"],
                "lastSeenAt": r["last_seen_at"],
                "jobsDone": r["jobs_done"],
                "jobsLost": r["jobs_lost"],
            })
        })
        .collect()
}

/// The counts of `status --json`.
pub fn counted(list: &[Value]) -> Value {
    let mut out = serde_json::Map::new();
    for word in [
        "idle", "working", "stopping", "silent", "resting", "draining", "revoked",
    ] {
        let n = list.iter().filter(|c| c["status"] == word).count();
        out.insert(word.to_string(), json!(n));
    }
    let outsourced = list.iter().filter(|c| !c["issue"].is_null()).count();
    out.insert("outsourced".to_string(), json!(outsourced));
    Value::Object(out)
}

pub fn main(given: &[String]) -> u8 {
    let asked = match words(given) {
        Ok(None) => {
            tehas_core::out!("{USAGE}");
            return 0;
        }
        Ok(Some(a)) => a,
        Err(rc) => return rc,
    };
    if !features::on("outsource") {
        eprintln!(
            "tehas outsource: outsource is off: TEHAS_OUTSOURCE is not on, or this release is still building it"
        );
        return 78;
    }
    let home = PathBuf::from(setting("TEHAS_HOME"));
    let (asked, json) = asked;
    let timeout: i64 = setting("TEHAS_OUTSOURCE_TIMEOUT").parse().unwrap_or(600);
    // The outsource verbs answer with the word of what happened, which the
    // store crate's own call gives as it is.
    let call = |args: &[&str]| {
        let answer = ::store::call(&home, args, &mut std::io::empty());
        match answer.map(|t| serde_json::from_str::<Value>(&t).unwrap_or(Value::Null)) {
            Ok(v) if v["verb"] == "refused" => {
                eprintln!(
                    "tehas outsource: the store refused: {}",
                    v["said"].as_str().unwrap_or("refused")
                );
                Err(1)
            }
            Ok(Value::Null) => {
                eprintln!("tehas outsource: the store gave no answer");
                Err(1)
            }
            Ok(v) => Ok(v),
            Err((rc, line)) => {
                eprintln!("tehas outsource: {}", line.trim_end());
                Err(if rc == 78 { 78 } else { 1 })
            }
        }
    };
    let said = match &asked {
        Asked::Invite { name } => {
            let Some(code) = code() else {
                eprintln!("tehas outsource: no randomness to make a code of");
                return 1;
            };
            let name = name
                .clone()
                .unwrap_or_else(|| format!("contractor-{}", &sha256(&code)[..8]));
            let ttl = setting("TEHAS_OUTSOURCE_JOIN_TTL");
            let mut args = vec!["invite-new", "--name", &name, "--code"];
            let hash = sha256(&code);
            args.push(&hash);
            if !ttl.is_empty() {
                args.extend(["--ttl", &ttl]);
            }
            let v = match call(&args) {
                Ok(v) => v,
                Err(rc) => return rc,
            };
            let command = format!(
                "tehas-contractor join {} {name} {code}",
                setting("TEHAS_OUTSOURCE_PROXY_ADDR")
            );
            if !json {
                outln!("{command}");
                return 0;
            }
            json!({ "name": name, "command": command, "expiresAt": v["expires_at"] })
        }
        Asked::List | Asked::Status => {
            let v = match call(&["contractor-list"]) {
                Ok(v) => v,
                Err(rc) => return rc,
            };
            let list = listed(&v, time::now(), timeout);
            if asked == Asked::List {
                if !json {
                    for c in &list {
                        outln!("{}", line_of(c));
                    }
                    return 0;
                }
                json!({ "contractors": list })
            } else {
                let counts = counted(&list);
                if !json {
                    outln!("{}", status_line(&counts));
                    return 0;
                }
                json!({ "contractors": counts })
            }
        }
        Asked::Revoke(name) | Asked::Drain(name) => {
            let verb = if matches!(asked, Asked::Revoke(_)) {
                "contractor-revoke"
            } else {
                "contractor-drain"
            };
            let v = match call(&[verb, "--name", name]) {
                Ok(v) => v,
                Err(rc) => return rc,
            };
            if !json {
                match &asked {
                    Asked::Revoke(_) => outln!(
                        "{name}: revoked, {} job(s) lost",
                        v["lost"].as_i64().unwrap_or_default()
                    ),
                    _ => outln!("{name}: draining"),
                }
                return 0;
            }
            json!({ "name": name, "said": v["verb"] })
        }
    };
    let mut out = json!({ "api": 1 });
    if let (Some(o), Some(s)) = (out.as_object_mut(), said.as_object()) {
        o.extend(s.clone());
    }
    outln!("{out}");
    0
}

fn line_of(c: &Value) -> String {
    let issue = c["issue"]
        .as_i64()
        .map_or_else(String::new, |n| format!(" on #{n}"));
    let seen = c["lastSeenAt"].as_i64().map_or_else(
        || "never heard of".to_string(),
        |t| format!("heard of {}s ago", (time::now() - t).max(0)),
    );
    format!(
        "{} {}{issue}, {seen}, {} done, {} lost",
        c["name"].as_str().unwrap_or_default(),
        c["status"].as_str().unwrap_or_default(),
        c["jobsDone"].as_i64().unwrap_or_default(),
        c["jobsLost"].as_i64().unwrap_or_default()
    )
}

/// The `outsource:` line: how many contractors are idle, working and resting,
/// and how many cycles are outsourced.
pub fn status_line(counts: &Value) -> String {
    let n = |k: &str| counts[k].as_i64().unwrap_or_default();
    format!(
        "outsource: {} idle, {} working, {} resting; {} cycle(s) outsourced",
        n("idle"),
        n("working") + n("stopping"),
        n("resting"),
        n("outsourced")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn the_words_are_a_subcommand_and_json() {
        assert_eq!(words(&w(&["list"])), Ok(Some((Asked::List, false))));
        assert_eq!(
            words(&w(&["status", "--json"])),
            Ok(Some((Asked::Status, true)))
        );
        assert_eq!(
            words(&w(&["invite", "--name", "box1"])),
            Ok(Some((
                Asked::Invite {
                    name: Some("box1".into())
                },
                false
            )))
        );
        assert_eq!(
            words(&w(&["revoke", "box1", "--json"])),
            Ok(Some((Asked::Revoke("box1".into()), true)))
        );
        assert_eq!(words(&w(&["drain", "--help"])), Ok(None));
        for bad in [
            &[][..],
            &["revoke"],
            &["revoke", "a b"],
            &["list", "x"],
            &["invite", "--name", "-x"],
        ] {
            assert_eq!(words(&w(bad)), Err(64), "{bad:?}");
        }
    }

    #[test]
    fn a_contractor_s_status_is_read_from_its_row() {
        let now = 10_000;
        let row = |v: Value| status_of(&v, now, 600);
        assert_eq!(
            row(json!({ "revoked": true, "job_state": "running" })),
            "revoked"
        );
        assert_eq!(
            row(json!({ "revoked": false, "job_state": "running" })),
            "working"
        );
        assert_eq!(
            row(json!({ "revoked": false, "job_state": "offered" })),
            "working"
        );
        assert_eq!(
            row(json!({ "revoked": false, "job_state": "stopping" })),
            "stopping"
        );
        assert_eq!(
            row(json!({ "revoked": false, "draining": true })),
            "draining"
        );
        assert_eq!(row(json!({ "revoked": false, "resting": true })), "resting");
        assert_eq!(
            row(json!({ "revoked": false, "last_seen_at": now - 10 })),
            "idle"
        );
        assert_eq!(
            row(json!({ "revoked": false, "last_seen_at": now - 601 })),
            "silent"
        );
        assert_eq!(row(json!({ "revoked": false })), "silent");
    }

    #[test]
    fn the_counts_and_the_status_line_say_what_is_outsourced() {
        let said = json!({ "rows": [
            json!({ "name": "a", "revoked": false, "job_issue": 7, "job_state": "running" }).to_string(),
            json!({ "name": "b", "revoked": false, "last_seen_at": 9_990 }).to_string(),
            json!({ "name": "c", "revoked": false, "resting": true }).to_string(),
        ]});
        let list = listed(&said, 10_000, 600);
        assert_eq!(list[0]["issue"], 7);
        let counts = counted(&list);
        assert_eq!(counts["working"], 1);
        assert_eq!(counts["idle"], 1);
        assert_eq!(counts["resting"], 1);
        assert_eq!(counts["outsourced"], 1);
        assert_eq!(
            status_line(&counts),
            "outsource: 1 idle, 1 working, 1 resting; 1 cycle(s) outsourced"
        );
    }
}
