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
