//! `Board::open_changes` asked for numbers, the refresh's read, as the
//! adapter answers it through the site, over hand-made recordings of the
//! fixture project under `tests/recorded/numbers/<state>/`: `pull-<n>` for
//! the question of one change, `pull-<n>-closers` for it with the closers
//! of what it closes, `issue-<n>` for an issue's REST read and `edit-<n>`
//! for its body's last edit. `.json` is gh's answer, `.fail` the reason it
//! failed. No test calls GitHub.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tehas_core::forge::GhRunner;
use tehas_github::{GitHubSite, MakeRunner};
use tehas_ports::laws;
use tehas_ports::{
    AccountKind, Actor, BlueprintNo, Board, Bound, Caller, ChangeAsk, ChangeFilter, ChangeItem,
    ChangeNo, ChangeStates, Got, Outcome, Passing, Ref, Refusal, Scope, Site, TicketKey,
    TicketState, Unread, Whole,
};

const PROJECT: &str = "acme/widget";

/// The bot's user id, as the factory records it.
const BOT_ID: u64 = 900;

type Asked = Arc<Mutex<Vec<Vec<String>>>>;

struct Replay {
    state: String,
    asked: Asked,
}

fn field<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.windows(2)
        .filter(|w| w[0] == "-f" || w[0] == "-F")
        .find_map(|w| w[1].strip_prefix(&format!("{name}=")))
}

impl GhRunner for Replay {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        self.asked.lock().unwrap().push(args.clone());
        let name = match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
            ["api", "graphql", ..] => {
                let query = field(&args, "query").expect("a query");
                let no = field(&args, "number").expect("a number");
                if query.contains("pullRequest(number: $number)") {
                    match query.contains(" closers: ") {
                        true => format!("pull-{no}-closers"),
                        false => format!("pull-{no}"),
                    }
                } else if query.contains("issue(number: $number) { lastEditedAt") {
                    format!("edit-{no}")
                } else {
                    panic!("no recording answers {query}")
                }
            }
            ["api", path] => {
                let no = path
                    .strip_prefix("repos/acme/widget/issues/")
                    .filter(|n| n.bytes().all(|b| b.is_ascii_digit()))
                    .unwrap_or_else(|| panic!("no recording answers {path}"));
                format!("issue-{no}")
            }
            _ => panic!("no recording answers {args:?}"),
        };
        let at = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/recorded/numbers")
            .join(&self.state);
        if let Ok(out) = std::fs::read_to_string(at.join(format!("{name}.json"))) {
            return Ok(out);
        }
        match std::fs::read_to_string(at.join(format!("{name}.fail"))) {
            Ok(why) => Err(why.trim_end().to_string()),
            Err(_) => Err(format!("no recording of {name} in {}", self.state)),
        }
    }
}

fn now() -> i64 {
    1_767_225_600
}

/// The board of the site acting as the factory, over the recordings of
/// `state`, and what it asked.
fn board(state: &str) -> (Box<dyn Board + Send>, Asked) {
    let asked = Asked::default();
    let (state, kept) = (state.to_string(), Arc::clone(&asked));
    let make: MakeRunner = Arc::new(move |_: &Actor| {
        Ok(Box::new(Replay {
            state: state.clone(),
            asked: Arc::clone(&kept),
        }) as Box<dyn GhRunner>)
    });
    let site = GitHubSite::new(PROJECT, "tehas-bot[bot]", Some(BOT_ID), make).on_clock(now);
    let b = site
        .board(&Actor::Ours(Scope::Full), Caller::new("test.refresh"))
        .unwrap();
    (b, asked)
}

fn no(n: u64) -> ChangeNo {
    ChangeNo::new(n).unwrap()
}

fn bound(n: u32) -> Bound {
    Bound::new(n).unwrap()
}

/// The refresh's ask: these changes, in every state, with what each
/// closes when `closing`, and their siblings up to `siblings`.
fn ask(numbers: &[u64], closing: bool, siblings: Option<u32>) -> ChangeAsk {
    ChangeAsk {
        filter: ChangeFilter {
            states: ChangeStates::All,
            numbers: numbers.iter().map(|n| no(*n)).collect(),
            ..ChangeFilter::default()
        },
        files: None,
        closing,
        siblings: siblings.map(bound),
        activity: None,
    }
}

fn read(state: &str, ask: &ChangeAsk, b: u32) -> (Got<Whole<ChangeItem>>, Vec<Vec<String>>) {
    let (mut board, asked) = board(state);
    let got = board.open_changes(ask, bound(b));
    let asked = asked.lock().unwrap().clone();
    (got, asked)
}

fn numbers(got: &Got<Whole<ChangeItem>>) -> Vec<u64> {
    got.as_ref()
        .unwrap()
        .items()
        .iter()
        .map(|i| i.change.no.get())
        .collect()
}

#[test]
fn the_refresh_reads_one_change_with_what_it_closes_and_its_siblings() {
    let (got, asked) = read("refresh", &ask(&[21], true, Some(5)), 5);
    let items = got.unwrap().into_items();
    assert_eq!(items.len(), 1);
    let item = &items[0];
    let change = &item.change;
    assert_eq!(change.no, no(21));
    let author = change.author.as_ref().unwrap();
    assert_eq!(author.kind, AccountKind::Ours);
    assert_eq!(author.login.as_str(), "tehas-bot");
    assert!(!change.from_fork);
    assert!(matches!(
        change.merge_state,
        tehas_ports::Mergeable::No(tehas_ports::NotMergeable::Conflict)
    ));
    // The digest keeps what it closes elsewhere; `closing` leaves it out.
    assert_eq!(change.closes.len(), 2);
    assert!(matches!(change.closes[1], Ref::Elsewhere { .. }));
    assert_eq!(item.closing.len(), 1);
    let (blueprint, ticket) = &item.closing[0];
    assert_eq!(blueprint.no, BlueprintNo::new(7).unwrap());
    assert!(blueprint.open);
    assert!(blueprint.comments.items.is_empty());
    assert!(blueprint.comments.more);
    assert_eq!(blueprint.edited.map(|a| a.secs()), Some(1_767_268_800));
    assert_eq!(
        blueprint.editor.as_ref().map(|a| a.kind),
        Some(AccountKind::Person)
    );
    let ticket = ticket.as_ref().unwrap();
    assert_eq!(ticket.key, TicketKey::Number(7.try_into().unwrap()));
    assert_eq!(ticket.state, TicketState::Open);
    assert_eq!(ticket.marks.len(), 1);
    // The closers of #7: itself, one merged, one of another project and
    // one of no project GitHub names; of other/lib#3, one there. None is
    // a sibling.
    assert!(item.siblings.is_empty());
    // One question for the change, the issue's REST read and its edit.
    assert_eq!(asked.len(), 3);
    assert_eq!(field(&asked[0], "number"), Some("21"));
    assert_eq!(asked[1], ["api", "repos/acme/widget/issues/7"]);
    assert_eq!(field(&asked[2], "number"), Some("7"));
}

#[test]
fn without_siblings_no_closers_are_asked_and_without_closing_no_issue() {
    let (got, asked) = read("refresh", &ask(&[21], false, None), 5);
    let item = &got.unwrap().into_items()[0];
    assert!(item.closing.is_empty());
    assert!(item.siblings.is_empty());
    assert_eq!(asked.len(), 1);
    let query = field(&asked[0], "query").unwrap();
    assert!(!query.contains("closedByPullRequestsReferences"), "{query}");
}

#[test]
fn ours_is_the_bot_by_its_type_and_its_id_alone() {
    let (got, _) = read("impostor", &ask(&[22, 23, 24], false, None), 5);
    let kinds: Vec<_> = got
        .unwrap()
        .into_items()
        .into_iter()
        .map(|i| i.change.author.map(|a| a.kind))
        .collect();
    assert_eq!(
        kinds,
        [
            Some(AccountKind::Bot),
            Some(AccountKind::Person),
            Some(AccountKind::Bot)
        ]
    );
}

#[test]
fn an_open_closer_of_this_project_is_a_sibling_counted_with_the_change() {
    let (got, _) = read("sibling", &ask(&[25], false, Some(2)), 5);
    let item = &got.unwrap().into_items()[0];
    let siblings: Vec<u64> = item.siblings.iter().map(|c| c.no.get()).collect();
    assert_eq!(siblings, [26]);
    // The change and its sibling are two: past a bound of one.
    let (got, _) = read("sibling", &ask(&[25], false, Some(1)), 5);
    assert_eq!(got, Err(Outcome::Unread(Unread::Cut)));
    // A merged change is not counted.
    let (got, _) = read("sibling", &ask(&[27], false, Some(1)), 5);
    assert_eq!(got.unwrap().items()[0].siblings.len(), 1);
}

#[test]
fn closers_past_what_one_question_reads_are_cut_and_a_missing_count_is_unread() {
    for (state, want) in [
        ("closers-cut", Outcome::Unread(Unread::Cut)),
        ("closers-no-count", Outcome::Unread(Unread::Malformed)),
        ("closers-disagree", Outcome::Unread(Unread::Inconsistent)),
        ("closer-twice", Outcome::Unread(Unread::Inconsistent)),
        ("no-closers", Outcome::Unread(Unread::Malformed)),
    ] {
        let n = if state == "closer-twice" { 28 } else { 25 };
        let (got, _) = read(state, &ask(&[n], false, Some(10)), 5);
        assert_eq!(got, Err(want), "{state}");
    }
}

#[test]
fn a_closed_item_that_is_no_issue_or_is_gone_is_left_out_and_a_failure_fails() {
    let (got, asked) = read("closing-gone", &ask(&[31], true, None), 5);
    let closing: Vec<u64> = got.unwrap().items()[0]
        .closing
        .iter()
        .map(|(b, _)| b.no.get())
        .collect();
    assert_eq!(closing, [7]);
    // #7 was never changed since it was opened: no question of its edit.
    assert_eq!(asked.len(), 4);
    let (got, _) = read("closing-fails", &ask(&[21], true, None), 5);
    assert!(
        matches!(
            got,
            Err(Outcome::Unavailable {
                why: Passing::Server,
                ..
            })
        ),
        "{got:?}"
    );
}

#[test]
fn an_answer_that_does_not_add_up_fails_the_read() {
    for (state, n, want) in [
        ("null", 21, Outcome::Unread(Unread::Inconsistent)),
        ("wrong-number", 21, Outcome::Unread(Unread::Inconsistent)),
        ("too-many-closes", 21, Outcome::Unread(Unread::TooLarge)),
    ] {
        let (got, _) = read(state, &ask(&[n], false, None), 5);
        assert_eq!(got, Err(want), "{state}");
    }
    // A number GitHub does not know: gh exits non-zero, and the read fails.
    let (got, _) = read("gone", &ask(&[99], false, None), 5);
    assert!(got.is_err(), "{got:?}");
}

#[test]
fn numbers_come_once_in_number_order_and_a_state_not_asked_is_left_out() {
    let (got, asked) = read("states", &ask(&[32, 21, 30, 21], false, None), 5);
    assert_eq!(numbers(&got), [21, 30, 32]);
    assert_eq!(asked.len(), 3);
    let mut merged = ask(&[32, 21, 30], false, None);
    merged.filter.states = ChangeStates::Merged;
    let (got, _) = read("states", &merged, 5);
    assert_eq!(numbers(&got), [30]);
}

#[test]
fn more_numbers_than_the_bound_are_cut_before_any_request() {
    let (got, asked) = read("states", &ask(&[21, 30, 32], false, None), 2);
    assert_eq!(got, Err(Outcome::Unread(Unread::Cut)));
    assert!(asked.is_empty());
    let (mut b, _) = board("states");
    let mut whole = |n| b.open_changes(&ask(&[21, 30, 32], false, None), n);
    assert_eq!(laws::lists::past_the_bound_is_unread(&mut whole, 3), Ok(()));
}

#[test]
fn numbers_beside_another_filter_files_or_events_are_refused_before_any_request() {
    let mut asks = Vec::new();
    let mut files = ask(&[21], false, None);
    files.files = Some(bound(5));
    asks.push(files);
    let mut events = ask(&[21], false, None);
    events.activity = Some(tehas_ports::Window::Last(1.try_into().unwrap()));
    asks.push(events);
    let mut head = ask(&[21], false, None);
    head.filter.head = Some(tehas_ports::Branch::new("tehas/issue-21").unwrap());
    asks.push(head);
    let mut bodies = ask(&[21], false, None);
    bodies.filter.bodies = true;
    asks.push(bodies);
    for a in asks {
        let (got, asked) = read("refresh", &a, 5);
        assert_eq!(got, Err(Outcome::Refused(Refusal::Unsupported)), "{a:?}");
        assert!(asked.is_empty());
    }
}
