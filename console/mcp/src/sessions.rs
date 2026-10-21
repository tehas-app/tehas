//! The session tools: `factory_sessions`, `factory_session` and
//! `factory_failures`, over the sessions snapshot. They read it one line at a
//! time and copy only the fields named here. A log's file name is never
//! returned.

use crate::snapshot::Snapshot;
use crate::tools::{self, Dirs, ToolError};
use regex::Regex;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::sync::LazyLock;

const OUTCOMES: [&str; 7] = [
    "pr", "merged", "handoff", "parked", "failed", "refused", "stopped",
];
const KINDS: [&str; 2] = ["work", "plan"];
const HOWS: [&str; 6] = [
    "label",
    "retry-tick",
    "outage-retry",
    "fixer",
    "answer",
    "refine",
];
const DEFAULT_LIMIT: u64 = 50;
const MAX_LIMIT: u64 = 200;
const TOP_CAUSES: usize = 5;
const CAUSE_ISSUES: usize = 10;

static ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(work|plan)-[1-9][0-9]{0,8}-[0-9]{9,11}$").unwrap());
static WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9._+-]{1,64}$").unwrap());
static REPO: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$").unwrap());

/// The snapshot's scalar fields: key, JSON type, what it is.
const FIELDS: [(&str, &str, &str); 38] = [
    ("id", "string", "The session id, like work-12-1792485967."),
    ("kind", "string", "work or plan."),
    ("issue", "integer", "The issue number."),
    (
        "attempt",
        "integer",
        "Which attempt on the issue this run was.",
    ),
    (
        "how",
        "string",
        "What started it: label, retry-tick, outage-retry, fixer, answer or refine.",
    ),
    ("started", "string", "When it started, UTC."),
    (
        "ended",
        "string",
        "When it ended, UTC. Left out while it runs.",
    ),
    ("seconds", "integer", "Seconds from start to end."),
    (
        "queueWait",
        "integer",
        "Seconds it waited before it started.",
    ),
    (
        "outcome",
        "string",
        "pr, merged, handoff, parked, failed, refused or stopped.",
    ),
    ("exitCode", "integer", "The exit status of the cycle."),
    ("status", "string", "The status word the cycle ended with."),
    ("lastStep", "string", "The last step it reached."),
    ("failedStep", "string", "The step that failed."),
    ("cap", "string", "The cap that stopped it."),
    ("parkReason", "string", "Why it was parked."),
    (
        "reviewHeld",
        "boolean",
        "Whether the review held the pull request.",
    ),
    ("failureKind", "string", "The kind of failure."),
    ("failureCause", "string", "The cause of the failure."),
    (
        "providerOutage",
        "boolean",
        "Whether a model provider outage caused it.",
    ),
    ("endpoint", "string", "The model endpoint it used."),
    ("pr", "integer", "The pull request number."),
    ("tehasVersion", "string", "The tehas version it ran."),
    ("tehasCommit", "string", "The tehas commit it ran."),
    ("rukasVersion", "string", "The pi-rukas version it ran."),
    ("rukasCommit", "string", "The pi-rukas commit it ran."),
    ("image", "string", "The sandbox image it ran in."),
    ("resultSchema", "integer", "The schema of the result file."),
    ("input", "integer", "Input tokens."),
    ("output", "integer", "Output tokens."),
    ("cacheRead", "integer", "Tokens read from the cache."),
    ("cacheWrite", "integer", "Tokens written to the cache."),
    ("turns", "integer", "Model turns."),
    ("dispatches", "integer", "Model dispatches."),
    ("cost", "number", "Model cost as the runner reported it."),
    ("runnerMs", "integer", "Milliseconds the runner ran."),
    ("oomKills", "integer", "Processes killed for memory."),
    (
        "forksRefused",
        "integer",
        "Forks refused at the process limit.",
    ),
];
const CONCURRENT: (&str, &str, &str) = (
    "concurrent",
    "integer",
    "Cycles running at the same time when it started.",
);
const SUMMARY: [&str; 15] = [
    "id",
    "kind",
    "issue",
    "attempt",
    "how",
    "started",
    "ended",
    "seconds",
    "outcome",
    "failedStep",
    "failureKind",
    "failureCause",
    "parkReason",
    "tehasVersion",
    "rukasVersion",
];
const STEP: [(&str, &str, &str); 5] = [
    ("step", "string", "The step name."),
    ("round", "integer", "Which round of the step."),
    ("started", "string", "When it started, UTC."),
    ("ended", "string", "When it ended, UTC."),
    ("seconds", "integer", "Seconds it took."),
];
const ROLE: [(&str, &str, &str); 11] = [
    ("role", "string", "The role, such as developer or reviewer."),
    ("model", "string", "The model."),
    ("provider", "string", "The provider."),
    ("dispatches", "integer", "Dispatches to the model."),
    ("turns", "integer", "Model turns."),
    ("input", "integer", "Input tokens."),
    ("output", "integer", "Output tokens."),
    ("cacheRead", "integer", "Tokens read from the cache."),
    ("cacheWrite", "integer", "Tokens written to the cache."),
    ("cost", "number", "Model cost as the runner reported it."),
    ("ms", "integer", "Milliseconds of model time."),
];

fn field(f: (&str, &str, &str)) -> (String, Value) {
    (f.0.to_string(), json!({"type": f.1, "description": f.2}))
}

fn object_of(fields: &[(&str, &str, &str)]) -> Value {
    let props: Map<String, Value> = fields.iter().map(|f| field(*f)).collect();
    json!({"type": "object", "properties": props})
}

fn text(desc: &str) -> Value {
    json!({"type": "string", "description": desc})
}

fn count(desc: &str) -> Value {
    json!({"type": "integer", "description": desc})
}

fn array_of(item: Value, desc: &str) -> Value {
    json!({"type": "array", "description": desc, "items": item})
}

fn schema(props: Vec<(&str, Value)>) -> Value {
    let props: Map<String, Value> = props.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    json!({"type": "object", "properties": props, "required": ["api"]})
}

fn links_props() -> Vec<(&'static str, Value)> {
    vec![
        (
            "issueUrl",
            text("The issue on GitHub. Left out without a valid repo."),
        ),
        (
            "prUrl",
            text("The pull request on GitHub. Left out without a pull request or a valid repo."),
        ),
    ]
}

fn summary_schema() -> Value {
    let mut item = object_of(
        &FIELDS
            .iter()
            .filter(|f| SUMMARY.contains(&f.0))
            .copied()
            .collect::<Vec<_>>(),
    );
    for (k, v) in links_props() {
        item["properties"][k] = v;
    }
    item
}

fn session_schema() -> Value {
    let mut all = FIELDS.to_vec();
    all.push(CONCURRENT);
    let mut item = object_of(&all);
    let p = &mut item["properties"];
    p["steps"] = array_of(object_of(&STEP), "The steps it went through, in order.");
    p["roles"] = array_of(object_of(&ROLE), "Model use per role.");
    p["logs"] = array_of(
        json!({"type": "object", "properties": {
            "file": text("Which log: a word to give factory_log."),
            "bytes": count("The log's size in bytes."),
        }}),
        "The logs it left.",
    );
    p["filed"] = array_of(
        json!({"type": "object", "properties": {
            "issue": count("The issue number a planning session filed."),
            "url": text("Its link."),
        }}),
        "The issues a planning session filed.",
    );
    for (k, v) in links_props() {
        p[k] = v;
    }
    item
}

fn outcome_counts(desc: &str) -> Value {
    json!({"type": "object", "description": desc,
           "additionalProperties": {"type": "integer"}})
}

fn failures_schema() -> Value {
    let cause = json!({"type": "object", "properties": {
        "cause": text("The failure cause."),
        "kind": text("Its failure kind. Left out when none was recorded."),
        "n": count("Work sessions with this cause."),
        "issues": array_of(text("An issue link."), "Up to ten issues, newest first. Empty without a valid repo."),
    }});
    let pair = json!({"type": "object", "properties": {
        "tehasVersion": text("The tehas version. Left out when unknown."),
        "rukasVersion": text("The pi-rukas version, or its commit. Left out when unknown."),
        "runs": count("Work sessions on this pair."),
        "outcomes": outcome_counts("Sessions per outcome."),
        "failureKinds": outcome_counts("Sessions per failure kind."),
        "causes": array_of(cause, "The five most common failure causes."),
    }});
    schema(vec![
        ("api", count("The API version, 1.")),
        ("days", count("The days the snapshot covers.")),
        (
            "stale",
            json!({"type": "boolean", "description": "True when the snapshot is more than ten minutes old."}),
        ),
        (
            "versions",
            array_of(pair, "One entry per version pair, the most runs first."),
        ),
    ])
}

fn arg(name: &str, ty: &str, desc: &str) -> (String, Value) {
    (name.to_string(), json!({"type": ty, "description": desc}))
}

/// The three tools, for `tools::list`.
pub fn tools() -> Vec<Value> {
    let mut props: Map<String, Value> = Map::new();
    let mut with = |(k, mut v): (String, Value), extra: Value| {
        if let Value::Object(e) = extra {
            v.as_object_mut().unwrap().extend(e);
        }
        props.insert(k, v);
    };
    with(
        arg("outcome", "string", "Only sessions with this outcome."),
        json!({"enum": OUTCOMES}),
    );
    with(
        arg(
            "failureKind",
            "string",
            "Only sessions with this failure kind, matched exactly.",
        ),
        json!({"pattern": WORD.as_str()}),
    );
    with(
        arg(
            "tehasVersion",
            "string",
            "Only sessions that ran this tehas version, matched exactly.",
        ),
        json!({"pattern": WORD.as_str()}),
    );
    with(
        arg(
            "rukasVersion",
            "string",
            "Only sessions that ran this pi-rukas version or commit, matched exactly.",
        ),
        json!({"pattern": WORD.as_str()}),
    );
    with(
        arg("kind", "string", "Only work or plan sessions."),
        json!({"enum": KINDS}),
    );
    with(
        arg(
            "how",
            "string",
            "Only sessions started this way. refine lists refining sessions alone.",
        ),
        json!({"enum": HOWS}),
    );
    with(
        arg("issue", "integer", "Only sessions on this issue."),
        json!({"minimum": 1}),
    );
    with(
        arg(
            "limit",
            "integer",
            "How many sessions to return, from 1 to 200. 50 without it.",
        ),
        json!({"minimum": 1, "maximum": MAX_LIMIT}),
    );
    with(
        arg(
            "before",
            "string",
            "A session id: return the sessions after it. Pass the next of the last answer.",
        ),
        json!({"pattern": ID.as_str()}),
    );
    let list_out = schema(vec![
        ("api", count("The API version, 1.")),
        ("days", count("The days the snapshot covers.")),
        (
            "stale",
            json!({"type": "boolean", "description": "True when the snapshot is more than ten minutes old."}),
        ),
        (
            "sessions",
            array_of(summary_schema(), "The sessions, newest first."),
        ),
        (
            "next",
            text("The id of the last session when more remain; pass it as before."),
        ),
    ]);
    let one_out = schema(vec![
        ("api", count("The API version, 1.")),
        (
            "stale",
            json!({"type": "boolean", "description": "True when the snapshot is more than ten minutes old."}),
        ),
        ("session", session_schema()),
    ]);
    vec![
        json!({
            "name": "factory_sessions",
            "description": "The runs of the last 30 days, newest first, with their outcome, failure and versions. Filter by outcome, failure kind, version, kind, how it started or issue, and page with limit and before.",
            "inputSchema": {"type": "object", "properties": props, "additionalProperties": false},
            "outputSchema": list_out,
        }),
        json!({
            "name": "factory_session",
            "description": "One run of the last 30 days: every figure of the session, its steps, its roles, its logs by word and size, the issues a planning session filed, and links to the issue and pull request.",
            "inputSchema": {"type": "object", "properties": {
                "id": {"type": "string", "pattern": ID.as_str(), "description": "The session id from factory_sessions."},
            }, "required": ["id"], "additionalProperties": false},
            "outputSchema": one_out,
        }),
        json!({
            "name": "factory_failures",
            "description": "Work runs of the last 30 days per tehas and pi-rukas version pair: runs, outcomes, failure kinds and the five most common causes with links to the issues. Most runs first.",
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false},
            "outputSchema": failures_schema(),
        }),
    ]
}

/// Links to an issue and its pull request. A repo that is not `owner/name`
/// gives none.
pub fn links(repo: Option<&str>, issue: i64, pr: Option<i64>) -> (Option<String>, Option<String>) {
    let Some(repo) = repo.filter(|r| REPO.is_match(r) && !r.contains("..")) else {
        return (None, None);
    };
    (
        Some(format!("https://github.com/{repo}/issues/{issue}")),
        pr.map(|n| format!("https://github.com/{repo}/pull/{n}")),
    )
}

fn bad(why: &str) -> ToolError {
    ToolError::BadArgs(why.to_string())
}

/// The session `id` as the snapshot has it, inside the window. The id is
/// checked before anything is opened.
pub fn find(snap: &Snapshot, id: &str) -> Result<Value, ToolError> {
    if !ID.is_match(id) {
        return Err(bad("id is like work-12-1792485967"));
    }
    snap.sessions()
        .find(|s| s["id"].as_str() == Some(id))
        .ok_or_else(|| bad(&format!("no session {id} in the last {} days", snap.days)))
}

fn repo(dirs: &Dirs) -> Option<String> {
    let bytes = std::fs::read(dirs.live.join("status.json")).ok()?;
    let v: Value = serde_json::from_slice(&bytes).ok()?;
    v["repo"].as_str().map(str::to_string)
}

fn session_links(repo: &Option<String>, s: &Value) -> (Option<String>, Option<String>) {
    match s["issue"].as_i64() {
        Some(n) => links(repo.as_deref(), n, s["pr"].as_i64()),
        None => (None, None),
    }
}

fn word(args: &Value, key: &str) -> Result<Option<String>, ToolError> {
    match &args[key] {
        Value::Null => Ok(None),
        Value::String(s) if WORD.is_match(s) => Ok(Some(s.clone())),
        _ => Err(bad(&format!("{key} is 1 to 64 of A-Za-z0-9._+-"))),
    }
}

fn one_of(args: &Value, key: &str, allowed: &[&str]) -> Result<Option<String>, ToolError> {
    match &args[key] {
        Value::Null => Ok(None),
        Value::String(s) if allowed.contains(&s.as_str()) => Ok(Some(s.clone())),
        _ => Err(bad(&format!("{key} is one of {}", allowed.join(", ")))),
    }
}

fn spec(keys: &[&str]) -> Value {
    Value::Object(keys.iter().map(|k| (k.to_string(), json!(true))).collect())
}

fn open(dirs: &Dirs) -> Result<Snapshot, ToolError> {
    Snapshot::open(&dirs.sessions, tools::now())
}

pub(crate) fn list(dirs: &Dirs, args: &Value) -> Result<Map<String, Value>, ToolError> {
    let outcome = one_of(args, "outcome", &OUTCOMES)?;
    let kind = one_of(args, "kind", &KINDS)?;
    let how = one_of(args, "how", &HOWS)?;
    let failure_kind = word(args, "failureKind")?;
    let tehas = word(args, "tehasVersion")?;
    let rukas = word(args, "rukasVersion")?;
    let issue = match &args["issue"] {
        Value::Null => None,
        v => Some(
            v.as_i64()
                .filter(|n| *n >= 1)
                .ok_or_else(|| bad("issue is a positive integer"))?,
        ),
    };
    let limit = match &args["limit"] {
        Value::Null => DEFAULT_LIMIT,
        v => v
            .as_u64()
            .filter(|n| (1..=MAX_LIMIT).contains(n))
            .ok_or_else(|| bad(&format!("limit is from 1 to {MAX_LIMIT}")))?,
    } as usize;
    let before = match &args["before"] {
        Value::Null => None,
        Value::String(s) if ID.is_match(s) => Some(s.as_str()),
        _ => return Err(bad("before is a session id")),
    };

    let snap = open(dirs)?;
    let repo = repo(dirs);
    let eq = |s: &Value, key: &str, want: &Option<String>| {
        want.as_deref().is_none_or(|w| s[key].as_str() == Some(w))
    };
    let mut passed = before.is_none();
    let mut found: Vec<Value> = Vec::new();
    for s in snap.sessions() {
        if !passed {
            passed = s["id"].as_str() == before;
            continue;
        }
        let keep = eq(&s, "outcome", &outcome)
            && eq(&s, "kind", &kind)
            && eq(&s, "how", &how)
            && eq(&s, "failureKind", &failure_kind)
            && eq(&s, "tehasVersion", &tehas)
            && rukas.as_deref().is_none_or(|w| {
                s["rukasVersion"].as_str() == Some(w) || s["rukasCommit"].as_str() == Some(w)
            })
            && issue.is_none_or(|n| s["issue"].as_i64() == Some(n));
        if keep {
            found.push(s);
            if found.len() > limit {
                break;
            }
        }
    }
    if !passed {
        return Err(bad("before is not a session of the last 30 days"));
    }
    let more = found.len() > limit;
    found.truncate(limit);
    let next = more
        .then(|| found.last().map(|s| s["id"].clone()))
        .flatten();

    let keys = spec(&SUMMARY);
    let sessions: Vec<Value> = found
        .iter()
        .map(|s| {
            let mut o = pick_object(s, &keys);
            let (i, p) = session_links(&repo, s);
            insert_opt(&mut o, "issueUrl", i);
            insert_opt(&mut o, "prUrl", p);
            Value::Object(o)
        })
        .collect();
    let mut out = Map::new();
    out.insert("days".into(), json!(snap.days));
    out.insert("stale".into(), json!(snap.stale));
    out.insert("sessions".into(), Value::Array(sessions));
    if let Some(n) = next {
        out.insert("next".into(), n);
    }
    Ok(out)
}

fn insert_opt(o: &mut Map<String, Value>, key: &str, v: Option<String>) {
    if let Some(v) = v {
        o.insert(key.into(), json!(v));
    }
}

fn pick_object(src: &Value, spec: &Value) -> Map<String, Value> {
    match tools::pick(src, spec) {
        Some(Value::Object(m)) => m,
        _ => Map::new(),
    }
}

pub(crate) fn one(dirs: &Dirs, args: &Value) -> Result<Map<String, Value>, ToolError> {
    let id = args["id"]
        .as_str()
        .ok_or_else(|| bad("id is a string like work-12-1792485967"))?;
    if !ID.is_match(id) {
        return Err(bad("id is like work-12-1792485967"));
    }
    let snap = open(dirs)?;
    let raw = find(&snap, id)?;
    let mut keys: Vec<&str> = FIELDS.iter().map(|f| f.0).collect();
    keys.push(CONCURRENT.0);
    let mut spec = spec(&keys);
    spec["steps"] = json!([spec_of(&STEP)]);
    spec["roles"] = json!([spec_of(&ROLE)]);
    spec["filed"] = json!([{"issue": true, "url": true}]);
    let mut s = pick_object(&raw, &spec);
    let logs: Vec<Value> = raw["logs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| {
            let file = l["word"].as_str()?;
            Some(json!({"file": file, "bytes": l["bytes"].as_u64()?}))
        })
        .collect();
    s.insert("logs".into(), Value::Array(logs));
    let (i, p) = session_links(&repo(dirs), &raw);
    insert_opt(&mut s, "issueUrl", i);
    insert_opt(&mut s, "prUrl", p);
    let mut out = Map::new();
    out.insert("stale".into(), json!(snap.stale));
    out.insert("session".into(), Value::Object(s));
    Ok(out)
}

fn spec_of(fields: &[(&str, &str, &str)]) -> Value {
    spec(&fields.iter().map(|f| f.0).collect::<Vec<_>>())
}

#[derive(Default)]
struct Pair {
    runs: u64,
    outcomes: BTreeMap<String, u64>,
    kinds: BTreeMap<String, u64>,
    causes: BTreeMap<(String, Option<String>), Cause>,
}

#[derive(Default)]
struct Cause {
    n: u64,
    issues: Vec<i64>,
}

pub(crate) fn failures(dirs: &Dirs) -> Result<Map<String, Value>, ToolError> {
    let snap = open(dirs)?;
    let repo = repo(dirs);
    let mut pairs: BTreeMap<(String, String), Pair> = BTreeMap::new();
    for s in snap.sessions().filter(|s| s["kind"] == "work") {
        let text = |k: &str| s[k].as_str().map(str::to_string);
        let tehas = text("tehasVersion").unwrap_or_default();
        let rukas = text("rukasVersion")
            .or_else(|| text("rukasCommit"))
            .unwrap_or_default();
        let p = pairs.entry((tehas, rukas)).or_default();
        p.runs += 1;
        *p.outcomes
            .entry(text("outcome").unwrap_or_else(|| "unknown".into()))
            .or_default() += 1;
        let kind = text("failureKind");
        if let Some(k) = &kind {
            *p.kinds.entry(k.clone()).or_default() += 1;
        }
        if let Some(cause) = text("failureCause") {
            let c = p.causes.entry((cause, kind)).or_default();
            c.n += 1;
            if let Some(i) = s["issue"].as_i64() {
                if c.issues.len() < CAUSE_ISSUES && !c.issues.contains(&i) {
                    c.issues.push(i);
                }
            }
        }
    }
    let mut ordered: Vec<_> = pairs.into_iter().collect();
    ordered.sort_by(|a, b| b.1.runs.cmp(&a.1.runs));
    let versions: Vec<Value> = ordered
        .into_iter()
        .map(|((tehas, rukas), p)| {
            let mut causes: Vec<_> = p.causes.into_iter().collect();
            causes.sort_by(|a, b| b.1.n.cmp(&a.1.n));
            let causes: Vec<Value> = causes
                .into_iter()
                .take(TOP_CAUSES)
                .map(|((cause, kind), c)| {
                    let issues: Vec<String> = c
                        .issues
                        .iter()
                        .filter_map(|i| links(repo.as_deref(), *i, None).0)
                        .collect();
                    let mut o = Map::new();
                    o.insert("cause".into(), json!(cause));
                    insert_opt(&mut o, "kind", kind);
                    o.insert("n".into(), json!(c.n));
                    o.insert("issues".into(), json!(issues));
                    Value::Object(o)
                })
                .collect();
            let mut o = Map::new();
            if !tehas.is_empty() {
                o.insert("tehasVersion".into(), json!(tehas));
            }
            if !rukas.is_empty() {
                o.insert("rukasVersion".into(), json!(rukas));
            }
            o.insert("runs".into(), json!(p.runs));
            o.insert("outcomes".into(), json!(p.outcomes));
            o.insert("failureKinds".into(), json!(p.kinds));
            o.insert("causes".into(), Value::Array(causes));
            Value::Object(o)
        })
        .collect();
    let mut out = Map::new();
    out.insert("days".into(), json!(snap.days));
    out.insert("stale".into(), json!(snap.stale));
    out.insert("versions".into(), Value::Array(versions));
    Ok(out)
}
