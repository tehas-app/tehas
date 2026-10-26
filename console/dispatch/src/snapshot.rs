//! What one poll read: from GitHub, the state store and the host. Built from
//! JSON in the shapes `gh`, the store and the files answer in. A field that a
//! poll reads only when it needs it is None until it is read.

use crate::events::Events;
use crate::slice::{Reading, Scope};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// A list GitHub gave, or a call that failed.
#[derive(Clone, Debug, Default)]
pub struct Issues(pub Option<Vec<Listed>>);

#[derive(Clone, Debug)]
pub struct Listed {
    /// The number as GitHub gave it; one that is not a number is None.
    pub number: Option<u64>,
    pub labels: Vec<String>,
    pub state: String,
}

/// What a poll reads of one issue when it decides about it.
#[derive(Clone, Debug)]
pub struct Issue {
    pub events: Events,
    /// The edit record: lastEditedAt, the editor's login, the body and the
    /// title. None when the call failed.
    pub edit: Option<Map<String, Value>>,
    /// `gh api repos/<repo>/issues/<n>`, or None when the call failed.
    pub work: Option<Value>,
    /// The pull requests that close it, {url, state}; None when the call
    /// failed or the answer cannot be read.
    pub prs: Option<Vec<Value>>,
    /// The call worked but its answer is no list of pull requests.
    pub prs_unreadable: bool,
    /// The issue's last 100 comments, for tehas:blocked; None when the call
    /// failed.
    pub comments: Option<Value>,
    /// The states of the issues its blocker comment names, by number: OPEN,
    /// CLOSED or MERGED. A number missing is one GitHub gave no state for.
    pub blockers: BTreeMap<u64, String>,
    /// The issue's park on the state store (park-show's .park), or the store's
    /// word and exit status when it could not say.
    pub park: Option<Result<Option<Value>, StoreErr>>,
    /// The approval of its latest start (tehas_approval_view), Err when the
    /// store could not answer.
    pub approval: Option<Result<Option<Value>, ()>>,
    /// approval-current's .current, for the owner of a refresh.
    pub current: Option<Result<Option<Value>, StoreErr>>,
    /// The question on record: {plan, id, kind} as plan-show, park-show and
    /// question-known answer for it.
    pub question: Option<Result<Value, StoreErr>>,
    /// gh issue view --json state,labels, for a retry after an outage.
    pub view: Option<Value>,
    /// The cycle history on record (tehas_cycle_history), Err when the store
    /// could not answer.
    pub history: Option<Result<Vec<Value>, ()>>,
    /// state/refresh/<n>.json, as written; and what <n>.said holds.
    pub request: Option<Value>,
    pub refresh_said: Option<String>,
}

/// A running cycle as cycle-show --running has it. 0 is none on record.
#[derive(Clone, Debug, Default)]
pub struct Cycle {
    /// When it took its lock: 0 for a lock with no open record.
    pub took: u64,
    pub heard: u64,
    /// The quiet episode on record: the heard time it was said for, when it
    /// was seen first, the last ask and the asks in it.
    pub said_for: String,
    pub quiet_at: u64,
    pub asked: u64,
    pub times: u64,
}

/// What a cycle left on this host for an issue, as the sweep finds it.
#[derive(Clone, Debug)]
pub struct Leftover {
    pub issue: u64,
    /// One of its names, or the lock, was used within 7 days, or cannot be
    /// dated.
    pub young: bool,
    /// The lock is a lock file, or there is none.
    pub lock_ok: bool,
}

/// A call to the state store that failed: its word, exit status and first
/// line, as lib/store.sh keeps them.
#[derive(Clone, Debug, Default)]
pub struct StoreErr {
    pub word: String,
    pub rc: i32,
    pub line: String,
}

impl StoreErr {
    fn parse(v: &Value) -> StoreErr {
        let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        StoreErr {
            word: s("word"),
            rc: v.get("rc").and_then(Value::as_i64).unwrap_or(1) as i32,
            line: s("line"),
        }
    }
}

/// What a poll asks before it starts an issue that links an attachment, with
/// TEHAS_WORK_IMAGES=1.
#[derive(Clone, Debug, Default)]
pub struct Images {
    /// The sandbox image's tehas.rukas.images label: 1, 0, or empty.
    pub rukas: String,
    /// Its tehas.rukas label: the pi-rukas commit, or empty.
    pub commit: String,
    /// models.json declares image input for the subagent model.
    pub declares: bool,
    /// The model answered a request that carries an image.
    pub answers: bool,
}

/// The gate of the lowest issues (TEHAS_LOW_GATE).
#[derive(Clone, Debug, Default)]
pub struct Gate {
    /// Its exit status.
    pub answer: i32,
    /// state/low-gate.held: absent, a file, or something that is not one.
    pub held: String,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    /// nameWithOwner, None when `gh repo view` failed.
    pub repo: Option<String>,
    /// `gh api user`'s answer, None when it failed.
    pub user: Option<Value>,
    /// tehas pause's flag: None when there is none, Some(Null) when it is
    /// there and cannot be read.
    pub pause_flag: Option<Value>,
    /// The control issue's labels, None when the call failed.
    pub control_labels: Option<Vec<String>>,
    pub control_events: Option<Events>,
    /// The issues with a planning session open, or the store's word and exit
    /// status when it could not say.
    pub plan_open: Result<Vec<u64>, (String, i32)>,
    /// The issues whose cycle holds its lock, with how each started on record.
    pub running: Vec<(u64, String)>,
    pub ready: Issues,
    pub blocked: Issues,
    pub awaiting: Option<Issues>,
    /// tehas_limit_read's answer.
    pub limit: Value,
    pub slice: Reading,
    /// What the model endpoints answered: the hold, when both failed, and
    /// which endpoint the subagent provider is on.
    pub endpoint_hold: String,
    pub endpoint: String,
    /// What asking them logged: a provider on its backup.
    pub endpoint_log: Vec<String>,
    /// The free kilobytes of the disk, Some(None) when they cannot be read.
    pub disk_kb: Option<Option<u64>>,
    /// The check runs of TEHAS_BRANCH_CHECK on the branch's head, Some(None)
    /// when the call failed.
    pub checks: Option<Option<Value>>,
    /// What state/branch-check says the last poll logged.
    pub branch_note: String,
    /// The why of the last live/dispatch.json.
    pub last_why: String,
    pub gate: Option<Gate>,
    /// The issues with a refresh request (state/refresh/<n>.json), lowest
    /// first.
    pub refresh_requests: Vec<u64>,
    /// What state/refresh/.held says, and whether the sandbox's pi-rukas takes
    /// --continue-pr (asked when a refresh may start).
    pub refresh_held: String,
    pub continues: Option<bool>,
    /// The pull requests a refresh reads, by number: GraphQL's
    /// .data.repository.pullRequest, or {"failed": true}.
    pub pulls: BTreeMap<u64, Value>,
    /// gh issue list --label tehas:question.
    pub asking: Option<Value>,
    /// park-show --automatic's parks, owed a retry after an outage.
    pub owed: Option<Result<Vec<Value>, StoreErr>>,
    pub images: Option<Images>,
    pub issues: BTreeMap<u64, Issue>,
    /// The poll's clock, in seconds, and this host's name.
    pub now: Option<u64>,
    pub hostname: Option<String>,
    /// The running cycles' records, by issue, from the entries of `running`;
    /// Err when the store could not list them (`cycles_failed`).
    pub cycles: Result<BTreeMap<u64, Cycle>, StoreErr>,
    /// gh pr list --author <bot>: asked when a cycle runs.
    pub bot_prs: Option<Value>,
    /// gh issue list --label tehas:running, as it answered.
    pub marked: Option<Value>,
    /// The issues with a refresh in their history.
    pub refresh_history: Option<Result<Vec<u64>, StoreErr>>,
    /// gh pr list --label tehas:refreshing, as it answered.
    pub refreshing: Option<Value>,
    pub leftovers: Option<Vec<Leftover>>,
}

fn failed(v: &Value) -> bool {
    v.get("failed").and_then(Value::as_bool) == Some(true)
}

fn names(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|l| l.get("name").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// A number as GitHub may give it: a number, or text that is one.
pub fn number(v: &Value) -> Option<u64> {
    match v {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn issues(v: Option<&Value>) -> Issues {
    match v {
        Some(v) if failed(v) => Issues(None),
        Some(Value::Array(a)) => Issues(Some(
            a.iter()
                .map(|i| Listed {
                    number: i.get("number").and_then(number),
                    labels: names(i.get("labels")),
                    state: i
                        .get("state")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                })
                .collect(),
        )),
        Some(_) => Issues(None),
        None => Issues(Some(Vec::new())),
    }
}

fn opt(v: &Value) -> Option<Value> {
    if failed(v) {
        None
    } else {
        Some(v.clone())
    }
}

fn reading(v: &Value) -> Reading {
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    Reading {
        show: v.get("show").and_then(Value::as_object).cloned(),
        docker: s("docker"),
        controllers: s("controllers"),
        cpus: v.get("cpus").map(|c| match c {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        }),
        root: s("root").unwrap_or_else(|| "/sys/fs/cgroup".into()),
        scopes: v.get("scopes").and_then(Value::as_object).map(|m| {
            m.iter()
                .map(|(name, sc)| Scope {
                    name: name.clone(),
                    stat: sc.get("stat").and_then(Value::as_str).map(str::to_string),
                    low: sc.get("low").and_then(Value::as_str).map(str::to_string),
                })
                .collect()
        }),
        pressure: s("pressure"),
        cids: v
            .get("cids")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
    }
}

fn issue(v: &Value) -> Issue {
    Issue {
        events: v.get("events").map(Events::parse).unwrap_or(Events::Failed),
        edit: v
            .get("edit")
            .filter(|e| !failed(e))
            .and_then(Value::as_object)
            .cloned(),
        work: v.get("work").and_then(opt),
        prs: v
            .get("prs")
            .filter(|p| !failed(p))
            .and_then(Value::as_array)
            .cloned(),
        prs_unreadable: v.get("prs").is_some_and(|p| !failed(p) && !p.is_array()),
        comments: v.get("comments").and_then(opt),
        blockers: v
            .get("blockers")
            .and_then(Value::as_object)
            .map(|m| {
                m.iter()
                    .filter_map(|(k, s)| Some((k.parse().ok()?, s.as_str()?.to_string())))
                    .collect()
            })
            .unwrap_or_default(),
        park: v.get("park").map(|p| match p {
            Value::Null => Ok(None),
            p if failed(p) => Err(StoreErr::parse(p)),
            p => Ok(Some(p.clone())),
        }),
        approval: v.get("approval").map(|a| match a {
            Value::Null => Ok(None),
            a if failed(a) => Err(()),
            a => Ok(Some(a.clone())),
        }),
        current: v.get("current").map(|a| match a {
            Value::Null => Ok(None),
            a if failed(a) => Err(StoreErr::parse(a)),
            a => Ok(Some(a.clone())),
        }),
        question: v.get("question").map(|q| {
            if failed(q) {
                Err(StoreErr::parse(q))
            } else {
                Ok(q.clone())
            }
        }),
        view: v.get("view").cloned(),
        history: v.get("history").map(|h| match h {
            h if failed(h) => Err(()),
            h => Ok(h.as_array().cloned().unwrap_or_default()),
        }),
        request: v.get("request").cloned(),
        refresh_said: v
            .get("refresh_said")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

fn cycles(v: Option<&Value>) -> Result<BTreeMap<u64, Cycle>, StoreErr> {
    let n = |r: &Value, k: &str| r.get(k).and_then(Value::as_u64).unwrap_or(0);
    Ok(v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|r| {
                    let c = Cycle {
                        took: n(r, "took"),
                        heard: n(r, "heard"),
                        said_for: r
                            .get("said_for")
                            .and_then(Value::as_str)
                            .unwrap_or("-")
                            .to_string(),
                        quiet_at: n(r, "quiet_at"),
                        asked: n(r, "asked"),
                        times: n(r, "times"),
                    };
                    Some((r.get("issue").and_then(number)?, c))
                })
                .collect()
        })
        .unwrap_or_default())
}

impl Snapshot {
    pub fn parse(v: &Value) -> Snapshot {
        let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let control = v.get("control").cloned().unwrap_or(Value::Null);
        Snapshot {
            repo: v.get("repo").and_then(Value::as_str).map(str::to_string),
            user: v.get("user").and_then(opt),
            pause_flag: match v.get("pause_flag") {
                None | Some(Value::Null) => None,
                Some(Value::String(t)) if t == "unreadable" => Some(Value::Null),
                Some(f) => Some(f.clone()),
            },
            control_labels: control
                .get("labels")
                .filter(|l| !failed(l))
                .map(|l| names(Some(l))),
            control_events: control.get("events").map(Events::parse),
            plan_open: match v.get("plan_open") {
                Some(Value::Array(a)) => Ok(a.iter().filter_map(number).collect()),
                Some(Value::Object(e)) => Err((
                    e.get("word")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    e.get("rc").and_then(Value::as_i64).unwrap_or(1) as i32,
                )),
                _ => Ok(Vec::new()),
            },
            running: v
                .get("running")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|r| {
                            let how = r
                                .get("how")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            Some((r.get("issue").and_then(number)?, how))
                        })
                        .collect()
                })
                .unwrap_or_default(),
            ready: issues(v.get("ready")),
            blocked: issues(v.get("blocked")),
            awaiting: v.get("awaiting").map(|a| issues(Some(a))),
            limit: v
                .get("limit")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({"state": "none"})),
            slice: reading(v.get("slice").unwrap_or(&Value::Null)),
            endpoint_hold: v
                .get("endpoint")
                .and_then(|e| e.get("hold"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            endpoint: v
                .get("endpoint")
                .and_then(|e| e.get("end"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            endpoint_log: v
                .get("endpoint")
                .and_then(|e| e.get("log"))
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            disk_kb: v.get("disk_kb").map(Value::as_u64),
            checks: v.get("checks").map(opt),
            branch_note: s("branch_note"),
            last_why: s("last_why"),
            gate: v.get("gate").map(|g| Gate {
                answer: g.get("answer").and_then(Value::as_i64).unwrap_or(0) as i32,
                held: g
                    .get("held")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            }),
            refresh_requests: v
                .get("refresh_requests")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(number).collect())
                .unwrap_or_default(),
            refresh_held: s("refresh_held"),
            continues: v.get("continues").and_then(Value::as_bool),
            pulls: v
                .get("pulls")
                .and_then(Value::as_object)
                .map(|m| {
                    m.iter()
                        .filter_map(|(k, p)| Some((k.parse().ok()?, p.clone())))
                        .collect()
                })
                .unwrap_or_default(),
            asking: v.get("asking").cloned(),
            owed: v.get("owed").map(|o| {
                if failed(o) {
                    Err(StoreErr::parse(o))
                } else {
                    Ok(o.as_array().cloned().unwrap_or_default())
                }
            }),
            images: v.get("images").map(|i| Images {
                rukas: i.get("rukas").and_then(Value::as_str).unwrap_or("").into(),
                commit: i.get("commit").and_then(Value::as_str).unwrap_or("").into(),
                declares: i.get("declares").and_then(Value::as_bool) == Some(true),
                answers: i.get("answers").and_then(Value::as_bool) == Some(true),
            }),
            now: v.get("now").and_then(Value::as_u64),
            hostname: v
                .get("hostname")
                .and_then(Value::as_str)
                .map(str::to_string),
            cycles: match v.get("cycles_failed") {
                Some(e) => Err(StoreErr::parse(e)),
                None => cycles(v.get("running")),
            },
            bot_prs: v.get("bot_prs").cloned(),
            marked: v.get("marked").cloned(),
            refresh_history: v.get("refresh_history").map(|h| match h {
                e if failed(e) => Err(StoreErr::parse(e)),
                h => Ok(h
                    .as_array()
                    .map(|a| a.iter().filter_map(number).collect())
                    .unwrap_or_default()),
            }),
            refreshing: v.get("refreshing").cloned(),
            leftovers: v.get("leftovers").and_then(Value::as_array).map(|a| {
                a.iter()
                    .filter_map(|l| {
                        Some(Leftover {
                            issue: l.get("issue").and_then(number)?,
                            young: l.get("young").and_then(Value::as_bool) == Some(true),
                            lock_ok: l.get("lock_ok").and_then(Value::as_bool) != Some(false),
                        })
                    })
                    .collect()
            }),
            issues: v
                .get("issues")
                .and_then(Value::as_object)
                .map(|m| {
                    m.iter()
                        .filter_map(|(k, i)| Some((k.parse().ok()?, issue(i))))
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}
