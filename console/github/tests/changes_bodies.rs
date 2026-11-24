//! `CodeHost::changes` with bodies, on a scripted gh that keeps a few pull
//! requests of a made-up project and answers both the list's question and
//! the one change's reads, so a listed body can be held against
//! `CodeHost::change`. The laws of `tehas_ports::laws` that read changes
//! with bodies run on the same site. No test calls GitHub.

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tehas_core::forge::GhRunner;
use tehas_github::GitHub;
use tehas_ports::laws;
use tehas_ports::{
    BlueprintNo, Bound, ChangeDigest, ChangeFilter, ChangeNo, ChangeStates, CodeHost, CommitId,
    Got, Outcome, Ref, Unread, Whole, ALL_MAX,
};

const HERE: u64 = 500_001;
const BASE: &str = "0123456789abcdef0123456789abcdef01234567";

/// One pull request the site keeps: its number, `OPEN` or `MERGED`, its
/// body as GitHub keeps it (None for a node with no body) and the issues
/// GitHub records it closes.
#[derive(Clone)]
struct Pr {
    no: u64,
    state: &'static str,
    body: Option<&'static str>,
    closes: Vec<u64>,
}

fn pr(no: u64, state: &'static str, body: &'static str, closes: &[u64]) -> Pr {
    Pr {
        no,
        state,
        body: Some(body),
        closes: closes.to_vec(),
    }
}

/// The site's pull requests: a body with this change's marker, a pull
/// request description that closes #7, one whose closing words stand only
/// in a quote, code and a code span, an empty body, and a merged one.
fn prs() -> Vec<Pr> {
    vec![
        pr(
            3,
            "OPEN",
            "Adds the parts list.\n\n<!-- tehas:question kind=plan issue=3 turn=1 -->",
            &[],
        ),
        pr(
            4,
            "OPEN",
            "## Summary\n\nWidgets count their parts.\n\nFixes #7.",
            &[7],
        ),
        pr(
            5,
            "OPEN",
            "Notes only.\n\n> Fixes #8.\n\n```\nCloses #9\n```\n\nSee `fixes #10` in the guide.",
            &[],
        ),
        pr(6, "OPEN", "", &[]),
        pr(8, "MERGED", "Resolves #2", &[2]),
    ]
}

fn sha(no: u64) -> String {
    format!("{no:02}").repeat(20)
}

fn at(minutes: u64) -> String {
    format!("2026-11-20T10:{minutes:02}:00Z")
}

/// What GitHub records the pull request closes, as GraphQL answers it.
macro_rules! closing {
    ($p:expr) => {{
    let p: &Pr = $p;
    let nodes: Vec<Value> = p
        .closes
        .iter()
        .map(|n| json!({"number": n, "repository": {"databaseId": HERE, "nameWithOwner": "acme/widget"}}))
        .collect();
    json!({"totalCount": nodes.len(), "nodes": nodes})
    }};
}

/// The pull request as a node of the list's question answers it, its
/// body only when the question asks it.
macro_rules! list_node {
    ($p:expr, $body:expr) => {{
    let (p, asked): (&Pr, bool) = ($p, $body);
    let merged = p.state == "MERGED";
    let mut n = json!({
        "number": p.no, "title": format!("Change {}", p.no), "state": p.state,
        "isDraft": false, "isCrossRepository": false,
        "headRepository": {"databaseId": HERE},
        "headRefName": format!("topic-{}", p.no), "headRefOid": sha(p.no),
        "baseRefName": "main", "baseRefOid": BASE,
        "createdAt": at(0), "updatedAt": at(p.no),
        "mergedAt": if merged { json!(at(p.no)) } else { Value::Null },
        "closedAt": if merged { json!(at(p.no)) } else { Value::Null },
        "lastEditedAt": null, "additions": 1, "deletions": 0, "changedFiles": 0,
        "mergeable": "MERGEABLE", "mergeStateStatus": "CLEAN",
        "url": format!("https://github.com/acme/widget/pull/{}", p.no),
        "author": {"__typename": "User", "login": "alice", "databaseId": 101},
        "editor": null,
        "mergedBy": if merged { json!({"__typename": "User", "login": "bob", "databaseId": 102}) } else { Value::Null },
        "mergeCommit": if merged { json!({"oid": "cd".repeat(20)}) } else { Value::Null },
        "labels": {"totalCount": 0, "nodes": []},
        "closingIssuesReferences": closing!(p),
    });
    if let Some(b) = p.body.filter(|_| asked) {
        n["body"] = b.into();
    }
    n
    }};
}

/// The pull request as its REST read answers it.
macro_rules! rest_pull {
    ($p:expr) => {{
    let p: &Pr = $p;
    let merged = p.state == "MERGED";
    json!({
        "number": p.no,
        "html_url": format!("https://github.com/acme/widget/pull/{}", p.no),
        "title": format!("Change {}", p.no),
        "body": p.body,
        "state": if merged { "closed" } else { "open" },
        "draft": false, "merged": merged,
        "merged_at": if merged { json!(at(p.no)) } else { Value::Null },
        "closed_at": if merged { json!(at(p.no)) } else { Value::Null },
        "created_at": at(0), "updated_at": at(p.no),
        "merge_commit_sha": if merged { json!("cd".repeat(20)) } else { Value::Null },
        "merged_by": if merged { json!({"login": "bob", "id": 102, "type": "User"}) } else { Value::Null },
        "user": {"login": "alice", "id": 101, "type": "User"},
        "author_association": "MEMBER",
        "head": {"ref": format!("topic-{}", p.no), "sha": sha(p.no)},
        "base": {"ref": "main", "sha": BASE},
        "labels": [], "comments": 0, "review_comments": 0,
        "additions": 1, "deletions": 0, "changed_files": 0,
    })
    }};
}

type Asked = Arc<Mutex<Vec<String>>>;

/// The scripted gh: the list's question `per` nodes a page, the cursor the
/// offset, narrowed by the states it asks; and the reads of one change.
struct Site {
    prs: Vec<Pr>,
    per: usize,
    asked: Asked,
}

impl Site {
    fn list(&self, args: &[&str], q: &str) -> Result<String, String> {
        let states = q.split("states: [").nth(1).ok_or("no states")?;
        let states = &states[..states.find(']').ok_or("no states")?];
        let taken: Vec<&Pr> = self
            .prs
            .iter()
            .filter(|p| states.contains(p.state))
            .collect();
        let from: usize = args
            .iter()
            .find_map(|a| a.strip_prefix("after="))
            .map_or(0, |c| c.parse().unwrap());
        let to = (from + self.per).min(taken.len());
        let nodes: Vec<Value> = taken[from..to]
            .iter()
            .map(|p| list_node!(p, q.contains(" body ")))
            .collect();
        Ok(
            json!({"data": {"repository": {"databaseId": HERE, "pullRequests": {
                "totalCount": taken.len(),
                "pageInfo": {"hasNextPage": to < taken.len(), "endCursor": to.to_string()},
                "nodes": nodes,
            }}}})
            .to_string(),
        )
    }

    fn find(&self, no: u64) -> Result<&Pr, String> {
        self.prs
            .iter()
            .find(|p| p.no == no)
            .ok_or_else(|| format!("no pull request {no}"))
    }

    fn answer(&self, args: &[&str]) -> Result<String, String> {
        if args.starts_with(&["api", "graphql"]) {
            let q = args
                .iter()
                .find_map(|a| a.strip_prefix("query="))
                .ok_or("no query")?;
            if q.contains("pullRequests(") {
                return self.list(args, q);
            }
            let no: u64 = args
                .iter()
                .find_map(|a| a.strip_prefix("number="))
                .and_then(|n| n.parse().ok())
                .ok_or("no number")?;
            let p = self.find(no)?;
            let node = json!({
                "number": no, "headRefOid": sha(no), "updatedAt": at(no),
                "isCrossRepository": false, "mergeable": "MERGEABLE",
                "mergeStateStatus": "CLEAN", "lastEditedAt": null, "editor": null,
                "closingIssuesReferences": closing!(p),
            });
            return Ok(
                json!({"data": {"repository": {"databaseId": HERE, "pullRequest": node}}})
                    .to_string(),
            );
        }
        let path = args
            .get(1)
            .and_then(|p| p.strip_prefix("repos/acme/widget/pulls/"))
            .ok_or_else(|| format!("no answer to {args:?}"))?;
        match path.split_once('/') {
            None => Ok(rest_pull!(self.find(path.parse().map_err(|_| "no number")?)?).to_string()),
            Some((_, "reviews?per_page=100&page=1")) => Ok("[]".into()),
            _ => Err(format!("no answer to {args:?}")),
        }
    }
}

impl GhRunner for Site {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        self.asked.lock().unwrap().push(args.join(" "));
        self.answer(args)
    }
}

fn site_of(prs: Vec<Pr>, per: usize) -> (GitHub, Asked) {
    let asked = Asked::default();
    let site = Site {
        prs,
        per,
        asked: Arc::clone(&asked),
    };
    (GitHub::new("acme/widget", Box::new(site)), asked)
}

fn site() -> (GitHub, Asked) {
    site_of(prs(), 100)
}

fn bodies(states: ChangeStates) -> ChangeFilter {
    ChangeFilter {
        states,
        bodies: true,
        ..ChangeFilter::default()
    }
}

fn bound(n: u32) -> Bound {
    Bound::new(n).unwrap()
}

fn listed(got: Got<Whole<ChangeDigest>>) -> Vec<ChangeDigest> {
    got.unwrap().into_items()
}

fn of(list: &[ChangeDigest], no: u64) -> &ChangeDigest {
    list.iter().find(|d| d.no.get() == no).unwrap()
}

fn here(n: u64) -> Ref<BlueprintNo> {
    Ref::Here(BlueprintNo::new(n).unwrap())
}

#[test]
fn each_listed_body_is_read_without_its_markers_with_what_it_says_it_closes() {
    let (mut gh, asked) = site();
    let list = listed(gh.changes(&bodies(ChangeStates::All), bound(10)));
    let nos: Vec<u64> = list.iter().map(|d| d.no.get()).collect();
    assert_eq!(nos, [3, 4, 5, 6, 8]);
    // This change's marker is taken out.
    assert_eq!(of(&list, 3).body.as_deref(), Some("Adds the parts list."));
    assert!(of(&list, 3).says_closes.is_empty());
    // A pull request description that closes #7.
    let d = of(&list, 4);
    assert_eq!(
        d.body.as_deref(),
        Some("## Summary\n\nWidgets count their parts.\n\nFixes #7.")
    );
    assert_eq!(d.says_closes, [here(7)]);
    // Closing words in a quote, a fence and a code span close nothing.
    assert!(of(&list, 5)
        .body
        .as_deref()
        .unwrap()
        .contains("> Fixes #8."));
    assert!(of(&list, 5).says_closes.is_empty());
    // An empty body is an empty body, given.
    assert_eq!(of(&list, 6).body.as_deref(), Some(""));
    assert!(of(&list, 6).says_closes.is_empty());
    assert_eq!(of(&list, 8).says_closes, [here(2)]);
    // One question, the bodies in it.
    let asked = asked.lock().unwrap();
    assert_eq!(asked.len(), 1);
    assert!(asked[0].contains(" body "));
}

#[test]
fn without_bodies_the_question_asks_none_and_the_digests_carry_none() {
    let (mut gh, asked) = site();
    let all = ChangeFilter {
        states: ChangeStates::All,
        ..ChangeFilter::default()
    };
    for d in listed(gh.changes(&all, bound(10))) {
        assert_eq!(d.body, None);
        assert!(d.says_closes.is_empty());
    }
    assert!(!asked.lock().unwrap()[0].contains(" body "));
}

#[test]
fn a_listed_body_is_what_the_one_changes_read_gives() {
    let (mut gh, _) = site();
    let list = listed(gh.changes(&bodies(ChangeStates::All), bound(10)));
    for d in &list {
        let one = gh.change(d.no, bound(10)).unwrap();
        assert_eq!(d.body, one.digest.body, "{}", d.no.get());
        assert_eq!(d.body.as_deref(), Some(one.body.as_str()), "{}", d.no.get());
        assert_eq!(d.says_closes, one.digest.says_closes, "{}", d.no.get());
        assert_eq!(d.closes, one.digest.closes, "{}", d.no.get());
    }
}

#[test]
fn bodies_over_pages_are_one_question_a_page_as_without() {
    let (mut gh, asked) = site_of(prs(), 2);
    let with = listed(gh.changes(&bodies(ChangeStates::All), bound(10)));
    assert_eq!(with.len(), 5);
    assert!(with.iter().all(|d| d.body.is_some()));
    let with_asked = asked.lock().unwrap().len();
    let (mut gh, asked) = site_of(prs(), 2);
    let all = ChangeFilter {
        states: ChangeStates::All,
        ..ChangeFilter::default()
    };
    gh.changes(&all, bound(10)).unwrap();
    assert_eq!(with_asked, 3);
    assert_eq!(asked.lock().unwrap().len(), with_asked);
}

#[test]
fn a_list_with_bodies_past_its_bound_is_cut_never_whole() {
    let (mut gh, asked) = site();
    assert_eq!(
        gh.changes(&bodies(ChangeStates::All), bound(4)),
        Err(Outcome::Unread(Unread::Cut))
    );
    assert_eq!(asked.lock().unwrap().len(), 1);
    // Over pages too: the bound is met on the second page, no third read.
    let (mut gh, asked) = site_of(prs(), 2);
    assert_eq!(
        gh.changes(&bodies(ChangeStates::All), bound(3)),
        Err(Outcome::Unread(Unread::Cut))
    );
    assert_eq!(asked.lock().unwrap().len(), 2);
}

#[test]
fn a_node_with_no_body_or_a_body_not_text_is_malformed_never_empty() {
    let mut gone = prs();
    gone[3].body = None;
    let (mut gh, _) = site_of(gone.clone(), 100);
    assert_eq!(
        gh.changes(&bodies(ChangeStates::Open), bound(10)),
        Err(Outcome::Unread(Unread::Malformed))
    );
    // Without bodies the same page is read whole.
    let (mut gh, _) = site_of(gone, 100);
    assert_eq!(
        listed(gh.changes(&ChangeFilter::default(), bound(10))).len(),
        4
    );
}

#[test]
fn a_change_the_filter_leaves_out_is_not_read_for_its_body() {
    let mut list = prs();
    list[4].body = None;
    let (mut gh, _) = site_of(list, 100);
    // #8 is merged: an open list never reads its body.
    let got = listed(gh.changes(&bodies(ChangeStates::Open), bound(10)));
    assert_eq!(got.len(), 4);
}

#[test]
fn the_laws_of_changes_with_bodies_hold() {
    let (mut gh, _) = site();
    assert_eq!(
        laws::code_host::updated_is_at_or_after_every_time(&mut gh),
        Ok(())
    );
    let (mut gh, _) = site();
    assert_eq!(
        laws::code_host::a_change_gives_its_base_and_what_its_text_closes(
            &mut gh,
            ChangeNo::new(4).unwrap(),
            &CommitId::new(BASE).unwrap(),
            &[here(7)],
        ),
        Ok(())
    );
    let (mut gh, _) = site();
    let read = |b| gh.changes(&bodies(ChangeStates::All), b);
    assert_eq!(laws::lists::past_the_bound_is_unread(read, 5), Ok(()));
    assert!(ALL_MAX.get() >= 5);
}
