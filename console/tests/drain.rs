//! `tehas drain` end to end: the limit of no new work it sets, and its wait
//! for the running cycles to end and for a poll that holds the dispatcher's
//! lock. A cycle here is its lock, held by this process; a poll is the
//! dispatcher's lock, held by a thread. gh is a stub that answers nothing: a
//! drain asks GitHub for nothing.

mod common;

use common::*;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};
use tehas_core::home::State;

fn host(name: &str) -> World {
    let w = World::new(&format!("drain-{name}"));
    w.gh_stub("");
    fs::create_dir_all(w.at().cycles_dir()).unwrap();
    w
}

fn file(w: &World) -> PathBuf {
    w.at().state(State::Limit)
}

fn drain(w: &World, words: &[&str]) -> Run {
    w.tehas("drain", words)
}

/// The file of the limit, as JSON text.
fn kept(w: &World) -> String {
    fs::read_to_string(file(w)).unwrap()
}

/// What an answer says of `drained` and `running`.
fn said(text: &str) -> [String; 2] {
    ["/drained", "/running"].map(|k| json_at(text, k))
}

fn locked(path: &Path) -> File {
    let f = OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .unwrap();
    f.lock().unwrap();
    f
}

/// A running cycle of `n`: its lock, held until the value drops.
fn running(w: &World, n: u32) -> File {
    locked(&w.at().cycle_note(n, "lock"))
}

/// A running cycle of `n` that ends `after` from now.
fn ends_in(w: &World, n: u32, after: Duration) -> thread::JoinHandle<()> {
    let f = running(w, n);
    thread::spawn(move || {
        thread::sleep(after);
        drop(f);
    })
}

/// A poll of the dispatcher: its lock, taken now and held for `hold`. With
/// an issue, it starts that issue's cycle `after` in: the cycle's lock is
/// taken before the poll's is let go, and outlives the poll by a minute, as
/// a cycle outlives the poll that started it. The limit holds only a poll
/// that has yet to read it, so only the take of the poll's lock counts a
/// cycle such a poll starts (#795).
fn poll(w: &World, after: Duration, hold: Duration, n: Option<u32>) -> thread::JoinHandle<()> {
    let lock = locked(&w.at().state(State::DispatchLock));
    let cycle = n.map(|n| w.at().cycle_note(n, "lock"));
    thread::spawn(move || {
        thread::sleep(after);
        let Some(cycle) = cycle else {
            thread::sleep(hold.saturating_sub(after));
            return drop(lock);
        };
        let started = locked(&cycle);
        drop(lock);
        thread::sleep(Duration::from_secs(60).min(hold));
        drop(started);
    })
}

#[test]
fn with_nothing_running_the_limit_is_set_and_drain_is_done_at_once() {
    let w = host("idle");
    let r = drain(&w, &[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "limit     0 cycles at once, planning off, until cleared: draining",
    );
    has(&r.out, "drained: no cycle is running");
    assert_eq!(
        json_without(&kept(&w), "/set"),
        r#"{"cycles":0,"plan":false,"reason":"draining","until":null,"v":1}"#
    );
    assert_eq!(w.gh(), "");

    w.reset();
    let r = drain(&w, &["--json"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(json_at(&r.out, "/limit/set"), json_at(&kept(&w), "/set"));
    assert_eq!(
        json_without(&r.out, "/limit/set"),
        r#"{"api":1,"drained":true,"limit":{"cycles":0,"plan":false,"reason":"draining","state":"ok"},"running":[]}"#
    );
}

#[test]
fn a_cycle_still_running_is_exit_3_and_named() {
    let w = host("running");
    let _cycle = running(&w, 7);
    let r = drain(&w, &["--json"]);
    assert_eq!(r.rc, 3, "{}", r.out);
    assert_eq!(said(&r.out), ["false", "[7]"]);
    let r = drain(&w, &[]);
    assert_eq!(r.rc, 3, "{}", r.out);
    has(&r.out, "still running: #7");
    let start = Instant::now();
    assert_eq!(drain(&w, &["--wait", "1"]).rc, 3);
    assert!(start.elapsed() < Duration::from_secs(30));
}

#[test]
fn a_cycle_that_ends_during_the_wait_is_exit_0() {
    let w = host("ends");
    let cycle = ends_in(&w, 7, Duration::from_secs(1));
    let r = drain(&w, &["--wait", "20", "--json"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(said(&r.out), ["true", "[]"]);
    cycle.join().unwrap();
}

#[test]
fn drain_replaces_a_limit_with_an_expiry_by_one_until_cleared() {
    let w = host("replace");
    fs::write(
        file(&w),
        r#"{"v":1,"cycles":3,"plan":true,"until":"2036-01-01T00:00:00Z","reason":"industry"}"#,
    )
    .unwrap();
    assert_eq!(drain(&w, &[]).rc, 0);
    let limit = kept(&w);
    let got = ["/cycles", "/plan", "/until", "/reason"].map(|k| json_at(&limit, k));
    assert_eq!(got, ["0", "false", "null", r#""draining""#]);
}

#[test]
fn bad_arguments_are_64_and_write_nothing() {
    let w = host("bad");
    let mut last = String::new();
    for words in [
        &["--wait"][..],
        &["--wait", "x"],
        &["--wait", "-1"],
        &["--wait", "1", "--wait", "2"],
        &["--json", "--json"],
        &["now"],
        &["--bogus"],
    ] {
        let r = drain(&w, words);
        assert_eq!(r.rc, 64, "{words:?}: {}", r.out);
        assert!(!file(&w).exists(), "{words:?} wrote the limit");
        last = r.out;
    }
    has(&last, "usage: tehas drain");
}

#[test]
fn a_limit_that_cannot_be_written_is_exit_1_and_prints_no_object() {
    let w = host("unwritable");
    let state = w.at().state_dir();
    fs::remove_dir_all(&state).unwrap();
    fs::write(&state, "").unwrap();
    let r = drain(&w, &["--wait", "0", "--json"]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "tehas limit: could not create");
    assert!(!r.out.lines().any(|l| l.starts_with('{')), "{}", r.out);
}

#[test]
fn without_an_instance_nothing_is_written_exit_78() {
    let w = host("noinstance");
    w.no_instance();
    assert_eq!(drain(&w, &[]).rc, 78);
    assert!(!file(&w).exists());
}

#[test]
fn a_cycle_a_running_poll_starts_after_the_limit_was_written_is_counted() {
    let w = host("race");
    // The poll outlives the drain, so the drain cannot have counted a cycle of
    // a poll it had already let go: the take of the lock is what waits for it.
    let _poll = poll(
        &w,
        Duration::from_millis(400),
        Duration::from_secs(30),
        Some(7),
    );
    let r = drain(&w, &["--wait", "4", "--json"]);
    assert_eq!(r.rc, 3, "{}", r.out);
    assert_eq!(said(&r.out), ["false", "[7]"]);
}

#[test]
fn a_poll_that_starts_nothing_is_waited_for_then_no_cycle_is_said() {
    let w = host("quietpoll");
    let held = poll(&w, Duration::ZERO, Duration::from_secs(1), None);
    let start = Instant::now();
    let r = drain(&w, &["--wait", "4", "--json"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        json_without(&r.out, "/limit/set"),
        r#"{"api":1,"drained":true,"limit":{"cycles":0,"plan":false,"reason":"draining","state":"ok"},"running":[]}"#
    );
    // It waited for the poll, which held the lock a second, not for its --wait.
    assert!(start.elapsed() < Duration::from_secs(4));
    held.join().unwrap();
}

#[test]
fn a_poll_that_holds_its_lock_through_the_wait_is_exit_3_with_one_answer() {
    let w = host("heldpoll");
    let _poll = poll(&w, Duration::ZERO, Duration::from_secs(30), None);
    let r = drain(&w, &["--wait", "1", "--json"]);
    assert_eq!(r.rc, 3, "{}", r.out);
    assert_eq!(said(&r.out), ["false", "[]"]);
    assert_eq!(json_at(&r.out, "/limit/reason"), r#""draining""#);
    let r = drain(&w, &["--wait", "1"]);
    assert_eq!(r.rc, 3, "{}", r.out);
    has(
        &r.out,
        "not drained: a poll held its lock for the whole wait",
    );
}
