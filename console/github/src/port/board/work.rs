//! `Board::open_work`: the open issues in pages of 25, one GraphQL question
//! a page, each issue with what its work item holds, as dispatch's queue
//! page reads them. The issue is its own blueprint, read whole from the
//! same node; what waits on it, its parts and the changes that name it are
//! the same fragments `Tickets::ticket` asks, and its parts and mentions
//! are paged on alone past their first hundred.
//!
//! The marks are read whole. The page holds the newest hundred of the kinds
//! asked; where that is not every one, or where a mark the issue carries is
//! not applied by the newest event of it the page holds, GitHub cut the
//! timeline short, and each kind is read again on its own, as
//! `Tickets::history` reads it.
//!
//! The changes are those GitHub links as closing the issue, from this
//! project, open ones alone: the twenty a page asks for, past them the
//! item is `Unread(Cut)`.

use std::collections::HashSet;
use std::num::{NonZeroU32, NonZeroU64};

use serde::Deserialize;

use super::super::blueprint_writes::{ends_with_folded, text_revision};
use super::super::blueprints::Edited;
use super::{answer, pull_fields, Counted, Pull};
use crate::ticket::{
    self, order, time, Event, Named, Node, Page, Pages, Who, ACTOR, CUT, CUT_TEXT, INCONSISTENT,
    MALFORMED,
};
use crate::tickets::take;
use crate::{markers, GitHub};
use tehas_ports::{
    Blocker, Blueprint, BoardAsk, Bound, ChangeDigest, ChangeStates, Depth, Filter, Got,
    HappenedKind, OpenWork, Ticket, TicketEvent, TicketKey, Took, Whole, Window, WorkItem,
};

/// The open issues one page asks for, dispatch's `ISSUE_PAGE`. A page over
/// GitHub's budget for nested items comes back with its timelines cut
/// short and no error, so the size is load-bearing.
const ISSUE_PAGE: usize = 25;

/// The changes that close an issue a page asks for, as dispatch's queue
/// page asks. An issue with more is `Unread(Cut)`.
const CLOSING_MAX: usize = 20;

/// The accounts an issue is assigned a page asks for, the most GitHub
/// allows.
const ASSIGNEES_MAX: usize = 10;

/// The kinds of a ticket's history the page answers: the marks and the
/// retitles, the timeline dispatch's queue page asks.
const PAGE_KINDS: [HappenedKind; 3] = [
    HappenedKind::Marked,
    HappenedKind::Unmarked,
    HappenedKind::Retitled,
];

const MARKS: [HappenedKind; 2] = [HappenedKind::Marked, HappenedKind::Unmarked];

/// The question of one page of open issues: with the newest hundred events
/// of `kinds` when any, and each issue's closing changes when `changes`.
fn work_query(kinds: &[HappenedKind], changes: bool) -> String {
    let mut node = format!(
        "{} parent {{ number repository {{ nameWithOwner }} }} lastEditedAt editor {{ {ACTOR} }} comments {{ totalCount }} assignees(first: {ASSIGNEES_MAX}) {{ totalCount nodes {{ __typename login databaseId }} }} {} {} {}",
        ticket::digest_fields(),
        ticket::blocked_by(),
        ticket::parts(false),
        ticket::mentions(false),
    );
    if !kinds.is_empty() {
        if let Some(events) = ticket::timeline(Some(kinds), Window::Last(NonZeroU32::MIN), false) {
            node.push(' ');
            node.push_str(&events);
        }
    }
    if changes {
        node.push_str(&format!(
            " closedByPullRequestsReferences(first: {CLOSING_MAX}) {{ totalCount nodes {{ {} }} }}",
            pull_fields()
        ));
    }
    format!("query($owner: String!, $name: String!, $cursor: String) {{ repository(owner: $owner, name: $name) {{ issues(states: [OPEN], first: {ISSUE_PAGE}, after: $cursor, orderBy: {{field: CREATED_AT, direction: ASC}}) {{ totalCount pageInfo {{ hasNextPage endCursor }} nodes {{ {node} }} }} }} }}")
}

/// The ask's windows of history per kind, each kind once by its first, if
/// this adapter answers the ask: `Depth::Cut { notes: 0 }`, so no comment
/// is read, a history of the marks and retitles alone or none, every open
/// ticket, and open changes alone. None for any other ask.
fn answered(ask: &BoardAsk) -> Option<Vec<(HappenedKind, Window)>> {
    let mut filter = ask.filter.clone();
    filter.open = None;
    let changes = ask
        .changes
        .is_none_or(|(states, _)| states == ChangeStates::Open);
    let history = !ask.history_by_kind.is_empty() || ask.history.is_none();
    if ask.depth != (Depth::Cut { notes: 0 }) || filter != Filter::default() || !changes || !history
    {
        return None;
    }
    let mut kinds: Vec<(HappenedKind, Window)> = Vec::new();
    for (kind, window) in &ask.history_by_kind {
        if !PAGE_KINDS.contains(kind) {
            return None;
        }
        if !kinds.iter().any(|(k, _)| k == kind) {
            kinds.push((*kind, *window));
        }
    }
    Some(kinds)
}

/// The kinds the page's timeline asks: those asked, and both marks when
/// either is, so the page can say which mark stands.
fn page_kinds(asked: &[(HappenedKind, Window)]) -> Vec<HappenedKind> {
    let marks = asked.iter().any(|(k, _)| MARKS.contains(k));
    PAGE_KINDS
        .into_iter()
        .filter(|k| asked.iter().any(|(a, _)| a == k) || (marks && MARKS.contains(k)))
        .collect()
}

/// Whether the page's events vouch for the marks the issue carries: the
/// newest application or removal of each label it carries is an
/// application. Of two in one second the removal is the newer, an event
/// with no time is older than any, and of two alike the later in the
/// answer is the newer, as dispatch's guard orders them. A time that does
/// not read vouches for nothing.
fn vouches(events: &[Option<Event>], carried: &[String]) -> bool {
    carried.iter().all(|label| {
        let mut newest: Option<(Option<i64>, bool, usize)> = None;
        for (at, e) in events.iter().enumerate() {
            let Some(e) = e else {
                return false;
            };
            let removal = match e.is.as_str() {
                "LabeledEvent" => false,
                "UnlabeledEvent" => true,
                _ => continue,
            };
            if e.label.as_ref().is_none_or(|l| l.name != *label) {
                continue;
            }
            let when = match e.created_at.as_deref() {
                None => None,
                Some(t) => match tehas_core::time::from_iso(t) {
                    Some(t) => Some(t),
                    None => return false,
                },
            };
            let this = (when, removal, at);
            if newest.is_none_or(|n| this > n) {
                newest = Some(this);
            }
        }
        newest.is_some_and(|(_, removal, _)| !removal)
    })
}

/// A list the ask did not read: empty, possibly cut.
fn not_read<T>() -> Took<T> {
    took(Vec::new(), true)
}

fn took<T>(items: Vec<T>, more: bool) -> Took<T> {
    let mut took = Took::default();
    took.items = items;
    took.more = more;
    took
}

#[derive(Deserialize)]
struct Repo {
    issues: Issues,
}

/// A page of open issues. Each node is read on its own, so one that does
/// not read fails its item alone.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Issues {
    total_count: u64,
    page_info: Option<ticket::Info>,
    nodes: Vec<Option<serde_json::Value>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Count {
    total_count: u64,
}

/// One open issue as the page answers it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    #[serde(flatten)]
    node: Node,
    #[serde(flatten)]
    edit: Edited,
    #[serde(deserialize_with = "crate::ticket::present")]
    parent: Option<Named>,
    comments: Count,
    assignees: Page<Who>,
    blocked_by: Page<Named>,
    sub_issues: Page<Named>,
    mentions: Page<Event>,
    /// Asked only with a history.
    #[serde(default)]
    events: Option<Page<Event>>,
    /// Asked only with the changes.
    #[serde(default)]
    closed_by_pull_requests_references: Option<Counted<Pull>>,
}

impl GitHub {
    /// The open work, page by page: whole, or `Unread(Cut)` once the
    /// site's count or the issues read pass `bound`. Each issue's own
    /// failure fails it alone. The control ticket is `Tickets::ticket` at
    /// `Depth::All`. An ask this adapter does not answer is
    /// `Refused(Unsupported)` before any request.
    pub(super) fn open_work_pages(&mut self, ask: &BoardAsk, bound: Bound) -> Got<OpenWork> {
        let Some(kinds) = answered(ask) else {
            return super::super::unsupported();
        };
        let question = work_query(&page_kinds(&kinds), ask.changes.is_some());
        let mut items: Vec<(Option<i64>, u64, TicketKey, Got<WorkItem>)> = Vec::new();
        let mut seen = HashSet::new();
        let mut pages = Pages::default();
        let mut total = None;
        let mut cursor: Option<String> = None;
        loop {
            let out = self
                .ask("the open work", &question, None, cursor.as_deref())
                .map_err(|e| self.outcome(&e))?;
            let page = answer::<Repo>(&out)?.issues;
            if *total.get_or_insert(page.total_count) != page.total_count {
                return Err(INCONSISTENT);
            }
            if page.total_count > u64::from(bound.get()) {
                return Err(CUT);
            }
            for node in page.nodes {
                let node = node.ok_or(INCONSISTENT)?;
                let no = node
                    .get("number")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(NonZeroU64::new)
                    .ok_or(INCONSISTENT)?;
                if !seen.insert(no) {
                    return Err(INCONSISTENT);
                }
                let filed = node
                    .get("createdAt")
                    .and_then(serde_json::Value::as_str)
                    .and_then(tehas_core::time::from_iso);
                let key = TicketKey::Number(no);
                let item = match serde_json::from_value::<Item>(node) {
                    Ok(item) => self.work_of(&key, item, ask, &kinds),
                    Err(_) => Err(MALFORMED),
                };
                items.push((filed, no.get(), key, item));
            }
            if items.len() > bound.get() as usize {
                return Err(CUT);
            }
            match pages.next(page.page_info)? {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        if Some(items.len() as u64) != total {
            return Err(INCONSISTENT);
        }
        items.sort_by_key(|(filed, no, _, _)| (*filed, *no));
        let mut work = OpenWork::default();
        work.items = Whole::new(items.into_iter().map(|(_, _, k, i)| (k, i)).collect());
        work.control = ask
            .control
            .as_ref()
            .map(|key| GitHub::ticket(self, key, Depth::All, None));
        Ok(work)
    }

    /// One issue of the page as its work item.
    fn work_of(
        &mut self,
        key: &TicketKey,
        item: Item,
        ask: &BoardAsk,
        kinds: &[(HappenedKind, Window)],
    ) -> Got<WorkItem> {
        let Item {
            node,
            edit,
            parent,
            comments,
            assignees,
            blocked_by,
            sub_issues,
            mentions,
            events,
            closed_by_pull_requests_references: closing,
        } = item;
        let no = key.same_item().ok_or(INCONSISTENT)?;
        let issue = no.get();
        let carried: Vec<String> = node
            .labels
            .nodes
            .iter()
            .flatten()
            .map(|l| l.name.clone())
            .collect();
        let title = node.title.clone();
        let body = node.body.clone();
        let mut digest = self.digest_from(node)?;
        digest.parent = parent.map(|p| self.named(&p)).transpose()?;

        if blocked_by.total_count.ok_or(INCONSISTENT)? != blocked_by.nodes.len() as u64 {
            return Err(CUT);
        }
        let blockers = blocked_by
            .nodes
            .into_iter()
            .map(|n| self.blocker(&n.ok_or(INCONSISTENT)?))
            .collect::<Got<Vec<Blocker>>>()?;
        if assignees.total_count.ok_or(INCONSISTENT)? != assignees.nodes.len() as u64 {
            return Err(CUT);
        }
        let mut assigned = Vec::with_capacity(assignees.nodes.len());
        for who in assignees.nodes {
            let who = who.ok_or(INCONSISTENT)?;
            assigned.extend(who.account(self.bot.as_ref()));
        }
        let parts = self.parts_from(issue, sub_issues)?;
        let mentioned_by = self.mentions_from(issue, mentions)?;
        let history = self.history_of(key, issue, kinds, events, &carried)?;
        let changes = match ask.changes {
            None => not_read(),
            Some((_, window)) => self.closing(closing.ok_or(MALFORMED)?, window)?,
        };

        let read = markers::read(&body, Some(no));
        let mut blueprint = Blueprint::new(
            no,
            digest.created,
            text_revision(&title, &body).ok_or(MALFORMED)?,
            digest.link.clone(),
        );
        blueprint.title = title;
        blueprint.body = read.text.clone();
        blueprint.open = true;
        blueprint.author = digest.author.clone();
        (blueprint.edited, blueprint.editor) = edit.read(self.bot.as_ref())?;
        blueprint.markers = read.markers.clone();
        blueprint.comments.more = comments.total_count > 0;

        let mut ticket = Ticket::new(digest);
        ticket.text = read.text.chars().take(CUT_TEXT).collect();
        ticket.parts = parts;
        ticket.blockers = blockers;
        ticket.assignees = assigned;
        ticket.markers = read.markers;
        ticket.mentioned_by = mentioned_by;
        ticket.history = history;
        let mut work = WorkItem::new(ticket);
        work.notes = not_read();
        work.blueprints = vec![blueprint];
        work.changes = changes;
        Ok(work)
    }

    /// The history the ask's windows take, merged oldest first: from the
    /// page's events where they hold every event of the kinds asked and
    /// vouch for the marks the issue carries, else each kind read again
    /// (`history_apart`). None asked, none is read: empty, with `more`.
    fn history_of(
        &mut self,
        key: &TicketKey,
        issue: u64,
        kinds: &[(HappenedKind, Window)],
        page: Option<Page<Event>>,
        carried: &[String],
    ) -> Got<Took<TicketEvent>> {
        if kinds.is_empty() {
            return Ok(not_read());
        }
        let marks = kinds.iter().any(|(k, _)| MARKS.contains(k));
        let page = page.filter(|p| {
            let whole = p.page_info.as_ref().and_then(|i| i.has_previous_page) == Some(false);
            whole && (!marks || vouches(&p.nodes, carried))
        });
        let mut read = Vec::new();
        match page {
            Some(page) => {
                for e in page.nodes.into_iter().flatten() {
                    let Some(what) = self.happened(&e, None)? else {
                        continue;
                    };
                    let mut event = TicketEvent::new(what);
                    event.by = e.actor.as_ref().and_then(|w| w.account(self.bot.as_ref()));
                    event.at = e.created_at.as_deref().map(time).transpose()?;
                    read.push(event);
                }
                order(&mut read);
            }
            None => {
                let marks = match marks {
                    true => self.mark_events(issue, &MARKS)?,
                    false => Vec::new(),
                };
                return self.history_apart(key, kinds, marks);
            }
        }
        merge(&read, kinds)
    }

    /// Each kind read apart, as `Tickets::history` reads it: a mark's
    /// window of `marks`, the marks read whole once, and each other kind
    /// its own read, merged oldest first.
    fn history_apart(
        &mut self,
        key: &TicketKey,
        kinds: &[(HappenedKind, Window)],
        marks: Vec<TicketEvent>,
    ) -> Got<Took<TicketEvent>> {
        let mut each = Vec::new();
        for (kind, window) in kinds {
            each.push(match MARKS.contains(kind) {
                true => take(
                    marks
                        .iter()
                        .filter(|e| e.what.kind() == *kind)
                        .cloned()
                        .collect(),
                    *window,
                )?,
                false => self.history(key, *window, Some(std::slice::from_ref(kind)))?,
            });
        }
        let more = each.iter().any(|t| t.more);
        let mut items: Vec<TicketEvent> = each.into_iter().flat_map(|t| t.items).collect();
        order(&mut items);
        Ok(took(items, more))
    }

    /// The open changes of this project that GitHub links as closing the
    /// issue, in number order, as much as `window` takes: more than it, or
    /// more than the page read, is `Unread(Cut)`. One of another project is
    /// left out.
    fn closing(&mut self, refs: Counted<Pull>, window: Window) -> Got<Took<ChangeDigest>> {
        if refs.total_count > refs.nodes.len() as u64 {
            return Err(CUT);
        }
        if refs.total_count < refs.nodes.len() as u64 {
            return Err(INCONSISTENT);
        }
        let mut open = Vec::new();
        for pull in refs.nodes {
            let pull = pull.ok_or(INCONSISTENT)?;
            let no = pull.number.ok_or(INCONSISTENT)?;
            if !ends_with_folded(&pull.url, &format!("/{}/pull/{no}", self.repo)) {
                continue;
            }
            if pull.state != "OPEN" {
                continue;
            }
            open.push(self.pull_digest(pull, ChangeStates::Open)?);
        }
        open.sort_by_key(|c| c.no);
        if open.windows(2).any(|w| w[0].no == w[1].no) {
            return Err(INCONSISTENT);
        }
        let room = match window {
            Window::Whole(bound) => bound.get(),
            Window::Last(n) => n.get(),
        };
        if open.len() > room as usize {
            return Err(CUT);
        }
        let mut took = Took::default();
        took.items = open;
        Ok(took)
    }
}

/// Each kind's window of `read`, oldest first, merged oldest first, with
/// `more` when any window held more.
fn merge(read: &[TicketEvent], kinds: &[(HappenedKind, Window)]) -> Got<Took<TicketEvent>> {
    let mut items = Vec::new();
    let mut more = false;
    for (kind, window) in kinds {
        let mut took = take(
            read.iter()
                .filter(|e| e.what.kind() == *kind)
                .cloned()
                .collect(),
            *window,
        )?;
        more |= took.more;
        items.append(&mut took.items);
    }
    order(&mut items);
    Ok(took(items, more))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_asks_each_variable_it_declares_and_no_cursor_of_its_own() {
        for kinds in [vec![], PAGE_KINDS.to_vec(), vec![HappenedKind::Retitled]] {
            for changes in [false, true] {
                let q = work_query(&kinds, changes);
                assert_eq!(q.matches("$cursor").count(), 2, "{q}");
                assert!(q.contains("first: 25, after: $cursor"), "{q}");
                assert_eq!(
                    q.contains("closedByPullRequestsReferences(first: 20)"),
                    changes
                );
                assert_eq!(q.contains("events: timelineItems"), !kinds.is_empty());
            }
        }
        let q = work_query(&[HappenedKind::Marked, HappenedKind::Unmarked], false);
        assert!(
            q.contains("itemTypes: [LABELED_EVENT, UNLABELED_EVENT], last: 100)"),
            "{q}"
        );
    }

    #[test]
    fn either_mark_asks_both_of_the_page() {
        let w = Window::Whole(tehas_ports::ALL_MAX);
        assert_eq!(page_kinds(&[(HappenedKind::Marked, w)]), MARKS.to_vec());
        assert_eq!(
            page_kinds(&[(HappenedKind::Retitled, w)]),
            vec![HappenedKind::Retitled]
        );
    }
}
