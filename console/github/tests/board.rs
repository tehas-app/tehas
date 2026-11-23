//! The joined reads as the adapter answers them: each sample of
//! `Board::open_changes` in `console/ports/samples/github/` replayed from the
//! adapter's recordings of the fixture project under
//! `tests/recorded/board/<state>/`, the laws of lists over the same answers,
//! and the questions the adapter asks for them.

use serde::Deserialize;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use tehas_core::forge::GhRunner;
use tehas_github::GitHub;
use tehas_ports::laws;
use tehas_ports::{
    Actor, At, Board, BoardAsk, Bound, ChangeAsk, ChangeFilter, ChangeItem, ChangedFile, Depth,
    Filter, Got, Outcome, Refusal, TargetId, TicketKey, Unread, Whole,
};

const PROJECT: &str = "acme/widget";

fn here() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

type Asked = Rc<RefCell<Vec<Vec<String>>>>;

/// A runner that answers each GraphQL question from the recordings of one
/// state of the fixture project: `pulls-<cursor>` for a page of changes,
/// `pulls-head-<branch>-<cursor>` for one from a branch, and
/// `files-<number>-<cursor>` for a page of one change's files; `first` for
/// a first page. `.json` is gh's answer, `.fail` the reason it failed.
struct Replay {
    state: String,
    asked: Asked,
}

fn field<'a>(args: &'a [&str], name: &str) -> Option<&'a str> {
    args.windows(2)
        .filter(|w| w[0] == "-f" || w[0] == "-F")
        .find_map(|w| w[1].strip_prefix(&format!("{name}=")))
}

impl GhRunner for Replay {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        self.asked
            .borrow_mut()
            .push(args.iter().map(|a| a.to_string()).collect());
        assert_eq!(&args[..2], ["api", "graphql"]);
        assert_eq!(field(args, "owner"), Some("acme"));
        assert_eq!(field(args, "name"), Some("widget"));
        let query = field(args, "query").expect("a query");
        let name = if query.contains("pullRequests(") {
            let head = field(args, "head")
                .map(|h| format!("head-{}-", h.replace('/', "_")))
                .unwrap_or_default();
            format!("pulls-{head}{}", field(args, "cursor").unwrap_or("first"))
        } else if query.contains("pullRequest(number: $number)") {
            format!(
                "files-{}-{}",
                field(args, "number").expect("a number"),
                field(args, "files").expect("a cursor")
            )
        } else {
            panic!("no recording answers {query}")
        };
        let at = here().join("tests/recorded/board").join(&self.state);
        if let Ok(out) = std::fs::read_to_string(at.join(format!("{name}.json"))) {
            return Ok(out);
        }
        match std::fs::read_to_string(at.join(format!("{name}.fail"))) {
            Ok(why) => Err(why.trim_end().to_string()),
            Err(_) => Err(format!("no recording of {name} in {}", self.state)),
        }
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

fn adapter(state: &str) -> (GitHub, Asked) {
    let asked = Asked::default();
    let replay = Replay {
        state: state.to_string(),
        asked: Rc::clone(&asked),
    };
    let gh = GitHub::new(PROJECT, Box::new(replay)).on_clock(replay_now);
    (gh, asked)
}

fn unasked() -> GitHub {
    GitHub::new(PROJECT, Box::new(Unasked)).on_clock(replay_now)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Sample {
    op: String,
    actor: Actor,
    fixture: String,
    /// The state of the fixture project the answer was read from; None
    /// where no request may go out.
    state: Option<String>,
    ask: ChangeAsk,
    bound: Bound,
    answer: Got<Whole<ChangeItem>>,
}

fn samples() -> Vec<Sample> {
    let at = here().join("../ports/samples/github/board-open-changes.json");
    let text = std::fs::read_to_string(&at).unwrap_or_else(|e| panic!("{}: {e}", at.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", at.display()))
}

fn bound(n: u32) -> Bound {
    Bound::new(n).unwrap()
}

fn open_changes(files: Option<u32>, siblings: Option<u32>) -> ChangeAsk {
    ChangeAsk {
        filter: ChangeFilter::default(),
        files: files.map(bound),
        closing: false,
        siblings: siblings.map(bound),
        activity: None,
    }
}

#[test]
fn open_changes_answers_as_each_sample_says() {
    let all = samples();
    assert!(all.len() >= 20);
    for s in all {
        assert_eq!(s.op, "Board::open_changes");
        assert!(matches!(s.actor, Actor::Ours(_)), "{}", s.fixture);
        let got = match &s.state {
            Some(state) => adapter(state).0.open_changes(&s.ask, s.bound),
            None => unasked().open_changes(&s.ask, s.bound),
        };
        assert_eq!(got, s.answer, "{:?}: {}", s.state, s.fixture);
    }
}

#[test]
fn a_sample_for_every_failure_the_lane_names() {
    let answers: Vec<Got<Whole<ChangeItem>>> = samples().into_iter().map(|s| s.answer).collect();
    for want in [
        Outcome::NotFound,
        Outcome::Forbidden,
        Outcome::Unread(Unread::Malformed),
        Outcome::Unread(Unread::Inconsistent),
        Outcome::Unread(Unread::Cut),
        Outcome::Refused(Refusal::Unsupported),
    ] {
        assert!(answers.contains(&Err(want.clone())), "{want:?}");
    }
    let named = |f: fn(&Outcome) -> bool| answers.iter().any(|a| a.as_ref().err().is_some_and(f));
    assert!(named(|o| matches!(o, Outcome::Spent { .. })));
    assert!(named(|o| matches!(
        o,
        Outcome::Unavailable {
            why: tehas_ports::Passing::Server,
            ..
        }
    )));
    assert!(named(|o| matches!(
        o,
        Outcome::Unavailable {
            why: tehas_ports::Passing::Timeout,
            ..
        }
    )));
}

#[test]
fn the_list_keeps_the_law_of_bounds_at_and_across_a_page() {
    for (state, held) in [
        ("three-open-bare", 3),
        ("full-page", 100),
        ("two-pages", 101),
    ] {
        let (mut gh, _) = adapter(state);
        let mut read = |b| gh.open_changes(&open_changes(None, None), b);
        assert_eq!(
            laws::lists::past_the_bound_is_unread(&mut read, held),
            Ok(()),
            "{state}"
        );
    }
}

#[test]
fn a_changes_files_keep_the_law_of_bounds_at_and_across_a_page() {
    for (state, no, held) in [("three-open", 21, 2), ("files-paged", 40, 101)] {
        let (mut gh, _) = adapter(state);
        let mut read = |b: Bound| -> Got<Whole<ChangedFile>> {
            let all = gh.open_changes(&open_changes(Some(b.get()), None), bound(1000))?;
            let item = all
                .into_items()
                .into_iter()
                .find(|i| i.change.no.get() == no)
                .expect("the change");
            Ok(item.files.expect("files read"))
        };
        assert_eq!(
            laws::lists::past_the_bound_is_unread(&mut read, held),
            Ok(()),
            "{state}"
        );
    }
}

#[test]
fn one_question_a_page_and_the_count_cuts_before_the_next() {
    let (mut gh, asked) = adapter("two-pages");
    let whole = gh.open_changes(&open_changes(None, None), bound(101));
    assert_eq!(whole.map(|w| w.items().len()), Ok(101));
    let asked = asked.borrow();
    assert_eq!(asked.len(), 2);
    assert_eq!(field(&strs(&asked[0]), "cursor"), None);
    assert_eq!(
        field(&strs(&asked[1]), "cursor"),
        Some("Y3Vyc29yOnYyOnBhZ2V1")
    );
    let query = field(&strs(&asked[0]), "query").unwrap().to_string();
    assert!(query.contains("states: [OPEN]"), "{query}");
    assert!(query.contains("first: 100"), "{query}");
    assert!(!query.contains("files("), "{query}");
    drop(asked);

    // Past the bound by the first page's count: no second question.
    let (mut gh, asked) = adapter("two-pages");
    assert_eq!(
        gh.open_changes(&open_changes(None, None), bound(100)),
        Err(Outcome::Unread(Unread::Cut))
    );
    assert_eq!(asked.borrow().len(), 1);
}

fn strs(args: &[String]) -> Vec<&str> {
    args.iter().map(String::as_str).collect()
}

#[test]
fn files_beyond_the_first_page_are_asked_of_their_change_alone() {
    let (mut gh, asked) = adapter("files-paged");
    let got = gh
        .open_changes(&open_changes(Some(200), None), bound(5))
        .unwrap();
    assert_eq!(got.items()[0].files.as_ref().unwrap().items().len(), 101);
    let asked = asked.borrow();
    assert_eq!(asked.len(), 2);
    let first = field(&strs(&asked[0]), "query").unwrap().to_string();
    assert!(first.contains("files(first: 100)"), "{first}");
    let more = strs(&asked[1]);
    assert_eq!(field(&more, "number"), Some("40"));
    assert_eq!(field(&more, "files"), Some("Y3Vyc29yOnYyOpHOAAAAZA=="));
}

#[test]
fn a_branch_and_every_state_are_asked_of_the_site() {
    let (mut gh, asked) = adapter("one-branch");
    let mut ask = open_changes(None, None);
    ask.filter.head = Some(tehas_ports::Branch::new("feature/issue-21").unwrap());
    assert!(gh.open_changes(&ask, bound(5)).is_ok());
    let query = field(&strs(&asked.borrow()[0]), "query")
        .unwrap()
        .to_string();
    assert!(query.contains("headRefName: $head"), "{query}");

    let (mut gh, asked) = adapter("every-state");
    let mut ask = open_changes(None, None);
    ask.filter.states = tehas_ports::ChangeStates::All;
    assert!(gh.open_changes(&ask, bound(5)).is_ok());
    let query = field(&strs(&asked.borrow()[0]), "query")
        .unwrap()
        .to_string();
    assert!(query.contains("states: [OPEN, MERGED, CLOSED]"), "{query}");
}

#[test]
fn a_change_of_a_state_not_asked_does_not_add_up() {
    // The recording of every state, asked for open changes alone.
    let (mut gh, _) = adapter("every-state");
    assert_eq!(
        gh.open_changes(&open_changes(None, None), bound(5)),
        Err(Outcome::Unread(Unread::Inconsistent))
    );
}

#[test]
fn what_needs_a_ticket_a_blueprint_or_a_body_is_unsupported_before_any_request() {
    let unsupported = || Some(Outcome::Refused(Refusal::Unsupported));
    let ask = BoardAsk {
        depth: Depth::All,
        history: None,
        changes: None,
        filter: Filter::default(),
        control: None,
    };
    let key = TicketKey::Number(std::num::NonZeroU64::new(7).unwrap());
    let mut gh = unasked();
    assert_eq!(gh.open_work(&ask, bound(10)).err(), unsupported());
    assert_eq!(gh.work_item(&key, &ask).err(), unsupported());
    assert_eq!(
        gh.closed_work(&ask, At::new(0), bound(10)).err(),
        unsupported()
    );
    assert_eq!(
        gh.target_work(&ask, &TargetId::new("3"), bound(10)).err(),
        unsupported()
    );
    for narrow in [
        ChangeFilter {
            author: Some(tehas_ports::Author::Me),
            ..ChangeFilter::default()
        },
        ChangeFilter {
            closing: vec![tehas_ports::BlueprintNo::new(7).unwrap()],
            ..ChangeFilter::default()
        },
        ChangeFilter {
            merged_since: Some(At::new(0)),
            ..ChangeFilter::default()
        },
        ChangeFilter {
            updated_since: Some(At::new(0)),
            ..ChangeFilter::default()
        },
        ChangeFilter {
            numbers: vec![tehas_ports::ChangeNo::new(21).unwrap()],
            ..ChangeFilter::default()
        },
    ] {
        let ask = ChangeAsk {
            filter: narrow.clone(),
            ..open_changes(None, None)
        };
        assert_eq!(
            gh.open_changes(&ask, bound(10)).err(),
            unsupported(),
            "{narrow:?}"
        );
    }
}
