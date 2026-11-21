//! The issues and pull requests of the repository, for the MCP server's
//! ticket tools (docs/specs/2026-11-21-mcp-tickets.md).
//!
//!   console tickets <owner/name> <dir>
//!
//! Reads, with one GitHub search a page, the issues and pull requests updated
//! since the last run, oldest change first, and writes one file for each in
//! <dir>, `<number>.json`: its title, state, body, labels, comments, the
//! epic it is part of and its sub-issues, and for a pull request the commit it
//! merged as and the issues it closes. `tehasbot status` runs it on each poll
//! while the MCP server is on, with <dir> the private `~/tehas/mcp/tickets`.
//!
//! At most `PAGES` pages a run, so the first read of a long history is spread
//! over polls. `.cursor.json` holds where the next run goes on. A page that
//! fails ends the run, and what it wrote stays.
//!
//! Exit: 0 read, or nothing new; 1 GitHub gave no whole answer, or a file
//! could not be written; 64 usage.

use serde_json::{json, Map, Value};
use std::fs;
use std::path::Path;
use tehas_core::file::{write_atomic, Temp};
use tehas_core::{call, time};

/// Items a page, and pages a run.
const FIRST: usize = 50;
pub const PAGES: usize = 4;
/// The most of a body and of a comment a file keeps, in characters.
const BODY_MAX: usize = 64 * 1024;
const COMMENT_MAX: usize = 16 * 1024;
pub const CURSOR: &str = ".cursor.json";

/// A search's answer for its query and page cursor.
type Ask<'a> = dyn FnMut(&str, Option<&str>) -> Result<String, String> + 'a;

const FIELDS: &str = "number title url state createdAt updatedAt closedAt body
  author { login }
  labels(first: 30) { nodes { name } }
  comments(first: 50) { totalCount nodes { author { login } createdAt body } }";

const QUERY: &str = r#"query($q: String!, $after: String) {
  search(query: $q, type: ISSUE, first: FIRST, after: $after) {
    pageInfo { hasNextPage endCursor }
    nodes {
      __typename
      ... on Issue { FIELDS stateReason
        parent { number }
        subIssues(first: 100) { nodes { number title state } } }
      ... on PullRequest { FIELDS mergedAt
        mergeCommit { oid }
        closingIssuesReferences(first: 20) { nodes { number } } }
    }
  }
}"#;

pub fn main(given: &[String]) -> u8 {
    let [repo, dir] = given else {
        eprintln!("usage: console tickets <owner/name> <dir>");
        return 64;
    };
    if repo.split_once('/').is_none() {
        eprintln!("console tickets: {repo} is not owner/name");
        return 64;
    }
    let dir = Path::new(dir);
    if let Err(e) = fs::create_dir_all(dir) {
        eprintln!("console tickets: {} cannot be made: {e}", dir.display());
        return 1;
    }
    match run(repo, dir, &time::iso(time::now()), &mut |q, after| {
        gh(q, after)
    }) {
        Ok(_) => 0,
        Err(why) => {
            eprintln!("console tickets: {why}");
            1
        }
    }
}

fn gh(q: &str, after: Option<&str>) -> Result<String, String> {
    let query = QUERY
        .replace("FIELDS", FIELDS)
        .replace("FIRST", &FIRST.to_string());
    let mut args = vec![
        "api".to_string(),
        "graphql".to_string(),
        "-f".to_string(),
        format!("query={query}"),
        "-f".to_string(),
        format!("q={q}"),
    ];
    if let Some(a) = after {
        args.extend(["-f".to_string(), format!("after={a}")]);
    }
    let out = call::run("gh", &args, call::GH_BOUND);
    if !out.ok() {
        return Err("GitHub did not answer".into());
    }
    Ok(out.out)
}

/// The search of one run: the items of `repo` updated at or after `since`,
/// oldest change first. Without `since`, all of them.
fn search_of(repo: &str, since: Option<&str>) -> String {
    match since {
        Some(t) => format!("repo:{repo} sort:updated-asc updated:>={t}"),
        None => format!("repo:{repo} sort:updated-asc"),
    }
}

/// One run: up to `PAGES` pages from the cursor, each item written as it
/// comes, the cursor moved after each page. Returns how many were written.
pub fn run(
    repo: &str,
    dir: &Path,
    now: &str,
    ask: &mut Ask,
) -> Result<usize, String> {
    let mut cursor: Value = fs::read(dir.join(CURSOR))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(json!({}));
    let mut written = 0;
    for _ in 0..PAGES {
        let since = cursor["since"].as_str().map(str::to_string);
        let after = cursor["after"].as_str().map(str::to_string);
        // A search goes on from its own page cursor while it has one, with
        // the time it began with; a new one starts from the last change read.
        let q = match (&after, cursor["query"].as_str()) {
            (Some(_), Some(q)) => q.to_string(),
            _ => search_of(repo, since.as_deref()),
        };
        let page: Value = serde_json::from_str(&ask(&q, after.as_deref())?)
            .map_err(|_| "the answer is no JSON".to_string())?;
        if page.get("errors").is_some() {
            return Err("the answer carries errors".into());
        }
        let search = page
            .pointer("/data/search")
            .ok_or("the answer holds no search")?;
        let nodes = search["nodes"]
            .as_array()
            .ok_or("the answer holds no list")?;
        let mut last = since.clone();
        for n in nodes {
            let Some(item) = ticket(n, now) else { continue };
            let number = item["number"].as_u64().unwrap_or(0);
            let body = format!("{item}\n");
            write_atomic(
                &dir.join(format!("{number}.json")),
                body.as_bytes(),
                0o600,
                &Temp::Pid,
            )
            .map_err(|e| format!("#{number} not written: {e}"))?;
            written += 1;
            if let Some(t) = n["updatedAt"].as_str().filter(|t| time::iso_shaped(t)) {
                last = Some(t.to_string());
            }
        }
        let more = search.pointer("/pageInfo/hasNextPage") == Some(&json!(true));
        let end = search
            .pointer("/pageInfo/endCursor")
            .and_then(Value::as_str);
        cursor = match (more, end) {
            // GitHub's search ends at 1000 items: a page cursor goes on within
            // that, and the next search starts again from the last change read.
            (true, Some(end)) => json!({"since": since, "after": end, "query": q, "read": now}),
            _ => json!({"since": last, "read": now}),
        };
        write_atomic(
            &dir.join(CURSOR),
            format!("{cursor}\n").as_bytes(),
            0o600,
            &Temp::Pid,
        )
        .map_err(|e| format!("{CURSOR} not written: {e}"))?;
        if !more {
            break;
        }
    }
    Ok(written)
}

fn s(v: &Value) -> Value {
    v.as_str().map_or(Value::Null, |s| json!(s))
}

fn cut(v: &Value, max: usize) -> Value {
    v.as_str().map_or(Value::Null, |s| {
        json!(s.chars().take(max).collect::<String>())
    })
}

fn numbers(v: &Value) -> Value {
    Value::Array(
        v.as_array()
            .into_iter()
            .flatten()
            .filter_map(|n| n["number"].as_u64().map(|n| json!(n)))
            .collect(),
    )
}

/// The file of one search node, or None for a node that is not an issue or
/// a pull request of the repository.
fn ticket(n: &Value, now: &str) -> Option<Value> {
    let kind = match n["__typename"].as_str()? {
        "Issue" => "issue",
        "PullRequest" => "pull",
        _ => return None,
    };
    let number = n["number"].as_u64().filter(|n| *n >= 1)?;
    let mut o = Map::new();
    o.insert("api".into(), json!(1));
    o.insert("kind".into(), json!(kind));
    o.insert("number".into(), json!(number));
    for k in [
        "title",
        "url",
        "createdAt",
        "updatedAt",
        "closedAt",
        "mergedAt",
    ] {
        let v = s(&n[k]);
        if !v.is_null() {
            o.insert(k.into(), v);
        }
    }
    let state = match (n["state"].as_str()?, kind) {
        ("OPEN", _) => "open",
        ("MERGED", _) => "merged",
        ("CLOSED", _) => "closed",
        _ => return None,
    };
    o.insert("state".into(), json!(state));
    if let Some(r) = n["stateReason"].as_str() {
        o.insert("stateReason".into(), json!(r.to_ascii_lowercase()));
    }
    o.insert("author".into(), s(&n["author"]["login"]));
    o.insert("body".into(), cut(&n["body"], BODY_MAX));
    o.insert(
        "labels".into(),
        Value::Array(
            n.pointer("/labels/nodes")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|l| l["name"].as_str().map(|s| json!(s)))
                .collect(),
        ),
    );
    if let Some(p) = n["parent"]["number"].as_u64() {
        o.insert("parent".into(), json!(p));
    }
    if kind == "issue" {
        let subs: Vec<Value> = n
            .pointer("/subIssues/nodes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|x| {
                Some(
                    json!({"number": x["number"].as_u64()?, "title": cut(&x["title"], 300),
                    "state": x["state"].as_str()?.to_ascii_lowercase()}),
                )
            })
            .collect();
        o.insert("subIssues".into(), Value::Array(subs));
    } else {
        o.insert("mergeCommit".into(), s(&n["mergeCommit"]["oid"]));
        o.insert(
            "closes".into(),
            numbers(&n["closingIssuesReferences"]["nodes"]),
        );
    }
    let comments: Vec<Value> = n
        .pointer("/comments/nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|c| {
            json!({"author": s(&c["author"]["login"]), "createdAt": s(&c["createdAt"]),
                "body": cut(&c["body"], COMMENT_MAX)})
        })
        .collect();
    o.insert(
        "commentsTotal".into(),
        json!(n["comments"]["totalCount"]
            .as_u64()
            .unwrap_or(comments.len() as u64)),
    );
    o.insert("comments".into(), Value::Array(comments));
    o.insert("readAt".into(), json!(now));
    Some(Value::Object(o))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tehas_core::testdir::TestDir;

    const REPO: &str = "acme/widget";

    fn issue(n: u64, updated: &str) -> Value {
        json!({"__typename": "Issue", "number": n, "title": format!("Issue {n}"),
            "url": format!("https://github.com/{REPO}/issues/{n}"), "state": "CLOSED",
            "stateReason": "COMPLETED", "createdAt": "2026-01-01T00:00:00Z",
            "updatedAt": updated, "closedAt": updated, "body": "Why: because.",
            "author": {"login": "alice"}, "labels": {"nodes": [{"name": "epic"}]},
            "parent": {"number": 2}, "subIssues": {"nodes": [{"number": 9, "title": "Part", "state": "OPEN"}]},
            "comments": {"totalCount": 3, "nodes": [{"author": {"login": "bob"}, "createdAt": updated, "body": "Agreed."}]}})
    }

    fn pull(n: u64, updated: &str) -> Value {
        json!({"__typename": "PullRequest", "number": n, "title": "A change", "state": "MERGED",
            "updatedAt": updated, "mergedAt": updated, "body": "Closes #7", "author": {"login": "bot"},
            "labels": {"nodes": []}, "mergeCommit": {"oid": "abc123"},
            "closingIssuesReferences": {"nodes": [{"number": 7}]},
            "comments": {"totalCount": 0, "nodes": []}})
    }

    fn page(nodes: Vec<Value>, next: Option<&str>) -> String {
        json!({"data": {"search": {"pageInfo": {"hasNextPage": next.is_some(), "endCursor": next}, "nodes": nodes}}})
            .to_string()
    }

    fn read(dir: &Path, n: u64) -> Value {
        serde_json::from_slice(&fs::read(dir.join(format!("{n}.json"))).unwrap()).unwrap()
    }

    #[test]
    fn an_issue_and_a_pull_request_are_written_with_their_links() {
        let d = TestDir::new("tickets-write");
        let mut asked = Vec::new();
        let n = run(REPO, &d, "2026-11-21T12:00:00Z", &mut |q, after| {
            asked.push((q.to_string(), after.map(str::to_string)));
            Ok(page(
                vec![
                    issue(5, "2026-11-01T00:00:00Z"),
                    pull(8, "2026-11-02T00:00:00Z"),
                    json!({"__typename": "Discussion"}),
                ],
                None,
            ))
        })
        .unwrap();
        assert_eq!(n, 2);
        assert_eq!(asked, vec![(format!("repo:{REPO} sort:updated-asc"), None)]);
        let i = read(&d, 5);
        assert_eq!(i["kind"], "issue");
        assert_eq!(i["state"], "closed");
        assert_eq!(i["stateReason"], "completed");
        assert_eq!(i["parent"], 2);
        assert_eq!(
            i["subIssues"],
            json!([{"number": 9, "title": "Part", "state": "open"}])
        );
        assert_eq!(i["labels"], json!(["epic"]));
        assert_eq!(i["comments"][0]["author"], "bob");
        assert_eq!(i["commentsTotal"], 3);
        let p = read(&d, 8);
        assert_eq!(p["kind"], "pull");
        assert_eq!(p["state"], "merged");
        assert_eq!(p["mergeCommit"], "abc123");
        assert_eq!(p["closes"], json!([7]));
        let c: Value = serde_json::from_slice(&fs::read(d.join(CURSOR)).unwrap()).unwrap();
        assert_eq!(c["since"], "2026-11-02T00:00:00Z");
        assert!(c.get("after").is_none());
    }

    #[test]
    fn the_next_run_reads_only_what_changed_since_the_last_change_read() {
        let d = TestDir::new("tickets-since");
        run(REPO, &d, "t", &mut |_, _| {
            Ok(page(vec![issue(5, "2026-11-01T00:00:00Z")], None))
        })
        .unwrap();
        let mut asked = Vec::new();
        run(REPO, &d, "t", &mut |q, _| {
            asked.push(q.to_string());
            Ok(page(vec![], None))
        })
        .unwrap();
        assert_eq!(
            asked,
            vec![format!(
                "repo:{REPO} sort:updated-asc updated:>=2026-11-01T00:00:00Z"
            )]
        );
    }

    // A long history is read a few pages a run: the cursor keeps the page it
    // stopped at, and the next run goes on from there with the same search.
    #[test]
    fn a_run_stops_after_its_pages_and_the_next_goes_on() {
        let d = TestDir::new("tickets-pages");
        let mut calls = 0;
        run(REPO, &d, "t", &mut |_, after| {
            calls += 1;
            let n = calls as u64;
            assert_eq!(after.is_some(), n > 1);
            Ok(page(
                vec![issue(n, &format!("2026-11-0{n}T00:00:00Z"))],
                Some(&format!("c{n}")),
            ))
        })
        .unwrap();
        assert_eq!(calls, PAGES);
        let mut asked = Vec::new();
        run(REPO, &d, "t", &mut |q, after| {
            asked.push((q.to_string(), after.map(str::to_string)));
            Ok(page(vec![], None))
        })
        .unwrap();
        assert_eq!(
            asked,
            vec![(
                format!("repo:{REPO} sort:updated-asc"),
                Some(format!("c{PAGES}"))
            )]
        );
    }

    #[test]
    fn a_failed_page_keeps_what_was_written_and_the_cursor_before_it() {
        let d = TestDir::new("tickets-fail");
        let mut calls = 0;
        let r = run(REPO, &d, "t", &mut |_, _| {
            calls += 1;
            if calls == 1 {
                Ok(page(vec![issue(5, "2026-11-01T00:00:00Z")], Some("c1")))
            } else {
                Err("GitHub did not answer".into())
            }
        });
        assert!(r.is_err());
        assert!(d.join("5.json").exists());
        let c: Value = serde_json::from_slice(&fs::read(d.join(CURSOR)).unwrap()).unwrap();
        assert_eq!(c["after"], "c1");
        for bad in [r#"{"errors":[{}]}"#, "{}", "no"] {
            let d = TestDir::new("tickets-bad");
            assert!(
                run(REPO, &d, "t", &mut |_, _| Ok(bad.to_string())).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn a_long_body_is_cut() {
        let mut n = issue(5, "2026-11-01T00:00:00Z");
        n["body"] = json!("x".repeat(BODY_MAX + 10));
        let t = ticket(&n, "t").unwrap();
        assert_eq!(t["body"].as_str().unwrap().len(), BODY_MAX);
    }
}
