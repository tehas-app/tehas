//! An issue's label events, read as lib/labels.sh reads them.
//!
//! One rule for an event with no time, and it has two directions: an event
//! with no `created_at` is older than any when it must not authorise something
//! (`label_actor`, `label_state`, `pause_holds`: an unstamped application is
//! not the latest one), and later than any when it must veto one
//! (`first_unallowed_rename`: an unstamped rename is never the title an
//! approval covered). Each direction holds the start, never lets it.

use serde_json::Value;

/// The account of an event that names nobody: tehas's word for it, the same
/// one decide's `or_unknown` answers for an empty login.
pub const UNKNOWN_ACTOR: &str = "unknown";

/// One label event of an issue, as the REST events list gives it.
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub event: String,
    pub label: String,
    pub actor: String,
    pub at: Option<String>,
}

/// The events of an issue, or why there are none to read.
#[derive(Clone, Debug, PartialEq)]
pub enum Events {
    Read(Vec<Event>),
    /// The call to GitHub failed.
    Failed,
    /// GitHub answered with something that is not a list of events.
    Unreadable,
}

impl Events {
    /// Pages of events as `gh api --paginate` prints them, joined: a list of
    /// events, or a list of such lists. `{"failed": true}` is a call that
    /// failed; anything else is an answer that cannot be read.
    pub fn parse(v: &Value) -> Events {
        if v.get("failed").and_then(Value::as_bool) == Some(true) {
            return Events::Failed;
        }
        let Some(items) = v.as_array() else {
            return Events::Unreadable;
        };
        let pages: Vec<&Value> = if items.iter().all(Value::is_array) && !items.is_empty() {
            items
                .iter()
                .flat_map(|p| p.as_array().unwrap().iter())
                .collect()
        } else {
            items.iter().collect()
        };
        let mut out = Vec::new();
        for e in pages {
            if e.is_null() {
                continue;
            }
            let Some(o) = e.as_object() else {
                return Events::Unreadable;
            };
            let s = |k: &str| o.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            out.push(Event {
                event: s("event"),
                label: o
                    .get("label")
                    .and_then(|l| l.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                actor: o
                    .get("actor")
                    .and_then(|a| a.get("login"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                at: o
                    .get("created_at")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            });
        }
        Events::Read(out)
    }
}

/// Who applied `label` most recently, and when: tehas_label_actor. Events
/// sharing a stamp keep their order; one with no stamp is older than any.
/// Empty strings when no event applied it.
pub fn label_actor(events: &[Event], label: &str) -> (String, String) {
    let mut v: Vec<&Event> = events
        .iter()
        .filter(|e| e.event == "labeled" && e.label == label)
        .collect();
    v.sort_by(|a, b| a.at.cmp(&b.at));
    match v.last() {
        Some(e) => (e.actor.clone(), e.at.clone().unwrap_or_default()),
        None => (String::new(), String::new()),
    }
}

/// The latest labeled or unlabeled event of `label`: tehas_label_state. A
/// removal sharing a stamp with an application comes after it.
pub fn label_state(events: &[Event], label: &str) -> (String, String, String) {
    let mut v: Vec<&Event> = events
        .iter()
        .filter(|e| (e.event == "labeled" || e.event == "unlabeled") && e.label == label)
        .collect();
    v.sort_by_key(|e| (e.at.clone().unwrap_or_default(), e.event == "unlabeled"));
    match v.last() {
        Some(e) => (
            e.event.clone(),
            e.actor.clone(),
            e.at.clone().unwrap_or_default(),
        ),
        None => (String::new(), String::new(), String::new()),
    }
}

/// tehas_pause_holds: the event that holds the host paused, `(event, by)`,
/// or None. Of two events in one second, the application is read last.
pub fn pause_holds(events: &[Event], allowed: &dyn Fn(&str) -> bool) -> Option<(String, String)> {
    let mut v: Vec<&Event> = events
        .iter()
        .filter(|e| (e.event == "labeled" || e.event == "unlabeled") && e.label == "tehas:pause")
        .collect();
    v.sort_by_key(|e| (e.at.clone().unwrap_or_default(), e.event != "unlabeled"));
    let e = v.last()?;
    match e.event.as_str() {
        "unlabeled" if allowed(&e.actor) => None,
        "unlabeled" | "labeled" => Some((e.event.clone(), e.actor.clone())),
        _ => None,
    }
}

/// The first `renamed` event after `since` (any of them when `since` is
/// empty) whose account is not allowed, or the veto of nobody: "unknown" for
/// a rename that names no account. None when no rename holds the title back.
/// One with no time is not old: it counts as after `since`, so an unstamped
/// rename never reads as a title the approval covered - the opposite of the
/// rule for the events that must authorise something, in the header. A stamp
/// GitHub sent in a shape the poll cannot read is read the same way, as no
/// stamp: it is GitHub's to send well, and the safe reading of one it did not
/// is the one that holds the start. The events come in GitHub's order, so the
/// newest of them answers first: the walk is from the back and stops at the
/// first veto.
pub fn first_unallowed_rename(
    events: &[Event],
    since: &str,
    allowed: &dyn Fn(&str) -> bool,
) -> Option<String> {
    events.iter().rev().find_map(|e| {
        if e.event != "renamed"
            || (!since.is_empty() && e.at.as_deref().is_some_and(|at| at <= since))
        {
            return None;
        }
        let who = if e.actor.is_empty() {
            UNKNOWN_ACTOR.to_string()
        } else {
            e.actor.clone()
        };
        (!allowed(&who)).then_some(who)
    })
}

#[cfg(test)]
mod renamed_tests {
    use super::{first_unallowed_rename, Event, UNKNOWN_ACTOR};

    const SINCE: &str = "2026-01-02T10:00:00Z";

    fn event(event: &str, actor: &str, at: Option<&str>) -> Event {
        Event {
            event: event.into(),
            label: "tehas:ready".into(),
            actor: actor.into(),
            at: at.map(str::to_string),
        }
    }

    fn vetoed(events: &[Event], since: &str) -> Option<String> {
        first_unallowed_rename(events, since, &|w| w == "alice")
    }

    /// An event with no time is later than any when it must veto one, so an
    /// unstamped rename holds the start whoever made it.
    #[test]
    fn a_rename_with_no_time_counts_as_after_the_approval() {
        let by_other = [event("renamed", "mallory", None)];
        assert_eq!(vetoed(&by_other, SINCE).as_deref(), Some("mallory"));
        let by_nobody = [event("renamed", "", None)];
        assert_eq!(vetoed(&by_nobody, SINCE).as_deref(), Some(UNKNOWN_ACTOR));
    }

    #[test]
    fn a_rename_before_the_approval_is_the_title_it_covered() {
        let before = [event("renamed", "mallory", Some("2026-01-02T09:59:00Z"))];
        assert_eq!(vetoed(&before, SINCE), None);
        // The second of the same stamp is not after it either.
        let same = [event("renamed", "mallory", Some(SINCE))];
        assert_eq!(vetoed(&same, SINCE), None);
    }

    /// The names of every rename after `since` are read, in GitHub's order,
    /// and the first one outside the list holds the start: an allowed account
    /// renaming later does not cover an earlier rename by anyone else. The
    /// walk from the back answers the same way - it finds the last one that
    /// does not belong here.
    #[test]
    fn a_rename_after_the_approval_by_an_allowed_account_vets_nothing() {
        let mine = [
            event("renamed", "mallory", Some("2026-01-02T10:01:00Z")),
            event("renamed", "alice", Some("2026-01-02T10:02:00Z")),
        ];
        assert_eq!(vetoed(&mine, SINCE).as_deref(), Some("mallory"));
        let theirs = [
            event("renamed", "alice", Some("2026-01-02T10:01:00Z")),
            event("renamed", "mallory", Some("2026-01-02T10:02:00Z")),
        ];
        assert_eq!(vetoed(&theirs, SINCE).as_deref(), Some("mallory"));
        let all_mine = [
            event("renamed", "alice", Some("2026-01-02T10:01:00Z")),
            event("renamed", "alice", Some("2026-01-02T10:02:00Z")),
        ];
        assert_eq!(vetoed(&all_mine, SINCE), None);
    }

    #[test]
    fn no_since_reads_every_rename_and_only_renames() {
        let mixed = [
            event("labeled", "mallory", Some(SINCE)),
            event("renamed", "mallory", Some("2026-01-01T00:00:00Z")),
        ];
        assert_eq!(vetoed(&mixed, "").as_deref(), Some("mallory"));
        let none = [event("labeled", "mallory", Some(SINCE))];
        assert_eq!(vetoed(&none, ""), None);
        assert_eq!(vetoed(&[], SINCE), None);
    }
}
