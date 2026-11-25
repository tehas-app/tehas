//! What one poll read: from GitHub, the state store and the host. Built from
//! JSON in the shapes `gh`, the store and the files answer in. A field that a
//! poll reads only when it needs it is None until it is read.

use crate::decide::HOST;
use crate::settings::Settings;
use crate::slice::{Probed, Reading, Scope};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use tehas_core::driver::Driver;
use tehas_core::events::{EditRecord, Events};
use tehas_core::grammar::MILLWRIGHT;

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
    /// title. None when the call failed or the answer is no object.
    pub edit: Option<EditRecord>,
    /// `gh api repos/<repo>/issues/<n>`, or None when the call failed.
    pub work: Option<Work>,
    /// The pull requests that close it; None when the call failed or the
    /// answer cannot be read.
    pub prs: Option<Vec<ClosingPr>>,
    /// The issue's last 100 comments, for tehas:blocked and a park's lift; None
    /// when the call failed or was not asked.
    pub comments: Option<Comments>,
    /// The line of the gh call the comments read failed on; None when no gh
    /// call failed, or none was asked.
    pub comments_failed: Option<String>,
    /// The states of the issues its blocker comment names, by number: OPEN,
    /// CLOSED or MERGED. A number missing is one GitHub gave no state for.
    pub blockers: BTreeMap<u64, String>,
    /// The issue's park on the state store (park-show's .park), or the store's
    /// word and exit status when it could not say. A park that is not an
    /// object is none.
    pub park: Option<Result<Option<Park>, StoreErr>>,
    /// The approval of its latest start (approval-view), Err when the
    /// store could not answer.
    pub approval: Option<Result<Option<Approval>, ()>>,
    /// The pull request approval-current's .current names as its refresh's:
    /// its refresh_pr when its how is refresh and that is a number of 1 to 10
    /// digits with no leading 0, None otherwise.
    pub current: Option<Result<Option<u64>, StoreErr>>,
    /// The question on record: {plan, id, kind}. plan is plan-show's
    /// `.planning` of an open session, null for none; id and kind as
    /// park-show and question-known answer for it.
    pub question: Option<Result<Question, StoreErr>>,
    /// The answer `tehas answer` gave to the question on record, as
    /// `question-known` has it; None for none, and for one that does not
    /// parse.
    pub answer: Option<crate::answer::Answer>,
    /// gh issue view --json state,labels,title,body, for a retry after an
    /// outage and a planning session's brief.
    pub view: Option<View>,
    /// The cycle history on record, Err when the store
    /// could not answer.
    pub history: Option<Result<Vec<CycleRow>, ()>>,
    /// state/refresh/<n>.json; and what <n>.said holds.
    pub request: Option<RefreshRequest>,
    pub refresh_said: Option<String>,
    /// state/plan-start/<n>.json: what lets the bot's tehas:ready start this
    /// one issue.
    pub plan_start: Option<PlanStart>,
}

/// A row of the cycle history, as the park rule of a refresh reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CycleRow {
    /// Its how is refresh.
    pub refresh: bool,
    /// Its parked is JSON true.
    pub parked: bool,
    /// Its start in seconds; None when it is missing, null or no u64.
    pub started: Option<u64>,
}

/// A refresh request. `at` is read from any request, `target` only from one
/// whose pr is a JSON number and whose shas are commit ids.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefreshRequest {
    pub at: String,
    pub target: Option<RefreshTarget>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefreshTarget {
    /// The pr as a u64, 0 for a number that is not one.
    pub pr: u64,
    pub head_sha: String,
    pub base_sha: String,
}

/// The record of a plan's start. Unreadable is a file that is there and is
/// not a JSON object: it still makes the bot's label a plan's, and so refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanStart {
    /// by is empty when it is not text; sum is None when it is not text.
    Record {
        by: String,
        sum: Option<String>,
    },
    Unreadable,
}

/// The issue as `gh issue view` gave it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum View {
    Failed,
    Read(ViewRead),
}

/// A text GitHub did not give as text is None; a label with no name is left
/// out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ViewRead {
    pub state: Option<String>,
    pub labels: Vec<String>,
    pub title: Option<String>,
    pub body: Option<String>,
}

/// The REST answer for one issue. Unreadable is an answer that is no
/// object: null, a list or a text.
#[derive(Clone, Debug, PartialEq)]
pub enum Work {
    Unreadable,
    Issue(Box<WorkIssue>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorkIssue {
    /// The state as GitHub wrote it; None when it is not text.
    pub state: Option<String>,
    /// pull_request is there and not null.
    pub is_pull: bool,
    pub title: Option<String>,
    pub body: Text,
    pub labels: Labels,
    pub sub_issues: Option<SubIssues>,
    /// parent as a u64.
    pub parent: Option<u64>,
    /// issue_dependencies_summary.blocked_by; 0 when missing or no u64.
    pub blocked_by: u64,
    /// Every text of the answer, in the order its keys sort, for
    /// text::promise_of.
    pub strings: Vec<String>,
}

pub use tehas_core::events::Text;

/// The labels of an answer. Names skips an entry with no text name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Labels {
    Absent,
    Null,
    Names(Vec<String>),
    Unreadable,
}

impl Labels {
    /// The names, or none when it is no list.
    pub fn names(&self) -> &[String] {
        match self {
            Labels::Names(n) => n,
            _ => &[],
        }
    }
}

/// sub_issues_summary's counts.
#[derive(Clone, Debug, PartialEq)]
pub struct SubIssues {
    pub total: Count,
    pub completed: Count,
}

/// A JSON number read both ways: as f64 and as u64.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Count {
    pub f: Option<f64>,
    pub u: Option<u64>,
}

/// The park on record of an issue's last cycle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Park {
    /// Empty when it is not text.
    pub body_sha256: String,
    /// As jq's `tostring` gives it: `null` for none, text as it is, any other
    /// value as its JSON text.
    pub reason: String,
    /// Its retries are 0.
    pub fresh: bool,
    /// The question asked of it: text that is not empty, or a number's text.
    pub question: Option<String>,
    /// Its told_at is there and not null.
    pub told: bool,
}

/// The question on record of an issue. kind and id are empty when the store
/// gave none as text; an id that is a number is its text.
#[derive(Clone, Debug, PartialEq)]
pub struct Question {
    pub id: String,
    pub kind: String,
    /// The park's reason, when it is text.
    pub reason: Option<String>,
    pub plan: PlanRow,
    /// Asked before 2.0 and open at the upgrade: asked again in 2.0's words.
    pub reask: bool,
    /// Its sum on record is of the comment as it was posted, its markers in
    /// (`rendered`, before T4.7): the poll adopts it in place on its first
    /// read, as the sum of its visible text.
    pub old_sum: bool,
}

/// plan-show's `.planning`: none, a value that is not an object, or the open
/// session.
#[derive(Clone, Debug, PartialEq)]
pub enum PlanRow {
    None,
    Unreadable,
    Open(Box<Planning>),
}

/// What the rules read of an open planning session.
#[derive(Clone, Debug, PartialEq)]
pub struct Planning {
    /// The question's comment id: empty for none, text as it is, any other
    /// value as its JSON text.
    pub comment_id: String,
    /// The options of its question, each without its final newlines; one
    /// that is not text is empty.
    pub options: Vec<String>,
    /// The session as `shape` gives it.
    pub view: PlanView,
}

/// What the rules read of `shape`'s session: who started it, the millwright
/// flags of its answers and the ids its notes hold. A list that is not one
/// is None.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanView {
    /// Empty when it is not text.
    pub by: String,
    pub millwrights: Option<Vec<bool>>,
    pub refused: Option<Vec<u64>>,
    pub held: Option<Vec<u64>>,
}

/// The session `shape` gives, as the millwright's rules read it; None when it is
/// no object.
pub(crate) fn plan_view(view: &Value) -> Option<PlanView> {
    let ids = |k: &str| match view.get(k) {
        None | Some(Value::Null) => Some(Vec::new()),
        Some(Value::Array(a)) => Some(a.iter().filter_map(Value::as_u64).collect()),
        Some(_) => None,
    };
    view.is_object().then(|| PlanView {
        by: view
            .get("by")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        millwrights: match view.get("answers") {
            None | Some(Value::Null) => Some(Vec::new()),
            Some(Value::Array(a)) => a
                .iter()
                .map(|x| x.get("millwright").and_then(Value::as_bool))
                .collect(),
            Some(_) => None,
        },
        refused: ids("millwright_refused"),
        held: ids("held"),
    })
}

/// The shape of `store plan-view`: the session as plan_view gives it, from plan-show's
/// `.planning`: the refused replies and the held ticks from the notes, and
/// each answer's turn, account and millwright mark.
pub(crate) fn shape(row: &Value) -> Value {
    if !row.is_object() {
        return Value::Null;
    }
    let notes = |k: &str| {
        row.pointer(&format!("/notes/{k}"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let refused = notes("millwright_refused");
    let answers: Vec<Value> = row
        .get("answers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|a| match a.get("millwright") {
            Some(Value::Bool(m)) => {
                json!({"turn": a.get("turn"), "by": a.get("by"), "millwright": m})
            }
            _ => a,
        })
        .collect();
    let mut v = json!({"answers": answers});
    for k in ["by", "label"] {
        if let Some(x) = row.get(k).filter(|x| !x.is_null()) {
            v[k] = x.clone();
        }
    }
    if !refused.is_empty() {
        v["millwright_refused"] = json!(refused);
    }
    let held = notes("held");
    if !held.is_empty() {
        v["held"] = json!(held);
    }
    v
}

/// An issue's comment read (GraphQL's .data.repository.issue.comments.nodes).
#[derive(Clone, Debug, PartialEq)]
pub enum Comments {
    Read(Vec<Comment>),
    /// The answer has no list of nodes, or a null one.
    NoList,
    /// The answer is itself a list, as REST gives it.
    Bare(Vec<Comment>),
    /// The nodes are neither a list nor null.
    Unreadable,
}

impl Comments {
    /// The comments of an answer, the GraphQL nodes or a bare list.
    pub fn parse(v: &Value) -> Comments {
        let account = |a: Option<&Value>| Account {
            login: a
                .and_then(|a| a.get("login"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            id: a.and_then(|a| a.get("databaseId")).and_then(number),
        };
        let comment = |x: &Value| {
            let s = |k: &str| x.get(k).and_then(Value::as_str).map(str::to_string);
            let created = x.get("createdAt");
            let edited = x.get("lastEditedAt");
            let h = x.get("userContentEdits");
            let note = || {
                let login = x
                    .pointer("/author/login")
                    .or_else(|| x.pointer("/user/login"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let created = created
                    .or_else(|| x.get("created_at"))
                    .and_then(Value::as_str)?
                    .to_string();
                let updated = match edited.or_else(|| x.get("updated_at")) {
                    None | Some(Value::Null) => created.clone(),
                    Some(u) => u.as_str()?.to_string(),
                };
                let Some(Value::Number(id)) = x.get("databaseId").or_else(|| x.get("id")) else {
                    return None;
                };
                Some(Note {
                    id: id.clone(),
                    editor: x
                        .pointer("/editor/login")
                        .and_then(Value::as_str)
                        .map_or_else(|| login.clone(), str::to_string),
                    login,
                    body: s("body")?,
                    created,
                    updated,
                })
            };
            let given = |v: Option<&Value>| match v? {
                Value::Null => None,
                Value::Number(n) => Some(Given::Number(n.clone())),
                Value::String(t) => Some(Given::Text(t.clone())),
                o => Some(Given::Other(o.to_string())),
            };
            Comment {
                id: given(x.get("databaseId")),
                author: account(x.get("author")),
                editor: x
                    .pointer("/editor/login")
                    .filter(|l| l.is_string())
                    .map(|_| account(x.get("editor"))),
                body: given(x.get("body")),
                created_at: given(created),
                edited: edited.is_some_and(|e| !e.is_null()),
                last_edited_at: edited.and_then(Value::as_str).unwrap_or("").to_string(),
                history: CommentHistory {
                    failed: h.and_then(|h| h.get("failed")) == Some(&Value::Bool(true)),
                    total: h
                        .and_then(|h| h.get("totalCount"))
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0),
                    editors: h
                        .and_then(|h| h.get("nodes"))
                        .and_then(Value::as_array)
                        .map(|a| a.iter().map(|e| account(e.get("editor"))).collect())
                        .unwrap_or_default(),
                },
                note: note(),
            }
        };
        if let Value::Array(a) = v {
            return Comments::Bare(a.iter().map(comment).collect());
        }
        match v.pointer("/data/repository/issue/comments/nodes") {
            None | Some(Value::Null) => Comments::NoList,
            Some(Value::Array(a)) => Comments::Read(a.iter().map(comment).collect()),
            Some(_) => Comments::Unreadable,
        }
    }

    /// The nodes GitHub listed, None for no list.
    pub fn listed(&self) -> Option<&[Comment]> {
        match self {
            Comments::Read(l) => Some(l),
            _ => None,
        }
    }

    /// The nodes as the question's read takes them: no list is none, and
    /// nodes that are no list cannot be read.
    pub fn or_none(&self) -> Option<&[Comment]> {
        match self {
            Comments::Read(l) => Some(l),
            Comments::NoList | Comments::Bare(_) => Some(&[]),
            Comments::Unreadable => None,
        }
    }

    /// The nodes, or the bare list a REST answer is.
    pub fn nodes_or_bare(&self) -> Option<&[Comment]> {
        match self {
            Comments::Read(l) | Comments::Bare(l) => Some(l),
            _ => None,
        }
    }
}

/// One comment. A text GitHub did not give as text is None, or empty where
/// it says so.
#[derive(Clone, Debug, PartialEq)]
pub struct Comment {
    /// databaseId, None when missing or null.
    pub id: Option<CommentId>,
    pub author: Account,
    /// Present when it has a login as text.
    pub editor: Option<Account>,
    pub body: Option<Given>,
    pub created_at: Option<Given>,
    /// lastEditedAt is there and not null.
    pub edited: bool,
    /// lastEditedAt as text, empty when it is none.
    pub last_edited_at: String,
    pub history: CommentHistory,
    /// The comment as change_note reads it, REST's names taken where
    /// GraphQL's are missing; None when it is no note.
    pub note: Option<Note>,
}

impl Comment {
    /// The editor's login, empty for none.
    pub fn editor_login(&self) -> &str {
        self.editor.as_ref().map_or("", |e| e.login.as_str())
    }

    /// The body when it is text.
    pub fn body_text(&self) -> Option<&str> {
        self.body.as_ref().and_then(Given::text)
    }

    /// createdAt when it is text.
    pub fn created_text(&self) -> Option<&str> {
        self.created_at.as_ref().and_then(Given::text)
    }
}

/// An account a comment names: its login, empty when missing, and its user
/// id, a number or text that is one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Account {
    pub login: String,
    pub id: Option<u64>,
}

/// A comment's databaseId as GitHub gave it.
pub type CommentId = Given;

/// A value of a comment as GitHub gave it, not null: text, a number, or
/// any other JSON as its text, kept so an answer passes it on unchanged.
#[derive(Clone, Debug, PartialEq)]
pub enum Given {
    Number(serde_json::Number),
    Text(String),
    Other(String),
}

impl Given {
    /// The value when it is text.
    pub fn text(&self) -> Option<&str> {
        match self {
            Given::Text(s) => Some(s),
            _ => None,
        }
    }

    /// The value as a number, as `number` reads it.
    pub fn number(&self) -> Option<u64> {
        match self {
            Given::Number(n) => n.as_u64(),
            Given::Text(s) => s.trim().parse().ok(),
            Given::Other(_) => None,
        }
    }

    /// The value as raw text: text as it is, anything else in JSON, `null`
    /// for none.
    pub fn said(id: Option<&Given>) -> String {
        match id {
            Some(Given::Text(s) | Given::Other(s)) => s.clone(),
            Some(Given::Number(n)) => n.to_string(),
            None => "null".into(),
        }
    }

    /// The value as GitHub gave it, null for none.
    pub fn json(g: Option<&Given>) -> Value {
        match g {
            Some(Given::Number(n)) => Value::Number(n.clone()),
            Some(Given::Text(t)) => json!(t),
            Some(Given::Other(j)) => serde_json::from_str(j).unwrap_or(Value::Null),
            None => Value::Null,
        }
    }

    /// The value for sorting: a whole number, 0 otherwise.
    pub fn sort_key(id: Option<&Given>) -> i64 {
        match id {
            Some(Given::Number(n)) => n.as_i64().unwrap_or(0),
            _ => 0,
        }
    }
}

/// A comment's edit history (userContentEdits).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CommentHistory {
    /// The read of the history failed.
    pub failed: bool,
    /// totalCount, 0 when it is none.
    pub total: f64,
    /// The editor of each edit read.
    pub editors: Vec<Account>,
}

/// A comment in the REST shape `window` reads: a number id, a text body and
/// both stamps; the last edit is the first stamp when nobody saved it after.
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub id: serde_json::Number,
    pub login: String,
    /// The editor's login, the author's when none is given.
    pub editor: String,
    pub body: String,
    pub created: String,
    pub updated: String,
}

/// The numbers every blocker comment of an issue names, for the snapshot to
/// ask their states: blocked_ok reads the newest one an allowed account wrote.
pub(crate) fn named_blockers(comments: &Value) -> Vec<u64> {
    crate::decide::named_blockers_of(Comments::parse(comments).listed().unwrap_or_default())
}

/// A pull request a refresh reads (GraphQL's .data.repository.pullRequest).
#[derive(Clone, Debug, PartialEq)]
pub enum PullNode {
    /// The read failed, on this gh call's line.
    Failed {
        line: Option<String>,
    },
    /// GitHub gave no object.
    Unreadable,
    Read(Box<PullRead>),
}

/// What the refresh rules read of a pull request. A text GitHub did not give
/// as text is empty.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PullRead {
    pub number: Option<u64>,
    /// The number as JSON text, `null` when missing.
    pub number_said: String,
    pub state: String,
    pub author: String,
    pub cross_repository: Option<bool>,
    pub head_repository: String,
    pub draft: Option<bool>,
    /// closingIssuesReferences' totalCount as a float, 0 when it is none.
    pub closing_count: f64,
    pub closing: Vec<ClosedIssue>,
    pub head_oid: Option<String>,
    pub base_ref: String,
    /// GitHub's word for whether it conflicts.
    pub merge: Option<String>,
    pub labels: Vec<String>,
    pub url: String,
}

/// An issue a pull request closes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClosedIssue {
    pub number: Option<u64>,
    pub state: String,
    pub labels: Vec<String>,
    /// The numbers of the open pull requests that close it, None for one that
    /// is not a u64.
    pub open_prs: Vec<Option<u64>>,
}

/// Whether a value is a commit id: 40 lowercase hex.
pub(crate) fn hex40(v: Option<&Value>) -> bool {
    v.and_then(Value::as_str)
        .is_some_and(tehas_core::grammar::commit_id)
}

/// A pull request as GraphQL gives it, read for the refresh rules.
pub(crate) fn pull_read(pr: &Value) -> PullRead {
    let text = |v: Option<&Value>| v.and_then(Value::as_str).unwrap_or("").to_string();
    let names = |v: &Value| names(v.pointer("/labels/nodes"));
    let closing = pr.get("closingIssuesReferences");
    PullRead {
        number: pr.get("number").and_then(Value::as_u64),
        number_said: pr.get("number").unwrap_or(&Value::Null).to_string(),
        state: text(pr.get("state")),
        author: text(pr.pointer("/author/login")),
        cross_repository: pr.get("isCrossRepository").and_then(Value::as_bool),
        head_repository: text(pr.pointer("/headRepository/nameWithOwner")),
        draft: pr.get("isDraft").and_then(Value::as_bool),
        closing_count: closing
            .and_then(|c| c.get("totalCount"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        closing: closing
            .and_then(|c| c.get("nodes"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|i| ClosedIssue {
                number: i.get("number").and_then(Value::as_u64),
                state: text(i.get("state")),
                labels: names(i),
                open_prs: i
                    .pointer("/closedByPullRequestsReferences/nodes")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter(|p| p.get("state").and_then(Value::as_str) == Some("OPEN"))
                    .map(|p| p.get("number").and_then(Value::as_u64))
                    .collect(),
            })
            .collect(),
        head_oid: pr
            .get("headRefOid")
            .and_then(Value::as_str)
            .map(str::to_string),
        base_ref: text(pr.get("baseRefName")),
        merge: pr
            .get("mergeable")
            .and_then(Value::as_str)
            .map(str::to_string),
        labels: names(pr),
        url: text(pr.get("url")),
    }
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

/// The approval of an issue's latest start, as approval-view gives it. A
/// text that is no string reads as empty, and a record that is no object as
/// one with every field empty.
#[derive(Clone, Debug, PartialEq)]
pub struct Approval {
    pub how: String,
    pub at: String,
    pub by: String,
    pub body_sha256: Option<String>,
    /// [`Driver::of_record`] of it.
    pub driver: Option<Driver>,
    pub rests_on: RestsOn,
    pub refreshes: RefreshCount,
}

/// The record's rests_on: the approval a millwright's or a refresh's start rests
/// on.
#[derive(Clone, Debug, PartialEq)]
pub enum RestsOn {
    Absent,
    Null,
    /// Neither an object nor null.
    Other,
    Record(Rests),
}

/// The fields of a rests_on object; one that is no string is None.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rests {
    pub at: Option<String>,
    pub by: Option<String>,
    pub how: Option<String>,
    pub body_sha256: Option<String>,
}

/// The record's refreshes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RefreshCount {
    Absent,
    Num(f64),
    Other,
}

/// The person's approval a record rests on, when it reads as one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Base {
    pub at: String,
    pub by: String,
    pub how: String,
    pub body_sha256: String,
}

impl Approval {
    pub(crate) fn of(v: &Value) -> Approval {
        let s = |o: &Value, k: &str| o.get(k).and_then(Value::as_str).map(str::to_string);
        let num = |k: &str| v.get(k).map(|x| x.as_f64().ok_or(()));
        Approval {
            how: s(v, "how").unwrap_or_default(),
            at: s(v, "at").unwrap_or_default(),
            by: s(v, "by").unwrap_or_default(),
            body_sha256: s(v, "body_sha256"),
            driver: Driver::of_record(v),
            rests_on: match v.get("rests_on") {
                None => RestsOn::Absent,
                Some(Value::Null) => RestsOn::Null,
                Some(r @ Value::Object(_)) => RestsOn::Record(Rests {
                    at: s(r, "at"),
                    by: s(r, "by"),
                    how: s(r, "how"),
                    body_sha256: s(r, "body_sha256"),
                }),
                Some(_) => RestsOn::Other,
            },
            refreshes: match num("refreshes") {
                None => RefreshCount::Absent,
                Some(Ok(f)) => RefreshCount::Num(f),
                Some(Err(())) => RefreshCount::Other,
            },
        }
    }

    /// A millwright's or a refresh's record rests on another approval.
    fn rests(&self) -> bool {
        matches!(self.how.as_str(), MILLWRIGHT | "refresh")
    }

    /// The base it rests on: rests_on for a millwright's or a refresh's, the
    /// record itself for any other.
    pub fn base(&self) -> Option<Base> {
        if !self.rests() {
            return Base::checked(&self.at, &self.by, &self.how, self.body_sha256.as_deref()?);
        }
        let RestsOn::Record(r) = &self.rests_on else {
            return None;
        };
        Base::checked(
            r.at.as_deref()?,
            r.by.as_deref()?,
            r.how.as_deref()?,
            r.body_sha256.as_deref()?,
        )
    }

    /// The refreshes it counts on that base.
    pub fn refresh_count(&self, base: &Base) -> Option<u64> {
        self.refreshes_on(Some(&base.at), Some(&base.by))
    }

    /// The refreshes it counts on the approval of `at` by `by`, None for one
    /// that is not there.
    fn refreshes_on(&self, at: Option<&str>, by: Option<&str>) -> Option<u64> {
        if self.how != "refresh" {
            return Some(0);
        }
        // jq's (.rests_on | {at, by}) fails on anything but an object or null.
        let same = match &self.rests_on {
            RestsOn::Other => return None,
            RestsOn::Absent | RestsOn::Null => at.is_none() && by.is_none(),
            RestsOn::Record(r) => r.at.as_deref() == at && r.by.as_deref() == by,
        };
        if !same {
            return Some(0);
        }
        let RefreshCount::Num(n) = self.refreshes else {
            return None;
        };
        (n >= 1.0 && n.fract() == 0.0 && n < 1_000_000.0).then_some(n as u64)
    }
}

impl Base {
    fn checked(at: &str, by: &str, how: &str, sum: &str) -> Option<Base> {
        let ok = !at.is_empty()
            && at
                .bytes()
                .all(|b| b.is_ascii_digit() || matches!(b, b'T' | b':' | b'.' | b'Z' | b'-'))
            && !by.is_empty()
            && by.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && matches!(how, "approved" | "outage-retry" | HOST | "answer" | "reply")
            && sum.len() == 64
            && sum.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        ok.then(|| Base {
            at: at.into(),
            by: by.into(),
            how: how.into(),
            body_sha256: sum.into(),
        })
    }

    /// Whether it still approves: the host's approval names no account
    /// (#1967), and a person's stands while the list names its account.
    pub fn stands(&self, c: &Settings) -> bool {
        self.how == HOST || c.allowed(&self.by)
    }

    /// The account it was given by, as `changed_after` reads an approver.
    pub fn approver(&self) -> &str {
        approver(&self.how, &self.by)
    }

    /// Its four fields as the record writes them.
    pub fn pairs(&self) -> [(&'static str, &str); 4] {
        [
            ("at", &self.at),
            ("by", &self.by),
            ("how", &self.how),
            ("body_sha256", &self.body_sha256),
        ]
    }
}

/// None for the host's approval, whose `by` is a word and not an account, so
/// a change in its second by an account called `host` counts as after it.
fn approver<'a>(how: &str, by: &'a str) -> &'a str {
    if how == HOST {
        ""
    } else {
        by
    }
}

/// [`Approval::base`] of a record, {at, by, how, body_sha256}.
pub(crate) fn record_base(record: &Value) -> Option<Value> {
    Approval::of(record).base().map(|b| {
        Value::Object(
            b.pairs()
                .into_iter()
                .map(|(k, v)| (k.into(), json!(v)))
                .collect(),
        )
    })
}

/// [`Base::stands`] of a base of `record_base`.
pub(crate) fn base_stands(c: &Settings, base: &Value) -> bool {
    let s = |k: &str| {
        base.get(k)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    Base {
        at: s("at"),
        by: s("by"),
        how: s("how"),
        body_sha256: s("body_sha256"),
    }
    .stands(c)
}

/// [`Base::approver`] of a base of `record_base`.
pub(crate) fn base_approver(base: &Value) -> &str {
    let s = |k: &str| base.get(k).and_then(Value::as_str).unwrap_or("");
    approver(s("how"), s("by"))
}

/// [`Approval::refresh_count`] of a record on a base of at least {at, by}.
#[cfg(test)]
pub(crate) fn refresh_count(record: &Value, base: &Value) -> Option<u64> {
    let s = |k: &str| base.get(k).and_then(Value::as_str);
    Approval::of(record).refreshes_on(s("at"), s("by"))
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
    pub slice: Probed,
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
    /// The pull requests a refresh reads, by number.
    pub pulls: BTreeMap<u64, PullNode>,
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
    /// The open records no process holds the lock of, `(issue, attempt)`:
    /// cycles that died without a word, which the poll ends as lost.
    pub lost: Vec<(u64, u64)>,
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

pub(crate) fn reading(v: &Value) -> Probed {
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    let reading = Reading {
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
    };
    Probed::of(reading)
}

fn issue(v: &Value) -> Issue {
    Issue {
        events: v.get("events").map(Events::parse).unwrap_or(Events::Failed),
        edit: v
            .get("edit")
            .filter(|e| !failed(e) && e.is_object())
            .map(EditRecord::parse),
        work: v.get("work").filter(|w| !failed(w)).map(|w| {
            let Value::Object(o) = w else {
                return Work::Unreadable;
            };
            let count = |s: &Value, k: &str| Count {
                f: s.get(k).and_then(Value::as_f64),
                u: s.get(k).and_then(Value::as_u64),
            };
            let mut strings = Vec::new();
            crate::text::strings(w, &mut strings);
            Work::Issue(Box::new(WorkIssue {
                state: o.get("state").and_then(Value::as_str).map(str::to_string),
                is_pull: !o.get("pull_request").unwrap_or(&Value::Null).is_null(),
                title: o.get("title").and_then(Value::as_str).map(str::to_string),
                body: match o.get("body") {
                    None | Some(Value::Null) => Text::None,
                    Some(Value::String(b)) => Text::Some(b.clone()),
                    Some(_) => Text::Unreadable,
                },
                labels: match o.get("labels") {
                    None => Labels::Absent,
                    Some(Value::Null) => Labels::Null,
                    Some(Value::Array(_)) => Labels::Names(names(o.get("labels"))),
                    Some(_) => Labels::Unreadable,
                },
                sub_issues: o.get("sub_issues_summary").map(|s| SubIssues {
                    total: count(s, "total"),
                    completed: count(s, "completed"),
                }),
                parent: o.get("parent").and_then(Value::as_u64),
                blocked_by: w
                    .pointer("/issue_dependencies_summary/blocked_by")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                strings: strings.into_iter().map(str::to_string).collect(),
            }))
        }),
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
        comments: v
            .get("comments")
            .filter(|c| !failed(c))
            .map(Comments::parse),
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
            p if failed(p) => Err(StoreErr::parse(p)),
            Value::Object(p) => Ok(Some(Park {
                body_sha256: p
                    .get("body_sha256")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                reason: match p.get("reason") {
                    None => "null".to_string(),
                    Some(Value::String(r)) => r.clone(),
                    Some(r) => r.to_string(),
                },
                fresh: p.get("retries").and_then(Value::as_f64) == Some(0.0),
                question: match p.get("question") {
                    Some(Value::Number(x)) => Some(x.to_string()),
                    Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
                    _ => None,
                },
                told: !p.get("told_at").unwrap_or(&Value::Null).is_null(),
            })),
            _ => Ok(None),
        }),
        approval: v.get("approval").map(|a| match a {
            Value::Null => Ok(None),
            a if failed(a) => Err(()),
            a => Ok(Some(Approval::of(a))),
        }),
        current: v.get("current").map(|a| match a {
            Value::Null => Ok(None),
            a if failed(a) => Err(StoreErr::parse(a)),
            a if a.get("how").and_then(Value::as_str) == Some("refresh") => {
                let pr = match a.get("refresh_pr") {
                    Some(Value::Number(x)) => x.to_string(),
                    Some(Value::String(x)) => x.clone(),
                    _ => String::new(),
                };
                let ok = !pr.is_empty()
                    && pr.len() <= 10
                    && !pr.starts_with('0')
                    && pr.bytes().all(|b| b.is_ascii_digit());
                Ok(ok.then(|| pr.parse().unwrap_or(0)))
            }
            _ => Ok(None),
        }),
        question: v.get("question").map(|q| {
            if failed(q) {
                return Err(StoreErr::parse(q));
            }
            let text = |v: &Value| v.as_str().map(str::to_string);
            let planning = |row: &Value| {
                let view = plan_view(&shape(row))?;
                Some(Planning {
                    comment_id: match row.get("comment_id") {
                        None | Some(Value::Null) => String::new(),
                        Some(Value::String(s)) => s.clone(),
                        Some(v) => v.to_string(),
                    },
                    options: row
                        .get("options")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .map(|o| o.as_str().unwrap_or("").trim_end_matches('\n').to_string())
                        .collect(),
                    view,
                })
            };
            Ok(Question {
                id: match q.get("id") {
                    Some(Value::String(s)) => s.clone(),
                    Some(Value::Number(x)) => x.to_string(),
                    _ => String::new(),
                },
                kind: q.get("kind").and_then(text).unwrap_or_default(),
                reason: q.get("reason").and_then(text),
                reask: q.get("reask") == Some(&Value::Bool(true)),
                old_sum: q.get("old_sum") == Some(&Value::Bool(true)),
                plan: match q.get("plan") {
                    None | Some(Value::Null) => PlanRow::None,
                    Some(row) => {
                        planning(row).map_or(PlanRow::Unreadable, |p| PlanRow::Open(Box::new(p)))
                    }
                },
            })
        }),
        view: v.get("view").map(|w| {
            if failed(w) {
                return View::Failed;
            }
            let text = |k: &str| w.get(k).and_then(Value::as_str).map(str::to_string);
            View::Read(ViewRead {
                state: text("state"),
                labels: names(w.get("labels")),
                title: text("title"),
                body: text("body"),
            })
        }),
        // The question read's, or the park's for a park whose hold may lift.
        // The poll knows a question by its comment: the store's `comment`, not
        // the store's own number in `question`.
        answer: ["/question/answer", "/park/answer"]
            .iter()
            .find_map(|p| v.pointer(p).filter(|a| a.is_object()))
            .and_then(|a| {
                let s = |k: &str| a.get(k).and_then(Value::as_str);
                let how = crate::answer::How::of(s("how")?)?;
                crate::answer::Answer::new(
                    a.get("comment")?.as_u64()?,
                    s("kind")?,
                    s("choice")?,
                    a.get("option").and_then(Value::as_u64),
                    s("text"),
                    s("by")?,
                    a.get("at")?.as_i64()?,
                )
                .map(|answer| crate::answer::Answer { how, ..answer })
            }),
        history: v.get("history").map(|h| match h {
            h if failed(h) => Err(()),
            h => Ok(h
                .as_array()
                .into_iter()
                .flatten()
                .map(|r| CycleRow {
                    refresh: r.get("how").and_then(Value::as_str) == Some("refresh"),
                    parked: r.get("parked").and_then(Value::as_bool) == Some(true),
                    started: r.get("started").and_then(Value::as_u64),
                })
                .collect()),
        }),
        request: v.get("request").map(|r| RefreshRequest {
            at: r
                .get("at")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            target: (r.get("pr").is_some_and(Value::is_number)
                && hex40(r.get("head_sha"))
                && hex40(r.get("base_sha")))
            .then(|| RefreshTarget {
                pr: r["pr"].as_u64().unwrap_or(0),
                head_sha: r["head_sha"].as_str().unwrap_or("").to_string(),
                base_sha: r["base_sha"].as_str().unwrap_or("").to_string(),
            }),
        }),
        refresh_said: v
            .get("refresh_said")
            .and_then(Value::as_str)
            .map(str::to_string),
        plan_start: v.get("plan_start").map(|p| match p {
            Value::Object(o) => PlanStart::Record {
                by: o
                    .get("by")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                sum: o.get("sum").and_then(Value::as_str).map(str::to_string),
            },
            _ => PlanStart::Unreadable,
        }),
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
                Some(c)
                    .filter(|c| !failed(c))
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
                        .filter_map(|(k, p)| {
                            let node = if failed(p) {
                                PullNode::Failed {
                                    line: failed_line(Some(p)),
                                }
                            } else if p.is_object() {
                                PullNode::Read(Box::new(pull_read(p)))
                            } else {
                                PullNode::Unreadable
                            };
                            Some((k.parse().ok()?, node))
                        })
                        .collect()
                })
                .unwrap_or_default(),
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
            lost: v
                .get("lost")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|x| {
                            Some((x.get("issue")?.as_u64()?, x.get("attempt")?.as_u64()?))
                        })
                        .collect()
                })
                .unwrap_or_default(),
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
        }
    }
}
