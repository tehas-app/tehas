//! `Join` over small stubs of the three read handles and the links: each
//! item answers alone, the links say which blueprints a ticket has, the
//! changes come open first, and a `Setup` takes each handle from the site of
//! its realm. And every trait the spec uses as `dyn` is object safe.

use std::num::{NonZeroU32, NonZeroU64};
use std::sync::{Arc, Mutex};

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
    {
        let mut ticket_digest = TicketDigest::new(
            key,
            if open {
                TicketState::Open
            } else {
                TicketState::Closed(CloseReason::Done)
            },
            At::new(0),
            At::new(0),
            Revision::new("r"),
            link(),
        );
        ticket_digest.title = "t".into();
        ticket_digest.priority = Priority::Normal;
        ticket_digest
    }
}

fn ticket(key: TicketKey, open: bool) -> Ticket {
    {
        let mut ticket = Ticket::new(digest(key, open));
        ticket.text = "text".into();
        ticket.history = Took::default();
        ticket
    }
}

fn comment(id: &str) -> Comment {
    {
        let mut comment = Comment::new(CommentId::new(id), At::new(0), Revision::new("r"), link());
        comment.body = id.into();
        comment
    }
}

fn blueprint(no: u64, comments: Vec<Comment>) -> Blueprint {
    {
        let mut blueprint = Blueprint::new(bp(no), At::new(0), Revision::new("r"), link());
        blueprint.title = "b".into();
        blueprint.body = "body".into();
        blueprint.open = true;
        blueprint.comments = {
            let mut took = Took::default();
            took.items = comments;
            took
        };
        blueprint
    }
}

fn change(no: u64, state: ChangeState, closes: Vec<Ref<BlueprintNo>>) -> ChangeDigest {
    {
        let mut change_digest = ChangeDigest::new(
            cn(no),
            state,
            Branch::new("work").unwrap(),
            CommitId::new("abc1234").unwrap(),
            Branch::new("main").unwrap(),
            CommitId::new("abc1230").unwrap(),
            At::new(0),
            At::new(0),
            link(),
        );
        change_digest.title = "c".into();
        change_digest.merge_state = Mergeable::Unknown;
        change_digest.closes = closes;
        change_digest
    }
}

fn merged() -> ChangeState {
    ChangeState::Merged {
        at: At::new(1),
        by: None,
        commit: CommitId::new("def5678").unwrap(),
    }
}

/// What the join asked of the tickets handle.
#[derive(Default)]
struct Asked {
    /// The history window of each `ticket` read.
    windows: Vec<Option<Window>>,
    /// How many `history` reads.
    history: usize,
    /// How many `notes` reads.
    notes: usize,
}

/// A tickets handle over a few tickets. A ticket in `unread` reads as
/// `Unread(Cut)`. Its list refuses an ask that is not for open tickets.
#[derive(Clone, Default)]
struct StubTickets {
    tickets: Vec<Ticket>,
    notes: Vec<(TicketKey, Comment)>,
    unread: Vec<TicketKey>,
    list_fails: bool,
    asked: Arc<Mutex<Asked>>,
}

fn reopened() -> TicketEvent {
    TicketEvent::new(Happened::Reopened)
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
        vec![{
            let mut report = Report::new(PortName::Tickets, Outcome::NotFound);
            report.calls = 2;
            report
        }]
    }
}

impl Cites for StubTickets {
    fn cite(&self, _: &Cite) -> String {
        String::new()
    }
}

impl Tickets for StubTickets {
    fn ticket(&mut self, key: &TicketKey, depth: Depth, history: Option<Window>) -> Got<Ticket> {
        let window = history.or(depth.history());
        self.asked.lock().unwrap().windows.push(window);
        let mut ticket = self.find(key)?;
        ticket.history = match window {
            None => {
                let mut took = Took::default();
                took.more = true;
                took
            }
            Some(w) => {
                let mut took = Took::default();
                took.items = vec![reopened()];
                took.more = matches!(w, Window::Last(_));
                took
            }
        };
        Ok(ticket)
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
    fn several_with_closers(&mut self, keys: &[TicketKey]) -> Vec<(TicketKey, Got<WithCloser>)> {
        keys.iter()
            .map(|k| (k.clone(), self.find(k).map(|t| (t.digest, t.closer))))
            .collect()
    }
    fn history(
        &mut self,
        _: &TicketKey,
        window: Window,
        _: Option<&[HappenedKind]>,
    ) -> Got<Took<TicketEvent>> {
        self.asked.lock().unwrap().history += 1;
        Ok({
            let mut took = Took::default();
            took.items = vec![reopened()];
            took.more = matches!(window, Window::Last(_));
            took
        })
    }
    fn notes(&mut self, key: &TicketKey, window: Window) -> Got<Took<Comment>> {
        self.asked.lock().unwrap().notes += 1;
        let mut mine: Vec<Comment> = self
            .notes
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, c)| c.clone())
            .collect();
        let n = match window {
            Window::Whole(b) if mine.len() > b.get() as usize => {
                return Err(Outcome::Unread(Unread::Cut))
            }
            Window::Whole(_) => mine.len(),
            Window::Last(n) => n.get() as usize,
        };
        let more = mine.len() > n;
        let items = mine.split_off(mine.len().saturating_sub(n));
        Ok({
            let mut took = Took::default();
            took.items = items;
            took.more = more;
            took
        })
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
                .map(|t| {
                    let mut closed_ticket = ClosedTicket::new(t.digest.clone(), At::new(1));
                    closed_ticket.part_count = 0;
                    closed_ticket
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
    fn marks(&mut self, _: Bound) -> Got<Whole<(Mark, MarkId)>> {
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
        vec![{
            let mut report = Report::new(PortName::Blueprints, Outcome::Unread(Unread::Cut));
            report.calls = 1;
            report
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
        vec![{
            let mut report = Report::new(PortName::Tickets, Outcome::NotFound);
            report.calls = 1;
            report
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
    fn changes(&mut self, filter: &ChangeFilter, bound: Bound) -> Got<Whole<ChangeDigest>> {
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
        let taken: Vec<ChangeDigest> = self
            .changes
            .iter()
            .filter(|c| state_ok(c) && closing_ok(c))
            .cloned()
            .collect();
        if taken.len() > bound.get() as usize {
            return Err(Outcome::Unread(Unread::Cut));
        }
        Ok(Whole::new(taken))
    }
    fn change(&mut self, _: ChangeNo, _: Bound) -> Got<Change> {
        Err(unsupported())
    }
    fn changed_files(&mut self, _: ChangeNo, bound: Bound) -> Got<Whole<ChangedFile>> {
        let files: Vec<ChangedFile> = ["src/lib.rs", "src/main.rs"]
            .into_iter()
            .map(|p| {
                let mut changed_file = ChangedFile::new(RepoPath::new(p).unwrap());
                changed_file.added = 1;
                changed_file.deleted = 0;
                changed_file
            })
            .collect();
        if files.len() > bound.get() as usize {
            return Err(Outcome::Unread(Unread::Cut));
        }
        Ok(Whole::new(files))
    }
    fn activity(&mut self, _: ChangeNo, _: Window) -> Got<Took<ChangeEvent>> {
        Ok({
            let mut took = Took::default();
            took.more = true;
            took
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
        asked: Arc::default(),
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

fn join_of(t: StubTickets, b: StubBlueprints, c: StubCodeHost, l: StubLinks) -> Join {
    Join {
        tickets: Box::new(t),
        blueprints: Box::new(b),
        code_host: Box::new(c),
        links: Arc::new(l),
    }
}

fn join() -> Join {
    let (t, b, c, l) = handles();
    join_of(t, b, c, l)
}

fn ask() -> BoardAsk {
    BoardAsk {
        depth: Depth::Last {
            notes: NonZeroU32::new(2).unwrap(),
        },
        history: Some(Window::Last(NonZeroU32::new(100).unwrap())),
        changes: Some((ChangeStates::All, Window::Whole(bound()))),
        filter: Filter::default(),
        control: Some(key("CTRL-1")),
    }
}

fn nos(items: &[ChangeDigest]) -> Vec<u64> {
    items.iter().map(|c| c.no.get()).collect()
}

fn n32(n: u32) -> NonZeroU32 {
    NonZeroU32::new(n).unwrap()
}

#[test]
fn open_work_answers_each_item_alone() {
    let work = join().open_work(&ask(), bound()).unwrap();
    assert_eq!(work.control.unwrap().unwrap().digest.key, key("CTRL-1"));
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
    assert_eq!(nos(&one.changes.items), [20, 22, 21], "open first");
    assert!(!one.changes.more);
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
    assert!(two.blueprints.is_empty() && two.changes.items.is_empty());
    assert!(!two.changes.more, "no blueprints, so no changes");

    assert_eq!(
        items[2].1,
        Err(Outcome::Unread(Unread::Inconsistent)),
        "a linked blueprint the answer leaves out fails its item"
    );
    assert_eq!(items[4].1, Err(Outcome::Unread(Unread::Cut)), "fails alone");
}

#[test]
fn a_whole_list_that_fails_fails_the_read_and_the_control_ticket_fails_alone() {
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
    let work = join().open_work(&unread_control, bound()).unwrap();
    assert_eq!(work.control, Some(Err(Outcome::Unread(Unread::Cut))));
    assert_eq!(work.items.items().len(), 5, "the list is still answered");
}

#[test]
fn a_cheap_ask_reads_no_changes_notes_or_history() {
    let cheap = BoardAsk {
        depth: Depth::Cut { notes: 0 },
        history: None,
        changes: None,
        filter: Filter::default(),
        control: None,
    };
    let item = join().work_item(&key("PROJ-1"), &cheap).unwrap();
    assert!(item.changes.items.is_empty() && item.changes.more);
    assert!(item.notes.items.is_empty() && item.notes.more);
    assert!(item.ticket.history.items.is_empty() && item.ticket.history.more);
}

#[test]
fn a_ticket_and_its_history_are_one_read_as_the_ask_or_its_depth_names() {
    let (t, b, c, l) = handles();
    let asked = t.asked.clone();
    let mut board = join_of(t, b, c, l);
    let item = board.work_item(&key("PROJ-1"), &ask()).unwrap();
    assert!(item.ticket.history.more, "the ask's window, Last");
    let by_depth = BoardAsk {
        depth: Depth::All,
        history: None,
        ..ask()
    };
    let item = board.work_item(&key("PROJ-1"), &by_depth).unwrap();
    assert!(!item.ticket.history.more, "the depth's window, Whole");
    let asked = asked.lock().unwrap();
    assert_eq!(asked.history, 0, "history is never read a second time");
    assert_eq!(
        asked.windows,
        [Some(Window::Last(n32(100))), Some(Window::Whole(ALL_MAX))]
    );
}

#[test]
fn notes_are_read_as_deep_as_asked_and_not_at_all_for_none() {
    let (t, b, c, l) = handles();
    let asked = t.asked.clone();
    let mut board = join_of(t, b, c, l);
    let none = BoardAsk {
        depth: Depth::Cut { notes: 0 },
        ..ask()
    };
    board.work_item(&key("PROJ-1"), &none).unwrap();
    assert_eq!(asked.lock().unwrap().notes, 0);
    let one = BoardAsk {
        depth: Depth::Cut { notes: 1 },
        ..ask()
    };
    let item = board.work_item(&key("PROJ-1"), &one).unwrap();
    assert_eq!(asked.lock().unwrap().notes, 1);
    let ids: Vec<&str> = item.notes.items.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["n3"]);
    assert!(item.notes.more);
}

fn ended_at(no: u64, at: i64, was_merged: bool) -> ChangeDigest {
    let state = if was_merged {
        ChangeState::Merged {
            at: At::new(at),
            by: None,
            commit: CommitId::new("def5678").unwrap(),
        }
    } else {
        ChangeState::Closed { at: At::new(at) }
    };
    change(no, state, vec![Ref::Here(bp(7))])
}

/// Two open changes and five ended ones on one blueprint.
fn busy() -> Vec<ChangeDigest> {
    let open = |no| change(no, ChangeState::Open, vec![Ref::Here(bp(7))]);
    vec![
        open(30),
        ended_at(31, 5, false),
        ended_at(32, 9, true),
        ended_at(33, 7, false),
        open(36),
        ended_at(34, 3, true),
        ended_at(35, 1, false),
    ]
}

fn item_changes(states: ChangeStates, window: Window) -> Got<Took<ChangeDigest>> {
    let (t, b, _, l) = handles();
    let mut board = join_of(t, b, StubCodeHost { changes: busy() }, l);
    let asked = BoardAsk {
        changes: Some((states, window)),
        ..ask()
    };
    board.work_item(&key("PROJ-1"), &asked).map(|i| i.changes)
}

#[test]
fn an_items_open_changes_alone() {
    let took = item_changes(ChangeStates::Open, Window::Last(n32(5))).unwrap();
    assert_eq!(nos(&took.items), [30, 36]);
    assert!(!took.more);
}

#[test]
fn an_items_changes_in_any_state_come_open_first_then_newest_ended_first() {
    let took = item_changes(ChangeStates::All, Window::Last(n32(9))).unwrap();
    assert_eq!(nos(&took.items), [30, 36, 32, 33, 31, 34, 35]);
    assert!(!took.more);
    let took = item_changes(ChangeStates::All, Window::Whole(bound())).unwrap();
    assert_eq!(nos(&took.items), [30, 36, 32, 33, 31, 34, 35]);
}

#[test]
fn ended_changes_past_the_window_are_cut_with_more() {
    let took = item_changes(ChangeStates::All, Window::Last(n32(5))).unwrap();
    assert_eq!(nos(&took.items), [30, 36, 32, 33, 31]);
    assert!(took.more);
}

#[test]
fn open_changes_past_the_window_make_the_item_unread() {
    let cut = Err(Outcome::Unread(Unread::Cut));
    assert_eq!(item_changes(ChangeStates::All, Window::Last(n32(1))), cut);
    assert_eq!(item_changes(ChangeStates::Open, Window::Last(n32(1))), cut);
    let three = Bound::new(3).unwrap();
    assert_eq!(item_changes(ChangeStates::All, Window::Whole(three)), cut);
}

#[test]
fn each_changes_files_have_their_own_bound() {
    let ask = |files| ChangeAsk {
        filter: ChangeFilter::default(),
        files: Some(Bound::new(files).unwrap()),
        closing: false,
        siblings: None,
        activity: None,
    };
    let cut = join().open_changes(&ask(1), bound());
    assert_eq!(cut, Err(Outcome::Unread(Unread::Cut)));
    assert!(join().open_changes(&ask(2), bound()).is_ok());
}

#[test]
fn a_closing_blueprint_not_found_is_left_out() {
    let (t, b, _, l) = handles();
    let missing = change(
        40,
        ChangeState::Open,
        vec![Ref::Here(bp(98)), Ref::Here(bp(7))],
    );
    let mut board = join_of(
        t,
        b,
        StubCodeHost {
            changes: vec![missing],
        },
        l,
    );
    let ask = ChangeAsk {
        filter: ChangeFilter::default(),
        files: None,
        closing: true,
        siblings: None,
        activity: None,
    };
    let items = board.open_changes(&ask, bound()).unwrap().into_items();
    let closing: Vec<u64> = items[0].closing.iter().map(|(b, _)| b.no.get()).collect();
    assert_eq!(closing, [7]);
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
        files: Some(bound()),
        closing: true,
        siblings: Some(bound()),
        activity: Some(Window::Last(NonZeroU32::new(50).unwrap())),
    };
    let items = join().open_changes(&ask, bound()).unwrap().into_items();
    let changes: Vec<u64> = items.iter().map(|i| i.change.no.get()).collect();
    assert_eq!(changes, [20, 22, 23], "open changes alone, by default");

    let twenty = &items[0];
    assert_eq!(twenty.files.as_ref().unwrap().items().len(), 2);
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
        files: None,
        closing: false,
        siblings: None,
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
            {
                let mut report = Report::new(PortName::Tickets, Outcome::NotFound);
                report.calls = 3;
                report
            },
            {
                let mut report = Report::new(PortName::Blueprints, Outcome::Unread(Unread::Cut));
                report.calls = 1;
                report
            },
        ]
    );
}

#[test]
fn one_item_reads_its_markers_from_the_blueprints_comments() {
    let n = NonZeroU64::new(7).unwrap();
    let item = {
        let mut work_item = WorkItem::new(ticket(TicketKey::Number(n), true));
        work_item.notes = Took::default();
        work_item.blueprints = vec![blueprint(7, vec![comment("on the item")])];
        work_item.changes = Took::default();
        work_item
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
    fn mark_name(&self, _: &Mark) -> String {
        String::new()
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
    /// The stub has no clock: every deadline has passed, so each handle
    /// asked answers the timeout.
    fn until(&self, _: At) -> Arc<dyn Site> {
        Arc::new(StubSite {
            realms: self.realms.clone(),
            fail: Outcome::Unavailable {
                why: Passing::Timeout,
                sent: Sent::No,
            },
            tickets: None,
            blueprints: None,
            code_host: None,
        })
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
    assert_eq!(nos(&item.changes.items), [20, 22, 21]);
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
