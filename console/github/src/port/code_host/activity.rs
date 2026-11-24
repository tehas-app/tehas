//! `CodeHost::activity`: a pull request's label events, as `Marked` and
//! `Unmarked`, and `CodeHost::branch`: the commit a branch points at.
//! Each is a GraphQL question; the events are paged here, a whole read
//! to its end or one past the bound, the newest n back from the end.
//!
//! The activity answers no other kind of event: pushes, comments,
//! reviews, merges and closes are not read, so a caller that needs them
//! does not get them from this adapter yet.

use serde::Deserialize;

use super::change::{answer, priced};
use crate::tickets::{account, take};
use crate::{gh, ForgeError, GitHub};
use tehas_ports::{
    Account, At, Branch, ChangeEvent, ChangeHappened, ChangeNo, CommitId, Got, Outcome, Took,
    Unread, Window,
};

const MALFORMED: Outcome = Outcome::Unread(Unread::Malformed);
const INCONSISTENT: Outcome = Outcome::Unread(Unread::Inconsistent);
const CUT: Outcome = Outcome::Unread(Unread::Cut);

/// The most pages one read of events asks for, a hundred events each.
/// Past them the read is `Unread(Cut)`.
const PAGES_MAX: usize = 100;

/// An account, with the user id beside its login that `Ours` is read by.
const ACTOR: &str = "__typename login ...on User { databaseId } ...on Bot { databaseId }";

/// A page of the pull request's label events: the oldest first for a
/// whole read, the newest for the last n.
fn events_question(window: Window) -> String {
    let (page, info) = match window {
        Window::Whole(_) => ("first: 100, after: $cursor", "hasNextPage endCursor"),
        Window::Last(_) => ("last: 100, before: $cursor", "hasPreviousPage startCursor"),
    };
    let node = format!("createdAt actor {{ {ACTOR} }} label {{ name }}");
    format!("query($owner: String!, $name: String!, $number: Int!, $cursor: String) {{ repository(owner: $owner, name: $name) {{ pullRequest(number: $number) {{ number events: timelineItems(itemTypes: [LABELED_EVENT, UNLABELED_EVENT], {page}) {{ pageInfo {{ {info} }} nodes {{ __typename ... on LabeledEvent {{ {node} }} ... on UnlabeledEvent {{ {node} }} }} }} }} }} }}")
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
    actor: Option<Who>,
    label: Option<Name>,
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

impl GitHub {
    /// The pull request's label events, oldest first; of two in one
    /// second an addition before a removal, else the site's order. A
    /// label no mark can have is left out, and kept for
    /// [`GitHub::take_left_out`]. A whole read past its bound, or past
    /// `PAGES_MAX` pages, is `Unread(Cut)`; the newest n are read back from
    /// the end until the event before them is older, so `more` is exact.
    /// An account is `Ours` only where GitHub types it a bot and it
    /// carries the factory's user id. An issue's number is `NotFound`.
    pub(super) fn activity_of(&mut self, no: ChangeNo, window: Window) -> Got<Took<ChangeEvent>> {
        let question = events_question(window);
        let back = matches!(window, Window::Last(_));
        let mut seen: Vec<String> = Vec::new();
        let mut all: Vec<ChangeEvent> = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let page = self.events_page(no, &question, cursor.as_deref())?;
            let mut read = self.read_events(page.nodes.ok_or(MALFORMED)?)?;
            let next = next_page(page.page_info, back, &mut seen)?;
            match window {
                Window::Whole(bound) => {
                    all.append(&mut read);
                    if all.len() > bound.get() as usize {
                        return Err(CUT);
                    }
                }
                Window::Last(n) => {
                    read.append(&mut all);
                    all = read;
                    order(&mut all);
                    // The newest n are sure once the event before them is
                    // older: one more of the same second could sort among
                    // them.
                    let n = n.get() as usize;
                    if all.len() > n && all[all.len() - n - 1].at < all[all.len() - n].at {
                        return take(all, window).map(|mut t| {
                            t.more = true;
                            t
                        });
                    }
                }
            }
            match next {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }
        order(&mut all);
        take(all, window)
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
        let out = match self.call("the branch", &args) {
            Ok(out) => out,
            Err(e) => {
                let path = format!("repos/{}", self.repo);
                return Err(self.question_outcome(&e, &path));
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

    /// One page of the pull request's label events, from `cursor`. The
    /// answer must be of the pull request asked.
    fn events_page(&mut self, no: ChangeNo, question: &str, cursor: Option<&str>) -> Got<Timeline> {
        let out = match self.ask(
            "the pull request's events",
            question,
            Some(no.get()),
            cursor,
        ) {
            Ok(out) => out,
            Err(e) => {
                let path = format!("repos/{}/pulls/{}", self.repo, no.get());
                return Err(self.question_outcome(&e, &path));
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

    /// The events of one page, in its order, those of a label no mark can
    /// have left out. Every node is a label event with a time and a label;
    /// an empty node is `Unread(Inconsistent)`.
    fn read_events(&mut self, nodes: Vec<Option<Node>>) -> Got<Vec<ChangeEvent>> {
        let mut nodes = nodes
            .into_iter()
            .map(|n| n.ok_or(INCONSISTENT))
            .collect::<Got<Vec<Node>>>()?;
        let mut labels = Vec::with_capacity(nodes.len());
        for n in &mut nodes {
            labels.push(n.label.take().and_then(|l| l.name).ok_or(MALFORMED)?);
        }
        let marks = self.read_marks(&labels);
        let mut read = Vec::with_capacity(nodes.len());
        for (n, mark) in nodes.into_iter().zip(marks) {
            let added = match n.is.as_deref() {
                Some("LabeledEvent") => true,
                Some("UnlabeledEvent") => false,
                _ => return Err(INCONSISTENT),
            };
            let at = n
                .created_at
                .as_deref()
                .and_then(tehas_core::time::from_iso)
                .map(At::new)
                .ok_or(MALFORMED)?;
            let by = n.actor.map(|w| self.actor(w)).transpose()?.flatten();
            let Some(mark) = mark else {
                continue;
            };
            let what = match added {
                true => ChangeHappened::Marked(mark),
                false => ChangeHappened::Unmarked(mark),
            };
            let mut event = ChangeEvent::new(what, at);
            event.by = by;
            read.push(event);
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

    /// The outcome of a GraphQL question that failed. gh exits 1 with no
    /// word of its own when the answer holds errors, as for a number no
    /// pull request has or a project that is gone: the REST read at `path`
    /// settles it, its `NotFound` or `Forbidden`, and anything else is
    /// `Unread(Inconsistent)`. A spent budget is the pool of priced reads.
    fn question_outcome(&mut self, e: &ForgeError, path: &str) -> Outcome {
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

/// The cursor of the page after this one, going back for the newest n,
/// None after the last. A page that does not say whether there is more,
/// or says so with no cursor or one already used, is
/// `Unread(Inconsistent)`; more than `PAGES_MAX` pages is `Unread(Cut)`.
fn next_page(info: Option<Info>, back: bool, seen: &mut Vec<String>) -> Got<Option<String>> {
    let info = info.ok_or(INCONSISTENT)?;
    let (more, cursor) = match back {
        true => (info.has_previous_page, info.start_cursor),
        false => (info.has_next_page, info.end_cursor),
    };
    if !more.ok_or(INCONSISTENT)? {
        return Ok(None);
    }
    let cursor = cursor.ok_or(INCONSISTENT)?;
    if seen.contains(&cursor) {
        return Err(INCONSISTENT);
    }
    if seen.len() + 1 >= PAGES_MAX {
        return Err(CUT);
    }
    seen.push(cursor.clone());
    Ok(Some(cursor))
}

/// Oldest first; of two events in one second an addition before a
/// removal, and else the order of the site's answer.
fn order(events: &mut [ChangeEvent]) {
    let removal = |e: &ChangeEvent| matches!(e.what, ChangeHappened::Unmarked(_));
    events.sort_by(|a, b| a.at.cmp(&b.at).then_with(|| removal(a).cmp(&removal(b))));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variable_the_questions_declare_is_used() {
        let last = Window::Last(std::num::NonZeroU32::MIN);
        let whole = Window::Whole(tehas_ports::Bound::new(1).unwrap());
        for q in [events_question(last), events_question(whole), BRANCH.into()] {
            let (head, body) = q.split_once(") {").unwrap();
            for var in head.split('$').skip(1) {
                let var = var.split(':').next().unwrap();
                assert!(body.contains(&format!("${var}")), "{var} in {q}");
            }
        }
    }

    #[test]
    fn every_account_the_question_reads_comes_with_its_user_id() {
        let q = events_question(Window::Last(std::num::NonZeroU32::MIN));
        assert_eq!(q.matches("actor {").count(), 2);
        assert_eq!(q.matches(ACTOR).count(), 2);
    }
}
