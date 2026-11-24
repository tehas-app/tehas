//! `CodeHost::activity` and `CodeHost::branch` as the adapter answers
//! them, replayed from its recordings under
//! `tests/recorded/activity/<state>/`: a pull request's label events,
//! paged whole or the newest n, the laws of lists over the same answers,
//! the commit a branch points at, and how a failed or odd answer reads.
//! `live/` holds live read-only answers of a public repository, its label
//! events applied by a bot; the rest are made by hand.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tehas_core::forge::GhRunner;
use tehas_github::{GitHub, GitHubSite, MakeRunner};
use tehas_ports::laws;
use tehas_ports::Site;
use tehas_ports::{
    AccountKind, Act, Actor, At, Bound, Branch, Caller, ChangeEvent, ChangeHappened, ChangeNo,
    CodeHost, CommitId, Got, Mark, MarkName, Outcome, Passing, Pool, Scope, Sent, State, Took,
    Unread, Window, ALL_MAX,
};

const PROJECT: &str = "acme/widget";

fn here() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn replay_now() -> i64 {
    1_767_225_600
}

type Asked = Arc<Mutex<Vec<Vec<String>>>>;

fn field<'a>(args: &'a [&str], name: &str) -> Option<&'a str> {
    args.windows(2)
        .filter(|w| w[0] == "-f" || w[0] == "-F")
        .find_map(|w| w[1].strip_prefix(&format!("{name}=")))
}

/// A runner that answers each call from the recordings of one state:
/// `events-<whole|last>-<cursor>` for a page of label events (`first`
/// for a first page), `branch-<name>` for a branch's ref, each byte of
/// the name outside `[A-Za-z0-9-]` written `_`, and `pull` and `repo` for
/// the REST reads that settle a failed question. `.json` is gh's answer,
/// `.fail` the reason it failed. The state `endless` makes its pages: each
/// says more follows, and holds one event of a label no mark can have.
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
        let name = if args.get(1) == Some(&"graphql") {
            assert_eq!(field(args, "owner"), Some("acme"));
            assert_eq!(field(args, "name"), Some("widget"));
            let query = field(args, "query").expect("a query");
            if query.contains("pullRequest(") {
                assert!(field(args, "number").is_some());
                let cursor = field(args, "cursor");
                if self.state == "endless" {
                    return Ok(endless(cursor));
                }
                let way = match query.contains("first: 100, after: $cursor") {
                    true => "whole",
                    false => "last",
                };
                format!("events-{way}-{}", cursor.unwrap_or("first"))
            } else if query.contains("ref(qualifiedName: $ref)") {
                let full = field(args, "ref").expect("a ref");
                let name = full.strip_prefix("refs/heads/").expect("a branch's ref");
                let name: String = name
                    .chars()
                    .map(|c| match c.is_ascii_alphanumeric() || c == '-' {
                        true => c,
                        false => '_',
                    })
                    .collect();
                format!("branch-{name}")
            } else {
                panic!("no recording answers {query}")
            }
        } else {
            match args {
                ["api", "repos/acme/widget/pulls/20"] => "pull".to_string(),
                ["api", "repos/acme/widget"] => "repo".to_string(),
                _ => panic!("no recording answers {args:?}"),
            }
        };
        let at = here().join("tests/recorded/activity").join(&self.state);
        if let Ok(out) = std::fs::read_to_string(at.join(format!("{name}.json"))) {
            return Ok(out);
        }
        match std::fs::read_to_string(at.join(format!("{name}.fail"))) {
            Ok(why) => Err(why.trim_end().to_string()),
            Err(_) => Err(format!("no recording of {name} in {}", self.state)),
        }
    }
}

/// A page of the endless state after `cursor`: more follows, always.
fn endless(cursor: Option<&str>) -> String {
    let n: u32 = cursor.map_or(0, |c| c.trim_start_matches('p').parse().unwrap());
    serde_json::json!({"data": {"repository": {"pullRequest": {"number": 20, "events": {
        "pageInfo": {"hasNextPage": true, "endCursor": format!("p{}", n + 1)},
        "nodes": [{"__typename": "LabeledEvent", "createdAt": "2026-11-20T10:00:00Z",
            "actor": null, "label": {"name": "not a label?"}}]
    }}}}})
    .to_string()
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

/// A handle made by a site given the bot `login` and its user id `id`, as
/// the factory makes its own.
fn site_handle(state: &str, login: &str, id: Option<u64>) -> Box<dyn CodeHost + Send> {
    let state = state.to_string();
    let make: MakeRunner = Arc::new(move |_: &Actor| {
        Ok(Box::new(Replay {
            state: state.clone(),
            asked: Asked::default(),
        }) as Box<dyn GhRunner>)
    });
    GitHubSite::new(PROJECT, login, id, make)
        .on_clock(replay_now)
        .code_host(&Actor::Ours(Scope::Full), Caller::new("test.activity"))
        .unwrap()
}

fn twenty() -> ChangeNo {
    ChangeNo::new(20).unwrap()
}

/// The live recording's pull request.
fn live() -> ChangeNo {
    ChangeNo::new(15145).unwrap()
}

fn whole(n: u32) -> Window {
    Window::Whole(Bound::new(n).unwrap())
}

fn last(n: u32) -> Window {
    Window::Last(std::num::NonZeroU32::new(n).unwrap())
}

fn activity(state: &str, w: Window) -> Got<Took<ChangeEvent>> {
    adapter(state).0.activity(twenty(), w)
}

fn mark(name: &str) -> Mark {
    Mark::Project(MarkName::new(name).unwrap())
}

fn at(iso: &str) -> At {
    At::new(tehas_core::time::from_iso(iso).unwrap())
}

/// What an event says, short: its kind and mark, and who by login.
fn said(e: &ChangeEvent) -> (ChangeHappened, Option<String>) {
    let by = e.by.as_ref().map(|a| a.login.as_str().to_string());
    (e.what.clone(), by)
}

#[test]
fn a_live_pull_requests_label_events_are_its_marks_put_on_by_the_bot_that_put_them() {
    let (mut gh, _) = adapter("live");
    let took = gh.activity(live(), whole(100)).unwrap();
    assert!(!took.more);
    let what: Vec<_> = took.items.iter().map(|e| e.what.clone()).collect();
    assert_eq!(
        what,
        [
            ChangeHappened::Marked(mark("dependencies")),
            ChangeHappened::Marked(mark("go"))
        ]
    );
    for e in &took.items {
        let by = e.by.as_ref().unwrap();
        assert_eq!(by.login.as_str(), "dependabot");
        assert_eq!(by.kind, AccountKind::Bot);
        assert_eq!(by.id.as_ref().unwrap().as_str(), "49699333");
        assert_eq!(e.at, at("2026-11-23T14:03:35Z"));
        assert_eq!(e.committed, None);
    }
    assert!(gh.take_left_out().is_empty());

    // Two in one second: the newest one is not sure until the start.
    // Asked of another number, the answer is of no pull request asked.
    assert_eq!(
        activity("live", whole(100)),
        Err(Outcome::Unread(Unread::Inconsistent))
    );
    let newest = adapter("live").0.activity(live(), last(1)).unwrap();
    assert!(newest.more);
    assert_eq!(newest.items, took.items[1..]);
}

#[test]
fn an_account_is_ours_only_typed_a_bot_with_the_factorys_user_id() {
    let kinds = |login: &str, id: Option<u64>| -> Vec<AccountKind> {
        let mut h = site_handle("two-pages", login, id);
        let took = h.activity(twenty(), whole(10)).unwrap();
        took.items
            .iter()
            .filter_map(|e| e.by.as_ref().map(|a| a.kind))
            .collect()
    };
    use AccountKind::{Bot, Ours, Person};
    // person-one, tehas-bot, person-one, tehas-bot; the third event names nobody.
    assert_eq!(
        kinds("tehas-bot[bot]", Some(100001)),
        [Person, Ours, Ours, Person]
    );
    // The login alone is no proof: another id, or none, is a bot.
    assert_eq!(
        kinds("tehas-bot[bot]", Some(100002)),
        [Person, Bot, Bot, Person]
    );
    assert_eq!(kinds("tehas-bot[bot]", None), [Person, Bot, Bot, Person]);
    // A person carrying the bot's id is no bot.
    assert_eq!(kinds("person-one", Some(5001)), [Person, Bot, Bot, Person]);
}

#[test]
fn events_over_two_pages_are_read_oldest_first_an_addition_before_a_removal_in_one_second() {
    let (mut gh, asked) = adapter("two-pages");
    let took = gh.activity(twenty(), whole(5)).unwrap();
    let said: Vec<_> = took.items.iter().map(said).collect();
    let person = Some("person-one".to_string());
    let bot = Some("tehas-bot".to_string());
    let ready = Mark::Act(Act::Approve);
    let refreshing = Mark::State(State::Refreshing);
    use ChangeHappened::{Marked, Unmarked};
    assert_eq!(
        said,
        [
            (Marked(ready.clone()), person.clone()),
            (Marked(refreshing.clone()), bot.clone()),
            (Unmarked(refreshing), None),
            (Marked(Mark::State(State::Running)), bot),
            (Unmarked(ready), person),
        ]
    );
    assert_eq!(asked.lock().unwrap().len(), 2);
    // The label no mark can have is left out, and kept once to report.
    assert_eq!(gh.take_left_out(), ["not a label?"]);
    assert!(gh.take_left_out().is_empty());
}

#[test]
fn a_whole_read_past_its_bound_is_cut_never_a_shorter_list() {
    assert_eq!(
        activity("two-pages", whole(4)),
        Err(Outcome::Unread(Unread::Cut))
    );
    // Past the bound on the first page, no second page is asked.
    let (mut gh, asked) = adapter("two-pages");
    assert_eq!(
        gh.activity(twenty(), whole(1)),
        Err(Outcome::Unread(Unread::Cut))
    );
    assert_eq!(asked.lock().unwrap().len(), 1);
}

#[test]
fn the_laws_of_lists_hold_over_the_label_events() {
    let held = 5;
    let mut read = |w: Window| activity("two-pages", w);
    assert_eq!(laws::lists::last_n_is_never_cut(&mut read, held), Ok(()));
    assert_eq!(laws::lists::more_is_exact(&mut read, held), Ok(()));
    let whole_read = |b: Bound| {
        activity("two-pages", Window::Whole(b)).map(|t| tehas_ports::Whole::new(t.items))
    };
    assert_eq!(
        laws::lists::past_the_bound_is_unread(whole_read, held),
        Ok(())
    );
}

#[test]
fn the_newest_n_stop_reading_once_they_are_sure() {
    // The newest two are sure on the last page: one question.
    let (mut gh, asked) = adapter("two-pages");
    let took = gh.activity(twenty(), last(2)).unwrap();
    assert!(took.more);
    assert_eq!(took.items.len(), 2);
    assert_eq!(asked.lock().unwrap().len(), 1);
    // The newest one ties with the one before it: the page before is read.
    let (mut gh, asked) = adapter("two-pages");
    let took = gh.activity(twenty(), last(1)).unwrap();
    assert!(took.more);
    assert_eq!(asked.lock().unwrap().len(), 2);
}

#[test]
fn past_the_most_pages_the_read_is_cut() {
    let (mut gh, asked) = adapter("endless");
    assert_eq!(
        gh.activity(twenty(), Window::Whole(ALL_MAX)),
        Err(Outcome::Unread(Unread::Cut))
    );
    assert_eq!(asked.lock().unwrap().len(), 100);
}

#[test]
fn an_issues_number_is_not_found_and_a_failed_question_never_an_empty_list() {
    let cases: [(&str, Outcome); 6] = [
        ("issue", Outcome::NotFound),
        ("forbidden", Outcome::Forbidden),
        ("failed-question", Outcome::Unread(Unread::Inconsistent)),
        (
            "rate-limited",
            Outcome::Spent {
                pool: Pool::Priced,
                until: At::new(replay_now()),
            },
        ),
        (
            "server-error",
            Outcome::Unavailable {
                why: Passing::Server,
                sent: Sent::Maybe,
            },
        ),
        (
            "timeout",
            Outcome::Unavailable {
                why: Passing::Timeout,
                sent: Sent::Maybe,
            },
        ),
    ];
    for (state, want) in cases {
        assert_eq!(activity(state, whole(10)), Err(want), "{state}");
    }
    assert_eq!(activity("issue", last(10)), Err(Outcome::NotFound));
    // The settling read is the pull request's REST read.
    let (mut gh, asked) = adapter("issue");
    let _ = gh.activity(twenty(), whole(10));
    assert_eq!(
        asked.lock().unwrap()[1],
        ["api", "repos/acme/widget/pulls/20"]
    );
}

#[test]
fn an_odd_answer_is_unread_never_an_event_less() {
    let malformed = Outcome::Unread(Unread::Malformed);
    let inconsistent = Outcome::Unread(Unread::Inconsistent);
    let cases = [
        ("null-node", inconsistent.clone()),
        ("no-time", malformed.clone()),
        ("no-login", malformed.clone()),
        ("no-label", malformed.clone()),
        ("other-type", inconsistent.clone()),
        ("other-number", inconsistent.clone()),
        ("no-page-info", inconsistent.clone()),
        ("no-cursor", inconsistent.clone()),
        ("repeated-cursor", inconsistent.clone()),
        ("garbled", malformed),
        ("errors-beside-data", inconsistent),
        (
            "priced-out",
            Outcome::Spent {
                pool: Pool::Priced,
                until: At::new(replay_now()),
            },
        ),
        ("no-pull", Outcome::NotFound),
    ];
    for (state, want) in cases {
        assert_eq!(activity(state, whole(10)), Err(want), "{state}");
    }
}

fn branch(state: &str, name: &str) -> Got<Option<CommitId>> {
    adapter(state).0.branch(&Branch::new(name).unwrap())
}

#[test]
fn a_branch_is_the_commit_its_ref_points_at_and_none_when_there_is_none() {
    let trunk = CommitId::new("761ea6d6915b74b5479e29e25019d4d9f09e58c7").unwrap();
    assert_eq!(branch("live", "trunk"), Ok(Some(trunk.clone())));
    assert_eq!(branch("live", "no-such-branch-here"), Ok(None));
    assert_eq!(branch("branch", "main"), Ok(Some(trunk.clone())));
    // The ref is asked by its full name, as a field, never in a path.
    let (mut gh, asked) = adapter("branch");
    let name = Branch::new("tehas/7-1").unwrap();
    assert_eq!(gh.branch(&name), Ok(Some(trunk)));
    let asked = asked.lock().unwrap();
    let args: Vec<&str> = asked[0].iter().map(String::as_str).collect();
    assert_eq!(field(&args, "ref"), Some("refs/heads/tehas/7-1"));
    assert!(args.contains(&"-f"));
}

#[test]
fn a_branch_answered_as_another_ref_or_no_commit_is_unread() {
    let malformed = Err(Outcome::Unread(Unread::Malformed));
    let inconsistent = Err(Outcome::Unread(Unread::Inconsistent));
    assert_eq!(branch("branch", "other-name"), inconsistent);
    assert_eq!(branch("branch", "tagged"), inconsistent);
    assert_eq!(branch("branch", "tag-target"), malformed);
    assert_eq!(branch("branch", "short"), malformed);
}

#[test]
fn a_branch_of_a_project_that_is_gone_is_not_found() {
    assert_eq!(branch("branch-gone", "main"), Err(Outcome::NotFound));
    assert_eq!(branch("branch-gone", "null"), Err(Outcome::NotFound));
    let (mut gh, asked) = adapter("branch-gone");
    let _ = gh.branch(&Branch::new("main").unwrap());
    assert_eq!(asked.lock().unwrap()[1], ["api", "repos/acme/widget"]);
}
