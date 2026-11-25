//! `tehas labels` end to end against a stub gh: which labels it makes, where,
//! that it needs the repository and nothing else, and the pins of the
//! granting labels in state/labels. And `console label allowed`: who applied
//! a label, asked of GitHub. Ported from test/labels_test.sh (T15).

mod common;

use common::*;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

/// gh for `tehas labels`. With GH_MADE set, a label exists only once a label
/// create made it there; FAIL_ON fails any call that holds it; GH_CREATED
/// answers the question of when the labels were made.
fn labels_gh() -> String {
    let none = root().join("test/fixtures/labels-none.json");
    format!(
        r#"case "$*" in *"${{FAIL_ON:-no label fails}}"*) exit 1 ;;
  "label create "*) [ -z "${{GH_MADE:-}}" ] || touch "$GH_MADE/$3" ;;
  "api repos/"*/labels/*) n=${{2##*/}}; [ -z "${{GH_MADE:-}}" ] || [ -e "$GH_MADE/$n" ] || exit 1; echo "LA_${{n#tehas:}}" ;;
  *"nodes {{ name color description }}"*) cat '{}' ;;
  "api graphql "*) [ -z "${{GH_CREATED:-}}" ] || cat "$GH_CREATED" ;;
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
    w.home.join("state/labels")
}

fn pins_text(w: &World) -> String {
    fs::read_to_string(pins(w)).unwrap_or_default()
}

fn calls_of(w: &World, what: &str) -> usize {
    w.gh().lines().filter(|l| l.contains(what)).count()
}

/// The seven granting labels, as state/labels pins them.
const GRANTING: &[&str] = &[
    "ready", "plan", "refine", "finish", "blocked", "lean", "classic",
];

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
    for name in ["tehas:question", "tehas:plan", "tehas:planning"] {
        has(&gh, &format!("label create {name} "));
    }
    // Each granting label is pinned to its id once the labels are made.
    let pinned = pins_text(&w);
    for name in GRANTING {
        has(
            &gh,
            &format!("api repos/acme/widget/labels/tehas:{name} --jq .node_id"),
        );
        has(
            &r.out,
            &format!("pinned the label tehas:{name} as LA_{name} in state/labels"),
        );
        has(&pinned, &format!("tehas:{name}\tLA_{name}"));
    }
    assert_eq!(pinned.lines().count(), 7);
    lacks(&pinned, "tehas:running");
}

#[test]
fn once_the_pins_exist_a_label_there_with_no_pin_is_named_and_not_pinned() {
    let w = host("unpinned");
    fs::create_dir_all(pins(&w).parent().unwrap()).unwrap();
    fs::write(pins(&w), "tehas:ready\tLA_old\n").unwrap();
    let r = labels(&w, &[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "not pinned: the label tehas:ready is LA_ready, and state/labels pins LA_old",
    );
    has(
        &r.out,
        "not pinned: the label tehas:plan (LA_plan) was there before with no pin",
    );
    // Left as it was.
    assert_eq!(pins_text(&w), "tehas:ready\tLA_old\n");
}

#[test]
fn once_the_pins_exist_a_label_this_run_made_is_pinned() {
    let w = host("made");
    let made = w.t().join("gh-made");
    fs::create_dir_all(&made).unwrap();
    w.env("GH_MADE", &made.to_string_lossy());
    fs::create_dir_all(pins(&w).parent().unwrap()).unwrap();
    fs::write(pins(&w), "tehas:ready\tLA_ready\n").unwrap();
    fs::write(made.join("tehas:ready"), "").unwrap();
    let r = labels(&w, &[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "pinned the label tehas:plan as LA_plan in state/labels",
    );
    assert_eq!(pins_text(&w).lines().count(), 7);
}

/// The pins of every granting label but tehas:blocked, and GitHub's answer of
/// when tehas:ready and tehas:blocked were made.
fn blocked_unpinned(w: &World, blocked_made: &str) {
    let text: String = GRANTING
        .iter()
        .filter(|n| **n != "blocked")
        .map(|n| format!("tehas:{n}\tLA_{n}\n"))
        .collect();
    fs::create_dir_all(pins(w).parent().unwrap()).unwrap();
    fs::write(pins(w), text).unwrap();
    let created = w.t().join("created");
    fs::write(
        &created,
        format!("LA_ready\t2026-10-19T18:16:01Z\nLA_blocked\t{blocked_made}\n"),
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
    has(&w.gh(), "api graphql -f query=");
    has(&w.gh(), "-f owner=acme -f name=widget");
    has(
        &r.out,
        "pinned the label tehas:blocked as LA_blocked in state/labels: it was made with the pinned labels",
    );
    assert_eq!(pins_text(&w).lines().count(), 7);
}

#[test]
fn one_made_long_after_the_pinned_ones_is_still_refused() {
    let w = host("created-after");
    blocked_unpinned(&w, "2026-11-19T09:00:00Z");
    let r = labels(&w, &[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "not pinned: the label tehas:blocked (LA_blocked) was there before with no pin",
    );
    lacks(&pins_text(&w), "tehas:blocked");
}

#[test]
fn a_label_that_cannot_be_made_is_named_and_the_rest_are_still_made() {
    let w = host("fails");
    w.env("FAIL_ON", "tehas:lean");
    let r = labels(&w, &[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "tehas labels: could not make the label tehas:lean: ",
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
        "label create tehas:ready --repo acme/widget --color 0e8a16 --description Tehas: eligible for dispatch (apply by hand to authorize a cycle) --force",
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
        "TEHAS_REPO=acme/widget TEHAS_ALLOWED_LABELERS='alice bob' '{}' label allowed 7 tehas:ready",
        w.console().display()
    ))
}

/// Each of `logins` applies tehas:ready in turn.
fn labelled(logins: &[&str]) -> Vec<Value> {
    logins
        .iter()
        .enumerate()
        .map(|(i, login)| {
            json!({"event": "labeled", "label": {"name": "tehas:ready"},
                "actor": {"login": login}, "created_at": format!("2026-10-1{}T00:00:00Z", i + 1)})
        })
        .collect()
}

fn events(w: &World, list: &[Value]) {
    fs::write(
        w.t().join("events.json"),
        Value::from(list.to_vec()).to_string(),
    )
    .unwrap();
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
        let mut list = labelled(&[login]);
        list[0]["actor"]["id"] = id.clone();
        events(&w, &list);
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
