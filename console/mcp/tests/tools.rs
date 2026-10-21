// The tools against live files with planted keys that must never come out.

use mcp::snapshot::Snapshot;
use mcp::tools::{list, Dirs, Output, ToolError};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn live() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/live")
}

// A tool over the live files in `live`, for the tools that answer JSON.
fn call(live: &Path, name: &str, args: &Value) -> Result<Value, ToolError> {
    let dirs = Dirs {
        live: live.to_path_buf(),
        sessions: live.join("no-sessions"),
        logs: live.join("no-logs"),
    };
    match mcp::tools::call(&dirs, name, args)? {
        Output::Json(v) => Ok(v),
        other => panic!("{name} gave {other:?}"),
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// "YYYY-MM-DDTHH:MM:SSZ" for seconds since 1970.
fn iso(t: i64) -> String {
    let (days, s) = (t.div_euclid(86400), t.rem_euclid(86400));
    // Civil from days (Howard Hinnant's algorithm).
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        s / 3600,
        s / 60 % 60,
        s % 60
    )
}

// A copy of the fixtures to change, removed when dropped.
struct Copy(PathBuf);

impl Copy {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("mcp-tools-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for e in std::fs::read_dir(live()).unwrap() {
            let e = e.unwrap();
            std::fs::copy(e.path(), dir.join(e.file_name())).unwrap();
        }
        Copy(dir)
    }
}

impl Drop for Copy {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn paths(v: &Value, prefix: &str, out: &mut BTreeSet<String>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                let p = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                out.insert(p.clone());
                paths(x, &p, out);
            }
        }
        Value::Array(a) => {
            for x in a {
                paths(x, &format!("{prefix}[]"), out);
            }
        }
        _ => {}
    }
}

fn key_paths(v: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    paths(v, "", &mut out);
    out
}

const NAMES: [&str; 5] = [
    "factory_status",
    "factory_issues",
    "factory_endings",
    "factory_console",
    "factory_stats",
];

fn args(name: &str) -> Value {
    if name == "factory_console" {
        json!({"issue": 7})
    } else {
        json!({})
    }
}

#[test]
fn the_tools_are_listed_in_order_with_their_schemas() {
    let tools = list();
    let names: Vec<&str> = tools
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    let mut want = NAMES.to_vec();
    want.extend(["factory_sessions", "factory_session"]);
    assert_eq!(names, want);
    for t in tools.as_array().unwrap() {
        assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
        assert_eq!(t["outputSchema"]["type"], "object", "{}", t["name"]);
        assert!(t["description"].as_str().unwrap().len() > 10);
    }
}

#[test]
fn no_tool_returns_a_planted_key() {
    for name in NAMES {
        let out = call(&live(), name, &args(name)).unwrap();
        let text = out.to_string();
        for bad in ["SECRET", "/home/x", "labels", "buzzwords", "disk", "slice"] {
            assert!(!text.contains(bad), "{name} returned {bad}: {text}");
        }
        assert_eq!(out["api"], 1, "{name}");
    }
}

#[test]
fn status_says_the_queue_and_that_an_old_poll_is_stale() {
    let out = call(&live(), "factory_status", &json!({})).unwrap();
    assert_eq!(out["queue"]["inLine"], 2);
    assert_eq!(out["status"], "pass");
    assert_eq!(out["stale"], true);
    assert_eq!(out["cycles"][0]["issue"], 7);
    assert_eq!(out["platform"]["behind"], 0);
}

#[test]
fn issues_by_stage() {
    let all = call(&live(), "factory_issues", &json!({})).unwrap();
    assert_eq!(all["queued"][0]["title"], "Queued issue");
    assert_eq!(all["planning"], json!({"count": 1, "issues": [6]}));
    assert_eq!(all["completed"][0]["closedAt"], "2026-10-19T18:00:00Z");
    let q = call(&live(), "factory_issues", &json!({"stage": "queued"})).unwrap();
    assert!(q.get("queued").is_some());
    for other in ["running", "awaiting", "stopped", "completed", "planning"] {
        assert!(q.get(other).is_none(), "{other}");
    }
    assert!(matches!(
        call(&live(), "factory_issues", &json!({"stage": "nope"})),
        Err(ToolError::BadArgs(_))
    ));
}

#[test]
fn endings_and_failures() {
    let out = call(&live(), "factory_endings", &json!({})).unwrap();
    assert_eq!(out["endings"].as_array().unwrap().len(), 2);
    assert_eq!(out["failed"][0]["issue"], 4);
}

#[test]
fn the_console_of_a_running_cycle() {
    let out = call(&live(), "factory_console", &json!({"issue": 7})).unwrap();
    assert_eq!(out["step"], "develop");
    assert!(out["console"].as_str().unwrap().contains("last line"));
    let short = call(&live(), "factory_console", &json!({"issue": 7, "bytes": 5})).unwrap();
    assert!(short["console"].as_str().unwrap().len() <= 5);
    assert!("last line\n".ends_with(short["console"].as_str().unwrap()));
}

#[test]
fn a_cut_never_splits_a_character() {
    // "✓\nlast line\n" is 13 bytes; asking for 12 cuts inside the check mark.
    let out = call(
        &live(),
        "factory_console",
        &json!({"issue": 7, "bytes": 12}),
    )
    .unwrap();
    assert_eq!(out["console"], "\nlast line\n");
}

#[test]
fn a_console_file_is_opened_only_by_its_own_name() {
    for name in [
        "../status.json",
        "console-8.txt",
        "/etc/passwd",
        "console-7.txt/..",
    ] {
        let c = Copy::new("console");
        let p = c.0.join("running.json");
        let mut r: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
        r["cycles"][0]["console"] = json!(name);
        std::fs::write(&p, r.to_string()).unwrap();
        let out = call(&c.0, "factory_console", &json!({"issue": 7})).unwrap();
        assert!(out.get("console").is_none(), "{name}");
        assert_eq!(out["step"], "develop");
    }
}

#[test]
fn console_arguments_are_checked() {
    for a in [
        json!({}),
        json!({"issue": "7"}),
        json!({"issue": -1}),
        json!({"issue": 7, "bytes": 0}),
        json!({"issue": 7, "bytes": 65537}),
        json!({"issue": 99}),
    ] {
        assert!(
            matches!(
                call(&live(), "factory_console", &a),
                Err(ToolError::BadArgs(_))
            ),
            "{a}"
        );
    }
}

#[test]
fn a_missing_or_broken_file_is_named() {
    let c = Copy::new("files");
    std::fs::remove_file(c.0.join("analytics.json")).unwrap();
    match call(&c.0, "factory_stats", &json!({})) {
        Err(ToolError::File { file, .. }) => assert_eq!(file, "analytics.json"),
        other => panic!("{other:?}"),
    }
    std::fs::write(c.0.join("status.json"), "{\"repo\":").unwrap();
    match call(&c.0, "factory_issues", &json!({})) {
        Err(ToolError::File { file, .. }) => assert_eq!(file, "status.json"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_unknown_tool_is_unknown() {
    assert!(matches!(
        call(&live(), "factory_delete", &json!({})),
        Err(ToolError::Unknown(_))
    ));
}

// Adding a key to what a tool returns is a change someone makes on purpose:
// it shows up here.
#[test]
fn every_key_is_listed() {
    let expected: [(&str, &[&str]); 5] = [
        (
            "factory_status",
            &[
                "api",
                "status",
                "output",
                "time",
                "stale",
                "generatedAt",
                "repo",
                "paused",
                "planning",
                "limit",
                "limit.state",
                "queue",
                "queue.inLine",
                "queue.building",
                "queue.review",
                "queue.stopped",
                "queue.time",
                "cycles",
                "cycles[].issue",
                "cycles[].running",
                "cycles[].status",
                "cycles[].step",
                "cycles[].startedAt",
                "cycles[].time",
                "demand",
                "demand.waiting",
                "demand.oldest",
                "demand.time",
                "platform",
                "platform.commit",
                "platform.behind",
                "platform.upstream",
                "platform.fetchedAt",
            ],
        ),
        (
            "factory_issues",
            &[
                "api",
                "generatedAt",
                "queued",
                "queued[].number",
                "queued[].title",
                "queued[].url",
                "queued[].updatedAt",
                "running",
                "running[].number",
                "running[].title",
                "running[].url",
                "running[].updatedAt",
                "awaiting",
                "awaiting[].number",
                "awaiting[].title",
                "awaiting[].url",
                "awaiting[].updatedAt",
                "stopped",
                "stopped[].number",
                "stopped[].title",
                "stopped[].url",
                "stopped[].updatedAt",
                "completed",
                "completed[].number",
                "completed[].title",
                "completed[].url",
                "completed[].updatedAt",
                "completed[].closedAt",
                "planning",
                "planning.count",
                "planning.issues",
            ],
        ),
        (
            "factory_endings",
            &[
                "api",
                "endings",
                "endings[].issue",
                "endings[].status",
                "endings[].step",
                "endings[].startedAt",
                "endings[].updatedAt",
                "failed",
                "failed[].issue",
                "failed[].status",
                "failed[].step",
                "failed[].startedAt",
                "failed[].time",
            ],
        ),
        (
            "factory_console",
            &[
                "api",
                "issue",
                "step",
                "status",
                "statusText",
                "startedAt",
                "updatedAt",
                "console",
            ],
        ),
        (
            "factory_stats",
            &[
                "api",
                "from",
                "to",
                "report",
                "runs",
                "runs.n",
                "runs.merged",
                "runs.pr",
                "runs.handoff",
                "runs.parked",
                "runs.failed",
                "runs.refused",
                "runs.stopped",
                "shipped",
                "shipped.n",
                "cycleTime",
                "cycleTime.n",
                "cycleTime.median",
                "cycleTime.p90",
                "leadTime",
                "leadTime.n",
                "leadTime.median",
                "leadTime.p90",
                "firstPassYield",
                "firstPassYield.n",
                "firstPassYield.share",
                "handoffRate",
                "handoffRate.n",
                "handoffRate.share",
                "availability",
                "availability.n",
                "availability.seconds",
                "availability.share",
                "machineTime",
                "machineTime.n",
                "machineTime.seconds",
                "machineTime.perShipped",
                "energy",
                "energy.n",
                "energy.kwh",
                "energy.perShipped",
                "tokens",
                "tokens.n",
                "tokens.input",
                "tokens.output",
                "tokens.cacheRead",
                "tokens.total",
                "tokens.perShipped",
                "planned",
                "planned.n",
                "planned.inLine",
                "planned.planning",
                "planned.trend",
                "planned.trend[].day",
                "planned.trend[].n",
                "planningTurns",
                "planningTurns.n",
                "stations",
                "stations.n",
                "stations.steps",
                "stations.steps[].step",
                "stations.steps[].n",
                "stations.steps[].passed",
                "stations.steps[].share",
                "stopReasons",
                "stopReasons.n",
                "stopReasons.top",
                "stopReasons.top[].reason",
                "stopReasons.top[].n",
                "versions",
                "versions[].tehas",
                "versions[].rukas",
                "versions[].n",
                "versions[].medianSeconds",
                "versions[].medianTokens",
                "versions[].prShare",
                "daily",
                "daily[].day",
                "daily[].runs",
                "daily[].shipped",
                "daily[].failed",
                "daily[].minutes",
            ],
        ),
    ];
    for (name, keys) in expected {
        let got = key_paths(&call(&live(), name, &args(name)).unwrap());
        let want: BTreeSet<String> = keys.iter().map(|s| s.to_string()).collect();
        assert_eq!(got, want, "{name}");
    }
}

// Only the tail is read: a console far larger than the container's memory is
// answered with its last bytes.
#[test]
fn a_huge_console_is_read_from_its_end_only() {
    let c = Copy::new("huge");
    let f = std::fs::OpenOptions::new()
        .write(true)
        .open(c.0.join("console-7.txt"))
        .unwrap();
    f.set_len(1 << 40).unwrap();
    let out = call(&c.0, "factory_console", &json!({"issue": 7, "bytes": 100})).unwrap();
    assert_eq!(out["console"].as_str().unwrap().len(), 100);
}

// A directory with sessions.jsonl of these lines, removed when dropped.
fn snapshot_dir(name: &str, lines: &[String]) -> Copy {
    let dir = std::env::temp_dir().join(format!("mcp-snap-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut text = lines.join("\n");
    if !lines.is_empty() {
        text.push('\n');
    }
    std::fs::write(dir.join("sessions.jsonl"), text).unwrap();
    Copy(dir)
}

fn header(days: Value, generated: i64) -> String {
    json!({"api": 1, "days": days, "generatedAt": iso(generated), "sessions": 1}).to_string()
}

#[test]
fn iso_is_the_form_the_files_use() {
    assert_eq!(iso(1792485967), "2026-10-20T08:46:07Z");
}

#[test]
fn snapshot_missing_or_bad_is_a_tool_error() {
    let t = now();
    let missing = std::env::temp_dir().join(format!("mcp-snap-none-{}", std::process::id()));
    let empty = snapshot_dir("empty", &[]);
    let no_days = snapshot_dir(
        "nodays",
        &[json!({"api": 1, "generatedAt": iso(t), "sessions": 1}).to_string()],
    );
    let zero = snapshot_dir("zero", &[header(json!(0), t)]);
    let other_api = snapshot_dir(
        "api",
        &[json!({"api": 2, "days": 30, "generatedAt": iso(t)}).to_string()],
    );
    let not_object = snapshot_dir("array", &["[1]".to_string()]);
    for dir in [
        &missing,
        &empty.0,
        &no_days.0,
        &zero.0,
        &other_api.0,
        &not_object.0,
    ] {
        match Snapshot::open(dir, t) {
            Err(ToolError::File { file, why }) => {
                assert_eq!(file, "sessions.jsonl", "{dir:?}");
                assert!(!why.is_empty());
            }
            Err(other) => panic!("{dir:?}: {other:?}"),
            Ok(_) => panic!("{dir:?} opened"),
        }
    }
}

#[test]
fn snapshot_old_is_stale() {
    let t = now();
    let old = snapshot_dir("old", &[header(json!(30), t - 11 * 60)]);
    let s = Snapshot::open(&old.0, t).unwrap();
    assert!(s.stale);
    assert_eq!(s.days, 30);
    assert_eq!(s.generated_at, iso(t - 11 * 60));
    let fresh = snapshot_dir("fresh", &[header(json!(30), t - 5 * 60)]);
    assert!(!Snapshot::open(&fresh.0, t).unwrap().stale);
}

#[test]
fn snapshot_sessions_skip_bad_lines_and_the_old() {
    let t = now();
    let run = |id: i64, started: i64| json!({"id": id, "started": iso(started)}).to_string();
    let lines = [
        header(json!(30), t),
        run(1, t - 60),
        "not json".to_string(),
        "[1, 2]".to_string(),
        json!({"id": 2}).to_string(),
        run(3, t - 29 * 86400),
        run(4, t - 31 * 86400),
    ];
    let dir = snapshot_dir("lines", &lines);
    // A line that is not UTF-8, then a session that must still come out.
    let mut bytes = std::fs::read(dir.0.join("sessions.jsonl")).unwrap();
    bytes.extend_from_slice(b"{\"id\": 5, \"x\": \"\xff\xfe\"}\n");
    bytes.extend_from_slice(run(6, t - 120).as_bytes());
    bytes.push(b'\n');
    std::fs::write(dir.0.join("sessions.jsonl"), bytes).unwrap();
    let ids: Vec<i64> = Snapshot::open(&dir.0, t)
        .unwrap()
        .sessions()
        .map(|v| v["id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, [1, 3, 6]);
}
