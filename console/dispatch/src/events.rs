//! The events the dispatcher raises (docs/specs/2026-11-22-events.md,
//! T11.9): `cycle.ended` of a record its poll ended as lost, and
//! `question.asked` of each question it records, a planning turn's among
//! them. Each is raised only when the store answered that this call wrote
//! the record, so the poll that wrote it is the one that raises it. The
//! payloads are `tehas_core::raise`'s, built from the store's answer.
//!
//! The dispatcher is built once, at the poll's wiring in src/main.rs: the
//! log listener alone, one line per event in the poll's log.

use crate::apply::Writer;
use crate::clock::iso_now;
use std::cell::RefCell;
use tehas_core::raise::{self, EndAnswer, Ended};
use tehas_ports::events::Dispatcher;

/// The dispatcher of a poll: the log.
pub fn wiring() -> Dispatcher {
    raise::logged(|l| tehas_core::outln!("{} {l}", iso_now())).build()
}

impl Writer {
    fn ask(&mut self, args: &[&str]) -> Result<String, String> {
        self.store_try(3, args)
            .map(|v| v.to_string())
            .map_err(|f| f["word"].as_str().unwrap_or("unknown").to_string())
    }

    /// Raises `e`, with a WARN line in `log` for each listener that failed.
    fn raise_ended(&self, e: Ended, log: &mut Vec<String>) {
        let warned = RefCell::new(Vec::new());
        e.raise(&self.events, &|l| warned.borrow_mut().push(l.to_string()));
        log.extend(warned.into_inner());
    }

    /// The end of attempt `attempt` of `n`, when `answer`, the store's
    /// answer to `cycle-end --lost`, says this call wrote its line.
    pub(crate) fn raise_lost(&mut self, n: u64, attempt: u64, answer: &str, log: &mut Vec<String>) {
        let wrote = serde_json::from_str::<EndAnswer>(answer).is_ok_and(|a| !a.already);
        let Some(attempt) = u32::try_from(attempt).ok().filter(|_| wrote) else {
            return;
        };
        let mut ask = |args: &[&str]| self.ask(args);
        match raise::read_ended(&mut ask, n, attempt, raise::Writer::Short) {
            Ok(e) => self.raise_ended(e, log),
            Err(why) => log.push(format!(
                "WARN: the history of #{n} could not be read, so its lost end raises no event ({why})"
            )),
        }
    }

    /// `question.asked` of the question `answer`, the store's answer to
    /// `question-asked`, recorded. A replay of one on record raises nothing.
    pub(crate) fn raise_asked(&mut self, n: u64, answer: &str, log: &mut Vec<String>) {
        let Some(id) = raise::recorded(answer) else {
            return;
        };
        let mut ask = |args: &[&str]| self.ask(args);
        match raise::read_asked(&mut ask, id) {
            Ok(Some(p)) => {
                let warned = RefCell::new(Vec::new());
                raise::raise(&self.events, p, &|l| {
                    warned.borrow_mut().push(l.to_string())
                });
                log.extend(warned.into_inner());
            }
            Ok(None) => log.push(format!(
                "WARN: question {id} of #{n} is not in the store's list, so it raises no event"
            )),
            Err(why) => log.push(format!(
                "WARN: question {id} of #{n} could not be read, so it raises no event ({why})"
            )),
        }
    }
}
