//! `TicketWrites::prepare_one`, and `prepare` where it renames, as the
//! adapter answers them, replayed from
//! the recordings under `tests/recorded/prepare-one/<state>/` (made by
//! hand from GitHub's documented shapes): `label` is the read of the one
//! label, `labels` the read of every label, `held` the read of every label
//! with its colour and text, `create` a label's write and `rename` its
//! `PATCH`.
//! After a write the replay answers from `made/` where the state has it,
//! so the law reads the site the write left. `.json` is gh's answer,
//! `.fail` the reason it failed and `.answer.json` what gh printed beside
//! it. No test calls GitHub. `tests/synthetic/prepare-one/garbled/`
//! holds the answer that is not JSON.

mod answers;

use std::sync::{Arc, Mutex};
use tehas_core::forge::{Failed, GhRunner};
use tehas_github::{table, GitHub};
use tehas_ports::laws;
use tehas_ports::{
    At, Did, Mark, MarkName, Outcome, Passing, Pool, Refusal, Sent, State, TicketWrites, Unread,
};

const PROJECT: &str = "acme/widget";

const ESCALATED: Mark = Mark::State(State::Escalated);

type Asked = Arc<Mutex<Vec<Vec<String>>>>;

struct Replay {
    state: String,
    made: bool,
    asked: Asked,
    input: Arc<Mutex<Vec<String>>>,
}

fn field<'a>(args: &'a [&str], name: &str) -> Option<&'a str> {
    args.windows(2)
        .filter(|w| w[0] == "-f" || w[0] == "-F")
        .find_map(|w| w[1].strip_prefix(&format!("{name}=")))
}

impl Replay {
    /// The recording a call is answered from, by its read.
    fn read(&self, name: &str) -> Option<String> {
        let made = answers::at("prepare-one", &self.state, &format!("made/{name}"));
        let at = if self.made && made.exists() {
            made
        } else {
            answers::at("prepare-one", &self.state, name)
        };
        std::fs::read_to_string(at).ok()
    }

    fn name(args: &[&str]) -> &'static str {
        match args {
            ["label", "create", ..] => "create",
            ["api", "-X", "PATCH", _, "--input", "-"] => "rename",
            ["api", "graphql", ..] => {
                let query = field(args, "query").expect("a query");
                if query.ends_with("{ databaseId nameWithOwner } }") {
                    "name"
                } else if query.contains("label(name: $label)") {
                    assert_eq!(field(args, "owner"), Some("acme"));
                    assert_eq!(field(args, "name"), Some("widget"));
                    "label"
                } else if query.contains("nodes { name color description }") {
                    "held"
                } else if query.contains("labels(") {
                    "labels"
                } else {
                    panic!("no recording answers {query}")
                }
            }
            _ => panic!("no recording answers {args:?}"),
        }
    }
}

impl GhRunner for Replay {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        self.asked
            .lock()
            .unwrap()
            .push(args.iter().map(|a| a.to_string()).collect());
        let name = Self::name(args);
        if name == "name" {
            let at = answers::at("repository", "here", "name.json");
            return Ok(std::fs::read_to_string(at).unwrap());
        }
        if let Some(out) = self.read(&format!("{name}.json")) {
            if name == "create" || name == "rename" {
                self.made = true;
            }
            return Ok(out);
        }
        match self.read(&format!("{name}.fail")) {
            Some(why) => Err(why.trim_end().to_string()),
            None => Err(format!("no recording {name} in {}", self.state)),
        }
    }

    fn run_input(&mut self, args: &[&str], input: &str) -> Result<String, String> {
        self.input.lock().unwrap().push(input.to_string());
        self.run(args)
    }

    fn run_answer(&mut self, args: &[&str]) -> Result<String, Failed> {
        self.run(args).map_err(|why| Failed {
            why,
            answer: self.read(&format!("{}.answer.json", Self::name(args))),
        })
    }
}

fn replay_now() -> i64 {
    1_767_225_600
}

fn adapter(state: &str) -> (GitHub, Asked) {
    let (gh, asked, _) = adapter_input(state);
    (gh, asked)
}

/// [`adapter`], and what each write sent on gh's standard input.
fn adapter_input(state: &str) -> (GitHub, Asked, Arc<Mutex<Vec<String>>>) {
    let asked = Asked::default();
    let input = Arc::default();
    let replay = Replay {
        state: state.to_string(),
        made: false,
        asked: Arc::clone(&asked),
        input: Arc::clone(&input),
    };
    let gh = GitHub::new(PROJECT, Box::new(replay)).on_clock(replay_now);
    (gh, asked, input)
}

/// The kind of each call asked, in order.
fn calls(asked: &Asked) -> Vec<&'static str> {
    asked
        .lock()
        .unwrap()
        .iter()
        .map(|a| Replay::name(&a.iter().map(String::as_str).collect::<Vec<_>>()))
        .collect()
}

fn prepare(state: &str) -> (Did, Vec<&'static str>) {
    let (mut gh, asked) = adapter(state);
    let did = gh.prepare_one(&ESCALATED);
    (did, calls(&asked))
}

/// The one write `prepare` sends for the mark, from its row of the table.
fn the_write() -> String {
    "label create tehas:millwright-escalated --repo acme/widget --color c5def5 \
     --description Tehas: the millwright left this issue to a person --force"
        .to_string()
}

#[test]
fn the_law_holds_on_the_recording() {
    let (mut gh, asked) = adapter("missing");
    assert_eq!(
        laws::marks::prepare_one_makes_the_mark_held(&mut gh, &ESCALATED),
        Ok(())
    );
    // The marks, the label read, the repository's name asked once and the
    // label made, the marks, the label read again and right; the project's
    // own mark asks nothing.
    assert_eq!(
        calls(&asked),
        ["labels", "label", "name", "create", "labels", "label"]
    );
}

#[test]
fn a_missing_label_is_made_as_prepare_makes_it() {
    let (mut gh, asked) = adapter("missing");
    assert_eq!(gh.prepare_one(&ESCALATED), Ok(()));
    assert_eq!(calls(&asked), ["label", "name", "create"]);
    let asked = asked.lock().unwrap();
    let read: Vec<&str> = asked[0].iter().map(String::as_str).collect();
    assert_eq!(field(&read, "label"), Some("tehas:millwright-escalated"));
    assert_eq!(asked[2].join(" "), the_write());
}

#[test]
fn a_label_with_another_colour_or_text_is_brought_back() {
    for state in ["wrong-colour", "wrong-text", "no-text"] {
        let (mut gh, asked) = adapter(state);
        assert_eq!(gh.prepare_one(&ESCALATED), Ok(()), "{state}");
        assert_eq!(calls(&asked), ["label", "name", "create"], "{state}");
        assert_eq!(asked.lock().unwrap()[2].join(" "), the_write(), "{state}");
    }
}

#[test]
fn a_label_named_in_another_case_is_renamed_in_one_write_and_then_left() {
    let (mut gh, asked, input) = adapter_input("other-case");
    assert_eq!(gh.prepare_one(&ESCALATED), Ok(()));
    assert_eq!(calls(&asked), ["label", "name", "rename"]);
    assert_eq!(
        asked.lock().unwrap()[2],
        [
            "api",
            "-X",
            "PATCH",
            "repos/acme/widget/labels/Tehas%3AMillwright-Escalated",
            "--input",
            "-"
        ]
    );
    let sent: serde_json::Value = serde_json::from_str(&input.lock().unwrap()[0]).unwrap();
    assert_eq!(
        sent,
        serde_json::json!({
            "new_name": "tehas:millwright-escalated",
            "color": "c5def5",
            "description": "Tehas: the millwright left this issue to a person",
        })
    );
    // The second call finds the label right and writes nothing.
    assert_eq!(gh.prepare_one(&ESCALATED), Ok(()));
    assert_eq!(calls(&asked), ["label", "name", "rename", "label"]);
}

#[test]
fn a_rename_that_fails_is_its_outcome() {
    assert_eq!(
        prepare("other-case-fails"),
        (
            Err(Outcome::Spent {
                pool: Pool::Writes,
                until: At::new(replay_now()),
            }),
            vec!["label", "name", "rename"]
        )
    );
}

#[test]
fn a_label_that_is_right_is_not_written() {
    for state in ["right", "right-upper"] {
        assert_eq!(prepare(state), (Ok(()), vec!["label"]), "{state}");
    }
}

#[test]
fn a_projects_mark_and_a_mark_no_label_is_are_refused_with_no_call() {
    for mark in [
        Mark::Project(MarkName::new("bug").unwrap()),
        Mark::Project(MarkName::new("tehas:millwright-escalated").unwrap()),
        Mark::Unknown,
    ] {
        let (mut gh, asked) = adapter("right");
        assert_eq!(
            gh.prepare_one(&mark),
            Err(Outcome::Refused(Refusal::Invalid)),
            "{mark:?}"
        );
        assert!(asked.lock().unwrap().is_empty(), "{mark:?}");
    }
}

#[test]
fn a_failure_is_its_outcome_and_nothing_is_written_after_a_failed_read() {
    let inconsistent = Outcome::Unread(Unread::Inconsistent);
    let cases = [
        ("gone", Outcome::NotFound),
        (
            "rate-limited",
            Outcome::Spent {
                pool: Pool::Priced,
                until: At::new(replay_now()),
            },
        ),
        (
            "server-error",
            Outcome::Unavailable {
                why: Passing::Server,
                sent: Sent::Maybe,
            },
        ),
        ("errors", inconsistent.clone()),
        ("other-name", inconsistent),
        ("no-repository", Outcome::NotFound),
        ("garbled", Outcome::Unread(Unread::Malformed)),
    ];
    for (state, want) in cases {
        assert_eq!(prepare(state), (Err(want), vec!["label"]), "{state}");
    }
    assert_eq!(
        prepare("create-fails"),
        (Err(Outcome::Forbidden), vec!["label", "name", "create"])
    );
}

#[test]
fn prepare_renames_a_label_in_another_case_and_leaves_one_that_is_right() {
    let specs = table::specs();
    let (mut gh, asked, input) = adapter_input("mixed");
    assert_eq!(gh.prepare(), Ok(()));
    let all = asked.lock().unwrap().clone();
    let first = calls(&asked);
    assert_eq!(first[0], "held");
    assert_eq!(first.iter().filter(|c| **c == "held").count(), 1);
    assert_eq!(first.iter().filter(|c| **c == "rename").count(), 1);
    // The right label and the renamed one are not made; every other is.
    assert_eq!(
        first.iter().filter(|c| **c == "create").count(),
        specs.len() - 2
    );
    let made: Vec<&str> = all
        .iter()
        .filter(|a| a[0] == "label")
        .map(|a| a[2].as_str())
        .collect();
    assert!(!made.contains(&"tehas:ready"), "{made:?}");
    assert!(!made.contains(&"tehas:running"), "{made:?}");
    assert!(made.contains(&"tehas:pause"), "{made:?}");
    let rename = all.iter().find(|a| a[2] == "PATCH").unwrap();
    assert_eq!(rename[3], "repos/acme/widget/labels/Tehas%3ARunning");
    let sent: serde_json::Value = serde_json::from_str(&input.lock().unwrap()[0]).unwrap();
    assert_eq!(sent["new_name"], "tehas:running");
    assert_eq!(sent["color"], "1d76db");
    // Brought in one call: the second reads the labels and writes nothing.
    asked.lock().unwrap().clear();
    assert_eq!(gh.prepare(), Ok(()));
    assert_eq!(calls(&asked), ["held"]);
}
