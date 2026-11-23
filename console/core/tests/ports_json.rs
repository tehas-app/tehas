//! The ports' values cross the store, the journal and the factory API as
//! JSON. Each data type of tehas-ports reads back from its JSON text as it
//! was written, and an enum that may grow reads a tag it does not know as
//! its `Unknown`, in JSON as in the ports' own tests.

use std::num::{NonZeroU32, NonZeroU64};

use tehas_ports::*;

/// The value, written to JSON text and read back, is itself. A macro, so
/// the test names no serde trait: this crate takes serde through
/// serde_json alone.
macro_rules! round {
    ($v:expr $(,)?) => {{
        let v = $v;
        let json = serde_json::to_string(&v).expect("writes");
        let back = same_type(
            &v,
            serde_json::from_str(&json).unwrap_or_else(|e| panic!("{v:?} as {json}: {e}")),
        );
        assert_eq!(back, v, "{json}");
    }};
}

fn same_type<T>(_: &T, t: T) -> T {
    t
}

/// The JSON text, read as the type.
macro_rules! read {
    ($t:ty, $json:expr) => {{
        let json = $json;
        serde_json::from_str::<$t>(json).unwrap_or_else(|e| panic!("{json}: {e}"))
    }};
}

fn nz(n: u64) -> NonZeroU64 {
    NonZeroU64::new(n).unwrap()
}

fn nz32(n: u32) -> NonZeroU32 {
    NonZeroU32::new(n).unwrap()
}

fn bp(n: u64) -> BlueprintNo {
    BlueprintNo::new(n).unwrap()
}

fn sha(c: char) -> Sha256 {
    Sha256::new(c.to_string().repeat(64)).unwrap()
}

fn name(s: &str) -> MarkName {
    MarkName::new(s).unwrap()
}

fn here(n: u64) -> Ref<TicketKey> {
    Ref::Here(TicketKey::Number(nz(n)))
}

fn elsewhere<T>() -> Ref<T> {
    Ref::Elsewhere {
        project: ProjectName::new("other/project"),
        item: ItemName::new("other/project 12"),
    }
}

fn every_marker() -> Vec<Marker> {
    vec![
        Marker::Question {
            kind: QuestionKind::Plan,
            blueprint: bp(7),
            turn: nz32(1),
        },
        Marker::Question {
            kind: QuestionKind::Unknown,
            blueprint: bp(7),
            turn: nz32(2),
        },
        Marker::Blocked {
            by: vec![here(3), elsewhere()],
        },
        Marker::HoldCleared,
        Marker::Park(ParkMarker {
            blueprint: bp(7),
            step: MarkerWord::new("").unwrap(),
            kind: MarkerWord::new("timeout").unwrap(),
            provider: true,
            sig: sha('a'),
        }),
        Marker::Finish(FinishMarker {
            blueprint: bp(7),
            result: FinishResult::Stopped,
            by_poll: true,
        }),
        Marker::Spec(SpecMarker {
            version: nz32(SPEC_VERSION),
            link: "https://example.test/spec.md".into(),
            sha256: sha('b'),
            requirements: vec![Requirement {
                id: "R1".into(),
                met: false,
                note: "not yet".into(),
            }],
        }),
        Marker::LastAttempt(LastAttemptMarker {
            base: sha('c'),
            note: "ran out of time".into(),
        }),
        Marker::Millwright(MillwrightMarker {
            blueprint: bp(7),
            action: MillwrightAction::Ask,
        }),
        Marker::TicketOf(Ref::Here(TicketKey::Key(KeyText::new("ABC-1").unwrap()))),
        Marker::Link {
            blueprint: bp(9),
            remove: true,
        },
        Marker::Project {
            name: name("hold"),
            key: Some(MarkerKey::new("k.1").unwrap()),
            fields: vec![("why".into(), "waiting".into())],
        },
        Marker::Unreadable {
            kind: MarkerKind::Millwright,
        },
        Marker::Bare {
            kind: MarkerKind::Blocked,
            read: Some(Box::new(Marker::Blocked { by: vec![here(4)] })),
        },
        Marker::Unknown,
    ]
}

#[test]
fn the_ports_values_read_back_from_json() {
    for m in [
        Mark::Act(Act::Block),
        Mark::Act(Act::Unknown),
        Mark::State(State::Proposal),
        Mark::State(State::Unknown),
        Mark::Choice(Choice::Lean),
        Mark::Choice(Choice::Unknown),
        Mark::Epic,
        Mark::Project(name("area/ui")),
        Mark::Unknown,
    ] {
        round!(m);
    }
    for k in [Kind::Bug, Kind::Other(name("kind/bug")), Kind::Unknown] {
        round!(k);
    }
    for k in [
        MarkerKind::Question,
        MarkerKind::Blocked,
        MarkerKind::HoldCleared,
        MarkerKind::Park,
        MarkerKind::Finish,
        MarkerKind::Spec,
        MarkerKind::LastAttempt,
        MarkerKind::Millwright,
        MarkerKind::TicketOf,
        MarkerKind::Link,
        MarkerKind::Project(name("hold")),
        MarkerKind::Unknown,
    ] {
        round!(k);
    }
    for m in every_marker() {
        round!(m);
    }
    round!(MarkerEdit {
        set: vec![Marker::HoldCleared],
        drop: vec![(MarkerKind::Blocked, None)],
    });
    for s in [Scope::Agent, Scope::Unknown] {
        round!(s);
        round!(Actor::Ours(s));
    }
    round!(QuestionKind::Unknown);
    assert_eq!(read!(HappenedKind, r#""Later""#), HappenedKind::Unknown);
    round!(Refusal::Unknown);
    round!(Unread::Unknown);
    round!(Pool::Unknown);
    round!(Priority::Urgent);
    round!(MarkSpec {
        name: name("area: docs"),
        colour: Some(Colour::new("0e8a16").unwrap()),
        about: None,
    });
    let sum = Sum {
        sha256: sha('d'),
        len: 12,
    };
    round!(sum.clone());
    round!(Body::Text("hello".into()));
    round!(Body::Summed(sum));
    round!(Cite::Blueprint(elsewhere()));
    round!(Cite::Account(Login::new("someone")));
    round!(here(1));
    round!(elsewhere::<ChangeNo>());
    round!(TicketKey::Key(KeyText::new("ABC-12").unwrap()));
    round!(Item::Change(ChangeNo::new(2).unwrap()));
    round!(Rev::Branch(Branch::new("fix/typo").unwrap()));
    round!(Account {
        realm: Realm::Tickets,
        login: Login::new("someone"),
        id: Some(AccountId::new("42")),
        kind: AccountKind::Person,
    });
    round!(Covers::Site(SiteId::new("s")));
    round!(Covers::projects(
        GrantId::new("g"),
        vec![ProjectName::new("b"), ProjectName::new("a")],
    )
    .unwrap(),);
    round!(Kept::Grant);
    round!(Window::Last(nz32(100)));
    round!(Depth::Cut { notes: 5 });
    round!(Within::Titles);
    round!(Outcome::Unavailable {
        why: Passing::Timeout,
        sent: Sent::Maybe,
    });
    round!(Applied {
        done: vec![bp(1)],
        stopped: Some((bp(2), Outcome::Refused(Refusal::Unknown))),
    });
    round!(MarkName::read_list(["area: docs", "a#b"]));
    round!(HappenedKind::BlockerAdded);
    round!(HappenedKind::Unknown);
    let link = WebLink::new("https://example.test/c").unwrap();
    round!((
        Applied {
            done: vec![StepPart::Note],
            stopped: Some((StepPart::Close, Outcome::Forbidden)),
        },
        Some(Posted {
            id: CommentId::new("c1"),
            link: link.clone(),
        }),
    ));
    round!(ChangeDelta {
        changes: vec![],
        comments: vec![(
            ChangeNo::new(4).unwrap(),
            Comment {
                id: CommentId::new("c2"),
                author: None,
                editor: None,
                created: At::new(1),
                edited: None,
                body: "seen".into(),
                markers: vec![],
                revision: Revision::new("r"),
                link,
            },
        )],
        next: Cursor::new("next"),
    });
    round!(BoardAsk {
        depth: Depth::Cut { notes: 0 },
        history: Some(Window::Whole(Bound::new(500).unwrap())),
        changes: Some((ChangeStates::Open, Window::Last(nz32(5)))),
        filter: Filter::default(),
        control: Some(TicketKey::Number(nz(1))),
    });
    round!(OpenWork {
        items: Whole::new(vec![]),
        control: Some(Err(Outcome::Unread(Unread::Cut))),
    });
    round!(ChangeAsk {
        filter: ChangeFilter::default(),
        files: Some(Bound::new(300).unwrap()),
        closing: true,
        siblings: Some(Bound::new(20).unwrap()),
        activity: None,
    });
}

#[test]
fn an_unknown_tag_with_content_of_any_shape_reads_as_unknown_in_json() {
    for json in [
        r#"{"type":"Future"}"#,
        r#"{"type":"Future","value":3}"#,
        r#"{"type":"Future","value":"x"}"#,
        r#"{"type":"Future","value":[true,null]}"#,
        r#"{"type":"Future","value":{"field":{"deep":1}}}"#,
    ] {
        assert_eq!(read!(Mark, json), Mark::Unknown, "{json}");
        assert_eq!(read!(Kind, json), Kind::Unknown, "{json}");
        assert_eq!(read!(MarkerKind, json), MarkerKind::Unknown, "{json}");
        assert_eq!(read!(Marker, json), Marker::Unknown, "{json}");
    }
    assert_eq!(read!(Act, r#""Future""#), Act::Unknown);
    assert_eq!(read!(Scope, r#""Future""#), Scope::Unknown);
    assert_eq!(
        serde_json::to_string(&Marker::Unknown).unwrap(),
        r#"{"type":"Unknown"}"#
    );
}

#[test]
fn a_known_marker_tag_whose_content_does_not_read_is_unreadable_in_json() {
    let unreadable = |kind| Marker::Unreadable { kind };
    for (json, kind) in [
        (
            r#"{"type":"Millwright","value":{"blueprint":7,"action":"Later"}}"#,
            MarkerKind::Millwright,
        ),
        (
            r#"{"type":"Finish","value":{"blueprint":7,"result":"Later","by_poll":false}}"#,
            MarkerKind::Finish,
        ),
        (r#"{"type":"Park","value":3}"#, MarkerKind::Park),
        (r#"{"type":"Question","value":"x"}"#, MarkerKind::Question),
        (r#"{"type":"Project","value":[1]}"#, MarkerKind::Unknown),
    ] {
        assert_eq!(read!(Marker, json), unreadable(kind), "{json}");
    }
}

#[test]
fn a_known_tag_of_another_enum_whose_content_does_not_read_is_unknown_in_json() {
    // One rule for every enum that may grow: an older reader never fails a
    // record. `Marker` alone reads such a tag as `Unreadable` instead.
    for json in [
        r#"{"type":"Act","value":3}"#,
        r#"{"type":"Project","value":"no mark/name#"}"#,
    ] {
        assert_eq!(read!(Mark, json), Mark::Unknown, "{json}");
    }
    // A newer word inside a known tag reads as the inner enum's own Unknown.
    assert_eq!(
        read!(Mark, r#"{"type":"State","value":"Later"}"#),
        Mark::State(State::Unknown)
    );
    assert_eq!(read!(Kind, r#"{"type":"Other","value":7}"#), Kind::Unknown);
    assert_eq!(
        read!(MarkerKind, r#"{"type":"Project","value":[1]}"#),
        MarkerKind::Unknown
    );
    // A value with no tag is no record of the enum.
    assert!(serde_json::from_str::<Mark>("3").is_err());
    assert!(serde_json::from_str::<Mark>(r#"{"value":3}"#).is_err());
}

#[test]
fn a_journal_record_the_reader_cannot_read_is_unknown_in_json() {
    for json in [
        r#"{"type":"Future"}"#,
        r#"{"type":"Future","value":3}"#,
        r#"{"type":"Future","value":"x"}"#,
        r#"{"type":"Future","value":[true,null]}"#,
        r#"{"type":"Future","value":{"field":{"deep":1}}}"#,
    ] {
        assert_eq!(read!(Write, json), Write::Unknown, "{json}");
        assert_eq!(read!(Receipt, json), Receipt::Unknown, "{json}");
        assert_eq!(read!(Happened, json), Happened::Unknown, "{json}");
        assert_eq!(
            read!(ChangeHappened, json),
            ChangeHappened::Unknown,
            "{json}"
        );
        assert_eq!(read!(Takeover, json), Takeover::Unknown, "{json}");
    }
    // A known tag whose content does not read: an older copy reading a
    // newer row never fails it.
    for json in [
        r#"{"type":"Mark","value":3}"#,
        r#"{"type":"Mark","value":{"ticket":1,"mark":{"type":"Act","value":3}}}"#,
        r#"{"type":"Merge","value":{"no":7}}"#,
        r#"{"type":"Prepare","value":[true]}"#,
        r#"{"type":"Upload"}"#,
    ] {
        assert_eq!(read!(Write, json), Write::Unknown, "{json}");
    }
    for json in [
        r#"{"type":"Done","value":3}"#,
        r#"{"type":"Change","value":"x"}"#,
        r#"{"type":"Edited","value":"http://example.test"}"#,
        r#"{"type":"Release"}"#,
    ] {
        assert_eq!(read!(Receipt, json), Receipt::Unknown, "{json}");
    }
    assert_eq!(
        read!(Happened, r#"{"type":"Closed","value":[true]}"#),
        Happened::Unknown
    );
    // A mark inside that does not read is the mark's own Unknown, by the
    // same rule one level in.
    assert_eq!(
        read!(
            ChangeHappened,
            r#"{"type":"Marked","value":{"type":"Act","value":3}}"#
        ),
        ChangeHappened::Marked(Mark::Unknown)
    );
    assert_eq!(
        read!(ChangeHappened, r#"{"type":"Marked","value":3}"#),
        ChangeHappened::Unknown
    );
    assert_eq!(
        read!(Takeover, r#"{"type":"SameShape","value":7}"#),
        Takeover::Unknown
    );
}

#[test]
fn a_blocked_of_21_reads_as_unreadable_in_json() {
    let over = Marker::Blocked {
        by: (1..=21).map(here).collect(),
    };
    let json = serde_json::to_string(&over).unwrap();
    assert_eq!(
        read!(Marker, &json),
        Marker::Unreadable {
            kind: MarkerKind::Blocked
        }
    );
    let twenty = Marker::Blocked {
        by: (1..=20).map(here).collect(),
    };
    round!(twenty);
}

#[test]
fn an_empty_set_of_projects_does_not_read_in_json() {
    assert!(serde_json::from_str::<Covers>(r#"{"Projects":{"grant":"g","projects":[]}}"#).is_err());
    let read = read!(
        Covers,
        r#"{"Projects":{"grant":"g","projects":["b","a","b"]}}"#
    );
    assert_eq!(
        read,
        Covers::projects(
            GrantId::new("g"),
            vec![ProjectName::new("a"), ProjectName::new("b")]
        )
        .unwrap()
    );
}

#[test]
fn the_ledger_and_health_read_back_from_json() {
    let ours = Holder::Ours(HolderId::new("grant-1"));
    round!(HostId::new("code-host"));
    round!(Whose::Host);
    round!(Whose::Holder(ours.clone()));
    round!(Whose::Holder(Holder::Person(AccountId::new("42"))));
    round!(Whose::Pool(ours.clone(), Pool::Priced));
    round!(Whose::Pool(
        Holder::Anonymous(HolderId::new("addr-sum")),
        Pool::Unknown
    ));
    assert_eq!(
        serde_json::to_string(&Whose::Pool(ours.clone(), Pool::Writes)).unwrap(),
        r#"{"Pool":[{"Ours":"grant-1"},"Writes"]}"#
    );
    round!(Holder::Anonymous(HolderId::new("addr-sum")));
    round!(Budget {
        left: 0,
        resets: At::new(3600),
    });
    round!(HealthState::Failing {
        since: At::new(5),
        why: Passing::Broken,
    });
    round!(HealthState::Spent {
        pool: Pool::Unknown,
        until: At::new(9),
    });
    round!(HealthState::SignedOut { since: At::new(7) });
    round!(Failure {
        at: At::new(1),
        site: SiteId::new("project"),
        outcome: Outcome::Spent {
            pool: Pool::Writes,
            until: At::new(60),
        },
    });
    round!(LedgerDown);
    for what in [
        Health::Unavailable(Passing::Timeout),
        Health::Spent {
            pool: Pool::Priced,
            until: At::new(60),
        },
        Health::SignedOut,
        Health::Recovered,
        Health::LedgerDown,
        Health::TokenNotDropped,
        Health::Unknown(Word::Marker),
        Health::Unknown(Word::Unknown),
    ] {
        round!(what);
        round!(Notice {
            host: HostId::new("code-host"),
            whose: Whose::Holder(ours.clone()),
            realm: Realm::CodeHost,
            what,
            at: At::new(2),
        });
    }
    for word in [
        Word::HappenedKind,
        Word::Takeover,
        Word::Health,
        Word::PortName,
        Word::Unknown,
    ] {
        round!(word);
    }
    assert_eq!(
        serde_json::to_string(&Word::HappenedKind).unwrap(),
        r#""HappenedKind""#
    );
}

#[test]
fn a_health_or_word_the_reader_cannot_read_is_unknown_in_json() {
    for json in [
        r#"{"type":"Future"}"#,
        r#"{"type":"Future","value":"Mark"}"#,
        r#"{"type":"Future","value":{"field":[1]}}"#,
        r#"{"type":"Unavailable","value":"Later"}"#,
        r#"{"type":"Spent","value":3}"#,
        r#"{"type":"SignedOut","value":[true]}"#,
        r#"{"type":"Unknown"}"#,
        r#"{"type":"Unknown","value":3}"#,
    ] {
        assert_eq!(read!(Health, json), Health::Unknown(Word::Health), "{json}");
    }
    assert_eq!(
        read!(Health, r#"{"type":"Unknown","value":"Write"}"#),
        Health::Unknown(Word::Write)
    );
    assert_eq!(
        read!(Health, r#"{"type":"Unknown","value":"Later"}"#),
        Health::Unknown(Word::Unknown)
    );
    assert_eq!(
        serde_json::to_string(&Health::Unknown(Word::Write)).unwrap(),
        r#"{"type":"Unknown","value":"Write"}"#
    );
    assert_eq!(
        serde_json::to_string(&Health::Recovered).unwrap(),
        r#"{"type":"Recovered"}"#
    );
    assert!(serde_json::from_str::<Health>("3").is_err());
    assert!(serde_json::from_str::<Health>(r#"{"value":"Mark"}"#).is_err());
    assert_eq!(read!(Word, r#""Future""#), Word::Unknown);
    assert_eq!(
        serde_json::to_string(&Word::Unknown).unwrap(),
        r#""Unknown""#
    );
    // Not on the list of enums that may grow: an unknown tag is an error.
    assert!(serde_json::from_str::<HealthState>(r#"{"Future":1}"#).is_err());
    assert!(serde_json::from_str::<Holder>(r#"{"Future":"x"}"#).is_err());
    assert!(serde_json::from_str::<Whose>(r#"{"Future":"x"}"#).is_err());
}

#[test]
fn the_fakes_and_the_laws_values_read_back_from_json() {
    use tehas_ports::fake::{Asked, BoardOp, Fault, Op, TicketsOp, Wrote};
    let factory = Account {
        realm: Realm::CodeHost,
        login: Login::new("factory"),
        id: Some(AccountId::new("id-factory")),
        kind: AccountKind::Ours,
    };
    round!(Wrote {
        by: factory,
        what: Write::Reopen {
            ticket: TicketKey::Number(nz(7)),
        },
    });
    round!(Op::Board(BoardOp::OpenWork));
    round!(Op::Tickets(TicketsOp::Note));
    round!(Fault::TimeoutMaybe { applied: true });
    round!(Fault::Server { applied: false });
    round!(Fault::Broken);
    round!(Fault::Spent {
        pool: Pool::Priced,
        until: At::new(9),
    });
    round!(Asked::Forget(Covers::Site(SiteId::new("p")), None));
    round!(tehas_ports::laws::Broken {
        law: "lists::more_is_exact".into(),
        what: "more".into(),
    });
}
