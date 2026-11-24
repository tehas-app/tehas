//! The tickets port's reads past an issue's digest: one ticket to the depth
//! asked, a history of every kind of event, a ticket's parts, and the list
//! a filter takes. Each is a GraphQL question paged here, to the whole or
//! one past the bound, and answered in the ports' types.
//!
//! gh says nothing of a question GitHub refused but that it failed, so a
//! question about one issue that fails that way is settled by the issue's
//! REST read: not there, or a pull request, is `NotFound`.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};

use crate::tickets::{account, number, take, Bot};
use crate::{gh, markers, table, ForgeError, GitHub};
use tehas_ports::{
    Account, At, Blocker, BlockerState, Bound, ChangeNo, CloseReason, Closer, CommitId, Depth,
    Filter, Got, Happened, HappenedKind, Item, ItemName, Outcome, Part, ProjectName, Ref, Refusal,
    Revision, TargetId, TargetName, Ticket, TicketDigest, TicketEvent, TicketKey, TicketState,
    Took, Unread, WebLink, Whole, Window, ALL_MAX,
};

const MALFORMED: Outcome = Outcome::Unread(Unread::Malformed);
const INCONSISTENT: Outcome = Outcome::Unread(Unread::Inconsistent);
const CUT: Outcome = Outcome::Unread(Unread::Cut);

/// How many characters of a ticket's text `Depth::Cut` keeps, as the ports'
/// fake cuts it.
pub const CUT_TEXT: usize = 200;

/// The most pages one read asks for after its first, a hundred items each.
/// A list longer than that is `Unread(Cut)`.
const PAGES_MAX: usize = 100;

/// An account, with the user id the vouch reads beside every login.
const ACTOR: &str = "__typename login ...on User { databaseId } ...on Bot { databaseId }";

const ASSIGNEE: &str =
    "assignee { __typename ...on User { login databaseId } ...on Bot { login databaseId } }";

const CLOSER: &str = "stateReason closer { __typename ...on PullRequest { number merged repository { nameWithOwner } } ...on Commit { oid } }";

const BLOCKING: &str = "blockingIssue { number repository { nameWithOwner } }";

/// An issue another names: a part, a blocker.
const NAMED: &str = "number state repository { nameWithOwner }";

/// Each kind of event the history reads: the ports' kind, GitHub's item
/// type, its node, and the fields asked of it beside its time and actor.
const EVENTS: [(HappenedKind, &str, &str, &str); 14] = [
    (
        HappenedKind::Marked,
        "LABELED_EVENT",
        "LabeledEvent",
        "label { name }",
    ),
    (
        HappenedKind::Unmarked,
        "UNLABELED_EVENT",
        "UnlabeledEvent",
        "label { name }",
    ),
    (
        HappenedKind::Assigned,
        "ASSIGNED_EVENT",
        "AssignedEvent",
        ASSIGNEE,
    ),
    (
        HappenedKind::Unassigned,
        "UNASSIGNED_EVENT",
        "UnassignedEvent",
        ASSIGNEE,
    ),
    (
        HappenedKind::Targeted,
        "MILESTONED_EVENT",
        "MilestonedEvent",
        "milestoneTitle",
    ),
    (
        HappenedKind::Targeted,
        "DEMILESTONED_EVENT",
        "DemilestonedEvent",
        "milestoneTitle",
    ),
    (
        HappenedKind::Retitled,
        "RENAMED_TITLE_EVENT",
        "RenamedTitleEvent",
        "previousTitle currentTitle",
    ),
    (HappenedKind::Closed, "CLOSED_EVENT", "ClosedEvent", CLOSER),
    (
        HappenedKind::Reopened,
        "REOPENED_EVENT",
        "ReopenedEvent",
        "",
    ),
    (
        HappenedKind::Kind,
        "ISSUE_TYPE_ADDED_EVENT",
        "IssueTypeAddedEvent",
        "issueType { name }",
    ),
    (
        HappenedKind::Kind,
        "ISSUE_TYPE_CHANGED_EVENT",
        "IssueTypeChangedEvent",
        "issueType { name }",
    ),
    (
        HappenedKind::Kind,
        "ISSUE_TYPE_REMOVED_EVENT",
        "IssueTypeRemovedEvent",
        "",
    ),
    (
        HappenedKind::BlockerAdded,
        "BLOCKED_BY_ADDED_EVENT",
        "BlockedByAddedEvent",
        BLOCKING,
    ),
    (
        HappenedKind::BlockerRemoved,
        "BLOCKED_BY_REMOVED_EVENT",
        "BlockedByRemovedEvent",
        BLOCKING,
    ),
];

/// The events that read as `Happened::Unknown` when GitHub leaves out what
/// they name: an account that is gone, a milestone not the issue's now, an
/// issue type or a blocking issue that is gone.
const MAY_BE_UNKNOWN: [&str; 7] = [
    "AssignedEvent",
    "UnassignedEvent",
    "MilestonedEvent",
    "IssueTypeAddedEvent",
    "IssueTypeChangedEvent",
    "BlockedByAddedEvent",
    "BlockedByRemovedEvent",
];

/// A question of one issue of the repository. `asks` uses `$cursor`.
fn of_issue(asks: &str) -> String {
    format!("query($owner: String!, $name: String!, $number: Int!, $cursor: String) {{ repository(owner: $owner, name: $name) {{ issue(number: $number) {{ number {asks} }} }} }}")
}

/// The events of the kinds asked, a page of them: the oldest first for a
/// whole read, the newest for the last n. Empty when GitHub has no event of
/// those kinds. A milestone's event names its title alone, so the issue's
/// milestone is asked beside it; one this read cannot name is `Unknown`.
/// `Unknown` asks every event that can read as it.
fn events(kinds: Option<&[HappenedKind]>, window: Window) -> String {
    let asked = |(k, _, node, _): &&(HappenedKind, &str, &str, &str)| {
        kinds.is_none_or(|ks| {
            ks.contains(k) || (ks.contains(&HappenedKind::Unknown) && MAY_BE_UNKNOWN.contains(node))
        })
    };
    let rows: Vec<_> = EVENTS.iter().filter(asked).collect();
    if rows.is_empty() {
        return String::new();
    }
    let types: Vec<&str> = rows.iter().map(|r| r.1).collect();
    let nodes: Vec<String> = rows
        .iter()
        .map(|(_, _, node, fields)| {
            format!("... on {node} {{ createdAt actor {{ {ACTOR} }} {fields} }}")
        })
        .collect();
    let (page, info) = match window {
        Window::Whole(_) => ("first: 100, after: $cursor", "hasNextPage endCursor"),
        Window::Last(_) => ("last: 100, before: $cursor", "hasPreviousPage startCursor"),
    };
    format!(
        "milestone {{ number title }} events: timelineItems(itemTypes: [{}], {page}) {{ pageInfo {{ {info} }} nodes {{ __typename {} }} }}",
        types.join(", "),
        nodes.join(" ")
    )
}

/// The changes that name the issue, a page of them.
fn mentions() -> String {
    "mentions: timelineItems(itemTypes: [CROSS_REFERENCED_EVENT], first: 100, after: $cursor) { pageInfo { hasNextPage endCursor } nodes { __typename ... on CrossReferencedEvent { source { __typename ...on PullRequest { number repository { nameWithOwner } } } } } }".to_string()
}

/// The issue's parts, a page of them.
fn parts() -> String {
    format!("subIssues(first: 100, after: $cursor) {{ totalCount pageInfo {{ hasNextPage endCursor }} nodes {{ {NAMED} }} }}")
}

/// The issue's newest close, what `Ticket.closer` is read from.
pub(crate) fn closes() -> String {
    format!("closes: timelineItems(itemTypes: [CLOSED_EVENT], last: 1) {{ nodes {{ __typename ... on ClosedEvent {{ createdAt actor {{ {ACTOR} }} {CLOSER} }} }} }}")
}

/// What a ticket holds beside its digest and its history: its newest
/// close, what it waits on, its parts and the changes that name it.
fn beside() -> String {
    format!(
        "{} blockedBy(first: 100) {{ totalCount nodes {{ {NAMED} }} }} {} {}",
        closes(),
        parts(),
        mentions()
    )
}

/// What an issue's digest is read from, a [`Node`].
pub(crate) fn digest_fields() -> String {
    format!("number title body state stateReason url createdAt updatedAt author {{ {ACTOR} }} issueType {{ name }} milestone {{ number title }} labels(first: 100) {{ totalCount nodes {{ name }} }}")
}

/// The list question: the repository's issues, oldest filed first, a page
/// of them, each with what its digest holds. `narrow` is GitHub's filter,
/// which only narrows: every ticket is held to the caller's filter again.
fn listed(narrow: &str) -> String {
    format!("query($owner: String!, $name: String!, $cursor: String) {{ repository(owner: $owner, name: $name) {{ issues(first: 100, after: $cursor, orderBy: {{field: CREATED_AT, direction: ASC}}{narrow}) {{ pageInfo {{ hasNextPage endCursor }} nodes {{ {} }} }} }} }}", digest_fields())
}

#[derive(Deserialize)]
struct Who {
    #[serde(rename = "__typename")]
    is: Option<String>,
    login: Option<String>,
    #[serde(rename = "databaseId")]
    id: Option<u64>,
}

impl Who {
    fn account(&self, bot: Option<&Bot>) -> Option<Account> {
        let facts = gh::Facts {
            id: self.id,
            kind: self.is.clone(),
        };
        account(self.login.as_deref()?, &facts, bot)
    }
}

#[derive(Deserialize)]
struct Name {
    name: String,
}

#[derive(Deserialize)]
struct Milestone {
    number: u64,
    title: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Repo {
    name_with_owner: String,
}

/// An issue or a pull request another item names.
#[derive(Deserialize)]
struct Named {
    #[serde(rename = "__typename")]
    is: Option<String>,
    number: Option<u64>,
    state: Option<String>,
    merged: Option<bool>,
    oid: Option<String>,
    repository: Option<Repo>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Info {
    has_next_page: Option<bool>,
    end_cursor: Option<String>,
    has_previous_page: Option<bool>,
    start_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Page<T> {
    total_count: Option<u64>,
    pub(crate) page_info: Option<Info>,
    pub(crate) nodes: Vec<Option<T>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Event {
    #[serde(rename = "__typename")]
    is: String,
    created_at: Option<String>,
    actor: Option<Who>,
    label: Option<Name>,
    assignee: Option<Who>,
    milestone_title: Option<String>,
    previous_title: Option<String>,
    current_title: Option<String>,
    state_reason: Option<String>,
    /// None where the answer holds no closer at all, which a close's never
    /// does: an empty one is a close by hand.
    #[serde(default, deserialize_with = "given")]
    closer: Option<Option<Named>>,
    issue_type: Option<Name>,
    blocking_issue: Option<Named>,
    source: Option<Named>,
}

/// One issue's answer: the parts of it the question asked.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Answer {
    milestone: Option<Milestone>,
    events: Option<Page<Event>>,
    closes: Option<Page<Event>>,
    mentions: Option<Page<Event>>,
    sub_issues: Option<Page<Named>>,
    blocked_by: Option<Page<Named>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Node {
    pub(crate) number: Option<u64>,
    title: String,
    /// Asked for the revision alone, which sums the title and the body.
    body: String,
    state: String,
    state_reason: Option<String>,
    url: String,
    created_at: String,
    updated_at: String,
    author: Option<Who>,
    issue_type: Option<Name>,
    milestone: Option<Milestone>,
    labels: Page<Name>,
}

/// What the issue's REST read holds beside its digest. Each is there in
/// every issue, empty or not: an answer without one is no issue.
#[derive(Deserialize)]
struct Rest {
    #[serde(deserialize_with = "present")]
    body: Option<String>,
    assignees: Vec<RestUser>,
    #[serde(deserialize_with = "present")]
    closed_at: Option<String>,
}

/// A field that may be empty and may not be left out.
pub(crate) fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(d)
}

/// A field that says whether it was there: `Some(None)` for an empty one.
fn given<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

#[derive(Deserialize)]
struct RestUser {
    login: Option<String>,
    id: Option<u64>,
    #[serde(rename = "type")]
    kind: Option<String>,
}

/// The part of an answer at `at`, its accounts vouched, in the type asked.
/// An answer that is not that is `Unread(Malformed)`.
pub(crate) fn part<T: DeserializeOwned>(out: &str, at: &str) -> Got<T> {
    let mut all: serde_json::Value = serde_json::from_str(out.trim()).map_err(|_| MALFORMED)?;
    tehas_core::accounts::vouch(&mut all);
    let part = all.pointer_mut(at).map(std::mem::take).ok_or(MALFORMED)?;
    serde_json::from_value(part).map_err(|_| MALFORMED)
}

pub(crate) fn time(iso: &str) -> Got<At> {
    tehas_core::time::from_iso(iso)
        .map(At::new)
        .ok_or(MALFORMED)
}

/// The pages of one list, by the cursors GitHub hands back.
#[derive(Default)]
pub(crate) struct Pages {
    seen: Vec<String>,
    back: bool,
}

impl Pages {
    /// The cursor of the page after this one, None after the last. A page
    /// that does not say whether there is more, or says so with no cursor
    /// or one already used, is `Unread(Inconsistent)`.
    pub(crate) fn next(&mut self, info: Option<Info>) -> Got<Option<String>> {
        let info = info.ok_or(INCONSISTENT)?;
        let (more, cursor) = match self.back {
            true => (info.has_previous_page, info.start_cursor),
            false => (info.has_next_page, info.end_cursor),
        };
        if !more.ok_or(INCONSISTENT)? {
            return Ok(None);
        }
        let cursor = cursor.ok_or(INCONSISTENT)?;
        if self.seen.contains(&cursor) {
            return Err(INCONSISTENT);
        }
        if self.seen.len() >= PAGES_MAX {
            return Err(CUT);
        }
        self.seen.push(cursor.clone());
        Ok(Some(cursor))
    }
}

/// Oldest first; of two events in one second an addition before a removal,
/// and of two of one kind the order of the site's answer.
fn order(events: &mut [TicketEvent]) {
    let removal = |e: &TicketEvent| {
        matches!(
            e.what,
            Happened::Unmarked(_) | Happened::Unassigned(_) | Happened::BlockerRemoved(_)
        )
    };
    events.sort_by(|a, b| a.at.cmp(&b.at).then_with(|| removal(a).cmp(&removal(b))));
}

fn reason(word: Option<&str>) -> CloseReason {
    match word {
        // No original of a duplicate is asked: it reads as the digest's.
        Some("NOT_PLANNED" | "DUPLICATE") => CloseReason::NotPlanned,
        _ => CloseReason::Done,
    }
}

impl GitHub {
    /// One ticket, to the depth asked: the issue's REST read for its
    /// digest, text, assignees and close, one question for what stands
    /// beside them with the first page of its history, and a question more
    /// for each page past the first.
    pub(crate) fn ticket(
        &mut self,
        key: &TicketKey,
        depth: Depth,
        history: Option<Window>,
    ) -> Got<Ticket> {
        let issue = number(key)?;
        let out = self.issue_answer(issue).map_err(|e| self.outcome(&e))?;
        let digest = self.digest_of(key, &out)?;
        let rest: Rest = part(&out, "")?;
        let window = history.or(depth.history());
        let asks = format!(
            "{} {}",
            beside(),
            window.map(|w| events(None, w)).unwrap_or_default()
        );
        let mut first: Answer = self.ask_issue(issue, &of_issue(&asks), None, false)?;

        let closer = self.newest_closer(&digest.state, first.closes.take())?;
        let blocked = first.blocked_by.take().ok_or(MALFORMED)?;
        if blocked.total_count.ok_or(INCONSISTENT)? != blocked.nodes.len() as u64 {
            return Err(CUT);
        }
        let blockers = blocked
            .nodes
            .into_iter()
            .map(|n| self.blocker(&n.ok_or(INCONSISTENT)?))
            .collect::<Got<Vec<Blocker>>>()?;
        let subs = first.sub_issues.take().ok_or(MALFORMED)?;
        let parts = self.parts_from(issue, subs)?;
        let named = first.mentions.take().ok_or(MALFORMED)?;
        let mentioned_by = self.mentions_from(issue, named)?;
        let history = match window {
            None => {
                let mut cut = Took::default();
                cut.more = true;
                cut
            }
            Some(w) => self.pages(issue, None, w, Some(first), false)?,
        };

        let read = markers::read(rest.body.as_deref().unwrap_or_default(), key.same_item());
        let text = match depth {
            Depth::Cut { .. } => read.text.chars().take(CUT_TEXT).collect(),
            _ => read.text,
        };
        let closed = rest.closed_at.as_deref().map(time).transpose()?;
        let mut ticket = Ticket::new(digest);
        ticket.text = text;
        ticket.closed = closed;
        ticket.closer = closer;
        ticket.parts = parts;
        ticket.blockers = blockers;
        ticket.assignees = rest
            .assignees
            .iter()
            .filter_map(|u| {
                let facts = gh::Facts {
                    id: u.id,
                    kind: u.kind.clone(),
                };
                account(u.login.as_deref()?, &facts, self.bot.as_ref())
            })
            .collect();
        ticket.markers = read.markers;
        ticket.mentioned_by = mentioned_by;
        ticket.history = history;
        Ok(ticket)
    }

    /// A ticket's history of the kinds asked, or of every kind: the
    /// `Tickets::history` of a read past the marks.
    pub(crate) fn timeline(
        &mut self,
        key: &TicketKey,
        window: Window,
        only: Option<&[HappenedKind]>,
    ) -> Got<Took<TicketEvent>> {
        let issue = number(key)?;
        if events(only, window).is_empty() {
            // GitHub keeps no event of these kinds: the issue's read alone
            // says whether it is there.
            let out = self.issue_answer(issue).map_err(|e| self.outcome(&e))?;
            if gh::is_pull(&out) {
                return Err(Outcome::NotFound);
            }
            part::<Rest>(&out, "")?;
            return take(Vec::new(), window);
        }
        self.pages(issue, only, window, None, true)
    }

    /// A ticket's parts, whole: `Tickets::parts`.
    pub(crate) fn parts(&mut self, key: &TicketKey) -> Got<Whole<Part>> {
        let issue = number(key)?;
        let first: Answer = self.ask_issue(issue, &of_issue(&parts()), None, true)?;
        let subs = first.sub_issues.ok_or(MALFORMED)?;
        Ok(Whole::new(self.parts_from(issue, subs)?))
    }

    /// The tickets the filter takes, whole, oldest filed first:
    /// `Tickets::list`. More than `bound` is `Unread(Cut)`.
    pub(crate) fn list(&mut self, filter: &Filter, bound: Bound) -> Got<Whole<TicketDigest>> {
        let invalid = Outcome::Refused(Refusal::Invalid);
        let mut narrow = Vec::new();
        match filter.open {
            Some(true) => narrow.push("states: [OPEN]".to_string()),
            Some(false) => narrow.push("states: [CLOSED]".to_string()),
            None => {}
        }
        if !filter.carrying.is_empty() {
            let labels = filter
                .carrying
                .iter()
                .map(|m| {
                    let label = table::label(m).ok_or(invalid.clone())?;
                    serde_json::to_string(&label).map_err(|_| invalid.clone())
                })
                .collect::<Got<Vec<String>>>()?;
            narrow.push(format!("labels: [{}]", labels.join(", ")));
        }
        if let Some(target) = &filter.target {
            let no = target.as_str();
            if no.is_empty() || !no.bytes().all(|b| b.is_ascii_digit()) {
                return Err(invalid);
            }
            narrow.push(format!("milestoneNumber: \"{no}\""));
        }
        let narrow = match narrow.is_empty() {
            true => String::new(),
            false => format!(", filterBy: {{{}}}", narrow.join(", ")),
        };
        let question = listed(&narrow);

        let mut all: Vec<TicketDigest> = Vec::new();
        let mut pages = Pages::default();
        let mut cursor: Option<String> = None;
        loop {
            let out = self
                .ask("the list of issues", &question, None, cursor.as_deref())
                .map_err(|e| self.outcome(&e))?;
            let page: Page<Node> = part(&out, "/data/repository/issues")?;
            for node in page.nodes {
                let digest = self.digest_from(node.ok_or(INCONSISTENT)?)?;
                if all.iter().any(|d| d.key == digest.key) {
                    return Err(INCONSISTENT);
                }
                let takes = filter
                    .open
                    .is_none_or(|o| o == (digest.state == TicketState::Open))
                    && filter.carrying.iter().all(|m| digest.marks.contains(m))
                    && (filter.carrying_any.is_empty()
                        || filter.carrying_any.iter().any(|m| digest.marks.contains(m)))
                    && filter
                        .kind
                        .as_ref()
                        .is_none_or(|k| digest.kind.as_ref() == Some(k))
                    && filter
                        .target
                        .as_ref()
                        .is_none_or(|t| digest.target.as_ref().is_some_and(|d| d.id == *t));
                if takes {
                    all.push(digest);
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
            a.created.cmp(&b.created).then_with(|| no(a).cmp(&no(b)))
        });
        Ok(Whole::new(all))
    }
}

impl GitHub {
    /// Ask GitHub a question of the repository, of one issue of it when
    /// `issue` is given, from `cursor` on.
    pub(crate) fn ask(
        &mut self,
        call: &str,
        question: &str,
        issue: Option<u64>,
        cursor: Option<&str>,
    ) -> Result<String, ForgeError> {
        let (owner, name) = self.owner_name();
        let mut args = vec!["api".to_string(), "graphql".to_string()];
        args.extend(["-F".to_string(), owner, "-F".to_string(), name]);
        if let Some(n) = issue {
            args.extend(["-F".to_string(), format!("number={n}")]);
        }
        args.extend(["-f".to_string(), format!("query={question}")]);
        if let Some(c) = cursor {
            args.extend(["-f".to_string(), format!("cursor={c}")]);
        }
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        self.call(call, &args)
    }

    /// One issue's answer to a question. With `settle`, a failure gh gives
    /// no reason for is settled by the issue's REST read, which says
    /// whether the issue is there; a caller that has read it passes false.
    fn ask_issue(
        &mut self,
        issue: u64,
        question: &str,
        cursor: Option<&str>,
        settle: bool,
    ) -> Got<Answer> {
        match self.ask("the issue's question", question, Some(issue), cursor) {
            Ok(out) => part(&out, "/data/repository/issue"),
            Err(e) => {
                let was = self.outcome(&e);
                if !settle || !e.why.contains("exited") {
                    return Err(was);
                }
                Err(match self.issue_answer(issue) {
                    Ok(out) if gh::is_pull(&out) => Outcome::NotFound,
                    Err(e) if self.outcome(&e) == Outcome::NotFound => Outcome::NotFound,
                    _ => was,
                })
            }
        }
    }

    /// A history, read page by page: a whole one from its oldest event to
    /// its end or one past the bound, the newest n from the end back until
    /// n are sure. `first` is a first page already asked. At least one kind
    /// asked has events on GitHub.
    fn pages(
        &mut self,
        issue: u64,
        kinds: Option<&[HappenedKind]>,
        window: Window,
        mut first: Option<Answer>,
        settle: bool,
    ) -> Got<Took<TicketEvent>> {
        let asks = events(kinds, window);
        let question = of_issue(&asks);
        let mut pages = Pages {
            seen: Vec::new(),
            back: matches!(window, Window::Last(_)),
        };
        let mut all: Vec<TicketEvent> = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let answer = match first.take() {
                Some(a) => a,
                None => self.ask_issue(issue, &question, cursor.as_deref(), settle)?,
            };
            let page = answer.events.ok_or(MALFORMED)?;
            let mut read = Vec::new();
            for e in page.nodes.into_iter().flatten() {
                let Some(what) = self.happened(&e, answer.milestone.as_ref())? else {
                    continue;
                };
                if kinds.is_some_and(|ks| !ks.contains(&what.kind())) {
                    continue;
                }
                let mut event = TicketEvent::new(what);
                event.by = e.actor.as_ref().and_then(|w| w.account(self.bot.as_ref()));
                event.at = e.created_at.as_deref().map(time).transpose()?;
                read.push(event);
            }
            let next = pages.next(page.page_info)?;
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
                    // older than they are: one more of the same second
                    // could sort among them.
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

    /// What one event of GitHub's is in the ports' words. None for a label
    /// event whose name no mark can have, which is kept to report.
    fn happened(&mut self, e: &Event, target: Option<&Milestone>) -> Got<Option<Happened>> {
        let mark = |gh: &mut GitHub| -> Got<Option<tehas_ports::Mark>> {
            let label = e.label.as_ref().ok_or(MALFORMED)?.name.clone();
            Ok(gh.read_marks(&[label]).pop().flatten())
        };
        let who = e
            .assignee
            .as_ref()
            .and_then(|w| w.account(self.bot.as_ref()));
        let blocker = |gh: &GitHub| -> Got<Option<Ref<Item>>> {
            e.blocking_issue
                .as_ref()
                .map(|b| gh.named(b).map(ticket_item))
                .transpose()
        };
        Ok(Some(match e.is.as_str() {
            "LabeledEvent" => match mark(self)? {
                Some(m) => Happened::Marked(m),
                None => return Ok(None),
            },
            "UnlabeledEvent" => match mark(self)? {
                Some(m) => Happened::Unmarked(m),
                None => return Ok(None),
            },
            "AssignedEvent" => who.map_or(Happened::Unknown, Happened::Assigned),
            "UnassignedEvent" => who.map_or(Happened::Unknown, Happened::Unassigned),
            "MilestonedEvent" => match target {
                Some(m) if Some(&m.title) == e.milestone_title.as_ref() => {
                    Happened::Targeted(Some(TargetId::new(m.number.to_string())))
                }
                _ => Happened::Unknown,
            },
            "DemilestonedEvent" => Happened::Targeted(None),
            "RenamedTitleEvent" => Happened::Retitled {
                from: e.previous_title.clone().ok_or(MALFORMED)?,
                to: e.current_title.clone().ok_or(MALFORMED)?,
            },
            "ClosedEvent" => Happened::Closed {
                reason: reason(e.state_reason.as_deref()),
                closer: self.closer(e)?,
            },
            "ReopenedEvent" => Happened::Reopened,
            "IssueTypeAddedEvent" | "IssueTypeChangedEvent" => match &e.issue_type {
                Some(t) => Happened::Kind(Some(tehas_ports::Kind::named(&t.name))),
                None => Happened::Unknown,
            },
            "IssueTypeRemovedEvent" => Happened::Kind(None),
            "BlockedByAddedEvent" => {
                blocker(self)?.map_or(Happened::Unknown, Happened::BlockerAdded)
            }
            "BlockedByRemovedEvent" => {
                blocker(self)?.map_or(Happened::Unknown, Happened::BlockerRemoved)
            }
            _ => Happened::Unknown,
        }))
    }

    /// What closed a ticket in `state`, from its newest close: None while
    /// it is open, or where its history holds no close.
    pub(crate) fn newest_closer(
        &self,
        state: &TicketState,
        closes: Option<Page<Event>>,
    ) -> Got<Option<Closer>> {
        match (state, closes) {
            (TicketState::Open, _) => Ok(None),
            (_, None) => Err(MALFORMED),
            (_, Some(closes)) => match closes.nodes.into_iter().flatten().next_back() {
                Some(e) => self.closer(&e),
                None => Ok(None),
            },
        }
    }

    /// What closed the ticket at this close: the change or the commit it
    /// names, else the account that closed it by hand. A change of another
    /// repository has no number here, and is not named.
    fn closer(&self, e: &Event) -> Got<Option<Closer>> {
        let by = match &e.closer {
            None => return Err(MALFORMED),
            Some(None) => {
                return Ok(e
                    .actor
                    .as_ref()
                    .and_then(|w| w.account(self.bot.as_ref()))
                    .map(Closer::Account))
            }
            Some(Some(by)) => by,
        };
        match by.is.as_deref() {
            Some("PullRequest") => {
                let repo = by.repository.as_ref().ok_or(MALFORMED)?;
                if !self.is_here(repo) {
                    return Ok(None);
                }
                Ok(Some(Closer::Change {
                    no: by.number.and_then(ChangeNo::new).ok_or(MALFORMED)?,
                    merged: by.merged.ok_or(MALFORMED)?,
                }))
            }
            Some("Commit") => {
                let oid = by.oid.as_deref().ok_or(MALFORMED)?;
                Ok(Some(Closer::Commit(
                    CommitId::new(oid).map_err(|_| MALFORMED)?,
                )))
            }
            _ => Ok(None),
        }
    }

    fn is_here(&self, repo: &Repo) -> bool {
        repo.name_with_owner.eq_ignore_ascii_case(&self.repo)
    }

    /// The ticket an issue of GitHub's names: of this project by its
    /// number, of another by its name. One with no number or no repository
    /// is `Unread(Inconsistent)`.
    fn named(&self, n: &Named) -> Got<Ref<TicketKey>> {
        let no = n.number.ok_or(INCONSISTENT)?;
        let repo = n.repository.as_ref().ok_or(INCONSISTENT)?;
        if self.is_here(repo) {
            let no = std::num::NonZeroU64::new(no).ok_or(INCONSISTENT)?;
            return Ok(Ref::Here(TicketKey::Number(no)));
        }
        Ok(Ref::Elsewhere {
            project: ProjectName::new(repo.name_with_owner.clone()),
            item: ItemName::new(no.to_string()),
        })
    }

    fn blocker(&self, n: &Named) -> Got<Blocker> {
        let on = ticket_item(self.named(n)?);
        let state = match open(n)? {
            true => BlockerState::Open,
            false => BlockerState::Closed,
        };
        Ok(Blocker::new(on, state))
    }

    /// A ticket's parts from their first page on, whole: every page, and a
    /// count that agrees with GitHub's.
    fn parts_from(&mut self, issue: u64, first: Page<Named>) -> Got<Vec<Part>> {
        let total = first.total_count.ok_or(INCONSISTENT)?;
        let question = of_issue(&parts());
        let mut out = Vec::new();
        let mut pages = Pages::default();
        let mut page = first;
        loop {
            for n in page.nodes {
                let n = n.ok_or(INCONSISTENT)?;
                let key = self.named(&n)?;
                out.push(Part::new(key, open(&n)?));
            }
            if out.len() > ALL_MAX.get() as usize {
                return Err(CUT);
            }
            let Some(cursor) = pages.next(page.page_info)? else {
                break;
            };
            let more = self.ask_issue(issue, &question, Some(&cursor), false)?;
            page = more.sub_issues.ok_or(MALFORMED)?;
        }
        if out.len() as u64 != total {
            return Err(INCONSISTENT);
        }
        Ok(out)
    }

    /// The changes that name a ticket, from their first page on, each once,
    /// in the order GitHub recorded them.
    fn mentions_from(&mut self, issue: u64, first: Page<Event>) -> Got<Vec<Ref<ChangeNo>>> {
        let question = of_issue(&mentions());
        let mut out: Vec<Ref<ChangeNo>> = Vec::new();
        let mut pages = Pages::default();
        let mut page = first;
        loop {
            for e in page.nodes.into_iter().flatten() {
                let Some(pull) = e
                    .source
                    .as_ref()
                    .filter(|s| s.is.as_deref() == Some("PullRequest"))
                else {
                    continue;
                };
                let change = match self.named(pull)? {
                    Ref::Here(key) => Ref::Here(ChangeNo::new(number(&key)?).ok_or(INCONSISTENT)?),
                    Ref::Elsewhere { project, item } => Ref::Elsewhere { project, item },
                };
                if !out.contains(&change) {
                    out.push(change);
                }
            }
            if out.len() > ALL_MAX.get() as usize {
                return Err(CUT);
            }
            let Some(cursor) = pages.next(page.page_info)? else {
                break;
            };
            let more = self.ask_issue(issue, &question, Some(&cursor), false)?;
            page = more.mentions.ok_or(MALFORMED)?;
        }
        Ok(out)
    }

    /// A ticket's digest from a node of a question (the list's, the
    /// closed reads'), as the issue's REST read gives it for
    /// `Tickets::several`.
    pub(crate) fn digest_from(&mut self, n: Node) -> Got<TicketDigest> {
        let no = n
            .number
            .and_then(std::num::NonZeroU64::new)
            .ok_or(INCONSISTENT)?;
        if n.labels.total_count.ok_or(INCONSISTENT)? != n.labels.nodes.len() as u64 {
            return Err(CUT);
        }
        let labels = n
            .labels
            .nodes
            .into_iter()
            .map(|l| l.map(|l| l.name).ok_or(INCONSISTENT))
            .collect::<Got<Vec<String>>>()?;
        let marks = self.read_marks(&labels).into_iter().flatten().collect();
        let state = match n.state.as_str() {
            "OPEN" => TicketState::Open,
            "CLOSED" => TicketState::Closed(reason(n.state_reason.as_deref())),
            _ => return Err(MALFORMED),
        };
        let author = n.author.as_ref().and_then(|w| w.account(self.bot.as_ref()));
        let created = time(&n.created_at)?;
        let updated = time(&n.updated_at)?;
        let link = WebLink::new(n.url).map_err(|_| MALFORMED)?;
        let sums = serde_json::json!({ "title": n.title, "body": n.body }).to_string();
        let revision = gh::text_revision(&sums).ok_or(MALFORMED)?;
        let mut digest = TicketDigest::new(
            TicketKey::Number(no),
            state,
            created,
            updated,
            Revision::new(revision),
            link,
        );
        digest.title = n.title;
        digest.kind = n.issue_type.map(|t| tehas_ports::Kind::named(&t.name));
        digest.marks = marks;
        digest.target = n
            .milestone
            .map(|m| TargetName::new(TargetId::new(m.number.to_string()), m.title));
        digest.author = author;
        Ok(digest)
    }
}

/// Whether an issue another names is open. One that does not say is
/// `Unread(Malformed)`.
fn open(n: &Named) -> Got<bool> {
    match n.state.as_deref() {
        Some("OPEN") => Ok(true),
        Some("CLOSED") => Ok(false),
        _ => Err(MALFORMED),
    }
}

fn ticket_item(key: Ref<TicketKey>) -> Ref<Item> {
    match key {
        Ref::Here(k) => Ref::Here(Item::Ticket(k)),
        Ref::Elsewhere { project, item } => Ref::Elsewhere { project, item },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The vouch reads a user id beside a login, so a question that asked a
    /// login without it would let a renamed account pass for a listed one.
    #[test]
    fn every_account_a_question_reads_comes_with_its_user_id() {
        let whole = Window::Whole(ALL_MAX);
        let last = Window::Last(std::num::NonZeroU32::MIN);
        let all = [
            of_issue(&format!("{} {}", beside(), events(None, whole))),
            of_issue(&format!("{} {}", beside(), events(None, last))),
            of_issue(&mentions()),
            of_issue(&parts()),
            listed(""),
        ];
        for q in all {
            for (at, _) in q.match_indices("login") {
                let rest = &q[at..];
                let close = rest.find('}').unwrap();
                let after = &rest[..rest.len().min(close + 60)];
                assert!(after.contains("databaseId"), "{after}");
            }
        }
    }

    /// GitHub refuses a question that declares a variable it does not use.
    #[test]
    fn every_question_uses_each_variable_it_declares() {
        let whole = Window::Whole(ALL_MAX);
        let last = Window::Last(std::num::NonZeroU32::MIN);
        let mut all = vec![
            of_issue(&format!("{} {}", beside(), events(None, whole))),
            of_issue(&format!("{} {}", beside(), events(None, last))),
            of_issue(&beside()),
            of_issue(&mentions()),
            of_issue(&parts()),
            listed(""),
        ];
        for k in [
            HappenedKind::Marked,
            HappenedKind::Closed,
            HappenedKind::Unknown,
            HappenedKind::BlockerRemoved,
        ] {
            all.push(of_issue(&events(Some(&[k]), whole)));
            all.push(of_issue(&events(Some(&[k]), last)));
        }
        for q in all {
            let (head, body) = q.split_once(')').unwrap();
            for var in head.split('$').skip(1) {
                let name = &var[..var.find(':').unwrap()];
                assert!(body.contains(&format!("${name}")), "${name}: {q}");
            }
        }
        // A kind GitHub keeps no event of asks nothing.
        assert!(events(Some(&[HappenedKind::Priority]), whole).is_empty());
    }

    #[test]
    fn a_history_asks_for_the_kinds_named_and_no_other() {
        let whole = Window::Whole(ALL_MAX);
        let closes = events(Some(&[HappenedKind::Closed, HappenedKind::Reopened]), whole);
        assert!(closes.contains("itemTypes: [CLOSED_EVENT, REOPENED_EVENT]"));
        assert!(closes.contains("first: 100, after: $cursor"));
        // GitHub has no priority: nothing is asked for.
        assert_eq!(events(Some(&[HappenedKind::Priority]), whole), "");
        // Every event that can read as unknown is asked for the unknown.
        let unknown = events(Some(&[HappenedKind::Unknown]), whole);
        assert!(
            unknown.contains("itemTypes: [ASSIGNED_EVENT, UNASSIGNED_EVENT, MILESTONED_EVENT, ISSUE_TYPE_ADDED_EVENT, ISSUE_TYPE_CHANGED_EVENT, BLOCKED_BY_ADDED_EVENT, BLOCKED_BY_REMOVED_EVENT]"),
            "{unknown}"
        );
        let last = events(None, Window::Last(std::num::NonZeroU32::MIN));
        assert!(last.contains("last: 100, before: $cursor"));
        assert!(last.contains("hasPreviousPage startCursor"));
    }
}
