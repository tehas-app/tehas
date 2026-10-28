//! The reading of gh's answers, and the rules that decide which of an issue's
//! events answer a term.
//!
//! These are the rules `lib/labels.sh` states, kept where the tests can hold
//! them without gh or a network. [`event_order`] is the one place the ordering
//! of events is stated; the terms here read it and nothing restates it.
//!
//! The other rule here is the one that keeps a poll honest: an answer that
//! cannot be read is an error and never an empty list. An empty list is the
//! site's answer that an issue carries no label, and a caller that took a
//! broken answer for it would clear a label nobody took off.
//!
//! The three readers of label events in this workspace - this one,
//! `console/dispatch/src/events.rs` and `lib/labels.sh` - order events by the
//! same rule, and the words [`why`] holds are the words `libexec/dispatch`'s
//! `gh_json` greps. The dispatcher's `read::run::gh_why` keeps a regex copy of
//! that list where its own caller is; `tehas-dispatch` already depends on this
//! crate, so the copy goes when the last issue of #800 moves the callers here.

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

/// The event the state term answers with: the latest of `label`'s events, or
/// None when no event carries it. Only the two kinds of label event count: a
/// `renamed` or `subscribed` event that names the label in passing is not a
/// label on the issue. Orders as [`event_order`] states, and reads both kinds.
pub(crate) fn latest_event(events: &[Event], label: &str) -> Option<super::LabelEvent> {
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
pub(crate) fn latest_applied(events: &[Event], label: &str) -> Option<super::LabelEvent> {
    let mut of: Vec<&Event> = events
        .iter()
        .filter(|e| e.event == "labeled" && e.label == label)
        .collect();
    of.sort_by(event_order);
    last_of(&of)
}

/// The last of a run the caller has ordered, as a term reads it: the fields the
/// event names, and the empty time for one that names none.
fn last_of(of: &[&Event]) -> Option<super::LabelEvent> {
    of.last().map(|e| super::LabelEvent {
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
/// keeps the order the pages handed back, so the LAST one answers, as jq's
/// stable `sort_by` answers in `lib/labels.sh`.
///
/// The pause is the one reader of these rules that orders the tie the other
/// way, so that a pause lifted in the second it was applied still holds the
/// host. That term is `dispatch`'s `events::pause_holds`, which the last issue
/// of #800 moves here; it is no term of this module today.
pub(crate) fn event_order(a: &&Event, b: &&Event) -> Ordering {
    a.at.cmp(&b.at).then_with(|| rank(a).cmp(&rank(b)))
}

/// The tie-break within one second: 0 for an application, 1 for a removal.
fn rank(e: &Event) -> u8 {
    u8::from(e.event == "unlabeled")
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
fn page_events(page: &Value) -> Result<Vec<Event>, String> {
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
        let stamp = match o.get("created_at") {
            // Absent is the site saying nothing of the time; present and no
            // text is an answer that cannot be read. jq sorts a number before
            // every string, so reading a wrong-typed stamp as no stamp would
            // win a tie the reference reader gives to the stamped event.
            None => None,
            Some(Value::Null) => None,
            Some(Value::String(s)) => Some(s.clone()),
            Some(_) => return Err("an event's stamp is no text".to_string()),
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
            at: stamp,
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
        let page = page.map_err(|_| NO_EVENTS.to_string())?;
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

/// Whether a failed removal answers that the label was not there, which is the
/// state wanted. The words are gh's own, as the poll's `refresh_unmark` and
/// `libexec/cycle` match `Not Found`, and the status is read whole out of the
/// digits `find_http` located, never as "404" anywhere in the text: a gateway
/// error whose body happens to carry those three digits is no removal.
pub(crate) fn says_not_found(why: &str) -> bool {
    let w = why.to_ascii_lowercase();
    if w.contains("not found") {
        return true;
    }
    find_http(&w).is_some_and(|i| &w.as_bytes()[i + 5..i + 8] == b"404")
}

/// What a failed gh call's answer says of its error: ": " and the first of a
/// fixed list of words in what gh printed, else the exit status. The list is
/// the one `libexec/dispatch`'s `gh_json` greps and the dispatcher's `gh_why`
/// holds, so a caller logs the reason the poll logs. Only these words, or a
/// status: the rest of gh's answer is written by a host and may name one, or
/// carry a token.
pub(crate) fn why(err: &str, code: Option<i32>) -> String {
    // Longest first where two could match the same word, so a secondary limit
    // is not reported as a plain one where they start alike.
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
        // the LEFTMOST word of the list, as grep -oE and the dispatcher's
        // regex answer: the first entry of the list would report "rate limit"
        // for a line whose only match is a secondary limit later on. Only
        // these words, so the rest of gh's answer - a host's address, a token
        // - stays out. The text is cut from `low`, the string that was
        // searched, so the byte offsets of `find` are its own.
        if let Some((i, w)) = WORDS
            .iter()
            .filter_map(|w| low.find(w).map(|i| (i, *w)))
            .min_by_key(|(i, w)| (*i, std::cmp::Reverse(w.len())))
        {
            return format!(": {}", &low[i..i + w.len()]);
        }
        // An HTTP status is gh's own word for the state and is kept whole.
        if let Some(i) = find_http(&low) {
            return format!(": HTTP {}", &low[i + 5..i + 8]);
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
