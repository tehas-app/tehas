//! A cycle that holds its lock and has gone quiet, and what the poll asks of
//! its unit.

mod common;
mod quiet;

use common::*;
use quiet::*;

#[test]
fn an_ask_is_recorded_before_the_stop_so_a_slow_stop_does_not_put_off_the_next_ask() {
    let w = World::new("quiet-slow-stop");
    stop_stub(&w, r#"sleep 3; date +%s >"$T/calls/stop.done""#);
    quiet_cycle(&w, 1300, 3600);
    seen_long_ago(&w);
    has(&w.poll().out, "stopping tehas-cycle@9.service");
    let asked = w.row(&["cycle-show", "9"])["cycle"]["stop_asked_at"]
        .as_i64()
        .unwrap();
    let done: i64 = w.calls("stop.done").trim().parse().unwrap();
    assert!(done - asked >= 3, "asked at {asked}, stopped at {done}");
    // The record is dated a moment after the poll's clock: the poll that comes
    // quiet_retry after the ask, by its own clock, asks again.
    ago(&w, 599);
    has(
        &w.poll().out,
        "the poll asks tehas-cycle@9.service to stop again",
    );
    assert_eq!(note_asks(&w), "2");
}

#[test]
fn a_quiet_cycle_that_is_heard_from_again_is_left_alone() {
    let w = World::new("quiet-heard-again");
    stop_stub(&w, "exit 1");
    quiet_cycle(&w, 1300, 3600);
    seen_long_ago(&w);
    w.poll();
    ago(&w, 601);
    w.poll();
    assert_eq!(note_asks(&w), "2");
    // The stop was asked, and the cycle went on with its heartbeats.
    heard_at(&w, 20);
    ago(&w, 3601);
    lacks(&w.poll().out, "holds its lock but");
    assert_eq!(stops(&w), "9 9 ");
}

// A heartbeat only moves forward, so the new episode is a cycle of its own.
#[test]
fn a_quiet_cycle_heard_from_since_its_episode_is_a_new_episode_whose_asks_count_from_one() {
    let w = World::new("quiet-new-episode");
    stop_stub(&w, "exit 1");
    quiet_cycle(&w, 1300, 3600);
    seen_long_ago(&w);
    w.poll();
    ago(&w, 601);
    w.poll();
    assert_eq!(note_asks(&w), "2");
    // The store dates in whole seconds, and an ask in the second the new
    // episode begins counts as the episode's own: the last ask goes back a
    // few seconds first.
    ago(&w, 5);
    heard_at(&w, 1250);
    let r = w.poll();
    has(&r.out, "more than 1200s: stopping tehas-cycle@9.service");
    lacks(&r.out, "to stop again");
    assert_eq!(stops(&w), "9 9 9 ");
    assert_eq!(note_asks(&w), "1");
}

#[test]
fn a_quiet_cycle_whose_episode_the_store_cannot_take_is_not_stopped_and_is_asked_once_it_can() {
    let w = World::new("quiet-episode-busy");
    stop_stub(&w, "");
    quiet_cycle(&w, 1300, 3600);
    w.store_answers("cycle-note", 1, "store: busy #9: the database is locked", 1);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "WARN: the cycle for #9 holds its lock but has not been heard from since ",
    );
    has(
        &r.out,
        "the state store could not record it (busy), so the poll asks nothing this time",
    );
    assert_eq!(stops(&w), "");
    assert_eq!(notes(&w), "");
    has(&w.poll().out, "stopping tehas-cycle@9.service");
    assert_eq!(stops(&w), "9 ");
    assert_eq!(note_asks(&w), "1");
}

#[test]
fn a_store_that_cannot_be_used_ends_the_poll_and_stops_no_quiet_cycle() {
    let w = World::new("quiet-store-corrupt");
    stop_stub(&w, "");
    quiet_cycle(&w, 1300, 3600);
    w.store_refuses(
        "cycle-note",
        78,
        "store: corrupt: the database disk image is malformed",
    );
    let r = w.poll();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "ERROR: the state store cannot be used: store: corrupt: the database disk image is malformed",
    );
    lacks(&r.out, "so the poll asks nothing this time");
    assert_eq!(stops(&w), "");
    w.store_refuses("cycle-note", 64, "store: usage cycle-note: --said-for");
    let r = w.poll();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "ERROR: the state store cannot be used: store: usage cycle-note",
    );
    assert_eq!(stops(&w), "");
    // The episode is taken, and the ask cannot be recorded: nothing is stopped.
    w.store_refuses(
        "cycle-stop-asked",
        78,
        "store: corrupt: the database disk image is malformed",
    );
    let r = w.poll();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "ERROR: the state store cannot be used: store: corrupt",
    );
    assert_eq!(stops(&w), "");
}

/// Six asks of a stop that fails, 10 minutes apart.
fn six_asks(w: &World) {
    stop_stub(w, "exit 1");
    quiet_cycle(w, 1300, 3600);
    seen_long_ago(w);
    for _ in 0..6 {
        ago(w, 601);
        w.poll();
    }
    assert_eq!(note_asks(w), "6");
    ago(w, 601);
}

#[test]
fn a_store_that_cannot_be_used_when_the_poll_says_a_cycle_needs_a_person_ends_the_poll() {
    let w = World::new("quiet-person-corrupt");
    six_asks(&w);
    w.store_refuses(
        "cycle-stop-asked",
        78,
        "store: corrupt: the database disk image is malformed",
    );
    let r = w.poll();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "asks no more");
    has(
        &r.out,
        "ERROR: the state store cannot be used: store: corrupt",
    );
}

#[test]
fn a_line_the_store_does_not_record_is_said_again_on_the_next_poll() {
    let w = World::new("quiet-line-busy");
    six_asks(&w);
    w.store_answers(
        "cycle-stop-asked",
        1,
        "store: busy #9: the database is locked",
        1,
    );
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "asks no more");
    has(
        &r.out,
        "WARN: the state store did not record that the poll asks no more of tehas-cycle@9.service (busy)",
    );
    assert_eq!(note_asks(&w), "6");
    let r = w.poll();
    has(
        &r.out,
        "asked 6 times to stop tehas-cycle@9.service and asks no more",
    );
    lacks(&r.out, "did not record");
    assert_eq!(note_asks(&w), "7");
    assert_eq!(stops(&w), "9 9 9 9 9 9 ");
}

#[test]
fn a_quiet_cycle_whose_record_does_not_read_as_one_is_asked_nothing() {
    let w = World::new("quiet-record-unread");
    stop_stub(&w, "");
    quiet_cycle(&w, 1300, 3600);
    w.store_refuses(
        "cycle-note",
        0,
        r#"{"api":1,"verb":"cycle-note","ok":true,"said":true,"episode_at":1,"times":"many","stop_asked_at":null}"#,
    );
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "the state store's record of it does not read as one, so the poll asks nothing this time",
    );
    assert_eq!(stops(&w), "");
}

#[test]
fn a_stop_the_store_does_not_record_is_asked_again_on_the_next_poll() {
    let w = World::new("quiet-stop-busy");
    stop_stub(&w, "");
    quiet_cycle(&w, 1300, 3600);
    w.store_answers(
        "cycle-stop-asked",
        1,
        "store: busy #9: the database is locked",
        1,
    );
    let r = w.poll();
    has(&r.out, "stopping tehas-cycle@9.service");
    has(
        &r.out,
        "WARN: the state store did not record the stop of tehas-cycle@9.service (busy): the poll asks again",
    );
    assert_eq!(stops(&w), "9 ");
    has(&w.poll().out, "stopping tehas-cycle@9.service");
    assert_eq!(stops(&w), "9 9 ");
    assert_eq!(note_asks(&w), "1");
}

#[test]
fn a_stuck_cycle_whose_last_heard_time_cannot_be_read_says_nothing_and_keeps_its_note() {
    let w = World::new("quiet-heard-unread");
    stop_stub(&w, "");
    quiet_cycle(&w, 1300, 3600);
    w.poll();
    assert_eq!(notes(&w), "9 ");
    assert_eq!(stops(&w), "9 ");
    // The store answers with no cycle on record: nothing is known about this
    // cycle.
    w.store_refuses(
        "cycle-show",
        0,
        r#"{"api":1,"verb":"cycle-show","ok":true,"probe_rc":0,"cycles":[{"issue":9,"lock":{"state":"held","probe_rc":1,"file":true,"seen_at":1},"cycle":null}]}"#,
    );
    lacks(&w.poll().out, "holds its lock but");
    assert_eq!(notes(&w), "9 ");
    assert_eq!(note_asks(&w), "1");
    assert_eq!(stops(&w), "9 ");
}

/// #9's record is ended, refused, while its lock is held.
fn end_refused(w: &World) {
    let a = w.attempt(9).to_string();
    let r = w.store(&["cycle-end", "9", "--attempt", &a, "--refused"]);
    assert_eq!(r.rc, 0, "{}", r.out);
}

#[test]
fn a_quiet_cycle_whose_record_has_ended_is_not_stopped_by_the_poll_which_says_so_for_a_person() {
    let w = World::new("quiet-ended");
    stop_stub(&w, "");
    quiet_cycle(&w, 1300, 3600);
    end_refused(&w);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    lacks(&r.out, "stopping tehas-cycle@9.service");
    has(
        &r.out,
        "WARN: the cycle for #9 holds its lock but has not been heard from since ",
    );
    has(&r.out, "and the state store has no open record of it, so the poll does not stop it: if it stays, a person can, with tehas stop 9 or systemctl --user stop tehas-cycle@9.service");
    assert_eq!(stops(&w), "");
    let r = w.dispatch(&["--dry-run"]);
    lacks(&r.out, "would stop");
    has(
        &r.out,
        "and the state store has no open record of it, so the poll does not stop it",
    );
    assert_eq!(stops(&w), "");
}

#[test]
fn a_quiet_lock_whose_record_has_ended_is_not_stopped_by_the_poll_which_says_so_for_a_person() {
    let w = World::new("quiet-lock-ended");
    stop_stub(&w, "");
    w.hold(9);
    let r = w.store_at(now() - 3600, &["cycle-start", "9", "--by-hand"]);
    assert_eq!(
        r.rc, 0,
        "the store did not start the record of #9: {}",
        r.out
    );
    heard_at(&w, 1300);
    end_refused(&w);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "and the state store has no open record of it, so the poll does not stop it",
    );
    assert_eq!(stops(&w), "");
    let r = w.dispatch(&["--dry-run"]);
    lacks(&r.out, "would stop");
    has(&r.out, "and the state store has no open record of it");
}

#[test]
fn a_cycle_whose_record_ends_after_the_poll_listed_it_is_not_stopped_and_that_is_said() {
    let w = World::new("quiet-ends-late");
    stop_stub(&w, "");
    quiet_cycle(&w, 1300, 3600);
    w.store_refuses("cycle-note", 1, "store: nocycle #9: no cycle on record");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "and the state store has no open record of it, so the poll does not stop it",
    );
    lacks(&r.out, "could not record it");
    assert_eq!(stops(&w), "");
}

#[test]
fn a_cycle_heard_from_lately_or_one_that_took_its_lock_lately_is_left_alone() {
    let w = World::new("quiet-lately");
    stop_stub(&w, "");
    quiet_cycle(&w, 20, 3600);
    lacks(&w.poll().out, "holds its lock but");
    w.release();
    // A live file left by the issue's last cycle, and a new one still starting.
    quiet_cycle(&w, 86400, 60);
    lacks(&w.poll().out, "holds its lock but");
    assert_eq!(stops(&w), "");
}

#[test]
fn a_dry_run_says_what_the_poll_would_do_about_a_quiet_cycle_and_stops_nothing() {
    let w = World::new("quiet-dry-run");
    stop_stub(&w, "exit 1");
    quiet_cycle(&w, 1300, 3600);
    let dry = || w.dispatch(&["--dry-run"]).out;
    has(&dry(), "would stop tehas-cycle@9.service: the cycle for #9 holds its lock but has not been heard from since ");
    has(&dry(), "would stop tehas-cycle@9.service");
    assert_eq!(stops(&w), "");
    assert_eq!(notes(&w), "");
    seen_long_ago(&w);
    w.poll();
    assert_eq!(stops(&w), "9 ");
    // Inside the wait the poll says nothing, and so does the dry run.
    lacks(&dry(), "holds its lock but");
    ago(&w, 601);
    let out = dry();
    has(
        &out,
        "would ask tehas-cycle@9.service to stop again: the cycle for #9 holds its lock",
    );
    lacks(&out, "would stop");
    for _ in 2..=6 {
        ago(&w, 601);
        w.poll();
    }
    assert_eq!(note_asks(&w), "6");
    ago(&w, 601);
    let out = dry();
    has(
        &out,
        "would say that tehas-cycle@9.service needs a person and ask no more",
    );
    lacks(&out, "would ask");
    lacks(&out, "would stop");
    assert_eq!(note_asks(&w), "6");
    assert_eq!(stops(&w), "9 9 9 9 9 9 ");
    // A cycle heard from since the episode on record: a new one, which the
    // poll would stop.
    heard_at(&w, 1250);
    has(&dry(), "would stop tehas-cycle@9.service");
}

#[test]
fn the_issue_s_next_cycle_starts_with_no_quiet_episode_on_record() {
    let w = World::new("quiet-next-cycle");
    stop_stub(&w, "");
    quiet_cycle(&w, 1300, 3600);
    w.poll();
    assert_eq!(notes(&w), "9 ");
    // The unit stopped, and the issue's next cycle took the lock.
    w.release();
    quiet_cycle(&w, 20, 20);
    assert_eq!(notes(&w), "");
    w.poll();
    assert_eq!(notes(&w), "");
    assert_eq!(stops(&w), "9 ");
}
