//! The events tehas raises, made from what the state store answers after a
//! record commits (docs/specs/2026-11-22-events.md, T11.8, T11.9 and T11.10).
//! Each payload is built from the store's answer, never from the raiser's own
//! view, so it carries what `tehas status`, `tehas history` and `tehas
//! question` print. One copy of each mapping, for every program that raises:
//! the cycle, `tehas stop`, the dispatcher and `tehas limit`.
//!
//! Each end is raised by one process, the one that wrote its line of
//! history. A line a report wrote (`reported`) is the cycle's: `tehas-cycle`
//! raises it after its report, also when the work's own push fault wrote it.
//! A line written with no report, of a refused start, `tehas stop`, the poll
//! or the next start, is raised by the process whose call the store answered
//! with the write: its `already` false, or for the next start its
//! `replaced`. The store writes one line per start, in one transaction, so
//! the two kinds of writer never both own one.

use crate::limit::{self, leading_number, quoted};
use crate::outcome::{self, ParkLine};
use crate::question::stored_choices;
use crate::verb::Verb;
use serde::Deserialize;
use serde_json::Value;
use std::num::{NonZeroU32, NonZeroU64};
use tehas_ports::events::{
    Builder, ChangeMerged, ChangeOffered, CycleEnded, CycleId, CycleParked, CycleStarted,
    Dispatcher, End, Envelope, How, LimitChanged, LimitState, Payload, QuestionAsked,
    QuestionChoice, QuestionNo, RunnerWord, TicketActed, TicketClosed,
};
use tehas_ports::{
    Account, AccountKind, Act, At, BlueprintNo, ChangeNo, CloseReason, Login, QuestionKind, Realm,
    TicketKey,
};

/// The log listener's id.
pub const LOG: &str = "log";

/// The log's line of an event: `event` and its envelope as JSON.
pub fn line(e: &Envelope) -> String {
    match serde_json::to_string(e) {
        Ok(json) => format!("event {json}"),
        Err(_) => format!("event {} {}", e.name().as_str(), e.key.as_str()),
    }
}

/// A builder with the log listener, which writes [`line`] with `log`.
pub fn logged(log: impl Fn(&str) + Send + Sync + 'static) -> Builder {
    let mut b = Builder::new();
    b.log(LOG, move |e| log(&line(e)));
    b
}

/// The dispatcher of a command that raises: the log listener alone, writing
/// one line per event to the factory log ([`crate::home::Home::factory_log`])
/// with the time of it, and saying the line as `who` when the log cannot be
/// written. The one place that writes the factory log for a raising command:
/// `tehas stop`, and `tehas limit`, `tehas pause`, `tehas resume` and
/// `tehas drain`.
pub fn factory_logged(home: &std::path::Path, who: &str) -> Dispatcher {
    let log = crate::home::Home::new(home).factory_log();
    let prefix = format!("{who}:");
    logged(move |line| {
        let at = crate::time::iso(crate::time::now());
        // One write of the whole line, so O_APPEND keeps it whole beside the
        // other writers of the log.
        let whole = format!("{at} {line}\n");
        // The log's directory is made if the home has none, so a command is
        // never silenced into its own stderr by a missing logs/.
        if let Some(dir) = log.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let wrote = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log)
            .and_then(|mut f| std::io::Write::write_all(&mut f, whole.as_bytes()));
        if wrote.is_err() {
            eprintln!("{prefix} {line}");
        }
    })
    .build()
}

/// Raises `p` on `d`, and says each listener that failed with `log`, one
/// WARN line each. A listener never fails the raiser.
pub fn raise<P: Payload>(d: &Dispatcher, p: P, log: &dyn Fn(&str)) {
    let (name, key) = (P::NAME.as_str(), p.key());
    for f in d.raise(p).faults {
        log(&format!(
            "WARN: event {name} {}: the listener {} failed: {}",
            key.as_str(),
            f.listener,
            f.why
        ));
    }
}

/// What the store answered a start with (`cycle-start`), or the cycle of
/// `cycle-show`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct StartRow {
    pub issue: u64,
    pub attempt: u32,
    pub started_at: i64,
    #[serde(default)]
    pub started_by: Option<String>,
    #[serde(default)]
    pub how: Option<String>,
    /// The attempt the start ended as lost, an open record no process held.
    #[serde(default)]
    pub replaced: Option<u32>,
}

/// `cycle.started` of the start the store answered, as `running[]` of
/// `tehas status --json` prints it.
pub fn started(s: &StartRow) -> Option<CycleStarted> {
    let blueprint = BlueprintNo::new(s.issue)?;
    let how = s.how.as_deref().unwrap_or("hand");
    let mut e = CycleStarted::new(
        CycleId::new(blueprint, NonZeroU32::new(s.attempt)?)?,
        blueprint,
        At::new(s.started_at),
        serde_json::from_value(Value::String(how.to_string())).unwrap_or(How::Unknown),
    );
    e.by = s
        .started_by
        .clone()
        .filter(|l| !l.is_empty())
        .map(Login::new);
    Some(e)
}

/// What the store answered an end with (`cycle-end`): `already` when the
/// record had ended before this call, so this call wrote no line.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub struct EndAnswer {
    pub already: bool,
}

fn yes() -> bool {
    true
}

/// A cycle's line of `history-show`, a work line of `tehas history`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct HistoryRow {
    pub kind: String,
    pub issue: u64,
    pub attempt: u32,
    pub at: f64,
    pub status: Option<String>,
    pub step: Option<String>,
    pub pr: Option<u64>,
    pub started_at: Option<f64>,
    pub rc: Option<i64>,
    pub cap: Option<String>,
    pub park_reason: Option<String>,
    pub failure_kind: Option<String>,
    pub failure_cause: Option<String>,
    pub provider: bool,
    pub parked: bool,
    pub seq: u64,
    pub recorded_at: Option<f64>,
    pub signalled: bool,
    /// A report wrote the line. False on the line of a refused start,
    /// `tehas stop` and a lost cycle.
    #[serde(default = "yes")]
    pub reported: bool,
}

impl HistoryRow {
    // A word the history prints: one that is there and not empty.
    fn word(w: &Option<String>) -> Option<&str> {
        w.as_deref().filter(|w| !w.is_empty())
    }

    fn park_line(&self) -> ParkLine<'_> {
        ParkLine {
            issue: self.issue,
            work: self.kind == "cycle",
            parked: self.parked,
            provider: self.provider,
            how: [
                Self::word(&self.step),
                Self::word(&self.cap),
                Self::word(&self.park_reason),
                Self::word(&self.failure_kind),
                Self::word(&self.failure_cause),
            ],
        }
    }

    fn end(&self) -> Option<End> {
        let word = outcome::end(&outcome::Line {
            outcome: self.status.as_deref(),
            status: self.rc.filter(|rc| *rc >= 0),
            change: self.pr.is_some(),
            parked: self.parked,
            failure_kind: Self::word(&self.failure_kind),
        });
        serde_json::from_value(Value::String(word.to_string())).ok()
    }
}

/// A question as `question-show` and `question-list` answer it.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct QuestionRow {
    pub id: u64,
    pub issue: u64,
    pub kind: String,
    pub attempt: u32,
    pub posted_at: i64,
    #[serde(default)]
    pub offers: Option<Vec<String>>,
    #[serde(default)]
    pub options: Option<Vec<String>>,
}

/// `question.asked` of a question the store holds, as `tehas question
/// --json` prints it: a park's with its cycle, a plan's with none, and a
/// plan's choices only while its options are on record.
pub fn asked(q: &QuestionRow) -> Option<QuestionAsked> {
    let blueprint = BlueprintNo::new(q.issue)?;
    let (kind, cycle) = match q.kind.as_str() {
        "park" => (
            QuestionKind::Park,
            Some(CycleId::new(blueprint, NonZeroU32::new(q.attempt)?)?),
        ),
        "plan" => (QuestionKind::Plan, None),
        _ => return None,
    };
    let known = kind == QuestionKind::Park || q.options.is_some();
    let choices = if known {
        stored_choices(
            &q.kind,
            q.offers.as_deref(),
            q.options.as_deref().unwrap_or_default(),
        )
        .iter()
        .map(|o| QuestionChoice::new(o.answer_word(), o.label.clone()))
        .collect()
    } else {
        Vec::new()
    };
    let mut e = QuestionAsked::new(
        QuestionNo::new(q.id)?,
        kind,
        blueprint,
        At::new(q.posted_at),
    );
    e.cycle = cycle;
    e.choices = choices;
    Some(e)
}

/// Who wrote the line of an end, and so raises it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Writer {
    /// The cycle's report: `tehas-cycle`, after it.
    Report,
    /// An end with no report, by the process whose call wrote it.
    Short,
}

/// The events of a cycle's end, in the order they are raised.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ended {
    pub parked: Option<CycleParked>,
    pub asked: Option<QuestionAsked>,
    pub ended: Option<CycleEnded>,
    /// The change's merge, raised of the line the report ended `merged` and
    /// did not park: only the cycle writes such a line, so it is raised once.
    pub merged: Option<ChangeMerged>,
    pub offered: Option<ChangeOffered>,
}

impl Ended {
    /// The envelopes, in the order they are raised.
    pub fn envelopes(&self) -> Vec<Envelope> {
        let mut all = Vec::new();
        all.extend(self.parked.clone().map(Envelope::new));
        all.extend(self.asked.clone().map(Envelope::new));
        all.extend(self.ended.clone().map(Envelope::new));
        all.extend(self.merged.clone().map(Envelope::new));
        all.extend(self.offered.clone().map(Envelope::new));
        all
    }

    /// Raises each, in order, as [`raise`] does.
    pub fn raise(self, d: &Dispatcher, log: &dyn Fn(&str)) {
        if let Some(p) = self.parked {
            raise(d, p, log);
        }
        if let Some(p) = self.asked {
            raise(d, p, log);
        }
        if let Some(p) = self.ended {
            raise(d, p, log);
        }
        if let Some(p) = self.merged {
            raise(d, p, log);
        }
        if let Some(p) = self.offered {
            raise(d, p, log);
        }
    }
}

fn word(w: Option<&str>) -> Option<RunnerWord> {
    w.and_then(|w| RunnerWord::new(w).ok())
}

/// The events of the end of cycle `attempt` of `issue` that `by` raises,
/// from the issue's lines of history (`history-show <issue>`) and the
/// question it waits on (`question-show <issue>`). None of a line the store
/// does not have, or one another writer wrote. `site_at` answers the site's
/// time of a merged change, asked of the code-host port by whoever raises:
/// `change.merged` keys on it, falling back to the line's end when the port
/// answers nothing or the read fails. It is asked once, for the merged line
/// only, never per poll; writers with no port pass `&mut |_| None`.
pub fn ended(
    issue: u64,
    attempt: u32,
    lines: &[HistoryRow],
    question: Option<&QuestionRow>,
    by: Writer,
    site_at: &mut dyn FnMut(ChangeNo) -> Option<At>,
) -> Ended {
    let mut lines: Vec<&HistoryRow> = lines.iter().filter(|l| l.issue == issue).collect();
    lines.sort_by_key(|l| l.seq);
    let repeats = outcome::repeats(&lines.iter().map(|l| l.park_line()).collect::<Vec<_>>());
    let at = lines
        .iter()
        .position(|l| l.kind == "cycle" && l.attempt == attempt);
    let (Some(at), Some(blueprint)) = (at, BlueprintNo::new(issue)) else {
        return Ended::default();
    };
    let line = lines[at];
    if line.reported != (by == Writer::Report) {
        return Ended::default();
    }
    let (Some(cycle), Some(end), Some(status)) = (
        NonZeroU32::new(attempt).and_then(|a| CycleId::new(blueprint, a)),
        line.end(),
        line.status.clone(),
    ) else {
        return Ended::default();
    };
    let ended_at = At::new(line.at.floor() as i64);
    let change = line.pr.and_then(ChangeNo::new);
    let parked = end.stop().filter(|_| line.parked).map(|stop| {
        let mut e = CycleParked::new(cycle, blueprint, ended_at, stop);
        e.step = word(HistoryRow::word(&line.step));
        e.cap = word(HistoryRow::word(&line.cap));
        e.park_reason = word(HistoryRow::word(&line.park_reason));
        e.failure_kind = word(HistoryRow::word(&line.failure_kind));
        e.failure_cause = word(HistoryRow::word(&line.failure_cause));
        e.provider = line.provider;
        e.repeat = u32::try_from(repeats[at]).ok().and_then(NonZeroU32::new);
        e.change = change;
        e
    });
    let asked = question
        .filter(|q| q.kind == "park" && q.issue == issue && q.attempt == attempt)
        .and_then(asked);
    let mut ended = CycleEnded::new(
        cycle,
        blueprint,
        At::new(line.started_at.unwrap_or(line.at).floor() as i64),
        ended_at,
        end,
        status,
    );
    ended.recorded = line.recorded_at.map(|r| At::new(r.floor() as i64));
    ended.status = line.rc.and_then(|rc| u8::try_from(rc).ok());
    ended.parked = line.parked;
    ended.signalled = line.signalled;
    ended.change = change;
    // A line that ended `merged` and parked is the runner's claim of a
    // merge the bot's list did not name: only the report's true merge ends
    // unparked, and it is the cycle's own record to raise.
    let merged = change
        .filter(|_| end == End::Merged && !line.parked)
        .map(|c| {
            // The site's time of the merge, asked of the code-host port by
            // the raising process; the line's end when the port answers
            // nothing or the read fails. Asked here, once, for this line.
            let at = site_at(c);
            let mut m = ChangeMerged::new(c, at.unwrap_or(ended_at));
            m.blueprint = Some(blueprint);
            m
        });
    Ended {
        parked,
        asked,
        merged,
        offered: offered(&ended),
        ended: Some(ended),
    }
}

/// `change.offered` of an end: when its line names a change, no signal
/// stopped it, and it did not end `refused` or `merged`. The 1.x hook's
/// rule, and a merged change was skipped by its open check.
pub fn offered(e: &CycleEnded) -> Option<ChangeOffered> {
    let free = !e.signalled && !matches!(e.end, End::Refused | End::Merged);
    Some(ChangeOffered::new(
        e.change.filter(|_| free)?,
        e.blueprint,
        e.cycle,
        e.end,
        e.ended,
    ))
}

/// The account of an act the raiser read from the tickets port's history
/// and checked against the allowed list. The site's id of it stays with the
/// witness: a raise answers the login the history names.
fn person(login: &str) -> Account {
    Account::new(Realm::Tickets, Login::new(login), AccountKind::Person)
}

/// `ticket.acted` of an issue's act recorded now. The time is the act's own,
/// as the tickets port's history stamps it: the key must be the key a
/// consumer makes from the same history.
pub fn acted(issue: u64, act: Act, by: &str, at: i64) -> Option<TicketActed> {
    BlueprintNo::new(issue)?;
    Some(TicketActed::new(
        TicketKey::Number(NonZeroU64::new(issue)?),
        act,
        person(by),
        At::new(at),
    ))
}

/// The record `approval-record` answered, or `approval-current` shows: the
/// act of approval, when the record approves. Its `at` is the label event's
/// time, which `tehas start` wrote with it, so the key is the key the
/// tickets port's history gives the same approval.
pub fn approved(answer: &str) -> Option<TicketActed> {
    #[derive(Deserialize)]
    struct Row {
        issue: u64,
        at: i64,
        by: String,
        how: String,
    }
    #[derive(Deserialize)]
    struct A {
        approval: Option<Row>,
    }
    let r = serde_json::from_str::<A>(answer).ok()?.approval?;
    (r.how == "approved")
        .then(|| acted(r.issue, Act::Approve, &r.by, r.at))
        .flatten()
}

/// `ticket.closed` of a ticket the factory follows, recorded closed now.
pub fn closed(ticket: TicketKey, reason: CloseReason, at: i64) -> TicketClosed {
    TicketClosed::new(ticket, reason, At::new(at))
}

/// How a limit reading's state word is an event's state.
fn limit_state(state: &str) -> Option<LimitState> {
    Some(match state {
        "none" => LimitState::None,
        "ok" => LimitState::Ok,
        "expired" => LimitState::Expired,
        "unreadable" => LimitState::Unreadable,
        _ => return None,
    })
}

/// `factory.limit-changed` of the limit `tehas limit set` or `tehas drain`
/// just wrote: `set` and `at` are the time the write put beside the limit,
/// so the raiser, the printer of `tehas limit --json` and a consumer that
/// polled all make one key. None of a reading with no `set`, which is no
/// write of tehas's.
pub fn written_limit(l: &limit::Limit) -> Option<LimitChanged> {
    let state = limit_state(l.state)?;
    let set = At::new(limit::epoch(l.set.as_deref()?)?);
    let field = |want: &str| {
        l.fields
            .iter()
            .find(|(name, _)| name == want)
            .map(|(_, text)| text.as_str())
    };
    let mut e = LimitChanged::written(set, state);
    e.cycles = field("cycles")
        .and_then(leading_number)
        .and_then(|n| n.parse().ok());
    e.plan = field("plan").map(|text| text == "true");
    e.until = field("until")
        .and_then(quoted)
        .and_then(|s| limit::epoch(&s))
        .map(At::new);
    e.reason = field("reason").and_then(quoted);
    Some(e)
}

/// `factory.limit-changed` of `tehas limit clear`: there is no `set` to
/// read after the clear, so tehas keys the change on the moment it made it.
pub fn cleared_limit(now: i64) -> LimitChanged {
    LimitChanged::observed(At::new(now), LimitState::None)
}

/// One verb of the store: the line it answered, or the word of its failure.
/// Each program asks through its own client.
pub type Ask<'a> = dyn FnMut(&Verb) -> Result<String, String> + 'a;

#[derive(Deserialize)]
struct Rows {
    rows: Vec<Value>,
}

#[derive(Deserialize)]
struct Shown {
    question: Option<QuestionRow>,
}

#[derive(Deserialize)]
struct Listed {
    questions: Vec<QuestionRow>,
}

/// The events of the end of cycle `attempt` of `issue` that `by` raises,
/// read from the store after the record. Why when the history could not be
/// read; a question that cannot be read leaves only `question.asked` out.
pub fn read_ended(
    ask: &mut Ask,
    issue: u64,
    attempt: u32,
    by: Writer,
    site_at: &mut dyn FnMut(ChangeNo) -> Option<At>,
) -> Result<Ended, String> {
    let rows: Rows = serde_json::from_str(&ask(&Verb::HistoryShow { issue: Some(issue) })?)
        .map_err(|e| format!("the history is not as the store writes it: {e}"))?;
    let lines: Vec<HistoryRow> = rows
        .rows
        .into_iter()
        .filter(|row| row.get("kind").and_then(Value::as_str) == Some("cycle"))
        .filter_map(|row| serde_json::from_value(row).ok())
        .collect();
    let question = match by {
        Writer::Report => ask(&Verb::QuestionShow { issue })
            .ok()
            .and_then(|l| serde_json::from_str::<Shown>(&l).ok())
            .and_then(|s| s.question),
        Writer::Short => None,
    };
    Ok(ended(
        issue,
        attempt,
        &lines,
        question.as_ref(),
        by,
        site_at,
    ))
}

/// `question.asked` of question `id`, the store's number, read from the
/// store after its record.
pub fn read_asked(ask: &mut Ask, id: u64) -> Result<Option<QuestionAsked>, String> {
    let after = Some(id.saturating_sub(1));
    let listed: Listed = serde_json::from_str(&ask(&Verb::QuestionList { after })?)
        .map_err(|e| format!("the questions are not as the store writes them: {e}"))?;
    Ok(listed.questions.iter().find(|q| q.id == id).and_then(asked))
}

/// The store's number of the question a `question-asked` answer names, when
/// that call recorded it: none for a replay of a question on record.
pub fn recorded(answer: &str) -> Option<u64> {
    #[derive(Deserialize)]
    struct Q {
        id: u64,
    }
    #[derive(Deserialize)]
    struct A {
        question: Q,
        already: bool,
    }
    let a: A = serde_json::from_str(answer).ok()?;
    (!a.already).then_some(a.question.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tehas_ports::events::{Heard, Name};

    fn row(attempt: u32, seq: u64, change: impl FnOnce(&mut HistoryRow)) -> HistoryRow {
        let mut r = HistoryRow {
            kind: "cycle".into(),
            issue: 12,
            attempt,
            at: 1_763_820_000.0,
            status: Some("handoff".into()),
            started_at: Some(1_763_812_800.0),
            rc: Some(10),
            seq,
            reported: true,
            ..HistoryRow::default()
        };
        change(&mut r);
        r
    }

    fn names(e: &Ended) -> Vec<Name> {
        e.envelopes().iter().map(Envelope::name).collect()
    }

    fn reported(issue: u64, attempt: u32, lines: &[HistoryRow], q: Option<&QuestionRow>) -> Ended {
        ended(issue, attempt, lines, q, Writer::Report, &mut |_| None)
    }

    #[test]
    fn a_cycle_that_left_a_change_ends_and_offers_it() {
        let lines = [row(3, 9, |r| r.pr = Some(21))];
        let e = reported(12, 3, &lines, None);
        assert_eq!(names(&e), [Name::CycleEnded, Name::ChangeOffered]);
        let end = e.ended.unwrap();
        assert_eq!(end.cycle.to_string(), "12-3");
        assert_eq!(end.end, End::Offered);
        assert_eq!(end.status, Some(10));
        assert_eq!(end.started.secs(), 1_763_812_800);
        let offer = e.offered.unwrap();
        assert_eq!((offer.change.get(), offer.at), (21, end.ended));
    }

    #[test]
    fn a_park_raises_its_park_its_question_and_its_end_in_order() {
        let park = |r: &mut HistoryRow| {
            r.parked = true;
            r.step = Some("develop".into());
            r.cap = Some("verify-failed:develop".into());
            r.failure_kind = Some("verify-failed".into());
        };
        let lines = [row(2, 4, park), row(3, 9, park), row(1, 1, |_| {})];
        let q = QuestionRow {
            id: 5,
            issue: 12,
            kind: "park".into(),
            attempt: 3,
            posted_at: 1_763_820_001,
            offers: Some(vec!["retry-text".into(), "park".into()]),
            options: None,
        };
        let e = reported(12, 3, &lines, Some(&q));
        assert_eq!(
            names(&e),
            [Name::CycleParked, Name::QuestionAsked, Name::CycleEnded]
        );
        let parked = e.parked.unwrap();
        assert_eq!(parked.repeat, NonZeroU32::new(1));
        assert_eq!(parked.end, tehas_ports::events::Stop::Handoff);
        let asked = e.asked.unwrap();
        assert_eq!(asked.cycle.map(|c| c.to_string()).as_deref(), Some("12-3"));
        let words: Vec<&str> = asked.choices.iter().map(|c| c.choice.as_str()).collect();
        assert_eq!(words, ["retry-text", "park"]);
        // Another attempt's question is not this end's.
        let other = QuestionRow { attempt: 2, ..q };
        assert!(reported(12, 3, &lines, Some(&other)).asked.is_none());
    }

    #[test]
    fn a_line_the_store_does_not_have_raises_nothing() {
        let lines = [row(2, 4, |_| {})];
        assert_eq!(reported(12, 3, &lines, None), Ended::default());
        let wordless = [row(3, 4, |r| r.status = None)];
        assert_eq!(reported(12, 3, &wordless, None), Ended::default());
    }

    // One line per start, and one writer of it: a short line is raised by
    // its writer alone, a reported line by the cycle alone.
    #[test]
    fn each_line_is_raised_by_its_writer_alone() {
        let short = [row(3, 9, |r| {
            r.status = Some("stopped".into());
            r.rc = None;
            r.reported = false;
        })];
        assert_eq!(reported(12, 3, &short, None), Ended::default());
        let e = ended(12, 3, &short, None, Writer::Short, &mut |_| None);
        assert_eq!(names(&e), [Name::CycleEnded]);
        assert_eq!(e.ended.unwrap().end, End::Stopped);
        let report = [row(3, 9, |_| {})];
        assert_eq!(
            ended(12, 3, &report, None, Writer::Short, &mut |_| None),
            Ended::default()
        );
        assert_eq!(names(&reported(12, 3, &report, None)), [Name::CycleEnded]);
    }

    // The 1.x hook asked when the cycle was not signalled, not refused, and
    // named a change; the bridge then asked only for one still open. The
    // event is narrower in one case: a line that ends `refused` by its exit
    // status alone (64, 78 and the rest of REFUSED_EXITS) is not offered,
    // where 1.x asked when the report did not refuse the start.
    #[test]
    fn every_1x_case_offers_a_change_as_the_hook_was_asked_but_a_refused_exit() {
        type Case = (&'static str, fn(&mut HistoryRow), bool);
        let cases: [Case; 7] = [
            ("left a change", |r| r.pr = Some(21), true),
            ("no change", |_| {}, false),
            (
                "signalled",
                |r| {
                    r.pr = Some(21);
                    r.signalled = true;
                },
                false,
            ),
            (
                "refused",
                |r| {
                    r.pr = Some(21);
                    r.status = Some("refused".into());
                    r.rc = Some(78);
                },
                false,
            ),
            (
                "refused by its exit status alone",
                |r| {
                    r.pr = Some(21);
                    r.rc = Some(78);
                },
                false,
            ),
            (
                "merged",
                |r| {
                    r.pr = Some(21);
                    r.status = Some("merged".into());
                    r.rc = Some(0);
                },
                false,
            ),
            (
                "parked with its change",
                |r| {
                    r.pr = Some(21);
                    r.parked = true;
                },
                true,
            ),
        ];
        for (case, change, asked) in cases {
            let e = reported(12, 3, &[row(3, 9, change)], None);
            assert_eq!(e.offered.is_some(), asked, "{case}");
        }
    }

    #[test]
    fn a_merged_line_ends_and_raises_its_merge_and_no_offer() {
        let merge = |r: &mut HistoryRow| {
            r.status = Some("merged".into());
            r.rc = Some(0);
            r.pr = Some(21);
        };
        let e = reported(12, 3, &[row(3, 9, merge)], None);
        assert_eq!(names(&e), [Name::CycleEnded, Name::ChangeMerged]);
        let m = e.merged.unwrap();
        assert_eq!((m.change.get(), m.at), (21, At::new(1_763_820_000)));
        assert_eq!(m.blueprint.map(|b| b.get()), Some(12));
        // A line that ends `merged` and parked is the runner's claim of a
        // merge the bot's list did not name: nothing was merged, nothing is
        // raised.
        let claimed = |r: &mut HistoryRow| {
            merge(r);
            r.parked = true;
        };
        assert_eq!(reported(12, 3, &[row(3, 9, claimed)], None).merged, None);
        // A merged line with no change names nothing to raise.
        let no_change = |r: &mut HistoryRow| {
            r.status = Some("merged".into());
            r.rc = Some(0);
        };
        assert_eq!(reported(12, 3, &[row(3, 9, no_change)], None).merged, None);
    }

    #[test]
    fn a_merged_line_keys_on_the_port_time_and_falls_back_to_the_line_end() {
        let merge = |r: &mut HistoryRow| {
            r.status = Some("merged".into());
            r.rc = Some(0);
            r.pr = Some(21);
        };
        let mut asked = |n: ChangeNo| {
            assert_eq!(n.get(), 21);
            Some(At::new(1_763_800_000))
        };
        let e = ended(12, 3, &[row(3, 9, merge)], None, Writer::Report, &mut asked);
        let m = e.merged.unwrap();
        assert_eq!(m.at, At::new(1_763_800_000));
        // Nothing from the port: the line's own end stands.
        let e = ended(
            12,
            3,
            &[row(3, 9, merge)],
            None,
            Writer::Report,
            &mut |_| None,
        );
        assert_eq!(e.merged.unwrap().at, At::new(1_763_820_000));
    }

    #[test]
    fn an_act_keeps_the_account_and_the_time_the_history_names() {
        let p = acted(12, Act::Approve, "alice", 1_763_820_000).unwrap();
        assert_eq!(
            (p.act, p.by.login.as_str(), p.at.secs()),
            (Act::Approve, "alice", 1_763_820_000)
        );
        assert_eq!(p.by.realm, Realm::Tickets);
        assert_eq!(p.by.id, None);
        assert_eq!(
            Envelope::new(p.clone()).key.as_str(),
            "12@Approve@2025-11-22T14:00:00Z"
        );
        assert!(acted(0, Act::Approve, "alice", 1).is_none());
        assert_eq!(acted(12, Act::Plan, "bob", 5).unwrap().act, Act::Plan);
        assert_eq!(acted(12, Act::Refine, "bob", 5).unwrap().act, Act::Refine);
        // The close of the plan's replaced session, and its key.
        let c = closed(
            TicketKey::Number(NonZeroU64::new(12).unwrap()),
            CloseReason::NotPlanned,
            1_763_820_000,
        );
        assert_eq!(c.reason, CloseReason::NotPlanned);
        assert_eq!(Envelope::new(c).key.as_str(), "12@2025-11-22T14:00:00Z");
    }

    #[test]
    fn a_ticket_closed_twice_is_two_facts_with_two_keys() {
        // The contract keys `ticket.closed` as `<ticket>@<at>`, so a ticket
        // closed, reopened and closed again is two facts, each keyed on the
        // close moment it records.
        let t = TicketKey::Number(NonZeroU64::new(12).unwrap());
        let first = Envelope::new(closed(t.clone(), CloseReason::NotPlanned, 1_763_820_000));
        let second = Envelope::new(closed(t, CloseReason::NotPlanned, 1_763_906_400));
        assert_ne!(first.key, second.key);
        assert_eq!(second.key.as_str(), "12@2025-11-23T14:00:00Z");
    }

    #[test]
    fn an_approval_record_answers_its_act_only_when_it_approves() {
        let a = r#"{"api":1,"verb":"approval-record","ok":true,"approval":{"issue":12,"at":1763820000,"by":"alice","how":"approved"}}"#;
        let p = approved(a).unwrap();
        assert_eq!(
            (p.act, p.by.login.as_str(), p.at.secs()),
            (Act::Approve, "alice", 1_763_820_000)
        );
        let retry = a.replace("approved", "outage-retry");
        assert_eq!(approved(&retry), None);
        assert_eq!(
            approved(r#"{"api":1,"verb":"approval-record","ok":true,"approval":null}"#),
            None
        );
        assert_eq!(approved("not json"), None);
    }

    #[test]
    fn a_written_limit_keys_on_set_and_a_clear_on_observed() {
        let l = limit::parse(
            br#"{"v":1,"cycles":2,"plan":false,"until":"2025-11-22T16:00:00Z","reason":"held for alpha","set":"2025-11-22T15:00:00Z"}"#,
            1_763_820_000,
        );
        let p = written_limit(&l).unwrap();
        assert_eq!(
            (p.state, p.cycles, p.plan, p.reason.as_deref()),
            (LimitState::Ok, Some(2), Some(false), Some("held for alpha"))
        );
        assert_eq!(p.set().map(|a| a.secs()), Some(1_763_823_600));
        assert_eq!(p.observed_at(), None);
        assert_eq!(p.until.map(|a| a.secs()), Some(1_763_827_200));
        assert_eq!(
            Envelope::new(p.clone()).key.as_str(),
            "2025-11-22T15:00:00Z"
        );
        // An expired limit is still the limit that was written.
        let l = limit::parse(
            br#"{"v":1,"cycles":2,"plan":true,"until":"2025-11-22T14:00:00Z","reason":"","set":"2025-11-22T13:00:00Z"}"#,
            1_763_820_000,
        );
        let p = written_limit(&l).unwrap();
        assert_eq!(
            (p.state, p.plan, p.reason.as_deref()),
            (LimitState::Expired, Some(true), None)
        );
        // A reading with no `set` is no write of tehas's: a consumer keys
        // what it polls on its own time.
        assert_eq!(written_limit(&limit::Limit::of("ok")), None);
        assert_eq!(
            written_limit(&limit::parse(br#"{"v":1,"cycles":1,"plan":true}"#, 1)),
            None
        );
        assert_eq!(written_limit(&limit::Limit::of("none")), None);
        let c = cleared_limit(1_763_820_000);
        assert_eq!(
            (c.state, c.set(), c.cycles, c.plan),
            (LimitState::None, None, None, None)
        );
        assert_eq!(c.observed_at().map(|a| a.secs()), Some(1_763_820_000));
        assert_eq!(Envelope::new(c).key.as_str(), "2025-11-22T14:00:00Z");
    }

    #[test]
    fn a_start_is_the_store_s_answer() {
        let s = StartRow {
            issue: 12,
            attempt: 4,
            started_at: 1_763_825_100,
            started_by: Some("alice".into()),
            how: Some("approved".into()),
            replaced: None,
        };
        let p = started(&s).unwrap();
        assert_eq!(p.cycle.to_string(), "12-4");
        assert_eq!(p.how, How::Approved);
        assert_eq!(
            p.by.map(|l| l.as_str().to_string()).as_deref(),
            Some("alice")
        );
        let hand = started(&StartRow {
            started_by: None,
            how: Some("hand".into()),
            ..s.clone()
        })
        .unwrap();
        assert_eq!((hand.how, hand.by), (How::Hand, None));
        assert!(started(&StartRow { attempt: 0, ..s }).is_none());
    }

    #[test]
    fn a_planning_question_has_no_cycle_and_its_choices_while_its_options_are_kept() {
        let q = QuestionRow {
            id: 7,
            issue: 12,
            kind: "plan".into(),
            attempt: 1,
            posted_at: 1_763_820_001,
            offers: None,
            options: Some(vec!["Adopt all".into()]),
        };
        let a = asked(&q).unwrap();
        assert_eq!(
            (a.kind, a.cycle, a.question.get()),
            (QuestionKind::Plan, None, 7)
        );
        assert!(a.choices.iter().any(|c| c.label == "Adopt all"), "{a:?}");
        let gone = asked(&QuestionRow { options: None, ..q }).unwrap();
        assert!(gone.choices.is_empty());
    }

    #[test]
    fn a_question_is_raised_by_the_call_that_recorded_it() {
        let a =
            r#"{"api":1,"verb":"question-asked","ok":true,"question":{"id":5},"already":false}"#;
        assert_eq!(recorded(a), Some(5));
        let replay = a.replace("\"already\":false", "\"already\":true");
        assert_eq!(recorded(&replay), None);
        assert_eq!(recorded("{}"), None);
        // An answer that does not say whether it recorded raises nothing.
        let unsaid = r#"{"api":1,"verb":"question-asked","ok":true,"question":{"id":5}}"#;
        assert_eq!(recorded(unsaid), None);
    }

    #[test]
    fn a_short_end_reads_no_question_and_a_failed_read_says_why() {
        let mut asked = Vec::new();
        let mut ask = |v: &Verb| -> Result<String, String> {
            asked.push(v.args().join(" "));
            Ok(r#"{"rows":[{"kind":"cycle","issue":12,"attempt":3,"at":10,"status":"stopped","reported":false,"seq":1}]}"#.into())
        };
        let e = read_ended(&mut ask, 12, 3, Writer::Short, &mut |_| None).unwrap();
        assert_eq!(names(&e), [Name::CycleEnded]);
        assert_eq!(asked, ["history-show 12"]);
        let mut fails = |_: &Verb| -> Result<String, String> { Err("busy".into()) };
        assert_eq!(
            read_ended(&mut fails, 12, 3, Writer::Report, &mut |_| None),
            Err("busy".into())
        );
    }

    #[test]
    fn the_log_hears_every_event_first_and_a_fault_fails_nothing() {
        let heard = Arc::new(Mutex::new(Vec::new()));
        let h = Arc::clone(&heard);
        let mut b = logged(move |l| h.lock().unwrap().push(l.to_string()));
        b.on_any("stops", 5, |_| Heard::Stop);
        b.on_any("never", 0, |_| panic!("called after a stop"));
        let d = b.build();
        let e = reported(12, 3, &[row(3, 9, |r| r.pr = Some(21))], None);
        let want: Vec<String> = e.envelopes().iter().map(line).collect();
        let warned = Mutex::new(Vec::new());
        e.raise(&d, &|l| warned.lock().unwrap().push(l.to_string()));
        assert_eq!(*heard.lock().unwrap(), want);
        assert!(want[0].starts_with("event {\"api\":1,"), "{}", want[0]);
        assert!(warned.lock().unwrap().is_empty());
        let mut b = logged(|_| {});
        b.on_any("fails", 0, |_| Heard::Failed("down".into()));
        let e = reported(12, 3, &[row(3, 9, |_| {})], None);
        e.raise(&b.build(), &|l| warned.lock().unwrap().push(l.to_string()));
        assert_eq!(
            *warned.lock().unwrap(),
            ["WARN: event cycle.ended 12-3: the listener fails failed: down"]
        );
    }
}
