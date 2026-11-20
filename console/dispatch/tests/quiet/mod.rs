//! A cycle on #9 that holds its lock and has gone quiet, as the sweep tests
//! make one, and what the store has of its episode.

use crate::common::*;
use std::fs;

pub fn now() -> i64 {
    tehas_core::time::now()
}

#[track_caller]
fn must(w: &World, at: i64, args: &[&str], what: &str) {
    let r = w.store_at(at, args);
    assert_eq!(r.rc, 0, "the store did not take {what}: {}", r.out);
}

/// The time of #9's heartbeat on record.
pub fn heard(w: &World) -> i64 {
    fs::read_to_string(w.t().join("heard-9"))
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

/// #9 was last heard from `age` seconds ago.
pub fn heard_at(w: &World, age: i64) {
    let at = now() - age;
    w.fs("heard-9", &format!("{at}\n"));
    must(
        w,
        at,
        &[
            "cycle-heartbeat",
            "9",
            "--running",
            "--status",
            "running",
            "--step",
            "develop",
        ],
        "the heartbeat of #9",
    );
}

/// A cycle on #9 heard from `age` seconds ago that took its lock `taken`
/// seconds ago, the lock held by the test.
pub fn quiet_cycle(w: &World, age: i64, taken: i64) {
    w.hold(9);
    must(
        w,
        now() - taken,
        &["cycle-start", "9", "--by-hand"],
        "the start of #9",
    );
    heard_at(w, age);
}

/// "9 " when #9 has a quiet episode on record, else "".
pub fn notes(w: &World) -> String {
    if w.row(&["cycle-show", "9"])["cycle"]["quiet"].is_null() {
        String::new()
    } else {
        "9 ".into()
    }
}

/// How many times the poll asked #9's unit to stop in the episode on record.
pub fn note_asks(w: &World) -> String {
    jq(
        ".cycle | if .quiet != null and (.stop_asked_at // -1) >= .quiet.at then .stop_said_times else 0 end",
        &w.row(&["cycle-show", "9"]).to_string(),
    )
}

/// The episode of #9 was first seen a day ago.
pub fn seen_long_ago(w: &World) {
    let said = heard(w).to_string();
    must(
        w,
        now() - 86400,
        &["cycle-note", "9", "--kind", "quiet", "--said-for", &said],
        "the episode of #9",
    );
}

/// The last ask or line about #9 was `secs` ago; the count stays.
pub fn ago(w: &World, secs: i64) {
    must(
        w,
        now() - secs,
        &["cycle-stop-asked", "9", "--said-only"],
        "the last ask of #9",
    );
}

/// The stop command of a cycle is the stub fake-stop, running `body`.
pub fn stop_stub(w: &World, body: &str) {
    w.stub("fake-stop", body);
    w.env("TEHAS_CYCLE_STOP_CMD", "fake-stop");
}

/// The issues fake-stop was asked to stop, each followed by a space.
pub fn stops(w: &World) -> String {
    w.calls("fake-stop")
        .lines()
        .map(|l| format!("{l} "))
        .collect()
}
