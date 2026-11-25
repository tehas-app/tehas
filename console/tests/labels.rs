//! `tehas labels` end to end against a stub gh: which labels it makes, where,
//! that it needs the repository and nothing else, and the pins of the
//! granting labels in state/labels. And `console label allowed`: who applied
//! a label, asked of GitHub. Ported from test/labels_test.sh (T15).

mod common;

use common::*;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use tehas_core::home::State;

/// gh for `tehas labels`. With GH_MADE set, a label exists only once a label
/// create made it there; FAIL_ON fails any call that holds it; GH_CREATED
/// answers the question of when the labels were made. A label's node id is
/// the stub's common answer, LA_ and the label's word.
fn labels_gh() -> String {
    let none = root().join("test/fixtures/labels-none.json");
    format!(
        r#"case "$*" in *"${{FAIL_ON:-no label fails}}"*) exit 1 ;;
  "label create "*) [ -z "${{GH_MADE:-}}" ] || touch "$GH_MADE/$3" ;;
  "api repos/"*/labels/*) n=${{2##*/}}; [ -z "${{GH_MADE:-}}" ] || [ -e "$GH_MADE/$n" ] || exit 1 ;;
  *"nodes {{ name color description }}"*) cat '{}' ;;
  "api "*" -f owner=acme -f name=widget"*) [ -z "${{GH_CREATED:-}}" ] || cat "$GH_CREATED" ;;
esac"#,
        none.display()
    )
}

/// The rows of the label table.
fn table() -> Vec<(String, String)> {
    fs::read_to_string(root().join("host/labels.tsv"))
        .unwrap()
        .lines()
        .map(|l| {
            let mut f = l.split('\t');
            let name = f.next().unwrap_or_default().to_string();
            (name, l.to_string())
        })
        .collect()
}

/// A host with only the repository set (and the App's id), its account ids
/// recorded, gh answering as `labels_gh`.
fn host(name: &str) -> World {
    let w = World::new(&format!("labels-{name}"));
    w.instance(&["TEHAS_GITHUB_APP_ID=1", "TEHAS_REPO=acme/widget"]);
    w.gh_stub(&labels_gh());
    w
}

fn labels(w: &World, words: &[&str]) -> Run {
    w.tehas("labels", words)
}

fn pins(w: &World) -> PathBuf {
    w.at().state(State::LabelPins)
}

fn pins_text(w: &World) -> String {
    fs::read_to_string(pins(w)).unwrap_or_default()
}

fn calls_of(w: &World, what: &str) -> usize {
    w.gh().lines().filter(|l| l.contains(what)).count()
}

/// The words of the seven granting labels, which state/labels pins.
const GRANTING: &[&str] = &[
    "Act::Approve",
    "Act::Plan",
    "Act::Refine",
    "Act::Finish",
    "Act::Block",
    "Choice::Lean",
    "Choice::Classic",
];

/// The node id the stub gives a label: LA_ and the label without its prefix.
fn id(label: &str) -> String {
    format!("LA_{}", label.split_once(':').map_or(label, |(_, w)| w))
}

#[test]
fn it_needs_only_the_repository_and_makes_every_label_of_the_table_there() {
    let w = host("all");
    let rows = table();
    let r = labels(&w, &[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(calls_of(&w, "label create"), rows.len());
    assert_eq!(
        w.gh()
            .lines()
            .filter(|l| l.contains("--repo acme/widget") && l.contains("--force"))
            .count(),
        rows.len()
    );
    has(&r.out, "made the tehas labels");
    let gh = w.gh();
    for (name, _) in &rows {
        has(&gh, &format!("label create {name} --repo acme/widget"));
    }
    // The question label and the planning labels among them.
    for word in ["State::Question", "Act::Plan", "State::Planning"] {
        has(&gh, &format!("label create {} ", label_of(word)));
    }
    // Each granting label is pinned to its id once the labels are made. The
    // stub answers the id only to the read of the label's node_id.
    let pinned = pins_text(&w);
    for word in GRANTING {
        let name = label_of(word);
        let id = id(&name);
        has(&gh, &["api repos/acme/widget/labels/", &name, " "].concat());
        has(
            &r.out,
            &format!("pinned the label {name} as {id} in state/labels"),
        );
        has(&pinned, &format!("{name}\t{id}"));
    }
    assert_eq!(pinned.lines().count(), 7);
    lacks(&pinned, &label_of("State::Running"));
}

#[test]
fn once_the_pins_exist_a_label_there_with_no_pin_is_named_and_not_pinned() {
    let w = host("unpinned");
    let (ready, plan) = (label_of("Act::Approve"), label_of("Act::Plan"));
    let old = format!("{ready}\tLA_old\n");
    w.home_file(pins(&w), &old);
    let r = labels(&w, &[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        &format!(
            "not pinned: the label {ready} is {}, and state/labels pins LA_old",
            id(&ready)
        ),
    );
    has(
        &r.out,
        &format!(
            "not pinned: the label {plan} ({}) was there before with no pin",
            id(&plan)
        ),
    );
    // Left as it was.
    assert_eq!(pins_text(&w), old);
}

#[test]
fn once_the_pins_exist_a_label_this_run_made_is_pinned() {
    let w = host("made");
    let made = w.t().join("gh-made");
    fs::create_dir_all(&made).unwrap();
    w.env("GH_MADE", &made.to_string_lossy());
    let (ready, plan) = (label_of("Act::Approve"), label_of("Act::Plan"));
    w.home_file(pins(&w), &format!("{ready}\t{}\n", id(&ready)));
    fs::write(made.join(&ready), "").unwrap();
    let r = labels(&w, &[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        &format!("pinned the label {plan} as {} in state/labels", id(&plan)),
    );
    assert_eq!(pins_text(&w).lines().count(), 7);
}

/// The pins of every granting label but the blocked one, and GitHub's answer
/// of when the ready and the blocked label were made.
fn blocked_unpinned(w: &World, blocked_made: &str) {
    let text: String = GRANTING
        .iter()
        .filter(|w| **w != "Act::Block")
        .map(|w| label_of(w))
        .map(|n| format!("{n}\t{}\n", id(&n)))
        .collect();
    w.home_file(pins(w), &text);
    let created = w.t().join("created");
    fs::write(
        &created,
        format!(
            "{}\t2026-10-19T18:16:01Z\n{}\t{blocked_made}\n",
            id(&label_of("Act::Approve")),
            id(&label_of("Act::Block"))
        ),
    )
    .unwrap();
    w.env("GH_CREATED", &created.to_string_lossy());
}

#[test]
fn a_label_there_with_no_pin_is_pinned_when_github_says_it_was_made_with_a_pinned_one() {
    let w = host("created-with");
    blocked_unpinned(&w, "2026-10-19T18:16:09Z");
    let r = labels(&w, &[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    // A question of the repository, by its owner and name.
    has(&w.gh(), " -f query=");
    has(&w.gh(), "-f owner=acme -f name=widget");
    let blocked = label_of("Act::Block");
    has(
        &r.out,
        &format!(
            "pinned the label {blocked} as {} in state/labels: it was made with the pinned labels",
            id(&blocked)
        ),
    );
    assert_eq!(pins_text(&w).lines().count(), 7);
}

#[test]
fn one_made_long_after_the_pinned_ones_is_still_refused() {
    let w = host("created-after");
    blocked_unpinned(&w, "2026-11-19T09:00:00Z");
    let r = labels(&w, &[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    let blocked = label_of("Act::Block");
    has(
        &r.out,
        &format!(
            "not pinned: the label {blocked} ({}) was there before with no pin",
            id(&blocked)
        ),
    );
    lacks(&pins_text(&w), &blocked);
}

#[test]
fn a_label_that_cannot_be_made_is_named_and_the_rest_are_still_made() {
    let w = host("fails");
    let lean = label_of("Choice::Lean");
    w.env("FAIL_ON", &lean);
    let r = labels(&w, &[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        &format!("tehas labels: could not make the label {lean}: "),
    );
    assert_eq!(calls_of(&w, "label create"), table().len());
}

#[test]
fn without_the_repository_nothing_is_called() {
    let w = host("no-repo");
    w.instance(&["TEHAS_GITHUB_APP_ID=1", "TEHAS_CONTROL_ISSUE=1"]);
    let r = labels(&w, &[]);
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "TEHAS_REPO is not set");
    assert_eq!(w.gh(), "");
}

#[test]
fn an_argument_is_a_usage_error_before_the_settings_are_read() {
    let w = host("usage");
    w.instance(&["TEHAS_GITHUB_APP_ID=1", "TEHAS_CONTROL_ISSUE=1"]);
    let r = labels(&w, &["--force"]);
    assert_eq!(r.rc, 64, "{}", r.out);
    has(&r.out, "usage: tehas labels");
    lacks(&r.out, "TEHAS_REPO");
    assert_eq!(w.gh(), "");
}

#[test]
fn a_labels_description_reaches_gh_as_one_argument_as_the_table_has_it() {
    let w = host("description");
    labels(&w, &[]);
    has(
        &w.gh(),
        &format!(
            "label create {} --repo acme/widget --color 0e8a16 --description Tehas: eligible for dispatch (apply by hand to authorize a cycle) --force",
            label_of("Act::Approve")
        ),
    );
}

// --- console label allowed ----------------------------------------------------

const EVENTS: &str = "api repos/acme/widget/issues/7/events?per_page=100 --paginate";

/// A host whose gh answers issue 7's events from events.json, or fails while
/// events-fail is there.
fn events_host(name: &str) -> World {
    let w = World::new(&format!("labels-allowed-{name}"));
    w.gh_stub(&format!(
        r#"case "$*" in
  "{EVENTS}")
    [ ! -e "$T/events-fail" ] || exit 1
    cat "$T/events.json" ;;
esac"#
    ));
    w
}

fn allowed(w: &World) -> Run {
    w.sh(&format!(
        "TEHAS_REPO=acme/widget TEHAS_ALLOWED_LABELERS='alice bob' '{}' label allowed 7 {}",
        w.console().display(),
        label_of("Act::Approve")
    ))
}

/// Each of `logins` applies the ready label in turn: the events as GitHub's
/// JSON, one text each.
fn labelled(logins: &[&str]) -> Vec<String> {
    let ready = label_of("Act::Approve");
    logins
        .iter()
        .enumerate()
        .map(|(i, login)| {
            json!({"event": "labeled", "label": {"name": ready},
                "actor": {"login": login}, "created_at": format!("2026-10-1{}T00:00:00Z", i + 1)})
            .to_string()
        })
        .collect()
}

fn events(w: &World, list: &[String]) {
    fs::write(w.t().join("events.json"), format!("[{}]", list.join(","))).unwrap();
}

#[test]
fn the_label_counts_when_an_allowed_account_applied_it_most_recently() {
    let w = events_host("counts");
    events(&w, &labelled(&["mallory", "bob"]));
    let r = allowed(&w);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.gh().trim_end(), EVENTS);
}

#[test]
fn it_does_not_when_the_last_account_is_not_allowed_or_no_event_applies_it() {
    let w = events_host("not");
    events(&w, &labelled(&["alice", "mallory"]));
    assert_eq!(allowed(&w).rc, 1);
    events(&w, &[]);
    assert_eq!(allowed(&w).rc, 1);
}

#[test]
fn it_does_not_when_the_allowed_login_in_any_case_comes_with_another_user_id_or_none() {
    let w = events_host("ids");
    for (login, id) in [
        ("bob", json!(99)),
        ("bob", Value::Null),
        ("Bob", json!(99)),
        ("BOB", Value::Null),
    ] {
        let mut event: Value = serde_json::from_str(&labelled(&[login])[0]).unwrap();
        event["actor"]["id"] = id.clone();
        events(&w, &[event.to_string()]);
        let r = allowed(&w);
        assert_eq!(r.rc, 1, "{login} {id}: {}", r.out);
        has(&r.out, "does not count");
    }
}

#[test]
fn events_that_cannot_be_read_are_2_not_a_refusal() {
    let w = events_host("unread");
    fs::write(w.t().join("events-fail"), "").unwrap();
    assert_eq!(allowed(&w).rc, 2);
    fs::remove_file(w.t().join("events-fail")).unwrap();
    fs::write(w.t().join("events.json"), r#"{"message":"Not Found"}"#).unwrap();
    assert_eq!(allowed(&w).rc, 2);
}
