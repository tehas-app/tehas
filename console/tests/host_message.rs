//! `tehas host-message` end to end: set, list and clear through bin/tehas,
//! the file that holds the list, and the schema of list --json. The list's
//! rules are beside its code in console/ops/src/host_message.rs.

mod common;

use common::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use tehas_core::home::State;

fn host(name: &str) -> World {
    let w = World::new(&format!("host-message-{name}"));
    w.gh_stub("");
    w
}

fn hm(w: &World, words: &[&str]) -> Run {
    w.tehas("host-message", words)
}

#[test]
fn a_message_is_set_listed_and_cleared() {
    let w = host("round");
    let kept = w.at().state(State::HostMessages);
    let r = hm(
        &w,
        &[
            "set",
            "disk-low",
            "--level",
            "warn",
            "--text",
            "Disk 91% full",
        ],
    );
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        fs::metadata(&kept).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let r = hm(&w, &["list", "--json"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    let mut listed: serde_json::Value = serde_json::from_str(&r.out).unwrap();
    let file: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&kept).unwrap()).unwrap();
    assert_eq!(file, listed);
    assert_eq!(schema_errors("host-messages", &r.out), "");
    for m in listed["messages"].as_array_mut().unwrap() {
        m.as_object_mut().unwrap().remove("at");
    }
    assert_eq!(
        listed["messages"].to_string(),
        r#"[{"id":"disk-low","level":"warn","link":null,"text":"Disk 91% full"}]"#
    );
    assert_eq!(hm(&w, &["clear", "disk-low"]).rc, 0);
    assert!(!kept.exists());
    let r = hm(&w, &["list", "--json"]);
    assert_eq!((r.rc, r.out.as_str()), (0, r#"{"api":1,"messages":[]}"#));
    assert_eq!(w.gh(), "");
}

#[test]
fn a_bad_argument_is_64_and_writes_nothing() {
    let w = host("bad");
    let _ = fs::remove_dir_all(w.at().state_dir());
    let r = hm(&w, &["set", "disk-low", "--level", "loud", "--text", "x"]);
    assert_eq!(r.rc, 64, "{}", r.out);
    assert!(!w.at().state_dir().exists());
}

#[test]
fn the_sample_fits_the_schema_and_an_unknown_level_does_not() {
    let sample = fs::read_to_string(root().join("api/host-messages.sample.json")).unwrap();
    assert_eq!(schema_errors("host-messages", &sample), "");
    let mut loud: serde_json::Value = serde_json::from_str(&sample).unwrap();
    loud["messages"][0]["level"] = "loud".into();
    has(
        &schema_errors("host-messages", &loud.to_string()),
        "is not one of",
    );
}

#[test]
fn the_help_text_names_the_command() {
    let w = host("help");
    let r = w.tehas("help", &["host-message"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "Usage: tehas host-message");
}
