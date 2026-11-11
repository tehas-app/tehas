//! `tehas labels`: make the tehas labels in the product repository, or bring
//! their colours and descriptions back to `lib/labels.tsv`. The text of the
//! command is src/usage/labels.txt.
//!
//! The table is a file of the install and not of this program, so a change to
//! it asks for the labels step and a change to the console does not. Each line
//! is the name, the colour and the description with tabs between them. gh's
//! own words go to standard error, and each call is its own: a label that
//! cannot be made stops none of the others. gh is the one on `PATH`: a first
//! install runs this before `$TEHAS_HOME/shim/gh` has a gh to call.
//!
//! Once the labels are made it pins each granting `tehas:` label to its
//! GitHub id in `$TEHAS_HOME/state/labels` (tehas_core::label_pins), so every
//! granting label has its pin before the dispatcher reads one, and a label
//! renamed to a granting name later grants nothing. Once the file exists it
//! pins only a label it made this run, and never changes a pin: a label with
//! no pin that was already there, or one that is not the pinned one, is said,
//! and the command fails.

use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use tehas_core::forge::is_label;

#[derive(Debug, PartialEq)]
struct Label<'a> {
    name: &'a str,
    color: &'a str,
    description: &'a str,
}

/// 64 with the usage: this command takes no argument.
pub fn words(given: &[String]) -> Result<(), u8> {
    if given.is_empty() {
        return Ok(());
    }
    eprintln!("usage: tehas labels");
    Err(64)
}

/// The labels of the table, or the number of the first line that is no label.
/// All or none: a table that is wrong makes nothing.
fn table(text: &str) -> Result<Vec<Label<'_>>, usize> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let mut fields = line.split('\t');
        let (Some(name), Some(color), Some(description), None) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            return Err(i + 1);
        };
        let hex = color.len() == 6 && color.bytes().all(|c| c.is_ascii_hexdigit());
        if !is_label(name) || !hex || description.is_empty() {
            return Err(i + 1);
        }
        out.push(Label {
            name,
            color,
            description,
        });
    }
    if out.is_empty() {
        return Err(1);
    }
    Ok(out)
}

fn make(repo: &str, label: &Label) -> bool {
    Command::new("gh")
        .args(["label", "create", label.name, "--repo", repo, "--color"])
        .args([label.color, "--description", label.description, "--force"])
        .stdout(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// The GitHub node id of the label `name`, None when gh could not read it.
fn node_id(repo: &str, name: &str) -> Option<String> {
    let out = Command::new("gh")
        .args([
            "api",
            &format!("repos/{repo}/labels/{name}"),
            "--jq",
            ".node_id",
        ])
        .stderr(Stdio::inherit())
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let id = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!id.is_empty() && !id.contains(char::is_whitespace)).then_some(id)
}

/// Each granting label's id as `id_of` asks GitHub for it: the `before`
/// half of what `pin` judges, read before the labels are made.
fn ids(id_of: &mut dyn FnMut(&str) -> Option<String>) -> Vec<Option<String>> {
    tehas_core::label_pins::GRANTING
        .iter()
        .map(|n| id_of(n))
        .collect()
}

/// Pins the granting labels: `before` from `ids` before the labels were made,
/// and `id_of` asking GitHub for each one's id now.
fn pin(
    home: &Path,
    before: Vec<Option<String>>,
    id_of: &mut dyn FnMut(&str) -> Option<String>,
) -> u8 {
    let seen: Vec<tehas_core::label_pins::Seen> = tehas_core::label_pins::GRANTING
        .iter()
        .zip(before)
        .map(|(n, before)| tehas_core::label_pins::Seen {
            name: n,
            before,
            after: id_of(n),
        })
        .collect();
    let (said, whole) = tehas_core::label_pins::pin_made(home, &seen);
    for l in &said {
        if l.starts_with("pinned ") {
            tehas_core::outln!("{l}");
        } else {
            eprintln!("tehas labels: {l}");
        }
    }
    u8::from(!whole)
}

fn run(labels: &[Label], made: &mut dyn FnMut(&Label) -> bool) -> u8 {
    let mut rc = 0;
    for label in labels {
        if made(label) {
            tehas_core::outln!("label {}", label.name);
        } else {
            eprintln!("tehas labels: could not make {}", label.name);
            rc = 1;
        }
    }
    rc
}

pub fn main(given: &[String]) -> u8 {
    if let Err(rc) = words(given) {
        return rc;
    }
    let home = env::var("TEHAS_HOME").unwrap_or_default();
    let file = Path::new(&home).join("lib/labels.tsv");
    let Ok(text) = fs::read_to_string(&file) else {
        eprintln!(
            "tehas labels: no table of labels at {}: run install.sh",
            file.display()
        );
        return 78;
    };
    let labels = match table(&text) {
        Ok(labels) => labels,
        Err(line) => {
            eprintln!("tehas labels: {} line {line} is no label", file.display());
            return 78;
        }
    };
    let repo = env::var("TEHAS_REPO").unwrap_or_default();
    let before = ids(&mut |name| node_id(&repo, name));
    let made = run(&labels, &mut |label| make(&repo, label));
    let pinned = pin(Path::new(&home), before, &mut |name| node_id(&repo, name));
    made.max(pinned)
}

#[cfg(test)]
mod tests {
    use super::*;

    const THE_TABLE: &str = include_str!("../../host/labels.tsv");

    #[test]
    fn the_table_of_the_repository_is_read_whole() {
        let labels = table(THE_TABLE).unwrap();
        assert_eq!(labels.len(), 21);
        assert_eq!(labels[0].name, "tehas:ready");
        assert_eq!(labels[0].color, "0e8a16");
        assert_eq!(
            labels[6],
            Label {
                name: "needs-human-attention",
                color: "b60205",
                description: "Cycle parked; the handoff comment says what to do",
            }
        );
        let mut names: Vec<&str> = labels.iter().map(|l| l.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 21, "a name twice");
    }

    #[test]
    fn a_line_that_is_no_label_stops_the_table() {
        for bad in [
            "",
            "tehas:ready\t0e8a16",
            "tehas:ready\t0e8a16\tx\ty",
            "tehas ready\t0e8a16\tx",
            "tehas:ready\t0e8a1\tx",
            "tehas:ready\tzzzzzz\tx",
            "tehas:ready\t0e8a16\t",
        ] {
            assert!(table(bad).is_err(), "{bad:?}");
        }
        assert_eq!(table("a\t000000\tx\nb\n"), Err(2));
    }

    #[test]
    fn a_label_that_cannot_be_made_stops_no_other() {
        let labels = table("a\t000000\tx\nb\t000000\ty\nc\t000000\tz\n").unwrap();
        let mut asked = Vec::new();
        let rc = run(&labels, &mut |l| {
            asked.push(l.name.to_string());
            l.name != "b"
        });
        assert_eq!(rc, 1);
        assert_eq!(asked, ["a", "b", "c"]);
        assert_eq!(run(&labels, &mut |_| true), 0);
    }

    #[test]
    fn the_granting_labels_are_pinned_after_they_are_made() {
        let d = tehas_core::testdir::TestDir::new("labels-pin");
        let mut asked = Vec::new();
        let none = vec![None; tehas_core::label_pins::GRANTING.len()];
        let rc = pin(d.path(), none.clone(), &mut |n| {
            asked.push(n.to_string());
            Some(format!("LA_{}", n.trim_start_matches("tehas:")))
        });
        assert_eq!(rc, 0);
        assert_eq!(asked.len(), tehas_core::label_pins::GRANTING.len());
        let pins = tehas_core::label_pins::load(d.path());
        assert_eq!(pins["tehas:ready"], "LA_ready");
        assert_eq!(pins.len(), tehas_core::label_pins::GRANTING.len());
        // A label gh cannot find fails the command and pins nothing for it.
        let d = tehas_core::testdir::TestDir::new("labels-pin-missing");
        let rc = pin(d.path(), none, &mut |n| {
            (n != "tehas:plan").then(|| format!("LA_{n}"))
        });
        assert_eq!(rc, 1);
        assert!(!tehas_core::label_pins::load(d.path()).contains_key("tehas:plan"));
    }
}
