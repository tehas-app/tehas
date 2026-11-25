//! `tehas labels`: make the tehas labels in the product repository, or bring
//! their colours and descriptions back to the table. The text of the
//! command is src/usage/labels.txt.
//!
//! The labels are the factory's marks, which the tickets port's `prepare`
//! makes from the table the adapter was built with, `host/labels.tsv`: a
//! label that cannot be made stops none of the others. gh is the one on
//! `PATH`: a first install runs this before `$TEHAS_HOME/shim/gh` has a gh
//! to call.
//!
//! Once the labels are made it pins each granting `tehas:` label to its
//! GitHub id in `$TEHAS_HOME/state/labels` (tehas_core::label_pins), so every
//! such label has its pin before the dispatcher reads one, and a label
//! renamed to one of those names later grants nothing. Once the file exists it
//! pins only a label it made this run, and never changes a pin: a label with
//! no pin that was already there, or one that is not the pinned one, is said,
//! and the command fails. A pins file that is there and cannot be read stops
//! it with 78 before it asks GitHub anything, and it never writes over one.

use std::collections::BTreeMap;
use std::env;
use std::path::Path;
use tehas_core::call;
use tehas_ports::{TicketWrites, Unprepared};

/// 64 with the usage: this command takes no argument.
pub fn words(given: &[String]) -> Result<(), u8> {
    if given.is_empty() {
        return Ok(());
    }
    eprintln!("usage: tehas labels");
    Err(64)
}

/// The gh this command runs: the one on `PATH`.
const GH: &str = "gh";

/// gh under its bound, what it printed held for the caller.
fn gh(args: &[&str]) -> call::Ran {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    call::run(GH, &args, call::GH_BOUND)
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
    tehas_core::label_pins::GRANTING
        .iter()
        .map(|n| id_of(n))
        .collect()
}

/// Pins the labels of `GRANTING`: `before` from `ids` before the labels were made,
/// `id_of` asking GitHub for each one's id now, and `made` when each id's
/// label was created.
fn pin(
    home: &Path,
    before: Vec<Option<String>>,
    id_of: &mut dyn FnMut(&str) -> Option<String>,
    made: &BTreeMap<String, i64>,
) -> u8 {
    let seen: Vec<tehas_core::label_pins::Seen> = tehas_core::label_pins::GRANTING
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
    let (said, whole) = match tehas_core::label_pins::pin_made(home, &seen) {
        Ok(r) => r,
        Err(u) => return unreadable(&u),
    };
    for l in &said {
        if l.starts_with("pinned ") {
            tehas_core::outln!("{l}");
        } else {
            eprintln!("tehas labels: {l}");
        }
    }
    u8::from(!whole)
}

/// 78 with the line: pins that cannot be read stop the command, and
/// nothing is written over them.
fn unreadable(u: &tehas_core::label_pins::Unreadable) -> u8 {
    eprintln!("tehas labels: {u}");
    78
}

/// Err(78) when the pins are there and cannot be read: checked before any
/// label is asked about or made.
fn readable(home: &Path) -> Result<(), u8> {
    tehas_core::label_pins::load(home)
        .map(drop)
        .map_err(|u| unreadable(&u))
}

/// The factory's labels made through `tickets`: 0, or 1 when one could
/// not be made.
fn make(tickets: &mut dyn TicketWrites) -> u8 {
    match tickets.prepare() {
        Ok(()) => {
            tehas_core::outln!("made the tehas labels");
            0
        }
        Err(u) => {
            eprintln!("{}", failed(&u));
            1
        }
    }
}

/// What a failed `prepare` says: the first label that could not be made,
/// and why, in tehas's words.
fn failed(u: &Unprepared) -> String {
    match &u.mark {
        Some(name) => format!(
            "tehas labels: could not make the label {}: {}",
            name.as_str(),
            u.why.line()
        ),
        None => format!("tehas labels: could not make the labels: {}", u.why.line()),
    }
}

pub fn main(given: &[String]) -> u8 {
    if let Err(rc) = words(given) {
        return rc;
    }
    let home = env::var("TEHAS_HOME").unwrap_or_default();
    let repo = env::var("TEHAS_REPO").unwrap_or_default();
    if let Err(rc) = readable(Path::new(&home)) {
        return rc;
    }
    let before = ids(&mut |name| node_id(&repo, name));
    let made = make(&mut *crate::wiring::tickets(&repo, GH, call::GH_BOUND));
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

    use std::fs;
    use tehas_ports::fake::{FakeSite, Fault, Faulty, Op, TicketsOp, World};
    use tehas_ports::{Actor, Caller, MarkName, Outcome, Passing, Scope, Sent, Site, Write};

    const THE_TABLE: &str = include_str!("../../host/labels.tsv");

    /// The label names of the table, its first column.
    fn names() -> Vec<&'static str> {
        THE_TABLE
            .lines()
            .filter_map(|l| l.split('\t').next())
            .collect()
    }

    fn writes(site: &impl Site) -> Box<dyn TicketWrites + Send> {
        site.ticket_writes(&Actor::Ours(Scope::Full), Caller::new("labels"))
            .unwrap()
    }

    #[test]
    fn the_labels_are_the_factorys_marks_prepared() {
        let world = World::new();
        assert_eq!(make(&mut *writes(&FakeSite::new(&world))), 0);
        let wrote: Vec<Write> = world.wrote().into_iter().map(|w| w.what).collect();
        assert_eq!(wrote, [Write::Prepare]);
    }

    #[test]
    fn a_label_that_cannot_be_made_fails_the_command() {
        let world = World::new();
        let site = Faulty::new(FakeSite::new(&world)).on(
            Op::Tickets(TicketsOp::Prepare),
            Fault::Server { applied: false },
        );
        assert_eq!(make(&mut *writes(&site)), 1);
    }

    #[test]
    fn a_failure_names_the_label_that_failed_in_tehass_words() {
        let server = Outcome::Unavailable {
            why: Passing::Server,
            sent: Sent::Maybe,
        };
        let mut lean = Unprepared::new(server.clone());
        lean.mark = Some(MarkName::new("a-label").unwrap());
        let line = failed(&lean);
        assert!(
            line.starts_with("tehas labels: could not make the label a-label: "),
            "{line}"
        );
        assert!(line.ends_with(server.line()), "{line}");
        let site = Unprepared::new(server.clone());
        assert_eq!(
            failed(&site),
            format!("tehas labels: could not make the labels: {}", server.line())
        );
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
        let table = names();
        let mut named = Vec::new();
        walk(Path::new(env!("CARGO_MANIFEST_DIR")), &mut named);
        named.sort_unstable();
        named.dedup();
        assert!(named.len() > 10, "{named:?}");
        // The plugins' prefix is named, and is no label of the table.
        let missing: Vec<&String> = named
            .iter()
            .filter(|n| *n != tehas_core::label_pins::PLUGIN_PHASE)
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
    fn the_pinned_labels_are_pinned_after_they_are_made() {
        let d = tehas_core::testdir::TestDir::new("labels-pin");
        let mut asked = Vec::new();
        let none = vec![None; tehas_core::label_pins::GRANTING.len()];
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
        assert_eq!(asked.len(), tehas_core::label_pins::GRANTING.len());
        let pins = tehas_core::label_pins::load(d.path()).unwrap().pins();
        assert_eq!(pins["tehas:ready"], "LA_ready");
        assert_eq!(pins.len(), tehas_core::label_pins::GRANTING.len());
        // A label gh cannot find fails the command and pins nothing for it.
        let d = tehas_core::testdir::TestDir::new("labels-pin-missing");
        let rc = pin(
            d.path(),
            none,
            &mut |n| (n != "tehas:plan").then(|| format!("LA_{n}")),
            &BTreeMap::new(),
        );
        assert_eq!(rc, 1);
        assert!(!tehas_core::label_pins::load(d.path())
            .unwrap()
            .pins()
            .contains_key("tehas:plan"));
    }

    // A link (dangling or not), a line of another shape, or a file tehas may
    // not read: the command stops with 78 before it asks GitHub, and after
    // making the labels it writes nothing over the file.
    #[test]
    fn pins_that_cannot_be_read_stop_the_command_and_are_kept_as_they_are() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        use tehas_core::label_pins::{FILE, GRANTING};
        const READY: &str = GRANTING[0];
        type Case = (&'static str, fn(&Path));
        let mut cases: Vec<Case> = vec![
            ("a dangling link", |h| {
                symlink(h.join("gone"), h.join(FILE)).unwrap()
            }),
            ("a live link", |h| {
                fs::write(h.join("elsewhere"), format!("{READY}\tLA_ready\n")).unwrap();
                symlink(h.join("elsewhere"), h.join(FILE)).unwrap();
            }),
            ("a line of another shape", |h| {
                fs::write(
                    h.join(FILE),
                    format!("{READY}\tLA_ready\n{READY}x LA_plan\n"),
                )
                .unwrap()
            }),
        ];
        let probe = tehas_core::testdir::TestDir::new("labels-probe");
        fs::write(probe.path().join("f"), "x").unwrap();
        fs::set_permissions(probe.path().join("f"), fs::Permissions::from_mode(0o000)).unwrap();
        if fs::read(probe.path().join("f")).is_err() {
            cases.push(("permission denied", |h| {
                fs::write(h.join(FILE), format!("{READY}\tLA_ready\n")).unwrap();
                fs::set_permissions(h.join(FILE), fs::Permissions::from_mode(0o000)).unwrap();
            }));
        }
        // What is at the path, without following a link.
        let as_is = |h: &Path| -> (Option<std::path::PathBuf>, Option<Vec<u8>>) {
            let f = h.join(FILE);
            if let Ok(to) = fs::read_link(&f) {
                return (Some(to), fs::read(h.join("elsewhere")).ok());
            }
            let mode = fs::metadata(&f).unwrap().permissions().mode();
            fs::set_permissions(&f, fs::Permissions::from_mode(0o600)).unwrap();
            let bytes = fs::read(&f).unwrap();
            fs::set_permissions(&f, fs::Permissions::from_mode(mode)).unwrap();
            (None, Some(bytes))
        };
        for (why, make) in cases {
            let d = tehas_core::testdir::TestDir::new("labels-pin-unreadable");
            fs::create_dir_all(d.path().join(FILE).parent().unwrap()).unwrap();
            make(d.path());
            let was = as_is(d.path());
            assert_eq!(readable(d.path()), Err(78), "{why}");
            let rc = pin(
                d.path(),
                vec![None; GRANTING.len()],
                &mut |n| Some(format!("LA_{}", n.replace(':', "_"))),
                &BTreeMap::new(),
            );
            assert_eq!(rc, 78, "{why}");
            assert_eq!(as_is(d.path()), was, "{why}");
            assert!(!d.path().join(FILE).with_extension("new").exists(), "{why}");
        }
        let bare = tehas_core::testdir::TestDir::new("labels-pin-absent");
        assert_eq!(readable(bare.path()), Ok(()));
    }

    // CRLF line ends, spaces or tabs after an id and blank lines: the
    // command goes on, keeps the pins and writes the file plain.
    #[test]
    fn pins_with_trailing_damage_pass_and_are_written_plain() {
        use tehas_core::label_pins::{FILE, GRANTING};
        let (ready, plan) = (GRANTING[0], GRANTING[1]);
        let id = |n: &str| format!("LA_{}", n.replace(':', "_"));
        let fill = |t: &str| {
            t.replace("{R}", &format!("{ready}\t{}", id(ready)))
                .replace("{P}", &format!("{plan}\t{}", id(plan)))
        };
        for text in ["{P}\r\n{R}\r\n", "{P} \n{R}\t\n", "\n{P}\n\n{R}\n\n"] {
            let d = tehas_core::testdir::TestDir::new("labels-pin-lenient");
            fs::create_dir_all(d.path().join(FILE).parent().unwrap()).unwrap();
            fs::write(d.path().join(FILE), fill(text)).unwrap();
            assert_eq!(readable(d.path()), Ok(()), "{text:?}");
            let before: Vec<Option<String>> = GRANTING
                .iter()
                .map(|n| [ready, plan].contains(n).then(|| id(n)))
                .collect();
            let rc = pin(d.path(), before, &mut |n| Some(id(n)), &BTreeMap::new());
            assert_eq!(rc, 0, "{text:?}");
            let got = fs::read_to_string(d.path().join(FILE)).unwrap();
            assert!(
                !got.contains(['\r', ' ']) && !got.contains("\n\n"),
                "{got:?}"
            );
            assert!(got.contains(&fill("{P}\n")), "{got:?}");
            assert!(got.contains(&fill("{R}\n")), "{got:?}");
        }
    }

    /// A plugin phase's label: no core label takes the prefix, and `tehas
    /// labels` neither makes, recolours nor pins one. It makes only the
    /// table's labels and pins only `GRANTING`, so a phase label the
    /// repository has, its creation time read beside the others, is never
    /// asked about.
    #[test]
    fn a_plugin_phase_label_is_neither_made_nor_pinned() {
        use tehas_core::label_pins::{GRANTING, PLUGIN_PHASE};
        let phase = |n: &str| n.starts_with(PLUGIN_PHASE);
        assert!(!names().iter().any(|n| phase(n)));
        let d = tehas_core::testdir::TestDir::new("labels-pin-phase");
        let mut asked = Vec::new();
        let at = times("LA_px\t2026-10-19T18:16:01Z\nLA_0\t2026-10-19T18:16:02Z\n");
        let none = vec![None; GRANTING.len()];
        pin(
            d.path(),
            none,
            &mut |n| {
                asked.push(n.to_string());
                Some(format!("LA_{}", GRANTING.iter().position(|g| *g == n)?))
            },
            &at,
        );
        assert!(!asked.iter().any(|n| phase(n)), "{asked:?}");
        let pins = tehas_core::label_pins::load(d.path()).unwrap().pins();
        assert!(!pins.keys().any(|n| phase(n)), "{pins:?}");
        assert!(!pins.values().any(|id| id == "LA_px"), "{pins:?}");
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
