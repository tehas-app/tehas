//! `factory_contractors`: the contractors of outsource, as `tehasbot status`
//! last wrote them to `live/contractors.json`. Read only, and served to a
//! dashboard key too: the file holds what the dashboard shows and no address
//! or key. The tool exists only on a server started with `--outsource`, which
//! `tehasbot mcp serve` passes while the feature is on (tools::serve_outsource).

use crate::tools::{self, ToolError};
use serde_json::{json, Map, Value};
use std::path::Path;

/// The tool's name.
pub const NAME: &str = "factory_contractors";

/// The published file, under the live directory.
pub const FILE: &str = "contractors.json";

/// The statuses a contractor can have, as the file writes them.
const STATUSES: [&str; 7] = [
    "idle", "working", "stopping", "silent", "resting", "draining", "revoked",
];

pub fn tool() -> Value {
    let int = |d: &str| json!({"type": "integer", "minimum": 0, "description": d});
    let mut counts = Map::new();
    for s in STATUSES {
        counts.insert(s.into(), int(&format!("Contractors that are {s}.")));
    }
    counts.insert(
        "outsourced".into(),
        int("Cycles on a contractor now: the contractors that hold a job."),
    );
    json!({
        "name": NAME,
        "description": "The contractors of outsource: the counts by status, how many cycles are outsourced now, and each contractor with its status, the issue of the job in its hand, when it last called, its jobs done and lost, and its last report (image id, pi-rukas version, free memory and disk). As the factory last wrote them at a poll; stale is true when that is more than ten minutes ago. Only while outsource is on.",
        "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false},
        "outputSchema": {"type": "object", "properties": {
            "api": {"type": "integer", "description": "The API version, 1."},
            "time": {"type": "string", "description": "When the factory wrote the list, UTC."},
            "stale": {"type": "boolean", "description": "The list is more than ten minutes old, or its time cannot be read."},
            "counts": {"type": "object", "properties": counts, "description": "How many contractors have each status, and how many cycles are outsourced."},
            "contractors": {"type": "array", "description": "Each contractor.", "items": {
                "type": "object", "properties": {
                    "name": {"type": "string", "description": "The contractor's name, the one it joined with."},
                    "status": {"type": "string", "enum": STATUSES, "description": "idle, working, stopping, silent (not heard of for the timeout), resting (after a lost job), draining or revoked."},
                    "issue": {"type": "integer", "minimum": 1, "description": "The issue of the job in its hand; left out when it holds none."},
                    "lastSeenAt": {"type": "string", "description": "When it last called, UTC; left out when it never has."},
                    "jobsDone": int("Jobs it finished."),
                    "jobsLost": int("Jobs it lost."),
                    "image": {"type": "string", "description": "The id of the sandbox image it last reported; left out when it did not say."},
                    "rukas": {"type": "string", "description": "The pi-rukas version that image is labelled with; left out when it did not say."},
                    "memMb": int("The free memory it last reported, in MiB; left out when it did not say."),
                    "diskKb": int("The free disk it last reported, in KiB; left out when it did not say."),
                    "reportedAt": {"type": "string", "description": "When the factory took its last report, UTC; left out when it never sent one."},
                },
            }},
        }, "required": ["api"]},
    })
}

/// What an answer copies from the file.
pub fn spec() -> Value {
    let mut counts = Map::new();
    for s in STATUSES.iter().chain(["outsourced"].iter()) {
        counts.insert((*s).into(), json!(true));
    }
    json!({
        "time": true,
        "counts": counts,
        "contractors": [{
            "name": true, "status": true, "issue": true, "lastSeenAt": true,
            "jobsDone": true, "jobsLost": true, "image": true, "rukas": true,
            "memMb": true, "diskKb": true, "reportedAt": true,
        }],
    })
}

pub fn call(live: &Path) -> Result<Map<String, Value>, ToolError> {
    call_at(live, tools::now())
}

fn call_at(live: &Path, now: i64) -> Result<Map<String, Value>, ToolError> {
    let file = tools::read(live, FILE)?;
    if file["api"] != 1 || !file["contractors"].is_array() {
        return Err(ToolError::File {
            file: FILE.to_string(),
            why: "is not a list of contractors".to_string(),
        });
    }
    let mut out = Map::new();
    tools::merge(&mut out, tools::pick(&file, &spec()));
    let stale = file["time"]
        .as_str()
        .and_then(tools::epoch)
        .is_none_or(|t| now - t > tools::STALE_SECS);
    out.insert("stale".into(), json!(stale));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tehas_core::testdir::TestDir;

    #[test]
    fn the_answer_copies_only_the_listed_fields_and_says_when_it_is_old() {
        let dir = TestDir::new("mcp-contractors");
        std::fs::write(
            dir.join(FILE),
            r#"{"api":1,"time":"2026-10-20T08:46:07Z",
                "counts":{"idle":1,"working":1,"stopping":0,"silent":0,"resting":0,"draining":0,"revoked":0,"outsourced":1,"later":"x"},
                "contractors":[{"name":"box1","status":"working","issue":12,"lastSeenAt":"2026-10-20T08:46:00Z","jobsDone":3,"jobsLost":1,"key":"SECRET","address":"192.0.2.7"},
                               {"name":"box2","status":"idle","jobsDone":0,"jobsLost":0}]}"#,
        )
        .unwrap();
        let t = 1_792_485_967;
        let out = call_at(&dir, t + 60).unwrap();
        assert_eq!(out["stale"], false);
        assert_eq!(out["counts"]["outsourced"], 1);
        assert!(out["counts"].get("later").is_none());
        assert_eq!(out["contractors"][0]["issue"], 12);
        assert!(out["contractors"][1].get("issue").is_none());
        let text = Value::Object(out).to_string();
        for bad in ["SECRET", "192.0.2.7", "address", "key"] {
            assert!(!text.contains(bad), "{bad}: {text}");
        }
        assert_eq!(call_at(&dir, t + 601).unwrap()["stale"], true);
    }

    #[test]
    fn no_file_or_another_shape_is_a_tool_error() {
        let dir = TestDir::new("mcp-contractors-none");
        assert!(matches!(
            call_at(&dir, 0),
            Err(ToolError::File { why, .. }) if why == "not published"
        ));
        std::fs::write(dir.join(FILE), r#"{"api":2,"contractors":[]}"#).unwrap();
        assert!(matches!(
            call_at(&dir, 0),
            Err(ToolError::File { why, .. }) if why == "is not a list of contractors"
        ));
    }

    #[test]
    fn the_schema_and_the_spec_name_the_same_fields() {
        let schema = tool()["outputSchema"]["properties"].clone();
        let spec = spec();
        for k in ["time", "counts", "contractors"] {
            assert!(schema.get(k).is_some(), "{k}");
            assert!(spec.get(k).is_some(), "{k}");
        }
        let mut a: Vec<String> = schema["counts"]["properties"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        let mut b: Vec<String> = spec["counts"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        a.sort();
        b.sort();
        assert_eq!(a, b);
        let mut a: Vec<String> = schema["contractors"]["items"]["properties"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        let mut b: Vec<String> = spec["contractors"][0]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        a.sort();
        b.sort();
        assert_eq!(a, b);
    }
}
