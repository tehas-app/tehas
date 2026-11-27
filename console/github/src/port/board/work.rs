//! `Board::open_work`: the open issues in pages of 25, one GraphQL question
//! a page, each issue with what its work item holds, as dispatch's queue
//! page reads them. The issue is its own blueprint, read whole from the
//! same node; what waits on it, its parts, the changes that name it and
//! the ticket it is a part of are the same fragments `Tickets::ticket` and
//! `Tickets::list` ask, and its parts and mentions are paged on alone past
//! their first hundred, to GitHub's count of them.
//!
//! The marks are read whole. The page holds the newest hundred of the kinds
//! asked, with GitHub's count of them; where that is not every one, or where
//! a mark the issue carries is not applied by the newest event of it the
//! page holds, GitHub cut the timeline short, and each kind is read again on
//! its own, as `Tickets::history` reads it. The marks read again must then
//! vouch for the marks the issue carries, or the item is
//! `Unread(Inconsistent)`.
//!
//! The changes are those GitHub links as closing the issue, from this
//! project, open ones alone, each a whole change's digest read as
//! `open_changes` reads it: the ten a page asks for, past them, or past
//! a change's bound of marks or of what it closes, the item is
//! `Unread(Cut)`.
//!
//! A GraphQL error under one issue's node fails that item alone where the
//! node itself is still there; a null node, which GitHub answers where the
//! issue could not be read at all, has no number to fail and fails the
//! read, as does any error elsewhere.

use std::collections::{HashMap, HashSet};
use std::num::{NonZeroU32, NonZeroU64};

use serde::Deserialize;

use super::super::blueprint_writes::text_revision;
use super::super::blueprints::Edited;
use super::{pull_fields, Counted, Pull};
use crate::graphql::{Error, Step};
use crate::ticket::{
    self, order, time, Event, Named, Node, Page, Pages, Who, ACTOR, CUT, CUT_TEXT, INCONSISTENT,
    MALFORMED,
};
use crate::tickets::take;
use crate::{markers, GitHub};
use tehas_ports::{
    Blueprint, BoardAsk, Bound, ChangeDigest, ChangeNo, ChangeStates, Depth, Filter, Got, Happened,
    HappenedKind, Mark, MarkId, OpenWork, Outcome, Ticket, TicketEvent, TicketKey, Took, Unread,
    Whole, Window, WorkItem,
};

/// The open issues one page asks for, dispatch's `ISSUE_PAGE`. A page over
/// GitHub's budget for nested items comes back with its timelines cut
/// short and no error, so the size is load-bearing.
const ISSUE_PAGE: usize = 25;

/// The changes that close an issue a page asks for. Each is a whole
/// change's digest, its marks, what it closes and its retitle counted per
/// change, so this bound is most of the page's cost: ten is about 9 points
/// a page of 25, twenty about 17 (measured 2026-11-24). An issue with more
/// is `Unread(Cut)`.
const CLOSING_MAX: usize = 10;

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
        "{} lastEditedAt editor {{ {ACTOR} }} comments {{ totalCount }} assignees(first: {ASSIGNEES_MAX}) {{ totalCount nodes {{ __typename login databaseId }} }} {} {} {}",
        ticket::digest_fields(),
        ticket::blocked_by(false),
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
            " closedByPullRequestsReferences(first: {CLOSING_MAX}) {{ totalCount nodes {{ {} repository {{ databaseId nameWithOwner }} }} }}",
            pull_fields()
        ));
    }
    format!("query($owner: String!, $name: String!, $cursor: String) {{ repository(owner: $owner, name: $name) {{ databaseId issues(states: [OPEN], first: {ISSUE_PAGE}, after: $cursor, orderBy: {{field: CREATED_AT, direction: ASC}}) {{ totalCount pageInfo {{ hasNextPage endCursor }} nodes {{ {node} }} }} }} }}")
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
/// application. Of two in one second, or two with no time, the removal is
/// the newer, an event with no time is older than any, and of two alike
/// the later in the answer is the newer, as `label_state` orders them. A time that does
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

/// Whether the marks read again vouch for the marks the issue carries, by
/// the same rule as [`vouches`]: the newest event of each, in the order of
/// `read`, oldest first, is its application.
fn reread_vouches(read: &[TicketEvent], carried: &[Mark]) -> bool {
    carried.iter().all(|mark| {
        read.iter()
            .rev()
            .find_map(|e| match &e.what {
                Happened::Marked(m) if m == mark => Some(true),
                Happened::Unmarked(m) if m == mark => Some(false),
                _ => None,
            })
            .unwrap_or(false)
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

/// A page's answer: its data, and the errors that came with it.
#[derive(Deserialize)]
struct PageAnswer {
    data: Option<PageData>,
    #[serde(default)]
    errors: Vec<Error>,
}

#[derive(Deserialize)]
struct PageData {
    repository: Option<Repo>,
}

#[derive(Deserialize)]
struct Repo {
    issues: Issues,
}

/// The issue of the page an error lies under, by its place in the page.
fn item_of(e: &Error) -> Option<usize> {
    match e.path.as_slice() {
        [Step::Name(r), Step::Name(i), Step::Name(n), Step::At(at), ..]
            if r == "repository" && i == "issues" && n == "nodes" =>
        {
            usize::try_from(*at).ok()
        }
        _ => None,
    }
}

/// What an error fails its item with.
fn failed_by(e: &Error) -> Outcome {
    match e.is("FORBIDDEN") {
        true => Outcome::Forbidden,
        false => INCONSISTENT,
    }
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

/// One issue of the pages, before it is read: its number, when it was
/// filed, and its node or the error GitHub answered under it.
struct Raw {
    no: NonZeroU64,
    filed: Option<i64>,
    node: Got<serde_json::Value>,
}

impl GitHub {
    /// The open work, page by page: whole, or `Unread(Cut)` once the
    /// site's count passes `bound`, with no item read. A count that moves
    /// between pages reads the pages once more, then fails. Each issue's
    /// own failure fails it alone, where its node is there to fail: a null
    /// node fails the read. The control ticket is `Tickets::ticket` at
    /// `Depth::All` whatever the ask, two calls. An ask this adapter does
    /// not answer is `Refused(Unsupported)` before any request.
    pub(super) fn open_work_pages(&mut self, ask: &BoardAsk, bound: Bound) -> Got<OpenWork> {
        let Some(kinds) = answered(ask) else {
            return super::super::unsupported();
        };
        let question = work_query(&page_kinds(&kinds), ask.changes.is_some());
        let raw = match self.issue_pages(&question, bound)? {
            Some(raw) => raw,
            None => self.issue_pages(&question, bound)?.ok_or(INCONSISTENT)?,
        };
        let mut items: Vec<(Option<i64>, u64, TicketKey, Got<WorkItem>)> = Vec::new();
        for Raw { no, filed, node } in raw {
            let key = TicketKey::Number(no);
            let item = node
                .and_then(|n| serde_json::from_value::<Item>(n).map_err(|_| MALFORMED))
                .and_then(|i| self.work_of(&key, i, ask, &kinds));
            items.push((filed, no.get(), key, item));
        }
        items.sort_by_key(|(filed, no, _, _)| (*filed, *no));
        let mut work = OpenWork::default();
        // The control ticket is read whole whatever the ask (Decided 120).
        work.control = ask
            .control
            .as_ref()
            .map(|key| GitHub::ticket(self, key, Depth::All, None));
        work.items = Whole::new(items.into_iter().map(|(_, _, k, i)| (k, i)).collect());
        Ok(work)
    }

    /// Every page of open issues, each node kept apart: None when GitHub's
    /// count moved between two pages. More than `bound` is `Unread(Cut)`
    /// on the first page that says so.
    fn issue_pages(&mut self, question: &str, bound: Bound) -> Got<Option<Vec<Raw>>> {
        let mut raw = Vec::new();
        let mut seen = HashSet::new();
        let mut pages = Pages::default();
        let mut total = None;
        let mut cursor: Option<String> = None;
        loop {
            let (page, mut failed) = self.issue_page(question, cursor.as_deref())?;
            if *total.get_or_insert(page.total_count) != page.total_count {
                return Ok(None);
            }
            if page.total_count > u64::from(bound.get()) {
                return Err(CUT);
            }
            for (at, node) in page.nodes.into_iter().enumerate() {
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
                let node = match failed.remove(&at) {
                    Some(outcome) => Err(outcome),
                    None => Ok(node),
                };
                raw.push(Raw { no, filed, node });
            }
            if !failed.is_empty() {
                // An error under a node the page did not give.
                return Err(INCONSISTENT);
            }
            match pages.next(page.page_info)? {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        if Some(raw.len() as u64) != total {
            return Err(INCONSISTENT);
        }
        Ok(Some(raw))
    }

    /// One page of open issues, and the outcome of each issue an error lies
    /// under, by its place in the page. gh exits 1 on any error; where the
    /// errors all lie under issues, the page's data still answers the rest.
    /// Any other error fails the read with the call's outcome.
    fn issue_page(
        &mut self,
        question: &str,
        cursor: Option<&str>,
    ) -> Got<(Issues, HashMap<usize, Outcome>)> {
        let (out, failed) = match self.ask_answer("the open work", question, cursor) {
            Ok(out) => (out, None),
            Err((e, Some(out))) => (out, Some(self.outcome(&e))),
            Err((e, None)) => return Err(self.outcome(&e)),
        };
        let fail = |or: Outcome| failed.clone().unwrap_or(or);
        let read = || -> Option<PageAnswer> {
            let mut v: serde_json::Value = serde_json::from_str(out.trim()).ok()?;
            tehas_core::accounts::vouch(&mut v);
            serde_json::from_value(v).ok()
        };
        let answer = read().ok_or_else(|| fail(MALFORMED))?;
        if failed.is_some() && answer.errors.is_empty() {
            return Err(fail(MALFORMED));
        }
        let mut items = HashMap::new();
        for error in &answer.errors {
            let at = item_of(error).ok_or_else(|| fail(INCONSISTENT))?;
            items.entry(at).or_insert_with(|| failed_by(error));
        }
        let page = answer
            .data
            .and_then(|d| d.repository)
            .ok_or_else(|| fail(MALFORMED))?
            .issues;
        Ok((page, items))
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
        // The marks the ask reads no events of stand aside: the page
        // vouches for the ticket without them, and they are not read
        // again apart. The control ticket is asked about whatever it
        // carries, as its own read is.
        let is_control = ask.control.as_ref() == Some(key);
        let mut carried: Vec<String> = Vec::new();
        for l in node.labels.nodes.iter().flatten() {
            if !is_control && self.is_not_read(&l.name, &ask.marks_not_read) {
                continue;
            }
            carried.push(l.name.clone());
        }
        let title = node.title.clone();
        let body = node.body.clone();
        let digest = self.digest_from(node)?;

        let blockers = self.blockers_from(blocked_by)?;
        if assignees.total_count.ok_or(INCONSISTENT)? != assignees.nodes.len() as u64 {
            return Err(CUT);
        }
        let mut assigned = Vec::with_capacity(assignees.nodes.len());
        for who in assignees.nodes {
            let who = who.ok_or(INCONSISTENT)?;
            assigned.extend(who.account(self.bot.as_ref()));
        }
        let parts = self.parts_from(issue, sub_issues)?;
        if mentions.total_count.is_none() {
            return Err(MALFORMED);
        }
        let mentioned_by = self.mentions_from(issue, mentions)?;
        let marks: Vec<Mark> = if is_control {
            digest.marks.clone()
        } else {
            digest
                .marks
                .iter()
                .filter(|m| !ask.marks_not_read.contains(m))
                .cloned()
                .collect()
        };
        let history = self.history_of(key, issue, kinds, events, &carried, &marks)?;
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
        marked: &[Mark],
    ) -> Got<Took<TicketEvent>> {
        if kinds.is_empty() {
            return Ok(not_read());
        }
        let marks = kinds.iter().any(|(k, _)| MARKS.contains(k));
        let page = page.filter(|p| {
            let whole = p.page_info.as_ref().and_then(|i| i.has_previous_page) == Some(false)
                && p.total_count == Some(p.nodes.len() as u64);
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
                    event.mark_id = e.label.as_ref().and_then(|l| l.id.clone()).map(MarkId::new);
                    read.push(event);
                }
                order(&mut read);
            }
            None => {
                let read = match marks {
                    true => self.mark_events(issue, &MARKS)?,
                    false => Vec::new(),
                };
                if marks && !reread_vouches(&read, marked) {
                    return Err(INCONSISTENT);
                }
                return self.history_apart(key, kinds, read);
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
            pull.number.and_then(ChangeNo::new).ok_or(INCONSISTENT)?;
            let repo = pull.repository.as_ref().ok_or(MALFORMED)?;
            if !self.is_here_id(repo.database_id)? {
                continue;
            }
            if pull.state != "OPEN" {
                continue;
            }
            // A change past its bound of marks or of what it closes is one
            // this read did not take whole: the item's, not the read's.
            open.push(match self.pull_digest(pull, ChangeStates::Open) {
                Err(Outcome::Unread(Unread::TooLarge)) => return Err(CUT),
                other => other?,
            });
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
        Ok(took(open, false))
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
                    q.contains("closedByPullRequestsReferences(first: 10)"),
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

    fn event(kind: &str, label: &str, at: Option<&str>) -> Option<Event> {
        Some(
            serde_json::from_value(serde_json::json!({
                "__typename": kind, "createdAt": at, "actor": null, "label": {"name": label},
            }))
            .unwrap(),
        )
    }

    fn on(label: &str, at: Option<&str>) -> Option<Event> {
        event("LabeledEvent", label, at)
    }

    fn off(label: &str, at: Option<&str>) -> Option<Event> {
        event("UnlabeledEvent", label, at)
    }

    const READY: &str = "ready";

    fn carried() -> Vec<String> {
        vec![READY.to_string()]
    }

    #[test]
    fn the_newest_event_of_each_carried_label_must_apply_it() {
        let t1 = Some("2026-01-01T00:00:01Z");
        let t2 = Some("2026-01-01T00:00:02Z");
        assert!(vouches(&[on(READY, t1)], &carried()));
        assert!(vouches(&[], &[]));
        // No event of it at all, or a removal newest.
        assert!(!vouches(&[on("other", t1)], &carried()));
        assert!(!vouches(&[on(READY, t1), off(READY, t2)], &carried()));
        // Applied again after a removal.
        assert!(vouches(&[off(READY, t1), on(READY, t2)], &carried()));
        // In one second the removal is the newer, whichever the page gave
        // first.
        assert!(!vouches(&[off(READY, t1), on(READY, t1)], &carried()));
        assert!(!vouches(&[on(READY, t1), off(READY, t1)], &carried()));
        // An event with no time is older than any.
        assert!(!vouches(&[off(READY, t1), on(READY, None)], &carried()));
        assert!(vouches(&[off(READY, None), on(READY, t1)], &carried()));
        // Two with no time share it: the removal is the newer.
        assert!(!vouches(&[off(READY, None), on(READY, None)], &carried()));
        // Two applications in one second: the later in the answer.
        assert!(vouches(&[on(READY, t1), on(READY, t1)], &carried()));
        // A time that does not read vouches for nothing.
        assert!(!vouches(&[on(READY, Some("yesterday"))], &carried()));
        assert!(!vouches(
            &[off(READY, Some("2026-13-01T00:00:00Z")), on(READY, t2)],
            &carried()
        ));
        // A node the page left out vouches for nothing.
        assert!(!vouches(&[None, on(READY, t1)], &carried()));
        // A retitle is no event of a label.
        let renamed: Option<Event> = Some(
            serde_json::from_value(
                serde_json::json!({"__typename": "RenamedTitleEvent", "createdAt": t2}),
            )
            .unwrap(),
        );
        assert!(vouches(&[on(READY, t1), renamed], &carried()));
    }

    #[test]
    fn the_marks_read_again_must_apply_each_carried_mark() {
        let ready = Mark::Act(tehas_ports::Act::Approve);
        let at = |what| TicketEvent::new(what);
        assert!(reread_vouches(
            &[at(Happened::Marked(ready.clone()))],
            std::slice::from_ref(&ready)
        ));
        assert!(!reread_vouches(&[], std::slice::from_ref(&ready)));
        assert!(!reread_vouches(
            &[
                at(Happened::Marked(ready.clone())),
                at(Happened::Unmarked(ready.clone()))
            ],
            std::slice::from_ref(&ready)
        ));
        assert!(reread_vouches(&[], &[]));
    }
}
