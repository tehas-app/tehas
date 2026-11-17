//! What a snapshot reads for the outsource feature, and only while it is on:
//! the sandbox image's id, the store's contractors, the contractors of the
//! running cycles' open jobs, and the jobs each candidate lost. The shape is
//! the one `crate::place` reads. A read that fails is said as failed, which
//! keeps every start in-house.

use super::run::{run, Calls};
use serde_json::{json, Map, Value};
use std::time::Duration;

/// The open states of a job, as the store names them.
const OPEN: [&str; 3] = ["offered", "running", "stopping"];

fn failed() -> Value {
    json!({"failed": true})
}

/// The store's answer, which names `verb`. A store with the feature off
/// answers `refused`, which is no answer here.
fn answer(calls: &mut Calls, args: &[&str], verb: &str) -> Option<Value> {
    calls.store_answering(args, verb).ok()
}

pub fn read(calls: &mut Calls, image: &str, candidates: &[u64], running: &[u64]) -> Value {
    let contractors = answer(calls, &["contractor-list"], "contractors")
        .and_then(|v| v.get("rows").filter(|r| r.is_array()).cloned())
        .unwrap_or_else(failed);
    let busy = busy(calls, running).map_or_else(failed, Value::Array);
    let mut lost = Map::new();
    for n in candidates {
        let s = n.to_string();
        if let Some(v) = answer(calls, &["job-attempts", "--issue", &s], "attempts") {
            if let Some(l) = v["lost"].as_u64() {
                lost.insert(s, json!(l));
            }
        }
    }
    json!({"digest": image_id(image), "contractors": contractors, "busy": busy,
        "lost": lost})
}

/// The contractors of the running cycles' open jobs. An issue with no job on
/// record runs on this host. None when one could not be read.
fn busy(calls: &mut Calls, running: &[u64]) -> Option<Vec<Value>> {
    let mut out = Vec::new();
    for n in running {
        match calls.store_answering(&["job-show", "--issue", &n.to_string()], "job") {
            Ok(v) if v["contractor"].is_string() => {
                if OPEN.iter().any(|s| v["state"] == *s) {
                    out.push(v["contractor"].clone());
                }
            }
            Err(e) if e["word"] == "nojob" => {}
            _ => return None,
        }
    }
    Some(out)
}

/// The id docker gives the sandbox image here, empty when it cannot say.
fn image_id(image: &str) -> String {
    if image.is_empty() {
        return String::new();
    }
    let r = run(
        "docker",
        &[
            "image".into(),
            "inspect".into(),
            "--format".into(),
            "{{.Id}}".into(),
            image.into(),
        ],
        Duration::from_secs(30),
    );
    let id = r.out.lines().next().unwrap_or("").trim();
    if r.ok() && id.starts_with("sha256:") {
        id.to_string()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tehas_core::testdir::TestDir;

    /// A store that answers the outsource reads of a world where #5 runs here,
    /// #6 runs on box-1 and #7 lost one job. `failing` names a verb it fails.
    /// The directory goes when the TestDir does.
    fn store(name: &str, failing: &str) -> (TestDir, String) {
        let dir = TestDir::new(&format!("dispatch-outsource-{name}"));
        let path = dir.path().join("store");
        let script = format!(
            r#"#!/bin/sh
[ "$1" = "{failing}" ] && {{ echo "store: busy" >&2; exit 1; }}
case "$*" in
  contractor-list) echo '{{"api":1,"verb":"contractors","ok":true,"rows":["{{\"name\":\"box-1\"}}"]}}' ;;
  "job-show --issue 5") echo "store: nojob of issue 5" >&2; exit 1 ;;
  "job-show --issue 6") echo '{{"api":1,"verb":"job","ok":true,"contractor":"box-1","state":"running"}}' ;;
  "job-attempts --issue 7") echo '{{"api":1,"verb":"attempts","ok":true,"lost":1}}' ;;
  "job-attempts --issue 8") echo '{{"api":1,"verb":"refused","ok":true}}' ;;
  *) exit 64 ;;
esac
"#
        );
        tehas_core::testing::write_exe(&path, script).unwrap();
        let path = path.to_string_lossy().into_owned();
        (dir, path)
    }

    fn of(name: &str, failing: &str) -> Value {
        let (_dir, store) = store(name, failing);
        let mut calls = Calls::idle(Duration::from_secs(5), store);
        read(&mut calls, "", &[7, 8], &[5, 6])
    }

    #[test]
    fn it_reads_what_placement_needs() {
        assert_eq!(
            of("all", ""),
            json!({"digest": "", "contractors": ["{\"name\":\"box-1\"}"],
                "busy": ["box-1"], "lost": {"7": 1}})
        );
    }

    #[test]
    fn a_read_that_fails_is_said_as_failed() {
        assert_eq!(of("list", "contractor-list")["contractors"], failed());
        assert_eq!(of("show", "job-show")["busy"], failed());
        assert_eq!(of("attempts", "job-attempts")["lost"], json!({}));
    }
}
