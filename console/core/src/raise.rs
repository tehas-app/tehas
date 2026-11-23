//! The events tehas raises, made from what the state store answers after a
//! record commits (docs/specs/2026-11-22-events.md, T11.8 and T11.9). Each
//! payload is built from the store's answer, never from the raiser's own
//! view, so it carries what `tehas status`, `tehas history` and `tehas
//! question` print. One copy of each mapping, for every program that raises:
//! the cycle, `tehas stop` and the dispatcher.
//!
//! Each end is raised by one process, the one that wrote its line of
//! history. A line a report wrote (`reported`) is the cycle's: `tehas-cycle`
//! raises it after its report, also when the work's own push fault wrote it.
//! A line written with no report, of a refused start, `tehas stop`, the poll
//! or the next start, is raised by the process whose call the store answered
//! with the write: its `already` false, or for the next start its
//! `replaced`. The store writes one line per start, in one transaction, so
//! the two kinds of writer never both own one.

use crate::outcome::{self, ParkLine};
use crate::question::stored_choices;
use serde::Deserialize;
use serde_json::Value;
use std::num::NonZeroU32;
use tehas_ports::events::{
    Builder, ChangeOffered, CycleEnded, CycleId, CycleParked, CycleStarted, Dispatcher, End,
    Envelope, How, Payload, QuestionAsked, QuestionChoice, QuestionNo, RunnerWord,
};
use tehas_ports::{At, BlueprintNo, ChangeNo, Login, QuestionKind};

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
    Some(CycleStarted {
        cycle: CycleId::new(blueprint, NonZeroU32::new(s.attempt)?)?,
        blueprint,
        ticket: None,
        started: At::new(s.started_at),
        how: serde_json::from_value(Value::String(how.to_string())).unwrap_or(How::Unknown),
        by: s
            .started_by
            .clone()
            .filter(|l| !l.is_empty())
            .map(Login::new),
    })
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
        .map(|o| QuestionChoice {
            choice: o.answer_word(),
            label: o.label.clone(),
        })
        .collect()
    } else {
        Vec::new()
    };
    Some(QuestionAsked {
        question: QuestionNo::new(q.id)?,
        kind,
        blueprint,
        cycle,
        asked_at: At::new(q.posted_at),
        choices,
    })
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
    pub offered: Option<ChangeOffered>,
}

impl Ended {
    /// The envelopes, in the order they are raised.
    pub fn envelopes(&self) -> Vec<Envelope> {
        let mut all = Vec::new();
        all.extend(self.parked.clone().map(Envelope::new));
        all.extend(self.asked.clone().map(Envelope::new));
        all.extend(self.ended.clone().map(Envelope::new));
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
/// does not have, or one another writer wrote.
pub fn ended(
    issue: u64,
    attempt: u32,
    lines: &[HistoryRow],
    question: Option<&QuestionRow>,
    by: Writer,
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
    let parked = end.stop().filter(|_| line.parked).map(|stop| CycleParked {
        cycle,
        blueprint,
        ticket: None,
        at: ended_at,
        end: stop,
        step: word(HistoryRow::word(&line.step)),
        cap: word(HistoryRow::word(&line.cap)),
        park_reason: word(HistoryRow::word(&line.park_reason)),
        failure_kind: word(HistoryRow::word(&line.failure_kind)),
        failure_cause: word(HistoryRow::word(&line.failure_cause)),
        provider: line.provider,
        repeat: u32::try_from(repeats[at]).ok().and_then(NonZeroU32::new),
        change,
    });
    let asked = question
        .filter(|q| q.kind == "park" && q.issue == issue && q.attempt == attempt)
        .and_then(asked);
    let ended = CycleEnded {
        cycle,
        blueprint,
        ticket: None,
        started: At::new(line.started_at.unwrap_or(line.at).floor() as i64),
        ended: ended_at,
        recorded: line.recorded_at.map(|r| At::new(r.floor() as i64)),
        end,
        outcome: status,
        status: line.rc.and_then(|rc| u8::try_from(rc).ok()),
        parked: line.parked,
        signalled: line.signalled,
        change,
    };
    Ended {
        parked,
        asked,
        offered: offered(&ended),
        ended: Some(ended),
    }
}

/// `change.offered` of an end: when its line names a change, no signal
/// stopped it, and it did not end `refused` or `merged`. The 1.x hook's
/// rule, and a merged change was skipped by its open check.
pub fn offered(e: &CycleEnded) -> Option<ChangeOffered> {
    let free = !e.signalled && !matches!(e.end, End::Refused | End::Merged);
    Some(ChangeOffered {
        change: e.change.filter(|_| free)?,
        blueprint: e.blueprint,
        cycle: e.cycle,
        end: e.end,
        at: e.ended,
    })
}

/// One verb of the store: the line it answered, or the word of its failure.
/// Each program asks through its own client.
pub type Ask<'a> = dyn FnMut(&[&str]) -> Result<String, String> + 'a;

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
pub fn read_ended(ask: &mut Ask, issue: u64, attempt: u32, by: Writer) -> Result<Ended, String> {
    let n = issue.to_string();
    let rows: Rows = serde_json::from_str(&ask(&["history-show", &n])?)
        .map_err(|e| format!("the history is not as the store writes it: {e}"))?;
    let lines: Vec<HistoryRow> = rows
        .rows
        .into_iter()
        .filter(|row| row.get("kind").and_then(Value::as_str) == Some("cycle"))
        .filter_map(|row| serde_json::from_value(row).ok())
        .collect();
    let question = match by {
        Writer::Report => ask(&["question-show", &n])
            .ok()
            .and_then(|l| serde_json::from_str::<Shown>(&l).ok())
            .and_then(|s| s.question),
        Writer::Short => None,
    };
    Ok(ended(issue, attempt, &lines, question.as_ref(), by))
}

/// `question.asked` of question `id`, the store's number, read from the
/// store after its record.
pub fn read_asked(ask: &mut Ask, id: u64) -> Result<Option<QuestionAsked>, String> {
    let after = id.saturating_sub(1).to_string();
    let listed: Listed = serde_json::from_str(&ask(&["question-list", "--after", &after])?)
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
        #[serde(default)]
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
        ended(issue, attempt, lines, q, Writer::Report)
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
        let e = ended(12, 3, &short, None, Writer::Short);
        assert_eq!(names(&e), [Name::CycleEnded]);
        assert_eq!(e.ended.unwrap().end, End::Stopped);
        let report = [row(3, 9, |_| {})];
        assert_eq!(ended(12, 3, &report, None, Writer::Short), Ended::default());
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
    }

    #[test]
    fn a_short_end_reads_no_question_and_a_failed_read_says_why() {
        let mut asked = Vec::new();
        let mut ask = |a: &[&str]| -> Result<String, String> {
            asked.push(a.join(" "));
            Ok(r#"{"rows":[{"kind":"cycle","issue":12,"attempt":3,"at":10,"status":"stopped","reported":false,"seq":1}]}"#.into())
        };
        let e = read_ended(&mut ask, 12, 3, Writer::Short).unwrap();
        assert_eq!(names(&e), [Name::CycleEnded]);
        assert_eq!(asked, ["history-show 12"]);
        let mut fails = |_: &[&str]| -> Result<String, String> { Err("busy".into()) };
        assert_eq!(
            read_ended(&mut fails, 12, 3, Writer::Report),
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
