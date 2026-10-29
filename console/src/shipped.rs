//! What closed an issue: the test that a merged pull request did, for
//! lib/shipped.sh. The GraphQL read stays bash - `console shipped query` prints
//! the document and the caller makes the call with gh - and the answer is
//! handed here, so the acceptance rule has one statement. Two commands act on
//! it: tehas tidy (libexec/tidy) puts tehas:shipped on in the edit that takes
//! the state labels off, and tehasbot status (libexec/status) lists the shipped
//! issues and gives them the label, and the two must not disagree.
//!
//!   console shipped page
//!     The page of the query: how many issues one read of the search may name,
//!     and the ceiling of the list a caller builds. The two are one number so
//!     they cannot drift apart. GitHub puts a ceiling on the terms of a search
//!     and the read carries one per issue, so a longer list is not truncated: it
//!     is no answer at all.
//!
//!   console shipped query
//!     The query: the last closed event of each issue the search $q names,
//!     whether its closer was a merged pull request, and the names of the labels
//!     the issue holds. $q must name every issue asked for, and no more than
//!     `page` of them.
//!
//!   console shipped answer <issue> <answer>
//!     Prints what the answer says of <issue> and answers for it:
//!       merged   a whole answer named the issue and its closer was a merged
//!                pull request - the label is earned;
//!       closed   it named the issue and no merged closer - a closed issue, not
//!                a read that failed, so nothing is added and the state labels
//!                still go;
//!       nothing  it did not answer, or answered and said nothing of that issue,
//!                which is no answer and not a negative one;
//!       invalid  <issue> is not digits and nothing else, so the question was
//!                not asked properly.
//!     The exit is 0 for the first two, the ones a read gave, and 1 for the
//!     others, so a caller may branch on either. A response that carries
//!     top-level errors is never a whole answer, even when it holds data.
//!
//!   console shipped list [--control <number>]... [--numbers] <answer
//!     The nodes of the answer whose closer was a merged pull request, one JSON
//!     object on a line, most recently closed first, which is the order tehasbot
//!     status lists them in. Each <number> given with --control is not work and is
//!     left out, as both commands leave the control issue out of every list and
//!     every count. `--numbers` prints the number of each node alone, which is
//!     all tehas tidy asks and the cheaper line to read. The answer is read from
//!     standard input, so one read of a whole poll is asked once and not once per
//!     issue. Nothing is printed and the exit is 1 when the answer is no answer at
//!     all: it carries errors, or its nodes are no list. A read that named issues
//!     and found no merged closer among them answers the empty list, which is a
//!     whole answer that earns no label: 0 and no lines, the case tehas tidy logs
//!     as a closed issue rather than as a failed read.
//!
//!   console shipped has <label> <node>
//!     Whether the node of the query carries <label>: it prints `yes`, `no` or
//!     `unreadable` and exits 0, 1 or 2 in that order. A caller edits on the 2 as
//!     on the 1, and never guesses that an issue it cannot read lacks the label.
//!
//!   console shipped whole <answer
//!     Whether the answer is a reply of the read at all: 0 when it names a list of
//!     nodes and 1 when it carries errors, names no list or is no JSON. Nothing is
//!     printed. A caller that took the list above asks this to tell a page of
//!     nothing from a failed read - the first is an answer and the second is not.
//!     It is `list`'s own test of the answer, so a caller reads the rule once and
//!     not twice.
//!
//! A closer is a merge only when GitHub says `merged` is the boolean true: a
//! string, a number or an object is no answer and is never taken for one. The
//! name of a label reaches this program as one argument and is compared as text,
//! so one of "*" is no pattern and one with a space is no two names.
//!
//! Exit: 0 an answer, and what was asked of it printed; 1 no answer; 2 usage.

use serde_json::Value;
use std::io::Read;

// How many issues one read of the search names: the page of the query and the
// ceiling of the list a caller builds, in the one number.
pub const PAGE: usize = 100;

// The document, in the lines the GitHub endpoint reads as one. $q is a GraphQL
// variable and reaches gh as the caller's -f q= argument. The braces are the
// document's own and are pinned against the text by a test below: the page of
// the search is the constant above, written into it once. tehasbot status
// lists each node it keeps, so the fields the list shows are asked here too.
const QUERY: &str = concat!(
    "query($q: String!) { search(type: ISSUE, query: $q, first: 100) { nodes { ... on Issue {\n",
    "    number title url createdAt updatedAt closedAt labels(first: 100) { nodes { name } } timelineItems(itemTypes: [CLOSED_EVENT], last: 1) { nodes { ... on ClosedEvent {\n",
    "      closer { ... on PullRequest { merged } } } } } } } } }"
);

pub fn main(given: &[String]) -> u8 {
    let Some(verb) = given.first().map(String::as_str) else {
        return usage();
    };
    match (verb, &given[1..]) {
        ("page", []) => {
            println!("{PAGE}");
            0
        }
        ("query", []) => {
            println!("{QUERY}");
            0
        }
        ("answer", [issue, answer]) => {
            // A read that answered, however it answered, exits 0 and prints how:
            // `merged` earns the label and `closed` does not, and both are reads
            // that spoke. `nothing` and `invalid` are no read at all, which is the
            // 1 a caller logs as a failed read rather than as a closed issue.
            match verdict(issue, answer) {
                (said, _) if said == "merged" || said == "closed" => {
                    println!("{said}");
                    0
                }
                (said, _) => {
                    println!("{said}");
                    1
                }
            }
        }
        ("list", rest) => {
            let mut controls = Vec::new();
            let mut numbers_only = false;
            let mut rest = rest.iter();
            while let Some(flag) = rest.next() {
                match flag.as_str() {
                    "--numbers" => numbers_only = true,
                    "--control" => match rest.next() {
                        Some(n) if digits(n) => controls.push(n.clone()),
                        _ => return usage(),
                    },
                    _ => return usage(),
                }
            }
            if rest.next().is_some() {
                return usage();
            }
            let mut answer = String::new();
            if std::io::stdin().read_to_string(&mut answer).is_err() {
                eprintln!("console shipped: the answer could not be read");
                return 1;
            }
            match list(&answer, &controls) {
                Some(nodes) if !nodes.is_empty() => {
                    for node in nodes {
                        println!(
                            "{}",
                            if numbers_only {
                                number_line(&node)
                            } else {
                                node.to_string()
                            }
                        );
                    }
                    0
                }
                // A read that named no shipped issue is the same failure to
                // answer as one that named none at all: tehas tidy has no label
                // to earn and tehasbot status writes no snapshot either way.
                _ => 1,
            }
        }
        ("has", [label, node]) => {
            let (said, rc) = has(label, node);
            println!("{said}");
            rc
        }
        ("whole", []) => {
            let mut answer = String::new();
            if std::io::stdin().read_to_string(&mut answer).is_err() {
                eprintln!("console shipped: the answer could not be read");
                return 1;
            }
            u8::from(nodes(&answer).is_none())
        }
        _ => usage(),
    }
}

fn usage() -> u8 {
    eprintln!(
        "console shipped: usage: page | query | answer <issue> <answer> \
         | list [--control <number>]... | has <label> <node> | whole"
    );
    2
}

/// Digits and nothing else: the number of an issue as GitHub named it. The test
/// is done byte by byte and not with a pattern, so there is one reading of what
/// a digit is, and the number reaches the comparison as text: `--argjson` would
/// take any JSON number for the number of an issue, so "42.0" or "1e2" would be
/// accepted. An empty argument is no number.
fn digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
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

/// The node's last closed event was a merged pull request. A node that names no
/// timeline, no event or no closer is an issue the read spoke of and whose
/// closer it did not see: a closed issue, and so not a merge, which is an answer
/// and not a failure to answer. Only the boolean true of a pull request's
/// `merged` is a merge; the read asks for the last closed event and so puts it
/// first.
fn shipped(node: &Value) -> bool {
    node.get("timelineItems")
        .and_then(|t| t.get("nodes").and_then(Value::as_array))
        .and_then(|a| a.first())
        .and_then(|c| c.get("closer"))
        .and_then(|c| c.get("merged"))
        .and_then(Value::as_bool)
        == Some(true)
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

/// What the answer says of one issue, as one word, and whether the label is
/// earned by it. `merged` and `closed` are reads that answered; `nothing` and
/// `invalid` are not, and a caller adds nothing on either and says so in its
/// log - the two apart, so a factory that shipped nothing does not read like
/// one whose search could not be read.
pub fn verdict(issue: &str, answer: &str) -> (String, bool) {
    // The question first, whatever the answer says. The number is digits and
    // nothing else - " 42", "42.0", "1e2" and "42 43" are asked of no issue -
    // and a leading zero is nothing to a number, so a padded form is no question
    // about an issue either. 0 is no issue's number, and the digits of a number
    // the read cannot hold as one number name no issue the answer can speak of.
    if !digits(issue) || issue.starts_with('0') {
        return ("invalid".to_string(), false);
    }
    // The largest number a read answers as one number: past it two spellings
    // name the same issue, which no caller may be allowed to ask about.
    let Ok(asked) = issue.parse::<u64>() else {
        return ("invalid".to_string(), false);
    };
    if asked == 0 || asked > 9_007_199_254_740_992 {
        return ("invalid".to_string(), false);
    }
    let Some(nodes) = nodes(answer) else {
        return ("nothing".to_string(), false);
    };
    let named = nodes
        .iter()
        .filter(|n| number(n) == Some(asked))
        .collect::<Vec<_>>();
    if named.is_empty() {
        // A read that never named this issue is no answer, not a negative one:
        // the caller cannot be told the issue was closed by a person when the
        // read in fact said nothing of it.
        return ("nothing".to_string(), false);
    }
    let merged = named.into_iter().any(shipped);
    (if merged { "merged" } else { "closed" }.to_string(), merged)
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

/// The number of a node on a line of its own, for the caller that asks with
/// `--numbers`. A node the caller reached through `list` has one.
fn number_line(node: &Value) -> String {
    number(node).map(|n| n.to_string()).unwrap_or_default()
}

/// The names the node holds, or nothing when the read named no list of them:
/// no answer, and not the empty list of an issue that carries none.
fn names(node: &str) -> Option<Vec<String>> {
    let doc = serde_json::from_str::<Value>(node).ok()?;
    let list = doc.get("labels")?.as_object()?.get("nodes")?.as_array()?;
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

fn has(label: &str, node: &str) -> (&'static str, u8) {
    match names(node) {
        None => ("unreadable", 2),
        Some(names) if names.iter().any(|n| n == label) => ("yes", 0),
        Some(_) => ("no", 1),
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

    /// The numbers of a list, in its order, as `--numbers` prints them.
    fn numbers(nodes: &[Value]) -> Vec<String> {
        nodes.iter().map(number_line).collect()
    }

    /// The word the answer gives for one issue of it.
    fn ask(issue: &str, nodes: &[Value]) -> (String, bool) {
        verdict(issue, &answer(nodes))
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
    fn an_issue_number_is_digits_and_nothing_else() {
        let one = answer(&[merged(42)]);
        for form in [
            "",
            " 42",
            "42 ",
            "42.0",
            "4.2e1",
            "1e2",
            "-42",
            "+42",
            "42x",
            "0x2a",
            "42;",
            "42 43",
            "18446744073709551616",
            "9007199254740993",
            "42 42",
            "4_2",
        ] {
            assert_eq!(
                verdict(form, &one),
                ("invalid".to_string(), false),
                "\"{form}\" is no number"
            );
        }
        // A leading zero is nothing to a number, so a padded form is no
        // question about an issue and asks nothing of the answer.
        for form in ["0", "042", "000042"] {
            assert_eq!(
                verdict(form, &one),
                ("invalid".to_string(), false),
                "\"{form}\" is no number"
            );
        }
        // The plain form answers, so the gate above is a gate and not a refusal.
        assert_eq!(verdict("42", &one), ("merged".to_string(), true));
    }

    #[test]
    fn the_verdict_of_a_question_is_said_and_its_exit_is_the_read() {
        // What the caller branches on: a read that answered exits 0 and says how
        // it answered, and one that did not exits 1 with the word that says why.
        // main takes the verb and the words after it, as main.rs hands them
        // over by slicing its own argv.
        let merged_one = answer(&[merged(7)]);
        let closed_one = answer(&[closed_by_hand(7)]);
        let none = answer(&[]);
        let carries = json!({"labels": {"nodes": [{"name": "x"}]}}).to_string();
        let carries_none = json!({"labels": {"nodes": []}}).to_string();
        // One helper of its own for the words main is handed, since a case's
        // list of them is written as text and main takes the strings of argv.
        fn words(given: &[&str]) -> Vec<String> {
            given.iter().map(|s| (*s).to_string()).collect()
        }
        let cases: [(Vec<String>, u8); 11] = [
            (words(&["answer", "7", &merged_one]), 0),
            (words(&["answer", "7", &closed_one]), 0),
            (words(&["answer", "7", &none]), 1),
            (words(&["answer", "x", &merged_one]), 1),
            (words(&["answer", "7"]), 2),
            (words(&["page"]), 0),
            (words(&["query"]), 0),
            (words(&["has", "x", &carries]), 0),
            (words(&["has", "x", &carries_none]), 1),
            (words(&["has", "x", "{}"]), 2),
            (words(&["nope"]), 2),
        ];
        for (given, rc) in cases {
            assert_eq!(main(&given), rc, "{given:?}");
        }
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
    fn a_control_number_of_no_digits_is_a_usage_error() {
        for bad in ["", " 1", "1.0", "-1", "1e2", "1x", "0x1"] {
            let given: Vec<String> = ["list", "--control", bad]
                .iter()
                .map(|s| s.to_string())
                .collect();
            assert_eq!(main(&given), 2, "--control \"{bad}\" is no number");
        }
        let trailing: Vec<String> = ["list", "--control", "1", "extra"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            main(&trailing),
            2,
            "an argument after the controls is no use"
        );
    }

    #[test]
    fn a_label_of_the_node_is_carried_and_another_is_not() {
        let node =
            json!({"number": 7, "labels": {"nodes": [{"name": "tehas:shipped"}, {"name": "bug"}]}})
                .to_string();
        assert_eq!(has("tehas:shipped", &node), ("yes", 0));
        assert_eq!(has("tehas:ready", &node), ("no", 1));
        // The name is text and no pattern: "*" is the name "*" and nothing else,
        // and a name with a space is no two names.
        assert_eq!(has("*", &node), ("no", 1));
        assert_eq!(has("tehas:shipped bug", &node), ("no", 1));
        // A name GitHub gave as a number is still a name of a label.
        let odd = json!({"labels": {"nodes": [{"name": 7}]}}).to_string();
        assert_eq!(has("7", &odd), ("yes", 0));
    }

    #[test]
    fn a_node_that_names_no_labels_is_no_answer() {
        let shapes = [
            json!({"number": 7}),
            json!({"labels": []}),
            json!({"labels": {}}),
            json!({"labels": {"nodes": {"name": "x"}}}),
            json!({"labels": {"nodes": null}}),
            json!([]),
        ];
        for shape in shapes
            .iter()
            .map(Value::to_string)
            .chain(["not json".to_string(), String::new()])
        {
            assert_eq!(
                has("tehas:shipped", &shape),
                ("unreadable", 2),
                "{shape} names no list"
            );
        }
        // The empty list of an issue that carries none is an answer, not a 2.
        let empty = json!({"labels": {"nodes": []}}).to_string();
        assert_eq!(has("tehas:shipped", &empty), ("no", 1));
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
    }

    #[test]
    fn the_query_asks_for_the_fields_the_shipped_list_shows() {
        // libexec/status publishes each node it keeps as an item of `completed`,
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
    }
}
