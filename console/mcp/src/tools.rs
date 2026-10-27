//! The tools: what each copies from the live files.
//!
//! Every tool reads files under live/ by fixed names and copies only the keys
//! its spec below names. A key the dashboard's files gain later is not served
//! until it is named here. Null values are left out.

use crate::logs::{self, LogGuard};
use crate::sessions;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use tehas_core::time::days_from_civil;

/// The most of a console a call returns, and the default.
pub const CONSOLE_MAX: u64 = 65536;
/// A poll older than this is stale, as on the dashboard.
pub(crate) const STALE_SECS: i64 = 600;

/// The directories the server reads, each mounted read only.
pub struct Dirs {
    /// What the dashboard publishes.
    pub live: PathBuf,
    /// The sessions snapshot, never published.
    pub sessions: PathBuf,
    pub logs: PathBuf,
}

/// What a call returns: JSON, or JSON that describes a file sent with it.
#[derive(Debug)]
pub enum Output {
    Json(Value),
    Blob {
        meta: Value,
        uri: String,
        mime: &'static str,
        /// The file's bytes in base64.
        blob: String,
    },
}

#[derive(Debug)]
pub enum ToolError {
    Unknown(String),
    BadArgs(String),
    File { file: String, why: String },
}

const STAGES: [&str; 6] = [
    "queued",
    "planning",
    "running",
    "awaiting",
    "stopped",
    "completed",
];

/// A status word a cycle can carry. The runner's own words, tehas's own for a
/// cycle it stopped, and `unknown` when nothing was reported.
const CYCLE_STATUSES: [&str; 13] = [
    "running",
    "merged",
    "handoff",
    "aborted",
    "interrupted",
    "refused",
    "killed",
    "stalled",
    "timeout",
    "lost",
    "closed",
    "completed",
    "unknown",
];
/// The overall state of a health check.
const CHECK_STATES: [&str; 3] = ["pass", "warn", "fail"];
/// The state of the limit, as `tehas limit` reports it.
const LIMIT_STATES: [&str; 4] = ["none", "ok", "expired", "unreadable"];

/// One described property of a known JSON type.
fn prop(ty: &str, desc: &str) -> Value {
    json!({"type": ty, "description": desc})
}

/// The API version every answer carries.
fn api() -> Value {
    prop("integer", "The API version, 1.")
}

/// A property whose value the dashboard's file gives as it is: a time, a count
/// or a flag, whose type the server does not check.
fn given(desc: &str) -> Value {
    json!({"description": desc})
}

fn one_of(values: &[&str], desc: &str) -> Value {
    json!({"type": "string", "enum": values, "description": desc})
}

fn arr(item: Value, desc: &str) -> Value {
    json!({"type": "array", "description": desc, "items": item})
}

fn obj(props: Vec<(&str, Value)>, desc: &str) -> Value {
    let props: Map<String, Value> = props.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    json!({"type": "object", "description": desc, "properties": props})
}

/// The top level of an answer. Every schema names "api" with the rest of its
/// fields, so the schema test can compare the two sets without a special case.
fn output(props: Vec<(&str, Value)>) -> Value {
    let props: Map<String, Value> = props.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    json!({"type": "object", "properties": props, "required": ["api"]})
}

/// One issue as the dashboard lists it.
fn issue_row() -> Value {
    obj(
        vec![
            ("number", prop("integer", "The issue number.")),
            ("title", prop("string", "The issue title.")),
            ("url", prop("string", "Its link on GitHub.")),
            ("updatedAt", given("When it last changed, UTC.")),
            (
                "closedAt",
                given("When it closed, UTC. Left out while it is open."),
            ),
        ],
        "One issue as the dashboard lists it.",
    )
}

fn status_schema() -> Value {
    output(vec![
        ("api", api()),
        (
            "status",
            one_of(&CHECK_STATES, "The factory: pass, warn or fail."),
        ),
        (
            "output",
            prop("string", "One line saying why the state is what it is."),
        ),
        ("time", given("When the dispatcher last reported, UTC.")),
        (
            "stale",
            prop(
                "boolean",
                "True when it has not reported for ten minutes. Left out without a readable time.",
            ),
        ),
        (
            "generatedAt",
            given("When the queue snapshot was written, UTC."),
        ),
        ("repo", given("The product repository as owner/name.")),
        ("paused", given("True while the factory is paused.")),
        ("planning", given("Issues being planned.")),
        (
            "limit",
            obj(
                vec![(
                    "state",
                    one_of(&LIMIT_STATES, "The limit: none, ok, expired or unreadable."),
                )],
                "The limit on what the factory starts.",
            ),
        ),
        (
            "queue",
            obj(
                vec![
                    ("inLine", given("Issues in line for a cycle.")),
                    ("building", given("Issues whose cycle owns them.")),
                    ("review", given("Issues waiting for a person.")),
                    ("stopped", given("Stopped issues.")),
                    ("time", given("When the snapshot was written, UTC.")),
                ],
                "The queue counts.",
            ),
        ),
        (
            "cycles",
            arr(
                obj(
                    vec![
                        ("issue", prop("integer", "The issue number.")),
                        ("running", prop("boolean", "True while the cycle runs.")),
                        (
                            "status",
                            one_of(&CYCLE_STATUSES, "What the cycle is doing."),
                        ),
                        ("step", prop("string", "The step it is in.")),
                        ("startedAt", given("When it started, UTC.")),
                        ("time", given("When it last reported, UTC.")),
                    ],
                    "One running cycle.",
                ),
                "The cycles running now.",
            ),
        ),
        (
            "demand",
            obj(
                vec![
                    ("waiting", given("Issues ready and waiting for a cycle.")),
                    ("oldest", given("When the oldest became ready, UTC.")),
                    ("time", given("When the dispatcher counted them, UTC.")),
                ],
                "The work waiting for a slot. Left out when the last poll wrote nothing.",
            ),
        ),
        (
            "platform",
            obj(
                vec![
                    (
                        "commit",
                        prop("string", "The platform commit the host runs."),
                    ),
                    (
                        "release",
                        given(
                            "The release the host runs, such as v0.9.0. On a release host instead of commit.",
                        ),
                    ),
                    (
                        "newest",
                        given("The newest published release, or null when it is unknown."),
                    ),
                    ("behind", given("Commits behind upstream.")),
                    (
                        "upstream",
                        prop(
                            "string",
                            "The upstream branch it is compared with, or releases for a release host.",
                        ),
                    ),
                    ("fetchedAt", given("When it was fetched, UTC.")),
                ],
                "The platform the host runs: its release, or its checkout against origin.",
            ),
        ),
    ])
}

fn issues_schema() -> Value {
    let mut props = vec![
        ("api", api()),
        ("generatedAt", given("When the snapshot was written, UTC.")),
    ];
    for stage in STAGES {
        let row = if stage == "planning" {
            obj(
                vec![
                    ("count", given("Issues being planned.")),
                    (
                        "issues",
                        arr(
                            prop("integer", "An issue number."),
                            "Their numbers, without titles.",
                        ),
                    ),
                ],
                "The issues being planned.",
            )
        } else {
            arr(issue_row(), &format!("The issues in stage {stage}."))
        };
        props.push((stage, row));
    }
    output(props)
}

/// One cycle that ended: the same keys in endings.json and in the failed list.
fn ending_row(time: &str, time_desc: &str) -> Value {
    obj(
        vec![
            ("issue", prop("integer", "The issue number.")),
            ("status", one_of(&CYCLE_STATUSES, "How it ended.")),
            ("step", prop("string", "The step it ended in.")),
            ("startedAt", given("When it started, UTC.")),
            (time, given(time_desc)),
        ],
        "One cycle that ended.",
    )
}

fn endings_schema() -> Value {
    output(vec![
        ("api", api()),
        (
            "endings",
            arr(
                ending_row("updatedAt", "When it was last seen, UTC."),
                "The ten cycles that ended last, oldest first.",
            ),
        ),
        (
            "failed",
            arr(
                ending_row("time", "When the entry was made, UTC."),
                "The failed ones among them, newest first.",
            ),
        ),
    ])
}

fn console_schema() -> Value {
    output(vec![
        ("api", api()),
        ("issue", prop("integer", "The issue number.")),
        ("step", prop("string", "The step it is in.")),
        (
            "status",
            one_of(&CYCLE_STATUSES, "What the cycle is doing."),
        ),
        (
            "statusText",
            prop("string", "The status line as the dashboard shows it."),
        ),
        (
            "startedAt",
            given("When it started, seconds since 1970, as the running file gives it."),
        ),
        (
            "updatedAt",
            given("When it last reported, seconds since 1970, as the running file gives it."),
        ),
        (
            "console",
            prop(
                "string",
                "The last bytes of its console. Left out when the running file does not name this file.",
            ),
        ),
    ])
}

/// One attempt on an issue: the session's own words, and a link built from the
/// repository the dashboard publishes. Its id is served so that one answer is
/// enough to reach `factory_session` or `factory_log` for the run.
fn attempt_row() -> Value {
    obj(
        vec![
            (
                "id",
                prop(
                    "string",
                    "The session id, like work-12-1792485967: the `id` to give factory_session or factory_log for this run.",
                ),
            ),
            ("kind", one_of(&sessions::KINDS, "work or plan.")),
            (
                "attempt",
                prop("integer", "Which attempt on the issue this run was."),
            ),
            (
                "how",
                one_of(
                    &sessions::HOWS,
                    "What started it. millwright and fixer name the same person.",
                ),
            ),
            ("started", given("When it started, UTC.")),
            ("ended", given("When it ended, UTC. Left out while it runs.")),
            ("seconds", prop("integer", "Seconds from start to end.")),
            (
                "outcome",
                one_of(&sessions::OUTCOMES, "How it ended. Left out while it runs."),
            ),
            ("failedStep", prop("string", "The step that failed.")),
            ("failureKind", prop("string", "The kind of failure.")),
            ("failureCause", prop("string", "The cause of the failure.")),
            ("parkReason", prop("string", "Why it was parked.")),
            (
                "prUrl",
                prop(
                    "string",
                    "Its pull request on GitHub. Left out without a pull request or a valid repo.",
                ),
            ),
        ],
        "One run of the issue within the snapshot's window.",
    )
}

fn issue_schema() -> Value {
    output(vec![
        ("api", api()),
        ("issue", prop("integer", "The issue number.")),
        (
            "issueUrl",
            prop(
                "string",
                "The issue on GitHub. Left out without a valid repo.",
            ),
        ),
        (
            "stage",
            one_of(
                &STAGES,
                "The stage the dashboard lists it under: the first of queued, planning, running, awaiting, stopped and completed that holds it. Left out when the dashboard lists it nowhere.",
            ),
        ),
        (
            "title",
            prop("string", "Its title, when the dashboard lists it with one."),
        ),
        (
            "running",
            obj(
                vec![
                    ("step", prop("string", "The step it is in.")),
                    ("status", one_of(&CYCLE_STATUSES, "What the cycle is doing.")),
                ],
                "The cycle running on it now. Left out when none is.",
            ),
        ),
        (
            "attempts",
            arr(
                attempt_row(),
                "Its runs of the last 30 days, oldest first. An empty list when it has none.",
            ),
        ),
        (
            "firstSeen",
            given("When its oldest returned run started, UTC."),
        ),
        ("lastSeen", given("When its newest returned run started, UTC.")),
        (
            "outcome",
            one_of(
                &sessions::OUTCOMES,
                "The outcome of the newest returned run. Left out when it has none, or while it runs.",
            ),
        ),
    ])
}

/// An n, a median and a p90, all in seconds.
fn quartiles(desc: &str) -> Value {
    obj(
        vec![
            ("n", given("How many it counts.")),
            ("median", given("The median, seconds.")),
            ("p90", given("The 90th percentile, seconds.")),
        ],
        desc,
    )
}

/// An n and a share of 0 to 1.
fn share(desc: &str) -> Value {
    obj(
        vec![
            ("n", given("How many it counts.")),
            ("share", given("The share of them, 0 to 1.")),
        ],
        desc,
    )
}

fn stats_schema() -> Value {
    output(vec![
        ("api", api()),
        (
            "from",
            given("The first day of the window, seconds since 1970."),
        ),
        (
            "to",
            given("The last day of the window, seconds since 1970."),
        ),
        ("report", given("The version of the report.")),
        (
            "runs",
            obj(
                vec![
                    ("n", given("Runs in the window.")),
                    ("merged", given("Runs whose pull request merged.")),
                    ("pr", given("Runs that left a pull request.")),
                    ("handoff", given("Runs handed to a person.")),
                    ("parked", given("Runs parked for a person.")),
                    ("failed", given("Runs that failed.")),
                    ("refused", given("Runs that refused to start.")),
                    ("stopped", given("Runs stopped here.")),
                ],
                "Runs by outcome.",
            ),
        ),
        (
            "shipped",
            obj(vec![("n", given("Runs that shipped."))], "Shipped runs."),
        ),
        ("cycleTime", quartiles("Time from start to end of a run.")),
        ("leadTime", quartiles("Time from ready to shipped.")),
        (
            "firstPassYield",
            share("Runs that passed review first time."),
        ),
        ("handoffRate", share("Runs handed to a person.")),
        (
            "availability",
            obj(
                vec![
                    ("n", given("How many it counts.")),
                    ("seconds", given("Seconds the factory was taking work.")),
                    ("share", given("The share of the window, 0 to 1.")),
                ],
                "Availability.",
            ),
        ),
        (
            "machineTime",
            obj(
                vec![
                    ("n", given("How many it counts.")),
                    ("seconds", given("Machine seconds in all.")),
                    ("perShipped", given("Machine seconds per shipped run.")),
                ],
                "Machine time.",
            ),
        ),
        (
            "energy",
            obj(
                vec![
                    ("n", given("How many it counts.")),
                    ("kwh", given("Kilowatt hours.")),
                    ("perShipped", given("Kilowatt hours per shipped run.")),
                ],
                "Energy.",
            ),
        ),
        (
            "tokens",
            obj(
                vec![
                    ("n", given("How many it counts.")),
                    ("input", given("Input tokens.")),
                    ("output", given("Output tokens.")),
                    ("cacheRead", given("Tokens read from the cache.")),
                    ("total", given("Tokens in all.")),
                    ("perShipped", given("Tokens per shipped run.")),
                ],
                "Token use.",
            ),
        ),
        (
            "planned",
            obj(
                vec![
                    ("n", given("Issues made ready.")),
                    ("inLine", given("Of those, the ones in line for a cycle.")),
                    ("planning", given("Of those, the ones still being planned.")),
                    (
                        "trend",
                        arr(
                            obj(
                                vec![
                                    ("day", prop("string", "The day, YYYY-MM-DD.")),
                                    ("n", given("Issues made ready that day.")),
                                ],
                                "One day.",
                            ),
                            "Issues made ready per day.",
                        ),
                    ),
                ],
                "The demand made ready.",
            ),
        ),
        (
            "planningTurns",
            obj(
                vec![("n", given("Model turns of planning."))],
                "Planning turns.",
            ),
        ),
        (
            "refine",
            obj(
                vec![
                    ("sessions", given("Refining sessions.")),
                    ("n", given("Issues they made ready.")),
                    ("started", given("Of those, the ones that started a cycle.")),
                    ("shipped", given("Of those, the ones that shipped.")),
                    ("shippedShare", given("The share that shipped, 0 to 1.")),
                    (
                        "leadTime",
                        quartiles("Ready to shipped, for the shipped ones."),
                    ),
                    (
                        "others",
                        obj(
                            vec![
                                ("n", given("Issues made ready the other way.")),
                                ("started", given("Of those, the ones that started a cycle.")),
                                ("shipped", given("Of those, the ones that shipped.")),
                                ("shippedShare", given("The share that shipped, 0 to 1.")),
                            ],
                            "The issues made ready without refining, to compare.",
                        ),
                    ),
                ],
                "The refine loop.",
            ),
        ),
        (
            "stations",
            obj(
                vec![
                    ("n", given("Steps run.")),
                    (
                        "steps",
                        arr(
                            obj(
                                vec![
                                    ("step", prop("string", "The step name.")),
                                    ("n", given("Times it ran.")),
                                    ("passed", given("Times it passed.")),
                                    ("share", given("The share that passed, 0 to 1.")),
                                ],
                                "One step.",
                            ),
                            "The steps by outcome.",
                        ),
                    ),
                ],
                "The steps of a cycle.",
            ),
        ),
        (
            "stopReasons",
            obj(
                vec![
                    ("n", given("Runs that stopped.")),
                    (
                        "top",
                        arr(
                            obj(
                                vec![
                                    ("reason", prop("string", "The reason.")),
                                    ("n", given("Runs with it.")),
                                ],
                                "One reason.",
                            ),
                            "The reasons, most first.",
                        ),
                    ),
                ],
                "Why runs stopped.",
            ),
        ),
        (
            "versions",
            arr(
                obj(
                    vec![
                        ("tehas", prop("string", "The tehas version.")),
                        ("rukas", prop("string", "The pi-rukas version or commit.")),
                        ("n", given("Runs on this pair.")),
                        ("medianSeconds", given("Median run seconds.")),
                        ("medianTokens", given("Median tokens.")),
                        (
                            "prShare",
                            given("The share that left a pull request, 0 to 1."),
                        ),
                    ],
                    "One version pair.",
                ),
                "Runs per tehas and pi-rukas pair.",
            ),
        ),
        (
            "daily",
            arr(
                obj(
                    vec![
                        ("day", prop("string", "The day, YYYY-MM-DD.")),
                        ("runs", given("Runs that day.")),
                        ("shipped", given("Runs shipped that day.")),
                        ("failed", given("Runs that failed that day.")),
                        ("minutes", given("Machine minutes that day.")),
                    ],
                    "One day.",
                ),
                "The daily rows.",
            ),
        ),
    ])
}

/// The tools, in a fixed order.
pub fn list() -> Value {
    let none = json!({"type": "object", "properties": {}, "additionalProperties": false});
    let mut tools = json!([
        {
            "name": "factory_status",
            "description": "Whether the factory is working, paused or failing, its queue counts, the running cycles and their steps, the demand waiting, and the platform it runs: the release, or the checkout's commit. Stale is true when the dispatcher has not reported for ten minutes.",
            "inputSchema": none,
            "outputSchema": status_schema(),
        },
        {
            "name": "factory_issues",
            "description": "The issues the dashboard lists, by stage: queued, planning, running, awaiting review, stopped and completed, each with number, title, link and times. Planning gives a count and issue numbers only.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "stage": {
                        "type": "string",
                        "enum": STAGES,
                        "description": "Only this stage. All of them without it.",
                    },
                },
                "additionalProperties": false,
            },
            "outputSchema": issues_schema(),
        },
        {
            "name": "factory_endings",
            "description": "The cycles that ended last, with their status and the step they ended in, and which of them failed.",
            "inputSchema": none,
            "outputSchema": endings_schema(),
        },
        {
            "name": "factory_console",
            "description": "One running cycle's step, status and the end of its console as the dashboard shows it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "issue": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "The issue of the running cycle.",
                    },
                    "bytes": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": CONSOLE_MAX,
                        "description": "How many bytes of its console, from the end. 64 KB without it.",
                    },
                },
                "required": ["issue"],
                "additionalProperties": false,
            },
            "outputSchema": console_schema(),
        },
        {
            "name": "factory_stats",
            "description": "The last ten days as the dashboard reports them: runs by outcome, cycle and lead time, first pass yield, handoff rate, availability, machine time, tokens, energy, stop reasons, versions, the refine figures and the daily rows.",
            "inputSchema": none,
            "outputSchema": stats_schema(),
        },
        {
            "name": "factory_issue",
            "description": "One issue's story in one call: the stage the dashboard lists it under and its title, the cycle running on it now with its step and status, and every run of it of the last 30 days, oldest first, with its id, kind, attempt, how it started, its times, outcome and failure. An issue the dashboard lists nowhere and that has not run in the window is a tool error.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "issue": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "The issue number.",
                    },
                },
                "required": ["issue"],
                "additionalProperties": false,
            },
            "outputSchema": issue_schema(),
        },
    ]);
    if let Value::Array(a) = &mut tools {
        a.extend(sessions::tools());
        // factory_log goes before factory_failures.
        a.insert(a.len() - 1, logs::tool());
    }
    tools
}

/// The structured content of one call, with "api": 1.
pub fn call(dirs: &Dirs, name: &str, args: &Value) -> Result<Output, ToolError> {
    call_held(dirs, name, args).map(|(out, _)| out)
}

/// As `call`, with the place of a log being sent, to keep until the answer
/// is written.
pub fn call_held(
    dirs: &Dirs,
    name: &str,
    args: &Value,
) -> Result<(Output, Option<LogGuard>), ToolError> {
    if name == "factory_log" {
        return logs::call(dirs, args).map(|(out, place)| (out, Some(place)));
    }
    let live = dirs.live.as_path();
    let mut out = match name {
        "factory_status" => status(live)?,
        "factory_issues" => issues(live, args)?,
        "factory_endings" => endings(live)?,
        "factory_console" => console(live, args)?,
        "factory_stats" => stats(live)?,
        "factory_issue" => sessions::issue(dirs, args)?,
        "factory_sessions" => sessions::list(dirs, args)?,
        "factory_session" => sessions::one(dirs, args)?,
        "factory_failures" => sessions::failures(dirs)?,
        _ => return Err(ToolError::Unknown(name.to_string())),
    };
    out.insert("api".into(), json!(1));
    Ok((Output::Json(Value::Object(out)), None))
}

fn read(live: &Path, file: &str) -> Result<Value, ToolError> {
    let err = |why: &str| ToolError::File {
        file: file.to_string(),
        why: why.to_string(),
    };
    let bytes = std::fs::read(live.join(file)).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            err("not published")
        } else {
            err("could not be read")
        }
    })?;
    serde_json::from_slice(&bytes).map_err(|_| err("is not complete JSON"))
}

/// The tool's own allowlist: the spec its answer is copied with, by the name
/// the answer gives it. `status`, `issues`, `endings`, `console` and `stats`
/// pick with these very values, so the schema test compares the schema with
/// the allowlist a call is actually built from. Three fields are written
/// rather than picked and so are absent here: `api`, which `call_held` adds,
/// `stale`, which `status` computes, and `console`, whose text comes from the
/// console file of the cycle.
pub fn spec_of(name: &str) -> Option<Value> {
    let issues = json!([{
        "number": true, "title": true, "url": true, "updatedAt": true, "closedAt": true
    }]);
    let cycles = json!([{
        "issue": true, "running": true, "status": true, "step": true,
        "startedAt": true, "time": true
    }]);
    let ended = |time: &str| {
        json!([{
            "issue": true, "status": true, "step": true, "startedAt": true, time: true
        }])
    };
    Some(match name {
        "factory_status" => json!({
            "status": true, "output": true, "time": true, "generatedAt": true,
            "repo": true, "paused": true, "planning": true, "stale": true,
            "limit": {"state": true},
            "queue": {"inLine": true, "building": true, "review": true,
                      "stopped": true, "time": true},
            "cycles": cycles,
            "demand": {"waiting": true, "oldest": true, "time": true},
            "platform": {"commit": true, "release": true, "newest": true, "behind": true,
                         "upstream": true, "fetchedAt": true},
        }),
        "factory_issues" => {
            let mut s = Map::new();
            s.insert("generatedAt".into(), json!(true));
            for stage in ["queued", "running", "awaiting", "stopped", "completed"] {
                s.insert(stage.into(), issues.clone());
            }
            s.insert("planning".into(), json!({"count": true, "issues": [true]}));
            Value::Object(s)
        }
        "factory_endings" => json!({
            "endings": ended("updatedAt"),
            "failed": ended("time"),
        }),
        "factory_console" => json!({
            "issue": true, "step": true, "status": true, "statusText": true,
            "startedAt": true, "updatedAt": true, "console": true,
        }),
        // The answer is joined from three live files and the snapshot, so its
        // allowlist lives beside the join. The schema test still compares this
        // spec with the outputSchema, which is the point of naming it here.
        "factory_issue" => sessions::issue_spec(),
        "factory_stats" => {
            let counts = [
                (
                    "runs",
                    vec![
                        "n", "merged", "pr", "handoff", "parked", "failed", "refused", "stopped",
                    ],
                ),
                ("shipped", vec!["n"]),
                ("cycleTime", vec!["n", "median", "p90"]),
                ("leadTime", vec!["n", "median", "p90"]),
                ("firstPassYield", vec!["n", "share"]),
                ("handoffRate", vec!["n", "share"]),
                ("availability", vec!["n", "seconds", "share"]),
                ("machineTime", vec!["n", "seconds", "perShipped"]),
                ("energy", vec!["n", "kwh", "perShipped"]),
                (
                    "tokens",
                    vec!["n", "input", "output", "cacheRead", "total", "perShipped"],
                ),
                ("planningTurns", vec!["n"]),
            ];
            let mut s = Map::new();
            for k in ["from", "to", "report"] {
                s.insert(k.into(), json!(true));
            }
            for (k, keys) in counts {
                s.insert(k.into(), spec_of_keys(&keys));
            }
            s.insert(
                "planned".into(),
                json!({"n": true, "inLine": true, "planning": true,
                       "trend": [{"day": true, "n": true}]}),
            );
            s.insert(
                "refine".into(),
                json!({
                    "sessions": true, "n": true, "started": true, "shipped": true,
                    "shippedShare": true,
                    "leadTime": {"n": true, "median": true, "p90": true},
                    "others": {"n": true, "started": true, "shipped": true,
                               "shippedShare": true},
                }),
            );
            s.insert(
                "stations".into(),
                json!({"n": true, "steps": [{"step": true, "n": true,
                                             "passed": true, "share": true}]}),
            );
            s.insert(
                "stopReasons".into(),
                json!({"n": true, "top": [{"reason": true, "n": true}]}),
            );
            s.insert(
                "versions".into(),
                json!([{"tehas": true, "rukas": true, "n": true,
                        "medianSeconds": true, "medianTokens": true, "prShare": true}]),
            );
            s.insert(
                "daily".into(),
                json!([{"day": true, "runs": true, "shipped": true,
                        "failed": true, "minutes": true}]),
            );
            Value::Object(s)
        }
        _ => return None,
    })
}

fn spec_of_keys(keys: &[&str]) -> Value {
    Value::Object(keys.iter().map(|k| (k.to_string(), json!(true))).collect())
}

/// The parts of `src` that `spec` names: `true` a scalar, an object its named
/// keys, `[spec]` each element of an array. None for null or a missing value.
pub(crate) fn pick(src: &Value, spec: &Value) -> Option<Value> {
    match (spec, src) {
        (_, Value::Null) => None,
        (Value::Bool(true), Value::Object(_) | Value::Array(_)) => None,
        (Value::Bool(true), v) => Some(v.clone()),
        (Value::Object(s), Value::Object(m)) => {
            let mut out = Map::new();
            for (k, sub) in s {
                if let Some(v) = m.get(k).and_then(|v| pick(v, sub)) {
                    out.insert(k.clone(), v);
                }
            }
            Some(Value::Object(out))
        }
        (Value::Array(s), Value::Array(a)) => {
            let sub = s.first()?;
            Some(Value::Array(
                a.iter().filter_map(|v| pick(v, sub)).collect(),
            ))
        }
        _ => None,
    }
}

fn put(out: &mut Map<String, Value>, key: &str, v: Option<Value>) {
    if let Some(v) = v {
        out.insert(key.to_string(), v);
    }
}

fn merge(out: &mut Map<String, Value>, v: Option<Value>) {
    if let Some(Value::Object(m)) = v {
        out.extend(m);
    }
}

fn status(live: &Path) -> Result<Map<String, Value>, ToolError> {
    let health = read(live, "health.json")?;
    let st = read(live, "status.json")?;
    let checks = &health["checks"];
    let spec = spec_of("factory_status").expect("a known tool");
    let mut out = Map::new();
    merge(
        &mut out,
        pick(&health, &spec_of_keys(&["status", "output", "time"])),
    );
    merge(
        &mut out,
        pick(
            &st,
            &spec_of_keys(&["generatedAt", "repo", "paused", "planning"]),
        ),
    );
    if let Some(t) = health["time"].as_str().and_then(epoch) {
        out.insert("stale".into(), json!(now() - t > STALE_SECS));
    }
    put(&mut out, "limit", pick(&checks["limit"], &spec["limit"]));
    put(&mut out, "queue", pick(&checks["queue"], &spec["queue"]));
    put(&mut out, "cycles", pick(&checks["cycles"], &spec["cycles"]));
    put(&mut out, "demand", pick(&checks["demand"], &spec["demand"]));
    put(
        &mut out,
        "platform",
        pick(&checks["platform"], &spec["platform"]),
    );
    Ok(out)
}

fn issues(live: &Path, args: &Value) -> Result<Map<String, Value>, ToolError> {
    let want = match &args["stage"] {
        Value::Null => None,
        Value::String(s) if STAGES.contains(&s.as_str()) => Some(s.as_str()),
        _ => {
            return Err(ToolError::BadArgs(format!(
                "stage is one of {}",
                STAGES.join(", ")
            )))
        }
    };
    let st = read(live, "status.json")?;
    let spec = spec_of("factory_issues").expect("a known tool");
    let mut out = Map::new();
    put(
        &mut out,
        "generatedAt",
        pick(&st["generatedAt"], &json!(true)),
    );
    let stages = spec
        .as_object()
        .expect("an object spec")
        .iter()
        .filter(|(k, _)| STAGES.contains(&k.as_str()))
        .map(|(k, v)| (k.as_str(), v.clone()))
        .collect::<Vec<_>>();
    for (stage, item) in &stages {
        let stage = *stage;
        if want.is_some_and(|w| w != stage) {
            continue;
        }
        if stage == "planning" {
            let health = read(live, "health.json")?;
            let numbers: Vec<Value> = health["checks"]["plans"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|p| pick(&p["issue"], &json!(true)))
                .collect();
            let mut p = Map::new();
            put(&mut p, "count", pick(&st["planning"], &json!(true)));
            p.insert("issues".into(), Value::Array(numbers));
            out.insert("planning".into(), Value::Object(p));
        } else {
            put(&mut out, stage, pick(&st[stage], item));
        }
    }
    Ok(out)
}

fn endings(live: &Path) -> Result<Map<String, Value>, ToolError> {
    let ends = read(live, "endings.json")?;
    let health = read(live, "health.json")?;
    let spec = spec_of("factory_endings").expect("a known tool");
    let mut out = Map::new();
    put(&mut out, "endings", pick(&ends, &spec["endings"]));
    put(
        &mut out,
        "failed",
        pick(&health["checks"]["failed"], &spec["failed"]),
    );
    Ok(out)
}

fn console(live: &Path, args: &Value) -> Result<Map<String, Value>, ToolError> {
    let issue = args["issue"]
        .as_u64()
        .filter(|n| *n > 0)
        .ok_or_else(|| ToolError::BadArgs("issue is a positive integer".into()))?;
    let bytes = match &args["bytes"] {
        Value::Null => CONSOLE_MAX,
        v => v
            .as_u64()
            .filter(|n| (1..=CONSOLE_MAX).contains(n))
            .ok_or_else(|| ToolError::BadArgs(format!("bytes is from 1 to {CONSOLE_MAX}")))?,
    };
    let running = read(live, "running.json")?;
    let cycle = running["cycles"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|c| c["issue"].as_u64() == Some(issue))
        .ok_or_else(|| ToolError::BadArgs(format!("no running cycle for issue {issue}")))?;
    let mut out = Map::new();
    merge(
        &mut out,
        // "console" is not in the spec: it comes from the file below.
        pick(
            cycle,
            &spec_of_keys(&[
                "issue",
                "step",
                "status",
                "statusText",
                "startedAt",
                "updatedAt",
            ]),
        ),
    );
    // The only file name this opens, and only when running.json gives it.
    let name = format!("console-{issue}.txt");
    if cycle["console"].as_str() == Some(name.as_str()) {
        if let Some(text) = read_tail(&live.join(&name), bytes) {
            out.insert("console".into(), json!(text));
        }
    }
    Ok(out)
}

/// The last `n` bytes of a file, cut forward to the start of a character.
/// Only those bytes are read: a console only grows.
fn read_tail(path: &Path, n: u64) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(n))).ok()?;
    let mut text = Vec::with_capacity(n.min(len) as usize);
    f.take(n).read_to_end(&mut text).ok()?;
    let start = text
        .iter()
        .position(|b| b & 0b1100_0000 != 0b1000_0000)
        .unwrap_or(text.len());
    Some(String::from_utf8_lossy(&text[start..]).into_owned())
}

fn stats(live: &Path) -> Result<Map<String, Value>, ToolError> {
    let a = read(live, "analytics.json")?;
    let spec = spec_of("factory_stats").expect("a known tool");
    match pick(&a, &spec) {
        Some(Value::Object(m)) => Ok(m),
        _ => Err(ToolError::File {
            file: "analytics.json".into(),
            why: "is not an object".into(),
        }),
    }
}

pub(crate) use tehas_core::time::now;

/// Seconds since 1970 for "YYYY-MM-DDTHH:MM:SSZ", the form the live files use.
pub(crate) fn epoch(t: &str) -> Option<i64> {
    let b = t.as_bytes();
    if b.len() != 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[19] != b'Z' {
        return None;
    }
    let n = |r: std::ops::Range<usize>| t.get(r)?.parse::<i64>().ok();
    let (y, m, d) = (n(0..4)?, n(5..7)?, n(8..10)?);
    let (hh, mm, ss) = (n(11..13)?, n(14..16)?, n(17..19)?);
    Some(days_from_civil(y, m, d) * 86400 + hh * 3600 + mm * 60 + ss)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_of_a_live_file_time() {
        assert_eq!(epoch("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(epoch("2026-10-20T08:46:07Z"), Some(1792485967));
        assert_eq!(epoch("2026-10-20 08:46:07"), None);
    }
}
