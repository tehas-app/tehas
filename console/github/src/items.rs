//! What a number names where issues and pull requests share one number
//! space: a ticket or a change, as `Tickets::what_is` and
//! `Tickets::what_are` answer. Several numbers go in one GraphQL question,
//! an alias a number, as the dispatcher's read of a blocker's state asks
//! today.
//!
//! The question is read item by item beside gh's failure, as
//! `aliases.rs` reads it.

use std::collections::BTreeMap;
use std::num::NonZeroU64;

use serde::Deserialize;

use crate::aliases::alias;
use crate::GitHub;
use tehas_ports::{ChangeNo, Got, Item, Outcome, TicketKey, Unread, ALL_MAX};

const INCONSISTENT: Outcome = Outcome::Unread(Unread::Inconsistent);
const CUT: Outcome = Outcome::Unread(Unread::Cut);

/// The most numbers one question asks: more go in another question.
const PER_QUESTION: usize = 100;

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
struct Node {
    #[serde(rename = "__typename")]
    is: String,
    number: Option<u64>,
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
        let aliases: Vec<String> = numbers.iter().map(|n| alias(*n)).collect();
        match self.ask_aliased::<Node>("the items' question", &args, &aliases) {
            Ok(asked) => numbers
                .iter()
                .zip(asked.nodes)
                .map(|(n, node)| node.and_then(|node| item(*n, &node)))
                .collect(),
            Err(o) => vec![Err(o); numbers.len()],
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
