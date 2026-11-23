//! The freeze rule, as an implementer outside the crate meets it: every
//! answer an adapter builds is made with `new` or `Default`, never as a
//! whole literal, and what `new` does not take is empty, false or None.

use std::num::NonZeroU64;

use tehas_ports::*;

fn at() -> At {
    At::new(1_700_000_000)
}

fn link() -> WebLink {
    WebLink::new("https://example.test/item/1").unwrap()
}

fn key() -> TicketKey {
    TicketKey::Number(NonZeroU64::new(1).unwrap())
}

fn no() -> BlueprintNo {
    BlueprintNo::new(1).unwrap()
}

fn branch(s: &str) -> Branch {
    Branch::new(s).unwrap()
}

fn commit() -> CommitId {
    CommitId::new("abc1234").unwrap()
}

fn digest() -> TicketDigest {
    TicketDigest::new(
        key(),
        TicketState::Open,
        at(),
        at(),
        Revision::new("r1"),
        link(),
    )
}

fn change() -> ChangeDigest {
    ChangeDigest::new(
        ChangeNo::new(2).unwrap(),
        ChangeState::Open,
        branch("work"),
        commit(),
        branch("main"),
        commit(),
        at(),
        at(),
        link(),
    )
}

#[test]
fn new_leaves_every_other_field_empty() {
    let d = digest();
    assert_eq!(d.title, "");
    assert_eq!(d.kind, None);
    assert_eq!(d.priority, Priority::Normal);
    assert!(d.marks.is_empty() && d.target.is_none() && d.author.is_none());

    let t = Ticket::new(d.clone());
    assert!(t.text.is_empty() && t.parts.is_empty() && t.blockers.is_empty());
    assert_eq!(t.history, Took::default());
    assert!(!t.history.more);

    let c = change();
    assert_eq!(c.merge_state, Mergeable::Unknown);
    assert!(!c.draft && !c.from_fork && c.size.is_none() && c.body.is_none());

    let b = Blueprint::new(no(), at(), Revision::new("r1"), link());
    assert!(b.body.is_empty() && !b.open && b.comments.items.is_empty());

    let delta = TicketDelta::new(Cursor::new("c1"));
    assert!(delta.tickets.is_empty() && delta.notes.is_empty());
}

#[test]
fn an_implementer_sets_the_rest_by_name() {
    let mut d = digest();
    d.title = "A title".into();
    d.marks.push(Mark::Epic);
    assert_eq!(d.title, "A title");

    let mut r = Release::new(ReleaseId::new("1"), TagName::new("v1.0.0").unwrap(), link());
    r.draft = true;
    assert!(r.draft && !r.prerelease && r.files.is_empty());
}

#[test]
fn every_answer_has_new_or_default() {
    let account = Account::new(Realm::CodeHost, Login::new("someone"), AccountKind::Person);
    assert_eq!(account.id, None);
    let _ = TargetName::new(TargetId::new("t1"), "Target".into());
    let _ = Part::new(Ref::Here(key()), true);
    let _ = Blocker::new(Ref::Here(Item::Ticket(key())), BlockerState::Open);
    assert_eq!(TicketEvent::new(Happened::Marked(Mark::Epic)).at, None);
    assert!(!Target::new(TargetId::new("t1"), "T".into(), at()).open);
    assert!(ClosedTicket::new(digest(), at()).merged.is_empty());
    assert!(TicketChange::new(digest()).closed.is_none());
    assert!(Filed::new(key(), link()).first_line.is_empty());
    let comment = Comment::new(CommentId::new("c1"), at(), Revision::new("r1"), link());
    assert!(comment.markers.is_empty());
    assert!(Edit::new(at()).after.is_empty());
    let file = Attachment::new(AttachmentId::new("a1"), link());
    assert!(file.name.is_none() && file.size.is_none());
    assert!(BlueprintHit::new(no()).title.is_empty());
    assert!(BlueprintChange::new(no(), Revision::new("r1"))
        .closed
        .is_none());
    assert!(BlueprintDelta::new(Cursor::new("c1")).comments.is_empty());
    let facts = Facts::new(ProjectName::new("acme/widget"), branch("main"));
    assert!(!facts.public && !facts.archived);
    assert_eq!(Size::default().files, 0);
    assert!(Change::new(change()).files.is_empty());
    assert_eq!(ChangedFile::new(RepoPath::new("a.rs").unwrap()).added, 0);
    assert!(Review::new(Verdict::Approved, at()).notes.is_empty());
    assert!(ReviewNote::default().path.is_none());
    let event = ChangeEvent::new(ChangeHappened::Reopened, at());
    assert!(event.committed.is_none());
    assert!(ChangeDelta::new(Cursor::new("c1")).changes.is_empty());
    assert_eq!(Checks::default().overall, Overall::None);
    let _ = Check::new("build".into(), CheckStatus::Queued);
    let posted = Posted::new(ChangeNo::new(2).unwrap(), link());
    let _ = Proposed::new(posted, commit());
    assert!(!Tag::new(TagName::new("v1").unwrap(), commit()).annotated);
    let _ = ReleaseFile::new(FileName::new("a.tar.gz").unwrap(), 1);
    let remote = Remote::new(GitRemote::new("https://example.test/acme/widget.git").unwrap());
    assert!(remote.with.is_none());
    let _ = Token::new(Secret::new("s"), at());
    assert!(Whole::<Part>::default().items().is_empty());
    assert!(Applied::<Step>::default().stopped.is_none());
    assert!(WorkItem::new(Ticket::new(digest())).blueprints.is_empty());
    assert!(OpenWork::default().control.is_none());
    assert!(ChangeItem::new(change()).files.is_none());
    assert_eq!(Report::new(PortName::Tickets, Outcome::NotFound).calls, 0);
    let row = MeterRow::new(Caller::new("test"), PortName::Tickets, Pool::Calls);
    assert_eq!(row.points, 0);
    let _ = Budget::new(10, at());
    let _ = Failure::new(at(), SiteId::new("s"), Outcome::NotFound);
    let _ = Notice::new(
        HostId::new("h"),
        Whose::Host,
        Realm::CodeHost,
        Health::Recovered,
        at(),
    );
}

#[test]
fn a_marker_read_from_text_has_new() {
    let sig = Sha256::new("a".repeat(64)).unwrap();
    let park = ParkMarker::new(no(), sig.clone());
    assert!(park.step.as_str().is_empty() && park.kind.as_str().is_empty());
    assert!(!park.provider);
    assert_eq!(MarkerWord::default().as_str(), "");
    let _ = MillwrightMarker::new(no(), MillwrightAction::Retry);
    assert!(!FinishMarker::new(no(), FinishResult::Opened).by_poll);
    assert!(LastAttemptMarker::new(sig).note.is_empty());
}
