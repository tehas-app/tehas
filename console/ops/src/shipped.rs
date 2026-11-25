//! What closed an issue: the test that a merged pull request did, for
//! `console tidy run` and `tehasbot status`. The acceptance rule has one
//! statement here. Two commands act on it: tidy (src/tidy.rs) puts tehas:shipped
//! on in the edit that takes the state labels off, and tehasbot status
//! (console/src/queue.rs) lists the shipped issues and gives them the label, and the
//! two must not disagree.
//!
//!   console shipped page
//!     The page of the query: how many issues one read of the search may name,
//!     and the ceiling of the list a caller builds. The two are one number so
//!     they cannot drift apart. GitHub puts a ceiling on the terms of a search
//!     and the read carries one per issue, so a longer list is not truncated: it
//!     is no answer at all.
//!
//! One answer, one rule: a merged pull request closed the issue, or none did.
//! The closer of the last closed event is the authoritative field - it is what
//! GitHub recorded at the close - and the references are the fallback that
//! speaks only when the closer names no pull request. A merge is a `merged`
//! GitHub says is the boolean true, and nothing else: a string, a number or an
//! object is no answer and is never taken for one, of a closer or of a
//! reference, and neither is a reference the read did not resolve as a pull
//! request. A closer that is a pull request, one whose `merged` the read read,
//! speaks for itself, so `merged` false is no merge even when a reference says
//! otherwise.
//!
//! The rule is one answer for one node. The callers do make two reads of it -
//! tidy builds its search from the plan and `tehasbot status` from its
//! own search, at different instants of the same poll - so a poll can still
//! disagree with a poll, and each caller logs what its own read said. When the
//! two sources disagree for one issue it is because they were read at two
//! instants, not because the rule has two answers.
//!
//! The name of a label reaches this program as one argument and is compared as
//! text, so one of "*" is no pattern and one with a space is no two names.
//!
//! On the ports (T4.10) the rule is `by_merge`, over the closer the tickets
//! port names and the merged changes of this project it lists, and the read
//! is `since`, over `Tickets::closed_since`. The search below and its reader
//! stay for their two callers until each moves: tehasbot status (T4.9, after
//! this) onto `since`, and tidy onto `by_merge` over its read by number.
//!
//! The rule on the ports differs from the search's in one place, on purpose:
//! a change of another project. The search read `merged` of any pull request,
//! the closer's and the references', whatever its repository. The port names
//! a closer of another project as `Closer::ChangeElsewhere`, with no `merged`,
//! and lists only this project's merged changes, so tehas:shipped means a
//! change of this project merged. A ticket closed by a merged pull request of
//! another repository, with none of this project's listed, was shipped and is
//! not; one closed by a pull request of another repository that is not merged,
//! with a merged one of this project listed, was not shipped and is. A test
//! below pins both.
//!
//! Open (before tidy moves): tidy reads by number with
//! `Tickets::several_with_closers`, whose `WithCloser` has the closer but no
//! merged changes listed, so `by_merge(closer, &[])` there drops #947's
//! fallback (a commit closer while a merged change is listed) and tidy and
//! tehasbot status would disagree on it. The fallback needs those changes on
//! `WithCloser`, a field its `#[non_exhaustive]` allows, added by an adapter
//! pull request first (Decided 80), or tidy reads with `closed_since`.
//!
//! Exit: 0, or 2 for usage.

use std::num::NonZeroU32;

use serde_json::Value;
use tehas_ports::{
    At, Bound, ChangeDigest, ChangeState, ClosedTicket, Closer, Got, Mark, State, TicketKey,
    Tickets,
};

// How many issues one read of the search names: the page of the query and the
// ceiling of the list a caller builds, in the one number.
pub const PAGE: usize = 100;

// `PAGE` as the bound of a port's read.
const BOUND: NonZeroU32 = match NonZeroU32::new(PAGE as u32) {
    Some(n) => n,
    None => panic!("PAGE is above 0"),
};

// How many of the pull requests GitHub lists as closing an issue one read
// names: the page of that connection and the most the rule scans, in the one
// number, so the rule cannot read past what the read asks for. An issue closed
// by more than that is rare, and one merge among them is the answer the rule
// needs.
pub const REFERENCES: usize = 20;

// The label of a merge: the name tidy puts on in the edit that takes the
// state labels off, and the name tehasbot status gives from its own read. It is
// named here because both the comparison that earns it and the edit that writes
// it read this one word.
pub const SHIPPED: &str = "tehas:shipped";

/// The mark of a merge, in tehas's words: what `SHIPPED` is on GitHub.
pub const MARK: Mark = Mark::State(State::Shipped);

/// Whether a merged change of this project closed the ticket. A closer that
/// is a change of this project speaks for itself, so one not merged is no
/// merge whatever else is listed. Any other closer, or none, lets the
/// merged changes the site lists as closing it decide.
pub fn by_merge(closer: Option<&Closer>, listed: &[ChangeDigest]) -> bool {
    match closer {
        Some(Closer::Change { merged, .. }) => *merged,
        _ => listed
            .iter()
            .any(|c| matches!(c.state, ChangeState::Merged { .. })),
    }
}

/// The tickets closed at or after `since` that a merged change closed, most
/// recently closed first, every one of `controls` left out. Two closed at
/// the same instant keep the port's order. More closed tickets than `PAGE`
/// since then is the port's `Unread(Cut)`, and any read that failed is its
/// outcome: no answer, never an empty list.
pub fn since(
    tickets: &mut dyn Tickets,
    since: At,
    controls: &[TicketKey],
) -> Got<Vec<ClosedTicket>> {
    let mut kept: Vec<ClosedTicket> = tickets
        .closed_since(since, Bound::from(BOUND))?
        .into_items()
        .into_iter()
        .filter(|t| by_merge(t.closer.as_ref(), &t.merged))
        .filter(|t| !controls.contains(&t.digest.key))
        .collect();
    kept.sort_by_key(|t| std::cmp::Reverse(t.closed));
    Ok(kept)
}

/// The keys of the shipped tickets that do not carry `MARK`, in their order.
pub fn lacking_mark(shipped: &[ClosedTicket]) -> Vec<TicketKey> {
    shipped
        .iter()
        .filter(|t| !t.digest.marks.contains(&MARK))
        .map(|t| t.digest.key.clone())
        .collect()
}

// The document, in the lines the GitHub endpoint reads as one. $q is a GraphQL
// variable and reaches gh as the caller's -f q= argument. The braces are the
// document's own and are pinned against the text by a test below: the page of
// the search and the page of the references are `PAGE` and `REFERENCES` of the
// constants above, each written into it once and pinned by its name there, so
// the bound a read asks for and the bound the rule scans cannot drift apart.
// tehasbot status lists each node it keeps, so the fields the list shows are
// asked here too. The references are read for the one boolean the rule needs,
// and each node names its type, so a reference the read did not resolve as a
// pull request is no merge.
pub const QUERY: &str = "query($q: String!) { search(type: ISSUE, query: $q, first: 100) { nodes { ... on Issue { number title url createdAt updatedAt closedAt labels(first: 100) { nodes { name } } closedByPullRequestsReferences(first: 20) { nodes { __typename merged } } timelineItems(itemTypes: [CLOSED_EVENT], last: 1) { nodes { ... on ClosedEvent { closer { ... on PullRequest { merged } } } } } } } } }";

// The fields of one issue that QUERY asks for, for a read that names its
// issues by number. A test pins that QUERY asks for the same.
const ISSUE: &str = "number title url createdAt updatedAt closedAt labels(first: 100) { nodes { name } } closedByPullRequestsReferences(first: 20) { nodes { __typename merged } } timelineItems(itemTypes: [CLOSED_EVENT], last: 1) { nodes { ... on ClosedEvent { closer { ... on PullRequest { merged } } } } }";

/// The document of a read of the issues `numbers` of the repository that the
/// variables $owner and $name name, each under an alias of its own. A search
/// has no term that picks an issue by its number, so tidy reads them this way.
/// The answer holds the issues under `data.repository`, which `list` reads as
/// it reads the nodes of a search.
pub fn by_number(numbers: &[u64]) -> String {
    let mut doc = String::from(
        "query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) {",
    );
    for n in numbers {
        doc.push_str(&format!(" i{n}: issue(number: {n}) {{ {ISSUE} }}"));
    }
    doc.push_str(" } }");
    doc
}

pub fn main(given: &[String]) -> u8 {
    if given.iter().map(String::as_str).eq(["page"]) {
        tehas_core::outln!("{PAGE}");
        return 0;
    }
    eprintln!("console shipped: usage: page");
    2
}

/// The nodes of a whole answer, or nothing when it is no answer at all: it
/// carries top-level errors, or its nodes are no list, or one of them is no
/// node. gh answers `"errors": []` for a read that went well, so an errors key
/// that holds nothing is no refusal; one that holds anything - a list, a string,
/// an object, a number or true - is. A search that found nothing answers the
/// empty list, which is a whole answer that names no issue.
fn nodes(answer: &str) -> Option<Vec<Value>> {
    let doc = serde_json::from_str::<Value>(answer).ok()?;
    let holds_something = |e: &Value| match e {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(m) => !m.is_empty(),
    };
    if doc.get("errors").is_some_and(holds_something) {
        return None;
    }
    let data = doc.get("data")?.as_object()?;
    // A read by number answers each issue under its alias; a null one is an
    // issue the read did not find, and no answer.
    let list: Vec<Value> = match data.get("search") {
        Some(search) => search.as_object()?.get("nodes")?.as_array()?.clone(),
        None => data
            .get("repository")?
            .as_object()?
            .values()
            .cloned()
            .collect(),
    };
    if list.iter().any(|n| !speaks(n)) {
        return None;
    }
    Some(list)
}

/// The number GitHub gave a node: a whole number of its own, and no number
/// written as text, no null, no bool and no fraction or negative, none of which
/// is the number of an issue. serde_json reads an integer exactly, so a number
/// past what a JavaScript number holds still reads as the number it is rather
/// than rounding into another issue's number.
fn number(node: &Value) -> Option<u64> {
    match node.get("number")? {
        Value::Number(n) if n.is_u64() => n.as_u64().filter(|i| *i > 0),
        Value::Number(n) if n.is_i64() => n
            .as_i64()
            .and_then(|i| u64::try_from(i).ok())
            .filter(|i| *i > 0),
        Value::Number(n) => n
            .as_f64()
            .filter(|d| d.is_finite() && d.fract() == 0.0 && *d > 0.0)
            .and_then(|d| u64::try_from(d as i64).ok()),
        _ => None,
    }
}

/// The node's last closed event, or nothing when the read named no timeline, no
/// event or no closer. Only the boolean true of a pull request's `merged` is a
/// merge; the read asks for the last closed event and so puts it first.
fn last_closer(node: &Value) -> Option<&Value> {
    node.get("timelineItems")?
        .get("nodes")?
        .as_array()?
        .first()?
        .get("closer")
}

/// The pull requests GitHub lists as closing the issue hold a merged one, of
/// the first `within` the read named. A reference the read did not resolve as a
/// pull request - no `__typename` of `PullRequest` - is no merge: the fallback
/// trusts only what the read proved about the node, as the closer must be a
/// pull request to speak at all. `within` is the page of the query, so the scan
/// never reads past what the read asks for.
fn a_reference_merged(node: &Value, within: usize) -> bool {
    node.get("closedByPullRequestsReferences")
        .and_then(|r| r.get("nodes"))
        .and_then(Value::as_array)
        .is_some_and(|a| {
            a.iter()
                .take(within)
                .filter(|r| r.get("__typename").and_then(Value::as_str) == Some("PullRequest"))
                .any(|r| r.get("merged").and_then(Value::as_bool) == Some(true))
        })
}

/// Whether a merged pull request closed the issue. The closer of the last closed
/// event is the authoritative read: a pull request speaks for itself, so
/// `merged` false stays no merge, and `merged` of any other shape is a closer
/// that spoke without being read - no merge, and the references stay silent.
/// Only a closer that names no pull request at all - a commit, a null, a closer
/// the read did not name - lets the references GitHub lists on the issue
/// decide: it sometimes records the close event with a commit as the closer
/// while those references name the merged pull request. A node that names
/// neither is an issue the read spoke of and whose closer it did not see: a
/// closed issue, and so not a merge, which is an answer and not a failure to
/// answer.
fn shipped(node: &Value) -> bool {
    match last_closer(node) {
        Some(closer @ Value::Object(_)) => match closer.get("merged") {
            Some(Value::Bool(merged)) => *merged,
            // A closer that named `merged` gave an answer the read cannot use:
            // a closed issue, not a merge, and no reference overrules it.
            Some(_) => false,
            None => a_reference_merged(node, REFERENCES),
        },
        // A null closer is a close by a person, and no closer at all is one the
        // read did not name: neither speaks about a pull request.
        _ => a_reference_merged(node, REFERENCES),
    }
}

/// Whether the node is one the read spoke of: an object that names the number
/// of an issue. A node that names none, or names one that is no number, is a
/// read that did not answer, and a caller may not be told an issue it cannot
/// read was closed by a person.
fn speaks(node: &Value) -> bool {
    match node.get("number") {
        Some(Value::Number(n)) if n.is_u64() => n.as_u64().is_some_and(|i| i > 0),
        Some(Value::Number(n)) if n.is_i64() => n.as_i64().is_some_and(|i| i > 0),
        Some(Value::Number(n)) => n
            .as_f64()
            .is_some_and(|d| d.is_finite() && d.fract() == 0.0 && d > 0.0),
        _ => false,
    }
}

fn closed_at(node: &Value) -> String {
    node.get("closedAt")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// The nodes the answer names as shipped, most recently closed first, with every
/// control number left out. A node the read gave no number for is no issue and
/// is left out; the caller that lists them shows the rest of what it read.
/// Nothing when the answer is no answer at all.
pub fn list(answer: &str, controls: &[String]) -> Option<Vec<Value>> {
    let mut kept: Vec<(String, String, Value)> = nodes(answer)?
        .into_iter()
        .filter(shipped)
        .filter_map(|n| {
            let at = closed_at(&n);
            let num = number(&n)?.to_string();
            Some((at, num, n))
        })
        .filter(|(_, num, _)| !controls.iter().any(|c| c == num))
        .collect();
    // sort_by is stable, so two issues the read gave the same closed time keep
    // the order it named them in.
    // Most recently closed first, by the text of the UTC instant GitHub gave,
    // which orders as the instant itself; an issue the read gave no closed time
    // for is the oldest of them, as the reader shows it last.
    kept.sort_by(|a, b| b.0.cmp(&a.0));
    Some(kept.into_iter().map(|(_, _, n)| n).collect())
}

/// The numbers `list` keeps, for tidy.
pub fn numbers(answer: &str, controls: &[String]) -> Option<Vec<u64>> {
    Some(list(answer, controls)?.iter().filter_map(number).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The answer of the batched search, holding the nodes the cases name.
    fn answer(nodes: &[Value]) -> String {
        format!(
            "{{\"data\":{{\"search\":{{\"nodes\":[{}]}}}}}}",
            nodes
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    /// One issue node of that answer whose closed event names `event`: the
    /// shape of the timeline is written here once, since the event is the only
    /// part a case chooses. An event is `{closer: <pull request or null>}`.
    fn event(n: u64, event: Value) -> Value {
        json!({"number": n, "timelineItems": {"nodes": [event]}})
    }

    /// One issue node whose closed event names a pull request whose `merged`
    /// GitHub gave as `merged`: the closer of the read is the object that holds
    /// the field, so it is written once here.
    fn by(n: u64, merged: Value) -> Value {
        event(n, json!({"closer": {"merged": merged}}))
    }

    /// One issue node whose closed event names no pull request: the read gave
    /// `closer` itself as null, which is a close by a person.
    fn closed_by_hand(n: u64) -> Value {
        event(n, json!({"closer": null}))
    }

    /// One issue node whose closer is a merged pull request.
    fn merged(n: u64) -> Value {
        by(n, json!(true))
    }

    /// The shape of #947: a last closed event whose closer is `closer`, with the
    /// pull requests GitHub lists as closing the issue beside it. A closer the
    /// read gave no `merged` for is no answer about a pull request, so those
    /// references decide the label. `merged` is what each reference holds, as the
    /// read gives it, and each is named a pull request as the read names one.
    fn refs(n: u64, closer: Value, refs: Vec<Value>) -> Value {
        let mut node = event(n, json!({"closer": closer}));
        node["closedByPullRequestsReferences"] = json!({"nodes": refs});
        node
    }

    /// One reference of a merged pull request, as the read gives it.
    fn merged_ref(merged: Value) -> Value {
        json!({"__typename": "PullRequest", "merged": merged})
    }

    fn number_line(node: &Value) -> String {
        number(node).map(|n| n.to_string()).unwrap_or_default()
    }

    /// The numbers of a list, in its order.
    fn numbers(nodes: &[Value]) -> Vec<String> {
        nodes.iter().map(number_line).collect()
    }

    /// What the answer says of one issue, as `true` when the label is earned.
    /// `nothing` is a read that never named the issue, which is not a read that
    /// named it and found no merge.
    fn verdict(issue: &str, answer: &str) -> (String, bool) {
        let asked = issue.parse::<u64>().unwrap();
        let Some(nodes) = nodes(answer) else {
            return ("nothing".to_string(), false);
        };
        let named: Vec<&Value> = nodes.iter().filter(|n| number(n) == Some(asked)).collect();
        if named.is_empty() {
            return ("nothing".to_string(), false);
        }
        if named.into_iter().any(shipped) {
            ("merged".to_string(), true)
        } else {
            ("closed".to_string(), false)
        }
    }

    /// The word the answer gives for one issue of it.
    fn ask(issue: &str, nodes: &[Value]) -> (String, bool) {
        verdict(issue, &answer(nodes))
    }

    /// The numbers `list` keeps of one node, empty when it earns no label. The
    /// node is taken by reference, so a case can ask the same node of `ask` and
    /// of `listed` without cloning it.
    fn listed(node: &Value) -> Vec<String> {
        list(&answer(std::slice::from_ref(node)), &[])
            .unwrap()
            .iter()
            .map(number_line)
            .collect()
    }

    #[test]
    fn a_merged_closer_is_merged_and_nothing_else_is_closed() {
        for merged in [
            json!(false),
            json!(null),
            json!("true"),
            json!("TRUE"),
            json!("yes"),
            json!(1),
            json!(0),
            json!(-1),
            json!(1.0),
            json!({"state": "merged"}),
            json!([]),
            json!({}),
        ] {
            assert_eq!(
                ask("7", &[by(7, merged.clone())]),
                ("closed".to_string(), false),
                "merged {merged} is no merged pull request"
            );
        }
        // A closed event that names no pull request: the read spoke of the issue
        // and saw no merged closer, which is a closed issue.
        assert_eq!(
            ask("7", &[closed_by_hand(7)]),
            ("closed".to_string(), false)
        );
        assert_eq!(ask("7", &[merged(7)]), ("merged".to_string(), true));
        // A node that names no timeline at all: the read spoke of the issue and
        // did not see its closer, which is a closed issue and not a merge.
        assert_eq!(
            ask("7", &[json!({"number": 7})]),
            ("closed".to_string(), false)
        );
        // The read asks for the last closed event and so puts it first: a later
        // null does not undo the merged closer, and a later closer is the one
        // that speaks.
        let after = json!({"number": 7, "timelineItems": {"nodes": [
            {"closer": {"merged": true}}, {"closer": null}]}});
        assert_eq!(ask("7", &[after]), ("merged".to_string(), true));
        let earlier = json!({"number": 7, "timelineItems": {"nodes": [
            {"closer": null}, {"closer": {"merged": true}}]}});
        assert_eq!(ask("7", &[earlier]), ("closed".to_string(), false));
        // A node whose timeline the read gave as no list at all is a node it
        // spoke of and whose closer it did not see.
        let none = json!({"number": 7, "timelineItems": {"nodes": []}});
        assert_eq!(ask("7", &[none]), ("closed".to_string(), false));
    }

    #[test]
    fn an_answer_that_spoke_of_another_issue_is_nothing() {
        let both = [merged(7), by(8, json!(false))];
        assert_eq!(ask("7", &both), ("merged".to_string(), true));
        assert_eq!(ask("8", &both), ("closed".to_string(), false));
        // The issue asked for is in no node of the answer at all: the read never
        // named it, which is not the same as naming it and finding no merge.
        assert_eq!(ask("9", &both), ("nothing".to_string(), false));
        assert_eq!(ask("9", &[]), ("nothing".to_string(), false));
        assert_eq!(verdict("9", "{}"), ("nothing".to_string(), false));
    }

    #[test]
    fn whole_is_the_answer_test_that_list_makes() {
        // A whole answer is one `list` would have read: a list of nodes and no
        // errors. An empty list is whole - a page of nothing is an answer.
        assert!(nodes(&answer(&[])).is_some());
        assert!(nodes(&answer(&[merged(7)])).is_some());
        // The shapes that are no reply of the read at all, the ones a caller must
        // not take for a poll that shipped nothing.
        for shape in [
            r#"{"data":{"search":{"nodes":null}}}"#,
            r#"{"data":{"search":{"nodes":{}}}}"#,
            r#"{"data":{"search":{"nodes":"7"}}}"#,
            r#"{"data":null}"#,
            r#"{"data":{}}"#,
            "not json",
            "",
            "[]",
            r#"{"errors":[{"message":"declined"}],"data":{"search":{"nodes":[]}}}"#,
        ] {
            assert!(nodes(shape).is_none(), "{shape} is no answer");
        }
        // The answer of a read that went well but found no errors key to hold
        // anything is whole: gh answers an empty errors list for those.
        assert!(nodes(r#"{"errors":[],"data":{"search":{"nodes":[]}}}"#).is_some());
        assert!(nodes(r#"{"errors":null,"data":{"search":{"nodes":[]}}}"#).is_some());
    }

    #[test]
    fn an_answer_that_carries_errors_is_nothing() {
        let merged_node = merged(7);
        for errors in [
            json!([{"message": "declined"}]),
            json!("declined"),
            json!({"m": 1}),
            json!(true),
            json!(1),
        ] {
            let doc =
                json!({"errors": errors, "data": {"search": {"nodes": [merged_node.clone()]}}});
            assert_eq!(
                verdict("7", &doc.to_string()),
                ("nothing".to_string(), false),
                "errors {errors}"
            );
            assert_eq!(list(&doc.to_string(), &[]), None);
        }
        // gh answers an errors key for a read that went well: a null or an empty
        // list holds nothing, so it is no refusal and the data answers.
        for errors in [json!([]), json!(null)] {
            let doc =
                json!({"errors": errors, "data": {"search": {"nodes": [merged_node.clone()]}}});
            assert_eq!(verdict("7", &doc.to_string()), ("merged".to_string(), true));
            assert_eq!(numbers(&list(&doc.to_string(), &[]).unwrap()), ["7"]);
        }
        // Errors and no data at all: still nothing.
        assert_eq!(
            verdict(
                "7",
                &json!({"errors": [{"message": "declined"}]}).to_string()
            ),
            ("nothing".to_string(), false)
        );
    }

    #[test]
    fn an_answer_that_is_no_answer_is_nothing() {
        for shape in [
            r#"{"data":{"search":{"nodes":null}}}"#,
            r#"{"data":{"search":{"nodes":{}}}}"#,
            r#"{"data":{"search":{"nodes":"7"}}}"#,
            r#"{"data":{"search":{"nodes":[7]}}}"#,
            r#"{"data":{"search":{"nodes":[null]}}}"#,
            r#"{"data":{"search":{"nodes":[{"timelineItems":{}}]}}}"#,
            r#"{"data":{"search":{"nodes":[{"number":null}]}}}"#,
            r#"{"data":{"search":{"nodes":[{"number":"7"}]}}}"#,
            r#"{"data":{"search":{"nodes":[{"number":0}]}}}"#,
            r#"{"data":{"search":{"nodes":[{"number":-7}]}}}"#,
            r#"{"data":{"search":{"nodes":[{"number":7.5}]}}}"#,
            r#"{"data":{"search":{}}}"#,
            r#"{"data":null}"#,
            r#"{"data":{}}"#,
            "{}",
            "not json",
            "",
            "[]",
        ] {
            assert_eq!(
                verdict("7", shape),
                ("nothing".to_string(), false),
                "{shape} is no answer"
            );
            assert_eq!(list(shape, &[]), None, "{shape} is no answer");
        }
    }

    #[test]
    fn a_merged_reference_shipps_an_issue_whose_close_event_names_a_commit() {
        // The #947 case: GitHub records the close event with a Commit as the
        // closer and lists the merged pull request in the issue's own references.
        let shipped = refs(
            7,
            json!({"__typename": "Commit"}),
            vec![merged_ref(json!(true))],
        );
        assert_eq!(
            ask("7", std::slice::from_ref(&shipped)),
            ("merged".to_string(), true)
        );
        assert_eq!(listed(&shipped), ["7"]);
        // The same node with only unmerged references is a closed issue.
        let not_shipped = refs(
            7,
            json!({"__typename": "Commit"}),
            vec![merged_ref(json!(false)), merged_ref(json!(false))],
        );
        assert_eq!(
            ask("7", std::slice::from_ref(&not_shipped)),
            ("closed".to_string(), false)
        );
        assert_eq!(listed(&not_shipped), Vec::<String>::new());
    }

    #[test]
    fn a_reference_the_read_did_not_name_as_a_pull_request_is_no_merge() {
        // The read asks for pull requests and asks each to name its type, so a
        // node that does not name one is no proof of a merge, whatever it holds
        // for `merged`.
        for shape in [
            json!({"merged": true}),
            json!({"__typename": "Commit", "merged": true}),
            json!({"__typename": "Issue", "merged": true}),
            json!({"__typename": "pullrequest", "merged": true}),
            json!({"__typename": null, "merged": true}),
            json!("merged"),
            json!([[{"merged": true}]]),
        ] {
            let node = refs(7, json!({"__typename": "Commit"}), vec![shape.clone()]);
            assert_eq!(
                ask("7", std::slice::from_ref(&node)),
                ("closed".to_string(), false),
                "a reference of {shape} is no merge"
            );
            assert_eq!(listed(&node), Vec::<String>::new());
        }
        // A pull request among them still decides when the others are noise.
        let mixed = refs(
            7,
            json!({"__typename": "Commit"}),
            vec![json!({"merged": true}), merged_ref(json!(true))],
        );
        assert_eq!(
            ask("7", std::slice::from_ref(&mixed)),
            ("merged".to_string(), true)
        );
    }

    #[test]
    fn the_rule_scans_no_more_than_the_read_asks_for() {
        // The bound the query writes and the bound the rule scans are the one
        // constant, so an answer that names more references than the read asked
        // for is not read past its page.
        assert!(QUERY.contains(&format!(
            "closedByPullRequestsReferences(first: {REFERENCES})"
        )));
        let past = |merged: Value| {
            let mut list = (0..REFERENCES)
                .map(|_| merged_ref(json!(false)))
                .collect::<Vec<_>>();
            list.push(merged_ref(merged));
            refs(7, json!({"__typename": "Commit"}), list)
        };
        // The one merged reference sits past the page the read answers.
        let beyond = past(json!(true));
        assert_eq!(
            ask("7", std::slice::from_ref(&beyond)),
            ("closed".to_string(), false),
            "a merge past the page of the references is no answer"
        );
        // The same node with the merge inside the page is shipped work.
        let mut within = past(json!(false));
        within["closedByPullRequestsReferences"]["nodes"][REFERENCES - 1]["merged"] = json!(true);
        assert_eq!(
            ask("7", std::slice::from_ref(&within)),
            ("merged".to_string(), true)
        );
    }

    #[test]
    fn an_unmerged_closer_stays_unmerged_whatever_the_references_say() {
        // A closer that is a pull request speaks for itself: GitHub's own answer
        // about the close event outranks a reference that claims a merge.
        let node = refs(
            7,
            json!({"merged": false}),
            vec![merged_ref(json!(true)), merged_ref(json!(true))],
        );
        assert_eq!(
            ask("7", std::slice::from_ref(&node)),
            ("closed".to_string(), false)
        );
        assert_eq!(listed(&node), Vec::<String>::new());
    }

    #[test]
    fn a_closer_that_spoke_without_a_boolean_merged_is_overruled_by_nothing() {
        // A closer that named `merged` spoke: an answer the read cannot use is
        // no merge and the references stay silent, so a corrupt closer field can
        // neither be ignored nor be overruled into a label.
        for merged in [
            json!("false"),
            json!("true"),
            json!(1),
            json!(null),
            json!({"state": "merged"}),
            json!([true]),
            json!([]),
            json!({}),
        ] {
            let closer = json!({"__typename": "PullRequest", "merged": merged});
            let node = refs(7, closer, vec![merged_ref(json!(true))]);
            assert_eq!(
                ask("7", std::slice::from_ref(&node)),
                ("closed".to_string(), false),
                "a closer merged of {merged} is no merge"
            );
            assert_eq!(listed(&node), Vec::<String>::new());
        }
        // A commit closer that carries `merged` is the same: it spoke, and the
        // boolean it gave is the answer, whatever the references say.
        let commit = refs(
            7,
            json!({"__typename": "Commit", "merged": true}),
            vec![merged_ref(json!(false))],
        );
        assert_eq!(
            ask("7", std::slice::from_ref(&commit)),
            ("merged".to_string(), true)
        );
        let closed_commit = refs(
            7,
            json!({"__typename": "Commit", "merged": false}),
            vec![merged_ref(json!(true))],
        );
        assert_eq!(
            ask("7", std::slice::from_ref(&closed_commit)),
            ("closed".to_string(), false)
        );
        // A closer the read gave as no object at all is no closer that spoke, so
        // the references still decide.
        let string = refs(7, json!("merged"), vec![merged_ref(json!(true))]);
        assert_eq!(
            ask("7", std::slice::from_ref(&string)),
            ("merged".to_string(), true)
        );
    }

    #[test]
    fn a_reference_of_merged_that_is_no_boolean_is_no_merge() {
        for merged in [
            json!("true"),
            json!("TRUE"),
            json!("yes"),
            json!(1),
            json!(1.0),
            json!(null),
            json!({"state": "merged"}),
            json!([true]),
            json!([]),
            json!({}),
        ] {
            let node = refs(
                7,
                json!({"__typename": "Commit"}),
                vec![merged_ref(merged.clone())],
            );
            assert_eq!(
                ask("7", std::slice::from_ref(&node)),
                ("closed".to_string(), false),
                "a reference merged of {merged} is no merge"
            );
            assert_eq!(listed(&node), Vec::<String>::new());
        }
        // A node that names no references at all, or none of them a merged one,
        // is the closed issue the read spoke of.
        let none = refs(7, json!({"__typename": "Commit"}), vec![]);
        assert_eq!(ask("7", &[none]), ("closed".to_string(), false));
        // A closer the read gave as null (a close by a person) is no pull request
        // either, so the references still speak: the shape the read gives for a
        // commit-shaped closer it could not resolve.
        let hand = refs(7, Value::Null, vec![merged_ref(json!(true))]);
        assert_eq!(ask("7", &[hand]), ("merged".to_string(), true));
    }

    #[test]
    fn the_list_is_most_recently_closed_first() {
        let mut nodes = [merged(20), merged(21), merged(22)];
        for (n, at) in [
            (20, "2026-01-02T00:00:00Z"),
            (21, "2026-01-04T00:00:00Z"),
            (22, "2026-01-03T00:00:00Z"),
        ] {
            nodes
                .iter_mut()
                .find(|v| v["number"] == n)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("closedAt".into(), json!(at));
        }
        assert_eq!(
            numbers(&list(&answer(&nodes), &[]).unwrap()),
            ["21", "22", "20"]
        );
        // An issue the read gave no closed time sorts after those it did.
        nodes[0].as_object_mut().unwrap().remove("closedAt");
        assert_eq!(
            numbers(&list(&answer(&nodes), &[]).unwrap()),
            ["21", "22", "20"]
        );
        // Only a merged closer is listed: a closed issue is not shipped work.
        let mixed = [merged(30), closed_by_hand(31), by(32, json!("true"))];
        assert_eq!(numbers(&list(&answer(&mixed), &[]).unwrap()), ["30"]);
    }

    #[test]
    fn the_control_issue_is_left_out_of_the_list() {
        let nodes = [merged(1), merged(20)];
        assert_eq!(numbers(&list(&answer(&nodes), &[]).unwrap()), ["1", "20"]);
        assert_eq!(
            numbers(&list(&answer(&nodes), &["1".into()]).unwrap()),
            ["20"]
        );
        assert!(list(&answer(&nodes), &["1".into(), "20".into()])
            .unwrap()
            .is_empty());
    }

    #[test]
    fn the_query_closes_every_brace_it_opens() {
        // The tests stub gh, so a query GitHub refuses would pass them all
        // (v0.14.0). Every brace, bracket and paren the document opens is closed
        // again, and the words the rule reads are pinned below the counts so a
        // rewrite that keeps the shape and loses the field is caught too.
        assert_eq!(QUERY.matches('{').count(), QUERY.matches('}').count());
        assert_eq!(QUERY.matches('(').count(), QUERY.matches(')').count());
        assert_eq!(QUERY.matches('[').count(), QUERY.matches(']').count());
        assert!(
            QUERY.starts_with("query($q: String!) { search(type: ISSUE, query: $q, first: 100) {")
        );
        assert!(QUERY.ends_with("closer { ... on PullRequest { merged } } } } } } } } }"));
        assert!(QUERY
            .contains("closedByPullRequestsReferences(first: 20) { nodes { __typename merged } }"));
    }

    #[test]
    fn a_read_by_number_asks_what_the_search_asks_and_names_each_issue() {
        assert!(QUERY.contains(&format!("... on Issue {{ {ISSUE} }}")));
        let doc = by_number(&[63, 65]);
        assert_eq!(
            doc,
            format!("query($owner: String!, $name: String!) {{ repository(owner: $owner, name: $name) {{ i63: issue(number: 63) {{ {ISSUE} }} i65: issue(number: 65) {{ {ISSUE} }} }} }}")
        );
        assert_eq!(doc.matches('{').count(), doc.matches('}').count());
        assert_eq!(doc.matches('(').count(), doc.matches(')').count());
    }

    #[test]
    fn the_answer_of_a_read_by_number_is_read_as_a_search_is() {
        let closer = |merged: bool| json!({"nodes": [{"closer": {"merged": merged}}]});
        let answer = json!({"data": {"repository": {
            "i63": {"number": 63, "closedAt": "2026-11-03T14:27:20Z", "timelineItems": closer(true)},
            "i65": {"number": 65, "closedAt": "2026-11-03T14:20:00Z", "timelineItems": closer(false)},
            "i1": {"number": 1, "timelineItems": closer(true)}
        }}})
        .to_string();
        assert_eq!(super::numbers(&answer, &["1".to_string()]), Some(vec![63]));
        // An issue the read did not find is no answer for any of them.
        let missing = json!({"data": {"repository": {"i63": null}}}).to_string();
        assert_eq!(super::numbers(&missing, &[]), None);
        assert_eq!(super::numbers(&json!({"data": {}}).to_string(), &[]), None);
    }

    #[test]
    fn the_query_asks_for_the_fields_the_shipped_list_shows() {
        // tehasbot status publishes each node it keeps as an item of `completed`,
        // and a field the query does not ask for is null on every one of them.
        assert!(QUERY.contains("number title url createdAt updatedAt closedAt labels("));
    }

    #[test]
    fn the_page_is_one_number_below_the_ceiling_of_a_search() {
        assert!(
            (1..=256).contains(&PAGE),
            "a search holds no more terms than that"
        );
        assert!(QUERY.contains(&format!("first: {PAGE}")));
        // The page of the references is the one number the rule scans, said once
        // in the document and once in the constant, as the page of the search is.
        assert!(
            (1..=256).contains(&REFERENCES),
            "the connection holds no more nodes than that"
        );
        assert!(QUERY.contains(&format!(
            "closedByPullRequestsReferences(first: {REFERENCES})"
        )));
    }

    #[test]
    fn the_verb_is_the_page() {
        let given = |w: &[&str]| w.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(main(&given(&["page"])), 0);
        assert_eq!(main(&given(&[])), 2);
        assert_eq!(main(&given(&["page", "x"])), 2);
        assert_eq!(main(&given(&["query"])), 2);
    }

    mod ports {
        //! The rule and the read on the ports, over the ports' fake: a
        //! ticket a merged change closed is shipped, by its closer first and
        //! by the merged changes listed as closing it when the closer names
        //! no change of this project.

        use super::super::*;
        use super::{merged_ref, refs};
        use serde_json::json;
        use tehas_ports::fake::{FakeSite, Fault, Faulty, Op, TicketsOp, World};
        use tehas_ports::{
            Actor, Caller, ChangeNo, CloseReason, CommitId, ItemName, Outcome, ProjectName, Site,
            Unread, WebLink,
        };

        fn merged_state(w: &World) -> ChangeState {
            ChangeState::Merged {
                at: w.now(),
                by: None,
                commit: None,
            }
        }

        /// Ticket `n` closed by hand now, and change `n + 1` merged that
        /// closes it.
        fn shipped_at(w: World, n: u64) -> World {
            let state = merged_state(&w);
            w.ticket(n, |t| t.closed(CloseReason::Done))
                .change(n + 1, &format!("work/{n}"), |c| c.closes(n).state(state))
        }

        fn read(site: &impl Site, since: At, controls: &[TicketKey]) -> Got<Vec<ClosedTicket>> {
            let mut t = site
                .tickets(&Actor::Operator, Caller::new("shipped"))
                .unwrap();
            super::super::since(&mut *t, since, controls)
        }

        fn keys(got: &[ClosedTicket]) -> Vec<TicketKey> {
            got.iter().map(|t| t.digest.key.clone()).collect()
        }

        /// The merged changes the fake lists as closing a shipped ticket,
        /// to give the rule as a site gives them.
        fn listed() -> Vec<ChangeDigest> {
            let w = shipped_at(World::new(), 7);
            let got = read(&FakeSite::new(&w), At::new(0), &[]).unwrap();
            assert_eq!(got.len(), 1);
            assert!(!got[0].merged.is_empty());
            got[0].merged.clone()
        }

        #[test]
        fn a_change_closer_speaks_for_itself_and_any_other_lets_the_list_decide() {
            let listed = listed();
            let no = ChangeNo::new(8).unwrap();
            let merged = Closer::Change { no, merged: true };
            let unmerged = Closer::Change { no, merged: false };
            assert!(by_merge(Some(&merged), &[]));
            assert!(!by_merge(Some(&unmerged), &listed));
            assert!(!by_merge(Some(&unmerged), &[]));
            let commit = Closer::Commit(CommitId::new("abc1234").unwrap());
            let elsewhere = Closer::change_elsewhere(
                ProjectName::new("acme/other"),
                ItemName::new("3"),
                WebLink::new("https://example.test/acme/other/pull/3").unwrap(),
            );
            for closer in [
                None,
                Some(&commit),
                Some(&elsewhere),
                Some(&Closer::Unknown),
            ] {
                assert!(by_merge(closer, &listed), "{closer:?} with a merge listed");
                assert!(!by_merge(closer, &[]), "{closer:?} with none listed");
            }
            // A change listed that is not merged is no merge.
            let mut open = listed.clone();
            open[0].state = ChangeState::Open;
            assert!(!by_merge(None, &open));
            let mut closed = listed;
            closed[0].state = ChangeState::Closed { at: At::new(0) };
            assert!(!by_merge(None, &closed));
        }

        /// The two statements of the rule answer alike while both stand: the
        /// search's reader of GitHub's nodes and `by_merge` of the port, for
        /// every closer but a change of another project (the next test).
        #[test]
        fn the_rule_on_the_ports_answers_as_the_rule_of_the_search() {
            let listed = listed();
            let no = ChangeNo::new(8).unwrap();
            let commit = Closer::Commit(CommitId::new("abc1234").unwrap());
            let cases: [(Value, Option<Closer>, bool); 6] = [
                (
                    json!({"merged": true}),
                    Some(Closer::Change { no, merged: true }),
                    false,
                ),
                (
                    json!({"merged": false}),
                    Some(Closer::Change { no, merged: false }),
                    true,
                ),
                (
                    json!({"merged": false}),
                    Some(Closer::Change { no, merged: false }),
                    false,
                ),
                (json!({"__typename": "Commit"}), Some(commit.clone()), true),
                (json!({"__typename": "Commit"}), Some(commit), false),
                (Value::Null, None, true),
            ];
            for (closer, port, a_merge_listed) in cases {
                let found = if a_merge_listed {
                    vec![merged_ref(json!(true))]
                } else {
                    vec![]
                };
                let node = refs(7, closer.clone(), found);
                let on_ports = by_merge(port.as_ref(), if a_merge_listed { &listed } else { &[] });
                assert_eq!(
                    shipped(&node),
                    on_ports,
                    "{closer} with a merge listed {a_merge_listed}"
                );
            }
        }

        /// The one difference, on purpose: a change of another project. The
        /// search reads its `merged` and the port has none, so the merged
        /// changes of this project listed decide, and one of another project
        /// is never listed.
        #[test]
        fn a_change_of_another_project_differs_from_the_search_on_purpose() {
            let listed = listed();
            let elsewhere = Closer::change_elsewhere(
                ProjectName::new("acme/other"),
                ItemName::new("3"),
                WebLink::new("https://example.test/acme/other/pull/3").unwrap(),
            );
            // Merged there, none of this project listed: the search said
            // shipped, the port says not.
            let node = refs(7, json!({"merged": true}), vec![]);
            assert!(shipped(&node));
            assert!(!by_merge(Some(&elsewhere), &[]));
            // Not merged there, a merge of this project listed: the search
            // said not, the port says shipped.
            let node = refs(7, json!({"merged": false}), vec![merged_ref(json!(true))]);
            assert!(!shipped(&node));
            assert!(by_merge(Some(&elsewhere), &listed));
            // A commit closer with only a merged reference of another
            // project: the search took the reference, the port lists none.
            let node = refs(
                7,
                json!({"__typename": "Commit"}),
                vec![merged_ref(json!(true))],
            );
            assert!(shipped(&node));
            let commit = Closer::Commit(CommitId::new("abc1234").unwrap());
            assert!(!by_merge(Some(&commit), &[]));
        }

        #[test]
        fn the_read_keeps_the_shipped_most_recently_closed_first_without_the_controls() {
            let w = shipped_at(World::new(), 1);
            let since = w.now();
            let w = shipped_at(w, 7);
            w.advance(10);
            let w = w.ticket(9, |t| t.closed(CloseReason::Done));
            w.advance(10);
            let w = shipped_at(w, 11);
            let site = FakeSite::new(&w);
            let control = w.key(1);
            assert_eq!(
                keys(&read(&site, since, std::slice::from_ref(&control)).unwrap()),
                vec![w.key(11), w.key(7)]
            );
            assert_eq!(
                keys(&read(&site, since, &[]).unwrap()),
                vec![w.key(11), w.key(1), w.key(7)]
            );
        }

        #[test]
        fn a_ticket_closed_before_the_time_asked_is_left_out() {
            let w = shipped_at(World::new(), 7);
            w.advance(10);
            let since = w.now();
            let w = shipped_at(w, 11);
            assert_eq!(
                keys(&read(&FakeSite::new(&w), since, &[]).unwrap()),
                vec![w.key(11)]
            );
        }

        #[test]
        fn a_read_that_failed_is_its_outcome_and_never_an_empty_list() {
            let w = shipped_at(World::new(), 7);
            let site = Faulty::new(FakeSite::new(&w))
                .on(Op::Tickets(TicketsOp::ClosedSince), Fault::Unreachable);
            let got = read(&site, At::new(0), &[]);
            assert!(matches!(got, Err(Outcome::Unavailable { .. })), "{got:?}");
        }

        #[test]
        fn more_closed_than_a_page_is_cut_and_not_a_shorter_list() {
            let mut w = World::new();
            for n in 1..=PAGE as u64 + 1 {
                w = w.ticket(n, |t| t.closed(CloseReason::Done));
            }
            assert_eq!(
                read(&FakeSite::new(&w), At::new(0), &[]),
                Err(Outcome::Unread(Unread::Cut))
            );
        }

        #[test]
        fn the_tickets_lacking_the_mark_are_the_shipped_without_it() {
            let state = merged_state(&World::new());
            let w = World::new()
                .ticket(7, |t| t.mark(MARK).closed(CloseReason::Done))
                .change(8, "work/7", |c| c.closes(7).state(state));
            let w = shipped_at(w, 11);
            let got = read(&FakeSite::new(&w), At::new(0), &[]).unwrap();
            assert_eq!(keys(&got).len(), 2);
            assert_eq!(lacking_mark(&got), vec![w.key(11)]);
            assert_eq!(lacking_mark(&[]), Vec::<TicketKey>::new());
        }
    }
}
