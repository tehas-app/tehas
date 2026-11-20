//! The limit tehas limit writes, as the turns it lets run are started. The
//! rules are in tests/fixtures/limit.json.

mod common;

use common::{has, lacks, park_body, plan_body, World};
use serde_json::json;

#[test]
fn limit_planning_off_holds_a_new_planning_session_not_an_answer() {
    let w = World::new("limit");
    w.to_plan();
    w.limit(3, false, "", "");
    w.poll();
    assert_eq!(w.plan_calls(), "");
    lacks(&w.gh(), "--add-label tehas:planning");
    w.planning();
    w.limit(3, false, "", "");
    w.comments(7, &plan_body(" ", "x", " ", 1, ""), "alice", json!([]));
    w.poll();
    has(
        &w.plan_calls(),
        &format!(
            "--session abc-123 --answer {}/answer.md",
            w.keep().display()
        ),
    );
}

#[test]
fn limit_cycles_0_holds_a_retry_tick_planning_on_lets_a_session_start() {
    let w = World::new("limit");
    w.parked("failed", "alice");
    w.limit(0, true, "", "");
    w.comments(7, &park_body(&["x", " "]), "alice", json!([]));
    w.poll();
    assert_eq!(w.started(), "");
    w.to_plan();
    w.limit(0, true, "", "");
    w.poll();
    has(
        &w.plan_calls(),
        &format!("7 --out {}.out", w.record().display()),
    );
}

#[test]
fn limit_expired_unreadable_or_of_another_version_nothing_starts_and_the_hold_says_why() {
    let expired = "holding: the limit expired at 2020-01-01T00:00:00Z: nothing starts";
    let unreadable = "holding: the limit cannot be read: nothing starts";
    for (kind, file, hold) in [
        ("expired", "", expired),
        ("unreadable", "not a limit\n", unreadable),
        (
            "version",
            "{\"v\":2,\"cycles\":3,\"plan\":true,\"until\":null}\n",
            unreadable,
        ),
    ] {
        let w = World::new("limit");
        w.slots(2, &[]);
        w.ready(&[7]);
        if file.is_empty() {
            w.limit(3, true, "2020-01-01T00:00:00Z", "");
        } else {
            w.home_file("state/limit.json", file);
        }
        let r = w.poll();
        assert_eq!(r.rc, 0, "{kind}: {}", r.out);
        assert_eq!(w.started(), "", "{kind}: a cycle started");
        let d = w.dispatch_state();
        assert_eq!(d["why"], json!("limit"), "{kind}");
        has(d["hold"].as_str().unwrap_or_default(), hold);
    }
}
