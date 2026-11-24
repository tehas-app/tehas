//! The code host port's reads as the adapter answers them: each sample in
//! `console/ports/samples/github/code-host-*.json` names a call, the
//! fixture project it reads and the answer it must give, and the adapter
//! answers it from its own recordings of that project, under
//! `tests/recorded/code-host/<project>/`, replayed through a runner. The
//! laws of `tehas_ports::laws` then check the same answers. Beside them, the
//! adapter's own cases: what it does not answer yet, and how gh's failures
//! of a GraphQL question read.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Deserialize;
use tehas_core::forge::{Failed, GhRunner};
use tehas_github::GitHub;
use tehas_ports::laws;
use tehas_ports::{
    Actor, At, Author, Bound, ChangeDigest, ChangeFilter, ChangeNo, ChangeStates, CheckStatus,
    Checks, CodeHost, CommitId, Conclusion, Facts, Got, Mark, MarkName, Outcome, Overall, Refusal,
    State, Unread, Whole, ALL_MAX,
};

fn here() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The time the replay's clock reads: a spent budget answers it as `until`.
fn replay_now() -> i64 {
    1_767_225_600
}

type Asked = Arc<Mutex<Vec<String>>>;

/// A runner that answers each call from the recording of the read it
/// makes: `<project>/<read>.json` is gh's answer, `<project>/<read>.fail`
/// the reason the call failed. A call with no recording fails.
struct Replay {
    asked: Asked,
}

/// The recording a call reads, named by the project and the read: `repo`,
/// `pulls` (the REST list that settles a failed question),
/// `checks-<commit>-<page>[-name=<name>]`, `statuses-<commit>-<page>`, or
/// `changes-<states>[-me][-head=<branch>][-after=<cursor>][-updated]`
/// (`-updated` for the question newest updated first), each byte
/// outside `[A-Za-z0-9=-]` written `_`, or `numbers-<first>-<last>[-me]`
/// for the question of pull requests by number, its first and last asked.
/// `<read>.answer.json` beside a `.fail` is what gh printed as it failed.
fn recording(args: &[&str]) -> Option<(String, String)> {
    let field = |name: &str| {
        args.iter()
            .find_map(|a| a.strip_prefix(&format!("{name}=")))
            .map(str::to_string)
    };
    let (project, read) = if args.get(1) == Some(&"graphql") {
        let query = field("query")?;
        let asked: Vec<&str> = query
            .split(": pullRequest(number: ")
            .skip(1)
            .filter_map(|r| r.split(')').next())
            .collect();
        if let (Some(first), Some(last)) = (asked.first(), asked.last()) {
            let me = if query.contains("viewer") { "-me" } else { "" };
            return Some((field("name")?, format!("numbers-{first}-{last}{me}")));
        }
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
        if query.contains("field: UPDATED_AT, direction: DESC") {
            read.push_str("-updated");
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
                if let Some((commit, page)) = rest.split_once("/status?per_page=100&page=") {
                    (project.to_string(), format!("statuses-{commit}-{page}"))
                } else {
                    let (commit, query) =
                        rest.split_once("/check-runs?filter=latest&per_page=100&page=")?;
                    let (page, name) = match query.split_once("&check_name=") {
                        Some((page, name)) => (page, format!("-name={name}")),
                        None => (query, String::new()),
                    };
                    (project.to_string(), format!("checks-{commit}-{page}{name}"))
                }
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
    fn run_answer(&mut self, args: &[&str]) -> Result<String, Failed> {
        let answer = recording(args).and_then(|(project, read)| {
            let at = here().join("tests/recorded/code-host").join(project);
            std::fs::read_to_string(at.join(format!("{read}.answer.json"))).ok()
        });
        self.run(args).map_err(|why| Failed { why, answer })
    }

    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        self.asked.lock().unwrap().push(args.join(" "));
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
        asked: Arc::clone(&asked),
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
        assert_eq!(asked.lock().unwrap().len(), 1, "{}", s.fixture);
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
    let asked = asked.lock().unwrap();
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
    assert_eq!(asked.lock().unwrap().len(), 1);
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
    let asked = asked.lock().unwrap();
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
        let asked = asked.lock().unwrap();
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
        let page = self.asked.lock().unwrap().len() as u64;
        self.asked.lock().unwrap().push(String::new());
        let nodes: Vec<String> = (1..=100)
            .map(|i| {
                let no = page * 100 + i;
                format!(
                    r#"{{"number":{no},"title":"Change {no}","state":"OPEN","isDraft":false,"isCrossRepository":false,"headRepository":{{"databaseId":7020}},"headRefName":"topic-{no}","headRefOid":"{sha}","baseRefName":"main","createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z","mergedAt":null,"closedAt":null,"lastEditedAt":null,"additions":1,"deletions":0,"changedFiles":1,"mergeable":"UNKNOWN","mergeStateStatus":"UNKNOWN","url":"https://github.com/acme/vast/pull/{no}","author":{{"__typename":"User","login":"alice","databaseId":101}},"editor":null,"mergedBy":null,"mergeCommit":null,"labels":{{"totalCount":0,"nodes":[]}},"closingIssuesReferences":{{"totalCount":0,"nodes":[]}}}}"#,
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
        asked: Arc::clone(&asked),
    };
    let mut gh = GitHub::new("acme/vast", Box::new(runner));
    assert_eq!(
        gh.changes(&mine(), Bound::new(10).unwrap()),
        Err(Outcome::Unread(Unread::Cut))
    );
    assert_eq!(asked.lock().unwrap().len(), 10);
}

/// A runner for `acme/huge`, 1500 pull requests, #n updated `n` minutes
/// into 2026, answered newest updated first, 100 a page, the cursor the
/// offset. Those in `merged` are merged at their updated time, the rest
/// open. With `moved` (#n, place, minutes), #n is updated after the second
/// call to that time and stands at that place from then on.
struct Huge {
    merged: Vec<u64>,
    moved: Option<(u64, usize, u64)>,
    asked: Asked,
}

fn iso(minutes: u64) -> String {
    format!(
        "2026-01-{:02}T{:02}:{:02}:00Z",
        1 + minutes / 1440,
        minutes % 1440 / 60,
        minutes % 60
    )
}

impl GhRunner for Huge {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        let calls = {
            let mut asked = self.asked.lock().unwrap();
            asked.push(args.join(" "));
            asked.len()
        };
        let mut list: Vec<(u64, u64)> = (1..=1500).rev().map(|n| (n, n)).collect();
        if let Some((no, place, at)) = self.moved.filter(|_| calls > 2) {
            list.retain(|(n, _)| *n != no);
            list.insert(place, (no, at));
        }
        let from: usize = args
            .iter()
            .find_map(|a| a.strip_prefix("after="))
            .map_or(0, |c| c.parse().unwrap());
        let to = (from + 100).min(list.len());
        let nodes: Vec<String> = list[from..to]
            .iter()
            .map(|&(no, at)| {
                let (state, merged, commit) = match self.merged.contains(&no) {
                    true => ("MERGED", format!("\"{}\"", iso(at)), format!(r#"{{"oid":"{}"}}"#, "cd".repeat(20))),
                    false => ("OPEN", "null".to_string(), "null".to_string()),
                };
                format!(
                    r#"{{"number":{no},"title":"Change {no}","state":"{state}","isDraft":false,"isCrossRepository":false,"headRepository":{{"databaseId":7030}},"headRefName":"topic-{no}","headRefOid":"{sha}","baseRefName":"main","baseRefOid":"{sha}","createdAt":"2026-01-01T00:00:00Z","updatedAt":"{updated}","mergedAt":{merged},"closedAt":null,"lastEditedAt":null,"additions":1,"deletions":0,"changedFiles":1,"mergeable":"UNKNOWN","mergeStateStatus":"UNKNOWN","url":"https://github.com/acme/huge/pull/{no}","author":{{"__typename":"User","login":"alice","databaseId":101}},"editor":null,"mergedBy":null,"mergeCommit":{commit},"labels":{{"totalCount":0,"nodes":[]}},"closingIssuesReferences":{{"totalCount":0,"nodes":[]}}}}"#,
                    sha = "ab".repeat(20),
                    updated = iso(at)
                )
            })
            .collect();
        Ok(format!(
            r#"{{"data":{{"repository":{{"databaseId":7030,"pullRequests":{{"totalCount":1500,"pageInfo":{{"hasNextPage":{more},"endCursor":"{to}"}},"nodes":[{}]}}}}}}}}"#,
            nodes.join(","),
            more = to < list.len()
        ))
    }
}

/// The merged changes of `acme/huge` since `minutes` into 2026, the
/// numbers answered and the calls made.
fn merged_since(
    merged: &[u64],
    minutes: i64,
    moved: Option<(u64, usize, u64)>,
) -> (Got<Vec<u64>>, Vec<String>) {
    let asked = Asked::default();
    let runner = Huge {
        merged: merged.to_vec(),
        moved,
        asked: Arc::clone(&asked),
    };
    let mut gh = GitHub::new("acme/huge", Box::new(runner));
    let filter = ChangeFilter {
        states: ChangeStates::Merged,
        merged_since: Some(At::new(1_767_225_600 + minutes * 60)),
        ..ChangeFilter::default()
    };
    let got = gh
        .changes(&filter, Bound::new(10).unwrap())
        .map(|w| w.items().iter().map(|d| d.no.get()).collect());
    let asked = asked.lock().unwrap().clone();
    (got, asked)
}

#[test]
fn a_recent_window_of_a_long_list_is_answered_whole_from_the_first_page() {
    let (got, asked) = merged_since(&[1498, 1490, 1495, 3], 1489, None);
    // Read newest first, answered in the order of their numbers.
    assert_eq!(got, Ok(vec![1490, 1495, 1498]));
    assert_eq!(asked.len(), 1);
    assert!(asked[0].contains("orderBy: {field: UPDATED_AT, direction: DESC}"));
}

#[test]
fn a_window_that_ends_at_a_page_boundary_reads_the_next_page_and_the_first_again() {
    let (got, asked) = merged_since(&[1500, 1450, 1401, 1400], 1401, None);
    assert_eq!(got, Ok(vec![1401, 1450, 1500]));
    assert_eq!(asked.len(), 3);
    assert!(asked[1].contains("-f after=100"));
    assert_eq!(asked[2], asked[0]);
}

#[test]
fn a_change_updated_while_the_pages_are_read_is_inconsistent() {
    let (got, _) = merged_since(&[1500, 7], 1401, Some((7, 0, 2000)));
    assert_eq!(got, Err(Outcome::Unread(Unread::Inconsistent)));
}

#[test]
fn a_change_that_ties_the_top_and_lands_second_while_the_pages_are_read_is_inconsistent() {
    // #7 is updated in the same minute as #1500 and GitHub keeps #1500 first.
    let (got, _) = merged_since(&[1500, 7], 1401, Some((7, 1, 1500)));
    assert_eq!(got, Err(Outcome::Unread(Unread::Inconsistent)));
}

#[test]
fn a_change_already_read_that_moves_up_while_the_pages_are_read_is_not_inconsistent() {
    let (got, asked) = merged_since(&[1500, 1450, 1401], 1401, Some((1450, 0, 2000)));
    assert_eq!(got, Ok(vec![1401, 1450, 1500]));
    assert_eq!(asked.len(), 3);
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
    let no = ChangeNo::new(20).unwrap();
    let bound = Bound::new(10).unwrap();
    assert_eq!(gh.changed_files(no, bound).map(|_| ()), unsupported);
    assert_eq!(gh.tags(bound).map(|_| ()), unsupported);
    assert_eq!(gh.latest_release().map(|_| ()), unsupported);
}

/// A runner that answers every call with `answer`, writing each down.
struct Answers {
    answer: String,
    asked: Asked,
}

impl GhRunner for Answers {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        self.asked.lock().unwrap().push(args.join(" "));
        Ok(self.answer.clone())
    }
}

fn answering(project: &str, answer: &str) -> (GitHub, Asked) {
    let asked = Asked::default();
    let runner = Answers {
        answer: answer.to_string(),
        asked: Arc::clone(&asked),
    };
    (GitHub::new(project, Box::new(runner)), asked)
}

/// A runner that answers each call in turn from `answers`, writing each
/// down; a call past them fails.
struct Script {
    answers: Vec<Result<String, String>>,
    asked: Asked,
}

impl GhRunner for Script {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        let mut asked = self.asked.lock().unwrap();
        asked.push(args.join(" "));
        self.answers
            .get(asked.len() - 1)
            .cloned()
            .unwrap_or_else(|| Err("no answer".to_string()))
    }
}

fn scripted(answers: &[Result<String, String>]) -> (GitHub, Asked) {
    let asked = Asked::default();
    let runner = Script {
        answers: answers.to_vec(),
        asked: Arc::clone(&asked),
    };
    (GitHub::new("acme/widget", Box::new(runner)), asked)
}

/// A site's check runs page: each run as `(id, App id, name, status,
/// conclusion)`, and the count it says it holds.
fn runs_page(total: usize, runs: &[(u64, u64, &str, &str, Option<&str>)]) -> String {
    let runs: Vec<serde_json::Value> = runs
        .iter()
        .map(|(id, app, name, status, conclusion)| {
            serde_json::json!({"id": id, "app": {"id": app}, "name": name, "status": status, "conclusion": conclusion})
        })
        .collect();
    serde_json::json!({"total_count": total, "check_runs": runs}).to_string()
}

/// A site's combined status page: each context and its newest state, and
/// the count it says it holds.
fn statuses_page(total: usize, statuses: &[(&str, &str)]) -> String {
    let statuses: Vec<serde_json::Value> = statuses
        .iter()
        .map(|(context, state)| serde_json::json!({"context": context, "state": state}))
        .collect();
    serde_json::json!({"state": "pending", "total_count": total, "statuses": statuses}).to_string()
}

fn at() -> CommitId {
    CommitId::new("e".repeat(40)).unwrap()
}

/// The checks of a commit whose runs and statuses each fit one page.
fn checks_of(runs: &[(u64, u64, &str, &str, Option<&str>)], statuses: &[(&str, &str)]) -> Checks {
    let (mut gh, asked) = scripted(&[
        Ok(runs_page(runs.len(), runs)),
        Ok(statuses_page(statuses.len(), statuses)),
    ]);
    let got = gh.checks(&at(), None).unwrap();
    assert_eq!(asked.lock().unwrap().len(), 2);
    got
}

fn runs(json: &str) -> Checks {
    let (mut gh, _) = scripted(&[Ok(json.to_string()), Ok(statuses_page(0, &[]))]);
    gh.checks(&at(), None).unwrap()
}

fn statuses(c: &Checks) -> Vec<CheckStatus> {
    c.runs.iter().map(|r| r.status).collect()
}

fn names(c: &Checks) -> Vec<&str> {
    c.runs.iter().map(|r| r.name.as_str()).collect()
}

const PASS: (u64, u64, &str, &str, Option<&str>) =
    (1, 15368, "build", "completed", Some("success"));

#[test]
fn a_cancelled_run_beside_one_running_is_pending() {
    let c = runs(
        r#"{"total_count":2,"check_runs":[{"id":1,"name":"e2e","status":"completed","conclusion":"cancelled"},{"id":2,"name":"build","status":"in_progress","conclusion":null}]}"#,
    );
    assert_eq!(c.overall, Overall::Pending);
}

#[test]
fn runs_all_done_cancelled_or_unknown_with_none_failed_are_unknown_never_a_fail_or_a_pass() {
    let c = runs(
        r#"{"total_count":3,"check_runs":[{"id":1,"name":"build","status":"completed","conclusion":"success"},{"id":2,"name":"e2e","status":"completed","conclusion":"cancelled"},{"id":3,"name":"deploy","status":"completed","conclusion":"stale"}]}"#,
    );
    assert_eq!(
        statuses(&c),
        [
            CheckStatus::Done(Conclusion::Passed),
            CheckStatus::Done(Conclusion::Cancelled),
            CheckStatus::Done(Conclusion::Unknown)
        ]
    );
    assert_eq!(c.overall, Overall::Unknown);
    assert_eq!(c.overall, Overall::of(&c.runs));
}

#[test]
fn a_neutral_run_passes() {
    let c = runs(
        r#"{"total_count":1,"check_runs":[{"id":1,"name":"advice","status":"completed","conclusion":"neutral"}]}"#,
    );
    assert_eq!(statuses(&c), [CheckStatus::Done(Conclusion::Passed)]);
    assert_eq!(c.overall, Overall::Pass);
}

#[test]
fn the_cheapest_checks_read_is_two_requests_the_newest_runs_and_the_combined_status() {
    let (mut gh, asked) = scripted(&[Ok(runs_page(0, &[])), Ok(statuses_page(0, &[]))]);
    gh.checks(&at(), Some("test")).unwrap();
    assert_eq!(
        *asked.lock().unwrap(),
        [
            format!(
                "api repos/acme/widget/commits/{}/check-runs?filter=latest&per_page=100&page=1&check_name=test",
                at().as_str()
            ),
            format!(
                "api repos/acme/widget/commits/{}/status?per_page=100&page=1",
                at().as_str()
            ),
        ]
    );
}

#[test]
fn a_failing_status_beside_a_passing_run_fails_the_checks() {
    for state in ["failure", "error"] {
        let c = checks_of(&[PASS], &[("ci/legacy", state)]);
        assert_eq!(names(&c), ["build", "ci/legacy"]);
        assert_eq!(
            statuses(&c),
            [
                CheckStatus::Done(Conclusion::Passed),
                CheckStatus::Done(Conclusion::Failed)
            ]
        );
        assert_eq!(c.overall, Overall::Fail, "{state}");
    }
}

#[test]
fn a_pending_status_keeps_the_checks_pending() {
    let c = checks_of(&[PASS], &[("deploy/preview", "pending")]);
    assert_eq!(
        statuses(&c),
        [CheckStatus::Done(Conclusion::Passed), CheckStatus::Running]
    );
    assert_eq!(c.overall, Overall::Pending);
}

#[test]
fn statuses_alone_are_the_checks() {
    let c = checks_of(&[], &[("ci/legacy", "success"), ("lint", "success")]);
    assert_eq!(names(&c), ["ci/legacy", "lint"]);
    assert_eq!(c.overall, Overall::Pass);
}

#[test]
fn runs_alone_are_the_checks() {
    let c = checks_of(&[PASS], &[]);
    assert_eq!(names(&c), ["build"]);
    assert_eq!(c.overall, Overall::Pass);
}

#[test]
fn no_runs_and_no_statuses_are_none() {
    let c = checks_of(&[], &[]);
    assert!(c.runs.is_empty());
    assert_eq!(c.overall, Overall::None);
}

#[test]
fn a_failed_statuses_request_fails_the_read_never_an_answer_from_runs_alone() {
    let failed = "HTTP 403: Resource not accessible by integration".to_string();
    let (mut gh, asked) = scripted(&[Ok(runs_page(1, &[PASS])), Err(failed.clone())]);
    let got = gh.checks(&at(), None);
    assert_eq!(asked.lock().unwrap().len(), 2);
    let (mut alone, _) = scripted(&[Err(failed)]);
    let want = alone.checks(&at(), None);
    assert!(want.is_err());
    assert_eq!(got, want);
}

/// The contexts `ctx-1` to `ctx-<n>`, each passed.
fn contexts(from: usize, to: usize) -> Vec<String> {
    (from..=to).map(|i| format!("ctx-{i}")).collect()
}

fn passed(names: &[String]) -> Vec<(&str, &str)> {
    names.iter().map(|n| (n.as_str(), "success")).collect()
}

#[test]
fn statuses_over_two_pages_are_read_whole() {
    let (first, second) = (contexts(1, 100), contexts(101, 101));
    let (mut gh, asked) = scripted(&[
        Ok(runs_page(0, &[])),
        Ok(statuses_page(101, &passed(&first))),
        Ok(statuses_page(101, &passed(&second))),
    ]);
    let c = gh.checks(&at(), None).unwrap();
    assert_eq!(c.runs.len(), 101);
    assert_eq!(c.overall, Overall::Pass);
    let asked = asked.lock().unwrap();
    assert_eq!(asked.len(), 3);
    assert!(
        asked[2].ends_with("/status?per_page=100&page=2"),
        "{}",
        asked[2]
    );
}

#[test]
fn statuses_that_move_between_pages_are_inconsistent() {
    let (first, second) = (contexts(1, 100), contexts(100, 101));
    for page2 in [
        statuses_page(102, &passed(&contexts(101, 101))),
        statuses_page(101, &passed(&second)),
        statuses_page(101, &[]),
    ] {
        let (mut gh, _) = scripted(&[
            Ok(runs_page(0, &[])),
            Ok(statuses_page(101, &passed(&first))),
            Ok(page2),
        ]);
        assert_eq!(
            gh.checks(&at(), None),
            Err(Outcome::Unread(Unread::Inconsistent))
        );
    }
}

#[test]
fn one_apps_failure_is_never_hidden_by_another_apps_pass() {
    let c = checks_of(
        &[
            (1, 15368, "test", "completed", Some("failure")),
            (2, 99001, "test", "completed", Some("success")),
        ],
        &[],
    );
    assert_eq!(names(&c), ["test", "test"]);
    assert_eq!(c.overall, Overall::Fail);
}

#[test]
fn a_status_named_like_a_run_stays_beside_it() {
    let c = checks_of(&[PASS], &[("build", "failure")]);
    assert_eq!(names(&c), ["build", "build"]);
    assert_eq!(c.overall, Overall::Fail);
}

#[test]
fn asked_by_name_the_status_of_that_context_alone_is_held_beside_the_runs_of_that_name() {
    let (mut gh, _) = scripted(&[
        Ok(runs_page(1, &[PASS])),
        Ok(statuses_page(
            2,
            &[("build", "pending"), ("lint", "failure")],
        )),
    ]);
    let c = gh.checks(&at(), Some("build")).unwrap();
    assert_eq!(names(&c), ["build", "build"]);
    assert_eq!(c.overall, Overall::Pending);
}

/// A site that answers each read of the check runs and the combined status
/// of one commit from its runs and statuses, the runs asked by name filtered
/// as `check_name` does.
struct Site {
    runs: Vec<(u64, u64, &'static str, &'static str, Option<&'static str>)>,
    statuses: Vec<(&'static str, &'static str)>,
}

impl GhRunner for Site {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        let path = args[1];
        if path.contains("/status?") {
            return Ok(statuses_page(self.statuses.len(), &self.statuses));
        }
        let runs: Vec<_> = match path.split_once("&check_name=") {
            Some((_, name)) => self.runs.iter().filter(|r| r.2 == name).copied().collect(),
            None => self.runs.clone(),
        };
        Ok(runs_page(runs.len(), &runs))
    }
}

#[test]
fn the_checks_keep_the_three_laws_of_runs_statuses_and_apps() {
    let site = |runs, statuses| GitHub::new("acme/widget", Box::new(Site { runs, statuses }));
    // filter=latest: GitHub gives the newest run of each App and name.
    let mut gh = site(
        vec![
            (3, 15368, "test", "completed", Some("success")),
            (4, 15368, "lint", "in_progress", None),
        ],
        vec![("deploy", "pending")],
    );
    assert_eq!(
        laws::code_host::checks_hold_the_newest_run_of_each_app_and_name(
            &mut gh,
            &at(),
            "test",
            &CheckStatus::Done(Conclusion::Passed)
        ),
        Ok(())
    );
    let mut gh = site(
        vec![
            (1, 15368, "test", "completed", Some("failure")),
            (2, 99001, "test", "completed", Some("success")),
        ],
        vec![],
    );
    assert_eq!(
        laws::code_host::two_apps_runs_of_one_name_are_both_held(&mut gh, &at(), "test"),
        Ok(())
    );
    let mut gh = site(vec![PASS], vec![("ci/legacy", "error")]);
    assert_eq!(
        laws::code_host::a_failed_status_beside_a_passing_run_fails(
            &mut gh,
            &at(),
            "build",
            "ci/legacy"
        ),
        Ok(())
    );
}

const REPO: &str = r#"{"full_name":"ACME/Widget","private":false,"has_issues":true,"archived":false,"default_branch":"main"}"#;

#[test]
fn the_facts_name_the_configured_spelling_where_github_differs_in_case_alone() {
    let (mut gh, _) = answering("acme/widget", REPO);
    assert_eq!(gh.facts().unwrap().name.as_str(), "acme/widget");
}

#[test]
fn the_facts_name_githubs_spelling_where_the_project_moved() {
    let (mut gh, _) = answering("acme/gadget", REPO);
    assert_eq!(gh.facts().unwrap().name.as_str(), "ACME/Widget");
}

fn every_change() -> String {
    let at = here().join("tests/recorded/code-host/widget/changes-OPEN_CLOSED_MERGED.json");
    std::fs::read_to_string(at).unwrap()
}

#[test]
fn a_filter_carrying_marks_asks_their_labels_and_takes_the_changes_carrying_every_one() {
    let (mut gh, asked) = answering("acme/widget", &every_change());
    let bug = Mark::Project(MarkName::new("bug").unwrap());
    let filter = ChangeFilter {
        states: ChangeStates::All,
        carrying: vec![bug.clone()],
        ..ChangeFilter::default()
    };
    let got = gh.changes(&filter, Bound::new(10).unwrap()).unwrap();
    let nos: Vec<u64> = got.items().iter().map(|d| d.no.get()).collect();
    assert_eq!(nos, [22]);
    assert!(asked.lock().unwrap()[0].contains(r#"labels: ["bug"]"#));

    let (mut gh, _) = answering("acme/widget", &every_change());
    let both = ChangeFilter {
        states: ChangeStates::All,
        carrying: vec![bug, Mark::State(State::Refreshing)],
        ..ChangeFilter::default()
    };
    let got = gh.changes(&both, Bound::new(10).unwrap()).unwrap();
    assert!(got.items().is_empty(), "{:?}", got.items());
}

#[test]
fn a_filter_carrying_a_mark_no_label_can_be_is_invalid_before_any_request() {
    let mut gh = GitHub::new("acme/widget", Box::new(Unasked));
    let filter = ChangeFilter {
        carrying: vec![Mark::Unknown],
        ..ChangeFilter::default()
    };
    assert_eq!(
        gh.changes(&filter, Bound::new(10).unwrap()).map(|_| ()),
        Err(Outcome::Refused(Refusal::Invalid))
    );
}

#[test]
fn an_unknown_status_state_alone_is_unknown_never_a_pass() {
    let c = checks_of(&[], &[("ci/legacy", "queued_new")]);
    assert_eq!(statuses(&c), [CheckStatus::Done(Conclusion::Unknown)]);
    assert_ne!(c.overall, Overall::Pass);
    assert_eq!(c.overall, Overall::Unknown);
}

#[test]
fn an_unknown_status_word_beside_passing_runs_is_not_a_pass() {
    let c = checks_of(
        &[PASS, (2, 15368, "test", "completed", Some("success"))],
        &[("ci/legacy", "queued_new")],
    );
    assert_eq!(
        statuses(&c),
        [
            CheckStatus::Done(Conclusion::Passed),
            CheckStatus::Done(Conclusion::Passed),
            CheckStatus::Done(Conclusion::Unknown)
        ]
    );
    assert_ne!(c.overall, Overall::Pass);
    assert_eq!(c.overall, Overall::Unknown);
}

/// The runs `first..=last`, each a passing run of its own name.
fn passing_runs(first: u64, last: u64) -> Vec<(u64, u64, String)> {
    (first..=last)
        .map(|i| (i, 15368, format!("shard-{i}")))
        .collect()
}

fn runs_page_of(total: usize, runs: &[(u64, u64, String)]) -> String {
    let runs: Vec<_> = runs
        .iter()
        .map(|(id, app, name)| (*id, *app, name.as_str(), "completed", Some("success")))
        .collect();
    runs_page(total, &runs)
}

#[test]
fn runs_alone_at_all_max_are_read_whole() {
    let all = ALL_MAX.get() as u64;
    let mut answers = vec![
        Ok(runs_page_of(all as usize, &passing_runs(1, 100))),
        Ok(statuses_page(0, &[])),
    ];
    for page in 1..all / 100 {
        answers.push(Ok(runs_page_of(
            all as usize,
            &passing_runs(page * 100 + 1, page * 100 + 100),
        )));
    }
    let (mut gh, asked) = scripted(&answers);
    let c = gh.checks(&at(), None).unwrap();
    assert_eq!(c.runs.len() as u64, all);
    assert_eq!(c.overall, Overall::Pass);
    assert_eq!(asked.lock().unwrap().len(), answers.len());
}

#[test]
fn runs_and_contexts_together_one_past_all_max_are_cut() {
    let all = ALL_MAX.get() as usize;
    for (runs, contexts) in [(all, 1), (all - 100, 101), (1, all)] {
        let (mut gh, _) = scripted(&[
            Ok(runs_page_of(runs, &passing_runs(1, runs.min(100) as u64))),
            Ok(statuses_page(
                contexts,
                &passed(&contexts_named(contexts.min(100))),
            )),
        ]);
        assert_eq!(
            gh.checks(&at(), None),
            Err(Outcome::Unread(Unread::Cut)),
            "{runs} runs and {contexts} contexts"
        );
    }
}

#[test]
fn a_cut_shown_by_the_first_pages_counts_reads_no_further_page() {
    let all = ALL_MAX.get() as usize;
    let (mut gh, asked) = scripted(&[
        Ok(runs_page_of(all, &passing_runs(1, 100))),
        Ok(statuses_page(1, &[("ci/legacy", "success")])),
    ]);
    assert_eq!(gh.checks(&at(), None), Err(Outcome::Unread(Unread::Cut)));
    assert_eq!(asked.lock().unwrap().len(), 2);
}

fn contexts_named(n: usize) -> Vec<String> {
    contexts(1, n)
}

fn by_number(numbers: &[u64]) -> ChangeFilter {
    ChangeFilter {
        states: ChangeStates::All,
        numbers: numbers.iter().map(|n| ChangeNo::new(*n).unwrap()).collect(),
        ..ChangeFilter::default()
    }
}

fn nos(got: Got<Whole<ChangeDigest>>) -> Vec<u64> {
    got.unwrap()
        .into_items()
        .iter()
        .map(|d| d.no.get())
        .collect()
}

#[test]
fn numbers_are_asked_by_number_in_one_question_with_the_lists_digests() {
    let (mut gh, asked) = adapter("acme/widget");
    let mut all = ChangeFilter {
        states: ChangeStates::All,
        ..ChangeFilter::default()
    };
    let listed = gh.changes(&all, Bound::new(10).unwrap()).unwrap();
    all.numbers = listed.items().iter().map(|d| d.no).collect();
    asked.lock().unwrap().clear();
    let numbered = gh.changes(&all, Bound::new(10).unwrap()).unwrap();
    assert_eq!(numbered, listed);
    let asked = asked.lock().unwrap();
    assert_eq!(asked.len(), 1);
    assert!(asked[0].contains(" n19: pullRequest(number: 19) { number "));
    assert!(!asked[0].contains("pullRequests("));
}

#[test]
fn a_project_with_more_pull_requests_than_the_list_reads_is_not_cut() {
    let (mut gh, asked) = adapter("acme/big");
    let numbers: Vec<u64> = (1..=150).rev().collect();
    let got = gh.changes(&by_number(&numbers), Bound::new(150).unwrap());
    assert_eq!(nos(got), (1..=150).collect::<Vec<_>>());
    assert_eq!(asked.lock().unwrap().len(), 2);
    // More taken than the bound is cut; more numbers than ALL_MAX are cut
    // before any question.
    let (mut gh, _) = adapter("acme/big");
    let cut = gh.changes(&by_number(&numbers), Bound::new(149).unwrap());
    assert_eq!(cut, Err(Outcome::Unread(Unread::Cut)));
    let (mut gh, asked) = adapter("acme/big");
    let many: Vec<u64> = (1..=u64::from(ALL_MAX.get()) + 1).collect();
    let cut = gh.changes(&by_number(&many), Bound::new(10).unwrap());
    assert_eq!(cut, Err(Outcome::Unread(Unread::Cut)));
    assert!(asked.lock().unwrap().is_empty());
}

#[test]
fn the_rest_of_the_filter_applies_to_what_the_numbers_answer() {
    let read = |f: ChangeFilter| {
        nos(adapter("acme/widget")
            .0
            .changes(&f, Bound::new(10).unwrap()))
    };
    let both = || by_number(&[21, 22]);
    assert_eq!(read(both()), [21, 22]);
    assert_eq!(
        read(ChangeFilter {
            states: ChangeStates::Open,
            ..both()
        }),
        Vec::<u64>::new()
    );
    assert_eq!(
        read(ChangeFilter {
            states: ChangeStates::Closed,
            ..both()
        }),
        [22]
    );
    let bob = Author::Account(tehas_ports::AccountId::new("102"));
    assert_eq!(
        read(ChangeFilter {
            author: Some(bob),
            ..both()
        }),
        [22]
    );
    assert_eq!(
        read(ChangeFilter {
            author: Some(Author::Me),
            ..both()
        }),
        [21]
    );
    let fix = tehas_ports::Branch::new("fix-typo").unwrap();
    assert_eq!(
        read(ChangeFilter {
            head: Some(fix),
            ..both()
        }),
        [22]
    );
    let bug = Mark::Project(MarkName::new("bug").unwrap());
    assert_eq!(
        read(ChangeFilter {
            carrying: vec![bug],
            ..both()
        }),
        [22]
    );
    let eight = tehas_ports::BlueprintNo::new(8).unwrap();
    assert_eq!(
        read(ChangeFilter {
            closing: vec![eight],
            ..both()
        }),
        [21]
    );
    let since = Some(At::new(0));
    assert_eq!(
        read(ChangeFilter {
            merged_since: since,
            ..both()
        }),
        [21]
    );
}

#[test]
fn a_number_github_does_not_know_is_left_out() {
    let (mut gh, asked) = adapter("acme/widget");
    let got = gh.changes(&by_number(&[99, 21]), Bound::new(10).unwrap());
    assert_eq!(nos(got), [21]);
    assert_eq!(asked.lock().unwrap().len(), 1);
}
