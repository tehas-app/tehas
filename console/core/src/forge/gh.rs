//! The reading of gh's answers, and the rules that decide which of an issue's
//! events answer a term.
//!
//! These are the rules `lib/labels.sh` states, kept where the tests can hold
//! them without gh or a network. GitHub documents no order for label events,
//! so the stamp decides, events sharing one stamp keep the order the pages
//! handed them back, and an event with no stamp is older than any stamped one.
//! Of a label applied and taken off inside one second the removal is read
//! last, whichever page either came in and whatever gh's stream order was: a
//! label that came off counts for nothing.
//!
//! The other rule here is the one that keeps a poll honest: an answer that
//! cannot be read is an error and never an empty list. An empty list is the
//! site's answer that an issue carries no label, and a caller that took a
//! broken answer for it would clear a label nobody took off.

use serde_json::Value;
use std::cmp::Ordering;

/// One label event, as the REST list gives it. `at` is None for an event with
/// no stamp: it sorts before every stamped one and answers as the empty time.
pub(crate) struct Event {
    pub(crate) event: String,
    pub(crate) label: String,
    pub(crate) actor: String,
    pub(crate) at: Option<String>,
}

/// The event `label_state` answers with: the latest of `label`'s events, or
/// None when no event carries it. Only the two kinds of label event count: a
/// `renamed` or `subscribed` event that names the label in passing is not a
/// label on the issue.
///
/// The sort is by stamp, then by kind, and `sort_by` is stable, so events
/// sharing a stamp keep the order the pages handed them back. The removal of a
/// label applied in the same second sorts after its application: whichever
/// came first in the stream, the label counts as gone.
pub(crate) fn latest_event(events: &[Event], label: &str) -> Option<super::LabelEvent> {
    let mut of: Vec<&Event> = events
        .iter()
        .filter(|e| (e.event == "labeled" || e.event == "unlabeled") && e.label == label)
        .collect();
    of.sort_by(event_order);
    of.last().map(|e| super::LabelEvent {
        kind: e.event.clone(),
        actor: e.actor.clone(),
        at: e.at.clone().unwrap_or_default(),
    })
}

/// The event `approval` answers with: the latest `labeled` event of `label`,
/// or None when no event applied it.
///
/// A removal is not an application, so it never supplies the account or the
/// time here: `tehas_label_actor` filters to `labeled` and this keeps that
/// rule. Were it kept out, a label taken off by an allowed maintainer would
/// read as an approval by that maintainer, and authorisation is the account
/// that applied the label (AGENTS.md rule 5): a poll would judge the remover
/// against TEHAS_ALLOWED_LABELERS.
///
/// The tie-break of one second belongs to the term, not to the events: of a
/// label applied and taken off inside a second `tehas_label_state` reads the
/// removal last, while `tehas_pause_holds` reads the application last so that
/// a pause lifted in the same second it was applied still holds the host. So
/// [`event_order`] is the state term's order alone, and this keeps the rule
/// `tehas_label_actor` states: the stamp decides, an event with no stamp is
/// older than any stamped one, and of events the stamp cannot separate the
/// LAST one the page handed back answers, because jq's `sort_by` and Rust's
/// `sort_by` are both stable. Checked against both readers, not reasoned out:
/// a rule about a tie is the kind that reads backwards in one's head.
pub(crate) fn latest_applied(events: &[Event], label: &str) -> Option<super::LabelEvent> {
    let mut of: Vec<&Event> = events
        .iter()
        .filter(|e| e.event == "labeled" && e.label == label)
        .collect();
    of.sort_by(|a, b| a.at.cmp(&b.at));
    of.last().map(|e| super::LabelEvent {
        kind: e.event.clone(),
        actor: e.actor.clone(),
        at: e.at.clone().unwrap_or_default(),
    })
}

/// The order of two label events: the stamp first, then, within one second,
/// the application before the removal. The state term's order: the approval
/// term reads [`latest_applied`], whose candidates are applications alone and
/// which therefore sorts by the stamp, as `tehas_label_actor` does. An
/// unstamped event sorts before every stamped one (see [`Event`]). [`event_order`]
/// and the test of one second's order both read the rule, so it is stated
/// once.
pub(crate) fn event_order(a: &&Event, b: &&Event) -> Ordering {
    a.at.cmp(&b.at).then_with(|| rank(a).cmp(&rank(b)))
}

/// The tie-break within one second: 0 for an application, 1 for a removal.
fn rank(e: &Event) -> u8 {
    u8::from(e.event == "unlabeled")
}

/// One page as events. A page that is no list, or an entry that is no object,
/// is an answer that cannot be read: the reason says so, and the caller stops
/// rather than deciding an issue carries no labels. An entry that names no
/// kind, or no label, is well formed and is no label event, which the site
/// answers with its own fields: it is read as the empty fields it has and
/// sorted out by the caller, not made into a failure.
fn page_events(page: &Value) -> Result<Vec<Event>, String> {
    let Some(items) = page.as_array() else {
        return Err(": a page of events is no list".to_string());
    };
    let mut out = Vec::with_capacity(items.len());
    for e in items {
        // An entry that says nothing is no event; what it meant is not for
        // this reader to decide.
        if e.is_null() {
            continue;
        }
        let Some(o) = e.as_object() else {
            return Err(": an event is no object".to_string());
        };
        let field = |k: &str| {
            o.get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        out.push(Event {
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
            at: o
                .get("created_at")
                .and_then(Value::as_str)
                .map(str::to_string),
        });
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
    let mut pages = serde_json::Deserializer::from_str(out)
        .into_iter::<Value>()
        .peekable();
    match pages.peek() {
        // Nothing at all: an answer that has no value in it even after the
        // whitespace between the pages is cut away is no answer at all, and a
        // caller that read it as "no labels" would stop work nobody asked it
        // to stop. The site says an issue with no events by answering `[]`.
        None => Err(": no events to read".to_string()),
        // A page, or something that is no JSON: a stream that breaks part way
        // is read below and reported there.
        Some(Ok(Value::Array(_))) | Some(Err(_)) => Ok(()),
        // A value that is no list says nothing of the events.
        Some(Ok(_)) => Err(": no events to read".to_string()),
    }?;
    let mut events = Vec::new();
    for page in pages {
        let page = page.map_err(|_| ": no events to read".to_string())?;
        events.append(&mut page_events(&page)?);
    }
    Ok(events)
}

/// The labels of the REST issue read: `{"labels": [{"name": "…"}]}`. None for
/// an answer that is no issue with labels: an issue carrying nothing answers
/// `{"labels":[]}`, which reads as the empty list it is.
pub(crate) fn parse_labels(out: &str) -> Option<Vec<String>> {
    let v: Value = serde_json::from_str(out.trim()).ok()?;
    let names = v.get("labels")?.as_array()?;
    names
        .iter()
        .map(|l| l.get("name").and_then(Value::as_str).map(str::to_string))
        .collect()
}

/// Whether gh's reason is the one that means the label was not there: the 404
/// of a removal that had nothing to remove. Matched on gh's own words, as the
/// poll's refresh_unmark matches on `Not Found`, and in lower case, because the
/// words are GitHub's and arrive in more than one spelling.
pub(crate) fn says_not_found(why: &str) -> bool {
    let w = why.to_ascii_lowercase();
    w.contains("not found") || w.contains("404")
}

/// What a failed gh call's answer says of its error: ": " and the first of a
/// fixed list of words in what gh printed, else the exit status. The list is
/// the one `libexec/dispatch`'s `gh_json` greps and the dispatcher's `gh_why`
/// holds, so a caller logs the reason the poll logs. Only these words, or a
/// status: the rest of gh's answer is written by a host and may name one, or
/// carry a token.
pub(crate) fn why(err: &str, code: Option<i32>) -> String {
    // Longest first where two could match the same line, so a secondary limit
    // is not reported as a plain one.
    const WORDS: [&str; 8] = [
        "secondary rate limit",
        "unexpected EOF",
        "connection reset",
        "connection refused",
        "rate limit",
        "timed out",
        "timeout",
        "no such host",
    ];
    for line in err.lines() {
        let low = line.to_ascii_lowercase();
        // The first line that says anything is the one quoted, and within it
        // the first word of the list that matches: only these words, so the
        // rest of gh's answer - a host's address, a token - stays out.
        if let Some((i, w)) = WORDS.iter().find_map(|w| low.find(w).map(|i| (i, *w))) {
            return format!(": {}", &line[i..i + w.len()]);
        }
        // An HTTP status is gh's own word for the state and is kept whole.
        if let Some(i) = find_http(&low) {
            return format!(": HTTP {}", &line[i + 5..i + 8]);
        }
    }
    match code {
        Some(c) => format!(": gh exited {c}"),
        None => ": gh was stopped by a signal".to_string(),
    }
}

/// Where a lower-case line names an HTTP status: "http " and the digits after
/// it. The digits are checked so that a line that only holds the word is not
/// read as a status.
fn find_http(line: &str) -> Option<usize> {
    let b = line.as_bytes();
    let head = b"http ";
    let mut i = 0;
    while i + head.len() + 3 <= b.len() {
        if b[i..i + head.len()] == head[..] && b[i + 5..i + 8].iter().all(u8::is_ascii_digit) {
            return Some(i);
        }
        i += 1;
    }
    None
}
