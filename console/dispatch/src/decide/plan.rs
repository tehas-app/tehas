//! Planning sessions: the turns already under way (rule 4), the answers to a
//! planning question (rule 4 and 5), new sessions (rule 5) and the refine a
//! park is owed (rule 5a). Each turn is the caller's to run (apply/turn.rs);
//! this decides which, with what answer, and says it.

use super::clean::iso;

/// Why the fixer may not answer this session's question, empty when it may.
fn fixer_why(c: &Settings, view: Option<&PlanView>) -> String {
    if c.millwright.is_empty() {
        return "there is no fixer".into();
    }
    let Some(view) = view else {
        return "the session has no record on this host".into();
    };
    let by = view.by.as_str();
    if by.is_empty() || !c.allowed(by) {
        return "no person's tehas:plan started this session, as far as its record says".into();
    }
    let Some(answers) = &view.fixers else {
        return "its record does not say who answered its turns".into();
    };
    let had = answers.iter().filter(|f| **f).count();
    if answers.last() == Some(&true) {
        return "the fixer answered the question before this one; a person answers this one".into();
    }
    if had as i64 >= c.fixer_max_plan_answers {
        return format!("the fixer has answered {had} questions in this session, the limit");
    }
    String::new()
}
/// fixer_why of a session in `shape`'s JSON.
#[cfg(test)]
fn fixer_answer_why(c: &Settings, view: &Value) -> String {
    fixer_why(c, crate::snapshot::plan_view(view).as_ref())
}
use super::{has, or_unknown, Action, End, NoteKind, Poll, Room, Spend, Step, Turn};
use crate::questions::Asked;
use crate::settings::Settings;
#[cfg(test)]
use crate::snapshot::shape;
use crate::snapshot::{Given, PlanList, PlanRow, PlanView, View};
use crate::text;
use serde_json::json;
#[cfg(test)]
use serde_json::Value;

/// The largest issue number and the latest cycle start (epoch seconds) a
/// park owed a refine is read with.
const ISSUE_MAX: u64 = 9_999_999_999;
const TIME_MAX: u64 = 99_999_999_999;

fn digits(v: Option<&Given>) -> Option<String> {
    let s = match v? {
        Given::Number(n) => n.to_string(),
        Given::Text(s) => s.clone(),
        Given::Other(_) => return None,
    };
    (!s.is_empty() && s.len() <= 16 && !s.starts_with('0') && s.bytes().all(|b| b.is_ascii_digit()))
        .then_some(s)
}

/// Whether a list of the record holds the id, a number as jq's index finds
/// it. A list that is not one holds every id.
fn listed(ids: &Option<Vec<u64>>, id: &str) -> bool {
    ids.as_ref()
        .is_none_or(|a| a.iter().any(|v| v.to_string() == id))
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
                self.act(Action::PlanCollect(n));
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
                self.act(Action::PlanPost(n));
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
                self.act(Action::PlanClose(n));
                self.plan_closed.push(n);
            }
        }
    }

    fn plan_end(&mut self, n: u64, text: &str) {
        self.act(Action::PlanEnd {
            issue: n,
            say: text.to_string(),
        });
    }

    /// fixer_answer_ok: whether the fixer's reply may run the turn.
    fn fixer_answer_ok(&mut self, n: u64, view: &PlanView, ans: &Asked) -> bool {
        let why = fixer_why(self.c, Some(view));
        if why.is_empty() {
            return true;
        }
        let Some(id) = digits(ans.fixer_reply.as_ref().and_then(|f| f.id.as_ref())) else {
            return false;
        };
        if listed(&view.refused, &id) {
            return false;
        }
        self.log(format!("skip #{n}: the fixer's answer runs no turn: {why}"));
        if !self.dry {
            self.act(Action::PlanNote {
                issue: n,
                kind: NoteKind::MillwrightRefused,
                id,
                say: format!("Tehas did not take the fixer's reply as an answer: {why}. A person answers this question."),
            });
        }
        false
    }

    /// The answer to a planning question: the next turn runs with it, and
    /// the poll ends there.
    pub(super) fn plan_answer(
        &mut self,
        n: u64,
        ans: &Asked,
        tick: &str,
        row: &PlanRow,
        room: &Room,
    ) -> Step {
        let mut by = ans.tick.by.clone();
        let mut reply = ans
            .reply
            .as_ref()
            .and_then(|r| r.text.as_ref())
            .and_then(Given::text)
            .unwrap_or("")
            .trim_end_matches('\n')
            .to_string();
        let mut reply_id = String::new();
        let mut fixer_said = "";
        if !reply.is_empty() {
            let r = ans.reply.as_ref();
            by = r.map(|r| r.by.clone()).unwrap_or_default();
            reply_id = digits(r.and_then(|r| r.id.as_ref())).unwrap_or_default();
        } else if tick == "one" {
        } else if ans.fixer_reply.is_some() {
            fixer_said = "reply";
        } else {
            return Ok(());
        }
        if *row == PlanRow::None {
            if !self.dry {
                self.plan_end(n, "The record of this planning session is gone from the host, so it cannot continue. Apply `tehas:plan` to start a new one.");
            }
            return Ok(());
        }
        let open = match row {
            PlanRow::Open(p) => Some(&**p),
            _ => None,
        };
        let wanted = open.map_or("", |p| p.comment_id.as_str());
        let answered = Given::said(ans.comment.as_ref());
        if wanted != answered {
            self.log(format!(
                "skip #{n}: the question answered is not the one this session is waiting on"
            ));
            return Ok(());
        }
        let mut speaker = "The operator";
        if !fixer_said.is_empty() {
            let Some(p) = open else {
                self.log(format!(
                    "skip #{n}: its planning session on record could not be read"
                ));
                return Ok(());
            };
            if !self.fixer_answer_ok(n, &p.view, ans) {
                return Ok(());
            }
            speaker = "The fixer";
            by = self.c.millwright.clone();
            if fixer_said == "reply" {
                let f = ans.fixer_reply.as_ref();
                reply = f
                    .map_or("", |f| f.text.as_str())
                    .trim_end_matches('\n')
                    .to_string();
                reply_id = digits(f.and_then(|f| f.id.as_ref())).unwrap_or_default();
            }
        }
        let boxes = ans.boxes as u64;
        let mut index = ans.tick.index.map(|i| i as u64);
        // The fixer's reply that names a choice makes it, as a person's does.
        if fixer_said == "reply" {
            if let Some(i) = crate::questions::choice_of(&reply, "plan", &ans.labels) {
                index = Some(i as u64);
                reply.clear();
            }
        }
        // As `$(jq -r ...)` leaves it: no final newlines.
        let option = |i: u64| {
            open.and_then(|p| p.options.get(i as usize))
                .cloned()
                .unwrap_or_default()
        };
        let text = if !reply.is_empty() {
            format!("{speaker} replied on the issue:\n\n{reply}")
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
                    "skip #{n}: the choice made matches no option on record"
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
        // Its line is logged once the answer is written, as the bash does.
        self.act(Action::PlanTurn {
            issue: n,
            turn: Turn::Next {
                answer: text,
                by,
                log: format!("planning #{n} continues (answer from {from})"),
                reply: (!reply_id.is_empty()).then_some(reply_id),
            },
        });
        Err(End(0))
    }

    /// The title and body of the view the poll read, which a first turn's
    /// brief is written from.
    fn brief_text(&self, n: u64) -> (Option<String>, Option<String>) {
        match self.s.issues.get(&n).and_then(|i| i.view.clone()) {
            Some(View::Read(v)) => (v.title, v.body),
            _ => (None, None),
        }
    }

    fn refine_path(&self) -> String {
        format!("{}/../lib/refine-brief.md", self.c.libexec)
    }

    /// Rule 5: a tehas:plan or tehas:refine of an allowed account starts a
    /// session, and the poll ends there. A list that failed or is no list
    /// starts no session; the poll goes on, as with a failed history of
    /// refreshes in `refreshing`.
    pub(super) fn plan_new(&mut self, room: &Room) -> Step {
        let mut lists = Vec::new();
        let mut unreadable = false;
        for l in [self.s.plans.clone(), self.s.refines.clone()] {
            match l {
                None => {
                    self.need("plans");
                    return Ok(());
                }
                Some(PlanList::Read(a)) => lists.extend(a),
                Some(PlanList::Unreadable) => unreadable = true,
            }
        }
        if unreadable {
            self.log(
                "WARN: the issues to plan could not be read, so no planning session starts this poll"
                    .to_string(),
            );
            return Ok(());
        }
        let mut entries: Vec<(u64, &'static str)> = Vec::new();
        for i in &lists {
            let names = &i.labels;
            if has(names, "tehas:planning") || has(names, "tehas:running") {
                continue;
            }
            let Some(n) = i.number else {
                return Err(self.fail("ERROR: unreadable list of issues to plan", 1));
            };
            let label = if has(names, "tehas:plan") {
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
            if self.read_only_ran(n) {
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
                Some(View::Failed) => {
                    return Err(self.fail("ERROR: gh issue view failed", 1));
                }
                Some(View::Read(_)) => {}
            }
            let mut on_epic = String::new();
            let mut epic = None;
            let who = match self.approval(n, label)? {
                Ok((who, _)) => who,
                Err(Some(refused)) if !refused.is_empty() && self.c.is_millwright(&refused) => {
                    match self.epic_plan(n, label, &refused)? {
                        Some((who, said, e)) => {
                            on_epic = said;
                            epic = Some(e);
                            who
                        }
                        None => continue,
                    }
                }
                Err(Some(refused)) => {
                    self.clear_unauthorised(n, label, or_unknown(&refused));
                    continue;
                }
                Err(None) => continue,
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
            let to = format!("{on_epic}{to}");
            if self.dry {
                self.log(format!(
                    "would start a planning session for #{n} (labelled by {who}{to})"
                ));
                return Err(End(0));
            }
            // The text the label approved goes in the brief; the line is
            // logged once the brief is written, as the bash does.
            let (title, body) = self.brief_text(n);
            let log = format!("start planning #{n} (labelled by {who}{to})");
            self.act(Action::PlanTurn {
                issue: n,
                turn: Turn::First {
                    label: label.to_string(),
                    by: who,
                    refine,
                    title,
                    body,
                    log,
                    brief_extra: refine.then(|| self.refine_path()),
                    epic,
                    auto: None,
                },
            });
            return Err(End(0));
        }
        Ok(())
    }

    /// epic_plan: the millwright's planning label on a sub-issue of an epic
    /// a person made ready, under TEHAS_MILLWRIGHT_EPICS. The session is on
    /// record as that person's, as the start of a cycle on the epic's
    /// approval is, so the millwright answers its questions within its
    /// grant. The person, what the log says of the label and the epic, or
    /// None with the label off and the reason said.
    fn epic_plan(
        &mut self,
        n: u64,
        label: &str,
        fixer: &str,
    ) -> Step<Option<(String, String, u64)>> {
        match self.epic_part(n)? {
            super::EpicPart::Granted(g) => {
                let ev = self.events(n)?;
                let (who, at) = super::standing_actor(&ev, label);
                if !self.unchanged_since(
                    n,
                    &at,
                    &who,
                    &format!("the fixer applied {label}"),
                    "the fixer or a person applies it again",
                )? {
                    return Ok(None);
                }
                let said = format!(
                    " as the fixer, on {}'s `tehas:ready` on epic #{} at {}",
                    g.by, g.epic, g.at
                );
                Ok(Some((g.by, said, g.epic)))
            }
            super::EpicPart::Refused(why) => {
                self.log(format!(
                    "skip #{n}: {label} from the fixer starts nothing: {why}"
                ));
                self.clear_unauthorised(n, label, fixer);
                Ok(None)
            }
            super::EpicPart::None => {
                self.deny("skip", n, label, fixer);
                self.clear_unauthorised(n, label, fixer);
                Ok(None)
            }
        }
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
            // A row the store gave of a shape it never writes is said, every
            // poll: one with no issue number cannot be spent, and the rest of
            // the list still runs.
            let Some(n) = p.issue.filter(|n| (1..=ISSUE_MAX).contains(n)) else {
                self.log(format!(
                    "WARN: a park owed a refine names no issue tehas can read (issue={})",
                    p.issue_said
                ));
                continue;
            };
            let started = p.started_at.filter(|t| *t <= TIME_MAX);
            let (by, reason, qid, sum) = (
                p.started_by.dash(),
                p.park_reason.dash(),
                p.question.dash(),
                p.body_sha256.dash(),
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
                    "skip #{n}: not refined: a choice was made on its park question"
                ));
                continue;
            }
            let Some(started) = started else {
                // Spent like any other refusal, or it is owed every poll.
                self.log(format!(
                    "skip #{n}: not refined: its park record has no time its cycle began (started_at={})",
                    p.started_said
                ));
                if !self.dry {
                    self.act(Action::ParkRetry {
                        issue: n,
                        how: Spend::Skipped,
                    });
                }
                continue;
            };
            let mut why = String::new();
            if self.control_ask(
                n,
                &format!("a cycle that parked as `{reason}` asked to be refined"),
            ) {
                why = "it is the control issue".into();
            } else {
                let view = match self.s.issues.get(&n).and_then(|i| i.view.clone()) {
                    None => {
                        self.need("view");
                        return Ok(());
                    }
                    Some(View::Failed) => {
                        return Err(self.fail("ERROR: gh issue view failed", 1));
                    }
                    Some(View::Read(v)) => v,
                };
                let have = &view.labels;
                for l in [
                    "tehas:ready",
                    "tehas:running",
                    "tehas:blocked",
                    "tehas:plan",
                    "tehas:refine",
                    "tehas:refined",
                    "tehas:replaced",
                ] {
                    if has(have, l) {
                        why = format!("it carries {l}");
                    }
                }
                if !(has(have, "needs-human-attention") && has(have, "tehas:question")) {
                    why = "it is no longer parked".into();
                }
                if qid == "-" {
                    why = "its park question was answered".into();
                }
                let line = view
                    .body
                    .as_deref()
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
                if sum != "-" && sum != text::body_sha256(&json!({"body": view.body})) {
                    why = "its text changed since its cycle began".into();
                }
                // Last: a closed issue is not in the page, so nothing else of
                // it is known.
                if view.state.as_deref() != Some("OPEN") {
                    why = "it is closed".into();
                }
                if why.is_empty()
                    && !self.unchanged_since(
                        n,
                        &iso(started),
                        &by,
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
                    self.act(Action::ParkRetry {
                        issue: n,
                        how: Spend::Skipped,
                    });
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
            // Spent once its brief is written, before the session begins: a
            // poll that fails after it never starts a second session.
            self.act(Action::ParkRetry {
                issue: n,
                how: Spend::Refine,
            });
            let (title, body) = self.brief_text(n);
            self.act(Action::PlanTurn {
                issue: n,
                turn: Turn::First {
                    label: "tehas:refine".into(),
                    by,
                    refine: true,
                    title,
                    body,
                    log: format!("start {line}"),
                    brief_extra: Some(self.refine_path()),
                    epic: None,
                    auto: Some(reason),
                },
            });
            return Err(End(0));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_fixer(max: &str) -> Settings {
        Settings::parse(&json!({
            "TEHAS_ALLOWED_LABELERS": "alice bob",
            "TEHAS_MILLWRIGHT_LOGIN": "tehas-fixer",
            "TEHAS_MILLWRIGHT_MAX_PLAN_ANSWERS": max,
        }))
    }

    /// A session alice started, with the fixer flags of its answers, oldest
    /// first.
    fn turns(flags: &[bool]) -> Value {
        let answers: Vec<Value> = flags
            .iter()
            .enumerate()
            .map(|(i, f)| {
                json!({"turn": i + 2, "by": if *f { "tehas-fixer" } else { "alice" }, "fixer": f})
            })
            .collect();
        json!({"session": "s", "turns": flags.len() + 1, "by": "alice", "answers": answers})
    }

    #[test]
    fn the_fixer_may_answer_in_a_session_a_person_started() {
        let c = with_fixer("3");
        let (p, f) = (false, true);
        let one = json!({"session": "s", "turns": 1, "by": "alice"});
        assert_eq!(fixer_answer_why(&c, &one), "");
        assert_eq!(fixer_answer_why(&c, &turns(&[p, f, p])), "");
    }

    #[test]
    fn a_session_no_allowed_account_started_takes_no_fixer_answer() {
        let c = with_fixer("3");
        for bad in [
            json!({"session": "s", "turns": 1}),
            json!({"session": "s", "turns": 1, "by": "mallory"}),
            json!({"session": "s", "turns": 1, "by": 7}),
        ] {
            assert_eq!(
                fixer_answer_why(&c, &bad),
                "no person's tehas:plan started this session, as far as its record says"
            );
        }
    }

    #[test]
    fn the_fixer_never_answers_two_turns_in_a_row() {
        assert_eq!(
            fixer_answer_why(&with_fixer("3"), &turns(&[false, true])),
            "the fixer answered the question before this one; a person answers this one"
        );
    }

    #[test]
    fn at_the_limit_the_fixer_answers_no_more_in_the_session() {
        let (p, f) = (false, true);
        let view = turns(&[f, p, f, p, f, p]);
        assert_eq!(
            fixer_answer_why(&with_fixer("3"), &view),
            "the fixer has answered 3 questions in this session, the limit"
        );
        assert_eq!(fixer_answer_why(&with_fixer("4"), &view), "");
    }

    #[test]
    fn answers_of_a_shape_tehas_does_not_know_take_no_fixer_answer() {
        let c = with_fixer("3");
        for bad in [
            json!("lots"),
            json!([{"turn": 2, "by": "alice"}]),
            json!([1]),
            json!({}),
        ] {
            let view = json!({"session": "s", "turns": 1, "by": "alice", "answers": bad});
            assert_eq!(
                fixer_answer_why(&c, &view),
                "its record does not say who answered its turns"
            );
        }
    }

    #[test]
    fn no_record_or_no_fixer_takes_no_fixer_answer() {
        assert_eq!(
            fixer_answer_why(&with_fixer("3"), &Value::Null),
            "the session has no record on this host"
        );
        let none = Settings::parse(&json!({"TEHAS_ALLOWED_LABELERS": "alice bob"}));
        assert_eq!(
            fixer_answer_why(&none, &json!({"session": "s", "turns": 1, "by": "alice"})),
            "there is no fixer"
        );
    }

    #[test]
    fn a_mark_that_is_no_boolean_is_not_read_as_the_fixers() {
        assert_eq!(
            shape(&json!({"answers": [{"turn": 1, "by": "alice", "millwright": "yes"}]})),
            json!({"answers": [{"turn": 1, "by": "alice", "millwright": "yes"}]})
        );
        assert_eq!(
            shape(&json!({"answers": [{"turn": 2, "by": "tehas-fixer", "millwright": true}]})),
            json!({"answers": [{"turn": 2, "by": "tehas-fixer", "fixer": true}]})
        );
    }
}
