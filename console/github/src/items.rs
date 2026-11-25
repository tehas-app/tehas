//! What a number names where issues and pull requests share one number
//! space: a ticket or a change, as `Tickets::what_is` and
//! `Tickets::what_are` answer. Several numbers go in one GraphQL question,
//! an alias a number, as the dispatcher's read of a blocker's state asks
//! today.
//!
//! GitHub answers a number it does not know with a null node and a
//! `NOT_FOUND` error at that alias, and gh then exits 1, so the question is
//! read from gh's answer beside its failure, item by item. That reading,
//! `ask_aliased`, is shared with every other aliased question of the
//! adapter (`port/closed.rs`, `port/code_host/numbers.rs`).

use std::collections::BTreeMap;
use std::num::NonZeroU64;

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;

use crate::graphql::{Error, Step};
use crate::GitHub;
use tehas_ports::{ChangeNo, Got, Item, Outcome, TicketKey, Unread, ALL_MAX};

const MALFORMED: Outcome = Outcome::Unread(Unread::Malformed);
const INCONSISTENT: Outcome = Outcome::Unread(Unread::Inconsistent);
const CUT: Outcome = Outcome::Unread(Unread::Cut);

/// The most numbers one question asks: more go in another question.
const PER_QUESTION: usize = 100;

/// The alias a number is asked under.
pub(crate) fn alias(n: u64) -> String {
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
    /// Each alias asked, null where GitHub has no node for it, and any
    /// other field asked of the repository.
    repository: Option<BTreeMap<String, Value>>,
    /// The account asking, when the question asks it.
    #[serde(default)]
    viewer: Option<Viewer>,
}

#[derive(Deserialize)]
struct Viewer {
    #[serde(rename = "databaseId")]
    database_id: Option<u64>,
}

/// What an aliased question answered: each alias's node, as
/// `GitHub::ask_aliased` reads it, and beside them the repository's id and
/// the account asking's, where the question asked them and GitHub gave
/// them.
pub(crate) struct Aliased<T> {
    pub(crate) nodes: Vec<Got<T>>,
    pub(crate) repository_id: Option<u64>,
    pub(crate) viewer_id: Option<u64>,
}

#[derive(Deserialize)]
struct Node {
    #[serde(rename = "__typename")]
    is: String,
    number: Option<u64>,
}

/// The alias an error is about, None for one about the question.
fn alias_of(e: &Error) -> Option<&str> {
    match e.path.as_slice() {
        [Step::Name(r), Step::Name(a), ..] if r == "repository" => Some(a),
        _ => None,
    }
}

/// GitHub's word that the alias names nothing: the alias itself is not
/// found, not something inside it (Decided 121).
fn not_found(e: &Error) -> bool {
    e.is("NOT_FOUND") && e.path.len() == 2
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
        let aliases: Vec<String> = numbers.iter().map(|n| alias(*n)).collect();
        let nodes: Vec<Got<Node>> =
            self.ask_aliased("the items' question", &question(numbers), &aliases);
        numbers
            .iter()
            .zip(nodes)
            .map(|(n, node)| node.and_then(|node| item(*n, &node)))
            .collect()
    }

    /// One aliased question of the repository, read beside gh's failure
    /// alias by alias: each alias's node in the type asked, its accounts
    /// vouched. A null node with exactly one `NOT_FOUND` at its alias is
    /// `NotFound` (Decided 121); any other error at an alias is
    /// `Unread(Inconsistent)` for it, and an error that names no alias for
    /// every one. A node not of the type asked, or an alias missing, is
    /// `Unread(Malformed)`. gh's failure with nothing to read is its
    /// outcome for every alias.
    pub(crate) fn ask_aliased<T: DeserializeOwned>(
        &mut self,
        call: &str,
        question: &str,
        aliases: &[String],
    ) -> Vec<Got<T>> {
        self.ask_aliased_beside(call, question, aliases).nodes
    }

    /// [`GitHub::ask_aliased`], with the repository's `databaseId` and the
    /// `viewer`'s where the question asks them: None where it fails whole.
    pub(crate) fn ask_aliased_beside<T: DeserializeOwned>(
        &mut self,
        call: &str,
        question: &str,
        aliases: &[String],
    ) -> Aliased<T> {
        let all = |o: Outcome| Aliased {
            nodes: aliases.iter().map(|_| Err(o.clone())).collect(),
            repository_id: None,
            viewer_id: None,
        };
        let (owner, name) = self.owner_name();
        let query = format!("query={question}");
        let args = ["api", "graphql", "-F", &owner, "-F", &name, "-f", &query];
        let (out, failed) = match self.call_answer(call, &args) {
            Ok(out) => (out, None),
            Err((e, Some(out))) => (out, Some(e)),
            Err((e, None)) => return all(self.failed(&e, aliases.len())),
        };
        let read = serde_json::from_str::<Value>(out.trim())
            .ok()
            .and_then(|mut v| {
                tehas_core::accounts::vouch(&mut v);
                serde_json::from_value::<Answer>(v).ok()
            });
        let Some(answer) = read else {
            return all(failed.map_or(MALFORMED, |e| self.failed(&e, aliases.len())));
        };
        let (repository, viewer_id) = match answer.data {
            Some(d) => (d.repository, d.viewer.and_then(|v| v.database_id)),
            None => (None, None),
        };
        let nodes = match (repository, failed.as_ref()) {
            (Some(nodes), None) => Some(nodes),
            (Some(nodes), Some(_)) if !answer.errors.is_empty() => Some(nodes),
            _ => None,
        };
        let Some(mut nodes) = nodes else {
            // No node, or gh failed with no error to read the nodes by: the
            // call's own outcome.
            return all(failed.map_or(MALFORMED, |e| self.failed(&e, aliases.len())));
        };
        // An error that names no alias leaves no item sure.
        if answer.errors.iter().any(|e| alias_of(e).is_none()) {
            return all(INCONSISTENT);
        }
        let answered = aliases
            .iter()
            .map(|a| {
                let errors: Vec<&Error> = answer
                    .errors
                    .iter()
                    .filter(|e| alias_of(e) == Some(a))
                    .collect();
                match (nodes.remove(a), errors.as_slice()) {
                    (None, _) => Err(MALFORMED),
                    (Some(Value::Null), [e]) if not_found(e) => Err(Outcome::NotFound),
                    (Some(Value::Null), _) => Err(INCONSISTENT),
                    (Some(node), []) => serde_json::from_value(node).map_err(|_| MALFORMED),
                    _ => Err(INCONSISTENT),
                }
            })
            .collect();
        Aliased {
            nodes: answered,
            repository_id: nodes.get("databaseId").and_then(Value::as_u64),
            viewer_id,
        }
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
