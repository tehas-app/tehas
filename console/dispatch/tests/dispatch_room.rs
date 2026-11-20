//! Admission by the slice's room and pressure, as the poll reads them from the
//! cgroup tree, and the holds that leave planning and ticks alone. The pure
//! rules are in tests/fixtures/room.json.

mod common;

use common::{has, lacks, park_body, plan_body, World, GIB};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

fn pressure(w: &World) -> String {
    w.t()
        .join("cgroup/tehas.slice/memory.pressure")
        .to_string_lossy()
        .into_owned()
}

fn rm_pressure(w: &World) {
    fs::remove_file(pressure(w)).unwrap();
}

/// The files under `dir`, at any depth, whose name starts with `start`.
fn found(dir: &Path, start: &str) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| {
            let p = e.path();
            let me = usize::from(e.file_name().to_string_lossy().starts_with(start));
            if p.is_dir() {
                me + found(&p, start)
            } else {
                me
            }
        })
        .sum()
}

#[test]
fn no_room_nothing_starts_no_label_is_edited_the_tick_stays_exit_0_the_dry_run_names_the_room() {
    let w = World::new("room");
    w.parked("failed", "alice");
    w.comments(7, &park_body(&["x", " "]), "alice", json!([]));
    w.ready(&[8]);
    w.other(19 * GIB + GIB / 2);
    let hold = "holding: tehas.slice has no room for another cycle (19.5 GiB of 20.0 GiB promised, each cycle is given 2g)";
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    lacks(&w.gh(), "issue edit");
    lacks(&w.gh(), "api -X PATCH");
    lacks(&w.gh(), "issue comment");
    assert_eq!(w.park_q(7), "4001");
    has(&r.out, hold);
    let r = w.dispatch(&["--dry-run"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, hold);
}

#[test]
fn an_unreadable_probe_holds_and_names_the_file() {
    let w = World::new("room");
    rm_pressure(&w);
    let hold = format!(
        "holding: the slice's use could not be read: {} cannot be read",
        pressure(&w)
    );
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    lacks(&w.gh(), "issue edit");
    has(&r.out, &hold);
    assert_eq!(w.dispatch_state()["hold"], json!(hold));

    w.reset();
    w.other(GIB);
    let stat = w.t().join(format!(
        "cgroup/tehas.slice/docker-{}.scope/memory.stat",
        common::slice_id('d')
    ));
    fs::write(&stat, "anon many\n").unwrap();
    let r = w.dispatch(&["--dry-run"]);
    has(
        &r.out,
        &format!(
            "holding: the slice's use could not be read: {} has no number for anon",
            stat.display()
        ),
    );
    lacks(&r.out, "would run a cycle");

    // A planning turn is held too: its container would start in a slice
    // nobody can see into, after its label and comment were spent.
    w.to_plan();
    rm_pressure(&w);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.plan_calls(), "");
    lacks(&w.gh(), "issue edit");
    lacks(&w.gh(), "issue comment");
    has(&r.out, "holding: the slice's use could not be read");
    assert_eq!(w.dispatch_state()["why"], json!("unreadable"));

    w.planning();
    w.comments(7, &plan_body(" ", "x", " ", 1, ""), "alice", json!([]));
    rm_pressure(&w);
    w.poll();
    assert_eq!(w.plan_calls(), "");
    lacks(&w.gh(), "/comments -f");
}

#[test]
fn no_slice_a_waiting_planning_answer_is_left_as_it_is_no_label_is_edited_no_comment() {
    let w = World::new("room");
    w.planning();
    w.comments(7, &plan_body(" ", "x", " ", 1, ""), "alice", json!([]));
    w.no_slice();
    let before = w.rec();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.plan_calls(), "");
    for no in ["issue edit", "issue comment", "api -X", "/comments -f"] {
        lacks(&w.gh(), no);
    }
    assert_eq!(w.rec(), before);
    assert_eq!(found(&w.home.join("tmp"), "plan-answer."), 0);
    let r = w.dispatch(&["--dry-run"]);
    has(&r.out, "holding: no slice file on this host");
    lacks(&r.out, "would run a planning turn");
    // The same answer runs its turn once the slice is there.
    w.slice_show(&(20 * GIB).to_string(), "");
    w.poll();
    has(&w.plan_calls(), "--session abc-123");
}

#[test]
fn no_slice_the_way_out_of_a_planning_session_still_ends_it() {
    let w = World::new("room");
    w.planning();
    w.comments(7, &plan_body(" ", " ", "x", 1, ""), "alice", json!([]));
    w.no_slice();
    w.poll();
    assert_eq!(w.plan_calls(), "");
    has(&w.gh(), "Planning session ended by alice.");
    assert_eq!(w.rec_open(), "none");
}

#[test]
fn no_slice_a_plan_label_starts_no_session_and_keeps_its_label() {
    let w = World::new("room");
    w.to_plan();
    w.no_slice();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.plan_calls(), "");
    lacks(&w.gh(), "issue edit");
    lacks(&w.gh(), "issue comment");
    assert_eq!(w.rec(), Value::Null);
    assert_eq!(found(&w.home.join("tmp"), "plan-7.brief."), 0);
    let r = w.dispatch(&["--dry-run"]);
    has(&r.out, "holding: no slice file on this host");
    lacks(&r.out, "would start a planning session");
}
