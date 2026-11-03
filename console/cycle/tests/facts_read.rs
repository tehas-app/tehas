//! What the facts hold of an ended cycle's files, and what the report decides
//! from it. For a runner that wrote no result and left the bot's pull request
//! open on its branch: `died-with-pr` when its run log was read whole, the park
//! with its retry boxes when only its end was read. A protocol file too big to
//! hold is not read, and is said of. The facts are those of the crash case of
//! the same shape.

use serde_json::{json, Value};
use std::fs;
use tehas_cycle::report::{report, Action, Given, Park};
use tehas_cycle::runner::LOG_TAIL;
use tehas_cycle::Facts;

const CASE: &str = "a runner that died after commit-pr is reported with its pull request, not as a park with a clean retry";

/// The crash case of a runner that died after its push, with its own branch.
fn case() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/crash.json");
    let cases: Vec<Value> = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    cases.into_iter().find(|c| c["case"] == CASE).unwrap()
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
    assert!(body.contains("Retry from a clean start"), "{body}");
}

#[test]
fn a_protocol_file_past_the_bound_is_said_and_reads_as_one_that_is_not_there() {
    let mut c = case();
    let github = c["facts"]["github"].clone();
    c["facts"]["files"]["result.json"] = json!({"too_big": true});
    let facts = Facts::from_json(&c["facts"]).unwrap();
    let r = report(&facts, &mut Given(github));
    assert!(
        r.log.contains(
            &"WARN: the result.json of #7 is bigger than tehas reads, so it is not read"
                .to_string()
        ),
        "{:?}",
        r.log
    );
    // Nothing is known of a file that is not read: this is the cycle whose
    // runner wrote no result, and its step comes from its run log.
    assert_eq!(r.arm, "died-with-pr");
}
