//! `CodeHost::change` as the adapter answers it: one pull request read
//! whole, over samples of GitHub's REST answers in
//! `tests/samples/change-read/` (hand-made from GitHub's documented shapes,
//! no secret and no private name), and the laws of the read. A failure
//! sample is gh's error stream, given to the adapter as `Bounded` would
//! give it. No test calls GitHub.

use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tehas_core::forge::GhRunner;
use tehas_github::GitHub;
use tehas_ports::laws;
use tehas_ports::{
    AccountKind, At, BlueprintNo, Bound, Branch, ChangeNo, ChangeState, CodeHost, CommitId, Mark,
    Marker, MarkerKind, Mergeable, NotMergeable, Outcome, Passing, Pool, Ref, Sent, State, Unread,
    Verdict, Whole,
};

const PROJECT: &str = "acme/widget";
const PULL: &str = "api repos/acme/widget/pulls/9";

fn sample(name: &str) -> String {
    let at = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/samples/change-read");
    std::fs::read_to_string(at.join(name)).unwrap()
}

fn json_of(name: &str) -> Value {
    serde_json::from_str(&sample(name)).unwrap()
}

/// What the adapter is given for a failure sample.
fn failed(name: &str) -> String {
    tehas_core::forge::why(&sample(name), Some(1))
}

fn now() -> i64 {
    1_795_000_000
}

fn nine() -> ChangeNo {
    ChangeNo::new(9).unwrap()
}

fn bound(n: u32) -> Bound {
    Bound::new(n).unwrap()
}

fn at(iso: &str) -> At {
    At::new(tehas_core::time::from_iso(iso).unwrap())
}

/// A change the site makes to itself between two reads.
type Hook = Box<dyn FnOnce(&mut Site) + Send>;
type Calls = Arc<Mutex<Vec<String>>>;
type Kept = Arc<Mutex<Site>>;

/// One pull request, 9, as GitHub keeps it: its REST answer, its lists
/// (`comments` the issue comments, `files`, `reviews`, `notes` the review
/// comments), the last edits GraphQL answers (`pull` for its body, a node
/// id for a comment), and what its question answers of it. A path in
/// `fail` fails with its reason; `graphql` fails every question.
struct Site {
    pull: Value,
    lists: BTreeMap<&'static str, Vec<Value>>,
    edits: BTreeMap<String, Value>,
    node: Value,
    fail: BTreeMap<String, String>,
    graphql: Option<String>,
    /// Run after the page of a list is answered: (list, page).
    after: Vec<((&'static str, usize), Hook)>,
    /// Run after every first page of comments, for a site that never
    /// holds still.
    restless: bool,
}

impl Default for Site {
    fn default() -> Self {
        Site {
            pull: json_of("pull-9.json"),
            lists: BTreeMap::new(),
            edits: BTreeMap::new(),
            node: json!({
                "number": 9,
                "isCrossRepository": false,
                "mergeable": "MERGEABLE",
                "mergeStateStatus": "DRAFT",
                "closingIssuesReferences": {"totalCount": 1, "nodes": [
                    {"number": 7, "repository": {"databaseId": 500001, "nameWithOwner": "acme/widget"}}
                ]}
            }),
            fail: BTreeMap::new(),
            graphql: None,
            after: Vec::new(),
            restless: false,
        }
    }
}

/// Where each list is read and what the pull request counts of it.
fn list_of(name: &str) -> (&'static str, Option<&'static str>, &'static str) {
    match name {
        "comments" => ("comments", Some("comments"), "comment-template.json"),
        "files" => ("files", Some("changed_files"), "file-template.json"),
        "reviews" => ("reviews", None, "review-template.json"),
        _ => ("notes", Some("review_comments"), "note-template.json"),
    }
}

impl Site {
    /// `count` items of a list made from its template, ids rising.
    fn with(mut self, name: &str, count: usize) -> Self {
        let (key, counted, template) = list_of(name);
        let made: Vec<Value> = (0..count).map(|i| item(key, template, i)).collect();
        if let Some(field) = counted {
            self.pull[field] = count.into();
        }
        self.lists.insert(key, made);
        self
    }

    fn comments(&mut self) -> &mut Vec<Value> {
        self.lists.entry("comments").or_default()
    }

    /// The pull request was touched now: GitHub moves its `updated_at`.
    fn touch(&mut self) {
        let was = self.pull["updated_at"].as_str().unwrap();
        let then = tehas_core::time::from_iso(was).unwrap() + 60;
        self.pull["updated_at"] = tehas_core::time::iso(then).into();
    }

    fn edited(mut self, key: &str, editor: Option<(&str, &str, u64)>, when: &str) -> Self {
        let editor = editor
            .map(|(kind, login, id)| json!({"__typename": kind, "login": login, "databaseId": id}));
        self.edits.insert(
            key.to_string(),
            json!({"lastEditedAt": when, "editor": editor}),
        );
        self
    }

    fn edit_of(&self, key: &str) -> Value {
        self.edits
            .get(key)
            .cloned()
            .unwrap_or(json!({"lastEditedAt": null, "editor": null}))
    }

    fn graphql_answer(&self, args: &[&str]) -> Result<String, String> {
        if let Some(why) = &self.graphql {
            return Err(why.clone());
        }
        let q = args
            .iter()
            .find_map(|a| a.strip_prefix("query="))
            .ok_or("no query")?;
        let mut data = json!({"repository": {"databaseId": 500001}});
        if q.contains("pullRequest(number: $number)") {
            if !args.contains(&"number=9") {
                return Err("no number".into());
            }
            let mut node = self.node.clone();
            let edit = self.edit_of("pull");
            node["lastEditedAt"] = edit["lastEditedAt"].clone();
            node["editor"] = edit["editor"].clone();
            data["repository"]["pullRequest"] = node;
        }
        if let Some(ids) = q.split("nodes(ids: [").nth(1) {
            let ids = &ids[..ids.find(']').ok_or("no ids")?];
            let nodes: Vec<Value> = ids
                .split(", ")
                .map(|id| {
                    let id = id.trim_matches('"');
                    let mut e = self.edit_of(id);
                    e["__typename"] = "IssueComment".into();
                    e["id"] = id.into();
                    e
                })
                .collect();
            data["nodes"] = nodes.into();
        }
        Ok(json!({ "data": data }).to_string())
    }

    fn answer(&mut self, args: &[&str]) -> Result<String, String> {
        if args.starts_with(&["api", "graphql"]) {
            return self.graphql_answer(args);
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
        if *path == "repos/acme/widget/pulls/9" {
            return Ok(self.pull.to_string());
        }
        if path.starts_with("repos/acme/widget/pulls/") && !path.contains('?') {
            return Err(failed("read-not-found.stderr"));
        }
        let (list, query) =
            if let Some(q) = path.strip_prefix("repos/acme/widget/issues/9/comments?") {
                ("comments", q)
            } else {
                let rest = path
                    .strip_prefix("repos/acme/widget/pulls/9/")
                    .ok_or("no recording")?;
                let (name, q) = rest.split_once('?').ok_or("no recording")?;
                let list = match name {
                    "comments" => "notes",
                    other => list_of(other).0,
                };
                (list, q)
            };
        let page: usize = query
            .strip_prefix("per_page=100&page=")
            .and_then(|p| p.parse().ok())
            .ok_or("no recording")?;
        let all = self.lists.get(list).cloned().unwrap_or_default();
        let from = ((page - 1) * 100).min(all.len());
        let to = (page * 100).min(all.len());
        let answer = Value::Array(all[from..to].to_vec()).to_string();
        let mut i = 0;
        while i < self.after.len() {
            if self.after[i].0 == (list, page) {
                let (_, hook) = self.after.remove(i);
                hook(self);
            } else {
                i += 1;
            }
        }
        if self.restless && list == "comments" && page == 1 {
            shift(self);
        }
        Ok(answer)
    }
}

/// An item of a list from its template, the `i`th.
fn item(list: &str, template: &str, i: usize) -> Value {
    let mut v = json_of(template);
    match list {
        "comments" => {
            let id = 2_501_240_000 + i as u64;
            v["id"] = id.into();
            v["html_url"] =
                format!("https://github.com/acme/widget/pull/9#issuecomment-{id}").into();
            v["body"] = format!("Comment {i}.").into();
            v["node_id"] = format!("IC_kwDOAbCdEf6Vt{i:04}").into();
        }
        "files" => v["filename"] = format!("src/part_{i}.rs").into(),
        "reviews" => {
            v["id"] = (3_100_000_000 + i as u64).into();
            v["body"] = format!("Review {i}.").into();
        }
        _ => {
            v["id"] = (3_200_000_000 + i as u64).into();
            v["body"] = format!("Note {i}.").into();
        }
    }
    v
}

/// The first comment deleted and a new one posted: the count holds, the
/// pull request's `updated_at` moves, and every later comment moves up a
/// place, so a read across pages skips one.
fn shift(site: &mut Site) {
    let last = site.comments().last().unwrap()["id"].as_u64().unwrap();
    let next = (last - 2_501_240_000 + 1000) as usize;
    site.comments().remove(0);
    let added = item("comments", "comment-template.json", next);
    site.comments().push(added);
    site.touch();
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

fn page(list: &str, n: usize) -> String {
    match list {
        "comments" => format!("api repos/acme/widget/issues/9/comments?per_page=100&page={n}"),
        "notes" => format!("api repos/acme/widget/pulls/9/comments?per_page=100&page={n}"),
        _ => format!("api repos/acme/widget/pulls/9/{list}?per_page=100&page={n}"),
    }
}

/// The calls a read made, GraphQL's named by the word `graphql` alone.
fn asked(calls: &Calls) -> Vec<String> {
    calls
        .lock()
        .unwrap()
        .iter()
        .map(|c| {
            if c.starts_with("api graphql") {
                "graphql".to_string()
            } else {
                c.clone()
            }
        })
        .collect()
}

// The read, its requests and its digest.

#[test]
fn the_cheapest_read_is_three_requests_and_gives_the_whole_digest() {
    let (mut gh, _, calls) = adapter(Site::default());
    let c = gh.change(nine(), bound(100)).unwrap();
    assert_eq!(
        asked(&calls),
        [PULL.to_string(), "graphql".into(), page("reviews", 1)]
    );
    let d = &c.digest;
    assert_eq!(d.no, nine());
    assert_eq!(d.title, "Widgets count their parts");
    assert_eq!(d.state, ChangeState::Open);
    assert_eq!(d.head, Branch::new("feature/issue-7-parts").unwrap());
    assert_eq!(
        d.head_commit,
        CommitId::new("a1b2c3d4e5f60718293a4b5c6d7e8f9012345678").unwrap()
    );
    assert_eq!(d.base, Branch::new("main").unwrap());
    assert_eq!(
        d.base_commit,
        CommitId::new("0123456789abcdef0123456789abcdef01234567").unwrap()
    );
    assert!(d.draft);
    assert!(!d.from_fork);
    assert_eq!(d.merge_state, Mergeable::No(NotMergeable::Draft));
    assert_eq!(d.closes, [Ref::Here(BlueprintNo::new(7).unwrap())]);
    assert_eq!(d.says_closes, [Ref::Here(BlueprintNo::new(7).unwrap())]);
    assert_eq!(d.created, at("2026-11-20T09:00:00Z"));
    assert_eq!(d.updated, at("2026-11-20T10:30:00Z"));
    assert_eq!(d.edited, None);
    assert_eq!(d.editor, None);
    let author = d.author.as_ref().unwrap();
    assert_eq!(author.login.as_str(), "tehas-bot");
    assert_eq!(author.kind, AccountKind::Bot);
    let size = d.size.unwrap();
    assert_eq!((size.files, size.added, size.deleted), (0, 12, 3));
    assert_eq!(d.link.as_str(), "https://github.com/acme/widget/pull/9");
    // The body without this item's marker, given in both places.
    assert_eq!(c.body, "Counts each part once.\n\nCloses #7");
    assert_eq!(d.body.as_deref(), Some(c.body.as_str()));
    assert!(c.comments.is_empty() && c.files.is_empty() && c.reviews.is_empty());
}

#[test]
fn a_merged_change_reads_its_merge_and_a_closed_one_its_close() {
    let mut site = Site::default();
    site.pull["state"] = "closed".into();
    site.pull["merged"] = true.into();
    site.pull["merged_at"] = "2026-11-21T09:00:00Z".into();
    site.pull["closed_at"] = "2026-11-21T09:00:00Z".into();
    site.pull["merged_by"] = json!({"login": "Octo-Person", "id": 200002, "type": "User"});
    let (mut gh, _, _) = adapter(site);
    match gh.change(nine(), bound(100)).unwrap().digest.state {
        ChangeState::Merged {
            at: when,
            by,
            commit,
        } => {
            assert_eq!(when, at("2026-11-21T09:00:00Z"));
            assert_eq!(by.unwrap().login.as_str(), "octo-person");
            assert_eq!(commit.as_str(), "5f1c2d3e4f5061728394a5b6c7d8e9f001122334");
        }
        other => panic!("{other:?}"),
    }
    let mut site = Site::default();
    site.pull["state"] = "closed".into();
    site.pull["closed_at"] = "2026-11-21T09:00:00Z".into();
    let (mut gh, _, _) = adapter(site);
    assert_eq!(
        gh.change(nine(), bound(100)).unwrap().digest.state,
        ChangeState::Closed {
            at: at("2026-11-21T09:00:00Z")
        }
    );
}

#[test]
fn an_edited_body_carries_its_editor() {
    let site = Site::default().edited(
        "pull",
        Some(("User", "Octo-Person", 200002)),
        "2026-11-20T10:20:00Z",
    );
    let (mut gh, _, _) = adapter(site);
    let d = gh.change(nine(), bound(100)).unwrap().digest;
    assert_eq!(d.edited, Some(at("2026-11-20T10:20:00Z")));
    let editor = d.editor.unwrap();
    assert_eq!(editor.login.as_str(), "octo-person");
    assert_eq!(editor.kind, AccountKind::Person);
    assert_eq!(editor.id.unwrap().as_str(), "200002");
}

#[test]
fn closing_references_of_another_project_and_past_one_page() {
    let mut site = Site::default();
    site.node["closingIssuesReferences"] = json!({"totalCount": 1, "nodes": [
        {"number": 5, "repository": {"databaseId": 500002, "nameWithOwner": "octo/tools"}}
    ]});
    let (mut gh, _, _) = adapter(site);
    match &gh.change(nine(), bound(100)).unwrap().digest.closes[..] {
        [Ref::Elsewhere { project, item }] => {
            assert_eq!((project.as_str(), item.as_str()), ("octo/tools", "5"));
        }
        other => panic!("{other:?}"),
    }
    let mut site = Site::default();
    site.node["closingIssuesReferences"]["totalCount"] = 101.into();
    let (mut gh, _, _) = adapter(site);
    assert_eq!(
        gh.change(nine(), bound(100)).map(|_| ()),
        Err(Outcome::Unread(Unread::Cut))
    );
}

#[test]
fn marks_come_from_labels_through_the_table_and_a_label_no_mark_can_have_is_reported() {
    let mut site = Site::default();
    site.pull["labels"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name": "party \u{1f389}"}));
    let (mut gh, _, _) = adapter(site);
    let marks = gh.change(nine(), bound(100)).unwrap().digest.marks;
    assert_eq!(marks.len(), 2);
    assert_eq!(marks[0], Mark::State(State::Running));
    assert!(matches!(&marks[1], Mark::Project(n) if n.as_str() == "area/parts"));
    assert_eq!(gh.take_left_out(), ["party \u{1f389}"]);
}

// The comments.

#[test]
fn one_comment_is_one_page_and_one_question() {
    let (mut gh, _, calls) = adapter(Site::default().with("comments", 1));
    let c = gh.change(nine(), bound(100)).unwrap();
    assert_eq!(c.comments.len(), 1);
    assert_eq!(c.comments[0].body, "Comment 0.");
    assert_eq!(c.comments[0].editor, None);
    assert_eq!(
        asked(&calls),
        [
            PULL.to_string(),
            page("comments", 1),
            "graphql".into(),
            page("reviews", 1)
        ]
    );
}

#[test]
fn a_hundred_comments_are_one_page_with_no_read_again() {
    let (mut gh, _, calls) = adapter(Site::default().with("comments", 100));
    let c = gh.change(nine(), bound(100)).unwrap();
    assert_eq!(c.comments.len(), 100);
    assert_eq!(c.comments[99].body, "Comment 99.");
    assert_eq!(calls.lock().unwrap().len(), 4);
}

#[test]
fn a_hundred_and_one_comments_are_two_pages_and_a_read_again() {
    let (mut gh, _, calls) = adapter(Site::default().with("comments", 101));
    let c = gh.change(nine(), bound(200)).unwrap();
    assert_eq!(c.comments.len(), 101);
    assert_eq!(c.comments[100].body, "Comment 100.");
    assert_eq!(
        asked(&calls),
        [
            PULL.to_string(),
            page("comments", 1),
            "graphql".into(),
            page("comments", 2),
            page("reviews", 1),
            PULL.to_string()
        ]
    );
}

#[test]
fn two_hundred_and_one_comments_are_three_pages() {
    let (mut gh, _, calls) = adapter(Site::default().with("comments", 201));
    let c = gh.change(nine(), bound(201)).unwrap();
    assert_eq!(c.comments.len(), 201);
    let bodies: Vec<&str> = c.comments.iter().map(|c| c.body.as_str()).collect();
    assert_eq!(bodies[200], "Comment 200.");
    let calls = asked(&calls);
    assert!(calls.contains(&page("comments", 3)));
    assert!(!calls.contains(&page("comments", 4)));
    assert_eq!(calls.len(), 7);
}

#[test]
fn more_comments_than_the_bound_are_cut_with_no_page_read() {
    let (mut gh, _, calls) = adapter(Site::default().with("comments", 101));
    assert_eq!(
        gh.change(nine(), bound(100)).map(|_| ()),
        Err(Outcome::Unread(Unread::Cut))
    );
    assert_eq!(asked(&calls), [PULL]);
}

#[test]
fn a_page_short_of_its_count_or_past_it_is_inconsistent_after_one_more_read() {
    // Counted 100 and holding 99; counted 50 and holding 51.
    for held in [99, 51] {
        let mut site = Site::default().with("comments", held);
        site.pull["comments"] = if held == 99 { 100 } else { 50 }.into();
        let (mut gh, _, calls) = adapter(site);
        assert_eq!(
            gh.change(nine(), bound(200)).map(|_| ()),
            Err(Outcome::Unread(Unread::Inconsistent)),
            "{held}"
        );
        assert_eq!(
            asked(&calls),
            [
                PULL.to_string(),
                page("comments", 1),
                PULL.to_string(),
                page("comments", 1)
            ]
        );
    }
}

#[test]
fn a_comment_deleted_and_one_posted_between_pages_is_read_once_more() {
    let mut site = Site::default().with("comments", 150);
    site.after
        .push((("comments", 1), Box::new(|s: &mut Site| shift(s))));
    let (mut gh, _, calls) = adapter(site);
    let c = gh.change(nine(), bound(200)).unwrap();
    // The second read sees the site as it is now.
    assert_eq!(c.comments.len(), 150);
    assert_eq!(c.comments[0].body, "Comment 1.");
    assert_eq!(c.comments[149].body, "Comment 1149.");
    let calls = asked(&calls);
    assert_eq!(
        calls.iter().filter(|c| **c == page("comments", 1)).count(),
        2
    );
}

#[test]
fn a_site_that_moves_between_pages_every_time_is_inconsistent() {
    let mut site = Site::default().with("comments", 150);
    site.restless = true;
    let (mut gh, _, calls) = adapter(site);
    assert_eq!(
        gh.change(nine(), bound(200)).map(|_| ()),
        Err(Outcome::Unread(Unread::Inconsistent))
    );
    // Two reads, each ending in the read again that found it moved.
    let calls = asked(&calls);
    assert_eq!(
        calls.iter().filter(|c| **c == page("comments", 1)).count(),
        2
    );
    assert_eq!(calls.last().unwrap(), PULL);
}

#[test]
fn comment_ids_out_of_order_are_inconsistent() {
    // Within a page.
    let mut site = Site::default().with("comments", 3);
    site.comments().swap(1, 2);
    let (mut gh, _, _) = adapter(site);
    assert_eq!(
        gh.change(nine(), bound(100)).map(|_| ()),
        Err(Outcome::Unread(Unread::Inconsistent))
    );
    // Across pages: page 2's first comment older than page 1's last.
    let mut site = Site::default().with("comments", 101);
    let first = site.comments()[0]["id"].clone();
    site.comments()[100]["id"] = (first.as_u64().unwrap() - 1).into();
    let id = site.comments()[100]["id"].as_u64().unwrap();
    site.comments()[100]["html_url"] =
        format!("https://github.com/acme/widget/pull/9#issuecomment-{id}").into();
    let (mut gh, _, _) = adapter(site);
    assert_eq!(
        gh.change(nine(), bound(200)).map(|_| ()),
        Err(Outcome::Unread(Unread::Inconsistent))
    );
}

#[test]
fn a_comment_of_another_item_is_inconsistent() {
    let mut site = Site::default().with("comments", 2);
    site.comments()[1]["html_url"] =
        "https://github.com/acme/widget/pull/8#issuecomment-2501240001".into();
    let (mut gh, _, _) = adapter(site);
    assert_eq!(
        gh.change(nine(), bound(100)).map(|_| ()),
        Err(Outcome::Unread(Unread::Inconsistent))
    );
}

#[test]
fn this_items_markers_are_taken_out_and_typed_and_another_items_left_in() {
    let mut site = Site::default().with("comments", 2);
    site.comments()[0]["body"] =
        "<!-- tehas:question kind=plan issue=9 turn=2 -->\r\nWhich parts count?".into();
    site.comments()[1]["body"] =
        "See there:\n<!-- tehas:question kind=plan issue=7 turn=1 -->".into();
    let (mut gh, _, _) = adapter(site);
    let c = gh.change(nine(), bound(100)).unwrap();
    assert_eq!(c.comments[0].body, "Which parts count?");
    assert_eq!(c.comments[0].markers.len(), 1);
    assert_eq!(c.comments[0].markers[0].kind(), MarkerKind::Question);
    assert!(!matches!(
        c.comments[0].markers[0],
        Marker::Unreadable { .. } | Marker::Bare { .. }
    ));
    assert!(c.comments[1].markers.is_empty());
    assert_eq!(
        c.comments[1].body,
        "See there:\n<!-- tehas:question kind=plan issue=7 turn=1 -->"
    );
}

#[test]
fn an_edited_comment_by_another_account_carries_that_editor() {
    let mut site = Site::default().with("comments", 2).edited(
        "IC_kwDOAbCdEf6Vt0001",
        Some(("User", "Octo-Person", 200002)),
        "2026-11-20T10:05:00Z",
    );
    site.comments()[0]["user"] = json!({"login": "tehas-bot[bot]", "id": 100001, "type": "Bot"});
    site.comments()[1]["user"] = json!({"login": "tehas-bot[bot]", "id": 100001, "type": "Bot"});
    site.comments()[1]["updated_at"] = "2026-11-20T10:05:00Z".into();
    // Touched but never edited: a reaction moved it, say.
    site.comments()[0]["updated_at"] = "2026-11-20T10:01:00Z".into();
    let (mut gh, _, calls) = adapter(site);
    let c = gh.change(nine(), bound(100)).unwrap();
    assert_eq!(c.comments[0].editor, None);
    assert_eq!(c.comments[0].edited, None);
    let editor = c.comments[1].editor.as_ref().unwrap();
    assert_eq!(editor.login.as_str(), "octo-person");
    assert_eq!(editor.kind, AccountKind::Person);
    assert_eq!(c.comments[1].edited, Some(at("2026-11-20T10:05:00Z")));
    assert_eq!(
        c.comments[1].author.as_ref().unwrap().kind,
        AccountKind::Bot
    );
    // Still one question: the pull request's and the page's edits together.
    assert_eq!(asked(&calls).iter().filter(|c| *c == "graphql").count(), 1);
    let q = calls.lock().unwrap()[2].clone();
    assert!(
        q.contains("\"IC_kwDOAbCdEf6Vt0000\", \"IC_kwDOAbCdEf6Vt0001\""),
        "{q}"
    );
}

#[test]
fn a_failed_editor_read_fails_the_answer() {
    let mut site = Site::default().with("comments", 1);
    site.graphql = Some(failed("read-server-error.stderr"));
    let (mut gh, _, _) = adapter(site);
    assert_eq!(
        gh.change(nine(), bound(100)).map(|_| ()),
        Err(Outcome::Unavailable {
            why: Passing::Server,
            sent: Sent::Maybe
        })
    );
    let mut site = Site::default().with("comments", 1);
    site.graphql = Some(failed("read-rate-limited.stderr"));
    let (mut gh, _, _) = adapter(site);
    assert_eq!(
        gh.change(nine(), bound(100)).map(|_| ()),
        Err(Outcome::Spent {
            pool: Pool::Priced,
            until: At::new(now())
        })
    );
}

#[test]
fn a_later_page_with_an_edited_comment_asks_its_own_question() {
    let mut site = Site::default().with("comments", 101).edited(
        "IC_kwDOAbCdEf6Vt0100",
        Some(("Bot", "other-bot", 300003)),
        "2026-11-20T10:07:00Z",
    );
    site.comments()[100]["updated_at"] = "2026-11-20T10:07:00Z".into();
    let (mut gh, _, calls) = adapter(site);
    let c = gh.change(nine(), bound(200)).unwrap();
    let editor = c.comments[100].editor.as_ref().unwrap();
    assert_eq!(
        (editor.login.as_str(), editor.kind),
        ("other-bot", AccountKind::Bot)
    );
    assert_eq!(asked(&calls).iter().filter(|c| *c == "graphql").count(), 2);
}

// The files, the reviews and their notes.

#[test]
fn files_are_read_by_their_count_and_past_the_bound_cut() {
    let (mut gh, _, calls) = adapter(Site::default().with("files", 101));
    let c = gh.change(nine(), bound(200)).unwrap();
    assert_eq!(c.files.len(), 101);
    assert_eq!(c.files[100].path.as_str(), "src/part_100.rs");
    assert_eq!((c.files[0].added, c.files[0].deleted), (4, 1));
    let calls = asked(&calls);
    assert!(calls.contains(&page("files", 2)));
    // More than one page: read again at the end.
    assert_eq!(calls.last().unwrap(), PULL);
    let (mut gh, _, calls) = adapter(Site::default().with("files", 101));
    assert_eq!(
        gh.change(nine(), bound(100)).map(|_| ()),
        Err(Outcome::Unread(Unread::Cut))
    );
    assert_eq!(asked(&calls), [PULL]);
}

#[test]
fn a_file_twice_is_inconsistent() {
    let mut site = Site::default().with("files", 2);
    site.lists.get_mut("files").unwrap()[1]["filename"] = "src/part_0.rs".into();
    let (mut gh, _, _) = adapter(site);
    assert_eq!(
        gh.change(nine(), bound(100)).map(|_| ()),
        Err(Outcome::Unread(Unread::Inconsistent))
    );
}

#[test]
fn reviews_carry_their_text_and_their_notes_and_a_pending_one_is_left_out() {
    let mut site = Site::default().with("reviews", 3).with("notes", 3);
    let reviews = site.lists.get_mut("reviews").unwrap();
    reviews[1]["state"] = "CHANGES_REQUESTED".into();
    reviews[1]["body"] = "".into();
    reviews[2]["state"] = "PENDING".into();
    reviews[2]["submitted_at"] = Value::Null;
    let notes = site.lists.get_mut("notes").unwrap();
    notes[0]["pull_request_review_id"] = 3_100_000_001u64.into();
    notes[1]["pull_request_review_id"] = 3_100_000_002u64.into();
    notes[2]["pull_request_review_id"] = 3_100_000_000u64.into();
    let (mut gh, _, calls) = adapter(site);
    let c = gh.change(nine(), bound(100)).unwrap();
    assert_eq!(c.reviews.len(), 2);
    let first = &c.reviews[0];
    assert_eq!(first.verdict, Verdict::Approved);
    assert_eq!(first.at, at("2026-11-20T10:10:00Z"));
    assert_eq!(first.by.as_ref().unwrap().login.as_str(), "octo-person");
    let notes: Vec<(Option<&str>, &str)> = first
        .notes
        .iter()
        .map(|n| (n.path.as_ref().map(|p| p.as_str()), n.body.as_str()))
        .collect();
    assert_eq!(
        notes,
        [(None, "Review 0."), (Some("src/parts.rs"), "Note 2.")]
    );
    let second = &c.reviews[1];
    assert_eq!(second.verdict, Verdict::ChangesAsked);
    assert_eq!(second.notes.len(), 1);
    assert_eq!(second.notes[0].body, "Note 0.");
    assert_eq!(
        asked(&calls),
        [
            PULL.to_string(),
            "graphql".into(),
            page("reviews", 1),
            page("notes", 1)
        ]
    );
}

#[test]
fn reviews_past_the_bound_are_cut_and_a_full_page_reads_the_next() {
    let (mut gh, _, calls) = adapter(Site::default().with("reviews", 100));
    assert_eq!(gh.change(nine(), bound(100)).unwrap().reviews.len(), 100);
    // A full page may have one more behind it.
    assert!(asked(&calls).contains(&page("reviews", 2)));
    let (mut gh, _, _) = adapter(Site::default().with("reviews", 101));
    assert_eq!(
        gh.change(nine(), bound(100)).map(|_| ()),
        Err(Outcome::Unread(Unread::Cut))
    );
}

#[test]
fn a_review_of_another_pull_request_is_inconsistent() {
    let mut site = Site::default().with("reviews", 1);
    site.lists.get_mut("reviews").unwrap()[0]["pull_request_url"] =
        "https://api.github.com/repos/acme/widget/pulls/8".into();
    let (mut gh, _, _) = adapter(site);
    assert_eq!(
        gh.change(nine(), bound(100)).map(|_| ()),
        Err(Outcome::Unread(Unread::Inconsistent))
    );
}

// What is not this change.

#[test]
fn an_issues_number_is_not_found_with_one_request() {
    let (mut gh, _, calls) = adapter(Site::default());
    assert_eq!(
        gh.change(ChangeNo::new(7).unwrap(), bound(100)).map(|_| ()),
        Err(Outcome::NotFound)
    );
    assert_eq!(asked(&calls), ["api repos/acme/widget/pulls/7"]);
}

#[test]
fn an_answer_of_another_repository_or_number_is_inconsistent() {
    let mut site = Site::default();
    site.pull["html_url"] = "https://github.com/octo/tools/pull/9".into();
    let (mut gh, _, calls) = adapter(site);
    assert_eq!(
        gh.change(nine(), bound(100)).map(|_| ()),
        Err(Outcome::Unread(Unread::Inconsistent))
    );
    // Read once more, then answered.
    assert_eq!(asked(&calls), [PULL, PULL]);
    let mut site = Site::default();
    site.pull["number"] = 8.into();
    let (mut gh, _, _) = adapter(site);
    assert_eq!(
        gh.change(nine(), bound(100)).map(|_| ()),
        Err(Outcome::Unread(Unread::Inconsistent))
    );
    let mut site = Site::default();
    site.node["number"] = 8.into();
    let (mut gh, _, _) = adapter(site);
    assert_eq!(
        gh.change(nine(), bound(100)).map(|_| ()),
        Err(Outcome::Unread(Unread::Inconsistent))
    );
}

// Each failure, on the first read and on a later page.

fn failures() -> Vec<(String, Outcome)> {
    let maybe = |why| Outcome::Unavailable {
        why,
        sent: Sent::Maybe,
    };
    vec![
        (failed("read-signed-out.stderr"), Outcome::SignedOut),
        (failed("read-forbidden.stderr"), Outcome::Forbidden),
        (
            failed("read-rate-limited.stderr"),
            Outcome::Spent {
                pool: Pool::Calls,
                until: At::new(now()),
            },
        ),
        (failed("read-not-found.stderr"), Outcome::NotFound),
        (failed("read-server-error.stderr"), maybe(Passing::Server)),
        (failed("read-timed-out.stderr"), maybe(Passing::Timeout)),
    ]
}

#[test]
fn each_failure_of_the_first_read_is_its_outcome() {
    for (why, want) in failures() {
        let mut site = Site::default();
        site.fail
            .insert("repos/acme/widget/pulls/9".into(), why.clone());
        let (mut gh, _, calls) = adapter(site);
        assert_eq!(
            gh.change(nine(), bound(100)).map(|_| ()),
            Err(want),
            "{why}"
        );
        assert_eq!(asked(&calls), [PULL], "{why}");
    }
}

#[test]
fn each_failure_of_a_later_page_fails_the_whole_read() {
    let lists = [
        (
            "comments",
            "repos/acme/widget/issues/9/comments?per_page=100&page=2",
        ),
        (
            "files",
            "repos/acme/widget/pulls/9/files?per_page=100&page=2",
        ),
        (
            "reviews",
            "repos/acme/widget/pulls/9/reviews?per_page=100&page=2",
        ),
        (
            "notes",
            "repos/acme/widget/pulls/9/comments?per_page=100&page=2",
        ),
    ];
    for (list, path) in lists {
        for (why, want) in failures() {
            let mut site = Site::default().with(list, 150);
            site.fail.insert(path.into(), why.clone());
            let (mut gh, _, _) = adapter(site);
            assert_eq!(
                gh.change(nine(), bound(200)).map(|_| ()),
                Err(want),
                "{list}: {why}"
            );
        }
    }
}

#[test]
fn a_malformed_answer_first_or_later_is_malformed() {
    let site = Site {
        pull: json!("not a pull request"),
        ..Site::default()
    };
    let (mut gh, _, _) = adapter(site);
    assert_eq!(
        gh.change(nine(), bound(100)).map(|_| ()),
        Err(Outcome::Unread(Unread::Malformed))
    );
    let mut site = Site::default().with("comments", 150);
    site.after.push((
        ("comments", 1),
        Box::new(|s: &mut Site| {
            s.lists.get_mut("comments").unwrap()[100] = json!({"id": "one hundred"});
        }),
    ));
    let (mut gh, _, _) = adapter(site);
    assert_eq!(
        gh.change(nine(), bound(200)).map(|_| ()),
        Err(Outcome::Unread(Unread::Malformed))
    );
}

// The consumer's settle, and the laws.

#[test]
fn the_comments_answer_what_the_settle_of_a_maybe_sent_comment_compares() {
    // The core's rule (`journal::settle::by_comments`) takes a comment of
    // the factory's own account made since the row, with the row's body to
    // the byte and its markers. The adapter answers the body as GitHub
    // keeps it and `created` as GitHub's; it cannot tell its own account
    // yet, so its author is a Bot and the rule finds nothing until the
    // site's wiring names the factory's account `Ours`.
    let sent = "The refresh is done.\n\nSee the new head.";
    let mut site = Site::default().with("comments", 2);
    site.comments()[1]["user"] = json!({"login": "tehas-bot[bot]", "id": 100001, "type": "Bot"});
    site.comments()[1]["body"] = sent.into();
    site.comments()[1]["created_at"] = "2026-11-20T10:00:30Z".into();
    site.comments()[1]["updated_at"] = "2026-11-20T10:00:30Z".into();
    let (mut gh, _, _) = adapter(site);
    let mut read = gh.change(nine(), bound(100)).unwrap().comments;
    let mine = &read[1];
    assert_eq!(mine.body.as_bytes(), sent.as_bytes());
    assert_eq!(mine.created, at("2026-11-20T10:00:30Z"));
    assert!(mine.markers.is_empty());
    let author = mine.author.as_ref().unwrap();
    assert_eq!(author.kind, AccountKind::Bot);
    assert_eq!(author.id.as_ref().unwrap().as_str(), "100001");
    let row = tehas_ports::Write::CommentChange {
        no: nine(),
        body: tehas_ports::Body::Text(sent.into()),
        markers: Vec::new(),
    };
    let since = at("2026-11-20T10:00:00Z");
    use tehas_core::journal::{settle::by_comments, Found};
    assert_eq!(by_comments(&row, &read, since), Some(Found::NotSent));
    for c in &mut read {
        if let Some(a) = c
            .author
            .as_mut()
            .filter(|a| a.id.as_ref().is_some_and(|i| i.as_str() == "100001"))
        {
            a.kind = AccountKind::Ours;
        }
    }
    assert_eq!(by_comments(&row, &read, since), Some(Found::Landed));
}

#[test]
fn the_marks_answer_what_the_settle_of_a_maybe_sent_mark_compares() {
    use tehas_core::journal::{settle::by_marks, Found};
    let (mut gh, _, _) = adapter(Site::default());
    let marks = gh.change(nine(), bound(100)).unwrap().digest.marks;
    let running = Mark::State(State::Running);
    let on = tehas_ports::Write::MarkChange {
        no: nine(),
        mark: running.clone(),
    };
    let off = tehas_ports::Write::UnmarkChange {
        no: nine(),
        mark: running,
    };
    assert_eq!(by_marks(&on, &marks), Some(Found::Landed));
    assert_eq!(by_marks(&off, &marks), Some(Found::NotSent));
}

#[test]
fn law_past_the_bound_is_unread_for_comments_files_and_reviews() {
    for list in ["comments", "files", "reviews"] {
        let (mut gh, _, _) = adapter(Site::default().with(list, 3));
        let law = laws::lists::past_the_bound_is_unread(
            |b| {
                gh.change(nine(), b).map(|c| match list {
                    "comments" => Whole::new(
                        c.comments
                            .iter()
                            .map(|c| c.id.as_str().to_string())
                            .collect(),
                    ),
                    "files" => Whole::new(
                        c.files
                            .iter()
                            .map(|f| f.path.as_str().to_string())
                            .collect(),
                    ),
                    _ => Whole::new(c.reviews.iter().map(|r| format!("{:?}", r.at)).collect()),
                })
            },
            3,
        );
        assert_eq!(law, Ok(()), "{list}");
    }
}

#[test]
fn updated_is_at_or_after_every_time_of_the_change() {
    // The law `code_host::updated_is_at_or_after_every_time` reads through
    // `changes` with bodies, which this adapter does not answer yet; its
    // rule is checked here on the one change's read.
    let mut site = Site::default().edited("pull", None, "2026-11-20T10:20:00Z");
    site.pull["state"] = "closed".into();
    site.pull["closed_at"] = "2026-11-20T10:25:00Z".into();
    let (mut gh, _, _) = adapter(site);
    let d = gh.change(nine(), bound(100)).unwrap().digest;
    let ended = match d.state {
        ChangeState::Closed { at } => at,
        other => panic!("{other:?}"),
    };
    assert!(d.updated >= d.created && Some(d.updated) >= d.edited && d.updated >= ended);
    // An editor GitHub no longer names is still an edit.
    assert_eq!(d.editor.unwrap().login.as_str(), "ghost");
}

#[test]
fn page_arithmetic_at_the_boundaries() {
    for (held, pages) in [(99, 1), (100, 1), (101, 2), (199, 2), (200, 2), (201, 3)] {
        let (mut gh, _, calls) = adapter(Site::default().with("comments", held));
        assert_eq!(gh.change(nine(), bound(300)).unwrap().comments.len(), held);
        let calls = asked(&calls);
        for p in 1..=pages + 1 {
            assert_eq!(
                calls.contains(&page("comments", p)),
                p <= pages,
                "{held}: page {p}"
            );
        }
        // Read again at the end only past one page.
        assert_eq!(
            calls.iter().filter(|c| *c == PULL).count(),
            1 + usize::from(pages > 1)
        );
    }
}
