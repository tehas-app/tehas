//! The adapter against the ports' recorded samples of GitHub: each sample in
//! `console/ports/samples/github/` names a port call and the answer it must
//! give, and the adapter answers it from its own recordings of the fixture
//! project, under `tests/recorded/`, replayed through a runner. The laws of
//! `tehas_ports::laws` then check the same answers.

use serde::Deserialize;
use std::path::PathBuf;
use tehas_core::forge::GhRunner;
use tehas_github::GitHub;
use tehas_ports::laws;
use tehas_ports::{Actor, Got, HappenedKind, Mark, TicketEvent, TicketKey, Took, Window};

/// The fixture project the recordings were made against.
const PROJECT: &str = "acme/widget";

fn here() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// A runner that answers each call from the recording of the fixture item
/// it asks about: `<item>/<read>.json` is gh's answer, `<item>/<read>.fail`
/// the reason the call failed. A call with no recording fails.
struct Replay;

impl GhRunner for Replay {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        let path = args.get(1).copied().unwrap_or_default();
        let rest = path
            .strip_prefix(&format!("repos/{PROJECT}/issues/"))
            .ok_or_else(|| format!("no recording of {path}"))?;
        let (issue, read) = match rest.split_once('/') {
            Some((n, "events?per_page=100")) => (n, "events"),
            None => (rest, "issue"),
            Some(_) => return Err(format!("no recording of {path}")),
        };
        let at = here().join("tests/recorded").join(format!("issue-{issue}"));
        if let Ok(out) = std::fs::read_to_string(at.join(format!("{read}.json"))) {
            return Ok(out);
        }
        match std::fs::read_to_string(at.join(format!("{read}.fail"))) {
            Ok(why) => Err(why.trim_end().to_string()),
            Err(_) => Err(format!("no recording of {path}")),
        }
    }
}

/// The time the replay's clock reads: a spent budget answers it as `until`.
fn replay_now() -> i64 {
    1_767_225_600
}

fn adapter() -> GitHub {
    GitHub::new(PROJECT, Box::new(Replay)).on_clock(replay_now)
}

fn samples<T: for<'de> Deserialize<'de>>(file: &str) -> Vec<T> {
    let at = here().join("../ports/samples/github").join(file);
    let text = std::fs::read_to_string(&at).unwrap_or_else(|e| panic!("{}: {e}", at.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", at.display()))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct History {
    op: String,
    actor: Actor,
    fixture: String,
    ticket: TicketKey,
    window: Window,
    only: Option<Vec<HappenedKind>>,
    answer: Got<Took<TicketEvent>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Marks {
    op: String,
    part: String,
    actor: Actor,
    fixture: String,
    ticket: TicketKey,
    answer: Got<Vec<Mark>>,
}

const MARKED: [HappenedKind; 2] = [HappenedKind::Marked, HappenedKind::Unmarked];

#[test]
fn the_history_answers_as_each_sample_says() {
    let all: Vec<History> = samples("tickets-history.json");
    assert!(!all.is_empty());
    for s in all {
        assert_eq!(s.op, "Tickets::history");
        assert!(matches!(s.actor, Actor::Ours(_)), "{}", s.fixture);
        let got = adapter().history(&s.ticket, s.window, s.only.as_deref());
        assert_eq!(
            got, s.answer,
            "{:?} {:?}: {}",
            s.ticket, s.window, s.fixture
        );
    }
}

#[test]
fn the_marks_answer_as_each_sample_says() {
    let all: Vec<Marks> = samples("tickets-marks.json");
    assert!(!all.is_empty());
    for s in all {
        assert_eq!(
            (s.op.as_str(), s.part.as_str()),
            ("Tickets::ticket", "digest.marks")
        );
        assert!(matches!(s.actor, Actor::Ours(_)), "{}", s.fixture);
        assert_eq!(adapter().marks(&s.ticket), s.answer, "{}", s.fixture);
    }
}

#[test]
fn the_history_keeps_the_laws_of_lists() {
    let key = TicketKey::Number(std::num::NonZeroU64::new(7).unwrap());
    let held = adapter()
        .history(&key, Window::Whole(tehas_ports::ALL_MAX), Some(&MARKED))
        .unwrap()
        .items
        .len() as u32;
    assert!(held >= 2);
    let mut gh = adapter();
    let mut read = |w| gh.history(&key, w, Some(&MARKED));
    assert_eq!(laws::lists::last_n_is_never_cut(&mut read, held), Ok(()));
    assert_eq!(laws::lists::more_is_exact(&mut read, held), Ok(()));
}

fn ticket(n: u64) -> TicketKey {
    TicketKey::Number(std::num::NonZeroU64::new(n).unwrap())
}

#[test]
fn a_name_no_mark_can_have_is_left_out_and_kept_once_to_report() {
    let mut gh = adapter();
    gh.marks(&ticket(7)).unwrap();
    gh.marks(&ticket(7)).unwrap();
    assert_eq!(gh.take_left_out(), ["not a label?"]);
    assert!(gh.take_left_out().is_empty());
}
