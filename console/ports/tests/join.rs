//! `Join` over small stubs of the three read handles and the links: each
//! item answers alone, the links say which blueprints a ticket has, the
//! changes come open first, and a `Setup` takes each handle from the site of
//! its realm. And every trait the spec uses as `dyn` is object safe.

use std::num::{NonZeroU32, NonZeroU64};
use std::sync::Arc;

use tehas_ports::*;

fn link() -> WebLink {
    WebLink::new("https://example.test/item").unwrap()
}

fn key(s: &str) -> TicketKey {
    TicketKey::Key(KeyText::new(s).unwrap())
}

fn bp(n: u64) -> BlueprintNo {
    BlueprintNo::new(n).unwrap()
}

fn cn(n: u64) -> ChangeNo {
    ChangeNo::new(n).unwrap()
}

fn bound() -> Bound {
    Bound::new(50).unwrap()
}

fn unsupported() -> Outcome {
    Outcome::Refused(Refusal::Unsupported)
}

fn digest(key: TicketKey, open: bool) -> TicketDigest {
    TicketDigest {
        key,
        title: "t".into(),
        state: if open {
            TicketState::Open
        } else {
            TicketState::Closed(CloseReason::Done)
        },
        kind: None,
        priority: Priority::Normal,
        marks: vec![],
        target: None,
        author: None,
        created: At::new(0),
        updated: At::new(0),
        revision: Revision::new("r"),
        link: link(),
    }
}

fn ticket(key: TicketKey, open: bool) -> Ticket {
    Ticket {
        digest: digest(key, open),
        text: "text".into(),
        closed: None,
        closer: None,
        parts: vec![],
        blockers: vec![],
        assignees: vec![],
        markers: vec![],
        mentioned_by: vec![],
        history: Took {
            items: vec![],
            more: false,
        },
    }
}

fn comment(id: &str) -> Comment {
    Comment {
        id: CommentId::new(id),
        author: None,
        editor: None,
        created: At::new(0),
        edited: None,
        body: id.into(),
        markers: vec![],
        revision: Revision::new("r"),
        link: link(),
    }
}

fn blueprint(no: u64, comments: Vec<Comment>) -> Blueprint {
    Blueprint {
        no: bp(no),
        title: "b".into(),
        body: "body".into(),
        open: true,
        author: None,
        editor: None,
        created: At::new(0),
        edited: None,
        revision: Revision::new("r"),
        markers: vec![],
        comments: Took {
            items: comments,
            more: false,
        },
        link: link(),
    }
}

fn change(no: u64, state: ChangeState, closes: Vec<Ref<BlueprintNo>>) -> ChangeDigest {
    ChangeDigest {
        no: cn(no),
        title: "c".into(),
        state,
        head: Branch::new("work").unwrap(),
        head_commit: CommitId::new("abc1234").unwrap(),
        base: Branch::new("main").unwrap(),
        author: None,
        from_fork: false,
        draft: false,
        merge_state: Mergeable::Unknown,
        marks: vec![],
        closes,
        size: None,
        body: None,
        created: At::new(0),
        edited: None,
        editor: None,
        link: link(),
    }
}

fn merged() -> ChangeState {
    ChangeState::Merged {
        at: At::new(1),
        by: None,
        commit: CommitId::new("def5678").unwrap(),
    }
}

/// A tickets handle over a few tickets. A ticket in `unread` reads as
/// `Unread(Cut)`. Its list refuses an ask that is not for open tickets.
#[derive(Clone, Default)]
struct StubTickets {
    tickets: Vec<Ticket>,
    notes: Vec<(TicketKey, Comment)>,
    unread: Vec<TicketKey>,
    list_fails: bool,
}

impl StubTickets {
    fn find(&self, key: &TicketKey) -> Got<Ticket> {
        if self.unread.contains(key) {
            return Err(Outcome::Unread(Unread::Cut));
        }
        let t = self.tickets.iter().find(|t| t.digest.key == *key);
        t.cloned().ok_or(Outcome::NotFound)
    }
}

impl Reports for StubTickets {
    fn take_reports(&mut self) -> Vec<Report> {
        vec![Report {
            port: PortName::Tickets,
            outcome: Outcome::NotFound,
            calls: 2,
        }]
    }
}

impl Cites for StubTickets {
    fn cite(&self, _: &Cite) -> String {
        String::new()
    }
}

impl Tickets for StubTickets {
    fn ticket(&mut self, key: &TicketKey, _: Depth) -> Got<Ticket> {
        self.find(key)
    }
    fn what_is(&mut self, _: NonZeroU64) -> Got<Item> {
        Err(unsupported())
    }
    fn what_are(&mut self, _: &[NonZeroU64]) -> Vec<(NonZeroU64, Got<Item>)> {
        vec![]
    }
    fn list(&mut self, filter: &Filter, _: Bound) -> Got<Whole<TicketDigest>> {
        if self.list_fails {
            return Err(Outcome::Unavailable {
                why: Passing::Timeout,
                sent: Sent::No,
            });
        }
        if filter.open != Some(true) {
            return Err(Outcome::Refused(Refusal::Invalid));
        }
        let open = self.tickets.iter().map(|t| t.digest.clone());
        let mut listed: Vec<TicketDigest> = open.filter(|d| d.state == TicketState::Open).collect();
        listed.extend(self.unread.iter().map(|k| digest(k.clone(), true)));
        Ok(Whole::new(listed))
    }
    fn several(&mut self, keys: &[TicketKey]) -> Vec<(TicketKey, Got<TicketDigest>)> {
        keys.iter()
            .map(|k| (k.clone(), self.find(k).map(|t| t.digest)))
            .collect()
    }
    fn history(
        &mut self,
        _: &TicketKey,
        window: Window,
        _: Option<&[HappenedKind]>,
    ) -> Got<Took<TicketEvent>> {
        Ok(Took {
            items: vec![TicketEvent {
                what: Happened::Reopened,
                by: None,
                at: None,
            }],
            more: matches!(window, Window::Last(_)),
        })
    }
    fn notes(&mut self, key: &TicketKey, _: Bound) -> Got<Whole<Comment>> {
        let mine = self.notes.iter().filter(|(k, _)| k == key);
        Ok(Whole::new(mine.map(|(_, c)| c.clone()).collect()))
    }
    fn note_edits(&mut self, _: &CommentId, _: Bound) -> Got<Whole<Edit>> {
        Err(unsupported())
    }
    fn blockers(&mut self, _: &TicketKey) -> Got<Whole<Blocker>> {
        Err(unsupported())
    }
    fn parts(&mut self, _: &TicketKey) -> Got<Whole<Part>> {
        Err(unsupported())
    }
    fn closed_since(&mut self, _: At, _: Bound) -> Got<Whole<ClosedTicket>> {
        let closed = self
            .tickets
            .iter()
            .filter(|t| t.digest.state != TicketState::Open);
        Ok(Whole::new(
            closed
                .map(|t| ClosedTicket {
                    digest: t.digest.clone(),
                    closed: At::new(1),
                    closer: None,
                    part_count: 0,
                    merged: vec![],
                })
                .collect(),
        ))
    }
    fn search(
        &mut self,
        _: &str,
        _: Within,
        _: Option<bool>,
        _: NonZeroU32,
    ) -> Got<Took<TicketDigest>> {
        Err(unsupported())
    }
    fn filed_by(&mut self, _: &AccountId, _: At, _: Bound) -> Got<Whole<Filed>> {
        Err(unsupported())
    }
    fn find_marked(
        &mut self,
        _: &Mark,
        _: &MarkerKind,
        _: &MarkerKey,
        _: Option<bool>,
    ) -> Got<Whole<TicketKey>> {
        Err(unsupported())
    }
    fn kinds(&mut self) -> Got<Whole<Kind>> {
        Err(unsupported())
    }
    fn targets(&mut self, _: Bound) -> Got<Whole<Target>> {
        Err(unsupported())
    }
    fn members(&mut self, _: &TargetId, _: Bound) -> Got<Whole<TicketDigest>> {
        Ok(Whole::new(
            self.tickets.iter().map(|t| t.digest.clone()).collect(),
        ))
    }
    fn changed(&mut self, _: Option<&ChangeToken>) -> Got<(bool, ChangeToken)> {
        Err(unsupported())
    }
    fn delta(&mut self, _: Option<&Cursor>, _: Bound) -> Got<TicketDelta> {
        Err(unsupported())
    }
    fn new_ticket_link(&self, _: &str, _: &str) -> WebLink {
        link()
    }
}

/// A blueprints handle over a few blueprints. `several` leaves out a
/// blueprint it does not hold.
#[derive(Clone, Default)]
struct StubBlueprints {
    blueprints: Vec<Blueprint>,
}

impl Reports for StubBlueprints {
    fn take_reports(&mut self) -> Vec<Report> {
        vec![Report {
            port: PortName::Blueprints,
            outcome: Outcome::Unread(Unread::Cut),
            calls: 1,
        }]
    }
}

impl Cites for StubBlueprints {
    fn cite(&self, _: &Cite) -> String {
        String::new()
    }
}

impl Blueprints for StubBlueprints {
    fn blueprint(&mut self, no: BlueprintNo, _: Depth) -> Got<Blueprint> {
        let b = self.blueprints.iter().find(|b| b.no == no);
        b.cloned().ok_or(Outcome::NotFound)
    }
    fn several(&mut self, nos: &[BlueprintNo], _: Depth) -> Vec<(BlueprintNo, Got<Blueprint>)> {
        let held = self.blueprints.iter().filter(|b| nos.contains(&b.no));
        held.rev().map(|b| (b.no, Ok(b.clone()))).collect()
    }
    fn comments(&mut self, _: BlueprintNo, _: Bound) -> Got<Whole<Comment>> {
        Err(unsupported())
    }
    fn comments_by_id(&mut self, _: &[CommentId]) -> Vec<(CommentId, Got<Comment>)> {
        vec![]
    }
    fn body_edits(&mut self, _: BlueprintNo, _: Bound) -> Got<Whole<Edit>> {
        Err(unsupported())
    }
    fn comment_edits(&mut self, _: &CommentId, _: Bound) -> Got<Whole<Edit>> {
        Err(unsupported())
    }
    fn search(
        &mut self,
        _: &str,
        _: Within,
        _: Option<bool>,
        _: NonZeroU32,
    ) -> Got<Took<BlueprintHit>> {
        Err(unsupported())
    }
    fn attachments(&mut self, _: BlueprintNo) -> Got<Whole<Attachment>> {
        Err(unsupported())
    }
    fn fetch(&mut self, _: &AttachmentId, _: u64, _: &mut dyn std::io::Write) -> Did {
        Err(unsupported())
    }
    fn changed(&mut self, _: Option<&ChangeToken>) -> Got<(bool, ChangeToken)> {
        Err(unsupported())
    }
    fn delta(&mut self, _: Option<&Cursor>, _: Bound) -> Got<BlueprintDelta> {
        Err(unsupported())
    }
}

/// A code host handle over a few changes, filtered by state and by the
/// blueprints they close. Its report has the tickets handle's kind, so the
/// join adds the two.
#[derive(Clone, Default)]
struct StubCodeHost {
    changes: Vec<ChangeDigest>,
}

impl Reports for StubCodeHost {
    fn take_reports(&mut self) -> Vec<Report> {
        vec![Report {
            port: PortName::Tickets,
            outcome: Outcome::NotFound,
            calls: 1,
        }]
    }
}

impl Cites for StubCodeHost {
    fn cite(&self, _: &Cite) -> String {
        String::new()
    }
}

impl CodeHost for StubCodeHost {
    fn facts(&mut self) -> Got<Facts> {
        Err(unsupported())
    }
    fn branch(&mut self, _: &Branch) -> Got<Option<CommitId>> {
        Err(unsupported())
    }
    fn compare(&mut self, _: &CommitId, _: &CommitId) -> Got<Compared> {
        Err(unsupported())
    }
    fn file(&mut self, _: &RepoPath, _: &Rev, _: u64) -> Got<Vec<u8>> {
        Err(unsupported())
    }
    fn changes(&mut self, filter: &ChangeFilter, _: Bound) -> Got<Whole<ChangeDigest>> {
        let state_ok = |c: &ChangeDigest| {
            matches!(
                (filter.states, &c.state),
                (ChangeStates::All, _)
                    | (ChangeStates::Open, ChangeState::Open)
                    | (ChangeStates::Merged, ChangeState::Merged { .. })
                    | (ChangeStates::Closed, ChangeState::Closed { .. })
            )
        };
        let closing_ok = |c: &ChangeDigest| {
            filter.closing.is_empty()
                || filter
                    .closing
                    .iter()
                    .any(|no| c.closes.contains(&Ref::Here(*no)))
        };
        let taken = self.changes.iter().filter(|c| state_ok(c) && closing_ok(c));
        Ok(Whole::new(taken.cloned().collect()))
    }
    fn change(&mut self, _: ChangeNo, _: Bound) -> Got<Change> {
        Err(unsupported())
    }
    fn changed_files(&mut self, _: ChangeNo, _: Bound) -> Got<Whole<ChangedFile>> {
        Ok(Whole::new(vec![ChangedFile {
            path: RepoPath::new("src/lib.rs").unwrap(),
            added: 1,
            deleted: 0,
        }]))
    }
    fn activity(&mut self, _: ChangeNo, _: Window) -> Got<Took<ChangeEvent>> {
        Ok(Took {
            items: vec![],
            more: true,
        })
    }
    fn checks(&mut self, _: &CommitId, _: Option<&str>) -> Got<Checks> {
        Err(unsupported())
    }
    fn tags(&mut self, _: Bound) -> Got<Whole<Tag>> {
        Err(unsupported())
    }
    fn resolve_tag(&mut self, _: &TagName) -> Got<CommitId> {
        Err(unsupported())
    }
    fn releases(&mut self, _: Bound) -> Got<Whole<Release>> {
        Err(unsupported())
    }
    fn latest_release(&mut self) -> Got<Option<Release>> {
        Err(unsupported())
    }
    fn release(&mut self, _: &TagName) -> Got<Release> {
        Err(unsupported())
    }
    fn download(&mut self, _: &TagName, _: &FileName, _: u64, _: &mut dyn std::io::Write) -> Did {
        Err(unsupported())
    }
    fn archive(&mut self, _: &CommitId, _: u64, _: &mut dyn std::io::Write) -> Did {
        Err(unsupported())
    }
    fn changed(&mut self, _: Option<&ChangeToken>) -> Got<(bool, ChangeToken)> {
        Err(unsupported())
    }
    fn delta(&mut self, _: Option<&Cursor>, _: Bound) -> Got<ChangeDelta> {
        Err(unsupported())
    }
    fn remote(&mut self, _: Access) -> Got<Remote> {
        Err(unsupported())
    }
}

/// Links as a table of pairs.
struct StubLinks(Vec<(TicketKey, BlueprintNo)>);

impl Links for StubLinks {
    fn blueprints_of(&self, ticket: &TicketKey) -> Got<Vec<BlueprintNo>> {
        let mine = self.0.iter().filter(|(t, _)| t == ticket);
        Ok(mine.map(|(_, b)| *b).collect())
    }
    fn ticket_of(&self, blueprint: BlueprintNo) -> Got<Option<TicketKey>> {
        let found = self.0.iter().find(|(_, b)| *b == blueprint);
        Ok(found.map(|(t, _)| t.clone()))
    }
}

/// A split setup's three handles: tickets keyed apart from blueprints,
/// blueprints numbered apart from changes.
fn handles() -> (StubTickets, StubBlueprints, StubCodeHost, StubLinks) {
    let tickets = StubTickets {
        tickets: vec![
            ticket(key("PROJ-1"), true),
            ticket(key("PROJ-2"), true),
            ticket(key("PROJ-4"), true),
            ticket(key("PROJ-5"), false),
            ticket(key("CTRL-1"), true),
        ],
        notes: ["n1", "n2", "n3"]
            .into_iter()
            .map(|id| (key("PROJ-1"), comment(id)))
            .collect(),
        unread: vec![key("PROJ-3")],
        list_fails: false,
    };
    let blueprints = StubBlueprints {
        blueprints: vec![
            blueprint(7, vec![]),
            blueprint(8, vec![]),
            blueprint(9, vec![]),
        ],
    };
    let here = |n| vec![Ref::Here(bp(n))];
    let code_host = StubCodeHost {
        changes: vec![
            change(21, merged(), here(7)),
            change(20, ChangeState::Open, here(7)),
            change(
                22,
                ChangeState::Open,
                vec![Ref::Here(bp(7)), Ref::Here(bp(8))],
            ),
            change(
                23,
                ChangeState::Open,
                vec![Ref::Elsewhere {
                    project: ProjectName::new("other"),
                    item: ItemName::new("other 7"),
                }],
            ),
        ],
    };
    let links = StubLinks(vec![
        (key("PROJ-1"), bp(7)),
        (key("PROJ-1"), bp(8)),
        (key("PROJ-4"), bp(99)),
    ]);
    (tickets, blueprints, code_host, links)
}

fn join() -> Join {
    let (t, b, c, l) = handles();
    Join {
        tickets: Box::new(t),
        blueprints: Box::new(b),
        code_host: Box::new(c),
        links: Arc::new(l),
    }
}

fn ask() -> BoardAsk {
    BoardAsk {
        depth: Depth::Last {
            notes: NonZeroU32::new(2).unwrap(),
        },
        history: Some(Window::Last(NonZeroU32::new(100).unwrap())),
        changes: true,
        filter: Filter::default(),
        control: Some(key("CTRL-1")),
    }
}

fn nos(items: &[ChangeDigest]) -> Vec<u64> {
    items.iter().map(|c| c.no.get()).collect()
}

#[test]
fn open_work_answers_each_item_alone() {
    let work = join().open_work(&ask(), bound()).unwrap();
    assert_eq!(work.control.unwrap().digest.key, key("CTRL-1"));
    let items = work.items.items();
    let keys: Vec<&TicketKey> = items.iter().map(|(k, _)| k).collect();
    assert_eq!(
        keys,
        [
            &key("PROJ-1"),
            &key("PROJ-2"),
            &key("PROJ-4"),
            &key("CTRL-1"),
            &key("PROJ-3")
        ]
    );

    let one = items[0].1.as_ref().unwrap();
    let linked: Vec<u64> = one.blueprints.iter().map(|b| b.no.get()).collect();
    assert_eq!(linked, [7, 8], "the links, in their order");
    assert_eq!(nos(&one.changes), [20, 22, 21], "open first");
    let notes: Vec<&str> = one.notes.items.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(notes, ["n2", "n3"], "the newest, as deep as asked");
    assert!(one.notes.more);
    assert!(
        one.ticket.history.more,
        "the history read with the ask's window"
    );
    assert_eq!(
        one.ticket_comments(),
        &one.notes,
        "a ticket by key keeps its notes"
    );

    let two = items[1].1.as_ref().unwrap();
    assert!(two.blueprints.is_empty() && two.changes.is_empty());

    assert_eq!(
        items[2].1,
        Err(Outcome::Unread(Unread::Inconsistent)),
        "a linked blueprint the answer leaves out fails its item"
    );
    assert_eq!(items[4].1, Err(Outcome::Unread(Unread::Cut)), "fails alone");
}

#[test]
fn a_whole_list_or_a_control_ticket_that_fails_fails_the_read() {
    let (mut t, b, c, l) = handles();
    t.list_fails = true;
    let mut failing = Join {
        tickets: Box::new(t),
        blueprints: Box::new(b),
        code_host: Box::new(c),
        links: Arc::new(l),
    };
    assert!(matches!(
        failing.open_work(&ask(), bound()),
        Err(Outcome::Unavailable { .. })
    ));
    let mut unread_control = ask();
    unread_control.control = Some(key("PROJ-3"));
    assert_eq!(
        join().open_work(&unread_control, bound()),
        Err(Outcome::Unread(Unread::Cut))
    );
}

#[test]
fn a_cheap_ask_reads_no_changes_and_keeps_the_tickets_history() {
    let cheap = BoardAsk {
        depth: Depth::Cut { notes: 0 },
        history: None,
        changes: false,
        filter: Filter::default(),
        control: None,
    };
    let item = join().work_item(&key("PROJ-1"), &cheap).unwrap();
    assert!(item.changes.is_empty());
    assert!(item.notes.items.is_empty() && item.notes.more);
    assert!(item.ticket.history.items.is_empty());
}

#[test]
fn closed_and_target_work_answer_per_item() {
    let closed = join().closed_work(&ask(), At::new(0), bound()).unwrap();
    let keys: Vec<&TicketKey> = closed.items().iter().map(|(k, _)| k).collect();
    assert_eq!(keys, [&key("PROJ-5")]);
    let target = join()
        .target_work(&ask(), &TargetId::new("t"), bound())
        .unwrap();
    assert_eq!(target.items().len(), 5);
    assert!(target.items()[2].1.is_err());
    assert!(target.items()[0].1.is_ok());
}

#[test]
fn open_changes_bring_what_they_close_and_their_siblings() {
    let ask = ChangeAsk {
        filter: ChangeFilter::default(),
        files: true,
        closing: true,
        siblings: true,
        activity: Some(Window::Last(NonZeroU32::new(50).unwrap())),
    };
    let items = join().open_changes(&ask, bound()).unwrap().into_items();
    let changes: Vec<u64> = items.iter().map(|i| i.change.no.get()).collect();
    assert_eq!(changes, [20, 22, 23], "open changes alone, by default");

    let twenty = &items[0];
    assert_eq!(twenty.files.as_ref().unwrap().items().len(), 1);
    assert_eq!(twenty.closing.len(), 1);
    assert_eq!(twenty.closing[0].0.no, bp(7));
    assert_eq!(
        twenty.closing[0].1.as_ref().map(|d| &d.key),
        Some(&key("PROJ-1")),
        "the linked ticket"
    );
    assert_eq!(nos(&twenty.siblings), [22]);
    assert!(twenty.activity.as_ref().unwrap().more);

    let elsewhere = &items[2];
    assert!(elsewhere.closing.is_empty() && elsewhere.siblings.is_empty());

    let bare = ChangeAsk {
        filter: ChangeFilter::default(),
        files: false,
        closing: false,
        siblings: false,
        activity: None,
    };
    let items = join().open_changes(&bare, bound()).unwrap().into_items();
    assert!(items
        .iter()
        .all(|i| i.files.is_none() && i.closing.is_empty() && i.activity.is_none()));
}

#[test]
fn the_join_reports_what_its_handles_met_each_kind_once() {
    let reports = join().take_reports();
    assert_eq!(
        reports,
        [
            Report {
                port: PortName::Tickets,
                outcome: Outcome::NotFound,
                calls: 3,
            },
            Report {
                port: PortName::Blueprints,
                outcome: Outcome::Unread(Unread::Cut),
                calls: 1,
            },
        ]
    );
}

#[test]
fn one_item_reads_its_markers_from_the_blueprints_comments() {
    let n = NonZeroU64::new(7).unwrap();
    let item = WorkItem {
        ticket: ticket(TicketKey::Number(n), true),
        notes: Took {
            items: vec![],
            more: false,
        },
        blueprints: vec![blueprint(7, vec![comment("on the item")])],
        changes: vec![],
    };
    assert_eq!(item.ticket_comments().items[0].id.as_str(), "on the item");
}

/// A site that makes the handles it holds and answers `fail` for the rest.
struct StubSite {
    realms: Vec<Realm>,
    fail: Outcome,
    tickets: Option<StubTickets>,
    blueprints: Option<StubBlueprints>,
    code_host: Option<StubCodeHost>,
}

impl StubSite {
    fn failing(realm: Realm, fail: Outcome) -> Self {
        StubSite {
            realms: vec![realm],
            fail,
            tickets: None,
            blueprints: None,
            code_host: None,
        }
    }
}

impl Site for StubSite {
    fn realms(&self) -> &[Realm] {
        &self.realms
    }
    fn tickets(&self, _: &Actor, _: Caller) -> Got<Box<dyn Tickets + Send>> {
        match &self.tickets {
            Some(t) => Ok(Box::new(t.clone())),
            None => Err(self.fail.clone()),
        }
    }
    fn ticket_writes(&self, _: &Actor, _: Caller) -> Got<Box<dyn TicketWrites + Send>> {
        Err(self.fail.clone())
    }
    fn blueprints(&self, _: &Actor, _: Caller) -> Got<Box<dyn Blueprints + Send>> {
        match &self.blueprints {
            Some(b) => Ok(Box::new(b.clone())),
            None => Err(self.fail.clone()),
        }
    }
    fn blueprint_writes(&self, _: &Actor, _: Caller) -> Got<Box<dyn BlueprintWrites + Send>> {
        Err(self.fail.clone())
    }
    fn code_host(&self, _: &Actor, _: Caller) -> Got<Box<dyn CodeHost + Send>> {
        match &self.code_host {
            Some(c) => Ok(Box::new(c.clone())),
            None => Err(self.fail.clone()),
        }
    }
    fn code_host_writes(&self, _: &Actor, _: Caller) -> Got<Box<dyn CodeHostWrites + Send>> {
        Err(self.fail.clone())
    }
    fn identity(&self, _: &Actor, _: Caller) -> Got<Box<dyn Identity + Send>> {
        Err(self.fail.clone())
    }
    fn board(&self, _: &Actor, _: Caller) -> Got<Box<dyn Board + Send>> {
        Err(self.fail.clone())
    }
    fn meter(&self) -> Vec<MeterRow> {
        vec![]
    }
}

const CALLER: Caller = Caller::new("test.join");

#[test]
fn a_split_setup_takes_each_handle_from_its_realms_site() {
    let split = Setup::Split(Split {
        code_host: Arc::new(StubSite::failing(Realm::CodeHost, Outcome::NotFound)),
        tickets: Arc::new(StubSite::failing(Realm::Tickets, Outcome::Forbidden)),
        links: Arc::new(SameItem),
    });
    let by = Actor::Ours(Scope::Read);
    let forbidden = Some(Outcome::Forbidden);
    let not_found = Some(Outcome::NotFound);
    assert_eq!(split.tickets(&by, CALLER).err(), forbidden);
    assert_eq!(split.ticket_writes(&by, CALLER).err(), forbidden);
    assert_eq!(split.blueprints(&by, CALLER).err(), not_found);
    assert_eq!(split.blueprint_writes(&by, CALLER).err(), not_found);
    assert_eq!(split.code_host(&by, CALLER).err(), not_found);
    assert_eq!(split.code_host_writes(&by, CALLER).err(), not_found);
    assert_eq!(split.identity(Realm::Tickets, &by, CALLER).err(), forbidden);
    assert_eq!(
        split.identity(Realm::CodeHost, &by, CALLER).err(),
        not_found
    );
    assert_eq!(split.board(&by, CALLER).err(), forbidden);

    let one = Setup::One(Arc::new(StubSite::failing(
        Realm::CodeHost,
        Outcome::NotFound,
    )));
    assert_eq!(one.board(&by, CALLER).err(), not_found, "the site's own");
    assert_eq!(
        one.identity(Realm::Tickets, &by, CALLER).err(),
        Some(unsupported()),
        "a realm the site does not serve"
    );
}

#[test]
fn a_split_setups_board_is_a_join_over_both_sites() {
    let (t, b, c, l) = handles();
    let tickets_site = StubSite {
        tickets: Some(t),
        ..StubSite::failing(Realm::Tickets, Outcome::Forbidden)
    };
    let code_host_site = StubSite {
        blueprints: Some(b),
        code_host: Some(c),
        ..StubSite::failing(Realm::CodeHost, Outcome::NotFound)
    };
    let setup = Setup::Split(Split {
        code_host: Arc::new(code_host_site),
        tickets: Arc::new(tickets_site),
        links: Arc::new(l),
    });
    let mut board = setup.board(&Actor::Ours(Scope::Read), CALLER).unwrap();
    let item = board.work_item(&key("PROJ-1"), &ask()).unwrap();
    assert_eq!(item.blueprints.len(), 2);
    assert_eq!(nos(&item.changes), [20, 22, 21]);
}

/// Every trait the spec uses as `dyn`, used as `dyn`: this builds only if
/// each is object safe.
#[test]
fn every_trait_is_object_safe() {
    let _: Option<&dyn Tickets> = None;
    let _: Option<&dyn TicketWrites> = None;
    let _: Option<&dyn Blueprints> = None;
    let _: Option<&dyn BlueprintWrites> = None;
    let _: Option<&dyn CodeHost> = None;
    let _: Option<&dyn CodeHostWrites> = None;
    let _: Option<&dyn Identity> = None;
    let _: Option<&dyn Board> = None;
    let _: Option<&dyn Site> = None;
    let _: Option<&dyn Links> = None;
    let _: Option<&dyn Keyring> = None;
    let _: Option<&dyn TokenStore> = None;
    let _: Option<&dyn Blobs> = None;
    let _: Option<&dyn Cites> = None;
    let _: Option<&dyn Writes> = None;
    let _: Option<&dyn Reports> = None;
}

fn shared<T: Send + Sync + ?Sized>() {}
fn sent<T: Send + ?Sized>() {}

#[test]
fn sites_and_the_callers_stores_are_shared_and_handles_are_sent() {
    shared::<dyn Site>();
    shared::<dyn Links>();
    shared::<dyn Keyring>();
    shared::<dyn TokenStore>();
    shared::<Setup>();
    sent::<Join>();
    sent::<Box<dyn Board + Send>>();
    sent::<TokenLock>();
}
