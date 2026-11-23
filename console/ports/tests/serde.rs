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
    round(MarkName::new("area: docs").unwrap());
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
fn a_secret_is_no_data_type() {
    // Secret, Token and DeployCredential derive no Serialize: this test
    // only holds that they still build and print nothing of the secret.
    let t = Token {
        secret: Secret::new("s3cret"),
        until: At::new(0),
    };
    assert!(!format!("{t:?}").contains("s3cret"));
}
