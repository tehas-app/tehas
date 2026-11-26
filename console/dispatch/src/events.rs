//! The events the dispatcher raises (docs/specs/2026-11-22-events.md,
//! T11.9): `cycle.ended` of a record its poll ended as lost, and
//! `question.asked` of each question it records, a planning turn's among
//! them. Each is raised only when the store answered that this call wrote
//! the record, so the poll that wrote it is the one that raises it. The
//! payloads are `tehas_core::raise`'s, built from the store's answer.
//!
//! The dispatcher is also the raiser of the facts its own store calls
//! record (T11.9): `ticket.acted` of the approval, the planning session and
//! the finish ask it puts on the record, and `ticket.closed` of the close
//! its write lands. Each is raised once, by the call that wrote the fact.
//!
//! The dispatcher is built once, at the poll's wiring in src/main.rs: the
//! log listener alone, one line per event in the poll's log.

use crate::apply::Writer;
use crate::clock::iso_now;
use std::cell::RefCell;
use tehas_core::raise::{self, EndAnswer, Ended};
use tehas_ports::events::{Dispatcher, Payload};
use tehas_ports::{CloseReason, TicketKey};

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

    /// Raises one payload, with a WARN line in `log` for each listener that
    /// failed.
    fn raise_one<P: Payload>(&self, p: P, log: &mut Vec<String>) {
        let warned = RefCell::new(Vec::new());
        raise::raise(&self.events, p, &|l| {
            warned.borrow_mut().push(l.to_string())
        });
        log.extend(warned.into_inner());
    }

    /// `ticket.acted` of the approval this writer's `approval-record` call
    /// put on the record. Only an approval of a person is an act; the
    /// store's answer carries the account and the label event's moment.
    pub(crate) fn raise_approved(&mut self, answer: &str, log: &mut Vec<String>) {
        let Some(p) = raise::approved(answer) else {
            return;
        };
        self.raise_one(p, log);
    }

    /// `ticket.acted` of the planning session this writer's `plan-begin`
    /// call began: the act of `by` asking for a session, keyed on the
    /// moment the label event says. The caller names the act: only the
    /// planning ones come here, and a session the poll started on its own
    /// never does.
    pub(crate) fn raise_plan(
        &mut self,
        n: u64,
        act: tehas_ports::Act,
        by: &str,
        at: i64,
        log: &mut Vec<String>,
    ) {
        let Some(p) = raise::acted(n, act, by, at) else {
            return;
        };
        self.raise_one(p, log);
    }

    /// `ticket.acted` of the finish a person asked for and the finish step
    /// took: the act of `by`, keyed on the moment their label event says.
    pub(crate) fn raise_finish(&mut self, n: u64, by: &str, at: i64, log: &mut Vec<String>) {
        let Some(p) = raise::acted(n, tehas_ports::Act::Finish, by, at) else {
            return;
        };
        self.raise_one(p, log);
    }

    /// `ticket.closed` of the ticket this writer's write closed.
    pub(crate) fn raise_closed(
        &mut self,
        ticket: TicketKey,
        reason: CloseReason,
        at: i64,
        log: &mut Vec<String>,
    ) {
        self.raise_one(raise::closed(ticket, reason, at), log);
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
