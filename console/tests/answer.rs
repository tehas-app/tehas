//! tehas answer (#2782): the answer a program gives for a person to the
//! question on record of an issue, recorded on the state store, what it
//! refuses, and its contract. api/answer.schema.json says what the object
//! holds; api/answer.sample.json (a retry of a stop) and api/answer.option.json
//! (an option of a planning question, given in place of an answer before it)
//! are what the command prints. It reads nothing of GitHub. What the poll does
//! with an answer is the dispatcher's: console/dispatch/tests/dispatch_answer.rs.
//!
//! To write the samples again after a change to the object:
//!   TEHAS_SAMPLES_WRITE=1 cargo test --locked -p console --test answer

mod common;

use common::*;
use std::fs;

/// 2026-10-17T14:00:20Z, the clock of the samples.
const NOW: &str = "1792245620";

fn sha(text: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// #12 stopped, its question 4001 on record.
fn stopped(w: &World) {
    let body = sha("Words.");
    w.store_ok(&[
        "approval-record",
        "12",
        "--at",
        "1760000000",
        "--by",
        "host",
        "--how",
        "host",
        "--body-sha",
        &body,
    ]);
    assert_eq!(w.store_locked(12, &["cycle-start", "12"]).rc, 0);
    let end = ["cycle-end", "12", "--attempt", "1", "--status", "failed"];
    assert_eq!(w.store_locked(12, &end).rc, 0);
    w.store_ok(&["park", "12", "--attempt", "1", "--reason", "failed"]);
    w.store_ok(&["question-asked", "12", "--id", "4001", "--kind", "park"]);
}

/// The label that starts a planning session, as host/labels.tsv names it.
fn plan_label() -> String {
    let table = fs::read_to_string(root().join("host/labels.tsv")).unwrap();
    table
        .lines()
        .find(|l| l.contains("start a planning session"))
        .and_then(|l| l.split('\t').next())
        .unwrap()
        .to_string()
}

/// #12 in planning after one turn, its question 4002 waiting with two options.
fn planning(w: &World) {
    let brief = w.t().join("brief.md");
    fs::write(&brief, "# Faster search\n").unwrap();
    let label = plan_label();
    w.store_ok(&[
        "plan-begin",
        "12",
        "--by",
        "alice",
        "--label",
        &label,
        "--brief-file",
        brief.to_str().unwrap(),
    ]);
    w.store_ok(&[
        "plan-end",
        "12",
        "--turn",
        "1",
        "--outcome",
        "finished",
        "--rc",
        "0",
        "--session",
        "abc-123",
        "--option",
        "Adopt all",
        "--option",
        "Update this issue",
    ]);
    w.store_ok(&["question-asked", "12", "--id", "4002", "--kind", "plan"]);
}

/// tehas answer with these words, as a person runs it: bin/tehas, under the
/// instance.
fn answer(w: &World, words: &str) -> Run {
    w.sh(&format!(
        "cd \"$T\" && \"$T_ROOT/bin/tehas\" answer {words} 2>&1"
    ))
}

/// The console run as itself at the samples' clock, which bin/tehas's load of
/// the instance would drop.
fn at_now(w: &World, words: &str) -> Run {
    w.sh(&format!(
        "TEHAS_REPO=acme/widget TEHAS_ALLOWED_LABELERS=alice TEHAS_STORE_TEST_NOW={NOW} '{}' tehas answer {words}",
        w.console().display()
    ))
}

/// The choice, option and account of the answer on record of question `id`.
fn waiting(w: &World, id: &str) -> String {
    let a = &w.store_ok(&["question-known", "12", "--id", id])["answer"];
    if a.is_null() {
        return "none".into();
    }
    format!(
        "{} {} {}",
        a["choice"].as_str().unwrap(),
        a["option"],
        a["by"].as_str().unwrap()
    )
}

/// The sample `name` is what the command printed, or is written again.
fn sample(name: &str, printed: &str) {
    let file = root().join("api").join(format!("answer.{name}.json"));
    let v: serde_json::Value = serde_json::from_str(printed).unwrap();
    let pretty = format!("{}\n", serde_json::to_string_pretty(&v).unwrap());
    if std::env::var_os("TEHAS_SAMPLES_WRITE").is_some() {
        fs::write(&file, &pretty).unwrap();
    }
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        pretty,
        "{}",
        file.display()
    );
}

#[test]
fn a_retry_of_a_stop_is_recorded_for_the_account_and_nothing_starts() {
    let w = World::new("answer-retry");
    stopped(&w);
    let r = answer(&w, "12 retry --by alice");
    assert_eq!(r.rc, 0, "{}", r.out);
    let line = r.out.trim_end();
    assert!(
        line.starts_with("answered the park question of #12 for alice at "),
        "{line}"
    );
    assert!(
        line.ends_with("Z: retry; the next poll acts on it"),
        "{line}"
    );
    assert_eq!(waiting(&w, "4001"), "retry null alice");
}

#[test]
fn the_samples_are_what_the_command_prints() {
    let w = World::new("answer-samples");
    stopped(&w);
    let r = at_now(&w, "12 retry --by alice --json");
    assert_eq!(r.rc, 0, "{}", r.out);
    sample("sample", &r.out);
    w.reset();
    planning(&w);
    assert_eq!(at_now(&w, "12 end --by alice").rc, 0);
    let r = at_now(&w, "12 option:2 --by alice --json");
    assert_eq!(r.rc, 0, "{}", r.out);
    sample("option", &r.out);
}

#[test]
fn a_planning_question_takes_an_option_update_end_or_words() {
    let w = World::new("answer-plan");
    planning(&w);
    let r = answer(&w, "12 option:1 --by alice");
    assert_eq!(r.rc, 0, "{}", r.out);
    assert!(r.out.contains(": option 1;"), "{}", r.out);
    assert_eq!(waiting(&w, "4002"), "option 1 alice");
    answer(&w, "12 update --by alice");
    assert_eq!(waiting(&w, "4002"), "update 2 alice");
    answer(&w, "12 end --by alice");
    assert_eq!(waiting(&w, "4002"), "end null alice");
    // A file named as the caller names it, from where it runs.
    fs::write(w.t().join("reply.md"), "Split it in two.\n").unwrap();
    let r = answer(&w, "12 --text reply.md --by alice");
    assert_eq!(r.rc, 0, "{}", r.out);
    let known = w.store_ok(&["question-known", "12", "--id", "4002"]);
    assert_eq!(known["answer"]["text"], "Split it in two.\n");
}

#[test]
fn a_reply_in_words_can_come_on_standard_input_and_leaves_no_file() {
    let w = World::new("answer-stdin");
    planning(&w);
    let r = w.sh(
        "printf 'Split it in two.\\n' | \"$T_ROOT/bin/tehas\" answer 12 --text - --by alice 2>&1",
    );
    assert_eq!(r.rc, 0, "{}", r.out);
    let known = w.store_ok(&["question-known", "12", "--id", "4002"]);
    assert_eq!(known["answer"]["choice"], "text");
    assert_eq!(known["answer"]["text"], "Split it in two.\n");
    let left: Vec<_> = fs::read_dir(w.t().join("tmp"))
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("tehas-answer."))
        .collect();
    assert!(left.is_empty(), "{left:?}");
    // Empty input, or a choice beside it, is a bad argument and records nothing.
    w.reset();
    planning(&w);
    for snippet in [
        "printf '  \\n' | \"$T_ROOT/bin/tehas\" answer 12 --text - --by alice",
        "printf 'x' | \"$T_ROOT/bin/tehas\" answer 12 end --text - --by alice",
    ] {
        assert_eq!(w.sh(snippet).rc, 64, "{snippet}");
    }
    assert_eq!(waiting(&w, "4002"), "none");
}

#[test]
fn an_account_that_may_not_authorise_work_is_refused_with_78() {
    let w = World::new("answer-stranger");
    stopped(&w);
    let r = answer(&w, "12 retry --by mallory");
    assert_eq!(r.rc, 78, "{}", r.out);
    assert!(
        r.out
            .contains("mallory may not authorise work for this factory"),
        "{}",
        r.out
    );
    assert_eq!(waiting(&w, "4001"), "none");
}

#[test]
fn no_question_of_that_kind_or_a_choice_not_offered_is_3() {
    let w = World::new("answer-unasked");
    let r = answer(&w, "12 retry --by alice");
    assert_eq!(r.rc, 3, "{}", r.out);
    assert!(r.out.contains("no stop waits for an answer"), "{}", r.out);
    stopped(&w);
    let r = answer(&w, "12 end --by alice");
    assert_eq!(r.rc, 3, "{}", r.out);
    assert!(r.out.contains("no planning question waits"), "{}", r.out);
    assert_eq!(waiting(&w, "4001"), "none");
    w.reset();
    planning(&w);
    let r = answer(&w, "12 option:3 --by alice");
    assert_eq!(r.rc, 3, "{}", r.out);
    assert!(
        r.out.contains("the question offers 2 options, not 3"),
        "{}",
        r.out
    );
    assert_eq!(waiting(&w, "4002"), "none");
}

#[test]
fn a_bad_argument_writes_nothing_and_exits_64() {
    let w = World::new("answer-usage");
    stopped(&w);
    for words in [
        "",
        "12",
        "12 retry",
        "0 retry --by alice",
        "12 tick --by alice",
        "12 option:0 --by alice",
        "12 retry --text f --by alice",
        "12 retry --by 'al ice'",
        "12 retry end --by alice",
    ] {
        assert_eq!(answer(&w, words).rc, 64, "{words}");
    }
    assert_eq!(waiting(&w, "4001"), "none");
}
