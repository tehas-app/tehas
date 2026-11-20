//! tehas:lean through the real read of a poll. The rules are in
//! tests/fixtures/lean.json.

mod common;

use common::{count_lines, has, lacks, park_body, World};
use serde_json::json;

/// lean is still being built: a test build turns it on, after reset().
fn lean_on(w: &World) {
    w.env("TEHAS_TEST_FEATURES", "lean");
    w.slots(1, &["TEHAS_LEAN=on"]);
}

/// Issue 7 parked with its Retry box ticked by alice. Its last start named
/// the driver given, and it carries tehas:lean from alice.
fn lean_parked(w: &World, input: &str, reason: &str) {
    w.reset();
    lean_on(w);
    w.fx("ready.json", "[]");
    w.fx(
        "asking.json",
        r#"[{"number":7,"labels":[{"name":"needs-human-attention"},{"name":"tehas:question"},{"name":"tehas:lean"}]}]"#,
    );
    w.fs("lean-input.json", input);
    let file = w.t().join("lean-input.json");
    w.t_approve(
        7,
        "alice",
        "label",
        1_700_000_000,
        &["--input-file", &file.to_string_lossy()],
    );
    w.t_started(7, None, &[]);
    let r = w.sh(
        r#"flock -w 5 "$TEHAS_HOME/state/cycles/7.lock" "$T/store-bin" cycle-end 7 --attempt 1 --status failed --parked >/dev/null"#,
    );
    assert_eq!(r.rc, 0, "the store refused the end of #7: {}", r.out);
    w.t_park(7, reason, "4001", &[]);
    w.label_events(7, &["tehas:lean=alice"]);
    w.comments(7, &park_body(&["x", " "]), "alice", json!([]));
}

const BLOCK_BODY: &str = r#"{"title":"Faster search","body":"```pi-spec v1\n{\"intent\":\"faster search\"}\n```\n\nMake it fast."}"#;

#[test]
fn a_retry_tick_reads_the_record_of_an_issue_that_is_no_candidate_and_keeps_its_driver() {
    let w = World::new("lean");
    lean_parked(&w, r#"{"links":[],"driver":"lean"}"#, "failed");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
    assert_eq!(w.approval(".driver", 7), "lean");
}

#[test]
fn a_retry_tick_of_a_classic_start_stays_classic_through_the_same_read() {
    let w = World::new("lean");
    lean_parked(&w, r#"{"links":[]}"#, "failed");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
    assert_eq!(w.approval(".driver", 7), "null");
}

#[test]
fn a_retry_tick_on_the_park_of_a_refused_lean_start_waits_while_tehas_lean_is_on_and_the_tick_stays(
) {
    let w = World::new("lean");
    lean_parked(&w, r#"{"links":[],"driver":"lean"}"#, "lean-refused");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    has(&r.out, "wait #7: a retry was ticked on the park of a lean start this tehas cannot run; take tehas:lean off first");
    lacks(&w.gh(), "api -X PATCH");
    lacks(&w.gh(), "issue comment 7");
}

#[test]
fn the_same_tick_starts_the_cycle_on_classic_once_tehas_lean_is_off() {
    let w = World::new("lean");
    // The world the case above leaves: the tick waited.
    lean_parked(&w, r#"{"links":[],"driver":"lean"}"#, "lean-refused");
    assert_eq!(w.poll().rc, 0);
    assert_eq!(w.started(), "");
    w.fx(
        "asking.json",
        r#"[{"number":7,"labels":[{"name":"needs-human-attention"},{"name":"tehas:question"}]}]"#,
    );
    w.label_events(7, &["tehas:lean=alice", "-tehas:lean=alice"]);
    w.clear_calls();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
    assert_eq!(w.approval(".driver", 7), "null");
}

#[test]
fn a_lean_start_whose_body_holds_no_pi_spec_block_starts_nothing_and_says_why_once() {
    let w = World::new("lean");
    lean_on(&w);
    w.fx(
        "ready.json",
        r#"[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"tehas:lean"}]}]"#,
    );
    w.label_events(7, &["tehas:lean=alice"]);
    w.fx(
        "issue-7.json",
        r#"{"title":"Faster search","body":"Make it fast, with no block."}"#,
    );
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    // The ask comes off, so this is said once, and tehas:lean stays: with it
    // off the next tehas:ready would run classic in lean's place.
    let gh = w.gh();
    has(
        &gh,
        "issue edit 7 --repo acme/widget --remove-label tehas:ready",
    );
    has(&gh, "issue comment 7 --repo acme/widget --body Tehas took `tehas:ready` off this issue and started nothing: its text holds no `pi-spec v1` block for the lean driver to read: it is not there, or it is not the one block of valid JSON lean needs. A person writes that block, and applies `tehas:ready` again.");
    assert_eq!(count_lines(&gh, "issue comment 7 "), 1, "{gh}");
}

#[test]
fn a_lean_start_whose_body_holds_the_block_is_started() {
    let w = World::new("lean");
    lean_on(&w);
    w.fx(
        "ready.json",
        r#"[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"tehas:lean"}]}]"#,
    );
    w.label_events(7, &["tehas:lean=alice"]);
    w.fx("issue-7.json", BLOCK_BODY);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
    assert_eq!(count_lines(&w.gh(), "issue comment 7 "), 1, "{}", w.gh());
}

#[test]
fn with_lean_off_tehas_lean_starts_nothing_classic_included_and_says_why_once() {
    let w = World::new("lean");
    w.fx(
        "ready.json",
        r#"[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"tehas:lean"}]}]"#,
    );
    w.label_events(7, &["tehas:lean=alice"]);
    w.fx("issue-7.json", BLOCK_BODY);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    has(
        &r.out,
        "skip #7: tehas:lean asks for the lean driver, still being built in this release",
    );
    let gh = w.gh();
    has(
        &gh,
        "issue edit 7 --repo acme/widget --remove-label tehas:ready",
    );
    lacks(&gh, "--remove-label tehas:lean");
    has(&gh, "issue comment 7 --repo acme/widget --body Tehas took `tehas:ready` off this issue and started nothing: `tehas:lean` asks for the lean driver, which is still being built in this release, and tehas does not run classic in its place.");
    assert_eq!(count_lines(&gh, "issue comment 7 "), 1, "{gh}");
}

#[test]
fn with_lean_off_a_lean_default_stops_the_poll_with_78_before_it_calls_github() {
    let w = World::new("lean");
    w.slots(1, &["TEHAS_DRIVER=lean"]);
    let r = w.poll();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "TEHAS_DRIVER=lean needs the feature lean, and lean is still being built in this release",
    );
    assert_eq!(w.gh(), "");
}
