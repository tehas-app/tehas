// factory_decisions: the dispatcher's last decisions from the end of
// dispatch.log, as `grep -E ' (start|skip|wait|hold|drop the refresh of) #' | tail -n N`
// shows, and the words of both pinned to the dispatcher's source.

use mcp::tools::{list_scoped, Dirs, Output, Scope, ToolError};
use regex::Regex;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use tehas_core::testdir::TestDir;

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

// A logs directory with this dispatch.log.
fn logs_with(name: &str, text: &str) -> TestDir {
    let l = TestDir::new(&format!("mcp-decisions-{name}"));
    std::fs::write(l.join("dispatch.log"), text).unwrap();
    l
}

// What the issue's grep and tail show of the fixture, as the four fields,
// read by a pattern of its own rather than by the tool's parse. The words are
// written out here, not read from the table, and pinned to the dispatcher's
// source below.
const GREP_WORDS: [&str; 5] = ["start", "skip", "wait", "hold", "drop the refresh of"];

static TAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ) (start|skip|wait|hold|drop the refresh of) #([1-9]\d{0,18})(?:: (.*)| \((.*)\))?$",
    )
    .unwrap()
});

fn grep_tail(text: &str, n: usize) -> Vec<Value> {
    let hits: Vec<Value> = text
        .lines()
        .filter_map(|l| TAIL.captures(l))
        .map(|c| {
            let word = match &c[2] {
                "drop the refresh of" => "drop",
                w => w,
            };
            let reason = c.get(4).or(c.get(5)).map_or("", |m| m.as_str());
            json!({"time": &c[1], "decision": word,
                   "issue": c[3].parse::<u64>().unwrap(), "reason": reason})
        })
        .collect();
    hits[hits.len().saturating_sub(n)..].to_vec()
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

// Every text of the dispatcher's source that starts with words and then an
// issue number: the start of a line of `dispatch.log`, or of a reason or a
// comment within one. A word the dispatcher starts to write that is not here
// fails `the_words_are_the_dispatchers`, so whether it is a decision is
// decided, not missed.
const ISSUE_WORDS: [&str; 29] = [
    "clear",
    "clear pull request",
    "done planning",
    "drop the refresh of",
    "hold",
    "ignore",
    "issue",
    "it is not the one open pull request that closes",
    "lift",
    "no cycle holds the lock of",
    "no cycle runs on",
    "park",
    "planning",
    "pull request",
    "refining",
    "skip",
    "start",
    "start planning",
    "stop",
    "the cycle for",
    "the planning turn of",
    "the state of",
    "wait",
    "what",
    "would post the planning turn of",
    "would post the waiting planning turn of",
    "would run a cycle for",
    "would run a planning turn for",
    "would start a planning session for",
];

// The string literals of the dispatcher's source, its tests left out.
fn dispatcher_literals() -> Vec<String> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../dispatch/src");
    let lit = Regex::new(r#""((?:[^"\\]|\\.)*)""#).unwrap();
    let mut out = Vec::new();
    let mut dirs = vec![src];
    while let Some(dir) = dirs.pop() {
        for e in std::fs::read_dir(&dir).unwrap() {
            let path = e.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if path.is_dir() {
                if name != "tests" {
                    dirs.push(path);
                }
                continue;
            }
            if !name.ends_with(".rs") || name.ends_with("_tests.rs") || name == "tests.rs" {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            let code = text.split("#[cfg(test)]").next().unwrap_or("");
            out.extend(lit.captures_iter(code).map(|c| c[1].to_string()));
        }
    }
    assert!(out.len() > 100, "the dispatcher's source was not read");
    out
}

#[test]
fn the_words_are_the_dispatchers() {
    let literals = dispatcher_literals();
    let start = Regex::new(r"^([a-z][a-z ]*?) #\{").unwrap();
    let mut written: BTreeSet<String> = literals
        .iter()
        .filter_map(|l| start.captures(l).map(|c| c[1].to_string()))
        .collect();
    // `deny` logs `{verb} #{n}: ...` with the verb it is given.
    let deny = Regex::new(r#"\.deny\(\s*"([a-z ]+)""#).unwrap();
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../dispatch/src/decide.rs");
    let text = std::fs::read_to_string(src).unwrap();
    written.extend(deny.captures_iter(&text).map(|c| c[1].to_string()));
    let listed: BTreeSet<String> = ISSUE_WORDS.iter().map(|w| w.to_string()).collect();
    assert_eq!(written, listed, "the dispatcher's words and ISSUE_WORDS");

    let decided: BTreeSet<&str> = mcp::decisions::DECISIONS.iter().map(|r| r.logged).collect();
    assert_eq!(
        decided,
        GREP_WORDS.iter().copied().collect(),
        "the grep and the table"
    );
    for (word, _, _) in mcp::decisions::WRITER_NON_DECISIONS {
        let said =
            written.contains(word) || literals.iter().any(|l| l.starts_with(&format!("{word}:")));
        assert!(said, "{word} is no word of the dispatcher");
        assert!(!decided.contains(word), "{word}");
    }
    let t = "2026-10-29T16:23:14Z";
    for word in &written {
        let (colon, brackets) = (format!("{t} {word} #7: x"), format!("{t} {word} #7 (x)"));
        let colon = mcp::decisions::parse(&colon);
        let brackets = mcp::decisions::parse(&brackets);
        if decided.contains(word.as_str()) {
            assert!(colon.is_some(), "{word}");
            assert_eq!(brackets.is_some(), word == "start", "{word}");
        } else {
            assert_eq!((colon, brackets), (None, None), "{word}");
        }
    }
}

// Every hold the dispatcher's source writes is a line the tests read: the
// table's sample or one of `HOST_HOLD_LINES`, and each of those is one of the
// source's holds.
#[test]
fn the_hold_lines_are_the_dispatchers() {
    let holes = Regex::new(r"\{[^{}]*\}").unwrap();
    let shapes: Vec<Regex> = dispatcher_literals()
        .iter()
        .filter(|l| l.starts_with("hold #{"))
        .map(|l| {
            let parts: Vec<String> = holes.split(l).map(regex::escape).collect();
            Regex::new(&format!("^{}$", parts.join(".+"))).unwrap()
        })
        .collect();
    assert!(!shapes.is_empty(), "the dispatcher writes no hold");
    let lines: Vec<&str> = mcp::decisions::HOST_HOLD_LINES
        .iter()
        .copied()
        .chain(
            mcp::decisions::DECISIONS
                .iter()
                .filter(|r| r.word == "hold")
                .map(|r| r.sample),
        )
        .map(|l| l.split_once(' ').expect("a timed line").1)
        .collect();
    for shape in &shapes {
        assert!(
            lines.iter().any(|l| shape.is_match(l)),
            "no line of the hold {shape}"
        );
    }
    for line in &lines {
        assert!(
            shapes.iter().any(|s| s.is_match(line)),
            "{line} is no hold of the dispatcher"
        );
    }
}

// A hold line of each shape a host's dispatcher writes today is answered, so a
// writer's shape that falls out of the table fails a test here instead of
// vanishing from the answer. The hold lines and the table's own sample are
// named by the crate, so this test cannot quietly read a different line.
#[test]
fn every_hold_line_a_host_writes_is_answered() {
    let log = [
        "2026-10-20T10:00:00Z start #1 (labelled by alice)",
        mcp::decisions::HOST_HOLD_LINES[0],
        mcp::decisions::HOST_HOLD_LINES[1],
        mcp::decisions::HOST_HOLD_LINES[2],
        mcp::decisions::DECISIONS[3].sample,
    ]
    .join("\n");
    let l = logs_with("holds", &log);
    let out = call_in(&l, &json!({})).unwrap();
    let lines = out["decisions"].as_array().unwrap();
    assert_eq!(lines.len(), 5, "{out}");
    for (line, d) in log.lines().zip(lines) {
        let (_, rest) = line.split_once(' ').expect("a timed line");
        let (word, _) = rest.split_once(" #").expect("an issue");
        assert_eq!(d["decision"], word, "{line}");
        assert!(
            d["reason"].as_str().is_some_and(|r| !r.is_empty()),
            "{line} gives no reason"
        );
    }
}

#[test]
fn other_lines_are_not_decisions() {
    let mut log = String::from(
        "2026-10-20T10:00:00Z start planning #9 (labelled by alice)\n\
         2026-10-20T10:00:01Z drop #8: no writer logs this shape\n\
         2026-10-20T10:00:03Z done #7 rc=0 status=merged\n\
         /home/x/libexec/dispatch: line 197: no such file\n\
         not a time skip #4: no\n\
         2026-10-20T10:00:04Z skip #0: no issue\n\
         2026-10-20T10:00:05Z wait #5x: not a number\n",
    );
    // The shapes the writers log today that are no decision, from the table's
    // own list of them, and one wait that is.
    for (_, line, _) in mcp::decisions::WRITER_NON_DECISIONS {
        log.push_str(line);
        log.push('\n');
    }
    log.push_str("2026-10-20T10:00:06Z wait #6: its cycle is still running\n");
    let l = logs_with("other", &log);
    let out = call_in(&l, &json!({})).unwrap();
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
    let l = logs_with(
        "mask",
        &format!(
            "2026-10-20T10:00:00Z skip #1: fetch failed with {token} at {address}\n\
             2026-10-20T10:00:01Z skip #2: {long}\n"
        ),
    );
    let out = call_in(&l, &json!({})).unwrap();
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
    let l = TestDir::new("mcp-decisions-huge");
    let path = l.join("dispatch.log");
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
    let out = call_in(&l, &json!({})).unwrap();
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
    let l = TestDir::new("mcp-decisions-missing");
    match call_in(&l, &json!({})) {
        Err(ToolError::File { file, .. }) => assert_eq!(file, "dispatch.log"),
        other => panic!("{other:?}"),
    }
    let target = l.join("other.log");
    std::fs::write(&target, "2026-10-20T10:00:00Z skip #1: through a link\n").unwrap();
    std::os::unix::fs::symlink(&target, l.join("dispatch.log")).unwrap();
    match call_in(&l, &json!({})) {
        Err(ToolError::File { file, why }) => {
            assert_eq!(file, "dispatch.log");
            assert!(why.contains("plain file"), "{why}");
        }
        other => panic!("{other:?}"),
    }
    let d = TestDir::new("mcp-decisions-dir");
    std::fs::create_dir(d.join("dispatch.log")).unwrap();
    assert!(matches!(
        call_in(&d, &json!({})),
        Err(ToolError::File { .. })
    ));
}

#[test]
fn an_empty_log_has_no_decisions() {
    let l = logs_with("empty", "");
    let out = call_in(&l, &json!({})).unwrap();
    assert_eq!(out["decisions"], json!([]));
    assert_eq!(out["whole"], true);
}

// Every field of the answer is in the outputSchema, and every property of
// both schemas says what it is.
#[test]
fn the_schema_describes_the_answer() {
    let tool = list_scoped(Scope::Full)
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
