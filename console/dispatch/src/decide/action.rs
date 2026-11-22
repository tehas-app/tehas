//! What a poll decides to do, one action at a time. `apply` still reads
//! each as JSON: `From<&Action> for Value` gives the keys it routes on.

use super::note::ContextNote;
use crate::snapshot::{Base, CommentId, Given};
use serde_json::{json, Map, Value};
use tehas_core::driver::Driver;
use tehas_core::grammar::SpecMark;

/// A comment's databaseId as GitHub gave it, None when it gave none.
pub type CommentRef = Option<CommentId>;

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Label(LabelChange),
    /// Said once for each key: the refresh's hold a person can act on.
    SayOnce {
        issue: u64,
        key: String,
        say: String,
    },
    /// The park's hold, said once.
    ParkTold {
        issue: u64,
        say: String,
    },
    Say {
        issue: u64,
        say: String,
    },
    /// The answer `tehas answer` gave to the question `id` is taken away.
    AnswerDrop {
        issue: u64,
        id: u64,
        fail: String,
        say: Option<String>,
    },
    /// A question asked before 2.0, open at the upgrade, asked again in
    /// 2.0's words once (#2782): `body` posted (None for a stop that offers
    /// nothing to decide), the question recorded with `offers`, the old
    /// comment `old` (whose text is `old_body`) told once that it is
    /// superseded, and taken off the record.
    Reask {
        issue: u64,
        /// The label taken off once the old question is off the record.
        unlabel: Option<String>,
        kind: String,
        old: String,
        old_body: String,
        body: Option<String>,
        offers: String,
    },
    /// tehas:refreshing comes off a pull request.
    Unmark {
        pr: u64,
        log: String,
    },
    PlanNote {
        issue: u64,
        kind: NoteKind,
        id: String,
        say: String,
    },
    RefreshDrop(u64),
    RefreshHeld(String),
    BranchNote(String),
    Reap {
        name: String,
        why: String,
    },
    Quiet {
        issue: u64,
        said_for: u64,
        now: u64,
        due: QuietDue,
    },
    StopDone {
        issue: u64,
        why: String,
    },
    Sweep {
        issue: u64,
        log: String,
    },
    PlanCollect(u64),
    PlanPost(u64),
    PlanClose(u64),
    PlanStartDrop(u64),
    Units {
        first: u64,
    },
    IndexGate {
        running: Vec<u64>,
    },
    PlanTurn {
        issue: u64,
        turn: Turn,
    },
    ParkRetry {
        issue: u64,
        how: Spend,
    },
    PlanEnd {
        issue: u64,
        say: String,
    },
    Start(Box<CycleStart>),
}

impl Action {
    /// The issue and the line of an action that names both: a label change
    /// that logs.
    pub fn issue_log(&self) -> Option<(u64, &str)> {
        match self {
            Action::Label(l) => Some((l.issue, l.log.as_deref()?)),
            _ => None,
        }
    }

    pub fn is_start(&self) -> bool {
        matches!(self, Action::Start(_))
    }
}

/// Labels come off and on an issue, and what is said of it.
#[derive(Clone, Debug, PartialEq)]
pub struct LabelChange {
    pub issue: u64,
    pub remove: Vec<String>,
    pub add: Vec<String>,
    pub fail: String,
    pub say: Option<String>,
    pub log: Option<String>,
    /// The label read again just before the change.
    pub reread: Option<String>,
    /// The line logged once that read says the change is still due.
    pub first: Option<String>,
    /// The question this change answers: its id and who answered.
    pub answered: Option<(String, String)>,
}

impl LabelChange {
    pub fn new(issue: u64, remove: &[&str], fail: impl Into<String>) -> Self {
        LabelChange {
            issue,
            remove: remove.iter().map(|l| l.to_string()).collect(),
            add: Vec::new(),
            fail: fail.into(),
            say: None,
            log: None,
            reread: None,
            first: None,
            answered: None,
        }
    }

    pub fn put_on(mut self, add: &[&str]) -> Self {
        self.add = add.iter().map(|l| l.to_string()).collect();
        self
    }

    pub fn say(mut self, say: impl Into<String>) -> Self {
        self.say = Some(say.into());
        self
    }

    pub fn log(mut self, log: impl Into<String>) -> Self {
        self.log = Some(log.into());
        self
    }

    pub fn reread(mut self, label: impl Into<String>, first: Option<String>) -> Self {
        self.reread = Some(label.into());
        self.first = first;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteKind {
    MillwrightRefused,
}

impl NoteKind {
    pub fn word(self) -> &'static str {
        match self {
            NoteKind::MillwrightRefused => "millwright-refused",
        }
    }
}

/// What a quiet cycle is owed: the line is the one the store's answer is
/// expected to give.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuietDue {
    Stop { log: String },
    Again { log: String },
    Line { log: String, said_only: bool },
    Wait,
}

/// A planning turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Turn {
    /// A session starts, on `label` of `by`.
    First {
        label: String,
        by: String,
        refine: bool,
        title: Option<String>,
        body: Option<String>,
        log: String,
        brief_extra: Option<String>,
        epic: Option<u64>,
        /// The park reason of a refine the poll started on its own.
        auto: Option<String>,
    },
    /// The session goes on with an answer.
    Next {
        answer: String,
        by: String,
        log: String,
        reply: Option<String>,
    },
}

/// Why a park's retry or refine is spent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spend {
    Outage,
    Skipped,
    Refine,
}

impl Spend {
    pub fn word(self) -> &'static str {
        match self {
            Spend::Outage => "outage",
            Spend::Skipped => "skipped",
            Spend::Refine => "refine",
        }
    }
}

/// The start of a cycle: what start_cycles records and changes.
#[derive(Clone, Debug, PartialEq)]
pub struct CycleStart {
    pub issue: u64,
    pub by: String,
    pub how: String,
    pub at: String,
    pub given: String,
    pub remove: Vec<String>,
    pub verb: String,
    /// The line apply logs as the start begins.
    pub log: String,
    pub text: bool,
    pub body_sha256: String,
    pub title_sha256: Option<String>,
    pub links: Vec<String>,
    /// A start of the fixer's: the person's approval it rests on, and its
    /// starts with this one.
    pub(crate) rests_on: Option<(Base, u64)>,
    pub(crate) park_lifted: Option<ContextNote>,
    pub spec: Option<SpecPin>,
    pub extra: StartExtra,
}

/// The spec a start pins: the `Spec:` line read, or why it cannot be, and
/// what comes off when the spec stops the start.
#[derive(Clone, Debug, PartialEq)]
pub struct SpecPin {
    pub read: Result<SpecMark, String>,
    pub off: Vec<String>,
}

/// What a start of its kind adds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StartExtra {
    /// Only a lean start names its driver.
    pub driver: Option<Driver>,
    pub contractor: Option<String>,
    pub approved_label: Option<String>,
    pub retry: Option<RetryTick>,
    pub refresh: Option<RefreshOf>,
}

/// The tick a retry starts on.
#[derive(Clone, Debug, PartialEq)]
pub struct RetryTick {
    pub comment: CommentRef,
    pub since: String,
    pub issue_alone: bool,
}

/// The pull request a refresh continues.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefreshOf {
    pub pr: u64,
    pub url: String,
    pub head_sha: String,
    pub base_sha: String,
    pub branch: String,
}

/// The keys the start's own fields write; `StartExtra` adds the rest.
const START_BASE: [&str; 16] = [
    "start",
    "by",
    "how",
    "at",
    "given",
    "remove",
    "verb",
    "log",
    "text",
    "body_sha256",
    "title_sha256",
    "links",
    "rests_on",
    "fixer_starts",
    "park_lifted",
    "spec",
];

impl From<&Action> for Value {
    fn from(a: &Action) -> Self {
        let mut m = Map::new();
        let mut put = |k: &str, v: Value| {
            m.insert(k.into(), v);
        };
        match a {
            Action::Label(l) => {
                put("issue", json!(l.issue));
                put("remove", json!(l.remove));
                if !l.add.is_empty() {
                    put("add", json!(l.add));
                }
                put("fail", json!(l.fail));
                let opt = [
                    ("say", &l.say),
                    ("log", &l.log),
                    ("reread", &l.reread),
                    ("first", &l.first),
                ];
                for (k, v) in opt {
                    if let Some(v) = v {
                        put(k, json!(v));
                    }
                }
                if let Some((id, by)) = &l.answered {
                    put("answered", json!({"id": id, "by": by}));
                }
            }
            Action::SayOnce { issue, key, say } => {
                put("issue", json!(issue));
                put("refresh_said", json!(key));
                put("say", json!(say));
            }
            Action::ParkTold { issue, say } => {
                put("issue", json!(issue));
                put("park_told", json!(true));
                put("say", json!(say));
            }
            Action::Say { issue, say } => {
                put("issue", json!(issue));
                put("say", json!(say));
            }
            Action::AnswerDrop {
                issue,
                id,
                fail,
                say,
            } => {
                put("answer_drop", json!(issue));
                put("id", json!(id));
                put("fail", json!(fail));
                if let Some(s) = say {
                    put("say", json!(s));
                }
            }
            Action::Reask {
                issue,
                unlabel,
                kind,
                old,
                old_body,
                body,
                offers,
            } => {
                put("reask", json!(issue));
                put("kind", json!(kind));
                put("old", json!(old));
                put("old_body", json!(old_body));
                put("body", json!(body));
                put("offers", json!(offers));
                put("unlabel", json!(unlabel));
            }
            Action::Unmark { pr, log } => {
                put("pr", json!(pr));
                put("remove", json!(["tehas:refreshing"]));
                put("log", json!(log));
            }
            Action::PlanNote {
                issue,
                kind,
                id,
                say,
            } => {
                put("plan_note", json!(issue));
                put("kind", json!(kind.word()));
                put("id", json!(id));
                put("say", json!(say));
            }
            Action::RefreshDrop(n) => put("refresh_drop", json!(n)),
            Action::RefreshHeld(h) => put("refresh_held", json!(h)),
            Action::BranchNote(s) => put("branch_note", json!(s)),
            Action::Reap { name, why } => {
                put("reap", json!(name));
                put("why", json!(why));
            }
            Action::Quiet {
                issue,
                said_for,
                now,
                due,
            } => {
                put("quiet", json!(issue));
                put("said_for", json!(said_for));
                put("now", json!(now));
                match due {
                    QuietDue::Stop { log } | QuietDue::Again { log } => {
                        put("log", json!(log));
                        put("stop", json!(true));
                    }
                    QuietDue::Line { log, said_only } => {
                        put("log", json!(log));
                        put("asked", json!(true));
                        put("said_only", json!(said_only));
                    }
                    QuietDue::Wait => {}
                }
            }
            Action::StopDone { issue, why } => {
                put("stop_done", json!(issue));
                put("why", json!(why));
            }
            Action::Sweep { issue, log } => {
                put("sweep", json!(issue));
                put("log", json!(log));
            }
            Action::PlanCollect(n) => put("plan_collect", json!(n)),
            Action::PlanPost(n) => put("plan_post", json!(n)),
            Action::PlanClose(n) => put("plan_close", json!(n)),
            Action::PlanStartDrop(n) => put("plan_start_drop", json!(n)),
            Action::Units { first } => put("units", json!(first)),
            Action::IndexGate { running } => put("index_gate", json!(running)),
            Action::PlanTurn { issue, turn } => {
                put("plan_turn", json!(issue));
                match turn {
                    Turn::First {
                        label,
                        by,
                        refine,
                        title,
                        body,
                        log,
                        brief_extra,
                        epic,
                        auto,
                    } => {
                        put("kind", json!("first"));
                        put("label", json!(label));
                        put("by", json!(by));
                        put("refine", json!(refine));
                        put("title", json!(title));
                        put("body", json!(body));
                        put("log", json!(log));
                        if let Some(b) = brief_extra {
                            put("brief_extra", json!(b));
                        }
                        if let Some(e) = epic {
                            put(crate::text::EPIC, json!(e));
                        }
                        if let Some(r) = auto {
                            put("auto", json!(r));
                        }
                    }
                    Turn::Next {
                        answer,
                        by,
                        log,
                        reply,
                    } => {
                        put("kind", json!("next"));
                        put("answer", json!(answer));
                        put("by", json!(by));
                        put("log", json!(log));
                        if let Some(r) = reply {
                            put("reply", json!(r));
                        }
                    }
                }
            }
            Action::ParkRetry { issue, how } => {
                put("park_retry", json!(issue));
                put("how", json!(how.word()));
            }
            Action::PlanEnd { issue, say } => {
                put("plan_end", json!(issue));
                put("say", json!(say));
            }
            Action::Start(st) => return Value::from(&**st),
        }
        Value::Object(m)
    }
}

impl From<&CycleStart> for Value {
    fn from(st: &CycleStart) -> Self {
        let mut a = json!({
            "start": st.issue, "by": st.by, "how": st.how, "at": st.at,
            "given": st.given, "remove": st.remove, "verb": st.verb,
            "log": st.log, "text": st.text,
            "body_sha256": st.body_sha256, "title_sha256": st.title_sha256,
            "links": st.links,
        });
        let pairs = |p: &[(&'static str, &str)]| -> Map<String, Value> {
            p.iter().map(|(k, v)| (k.to_string(), json!(v))).collect()
        };
        if let Some((base, count)) = &st.rests_on {
            a["rests_on"] = Value::Object(pairs(&base.pairs()));
            a["fixer_starts"] = json!(count);
        }
        if let Some(note) = &st.park_lifted {
            a["park_lifted"] = Value::Object(pairs(&note.pairs()));
        }
        if let Some(spec) = &st.spec {
            a["spec"] = Value::from(spec);
        }
        let x = &st.extra;
        let mut put = |k: &str, v: Value| {
            debug_assert!(!START_BASE.contains(&k), "{k} is a start's own key");
            a[k] = v;
        };
        if let Some(d) = x.driver {
            put("driver", json!(d.word()));
        }
        if let Some(c) = &x.contractor {
            put("contractor", json!(c));
        }
        if let Some(l) = &x.approved_label {
            put("approved_label", json!(l));
        }
        if let Some(r) = &x.retry {
            put("comment", Given::json(r.comment.as_ref()));
            put("since", json!(r.since));
            if r.issue_alone {
                put("issue_alone", json!(true));
            }
        }
        if let Some(r) = &x.refresh {
            put("refresh_pr", json!(r.pr));
            put("refresh_url", json!(r.url));
            put(
                "refresh_of",
                json!({"pr": r.pr, "head_sha": r.head_sha, "base_sha": r.base_sha,
                    "branch": r.branch}),
            );
        }
        a
    }
}

/// {link, sha256, off} for a `Spec:` line, {why, off} for one tehas cannot
/// read.
impl From<&SpecPin> for Value {
    fn from(spec: &SpecPin) -> Self {
        match &spec.read {
            Ok(m) => json!({"link": m.link, "sha256": m.sha256, "off": spec.off}),
            Err(why) => json!({"why": why, "off": spec.off}),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full() -> CycleStart {
        CycleStart {
            issue: 7,
            by: "alice".into(),
            how: "how".into(),
            at: "2026-10-01T00:00:00Z".into(),
            given: "label".into(),
            remove: vec!["tehas:ready".into()],
            verb: "labelled".into(),
            log: "start #7".into(),
            text: true,
            body_sha256: "b".into(),
            title_sha256: Some("t".into()),
            links: vec![],
            rests_on: Some((
                Base {
                    at: "a".into(),
                    by: "alice".into(),
                    how: "label".into(),
                    body_sha256: "b".into(),
                },
                2,
            )),
            park_lifted: Some(ContextNote {
                source: "s".into(),
                by: "b".into(),
                at: "a".into(),
                text: "t".into(),
            }),
            spec: Some(SpecPin {
                read: Err("why".into()),
                off: vec![],
            }),
            extra: StartExtra {
                driver: Some(Driver::Lean),
                contractor: Some("c".into()),
                approved_label: Some("tehas:plan".into()),
                retry: Some(RetryTick {
                    comment: Some(Given::Number(5.into())),
                    since: "s".into(),
                    issue_alone: true,
                }),
                refresh: Some(RefreshOf {
                    pr: 3,
                    url: "u".into(),
                    head_sha: "h".into(),
                    base_sha: "b".into(),
                    branch: "main".into(),
                }),
            },
        }
    }

    #[test]
    fn a_start_s_extra_keys_override_none_of_its_own() {
        let v = Value::from(&Action::Start(Box::new(full())));
        let keys = v.as_object().unwrap().len();
        // Its own keys, and the nine of the extra.
        assert_eq!(keys, START_BASE.len() + 9, "{v}");
    }

    #[test]
    fn only_a_label_change_that_logs_names_its_issue_and_line() {
        let l = LabelChange::new(4, &["tehas:ready"], "f");
        assert_eq!(Action::Label(l.clone()).issue_log(), None);
        assert_eq!(
            Action::Label(l.log("label #4")).issue_log(),
            Some((4, "label #4"))
        );
        let sweep = Action::Sweep {
            issue: 4,
            log: "removed".into(),
        };
        assert_eq!(sweep.issue_log(), None);
    }
}
