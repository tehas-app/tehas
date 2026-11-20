//! What a poll's own decision says of each issue it could start, for
//! state/starts.json: read back from the lines the rules logged, so no rule
//! is asked twice and no reason is written apart from its rule.

use super::{blockers, has, newest_marker, Poll, BLOCKED};
use crate::starts::{code_of, of_issue};
use serde_json::Value;
use tehas_core::starts::{code, Edge, Reason, Record};

impl Poll<'_> {
    /// The ranked issues, then the rest of rule 7's order, each once.
    fn start_issues(&self) -> Vec<u64> {
        let mut out: Vec<u64> = Vec::new();
        if self.s.order.get("state").and_then(Value::as_str) == Some("ok") {
            out.extend(
                self.s
                    .order
                    .get("issues")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_u64),
            );
        }
        for n in &self.cand_order {
            if !out.contains(n) {
                out.push(*n);
            }
        }
        out
    }

    /// The first line the rules logged of `n`, or the log of an action on it.
    fn line_of(&self, n: u64) -> Option<String> {
        let (line, action) = self.cand_from?;
        let own = self.d.log[line..]
            .iter()
            .find_map(|l| of_issue(l, n).map(str::to_string));
        own.or_else(|| {
            self.d.actions[action..]
                .iter()
                .filter(|a| a.get("issue").and_then(Value::as_u64) == Some(n))
                .filter_map(|a| a.get("log").and_then(Value::as_str))
                .find_map(|l| of_issue(l, n).map(str::to_string))
        })
    }

    fn verdict(&self, n: u64) -> Option<Reason> {
        if self.d.picked.contains(&n) {
            return None;
        }
        if let Some(text) = self.line_of(n) {
            return Some(Reason::new(code_of(&text), text));
        }
        let room = || {
            if self.room_hold.is_empty() {
                "the poll had no room for another cycle".to_string()
            } else {
                self.room_hold.clone()
            }
        };
        Some(if self.d.paused {
            Reason::new(code::PAUSED, "the factory is paused")
        } else if self.cand_from.is_none() {
            Reason::new(code::HELD, room())
        } else if !self.cand_order.contains(&n) {
            Reason::new(
                code::NOT_READY,
                "no tehas:ready or tehas:blocked of an allowed account and no approval of tehas start: the poll does not list it as ready",
            )
        } else if !self.cand_seen.contains(&n) {
            Reason::new(
                code::NO_ROOM,
                format!("the poll had no room left when it came to it, so its own holds were not read: {}", room()),
            )
        } else {
            Reason::new(code::OTHER, "the poll held it; the dispatch log says why")
        })
    }

    /// The labels of `n` on the ready and blocked lists.
    fn listed_labels(&self, n: u64) -> Vec<String> {
        [&self.s.ready, &self.s.blocked]
            .into_iter()
            .filter_map(|l| l.0.as_ref())
            .flatten()
            .filter(|i| i.number == Some(n))
            .flat_map(|i| i.labels.clone())
            .collect()
    }

    /// The holds the poll read between issues: the blockers the newest marker
    /// comment names that are not closed, when nobody else edited it, and
    /// the open blocked-by links.
    fn edges(&self, issues: &[u64]) -> Vec<Edge> {
        let mut out = Vec::new();
        for &n in issues {
            let open = self
                .work(n)
                .pointer("/issue_dependencies_summary/blocked_by")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if open > 0 {
                out.push(Edge::Link { issue: n, open });
            }
            if !has(&self.listed_labels(n), BLOCKED) {
                continue;
            }
            let Some(issue) = self.s.issues.get(&n) else {
                continue;
            };
            let Some(nodes) = issue
                .comments
                .as_ref()
                .and_then(|c| c.pointer("/data/repository/issue/comments/nodes"))
                .and_then(Value::as_array)
            else {
                continue;
            };
            let Some(x) = newest_marker(nodes, self.c) else {
                continue;
            };
            let login = |k: &str| {
                x.pointer(&format!("/{k}/login"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string()
            };
            let by = login("author");
            if !x.get("lastEditedAt").unwrap_or(&Value::Null).is_null() && login("editor") != by {
                continue;
            }
            let body = x.get("body").and_then(Value::as_str).unwrap_or("");
            for on in blockers(body).unwrap_or_default() {
                let closed = matches!(
                    issue.blockers.get(&on).map(String::as_str),
                    Some("CLOSED" | "MERGED")
                );
                if !closed {
                    out.push(Edge::Marker {
                        issue: n,
                        on,
                        by: by.clone(),
                    });
                }
            }
        }
        out
    }

    /// What holds a blocker the poll did not decide on, from the lists it read.
    fn hold_of(&self, n: u64) -> Reason {
        let numbered = |v: Option<&Value>| {
            v.and_then(Value::as_array).is_some_and(|l| {
                l.iter()
                    .any(|i| i.get("number").and_then(Value::as_u64) == Some(n))
            })
        };
        let awaiting = self
            .s
            .awaiting
            .as_ref()
            .and_then(|l| l.0.as_ref())
            .is_some_and(|l| l.iter().any(|i| i.number == Some(n)));
        if self.running(n) {
            Reason::new(code::RUNNING, "its cycle is running")
        } else if numbered(self.s.asking.as_ref()) {
            Reason::new(
                code::QUESTION,
                "it carries tehas:question: it waits for an answer",
            )
        } else if self.s.planning.as_ref().is_some_and(|p| p.contains(&n)) {
            Reason::new(code::PLANNING, "a planning session is open on it")
        } else if awaiting || self.s.pulls_name.contains_key(&n) {
            Reason::new(
                code::OPEN_PR,
                "an open pull request is on it: it waits for review or merge",
            )
        } else {
            Reason::new(
                code::NO_APPROVAL,
                "it is open, and no allowed account made it ready",
            )
        }
    }

    /// The record of this poll.
    pub(super) fn starts_record(&self) -> Record {
        let numbers = self.start_issues();
        let issues: Vec<(u64, Option<Reason>)> =
            numbers.iter().map(|n| (*n, self.verdict(*n))).collect();
        let edges = self.edges(&self.cand_order);
        let mut holds: Vec<(u64, Reason)> = Vec::new();
        for e in &edges {
            if let Edge::Marker { on, .. } = e {
                if !numbers.contains(on) && !holds.iter().any(|(h, _)| h == on) {
                    holds.push((*on, self.hold_of(*on)));
                }
            }
        }
        Record {
            issues,
            edges,
            holds,
        }
    }
}
