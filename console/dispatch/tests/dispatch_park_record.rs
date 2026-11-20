//! A retry tick that is not taken, and a question on record that this host
//! does not know or cannot read: what the poll writes to GitHub and the
//! store. The decisions are fixtures in tests/fixtures/park.json.

mod common;

use common::{has, lacks, park_body, World};
use serde_json::json;
use std::fs;
use std::process::{Command, Stdio};
use tehas_core::testing::{copy_exe, write_exe};

fn started(w: &World) -> String {
    w.calls("fake-start").trim_end().to_string()
}

/// jq -r of `filter` over `input`.
fn jqr(filter: &str, input: &str) -> String {
    use std::io::Write;
    let mut c = Command::new("jq")
        .args(["-r", filter])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    c.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let out = c.wait_with_output().unwrap();
    String::from_utf8_lossy(&out.stdout)
        .trim_end_matches('\n')
        .to_string()
}

/// t_row: a read verb of the store through a jq filter.
fn row(w: &World, args: &[&str], filter: &str) -> String {
    jqr(filter, &w.row(args).to_string())
}

/// #7 parked, its retry ticked by alice.
fn retry_ticked(w: &World) {
    w.parked("failed", "alice");
    w.comments(7, &park_body(&["x", " "]), "alice", json!([]));
}

/// #7 as an epic with 13 sub-issues, `done` of them closed.
fn epic(w: &World, done: u64) {
    w.fx_json(
        "work-7.json",
        &json!({"number": 7, "state": "open", "title": "Faster search", "body": "Make it fast.",
            "labels": [{"name": "needs-human-attention"}, {"name": "tehas:question"}],
            "sub_issues_summary": {"total": 13, "completed": done}}),
    );
}

#[test]
fn a_retry_tick_on_an_issue_whose_cycle_was_an_epic_starts_nothing_and_the_tick_stays() {
    let w = World::new("record-epic");
    retry_ticked(&w);
    epic(&w, 0);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(started(&w), "");
    has(
        &r.out,
        "skip #7: it is an epic with 13 sub-issues open, and an epic is never started",
    );
    // No label of tehas's on it is theirs to take, and the tick stays for a
    // person to deal with.
    lacks(&w.gh(), "issue edit 7");
    lacks(&w.gh(), "issue comment 7");
    lacks(&w.gh(), "api -X PATCH");
    assert_eq!(w.park_q(7), "4001");
}

#[test]
fn and_a_dry_run_of_that_tick_says_the_same_and_changes_nothing() {
    let w = World::new("record-epic-dry");
    retry_ticked(&w);
    epic(&w, 0);
    let r = w.dispatch(&["--dry-run"]);
    has(
        &r.out,
        "skip #7: it is an epic with 13 sub-issues open, and an epic is never started",
    );
    lacks(&r.out, "would run a cycle");
    lacks(&w.gh(), "issue edit");
    lacks(&w.gh(), "api -X");
    assert_eq!(w.park_q(7), "4001");
}

#[test]
fn a_retry_tick_is_used_up_once_the_issue_is_marked() {
    let w = World::new("record-used-up");
    retry_ticked(&w);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(started(&w), "7");
    let gh = w.gh();
    has(&gh, "api -X PATCH repos/acme/widget/issues/comments/4001");
    has(&gh, "- [ ] Retry from a clean start");
    let line = |want: &str| gh.lines().position(|l| l.contains(want));
    let (patch, start) = (line("api -X PATCH"), line("--add-label tehas:running"));
    assert!(
        matches!((patch, start), (Some(p), Some(s)) if s < p),
        "the untick does not follow the mark:\n{gh}"
    );
}

#[test]
fn when_the_issue_cannot_be_marked_the_tick_stays_and_the_next_poll_starts_the_retry() {
    let w = World::new("record-mark-fails");
    retry_ticked(&w);
    w.fx("edit-fails-on", "--add-label tehas:running");
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "ERROR: could not mark #7 as running, not starting a cycle",
    );
    assert_eq!(started(&w), "");
    lacks(&w.gh(), "api -X PATCH");
    assert_eq!(w.park_q(7), "4001");
    w.rm_fx("edit-fails-on");
    w.clear_calls();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(started(&w), "7");
    has(
        &w.gh(),
        "api -X PATCH repos/acme/widget/issues/comments/4001",
    );
}

#[test]
fn when_the_tick_cannot_be_undone_the_cycle_still_starts_and_the_tick_starts_nothing_more() {
    let w = World::new("record-untick-fails");
    retry_ticked(&w);
    w.touch("patch-fails");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(started(&w), "7");
    has(&r.out, "WARN: could not untick the retry on #7; its question is no longer on record, so it starts nothing more");
    assert_eq!(w.park_q(7), "none");
}

#[test]
fn a_start_whose_question_cannot_be_taken_off_the_record_starts_nothing_and_its_tick_and_question_stay(
) {
    let w = World::new("record-withdraw-refused");
    retry_ticked(&w);
    w.store_refuses(
        "question-answered 7 --id 4001 --withdraw",
        1,
        "store: busy #7: the database is locked",
    );
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "ERROR: #7: the question of its park could not be taken off the record (busy), not starting a cycle");
    assert_eq!(started(&w), "");
    lacks(&w.gh(), "issue edit 7");
    lacks(&w.gh(), "api -X PATCH");
    assert_eq!(w.park_q(7), "4001");
    assert_eq!(row(&w, &["approval-current", "7"], ".approval"), "null");
}

#[test]
fn a_withdraw_that_timed_out_after_it_was_done_puts_the_question_back_and_the_next_poll_starts_the_retry(
) {
    let w = World::new("record-withdraw-timeout");
    retry_ticked(&w);
    let bin = w.home.join("libexec/store");
    let store = w.t().join("store-bin");
    fs::remove_file(&bin).unwrap();
    write_exe(
        &bin,
        format!(
            "#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" >>'{calls}'\nif [ \"${{1:-}}\" = question-answered ]; then\n  '{s}' \"$@\" >/dev/null 2>&1; exit 124\nfi\nexec '{s}' \"$@\"\n",
            calls = w.t().join("calls/store").display(),
            s = store.display()
        ),
    )
    .unwrap();
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "could not be taken off the record (timeout)");
    assert_eq!(started(&w), "");
    assert_eq!(w.park_q(7), "4001");
    fs::remove_file(&bin).unwrap();
    w.clear_calls();
    copy_exe(&store, &bin).unwrap();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(started(&w), "7");
}

#[test]
fn a_retry_the_store_would_not_spend_once_the_issue_is_marked_is_a_warning_and_the_cycle_starts() {
    let w = World::new("record-spend-refused");
    retry_ticked(&w);
    w.store_refuses("park-retry 7", 1, "store: busy #7: the database is locked");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "WARN: #7: the retry of its park could not be spent (busy)",
    );
    assert_eq!(started(&w), "7");
    assert_eq!(w.park_q(7), "none");
}

fn not_parked(w: &World, labels: &str) {
    let l: Vec<_> = labels.split(' ').map(|n| json!({"name": n})).collect();
    w.fx_json("asking.json", &json!([{"number": 7, "labels": l}]));
}

#[test]
fn a_retry_ticked_on_an_issue_that_is_not_parked_starts_nothing() {
    let w = World::new("record-not-parked");
    for have in [
        "tehas:question",
        "needs-human-attention tehas:question tehas:running",
        "needs-human-attention tehas:question tehas:awaiting-merge",
    ] {
        retry_ticked(&w);
        not_parked(&w, have);
        let r = w.poll();
        assert_eq!(started(&w), "", "{have}");
        let gh = w.gh();
        lacks(&gh, "issue edit");
        // A tick left in place would fire later, when the labels change.
        has(&gh, "api -X PATCH repos/acme/widget/issues/comments/4001");
        has(&gh, "- [ ] Retry from a clean start");
        has(&gh, "issue comment 7 --repo acme/widget --body Retry was ticked, but this issue is not parked, so nothing was started.");
        has(
            &r.out,
            "skip #7: a retry was ticked but the issue is not parked",
        );
        assert_eq!(w.park_q(7), "4001", "{have}");
    }
}

#[test]
fn when_that_tick_cannot_be_undone_nothing_is_said() {
    let w = World::new("record-not-parked-untick");
    retry_ticked(&w);
    not_parked(&w, "tehas:question");
    w.touch("patch-fails");
    let r = w.poll();
    assert_eq!(started(&w), "");
    lacks(&w.gh(), "issue comment");
    has(&r.out, "could not untick the question on #7");
}

#[test]
fn a_retry_tick_on_an_issue_closed_since_its_cycle_stopped_starts_nothing() {
    let w = World::new("record-closed");
    retry_ticked(&w);
    w.fx("work-7.json", r#"{"number":7,"state":"closed"}"#);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(started(&w), "");
    lacks(&w.gh(), "issue edit 7");
    lacks(&w.gh(), "api -X PATCH");
    assert_eq!(w.park_q(7), "4001");
}

// A question is the factory's only if this host has its id on record. The
// agent in a container holds the bot's token: it can post a comment with a
// marker and apply labels, but it cannot write the host's state directory.
/// Issue 9, never parked by this host, made to look parked.
fn unrecorded(w: &World) {
    w.reset();
    w.fx("ready.json", "[]");
    w.fx(
        "asking.json",
        r#"[{"number":9,"labels":[{"name":"needs-human-attention"},{"name":"tehas:question"}]}]"#,
    );
    fs::copy(w.fx.join("events-7.json"), w.fx.join("events-9.json")).unwrap();
    w.comments(
        9,
        "Please confirm you have seen this.\n\n- [x] Retry from a clean start\n\n<!-- tehas:question kind=park issue=3 turn=1 -->",
        "alice",
        json!([]),
    );
}

#[test]
fn a_bot_comment_with_a_park_marker_that_the_host_has_no_record_of_starts_nothing() {
    let w = World::new("record-unrecorded");
    unrecorded(&w);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(started(&w), "");
    let gh = w.gh();
    lacks(&gh, "issue edit");
    has(&gh, "api -X PATCH repos/acme/widget/issues/comments/4001");
    has(&gh, "- [ ] Retry from a clean start");
    has(&gh, "issue comment 9 --repo acme/widget --body This question is not one the factory is waiting on. To retry, apply `tehas:ready`.");
    assert_eq!(w.park_q(9), "none");
}

#[test]
fn and_when_its_box_cannot_be_unticked_nothing_starts_and_nothing_is_said() {
    let w = World::new("record-unrecorded-untick");
    unrecorded(&w);
    w.touch("patch-fails");
    let r = w.poll();
    assert_eq!(started(&w), "");
    lacks(&w.gh(), "issue edit");
    lacks(&w.gh(), "issue comment");
    has(&r.out, "could not untick the question on #9");
}

#[test]
fn a_question_on_record_of_another_issue_is_no_record_for_this_one() {
    let w = World::new("record-other-issue");
    unrecorded(&w);
    // 4001 is the question of #7's park. The tick on #9 is in a comment of
    // that id.
    w.t_ended(7, "alice", "label", &["--parked"]);
    w.t_park(7, "failed", "4001", &[]);
    w.poll();
    assert_eq!(started(&w), "");
    has(
        &w.gh(),
        "This question is not one the factory is waiting on.",
    );
    assert_eq!(w.park_q(7), "4001");
}

#[test]
fn a_tick_in_a_question_taken_off_the_record_starts_nothing() {
    let w = World::new("record-withdrawn");
    // As the cycle leaves it when the label tehas:question could not be added.
    retry_ticked(&w);
    w.store_ok(&["question-answered", "7", "--id", "4001", "--withdraw"]);
    let r = w.poll();
    assert_eq!(started(&w), "");
    has(
        &r.out,
        "skip #7: a box was ticked in a question this host has no record of",
    );
    has(
        &w.gh(),
        "This question is not one the factory is waiting on.",
    );
}

#[test]
fn a_question_of_an_earlier_attempt_is_not_honoured_after_a_new_start() {
    let w = World::new("record-earlier");
    retry_ticked(&w);
    // A later cycle of #7, by hand, that ended without a park.
    w.t_ended(7, "-", "", &[]);
    let r = w.poll();
    assert_eq!(started(&w), "");
    has(
        &r.out,
        "skip #7: a box was ticked in a question this host has no record of",
    );
    assert_eq!(
        row(
            &w,
            &["question-known", "7", "--id", "4001"],
            "[.known, .recorded] | tostring"
        ),
        "[false,true]"
    );
}

#[test]
fn a_question_on_record_that_could_not_be_read_skips_the_issue_no_tick_counts_and_none_is_undone() {
    let w = World::new("record-unreadable");
    for refused in ["park-show 7", "question-known 7"] {
        w.parked("failed", "alice");
        // A stranger's tick, which a poll that read the record would undo.
        w.comments(7, &park_body(&["x", " "]), "mallory", json!([]));
        w.store_refuses(refused, 1, "store: busy #7: the database is locked");
        let r = w.poll();
        assert_eq!(r.rc, 0, "{refused}: {}", r.out);
        assert_eq!(started(&w), "", "{refused}");
        has(
            &r.out,
            "skip #7: its question on record could not be read (busy)",
        );
        lacks(&w.gh(), "api -X PATCH");
        lacks(&w.gh(), "issue comment");
    }
}

#[test]
fn a_store_that_cannot_be_used_for_the_question_on_record_stops_the_poll_at_78_and_nothing_is_undone(
) {
    let w = World::new("record-corrupt");
    w.parked("failed", "alice");
    w.comments(7, &park_body(&["x", " "]), "mallory", json!([]));
    w.store_refuses(
        "park-show 7",
        78,
        "store: corrupt: the file is not a database",
    );
    let r = w.poll();
    assert_eq!(r.rc, 78, "{}", r.out);
    assert_eq!(started(&w), "");
    has(
        &r.out,
        "ERROR: the state store cannot be used: store: corrupt",
    );
    lacks(&w.gh(), "api -X PATCH");
}

/// A park comment whose leave box comes first.
fn swapped(leave: &str, retry: &str) -> String {
    format!("Tehas cycle stopped.\n\n- [{leave}] Leave it parked\n- [{retry}] Retry from a clean start\n\nTick one box.\n\n<!-- tehas:question kind=park issue=7 turn=1 -->")
}

#[test]
fn retry_is_the_ticked_lines_text_not_its_place() {
    let w = World::new("record-swapped");
    w.parked("failed", "alice");
    w.comments(7, &swapped("x", " "), "alice", json!([]));
    w.poll();
    assert_eq!(started(&w), "");
    has(
        &w.gh(),
        "issue edit 7 --repo acme/widget --remove-label tehas:question",
    );
    w.parked("failed", "alice");
    w.comments(7, &swapped(" ", "x"), "alice", json!([]));
    w.poll();
    assert_eq!(started(&w), "7");
}
