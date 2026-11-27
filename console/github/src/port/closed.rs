//! The tickets port's reads of closed tickets: the tickets closed since a
//! time (`closed_since`) and several tickets' digests each with what
//! closed it (`several_with_closers`). The closer is the one
//! `Ticket.closer` gives, read by the same rule from the newest close.
//!
//! `several_with_closers` asks one GraphQL question for up to a hundred
//! tickets, an alias a number, read alias by alias as `items.rs` reads
//! every aliased question: a number that is no issue is `NotFound`. Its
//! merged changes are the merged pull requests of this project among those
//! GitHub lists as closing the ticket, as `closed_since` counts them.

use std::collections::BTreeMap;

use serde::Deserialize;

use super::board::Pull;
use super::number;
use crate::ticket::{closes, digest_fields, present, time, Event, Node, Page, Pages};
use crate::GitHub;
use tehas_ports::{
    At, Bound, ChangeDigest, ChangeState, ChangeStates, ClosedTicket, Got, Outcome, TicketDigest,
    TicketKey, TicketState, Unread, Whole, WithCloser, ALL_MAX,
};

const MALFORMED: Outcome = Outcome::Unread(Unread::Malformed);
const INCONSISTENT: Outcome = Outcome::Unread(Unread::Inconsistent);
const CUT: Outcome = Outcome::Unread(Unread::Cut);

/// The tickets one question of `several_with_closers` asks: more go in
/// another question.
const PER_QUESTION: usize = 100;

/// The closed issues one page of `closed_since` reads. Each carries the
/// digests of the pull requests that close it, so a page is kept smaller
/// than the list's.
const SINCE_PAGE: usize = 50;

/// The pull requests one read names of each closed issue, as ops reads
/// them today (`shipped.rs`): past that the read is `Unread(Cut)`.
const REFERENCES: usize = 20;

/// The alias a ticket is asked under.
fn alias(n: u64) -> String {
    format!("i{n}")
}

/// The pull requests GitHub lists as closing an issue, which both reads
/// ask for and read the same way.
fn closing_references() -> String {
    format!(
        "closedByPullRequestsReferences(first: {REFERENCES}) {{ totalCount nodes {{ baseRepository {{ databaseId nameWithOwner }} {} }} }}",
        super::board::pull_fields()
    )
}

/// The question of several issues, each with its digest, its newest close
/// and the changes listed as closing it, one alias a number.
fn several_question(numbers: &[u64]) -> String {
    let fields = format!("{} {} {}", digest_fields(), closes(), closing_references());
    let mut q = String::from(
        "query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { databaseId",
    );
    for n in numbers {
        q.push_str(&format!(
            " {}: issue(number: {n}) {{ {fields} }}",
            alias(*n)
        ));
    }
    q.push_str(" } }");
    q
}

/// The question of the closed issues GitHub last saw change since
/// `$since`, oldest filed first, a page of them: each with its digest, its
/// close, its count of parts and the pull requests GitHub lists as closing
/// it. A close changes an issue, so every issue closed since then is among
/// them.
fn since_question() -> String {
    format!(
        "query($owner: String!, $name: String!, $cursor: String, $since: DateTime!) {{ repository(owner: $owner, name: $name) {{ databaseId issues(first: {SINCE_PAGE}, after: $cursor, orderBy: {{field: CREATED_AT, direction: ASC}}, filterBy: {{states: [CLOSED], since: $since}}) {{ pageInfo {{ hasNextPage endCursor }} nodes {{ {} closedAt {} subIssues {{ totalCount }} {} }} }} }} }}",
        digest_fields(),
        closes(),
        closing_references()
    )
}

/// An issue of the several question.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WithClose {
    #[serde(flatten)]
    node: Node,
    closes: Option<Page<Event>>,
    closed_by_pull_requests_references: Option<References>,
}

/// An issue of the closed-since question.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClosedNode {
    #[serde(flatten)]
    node: Node,
    #[serde(deserialize_with = "present")]
    closed_at: Option<String>,
    closes: Option<Page<Event>>,
    sub_issues: Option<Total>,
    closed_by_pull_requests_references: Option<References>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Total {
    total_count: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct References {
    total_count: Option<u64>,
    nodes: Vec<Option<Reference>>,
}

/// A pull request GitHub lists as closing an issue, and its project.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Reference {
    base_repository: Option<Repo>,
    #[serde(flatten)]
    pull: Pull,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Repo {
    #[serde(rename = "databaseId")]
    database_id: Option<u64>,
}

impl GitHub {
    /// Several tickets' digests, each with its closer, in the order asked,
    /// a repeated key answered each time from one alias. A key that is no
    /// number is `Refused(Invalid)`. More distinct numbers than `ALL_MAX`
    /// is `Unread(Cut)` for every one, with no question asked.
    pub(super) fn several_with_closers(
        &mut self,
        keys: &[TicketKey],
    ) -> Vec<(TicketKey, Got<WithCloser>)> {
        let mut distinct: Vec<u64> = keys.iter().filter_map(|k| number(k).ok()).collect();
        distinct.sort_unstable();
        distinct.dedup();
        let mut answers: BTreeMap<u64, Got<WithCloser>> = BTreeMap::new();
        if distinct.len() > ALL_MAX.get() as usize {
            answers.extend(distinct.iter().map(|n| (*n, Err(CUT))));
        } else {
            for chunk in distinct.chunks(PER_QUESTION) {
                let got = self.ask_closers(chunk);
                answers.extend(chunk.iter().copied().zip(got));
            }
        }
        keys.iter()
            .map(|k| {
                let got =
                    number(k).and_then(|n| answers.get(&n).cloned().unwrap_or(Err(INCONSISTENT)));
                (k.clone(), got)
            })
            .collect()
    }

    /// One question of distinct numbers: an answer for each, in order.
    fn ask_closers(&mut self, numbers: &[u64]) -> Vec<Got<WithCloser>> {
        let aliases: Vec<String> = numbers.iter().map(|n| alias(*n)).collect();
        let nodes: Vec<Got<WithClose>> =
            self.ask_aliased("the tickets' closers", &several_question(numbers), &aliases);
        numbers
            .iter()
            .zip(nodes)
            .map(|(n, node)| node.and_then(|node| self.with_closer(*n, node)))
            .collect()
    }

    /// One issue's digest, closer and merged changes, of the number asked.
    /// A response of a question that did not ask for the listed changes
    /// names none.
    fn with_closer(&mut self, asked: u64, read: WithClose) -> Got<WithCloser> {
        if read.node.number != Some(asked) {
            return Err(INCONSISTENT);
        }
        let digest = self.digest_from(read.node)?;
        let closer = self.newest_closer(&digest.state, read.closes)?;
        let merged = match read.closed_by_pull_requests_references {
            Some(refs) => self.merged_here(refs)?,
            None => Vec::new(),
        };
        let mut with = WithCloser::new(digest);
        with.closer = closer;
        with.merged = merged;
        Ok(with)
    }

    /// The tickets closed at or after `since`, whole, oldest filed first,
    /// ties by number. More than `bound` is `Unread(Cut)`.
    pub(super) fn closed_since(&mut self, since: At, bound: Bound) -> Got<Whole<ClosedTicket>> {
        let (owner, name) = self.owner_name();
        let query = format!("query={}", since_question());
        let from = format!("since={}", tehas_core::time::iso(since.secs()));
        let mut all: Vec<ClosedTicket> = Vec::new();
        let mut pages = Pages::default();
        let mut cursor: Option<String> = None;
        loop {
            let after = cursor.as_ref().map(|c| format!("cursor={c}"));
            let mut args = vec![
                "api", "graphql", "-F", &owner, "-F", &name, "-f", &from, "-f", &query,
            ];
            if let Some(after) = &after {
                args.extend(["-f", after.as_str()]);
            }
            let out = self
                .call("the closed issues", &args)
                .map_err(|e| self.outcome(&e))?;
            let page: Page<ClosedNode> =
                crate::graphql::part(&out, "/data/repository/issues", (self.now)())?;
            for node in page.nodes {
                let closed = self.closed_ticket(node.ok_or(INCONSISTENT)?)?;
                if all.iter().any(|c| c.digest.key == closed.digest.key) {
                    return Err(INCONSISTENT);
                }
                if closed.closed >= since {
                    all.push(closed);
                }
            }
            if all.len() > bound.get() as usize {
                return Err(CUT);
            }
            match pages.next(page.page_info)? {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        all.sort_by(|a, b| {
            let no = |d: &TicketDigest| number(&d.key).unwrap_or_default();
            a.digest
                .created
                .cmp(&b.digest.created)
                .then_with(|| no(&a.digest).cmp(&no(&b.digest)))
        });
        Ok(Whole::new(all))
    }

    /// A closed issue as the port holds it.
    fn closed_ticket(&mut self, n: ClosedNode) -> Got<ClosedTicket> {
        let digest = self.digest_from(n.node)?;
        if digest.state == TicketState::Open {
            return Err(INCONSISTENT);
        }
        let closed = time(n.closed_at.as_deref().ok_or(MALFORMED)?)?;
        let closer = self.newest_closer(&digest.state, n.closes)?;
        let parts = n
            .sub_issues
            .ok_or(MALFORMED)?
            .total_count
            .ok_or(INCONSISTENT)?;
        let merged = self.merged_here(n.closed_by_pull_requests_references.ok_or(MALFORMED)?)?;
        let mut ticket = ClosedTicket::new(digest, closed);
        ticket.closer = closer;
        ticket.part_count = u32::try_from(parts).map_err(|_| INCONSISTENT)?;
        ticket.merged = merged;
        Ok(ticket)
    }

    /// The merged changes of this project among the pull requests GitHub
    /// lists as closing a ticket; one of another project, or of none GitHub
    /// names, has no number here and is left out. More listed than the
    /// answer holds is `Unread(Cut)`.
    fn merged_here(&mut self, refs: References) -> Got<Vec<ChangeDigest>> {
        let total = refs.total_count.ok_or(INCONSISTENT)?;
        let held = refs.nodes.len() as u64;
        if total > held {
            return Err(CUT);
        }
        if total < held {
            return Err(INCONSISTENT);
        }
        let mut merged = Vec::new();
        for r in refs.nodes {
            let r = r.ok_or(INCONSISTENT)?;
            // A pull request whose repository is gone is no change here.
            let ours = match r.base_repository {
                Some(b) => self.is_here_id(b.database_id)?,
                None => false,
            };
            if !ours {
                continue;
            }
            let change = self.pull_digest(r.pull, ChangeStates::All)?;
            if matches!(change.state, ChangeState::Merged { .. })
                && !merged.iter().any(|m: &ChangeDigest| m.no == change.no)
            {
                merged.push(change);
            }
        }
        Ok(merged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_ticket_is_asked_under_its_own_alias() {
        let q = several_question(&[12, 34]);
        assert!(q.contains(" i12: issue(number: 12) { number title "));
        assert!(q.contains(" i34: issue(number: 34) { number title "));
        assert!(q.contains("closes: timelineItems(itemTypes: [CLOSED_EVENT], last: 1)"));
        assert!(q.ends_with(" } }"));
    }

    /// `several_with_closers` answers `WithCloser.merged` from them, so
    /// the question must carry the same list `closed_since` reads.
    #[test]
    fn the_several_question_asks_the_changes_listed_as_closing() {
        let q = several_question(&[12]);
        assert!(q.contains(&format!(
            "closedByPullRequestsReferences(first: {REFERENCES}) {{ totalCount nodes {{ baseRepository {{ databaseId nameWithOwner }} number title "
        )));
    }

    /// GitHub refuses a question that declares a variable it does not use.
    #[test]
    fn every_question_uses_each_variable_it_declares() {
        for q in [several_question(&[7]), since_question()] {
            let (head, body) = q.split_once(')').unwrap();
            for var in head.split('$').skip(1) {
                let name = &var[..var.find(':').unwrap()];
                assert!(body.contains(&format!("${name}")), "${name}: {q}");
            }
        }
    }

    /// The vouch reads a user id beside a login, so a question that asked a
    /// login without it would let a renamed account pass for a listed one.
    #[test]
    fn every_account_a_question_reads_comes_with_its_user_id() {
        for q in [several_question(&[7]), since_question()] {
            for (at, _) in q.match_indices("login") {
                let rest = &q[at..];
                let close = rest.find('}').unwrap();
                assert!(rest[..close].contains("databaseId"), "{}", &rest[..close]);
            }
        }
    }

    #[test]
    fn the_closed_read_asks_the_closed_issues_seen_since() {
        let q = since_question();
        assert!(q.contains(&format!("issues(first: {SINCE_PAGE}, after: $cursor")));
        assert!(q.contains("filterBy: {states: [CLOSED], since: $since}"));
        assert!(q.contains(&format!(
            "closedByPullRequestsReferences(first: {REFERENCES}) {{ totalCount "
        )));
        assert!(q.contains("subIssues { totalCount }"));
    }
}
