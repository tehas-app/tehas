// The tools against live files with planted keys that must never come out.

use mcp::snapshot::Snapshot;
use mcp::tools::{list_scoped, Dirs, Output, Scope, ToolError};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use tehas_core::time::{iso, now};

fn live() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/live")
}

// A snapshot with no sessions, and with the days an answer names: the tools
// of the live files do not read it, and `factory_issue` answers the stage of
// issue 7 from those files alone. The tests of its runs write their own.
fn empty_snapshot() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sessions")
}

// A tool over the live files in `live`, for the tools that answer JSON.
fn call(live: &Path, name: &str, args: &Value) -> Result<Value, ToolError> {
    let dirs = Dirs {
        live: live.to_path_buf(),
        sessions: empty_snapshot(),
        logs: live.join("no-logs"),
    };
    match mcp::tools::call(&dirs, name, args)? {
        Output::Json(v) => Ok(v),
        other => panic!("{name} gave {other:?}"),
    }
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

// The paths an outputSchema names, in the same shape as the paths of an
// answer. The five tools name no map, so their schema needs none of the
// `*` normalisation the session tools need.
fn schema_paths(s: &Value, prefix: &str, out: &mut BTreeSet<String>) {
    if let Some(props) = s["properties"].as_object() {
        for (k, v) in props {
            let p = if prefix.is_empty() {
                k.clone()
            } else {
                format!("{prefix}.{k}")
            };
            out.insert(p.clone());
            schema_paths(v, &p, out);
        }
    }
    if s["type"] == "array" {
        schema_paths(&s["items"], &format!("{prefix}[]"), out);
    }
}

// The paths of a pick spec, in the same shape: an object names its keys and
// an array names those of its items.
fn spec_paths(s: &Value, prefix: &str, out: &mut BTreeSet<String>) {
    match s {
        Value::Object(m) => {
            for (k, sub) in m {
                let p = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                out.insert(p.clone());
                spec_paths(sub, &p, out);
            }
        }
        Value::Array(a) => {
            if let Some(sub) = a.first() {
                spec_paths(sub, &format!("{prefix}[]"), out);
            }
        }
        _ => {}
    }
}

fn spec_paths_of(name: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    spec_paths(&mcp::tools::spec_of(name).unwrap(), "", &mut out);
    // call_held adds it to every answer, so it is not in any tool's spec.
    out.insert("api".into());
    out
}

fn schema_of(name: &str) -> Value {
    list_scoped(Scope::Full)
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == name)
        .unwrap()["outputSchema"]
        .clone()
}

const NAMES: [&str; 6] = [
    "factory_status",
    "factory_issues",
    "factory_endings",
    "factory_console",
    "factory_stats",
    "factory_issue",
];

fn args(name: &str) -> Value {
    match name {
        "factory_console" | "factory_issue" => json!({"issue": 7}),
        _ => json!({}),
    }
}

#[test]
fn the_tools_are_listed_in_order_with_their_schemas() {
    let tools = list_scoped(Scope::Full);
    let names: Vec<&str> = tools
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    let mut want = NAMES.to_vec();
    want.extend([
        "factory_sessions",
        "factory_session",
        "factory_log",
        "factory_search",
        "factory_events",
        "factory_failures",
        "factory_outcomes",
        "factory_park",
        "factory_decisions",
        "factory_disk",
    ]);
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
fn stats_carry_refine() {
    let out = call(&live(), "factory_stats", &json!({})).unwrap();
    assert_eq!(out["refine"]["sessions"], 4);
    assert_eq!(out["refine"]["shippedShare"], 0.333);
    assert_eq!(out["refine"]["leadTime"]["median"], 43400);
    assert_eq!(out["refine"]["others"]["shipped"], 2);
    let text = out["refine"].to_string();
    for bad in ["buzzwords", "issues", "filedIssues"] {
        assert!(!text.contains(bad), "{text}");
    }
    assert!(out.get("filedIssues").is_none());
}

#[test]
fn status_says_the_queue_and_that_an_old_poll_is_stale() {
    let out = call(&live(), "factory_status", &json!({})).unwrap();
    assert_eq!(out["queue"]["inLine"], 2);
    assert_eq!(out["status"], "pass");
    assert_eq!(out["stale"], true);
    assert_eq!(out["cycles"][0]["issue"], 7);
    assert_eq!(out["platform"]["behind"], 0);
    assert_eq!(
        out["platform"]["commit"],
        "9b1b7dc11b73b93f7296b5f3131fcc6ef6b964e9"
    );
}

// "why is nothing starting" is the most useful thing to say to someone
// waiting, and dispatch.json is published already: the hold of the poll is
// copied from it, in its two keys.
#[test]
fn status_copies_the_hold_of_the_poll() {
    let out = call(&live(), "factory_status", &json!({})).unwrap();
    assert_eq!(out["hold"]["why"], "count");
    assert_eq!(
        out["hold"]["text"],
        "holding: 5 cycles are running (limit 5)"
    );
    // The rest of the poll's report is not served: only the two keys it names.
    // The words are those of the hold's own line, so `limit` is asked for with
    // its dot and the rest by whole word. The file's time is the poll's stamp,
    // and is left out as the file's own name that no answer gives.
    let text = out["hold"].to_string();
    for bad in [
        "\"room\"",
        "\"committed\"",
        "\"reserve\"",
        "\"free\"",
        "\"time\"",
        "2026-10-20",
        ".ts.net",
    ] {
        assert!(!text.contains(bad), "{bad}: {text}");
    }
    for bad in ["room", "committed", "reserve", "free"] {
        assert!(!text.contains(&format!("{bad}:")), "{bad}: {text}");
    }
}

// A poll that found nothing holding writes the empty word, and an answer with
// no hold at all reads better than one of an empty line.
#[test]
fn status_leaves_hold_out_when_nothing_holds() {
    for (name, report) in [
        ("nothing", json!({"why": "", "hold": "", "count": 0})),
        ("no-line", json!({"why": "count", "hold": ""})),
        ("no-why", json!({"hold": "holding: 5 cycles are running"})),
        ("not-json", json!("{")),
    ] {
        let c = Copy::new(&format!("hold-{name}"));
        std::fs::write(c.0.join("dispatch.json"), report.to_string()).unwrap();
        let out = call(&c.0, "factory_status", &json!({})).unwrap();
        assert!(out.get("hold").is_none(), "{name}: {}", out["hold"]);
        // The rest of the answer stands on its own.
        assert_eq!(out["queue"]["inLine"], 2, "{name}");
    }
    // No file at all: a host whose dispatcher never polled is answered.
    let c = Copy::new("hold-gone");
    std::fs::remove_file(c.0.join("dispatch.json")).unwrap();
    let out = call(&c.0, "factory_status", &json!({})).unwrap();
    assert!(out.get("hold").is_none());
    assert_eq!(out["status"], "pass");
    // A report of another shape is no report either: a link is not a regular
    // file, and a file over the bound of one poll's report is not read at all,
    // so no client can cost the answer more than a fixed number of bytes.
    let c = Copy::new("hold-link");
    let file = c.0.join("dispatch.json");
    std::fs::remove_file(&file).unwrap();
    std::fs::write(c.0.join("report.json"), r#"{""why":""}"#).unwrap();
    std::os::unix::fs::symlink(c.0.join("report.json"), &file).unwrap();
    let out = call(&c.0, "factory_status", &json!({})).unwrap();
    assert!(out.get("hold").is_none(), "a link is read");
    let c = Copy::new("hold-huge");
    let text = "x".repeat(64 * 1024 + 8);
    std::fs::write(
        c.0.join("dispatch.json"),
        format!(r#"{{"why":"count","hold":"{text}"}}"#),
    )
    .unwrap();
    let out = call(&c.0, "factory_status", &json!({})).unwrap();
    assert!(out.get("hold").is_none(), "a file over the bound is read");
    // The same file, one byte under the bound, is a report and is answered, so
    // the bound is the file's size and not the length of the line.
    let c = Copy::new("hold-under-bound");
    let line = "y".repeat(64 * 1024 - 40);
    std::fs::write(
        c.0.join("dispatch.json"),
        format!(r#"{{"why":"count","hold":"{line}"}}"#),
    )
    .unwrap();
    let out = call(&c.0, "factory_status", &json!({})).unwrap();
    assert_eq!(out["hold"]["why"], "count");
    assert_eq!(out["hold"]["text"].as_str().unwrap().chars().count(), 400);
}

// A hold is free text of the dispatcher's, masked like the tool's other free
// text and cut to 400 characters. The cut is of the masked text, so no secret
// is left in part.
//
// The mask of a process also hides, wherever they appear, the values of the
// environment whose names say they are secret; a fixture line is not free of
// them on every host, so the hold asserted here is one that no pattern of the
// mask rewrites: the github token, whose `ghp_` the mask leaves and whose rest
// it cuts, is what says the mask ran.
#[test]
fn a_hold_is_masked_then_cut() {
    let c = Copy::new("hold-masked");
    std::fs::write(
        c.0.join("dispatch.json"),
        json!({
            "why": "limit",
            "hold": format!(
                "the limit expired at {} with ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij and {}",
                "2020-01-01T00:00:00Z",
                "x".repeat(500)
            ),
        })
        .to_string(),
    )
    .unwrap();
    let out = call(&c.0, "factory_status", &json!({})).unwrap();
    let text = out["hold"]["text"].as_str().unwrap();
    // Masked: the token's secret half is gone, its prefix kept, as the mask
    // words it, and no line of the mask's own words is in the fixture.
    assert!(
        !text.contains("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij"),
        "{text}"
    );
    assert!(text.contains("ghp_***"), "{text}");
    assert!(
        text.starts_with("the limit expired at 2020-01-01T00:00:00Z "),
        "{text}"
    );
    // Then cut to 400 characters of the masked text.
    assert_eq!(text.chars().count(), 400, "{}", text.len());
    assert!(text.ends_with('x'), "{text}");
    assert_eq!(out["hold"]["why"], "limit");
}

// A host that runs a release names it instead of a commit: pick drops the
// keys of the checkout it does not have.
#[test]
fn status_names_the_release_of_a_release_host() {
    let dir = Copy::new("release-platform");
    let file = dir.0.join("health.json");
    let mut health = std::fs::read_to_string(&file).unwrap();
    health = health.replace(
        "\"platform\":{\"commit\":\"9b1b7dc11b73b93f7296b5f3131fcc6ef6b964e9\",\"upstream\":\"origin/main\",\"behind\":0,",
        "\"platform\":{\"release\":\"v0.9.1\",\"newest\":\"v0.9.2\",\"upstream\":\"releases\",\"behind\":0,",
    );
    std::fs::write(&file, health).unwrap();
    let out = call(&dir.0, "factory_status", &json!({})).unwrap();
    assert_eq!(out["platform"]["release"], "v0.9.1");
    assert_eq!(out["platform"]["newest"], "v0.9.2");
    assert_eq!(out["platform"]["upstream"], "releases");
    assert!(out["platform"].get("commit").is_none());
    assert_eq!(out["platform"]["fetchedAt"], "2026-10-20T08:36:59Z");
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

// The bug of #1200: the tool was in the dispatch and in the answer allowlist,
// but not in `CALLED` and not in the catalog, so a client was told nothing of
// it and a call was refused as unknown. Both halves are answered here: the
// name in the published list, and the published file copied by a call.
#[test]
fn factory_disk_is_listed_and_answers() {
    let catalog = list_scoped(Scope::Full);
    let listed: Vec<&str> = catalog
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(listed.contains(&"factory_disk"), "{listed:?}");
    // A dashboard key is not served it: of the full catalog it is given only
    // the four tools that copy from the live files.
    let short = list_scoped(Scope::Dashboard);
    let dashboard: Vec<&str> = short
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(!dashboard.contains(&"factory_disk"), "{dashboard:?}");

    let c = Copy::new("disk");
    std::fs::write(
        c.0.join("disk.json"),
        r#"{"api":1,"generatedAt":"2026-10-31T23:09:32Z","dockerRoot":"/var/lib/docker",
            "filesystems":[{"paths":["/home/boss/tehas"],"mount":"overlay","freeBytes":223,"totalBytes":307}],
            "full":{"places":[{"path":"/home/boss/tehas/logs","bytes":2048,"cut":"whole"}],
                    "stoppedContainers":2,"hostname":"a name no answer gives"}}"#,
    )
    .unwrap();
    let out = call(&c.0, "factory_disk", &json!({})).unwrap();
    assert_eq!(out["api"], 1);
    assert_eq!(out["generatedAt"], "2026-10-31T23:09:32Z");
    assert_eq!(out["dockerRoot"], "/var/lib/docker");
    assert_eq!(out["filesystems"][0]["freeBytes"], 223);
    assert_eq!(out["full"]["stoppedContainers"], 2);
    let text = out.to_string();
    for bad in ["hostname", "a name no answer gives"] {
        assert!(!text.contains(bad), "{bad} reached the answer: {text}");
    }
    // The schema a client is told and the allowlist the answer is copied with
    // name the same fields, as every tool of this file's five does.
    let mut want = BTreeSet::new();
    schema_paths(&schema_of("factory_disk"), "", &mut want);
    assert_eq!(want, spec_paths_of("factory_disk"), "schema and spec");
    for p in key_paths(&out) {
        assert!(p == "api" || want.contains(&p), "{p} is not in its schema");
    }

    // No file is the tool error that says so, not an empty answer.
    let c = Copy::new("disk-none");
    match call(&c.0, "factory_disk", &json!({})) {
        Err(ToolError::File { file, why }) => {
            assert_eq!(file, "disk.json");
            assert_eq!(why, "not published");
        }
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
                "platform.release",
                "platform.newest",
                "platform.behind",
                "platform.upstream",
                "platform.fetchedAt",
                "hold",
                "hold.why",
                "hold.text",
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
                "refine",
                "refine.sessions",
                "refine.n",
                "refine.started",
                "refine.shipped",
                "refine.shippedShare",
                "refine.leadTime",
                "refine.leadTime.n",
                "refine.leadTime.median",
                "refine.leadTime.p90",
                "refine.others",
                "refine.others.n",
                "refine.others.started",
                "refine.others.shipped",
                "refine.others.shippedShare",
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
        let mut got = key_paths(&call(&live(), name, &args(name)).unwrap());
        // The fixtures' health.json is a checkout host, so it has no release
        // and no newest: the answer of a release host adds those two, and the
        // commit it cannot show is in the answer above.
        if name == "factory_status" {
            let c = Copy::new("status-keys-release");
            let file = c.0.join("health.json");
            let health = std::fs::read_to_string(&file).unwrap().replace(
                "\"commit\":\"9b1b7dc11b73b93f7296b5f3131fcc6ef6b964e9\",\"upstream\":\"origin/main\"",
                "\"release\":\"v0.9.1\",\"newest\":\"v0.9.2\",\"upstream\":\"releases\"",
            );
            std::fs::write(&file, health).unwrap();
            got.extend(key_paths(&call(&c.0, name, &args(name)).unwrap()));
        }
        let want: BTreeSet<String> = keys.iter().map(|s| s.to_string()).collect();
        assert_eq!(got, want, "{name}");
    }
}

// The schema and what the tool copies are the same list of fields, checked
// against `spec_of`, the allowlist the answer is built from: a field that is
// in one and not the other fails here. The paths of a real answer must be a
// subset of the schema, so a field the tool answers without describing it
// fails too. A schema field the fixtures cannot show is allowed, because
// pick() drops a null and an absent key alike.
#[test]
fn schema_matches_output() {
    for name in NAMES {
        let cases = if name == "factory_issues" {
            vec![
                json!({}),
                json!({"stage": "queued"}),
                json!({"stage": "planning"}),
                json!({"stage": "running"}),
                json!({"stage": "awaiting"}),
                json!({"stage": "stopped"}),
                json!({"stage": "completed"}),
            ]
        } else {
            vec![args(name)]
        };
        let mut got = BTreeSet::new();
        for a in cases {
            got.extend(key_paths(&call(&live(), name, &a).unwrap()));
        }
        // A field the one fixture of health.json cannot show: a host that runs
        // a release names its version and the newest there is instead of a
        // commit, so those two are answered here from a health file of that
        // shape.
        if name == "factory_status" {
            let c = Copy::new("status-release");
            let file = c.0.join("health.json");
            let health = std::fs::read_to_string(&file).unwrap().replace(
                "\"commit\":\"9b1b7dc11b73b93f7296b5f3131fcc6ef6b964e9\",\"upstream\":\"origin/main\"",
                "\"release\":\"v0.9.1\",\"newest\":\"v0.9.2\",\"upstream\":\"releases\"",
            );
            std::fs::write(&file, health).unwrap();
            got.extend(key_paths(&call(&c.0, name, &args(name)).unwrap()));
        }
        let mut want = BTreeSet::new();
        schema_paths(&schema_of(name), "", &mut want);
        assert_eq!(want, spec_paths_of(name), "{name}: schema and spec");
        want.remove("api");
        for p in &got {
            assert!(
                p == "api" || want.contains(p),
                "{name}: {p} is not in its schema"
            );
        }
        for p in want.difference(&got) {
            // A closed issue has closedAt, so the fixtures cannot show it in
            // every stage; the issues of a plan with no turn are none; and
            // factory_issue answers the stage of issue 7 from the live files
            // with a snapshot of no runs, so nothing of an attempt comes out.
            // The runs of an issue are the subject of their own tests, which
            // write a snapshot and compare the same schema with that answer.
            let allowed = match name {
                "factory_issues" => p.ends_with(".closedAt") || p == "planning.issues",
                // The snapshot of these fixtures has no runs, so nothing of a
                // run comes out: not an attempt's fields, and not the two
                // times and the outcome drawn from them.
                "factory_issue" => {
                    p.starts_with("attempts[].")
                        || p == "running.step"
                        || p == "firstSeen"
                        || p == "lastSeen"
                        || p == "outcome"
                }
                _ => false,
            };
            assert!(allowed, "{name}: {p} is in its schema and never comes out");
        }
    }
}

// Every property of an input and of an output says what it is, and every
// field of an answer is a property, so nothing comes out undescribed.
#[test]
fn inputs_and_outputs_are_described() {
    for name in NAMES {
        let tool = list_scoped(Scope::Full)
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == name)
            .unwrap()
            .clone();
        for (k, v) in tool["inputSchema"]["properties"]
            .as_object()
            .unwrap_or(&serde_json::Map::new())
        {
            assert!(
                !v["description"].as_str().unwrap_or("").is_empty(),
                "{name} input {k}"
            );
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
        described(&tool["outputSchema"], name);
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
