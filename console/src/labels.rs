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

fn run(labels: &[Label], made: &mut dyn FnMut(&Label) -> bool) -> u8 {
    let mut rc = 0;
    for label in labels {
        if made(label) {
            println!("label {}", label.name);
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
    run(&labels, &mut |label| make(&repo, label))
}

#[cfg(test)]
mod tests {
    use super::*;

    const THE_TABLE: &str = include_str!("../../host/labels.tsv");

    #[test]
    fn the_table_of_the_repository_is_read_whole() {
        let labels = table(THE_TABLE).unwrap();
        assert_eq!(labels.len(), 19);
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
        assert_eq!(names.len(), 19, "a name twice");
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
}
