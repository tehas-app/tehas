//! The events a cycle raises (docs/specs/2026-11-22-events.md, T11.8 and
//! T11.9): `cycle.started` once its start is on record, and after its report
//! `cycle.parked`, `question.asked`, `cycle.ended` and `change.offered`, in
//! that order. A start refused before its runner raises its `cycle.ended`
//! itself, and a start that ended an open record as lost raises that
//! record's end before its own start. The payloads are built by
//! `tehas_core::raise` from what the store answers after the record. A
//! listener never fails the cycle: its faults are logged, one WARN line each.
//!
//! The dispatcher is built once, at the cycle's wiring in src/main.rs: the
//! log listener, one line per event with its envelope, and while plugins are
//! on the plugins' bridge (src/hook.rs).

use crate::after::log;
use crate::host;
use std::path::Path;
use std::sync::Arc;
use tehas_core::raise::{self, Ended, Writer};
use tehas_ports::events::{Dispatcher, Payload};

/// The settings the listeners read, as the cycle's environment has them.
pub type Get = Arc<dyn Fn(&str) -> String + Send + Sync>;

/// The dispatcher of a cycle: the log, and the plugins' bridge while the
/// feature `plugins` is on.
pub fn wiring(home: &Path, get: Get) -> Dispatcher {
    let mut b = raise::logged(log);
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
    raise::raise(d, p, &log);
}

/// The events of the end of cycle `attempt` of `issue` that `by` raises,
/// read from the store after its record. A read that fails raises nothing,
/// and says so. `merged_at` is the site's time of the merged pull request as
/// the report read it; the short writers have none.
pub fn read_ended(
    home: &Path,
    issue: u64,
    attempt: u32,
    by: Writer,
    merged_at: Option<tehas_ports::At>,
) -> Ended {
    let mut ask = |args: &[&str]| {
        host::store_try(home, 3, args)
            .map(|v| v.to_string())
            .map_err(|r| r.word)
    };
    match raise::read_ended(&mut ask, issue, attempt, by, merged_at) {
        Ok(e) => e,
        Err(why) => {
            log(&format!(
                "WARN: #{issue}: the history could not be read, so the end raises no event ({why})"
            ));
            Ended::default()
        }
    }
}

/// Raises the end of cycle `attempt` of `issue` that `by` wrote.
pub fn raise_ended(
    d: &Dispatcher,
    home: &Path,
    issue: u64,
    attempt: u32,
    by: Writer,
    merged_at: Option<tehas_ports::At>,
) {
    let e = read_ended(home, issue, attempt, by, merged_at);
    if by == Writer::Report && e.ended.is_none() {
        log(&format!(
            "#{issue}: no line of attempt {attempt} that its report wrote is in the history, so its end raises no event here"
        ));
    }
    e.raise(d, &log);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tehas_core::raise::{ended, HistoryRow};

    #[test]
    fn the_log_line_is_event_and_the_envelope_as_compact_json_byte_for_byte() {
        use tehas_core::raise::line;
        use tehas_ports::events::{samples, Event, Name, Source};
        let asked = samples()
            .into_iter()
            .find(|s| s.name == Name::QuestionAsked)
            .unwrap();
        let name = Name::QuestionAsked.as_str();
        let relayed = asked.full.clone().from(Source::new("tehas-tehas").unwrap());
        assert_eq!(
            line(&relayed),
            format!(
                r#"event {{"api":1,"source":"tehas-tehas","event":"{name}","data":{{"question":5,"kind":"Park","blueprint":12,"cycle":"12-3","askedAt":"2026-11-22T13:20:00Z","choices":[{{"choice":"retry","label":"Run the cycle again"}}]}},"when":"after","key":"5","at":"2026-11-22T13:20:00Z"}}"#
            )
        );
        assert_eq!(
            line(&asked.bare),
            format!(
                r#"event {{"api":1,"event":"{name}","data":{{"question":5,"kind":"Park","blueprint":12,"askedAt":"2026-11-22T13:20:00Z"}},"when":"after","key":"5","at":"2026-11-22T13:20:00Z"}}"#
            )
        );
        let mut unknown = asked.full;
        unknown.event = Event::Unknown;
        assert_eq!(line(&unknown), "event unknown 5");
    }

    #[test]
    fn a_cycle_wires_the_log_alone_while_plugins_are_off() {
        let off: Get = Arc::new(|_| String::new());
        let d = wiring(Path::new("/nonexistent"), off);
        let line = HistoryRow {
            kind: "cycle".into(),
            issue: 12,
            attempt: 3,
            at: 1_763_820_000.0,
            status: Some("handoff".into()),
            rc: Some(10),
            pr: Some(21),
            seq: 9,
            reported: true,
            ..HistoryRow::default()
        };
        let e = ended(12, 3, &[line], None, Writer::Report, None);
        // The bridge would read GitHub; with plugins off nothing but the
        // log hears the offer.
        let r = d.raise(e.offered.unwrap());
        assert_eq!(r, Default::default());
        assert_ne!(raise::LOG, crate::hook::BRIDGE);
    }
}
