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
//! Once the labels are made it pins each granting `tehas:` label, and
//! `tehas:prototype`, to its GitHub id in `$TEHAS_HOME/state/labels`
//! (tehas_core::label_pins), so every such label has its pin before the
//! dispatcher or the Protolab reads one, and a label renamed to one of those
//! names later grants nothing. Once the file exists it
//! pins only a label it made this run, and never changes a pin: a label with
//! no pin that was already there, or one that is not the pinned one, is said,
//! and the command fails.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::Path;
use tehas_core::call;
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
    let args = [
        "label",
        "create",
        label.name,
        "--repo",
        repo,
        "--color",
        label.color,
        "--description",
        label.description,
        "--force",
    ];
    let ran = gh(&args);
    eprint!("{}", ran.err);
    ran.ok()
}

/// gh under its bound, what it printed held for the caller.
fn gh(args: &[&str]) -> call::Ran {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    call::run("gh", &args, call::GH_BOUND)
}

/// The GitHub node id of the label `name`, None when gh could not read it.
fn node_id(repo: &str, name: &str) -> Option<String> {
    let path = format!("repos/{repo}/labels/{name}");
    let ran = gh(&["api", &path, "--jq", ".node_id"]);
    eprint!("{}", ran.err);
    if !ran.ok() {
        return None;
    }
    let id = ran.out.trim().to_string();
    (!id.is_empty() && !id.contains(char::is_whitespace)).then_some(id)
}

/// When GitHub says each `tehas:` label of `repo` was created, in Unix
/// seconds by node id. Empty when gh cannot say: no label is then taken as
/// made with the pinned ones.
fn created(repo: &str) -> BTreeMap<String, i64> {
    let Some((owner, name)) = repo.split_once('/') else {
        return BTreeMap::new();
    };
    let query = "query($owner:String!,$name:String!){repository(owner:$owner,name:$name){labels(first:100,query:\"tehas:\"){nodes{id createdAt}}}}";
    let ran = gh(&[
        "api",
        "graphql",
        "-f",
        &format!("query={query}"),
        "-f",
        &format!("owner={owner}"),
        "-f",
        &format!("name={name}"),
        "--jq",
        ".data.repository.labels.nodes[] | .id + \"\\t\" + .createdAt",
    ]);
    eprint!("{}", ran.err);
    if !ran.ok() {
        return BTreeMap::new();
    }
    times(&ran.out)
}

/// `<id>\t<time>` lines to times by id; a line of another shape is skipped.
fn times(out: &str) -> BTreeMap<String, i64> {
    out.lines()
        .filter_map(|l| {
            let (id, at) = l.split_once('\t')?;
            Some((id.to_string(), tehas_core::time::from_iso(at)?))
        })
        .collect()
}

/// Each pinned label's id as `id_of` asks GitHub for it: the `before`
/// half of what `pin` judges, read before the labels are made.
fn ids(id_of: &mut dyn FnMut(&str) -> Option<String>) -> Vec<Option<String>> {
    tehas_core::label_pins::PINNED
        .iter()
        .map(|n| id_of(n))
        .collect()
}

/// Pins the labels of `PINNED`: `before` from `ids` before the labels were made,
/// `id_of` asking GitHub for each one's id now, and `made` when each id's
/// label was created.
fn pin(
    home: &Path,
    before: Vec<Option<String>>,
    id_of: &mut dyn FnMut(&str) -> Option<String>,
    made: &BTreeMap<String, i64>,
) -> u8 {
    let seen: Vec<tehas_core::label_pins::Seen> = tehas_core::label_pins::PINNED
        .iter()
        .zip(before)
        .map(|(n, before)| {
            let after = id_of(n);
            let created = after.as_ref().and_then(|id| made.get(id).copied());
            tehas_core::label_pins::Seen {
                name: n,
                before,
                after,
                created,
            }
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
    let made_at = created(&repo);
    let pinned = pin(
        Path::new(&home),
        before,
        &mut |name| node_id(&repo, name),
        &made_at,
    );
    made.max(pinned)
}

#[cfg(test)]
mod tests {
    use super::*;

    const THE_TABLE: &str = include_str!("../../host/labels.tsv");

    #[test]
    fn the_table_of_the_repository_is_read_whole() {
        let labels = table(THE_TABLE).unwrap();
        assert_eq!(labels.len(), 22);
        assert_eq!(labels[0].name, "tehas:ready");
        assert_eq!(labels[0].color, "0e8a16");
        assert_eq!(
            labels[5],
            Label {
                name: "needs-human-attention",
                color: "b60205",
                description: "Cycle parked; the handoff comment says what to do",
            }
        );
        let mut names: Vec<&str> = labels.iter().map(|l| l.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 22, "a name twice");
    }

    /// The labels a program names without the `tehas:` prefix.
    const UNPREFIXED: [&str; 2] = ["needs-human-attention", "epic"];

    /// Each label name quoted in the code of `file` before its tests.
    fn named_in(file: &str, out: &mut Vec<String>) {
        let code = file.split("#[cfg(test)]").next().unwrap_or_default();
        for (at, _) in code.match_indices("\"tehas:") {
            let rest = &code[at + 1..];
            let len = "tehas:".len()
                + rest["tehas:".len()..]
                    .find(|c: char| !(c.is_ascii_lowercase() || c == '-'))
                    .unwrap_or(0);
            if len > "tehas:".len() && rest[len..].starts_with('"') {
                out.push(rest[..len].to_string());
            }
        }
        for name in UNPREFIXED {
            if code.contains(&format!("\"{name}\"")) {
                out.push(name.to_string());
            }
        }
    }

    fn walk(dir: &Path, out: &mut Vec<String>) {
        for entry in fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            let name = entry.file_name();
            if path.is_dir() {
                if name != "target" && name != "tests" {
                    walk(&path, out);
                }
            } else if path.extension().is_some_and(|e| e == "rs") && name != "tests.rs" {
                named_in(&fs::read_to_string(&path).unwrap(), out);
            }
        }
    }

    #[test]
    fn every_label_a_program_names_is_in_the_table() {
        let table: Vec<&str> = table(THE_TABLE).unwrap().iter().map(|l| l.name).collect();
        let mut named = Vec::new();
        walk(Path::new(env!("CARGO_MANIFEST_DIR")), &mut named);
        named.sort_unstable();
        named.dedup();
        assert!(named.len() > 10, "{named:?}");
        let missing: Vec<&String> = named
            .iter()
            .filter(|n| !table.contains(&n.as_str()))
            .collect();
        assert!(missing.is_empty(), "not in host/labels.tsv: {missing:?}");
    }

    #[test]
    fn a_name_in_the_code_is_found_and_one_in_its_tests_is_not() {
        let mut out = Vec::new();
        named_in(
            "const A: &str = \"tehas:ready\";\nsay(\"tehas: no\");\nlet b = [\"epic\", \"tehas:{x}\"];\n#[cfg(test)]\nmod tests { \"tehas:x\" }\n",
            &mut out,
        );
        assert_eq!(out, ["tehas:ready", "epic"]);
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
    fn the_pinned_labels_are_pinned_after_they_are_made() {
        let d = tehas_core::testdir::TestDir::new("labels-pin");
        let mut asked = Vec::new();
        let none = vec![None; tehas_core::label_pins::PINNED.len()];
        let rc = pin(
            d.path(),
            none.clone(),
            &mut |n| {
                asked.push(n.to_string());
                Some(format!("LA_{}", n.trim_start_matches("tehas:")))
            },
            &BTreeMap::new(),
        );
        assert_eq!(rc, 0);
        assert_eq!(asked.len(), tehas_core::label_pins::PINNED.len());
        let pins = tehas_core::label_pins::load(d.path());
        assert_eq!(pins["tehas:ready"], "LA_ready");
        assert_eq!(pins["tehas:prototype"], "LA_prototype");
        assert_eq!(pins.len(), tehas_core::label_pins::PINNED.len());
        // A label gh cannot find fails the command and pins nothing for it.
        let d = tehas_core::testdir::TestDir::new("labels-pin-missing");
        let rc = pin(
            d.path(),
            none,
            &mut |n| (n != "tehas:plan").then(|| format!("LA_{n}")),
            &BTreeMap::new(),
        );
        assert_eq!(rc, 1);
        assert!(!tehas_core::label_pins::load(d.path()).contains_key("tehas:plan"));
    }

    #[test]
    fn the_creation_times_are_read_by_id() {
        let t = times(
            "LA_r\t2026-10-19T18:16:01Z\nno tab\nLA_b\tyesterday\nLA_c\t2026-10-19T18:16:09Z\n",
        );
        assert_eq!(t.len(), 2);
        assert_eq!(t["LA_c"] - t["LA_r"], 8);
    }
}
