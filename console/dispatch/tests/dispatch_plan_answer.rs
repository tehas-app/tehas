//! The answers to a planning session: the options a turn posts, what it
//! filed, ticks and replies, and how a session ends.

mod common;

use common::*;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

fn v(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{e}: {text}"))
}

/// `filter` of #7's session on record, through jq.
fn rec_jq(w: &World, filter: &str) -> Value {
    v(&jq(filter, &w.rec().to_string()))
}

/// `filter` of what a read verb of the store answers, through jq.
fn row_jq(w: &World, args: &[&str], filter: &str) -> Value {
    v(&jq(filter, &w.row(args).to_string()))
}

/// The gh call lines that start with `start`.
fn gh_lines(w: &World, start: &str) -> usize {
    count_lines(&w.gh(), start)
}

/// The gh call lines that hold `text`.
fn gh_holding(w: &World, text: &str) -> usize {
    w.gh().lines().filter(|l| l.contains(text)).count()
}

fn answer(w: &World) -> String {
    fs::read_to_string(w.keep().join("answer.md"))
        .unwrap_or_default()
        .trim_end_matches('\n')
        .to_string()
}

fn turn_line(w: &World) -> String {
    let record = w.record().display().to_string();
    let keep = w.keep().display().to_string();
    format!("7 --out {record}.out --keep {keep} --session abc-123 --answer {keep}/answer.md")
}

/// The files under `dir`, at any depth, whose names start with `start`.
fn found(dir: &Path, start: &str) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| {
            let p = e.path();
            usize::from(e.file_name().to_string_lossy().starts_with(start))
                + if p.is_dir() { found(&p, start) } else { 0 }
        })
        .sum()
}

fn reply(id: u64, login: &str, body: &str) -> Value {
    json!([{"databaseId": id, "author": {"login": login}, "body": body,
        "createdAt": "2026-01-01T02:00:00Z"}])
}

const I41: &str = "https://github.com/acme/widget/issues/41";
const I42: &str = "https://github.com/acme/widget/issues/42";
const I43: &str = "https://github.com/acme/widget/issues/43";

#[test]
fn the_record_lists_the_options_that_were_posted_so_a_box_and_its_option_keep_the_same_place() {
    let w = World::new("plan-answer");
    w.to_plan();
    let options: Vec<Value> = (0..30)
        .map(|i| json!({"label": format!("Way {i}"), "description": ""}))
        .collect();
    w.fx_json(
        "plan-result.json",
        &json!({"session": "abc-123", "text": "Many ways.",
            "question": {"question": "Which?", "options": options}}),
    );
    w.poll();
    assert_eq!(
        rec_jq(&w, "[(.options | length), .options[0], .options[19]]"),
        json!([20, "Way 0", "Way 19"])
    );
    assert_eq!(gh_lines(&w, "- [ ] Way "), 20);
    has(&w.gh(), "Options left out: 10.");
}

#[test]
fn questions_posted_as_one_have_no_option_boxes_so_a_tick_answers_none_of_them() {
    let w = World::new("plan-answer");
    w.to_plan();
    w.fx_json(
        "plan-result.json",
        &json!({"session": "abc-123", "text": "Two things.", "question": {
            "question": "The agent asked 2 questions. Answer each by its number in a reply.\n\n1. Which?\n   - A\n\n2. Where?\n   - B",
            "options": []}}),
    );
    w.poll();
    assert_eq!(w.rec()["options"], json!([]));
    has(&w.gh(), "1. Which?");
    has(&w.gh(), "2. Where?");
    assert_eq!(gh_lines(&w, "- [ ] "), 1);
    has(&w.gh(), "- [ ] End this planning session");
}

#[test]
fn a_turn_that_filed_issues_says_so_under_its_words_and_keeps_them_on_record() {
    let w = World::new("plan-answer");
    w.to_plan();
    w.plan_files(&[I42, I43]);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        gh_holding(&w, &format!("Filed in this turn: {I42}, {I43}")),
        1
    );
    assert_eq!(w.rec()["filed"], json!([I42, I43]));
}

#[test]
fn the_next_turn_of_a_session_that_filed_is_told_not_to_file_them_again() {
    let w = World::new("plan-answer");
    w.planning();
    w.t_plan(
        7,
        json!({"turns": 1, "options": ["Adopt all", "Change some"], "comment": 4001,
            "filed": [I42, I43]}),
    );
    w.plan_files(&[]);
    w.comments(7, &plan_filed_body(1, &[I42, I43]), "alice", json!([]));
    w.poll();
    assert_eq!(w.plan_calls().trim_end(), turn_line(&w));
    assert_eq!(
        answer(&w),
        format!("Already filed in this session: {I42}, {I43}\n\nThe operator chose: Change some")
    );
    // The answer on record is the answer as it came; the turn was given it with
    // the line on top.
    let at = fs::read_to_string(w.t().join("calls/record.at-turn")).unwrap_or_default();
    assert_eq!(
        v(&at)["planning"]["answer"]
            .as_str()
            .map(|a| a.trim_end_matches('\n')),
        Some("The operator chose: Change some")
    );
}

#[test]
fn what_a_later_turn_files_is_added_to_what_the_session_filed_before() {
    let w = World::new("plan-answer");
    w.planning();
    w.t_plan(
        7,
        json!({"turns": 1, "options": ["Adopt all", "Change some"], "comment": 4001,
            "filed": [I41]}),
    );
    w.plan_files(&[I43, I42]);
    w.comments(7, &plan_filed_body(1, &[I41]), "alice", json!([]));
    w.poll();
    assert_eq!(rec_jq(&w, ".filed | sort"), json!([I41, I42, I43]));
    assert_eq!(
        gh_holding(&w, &format!("Filed in this turn: {I42}, {I43}")),
        1
    );
}

#[test]
fn a_turn_that_filed_nothing_names_none_and_the_next_answer_says_nothing_of_filings() {
    let w = World::new("plan-answer");
    w.planning();
    w.plan_files(&[]);
    w.comments(7, &plan_filed_body(1, &[]), "alice", json!([]));
    w.poll();
    assert_eq!(w.plan_calls().trim_end(), turn_line(&w));
    assert_eq!(answer(&w), "The operator chose: Change some");
    lacks(&w.gh(), "Filed in this turn");
    assert!(w.rec().get("filed").is_none(), "the record has a filed key");
}

#[test]
fn of_what_a_turn_says_it_filed_only_github_issue_addresses_are_kept_each_once() {
    let w = World::new("plan-answer");
    w.to_plan();
    w.plan_files(&[I42, "https://evil.example/acme/widget/issues/8", I42]);
    w.poll();
    assert_eq!(w.rec()["filed"], json!([I42]));
    w.clear_calls();
    w.fx("asking.json", r#"[{"number":7}]"#);
    w.fx("plans.json", "[]");
    w.plan_files(&[]);
    w.comments_of(7, 4002, &plan_filed_body(1, &[I42]), "alice");
    w.poll();
    assert_eq!(
        answer(&w).lines().next(),
        Some(format!("Already filed in this session: {I42}").as_str())
    );
    lacks(&answer(&w), "evil.example");
}

#[test]
fn a_reply_s_turn_is_given_the_reply_s_comment_id_so_its_attachments_go_with_it() {
    let w = World::new("plan-answer");
    w.planning();
    w.comments(
        7,
        &plan_body(" ", " ", " ", 1, ""),
        "",
        reply(
            4003,
            "alice",
            "Adopt all, but keep `tagged-at` open.\nThen file 3 and 5.",
        ),
    );
    w.poll();
    has(
        &w.plan_calls(),
        &format!("{} --comment 4003", turn_line(&w)),
    );
}

#[test]
fn a_reply_and_a_tick_from_a_listed_login_with_another_user_id_run_no_turn() {
    // The login alice changed hands: the account that answers is not the one listed.
    let w = World::new("plan-answer");
    w.planning();
    let mut after = reply(4003, "alice", "file everything");
    after[0]["author"]["databaseId"] = json!(99);
    w.comments(7, &plan_body("x", " ", " ", 1, ""), "alice", after);
    let path = w.fx.join("comments-7.json");
    let mut c = v(&fs::read_to_string(&path).unwrap());
    c["data"]["repository"]["issue"]["comments"]["nodes"][0]["editor"]["databaseId"] = json!(99);
    w.fx_json("comments-7.json", &c);
    let r = w.poll();
    assert_eq!(w.plan_calls(), "");
    has(
        &r.out,
        &format!(
            "tehas: alice acted with the user id 99, not the recorded {}, so it does not count",
            uid("alice")
        ),
    );
    has(&w.gh(), "api -X PATCH");
}

#[test]
fn the_turn_limit_ends_the_session_in_place_of_another_turn() {
    let w = World::new("plan-answer");
    w.planning();
    w.slots(1, &["TEHAS_PLAN_MAX_TURNS=2"]);
    w.t_plan(
        7,
        json!({"turns": 2, "options": ["Adopt all", "Change some"], "comment": 4001}),
    );
    w.comments(7, &plan_body("x", " ", " ", 2, ""), "alice", json!([]));
    w.poll();
    assert_eq!(w.plan_calls(), "");
    has(&w.gh(), "reached its limit of 2 turns");
    assert_eq!(w.rec_open(), "none");
    assert_eq!(found(&w.home.join("tmp"), "plan-answer."), 0);
}

#[test]
fn a_later_turn_that_fails_asks_again_and_keeps_the_session() {
    let w = World::new("plan-answer");
    w.planning();
    w.comments(7, &plan_body("x", " ", " ", 1, ""), "alice", json!([]));
    w.touch("plan-fails");
    w.poll();
    has(&w.gh(), "This planning turn did not finish");
    has(
        &w.gh(),
        "--add-label tehas:question --add-label needs-human-attention",
    );
    assert_eq!(
        rec_jq(&w, "[.session, .turns, .failed]"),
        json!(["abc-123", 2, true])
    );
}

#[test]
fn a_later_turn_whose_session_pi_rukas_no_longer_has_ends_the_session_and_says_to_start_again() {
    let w = World::new("plan-answer");
    w.planning();
    // Made in tehas's own directory of workspaces, as an earlier turn leaves it.
    fs::create_dir_all(w.keep()).unwrap();
    fs::write(w.home.join("work/.tehas-workspaces"), "").unwrap();
    w.comments(7, &plan_body("x", " ", " ", 1, ""), "alice", json!([]));
    w.fx("plan-fails", "{\"session\":\"abc-123\",\"lost\":true}\n");
    let r = w.poll();
    has(&w.plan_calls(), "--session abc-123");
    has(&r.out, "planning #7: Pi no longer has session abc-123");
    let gh = w.gh();
    has(&gh, "--remove-label tehas:question --remove-label tehas:planning --remove-label tehas:plan --remove-label needs-human-attention");
    has(&gh, "pi-rukas has no record of it on");
    has(&gh, "Apply `tehas:plan` again to start a new session.");
    lacks(&gh, "This planning turn did not finish");
    lacks(&gh, "tehas:question kind=plan issue=7 turn=2");
    assert_eq!(w.rec_open(), "none");
    assert!(!w.keep().exists());
    // It ended lost, and the turn's line says so.
    assert_eq!(
        row_jq(
            &w,
            &["plan-show", "7"],
            ".planning | [.lost, (.ended_at | type)]"
        ),
        json!([true, "number"])
    );
    assert_eq!(
        row_jq(
            &w,
            &["history-show", "7", "--kind", "plan"],
            ".rows[-1] | [.turn, .outcome]"
        ),
        json!([2, "lost"])
    );
}

#[test]
fn a_refine_the_poll_started_on_its_own_hands_the_issue_back_when_it_ends_without_a_plan() {
    let w = World::new("plan-answer");
    w.planning();
    w.slots(1, &["TEHAS_PLAN_MAX_TURNS=2"]);
    w.t_plan(
        7,
        json!({"turns": 2, "options": ["Adopt all", "Change some"], "comment": 4001,
            "label": "tehas:refine", "auto": true}),
    );
    w.comments(7, &plan_body("x", " ", " ", 2, ""), "alice", json!([]));
    w.poll();
    has(&w.gh(), "reached its limit of 2 turns");
    has(
        &w.gh(),
        "--remove-label tehas:plan --add-label needs-human-attention",
    );
    assert_eq!(w.rec_open(), "none");

    w.planning();
    w.t_plan(
        7,
        json!({"session": "abc-123", "turns": 1, "options": ["Adopt all", "Change some"],
            "comment": 4001, "label": "tehas:refine", "auto": true}),
    );
    w.comments(7, &plan_body("x", " ", " ", 1, ""), "alice", json!([]));
    w.fx("plan-fails", "{\"session\":\"abc-123\",\"lost\":true}\n");
    w.poll();
    has(&w.gh(), "pi-rukas has no record of it on");
    has(
        &w.gh(),
        "--remove-label tehas:plan --add-label needs-human-attention",
    );

    // A refine a label started ends as a plan does.
    w.planning();
    w.slots(1, &["TEHAS_PLAN_MAX_TURNS=2"]);
    w.t_plan(
        7,
        json!({"turns": 2, "options": ["Adopt all", "Change some"], "comment": 4001,
            "label": "tehas:refine"}),
    );
    w.comments(7, &plan_body("x", " ", " ", 2, ""), "alice", json!([]));
    w.poll();
    has(
        &w.gh(),
        "--remove-label tehas:plan --remove-label needs-human-attention",
    );
}

#[test]
fn a_lost_session_whose_labels_cannot_be_taken_off_is_ended_by_the_next_poll() {
    let w = World::new("plan-answer");
    w.planning();
    w.comments(7, &plan_body("x", " ", " ", 1, ""), "alice", json!([]));
    w.fx("plan-fails", "{\"session\":\"abc-123\",\"lost\":true}\n");
    w.fx(
        "edit-fails-on",
        "--remove-label tehas:question --remove-label tehas:planning\n",
    );
    w.poll();
    assert_eq!(rec_jq(&w, "[.lost, .running]"), json!([true, true]));
    lacks(&w.gh(), "pi-rukas has no record of it on");
    w.clear_calls();
    w.rm_fx("edit-fails-on");
    w.rm_fx("plan-fails");
    w.poll();
    assert_eq!(w.plan_calls(), "");
    has(&w.gh(), "pi-rukas has no record of it on");
    assert_eq!(w.rec_open(), "none");
}

// The decisions of the next three are fixture cases too; these hold what the
// poll writes for them.
#[test]
fn a_ticked_option_goes_to_the_session_as_the_next_message() {
    let w = World::new("plan-answer");
    w.planning();
    w.comments(7, &plan_body(" ", "x", " ", 1, ""), "alice", json!([]));
    w.poll();
    has(&w.plan_calls(), &turn_line(&w));
    assert_eq!(answer(&w), "The operator chose: Change some");
    has(&w.gh(), "<!-- tehas:question kind=plan issue=7 turn=2 -->");
    assert_eq!(w.rec()["turns"], json!(2));
    // The answer's text is on record with who gave it, and nothing is left in tmp.
    assert_eq!(
        row_jq(
            &w,
            &["history-show", "7", "--kind", "plan"],
            "[.rows[] | .turn]"
        ),
        json!([1, 2])
    );
    assert_eq!(found(&w.home.join("tmp"), "plan-answer."), 0);
}

#[test]
fn the_way_out_ends_the_session_labels_off_record_gone_one_comment() {
    let w = World::new("plan-answer");
    w.planning();
    fs::create_dir_all(w.keep().join("images")).unwrap();
    fs::write(w.home.join("work/.tehas-workspaces"), "").unwrap();
    w.comments(7, &plan_body(" ", " ", "x", 1, ""), "alice", json!([]));
    w.poll();
    assert_eq!(w.plan_calls(), "");
    has(&w.gh(), "--remove-label tehas:question");
    has(&w.gh(), "Planning session ended by alice.");
    assert_eq!(w.rec_open(), "none");
    assert!(!w.keep().exists());
    // The question it ended in is withdrawn with the session.
    let q = w.row(&["question-known", "7", "--id", "4001"]);
    assert_eq!(json!([q["known"], q["recorded"]]), json!([false, false]));
}

#[test]
fn an_outcome_that_could_not_be_posted_is_posted_by_the_next_poll_before_anything_else() {
    let w = World::new("plan-answer");
    w.planning();
    w.comments(7, &plan_body("x", " ", " ", 1, ""), "alice", json!([]));
    w.touch("post-fails");
    w.poll();
    assert_eq!(w.rec()["pending"], json!(true));
    let mut out = w.record().into_os_string();
    out.push(".out");
    assert!(fs::metadata(&out).map(|m| m.len() > 0).unwrap_or(false));
    lacks(&w.gh(), "--add-label tehas:question");
    w.clear_calls();
    w.rm_fx("post-fails");
    w.fx("asking.json", "[]");
    w.poll();
    assert_eq!(w.plan_calls(), "");
    has(&w.gh(), "<!-- tehas:question kind=plan issue=7 turn=2 -->");
    has(
        &w.gh(),
        "--remove-label tehas:planning --add-label tehas:question",
    );
    assert_eq!(w.rec()["pending"], json!(false));
}

#[test]
fn a_ticked_option_passes_no_comment() {
    let w = World::new("plan-answer");
    w.planning();
    w.comments(7, &plan_body(" ", "x", " ", 1, ""), "alice", json!([]));
    w.poll();
    has(&w.plan_calls(), &turn_line(&w));
    lacks(&w.plan_calls(), "--comment");
}

#[test]
fn the_record_of_a_session_whose_issue_no_longer_asks_is_removed() {
    let w = World::new("plan-answer");
    w.planning();
    w.fx("asking.json", "[]");
    w.poll();
    assert_eq!(w.rec_open(), "none");
    lacks(&w.gh(), "issue comment");
}
