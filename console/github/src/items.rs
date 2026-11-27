//! What a number names where issues and pull requests share one number
//! space: a ticket or a change, as `Tickets::what_is` and
//! `Tickets::what_are` answer. Several numbers go in one GraphQL question,
//! an alias a number, as the dispatcher's read of a blocker's state asks
//! today.
//!
//! GitHub answers a number it does not know with a null node and a
//! `NOT_FOUND` error at that alias, and gh then exits 1, so the question is
//! read from gh's answer beside its failure, item by item. That reading,
//! `ask_aliased`, is the adapter's one reader of a question of several
//! items (`port/closed.rs`, `port/code_host/numbers.rs`, and by node id
//! `port/blueprints/edits.rs`).

use std::collections::BTreeMap;
use std::num::NonZeroU64;

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;

use crate::graphql::{limited, priced, Error, Step, LIMITED_FLOOR};
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
    errors: Option<Vec<Error>>,
}

#[derive(Deserialize)]
struct Data {
    /// Each alias asked, null where GitHub has no node for it, and any
    /// other field asked of the repository.
    #[serde(default)]
    repository: Option<BTreeMap<String, Value>>,
    /// The nodes asked by id, null where GitHub has none, when the
    /// question asks them so.
    #[serde(default)]
    nodes: Option<Vec<Value>>,
    /// The account asking, when the question asks it.
    #[serde(default)]
    viewer: Option<Viewer>,
}

#[derive(Deserialize)]
struct Viewer {
    #[serde(rename = "databaseId")]
    database_id: Option<u64>,
}

/// What an aliased question answered: each item's node, as
/// `GitHub::ask_aliased` reads it, and beside them the account asking's
/// id, where the question asked it and GitHub gave it.
pub(crate) struct Aliased<T> {
    pub(crate) nodes: Vec<Got<T>>,
    pub(crate) viewer_id: Option<u64>,
}

/// Where the items of a question of several lie: under the repository,
/// each by its alias, or in `nodes`, each by its place in the ids asked.
#[derive(Clone, Copy)]
enum Items<'a> {
    Aliases(&'a [String]),
    Nodes(usize),
}

/// An item an error names: an alias asked, or a place in `nodes`.
#[derive(PartialEq)]
enum Named<'a> {
    Alias(&'a str),
    At(u64),
}

impl Items<'_> {
    fn len(&self) -> usize {
        match self {
            Items::Aliases(a) => a.len(),
            Items::Nodes(n) => *n,
        }
    }

    /// The item `e` is about, None for one about the question. An alias
    /// not asked, or a place in `nodes` past those asked, is no item's: the
    /// question's.
    fn named<'e>(&self, e: &'e Error) -> Option<Named<'e>> {
        match (self, e.path.as_slice()) {
            (Items::Aliases(asked), [Step::Name(r), Step::Name(a), ..])
                if r == "repository" && asked.contains(a) =>
            {
                Some(Named::Alias(a))
            }
            (Items::Nodes(asked), [Step::Name(n), Step::At(i), ..])
                if n == "nodes" && *i < *asked as u64 =>
            {
                Some(Named::At(*i))
            }
            _ => None,
        }
    }
}

#[derive(Deserialize)]
struct Node {
    #[serde(rename = "__typename")]
    is: String,
    number: Option<u64>,
}

/// GitHub's word that the item names nothing: the item itself is not
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
    /// vouched.
    ///
    /// - An error that names no alias asked leaves no alias sure: a spent
    ///   priced budget for every one when GitHub says the rate is limited,
    ///   else `Unread(Inconsistent)`.
    /// - A null node with exactly one `NOT_FOUND` at its alias is
    ///   `NotFound` (Decided 121); any other error at an alias, or a null
    ///   node with none, is `Unread(Inconsistent)` for it.
    /// - A node not of the type asked, or an alias missing, is
    ///   `Unread(Malformed)`.
    /// - gh's failure with nothing to read, or with an answer that holds no
    ///   error to read it by, is its outcome for every alias; its
    ///   `NotFound`, which names no alias, `Unread(Inconsistent)`.
    pub(crate) fn ask_aliased<T: DeserializeOwned>(
        &mut self,
        call: &str,
        question: &str,
        aliases: &[String],
    ) -> Vec<Got<T>> {
        self.ask_aliased_beside(call, question, aliases).nodes
    }

    /// [`GitHub::ask_aliased`], with the `viewer`'s `databaseId` where the
    /// question asks it: None where it fails whole.
    pub(crate) fn ask_aliased_beside<T: DeserializeOwned>(
        &mut self,
        call: &str,
        question: &str,
        aliases: &[String],
    ) -> Aliased<T> {
        self.ask_several(call, question, Items::Aliases(aliases))
    }

    /// [`GitHub::ask_aliased`] of a question of `count` nodes by id, read
    /// from its `nodes` place by place.
    pub(crate) fn ask_nodes<T: DeserializeOwned>(
        &mut self,
        call: &str,
        question: &str,
        count: usize,
    ) -> Vec<Got<T>> {
        self.ask_several(call, question, Items::Nodes(count)).nodes
    }

    fn ask_several<T: DeserializeOwned>(
        &mut self,
        call: &str,
        question: &str,
        items: Items,
    ) -> Aliased<T> {
        let asked = items.len();
        let all = |o: Outcome| Aliased {
            nodes: (0..asked).map(|_| Err(o.clone())).collect(),
            viewer_id: None,
        };
        let (owner, name) = self.owner_name();
        let query = format!("query={question}");
        let args = ["api", "graphql", "-F", &owner, "-F", &name, "-f", &query];
        let (out, failed) = match self.call_answer(call, &args) {
            Ok(out) => (out, None),
            Err((e, Some(out))) => (out, Some(e)),
            Err((e, None)) => return all(self.failed(&e)),
        };
        let read = serde_json::from_str::<Value>(out.trim())
            .ok()
            .and_then(|mut v| {
                tehas_core::accounts::vouch(&mut v);
                serde_json::from_value::<Answer>(v).ok()
            });
        let Some(answer) = read else {
            return all(failed.map_or(MALFORMED, |e| self.failed(&e)));
        };
        let errors = answer.errors.unwrap_or_default();
        let whole: Vec<&Error> = errors.iter().filter(|e| items.named(e).is_none()).collect();
        if !whole.is_empty() {
            return all(limited(whole, (self.now)().saturating_add(LIMITED_FLOOR)));
        }
        let (mut repository, mut nodes, viewer_id) = match answer.data {
            Some(d) => (d.repository, d.nodes, d.viewer.and_then(|v| v.database_id)),
            None => (None, None, None),
        };
        let listed = match items {
            Items::Aliases(_) => repository.is_some(),
            Items::Nodes(_) => nodes.is_some(),
        };
        if !listed || (failed.is_some() && errors.is_empty()) {
            // No item, or gh failed with no error to read the items by: the
            // call's own outcome.
            return all(failed.map_or(MALFORMED, |e| self.failed(&e)));
        }
        let mut take = |i: usize| -> (Named, Option<Value>) {
            match items {
                Items::Aliases(a) => (
                    Named::Alias(&a[i]),
                    repository.as_mut().and_then(|r| r.remove(&a[i])),
                ),
                Items::Nodes(_) => (
                    Named::At(i as u64),
                    nodes
                        .as_mut()
                        .and_then(|n| n.get_mut(i))
                        .map(std::mem::take),
                ),
            }
        };
        let answered = (0..asked)
            .map(|i| {
                let (at, node) = take(i);
                let errors: Vec<&Error> = errors
                    .iter()
                    .filter(|e| items.named(e).as_ref() == Some(&at))
                    .collect();
                match (node, errors.as_slice()) {
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
            viewer_id,
        }
    }

    /// The outcome of a question gh failed with no answer to read. GitHub
    /// answers an item it does not know beside the error, so a `NotFound`
    /// with no answer names no item, even of one: more likely the
    /// repository or the token.
    fn failed(&self, e: &crate::ForgeError) -> Outcome {
        match priced(self.outcome(e)) {
            Outcome::NotFound => INCONSISTENT,
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

    /// gh's exit 1 on an answer with errors, the answer beside it.
    struct Answers(String);

    impl tehas_core::forge::GhRunner for Answers {
        fn run(&mut self, _: &[&str]) -> Result<String, String> {
            unreachable!()
        }
        fn run_answer(&mut self, _: &[&str]) -> Result<String, tehas_core::forge::Failed> {
            Err(tehas_core::forge::Failed {
                why: ": gh exited 1".into(),
                answer: Some(self.0.clone()),
            })
        }
    }

    #[test]
    fn an_error_at_a_node_past_those_asked_is_the_questions() {
        let at = |i: u64| {
            format!(
                r#"{{"data":{{"nodes":[{{"n":1}},{{"n":2}}]}},"errors":[{{"type":"NOT_FOUND","path":["nodes",{i}]}}]}}"#
            )
        };
        let ask = |answer: String| {
            GitHub::new("acme/widget", Box::new(Answers(answer))).ask_nodes::<Value>(
                "the nodes",
                "query",
                2,
            )
        };
        assert_eq!(
            ask(at(1)),
            [Ok(serde_json::json!({"n": 1})), Err(INCONSISTENT)]
        );
        assert_eq!(ask(at(2)), [Err(INCONSISTENT), Err(INCONSISTENT)]);
    }

    #[test]
    fn an_error_at_an_alias_not_asked_is_the_questions() {
        let at = |alias: &str| {
            format!(
                r#"{{"data":{{"repository":{{"n1":{{"n":1}},"n2":{{"n":2}}}}}},"errors":[{{"type":"NOT_FOUND","path":["repository","{alias}"]}}]}}"#
            )
        };
        let ask = |answer: String| {
            GitHub::new("acme/widget", Box::new(Answers(answer))).ask_aliased::<Value>(
                "the aliases",
                "query",
                &["n1".to_string(), "n2".to_string()],
            )
        };
        assert_eq!(
            ask(at("n2")),
            [Ok(serde_json::json!({"n": 1})), Err(INCONSISTENT)]
        );
        assert_eq!(ask(at("foo")), [Err(INCONSISTENT), Err(INCONSISTENT)]);
    }

    /// A limited rate holds the priced budget out past now: GitHub's
    /// reset is not read, and until now would hold nothing.
    #[test]
    fn a_limited_question_holds_the_budget_past_now() {
        let answer = r#"{"data":{"repository":{"n1":null}},"errors":[{"type":"RATE_LIMITED","message":"API rate limit exceeded"}]}"#;
        let got = GitHub::new("acme/widget", Box::new(Answers(answer.into())))
            .on_clock(|| 1_000)
            .ask_aliased::<Value>("the aliases", "query", &["n1".to_string()]);
        let [Err(Outcome::Spent { pool, until })] = got.as_slice() else {
            panic!("{got:?}");
        };
        assert_eq!(*pool, tehas_ports::Pool::Priced);
        assert!(until.secs() > 1_000, "{until:?}");
    }
}
