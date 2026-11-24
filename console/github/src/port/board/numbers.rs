//! `Board::open_changes` asked for numbers: the refresh's read of one
//! pull request, and of a few. Each number is one GraphQL question, its
//! digest and, when siblings are asked, the open pull requests that close
//! each issue it closes. A blueprint it closes and its ticket are the one
//! REST read of the issue, as `Blueprints::blueprint` reads it at
//! `Depth::Cut { notes: 0 }`.

use std::collections::{BTreeMap, HashSet};

use serde::Deserialize;

use super::{answer, malformed, pull_fields, Closing, Counted, NamedRepo, Pull, CLOSES_MAX};
use crate::GitHub;
use tehas_ports::{
    BlueprintNo, Bound, ChangeAsk, ChangeDigest, ChangeFilter, ChangeItem, ChangeNo, ChangeState,
    ChangeStates, Got, Outcome, Ref, Refusal, Unread, Whole,
};

/// The pull requests one question reads of each issue a change closes,
/// the cap of the siblings' bound: past ten closers of one issue, a change
/// asked with siblings is `Unread(Cut)`, whatever bound was asked. Ten, as
/// the dispatcher's read of one pull request asks today; each costs
/// GitHub's budget the change's whole digest.
const CLOSERS_PAGE: usize = 10;

/// One pull request by its number, `{PULL}` its digest's fields and
/// `{CLOSERS}` the closers of each issue it closes, when asked.
const ONE: &str = "query($owner: String!, $name: String!, $number: Int!) { repository(owner: $owner, name: $name) { databaseId pullRequest(number: $number) { {PULL}{CLOSERS} } } }";

/// The pull requests that close each issue a change closes, beside the
/// change's own list of those issues.
const CLOSERS: &str = " closers: closingIssuesReferences(first: {CLOSES_MAX}) { totalCount nodes { number repository { databaseId nameWithOwner } closedByPullRequestsReferences(first: {CLOSERS_PAGE}) { totalCount nodes { baseRepository { databaseId nameWithOwner } {PULL} } } } }";

fn one_query(closers: bool) -> String {
    let closers = match closers {
        true => CLOSERS
            .replace("{CLOSES_MAX}", &CLOSES_MAX.to_string())
            .replace("{CLOSERS_PAGE}", &CLOSERS_PAGE.to_string()),
        false => String::new(),
    };
    ONE.replace("{CLOSERS}", &closers)
        .replace("{PULL}", &pull_fields())
}

/// Whether the ask is one this file answers: numbers and states alone,
/// with no files and no events.
pub(super) fn asked(ask: &ChangeAsk) -> bool {
    let f = &ask.filter;
    let rest = ChangeFilter {
        states: f.states,
        numbers: f.numbers.clone(),
        ..ChangeFilter::default()
    };
    !f.numbers.is_empty() && *f == rest && ask.files.is_none() && ask.activity.is_none()
}

impl GitHub {
    /// The changes of the numbers asked, in number order, each once. More
    /// numbers than `bound` is `Unread(Cut)` before any request. A change
    /// of a state not asked is left out. A number GitHub does not know
    /// fails the read: gh exits non-zero on the error that says so.
    pub(super) fn numbered_changes(
        &mut self,
        ask: &ChangeAsk,
        bound: Bound,
    ) -> Got<Whole<ChangeItem>> {
        let mut numbers = ask.filter.numbers.clone();
        numbers.sort();
        numbers.dedup();
        if numbers.len() > bound.get() as usize {
            return Err(Outcome::Unread(Unread::Cut));
        }
        let mut items = Vec::with_capacity(numbers.len());
        for no in numbers {
            let (change, open_closers) = self.one_change(no, ask.siblings.is_some())?;
            if !takes(ask.filter.states, &change.state) {
                continue;
            }
            let mut item = ChangeItem::new(change);
            if let Some(each) = ask.siblings {
                item.siblings = siblings(&item.change, open_closers, each)?;
            }
            if ask.closing {
                for no in here(&item.change) {
                    match self.blueprint_and_digest(no) {
                        Ok((blueprint, ticket)) => item.closing.push((blueprint, Some(ticket))),
                        Err(Outcome::NotFound) => {}
                        Err(other) => return Err(other),
                    }
                }
            }
            items.push(item);
        }
        Ok(Whole::new(items))
    }

    /// Pull request `no` as a digest, and, when `closers`, the open pull
    /// requests of this project that close an issue of this project it
    /// closes, each once, in number order.
    fn one_change(
        &mut self,
        no: ChangeNo,
        closers: bool,
    ) -> Got<(ChangeDigest, Vec<ChangeDigest>)> {
        let (owner, name) = self.owner_name();
        let query = format!("query={}", one_query(closers));
        let number = format!("number={}", no.get());
        let out = self
            .call(
                "a change",
                &[
                    "api", "graphql", "-F", &owner, "-F", &name, "-F", &number, "-f", &query,
                ],
            )
            .map_err(|e| self.outcome(&e))?;
        // gh exits non-zero when GitHub says the number is not there, so
        // an answer of none does not add up.
        let read = answer::<OneRepo>(&out)?
            .pull_request
            .ok_or(Outcome::Unread(Unread::Inconsistent))?;
        let change = self.pull_digest(read.pull, ChangeStates::All)?;
        if change.no != no {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        let open = match (closers, read.closers) {
            (false, _) => Vec::new(),
            (true, None) => return Err(malformed()),
            (true, Some(c)) => self.open_closers(&change, c)?,
        };
        Ok((change, open))
    }

    /// The open closers of this project of each issue of this project the
    /// change closes. The issues must be the change's own list of them;
    /// an issue with more closers than one question reads is
    /// `Unread(Cut)`. An open closer of another project, or of none
    /// GitHub names, is no change the port can carry as a sibling, and
    /// leaving it out would read as none: `Refused(Unsupported)`. A closer
    /// that is not open is left out.
    fn open_closers(
        &mut self,
        change: &ChangeDigest,
        closers: Counted<Closer>,
    ) -> Got<Vec<ChangeDigest>> {
        let inconsistent = Outcome::Unread(Unread::Inconsistent);
        if closers.total_count != closers.nodes.len() as u64 {
            return Err(inconsistent);
        }
        let mut issues = Vec::with_capacity(closers.nodes.len());
        let mut lists = Vec::with_capacity(closers.nodes.len());
        for c in closers.nodes {
            let c = c.ok_or(inconsistent.clone())?;
            let closing = Closing {
                number: c.number,
                repository: c.repository,
            };
            let at = self.closed_ref(closing)?;
            if matches!(at, Ref::Here(_)) {
                lists.push(c.closed_by_pull_requests_references);
            }
            issues.push(at);
        }
        if issues != change.closes {
            return Err(inconsistent);
        }
        let mut open: BTreeMap<ChangeNo, ChangeDigest> = BTreeMap::new();
        for list in lists {
            let held = list.nodes.len() as u64;
            if list.total_count > held {
                return Err(Outcome::Unread(Unread::Cut));
            }
            if list.total_count < held {
                return Err(inconsistent);
            }
            for node in list.nodes {
                let node = node.ok_or(inconsistent.clone())?;
                let ours = match &node.base_repository {
                    Some(r) => self.is_here_id(r.database_id)?,
                    None => false,
                };
                if !ours {
                    match node.pull.state.as_str() {
                        "OPEN" => return Err(Outcome::Refused(Refusal::Unsupported)),
                        "MERGED" | "CLOSED" => continue,
                        _ => return Err(malformed()),
                    }
                }
                let closer = self.pull_digest(node.pull, ChangeStates::All)?;
                if closer.state != ChangeState::Open {
                    continue;
                }
                match open.get(&closer.no) {
                    Some(seen) if *seen != closer => return Err(inconsistent),
                    Some(_) => {}
                    None => {
                        open.insert(closer.no, closer);
                    }
                }
            }
        }
        Ok(open.into_values().collect())
    }
}

/// Whether a change of `state` is of the states asked.
fn takes(asked: ChangeStates, state: &ChangeState) -> bool {
    matches!(
        (asked, state),
        (ChangeStates::All, _)
            | (ChangeStates::Open, ChangeState::Open)
            | (ChangeStates::Merged, ChangeState::Merged { .. })
            | (ChangeStates::Closed, ChangeState::Closed { .. })
    )
}

/// The issues of this project a change closes, each once, in its order.
fn here(change: &ChangeDigest) -> Vec<BlueprintNo> {
    let mut seen = HashSet::new();
    change
        .closes
        .iter()
        .filter_map(super::here_of)
        .filter(|no| seen.insert(*no))
        .collect()
}

/// The other open changes of `open` that close what `change` closes, in
/// number order: more than `each`, `change` itself counted where it is
/// open, is `Unread(Cut)`.
fn siblings(change: &ChangeDigest, open: Vec<ChangeDigest>, each: Bound) -> Got<Vec<ChangeDigest>> {
    let others: Vec<ChangeDigest> = open.into_iter().filter(|c| c.no != change.no).collect();
    let counted = others.len() + usize::from(change.state == ChangeState::Open);
    if counted > each.get() as usize {
        return Err(Outcome::Unread(Unread::Cut));
    }
    Ok(others)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OneRepo {
    pull_request: Option<WithClosers>,
}

/// A pull request, and the closers of what it closes when asked.
#[derive(Deserialize)]
struct WithClosers {
    #[serde(flatten)]
    pull: Pull,
    #[serde(default)]
    closers: Option<Counted<Closer>>,
}

/// An issue a change closes, with the pull requests that close it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Closer {
    number: Option<u64>,
    repository: NamedRepo,
    closed_by_pull_requests_references: Counted<Sibling>,
}

/// A pull request that closes an issue, and the project it is in.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Sibling {
    base_repository: Option<NamedRepo>,
    #[serde(flatten)]
    pull: Pull,
}
