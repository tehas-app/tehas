//! `TicketWrites::prepare_one` as the adapter answers it, replayed from
//! the recordings under `tests/recorded/prepare-one/<state>/` (made by
//! hand from GitHub's documented shapes): `label` is the read of the one
//! label, `labels` the read of every label, `create` the label's write.
//! After a write the replay answers from `made/` where the state has it,
//! so the law reads the site the write left. `.json` is gh's answer,
//! `.fail` the reason it failed and `.answer.json` what gh printed beside
//! it. No test calls GitHub.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tehas_core::forge::{Failed, GhRunner};
use tehas_github::GitHub;
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
}

fn field<'a>(args: &'a [&str], name: &str) -> Option<&'a str> {
    args.windows(2)
        .filter(|w| w[0] == "-f" || w[0] == "-F")
        .find_map(|w| w[1].strip_prefix(&format!("{name}=")))
}

impl Replay {
    /// The recording a call is answered from, by its read.
    fn read(&self, name: &str) -> Option<String> {
        let at = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/recorded/prepare-one")
            .join(&self.state);
        let made = at.join("made").join(name);
        let at = if self.made && made.exists() {
            made
        } else {
            at.join(name)
        };
        std::fs::read_to_string(at).ok()
    }

    fn name(args: &[&str]) -> &'static str {
        match args {
            ["label", "create", ..] => "create",
            ["api", "graphql", ..] => {
                let query = field(args, "query").expect("a query");
                if query.contains("label(name: $label)") {
                    assert_eq!(field(args, "owner"), Some("acme"));
                    assert_eq!(field(args, "name"), Some("widget"));
                    "label"
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
        if let Some(out) = self.read(&format!("{name}.json")) {
            if name == "create" {
                self.made = true;
            }
            return Ok(out);
        }
        match self.read(&format!("{name}.fail")) {
            Some(why) => Err(why.trim_end().to_string()),
            None => Err(format!("no recording {name} in {}", self.state)),
        }
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
    let asked = Asked::default();
    let replay = Replay {
        state: state.to_string(),
        made: false,
        asked: Arc::clone(&asked),
    };
    let gh = GitHub::new(PROJECT, Box::new(replay)).on_clock(replay_now);
    (gh, asked)
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
    // The marks, the label read and made, the marks, the label read again
    // and right; the project's own mark asks nothing.
    assert_eq!(
        calls(&asked),
        ["labels", "label", "create", "labels", "label"]
    );
}

#[test]
fn a_missing_label_is_made_as_prepare_makes_it() {
    let (mut gh, asked) = adapter("missing");
    assert_eq!(gh.prepare_one(&ESCALATED), Ok(()));
    let asked = asked.lock().unwrap();
    assert_eq!(asked.len(), 2);
    let read: Vec<&str> = asked[0].iter().map(String::as_str).collect();
    assert_eq!(field(&read, "label"), Some("tehas:millwright-escalated"));
    assert_eq!(asked[1].join(" "), the_write());
}

#[test]
fn a_label_with_another_colour_or_text_is_brought_back() {
    for state in ["wrong-colour", "wrong-text", "no-text", "other-case"] {
        let (mut gh, asked) = adapter(state);
        assert_eq!(gh.prepare_one(&ESCALATED), Ok(()), "{state}");
        let asked = asked.lock().unwrap();
        assert_eq!(asked.len(), 2, "{state}");
        assert_eq!(asked[1].join(" "), the_write(), "{state}");
    }
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
        (Err(Outcome::Forbidden), vec!["label", "create"])
    );
}
