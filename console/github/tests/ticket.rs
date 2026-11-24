//! The tickets port's reads past the digest, as the adapter answers them:
//! one ticket to a depth, a history of every kind, a ticket's parts and the
//! list a filter takes. Recorded samples, in the ports' form, say what each
//! call must answer, the adapter answers from its recordings under
//! `tests/recorded/`,
//! and the laws of `tehas_ports::laws` check the same answers. The cases a
//! recording cannot hold, an answer that disagrees with itself among them,
//! run against a runner that answers from a list.

mod replay;

use replay::{adapter_of, PROJECT};
use serde::Deserialize;
use std::num::{NonZeroU32, NonZeroU64};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tehas_core::forge::GhRunner;
use tehas_github::GitHub;
use tehas_ports::laws;
use tehas_ports::{
    Act, Actor, Bound, ChangeNo, Closer, CommitId, Depth, Filter, Got, HappenedKind, Mark,
    MarkName, Outcome, Part, Passing, Pool, Refusal, Sent, TargetId, Ticket, TicketDigest,
    TicketEvent, TicketKey, Tickets, Took, Unread, Whole, Window, ALL_MAX,
};

/// A project of 102 open tickets, for a list of more than a page.
const BUSY: &str = "acme/busy";

fn adapter() -> GitHub {
    adapter_of(PROJECT).0
}

fn key(n: u64) -> TicketKey {
    TicketKey::Number(NonZeroU64::new(n).unwrap())
}

fn last(n: u32) -> Window {
    Window::Last(NonZeroU32::new(n).unwrap())
}

fn bound(n: u32) -> Bound {
    Bound::new(n).unwrap()
}

/// The samples of one file: the ports' own, under `console/ports/samples/`,
/// or, by a name with `own/` before it, those of this pull request, kept
/// beside the adapter's tests until the ports' owner takes them in.
fn samples<T: for<'de> Deserialize<'de>>(file: &str) -> Vec<T> {
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let at = match file.strip_prefix("own/") {
        Some(own) => here.join("tests/samples").join(own),
        None => here.join("../ports/samples/github").join(file),
    };
    let text = std::fs::read_to_string(&at).unwrap_or_else(|e| panic!("{}: {e}", at.display()));
    let mut all: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", at.display()));
    // The ports' samples still give REST's `x[bot]` for an app until their
    // owner takes the adapter's one spelling in.
    let mut left = vec![&mut all];
    while let Some(v) = left.pop() {
        match v {
            serde_json::Value::Object(o) => {
                if o.contains_key("realm") {
                    if let Some(serde_json::Value::String(login)) = o.get_mut("login") {
                        *login = tehas_core::accounts::folded(tehas_core::accounts::account(login));
                    }
                }
                left.extend(o.values_mut());
            }
            serde_json::Value::Array(a) => left.extend(a.iter_mut()),
            _ => {}
        }
    }
    let all: Vec<T> =
        serde_json::from_value(all).unwrap_or_else(|e| panic!("{}: {e}", at.display()));
    assert!(!all.is_empty(), "{file}");
    all
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OfTicket {
    op: String,
    actor: Actor,
    fixture: String,
    ticket: TicketKey,
    depth: Depth,
    history: Option<Window>,
    answer: Got<Ticket>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OfHistory {
    op: String,
    actor: Actor,
    fixture: String,
    ticket: TicketKey,
    window: Window,
    only: Option<Vec<HappenedKind>>,
    answer: Got<Took<TicketEvent>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OfParts {
    op: String,
    actor: Actor,
    fixture: String,
    ticket: TicketKey,
    answer: Got<Whole<Part>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OfList {
    op: String,
    actor: Actor,
    fixture: String,
    filter: Filter,
    bound: Bound,
    answer: Got<Whole<TicketDigest>>,
}

#[test]
fn a_ticket_answers_as_each_sample_says() {
    for s in samples::<OfTicket>("own/tickets-ticket.json") {
        assert_eq!(s.op, "Tickets::ticket");
        assert!(matches!(s.actor, Actor::Ours(_)), "{}", s.fixture);
        let got = adapter().ticket(&s.ticket, s.depth, s.history);
        assert_eq!(got, s.answer, "{:?} {:?}: {}", s.ticket, s.depth, s.fixture);
    }
}

#[test]
fn a_history_of_any_kind_answers_as_each_sample_says() {
    for s in samples::<OfHistory>("own/tickets-history-kinds.json") {
        assert_eq!(s.op, "Tickets::history");
        assert!(matches!(s.actor, Actor::Ours(_)), "{}", s.fixture);
        let mut gh = adapter();
        let got = Tickets::history(&mut gh, &s.ticket, s.window, s.only.as_deref());
        assert_eq!(
            got, s.answer,
            "{:?} {:?}: {}",
            s.ticket, s.window, s.fixture
        );
    }
}

#[test]
fn the_marks_history_answers_the_same_through_the_port() {
    // The marks alone stay the label events' read, as their samples say.
    for s in samples::<OfHistory>("tickets-history.json") {
        let mut gh = adapter();
        let got = Tickets::history(&mut gh, &s.ticket, s.window, s.only.as_deref());
        assert_eq!(
            got, s.answer,
            "{:?} {:?}: {}",
            s.ticket, s.window, s.fixture
        );
    }
}

#[test]
fn the_parts_answer_as_each_sample_says() {
    for s in samples::<OfParts>("own/tickets-parts.json") {
        assert_eq!(s.op, "Tickets::parts");
        assert!(matches!(s.actor, Actor::Ours(_)), "{}", s.fixture);
        assert_eq!(
            adapter().parts(&s.ticket),
            s.answer,
            "{:?}: {}",
            s.ticket,
            s.fixture
        );
    }
}

#[test]
fn a_list_answers_as_each_sample_says() {
    for s in samples::<OfList>("own/tickets-list.json") {
        assert_eq!(s.op, "Tickets::list");
        assert!(matches!(s.actor, Actor::Ours(_)), "{}", s.fixture);
        let got = adapter().list(&s.filter, s.bound);
        assert_eq!(got, s.answer, "{:?}: {}", s.filter, s.fixture);
    }
}

#[test]
fn an_unknown_history_is_every_events_unknown_ones() {
    // Ticket 7's unknown events: an assignee whose account is gone and a
    // target that is gone, as every kind's history reads them.
    let whole = Window::Whole(ALL_MAX);
    let unknown = Some(&[HappenedKind::Unknown][..]);
    let only = adapter().history(&key(7), whole, unknown).unwrap();
    let all = adapter().history(&key(7), whole, None).unwrap();
    let cut: Vec<TicketEvent> = all
        .items
        .into_iter()
        .filter(|e| e.what.kind() == HappenedKind::Unknown)
        .collect();
    assert_eq!(cut.len(), 2);
    assert_eq!(only.items, cut);
}

#[test]
fn one_account_has_one_login_in_every_read() {
    // REST spells an app `x[bot]`, GraphQL `x`; the ids are the same.
    let mut seen: std::collections::HashMap<String, String> = Default::default();
    let mut hold = |a: &tehas_ports::Account, read: &str| {
        let id = a.id.as_ref().unwrap().as_str().to_string();
        let login = a.login.as_str().to_string();
        let was = seen.entry(id.clone()).or_insert(login.clone());
        assert_eq!(*was, login, "account {id} in {read}");
    };
    let whole = Window::Whole(ALL_MAX);
    let marks = [HappenedKind::Marked, HappenedKind::Unmarked];
    for (read, only) in [("the marks", Some(&marks[..])), ("every kind", None)] {
        for e in adapter().history(&key(7), whole, only).unwrap().items {
            e.by.iter().for_each(|a| hold(a, read));
        }
    }
    for (_, d) in adapter().several(&[key(20), key(21), key(25)]) {
        d.unwrap().author.iter().for_each(|a| hold(a, "several"));
    }
    let t = adapter().ticket(&key(20), Depth::All, None).unwrap();
    t.assignees
        .iter()
        .for_each(|a| hold(a, "the ticket's assignees"));
    for d in adapter()
        .list(&Filter::default(), bound(50))
        .unwrap()
        .items()
    {
        d.author.iter().for_each(|a| hold(a, "the list"));
    }
    assert_eq!(seen.get("900").map(String::as_str), Some("tehas-bot"));
}

fn ok(law: laws::Law) {
    assert_eq!(law, Ok(()));
}

#[test]
fn the_history_keeps_its_laws() {
    // The ticket's own history and the history read agree, oldest first.
    ok(laws::history::oldest_first(&mut adapter(), &key(20)));
    // 103 events: a page and three more, read from either end.
    let mut gh = adapter();
    let mut read = |w| gh.history(&key(25), w, None);
    ok(laws::lists::last_n_is_never_cut(&mut read, 103));
    ok(laws::lists::more_is_exact(&mut read, 103));
    let mut gh = adapter();
    let mut read = |w| gh.ticket(&key(25), Depth::All, Some(w)).map(|t| t.history);
    ok(laws::lists::more_is_exact(&mut read, 103));
}

#[test]
fn a_list_past_its_bound_is_unread() {
    let all = Filter::default();
    ok(laws::lists::past_the_bound_is_unread(
        |b| adapter().list(&all, b),
        5,
    ));
    let open = Filter {
        open: Some(true),
        ..Filter::default()
    };
    ok(laws::lists::past_the_bound_is_unread(
        |b| adapter_of(BUSY).0.list(&open, b),
        102,
    ));
}

#[test]
fn taking_off_a_mark_that_is_not_on_is_done() {
    ok(laws::tickets::removing_an_absent_mark_is_done(
        &mut adapter(),
        &key(20),
        &Mark::Act(Act::Pause),
    ));
}

#[test]
fn a_history_is_paged_to_its_end_or_one_past_its_bound() {
    let whole = |b: Bound| {
        let (mut gh, asked) = adapter_of(PROJECT);
        let got = gh.history(&key(25), Window::Whole(b), None);
        (got.map(|t| (t.items.len(), t.more)), asked.get())
    };
    assert_eq!(whole(bound(103)), (Ok((103, false)), 2));
    assert_eq!(whole(bound(102)), (Err(Outcome::Unread(Unread::Cut)), 2));
    // At the page's own size: the page after it is what says there is more.
    assert_eq!(whole(bound(100)), (Err(Outcome::Unread(Unread::Cut)), 2));
    assert_eq!(whole(bound(99)), (Err(Outcome::Unread(Unread::Cut)), 1));

    let newest = |n: u32| {
        let (mut gh, asked) = adapter_of(PROJECT);
        let got = gh.history(&key(25), last(n), None).unwrap();
        (got.items.len(), got.more, asked.get())
    };
    // The newest page holds them, and says so alone.
    assert_eq!(newest(3), (3, true, 1));
    assert_eq!(newest(99), (99, true, 1));
    // A page full of them does not say whether one more is older.
    assert_eq!(newest(100), (100, true, 2));
    assert_eq!(newest(103), (103, false, 2));
    assert_eq!(newest(500), (103, false, 2));
}

#[test]
fn a_ticket_costs_the_issues_read_and_one_question_a_page() {
    let read = |depth, history| {
        let (mut gh, asked) = adapter_of(PROJECT);
        let t = gh.ticket(&key(25), depth, history).unwrap();
        (t.history.items.len(), t.history.more, asked.get())
    };
    assert_eq!(read(Depth::Cut { notes: 0 }, None), (0, true, 2));
    assert_eq!(read(Depth::All, None), (103, false, 3));
    let two = Depth::Last {
        notes: NonZeroU32::new(2).unwrap(),
    };
    assert_eq!(read(two, None), (2, true, 2));
    // The window given beside the depth is the one read.
    assert_eq!(read(Depth::All, Some(last(1))), (1, true, 2));
}

#[test]
fn a_name_no_mark_can_have_is_kept_once_to_report() {
    let mut gh = adapter();
    gh.ticket(&key(20), Depth::All, None).unwrap();
    gh.list(&Filter::default(), bound(50)).unwrap();
    assert_eq!(gh.take_left_out(), ["not a label?"]);
    assert!(gh.take_left_out().is_empty());
}

#[test]
fn a_list_of_more_than_a_page_is_whole_and_oldest_first() {
    let open = Filter {
        open: Some(true),
        ..Filter::default()
    };
    let (mut gh, asked) = adapter_of(BUSY);
    let all = gh.list(&open, bound(102)).unwrap();
    assert_eq!(asked.get(), 2);
    let keys: Vec<TicketKey> = all.items().iter().map(|d| d.key.clone()).collect();
    assert_eq!(keys, (100..202).map(key).collect::<Vec<_>>());
    // One under: the second page is what shows it.
    let (mut gh, asked) = adapter_of(BUSY);
    assert_eq!(
        gh.list(&open, bound(101)),
        Err(Outcome::Unread(Unread::Cut))
    );
    assert_eq!(asked.get(), 2);
    // Under the first page: no second page is asked.
    let (mut gh, asked) = adapter_of(BUSY);
    assert_eq!(gh.list(&open, bound(99)), Err(Outcome::Unread(Unread::Cut)));
    assert_eq!(asked.get(), 1);
}

type Asked = Arc<Mutex<Vec<String>>>;

/// Answers each call from the list in turn; a call past it fails.
struct Stub {
    answers: Vec<Result<String, String>>,
    asked: Asked,
}

impl GhRunner for Stub {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        let mut asked = self.asked.lock().unwrap();
        asked.push(args.join(" "));
        self.answers
            .get(asked.len() - 1)
            .cloned()
            .unwrap_or_else(|| Err("no fixture".to_string()))
    }
}

fn stub(answers: Vec<Result<String, &str>>) -> (GitHub, Asked) {
    let asked = Asked::default();
    let stub = Stub {
        answers: answers
            .into_iter()
            .map(|a| a.map_err(str::to_string))
            .collect(),
        asked: Arc::clone(&asked),
    };
    let gh = GitHub::new(PROJECT, Box::new(stub)).on_clock(|| 1_767_225_600);
    (gh, asked)
}

/// One issue's answer to a question, `inner` its fields.
fn issue(inner: &str) -> Result<String, &'static str> {
    Ok(format!(
        r#"{{"data":{{"repository":{{"issue":{{"number":7,{inner}}}}}}}}}"#
    ))
}

fn labeled(kind: &str, at: &str) -> String {
    format!(
        r#"{{"__typename":"{kind}","createdAt":"{at}","actor":{{"__typename":"User","login":"alice","databaseId":101}},"label":{{"name":"bug"}}}}"#
    )
}

fn events(info: &str, nodes: &[String]) -> Result<String, &'static str> {
    issue(&format!(
        r#""milestone":null,"events":{{"pageInfo":{{{info}}},"nodes":[{}]}}"#,
        nodes.join(",")
    ))
}

const INCONSISTENT: Outcome = Outcome::Unread(Unread::Inconsistent);
const MALFORMED: Outcome = Outcome::Unread(Unread::Malformed);
const SERVER: Outcome = Outcome::Unavailable {
    why: Passing::Server,
    sent: Sent::Maybe,
};

fn whole_history(answers: Vec<Result<String, &str>>) -> (Got<usize>, usize) {
    let (mut gh, asked) = stub(answers);
    let got = gh.history(&key(7), Window::Whole(ALL_MAX), None);
    let calls = asked.lock().unwrap().len();
    (got.map(|t| t.items.len()), calls)
}

#[test]
fn pages_that_disagree_are_inconsistent_and_never_a_part_as_the_whole() {
    let one = [labeled("LabeledEvent", "2026-01-01T00:00:00Z")];
    let more = r#""hasNextPage":true,"endCursor":"A""#;
    let end = r#""hasNextPage":false,"endCursor":"B""#;
    // The pages follow their cursors to the end.
    let (mut gh, asked) = stub(vec![events(more, &one), events(end, &one)]);
    assert_eq!(
        gh.history(&key(7), Window::Whole(ALL_MAX), None)
            .map(|t| t.items.len()),
        Ok(2)
    );
    let asked = asked.lock().unwrap();
    assert!(asked[0].starts_with("api graphql -F owner=acme -F name=widget -F number=7 -f query="));
    assert!(!asked[0].contains("cursor="));
    assert!(asked[1].ends_with(" -f cursor=A"), "{}", asked[1]);
    // A cursor handed back twice.
    assert_eq!(
        whole_history(vec![
            events(more, &one),
            events(more, &one),
            events(end, &one)
        ]),
        (Err(INCONSISTENT), 2)
    );
    // More, with no cursor to read it from.
    assert_eq!(
        whole_history(vec![events(r#""hasNextPage":true,"endCursor":null"#, &one)]),
        (Err(INCONSISTENT), 1)
    );
    // A page that does not say whether there is more.
    assert_eq!(
        whole_history(vec![events(r#""endCursor":"A""#, &one)]),
        (Err(INCONSISTENT), 1)
    );
    assert_eq!(
        whole_history(vec![issue(r#""milestone":null,"events":{"nodes":[]}"#)]),
        (Err(INCONSISTENT), 1)
    );
    // A second page that fails fails the read: no first page as the whole.
    assert_eq!(
        whole_history(vec![events(more, &one), Err("HTTP 502")]),
        (Err(SERVER), 2)
    );
    assert_eq!(
        whole_history(vec![
            events(more, &one),
            Ok("{\"data\":{\"repo".to_string())
        ]),
        (Err(MALFORMED), 2)
    );
}

#[test]
fn an_answer_that_is_no_history_is_malformed() {
    let end = r#""hasNextPage":false,"endCursor":null"#;
    for answer in [
        Ok("not json".to_string()),
        Ok(String::new()),
        Ok(r#"{"data":{"repository":null}}"#.to_string()),
        Ok(r#"{"data":{"repository":{"issue":null}}}"#.to_string()),
        issue(r#""milestone":null"#),
        issue(r#""milestone":null,"events":{"pageInfo":{},"nodes":{}}"#),
        // A stamp that is no time, and one that is no text.
        events(end, &[labeled("LabeledEvent", "yesterday")]),
        events(end, &[labeled("LabeledEvent", "2026-01-01T00:00:00Z").replace("\"2026-01-01T00:00:00Z\"", "17")]),
        // A label event with no label, a title change with no titles.
        events(end, &[r#"{"__typename":"LabeledEvent","createdAt":null,"actor":null}"#.to_string()]),
        events(end, &[r#"{"__typename":"RenamedTitleEvent","createdAt":null,"actor":null}"#.to_string()]),
        // A close that does not say what closed it, not even nothing.
        events(end, &[r#"{"__typename":"ClosedEvent","createdAt":null,"actor":null,"stateReason":"COMPLETED"}"#.to_string()]),
        // A close by a change with no number, and by a commit that is none.
        events(end, &[r#"{"__typename":"ClosedEvent","closer":{"__typename":"PullRequest","repository":{"nameWithOwner":"acme/widget"},"merged":true}}"#.to_string()]),
        events(end, &[r#"{"__typename":"ClosedEvent","closer":{"__typename":"Commit","oid":"zz"}}"#.to_string()]),
    ] {
        let shown = format!("{answer:?}");
        assert_eq!(whole_history(vec![answer]), (Err(MALFORMED), 1), "{shown}");
    }
}

#[test]
fn an_event_is_read_in_the_ports_words() {
    let end = r#""hasNextPage":false,"endCursor":null"#;
    let nodes = [
        // No stamp and no actor: a fact, read as the oldest.
        r#"{"__typename":"ReopenedEvent","createdAt":null,"actor":null}"#.to_string(),
        r#"{"__typename":"ClosedEvent","createdAt":"2026-01-01T00:00:03Z","actor":{"__typename":"User","login":"Alice","databaseId":101},"stateReason":"NOT_PLANNED","closer":null}"#.to_string(),
        r#"{"__typename":"ClosedEvent","createdAt":"2026-01-01T00:00:04Z","actor":null,"stateReason":"COMPLETED","closer":{"__typename":"Commit","oid":"0123456789abcdef0123456789abcdef01234567"}}"#.to_string(),
        r#"{"__typename":"ClosedEvent","createdAt":"2026-01-01T00:00:05Z","actor":null,"stateReason":"COMPLETED","closer":{"__typename":"PullRequest","number":4,"merged":false,"repository":{"nameWithOwner":"Acme/Widget"}}}"#.to_string(),
        r#"{"__typename":"ClosedEvent","createdAt":"2026-01-01T00:00:06Z","actor":null,"stateReason":"DUPLICATE","closer":{"__typename":"PullRequest","number":4,"merged":true,"repository":{"nameWithOwner":"octo/tools"}}}"#.to_string(),
        r#"{"__typename":"IssueTypeRemovedEvent","createdAt":"2026-01-01T00:00:07Z","actor":null}"#.to_string(),
        r#"{"__typename":"UnassignedEvent","createdAt":"2026-01-01T00:00:08Z","actor":null,"assignee":null}"#.to_string(),
        r#"{"__typename":"BlockedByRemovedEvent","createdAt":"2026-01-01T00:00:09Z","actor":null,"blockingIssue":{"number":3,"repository":{"nameWithOwner":"octo/tools"}}}"#.to_string(),
    ];
    let (mut gh, _) = stub(vec![events(end, &nodes)]);
    let got = gh.history(&key(7), Window::Whole(ALL_MAX), None).unwrap();
    let shown: Vec<String> = got
        .items
        .iter()
        .map(|e| serde_json::to_string(&e.what).unwrap())
        .collect();
    assert_eq!(
        shown,
        [
            r#"{"type":"Reopened"}"#,
            r#"{"type":"Closed","value":{"reason":"NotPlanned","closer":{"Account":{"realm":"CodeHost","login":"alice","id":"101","kind":"Person"}}}}"#,
            r#"{"type":"Closed","value":{"reason":"Done","closer":{"Commit":"0123456789abcdef0123456789abcdef01234567"}}}"#,
            r#"{"type":"Closed","value":{"reason":"Done","closer":{"Change":{"no":4,"merged":false}}}}"#,
            // A duplicate's original is not asked, and a change of another
            // project has no number here.
            r#"{"type":"Closed","value":{"reason":"NotPlanned","closer":null}}"#,
            r#"{"type":"Kind","value":null}"#,
            // An account that is gone cannot be named.
            r#"{"type":"Unknown"}"#,
            r#"{"type":"BlockerRemoved","value":{"Elsewhere":{"project":"octo/tools","item":"3"}}}"#,
        ]
    );
    assert_eq!(got.items[0].at, None);
    assert_eq!(got.items[0].by, None);
    assert_eq!(
        got.items[2].what,
        tehas_ports::Happened::Closed {
            reason: tehas_ports::CloseReason::Done,
            closer: Some(Closer::Commit(
                CommitId::new("0123456789abcdef0123456789abcdef01234567").unwrap()
            )),
        }
    );
    assert!(matches!(
        &got.items[3].what,
        tehas_ports::Happened::Closed { closer: Some(Closer::Change { no, merged: false }), .. }
            if *no == ChangeNo::new(4).unwrap()
    ));
}

#[test]
fn the_newest_n_are_read_back_until_no_event_of_their_second_can_be_older() {
    let back =
        |more: bool, cursor: &str| format!(r#""hasPreviousPage":{more},"startCursor":"{cursor}""#);
    let t0 = "2026-01-01T00:00:00Z";
    let t1 = "2026-01-01T00:00:01Z";
    // In one second the site lists the removal before the addition; the
    // addition sorts first, so the newest one is the removal.
    let newest = [labeled("UnlabeledEvent", t1), labeled("LabeledEvent", t1)];
    let older = [labeled("LabeledEvent", t0)];
    let (mut gh, asked) = stub(vec![
        events(&back(true, "B"), &newest),
        events(&back(false, "A"), &older),
    ]);
    let got = gh.history(&key(7), last(1), None).unwrap();
    assert_eq!(asked.lock().unwrap().len(), 2);
    assert!(asked.lock().unwrap()[1].ends_with(" -f cursor=B"));
    assert!(got.more);
    assert!(matches!(
        got.items[..],
        [TicketEvent {
            what: tehas_ports::Happened::Unmarked(_),
            ..
        }]
    ));
    // Two seconds on the newest page: it is sure alone.
    let apart = [labeled("LabeledEvent", t0), labeled("UnlabeledEvent", t1)];
    let (mut gh, asked) = stub(vec![events(&back(true, "B"), &apart)]);
    let got = gh.history(&key(7), last(1), None).unwrap();
    assert_eq!(
        (got.items.len(), got.more, asked.lock().unwrap().len()),
        (1, true, 1)
    );
}

#[test]
fn a_question_gh_gives_no_reason_for_is_settled_by_the_issues_read() {
    let whole = Window::Whole(ALL_MAX);
    // Not there.
    let (mut gh, asked) = stub(vec![Err(": gh exited 1"), Err(": HTTP 404")]);
    assert_eq!(gh.history(&key(7), whole, None), Err(Outcome::NotFound));
    assert_eq!(asked.lock().unwrap()[1], "api repos/acme/widget/issues/7");
    // A change's number.
    let pull = r#"{"number":7,"pull_request":{"url":"x"}}"#.to_string();
    let (mut gh, _) = stub(vec![Err(": gh exited 1"), Ok(pull)]);
    assert_eq!(gh.parts(&key(7)), Err(Outcome::NotFound));
    // There: the question's own failure stands.
    let (mut gh, _) = stub(vec![
        Err(": gh exited 1"),
        Ok(r#"{"number":7}"#.to_string()),
    ]);
    assert_eq!(gh.history(&key(7), whole, None), Err(SERVER));
    // The settling read fails too: the question's failure stands.
    let (mut gh, _) = stub(vec![Err(": gh exited 1"), Err(": HTTP 502")]);
    assert_eq!(gh.parts(&key(7)), Err(SERVER));
    // A failure with a reason is its outcome, with no second call.
    for (why, want) in [
        (": HTTP 403", Outcome::Forbidden),
        (": HTTP 401", Outcome::SignedOut),
        (
            ": secondary rate limit",
            Outcome::Spent {
                pool: Pool::Calls,
                until: tehas_ports::At::new(1_767_225_600),
            },
        ),
        (
            "gh ran out of its 30s bound",
            Outcome::Unavailable {
                why: Passing::Timeout,
                sent: Sent::Maybe,
            },
        ),
        (
            ": connection refused",
            Outcome::Unavailable {
                why: Passing::Unreachable,
                sent: Sent::No,
            },
        ),
    ] {
        let (mut gh, asked) = stub(vec![Err(why)]);
        assert_eq!(gh.history(&key(7), whole, None), Err(want.clone()), "{why}");
        assert_eq!(asked.lock().unwrap().len(), 1, "{why}");
        let (mut gh, _) = stub(vec![Err(why)]);
        assert_eq!(gh.parts(&key(7)), Err(want.clone()), "{why}");
        let (mut gh, _) = stub(vec![Err(why)]);
        assert_eq!(gh.list(&Filter::default(), bound(5)), Err(want), "{why}");
    }
    // A key that is no number names no ticket here: no call.
    let (mut gh, asked) = stub(vec![]);
    let named = TicketKey::Key(tehas_ports::KeyText::new("PROJ-1").unwrap());
    assert_eq!(gh.parts(&named), Err(Outcome::Refused(Refusal::Invalid)));
    assert_eq!(
        gh.ticket(&named, Depth::All, None).map(|_| ()),
        Err(Outcome::Refused(Refusal::Invalid))
    );
    assert!(asked.lock().unwrap().is_empty());
}

#[test]
fn a_kind_github_keeps_no_event_of_asks_no_question_and_settles_the_issue() {
    let priority = Some(&[HappenedKind::Priority][..]);
    let whole = Window::Whole(ALL_MAX);
    // There: nothing happened of that kind, from the issue's read alone.
    let (mut gh, asked) = stub(vec![Ok(REST.to_string())]);
    assert_eq!(gh.history(&key(7), whole, priority), Ok(Took::default()));
    assert_eq!(*asked.lock().unwrap(), ["api repos/acme/widget/issues/7"]);
    let (mut gh, _) = stub(vec![Ok(REST.to_string())]);
    assert_eq!(gh.history(&key(7), last(3), priority), Ok(Took::default()));
    // Not there, a change's number, a failure, an answer that is no issue.
    let pull = r#"{"number":7,"pull_request":{"url":"x"}}"#.to_string();
    for (answer, want) in [
        (Err(": HTTP 404"), Outcome::NotFound),
        (Ok(pull), Outcome::NotFound),
        (Err(": HTTP 403"), Outcome::Forbidden),
        (Err("HTTP 502"), SERVER),
        (Ok(r#"{"number":7}"#.to_string()), MALFORMED),
    ] {
        let shown = format!("{answer:?}");
        let (mut gh, asked) = stub(vec![answer]);
        assert_eq!(gh.history(&key(7), whole, priority), Err(want), "{shown}");
        assert_eq!(asked.lock().unwrap().len(), 1, "{shown}");
    }
}

fn sub(n: u64) -> String {
    format!(r#"{{"number":{n},"state":"OPEN","repository":{{"nameWithOwner":"acme/widget"}}}}"#)
}

fn subs(total: u64, info: &str, nodes: &[String]) -> Result<String, &'static str> {
    issue(&format!(
        r#""subIssues":{{"totalCount":{total},"pageInfo":{{{info}}},"nodes":[{}]}}"#,
        nodes.join(",")
    ))
}

#[test]
fn the_parts_are_paged_whole_and_counted() {
    let more = r#""hasNextPage":true,"endCursor":"P""#;
    let end = r#""hasNextPage":false,"endCursor":"Q""#;
    let (mut gh, asked) = stub(vec![
        subs(3, more, &[sub(1), sub(2)]),
        subs(3, end, &[sub(3)]),
    ]);
    let got = gh.parts(&key(7)).unwrap();
    assert_eq!(got.items().len(), 3);
    assert!(asked.lock().unwrap()[1].ends_with(" -f cursor=P"));
    // A count that does not add up, a part with no number, an empty entry,
    // a part that does not say whether it is open.
    let parts = |answers| stub(answers).0.parts(&key(7)).map(|w| w.items().len());
    assert_eq!(parts(vec![subs(4, end, &[sub(1)])]), Err(INCONSISTENT));
    let nameless = r#"{"state":"OPEN","repository":{"nameWithOwner":"acme/widget"}}"#.to_string();
    assert_eq!(parts(vec![subs(1, end, &[nameless])]), Err(INCONSISTENT));
    assert_eq!(
        parts(vec![subs(1, end, &["null".to_string()])]),
        Err(INCONSISTENT)
    );
    assert_eq!(
        parts(vec![subs(1, end, &[sub(1).replace("OPEN", "DRAFT")])]),
        Err(MALFORMED)
    );
    assert_eq!(parts(vec![issue(r#""title":"x""#)]), Err(MALFORMED));
    assert_eq!(
        parts(vec![subs(3, more, &[sub(1)]), Err("HTTP 502")]),
        Err(SERVER)
    );
}

const REST: &str = r#"{"number":7,"title":"T","state":"closed","state_reason":"completed","labels":[],
"user":{"login":"alice","id":101,"type":"User"},"assignees":[],"body":null,
"created_at":"2026-01-02T03:04:05Z","updated_at":"2026-01-03T00:00:00Z","closed_at":"2026-01-03T00:00:00Z",
"html_url":"https://github.com/acme/widget/issues/7"}"#;

fn beside(closes: &str, blocked: &str, mentions: &str) -> Result<String, &'static str> {
    issue(&format!(
        r#""closes":{{"nodes":[{closes}]}},"blockedBy":{blocked},"subIssues":{{"totalCount":0,"pageInfo":{{"hasNextPage":false,"endCursor":null}},"nodes":[]}},"mentions":{mentions}"#
    ))
}

const NO_BLOCKERS: &str = r#"{"totalCount":0,"nodes":[]}"#;
const NO_MENTIONS: &str = r#"{"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[]}"#;

fn cut_ticket(answers: Vec<Result<String, &str>>) -> (Got<Ticket>, usize) {
    let (mut gh, asked) = stub(answers);
    let got = gh.ticket(&key(7), Depth::Cut { notes: 0 }, None);
    let calls = asked.lock().unwrap().len();
    (got, calls)
}

#[test]
fn what_stands_beside_a_ticket_is_whole_or_the_ticket_is_unread() {
    // Closed by hand: the account of its newest close.
    let by_hand = r#"{"__typename":"ClosedEvent","createdAt":"2026-01-03T00:00:00Z","actor":{"__typename":"User","login":"Bob","databaseId":102},"stateReason":"COMPLETED","closer":null}"#;
    let (got, calls) = cut_ticket(vec![
        Ok(REST.to_string()),
        beside(by_hand, NO_BLOCKERS, NO_MENTIONS),
    ]);
    let t = got.unwrap();
    assert_eq!(calls, 2);
    assert!(matches!(&t.closer, Some(Closer::Account(a)) if a.login.as_str() == "bob"));
    assert_eq!((t.text.as_str(), t.markers.len()), ("", 0));
    // More blockers than the answer holds.
    let cut = r#"{"totalCount":2,"nodes":[{"number":1,"state":"OPEN","repository":{"nameWithOwner":"acme/widget"}}]}"#;
    assert_eq!(
        cut_ticket(vec![Ok(REST.to_string()), beside("", cut, NO_MENTIONS)]).0,
        Err(Outcome::Unread(Unread::Cut))
    );
    // The changes that name it run to a second page, which is asked.
    let pull = |n: u64| {
        format!(
            r#"{{"__typename":"CrossReferencedEvent","source":{{"__typename":"PullRequest","number":{n},"repository":{{"nameWithOwner":"acme/widget"}}}}}}"#
        )
    };
    let page = |more: bool, cursor: &str, n: u64| {
        format!(
            r#"{{"pageInfo":{{"hasNextPage":{more},"endCursor":"{cursor}"}},"nodes":[{}]}}"#,
            pull(n)
        )
    };
    let (mut gh, asked) = stub(vec![
        Ok(REST.to_string()),
        beside("", NO_BLOCKERS, &page(true, "M", 31)),
        issue(&format!(r#""mentions":{}"#, page(false, "N", 32))),
    ]);
    let t = gh.ticket(&key(7), Depth::Cut { notes: 0 }, None).unwrap();
    assert_eq!(t.mentioned_by.len(), 2);
    assert!(asked.lock().unwrap()[2].ends_with(" -f cursor=M"));
    // That page failing fails the ticket.
    assert_eq!(
        cut_ticket(vec![
            Ok(REST.to_string()),
            beside("", NO_BLOCKERS, &page(true, "M", 31)),
            Err("HTTP 502"),
        ])
        .0,
        Err(SERVER)
    );
    // The question failing with no reason, after the issue was read, is the
    // site's failure: the issue is there.
    assert_eq!(
        cut_ticket(vec![Ok(REST.to_string()), Err(": gh exited 1")]),
        (Err(SERVER), 2)
    );
    // A part of the answer that is not there.
    assert_eq!(
        cut_ticket(vec![
            Ok(REST.to_string()),
            issue(r#""closes":{"nodes":[]}"#)
        ])
        .0,
        Err(MALFORMED)
    );
    // An issue's read without its text, its assignees or its close.
    for field in [
        r#""body":null,"#,
        r#""assignees":[],"#,
        r#""closed_at":"2026-01-03T00:00:00Z","#,
    ] {
        assert!(REST.contains(field), "{field}");
        assert_eq!(
            cut_ticket(vec![
                Ok(REST.replace(field, "")),
                beside("", NO_BLOCKERS, NO_MENTIONS)
            ])
            .0,
            Err(MALFORMED),
            "{field}"
        );
    }
    // A close time that is no time.
    let odd = REST.replace(
        r#""closed_at":"2026-01-03T00:00:00Z""#,
        r#""closed_at":"soon""#,
    );
    assert_eq!(
        cut_ticket(vec![Ok(odd), beside("", NO_BLOCKERS, NO_MENTIONS)]).0,
        Err(MALFORMED)
    );
}

fn node(n: u64, labels: &str, total: u64) -> String {
    format!(
        r#"{{"number":{n},"title":"T{n}","body":"","state":"OPEN","stateReason":null,"url":"https://github.com/acme/widget/issues/{n}","createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z","author":null,"issueType":null,"milestone":null,"labels":{{"totalCount":{total},"nodes":[{labels}]}}}}"#
    )
}

fn page(info: &str, nodes: &[String]) -> Result<String, &'static str> {
    Ok(format!(
        r#"{{"data":{{"repository":{{"issues":{{"pageInfo":{{{info}}},"nodes":[{}]}}}}}}}}"#,
        nodes.join(",")
    ))
}

#[test]
fn a_list_asks_githubs_filter_and_holds_every_ticket_to_the_callers() {
    let end = r#""hasNextPage":false,"endCursor":null"#;
    let more = r#""hasNextPage":true,"endCursor":"L""#;
    let bug = r#"{"name":"bug"}"#;
    let filter = Filter {
        open: Some(true),
        carrying: vec![
            Mark::Project(MarkName::new("bug").unwrap()),
            Mark::Act(Act::Pause),
        ],
        carrying_any: vec![],
        kind: None,
        target: Some(TargetId::new("3")),
    };
    // GitHub's filter narrows; a ticket it hands back that the caller's
    // filter does not take is left out.
    let (mut gh, asked) = stub(vec![page(end, &[node(1, bug, 1)])]);
    assert_eq!(gh.list(&filter, bound(5)).map(|w| w.items().len()), Ok(0));
    let asked = asked.lock().unwrap();
    assert!(asked[0].starts_with("api graphql -F owner=acme -F name=widget -f query="));
    assert!(
        asked[0].contains(
            r#"filterBy: {states: [OPEN], labels: ["bug", "tehas:pause"], milestoneNumber: "3"})"#
        ),
        "{}",
        asked[0]
    );
    assert!(asked[0].contains("orderBy: {field: CREATED_AT, direction: ASC}"));
    let (mut gh, asked) = stub(vec![page(end, &[])]);
    assert_eq!(
        gh.list(&Filter::default(), bound(5))
            .map(|w| w.items().len()),
        Ok(0)
    );
    assert!(!asked.lock().unwrap()[0].contains("filterBy"));

    let list = |answers| {
        stub(answers)
            .0
            .list(&Filter::default(), bound(5))
            .map(|w| w.items().len())
    };
    // The pages follow their cursors; a ticket handed back twice, a cursor
    // twice, an entry with no number or none at all.
    assert_eq!(
        list(vec![
            page(more, &[node(1, "", 0)]),
            page(end, &[node(2, "", 0)])
        ]),
        Ok(2)
    );
    assert_eq!(
        list(vec![
            page(more, &[node(1, "", 0)]),
            page(end, &[node(1, "", 0)])
        ]),
        Err(INCONSISTENT)
    );
    assert_eq!(
        list(vec![
            page(more, &[node(1, "", 0)]),
            page(more, &[node(2, "", 0)])
        ]),
        Err(INCONSISTENT)
    );
    assert_eq!(
        list(vec![page(
            end,
            &[node(1, "", 0).replace(r#""number":1,"#, "")]
        )]),
        Err(INCONSISTENT)
    );
    assert_eq!(
        list(vec![page(end, &["null".to_string()])]),
        Err(INCONSISTENT)
    );
    assert_eq!(
        list(vec![page(r#""endCursor":null"#, &[])]),
        Err(INCONSISTENT)
    );
    // A ticket with more labels than its page of them holds.
    assert_eq!(
        list(vec![page(end, &[node(1, bug, 101)])]),
        Err(Outcome::Unread(Unread::Cut))
    );
    // A second page that fails, an answer that is no list.
    assert_eq!(
        list(vec![page(more, &[node(1, "", 0)]), Err("HTTP 502")]),
        Err(SERVER)
    );
    assert_eq!(list(vec![Ok("[]".to_string())]), Err(MALFORMED));
    assert_eq!(
        list(vec![Ok(r#"{"data":{"repository":null}}"#.to_string())]),
        Err(MALFORMED)
    );
    assert_eq!(
        list(vec![page(end, &[node(1, "", 0).replace("OPEN", "DRAFT")])]),
        Err(MALFORMED)
    );
}

#[test]
fn a_filter_githubs_list_cannot_take_is_refused_before_any_request() {
    let invalid = Err(Outcome::Refused(Refusal::Invalid));
    for filter in [
        Filter {
            carrying: vec![Mark::Unknown],
            ..Filter::default()
        },
        Filter {
            target: Some(TargetId::new("v2")),
            ..Filter::default()
        },
        Filter {
            target: Some(TargetId::new("")),
            ..Filter::default()
        },
    ] {
        let (mut gh, asked) = stub(vec![]);
        assert_eq!(
            gh.list(&filter, bound(5)).map(|_| ()),
            invalid,
            "{filter:?}"
        );
        assert!(asked.lock().unwrap().is_empty());
    }
}

#[test]
fn a_changes_number_is_no_ticket_to_any_read() {
    // GitHub answers a pull request at an issue's path: it is no ticket.
    let mut gh = adapter();
    assert_eq!(gh.several(&[key(30)]), [(key(30), Err(Outcome::NotFound))]);
    assert_eq!(
        gh.ticket(&key(30), Depth::Cut { notes: 0 }, None)
            .map(|_| ()),
        Err(Outcome::NotFound)
    );
    assert_eq!(gh.parts(&key(30)), Err(Outcome::NotFound));
}
