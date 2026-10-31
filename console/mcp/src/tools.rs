//! The tools: what each copies from the live files.
//!
//! Every tool reads files under live/ by fixed names and copies only the keys
//! its spec below names. A key the dashboard's files gain later is not served
//! until it is named here. Null values are left out.

use crate::decisions;
use crate::events;
use crate::logs::{self, LogGuard};
use crate::outcomes;
use crate::outcomes_schema;
use crate::park;
use crate::sessions;
use regex::Regex;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use tehas_core::hold;
use tehas_core::mask::mask;
use tehas_core::time::days_from_civil;

/// The most of a console a call returns, and the default.
pub const CONSOLE_MAX: u64 = 65536;
/// The most of the dispatcher's line of a hold an answer gives.
const HOLD_MAX: usize = 400;
/// The most of the poll's report an answer reads: the two keys of the hold
/// and the numbers the poll decided by, of any host, in far fewer bytes. A
/// file over it is no report, as one that is not JSON is none.
const HOLD_BOUND: u64 = 64 * 1024;
/// The file of the poll's report, named by the dispatcher's own words: the
/// file `console/store` publishes and the health page reads.
const DISPATCH_FILE: &str = "dispatch.json";
/// A poll older than this is stale, as on the dashboard.
pub(crate) const STALE_SECS: i64 = 600;

/// The directories the server reads, each mounted read only. The container is
/// given one directory for the two private files of `~/tehas/mcp` - the
/// sessions snapshot and the outcomes counts - so the two reads name the same
/// path; `snapshot::FILE` and `outcomes::FILE` tell them apart.
pub struct Dirs {
    /// What the dashboard publishes.
    pub live: PathBuf,
    /// The sessions snapshot and the outcomes counts, never published.
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
        (
            "hold",
            obj(
                vec![
                    (
                        "why",
                        one_of(
                            &hold::Why::ALL.map(|w| w.as_str()),
                            "The one reason nothing starts, as the poll's report words it.",
                        ),
                    ),
                    (
                        "text",
                        prop(
                            "string",
                            "That reason in a line, masked and cut to 400 characters, such as \"holding: 5 cycles are running (limit 5)\".",
                        ),
                    ),
                ],
                "Why the dispatcher starts nothing. Left out when nothing holds.",
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

/// The tools a dashboard key is served: the four that copy from the live
/// files and read no other directory, so the key that faces strangers sees
/// only what the public dashboard shows. `factory_console` is under `live/`
/// too and is not one of them: a console is a model's output, and nothing
/// that wants a summary needs it. `factory_issue` reads the private sessions
/// snapshot as well, so it is not one either. The test beside `call_held`
/// proves that every name here is in `CALLED`, is answered by the catalog, and
/// is answered by an arm that reads nothing but `live`.
pub const DASHBOARD: [&str; 4] = [
    "factory_status",
    "factory_issues",
    "factory_endings",
    "factory_stats",
];

/// What a key's holder may call. The scope lives with the tool catalog, so
/// the one rule of what a dashboard key may call is read here, where the tools
/// and the words of `CALLED` are written; `keys::Authenticated` carries one
/// and is built only by the key store, so no caller names a scope the settings
/// do not give a key. The arms of `call_held` take their private directories
/// (`dirs.sessions`, `dirs.logs`) as arguments, which is why the four of
/// `DASHBOARD` are the four of the arms that take only `live` - a promise the
/// test beside `call_held` checks by calling each arm through the dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Every tool the server has.
    Full,
    /// The four of `DASHBOARD`, and no other.
    Dashboard,
}

/// The server's own word for a key's scope, for the access log.
impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Full => "full",
            Scope::Dashboard => "dashboard",
        }
    }
}

/// Whether a key of this scope is served the tool: every one of them for a
/// full key, and the four of `DASHBOARD` for a dashboard key. The single
/// reader of that list: `list_scoped` filters the published entries by it, so
/// what a key may call and what it is told exists cannot say different things.
pub(crate) fn serves(scope: Scope, name: &str) -> bool {
    match scope {
        Scope::Full => known(name),
        Scope::Dashboard => DASHBOARD.contains(&name),
    }
}

/// A key of the settings with the scope of the list it was read from: the
/// entry of the store `keys::Keys` holds. The pair is a type so the order of
/// the store is taken from the key's own field, and no scope is inferred from
/// where an entry sits.
#[derive(Clone, Debug)]
pub struct Entry {
    pub(crate) key: String,
    pub(crate) scope: Scope,
}

impl Entry {
    pub(crate) fn is_dashboard(&self) -> bool {
        self.scope == Scope::Dashboard
    }
}

/// The catalog as one array of entries, each a tool's name, its description and
/// its two schemas. `list_scoped` is the only function that answers it to a
/// caller, and the entries are built once for the process.
fn build_entries() -> Value {
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
        // factory_log, then factory_search beside it - the reading of one log
        // and the search over them - then factory_events, the host's own two
        // logs, and factory_failures after them, and factory_outcomes after
        // it: the counts of the last 30 days first, then the longer view
        // beside it, then factory_park, and factory_decisions last.
        a.insert(a.len() - 1, logs::tool());
        a.insert(a.len() - 1, logs::search_tool());
        a.insert(a.len() - 1, events::tool());
        a.push(outcomes_schema::tool());
        a.push(park::tool());
        a.push(decisions::tool());
    }
    tools
}

/// The names `known` admits and `call_held` dispatches. `build_entries` and
/// `spec_of` name the same tools in their own literals, and the test beside
/// `call_held` compares the three, so the access log can name no name the
/// server does not serve, and serves none it does not publish.
const CALLED: [&str; 15] = [
    "factory_status",
    "factory_issues",
    "factory_endings",
    "factory_console",
    "factory_stats",
    "factory_issue",
    "factory_sessions",
    "factory_session",
    "factory_log",
    "factory_search",
    "factory_events",
    "factory_failures",
    "factory_outcomes",
    "factory_park",
    "factory_decisions",
];

/// Whether the server has a tool of this name. The access log asks it before
/// naming one, so a name it prints is one this dispatch serves.
pub fn known(name: &str) -> bool {
    CALLED.contains(&name)
}

/// The tools of a `tools/list` for a key of this scope: every one of them for
/// a full key, as it has always been, and the four of `DASHBOARD` in the order
/// the catalog gives them for a dashboard key. The entries are the same
/// objects, so a tool is described the same way to either key.
///
/// This is the only function that answers the catalog, so a method added to
/// the dispatch cannot answer a tool list of its own and leave the scope out.
/// The entries are held once for the process, so a request for the list costs
/// a clone of the entries it needs and not the building of all fifteen.
pub fn list_scoped(scope: Scope) -> Value {
    Value::Array(
        entries()
            .iter()
            .filter(|t| t["name"].as_str().is_some_and(|n| serves(scope, n)))
            .cloned()
            .collect(),
    )
}

/// The published entries, built once for the process. The schemas of the tools
/// are constants of this file, so every `tools/list` of either scope answers
/// the same bytes, and a dashboard key that polls the list costs a clone of
/// four entries rather than the reconstruction of fifteen.
fn entries() -> &'static [Value] {
    static ENTRIES: LazyLock<Value> = LazyLock::new(build_entries);
    ENTRIES.as_array().map_or(&[], Vec::as_slice)
}

/// The structured content of one call, with "api": 1.
pub fn call(dirs: &Dirs, name: &str, args: &Value) -> Result<Output, ToolError> {
    call_held(dirs, name, args).map(|(out, _)| out)
}

/// As `call`, with the place of a log being sent, to keep until the answer
/// is written. Crate-private: a request reaches it only past
/// `Authenticated::may_call`.
pub(crate) fn call_held(
    dirs: &Dirs,
    name: &str,
    args: &Value,
) -> Result<(Output, Option<LogGuard>), ToolError> {
    // A name outside `CALLED` is refused here, before any arm is reached. The
    // test beside `call_held` holds a list of these arms and compares it with
    // `CALLED`, so the guard and the arms answer the same set.
    if !known(name) {
        return Err(ToolError::Unknown(name.to_string()));
    }
    if name == "factory_log" {
        return logs::call(dirs, args).map(|(out, place)| (out, Some(place)));
    }
    if name == "factory_search" {
        return logs::search_call(dirs, args).map(|(out, place)| (out, Some(place)));
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
        "factory_failures" => sessions::failures(dirs, args)?,
        "factory_events" => events::call(&dirs.logs, args)?,
        "factory_outcomes" => outcomes::call(dirs, args)?,
        "factory_park" => park::call(dirs, args)?,
        "factory_decisions" => decisions::call(dirs, args)?,
        // The guard above admits only the names of `CALLED` and every one of
        // them has an arm, which the test beside it proves name by name.
        _ => unreachable!("known admits only the names matched above"),
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
            // The two keys of `hold`, copied from the poll's own report by
            // `held`, which names the file's fields through `tehas_core::hold`.
            // A report whose word is not one of the thirteen is no hold, so
            // the empty word of a free poll is never served and needs no arm
            // of its own in the answer.
            "hold": {"why": true, "text": true},
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
        // The entries are copied with the spec of the grouping the file's own
        // header gives, which is the grouping of the answer this spec
        // describes: the entries of a version file name the two versions, and
        // those of a week file name its week. The schema of the tool's output
        // holds both keys, because a file of either grouping is possible on a
        // host, and the test of each grouping compares this spec with the
        // entries a call of that file serves.
        "factory_outcomes" => outcomes_schema::spec("version"),
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
        (Value::Bool(true), v) if v.is_object() || v.is_array() || v.is_null() => None,
        (Value::Bool(true), v) => Some(v.clone()),
        // A map of counts holds the file's own words, so its spec names no keys
        // of its own: it keeps the whole map only when every value in it is a
        // non-negative whole number, which is what a count is, and leaves the
        // key out rather than serve a value its schema does not promise. A
        // malformed file is therefore answered as a group that recorded
        // nothing of the kind, never as one that holds a word or a fraction.
        (Value::Object(s), Value::Object(m)) if s.is_empty() => m
            .iter()
            .map(|(k, v)| {
                let whole = v.as_u64().is_some();
                whole.then(|| (k.clone(), v.clone()))
            })
            .collect::<Option<Map<String, Value>>>()
            .map(Value::Object),
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
    // "why is nothing starting" is the most useful thing to say to someone
    // waiting, and dispatch.json is published already: the hold is copied from
    // it, and left out when the poll said nothing held.
    put(&mut out, "hold", held(live).map(Held::into_value));
    Ok(out)
}

/// The hold of the poll's report as the answer gives it: the one word of the
/// reason and the line that says it. The two keys of the answer, and no more,
/// so a value the schema does not describe cannot be built by this type.
struct Held {
    why: &'static str,
    text: String,
}

impl Held {
    /// The two keys of the answer, in the order the schema gives them.
    fn into_value(self) -> Value {
        let mut hold = Map::new();
        hold.insert("why".into(), json!(self.why));
        hold.insert("text".into(), json!(self.text));
        Value::Object(hold)
    }
}

/// What the dispatcher's last poll says holds, or None when it says nothing
/// holds, and None for a report that is not one: no file, no JSON, no reason
/// of the thirteen words, or no line of either field. The line of the file is
/// `hold`; the answer calls it `text`, like the free text of the tool's other
/// answers, and masks both fields before it cuts the line, so a cut never
/// leaves part of a secret the mask would have known whole.
fn held(live: &Path) -> Option<Held> {
    // A poll that is not on record leaves `hold` out like any other field
    // without a value, rather than failing the tool: the rest of the answer
    // stands on its own. A file over the bound, a link or one the reader
    // cannot open is answered the same way: the tool says nothing about a
    // report it did not read, and a bounded read costs the same for any file.
    let (report, _) = tehas_core::file::read_regular(&live.join(DISPATCH_FILE), HOLD_BOUND).ok()?;
    let report = serde_json::from_slice::<Value>(&report).ok()?;
    // Both fields of the pair are read before anything is built: a word that
    // is not one of the thirteen, or a line that is not text at all, is no
    // hold, never an answer of one key of the two.
    let why = report.get(hold::KEY_WHY)?.as_str()?.to_string();
    let why = hold::Why::parse(&why)?;
    let text = mask(report.get(hold::KEY_HOLD)?.as_str()?)
        .chars()
        .take(HOLD_MAX)
        .collect::<String>();
    if text.is_empty() {
        return None;
    }
    Some(Held {
        why: why.as_str(),
        text,
    })
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

/// An argument of another shape: the tool says which, and what it should have
/// been. The one way the private tools refuse an argument.
pub(crate) fn bad(why: &str) -> ToolError {
    ToolError::BadArgs(why.to_string())
}

/// The shape a version, a failure kind or a park reason may be: one word of
/// the published vocabularies, matched exactly. The tools that filter by one
/// ask for it, and their schemas name it as their pattern, so the alphabet is
/// stated once — the pattern of the readers of the two private files, named by
/// their schemas.
pub static WORD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9._+-]{1,64}$").unwrap());

/// One word argument of a call: None when it was not given, and an error that
/// names the key and the shape when it is not a word.
pub(crate) fn word(args: &Value, key: &str) -> Result<Option<String>, ToolError> {
    match &args[key] {
        Value::Null => Ok(None),
        Value::String(s) if WORD.is_match(s) => Ok(Some(s.clone())),
        _ => Err(bad(&format!("{key} is 1 to 64 of A-Za-z0-9._+-"))),
    }
}

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

    // The log prints the name of a called tool only when `known` says it is
    // one, so the list of names, the dispatch and the published list may not
    // drift apart. A name outside `CALLED` is refused by the guard of
    // `call_held`, so an assertion about its error cannot tell a served name
    // from an armless one: the arms themselves are compared with `CALLED` by
    // `arms_agree_with_the_called_list`.
    #[test]
    fn the_names_the_log_calls_are_the_names_the_server_lists() {
        let listed = list_scoped(Scope::Full);
        let listed: Vec<&str> = listed
            .as_array()
            .expect("a list of tools")
            .iter()
            .map(|t| t["name"].as_str().expect("a name"))
            .collect();
        // The published list and `CALLED` hold the same names, so each is
        // checked against the other and a name in one and not the other fails.
        let mut want = CALLED.to_vec();
        want.sort_unstable();
        let mut listed = listed;
        listed.sort_unstable();
        assert_eq!(listed, want);
        // `spec_of` is the third list of the names, and the one the dispatch's
        // guard does not touch: the five tools whose answer is copied from a
        // live file name themselves there, so a name that drifts out of
        // `CALLED` loses its allowlist here. The other nine build their answer
        // beside their own pick list, in their own module, and have none.
        for name in [
            "factory_status",
            "factory_issues",
            "factory_endings",
            "factory_console",
            "factory_stats",
        ] {
            assert!(known(name), "{name} is not called");
            assert!(spec_of(name).is_some(), "{name} has no answer allowlist");
        }
        for name in ["", "factory_delete", "factory_status_x", "Factory_Status"] {
            assert!(!known(name), "{name}");
            assert!(spec_of(name).is_none(), "{name} has an answer allowlist");
            assert!(
                matches!(
                    call_held(&fixture_dirs(), name, &json!({})),
                    Err(ToolError::Unknown(_))
                ),
                "{name} is dispatched by an arm"
            );
        }
    }

    // The four of `DASHBOARD` are served to a dashboard key, so the list may
    // not name a tool the server does not have, and may not name one that
    // reads a directory but `live`: the scope is the promise that such a key
    // sees only what the public dashboard shows. The second half of that is
    // mechanical: `call_held` answers a name through an arm that takes `live`
    // alone, and the private directories are arguments of the arms of the
    // other eleven, so a name of `DASHBOARD` cannot reach a reader of them.
    // The test asks the dispatch each of its questions rather than reading a
    // copy of the arms' shape. `factory_console` and `factory_issue` are the
    // two neighbours that do not qualify, and are named here so the list
    // cannot quietly grow to take them in.
    #[test]
    fn the_dashboard_list_is_four_live_only_tools() {
        let catalog = list_scoped(Scope::Full);
        let listed = names_of(&catalog);
        for name in DASHBOARD {
            // In `CALLED`, so the dispatch answers it, and listed, so the
            // server publishes it.
            assert!(known(name), "{name} is not called");
            assert!(listed.contains(&name), "{name} is not listed");
            assert!(spec_of(name).is_some(), "{name} has no answer allowlist");
            // Served, answered, and answered by the arms that are given only
            // `live`: an arm that opened the snapshot or the logs would be a
            // different answer, and the arms of the two private directories
            // are the eleven that a dashboard key is refused as unknown.
            assert!(
                serves(Scope::Dashboard, name),
                "{name} is not served to a dashboard key"
            );
            assert!(
                !matches!(
                    call_held(&fixture_dirs(), name, &json!({})),
                    Err(ToolError::Unknown(_))
                ),
                "{name} is dispatched by no arm"
            );
        }
        for name in ["factory_console", "factory_issue"] {
            assert!(!DASHBOARD.contains(&name), "{name} is not dashboard-only");
            // The two are refused to a dashboard key, though both are tools of
            // the server: `factory_console` is a model's text and
            // `factory_issue` opens the private snapshot beside the live files.
            assert!(!serves(Scope::Dashboard, name), "{name} is served");
            assert!(known(name), "{name} is not called");
        }
        // A dashboard key is served exactly these four, in the order the
        // catalog gives them, and a full key is served all fifteen.
        let short = list_scoped(Scope::Dashboard);
        let scoped = names_of(&short);
        let want: Vec<&str> = listed
            .iter()
            .filter(|n| serves(Scope::Dashboard, n))
            .copied()
            .collect();
        assert_eq!(scoped, want);
        assert_eq!(names_of(&catalog).len(), 15);
        // The entries of the shorter list are the entries of the longer one:
        // the same description and the same two schemas.
        for name in DASHBOARD {
            let of = |tools: &Value| {
                tools
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|t| t["name"] == json!(name))
                    .cloned()
            };
            assert_eq!(
                of(&list_scoped(Scope::Dashboard)),
                of(&list_scoped(Scope::Full)),
                "{name} is described differently to the two keys"
            );
        }
    }

    // The names of the entries of the catalog, in the order it gives them.
    fn names_of(tools: &Value) -> Vec<&str> {
        tools
            .as_array()
            .expect("a list of tools")
            .iter()
            .map(|t| t["name"].as_str().expect("a name"))
            .collect()
    }

    // Every name `known` admits is answered by an arm of the dispatch, and
    // no arm answers a name the allowlist does not admit. The arms and
    // `CALLED` are written out twice on purpose: reading them apart is the
    // point, and a name in one and not the other fails here.
    #[test]
    fn arms_agree_with_the_called_list() {
        // The names of the match arms of `call_held`, in its order: the guard
        // of `call_held` answers every name absent from this list, so a name
        // written here and not there, or there and not here, is the drift the
        // dispatch alone cannot show.
        const ARMS: [&str; 15] = [
            "factory_log",
            "factory_search",
            "factory_status",
            "factory_issues",
            "factory_endings",
            "factory_console",
            "factory_stats",
            "factory_issue",
            "factory_sessions",
            "factory_session",
            "factory_failures",
            "factory_events",
            "factory_outcomes",
            "factory_park",
            "factory_decisions",
        ];
        let mut arms = ARMS.to_vec();
        arms.sort_unstable();
        let mut called = CALLED.to_vec();
        called.sort_unstable();
        assert_eq!(arms, called);
        // And each arm really answers its name, read off the dispatch rather
        // than off this list: only a name the guard refuses answers `Unknown`.
        let dirs = fixture_dirs();
        for name in ARMS {
            assert!(
                !matches!(
                    call_held(&dirs, name, &json!({})),
                    Err(ToolError::Unknown(_))
                ),
                "{name} is dispatched by no arm"
            );
        }
    }

    // The hold of the poll's report, and only when the poll named a reason of
    // the thirteen words and gave a line of text. The pair is all or nothing:
    // a report whose `why` or whose `hold` is not a string of its own is no
    // hold, never an answer of one key of the two.
    #[test]
    fn a_hold_is_copied_and_a_free_poll_is_not() {
        for (name, report, want) in [
            (
                "held",
                json!({"why": "count", "hold": "holding: 5"}),
                Some(json!({"why": "count", "text": "holding: 5"})),
            ),
            ("free", json!({"why": "", "hold": ""}), None),
            ("no-line", json!({"hold": "holding: 5"}), None),
            ("empty-line", json!({"why": "count", "hold": ""}), None),
            // A file of another shape is no report, and no hold.
            ("no-why", json!({"why": "count"}), None),
            ("not-an-object", json!([]), None),
            // A word that is not one of the thirteen is no reason, and a
            // value that is not text is no line: either half of the pair, in
            // either position, refuses the whole hold.
            ("number-why", json!({"why": 7, "hold": "x"}), None),
            ("array-why", json!({"why": [], "hold": "x"}), None),
            (
                "word-not-known",
                json!({"why": "secrets", "hold": "x"}),
                None,
            ),
            ("number-line", json!({"why": "count", "hold": 7}), None),
            ("array-line", json!({"why": "count", "hold": []}), None),
            ("null-line", json!({"why": "count", "hold": null}), None),
        ] {
            let dir =
                std::env::temp_dir().join(format!("mcp-hold-{}-{}", std::process::id(), name));
            let _ = std::fs::remove_dir_all(&dir);
            let _ = std::fs::create_dir_all(&dir);
            std::fs::write(dir.join(DISPATCH_FILE), report.to_string()).unwrap();
            let got = self::held(&dir).map(Held::into_value);
            let _ = std::fs::remove_dir_all(&dir);
            assert_eq!(got, want, "{name}: {report}");
        }
        // Every word of the poll's own type is a reason the answer serves, so
        // the schema of `why`, which is built from the same type, names each
        // of them.
        for why in hold::Why::ALL {
            let dir = std::env::temp_dir().join(format!(
                "mcp-hold-{}-word-{}",
                std::process::id(),
                why.as_str()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            let _ = std::fs::create_dir_all(&dir);
            std::fs::write(
                dir.join(DISPATCH_FILE),
                json!({ hold::KEY_WHY: why.as_str(), hold::KEY_HOLD: "a line" }).to_string(),
            )
            .unwrap();
            let got = self::held(&dir).map(Held::into_value);
            let _ = std::fs::remove_dir_all(&dir);
            assert_eq!(
                got,
                Some(json!({"why": why.as_str(), "text": "a line"})),
                "{}",
                why.as_str()
            );
        }
    }

    // Dirs of the fixtures, for tests that ask the dispatch which side of it
    // refused a name: a name it serves may well fail on a file the fixtures do
    // not hold, and only the guard of the dispatch answers `Unknown`.
    fn fixture_dirs() -> Dirs {
        Dirs {
            live: Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/live"),
            sessions: Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sessions"),
            logs: Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/logs"),
        }
    }
}
