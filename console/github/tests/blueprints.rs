//! The blueprints port as the adapter answers it: the search, against the
//! ports' recorded samples in `console/ports/samples/github/` and the laws
//! of searches, answered from the adapter's recordings of the fixture
//! project's issue search under `tests/recorded/search/`. What it does not
//! answer yet is refused before any request.

use serde::Deserialize;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::num::{NonZeroU32, NonZeroU64};
use std::path::PathBuf;
use std::rc::Rc;
use tehas_core::forge::GhRunner;
use tehas_github::GitHub;
use tehas_ports::laws;
use tehas_ports::{
    Actor, BlueprintHit, BlueprintNo, Blueprints, Bound, CommentId, Depth, Got, Outcome, Pool,
    Refusal, Took, Unread, Within,
};

const PROJECT: &str = "acme/widget";

fn here() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

type Asked = Rc<RefCell<Vec<String>>>;

/// A runner that answers each page of a search from its recording:
/// `index.json` lists the recorded pages of each question, in order; a
/// `.json` page is gh's answer, a `.fail` page the reason the call failed.
/// A call with no recording fails. Every call is written down.
struct Replay {
    asked: Asked,
}

impl GhRunner for Replay {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        self.asked.borrow_mut().push(args.join(" "));
        let field = |name: &str| {
            args.iter()
                .find_map(|a| a.strip_prefix(&format!("{name}=")))
                .ok_or_else(|| format!("no {name} in {args:?}"))
        };
        if args[..4] != ["api", "-X", "GET", "search/issues"] || field("per_page")? != "100" {
            return Err(format!("no recording of {args:?}"));
        }
        let (q, page) = (field("q")?, field("page")?);
        let at = here().join("tests/recorded/search");
        let index: BTreeMap<String, Vec<String>> =
            serde_json::from_str(&std::fs::read_to_string(at.join("index.json")).unwrap()).unwrap();
        let pages = index.get(q).ok_or_else(|| format!("no recording of {q}"))?;
        let file = page
            .parse::<usize>()
            .ok()
            .and_then(|n| pages.get(n.checked_sub(1)?))
            .ok_or_else(|| format!("no recording of page {page} of {q}"))?;
        let out = std::fs::read_to_string(at.join(file)).unwrap();
        if file.ends_with(".fail") {
            Err(out.trim_end().to_string())
        } else {
            Ok(out)
        }
    }
}

/// The time the replay's clock reads: a spent budget answers it as `until`.
fn replay_now() -> i64 {
    1_767_225_600
}

fn adapter() -> (GitHub, Asked) {
    let asked = Asked::default();
    let replay = Replay {
        asked: Rc::clone(&asked),
    };
    let gh = GitHub::new(PROJECT, Box::new(replay)).on_clock(replay_now);
    (gh, asked)
}

fn n(k: u32) -> NonZeroU32 {
    NonZeroU32::new(k).unwrap()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Search {
    op: String,
    actor: Actor,
    fixture: String,
    words: String,
    within: Within,
    open: Option<bool>,
    first: NonZeroU32,
    answer: Got<Took<BlueprintHit>>,
}

fn samples() -> Vec<Search> {
    let at = here().join("../ports/samples/github/blueprints-search.json");
    let text = std::fs::read_to_string(&at).unwrap_or_else(|e| panic!("{}: {e}", at.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", at.display()))
}

#[test]
fn the_search_answers_as_each_sample_says() {
    let all = samples();
    assert!(!all.is_empty());
    for s in all {
        assert_eq!(s.op, "Blueprints::search");
        assert!(matches!(s.actor, Actor::Ours(_)), "{}", s.fixture);
        let (mut gh, _) = adapter();
        let got = gh.search(&s.words, s.within, s.open, s.first);
        assert_eq!(
            got, s.answer,
            "{:?} {:?} {:?} first {}: {}",
            s.words, s.within, s.open, s.first, s.fixture
        );
    }
}

#[test]
fn every_failure_has_a_sample() {
    let answers: Vec<_> = samples().into_iter().map(|s| s.answer).collect();
    let has = |f: &dyn Fn(&Outcome) -> bool| answers.iter().any(|a| a.as_ref().is_err_and(f));
    assert!(has(&|o| *o == Outcome::NotFound));
    assert!(has(&|o| *o == Outcome::Forbidden));
    assert!(has(&|o| matches!(o, Outcome::Spent { .. })));
    assert!(has(&|o| matches!(o, Outcome::Unavailable { .. })));
    assert!(has(&|o| matches!(o, Outcome::Unread(_))));
    assert!(has(&|o| matches!(o, Outcome::Refused(_))));
}

#[test]
fn the_search_keeps_the_laws_of_searches() {
    let (mut gh, _) = adapter();
    assert_eq!(
        laws::search::first_n_by_relevance(|k| gh.search("alpha", Within::Titles, None, k), 3),
        Ok(())
    );
    assert_eq!(
        laws::search::first_n_by_relevance(|k| gh.search("report", Within::Titles, None, k), 103),
        Ok(())
    );
    assert_eq!(
        laws::search::titles_only_matches_titles(
            |within| gh.search("alpha", within, None, n(50)),
            |h: &BlueprintHit| h.title.as_str(),
            "alpha",
        ),
        Ok(())
    );
}

#[test]
fn words_that_spell_githubs_qualifiers_never_widen_the_search() {
    // Each hit's title and text as the fixture project holds them.
    let texts: BTreeMap<u64, &str> = [
        (
            26,
            "Search for label:bug and is:pr words, repo:acme/other too\nTitles hold the words.",
        ),
        (
            27,
            "Body words\nThis text says label:bug, is:pr and repo:acme/other.",
        ),
    ]
    .into();
    let (mut gh, _) = adapter();
    assert_eq!(
        laws::search::words_never_widen(
            |words, within| gh.search(words, within, None, n(50)),
            |h: &BlueprintHit| h.title.as_str(),
            |h: &BlueprintHit| texts.get(&h.no.get()).copied().unwrap_or("").to_string(),
            &["label:bug", "repo:acme/other", "is:pr"],
        ),
        Ok(())
    );
}

#[test]
fn a_search_asks_for_this_projects_issues_with_each_word_quoted() {
    let (mut gh, asked) = adapter();
    let _ = gh.search("Fix \"label:bug\"  now", Within::Titles, Some(true), n(5));
    let _ = gh.search("alpha", Within::Everything, Some(false), n(5));
    assert_eq!(
        *asked.borrow(),
        [
            r#"api -X GET search/issues -f q=repo:acme/widget is:issue in:title "fix" "label:bug" "now" is:open -f per_page=100 -f page=1"#,
            r#"api -X GET search/issues -f q=repo:acme/widget is:issue in:title,body "alpha" is:closed -f per_page=100 -f page=1"#,
        ]
    );
}

#[test]
fn a_search_reads_the_next_page_only_when_it_needs_one_more_hit() {
    for (first, pages) in [(1, 1), (99, 1), (100, 2), (103, 2), (500, 2)] {
        let (mut gh, asked) = adapter();
        gh.search("report", Within::Titles, None, n(first)).unwrap();
        assert_eq!(asked.borrow().len(), pages, "first {first}");
    }
}

#[test]
fn a_search_with_no_words_or_words_github_cannot_search_asks_nothing() {
    let (mut gh, asked) = adapter();
    assert_eq!(
        gh.search(" \n ", Within::Everything, None, n(5)),
        Ok(Took::default())
    );
    assert_eq!(
        gh.search(&"long ".repeat(60), Within::Titles, None, n(5)),
        Err(Outcome::Refused(Refusal::Invalid))
    );
    assert_eq!(
        gh.search("\" \"\"", Within::Titles, None, n(5)),
        Err(Outcome::Refused(Refusal::Invalid))
    );
    assert!(asked.borrow().is_empty());
}

#[test]
fn a_spent_search_is_the_search_pools() {
    let (mut gh, _) = adapter();
    assert!(matches!(
        gh.search("busy", Within::Everything, None, n(5)),
        Err(Outcome::Spent {
            pool: Pool::Search,
            ..
        })
    ));
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
    let mut gh = GitHub::new(PROJECT, Box::new(Unasked));
    let no = BlueprintNo::from(NonZeroU64::new(7).unwrap());
    let bound = Bound::new(10).unwrap();
    let id = CommentId::new("1");
    let unsupported = Outcome::Refused(Refusal::Unsupported);
    assert_eq!(
        gh.blueprint(no, Depth::Cut { notes: 0 }),
        Err(unsupported.clone())
    );
    assert_eq!(gh.several(&[no], Depth::All)[0].1, Err(unsupported.clone()));
    assert_eq!(gh.comments(no, bound), Err(unsupported.clone()));
    assert_eq!(
        gh.comments_by_id(std::slice::from_ref(&id))[0].1,
        Err(unsupported.clone())
    );
    assert_eq!(gh.body_edits(no, bound), Err(unsupported.clone()));
    assert_eq!(gh.comment_edits(&id, bound), Err(unsupported.clone()));
    assert_eq!(gh.attachments(no), Err(unsupported.clone()));
    assert_eq!(gh.changed(None).map(|_| ()), Err(unsupported.clone()));
    assert_eq!(gh.delta(None, bound).map(|_| ()), Err(unsupported));
}

/// A search that holds more than GitHub gives: every page is full, of
/// issues titled `title`, numbered on from the page's start, and the
/// total is `total`.
struct Wide {
    total: usize,
    title: &'static str,
    asked: Asked,
}

impl GhRunner for Wide {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        self.asked.borrow_mut().push(args.join(" "));
        let page: usize = args
            .iter()
            .find_map(|a| a.strip_prefix("page="))
            .and_then(|p| p.parse().ok())
            .unwrap();
        let items: Vec<String> = (1..=100)
            .map(|k| {
                let no = (page - 1) * 100 + k;
                format!(
                    r#"{{"number":{no},"title":"{} {no}","state":"open","body":null,"repository_url":"https://api.github.com/repos/{PROJECT}"}}"#,
                    self.title
                )
            })
            .collect();
        Ok(format!(
            r#"{{"total_count":{},"incomplete_results":false,"items":[{}]}}"#,
            self.total,
            items.join(",")
        ))
    }
}

#[test]
fn past_githubs_reach_a_search_that_cannot_tell_is_inconsistent() {
    let asked = Asked::default();
    let wide = |title| Wide {
        total: 1500,
        title,
        asked: Rc::clone(&asked),
    };
    // No hit holds the word: past the thousandth item, one might.
    let mut gh = GitHub::new(PROJECT, Box::new(wide("Other")));
    assert_eq!(
        gh.search("report", Within::Titles, None, n(5)),
        Err(Outcome::Unread(Unread::Inconsistent))
    );
    assert_eq!(asked.borrow().len(), 10);
    // Hits enough within reach: the first n, and more.
    asked.borrow_mut().clear();
    let mut gh = GitHub::new(PROJECT, Box::new(wide("Report")));
    let took = gh.search("report", Within::Titles, None, n(150)).unwrap();
    assert_eq!((took.items.len(), took.more), (150, true));
    assert_eq!(asked.borrow().len(), 2);
}
