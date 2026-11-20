//! What the dispatcher's last poll decided of each issue it could start, and
//! the holds between issues it read: `state/starts.json`. The poll writes it
//! (console/dispatch), `tehas order --json` and `tehas blockers` read it, so
//! the reasons a host is shown are the poll's own and cannot drift from it.
//!
//! The file is internal; the objects the two commands print are the API. A
//! reason is a code and a line: the codes are an open list, and a reader
//! takes a code it does not know as a hold it cannot name.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Where the record is kept under the home.
pub fn path(home: &Path) -> PathBuf {
    home.join("state/starts.json")
}

/// The codes of a reason. The list only grows; `OTHER` is any hold the poll
/// logged that no code below names.
pub mod code {
    /// The poll was paused.
    pub const PAUSED: &str = "paused";
    /// The factory held every start: the hold's own line says why.
    pub const HELD: &str = "held";
    /// Ready, but the poll had no room left when it came to it; its own holds
    /// were not read.
    pub const NO_ROOM: &str = "no-room";
    /// No `tehas:ready` or `tehas:blocked` of an allowed account and no host
    /// approval: the poll did not list it as ready.
    pub const NOT_READY: &str = "not-ready";
    /// The last poll did not see it: it was ranked after that poll.
    pub const UNSEEN: &str = "unseen";
    pub const NOT_IN_ORDER: &str = "not-in-order";
    pub const RUNNING: &str = "running";
    pub const PLANNING: &str = "planning";
    pub const QUESTION: &str = "question";
    pub const CONTROL: &str = "control-issue";
    pub const CLOSED: &str = "closed";
    pub const NOT_ISSUE: &str = "not-issue";
    pub const BLOCKED: &str = "blocked";
    pub const BLOCKED_LINK: &str = "blocked-link";
    pub const OPEN_PR: &str = "open-pr";
    pub const EPIC: &str = "epic";
    pub const TOO_LARGE: &str = "too-large";
    pub const PARKED: &str = "parked";
    pub const TEXT_CHANGED: &str = "text-changed";
    pub const NOT_ALLOWED: &str = "not-allowed";
    pub const LABEL_OFF: &str = "label-off";
    pub const MILLWRIGHT: &str = "millwright";
    pub const DRIVER: &str = "driver";
    pub const ATTACHMENT: &str = "attachment";
    pub const SPEC: &str = "spec";
    pub const READ_ONLY: &str = "read-only";
    pub const REFRESH_LANE: &str = "refresh-lane";
    pub const UNREADABLE: &str = "unreadable";
    pub const NO_APPROVAL: &str = "no-approval";
    pub const OTHER: &str = "other";
}

/// Why an issue would not start now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reason {
    pub code: String,
    pub text: String,
}

impl Reason {
    pub fn new(code: &str, text: impl Into<String>) -> Reason {
        Reason {
            code: code.to_string(),
            text: text.into(),
        }
    }

    pub fn to_value(&self) -> Value {
        json!({"code": self.code, "text": self.text})
    }

    fn parse(v: &Value) -> Option<Reason> {
        let s = |k: &str| {
            v.get(k)?
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        Some(Reason {
            code: s("code")?,
            text: s("text")?,
        })
    }
}

/// One hold between two issues, or of an issue by its links.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edge {
    /// The newest marker comment of `issue` names `on`, which is open or of a
    /// state GitHub did not give; `by` wrote the comment.
    Marker { issue: u64, on: u64, by: String },
    /// `open` blocked-by links of `issue` are open. Which issues they name is
    /// not read.
    Link { issue: u64, open: u64 },
}

impl Edge {
    pub fn issue(&self) -> u64 {
        match self {
            Edge::Marker { issue, .. } | Edge::Link { issue, .. } => *issue,
        }
    }

    pub fn to_value(&self) -> Value {
        match self {
            Edge::Marker { issue, on, by } => {
                json!({"issue": issue, "kind": "marker", "on": on, "by": by})
            }
            Edge::Link { issue, open } => json!({"issue": issue, "kind": "link", "open": open}),
        }
    }

    fn parse(v: &Value) -> Option<Edge> {
        let n = |k: &str| v.get(k)?.as_u64().filter(|n| *n > 0);
        match v.get("kind")?.as_str()? {
            "marker" => Some(Edge::Marker {
                issue: n("issue")?,
                on: n("on")?,
                by: v.get("by")?.as_str()?.to_string(),
            }),
            "link" => Some(Edge::Link {
                issue: n("issue")?,
                open: n("open")?,
            }),
            _ => None,
        }
    }
}

/// What one poll decided. `issues` holds `None` for an issue the poll chose
/// to start, a reason for one it did not.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Record {
    pub issues: Vec<(u64, Option<Reason>)>,
    pub edges: Vec<Edge>,
    /// What holds an issue the edges name that the poll did not decide on.
    pub holds: Vec<(u64, Reason)>,
}

impl Record {
    /// The decision of `n`: Some(None) to start, Some(reason), or None when
    /// the poll did not decide on it.
    pub fn verdict(&self, n: u64) -> Option<Option<&Reason>> {
        self.issues
            .iter()
            .find(|(i, _)| *i == n)
            .map(|(_, r)| r.as_ref())
    }

    /// What holds `n`: its own decision's reason, else the hold read of it.
    pub fn hold_of(&self, n: u64) -> Option<&Reason> {
        match self.verdict(n) {
            Some(r) => r,
            None => self.holds.iter().find(|(i, _)| *i == n).map(|(_, r)| r),
        }
    }

    /// The file, stamped with `at`.
    pub fn to_value(&self, at: &str) -> Value {
        let issues: Vec<Value> = self
            .issues
            .iter()
            .map(|(n, r)| match r {
                None => json!({"issue": n, "start": true}),
                Some(r) => json!({"issue": n, "start": false, "reason": r.to_value()}),
            })
            .collect();
        let holds: Vec<Value> = self
            .holds
            .iter()
            .map(|(n, r)| json!({"issue": n, "reason": r.to_value()}))
            .collect();
        let edges: Vec<Value> = self.edges.iter().map(Edge::to_value).collect();
        json!({"at": at, "issues": issues, "edges": edges, "holds": holds})
    }

    /// The file back with its stamp, or None for one that is not a record. An
    /// entry this release cannot read is dropped.
    pub fn parse(v: &Value) -> Option<(String, Record)> {
        let at = v.get("at")?.as_str()?.to_string();
        let list = |k: &str| v.get(k).and_then(Value::as_array).cloned();
        let num = |e: &Value| e.get("issue")?.as_u64().filter(|n| *n > 0);
        let issues = list("issues")?
            .iter()
            .filter_map(|e| {
                let n = num(e)?;
                match e.get("start")?.as_bool()? {
                    true => Some((n, None)),
                    false => Some((n, Some(Reason::parse(e.get("reason")?)?))),
                }
            })
            .collect();
        let edges = list("edges")?.iter().filter_map(Edge::parse).collect();
        let holds = list("holds")?
            .iter()
            .filter_map(|e| Some((num(e)?, Reason::parse(e.get("reason")?)?)))
            .collect();
        Some((
            at,
            Record {
                issues,
                edges,
                holds,
            },
        ))
    }
}

/// A record that is there and cannot be read.
#[derive(Debug, PartialEq, Eq)]
pub struct Unreadable;

/// The record on disk: Ok(None) when there is none, Err when it is there and
/// cannot be read.
pub fn read(home: &Path) -> Result<Option<(String, Record)>, Unreadable> {
    match std::fs::read(path(home)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(Unreadable),
        Ok(b) => serde_json::from_slice(&b)
            .ok()
            .as_ref()
            .and_then(Record::parse)
            .map(Some)
            .ok_or(Unreadable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_none_and_a_broken_one_is_unreadable() {
        let home = std::env::temp_dir().join(format!("tehas-starts-{}", std::process::id()));
        std::fs::create_dir_all(home.join("state")).unwrap();
        assert_eq!(read(&home), Ok(None));
        std::fs::write(path(&home), "{\"at\": \"t\"").unwrap();
        assert_eq!(read(&home), Err(Unreadable));
        let r = Record::default();
        std::fs::write(path(&home), r.to_value("t").to_string()).unwrap();
        assert_eq!(read(&home), Ok(Some(("t".to_string(), r))));
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn the_file_reads_back_as_written() {
        let r = Record {
            issues: vec![
                (12, None),
                (
                    8,
                    Some(Reason::new(
                        code::BLOCKED,
                        "tehas:blocked holds it: #3 is open",
                    )),
                ),
            ],
            edges: vec![
                Edge::Marker {
                    issue: 8,
                    on: 3,
                    by: "alice".into(),
                },
                Edge::Link { issue: 9, open: 2 },
            ],
            holds: vec![(3, Reason::new(code::QUESTION, "it waits for an answer"))],
        };
        let v = r.to_value("2026-11-20T10:00:00Z");
        assert_eq!(
            Record::parse(&v),
            Some(("2026-11-20T10:00:00Z".to_string(), r.clone()))
        );
        assert_eq!(r.verdict(12), Some(None));
        assert_eq!(r.hold_of(8).map(|r| r.code.as_str()), Some(code::BLOCKED));
        assert_eq!(r.hold_of(3).map(|r| r.code.as_str()), Some(code::QUESTION));
        assert_eq!(r.verdict(4), None);
    }

    #[test]
    fn an_entry_it_cannot_read_is_dropped_and_a_file_of_another_shape_is_none() {
        let v = json!({"at": "t", "issues": [{"issue": 0, "start": true}, {"issue": 5, "start": false}, {"issue": 6, "start": false, "reason": {"code": "new-hold", "text": "a hold of a later release"}}],
                       "edges": [{"issue": 1, "kind": "rope", "on": 2}], "holds": []});
        let (_, r) = Record::parse(&v).unwrap();
        assert_eq!(
            r.issues,
            vec![(
                6,
                Some(Reason::new("new-hold", "a hold of a later release"))
            )]
        );
        assert!(r.edges.is_empty());
        assert_eq!(Record::parse(&json!({"issues": []})), None);
    }
}
