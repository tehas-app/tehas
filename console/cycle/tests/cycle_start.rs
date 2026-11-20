//! What `tehas-cycle cycle` refuses to start, its lock and the notes it
//! leaves. The words and the settings the console checks in front of it are
//! console/src/commands.rs's tests and test/cycle_front_test.sh.

mod common;

use common::{count_lines, has, lacks, World};
use serde_json::{json, Value};
use std::fs::{self, File};
use std::path::Path;
use tehas_core::testing::write_exe;

/// What tehas work was called with.
fn work_calls(w: &World) -> String {
    w.calls("fake-work").trim_end().to_string()
}

/// Nothing was asked of GitHub and tehas work never ran.
#[track_caller]
fn nothing_ran(w: &World) {
    assert_eq!(format!("{}{}", w.gh(), w.calls("fake-work")), "");
}

fn cycle(w: &World, n: &str) -> Value {
    w.row(&["cycle-show", n])["cycle"].clone()
}

/// The cycle's attempt and whether it ended.
fn attempt_ended(w: &World, n: &str) -> Value {
    let c = cycle(w, n);
    json!([c["attempt"], !c["ended_at"].is_null()])
}

/// `path` locked, as another cycle holds it, until the file drops.
fn held(path: &Path) -> File {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let f = File::create(path).unwrap();
    f.try_lock().unwrap();
    f
}

const DELETE: &str = "api -X DELETE repos/acme/widget/issues/7/labels/needs-human-attention";

fn deletes(w: &World) -> usize {
    w.gh().lines().filter(|l| *l == DELETE).count()
}

fn needs_attention(w: &World) {
    w.fx(
        "issue-7.json",
        r#"{"number":7,"state":"open","labels":[{"name":"tehas:running"},{"name":"needs-human-attention"}]}"#,
    );
}

// The factory works on open issues only. What the number is decides it, and
// it is asked before a label is changed, a comment is posted or a container
// starts.
#[test]
fn a_closed_issue_is_asked_for_and_refused_no_cycle_no_label_no_comment() {
    for answer in [
        r#"{"number":7,"state":"closed"}"#,
        r#"{"number":7,"state":"CLOSED"}"#,
    ] {
        let w = World::new("start-closed");
        w.fx("issue-7.json", answer);
        let r = w.run7();
        assert_eq!(r.rc, 65);
        has(&r.out, "refuse #7: the issue is closed");
        has(&w.gh(), "api repos/acme/widget/issues/7");
        assert_eq!(work_calls(&w), "");
        lacks(&w.gh(), "issue edit");
        lacks(&w.gh(), "issue comment");
        // It still said it came up, and its record is ended as a refusal.
        let c = cycle(&w, "7");
        assert_eq!(
            json!([c["attempt"], !c["ended_at"].is_null(), c["end_reason"]]),
            json!([1, true, null])
        );
        assert_eq!(
            w.row(&["history-show", "7"])["rows"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }
}

#[test]
fn a_refused_start_whose_end_the_store_will_not_take_is_an_error_and_the_next_start_replaces_its_row(
) {
    let w = World::new("start-refused-end");
    w.fx("issue-7.json", r#"{"number":7,"state":"closed"}"#);
    w.store_answers("cycle-end", 1, "store: busy #7: the database is locked", 0);
    let r = w.run7();
    assert_eq!(r.rc, 65);
    has(
        &r.out,
        "ERROR: the refused start of #7 is not on record (busy): cycle-end 7 --attempt 1 --refused",
    );
    assert_eq!(w.store_answered(), 5);
    let c = cycle(&w, "7");
    assert_eq!(json!([c["attempt"], c["ended_at"]]), json!([1, null]));
    w.store_answers_off();
    fs::remove_file(w.fx.join("issue-7.json")).unwrap();
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert_eq!(cycle(&w, "7")["attempt"], 2);
}

#[test]
fn the_number_of_a_pull_request_is_refused_open_merged_or_closed() {
    for answer in [
        r#"{"number":9,"state":"open","pull_request":{"url":"u"}}"#,
        r#"{"number":9,"state":"closed","pull_request":{"url":"u"}}"#,
    ] {
        let w = World::new("start-pr");
        w.fx("issue-9.json", answer);
        let r = w.cycle(&["9"]);
        assert_eq!(r.rc, 65);
        has(
            &r.out,
            "refuse #9: it is a pull request, not an issue the factory works on",
        );
        assert_eq!(work_calls(&w), "");
        lacks(&w.gh(), "issue edit");
        assert_eq!(attempt_ended(&w, "9"), json!([1, true]));
    }
}

#[test]
fn an_answer_that_cannot_be_read_is_a_refusal_not_a_guess() {
    for answer in ["", "not json", "null", "[]", r#"{"message":"Not Found"}"#] {
        let w = World::new("start-unread");
        w.fx("issue-7.json", answer);
        let r = w.run7();
        assert_eq!(r.rc, 65, "{answer}");
        has(
            &r.out,
            "refuse #7: GitHub's answer for it could not be read",
        );
        assert_eq!(work_calls(&w), "");
        lacks(&w.gh(), "issue edit");
    }

    // GitHub could not be reached at all: three tries, then a refusal that
    // says the ask failed, which is not an answer that is unreadable.
    let w = World::new("start-unreached");
    w.touch("issue-fails");
    let r = w.run7();
    assert_eq!(r.rc, 65);
    has(
        &r.out,
        "refuse #7: GitHub could not be asked what the number is (3 tries, the last: exit 1)",
    );
    lacks(&r.out, "could not be read");
    let asks = |w: &World| {
        w.gh()
            .lines()
            .filter(|l| *l == "api repos/acme/widget/issues/7")
            .count()
    };
    assert_eq!(asks(&w), 3);
    assert_eq!(work_calls(&w), "");
    lacks(&w.gh(), "issue edit");
    assert_eq!(attempt_ended(&w, "7"), json!([1, true]));

    // A call that only failed is retried: the cycle the dispatcher already
    // authorized still starts.
    let w = World::new("start-retried");
    w.fx("issue-fails-times", "2\n");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert_eq!(asks(&w), 3);
    assert_eq!(work_calls(&w), "7");
    lacks(&r.out, "refuse #7");
}

#[test]
fn an_open_issue_is_asked_for_first_before_its_cycle_starts() {
    let w = World::new("start-first");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert_eq!(
        w.gh().lines().next().unwrap(),
        "api repos/acme/widget/issues/7"
    );
    assert_eq!(work_calls(&w), "7");
}

// --restart is the answer to a handoff: pi-rukas's attention gate lets it
// through. The cycle's container cannot take a label off (the relay refuses
// it), so the cycle takes needs-human-attention off on the host before the
// runner starts.
#[test]
fn a_cycle_restarts_only_an_issue_that_carries_needs_human_attention() {
    let w = World::new("start-restart");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert_eq!(work_calls(&w), "7");
    w.clear_calls();
    w.run7();
    assert_eq!(work_calls(&w), "7");
    w.clear_calls();
    needs_attention(&w);
    w.run7();
    assert_eq!(work_calls(&w), "7 --restart");
    assert_eq!(deletes(&w), 1);
    assert_eq!(w.gh().lines().nth(1).unwrap(), DELETE);
}

#[test]
fn a_restart_whose_label_cannot_be_taken_off_still_runs_and_the_log_says_so() {
    let w = World::new("start-restart-fails");
    needs_attention(&w);
    w.touch("unmark-fails");
    let r = w.run7();
    assert_eq!(work_calls(&w), "7 --restart");
    assert_eq!(deletes(&w), 3);
    has(
        &r.out,
        "WARN: #7: needs-human-attention could not be taken off before its restart",
    );
}

#[test]
fn a_restart_whose_label_a_person_took_off_already_is_not_tried_again() {
    let w = World::new("start-restart-gone");
    needs_attention(&w);
    w.touch("label-missing");
    let r = w.run7();
    assert_eq!(work_calls(&w), "7 --restart");
    assert_eq!(deletes(&w), 1);
    lacks(&r.out, "could not be taken off");
}

#[test]
fn a_start_of_an_issue_without_needs_human_attention_takes_no_label_off() {
    let w = World::new("start-no-label");
    w.run7();
    assert_eq!(work_calls(&w), "7");
    lacks(&w.gh(), "DELETE");
}

#[test]
fn it_says_that_it_holds_its_lock_and_lets_go_of_it_at_the_end() {
    // As the shell file had it: the sixth run of the restart cases above.
    let w = World::new("start-lock");
    for _ in 0..6 {
        assert_eq!(w.run7().rc, 0);
    }
    assert_eq!(attempt_ended(&w, "7"), json!([6, true]));
    let f = File::create(w.state().join("7.lock")).unwrap();
    assert!(f.try_lock().is_ok(), "the lock is still held");
}

// A run leaves its lock and the carriers kept to the next start beside it;
// its record and its line of history are on the store.
#[test]
fn a_cycle_leaves_its_lock_and_the_suffixed_notes_and_its_history_on_the_store() {
    let w = World::new("start-notes");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert_eq!(w.files(&w.state()), ["7.lock", "7.progress.json"]);
    let rows: Vec<Value> = w.row(&["history-show", "7"])["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| json!([r["attempt"], r["status"], r["how"]]))
        .collect();
    assert_eq!(json!(rows), json!([[1, "unknown", "hand"]]));
}

#[test]
fn a_second_cycle_for_the_same_issue_runs_nothing_exit_3() {
    let w = World::new("start-second");
    let _lock = held(&w.state().join("7.lock"));
    let r = w.run7();
    assert_eq!(r.rc, 3);
    has(&r.out, "a cycle for #7 is already running");
    nothing_ran(&w);
    assert_eq!(cycle(&w, "7"), Value::Null);
}

// --- The start on the state store ---------------------------------------------
#[test]
fn a_cycle_run_by_hand_is_on_record_as_one_uses_no_approval_and_runs_work_with_its_attempt() {
    let w = World::new("start-hand");
    w.approve("alice", "label", 1_760_000_000, &[]);
    let r = w.run7();
    assert_eq!(r.rc, 0);
    let c = cycle(&w, "7");
    assert_eq!(
        json!([c["how"], c["started_by"], c["requeued"]]),
        json!(["hand", null, false])
    );
    // The approval stays for the start it was recorded for.
    let a = w.row(&["approval-current", "7"]);
    assert_eq!(
        json!([a["approval"]["by"], a["current"]]),
        json!(["alice", null])
    );
    let h = w.history0();
    assert_eq!(json!([h["by"], h["how"]]), json!([null, "hand"]));
}

#[test]
fn work_is_given_the_attempt_of_the_start_and_nothing_new_reaches_it() {
    let w = World::new("start-attempt");
    w.work(r#"printf "%s\n" "${TEHAS_CYCLE_ATTEMPT:-none}" >"$T/calls/attempt""#);
    w.run7();
    assert_eq!(w.calls("attempt"), "1\n");
    w.run7();
    assert_eq!(w.calls("attempt"), "2\n");
}

#[test]
fn from_the_unit_a_start_uses_the_approval_the_dispatcher_recorded_and_the_account_is_on_record() {
    let mut w = World::new("start-unit");
    w.env("TEHAS_CYCLE_FROM", "unit");
    w.approve("alice", "label", 1_760_000_000, &[]);
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert_eq!(work_calls(&w), "7");
    let c = cycle(&w, "7");
    assert_eq!(
        json!([c["how"], c["started_by"]]),
        json!(["label", "alice"])
    );
    let a = w.row(&["approval-current", "7"]);
    assert_eq!(
        json!([a["approval"], a["current"]["used_by"]]),
        json!([null, 1])
    );
    let h = w.history0();
    assert_eq!(json!([h["by"], h["how"]]), json!(["alice", "label"]));
}

#[test]
fn systemds_invocation_id_is_a_start_from_the_unit_too() {
    let mut w = World::new("start-invocation");
    w.env("INVOCATION_ID", "0123456789abcdef");
    let r = w.run7();
    assert_eq!(r.rc, 77);
    assert_eq!(work_calls(&w), "");
}

#[test]
fn from_the_unit_no_approval_waiting_starts_nothing_exit_77_before_any_label_comment_or_call() {
    let mut w = World::new("start-unapproved");
    w.env("TEHAS_CYCLE_FROM", "unit");
    let r = w.run7();
    assert_eq!(r.rc, 77);
    has(
        &r.out,
        "refuse #7: the state store refused the start: store: unapproved #7",
    );
    nothing_ran(&w);
    assert_eq!(cycle(&w, "7"), Value::Null);
    // A used approval is no approval for a second start (A3).
    w.approve("alice", "label", 1_760_000_000, &[]);
    assert_eq!(w.run7().rc, 0);
    w.clear_calls();
    assert_eq!(w.run7().rc, 77);
    nothing_ran(&w);
    assert_eq!(cycle(&w, "7")["attempt"], 1);
}

#[test]
fn a_busy_store_is_asked_three_times_then_the_cycle_exits_75_and_changes_nothing() {
    let mut w = World::new("start-busy");
    w.env("TEHAS_CYCLE_FROM", "unit");
    w.approve("alice", "label", 1_760_000_000, &[]);
    w.store_answers(
        "cycle-start",
        1,
        "store: busy #7: the database is locked",
        0,
    );
    let r = w.run7();
    assert_eq!(r.rc, 75);
    assert_eq!(w.store_answered(), 3);
    nothing_ran(&w);
    let a = w.row(&["approval-current", "7"]);
    assert_eq!(
        json!([a["approval"]["by"], a["current"]]),
        json!(["alice", null])
    );
}

/// The store's first cycle-start ends as a store killed at its bound does
/// (124), after it committed the start or before; every other call runs the
/// real store.
fn start_times_out(w: &World, commit: bool) {
    let store = w.t().join("store-bin");
    let first = if commit {
        format!("'{}' \"$@\" >/dev/null 2>&1; exit 124", store.display())
    } else {
        "exit 124".to_string()
    };
    let bin = w.home.join("libexec/store");
    fs::remove_file(&bin).unwrap();
    let body = format!(
        "#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" >>\"$T/calls/store\"\nif [ \"${{1:-}}\" = cycle-start ] && [ ! -e \"$T/start.n\" ]; then\n  : >\"$T/start.n\"\n  {first}\nfi\nexec '{}' \"$@\"\n",
        store.display()
    );
    write_exe(&bin, body).unwrap();
}

fn starts(w: &World) -> usize {
    count_lines(&w.calls("store"), "cycle-start")
}

#[test]
fn a_start_the_store_took_before_it_timed_out_is_not_asked_again_it_is_ended_as_refused_exit_75() {
    let mut w = World::new("start-took");
    w.env("TEHAS_CYCLE_FROM", "unit");
    w.approve("alice", "label", 1_760_000_000, &[]);
    start_times_out(&w, true);
    let r = w.run7();
    assert_eq!(r.rc, 75);
    assert_eq!(starts(&w), 1);
    has(
        &r.out,
        "the state store took the start only after it timed out",
    );
    let c = cycle(&w, "7");
    assert_eq!(
        json!([c["attempt"], !c["ended_at"].is_null(), c["end_reason"]]),
        json!([1, true, null])
    );
    nothing_ran(&w);
}

#[test]
fn a_start_that_timed_out_before_the_store_took_it_is_asked_again() {
    let mut w = World::new("start-again");
    w.env("TEHAS_CYCLE_FROM", "unit");
    w.approve("alice", "label", 1_760_000_000, &[]);
    start_times_out(&w, false);
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert_eq!(starts(&w), 2);
    assert_eq!(cycle(&w, "7")["attempt"], 1);
}

#[test]
fn a_store_that_cannot_be_used_or_is_not_installed_stops_the_cycle_at_78_before_github_is_asked() {
    let w = World::new("start-corrupt");
    w.store_answers(
        "cycle-start",
        78,
        "store: corrupt: the file is not a database",
        0,
    );
    assert_eq!(w.run7().rc, 78);
    nothing_ran(&w);

    let w = World::new("start-no-store");
    fs::remove_file(w.home.join("libexec/store")).unwrap();
    let r = w.run7();
    assert_eq!(r.rc, 78);
    has(
        &r.out,
        "ERROR: the state store is not installed (run install.sh)",
    );
    nothing_ran(&w);
    assert!(!w.state().join("7.lock").exists());

    // A database that is a link or a directory is no store either.
    for link in [true, false] {
        let w = World::new("start-db");
        let db = w.home.join("state/tehas.db");
        let kept = w.t().join("tehas.db.kept");
        fs::rename(&db, &kept).unwrap();
        if link {
            std::os::unix::fs::symlink(&kept, &db).unwrap();
        } else {
            fs::create_dir(&db).unwrap();
        }
        let r = w.run7();
        assert_eq!(r.rc, 78);
        has(
            &r.out,
            "ERROR: the state store is not installed (run install.sh)",
        );
        assert!(!w.state().join("7.lock").exists());
        if link {
            fs::remove_file(&db).unwrap();
        } else {
            fs::remove_dir(&db).unwrap();
        }
        fs::rename(&kept, &db).unwrap();
    }
}

#[test]
fn the_account_on_record_is_the_approvals_whatever_is_approved_after_the_start() {
    let mut w = World::new("start-account");
    w.env("TEHAS_CYCLE_FROM", "unit");
    w.approve("alice", "label", 1_760_000_000, &[]);
    // While the cycle runs, a poll records bob's approval of a later label.
    w.work(r#"TEHAS_HOME=$T/home/tehas "$T/store-bin" approval-record 7 --at 1760000100 --by bob --how label --label tehas:ready --body-sha "$(printf a%.0s {1..64})" >/dev/null"#);
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert_eq!(cycle(&w, "7")["started_by"], "alice");
    assert_eq!(w.history0()["by"], "alice");
    let a = w.row(&["approval-current", "7"]);
    assert_eq!(
        json!([a["approval"]["by"], a["current"]["by"]]),
        json!(["bob", "alice"])
    );
}

#[test]
fn a_cycle_for_another_issue_is_not_held_back() {
    let w = World::new("start-other");
    let _lock = held(&w.state().join("8.lock"));
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert_eq!(work_calls(&w), "7");
}
