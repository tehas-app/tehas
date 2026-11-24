//! What a number names where issues and pull requests share one number
//! space: a ticket or a change, as `Tickets::what_is` and
//! `Tickets::what_are` answer. Several numbers go in one GraphQL question,
//! an alias a number, as the dispatcher's read of a blocker's state asks
//! today.
//!
//! GitHub answers a number it does not know with a null node and a
//! `NOT_FOUND` error at that alias, and gh then exits 1, so the question is
//! read from gh's answer beside its failure, item by item.

use std::collections::BTreeMap;
use std::num::NonZeroU64;

use serde::Deserialize;
use serde_json::Value;

use crate::GitHub;
use tehas_ports::{ChangeNo, Got, Item, Outcome, TicketKey, Unread, ALL_MAX};

const MALFORMED: Outcome = Outcome::Unread(Unread::Malformed);
const INCONSISTENT: Outcome = Outcome::Unread(Unread::Inconsistent);
const CUT: Outcome = Outcome::Unread(Unread::Cut);

/// The most numbers one question asks: more go in another question.
const PER_QUESTION: usize = 100;

/// The alias a number is asked under.
fn alias(n: u64) -> String {
    format!("n{n}")
}

/// The question of what each number names, one alias a number.
fn question(numbers: &[u64]) -> String {
    let mut q = String::from(
        "query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) {",
    );
    for n in numbers {
        q.push_str(&format!(
            " {}: issueOrPullRequest(number: {n}) {{ __typename ... on Issue {{ number }} ... on PullRequest {{ number }} }}",
            alias(*n)
        ));
    }
    q.push_str(" } }");
    q
}

#[derive(Deserialize)]
struct Answer {
    data: Option<Data>,
    #[serde(default)]
    errors: Vec<Error>,
}

#[derive(Deserialize)]
struct Data {
    /// Each alias asked, null where GitHub has no node for it.
    repository: Option<BTreeMap<String, Option<Node>>>,
}

#[derive(Deserialize)]
struct Node {
    #[serde(rename = "__typename")]
    is: String,
    number: Option<u64>,
}

#[derive(Deserialize)]
struct Error {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    path: Vec<Value>,
}

impl Error {
    /// The alias this error is about, None for one about the question.
    fn alias(&self) -> Option<&str> {
        match self.path.as_slice() {
            [Value::String(r), Value::String(a), ..] if r == "repository" => Some(a),
            _ => None,
        }
    }

    /// GitHub's word that the alias names nothing: the alias itself is not
    /// found, not something inside it (Decided 121).
    fn not_found(&self) -> bool {
        self.kind.as_deref() == Some("NOT_FOUND") && self.path.len() == 2
    }
}

impl GitHub {
    /// What each number names, in the order asked, a repeated number
    /// answered each time from one alias. More distinct numbers than
    /// `ALL_MAX` is `Unread(Cut)` for every one, with no question asked.
    pub(crate) fn what_are(&mut self, numbers: &[NonZeroU64]) -> Vec<(NonZeroU64, Got<Item>)> {
        let mut distinct: Vec<u64> = numbers.iter().map(|n| n.get()).collect();
        distinct.sort_unstable();
        distinct.dedup();
        let mut answers: BTreeMap<u64, Got<Item>> = BTreeMap::new();
        if distinct.len() > ALL_MAX.get() as usize {
            answers.extend(distinct.iter().map(|n| (*n, Err(CUT))));
        } else {
            for chunk in distinct.chunks(PER_QUESTION) {
                answers.extend(chunk.iter().copied().zip(self.ask_items(chunk)));
            }
        }
        numbers
            .iter()
            .map(|n| {
                (
                    *n,
                    answers.get(&n.get()).cloned().unwrap_or(Err(INCONSISTENT)),
                )
            })
            .collect()
    }

    /// One question of distinct numbers: an answer for each, in order.
    fn ask_items(&mut self, numbers: &[u64]) -> Vec<Got<Item>> {
        let (owner, name) = self.owner_name();
        let query = format!("query={}", question(numbers));
        let args = ["api", "graphql", "-F", &owner, "-F", &name, "-f", &query];
        let (out, failed) = match self.call_answer("the items' question", &args) {
            Ok(out) => (out, None),
            Err((e, Some(out))) => (out, Some(e)),
            Err((e, None)) => return vec![Err(self.failed(&e, numbers.len())); numbers.len()],
        };
        let answer: Answer = match serde_json::from_str(out.trim()) {
            Ok(a) => a,
            Err(_) => {
                let o = failed.map_or(MALFORMED, |e| self.failed(&e, numbers.len()));
                return vec![Err(o); numbers.len()];
            }
        };
        let repository = answer.data.and_then(|d| d.repository);
        let nodes = match (repository, failed.as_ref()) {
            (Some(nodes), None) => Some(nodes),
            (Some(nodes), Some(_)) if !answer.errors.is_empty() => Some(nodes),
            _ => None,
        };
        let Some(mut nodes) = nodes else {
            // No node, or gh failed with no error to read the nodes by: the
            // call's own outcome.
            let o = failed.map_or(MALFORMED, |e| self.failed(&e, numbers.len()));
            return vec![Err(o); numbers.len()];
        };
        // An error that names no alias leaves no item sure.
        if answer.errors.iter().any(|e| e.alias().is_none()) {
            return vec![Err(INCONSISTENT); numbers.len()];
        }
        numbers
            .iter()
            .map(|n| {
                let a = alias(*n);
                let errors: Vec<&Error> = answer
                    .errors
                    .iter()
                    .filter(|e| e.alias() == Some(&a))
                    .collect();
                match (nodes.remove(&a), errors.as_slice()) {
                    (None, _) => Err(MALFORMED),
                    (Some(None), [e]) if e.not_found() => Err(Outcome::NotFound),
                    (Some(Some(node)), []) => item(*n, &node),
                    _ => Err(INCONSISTENT),
                }
            })
            .collect()
    }

    /// The outcome of a question gh failed with no answer to read. A
    /// number GitHub does not know fails the whole question, so where
    /// several were asked a `NotFound` names none of them.
    fn failed(&self, e: &crate::ForgeError, asked: usize) -> Outcome {
        match self.outcome(e) {
            Outcome::NotFound if asked > 1 => INCONSISTENT,
            o => o,
        }
    }
}

/// What a node names: a ticket for an issue, a change for a pull request,
/// each with the number asked.
fn item(asked: u64, node: &Node) -> Got<Item> {
    if node.number != Some(asked) {
        return Err(INCONSISTENT);
    }
    let n = NonZeroU64::new(asked).ok_or(INCONSISTENT)?;
    match node.is.as_str() {
        "Issue" => Ok(Item::Ticket(TicketKey::Number(n))),
        "PullRequest" => Ok(Item::Change(ChangeNo::from(n))),
        _ => Err(INCONSISTENT),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_number_is_asked_under_its_own_alias() {
        let q = question(&[12, 34]);
        assert!(q.contains(" n12: issueOrPullRequest(number: 12) { __typename "));
        assert!(q.contains(" n34: issueOrPullRequest(number: 34) { __typename "));
        assert!(q.ends_with(" } }"));
    }
}
