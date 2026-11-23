//! The code host port's reads as the adapter answers them: each sample in
//! `console/ports/samples/github/code-host-*.json` names a call, the
//! fixture project it reads and the answer it must give, and the adapter
//! answers it from its own recordings of that project, under
//! `tests/recorded/code-host/<project>/`, replayed through a runner. The
//! laws of `tehas_ports::laws` then check the same answers. Beside them, the
//! adapter's own cases: what it does not answer yet, and how gh's failures
//! of a GraphQL question read.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use serde::Deserialize;
use tehas_core::forge::GhRunner;
use tehas_github::GitHub;
use tehas_ports::laws;
use tehas_ports::{
    Access, Actor, Author, Bound, ChangeDigest, ChangeFilter, ChangeNo, ChangeStates, Checks,
    CodeHost, CommitId, Facts, Got, Outcome, Refusal, Unread, Whole, Window,
};

fn here() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The time the replay's clock reads: a spent budget answers it as `until`.
fn replay_now() -> i64 {
    1_767_225_600
}

type Asked = Rc<RefCell<Vec<String>>>;

/// A runner that answers each call from the recording of the read it
/// makes: `<project>/<read>.json` is gh's answer, `<project>/<read>.fail`
/// the reason the call failed. A call with no recording fails.
struct Replay {
    asked: Asked,
}

/// The recording a call reads, named by the project and the read: `repo`,
/// `pulls` (the REST list that settles a failed question),
/// `checks-<commit>-<page>[-name=<name>]`, or
/// `changes-<states>[-me][-head=<branch>][-after=<cursor>]`, each byte
/// outside `[A-Za-z0-9=-]` written `_`.
fn recording(args: &[&str]) -> Option<(String, String)> {
    let field = |name: &str| {
        args.iter()
            .find_map(|a| a.strip_prefix(&format!("{name}=")))
            .map(str::to_string)
    };
    let (project, read) = if args.get(1) == Some(&"graphql") {
        let query = field("query")?;
        let states = query.split("states: [").nth(1)?.split(']').next()?;
        let mut read = format!("changes-{}", states.replace(", ", "_"));
        if query.contains("viewer") {
            read.push_str("-me");
        }
        for var in ["head", "after"] {
            if let Some(v) = field(var) {
                read.push_str(&format!("-{var}={v}"));
            }
        }
        (field("name")?, read)
    } else {
        let path = args.get(1)?.strip_prefix("repos/acme/")?;
        match path.split_once('/') {
            None => (path.to_string(), "repo".to_string()),
            Some((project, "pulls?per_page=1&state=all")) => {
                (project.to_string(), "pulls".to_string())
            }
            Some((project, rest)) => {
                let rest = rest.strip_prefix("commits/")?;
                let (commit, query) = rest.split_once("/check-runs?per_page=100&page=")?;
                let (page, name) = match query.split_once("&check_name=") {
                    Some((page, name)) => (page, format!("-name={name}")),
                    None => (query, String::new()),
                };
                (project.to_string(), format!("checks-{commit}-{page}{name}"))
            }
        }
    };
    let read = read
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '=' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    Some((project, read))
}

impl GhRunner for Replay {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        self.asked.borrow_mut().push(args.join(" "));
        let (project, read) =
            recording(args).ok_or_else(|| format!("no recording of {}", args.join(" ")))?;
        let at = here().join("tests/recorded/code-host").join(project);
        if let Ok(out) = std::fs::read_to_string(at.join(format!("{read}.json"))) {
            return Ok(out);
        }
        match std::fs::read_to_string(at.join(format!("{read}.fail"))) {
            Ok(why) => Err(why.trim_end().to_string()),
            Err(_) => Err(format!("no recording of {read}")),
        }
    }
}

fn adapter(project: &str) -> (GitHub, Asked) {
    let asked = Asked::default();
    let replay = Replay {
        asked: Rc::clone(&asked),
    };
    let gh = GitHub::new(project, Box::new(replay)).on_clock(replay_now);
    (gh, asked)
}

fn samples<T: for<'de> Deserialize<'de>>(file: &str) -> Vec<T> {
    let at = here().join("../ports/samples/github").join(file);
    let text = std::fs::read_to_string(&at).unwrap_or_else(|e| panic!("{}: {e}", at.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", at.display()))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FactsSample {
    op: String,
    actor: Actor,
    project: String,
    fixture: String,
    answer: Got<Facts>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChangesSample {
    op: String,
    actor: Actor,
    project: String,
    fixture: String,
    filter: ChangeFilter,
    bound: Bound,
    answer: Got<Whole<ChangeDigest>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChecksSample {
    op: String,
    actor: Actor,
    project: String,
    fixture: String,
    at: CommitId,
    name: Option<String>,
    answer: Got<Checks>,
}

/// Every named outcome a read's samples hold, failures included, so no
/// failure is left without its sample.
fn outcomes<T>(answers: impl Iterator<Item = Got<T>>) -> Vec<String> {
    let mut seen: Vec<String> = answers
        .filter_map(|a| a.err())
        .map(|o| format!("{o:?}"))
        .collect();
    seen.sort();
    seen.dedup();
    seen
}

/// The failures every read's samples cover: not found, forbidden, a spent
/// budget, a server error, a time limit and a cut answer.
fn covers_the_failures(seen: &[String], not_found: bool) {
    let mut want = vec!["Forbidden", "Spent", "Server", "Timeout", "Malformed"];
    if not_found {
        want.push("NotFound");
    }
    for w in want {
        assert!(seen.iter().any(|s| s.contains(w)), "no sample of {w}");
    }
}

#[test]
fn the_facts_answer_as_each_sample_says() {
    let all: Vec<FactsSample> = samples("code-host-facts.json");
    assert!(!all.is_empty());
    covers_the_failures(&outcomes(all.iter().map(|s| s.answer.clone())), true);
    for s in all {
        assert_eq!(s.op, "CodeHost::facts");
        assert!(matches!(s.actor, Actor::Ours(_)), "{}", s.fixture);
        let (mut gh, asked) = adapter(&s.project);
        assert_eq!(gh.facts(), s.answer, "{}: {}", s.project, s.fixture);
        assert_eq!(asked.borrow().len(), 1, "{}", s.fixture);
    }
}

#[test]
fn the_changes_answer_as_each_sample_says() {
    let all: Vec<ChangesSample> = samples("code-host-changes.json");
    assert!(!all.is_empty());
    let seen = outcomes(all.iter().map(|s| s.answer.clone()));
    covers_the_failures(&seen, true);
    assert!(seen.iter().any(|s| s.contains("Inconsistent")));
    assert!(seen.iter().any(|s| s.contains("Cut")));
    for s in all {
        assert_eq!(s.op, "CodeHost::changes");
        assert!(matches!(s.actor, Actor::Ours(_)), "{}", s.fixture);
        let (mut gh, _) = adapter(&s.project);
        assert_eq!(
            gh.changes(&s.filter, s.bound),
            s.answer,
            "{}: {}",
            s.project,
            s.fixture
        );
    }
}

#[test]
fn the_checks_answer_as_each_sample_says() {
    let all: Vec<ChecksSample> = samples("code-host-checks.json");
    assert!(!all.is_empty());
    covers_the_failures(&outcomes(all.iter().map(|s| s.answer.clone())), true);
    for s in all {
        assert_eq!(s.op, "CodeHost::checks");
        assert!(matches!(s.actor, Actor::Ours(_)), "{}", s.fixture);
        let (mut gh, _) = adapter(&s.project);
        assert_eq!(
            gh.checks(&s.at, s.name.as_deref()),
            s.answer,
            "{}: {}",
            s.project,
            s.fixture
        );
    }
}

fn mine() -> ChangeFilter {
    ChangeFilter {
        states: ChangeStates::All,
        author: Some(Author::Me),
        ..ChangeFilter::default()
    }
}

#[test]
fn the_changes_keep_the_laws_of_lists() {
    for (project, held) in [("acme/widget", 4), ("acme/many", 2)] {
        let (mut gh, _) = adapter(project);
        let read = |b| gh.changes(&mine(), b);
        assert_eq!(
            laws::lists::past_the_bound_is_unread(read, held),
            Ok(()),
            "{project}"
        );
    }
}

#[test]
fn a_page_boundary_asks_the_next_page_by_its_cursor_and_stops_at_the_last() {
    let (mut gh, asked) = adapter("acme/many");
    let got = gh.changes(&mine(), Bound::new(2).unwrap()).unwrap();
    let numbers: Vec<u64> = got.items().iter().map(|d| d.no.get()).collect();
    assert_eq!(numbers, [100, 101]);
    let asked = asked.borrow();
    assert_eq!(asked.len(), 2);
    assert!(!asked[0].contains("after="));
    assert!(asked[1].contains("-f after=Y3Vyc29yOnYyOpHOAAAAZA=="));
}

#[test]
fn past_the_bound_stops_reading_at_once() {
    // The first page alone holds more than one of ours: no second request.
    let (mut gh, asked) = adapter("acme/widget");
    assert_eq!(
        gh.changes(&mine(), Bound::new(1).unwrap()),
        Err(Outcome::Unread(Unread::Cut))
    );
    assert_eq!(asked.borrow().len(), 1);
}

#[test]
fn the_question_narrows_by_state_and_branch_and_names_the_asker_only_for_me() {
    let (mut gh, asked) = adapter("acme/widget");
    let settle = ChangeFilter {
        head: Some(tehas_ports::Branch::new("tehas/issue-7").unwrap()),
        ..mine()
    };
    gh.changes(&settle, Bound::new(5).unwrap()).unwrap();
    let open = ChangeFilter::default();
    gh.changes(&open, Bound::new(10).unwrap()).unwrap();
    let asked = asked.borrow();
    assert!(asked[0].contains("-f head=tehas/issue-7"));
    assert!(asked[0].contains("states: [OPEN, CLOSED, MERGED]"));
    assert!(asked[0].contains("viewer { databaseId }"));
    assert!(!asked[1].contains("head="));
    assert!(asked[1].contains("states: [OPEN]"));
    assert!(!asked[1].contains("viewer"));
}

#[test]
fn a_query_that_fails_with_errors_is_settled_by_the_rest_list() {
    // gh exits 1 with no word of its own when GitHub's answer holds errors.
    // The REST list says whether the project is gone or not to be read;
    // when it reads, the list is unread: never an empty list.
    for (project, want) in [
        ("acme/gone", Outcome::NotFound),
        ("acme/locked", Outcome::Forbidden),
        ("acme/troubled", Outcome::Unread(Unread::Inconsistent)),
    ] {
        let (mut gh, asked) = adapter(project);
        let got = gh.changes(&ChangeFilter::default(), Bound::new(10).unwrap());
        assert_eq!(got, Err(want), "{project}");
        let asked = asked.borrow();
        assert_eq!(asked.len(), 2, "{project}");
        assert!(
            asked[1].ends_with("/pulls?per_page=1&state=all"),
            "{project}"
        );
    }
}

#[test]
fn a_change_the_filter_leaves_out_is_not_read_whole() {
    // Strangers' changes the factory's own list never takes: one closing
    // more issues than a page holds, one from a branch git refuses, one
    // merged with no merge commit. Each fails a list that takes it.
    let (mut gh, _) = adapter("acme/crowded");
    let got = gh.changes(&mine(), Bound::new(10).unwrap()).unwrap();
    let numbers: Vec<u64> = got.items().iter().map(|d| d.no.get()).collect();
    assert_eq!(numbers, [1]);
    let all = ChangeFilter {
        states: ChangeStates::All,
        ..ChangeFilter::default()
    };
    assert_eq!(
        gh.changes(&all, Bound::new(10).unwrap()),
        Err(Outcome::Unread(Unread::Cut))
    );
}

/// A runner that answers every question with a full page of 100 open pull
/// requests of `acme/vast`, which holds 1100, and says another follows.
struct Endless {
    asked: Asked,
}

impl GhRunner for Endless {
    fn run(&mut self, _: &[&str]) -> Result<String, String> {
        let page = self.asked.borrow().len() as u64;
        self.asked.borrow_mut().push(String::new());
        let nodes: Vec<String> = (1..=100)
            .map(|i| {
                let no = page * 100 + i;
                format!(
                    r#"{{"number":{no},"title":"Change {no}","state":"OPEN","isDraft":false,"isCrossRepository":false,"headRefName":"topic-{no}","headRefOid":"{sha}","baseRefName":"main","createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z","mergedAt":null,"closedAt":null,"lastEditedAt":null,"additions":1,"deletions":0,"changedFiles":1,"mergeable":"UNKNOWN","mergeStateStatus":"UNKNOWN","url":"https://github.com/acme/vast/pull/{no}","author":{{"__typename":"User","login":"alice","databaseId":101}},"editor":null,"mergedBy":null,"mergeCommit":null,"labels":{{"totalCount":0,"nodes":[]}},"closingIssuesReferences":{{"totalCount":0,"nodes":[]}}}}"#,
                    sha = "ab".repeat(20)
                )
            })
            .collect();
        Ok(format!(
            r#"{{"data":{{"viewer":{{"databaseId":900}},"repository":{{"databaseId":7020,"pullRequests":{{"totalCount":1100,"pageInfo":{{"hasNextPage":true,"endCursor":"page-{page}"}},"nodes":[{}]}}}}}}}}"#,
            nodes.join(",")
        ))
    }
}

#[test]
fn a_list_longer_than_ten_pages_is_cut_with_no_eleventh_request() {
    let asked = Asked::default();
    let runner = Endless {
        asked: Rc::clone(&asked),
    };
    let mut gh = GitHub::new("acme/vast", Box::new(runner));
    assert_eq!(
        gh.changes(&mine(), Bound::new(10).unwrap()),
        Err(Outcome::Unread(Unread::Cut))
    );
    assert_eq!(asked.borrow().len(), 10);
}

#[test]
fn a_name_no_mark_can_have_is_left_out_and_kept_once_to_report() {
    let (mut gh, _) = adapter("acme/widget");
    let all = ChangeFilter {
        states: ChangeStates::All,
        ..ChangeFilter::default()
    };
    gh.changes(&all, Bound::new(10).unwrap()).unwrap();
    gh.changes(&all, Bound::new(10).unwrap()).unwrap();
    assert_eq!(gh.take_left_out(), ["not a label?"]);
}

/// A runner no call may reach.
struct Unasked;

impl GhRunner for Unasked {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        panic!("asked {args:?}")
    }
}

#[test]
fn what_no_caller_asks_yet_is_unsupported_before_any_request() {
    let unsupported = Err(Outcome::Refused(Refusal::Unsupported));
    let mut gh = GitHub::new("acme/widget", Box::new(Unasked));
    let bodies = ChangeFilter {
        bodies: true,
        ..ChangeFilter::default()
    };
    let no = ChangeNo::new(20).unwrap();
    let bound = Bound::new(10).unwrap();
    assert_eq!(gh.changes(&bodies, bound).map(|_| ()), unsupported);
    assert_eq!(gh.change(no, bound).map(|_| ()), unsupported);
    assert_eq!(gh.changed_files(no, bound).map(|_| ()), unsupported);
    assert_eq!(
        gh.activity(no, Window::Last(std::num::NonZeroU32::MIN))
            .map(|_| ()),
        unsupported
    );
    assert_eq!(gh.remote(Access::Fetch).map(|_| ()), unsupported);
    assert_eq!(gh.tags(bound).map(|_| ()), unsupported);
    assert_eq!(gh.latest_release().map(|_| ()), unsupported);
}
