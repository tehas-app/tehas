//! `TicketDigest.labels` as the adapter answers it: the site's own label
//! names, in its order, from the reads it makes for the marks, beside them.
//! The ports' samples in `console/ports/samples/github/tickets-labels.json`
//! name the answer; the adapter gives it from its recordings of the fixture
//! project under `tests/recorded/`, and the law of the ports checks
//! `several` and `list` alike.

mod replay;

use replay::{adapter_of, PROJECT};
use serde::Deserialize;
use std::path::PathBuf;
use tehas_ports::{laws, Actor, Bound, Filter, Got, TicketKey, Tickets};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Labels {
    op: String,
    part: String,
    actor: Actor,
    fixture: String,
    ticket: TicketKey,
    answer: Got<Vec<String>>,
}

fn samples() -> Vec<Labels> {
    let at = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../ports/samples/github/tickets-labels.json");
    let text = std::fs::read_to_string(&at).unwrap_or_else(|e| panic!("{}: {e}", at.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", at.display()))
}

#[test]
#[ignore = "waits for beta2b: the parents' question of the fixture project has no recording until kit run #2"]
fn the_labels_answer_as_each_sample_says() {
    let all = samples();
    assert!(!all.is_empty());
    for s in all {
        assert_eq!(
            (s.op.as_str(), s.part.as_str()),
            ("Tickets::several", "digest.labels")
        );
        assert!(matches!(s.actor, Actor::Ours(_)), "{}", s.fixture);
        let (mut gh, _) = adapter_of(PROJECT);
        let (_, got) = gh.several(std::slice::from_ref(&s.ticket)).remove(0);
        assert_eq!(got.map(|d| d.labels), s.answer, "{}", s.fixture);
    }
}

#[test]
#[ignore = "waits for beta2b: the parents' question of the fixture project has no recording until kit run #2"]
fn every_label_is_in_labels_in_order() {
    let held: Vec<(TicketKey, Vec<String>)> = samples()
        .into_iter()
        .map(|s| (s.ticket, s.answer.expect("a sample the site answers")))
        .collect();
    let (mut gh, _) = adapter_of(PROJECT);
    assert_eq!(
        laws::tickets::every_label_is_in_labels_in_order(&mut gh, &held, Bound::new(50).unwrap()),
        Ok(())
    );
}

#[test]
#[ignore = "waits for beta2b: the parents' question of the fixture project has no recording until kit run #2"]
fn labels_cost_no_call_of_their_own() {
    // One issue read and one question for its parent, as before labels.
    let (mut gh, asked) = adapter_of(PROJECT);
    let key = samples().remove(0).ticket;
    gh.several(&[key]).remove(0).1.unwrap();
    assert_eq!(asked.get(), 2);
    // The list's one question gives them for every ticket it lists.
    let (mut gh, asked) = adapter_of(PROJECT);
    let listed = gh
        .list(&Filter::default(), Bound::new(50).unwrap())
        .unwrap();
    assert!(listed.into_items().iter().any(|d| !d.labels.is_empty()));
    assert_eq!(asked.get(), 1);
}
