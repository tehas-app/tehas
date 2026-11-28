//! The session tools: `factory_sessions`, `factory_session` and
//! `factory_failures`, over the sessions snapshot. They read it one line at a
//! time and copy only the fields of the contract in `tehas_core::snapshot`,
//! which the analytics writer reads too. A log's file name is never returned.

use crate::args::{Arg, Args};
use crate::snapshot::{Fact, Loose, Many, Scalar, Session, Snapshot};
use crate::tools::{self, Dirs, Links, ToolError};
use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::sync::LazyLock;
use tehas_core::queue_lists::{AWAITING, COMPLETED, QUEUED, RUNNING, STOPPED};
use tehas_ports::{BlueprintNo, ChangeNo, Cite, Cites, Ref};

// The words an answer may give for an outcome or a kind, and the words the
// filters of `factory_sessions` accept. The outcomes are the core's one
// vocabulary; `factory_issue` describes the same fields of an attempt, so it
// reads these rather than a list written twice.
pub const OUTCOMES: [&str; 7] = tehas_core::outcome::OUTCOMES;
pub const KINDS: [&str; 2] = ["work", "plan"];
// Every word a record's `how` can hold, work and plan words alike: the words
// the store's verbs take and the plan words analytics' `plan::how` writes. The tool's
// schema enum and its filter both read this list, and nothing else in the
// crate names it; the test of the filter reads it here, so it pins the tool to
// what it says rather than to a list written twice.
pub const HOWS: [&str; 9] = [
    "approved",
    "refresh",
    "outage-retry",
    "millwright",
    "host",
    "answer",
    "reply",
    "refine",
    "hand",
];
const DEFAULT_LIMIT: u64 = 50;
const MAX_LIMIT: u64 = 200;
const TOP_CAUSES: usize = 5;
const CAUSE_ISSUES: usize = 10;

pub(crate) static ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(work|plan)-[1-9][0-9]{0,8}-[0-9]{9,11}$").unwrap());
// The word of a version, a failure kind or a park reason: the one rule of the
// crate, from tools, which the schemas below name as their pattern.
pub use tools::WORD;
// An image id, or its first 12 hex or more, with or without `sha256:`.
static IMAGE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(sha256:)?[0-9a-f]{12,64}$").unwrap());
const GROUPS: [&str; 2] = ["version", "image"];

/// The snapshot's fields as this crate names them: key, JSON type,
/// description. All three come from `tehas_core::snapshot`, in its order, so
/// the writer's columns and the fields this reader copies are one list.
const fn contract<const N: usize>(
    fields: [tehas_core::snapshot::Field; N],
) -> [(&'static str, &'static str, &'static str); N] {
    let mut out: [(&'static str, &'static str, &'static str); N] = [("", "", ""); N];
    let mut i = 0;
    while i < N {
        out[i] = (fields[i].0, fields[i].1.json_type(), fields[i].2);
        i += 1;
    }
    out
}

/// The scalar fields of a session, `concurrent` last. A summary and an attempt
/// answer a subset of them, named below.
const FIELDS: [(&str, &str, &str); 39] = contract(tehas_core::snapshot::RUN_FIELDS);
/// What a summary of a session answers: a subset of `FIELDS`. A name here
/// that is not a field of the contract would answer nothing for it, so the
/// tests check these names against the contract.
pub const SUMMARY: [&str; 17] = [
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
    "image",
    "imageBuilt",
];
const STEP: [(&str, &str, &str); 5] = contract(tehas_core::snapshot::STEP_FIELDS);
const ROLE: [(&str, &str, &str); 10] = contract(tehas_core::snapshot::ROLE_FIELDS);

fn field(f: (&str, &str, &str)) -> (String, Value) {
    (f.0.to_string(), json!({"type": f.1, "description": f.2}))
}

/// The fields as schema properties. A field that holds a word of a closed
/// list names the list, so an answer is checked against the words too.
fn object_of(fields: &[(&str, &str, &str)]) -> Value {
    let mut props: Map<String, Value> = fields.iter().map(|f| field(*f)).collect();
    for (key, words) in [
        ("outcome", &OUTCOMES[..]),
        ("kind", &KINDS[..]),
        ("how", &HOWS[..]),
    ] {
        if let Some(p) = props.get_mut(key) {
            p["enum"] = json!(words);
        }
    }
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
    let mut item = object_of(&FIELDS);
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

fn counts(desc: &str) -> Value {
    json!({"type": "object", "description": desc,
           "additionalProperties": {"type": "integer"}})
}

fn failures_schema() -> Value {
    let cause = json!({"type": "object", "properties": {
        "cause": text("The failure cause."),
        "kind": text("Its failure kind. Left out when none was recorded."),
        "n": count("Work sessions with this cause."),
        "issues": array_of(text("An issue link."), "Up to ten issues, newest first. Empty when the dashboard publishes no status file or no valid repo in it."),
    }});
    let pair = json!({"type": "object", "properties": {
        "tehasVersion": text("The tehas version. Left out when unknown."),
        "rukasVersion": text("The pi-rukas version, or its commit. Left out when unknown."),
        "image": text("With by image: the sandbox image's id. Left out when the runs recorded none."),
        "imageBuilt": text("With by image: when that image was built, UTC. Left out when unknown."),
        "runs": count("Work sessions on this pair."),
        "outcomes": {"type": "object", "description": "Sessions per outcome, by the outcome words.",
                     "propertyNames": {"enum": OUTCOMES},
                     "additionalProperties": {"type": "integer"}},
        "outcomesUnknown": count("Sessions with no outcome word on record: none, or a word that is not an outcome. Counted here and not in outcomes."),
        "failureKinds": counts("Sessions per failure kind."),
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
            array_of(pair, "One entry per version pair, or per pair and image with by image, the most runs first. An entry with either version unknown comes after every entry that names both."),
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
        arg(
            "image",
            "string",
            "Only sessions that ran this sandbox image: its id sha256:<hex>, or the first 12 hex or more of it.",
        ),
        json!({"pattern": IMAGE.as_str()}),
    );
    with(
        arg("kind", "string", "Only work or plan sessions."),
        json!({"enum": KINDS}),
    );
    with(
        arg(
            "how",
            "string",
            "Only sessions started this way. refine lists refining sessions alone; a value outside the list is a tool error.",
        ),
        json!({"enum": HOWS}),
    );
    with(
        arg("issue", "integer", "Only sessions on this issue."),
        json!({"minimum": 1, "maximum": tools::ISSUE_MAX}),
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
        (
            "leftOut",
            count("Sessions skipped while filling this page because a field of theirs is not of the type or a word the schema names. A page counts only its own; later pages count theirs. Left out when none."),
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
            "description": "The runs of the last 30 days, newest first, with their outcome, failure, versions and sandbox image. Filter by outcome, failure kind, version, image, kind, how it started or issue, and page with limit and before.",
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
            "description": "Work runs of the last 30 days per tehas and pi-rukas version pair: runs, outcomes, failure kinds and the five most common causes with links to the issues. Most runs first. With by image, per pair and sandbox image, so two builds of one pi-rukas version can be compared; rukasVersion keeps one version.",
            "inputSchema": {"type": "object", "properties": {
                "by": {"type": "string", "enum": GROUPS, "description": "version (the default) or image: also split each pair by the image its runs used."},
                "rukasVersion": {"type": "string", "pattern": WORD.as_str(), "description": "Only runs of this pi-rukas version or commit, matched exactly."},
            }, "additionalProperties": false},
            "outputSchema": failures_schema(),
        }),
    ]
}

/// Where a person opens an issue and its pull request, as the code host
/// links them (`Cites::link`). With no code host for the repository, or a
/// number that is no item's, nothing is linked.
pub struct Linker(Option<Box<dyn Cites + Send>>);

impl Linker {
    /// The links of the repository `repo` names, made by `links`.
    pub fn of(links: Links, repo: Option<&str>) -> Linker {
        Linker(repo.and_then(links))
    }

    fn link(&self, what: Cite) -> Option<String> {
        Some(self.0.as_ref()?.link(&what)?.as_str().to_string())
    }

    /// The link of issue `issue`.
    pub fn issue(&self, issue: i64) -> Option<String> {
        let no = BlueprintNo::new(u64::try_from(issue).ok()?)?;
        self.link(Cite::Blueprint(Ref::Here(no)))
    }

    /// The link of pull request `pr`.
    pub fn pr(&self, pr: i64) -> Option<String> {
        let no = ChangeNo::new(u64::try_from(pr).ok()?)?;
        self.link(Cite::Change(Ref::Here(no)))
    }
}

fn bad(why: &str) -> ToolError {
    ToolError::BadArgs(why.to_string())
}

/// The session `id` as the snapshot has it, inside the window. The id is
/// checked before anything is opened.
pub fn find(snap: &Snapshot, id: &str) -> Result<Session, ToolError> {
    if !ID.is_match(id) {
        return Err(bad("id is like work-12-1792485967"));
    }
    snap.sessions()
        .find(|s| s.text("id") == Some(id))
        .ok_or_else(|| bad(&format!("no session {id} in the last {} days", snap.days)))
}

/// `live/status.json`, read for the server: the repository whose items an
/// answer links, and the numbers the dashboard lists under each stage.
#[derive(Deserialize, Default)]
pub(crate) struct Status {
    #[serde(default)]
    pub repo: Loose<String>,
    #[serde(flatten)]
    pub stages: BTreeMap<String, Many<StageRow>>,
}

/// One row of a stage list: its number, and its title as a scalar, which
/// `tools::title` serves on one line.
#[derive(Deserialize, Default)]
pub(crate) struct StageRow {
    #[serde(default)]
    pub number: Loose<i64>,
    #[serde(default)]
    pub title: Fact,
}

/// `live/running.json`, read for the server: the cycles it says are running.
#[derive(Deserialize, Default)]
pub(crate) struct Running {
    #[serde(default)]
    pub cycles: Many<CycleRow>,
}

/// One cycle of the running file: the issue it runs, whether it runs, the
/// store's start its id is built from, and the step and status word the
/// dashboard gives it.
#[derive(Deserialize, Default)]
pub(crate) struct CycleRow {
    #[serde(default)]
    pub issue: Fact,
    #[serde(default)]
    pub running: Loose<bool>,
    #[serde(rename = "startedAt", default)]
    pub started_at: Loose<i64>,
    #[serde(default)]
    pub step: Fact,
    #[serde(default)]
    pub status: Fact,
}

/// `live/health.json`, read for the server: the issues of the planning check.
#[derive(Deserialize, Default)]
pub(crate) struct Health {
    #[serde(default)]
    pub checks: Loose<Checks>,
}

#[derive(Deserialize, Default)]
pub(crate) struct Checks {
    #[serde(default)]
    pub plans: Many<Planned>,
}

#[derive(Deserialize, Default)]
pub(crate) struct Planned {
    #[serde(default)]
    pub issue: Loose<i64>,
}

/// A published file, read and typed: a file the host has not written reads as
/// the empty reading, and a field of another type than the one named reads as
/// nothing - what indexing a document gave for any shape this file is not. A
/// file that is there and cannot be read or parsed is the tool's error: "I
/// could not look" is never answered as "it is not listed".
pub(crate) fn live<T: serde::de::DeserializeOwned + Default>(
    dirs: &Dirs,
    file: &str,
) -> Result<T, ToolError> {
    match std::fs::metadata(dirs.live.join(file)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        _ => tools::read(&dirs.live, file).map(|v| serde_json::from_value(v).unwrap_or_default()),
    }
}

/// The links of the repository the published status names.
fn linker(dirs: &Dirs) -> Result<Linker, ToolError> {
    let status = live::<Status>(dirs, "status.json")?;
    Ok(Linker::of(
        dirs.links,
        status.repo.get().map(String::as_str),
    ))
}

fn session_links(links: &Linker, s: &Session) -> (Option<String>, Option<String>) {
    match s.count("issue") {
        Some(n) => (links.issue(n), s.count("change").and_then(|p| links.pr(p))),
        None => (None, None),
    }
}

fn word(args: &Args, key: &str) -> Result<Option<String>, ToolError> {
    match args.get(key) {
        Arg::Absent => Ok(None),
        Arg::Text(s) if WORD.is_match(s) => Ok(Some(s.to_string())),
        _ => Err(bad(&format!("{key} is 1 to 64 of A-Za-z0-9._+-"))),
    }
}

fn one_of(args: &Args, key: &str, allowed: &[&str]) -> Result<Option<String>, ToolError> {
    match args.get(key) {
        Arg::Absent => Ok(None),
        Arg::Text(s) if allowed.contains(&s) => Ok(Some(s.to_string())),
        _ => Err(bad(&format!("{key} is one of {}", allowed.join(", ")))),
    }
}

fn spec(keys: &[&str]) -> Value {
    Value::Object(keys.iter().map(|k| (k.to_string(), json!(true))).collect())
}

fn open(dirs: &Dirs) -> Result<Snapshot, ToolError> {
    Snapshot::open(&dirs.sessions, tools::now())
}

/// What `factory_sessions` takes: the filters, each given or not, the most
/// sessions of a page and the session the page comes after.
#[derive(Debug)]
pub struct ListArgs {
    outcome: Option<String>,
    kind: Option<String>,
    how: Option<String>,
    failure_kind: Option<String>,
    tehas: Option<String>,
    rukas: Option<String>,
    /// The image id's hex, without `sha256:`.
    image: Option<String>,
    issue: Option<i64>,
    limit: usize,
    before: Option<String>,
}

impl ListArgs {
    pub(crate) fn read(args: &Args) -> Result<Self, ToolError> {
        let outcome = one_of(args, "outcome", &OUTCOMES)?;
        let kind = one_of(args, "kind", &KINDS)?;
        let how = one_of(args, "how", &HOWS)?;
        let failure_kind = word(args, "failureKind")?;
        let tehas = word(args, "tehasVersion")?;
        let rukas = word(args, "rukasVersion")?;
        let image = match args.get("image") {
            Arg::Absent => None,
            Arg::Text(s) if IMAGE.is_match(s) => {
                Some(s.strip_prefix("sha256:").unwrap_or(s).to_string())
            }
            _ => {
                return Err(bad(
                    "image is an image id sha256:<hex>, or 12 to 64 of its hex",
                ))
            }
        };
        let issue = match args.get("issue") {
            Arg::Absent => None,
            v => Some(i64::from(tools::issue_arg(&v)?)),
        };
        let limit = match args.get("limit") {
            Arg::Absent => DEFAULT_LIMIT,
            v => v
                .whole()
                .filter(|n| (1..=MAX_LIMIT).contains(n))
                .ok_or_else(|| bad(&format!("limit is from 1 to {MAX_LIMIT}")))?,
        } as usize;
        let before = match args.get("before") {
            Arg::Absent => None,
            Arg::Text(s) if ID.is_match(s) => Some(s.to_string()),
            _ => return Err(bad("before is a session id")),
        };
        Ok(ListArgs {
            outcome,
            kind,
            how,
            failure_kind,
            tehas,
            rukas,
            image,
            issue,
            limit,
            before,
        })
    }
}

pub(crate) fn list(dirs: &Dirs, asked: &ListArgs) -> Result<Map<String, Value>, ToolError> {
    let ListArgs {
        outcome,
        kind,
        how,
        failure_kind,
        tehas,
        rukas,
        image,
        issue,
        limit,
        before,
    } = asked;
    let (limit, before) = (*limit, before.as_deref());
    let json_of = |f: &Scalar| serde_json::to_value(f).expect("a scalar serialises");

    let snap = open(dirs)?;
    let links = linker(dirs)?;
    let eq = |s: &Session, key: &str, want: &Option<String>| {
        want.as_deref().is_none_or(|w| s.text(key) == Some(w))
    };
    let item = summary_schema();
    let summary = |s: &Session| {
        let mut o = Map::new();
        for k in SUMMARY {
            if let Some(f) = s.field(k) {
                o.insert(k.to_string(), json_of(f));
            }
        }
        let (i, p) = session_links(&links, s);
        insert_opt(&mut o, "issueUrl", i);
        insert_opt(&mut o, "prUrl", p);
        Value::Object(o)
    };
    let mut passed = before.is_none();
    let mut found: Vec<Value> = Vec::new();
    let mut left_out = 0u64;
    for s in snap.sessions() {
        if !passed {
            passed = s.text("id") == before;
            continue;
        }
        let keep = eq(&s, "outcome", outcome)
            && eq(&s, "kind", kind)
            && eq(&s, "how", how)
            && eq(&s, "failureKind", failure_kind)
            && eq(&s, "tehasVersion", tehas)
            && rukas.as_deref().is_none_or(|w| {
                s.text("rukasVersion") == Some(w) || s.text("rukasCommit") == Some(w)
            })
            && image.as_deref().is_none_or(|w| {
                s.text("image")
                    .and_then(|i| i.strip_prefix("sha256:"))
                    .is_some_and(|i| i.starts_with(w))
            })
            && issue.is_none_or(|n| s.count("issue") == Some(n));
        if !keep {
            continue;
        }
        let o = summary(&s);
        if crate::conform::check(&o, &item).is_err() {
            left_out += 1;
            continue;
        }
        found.push(o);
        if found.len() > limit {
            break;
        }
    }
    if !passed {
        return Err(bad(&format!(
            "before is not a session of the last {} days",
            snap.days
        )));
    }
    let more = found.len() > limit;
    found.truncate(limit);
    let next = more
        .then(|| found.last().map(|s| s["id"].clone()))
        .flatten();

    let mut out = Map::new();
    out.insert("days".into(), json!(snap.days));
    out.insert("stale".into(), json!(snap.stale));
    out.insert("sessions".into(), Value::Array(found));
    if let Some(n) = next {
        out.insert("next".into(), n);
    }
    if left_out > 0 {
        out.insert("leftOut".into(), json!(left_out));
    }
    Ok(out)
}

fn insert_opt(o: &mut Map<String, Value>, key: &str, v: Option<String>) {
    if let Some(v) = v {
        o.insert(key.into(), json!(v));
    }
}

/// As `insert_opt`, for a value the answer carries as it is.
fn put(o: &mut Map<String, Value>, key: &str, v: Option<Value>) {
    if let Some(v) = v {
        o.insert(key.into(), v);
    }
}

/// What `factory_session` takes: the id of one session.
#[derive(Debug)]
pub struct OneArgs {
    id: String,
}

impl OneArgs {
    pub(crate) fn read(args: &Args) -> Result<Self, ToolError> {
        let id = args
            .get("id")
            .text()
            .ok_or_else(|| bad("id is a string like work-12-1792485967"))?;
        if !ID.is_match(id) {
            return Err(bad("id is like work-12-1792485967"));
        }
        Ok(OneArgs { id: id.to_string() })
    }
}

pub(crate) fn one(dirs: &Dirs, asked: &OneArgs) -> Result<Map<String, Value>, ToolError> {
    let json_of = |f: &Scalar| serde_json::to_value(f).expect("a scalar serialises");
    let id = asked.id.as_str();
    let snap = open(dirs)?;
    let raw = find(&snap, id)?;
    let mut s = Map::new();
    for (k, _, _) in FIELDS {
        if let Some(f) = raw.field(k) {
            s.insert(k.to_string(), json_of(f));
        }
    }
    // The three lists of the session, each element copied with its named keys,
    // as `pick` copied them: a key the line holds as an object, an array or
    // null is left out, and one of another scalar type travels to the check.
    let step_keys: Vec<&str> = STEP.iter().map(|f| f.0).collect();
    let role_keys: Vec<&str> = ROLE.iter().map(|f| f.0).collect();
    for (key, named, rows) in [
        ("steps", &step_keys[..], raw.steps()),
        ("roles", &role_keys[..], raw.roles()),
        ("filed", &["issue", "url"][..], raw.filed()),
    ] {
        if let Some(rows) = rows {
            let a: Vec<Value> = rows
                .iter()
                .map(|row| {
                    let mut o = Map::new();
                    for k in named {
                        if let Some(f) = row.get(*k) {
                            o.insert((*k).to_string(), json_of(f));
                        }
                    }
                    Value::Object(o)
                })
                .collect();
            s.insert(key.to_string(), Value::Array(a));
        }
    }
    let logs: Vec<Value> = raw
        .logs()
        .iter()
        .filter_map(|l| Some(json!({"file": l.word()?, "bytes": l.bytes()?})))
        .collect();
    s.insert("logs".into(), Value::Array(logs));
    let (i, p) = session_links(&linker(dirs)?, &raw);
    insert_opt(&mut s, "issueUrl", i);
    insert_opt(&mut s, "prUrl", p);
    let s = Value::Object(s);
    crate::conform::check(&s, &session_schema()).map_err(|why| ToolError::File {
        file: crate::snapshot::FILE.into(),
        why: format!("has session {id} that does not match the schema: {why}"),
    })?;
    let mut out = Map::new();
    out.insert("stale".into(), json!(snap.stale));
    out.insert("session".into(), s);
    Ok(out)
}

#[derive(Default)]
struct Pair {
    built: Option<String>,
    runs: u64,
    outcomes: BTreeMap<String, u64>,
    outcomes_unknown: u64,
    kinds: BTreeMap<String, u64>,
    causes: BTreeMap<(String, Option<String>), Cause>,
}

#[derive(Default)]
struct Cause {
    n: u64,
    issues: Vec<i64>,
}

/// What `factory_failures` takes: whether to group by image, and the one
/// rukas version to count.
#[derive(Debug)]
pub struct FailuresArgs {
    by_image: bool,
    only: Option<String>,
}

impl FailuresArgs {
    pub(crate) fn read(args: &Args) -> Result<Self, ToolError> {
        let by_image = one_of(args, "by", &GROUPS)?.as_deref() == Some("image");
        let only = word(args, "rukasVersion")?;
        Ok(FailuresArgs { by_image, only })
    }
}

pub(crate) fn failures(dirs: &Dirs, asked: &FailuresArgs) -> Result<Map<String, Value>, ToolError> {
    let (by_image, only) = (asked.by_image, &asked.only);
    let snap = open(dirs)?;
    let links = linker(dirs)?;
    let mut pairs: BTreeMap<(Option<String>, Option<String>, String), Pair> = BTreeMap::new();
    for s in snap.sessions().filter(|s| s.text("kind") == Some("work")) {
        let text = |k: &str| s.text(k).map(str::to_string);
        if only.as_deref().is_some_and(|w| {
            text("rukasVersion").as_deref() != Some(w) && text("rukasCommit").as_deref() != Some(w)
        }) {
            continue;
        }
        let named = |k: &str| text(k).filter(|v| !v.is_empty());
        let tehas = named("tehasVersion");
        let rukas = named("rukasVersion").or_else(|| named("rukasCommit"));
        let image = match by_image {
            true => text("image").unwrap_or_default(),
            false => String::new(),
        };
        let p = pairs.entry((tehas, rukas, image)).or_default();
        if by_image && p.built.is_none() {
            p.built = text("imageBuilt");
        }
        p.runs += 1;
        match text("outcome").filter(|o| OUTCOMES.contains(&o.as_str())) {
            Some(o) => *p.outcomes.entry(o).or_default() += 1,
            None => p.outcomes_unknown += 1,
        }
        let kind = text("failureKind");
        if let Some(k) = &kind {
            *p.kinds.entry(k.clone()).or_default() += 1;
        }
        if let Some(cause) = text("failureCause") {
            let c = p.causes.entry((cause, kind)).or_default();
            c.n += 1;
            if let Some(i) = s.count("issue") {
                if c.issues.len() < CAUSE_ISSUES && !c.issues.contains(&i) {
                    c.issues.push(i);
                }
            }
        }
    }
    let mut ordered: Vec<_> = pairs.into_iter().collect();
    ordered.sort_by_key(|((t, r, _), p)| (t.is_none() || r.is_none(), std::cmp::Reverse(p.runs)));
    let versions: Vec<Value> = ordered
        .into_iter()
        .map(|((tehas, rukas, image), p)| {
            let mut causes: Vec<_> = p.causes.into_iter().collect();
            causes.sort_by_key(|a| std::cmp::Reverse(a.1.n));
            let causes: Vec<Value> = causes
                .into_iter()
                .take(TOP_CAUSES)
                .map(|((cause, kind), c)| {
                    let issues: Vec<String> =
                        c.issues.iter().filter_map(|i| links.issue(*i)).collect();
                    let mut o = Map::new();
                    o.insert("cause".into(), json!(cause));
                    insert_opt(&mut o, "kind", kind);
                    o.insert("n".into(), json!(c.n));
                    o.insert("issues".into(), json!(issues));
                    Value::Object(o)
                })
                .collect();
            let mut o = Map::new();
            insert_opt(&mut o, "tehasVersion", tehas);
            insert_opt(&mut o, "rukasVersion", rukas);
            if !image.is_empty() {
                o.insert("image".into(), json!(image));
                insert_opt(&mut o, "imageBuilt", p.built);
            }
            o.insert("runs".into(), json!(p.runs));
            o.insert("outcomes".into(), json!(p.outcomes));
            o.insert("outcomesUnknown".into(), json!(p.outcomes_unknown));
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

/// The stage lists of the dashboard, in the order `factory_issues` gives them.
pub(crate) const ISSUE_STAGES: [&str; 5] = [QUEUED, RUNNING, AWAITING, STOPPED, COMPLETED];

// The attempt's fields this tool copies as the snapshot gives them, as
// `SUMMARY` is for a summary: a name here that is not a field of the contract
// would answer nothing for it, so the tests check these names against it. Two
// more are named by `issue_spec` and put in by hand: `prUrl`, whose link the
// snapshot does not hold, and the session's own `id`, which `factory_sessions`
// gives and this answer leaves out to keep an attempt to its story.
pub const ATTEMPT_KEYS: [&str; 11] = [
    "kind",
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
];

pub(crate) fn issue_spec() -> Value {
    let mut s = Map::new();
    for k in [
        "issue",
        "issueUrl",
        "stage",
        "title",
        "firstSeen",
        "lastSeen",
        "outcome",
    ] {
        s.insert(k.into(), json!(true));
    }
    s.insert("running".into(), json!({"step": true, "status": true}));
    let mut attempt = spec(&ATTEMPT_KEYS);
    // Named by the schema and written here, so they are named in the allowlist
    // too: `pick` never sees either, as both come from outside the record.
    attempt["id"] = json!(true);
    attempt["prUrl"] = json!(true);
    s.insert("attempts".into(), json!([attempt]));
    Value::Object(s)
}

/// What `factory_issue` takes: the issue's number.
#[derive(Debug)]
pub struct IssueArgs {
    issue: i64,
}

impl IssueArgs {
    pub(crate) fn read(args: &Args) -> Result<Self, ToolError> {
        let issue = i64::from(tools::issue_arg(&args.get("issue"))?);
        Ok(IssueArgs { issue })
    }
}

/// One issue's story: the stage the dashboard lists it under, the cycle that
/// owns it now, and its runs of the snapshot's window. The caller does not
/// join `factory_issues`, `factory_sessions` and `factory_session`.
pub(crate) fn issue(dirs: &Dirs, asked: &IssueArgs) -> Result<Map<String, Value>, ToolError> {
    let issue = asked.issue;
    let json_of = |f: &Scalar| serde_json::to_value(f).expect("a scalar serialises");
    // The snapshot first: its header is the one file this tool must read, and
    // a host without it has nothing to answer with.
    let snap = open(dirs)?;
    let status = live::<Status>(dirs, "status.json")?;
    let links = Linker::of(dirs.links, status.repo.get().map(String::as_str));

    // The first list that holds the number, as the dashboard shows it. The
    // planning list is not in status.json: it gives a count, so the numbers
    // come from the poll's plans, as `factory_issues` takes them.
    let mut stage: Option<&'static str> = None;
    let mut title: Option<Scalar> = None;
    for name in ISSUE_STAGES {
        let row = status
            .stages
            .get(name)
            .into_iter()
            .flat_map(|m| m.list())
            .find(|r| r.number.get() == Some(&issue));
        if let Some(r) = row {
            stage = Some(name);
            title = r.title.0.clone();
            break;
        }
    }
    let running = live::<Running>(dirs, "running.json")?;
    let running_cycle = running
        .cycles
        .list()
        .iter()
        .find(|c| c.issue.count() == Some(issue));
    if stage.is_none() && running_cycle.is_none() {
        let planning = live::<Health>(dirs, "health.json")?
            .checks
            .into_inner()
            .is_some_and(|c| c.plans.list().iter().any(|p| p.issue.get() == Some(&issue)));
        if planning {
            stage = Some("planning");
        }
    }
    // A row of the running file is the dashboard's own word for the issue:
    // its step and status are served whatever the stage, but only a cycle
    // that is running counts as the `running` stage.
    if stage.is_none() && running_cycle.is_some_and(|c| c.running.get() == Some(&true)) {
        stage = Some("running");
    }

    // The attempt's fields the answer copies, as the snapshot gives them, with
    // the two the answer writes: `prUrl`, whose link the snapshot does not
    // hold, is named here too so one of a hand-written line is no more served
    // than `pick` once served it, until the link replaces it.
    let mut attempts: Vec<Value> = snap
        .sessions()
        .filter(|s| s.count("issue") == Some(issue))
        .map(|s| {
            let mut o = Map::new();
            for k in ATTEMPT_KEYS.into_iter().chain(["id", "prUrl"]) {
                if let Some(f) = s.field(k) {
                    o.insert(k.to_string(), json_of(f));
                }
            }
            insert_opt(&mut o, "prUrl", s.count("change").and_then(|p| links.pr(p)));
            Value::Object(o)
        })
        .collect();
    // The snapshot is newest first; the story reads oldest first.
    attempts.reverse();

    let mut out = Map::new();
    out.insert("issue".into(), json!(issue));
    insert_opt(&mut out, "issueUrl", links.issue(issue));
    if let Some(s) = stage {
        out.insert("stage".into(), json!(s));
    }
    put(
        &mut out,
        "title",
        tools::title(title.as_ref().map(&json_of)),
    );
    if let Some(c) = running_cycle {
        let mut r = Map::new();
        put(&mut r, "step", c.step.0.as_ref().map(&json_of));
        put(&mut r, "status", c.status.0.as_ref().map(&json_of));
        tools::cycle_status_in(&mut r);
        if !r.is_empty() {
            out.insert("running".into(), Value::Object(r));
        }
    }
    out.insert("attempts".into(), Value::Array(attempts.clone()));
    if stage.is_none() && attempts.is_empty() {
        // The dashboard lists it nowhere and it has not run in the window.
        return Err(bad(&format!(
            "no cycle of the last {} days and no stage for issue {issue}",
            snap.days
        )));
    }
    // The named keys copied as they are: an attempt holds only scalars, so
    // the key is the value or nothing, which is all `pick` with `true` made.
    if let Some(first) = attempts.first() {
        put(&mut out, "firstSeen", first.get("started").cloned());
        if let Some(last) = attempts.last() {
            put(&mut out, "lastSeen", last.get("started").cloned());
            put(&mut out, "outcome", last.get("outcome").cloned());
        }
    }
    Ok(out)
}
