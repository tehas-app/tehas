//! A cycle stopped by tehas work at its time limit, as stalled or by TERM, and
//! the bounds on the docker stop, the console and the push check.

mod work;

use serde_json::{json, Value};
use std::fs;
use std::process::{Command, Stdio};
use std::time::Instant;
use work::{has, lacks, World, GONE, HOLD, PUSH_SAYS};

/// A world whose gh answers for the issue: a start that finds the host cannot
/// push comments it.
fn world(name: &str) -> World {
    let w = World::new(name);
    w.stub_github();
    w
}

fn file(w: &World, rel: &str) -> String {
    fs::read_to_string(w.home.join(rel))
        .unwrap_or_default()
        .trim_end_matches('\n')
        .to_string()
}

fn gone(w: &World, rel: &str) {
    assert!(!w.home.join(rel).exists(), "{rel} is there");
}

fn live7(w: &World) -> Value {
    let v: Value =
        serde_json::from_str(&fs::read_to_string(w.live.join("cycles/7.json")).unwrap()).unwrap();
    json!([v["running"], v["status"]])
}

fn kill_sleeper(w: &World) {
    if let Ok(pid) = fs::read_to_string(w.path("sleeper.pid")) {
        let _ = Command::new("kill")
            .arg(pid.trim())
            .stderr(Stdio::null())
            .status();
    }
}

/// A container that writes interrupted when TERM reaches it.
const INTERRUPTED: &str =
    r#"trap 'says interrupted; exit 21' TERM; "$nap" 60 & echo $! >"$T/sleeper.pid"; wait; wait"#;

/// tehas work 7 in the background, sent TERM once `ready` holds or after
/// `ticks` tenths of a second; its output goes to `$T/work.out` and the
/// snippet prints `rc=<status>`.
fn term_when(w: &World, ready: &str, ticks: u32) -> work::Run {
    w.sh(&format!(
        "\"{work}\" 7 >\"$T/work.out\" 2>&1 & pid=$!
for _ in $(seq {ticks}); do {ready} && break; \"$(command -pv sleep)\" 0.1; done
kill -TERM $pid; wait $pid; echo rc=$?",
        work = w.work_cmd().display()
    ))
}

/// The live file says running, after the sleeper started.
fn running_ready(w: &World) -> String {
    format!(
        "[ -e \"$T/sleeper.pid\" ] && [ \"$(jq -r .running \"{}/cycles/7.json\" 2>/dev/null)\" = true ]",
        w.live.display()
    )
}

/// The line of the first docker call that matches.
fn docker_at(w: &World, matches: impl Fn(&str) -> bool) -> Option<usize> {
    w.calls("docker").lines().position(matches)
}

#[test]
fn a_runner_that_does_not_stop_at_its_own_limit_is_stopped_after_the_grace_and_not_reported_as_killed(
) {
    let w = world("stop-grace");
    w.fresh();
    w.minimal(&["TEHAS_WORK_TIMEOUT=1", "TEHAS_CHILD_TIMEOUT=1"]);
    w.env_ok();
    w.env("TEHAS_WORK_GRACE", "0");
    // The step is on record once the console has read it, which under load can
    // be after the limit: the console waits for the line, the container for the step.
    w.touch("now.wait");
    let seen = format!(
        "i=0; until grep -qs lens-review '{}/console-7.txt.now' || [ $i -ge 200 ]; do \"$nap\" 0.05; i=$((i + 1)); done",
        w.live.display()
    );
    w.container(&format!(
        r#"progress '{{"v":1,"kind":"step-started","event":{{"step":"lens-review"}}}}'; {HOLD}; {seen}; {GONE}"#
    ));
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    has(&r.out, "time limit of 1s passed");
    has(&r.out, "status=timeout");
    has(&r.out, "step=lens-review cap=wall-clock");
    lacks(&r.out, "was killed");
    has(&w.calls("docker"), "stop -t 20 tehas-work-7");
    assert_eq!(file(&w, "state/cycles/7.timed"), "1 tehas lens-review");
}

/// The time limit of a second, and the grace given.
fn limited(name: &str, grace: &str) -> World {
    let w = world(name);
    w.minimal(&["TEHAS_WORK_TIMEOUT=1", "TEHAS_CHILD_TIMEOUT=1"]);
    w.env_ok();
    w.env("TEHAS_WORK_GRACE", grace);
    w.fresh();
    w
}

#[test]
fn a_runner_stopped_at_the_time_limit_that_writes_interrupted_is_a_timeout_exit_2() {
    let w = limited("stop-limit-interrupted", "0");
    w.under_cycle();
    w.container(INTERRUPTED);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    has(&r.out, "status=timeout");
    has(&r.out, "cap=wall-clock");
    assert_eq!(live7(&w), json!([false, "timeout"]));
    assert_eq!(file(&w, "state/cycles/7.timed"), "1 tehas -");
}

#[test]
fn a_runner_that_stops_itself_at_the_time_limit_with_a_result_that_names_no_end_is_a_timeout_its_own_exit_kept(
) {
    let w = limited("stop-self-result", "30");
    w.container(r#""$nap" 1.2; says running; exit 30"#);
    let r = w.work7();
    assert_eq!(r.rc, 30, "{}", r.out);
    has(&r.out, "status=timeout");
    lacks(&r.out, "time limit of 1s passed");
    assert_eq!(file(&w, "state/cycles/7.timed"), "1 runner develop");
}

#[test]
fn a_runner_that_stops_itself_at_the_time_limit_with_no_result_is_a_timeout_at_the_last_step_it_started(
) {
    let w = limited("stop-self-none", "30");
    w.touch("now.wait");
    w.container(
        r#"progress '{"v":1,"kind":"step-started","event":{"step":"adversarial"}}'; "$nap" 1.2; exit 30"#,
    );
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    has(&r.out, "status=timeout");
    assert_eq!(file(&w, "state/cycles/7.timed"), "1 runner adversarial");
}

#[test]
fn a_runner_that_ends_at_the_time_limit_on_a_cap_of_its_own_or_with_an_end_is_not_a_timeout() {
    let w = limited("stop-self-end", "30");
    for end in ["says handoff deadline; exit 10", "says merged; exit 0"] {
        w.fresh();
        let _ = fs::remove_file(w.home.join("state/cycles/7.timed"));
        w.container(&format!(r#""$nap" 1.2; {end}"#));
        let r = w.work7();
        lacks(&r.out, "status=timeout");
        gone(&w, "state/cycles/7.timed");
    }
}

/// A stall of two seconds.
fn stalling(name: &str) -> World {
    let w = world(name);
    w.fresh();
    w.minimal(&["TEHAS_WORK_STALL=2", "TEHAS_VERIFY_TIMEOUT=1"]);
    w
}

#[test]
fn a_cycle_none_of_whose_logs_change_is_stopped_as_stalled_before_its_time_limit() {
    let w = stalling("stop-stalled");
    w.under_cycle();
    let _ = fs::remove_file(w.home.join("state/cycles/7.stalled"));
    w.container(&format!("{HOLD}; {GONE}"));
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    has(
        &r.out,
        "none of the cycle's logs changed for 2s, stopping it as stalled",
    );
    has(&r.out, "status=stalled");
    lacks(&r.out, "time limit");
    lacks(&r.out, "was killed");
    has(&w.calls("docker"), "stop -t 20 tehas-work-7");
    assert_eq!(file(&w, "state/cycles/7.stalled"), "2\n-");
    has(&r.out, "status=stalled step=- cap=stall");
    assert_eq!(live7(&w), json!([false, "stalled"]));
}

#[test]
fn a_stalled_runner_that_writes_its_result_as_it_is_stopped_is_still_stalled() {
    let w = stalling("stop-stalled-result");
    w.container(INTERRUPTED);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    has(&r.out, "status=stalled");
}

/// A stall of two seconds inside a limit of five, with no grace.
fn busy(name: &str) -> World {
    let w = world(name);
    w.fresh();
    w.minimal(&[
        "TEHAS_WORK_STALL=2",
        "TEHAS_VERIFY_TIMEOUT=1",
        "TEHAS_WORK_TIMEOUT=5",
        "TEHAS_CHILD_TIMEOUT=5",
    ]);
    w.env_ok();
    w.env("TEHAS_WORK_GRACE", "0");
    w
}

#[test]
fn a_cycle_whose_agents_keep_writing_is_not_stalled() {
    let w = busy("stop-agents-write");
    let _ = fs::remove_file(w.home.join("state/cycles/7.stalled"));
    w.container(&format!(
        r#"{HOLD}; ( while kill -0 "$(cat "$T/sleeper.pid")" 2>/dev/null; do date >>"{}/agent.jsonl"; "$nap" 0.3; done ) & {GONE}"#,
        w.runs.display()
    ));
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    has(&r.out, "time limit of 5s passed");
    lacks(&r.out, "stalled");
    gone(&w, "state/cycles/7.stalled");
}

#[test]
fn progress_the_runner_writes_keeps_a_cycle_from_being_stalled() {
    let w = busy("stop-progress");
    w.container(&format!(
        r#"{HOLD}; ( while kill -0 "$(cat "$T/sleeper.pid")" 2>/dev/null; do progress '{{"v":1,"kind":"log","message":"x"}}'; "$nap" 0.3; done ) & {GONE}"#
    ));
    let r = w.work7();
    has(&r.out, "time limit of 5s passed");
    lacks(&r.out, "stalled");
}

#[test]
fn term_stops_the_container_and_the_runners_result_says_interrupted() {
    let w = world("stop-term");
    w.fresh();
    w.under_cycle();
    w.container(INTERRUPTED);
    term_when(&w, &running_ready(&w), 100);
    kill_sleeper(&w);
    has(&w.calls("docker"), "stop -t 10 tehas-work-7");
    lacks(&w.read("work.out"), "was killed");
    assert_eq!(live7(&w), json!([false, "interrupted"]));
    // No gate ran, so none is closed. No step was seen, so none is named.
    assert_eq!(file(&w, "state/cycles/7.signal"), "TERM -");
    let mut logs = String::new();
    for e in fs::read_dir(w.home.join("logs")).unwrap().flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        if n.starts_with("work-7-") && n.ends_with(".jsonl") {
            logs.push_str(&fs::read_to_string(e.path()).unwrap());
        }
    }
    lacks(&logs, "verify-aborted");
}

#[test]
fn term_with_no_result_gives_interrupted() {
    let w = world("stop-term-none");
    w.fresh();
    w.under_cycle();
    w.container(&format!("{HOLD}; {GONE}"));
    let r = term_when(&w, &running_ready(&w), 100);
    kill_sleeper(&w);
    has(&r.out, "rc=2");
    let out = w.read("work.out");
    lacks(&out, "was killed");
    has(&out, "status=interrupted");
    assert_eq!(live7(&w), json!([false, "interrupted"]));
}

#[test]
fn a_term_inside_a_gate_leaves_the_signal_and_the_step_closes_the_gate_in_the_stream_and_says_so_on_the_console(
) {
    let w = world("stop-term-gate");
    w.fresh();
    w.under_cycle();
    w.container(&format!(
        r#"progress '{{"v":1,"kind":"step-started","event":{{"step":"develop"}}}}'; progress '{{"v":1,"kind":"verify-started","issue":7,"event":{{"kind":"verify-started","step":"develop","command":"make test"}}}}'; {INTERRUPTED}"#
    ));
    let ready = format!(
        "[ -e \"$T/sleeper.pid\" ] && grep -qs develop \"{}/console-7.txt.now\"",
        w.live.display()
    );
    term_when(&w, &ready, 200);
    kill_sleeper(&w);
    has(&w.read("work.out"), "status=interrupted step=- cap=signal");
    assert_eq!(file(&w, "state/cycles/7.signal"), "TERM develop");
    let log = fs::read_dir(w.home.join("logs"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            let n = p.file_name().unwrap().to_string_lossy();
            n.starts_with("work-7-") && n.ends_with(".jsonl")
        })
        .unwrap();
    let aborted: Vec<Value> = fs::read_to_string(log)
        .unwrap()
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|v| v["kind"] == "verify-aborted")
        .map(|v| {
            let e = &v["event"];
            json!([v["by"], e["step"], e["command"], e["status"], e["signal"]])
        })
        .collect();
    assert_eq!(
        aborted,
        vec![json!(["tehas", "develop", "make test", "aborted", "TERM"])]
    );
    has(
        &fs::read_to_string(w.live.join("console-7.txt")).unwrap(),
        "tehas work: SIGTERM came at step develop, so tehas stopped the container; the gate of step develop was left open and is closed as aborted",
    );
}

/// A TERM sent by the container to tehas work as soon as it starts.
fn term_at_start(name: &str) -> (World, work::Run) {
    let w = world(name);
    w.fresh();
    w.container(&format!(
        r#""$nap" 60 >/dev/null 2>&1 & echo $! >"$T/sleeper.pid"; kill -TERM $PPID; {GONE}"#
    ));
    let r = w.work7();
    kill_sleeper(&w);
    (w, r)
}

#[test]
fn a_term_as_soon_as_the_container_starts_is_caught_interrupted_and_the_container_stopped() {
    let (w, r) = term_at_start("stop-term-start");
    assert_eq!(r.rc, 2, "{}", r.out);
    has(&r.out, "status=interrupted");
    has(&w.calls("docker"), "stop -t 10 tehas-work-7");
}

#[test]
fn a_container_left_under_the_issues_name_is_removed_before_the_cycles_starts() {
    let (w, _) = term_at_start("stop-rm-first");
    let rm = docker_at(&w, |l| l == "rm -f tehas-work-7");
    let run = docker_at(&w, |l| l.contains("pi-rukas-headless work "));
    assert!(
        matches!((rm, run), (Some(a), Some(b)) if a < b),
        "{}",
        w.calls("docker")
    );
}

/// A bound of a second on docker stop, at a limit of a second.
fn stop_bound(name: &str) -> World {
    let w = world(name);
    w.fresh();
    w.minimal(&["TEHAS_WORK_TIMEOUT=1", "TEHAS_CHILD_TIMEOUT=1"]);
    w.env_ok();
    w.env("TEHAS_STOP_WAIT", "1");
    w.env("TEHAS_WORK_GRACE", "0");
    w
}

#[test]
fn a_docker_stop_that_does_not_end_is_given_up_after_its_bound_said_and_the_container_killed() {
    let w = stop_bound("stop-hangs");
    w.touch("stop-hangs");
    w.container(&format!("{HOLD}; says handoff; {GONE}"));
    let r = w.work7();
    w.rm("stop-hangs");
    assert_eq!(r.rc, 2, "{}", r.out);
    has(
        &r.out,
        "tehas work: docker stop of tehas-work-7 did not end within 1s, killing it",
    );
    has(&w.calls("docker"), "kill tehas-work-7");
    has(&r.out, "status=handoff");
}

#[test]
fn a_stop_that_ends_in_time_kills_nothing() {
    let w = stop_bound("stop-in-time");
    w.container(&format!("{HOLD}; says handoff; {GONE}"));
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    has(&r.out, "status=handoff");
    lacks(&w.calls("docker"), "kill ");
}

#[test]
fn a_docker_client_that_never_exits_is_killed_after_the_stop_wait_and_the_cycle_is_reported() {
    let w = world("stop-client-hangs");
    w.fresh();
    // A dockerd that stopped answering the attached run: only a KILL of the
    // client frees the cycle, and the container writes no result.
    w.docker_hangs();
    w.under_cycle();
    w.minimal(&["TEHAS_WORK_TIMEOUT=1", "TEHAS_CHILD_TIMEOUT=1"]);
    w.env_ok();
    w.env("TEHAS_STOP_WAIT", "1");
    w.env("TEHAS_WORK_GRACE", "0");
    w.container(r#""$nap" 30"#);
    let began = Instant::now();
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    has(
        &r.out,
        "tehas work: the docker client of tehas-work-7 did not end within 1s, killing it",
    );
    has(
        &r.out,
        "its container may still run and the poll's reap removes it",
    );
    has(&r.out, "status=timeout");
    assert_eq!(live7(&w), json!([false, "timeout"]));
    // The cycle is reported, not left running.
    assert_eq!(file(&w, "state/cycles/7.runner_exit"), "2");
    lacks(&r.out, "was killed");
    let secs = began.elapsed().as_secs();
    assert!(secs < 20, "the docker client was waited on for {secs}s");
}

/// The console of the tree is a stub with this body.
fn console_is(w: &World, body: &str) {
    w.stub("console", body);
    fs::rename(w.path("bin/console"), w.tree.join("libexec/console")).unwrap();
}

#[test]
fn a_console_that_does_not_stop_on_term_is_killed_after_its_bound_and_said() {
    let w = world("stop-console-hangs");
    w.fresh();
    // exec keeps the ignored TERM and leaves no child behind holding the lock.
    console_is(
        &w,
        r#"[ "$1" = --mask ] && exit 0; trap "" TERM; exec "$(command -pv sleep)" 30"#,
    );
    w.env_ok();
    w.env("TEHAS_CONSOLE_WAIT", "1");
    w.container("says handoff; exit 10");
    let start = Instant::now();
    let r = w.work7();
    assert_eq!(r.rc, 10, "{}", r.out);
    has(
        &r.out,
        "tehas work: the console did not stop within 1s, killing it",
    );
    has(&r.out, "status=handoff");
    let secs = start.elapsed().as_secs();
    assert!(secs < 20, "the console was waited on for {secs}s");
}

#[test]
fn a_console_that_stops_in_time_is_not_killed() {
    let w = world("stop-console-in-time");
    w.fresh();
    console_is(&w, "");
    w.env_ok();
    w.env("TEHAS_CONSOLE_WAIT", "1");
    w.container("says handoff; exit 10");
    let r = w.work7();
    lacks(&r.out, "the console did not stop");
    lacks(&r.out, "the console ended");
}

#[test]
fn a_push_check_that_does_not_end_is_given_up_after_its_bound_exit_3_no_cycle() {
    let w = world("stop-push-hangs");
    w.fresh();
    w.touch("push-hangs");
    w.env("TEHAS_PREFLIGHT_WAIT", "1");
    let began = Instant::now();
    let r = w.work7();
    w.rm("push-hangs");
    assert_eq!(r.rc, 3, "{}", r.out);
    has(&r.out, "the push check did not end within 1s");
    assert!(w.cycle_args().is_empty());
    assert!(began.elapsed().as_secs() < 20);
}

// A sandbox that cannot push is a fault of the factory: the host records it,
// the start ends as push-fault, and the issue gets the short comment of a
// start that began nothing, with its labels left alone.

/// A start under tehasbot cycle whose push check git refuses (`PUSH_RC`).
fn refused(name: &str, store_rc: i32, line: &str, push_rc: &str) -> World {
    let w = world(name);
    w.fresh();
    w.under_cycle();
    w.push_fault_store(store_rc, line);
    w.env("PUSH_RC", push_rc);
    w
}

#[test]
fn a_push_check_the_sandbox_refused_records_the_fault_ends_the_start_as_push_fault_and_posts_no_retry_boxes(
) {
    let w = refused("stop-push-fault", 0, "", "1");
    let r = w.work7();
    assert_eq!(r.rc, 3, "{}", r.out);
    assert!(w.cycle_args().is_empty());
    has(
        &w.store_lines(),
        "cycle-end 7 --attempt 1 --status push-fault",
    );
    // The store closed the cycle and kept the word of its end in endings.
    let c = w.row(&["cycle-show", "7"]);
    assert!(!c["cycle"]["ended_at"].is_null());
    assert_eq!(c["cycle"]["attempt"], 1);
    assert_eq!(w.push_fault_ended(7), "push-fault");
    // One host file: the time, the account the cycle pushes as, git's line.
    let parts = w.push_fault_parts();
    let parts: Vec<&str> = parts.lines().collect();
    assert_eq!(parts.first(), Some(&"true"));
    assert_eq!(parts.get(1), Some(&w.cycle_account().as_str()));
    assert_eq!(parts.get(2), Some(&PUSH_SAYS));
    let v: Value =
        serde_json::from_str(&file(&w, "state/push-fault.json")).expect("the fault file");
    let mut keys: Vec<&String> = v.as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(keys, ["at", "line", "login"]);
    // No park: no retry box, no question and no label change.
    let gh = w.gh();
    has(
        &gh,
        "issue comment 7 --repo acme/widget --body Tehas started no cycle: the sandbox cannot push to the repository, which is a fault of this host. To retry, apply `tehas:ready`.",
    );
    assert_eq!(gh.lines().filter(|l| l.contains("issue edit")).count(), 0);
    lacks(&gh, "Retry with the issue");
    lacks(&gh, "tehas:question");
    lacks(&gh, "needs-human-attention");
}

#[test]
fn a_push_fault_is_recorded_even_when_the_store_will_not_take_the_end_of_the_start() {
    let w = refused(
        "stop-push-fault-busy",
        1,
        "store: busy #7: the database is locked",
        "1",
    );
    let r = w.work7();
    assert_eq!(r.rc, 3, "{}", r.out);
    assert_eq!(w.push_fault_parts().lines().nth(2), Some(PUSH_SAYS));
    has(&r.out, "the push fault of #7 is not on its record");
    has(
        &w.gh(),
        "Tehas started no cycle: the sandbox cannot push to the repository",
    );
}

#[test]
fn a_push_check_that_passes_after_a_refused_one_takes_the_fault_off_the_host() {
    let w = refused("stop-push-fault-passes", 0, "", "1");
    let r = w.work7();
    assert_eq!(r.rc, 3, "{}", r.out);
    assert_eq!(w.push_fault_parts().lines().nth(2), Some(PUSH_SAYS));
    // The next start of the same attempt, with the refusal taken away.
    w.env_unset("PUSH_RC");
    let a = w.attempt(7).to_string();
    w.env("TEHAS_CYCLE_ATTEMPT", &a);
    w.work7();
    assert_eq!(w.push_fault_file(), "");
}

#[test]
fn a_push_check_docker_could_not_start_records_no_fault_and_stops_the_start_with_78() {
    let w = refused("stop-push-125", 0, "", "125");
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    assert!(w.cycle_args().is_empty());
    has(
        &r.out,
        "the sandbox could not be asked whether it can push (exit 125), not starting a cycle: docker: Error response from daemon",
    );
    assert_eq!(w.push_fault_file(), "");
    lacks(&w.store_lines(), "push-fault");
    lacks(&w.gh(), "the sandbox cannot push");
}

/// TERM to tehas work once the stub of docker holds a step, then a wait for
/// that step to see it.
fn stop_in_prepare(w: &World) {
    w.rm("slow-began");
    w.sh(&format!(
        "\"{work}\" 7 >\"$T/work.out\" 2>&1 & pid=$!
for _ in $(seq 100); do [ -e \"$T/slow-began\" ] && break; \"$(command -pv sleep)\" 0.1; done
kill -TERM $pid; wait $pid
for _ in $(seq 50); do grep -q 'stopped, not going on' \"$T/work.out\" && break; \"$(command -pv sleep)\" 0.1; done",
        work = w.work_cmd().display()
    ));
    has(&w.read("work.out"), "stopped, not going on with the start");
    assert!(w.cycle_args().is_empty());
    assert!(
        !w.home.join("state/cycles/7.prepared").exists(),
        "the plan of the start outlived the stop"
    );
}

#[test]
fn a_stop_during_the_workspace_step_ends_it_after_the_command_it_is_in() {
    let w = world("stop-in-workspace");
    w.fresh();
    w.touch("push-slow");
    stop_in_prepare(&w);
    w.rm("push-slow");
    assert!(
        !w.state.join("models.json").exists(),
        "the step went on after the stop"
    );
    lacks(&w.read("work.out"), "the code index");
}

#[test]
fn a_stop_during_the_record_step_ends_it_after_the_command_it_is_in() {
    let w = world("stop-in-record");
    w.fresh();
    w.under_cycle();
    w.touch("rm-slow");
    stop_in_prepare(&w);
    w.rm("rm-slow");
    lacks(&w.calls("docker"), "image inspect");
}
