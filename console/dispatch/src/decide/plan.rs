//! Planning sessions: the turns already under way (rule 4), the answers to a
//! planning question (rule 4 and 5), new sessions (rule 5) and the refine a
//! park is owed (rule 5a). Each turn is the caller's to run (lib/planning.sh);
//! this decides which, with what answer, and says it as the bash does.

use super::clean::iso;
use super::{has, End, Poll, Room, Step};
use crate::text;
use serde_json::{json, Value};

fn digits(v: &Value) -> Option<String> {
    let s = match v {
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        _ => return None,
    };
    (!s.is_empty() && s.len() <= 16 && !s.starts_with('0') && s.bytes().all(|b| b.is_ascii_digit()))
        .then_some(s)
}

/// tehas_plan_shape: the session as plan_view gives it, from plan-show's
/// `.planning`: the refused replies of both names and the held ticks from
/// the notes, and an answer's millwright as fixer.
fn shape(row: &Value) -> Value {
    if !row.is_object() {
        return Value::Null;
    }
    let notes = |k: &str| {
        row.pointer(&format!("/notes/{k}"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let mut refused = notes("millwright_refused");
    refused.extend(notes("fixer_refused"));
    let answers: Vec<Value> = row
        .get("answers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|a| match a.get("millwright") {
            Some(Value::Bool(m)) => json!({"turn": a.get("turn"), "by": a.get("by"), "fixer": m}),
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
        v["fixer_refused"] = json!(refused);
    }
    let held = notes("held");
    if !held.is_empty() {
        v["held"] = json!(held);
    }
    v
}

/// Whether a list of the record holds the id, a number as jq's index finds
/// it.
fn listed(v: &Value, key: &str, id: &str) -> bool {
    v.get(key).and_then(Value::as_array).is_some_and(|a| {
        a.iter()
            .any(|x| x.as_u64().is_some_and(|v| v.to_string() == id))
    })
}

impl Poll<'_> {
    /// The open sessions whose turn is running or waits to be posted, before
    /// any question is read. Returns the sessions this step took.
    pub(super) fn plan_turns(&mut self) -> Vec<u64> {
        let mut fresh = Vec::new();
        for s in self.s.sessions.clone().iter().filter(|s| s.running) {
            let n = s.issue;
            if s.in_flight {
                self.plan_busy = Some(n);
            } else if self.dry {
                if s.rc {
                    self.log(format!("would post the planning turn of #{n} that ended"));
                } else {
                    self.log(format!("would report the cut-off planning turn of #{n}"));
                }
            } else {
                self.act(json!({"plan_collect": n}));
            }
            fresh.push(n);
        }
        for s in self.s.sessions.clone().iter().filter(|s| s.pending) {
            let n = s.issue;
            if fresh.contains(&n) {
                continue;
            }
            if self.dry {
                self.log(format!("would post the waiting planning turn of #{n}"));
            } else {
                self.act(json!({"plan_post": n}));
            }
            fresh.push(n);
        }
        fresh
    }

    /// A session whose issue lost its question label, or was closed, is over.
    pub(super) fn plan_over(&mut self, fresh: &[u64], asking: &[u64]) {
        for s in self.s.sessions.clone() {
            let n = s.issue;
            if fresh.contains(&n) || asking.contains(&n) {
                continue;
            }
            // A dry run closes nothing: the session stays open for the rules
            // after this one.
            if !self.dry {
                self.act(json!({"plan_close": n}));
                self.plan_closed.push(n);
            }
        }
    }

    fn plan_end(&mut self, n: u64, text: &str) {
        self.act(json!({"plan_end": n, "say": text}));
    }

    /// tehas_fixer_answer: why the fixer may not answer this session's
    /// question, empty when it may.
    fn fixer_answer_why(&self, view: &Value) -> String {
        if self.c.fixer.is_empty() {
            return "there is no fixer".into();
        }
        if !view.is_object() {
            return "the session has no record on this host".into();
        }
        let by = view.get("by").and_then(Value::as_str).unwrap_or("");
        if by.is_empty() || !self.c.allowed(by) {
            return "no person's tehas:plan started this session, as far as its record says".into();
        }
        let answers = match view.get("answers") {
            None | Some(Value::Null) => Some(Vec::new()),
            Some(Value::Array(a)) => a
                .iter()
                .map(|x| x.get("fixer").and_then(Value::as_bool))
                .collect::<Option<Vec<bool>>>(),
            Some(_) => None,
        };
        let Some(answers) = answers else {
            return "its record does not say who answered its turns".into();
        };
        let had = answers.iter().filter(|f| **f).count();
        if answers.last() == Some(&true) {
            return "the fixer answered the question before this one; a person answers this one"
                .into();
        }
        if had as i64 >= self.c.fixer_max_plan_answers {
            return format!("the fixer has answered {had} questions in this session, the limit");
        }
        String::new()
    }

    /// fixer_answer_ok: whether the fixer's tick or reply may run the turn.
    fn fixer_answer_ok(&mut self, n: u64, view: &Value, ans: &Value, said: &str) -> bool {
        let boxes = ans["boxes"].as_u64().unwrap_or(0);
        let why = if ans["tick"]["status"] == "fixer"
            && ans["tick"]["index"].as_u64().map(|i| i + 1) == Some(boxes)
        {
            "it ticked End this planning session, which only a person may".to_string()
        } else if said == "reply" && ans["fixer_reply"]["marked"] != json!(true) {
            format!("its reply does not start with <!-- tehas:fixer issue={n} action=answer -->")
        } else {
            self.fixer_answer_why(view)
        };
        if why.is_empty() {
            return true;
        }
        if said == "tick" {
            self.log(format!("skip #{n}: the fixer's answer runs no turn: {why}"));
            if !self.dry {
                let note = format!("The fixer's tick was not taken as an answer: {why}. It was unticked. A person answers this question.");
                self.untick(n, ans, Some(&note));
            }
            return false;
        }
        let Some(id) = digits(&ans["fixer_reply"]["id"]) else {
            return false;
        };
        if view
            .get("fixer_refused")
            .is_some_and(|v| !v.is_array() && !v.is_null())
            || listed(view, "fixer_refused", &id)
        {
            return false;
        }
        self.log(format!("skip #{n}: the fixer's answer runs no turn: {why}"));
        if !self.dry {
            self.act(json!({"plan_note": n, "kind": "fixer-refused", "id": id,
                "say": format!("Tehas did not take the fixer's reply as an answer: {why}. A person answers this question.")}));
        }
        false
    }

    /// fixer_held: a tick the fixer saved after someone else edited the
    /// question is neither used nor undone. Said once a question.
    fn fixer_held(&mut self, n: u64, view: &Value, ans: &Value) {
        let Some(id) = digits(&ans["comment"]) else {
            return;
        };
        if view
            .get("held")
            .is_some_and(|v| !v.is_array() && !v.is_null())
            || listed(view, "held", &id)
        {
            return;
        }
        self.log(format!("skip #{n}: the fixer saved the question after another account did, so who ticked it is not known"));
        if !self.dry {
            self.act(json!({"plan_note": n, "kind": "held", "id": id,
                "say": "Tehas did not take the box ticked in this question as an answer: the fixer saved the comment after another account did, and GitHub does not say who ticked it. A person ticks the box again or replies."}));
        }
    }

    /// The answer to a planning question: the next turn runs with it, and
    /// the poll ends there.
    pub(super) fn plan_answer(
        &mut self,
        n: u64,
        ans: &Value,
        tick: &str,
        row: &Value,
        room: &Room,
    ) -> Step {
        let mut by = ans["tick"]["by"].as_str().unwrap_or("").to_string();
        let mut reply = ans["reply"]["text"]
            .as_str()
            .unwrap_or("")
            .trim_end_matches('\n')
            .to_string();
        let mut reply_id = String::new();
        let mut fixer_said = "";
        if !reply.is_empty() {
            by = ans["reply"]["by"].as_str().unwrap_or("").to_string();
            reply_id = digits(&ans["reply"]["id"]).unwrap_or_default();
        } else if tick == "one" {
        } else if tick == "held" {
            if row.is_object() {
                self.fixer_held(n, &shape(row), ans);
            }
            return Ok(());
        } else if !ans["fixer_reply"].is_null() {
            fixer_said = "reply";
        } else if tick == "fixer" {
            fixer_said = "tick";
        } else {
            return Ok(());
        }
        if row.is_null() {
            if !self.dry {
                self.plan_end(n, "The record of this planning session is gone from the host, so it cannot continue. Apply `tehas:plan` to start a new one.");
            }
            return Ok(());
        }
        let wanted = match row.get("comment_id") {
            None | Some(Value::Null) => String::new(),
            Some(Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
        };
        let answered = match &ans["comment"] {
            Value::String(s) => s.clone(),
            v => v.to_string(),
        };
        if wanted != answered {
            self.log(format!(
                "skip #{n}: the question answered is not the one this session is waiting on"
            ));
            return Ok(());
        }
        let mut speaker = "The operator";
        if !fixer_said.is_empty() {
            if !row.is_object() {
                self.log(format!(
                    "skip #{n}: its planning session on record could not be read"
                ));
                return Ok(());
            }
            if !self.fixer_answer_ok(n, &shape(row), ans, fixer_said) {
                return Ok(());
            }
            speaker = "The fixer";
            by = self.c.fixer.clone();
            if fixer_said == "reply" {
                reply = ans["fixer_reply"]["text"]
                    .as_str()
                    .unwrap_or("")
                    .trim_end_matches('\n')
                    .to_string();
                reply_id = digits(&ans["fixer_reply"]["id"]).unwrap_or_default();
            }
        }
        let boxes = ans["boxes"].as_u64().unwrap_or(0);
        let index = ans["tick"]["index"].as_u64();
        // As `$(jq -r ...)` leaves it: no final newlines.
        let option = |i: u64| {
            row.get("options")
                .and_then(|o| o.get(i as usize))
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim_end_matches('\n')
                .to_string()
        };
        let text = if !reply.is_empty() {
            let mut text = format!("{speaker} replied on the issue:\n\n{reply}");
            if tick == "one" {
                let i = index.unwrap_or(u64::MAX);
                let choice = if i.checked_add(1) == Some(boxes) {
                    "End this planning session".to_string()
                } else {
                    Some(option(i))
                        .filter(|c| !c.is_empty())
                        .unwrap_or_else(|| "a box that matches no option on record".into())
                };
                let ticker = ans["tick"]["by"].as_str().unwrap_or("");
                text = format!("{text}\n\nA box was also ticked, by {ticker}: {choice}");
            }
            text
        } else {
            let i = index.unwrap_or(u64::MAX);
            if i.checked_add(1) == Some(boxes) {
                if !fixer_said.is_empty() {
                    self.log(format!("skip #{n}: the fixer's answer runs no turn: it would end the session, which only a person may"));
                    return Ok(());
                }
                if !self.dry {
                    self.plan_end(n, &format!("Planning session ended by {by}."));
                }
                return Ok(());
            }
            let choice = option(i);
            if choice.is_empty() {
                self.log(format!(
                    "skip #{n}: the ticked box matches no option on record"
                ));
                return Ok(());
            }
            format!("{speaker} chose: {choice}")
        };
        if let Some(busy) = self.plan_busy {
            self.log(format!(
                "planning #{n} waits: the turn of #{busy} is still running"
            ));
            return Ok(());
        }
        if !self.work_ok(n)? || !room.slice_ok || !room.limit_ok {
            return Ok(());
        }
        self.mesh_ok()?;
        let from = if fixer_said.is_empty() {
            by.clone()
        } else {
            format!("{by}, the fixer")
        };
        if self.dry {
            self.log(format!(
                "would run a planning turn for #{n} (answer from {from})"
            ));
            return Err(End(0));
        }
        self.log(format!("planning #{n} continues (answer from {from})"));
        let mut a = json!({"plan_turn": n, "kind": "next", "answer": text, "by": by});
        if !reply_id.is_empty() {
            a["reply"] = json!(reply_id);
        }
        self.act(a);
        Err(End(0))
    }

    fn refine_path(&self) -> String {
        format!("{}/../lib/refine-brief.md", self.c.libexec)
    }

    /// Rule 5: a tehas:plan or tehas:refine of an allowed account starts a
    /// session, and the poll ends there.
    pub(super) fn plan_new(&mut self, room: &Room) -> Step {
        let mut lists = Vec::new();
        for l in [self.s.plans.clone(), self.s.refines.clone()] {
            let Some(l) = l else {
                self.need("plans");
                return Ok(());
            };
            if l.get("failed") == Some(&json!(true)) {
                return Err(self.fail("ERROR: gh issue list failed", 1));
            }
            let Some(a) = l.as_array() else {
                return Err(self.fail("ERROR: unreadable list of issues to plan", 1));
            };
            lists.extend(a.clone());
        }
        let mut entries: Vec<(u64, &'static str)> = Vec::new();
        for i in &lists {
            let names: Vec<String> = i
                .get("labels")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|l| l.get("name").and_then(Value::as_str).map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            if has(&names, "tehas:planning") || has(&names, "tehas:running") {
                continue;
            }
            let Some(n) = i.get("number").and_then(Value::as_u64) else {
                return Err(self.fail("ERROR: unreadable list of issues to plan", 1));
            };
            let label = if has(&names, "tehas:plan") {
                "tehas:plan"
            } else {
                "tehas:refine"
            };
            entries.push((n, label));
        }
        // unique_by(.number): sorted, the first of each number kept.
        entries.sort_by_key(|(n, _)| *n);
        entries.dedup_by_key(|(n, _)| *n);
        for (n, label) in entries {
            if let Some(busy) = self.plan_busy {
                self.log(format!(
                    "planning #{n} waits: the turn of #{busy} is still running"
                ));
                break;
            }
            if !room.slice_ok {
                break;
            }
            if self.control_ask(n, &format!("`{label}`")) {
                continue;
            }
            if !room.limit_plan_ok {
                if self.dry {
                    self.log(format!(
                        "planning #{n} waits: the limit holds new planning turns"
                    ));
                }
                break;
            }
            match self.s.issues.get(&n).and_then(|i| i.view.as_ref()) {
                None => {
                    self.need("view");
                    return Ok(());
                }
                Some(v) if v.get("failed") == Some(&json!(true)) => {
                    return Err(self.fail("ERROR: gh issue view failed", 1));
                }
                Some(_) => {}
            }
            let who = match self.approval(n, label)? {
                Ok((who, _)) => who,
                Err(refused) => {
                    if !refused.is_empty() {
                        self.clear_unauthorised(n, label, &refused);
                    }
                    continue;
                }
            };
            if !self.work_ok(n)? {
                continue;
            }
            self.mesh_ok()?;
            let refine = label == "tehas:refine";
            if refine && !self.s.refine_brief {
                let path = self.refine_path();
                self.log(format!(
                    "ERROR: no refine brief at {path}, not refining #{n}"
                ));
                continue;
            }
            let to = if refine { ", to refine it" } else { "" };
            if self.dry {
                self.log(format!(
                    "would start a planning session for #{n} (labelled by {who}{to})"
                ));
                return Err(End(0));
            }
            self.log(format!("start planning #{n} (labelled by {who}{to})"));
            self.act(json!({"plan_turn": n, "kind": "first", "label": label, "by": who, "refine": refine}));
            return Err(End(0));
        }
        Ok(())
    }

    /// Rule 5a: a park the store says is owed a refine starts one, unless a
    /// person has acted on the issue since. One that cannot start is spent.
    pub(super) fn plan_refine(&mut self, room: &Room) -> Step {
        let owed = match self.s.refines_owed.clone() {
            None => {
                self.need("refines_owed");
                return Ok(());
            }
            Some(Err(e)) => {
                let rc = if e.rc == 1 { 1 } else { 78 };
                return Err(self.fail(
                    format!(
                        "ERROR: the state store could not list the parks owed a refine ({})",
                        e.word
                    ),
                    rc,
                ));
            }
            Some(Ok(o)) => o,
        };
        for p in owed {
            let (Some(n), Some(started)) = (
                p.get("issue").and_then(Value::as_u64),
                p.get("started_at").and_then(Value::as_u64),
            ) else {
                continue;
            };
            if n == 0 || n > 9_999_999_999 || started > 99_999_999_999 {
                continue;
            }
            let field = |k: &str| match p.get(k) {
                None | Some(Value::Null) | Some(Value::Bool(false)) => "-".to_string(),
                Some(Value::String(s)) => s.clone(),
                Some(v) => v.to_string(),
            };
            let (by, reason, qid, sum) = (
                field("started_by"),
                field("park_reason"),
                field("question"),
                field("body_sha256"),
            );
            if let Some(busy) = self.plan_busy {
                self.log(format!(
                    "refining #{n} waits: the turn of #{busy} is still running"
                ));
                break;
            }
            if !room.slice_ok {
                break;
            }
            if !room.limit_plan_ok {
                if self.dry {
                    self.log(format!(
                        "refining #{n} waits: the limit holds new planning turns"
                    ));
                }
                break;
            }
            self.mesh_ok()?;
            if self.park_ticked.contains(&n) {
                self.log(format!(
                    "skip #{n}: not refined: a box of its park question is ticked"
                ));
                continue;
            }
            let mut why = String::new();
            if self.control_ask(
                n,
                &format!("a cycle that parked as `{reason}` asked to be refined"),
            ) {
                why = "it is the control issue".into();
            } else {
                let Some(view) = self.s.issues.get(&n).and_then(|i| i.view.clone()) else {
                    self.need("view");
                    return Ok(());
                };
                if view.get("failed") == Some(&json!(true)) {
                    return Err(self.fail("ERROR: gh issue view failed", 1));
                }
                let have: Vec<String> = view
                    .get("labels")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|l| {
                                l.get("name").and_then(Value::as_str).map(str::to_string)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                for l in [
                    "tehas:ready",
                    "tehas:running",
                    "tehas:blocked",
                    "tehas:plan",
                    "tehas:refine",
                    "tehas:refined",
                    "tehas:replaced",
                ] {
                    if has(&have, l) {
                        why = format!("it carries {l}");
                    }
                }
                if view.get("state").and_then(Value::as_str) != Some("OPEN") {
                    why = "it is closed".into();
                }
                if !(has(&have, "needs-human-attention") && has(&have, "tehas:question")) {
                    why = "it is no longer parked".into();
                }
                if qid == "-" {
                    why = "its park question was answered".into();
                }
                let line = view
                    .get("body")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .split('\n')
                    .next()
                    .unwrap_or("");
                // rtrimstr("\r"): one carriage return.
                let first = line.strip_suffix('\r').unwrap_or(line).to_string();
                let replaces = first
                    .strip_prefix("Replaces #")
                    .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()));
                if replaces {
                    why = "a planning session filed it".into();
                }
                if !self.c.allowed(&by) {
                    why = format!("its cycle was not started by an account that may label ({by})");
                }
                if sum != "-" && sum != text::body_sha256(&view) {
                    why = "its text changed since its cycle began".into();
                }
                if why.is_empty()
                    && !self.unchanged_since(
                        n,
                        &iso(started),
                        "its last cycle began",
                        "apply tehas:refine",
                    )?
                {
                    why = "its text changed since its cycle began".into();
                }
            }
            if !why.is_empty() {
                self.log(format!("skip #{n}: not refined: {why}"));
                if !self.dry {
                    self.act(json!({"park_retry": n}));
                }
                continue;
            }
            if !self.s.refine_brief {
                let path = self.refine_path();
                self.log(format!(
                    "ERROR: no refine brief at {path}, not refining #{n}"
                ));
                break;
            }
            let line = format!("refining #{n} (its cycle parked as {reason}, started by {by})");
            if self.dry {
                self.log(format!("would start {line}"));
                return Err(End(0));
            }
            self.act(json!({"park_retry": n}));
            self.log(format!("start {line}"));
            self.act(
                json!({"plan_turn": n, "kind": "first", "label": "tehas:refine", "by": by,
                "refine": true, "auto": reason}),
            );
            return Err(End(0));
        }
        Ok(())
    }
}
