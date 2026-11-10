//! Grants by label are pinned to the label's identity.
//!
//! A label event names a label by its current name, so an account that may
//! manage the repository's labels could rename another label a person applied
//! to `tehas:ready`, and the person's old application would read as an
//! approval. The poll keeps the GitHub node id of each granting `tehas:` label
//! in `$TEHAS_HOME/state/labels`, one `<name>\t<node id>` line each, and an
//! application of a granting label counts only when its label is the pinned
//! one:
//!
//! - The first page of the queue names the repository's `tehas:` labels with
//!   their ids. A granting label with no pin is pinned as it is now (first
//!   sight); one whose id is not the pinned one grants nothing this poll, in
//!   any event read of it, and the poll says so with both ids.
//! - An application read from GraphQL carries its label's id, and counts only
//!   when that id is the pinned one. Without a pin for its name the first id
//!   seen is pinned.
//! - An application that does not count keeps its place in the timeline, so
//!   the label still reads as applied, with `unknown` as its account: no rule
//!   takes it as anyone's approval.
//!
//! A person who deletes and recreates a `tehas:` label removes its line from
//! the file (or the file) to pin the new one at the next poll.

use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use tehas_core::events::UNKNOWN_ACTOR;

/// The labels whose application the dispatcher takes as a person's word to
/// start, plan or order work.
pub const GRANTING: [&str; 7] = [
    "tehas:ready",
    "tehas:plan",
    "tehas:refine",
    "tehas:finish",
    "tehas:urgent",
    "tehas:next",
    "tehas:lean",
];

/// Where the pins are kept, under TEHAS_HOME.
pub const FILE: &str = "state/labels";

/// Label name to node id.
pub type Pins = BTreeMap<String, String>;

/// The pins on record. A missing or unreadable file is none; a line of no
/// `<name>\t<id>` shape is skipped.
pub fn load(home: &Path) -> Pins {
    let text = fs::read_to_string(home.join(FILE)).unwrap_or_default();
    text.lines()
        .filter_map(|l| {
            let (name, id) = l.split_once('\t')?;
            (!name.is_empty() && !id.is_empty() && !id.contains('\t'))
                .then(|| (name.to_string(), id.to_string()))
        })
        .collect()
}

fn save(home: &Path, pins: &Pins) -> io::Result<()> {
    let file = home.join(FILE);
    let new = file.with_extension("new");
    let _ = fs::remove_file(&new);
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&new)?;
    for (name, id) in pins {
        writeln!(f, "{name}\t{id}")?;
    }
    f.sync_all()?;
    fs::rename(&new, &file)
}

/// What judging a poll's read found.
#[derive(Debug, Default, PartialEq)]
pub struct Judged {
    /// Pins made this poll, at first sight.
    pub pinned: Vec<(String, String)>,
    /// The granting labels whose current id is not the pinned one.
    pub refused: BTreeSet<String>,
    /// One line for each refusal.
    pub said: Vec<String>,
}

fn granting(name: &str) -> bool {
    GRANTING.contains(&name)
}

/// Judges the snapshot `snap` against `pins`, adding the pins it makes: the
/// current labels (`tehas_labels`) first, then each application in the
/// issues' and the control issue's events, which a refused one leaves with
/// `unknown` as its account.
pub fn judge(pins: &mut Pins, snap: &mut Value) -> Judged {
    let mut j = Judged::default();
    for l in snap
        .get("tehas_labels")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
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
            None => {
                pins.insert(name.to_string(), id.to_string());
                j.pinned.push((name.to_string(), id.to_string()));
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
            judge_event(pins, &mut j, &mut said, &n, e);
        });
    }
    if let Some(ev) = snap.pointer_mut("/control/events") {
        each_event(ev, &mut |e| {
            judge_event(pins, &mut j, &mut said, "the control issue", e);
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

fn judge_event(pins: &mut Pins, j: &mut Judged, said: &mut Vec<String>, at: &str, e: &mut Value) {
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
    } else if id.is_empty() {
        false
    } else {
        match pins.get(&name) {
            None => {
                pins.insert(name.clone(), id.clone());
                j.pinned.push((name.clone(), id.clone()));
                false
            }
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
    let j = judge(&mut pins, snap);
    let mut said = Vec::new();
    for (name, id) in &j.pinned {
        let word = if dry { "would pin" } else { "pinned" };
        said.push(format!("{word} the label {name} as {id} in {FILE}"));
    }
    if !dry && !j.pinned.is_empty() {
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
        let j = judge(&mut pins, &mut s);
        assert_eq!(actor(&s), "alice");
        assert!(j.said.is_empty() && j.pinned.is_empty() && j.refused.is_empty());
    }

    // A person's `bug` renamed to tehas:ready: its events name tehas:ready
    // with the id of the old label.
    #[test]
    fn another_label_renamed_to_ready_grants_nothing() {
        let mut pins = Pins::from([("tehas:ready".into(), "LA_real".into())]);
        let mut s = snap(json!([]), json!([ready("LA_bug", "alice")]));
        let j = judge(&mut pins, &mut s);
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
        let j = judge(&mut pins, &mut s);
        assert_eq!(actor(&s), UNKNOWN_ACTOR);
        assert_eq!(j.refused, BTreeSet::from(["tehas:ready".to_string()]));
        assert!(j.said[0].contains("pinned LA_real, now LA_bug"));
    }

    #[test]
    fn with_no_pin_the_label_is_pinned_on_first_sight() {
        let mut pins = Pins::new();
        let mut s = snap(
            json!([{"name": "tehas:ready", "id": "LA_real"}, {"name": "tehas:running", "id": "LA_x"}]),
            json!([ready("LA_real", "alice")]),
        );
        let j = judge(&mut pins, &mut s);
        assert_eq!(actor(&s), "alice");
        assert_eq!(j.pinned, vec![("tehas:ready".into(), "LA_real".into())]);
        assert_eq!(pins.len(), 1);
        // And without the current labels, from the first event seen.
        let mut pins = Pins::new();
        let mut s = snap(Value::Null, json!([ready("LA_real", "alice")]));
        judge(&mut pins, &mut s);
        assert_eq!(pins["tehas:ready"], "LA_real");
    }

    #[test]
    fn labels_that_grant_nothing_are_not_judged() {
        let mut pins = Pins::from([("tehas:pause".into(), "LA_p".into())]);
        let pause = json!({"event": "labeled", "label": {"name": "tehas:pause", "id": "LA_other"},
            "actor": {"login": "alice"}});
        let mut s = json!({"control": {"events": [pause]}});
        let j = judge(&mut pins, &mut s);
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
}
