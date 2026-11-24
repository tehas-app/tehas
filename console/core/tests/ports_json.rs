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
        Marker::Park({
            let mut park_marker = ParkMarker::new(bp(7), sha('a'));
            park_marker.step = MarkerWord::new("").unwrap();
            park_marker.kind = MarkerWord::new("timeout").unwrap();
            park_marker.provider = true;
            park_marker
        }),
        Marker::Finish({
            let mut finish_marker = FinishMarker::new(bp(7), FinishResult::Stopped);
            finish_marker.by_poll = true;
            finish_marker
        }),
        Marker::LastAttempt({
            let mut last_attempt_marker = LastAttemptMarker::new(sha('c'));
            last_attempt_marker.note = "ran out of time".into();
            last_attempt_marker
        }),
        Marker::Millwright(MillwrightMarker::new(bp(7), MillwrightAction::Ask)),
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
    for s in [Scope::Agent, Scope::CycleMerge, Scope::Unknown] {
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
    round!({
        let mut account = Account::new(Realm::Tickets, Login::new("someone"), AccountKind::Person);
        account.id = Some(AccountId::new("42"));
        account
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
    round!({
        let mut applied = Applied::default();
        applied.done = vec![bp(1)];
        applied.stopped = Some((bp(2), Outcome::Refused(Refusal::Unknown)));
        applied
    });
    round!(MarkName::read_list(["area: docs", "a#b"]));
    round!(HappenedKind::BlockerAdded);
    round!(HappenedKind::Unknown);
    let link = WebLink::new("https://example.test/c").unwrap();
    round!((
        {
            let mut applied = Applied::default();
            applied.done = vec![StepPart::Note];
            applied.stopped = Some((StepPart::Close, Outcome::Forbidden));
            applied
        },
        Some(Posted::new(CommentId::new("c1"), link.clone())),
    ));
    round!({
        let mut change_delta = ChangeDelta::new(Cursor::new("next"));
        change_delta.comments = vec![(ChangeNo::new(4).unwrap(), {
            let mut comment =
                Comment::new(CommentId::new("c2"), At::new(1), Revision::new("r"), link);
            comment.body = "seen".into();
            comment
        })];
        change_delta
    });
    round!({
        let mut ask = BoardAsk::new(Depth::Cut { notes: 0 });
        ask.history = Some(Window::Whole(Bound::new(500).unwrap()));
        ask.changes = Some((ChangeStates::Open, Window::Last(nz32(5))));
        ask.control = Some(TicketKey::Number(nz(1)));
        ask
    });
    round!({
        let mut open_work = OpenWork::default();
        open_work.items = Whole::new(vec![]);
        open_work.control = Some(Err(Outcome::Unread(Unread::Cut)));
        open_work
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
fn a_delta_stored_without_more_reads_as_not_more() {
    let ticket = read!(TicketDelta, r#"{"tickets":[],"notes":[],"next":"c1"}"#);
    assert_eq!(ticket, TicketDelta::new(Cursor::new("c1")));
    assert!(!ticket.more);
    let blueprint = read!(
        BlueprintDelta,
        r#"{"blueprints":[],"comments":[],"next":"c2"}"#
    );
    assert_eq!(blueprint, BlueprintDelta::new(Cursor::new("c2")));
    assert!(!blueprint.more);
    let change = read!(ChangeDelta, r#"{"changes":[],"comments":[],"next":"c3"}"#);
    assert_eq!(change, ChangeDelta::new(Cursor::new("c3")));
    assert!(!change.more);
}

#[test]
fn a_filter_stored_without_carrying_any_reads_as_narrowing_nothing() {
    let filter = read!(
        Filter,
        r#"{"open":true,"carrying":[],"kind":null,"target":null}"#
    );
    assert!(filter.carrying_any.is_empty());
    let any = Filter {
        carrying_any: vec![Mark::State(State::Running), Mark::Epic],
        ..Filter::default()
    };
    round!(any);
}

#[test]
fn a_change_digest_carries_its_base_commit_update_and_what_its_text_closes() {
    let commit = |c: char| CommitId::new(c.to_string().repeat(40)).unwrap();
    let mut d = ChangeDigest::new(
        ChangeNo::new(5).unwrap(),
        ChangeState::Open,
        Branch::new("work").unwrap(),
        commit('a'),
        Branch::new("main").unwrap(),
        commit('b'),
        At::new(10),
        At::new(20),
        WebLink::new("https://example.test/c/5").unwrap(),
    );
    d.says_closes = vec![Ref::Here(bp(7))];
    let json = serde_json::to_value(&d).unwrap();
    assert_eq!(json["base_commit"], "b".repeat(40));
    assert_eq!(json["updated"], 20);
    assert_eq!(json["says_closes"], serde_json::json!([{"Here": 7}]));
    round!(d);
}

#[test]
fn a_ticket_event_stored_without_a_mark_id_reads_as_none() {
    let e = read!(
        TicketEvent,
        r#"{"what":{"type":"Reopened"},"by":null,"at":null}"#
    );
    assert_eq!(e, TicketEvent::new(Happened::Reopened));
    let mut marked = TicketEvent::new(Happened::Marked(Mark::Epic));
    marked.mark_id = Some(MarkId::new("LA_kwDO"));
    assert_eq!(serde_json::to_value(&marked).unwrap()["mark_id"], "LA_kwDO");
    round!(marked);
}

#[test]
fn a_ticket_digest_stored_without_a_parent_reads_as_none() {
    let mut d = TicketDigest::new(
        TicketKey::Number(nz(3)),
        TicketState::Open,
        At::new(1),
        At::new(2),
        Revision::new("r"),
        WebLink::new("https://example.test/t/3").unwrap(),
    );
    let mut json = serde_json::to_value(&d).unwrap();
    json.as_object_mut().unwrap().remove("parent");
    let back: TicketDigest = serde_json::from_value(json).unwrap();
    assert_eq!(back, d);
    d.parent = Some(here(1));
    round!(d);
}

#[test]
fn a_target_stored_without_open_or_due_reads_as_none() {
    let t = read!(TargetName, r#"{"id":"3","title":"v2"}"#);
    assert_eq!(t, TargetName::new(TargetId::new("3"), "v2".into()));
    assert_eq!((t.open, t.due), (None, None));
    let mut full = t;
    full.open = Some(false);
    full.due = Some(At::new(5));
    round!(full);
}

#[test]
fn a_closer_of_another_project_reads_back_and_one_not_known_is_unknown() {
    round!(Closer::change_elsewhere(
        ProjectName::new("other/project"),
        ItemName::new("other/project 12"),
        WebLink::new("https://example.test/other/project/12").unwrap(),
    ));
    round!(Closer::Commit(CommitId::new("abc1234").unwrap()));
    // The older shapes read as they did.
    let change = read!(Closer, r#"{"Change":{"no":4,"merged":true}}"#);
    assert_eq!(
        change,
        Closer::Change {
            no: ChangeNo::new(4).unwrap(),
            merged: true
        }
    );
    for json in [
        r#"{"Later":{"x":1}}"#,
        r#"{"Commit":5}"#,
        r#""Later""#,
        r#""Unknown""#,
    ] {
        assert_eq!(read!(Closer, json), Closer::Unknown, "{json}");
    }
    assert_eq!(
        serde_json::to_string(&Closer::Unknown).unwrap(),
        r#""Unknown""#
    );
    for bad in ["3", "[]", "{}", r#"{"a":1,"b":2}"#] {
        assert!(serde_json::from_str::<Closer>(bad).is_err(), "{bad}");
    }
}

#[test]
fn a_relation_is_its_name_and_an_author_without_one_reads_as_none() {
    for r in [
        Relation::Owner,
        Relation::Member,
        Relation::Collaborator,
        Relation::Outside,
    ] {
        round!(r);
    }
    assert_eq!(
        serde_json::to_string(&Relation::Collaborator).unwrap(),
        r#""Collaborator""#
    );
    assert_eq!(read!(Relation, r#""Contributor""#), Relation::Unknown);
    let link = WebLink::new("https://example.test/b/7").unwrap();
    let mut b = Blueprint::new(bp(7), At::new(1), Revision::new("r"), link);
    let mut json = serde_json::to_value(&b).unwrap();
    json.as_object_mut().unwrap().remove("author_relation");
    assert_eq!(serde_json::from_value::<Blueprint>(json).unwrap(), b);
    b.author_relation = Some(Relation::Member);
    round!(b);
}

#[test]
fn a_release_without_notes_and_a_merge_way_not_known_read() {
    let mut r = Release::new(
        ReleaseId::new("r1"),
        TagName::new("v2.0.0").unwrap(),
        WebLink::new("https://example.test/r/v2.0.0").unwrap(),
    );
    let mut json = serde_json::to_value(&r).unwrap();
    json.as_object_mut().unwrap().remove("notes");
    assert_eq!(serde_json::from_value::<Release>(json).unwrap(), r);
    r.notes = "What changed.".into();
    round!(r);
    round!(MergeHow::Site);
    assert_eq!(serde_json::to_string(&MergeHow::Site).unwrap(), r#""Site""#);
    assert_eq!(read!(MergeHow, r#""FastForward""#), MergeHow::Unknown);
    let delta = read!(ChangeDelta, r#"{"changes":[],"next":"c3"}"#);
    assert!(delta.comments.is_empty());
}

#[test]
fn a_board_ask_stored_without_windows_per_kind_reads_as_none() {
    let ask = read!(
        BoardAsk,
        r#"{"depth":"All","history":null,"changes":null,"filter":{"open":null,"carrying":[],"kind":null,"target":null},"control":null}"#
    );
    assert!(ask.history_by_kind.is_empty());
    round!({
        let mut ask = ask;
        ask.history_by_kind = vec![
            (
                HappenedKind::Marked,
                Window::Whole(Bound::new(500).unwrap()),
            ),
            (HappenedKind::Retitled, Window::Last(nz32(5))),
        ];
        ask
    });
}

#[test]
fn a_change_filter_stored_without_carrying_and_an_unknown_overall_read() {
    let f = read!(
        ChangeFilter,
        r#"{"states":"Open","author":null,"head":null,"closing":[],"merged_since":null,"updated_since":null,"numbers":[],"bodies":false}"#
    );
    assert_eq!(f, ChangeFilter::default());
    let carrying = ChangeFilter {
        carrying: vec![Mark::State(State::Refreshing)],
        ..ChangeFilter::default()
    };
    round!(carrying);
    assert_eq!(read!(Overall, r#""Unknown""#), Overall::Unknown);
    round!(Overall::Unknown);
}

#[test]
fn an_old_spec_marker_reads_as_unknown() {
    // The Spec marker left with the Spec marks (#2892): a record that still
    // holds one reads under the one rule, as a tag this copy does not know.
    let old = r#"{"type":"Spec","value":{"version":1,"link":"https://example.test/spec.md","sha256":"bbbb","requirements":[{"id":"R1","met":false,"note":"not yet"}]}}"#;
    assert_eq!(read!(Marker, old), Marker::Unknown);
    assert_eq!(read!(MarkerKind, r#"{"type":"Spec"}"#), MarkerKind::Unknown);
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
        serde_json::to_string(&Scope::CycleMerge).unwrap(),
        r#""CycleMerge""#
    );
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
    // The tag is a map's one string `type`: a sequence, a tag of another
    // type and a tag given twice are no record of the enum.
    for json in [
        r#"["Act"]"#,
        r#"{"type":3}"#,
        r#"{"type":"Future","type":"Other"}"#,
    ] {
        assert!(serde_json::from_str::<Mark>(json).is_err(), "{json}");
    }
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
    round!(Budget::new(0, At::new(3600)));
    round!(HealthState::Failing {
        since: At::new(5),
        why: Passing::Broken,
    });
    round!(HealthState::Spent {
        pool: Pool::Unknown,
        until: At::new(9),
    });
    round!(HealthState::SignedOut { since: At::new(7) });
    round!(Failure::new(
        At::new(1),
        SiteId::new("project"),
        Outcome::Spent {
            pool: Pool::Writes,
            until: At::new(60),
        }
    ));
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
        round!(Notice::new(
            HostId::new("code-host"),
            Whose::Holder(ours.clone()),
            Realm::CodeHost,
            what,
            At::new(2)
        ));
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
    let factory = {
        let mut account = Account::new(Realm::CodeHost, Login::new("factory"), AccountKind::Ours);
        account.id = Some(AccountId::new("id-factory"));
        account
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

#[test]
fn a_standing_is_its_name_and_the_old_words_still_read() {
    for (s, word) in [
        (Standing::None, "None"),
        (Standing::Read, "Read"),
        (Standing::Triage, "Triage"),
        (Standing::Write, "Write"),
        (Standing::Maintain, "Maintain"),
        (Standing::Admin, "Admin"),
    ] {
        let json = format!("\"{word}\"");
        assert_eq!(serde_json::to_string(&s).unwrap(), json);
        assert_eq!(serde_json::from_str::<Standing>(&json).unwrap(), s);
    }
}
