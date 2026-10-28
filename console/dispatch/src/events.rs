//! An issue's label events, read as lib/labels.sh reads them.

use serde_json::Value;

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

/// The newest labeled or unlabeled event of one label: tehas_label_state.
#[derive(Clone, Debug, PartialEq)]
pub enum LabelState {
    /// The page had no event of the label, so it says nothing about it.
    Absent,
    /// An application by `actor` at `at` (empty when the page gave no stamp).
    Labeled { actor: String, at: String },
    /// A removal by `actor` at `at`.
    Unlabeled { actor: String, at: String },
}

impl LabelState {
    /// The kind as the bash's rule compares it: `"labeled"`, `"unlabeled"` or
    /// `""` for a label the timeline has no event of.
    pub fn event(&self) -> &str {
        match self {
            LabelState::Absent => "",
            LabelState::Labeled { .. } => "labeled",
            LabelState::Unlabeled { .. } => "unlabeled",
        }
    }

    /// Who applied or took off the label, "" when the timeline has no event
    /// of it.
    pub fn actor(&self) -> &str {
        match self {
            LabelState::Absent => "",
            LabelState::Labeled { actor, .. } | LabelState::Unlabeled { actor, .. } => actor,
        }
    }
}

/// The latest labeled or unlabeled event of `label`. Of two events sharing a
/// stamp the removal comes last; one with no stamp is older than any. A
/// caller that reads the answer as a trust decision relies on that ordering:
/// it is what settles the newest event at a one-second boundary and among
/// unstamped events.
pub fn label_state(events: &[Event], label: &str) -> LabelState {
    let mut v: Vec<&Event> = events
        .iter()
        .filter(|e| (e.event == "labeled" || e.event == "unlabeled") && e.label == label)
        .collect();
    v.sort_by_key(|e| (e.at.clone().unwrap_or_default(), e.event == "unlabeled"));
    let Some(e) = v.last() else {
        return LabelState::Absent;
    };
    let at = e.at.clone().unwrap_or_default();
    if e.event == "labeled" {
        LabelState::Labeled {
            actor: e.actor.clone(),
            at,
        }
    } else {
        LabelState::Unlabeled {
            actor: e.actor.clone(),
            at,
        }
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

/// Logins named by `renamed` events after `since` (all of them when `since`
/// is empty), "unknown" for one that names nobody.
pub fn renamed_since(events: &[Event], since: &str) -> Vec<String> {
    events
        .iter()
        .filter(|e| {
            e.event == "renamed" && (since.is_empty() || e.at.as_deref().unwrap_or("") > since)
        })
        .map(|e| {
            if e.actor.is_empty() {
                "unknown".to_string()
            } else {
                e.actor.clone()
            }
        })
        .collect()
}
