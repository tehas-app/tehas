//! The events at their interface: the dispatcher's order, stop and faults,
//! the envelope read and written, the keys the payloads make, and the
//! samples.

mod tree;

use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};

use tehas_ports::events::*;
use tehas_ports::{Act, At, BlueprintNo, KeyText, QuestionKind, TicketKey};
use tree::{from_node, map, text, to_node, Node};

type Calls = Arc<Mutex<Vec<&'static str>>>;

fn sample(name: Name) -> Sample {
    samples()
        .into_iter()
        .find(|s| s.name == name)
        .expect("a sample of every name")
}

fn payload<P: Payload>(s: &Sample) -> P {
    P::peek(&s.full.event)
        .expect("the sample's own payload")
        .clone()
}

fn ended() -> CycleEnded {
    payload(&sample(Name::CycleEnded))
}

fn note(calls: &Calls, id: &'static str, answer: Heard) -> impl Fn(&CycleEnded) -> Heard {
    let calls = calls.clone();
    move |_| {
        calls.lock().unwrap().push(id);
        answer.clone()
    }
}

#[test]
fn listeners_run_by_priority_then_in_the_order_registered() {
    let calls: Calls = Arc::default();
    let mut b = Builder::new();
    b.on::<CycleEnded>("a", 0, note(&calls, "a", Heard::Go))
        .on::<CycleEnded>("b", 5, note(&calls, "b", Heard::Go))
        .on::<CycleEnded>("c", 0, note(&calls, "c", Heard::Go))
        .on::<CycleEnded>("d", -1, note(&calls, "d", Heard::Go));
    let log = calls.clone();
    b.log("log", move |e| {
        assert_eq!(e.name(), Name::CycleEnded);
        log.lock().unwrap().push("log");
    });
    // Another name's listener is not called.
    let other = calls.clone();
    b.on::<CycleStarted>("started", 9, move |_| {
        other.lock().unwrap().push("started");
        Heard::Go
    });
    let raised = b.build().raise(ended());
    assert_eq!(raised, Raised::default());
    assert_eq!(*calls.lock().unwrap(), ["log", "b", "a", "c", "d"]);
}

#[test]
fn a_stop_ends_the_list_and_names_its_listener() {
    let calls: Calls = Arc::default();
    let mut b = Builder::new();
    b.on::<CycleEnded>("first", 10, note(&calls, "first", Heard::Go))
        .on::<CycleEnded>("stopper", 5, note(&calls, "stopper", Heard::Stop))
        .on::<CycleEnded>("later", 0, note(&calls, "later", Heard::Go));
    let d = b.build();
    let raised = d.raise(ended());
    assert_eq!(raised.stopped_by, Some("stopper"));
    assert!(raised.faults.is_empty());
    assert_eq!(*calls.lock().unwrap(), ["first", "stopper"]);
    // The stop is of that raise alone.
    calls.lock().unwrap().clear();
    d.raise(ended());
    assert_eq!(*calls.lock().unwrap(), ["first", "stopper"]);
}

#[test]
fn a_failure_and_a_panic_are_faults_handed_back_and_the_next_listener_runs() {
    let calls: Calls = Arc::default();
    let mut b = Builder::new();
    b.on::<CycleEnded>(
        "fails",
        3,
        note(&calls, "fails", Heard::Failed("no disk".into())),
    )
    .on::<CycleEnded>("panics", 2, |_| panic!("listener broke"))
    .on::<CycleEnded>("owned", 1, |_| {
        std::panic::panic_any(String::from("owned text"))
    })
    .on::<CycleEnded>("after", 0, note(&calls, "after", Heard::Go));
    let raised = b.build().raise(ended());
    assert_eq!(raised.stopped_by, None);
    assert_eq!(
        raised.faults,
        [
            Fault {
                listener: "fails",
                why: "no disk".into()
            },
            Fault {
                listener: "panics",
                why: "panicked: listener broke".into()
            },
            Fault {
                listener: "owned",
                why: "panicked: owned text".into()
            },
        ]
    );
    assert_eq!(*calls.lock().unwrap(), ["fails", "after"]);
}

#[test]
fn a_dispatcher_with_no_listener_raises_nothing_and_is_send_and_sync() {
    fn shared<T: Send + Sync>(_: &T) {}
    let d = Builder::new().build();
    shared(&d);
    assert_eq!(d.raise(ended()), Raised::default());
}

#[test]
fn each_payload_makes_its_key_and_time_from_the_fact() {
    let want = [
        (Name::CycleStarted, "12-4", "2026-11-22T11:25:00Z"),
        (Name::CycleParked, "12-3", "2026-11-22T13:20:00Z"),
        (Name::QuestionAsked, "5", "2026-11-22T13:20:00Z"),
        (Name::CycleEnded, "12-3", "2026-11-22T13:20:00Z"),
        (Name::ChangeOffered, "12-4", "2026-11-22T14:20:00Z"),
        (
            Name::FactoryLimitChanged,
            "2026-11-22T14:40:00Z",
            "2026-11-22T14:40:00Z",
        ),
        (
            Name::TicketActed,
            "12@Approve@2026-11-22T11:10:00Z",
            "2026-11-22T11:10:00Z",
        ),
        (
            Name::TicketClosed,
            "12@2026-11-22T15:20:00Z",
            "2026-11-22T15:20:00Z",
        ),
        (Name::ChangeMerged, "21", "2026-11-22T15:10:00Z"),
    ];
    for (name, key, at) in want {
        let s = sample(name);
        assert_eq!(s.full.key.as_str(), key, "{name:?}");
        assert_eq!(rfc3339::text(s.full.at).as_deref(), Some(at), "{name:?}");
        assert_eq!(s.full.when, When::After);
        assert_eq!(s.full.source, None);
        assert_eq!(s.full.api, API);
    }
    // A clear has no `set`: it is keyed on its own time.
    let clear = sample(Name::FactoryLimitChanged).bare;
    assert_eq!(clear.key.as_str(), "2026-11-22T16:50:00Z");
    // A key of a ticket by its key, not its number.
    let mut acted: TicketActed = payload(&sample(Name::TicketActed));
    acted.ticket = TicketKey::Key(KeyText::new("OPS-7").unwrap());
    assert_eq!(acted.key().as_str(), "OPS-7@Approve@2026-11-22T11:10:00Z");
}

#[test]
fn every_sample_reads_back_as_it_was_written_full_and_bare() {
    let all = samples();
    let names: Vec<Name> = all.iter().map(|s| s.name).collect();
    assert_eq!(names, Name::ALL);
    for s in all {
        for e in [&s.full, &s.bare] {
            let node = to_node(e).unwrap();
            let back: Envelope =
                from_node(node.clone()).unwrap_or_else(|f| panic!("{node:?}: {f}"));
            assert_eq!(&back, e);
            assert_eq!(back.name(), s.name);
            assert_eq!(field(&node, "event"), Some(&text(s.name.as_str())));
        }
        let (full, bare) = (
            keys(&to_node(&s.full).unwrap()),
            keys(&to_node(&s.bare).unwrap()),
        );
        // A limit change has exactly one of `set` and `observed`: the full
        // sample `set`, the bare one `observed`.
        let one = |k: &Node| s.name == Name::FactoryLimitChanged && *k == text("observed");
        assert!(
            bare.iter().all(|k| full.contains(k) || one(k)),
            "{:?}",
            s.name
        );
    }
}

#[test]
fn the_log_alone_takes_the_top_priority_and_runs_before_a_stop() {
    let calls: Calls = Arc::default();
    let mut b = Builder::new();
    b.on::<CycleEnded>("stop", i16::MAX - 1, note(&calls, "stop", Heard::Stop));
    let log = calls.clone();
    b.log("log", move |_| log.lock().unwrap().push("log"));
    let raised = b.build().raise(ended());
    assert_eq!(raised.stopped_by, Some("stop"));
    assert_eq!(*calls.lock().unwrap(), ["log", "stop"]);
}

#[test]
#[should_panic(expected = "the log's alone")]
fn a_typed_listener_at_the_log_priority_is_refused_at_wiring() {
    let calls: Calls = Arc::default();
    Builder::new().on::<CycleEnded>("early", LOG, note(&calls, "early", Heard::Go));
}

#[test]
#[should_panic(expected = "the log's alone")]
fn a_listener_of_every_name_at_the_log_priority_is_refused_at_wiring() {
    Builder::new().on_any("early", LOG, |_| Heard::Go);
}

#[test]
fn a_limit_change_has_exactly_one_of_set_and_observed() {
    let full = sample(Name::FactoryLimitChanged).full;
    let l: LimitChanged = payload(&sample(Name::FactoryLimitChanged));
    assert_eq!((l.set(), l.observed_at()), (Some(full.at), None));
    assert_eq!(l.key().as_str(), "2026-11-22T14:40:00Z");
    let o = LimitChanged::observed(At::new(1_795_366_200), LimitState::Unreadable);
    assert_eq!(
        (o.set(), o.observed_at(), o.at()),
        (None, Some(o.at()), At::new(1_795_366_200))
    );
    let data = |times: Vec<(&'static str, Node)>| {
        let mut m = times;
        m.push(("state", text("ok")));
        map(m)
    };
    let t = || text("2026-11-22T14:40:00Z");
    assert!(from_node::<LimitChanged>(data(vec![])).is_err());
    assert!(from_node::<LimitChanged>(data(vec![("set", t()), ("observed", t())])).is_err());
    assert!(from_node::<LimitChanged>(data(vec![("observed", t())])).is_ok());
}

#[test]
fn an_unknown_event_is_never_written() {
    let mut e = sample(Name::CycleEnded).full;
    e.event = Event::Unknown;
    assert!(to_node(&e).is_err());
}

#[test]
fn an_envelope_of_another_api_reads_its_event_as_unknown() {
    let full = sample(Name::CycleEnded).full;
    let Node::Map(mut m) = to_node(&full).unwrap() else {
        panic!("an envelope is a map")
    };
    for (k, v) in m.iter_mut() {
        if *k == text("api") {
            *v = Node::Uint(2);
        }
    }
    let e: Envelope = from_node(Node::Map(m)).unwrap();
    assert_eq!((e.api, e.event, e.key), (2, Event::Unknown, full.key));
}

fn field<'a>(n: &'a Node, key: &str) -> Option<&'a Node> {
    match n {
        Node::Map(m) => m.iter().find(|(k, _)| *k == text(key)).map(|(_, v)| v),
        _ => None,
    }
}

fn keys(n: &Node) -> Vec<Node> {
    match field(n, "data") {
        Some(Node::Map(m)) => m.iter().map(|(k, _)| k.clone()).collect(),
        _ => Vec::new(),
    }
}

#[test]
fn a_name_this_copy_does_not_know_is_read_as_unknown_and_reaches_no_typed_listener() {
    let node = map(vec![
        ("api", Node::Uint(1)),
        ("event", text("ticket.reopened")),
        ("when", text("after")),
        ("key", text("12@2026-11-22T13:20:00Z")),
        ("at", text("2026-11-22T13:20:00Z")),
        (
            "data",
            map(vec![
                ("ticket", text("12")),
                ("why", map(vec![("x", Node::Bool(true))])),
            ]),
        ),
    ]);
    let e: Envelope = from_node(node).unwrap();
    assert_eq!(e.event, Event::Unknown);
    assert_eq!(e.name(), Name::Unknown);
    assert_eq!(e.key.as_str(), "12@2026-11-22T13:20:00Z");
    assert!(CycleEnded::peek(&e.event).is_none());
    assert!(TicketClosed::peek(&e.event).is_none());
    let n: Name = from_node(text("ticket.reopened")).unwrap();
    assert_eq!(n, Name::Unknown);
    assert!(!Name::ALL.contains(&n));
}

#[test]
fn a_source_is_kept_through_a_read_and_a_write_and_changes_nothing_else() {
    let plain = sample(Name::CycleStarted).full;
    let relayed = plain.clone().from(Source::new("tehas-tehas").unwrap());
    assert_eq!(
        Envelope {
            source: None,
            ..relayed.clone()
        },
        plain
    );
    let node = to_node(&relayed).unwrap();
    assert_eq!(field(&node, "source"), Some(&text("tehas-tehas")));
    assert_eq!(field(&to_node(&plain).unwrap(), "source"), None);
    let back: Envelope = from_node(node.clone()).unwrap();
    assert_eq!(back, relayed);
    assert_eq!(to_node(&back).unwrap(), node);
    for bad in ["", "Tehas", "a b", "a_b", &"a".repeat(65)] {
        assert!(Source::new(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn each_name_is_written_once_and_its_serde_name_is_its_text() {
    for n in Name::ALL {
        assert_eq!(to_node(&n).unwrap(), text(n.as_str()));
        assert_eq!(from_node::<Name>(text(n.as_str())).unwrap(), n);
        assert!(!n.gate(), "no gate is raised in 2.0");
        let parts: Vec<&str> = n.as_str().split('.').collect();
        assert_eq!(parts.len(), 2);
        assert!(n
            .as_str()
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b == b'.' || b == b'-'));
    }
}

#[test]
fn the_words_read_and_write_as_the_factory_api_prints_them() {
    let words: Vec<Node> = End::ALL.iter().map(|e| to_node(e).unwrap()).collect();
    let want = [
        "offered", "merged", "handoff", "parked", "failed", "refused", "stopped",
    ];
    assert_eq!(words, want.map(text));
    for e in End::ALL {
        assert_eq!(e.stop().map(End::from), e.stop().map(|_| e));
    }
    assert_eq!(to_node(&How::OutageRetry).unwrap(), text("outage-retry"));
    assert_eq!(from_node::<How>(text("label")).unwrap(), How::Unknown);
    assert_eq!(to_node(&When::Before).unwrap(), text("before"));
    assert_eq!(
        to_node(&LimitState::Unreadable).unwrap(),
        text("unreadable")
    );
}

#[test]
fn a_cycle_id_is_blueprint_dash_attempt_and_nothing_else() {
    let id = CycleId::read("12-3").unwrap();
    assert_eq!((id.blueprint.get(), id.attempt.get()), (12, 3));
    assert_eq!(id.to_string(), "12-3");
    assert_eq!(to_node(&id).unwrap(), text("12-3"));
    for bad in [
        "",
        "12",
        "12-",
        "-3",
        "0-1",
        "1-0",
        "012-3",
        "12-03",
        "1-2-3",
        "1234567890-1",
        "a-1",
    ] {
        assert_eq!(CycleId::read(bad), None, "{bad}");
        assert!(from_node::<CycleId>(text(bad)).is_err(), "{bad}");
    }
    let big = BlueprintNo::new(1_000_000_000).unwrap();
    assert_eq!(CycleId::new(big, NonZeroU32::MIN), None);
}

#[test]
fn a_key_has_no_white_space_and_at_most_200_characters() {
    assert!(Key::new("12-3").is_ok());
    assert!(Key::new("x".repeat(200)).is_ok());
    for bad in [
        "".to_string(),
        "a b".to_string(),
        "a\tb".to_string(),
        "x".repeat(201),
    ] {
        assert!(Key::new(bad.clone()).is_err(), "{bad:?}");
    }
}

#[test]
fn a_time_of_another_shape_does_not_read() {
    let mut node = to_node(&sample(Name::ChangeMerged).full).unwrap();
    if let Node::Map(m) = &mut node {
        for (k, v) in m.iter_mut() {
            if *k == text("at") {
                *v = text("2026-11-22T15:10:00.5Z");
            }
        }
    }
    assert!(from_node::<Envelope>(node).is_err());
    assert_eq!(
        rfc3339::read("2026-11-22T15:10:00Z"),
        Some(At::new(1_795_360_200))
    );
}

#[test]
fn a_port_type_in_a_payload_keeps_the_ports_own_shape() {
    let acted = sample(Name::TicketActed).full;
    let data = field(&to_node(&acted).unwrap(), "data").cloned().unwrap();
    let p: TicketActed = payload(&sample(Name::TicketActed));
    assert_eq!(field(&data, "act"), Some(&to_node(&p.act).unwrap()));
    assert_eq!(field(&data, "by"), Some(&to_node(&p.by).unwrap()));
    assert_eq!(field(&data, "ticket"), Some(&to_node(&p.ticket).unwrap()));
    let asked = to_node(&sample(Name::QuestionAsked).full).unwrap();
    let kind = field(field(&asked, "data").unwrap(), "kind").cloned();
    assert_eq!(kind, Some(to_node(&QuestionKind::Park).unwrap()));
    // The act in a key is its name as the ports write it.
    for act in [
        Act::Approve,
        Act::Plan,
        Act::Refine,
        Act::Finish,
        Act::Block,
        Act::Pause,
        Act::Unknown,
    ] {
        let mut p = p.clone();
        p.act = act;
        let word = p.key().as_str().split('@').nth(1).unwrap().to_string();
        assert_eq!(to_node(&act).unwrap(), text(&word));
    }
}

#[test]
fn a_question_with_no_options_kept_has_no_choices() {
    let bare = sample(Name::QuestionAsked).bare;
    let node = to_node(&bare).unwrap();
    assert!(!keys(&node).contains(&text("choices")), "{node:?}");
    let back: Envelope = from_node(node).unwrap();
    let q = QuestionAsked::peek(&back.event).unwrap();
    assert!(q.choices.is_empty());
    let full: QuestionAsked = payload(&sample(Name::QuestionAsked));
    assert!(!full.choices.is_empty());
}

#[test]
fn the_start_ask_is_keyed_on_its_blueprint_and_approval_and_waits_before() {
    let gates = gate_samples();
    assert_eq!(
        gates.iter().map(|g| g.name).collect::<Vec<_>>(),
        [Name::CycleStarted]
    );
    for g in &gates {
        for a in [&g.full, &g.bare] {
            assert_eq!((a.api, a.when, a.event), (API, When::Before, g.name));
            assert_eq!(a.key, a.data.key());
            assert_eq!(a.at, a.data.approval);
            let node = to_node(a).unwrap();
            assert_eq!(field(&node, "when"), Some(&text("before")));
            let back: Ask<StartAsk> = from_node(node).unwrap();
            assert_eq!(&back, a);
        }
    }
    let full = &gates[0].full;
    assert_eq!(full.key.as_str(), "12@2026-11-22T11:10:00Z");
    assert!(full.data.ticket.is_some() && full.data.by.is_some());
    assert!(gates[0].bare.data.ticket.is_none() && gates[0].bare.data.by.is_none());
    // An ask is only ever `before`, of its own name.
    let Node::Map(m) = to_node(full).unwrap() else {
        panic!("an ask is a map")
    };
    let with = |k: &str, v: Node| {
        let m: Vec<(Node, Node)> = m
            .iter()
            .map(|(a, b)| (a.clone(), if *a == text(k) { v.clone() } else { b.clone() }))
            .collect();
        Node::Map(m)
    };
    assert!(from_node::<Ask<StartAsk>>(with("when", text("after"))).is_err());
    // Another api is refused, as this copy knows only its own asks.
    assert!(from_node::<Ask<StartAsk>>(with("api", Node::Uint(2))).is_err());
    assert!(from_node::<Ask<StartAsk>>(with("event", text(Name::CycleEnded.as_str()))).is_err());
}

#[test]
fn a_gate_answer_allows_or_refuses_with_a_short_why() {
    let allow = GateAnswer::allow();
    let node = to_node(&allow).unwrap();
    assert_eq!(field(&node, "answer"), Some(&text("allow")));
    assert_eq!(field(&node, "api"), Some(&Node::Uint(1)));
    assert_eq!(field(&node, "why"), None);
    let refuse = GateAnswer::refuse(Why::new("the disk is full").unwrap());
    let node = to_node(&refuse).unwrap();
    assert_eq!(field(&node, "why"), Some(&text("the disk is full")));
    assert_eq!(from_node::<GateAnswer>(node).unwrap(), refuse);
    assert!(Why::new("x".repeat(140)).is_ok());
    for bad in [
        "".to_string(),
        "x".repeat(141),
        "a\nb".to_string(),
        "a\u{7f}b".to_string(),
        "a\u{85}b".to_string(),
        "a\u{9f}b".to_string(),
    ] {
        assert!(Why::new(bad.clone()).is_err(), "{bad:?}");
    }
    // A refusal says why.
    let bare = map(vec![("api", Node::Uint(1)), ("answer", text("refuse"))]);
    assert!(from_node::<GateAnswer>(bare).is_err());
    let other = map(vec![("api", Node::Uint(1)), ("answer", text("maybe"))]);
    assert!(from_node::<GateAnswer>(other).is_err());
    let later = map(vec![("api", Node::Uint(2)), ("answer", text("allow"))]);
    assert!(from_node::<GateAnswer>(later).is_err());
    assert!(
        Why::new("a\u{a0}b").is_ok(),
        "a no-break space is no control"
    );
}

// The log and a listener of every name, each keeping what it heard.
fn hear_all(b: &mut Builder, heard: &Arc<Mutex<Vec<Envelope>>>) {
    let h = heard.clone();
    b.log("log", move |e| h.lock().unwrap().push(e.clone()));
    let h = heard.clone();
    b.on_any("any", 1, move |e| {
        h.lock().unwrap().push(e.clone());
        Heard::Go
    });
}

fn heard_all(heard: &Arc<Mutex<Vec<Envelope>>>) -> Dispatcher {
    let mut b = Builder::new();
    hear_all(&mut b, heard);
    b.build()
}

#[test]
fn raise_envelope_hands_every_listener_the_envelope_with_its_source() {
    let calls: Calls = Arc::default();
    let heard = Arc::new(Mutex::new(Vec::new()));
    let mut b = Builder::new();
    hear_all(&mut b, &heard);
    b.on::<CycleEnded>("typed", 0, note(&calls, "typed", Heard::Go));
    let relayed = Envelope::new(ended()).from(Source::new("tehas-tehas").unwrap());
    assert_eq!(b.build().raise_envelope(&relayed), Raised::default());
    assert_eq!(*heard.lock().unwrap(), [relayed.clone(), relayed]);
    assert_eq!(*calls.lock().unwrap(), ["typed"]);
}

#[test]
fn raise_is_raise_envelope_of_the_payload_s_envelope() {
    let mut b = Builder::new();
    b.on::<CycleEnded>("stops", 2, |_| Heard::Stop)
        .on_any("fails", 3, |_| Heard::Failed("no disk".into()))
        .on_any("never", 0, |_| panic!("called after a stop"));
    let d = b.build();
    assert_eq!(d.raise(ended()), d.raise_envelope(&Envelope::new(ended())));
    let raised = d.raise(ended());
    assert_eq!(raised.stopped_by, Some("stops"));
    assert_eq!(raised.faults.len(), 1);
    for s in samples() {
        let heard = Arc::new(Mutex::new(Vec::new()));
        let d = heard_all(&heard);
        let by_raise = match s.full.event.clone() {
            Event::CycleStarted(p) => d.raise(p),
            Event::CycleParked(p) => d.raise(p),
            Event::QuestionAsked(p) => d.raise(p),
            Event::CycleEnded(p) => d.raise(p),
            Event::ChangeOffered(p) => d.raise(p),
            Event::FactoryLimitChanged(p) => d.raise(p),
            Event::TicketActed(p) => d.raise(p),
            Event::TicketClosed(p) => d.raise(p),
            Event::ChangeMerged(p) => d.raise(p),
            _ => panic!("a sample of a known name"),
        };
        let by_envelope = d.raise_envelope(&s.full);
        assert_eq!(by_raise, by_envelope);
        let heard = heard.lock().unwrap();
        assert_eq!(heard[..2], heard[2..], "{:?}", s.name);
    }
}

#[test]
fn an_envelope_no_raise_makes_is_refused_and_reaches_no_listener() {
    let heard = Arc::new(Mutex::new(Vec::new()));
    let d = heard_all(&heard);
    let mut unknown = Envelope::new(ended());
    unknown.event = Event::Unknown;
    let before = Envelope {
        when: When::Before,
        ..Envelope::new(ended())
    };
    assert!(!Name::CycleEnded.gate());
    assert_eq!(d.raise_envelope(&unknown).refused, Some(Refused::Unknown));
    assert_eq!(d.raise_envelope(&before).refused, Some(Refused::Before));
    let relayed = before.from(Source::new("tehas-tehas").unwrap());
    assert_eq!(d.raise_envelope(&relayed).refused, Some(Refused::Before));
    assert!(heard.lock().unwrap().is_empty());
    // `raise` makes none of them.
    for s in samples() {
        assert_eq!(d.raise_envelope(&s.full).refused, None, "{:?}", s.name);
        assert_eq!(d.raise_envelope(&s.bare).refused, None, "{:?}", s.name);
    }
}

#[test]
fn the_line_is_event_and_the_envelope_as_compact_json() {
    let source = Source::new("tehas-tehas").unwrap();
    let (asked_name, acted_name) = (Name::QuestionAsked.as_str(), Name::TicketActed.as_str());
    let asked = sample(Name::QuestionAsked);
    assert_eq!(
        asked.full.clone().from(source.clone()).line(),
        format!(
            r#"event {{"api":1,"source":"tehas-tehas","event":"{asked_name}","data":{{"question":5,"kind":"Park","blueprint":12,"cycle":"12-3","askedAt":"2026-11-22T13:20:00Z","choices":[{{"choice":"retry","label":"Run the cycle again"}}]}},"when":"after","key":"5","at":"2026-11-22T13:20:00Z"}}"#
        )
    );
    assert_eq!(
        asked.bare.line(),
        format!(
            r#"event {{"api":1,"event":"{asked_name}","data":{{"question":5,"kind":"Park","blueprint":12,"askedAt":"2026-11-22T13:20:00Z"}},"when":"after","key":"5","at":"2026-11-22T13:20:00Z"}}"#
        )
    );
    assert_eq!(
        sample(Name::TicketActed).full.from(source).line(),
        format!(
            r#"event {{"api":1,"source":"tehas-tehas","event":"{acted_name}","data":{{"ticket":{{"Number":12}},"act":"Approve","by":{{"realm":"Tickets","login":"alice","id":"1001","kind":"Person"}},"at":"2026-11-22T11:10:00Z"}},"when":"after","key":"12@Approve@2026-11-22T11:10:00Z","at":"2026-11-22T11:10:00Z"}}"#
        )
    );
    let mut q: QuestionAsked = payload(&asked);
    q.choices[0].label = "a \"b\" \\ \n\r\t\u{8}\u{c}\u{1}\u{1f}\u{7f} é/".into();
    let line = Envelope::new(q).line();
    let want = r#""label":"a \"b\" \\ \n\r\t\b\f\u0001\u001f"#.to_string() + "\u{7f} é/\"";
    assert!(line.contains(&want), "{line}");
    let mut unknown = asked.full;
    unknown.event = Event::Unknown;
    assert_eq!(unknown.line(), "event unknown 5");
}
