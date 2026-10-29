//! The open epics of the repository, for the dashboard's epics panel.
//!
//!   console epics <owner/name> <out>
//!
//! An epic is an open issue with sub-issues, GitHub's own links, as the
//! dispatcher reads them (dispatch/src/decide.rs, epic_ok). This reads the open
//! issues a page at a time with gh, keeps the ones with sub-issues and writes
//! <out>, live/epics.json:
//!
//!   {generatedAt, repo, epics: [{number, title, url, total, completed, more,
//!     subIssues: [{number, title, url, state}]}]}
//!
//! Newest epic first; the sub-issues in the order the epic gives them, with
//! `state` "open" or "closed". `total` and `completed` are GitHub's count of
//! all of them. Only sub-issues of this repository are listed, the first
//! hundred: `more` is how many of the count are not, so a sub-issue elsewhere,
//! perhaps in a private repository, is never named on the public page. A closed
//! epic is not read, so it leaves the file at the next run.
//!
//! The file is written whole or not at all. A read that fails, or an answer
//! with errors or without the list, writes nothing and leaves the file there.
//!
//! Exit: 0 written; 1 GitHub gave no whole answer, or the file could not be
//! written; 64 usage.

use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use tehas_core::time;

/// At most this many pages of open issues are read: a thousand.
const PAGES: usize = 10;

const QUERY: &str = r#"query($owner: String!, $name: String!, $cursor: String) {
  repository(owner: $owner, name: $name) {
    issues(states: OPEN, first: 100, after: $cursor, orderBy: {field: CREATED_AT, direction: DESC}) {
      pageInfo { hasNextPage endCursor }
      nodes {
        number title url
        subIssuesSummary { total completed }
        subIssues(first: 100) { nodes { number title url state repository { nameWithOwner } } }
      }
    }
  }
}"#;

pub fn main(given: &[String]) -> u8 {
    let [repo, out] = given else {
        eprintln!("usage: console epics <owner/name> <out>");
        return 64;
    };
    let Some((owner, name)) = repo.split_once('/') else {
        eprintln!("console epics: {repo} is not owner/name");
        return 64;
    };
    let nodes = match read(|cursor| gh(owner, name, cursor)) {
        Ok(nodes) => nodes,
        Err(why) => {
            eprintln!("console epics: {why}, live/epics.json left as it was");
            return 1;
        }
    };
    let file = epics(repo, &nodes, &time::iso(time::now()));
    match put(Path::new(out), &format!("{file}\n")) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("console epics: {out} not written: {e}");
            1
        }
    }
}

fn gh(owner: &str, name: &str, cursor: Option<&str>) -> Result<String, String> {
    let mut cmd = Command::new("gh");
    cmd.args(["api", "graphql", "-f"])
        .arg(format!("query={QUERY}"))
        .args([
            "-f",
            &format!("owner={owner}"),
            "-f",
            &format!("name={name}"),
        ]);
    if let Some(c) = cursor {
        cmd.args(["-f", &format!("cursor={c}")]);
    }
    let out = cmd
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("gh: {e}"))?;
    if !out.status.success() {
        return Err("GitHub did not answer".into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The issue nodes of every page `ask` answers, given the cursor of the next.
fn read(mut ask: impl FnMut(Option<&str>) -> Result<String, String>) -> Result<Vec<Value>, String> {
    let mut nodes = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..PAGES {
        let page: Value = serde_json::from_str(&ask(cursor.as_deref())?)
            .map_err(|_| "the answer is no JSON".to_string())?;
        if page.get("errors").is_some() {
            return Err("the answer carries errors".into());
        }
        let issues = page.pointer("/data/repository/issues");
        let list = issues
            .and_then(|i| i.get("nodes"))
            .and_then(Value::as_array)
            .ok_or("the answer holds no list of issues")?;
        nodes.extend(list.iter().cloned());
        let next = issues
            .and_then(|i| i.pointer("/pageInfo/endCursor"))
            .and_then(Value::as_str)
            .filter(|_| {
                issues.and_then(|i| i.pointer("/pageInfo/hasNextPage")) == Some(&json!(true))
            });
        match next {
            Some(c) => cursor = Some(c.to_string()),
            None => break,
        }
    }
    Ok(nodes)
}

fn count(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn text(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .chars()
        .take(300)
        .collect()
}

/// The file: the nodes with sub-issues, newest first.
fn epics(repo: &str, nodes: &[Value], now: &str) -> Value {
    let mut list: Vec<Value> = nodes
        .iter()
        .filter_map(|n| {
            let number = n.get("number").and_then(Value::as_u64)?;
            let summary = n.get("subIssuesSummary")?;
            let total = count(summary, "total");
            if total == 0 {
                return None;
            }
            let subs: Vec<Value> = n
                .pointer("/subIssues/nodes")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|s| {
                    s.pointer("/repository/nameWithOwner")
                        .and_then(Value::as_str)
                        == Some(repo)
                })
                .filter_map(|s| {
                    let state = match s.get("state").and_then(Value::as_str)? {
                        "OPEN" => "open",
                        "CLOSED" => "closed",
                        _ => return None,
                    };
                    Some(json!({"number": s.get("number").and_then(Value::as_u64)?,
                        "title": text(s, "title"), "url": text(s, "url"), "state": state}))
                })
                .collect();
            Some(
                json!({"number": number, "title": text(n, "title"), "url": text(n, "url"),
                "total": total, "completed": count(summary, "completed").min(total),
                "more": total.saturating_sub(subs.len() as u64), "subIssues": subs}),
            )
        })
        .collect();
    list.sort_by_key(|e| std::cmp::Reverse(count(e, "number")));
    json!({"generatedAt": now, "repo": repo, "epics": list})
}

/// Writes the file whole: a side file renamed into place. The edge serves no
/// *.new, so a half-written file is never public.
fn put(out: &Path, body: &str) -> std::io::Result<()> {
    let mut side = out.as_os_str().to_owned();
    side.push(format!(".{}.new", std::process::id()));
    fs::write(&side, body)?;
    fs::rename(&side, out).inspect_err(|_| {
        let _ = fs::remove_file(&side);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPO: &str = "acme/widget";

    fn sub(n: u64, state: &str) -> Value {
        json!({"number": n, "title": format!("Sub {n}"), "url": format!("https://github.com/{REPO}/issues/{n}"),
            "state": state, "repository": {"nameWithOwner": REPO}})
    }

    fn issue(n: u64, total: u64, completed: u64, subs: Vec<Value>) -> Value {
        json!({"number": n, "title": format!("Issue {n}"), "url": format!("https://github.com/{REPO}/issues/{n}"),
            "subIssuesSummary": {"total": total, "completed": completed}, "subIssues": {"nodes": subs}})
    }

    fn page(nodes: Vec<Value>, next: Option<&str>) -> String {
        json!({"data": {"repository": {"issues": {
            "pageInfo": {"hasNextPage": next.is_some(), "endCursor": next}, "nodes": nodes}}}})
        .to_string()
    }

    #[test]
    fn an_issue_with_sub_issues_is_an_epic_and_one_without_is_not() {
        let nodes = [
            issue(10, 2, 1, vec![sub(11, "CLOSED"), sub(12, "OPEN")]),
            issue(13, 0, 0, vec![]),
        ];
        let f = epics(REPO, &nodes, "2026-10-29T10:00:00Z");
        assert_eq!(f["generatedAt"], "2026-10-29T10:00:00Z");
        assert_eq!(f["repo"], REPO);
        let list = f["epics"].as_array().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["number"], 10);
        assert_eq!(list[0]["total"], 2);
        assert_eq!(list[0]["completed"], 1);
        assert_eq!(list[0]["more"], 0);
        assert_eq!(
            list[0]["subIssues"],
            json!([
                {"number": 11, "title": "Sub 11", "url": "https://github.com/acme/widget/issues/11", "state": "closed"},
                {"number": 12, "title": "Sub 12", "url": "https://github.com/acme/widget/issues/12", "state": "open"},
            ])
        );
    }

    #[test]
    fn epics_are_newest_first_and_sub_issues_keep_their_order() {
        let nodes = [
            issue(5, 2, 0, vec![sub(9, "OPEN"), sub(7, "OPEN")]),
            issue(8, 1, 0, vec![sub(6, "OPEN")]),
        ];
        let f = epics(REPO, &nodes, "t");
        let numbers: Vec<_> = f["epics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["number"].clone())
            .collect();
        assert_eq!(numbers, [json!(8), json!(5)]);
        assert_eq!(f["epics"][1]["subIssues"][0]["number"], 9);
    }

    #[test]
    fn a_sub_issue_of_another_repository_is_counted_and_not_named() {
        let mut other = sub(3, "OPEN");
        other["repository"]["nameWithOwner"] = json!("acme/secret");
        let f = epics(REPO, &[issue(10, 3, 0, vec![sub(11, "OPEN"), other])], "t");
        let e = &f["epics"][0];
        assert_eq!(e["subIssues"].as_array().unwrap().len(), 1);
        assert_eq!(e["more"], 2);
        assert!(!f.to_string().contains("secret"));
    }

    #[test]
    fn odd_nodes_are_left_out() {
        let nodes = [
            json!({"title": "no number", "subIssuesSummary": {"total": 1}}),
            json!({"number": 4, "title": "no summary"}),
            issue(
                10,
                1,
                5,
                vec![
                    json!({"number": 1, "state": "MERGED", "repository": {"nameWithOwner": REPO}}),
                ],
            ),
        ];
        let f = epics(REPO, &nodes, "t");
        let list = f["epics"].as_array().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["completed"], 1, "completed is never over total");
        assert_eq!(list[0]["subIssues"], json!([]));
    }

    #[test]
    fn every_page_is_read_with_the_cursor_of_the_one_before() {
        let mut asked = Vec::new();
        let nodes = read(|c| {
            asked.push(c.map(str::to_string));
            Ok(match c {
                None => page(vec![issue(2, 0, 0, vec![])], Some("A")),
                _ => page(vec![issue(1, 0, 0, vec![])], None),
            })
        })
        .unwrap();
        assert_eq!(asked, [None, Some("A".to_string())]);
        assert_eq!(nodes.len(), 2);
    }

    #[test]
    fn the_pages_stop_at_the_bound() {
        let mut n = 0;
        read(|_| {
            n += 1;
            Ok(page(vec![], Some("again")))
        })
        .unwrap();
        assert_eq!(n, PAGES);
    }

    #[test]
    fn no_whole_answer_is_an_error() {
        for answer in [
            "",
            "{}",
            r#"{"errors":[{"message":"x"}],"data":null}"#,
            r#"{"data":{"repository":null}}"#,
        ] {
            assert!(read(|_| Ok(answer.to_string())).is_err(), "{answer}");
        }
        assert!(read(|_| Err("down".into())).is_err());
        let mut n = 0;
        let r = read(|_| {
            n += 1;
            if n == 1 {
                Ok(page(vec![], Some("A")))
            } else {
                Err("down".into())
            }
        });
        assert!(r.is_err(), "a later page that fails fails the read");
    }

    #[test]
    fn the_file_is_put_whole() {
        let dir = std::env::temp_dir().join(format!("epics-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let out = dir.join("epics.json");
        put(&out, "{}\n").unwrap();
        assert_eq!(fs::read_to_string(&out).unwrap(), "{}\n");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }
}
