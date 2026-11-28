//! What a snapshot asks of GitHub: the marks on closed issues and pull
//! requests, and the reads the bash makes of one issue at a time, in the
//! shapes the bash dispatcher read them. The queue's own page moved onto the
//! ports (`read::board`, `read::build::queue`); what is left here shapes
//! what the board's answer already holds.

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
/// `console/ops/src/stop.rs` and `console/ops/src/tidy.rs` name it in their commands: a rename has to reach this constant and those sites. The test below ties it to the guard's exemption; the queue's read asks for it through the ports, as `Mark::State(State::Running)`.
pub const RUNNING: &str = "tehas:running";

/// What the refresh rule reads of a pull request and of the issue it closes.
macro_rules! pull_fields {
    () => {
        r#"number url isDraft isCrossRepository headRefOid baseRefName baseRefOid mergeable
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

/// The page's field of an issue's label and title events, and the field of
/// when an issue or an event was made: named once, for the parse here and the
/// mapping of the board's answer (`read::board`).
pub(super) const TIMELINE: &str = "timelineItems";
pub(super) const CREATED: &str = "createdAt";

/// One open issue as the page gave it.
pub struct Open {
    pub number: u64,
    pub labels: Vec<String>,
    pub node: Value,
}

/// The open issues, the repository's name, the closed issues still marked
/// tehas:running and the open pull requests: the queue's shapes, as
/// `read::board` maps the board's answer and `read::build::queue` fills what
/// the board does not answer.
pub struct Queue {
    pub open: Vec<Open>,
    pub closed_running: Vec<u64>,
    /// The open pull requests as nodes the hold of rule 6 reads: None when
    /// the code host was not wired, which reads as nothing known rather
    /// than nothing open.
    pub open_pulls: Option<Value>,
    /// The issue types of the repository's owner, when the queue asked for
    /// them: empty for an owner with none.
    pub issue_types: Option<Vec<String>>,
    /// The repository's `tehas:` labels with their ids, as `crate::pins`
    /// compares them with the ids it pinned, or None.
    pub labels: Option<Value>,
}

fn split(repo: &str) -> (String, String) {
    let (o, n) = repo.split_once('/').unwrap_or((repo, ""));
    (format!("owner={o}"), format!("name={n}"))
}

/// The label events of an issue as the REST events list gives them.
pub fn events(node: &Value) -> Value {
    let list: Vec<Value> = node
        .get(TIMELINE)
        .and_then(|t| t.get("nodes"))
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| {
                    let actor = match e.pointer("/actor/login").and_then(Value::as_str) {
                        Some(l) => json!({"login": l}),
                        None => Value::Null,
                    };
                    let at = e.get(CREATED).cloned().unwrap_or(Value::Null);
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
/// `tehas:lean` and `tehas:classic` only, and the exemption reaches no other label - an issue carrying the mark beside a
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
        .get(TIMELINE)
        .and_then(|t| t.get("nodes"))
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
        .get(TIMELINE)
        .and_then(|t| t.get("nodes"))
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
            CREATED: n.get(CREATED), "author": n.get("author")}),
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

/// The marker of a read whose gh call failed, with the call's line: a rule
/// that ends the poll on the marker says what gh said, and a marker with no
/// line is a read no gh call failed for. The line holds only the words of
/// `gh_why`'s fixed list, so nothing gh printed beyond them reaches the log.
pub fn failed_read(f: &GhFail) -> Value {
    json!({"failed": true, "line": f.line})
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

/// A list gh answers, or {"failed": true}.
pub fn list(calls: &mut Calls, args: &[&str]) -> Value {
    calls.gh(args).unwrap_or_else(|_| json!({"failed": true}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_one_pull_request_asks_the_refresh_rules_fields() {
        let fields = pull_fields!();
        assert!(PULL.contains(&format!("{{ state {fields} }}")));
        let args = [
            "api".to_string(),
            "graphql".to_string(),
            "-F".to_string(),
            "owner=a".to_string(),
            "-f".to_string(),
            format!("query={PULL}"),
        ];
        assert!(crate::read::run::gh_reads(&args));
    }

    #[test]
    fn every_account_a_query_reads_comes_with_its_user_id() {
        // tehas_core::accounts counts a listed login only with its recorded id.
        let with_id = "{ login ...on User { databaseId } ...on Bot { databaseId } }";
        let q = PULL.to_string();
        assert_eq!(
            q.matches("login").count(),
            q.matches(with_id).count(),
            "{q}"
        );
        assert!(q.contains(with_id), "{q}");
    }

    #[test]
    fn an_issue_names_its_epic() {
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

    /// How many open issues block each one through a native blocked-by link,
    /// as the issue's REST-shaped answer carries it.
    /// A forge answer without the field, or with one that is no count, carries
    /// nothing, so no link holds the issue.
    #[test]
    fn an_issue_carries_its_open_blocked_by_links() {
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
    fn the_running_mark_the_guard_exempts_is_the_one_the_table_spells() {
        // The queue's page used to name the mark inside a GraphQL string the
        // compiler could not check; the read now asks it as a typed mark, and
        // the guard's exemption stays tied to the table's word for it.
        assert_eq!(RUNNING, "tehas:running");
    }
}
