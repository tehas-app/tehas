//! Rules 4, 4a and 4b: a cycle started by a retry ticked in a park's
//! question, by the one retry a provider outage is owed, and by a refresh
//! request. Each takes the room rules 2 and 3 left, as the bash dispatcher did.

use super::clean::iso;
use super::{
    has, or_unknown, standing_actor, Action, Choice, End, LabelChange, Poll, RefreshOf, RetryTick,
    Room, Spend, Start, StartExtra, Step, CLASSIC, LEAN,
};
use crate::answer::{self, Overlay};
use crate::questions::{self, Answer, Asked};
#[cfg(test)]
use crate::snapshot::refresh_count;
use crate::snapshot::{CycleRow, Given, PullNode, PullRead, View};
use crate::text;
#[cfg(test)]
use serde_json::json;
#[cfg(test)]
use serde_json::Value;
use tehas_core::driver::Driver;

const ISSUE_MAX: u64 = 9_999_999_999;
const TIME_MAX: u64 = 99_999_999_999;

/// Why the labels `have` of an issue keep its refresh from starting, None
/// when they do not.
fn refresh_labels_why(have: &[String]) -> Option<String> {
    if !has(have, "tehas:awaiting-merge") && !has(have, "needs-human-attention") {
        return Some("it carries neither tehas:awaiting-merge nor needs-human-attention".into());
    }
    if has(have, "needs-human-attention") && has(have, "tehas:ready") {
        return Some("it was handed off and carries tehas:ready".into());
    }
    [
        "tehas:running",
        "tehas:millwright-escalated",
        "tehas:plan",
        "tehas:planning",
    ]
    .into_iter()
    .find(|l| has(have, l))
    .map(|l| format!("it carries {l}"))
}

/// tehas_refresh_rule's refresh_why: what keeps a pull request from being
/// refreshed, empty when nothing does. The bot's are those of `bot`.
pub(crate) fn refresh_why(pr: &PullRead, bot: &str, repo: &str, control: u64) -> String {
    let Some(num) = pr.number else {
        return "it cannot be read".into();
    };
    if !tehas_core::accounts::factorys(&pr.author, bot) {
        "it is not the bot's".into()
    } else if pr.cross_repository != Some(false) || !pr.head_repository.eq_ignore_ascii_case(repo) {
        format!("its branch is not in {repo}")
    } else if pr.draft.is_none() {
        "its draft state cannot be read".into()
    } else if pr.closing_count != 1.0 || pr.closing.len() != 1 {
        "it does not close exactly one issue".into()
    } else {
        let i = &pr.closing[0];
        let Some(n) = i.number else {
            return "the issue it closes cannot be read".into();
        };
        let l = &i.labels;
        if n == control {
            "it closes the control issue".into()
        } else if i.state != "OPEN" {
            format!("#{n} is not open")
        } else if !has(l, "tehas:awaiting-merge") && !has(l, "needs-human-attention") {
            format!("#{n} carries neither tehas:awaiting-merge nor needs-human-attention")
        } else if has(l, "tehas:running") {
            format!("#{n} carries tehas:running")
        } else if i.open_prs != [Some(num)] {
            format!("it is not the one open pull request that closes #{n}")
        } else {
            String::new()
        }
    }
}

/// The largest start this rule can put a date on, as the other rules that
/// read a start from the store take it: 9999-12-31T23:59:59Z.
const MAX_START: u64 = 99_999_999_999;

/// The shape `iso` writes: 20 bytes of `%FT%TZ`, so that the park rule below
/// may order a formatted start against the approval's stamp as text. An empty
/// stamp is no instant, and an approval that names none is not held on it.
fn is_instant(s: &str) -> bool {
    tehas_core::time::iso_shaped(s)
}

/// A cycle_history row: the parked start of a refresh since `at`, the approval
/// its pull request rests on. Tells apart whether a park since `at` holds the
/// refresh, and whether any park it held rests on a start the store did not
/// give. `at` is the approval's `%FT%TZ` stamp, as
/// `read::store::approval_shape` renders it: a start is ordered against it as
/// text, so a stamp of any other shape holds rather than orders wrongly. A
/// start the row does not give is one this rule cannot date, and a park of a
/// row it cannot date is held: unknown is not old, and the second answer lets
/// the poll say so instead of reading as a park it dated. A `parked` that is
/// not JSON true is no park; the store answers that column as a bool or NULL,
/// and the importer takes only a boolean for it.
fn parked_rows(history: &[CycleRow], at: &str) -> (bool, bool) {
    // No instant on the record is no date to measure a park from; the rule of
    // the approval itself has already said what to do with such a record.
    if at.is_empty() {
        return (false, false);
    }
    // A stamp that is not the shape of an instant orders nothing. A park is
    // held on it: holding is the side this rule is for, and the row is not the
    // unread part of it, so it is not named as one.
    if !is_instant(at) {
        return (true, false);
    }
    let mut held = false;
    let mut unread = false;
    for h in history {
        if !h.refresh || !h.parked {
            continue;
        }
        match h.started.filter(|s| *s <= MAX_START) {
            // A park from the approval's second on holds the refresh.
            Some(started) => held |= iso(started).as_str() >= at,
            // No start on record, one that is not a count of seconds this rule
            // can date, or one past the last date it can write: held, and
            // named as one the store did not give.
            None => {
                held = true;
                unread = true;
            }
        }
    }
    (held, unread)
}

/// [`parked_rows`] of cycle_history rows as the store gives them.
#[cfg(test)]
fn parked(history: &[Value], at: &str) -> (bool, bool) {
    let s = crate::snapshot::Snapshot::parse(&json!({"issues": {"1": {"history": history}}}));
    match &s.issues[&1].history {
        Some(Ok(rows)) => parked_rows(rows, at),
        _ => unreachable!("an array of rows is a history"),
    }
}

impl Poll<'_> {
    /// control_ask: the control issue is never worked on; said once a poll.
    pub(super) fn control_ask(&mut self, n: u64, what: &str) -> bool {
        if n != self.c.control {
            return false;
        }
        if !self.control_skipped {
            self.log(format!(
                "skip #{n}: the control issue is never worked on ({what})"
            ));
            self.control_skipped = true;
        }
        true
    }

    /// A choice the poll does not act on: an answer of `tehas answer` is
    /// taken away with the note, and a reply's choice is answered by the
    /// note, which the next poll sees after it and reads the reply no more.
    pub(super) fn decline(&mut self, n: u64, ans: &Asked, note: Option<&str>) {
        self.decline_failing(n, ans, note, None);
    }

    /// A decline whose failure to take an answer away logs `fail`, or the
    /// default line when None.
    fn decline_failing(&mut self, n: u64, ans: &Asked, note: Option<&str>, fail: Option<String>) {
        if ans.tick.answered {
            let question = match &ans.comment {
                Some(Given::Number(c)) => c.as_u64().unwrap_or(0),
                _ => 0,
            };
            self.drop_answer_failing(n, question, note, fail);
            return;
        }
        if let (Some(note), false) = (note, self.dry) {
            let mark = crate::questions::declined_marker(&Given::said(ans.comment.as_ref()));
            self.act(Action::Say {
                issue: n,
                say: format!("{note}\n\n{mark}"),
            });
        }
    }

    /// The question of before 2.0 in `ans` asked again in 2.0's words.
    fn reask(&mut self, n: u64, ans: &Asked) {
        use tehas_core::question::{reasked, Kind};
        let kind = if ans.kind == "park" {
            Kind::Park
        } else {
            Kind::Plan
        };
        let turn = ans.turn.parse().unwrap_or(1);
        let Some(r) = reasked(kind, n, turn, &ans.body) else {
            self.log(format!(
                "skip #{n}: its question of before 2.0 holds no choice to ask again"
            ));
            return;
        };
        if self.dry {
            self.log(format!(
                "would ask #{n}'s question again in the words of 2.0"
            ));
            return;
        }
        let offers: Vec<&str> = r.offers.iter().map(|c| c.word()).collect();
        self.act(Action::Reask {
            issue: n,
            kind: ans.kind.clone(),
            old: Given::said(ans.comment.as_ref()),
            old_body: ans.body.clone(),
            body: r.body,
            offers: offers.join(","),
        });
    }

    /// The answer `tehas answer` gave to question `question` taken away, as a
    /// box is unticked, with its note. A dry run takes nothing away.
    pub(super) fn drop_answer(&mut self, n: u64, question: u64, note: Option<&str>) {
        self.drop_answer_failing(n, question, note, None);
    }

    /// A drop of an answer whose failure logs `fail`, or the default line
    /// when None.
    fn drop_answer_failing(
        &mut self,
        n: u64,
        question: u64,
        note: Option<&str>,
        fail: Option<String>,
    ) {
        if self.dry {
            return;
        }
        self.act(Action::AnswerDrop {
            issue: n,
            id: question,
            fail: fail.unwrap_or_else(|| {
                format!("WARN: could not take away the answer to the question on #{n}")
            }),
            say: note.map(str::to_string),
        });
    }

    /// The hold of a ticked retry: the pull request of `holds_on_pr` read the
    /// same way, and the box taken off with the note that names it. The issue
    /// keeps `tehas:question` and stays parked, so the park's comment is still
    /// there to tick once the pull request is gone.
    pub(super) fn holds_on_pr_tick(&mut self, n: u64, ans: &Asked, url: &str) {
        self.log(format!(
            "skip #{n}: pull request {url} is open and closes it"
        ));
        if self.dry {
            return;
        }
        let note = format!(
            "Retry was asked for and nothing was started: {url} is open and closes this issue. Merge or close that pull request, then answer retry again."
        );
        self.decline(n, ans, Some(&note));
    }

    /// Rule 4: the questions that wait for an answer. A box ticked to retry
    /// a parked cycle starts it, and the poll ends there. A list that cannot
    /// be read skips only this rule, as refreshing() does for the history of
    /// refreshes: only the questions need it, and the poll goes on. plan_over()
    /// is not reached, so an open session is not closed on a read the poll
    /// could not make.
    pub(super) fn questions(&mut self, room: &Room, bot: &str) -> Step {
        let fresh = self.plan_turns();
        let Some(asking) = self.s.asking.clone() else {
            self.need("asking");
            return Ok(());
        };
        let Some(list) = asking else {
            self.log("WARN: the issues with a question could not be read, so no question is answered this poll");
            return Ok(());
        };
        let mut numbers: Vec<(u64, Vec<String>)> = Vec::new();
        for i in list {
            let Some(n) = i.number else {
                return Err(self.fail("ERROR: unreadable list of issues with a question", 1));
            };
            numbers.push((n, i.labels));
        }
        numbers.sort_by_key(|(n, _)| *n);
        let asked: Vec<u64> = numbers.iter().map(|(n, _)| *n).collect();
        self.plan_over(&fresh, &asked);
        for (n, have) in numbers {
            if self.control_ask(n, "its question was answered") {
                continue;
            }
            let Some(q) = self.s.issues.get(&n).and_then(|i| i.question.clone()) else {
                self.need("question");
                continue;
            };
            let q = match q {
                Ok(q) => q,
                Err(e) if e.rc == 1 => {
                    self.log(format!(
                        "skip #{n}: its question on record could not be read ({})",
                        e.word
                    ));
                    continue;
                }
                Err(e) => {
                    return Err(self.fail(
                        format!("ERROR: the state store cannot be used: {}", e.line),
                        78,
                    ))
                }
            };
            let mut qkind = q.kind.clone();
            let mut qid = q.id.clone();
            if qkind != "park" && qkind != "plan" {
                qid.clear();
                qkind.clear();
            }
            let Some(comments) = self.s.issues.get(&n).and_then(|i| i.comments.clone()) else {
                return Err(self.comments_unread(n));
            };
            let Some(ans) = comments.or_none().and_then(|c| {
                questions::answer_of(
                    c,
                    bot,
                    self.s.bot_id,
                    &self.c.allowed,
                    &qid,
                    &self.c.millwright,
                )
            }) else {
                return Err(self.fail(format!("ERROR: unreadable comments of #{n}"), 1));
            };
            let mut ans = match ans {
                Answer::Asked(a) => a,
                Answer::NotAsked { written_by } => {
                    if let Some(by) = written_by {
                        self.log(format!(
                            "skip #{n}: its question was written by '{}', not the bot '{bot}'",
                            or_unknown(&by)
                        ));
                    } else if !qid.is_empty() {
                        // The line names the hold and what clears it: the label
                        // is the hold, not the read's bound, and a new comment
                        // lifts nothing while the question on record stays open.
                        self.log(format!(
                            "skip #{n}: its question is not among the last 100 comments; tehas:question is set and no comment carries the question, so the issue waits until its open points are answered or the label is taken off"
                        ));
                    }
                    continue;
                }
            };
            // A question asked before 2.0 and open at the upgrade is asked
            // again in 2.0's words, once: nothing of the old comment is read
            // as an answer this poll.
            if q.reask && !qid.is_empty() && ans.kind == qkind && ans.issue_number() == Some(n) {
                self.reask(n, &ans);
                continue;
            }
            // An answer `tehas answer` gave reads as the box it stands for.
            // One the question does not offer is taken away, and said.
            // One a person's later tick or reply stands over is taken away
            // without a word. Either way the tick of the comment is read as
            // it is.
            let given = self.s.issues.get(&n).and_then(|i| i.answer.clone());
            let allowed = |by: &str| self.c.allowed(by);
            match (answer::overlay(&mut ans, given.as_ref(), allowed), &given) {
                (Overlay::Refused(why), Some(a)) => {
                    self.log(format!("skip #{n}: {why}"));
                    let note = format!("The answer given for this question was not taken: {why}.");
                    self.drop_answer(n, a.question, Some(&note));
                }
                (Overlay::Later(who), Some(a)) => {
                    self.log(format!(
                        "skip #{n}: the answer of {} is no answer: {who} answered the question after it",
                        a.by
                    ));
                    self.drop_answer(n, a.question, None);
                }
                _ => {}
            }
            let kind = ans.kind.clone();
            if !qid.is_empty() && (ans.issue_number() != Some(n) || kind != qkind) {
                self.log(format!(
                    "skip #{n}: the question on record does not belong to this issue"
                ));
                continue;
            }
            let tick = ans.tick.status.word().to_string();
            let by = ans.tick.by.clone();
            if kind == "plan" {
                // A turn this poll collects or posts asks a new question: the
                // one read before it answers nothing. A turn still in flight
                // posts none, and its issue is read as the bash reads it.
                let in_flight = self.s.sessions.iter().any(|s| s.issue == n && s.in_flight);
                if fresh.contains(&n) && !in_flight {
                    continue;
                }
                self.plan_answer(n, &ans, &tick, &q.plan, room)?;
                continue;
            }
            if kind == "park" && tick == "unoffered" {
                self.log(format!(
                    "skip #{n}: {by} answered a choice its question does not offer"
                ));
                if !self.dry {
                    self.decline(n, &ans, Some("The reply names a choice this question does not offer, so nothing was started. Reply with the word of one of the choices it lists."));
                }
                continue;
            }
            if kind != "park" || tick != "one" {
                continue;
            }
            self.park_ticked.push(n);
            if qkind != "park" {
                self.log(format!(
                    "skip #{n}: a choice was made in a question this host has no record of"
                ));
                if !self.dry {
                    self.decline(n, &ans, Some("This question is not one the factory is waiting on. To retry, apply `tehas:ready`."));
                }
                continue;
            }
            // What was ticked is read from the label of the ticked box, which
            // questions::answer read from the comment's own line: the order of
            // the boxes decides nothing, and the two polls read the same list.
            // The text-only box is a retry as the comments one is; what
            // differs is that its record names no comment of the issue or of
            // the pull request. A comment posted before #1492 carries the old
            // labels, read as the retries they stand for.
            let retry = ans.ticked_label().and_then(tehas_core::question::retry_of);
            let Some(with_comments) = retry else {
                if !self.dry {
                    let mut l = LabelChange::new(
                        n,
                        &["tehas:question"],
                        Self::label_fail(n, &["tehas:question"], &[]),
                    );
                    l.answered = Some((qid.clone(), by.clone()));
                    self.act(Action::Label(l));
                }
                continue;
            };
            let issue_alone = !with_comments;
            let parked = !has(&have, "tehas:running")
                && !has(&have, "tehas:awaiting-merge")
                && has(&have, "needs-human-attention");
            if !parked {
                self.log(format!(
                    "skip #{n}: a retry was answered but the issue is not parked"
                ));
                if !self.dry {
                    self.decline(
                        n,
                        &ans,
                        Some("Retry was asked for, but this issue is not parked, so nothing was started."),
                    );
                }
                continue;
            }
            // The park of a lean start tehas work refused: the same start
            // would be refused again. The tick stays, and starts classic once
            // the label is off, or under a lean default once `tehas:classic`
            // is on. That label is then a person's new choice, read as one
            // and not against the record of the refused lean start.
            let lean_refused = q.reason.as_deref() == Some(tehas_core::question::LEAN_REFUSED);
            if lean_refused && (self.c.driver_default == Driver::Lean) && !has(&have, CLASSIC) {
                self.log(format!("wait #{n}: a retry was answered on the park of a lean start this tehas cannot run; apply {CLASSIC} first, and the answer then starts it on classic"));
                continue;
            }
            if lean_refused && !(self.c.driver_default == Driver::Lean) && has(&have, LEAN) {
                self.log(format!("wait #{n}: a retry was answered on the park of a lean start this tehas cannot run; take {LEAN} off first, and the answer then starts it on classic"));
                continue;
            }
            if !self.work_ok(n)? || !self.epic_ok(n, "tehas:ready", &have) {
                continue;
            }
            let at = ans.tick.at.clone();
            if !self.unchanged_since(
                n,
                &at,
                &by,
                &format!("{by} answered retry"),
                "answer again",
            )? {
                continue;
            }
            // The same hold the `tehas:ready` path applies before it starts
            // anything: while an open pull request closes the issue a cycle
            // would only stop at `existing-pr-detected`, so the tick is held
            // here. What comes off is the box, not a label.
            let url = match self.open_pr(n)? {
                Some(url) => Some(url),
                None => self.named_pr(n),
            };
            if let Some(url) = url {
                self.holds_on_pr_tick(n, &ans, &url);
                continue;
            }
            if room.want == 0 {
                if self.dry {
                    self.log(room.hold.clone());
                }
                continue;
            }
            self.mesh_ok()?;
            if !self.image_ok(n)? || !self.image_missing_ok(n) {
                continue;
            }
            // A person's tehas:ready that park_holds held comes off with this
            // start: it is the new approval the driver reads the label for.
            let held = has(&have, "tehas:ready") && {
                let ev = self.events(n)?;
                self.c.allowed(&standing_actor(&ev, "tehas:ready").0)
            };
            let follows = !held && !(lean_refused && (self.c.driver_default == Driver::Lean));
            let driver = match self.driver(n, &have, follows)? {
                Choice::Wait => continue,
                Choice::Stranger(s) => {
                    self.lean_deny(n, &s);
                    if !self.dry {
                        let note = format!(
                            "Retry was asked for and nothing was started: {}, then answers retry again.",
                            s.said()
                        );
                        self.decline(n, &ans, Some(&note));
                    }
                    continue;
                }
                d => d,
            };
            if self.dry {
                self.log(format!(
                    "would run a cycle for #{n} (retry answered by {by})"
                ));
                return Err(End(0));
            }
            let mut remove = vec!["tehas:question".to_string()];
            for l in ["tehas:ready", "tehas:millwright-escalated"] {
                if has(&have, l) {
                    remove.push(l.into());
                }
            }
            let mut extra = StartExtra {
                retry: Some(RetryTick {
                    comment: ans.comment.clone(),
                    since: ans.asked_at.clone(),
                    issue_alone,
                }),
                ..StartExtra::default()
            };
            driver.bind(&mut extra, &mut remove, &have);
            self.d.picked.push(n);
            self.act_gates(n);
            self.act_start(
                n,
                Start {
                    starter: by,
                    verb: "retry answered".into(),
                    how: if ans.tick.answered { "answer" } else { "reply" }.into(),
                    given_at: at,
                    given_how: if ans.tick.answered { "answer" } else { "reply" }.into(),
                    remove,
                    fixer: None,
                    park_lifted: None,
                    extra,
                    placed: None,
                },
            );
            // The answer of `tehas answer` is spent; a reply needs nothing: its
            // question is no longer on record.
            self.decline_failing(n, &ans, None, Some(format!("WARN: could not take away the answer to the retry on #{n}; its question is no longer on record, so it starts nothing more")));
            return Err(End(0));
        }
        Ok(())
    }

    /// mesh_ok: a model endpoint that does not answer ends the poll; a dry
    /// run says why.
    pub(super) fn mesh_ok(&mut self) -> Step {
        if self.s.endpoint_hold.is_empty() {
            return Ok(());
        }
        if self.dry {
            let h = self.s.endpoint_hold.clone();
            self.log(h);
        }
        Err(End(0))
    }

    /// Rule 4a: a cycle a provider outage parked starts again once, on the
    /// word of whoever started it.
    pub(super) fn outage(&mut self, room: &Room) -> Step {
        if room.want <= 0 {
            return Ok(());
        }
        let owed = match self.s.owed.clone() {
            None => {
                self.need("owed");
                return Ok(());
            }
            Some(Err(e)) => {
                let rc = if e.rc == 1 { 1 } else { 78 };
                return Err(self.fail(
                    format!(
                        "ERROR: the state store could not list the parks owed a retry ({})",
                        e.word
                    ),
                    rc,
                ));
            }
            Some(Ok(o)) => o,
        };
        for p in owed {
            // A row of a shape the store never writes is said: one with no
            // issue cannot be spent, one with no start is spent once.
            let Some(n) = p.issue.filter(|n| (1..=ISSUE_MAX).contains(n)) else {
                self.log(format!(
                    "WARN: a park owed a retry after a provider outage names no issue tehas can read (issue={})",
                    p.issue_said
                ));
                continue;
            };
            let Some(started) = p.started_at.filter(|t| *t <= TIME_MAX) else {
                self.log(format!(
                    "skip #{n}: its retry after a provider outage is not started: its park record has no time its cycle began (started_at={})",
                    p.started_said
                ));
                if !self.dry {
                    self.act(Action::ParkRetry {
                        issue: n,
                        how: Spend::Outage,
                    });
                }
                continue;
            };
            let by = p.started_by.text_or_empty().to_string();
            if self.running(n) {
                continue;
            }
            let view = match self.s.issues.get(&n).and_then(|i| i.view.clone()) {
                None => {
                    self.need("view");
                    continue;
                }
                Some(View::Failed) => return Err(self.fail("ERROR: gh issue view failed", 1)),
                Some(View::Read(v)) => v,
            };
            let have = view.labels;
            if has(&have, "tehas:ready") {
                continue;
            }
            if self.control_ask(n, "a cycle its provider outage parked asked to start again") {
                continue;
            }
            let driver = self.driver(n, &have, true)?;
            if matches!(driver, Choice::Wait) {
                continue;
            }
            let held = matches!(driver, Choice::Stranger(_))
                || view.state.as_deref() != Some("OPEN")
                || !has(&have, "needs-human-attention")
                || has(&have, "tehas:running")
                || self.plan_open(n)
                || !self.c.allowed(&by)
                || !self.unchanged_since(
                    n,
                    &iso(started),
                    &by,
                    "its last cycle began",
                    "apply tehas:ready",
                )?;
            if held {
                if let Choice::Stranger(s) = &driver {
                    self.lean_deny(n, s);
                }
                self.log(format!("skip #{n}: its retry after a provider outage is not started; it waits for a person"));
                if !self.dry {
                    self.act(Action::ParkRetry {
                        issue: n,
                        how: Spend::Outage,
                    });
                    if let Choice::Stranger(s) = &driver {
                        let say = format!("Tehas did not start this issue again after the provider outage: {}, then applies `tehas:ready`.", s.said());
                        self.act(Action::Say { issue: n, say });
                    }
                }
                continue;
            }
            if self.dry {
                self.log(format!(
                    "would run a cycle for #{n} (retry after a provider outage, started by {by})"
                ));
                return Err(End(0));
            }
            let mut remove = if has(&have, "tehas:question") {
                vec!["tehas:question".to_string()]
            } else {
                Vec::new()
            };
            self.act_gates(n);
            self.act(Action::Say { issue: n, say: "Tehas starts this issue again on its own: its last cycle was parked by a provider outage, and the model endpoint answers again. It does this once. If the new cycle is parked the same way, it waits for a person.".into() });
            self.d.picked.push(n);
            let mut extra = StartExtra::default();
            driver.bind(&mut extra, &mut remove, &have);
            self.act_start(
                n,
                Start {
                    starter: by,
                    verb: "requeued".into(),
                    how: "start of the cycle a provider outage parked".into(),
                    given_at: iso(started),
                    given_how: "outage-retry".into(),
                    remove,
                    fixer: None,
                    park_lifted: None,
                    extra,
                    placed: None,
                },
            );
            return Err(End(0));
        }
        Ok(())
    }

    /// refresh_held: why the sandbox image lets no refresh start on this
    /// host, empty when it lets one.
    pub(super) fn refresh_held(&mut self) -> Option<String> {
        match self.s.continues {
            None => {
                self.need("continues");
                None
            }
            Some(true) => Some(String::new()),
            Some(false) => Some(format!(
                "the pi-rukas in the sandbox image {} does not take --continue-pr",
                self.c.image
            )),
        }
    }

    /// What of pull request `node` keeps the refresh of `n` that `req` asks
    /// for from starting: "wait" while GitHub has not said whether it
    /// conflicts, "busy" while it carries tehas:refreshing, empty when
    /// nothing does, and otherwise why the request goes.
    fn refresh_pull_why(&self, n: u64, node: &PullRead, head_sha: &str, bot: &str) -> String {
        let number = &node.number_said;
        let rule = refresh_why(node, bot, &self.c.repo, self.c.control);
        let merge = node.merge.as_deref().unwrap_or("");
        if node.state != "OPEN" {
            format!("pull request #{number} is not open")
        } else if !rule.is_empty() {
            format!("pull request #{number}: {rule}")
        } else if node.closing.first().and_then(|i| i.number) != Some(n) {
            format!("pull request #{number} does not close #{n}")
        } else if node.head_oid.as_deref() != Some(head_sha) {
            format!("pull request #{number} moved since the request")
        } else if merge == "UNKNOWN" {
            "wait".into()
        } else if merge != "CONFLICTING" {
            format!("pull request #{number} no longer conflicts")
        } else if has(&node.labels, "tehas:refreshing") {
            "busy".into()
        } else {
            String::new()
        }
    }

    fn refresh_drop(&mut self, n: u64, why: &str) -> Step<Option<Start>> {
        self.log(format!("drop the refresh of #{n}: {why}"));
        if !self.dry {
            self.act(Action::RefreshDrop(n));
        }
        Ok(None)
    }

    /// refresh_say: a hold a person can act on, said once for each key.
    fn refresh_say(&mut self, n: u64, key: &str, why: &str) -> Step<Option<Start>> {
        self.refresh_drop(n, why)?;
        let Some(said) = self.s.issues.get(&n).and_then(|i| i.refresh_said.clone()) else {
            self.need("refresh_said");
            return Ok(None);
        };
        if !self.dry && said != key {
            self.act(Action::SayOnce {
                issue: n,
                key: key.to_string(),
                say: format!("Tehas does not refresh this issue's pull request on its own: {why}. It conflicts with the branch: a person resolves the conflict, or closes the pull request and then applies `tehas:ready` to start a fresh cycle."),
            });
        }
        Ok(None)
    }

    /// Rule 4b, refresh_ok: the start of a refresh request, or None when it
    /// waits or goes.
    pub(super) fn refresh_ok(&mut self, n: u64, bot: &str) -> Step<Option<Start>> {
        let Some(held) = self.refresh_held() else {
            return Ok(None);
        };
        if !held.is_empty() {
            if !self.refresh_said {
                self.log(format!("hold refreshes: {held}"));
            }
            self.refresh_said = true;
            return Ok(None);
        }
        let Some(req) = self.s.issues.get(&n).and_then(|i| i.request.clone()) else {
            self.need("request");
            return Ok(None);
        };
        let Some(req) = req.target else {
            return self.refresh_drop(n, "its request cannot be read");
        };
        let pr = req.pr;
        if self.running(n) {
            return self.refresh_drop(n, "a cycle runs on it");
        }
        if self.plan_open(n) {
            return self.refresh_drop(n, "a planning session is open on it");
        }
        if !self.work_ok(n)? {
            return self.refresh_drop(n, "it is not an open issue");
        }
        let have = self
            .work(n)
            .map(|w| w.labels.names().to_vec())
            .unwrap_or_default();
        if let Some(why) = refresh_labels_why(&have) {
            return self.refresh_drop(n, &why);
        }
        let none = "this host has no approval of a person's that its pull request rests on";
        let record = match self.s.issues.get(&n).and_then(|i| i.approval.clone()) {
            None => {
                self.need("approval");
                return Ok(None);
            }
            Some(Err(())) => {
                self.log(format!("wait #{n}: its approval could not be read"));
                return Ok(None);
            }
            Some(Ok(None)) => return self.refresh_drop(n, none),
            Some(Ok(Some(r))) => r,
        };
        let Some(rests) = record.base().filter(|b| b.stands(self.c)) else {
            return self.refresh_drop(n, none);
        };
        let count = record.refresh_count(&rests);
        let by = rests.by.clone();
        let at = rests.at.clone();
        let approver = rests.approver().to_string();
        if !self.unchanged_since(
            n,
            &at,
            &approver,
            &format!("{by} approved it"),
            "a person applies tehas:ready",
        )? {
            return self.refresh_drop(
                n,
                &format!("its title or text was changed since {by} approved it"),
            );
        }
        if text::body_sum(self.text(n).body_text()) != rests.body_sha256 {
            return self.refresh_drop(n, &format!("its text is not the text {by} approved"));
        }
        let node = match self.s.pulls.get(&pr).cloned() {
            None => {
                self.need("pull");
                return Ok(None);
            }
            Some(PullNode::Failed { line }) => {
                return Err(self.unreadable(&format!("pull request #{pr}"), line.as_deref()));
            }
            Some(PullNode::Unreadable) => {
                return self.refresh_drop(n, &format!("pull request #{pr} cannot be read"));
            }
            Some(PullNode::Read(node)) => node,
        };
        let why = self.refresh_pull_why(n, &node, &req.head_sha, bot);
        match why.as_str() {
            "" => {}
            "wait" => {
                self.log(format!(
                    "wait #{n}: GitHub has not said yet whether pull request #{pr} conflicts"
                ));
                return Ok(None);
            }
            "busy" => {
                self.log(format!(
                    "wait #{n}: pull request #{pr} carries tehas:refreshing"
                ));
                return Ok(None);
            }
            w => return self.refresh_drop(n, w),
        }
        let Some(count) = count else {
            return self.refresh_drop(
                n,
                "its approval record does not say how many refreshes there were",
            );
        };
        if count as i64 >= self.c.refresh_max {
            return self.refresh_say(
                n,
                &format!("cap {at} {count}"),
                &format!("it was refreshed {count} times since {by} approved it, the limit"),
            );
        }
        let history = match self.s.issues.get(&n).and_then(|i| i.history.clone()) {
            None => {
                self.need("history");
                return Ok(None);
            }
            Some(Err(())) => {
                self.log(format!("wait #{n}: its history could not be read"));
                return Ok(None);
            }
            Some(Ok(h)) => h,
        };
        // The last of the holds: a refresh that parked since the approval is
        // not offered again. One whose history cannot be read waits, above.
        let (parked, unread) = parked_rows(&history, &at);
        if parked {
            let why = if unread {
                format!("a refresh of it since {by} approved it parked, its start unread, so it is held")
            } else {
                format!("a refresh of it since {by} approved it parked")
            };
            return self.refresh_say(n, &format!("park {at}"), &why);
        }
        if !self.image_ok(n)? || !self.image_missing_ok(n) {
            return Ok(None);
        }
        let driver = match self.driver(n, &have, true)? {
            Choice::Wait => return Ok(None),
            Choice::Stranger(s) => {
                return self.refresh_say(n, &format!("lean {}", s.who), &s.said());
            }
            d => d,
        };
        let branch: String = node
            .base_ref
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '-'))
            .collect();
        let url = Some(node.url.as_str())
            .filter(|u| {
                u.starts_with("https://")
                    && u[8..].bytes().all(|b| {
                        b.is_ascii_alphanumeric() || matches!(b, b'.' | b'/' | b'_' | b'-')
                    })
                    && u.len() > 8
            })
            .unwrap_or("");
        let mut remove = Vec::new();
        for l in ["tehas:awaiting-merge", "tehas:question", "tehas:ready"] {
            if has(&have, l) {
                remove.push(l.to_string());
            }
        }
        let mut extra = StartExtra {
            refresh: Some(RefreshOf {
                pr,
                url: url.to_string(),
                head_sha: req.head_sha.clone(),
                base_sha: req.base_sha.clone(),
                branch: if branch.is_empty() {
                    self.c.branch.clone()
                } else {
                    branch
                },
            }),
            ..StartExtra::default()
        };
        driver.bind(&mut extra, &mut remove, &have);
        Ok(Some(Start {
            starter: by,
            verb: "refresh".into(),
            how: format!("approval of {at}"),
            given_at: at,
            given_how: "refresh".into(),
            remove,
            fixer: None,
            park_lifted: None,
            extra,
            placed: None,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_count_counts_on_the_same_approval_only() {
        let base = json!({"at": "2026-10-01T00:00:00Z", "by": "alice"});
        let r = json!({"how": "refresh", "refreshes": 2, "rests_on": {"at": "2026-10-01T00:00:00Z", "by": "alice"}});
        assert_eq!(refresh_count(&r, &base), Some(2));
        assert_eq!(refresh_count(&json!({"how": "label"}), &base), Some(0));
        let other = json!({"how": "refresh", "refreshes": 2, "rests_on": {"at": "2026-09-01T00:00:00Z", "by": "alice"}});
        assert_eq!(refresh_count(&other, &base), Some(0));
        let bad = json!({"how": "refresh", "refreshes": 0, "rests_on": base});
        assert_eq!(refresh_count(&bad, &base), None);
    }

    #[test]
    fn a_park_holds_a_refresh_only_from_the_approval_on() {
        let at = "2026-10-01T00:00:00Z";
        let park = |started: Value| json!({"how": "refresh", "parked": true, "started": started});
        // [why, row, holds]: one row, and whether it holds a refresh.
        let cases: &[(&str, Value, bool)] = &[
            (
                "a park from the approval's second on",
                park(json!(1790899201)),
                true,
            ),
            (
                "a park before the approval: of an older call",
                park(json!(1790769600)),
                false,
            ),
            ("a park with no start on record", park(Value::Null), true),
            (
                "a park whose row has no start at all",
                json!({"how": "refresh", "parked": true}),
                true,
            ),
            (
                "a start that is not a count of seconds",
                park(json!("1790899201")),
                true,
            ),
            ("a start that is negative", park(json!(-1)), true),
            (
                "a start past the last date iso writes",
                park(json!(100_000_000_000u64)),
                true,
            ),
            (
                "a park that is not JSON true",
                json!({"how": "refresh", "parked": "true", "started": 1790899201}),
                false,
            ),
            (
                "a park that is not parked",
                json!({"how": "refresh", "parked": false, "started": 1790899201}),
                false,
            ),
            (
                "the park of a cycle that was no refresh",
                json!({"how": "label", "parked": true, "started": 1790899201}),
                false,
            ),
        ];
        for (why, row, holds) in cases {
            // A park held on a start the rule cannot read is told apart from
            // one it dated, so the poll can say which it answered: a row that
            // gives no start, or one the bound has just struck out.
            let readable = row
                .get("started")
                .and_then(Value::as_u64)
                .is_some_and(|s| s <= MAX_START);
            assert_eq!(
                parked(std::slice::from_ref(row), at),
                (*holds, *holds && !readable),
                "{why}"
            );
        }
        assert!(!parked(&[], at).0);
    }

    #[test]
    fn an_approval_that_names_no_instant_orders_no_park() {
        // The rule orders a park's start against the approval's stamp as text,
        // so a stamp of any other shape holds, and none at all is no date to
        // measure a park from.
        let row = |started| json!({"how": "refresh", "parked": true, "started": started});
        for at in ["2026-10-01Z", "2026-10-1", "Z", "2026-10-01T00:00:00"] {
            assert!(parked(&[row(1790769600)], at).0, "{at}");
        }
        assert!(!parked(&[row(1790769600)], "").0);
    }
}
