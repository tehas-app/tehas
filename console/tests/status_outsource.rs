//! What outsource adds to the factory's state: tehasbot status writes
//! live/contractors.json with the feature on and takes it away with it off,
//! and tehas status and its --json show the contractors only while the file
//! is there. A test build turns the feature on (TEHAS_TEST_FEATURES).

mod common;

use common::*;
use serde_json::json;

fn key(digit: u8) -> String {
    format!("{:064}", digit)
}

/// The lines of `text` that start with `prefix` and end with `suffix`.
fn lines_like(text: &str, prefix: &str, suffix: &str) -> usize {
    text.lines()
        .filter(|l| l.starts_with(prefix) && l.ends_with(suffix))
        .count()
}

/// 2026-11-21T13:59:40Z: ^[0-9-]{10}T[0-9:]{8}Z$.
fn stamp_shaped(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 20
        && b[..10].iter().all(|c| c.is_ascii_digit() || *c == b'-')
        && b[10] == b'T'
        && b[11..19].iter().all(|c| c.is_ascii_digit() || *c == b':')
        && b[19] == b'Z'
}

/// Two contractors, box1 working on #12 and box2 idle, #13 built in-house,
/// the feature on, and one status run.
fn on(w: &World) -> Run {
    w.no_db();
    w.store_ok(&[
        "init",
        "--outsource",
        "--version",
        "v0.30.0",
        "--commit",
        "cb6912a",
        "--instance",
        "example",
        "--repo",
        "acme/widget",
    ]);
    let (k1, k2) = (key(7), key(8));
    w.store_ok(&["contractor-add", "--name", "box1", "--key", &k1]);
    w.store_ok(&["contractor-add", "--name", "box2", "--key", &k2]);
    w.store_ok(&["contractor-seen", "--name", "box1"]);
    w.store_ok(&["contractor-seen", "--name", "box2"]);
    w.cycle(true, "running", "develop");
    w.one(13, true, "running", "review", None, None);
    w.store_ok(&["job-offer", "--issue", "12", "--contractor", "box1"]);
    w.minimal(&["TEHAS_OUTSOURCE=on"]);
    w.set_env(&["TEHAS_TEST_FEATURES=outsource"]);
    w.status()
}

#[test]
fn off_status_writes_no_contractors_file_and_takes_a_stray_one_away() {
    let w = World::new("off");
    let r = w.status();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert!(!w.live_file("contractors.json").exists());
    w.fx("home/tehas/live/contractors.json", r#"{"contractors":[]}"#);
    let r = w.status();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert!(!w.live_file("contractors.json").exists());
    let r = w.summary(&[]);
    assert_eq!(count_lines(&r.out, "outsource:"), 0, "{}", r.out);
    let r = w.summary(&["--json"]);
    assert!(json_of(&r).get("contractors").is_none(), "{}", r.out);
}

#[test]
fn on_in_a_test_build_status_writes_the_contractors_with_no_key() {
    let w = World::new("on");
    let r = on(&w);
    assert_eq!(r.rc, 0, "{}", r.out);
    let c = w.live_json("contractors.json");
    let list: Vec<_> = c["contractors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| json!([x["name"], x["status"], x["issue"]]))
        .collect();
    assert_eq!(
        json!([c["api"], list]),
        json!([1, [["box1", "working", 12], ["box2", "idle", null]]])
    );
    let n = &c["counts"];
    assert_eq!(
        json!([n["working"], n["idle"], n["outsourced"]]),
        json!([1, 1, 1])
    );
    let text = w.read_home("live/contractors.json");
    lacks(&text, &key(7));
    assert!(
        stamp_shaped(c["contractors"][0]["lastSeenAt"].as_str().unwrap_or("")),
        "{c}"
    );
}

#[test]
fn on_tehas_status_says_where_each_running_cycle_is_built_and_prints_the_outsource_line() {
    let w = World::new("on-summary");
    assert_eq!(on(&w).rc, 0);
    let r = w.summary(&[]);
    has(&r.out, "cycle     #12 running, step develop");
    assert_eq!(
        lines_like(&r.out, "cycle     #12", ", outsourced to box1"),
        1,
        "{}",
        r.out
    );
    assert_eq!(
        lines_like(&r.out, "cycle     #13", ", in-house"),
        1,
        "{}",
        r.out
    );
    has(
        &r.out,
        "outsource: 1 idle, 1 working, 0 resting; 1 cycle(s) outsourced",
    );
}

#[test]
fn on_tehas_status_json_has_contractors_and_running_still_holds_the_outsourced_cycle() {
    let w = World::new("on-json");
    assert_eq!(on(&w).rc, 0);
    let r = w.summary(&["--json"]);
    let v = json_of(&r);
    let c = &v["contractors"];
    let names: Vec<_> = c["list"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["name"].clone())
        .collect();
    assert_eq!(
        json!([c["working"], c["idle"], c["outsourced"], names]),
        json!([1, 1, 1, ["box1", "box2"]])
    );
    let mut running: Vec<u64> = v["running"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["issue"].as_u64().unwrap())
        .collect();
    running.sort();
    assert_eq!(running, [12, 13]);
    assert_eq!(schema_errors("status", &r.out), "");
}

#[test]
fn switched_off_again_the_next_status_takes_the_file_away_and_the_readers_say_nothing_of_it() {
    let w = World::new("off-again");
    assert_eq!(on(&w).rc, 0);
    w.minimal(&["TEHAS_OUTSOURCE=off"]);
    let r = w.status();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert!(!w.live_file("contractors.json").exists());
    let r = w.summary(&[]);
    // The variable is still set, so the lines that name it say outsource; no
    // other does.
    let said = r
        .out
        .lines()
        .filter(|l| !l.contains("TEHAS_TEST_FEATURES"))
        .filter(|l| l.contains("outsource") || l.contains("in-house"))
        .count();
    assert_eq!(said, 0, "{}", r.out);
}
