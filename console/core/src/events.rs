//! An issue's label events.
//!
//! One rule for an event with no time, and it has two directions: an event
//! with no `created_at` is older than any when it must not authorise something
//! (`label_actor`, `label_state`, `standing_actor`, `pause_holds`: an
//! unstamped application is not the latest one), and later than any when it
//! must veto one
//! (`first_unallowed_rename`: an unstamped rename is never the title an
//! approval covered). Each direction holds the start, never lets it.

use crate::accounts::account;
use crate::grant::{self, Actor, Change, LabelEvent};
use serde_json::Value;

/// The account of an event that names nobody, as a person reads it: the
/// same word decide's `or_unknown` answers for an empty login. A list may
/// name a GitHub account of that login, so no rule compares this word with
/// one: they compare [`NOBODY`].
pub const UNKNOWN_ACTOR: &str = "unknown";

/// The login of an event that names nobody, such as an application the pins
/// refuse. `allowed_in` allows no list to hold it.
pub const NOBODY: &str = "";

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
            // A stamp that is present and is no text cannot be ordered, so
            // the whole answer is unreadable rather than read as no stamp:
            // the same rule `forge::gh::page_events` refuses by.
            let at = match o.get("created_at") {
                None | Some(Value::Null) => None,
                Some(Value::String(s)) => Some(s.clone()),
                Some(_) => return Events::Unreadable,
            };
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
                at,
            });
        }
        Events::Read(out)
    }
}

/// Whether `login` is on the allowed list, logins with one space between them
/// as lib/config.sh checks it. An empty name is nobody and is never allowed.
pub fn allowed_in(list: &str, login: &str) -> bool {
    !login.is_empty() && list.split(' ').any(|l| l == login)
}

/// Who applied `label` most recently, and when: tehas_label_actor. Events
/// sharing a stamp keep their order; one with no stamp is older than any.
/// Empty strings when no event applied it. A label taken off since still names
/// its applier here, so an approval reads `standing_actor` instead.
pub fn label_actor(events: &[Event], label: &str) -> (String, String) {
    let mut v: Vec<&Event> = events
        .iter()
        .filter(|e| e.event == "labeled" && e.label == label)
        .collect();
    v.sort_by(|a, b| a.at.cmp(&b.at));
    match grant::last_application(&timeline(&v), &label).map(|i| v[i]) {
        Some(e) => (e.actor.clone(), e.at.clone().unwrap_or_default()),
        None => (String::new(), String::new()),
    }
}

/// Labeled and unlabeled events in their order, as the grant decision reads
/// them. Their accounts are vouched for and their applications judged by the
/// pins already, when the snapshot was read.
fn timeline<'a>(v: &[&'a Event]) -> Vec<LabelEvent<&'a str, &'a str>> {
    v.iter()
        .map(|e| LabelEvent {
            actor: Actor::vouched(e.actor.as_str()),
            label: e.label.as_str(),
            pinned: true,
            change: if e.event == "labeled" {
                Change::Applied
            } else {
                Change::Removed
            },
        })
        .collect()
}

/// The time of the `tehas:plan` a plan's start is credited to: `by` applied it
/// last, as the events say and not only the session's record, is allowed and
/// is not the millwright, who may be on the list and is no person. None when
/// any of that fails, an unknown account included.
pub fn plan_credit(
    events: &[Event],
    by: &str,
    allowed: &dyn Fn(&str) -> bool,
    millwright: &dyn Fn(&str) -> bool,
) -> Option<String> {
    let (who, at) = label_actor(events, "tehas:plan");
    (!by.is_empty() && who == by && allowed(by) && !millwright(by)).then_some(at)
}

/// Whether the only event of `label` after `since` is one application by the
/// account `who` names. An event with no time counts as after, as a veto does:
/// a label taken off or applied again is no longer the one a plan's record
/// stood for.
pub fn sole_application(
    events: &[Event],
    label: &str,
    since: &str,
    who: &dyn Fn(&str) -> bool,
) -> bool {
    let mut after = events.iter().filter(|e| {
        (e.event == "labeled" || e.event == "unlabeled")
            && e.label == label
            && e.at.as_deref().is_none_or(|at| at > since)
    });
    match (after.next(), after.next()) {
        (Some(e), None) => e.event == "labeled" && who(&e.actor),
        _ => false,
    }
}

/// The newest labeled or unlabeled event of one label.
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
    let v = ordered(events, label);
    let Some(e) = grant::latest(&timeline(&v), &label).map(|i| v[i]) else {
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

/// Who applied `label` and when, as `label_actor` gives it, when the newest
/// event of the label in `label_state`'s order is that application: the
/// account an approval is judged by. Empty strings when the label was taken
/// off since or never applied, so a removal the issue's labels do not show yet
/// grants nothing.
pub fn standing_actor(events: &[Event], label: &str) -> (String, String) {
    let v = ordered(events, label);
    match grant::standing(&timeline(&v), &label).map(|i| v[i]) {
        Some(e) => (e.actor.clone(), e.at.clone().unwrap_or_default()),
        None => (String::new(), String::new()),
    }
}

/// `standing_actor`, but a removal by the factory's own account `bot`
/// (`accounts::factorys`) does not take the label off. Tehas takes
/// `tehas:ready` off an epic and the person's application still approves its
/// sub-issues; a removal by anyone else ends it.
pub fn standing_past_bot(events: &[Event], label: &str, bot: &str) -> (String, String) {
    let v: Vec<&Event> = ordered(events, label)
        .into_iter()
        .filter(|e| !(e.event == "unlabeled" && crate::accounts::factorys(&e.actor, bot)))
        .collect();
    match grant::standing(&timeline(&v), &label).map(|i| v[i]) {
        Some(e) => (e.actor.clone(), e.at.clone().unwrap_or_default()),
        None => (String::new(), String::new()),
    }
}

/// The labeled and unlabeled events of `label`, oldest first: of two sharing
/// a stamp the removal comes last, and one with no stamp is older than any.
fn ordered<'a>(events: &'a [Event], label: &str) -> Vec<&'a Event> {
    let mut v: Vec<&Event> = events
        .iter()
        .filter(|e| (e.event == "labeled" || e.event == "unlabeled") && e.label == label)
        .collect();
    v.sort_by_key(|e| (e.at.clone().unwrap_or_default(), e.event == "unlabeled"));
    v
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

/// Whether a change `who` made at `at` is after an approval `approver` gave
/// at `since`. GitHub stamps to the second, so a change in the approval's own
/// second may have come after it: it counts as after, unless the approver's
/// own account made it. An empty `since` has every change after it.
pub fn changed_after(at: &str, who: &str, since: &str, approver: &str) -> bool {
    since.is_empty()
        || at > since
        || (at == since && (approver.is_empty() || account(who) != account(approver)))
}

/// Whether the edits of an issue's body hold back the approval `approver`
/// gave at `since`: the login of an editor `allowed` refuses whose edit is
/// after `since` (as `changed_after` reads it), or "unknown" for one that
/// names no editor or a history that may be cut. An edit by no account holds
/// even when a list names an account "unknown": the rule compares the login,
/// and only the answer is text. `edit` carries GraphQL's
/// `lastEditedAt` and `editor.login`, and `userContentEdits` with
/// `totalCount` and `nodes { editedAt editor { login } }`.
///
/// An empty or missing `lastEditedAt` is no edit: GitHub stamps every edit it
/// saves, so an unstamped record is one nobody saved. When the last edit is
/// after the approval, every edit is judged, not only the last: an allowed
/// account that saves the body after an outside one does not clear it. A
/// history that is missing, that holds fewer nodes than `totalCount`, or a
/// node with no stamp, cannot be placed before the approval, so it holds. A
/// rename is read the same way (`first_unallowed_rename`).
pub fn edit_holds(
    edit: &Value,
    since: &str,
    approver: &str,
    allowed: &dyn Fn(&str) -> bool,
) -> Option<String> {
    let at = edit
        .get("lastEditedAt")
        .and_then(Value::as_str)
        .unwrap_or("");
    let by = login_of(edit);
    if at.is_empty() || !changed_after(at, by, since, approver) {
        return None;
    }
    if by == NOBODY || !allowed(by) {
        return Some(shown(by));
    }
    let history = edit.get("userContentEdits");
    let nodes = history
        .and_then(|h| h.get("nodes"))
        .and_then(Value::as_array);
    let total = history
        .and_then(|h| h.get("totalCount"))
        .and_then(Value::as_u64);
    let (Some(nodes), Some(total)) = (nodes, total) else {
        return Some(UNKNOWN_ACTOR.to_string());
    };
    if total > nodes.len() as u64 {
        return Some(UNKNOWN_ACTOR.to_string());
    }
    nodes.iter().find_map(|n| {
        let at = n.get("editedAt").and_then(Value::as_str).unwrap_or("");
        let by = login_of(n);
        if !at.is_empty() && !changed_after(at, by, since, approver) {
            return None;
        }
        (by == NOBODY || !allowed(by)).then(|| shown(by))
    })
}

/// A login as a person reads it: [`UNKNOWN_ACTOR`] for no account. Only for
/// text; a rule compares the login itself.
fn shown(login: &str) -> String {
    if login == NOBODY {
        UNKNOWN_ACTOR
    } else {
        login
    }
    .to_string()
}

/// Whether `edit_holds` held only because the history it needed was a read
/// that failed: the last editor is one `allowed` takes, and the history is
/// `{"failed": true}`. The caller waits for a read rather than refuse.
pub fn edit_history_unread(edit: &Value, allowed: &dyn Fn(&str) -> bool) -> bool {
    let by = login_of(edit);
    !by.is_empty()
        && allowed(by)
        && edit.pointer("/userContentEdits/failed") == Some(&Value::Bool(true))
}

fn login_of(v: &Value) -> &str {
    v.get("editor")
        .and_then(|e| e.get("login"))
        .and_then(Value::as_str)
        .unwrap_or("")
}

/// The first `renamed` event after `since` (any of them when `since` is
/// empty) whose account is not allowed, or the veto of nobody: "unknown" for
/// a rename that names no account. None when no rename holds the title back.
/// After is as `changed_after` reads it. One with no time is not old: it
/// counts as after `since`, so an unstamped rename never reads as a title the
/// approval covered - the opposite of the rule for the events that must
/// authorise something, in the header. A stamp GitHub sent in a shape the
/// poll cannot read is read the same way, as no stamp: it is GitHub's to send
/// well, and the safe reading of one it did not is the one that holds the
/// start. The events come in GitHub's order, so the newest of them answers
/// first: the walk is from the back and stops at the first veto.
pub fn first_unallowed_rename(
    events: &[Event],
    since: &str,
    approver: &str,
    allowed: &dyn Fn(&str) -> bool,
) -> Option<String> {
    events.iter().rev().find_map(|e| {
        if e.event != "renamed"
            || e.at
                .as_deref()
                .is_some_and(|at| !changed_after(at, &e.actor, since, approver))
        {
            return None;
        }
        if e.actor == NOBODY {
            return Some(UNKNOWN_ACTOR.to_string());
        }
        (!allowed(&e.actor)).then(|| e.actor.clone())
    })
}

#[cfg(test)]
mod allowed_tests {
    use super::allowed_in;

    #[test]
    fn only_a_whole_login_of_the_list_is_allowed() {
        assert!(allowed_in("alice bob", "bob"));
        assert!(!allowed_in("alice bob", "bo"));
        assert!(!allowed_in("alice bob", "alice bob"));
        assert!(!allowed_in("alice bob", ""));
        assert!(!allowed_in("", ""));
    }

    /// The list is what lib/config.sh checks: whole logins with one space
    /// between them, exact case. The rule reads no other spelling as a login
    /// of it.
    #[test]
    fn the_rule_reads_the_list_as_lib_config_sh_writes_it() {
        // A login of the list, and one of no other case.
        assert!(allowed_in("alice", "alice"));
        assert!(!allowed_in("alice", "Alice"));
        // Two spaces stand for no login: the empty word between them names
        // nobody. The logins on either side of it are still whole logins.
        assert!(!allowed_in("alice  bob", ""));
        assert!(allowed_in("alice  bob", "alice"));
        // A list of no logins allows nobody.
        assert!(!allowed_in("", "alice"));
        // A login that holds a space is no login of a space-separated list.
        assert!(!allowed_in("alice bob", "alice bob"));
        assert!(!allowed_in("alice bob", " alice"));
        assert!(!allowed_in("alice bob", "alice "));
    }
}

#[cfg(test)]
mod parse_tests {
    use super::{Event, Events};
    use serde_json::json;

    fn labeled(actor: &str, at: serde_json::Value) -> serde_json::Value {
        json!({
            "event": "labeled",
            "label": {"name": "tehas:ready"},
            "actor": {"login": actor},
            "created_at": at,
        })
    }

    /// One rule for a stamp, the one `forge::gh::page_events` refuses by: a
    /// stamp that is there and is no text cannot be ordered, and an entry
    /// whose tie would be won by it is not the site's own unstamped event.
    #[test]
    fn a_stamp_that_is_no_text_makes_the_answer_unreadable() {
        for stamp in [json!(1), json!({"$date": 1}), json!([1]), json!(true)] {
            let v = json!([labeled("mallory", stamp.clone())]);
            assert_eq!(
                Events::parse(&v),
                Events::Unreadable,
                "{stamp} was read as a stamp"
            );
            // Among pages, one bad stamp unreadables the whole answer.
            let v = json!([
                labeled("alice", json!("2026-01-01T00:00:00Z")),
                [labeled("mallory", stamp.clone())]
            ]);
            assert_eq!(Events::parse(&v), Events::Unreadable, "{stamp}");
        }
    }

    /// The site's own answers of no time: no `created_at`, or one that is
    /// JSON null. Both are read as no stamp, the oldest of the page.
    #[test]
    fn an_absent_or_null_stamp_is_still_read_as_no_stamp() {
        let mut no_stamp = labeled("mallory", json!(null));
        let v = json!([
            no_stamp.clone(),
            labeled("alice", json!("2026-01-01T00:00:00Z"))
        ]);
        let mallory = Event {
            event: "labeled".into(),
            label: "tehas:ready".into(),
            actor: "mallory".into(),
            at: None,
        };
        let want = Events::Read(vec![
            mallory.clone(),
            Event {
                event: "labeled".into(),
                label: "tehas:ready".into(),
                actor: "alice".into(),
                at: Some("2026-01-01T00:00:00Z".into()),
            },
        ]);
        assert_eq!(Events::parse(&v), want);
        no_stamp.as_object_mut().unwrap().remove("created_at");
        let v = json!([no_stamp]);
        assert_eq!(Events::parse(&v), Events::Read(vec![mallory]));
    }
}

#[cfg(test)]
mod plan_tests {
    use super::{plan_credit, sole_application, Event};

    fn event(event: &str, label: &str, actor: &str, at: Option<&str>) -> Event {
        Event {
            event: event.into(),
            label: label.into(),
            actor: actor.into(),
            at: at.map(str::to_string),
        }
    }

    const T: &str = "2026-10-01T00:00:00Z";
    const L: &str = "2026-10-01T00:20:00Z";

    fn credit(events: &[Event], by: &str) -> Option<String> {
        plan_credit(events, by, &|w| w == "alice" || w == "mill", &|w| {
            w == "mill"
        })
    }

    #[test]
    fn a_plan_is_credited_to_the_allowed_person_the_events_name() {
        let ev = [event("labeled", "tehas:plan", "alice", Some(T))];
        assert_eq!(credit(&ev, "alice").as_deref(), Some(T));
    }

    #[test]
    fn nobody_else_is_credited() {
        let alice = [event("labeled", "tehas:plan", "alice", Some(T))];
        assert_eq!(credit(&alice, "bob"), None);
        assert_eq!(credit(&alice, ""), None);
        assert_eq!(credit(&[], "alice"), None);
        // Allowed, and the millwright all the same.
        let mill = [event("labeled", "tehas:plan", "mill", Some(T))];
        assert_eq!(credit(&mill, "mill"), None);
        // On record, and the events name another account.
        let bob = [event("labeled", "tehas:plan", "bob", Some(T))];
        assert_eq!(credit(&bob, "alice"), None);
        // The last application decides, and a refine is no plan.
        let later = [
            event("labeled", "tehas:plan", "alice", Some(T)),
            event("labeled", "tehas:plan", "bob", Some(L)),
        ];
        assert_eq!(credit(&later, "alice"), None);
        let refine = [event("labeled", "tehas:refine", "alice", Some(T))];
        assert_eq!(credit(&refine, "alice"), None);
    }

    #[test]
    fn the_bots_one_application_since_is_sole() {
        let bot = |w: &str| w == "bot";
        let once = [
            event(
                "labeled",
                "tehas:ready",
                "alice",
                Some("2026-09-01T00:00:00Z"),
            ),
            event(
                "unlabeled",
                "tehas:ready",
                "alice",
                Some("2026-09-02T00:00:00Z"),
            ),
            event("labeled", "tehas:ready", "bot", Some(L)),
        ];
        assert!(sole_application(&once, "tehas:ready", T, &bot));
        assert!(!sole_application(&[], "tehas:ready", T, &bot));
        let again = [
            event("labeled", "tehas:ready", "bot", Some(L)),
            event("unlabeled", "tehas:ready", "alice", Some(L)),
            event(
                "labeled",
                "tehas:ready",
                "bot",
                Some("2026-10-02T00:00:00Z"),
            ),
        ];
        assert!(!sole_application(&again, "tehas:ready", T, &bot));
        let theirs = [event("labeled", "tehas:ready", "mallory", Some(L))];
        assert!(!sole_application(&theirs, "tehas:ready", T, &bot));
        // An event with no time is later than any when it must veto.
        let unstamped = [
            event("labeled", "tehas:ready", "bot", Some(L)),
            event("unlabeled", "tehas:ready", "mallory", None),
        ];
        assert!(!sole_application(&unstamped, "tehas:ready", T, &bot));
    }
}

#[cfg(test)]
mod standing_tests {
    use super::{label_actor, standing_actor, standing_past_bot, Event};

    fn event(event: &str, actor: &str, at: Option<&str>) -> Event {
        Event {
            event: event.into(),
            label: "tehas:ready".into(),
            actor: actor.into(),
            at: at.map(str::to_string),
        }
    }

    const T: &str = "2026-10-01T00:00:00Z";
    const L: &str = "2026-10-01T00:01:00Z";

    fn who(events: &[Event]) -> (String, String) {
        standing_actor(events, "tehas:ready")
    }

    fn none() -> (String, String) {
        (String::new(), String::new())
    }

    /// The issue's labels still list the label and the events say it was
    /// taken off since: the earlier application grants nothing.
    #[test]
    fn an_application_taken_off_since_stands_for_nobody() {
        let ev = [
            event("labeled", "alice", Some(T)),
            event("unlabeled", "alice", Some(L)),
        ];
        assert_eq!(label_actor(&ev, "tehas:ready").0, "alice");
        assert_eq!(who(&ev), none());
        let by_other = [
            event("labeled", "alice", Some(T)),
            event("unlabeled", "mallory", Some(L)),
        ];
        assert_eq!(who(&by_other), none());
    }

    #[test]
    fn an_application_stands_when_nothing_took_it_off() {
        let once = [event("labeled", "alice", Some(T))];
        assert_eq!(who(&once), ("alice".into(), T.into()));
        let bare = [event("labeled", "alice", None)];
        assert_eq!(who(&bare), ("alice".into(), String::new()));
        assert_eq!(who(&[]), none());
        let again = [
            event("labeled", "mallory", Some(T)),
            event("unlabeled", "alice", Some(T)),
            event("labeled", "alice", Some(L)),
        ];
        assert_eq!(who(&again), ("alice".into(), L.into()));
    }

    /// The order is `label_state`'s: of two events in one second the removal
    /// is last, and an event with no stamp is older than any.
    #[test]
    fn ties_and_missing_stamps_are_read_as_label_state_reads_them() {
        let same = [
            event("unlabeled", "alice", Some(T)),
            event("labeled", "alice", Some(T)),
        ];
        assert_eq!(who(&same), none());
        let unstamped_removal = [
            event("labeled", "alice", Some(T)),
            event("unlabeled", "mallory", None),
        ];
        assert_eq!(who(&unstamped_removal), ("alice".into(), T.into()));
        let unstamped_application = [
            event("unlabeled", "alice", Some(T)),
            event("labeled", "alice", None),
        ];
        assert_eq!(who(&unstamped_application), none());
        let two = [
            event("labeled", "alice", Some(T)),
            event("labeled", "bob", Some(T)),
        ];
        assert_eq!(who(&two), label_actor(&two, "tehas:ready"));
        assert_eq!(who(&two).0, "bob");
    }

    /// #2605: tehas's own removal leaves the application standing; anyone
    /// else's ends it, as `standing_actor` reads it.
    #[test]
    fn past_the_bot_only_the_factorys_removal_is_passed_over() {
        let past = |ev: &[Event]| standing_past_bot(ev, "tehas:ready", "tehas-bot");
        let by_bot = [
            event("labeled", "alice", Some(T)),
            event("unlabeled", "tehas-bot[bot]", Some(L)),
        ];
        assert_eq!(past(&by_bot), ("alice".into(), T.into()));
        assert_eq!(who(&by_bot), none());
        for by in ["alice", "mallory", ""] {
            let ev = [
                event("labeled", "alice", Some(T)),
                event("unlabeled", "tehas-bot", Some(T)),
                event("unlabeled", by, Some(L)),
            ];
            assert_eq!(past(&ev), none(), "{by}");
        }
        // With no bot named, no removal is passed over.
        assert_eq!(standing_past_bot(&by_bot, "tehas:ready", ""), none());
        assert_eq!(past(&[]), none());
    }

    #[test]
    fn another_label_does_not_take_this_one_off() {
        let mut other = event("unlabeled", "mallory", Some(L));
        other.label = "tehas:blocked".into();
        let ev = [event("labeled", "alice", Some(T)), other];
        assert_eq!(who(&ev), ("alice".into(), T.into()));
    }
}

#[cfg(test)]
mod edit_tests {
    use super::{edit_history_unread, edit_holds, UNKNOWN_ACTOR};
    use serde_json::json;

    const SINCE: &str = "2026-01-02T10:00:00Z";
    const AFTER: &str = "2026-01-02T10:05:00Z";

    fn held(edit: &serde_json::Value) -> Option<String> {
        edit_holds(edit, SINCE, "alice", &|w| w == "alice")
    }

    /// GitHub stamps every saved edit: one with no stamp is no edit.
    #[test]
    fn an_unstamped_edit_is_no_edit() {
        assert_eq!(
            held(&json!({"lastEditedAt": null, "editor": {"login": "mallory"}})),
            None
        );
        assert_eq!(held(&json!({"editor": {"login": "mallory"}})), None);
        assert_eq!(
            held(&json!({"lastEditedAt": "", "editor": {"login": "mallory"}})),
            None
        );
    }

    /// The record of an issue last edited by `last` after the approval, with
    /// a history of `edits` (editedAt, editor) and `total` edits in all.
    fn record(last: &str, edits: &[(&str, &str)], total: usize) -> serde_json::Value {
        let nodes: Vec<_> = edits
            .iter()
            .map(|(at, who)| json!({"editedAt": at, "editor": {"login": who}}))
            .collect();
        json!({"lastEditedAt": AFTER, "editor": {"login": last},
            "userContentEdits": {"totalCount": total, "nodes": nodes}})
    }

    #[test]
    fn an_edit_after_the_approval_holds_unless_allowed() {
        assert_eq!(
            held(&record("mallory", &[(AFTER, "mallory")], 1)).as_deref(),
            Some("mallory")
        );
        assert_eq!(held(&record("alice", &[(AFTER, "alice")], 1)), None);
        let nobody = json!({"lastEditedAt": AFTER, "editor": null});
        assert_eq!(held(&nobody).as_deref(), Some(UNKNOWN_ACTOR));
    }

    /// An allowed account that saves the body after an outside one does not
    /// clear the outside edit.
    #[test]
    fn an_outside_edit_followed_by_an_allowed_one_holds() {
        let edits = [("2026-01-02T10:01:00Z", "mallory"), (AFTER, "alice")];
        assert_eq!(
            held(&record("alice", &edits, 2)).as_deref(),
            Some("mallory")
        );
        // Newest first, as GitHub may list them.
        let edits = [(AFTER, "alice"), ("2026-01-02T10:01:00Z", "mallory")];
        assert_eq!(
            held(&record("alice", &edits, 2)).as_deref(),
            Some("mallory")
        );
    }

    #[test]
    fn only_allowed_edits_after_the_approval_start() {
        let edits = [
            ("2026-01-01T00:00:00Z", "mallory"),
            ("2026-01-02T09:59:00Z", "mallory"),
            ("2026-01-02T10:01:00Z", "alice"),
            (AFTER, "alice"),
        ];
        assert_eq!(held(&record("alice", &edits, 4)), None);
    }

    #[test]
    fn a_history_that_may_be_cut_or_cannot_be_read_holds() {
        let unknown = Some(UNKNOWN_ACTOR);
        // Fewer nodes than GitHub counts: the edits left out may be after.
        assert_eq!(
            held(&record("alice", &[(AFTER, "alice")], 101)).as_deref(),
            unknown
        );
        // No history read at all.
        let bare = json!({"lastEditedAt": AFTER, "editor": {"login": "alice"}});
        assert_eq!(held(&bare).as_deref(), unknown);
        let failed = json!({"lastEditedAt": AFTER, "editor": {"login": "alice"},
            "userContentEdits": {"failed": true}});
        assert_eq!(held(&failed).as_deref(), unknown);
        // An edit that names no editor, or carries no stamp.
        let mut r = record("alice", &[(AFTER, "alice")], 1);
        r["userContentEdits"]["nodes"][0]["editor"] = json!(null);
        assert_eq!(held(&r).as_deref(), unknown);
        let mut r = record("alice", &[(AFTER, "mallory")], 1);
        r["userContentEdits"]["nodes"][0]["editedAt"] = json!(null);
        assert_eq!(held(&r).as_deref(), Some("mallory"));
    }

    /// A list may name a GitHub account "unknown": an edit by no account still
    /// holds, as the last edit and as a node of the history.
    #[test]
    fn an_edit_by_no_account_holds_with_unknown_on_the_list() {
        let list = |w: &str| w == "alice" || w == UNKNOWN_ACTOR;
        let held = |e: &serde_json::Value| edit_holds(e, SINCE, "alice", &list);
        let nobody = json!({"lastEditedAt": AFTER, "editor": null});
        assert_eq!(held(&nobody).as_deref(), Some(UNKNOWN_ACTOR));
        let mut r = record("alice", &[(AFTER, "alice")], 1);
        r["userContentEdits"]["nodes"][0]["editor"] = json!(null);
        assert_eq!(held(&r).as_deref(), Some(UNKNOWN_ACTOR));
        let mut r = record("alice", &[(AFTER, "alice")], 1);
        r["userContentEdits"]["nodes"][0]["editor"] = json!({"login": ""});
        assert_eq!(held(&r).as_deref(), Some(UNKNOWN_ACTOR));
        // The account named "unknown" is allowed like any other.
        let named = record(UNKNOWN_ACTOR, &[(AFTER, UNKNOWN_ACTOR)], 1);
        assert_eq!(held(&named), None);
        assert!(!edit_history_unread(&nobody, &list));
    }

    /// A last edit before the approval needs no history: every edit is older.
    #[test]
    fn a_last_edit_before_the_approval_reads_no_history() {
        let before = json!({"lastEditedAt": "2026-01-02T09:59:00Z", "editor": {"login": "alice"}});
        assert_eq!(held(&before), None);
    }

    #[test]
    fn an_edit_before_the_approval_is_the_text_it_covered() {
        let before =
            json!({"lastEditedAt": "2026-01-02T09:59:00Z", "editor": {"login": "mallory"}});
        assert_eq!(held(&before), None);
    }

    /// An empty `since` has every edit after it, but still no unstamped one.
    #[test]
    fn with_no_approval_time_only_a_stamped_edit_holds() {
        let allowed = |w: &str| w == "alice";
        let unstamped = json!({"lastEditedAt": null, "editor": {"login": "mallory"}});
        assert_eq!(edit_holds(&unstamped, "", "alice", &allowed), None);
        let stamped = json!({"lastEditedAt": SINCE, "editor": {"login": "mallory"}});
        assert_eq!(
            edit_holds(&stamped, "", "alice", &allowed).as_deref(),
            Some("mallory")
        );
    }

    /// GitHub stamps to the second: an edit in the approval's second holds
    /// unless the approver made it.
    #[test]
    fn an_edit_in_the_approval_second_holds_unless_the_approver_made_it() {
        let by = |who: &str| json!({"lastEditedAt": SINCE, "editor": {"login": who}});
        assert_eq!(held(&by("mallory")).as_deref(), Some("mallory"));
        let only_bob = |w: &str| w == "bob";
        assert_eq!(edit_holds(&by("alice"), SINCE, "alice", &only_bob), None);
    }
}

#[cfg(test)]
mod renamed_tests {
    use super::{allowed_in, changed_after, first_unallowed_rename, Event, UNKNOWN_ACTOR};

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
        first_unallowed_rename(events, since, "alice", &|w| w == "alice")
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
    }

    /// GitHub stamps to the second: a rename in the approval's second may
    /// have come after the label, so one by an account outside the list holds
    /// the start.
    #[test]
    fn a_rename_in_the_second_of_the_approval_counts_as_after_it() {
        let same = [event("renamed", "mallory", Some(SINCE))];
        assert_eq!(vetoed(&same, SINCE).as_deref(), Some("mallory"));
        let nobody = [event("renamed", "", Some(SINCE))];
        assert_eq!(vetoed(&nobody, SINCE).as_deref(), Some(UNKNOWN_ACTOR));
        let mine = [event("renamed", "alice", Some(SINCE))];
        assert_eq!(vetoed(&mine, SINCE), None);
    }

    /// A list may name a GitHub account called unknown: a rename by no
    /// account is still nobody's, and the account's own is allowed.
    #[test]
    fn a_rename_by_no_account_is_not_the_account_unknown() {
        let listed = |w: &str| allowed_in("alice unknown", w);
        for at in [None, Some(SINCE), Some("2026-01-02T10:01:00Z")] {
            let nobody = [event("renamed", "", at)];
            let got = first_unallowed_rename(&nobody, SINCE, "alice", &listed);
            assert_eq!(got.as_deref(), Some(UNKNOWN_ACTOR), "{at:?}");
            let theirs = [event("renamed", "unknown", at)];
            assert_eq!(
                first_unallowed_rename(&theirs, SINCE, "alice", &listed),
                None
            );
        }
    }

    /// The approver's own rename in that second is the title it approved,
    /// even when the approver is not on the list (the bot of a plan's start).
    #[test]
    fn the_approvers_own_rename_in_its_second_is_before_it() {
        let bot = [event("renamed", "tehas-bot[bot]", Some(SINCE))];
        let by_bot = first_unallowed_rename(&bot, SINCE, "tehas-bot", &|w| w == "alice");
        assert_eq!(by_bot, None);
        let by_alice = first_unallowed_rename(&bot, SINCE, "alice", &|w| w == "alice");
        assert_eq!(by_alice.as_deref(), Some("tehas-bot[bot]"));
    }

    #[test]
    fn a_change_in_the_second_of_an_approval_is_after_it_unless_the_approvers() {
        const AFTER: &str = "2026-01-02T10:00:01Z";
        const BEFORE: &str = "2026-01-02T09:59:59Z";
        assert!(changed_after(SINCE, "mallory", SINCE, "alice"));
        assert!(changed_after(SINCE, "", SINCE, "alice"));
        assert!(changed_after(SINCE, "", SINCE, ""));
        assert!(changed_after(SINCE, "alice", SINCE, ""));
        assert!(!changed_after(SINCE, "alice", SINCE, "alice"));
        assert!(!changed_after(SINCE, "bot[bot]", SINCE, "bot"));
        assert!(changed_after(AFTER, "alice", SINCE, "alice"));
        assert!(!changed_after(BEFORE, "mallory", SINCE, "alice"));
        assert!(changed_after(BEFORE, "mallory", "", "alice"));
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
