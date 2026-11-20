//! The containers and locks a poll clears, a start that does not come up, the
//! approval record it leaves, the analytics a start records, and the labels a
//! poll cannot change. The pure rules are the fixtures of
//! tests/fixtures/start.json.

mod common;

use common::*;
use serde_json::{json, Value};
use std::fs;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn approval_current(w: &World) -> Value {
    let v = w.row(&["approval-current", "7"]);
    json!([v["approval"], v["current"]])
}

/// #9 marked running with no cycle, and its container left when `left`.
fn marked_9(w: &World, left: bool) {
    w.fx("marked.json", r#"[{"number":9}]"#);
    if left {
        w.touch("orphan-tehas-work-9");
    }
}

#[test]
fn a_container_left_by_a_cycle_whose_lock_nobody_holds_is_removed_and_that_is_logged() {
    let w = World::new("sc-orphan");
    marked_9(&w, true);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&w.calls("docker"), "rm -f tehas-work-9");
    has(
        &r.out,
        "removed the container tehas-work-9: no cycle holds the lock of #9",
    );
}

#[test]
fn with_no_container_left_the_removal_says_nothing() {
    let w = World::new("sc-no-orphan");
    marked_9(&w, false);
    let r = w.poll();
    has(&w.calls("docker"), "rm -f tehas-work-9");
    lacks(&r.out, "removed the container");
}

#[test]
fn the_container_of_a_cycle_that_holds_its_lock_is_not_touched() {
    let w = World::new("sc-held");
    w.slots(2, &[]);
    marked_9(&w, true);
    w.hold(9);
    w.poll();
    lacks(&w.calls("docker"), "rm ");
    w.release();
}

#[test]
fn the_lock_file_of_a_cycle_that_ended_holds_nothing_back() {
    let w = World::new("sc-lock-file");
    w.home_file("state/cycles/9.lock", "");
    w.poll();
    assert_eq!(w.started(), "7 ");
}

#[test]
fn a_cycle_that_cannot_be_started_is_handed_to_a_human_exit_1() {
    let w = World::new("sc-start-fails");
    w.touch("start-fails");
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "ERROR: the cycle for #7 did not start");
    has(
        &w.gh(),
        "issue edit 7 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention",
    );
    has(&w.gh(), "could not start the cycle");
}

#[test]
fn a_cycle_that_never_says_it_holds_its_lock_is_handed_to_a_human() {
    let w = World::new("sc-silent");
    w.touch("start-silent");
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &w.gh(),
        "--remove-label tehas:running --add-label needs-human-attention",
    );
}

#[test]
fn a_cycle_that_holds_its_lock_but_wrote_no_marker_in_time_is_taken_as_started() {
    let w = World::new("sc-late");
    // The holder keeps the cycle's lock for a few seconds, its output detached.
    w.stub(
        "fake-late",
        r#"( exec 9>"$TEHAS_HOME/state/cycles/$1.lock"; flock -n 9; exec sleep 5 ) >/dev/null 2>&1 &
echo $! >"$T/late.pid""#,
    );
    w.env("TEHAS_CYCLE_START_CMD", "fake-late");
    w.env("TEHAS_CYCLE_START_WAIT", "1");
    let r = w.poll();
    let pid = fs::read_to_string(w.t().join("late.pid")).unwrap();
    let _ = std::process::Command::new("kill").arg(pid.trim()).status();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "WARN: the cycle for #7 holds its lock but did not say so in 1s; taking it as started",
    );
    lacks(&w.gh(), "needs-human-attention");
}

#[test]
fn the_start_of_an_earlier_cycle_on_record_does_not_count() {
    let w = World::new("sc-earlier");
    w.touch("start-silent");
    w.t_started(7, None, &["--by-hand"]);
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "ERROR: the cycle for #7 did not start");
    assert_eq!(w.attempt(7), 1);
    // The approval this poll recorded is taken off again: a unit that comes
    // up late finds none to use.
    assert_eq!(approval_current(&w), json!([null, null]));
}

/// The store answers the undo of an approval busy for its first `times`
/// calls, and is the real store otherwise; each call is in calls/store.
fn undo_busy(w: &World, times: u32) {
    let bin = w.home.join("libexec/store");
    let _ = fs::remove_file(&bin);
    let _ = fs::remove_file(w.t().join("undo.n"));
    let t = w.t().display().to_string();
    let text = format!(
        r#"#!/usr/bin/env bash
printf '%s\n' "$*" >>'{t}/calls/store'
case " $* " in *" --undo "*)
  n=$(($(cat '{t}/undo.n' 2>/dev/null || echo 0) + 1)); echo "$n" >'{t}/undo.n'
  if [ "$n" -le {times} ]; then echo 'store: busy: the database is locked' >&2; exit 1; fi ;;
esac
exec '{t}/store-bin' "$@"
"#
    );
    tehas_core::testing::write_exe(&bin, text).unwrap();
}

fn undos(w: &World) -> usize {
    w.calls("store")
        .lines()
        .filter(|l| l.contains("--undo"))
        .count()
}

#[test]
fn a_busy_store_is_asked_again_to_take_back_the_approval_of_a_start_that_did_not_come_up() {
    let w = World::new("sc-undo-busy");
    w.touch("start-silent");
    undo_busy(&w, 1);
    w.env("TEHAS_CYCLE_STOP_CMD", "fake-stop");
    w.stub("fake-stop", "");
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    assert_eq!(undos(&w), 2);
    assert_eq!(w.row(&["approval-current", "7"])["approval"], Value::Null);
    assert_eq!(w.calls("fake-stop"), "");
}

#[test]
fn when_the_approval_of_a_start_that_did_not_come_up_stays_on_record_the_unit_is_stopped() {
    let w = World::new("sc-undo-stays");
    w.touch("start-silent");
    undo_busy(&w, 9);
    w.env("TEHAS_CYCLE_STOP_CMD", "fake-stop");
    w.stub("fake-stop", "");
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    assert_eq!(undos(&w), 3);
    has(
        &r.out,
        "WARN: could not take the approval of #7 off the record (busy)",
    );
    assert_eq!(w.calls("fake-stop").trim_end(), "7");
    has(
        &w.gh(),
        "--remove-label tehas:running --add-label needs-human-attention",
    );
}

#[test]
fn a_cycle_that_refuses_its_number_after_its_start_still_counts_as_started() {
    let w = World::new("sc-refused");
    w.touch("start-refused");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    lacks(&r.out, "did not start");
    let c = &w.row(&["cycle-show", "7"])["cycle"];
    assert_eq!(
        json!([c["attempt"], !c["ended_at"].is_null(), c["end_reason"]]),
        json!([1, true, null])
    );
}

#[test]
fn a_cycle_that_ended_at_once_still_counts_as_started() {
    // fake-start writes the marker and is gone: no lock is held when the poll
    // looks.
    let w = World::new("sc-ended");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    lacks(&w.gh(), "needs-human-attention");
    lacks(&r.out, "did not start");
}

/// The analytics program where install.sh puts it. Each call notes what the
/// poll had done when it came: the approval record, the start, the tidy, and
/// how many times the status was refreshed.
fn analytics_stub(w: &World, more: &str) {
    w.stub(
        "analytics",
        &format!(
            r#"printf "%s:%s%s%s%s\n" "$*" \
  "$("$T/store-bin" approval-current 7 | jq -e ".approval != null or .current != null" >/dev/null && echo " approved")" \
  "$([ -e "$T/calls/fake-start" ] && echo " started")" \
  "$([ -e "$T/calls/fake-tidy" ] && echo " tidied")" \
  " status=$(wc -l <"$T/calls/fake-status" 2>/dev/null || echo 0)" >>"$T/calls/analytics.moment"
{more}"#
        ),
    );
    fs::create_dir_all(w.home.join("libexec")).unwrap();
    fs::rename(
        w.t().join("bin/analytics"),
        w.home.join("libexec/analytics"),
    )
    .unwrap();
}

fn analytics(w: &World) -> String {
    w.calls("analytics").trim_end().to_string()
}

#[test]
fn a_start_records_ready_once_the_approval_is_written_and_started_once_the_cycle_runs_the_poll_ends_with_a_sample(
) {
    let w = World::new("sc-an");
    analytics_stub(&w, "");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
    assert_eq!(analytics(&w), "event 7 ready\nevent 7 started\nsample");
    assert_eq!(
        w.calls("analytics.moment").trim_end(),
        "event 7 ready: approved status=1\nevent 7 started: approved started status=1\nsample: approved started tidied status=3"
    );
    lacks(&r.out, "analytics");
}

#[test]
fn ready_is_recorded_at_the_time_tehas_ready_was_accepted_as_the_approval_record_says_it() {
    let w = World::new("sc-an-at");
    analytics_stub(&w, "");
    w.fx(
        "events-7.json",
        r#"[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-01T00:00:00Z"}]"#,
    );
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.approval(".at", 7), "2026-01-01T00:00:00Z");
    assert_eq!(
        analytics(&w),
        "event 7 ready --at 1767225600\nevent 7 started\nsample"
    );
}

#[test]
fn an_approval_github_gave_no_time_for_is_on_record_at_the_polls_and_ready_has_none() {
    let w = World::new("sc-an-now");
    analytics_stub(&w, "");
    let before = now();
    w.poll();
    let at = w.row(&["approval-current", "7"])["current"]["at"]
        .as_i64()
        .unwrap();
    assert!(at >= before);
    assert_eq!(analytics(&w).lines().next(), Some("event 7 ready"));
}

#[test]
fn a_cycle_that_does_not_start_is_ready_and_not_started() {
    let w = World::new("sc-an-fails");
    analytics_stub(&w, "");
    w.touch("start-fails");
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    assert_eq!(analytics(&w), "event 7 ready\nsample");
}

#[test]
fn a_poll_that_starts_nothing_a_held_poll_and_a_paused_poll_take_no_sample_a_poll_github_fails_still_does(
) {
    let w = World::new("sc-an-none");
    analytics_stub(&w, "");
    w.fx("ready.json", "[]");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(analytics(&w), "");
    w.reset();
    analytics_stub(&w, "");
    w.no_slice();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    assert_eq!(analytics(&w), "");
    w.reset();
    analytics_stub(&w, "");
    w.fx("control.json", r#"{"labels":[{"name":"tehas:pause"}]}"#);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    assert_eq!(analytics(&w), "");
    // The read of GitHub fails before any write, so nothing marks the queue
    // changed: the sample is the next poll's.
    w.reset();
    analytics_stub(&w, "");
    w.rm_fx("repo.json");
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    assert_eq!(analytics(&w), "");
}

#[test]
fn a_dry_run_and_tehas_analytics_off_call_no_analytics() {
    let w = World::new("sc-an-off");
    analytics_stub(&w, "");
    let r = w.dispatch(&["--dry-run"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(analytics(&w), "");
    w.reset();
    analytics_stub(&w, "");
    w.slots(1, &["TEHAS_ANALYTICS=off"]);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
    assert_eq!(analytics(&w), "");
}

#[test]
fn a_failing_analytics_is_a_warning_and_the_poll_starts_its_cycle_and_ends_as_without_it() {
    let w = World::new("sc-an-exit");
    analytics_stub(&w, "exit 70");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
    has(&r.out, "WARN analytics event: exit 70");
    has(&r.out, "WARN analytics sample: exit 70");
    assert_eq!(
        r.out
            .lines()
            .filter(|l| l.contains("WARN analytics"))
            .count(),
        3
    );
    w.reset();
    analytics_stub(&w, "exit 70");
    w.rm_fx("repo.json");
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
}

#[test]
fn an_analytics_that_hangs_is_cut_off_and_the_poll_goes_on() {
    let w = World::new("sc-an-hangs");
    analytics_stub(&w, "exec sleep 30");
    w.env("TEHAS_ANALYTICS_WAIT", "1");
    let before = Instant::now();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert!(before.elapsed().as_secs() < 20);
    assert_eq!(w.started(), "7 ");
    let cut = r
        .out
        .lines()
        .filter(|l| {
            l.find("WARN analytics ")
                .is_some_and(|i| l[i..].contains(": exit 124"))
        })
        .count();
    assert_eq!(cut, 3, "{}", r.out);
}

// --- The approval and the start on the state store --------------------------

#[test]
fn a_state_store_that_is_not_installed_stops_the_poll_at_78_before_any_call_to_github() {
    let w = World::new("sc-no-store");
    fs::remove_file(w.home.join("libexec/store")).unwrap();
    let r = w.poll();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "ERROR: the state store is not installed (run install.sh)",
    );
    assert_eq!(w.gh(), "");
    assert_eq!(w.started(), "");
}

#[test]
fn a_label_that_cannot_be_changed_takes_the_approval_off_the_record_again_and_the_last_starts_stays_current_a2(
) {
    let w = World::new("sc-a2");
    w.past(7, "bob", "label", "2026-01-01T00:00:00Z", &[]);
    w.fx("edit-fails-on", "--add-label tehas:running\n");
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "ERROR: could not mark #7 as running, not starting a cycle",
    );
    assert_eq!(w.started(), "");
    let v = w.row(&["approval-current", "7"]);
    assert_eq!(
        json!([v["approval"], v["current"]["by"]]),
        json!([null, "bob"])
    );
    assert_eq!(w.attempt(7), 1);
}

#[test]
fn an_attempt_on_record_that_cannot_be_read_before_the_label_changes_no_label_d2() {
    let w = World::new("sc-d2");
    w.store_answers("cycle-show", 1, "store: busy #7: the database is locked", 0);
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "ERROR: #7: the store could not say which attempt is on record (busy), not starting a cycle",
    );
    lacks(&w.gh(), "--add-label tehas:running");
    assert_eq!(w.started(), "");
    assert_eq!(approval_current(&w), json!([null, null]));
}
