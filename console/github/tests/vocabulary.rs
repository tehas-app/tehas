//! `Tickets::marks` and `Tickets::kinds` as the adapter answers them,
//! replayed from its recordings of the fixture project under
//! `tests/recorded/vocabulary/<state>/`: the repository's labels with their
//! node ids, the owner's issue types, the laws over the same answers, and
//! the questions asked for them. `live/` and `personal/` are live
//! read-only answers of a public repository; the rest are made by hand.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tehas_core::forge::GhRunner;
use tehas_github::GitHub;
use tehas_ports::laws;
use tehas_ports::{
    Act, Bound, Choice, Got, Kind, Mark, MarkHeld, MarkId, MarkName, Outcome, Passing, Pool, Sent,
    State, TicketKey, Tickets, Unread, Whole,
};

const PROJECT: &str = "acme/widget";

fn here() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

type Asked = Arc<Mutex<Vec<Vec<String>>>>;

fn field<'a>(args: &'a [&str], name: &str) -> Option<&'a str> {
    args.windows(2)
        .filter(|w| w[0] == "-f" || w[0] == "-F")
        .find_map(|w| w[1].strip_prefix(&format!("{name}=")))
}

/// A runner that answers each question from the recordings of one state:
/// `labels-<cursor>` for a page of labels, `types-<cursor>` for a page of
/// issue types, `first` for a first page. `.json` is gh's answer, `.fail`
/// the reason it failed. The label write and the label events of issue 7
/// are answered too, for the law that puts a mark on.
struct Replay {
    state: String,
    asked: Asked,
}

impl GhRunner for Replay {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        self.asked
            .lock()
            .unwrap()
            .push(args.iter().map(|a| a.to_string()).collect());
        if args
            == [
                "api",
                "-X",
                "POST",
                "repos/acme/widget/issues/7/labels",
                "-f",
                "labels[]=tehas:ready",
            ]
        {
            return Ok(r#"[{"name":"tehas:ready"}]"#.to_string());
        }
        if args
            == [
                "api",
                "repos/acme/widget/issues/7/events?per_page=100",
                "--paginate",
            ]
        {
            let at = here().join("tests/recorded/issue-7/events.json");
            return Ok(std::fs::read_to_string(at).unwrap());
        }
        assert_eq!(&args[..2], ["api", "graphql"]);
        assert_eq!(field(args, "owner"), Some("acme"));
        assert_eq!(field(args, "name"), Some("widget"));
        let query = field(args, "query").expect("a query");
        let read = if query.contains("labels(") {
            "labels"
        } else if query.contains("issueTypes(") {
            "types"
        } else {
            panic!("no recording answers {query}")
        };
        let name = format!("{read}-{}", field(args, "cursor").unwrap_or("first"));
        let at = here().join("tests/recorded/vocabulary").join(&self.state);
        if let Ok(out) = std::fs::read_to_string(at.join(format!("{name}.json"))) {
            return Ok(out);
        }
        match std::fs::read_to_string(at.join(format!("{name}.fail"))) {
            Ok(why) => Err(why.trim_end().to_string()),
            Err(_) => Err(format!("no recording of {name} in {}", self.state)),
        }
    }
}

fn replay_now() -> i64 {
    1_767_225_600
}

fn adapter(state: &str) -> (GitHub, Asked) {
    let asked = Asked::default();
    let replay = Replay {
        state: state.to_string(),
        asked: Arc::clone(&asked),
    };
    let gh = GitHub::new(PROJECT, Box::new(replay)).on_clock(replay_now);
    (gh, asked)
}

fn bound(n: u32) -> Bound {
    Bound::new(n).unwrap()
}

fn marks(state: &str, b: u32) -> Got<Whole<MarkHeld>> {
    Tickets::marks(&mut adapter(state).0, bound(b))
}

fn kinds(state: &str) -> Got<Whole<Kind>> {
    Tickets::kinds(&mut adapter(state).0)
}

fn id_of(held: &[MarkHeld], mark: &Mark) -> Option<String> {
    held.iter()
        .find(|h| h.mark == *mark)
        .map(|h| h.id.as_str().to_string())
}

fn project(name: &str) -> Mark {
    Mark::Project(MarkName::new(name).unwrap())
}

#[test]
fn marks_name_every_label_of_the_repository_with_its_node_id() {
    let (mut gh, asked) = adapter("live");
    let held = Tickets::marks(&mut gh, bound(100)).unwrap().into_items();
    assert_eq!(held.len(), 37);
    let ready = Mark::Act(Act::Approve);
    assert_eq!(
        id_of(&held, &ready).as_deref(),
        Some("LA_kwDOVB1zmM8AAAAC8IM5PA")
    );
    // The words of the table beyond `tehas:`, and a project's own marks,
    // are held too: the dispatcher's read of `tehas:` labels left them out.
    for (mark, id) in [
        (Mark::Epic, "LA_kwDOVB1zmM8AAAADAr6L5A"),
        (Mark::State(State::NeedsPerson), "LA_kwDOVB1zmM8AAAAC8IM6bg"),
        (Mark::Choice(Choice::Lean), "LA_kwDOVB1zmM8AAAADANcnlg"),
        (project("good first issue"), "LA_kwDOVB1zmM8AAAAC72eePQ"),
        (project("tehas:all-hands"), "LA_kwDOVB1zmM8AAAAC-TyUCw"),
    ] {
        assert_eq!(id_of(&held, &mark).as_deref(), Some(id), "{mark:?}");
    }
    assert!(!held.iter().any(|h| h.mark == Mark::Unknown));
    assert!(gh.take_left_out().is_empty());
    let asked = asked.lock().unwrap();
    assert_eq!(asked.len(), 1);
    let query = field(
        &asked[0].iter().map(String::as_str).collect::<Vec<_>>(),
        "query",
    )
    .unwrap()
    .to_string();
    assert!(
        query.contains("labels(first: 100, after: $cursor"),
        "{query}"
    );
    assert!(query.contains("nodes { id name }"), "{query}");
    assert!(
        !query.contains("query: "),
        "the read narrows by no name: {query}"
    );
}

#[test]
fn marks_are_read_page_by_page_and_cut_past_the_bound() {
    let (mut gh, asked) = adapter("two-pages");
    let held = Tickets::marks(&mut gh, bound(101)).unwrap().into_items();
    assert_eq!(held.len(), 101);
    assert_eq!(
        id_of(&held, &Mark::Act(Act::Approve)).as_deref(),
        Some("LA_kwDOAAAAAM8AAAAA00000100")
    );
    let cursors: Vec<Option<String>> = asked
        .lock()
        .unwrap()
        .iter()
        .map(|a| {
            let a: Vec<&str> = a.iter().map(String::as_str).collect();
            field(&a, "cursor").map(str::to_string)
        })
        .collect();
    assert_eq!(cursors, [None, Some("MTAw".to_string())]);
    assert_eq!(marks("two-pages", 100), Err(Outcome::Unread(Unread::Cut)));
    // A first page past the bound asks no second.
    let (mut gh, asked) = adapter("two-pages");
    assert_eq!(
        Tickets::marks(&mut gh, bound(99)),
        Err(Outcome::Unread(Unread::Cut))
    );
    assert_eq!(asked.lock().unwrap().len(), 1);
}

#[test]
fn marks_keep_the_law_of_bounds_at_and_across_a_page() {
    for (state, held) in [("live", 37), ("two-pages", 101)] {
        let (mut gh, _) = adapter(state);
        let mut read = |b| Tickets::marks(&mut gh, b);
        assert_eq!(
            laws::lists::past_the_bound_is_unread(&mut read, held),
            Ok(()),
            "{state}"
        );
    }
}

#[test]
fn a_mark_put_on_is_named_by_marks_with_its_id() {
    let (mut gh, _) = adapter("live");
    let key = TicketKey::Number(std::num::NonZeroU64::new(7).unwrap());
    assert_eq!(
        laws::tickets::a_mark_event_names_the_marks_id(
            &mut gh,
            &key,
            &Mark::Act(Act::Approve),
            bound(100),
        ),
        Ok(())
    );
}

#[test]
fn a_name_no_mark_can_have_is_left_out_and_reported() {
    let (mut gh, _) = adapter("unreadable");
    let held = Tickets::marks(&mut gh, bound(10)).unwrap().into_items();
    assert_eq!(
        held,
        [
            MarkHeld::new(
                Mark::Act(Act::Approve),
                MarkId::new("LA_kwDOAAAAAM8AAAAA00000001")
            ),
            MarkHeld::new(Mark::Epic, MarkId::new("LA_kwDOAAAAAM8AAAAA00000003")),
        ]
    );
    assert_eq!(gh.take_left_out(), ["\u{1f41b} bug"]);
    // A name left out counts against no bound.
    assert_eq!(marks("unreadable", 2).map(|w| w.items().len()), Ok(2));
}

#[test]
fn kinds_name_the_owners_issue_types_in_the_sites_order() {
    let (mut gh, asked) = adapter("live");
    assert_eq!(
        Tickets::kinds(&mut gh).unwrap().into_items(),
        [Kind::Task, Kind::Bug, Kind::Feature]
    );
    assert_eq!(asked.lock().unwrap().len(), 1);
    // A personal account's repository answers GitHub's own types.
    assert_eq!(
        kinds("personal").unwrap().into_items(),
        [Kind::Task, Kind::Bug, Kind::Feature]
    );
    assert_eq!(kinds("no-types"), Ok(Whole::default()));
    let other = Kind::Other(MarkName::new("Improvement").unwrap());
    assert_eq!(
        kinds("two-pages").unwrap().into_items(),
        [Kind::Task, Kind::Bug, other]
    );
}

#[test]
fn a_type_no_kind_can_name_is_left_out_and_reported() {
    let (mut gh, _) = adapter("unreadable");
    assert_eq!(
        Tickets::kinds(&mut gh).unwrap().into_items(),
        [Kind::Task, Kind::Epic]
    );
    assert_eq!(gh.take_left_out(), ["Spike \u{26a1}"]);
}

#[test]
fn kinds_hold_no_unknown_on_a_type_no_kind_can_name() {
    let (mut gh, _) = adapter("unreadable");
    assert_eq!(laws::tickets::kinds_hold_no_unknown(&mut gh), Ok(()));
}

#[test]
fn an_answer_that_cannot_be_trusted_is_unread() {
    let malformed = Outcome::Unread(Unread::Malformed);
    let inconsistent = Outcome::Unread(Unread::Inconsistent);
    let states = [
        ("repeated-cursor", &inconsistent),
        ("no-page-info", &inconsistent),
        ("null-node", &inconsistent),
        ("twice", &inconsistent),
        ("no-id", &malformed),
        ("gone", &malformed),
        ("hollow", &malformed),
        ("garbled", &malformed),
    ];
    for (state, want) in states {
        assert_eq!(marks(state, 100).err().as_ref(), Some(want), "{state}");
        assert_eq!(kinds(state).err().as_ref(), Some(want), "{state}");
    }
    assert_eq!(marks("one-id", 100), Err(inconsistent));
}

#[test]
fn a_failed_call_answers_its_outcome() {
    let server = Outcome::Unavailable {
        why: Passing::Server,
        sent: Sent::Maybe,
    };
    for (state, want) in [
        ("forbidden", Outcome::Forbidden),
        (
            "rate-limited",
            Outcome::Spent {
                pool: Pool::Calls,
                until: tehas_ports::At::new(replay_now() + 60),
            },
        ),
        ("server-error", server.clone()),
        (
            "timeout",
            Outcome::Unavailable {
                why: Passing::Timeout,
                sent: Sent::Maybe,
            },
        ),
        ("second-page-fails", server),
    ] {
        assert_eq!(marks(state, 100), Err(want.clone()), "{state}");
        assert_eq!(kinds(state), Err(want), "{state}");
    }
}
