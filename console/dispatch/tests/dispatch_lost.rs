//! A cycle that died without a word: its record is open and no process holds
//! its lock. The poll asks the store to end it as lost, which writes its line
//! of history (T11.3), once it has been quiet as long as publish takes to
//! show it lost.

mod common;

use common::*;
use serde_json::json;

#[test]
fn an_open_record_no_cycle_holds_is_ended_as_lost_with_its_line_of_history() {
    let w = World::new("lost-record");
    w.t_approve(7, "alice", "approved", 1_700_000_000, &[]);
    let long_ago = 1_700_000_100;
    w.t_started(7, Some(long_ago), &[]);
    let attempt = w.attempt(7);
    let r = w.poll();
    has(
        &r.out,
        &format!(
            "lost #7: attempt {attempt} died without a word and no cycle holds its lock; ended as lost"
        ),
    );
    let rows = w.row(&["history-show", "7"])["rows"].clone();
    assert_eq!(
        json!([
            rows.as_array().unwrap().len(),
            rows[0]["status"],
            rows[0]["attempt"]
        ]),
        json!([1, "lost", attempt])
    );
    // The lost line ended its record, so the poll was free to start the
    // issue again on its label: the new start is the next attempt.
    assert!(w.attempt(7) > attempt);
}

#[test]
fn an_open_record_heard_from_just_now_is_left_alone() {
    let w = World::new("lost-heard");
    w.t_started(8, None, &["--by-hand"]);
    let attempt = w.attempt(8);
    let r = w.poll();
    assert!(!r.out.contains("lost #8"), "{}", r.out);
    assert!(!r.out.contains("WARN: the lost cycle"), "{}", r.out);
    assert_eq!(w.row(&["history-show", "8"])["rows"], json!([]));
    assert_eq!(w.attempt(8), attempt);
}
