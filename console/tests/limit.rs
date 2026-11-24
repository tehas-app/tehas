//! `tehas limit` end to end: setting, showing and clearing the factory's
//! limit, the file it keeps, the words it refuses, and the console's reading
//! of it for everything that honours it. The limit's rules are beside its
//! code in console/ops/src/holds.rs. gh is a stub that answers nothing: a
//! limit asks GitHub for nothing.

mod common;

use common::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use tehas_core::home::State;
use tehas_core::time::{iso, now};

fn host(name: &str) -> World {
    let w = World::new(&format!("limit-{name}"));
    w.gh_stub("");
    w
}

/// The world of a case: no home's state, the minimal instance.
fn reset(w: &World) {
    let _ = fs::remove_dir_all(w.at().state_dir());
    w.reset();
    let _ = fs::remove_file(file(w));
}

fn file(w: &World) -> PathBuf {
    w.at().state(State::Limit)
}

fn limit(w: &World, words: &[&str]) -> Run {
    w.tehas("limit", words)
}

fn show(w: &World) -> serde_json::Value {
    let r = limit(w, &["--json"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    serde_json::from_str(&r.out).unwrap()
}

/// The limit as the console reads it, at `at` epoch seconds or now.
fn read_limit(w: &World, at: Option<i64>) -> String {
    let mut words = vec!["flags".to_string(), "limit".into(), "read".into()];
    if let Some(at) = at {
        words.extend(["--now".to_string(), at.to_string()]);
    }
    w.sh(&format!("\"$TEHAS_CONSOLE\" {}", words.join(" "))).out
}

fn in_secs(s: i64) -> String {
    iso(now() + s)
}

fn kept(w: &World) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(file(w)).unwrap()).unwrap()
}

#[test]
fn no_limit_is_none_in_words_and_as_json() {
    let w = host("none");
    let r = limit(&w, &[]);
    assert_eq!((r.rc, r.out.as_str()), (0, "limit     none"));
    let r = limit(&w, &["--json"]);
    assert_eq!((r.rc, r.out.as_str()), (0, r#"{"api":1,"state":"none"}"#));
    assert_eq!(w.gh(), "");
}

#[test]
fn set_cycles_is_a_limit_with_planning_on_and_no_expiry() {
    let w = host("cycles");
    let r = limit(&w, &["set", "--cycles", "2"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        r.out,
        "limit     2 cycles at once, planning on, until cleared"
    );
    let mut shown = show(&w);
    assert_eq!(shown["set"], kept(&w)["set"]);
    shown.as_object_mut().unwrap().remove("set");
    assert_eq!(
        shown.to_string(),
        r#"{"api":1,"cycles":2,"plan":true,"state":"ok"}"#
    );
    assert_eq!(
        fs::metadata(file(&w)).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let temps = fs::read_dir(w.at().state_dir())
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".new")
        })
        .count();
    assert_eq!(temps, 0);
}

#[test]
fn set_with_every_flag_has_planning_off_an_expiry_and_a_reason() {
    let w = host("every");
    let r = limit(
        &w,
        &[
            "set",
            "--cycles",
            "1",
            "--plan",
            "off",
            "--for",
            "300",
            "--reason",
            "held for alpha",
        ],
    );
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "limit     1 cycle at once, planning off, until 20");
    has(&r.out, ": held for alpha");
    let shown = show(&w);
    assert_eq!(shown["state"], "ok");
    assert_eq!(shown["cycles"], 1);
    assert_eq!(shown["plan"], false);
    assert_eq!(shown["reason"], "held for alpha");
    let left = epoch(shown["until"].as_str().unwrap()) - now();
    assert!(left > 290 && left <= 300, "{left}");
}

#[test]
fn set_until_a_time_and_setting_it_twice_leaves_the_same_file() {
    let w = host("until");
    let until = in_secs(3600);
    let r = limit(&w, &["set", "--cycles", "0", "--until", &until, "--json"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    let mut said: serde_json::Value = serde_json::from_str(&r.out).unwrap();
    assert_eq!(said["set"], kept(&w)["set"]);
    said.as_object_mut().unwrap().remove("set");
    assert_eq!(
        said.to_string(),
        format!(r#"{{"api":1,"cycles":0,"plan":true,"state":"ok","until":"{until}"}}"#)
    );

    reset(&w);
    let words = [
        "set", "--cycles", "3", "--plan", "off", "--until", &until, "--reason", "r",
    ];
    assert_eq!(limit(&w, &words).rc, 0);
    let first = fs::read_to_string(file(&w)).unwrap();
    assert_eq!(limit(&w, &words).rc, 0);
    assert_eq!(fs::read_to_string(file(&w)).unwrap(), first);
}

#[test]
fn a_new_set_replaces_the_whole_limit_and_clear_takes_it_off() {
    let w = host("replace");
    let old = [
        "set", "--cycles", "1", "--plan", "off", "--for", "60", "--reason", "old",
    ];
    assert_eq!(limit(&w, &old).rc, 0);
    assert_eq!(limit(&w, &["set", "--cycles", "4"]).rc, 0);
    let mut shown = show(&w);
    shown.as_object_mut().unwrap().remove("set");
    assert_eq!(
        shown.to_string(),
        r#"{"api":1,"cycles":4,"plan":true,"state":"ok"}"#
    );
    let r = limit(&w, &["clear"]);
    assert_eq!((r.rc, r.out.as_str()), (0, "limit     none"));
    assert!(!file(&w).exists());
    assert_eq!(limit(&w, &["clear"]).rc, 0);
}

#[test]
fn the_reason_is_one_line_of_at_most_200_characters() {
    let w = host("reason");
    let r = limit(&w, &["set", "--cycles", "1", "--reason", "a\tb\nc\u{1}d"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(show(&w)["reason"], "a b c d");
    let long = "x".repeat(300);
    assert_eq!(
        limit(&w, &["set", "--cycles", "1", "--reason", &long]).rc,
        0
    );
    assert_eq!(show(&w)["reason"], "x".repeat(200));
    let r = limit(&w, &["set", "--cycles", "1", "--reason", "--json"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(kept(&w)["reason"], "--json");
}

#[test]
fn bad_arguments_are_64_and_write_nothing() {
    let w = host("bad");
    let soon = in_secs(600);
    let gone = in_secs(-60);
    let cases: Vec<Vec<&str>> = vec![
        vec!["set"],
        vec!["set", "--cycles"],
        vec!["set", "--cycles", "-1"],
        vec!["set", "--cycles", "1.5"],
        vec!["set", "--cycles", "x"],
        vec!["set", "--cycles", "10000"],
        vec!["set", "--cycles", "1", "--cycles", "2"],
        vec!["set", "--cycles", "1", "--plan", "maybe"],
        vec!["set", "--cycles", "1", "--for", "0"],
        vec!["set", "--cycles", "1", "--for", "5s"],
        vec!["set", "--cycles", "1", "--for", "60", "--until", &soon],
        vec!["set", "--cycles", "1", "--until", "tomorrow"],
        vec!["set", "--cycles", "1", "--until", &gone],
        vec!["set", "--cycles", "1", "--bogus"],
        vec!["set", "--cycles", "1", "extra"],
        vec!["clear", "now"],
        vec!["clear", "--json"],
        vec!["set", "--json", "--json", "--cycles", "2"],
        vec!["set", "--cycles", "2", "--json", "x"],
        vec!["--json", "extra"],
        vec!["bogus"],
        vec!["--jsn"],
    ];
    let mut last = String::new();
    for words in cases {
        let r = limit(&w, &words);
        assert_eq!(r.rc, 64, "{words:?}: {}", r.out);
        assert!(!file(&w).exists(), "{words:?} wrote the limit");
        last = r.out;
    }
    has(&last, "usage: tehas limit");
}

#[test]
fn a_limit_file_that_cannot_be_written_or_removed_is_exit_1() {
    let w = host("unwritable");
    let state = w.at().state_dir();
    fs::remove_dir_all(&state).unwrap();
    fs::write(&state, "").unwrap();
    let r = limit(&w, &["set", "--cycles", "1"]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "tehas limit: could not create");
    fs::remove_file(&state).unwrap();
    reset(&w);
    fs::create_dir_all(file(&w).join("in-the-way")).unwrap();
    let r = limit(&w, &["clear"]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "tehas limit: could not remove");
}

#[test]
fn an_expired_limit_is_read_as_expired_and_says_nothing_starts() {
    let w = host("expired");
    fs::create_dir_all(w.at().state_dir()).unwrap();
    fs::write(
        file(&w),
        format!(
            r#"{{"v":1,"cycles":2,"plan":true,"until":"{}","reason":"r"}}"#,
            in_secs(-60)
        ),
    )
    .unwrap();
    let shown = show(&w);
    assert_eq!(
        (
            &shown["state"],
            &shown["cycles"],
            &shown["plan"],
            &shown["set"]
        ),
        (
            &"expired".into(),
            &2.into(),
            &true.into(),
            &serde_json::Value::Null
        )
    );
    let r = limit(&w, &[]);
    has(&r.out, "limit     expired at ");
    has(&r.out, "nothing starts until it is set again or cleared");
}

#[test]
fn a_limit_that_cannot_be_read_is_unreadable_never_none() {
    let w = host("unreadable");
    let unreadable = r#"{"api":1,"state":"unreadable"}"#;
    for bad in [
        "not json",
        "[]",
        r#"{"v":2,"cycles":1,"plan":true,"until":null}"#,
        r#"{"v":1,"cycles":-1,"plan":true,"until":null}"#,
        r#"{"v":1,"cycles":1,"plan":"yes","until":null}"#,
        r#"{"v":1,"cycles":1,"plan":true,"until":"soon"}"#,
        r#"{"v":1,"plan":true,"until":null}"#,
        "",
    ] {
        fs::create_dir_all(w.at().state_dir()).unwrap();
        fs::write(file(&w), bad).unwrap();
        assert_eq!(limit(&w, &["--json"]).out, unreadable, "{bad:?}");
    }
    fs::remove_file(file(&w)).unwrap();
    fs::create_dir_all(file(&w)).unwrap();
    assert_eq!(limit(&w, &["--json"]).out, unreadable);
}

#[test]
fn flags_limit_read_is_what_a_reader_is_given() {
    let w = host("read");
    assert_eq!(read_limit(&w, None), r#"{"state":"none"}"#);
    let words = [
        "set",
        "--cycles",
        "2",
        "--plan",
        "off",
        "--until",
        "2036-01-01T14:05:00Z",
        "--reason",
        "r",
    ];
    assert_eq!(limit(&w, &words).rc, 0);
    assert_eq!(
        read_limit(&w, Some(epoch("2036-01-01T14:00:00Z"))),
        r#"{"state":"ok","cycles":2,"plan":false,"until":"2036-01-01T14:05:00Z","reason":"r"}"#
    );
    has(
        &read_limit(&w, Some(epoch("2036-01-01T14:05:00Z"))),
        r#""state":"expired""#,
    );
}

#[test]
fn without_an_instance_nothing_is_written_exit_78() {
    let w = host("noinstance");
    w.no_instance();
    let r = limit(&w, &["set", "--cycles", "1"]);
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "no instance.env");
    assert!(!file(&w).exists());
}
