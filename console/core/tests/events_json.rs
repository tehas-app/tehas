//! The events' envelope in JSON: the spec's two examples read as an
//! `Envelope` and write back as the same JSON, and each sample reads back
//! from its JSON text as it was written. An event's name is written only in
//! the events module, so the texts here say `NAME` and take it from there.

use tehas_ports::events::*;

fn named(text: &str, name: Name) -> String {
    text.replace("NAME", name.as_str())
}

const ENDED: &str = r#"{"api": 1, "event": "NAME", "when": "after", "key": "12-3",
 "at": "2026-11-22T13:20:00Z",
 "data": {"cycle": "12-3", "blueprint": 12, "started": "2026-11-22T11:20:00Z",
          "ended": "2026-11-22T13:20:00Z", "end": "failed", "outcome": "failed",
          "status": 10, "parked": true, "change": 9}}"#;

const RELAYED: &str = r#"{"api": 1, "source": "tehas-tehas", "event": "NAME", "when": "after",
 "key": "12-4", "at": "2026-11-22T13:25:00Z",
 "data": {"cycle": "12-4", "blueprint": 12, "started": "2026-11-22T13:25:00Z",
          "how": "approved", "by": "alice"}}"#;

#[test]
fn the_specs_examples_read_and_write_back_as_the_same_json() {
    let (ended, relayed) = (
        named(ENDED, Name::CycleEnded),
        named(RELAYED, Name::CycleStarted),
    );
    for text in [&ended, &relayed] {
        let e: Envelope = serde_json::from_str(text).unwrap_or_else(|f| panic!("{text}: {f}"));
        let back = serde_json::to_value(&e).unwrap();
        let want: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(back, want);
    }
    let e: Envelope = serde_json::from_str(&ended).unwrap();
    let Event::CycleEnded(p) = &e.event else {
        panic!("not cycle.ended: {e:?}")
    };
    assert_eq!((p.end, p.parked, p.signalled), (End::Failed, true, false));
    assert_eq!(p.key(), e.key);
    assert_eq!(p.at(), e.at);
    let relayed: Envelope = serde_json::from_str(&relayed).unwrap();
    assert_eq!(
        relayed.source.as_ref().map(Source::as_str),
        Some("tehas-tehas")
    );
    assert_eq!(relayed.name(), Name::CycleStarted);
}

#[test]
fn every_sample_reads_back_from_its_json_text() {
    for s in samples() {
        for e in [&s.full, &s.bare] {
            let json = serde_json::to_string(e).unwrap();
            let back: Envelope =
                serde_json::from_str(&json).unwrap_or_else(|f| panic!("{json}: {f}"));
            assert_eq!(&back, e, "{json}");
            assert!(
                !json.contains("null"),
                "a field with no value is left out: {json}"
            );
        }
    }
}

#[test]
fn an_unknown_name_or_unreadable_data_reads_as_unknown_in_json() {
    for text in [
        r#"{"api":1,"event":"ticket.reopened","when":"after","key":"12","at":"2026-11-22T13:20:00Z","data":{"x":[1]}}"#,
        r#"{"api":1,"event":"ticket.reopened","when":"after","key":"12","at":"2026-11-22T13:20:00Z"}"#,
        r#"{"api":1,"event":"NAME","when":"after","key":"12-3","at":"2026-11-22T13:20:00Z","data":{"cycle":"x"}}"#,
    ] {
        let text = named(text, Name::CycleEnded);
        let e: Envelope = serde_json::from_str(&text).unwrap_or_else(|f| panic!("{text}: {f}"));
        assert_eq!(e.event, Event::Unknown, "{text}");
    }
    let bad_time = r#"{"api":1,"event":"ticket.reopened","when":"after","key":"12","at":"2026-11-22T13:20:00+02:00"}"#;
    assert!(serde_json::from_str::<Envelope>(bad_time).is_err());
}

const ACTED: &str = r#"{"api": 1, "event": "NAME", "when": "after",
 "key": "12@Approve@2026-11-22T11:10:00Z", "at": "2026-11-22T11:10:00Z",
 "data": {"ticket": {"Number": 12}, "act": "Approve",
          "by": {"realm": "Tickets", "login": "alice", "id": "1001", "kind": "Person"},
          "at": "2026-11-22T11:10:00Z"}}"#;

const CLOSED: &str = r#"{"api": 1, "event": "NAME", "when": "after",
 "key": "12@2026-11-22T15:20:00Z", "at": "2026-11-22T15:20:00Z",
 "data": {"ticket": {"Number": 12}, "reason": "Done", "at": "2026-11-22T15:20:00Z"}}"#;

const ASKED: &str = r#"{"api": 1, "event": "NAME", "when": "after", "key": "5",
 "at": "2026-11-22T13:20:00Z",
 "data": {"question": 5, "kind": "Park", "blueprint": 12, "cycle": "12-3",
          "askedAt": "2026-11-22T13:20:00Z",
          "choices": [{"choice": "retry", "label": "Run the cycle again"}]}}"#;

// The spec's envelopes with port types are the samples, port types in the
// ports' own shape.
#[test]
fn the_specs_envelopes_with_port_types_are_the_samples() {
    for (text, name) in [
        (ACTED, Name::TicketActed),
        (CLOSED, Name::TicketClosed),
        (ASKED, Name::QuestionAsked),
    ] {
        let text = named(text, name);
        let e: Envelope = serde_json::from_str(&text).unwrap_or_else(|f| panic!("{text}: {f}"));
        let sample = samples().into_iter().find(|s| s.name == name).unwrap();
        assert_eq!(e, sample.full, "{text}");
        let want: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(serde_json::to_value(&e).unwrap(), want);
    }
}
