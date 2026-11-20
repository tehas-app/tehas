//! Starting a cycle, the poll's own lock, issues marked as running with no
//! cycle and the wait for a start. The pure rules of the same suite are the
//! fixtures of tests/fixtures/start.json.

mod common;

use common::*;
use serde_json::json;
use std::fs::{self, File, OpenOptions};
use std::time::{SystemTime, UNIX_EPOCH};

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn exists(w: &World, rel: &str) -> bool {
    w.t().join(rel).exists()
}

#[test]
fn the_cycle_is_started_for_the_issue_and_told_the_labeller_in_a_comment() {
    let w = World::new("st-start");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
    has(&w.gh(), "Tehas cycle started on");
    has(&w.gh(), "authorized by @alice");
}

#[test]
fn with_no_owner_the_claim_assigns_nobody() {
    let w = World::new("st-no-owner");
    w.poll();
    lacks(&w.gh(), "--add-assignee");
}

#[test]
fn with_an_owner_the_claim_assigns_the_issue_to_the_owner_in_the_same_call() {
    let w = World::new("st-owner");
    w.slots(1, &["TEHAS_ASSIGNEE=carol"]);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &w.gh(),
        "issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:ready --add-assignee carol",
    );
    assert_eq!(w.started(), "7 ");
}

#[test]
fn an_assign_github_refuses_still_marks_the_issue_running_and_starts_it_and_the_log_says_so() {
    let w = World::new("st-assign-fails");
    w.slots(1, &["TEHAS_ASSIGNEE=carol"]);
    w.fx("edit-fails-on", "--add-assignee carol\n");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "WARN: could not assign #7 to carol, marking it running without an assignee",
    );
    has(
        &w.gh(),
        "issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:ready\n",
    );
    assert_eq!(w.started(), "7 ");
}

#[test]
fn the_polls_lock_is_not_passed_on_to_the_cycle_it_starts() {
    let w = World::new("st-fd8");
    w.poll();
    assert!(!exists(&w, "calls/start.fd8"));
}

// Three: the poll's own read, the one after the starts, and the poll's end,
// which runs only after a poll that changed the queue: a start is one.
#[test]
fn the_status_is_refreshed_before_the_poll_and_after_the_start() {
    let w = World::new("st-status");
    w.poll();
    assert_eq!(w.calls("fake-status").lines().count(), 3);
}

#[test]
fn a_poll_that_starts_nothing_runs_no_status_the_queue_did_not_change() {
    let w = World::new("st-no-status");
    w.fx("ready.json", "[]");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    // The poll's own read runs; none of the end's.
    assert_eq!(w.calls("fake-status").trim_end(), "");
}

#[test]
fn for_one_issue_the_running_label_then_the_start_comment_then_the_start() {
    let w = World::new("st-order");
    // What the gh calls held when the start was asked for.
    w.stub(
        "fake-order",
        r#"grep -q -e "--add-label tehas:running" "$T/calls/gh" && : >"$T/calls/order.label"
grep -q "^issue comment $1 " "$T/calls/gh" && : >"$T/calls/order.comment"
flock -w 5 "$TEHAS_HOME/state/cycles/$1.lock" "$T/store-bin" cycle-start "$1" >/dev/null"#,
    );
    w.env("TEHAS_CYCLE_START_CMD", "fake-order");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert!(exists(&w, "calls/order.label"));
    assert!(exists(&w, "calls/order.comment"));
    let gh = w.gh();
    let at = |p: &dyn Fn(&str) -> bool| gh.lines().position(p).unwrap();
    let label = at(&|l| l.contains("--add-label tehas:running"));
    let comment = at(&|l| l.starts_with("issue comment 7 "));
    assert!(label < comment, "{gh}");
}

#[test]
fn the_poll_does_not_wait_for_the_cycle_it_returns_while_the_cycle_lives_with_its_own_lock_free() {
    let w = World::new("st-live");
    // The cycle outlives the poll and holds its own lock, not the poll's.
    w.stub(
        "fake-live",
        r#"( exec 9>"$TEHAS_HOME/state/cycles/$1.lock"; flock -n 9; "$T/store-bin" cycle-start "$1"; exec sleep 8 ) >/dev/null 2>&1 &
echo $! >"$T/live.pid""#,
    );
    w.env("TEHAS_CYCLE_START_CMD", "fake-live");
    let r = w.poll();
    let pid = fs::read_to_string(w.t().join("live.pid")).unwrap();
    let pid = pid.trim();
    let alive = std::path::Path::new(&format!("/proc/{pid}")).exists();
    let lock = File::open(w.home.join("state/dispatch.lock")).unwrap();
    let free = lock.try_lock().is_ok();
    drop(lock);
    let _ = std::process::Command::new("kill").arg(pid).status();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert!(alive);
    assert!(free);
}

#[test]
fn the_polls_lock_is_not_handed_to_the_index() {
    let w = World::new("st-index-fd8");
    w.poll();
    assert_eq!(w.calls("fake-index").trim_end(), "--before-cycle");
    assert!(!exists(&w, "calls/index.fd8"));
}

#[test]
fn a_real_poll_records_when_it_took_the_lock() {
    let w = World::new("st-taken");
    w.poll();
    let taken: u64 = w.read_home("state/dispatch.taken").trim().parse().unwrap();
    assert!(now() - taken < 60);
}

/// The dispatcher's lock, held by the test, as taken `ago` seconds since.
fn lock_held(w: &World, ago: u64) -> File {
    let _ = fs::remove_file(w.home.join("state/dispatch.stuck"));
    w.home_file("state/dispatch.taken", &format!("{}\n", now() - ago));
    let f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(w.home.join("state/dispatch.lock"))
        .unwrap();
    f.try_lock().unwrap();
    f
}

#[test]
fn a_poll_that_finds_the_lock_held_for_longer_than_a_turn_may_take_says_so_once_and_exits_0() {
    let w = World::new("st-stuck");
    let _lock = lock_held(&w, 6000);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "WARN: the dispatcher's lock has been held since");
    has(
        &r.out,
        "more than 1800s: a poll hangs, and no poll runs until it ends",
    );
    assert_eq!(w.gh(), "");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(r.out, "");
}

#[test]
fn a_poll_that_finds_the_lock_held_for_a_short_while_says_nothing() {
    let w = World::new("st-held-short");
    let _lock = lock_held(&w, 60);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(r.out, "");
    let r = w.dispatch(&["--dry-run"]);
    has(&r.out, "another poll holds the dispatcher's lock");
    lacks(&r.out, "WARN");
}

#[test]
fn a_poll_that_finds_the_lock_held_does_not_change_when_it_was_taken() {
    let w = World::new("st-held-taken");
    let _lock = lock_held(&w, 60);
    let stamp = w.read_home("state/dispatch.taken");
    w.poll();
    w.dispatch(&["--dry-run"]);
    assert_eq!(w.read_home("state/dispatch.taken"), stamp);
}

#[test]
fn a_poll_that_finds_the_lock_held_leaves_the_ended_cycles_request_so_the_path_unit_starts_a_poll_when_the_running_one_ends(
) {
    let w = World::new("st-held-again");
    let _lock = lock_held(&w, 60);
    w.home_file("state/dispatch.again", "");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert!(w.home.join("state/dispatch.again").exists());
}

#[test]
fn a_poll_takes_the_request_an_ended_cycle_left_and_a_dry_run_leaves_it() {
    let w = World::new("st-again");
    let again = w.home.join("state/dispatch.again");
    w.home_file("state/dispatch.again", "");
    w.dispatch(&["--dry-run"]);
    assert!(again.exists());
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert!(!again.exists());
}

/// #7 as an epic: `total` sub-issues, `completed` of them closed.
fn epic(w: &World, total: u64, completed: u64) {
    w.fx_json(
        "work-7.json",
        &json!({"number": 7, "state": "open", "title": "Faster search", "body": "Make it fast.",
            "labels": [{"name": "tehas:ready"}],
            "sub_issues_summary": {"total": total, "completed": completed}}),
    );
}

#[test]
fn when_its_label_cannot_be_changed_nothing_is_said_on_it_and_the_poll_goes_on_the_epic() {
    let w = World::new("st-epic-edit-fails");
    epic(&w, 3, 1);
    w.fx("edit-fails-on", "issue edit 7\n");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "WARN: label change on #7 failed: --remove-label tehas:ready",
    );
    lacks(&w.gh(), "issue comment 7");
    assert_eq!(w.started(), "");
}

#[test]
fn a_lock_file_nobody_holds_does_not_make_it_a_running_cycle() {
    let w = World::new("st-lock-file");
    w.fx("marked.json", r#"[{"number":9}]"#);
    w.home_file("state/cycles/9.lock", "");
    w.poll();
    has(
        &w.gh(),
        "issue edit 9 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention",
    );
}

#[test]
fn when_its_label_cannot_be_changed_nothing_is_said_on_it_and_the_poll_goes_on_the_marked_issue() {
    let w = World::new("st-marked-edit-fails");
    w.fx("marked.json", r#"[{"number":9}]"#);
    w.fx("edit-fails-on", "issue edit 9\n");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "WARN: label change on #9 failed");
    lacks(&w.gh(), "issue comment 9");
    assert_eq!(w.started(), "7 ");
}

#[test]
fn an_approval_starts_one_cycle_a_second_poll_on_the_same_label_event_starts_nothing_a3() {
    let w = World::new("st-a3");
    w.queue(&["7"]);
    w.poll();
    assert_eq!(w.started(), "7 ");
    w.clear_calls();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "skip #7: its approval of 2026-10-01T00:00:00Z started a cycle already",
    );
    assert_eq!(w.started(), "");
    lacks(&w.gh(), "--add-label tehas:running");
    assert_eq!(w.attempt(7), 1);
}

/// A store at the installed path whose cycle-show of #7 answers busy while
/// $T/busy-left counts down, or takes 2 seconds while $T/slow is there, and
/// runs the real store for everything else.
fn slow_store(w: &World) {
    let bin = w.home.join("libexec/store");
    for p in [
        bin.clone(),
        w.t().join("busy-left"),
        w.t().join("slow"),
        w.t().join("looks"),
    ] {
        let _ = fs::remove_file(p);
    }
    let sleep = run({
        let mut c = std::process::Command::new("bash");
        c.args(["-c", "command -pv sleep"]);
        c
    })
    .out;
    let t = w.t().display().to_string();
    let text = format!(
        r#"#!/usr/bin/env bash
if [ "$1 ${{2:-}}" = "cycle-show 7" ]; then
  n=$(cat '{t}/busy-left' 2>/dev/null || echo 0)
  if [ "$n" -gt 0 ]; then echo $((n - 1)) >'{t}/busy-left'; echo 'store: busy #7: the database is locked' >&2; exit 1; fi
  [ ! -e '{t}/slow' ] || {{ echo look >>'{t}/looks'; '{sleep}' 2; }}
fi
exec '{t}/store-bin' "$@"
"#
    );
    tehas_core::testing::write_exe(&bin, text).unwrap();
}

#[test]
fn a_store_that_is_busy_during_the_wait_still_finds_the_start() {
    let w = World::new("st-busy-wait");
    slow_store(&w);
    // Three looks must fit in the wait, which a loaded runner cannot promise
    // in 2 seconds; the start is found at the third look.
    w.env("TEHAS_CYCLE_START_WAIT", "10");
    // The store is busy for the first two looks after the unit is started.
    w.stub(
        "fake-start",
        r#"echo 2 >"$T/busy-left"
flock -w 5 "$TEHAS_HOME/state/cycles/$1.lock" "$T/store-bin" cycle-start "$1" >/dev/null"#,
    );
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    lacks(&r.out, "did not start");
    lacks(&r.out, "taking it as started");
    assert_eq!(
        fs::read_to_string(w.t().join("busy-left")).unwrap().trim(),
        "0"
    );
}

#[test]
fn the_wait_is_bounded_by_the_clock_however_long_each_look_takes() {
    let w = World::new("st-wait-clock");
    slow_store(&w);
    w.touch("start-silent");
    w.stub("fake-start", r#": >"$T/slow""#);
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "ERROR: the cycle for #7 did not start");
    // TEHAS_CYCLE_START_WAIT is 2 and a look takes 2 seconds: the clock ends
    // the wait after one look, two at most. A count of looks would make four.
    let looks = fs::read_to_string(w.t().join("looks")).unwrap_or_default();
    assert!(looks.lines().count() <= 2, "{looks}");
}
