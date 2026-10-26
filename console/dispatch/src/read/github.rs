//! What a snapshot asks of GitHub: the open issues in pages of 100 with
//! everything the rules read of each, the marks on closed issues and pull
//! requests, and the reads the bash makes of one issue at a time, in the
//! shapes libexec/dispatch reads them.

use super::run::{Calls, GhFail};
use serde_json::{json, Map, Value};

const PAGE: &str = r#"query($owner: String!, $name: String!, $cursor: String) {
  repository(owner: $owner, name: $name) {
    nameWithOwner
    issues(states: OPEN, first: 100, after: $cursor) {
      pageInfo { hasNextPage endCursor }
      nodes {
        number title body lastEditedAt editor { login }
        labels(first: 50) { nodes { name } }
        subIssuesSummary { total completed }
        timelineItems(itemTypes: [LABELED_EVENT, UNLABELED_EVENT, RENAMED_TITLE_EVENT], last: 100) {
          nodes {
            __typename
            ... on LabeledEvent { createdAt actor { login } label { name } }
            ... on UnlabeledEvent { createdAt actor { login } label { name } }
            ... on RenamedTitleEvent { createdAt actor { login } }
          }
        }
        closedByPullRequestsReferences(first: 20) { nodes { url state } }
      }
    }
    closedRunning: issues(states: CLOSED, labels: ["tehas:running"], first: 100) { nodes { number } }
  }
}"#;

const COMMENTS: &str = r#"query($owner: String!, $name: String!, $number: Int!) {
      repository(owner: $owner, name: $name) { issue(number: $number) {
        comments(last: 100) { nodes { databaseId author { login } body createdAt lastEditedAt editor { login }
          userContentEdits(last: 100) { totalCount nodes { editor { login } } } } } } } }"#;

const PULL: &str = r#"query($owner: String!, $name: String!, $number: Int!) { repository(owner: $owner, name: $name) {
      pullRequest(number: $number) { state number url isDraft isCrossRepository headRefOid baseRefName baseRefOid mergeable
        author { login } headRepository { nameWithOwner } labels(first: 50) { nodes { name } }
        closingIssuesReferences(first: 5) { totalCount nodes { number state labels(first: 50) { nodes { name } }
          closedByPullRequestsReferences(first: 10) { nodes { number state } } } } } } }"#;

/// One open issue as the page gave it.
pub struct Open {
    pub number: u64,
    pub labels: Vec<String>,
    pub node: Value,
}

/// The open issues, the repository's name and the closed issues still marked
/// tehas:running.
pub struct Queue {
    pub repo: Option<String>,
    pub open: Vec<Open>,
    pub closed_running: Vec<u64>,
}

fn names(v: Option<&Value>) -> Vec<String> {
    v.and_then(|l| l.get("nodes"))
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.get("name").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn split(repo: &str) -> (String, String) {
    let (o, n) = repo.split_once('/').unwrap_or((repo, ""));
    (format!("owner={o}"), format!("name={n}"))
}

/// The open issues, page by page.
pub fn queue(calls: &mut Calls, repo: &str) -> Result<Queue, GhFail> {
    let (owner, name) = split(repo);
    let mut q = Queue {
        repo: None,
        open: Vec::new(),
        closed_running: Vec::new(),
    };
    let mut cursor: Option<String> = None;
    loop {
        let query = format!("query={PAGE}");
        let cur = cursor.as_ref().map(|c| format!("cursor={c}"));
        let mut args = vec!["api", "graphql", "-F", &owner, "-F", &name, "-f", &query];
        if let Some(c) = cur.as_deref() {
            args.extend(["-f", c]);
        }
        let v = calls.gh(&args)?;
        let r = v
            .pointer("/data/repository")
            .cloned()
            .unwrap_or(Value::Null);
        q.repo = r
            .get("nameWithOwner")
            .and_then(Value::as_str)
            .map(str::to_string);
        for n in r
            .pointer("/issues/nodes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(number) = n.get("number").and_then(Value::as_u64) else {
                continue;
            };
            q.open.push(Open {
                number,
                labels: names(n.get("labels")),
                node: n.clone(),
            });
        }
        if cursor.is_none() {
            q.closed_running = r
                .pointer("/closedRunning/nodes")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.get("number").and_then(Value::as_u64))
                        .collect()
                })
                .unwrap_or_default();
        }
        let more = r.pointer("/issues/pageInfo/hasNextPage") == Some(&json!(true));
        let next = r
            .pointer("/issues/pageInfo/endCursor")
            .and_then(Value::as_str)
            .map(str::to_string);
        match (more, next) {
            (true, Some(n)) => cursor = Some(n),
            _ => break,
        }
    }
    Ok(q)
}

/// The label events of an issue as the REST events list gives them.
pub fn events(node: &Value) -> Value {
    let list: Vec<Value> = node
        .pointer("/timelineItems/nodes")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| {
                    let actor = match e.pointer("/actor/login").and_then(Value::as_str) {
                        Some(l) => json!({"login": l}),
                        None => Value::Null,
                    };
                    let at = e.get("createdAt").cloned().unwrap_or(Value::Null);
                    match e.get("__typename").and_then(Value::as_str)? {
                        "LabeledEvent" => Some(json!({"event": "labeled", "label": e.get("label"), "actor": actor, "created_at": at})),
                        "UnlabeledEvent" => Some(json!({"event": "unlabeled", "label": e.get("label"), "actor": actor, "created_at": at})),
                        "RenamedTitleEvent" => Some(json!({"event": "renamed", "actor": actor, "created_at": at})),
                        _ => None,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    Value::Array(list)
}

/// What an open issue answers to each read of the bash's: the REST issue
/// (work), the edit record (edit), gh issue view (view) and the pull
/// requests that close it (prs).
pub fn issue(o: &Open) -> Map<String, Value> {
    let n = &o.node;
    let labels: Vec<Value> = o.labels.iter().map(|l| json!({"name": l})).collect();
    let mut work = json!({"number": o.number, "state": "open", "title": n.get("title"),
        "body": n.get("body"), "labels": labels});
    if let Some(s) = n.get("subIssuesSummary").filter(|s| s.is_object()) {
        work["sub_issues_summary"] = s.clone();
    }
    let mut m = Map::new();
    m.insert("work".into(), work);
    m.insert(
        "edit".into(),
        json!({"lastEditedAt": n.get("lastEditedAt"), "editor": n.get("editor"),
            "body": n.get("body"), "title": n.get("title")}),
    );
    m.insert(
        "view".into(),
        json!({"title": n.get("title"), "body": n.get("body"), "state": "OPEN", "labels": labels}),
    );
    m.insert(
        "prs".into(),
        n.pointer("/closedByPullRequestsReferences/nodes")
            .cloned()
            .unwrap_or_else(|| json!([])),
    );
    m.insert("events".into(), events(n));
    m
}

/// The comments of several issues, read side by side.
pub fn comments_of(calls: &mut Calls, repo: &str, numbers: &[u64]) -> Vec<Value> {
    let (owner, name) = split(repo);
    let query = format!("query={COMMENTS}");
    let list: Vec<Vec<String>> = numbers
        .iter()
        .map(|n| {
            ["api", "graphql", "-F", &owner, "-F", &name, "-F", &format!("number={n}"), "-f", &query]
                .iter()
                .map(|s| s.to_string())
                .collect()
        })
        .collect();
    calls
        .gh_all(&list)
        .into_iter()
        .map(|r| r.unwrap_or_else(|_| json!({"failed": true})))
        .collect()
}

/// The comments of one issue, as the bash's question and blocker reads have
/// them; {"failed": true} when the call failed.
pub fn comments(calls: &mut Calls, repo: &str, n: u64) -> Value {
    let (owner, name) = split(repo);
    let num = format!("number={n}");
    let query = format!("query={COMMENTS}");
    calls
        .gh(&[
            "api", "graphql", "-F", &owner, "-F", &name, "-F", &num, "-f", &query,
        ])
        .unwrap_or_else(|_| json!({"failed": true}))
}

/// The states of the issues a blocker comment names, by number.
pub fn blocker_states(calls: &mut Calls, repo: &str, numbers: &[u64]) -> Map<String, Value> {
    let mut out = Map::new();
    if numbers.is_empty() {
        return out;
    }
    let (owner, name) = split(repo);
    let mut q = String::from(
        "query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) {",
    );
    for b in numbers {
        q.push_str(&format!(
            " b{b}: issueOrPullRequest(number: {b}) {{ ... on Issue {{ state }} ... on PullRequest {{ state }} }}"
        ));
    }
    q.push_str(" } }");
    let query = format!("query={q}");
    // As the bash: what came back counts, whatever gh's exit, so a number
    // GitHub does not know spoils only itself.
    if let Some(v) = calls.gh_any(&["api", "graphql", "-F", &owner, "-F", &name, "-f", &query]) {
        for b in numbers {
            if let Some(s) = v
                .pointer(&format!("/data/repository/b{b}/state"))
                .and_then(Value::as_str)
            {
                out.insert(b.to_string(), json!(s));
            }
        }
    }
    out
}

/// One pull request as a refresh reads it: .data.repository.pullRequest.
pub fn pull(calls: &mut Calls, repo: &str, pr: u64) -> Value {
    let (owner, name) = split(repo);
    let num = format!("number={pr}");
    let query = format!("query={PULL}");
    match calls.gh(&[
        "api", "graphql", "-F", &owner, "-F", &name, "-F", &num, "-f", &query,
    ]) {
        Ok(v) => v
            .pointer("/data/repository/pullRequest")
            .cloned()
            .unwrap_or(Value::Null),
        Err(_) => json!({"failed": true}),
    }
}

/// A list gh answers, or {"failed": true}.
pub fn list(calls: &mut Calls, args: &[&str]) -> Value {
    calls.gh(args).unwrap_or_else(|_| json!({"failed": true}))
}
