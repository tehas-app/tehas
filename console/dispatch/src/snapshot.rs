//! What one poll read: from GitHub, the state store and the host. Built from
//! JSON in the shapes `gh`, the store and the files answer in. A field that a
//! poll reads only when it needs it is None until it is read.

use crate::slice::{Reading, Scope};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use tehas_core::events::Events;

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
    /// The pull requests that close it; None when the call failed or the
    /// answer cannot be read.
    pub prs: Option<Vec<ClosingPr>>,
    /// The issue's last 100 comments, for tehas:blocked and a park's lift; None
    /// when the call failed or was not asked.
    pub comments: Option<Value>,
    /// The line of the gh call the comments read failed on; None when no gh
    /// call failed, or none was asked.
    pub comments_failed: Option<String>,
    /// The states of the issues its blocker comment names, by number: OPEN,
    /// CLOSED or MERGED. A number missing is one GitHub gave no state for.
    pub blockers: BTreeMap<u64, String>,
    /// The issue's park on the state store (park-show's .park), or the store's
    /// word and exit status when it could not say.
    pub park: Option<Result<Option<Value>, StoreErr>>,
    /// The approval of its latest start (approval-view), Err when the
    /// store could not answer.
    pub approval: Option<Result<Option<Value>, ()>>,
    /// approval-current's .current, for the owner of a refresh.
    pub current: Option<Result<Option<Value>, StoreErr>>,
    /// The question on record: {plan, id, kind}. plan is plan-show's
    /// `.planning` of an open session, null for none; id and kind as
    /// park-show and question-known answer for it.
    pub question: Option<Result<Value, StoreErr>>,
    /// The answer `tehas answer` gave to the question on record, as
    /// `question-known` has it; None for none, and for one that does not
    /// parse.
    pub answer: Option<crate::answer::Answer>,
    /// gh issue view --json state,labels, for a retry after an outage.
    pub view: Option<Value>,
    /// The cycle history on record, Err when the store
    /// could not answer.
    pub history: Option<Result<Vec<Value>, ()>>,
    /// state/refresh/<n>.json, as written; and what <n>.said holds.
    pub request: Option<Value>,
    pub refresh_said: Option<String>,
    /// state/plan-start/<n>.json: what lets the bot's tehas:ready start this
    /// one issue. Null when it is there and cannot be read.
    pub plan_start: Option<Value>,
}

/// A pull request that closes an issue. A url GitHub did not give is
/// empty, and no address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosingPr {
    pub url: String,
    pub open: bool,
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
    /// One of its names, or the lock, was used within TEHAS_WORKSPACE_KEEP_DAYS
    /// days, or cannot be dated.
    pub young: bool,
    /// The issue is not among the open ones the queue listed.
    pub closed: bool,
    /// The lock is a lock file, or there is none.
    pub lock_ok: bool,
}

/// An open planning session.
#[derive(Clone, Debug, Default)]
pub struct Session {
    pub issue: u64,
    pub running: bool,
    pub pending: bool,
    pub in_flight: bool,
    pub rc: bool,
}

/// A call to the state store that failed: its word, exit status and first
/// line, as tehas_core::call keeps them.
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

/// A row of park-show --automatic or --refine. `*_said` is the raw value as
/// JSON text, `null` when it is missing, for the line that says the row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwedPark {
    pub issue: Option<u64>,
    pub issue_said: String,
    pub started_at: Option<u64>,
    pub started_said: String,
    pub started_by: ParkField,
    pub park_reason: ParkField,
    pub question: ParkField,
    pub body_sha256: ParkField,
}

/// A text field of a park row: absent, null or false; a string; or any other
/// value, as its JSON text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParkField {
    Missing,
    Text(String),
    Other(String),
}

impl ParkField {
    /// The field as a line says it: `-` for none.
    pub fn dash(&self) -> String {
        match self {
            ParkField::Missing => "-".into(),
            ParkField::Text(s) | ParkField::Other(s) => s.clone(),
        }
    }

    /// The field when it is a string, empty otherwise.
    pub fn text_or_empty(&self) -> &str {
        match self {
            ParkField::Text(s) => s,
            _ => "",
        }
    }
}

/// What the poll reads of a check's runs on the branch's head. The words are
/// the set the store pins in console/store/src/schema.sql
/// (`red`|`done`|`pending`|`none`|`unreadable`), so this enum is the contract
/// in both places.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Check {
    /// A run says it failed, timed out or could not start.
    Red(String),
    /// Every run says it passed.
    Done(String),
    /// A run has not finished.
    Pending(String),
    /// The branch's head has no run of that check at all.
    None,
    /// Every run finished and none says it passed: the poll cannot tell what
    /// ended the gate, so nothing starts on it unsaid. Its sha is the head the
    /// hold names.
    Unknown(String),
    /// The answer is no answer: no list of runs, or a head sha that is not one.
    Unreadable,
}

/// The state of a branch's check runs, as the bash dispatcher read them. Of the
/// conclusions GitHub answers, these three pass: `success`, `neutral` and
/// `skipped`; these three are red: `failure`, `timed_out` and
/// `startup_failure`. Every other conclusion, and a finished run that names
/// none, is unknown: the gate holds. A conclusion GitHub adds later is a
/// deliberate edit to one of the three lists beside this function, never a
/// run that reads as green.
pub(crate) fn check_state(a: &Value) -> Option<Check> {
    let runs = a.get("check_runs")?.as_array()?;
    if runs.is_empty() {
        return Some(Check::None);
    }
    let sha_ok = |r: &Value| {
        r.get("head_sha")
            .and_then(Value::as_str)
            .is_some_and(tehas_core::grammar::commit_id)
    };
    if !runs.iter().all(sha_ok) {
        return None;
    }
    let sha = &runs[0]["head_sha"].as_str()?[..12];
    // One walk of the runs: which of the four readings hold.
    let mut any_red = false;
    let mut all_done = true;
    let mut all_passed = true;
    for r in runs {
        let finished = r.get("status").and_then(Value::as_str) == Some("completed");
        let conclusion = r.get("conclusion").and_then(Value::as_str);
        all_done &= finished;
        if finished
            && matches!(
                conclusion,
                Some("failure" | "timed_out" | "startup_failure")
            )
        {
            any_red = true;
        }
        all_passed &= finished && matches!(conclusion, Some("success" | "neutral" | "skipped"));
    }
    if any_red {
        Some(Check::Red(sha.into()))
    } else if all_passed {
        Some(Check::Done(sha.into()))
    } else if all_done {
        Some(Check::Unknown(sha.into()))
    } else {
        Some(Check::Pending(sha.into()))
    }
}

/// A sandbox that cannot push, as `state/push-fault.json` has it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushFault {
    /// What the hold says of it: [`push_fault_line`].
    pub line: String,
    /// The account it is on record for; empty when the record names none.
    pub login: String,
}

/// The fault's line as the poll says it: git's words when the file has them,
/// the one line of the host's own for a fault that says nothing.
pub fn push_fault_line(fault: &Value) -> String {
    let line = match fault {
        Value::Object(_) => fault.get("line").and_then(Value::as_str).unwrap_or(""),
        _ => "",
    };
    if line.is_empty() {
        "the sandbox cannot push to origin".into()
    } else {
        line.to_string()
    }
}

/// Whether a push fault holds the poll's starts: any fault on record. Past
/// `PUSH_FAULT_HOLD` of read::host the poll has asked the sandbox before the rules read
/// the record, so what stands is a push still refused. A fault with no
/// readable time holds too: nothing about the host has been proven.
pub fn push_fault_holds(fault: &Value) -> bool {
    !matches!(fault, Value::Null)
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

/// tehas pause's flag, when one is there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PauseFlag {
    /// The flag holds an `at` and a `reason`, both text.
    Held { at: String },
    /// The flag is there and cannot be read as one.
    Unreadable,
}

/// The limit as `flags limit read` reads it. An absent limit is the default.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Limit {
    pub state: LimitState,
    /// The cycles it allows at once; 0 when it gives no whole number.
    pub cycles: i64,
    /// The plan is off: `false`, or the text of it.
    pub plan_off: bool,
    pub until: Until,
    pub reason: String,
}

/// A state the dispatcher does not know reads as `None`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LimitState {
    #[default]
    None,
    Ok,
    Expired,
    /// The state is missing or not text.
    Unreadable,
}

/// When a limit ends.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Until {
    /// Missing or null.
    #[default]
    Unset,
    At(String),
    /// Something that is neither text nor null.
    Other,
}

/// The start order as `tehas order` reads it, read leniently: an unknown
/// state is `Other`, an issue that is not a number is dropped.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StartOrder {
    pub state: OrderState,
    /// Its issues, in its order.
    pub issues: Vec<u64>,
    /// It holds every other ready issue.
    pub only: bool,
    pub until: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OrderState {
    Ok,
    Expired,
    Unreadable,
    /// none, missing, or a state the dispatcher does not know.
    #[default]
    Other,
}

/// An approval `tehas start` recorded that no cycle has used yet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostStart {
    pub issue: Option<u64>,
    /// When it was given, in seconds.
    pub at: Option<u64>,
    pub body_sha256: Option<String>,
    /// None binds no title.
    pub title_sha256: Option<String>,
    /// The issue's labels.
    pub labels: Vec<String>,
}

/// An issue of a list the poll reads strictly: a number that is not a whole
/// number in the JSON is None, and the decision says what that costs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asked {
    pub number: Option<u64>,
    pub labels: Vec<String>,
}

/// The issues to plan or refine: a list, or an answer that is not one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanList {
    Read(Vec<Asked>),
    Unreadable,
}

/// The issues marked tehas:running, as gh issue list answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Marked {
    /// The call failed.
    Failed,
    /// No list, or an issue in it without a whole number.
    Unreadable,
    /// In the order GitHub gave them.
    Listed(Vec<MarkedIssue>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkedIssue {
    pub number: u64,
    pub closed: bool,
}

/// A pull request of the bot's. A field GitHub did not give as text is empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BotPr {
    pub url: String,
    pub state: String,
    /// The branch it is from.
    pub head: String,
    pub created: String,
}

impl BotPr {
    /// A pull request of issue `n`'s own branch, `feature/issue-<n>` or
    /// `feature/issue-<n>-*`. Its body does not count: any cycle can write
    /// `Fixes #<n>` in a pull request of its own.
    pub fn own_branch(&self, n: u64) -> bool {
        let branch = format!("feature/issue-{n}");
        self.head == branch || self.head.starts_with(&format!("{branch}-"))
    }
}

/// [`BotPr::own_branch`] of a pull request as GitHub gave it.
#[cfg(test)]
pub(crate) fn own_branch(pr: &Value, n: u64) -> bool {
    let s = Snapshot::parse(&serde_json::json!({ "bot_prs": [pr] }));
    s.bot_prs
        .flatten()
        .unwrap_or_default()
        .first()
        .is_some_and(|p| p.own_branch(n))
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    /// nameWithOwner, None when `gh repo view` failed.
    pub repo: Option<String>,
    /// tehas pause's flag: None when there is none.
    pub pause_flag: Option<PauseFlag>,
    /// The control issue's labels, None when the call failed.
    pub control_labels: Option<Vec<String>>,
    pub control_events: Option<Events>,
    /// With TEHAS_ISSUE_TYPES mapping improvement: the issue types of the
    /// repository's owner, None when the page did not say.
    pub issue_types: Option<Vec<String>>,
    /// The issues with a planning session open, or the store's word and exit
    /// status when it could not say.
    pub plan_open: Result<Vec<u64>, (String, i32)>,
    /// The open sessions as plan-show --open lists them, a number or
    /// {issue, running, pending}, with what the host says of a running turn:
    /// its unit runs or holds the lock (in_flight), it left an exit status
    /// (rc).
    pub sessions: Vec<Session>,
    /// gh issue list --label tehas:plan and --label tehas:refine.
    pub plans: Option<PlanList>,
    pub refines: Option<PlanList>,
    /// lib/refine-brief.md is there and not empty.
    pub refine_brief: bool,
    /// park-show --refine's parks, owed a refine.
    pub refines_owed: Option<Result<Vec<OwedPark>, StoreErr>>,
    /// The issues whose cycle holds its lock, with how each started on record.
    pub running: Vec<(u64, String)>,
    /// state/cycles is there but could not be listed: `running` is empty and
    /// any cycle may be running.
    pub running_unlisted: bool,
    pub ready: Issues,
    pub blocked: Issues,
    pub awaiting: Option<Issues>,
    /// The limit as `flags limit read` reads it.
    pub limit: Limit,
    /// The start order as `tehas order` reads it.
    pub order: StartOrder,
    /// The open issues `tehas start` approved that no cycle has used yet, as
    /// `store approval-hosts` lists them with each issue's labels. None when
    /// the list could not be read.
    pub host_starts: Option<Vec<HostStart>>,
    pub slice: Reading,
    /// What the model endpoints answered: the hold, when both failed, and
    /// which endpoint the subagent provider is on.
    pub endpoint_hold: String,
    pub endpoint: String,
    /// What asking them logged: a provider on its backup.
    pub endpoint_log: Vec<String>,
    /// The free kilobytes of the disk, Some(None) when they cannot be read.
    pub disk_kb: Option<Option<u64>>,
    /// The state of TEHAS_BRANCH_CHECK's runs on the branch's head;
    /// Unreadable when the call failed.
    pub checks: Option<Check>,
    /// What state/branch-check says the last poll logged.
    pub branch_note: String,
    /// The fault of a sandbox that cannot push, `state/push-fault.json`: None
    /// when there is none. One that cannot be read has the host's own line
    /// and no login. Read by the same poll that writes it (Part 1 of this
    /// issue's series).
    pub push_fault: Option<PushFault>,
    /// The why of the last live/dispatch.json.
    pub last_why: String,
    /// The issues with a refresh request (state/refresh/<n>.json), lowest
    /// first.
    pub refresh_requests: Vec<u64>,
    /// With TEHAS_READ_ONLY: the issues a start of this host is on record for
    /// (state/read-only/<n>.json), lowest first.
    pub read_only_ran: Vec<u64>,
    /// What state/refresh/.held says, and whether the sandbox's pi-rukas takes
    /// --continue-pr (asked when a refresh may start).
    pub refresh_held: String,
    pub continues: Option<bool>,
    /// The pull requests a refresh reads, by number: GraphQL's
    /// .data.repository.pullRequest, or {"failed": true}.
    pub pulls: BTreeMap<u64, Value>,
    /// With TEHAS_MILLWRIGHT_FEED: the running cycles a fed start began, None
    /// when that could not be read.
    pub fed: Option<u64>,
    /// gh issue list --label tehas:question; Some(None) when it gave no list.
    pub asking: Option<Option<Vec<Asked>>>,
    /// park-show --automatic's parks, owed a retry after an outage.
    pub owed: Option<Result<Vec<OwedPark>, StoreErr>>,
    pub images: Option<Images>,
    pub issues: BTreeMap<u64, Issue>,
    /// The open issues whose title starts with the epic marker and that do not
    /// carry `epic`, the control issue left out.
    pub epic_titled: Vec<u64>,
    /// The poll's clock, in seconds, and this host's name.
    pub now: Option<u64>,
    pub hostname: Option<String>,
    /// The bot's user id recorded in state/account-ids, which names its
    /// comments when its login changed.
    pub bot_id: Option<u64>,
    /// The running cycles' records, by issue, from the entries of `running`;
    /// Err when the store could not list them (`cycles_failed`).
    pub cycles: Result<BTreeMap<u64, Cycle>, StoreErr>,
    /// gh pr list --author <bot>: asked when a cycle runs. Some(None) when
    /// it gave no list; an entry that is no object is left out.
    pub bot_prs: Option<Option<Vec<BotPr>>>,
    /// gh issue list --label tehas:running, as it answered.
    pub marked: Option<Marked>,
    /// The open issues labelled tehas:planning.
    pub planning: Option<Vec<u64>>,
    /// The issues with a refresh in their history.
    pub refresh_history: Option<Result<Vec<u64>, StoreErr>>,
    /// gh pr list --label tehas:refreshing: the numbers of those that carry
    /// it, Some(None) when it gave no list.
    pub refreshing: Option<Option<Vec<u64>>>,
    /// The open pull requests whose body names each open issue with a closing
    /// keyword, by issue: `pulls_name` of the snapshot, from the page's own
    /// read of the bodies. GitHub links a closing keyword only for a pull
    /// request into the default branch, so this is how a stacked one holds its
    /// issue. A number the page named nobody is absent, and so is one whose
    /// list of pull requests held nothing a reader could use: an entry of no
    /// numbers would hold an issue back on nothing.
    pub pulls_name: BTreeMap<u64, Vec<u64>>,
    pub leftovers: Option<Vec<Leftover>>,
    /// What the outsource feature reads, present only while it is on.
    pub outsource: Option<crate::place::Outsourcing>,
}

fn failed(v: &Value) -> bool {
    v.get("failed").and_then(Value::as_bool) == Some(true)
}

/// The line a failed read's marker holds of its gh call, as
/// `read::github::failed_read` writes it.
pub fn failed_line(v: Option<&Value>) -> Option<String> {
    v.filter(|v| failed(v))
        .and_then(|v| v.get("line"))
        .and_then(Value::as_str)
        .map(str::to_string)
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

pub(crate) fn reading(v: &Value) -> Reading {
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
            .map(|a| {
                a.iter()
                    .map(|p| ClosingPr {
                        url: p
                            .get("url")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        open: p.get("state").and_then(Value::as_str) == Some("OPEN"),
                    })
                    .collect()
            }),
        comments: v.get("comments").and_then(opt),
        comments_failed: failed_line(v.get("comments")),
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
        answer: v.pointer("/question/answer").and_then(|a| {
            let s = |k: &str| a.get(k).and_then(Value::as_str);
            crate::answer::Answer::new(
                a.get("question")?.as_u64()?,
                s("kind")?,
                s("choice")?,
                a.get("option").and_then(Value::as_u64),
                s("text"),
                s("by")?,
                a.get("at")?.as_i64()?,
            )
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
        plan_start: v.get("plan_start").cloned(),
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
        let owed = |o: &Value| -> Result<Vec<OwedPark>, StoreErr> {
            if failed(o) {
                return Err(StoreErr::parse(o));
            }
            let field = |p: &Value, k: &str| match p.get(k) {
                None | Some(Value::Null) | Some(Value::Bool(false)) => ParkField::Missing,
                Some(Value::String(s)) => ParkField::Text(s.clone()),
                Some(x) => ParkField::Other(x.to_string()),
            };
            let said = |p: &Value, k: &str| p.get(k).unwrap_or(&Value::Null).to_string();
            Ok(o.as_array()
                .map(|a| {
                    a.iter()
                        .map(|p| OwedPark {
                            issue: p.get("issue").and_then(Value::as_u64),
                            issue_said: said(p, "issue"),
                            started_at: p.get("started_at").and_then(Value::as_u64),
                            started_said: said(p, "started_at"),
                            started_by: field(p, "started_by"),
                            park_reason: field(p, "park_reason"),
                            question: field(p, "question"),
                            body_sha256: field(p, "body_sha256"),
                        })
                        .collect()
                })
                .unwrap_or_default())
        };
        let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let control = v.get("control").cloned().unwrap_or(Value::Null);
        let asked = |i: &Value| Asked {
            number: i.get("number").and_then(Value::as_u64),
            labels: names(i.get("labels")),
        };
        let plan_list = |l: &Value| match l.as_array() {
            Some(a) => PlanList::Read(a.iter().map(asked).collect()),
            None => PlanList::Unreadable,
        };
        let marked = |m: &Value| {
            if m.get("failed") == Some(&Value::Bool(true)) {
                return Marked::Failed;
            }
            let Some(a) = m.as_array() else {
                return Marked::Unreadable;
            };
            let list: Option<Vec<MarkedIssue>> = a
                .iter()
                .map(|i| {
                    Some(MarkedIssue {
                        number: i.get("number").and_then(Value::as_u64)?,
                        closed: i.get("state").and_then(Value::as_str) == Some("CLOSED"),
                    })
                })
                .collect();
            list.map_or(Marked::Unreadable, Marked::Listed)
        };
        // The pull requests that carry tehas:refreshing and a whole number,
        // in the order given.
        let refreshing = |r: &Value| {
            r.as_array().map(|a| {
                a.iter()
                    .filter(|p| {
                        names(p.get("labels"))
                            .iter()
                            .any(|l| l == "tehas:refreshing")
                    })
                    .filter_map(|p| p.get("number").and_then(Value::as_u64))
                    .collect()
            })
        };
        let bot_pr = |p: &Value| {
            let s = |k: &str| p.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            BotPr {
                url: s("url"),
                state: s("state"),
                head: s("headRefName"),
                created: s("createdAt"),
            }
        };
        Snapshot {
            repo: v.get("repo").and_then(Value::as_str).map(str::to_string),
            pause_flag: v.get("pause_flag").filter(|f| !f.is_null()).map(|f| {
                let text = |k: &str| f.get(k).and_then(Value::as_str);
                match (text("at"), text("reason")) {
                    (Some(at), Some(_)) => PauseFlag::Held { at: at.into() },
                    _ => PauseFlag::Unreadable,
                }
            }),
            control_labels: control
                .get("labels")
                .filter(|l| !failed(l))
                .map(|l| names(Some(l))),
            control_events: control.get("events").map(Events::parse),
            issue_types: v.get("issue_types").and_then(Value::as_array).map(|a| {
                a.iter()
                    .filter_map(|t| t.as_str().map(str::to_string))
                    .collect()
            }),
            plan_open: match v.get("plan_open") {
                Some(Value::Array(a)) => Ok(a
                    .iter()
                    .filter_map(|x| number(x).or_else(|| x.get("issue").and_then(number)))
                    .collect()),
                Some(Value::Object(e)) => Err((
                    e.get("word")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    e.get("rc").and_then(Value::as_i64).unwrap_or(1) as i32,
                )),
                _ => Ok(Vec::new()),
            },
            sessions: v
                .get("plan_open")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|x| {
                            let b = |k: &str| x.get(k).and_then(Value::as_bool) == Some(true);
                            Some(Session {
                                issue: number(x).or_else(|| x.get("issue").and_then(number))?,
                                running: b("running"),
                                pending: b("pending"),
                                in_flight: b("in_flight"),
                                rc: b("rc"),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default(),
            plans: v.get("plans").map(plan_list),
            refines: v.get("refines").map(plan_list),
            refine_brief: v.get("refine_brief").and_then(Value::as_bool) != Some(false),
            refines_owed: v.get("refines_owed").map(owed),
            running_unlisted: v.get("running_unlisted") == Some(&Value::Bool(true)),
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
                .map(|l| Limit {
                    state: match l.get("state").and_then(Value::as_str) {
                        None => LimitState::Unreadable,
                        Some("ok") => LimitState::Ok,
                        Some("expired") => LimitState::Expired,
                        Some("unreadable") => LimitState::Unreadable,
                        Some(_) => LimitState::None,
                    },
                    cycles: l.get("cycles").and_then(Value::as_i64).unwrap_or(0),
                    plan_off: matches!(l.get("plan"), Some(Value::Bool(false)))
                        || l.get("plan").and_then(Value::as_str) == Some("false"),
                    until: match l.get("until") {
                        None | Some(Value::Null) => Until::Unset,
                        Some(Value::String(t)) => Until::At(t.clone()),
                        Some(_) => Until::Other,
                    },
                    reason: l
                        .get("reason")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                })
                .unwrap_or_default(),
            order: v
                .get("order")
                .map(|o| StartOrder {
                    state: match o.get("state").and_then(Value::as_str) {
                        Some("ok") => OrderState::Ok,
                        Some("expired") => OrderState::Expired,
                        Some("unreadable") => OrderState::Unreadable,
                        _ => OrderState::Other,
                    },
                    issues: o
                        .get("issues")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_u64)
                        .collect(),
                    only: o.get("only").and_then(Value::as_bool) == Some(true),
                    until: o.get("until").and_then(Value::as_str).map(str::to_string),
                })
                .unwrap_or_default(),
            host_starts: match v.get("host_starts") {
                None => Some(Vec::new()),
                Some(Value::Array(a)) => Some(
                    a.iter()
                        .map(|h| {
                            let text =
                                |k: &str| h.get(k).and_then(Value::as_str).map(str::to_string);
                            HostStart {
                                issue: h.get("issue").and_then(Value::as_u64),
                                at: h.get("at").and_then(Value::as_u64),
                                body_sha256: text("body_sha256"),
                                title_sha256: text("title_sha256"),
                                labels: h
                                    .get("labels")
                                    .and_then(Value::as_array)
                                    .into_iter()
                                    .flatten()
                                    .filter_map(|l| l.get("name")?.as_str().map(str::to_string))
                                    .collect(),
                            }
                        })
                        .collect(),
                ),
                Some(_) => None,
            },
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
            checks: v.get("checks").map(|c| {
                opt(c)
                    .as_ref()
                    .and_then(check_state)
                    .unwrap_or(Check::Unreadable)
            }),
            branch_note: s("branch_note"),
            push_fault: match v.get("push_fault") {
                None | Some(Value::Null) => None,
                Some(f) => Some(PushFault {
                    line: push_fault_line(f),
                    login: f
                        .get("login")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                }),
            },
            last_why: s("last_why"),
            refresh_requests: v
                .get("refresh_requests")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(number).collect())
                .unwrap_or_default(),
            read_only_ran: v
                .get("read_only_ran")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(number).collect())
                .unwrap_or_default(),
            refresh_held: s("refresh_held"),
            epic_titled: v
                .get("epic_titled")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(number).collect())
                .unwrap_or_default(),
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
            fed: v.get("fed").and_then(Value::as_u64),
            asking: v
                .get("asking")
                .map(|a| a.as_array().map(|a| a.iter().map(asked).collect())),
            owed: v.get("owed").map(owed),
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
            bot_id: v.get("bot_id").and_then(Value::as_u64),
            cycles: match v.get("cycles_failed") {
                Some(e) => Err(StoreErr::parse(e)),
                None => cycles(v.get("running")),
            },
            bot_prs: v.get("bot_prs").map(|b| {
                b.as_array()
                    .map(|a| a.iter().filter(|p| p.is_object()).map(bot_pr).collect())
            }),
            marked: v.get("marked").map(marked),
            planning: v
                .get("planning")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(number).collect()),
            refresh_history: v.get("refresh_history").map(|h| match h {
                e if failed(e) => Err(StoreErr::parse(e)),
                h => Ok(h
                    .as_array()
                    .map(|a| a.iter().filter_map(number).collect())
                    .unwrap_or_default()),
            }),
            refreshing: v.get("refreshing").map(refreshing),
            pulls_name: v
                .get("pulls_name")
                .and_then(Value::as_object)
                .map(|m| {
                    m.iter()
                        .filter_map(|(k, hits)| {
                            let n = number(&Value::String(k.clone()))?;
                            let hits: Vec<u64> = hits
                                .as_array()
                                .map(|a| a.iter().filter_map(number).collect())
                                .unwrap_or_default();
                            (!hits.is_empty()).then_some((n, hits))
                        })
                        .collect()
                })
                .unwrap_or_default(),
            leftovers: v.get("leftovers").and_then(Value::as_array).map(|a| {
                a.iter()
                    .filter_map(|l| {
                        Some(Leftover {
                            issue: l.get("issue").and_then(number)?,
                            young: l.get("young").and_then(Value::as_bool) == Some(true),
                            closed: l.get("closed").and_then(Value::as_bool) == Some(true),
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
            outsource: v.get("outsource").map(crate::place::Outsourcing::parse),
        }
    }
}
