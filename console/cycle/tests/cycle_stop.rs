//! A stop or a hangup while a cycle runs, and the calls to GitHub it makes
//! again when they are refused.

mod common;

use common::{count_lines, has, lacks, Run, World};
use serde_json::{json, Value};
use std::fs;
use std::process::Command;
use std::time::{Duration, Instant};

const PR9: &str = "https://github.com/acme/widget/pull/9";

fn pr9(w: &World) {
    w.fx("pr.txt", &format!("{PR9}\n"));
}

/// The lines of `text` that hold `needle`.
fn count_in(text: &str, needle: &str) -> usize {
    text.lines().filter(|l| l.contains(needle)).count()
}

/// The runner notes the cycle's pid (its parent) before what it does.
const PID: &str = r#"echo "$PPID" >"$T/calls/cycle.pid""#;

/// A runner whose trap of TERM is set before it says it is ready.
const TRAPPED: &str = r#"sleep 30 & s=$!
trap "kill \$s; : >\"\$T/calls/work.term\"; exit 2" TERM
: >"$T/calls/work.ready"
wait "$s""#;

/// A cycle of #7 sent `signal` once `calls/<ready>` is there, the wait for it
/// bounded so a cycle that never gets there fails the case rather than hang.
/// The run, and how long the cycle took to end after the signal.
fn signalled(w: &World, ready: &str, signal: &str) -> (Run, Duration) {
    let calls = w.t().join("calls");
    std::thread::scope(|s| {
        let sender = s.spawn(|| {
            for _ in 0..100 {
                if calls.join(ready).exists() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            let pid = fs::read_to_string(calls.join("cycle.pid")).expect("no pid of the cycle");
            let pid = pid.trim().to_string();
            let comm = fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
            assert_eq!(comm.trim(), "tehas-cycle", "the pid is the cycle's");
            let at = Instant::now();
            let ok = Command::new("kill")
                .args([&format!("-{signal}"), &pid])
                .status()
                .unwrap();
            assert!(ok.success());
            at
        });
        let r = w.run7();
        let at = sender.join().unwrap();
        (r, at.elapsed())
    })
}

#[test]
fn a_stop_is_passed_on_to_the_runner_and_the_cycle_is_still_reported() {
    let w = World::new("st-term");
    w.work(&format!("{PID}\n{TRAPPED}"));
    let (r, _) = signalled(&w, "work.ready", "TERM");
    assert_eq!(r.rc, 0, "{}", r.out);
    assert!(w.t().join("calls/work.term").exists());
    has(
        &w.gh(),
        "--remove-label tehas:running --add-label needs-human-attention",
    );
    has(&w.gh(), "(runner exit 2)");
}

#[test]
fn a_stop_that_arrives_while_the_report_is_made_does_not_cut_the_report_short() {
    let w = World::new("st-report");
    // The first relabel takes a second and says when it starts.
    w.touch("edit-slow");
    w.work(PID);
    let (r, _) = signalled(&w, "edit.started", "TERM");
    assert_eq!(r.rc, 0, "{}", r.out);
    let gh = w.gh();
    has(&gh, "issue edit 7");
    has(&gh, "Tehas cycle stopped without a pull request");
    has(
        &gh,
        "issue edit 7 --repo acme/widget --add-label tehas:question",
    );
}

#[test]
fn a_stop_while_github_does_not_answer_is_reported_with_one_bounded_try_a_call_and_no_status_refresh(
) {
    let mut w = World::new("st-hangs");
    w.env("TEHAS_CYCLE_STOP_GH_WAIT", "1");
    w.ended("running", "develop", "feature/x", "");
    // GitHub stops answering once the cycle runs.
    w.work(&format!(
        r#"{PID}
sleep 30 & s=$!
trap "kill \$s; exit 2" TERM
: >"$T/fx/gh-hangs"
: >"$T/calls/work.ready"
wait "$s""#
    ));
    let (r, took) = signalled(&w, "work.ready", "TERM");
    assert_eq!(r.rc, 0, "{}", r.out);
    // Four calls, each cut after a second. Three tries of each, or the
    // default bound of 20 seconds, would take far longer. The report ignores
    // TERM, so a bound that only sent TERM would not cut a call.
    assert!(took <= Duration::from_secs(15), "{took:?}");
    let gh = w.gh();
    assert_eq!(count_in(&gh, "--remove-label tehas:running"), 1);
    assert_eq!(count_lines(&gh, "pr list "), 1);
    assert_eq!(w.calls("fake-status"), "");
    // A call cut at its bound is a write that failed, or one GitHub may have
    // taken that no read could settle (the read hangs too): under load either.
    assert!(
        r.out.contains("WARN: label change on #7 failed")
            || r.out.contains("WARN: a write on #7 may have been posted"),
        "{}",
        r.out
    );
}

#[test]
fn a_cycle_that_was_not_stopped_still_refreshes_the_status_and_tries_three_times() {
    let w = World::new("st-notstopped");
    w.fx("edit-fails-on", "--remove-label tehas:running\n");
    w.run7();
    assert_eq!(count_in(&w.gh(), "--remove-label tehas:running"), 3);
    assert_eq!(w.calls("fake-status").lines().count(), 1);
}

#[test]
fn a_hangup_is_a_stop_too_the_cycle_is_still_reported() {
    let w = World::new("st-hup");
    w.work(&format!("{PID}\n{TRAPPED}"));
    let (r, _) = signalled(&w, "work.ready", "HUP");
    assert_eq!(r.rc, 0, "{}", r.out);
    assert!(w.t().join("calls/work.term").exists());
    has(
        &w.gh(),
        "--remove-label tehas:running --add-label needs-human-attention",
    );
}

#[test]
fn a_label_change_that_github_refuses_once_is_made_again() {
    let w = World::new("st-label-once");
    w.ended("handoff", "commit-pr", "feature/x", "");
    pr9(&w);
    w.fx("edit-fails-times", "1\n");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert_eq!(
        count_in(
            &w.gh(),
            "--remove-label tehas:running --add-label tehas:awaiting-merge"
        ),
        2
    );
    lacks(&r.out, "WARN: label change");
    has(&w.gh(), "Pull request waiting for a human");
}

#[test]
fn after_three_refusals_it_gives_up_says_so_in_the_log_and_still_reports() {
    let w = World::new("st-label-three");
    w.ended("handoff", "commit-pr", "feature/x", "");
    pr9(&w);
    w.fx("edit-fails-on", "--remove-label tehas:running\n");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert_eq!(count_in(&w.gh(), "--remove-label tehas:running"), 3);
    has(&r.out, "WARN: label change on #7 failed: --remove-label tehas:running --add-label tehas:awaiting-merge");
    has(&w.gh(), "Pull request waiting for a human");
}

#[test]
fn a_run_that_ended_as_st_is_not_said_to_wait_for_a_merge_though_a_pull_request_is_open() {
    for st in ["failed", "aborted", "unknown", "running"] {
        let w = World::new("st-not-merge");
        w.ended(st, "review", "feature/x", "");
        pr9(&w);
        let r = w.run7();
        assert_eq!(r.rc, 0, "{st}");
        let gh = w.gh();
        lacks(&gh, "tehas:awaiting-merge");
        has(
            &gh,
            "--remove-label tehas:running --add-label needs-human-attention",
        );
        has(
            &gh,
            "Its branch has an open pull request: https://github.com/acme/widget/pull/9",
        );
        has(
            &gh,
            &format!("Tehas cycle stopped with pull request #9 open: status `{st}`"),
        );
        lacks(&gh, "without a pull request");
    }
}

#[test]
fn a_stop_at_commit_pr_with_its_draft_pull_request_open_names_it_as_a_draft() {
    let w = World::new("st-draft");
    w.ended("running", "commit-pr", "feature/x", "");
    w.fx("pr.txt", "https://github.com/acme/widget/pull/67\n");
    w.touch("pr-draft");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    let gh = w.gh();
    has(&gh, "pr list --repo acme/widget --head feature/x --author tehas-bot[bot] --state all --json url,state,body,headRefName,isDraft,createdAt");
    has(&gh, "Tehas cycle stopped with pull request #67 open (draft): status `running`, last step `commit-pr`");
    has(
        &gh,
        "--remove-label tehas:running --add-label needs-human-attention",
    );
    lacks(&gh, "without a pull request");
}

#[test]
fn a_stop_at_commit_pr_with_no_pull_request_says_it_has_none() {
    let w = World::new("st-nopr");
    w.ended("running", "commit-pr", "feature/x", "");
    w.fx("pr.json", "[]\n");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    has(
        &w.gh(),
        "Tehas cycle stopped without a pull request: status `running`, last step `commit-pr`",
    );
    lacks(&w.gh(), "open pull request");
}

fn ended_at(w: &World) -> bool {
    !w.row(&["cycle-show", "7"])["cycle"]["ended_at"].is_null()
}

/// The kinds and states of the journal's open actions.
fn journal_open(w: &World) -> Value {
    let open = w.store_ok(&["journal-open"]);
    let rows: Vec<Value> = open["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| json!([a["kind"], a["state"]]))
        .collect();
    json!(rows)
}

const COMMENT: &str = "issue comment 7 ";
const READ: &str = "api repos/acme/widget/issues/7/comments?per_page=100";

#[test]
fn a_comment_that_github_refuses_once_is_posted_on_the_second_try() {
    let w = World::new("st-comment-once");
    w.ended("handoff", "commit-pr", "feature/x", "");
    pr9(&w);
    w.fx("comment-fails-times", "1\n");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert_eq!(count_lines(&w.gh(), COMMENT), 2);
    lacks(&r.out, "WARN: comment on #7 failed");
    has(&w.gh(), "Pull request waiting for a human");
    assert!(ended_at(&w));
}

#[test]
fn after_three_refusals_the_comment_is_said_once_and_the_cycle_still_ends_its_record() {
    let w = World::new("st-comment-three");
    w.ended("handoff", "commit-pr", "feature/x", "");
    pr9(&w);
    w.touch("comment-fails");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert_eq!(count_lines(&w.gh(), COMMENT), 3);
    assert_eq!(count_in(&r.out, "WARN: comment on #7 failed"), 1);
    assert!(ended_at(&w));
}

#[test]
fn a_comment_github_answered_with_a_5xx_and_did_not_take_is_posted_again_after_the_read() {
    let w = World::new("st-5xx-not-taken");
    w.ended("handoff", "commit-pr", "feature/x", "");
    pr9(&w);
    w.fx("comment-fails-times", "1\n");
    w.fx("comment-says", "HTTP 502: Bad Gateway\n");
    w.fx("comments.json", "[]\n");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert_eq!(count_lines(&w.gh(), COMMENT), 2);
    assert_eq!(count_lines(&w.gh(), READ), 1);
    lacks(&r.out, "may have been posted");
    lacks(&r.out, "WARN: comment on #7 failed");
    // The unknown row stays for the poll, which finds the comment by its text.
    assert_eq!(journal_open(&w), json!([["comment-post", "unknown"]]));
}

#[test]
fn a_comment_github_answered_with_a_5xx_and_took_is_not_posted_again() {
    let w = World::new("st-5xx-taken");
    w.ended("handoff", "commit-pr", "feature/x", "");
    pr9(&w);
    w.fx("comment-fails", "HTTP 502: Bad Gateway\n");
    w.touch("comment-taken");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert_eq!(count_lines(&w.gh(), COMMENT), 1);
    assert_eq!(count_lines(&w.gh(), READ), 1);
    lacks(&r.out, "may have been posted");
    lacks(&r.out, "WARN: comment on #7 failed");
}

#[test]
fn a_comment_github_answered_with_a_5xx_that_no_read_settles_is_not_posted_again() {
    let w = World::new("st-5xx-unsettled");
    w.ended("handoff", "commit-pr", "feature/x", "");
    pr9(&w);
    w.fx("comment-fails", "HTTP 502: Bad Gateway\n");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert_eq!(count_lines(&w.gh(), COMMENT), 1);
    has(&r.out, "may have been posted");
    assert_eq!(journal_open(&w), json!([["comment-post", "unknown"]]));
    assert!(ended_at(&w));
}

#[test]
fn a_list_of_pull_requests_that_github_refuses_once_is_asked_for_again() {
    let w = World::new("st-list-once");
    w.ended("handoff", "commit-pr", "feature/x", "");
    pr9(&w);
    w.fx("pr-fails-times", "1\n");
    w.run7();
    assert_eq!(count_lines(&w.gh(), "pr list "), 2);
    has(
        &w.gh(),
        "--remove-label tehas:running --add-label tehas:awaiting-merge",
    );
}

/// A handoff on feature/x whose pull requests GitHub would not list.
fn unlisted(w: &World) -> Run {
    w.ended("handoff", "commit-pr", "feature/x", "");
    pr9(w);
    w.fx("pr-fails-times", "3\n");
    w.run7()
}

#[test]
fn after_three_refusals_the_cycle_is_parked_as_one_whose_pull_request_could_not_be_looked_up() {
    let w = World::new("st-list-three");
    let r = unlisted(&w);
    let gh = w.gh();
    assert_eq!(count_lines(&gh, "pr list "), 3);
    has(
        &r.out,
        "WARN: the pull requests of feature/x could not be read",
    );
    has(
        &gh,
        "--remove-label tehas:running --add-label needs-human-attention",
    );
    has(&gh, "its pull request could not be looked up: GitHub did not list the pull requests of the branch `feature/x`");
    lacks(&gh, "stopped without a pull request");
    lacks(&gh, "tehas:awaiting-merge");
}

// The two cases after it go on from where the one above left the host.
#[test]
fn the_branch_whose_pull_request_could_not_be_looked_up_stays_on_record() {
    let w = World::new("st-branch-kept");
    unlisted(&w);
    assert_eq!(w.row(&["cycle-show", "7"])["cycle"]["branch"], "feature/x");
}

#[test]
fn the_branch_is_on_record_with_every_end_and_an_earlier_cycles_is_not_the_next_ones() {
    let w = World::new("st-branch-each");
    unlisted(&w);
    w.ended("handoff", "commit-pr", "feature/y", "");
    w.run7();
    let c = w.row(&["cycle-show", "7"])["cycle"].clone();
    assert_eq!(json!([c["attempt"], c["branch"]]), json!([2, "feature/y"]));
    has(&w.gh(), "Pull request waiting for a human");
    fs::remove_file(w.fx.join("result.json")).unwrap();
    w.run7();
    let c = w.row(&["cycle-show", "7"])["cycle"].clone();
    assert_eq!(json!([c["attempt"], c["branch"]]), json!([3, null]));
}

#[test]
fn a_branch_name_the_container_wrote_is_reduced_before_it_is_recorded() {
    let w = World::new("st-branch-reduced");
    w.ended("handoff", "commit-pr", "feature/x`;@y$z", "");
    w.fx("pr-fails-times", "3\n");
    w.run7();
    assert_eq!(
        w.row(&["cycle-show", "7"])["cycle"]["branch"],
        "feature/xyz"
    );
    has(&w.gh(), "the branch `feature/xyz`");
}

#[test]
fn when_the_label_cannot_be_changed_the_comment_says_so() {
    let w = World::new("st-label-said");
    w.ended("handoff", "commit-pr", "feature/x", "");
    pr9(&w);
    w.fx("edit-fails-on", "--remove-label tehas:running\n");
    w.run7();
    has(&w.gh(), "Pull request waiting for a human: https://github.com/acme/widget/pull/9 The label of this issue could not be changed");
}

#[test]
fn and_a_comment_after_a_label_change_that_worked_does_not() {
    let w = World::new("st-label-fine");
    w.ended("handoff", "commit-pr", "feature/x", "");
    pr9(&w);
    w.run7();
    lacks(&w.gh(), "could not be changed");
}

#[test]
fn the_question_label_is_tried_three_times() {
    let w = World::new("st-question");
    w.fx("edit-fails-on", "--add-label tehas:question");
    w.run7();
    let gh = w.gh();
    let tries = gh
        .lines()
        .filter(|l| *l == "issue edit 7 --repo acme/widget --add-label tehas:question")
        .count();
    assert_eq!(tries, 3);
    has(&gh, "The boxes above cannot be used");
}

#[test]
fn a_cycle_that_parks_takes_the_build_output_of_its_kept_workspace_as_it_ends() {
    let w = World::new("st-clean");
    w.ended("failed", "develop", "", "");
    w.run7();
    has(
        &w.calls("fake-console"),
        &format!("clean ended {} target node_modules", w.workdir().display()),
    );
}

#[test]
fn and_a_failure_to_do_so_is_a_warning() {
    let w = World::new("st-clean-fails");
    w.ended("failed", "develop", "", "");
    w.stub("fake-console", "exit 1");
    let r = w.run7();
    has(&r.out, "was not removed");
}
