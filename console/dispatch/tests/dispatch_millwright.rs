//! The millwright's labels and its planning answers, through a real poll: the
//! approval records the store keeps, the answer a turn is given, the notes on
//! the issue. The decisions are the fixtures of fixer.json.

mod common;

use common::*;
use serde_json::{json, Value};
use std::fs;

// --- The fixer ---------------------------------------------------------------

/// Issue 7 is labelled by the fixer, its body is "Words.", and alice approved
/// its last cycle on that body, then the fixer started `starts` cycles; no
/// approval at all for None.
fn fixer_ready(w: &World, starts: Option<u32>) {
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
    w.past(7, "alice", "label", "2026-10-01T00:00:00Z", &[]);
    for i in 1..=k {
        w.past(
            7,
            "tehas-bot",
            "fixer",
            &format!("2026-10-01T0{i}:00:00Z"),
            &[],
        );
    }
}

fn fixer(starts: Option<u32>) -> World {
    let w = World::new("millwright");
    fixer_ready(&w, starts);
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
    let ids = w.home.join("state/account-ids");
    let mut text = fs::read_to_string(&ids).unwrap();
    text.push_str(&format!("unknown {}\n", uid("unknown")));
    fs::write(&ids, text).unwrap();
    let env = w.t().join("instance/instance.env");
    let mut text = fs::read_to_string(&env).unwrap();
    text.push_str("TEHAS_MILLWRIGHT_APP=unknown[bot]\n");
    fs::write(&env, text).unwrap();
}

fn icomments(w: &World, list: Value) {
    w.fx_json("icomments-7.json", &list);
}

const CHANGE: &str = "<!-- tehas:millwright issue=7 action=change -->\n";

#[test]
fn the_fixer_s_tehas_ready_on_an_issue_alice_approved_starts_a_cycle_that_rests_on_her_approval() {
    let w = fixer(Some(0));
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.calls("fake-start").trim_end(), "7");
    assert_eq!(
        w.approval(
            "[.by, .how, .fixer_starts, .rests_on.by, .rests_on.at, .rests_on.how, (.rests_on.body_sha256 == .body_sha256)]",
            7
        ),
        r#"["tehas-bot","fixer",1,"alice","2026-10-01T00:00:00Z","label",true]"#
    );
    has(&r.out, "start #7 (labelled by the fixer by tehas-bot)");
    has(
        &w.gh(),
        "authorized by @tehas-bot's `tehas:ready` label as the fixer, on alice's approval of 2026-10-01T00:00:00Z",
    );
}

#[test]
fn the_store_is_told_the_millwright_s_word() {
    let w = fixer(Some(0));
    w.poll();
    assert_eq!(
        jq(
            "[.current.at, .current.by, .current.how]",
            &w.row(&["approval-current", "7"]).to_string()
        ),
        r#"[1790899200,"tehas-bot","millwright"]"#
    );
}

#[test]
fn a_second_fixer_start_counts_on_from_the_first_and_keeps_the_person_s_approval() {
    let w = fixer(Some(1));
    w.poll();
    assert_eq!(w.calls("fake-start").trim_end(), "7");
    assert_eq!(
        w.approval("[.how, .fixer_starts, .rests_on.by]", 7),
        r#"["fixer",2,"alice"]"#
    );
}

#[test]
fn a_person_s_approval_whose_start_never_ran_is_no_base_for_the_fixer() {
    let w = fixer(None);
    w.past(
        7,
        "alice",
        "label",
        "2026-10-01T00:00:00Z",
        &["--rc", "4", "--never-ran"],
    );
    let r = w.poll();
    assert_eq!(w.calls("fake-start"), "");
    has(&r.out, "no person approved an earlier cycle of it");
}

// The millwright's app may be called unknown. A tehas:ready by no account,
// which the log names unknown, is still nobody's and is not the millwright's.
#[test]
fn with_the_millwright_app_called_unknown_a_tehas_ready_by_no_account_starts_nothing() {
    let w = fixer(Some(0));
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
    lacks(&r.out, "labelled by the fixer");
}

#[test]
fn the_millwright_app_called_unknown_starts_with_its_own_tehas_ready() {
    let w = fixer(Some(0));
    app_unknown(&w);
    ready_by(&w, 7, "unknown[bot]");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.calls("fake-start").trim_end(), "7");
    has(&r.out, "start #7 (labelled by the fixer by unknown[bot])");
}

#[test]
fn a_busy_store_leaves_the_fixer_s_label_for_the_next_poll_no_label_comes_off_nothing_is_said() {
    let w = fixer(Some(0));
    w.store_refuses(
        "approval-current 7",
        1,
        "store: busy #7: the database is locked",
    );
    let r = w.poll();
    assert_eq!(w.calls("fake-start"), "");
    has(
        &r.out,
        "skip #7: tehas:ready from the fixer waits: its approvals on the state store could not be read",
    );
    lacks(&w.gh(), "--remove-label tehas:ready");
    lacks(&w.gh(), "issue comment 7");
}

#[test]
fn the_app_millwright_s_marked_comment_by_its_rest_login_is_the_retry_s_context() {
    let w = fixer(Some(0));
    icomments(
        &w,
        json!([
            icomment(
                81,
                "tehas-bot[bot]",
                "2026-10-01T23:55:00Z",
                "2026-10-01T23:55:00Z",
                &format!("{CHANGE}Run the gate first.")
            ),
            icomment(
                82,
                "tehas-bot2[bot]",
                "2026-10-01T23:58:00Z",
                "2026-10-01T23:58:00Z",
                &format!("{CHANGE}Not the millwright.")
            ),
        ]),
    );
    w.poll();
    assert_eq!(w.calls("fake-start").trim_end(), "7");
    assert_eq!(
        w.approval("[.retry_context[] | [.source, .by]]", 7),
        r#"[["millwright:81","tehas-bot[bot]"]]"#
    );
}

// REST names the app's label actor <slug>[bot]; GraphQL names its author
// <slug>. The one identity is both forms of one account, and no account that
// only resembles it.
#[test]
fn the_app_s_label_event_by_its_rest_login_starts_the_cycle_on_alice_s_approval() {
    let w = fixer(Some(0));
    ready_by(&w, 7, "tehas-bot[bot]");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.calls("fake-start").trim_end(), "7");
    has(&r.out, "start #7 (labelled by the fixer by tehas-bot[bot])");
}

#[test]
fn an_account_with_a_name_like_the_app_s_is_an_outsider() {
    let w = World::new("millwright");
    for who in ["tehas-bot2[bot]", "tehas-bot2", "tehas-bo[bot]"] {
        fixer_ready(&w, Some(0));
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

#[test]
fn the_fixer_s_marked_comment_before_its_label_is_the_retry_s_context_whatever_the_comments_setting(
) {
    let w = fixer(Some(0));
    icomments(
        &w,
        json!([
            icomment(
                81,
                "tehas-bot",
                "2026-10-01T23:55:00Z",
                "2026-10-01T23:55:00Z",
                &format!("{CHANGE}Run the gate first.")
            ),
            icomment(
                80,
                "tehas-bot",
                "2026-10-01T20:00:00Z",
                "2026-10-01T20:00:00Z",
                &format!("{CHANGE}An older decision.")
            ),
            icomment(
                82,
                "mallory",
                "2026-10-01T23:58:00Z",
                "2026-10-01T23:58:00Z",
                &format!("{CHANGE}Not the fixer.")
            ),
            icomment(
                83,
                "tehas-bot",
                "2026-10-01T23:59:00Z",
                "2026-10-02T01:00:00Z",
                &format!("{CHANGE}Edited after the label.")
            ),
        ]),
    );
    w.poll();
    assert_eq!(w.calls("fake-start").trim_end(), "7");
    // Sorted, as the shell's sorted() compared it.
    assert_eq!(
        w.approval(".retry_context", 7),
        r#"[{"at":"2026-10-01T23:55:00Z","by":"tehas-bot","source":"millwright:81","text":"<!-- tehas:millwright issue=7 action=change -->\nRun the gate first."}]"#
    );
    assert_eq!(w.approval(r#"has("comment_links")"#, 7), "false");
}

#[test]
fn the_old_fixer_marker_is_no_longer_the_retry_s_context() {
    let w = fixer(Some(0));
    icomments(
        &w,
        json!([icomment(
            84,
            "tehas-bot",
            "2026-10-01T23:56:00Z",
            "2026-10-01T23:56:00Z",
            "<!-- tehas:fixer issue=7 action=retry -->\nTry once more."
        )]),
    );
    w.poll();
    assert_eq!(w.calls("fake-start").trim_end(), "7");
    assert_eq!(w.approval(r#"has("retry_context")"#, 7), "false");
}

#[test]
fn without_such_a_comment_the_record_has_no_context() {
    let w = fixer(Some(0));
    w.poll();
    assert_eq!(w.approval(r#"has("retry_context")"#, 7), "false");
}

// --- The fixer's planning answers --------------------------------------------

const FMARK: &str = "<!-- tehas:millwright issue=7 action=answer -->";

/// Issue 7 has a planning question open in a session `by` started, with as
/// many turns as the last of `answers` on record names.
fn fixer_planning(w: &World, answers: Value, by: &str) {
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
    fixer_planning(&w, answers, "alice");
    w
}

/// One comment by the fixer, as the comments after the question.
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

/// alice saved the question, then the fixer did.
fn saved_after_alice(q: &mut Value) {
    q["userContentEdits"] = json!({"totalCount": 2, "nodes": [{"editor": {"login": "alice"}},
        {"editor": {"login": "tehas-bot"}}]});
}

fn comments_7(w: &World) -> usize {
    count_lines(&w.gh(), "issue comment 7 ")
}

#[test]
fn a_marked_reply_of_the_fixer_s_runs_the_turn_and_the_record_says_the_fixer_answered() {
    let w = planning_world(json!([{"turn": 2, "by": "tehas-bot", "fixer": true},
        {"turn": 3, "by": "alice", "fixer": false}]));
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
        "The fixer replied on the issue:\n\nAdopt all, as alice chose in #12."
    );
    has(
        &r.out,
        "planning #7 continues (answer from tehas-bot, the fixer)",
    );
    assert_eq!(
        last_answer(&w),
        json!({"turn": 4, "by": "tehas-bot", "fixer": true})
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
        json!({"turn": 2, "by": "tehas-bot", "fixer": true})
    );
}

#[test]
fn an_app_millwright_set_with_bot_ticks_a_box_as_graphql_names_it() {
    let w = planning_world(json!([]));
    w.slots(1, &[]);
    w.comments(7, &plan_body(" ", "x", " ", 1, ""), "tehas-bot", json!([]));
    w.poll();
    assert_eq!(answer(&w), "The fixer chose: Change some");
}

#[test]
fn a_box_the_fixer_ticked_runs_the_turn_as_its_choice() {
    let w = planning_world(json!([]));
    w.comments(7, &plan_body(" ", "x", " ", 1, ""), "tehas-bot", json!([]));
    w.poll();
    assert_eq!(answer(&w), "The fixer chose: Change some");
    lacks(&w.plan_calls(), "--comment");
    assert_eq!(
        w.rec()["answers"],
        json!([{"turn": 2, "by": "tehas-bot", "fixer": true}])
    );
}

#[test]
fn a_person_s_reply_wins_over_a_newer_reply_of_the_fixer_s_and_a_person_s_tick_wins_too() {
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
        json!([{"turn": 2, "by": "alice", "fixer": false}])
    );
    fixer_planning(&w, json!([]), "alice");
    w.comments(
        7,
        &plan_body(" ", "x", " ", 1, ""),
        "alice",
        freply(&marked("Adopt all."), 4003),
    );
    w.poll();
    assert_eq!(answer(&w), "The operator chose: Change some");
}

/// The fixer answered the last question, and replies to this one too.
fn twice() -> World {
    let w = planning_world(json!([{"turn": 2, "by": "tehas-bot", "fixer": true}]));
    w.comments(7, &unticked(), "", freply(&marked("Adopt all."), 4003));
    w
}

#[test]
fn never_two_in_a_row_after_the_fixer_s_answer_its_next_one_runs_no_turn_and_the_issue_says_so_once(
) {
    let w = twice();
    let r = w.poll();
    assert_eq!(w.plan_calls(), "");
    has(&r.out, "skip #7: the fixer's answer runs no turn: the fixer answered the question before this one; a person answers this one");
    assert_eq!(comments_7(&w), 1);
    has(&w.gh(), "Tehas did not take the fixer's reply as an answer");
    assert_eq!(w.rec()["fixer_refused"], json!([4003]));
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
    lacks(&r.out, "the fixer's answer runs no turn");
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

/// The fixer's tick, saved after alice saved the question.
fn held() -> World {
    let w = planning_world(json!([]));
    w.comments(
        7,
        &plan_body(" ", "x", " ", 1, ""),
        "tehas-bot",
        freply(&marked("Adopt all."), 4003),
    );
    edit_question(&w, saved_after_alice);
    w
}

#[test]
fn a_tick_the_fixer_saved_after_alice_edited_the_question_is_held_for_a_person_not_unticked() {
    let w = held();
    let r = w.poll();
    assert_eq!(w.plan_calls(), "");
    lacks(&w.gh(), "api -X PATCH");
    has(&r.out, "skip #7: the fixer saved the question after another account did, so who ticked it is not known");
    assert_eq!(comments_7(&w), 1);
    has(&w.gh(), "A person ticks the box again or replies");
    assert_eq!(w.rec()["held"], json!([4001]));
    w.clear_calls();
    w.poll();
    assert_eq!(w.plan_calls(), "");
    lacks(&w.gh(), "issue comment");
}

#[test]
fn and_alice_s_tick_saved_again_runs_the_turn_as_hers() {
    let w = held();
    w.poll();
    w.poll();
    edit_question(&w, |q| q["editor"]["login"] = json!("alice"));
    w.poll();
    assert_eq!(answer(&w), "The operator chose: Change some");
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

// The fixer is the bot, so every note the host writes is a comment of the
// fixer's account. None starts with the answer marker, so none is a reply.
#[test]
fn a_host_note_after_a_plan_question_is_no_fixer_reply_two_polls_post_nothing() {
    let w = World::new("millwright");
    for note in [
        "The fixer's tick was not taken as an answer: it was unticked. A person answers this question.",
        "Tehas did not take the box ticked in this question as an answer: the fixer saved the comment after another account did.",
        "The label `tehas:ready` was removed: mallory is not an account that may authorise work.",
    ] {
        fixer_planning(&w, json!([]), "alice");
        w.comments(7, &unticked(), "", freply(note, 4010));
        w.clear_calls();
        w.poll();
        let r = w.poll();
        assert_eq!(w.plan_calls(), "", "{note}");
        lacks(&w.gh(), "issue comment");
        lacks(&r.out, "the fixer's answer runs no turn");
    }
}

/// The fixer's tick, saved after alice saved the question, read as GitHub
/// now answers it: an id and no history, which the poll reads by the id.
fn held_by_id() -> World {
    let w = planning_world(json!([]));
    w.comments(
        7,
        &plan_body(" ", "x", " ", 1, ""),
        "tehas-bot",
        freply(&marked("Adopt all."), 4003),
    );
    edit_question(&w, |q| {
        q["id"] = json!("IC_q7");
        q.as_object_mut().unwrap().remove("userContentEdits");
    });
    w.fx(
        "comment-edits.json",
        r#"{"data":{"nodes":[{"id":"IC_q7","userContentEdits":{"totalCount":2,"nodes":[{"editor":{"login":"alice"}},{"editor":{"login":"tehas-bot"}}]}}]}}"#,
    );
    w
}

#[test]
fn a_tick_the_fixer_saved_after_alice_edited_the_question_is_held_its_history_read_by_the_comment_s_id(
) {
    let w = held_by_id();
    let r = w.poll();
    has(&w.gh(), r#"nodes(ids: ["IC_q7"])"#);
    assert_eq!(w.plan_calls(), "");
    has(&r.out, "skip #7: the fixer saved the question after another account did, so who ticked it is not known");
}

#[test]
fn a_tick_the_fixer_saved_whose_history_cannot_be_read_waits_with_no_note() {
    let w = held_by_id();
    w.poll();
    w.clear_calls();
    w.touch("comment-edits-fail");
    let r = w.poll();
    assert_eq!(w.plan_calls(), "");
    has(
        &r.out,
        "skip #7: the edit history of its question could not be read; it waits for the next poll",
    );
    lacks(&w.gh(), "issue comment");
    lacks(&w.gh(), "api -X PATCH");
}
