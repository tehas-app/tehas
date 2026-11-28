//! `Board::open_work` as the adapter answers it: the open work of the
//! fixture project replayed from the recordings under
//! `tests/recorded/work/<state>/`, made after dispatch's queue, start and
//! approval cases (`console/dispatch/tests/fixtures/`): an issue an allowed
//! account readied, one retitled after its approval with closing changes of
//! its own and of another project, an epic with its parts on two pages and
//! its blockers, a part a person readied again after an outsider, a busy
//! timeline past the page's hundred, a timeline the page dropped, and one
//! that waits on more than the page holds. The laws of lists and of one item
//! failing alone are held over the same answers.

use std::num::{NonZeroU32, NonZeroU64};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tehas_core::forge::{Failed, GhRunner};
use tehas_github::{GitHub, GitHubSite, MakeRunner};
use tehas_ports::laws;
use tehas_ports::{
    Act, Actor, At, Blocker, BlockerState, Board, BoardAsk, Bound, Caller, ChangeStates, Depth,
    Filter, Got, Happened, HappenedKind, Item, ItemName, Mark, Marker, OpenWork, Outcome,
    ProjectName, Ref, Refusal, Scope, Site, TicketEvent, TicketKey, Tickets, Unread, Window,
    WorkItem, ALL_MAX,
};

const PROJECT: &str = "acme/widget";
const CALLER: Caller = Caller::new("test.open-work");

type Asked = Arc<Mutex<Vec<Vec<String>>>>;

fn field<'a>(args: &'a [&str], name: &str) -> Option<&'a str> {
    args.windows(2)
        .filter(|w| w[0] == "-f" || w[0] == "-F")
        .find_map(|w| w[1].strip_prefix(&format!("{name}=")))
}

/// The words of `text` between `open` and the next `close`.
fn between<'a>(text: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let rest = &text[text.find(open)? + open.len()..];
    Some(&rest[..rest.find(close)?])
}

/// The recording a call is answered from, in the state's directory:
/// `work-<cursor>` for a page of open work, `issue-<n>-<read>-<cursor>` for
/// a question of one issue (`ticket`, `history-<types>[-back]`, `parts`,
/// `mentions`), `issue-<n>` for its REST read and `events-<n>` for its
/// label events; `first` for a first page of work.
fn recording(args: &[&str]) -> String {
    let cursor = field(args, "cursor");
    match args {
        ["api", "graphql", ..] => {
            let query = field(args, "query").expect("a query");
            if query.contains("issues(states: [OPEN]") {
                return format!("work-{}", cursor.unwrap_or("first"));
            }
            assert!(query.contains("issue(number: $number)"), "{query}");
            let n = field(args, "number").expect("a number");
            let read = if query.contains("closes:") {
                "ticket".to_string()
            } else if query.contains("events:") {
                let types = between(query, "itemTypes: [", "]").unwrap_or_default();
                let mut name = format!("history-{}", types.to_ascii_lowercase().replace(", ", "+"));
                if query.contains("last: 100, before") {
                    name.push_str("-back");
                }
                name
            } else if query.contains("mentions:") {
                "mentions".to_string()
            } else {
                assert!(query.contains("subIssues"), "{query}");
                "parts".to_string()
            };
            match cursor {
                Some(c) => format!("issue-{n}-{read}-{c}"),
                None => format!("issue-{n}-{read}"),
            }
        }
        ["api", path, rest @ ..] => {
            let issue = path
                .strip_prefix(&format!("repos/{PROJECT}/issues/"))
                .unwrap_or_else(|| panic!("asked {args:?}"));
            match issue.split_once('/') {
                Some((n, "events?per_page=100")) if rest == ["--paginate"] => format!("events-{n}"),
                None => format!("issue-{issue}"),
                Some(_) => panic!("asked {args:?}"),
            }
        }
        _ => panic!("asked {args:?}"),
    }
}

/// Answers each call from the state's recordings: `<name>.json` is gh's
/// answer, `<name>.fail` the reason it failed, and `<name>.partial.json`
/// what gh printed when it exited 1 on an answer with errors. The second
/// time a name is asked, `<name>.2.json` answers it where there is one.
struct Replay {
    state: String,
    asked: Asked,
}

impl Replay {
    fn answer(&mut self, args: &[&str]) -> Result<String, Failed> {
        let mut asked = self.asked.lock().unwrap();
        let name = recording(args);
        let times = asked.iter().filter(|a| recording(&strs(a)) == name).count();
        asked.push(args.iter().map(|a| a.to_string()).collect());
        drop(asked);
        if args[..2] == ["api", "graphql"] {
            assert_eq!(field(args, "owner"), Some("acme"));
            assert_eq!(field(args, "name"), Some("widget"));
        }
        let at = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/recorded/work")
            .join(&self.state);
        let read = |file: String| std::fs::read_to_string(at.join(file)).ok();
        let again = match times {
            0 => None,
            n => read(format!("{name}.{}.json", n + 1)),
        };
        if let Some(out) = again.or_else(|| read(format!("{name}.json"))) {
            return Ok(out);
        }
        if let Some(out) = read(format!("{name}.partial.json")) {
            return Err(Failed {
                why: "gh exited 1".to_string(),
                answer: Some(out),
            });
        }
        let why = read(format!("{name}.fail"))
            .map(|why| why.trim_end().to_string())
            .unwrap_or_else(|| format!("no recording {name} in {}", self.state));
        Err(Failed { why, answer: None })
    }
}

impl GhRunner for Replay {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        self.answer(args).map_err(|f| f.why)
    }

    fn run_answer(&mut self, args: &[&str]) -> Result<String, Failed> {
        self.answer(args)
    }
}

/// A runner no call may reach.
struct Unasked;

impl GhRunner for Unasked {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        panic!("asked {args:?}")
    }
}

fn replay_now() -> i64 {
    1_767_225_600
}

/// The site of the fixture project, its bot the factory's account with
/// the user id 900, its runners replaying `state`.
fn site(state: &str, asked: &Asked) -> GitHubSite {
    let (state, asked) = (state.to_string(), Arc::clone(asked));
    let make: MakeRunner = Arc::new(move |_: &Actor| {
        Ok(Box::new(Replay {
            state: state.clone(),
            asked: Arc::clone(&asked),
        }) as Box<dyn GhRunner>)
    });
    GitHubSite::new(PROJECT, "tehas-bot[bot]", Some(900), make).on_clock(replay_now)
}

fn board(state: &str) -> (Box<dyn Board + Send>, Asked) {
    let asked = Asked::default();
    let b = site(state, &asked)
        .board(&Actor::Ours(Scope::Full), CALLER)
        .unwrap();
    (b, asked)
}

fn tickets(state: &str) -> Box<dyn Tickets + Send> {
    site(state, &Asked::default())
        .tickets(&Actor::Ours(Scope::Full), CALLER)
        .unwrap()
}

fn bound(n: u32) -> Bound {
    Bound::new(n).unwrap()
}

fn key(n: u64) -> TicketKey {
    TicketKey::Number(NonZeroU64::new(n).unwrap())
}

fn last(n: u32) -> Window {
    Window::Last(NonZeroU32::new(n).unwrap())
}

/// The queue's ask: no comment, the marks whole beside the newest five
/// retitles, each item's open closing changes, and the control ticket.
fn queue_ask() -> BoardAsk {
    let mut ask = BoardAsk::new(Depth::Cut { notes: 0 });
    ask.history_by_kind = vec![
        (HappenedKind::Marked, Window::Whole(ALL_MAX)),
        (HappenedKind::Unmarked, Window::Whole(ALL_MAX)),
        (HappenedKind::Retitled, last(5)),
    ];
    ask.changes = Some((ChangeStates::Open, Window::Whole(bound(5))));
    ask.control = Some(key(1));
    ask
}

fn bare() -> BoardAsk {
    BoardAsk::new(Depth::Cut { notes: 0 })
}

fn item(work: &OpenWork, n: u64) -> &Got<WorkItem> {
    &work
        .items
        .items()
        .iter()
        .find(|(k, _)| *k == key(n))
        .unwrap_or_else(|| panic!("#{n} is not in the work"))
        .1
}

fn ok(work: &OpenWork, n: u64) -> &WorkItem {
    item(work, n)
        .as_ref()
        .unwrap_or_else(|e| panic!("#{n}: {e:?}"))
}

fn ready() -> Mark {
    Mark::Act(Act::Approve)
}

/// Each event as its kind, its mark or titles, its account and its time.
fn said(events: &[TicketEvent]) -> Vec<String> {
    events
        .iter()
        .map(|e| {
            let what = match &e.what {
                Happened::Marked(m) => format!("+{m:?}"),
                Happened::Unmarked(m) => format!("-{m:?}"),
                Happened::Retitled { to, .. } => format!("retitled {to}"),
                other => format!("{other:?}"),
            };
            let by =
                e.by.as_ref()
                    .map(|a| format!("{}:{:?}", a.login.as_str(), a.kind))
                    .unwrap_or_default();
            format!(
                "{what} by {by} at {}",
                e.at.map(At::secs).unwrap_or_default()
            )
        })
        .collect()
}

fn at(iso: &str) -> i64 {
    tehas_core::time::from_iso(iso).unwrap()
}

#[test]
fn the_queue_reads_each_open_issue_with_what_its_work_item_holds() {
    let (mut b, asked) = board("queue");
    let work = b.open_work(&queue_ask(), bound(50)).unwrap();

    // Oldest filed first, whatever order the page gave.
    let keys: Vec<&TicketKey> = work.items.items().iter().map(|(k, _)| k).collect();
    let want: Vec<TicketKey> = [1, 3, 7, 8, 9, 11, 12, 14, 16, 18].map(key).to_vec();
    assert_eq!(keys, want.iter().collect::<Vec<_>>());

    // An allowed account's approval, and nothing else read.
    let three = ok(&work, 3);
    assert_eq!(three.ticket.digest.marks, [ready()]);
    assert_eq!(
        said(&three.ticket.history.items),
        [format!(
            "+{:?} by alice:Person at {}",
            ready(),
            at("2026-01-02T01:00:00Z")
        )]
    );
    assert!(!three.ticket.history.more);
    assert_eq!(three.blueprints.len(), 1);
    assert_eq!(three.blueprints[0].body, "The text of #3.");
    assert_eq!(three.blueprints[0].revision, three.ticket.digest.revision);
    assert!(three.blueprints[0].open);
    assert!(three.blueprints[0].comments.items.is_empty());
    assert!(!three.blueprints[0].comments.more);
    // Ticket and blueprint are one item: the notes are never read.
    assert!(three.notes.items.is_empty() && three.notes.more);
    assert!(three.changes.items.is_empty() && !three.changes.more);
    assert!(three.ticket.parts.is_empty() && three.ticket.blockers.is_empty());
    assert_eq!(three.ticket.digest.parent, None);

    // The ticket's text is cut, the blueprint's whole, the marker apart.
    let seven = ok(&work, 7);
    let body = &seven.blueprints[0].body;
    assert!(
        body.chars().count() > 200 && !body.contains("<!--"),
        "{body}"
    );
    assert_eq!(
        seven.ticket.text,
        body.chars().take(200).collect::<String>()
    );
    assert_eq!(
        seven.ticket.markers,
        [Marker::Blocked {
            by: vec![Ref::Here(key(3))]
        }]
    );
    assert_eq!(seven.blueprints[0].markers, seven.ticket.markers);
    assert_eq!(
        said(&seven.ticket.history.items),
        [
            format!(
                "+{:?} by alice:Person at {}",
                Mark::Project(tehas_ports::MarkName::new("bug").unwrap()),
                at("2026-01-03T00:10:00Z")
            ),
            format!(
                "+{:?} by alice:Person at {}",
                ready(),
                at("2026-01-03T01:00:00Z")
            ),
            format!(
                "retitled Retitled after its approval by dave:Person at {}",
                at("2026-01-03T02:00:00Z")
            ),
        ]
    );
    // The open change of this project alone: the merged one and the one of
    // another project are left out. The factory's change is its own.
    let nos: Vec<u64> = seven.changes.items.iter().map(|c| c.no.get()).collect();
    assert_eq!(nos, [21]);
    assert!(!seven.changes.more);
    // A whole change's digest, read as `open_changes` reads one.
    let change = &seven.changes.items[0];
    assert_eq!(change.marks, [Mark::State(tehas_ports::State::Refreshing)]);
    assert_eq!(
        change.closes,
        [Ref::Here(tehas_ports::BlueprintNo::new(7).unwrap())]
    );
    assert_eq!(
        change.size.map(|s| (s.files, s.added, s.deleted)),
        Some((2, 52, 3))
    );
    assert_eq!(change.merge_state, tehas_ports::Mergeable::Yes);
    let author = seven.changes.items[0].author.as_ref().unwrap();
    assert_eq!(author.kind, tehas_ports::AccountKind::Ours);
    assert_eq!(
        seven.ticket.mentioned_by,
        [
            Ref::Here(tehas_ports::ChangeNo::new(21).unwrap()),
            Ref::Elsewhere {
                project: ProjectName::new("other/lib"),
                item: ItemName::new("4"),
            },
        ]
    );

    // The epic's parts, the second page read on alone, and what it waits on.
    let eight = ok(&work, 8);
    let parts: Vec<(Ref<TicketKey>, bool)> = eight
        .ticket
        .parts
        .iter()
        .map(|p| (p.key.clone(), p.open))
        .collect();
    assert_eq!(
        parts,
        [(Ref::Here(key(9)), true), (Ref::Here(key(10)), false)]
    );
    assert_eq!(
        eight.ticket.blockers,
        [
            Blocker::new(Ref::Here(Item::Ticket(key(3))), BlockerState::Open),
            Blocker::new(
                Ref::Elsewhere {
                    project: ProjectName::new("other/lib"),
                    item: ItemName::new("5"),
                },
                BlockerState::Closed
            ),
        ]
    );
    let nos: Vec<u64> = eight.changes.items.iter().map(|c| c.no.get()).collect();
    assert_eq!(nos, [22, 24]);

    // A part: its parent, its assignee, its body's editor, its comments
    // not read, and an outsider's approval the factory took off before an
    // allowed account gave it again.
    let nine = ok(&work, 9);
    assert_eq!(nine.ticket.digest.parent, Some(Ref::Here(key(8))));
    let assigned: Vec<&str> = nine
        .ticket
        .assignees
        .iter()
        .map(|a| a.login.as_str())
        .collect();
    assert_eq!(assigned, ["alice"]);
    let bp = &nine.blueprints[0];
    assert_eq!(bp.edited, Some(At::new(at("2026-01-05T03:00:00Z"))));
    assert_eq!(bp.editor.as_ref().map(|a| a.login.as_str()), Some("bob"));
    assert!(bp.comments.items.is_empty() && bp.comments.more);
    assert_eq!(
        said(&nine.ticket.history.items),
        [
            format!(
                "+{:?} by mallory:Person at {}",
                ready(),
                at("2026-01-05T01:00:00Z")
            ),
            format!(
                "-{:?} by tehas-bot:Ours at {}",
                ready(),
                at("2026-01-05T01:05:00Z")
            ),
            format!(
                "+{:?} by alice:Person at {}",
                ready(),
                at("2026-01-05T02:00:00Z")
            ),
        ]
    );

    // A timeline past the page's hundred: the marks are read whole, the
    // retitles' newest five, with more.
    let eleven = ok(&work, 11);
    let history = said(&eleven.ticket.history.items);
    assert_eq!(history.len(), 8, "{history:?}");
    assert_eq!(
        history[0],
        format!(
            "+{:?} by alice:Person at {}",
            ready(),
            at("2026-01-05T00:00:00Z")
        )
    );
    assert_eq!(
        history[1],
        format!(
            "-{:?} by alice:Person at {}",
            ready(),
            at("2026-01-05T00:30:00Z")
        )
    );
    assert_eq!(
        history[2],
        format!(
            "retitled Busy 95 by dave:Person at {}",
            at("2026-01-06T01:34:00Z")
        )
    );
    assert_eq!(
        history[7],
        format!(
            "+{:?} by alice:Person at {}",
            Mark::Act(Act::Block),
            at("2026-01-07T00:00:00Z")
        )
    );
    assert!(eleven.ticket.history.more);

    // A page that dropped the timeline of a mark the issue carries: the
    // marks are read again.
    let twelve = ok(&work, 12);
    assert_eq!(
        said(&twelve.ticket.history.items),
        [format!(
            "+{:?} by alice:Person at {}",
            ready(),
            at("2026-01-08T01:00:00Z")
        )]
    );
    assert!(!twelve.ticket.history.more);

    // Waiting on more than the page holds fails that item alone.
    assert_eq!(item(&work, 14), &Err(Outcome::Unread(Unread::Cut)));

    // The page's newest event of the mark is its removal: the marks are
    // read again, and say it was applied after.
    let sixteen = ok(&work, 16);
    assert_eq!(
        said(&sixteen.ticket.history.items).last(),
        Some(&format!(
            "+{:?} by alice:Person at {}",
            ready(),
            at("2026-01-10T02:00:00Z")
        ))
    );

    // A page that counts more retitles than it holds is read again.
    let eighteen = ok(&work, 18);
    assert_eq!(eighteen.ticket.history.items.len(), 2);

    // The control ticket is read whole on its own, though it is on the
    // page (Decided 120).
    let control = work.control.as_ref().unwrap().as_ref().unwrap();
    assert_eq!(control.digest.marks, [Mark::Act(Act::Pause)]);
    assert!(!control.history.more);
    assert_eq!(control.history.items.len(), 1);

    // One question for the page, one for the epic's second page of parts,
    // two each for the timelines read again, two for the control ticket.
    let asked = asked.lock().unwrap();
    let names: Vec<String> = asked
        .iter()
        .map(|a| recording(&a.iter().map(String::as_str).collect::<Vec<_>>()))
        .collect();
    assert_eq!(
        names,
        [
            "work-first",
            "issue-8-parts-Y3Vyc29yOnYyOjE=",
            "events-11",
            "issue-11-history-renamed_title_event-back",
            "events-12",
            "issue-12-history-renamed_title_event-back",
            "events-16",
            "issue-16-history-renamed_title_event-back",
            "events-18",
            "issue-18-history-renamed_title_event-back",
            "issue-1",
            "issue-1-ticket",
        ]
    );
    let query = field(&strs(&asked[0]), "query").unwrap().to_string();
    for words in [
        "issues(states: [OPEN], first: 25, after: $cursor, orderBy: {field: CREATED_AT, direction: ASC})",
        "itemTypes: [LABELED_EVENT, UNLABELED_EVENT, RENAMED_TITLE_EVENT], last: 100)",
        "closedByPullRequestsReferences(first: 10)",
        "blockedBy(first: 100)",
        "subIssues(first: 100)",
        "parent { number repository { databaseId nameWithOwner } }",
    ] {
        assert!(query.contains(words), "{words}: {query}");
    }
}

fn strs(args: &[String]) -> Vec<&str> {
    args.iter().map(String::as_str).collect()
}

#[test]
fn a_kind_read_again_is_what_tickets_history_answers() {
    let (mut b, _) = board("queue");
    let ask = queue_ask();
    let work = b.open_work(&ask, bound(50)).unwrap();
    let mut t = tickets("queue");
    for n in [11, 12] {
        let history = &ok(&work, n).ticket.history;
        for (kind, window) in &ask.history_by_kind {
            let want = t
                .history(&key(n), *window, Some(std::slice::from_ref(kind)))
                .unwrap();
            let have: Vec<&TicketEvent> = history
                .items
                .iter()
                .filter(|e| e.what.kind() == *kind)
                .collect();
            assert_eq!(have, want.items.iter().collect::<Vec<_>>(), "#{n} {kind:?}");
        }
    }
}

#[test]
fn a_bare_ask_reads_no_timeline_and_no_change() {
    let (mut b, asked) = board("queue");
    let work = b.open_work(&bare(), bound(50)).unwrap();
    let three = ok(&work, 3);
    assert!(three.ticket.history.items.is_empty() && three.ticket.history.more);
    assert!(three.changes.items.is_empty() && three.changes.more);
    assert!(work.control.is_none());
    // The epic's second page of parts is read still: parts are whole.
    let asked = asked.lock().unwrap();
    assert_eq!(asked.len(), 2);
    let query = field(&strs(&asked[0]), "query").unwrap().to_string();
    assert!(!query.contains("events:"), "{query}");
    assert!(!query.contains("closedByPullRequestsReferences"), "{query}");
}

#[test]
fn a_mark_alone_asks_both_marks_and_takes_its_own() {
    let (mut b, asked) = board("queue");
    let mut ask = bare();
    ask.history_by_kind = vec![
        (HappenedKind::Marked, last(1)),
        (HappenedKind::Marked, Window::Whole(ALL_MAX)),
    ];
    let work = b.open_work(&ask, bound(50)).unwrap();
    let nine = &ok(&work, 9).ticket.history;
    // The first window of a kind named twice.
    assert_eq!(
        said(&nine.items),
        [format!(
            "+{:?} by alice:Person at {}",
            ready(),
            at("2026-01-05T02:00:00Z")
        )]
    );
    assert!(nine.more);
    let query = field(&strs(&asked.lock().unwrap()[0]), "query")
        .unwrap()
        .to_string();
    assert!(
        query.contains("itemTypes: [LABELED_EVENT, UNLABELED_EVENT], last: 100)"),
        "{query}"
    );
}

#[test]
fn the_list_keeps_the_law_of_bounds_and_cuts_before_any_item_is_read() {
    let (mut b, _) = board("queue");
    let mut read = |n: Bound| b.open_work(&bare(), n).map(|w| w.items);
    assert_eq!(laws::lists::past_the_bound_is_unread(&mut read, 10), Ok(()));
    let (mut b, asked) = board("queue");
    assert_eq!(
        b.open_work(&queue_ask(), bound(9)).map(|_| ()),
        Err(Outcome::Unread(Unread::Cut))
    );
    assert_eq!(asked.lock().unwrap().len(), 1);
}

#[test]
fn one_item_past_its_bound_fails_alone() {
    let (mut b, _) = board("queue");
    assert_eq!(
        laws::board::one_item_fails_alone(&mut *b, &queue_ask(), bound(50), &key(14)),
        Ok(())
    );
    // An item that does not read, and one whose next page of parts fails.
    for (state, n, want) in [
        ("one-malformed", 9, Outcome::Unread(Unread::Malformed)),
        (
            "parts-fail",
            8,
            Outcome::Unavailable {
                why: tehas_ports::Passing::Server,
                sent: tehas_ports::Sent::Maybe,
            },
        ),
    ] {
        let (mut b, _) = board(state);
        let work = b.open_work(&bare(), bound(50)).unwrap();
        assert_eq!(item(&work, n), &Err(want), "{state}");
        assert!(item(&work, 3).is_ok(), "{state}");
    }
}

#[test]
fn pages_are_read_on_by_their_cursor() {
    let (mut b, asked) = board("two-pages");
    let work = b.open_work(&bare(), bound(50)).unwrap();
    let keys: Vec<TicketKey> = work
        .items
        .into_items()
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    assert_eq!(keys, [key(3), key(9)]);
    let asked = asked.lock().unwrap();
    assert_eq!(asked.len(), 2);
    assert_eq!(field(&strs(&asked[1]), "cursor"), Some("Y3Vyc29yOnYyOjI1"));
}

#[test]
fn a_failed_or_odd_page_fails_the_read_never_a_short_list() {
    let maybe = |why| Outcome::Unavailable {
        why,
        sent: tehas_ports::Sent::Maybe,
    };
    // A bare 403 is this read's bad minute, so the queue asks again; a 403
    // naming its permission is a refusal the queue never asks again.
    for (state, want) in [
        (
            "rate-limited",
            Outcome::Spent {
                pool: tehas_ports::Pool::Calls,
                until: At::new(replay_now() + 60),
            },
        ),
        ("server-error", maybe(tehas_ports::Passing::Server)),
        ("timeout", maybe(tehas_ports::Passing::Timeout)),
        ("forbidden", maybe(tehas_ports::Passing::Server)),
        ("forbidden-named", Outcome::Forbidden),
        ("errors-beside-data", Outcome::Unread(Unread::Inconsistent)),
        ("same-twice", Outcome::Unread(Unread::Inconsistent)),
        ("short", Outcome::Unread(Unread::Inconsistent)),
        ("null-node", Outcome::Unread(Unread::Inconsistent)),
        ("no-number", Outcome::Unread(Unread::Inconsistent)),
    ] {
        let (mut b, _) = board(state);
        assert_eq!(
            b.open_work(&bare(), bound(50)).map(|_| ()),
            Err(want),
            "{state}"
        );
    }
}

#[test]
fn an_items_open_changes_past_its_window_cut_that_item() {
    let (mut b, _) = board("queue");
    let mut ask = queue_ask();
    ask.changes = Some((ChangeStates::Open, last(1)));
    let work = b.open_work(&ask, bound(50)).unwrap();
    assert_eq!(item(&work, 8), &Err(Outcome::Unread(Unread::Cut)));
    assert_eq!(ok(&work, 7).changes.items.len(), 1);
}

#[test]
fn an_ask_past_the_queues_is_unsupported_before_any_request() {
    let unsupported = Err(Outcome::Refused(Refusal::Unsupported));
    let mut asks = Vec::new();
    for depth in [
        Depth::All,
        Depth::Cut { notes: 1 },
        Depth::Last {
            notes: NonZeroU32::MIN,
        },
    ] {
        asks.push(BoardAsk::new(depth));
    }
    let mut every_kind = bare();
    every_kind.history = Some(Window::Whole(ALL_MAX));
    asks.push(every_kind);
    let mut assigned = bare();
    assigned.history_by_kind = vec![(HappenedKind::Assigned, last(5))];
    asks.push(assigned);
    for states in [
        ChangeStates::All,
        ChangeStates::Merged,
        ChangeStates::Closed,
    ] {
        let mut changes = bare();
        changes.changes = Some((states, last(5)));
        asks.push(changes);
    }
    let mut marked = bare();
    marked.filter = Filter {
        carrying: vec![ready()],
        ..Filter::default()
    };
    asks.push(marked);
    let mut gh = GitHub::new(PROJECT, Box::new(Unasked));
    for ask in asks {
        assert_eq!(
            gh.open_work(&ask, bound(10)).map(|_| ()),
            unsupported,
            "{ask:?}"
        );
    }
    // The filter's open is not asked: open tickets alone are read.
    let mut closed = bare();
    closed.filter.open = Some(false);
    let (mut b, _) = board("two-pages");
    assert_eq!(
        b.open_work(&closed, bound(10))
            .map(|w| w.items.items().len()),
        Ok(2)
    );
}

#[test]
fn an_error_under_one_issue_fails_that_item_alone() {
    let (mut b, _) = board("item-error");
    let work = b.open_work(&bare(), bound(50)).unwrap();
    assert_eq!(item(&work, 8), &Err(Outcome::Forbidden));
    assert!(item(&work, 3).is_ok());
    // An error anywhere else fails the read with the call's outcome.
    let (mut b, _) = board("error-elsewhere");
    assert_eq!(
        b.open_work(&bare(), bound(50)).map(|_| ()),
        Err(Outcome::Unavailable {
            why: tehas_ports::Passing::Server,
            sent: tehas_ports::Sent::Maybe,
        })
    );
}

#[test]
fn a_count_that_moves_between_pages_reads_the_pages_once_more() {
    let (mut b, asked) = board("count-settles");
    let work = b.open_work(&bare(), bound(50)).unwrap();
    assert_eq!(work.items.items().len(), 2);
    assert_eq!(asked.lock().unwrap().len(), 4);
    // It moves again: the read fails.
    let (mut b, asked) = board("count-changes");
    assert_eq!(
        b.open_work(&bare(), bound(50)).map(|_| ()),
        Err(Outcome::Unread(Unread::Inconsistent))
    );
    assert_eq!(asked.lock().unwrap().len(), 4);
}

#[test]
fn marks_read_again_that_do_not_apply_a_carried_mark_fail_the_item() {
    let (mut b, _) = board("marks-disagree");
    let mut ask = queue_ask();
    ask.control = None;
    let work = b.open_work(&ask, bound(50)).unwrap();
    assert_eq!(item(&work, 17), &Err(Outcome::Unread(Unread::Inconsistent)));
    assert!(item(&work, 3).is_ok());
}

#[test]
fn a_control_ticket_off_the_page_is_read_whole_on_its_own() {
    let (mut b, asked) = board("two-pages");
    let mut ask = queue_ask();
    ask.changes = None;
    let work = b.open_work(&ask, bound(50)).unwrap();
    let control = work.control.unwrap().unwrap();
    assert_eq!(control.digest.marks, [Mark::Act(Act::Pause)]);
    assert_eq!(control.digest.parent, None);
    let names: Vec<String> = asked
        .lock()
        .unwrap()
        .iter()
        .map(|a| recording(&strs(a)))
        .collect();
    assert_eq!(&names[names.len() - 2..], ["issue-1", "issue-1-ticket"]);
}

#[test]
fn more_closing_changes_than_the_page_asks_cut_their_item() {
    let mut ask = queue_ask();
    ask.control = None;
    let (mut b, _) = board("closing-many");
    let work = b.open_work(&ask, bound(50)).unwrap();
    assert_eq!(item(&work, 7), &Err(Outcome::Unread(Unread::Cut)));
    assert!(item(&work, 3).is_ok());
}

#[test]
fn a_closing_change_past_its_bound_of_marks_cuts_its_item() {
    let (mut b, _) = board("closing-big");
    let work = b.open_work(&queue_ask(), bound(50));
    let work = work.unwrap();
    assert_eq!(item(&work, 7), &Err(Outcome::Unread(Unread::Cut)));
    assert!(item(&work, 3).is_ok());
}
