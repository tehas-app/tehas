//! What the slots and the room under the review cap reach in
//! live/dispatch.json.

mod common;

use common::World;
use serde_json::{json, Value};

/// The two figures of live/dispatch.json this file reads.
fn room(w: &World) -> Value {
    let d = w.dispatch_state();
    json!({"free": d["free"], "awaitRoom": d["awaitRoom"]})
}

#[test]
fn the_slots_and_the_room_under_the_review_cap_reach_live_dispatch_json() {
    let w = World::new("queue-room");
    w.slots(3, &[]);
    w.fx("awaiting.json", r#"[{"number":20},{"number":21}]"#);
    w.fx("ready.json", "[]");
    w.hold(5);
    w.poll();
    w.release();
    assert_eq!(room(&w), json!({"free": 2, "awaitRoom": 2}));
    w.reset();
    w.hold(5);
    w.poll();
    w.release();
    assert_eq!(room(&w), json!({"free": 0, "awaitRoom": null}));
}
