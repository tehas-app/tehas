//! The reading of gh's answers, and the rules that decide which of an issue's
//! events answer a term.
//!
//! These are the rules of a label's events, kept where the tests can hold
//! them without gh or a network. [`event_order`] is the one place the ordering
//! of events is stated; the terms here read it and nothing restates it.
//!
//! The other rule here is the one that keeps a poll honest: an answer that
//! cannot be read is an error and never an empty list. An empty list is the
//! site's answer that an issue carries no label, and a caller that took a
//! broken answer for it would clear a label nobody took off.
//!
//! The two readers of label events in this workspace - this one and
//! `console/core/src/events.rs` - order events by the same rule. The reading
//! of a failed call's reason, [`why`] and [`says_not_found`], stays in core
//! beside [`tehas_core::forge::Bounded`], the runner that makes the reason,
//! until the runner moves here.

use crate::{Approval, LabelEvent};
use serde_json::Value;
use std::cmp::Ordering;
pub use tehas_core::forge::{says_not_found, why};

/// One label event, as the REST list gives it. `at` is None for an event with
/// no stamp: it sorts before every stamped one and answers as the empty time.
pub struct Event {
    pub event: String,
    pub label: String,
    pub actor: String,
    pub at: Option<String>,
}

/// The event the state term answers with: the latest of `label`'s events, or
/// None when no event carries it. Only the two kinds of label event count: a
/// `renamed` or `subscribed` event that names the label in passing is not a
/// label on the issue. Orders as [`event_order`] states, and reads both kinds.
pub(crate) fn latest_event(events: &[Event], label: &str) -> Option<LabelEvent> {
    let mut of: Vec<&Event> = events
        .iter()
        .filter(|e| (e.event == "labeled" || e.event == "unlabeled") && e.label == label)
        .collect();
    of.sort_by(event_order);
    last_of(&of)
}

/// The event the approval term answers with: the latest `labeled` event of
/// `label`, or None when no event applied it. Orders as [`event_order`] states,
/// and reads applications alone.
///
/// A removal is not an application, so it never supplies the account or the
/// time here: `tehas_label_actor` filters to `labeled` and this keeps that
/// rule. Were it kept out, a label taken off by an allowed maintainer would
/// read as an approval by that maintainer, and authorisation is the account
/// that applied the label (AGENTS.md rule 5): a poll would judge the remover
/// against TEHAS_ALLOWED_LABELERS.
pub(crate) fn latest_applied(events: &[Event], label: &str) -> Option<LabelEvent> {
    let mut of: Vec<&Event> = events
        .iter()
        .filter(|e| e.event == "labeled" && e.label == label)
        .collect();
    of.sort_by(event_order);
    last_of(&of)
}

/// The last of a run the caller has ordered, as a term reads it: the fields the
/// event names, and the empty time for one that names none.
fn last_of(of: &[&Event]) -> Option<LabelEvent> {
    of.last().map(|e| LabelEvent {
        kind: e.event.clone(),
        actor: e.actor.clone(),
        at: e.at.clone().unwrap_or_default(),
    })
}

/// The order of two label events, stated once for every term here. GitHub
/// documents no order, so the stamp decides and an unstamped event is older
/// than any stamped one (see [`Event`]). Of events the stamp cannot separate,
/// the application sorts before the removal, so a label applied and taken off
/// inside one second counts as gone; of two of one kind `sort_by`'s stability
/// keeps the order the pages handed back, so the LAST one answers, as a
/// stable sort answers.
///
/// The pause is the one reader of these rules that orders the tie the other
/// way, so that a pause lifted in the second it was applied still holds the
/// host. That term is [`tehas_core::events::pause_holds`], which the last issue
/// of #800 moves here; it is no term of this module today.
pub fn event_order(a: &&Event, b: &&Event) -> Ordering {
    a.at.cmp(&b.at).then_with(|| rank(a).cmp(&rank(b)))
}

/// The tie-break within one second: 0 for an application, 1 for a removal.
fn rank(e: &Event) -> u8 {
    u8::from(e.event == "unlabeled")
}

/// What a label event says of its account beyond the login: the site's id
/// and its kind of account, each None where the event does not say.
pub(crate) struct Facts {
    pub(crate) id: Option<u64>,
    pub(crate) kind: Option<String>,
}

/// What the reader of the events says of an answer that holds none of them.
/// It carries no `": "` of its own: that is the line of the error, not the
/// reason's.
const NO_EVENTS: &str = "no events to read";

/// The reading of one page as its events. A page that is no list, or an entry
/// that is no object, is an answer that cannot be read: the reason says so,
/// and the caller stops rather than deciding an issue carries no labels. An
/// entry that names no kind, or no label, is well formed and is no label
/// event, which the site answers with its own fields: it is read as the empty
/// fields it has and sorted out by the caller, not made into a failure. A
/// reason carries no `": "` of its own: the error's line adds that.
fn page_events(page: &Value) -> Result<Vec<(Event, Facts)>, String> {
    let Some(items) = page.as_array() else {
        return Err("a page of events is no list".to_string());
    };
    let mut out = Vec::with_capacity(items.len());
    for e in items {
        // An entry that says nothing is no event; what it meant is not for
        // this reader to decide.
        if e.is_null() {
            continue;
        }
        let Some(o) = e.as_object() else {
            return Err("an event is no object".to_string());
        };
        let field = |k: &str| {
            o.get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        // The same rule as `Events::parse`: a stamp that is present and is no
        // text cannot be ordered, so the whole page is refused rather than
        // read as no stamp. jq sorts a number before every string, so reading
        // a wrong-typed stamp as no stamp would win a tie the reference reader
        // gives to the stamped event.
        let stamp = match o.get("created_at") {
            None => None,
            Some(Value::Null) => None,
            Some(Value::String(s)) => Some(s.clone()),
            Some(_) => return Err("an event's stamp is no text".to_string()),
        };
        let who = o.get("actor");
        let facts = Facts {
            id: who.and_then(|a| a.get("id")).and_then(|i| i.as_u64()),
            kind: who
                .and_then(|a| a.get("type"))
                .and_then(|t| t.as_str())
                .map(str::to_string),
        };
        let event = Event {
            event: field("event"),
            label: o
                .get("label")
                .and_then(|l| l.get("name"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            actor: o
                .get("actor")
                .and_then(|a| a.get("login"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            at: stamp,
        };
        out.push((event, facts));
    }
    Ok(out)
}

/// gh's answer to the events call: every page's events, in the order the
/// pages came back. `gh api --paginate` prints one JSON array per page, one
/// after another, so the answer is read as a stream of values, each of which
/// must be a list: one array is one page, and two arrays with no separator
/// between them are two. Anything else says nothing of the issue's events,
/// which is the answer a caller must never be handed as an empty list.
pub(crate) fn parse_events(out: &str) -> Result<Vec<Event>, String> {
    Ok(parse_with_facts(out)?.into_iter().map(|(e, _)| e).collect())
}

/// [`parse_events`], each event with what it says of its account.
pub(crate) fn parse_with_facts(out: &str) -> Result<Vec<(Event, Facts)>, String> {
    let mut pages = serde_json::Deserializer::from_str(out)
        .into_iter::<Value>()
        .peekable();
    match pages.peek() {
        // Nothing at all, which is also what only the whitespace between the
        // pages leaves: an answer with no value in it is no answer, and a
        // caller that read it as "no labels" would stop work nobody asked it
        // to stop. The site says an issue with no events by answering `[]`.
        None => Err(NO_EVENTS.to_string()),
        // A page, or something that is no JSON: a stream that breaks part way
        // is read below and reported there.
        Some(Ok(Value::Array(_))) | Some(Err(_)) => Ok(()),
        // A value that is no list says nothing of the events.
        Some(Ok(_)) => Err(NO_EVENTS.to_string()),
    }?;
    let mut events = Vec::new();
    for page in pages {
        let mut page = page.map_err(|_| NO_EVENTS.to_string())?;
        tehas_core::accounts::vouch(&mut page);
        events.append(&mut page_events(&page)?);
    }
    Ok(events)
}

/// The labels of the REST issue read: `{"labels": [{"name": "…"}]}`. None for
/// an answer that is no issue with labels: an issue carrying nothing answers
/// `{"labels":[]}`, which reads as the empty list it is. An entry that names
/// no label is no label and is dropped, as the dispatcher's `snapshot::names`
/// reads it: a hand-written entry of one issue stops no other label of it.
pub(crate) fn parse_labels(out: &str) -> Option<Vec<String>> {
    let v: Value = serde_json::from_str(out.trim()).ok()?;
    let names = v.get("labels")?.as_array()?;
    Some(
        names
            .iter()
            .filter_map(|l| l.get("name").and_then(Value::as_str).map(str::to_string))
            .collect(),
    )
}

/// Whether the REST issue read answered a pull request: GitHub answers one
/// at an issue's path, with a `pull_request` beside its fields.
pub(crate) fn is_pull(out: &str) -> bool {
    serde_json::from_str::<Value>(out.trim())
        .ok()
        .and_then(|v| v.get("pull_request").map(|p| !p.is_null()))
        .unwrap_or(false)
}

/// An issue's title and body as one revision, or None when the answer is no
/// issue with a title.
pub(crate) fn text_revision(out: &str) -> Option<String> {
    let v: Value = serde_json::from_str(out.trim()).ok()?;
    let title = tehas_core::approval::title_sha256(&v)?;
    Some(format!("{title}:{}", tehas_core::approval::body_sha256(&v)))
}

/// The question of the issue types the repository's owner has.
pub(crate) const ISSUE_TYPES: &str = "query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { issueTypes(first: 50) { nodes { name } } } }";

/// The question of an issue's type and the newest changes of it, each with the
/// account that made it.
pub(crate) const TYPE_EVENTS: &str = "query($owner: String!, $name: String!, $number: Int!) { repository(owner: $owner, name: $name) { issue(number: $number) { issueType { name } timelineItems(itemTypes: [ISSUE_TYPE_ADDED_EVENT, ISSUE_TYPE_CHANGED_EVENT, ISSUE_TYPE_REMOVED_EVENT], last: 20) { nodes { __typename ... on IssueTypeAddedEvent { createdAt actor { login ...on User { databaseId } ...on Bot { databaseId } } issueType { name } } ... on IssueTypeChangedEvent { createdAt actor { login ...on User { databaseId } ...on Bot { databaseId } } issueType { name } } ... on IssueTypeRemovedEvent { createdAt actor { login ...on User { databaseId } ...on Bot { databaseId } } issueType { name } } } } } } }";

fn graphql(out: &str) -> Result<Value, String> {
    serde_json::from_str(out).map_err(|_| "the answer is no JSON".to_string())
}

/// The names of the owner's issue types. A repository whose owner has none, a
/// personal account's, answers the empty list; an answer with no repository
/// in it is no answer.
pub(crate) fn parse_issue_types(out: &str) -> Result<Vec<String>, String> {
    let v = graphql(out)?;
    let Some(repo) = v.pointer("/data/repository").filter(|r| r.is_object()) else {
        return Err("the answer names no repository".to_string());
    };
    match repo.get("issueTypes") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(t) => t
            .get("nodes")
            .and_then(Value::as_array)
            .map(|nodes| {
                nodes
                    .iter()
                    .filter_map(|n| n.get("name").and_then(Value::as_str).map(str::to_string))
                    .collect()
            })
            .ok_or_else(|| "the issue types are no list".to_string()),
    }
}

/// Who gave the issue the type `name`, as [`crate::GitHub::typed_by`] answers.
/// GitHub gives the timeline oldest first, so the newest change is the last.
/// It names the setter only when it is the change that set `name`: a removal,
/// a change to another type, or none at all, and the type came some way the
/// page does not show, which counts for nobody. A listed login whose actor
/// does not carry its recorded user id counts for nobody too, as in a label
/// event.
pub fn parse_typed_by(out: &str, name: &str) -> Result<Option<Approval>, String> {
    let mut v = graphql(out)?;
    tehas_core::accounts::vouch(&mut v);
    let Some(issue) = v
        .pointer("/data/repository/issue")
        .filter(|i| i.is_object())
    else {
        return Err("the answer names no issue".to_string());
    };
    if issue.pointer("/issueType/name").and_then(Value::as_str) != Some(name) {
        return Ok(None);
    }
    let Some(nodes) = issue
        .pointer("/timelineItems/nodes")
        .and_then(Value::as_array)
    else {
        return Err("the type's events are no list".to_string());
    };
    let set = nodes.last().filter(|e| {
        matches!(
            e.get("__typename").and_then(Value::as_str),
            Some("IssueTypeAddedEvent" | "IssueTypeChangedEvent")
        ) && e.pointer("/issueType/name").and_then(Value::as_str) == Some(name)
    });
    let Some(e) = set else {
        return Ok(Some(Approval::default()));
    };
    // Absent or null is a fact: the account is gone, or no stamp. Any other
    // type is a malformed answer, refused as in `page_events`, so it is not
    // read as an account that is gone.
    let actor = match e.get("actor") {
        None | Some(Value::Null) => None,
        Some(Value::Object(a)) => a.get("login"),
        Some(_) => return Err("a type event's actor is no text".to_string()),
    };
    let actor = match actor {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(_) => return Err("a type event's actor is no text".to_string()),
    };
    let at = match e.get("createdAt") {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(_) => return Err("a type event's stamp is no text".to_string()),
    };
    Ok(Some(Approval { actor, at }))
}
