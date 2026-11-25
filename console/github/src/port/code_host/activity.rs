//! `CodeHost::activity`: a pull request's timeline as the ports' events,
//! every kind `ChangeHappened` names and any other item as `Unknown`, and
//! `CodeHost::branch`: the commit a branch points at. Each is a GraphQL
//! question. The timeline is paged here: a whole read forward to its end
//! or one past the bound, the newest n backward from the end, a page of a
//! hundred items for each hundred asked.

use serde::Deserialize;
use serde_json::Value;

use super::change::{answer, priced};
use crate::tickets::{account, take};
use crate::{gh, ForgeError, GitHub};
use tehas_ports::{
    Account, At, Branch, ChangeEvent, ChangeHappened, ChangeNo, CommitId, Got, Outcome, Pool, Took,
    Unread, Window, ALL_MAX,
};

const MALFORMED: Outcome = Outcome::Unread(Unread::Malformed);
const INCONSISTENT: Outcome = Outcome::Unread(Unread::Inconsistent);
const CUT: Outcome = Outcome::Unread(Unread::Cut);

/// The most pages one read asks for, a hundred items each. Past them a
/// whole read is `Unread(Cut)`, and the newest n hold what was read, with
/// `more`.
const PAGES_MAX: usize = 100;

/// An account, with the user id beside its login that `Ours` is read by.
const ACTOR: &str = "__typename login ...on User { databaseId } ...on Bot { databaseId }";

/// A page of the pull request's timeline, every item: the oldest first
/// for a whole read, the newest for the last n. Each kind the ports name
/// is asked with its time and its account; any other item names its type
/// alone.
fn events_question(back: bool) -> String {
    let (page, info) = match back {
        false => ("first: 100, after: $cursor", "hasNextPage endCursor"),
        true => ("last: 100, before: $cursor", "hasPreviousPage startCursor"),
    };
    let by = format!("createdAt actor {{ {ACTOR} }}");
    let nodes = [
        "... on PullRequestCommit { commit { committedDate } }".to_string(),
        format!("... on HeadRefForcePushedEvent {{ {by} afterCommit {{ committedDate }} }}"),
        format!("... on IssueComment {{ createdAt author {{ {ACTOR} }} }}"),
        format!("... on PullRequestReview {{ state submittedAt author {{ {ACTOR} }} }}"),
        format!("... on LabeledEvent {{ {by} label {{ name }} }}"),
        format!("... on UnlabeledEvent {{ {by} label {{ name }} }}"),
        format!("... on MergedEvent {{ {by} }}"),
        format!("... on ClosedEvent {{ {by} }}"),
        format!("... on ReopenedEvent {{ {by} }}"),
    ];
    format!("query($owner: String!, $name: String!, $number: Int!, $cursor: String) {{ repository(owner: $owner, name: $name) {{ pullRequest(number: $number) {{ number events: timelineItems({page}) {{ pageInfo {{ {info} }} nodes {{ __typename {} }} }} }} }} }}", nodes.join(" "))
}

/// The ref of one branch, exactly: no other branch answers for it.
const BRANCH: &str = "query($owner: String!, $name: String!, $ref: String!) { repository(owner: $owner, name: $name) { ref(qualifiedName: $ref) { name prefix target { __typename oid } } } }";

#[derive(Deserialize)]
struct Data<T> {
    repository: Option<T>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PullRepo {
    pull_request: Option<Pull>,
}

#[derive(Deserialize)]
struct Pull {
    number: Option<u64>,
    events: Option<Timeline>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Timeline {
    page_info: Option<Info>,
    nodes: Option<Vec<Option<Node>>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Info {
    has_next_page: Option<bool>,
    end_cursor: Option<String>,
    has_previous_page: Option<bool>,
    start_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Node {
    #[serde(rename = "__typename")]
    is: Option<String>,
    created_at: Option<String>,
    submitted_at: Option<String>,
    state: Option<String>,
    actor: Option<Who>,
    author: Option<Who>,
    label: Option<Name>,
    commit: Option<Commit>,
    after_commit: Option<Commit>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Commit {
    committed_date: Option<String>,
}

#[derive(Deserialize)]
struct Who {
    #[serde(rename = "__typename")]
    is: Option<String>,
    login: Option<String>,
    #[serde(rename = "databaseId")]
    id: Option<u64>,
}

#[derive(Deserialize)]
struct Name {
    name: Option<String>,
}

#[derive(Deserialize)]
struct RefRepo {
    #[serde(rename = "ref")]
    at: Option<GitRef>,
}

#[derive(Deserialize)]
struct GitRef {
    name: Option<String>,
    prefix: Option<String>,
    target: Option<Target>,
}

#[derive(Deserialize)]
struct Target {
    #[serde(rename = "__typename")]
    is: Option<String>,
    oid: Option<String>,
}

/// One item of the timeline, read: an event whose time an item of a kind
/// this read does not ask lacks until it is placed.
struct Item {
    what: ChangeHappened,
    by: Option<Account>,
    at: Option<At>,
    committed: Option<At>,
}

impl GitHub {
    /// The pull request's timeline, in the site's order, which is oldest
    /// first. Pushes are its commits, each at the time it was committed
    /// (GitHub names no time it was pushed, and no pusher), and its force
    /// pushes, at the push and committed when the new head was. A review
    /// still pending is no review given and is left out; so is a label no
    /// mark can have, which is kept for [`GitHub::take_left_out`]. Any other
    /// item is `Unknown`. GitHub's question gives it no time, so it takes
    /// the time of the next item after it that has one, or of the one
    /// before it when none follows, never the time of the read. Its
    /// borrowed time is then never later than a real event after it, and
    /// the events stay oldest first.
    ///
    /// A whole read past its bound, or past `PAGES_MAX` pages, is
    /// `Unread(Cut)`. The newest n are read backward, a page for each
    /// hundred, and are never cut: n past `ALL_MAX` reads as `ALL_MAX`,
    /// with `more` when the site holds more. An account is `Ours` only
    /// where GitHub types it a bot and it carries the factory's user id.
    /// An issue's number is `NotFound`.
    pub(super) fn activity_of(&mut self, no: ChangeNo, window: Window) -> Got<Took<ChangeEvent>> {
        let back = matches!(window, Window::Last(_));
        let question = events_question(back);
        let want = match window {
            Window::Whole(_) => None,
            Window::Last(n) => Some(n.get().min(ALL_MAX.get()) as usize),
        };
        let mut seen: Vec<String> = Vec::new();
        let mut all: Vec<Item> = Vec::new();
        let mut cursor: Option<String> = None;
        let mut earlier = false;
        loop {
            let page = self.events_page(no, &question, cursor.as_deref())?;
            let mut read = self.read_items(page.nodes.ok_or(MALFORMED)?)?;
            let next = next_page(page.page_info, back)?;
            match (window, want) {
                (Window::Whole(bound), _) => {
                    all.append(&mut read);
                    if all.len() > bound.get() as usize {
                        return Err(CUT);
                    }
                }
                (_, want) => {
                    read.append(&mut all);
                    all = read;
                    if want.is_some_and(|n| all.len() >= n) {
                        earlier = next.is_some();
                        break;
                    }
                }
            }
            let Some(c) = next else {
                break;
            };
            if seen.contains(&c) {
                return Err(INCONSISTENT);
            }
            if seen.len() + 1 >= PAGES_MAX {
                if back {
                    earlier = true;
                    break;
                }
                return Err(CUT);
            }
            seen.push(c.clone());
            cursor = Some(c);
        }
        let events = placed(all)?;
        match want {
            None => take(events, window),
            Some(n) => {
                let more = earlier || events.len() > n;
                let skip = events.len().saturating_sub(n);
                let mut took = Took::default();
                took.items = events.into_iter().skip(skip).collect();
                took.more = more;
                Ok(took)
            }
        }
    }

    /// The commit `name` points at, None when the project has no such
    /// branch. The ref is asked by its full name, so a renamed branch's
    /// old name answers None, never the new branch's head. A project that
    /// is gone is `NotFound`.
    pub(super) fn branch_at(&mut self, name: &Branch) -> Got<Option<CommitId>> {
        let (owner, project) = self.owner_name();
        let full = format!("ref=refs/heads/{}", name.as_str());
        let query = format!("query={BRANCH}");
        let args = [
            "api", "graphql", "-F", &owner, "-F", &project, "-f", &full, "-f", &query,
        ];
        let out = match self.call_answer("the branch", &args) {
            Ok(out) => out,
            Err((e, said)) => {
                let path = format!("repos/{}", self.repo);
                return Err(self.question_outcome(&e, said.as_deref(), &path));
            }
        };
        let data: Data<RefRepo> = answer(&out, (self.now)())?;
        let Some(at) = data.repository.ok_or(Outcome::NotFound)?.at else {
            return Ok(None);
        };
        if at.name.as_deref() != Some(name.as_str()) || at.prefix.as_deref() != Some("refs/heads/")
        {
            return Err(INCONSISTENT);
        }
        let target = at.target.ok_or(MALFORMED)?;
        if target.is.as_deref() != Some("Commit") {
            return Err(MALFORMED);
        }
        let oid = target.oid.ok_or(MALFORMED)?;
        CommitId::new(oid).map(Some).map_err(|_| MALFORMED)
    }

    /// One page of the pull request's timeline, from `cursor`. The answer
    /// must be of the pull request asked.
    fn events_page(&mut self, no: ChangeNo, question: &str, cursor: Option<&str>) -> Got<Timeline> {
        let (owner, name) = self.owner_name();
        let number = format!("number={}", no.get());
        let query = format!("query={question}");
        let from = cursor.map(|c| format!("cursor={c}"));
        let mut args = vec![
            "api", "graphql", "-F", &owner, "-F", &name, "-F", &number, "-f", &query,
        ];
        if let Some(from) = &from {
            args.extend(["-f", from.as_str()]);
        }
        let out = match self.call_answer("the pull request's events", &args) {
            Ok(out) => out,
            Err((e, said)) => {
                let path = format!("repos/{}/pulls/{}", self.repo, no.get());
                return Err(self.question_outcome(&e, said.as_deref(), &path));
            }
        };
        let data: Data<PullRepo> = answer(&out, (self.now)())?;
        let pull = data
            .repository
            .ok_or(Outcome::NotFound)?
            .pull_request
            .ok_or(Outcome::NotFound)?;
        if pull.number != Some(no.get()) {
            return Err(INCONSISTENT);
        }
        pull.events.ok_or(MALFORMED)
    }

    /// The items of one page, in its order, those left out left out. An
    /// empty node, or one with no type, is `Unread(Inconsistent)`; a kind
    /// asked that lacks its time, its label or an account's login is
    /// `Unread(Malformed)`.
    fn read_items(&mut self, nodes: Vec<Option<Node>>) -> Got<Vec<Item>> {
        let nodes = nodes
            .into_iter()
            .map(|n| n.ok_or(INCONSISTENT))
            .collect::<Got<Vec<Node>>>()?;
        let labels: Vec<String> = nodes
            .iter()
            .filter(|n| matches!(n.is.as_deref(), Some("LabeledEvent" | "UnlabeledEvent")))
            .map(|n| {
                n.label
                    .as_ref()
                    .and_then(|l| l.name.clone())
                    .ok_or(MALFORMED)
            })
            .collect::<Got<_>>()?;
        let mut marks = self.read_marks(&labels).into_iter();
        let mut read = Vec::with_capacity(nodes.len());
        for n in nodes {
            let is = n.is.as_deref().ok_or(INCONSISTENT)?;
            let mut committed = None;
            let (what, at, by) = match is {
                "PullRequestCommit" => {
                    let at = time(n.commit.and_then(|c| c.committed_date))?;
                    committed = Some(at);
                    (ChangeHappened::Pushed, Some(at), None)
                }
                "HeadRefForcePushedEvent" => {
                    committed = match n.after_commit {
                        Some(c) => Some(time(c.committed_date)?),
                        None => None,
                    };
                    (ChangeHappened::Pushed, Some(time(n.created_at)?), n.actor)
                }
                "IssueComment" => (
                    ChangeHappened::Commented,
                    Some(time(n.created_at)?),
                    n.author,
                ),
                "PullRequestReview" => {
                    if n.state.as_deref().ok_or(MALFORMED)? == "PENDING" {
                        continue;
                    }
                    (
                        ChangeHappened::Reviewed,
                        Some(time(n.submitted_at)?),
                        n.author,
                    )
                }
                "LabeledEvent" | "UnlabeledEvent" => {
                    let Some(mark) = marks.next().flatten() else {
                        continue;
                    };
                    let what = match is {
                        "LabeledEvent" => ChangeHappened::Marked(mark),
                        _ => ChangeHappened::Unmarked(mark),
                    };
                    (what, Some(time(n.created_at)?), n.actor)
                }
                "MergedEvent" => (ChangeHappened::Merged, Some(time(n.created_at)?), n.actor),
                "ClosedEvent" => (ChangeHappened::Closed, Some(time(n.created_at)?), n.actor),
                "ReopenedEvent" => (ChangeHappened::Reopened, Some(time(n.created_at)?), n.actor),
                _ => (ChangeHappened::Unknown, None, None),
            };
            let by = by.map(|w| self.actor(w)).transpose()?.flatten();
            read.push(Item {
                what,
                by,
                at,
                committed,
            });
        }
        Ok(read)
    }

    /// An event's account. One with no login is `Unread(Malformed)`.
    fn actor(&self, w: Who) -> Got<Option<Account>> {
        let login = w.login.ok_or(MALFORMED)?;
        let facts = gh::Facts {
            id: w.id,
            kind: w.is,
        };
        Ok(account(&login, &facts, self.bot.as_ref()))
    }

    /// The outcome of a GraphQL question that failed. gh exits 1 on any
    /// error of the answer and prints the answer beside it: a rate limit
    /// there is a spent budget of the priced pool, a `NOT_FOUND` is
    /// `NotFound`, and any other is `Unread(Inconsistent)`. With no answer,
    /// a bare exit 1 is settled by the REST read at `path`, its `NotFound`
    /// or `Forbidden`, anything else `Unread(Inconsistent)`; any other
    /// failure is its own outcome, a spent budget the priced pool's.
    pub(crate) fn question_outcome(
        &mut self,
        e: &ForgeError,
        said: Option<&str>,
        path: &str,
    ) -> Outcome {
        if let Some(said) = said.filter(|s| !s.trim().is_empty()) {
            let types: Vec<String> = serde_json::from_str::<Value>(said.trim())
                .ok()
                .and_then(|v| v.get("errors").and_then(Value::as_array).cloned())
                .unwrap_or_default()
                .iter()
                .filter_map(|e| e.get("type").and_then(Value::as_str).map(str::to_string))
                .collect();
            return if types.iter().any(|t| t == "RATE_LIMITED") {
                crate::graphql::limited(Pool::Priced, (self.now)())
            } else if types.iter().any(|t| t == "NOT_FOUND") {
                Outcome::NotFound
            } else {
                INCONSISTENT
            };
        }
        if e.why.trim_start_matches(": ") != "gh exited 1" {
            return priced(self.outcome(e));
        }
        match self.call("the settling read", &["api", path]) {
            Ok(_) => INCONSISTENT,
            Err(e) => match self.outcome(&e) {
                o @ (Outcome::NotFound | Outcome::Forbidden) => o,
                _ => INCONSISTENT,
            },
        }
    }
}

/// A time GitHub gives, or `Unread(Malformed)`.
fn time(iso: Option<String>) -> Got<At> {
    iso.as_deref()
        .and_then(tehas_core::time::from_iso)
        .map(At::new)
        .ok_or(MALFORMED)
}

/// The items as events, each without a time of its own placed at the next
/// time after it, or the last before it when none follows, so no borrowed
/// time is later than a real event after it. A read with no time at all to
/// place one by is `Unread(Inconsistent)`.
fn placed(items: Vec<Item>) -> Got<Vec<ChangeEvent>> {
    let mut next: Option<At> = None;
    let mut times = vec![None; items.len()];
    for (i, item) in items.iter().enumerate().rev() {
        next = item.at.or(next);
        times[i] = next;
    }
    let mut last: Option<At> = None;
    let mut events = Vec::with_capacity(items.len());
    for (item, after) in items.into_iter().zip(times) {
        let at = after.or(last).ok_or(INCONSISTENT)?;
        last = Some(at);
        let mut event = ChangeEvent::new(item.what, at);
        event.by = item.by;
        event.committed = item.committed;
        events.push(event);
    }
    Ok(events)
}

/// The cursor of the page after this one, going back for the newest n,
/// None after the last. A page that does not say whether there is more,
/// or says so with no cursor, is `Unread(Inconsistent)`.
fn next_page(info: Option<Info>, back: bool) -> Got<Option<String>> {
    let info = info.ok_or(INCONSISTENT)?;
    let (more, cursor) = match back {
        true => (info.has_previous_page, info.start_cursor),
        false => (info.has_next_page, info.end_cursor),
    };
    match more.ok_or(INCONSISTENT)? {
        false => Ok(None),
        true => cursor.map(Some).ok_or(INCONSISTENT),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variable_the_questions_declare_is_used() {
        for q in [events_question(true), events_question(false), BRANCH.into()] {
            let (head, body) = q.split_once(") {").unwrap();
            for var in head.split('$').skip(1) {
                let var = var.split(':').next().unwrap();
                assert!(body.contains(&format!("${var}")), "{var} in {q}");
            }
        }
    }

    #[test]
    fn every_account_the_question_reads_comes_with_its_user_id() {
        let q = events_question(true);
        let accounts = q.matches("actor {").count() + q.matches("author {").count();
        assert_eq!(accounts, 8);
        assert_eq!(q.matches(ACTOR).count(), accounts);
    }

    #[test]
    fn an_item_with_no_time_takes_the_next_time_or_else_the_last() {
        let at = |s: i64| Some(At::new(s));
        let item = |t: Option<At>| Item {
            what: ChangeHappened::Unknown,
            by: None,
            at: t,
            committed: None,
        };
        let got: Vec<At> = placed(vec![
            item(None),
            item(at(5)),
            item(None),
            item(at(9)),
            item(None),
        ])
        .unwrap()
        .into_iter()
        .map(|e| e.at)
        .collect();
        assert_eq!(
            got,
            [At::new(5), At::new(5), At::new(9), At::new(9), At::new(9)]
        );
        assert_eq!(placed(vec![item(None)]).map(|_| ()), Err(INCONSISTENT));
    }
}
