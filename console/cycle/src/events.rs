//! The events a cycle raises (docs/specs/2026-11-22-events.md, T11.8):
//! `cycle.started` once its start is on record, and after its report
//! `cycle.parked`, `question.asked`, `cycle.ended` and `change.offered`, in
//! that order. Each payload is built from what the store answers after the
//! record commits, never from the cycle's own view, so it carries what
//! `tehas status`, `tehas history` and `tehas question` print. A listener
//! never fails the cycle: its faults are logged, one WARN line each.
//!
//! The dispatcher is built once, at the cycle's wiring in src/main.rs: the
//! log listener, one line per event with its envelope, and while plugins are
//! on the plugins' bridge (src/hook.rs).

use crate::after::log;
use crate::host;
use serde::Deserialize;
use serde_json::Value;
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::Arc;
use tehas_core::outcome::{self, ParkLine};
use tehas_core::question::stored_choices;
use tehas_ports::events::{
    Builder, ChangeOffered, CycleEnded, CycleId, CycleParked, CycleStarted, Dispatcher, End,
    Envelope, How, Payload, QuestionAsked, QuestionChoice, QuestionNo, RunnerWord,
};
use tehas_ports::{At, BlueprintNo, ChangeNo, Login, QuestionKind};

/// The settings the listeners read, as the cycle's environment has them.
pub type Get = Arc<dyn Fn(&str) -> String + Send + Sync>;

/// The log listener's id.
pub const LOG: &str = "log";

/// The dispatcher of a cycle: the log, and the plugins' bridge while the
/// feature `plugins` is on.
pub fn wiring(home: &Path, get: Get) -> Dispatcher {
    let mut b = Builder::new();
    b.log(LOG, |e| log(&e.line()));
    if tehas_core::features::on_in("plugins", &*get) {
        b.on_any(
            crate::hook::BRIDGE,
            0,
            crate::hook::bridge(home.to_path_buf(), get),
        );
    }
    b.build()
}

/// Raises `p`, and logs each listener that failed.
pub fn raise<P: Payload>(d: &Dispatcher, p: P) {
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

/// What the store answered a start with (`cycle-start`).
#[derive(Clone, Debug, Default, Deserialize)]
pub struct StartRow {
    pub issue: u64,
    pub attempt: u32,
    pub started_at: i64,
    #[serde(default)]
    pub started_by: Option<String>,
    #[serde(default)]
    pub how: Option<String>,
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

/// A question as `question-show` answers it.
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

    /// Raises each, in order.
    pub fn raise(self, d: &Dispatcher) {
        if let Some(p) = self.parked {
            raise(d, p);
        }
        if let Some(p) = self.asked {
            raise(d, p);
        }
        if let Some(p) = self.ended {
            raise(d, p);
        }
        if let Some(p) = self.offered {
            raise(d, p);
        }
    }
}

fn word(w: Option<&str>) -> Option<RunnerWord> {
    w.and_then(|w| RunnerWord::new(w).ok())
}

/// The events of the end of cycle `attempt` of `issue`, from the issue's
/// lines of history (`history-show <issue>`) and the question it waits on
/// (`question-show <issue>`). None of a line the store does not have.
pub fn ended(
    issue: u64,
    attempt: u32,
    lines: &[HistoryRow],
    question: Option<&QuestionRow>,
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
        .and_then(|q| {
            let choices = stored_choices(&q.kind, q.offers.as_deref(), &[])
                .iter()
                .map(|o| QuestionChoice {
                    choice: o.answer_word(),
                    label: o.label.clone(),
                })
                .collect();
            Some(QuestionAsked {
                question: QuestionNo::new(q.id)?,
                kind: QuestionKind::Park,
                blueprint,
                cycle: Some(cycle),
                asked_at: At::new(q.posted_at),
                choices,
            })
        });
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

#[derive(Deserialize)]
struct Rows {
    rows: Vec<Value>,
}

#[derive(Deserialize)]
struct Shown {
    question: Option<QuestionRow>,
}

/// The events of the end of cycle `attempt` of `issue`, read from the store
/// after its record. A read that fails raises nothing, and says so.
pub fn read_ended(home: &Path, issue: u64, attempt: u32) -> Ended {
    let n = issue.to_string();
    let rows = host::store_try(home, 3, &["history-show", &n])
        .map_err(|r| r.word)
        .and_then(|v| serde_json::from_value::<Rows>(v).map_err(|e| e.to_string()));
    let lines: Vec<HistoryRow> = match rows {
        Ok(r) => r
            .rows
            .into_iter()
            .filter(|row| row.get("kind").and_then(Value::as_str) == Some("cycle"))
            .filter_map(|row| serde_json::from_value(row).ok())
            .collect(),
        Err(why) => {
            log(&format!(
                "WARN: #{issue}: the history could not be read, so the end raises no event ({why})"
            ));
            return Ended::default();
        }
    };
    let question = host::store_try(home, 3, &["question-show", &n])
        .ok()
        .and_then(|v| serde_json::from_value::<Shown>(v).ok())
        .and_then(|s| s.question);
    let e = ended(issue, attempt, &lines, question.as_ref());
    if e.ended.is_none() {
        log(&format!(
            "WARN: #{issue}: no line of attempt {attempt} is in the history, so the end raises no event"
        ));
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
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
            ..HistoryRow::default()
        };
        change(&mut r);
        r
    }

    fn names(e: &Ended) -> Vec<Name> {
        e.envelopes().iter().map(Envelope::name).collect()
    }

    #[test]
    fn a_cycle_that_left_a_change_ends_and_offers_it() {
        let lines = [row(3, 9, |r| r.pr = Some(21))];
        let e = ended(12, 3, &lines, None);
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
        let e = ended(12, 3, &lines, Some(&q));
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
        assert!(ended(12, 3, &lines, Some(&other)).asked.is_none());
    }

    #[test]
    fn a_line_the_store_does_not_have_raises_nothing() {
        let lines = [row(2, 4, |_| {})];
        assert_eq!(ended(12, 3, &lines, None), Ended::default());
        let wordless = [row(3, 4, |r| r.status = None)];
        assert_eq!(ended(12, 3, &wordless, None), Ended::default());
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
            let e = ended(12, 3, &[row(3, 9, change)], None);
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
    fn the_log_hears_every_event_first_and_a_fault_fails_nothing() {
        let heard = Arc::new(Mutex::new(Vec::new()));
        let mut b = Builder::new();
        let h = Arc::clone(&heard);
        b.log(LOG, move |e| h.lock().unwrap().push(e.line()));
        b.on_any("stops", 5, |_| Heard::Stop);
        b.on_any("never", 0, |_| panic!("called after a stop"));
        let d = b.build();
        let e = ended(12, 3, &[row(3, 9, |r| r.pr = Some(21))], None);
        let want: Vec<String> = e.envelopes().iter().map(Envelope::line).collect();
        e.raise(&d);
        assert_eq!(*heard.lock().unwrap(), want);
        assert!(want[0].starts_with("event {\"api\":1,"), "{}", want[0]);
    }

    // The line the log wrote before `Envelope::line` owned it.
    fn serde_json_line(e: &Envelope) -> String {
        match serde_json::to_string(e) {
            Ok(json) => format!("event {json}"),
            Err(_) => format!("event {} {}", e.name().as_str(), e.key.as_str()),
        }
    }

    #[test]
    fn the_log_line_is_what_serde_json_writes_byte_for_byte() {
        let source = tehas_ports::events::Source::new("tehas-tehas").unwrap();
        let mut all = Vec::new();
        for s in tehas_ports::events::samples() {
            all.push(s.full.clone().from(source.clone()));
            all.extend([s.full, s.bare]);
        }
        let e = ended(12, 3, &[row(3, 9, |r| r.pr = Some(21))], None);
        all.extend(e.envelopes());
        let asked = all
            .iter()
            .find(|e| e.name() == Name::QuestionAsked)
            .unwrap();
        let mut q = QuestionAsked::peek(&asked.event).unwrap().clone();
        q.choices = vec![QuestionChoice {
            choice: "odd".into(),
            label: "a \"b\" \\ \n\r\t\u{8}\u{c}\u{1}\u{1f}\u{7f} \u{e9}/\u{2028}".into(),
        }];
        all.push(Envelope::new(q));
        let mut unknown = all[0].clone();
        unknown.event = tehas_ports::events::Event::Unknown;
        all.push(unknown);
        for e in &all {
            assert_eq!(e.line(), serde_json_line(e));
        }
    }

    #[test]
    fn a_cycle_wires_the_log_alone_while_plugins_are_off() {
        let off: Get = Arc::new(|_| String::new());
        let d = wiring(Path::new("/nonexistent"), off);
        let e = ended(12, 3, &[row(3, 9, |r| r.pr = Some(21))], None);
        // The bridge would read GitHub; with plugins off nothing but the
        // log hears the offer.
        let r = d.raise(e.offered.unwrap());
        assert_eq!(r, Default::default());
        assert_ne!(LOG, crate::hook::BRIDGE);
    }
}
