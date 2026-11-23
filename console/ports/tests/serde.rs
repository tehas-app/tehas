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
    assert!(from_node::<MarkName>(text("a/b")).is_err());
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
        sha256: "ab".repeat(32),
        len: 12,
    };
    round(sum.clone());
    round(Body::Text("hello".into()));
    round(Body::Summed(sum));
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
    round(Covers::projects(
        GrantId::new("g"),
        vec![ProjectName::new("b"), ProjectName::new("a")],
    ));
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
    );
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
        Act::AllHands,
        Act::Prototype,
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
fn a_known_tag_whose_content_does_not_read_is_an_error() {
    // Every tag of each enum that may grow, with content that cannot read as
    // any variant's. None may read as `Unknown`.
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
        assert!(from_node::<Mark>(tagged(&tag)).is_err(), "Mark {tag}");
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
        assert!(from_node::<Kind>(tagged(&tag)).is_err(), "Kind {tag}");
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
        assert!(
            from_node::<MarkerKind>(tagged(&tag)).is_err(),
            "MarkerKind {tag}"
        );
    }
    for m in every_marker().iter().filter(|m| **m != Marker::Unknown) {
        let tag = tag_of(&to_node(m).unwrap());
        assert!(from_node::<Marker>(tagged(&tag)).is_err(), "Marker {tag}");
    }
    assert!(from_node::<Mark>(Node::Uint(3)).is_err());
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
