//! A cycle stopped at a limit or by a person: the time limit, the stall, the
//! signal, the caps it parks under, and the notes it reads.

mod common;

use common::{has, lacks, World};
use serde_json::{json, Value};
use std::fs;

/// The first row of #7's history, these fields of it.
fn hist(w: &World, keys: &[&str]) -> Value {
    let h = w.history0();
    json!(keys.iter().map(|k| h[*k].clone()).collect::<Vec<_>>())
}

fn sc(w: &World) -> Value {
    hist(w, &["status", "step", "cap"])
}

/// Whether a note of the cycle's state is gone.
fn gone(w: &World, name: &str) -> bool {
    !w.state().join(name).exists()
}

fn now() -> i64 {
    tehas_core::time::now()
}

/// A log of the run with nothing stable in it, then `rest`.
fn logged(rest: &str) -> String {
    format!(r#"echo "{{}}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; {rest}"#)
}

#[test]
fn a_cycle_tehas_stopped_at_its_time_limit_with_no_cap_from_the_runner_is_on_record_as_wall_clock()
{
    let w = World::new("rs-wall");
    w.ended("running", "develop", "-", "");
    w.fx("progress.json", "{}\n");
    w.entry(|e| {
        e["handoff"] = json!({"channel": "host", "body": "<!-- pi-rukas:handoff issue=7 cap=none -->\n## Cap hit"})
    });
    w.work(&logged(
        r#"echo 21600 tehas develop >"$TEHAS_HOME/state/cycles/7.timed"; exit 2"#,
    ));
    w.run7();
    assert_eq!(w.park()["reason"], "wall-clock");
    has(&w.gh(), "cap `wall-clock`");
    has(&w.gh(), "pi-rukas:handoff issue=7 cap=wall-clock -->");
    assert_eq!(sc(&w), json!(["timeout", "develop", "wall-clock"]));
}

#[test]
fn a_cycle_pi_rukas_ended_at_a_childs_run_bound_is_on_record_with_the_childs_cap() {
    let w = World::new("rs-child");
    w.ended(
        "aborted",
        "develop",
        "feature/issue-7-x",
        "child-timeout:develop",
    );
    w.work(&logged("exit 20"));
    w.run7();
    assert_eq!(
        hist(&w, &["status", "step", "rc", "cap"]),
        json!(["aborted", "develop", 20, "child-timeout:develop"])
    );
}

#[test]
fn a_cycle_a_signal_stopped_inside_a_gate_is_on_record_with_the_cap_signal_and_its_gate_closed() {
    let w = World::new("rs-signal");
    w.ended("interrupted", "develop", "", "");
    w.fx("progress.json", "{}\n");
    // The stream and the note as tehas work leaves them for a TERM in the gate
    // of step develop: the gate's start, the line that closes it, and the signal.
    let stream = [
        json!({"v": 1, "kind": "verify-started", "issue": 7, "event": {"kind": "verify-started", "step": "develop", "command": "make test"}}),
        json!({"v": 1, "kind": "verify-aborted", "issue": 7, "by": "tehas", "event": {"kind": "verify-aborted", "step": "develop", "command": "make test", "status": "aborted", "signal": "TERM"}}),
    ];
    w.fx("stream", &lines(&stream));
    w.work(r#"cp "$T/fx/stream" "$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; echo TERM develop >"$TEHAS_HOME/state/cycles/7.signal"; exit 21"#);
    w.run7();
    assert_eq!(sc(&w), json!(["interrupted", "develop", "signal"]));
    has(&w.gh(), "tehas stopped the cycle: the cap `signal`.");
    // The note is read once and gone.
    assert!(gone(&w, "7.signal"));
}

fn lines(v: &[Value]) -> String {
    v.iter().map(|l| format!("{l}\n")).collect()
}

#[test]
fn a_container_killed_after_its_pull_request_opened_is_on_record_as_container_killed_with_its_memory_kills_and_its_pull_request(
) {
    let w = World::new("rs-killed");
    // A driver that names its branch in an event after commit-pr, not as a
    // step, and a container killed out of memory in lens-fix, with no result.
    let step = |s: &str| json!({"v": 1, "kind": "step-started", "issue": 7, "event": {"kind": "step-started", "step": s}});
    let mut stream: Vec<Value> = ["explore", "develop", "commit-pr", "lens-review"]
        .into_iter()
        .map(step)
        .collect();
    stream.push(json!({"v": 1, "kind": "lens-residuals-posted", "issue": 7, "unstable": true,
        "event": {"kind": "lens-residuals-posted", "prNumber": 347, "branch": "feature/issue-7-checks"}}));
    stream.push(step("lens-fix"));
    w.fx("stream", &lines(&stream));
    w.work(
        r#"s=$TEHAS_HOME/state/cycles
: >"$s/7.result.json"
echo killed >"$s/7.status"
echo 137 >"$s/7.runner_exit"
echo 4 >"$s/7.oom"
cp "$T/fx/stream" "$TEHAS_HOME/logs/work-7-261011-120000.jsonl"
cp "$T/fx/stream" "$TEHAS_HOME/logs/dispatch-work-7.log"
exit 137"#,
    );
    w.open_pr("");
    w.run7();
    assert_eq!(
        hist(&w, &["status", "step", "cap", "pr"]),
        json!(["handoff", "lens-fix", "container-killed", 347])
    );
    assert_eq!(w.row(&["cycle-show", "7"])["cycle"]["oom_kills"], 4);
    has(
        &w.gh(),
        "pr list --repo acme/widget --head feature/issue-7-checks",
    );
    has(&w.gh(), "https://github.com/acme/widget/pull/347");
}

#[test]
fn a_runner_that_stopped_itself_at_the_time_limit_with_no_result_is_on_record_as_wall_clock_at_the_step_it_was_in_not_running(
) {
    let w = World::new("rs-runner-timed");
    w.work(&logged(
        r#"echo 21600 runner adversarial >"$TEHAS_HOME/state/cycles/7.timed"; exit 2"#,
    ));
    let r = w.run7();
    has(&r.out, "status=timeout step=adversarial");
    has(
        &w.gh(),
        "It reached its time limit of 360 min at step `adversarial`, and the runner stopped there.",
    );
    lacks(&w.gh(), "did not stop on its own");
    assert_eq!(sc(&w), json!(["timeout", "adversarial", "wall-clock"]));
}

#[test]
fn a_runner_that_stopped_itself_at_the_time_limit_with_a_result_that_names_no_end_is_on_record_as_wall_clock(
) {
    let w = World::new("rs-runner-noend");
    w.ended("running", "commit-pr", "-", "");
    w.work(&logged(
        r#"echo 21600 runner commit-pr >"$TEHAS_HOME/state/cycles/7.timed"; exit 30"#,
    ));
    w.run7();
    assert_eq!(
        hist(&w, &["status", "step", "cap", "rc"]),
        json!(["timeout", "commit-pr", "wall-clock", 30])
    );
    assert_eq!(w.park()["reason"], "wall-clock");
}

#[test]
fn the_step_in_the_time_limits_file_is_read_as_a_word() {
    let w = World::new("rs-timed-word");
    w.work(r#"echo "21600 runner \`rm\`;x" >"$TEHAS_HOME/state/cycles/7.timed"; exit 2"#);
    let r = w.run7();
    has(&r.out, "status=timeout step=- ");
}

#[test]
fn a_stop_at_the_time_limit_names_the_step_tehas_work_saw_in_flight_when_the_result_names_none() {
    let w = World::new("rs-inflight");
    w.ended("running", "-", "", "");
    w.work(&logged(
        r#"echo "21600 tehas adversarial" >"$TEHAS_HOME/state/cycles/7.timed"; exit 2"#,
    ));
    w.run7();
    assert_eq!(sc(&w), json!(["timeout", "adversarial", "wall-clock"]));
    has(
        &w.gh(),
        "It ran past its time limit of 360 min at step `adversarial`",
    );
    assert!(gone(&w, "7.timed"));
}

#[test]
fn a_runner_stopped_mid_step_at_the_time_limit_is_on_record_with_tehass_cap_its_own_last_cap_in_the_reason(
) {
    let w = World::new("rs-midstep");
    w.ended("interrupted", "develop", "-", "verify-failed:develop");
    w.fx("progress.json", r#"{"reason": "fence violated"}"#);
    w.work(&logged(
        r#"echo "21600 tehas develop" >"$TEHAS_HOME/state/cycles/7.timed"; exit 2"#,
    ));
    w.run7();
    assert_eq!(sc(&w), json!(["timeout", "develop", "wall-clock"]));
    assert_eq!(w.park()["reason"], "wall-clock");
    has(&w.gh(), "tehas stopped the cycle: the cap `wall-clock`.");
    lacks(&w.gh(), "pi-rukas stopped on the cap");
}

// #1207: the seconds of a stop file and its step come of one read, so a first
// word that is no count of seconds is no stop and no step.
#[test]
fn a_stop_file_of_no_count_of_seconds_is_no_stalled_status_and_no_stalled_step() {
    let w = World::new("rs-stall-nope");
    w.ended("aborted", "develop", "", "");
    w.work(&logged(
        r#"printf "nope\nstall-step\n" >"$TEHAS_HOME/state/cycles/7.stalled"; exit 2"#,
    ));
    let r = w.run7();
    assert_eq!(sc(&w), json!(["aborted", "develop", null]));
    lacks(&r.out, "status=stalled");
    lacks(&w.gh(), "stall-step");
    lacks(&w.gh(), "None of its logs changed");
    assert!(gone(&w, "7.stalled"));
}

#[test]
fn a_stop_file_of_no_count_of_seconds_is_no_timeout_and_no_step_of_the_time_limit() {
    let w = World::new("rs-timed-nope");
    w.ended("aborted", "develop", "", "");
    w.work(&logged(
        r#"echo "nope tehas timed-step" >"$TEHAS_HOME/state/cycles/7.timed"; exit 2"#,
    ));
    let r = w.run7();
    assert_eq!(sc(&w), json!(["aborted", "develop", null]));
    lacks(&r.out, "status=timeout");
    lacks(&w.gh(), "timed-step");
    assert!(gone(&w, "7.timed"));
}

#[test]
fn a_stall_names_the_step_in_flight_and_is_tehass_cap_over_a_runners_mid_step_cap() {
    let w = World::new("rs-stall");
    w.ended("interrupted", "-", "-", "step-failed:lens-fix");
    w.work(&logged(
        r#"printf "3600\nlens-fix\n" >"$TEHAS_HOME/state/cycles/7.stalled"; exit 2"#,
    ));
    w.run7();
    assert_eq!(sc(&w), json!(["stalled", "lens-fix", "stall"]));
    has(
        &w.gh(),
        "None of its logs changed for 60 min at step `lens-fix`",
    );
}

#[test]
fn a_cap_the_runner_named_is_kept_when_tehas_also_stopped_the_cycle() {
    let w = World::new("rs-cap-kept");
    w.ended("handoff", "develop", "feature/x", "deadline");
    w.work(r#"echo 21600 >"$TEHAS_HOME/state/cycles/7.timed"; exit 2"#);
    w.fx("pr.txt", "https://github.com/acme/widget/pull/9\n");
    w.run7();
    assert_eq!(w.park()["reason"], "deadline");
    has(&w.gh(), "pi-rukas stopped on the cap `deadline`.");
}

#[test]
fn a_stall_file_beside_a_state_that_names_an_end_leaves_the_end_as_it_is() {
    let w = World::new("rs-stall-end");
    w.ended("aborted", "develop", "", "");
    w.work(r#"echo 3600 >"$TEHAS_HOME/state/cycles/7.stalled"; exit 1"#);
    let r = w.run7();
    has(&r.out, "status=aborted");
}

#[test]
fn a_runner_tehas_stopped_past_its_time_limit_is_reported_as_a_timeout_and_parked() {
    let w = World::new("rs-past");
    w.ended("interrupted", "develop", "", "");
    w.work(r#"echo 21600 >"$TEHAS_HOME/state/cycles/7.timed"; exit 2"#);
    w.run7();
    let gh = w.gh();
    has(&gh, "status `timeout`, last step `develop`");
    has(
        &gh,
        "It ran past its time limit of 360 min at step `develop`",
    );
    has(&gh, "--add-label needs-human-attention");
    assert!(gone(&w, "7.timed"));
}

// The marker tehas stop leaves: one line of seconds and an account. No new
// cap: the stop is no end tehas work gives a cap or a status for.
#[test]
fn a_cycle_a_person_stopped_is_on_record_with_the_stop_in_its_reason_and_no_cap_of_its_own() {
    let w = World::new("rs-person");
    w.ended("interrupted", "develop", "-", "deadline");
    w.fx("progress.json", r#"{"reason": "waiting on the fence"}"#);
    w.work(&logged(&format!(
        r#"echo "{} alice" >"$TEHAS_HOME/state/cycles/7.stopped"; exit 2"#,
        now()
    )));
    w.run7();
    // The cap of the runner is kept: a person's stop is not a cap.
    assert_eq!(sc(&w), json!(["interrupted", "develop", "deadline"]));
    has(&w.gh(), "A person stopped it with `tehas stop` (alice, at ");
    has(&w.gh(), "pi-rukas stopped on the cap `deadline`.");
    // The marker is this cycle's name: it is read once and gone.
    assert!(gone(&w, "7.stopped"));
}

#[test]
fn a_persons_stop_with_no_cap_of_its_own_says_the_stop_in_the_reason_and_leaves_the_cap_empty() {
    let w = World::new("rs-person-nocap");
    w.ended("interrupted", "develop", "", "");
    // The marker is written from a file of the case, so no quoting of the case
    // reaches the bytes the cycle reads.
    w.fx("marker-line", &format!("{} bob.smith_1-2\n", now()));
    w.work(r#"cat "$T/fx/marker-line" >"$TEHAS_HOME/state/cycles/7.stopped"; echo {} >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; exit 137"#);
    w.run7();
    assert_eq!(sc(&w), json!(["interrupted", "develop", null]));
    assert!(gone(&w, "7.stopped"));
}

#[test]
fn the_marker_says_nothing_beside_a_time_limit_or_a_stall_tehass_own_cap_stands() {
    let w = World::new("rs-marker-timed");
    w.ended("running", "develop", "-", "");
    w.fx("progress.json", "{}\n");
    w.work(&logged(&format!(
        r#"echo 21600 tehas develop >"$TEHAS_HOME/state/cycles/7.timed"; echo "{} alice" >"$TEHAS_HOME/state/cycles/7.stopped"; exit 2"#,
        now()
    )));
    w.run7();
    assert_eq!(
        hist(&w, &["status", "cap"]),
        json!(["timeout", "wall-clock"])
    );
}

#[test]
fn a_marker_that_holds_no_line_or_one_of_the_two_fields_is_no_marker_and_is_gone() {
    let w = World::new("rs-marker-bad");
    w.ended("running", "develop", "", "");
    fs::create_dir_all(w.state()).unwrap();
    for bad in [
        "",
        "not-a-time alice",
        "1792000000",
        "   ",
        "1792000000 alice extra",
    ] {
        fs::write(w.state().join("7.stopped"), format!("{bad}\n")).unwrap();
        w.run7();
        lacks(&w.gh(), "A person stopped it");
        assert!(gone(&w, "7.stopped"), "{bad:?}");
    }
}

#[test]
fn a_marker_that_is_a_directory_or_a_link_is_not_read_and_the_cycle_is_reported_as_it_is() {
    let w = World::new("rs-marker-kind");
    w.ended("interrupted", "develop", "", "");
    let marker = w.state().join("7.stopped");
    fs::create_dir_all(&marker).unwrap();
    let r = w.run7();
    lacks(&w.gh(), "A person stopped it");
    has(&r.out, "status=interrupted");
    fs::remove_dir(&marker).unwrap();
    let elsewhere = w.t().join("elsewhere");
    fs::write(&elsewhere, format!("{} alice\n", now())).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &marker).unwrap();
    w.run7();
    lacks(&w.gh(), "A person stopped it");
    assert!(elsewhere.exists());
}

#[test]
fn a_name_that_is_not_one_a_comment_can_carry_is_cut_and_the_stop_is_still_said() {
    let w = World::new("rs-marker-noname");
    w.ended("interrupted", "develop", "", "");
    // A marker of one word: the cycle says the stop without a name rather than
    // saying none.
    w.fx("marker-line", &format!("{}\n", now()));
    w.work(r#"cat "$T/fx/marker-line" >"$TEHAS_HOME/state/cycles/7.stopped"; echo {} >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; exit 2"#);
    w.run7();
    has(
        &w.gh(),
        "A person stopped it with `tehas stop` (name not recorded, at ",
    );
    assert!(gone(&w, "7.stopped"));
}

#[test]
fn a_stall_file_left_by_an_earlier_cycle_is_not_this_cycles() {
    let w = World::new("rs-stall-old");
    w.ended("running", "develop", "", "");
    fs::create_dir_all(w.state()).unwrap();
    fs::write(w.state().join("7.stalled"), "3600\n").unwrap();
    let r = w.run7();
    has(&r.out, "status=running");
    lacks(&w.gh(), "None of its logs changed");
}
