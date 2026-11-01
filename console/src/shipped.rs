//! What closed an issue: the test that a merged pull request did, for
//! `console tidy run` and `tehasbot status`. The acceptance rule has one
//! statement here. Two commands act on it: tidy (src/tidy.rs) puts tehas:shipped
//! on in the edit that takes the state labels off, and tehasbot status
//! (src/queue.rs) lists the shipped issues and gives them the label, and the
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
//! Exit: 0, or 2 for usage.

use serde_json::Value;

// How many issues one read of the search names: the page of the query and the
// ceiling of the list a caller builds, in the one number.
pub const PAGE: usize = 100;

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

pub fn main(given: &[String]) -> u8 {
    if given.iter().map(String::as_str).eq(["page"]) {
        println!("{PAGE}");
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
    let list = doc
        .get("data")?
        .as_object()?
        .get("search")?
        .as_object()?
        .get("nodes")?
        .as_array()?;
    if list.iter().any(|n| !speaks(n)) {
        return None;
    }
    Some(list.clone())
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

/// The numbers of the shipped issues that do not carry `label`, for tehasbot
/// status, which gives it to them. None of them when the answer is no answer.
pub fn lacking(answer: &str, controls: &[String], label: &str) -> Vec<u64> {
    let listed = list(answer, controls).unwrap_or_default();
    listed
        .iter()
        .filter(|n| lacks(n, label))
        .filter_map(number)
        .collect()
}

/// The names a read gave under `labels`, or nothing when it named no list of
/// them: no answer, and not the empty list of an issue that carries none.
fn names_of(labels: &Value) -> Option<Vec<String>> {
    let list = labels.as_object()?.get("nodes")?.as_array()?;
    Some(
        list.iter()
            .map(|n| match n.get("name") {
                Some(Value::String(s)) => s.clone(),
                Some(other) => other.to_string(),
                None => "null".to_string(),
            })
            .collect(),
    )
}

/// Whether the node of the read does not carry `label`. A node whose labels the read did not answer lacks the
/// label, as a caller has always edited on that answer, so a poll that could
/// not read them still asks for it.
fn lacks(node: &Value, label: &str) -> bool {
    match names_of(node.get("labels").unwrap_or(&Value::Null)) {
        None => true,
        Some(held) => !held.iter().any(|n| n == label),
    }
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

    /// One shipped node closed at `at` that carries `names` as its labels.
    fn labelled(n: u64, at: &str, names: Value) -> Value {
        let mut node = by(n, json!(true));
        {
            let fields = node.as_object_mut().unwrap();
            fields.insert("closedAt".into(), json!(at));
            fields.insert("labels".into(), json!({ "nodes": names }));
        }
        node
    }

    /// The numbers `lacking` keeps of the nodes, in the order of the list.
    fn lacking(nodes: &[Value], label: &str, controls: &[String]) -> Vec<String> {
        super::lacking(&answer(nodes), controls, label)
            .iter()
            .map(u64::to_string)
            .collect()
    }

    #[test]
    fn the_list_lacking_a_label_leaves_out_the_node_that_carries_it() {
        let holds = labelled(
            20,
            "2026-01-02T00:00:00Z",
            json!([{"name": "tehas:shipped"}, {"name": "bug"}]),
        );
        assert_eq!(
            lacking(std::slice::from_ref(&holds), "tehas:shipped", &[]),
            Vec::<String>::new()
        );
        // A node that carries other labels lacks the one asked for, and one that
        // carries none lacks it too.
        let other = labelled(21, "2026-01-03T00:00:00Z", json!([{"name": "bug"}]));
        assert_eq!(lacking(&[other], "tehas:shipped", &[]), ["21"]);
        let none = labelled(22, "2026-01-04T00:00:00Z", json!([]));
        assert_eq!(lacking(&[none], "tehas:shipped", &[]), ["22"]);
        // Labels the read did not answer are taken for missing, as every caller
        // has edited on that answer.
        let no_labels = json!({"number": 23, "closedAt": "2026-01-05T00:00:00Z",
          "timelineItems": {"nodes": [{"closer": {"merged": true}}]}});
        assert_eq!(lacking(&[no_labels], "tehas:shipped", &[]), ["23"]);
        for shape in [json!([]), json!(null), json!({"nodes": null})] {
            let mut odd = labelled(24, "2026-01-06T00:00:00Z", json!([]));
            odd.as_object_mut()
                .unwrap()
                .insert("labels".into(), shape.clone());
            assert_eq!(
                lacking(&[odd], "tehas:shipped", &[]),
                ["24"],
                "labels of {shape} name no list"
            );
        }
        // The name is text and no pattern, so "*" is the name "*" and the node
        // that carries tehas:shipped lacks it.
        assert_eq!(lacking(&[holds], "*", &[]), ["20"]);
    }

    #[test]
    fn the_list_lacking_a_label_keeps_the_order_and_the_control_rule() {
        let nodes = [
            labelled(
                20,
                "2026-01-02T00:00:00Z",
                json!([{"name": "tehas:shipped"}]),
            ),
            labelled(21, "2026-01-04T00:00:00Z", json!([])),
            labelled(22, "2026-01-03T00:00:00Z", json!([{"name": "bug"}])),
        ];
        // Most recently closed first, of the ones that lack the label.
        assert_eq!(lacking(&nodes, "tehas:shipped", &[]), ["21", "22"]);
        // The control issue is never printed, whether or not it carries it: the
        // two commands leave it out of every list.
        assert_eq!(lacking(&nodes, "tehas:shipped", &["21".into()]), ["22"]);
        assert_eq!(
            lacking(
                &[
                    labelled(1, "2026-01-05T00:00:00Z", json!([])),
                    nodes[1].clone()
                ],
                "tehas:shipped",
                &["1".into()]
            ),
            ["21"]
        );
        // Without the flag nothing changes: every shipped node is listed, the
        // one that carries the label among them.
        assert_eq!(
            numbers(&list(&answer(&nodes), &[]).unwrap()),
            ["21", "22", "20"]
        );
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
    fn a_name_is_text_and_a_list_with_no_names_lacks_every_label() {
        let node =
            json!({"number": 7, "labels": {"nodes": [{"name": "tehas:shipped"}, {"name": "bug"}]}});
        assert!(!lacks(&node, "tehas:shipped"));
        assert!(lacks(&node, "tehas:ready"));
        assert!(lacks(&node, "*"));
        assert!(lacks(&node, "tehas:shipped bug"));
        // A name GitHub gave as a number is still a name of a label.
        assert!(!lacks(&json!({"labels": {"nodes": [{"name": 7}]}}), "7"));
        for shape in [
            json!({"number": 7}),
            json!({"labels": []}),
            json!({"labels": {}}),
            json!({"labels": {"nodes": {"name": "x"}}}),
            json!({"labels": {"nodes": null}}),
            json!([]),
        ] {
            assert!(lacks(&shape, "tehas:shipped"), "{shape} names no list");
        }
    }

    #[test]
    fn the_verb_is_the_page() {
        let given = |w: &[&str]| w.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(main(&given(&["page"])), 0);
        assert_eq!(main(&given(&[])), 2);
        assert_eq!(main(&given(&["page", "x"])), 2);
        assert_eq!(main(&given(&["query"])), 2);
    }
}
