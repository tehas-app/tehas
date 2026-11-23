//! tehas answer (#2782): the answer a program gives for a person to the
//! question on record of an issue, recorded on the state store, what it
//! refuses, and its contract. api/answer.schema.json says what the object
//! holds; api/answer.sample.json (a retry of a stop) and api/answer.option.json
//! (an option of a planning question, given in place of an answer before it)
//! are what the command prints. tehas question: api/question.schema.json,
//! with api/question.sample.json (a stop) and api/question.plan.json (a
//! planning question). tehas question with no issue: api/questions.schema.json,
//! with api/questions.sample.json (every question that waits) and
//! api/questions.after.json (every question after a number). None reads
//! anything of GitHub. What the poll does
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

/// The issue parked after one failed cycle, no question asked yet.
fn parked(w: &World, issue: u64) {
    let (body, n) = (sha("Words."), issue.to_string());
    w.store_ok(&[
        "approval-record",
        &n,
        "--at",
        "1760000000",
        "--by",
        "host",
        "--how",
        "host",
        "--body-sha",
        &body,
    ]);
    assert_eq!(w.store_locked(issue, &["cycle-start", &n]).rc, 0);
    let end = ["cycle-end", &n, "--attempt", "1", "--status", "failed"];
    assert_eq!(w.store_locked(issue, &end).rc, 0);
    w.store_ok(&["park", &n, "--attempt", "1", "--reason", "failed"]);
}

/// #12 stopped, its question 4001 on record.
fn stopped(w: &World) {
    parked(w, 12);
    w.store_ok(&["question-asked", "12", "--id", "4001", "--kind", "park"]);
}

/// tehas question with these words, the console run as itself at the
/// samples' clock.
fn question_at_now(w: &World, words: &str) -> Run {
    w.sh(&format!(
        "TEHAS_REPO=acme/widget TEHAS_STORE_TEST_NOW={NOW} '{}' tehas question {words}",
        w.console().display()
    ))
}

/// The sample `name` of `surface` is what the command printed, or is
/// written again.
fn sample_of(surface: &str, name: &str, printed: &str) {
    let file = root().join("api").join(format!("{surface}.{name}.json"));
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
    let asked = ["question-asked", "12", "--id", "4002", "--kind", "plan"];
    assert_eq!(w.store_at(NOW.parse().unwrap(), &asked).rc, 0);
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

#[test]
fn the_question_samples_are_what_the_command_prints() {
    let w = World::new("question-samples");
    stopped(&w);
    // The stop of a cycle whose image gives the comments offers all three.
    w.store_ok(&["question-answered", "12", "--id", "4001", "--withdraw"]);
    let asked = [
        "question-asked",
        "12",
        "--id",
        "4003",
        "--kind",
        "park",
        "--offers",
        "retry,retry-text,park",
    ];
    assert_eq!(w.store_at(NOW.parse().unwrap(), &asked).rc, 0);
    let r = question_at_now(&w, "12 --json");
    assert_eq!(r.rc, 0, "{}", r.out);
    sample_of("question", "sample", &r.out);
    w.reset();
    planning(&w);
    let r = question_at_now(&w, "12 --json");
    assert_eq!(r.rc, 0, "{}", r.out);
    sample_of("question", "plan", &r.out);
}

/// A verb of the store at `after` seconds past the samples' clock.
fn store_past(w: &World, after: i64, args: &[&str]) {
    let r = w.store_at(NOW.parse::<i64>().unwrap() + after, args);
    assert_eq!(r.rc, 0, "store {args:?}: {}", r.out);
}

/// tehas question with these words, as one object.
fn questions_at_now(w: &World, words: &str) -> Run {
    question_at_now(w, &format!("{words} --json"))
}

/// The numbers of the questions a list printed, in its order.
fn numbers(printed: &str) -> Vec<u64> {
    let v: serde_json::Value = serde_json::from_str(printed).unwrap();
    let all = v["questions"].as_array().unwrap().iter();
    all.map(|q| q["id"].as_u64().unwrap()).collect()
}

/// With no issue the command lists the question each issue waits on, and with
/// --after every question after a number, the withdrawn and the answered among
/// them, in number order (docs/specs/2026-11-22-events.md, T11.6).
#[test]
fn the_questions_samples_are_the_waiting_ones_and_every_one_after_a_number() {
    let w = World::new("questions-samples");
    let r = questions_at_now(&w, "");
    assert_eq!((r.rc, r.out.trim()), (0, r#"{"api":1,"questions":[]}"#));
    // 1: a stop of #12, withdrawn. It is listed only after a number.
    parked(&w, 12);
    let asked = ["question-asked", "12", "--id", "4001", "--kind", "park"];
    store_past(
        &w,
        0,
        &[&asked[..], &["--offers", "retry-text,park"]].concat(),
    );
    assert_eq!(numbers(&questions_at_now(&w, "").out), [1]);
    let withdraw = ["question-answered", "12", "--id", "4001", "--withdraw"];
    store_past(&w, 30, &withdraw);
    assert_eq!(numbers(&questions_at_now(&w, "").out), [0u64; 0]);
    // 2: a planning question of #12, answered. 3: the next turn's, waiting.
    planning(&w);
    store_past(
        &w,
        60,
        &["plan-answer", "12", "--turn", "2", "--by", "alice"],
    );
    let mut end = vec!["plan-end", "12", "--turn", "2", "--outcome", "finished"];
    end.extend(["--rc", "0", "--session", "abc-123", "--option", "Ship it"]);
    store_past(&w, 90, &end);
    let asked = ["question-asked", "12", "--id", "4003", "--kind", "plan"];
    store_past(&w, 120, &asked);
    // 4: a stop of #13, waiting.
    parked(&w, 13);
    let asked = ["question-asked", "13", "--id", "4004", "--kind", "park"];
    store_past(&w, 180, &asked);

    let waiting = questions_at_now(&w, "");
    assert_eq!(waiting.rc, 0, "{}", waiting.out);
    assert_eq!(numbers(&waiting.out), [3, 4]);
    sample_of("questions", "sample", &waiting.out);
    // Each is the question tehas question <issue> prints for its issue.
    let listed: serde_json::Value = serde_json::from_str(&waiting.out).unwrap();
    for q in listed["questions"].as_array().unwrap() {
        let r = question_at_now(&w, &format!("{} --json", q["issue"]));
        let one: serde_json::Value = serde_json::from_str(&r.out).unwrap();
        for field in ["id", "kind", "comment", "askedAt", "choices"] {
            assert_eq!(one["question"][field], q[field], "{field} of {q}");
        }
    }

    let all = questions_at_now(&w, "--after 0");
    assert_eq!(all.rc, 0, "{}", all.out);
    assert_eq!(numbers(&all.out), [1, 2, 3, 4]);
    sample_of("questions", "after", &all.out);
    let after = questions_at_now(&w, "--after 1");
    assert_eq!(numbers(&after.out), [2, 3, 4]);
    let v: serde_json::Value = serde_json::from_str(&after.out).unwrap();
    assert_eq!(v["questions"][0]["answeredAt"], "2026-10-17T14:01:20Z");
    assert_eq!(numbers(&questions_at_now(&w, "--after 4").out), [0u64; 0]);

    // The same lists for a person, one line a question.
    let r = question_at_now(&w, "--after 1");
    assert_eq!(
        r.out.lines().next(),
        Some(
            "question 2: #12 plan, asked at 2026-10-17T14:00:20Z, answered at 2026-10-17T14:01:20Z"
        ),
        "{}",
        r.out
    );
    assert_eq!(question_at_now(&w, "").out.lines().count(), 2);
    assert_eq!(
        question_at_now(&w, "--after 4").out.trim(),
        "no question after 4"
    );
}

#[test]
fn the_choices_listed_are_those_offered_and_each_is_an_answer() {
    let w = World::new("question-choices");
    stopped(&w);
    w.store_ok(&["question-answered", "12", "--id", "4001", "--withdraw"]);
    w.store_ok(&[
        "question-asked",
        "12",
        "--id",
        "4003",
        "--kind",
        "park",
        "--offers",
        "retry-text,park",
    ]);
    let r = question_at_now(&w, "12 --json");
    let v: serde_json::Value = serde_json::from_str(&r.out).unwrap();
    let words: Vec<&str> = v["question"]["choices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["choice"].as_str().unwrap())
        .collect();
    assert_eq!(words, ["retry-text", "park"]);
    for word in words {
        let r = answer(&w, &format!("12 {word} --by alice"));
        assert_eq!(r.rc, 0, "{word}: {}", r.out);
    }
    w.reset();
    planning(&w);
    let r = question_at_now(&w, "12 --json");
    let v: serde_json::Value = serde_json::from_str(&r.out).unwrap();
    let words: Vec<String> = v["question"]["choices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["choice"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(words, ["option:1", "update", "end"]);
    for word in &words {
        let r = answer(&w, &format!("12 {word} --by alice"));
        assert_eq!(r.rc, 0, "{word}: {}", r.out);
    }
}

#[test]
fn no_question_waiting_is_3_and_a_bad_argument_64() {
    let w = World::new("question-none");
    let r = w.sh("\"$T_ROOT/bin/tehas\" question 12 2>&1");
    assert_eq!(r.rc, 3, "{}", r.out);
    assert!(r.out.contains("no question waits on #12"), "{}", r.out);
    let r = w.sh("\"$T_ROOT/bin/tehas\" question 2>&1");
    assert_eq!((r.rc, r.out.trim()), (0, "no question waits"));
    for words in ["0", "12 13", "12 --soon", "--after", "12 --after 1"] {
        assert_eq!(
            w.sh(&format!("\"$T_ROOT/bin/tehas\" question {words}")).rc,
            64,
            "{words}"
        );
    }
    stopped(&w);
    let r = w.sh("\"$T_ROOT/bin/tehas\" question 12");
    assert_eq!(r.rc, 0, "{}", r.out);
    assert!(
        r.out
            .starts_with("#12 waits on its park question, asked at "),
        "{}",
        r.out
    );
}
