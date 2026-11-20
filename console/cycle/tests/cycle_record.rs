//! The report a cycle leaves with GitHub, the poll it starts, and what it
//! puts on record when it ends.

mod common;

use common::{has, lacks, World};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

const RUKAS: &str = "0123456789abcdef0123456789abcdef01234567";
const LOG: &str = r#"echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl""#;
const POLL: &str = "--user start --no-block tehas-dispatch.service";

fn gone(p: &Path) -> bool {
    fs::symlink_metadata(p).is_err()
}

fn image_id() -> String {
    format!("sha256:{}", "e".repeat(64))
}

/// What tehas work puts on the cycle's record as the container starts.
fn image_work(w: &World, image: &str, rukas: &str) {
    let rukas = if rukas.is_empty() {
        String::new()
    } else {
        format!(" --rukas {rukas}")
    };
    w.work(&format!(
        "{LOG}\n\"$T/store-bin\" cycle-image 7 --attempt \"$TEHAS_CYCLE_ATTEMPT\" --image {image}{rukas} >/dev/null; exit 2"
    ));
}

/// Fields of the first history row.
fn hist(w: &World, keys: &[&str]) -> Value {
    let h = w.history0();
    Value::Array(keys.iter().map(|k| h[*k].clone()).collect())
}

fn cycle(w: &World) -> Value {
    w.row(&["cycle-show", "7"])["cycle"].clone()
}

fn edits(gh: &str) -> String {
    gh.lines()
        .filter(|l| l.starts_with("issue edit"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A note of a runner that never started, with a run log and an exit of 2.
fn never_started(w: &World, more: &str) {
    let _ = fs::remove_file(w.fx.join("result.json"));
    w.work(&format!(
        "mkdir -p \"$TEHAS_HOME/state/cycles\" \"$TEHAS_HOME/logs\"; {LOG}; cat \"$T/fx/nostart\" >\"$TEHAS_HOME/state/cycles/7.nostart\"; {more}exit 2"
    ));
}

#[test]
fn a_cycle_that_ended_is_on_record_with_its_end() {
    let w = World::new("rec-ended");
    w.ended("handoff", "commit-pr", "feature/x", "");
    w.fx("pr.txt", "https://github.com/acme/widget/pull/9\n");
    w.work(r#"echo "{\"type\":\"agent_start\"}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; exit 0"#);
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert_eq!(
        hist(&w, &["status", "step", "rc", "pr"]),
        json!(["handoff", "commit-pr", 0, 9])
    );
    assert_eq!(w.calls("fake-status").lines().count(), 1);
}

#[test]
fn the_record_names_the_image_and_the_pi_rukas_commit_tehas_work_saw_as_the_container_started() {
    let w = World::new("rec-image");
    w.fx("image.label", &"c".repeat(40));
    image_work(&w, &image_id(), RUKAS);
    w.run7();
    assert_eq!(cycle(&w)["image"], json!(image_id()));
    assert_eq!(cycle(&w)["rukas_commit"], RUKAS);
}

#[test]
fn an_earlier_cycles_image_is_not_this_cycles() {
    let w = World::new("rec-oldimage");
    w.fx("image.label", &"c".repeat(40));
    image_work(&w, &image_id(), RUKAS);
    w.run7();
    w.work(&format!("{LOG}; exit 2"));
    w.run7();
    let c = cycle(&w);
    assert_eq!(json!([c["attempt"], c["image"]]), json!([2, null]));
}

#[test]
fn an_image_that_is_no_image_id_is_not_on_record() {
    let w = World::new("rec-badimage");
    w.fx("image.label", &"c".repeat(40));
    image_work(&w, "sha256:short", "");
    w.run7();
    assert_eq!(cycle(&w)["image"], Value::Null);
}

#[test]
fn the_record_names_the_runners_exit_when_there_is_no_pull_request() {
    let w = World::new("rec-exit");
    w.work(&format!("{LOG}; exit 2"));
    w.run7();
    assert_eq!(
        hist(&w, &["status", "rc", "pr"]),
        json!(["unknown", 2, null])
    );
}

#[test]
fn runner_exit_137_with_no_end_state_is_status_killed_in_the_comment_and_the_record() {
    let w = World::new("rec-137");
    w.work(&format!("{LOG}; exit 137"));
    let r = w.run7();
    assert_eq!(r.rc, 0);
    let gh = w.gh();
    has(&gh, "issue edit 7 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention");
    has(
        &gh,
        "Tehas cycle stopped without a pull request: status `killed`",
    );
    has(&gh, "(runner exit 137)");
    lacks(&gh, "killed for memory");
    has(&r.out, "done #7 rc=137 status=killed");
    assert_eq!(
        hist(&w, &["status", "rc", "pr"]),
        json!(["killed", 137, null])
    );
}

#[test]
fn runner_exit_137_with_a_state_that_still_says_running_is_killed_too() {
    let w = World::new("rec-137run");
    w.ended("running", "develop", "", "");
    w.work("exit 137");
    w.run7();
    has(&w.gh(), "status `killed`, last step `develop`");
    has(&w.gh(), "--add-label needs-human-attention");
}

#[test]
fn runner_exit_137_with_a_state_that_names_an_end_keeps_that_status_and_is_still_parked() {
    let w = World::new("rec-137end");
    w.ended("handoff", "commit-pr", "feature/x", "");
    w.fx("pr.txt", "https://github.com/acme/widget/pull/9\n");
    w.work("exit 137");
    w.run7();
    let gh = w.gh();
    has(&gh, "status `handoff`");
    lacks(&gh, "status `killed`");
    has(&gh, "--add-label needs-human-attention");
    lacks(&gh, "tehas:awaiting-merge");
}

#[test]
fn another_exit_with_no_end_state_is_not_called_killed() {
    let w = World::new("rec-exit2");
    w.work("exit 2");
    let r = w.run7();
    lacks(&w.gh(), "killed");
    has(&r.out, "status=unknown");
}

#[test]
fn the_oom_count_file_is_read_as_digits_said_in_the_comment_put_in_the_record_and_removed() {
    let w = World::new("rec-oom");
    w.work(&format!(
        r#"{LOG}; echo 3 >"$TEHAS_HOME/state/cycles/7.oom"; exit 137"#
    ));
    let r = w.run7();
    has(
        &w.gh(),
        "3 processes were killed for memory in its container.",
    );
    has(&r.out, "oom_kills=3");
    assert_eq!(hist(&w, &["status", "rc"]), json!(["killed", 137]));
    assert_eq!(cycle(&w)["oom_kills"], 3);
    assert!(gone(&w.state().join("7.oom")));
}

#[test]
fn a_stalled_cycle_is_reported_as_stalled_with_how_long_nothing_moved_parked_and_the_file_removed()
{
    let w = World::new("rec-stall");
    w.ended("running", "lens-fix", "", "");
    w.work(&format!(
        r#"{LOG}; echo 3600 >"$TEHAS_HOME/state/cycles/7.stalled"; exit 2"#
    ));
    let r = w.run7();
    let gh = w.gh();
    has(&r.out, "status=stalled");
    has(
        &gh,
        "None of its logs changed for 60 min at step `lens-fix`",
    );
    has(&gh, "needs-human-attention");
    assert_eq!(hist(&w, &["status", "rc"]), json!(["stalled", 2]));
    assert_eq!(w.park()["reason"], "stall");
    has(&gh, "cap `stall`");
    assert!(gone(&w.state().join("7.stalled")));
}

// The three cases below are the shape of the two cycles of tehas-org/tehas#110:
// a container that ended with nothing in its record, no step and no cap.

#[test]
fn a_container_that_ended_before_the_driver_wrote_an_event_is_on_record_as_runner_never_started_with_its_exit_status_and_no_step(
) {
    let w = World::new("rec-nostart");
    w.nostart("1");
    never_started(&w, "");
    w.run7();
    assert_eq!(
        hist(&w, &["status", "step", "cap"]),
        json!(["unknown", "no-step", "runner-never-started"])
    );
    assert_eq!(w.park()["reason"], "runner-never-started");
    has(
        &w.gh(),
        "tehas stopped the cycle: the cap `runner-never-started`.",
    );
    has(
        &w.gh(),
        "no step started, the container of the cycle ended before the driver wrote an event",
    );
    assert!(gone(&w.state().join("7.nostart")));
}

#[test]
fn the_memory_kills_of_a_runner_that_never_started_are_said_once_from_the_oom_file() {
    let w = World::new("rec-nostart-oom");
    w.nostart("1");
    never_started(&w, r#"echo 3 >"$TEHAS_HOME/state/cycles/7.oom"; "#);
    w.run7();
    has(
        &w.gh(),
        "3 processes were killed for memory in its container.",
    );
    lacks(&w.gh(), "processes in its container were killed for memory");
    assert!(gone(&w.state().join("7.nostart")));
    assert!(gone(&w.state().join("7.oom")));
}

#[test]
fn the_note_of_a_runner_that_never_started_is_read_for_nothing_when_the_cycle_wrote_a_result_and_one_of_an_earlier_cycle_is_not_this_cycles(
) {
    let w = World::new("rec-nostart-old");
    w.ended("handoff", "commit-pr", "feature/x", "");
    w.fx("pr.txt", "https://github.com/acme/widget/pull/9\n");
    // The note of an earlier cycle: it is gone before the report reads
    // anything, and this cycle's result has the last word.
    fs::create_dir_all(w.state()).unwrap();
    w.nostart("1");
    fs::copy(w.fx.join("nostart"), w.state().join("7.nostart")).unwrap();
    w.work(&format!("{LOG}; exit 10"));
    w.run7();
    assert_eq!(
        hist(&w, &["status", "step", "cap"]),
        json!(["handoff", "commit-pr", "awaiting-human-merge"])
    );
    assert!(gone(&w.state().join("7.nostart")));
}

#[test]
fn a_note_that_holds_a_bad_value_is_no_note_the_cycle_is_reported_as_one_that_died_part_way_through(
) {
    // A note the reader cannot believe is no note: the step is read from the
    // run log of a watched run, and no word of the cap.
    for note in ["", "x", "0", "-1", "1000", "01"] {
        let w = World::new("rec-badnote");
        let _ = fs::remove_file(w.fx.join("result.json"));
        w.nostart(note);
        w.died(&["explore"]);
        w.work_more(r#"mkdir -p "$TEHAS_HOME/state/cycles"; cat "$T/fx/nostart" >"$TEHAS_HOME/state/cycles/7.nostart"; exit 2"#);
        w.run7();
        assert_eq!(
            hist(&w, &["step", "cap"]),
            json!(["explore", null]),
            "{note:?}"
        );
        lacks(&w.gh(), "runner-never-started");
        assert!(gone(&w.state().join("7.nostart")));
    }
}

#[test]
fn a_second_field_of_the_note_is_read_for_nothing_the_cap_stands_and_the_count_of_the_oom_file_is_the_one_said(
) {
    let w = World::new("rec-note2");
    w.nostart("1 007");
    never_started(&w, r#"echo 3 >"$TEHAS_HOME/state/cycles/7.oom"; "#);
    w.run7();
    assert_eq!(
        hist(&w, &["step", "cap"]),
        json!(["no-step", "runner-never-started"])
    );
    has(
        &w.gh(),
        "3 processes were killed for memory in its container.",
    );
}

#[test]
fn a_note_that_is_a_link_is_not_read() {
    let w = World::new("rec-notelink");
    fs::write(w.t().join("elsewhere"), "1\n").unwrap();
    fs::create_dir_all(w.state()).unwrap();
    std::os::unix::fs::symlink(w.t().join("elsewhere"), w.state().join("7.nostart")).unwrap();
    w.work(&format!("{LOG}; exit 2"));
    w.run7();
    assert_eq!(hist(&w, &["step", "cap"]), json!(["-", null]));
    lacks(&w.gh(), "runner-never-started");
    assert_eq!(fs::read_to_string(w.t().join("elsewhere")).unwrap(), "1\n");
}

#[test]
fn the_pi_rukas_a_cycle_starts_with_is_on_its_record_and_nothing_writes_state_rukas_proven() {
    let w = World::new("rec-rukas");
    let rukas = "c".repeat(40);
    w.fx("image.label", &format!("{rukas}\n"));
    w.ended("merged", "commit-pr", "feature/x", "");
    w.merged_pr();
    // On record before tehas work runs.
    w.work(r#""$T/store-bin" cycle-show 7 | jq -r .cycle.rukas_commit >"$T/calls/rukas.at-work""#);
    w.run7();
    assert_eq!(w.calls("rukas.at-work").trim(), rukas);
    assert_eq!(cycle(&w)["rukas_commit"], json!(rukas));
    fs::remove_file(w.fx.join("work.sh")).unwrap();
    w.ended("handoff", "commit-pr", "feature/x", "");
    w.fx("pr-state", "OPEN\n");
    w.run7();
    has(&w.gh(), "Pull request waiting for a human");
    assert!(gone(&w.home.join("state/rukas-proven")));
}

#[test]
fn an_end_the_store_will_not_take_is_an_error_with_its_arguments_the_cycle_is_still_reported_and_the_next_start_replaces_the_open_row(
) {
    let w = World::new("rec-busy");
    w.ended("handoff", "commit-pr", "feature/x", "");
    w.store_answers("cycle-end", 1, "store: busy #7: the database is locked", 0);
    let r = w.run7();
    // An end the store did not take is exit 70: the unit did not succeed.
    assert_eq!(r.rc, 70);
    has(&r.out, "ERROR: the end of #7 is not on record (busy: store: busy #7: the database is locked): cycle-end 7 --attempt 1 --status handoff");
    assert_eq!(w.store_answered(), 5);
    assert_eq!(w.calls("fake-status").lines().count(), 1);
    let c = cycle(&w);
    assert_eq!(json!([c["attempt"], c["ended_at"]]), json!([1, null]));
    w.store_answers_off();
    let r = w.run7();
    assert_eq!(r.rc, 0);
    let c = cycle(&w);
    assert_eq!(c["attempt"], 2);
    assert!(!c["ended_at"].is_null());
}

#[test]
fn the_end_of_a_cycle_starts_one_poll_of_the_dispatcher_and_leaves_the_request_for_the_path_unit() {
    let w = World::new("rec-poll");
    let _ = fs::remove_file(w.home.join("state/dispatch.again"));
    w.run7();
    assert_eq!(w.calls("systemctl").trim_end(), POLL);
    assert!(w.home.join("state/dispatch.again").exists());
}

#[test]
fn the_path_unit_watches_for_the_file_the_cycle_leaves_and_starts_a_poll() {
    let unit = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../host/systemd/tehas-dispatch.path"
    );
    let text = fs::read_to_string(unit).unwrap();
    let value =
        |key: &str| -> Vec<&str> { text.lines().filter_map(|l| l.strip_prefix(key)).collect() };
    assert_eq!(value("PathExists="), ["%h/tehas/state/dispatch.again"]);
    assert_eq!(value("Unit="), ["tehas-dispatch.service"]);
}

#[test]
fn a_poll_that_cannot_be_started_is_a_warning_and_the_cycle_is_still_reported() {
    let w = World::new("rec-nopoll");
    w.stub("systemctl", "exit 1");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    has(&r.out, "WARN: could not start a poll of the dispatcher");
    has(&r.out, "done #7");
}

#[test]
fn a_step_longer_than_one_argument_may_be_stops_nothing_that_follows_the_end() {
    let w = World::new("rec-long");
    w.ended("failed", "develop", "", "");
    w.entry(|e| e["failedStep"] = json!("x".repeat(200_000)));
    let r = w.run7();
    let kept: Value =
        serde_json::from_str(&fs::read_to_string(w.fx.join("result.json")).unwrap()).unwrap();
    assert_eq!(
        kept["issues"][0]["failedStep"].as_str().unwrap().len(),
        200_000
    );
    lacks(&r.out, "did not run");
    assert_eq!(w.calls("systemctl").trim_end(), POLL);
    assert!(w.home.join("state/dispatch.again").exists());
}

#[test]
fn a_handoff_whose_pull_request_merged_before_the_cycle_ended_is_reported_with_it() {
    let w = World::new("rec-merged");
    w.ended("handoff", "handoff", "feature/x", "");
    w.merged_pr();
    let r = w.run7();
    assert_eq!(
        edits(&w.gh()),
        "issue edit 7 --repo acme/widget --remove-label tehas:running"
    );
    has(&w.gh(), "https://github.com/acme/widget/pull/9");
    has(&r.out, "status=merged");
    has(&r.out, "pr=https://github.com/acme/widget/pull/9");
}

#[test]
fn a_cycle_the_dispatcher_stopped_because_its_issue_was_closed_only_loses_its_running_label() {
    let w = World::new("rec-closed");
    w.ended("running", "adversarial", "feature/x", "");
    w.work(r#"mkdir -p "$TEHAS_HOME/state/cycles"; echo closed >"$TEHAS_HOME/state/cycles/7.done"; exit 2"#);
    let r = w.run7();
    assert_eq!(r.rc, 0);
    let gh = w.gh();
    assert_eq!(
        edits(&gh),
        "issue edit 7 --repo acme/widget --remove-label tehas:running"
    );
    has(
        &gh,
        "Tehas stopped the cycle: this issue was closed while it ran",
    );
    lacks(&gh, "needs-human-attention");
    lacks(&gh, "issues/7/comments");
    has(&r.out, "status=closed");
    assert!(gone(&w.state().join("7.done")));
}

#[test]
fn a_cycle_the_dispatcher_stopped_for_being_quiet_names_that_stop_in_its_comment_and_its_record() {
    let w = World::new("rec-quiet");
    w.ended("running", "adversarial", "feature/x", "");
    w.work(r#"mkdir -p "$TEHAS_HOME/state/cycles"; echo quiet >"$TEHAS_HOME/state/cycles/7.done"; exit 2"#);
    let r = w.run7();
    assert_eq!(r.rc, 0);
    has(&w.gh(), "The dispatcher stopped it");
    assert_eq!(w.history0()["park_reason"], "quiet");
    assert!(gone(&w.state().join("7.done")));
}

#[test]
fn a_reason_the_dispatcher_left_for_an_earlier_cycle_is_not_this_cycles() {
    let w = World::new("rec-olddone");
    fs::create_dir_all(w.state()).unwrap();
    fs::write(w.state().join("7.done"), "closed\n").unwrap();
    w.run7();
    has(&w.gh(), "--add-label needs-human-attention");
    lacks(&w.gh(), "this issue was closed");
}

#[test]
fn a_merged_pull_request_wins_over_a_closed_issue() {
    let w = World::new("rec-mergedclosed");
    w.ended("running", "adversarial", "feature/x", "");
    w.merged_pr();
    w.work(r#"mkdir -p "$TEHAS_HOME/state/cycles"; echo closed >"$TEHAS_HOME/state/cycles/7.done"; exit 2"#);
    let r = w.run7();
    has(&w.gh(), "was merged while the cycle ran");
    has(&r.out, "status=merged");
}

#[test]
fn a_cycle_that_left_a_pull_request_says_the_count_too() {
    let w = World::new("rec-prcount");
    w.ended("handoff", "commit-pr", "feature/x", "");
    w.fx("pr.txt", "https://github.com/acme/widget/pull/9\n");
    w.work(r#"echo 2 >"$TEHAS_HOME/state/cycles/7.oom""#);
    w.run7();
    has(&w.gh(), "Pull request waiting for a human: https://github.com/acme/widget/pull/9 2 processes were killed for memory in its container.");
    has(&w.gh(), "--add-label tehas:awaiting-merge");
}

#[test]
fn a_cycle_leaves_nothing_under_tmp() {
    let w = World::new("rec-tmp");
    w.work(LOG);
    let r = w.run7();
    assert_eq!(r.rc, 0);
    fn files(d: &Path) -> usize {
        fs::read_dir(d)
            .map(|it| {
                it.map(|e| {
                    let p = e.unwrap().path();
                    if p.is_dir() {
                        files(&p)
                    } else {
                        1
                    }
                })
                .sum()
            })
            .unwrap_or(0)
    }
    assert_eq!(files(&w.home.join("tmp")), 0);
}
