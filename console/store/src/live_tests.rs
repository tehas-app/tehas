//! cycle-heartbeat and publish.

use crate::live::{text, KEEP, LOST_AFTER};
use crate::verbs_tests::*;
use serde_json::Value;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/live")
}

fn beat(home: &Path, at: i64, issue: i64, running: bool, status: &str, step: &str, text: &str) {
    let n = issue.to_string();
    let mut args = vec![
        "cycle-heartbeat",
        &n,
        if running { "--running" } else { "--ended" },
        "--status",
        status,
        "--step",
        step,
    ];
    if !text.is_empty() {
        args.extend(["--text", text]);
    }
    ok_at(home, at, &args);
}

fn end_at(home: &Path, at: i64, issue: i64, attempt: i64, status: &str, step: Option<&str>) {
    let (n, a) = (issue.to_string(), attempt.to_string());
    let mut args = vec!["cycle-end", &n, "--attempt", &a, "--status", status];
    if let Some(s) = step {
        args.extend(["--step", s]);
    }
    ok_at(home, at, &args);
}

fn read(home: &Path, name: &str) -> String {
    fs::read_to_string(home.join("live").join(name)).unwrap()
}

fn left(home: &Path, dir: &str) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(home.join("live").join(dir))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

fn publish(home: &Path, at: i64, extra: &[&str]) -> Result<Value, (u8, String)> {
    let mut args = vec!["publish"];
    args.extend(extra);
    call_at(home, at, &args)
}

/// The rows behind the fixtures (tests/fixtures/live/README): the cycle files
/// tehas_live_assemble read, made as heartbeats and ends.
fn fixture_home() -> (PathBuf, Vec<File>) {
    let home = home("publish-fixture");
    let mut locks = Vec::new();
    for (issue, started, status, step, text, ended) in [
        (3, 4_102_400_000, "timeout", "build", "", 4_102_410_000),
        (
            9,
            4_102_420_000,
            "handoff",
            "review",
            "Asked a person",
            4_102_438_000,
        ),
        (
            5,
            4_102_430_000,
            "merged",
            "handoff",
            "Opened #40",
            4_102_439_000,
        ),
    ] {
        let (lock, attempt) = start_by(&home, issue, "alice", "label", started);
        beat(&home, started + 10, issue, true, "running", step, text);
        drop(lock);
        end_at(&home, ended, issue, attempt, status, Some(step));
    }
    for (issue, started, step, text, at) in [
        (
            12,
            4_102_440_000,
            "build",
            "Running the gate",
            4_102_444_800,
        ),
        (7, 4_102_441_000, "review", "", 4_102_444_790),
    ] {
        let (lock, _) = start_by(&home, issue, "alice", "label", started);
        beat(&home, at, issue, true, "running", step, text);
        locks.push(lock);
    }
    (home, locks)
}

#[test]
fn publish_writes_the_views_tehas_live_assemble_wrote() {
    let (home, _locks) = fixture_home();
    let cycles = home.join("live/cycles");
    fs::create_dir_all(&cycles).unwrap();
    // A view of an earlier cycle, and a file that is no cycle's.
    fs::write(cycles.join("44.json"), "{}\n").unwrap();
    fs::write(cycles.join("notes.txt"), "kept\n").unwrap();
    let v = publish(&home, 4_102_444_800, &[]).unwrap();
    let f = fixtures();
    for name in ["running.json", "endings.json"] {
        assert_eq!(
            read(&home, name),
            fs::read_to_string(f.join(name)).unwrap(),
            "{name}"
        );
    }
    for n in ["12", "7", "5"] {
        let name = format!("cycles/{n}.json");
        assert_eq!(
            read(&home, &name),
            fs::read_to_string(f.join(&name)).unwrap(),
            "{name}"
        );
    }
    let kept: Vec<String> = fs::read_to_string(f.join("kept.txt"))
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    let mut want = kept.clone();
    want.push("notes.txt".to_string());
    want.sort();
    assert_eq!(left(&home, "cycles"), want);
    let mut removed: Vec<i64> = v["removed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_i64().unwrap())
        .collect();
    removed.sort();
    assert_eq!(removed, vec![44]);
    // Nothing hidden is left behind.
    assert!(left(&home, "").iter().all(|n| !n.starts_with('.')));
    assert!(left(&home, "cycles").iter().all(|n| !n.starts_with('.')));
}

#[test]
fn a_quiet_cycle_whose_lock_is_free_is_lost() {
    let home = home("publish-lost");
    let (lock, _) = start_by(&home, 20, "alice", "label", 1000);
    beat(&home, 1010, 20, true, "running", "build", "Building");
    // Held: still running, however quiet.
    let later = 1010 + LOST_AFTER + 60;
    publish(&home, later, &[]).unwrap();
    let v: Value = serde_json::from_str(&read(&home, "running.json")).unwrap();
    assert_eq!(v["cycles"][0]["issue"], 20);
    assert_eq!(v["last"], Value::Null);
    drop(lock);
    publish(&home, later, &[]).unwrap();
    let v: Value = serde_json::from_str(&read(&home, "running.json")).unwrap();
    assert_eq!(v["cycles"], serde_json::json!([]));
    assert_eq!(v["last"]["status"], "lost");
    assert_eq!(v["last"]["running"], false);
    assert_eq!(v["last"]["updatedAt"], 1010 + LOST_AFTER);
    assert_eq!(v["last"]["statusText"], "Building");
    let e: Value = serde_json::from_str(&read(&home, "endings.json")).unwrap();
    assert_eq!(
        e,
        serde_json::json!([{"issue": 20, "status": "lost", "step": "build",
                            "startedAt": 1000, "updatedAt": 1010 + LOST_AFTER}])
    );
    // The view stays the same on the next publish: publish writes no row.
    let before = read(&home, "running.json");
    publish(&home, later + 500, &[]).unwrap();
    assert_eq!(read(&home, "running.json"), before);
    assert_eq!(count(&home, "endings"), 0);
}

#[test]
fn a_fresh_running_cycle_with_a_free_lock_is_running() {
    let home = home("publish-fresh");
    let (lock, _) = start_by(&home, 21, "alice", "label", 1000);
    drop(lock);
    beat(&home, 1010, 21, true, "running", "build", "");
    publish(&home, 1010 + LOST_AFTER, &[]).unwrap();
    let v: Value = serde_json::from_str(&read(&home, "running.json")).unwrap();
    assert_eq!(v["cycles"][0]["issue"], 21);
}

#[test]
fn an_empty_store_publishes_empty_views() {
    let home = home("publish-empty");
    let v = publish(&home, NOW, &[]).unwrap();
    assert_eq!(
        read(&home, "running.json"),
        "{\"cycles\":[],\"last\":null}\n"
    );
    assert_eq!(read(&home, "endings.json"), "[]\n");
    assert_eq!(
        v["written"],
        serde_json::json!(["endings.json", "running.json"])
    );
    assert_eq!(v["down"], Value::Null);
    assert!(!home.join("live/health.json").exists());
}

#[test]
fn health_down_goes_with_the_report_that_failed() {
    let home = home("publish-health");
    let files = scratch("publish-health-files");
    let fail = write_file(&files, "fail.json", b"{\n  \"status\": \"fail\"\n}\n");
    let fine = write_file(&files, "ok.json", b"{\"status\":\"ok\"}\n");
    let dispatch = write_file(&files, "dispatch.json", b"{\"why\":\"idle\"}\n");
    let v = publish(
        &home,
        NOW,
        &[
            "--health",
            fail.to_str().unwrap(),
            "--dispatch",
            dispatch.to_str().unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(v["down"], true);
    assert_eq!(read(&home, "health.json"), "{\n  \"status\": \"fail\"\n}\n");
    assert_eq!(read(&home, "dispatch.json"), "{\"why\":\"idle\"}\n");
    assert!(home.join("live/health.down").exists());
    let v = publish(&home, NOW, &["--health", fine.to_str().unwrap()]).unwrap();
    assert_eq!(v["down"], false);
    assert_eq!(read(&home, "health.json"), "{\"status\":\"ok\"}\n");
    assert!(!home.join("live/health.down").exists());
    // The dispatch report is left as it was.
    assert_eq!(read(&home, "dispatch.json"), "{\"why\":\"idle\"}\n");
}

#[test]
fn a_report_that_is_not_an_object_writes_nothing() {
    let home = home("publish-bad");
    let files = scratch("publish-bad-files");
    for (name, body) in [
        ("list.json", &b"[1]"[..]),
        ("broken.json", b"{\"status\":"),
        ("nostatus.json", b"{\"why\":\"x\"}"),
    ] {
        let p = write_file(&files, name, body);
        let (code, line) = publish(&home, NOW, &["--health", p.to_str().unwrap()]).unwrap_err();
        assert_eq!(code, 1, "{name}");
        assert!(line.starts_with("unreadable --health"), "{name}: {line}");
    }
    let p = write_file(&files, "list2.json", b"[]");
    let (code, _) = publish(&home, NOW, &["--dispatch", p.to_str().unwrap()]).unwrap_err();
    assert_eq!(code, 1);
    assert!(!home.join("live/running.json").exists());
}

#[test]
fn a_held_publish_lock_is_busy() {
    let home = home("publish-busy");
    let f = File::create(home.join("state/publish.lock")).unwrap();
    f.lock().unwrap();
    let (code, line) = publish(&home, NOW, &[]).unwrap_err();
    assert_eq!(code, 1);
    assert!(line.starts_with("busy"), "{line}");
    assert!(!home.join("live/running.json").exists());
}

#[test]
fn a_rename_that_fails_fails_the_call_and_leaves_no_temp_file() {
    let home = home("publish-rename");
    fs::create_dir_all(home.join("live/running.json/x")).unwrap();
    let (code, line) = publish(&home, NOW, &[]).unwrap_err();
    assert_eq!(code, 1);
    assert!(line.starts_with("write live/running.json"), "{line}");
    assert!(left(&home, "").iter().all(|n| !n.starts_with('.')));
}

// --- cycle-heartbeat --------------------------------------------------------------

#[test]
fn a_heartbeat_is_refused_for_no_cycle_an_ended_one_or_another_attempt() {
    let home = home("beat-refused");
    let args = ["cycle-heartbeat", "30", "--running", "--status", "running"];
    assert_eq!(refused(&home, &args).0, 1);
    assert_eq!(refused(&home, &args).1, "nocycle");
    let (lock, attempt) = start(&home, 30);
    let other = (attempt + 1).to_string();
    let mut wrong = args.to_vec();
    wrong.extend(["--attempt", &other]);
    assert_eq!(refused(&home, &wrong).1, "attempt");
    let right = attempt.to_string();
    let mut given = args.to_vec();
    given.extend(["--attempt", &right]);
    assert_eq!(ok(&home, &given)["at"], NOW);
    drop(lock);
    end(&home, 30, attempt, "merged");
    assert_eq!(refused(&home, &args).1, "ended");
}

#[test]
fn a_heartbeat_takes_one_of_running_and_ended() {
    for args in [
        &["cycle-heartbeat", "1", "--status", "running"][..],
        &[
            "cycle-heartbeat",
            "1",
            "--running",
            "--ended",
            "--status",
            "x",
        ],
    ] {
        assert!(usage(args).contains("--running"), "{args:?}");
    }
    assert!(usage(&["cycle-heartbeat", "1", "--running"]).contains("--status"));
}

#[test]
fn heartbeats_keep_the_newest_of_an_issue() {
    let home = home("beat-keep");
    let (_lock, _) = start_by(&home, 31, "alice", "label", 1000);
    for i in 0..(KEEP + 5) {
        beat(&home, 1001 + i, 31, true, "running", "build", "");
    }
    assert_eq!(count(&home, "heartbeats WHERE issue = 31"), KEEP);
    assert_eq!(
        one::<i64>(&home, "SELECT min(at) FROM heartbeats WHERE issue = 31"),
        1006
    );
}

#[test]
fn the_notice_loses_control_characters_and_is_cut() {
    assert_eq!(text("a\u{1b}[31mb\nc"), Some("a[31mbc".to_string()));
    assert_eq!(text("\n\t"), None);
    assert_eq!(text(&"ä".repeat(400)).unwrap().chars().count(), 300);
}

#[test]
fn cycle_show_says_when_the_cycle_was_last_heard_from() {
    let home = home("beat-heard");
    let (_lock, _) = start_by(&home, 32, "alice", "label", 1000);
    assert_eq!(
        ok(&home, &["cycle-show", "32"])["cycle"]["heard_at"],
        Value::Null
    );
    beat(&home, 1010, 32, true, "running", "build", "");
    beat(&home, 1020, 32, true, "running", "build", "");
    assert_eq!(ok(&home, &["cycle-show", "32"])["cycle"]["heard_at"], 1020);
    let running = ok(&home, &["cycle-show", "--running"]);
    assert_eq!(running["cycles"][0]["cycle"]["heard_at"], 1020);
}

#[test]
fn a_heartbeat_of_an_earlier_attempt_is_not_heard_from() {
    let home = home("beat-earlier");
    let (lock, attempt) = start_by(&home, 33, "alice", "label", 1000);
    beat(&home, 1010, 33, true, "running", "build", "");
    drop(lock);
    end_at(&home, 1020, 33, attempt, "failed", None);
    let (_lock, _) = start_by(&home, 33, "alice", "label", 2000);
    assert_eq!(
        ok(&home, &["cycle-show", "33"])["cycle"]["heard_at"],
        Value::Null
    );
}

#[test]
fn cycle_end_keeps_the_last_notice_and_step() {
    let home = home("beat-end");
    let (lock, attempt) = start_by(&home, 34, "alice", "label", 1000);
    beat(
        &home,
        1010,
        34,
        true,
        "running",
        "review",
        "Asked for a review",
    );
    drop(lock);
    end_at(&home, 1020, 34, attempt, "handoff", None);
    publish(&home, 1020, &[]).unwrap();
    let v: Value = serde_json::from_str(&read(&home, "running.json")).unwrap();
    assert_eq!(v["last"]["status"], "handoff");
    assert_eq!(v["last"]["step"], "review");
    assert_eq!(v["last"]["statusText"], "Asked for a review");
    assert_eq!(v["last"]["updatedAt"], 1020);
    let e: Value = serde_json::from_str(&read(&home, "endings.json")).unwrap();
    assert_eq!(e[0]["step"], Value::Null);
}

#[test]
fn a_restarted_issue_does_not_show_its_earlier_attempt() {
    let home = home("publish-restart");
    let (lock, _) = start_by(&home, 40, "alice", "label", 1000);
    beat(&home, 1010, 40, true, "running", "build", "Old notice");
    drop(lock);
    // The attempt died without an end; the next start replaces it.
    let (_lock, attempt) = start_by(&home, 40, "alice", "label", 2000);
    publish(&home, 2000, &[]).unwrap();
    let v: Value = serde_json::from_str(&read(&home, "running.json")).unwrap();
    assert_eq!(v, serde_json::json!({"cycles": [], "last": null}));
    // Its end is kept as lost, from when it was last heard from.
    let e: Value = serde_json::from_str(&read(&home, "endings.json")).unwrap();
    assert_eq!(
        e,
        serde_json::json!([{"issue": 40, "status": "lost", "step": "build",
                            "startedAt": 1000, "updatedAt": 1010 + LOST_AFTER}])
    );
    // An end with no heartbeat of its own carries nothing of the earlier one.
    end_at(&home, 2010, 40, attempt, "failed", None);
    publish(&home, 2010, &[]).unwrap();
    let v: Value = serde_json::from_str(&read(&home, "running.json")).unwrap();
    assert_eq!(v["last"]["step"], "");
    assert_eq!(v["last"]["statusText"], "");
}

#[test]
fn ends_of_the_same_second_stay_in_the_order_they_ended() {
    let home = home("publish-ties");
    for issue in [51, 50] {
        let (lock, attempt) = start_by(&home, issue, "alice", "label", 1000);
        drop(lock);
        end_at(&home, 1100, issue, attempt, "merged", None);
    }
    publish(&home, 1100, &[]).unwrap();
    let e: Value = serde_json::from_str(&read(&home, "endings.json")).unwrap();
    assert_eq!(e[0]["issue"], 51);
    assert_eq!(e[1]["issue"], 50);
}
