//! The clock sample of #1043: the pair a call takes, the step between two
//! pairs, and the hold that follows a step.

use crate::clock::{may_act, mono, pair, step, Now, Sample, HOLD_AFTER_STEP, TOLERANCE};
use crate::verbs_tests::*;
use std::path::Path;

fn sample(wall: i64, mono: i64) -> Option<Sample> {
    Some(pair(wall, mono))
}

#[test]
fn two_clocks_that_agree_show_no_step() {
    // The same advance on both: the clocks agree, whatever the wall clock's
    // own value.
    assert_eq!(step(sample(1000, 0), sample(1600, 600)), None);
    // Up to TOLERANCE seconds apart is still agreement: the two readings of
    // one call are not taken at the same instant.
    assert_eq!(step(sample(1000, 0), sample(1600, 600 - TOLERANCE)), None);
    assert_eq!(step(sample(1000, 0), sample(1600, 600 + TOLERANCE)), None);
}

#[test]
fn a_step_forward_is_named_by_the_seconds_it_moved() {
    // An hour restored: the wall clock moved 3600 s, the monotonic clock 10 s.
    assert_eq!(step(sample(1000, 0), sample(4610, 10)), Some(3610));
    // The step is the wall clock's own advance, so the line can name it.
    assert_eq!(step(sample(1000, 100), sample(4600, 110)), Some(3600));
}

#[test]
fn a_step_backwards_is_a_step_too() {
    // A clock put right after the save ran fast: the rules must not read the
    // negative ages that follow as heartbeats from the future.
    assert_eq!(step(sample(4600, 100), sample(1000, 110)), Some(-3600));
}

#[test]
fn a_call_with_no_pair_to_compare_acts() {
    assert_eq!(step(None, sample(1000, 0)), None);
    assert_eq!(step(sample(1000, 0), None), None);
    assert!(may_act(0, None));
}

#[test]
fn a_rule_is_held_off_for_one_heartbeat_interval_after_a_step() {
    // The step was measured from the pair taken at monotonic 100.
    let seen = Some(100);
    assert!(!may_act(100, seen), "the call that found the step");
    assert!(!may_act(100 + HOLD_AFTER_STEP - 1, seen));
    assert!(may_act(100 + HOLD_AFTER_STEP, seen), "one interval on");
    assert!(may_act(100 + 3 * HOLD_AFTER_STEP, seen));
    // No step seen: the rule acts.
    assert!(may_act(0, None));
}

#[test]
fn the_pair_of_a_call_is_its_now_and_the_stores_own_clock() {
    // The wall reading of a pair is the `now` the call was given; the monotonic
    // reading is the seconds since the store's process began, and it never runs
    // back: the two readings a step is measured between are of one clock.
    let first = pair(1234, mono());
    assert_eq!(first.wall, 1234);
    let later = pair(9999, mono());
    assert_eq!(later.wall, 9999);
    assert!(later.mono >= first.mono, "the monotonic clock went back");
}

#[test]
fn the_published_entry_names_the_newest_pair_and_the_step() {
    let got = sample(4600, 110);
    let clocks = Now::of(sample(1000, 100), got);
    assert_eq!(clocks.stepped, Some(3600));
    assert_eq!(clocks.seen, Some(100), "measured from the pair before it");
    assert_eq!(
        clocks.json().unwrap(),
        r#"{"at":4600,"wall":4600,"mono":110,"step":3600,"seen":100}"#
    );
    // A step held the age rules of this call.
    assert!(clocks.holds());
    // The newest pair is this call's: publish writes it.
    assert_eq!(clocks.keep(), got);
}

#[test]
fn a_pair_with_no_step_leaves_the_rules_alone() {
    let clocks = Now::of(sample(1000, 100), sample(1030, 130));
    assert_eq!(clocks.stepped, None);
    assert_eq!(clocks.seen, None);
    assert!(!clocks.holds());
    assert_eq!(
        clocks.json().unwrap(),
        r#"{"at":1030,"wall":1030,"mono":130,"step":null,"seen":null}"#
    );
}

#[test]
fn a_call_that_read_no_pair_publishes_the_pair_the_store_holds() {
    let clocks = Now::of(sample(1000, 100), None);
    assert_eq!(clocks.stepped, None);
    assert!(!clocks.holds(), "nothing was measured, so nothing is held");
    assert_eq!(clocks.keep(), sample(1000, 100));
    assert_eq!(
        clocks.json().unwrap(),
        r#"{"at":1000,"wall":1000,"mono":100,"step":null,"seen":null}"#
    );
}

#[test]
fn a_store_with_no_pair_has_nothing_to_publish() {
    assert_eq!(Now::default().json(), None);
    assert_eq!(Now::default().keep(), None);
    assert!(!Now::default().holds());
}

/// The row of the sample, read back the way the verbs read it: `kept`, which is
/// the pair on record whatever its own date.
fn kept(home: &Path, now: i64) -> Option<Sample> {
    crate::clock::kept(&conn(home), now)
}

#[test]
fn the_sample_of_a_beat_is_the_newest_pair_of_the_store() {
    let home = home("clock-record");
    let (_lock, _) = start_by(&home, 4, "alice", "label", NOW);
    ok_at(
        &home,
        NOW,
        &["cycle-heartbeat", "4", "--running", "--status", "running"],
    );
    let got = kept(&home, NOW).expect("the beat kept no pair");
    assert_eq!(got.wall, NOW, "the pair's wall reading is the beat's now");
    // The next beat replaces it: the store holds the newest pair of the host.
    ok_at(
        &home,
        NOW + 10,
        &["cycle-heartbeat", "4", "--running", "--status", "running"],
    );
    let again = kept(&home, NOW + 10).expect("the second beat left no pair");
    assert_eq!(again.wall, NOW + 10, "one row, the newest pair");
}

#[test]
fn a_pair_is_read_whatever_its_own_date() {
    // The age a rule judges is counted on the wall clock, and the pair's job is
    // to say whether that clock moved; a pair of last hour says that as well as
    // one of this minute, so the row is read whatever its date. The row is the
    // store's, written by the beat.
    let home = home("clock-old");
    let (_lock, _) = start_by(&home, 5, "alice", "label", NOW);
    ok_at(
        &home,
        NOW,
        &["cycle-heartbeat", "5", "--running", "--status", "running"],
    );
    assert!(crate::clock::kept(&conn(&home), NOW).is_some());
    assert!(crate::clock::kept(&conn(&home), NOW + 3 * 3600).is_some());
}
