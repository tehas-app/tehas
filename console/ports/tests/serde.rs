//! Every data type of the ports reads back as it was written, a checked
//! type refuses a bad text when read, and an enum that may grow reads a tag
//! it does not know as its `Unknown`.

mod tree;

use std::fmt::Debug;
use std::num::{NonZeroU32, NonZeroU64};

use serde::{Deserialize, Serialize};
use tehas_ports::*;
use tree::{from_node, map, text, to_node, Node};

fn round<T>(v: T)
where
    T: Serialize + for<'de> Deserialize<'de> + PartialEq + Debug,
{
    let node = to_node(&v).expect("writes");
    let back: T = from_node(node.clone()).unwrap_or_else(|e| panic!("{v:?} as {node:?}: {e}"));
    assert_eq!(back, v);
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

fn mark_name(s: &str) -> MarkName {
    MarkName::new(s).unwrap()
}

fn elsewhere<T>() -> Ref<T> {
    Ref::Elsewhere {
        project: ProjectName::new("other/project"),
        item: ItemName::new("other/project 12"),
    }
}

#[test]
fn ids_read_back() {
    round(TicketKey::Number(nz(12)));
    round(TicketKey::Key(KeyText::new("ABC-12").unwrap()));
    round(bp(7));
    round(ChangeNo::new(8).unwrap());
    round(Item::Ticket(TicketKey::Number(nz(1))));
    round(Item::Change(ChangeNo::new(2).unwrap()));
    round(Ref::Here(bp(3)));
    round(elsewhere::<ChangeNo>());
    round(ProjectName::new("p"));
    round(ItemName::new("i"));
    round(CommentId::new("c1"));
    round(TargetId::new("t1"));
    round(ReleaseId::new("r1"));
    round(AttachmentId::new("a1"));
    round(Revision::new("rev"));
    round(ChangeToken::new("tok"));
    round(Cursor::new("cur"));
    round(CommitId::new("abc1234").unwrap());
    round(Branch::new("fix/typo").unwrap());
    round(TagName::new("v2.0.0").unwrap());
    round(RepoPath::new("src/lib.rs").unwrap());
    round(mark_name("area: docs"));
    round(FileName::new("tehas.tar.gz").unwrap());
    round(Rev::Commit(CommitId::new("abc1234").unwrap()));
    round(Rev::Branch(Branch::new("main").unwrap()));
    round(Rev::Tag(TagName::new("v1").unwrap()));
    round(WebLink::new("https://example.test/run/1").unwrap());
    round(GitRemote::new("git@example.test:o/p.git").unwrap());
    round(At::new(1_700_000_000));
}

#[test]
fn a_number_is_written_as_itself() {
    assert_eq!(to_node(&bp(7)).unwrap(), Node::Uint(7));
    assert_eq!(to_node(&KeyText::new("A-1").unwrap()).unwrap(), text("A-1"));
    assert_eq!(to_node(&At::new(-1)).unwrap(), Node::Int(-1));
}

#[test]
fn a_checked_type_refuses_a_bad_text_when_read() {
    assert!(from_node::<KeyText>(text("a b")).is_err());
    assert!(from_node::<CommitId>(text("xyz")).is_err());
    assert!(from_node::<Branch>(text("a..b")).is_err());
    assert!(from_node::<TagName>(text("")).is_err());
    assert!(from_node::<RepoPath>(text("../x")).is_err());
    assert!(from_node::<MarkName>(text("a#b")).is_err());
    assert!(from_node::<FileName>(text("a/b")).is_err());
    assert!(from_node::<WebLink>(text("http://example.test")).is_err());
    assert!(from_node::<GitRemote>(text("file:///x")).is_err());
    assert!(from_node::<Colour>(text("abc")).is_err());
    assert!(from_node::<MarkerKey>(text("a b")).is_err());
    assert!(from_node::<MarkerWord>(text("UP")).is_err());
    assert!(from_node::<Sha256>(text("ABC")).is_err());
    assert!(from_node::<BlueprintNo>(Node::Uint(0)).is_err());
    assert!(from_node::<Bound>(Node::Uint(0)).is_err());
}

#[test]
fn bodies_and_bounds_read_back() {
    let sum = Sum {
        sha256: sha('a'),
        len: 12,
    };
    round(sum.clone());
    round(Body::Text("hello".into()));
    round(Body::Summed(sum));
    round(MarkName::read_list(["area: docs", "a#b"]));
    assert!(
        from_node::<Sum>(map(vec![("sha256", text("ABC")), ("len", Node::Uint(1))])).is_err(),
        "a sum's hash is checked when read"
    );
    round(Cite::Ticket(Ref::Here(TicketKey::Number(nz(1)))));
    round(Cite::Blueprint(elsewhere()));
    round(Cite::Change(Ref::Here(ChangeNo::new(3).unwrap())));
    round(Cite::Account(Login::new("someone")));
    round(Bound::new(50).unwrap());
    round(Window::Whole(Bound::new(10).unwrap()));
    round(Window::Last(nz32(100)));
    round(Depth::Cut { notes: 5 });
    round(Depth::Last { notes: nz32(3) });
    round(Depth::All);
    round(Within::Titles);
    round(Within::Everything);
}

#[test]
fn accounts_read_back() {
    round(Realm::CodeHost);
    round(Realm::Tickets);
    for kind in [
        AccountKind::Ours,
        AccountKind::Bot,
        AccountKind::Person,
        AccountKind::Unknown,
    ] {
        round(Account {
            realm: Realm::CodeHost,
            login: Login::new("someone"),
            id: Some(AccountId::new("42")),
            kind,
        });
    }
    round(Account {
        realm: Realm::Tickets,
        login: Login::new("ghost"),
        id: None,
        kind: AccountKind::Unknown,
    });
    for scope in [
        Scope::Full,
        Scope::Cycle,
        Scope::Session,
        Scope::Read,
        Scope::Files,
        Scope::Agent,
        Scope::Unknown,
    ] {
        round(scope);
        round(Actor::Ours(scope));
    }
    round(Actor::Person(PersonKey::new("p1")));
    round(Actor::Operator);
    round(Actor::Deploy(PersonKey::new("d1")));
    round(Actor::Given(PersonKey::new("g1")));
    round(Actor::Anonymous);
    round(SiteId::new("s"));
    round(GrantId::new("g"));
    round(Covers::Site(SiteId::new("s")));
    round(
        Covers::projects(
            GrantId::new("g"),
            vec![ProjectName::new("b"), ProjectName::new("a")],
        )
        .unwrap(),
    );
    round(Kept::Me);
    round(Kept::Grant);
}

#[test]
fn a_set_of_projects_is_sorted_when_read() {
    let node = map(vec![(
        "Projects",
        map(vec![
            ("grant", text("g")),
            ("projects", Node::Seq(vec![text("b"), text("a"), text("b")])),
        ]),
    )]);
    let read: Covers = from_node(node).unwrap();
    assert_eq!(
        read,
        Covers::projects(
            GrantId::new("g"),
            vec![ProjectName::new("a"), ProjectName::new("b")]
        )
        .unwrap()
    );
    let empty = map(vec![(
        "Projects",
        map(vec![("grant", text("g")), ("projects", Node::Seq(vec![]))]),
    )]);
    assert!(from_node::<Covers>(empty).is_err());
    assert!(from_node::<ProjectSet>(Node::Seq(vec![])).is_err());
}

#[test]
fn outcomes_read_back() {
    let all = vec![
        Outcome::NotFound,
        Outcome::Conflict(Lost::Changed),
        Outcome::Conflict(Lost::Gone),
        Outcome::Conflict(Lost::HeadMoved),
        Outcome::Conflict(Lost::Elsewhere),
        Outcome::Refused(Refusal::Invalid),
        Outcome::Refused(Refusal::Cycle),
        Outcome::Refused(Refusal::NotMergeable),
        Outcome::Refused(Refusal::Exists),
        Outcome::Refused(Refusal::Unsupported),
        Outcome::Refused(Refusal::ReadOnly),
        Outcome::Refused(Refusal::Unknown),
        Outcome::Forbidden,
        Outcome::SignedOut,
        Outcome::Spent {
            pool: Pool::Calls,
            until: At::new(5),
        },
        Outcome::Spent {
            pool: Pool::Priced,
            until: At::new(5),
        },
        Outcome::Spent {
            pool: Pool::Search,
            until: At::new(5),
        },
        Outcome::Spent {
            pool: Pool::Writes,
            until: At::new(5),
        },
        Outcome::Spent {
            pool: Pool::Unknown,
            until: At::new(5),
        },
        Outcome::Unavailable {
            why: Passing::Timeout,
            sent: Sent::No,
        },
        Outcome::Unavailable {
            why: Passing::Unreachable,
            sent: Sent::Maybe,
        },
        Outcome::Unavailable {
            why: Passing::Server,
            sent: Sent::No,
        },
        Outcome::Unavailable {
            why: Passing::Broken,
            sent: Sent::No,
        },
        Outcome::Unread(Unread::Cut),
        Outcome::Unread(Unread::Inconsistent),
        Outcome::Unread(Unread::TooLarge),
        Outcome::Unread(Unread::Malformed),
        Outcome::Unread(Unread::Unknown),
    ];
    for o in &all {
        round(o.clone());
    }
    round(Whole::new(vec![bp(1), bp(2)]));
    round(Took {
        items: vec![Login::new("a")],
        more: true,
    });
    round(Applied {
        done: vec![bp(1)],
        stopped: Some((bp(2), Outcome::Forbidden)),
    });
    round(Applied::<BlueprintNo> {
        done: vec![],
        stopped: None,
    });
    let got: Got<BlueprintNo> = Err(Outcome::NotFound);
    assert!(got.is_err());
    let did: Did = Ok(());
    assert!(did.is_ok());
}

#[test]
fn marks_read_back() {
    let acts = [
        Act::Approve,
        Act::Plan,
        Act::Refine,
        Act::Finish,
        Act::Block,
        Act::Pause,
        Act::Unknown,
    ];
    let states = [
        State::Running,
        State::Planning,
        State::Refreshing,
        State::AwaitingMerge,
        State::Shipped,
        State::Question,
        State::NeedsPerson,
        State::Replaced,
        State::Refined,
        State::Escalated,
        State::Proposal,
        State::Unknown,
    ];
    for a in acts {
        round(a);
        round(Mark::Act(a));
    }
    for s in states {
        round(s);
        round(Mark::State(s));
    }
    for c in [Choice::Lean, Choice::Classic, Choice::Unknown] {
        round(c);
        round(Mark::Choice(c));
    }
    round(Mark::Epic);
    round(Mark::Project(mark_name("area: docs")));
    round(Mark::Unknown);
    for k in [
        Kind::Feature,
        Kind::Bug,
        Kind::Task,
        Kind::Epic,
        Kind::Other(mark_name("Spike")),
        Kind::Unknown,
    ] {
        round(k);
    }
    round(Priority::Urgent);
    round(Priority::Normal);
    round(MarkSpec {
        name: mark_name("area: docs"),
        colour: Some(Colour::new("0e8a16").unwrap()),
        about: Some("Docs work".into()),
    });
    round(MarkSpec {
        name: mark_name("plain"),
        colour: None,
        about: None,
    });
}

fn every_marker() -> Vec<Marker> {
    let park = ParkMarker {
        blueprint: bp(7),
        step: MarkerWord::new("").unwrap(),
        kind: MarkerWord::new("timeout").unwrap(),
        provider: true,
        sig: sha('a'),
    };
    let mut markers = vec![
        Marker::Question {
            kind: QuestionKind::Plan,
            blueprint: bp(7),
            turn: nz32(1),
        },
        Marker::Question {
            kind: QuestionKind::Park,
            blueprint: bp(7),
            turn: nz32(2),
        },
        Marker::Question {
            kind: QuestionKind::Unknown,
            blueprint: bp(7),
            turn: nz32(3),
        },
        Marker::Blocked {
            by: vec![Ref::Here(TicketKey::Number(nz(3))), elsewhere()],
        },
        Marker::HoldCleared,
        Marker::Park(park),
        Marker::Spec(SpecMarker {
            version: nz32(1),
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
            note: "ran out of time at step 3".into(),
        }),
        Marker::TicketOf(Ref::Here(TicketKey::Key(KeyText::new("ABC-1").unwrap()))),
        Marker::Link {
            blueprint: bp(9),
            remove: false,
        },
        Marker::Project {
            name: mark_name("hold"),
            key: Some(MarkerKey::new("k.1").unwrap()),
            fields: vec![("why".into(), "waiting".into())],
        },
        Marker::Unreadable {
            kind: MarkerKind::Millwright,
        },
        Marker::Bare {
            kind: MarkerKind::Blocked,
            read: Some(Box::new(Marker::Blocked {
                by: vec![Ref::Here(TicketKey::Number(nz(4)))],
            })),
        },
        Marker::Bare {
            kind: MarkerKind::Park,
            read: None,
        },
        Marker::Unknown,
    ];
    for action in [
        MillwrightAction::Retry,
        MillwrightAction::Change,
        MillwrightAction::Answer,
        MillwrightAction::Finish,
        MillwrightAction::Person,
        MillwrightAction::Ask,
    ] {
        markers.push(Marker::Millwright(MillwrightMarker {
            blueprint: bp(7),
            action,
        }));
    }
    for (result, by_poll) in [
        (FinishResult::Opened, false),
        (FinishResult::Refused, false),
        (FinishResult::Stopped, true),
    ] {
        markers.push(Marker::Finish(FinishMarker {
            blueprint: bp(7),
            result,
            by_poll,
        }));
    }
    markers
}

#[test]
fn markers_read_back() {
    for kind in [
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
        MarkerKind::Project(mark_name("hold")),
        MarkerKind::Unknown,
    ] {
        round(kind);
    }
    for m in every_marker() {
        round(m);
    }
    round(MarkerEdit::default());
    round(MarkerEdit {
        set: vec![Marker::HoldCleared],
        drop: vec![
            (MarkerKind::Blocked, None),
            (
                MarkerKind::Question,
                Some(MarkerKey::new("blueprint=7.turn=1").unwrap()),
            ),
        ],
    });
}

#[test]
fn a_marker_read_back_is_checked() {
    let over = Marker::Blocked {
        by: (1..=21)
            .map(|n| Ref::Here(TicketKey::Number(nz(n))))
            .collect(),
    };
    let node = to_node(&over).unwrap();
    assert_eq!(
        from_node::<Marker>(node).unwrap(),
        Marker::Unreadable {
            kind: MarkerKind::Blocked
        }
    );
    let v2 = Marker::Spec(SpecMarker {
        version: nz32(SPEC_VERSION + 1),
        link: "https://example.test/spec.md".into(),
        sha256: sha('b'),
        requirements: vec![Requirement {
            id: "R1".into(),
            met: true,
            note: "done".into(),
        }],
    });
    let node = to_node(&v2).unwrap();
    assert_eq!(
        from_node::<Marker>(node).unwrap(),
        Marker::Unreadable {
            kind: MarkerKind::Spec
        }
    );
    // Inside a bare marker and an edit too.
    let bare = Marker::Bare {
        kind: MarkerKind::Spec,
        read: Some(Box::new(v2.clone())),
    };
    assert_eq!(
        from_node::<Marker>(to_node(&bare).unwrap()).unwrap(),
        Marker::Bare {
            kind: MarkerKind::Spec,
            read: Some(Box::new(Marker::Unreadable {
                kind: MarkerKind::Spec
            })),
        }
    );
    let edit = MarkerEdit {
        set: vec![over],
        drop: vec![],
    };
    assert_eq!(
        from_node::<MarkerEdit>(to_node(&edit).unwrap())
            .unwrap()
            .set,
        [Marker::Unreadable {
            kind: MarkerKind::Blocked
        }]
    );
}

/// A tag this copy does not know, with content of any shape.
fn future(content: Node) -> Node {
    map(vec![("type", text("Future")), ("value", content)])
}

#[test]
fn an_unknown_tag_reads_as_unknown() {
    let contents = [
        None,
        Some(Node::Uint(3)),
        Some(text("x")),
        Some(Node::Seq(vec![Node::Bool(true)])),
        Some(map(vec![("field", text("x"))])),
    ];
    for content in contents {
        let node = match content {
            Some(c) => future(c),
            None => map(vec![("type", text("Future"))]),
        };
        assert_eq!(from_node::<Mark>(node.clone()).unwrap(), Mark::Unknown);
        assert_eq!(from_node::<Kind>(node.clone()).unwrap(), Kind::Unknown);
        assert_eq!(
            from_node::<MarkerKind>(node.clone()).unwrap(),
            MarkerKind::Unknown
        );
        assert_eq!(from_node::<Marker>(node).unwrap(), Marker::Unknown);
    }
    let word = text("Future");
    assert_eq!(from_node::<Act>(word.clone()).unwrap(), Act::Unknown);
    assert_eq!(from_node::<State>(word.clone()).unwrap(), State::Unknown);
    assert_eq!(from_node::<Choice>(word.clone()).unwrap(), Choice::Unknown);
    assert_eq!(from_node::<Scope>(word.clone()).unwrap(), Scope::Unknown);
    assert_eq!(
        from_node::<Refusal>(word.clone()).unwrap(),
        Refusal::Unknown
    );
    assert_eq!(from_node::<Unread>(word.clone()).unwrap(), Unread::Unknown);
    assert_eq!(from_node::<Pool>(word.clone()).unwrap(), Pool::Unknown);
    assert_eq!(
        from_node::<QuestionKind>(word).unwrap(),
        QuestionKind::Unknown
    );
}

#[test]
fn an_unknown_word_inside_a_known_tag_is_that_enums_unknown() {
    let node = map(vec![("type", text("Act")), ("value", text("Future"))]);
    assert_eq!(from_node::<Mark>(node).unwrap(), Mark::Act(Act::Unknown));
    let node = map(vec![
        ("type", text("Unreadable")),
        ("value", map(vec![("kind", future(Node::Null))])),
    ]);
    assert_eq!(
        from_node::<Marker>(node).unwrap(),
        Marker::Unreadable {
            kind: MarkerKind::Unknown
        }
    );
}

#[test]
fn a_known_tag_whose_content_does_not_read_is_unknown_never_an_error() {
    // Every tag of each enum that may grow, with content that cannot read as
    // any variant's: an older reader of a newer record never fails it, so
    // each reads as `Unknown`.
    let bad = || Node::Seq(vec![Node::Bool(true), Node::Bool(false)]);
    let tagged = |tag: &str| map(vec![("type", text(tag)), ("value", bad())]);
    let tag_of = |n: &Node| match n {
        Node::Map(entries) => match &entries[0] {
            (k, Node::Text(t)) if *k == text("type") => t.clone(),
            other => panic!("a tag first, not {other:?}"),
        },
        other => panic!("a map, not {other:?}"),
    };
    let marks = [
        Mark::Act(Act::Approve),
        Mark::State(State::Running),
        Mark::Choice(Choice::Lean),
        Mark::Epic,
        Mark::Project(mark_name("x")),
    ];
    for m in &marks {
        let tag = tag_of(&to_node(m).unwrap());
        assert_eq!(
            from_node::<Mark>(tagged(&tag)).unwrap(),
            Mark::Unknown,
            "Mark {tag}"
        );
    }
    let kinds = [
        Kind::Feature,
        Kind::Bug,
        Kind::Task,
        Kind::Epic,
        Kind::Other(mark_name("x")),
    ];
    for k in &kinds {
        let tag = tag_of(&to_node(k).unwrap());
        assert_eq!(
            from_node::<Kind>(tagged(&tag)).unwrap(),
            Kind::Unknown,
            "Kind {tag}"
        );
    }
    let marker_kinds = [
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
        MarkerKind::Project(mark_name("x")),
    ];
    for k in &marker_kinds {
        let tag = tag_of(&to_node(k).unwrap());
        assert_eq!(
            from_node::<MarkerKind>(tagged(&tag)).unwrap(),
            MarkerKind::Unknown,
            "MarkerKind {tag}"
        );
    }
    // A marker reads it as `Unreadable` of the tag's kind instead, as the
    // text's reading does; a tag that names no one kind is of `Unknown`.
    for m in every_marker().iter().filter(|m| **m != Marker::Unknown) {
        let tag = tag_of(&to_node(m).unwrap());
        let kind = match m {
            Marker::Project { .. } | Marker::Unreadable { .. } | Marker::Bare { .. } => {
                MarkerKind::Unknown
            }
            known => known.kind(),
        };
        assert_eq!(
            from_node::<Marker>(tagged(&tag)).unwrap(),
            Marker::Unreadable { kind },
            "Marker {tag}"
        );
    }
    assert!(from_node::<Mark>(Node::Uint(3)).is_err());
    assert!(from_node::<Marker>(Node::Uint(3)).is_err());
}

#[test]
fn a_newer_word_inside_a_known_marker_reads_as_unreadable() {
    let marker = |tag: &str, value: Node| map(vec![("type", text(tag)), ("value", value)]);
    let millwright = marker(
        "Millwright",
        map(vec![
            ("blueprint", Node::Uint(7)),
            ("action", text("later")),
        ]),
    );
    assert_eq!(
        from_node::<Marker>(millwright).unwrap(),
        Marker::Unreadable {
            kind: MarkerKind::Millwright
        }
    );
    let finish = marker(
        "Finish",
        map(vec![
            ("blueprint", Node::Uint(7)),
            ("result", text("later")),
            ("by_poll", Node::Bool(false)),
        ]),
    );
    assert_eq!(
        from_node::<Marker>(finish).unwrap(),
        Marker::Unreadable {
            kind: MarkerKind::Finish
        }
    );
    // A number where an object is.
    assert_eq!(
        from_node::<Marker>(marker("Park", Node::Uint(3))).unwrap(),
        Marker::Unreadable {
            kind: MarkerKind::Park
        }
    );
    // Inside a bare marker too, and an unknown tag is still `Unknown`.
    let bare = marker(
        "Bare",
        map(vec![
            ("kind", map(vec![("type", text("Millwright"))])),
            ("read", marker("Millwright", Node::Uint(3))),
        ]),
    );
    assert_eq!(
        from_node::<Marker>(bare).unwrap(),
        Marker::Bare {
            kind: MarkerKind::Millwright,
            read: Some(Box::new(Marker::Unreadable {
                kind: MarkerKind::Millwright
            })),
        }
    );
    assert_eq!(
        from_node::<Marker>(future(Node::Uint(3))).unwrap(),
        Marker::Unknown
    );
}

#[test]
fn unknown_is_written_as_its_tag_alone() {
    let alone = map(vec![("type", text("Unknown"))]);
    assert_eq!(to_node(&Mark::Unknown).unwrap(), alone);
    assert_eq!(to_node(&Marker::Unknown).unwrap(), alone);
    assert_eq!(to_node(&Scope::Unknown).unwrap(), text("Unknown"));
}

#[test]
fn a_secret_is_no_data_type() {
    // Secret, Token and DeployCredential derive no Serialize: this test
    // only holds that they still build and print nothing of the secret.
    let t = Token {
        secret: Secret::new("s3cret"),
        until: At::new(0),
    };
    assert!(!format!("{t:?}").contains("s3cret"));
}

fn link() -> WebLink {
    WebLink::new("https://example.test/item/1").unwrap()
}

fn account() -> Account {
    Account {
        realm: Realm::CodeHost,
        login: Login::new("someone"),
        id: Some(AccountId::new("42")),
        kind: AccountKind::Person,
    }
}

fn tkey(n: u64) -> TicketKey {
    TicketKey::Number(nz(n))
}

fn commit() -> CommitId {
    CommitId::new("abc1234").unwrap()
}

fn branch(s: &str) -> Branch {
    Branch::new(s).unwrap()
}

fn sum() -> Sum {
    Sum {
        sha256: sha('d'),
        len: 9,
    }
}

fn markers() -> Vec<Marker> {
    vec![Marker::HoldCleared]
}

fn digest() -> TicketDigest {
    TicketDigest {
        key: tkey(1),
        title: "a ticket".into(),
        state: TicketState::Closed(CloseReason::Duplicate(tkey(2))),
        kind: Some(Kind::Bug),
        priority: Priority::Urgent,
        marks: vec![Mark::Act(Act::Approve)],
        target: Some(TargetName {
            id: TargetId::new("t1"),
            title: "2.0".into(),
        }),
        author: Some(account()),
        created: At::new(1),
        updated: At::new(2),
        revision: Revision::new("r1"),
        link: link(),
    }
}

fn comment() -> Comment {
    Comment {
        id: CommentId::new("c1"),
        author: Some(account()),
        editor: None,
        created: At::new(1),
        edited: Some(At::new(2)),
        body: "a comment".into(),
        markers: markers(),
        revision: Revision::new("r"),
        link: link(),
    }
}

fn every_happened() -> Vec<Happened> {
    let item = Ref::Here(Item::Change(ChangeNo::new(3).unwrap()));
    vec![
        Happened::Marked(Mark::Epic),
        Happened::Unmarked(Mark::State(State::Running)),
        Happened::Kind(Some(Kind::Task)),
        Happened::Kind(None),
        Happened::Priority(Priority::Normal),
        Happened::BlockerAdded(item.clone()),
        Happened::BlockerRemoved(elsewhere()),
        Happened::Assigned(account()),
        Happened::Unassigned(account()),
        Happened::Targeted(Some(TargetId::new("t"))),
        Happened::Targeted(None),
        Happened::Retitled {
            from: "a".into(),
            to: "b".into(),
        },
        Happened::Closed {
            reason: CloseReason::NotPlanned,
            closer: Some(Closer::Commit(commit())),
        },
        Happened::Reopened,
        Happened::Unknown,
    ]
}

fn ticket() -> Ticket {
    Ticket {
        digest: digest(),
        text: "text".into(),
        closed: Some(At::new(3)),
        closer: Some(Closer::Change {
            no: ChangeNo::new(4).unwrap(),
            merged: true,
        }),
        parts: vec![Part {
            key: Ref::Here(tkey(5)),
            open: true,
        }],
        blockers: vec![
            Blocker {
                on: Ref::Here(Item::Ticket(tkey(6))),
                state: BlockerState::Open,
            },
            Blocker {
                on: elsewhere(),
                state: BlockerState::Merged,
            },
        ],
        assignees: vec![account()],
        markers: markers(),
        mentioned_by: vec![Ref::Here(ChangeNo::new(4).unwrap())],
        history: Took {
            items: every_happened()
                .into_iter()
                .map(|what| TicketEvent {
                    what,
                    by: Some(account()),
                    at: Some(At::new(1)),
                })
                .collect(),
            more: true,
        },
    }
}

#[test]
fn tickets_read_back() {
    round(TicketState::Open);
    round(CloseReason::Done);
    round(digest());
    round(ticket());
    for s in [
        BlockerState::Open,
        BlockerState::Closed,
        BlockerState::Merged,
    ] {
        round(s);
    }
    round(Closer::Account(account()));
    for h in every_happened() {
        round(h);
    }
    round(TicketEvent {
        what: Happened::Reopened,
        by: None,
        at: None,
    });
    round(Target {
        id: TargetId::new("t"),
        title: "2.0".into(),
        due: Some(At::new(9)),
        created: At::new(1),
        open: true,
        open_count: 3,
        closed_count: 4,
    });
    for e in [
        TargetEdit::Retitle("x".into()),
        TargetEdit::Due(None),
        TargetEdit::Due(Some(At::new(1))),
        TargetEdit::Close,
        TargetEdit::Reopen,
    ] {
        round(e);
    }
    round(Filter::default());
    round(Filter {
        open: Some(true),
        carrying: vec![Mark::Act(Act::Plan)],
        kind: Some(Kind::Feature),
        target: Some(TargetId::new("t")),
    });
    round(NewTicket {
        title: "new".into(),
        text: Body::Text("text".into()),
        markers: markers(),
        marks: vec![Mark::Epic],
        kind: None,
        priority: Priority::Normal,
        target: None,
        parent: Some(tkey(1)),
    });
    round(Step::default());
    round(Step {
        note: Some("held".into()),
        markers: vec![Marker::Blocked {
            by: vec![Ref::Here(tkey(2))],
        }],
        unmark: vec![Mark::Act(Act::Approve)],
        mark: vec![Mark::Act(Act::Block)],
        close: Some(CloseReason::Done),
    });
    round((
        Applied {
            done: vec![StepPart::Note, StepPart::Close],
            stopped: None,
        },
        Some(Posted {
            id: CommentId::new("c1"),
            link: link(),
        }),
    ));
    round((
        Applied {
            done: vec![],
            stopped: Some((StepPart::Note, Outcome::Forbidden)),
        },
        None::<Posted<CommentId>>,
    ));
    for p in [
        StepPart::Note,
        StepPart::Unmark(Mark::Epic),
        StepPart::Mark(Mark::Epic),
        StepPart::Close,
    ] {
        round(p);
    }
    round(ClosedTicket {
        digest: digest(),
        closed: At::new(3),
        closer: None,
        part_count: 2,
        merged: vec![change_digest()],
    });
    round(TicketDelta {
        tickets: vec![TicketChange {
            digest: digest(),
            text: "t".into(),
            closed: None,
        }],
        notes: vec![(tkey(1), comment())],
        next: Cursor::new("next"),
    });
    round(Filed {
        key: tkey(1),
        title: "t".into(),
        first_line: "first".into(),
        link: link(),
    });
}

fn blueprint() -> Blueprint {
    Blueprint {
        no: bp(7),
        title: "b".into(),
        body: "body".into(),
        open: true,
        author: Some(account()),
        editor: Some(account()),
        created: At::new(1),
        edited: None,
        revision: Revision::new("r"),
        markers: markers(),
        comments: Took {
            items: vec![comment()],
            more: false,
        },
        link: link(),
    }
}

#[test]
fn blueprints_read_back() {
    round(blueprint());
    round(comment());
    round(Edit {
        by: None,
        at: At::new(1),
        after: "after".into(),
        markers: markers(),
    });
    round(Attachment {
        id: AttachmentId::new("a"),
        name: "shot.png".into(),
        size: Some(10),
    });
    round(NewBlueprint {
        title: "b".into(),
        body: Body::Summed(sum()),
        markers: vec![],
    });
    round(BlueprintHit {
        no: bp(7),
        title: "b".into(),
        open: false,
    });
    round(BlueprintDelta {
        blueprints: vec![BlueprintChange {
            no: bp(7),
            title: "b".into(),
            body: "body".into(),
            open: false,
            closed: Some(At::new(2)),
            revision: Revision::new("r"),
        }],
        comments: vec![(bp(7), comment())],
        next: Cursor::new("c"),
    });
}

fn change_digest() -> ChangeDigest {
    ChangeDigest {
        no: ChangeNo::new(4).unwrap(),
        title: "c".into(),
        state: ChangeState::Merged {
            at: At::new(1),
            by: Some(account()),
            commit: commit(),
        },
        head: branch("work"),
        head_commit: commit(),
        base: branch("main"),
        author: None,
        from_fork: false,
        draft: true,
        merge_state: Mergeable::No(NotMergeable::Behind),
        marks: vec![Mark::State(State::Refreshing)],
        closes: vec![Ref::Here(bp(7)), elsewhere()],
        size: Some(Size {
            files: 1,
            added: 2,
            deleted: 3,
        }),
        body: Some("body".into()),
        created: At::new(1),
        edited: None,
        editor: None,
        link: link(),
    }
}

fn new_change() -> NewChange {
    NewChange {
        head: branch("work"),
        base: branch("main"),
        title: "c".into(),
        body: Body::Text("body".into()),
        draft: false,
        closes: vec![bp(7)],
    }
}

fn proposal(takeover: Takeover) -> Proposal {
    Proposal {
        base: commit(),
        branch: branch("bump"),
        files: vec![(RepoPath::new("VERSION").unwrap(), sum())],
        message: "Bump".into(),
        change: new_change(),
        takeover,
    }
}

fn proposed() -> Proposed {
    Proposed {
        change: Posted {
            id: ChangeNo::new(4).unwrap(),
            link: link(),
        },
        head: commit(),
    }
}

fn every_takeover() -> Vec<Takeover> {
    vec![
        Takeover::Never,
        Takeover::OwnWithoutChange,
        Takeover::SameShape {
            message: "Bump".into(),
        },
        Takeover::Unknown,
    ]
}

fn every_change_happened() -> Vec<ChangeHappened> {
    vec![
        ChangeHappened::Pushed,
        ChangeHappened::Commented,
        ChangeHappened::Reviewed,
        ChangeHappened::Marked(Mark::Epic),
        ChangeHappened::Unmarked(Mark::Act(Act::Approve)),
        ChangeHappened::Merged,
        ChangeHappened::Closed,
        ChangeHappened::Reopened,
        ChangeHappened::Unknown,
    ]
}

#[test]
fn the_code_host_reads_back() {
    round(Facts {
        name: ProjectName::new("p"),
        public: true,
        tracker_on: true,
        archived: false,
        default_branch: branch("main"),
    });
    round(Mergeable::Yes);
    round(Mergeable::Unknown);
    for n in [
        NotMergeable::Conflict,
        NotMergeable::Behind,
        NotMergeable::ChecksFailing,
        NotMergeable::ReviewWanted,
        NotMergeable::Draft,
        NotMergeable::Blocked,
        NotMergeable::Unknown,
    ] {
        round(n);
        round(Mergeable::No(n));
    }
    round(ChangeState::Open);
    round(ChangeState::Closed { at: At::new(1) });
    round(change_digest());
    let verdicts = [
        Verdict::Approved,
        Verdict::ChangesAsked,
        Verdict::Commented,
        Verdict::Unknown,
    ];
    round(Change {
        digest: change_digest(),
        body: "body".into(),
        files: vec![ChangedFile {
            path: RepoPath::new("src/lib.rs").unwrap(),
            added: 1,
            deleted: 2,
        }],
        reviews: verdicts
            .iter()
            .map(|v| Review {
                by: Some(account()),
                verdict: *v,
                at: At::new(1),
                notes: vec![ReviewNote {
                    by: None,
                    path: Some(RepoPath::new("a").unwrap()),
                    body: "note".into(),
                }],
            })
            .collect(),
        comments: vec![comment()],
    });
    for what in every_change_happened() {
        round(ChangeEvent {
            what,
            by: None,
            at: At::new(1),
            committed: Some(At::new(0)),
        });
    }
    for s in [
        ChangeStates::Open,
        ChangeStates::Merged,
        ChangeStates::Closed,
        ChangeStates::All,
    ] {
        round(s);
    }
    round(ChangeFilter::default());
    round(ChangeFilter {
        states: ChangeStates::All,
        author: Some(Author::Account(AccountId::new("1"))),
        head: Some(branch("work")),
        closing: vec![bp(7)],
        merged_since: Some(At::new(1)),
        updated_since: None,
        numbers: vec![ChangeNo::new(4).unwrap()],
        bodies: true,
    });
    round(Author::Me);
    round(ChangeDelta {
        changes: vec![change_digest()],
        comments: vec![(ChangeNo::new(4).unwrap(), comment())],
        next: Cursor::new("c"),
    });
    let conclusions = [
        Conclusion::Passed,
        Conclusion::Failed,
        Conclusion::Cancelled,
        Conclusion::Skipped,
        Conclusion::Unknown,
    ];
    let mut runs: Vec<Check> = conclusions
        .iter()
        .map(|c| Check {
            name: "ci".into(),
            status: CheckStatus::Done(*c),
        })
        .collect();
    runs.push(Check {
        name: "q".into(),
        status: CheckStatus::Queued,
    });
    runs.push(Check {
        name: "r".into(),
        status: CheckStatus::Running,
    });
    for overall in [
        Overall::Pass,
        Overall::Fail,
        Overall::Pending,
        Overall::None,
    ] {
        round(Checks {
            runs: runs.clone(),
            overall,
        });
    }
    for state in [
        StatusState::Pending,
        StatusState::Passed,
        StatusState::Failed,
        StatusState::Errored,
    ] {
        round(Status {
            name: "tehas".into(),
            state,
            about: "ok".into(),
            link: Some(link()),
        });
    }
    for c in [
        Compared::Identical,
        Compared::Behind(1),
        Compared::Ahead(2),
        Compared::Diverged,
    ] {
        round(c);
    }
    round(new_change());
    for how in [MergeHow::Merge, MergeHow::Squash, MergeHow::Rebase] {
        round(how);
    }
    for t in every_takeover() {
        round(proposal(t));
    }
    for p in [
        ProposalPart::Branch,
        ProposalPart::Commit,
        ProposalPart::Change,
    ] {
        round(p);
    }
    round(proposed());
    round(Tag {
        name: TagName::new("v2.0.0").unwrap(),
        commit: commit(),
        at: None,
        annotated: true,
    });
    round(Release {
        id: ReleaseId::new("r"),
        tag: TagName::new("v2.0.0").unwrap(),
        title: "2.0.0".into(),
        draft: false,
        prerelease: false,
        published: Some(At::new(1)),
        files: vec![ReleaseFile {
            name: FileName::new("tehas.tar.gz").unwrap(),
            size: 10,
        }],
        link: link(),
    });
    round(NewRelease {
        tag: TagName::new("v2.0.0").unwrap(),
        title: "2.0.0".into(),
        notes: Body::Text("notes".into()),
        draft: true,
        prerelease: false,
    });
    round(Access::Fetch);
    round(Access::Push);
}

#[test]
fn identity_and_reports_read_back() {
    for s in [
        Standing::Admin,
        Standing::Write,
        Standing::Read,
        Standing::None,
    ] {
        round(s);
    }
    round(SignInState::new("s"));
    round(SignInCode::new("c"));
    for port in [
        PortName::Tickets,
        PortName::Blueprints,
        PortName::CodeHost,
        PortName::Identity,
        PortName::Board,
        PortName::Unknown,
    ] {
        round(port);
        round(Report {
            port,
            outcome: Outcome::Forbidden,
            calls: 2,
        });
    }
    let row = MeterRow {
        caller: Caller::new("dispatch.poll"),
        port: PortName::Board,
        pool: Pool::Priced,
        calls: 3,
        points: 12,
    };
    let node = to_node(&row).unwrap();
    let Node::Map(entries) = node else {
        panic!("a map")
    };
    assert_eq!(entries[0], (text("caller"), text("dispatch.poll")));
    assert_eq!(Caller::new("dispatch.poll").as_str(), "dispatch.poll");
}

fn every_write() -> Vec<Write> {
    let t = tkey(1);
    let item = Item::Ticket(tkey(2));
    let rev = Revision::new("r");
    let edit = MarkerEdit {
        set: markers(),
        drop: vec![(MarkerKind::Blocked, None)],
    };
    let text = |s: &str| Body::Text(s.into());
    let change = ChangeNo::new(4).unwrap();
    vec![
        Write::Mark {
            ticket: t.clone(),
            mark: Mark::Act(Act::Approve),
        },
        Write::Unmark {
            ticket: t.clone(),
            mark: Mark::Epic,
        },
        Write::Close {
            ticket: t.clone(),
            reason: CloseReason::Done,
        },
        Write::Reopen { ticket: t.clone() },
        Write::TicketNote {
            ticket: t.clone(),
            body: text("note"),
            markers: markers(),
        },
        Write::FileTicket(NewTicket {
            title: "new".into(),
            text: text("ticket text"),
            markers: vec![],
            marks: vec![],
            kind: Some(Kind::Other(mark_name("Spike"))),
            priority: Priority::Normal,
            target: None,
            parent: None,
        }),
        Write::EditTicket {
            ticket: t.clone(),
            title: Some("t".into()),
            text: Some(text("edited")),
            markers: edit.clone(),
            since: rev.clone(),
        },
        Write::SetKind {
            ticket: t.clone(),
            kind: None,
        },
        Write::SetPriority {
            ticket: t.clone(),
            priority: Priority::Urgent,
        },
        Write::WaitOn {
            ticket: t.clone(),
            on: item.clone(),
        },
        Write::StopWaiting {
            ticket: t.clone(),
            on: item,
        },
        Write::Assign {
            ticket: t.clone(),
            who: AccountId::new("42"),
        },
        Write::AddPart {
            parent: t.clone(),
            child: tkey(3),
        },
        Write::RemovePart {
            parent: t.clone(),
            child: tkey(3),
        },
        Write::NewTarget {
            title: "2.0".into(),
            due: Some(At::new(1)),
        },
        Write::EditTarget {
            id: TargetId::new("t"),
            edit: TargetEdit::Close,
        },
        Write::SetTarget {
            ticket: t,
            target: Some(TargetId::new("t")),
        },
        Write::Prepare,
        Write::EnsureMark(MarkSpec {
            name: mark_name("area: docs"),
            colour: None,
            about: None,
        }),
        Write::OpenBlueprint(NewBlueprint {
            title: "b".into(),
            body: text("blueprint body"),
            markers: vec![Marker::TicketOf(Ref::Here(tkey(1)))],
        }),
        Write::EditBlueprint {
            no: bp(7),
            title: None,
            body: Some(text("edited body")),
            markers: MarkerEdit::default(),
            since: rev.clone(),
        },
        Write::Comment {
            on: bp(7),
            body: text("comment"),
            markers: vec![],
        },
        Write::EditComment {
            id: CommentId::new("c"),
            body: text("ticked"),
            markers: edit.clone(),
            since: rev,
        },
        Write::CloseBlueprint {
            no: bp(7),
            done: true,
        },
        Write::OpenChange(new_change()),
        Write::EditChange {
            no: change,
            title: None,
            body: Some(text("stamped")),
            markers: edit,
        },
        Write::MarkChange {
            no: change,
            mark: Mark::State(State::Refreshing),
        },
        Write::UnmarkChange {
            no: change,
            mark: Mark::State(State::Refreshing),
        },
        Write::CommentChange {
            no: change,
            body: text("review"),
            markers: vec![],
        },
        Write::CloseChange { no: change },
        Write::Merge {
            no: change,
            head: commit(),
            how: MergeHow::Squash,
        },
        Write::Propose(proposal(Takeover::SameShape {
            message: "Bump".into(),
        })),
        Write::Tag {
            name: TagName::new("v2.0.0").unwrap(),
            at: commit(),
        },
        Write::SetStatus {
            at: commit(),
            status: Status {
                name: "tehas".into(),
                state: StatusState::Passed,
                about: "ok".into(),
                link: None,
            },
        },
        Write::NewRelease(NewRelease {
            tag: TagName::new("v2.0.0").unwrap(),
            title: "2.0.0".into(),
            notes: text("notes"),
            draft: true,
            prerelease: false,
        }),
        Write::Upload {
            release: ReleaseId::new("r"),
            name: FileName::new("tehas.tar.gz").unwrap(),
            file: sum(),
        },
        Write::PublishRelease {
            release: ReleaseId::new("r"),
        },
        Write::Unknown,
    ]
}

fn every_receipt() -> Vec<Receipt> {
    let posted = |id| Posted { id, link: link() };
    vec![
        Receipt::Done,
        Receipt::Comment(posted(CommentId::new("c"))),
        Receipt::Ticket(Posted {
            id: tkey(1),
            link: link(),
        }),
        Receipt::Blueprint(Posted {
            id: bp(7),
            link: link(),
        }),
        Receipt::Change(Posted {
            id: ChangeNo::new(4).unwrap(),
            link: link(),
        }),
        Receipt::Proposed(proposed()),
        Receipt::Target(TargetId::new("t")),
        Receipt::Release(Posted {
            id: ReleaseId::new("r"),
            link: link(),
        }),
        Receipt::Edited(link()),
        Receipt::Unknown,
    ]
}

/// The tag a value of an adjacently tagged enum is written with.
fn tag_of(n: &Node) -> String {
    match n {
        Node::Map(entries) => match &entries[0] {
            (k, Node::Text(t)) if *k == text("type") => t.clone(),
            other => panic!("a tag first, not {other:?}"),
        },
        other => panic!("a map, not {other:?}"),
    }
}

#[test]
fn every_write_and_receipt_reads_back() {
    for w in every_write() {
        round(w.clone());
        round(w.summed(given).unwrap());
        round(w.kind());
    }
    for r in every_receipt() {
        round(r);
    }
    round(Posted {
        id: bp(7),
        link: link(),
    });
}

#[test]
fn a_writes_kind_is_named_as_its_variant() {
    let writes = every_write();
    for w in &writes {
        let kind = to_node(&w.kind()).unwrap();
        assert_eq!(kind, text(&tag_of(&to_node(w).unwrap())), "{w:?}");
    }
    let mut kinds: Vec<WriteKind> = writes.iter().map(Write::kind).collect();
    kinds.dedup();
    assert_eq!(kinds.len(), writes.len(), "one kind per variant");
}

/// The standard sha-256 of "abc".
const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
/// The standard sha-256 of the empty string.
const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// The digests the tests give, as literals: the ports never hash. Any other
/// body takes a stand-in, but "unsummed", whose digest it does not give.
fn given(body: &str) -> Option<Sha256> {
    match body {
        "abc" => Sha256::new(ABC).ok(),
        "" => Sha256::new(EMPTY).ok(),
        "unsummed" => None,
        _ => Some(sha('7')),
    }
}

#[test]
fn a_summed_write_keeps_no_text() {
    let texts = |w: &Write| {
        format!("{:?}", to_node(w).unwrap())
            .matches(r#"Text("Text")"#)
            .count()
    };
    assert_eq!(
        every_write().iter().map(texts).sum::<usize>(),
        12,
        "every body"
    );
    for w in every_write() {
        let summed = w.summed(given).unwrap();
        assert!(
            !format!("{:?}", to_node(&summed).unwrap()).contains(r#"Text("Text")"#),
            "{:?} keeps a text",
            w.kind()
        );
        assert_eq!(summed.kind(), w.kind());
        assert_eq!(summed.summed(|_| None), Ok(summed.clone()), "asks for none");
    }
    for (body, digest, len) in [("abc", ABC, 3), ("", EMPTY, 0)] {
        let note = Write::TicketNote {
            ticket: tkey(1),
            body: Body::Text(body.into()),
            markers: vec![],
        };
        let Ok(Write::TicketNote { body, .. }) = note.summed(given) else {
            unreachable!()
        };
        assert_eq!(
            body,
            Body::Summed(Sum {
                sha256: Sha256::new(digest).unwrap(),
                len,
            })
        );
    }
    // A title is no body: it stays as written.
    let edit = Write::EditTicket {
        ticket: tkey(1),
        title: Some("kept".into()),
        text: None,
        markers: MarkerEdit::default(),
        since: Revision::new("r"),
    };
    assert_eq!(edit.summed(|_| None), Ok(edit));
}

#[test]
fn a_body_whose_sum_is_not_given_refuses_the_write() {
    let mut refused = 0;
    for w in every_write() {
        let has_body = format!("{:?}", to_node(&w).unwrap()).contains(r#"Text("Text")"#);
        let got = w.summed(|_| None);
        if has_body {
            assert_eq!(got, Err(Outcome::Refused(Refusal::Invalid)), "{w:?}");
            refused += 1;
        } else {
            assert_eq!(got, Ok(w));
        }
    }
    assert_eq!(refused, 12, "every body");
    // A lookup that holds other texts makes no guess either.
    let note = Write::Comment {
        on: bp(1),
        body: Body::Text("unsummed".into()),
        markers: vec![],
    };
    assert_eq!(note.summed(given), Err(Outcome::Refused(Refusal::Invalid)));
}

fn work_item() -> WorkItem {
    WorkItem {
        ticket: ticket(),
        notes: Took {
            items: vec![comment()],
            more: true,
        },
        blueprints: vec![blueprint()],
        changes: vec![change_digest()],
    }
}

#[test]
fn the_boards_asks_and_answers_read_back() {
    let ask = BoardAsk {
        depth: Depth::Last { notes: nz32(100) },
        history: Some(Window::Last(nz32(100))),
        changes: true,
        filter: Filter::default(),
        control: Some(tkey(9)),
    };
    round(ask);
    round(work_item());
    round(OpenWork {
        items: Whole::new(vec![
            (tkey(1), Ok(work_item())),
            (tkey(2), Err(Outcome::Unread(Unread::Cut))),
        ]),
        control: Some(ticket()),
    });
    round(ChangeAsk {
        filter: ChangeFilter::default(),
        files: true,
        closing: true,
        siblings: false,
        activity: Some(Window::Last(nz32(50))),
    });
    round(ChangeItem {
        change: change_digest(),
        files: Some(Whole::new(vec![])),
        closing: vec![(blueprint(), Some(digest())), (blueprint(), None)],
        siblings: vec![change_digest()],
        activity: Some(Took {
            items: vec![],
            more: false,
        }),
    });
}

#[test]
fn the_ports_enums_that_may_grow_read_an_unknown_tag_as_unknown() {
    let node = future(map(vec![("field", text("x"))]));
    assert_eq!(
        from_node::<Happened>(node.clone()).unwrap(),
        Happened::Unknown
    );
    assert_eq!(
        from_node::<ChangeHappened>(node.clone()).unwrap(),
        ChangeHappened::Unknown
    );
    assert_eq!(from_node::<Write>(node.clone()).unwrap(), Write::Unknown);
    assert_eq!(
        from_node::<Receipt>(node.clone()).unwrap(),
        Receipt::Unknown
    );
    assert_eq!(from_node::<Takeover>(node).unwrap(), Takeover::Unknown);
    let word = text("Future");
    assert_eq!(
        from_node::<WriteKind>(word.clone()).unwrap(),
        WriteKind::Unknown
    );
    assert_eq!(
        from_node::<NotMergeable>(word.clone()).unwrap(),
        NotMergeable::Unknown
    );
    assert_eq!(
        from_node::<Conclusion>(word.clone()).unwrap(),
        Conclusion::Unknown
    );
    assert_eq!(
        from_node::<Verdict>(word.clone()).unwrap(),
        Verdict::Unknown
    );
    assert_eq!(from_node::<PortName>(word).unwrap(), PortName::Unknown);
    let alone = map(vec![("type", text("Unknown"))]);
    assert_eq!(to_node(&Write::Unknown).unwrap(), alone);
    assert_eq!(to_node(&Receipt::Unknown).unwrap(), alone);
    assert_eq!(to_node(&WriteKind::Unknown).unwrap(), text("Unknown"));
}

/// An older reader never fails a record: a tag `T` does not know reads as
/// `unknown` whatever its content, and so does a tag of `samples` whose
/// content does not read.
fn reads_what_it_cannot_as_unknown<T>(samples: &[T], unknown: T)
where
    T: Serialize + for<'de> Deserialize<'de> + PartialEq + Debug,
{
    let name = std::any::type_name::<T>();
    let tagged = |tag: &str, content: Option<Node>| {
        let mut entries = vec![("type", text(tag))];
        entries.extend(content.map(|c| ("value", c)));
        map(entries)
    };
    let any = [
        None,
        Some(Node::Null),
        Some(Node::Bool(true)),
        Some(Node::Int(-1)),
        Some(text("x")),
        Some(Node::Seq(vec![Node::Bool(true)])),
        Some(map(vec![("field", text("x"))])),
    ];
    for content in any {
        let node = tagged("Future", content);
        assert_eq!(
            from_node::<T>(node.clone()).unwrap(),
            unknown,
            "{name} {node:?}"
        );
    }
    let bad = [
        Node::Bool(true),
        Node::Float(0.5),
        Node::Seq(vec![Node::Bool(true), Node::Bool(false)]),
    ];
    for v in samples.iter().filter(|v| **v != unknown) {
        let node = to_node(v).unwrap();
        let tag = tag_of(&node);
        for content in bad.iter().cloned() {
            let node = tagged(&tag, Some(content));
            assert_eq!(
                from_node::<T>(node.clone()).unwrap(),
                unknown,
                "{name} {node:?}"
            );
        }
        // A tag that takes content, given none: Unknown, or None where its
        // content is optional, never an error.
        if matches!(&node, Node::Map(entries) if entries.len() > 1) {
            let none = from_node::<T>(tagged(&tag, None));
            assert!(none.is_ok(), "{name} {tag}: {none:?}");
        }
    }
}

#[test]
fn a_record_the_reader_cannot_read_is_unknown_never_an_error() {
    reads_what_it_cannot_as_unknown(&every_happened(), Happened::Unknown);
    reads_what_it_cannot_as_unknown(&every_change_happened(), ChangeHappened::Unknown);
    reads_what_it_cannot_as_unknown(&every_write(), Write::Unknown);
    reads_what_it_cannot_as_unknown(&every_receipt(), Receipt::Unknown);
    reads_what_it_cannot_as_unknown(&every_takeover(), Takeover::Unknown);
}

#[test]
fn a_marker_inside_a_write_is_checked_when_read() {
    let over = Write::TicketNote {
        ticket: tkey(1),
        body: Body::Text("held".into()),
        markers: vec![Marker::Blocked {
            by: (1..=21).map(|n| Ref::Here(tkey(n))).collect(),
        }],
    };
    let Write::TicketNote { markers, .. } = from_node::<Write>(to_node(&over).unwrap()).unwrap()
    else {
        panic!("a note")
    };
    assert_eq!(
        markers,
        [Marker::Unreadable {
            kind: MarkerKind::Blocked
        }]
    );
}
