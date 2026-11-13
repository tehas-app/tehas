//! What a cycle leaves in state/cycles/<issue>.progress.json, and whether a
//! result is an error only. The watch writes the file at a cycle's end; the
//! watch's end and the report read it through [`crate::runner::progress_fields`],
//! and the analytics import (console/analytics/src/run.rs) reads it for the
//! fields host/docs/analytics.md lists. A change to its shape reaches all three.
//!
//! roles and failure come from the issue's entry of a schema 5 result, in the
//! order the result lists its roles. With no such entry they come from the
//! progress: the stable dispatch-completed lines, each counted once by its
//! key, and for the reason the lines the runner marks unstable, which may be
//! empty. reviewHeld is a stable cap-hit that ended the review on its round cap
//! and sent the cycle on to ci, so its findings were left open. It always
//! comes from the progress lines, whatever `from` says of roles and failure.
//! `from` is `result`, `progress` when there is no result file, or
//! `bad-result` when there is one and it gives no entry for the issue.

use crate::runner::{self, kept, RESULT_SCHEMA};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use tehas_core::jq::{obj, J};

fn stable(o: &Value) -> bool {
    matches!(
        o.get("unstable"),
        None | Some(Value::Null) | Some(Value::Bool(false))
    )
}

fn kind(o: &Value, word: &str) -> bool {
    o.get("kind").and_then(Value::as_str) == Some(word)
}

/// The JSON objects of version 1 among the lines of a stream.
fn stream(text: &str) -> Vec<Value> {
    text.lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|v| v.is_object() && v.get("v").and_then(Value::as_f64) == Some(1.0))
        .collect()
}

/// A number jq would print: whole ones without a point.
fn figure(x: f64) -> J {
    if x.fract() == 0.0 && x.abs() < 1e17 {
        J::int(x as i64)
    } else {
        J::Num(format!("{x}"))
    }
}

/// `tonumber? // 0`.
fn counted(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Number(x)) => x.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn role_of(event: &serde_json::Map<String, Value>) -> String {
    let text = match event.get("role") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    };
    let role = kept(&text, |c| c.is_ascii_alphanumeric() || "_.-".contains(c));
    if role.is_empty() {
        "driver".to_string()
    } else {
        role
    }
}

#[derive(Default)]
struct Row {
    dispatches: f64,
    turns: f64,
    input: f64,
    output: f64,
    cache_read: f64,
}

fn role_row(role: &str, r: [J; 5]) -> J {
    let [dispatches, turns, input, output, cache_read] = r;
    obj(vec![
        ("role", J::str(role)),
        ("dispatches", dispatches),
        ("turns", turns),
        ("input", input),
        ("output", output),
        ("cacheRead", cache_read),
    ])
}

struct Lines {
    roles: J,
    reason: String,
    review_held: bool,
}

/// None where jq stops: a usage that is no object.
fn roles_of(lines: &[&Value]) -> Option<J> {
    let mut keys = HashSet::new();
    let mut rows: BTreeMap<String, Row> = BTreeMap::new();
    for o in lines
        .iter()
        .filter(|o| kind(o, "dispatch-completed") && stable(o))
    {
        let key = o.get("key").unwrap_or(&Value::Null).to_string();
        if !keys.insert(key) {
            continue;
        }
        let Some(event) = o.get("event").and_then(Value::as_object) else {
            continue;
        };
        let empty = serde_json::Map::new();
        let usage = match event.get("usage") {
            None | Some(Value::Null) | Some(Value::Bool(false)) => &empty,
            Some(Value::Object(u)) => u,
            _ => return None,
        };
        let row = rows.entry(role_of(event)).or_default();
        row.dispatches += 1.0;
        row.turns += counted(usage.get("turns"));
        row.input += counted(usage.get("input"));
        row.output += counted(usage.get("output"));
        row.cache_read += counted(usage.get("cacheRead"));
    }
    Some(J::Arr(
        rows.iter()
            .map(|(role, r)| {
                role_row(
                    role,
                    [
                        figure(r.dispatches),
                        figure(r.turns),
                        figure(r.input),
                        figure(r.output),
                        figure(r.cache_read),
                    ],
                )
            })
            .collect(),
    ))
}

fn string_of(v: Option<&Value>) -> Option<&str> {
    v.and_then(Value::as_str).filter(|s| !s.is_empty())
}

/// What the stream of a run says, read only for a runner that wrote no result.
fn from_lines(text: &str) -> Lines {
    let all = stream(text);
    let is_failed = |o: &&Value| {
        o.get("unstable") == Some(&Value::Bool(true))
            && (kind(o, "dispatch-failed") || kind(o, "dispatch-failed-provider"))
    };
    let mut lines: Vec<&Value> = all.iter().filter(|o| stable(o)).collect();
    lines.extend(all.iter().filter(is_failed));
    lines.extend(all.iter().filter(|o| kind(o, "cap-hit")));
    let Some(roles) = roles_of(&lines) else {
        return Lines {
            roles: J::Arr(vec![]),
            reason: String::new(),
            review_held: false,
        };
    };
    let event = |o: &&Value, k: &str| o.get("event").and_then(|e| e.get(k)).cloned();
    let evidence = lines
        .iter()
        .filter(|o| kind(o, "cap-hit"))
        .filter_map(|o| string_of(o.get("event").and_then(|e| e.get("evidence"))))
        .next_back();
    let held = lines.iter().any(|o| {
        kind(o, "cap-hit")
            && stable(o)
            && event(o, "cap").as_ref().and_then(Value::as_str) == Some("round-cap")
            && event(o, "nextStep").as_ref().and_then(Value::as_str) == Some("ci")
    });
    let failure = lines
        .iter()
        .filter(|o| o.get("unstable") == Some(&Value::Bool(true)))
        .flat_map(|o| {
            let e = o.get("event");
            [
                string_of(e.and_then(|e| e.get("providerMessage"))),
                string_of(e.and_then(|e| e.get("errorTail"))),
            ]
        })
        .flatten()
        .next_back();
    Lines {
        roles,
        reason: evidence.or(failure).unwrap_or("").to_string(),
        review_held: held,
    }
}

/// The issue's entry of a schema 5 result. A file of more than one JSON value
/// is none. Err(true) when there is a file and it gives no entry: a link, an
/// unreadable file, no JSON value, another schema, no issues, or no entry for
/// `issue`. Err(false) when there is no file.
fn entry(path: &Path, issue: u64) -> Result<J, bool> {
    let m = fs::symlink_metadata(path).map_err(|e| e.kind() != ErrorKind::NotFound)?;
    if m.file_type().is_symlink() {
        return Err(true);
    }
    let text = String::from_utf8_lossy(&fs::read(path).map_err(|_| true)?).into_owned();
    let r = J::parse(&text).ok_or(true)?;
    if !is_number(r.get("schemaVersion"), RESULT_SCHEMA as f64) {
        return Err(true);
    }
    let entries: Vec<&J> = match r.get("issues") {
        J::Arr(a) => a.iter().collect(),
        J::Obj(o) => o.iter().map(|(_, v)| v).collect(),
        _ => return Err(true),
    };
    entries
        .into_iter()
        .find(|e| matches!(**e, J::Obj(_)) && is_number(e.get("issue"), issue as f64))
        .cloned()
        .ok_or(true)
}

fn is_number(v: &J, n: f64) -> bool {
    matches!(v, J::Num(l) if l.parse::<f64>() == Ok(n))
}

fn plain(v: &J) -> J {
    J::Str(match v {
        J::Str(s) => kept(s, |c| c.is_ascii_alphanumeric() || "_.:-".contains(c)),
        _ => String::new(),
    })
}

/// A number of at least 0, as written; else 0.
fn at_least_zero(v: &J) -> J {
    match v {
        J::Num(l) if l.parse::<f64>().is_ok_and(|x| x >= 0.0) => J::Num(l.clone()),
        _ => J::int(0),
    }
}

fn result_roles(entry: &J) -> J {
    let by_role = match entry.get("usage").get("byRole") {
        J::Obj(o) => o.as_slice(),
        _ => &[],
    };
    J::Arr(
        by_role
            .iter()
            .filter(|(_, v)| matches!(v, J::Obj(_)))
            .map(|(k, v)| {
                let name = match plain(&J::str(k)) {
                    J::Str(s) if !s.is_empty() => s,
                    _ => "driver".to_string(),
                };
                let n = |k: &str| at_least_zero(v.get(k));
                role_row(
                    &name,
                    [
                        n("dispatches"),
                        n("turns"),
                        n("input"),
                        n("output"),
                        n("cacheRead"),
                    ],
                )
            })
            .collect(),
    )
}

/// One JSON object, as the file holds it. `result` is the result and the issue
/// when the run has them.
pub fn summary(ndjson: &Path, result: Option<(&Path, u64)>) -> String {
    let text = fs::read(ndjson).unwrap_or_default();
    let lines = from_lines(&String::from_utf8_lossy(&text));
    let (entry, bad) = match result.map(|(path, issue)| entry(path, issue)) {
        Some(Ok(e)) => (Some(e), false),
        Some(Err(bad)) => (None, bad),
        None => (None, false),
    };
    // The failure of the entry, only when it is an object: a plain get of a
    // missing key is null, which is not a failure.
    let failure = entry
        .as_ref()
        .map(|e| e.get("failure"))
        .filter(|f| matches!(f, J::Obj(_)));
    let from = |w: &str| obj(vec![("roles", J::str(w)), ("failure", J::str(w))]);
    let reason = |f: Option<&J>| {
        [
            f.map(|f| f.get("providerMessage")),
            f.map(|f| f.get("message")),
        ]
        .into_iter()
        .flatten()
        .find_map(|m| match m {
            J::Str(s) if !s.is_empty() => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_default()
    };
    let (roles, why, fail, word) = match &entry {
        Some(e) => (
            result_roles(e),
            reason(failure),
            match failure {
                Some(f) => obj(vec![
                    ("kind", plain(f.get("kind"))),
                    ("step", plain(f.get("step"))),
                    ("role", plain(f.get("role"))),
                    ("cause", plain(f.get("cause"))),
                ]),
                None => J::Null,
            },
            "result",
        ),
        None if bad => (lines.roles, lines.reason, J::Null, "bad-result"),
        None => (lines.roles, lines.reason, J::Null, "progress"),
    };
    obj(vec![
        ("roles", roles),
        ("reason", J::Str(why)),
        ("failure", fail),
        ("reviewHeld", J::Bool(lines.review_held)),
        ("from", from(word)),
    ])
    .dump()
}

/// Whether `text` is a schema 5 result with no issue entry at all, as a refusal
/// that carries only an error may be.
pub fn error_only(text: &str) -> bool {
    let Some(values) = runner::values(text) else {
        return false;
    };
    let Some(Value::Object(o)) = values.last() else {
        return false;
    };
    let none = match o.get("issues") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => true,
        Some(Value::Array(a)) => a.is_empty(),
        _ => false,
    };
    o.get("schemaVersion").and_then(Value::as_f64) == Some(RESULT_SCHEMA as f64) && none
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tehas_core::testdir::TestDir;

    fn file(dir: &Path, name: &str, text: &str) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, text).unwrap();
        p
    }

    #[test]
    fn the_stable_lines_are_counted_once_by_their_key() {
        let d = TestDir::new("progress");
        let log = file(
            &d,
            "a.jsonl",
            concat!(
                r#"{"v":1,"kind":"dispatch-completed","key":"k1","event":{"role":"dev","usage":{"turns":2,"input":3,"output":"4"}}}"#,
                "\n",
                r#"{"v":1,"kind":"dispatch-completed","key":"k1","event":{"role":"dev","usage":{"turns":2,"input":3}}}"#,
                "\n",
                r#"{"v":1,"kind":"dispatch-completed","key":"k2","event":{"usage":null}}"#,
                "\n",
                r#"{"v":1,"unstable":true,"kind":"dispatch-completed","key":"k3","event":{"role":"no"}}"#,
                "\n",
                "not json\n",
            ),
        );
        assert_eq!(
            summary(&log, None),
            concat!(
                r#"{"roles":[{"role":"dev","dispatches":1,"turns":2,"input":3,"output":4,"cacheRead":0},"#,
                r#"{"role":"driver","dispatches":1,"turns":0,"input":0,"output":0,"cacheRead":0}],"#,
                r#""reason":"","failure":null,"reviewHeld":false,"from":{"roles":"progress","failure":"progress"}}"#
            )
        );
    }

    #[test]
    fn a_usage_that_is_no_object_leaves_the_lines_with_nothing() {
        let d = TestDir::new("progress");
        let log = file(
            &d,
            "b.jsonl",
            concat!(
                r#"{"v":1,"kind":"dispatch-completed","key":"k","event":{"usage":7}}"#,
                "\n",
                r#"{"v":1,"unstable":true,"kind":"dispatch-failed","event":{"errorTail":"boom"}}"#,
                "\n",
            ),
        );
        assert_eq!(
            summary(&log, None),
            r#"{"roles":[],"reason":"","failure":null,"reviewHeld":false,"from":{"roles":"progress","failure":"progress"}}"#
        );
    }

    #[test]
    fn the_evidence_of_a_cap_comes_before_a_failure_and_the_last_of_each_counts() {
        let d = TestDir::new("progress");
        let log = file(
            &d,
            "c.jsonl",
            concat!(
                r#"{"v":1,"unstable":true,"kind":"dispatch-failed-provider","event":{"providerMessage":"first","errorTail":"tail"}}"#,
                "\n",
                r#"{"v":1,"kind":"cap-hit","event":{"evidence":"","cap":"round-cap","nextStep":"ci"}}"#,
                "\n",
                r#"{"v":1,"kind":"cap-hit","event":{"evidence":"why"}}"#,
                "\n",
            ),
        );
        let s = summary(&log, None);
        assert!(s.contains(r#""reason":"why""#), "{s}");
        assert!(s.contains(r#""reviewHeld":true"#), "{s}");
    }

    #[test]
    fn a_failure_is_the_reason_when_no_cap_gives_one() {
        let d = TestDir::new("progress");
        let log = file(
            &d,
            "d.jsonl",
            concat!(
                r#"{"v":1,"unstable":true,"kind":"dispatch-failed","event":{"errorTail":"boom"}}"#,
                "\n",
                r#"{"v":1,"unstable":true,"kind":"log","event":{"errorTail":"not read"}}"#,
                "\n",
            ),
        );
        assert!(summary(&log, None).contains(r#""reason":"boom""#));
    }

    #[test]
    fn a_result_gives_its_roles_in_its_own_order_and_its_failure() {
        let d = TestDir::new("progress");
        let result = file(
            &d,
            "r.json",
            concat!(
                r#"{"schemaVersion":5,"issues":[{"issue":8},{"issue":7,"usage":{"byRole":"#,
                r#"{"zed":{"dispatches":1,"turns":2,"input":3,"output":4,"cacheRead":5},"a b":{"dispatches":"x"},"bad":7}},"#,
                r#""failure":{"kind":"provider","step":"dev elop","role":"r","cause":"c:1","message":"m","providerMessage":""}}]}"#
            ),
        );
        let log = file(&d, "e.jsonl", "");
        let s = summary(&log, Some((&result, 7)));
        assert_eq!(
            s,
            concat!(
                r#"{"roles":[{"role":"zed","dispatches":1,"turns":2,"input":3,"output":4,"cacheRead":5},"#,
                r#"{"role":"ab","dispatches":0,"turns":0,"input":0,"output":0,"cacheRead":0}],"#,
                r#""reason":"m","failure":{"kind":"provider","step":"develop","role":"r","cause":"c:1"},"#,
                r#""reviewHeld":false,"from":{"roles":"result","failure":"result"}}"#
            )
        );
    }

    #[test]
    fn a_result_that_gives_no_entry_is_a_bad_result_and_no_file_is_progress() {
        let d = TestDir::new("progress");
        let log = file(&d, "f.jsonl", "");
        let other = file(
            &d,
            "o.json",
            r#"{"schemaVersion":3,"issues":[{"issue":7}]}"#,
        );
        let link = d.join("link.json");
        std::os::unix::fs::symlink(
            file(
                &d,
                "g.json",
                r#"{"schemaVersion":5,"issues":[{"issue":7}]}"#,
            ),
            &link,
        )
        .unwrap();
        let two = file(&d, "t.json", "{} {}");
        let empty = file(&d, "e.json", "");
        let no_issues = file(&d, "n.json", r#"{"schemaVersion":5,"issues":7}"#);
        let good = file(
            &d,
            "h.json",
            r#"{"schemaVersion":5,"issues":[{"issue":7}]}"#,
        );
        let from = |w: &str| format!(r#""from":{{"roles":"{w}","failure":"{w}"}}"#);
        for (path, issue) in [
            (other.as_path(), 7),
            (link.as_path(), 7),
            (two.as_path(), 7),
            (empty.as_path(), 7),
            (no_issues.as_path(), 7),
            (Path::new(&*d), 7),
            (good.as_path(), 8),
        ] {
            let s = summary(&log, Some((path, issue)));
            assert!(s.contains(&from("bad-result")), "{path:?} {s}");
            assert!(s.contains(r#""failure":null"#), "{s}");
        }
        assert!(summary(&log, Some((Path::new("/nothing/here"), 7))).contains(&from("progress")));
        assert!(summary(&log, None).contains(&from("progress")));
        assert!(summary(&log, Some((&good, 7))).contains(&from("result")));
    }

    #[test]
    fn an_error_only_result_is_a_schema_5_result_with_no_entries() {
        assert!(error_only(r#"{"schemaVersion":5,"issues":[],"error":{}}"#));
        assert!(error_only(r#"{"schemaVersion":5}"#));
        assert!(!error_only(r#"{"schemaVersion":5,"issues":[{"issue":7}]}"#));
        assert!(!error_only(r#"{"schemaVersion":3,"issues":[]}"#));
        assert!(!error_only(r#"{"schemaVersion":5,"issues":{}}"#));
        assert!(!error_only(""));
        assert!(!error_only("not json"));
        assert!(!error_only("[]"));
    }
}
