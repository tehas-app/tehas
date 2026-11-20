//! A cycle whose runner died and wrote no result. Its step comes from the
//! step-started lines of its run log, its pull request from GitHub, and the
//! report is a handoff whose review did not finish, not a park that offers a
//! clean retry. The runner's own status is named, not tehas work's.

mod common;

use common::{has, lacks, World};
use serde_json::{json, Value};
use std::fs;
use std::time::{Duration, SystemTime};

/// The branch of #7 as pi-rukas names it: the step right after commit-pr in
/// the log of a run that reached it.
const BRANCH: &str = "feature/issue-7-install-sh-checks-instance-env";

/// The steps of a run that died after its push.
fn died_pushed(w: &World, more: &[&str]) {
    let mut words = vec!["explore", "plan", "develop", "commit-pr", BRANCH];
    words.extend_from_slice(more);
    w.died(&words);
}

fn pr_list() -> String {
    format!("pr list --repo acme/widget --head {BRANCH} --author tehas-bot[bot] --state all")
}

/// The name tehas work gives a log started `ago` before now.
fn log_name(ago: Duration) -> String {
    let at = SystemTime::now() - ago;
    let secs = at.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
    let out = std::process::Command::new("date")
        .args(["-u", "-d", &format!("@{secs}"), "+%y%m%d-%H%M%S"])
        .output()
        .unwrap();
    format!(
        "work-7-{}.jsonl",
        String::from_utf8(out.stdout).unwrap().trim()
    )
}

#[test]
fn a_runner_that_died_after_commit_pr_is_reported_with_its_pull_request_not_as_a_park_with_a_clean_retry(
) {
    let w = World::new("crash-pushed");
    died_pushed(&w, &[]);
    w.open_pr("https://github.com/acme/widget/pull/347");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    let gh = w.gh();
    has(&gh, &pr_list());
    has(
        &gh,
        "Tehas cycle handed off with its pull request open and its review unfinished",
    );
    has(&gh, &format!("last step `{BRANCH}` read from its run log"));
    has(&gh, "https://github.com/acme/widget/pull/347");
    has(
        &gh,
        "--remove-label tehas:running --add-label needs-human-attention",
    );
    // Not a park's question: no boxes, and nothing on record for the poll.
    lacks(&gh, "Retry with the issue text only");
    lacks(&gh, "tehas:question");
    lacks(&gh, "stopped without a pull request");
    assert!(w.park()["question"].is_null());
    assert_eq!(w.park(), Value::Null);
}

#[test]
fn the_runners_own_status_is_named_and_the_polls_line_keeps_the_status_of_tehas_work() {
    let w = World::new("crash-status");
    died_pushed(&w, &["rc=134"]);
    w.open_pr("");
    let r = w.run7();
    has(&w.gh(), "(runner exit 134)");
    has(&r.out, "done #7 rc=2 ");
}

#[test]
fn with_no_pull_request_on_the_branch_the_cycle_is_parked_as_any_other_that_left_none() {
    let w = World::new("crash-nopr");
    died_pushed(&w, &[]);
    w.fx("pr.json", "[]\n");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    let gh = w.gh();
    has(&gh, &pr_list());
    has(&gh, "Tehas cycle stopped without a pull request");
    has(&gh, "- [ ] Retry with the issue text only");
    has(
        &gh,
        "--remove-label tehas:running --add-label needs-human-attention",
    );
    lacks(&gh, "handed off with its pull request open");
}

#[test]
fn a_merged_pull_request_of_the_bot_on_that_branch_ends_the_cycle_as_merged_not_as_a_handoff() {
    let w = World::new("crash-merged");
    died_pushed(&w, &[]);
    w.merged_pr();
    w.run7();
    let gh = w.gh();
    has(&gh, "Tehas cycle ended: its pull request https://github.com/acme/widget/pull/9 was merged while the cycle ran");
    lacks(&gh, "review unfinished");
}

#[test]
fn a_closed_pull_request_of_the_bot_on_that_branch_is_said_as_one_that_was_closed() {
    let w = World::new("crash-closed");
    died_pushed(&w, &[]);
    w.fx("pr.txt", "https://github.com/acme/widget/pull/347\n");
    w.fx("pr-state", "CLOSED\n");
    w.run7();
    let gh = w.gh();
    has(&gh, "was closed without being merged");
    lacks(&gh, "review unfinished");
}

#[test]
fn a_runner_that_died_before_commit_pr_asks_github_about_no_branch_and_is_parked() {
    let w = World::new("crash-early");
    w.died(&["explore", "plan", "develop"]);
    let r = w.run7();
    assert_eq!(r.rc, 0);
    let gh = w.gh();
    lacks(&gh, "pr list --repo");
    has(&gh, "Tehas cycle stopped without a pull request");
    has(&gh, "last step `develop` read from its run log");
}

#[test]
fn a_step_after_commit_pr_that_is_no_branch_of_this_issue_asks_github_nothing() {
    let w = World::new("crash-nobranch");
    // A step name of any other shape than a branch of this issue is a step.
    w.died(&["explore", "plan", "develop", "commit-pr", "lens-review"]);
    w.run7();
    let gh = w.gh();
    lacks(&gh, "pr list --repo");
    has(&gh, "Tehas cycle stopped without a pull request");
}

#[test]
fn a_pull_request_of_the_bot_that_names_another_issue_is_said_not_taken_as_this_cycles() {
    let w = World::new("crash-other");
    died_pushed(&w, &[]);
    // A branch other than the one asked for, and a body that names another
    // issue: the bot's, on this branch, but of #8.
    w.fx("pr-body-issue", "8\n");
    w.fx(
        "pr.json",
        &json!([{"url": "https://github.com/acme/widget/pull/347", "state": "OPEN",
            "body": "Work on another thing", "headRefName": "feature/issue-8-another-branch"}])
        .to_string(),
    );
    w.run7();
    let gh = w.gh();
    has(&gh, "does not name this issue");
    has(&gh, "Tehas cycle stopped without a pull request");
    lacks(&gh, "review unfinished");
}

#[test]
fn a_run_whose_steps_stop_at_commit_pr_names_no_branch() {
    let w = World::new("crash-atpush");
    // The push is the step after commit-pr: nothing was pushed.
    w.died(&["explore", "plan", "develop", "commit-pr"]);
    w.open_pr("");
    w.run7();
    let gh = w.gh();
    lacks(&gh, "pr list --repo");
    has(&gh, "Tehas cycle stopped without a pull request");
}

#[test]
fn github_refusing_the_list_is_said_as_the_unknown_it_is_with_the_branch_on_record() {
    let w = World::new("crash-refused");
    died_pushed(&w, &[]);
    w.fx("pr-fails-times", "3\n");
    let r = w.run7();
    has(
        &r.out,
        &format!("WARN: the pull requests of {BRANCH} could not be read"),
    );
    has(&w.gh(), "its pull request could not be looked up");
    assert_eq!(w.row(&["cycle-show", "7"])["cycle"]["branch"], BRANCH);
}

#[test]
fn the_step_of_a_cycle_with_no_log_at_all_stays_a_dash() {
    let w = World::new("crash-nolog");
    w.died(&[]);
    let r = w.run7();
    has(&r.out, "done #7 rc=2 status=unknown step=-");
}

#[test]
fn a_run_whose_tehas_work_wrote_nothing_at_all_is_read_as_a_cycle_that_started_no_container() {
    let w = World::new("crash-nothing");
    died_pushed(&w, &[]);
    // No runner_exit and no progress: tehas work stopped before it watched a
    // container, so the log of an earlier run is not read for this one.
    w.work(
        r#"mkdir -p "$TEHAS_HOME/logs"
: >"$TEHAS_HOME/logs/work-7-261017-012100.jsonl"
printf '{"v":1,"kind":"step-started","issue":7,"event":{"step":"lied-step"}}\n' \
  >>"$TEHAS_HOME/logs/work-7-261017-012100.jsonl"
exit 2"#,
    );
    w.open_pr("");
    w.run7();
    let gh = w.gh();
    lacks(&gh, "lied-step");
    lacks(&gh, "read from its run log");
    has(&gh, "Tehas cycle stopped without a pull request");
    lacks(&gh, "pr list --repo");
}

#[test]
fn a_result_that_was_written_names_the_step_nothing_of_a_log_is_read_for_it() {
    let w = World::new("crash-result");
    w.ended(
        "handoff",
        "commit-pr",
        "feature/issue-7-x",
        "review-incomplete",
    );
    w.fx("pr.txt", "https://github.com/acme/widget/pull/9\n");
    w.work(
        r#"echo "{\"v\":1,\"kind\":\"step-started\",\"event\":{\"step\":\"lied-step\"}}" >"$TEHAS_HOME/logs/work-7-261018-012100.jsonl"; exit 2"#,
    );
    w.run7();
    let gh = w.gh();
    lacks(&gh, "lied-step");
    lacks(&gh, "read from its run log");
}

#[test]
fn a_runner_killed_after_commit_pr_is_reported_with_its_pull_request_as_any_other_that_died() {
    let w = World::new("crash-killed-late");
    died_pushed(&w, &["rc=137", "status=killed"]);
    w.open_pr("");
    w.run7();
    let gh = w.gh();
    has(
        &gh,
        "Tehas cycle handed off with its pull request open and its review unfinished",
    );
    has(&gh, "killed");
    lacks(&gh, "Retry with the issue text only");
}

#[test]
fn a_runner_killed_before_commit_pr_is_reported_as_killed_and_parked() {
    let w = World::new("crash-killed-early");
    w.died(&["explore", "plan", "develop", "rc=137", "status=killed"]);
    w.open_pr("");
    let r = w.run7();
    has(&r.out, "status=killed");
    has(&w.gh(), "Tehas cycle stopped without a pull request");
}

#[test]
fn the_line_of_history_of_a_runner_that_died_names_the_step_read_from_its_log_and_its_pull_request()
{
    let w = World::new("crash-history");
    died_pushed(&w, &[]);
    w.open_pr("https://github.com/acme/widget/pull/347");
    w.run7();
    // The line carries tehas work's exit, 2, and the step cleaned to
    // [A-Za-z0-9_.-] and 40 bytes.
    let step: String = BRANCH
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || "_.-".contains(*c))
        .take(40)
        .collect();
    let h = w.history0();
    assert_eq!(
        json!([h["status"], h["step"], h["rc"], h["pr"], h["parked"]]),
        json!(["handoff", step, 2, 347, false])
    );
}

#[test]
fn the_runners_own_status_file_is_not_left_for_the_next_cycle_of_the_issue() {
    let w = World::new("crash-cleared");
    died_pushed(&w, &["rc=134", "work=2"]);
    w.open_pr("");
    w.run7();
    assert!(!w.state().join("7.runner_exit").exists());
    assert!(!w.state().join("7.status").exists());
}

#[test]
fn a_runner_exit_and_a_status_of_an_earlier_cycle_are_not_this_cycles_end() {
    let w = World::new("crash-earlier");
    // The files are cleared when the cycle takes its lock, before tehas work
    // starts. The summary of the run, which says a log may be read at all, is
    // seeded too, two days old.
    let s = w.state();
    fs::create_dir_all(&s).unwrap();
    fs::write(s.join("7.runner_exit"), "134\n").unwrap();
    fs::write(s.join("7.status"), "killed\n").unwrap();
    fs::write(s.join("7.result.json"), "").unwrap();
    let progress = s.join("7.progress.json");
    fs::write(
        &progress,
        "{\"roles\":[],\"reason\":\"\",\"reviewHeld\":false}\n",
    )
    .unwrap();
    fs::File::options()
        .write(true)
        .open(&progress)
        .unwrap()
        .set_modified(SystemTime::now() - Duration::from_secs(2 * 86400))
        .unwrap();
    fs::write(
        w.home.join("logs/work-7-261019-012100.jsonl"),
        "{\"v\":1,\"kind\":\"step-started\",\"issue\":7,\"event\":{\"step\":\"lied-step\"}}\n\
         {\"v\":1,\"kind\":\"step-started\",\"issue\":7,\"event\":{\"step\":\"feature/issue-7-from-an-earlier-run\"}}\n",
    )
    .unwrap();
    fs::create_dir_all(w.workdir()).unwrap();
    w.open_pr("");
    w.run7();
    let gh = w.gh();
    lacks(&gh, "lied-step");
    lacks(&gh, "read from its run log");
    lacks(&gh, "(runner exit 134)");
    lacks(&gh, "pr list --repo");
    lacks(&gh, "handed off with its pull request open");
    assert!(!s.join("7.runner_exit").exists());
    assert!(!s.join("7.status").exists());
}

#[test]
fn of_several_run_logs_of_the_issue_only_the_newest_is_read() {
    let w = World::new("crash-newest");
    died_pushed(&w, &[]);
    // Two earlier logs, one a day old and one two days: the newest name
    // decides, and no step of the others is said.
    fs::write(
        w.home
            .join("logs")
            .join(log_name(Duration::from_secs(86400))),
        "{\"v\":1,\"kind\":\"step-started\",\"issue\":7,\"event\":{\"step\":\"lied-step\"}}\n",
    )
    .unwrap();
    fs::write(
        w.home
            .join("logs")
            .join(log_name(Duration::from_secs(2 * 86400))),
        "{\"v\":1,\"kind\":\"step-started\",\"issue\":7,\"event\":{\"step\":\"another-lied-step\"}}\n",
    )
    .unwrap();
    w.open_pr("");
    w.run7();
    let gh = w.gh();
    lacks(&gh, "lied-step");
    lacks(&gh, "another-lied-step");
    has(&gh, &format!("last step `{BRANCH}` read from its run log"));
    has(&gh, &pr_list());
}

#[test]
fn a_step_name_over_the_bound_of_100_characters_is_no_step_of_a_run() {
    let w = World::new("crash-long");
    let long = "x".repeat(101);
    w.died(&["commit-pr", &long]);
    w.open_pr("");
    w.run7();
    let gh = w.gh();
    lacks(&gh, &long);
    lacks(&gh, "pr list --repo");
    has(&gh, "Tehas cycle stopped without a pull request");
}

#[test]
fn the_step_tehas_work_saw_in_flight_is_said_as_its_own_word_not_the_runners() {
    let w = World::new("crash-timed");
    w.died(&[
        "explore",
        "plan",
        "develop",
        "timed-step",
        "rc=2",
        "work=2",
        "status=-",
    ]);
    w.work(
        r#"mkdir -p "$TEHAS_HOME/state/cycles" "$TEHAS_HOME/logs"
: >"$TEHAS_HOME/state/cycles/7.result.json"
echo '{"roles":[],"reason":"","reviewHeld":false}' >"$TEHAS_HOME/state/cycles/7.progress.json"
printf '3600 runner timed-step\n' >"$TEHAS_HOME/state/cycles/7.timed"
printf '%s\n' '3600' >"$TEHAS_HOME/state/cycles/7.runner_exit"
log=$TEHAS_HOME/logs/work-7-$(date -u +%y%m%d-%H%M%S).jsonl
printf '{"v":1,"kind":"step-started","event":{"step":"a-later-step"}}\n' >"$log"
exit 2"#,
    );
    w.run7();
    let gh = w.gh();
    has(&gh, "last step `timed-step` tehas work saw in flight");
    lacks(&gh, "read from its run log");
    lacks(&gh, "a-later-step");
}

#[test]
fn the_step_of_a_stalled_cycle_is_said_as_what_tehas_work_saw() {
    let w = World::new("crash-stalled");
    w.work(
        r#"mkdir -p "$TEHAS_HOME/state/cycles" "$TEHAS_HOME/logs"
: >"$TEHAS_HOME/state/cycles/7.result.json"
echo '{"roles":[],"reason":"","reviewHeld":false}' >"$TEHAS_HOME/state/cycles/7.progress.json"
printf '%s\n%s\n' 1800 stall-step >"$TEHAS_HOME/state/cycles/7.stalled"
printf '%s\n' '3600' >"$TEHAS_HOME/state/cycles/7.runner_exit"
exit 2"#,
    );
    w.run7();
    let gh = w.gh();
    has(&gh, "last step `stall-step` tehas work saw in flight");
    lacks(&gh, "read from its run log");
}

#[test]
fn a_run_log_past_the_200_kb_tehas_reads_is_not_taken_for_a_clean_handoff() {
    let w = World::new("crash-big");
    // The commit-pr line of a big run may have gone at the cut, so such a
    // cycle is parked with the pull request named.
    died_pushed(&w, &[]);
    w.open_pr("");
    w.work(
        r#"mkdir -p "$TEHAS_HOME/state/cycles" "$TEHAS_HOME/logs"
: >"$TEHAS_HOME/state/cycles/7.result.json"
echo '{"roles":[],"reason":"","reviewHeld":false}' >"$TEHAS_HOME/state/cycles/7.progress.json"
printf '%s\n' '134' >"$TEHAS_HOME/state/cycles/7.runner_exit"
log=$TEHAS_HOME/logs/work-7-$(date -u +%y%m%d-%H%M%S).jsonl
head -c 210000 /dev/zero | tr '\0' 'y' | fold -w 100 | sed 's/.*/{"v":1,"kind":"log","message":"&"}/' >"$log"
for step in explore plan develop commit-pr feature/issue-7-installed; do
  printf '{"v":1,"kind":"step-started","event":{"step":"%s"}}\n' "$step" >>"$log"
done
exit 2"#,
    );
    w.run7();
    let gh = w.gh();
    has(&gh, "Tehas cycle stopped with pull request #347 open");
    has(
        &gh,
        "Its branch has an open pull request: https://github.com/acme/widget/pull/347",
    );
    lacks(&gh, "handed off with its pull request open");
    lacks(&gh, "its review unfinished");
}

#[test]
fn the_table_of_a_run_whose_result_carries_no_usage_totals_the_rows_it_did_write() {
    let w = World::new("crash-rows");
    w.ended_no_usage_with_rows();
    w.open_pr("");
    w.run7();
    let gh = w.gh();
    has(&gh, "| planner | 2 | 3 | 7 | 1 | 0 |");
    has(&gh, "| **total, all dispatches** | 3 | 5 | 10 | 5 | 5 |");
}

#[test]
fn a_row_with_a_word_in_place_of_one_of_its_counts_is_not_summed_into_the_totals() {
    let w = World::new("crash-sly");
    w.ended_no_usage_with_a_sly_row();
    w.open_pr("");
    w.run7();
    let gh = w.gh();
    has(&gh, "| planner | 2 | 0 | 7 | 0 | 0 |");
    has(&gh, "| **total, all dispatches** | 0 | 0 | 0 | 0 | 0 |");
}
