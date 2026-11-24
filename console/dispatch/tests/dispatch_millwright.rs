//! The millwright's labels and its planning answers, through a real poll: the
//! approval records the store keeps, the answer a turn is given, the notes on
//! the issue. The decisions are the fixtures of fixer.json.

mod common;

use common::*;
use serde_json::{json, Value};
use std::fs;
use tehas_core::home::{Home, State};

// --- The millwright ---------------------------------------------------------------

/// Issue 7 is labelled by the millwright, its body is "Words.", and alice approved
/// its last cycle on that body, then the millwright started `starts` cycles; no
/// approval at all for None.
fn millwright_ready(w: &World, starts: Option<u32>) {
    w.reset();
    w.slots(1, &[]);
    w.fx(
        "ready.json",
        r#"[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"needs-human-attention"},{"name":"tehas:question"},{"name":"tehas:millwright-escalated"}]}]"#,
    );
    ready_by(w, 7, "tehas-bot");
    body_of(w, 7, "Words.");
    w.fx("icomments-7.json", "[]");
    fs::create_dir_all(w.cycles_dir()).unwrap();
    let Some(k) = starts else { return };
    w.past(7, "alice", "approved", "2026-10-01T00:00:00Z", &[]);
    for i in 1..=k {
        w.past(
            7,
            "tehas-bot",
            "millwright",
            &format!("2026-10-01T0{i}:00:00Z"),
            &[],
        );
    }
}

fn millwright(starts: Option<u32>) -> World {
    let w = World::new("millwright");
    millwright_ready(&w, starts);
    w
}

/// tehas:ready on issue `n` by `who`, at 2026-10-02.
fn ready_by(w: &World, n: u64, who: &str) {
    w.fx_json(
        &format!("events-{n}.json"),
        &json!([{"event": "labeled", "label": {"name": "tehas:ready"}, "actor": {"login": who},
            "created_at": "2026-10-02T00:00:00Z"}]),
    );
}

fn body_of(w: &World, n: u64, body: &str) {
    w.fx_json(
        &format!("edit-{n}.json"),
        &json!({"data": {"repository": {"issue": {"lastEditedAt": null, "editor": null, "body": body}}}}),
    );
}

/// The millwright is the app unknown[bot], whose id the host knows.
fn app_unknown(w: &World) {
    let ids = Home::new(&w.home).state(State::AccountIds);
    let mut text = fs::read_to_string(&ids).unwrap();
    text.push_str(&format!("unknown {}\n", uid("unknown")));
    fs::write(&ids, text).unwrap();
    let env = w.t().join("instance/instance.env");
    let mut text = fs::read_to_string(&env).unwrap();
    text.push_str("TEHAS_MILLWRIGHT_APP=unknown[bot]\n");
    fs::write(&env, text).unwrap();
}

/// No grant in 2.0: the millwright's tehas:ready is any outsider's, on an
/// issue a person approved before too. A program starts work for a person
/// only through tehas start.
#[test]
fn the_millwright_s_tehas_ready_on_an_issue_alice_approved_starts_nothing() {
    let w = millwright(Some(0));
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.calls("fake-start"), "");
    has(
        &r.out,
        "clear #7: tehas:ready off, applied by 'tehas-bot' who may not authorise work here",
    );
    lacks(&r.out, "labelled by the millwright");
}

// The millwright's app may be called unknown. A tehas:ready by no account,
// which the log names unknown, is still nobody's and is not the millwright's.
#[test]
fn with_the_millwright_app_called_unknown_a_tehas_ready_by_no_account_starts_nothing() {
    let w = millwright(Some(0));
    app_unknown(&w);
    w.fx(
        "events-7.json",
        r#"[{"event":"labeled","label":{"name":"tehas:ready"},"actor":null,"created_at":"2026-10-02T00:00:00Z"}]"#,
    );
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.calls("fake-start"), "");
    has(
        &r.out,
        "skip #7: tehas:ready was applied by 'unknown', not an allowed account",
    );
    has(
        &r.out,
        "clear #7: tehas:ready off, applied by 'unknown' who may not authorise work here",
    );
    lacks(&r.out, "labelled by the millwright");
}

#[test]
fn an_account_with_a_name_like_the_app_s_is_an_outsider() {
    let w = World::new("millwright");
    for who in ["tehas-bot2[bot]", "tehas-bot2", "tehas-bo[bot]"] {
        millwright_ready(&w, Some(0));
        ready_by(&w, 7, who);
        let r = w.poll();
        assert_eq!(w.calls("fake-start"), "", "{who}");
        has(
            &r.out,
            &format!(
                "clear #7: tehas:ready off, applied by '{who}' who may not authorise work here"
            ),
        );
    }
}

// --- The millwright's planning answers ---------------------------------------

const FMARK: &str = "<!-- tehas:millwright issue=7 action=answer -->";

/// Issue 7 has a planning question open in a session `by` started, with as
/// many turns as the last of `answers` on record names.
fn millwright_planning(w: &World, answers: Value, by: &str) {
    w.planning();
    w.slots(1, &[]);
    let turns = answers
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| a["turn"].as_u64())
        .chain([1])
        .max()
        .unwrap();
    w.t_plan(
        7,
        json!({"by": by, "turns": turns, "options": ["Adopt all", "Change some"],
            "comment": 4001, "answers": answers}),
    );
}

fn planning_world(answers: Value) -> World {
    let w = World::new("millwright");
    millwright_planning(&w, answers, "alice");
    w
}

/// One comment by the millwright, as the comments after the question.
fn freply(body: &str, id: u64) -> Value {
    json!([{"databaseId": id, "author": {"login": "tehas-bot"}, "body": body,
        "createdAt": "2026-01-01T02:00:00Z"}])
}

fn marked(text: &str) -> String {
    format!("{FMARK}\n{text}")
}

fn unticked() -> String {
    plan_body(" ", " ", " ", 1, "")
}

fn answer(w: &World) -> String {
    fs::read_to_string(w.keep().join("answer.md"))
        .unwrap_or_default()
        .trim_end_matches('\n')
        .to_string()
}

fn last_answer(w: &World) -> Value {
    w.rec()["answers"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone()
}

/// The question, comment 4001, as GitHub gives it, changed by `f`.
fn edit_question(w: &World, f: impl FnOnce(&mut Value)) {
    let mut v: Value =
        serde_json::from_str(&fs::read_to_string(w.fx.join("comments-7.json")).unwrap()).unwrap();
    f(&mut v["data"]["repository"]["issue"]["comments"]["nodes"][0]);
    w.fx_json("comments-7.json", &v);
}

/// alice saved the question, then the millwright did.
fn saved_after_alice(q: &mut Value) {
    q["userContentEdits"] = json!({"totalCount": 2, "nodes": [{"editor": {"login": "alice"}},
        {"editor": {"login": "tehas-bot"}}]});
}

fn comments_7(w: &World) -> usize {
    count_lines(&w.gh(), "issue comment 7 ")
}

/// The millwright's marked reply that names an option chooses it, as a
/// person's reply does; one that names the end runs no turn, which only a
/// person may.
#[test]
fn a_marked_reply_of_the_millwright_s_that_names_an_option_chooses_it() {
    for (said, chose) in [
        ("1", Some("Adopt all")),
        ("Change some", Some("Change some")),
        ("end", None),
    ] {
        let w = planning_world(json!([{"turn": 2, "by": "tehas-bot", "millwright": true},
            {"turn": 3, "by": "alice", "millwright": false}]));
        w.comments(7, &unticked(), "", freply(&marked(said), 4003));
        let r = w.poll();
        if let Some(option) = chose {
            assert_eq!(
                answer(&w),
                format!("The millwright chose: {option}"),
                "{said}"
            );
            has(
                &r.out,
                "planning #7 continues (answer from tehas-bot, the millwright)",
            );
        } else {
            has(&r.out, "the millwright's answer runs no turn");
            assert_eq!(w.plan_calls(), "", "{said}");
        }
    }
}

#[test]
fn a_marked_reply_of_the_millwright_s_runs_the_turn_and_the_record_says_the_millwright_answered() {
    let w = planning_world(json!([{"turn": 2, "by": "tehas-bot", "millwright": true},
        {"turn": 3, "by": "alice", "millwright": false}]));
    w.comments(
        7,
        &unticked(),
        "",
        freply(&marked("Adopt all, as alice chose in #12."), 4003),
    );
    let r = w.poll();
    has(
        &w.plan_calls(),
        &format!("--answer {}/answer.md --comment 4003", w.keep().display()),
    );
    assert_eq!(
        answer(&w),
        "The millwright replied on the issue:\n\nAdopt all, as alice chose in #12."
    );
    has(
        &r.out,
        "planning #7 continues (answer from tehas-bot, the millwright)",
    );
    assert_eq!(
        last_answer(&w),
        json!({"turn": 4, "by": "tehas-bot", "millwright": true})
    );
}

#[test]
fn a_reply_marked_as_the_millwright_s_runs_the_turn_and_the_store_is_told_the_millwright_answered()
{
    let w = planning_world(json!([]));
    w.comments(7, &unticked(), "", freply(&marked("Adopt all."), 4003));
    w.poll();
    has(
        &w.plan_calls(),
        &format!("--answer {}/answer.md --comment 4003", w.keep().display()),
    );
    let p = w.row(&["plan-show", "7"]);
    assert_eq!(
        p["planning"]["answers"].as_array().unwrap().last().unwrap()["millwright"],
        json!(true)
    );
}

#[test]
fn an_app_millwright_set_with_bot_answers_as_graphql_names_it_and_the_store_has_the_one_form() {
    let w = planning_world(json!([]));
    w.slots(1, &[]);
    w.comments(7, &unticked(), "", freply(&marked("Adopt all."), 4003));
    w.poll();
    has(
        &w.plan_calls(),
        &format!("--answer {}/answer.md --comment 4003", w.keep().display()),
    );
    assert_eq!(
        jq(
            ".planning.answers[-1] | [.turn, .by, .millwright]",
            &w.row(&["plan-show", "7"]).to_string()
        ),
        r#"[2,"tehas-bot",true]"#
    );
    assert_eq!(
        last_answer(&w),
        json!({"turn": 2, "by": "tehas-bot", "millwright": true})
    );
}

#[test]
fn a_person_s_reply_wins_over_a_newer_reply_of_the_millwright_s_and_a_person_s_tick_wins_too() {
    let w = planning_world(json!([]));
    let mut after = vec![
        json!({"databaseId": 4003, "author": {"login": "alice"}, "body": "mine",
        "createdAt": "2026-01-01T01:30:00Z"}),
    ];
    after.extend(
        freply(&marked("Adopt all."), 4004)
            .as_array()
            .unwrap()
            .clone(),
    );
    w.comments(7, &unticked(), "", Value::Array(after));
    w.poll();
    assert_eq!(answer(&w), "The operator replied on the issue:\n\nmine");
    assert_eq!(
        w.rec()["answers"],
        json!([{"turn": 2, "by": "alice", "millwright": false}])
    );
    millwright_planning(&w, json!([]), "alice");
    w.comments(
        7,
        &plan_body(" ", "x", " ", 1, ""),
        "alice",
        freply(&marked("Adopt all."), 4003),
    );
    w.poll();
    assert_eq!(answer(&w), "The operator chose: Change some");
}

/// The millwright answered the last question, and replies to this one too.
fn twice() -> World {
    let w = planning_world(json!([{"turn": 2, "by": "tehas-bot", "millwright": true}]));
    w.comments(7, &unticked(), "", freply(&marked("Adopt all."), 4003));
    w
}

#[test]
fn never_two_in_a_row_after_the_millwright_s_answer_its_next_one_runs_no_turn_and_the_issue_says_so_once(
) {
    let w = twice();
    let r = w.poll();
    assert_eq!(w.plan_calls(), "");
    has(&r.out, "skip #7: the millwright's answer runs no turn: the millwright answered the question before this one; a person answers this one");
    assert_eq!(comments_7(&w), 1);
    has(
        &w.gh(),
        "Tehas did not take the millwright's reply as an answer",
    );
    assert_eq!(w.rec()["millwright_refused"], json!([4003]));
    assert_eq!(
        jq(
            ".planning.notes.millwright_refused",
            &w.row(&["plan-show", "7"]).to_string()
        ),
        "[4003]"
    );
    w.clear_calls();
    let r = w.poll();
    assert_eq!(w.plan_calls(), "");
    lacks(&r.out, "the millwright's answer runs no turn");
    lacks(&w.gh(), "issue comment");
}

#[test]
fn and_a_person_s_answer_then_runs_the_turn() {
    let w = twice();
    w.poll();
    let mut after = freply(&marked("Adopt all."), 4003)
        .as_array()
        .unwrap()
        .clone();
    after.push(
        json!({"databaseId": 4005, "author": {"login": "alice"}, "body": "go",
        "createdAt": "2026-01-01T03:00:00Z"}),
    );
    w.comments(7, &unticked(), "", Value::Array(after));
    w.poll();
    has(&w.plan_calls(), "--comment 4005");
}

#[test]
fn a_held_tick_on_a_parked_cycle_s_question_starts_nothing_and_stays() {
    let w = World::new("millwright");
    w.parked("failed", "alice");
    w.slots(1, &[]);
    w.comments(7, &park_body(&["x", " "]), "tehas-bot", json!([]));
    edit_question(&w, saved_after_alice);
    w.poll();
    assert_eq!(w.calls("fake-start"), "");
    lacks(&w.gh(), "api -X PATCH");
}

// The millwright is the bot, so every note the host writes is a comment of the
// millwright's account. None starts with the answer marker, so none is a reply.
#[test]
fn a_host_note_after_a_plan_question_is_no_millwright_reply_two_polls_post_nothing() {
    let w = World::new("millwright");
    for note in [
        "The millwright's tick was not taken as an answer: it was unticked. A person answers this question.",
        "Tehas did not take the box ticked in this question as an answer: the millwright saved the comment after another account did.",
        "The label `tehas:ready` was removed: mallory is not an account that may authorise work.",
    ] {
        millwright_planning(&w, json!([]), "alice");
        w.comments(7, &unticked(), "", freply(note, 4010));
        w.clear_calls();
        w.poll();
        let r = w.poll();
        assert_eq!(w.plan_calls(), "", "{note}");
        lacks(&w.gh(), "issue comment");
        lacks(&r.out, "the millwright's answer runs no turn");
    }
}
