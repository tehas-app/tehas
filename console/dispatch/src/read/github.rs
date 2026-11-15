//! What a snapshot asks of GitHub: the open issues in pages of 25 with
//! everything the rules read of each, the marks on closed issues and pull
//! requests, and the reads the bash makes of one issue at a time, in the
//! shapes the bash dispatcher read them.

use super::run::{Calls, GhFail};
use serde_json::{json, Map, Value};
use tehas_core::events::Events;

/// The mark a cycle's owner wears: applied by the bot alone when a start is
/// journaled, and taken off by the bot when the cycle ends. The rules that
/// read it - `marked` in `read/build.rs`, the park in `decide/clean.rs`, the
/// retry and plan rules - all read the mark itself or the lock file, never its
/// events, which is what lets `dropped_events` skip it for the queue. A rule
/// that starts deciding anything from the events of a label on this list has
/// to take it off the list first.
///
/// `host/labels.tsv` is where the label is created and described, and
/// `console/dispatch/src/finish.rs`, `console/cycle/src/report.rs`,
/// `console/ops/src/stop.rs` and `console/ops/src/tidy.rs` name it in their commands: a rename has to reach this constant and those sites, and the two
/// tests below tie it to the queue's page and the guard's exemption.
pub const RUNNING: &str = "tehas:running";

/// The open issues one page asks for. A page over GitHub's budget for nested
/// items comes back with its timelines cut short and no error, so the size is
/// load-bearing for correctness, and the page count rather than the points is
/// what the poll pays in: each page is one `gh` process run before any
/// decision. `test/dispatch_poll_test.sh` pins the number in the query, so a
/// change has to be a deliberate one.
const ISSUE_PAGE: usize = 25;

/// The queue's page. `{ISSUE_PAGE}` is filled in from the constant above by
/// `page_query`, so the number lives in one place.
const PAGE: &str = r#"query($owner: String!, $name: String!, $cursor: String) {
  repository(owner: $owner, name: $name) {
    nameWithOwner
    issues(states: OPEN, first: {ISSUE_PAGE}, after: $cursor) {
      pageInfo { hasNextPage endCursor }
      nodes {
        number title body createdAt author { login ...on User { databaseId } ...on Bot { databaseId } } lastEditedAt editor { login ...on User { databaseId } ...on Bot { databaseId } }
        labels(first: 50) { nodes { name } }
        subIssuesSummary { total completed }
        issueDependenciesSummary { blockedBy }
        parent { number }
        timelineItems(itemTypes: [LABELED_EVENT, UNLABELED_EVENT, RENAMED_TITLE_EVENT], last: 100) {
          nodes {
            __typename
            ... on LabeledEvent { createdAt actor { login ...on User { databaseId } ...on Bot { databaseId } } label { id name } }
            ... on UnlabeledEvent { createdAt actor { login ...on User { databaseId } ...on Bot { databaseId } } label { id name } }
            ... on RenamedTitleEvent { createdAt actor { login ...on User { databaseId } ...on Bot { databaseId } } }
          }
        }
        closedByPullRequestsReferences(first: 20) { nodes { url state isCrossRepository author { login ...on User { databaseId } ...on Bot { databaseId } } headRepository { nameWithOwner } } }{TYPE}
      }
    }
    closedRunning: issues(states: CLOSED, labels: ["tehas:running"], first: 100) { nodes { number } }{OPEN_PULLS}
  }
}"#;

/// The repository's `tehas:` labels with their node ids, on the first page
/// alone: what `crate::pins` compares with the ids it pinned.
const TEHAS_LABELS: &str = r#"
    tehasLabels: labels(first: 100, query: "tehas:") { nodes { id name } }"#;

/// What the hold of rule 6 reads of an open pull request. `body` is read
/// whole, and it is the one field of the page that a description can make
/// long. The author and head repository are what `build::own_pull` reads.
macro_rules! open_pull_fields {
    () => {
        "number body isCrossRepository author { login ...on User { databaseId } ...on Bot { databaseId } } headRepository { nameWithOwner }"
    };
}

/// The open pull requests, on the first page: the hold of rule 6 that reads a
/// body for a closing keyword needs no call of its own while they fit in one
/// list. Last in the page, and small, for the budget `ISSUE_PAGE` names. More
/// than one list are read on with `MORE_OPEN_PULLS`.
const OPEN_PULLS: &str = concat!(
    r#"
    openPulls: pullRequests(states: OPEN, first: 100, orderBy: {field: CREATED_AT, direction: DESC}) {
      pageInfo { hasNextPage endCursor }
      nodes { "#,
    open_pull_fields!(),
    " }\n    }"
);

/// The open pull requests after the first list of the queue's page.
const MORE_OPEN_PULLS: &str = concat!(
    r#"query($owner: String!, $name: String!, $cursor: String) { repository(owner: $owner, name: $name) {
      openPulls: pullRequests(states: OPEN, first: 100, after: $cursor, orderBy: {field: CREATED_AT, direction: DESC}) {
        pageInfo { hasNextPage endCursor }
        nodes { "#,
    open_pull_fields!(),
    " } } } }"
);

/// The most lists of 100 a read of the open pull requests goes on to. A
/// repository with more open is a read that failed.
const PULL_PAGES: usize = 50;

// The page above names `tehas:running` inside a GraphQL string, where the
// constant cannot reach it; this test is what ties the word in the query to
// `RUNNING`.

/// With `TEHAS_ISSUE_TYPES` mapping improvement: each issue's type.
const TYPE: &str = "\n        issueType { name }";

/// With `TEHAS_ISSUE_TYPES` mapping improvement, on the first page: the issue
/// types of the repository's owner.
const OWNER_TYPES: &str = "\n    issueTypes(first: 50) { nodes { name } }";

/// The queue's page as it is sent, its page size filled in from `ISSUE_PAGE`.
#[cfg(test)]
fn page_query(first: bool) -> String {
    page_query_typed(first, false)
}

/// The same, and with `types` each issue's type and, on the first page, the
/// owner's types.
fn page_query_typed(first: bool, types: bool) -> String {
    let owner_types = if first && types { OWNER_TYPES } else { "" };
    PAGE.replace("{ISSUE_PAGE}", &ISSUE_PAGE.to_string())
        .replace("{TYPE}", if types { TYPE } else { "" })
        .replace(
            "{OPEN_PULLS}",
            &format!(
                "{}{owner_types}",
                if first {
                    format!("{OPEN_PULLS}{TEHAS_LABELS}")
                } else {
                    String::new()
                }
            ),
        )
}

/// What a comment read asks of one issue: the one-issue query and the chunked
/// one ask the same fields, so an issue's answer is the same either way.
macro_rules! comment_fields {
    () => {
        r#"comments(last: 100) { nodes { databaseId id author { login ...on User { databaseId } ...on Bot { databaseId } } body createdAt lastEditedAt editor { login ...on User { databaseId } ...on Bot { databaseId } }
          userContentEdits(last: 100) { totalCount nodes { editor { login ...on User { databaseId } ...on Bot { databaseId } } } } } }"#
    };
}

const COMMENTS: &str = concat!(
    r#"query($owner: String!, $name: String!, $number: Int!) {
      repository(owner: $owner, name: $name) { issue(number: $number) {
        "#,
    comment_fields!(),
    " } } }"
);

/// The most issues one comment query asks of. Each asks up to 100 comments of
/// 100 edits, so ten issues stay well under GitHub's 500,000 nodes a query.
pub const COMMENT_CHUNK: usize = 10;

/// One aliased query for the comments of a chunk of issues: `i{n}` answers #n.
fn comments_query(chunk: &[u64]) -> String {
    let mut q = String::from(
        "query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) {",
    );
    for n in chunk {
        q.push_str(&format!(
            " i{n}: issue(number: {n}) {{ {} }}",
            comment_fields!()
        ));
    }
    q.push_str(" } }");
    q
}

/// What the refresh rule reads of a pull request and of the issue it closes.
/// `statusCheckRollup` is the one answer `ended_badly` reads (#1751): the runs
/// GitHub kept for the head, each with its name and its conclusion. The rule
/// asks only whether the check the instance names went red, so a run of any
/// other name says nothing.
macro_rules! pull_fields {
    () => {
        r#"number url isDraft isCrossRepository headRefOid baseRefName baseRefOid mergeable
        statusCheckRollup { nodes { __typename ...on CheckRun { name conclusion } ...on StatusContext { context state } } }
        author { login ...on User { databaseId } ...on Bot { databaseId } } headRepository { nameWithOwner } labels(first: 50) { nodes { name } }
        closingIssuesReferences(first: 5) { totalCount nodes { number state labels(first: 50) { nodes { name } }
          closedByPullRequestsReferences(first: 10) { nodes { number state } } } }"#
    };
}

const PULL: &str = concat!(
    r#"query($owner: String!, $name: String!, $number: Int!) { repository(owner: $owner, name: $name) {
      pullRequest(number: $number) { state "#,
    pull_fields!(),
    " } } }"
);

const PULLS: &str = concat!(
    r#"query($owner: String!, $name: String!, $cursor: String) { repository(owner: $owner, name: $name) {
      pullRequests(states: OPEN, first: 100, after: $cursor) { pageInfo { hasNextPage endCursor } nodes { "#,
    pull_fields!(),
    " } } } }"
);

/// One open issue as the page gave it.
pub struct Open {
    pub number: u64,
    pub labels: Vec<String>,
    pub node: Value,
}

/// The open issues, the repository's name, the closed issues still marked
/// tehas:running and the open pull requests, as the first page gave them.
pub struct Queue {
    pub repo: Option<String>,
    pub open: Vec<Open>,
    pub closed_running: Vec<u64>,
    /// The nodes of `openPulls` as the first page gave them: None when the page
    /// answered no such list, which is how a page cut short of GitHub's nested
    /// budget reads as nothing known rather than nothing open. Only the first
    /// page asks for the list, so a page after the first says nothing about it.
    pub open_pulls: Option<Value>,
    /// The issue types of the repository's owner, when the queue asked for
    /// them and the first page answered the repository: empty for an owner
    /// with none.
    pub issue_types: Option<Vec<String>>,
    /// The nodes of `tehasLabels` as the first page gave them, or None.
    pub labels: Option<Value>,
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

/// The open issues, page by page. `types` asks for the issue types too.
pub fn queue(calls: &mut Calls, repo: &str, types: bool) -> Result<Queue, GhFail> {
    let (owner, name) = split(repo);
    let mut q = Queue {
        repo: None,
        open: Vec::new(),
        closed_running: Vec::new(),
        open_pulls: None,
        issue_types: None,
        labels: None,
    };
    let mut cursor: Option<String> = None;
    loop {
        let query = format!("query={}", page_query_typed(cursor.is_none(), types));
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
        if cursor.is_none() && types && r.is_object() {
            q.issue_types = Some(names(r.get("issueTypes")));
        }
        if cursor.is_none() {
            q.open_pulls = rest_of(&r, "/openPulls", |c| {
                let cur = format!("cursor={c}");
                let query = format!("query={MORE_OPEN_PULLS}");
                calls
                    .gh(&[
                        "api", "graphql", "-F", &owner, "-F", &name, "-f", &query, "-f", &cur,
                    ])
                    .map(|v| {
                        v.pointer("/data/repository")
                            .cloned()
                            .unwrap_or(Value::Null)
                    })
            })?;
            q.labels = r.pointer("/tehasLabels/nodes").cloned();
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

/// The nodes of a list read on to its last page: `first` is the answer that
/// holds the first list at `at`, and `more` reads the list after a cursor,
/// answering what holds it at the same `at`. None when an answer holds no
/// list, which is nothing known rather than nothing there.
fn rest_of(
    first: &Value,
    at: &str,
    mut more: impl FnMut(&str) -> Result<Value, GhFail>,
) -> Result<Option<Value>, GhFail> {
    let mut page = first.clone();
    let mut nodes = Vec::new();
    for _ in 0..PULL_PAGES {
        let Some(list) = page
            .pointer(&format!("{at}/nodes"))
            .and_then(Value::as_array)
        else {
            return Ok(None);
        };
        nodes.extend(list.iter().cloned());
        let next = page
            .pointer(&format!("{at}/pageInfo"))
            .filter(|p| p.get("hasNextPage") == Some(&json!(true)))
            .and_then(|p| p.get("endCursor"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let Some(cursor) = next else {
            return Ok(Some(Value::Array(nodes)));
        };
        page = more(&cursor)?;
    }
    Err(GhFail {
        line: format!(
            "ERROR: more than {} open pull requests to read",
            PULL_PAGES * 100
        ),
        ran: None,
    })
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

/// Whether the page left out the label events the rules read.
///
/// For a label the rules ask who applied, or whether it still holds: a newest
/// event in the timeline that is not an application, or no event at all, means
/// the timeline disagrees with the labels. GitHub answers a large page with
/// timelines cut short or left out and no error, and a timeline cut in the
/// middle still holds an older application of the label, so the newest event is
/// what tells them apart. The newest event is `label_state`'s, so the guard
/// takes its ordering too: a removal sharing a stamp with an application comes
/// after it, and an event with no stamp is older than any, which is what
/// settles the guard at a one-second boundary.
///
/// `exempt` names the labels the caller does not ask about. The queue passes
/// the bot's own mark, `RUNNING`: the bot applies it alone, every running
/// cycle carries it, and nothing of the rules reads that mark's *timeline* -
/// `marked` lists the issues that carry it, and the park of one whose cycle is
/// gone reads the lock file and a fresh REST label read, never its events.
/// Guarding it would spend a paginated read on every running issue on every
/// poll, and no start is decided from it: a candidate carrying the mark is left
/// out of the ready list. What a start does rest on is read of `tehas:ready`,
/// `tehas:blocked`, `tehas:plan`, `tehas:refine` and
/// `tehas:lean` only, and the exemption reaches no other label - an issue carrying the mark beside a
/// person's label is still guarded by that one.
///
/// The exemption is not a waiver of the page's own shape. A node the page
/// answered with no timeline at all, or with one holding no application or
/// removal of any label the issue carries, is a dropped page whatever it
/// carries, so the exemption never rests on the absence of events.
///
/// The control issue passes nothing: every label it carries is compared, and
/// one that holds the factory-wide pause is never read from a page the guard
/// cannot vouch for.
pub fn dropped_events(o: &Open, exempt: &[&str]) -> bool {
    // What the page left out is read of the node itself, not of a timeline
    // re-parsed into the REST shape: a node with no `timelineItems/nodes`
    // array at all, or one holding no application or removal of a label the
    // issue carries, is a page that left those events out. That is the real
    // question, it is answered without building and parsing a second timeline,
    // and no `events()` change can quietly turn every issue into a dropped
    // page through an unreadable shape.
    let Some(nodes) = o
        .node
        .pointer("/timelineItems/nodes")
        .and_then(Value::as_array)
    else {
        return true;
    };
    // A label whose events the page gave: an application or a removal of it.
    let given = |l: &str| {
        nodes.iter().any(|e| {
            let named = |t: &str| {
                e.get("__typename").and_then(Value::as_str) == Some(t)
                    && e.pointer("/label/name").and_then(Value::as_str) == Some(l)
            };
            named("LabeledEvent") || named("UnlabeledEvent")
        })
    };
    // The exemption may cover an issue only when the page answered it with a
    // timeline at all: an issue carrying the bot's mark and nothing else, on a
    // node the page left blank, is still a page that was cut.
    if !o.labels.is_empty()
        && !o
            .labels
            .iter()
            .any(|l| given(l) || exempt.contains(&l.as_str()))
    {
        return true;
    }
    let ev = events(&o.node);
    o.labels
        .iter()
        .any(|l| !exempt.contains(&l.as_str()) && label_state_of(&ev, l) != "labeled")
}

/// An open issue the poll labels `epic`: its title starts with the marker and
/// it does not carry the label.
pub fn epic_titled(o: &Open) -> bool {
    let title = o.node.get("title").and_then(Value::as_str).unwrap_or("");
    crate::text::epic_marker(title) && !o.labels.iter().any(|l| l == crate::text::EPIC)
}

/// Whether the page's timeline holds an application or a removal of `label`.
pub fn has_event_of(o: &Open, label: &str) -> bool {
    o.node
        .pointer("/timelineItems/nodes")
        .and_then(Value::as_array)
        .is_some_and(|nodes| {
            nodes.iter().any(|e| {
                matches!(
                    e.get("__typename").and_then(Value::as_str),
                    Some("LabeledEvent" | "UnlabeledEvent")
                ) && e.pointer("/label/name").and_then(Value::as_str) == Some(label)
            })
        })
}

/// The newest labeled or unlabeled event of one label of a REST-shaped
/// timeline, as `events()` gives it: `"labeled"`, `"unlabeled"` or `""`.
/// `label_state`'s ordering, of which this asks only the kind: a removal
/// sharing a stamp with an application comes last, and an event with no stamp
/// is older than any.
fn label_state_of(v: &Value, label: &str) -> String {
    match Events::parse(v) {
        Events::Read(ev) => tehas_core::events::label_state(&ev, label)
            .event()
            .to_string(),
        // Nothing of a timeline that cannot be read says the label holds.
        Events::Failed | Events::Unreadable => String::new(),
    }
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
    if let Some(p) = n.pointer("/parent/number").filter(|p| p.is_u64()) {
        work["parent"] = p.clone();
    }
    // The open blockers of its native blocked-by links, in the REST answer's
    // shape: `blocked_by` counts the open ones, the closed ones hold nothing.
    // A forge that answers no such field leaves it out, and nothing is held.
    if let Some(b) = n
        .pointer("/issueDependenciesSummary/blockedBy")
        .filter(|b| b.is_u64())
    {
        work["issue_dependencies_summary"] = json!({"blocked_by": b});
    }
    let mut m = Map::new();
    m.insert("work".into(), work);
    m.insert(
        "edit".into(),
        json!({"lastEditedAt": n.get("lastEditedAt"), "editor": n.get("editor"),
            "body": n.get("body"), "title": n.get("title"),
            "createdAt": n.get("createdAt"), "author": n.get("author")}),
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

/// The comments of several issues, one aliased query per chunk of
/// `COMMENT_CHUNK`, the chunks side by side. Each issue's answer has the shape
/// `comments` gives, so a rule cannot tell the two reads apart. A chunk that
/// failed reads every issue of it as `failed_read`, as a failed `comments`
/// does, so a hold stays a hold; an alias the answer left out or null is that
/// one issue's failed read, with no line: no gh call failed.
pub fn comments_of(calls: &mut Calls, repo: &str, numbers: &[u64]) -> Vec<Value> {
    let (owner, name) = split(repo);
    let list: Vec<Vec<String>> = numbers
        .chunks(COMMENT_CHUNK)
        .map(|chunk| {
            [
                "api".to_string(),
                "graphql".to_string(),
                "-F".to_string(),
                owner.clone(),
                "-F".to_string(),
                name.clone(),
                "-f".to_string(),
                format!("query={}", comments_query(chunk)),
            ]
            .to_vec()
        })
        .collect();
    let answers = calls.gh_all(&list);
    numbers
        .chunks(COMMENT_CHUNK)
        .zip(answers)
        .flat_map(|(chunk, answer)| {
            chunk
                .iter()
                .map(|n| match &answer {
                    Ok(v) => match v.pointer(&format!("/data/repository/i{n}")) {
                        Some(issue) if issue.is_object() => {
                            json!({"data": {"repository": {"issue": issue}}})
                        }
                        _ => json!({"failed": true}),
                    },
                    Err(f) => failed_read(f),
                })
                .collect::<Vec<Value>>()
        })
        .collect()
}

/// The comments of one issue, as the bash's question and blocker reads have
/// them; `failed_read` when the call failed.
pub fn comments(calls: &mut Calls, repo: &str, n: u64) -> Value {
    let (owner, name) = split(repo);
    let num = format!("number={n}");
    let query = format!("query={COMMENTS}");
    calls
        .gh(&[
            "api", "graphql", "-F", &owner, "-F", &name, "-F", &num, "-f", &query,
        ])
        .unwrap_or_else(|f| failed_read(&f))
}

/// The marker of a read whose gh call failed, with the call's line: a rule
/// that ends the poll on the marker says what gh said, and a marker with no
/// line is a read no gh call failed for. The line holds only the words of
/// `gh_why`'s fixed list, so nothing gh printed beyond them reaches the log.
pub fn failed_read(f: &GhFail) -> Value {
    json!({"failed": true, "line": f.line})
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
        Err(f) => failed_read(&f),
    }
}

/// The open pull requests as tehasbot refresh reads them: gh's whole answer,
/// its list holding the nodes of every page.
pub fn pulls(calls: &mut Calls, repo: &str) -> Result<Value, GhFail> {
    let (owner, name) = split(repo);
    let query = format!("query={PULLS}");
    let mut first = calls.gh(&["api", "graphql", "-F", &owner, "-F", &name, "-f", &query])?;
    let all = rest_of(&first, "/data/repository/pullRequests", |c| {
        let cur = format!("cursor={c}");
        calls.gh(&[
            "api", "graphql", "-F", &owner, "-F", &name, "-f", &query, "-f", &cur,
        ])
    })?;
    if let (Some(all), Some(list)) = (all, first.pointer_mut("/data/repository/pullRequests")) {
        list["nodes"] = all;
    }
    Ok(first)
}

/// A list gh answers, or {"failed": true}.
pub fn list(calls: &mut Calls, args: &[&str]) -> Value {
    calls.gh(args).unwrap_or_else(|_| json!({"failed": true}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_and_the_one_pull_request_ask_the_same_fields() {
        let fields = pull_fields!();
        assert!(PULL.contains(&format!("{{ state {fields} }}")));
        assert!(PULLS.contains(&format!(
            "pageInfo {{ hasNextPage endCursor }} nodes {{ {fields} }}"
        )));
        assert!(PULLS.contains("pullRequests(states: OPEN, first: 100, after: $cursor)"));
        assert!(OPEN_PULLS.contains(&format!("nodes {{ {} }}", open_pull_fields!())));
        assert!(MORE_OPEN_PULLS.contains(&format!("nodes {{ {} }}", open_pull_fields!())));
        let args = [
            "api".to_string(),
            "graphql".to_string(),
            "-F".to_string(),
            "owner=a".to_string(),
            "-f".to_string(),
            format!("query={PULLS}"),
        ];
        assert!(crate::read::run::gh_reads(&args));
    }

    fn list(nodes: &[u64], next: Option<&str>) -> Value {
        json!({"l": {"pageInfo": {"hasNextPage": next.is_some(), "endCursor": next},
            "nodes": nodes.iter().map(|n| json!({"number": n})).collect::<Vec<_>>()}})
    }

    #[test]
    fn a_list_of_more_than_one_page_is_read_to_its_end() {
        let mut asked = Vec::new();
        let all = rest_of(&list(&[1, 2], Some("a")), "/l", |c| {
            asked.push(c.to_string());
            Ok(match c {
                "a" => list(&[3], Some("b")),
                _ => list(&[4], None),
            })
        });
        assert_eq!(asked, ["a", "b"]);
        assert_eq!(
            all.ok().flatten(),
            Some(json!([{"number": 1}, {"number": 2}, {"number": 3}, {"number": 4}]))
        );
    }

    #[test]
    fn a_list_that_one_page_does_not_answer_is_not_known() {
        let all = rest_of(&list(&[1], Some("a")), "/l", |_| Ok(json!({})));
        assert_eq!(all.ok(), Some(None));
        let failed = rest_of(&list(&[1], Some("a")), "/l", |_| {
            Err(GhFail {
                line: "ERROR: gh failed".into(),
                ran: None,
            })
        });
        assert!(failed.is_err());
        let endless = rest_of(&list(&[1], Some("a")), "/l", |_| Ok(list(&[1], Some("a"))));
        assert!(endless.is_err());
    }

    #[test]
    fn every_account_a_query_reads_comes_with_its_user_id() {
        // tehas_core::accounts counts a listed login only with its recorded id.
        let with_id = "{ login ...on User { databaseId } ...on Bot { databaseId } }";
        for q in [
            page_query(true),
            COMMENTS.to_string(),
            comments_query(&[7, 8]),
            PULL.to_string(),
            PULLS.to_string(),
            MORE_OPEN_PULLS.to_string(),
        ] {
            assert_eq!(
                q.matches("login").count(),
                q.matches(with_id).count(),
                "{q}"
            );
            assert!(q.contains(with_id), "{q}");
        }
    }

    #[test]
    fn an_issue_names_its_epic_and_the_page_asks_for_it() {
        assert!(PAGE.contains("parent { number }"));
        let o = |node: Value| Open {
            number: 7,
            labels: vec![],
            node,
        };
        let part = issue(&o(
            json!({"title": "t", "body": "b", "parent": {"number": 5}}),
        ));
        assert_eq!(part["work"]["parent"], json!(5));
        let alone = issue(&o(json!({"title": "t", "body": "b", "parent": null})));
        assert!(alone["work"].get("parent").is_none());
    }

    /// The page asks how many open issues block each one through a native
    /// blocked-by link, and the issue's REST-shaped answer carries the count.
    /// A forge answer without the field, or with one that is no count, carries
    /// nothing, so no link holds the issue.
    #[test]
    fn an_issue_carries_its_open_blocked_by_links_and_the_page_asks_for_them() {
        assert!(PAGE.contains("issueDependenciesSummary { blockedBy }"));
        let o = |node: Value| Open {
            number: 7,
            labels: vec![],
            node,
        };
        let held = issue(&o(json!({"title": "t", "body": "b",
            "issueDependenciesSummary": {"blockedBy": 2}})));
        assert_eq!(
            held["work"]["issue_dependencies_summary"],
            json!({"blocked_by": 2})
        );
        for node in [
            json!({"title": "t", "body": "b"}),
            json!({"title": "t", "body": "b", "issueDependenciesSummary": null}),
            json!({"title": "t", "body": "b", "issueDependenciesSummary": {"blockedBy": "2"}}),
        ] {
            let none = issue(&o(node.clone()));
            assert!(
                none["work"].get("issue_dependencies_summary").is_none(),
                "{node}"
            );
        }
    }

    /// A label event's kind. An unlisted kind does not compile, so a case
    /// cannot ask for an application and get a removal.
    #[derive(Clone, Copy)]
    enum Kind {
        Labeled,
        Unlabeled,
    }

    /// An issue with the labels it carries and the label events its page gave
    /// it, one per entry, a later entry a later event.
    ///
    /// The stamp and the actor are load-bearing, not decoration:
    /// `label_state` reads the newest event by `at`, so an event with no stamp
    /// is older than any and a removal sharing a stamp with an application
    /// comes after it. An assertion that left the stamps out would be testing
    /// that ordering instead of the guard. `None` for the stamp is such an
    /// unstamped event.
    fn stamped(labels: &[&str], events: &[(Kind, &str, Option<&str>)]) -> Open {
        let nodes: Vec<Value> = events
            .iter()
            .enumerate()
            .map(|(i, (kind, l, at))| {
                json!({"__typename": match kind {
                        Kind::Labeled => "LabeledEvent",
                        Kind::Unlabeled => "UnlabeledEvent",
                    },
                    "createdAt": at.unwrap_or(&format!("2026-01-0{}T00:00:00Z", i + 1)),
                    "actor": {"login": "alice"}, "label": {"name": l}})
            })
            .collect();
        Open {
            number: 7,
            labels: labels.iter().map(|l| l.to_string()).collect(),
            node: json!({"timelineItems": {"nodes": nodes}}),
        }
    }

    /// The same, with the events given as the applications of the labels in
    /// `applied`, each at its own time.
    fn open(labels: &[&str], applied: &[&str]) -> Open {
        let events: Vec<(Kind, &str, Option<&str>)> =
            applied.iter().map(|l| (Kind::Labeled, *l, None)).collect();
        stamped(labels, &events)
    }

    /// A page that gave the issue no timeline at all.
    fn no_timeline(labels: &[&str]) -> Open {
        Open {
            number: 7,
            labels: labels.iter().map(|l| l.to_string()).collect(),
            node: json!({}),
        }
    }

    /// The queue's question: the bot's own mark is the label it does not ask
    /// about. `&[]`, what the control issue passes, asks about every label.
    const QUEUE: &[&str] = &[RUNNING];
    const EVERYTHING: &[&str] = &[];

    #[test]
    fn an_epic_titled_issue_lacks_the_label_and_its_events_are_asked_of_it() {
        let mut o = open(&["bug"], &["bug"]);
        o.node["title"] = json!("EPIC: Billing rework");
        assert!(epic_titled(&o));
        assert!(!has_event_of(&o, "epic"));
        o.node["timelineItems"]["nodes"]
            .as_array_mut()
            .unwrap()
            .push(json!({"__typename": "UnlabeledEvent", "label": {"name": "epic"}}));
        assert!(has_event_of(&o, "epic"));
        o.labels.push("epic".into());
        assert!(!epic_titled(&o));
        o.node["title"] = json!("feat: a part of epic 412");
        o.labels.pop();
        assert!(!epic_titled(&o));
        assert!(!epic_titled(&no_timeline(&[])));
        assert!(!has_event_of(&no_timeline(&[]), "epic"));
    }

    #[test]
    fn a_label_no_event_applied_was_left_out() {
        assert!(dropped_events(&open(&["tehas:ready"], &[]), EVERYTHING));
        assert!(dropped_events(
            &open(&["tehas:ready", "bug"], &["bug"]),
            EVERYTHING
        ));
        // A label on an issue with no timeline at all is a page that left its
        // events out: the guard must notice the shape, not rest on the
        // tiebreak of unstamped events.
        assert!(dropped_events(&no_timeline(&["tehas:ready"]), EVERYTHING));
        assert!(!dropped_events(
            &open(&["tehas:ready"], &["tehas:ready"]),
            EVERYTHING
        ));
        assert!(!dropped_events(&open(&[], &[]), EVERYTHING));
    }

    #[test]
    fn a_timeline_cut_short_of_the_newest_approval_is_left_out() {
        // The page kept the older application and lost the one that came after
        // it, so the issue carries a label the timeline says was taken off.
        assert!(dropped_events(
            &stamped(
                &["tehas:ready"],
                &[
                    (Kind::Labeled, "tehas:ready", None),
                    (Kind::Unlabeled, "tehas:ready", None)
                ]
            ),
            EVERYTHING
        ));
        // And the newest event of the label really was an application.
        assert!(!dropped_events(
            &stamped(
                &["tehas:ready"],
                &[
                    (Kind::Unlabeled, "tehas:ready", None),
                    (Kind::Labeled, "tehas:ready", None)
                ]
            ),
            EVERYTHING
        ));
        // One label cut short is enough, whatever the others agree on.
        assert!(dropped_events(
            &stamped(
                &["tehas:ready", "bug"],
                &[
                    (Kind::Labeled, "bug", None),
                    (Kind::Labeled, "tehas:ready", None),
                    (Kind::Unlabeled, "tehas:ready", None)
                ]
            ),
            EVERYTHING
        ));
    }

    #[test]
    fn a_timeline_whose_newest_event_shares_a_stamp_is_left_out() {
        // The guard rests on `label_state`'s ordering, not on the stamps: a
        // removal sharing a stamp with an application is read last, and an
        // event with no stamp is older than any. A page that lost the
        // application of a label the issue still carries is judged cut both
        // when the events it kept share one second and when one carries none.
        assert!(dropped_events(
            &stamped(
                &["tehas:ready"],
                &[
                    (Kind::Labeled, "tehas:ready", Some("2026-01-01T00:00:00Z")),
                    (Kind::Unlabeled, "tehas:ready", Some("2026-01-01T00:00:00Z"))
                ]
            ),
            EVERYTHING
        ));
        assert!(dropped_events(
            &stamped(
                &["tehas:ready"],
                &[
                    (Kind::Labeled, "tehas:ready", None),
                    (Kind::Unlabeled, "tehas:ready", Some("2026-01-01T00:00:00Z"))
                ]
            ),
            EVERYTHING
        ));
    }

    #[test]
    fn a_timeline_holding_no_application_of_the_label_is_left_out() {
        // The page answered this node with a timeline, so its shape alone is
        // not the evidence; the label's newest event, a removal, is.
        assert!(dropped_events(
            &stamped(&["tehas:ready"], &[(Kind::Unlabeled, "tehas:ready", None)]),
            EVERYTHING
        ));
        // A timeline of other labels' events says nothing of this one.
        assert!(dropped_events(
            &stamped(&["tehas:ready", "bug"], &[(Kind::Labeled, "bug", None)]),
            EVERYTHING
        ));
    }

    #[test]
    fn a_cycle_marked_running_by_the_bot_is_left_alone_when_its_event_is_cut() {
        // The bot's own mark is the one label the queue does not ask about:
        // every running cycle carries it, the bot applies it alone, and the
        // park of a mark with no cycle behind it rests on the lock file.
        //
        // What the mark does not excuse is the page's own shape: an issue the
        // page gave no timeline at all is a dropped page whatever it carries,
        // so the exemption never rests on the absence of events.
        assert!(dropped_events(&no_timeline(&["tehas:running"]), QUEUE));
        // A timeline the page did answer is trusted: neither a history of the
        // mark cut short nor one that ends with it taken off asks for a read.
        assert!(!dropped_events(
            &stamped(
                &["tehas:running"],
                &[
                    (Kind::Labeled, "tehas:running", None),
                    (Kind::Unlabeled, "tehas:running", None)
                ]
            ),
            QUEUE
        ));
        // The page's answer need not name the mark: the one label the issue
        // carries is the bot's, so nothing is asked of that timeline.
        assert!(!dropped_events(
            &stamped(&["tehas:running"], &[(Kind::Labeled, "bug", None)]),
            QUEUE
        ));
        // What the mark does not excuse: a person's label on the same issue,
        // whose newest event is not an application, still asks for the read.
        assert!(dropped_events(
            &stamped(
                &["tehas:ready", "tehas:running"],
                &[
                    (Kind::Labeled, "tehas:ready", None),
                    (Kind::Labeled, "tehas:running", None),
                    (Kind::Unlabeled, "tehas:ready", None)
                ]
            ),
            QUEUE
        ));
    }

    #[test]
    fn an_open_control_issue_with_a_cut_pause_timeline_is_not_trusted() {
        // The control issue asks about every label it carries, so the mark's
        // exemption cannot reach it: the page kept the older application of
        // `tehas:pause` and lost the removal that came after it, and a poll
        // that read that timeline would start work on a host a person paused.
        let cut = |labels: &[&str]| {
            stamped(
                labels,
                &[
                    (Kind::Labeled, "tehas:pause", None),
                    (Kind::Unlabeled, "tehas:pause", None),
                ],
            )
        };
        // The control issue is a control issue whether or not a cycle happens
        // to have parked on it: asked about, its cut pause timeline is never
        // trusted, and exempting the bot's mark changes that answer not at all.
        assert!(dropped_events(&cut(&["tehas:pause"]), EVERYTHING));
        assert!(dropped_events(&cut(&["tehas:pause"]), QUEUE));
        assert!(dropped_events(
            &cut(&["tehas:pause", "tehas:running"]),
            EVERYTHING
        ));
        assert!(dropped_events(
            &cut(&["tehas:pause", "tehas:running"]),
            QUEUE
        ));
        // A timeline whose newest pause event is the application is one the
        // guard trusts: the guard answers only whether the timeline it was
        // given is whole, not who lifted the pause or whether they may.
        assert!(!dropped_events(
            &stamped(
                &["tehas:pause"],
                &[
                    (Kind::Unlabeled, "tehas:pause", None),
                    (Kind::Labeled, "tehas:pause", None),
                ]
            ),
            EVERYTHING
        ));
    }

    #[test]
    fn only_the_first_page_asks_for_the_open_pull_requests() {
        assert!(page_query(true).contains("openPulls: pullRequests(states: OPEN"));
        assert!(!page_query(false).contains("pullRequests"));
        assert!(!page_query(false).contains("{OPEN_PULLS}"));
    }

    #[test]
    fn the_issue_types_are_asked_only_with_the_setting_and_the_owners_on_the_first_page() {
        for first in [true, false] {
            let off = page_query(first);
            assert!(!off.contains("issueType") && !off.contains("{TYPE}"));
            let on = page_query_typed(first, true);
            assert!(on.contains("issueType { name }"));
            assert_eq!(on.contains("issueTypes(first: 50)"), first);
        }
    }

    #[test]
    fn the_page_asks_for_the_page_size_the_guard_rests_on() {
        // `first:` is not a tuning number: a page over GitHub's budget for
        // nested items answers with timelines cut short and no error. An edit
        // to the constant is caught here and in the shell poll test.
        let q = page_query(true);
        assert!(
            q.contains(&format!("first: {ISSUE_PAGE}, after: $cursor")),
            "the queue's page does not ask for ISSUE_PAGE: {q}"
        );
        assert!(
            !q.contains("{ISSUE_PAGE}"),
            "the page size was not filled in"
        );
    }

    #[test]
    fn the_running_mark_the_page_selects_is_the_mark_the_guard_exempts() {
        // The queue's page names the mark inside a GraphQL string the
        // compiler cannot check, so the exemption and the query could drift
        // apart: the page would go on selecting the closed marks while the
        // guard exempted a name no issue carries, and every running issue
        // would pay a paginated read on every poll with nothing failing.
        assert!(PAGE.contains(&format!("labels: [\"{RUNNING}\"]")));
        assert_eq!(RUNNING, "tehas:running");
    }
}
