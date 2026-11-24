//! `tehas pause` and `tehas resume` end to end: the flag they write and
//! take off, the words they refuse, the settings they stop without, and the
//! control issue resume reads through a stub gh. Who took the pause mark off
//! and what cannot be read are on the ports' fake in
//! console/ops/tests/resume_switch.rs; the flag's rules are beside its code
//! in console/ops/src/holds.rs.

mod common;

use common::*;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::PathBuf;
use tehas_core::home::State;

/// The control issue, #1: its labels and its pause events, from fixtures.
const GH: &str = r#"case "$1 $2" in
  "api repos/acme/widget/issues/1") cat "$T/labels.json" ;;
  "api repos/acme/widget/issues/1/events?per_page=100") cat "$T/events.json" ;;
  "issue edit") exit 1 ;;
esac"#;

fn host(name: &str) -> World {
    let w = World::new(&format!("pause-{name}"));
    w.gh_stub(GH);
    labels(&w, "[]");
    w
}

fn flag(w: &World) -> PathBuf {
    w.at().state(State::Pause)
}

/// The control issue as GitHub's REST read gives it, with these labels.
fn labels(w: &World, names: &str) {
    w.fx(
        "labels.json",
        &format!(
            r#"{{"title":"control","state":"open","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","html_url":"https://github.com/acme/widget/issues/1","labels":{names}}}"#
        ),
    );
}

/// One event of the pause label on the control issue, by `login` or by no one.
fn pause_event(w: &World, event: &str, login: Option<&str>) {
    let actor = login.map_or("null".to_string(), |l| format!(r#"{{"login":"{l}"}}"#));
    w.fx(
        "events.json",
        &format!(
            r#"[{{"event":"{event}","label":{{"name":"tehas:pause"}},"actor":{actor},"created_at":"2026-01-02T00:00:00Z"}}]"#
        ),
    );
}

fn pause(w: &World, words: &[&str]) -> Run {
    w.tehas("pause", words)
}

fn resume(w: &World, words: &[&str]) -> Run {
    w.tehas("resume", words)
}

fn reason(w: &World) -> String {
    let doc: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(flag(w)).unwrap()).unwrap();
    doc["reason"].as_str().unwrap().to_string()
}

#[test]
fn pause_writes_who_when_and_why_for_this_account_alone() {
    let w = host("writes");
    let r = pause(&w, &["--reason", "waiting for the gate"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "paused at ");
    has(&r.out, "waiting for the gate");
    let doc: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(flag(&w)).unwrap()).unwrap();
    let keys: Vec<&String> = doc.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["at", "by", "reason"]);
    assert_eq!(doc["reason"], "waiting for the gate");
    assert!(doc["by"].is_string());
    let at = doc["at"].as_str().unwrap();
    assert!(
        at.len() == 20 && at.as_bytes()[10] == b'T' && at.ends_with('Z'),
        "{at}"
    );
    assert_eq!(
        fs::metadata(flag(&w)).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(w.gh(), "");
}

#[test]
fn a_reason_is_empty_when_none_is_given_and_cut_to_200_characters() {
    let w = host("reason");
    assert_eq!(pause(&w, &[]).rc, 0);
    assert_eq!(reason(&w), "");
    w.reset();
    let long = "x".repeat(400);
    assert_eq!(pause(&w, &["--reason", &long]).rc, 0);
    assert_eq!(reason(&w), "x".repeat(200));
}

#[test]
fn a_second_pause_keeps_the_first_flag() {
    let w = host("again");
    pause(&w, &["--reason", "first"]);
    let r = pause(&w, &["--reason", "second"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "already paused");
    has(&r.out, "the flag is unchanged");
    assert_eq!(reason(&w), "first");
}

#[test]
fn a_flag_that_cannot_be_read_is_replaced_and_said() {
    let w = host("unreadable");
    fs::create_dir_all(flag(&w).parent().unwrap()).unwrap();
    fs::write(flag(&w), "{\"at\": 7}\n").unwrap();
    fs::set_permissions(flag(&w), fs::Permissions::from_mode(0o600)).unwrap();
    let r = pause(&w, &["--reason", "fresh"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "is not a pause flag");
    assert_eq!(reason(&w), "fresh");
}

#[test]
fn resume_removes_the_flag_and_changes_no_label() {
    let w = host("resume");
    pause(&w, &["--reason", "x"]);
    let r = resume(&w, &[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "resumed: the pause flag is gone");
    assert!(!flag(&w).exists());
    lacks(&w.gh(), "issue edit");
    let r = resume(&w, &[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "no pause flag on this host");
}

#[test]
fn resume_leaves_a_symlinked_name_alone() {
    let w = host("symlink");
    let victim = w.t().join("victim");
    let file = flag(&w);
    let temp = PathBuf::from(format!("{}.new", file.display()));
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    for name in [&file, &temp] {
        symlink(&victim, name).unwrap();
        let r = resume(&w, &[]);
        assert_eq!(r.rc, 1, "{}", r.out);
        lacks(&r.out, "the pause flag is gone");
        has(&r.out, &format!("{} is not ours to remove", name.display()));
        assert!(name.is_symlink());
        assert!(!victim.exists());
        fs::remove_file(name).unwrap();
    }
    let r = resume(&w, &[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "no pause flag on this host");
}

#[test]
fn resume_reads_the_other_switch_on_the_control_issue() {
    let w = host("other");
    labels(&w, r#"[{"name":"tehas:pause"}]"#);
    pause(&w, &["--reason", "x"]);
    let r = resume(&w, &[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "the factory stays paused: tehas:pause is still on issue #1",
    );
    assert!(!flag(&w).exists());

    labels(&w, "[]");
    pause_event(&w, "unlabeled", Some("mallory"));
    let r = resume(&w, &[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "stays paused: tehas:pause was taken off #1 by 'mallory'",
    );
    pause_event(&w, "unlabeled", Some("alice"));
    let r = resume(&w, &[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    lacks(&r.out, "stays paused");
    assert_eq!(w.gh().lines().filter(|l| l.contains("events")).count(), 2);
}

#[test]
fn resume_that_cannot_read_the_control_issue_still_removes_the_flag() {
    let w = host("unread");
    w.rm_fx("labels.json");
    pause(&w, &["--reason", "x"]);
    let r = resume(&w, &[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "the labels of issue #1 could not be read");
    assert!(!flag(&w).exists());
}

#[test]
fn stray_words_are_a_usage_error_before_the_settings_are_read() {
    let w = host("usage");
    w.no_instance();
    let r = pause(&w, &["waiting for the gate"]);
    assert_eq!(r.rc, 64, "{}", r.out);
    has(&r.out, "the reason is given as --reason TEXT");
    let r = pause(&w, &["too", "many", "arguments", "here"]);
    assert_eq!(r.rc, 64, "{}", r.out);
    has(&r.out, "usage: tehas pause [--reason TEXT]");
    assert_eq!(pause(&w, &["--reason"]).rc, 64);
    let r = resume(&w, &["7"]);
    assert_eq!(r.rc, 64, "{}", r.out);
    has(&r.out, "usage: tehas resume");
    assert!(!flag(&w).exists());
}

#[test]
fn help_prints_the_text_of_each_command_and_writes_nothing() {
    let w = host("help");
    for verb in ["pause", "resume"] {
        let r = w.tehas(verb, &["--help"]);
        assert_eq!(r.rc, 0, "{}", r.out);
        has(&r.out, &format!("Usage: tehas {verb}"));
        lacks(&r.out, "set -uo pipefail");
    }
    assert!(!flag(&w).exists());
}

#[test]
fn a_flag_that_cannot_be_written_is_exit_1_and_says_why() {
    let w = host("unwritable");
    let state = w.at().state_dir();
    fs::remove_dir_all(&state).unwrap();
    fs::write(&state, "").unwrap();
    let r = pause(&w, &["--reason", "why"]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "tehas pause: could not create");
}

#[test]
fn without_settings_nothing_is_written_or_asked() {
    let w = host("nosettings");
    w.no_instance();
    let state = w.at().state_dir();
    fs::remove_dir_all(&state).unwrap();
    let r = pause(&w, &["--reason", "why"]);
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "no instance.env");
    assert!(!state.exists());
    assert_eq!(resume(&w, &[]).rc, 78);
    assert!(!state.exists());
    assert_eq!(w.gh(), "");
}

#[test]
fn resume_without_the_labellers_stops_at_78_and_keeps_the_flag() {
    let w = host("nolabellers");
    pause(&w, &["--reason", "x"]);
    w.instance(&[
        "TEHAS_GITHUB_APP_ID=1",
        "TEHAS_REPO=acme/widget",
        "TEHAS_CONTROL_ISSUE=1",
    ]);
    let r = resume(&w, &[]);
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "TEHAS_ALLOWED_LABELERS is not set");
    assert!(flag(&w).exists());
}
