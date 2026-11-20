//! Refreshes of pull requests that conflict: how one starts and what drops
//! it, where the case reads the calls the poll makes or what it records. The
//! decisions alone are cases of tests/fixtures/refresh.json.

mod common;
#[allow(dead_code)] // the helpers are shared with dispatch_refresh_hold.rs
mod refresh;

use common::*;
use refresh::*;
use serde_json::{json, Value};
use std::fs;

/// The REST call of rule 4b that marks a pull request.
const MARK_CALL: &str =
    "api -X POST repos/acme/widget/issues/50/labels -f labels[]=tehas:refreshing";

/// The two calls of a start that change a label, the mark of the pull
/// request first and the mark of the issue second.
fn label_pair(w: &World) -> String {
    w.gh()
        .lines()
        .filter(|l| l.starts_with("api -X POST") || l.starts_with("issue edit 7"))
        .map(|l| format!("{l}\n"))
        .collect::<String>()
        .trim_end()
        .to_string()
}

/// What the poll counted as refreshes of #7 in the record it wrote: "none"
/// when it wrote none at all.
fn record_refreshes(w: &World) -> String {
    w.approval(r#".refreshes // "none""#, 7)
}

/// A refresh started nothing, and its request is gone.
#[track_caller]
fn refresh_dropped(w: &World, r: &Run, why: &str) {
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    lacks(&w.gh(), "--add-label tehas:refreshing");
    has(&r.out, &format!("drop the refresh of #7: {why}"));
    assert_eq!(requests(w), 0);
}

/// The request of issue 7 is one tehasbot refresh --pr N wrote.
fn asked_pr(w: &World) {
    let p = refresh_dir(w).join("7.json");
    let mut v: Value = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
    v["asked"] = json!("pr");
    fs::write(p, v.to_string()).unwrap();
}

/// The image's answer: its flags of work, and the features when given.
fn caps(w: &World, features: bool) {
    let mut v = json!({"verbs": {"work": {"flags": ["--json", "--timeout", "--restart"]}}});
    if features {
        v["verbs"]["work"]["flags"]
            .as_array_mut()
            .unwrap()
            .push(json!("--continue-pr"));
    }
    w.fx_json("caps.json", &v);
}

/// What the demand file says, as tehas status reads it.
fn demand(w: &World) -> Value {
    serde_json::from_str(&fs::read_to_string(w.demand_file()).unwrap()).unwrap()
}

/// The refresh of the setup, started.
fn started_refresh(name: &str) -> World {
    let w = refresh_world(name, &[]);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    w
}

#[test]
fn a_refresh_starts_the_pull_request_is_marked_first_then_the_issue_and_the_request_goes() {
    let w = refresh_world("start", &[]);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
    has(&r.out, "start #7 (refresh by alice)");
    let edits = label_pair(&w);
    has(&edits, MARK_CALL);
    has(
        &edits,
        "issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:awaiting-merge",
    );
    assert_eq!(edits.lines().next(), Some(MARK_CALL));
    assert_eq!(edits.lines().count(), 2);
    lacks(&w.gh(), "pr edit");
    assert_eq!(requests(&w), 0);
}

#[test]
fn the_start_comment_says_it_is_a_refresh_names_the_pull_request_and_whose_approval_it_rests_on() {
    let w = started_refresh("start-comment");
    let gh = w.gh();
    has(&gh, "Tehas refresh started on");
    has(
        &gh,
        "https://github.com/acme/widget/pull/50 conflicts with `main`",
    );
    has(
        &gh,
        "It rests on @alice's approval of 2026-10-01T00:00:00Z.",
    );
}

#[test]
fn the_approval_record_of_a_refresh_by_the_person_rests_on_refreshes_refresh_of_and_a_fixed_retry_context(
) {
    let w = started_refresh("start-record");
    let filter = format!(
        "[.by, .how, .at, .body_sha256 == \"{}\", .rests_on.by, .rests_on.how, .refreshes, .fixer_starts, \
         .refresh_of.pr, .refresh_of.head_sha == \"{RHEAD}\", .refresh_of.base_sha == \"{RBASE}\", .refresh_of.branch]",
        wsum()
    );
    assert_eq!(
        w.approval(&filter, 7),
        r#"["alice","refresh","2026-10-01T00:00:00Z",true,"alice","label",1,0,50,true,true,"main"]"#
    );
    assert_eq!(
        w.approval("[.retry_context[] | {source, by}]", 7),
        r#"[{"source":"refresh","by":"tehas"}]"#
    );
    assert_eq!(
        w.approval(".retry_context[0].text", 7),
        "This pull request conflicts with `main`. Merge `main` into its branch, resolve the conflicts so the change still does what the issue asks, run the gate in `.pi/verify-cmd`, and push to the same branch. Do no other work."
    );
}

#[test]
fn the_record_was_on_disk_before_the_issue_was_marked_and_after_the_mark() {
    let w = started_refresh("start-at-mark");
    assert_eq!(w.at_mark(".how"), "refresh");
    // On record at the time the poll decided it: the time of the approval it
    // rests on is the key of a row that started a cycle already.
    let at = w.row(&["approval-current", "7"])["current"]["at"]
        .as_i64()
        .unwrap();
    assert!(at > epoch("2026-10-01T00:00:00Z"));
    assert_eq!(
        w.at_mark("[.refresh_pr, .refresh_head]"),
        format!("[50,\"{RHEAD}\"]")
    );
    // The mark of the pull request comes before the mark of the issue, so the
    // record that says refresh is on disk between them.
    let pair = label_pair(&w);
    assert_eq!(pair.lines().next(), Some(MARK_CALL));
    has(pair.lines().last().unwrap(), "--add-label tehas:running");
}

#[test]
fn a_mark_that_fails_starts_nothing_marks_no_issue_and_leaves_no_refresh_in_the_count() {
    let w = refresh_world("mark-fails", &[]);
    w.touch("mark-fails");
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    assert_eq!(w.started(), "");
    lacks(&w.gh(), "issue edit 7");
    lacks(&w.gh(), "pr edit");
    has(&r.out, "could not mark pull request #50 as refreshing");
    assert_eq!(requests(&w), 1);
    assert_eq!(record_refreshes(&w), "none");
}

#[test]
fn the_approval_record_of_the_last_start_is_put_back_so_failed_starts_spend_no_refresh() {
    let w = refresh_world("put-back", &[]);
    refresh_record(&w, 1, false);
    w.touch("mark-fails");
    assert_eq!(w.poll().rc, 1);
    assert_eq!(record_refreshes(&w), "1");
    w.fx("edit-fails-on", "--add-label tehas:running\n");
    assert_eq!(w.poll().rc, 1);
    assert_eq!(w.started(), "");
    assert_eq!(record_refreshes(&w), "1");
    w.rm_fx("mark-fails");
    w.rm_fx("edit-fails-on");
    w.poll();
    assert_eq!(w.started(), "7 ");
    assert_eq!(record_refreshes(&w), "2");
}

#[test]
fn the_first_two_marks_fail_and_the_third_starts_the_refresh_the_failures_counted_for_nothing() {
    let w = refresh_world("marks-fail-twice", &[]);
    w.fx("mark-fails-times", "2\n");
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    assert_eq!(w.started(), "");
    has(&r.out, "could not mark pull request #50 as refreshing");
    assert_eq!(record_refreshes(&w), "none");
    assert_eq!(w.poll().rc, 1);
    assert_eq!(w.started(), "");
    assert_eq!(record_refreshes(&w), "none");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
    assert_eq!(record_refreshes(&w), "1");
    // Three marks were asked for: the two that failed and the one that worked.
    assert_eq!(count_lines(&w.gh(), "api -X POST"), 3);
    assert_eq!(requests(&w), 0);
}

#[test]
fn the_reaper_clears_a_mark_its_cycle_could_not_take_off() {
    let w = refresh_world("reaper-unmark", &[]);
    fs::remove_file(refresh_dir(&w).join("7.json")).unwrap();
    refresh_record(&w, 1, false);
    marked_refreshing(&w);
    w.touch("unmark-fails");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "WARN: label change on pull request #50 failed: --remove-label tehas:refreshing",
    );
    w.touch("refreshing.json");
    w.rm_fx("unmark-fails");
    w.poll();
    has(&w.gh(), UNMARK_CALL);
}

#[test]
fn a_refresh_after_a_refresh_counts_on_from_it() {
    let w = refresh_world("after-refresh", &[]);
    refresh_record(&w, 1, false);
    w.poll();
    assert_eq!(w.started(), "7 ");
    assert_eq!(
        w.approval("[.refreshes, .rests_on.by]", 7),
        r#"[2,"alice"]"#
    );
}

#[test]
fn a_refresh_of_a_fixer_s_start_rests_on_the_person_s_approval_and_keeps_the_fixer_s_count() {
    let w = refresh_world("fixer-start", &[]);
    w.past(7, "tehas-bot", "fixer", "2026-10-02T00:00:00Z", &[]);
    w.poll();
    assert_eq!(w.started(), "7 ");
    assert_eq!(
        w.approval("[.by, .how, .rests_on.by, .fixer_starts, .refreshes]", 7),
        r#"["alice","refresh","alice",1,1]"#
    );
}

#[test]
fn a_start_whose_pull_request_label_fails_starts_nothing_and_marks_no_issue() {
    let w = refresh_world("label-fails", &[]);
    w.touch("mark-fails");
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    assert_eq!(w.started(), "");
    lacks(&w.gh(), "issue edit 7");
    lacks(&w.gh(), "pr edit");
    has(&r.out, "could not mark pull request #50 as refreshing");
    assert_eq!(requests(&w), 1);
}

#[test]
fn a_request_tehasbot_refresh_pr_n_wrote_starts_a_refresh() {
    let w = refresh_world("asked-pr", &[]);
    asked_pr(&w);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
    has(&r.out, "start #7 (refresh by alice)");
    lacks(&r.out, "hold refreshes");
    has(&label_pair(&w), MARK_CALL);
    assert_eq!(requests(&w), 0);
}

#[test]
fn a_request_without_asked_as_the_timed_scan_of_before_wrote_it_starts_too() {
    let w = refresh_world("asked-none", &[]);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
    lacks(&r.out, "hold refreshes");
    assert_eq!(w.read_home("state/refresh/.held"), "");
}

#[test]
fn a_pr_request_counts_in_the_demand_and_nothing_is_held() {
    let w = refresh_world("asked-demand", &[]);
    asked_pr(&w);
    // Every slot is busy, so the request waits.
    w.slots(1, &[]);
    running_refresh(&w, 9);
    let r = w.poll();
    w.release();
    assert_eq!(w.started(), "");
    let d = demand(&w);
    assert_eq!(
        json!({"waiting": d["waiting"], "oldest": d["oldest"], "refreshHold": d["refreshHold"]}),
        json!({"waiting": 1, "oldest": "2026-10-17T09:00:00Z", "refreshHold": null})
    );
    assert_eq!(w.read_home("state/refresh/.held"), "");
    lacks(&r.out, "hold refreshes");
    assert_eq!(requests(&w), 1);
}

#[test]
fn a_pr_request_whose_pull_request_no_longer_conflicts_is_dropped() {
    let w = refresh_world("asked-merged", &[]);
    asked_pr(&w);
    pull(&w, |p| p["mergeable"] = json!("MERGEABLE"));
    let r = w.poll();
    refresh_dropped(&w, &r, "pull request #50 no longer conflicts");
}

#[test]
fn an_image_without_continue_pr_holds_a_pr_request_which_is_not_counted() {
    let w = refresh_world("asked-no-continue", &[]);
    asked_pr(&w);
    caps(&w, false);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    let d = demand(&w);
    has(
        d["refreshHold"].as_str().unwrap_or(""),
        "does not take --continue-pr",
    );
    assert_eq!(d["waiting"], 0);
    assert_eq!(requests(&w), 1);
}

// The slice of reset holds four containers: a quarter of it is a container's
// ceiling.
#[test]
fn a_pr_request_keeps_the_lane_it_starts_there_and_the_hold_says_so() {
    let w = refresh_world("asked-lane", &[]);
    w.slots(4, &["TEHAS_REFRESH_SLOTS=1"]);
    asked_pr(&w);
    w.queue(&["20", "21", "22", "23"]);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 20 21 22 ");
    has(
        &r.out,
        "holding: 3 of 4 free slots start: tehas.slice has room for 4 more and the refresh lane keeps 1",
    );
    has(&r.out, "start #7 (refresh by alice)");
}

#[test]
fn a_running_refresh_fills_the_lane_a_pr_request_waits() {
    let w = refresh_world("asked-lane-full", &[]);
    w.slots(3, &["TEHAS_REFRESH_SLOTS=1"]);
    asked_pr(&w);
    running_refresh(&w, 8);
    let r = w.poll();
    w.release();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    has(&r.out, "wait #7: the refresh lane is full (1 of 1)");
    assert_eq!(requests(&w), 1);
}

#[test]
fn with_tehas_refresh_slots_0_a_pr_request_takes_a_cycle_slot() {
    let w = refresh_world("asked-no-lane", &[]);
    w.slots(1, &["TEHAS_REFRESH_SLOTS=0"]);
    asked_pr(&w);
    w.queue(&["9"]);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
    lacks(&r.out, "refresh lane");
    assert!(w.dispatch_state()["refreshRoom"].is_null());
}

#[test]
fn with_no_request_and_no_refresh_the_lane_keeps_nothing_and_tehas_refreshing_is_not_listed() {
    let w = refresh_world("no-request", &[]);
    w.slots(4, &[]);
    fs::remove_file(refresh_dir(&w).join("7.json")).unwrap();
    w.queue(&["20", "21", "22", "23"]);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "20 21 22 23 ");
    lacks(&r.out, "refresh lane");
    lacks(&w.gh(), "--label tehas:refreshing");
}

#[test]
fn a_stale_tehas_refreshing_is_cleared_while_a_pr_request_waits() {
    let w = refresh_world("asked-stale-mark", &[]);
    asked_pr(&w);
    w.fx(
        "refreshing.json",
        r#"[{"number":60,"labels":[{"name":"tehas:refreshing"}]}]"#,
    );
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&w.gh(), "--label tehas:refreshing");
    has(
        &w.gh(),
        "api -X DELETE repos/acme/widget/issues/60/labels/tehas:refreshing",
    );
    has(
        &r.out,
        "clear pull request #60: tehas:refreshing off, no refresh on this host names it",
    );
}

#[test]
fn an_image_without_continue_pr_holds_refreshes_said_once_and_in_tehas_status_s_demand() {
    let w = refresh_world("no-continue", &[]);
    caps(&w, false);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    has(&r.out, "hold refreshes: the pi-rukas in the sandbox image");
    let d = demand(&w);
    has(
        d["refreshHold"].as_str().unwrap_or(""),
        "does not take --continue-pr",
    );
    assert_eq!(d["waiting"], 0);
    assert_eq!(requests(&w), 1);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    lacks(&r.out, "hold refreshes");
    has(
        demand(&w)["refreshHold"].as_str().unwrap_or(""),
        "does not take --continue-pr",
    );
    assert_eq!(requests(&w), 1);
}

#[test]
fn the_flag_alone_is_not_enough_an_answer_without_features_continue_pr_holds_refreshes() {
    let w = refresh_world("flag-alone", &[]);
    caps(&w, true);
    let r = w.poll();
    assert_eq!(w.started(), "");
    has(&r.out, "hold refreshes: the pi-rukas in the sandbox image");
}

#[test]
fn an_image_that_cannot_be_asked_holds_refreshes_too() {
    let w = refresh_world("no-caps", &[]);
    w.rm_fx("caps.json");
    let r = w.poll();
    assert_eq!(w.started(), "");
    has(&r.out, "hold refreshes");
}

/// Issue 7 ended as a handoff, its pull request open, with `labels`.
fn handed_off(w: &World, labels: &[&str]) {
    let all: Vec<Value> = std::iter::once("needs-human-attention")
        .chain(labels.iter().copied())
        .map(|l| json!({"name": l}))
        .collect();
    w.fx_json(
        "work-7.json",
        &json!({"number": 7, "state": "open", "labels": all}),
    );
    pull(w, |p| {
        p["closingIssuesReferences"]["nodes"][0]["labels"]["nodes"] =
            json!([{"name": "needs-human-attention"}])
    });
    w.fx("awaiting.json", "[]");
}

#[test]
fn a_handed_off_issue_s_pull_request_is_refreshed_the_issue_is_marked_running_with_no_label_to_take_off(
) {
    let w = refresh_world("handed-off", &[]);
    handed_off(&w, &[]);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
    let edits = label_pair(&w);
    has(&edits, MARK_CALL);
    has(
        &edits,
        "issue edit 7 --repo acme/widget --add-label tehas:running",
    );
    lacks(&edits, "--remove-label");
    assert_eq!(w.approval(".how", 7), "refresh");
}

#[test]
fn a_draft_pull_request_is_refreshed_and_it_stays_a_draft() {
    let w = refresh_world("draft", &[]);
    pull(&w, |p| p["isDraft"] = json!(true));
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
    lacks(&w.gh(), "pr edit");
}

#[test]
fn a_handed_off_issue_that_carries_tehas_question_is_refreshed_and_the_start_takes_it_off() {
    let w = refresh_world("handed-off-question", &[]);
    handed_off(&w, &["tehas:question"]);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
    let edits = label_pair(&w);
    has(
        &edits,
        "issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:question",
    );
    assert_eq!(edits.lines().count(), 2);
}

#[test]
fn an_issue_that_carries_tehas_awaiting_merge_and_tehas_question_has_both_taken_off() {
    let w = refresh_world("awaiting-question", &[]);
    issue7(&w, &["tehas:question"]);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
    has(
        &label_pair(&w),
        "--remove-label tehas:awaiting-merge --remove-label tehas:question",
    );
}
