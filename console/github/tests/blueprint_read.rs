//! The blueprints port's reads of one blueprint as the adapter answers
//! them: `blueprint` at each depth, `several` and `comments`, over samples
//! of GitHub's REST answers in `tests/samples/blueprint-read/` (hand-made
//! from GitHub's documented shapes, no secret and no private name), the
//! laws of these reads, and the contract with `edit`: a read then an edit
//! at its revision. A failure sample is gh's error stream, given to the
//! adapter as `Bounded` would give it. No test calls GitHub.

use serde_json::Value;
use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use tehas_core::forge::GhRunner;
use tehas_github::markers;
use tehas_github::GitHub;
use tehas_ports::laws::{self, markers::Text};
use tehas_ports::{
    AccountKind, At, BlueprintNo, BlueprintWrites, Blueprints, Bound, Depth, Got, Lost, Marker,
    MarkerEdit, MarkerKind, Outcome, Passing, Pool, Refusal, Revision, Sent, Took, Unread, Window,
};

const PROJECT: &str = "acme/widget";

/// The words `Bounded` gives for a gh it stopped at its bound.
const BOUND: &str = "gh ran out of its 30s bound";

fn sample(name: &str) -> String {
    let at = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/samples/blueprint-read");
    std::fs::read_to_string(at.join(name)).unwrap()
}

fn json<T: serde::de::DeserializeOwned>(name: &str) -> T {
    serde_json::from_str(&sample(name)).unwrap()
}

/// What the adapter is given for a failure sample.
fn failed(name: &str) -> String {
    tehas_core::forge::why(&sample(name), Some(1))
}

fn now() -> i64 {
    1_795_000_000
}

fn no(n: u64) -> BlueprintNo {
    BlueprintNo::new(n).unwrap()
}

fn n32(n: u32) -> NonZeroU32 {
    NonZeroU32::new(n).unwrap()
}

/// The characters of a text `Depth::Cut` keeps, as the tickets read cuts.
const CUT_TEXT: usize = 200;

const CUT0: Depth = Depth::Cut { notes: 0 };

type Calls = Arc<Mutex<Vec<String>>>;
type Kept = Arc<Mutex<Site>>;

/// The issues a site keeps, by number, and each one's comments, oldest
/// first. A read answers the issue, a page of comments its slice, and a
/// `PATCH` changes the issue's title and body. A path in `fail` fails with
/// its reason. Any other call fails.
#[derive(Default)]
struct Site {
    issues: BTreeMap<u64, Value>,
    comments: BTreeMap<u64, Vec<Value>>,
    fail: BTreeMap<String, String>,
}

impl Site {
    fn issue(mut self, n: u64, title: &str, body: &str) -> Self {
        let mut v = json::<Value>("issue-7.json");
        v["number"] = n.into();
        v["html_url"] = format!("https://github.com/acme/widget/issues/{n}").into();
        v["title"] = title.into();
        v["body"] = body.into();
        v["comments"] = 0.into();
        self.issues.insert(n, v);
        self
    }

    /// Issue 7 as its sample has it, with the comments of its sample.
    fn seven(mut self) -> Self {
        self.issues.insert(7, json::<Value>("issue-7.json"));
        let page = json::<Value>("comments-7-page-1.json");
        self.comments.insert(7, page.as_array().unwrap().clone());
        self
    }

    /// Issue `n` holding `count` comments made from the template.
    fn with_comments(mut self, n: u64, count: usize) -> Self {
        let made = (0..count)
            .map(|i| {
                let mut c = json::<Value>("comment-template.json");
                let id = 2_501_240_000 + i as u64;
                c["id"] = id.into();
                c["html_url"] =
                    format!("https://github.com/acme/widget/issues/{n}#issuecomment-{id}").into();
                c["body"] = format!("Comment {i}.").into();
                c
            })
            .collect::<Vec<_>>();
        self.issues.get_mut(&n).unwrap()["comments"] = count.into();
        self.comments.insert(n, made);
        self
    }

    fn answer(&mut self, args: &[&str]) -> Result<String, String> {
        if args.starts_with(&["api", "-X", "GET", "search/issues"]) {
            return Ok(self.search_page(args));
        }
        let [api, path] = args else {
            return Err(format!("no recording of {args:?}"));
        };
        if *api != "api" {
            return Err(format!("no recording of {args:?}"));
        }
        if let Some(why) = self.fail.get(*path) {
            return Err(why.clone());
        }
        let rest = path
            .strip_prefix("repos/acme/widget/issues/")
            .ok_or("no recording")?;
        if let Some((n, query)) = rest.split_once("/comments?") {
            let n: u64 = n.parse().map_err(|_| "no recording")?;
            let page: usize = query
                .strip_prefix("per_page=100&page=")
                .and_then(|p| p.parse().ok())
                .ok_or("no recording")?;
            let all = self.comments.get(&n).cloned().unwrap_or_default();
            let from = ((page - 1) * 100).min(all.len());
            let to = (page * 100).min(all.len());
            return Ok(Value::Array(all[from..to].to_vec()).to_string());
        }
        let n: u64 = rest.parse().map_err(|_| "no recording")?;
        self.issues
            .get(&n)
            .map(Value::to_string)
            .ok_or_else(|| "gh: Not Found (HTTP 404)".to_string())
    }
}

impl Site {
    /// The first page of GitHub's issue search over the issues the site
    /// keeps: each one whose title holds the question's quoted words, as
    /// the search's items carry it, the issue's own REST answer.
    fn search_page(&self, args: &[&str]) -> String {
        let q = args
            .iter()
            .find_map(|a| a.strip_prefix("q="))
            .unwrap_or_default()
            .to_ascii_lowercase();
        let words: Vec<&str> = q.split('"').skip(1).step_by(2).collect();
        let items: Vec<Value> = self
            .issues
            .values()
            .filter(|v| {
                let title = v["title"].as_str().unwrap_or_default().to_ascii_lowercase();
                words.iter().all(|w| title.contains(w))
            })
            .cloned()
            .collect();
        serde_json::json!({
            "total_count": items.len(),
            "incomplete_results": false,
            "items": items,
        })
        .to_string()
    }
}

struct Runner {
    site: Kept,
    calls: Calls,
}

impl GhRunner for Runner {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        self.calls.lock().unwrap().push(args.join(" "));
        self.site.lock().unwrap().answer(args)
    }

    fn run_input(&mut self, args: &[&str], input: &str) -> Result<String, String> {
        self.calls.lock().unwrap().push(args.join(" "));
        let [_, _, method, path, _, _] = args else {
            return Err(format!("no recording of {args:?}"));
        };
        let n: u64 = path
            .strip_prefix("repos/acme/widget/issues/")
            .and_then(|n| n.parse().ok())
            .ok_or("no recording")?;
        if *method != "PATCH" {
            return Err(format!("no recording of {args:?}"));
        }
        let sent: Value = serde_json::from_str(input).map_err(|_| "no JSON")?;
        let mut site = self.site.lock().unwrap();
        let issue = site.issues.get_mut(&n).ok_or("no recording")?;
        for k in ["title", "body"] {
            if let Some(v) = sent.get(k) {
                issue[k] = v.clone();
            }
        }
        Ok(issue.to_string())
    }
}

fn adapter(site: Site) -> (GitHub, Kept, Calls) {
    let kept = Arc::new(Mutex::new(site));
    let calls = Calls::default();
    let runner = Runner {
        site: Arc::clone(&kept),
        calls: Arc::clone(&calls),
    };
    (
        GitHub::new(PROJECT, Box::new(runner)).on_clock(now),
        kept,
        calls,
    )
}

/// An adapter whose one call answers `answer`.
fn answering(answer: Result<String, String>) -> (GitHub, Calls) {
    struct One(Option<Result<String, String>>, Calls);
    impl GhRunner for One {
        fn run(&mut self, args: &[&str]) -> Result<String, String> {
            self.1.lock().unwrap().push(args.join(" "));
            self.0.take().unwrap_or_else(|| Err("no answer".into()))
        }
    }
    let calls = Calls::default();
    let gh = GitHub::new(PROJECT, Box::new(One(Some(answer), Arc::clone(&calls)))).on_clock(now);
    (gh, calls)
}

const ISSUE_7: &str = "api repos/acme/widget/issues/7";

fn page(n: u64, p: usize) -> String {
    format!("api repos/acme/widget/issues/{n}/comments?per_page=100&page={p}")
}

#[test]
fn a_blueprint_at_cut_0_is_one_read_of_the_issue_its_markers_apart() {
    let (mut gh, _, calls) = adapter(Site::default().seven());
    let b = gh.blueprint(no(7), CUT0).unwrap();
    assert_eq!(*calls.lock().unwrap(), [ISSUE_7], "no comment request");
    let raw = json::<Value>("issue-7.json")["body"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(b.no, no(7));
    assert_eq!(b.title, "Plan: the widget's settings page");
    assert!(b.open);
    // This item's marker is out of the text and typed; another item's
    // marker is no marker of it, and its words stay.
    assert_eq!(b.markers, [Marker::HoldCleared]);
    assert!(!b.body.contains("tehas:hold-cleared"), "{}", b.body);
    assert!(b.body.contains("<!-- tehas:park issue=5 "), "{}", b.body);
    assert!(b.body.starts_with("One page for every setting."));
    assert_eq!(b.body, markers::read(&raw, Some(no(7))).text);
    // The whole body, not cut.
    assert!(b.body.len() > CUT_TEXT);
    // The revision is over the raw answer, markers in.
    let mut gh2 = GitHub::new(PROJECT, Box::new(Echo(sample("issue-7.json"))));
    assert_eq!(b.revision, Revision::new(gh2.text_revision(7).unwrap()));
    assert_eq!(
        b.created,
        At::new(tehas_core::time::from_iso("2026-11-20T09:00:00Z").unwrap())
    );
    assert_eq!(b.link.as_str(), "https://github.com/acme/widget/issues/7");
    // The author through the base's account: folded, with its id.
    let author = b.author.unwrap();
    assert_eq!(author.login.as_str(), "tehas-bot");
    assert_eq!(author.id.unwrap().as_str(), "100001");
    assert_eq!(author.kind, AccountKind::Bot);
    // REST names no editor and no time the body was edited.
    assert_eq!((b.editor, b.edited), (None, None));
    // No comment taken; the issue holds three.
    assert!(b.comments.items.is_empty());
    assert!(b.comments.more);
}

/// A runner that answers every call with one answer.
struct Echo(String);

impl GhRunner for Echo {
    fn run(&mut self, _: &[&str]) -> Result<String, String> {
        Ok(self.0.clone())
    }
}

#[test]
fn an_issue_with_no_comment_says_no_more_at_cut_0() {
    let (mut gh, _, _) = adapter(Site::default().issue(9, "Plan", "Text."));
    let b = gh.blueprint(no(9), CUT0).unwrap();
    assert_eq!(b.comments, Took::default());
    assert!(b.markers.is_empty());
    assert_eq!(b.body, "Text.");
}

#[test]
fn comments_at_each_depth_carry_their_markers_apart_and_their_accounts() {
    let (mut gh, _, calls) = adapter(Site::default().seven());
    let all = gh.blueprint(no(7), Depth::All).unwrap();
    assert_eq!(*calls.lock().unwrap(), [ISSUE_7.to_string(), page(7, 1)]);
    assert!(!all.comments.more);
    let c = &all.comments.items;
    assert_eq!(c.len(), 3);
    assert_eq!(c[0].id.as_str(), "2501230001");
    assert_eq!(c[0].body, "Can the page show the defaults too?");
    assert_eq!(c[0].author.as_ref().unwrap().login.as_str(), "alice-dev");
    assert_eq!(c[0].author.as_ref().unwrap().kind, AccountKind::Person);
    assert_eq!(c[0].edited, None);
    assert_eq!(
        c[0].link.as_str(),
        "https://github.com/acme/widget/issues/7#issuecomment-2501230001"
    );
    // The question's marker, of this item, is out of its text and typed.
    assert_eq!(c[1].markers.len(), 1);
    assert_eq!(c[1].markers[0].kind(), MarkerKind::Question);
    assert_eq!(c[1].body, "Which defaults: the shipped ones or the site's?");
    assert_eq!(c[1].author.as_ref().unwrap().login.as_str(), "tehas-bot");
    // A deleted account names nobody; an edited comment says when.
    assert_eq!(c[2].author, None);
    assert_eq!(c[2].editor, None);
    assert!(c[2].edited.is_some());
    assert!(c[2].body.len() > 200);
    // A comment's revision is over its raw text, markers in.
    assert_ne!(c[1].revision, c[0].revision);

    // The newest two, whole, with more.
    let last = gh.blueprint(no(7), Depth::Last { notes: n32(2) }).unwrap();
    assert!(last.comments.more);
    assert_eq!(last.comments.items, c[1..]);
    // The newest one, cut to the bound, with more.
    let cut = gh.blueprint(no(7), Depth::Cut { notes: 1 }).unwrap();
    assert!(cut.comments.more);
    assert_eq!(cut.comments.items.len(), 1);
    assert_eq!(cut.comments.items[0].body.chars().count(), CUT_TEXT);
    assert!(c[2].body.starts_with(&cut.comments.items[0].body));
    // More than it holds: all of them, no more.
    let five = gh.blueprint(no(7), Depth::Last { notes: n32(5) }).unwrap();
    assert!(!five.comments.more);
    assert_eq!(five.comments.items, *c);
    // The comments port reads the same, whole.
    let whole = gh.comments(no(7), Bound::new(3).unwrap()).unwrap();
    assert_eq!(whole.items(), &c[..]);
}

#[test]
fn the_comments_of_an_issue_at_a_bound_of_100_carry_what_a_settle_compares() {
    let (mut gh, _, calls) = adapter(Site::default().seven());
    let whole = gh.comments(no(7), Bound::new(100).unwrap()).unwrap();
    assert_eq!(*calls.lock().unwrap(), [ISSUE_7.to_string(), page(7, 1)]);
    let page_1 = json::<Value>("comments-7-page-1.json");
    let recorded = page_1.as_array().unwrap();
    assert_eq!(whole.items().len(), recorded.len());
    for (c, r) in whole.items().iter().zip(recorded) {
        // The REST comment id, and GitHub's created_at exactly.
        assert_eq!(c.id.as_str(), r["id"].as_u64().unwrap().to_string());
        assert_eq!(
            c.created,
            At::new(tehas_core::time::from_iso(r["created_at"].as_str().unwrap()).unwrap())
        );
        // The text without this item's markers, the markers typed.
        let read = markers::read(r["body"].as_str().unwrap(), Some(no(7)));
        assert_eq!(c.body, read.text);
        assert_eq!(c.markers, read.markers);
        // The author with its id and kind, or nobody.
        match r["user"].as_object() {
            None => assert_eq!(c.author, None),
            Some(u) => {
                let a = c.author.as_ref().unwrap();
                assert_eq!(a.id.as_ref().unwrap().as_str(), u["id"].to_string());
                let kind = match u["type"].as_str() {
                    Some("Bot") => AccountKind::Bot,
                    _ => AccountKind::Person,
                };
                assert_eq!(a.kind, kind);
            }
        }
    }
    let question = &whole.items()[1];
    assert_eq!(question.markers.len(), 1);
    assert_eq!(question.markers[0].kind(), MarkerKind::Question);
    assert!(!question.body.contains("tehas:"));
}

#[test]
fn the_newest_n_read_only_the_pages_that_hold_them() {
    let (mut gh, _, calls) = adapter(Site::default().issue(9, "Plan", "T.").with_comments(9, 205));
    let b = gh.blueprint(no(9), Depth::Last { notes: n32(5) }).unwrap();
    assert_eq!(b.comments.items.len(), 5);
    assert_eq!(b.comments.items[0].body, "Comment 200.");
    assert_eq!(b.comments.items[4].body, "Comment 204.");
    assert_eq!(
        *calls.lock().unwrap(),
        ["api repos/acme/widget/issues/9".to_string(), page(9, 3)]
    );
    calls.lock().unwrap().clear();
    // Across a page boundary: the newest 106 start on page 1's last item.
    let b = gh
        .blueprint(no(9), Depth::Last { notes: n32(106) })
        .unwrap();
    assert_eq!(b.comments.items.len(), 106);
    assert_eq!(b.comments.items[0].body, "Comment 99.");
    assert!(b.comments.more);
    assert_eq!(
        *calls.lock().unwrap(),
        [
            "api repos/acme/widget/issues/9".to_string(),
            page(9, 1),
            page(9, 2),
            page(9, 3)
        ]
    );
    let all = gh.blueprint(no(9), Depth::All).unwrap();
    let bodies: Vec<String> = all.comments.items.iter().map(|c| c.body.clone()).collect();
    assert_eq!(
        bodies,
        (0..205)
            .map(|i| format!("Comment {i}."))
            .collect::<Vec<_>>()
    );
}

#[test]
fn the_list_laws_hold_over_a_page_boundary() {
    let held = 103;
    let (mut gh, _, _) = adapter(
        Site::default()
            .issue(9, "Plan", "T.")
            .with_comments(9, held),
    );
    let mut read = |w: Window| -> Got<Took<tehas_ports::Comment>> {
        let depth = match w {
            Window::Last(notes) => Depth::Last { notes },
            Window::Whole(_) => Depth::All,
        };
        gh.blueprint(no(9), depth).map(|b| b.comments)
    };
    assert_eq!(
        laws::lists::last_n_is_never_cut(&mut read, held as u32),
        Ok(())
    );
    assert_eq!(laws::lists::more_is_exact(&mut read, held as u32), Ok(()));
    assert_eq!(
        laws::lists::past_the_bound_is_unread(|b| gh.comments(no(9), b), held as u32),
        Ok(())
    );
}

#[test]
fn more_comments_than_all_takes_is_cut_with_no_page_read() {
    let mut site = Site::default().issue(9, "Plan", "T.");
    site.issues.get_mut(&9).unwrap()["comments"] = 1001.into();
    let (mut gh, _, calls) = adapter(site);
    assert_eq!(
        gh.blueprint(no(9), Depth::All),
        Err(Outcome::Unread(Unread::Cut))
    );
    assert_eq!(calls.lock().unwrap().len(), 1);
}

#[test]
fn comments_that_do_not_add_up_to_the_issues_count_are_inconsistent() {
    // A comment came or went between the issue's read and its pages.
    for count in [2, 4] {
        let mut site = Site::default().seven();
        site.issues.get_mut(&7).unwrap()["comments"] = count.into();
        let (mut gh, _, _) = adapter(site);
        assert_eq!(
            gh.blueprint(no(7), Depth::All),
            Err(Outcome::Unread(Unread::Inconsistent)),
            "{count}"
        );
    }
    // A comment twice.
    let mut site = Site::default().seven();
    let c = site.comments.get(&7).unwrap()[0].clone();
    site.comments.get_mut(&7).unwrap()[1] = c;
    let (mut gh, _, _) = adapter(site);
    assert_eq!(
        gh.blueprint(no(7), Depth::All),
        Err(Outcome::Unread(Unread::Inconsistent))
    );
    // A comment of another issue.
    let mut site = Site::default().seven();
    site.comments.get_mut(&7).unwrap()[0]["html_url"] =
        "https://github.com/acme/widget/issues/9#issuecomment-2501230001".into();
    let (mut gh, _, _) = adapter(site);
    assert_eq!(
        gh.blueprint(no(7), Depth::All),
        Err(Outcome::Unread(Unread::Inconsistent))
    );
    // A page that is no list of comments.
    let mut site = Site::default().seven();
    site.comments.get_mut(&7).unwrap()[0]
        .as_object_mut()
        .unwrap()
        .remove("created_at");
    let (mut gh, _, _) = adapter(site);
    assert_eq!(
        gh.blueprint(no(7), Depth::All),
        Err(Outcome::Unread(Unread::Malformed))
    );
}

#[test]
fn a_pull_requests_number_is_not_found_with_no_further_request() {
    for depth in [CUT0, Depth::All, Depth::Last { notes: n32(3) }] {
        let (mut gh, calls) = answering(Ok(sample("pull-request-8.json")));
        assert_eq!(gh.blueprint(no(8), depth), Err(Outcome::NotFound));
        assert_eq!(calls.lock().unwrap().len(), 1);
    }
    let (mut gh, calls) = answering(Ok(sample("pull-request-8.json")));
    assert_eq!(
        gh.comments(no(8), Bound::new(10).unwrap()).map(|_| ()),
        Err(Outcome::NotFound)
    );
    assert_eq!(calls.lock().unwrap().len(), 1);
}

#[test]
fn every_failure_of_the_read_is_its_named_outcome_never_an_empty_blueprint() {
    let maybe = |why| Outcome::Unavailable {
        why,
        sent: Sent::Maybe,
    };
    let spent = Outcome::Spent {
        pool: Pool::Calls,
        until: At::new(now()),
    };
    let cases = [
        (Err(failed("read-not-found.stderr")), Outcome::NotFound),
        (Err(failed("read-gone.stderr")), Outcome::NotFound),
        (Err(failed("read-forbidden.stderr")), Outcome::Forbidden),
        (Err(failed("read-signed-out.stderr")), Outcome::SignedOut),
        (Err(failed("read-rate-limited.stderr")), spent.clone()),
        (Err(failed("read-secondary-limit.stderr")), spent),
        (
            Err(failed("read-server-error.stderr")),
            maybe(Passing::Server),
        ),
        (
            Err(failed("read-timed-out.stderr")),
            maybe(Passing::Timeout),
        ),
        (Err(BOUND.to_string()), maybe(Passing::Timeout)),
        (
            Err("not started: out of time".to_string()),
            Outcome::Unavailable {
                why: Passing::Timeout,
                sent: Sent::No,
            },
        ),
        (
            Err(failed("read-unreachable.stderr")),
            Outcome::Unavailable {
                why: Passing::Unreachable,
                sent: Sent::No,
            },
        ),
        (
            Ok(sample("issue-7-cut-off.json")),
            Outcome::Unread(Unread::Malformed),
        ),
    ];
    for (answer, want) in cases {
        let (mut gh, calls) = answering(answer.clone());
        assert_eq!(gh.blueprint(no(7), CUT0), Err(want.clone()), "{answer:?}");
        assert_eq!(calls.lock().unwrap().len(), 1, "{answer:?}");
        let (mut gh, _) = answering(answer.clone());
        assert_eq!(gh.several(&[no(7)], CUT0), [(no(7), Err(want.clone()))]);
        let (mut gh, _) = answering(answer.clone());
        let got = gh.comments(no(7), Bound::new(5).unwrap()).map(|_| ());
        assert_eq!(got, Err(want), "{answer:?}");
    }
}

#[test]
fn a_failed_page_of_comments_fails_the_read() {
    let mut site = Site::default().seven();
    site.fail.insert(
        "repos/acme/widget/issues/7/comments?per_page=100&page=1".into(),
        failed("read-server-error.stderr"),
    );
    let (mut gh, _, _) = adapter(site);
    assert_eq!(
        gh.blueprint(no(7), Depth::Cut { notes: 1 }),
        Err(Outcome::Unavailable {
            why: Passing::Server,
            sent: Sent::Maybe
        })
    );
    // At Cut 0 no page is read, so none can fail it.
    assert!(gh.blueprint(no(7), CUT0).is_ok());
}

#[test]
fn an_answer_of_another_issue_or_project_or_one_missing_a_field_is_unread() {
    let base = json::<Value>("issue-7.json");
    let mut other_no = base.clone();
    other_no["number"] = 9.into();
    other_no["html_url"] = "https://github.com/acme/widget/issues/9".into();
    let mut other_repo = base.clone();
    other_repo["html_url"] = "https://github.com/acme/other/issues/7".into();
    let inconsistent = Outcome::Unread(Unread::Inconsistent);
    let malformed = Outcome::Unread(Unread::Malformed);
    let mut cases = vec![(other_no, inconsistent.clone()), (other_repo, inconsistent)];
    for field in [
        "title",
        "state",
        "created_at",
        "comments",
        "number",
        "html_url",
    ] {
        let mut v = base.clone();
        v.as_object_mut().unwrap().remove(field);
        cases.push((v, malformed.clone()));
    }
    let mut v = base.clone();
    v["state"] = "merged".into();
    cases.push((v, malformed));
    for (answer, want) in cases {
        let (mut gh, _) = answering(Ok(answer.to_string()));
        assert_eq!(gh.blueprint(no(7), CUT0), Err(want), "{answer}");
    }
}

#[test]
fn no_word_of_gh_leaves_the_adapter() {
    let (mut gh, _) = answering(Err(failed("read-forbidden.stderr")));
    let line = gh.blueprint(no(7), CUT0).unwrap_err().line();
    for word in ["HTTP", "403", "integration", "Resource"] {
        assert!(!line.contains(word), "{line}");
    }
}

#[test]
fn several_answers_each_number_on_its_own_in_the_order_asked() {
    let (mut gh, _, calls) = adapter(Site::default().seven().issue(9, "Other", "T."));
    let got = gh.several(&[no(9), no(404), no(7)], CUT0);
    assert_eq!(got.len(), 3);
    assert_eq!(got[0].1.as_ref().unwrap().title, "Other");
    assert_eq!(got[1], (no(404), Err(Outcome::NotFound)));
    assert_eq!(got[2].1.as_ref().unwrap().markers, [Marker::HoldCleared]);
    assert_eq!(calls.lock().unwrap().len(), 3);
}

#[test]
fn a_title_search_then_the_blueprint_of_its_hit_answer_the_same_title_author_and_created() {
    // The relay's settle of an open that may have landed: a search of the
    // title, then each hit read at Cut 0, over one set of recordings.
    let (mut gh, _, calls) = adapter(Site::default().seven().issue(9, "Another plan", "T."));
    let title = "Plan: the widget's settings page";
    let hits = gh
        .search(title, tehas_ports::Within::Titles, Some(true), n32(20))
        .unwrap();
    assert!(!hits.more);
    assert_eq!(hits.items.len(), 1);
    let hit = &hits.items[0];
    assert_eq!((hit.no, hit.title.as_str(), hit.open), (no(7), title, true));
    let b = gh.blueprint(hit.no, CUT0).unwrap();
    assert_eq!(b.title, hit.title);
    let issue = json::<Value>("issue-7.json");
    let author = b.author.as_ref().unwrap();
    assert_eq!(author.id.as_ref().unwrap().as_str(), "100001");
    assert_eq!(author.kind, AccountKind::Bot);
    assert_eq!(
        b.created,
        At::new(tehas_core::time::from_iso(issue["created_at"].as_str().unwrap()).unwrap())
    );
    // The body without this item's markers, nothing else changed.
    let raw = issue["body"].as_str().unwrap();
    assert_eq!(b.body, markers::read(raw, Some(no(7))).text);
    assert!(b
        .body
        .starts_with("One page for every setting.\r\n\r\n- read them\r\n- save them"));
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert!(calls[0].starts_with("api -X GET search/issues"));
    assert_eq!(calls[1], ISSUE_7);
}

#[test]
fn a_persons_issue_answers_a_person_with_its_id() {
    let mut site = Site::default().issue(9, "Plan", "T.");
    site.issues.get_mut(&9).unwrap()["user"] =
        serde_json::json!({"login": "Alice-Dev", "id": 200002, "type": "User"});
    let (mut gh, _, _) = adapter(site);
    let author = gh.blueprint(no(9), CUT0).unwrap().author.unwrap();
    assert_eq!(author.login.as_str(), "alice-dev");
    assert_eq!(author.id.unwrap().as_str(), "200002");
    assert_eq!(author.kind, AccountKind::Person);
}

// The contract with the edit.

#[test]
fn an_edit_at_the_reads_revision_goes_through_on_an_unchanged_issue() {
    let (mut gh, kept, _) = adapter(Site::default().issue(7, "Plan", "One page."));
    let b = gh.blueprint(no(7), CUT0).unwrap();
    let got = gh.edit(
        no(7),
        None,
        Some("Two pages."),
        &MarkerEdit::default(),
        &b.revision,
    );
    assert!(got.is_ok(), "{got:?}");
    assert_eq!(kept.lock().unwrap().issues[&7]["body"], "Two pages.");
}

#[test]
fn an_edit_at_the_reads_revision_of_a_marked_body_meets_the_edits_marker_refusal() {
    let raw = "One page.\n\n<!-- tehas:hold-cleared -->";
    let (mut gh, kept, calls) = adapter(Site::default().issue(7, "Plan", raw));
    let b = gh.blueprint(no(7), CUT0).unwrap();
    assert_eq!(b.markers, [Marker::HoldCleared]);
    assert_eq!(b.body, "One page.");
    // A new body would lose the marker: the edit refuses it, sends nothing.
    let got = gh.edit(
        no(7),
        None,
        Some("Two pages."),
        &MarkerEdit::default(),
        &b.revision,
    );
    assert_eq!(got, Err(Outcome::Refused(Refusal::Unsupported)));
    assert_eq!(kept.lock().unwrap().issues[&7]["body"], raw);
    // A title alone goes through at the read's revision: not a conflict.
    let got = gh.edit(
        no(7),
        Some("Retitled"),
        None,
        &MarkerEdit::default(),
        &b.revision,
    );
    assert!(got.is_ok(), "{got:?}");
    assert_eq!(kept.lock().unwrap().issues[&7]["title"], "Retitled");
    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.contains("PATCH"))
            .count(),
        1
    );
}

#[test]
fn a_body_read_back_with_another_items_marker_words_is_refused_by_the_edits_text_check() {
    // Another item's marker stays in the body as words, and the edit takes
    // no text that holds a marker's words: sent back, it is refused.
    let raw = json::<Value>("issue-7.json")["body"]
        .as_str()
        .unwrap()
        .to_string();
    let (mut gh, kept, calls) = adapter(Site::default().issue(7, "Plan", &raw));
    let b = gh.blueprint(no(7), CUT0).unwrap();
    assert!(b.body.contains("tehas:park issue=5"));
    let got = gh.edit(
        no(7),
        None,
        Some(&b.body),
        &MarkerEdit::default(),
        &b.revision,
    );
    assert_eq!(got, Err(Outcome::Refused(Refusal::Invalid)));
    assert_eq!(kept.lock().unwrap().issues[&7]["body"], raw.as_str());
    assert!(!calls.lock().unwrap().iter().any(|c| c.contains("PATCH")));
}

#[test]
fn an_edit_at_the_reads_revision_after_the_issue_changed_is_a_conflict() {
    let marked = |m: &str| format!("One page.\n\n<!-- tehas:{m} -->");
    for change in ["body", "title", "marker"] {
        let (mut gh, kept, calls) =
            adapter(Site::default().issue(7, "Plan", &marked("hold-cleared")));
        let b = gh.blueprint(no(7), CUT0).unwrap();
        {
            let mut site = kept.lock().unwrap();
            let issue = site.issues.get_mut(&7).unwrap();
            match change {
                "body" => issue["body"] = "Another account's body.".into(),
                "title" => issue["title"] = "Retitled by another".into(),
                // The text without markers is the same; the raw body is not.
                _ => issue["body"] = marked("blocked by=3").into(),
            }
        }
        let again = gh.blueprint(no(7), CUT0).unwrap();
        if change == "marker" {
            assert_eq!(again.body, b.body);
        }
        assert_ne!(again.revision, b.revision, "{change}");
        let got = gh.edit(
            no(7),
            Some("Mine"),
            None,
            &MarkerEdit::default(),
            &b.revision,
        );
        assert_eq!(got, Err(Outcome::Conflict(Lost::Changed)), "{change}");
        assert!(
            !calls.lock().unwrap().iter().any(|c| c.contains("PATCH")),
            "{change}"
        );
    }
}

// The laws of these reads, run against the adapter.

#[test]
fn the_law_an_edit_moves_the_revision_holds_over_the_adapters_read_and_edit() {
    let (mut gh, kept, _) = adapter(Site::default().issue(
        7,
        "Plan: the widget's settings page",
        "One page for every setting.",
    ));
    laws::blueprints::an_edit_moves_the_revision(&mut gh, no(7)).unwrap();
    assert_eq!(
        kept.lock().unwrap().issues[&7]["body"],
        "One page for every setting.\n\nEdited."
    );
}

#[test]
fn the_law_an_unreadable_marker_is_kept_meets_the_edits_refusal_and_writes_nothing() {
    // With no marker writer, an edit of a body that holds markers is
    // refused, nothing is written, and the markers stay.
    let body = "The plan.\n<!-- tehas:hold-cleared -->\n<!-- tehas:park issue=07 step=work kind=stopped provider=0 sig=0000000000000000000000000000000000000000000000000000000000000000 -->";
    let (mut gh, kept, _) = adapter(Site::default().issue(7, "Plan", body));
    let law = laws::markers::an_unreadable_marker_is_kept(&mut gh, no(7));
    assert_eq!(
        law,
        Err(laws::Broken {
            law: "markers::an_unreadable_marker_is_kept".into(),
            what: format!(
                "the edit: {}",
                Outcome::Refused(Refusal::Unsupported).line()
            ),
        })
    );
    assert_eq!(kept.lock().unwrap().issues[&7]["body"], body);
}

#[test]
fn the_marker_laws_hold_over_a_blueprints_body() {
    let sha = |c: char| c.to_string().repeat(64);
    let park = |n: u64| {
        format!(
            "<!-- tehas:park issue={n} step=develop kind= provider=0 sig={} -->",
            sha('b')
        )
    };
    let many: Vec<String> = (1..=21).map(|n| n.to_string()).collect();
    let bodies = [
        (101, "> <!-- tehas:hold-cleared -->".to_string()),
        (
            102,
            format!(
                "<!-- tehas:park issue=102 step=develop kind= provider=x sig={} -->",
                sha('b')
            ),
        ),
        (103, "<!-- tehas:question kind=plan issue=0103 turn=1 -->".to_string()),
        (104, format!("<!-- tehas:blocked by={} -->", many.join(","))),
        (105, format!("Prose: {}", park(105).trim_start_matches("<!-- ").trim_end_matches(" -->"))),
        (106, "\ttehas:park issue=106".to_string()),
        (107, "The tehas:blocked words, no field.".to_string()),
        (
            108,
            "<!-- tehas:blocked by=x -->\n<!-- tehas:blocked by=1 -->\n<!-- tehas:blocked by=03 -->\n<!-- tehas:hold-cleared -->".to_string(),
        ),
        (109, format!("A copy: {}", park(101))),
        (110, format!("<!-- tehas:last-attempt base={} -->\nA note.", sha('c'))),
    ];
    let mut site = Site::default();
    for (n, body) in &bodies {
        site = site.issue(*n, "Plan", body);
    }
    let (mut gh, _, _) = adapter(site);
    let park_105 = markers::read(&park(105), Some(no(105))).markers[0].clone();
    let order = markers::read(&bodies[7].1, Some(no(108))).markers;
    let unreadable = |kind| Marker::Unreadable { kind };
    assert_eq!(
        order,
        [
            unreadable(MarkerKind::Blocked),
            Marker::Blocked {
                by: vec![tehas_ports::Ref::Here(tehas_ports::TicketKey::Number(
                    std::num::NonZeroU64::new(1).unwrap()
                ))]
            },
            unreadable(MarkerKind::Blocked),
            Marker::HoldCleared,
        ]
    );
    use laws::markers as m;
    fn body(gh: &mut GitHub, n: u64) -> Text<'_> {
        Text::Body(gh, no(n))
    }
    let ok = |law: laws::Law| assert_eq!(law, Ok(()));
    ok(m::a_quoted_marker_is_no_marker(body(&mut gh, 101)));
    ok(m::a_bad_field_is_unreadable(
        body(&mut gh, 102),
        &MarkerKind::Park,
    ));
    ok(m::leading_zeros_are_unreadable(
        body(&mut gh, 103),
        &MarkerKind::Question,
    ));
    ok(m::a_list_over_its_limit_is_unreadable(body(&mut gh, 104)));
    ok(m::a_bare_marker_is_bare(
        body(&mut gh, 105),
        &MarkerKind::Park,
        Some(&park_105),
    ));
    ok(m::a_bare_marker_is_bare(
        body(&mut gh, 106),
        &MarkerKind::Park,
        None,
    ));
    ok(m::words_without_a_field_are_prose(body(&mut gh, 107)));
    ok(m::markers_keep_their_order(body(&mut gh, 108), &order));
    ok(m::another_items_marker_does_not_read(
        body(&mut gh, 109),
        &MarkerKind::Park,
    ));
    ok(m::a_block_without_its_close_is_unreadable(
        body(&mut gh, 110),
        &MarkerKind::LastAttempt,
    ));
    // Another item's marker keeps its words in the body.
    let b = gh.blueprint(no(109), CUT0).unwrap();
    assert_eq!(b.body, format!("A copy: {}", park(101)));
}
