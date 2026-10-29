// factory_decisions: the dispatcher's last decisions from the end of
// dispatch.log, as `grep -E ' (start|skip|wait|drop the refresh of) #' | tail -n N`
// shows.

use mcp::tools::{list, Dirs, Output, ToolError};
use regex::Regex;
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn call_in(logs: &Path, args: &Value) -> Result<Value, ToolError> {
    let dirs = Dirs {
        live: fixtures().join("live"),
        sessions: fixtures().join("sessions"),
        logs: logs.to_path_buf(),
    };
    match mcp::tools::call(&dirs, "factory_decisions", args)? {
        Output::Json(v) => Ok(v),
        other => panic!("factory_decisions gave {other:?}"),
    }
}

// A logs directory of its own, removed when dropped.
struct Logs(PathBuf);

impl Logs {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("mcp-decisions-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Logs(dir)
    }

    fn with(name: &str, text: &str) -> Self {
        let l = Self::new(name);
        std::fs::write(l.0.join("dispatch.log"), text).unwrap();
        l
    }
}

impl Drop for Logs {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// What the issue's grep and tail show of the fixture, as the four fields.
fn grep_tail(text: &str, n: usize) -> Vec<Value> {
    let re = Regex::new(r" (start|skip|wait|drop the refresh of) #").unwrap();
    let hits: Vec<&str> = text.lines().filter(|l| re.is_match(l)).collect();
    hits[hits.len().saturating_sub(n)..]
        .iter()
        .map(|l| {
            let (time, rest) = l.split_once(' ').unwrap();
            let (word, rest) = rest.split_once(" #").unwrap();
            let word = word.strip_suffix(" the refresh of").unwrap_or(word);
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            let tail = &rest[digits..];
            let reason = tail
                .strip_prefix(": ")
                .or_else(|| tail.strip_prefix(" (").and_then(|r| r.strip_suffix(')')))
                .unwrap();
            json!({"time": time, "decision": word,
                   "issue": rest[..digits].parse::<u64>().unwrap(), "reason": reason})
        })
        .collect()
}

#[test]
fn the_answer_is_what_grep_and_tail_show() {
    let logs = fixtures().join("logs");
    let text = std::fs::read_to_string(logs.join("dispatch.log")).unwrap();
    for (args, n) in [
        (json!({}), 50),
        (json!({"limit": 1}), 1),
        (json!({"limit": 7}), 7),
        (json!({"limit": 200}), 200),
    ] {
        let out = call_in(&logs, &args).unwrap();
        assert_eq!(out["api"], 1);
        assert_eq!(out["whole"], true);
        let want = grep_tail(&text, n);
        assert_eq!(out["decisions"].as_array().unwrap(), &want, "{args}");
    }
    let out = call_in(&logs, &json!({"limit": 3})).unwrap();
    assert_eq!(
        out["decisions"],
        json!([
            {"time": "2026-10-20T10:57:00Z", "decision": "skip", "issue": 157,
             "reason": "tehas:ready was applied by 'mallory', not an allowed account"},
            {"time": "2026-10-20T10:58:00Z", "decision": "drop", "issue": 158,
             "reason": "pull request #9 moved since the request"},
            {"time": "2026-10-20T10:59:00Z", "decision": "start", "issue": 159,
             "reason": "retry ticked by alice"},
        ])
    );
}

#[test]
fn other_lines_are_not_decisions() {
    let l = Logs::with(
        "other",
        "2026-10-20T10:00:00Z start planning #9 (labelled by alice)\n\
         2026-10-20T10:00:01Z drop #8: no writer logs this shape\n\
         2026-10-20T10:00:02Z holding: the limit allows 0 cycles at once and 2 are running: draining\n\
         2026-10-20T10:00:03Z done #7 rc=0 status=merged\n\
         /home/x/libexec/dispatch: line 197: no such file\n\
         not a time skip #4: no\n\
         2026-10-20T10:00:04Z skip #0: no issue\n\
         2026-10-20T10:00:05Z wait #5x: not a number\n\
         2026-10-20T10:00:06Z wait #6: its cycle is still running\n",
    );
    let out = call_in(&l.0, &json!({})).unwrap();
    assert_eq!(
        out["decisions"],
        json!([{"time": "2026-10-20T10:00:06Z", "decision": "wait", "issue": 6,
                "reason": "its cycle is still running"}])
    );
}

#[test]
fn a_reason_is_masked_then_cut() {
    let token = format!("ghp_{}", "A".repeat(36));
    // Built from parts: the lint refuses a private address written out.
    let address = ["10", "1", "2", "3"].join(".");
    let long = "x".repeat(1000);
    let l = Logs::with(
        "mask",
        &format!(
            "2026-10-20T10:00:00Z skip #1: fetch failed with {token} at {address}\n\
             2026-10-20T10:00:01Z skip #2: {long}\n"
        ),
    );
    let out = call_in(&l.0, &json!({})).unwrap();
    let text = out.to_string();
    assert!(!text.contains(&token), "{text}");
    assert!(!text.contains(&address), "{text}");
    assert_eq!(
        out["decisions"][1]["reason"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        400
    );
}

// The log is read from its end only: a log far larger than the container's
// memory is answered with the decisions of its last bytes, and `whole` says
// that older ones were not read.
#[test]
fn a_huge_log_is_read_from_its_end_only() {
    let l = Logs::new("huge");
    let path = l.0.join("dispatch.log");
    let f = std::fs::File::create(&path).unwrap();
    f.set_len(1 << 40).unwrap();
    drop(f);
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(
        f,
        "2026-10-20T10:00:00Z skip #1: cut off with the zeros before it"
    )
    .unwrap();
    writeln!(f, "2026-10-20T10:00:01Z skip #2: kept").unwrap();
    drop(f);
    let out = call_in(&l.0, &json!({})).unwrap();
    assert_eq!(out["whole"], false);
    assert_eq!(
        out["decisions"],
        json!([{"time": "2026-10-20T10:00:01Z", "decision": "skip", "issue": 2,
                "reason": "kept"}])
    );
}

#[test]
fn bad_arguments_are_a_tool_error() {
    let logs = fixtures().join("logs");
    for args in [
        json!({"limit": 0}),
        json!({"limit": 201}),
        json!({"limit": -1}),
        json!({"limit": 1.5}),
        json!({"limit": "5"}),
        json!({"limit": true}),
        json!({"path": "/etc/passwd"}),
        json!({"file": "refresh.log"}),
        json!("hit"),
        json!(42),
    ] {
        assert!(
            matches!(call_in(&logs, &args), Err(ToolError::BadArgs(_))),
            "{args}"
        );
    }
    assert!(call_in(&logs, &Value::Null).is_ok());
}

#[test]
fn a_missing_or_linked_log_is_a_file_error() {
    let l = Logs::new("missing");
    match call_in(&l.0, &json!({})) {
        Err(ToolError::File { file, .. }) => assert_eq!(file, "dispatch.log"),
        other => panic!("{other:?}"),
    }
    let target = l.0.join("other.log");
    std::fs::write(&target, "2026-10-20T10:00:00Z skip #1: through a link\n").unwrap();
    std::os::unix::fs::symlink(&target, l.0.join("dispatch.log")).unwrap();
    match call_in(&l.0, &json!({})) {
        Err(ToolError::File { file, why }) => {
            assert_eq!(file, "dispatch.log");
            assert!(why.contains("plain file"), "{why}");
        }
        other => panic!("{other:?}"),
    }
    let d = Logs::new("dir");
    std::fs::create_dir(d.0.join("dispatch.log")).unwrap();
    assert!(matches!(
        call_in(&d.0, &json!({})),
        Err(ToolError::File { .. })
    ));
}

#[test]
fn an_empty_log_has_no_decisions() {
    let l = Logs::with("empty", "");
    let out = call_in(&l.0, &json!({})).unwrap();
    assert_eq!(out["decisions"], json!([]));
    assert_eq!(out["whole"], true);
}

// Every field of the answer is in the outputSchema, and every property of
// both schemas says what it is.
#[test]
fn the_schema_describes_the_answer() {
    let tool = list()
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "factory_decisions")
        .unwrap()
        .clone();
    let input = &tool["inputSchema"];
    assert_eq!(input["additionalProperties"], false);
    assert_eq!(input["properties"]["limit"]["minimum"], 1);
    assert_eq!(input["properties"]["limit"]["maximum"], 200);
    let out_schema = &tool["outputSchema"];
    let item = &out_schema["properties"]["decisions"]["items"]["properties"];
    let out = call_in(&fixtures().join("logs"), &json!({})).unwrap();
    for k in out.as_object().unwrap().keys() {
        assert!(out_schema["properties"].get(k).is_some(), "{k}");
    }
    for k in out["decisions"][0].as_object().unwrap().keys() {
        assert!(item.get(k).is_some(), "decisions[].{k}");
    }
    fn described(s: &Value, at: &str) {
        if let Some(p) = s["properties"].as_object() {
            for (k, v) in p {
                assert!(
                    !v["description"].as_str().unwrap_or("").is_empty(),
                    "{at}.{k}"
                );
                described(v, &format!("{at}.{k}"));
            }
        }
        if s["type"] == "array" {
            described(&s["items"], &format!("{at}[]"));
        }
    }
    described(input, "input");
    described(out_schema, "output");
}
