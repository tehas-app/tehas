//! Grants by label are pinned to the label's identity.
//!
//! A label event names a label by its current name, so an account that may
//! manage the repository's labels could rename another label a person applied
//! to `tehas:ready`, and the person's old application would read as an
//! approval. The pins keep the GitHub node id of each granting `tehas:` label
//! in `$TEHAS_HOME/state/labels`, one `<name>\t<node id>` line each, and an
//! application of a granting label counts only when its label is the pinned
//! one.
//!
//! `tehas labels` makes the labels and then pins each granting one it finds
//! with no pin (`pin_made`), so a host that ran it has a pin for every
//! granting label. The poll pins only when the file does not exist yet: a host
//! from before the pins. Its first poll that reads the labels pins the
//! granting labels it finds and writes the file, even with no pin in it.
//! From then on the poll pins nothing:
//!
//! - The first page of the queue names the repository's `tehas:` labels with
//!   their ids. A granting label whose id is not the pinned one, or that has
//!   no pin, grants nothing this poll, in any event read of it, and the poll
//!   says so. A label missing when the pins were made has no pin, so a label
//!   renamed to its name later grants nothing either.
//! - An application read from GraphQL carries its label's id, and counts only
//!   when that id is the pinned one. An application of a granting label with
//!   no pin counts for nothing once the file exists.
//! - An application that does not count keeps its place in the timeline, so
//!   the label still reads as applied, with `unknown` as its account: no rule
//!   takes it as anyone's approval.
//!
//! A person who deletes and recreates a `tehas:` label removes its line from
//! the file and runs `tehas labels`, which pins the label as it now is.

use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;
use tehas_core::events::UNKNOWN_ACTOR;
use tehas_core::label_pins::{granting, save};
pub use tehas_core::label_pins::{load, pin_made, Pins, FILE, GRANTING};

/// What judging a poll's read found.
#[derive(Debug, Default, PartialEq)]
pub struct Judged {
    /// Pins made this poll, at first sight.
    pub pinned: Vec<(String, String)>,
    /// The granting labels whose current id is not the pinned one.
    pub refused: BTreeSet<String>,
    /// One line for each refusal.
    pub said: Vec<String>,
    /// The current labels were read.
    pub read: bool,
}

/// Judges the snapshot `snap` against `pins`: the current labels
/// (`tehas_labels`) first, then each application in the issues' and the
/// control issue's events, which a refused one leaves with `unknown` as its
/// account. Only when the pins are not `kept` yet does it pin, and then only
/// the current labels: an event pins nothing.
pub fn judge(pins: &mut Pins, kept: bool, snap: &mut Value) -> Judged {
    let mut j = Judged::default();
    let current = snap.get("tehas_labels").and_then(Value::as_array).cloned();
    j.read = current.is_some();
    for l in current.iter().flatten() {
        let (Some(name), Some(id)) = (
            l.get("name").and_then(Value::as_str),
            l.get("id").and_then(Value::as_str),
        ) else {
            continue;
        };
        if !granting(name) || id.is_empty() {
            continue;
        }
        match pins.get(name) {
            None if !kept => {
                pins.insert(name.to_string(), id.to_string());
                j.pinned.push((name.to_string(), id.to_string()));
            }
            None => {
                j.refused.insert(name.to_string());
                j.said.push(format!(
                    "refused: the label {name} ({id}) has no pin in {FILE}: no application of it counts until a person pins it (see tehas labels)"
                ));
            }
            Some(p) if p != id => {
                j.refused.insert(name.to_string());
                j.said.push(format!(
                    "refused: the label {name} is not the one pinned in {FILE} (pinned {p}, now {id}): no application of it counts"
                ));
            }
            Some(_) => {}
        }
    }
    // Once the pins are kept, a pinned label the read did not name (gone, or
    // pushed off the page by other labels), or every pinned label when the
    // labels could not be read, is refused this poll: an event read without
    // label ids, from REST, could not tell it from another label renamed to
    // its name.
    if kept {
        let named: BTreeSet<&str> = current
            .iter()
            .flatten()
            .filter_map(|l| l.get("name").and_then(Value::as_str))
            .collect();
        for name in pins.keys() {
            if granting(name) && !named.contains(name.as_str()) && j.refused.insert(name.clone()) {
                j.said.push(format!(
                    "refused: the pinned label {name} was not among the labels this poll read: no application of it counts this poll"
                ));
            }
        }
    }
    let mut lists: Vec<(String, &mut Value)> = Vec::new();
    if let Some(issues) = snap.get_mut("issues").and_then(Value::as_object_mut) {
        for (n, i) in issues.iter_mut() {
            if let Some(ev) = i.get_mut("events") {
                lists.push((format!("#{n}"), ev));
            }
        }
    }
    let mut said = Vec::new();
    for (n, ev) in lists {
        each_event(ev, &mut |e| {
            judge_event(pins, kept, &mut j, &mut said, &n, e);
        });
    }
    if let Some(ev) = snap.pointer_mut("/control/events") {
        each_event(ev, &mut |e| {
            judge_event(pins, kept, &mut j, &mut said, "the control issue", e);
        });
    }
    j.said.extend(said);
    j
}

/// Each event of a list, or of a list of pages.
fn each_event(v: &mut Value, f: &mut dyn FnMut(&mut Value)) {
    if let Some(a) = v.as_array_mut() {
        for e in a {
            if e.is_array() {
                each_event(e, f);
            } else {
                f(e);
            }
        }
    }
}

fn judge_event(
    pins: &Pins,
    kept: bool,
    j: &mut Judged,
    said: &mut Vec<String>,
    at: &str,
    e: &mut Value,
) {
    if e.get("event").and_then(Value::as_str) != Some("labeled") {
        return;
    }
    let Some(name) = e.pointer("/label/name").and_then(Value::as_str) else {
        return;
    };
    if !granting(name) {
        return;
    }
    let name = name.to_string();
    let id = e
        .pointer("/label/id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let refuse = if j.refused.contains(&name) {
        true
    } else {
        match pins.get(&name) {
            // A host from before the pins whose labels could not be read this
            // poll: as before the pins.
            None if !kept => false,
            None => {
                said.push(format!(
                    "refused: an application of {name} on {at} is of a label with no pin in {FILE}"
                ));
                true
            }
            Some(_) if id.is_empty() => false,
            Some(p) if *p != id => {
                said.push(format!(
                    "refused: an application of {name} on {at} is of another label than the one pinned in {FILE} (pinned {p}, event {id})"
                ));
                true
            }
            Some(_) => false,
        }
    };
    if refuse {
        e["actor"] = json!({"login": UNKNOWN_ACTOR});
    }
}

/// The poll's step: judges the snapshot and keeps the new pins, unless `dry`.
/// The lines to log, and the labels refused this poll.
pub fn settle(home: &Path, snap: &mut Value, dry: bool) -> (Vec<String>, Vec<String>) {
    let mut pins = load(home);
    let kept = home.join(FILE).exists();
    let j = judge(&mut pins, kept, snap);
    let mut said = Vec::new();
    for (name, id) in &j.pinned {
        let word = if dry { "would pin" } else { "pinned" };
        said.push(format!("{word} the label {name} as {id} in {FILE}"));
    }
    if !dry && (!j.pinned.is_empty() || (!kept && j.read)) {
        if let Err(e) = save(home, &pins) {
            said.push(format!(
                "WARN: the label pins could not be kept in {FILE}: {e}"
            ));
        }
    }
    said.extend(j.said);
    (said, j.refused.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tehas_core::testdir::TestDir;

    fn snap(labels: Value, events: Value) -> Value {
        json!({"tehas_labels": labels, "issues": {"7": {"events": events}}})
    }

    fn ready(id: &str, by: &str) -> Value {
        json!({"event": "labeled", "label": {"name": "tehas:ready", "id": id},
            "actor": {"login": by}, "created_at": "2026-10-02T00:00:00Z"})
    }

    fn actor(s: &Value) -> &str {
        s.pointer("/issues/7/events/0/actor/login")
            .and_then(Value::as_str)
            .unwrap()
    }

    #[test]
    fn the_real_label_grants_as_before() {
        let mut pins = Pins::from([("tehas:ready".into(), "LA_real".into())]);
        let mut s = snap(
            json!([{"name": "tehas:ready", "id": "LA_real"}]),
            json!([ready("LA_real", "alice")]),
        );
        let j = judge(&mut pins, true, &mut s);
        assert_eq!(actor(&s), "alice");
        assert!(j.said.is_empty() && j.pinned.is_empty() && j.refused.is_empty());
    }

    // A person's `bug` renamed to tehas:ready: its events name tehas:ready
    // with the id of the old label.
    #[test]
    fn another_label_renamed_to_ready_grants_nothing() {
        let mut pins = Pins::from([("tehas:ready".into(), "LA_real".into())]);
        let mut s = snap(
            json!([{"name": "tehas:ready", "id": "LA_real"}]),
            json!([ready("LA_bug", "alice")]),
        );
        let j = judge(&mut pins, true, &mut s);
        assert_eq!(actor(&s), UNKNOWN_ACTOR);
        assert_eq!(j.said.len(), 1);
        assert!(j.said[0].contains("tehas:ready") && j.said[0].contains("#7"));
        assert!(j.said[0].contains("pinned LA_real") && j.said[0].contains("event LA_bug"));
        assert_eq!(pins["tehas:ready"], "LA_real");
    }

    // The real one renamed away and the other renamed in: the label now
    // called tehas:ready is not the pinned one, so no read of it counts, not
    // even one with no id.
    #[test]
    fn a_current_label_that_is_not_the_pinned_one_refuses_every_application() {
        let mut pins = Pins::from([("tehas:ready".into(), "LA_real".into())]);
        let no_id = json!({"event": "labeled", "label": {"name": "tehas:ready"},
            "actor": {"login": "alice"}});
        let mut s = snap(
            json!([{"name": "tehas:ready", "id": "LA_bug"}]),
            json!([no_id]),
        );
        let j = judge(&mut pins, true, &mut s);
        assert_eq!(actor(&s), UNKNOWN_ACTOR);
        assert_eq!(j.refused, BTreeSet::from(["tehas:ready".to_string()]));
        assert!(j.said[0].contains("pinned LA_real, now LA_bug"));
    }

    #[test]
    fn with_no_file_the_current_labels_are_pinned_on_first_sight() {
        let mut pins = Pins::new();
        let mut s = snap(
            json!([{"name": "tehas:ready", "id": "LA_real"}, {"name": "tehas:running", "id": "LA_x"}]),
            json!([ready("LA_real", "alice")]),
        );
        let j = judge(&mut pins, false, &mut s);
        assert_eq!(actor(&s), "alice");
        assert_eq!(j.pinned, vec![("tehas:ready".into(), "LA_real".into())]);
        assert_eq!(pins.len(), 1);
        assert!(j.read);
    }

    // With no file and no label read this poll, an event pins nothing and
    // counts as before the pins.
    #[test]
    fn with_no_file_an_event_pins_nothing() {
        let mut pins = Pins::new();
        let mut s = snap(Value::Null, json!([ready("LA_real", "alice")]));
        let j = judge(&mut pins, false, &mut s);
        assert_eq!(actor(&s), "alice");
        assert!(pins.is_empty() && j.pinned.is_empty() && !j.read);
    }

    // tehas:plan was missing when the pins were made; a label a cycle renames
    // to tehas:plan later is not pinned on first sight, and grants nothing.
    #[test]
    fn once_the_file_is_kept_a_label_with_no_pin_grants_nothing() {
        let mut pins = Pins::from([("tehas:ready".into(), "LA_real".into())]);
        let plan = json!({"event": "labeled", "label": {"name": "tehas:plan", "id": "LA_bug"},
            "actor": {"login": "alice"}});
        let mut s = snap(
            json!([{"name": "tehas:plan", "id": "LA_bug"}, {"name": "tehas:ready", "id": "LA_real"}]),
            json!([plan]),
        );
        let j = judge(&mut pins, true, &mut s);
        assert_eq!(actor(&s), UNKNOWN_ACTOR);
        assert!(j.pinned.is_empty());
        assert!(!pins.contains_key("tehas:plan"));
        assert_eq!(j.refused, BTreeSet::from(["tehas:plan".to_string()]));
        assert!(j.said[0].contains("tehas:plan (LA_bug) has no pin"));
        // And an event of it, with the label not on the first page, and with
        // no id at all.
        for label in [
            json!({"name": "tehas:plan", "id": "LA_bug"}),
            json!({"name": "tehas:plan"}),
        ] {
            let e = json!({"event": "labeled", "label": label, "actor": {"login": "alice"}});
            let mut s = snap(
                json!([{"name": "tehas:ready", "id": "LA_real"}]),
                json!([e]),
            );
            let j = judge(&mut pins, true, &mut s);
            assert_eq!(actor(&s), UNKNOWN_ACTOR);
            assert!(j.said[0].contains("of a label with no pin"));
        }
    }

    // The real tehas:plan renamed away, `bug` renamed to tehas:plan and pushed
    // off the read's page: an event with no label id, as REST reads it, must
    // not count. Nor when the labels could not be read at all.
    #[test]
    fn a_pinned_label_the_read_did_not_name_is_refused() {
        let pins0 = Pins::from([("tehas:plan".into(), "LA_plan".into())]);
        let no_id = json!({"event": "labeled", "label": {"name": "tehas:plan"},
            "actor": {"login": "alice"}});
        for labels in [json!([{"name": "tehas:x", "id": "LA_x"}]), Value::Null] {
            let mut pins = pins0.clone();
            let mut s = snap(labels, json!([no_id.clone()]));
            let j = judge(&mut pins, true, &mut s);
            assert_eq!(actor(&s), UNKNOWN_ACTOR);
            assert_eq!(j.refused, BTreeSet::from(["tehas:plan".to_string()]));
            assert!(j.said[0].contains("was not among the labels this poll read"));
        }
        // Named by the read with its pinned id, the same event counts.
        let mut pins = pins0.clone();
        let mut s = snap(
            json!([{"name": "tehas:plan", "id": "LA_plan"}]),
            json!([no_id]),
        );
        let j = judge(&mut pins, true, &mut s);
        assert_eq!(actor(&s), "alice");
        assert!(j.refused.is_empty());
    }

    #[test]
    fn labels_that_grant_nothing_are_not_judged() {
        let mut pins = Pins::from([("tehas:pause".into(), "LA_p".into())]);
        let pause = json!({"event": "labeled", "label": {"name": "tehas:pause", "id": "LA_other"},
            "actor": {"login": "alice"}});
        let mut s = json!({"control": {"events": [pause]}});
        let j = judge(&mut pins, true, &mut s);
        assert_eq!(s["control"]["events"][0]["actor"]["login"], "alice");
        assert!(j.said.is_empty());
    }

    #[test]
    fn settle_keeps_new_pins_unless_dry_and_reads_them_back() {
        let d = TestDir::new("pins");
        fs::create_dir_all(d.path().join("state")).unwrap();
        let mut s = snap(json!([{"name": "tehas:ready", "id": "LA_real"}]), json!([]));
        let (said, _) = settle(d.path(), &mut s.clone(), true);
        assert!(said[0].starts_with("would pin the label tehas:ready as LA_real"));
        assert!(!d.path().join(FILE).exists());
        let (said, refused) = settle(d.path(), &mut s, false);
        assert!(said[0].starts_with("pinned the label tehas:ready as LA_real"));
        assert!(refused.is_empty());
        assert_eq!(load(d.path())["tehas:ready"], "LA_real");
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(d.path().join(FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        let mut s = snap(json!([{"name": "tehas:ready", "id": "LA_new"}]), json!([]));
        let (said, refused) = settle(d.path(), &mut s, false);
        assert_eq!(refused, vec!["tehas:ready".to_string()]);
        assert!(said[0].contains("pinned LA_real, now LA_new"));
        assert_eq!(load(d.path())["tehas:ready"], "LA_real");
    }

    #[test]
    fn with_no_file_a_poll_that_read_the_labels_writes_it_even_empty() {
        let d = TestDir::new("pins-empty");
        fs::create_dir_all(d.path().join("state")).unwrap();
        // Labels not read: nothing is written.
        settle(d.path(), &mut json!({}), false);
        assert!(!d.path().join(FILE).exists());
        settle(d.path(), &mut snap(json!([]), json!([])), false);
        assert_eq!(fs::read_to_string(d.path().join(FILE)).unwrap(), "");
        // From then on a label that appears is not pinned.
        let mut s = snap(json!([{"name": "tehas:ready", "id": "LA_late"}]), json!([]));
        let (said, refused) = settle(d.path(), &mut s, false);
        assert_eq!(refused, vec!["tehas:ready".to_string()]);
        assert!(said[0].contains("has no pin"));
        assert!(load(d.path()).is_empty());
    }
}
