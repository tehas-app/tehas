// `factory_outcomes`: the long view of how the factory has done, read from the
// counts file the host writes for it. The file is written here as the analytics
// program writes it: a header, then one entry per line, counts only.

use mcp::outcomes_schema;
use mcp::tools::{list, Dirs, Output, ToolError};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use tehas_core::time::{iso, now};

const DAY: i64 = 86_400;

/// The days the writer of the file names, which is the retention it applies.
const DAYS: i64 = 365;

/// The counts file of a factory that ran a few releases over a few weeks.
fn write(dir: &Path, by: &str, generated: i64, entries: &[Value]) {
    std::fs::create_dir_all(dir).unwrap();
    let mut text = json!({
        "api": 1, "outcomes": 1, "file": "outcomes.jsonl",
        "days": DAYS, "by": by, "generatedAt": iso(generated),
    })
    .to_string();
    for e in entries {
        text.push('\n');
        text.push_str(&e.to_string());
    }
    std::fs::write(dir.join("outcomes.jsonl"), text + "\n").unwrap();
}

fn dirs(name: &str) -> (PathBuf, Dirs) {
    let root = std::env::temp_dir().join(format!("mcp-out-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let live = root.join("live");
    let mcp = root.join("mcp");
    std::fs::create_dir_all(&live).unwrap();
    let dir = Dirs {
        live,
        sessions: mcp.clone(),
        logs: root.join("no-logs"),
    };
    (root, dir)
}

fn call(dir: &Dirs, args: Value) -> Result<Value, ToolError> {
    match mcp::tools::call(dir, "factory_outcomes", &args)? {
        Output::Json(v) => Ok(v),
        other => panic!("not JSON: {other:?}"),
    }
}

fn ok(dir: &Dirs, args: Value) -> Value {
    call(dir, args).unwrap_or_else(|e| panic!("not ok: {e:?}"))
}

fn bad(dir: &Dirs, args: Value) -> String {
    match call(dir, args) {
        Err(ToolError::BadArgs(why)) => why,
        other => panic!("not bad arguments: {other:?}"),
    }
}

fn versions(entries: &[Value]) -> Vec<String> {
    entries
        .iter()
        .map(|e| {
            format!(
                "{}/{}",
                e["tehasVersion"].as_str().unwrap_or("-"),
                e["rukasVersion"].as_str().unwrap_or("-"),
            )
        })
        .collect()
}

/// The two pairs of two weeks of runs, and the counts of how each ended.
fn version_entries() -> Vec<Value> {
    vec![
        json!({
            "tehasVersion": "v0.14.2", "rukasVersion": "r1", "runs": 9,
            "outcomes": {"merged": 6, "parked": 2, "failed": 1},
            "failureKinds": {"tests": 1}, "parkReasons": {"needs-human": 2},
        }),
        json!({
            "tehasVersion": "v0.14.1", "rukasVersion": "r1", "runs": 4,
            "outcomes": {"merged": 3, "failed": 1},
            "failureKinds": {"provider": 1}, "parkReasons": {},
        }),
    ]
}

fn weeks_entries() -> Vec<Value> {
    vec![
        json!({"week": "2026-10-12", "runs": 5, "outcomes": {"merged": 5}, "failureKinds": {}, "parkReasons": {}}),
        json!({"week": "2026-10-05", "runs": 8, "outcomes": {"merged": 4, "parked": 4}, "failureKinds": {}, "parkReasons": {"needs-human": 4}}),
    ]
}

#[test]
fn the_counts_of_a_version_pair_come_the_most_runs_first() {
    let (root, dir) = dirs("versions");
    write(&dir.sessions, "version", now(), &version_entries());
    let out = ok(&dir, json!({}));
    assert_eq!(out["api"], 1);
    assert_eq!(out["by"], "version");
    assert_eq!(out["days"], DAYS, "the whole retention is named");
    assert_eq!(out["stale"], false);
    assert_eq!(
        versions(&out["entries"].as_array().unwrap().clone()),
        ["v0.14.2/r1", "v0.14.1/r1"]
    );
    assert_eq!(out["entries"][0]["runs"], 9);
    assert_eq!(out["entries"][0]["outcomes"]["parked"], 2);
    assert_eq!(out["entries"][0]["parkReasons"]["needs-human"], 2);
    drop(root);
}

#[test]
fn a_version_argument_keeps_one_pair() {
    let (root, dir) = dirs("one-version");
    write(&dir.sessions, "version", now(), &version_entries());
    let out = ok(&dir, json!({"tehasVersion": "v0.14.1"}));
    assert_eq!(out["entries"].as_array().unwrap().len(), 1);
    assert_eq!(out["entries"][0]["tehasVersion"], "v0.14.1");
    // Asked for a release the file holds none of: an empty list, not an error.
    let out = ok(&dir, json!({"tehasVersion": "v9.9.9"}));
    assert!(out["entries"].as_array().unwrap().is_empty());
    // Both halves of the pair are matched.
    let out = ok(
        &dir,
        json!({"tehasVersion": "v0.14.2", "rukasVersion": "r1"}),
    );
    assert_eq!(out["entries"].as_array().unwrap().len(), 1);
    drop(root);
}

#[test]
fn the_weeks_are_filtered_by_their_whole_week() {
    let (root, dir) = dirs("weeks");
    write(&dir.sessions, "week", now(), &weeks_entries());
    // A day inside the older week keeps that week whole: it stands for seven
    // days, and a count cannot be split between them.
    let out = ok(
        &dir,
        json!({"since": "2026-10-08", "until": "2026-10-09", "by": "week"}),
    );
    let weeks: Vec<&str> = out["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["week"].as_str().unwrap())
        .collect();
    assert_eq!(weeks, ["2026-10-05"]);
    // A window from the Monday of the newer week on: that week alone, the
    // older one being wholly before it.
    let out = ok(&dir, json!({"since": "2026-10-12", "by": "week"}));
    assert_eq!(out["entries"].as_array().unwrap().len(), 1);
    assert_eq!(out["entries"][0]["week"], "2026-10-12");
    // A window that reaches both weeks gives both, newest first.
    let out = ok(&dir, json!({"since": "2026-10-01", "by": "week"}));
    assert_eq!(out["entries"].as_array().unwrap().len(), 2);
    assert_eq!(out["entries"][0]["week"], "2026-10-12");
    // The newer week alone.
    let out = ok(&dir, json!({"since": "2026-10-13", "by": "week"}));
    assert_eq!(out["entries"].as_array().unwrap().len(), 1);
    assert_eq!(out["entries"][0]["week"], "2026-10-12");
    drop(root);
}

#[test]
fn a_grouping_the_file_does_not_give_is_refused() {
    let (root, dir) = dirs("wrong-by");
    write(&dir.sessions, "version", now(), &version_entries());
    let why = bad(&dir, json!({"by": "week"}));
    assert!(why.contains("version"), "{why}");
    // A day bound is read as its own day, and a full time as its instant.
    assert!(call(&dir, json!({"since": "2026-10-08"})).is_ok());
    assert!(call(&dir, json!({"until": "2026-10-08T09:30:00Z"})).is_ok());
    drop(root);
}

#[test]
fn bad_arguments_are_refused_and_named() {
    let (root, dir) = dirs("bad");
    write(&dir.sessions, "version", now(), &version_entries());
    for (args, part) in [
        (json!({"tehasVersion": "v1 v2"}), "tehasVersion"),
        (json!({"rukasVersion": ""}), "rukasVersion"),
        (json!({"tehasVersion": "../x"}), "tehasVersion"),
        (json!({"since": "yesterday"}), "since"),
        (json!({"until": "2026-13-01"}), "until"),
        (json!({"by": "day"}), "by"),
    ] {
        let why = bad(&dir, args.clone());
        assert!(why.contains(part), "{args:?} gave {why}");
    }
    // An argument the tool does not have is not an error: as in the other
    // tools, the schema refuses it and the call answers without it.
    assert!(call(&dir, json!({"issue": 6})).is_ok());
    drop(root);
}

#[test]
fn a_file_of_another_api_or_no_header_is_an_error() {
    let (root, dir) = dirs("bad-header");
    std::fs::create_dir_all(&dir.sessions).unwrap();
    std::fs::write(dir.sessions.join("outcomes.jsonl"), "not json\n").unwrap();
    assert!(matches!(call(&dir, json!({})), Err(ToolError::File { .. })));
    std::fs::write(
        dir.sessions.join("outcomes.jsonl"),
        format!(
            "{}\n",
            json!({"api": 2, "by": "version", "generatedAt": iso(now())})
        ),
    )
    .unwrap();
    assert!(matches!(call(&dir, json!({})), Err(ToolError::File { .. })));
    // A header that names no reach, or no shape, or a time of another type,
    // is no header of a counts file: the tool would answer with a reach it
    // cannot name, or a shape it did not agree to, or a stale flag that
    // defaulted to true rather than one the file earned.
    for head in [
        json!({"api": 1, "outcomes": 1, "file": "outcomes.jsonl",
               "by": "version", "generatedAt": iso(now())}),
        json!({"api": 1, "outcomes": 2, "file": "outcomes.jsonl",
               "days": DAYS, "by": "version", "generatedAt": iso(now())}),
        json!({"api": 1, "outcomes": 1, "file": "outcomes.jsonl",
               "days": DAYS, "by": "version", "generatedAt": 1792000000}),
    ] {
        std::fs::write(dir.sessions.join("outcomes.jsonl"), format!("{}\n", head)).unwrap();
        let why = match call(&dir, json!({})) {
            Err(ToolError::File { why, .. }) => why,
            other => panic!("{head:?} gave {other:?}"),
        };
        assert!(!why.contains("not written"), "{head:?} gave {why}");
    }
    // A header written for another file is no counts file, even of the right
    // api: the writer names the file it writes.
    std::fs::write(
        dir.sessions.join("outcomes.jsonl"),
        format!(
            "{}\n",
            json!({"api": 1, "outcomes": 1, "file": "sessions.jsonl",
                   "days": DAYS, "by": "version", "generatedAt": iso(now())})
        ),
    )
    .unwrap();
    let why = match call(&dir, json!({})) {
        Err(ToolError::File { why, .. }) => why,
        other => panic!("not a file error: {other:?}"),
    };
    assert!(why.contains("another file"), "{why}");
    // Nothing written at all: the tool says so, rather than answering nothing.
    std::fs::remove_file(dir.sessions.join("outcomes.jsonl")).unwrap();
    let why = match call(&dir, json!({})) {
        Err(ToolError::File { why, .. }) => why,
        other => panic!("not a file error: {other:?}"),
    };
    assert!(why.contains("not written"), "{why}");
    drop(root);
}

#[test]
fn a_file_the_host_has_not_written_lately_is_marked_stale() {
    let (root, dir) = dirs("stale");
    write(
        &dir.sessions,
        "version",
        now() - 3 * DAY,
        &version_entries(),
    );
    let out = ok(&dir, json!({}));
    assert_eq!(out["stale"], true);
    assert_eq!(out["entries"].as_array().unwrap().len(), 2, "still counted");
    write(&dir.sessions, "version", now() - 3600, &version_entries());
    assert_eq!(ok(&dir, json!({}))["stale"], false);
    drop(root);
}

#[test]
fn a_line_that_is_not_an_entry_is_skipped() {
    let (root, dir) = dirs("bad-lines");
    std::fs::create_dir_all(&dir.sessions).unwrap();
    let head = json!({
        "api": 1,
        "outcomes": 1,
        "file": "outcomes.jsonl",
        "days": DAYS,
        "by": "version",
        "generatedAt": iso(now()),
    })
    .to_string();
    let body = version_entries()
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(
        dir.sessions.join("outcomes.jsonl"),
        format!("{head}\nnonsense\n{body}\n[]\n\n"),
    )
    .unwrap();
    let out = ok(&dir, json!({}));
    assert_eq!(out["entries"].as_array().unwrap().len(), 2);
    drop(root);
}

#[test]
fn the_counts_hold_no_issue_number_or_cause() {
    let (root, dir) = dirs("no-leak");
    // What the writer would never put in the file, planted here to show the
    // tool could not serve it if it did: it copies only the keys it names.
    write(
        &dir.sessions,
        "version",
        now(),
        &[json!({
            "tehasVersion": "v1", "runs": 2,
            "outcomes": {"failed": 2}, "failureKinds": {"tests": 1}, "parkReasons": {},
            "issue": 98765, "cause": "the whole of a log line with a token sk-abc123",
            "title": "the issue's title", "started": iso(now() - DAY),
        })],
    );
    let text = ok(&dir, json!({})).to_string();
    // 98765 and not 42: the answer's generatedAt can hold any two digits.
    for forbidden in [
        "sk-abc123",
        "log line",
        "98765",
        "title",
        "started",
        "cause",
    ] {
        assert!(
            !text.contains(forbidden),
            "the answer holds {forbidden:?}: {text}"
        );
    }
    assert!(text.contains("tests"), "the kinds are served: {text}");
    drop(root);
}

#[test]
fn the_tool_is_listed_with_a_schema_that_matches_the_answer() {
    let tool = list()
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "factory_outcomes")
        .unwrap()
        .clone();
    assert_eq!(tool["inputSchema"]["type"], "object");
    assert_eq!(tool["inputSchema"]["additionalProperties"], false);
    for key in ["tehasVersion", "rukasVersion", "since", "until", "by"] {
        assert_eq!(
            tool["inputSchema"]["properties"][key]["type"], "string",
            "{key}"
        );
    }
    assert_eq!(
        tool["inputSchema"]["properties"]["by"]["enum"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let (root, dir) = dirs("schema");
    write(&dir.sessions, "version", now(), &version_entries());
    let out = ok(&dir, json!({}));
    let entry = &out["entries"][0];
    // Every key the schema promises, and only those: the spec is built from
    // the same list as the schema, so one statement of the entry's keys
    // answers both.
    let want: BTreeSet<String> = outcomes_schema::spec("version")
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    let got: BTreeSet<String> = entry.as_object().unwrap().keys().cloned().collect();
    assert_eq!(got, want);
    // The keys of the grouping the file does not hold are not served, and the
    // keys it does are: a spec of one grouping cannot describe the other.
    let week: BTreeSet<String> = outcomes_schema::spec("week")
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert!(!week.contains("tehasVersion") && !week.contains("rukasVersion"));
    assert!(week.contains("week"));
    for k in ["runs", "outcomes", "failureKinds", "parkReasons"] {
        assert!(entry.get(k).is_some(), "{k}");
    }
    // The types the outputSchema promises: the header's four fields, and a
    // count that is a whole number and never negative.
    assert_eq!(out["by"], "version");
    assert_eq!(out["days"].as_u64(), Some(DAYS as u64));
    assert!(out["generatedAt"].is_string());
    assert!(out["stale"].is_boolean());
    assert!(entry["runs"].as_u64().is_some());
    for map in ["outcomes", "failureKinds", "parkReasons"] {
        for (k, v) in entry[map].as_object().unwrap() {
            assert!(v.as_u64().is_some(), "{map}.{k} is {v}, not a count");
        }
    }
    drop(root);
}

#[test]
fn a_count_of_another_shape_is_left_out_rather_than_served() {
    let (root, dir) = dirs("bad-counts");
    // A map of counts whose values are not counts is no map of counts: the
    // schema of the tool promises integers, so pick serves none of it rather
    // than a string, a boolean or a fraction.
    write(
        &dir.sessions,
        "version",
        now(),
        &[
            json!({
                "tehasVersion": "v1", "runs": 3,
                "outcomes": {"cause": "a whole failure cause of 200 characters", "ok": true},
                "failureKinds": {"tests": 1}, "parkReasons": {},
            }),
            // A run count of another type is no count: the same rule, on the
            // scalar the spec names rather than the map it leaves open.
            json!({
                "tehasVersion": "v2", "runs": 3.7,
                "outcomes": {"merged": 1}, "failureKinds": {}, "parkReasons": {},
            }),
        ],
    );
    let out = ok(&dir, json!({}));
    let entry = &out["entries"][0];
    let keys: BTreeSet<String> = entry.as_object().unwrap().keys().cloned().collect();
    assert_eq!(
        keys,
        ["tehasVersion", "runs", "failureKinds", "parkReasons"]
            .iter()
            .map(|s| s.to_string())
            .collect::<BTreeSet<String>>(),
        "the map of non-counts is left out, as any key the file invents"
    );
    assert!(entry.get("outcomes").is_none());
    assert_eq!(entry["runs"], 3, "a run count stays whole");
    // A run count of another type is no count, and the entry served without it
    // is not served at all: a group of runs of no known size answers nothing.
    assert_eq!(
        out["entries"].as_array().unwrap().len(),
        1,
        "{}",
        out["entries"]
    );
    assert_eq!(
        entry["failureKinds"]["tests"], 1,
        "a map of counts is served"
    );
    let text = out.to_string();
    for forbidden in ["200 characters", "3.7"] {
        assert!(
            !text.contains(forbidden),
            "{forbidden:?} was served: {text}"
        );
    }
    drop(root);
}

#[test]
fn a_version_the_writer_accepts_is_a_version_the_reader_accepts() {
    // The two ends of one contract: a group key the writer may name must be
    // matchable by the reader's filter, or a call for it answers nothing.
    for v in [
        "v0.14.2",
        "abc1234",
        "0",
        "a-b_c.d",
        "v".repeat(64).as_str(),
    ] {
        assert!(mcp::tools::WORD.is_match(v), "{v}");
    }
    // A word the writer refuses is refused here too, so neither end accepts
    // what the other cannot name.
    for v in ["", "a b", "../x", "x/y", "v\n", &"v".repeat(65)] {
        assert!(!mcp::tools::WORD.is_match(v), "{v}");
    }
}
