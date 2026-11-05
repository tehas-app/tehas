//! How a cycle on the lean driver ends. Each result in tests/lean/ is the
//! document pi-rukas's `work-lean` verb wrote for one ending (tests/lean/ends.ts
//! says how they were made); the report reads it with the GitHub that ending
//! leaves and gives the issue its labels and exactly one comment.

use serde_json::{json, Value};
use std::fs;
use tehas_core::testdir::TestDir;
use tehas_cycle::report::{report, Action, Given};
use tehas_cycle::{progress, Facts, Report};

const PR: &str = "https://github.com/acme/widget/pull/41";

/// The bot's pull request on lean's branch, as GitHub lists it.
fn pr(state: &str, draft: bool) -> Value {
    json!([{"url": PR, "state": state, "isDraft": draft, "headRefName": "lean/issue-7",
            "body": "Closes #7"}])
}

/// The result of the ending `name`.
fn doc(name: &str) -> String {
    fs::read_to_string(format!(
        "{}/tests/lean/{name}.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

/// The report of the ending `name`, with `prs` on GitHub and the driver
/// `driver` on the approval this start used.
fn ended(name: &str, rc: i64, prs: Value, driver: &str) -> Report {
    read(name, &doc(name), rc, prs, driver)
}

/// The report of the result `result`, in a directory of its own named `case`.
fn read(case: &str, result: &str, rc: i64, prs: Value, driver: &str) -> Report {
    let dir = TestDir::new(&format!("lean-{case}"));
    fs::write(dir.join("result.json"), result).unwrap();
    fs::write(dir.join("stream.jsonl"), "").unwrap();
    let progress = progress::summary(
        &dir.join("stream.jsonl"),
        Some((&dir.join("result.json"), 7)),
    );
    let facts = json!({
        "issue": 7,
        "repo": "acme/widget",
        "home": "/home/tehas",
        "hostname": "tehas-test",
        "workdir": "/home/tehas/work/widget-7",
        "rc": rc,
        "started": 1000,
        "now": 1600,
        "start": {"attempt": 2, "by": "alice", "how": "label"},
        "files": {"result.json": result, "progress.json": progress, "runner_exit": format!("{rc}\n")},
        "record": {"cycle": {"attempt": 2}},
        "approval": {"current": {"used_by": 2, "how": "label", "input": {"links": [], "driver": driver}}},
        "github": {"prs": prs, "attention": false},
    });
    let f = Facts::from_json(&facts).expect("the facts read");
    report(&f, &mut Given(facts["github"].clone()))
}

/// The labels the report adds, and the one thing it says on the issue: the
/// comment, or the text of its park.
fn said(r: &Report) -> (Vec<String>, String) {
    let mut added = vec![];
    let mut on_issue = vec![];
    for a in &r.actions {
        match a {
            Action::Relabel { add, .. } => added.extend(add.clone()),
            Action::Comment { ok, .. } => on_issue.push(ok.clone()),
            Action::Park(p) => {
                added.push("needs-human-attention".into());
                on_issue.push(p.comment(false, true));
            }
            _ => {}
        }
    }
    assert_eq!(on_issue.len(), 1, "one comment on the issue: {on_issue:?}");
    (added, on_issue.remove(0))
}

fn end_arg(r: &Report, flag: &str) -> Option<String> {
    let i = r.end.iter().position(|a| a == flag)?;
    r.end.get(i + 1).cloned()
}

#[test]
fn a_lean_run_that_landed_takes_the_cycle_labels_off() {
    let r = ended("landed", 0, pr("MERGED", false), "lean");
    assert_eq!(r.arm, "merged");
    let (added, comment) = said(&r);
    assert!(added.is_empty(), "{added:?}");
    assert!(
        comment.contains(&format!("The change was merged: {PR}")),
        "{comment}"
    );
    assert!(comment.contains("driver `lean`"), "{comment}");
    assert_eq!(end_arg(&r, "--branch").as_deref(), Some("lean/issue-7"));
}

#[test]
fn a_green_ready_lean_pull_request_waits_for_a_merge() {
    let r = ended("green", 10, pr("OPEN", false), "lean");
    assert_eq!(r.arm, "awaiting-merge");
    let (added, comment) = said(&r);
    assert_eq!(added, ["tehas:awaiting-merge"]);
    assert!(
        comment.contains(&format!("Pull request waiting for a human: {PR}")),
        "{comment}"
    );
    assert!(comment.contains("driver `lean`"), "{comment}");
}

#[test]
fn a_lean_pull_request_github_shows_as_a_draft_never_waits_for_a_merge() {
    let r = ended("green", 10, pr("OPEN", true), "lean");
    assert_eq!(r.arm, "handoff-parked");
    let (added, comment) = said(&r);
    assert_eq!(added, ["needs-human-attention"]);
    assert!(
        comment.contains("Lean left its pull request a draft"),
        "{comment}"
    );
}

#[test]
fn a_red_lean_draft_parks_and_says_why() {
    let r = ended("draft", 10, pr("OPEN", true), "lean");
    assert_eq!(r.arm, "handoff-parked");
    let (added, comment) = said(&r);
    assert_eq!(added, ["needs-human-attention"]);
    assert!(
        comment.contains("Lean left its pull request a draft"),
        "{comment}"
    );
    assert!(comment.contains("kind `verify`"), "{comment}");
    assert!(
        comment.contains("acceptance test test-ac2 is red"),
        "{comment}"
    );
    assert!(comment.contains("driver `lean`"), "{comment}");
    // Lean handed its report to the host, which posts it once.
    assert_eq!(
        comment.matches("The draft pull request").count(),
        1,
        "{comment}"
    );
}

#[test]
fn a_ready_claim_over_a_draft_state_is_not_taken() {
    let claim = doc("green").replace("\"prState\": \"ready\"", "\"prState\": \"draft\"");
    assert!(claim.contains("\"prState\": \"draft\""));
    let r = read("claim", &claim, 10, pr("OPEN", false), "lean");
    assert_eq!(r.arm, "handoff-parked");
    let (_, comment) = said(&r);
    assert!(comment.contains("is `draft`, not `ready`"), "{comment}");
}

#[test]
fn a_lean_run_with_no_pull_request_parks_with_its_reason() {
    let r = ended("nopr", 10, json!([]), "lean");
    assert_eq!(r.arm, "parked");
    let (added, comment) = said(&r);
    assert_eq!(added, ["needs-human-attention"]);
    assert!(
        comment.contains("stopped without a pull request"),
        "{comment}"
    );
    assert!(
        comment.contains("the developer produced no change"),
        "{comment}"
    );
    assert!(
        comment.contains("The build produced no change"),
        "{comment}"
    );
}

#[test]
fn a_lean_run_that_found_the_issue_not_ready_asks_for_a_pi_spec_block() {
    let r = ended("notready", 11, json!([]), "lean");
    assert_eq!(r.arm, "not-ready");
    let (added, comment) = said(&r);
    assert_eq!(added, ["needs-human-attention"]);
    assert!(comment.contains("not ready to build from"), "{comment}");
    assert!(comment.contains("needs a `pi-spec v1` block"), "{comment}");
    assert_eq!(
        comment.matches("no pi-spec v1 block").count(),
        1,
        "{comment}"
    );
    let park = r.actions.iter().find_map(|a| match a {
        Action::Park(p) => Some(p.args.clone()),
        _ => None,
    });
    assert!(park
        .unwrap()
        .windows(2)
        .any(|w| w == ["--reason", "not-ready"]));
}

#[test]
fn lean_s_own_comment_on_the_issue_is_not_repeated() {
    let forge = doc("draft").replace("\"channel\": \"host\"", "\"channel\": \"forge\"");
    assert!(forge.contains("\"channel\": \"forge\""));
    let r = read("forge", &forge, 10, pr("OPEN", true), "lean");
    let (_, comment) = said(&r);
    assert!(!comment.contains("The draft pull request"), "{comment}");
    assert!(
        comment.contains("Lean posted its report on this issue itself"),
        "{comment}"
    );
}

#[test]
fn a_classic_cycle_names_no_driver() {
    let r = ended("green", 10, pr("OPEN", false), "classic");
    let (_, comment) = said(&r);
    assert!(!comment.contains("driver"), "{comment}");
}

#[test]
fn a_lean_runner_that_died_is_looked_up_on_lean_s_branch() {
    let r = read("died", "", 137, pr("OPEN", false), "lean");
    assert!(
        r.asked.contains(&"prs lean/issue-7".to_string()),
        "{:?}",
        r.asked
    );
    let (_, comment) = said(&r);
    assert!(comment.contains("with pull request #41 open"), "{comment}");
}
