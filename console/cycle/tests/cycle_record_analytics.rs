//! How a cycle calls the analytics program, and when it stays silent about it.

mod common;

use common::{has, lacks, World};
use regex::Regex;
use serde_json::{json, Value};
use std::time::Instant;

const RUKAS: &str = "0123456789abcdef0123456789abcdef01234567";

fn image_id() -> String {
    format!("sha256:{}", "e".repeat(64))
}

/// The analytics program, where install.sh puts it. Each call notes how many
/// history lines the issue had and whether the status was refreshed yet, and
/// a run keeps its facts.
fn analytics_stub(w: &World, more: &str) {
    w.stub_analytics(&format!(
        r#"printf "%s %s\n" "$("$T/store-bin" history-show 7 | jq ".rows | length")" \
  "$([ -e "$T/calls/fake-status" ] && echo late || echo early)" >>"$T/calls/analytics.moment"
[ "$1 $3" != "run --facts" ] || cp "$4" "$T/calls/analytics.facts"
{more}"#
    ));
}

/// The calls, without the facts file's name, which is new each time.
fn analytics_calls(w: &World) -> String {
    let re = Regex::new(r" --facts [^ ]*").unwrap();
    re.replace_all(w.calls("analytics").trim_end(), "")
        .to_string()
}

fn facts(w: &World) -> Value {
    serde_json::from_str(&w.calls("analytics.facts")).unwrap()
}

#[test]
fn the_run_in_analytics_carries_when_its_image_was_built_as_docker_says_of_the_id_on_record() {
    let w = World::new("an-image");
    analytics_stub(&w, "");
    let id = image_id();
    w.fx("image.label", "2026-10-29T14:44:16.123456789Z\n");
    w.work(&format!(
        r#""$T/store-bin" cycle-image 7 --attempt "$TEHAS_CYCLE_ATTEMPT" --image {id} >/dev/null; exit 2"#
    ));
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert_eq!(
        analytics_calls(&w).lines().next().unwrap(),
        format!(
            "run 7 --image {id} --image-built 2026-10-29T14:44:16.123456789Z --oom 0 --forks 0"
        )
    );
    has(
        &w.calls("docker"),
        &format!("image inspect --format {{{{.Created}}}} {id}"),
    );
}

#[test]
fn the_cycle_records_its_run_and_its_end_in_analytics_after_the_history_line_with_what_it_read_and_removed(
) {
    let w = World::new("an-run");
    analytics_stub(&w, "");
    let id = image_id();
    w.work(&format!(
        r#""$T/store-bin" cycle-image 7 --attempt "$TEHAS_CYCLE_ATTEMPT" --image {id} --rukas {RUKAS} >/dev/null
echo 3 >"$TEHAS_HOME/state/cycles/7.oom"; echo 2 >"$TEHAS_HOME/state/cycles/7.forks"; exit 2"#
    ));
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert_eq!(
        analytics_calls(&w),
        format!("run 7 --image {id} --rukas-image {RUKAS} --oom 3 --forks 2\nevent 7 ended")
    );
    assert_eq!(w.calls("analytics.moment"), "1 early\n1 early\n");
    let f = facts(&w);
    assert_eq!(
        json!([
            f["line"]["rc"],
            f["line"]["status"],
            f["line"]["how"],
            f["line"]["parked"],
            f["attempt"],
            f["rukas"]
        ]),
        json!([2, "unknown", "hand", true, 1, RUKAS])
    );
    let h = w.history0();
    assert_eq!(
        json!([h["rc"], h["status"], h["how"], h["parked"]]),
        json!([2, "unknown", "hand", true])
    );
    lacks(&r.out, "analytics");
}

#[test]
fn an_option_with_no_value_is_left_out_of_the_run() {
    let w = World::new("an-novalue");
    analytics_stub(&w, "");
    w.ended("handoff", "commit-pr", "feature/x", "");
    w.run7();
    assert_eq!(
        analytics_calls(&w),
        "run 7 --oom 0 --forks 0\nevent 7 ended"
    );
}

#[test]
fn a_cycle_stopped_by_a_signal_is_recorded_too() {
    let w = World::new("an-signal");
    analytics_stub(&w, "");
    w.work(r#"kill -TERM "$PPID"; sleep 1; exit 143"#);
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert_eq!(
        analytics_calls(&w),
        "run 7 --oom 0 --forks 0\nevent 7 ended"
    );
}

#[test]
fn a_start_that_never_ran_gives_analytics_the_approval_it_used_though_the_record_goes_with_the_report(
) {
    let mut w = World::new("an-never");
    w.env("TEHAS_CYCLE_FROM", "unit");
    analytics_stub(&w, "");
    // 2026-01-01T00:00:00Z
    w.approve("alice", "label", 1_767_225_600, &[]);
    w.work("exit 4");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    let f = facts(&w);
    assert_eq!(
        json!([f["approval"]["at"], f["approval"]["how"]]),
        json!(["2026-01-01T00:00:00Z", "label"])
    );
    assert_eq!(w.row(&["approval-current", "7"])["current"], Value::Null);
}

#[test]
fn with_tehas_analytics_off_the_cycle_calls_no_analytics() {
    // The console gives the cycle what instance.env says.
    let mut w = World::new("an-off");
    analytics_stub(&w, "");
    w.env("TEHAS_ANALYTICS", "off");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert_eq!(w.calls("analytics"), "");
}

#[test]
fn a_failing_analytics_is_a_warning_and_the_cycle_is_reported_as_without_it() {
    let w = World::new("an-without");
    w.ended("handoff", "commit-pr", "feature/x", "");
    w.run7();
    let without = w.gh();
    let w = World::new("an-failing");
    w.ended("handoff", "commit-pr", "feature/x", "");
    analytics_stub(&w, "exit 70");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    has(&r.out, "WARN analytics run: exit 70");
    has(&r.out, "WARN analytics event: exit 70");
    has(&r.out, "done #7 rc=10 status=handoff");
    assert_eq!(w.gh(), without);
    assert_eq!(w.calls("fake-status").lines().count(), 1);
    assert_eq!(
        w.row(&["history-show", "7"])["rows"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn an_analytics_that_hangs_is_cut_off_and_the_cycle_is_reported() {
    let mut w = World::new("an-hangs");
    analytics_stub(&w, "exec sleep 30");
    w.env("TEHAS_ANALYTICS_WAIT", "1");
    let before = Instant::now();
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert!(before.elapsed().as_secs() < 15);
    // GNU timeout reports the TERM as 124; uutils reports the kill that
    // follows as 137.
    for sub in ["run", "event"] {
        let re = Regex::new(&format!("WARN analytics {sub}: exit (124|137)")).unwrap();
        assert!(
            re.is_match(&r.out),
            "no time limit warning for {sub} in: {}",
            r.out
        );
    }
    assert_eq!(w.calls("fake-status").lines().count(), 1);
}

#[test]
fn a_host_without_the_analytics_program_is_silent() {
    let w = World::new("an-none");
    assert!(!w.home.join("libexec/analytics").exists());
    let r = w.run7();
    assert_eq!(r.rc, 0);
    lacks(&r.out, "analytics");
}
