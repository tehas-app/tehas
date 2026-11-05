//! What the facts hold of an ended cycle's files, and what the report decides
//! from it. For a runner that wrote no result and left the bot's pull request
//! open on its branch: `died-with-pr` when its run log was read whole, the park
//! with its retry boxes when only its end was read. A protocol file is read
//! whole however big it is: the result of a run with long handoff bodies is
//! held and reported, not read as a file that is not there. The facts are those
//! of the crash case of the same shape.

use serde_json::{json, Value};
use std::fs;
use tehas_cycle::report::{report, Action, Given, Park};
use tehas_cycle::runner::LOG_TAIL;
use tehas_cycle::Facts;

const CASE: &str = "a runner that died after commit-pr is reported with its pull request, not as a park with a clean retry";

/// A case of the report's fixtures, by its name.
fn fixture(path: &str, name: &str) -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/").to_string() + path;
    let cases: Vec<Value> = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    cases
        .into_iter()
        .find(|c| c["case"] == name)
        .unwrap_or_else(|| panic!("no fixture case {name}"))
}

/// The crash case of a runner that died after its push, with its own branch.
fn case() -> Value {
    fixture("crash.json", CASE)
}

/// At least `n` bytes of what a runner writes between its steps: one line,
/// so the steps after it are still whole lines.
fn written(n: usize) -> String {
    "y".repeat(n) + "\n"
}

/// The arm of the report of the case when its run log is `log`, and its park
/// when it has one.
fn report_of(log: Value) -> (&'static str, Option<Park>) {
    let mut c = case();
    let github = c["facts"]["github"].clone();
    c["facts"]["run_log"] = log;
    let facts = Facts::from_json(&c["facts"]).unwrap();
    let r = report(&facts, &mut Given(github));
    let park = r.actions.iter().find_map(|a| match a {
        Action::Park(p) => Some((**p).clone()),
        _ => None,
    });
    (r.arm, park)
}

#[test]
fn a_run_log_read_whole_is_a_cycle_that_died_with_its_pull_request_open() {
    let log = case()["facts"]["run_log"].clone();
    assert!(log["text"].as_str().unwrap().len() <= LOG_TAIL as usize);
    let (arm, park) = report_of(log);
    assert_eq!(arm, "died-with-pr");
    assert!(park.is_none(), "a clean handoff is not parked: {park:?}");
}

#[test]
fn a_run_log_past_the_bound_tehas_reads_is_parked_though_its_branch_has_an_open_pull_request() {
    let c = case();
    let text = c["facts"]["run_log"]["text"].as_str().unwrap();
    // What the host holds of a log longer than the bound: its last LOG_TAIL
    // bytes, and the word that says the rest was left out.
    let whole = written(LOG_TAIL as usize) + text;
    let held = &whole[whole.len() - LOG_TAIL as usize..];
    let (arm, park) = report_of(json!({
        "name": "logs/work-7-261101-093921.jsonl",
        "text": held,
        "truncated": true,
    }));
    assert_eq!(arm, "parked");
    let park = park.expect("a log read from its end says no clean handoff, so it is parked");
    // The open pull request is still named, and the retry boxes come with it.
    assert!(park.text.contains("pull/347"), "{}", park.text);
    let (_, body) = park.question(7, false);
    assert!(body.contains("Retry with the issue text only"), "{body}");
}

#[test]
fn a_result_past_the_bound_of_a_run_log_is_read_and_still_carries_its_handoff() {
    // The handoff park of a run that stopped on a cap, with its body long
    // enough that the file passes what tehas reads a run log at. It is the
    // cycle's own account of its end: the report reads it and posts the
    // handoff, as it does for the same result when it is small.
    let mut c = fixture(
        "handoff.json",
        "a park carries pi-rukas's handoff under its marker, cleaned like any agent's words",
    );
    let github = c["facts"]["github"].clone();
    let mut result: Value =
        serde_json::from_str(c["facts"]["files"]["result.json"].as_str().unwrap()).unwrap();
    let body = format!(
        "<!-- pi-rukas:handoff issue=7 cap=verify-failed:develop -->\n## Cap hit\n\n{}\nask @someone",
        "u".repeat(1 << 20)
    );
    result["issues"][0]["handoff"]["body"] = json!(body);
    let text = result.to_string();
    assert!(
        text.len() > (1 << 20) && text.len() > LOG_TAIL as usize,
        "{}",
        text.len()
    );
    c["facts"]["files"]["result.json"] = json!(text);
    let facts = Facts::from_json(&c["facts"]).unwrap();
    let r = report(&facts, &mut Given(github));
    // Nothing was left out of the file, so nothing is said of a result that
    // holds nothing, and the step is the result's own.
    assert!(
        !r.log.iter().any(|l| l.contains("holds nothing")),
        "{:?}",
        r.log
    );
    let park = r
        .actions
        .iter()
        .find_map(|a| match a {
            Action::Park(p) => Some((**p).clone()),
            _ => None,
        })
        .expect("a handoff on a cap is parked");
    assert!(park.handoff.contains("## Cap hit"), "{}", park.handoff);
    assert!(
        park.handoff.contains(&"u".repeat(29_000)),
        "the body is not the one the result holds"
    );
    // What is left out of a long handoff is the 30 000 characters a report
    // takes of it for a comment, not bytes of a file tehas refused to read.
    assert!(park.handoff.len() <= 30_000, "{}", park.handoff.len());
    let (_, body) = park.question(7, false);
    assert!(body.contains("## Cap hit"), "{body}");
}

/// The facts of the case with the image's word on comments, the approval of a
/// comments retry used by this start, and the runner's result.
fn comments_facts(mode: Option<&str>, how: &str, result: Option<Value>) -> Facts {
    let mut c = case();
    let attempt = c["facts"]["start"]["attempt"].clone();
    if let Some(m) = mode {
        c["facts"]["files"]["comments"] = json!(format!("{m}\n"));
    }
    c["facts"]["approval"] = json!({"current": {"used_by": attempt, "how": how,
        "input": {"retry_context": [{"source": "issue-comment", "id": 1}]}}});
    if let Some(r) = result {
        c["facts"]["files"]["result.json"] = json!(r.to_string());
    }
    Facts::from_json(&c["facts"]).unwrap()
}

#[test]
fn a_park_offers_the_comments_retry_only_when_the_image_gives_the_comments() {
    use tehas_cycle::report::comments_box;
    for (mode, offered) in [
        (Some("native"), true),
        (Some("polyfill"), true),
        (Some("off"), false),
        (None, false),
    ] {
        let f = comments_facts(mode, "retry-tick", None);
        assert_eq!(comments_box(&f), offered, "{mode:?}");
        let park = Park {
            comments: comments_box(&f),
            ..Park::default()
        };
        let (_, body) = park.question(7, false);
        assert_eq!(
            body.contains("- [ ] Retry with the issue and all its comments"),
            offered,
            "{body}"
        );
        assert!(body.contains("- [ ] Retry with the issue text only"));
    }
}

#[test]
fn the_end_comment_says_how_many_comments_the_runner_read() {
    use tehas_cycle::report::comments_said;
    let read = json!({"schemaVersion": 5, "comments": {"read": true, "count": 3, "bytes": 900, "dropped": 1}});
    assert_eq!(
        comments_said(&comments_facts(Some("native"), "retry-tick", Some(read))),
        " The cycle was given 3 comments of the issue and its pull requests, 1 older ones left out for room."
    );
    let unread =
        json!({"schemaVersion": 5, "comments": {"read": false, "reason": "sha256 mismatch"}});
    assert_eq!(
        comments_said(&comments_facts(Some("native"), "retry-tick", Some(unread))),
        " The cycle's runner did not read the comments it was given: sha256 mismatch."
    );
    assert_eq!(
        comments_said(&comments_facts(Some("polyfill"), "retry-tick", None)),
        " The comments of the issue and its pull requests were given to the cycle's agents."
    );
    assert!(
        comments_said(&comments_facts(Some("off"), "retry-tick", None))
            .contains("does not read comments")
    );
    // Only a comments retry says anything.
    assert_eq!(
        comments_said(&comments_facts(Some("native"), "label", None)),
        ""
    );
}
