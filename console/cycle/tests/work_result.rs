//! How the runner's end is reported by tehas work, from its exit status, its
//! result and its progress.

mod work;

use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use work::{has, lacks, World, GONE, HOLD};

/// The log a run names in its line `tehas work: issue 7 started, log <path>`.
fn log_of(out: &str) -> String {
    out.lines()
        .find_map(|l| l.strip_prefix("tehas work: issue 7 started, log "))
        .unwrap_or_else(|| panic!("no log named in:\n{out}"))
        .to_string()
}

/// What the error file beside the log holds, "" for none.
fn err_of(out: &str) -> String {
    fs::read_to_string(format!("{}.err", log_of(out)))
        .unwrap_or_default()
        .trim_end_matches('\n')
        .to_string()
}

fn base(log: &str) -> String {
    Path::new(log)
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

fn json_of(p: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(p).unwrap()).unwrap()
}

fn no_nostart(w: &World) {
    assert!(!w.home.join("state/cycles/7.nostart").exists());
}

/// The words of the cycle's docker call, one line.
fn cycle_line(w: &World) -> String {
    w.cycle_args().join(" ")
}

fn done_line(key: &str, role: &str) -> String {
    format!(
        r#"{{"v":1,"kind":"dispatch-completed","key":"{key}","event":{{"role":"{role}","ok":true,"usage":{{"turns":2,"input":10,"output":4,"cacheRead":1}}}}}}"#
    )
}

#[test]
fn the_runners_exit_status_is_the_commands_when_it_wrote_its_result() {
    let w = World::new("result-exit");
    for code in [0, 10, 11, 20, 21, 30, 40] {
        w.fresh();
        w.container(&format!("says handoff; exit {code}"));
        let r = w.work7();
        assert_eq!(r.rc, code, "{}", r.out);
        has(
            &r.out,
            &format!("status=handoff step=develop cap=- runner_exit={code}"),
        );
    }
}

#[test]
fn a_runner_that_ends_without_a_result_is_exit_2_whatever_its_status() {
    let w = World::new("result-none");
    w.fresh();
    w.container("exit 10");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    has(&r.out, "status=running");
}

#[test]
fn a_container_that_ends_with_no_result_and_no_line_of_its_stream_leaves_the_note_of_a_runner_that_never_started_and_one_line_in_the_error_file(
) {
    let w = World::new("result-nostart");
    w.fresh();
    w.container("exit 1");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    let log = log_of(&r.out);
    assert_eq!(
        fs::read_to_string(w.home.join("state/cycles/7.nostart")).unwrap(),
        "1\n"
    );
    assert_eq!(
        err_of(&r.out),
        format!(
            "tehas work: issue 7 ended with the container's exit 1 and no line of its stream: the runner wrote no event, so no step started. Its preflight is {}.preflight.",
            base(&log)
        )
    );
}

#[test]
fn a_container_that_wrote_one_line_of_its_stream_is_not_a_runner_that_never_started() {
    let w = World::new("result-one-line");
    for line in [
        r#"progress '{"v":1,"kind":"step-started","event":{"step":"explore"}}'"#,
        "progress 'not json at all, but the stream is no longer empty'",
    ] {
        w.fresh();
        w.container(&format!("{line}; exit 1"));
        let r = w.work7();
        assert_eq!(r.rc, 2, "{}", r.out);
        no_nostart(&w);
        assert_eq!(err_of(&r.out), "");
    }
}

#[test]
fn a_container_that_wrote_a_result_the_reader_refuses_is_not_a_runner_that_never_started() {
    let w = World::new("result-refused");
    for shape in [
        "not json",
        r#"{"schemaVersion":4,"issues":[]}"#,
        r#"{"schemaVersion":5,"issues":[{"issue":8}]}"#,
        r#"{"schemaVersion":5,"issues":[{"issue":7,"handoff":"x"}]}"#,
        r#"{"schemaVersion":5,"issues":[{"issue":7,"usage":"x"}]}"#,
    ] {
        w.fresh();
        w.container(&format!("printf '%s\\n' '{shape}'; exit 1"));
        let r = w.work7();
        assert_eq!(r.rc, 2, "{shape}: {}", r.out);
        no_nostart(&w);
        assert_eq!(err_of(&r.out), "");
    }
}

#[test]
fn a_killed_container_one_tehas_stopped_and_one_that_wrote_its_result_leave_no_note_of_a_runner_that_never_started(
) {
    let w = World::new("result-no-note");
    w.fresh();
    w.container("exit 137");
    let r = w.work7();
    assert_eq!(r.rc, 137, "{}", r.out);
    no_nostart(&w);
    w.fresh();
    w.container("says handoff; exit 10");
    let r = w.work7();
    assert_eq!(r.rc, 10, "{}", r.out);
    no_nostart(&w);
    // A container tehas stopped: the account of its end is its stop file.
    w.fresh();
    w.minimal(&["TEHAS_WORK_TIMEOUT=1", "TEHAS_CHILD_TIMEOUT=1"]);
    w.env_ok();
    w.env("TEHAS_WORK_GRACE", "0");
    w.container(&format!("{HOLD}; {GONE}"));
    let r = w.work7();
    has(&r.out, "time limit of 1s passed");
    no_nostart(&w);
    w.fresh();
    w.minimal(&["TEHAS_WORK_STALL=2", "TEHAS_VERIFY_TIMEOUT=1"]);
    w.env_ok();
    w.container(&format!("{HOLD}; {GONE}"));
    let r = w.work7();
    has(&r.out, "stopping it as stalled");
    no_nostart(&w);
}

#[test]
fn a_result_without_an_exit_code_of_its_own_is_no_success_exit_2() {
    let w = World::new("result-no-code");
    for code in [1, 137] {
        w.fresh();
        w.container(&format!(
            "says merged | jq -c 'del(.exitCode, .issues[0].exitCode)'; exit {code}"
        ));
        let r = w.work7();
        assert_eq!(r.rc, 2, "{code}: {}", r.out);
    }
}

#[test]
fn a_result_with_no_issue_entry_keeps_the_runners_own_exit_a_refusal_is_40_and_is_no_kill() {
    let w = World::new("result-no-entry");
    for code in [30, 40] {
        w.fresh();
        w.container(&format!(
            r#"echo '{{"schemaVersion":5,"exitCode":{code},"issues":[],"error":{{"code":"refused"}}}}'; exit {code}"#
        ));
        let r = w.work7();
        assert_eq!(r.rc, code, "{}", r.out);
    }
    w.fresh();
    w.container(r#"echo '{"schemaVersion":5,"issues":[]}'; exit 137"#);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    lacks(&r.out, "was killed");
}

#[test]
fn a_result_of_another_schema_or_for_another_issue_is_no_result() {
    let w = World::new("result-other");
    w.fresh();
    w.container(r#"says merged | sed "s/\"schemaVersion\":5/\"schemaVersion\":3/"; exit 0"#);
    assert_eq!(w.work7().rc, 2);
    w.fresh();
    w.container(r#"says merged | sed "s/\"issue\":7/\"issue\":8/"; exit 0"#);
    assert_eq!(w.work7().rc, 2);
}

#[test]
fn the_result_is_kept_for_tehasbot_cycle_and_beside_the_log_and_the_cap_is_said() {
    let w = World::new("result-kept");
    w.fresh();
    w.under_cycle();
    w.container("says handoff verify-failed:develop; exit 10");
    let r = w.work7();
    assert_eq!(r.rc, 10, "{}", r.out);
    assert_eq!(
        json_of(&w.result_file)["issues"][0]["cap"],
        "verify-failed:develop"
    );
    let log = log_of(&r.out);
    assert_eq!(
        json_of(Path::new(&format!("{log}.result.json")))["issues"][0]["cap"],
        "verify-failed:develop"
    );
    has(
        &r.out,
        "status=handoff step=develop cap=verify-failed:develop runner_exit=10",
    );
    let v = json_of(&w.live.join("cycles/7.json"));
    assert_eq!(
        json!([v["running"], v["status"]]),
        json!([false, "handoff"])
    );
}

#[test]
fn a_cycle_that_ended_at_a_cap_with_nothing_in_its_error_stream_leaves_the_cap_there_as_no_evidence_with_the_streams_name(
) {
    let w = World::new("result-cap");
    w.fresh();
    w.container("says handoff verify-failed:develop; exit 10");
    let r = w.work7();
    assert_eq!(r.rc, 10, "{}", r.out);
    let err = err_of(&r.out);
    assert_eq!(
        err,
        format!(
            "tehas work: issue 7 ended at the cap verify-failed:develop: no evidence from the driver\ntehas work: the driver's diagnostics are in {}",
            base(&log_of(&r.out))
        )
    );
    lacks(&err, "holds nothing");
    lacks(&err, "the cap is all the record says");
}

#[test]
fn the_reason_the_result_gives_for_the_cap_is_said_with_it() {
    let w = World::new("result-reason");
    w.fresh();
    w.container(r#"says2 handoff verify-failed:develop '{"kind":"provider","providerMessage":"502 Bad Gateway"}'; exit 10"#);
    let r = w.work7();
    assert_eq!(r.rc, 10, "{}", r.out);
    assert_eq!(
        err_of(&r.out),
        format!(
            "tehas work: issue 7 ended at the cap verify-failed:develop: 502 Bad Gateway\ntehas work: the driver's diagnostics are in {}",
            base(&log_of(&r.out))
        )
    );
}

#[test]
fn the_pi_rukas_warn_kill_and_failure_lines_of_the_cycles_stream_are_copied_into_its_error_file() {
    let w = World::new("result-driver-lines");
    w.fresh();
    w.container(
        "progress '[pi-rukas] WARN work-driver: plan to-spec gate, halting before branch'
progress '[pi-rukas] killed after 7200000ms timeout, after 71620 lines'
progress '[pi-rukas] work-driver: exec error tail is unattributed, no FAILED marker found'
progress '[pi-rukas] a line with none of the three words'
progress 'WARN work-driver: not from the driver, so not copied'
says handoff developer-timeout; exit 10",
    );
    let r = w.work7();
    assert_eq!(r.rc, 10, "{}", r.out);
    let err = err_of(&r.out);
    assert_eq!(
        err,
        format!(
            "tehas work: issue 7 ended at the cap developer-timeout: no evidence from the driver
[pi-rukas] WARN work-driver: plan to-spec gate, halting before branch
[pi-rukas] killed after 7200000ms timeout, after 71620 lines
[pi-rukas] work-driver: exec error tail is unattributed, no FAILED marker found
tehas work: the driver's diagnostics are in {}",
            base(&log_of(&r.out))
        )
    );
    lacks(&err, "holds nothing");
    lacks(&err, "the cap is all the record says");
}

#[test]
fn a_cycle_the_runner_aborted_with_no_cap_leaves_the_abort_in_its_error_file() {
    let w = World::new("result-aborted");
    w.fresh();
    w.container(
        "progress '[pi-rukas] cancelled (abort signal) · last output: tool_execution_update 610s before the kill, after 249 line(s)'
says aborted; exit 20",
    );
    let r = w.work7();
    assert_eq!(r.rc, 20, "{}", r.out);
    assert_eq!(
        err_of(&r.out),
        format!(
            "tehas work: issue 7 ended as aborted with no cap
[pi-rukas] cancelled (abort signal) · last output: tool_execution_update 610s before the kill, after 249 line(s)
tehas work: the driver's diagnostics are in {}",
            base(&log_of(&r.out))
        )
    );
}

#[test]
fn an_aborted_cycle_whose_stream_holds_no_driver_line_still_names_the_stream() {
    let w = World::new("result-aborted-bare");
    w.fresh();
    w.container("progress 'not a driver line'\nsays aborted; exit 20");
    let r = w.work7();
    assert_eq!(r.rc, 20, "{}", r.out);
    let log = log_of(&r.out);
    assert_eq!(
        err_of(&r.out),
        format!(
            "tehas work: issue 7 ended as aborted with no cap\ntehas work: the driver's diagnostics are in {}",
            base(&log)
        )
    );
    assert_eq!(fs::metadata(format!("{log}.err")).unwrap().len(), 120);
}

#[test]
fn only_the_last_20_pi_rukas_lines_of_the_stream_are_copied() {
    let w = World::new("result-last-20");
    w.fresh();
    let mut lines = String::new();
    for n in 1..=25 {
        lines.push_str(&format!(
            "progress '[pi-rukas] WARN work-driver: repetition {n}'\n"
        ));
    }
    w.container(&format!("{lines}says handoff developer-timeout; exit 10"));
    let r = w.work7();
    assert_eq!(r.rc, 10, "{}", r.out);
    let err = err_of(&r.out);
    assert_eq!(
        err.lines().filter(|l| l.starts_with("[pi-rukas]")).count(),
        20
    );
    has(&err, "repetition 25");
    lacks(&err, "repetition 4");
    lacks(&err, "repetition 5");
}

#[test]
fn a_pi_rukas_line_of_the_stream_is_copied_only_as_far_as_1000_characters_of_it() {
    let w = World::new("result-1000");
    w.fresh();
    let fat = "e".repeat(5000);
    w.container(&format!(
        "progress '[pi-rukas] killed after 7200000ms timeout {fat} ends here'\nsays handoff developer-timeout; exit 10"
    ));
    let r = w.work7();
    assert_eq!(r.rc, 10, "{}", r.out);
    let err = err_of(&r.out);
    let widest = err
        .lines()
        .filter(|l| l.starts_with("[pi-rukas]"))
        .map(|l| l.chars().count())
        .max();
    assert_eq!(widest, Some(1000));
    lacks(&err, "ends here");
    // The line of the cap (80), the copied line (1000), the stream's name (47),
    // each with its end of line.
    let log = log_of(&r.out);
    assert_eq!(fs::metadata(format!("{log}.err")).unwrap().len(), 1156);
}

#[test]
fn the_copied_lines_of_the_stream_together_stay_under_64_kib_of_the_record() {
    let w = World::new("result-64k");
    w.fresh();
    let fat = "e".repeat(1000);
    let mut lines = String::new();
    for n in 1..=100 {
        lines.push_str(&format!(
            "progress '[pi-rukas] WARN work-driver: repetition {n} {fat}'\n"
        ));
    }
    w.container(&format!("{lines}says handoff developer-timeout; exit 10"));
    let r = w.work7();
    assert_eq!(r.rc, 10, "{}", r.out);
    let err = err_of(&r.out);
    assert_eq!(
        err.lines().filter(|l| l.starts_with("[pi-rukas]")).count(),
        20
    );
    // 20 lines of 1000, the line of the cap (81), the stream's name (47).
    let log = log_of(&r.out);
    assert_eq!(fs::metadata(format!("{log}.err")).unwrap().len(), 20175);
    has(&err, "repetition 100");
}

#[test]
fn the_reason_of_the_cap_is_kept_to_what_the_park_comment_can_hold() {
    let w = World::new("result-reason-cut");
    w.fresh();
    let long = "r".repeat(2100);
    w.container(&format!(
        r#"says2 handoff plan-spec-mismatch '{{"kind":"provider","providerMessage":"{long}"}}'; exit 10"#
    ));
    let r = w.work7();
    assert_eq!(r.rc, 10, "{}", r.out);
    let err = err_of(&r.out);
    // Read as tehasbot cycle reads it for the park comment: 2000 characters,
    // the last six of them ` (cut)`.
    let first = err.lines().next().unwrap();
    assert_eq!(first.len() + 1, 2058);
    assert_eq!(
        first,
        format!(
            "tehas work: issue 7 ended at the cap plan-spec-mismatch: {} (cut)",
            "r".repeat(1994)
        )
    );
    assert_eq!(err.lines().count(), 2);
}

#[test]
fn a_cycle_that_ended_with_no_cap_leaves_its_error_stream_as_it_is() {
    let w = World::new("result-no-cap");
    w.fresh();
    w.container("says handoff; exit 10");
    let r = w.work7();
    assert_eq!(r.rc, 10, "{}", r.out);
    assert!(!Path::new(&format!("{}.err", log_of(&r.out))).exists());
}

#[test]
fn a_result_left_by_an_earlier_cycle_is_gone_before_the_container_starts() {
    let w = World::new("result-left");
    w.fresh();
    fs::create_dir_all(w.result_file.parent().unwrap()).unwrap();
    fs::write(
        &w.result_file,
        "{\"schemaVersion\":5,\"issues\":[{\"issue\":7,\"status\":\"merged\"}]}\n",
    )
    .unwrap();
    w.container("exit 0");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(fs::read_to_string(&w.result_file).unwrap(), "");
}

#[test]
fn the_progress_of_a_run_whose_runner_wrote_no_result_gives_the_usage_of_the_stable_dispatch_lines_each_key_once_and_the_last_caps_reason(
) {
    let w = World::new("result-progress");
    w.fresh();
    let dev1 = done_line("7:1:1", "developer");
    let dev2 = done_line("7:2:1", "developer");
    let ops = done_line("7:3:1", "ops x;");
    w.container(&format!(
        r#"progress '{dev1}'; progress '{dev1}'; progress '{dev2}'
progress '{ops}'
progress '{{"v":1,"kind":"dispatch-completed","unstable":true,"key":"7:4:1","event":{{"role":"liar","usage":{{"turns":99}}}}}}'
progress 'not json'
progress '{{"v":1,"kind":"dispatch-failed-provider","unstable":true,"event":{{"providerMessage":"502 Bad Gateway"}}}}'
exit 30"#
    ));
    w.work7();
    let p = json_of(&w.progress_file);
    assert_eq!(
        p["roles"],
        json!([{"role":"developer","dispatches":2,"turns":4,"input":20,"output":8,"cacheRead":2},{"role":"opsx","dispatches":1,"turns":2,"input":10,"output":4,"cacheRead":1}])
    );
    assert_eq!(p["reason"], "502 Bad Gateway");
    assert_eq!(p["from"], json!({"roles":"progress","failure":"progress"}));
}

#[test]
fn a_container_that_prints_nothing_leaves_an_empty_result_which_is_no_result_progress_not_a_bad_result(
) {
    let w = World::new("result-empty");
    w.fresh();
    w.container("exit 1");
    w.work7();
    assert_eq!(fs::read_to_string(&w.result_file).unwrap(), "");
    let p = json_of(&w.progress_file);
    assert_eq!(
        json!([p["roles"], p["failure"], p["from"]]),
        json!([[], null, {"roles":"progress","failure":"progress"}])
    );
}

#[test]
fn a_result_of_another_schema_is_a_bad_result() {
    let w = World::new("result-bad");
    w.fresh();
    w.container(r#"says merged | sed "s/\"schemaVersion\":5/\"schemaVersion\":3/"; exit 0"#);
    w.work7();
    assert_eq!(
        json_of(&w.progress_file)["from"],
        json!({"roles":"bad-result","failure":"bad-result"})
    );
}

#[test]
fn the_usage_per_role_and_the_failure_of_a_schema_5_run_come_from_its_result() {
    let w = World::new("result-usage");
    w.fresh();
    let liar = done_line("7:1:1", "liar");
    w.container(&format!(
        r#"progress '{liar}'
progress '{{"v":1,"kind":"dispatch-failed-provider","unstable":true,"event":{{"providerMessage":"from the progress"}}}}'
echo '{{"schemaVersion":5,"buildCommit":null,"exitCode":10,"issues":[{{"issue":7,"status":"handoff","cap":"step-failed:develop","failure":{{"kind":"provider","message":"m","providerMessage":"503 from the result"}},"usage":{{"dispatches":1,"byRole":{{"developer":{{"input":3,"output":1,"cacheRead":0,"turns":2,"dispatches":1}}}}}},"exitCode":10}}]}}'
exit 10"#
    ));
    let r = w.work7();
    assert_eq!(r.rc, 10, "{}", r.out);
    let p = json_of(&w.progress_file);
    assert_eq!(
        p["roles"],
        json!([{"role":"developer","dispatches":1,"turns":2,"input":3,"output":1,"cacheRead":0}])
    );
    assert_eq!(
        json!([p["reason"], p["failure"]["kind"], p["from"]]),
        json!(["503 from the result", "provider", {"roles":"result","failure":"result"}])
    );
}

#[test]
fn a_container_that_exits_137_with_no_result_is_reported_as_killed_and_the_command_exits_137() {
    let w = World::new("result-137");
    w.fresh();
    w.under_cycle();
    w.container("exit 137");
    let r = w.work7();
    assert_eq!(r.rc, 137, "{}", r.out);
    has(
        &r.out,
        "tehas work: the container was killed (137) while the cycle was running: out of memory, or stopped from outside",
    );
    has(&r.out, "status=killed");
    let v = json_of(&w.live.join("cycles/7.json"));
    assert_eq!(json!([v["running"], v["status"]]), json!([false, "killed"]));
    lacks(&w.calls("docker"), "stop ");
}

#[test]
fn a_137_after_the_runner_wrote_its_result_is_the_results_end_by_the_results_exit_code() {
    let w = World::new("result-137-after");
    w.fresh();
    w.container("says handoff; exit 137");
    let r = w.work7();
    assert_eq!(r.rc, 10, "{}", r.out);
    has(&r.out, "status=handoff");
    lacks(&r.out, "was killed");
}

#[test]
fn a_result_that_says_killed_is_the_runners_word_not_a_kill_no_kill_line() {
    let w = World::new("result-says-killed");
    w.fresh();
    w.container("says killed; exit 30");
    let r = w.work7();
    assert_eq!(r.rc, 30, "{}", r.out);
    has(&r.out, "status=killed");
    lacks(&r.out, "was killed");
}

#[test]
fn the_containers_input_is_nothing_no_fifo_no_terminal() {
    let w = World::new("result-stdin");
    w.fresh();
    w.container(r#"ls -l /proc/$$/fd/ >"$T/calls/container.fds" 2>&1; says handoff; exit 10"#);
    let r = w.work7();
    assert_eq!(r.rc, 10, "{}", r.out);
    let fds = w.calls("container.fds");
    let zero: String = fds
        .lines()
        .filter(|l| l.contains(" 0 -> "))
        .collect::<Vec<_>>()
        .join("\n");
    has(&zero, "/dev/null");
}

#[test]
fn the_runner_is_given_the_time_limit_in_whole_minutes_at_least_one() {
    let w = World::new("result-minutes");
    w.fresh();
    w.minimal(&["TEHAS_WORK_TIMEOUT=61", "TEHAS_CHILD_TIMEOUT=61"]);
    w.work7();
    has(&cycle_line(&w), "work 7 --timeout 2 --json");
    w.minimal(&["TEHAS_WORK_TIMEOUT=1", "TEHAS_CHILD_TIMEOUT=1"]);
    w.fresh();
    w.work7();
    has(&cycle_line(&w), "work 7 --timeout 1 --json");
}

#[test]
fn a_restart_is_passed_to_the_runner_as_its_flag() {
    let w = World::new("result-restart");
    w.fresh();
    w.work(&["7", "--restart"]);
    has(&cycle_line(&w), "work 7 --timeout 360 --json --restart");
}
