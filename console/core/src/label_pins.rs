//! The pins of the granting `tehas:` labels and of `tehas:prototype`:
//! `$TEHAS_HOME/state/labels`, one `<name>\t<GitHub node id>` line each. The
//! dispatcher judges each label application against them
//! (tehas_dispatch::pins), the Protolab its label, and `tehas labels` pins the
//! labels it made (`pin_made`).

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

/// The labels whose application the dispatcher takes as a person's word to
/// start or plan work. A lone `tehas:blocked` starts an issue once its
/// blockers close, so it is one of them.
pub const GRANTING: [&str; 7] = [
    "tehas:ready",
    "tehas:plan",
    "tehas:refine",
    "tehas:finish",
    "tehas:blocked",
    "tehas:lean",
    "tehas:classic",
];

/// The labels `tehas labels` pins: the granting ones, and the label that
/// puts a ticket in the Protolab.
pub const PINNED: [&str; 8] = [
    "tehas:ready",
    "tehas:plan",
    "tehas:refine",
    "tehas:finish",
    "tehas:blocked",
    "tehas:lean",
    "tehas:classic",
    "tehas:prototype",
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

pub fn save(home: &Path, pins: &Pins) -> io::Result<()> {
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

/// Whether `name` is a granting label.
pub fn granting(name: &str) -> bool {
    GRANTING.contains(&name)
}

/// A label of [`PINNED`] as `tehas labels` saw it: its id before it made the
/// labels and after, None where GitHub had no such label, and when GitHub
/// says the label of `after` was created, in Unix seconds.
pub struct Seen<'a> {
    pub name: &'a str,
    pub before: Option<String>,
    pub after: Option<String>,
    pub created: Option<i64>,
}

/// How close in time two labels' creation must be for one `tehas labels`
/// run to have made both.
pub const SAME_RUN_SECS: i64 = 60;

/// `tehas labels`' step, after it made the labels. With no file yet (a fresh
/// install, or a host from before the pins) every label of [`PINNED`] found
/// is pinned, as the poll would pin it. Once the file exists, only a label this
/// run made, one there was none of before, is pinned: a label that was there
/// with no pin may be another label renamed to the name, and its name cannot
/// tell, so it is said and left refused for a person to pin by hand, unless
/// GitHub says it was created within [`SAME_RUN_SECS`] of a label that is
/// pinned and still that label: the run that made the pinned one made it too.
/// A label renamed keeps its creation time, so another label renamed to the
/// name is still refused. That is how a label added to [`PINNED`] after a host
/// pinned the others gets its pin. A pin is never changed here. The file is written even with no pin in it, so the poll
/// pins nothing after. The lines to print, and whether every label of
/// [`PINNED`] now has its pin.
pub fn pin_made(home: &Path, seen: &[Seen]) -> (Vec<String>, bool) {
    let kept = home.join(FILE).exists();
    let mut pins = load(home);
    let anchors: Vec<i64> = seen
        .iter()
        .filter(|s| s.after.is_some() && pins.get(s.name) == s.after.as_ref())
        .filter_map(|s| s.created)
        .collect();
    let same_run = |s: &Seen| {
        s.before == s.after
            && s.created
                .is_some_and(|c| anchors.iter().any(|a| (c - a).abs() <= SAME_RUN_SECS))
    };
    let mut said = Vec::new();
    let mut whole = true;
    for s in seen {
        let name = s.name;
        if !PINNED.contains(&name) {
            continue;
        }
        match (pins.get(name), &s.after) {
            (Some(p), Some(id)) if p != id => {
                whole = false;
                said.push(format!(
                    "not pinned: the label {name} is {id}, and {FILE} pins {p}: no application of it counts until the pin and the label agree"
                ));
            }
            (Some(_), _) => {}
            (None, Some(id)) if !kept || s.before.is_none() => {
                pins.insert(name.to_string(), id.clone());
                said.push(format!("pinned the label {name} as {id} in {FILE}"));
            }
            (None, Some(id)) if same_run(s) => {
                pins.insert(name.to_string(), id.clone());
                said.push(format!(
                    "pinned the label {name} as {id} in {FILE}: it was made with the pinned labels"
                ));
            }
            (None, Some(id)) => {
                whole = false;
                said.push(format!(
                    "not pinned: the label {name} ({id}) was there before with no pin, and its name cannot tell whether it is the label it should be: if it is, add the line \"{name}<tab>{id}\" to {FILE}; if not, rename or delete it and run tehas labels again"
                ));
            }
            (None, None) => {
                whole = false;
                said.push(format!("not pinned: the label {name} was not found"));
            }
        }
    }
    if let Some(dir) = home.join(FILE).parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Err(e) = save(home, &pins) {
        said.push(format!("the label pins could not be kept in {FILE}: {e}"));
        whole = false;
    }
    (said, whole)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdir::TestDir;

    fn seen<'a>(name: &'a str, before: Option<&str>, after: Option<&str>) -> Seen<'a> {
        made(name, before, after, None)
    }

    fn made<'a>(
        name: &'a str,
        before: Option<&str>,
        after: Option<&str>,
        created: Option<i64>,
    ) -> Seen<'a> {
        Seen {
            name,
            before: before.map(str::to_string),
            after: after.map(str::to_string),
            created,
        }
    }

    #[test]
    fn with_no_file_every_label_found_is_pinned() {
        let d = TestDir::new("pins-made-first");
        let (said, whole) = pin_made(
            d.path(),
            &[
                seen("tehas:ready", Some("LA_r"), Some("LA_r")),
                seen("tehas:plan", None, None),
                seen("tehas:running", None, Some("LA_x")),
                seen("tehas:prototype", None, Some("LA_p")),
            ],
        );
        assert!(!whole);
        assert_eq!(
            said[0],
            "pinned the label tehas:ready as LA_r in state/labels"
        );
        assert_eq!(said[1], "not pinned: the label tehas:plan was not found");
        assert_eq!(
            said[2],
            "pinned the label tehas:prototype as LA_p in state/labels"
        );
        assert_eq!(said.len(), 3);
        assert_eq!(
            load(d.path()),
            Pins::from([
                ("tehas:ready".into(), "LA_r".into()),
                ("tehas:prototype".into(), "LA_p".into())
            ])
        );
        // The Protolab's label is pinned and grants the dispatcher nothing.
        assert!(!granting("tehas:prototype"));
    }

    // tehas:plan has no pin and a label of that name is there: it may be a
    // person's `bug` a container renamed, so it is not pinned.
    #[test]
    fn once_the_file_exists_only_a_label_this_run_made_is_pinned() {
        let d = TestDir::new("pins-made-kept");
        fs::create_dir_all(d.path().join("state")).unwrap();
        fs::write(d.path().join(FILE), "tehas:ready\tLA_r\n").unwrap();
        let (said, whole) = pin_made(
            d.path(),
            &[
                seen("tehas:ready", Some("LA_other"), Some("LA_other")),
                seen("tehas:plan", Some("LA_bug"), Some("LA_bug")),
                seen("tehas:lean", None, Some("LA_lean")),
            ],
        );
        assert!(!whole);
        assert!(said[0].starts_with(
            "not pinned: the label tehas:ready is LA_other, and state/labels pins LA_r"
        ));
        assert!(said[1]
            .starts_with("not pinned: the label tehas:plan (LA_bug) was there before with no pin"));
        assert_eq!(
            said[2],
            "pinned the label tehas:lean as LA_lean in state/labels"
        );
        let pins = load(d.path());
        assert_eq!(pins["tehas:ready"], "LA_r");
        assert!(!pins.contains_key("tehas:plan"));
        let (said, whole) = pin_made(
            d.path(),
            &[
                seen("tehas:ready", Some("LA_r"), Some("LA_r")),
                seen("tehas:lean", Some("LA_lean"), Some("LA_lean")),
            ],
        );
        assert!(whole && said.is_empty());
    }

    // A host pinned the labels before tehas:blocked joined PINNED. The run
    // that made tehas:ready made tehas:blocked 8 s later, so it is adopted.
    #[test]
    fn a_label_made_with_a_pinned_one_is_pinned() {
        let d = TestDir::new("pins-same-run");
        fs::create_dir_all(d.path().join("state")).unwrap();
        fs::write(d.path().join(FILE), "tehas:ready\tLA_r\n").unwrap();
        let (said, whole) = pin_made(
            d.path(),
            &[
                made("tehas:ready", Some("LA_r"), Some("LA_r"), Some(1_000)),
                made("tehas:blocked", Some("LA_b"), Some("LA_b"), Some(1_008)),
            ],
        );
        assert!(whole);
        assert_eq!(
            said,
            ["pinned the label tehas:blocked as LA_b in state/labels: it was made with the pinned labels"]
        );
        assert_eq!(load(d.path())["tehas:blocked"], "LA_b");
    }

    // Each is refused: made long before or after the pinned label, of no
    // known time, beside a pinned label that is not the label of its pin,
    // or a different label at the end of the run than at its start.
    #[test]
    fn a_label_not_made_with_a_pinned_one_is_still_refused() {
        let cases: [(&str, Seen, Seen); 5] = [
            (
                "made a day before",
                made("tehas:ready", Some("LA_r"), Some("LA_r"), Some(90_000)),
                made("tehas:blocked", Some("LA_b"), Some("LA_b"), Some(3_600)),
            ),
            (
                "made just past the bound",
                made("tehas:ready", Some("LA_r"), Some("LA_r"), Some(1_000)),
                made("tehas:blocked", Some("LA_b"), Some("LA_b"), Some(1_061)),
            ),
            (
                "no time",
                made("tehas:ready", Some("LA_r"), Some("LA_r"), Some(1_000)),
                made("tehas:blocked", Some("LA_b"), Some("LA_b"), None),
            ),
            (
                "the pinned name is another label now",
                made("tehas:ready", Some("LA_x"), Some("LA_x"), Some(1_000)),
                made("tehas:blocked", Some("LA_b"), Some("LA_b"), Some(1_000)),
            ),
            (
                "another label by the end of the run",
                made("tehas:ready", Some("LA_r"), Some("LA_r"), Some(1_000)),
                made("tehas:blocked", Some("LA_b"), Some("LA_c"), Some(1_000)),
            ),
        ];
        for (why, ready, blocked) in cases {
            let d = TestDir::new("pins-not-same-run");
            fs::create_dir_all(d.path().join("state")).unwrap();
            fs::write(d.path().join(FILE), "tehas:ready\tLA_r\n").unwrap();
            let (said, whole) = pin_made(d.path(), &[ready, blocked]);
            assert!(!whole, "{why}");
            assert!(
                said.iter()
                    .all(|l| !l.starts_with("pinned the label tehas:blocked")),
                "{why}: {said:?}"
            );
            assert!(!load(d.path()).contains_key("tehas:blocked"), "{why}");
        }
    }
}
