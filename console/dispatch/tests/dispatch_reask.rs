//! The first poll of 2.0 on a store of v1.4.0 (#2782): each question asked
//! before 2.0 and open at the upgrade is asked again in 2.0's words, once,
//! and its old comment is told where it went.

mod common;

use common::{count_lines, has, Part::Off, World};
use serde_json::json;
use std::fs;
use tehas_core::question::{question_marker, reasked, Kind};
use tehas_ports::{Mark, State};

/// #21's park question as tehas v1.4.0 posted it.
fn old_park() -> String {
    format!("Tehas cycle stopped without a pull request: status `failed`, last step `develop`.\n\n- [ ] Retry with the issue text only\n- [ ] Leave it parked\n\nTick one box, or reply in a comment.\n\n{}", question_marker("park", 21, 1))
}

/// #22's planning question as tehas v1.4.0 posted it.
fn old_plan() -> String {
    format!("Adopt?\n\n- [ ] Adopt all\n  Apply all five\n- [ ] Change some\n- [ ] End this planning session\n\nTick one box, or reply in a comment.\n\n{}", question_marker("plan", 22, 1))
}

/// The home's store made by v1.4.0, with a retry ticked and the two
/// questions open, then migrated as `tehas update` does.
fn upgraded(w: &World) {
    let state = tehas_core::home::Home::new(&w.home).state_dir();
    for f in ["tehas.db", "tehas.db-wal", "tehas.db-shm"] {
        let _ = fs::remove_file(state.join(f));
    }
    let c = rusqlite::Connection::open(state.join("tehas.db")).unwrap();
    c.pragma_update(None, "journal_mode", "WAL").unwrap();
    c.execute_batch(include_str!("../../store/tests/fixtures/store-v1.4.0.sql"))
        .unwrap();
    c.execute_batch(include_str!(
        "../../store/tests/fixtures/store-v1.4.0-open.sql"
    ))
    .unwrap();
    c.execute_batch("UPDATE meta SET value = 'acme/widget' WHERE key = 'repo';")
        .unwrap();
    drop(c);
    use std::os::unix::fs::PermissionsExt;
    for f in ["tehas.db", "tehas.db-wal", "tehas.db-shm"] {
        let _ = fs::set_permissions(state.join(f), fs::Permissions::from_mode(0o600));
    }
    let backup = state.join("backup");
    fs::create_dir_all(&backup).unwrap();
    fs::set_permissions(&backup, fs::Permissions::from_mode(0o700)).unwrap();
    w.store_ok(&[
        "migrate",
        "--backup",
        backup.to_str().unwrap(),
        "--version",
        "v2.0.0",
        "--commit",
        "0123456789abcdef0123456789abcdef01234567",
        "--instance",
        "instance",
        "--repo",
        "acme/widget",
    ]);
}

/// The upgraded home, asking #21 and #22, their old comments in place.
fn asking(w: &World, park: &str) {
    upgraded(w);
    for f in ["ready.json", "plans.json", "marked.json", "awaiting.json"] {
        w.fx(f, "[]");
    }
    w.fx("asking.json", r#"[{"number":21},{"number":22}]"#);
    w.comments_of(21, 2101, park, "");
    w.comments_of(22, 2201, &old_plan(), "");
    w.fx("post-id-21", "4121");
    w.fx("post-id-22", "4122");
    w.clear_calls();
}

/// A stop of before 2.0 that offered no retry is not asked again: 2.0 asks
/// nothing of it. Its comment is told it is closed, and it goes off the
/// record.
#[test]
fn a_stop_that_offered_no_retry_is_closed_and_not_asked_again() {
    let w = World::new("reask-none");
    let park = old_park().replace("- [ ] Retry with the issue text only\n", "");
    asking(&w, &park);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    let gh = w.gh();
    assert_eq!(
        count_lines(&gh, "api -X POST repos/acme/widget/issues/21/comments"),
        0,
        "{gh}"
    );
    has(
        &gh,
        &format!("\n\n{}", tehas_core::question::superseded(None)),
    );
    let k = w.row(&["question-known", "21", "--id", "2101"]);
    assert_eq!(k["recorded"], json!(false), "{k}");
    assert_eq!(w.row(&["question-show", "21"])["question"], json!(null));
    // It waits on no question any more.
    assert!(
        w.changes(21, &[Off(Mark::State(State::Question))]),
        "{:?}",
        w.writes_of(21)
    );
    // The next poll, with the old comment as it now reads, does nothing.
    let closed = format!("{park}\n\n{}", tehas_core::question::superseded(None));
    w.comments_of(21, 2101, &closed, "");
    w.clear_calls();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    let gh = w.gh();
    assert_eq!(count_lines(&gh, "api -X POST"), 0, "{gh}");
    assert_eq!(count_lines(&gh, "api -X PATCH"), 0, "{gh}");
    assert_eq!(w.marks(Some(21)), 0, "{gh}");
}

/// The old question goes off the record before its comment is edited: a
/// withdrawal the store refuses edits nothing, poll after poll, so the closing
/// line is never added twice; and an old comment that already ends in it is
/// not edited again.
#[test]
fn a_withdrawal_that_fails_edits_nothing_and_a_closed_comment_is_not_edited_again() {
    let w = World::new("reask-withdraw");
    let park = old_park().replace("- [ ] Retry with the issue text only\n", "");
    asking(&w, &park);
    w.store_answers("question-answered 21", 1, "store: busy the database", 0);
    for _ in 0..2 {
        w.clear_calls();
        let r = w.poll();
        has(
            &r.out,
            "WARN: the old question of #21 could not be taken off the record",
        );
        assert_eq!(
            count_lines(
                &w.gh(),
                "api -X PATCH repos/acme/widget/issues/comments/2101"
            ),
            0
        );
    }
    let w = World::new("reask-closed");
    let closed = format!("{park}\n\n{}", tehas_core::question::superseded(None));
    asking(&w, &closed);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        count_lines(
            &w.gh(),
            "api -X PATCH repos/acme/widget/issues/comments/2101"
        ),
        0
    );
    assert_eq!(
        w.row(&["question-known", "21", "--id", "2101"])["recorded"],
        json!(false)
    );
}

#[test]
fn the_first_poll_asks_each_open_question_again_once_and_tells_the_old_comment() {
    let w = World::new("reask");
    asking(&w, &old_park());
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    let gh = w.gh();
    for (n, old, post, patch, new) in [
        (
            21,
            2101,
            "api -X POST repos/acme/widget/issues/21/comments",
            "api -X PATCH repos/acme/widget/issues/comments/2101",
            4121,
        ),
        (
            22,
            2201,
            "api -X POST repos/acme/widget/issues/22/comments",
            "api -X PATCH repos/acme/widget/issues/comments/2201",
            4122,
        ),
    ] {
        assert_eq!(count_lines(&gh, post), 1, "{gh}");
        assert_eq!(count_lines(&gh, patch), 1, "{gh}");
        has(
            &r.out,
            &format!("asked #{n}'s question again in the words of 2.0 (comment {old} superseded)"),
        );
        // The old comment, as it was, with the one line that says where it went.
        has(
            &gh,
            &format!("\n\n{}", tehas_core::question::superseded(Some(new))),
        );
        // The old one is off the record.
        let k = w.row(&["question-known", &n.to_string(), "--id", &old.to_string()]);
        assert_eq!(k["recorded"], json!(false), "{k}");
    }
    has(&gh, "Reply in a comment with the word of one of these:");
    has(
        &gh,
        "Reply in a comment with the number of one of these, or answer in your own words:",
    );
    // The new ones are on record, unmarked, the stop's with what it offers,
    // each with a number of the store's after those of the old ones.
    let q = w.row(&["question-show", "21"])["question"].clone();
    assert_eq!(q["offers"], json!(["retry-text", "park"]), "{q}");
    assert!(q["id"].as_i64().is_some_and(|id| id > 2), "{q}");
    let k = w.row(&["question-known", "21", "--id", "4121"]);
    assert_eq!(
        (k["known"].clone(), k["reask"].clone()),
        (json!(true), json!(false)),
        "{k}"
    );
    let q = w.row(&["question-show", "22"])["question"].clone();
    assert!(q["id"].as_i64().is_some_and(|id| id > 2), "{q}");
    assert_eq!(
        w.row(&["question-known", "22", "--id", "4122"])["known"],
        json!(true)
    );
    // The next poll finds the new questions and does nothing.
    let new21 = reasked(Kind::Park, 21, 1, &old_park())
        .unwrap()
        .body
        .unwrap();
    let new22 = reasked(Kind::Plan, 22, 1, &old_plan())
        .unwrap()
        .body
        .unwrap();
    w.comments_of(21, 4121, &new21, "");
    w.comments_of(22, 4122, &new22, "");
    w.clear_calls();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    let gh = w.gh();
    assert_eq!(count_lines(&gh, "api -X POST"), 0, "{gh}");
    assert_eq!(count_lines(&gh, "api -X PATCH"), 0, "{gh}");
    assert!(!r.out.contains("asked #"), "{}", r.out);
}
