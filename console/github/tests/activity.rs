//! `CodeHost::activity` and `CodeHost::branch` as the adapter answers
//! them. The samples `console/ports/samples/github/code-host-activity.json`
//! and `code-host-branch.json` name a call, the recorded state it reads and
//! the answer it must give; the adapter answers each from its recordings
//! under `tests/recorded/activity/<state>/`, and the laws of lists check
//! the same answers. Beside them, the adapter's own cases: every kind of
//! event, the pages each window asks, and how a failed or odd answer
//! reads. `live/` holds live read-only answers of a public repository,
//! one person's login and id replaced; the rest are made by hand, or made
//! by the runner (`many`, `huge`, `endless`).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Deserialize;
use tehas_core::forge::{Failed, GhRunner};
use tehas_github::{GitHub, GitHubSite, MakeRunner};
use tehas_ports::laws;
use tehas_ports::{
    AccountKind, Act, Actor, At, Bound, Branch, Caller, ChangeEvent, ChangeHappened, ChangeNo,
    CodeHost, CommitId, Got, Mark, MarkName, Outcome, Pool, Scope, Site, State, Took, Unread,
    Whole, Window, ALL_MAX,
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
/// `events-<whole|last>-<cursor>` for a page of the timeline (`first` for
/// a first page), `branch-<name>` for a branch's ref, each byte of the
/// name outside `[A-Za-z0-9-]` written `_`, and `pull` and `repo` for the
/// REST reads that settle a failed question. `.json` is gh's answer;
/// `.fail` is the runner's reason on its first line and what gh answered
/// beside it on the rest, if anything. The states `many`, `huge` and
/// `endless` are made here (`made`).
struct Replay {
    state: String,
    asked: Asked,
}

impl Replay {
    fn answer(&mut self, args: &[&str]) -> Result<String, Failed> {
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
                let back = !query.contains("first: 100, after: $cursor");
                let cursor = field(args, "cursor");
                if let Some(made) = made(&self.state, back, cursor) {
                    return Ok(made);
                }
                let way = if back { "last" } else { "whole" };
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
        let text =
            std::fs::read_to_string(at.join(format!("{name}.fail"))).map_err(|_| Failed {
                why: format!("no recording of {name} in {}", self.state),
                answer: None,
            })?;
        let (why, said) = text.split_once('\n').unwrap_or((&text, ""));
        Err(Failed {
            why: why.to_string(),
            answer: Some(said.trim())
                .filter(|s| !s.is_empty())
                .map(str::to_string),
        })
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

/// How many comments a made state's timeline holds.
fn made_held(state: &str) -> Option<usize> {
    match state {
        "many" => Some(250),
        "huge" => Some(1500),
        _ => None,
    }
}

/// A page of a made state. `many` and `huge` hold comments a second apart;
/// the cursor `c<k>` is after the k-th, `b<k>` before it. `endless` says
/// more follows, either way, on every page, each holding one event of a
/// label no mark can have.
fn made(state: &str, back: bool, cursor: Option<&str>) -> Option<String> {
    let info = |more: bool, cursor: String| match back {
        false => serde_json::json!({"hasNextPage": more, "endCursor": cursor}),
        true => serde_json::json!({"hasPreviousPage": more, "startCursor": cursor}),
    };
    let (nodes, info) = if state == "endless" {
        let n: u32 = cursor.map_or(0, |c| c[1..].parse().unwrap());
        let node = serde_json::json!({"__typename": "LabeledEvent",
            "createdAt": "2026-11-20T10:00:00Z", "actor": null, "label": {"name": "not a label?"}});
        (vec![node], info(true, format!("p{}", n + 1)))
    } else {
        let held = made_held(state)?;
        let k: Option<usize> = cursor.map(|c| c[1..].parse().unwrap());
        let (from, to) = match back {
            false => {
                let from = k.unwrap_or(0);
                (from, (from + 100).min(held))
            }
            true => {
                let to = k.unwrap_or(held);
                (to.saturating_sub(100), to)
            }
        };
        let nodes = (from..to)
            .map(|i| {
                let at = tehas_core::time::iso(1_763_632_800 + i as i64);
                serde_json::json!({"__typename": "IssueComment", "createdAt": at,
                    "author": {"__typename": "User", "login": "person-one", "databaseId": 5001}})
            })
            .collect();
        let info = match back {
            false => info(to < held, format!("c{to}")),
            true => info(from > 0, format!("b{from}")),
        };
        (nodes, info)
    };
    Some(
        serde_json::json!({"data": {"repository": {"pullRequest": {"number": 20,
            "events": {"pageInfo": info, "nodes": nodes}}}}})
        .to_string(),
    )
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

/// The activity and how many questions it asked.
fn activity_asked(state: &str, w: Window) -> (Got<Took<ChangeEvent>>, usize) {
    let (mut gh, asked) = adapter(state);
    let got = gh.activity(twenty(), w);
    let n = asked.lock().unwrap().len();
    (got, n)
}

fn at(iso: &str) -> At {
    At::new(tehas_core::time::from_iso(iso).unwrap())
}

fn samples<T: for<'de> Deserialize<'de>>(file: &str) -> Vec<T> {
    let at = here().join("../ports/samples/github").join(file);
    let text = std::fs::read_to_string(&at).unwrap_or_else(|e| panic!("{}: {e}", at.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", at.display()))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ActivitySample {
    op: String,
    actor: Actor,
    project: String,
    fixture: String,
    no: ChangeNo,
    window: Window,
    answer: Got<Took<ChangeEvent>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BranchSample {
    op: String,
    actor: Actor,
    project: String,
    fixture: String,
    branch: Branch,
    answer: Got<Option<CommitId>>,
}

/// The failures every read's samples cover: not found, forbidden, a spent
/// budget, a server error, a time limit and an unreadable answer.
fn covers_the_failures<T>(answers: impl Iterator<Item = Got<T>>) {
    let seen: Vec<String> = answers
        .filter_map(|a| a.err())
        .map(|o| format!("{o:?}"))
        .collect();
    for w in [
        "NotFound",
        "Forbidden",
        "Spent",
        "Server",
        "Timeout",
        "Malformed",
    ] {
        assert!(seen.iter().any(|s| s.contains(w)), "no sample of {w}");
    }
}

#[test]
fn the_activity_answers_as_each_sample_says_and_keeps_the_laws_of_lists() {
    let all: Vec<ActivitySample> = samples("code-host-activity.json");
    assert!(!all.is_empty());
    covers_the_failures(all.iter().map(|s| s.answer.clone()));
    for s in &all {
        assert_eq!(s.op, "CodeHost::activity");
        assert!(matches!(s.actor, Actor::Ours(_)), "{}", s.fixture);
        let (mut gh, _) = adapter(&s.project);
        assert_eq!(
            gh.activity(s.no, s.window),
            s.answer,
            "{}: {}",
            s.project,
            s.fixture
        );
        // A whole read of a sample's state keeps the laws of lists.
        let (Window::Whole(_), Ok(took)) = (s.window, &s.answer) else {
            continue;
        };
        let held = took.items.len() as u32;
        let mut read = |w: Window| adapter(&s.project).0.activity(s.no, w);
        assert_eq!(
            laws::lists::last_n_is_never_cut(&mut read, held),
            Ok(()),
            "{}",
            s.fixture
        );
        assert_eq!(
            laws::lists::more_is_exact(&mut read, held),
            Ok(()),
            "{}",
            s.fixture
        );
        if held >= 2 {
            let whole = |b: Bound| {
                adapter(&s.project)
                    .0
                    .activity(s.no, Window::Whole(b))
                    .map(|t| Whole::new(t.items))
            };
            assert_eq!(
                laws::lists::past_the_bound_is_unread(whole, held),
                Ok(()),
                "{}",
                s.fixture
            );
        }
    }
}

#[test]
fn the_branch_answers_as_each_sample_says() {
    let all: Vec<BranchSample> = samples("code-host-branch.json");
    assert!(!all.is_empty());
    covers_the_failures(all.iter().map(|s| s.answer.clone()));
    for s in all {
        assert_eq!(s.op, "CodeHost::branch");
        assert!(matches!(s.actor, Actor::Ours(_)), "{}", s.fixture);
        let (mut gh, _) = adapter(&s.project);
        assert_eq!(
            gh.branch(&s.branch),
            s.answer,
            "{}: {}",
            s.project,
            s.fixture
        );
    }
}

#[test]
fn a_live_timeline_gives_every_kind_it_holds_in_the_sites_order() {
    let (mut gh, _) = adapter("live");
    let took = gh.activity(live(), whole(100)).unwrap();
    let what: Vec<_> = took
        .items
        .iter()
        .map(|e| {
            let by =
                e.by.as_ref()
                    .map(|a| (a.login.as_str().to_string(), a.kind));
            (e.what.clone(), by)
        })
        .collect();
    let mark = |n: &str| Mark::Project(MarkName::new(n).unwrap());
    let bot = |l: &str| Some((l.to_string(), AccountKind::Bot));
    let person = Some(("person-one".to_string(), AccountKind::Person));
    use ChangeHappened::*;
    assert_eq!(
        what,
        [
            (Pushed, None),
            (Unknown, None),
            (Unknown, None),
            (Marked(mark("dependencies")), bot("dependabot")),
            (Marked(mark("go")), bot("dependabot")),
            (Unknown, None),
            (Reviewed, bot("copilot-pull-request-reviewer")),
            (Commented, bot("cli-triage")),
            (Reviewed, person.clone()),
            (Merged, person.clone()),
            (Closed, person),
            (Unknown, None),
            (Unknown, None),
        ]
    );
    // The commit is pushed at the time it was committed.
    assert_eq!(took.items[0].committed, Some(took.items[0].at));
    assert!(gh.take_left_out().is_empty());
    // Read backward, the newest n are the whole read's last n.
    let newest = adapter("live").0.activity(live(), last(3)).unwrap();
    assert!(newest.more);
    assert_eq!(newest.items, took.items[10..]);
    // Asked of another number, the answer is of no pull request asked.
    assert_eq!(
        activity("live", whole(100)),
        Err(Outcome::Unread(Unread::Inconsistent))
    );
}

#[test]
fn every_kind_the_ports_name_is_read_from_its_item() {
    let took = activity("kinds", whole(100)).unwrap();
    let ours = |e: &ChangeEvent| e.by.as_ref().map(|a| a.login.as_str().to_string());
    let rows: Vec<_> = took
        .items
        .iter()
        .map(|e| (e.what.clone(), e.at, e.committed, ours(e)))
        .collect();
    let refreshing = Mark::State(State::Refreshing);
    let bot = Some("tehas-bot".to_string());
    let person = Some("person-one".to_string());
    use ChangeHappened::*;
    assert_eq!(
        rows,
        [
            // An item of another kind takes the next time after it.
            (Unknown, at("2026-11-19T09:00:00Z"), None, None),
            (
                Pushed,
                at("2026-11-19T09:00:00Z"),
                Some(at("2026-11-19T09:00:00Z")),
                None
            ),
            (
                Marked(refreshing.clone()),
                at("2026-11-20T10:00:00Z"),
                None,
                bot.clone()
            ),
            (Commented, at("2026-11-20T10:01:00Z"), None, person.clone()),
            // The pending review is left out.
            (Reviewed, at("2026-11-20T10:02:00Z"), None, person.clone()),
            (
                Pushed,
                at("2026-11-20T10:03:00Z"),
                Some(at("2026-11-20T09:59:00Z")),
                bot.clone()
            ),
            (Pushed, at("2026-11-20T10:04:00Z"), None, bot.clone()),
            (Unmarked(refreshing), at("2026-11-20T10:05:00Z"), None, None),
            // The label no mark can have is left out.
            (Closed, at("2026-11-20T10:06:00Z"), None, person.clone()),
            (Reopened, at("2026-11-20T10:07:00Z"), None, person),
            (Merged, at("2026-11-20T10:08:00Z"), None, bot.clone()),
            (Closed, at("2026-11-20T10:08:00Z"), None, bot),
            // The newest, of another kind, takes the time before it.
            (Unknown, at("2026-11-20T10:08:00Z"), None, None),
        ]
    );
    let mut read = |w: Window| activity("kinds", w);
    assert_eq!(laws::lists::last_n_is_never_cut(&mut read, 13), Ok(()));
    assert_eq!(laws::lists::more_is_exact(&mut read, 13), Ok(()));
    let (mut gh, _) = adapter("kinds");
    let _ = gh.activity(twenty(), whole(100));
    assert_eq!(gh.take_left_out(), ["not a label?"]);
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
    assert_eq!(
        kinds("tehas-bot[bot]", Some(100001)),
        [Person, Ours, Person, Ours]
    );
    // The login alone is no proof: another id, or none, is a bot.
    assert_eq!(
        kinds("tehas-bot[bot]", Some(100002)),
        [Person, Bot, Person, Bot]
    );
    assert_eq!(kinds("tehas-bot[bot]", None), [Person, Bot, Person, Bot]);
    // A person carrying the bot's id is no bot.
    assert_eq!(kinds("person-one", Some(5001)), [Person, Bot, Person, Bot]);
}

#[test]
fn label_events_over_two_pages_are_read_in_the_sites_order() {
    let (mut gh, asked) = adapter("two-pages");
    let took = gh.activity(twenty(), whole(5)).unwrap();
    let ready = Mark::Act(Act::Approve);
    let refreshing = Mark::State(State::Refreshing);
    use ChangeHappened::{Marked, Unmarked};
    let what: Vec<_> = took.items.iter().map(|e| e.what.clone()).collect();
    assert_eq!(
        what,
        [
            Marked(ready.clone()),
            Marked(refreshing.clone()),
            Unmarked(refreshing),
            Unmarked(ready),
            Marked(Mark::State(State::Running)),
        ]
    );
    assert_eq!(asked.lock().unwrap().len(), 2);
    assert_eq!(gh.take_left_out(), ["not a label?"]);
    assert!(gh.take_left_out().is_empty());
}

#[test]
fn a_whole_read_past_its_bound_is_cut_and_asks_no_page_more() {
    assert_eq!(
        activity_asked("many", whole(250)).0.map(|t| t.items.len()),
        Ok(250)
    );
    assert_eq!(
        activity_asked("many", whole(249)),
        (Err(Outcome::Unread(Unread::Cut)), 3)
    );
    assert_eq!(
        activity_asked("many", whole(99)),
        (Err(Outcome::Unread(Unread::Cut)), 1)
    );
    assert_eq!(
        activity_asked("endless", Window::Whole(ALL_MAX)),
        (Err(Outcome::Unread(Unread::Cut)), 100)
    );
}

#[test]
fn the_newest_n_are_read_backward_a_page_for_each_hundred_and_never_cut() {
    for (n, pages, more) in [
        (1, 1, true),
        (100, 1, true),
        (101, 2, true),
        (250, 3, false),
        (400, 3, false),
    ] {
        let (got, asked) = activity_asked("many", last(n));
        let took = got.unwrap();
        assert_eq!(asked, pages, "{n}");
        assert_eq!(took.more, more, "{n}");
        assert_eq!(took.items.len(), (n as usize).min(250), "{n}");
        assert_eq!(
            took.items.last().unwrap().at,
            At::new(1_763_632_800 + 249),
            "{n}"
        );
    }
    let mut read = |w: Window| activity("many", w);
    assert_eq!(laws::lists::last_n_is_never_cut(&mut read, 250), Ok(()));
}

#[test]
fn past_all_max_the_newest_n_read_as_all_max_with_more() {
    let (got, asked) = activity_asked("huge", last(5000));
    let took = got.unwrap();
    assert_eq!(took.items.len(), ALL_MAX.get() as usize);
    assert!(took.more);
    assert_eq!(asked, 10);
    assert_eq!(took.items[0].at, At::new(1_763_632_800 + 500));
    // Where nothing more is held, more is false.
    let took = activity("many", last(5000)).unwrap();
    assert_eq!((took.items.len(), took.more), (250, false));
    // Past the most pages, the newest n hold what was read, with more.
    let (got, asked) = activity_asked("endless", last(10));
    assert_eq!(got.map(|t| (t.items.len(), t.more)), Ok((0, true)));
    assert_eq!(asked, 100);
}

#[test]
fn gh_s_answer_beside_a_failure_names_it_and_the_rest_settle_by_rest() {
    let spent = Outcome::Spent {
        pool: Pool::Priced,
        until: At::new(replay_now() + 60),
    };
    // A rate limit and an issue's number are read from gh's answer, with
    // no request more.
    assert_eq!(activity_asked("rate-limited", whole(10)), (Err(spent), 1));
    assert_eq!(
        activity_asked("issue", whole(10)),
        (Err(Outcome::NotFound), 1)
    );
    assert_eq!(
        activity_asked("issue", last(10)),
        (Err(Outcome::NotFound), 1)
    );
    assert_eq!(
        activity_asked("errors-beside-data", whole(10)),
        (Err(Outcome::Unread(Unread::Inconsistent)), 1)
    );
    // With no answer, the pull request's REST read settles it.
    let (mut gh, asked) = adapter("no-answer");
    assert_eq!(gh.activity(twenty(), whole(10)), Err(Outcome::NotFound));
    assert_eq!(
        asked.lock().unwrap()[1],
        ["api", "repos/acme/widget/pulls/20"]
    );
    assert_eq!(
        activity("failed-question", whole(10)),
        Err(Outcome::Unread(Unread::Inconsistent))
    );
}

#[test]
fn an_odd_answer_is_unread_never_an_event_less() {
    let malformed = Outcome::Unread(Unread::Malformed);
    let inconsistent = Outcome::Unread(Unread::Inconsistent);
    let cases = [
        ("null-node", inconsistent.clone()),
        ("no-type", inconsistent.clone()),
        ("no-time", malformed.clone()),
        ("no-login", malformed.clone()),
        ("no-label", malformed.clone()),
        ("no-commit-time", malformed.clone()),
        ("no-review-state", malformed.clone()),
        ("untimed", inconsistent.clone()),
        ("other-number", inconsistent.clone()),
        ("no-page-info", inconsistent.clone()),
        ("no-cursor", inconsistent.clone()),
        ("repeated-cursor", inconsistent.clone()),
        ("garbled", malformed),
        ("no-pull", Outcome::NotFound),
    ];
    for (state, want) in cases {
        assert_eq!(activity(state, whole(10)), Err(want), "{state}");
    }
}

fn branch(state: &str, name: &str) -> (Got<Option<CommitId>>, Vec<Vec<String>>) {
    let (mut gh, asked) = adapter(state);
    let got = gh.branch(&Branch::new(name).unwrap());
    let asked = asked.lock().unwrap().clone();
    (got, asked)
}

#[test]
fn a_branch_is_asked_by_its_full_ref_as_a_field() {
    let (got, asked) = branch("branch", "tehas/7-1");
    assert!(got.unwrap().is_some());
    let args: Vec<&str> = asked[0].iter().map(String::as_str).collect();
    assert_eq!(field(&args, "ref"), Some("refs/heads/tehas/7-1"));
}

#[test]
fn a_branch_failure_is_read_from_gh_s_answer_and_settled_by_rest_without_one() {
    let (got, asked) = branch("branch", "gone");
    assert_eq!((got, asked.len()), (Err(Outcome::NotFound), 1));
    let (got, asked) = branch("branch-settled", "main");
    assert_eq!(got, Err(Outcome::NotFound));
    assert_eq!(asked[1], ["api", "repos/acme/widget"]);
    assert_eq!(branch("branch-settled", "null").0, Err(Outcome::NotFound));
}

#[test]
fn unknown_items_at_the_end_take_the_time_before_them_never_the_reads() {
    for w in [whole(10), last(2)] {
        let took = activity("unknown-last", w).unwrap();
        let rows: Vec<_> = took.items.iter().map(|e| (e.what.clone(), e.at)).collect();
        let merged = at("2026-11-20T10:08:00Z");
        let tail = [
            (ChangeHappened::Unknown, merged),
            (ChangeHappened::Unknown, merged),
        ];
        assert_eq!(rows[rows.len() - 2..], tail, "{w:?}");
        assert!(took.items.iter().all(|e| e.at != At::new(replay_now())));
        // Oldest first: no time falls down the list.
        assert!(took.items.windows(2).all(|p| p[0].at <= p[1].at), "{w:?}");
    }
}
