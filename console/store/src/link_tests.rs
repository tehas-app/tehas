//! link-record against a real store: the one writer of `links`.

use crate::verbs_tests::*;
use serde_json::json;
use std::path::Path;

const AT: i64 = 1_790_000_000;

/// The link on record after a call, as compact JSON ("null" for none), and
/// whether the call changed it.
type Answer = Result<(String, bool), (u8, String)>;

fn answer(home: &Path, now: i64, args: &[&str]) -> Answer {
    call_at(home, now, args).map(|v| {
        assert_eq!(v["verb"], "link-record");
        (v["link"].to_string(), v["changed"].as_bool().unwrap())
    })
}

fn link(home: &Path, blueprint: &str, ticket: &str, how: &str) -> Answer {
    answer(
        home,
        AT,
        &[
            "link-record",
            blueprint,
            "--ticket",
            ticket,
            "--by",
            "alice",
            "--realm",
            "tickets",
            "--how",
            how,
        ],
    )
}

fn unlink(home: &Path, blueprint: &str, ticket: &str) -> Answer {
    answer(
        home,
        AT + 5,
        &[
            "link-record",
            blueprint,
            "--ticket",
            ticket,
            "--by",
            "alice",
            "--realm",
            "tickets",
            "--remove",
        ],
    )
}

fn row(ticket: &str, blueprint: i64, how: &str) -> String {
    json!({"ticket": ticket, "blueprint": blueprint, "made_by": "alice",
           "made_by_realm": "tickets", "made_at": AT, "how": how})
    .to_string()
}

#[test]
fn a_link_is_recorded_once_with_who_made_it_and_stands() {
    let home = home("link-record");
    assert_eq!(count(&home, "links"), 0);
    assert_eq!(
        link(&home, "7", "PROJ-1", "linked").unwrap(),
        (row("PROJ-1", 7, "linked"), true)
    );
    // The same link again stands as it was written: who, when and how.
    let again = answer(
        &home,
        AT + 60,
        &[
            "link-record",
            "7",
            "--ticket",
            "PROJ-1",
            "--by",
            "bob",
            "--realm",
            "code-host",
            "--how",
            "opened",
        ],
    )
    .unwrap();
    assert_eq!(again, (row("PROJ-1", 7, "linked"), false));
    // A ticket has many blueprints.
    link(&home, "8", "PROJ-1", "opened").unwrap();
    assert_eq!(count(&home, "links WHERE ticket = 'PROJ-1'"), 2);
}

#[test]
fn a_blueprint_has_one_ticket_and_another_is_refused() {
    let home = home("link-other");
    link(&home, "7", "PROJ-1", "linked").unwrap();
    let (code, why) = link(&home, "7", "PROJ-2", "linked").unwrap_err();
    assert_eq!(code, 1);
    assert!(why.contains("linked to another ticket"), "{why}");
    let (_, why) = unlink(&home, "7", "PROJ-2").unwrap_err();
    assert!(why.contains("linked to another ticket"), "{why}");
    assert_eq!(count(&home, "links WHERE ticket = 'PROJ-1'"), 1);
}

#[test]
fn the_same_act_with_remove_takes_the_link_away() {
    let home = home("link-remove");
    assert_eq!(
        unlink(&home, "7", "PROJ-1").unwrap(),
        ("null".into(), false)
    );
    link(&home, "7", "PROJ-1", "linked").unwrap();
    assert_eq!(unlink(&home, "7", "PROJ-1").unwrap(), ("null".into(), true));
    assert_eq!(count(&home, "links"), 0);
    // Taken away, the blueprint may be linked to another ticket.
    link(&home, "7", "PROJ-2", "linked").unwrap();
    assert_eq!(one::<String>(&home, "SELECT ticket FROM links"), "PROJ-2");
}

#[test]
fn what_the_verb_takes_is_checked_before_anything_is_written() {
    let base = [
        "link-record",
        "7",
        "--ticket",
        "PROJ-1",
        "--by",
        "alice",
        "--realm",
        "tickets",
    ];
    let with = |more: &[&'static str]| {
        let mut a = base.to_vec();
        a.extend_from_slice(more);
        a
    };
    assert!(usage(&with(&[])).contains("--how is required, or --remove"));
    assert!(
        usage(&with(&["--how", "linked", "--remove"])).contains("--how is not taken with --remove")
    );
    assert!(usage(&with(&["--how", "same-item"])).contains("--how is not one of opened, linked"));
    assert!(usage(&[
        "link-record",
        "7",
        "--ticket",
        "PROJ-1",
        "--by",
        "alice",
        "--realm",
        "jira",
        "--how",
        "linked"
    ])
    .contains("--realm is not one of code-host, tickets"));
    assert!(usage(&[
        "link-record",
        "7",
        "--ticket",
        "PROJ 1",
        "--by",
        "alice",
        "--realm",
        "tickets",
        "--how",
        "linked"
    ])
    .contains("--ticket"));
    assert!(usage(&[
        "link-record",
        "0",
        "--ticket",
        "PROJ-1",
        "--by",
        "alice",
        "--realm",
        "tickets",
        "--how",
        "linked"
    ])
    .contains("<issue>"));
    assert!(usage(&[
        "link-record",
        "7",
        "--ticket",
        "PROJ-1",
        "--realm",
        "tickets",
        "--how",
        "linked"
    ])
    .contains("--by is required"));
    let home = home("link-usage");
    assert_eq!(count(&home, "links"), 0);
}
