//! The runner's output, a handoff with or without a pull request, the park
//! comment and the usage posted.

mod common;

use common::{count_lines, has, lacks, World};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

const PR9: &str = "https://github.com/acme/widget/pull/9";

fn pr9(w: &World) {
    w.fx("pr.txt", &format!("{PR9}\n"));
}

/// The lines of `text` that hold `needle`.
fn count_in(text: &str, needle: &str) -> usize {
    text.lines().filter(|l| l.contains(needle)).count()
}

/// A path beside the workspace: `<workdir><ext>`.
fn beside(w: &World, ext: &str) -> PathBuf {
    PathBuf::from(format!("{}{ext}", w.workdir().display()))
}

/// The workspace's directory is one tehas work marked as its own.
fn mark_workspaces(w: &World) {
    let d = w.home.join("work");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join(".tehas-workspaces"), "").unwrap();
}

#[test]
fn the_runners_output_is_in_the_issues_log() {
    let w = World::new("ho-log");
    w.work(r#"echo "from the runner""#);
    w.run7();
    let log = fs::read_to_string(w.home.join("logs/dispatch-work-7.log")).unwrap();
    assert_eq!(log.trim_end(), "from the runner");
}

#[test]
fn a_cycle_that_ends_without_a_pull_request_is_handed_to_a_human() {
    let w = World::new("ho-nopr");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    has(&w.gh(), "issue edit 7 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention");
    has(&w.gh(), "Tehas cycle stopped without a pull request");
    has(&r.out, "done #7 rc=0 status=unknown");
    assert_eq!(w.calls("fake-status").lines().count(), 1);
}

#[test]
fn the_runners_exit_status_is_in_the_report() {
    let w = World::new("ho-exit");
    w.work("exit 3");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    has(&w.gh(), "(runner exit 3)");
}

#[test]
fn a_runner_that_started_no_cycle_exit_4_is_parked_without_naming_a_cause_it_cannot_know() {
    let w = World::new("ho-exit4");
    w.work("exit 4");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    let gh = w.gh();
    has(&gh, "Tehas started no cycle: tehas work refused the start before its runner ran (runner exit 4). It refuses one, for example, when the issue changed after its approval, when the approval or the issue cannot be read, or when an image or file the issue links cannot be given to the cycle.");
    lacks(&gh, "stopped without a pull request");
    has(&gh, "issue edit 7 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention");
    // Exit 4 has many causes and only tehas work's own comment names which:
    // an edited body is not said to be an image that could not be given.
    lacks(&gh, "could not be given to it");
    has(&r.out, "#7: tehas work refused the start before its runner ran (exit 4): the comment tehas work posted on the issue says why");
    let live: Value =
        serde_json::from_str(&fs::read_to_string(w.home.join("live/running.json")).unwrap())
            .unwrap();
    assert_eq!(
        live["last"]["statusText"],
        "the comment tehas work posted on the issue says why"
    );
}

#[test]
fn a_cycle_that_never_ran_is_reported_from_the_approval_record_and_the_record_goes_with_the_report()
{
    let mut w = World::new("ho-never");
    w.env("TEHAS_CYCLE_FROM", "unit");
    // 2026-01-01T00:00:00Z
    w.approve("alice", "label", 1_767_225_600, &[]);
    w.work("exit 4");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    let gh = w.gh();
    has(&gh, "alice approved this start at 2026-01-01T00:00:00Z");
    has(
        &gh,
        "Nothing of it ran: no workspace, no container, no step, no usage.",
    );
    lacks(&gh, "last step");
    lacks(&gh, "min,");
    lacks(&gh, "total, all dispatches");
    has(&gh, "- [ ] Retry with the issue text only");
    // The approval goes with the report: no later start, the fixer's or a
    // refresh, rests on an approval that started nothing.
    let a = w.row(&["approval-current", "7"]);
    assert_eq!(
        json!([a["approval"], a["current"], a["rests_on"]]),
        json!([null, null, null])
    );
    let p = w.park();
    assert_eq!(
        json!([p["reason"], p["automatic"]]),
        json!(["unknown", false])
    );
    has(&gh, "--add-label tehas:question");
    // The line of history still names how the start was authorised.
    let h = w.history0();
    assert_eq!(
        json!([h["rc"], h["status"], h["step"], h["how"], h["by"]]),
        json!([4, "refused", "-", "label", "alice"])
    );
}

#[test]
fn a_cycle_that_never_ran_and_holds_no_approval_record_says_that_and_asks_for_a_fresh_one() {
    let w = World::new("ho-noappr");
    w.work("exit 4");
    w.run7();
    has(
        &w.gh(),
        "No approval of this start is recorded on this host: a fresh `tehas:ready` is needed.",
    );
    has(&w.gh(), "runner exit 4");
}

#[test]
fn the_issues_images_are_gone_when_the_cycle_is_over_however_it_ended() {
    for (status, step) in [("handoff", "commit-pr"), ("aborted", "develop")] {
        let w = World::new("ho-images");
        w.ended(status, step, "feature/x", "");
        mark_workspaces(&w);
        fs::create_dir_all(beside(&w, ".images")).unwrap();
        pr9(&w);
        w.run7();
        assert!(!beside(&w, ".images").exists(), "{status}");
    }
}

#[test]
fn an_images_directory_under_a_directory_tehas_work_did_not_mark_is_left_alone() {
    let w = World::new("ho-images-unmarked");
    w.ended("merged", "done", "feature/x", "");
    w.merged_pr();
    fs::create_dir_all(beside(&w, ".images")).unwrap();
    w.run7();
    assert!(beside(&w, ".images").exists());
}

#[test]
fn the_cycles_own_state_directory_is_gone_when_the_cycle_is_over_however_it_ended() {
    for (status, step) in [("handoff", "commit-pr"), ("aborted", "develop")] {
        let w = World::new("ho-state");
        w.ended(status, step, "feature/x", "");
        mark_workspaces(&w);
        let st = beside(&w, ".state");
        fs::create_dir_all(st.join("cache")).unwrap();
        fs::create_dir_all(st.join("memory")).unwrap();
        fs::write(st.join("memory/note.json"), "planted\n").unwrap();
        pr9(&w);
        w.run7();
        assert!(!st.exists(), "{status}");
    }
}

#[test]
fn a_state_directory_under_a_directory_tehas_work_did_not_mark_is_left_alone() {
    let w = World::new("ho-state-unmarked");
    w.ended("merged", "done", "feature/x", "");
    fs::create_dir_all(beside(&w, ".state/cache")).unwrap();
    w.run7();
    assert!(beside(&w, ".state").exists());
}

#[test]
fn a_cycle_that_ends_with_a_pull_request_waits_for_a_human_and_is_reported_on_it() {
    let w = World::new("ho-pr");
    w.ended("handoff", "commit-pr", "feature/x", "");
    pr9(&w);
    let r = w.run7();
    assert_eq!(r.rc, 0);
    let gh = w.gh();
    has(&gh, "issue edit 7 --repo acme/widget --remove-label tehas:running --add-label tehas:awaiting-merge");
    has(&gh, "pr comment https://github.com/acme/widget/pull/9");
    has(
        &gh,
        "pr list --repo acme/widget --head feature/x --author tehas-bot[bot]",
    );
    has(&gh, "No token usage was recorded for this cycle.");
}

// #33: only a pull request that waits for nothing but a merge is labelled so.
#[test]
fn a_handoff_with_its_pull_request_open_on_another_cap_is_parked_and_names_the_cap_and_the_pull_request(
) {
    let w = World::new("ho-othercap");
    w.ended("handoff", "lens-review", "feature/x", "review-incomplete");
    pr9(&w);
    let r = w.run7();
    assert_eq!(r.rc, 0);
    let gh = w.gh();
    lacks(&gh, "tehas:awaiting-merge");
    has(
        &gh,
        "--remove-label tehas:running --add-label needs-human-attention",
    );
    has(&gh, "Tehas cycle handed off with its pull request open");
    has(&gh, "pi-rukas stopped on the cap `review-incomplete`.");
    has(
        &gh,
        "Read the pull request before you retry: https://github.com/acme/widget/pull/9",
    );
    has(&gh, "- [ ] Retry with the issue text only");
    let p = w.park();
    assert_eq!(
        json!([p["reason"], p["pr"]]),
        json!(["review-incomplete", 9])
    );
}

#[test]
fn a_handoff_with_its_pull_request_open_and_no_cap_is_parked() {
    let w = World::new("ho-nocap");
    w.ended("handoff", "commit-pr", "feature/x", "-");
    pr9(&w);
    w.run7();
    lacks(&w.gh(), "tehas:awaiting-merge");
    has(&w.gh(), "The result names no cap, so it is not taken for a pull request that waits only for a merge.");
}

#[test]
fn a_review_whose_round_cap_left_findings_open_is_parked_though_its_cap_is_a_waiting_merge() {
    let w = World::new("ho-held");
    w.ended("handoff", "ci", "feature/x", "");
    w.progress(json!({"roles": [], "reason": "", "reviewHeld": true}));
    pr9(&w);
    w.run7();
    lacks(&w.gh(), "tehas:awaiting-merge");
    has(
        &w.gh(),
        "Its review ran out of rounds with findings still open",
    );
}

#[test]
fn a_waiting_merge_with_no_progress_to_read_is_parked() {
    let w = World::new("ho-noprog");
    w.ended("handoff", "ci", "feature/x", "");
    w.touch("no-progress");
    pr9(&w);
    w.run7();
    lacks(&w.gh(), "tehas:awaiting-merge");
    has(&w.gh(), "--add-label needs-human-attention");
}

#[test]
fn an_issue_that_carries_needs_human_attention_at_the_end_is_not_also_labelled_awaiting_merge() {
    let w = World::new("ho-nha");
    w.ended("handoff", "ci", "feature/x", "");
    pr9(&w);
    w.fx(
        "issue-7.json",
        r#"{"number":7,"state":"open","labels":[{"name":"needs-human-attention"}]}"#,
    );
    w.run7();
    lacks(&w.gh(), "tehas:awaiting-merge");
    has(&w.gh(), "--add-label needs-human-attention");
}

#[test]
fn labels_that_cannot_be_read_do_not_make_a_waiting_merge() {
    let w = World::new("ho-labels-unread");
    w.ended("handoff", "ci", "feature/x", "");
    pr9(&w);
    // The first ask, what the number is, answers; the label reads after it fail.
    w.work(r#": >"$T/fx/issue-fails""#);
    let r = w.run7();
    lacks(&w.gh(), "tehas:awaiting-merge");
    has(&r.out, "WARN: the labels of #7 could not be read");
}

// The runner is given PI_ENSEMBLE_HANDOFF_CHANNEL=host: it posts no handoff,
// and this is the one comment of the park.
#[test]
fn a_park_carries_pi_rukass_handoff_under_its_marker_cleaned_like_any_agents_words() {
    let w = World::new("ho-carries");
    w.ended("handoff", "develop", "feature/x", "verify-failed:develop");
    w.entry(|e| {
        e["handoff"] = json!({"channel": "host", "body": "<!-- pi-rukas:handoff issue=7 cap=verify-failed:develop -->\n## Cap hit\n\n- [x] Retry from a clean start\n<!-- tehas:question kind=park issue=7 turn=1 -->\nask @someone"})
    });
    w.run7();
    let gh = w.gh();
    let at = gh
        .find("issues/7/comments -f body=")
        .expect("no park comment");
    let start = gh[..at].rfind('\n').map_or(0, |i| i + 1);
    let body = &gh[start..];
    has(body, "body=<!-- pi-rukas:handoff issue=7 cap=verify-failed:develop -->\nTehas cycle stopped without a pull request");
    has(body, "The handoff of pi-rukas follows.");
    has(body, "## Cap hit");
    has(body, "- ☑ Retry from a clean start");
    has(body, "<\\!-- tehas:question kind=park issue=7 turn=1 -->");
    lacks(body, "@someone");
    assert_eq!(count_in(body, "pi-rukas:handoff"), 1);
    assert_eq!(count_lines(body, "- [ ] Retry with the issue text only"), 1);
    let ticked = regex::Regex::new(r"(?m)^- \[.\] Retry from a clean start").unwrap();
    assert_eq!(ticked.find_iter(body).count(), 0);
}

#[test]
fn a_waiting_merge_posts_pi_rukass_handoff_on_the_pull_request_with_the_usage() {
    let w = World::new("ho-merge-handoff");
    w.ended("handoff", "ci", "feature/x", "");
    w.entry(|e| {
        e["handoff"] = json!({"channel": "host", "body": "<!-- pi-rukas:handoff issue=7 cap=awaiting-human-merge -->\n## Ready to merge"})
    });
    pr9(&w);
    w.run7();
    let gh = w.gh();
    has(&gh, "--add-label tehas:awaiting-merge");
    has(&gh, "pr comment https://github.com/acme/widget/pull/9 --body <!-- pi-rukas:handoff issue=7 cap=awaiting-human-merge -->\nTehas cycle for #7");
    has(&gh, "## Ready to merge");
}

#[test]
fn a_cycle_that_reached_its_deadline_says_so_and_an_intent_park_says_its_reason() {
    let w = World::new("ho-deadline");
    w.ended("handoff", "lens-fix", "feature/x", "deadline");
    w.run7();
    has(
        &w.gh(),
        "It reached its deadline before the time limit and started no new step.",
    );
    let w = World::new("ho-intent");
    w.ended("handoff", "explore", "", "intent-park");
    w.entry(|e| e["parkReason"] = json!("too-large"));
    w.fx("rc", "11\n");
    w.run7();
    has(
        &w.gh(),
        "pi-rukas stopped on the cap `intent-park` (`too-large`).",
    );
}

#[test]
fn each_end_of_a_cycle_leaves_a_line_of_history_for_the_fixer_a_park_and_its_reason_a_waiting_merge(
) {
    let mut w = World::new("ho-history");
    w.env("TEHAS_CYCLE_FROM", "unit");
    w.approve("alice", "label", 1_760_000_000, &[]);
    w.ended("handoff", "explore", "", "intent-park");
    w.entry(|e| e["parkReason"] = json!("too-large"));
    w.fx("rc", "11\n");
    w.run7();
    let h = w.history0();
    assert_eq!(
        json!([
            h["parked"],
            h["status"],
            h["step"],
            h["cap"],
            h["park_reason"],
            h["provider"],
            h["by"],
            h["how"],
            h["rc"]
        ]),
        json!([
            true,
            "handoff",
            "explore",
            "intent-park",
            "too-large",
            false,
            "alice",
            "label",
            11
        ])
    );
    w.approve("alice", "label", 1_760_000_100, &[]);
    w.ended("handoff", "commit-pr", "feature/x", "");
    pr9(&w);
    w.run7();
    let rows = w.row(&["history-show", "7"])["rows"].clone();
    assert_eq!(rows.as_array().unwrap().len(), 2);
    let r1 = &rows[1];
    assert_eq!(
        json!([
            r1["parked"],
            r1["pr"],
            r1["at"].as_u64().unwrap() > 1_760_000_000
        ]),
        json!([false, 9, true])
    );
}

#[test]
fn the_token_usage_per_role_is_posted_on_the_pull_request_with_the_results_totals() {
    let w = World::new("ho-usage");
    w.ended("handoff", "commit-pr", "feature/x", "");
    w.entry(|e| {
        e["usage"] = json!({"turns": 4, "input": 35, "output": 11, "cacheRead": 1, "dispatches": 3})
    });
    w.progress(json!({"roles": [{"role": "developer", "dispatches": 2, "turns": 3, "input": 30, "output": 10, "cacheRead": 1}], "reason": "", "reviewHeld": false}));
    pr9(&w);
    w.run7();
    has(&w.gh(), "| developer | 2 | 3 | 30 | 10 | 1 |");
    has(
        &w.gh(),
        "| **total, all dispatches** | 3 | 4 | 35 | 11 | 1 |",
    );
}

/// A refresh's approval record, as the dispatcher writes it before the start.
fn refresh_started(w: &mut World) {
    w.env("TEHAS_CYCLE_FROM", "unit");
    let head = "a".repeat(40);
    w.approve(
        "alice",
        "refresh",
        1_760_000_000,
        &["--refresh-pr", "9", "--refresh-head", &head],
    );
}

/// The call that takes the mark of a refresh off its pull request: the REST
/// issues endpoint, since gh pr edit asks GitHub a GraphQL question the bot's
/// token may not answer (issue #120).
const REFRESH_UNMARK: &str = "api -X DELETE repos/acme/widget/issues/9/labels/tehas:refreshing";

#[test]
fn a_refresh_takes_tehas_refreshing_off_its_pull_request_when_it_ends() {
    for outcome in ["pushed", "parked", "failed"] {
        let mut w = World::new("ho-refresh");
        refresh_started(&mut w);
        match outcome {
            "pushed" => {
                w.ended("handoff", "commit-pr", "feature/issue-7-x", "");
                pr9(&w);
            }
            "parked" => w.ended(
                "handoff",
                "develop",
                "feature/issue-7-x",
                "verify-failed:develop",
            ),
            _ => w.ended("crash", "develop", "", ""),
        }
        let r = w.run7();
        assert_eq!(r.rc, 0, "{outcome}");
        let gh = w.gh();
        assert_eq!(count_lines(&gh, REFRESH_UNMARK), 1, "{outcome}");
        lacks(&r.out, "WARN: label change on pull request #9");
        lacks(&gh, "pr edit");
        if outcome == "pushed" {
            has(
                &gh,
                "--remove-label tehas:running --add-label tehas:awaiting-merge",
            );
        } else {
            has(&gh, "--add-label needs-human-attention");
        }
        assert_eq!(w.history0()["how"], "refresh", "{outcome}");
    }
}

#[test]
fn a_refresh_that_pushed_to_an_older_open_pull_request_awaits_its_merge() {
    // The pull request a refresh pushes to was opened by an earlier cycle, so
    // its createdAt is before this cycle's start; the bound of #1747 does not
    // apply to it.
    let mut w = World::new("ho-refresh-older");
    refresh_started(&mut w);
    w.ended("handoff", "commit-pr", "feature/issue-7-x", "");
    let prs = json!([{"url": PR9, "state": "OPEN", "body": "Fixes #7",
        "headRefName": "feature/issue-7-x", "isDraft": false, "createdAt": "2020-01-01T00:00:00Z"}]);
    w.fx("pr.json", &prs.to_string());
    let r = w.run7();
    assert_eq!(r.rc, 0);
    has(
        &w.gh(),
        "--remove-label tehas:running --add-label tehas:awaiting-merge",
    );
    assert_eq!(count_lines(&w.gh(), REFRESH_UNMARK), 1);
    lacks(&r.out, "WARN: label change on pull request #9");
}

#[test]
fn a_cycle_that_is_no_refresh_leaves_the_labels_of_pull_requests_alone() {
    let w = World::new("ho-norefresh");
    w.ended("handoff", "commit-pr", "feature/x", "");
    pr9(&w);
    w.run7();
    lacks(&w.gh(), "tehas:refreshing");
}

#[test]
fn a_mark_that_cannot_be_taken_off_is_said_after_the_three_tries_not_taken_as_gone() {
    let mut w = World::new("ho-unmark-fails");
    refresh_started(&mut w);
    w.touch("unmark-fails");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert_eq!(count_lines(&w.gh(), REFRESH_UNMARK), 3);
    has(&r.out, "WARN: label change on pull request #9 failed");
}

#[test]
fn a_404_from_a_mark_github_has_not_got_counts_as_the_mark_gone_and_is_not_tried_again() {
    // A 404 says the label is not on the pull request, which is what the
    // cycle wants (host/docs/operating.md). What the dispatcher does on a 404
    // of its own is a case in test/dispatch_refresh_test.sh.
    let mut w = World::new("ho-404");
    refresh_started(&mut w);
    w.touch("label-missing");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert_eq!(count_lines(&w.gh(), REFRESH_UNMARK), 1);
    lacks(&r.out, "WARN: label change on pull request #9");
}
