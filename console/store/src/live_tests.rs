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
    let _ = &at;
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
        // The host has been running since before the earliest of these times and
        // its clocks have not moved: every pair of the fixture's calls is of one
        // process, so the age rules of the publish read the beats as ever.
        clocks_of_the_fixture(&home, at);
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
    // The lock free and the clocks having moved no differently since the beat:
    // the age rule acts, and the cycle is lost from two minutes after the beat.
    drop(lock);
    publish(&home, later, &[]).unwrap();
    let v: Value = serde_json::from_str(&read(&home, "running.json")).unwrap();
    assert_eq!(v["cycles"], serde_json::json!([]), "kept={:?}", kept(&home));
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
    // The view stays the same on the next publish, which finds the clocks
    // agreeing still and writes no row: the cycle is already ended and the age
    // rule has nothing left to do with it.
    let before = read(&home, "running.json");
    clocks_of_the_fixture(&home, later + 500);
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
        serde_json::json!(["endings.json", "running.json", "clock.json"])
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
    // The publisher has to meet the bound of state/publish.lock rather than beat
    // the holder, so this thread asks for the short pair.
    short_bounds();
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
    // KEEP is what the test counts, not what it waits: the sixty-odd heartbeats
    // are sixty-odd writes, and the thread takes the short pair so a host that
    // met contention elsewhere does not push this one into the installed seconds.
    short_bounds();
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
    // The attempt died without an end; the next start replaces it, and the
    // lost end it leaves is written by the age rule of that start. The clocks
    // agree -- the host has run from the beat's moment to the start's -- so the
    // rule acts.
    clocks_of_the_fixture(&home, 2000);
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

// --- the clock step of #1043 -------------------------------------------------
//
// A step is the difference between the wall clock and the monotonic clock,
// measured between two of the store's own pairs. A test makes one by writing
// the row of the pair the host took before it was saved: the wall reading has
// moved and the monotonic one has not, which is what a save and a wake leave.
// The monotonic reading is the process's own, so the tests name it rather than
// read what the test binary's process owes.

/// The row of the pair a host's call left: the wall clock said `wall` when the
/// store's own clock read `mono`, and the pair was taken by the call that
/// judged by `taken_at`. The verbs write this row beside every view they
/// publish, so a test that means a host saved and woken, or one that has simply
/// been running, names the row here rather than waiting on a call to leave it.
fn note_pair(home: &Path, wall: i64, mono: i64, taken_at: i64) {
    conn(home)
        .execute(
            "INSERT OR REPLACE INTO clock_sample (id, wall, mono, taken_at) VALUES (1, ?1, ?2, ?3)",
            rusqlite::params![wall, mono, taken_at],
        )
        .unwrap();
}

/// The pair of the row the store holds, if it holds one.
fn kept(home: &Path) -> Option<crate::clock::Sample> {
    crate::clock::kept(&conn(home), 0)
}

/// The row of a host saved at the wall time `saved` and woken since, `mono`
/// seconds into the store's run: the pair the host left when it was saved, so its
/// reading of the store's clock stands still while the wall clock has run. That is
/// the step. `mono` is named when the test means a pair of an earlier run than any
/// call of the test could have taken.
fn saved_home(home: &Path, saved: i64, mono: i64) {
    note_pair(home, saved, mono, saved);
}

/// A cycle of `issue` started at 1000 and last heard from at 1010, its lock
/// free: a fixture for the clock file, where the view of the cycle is not the
/// subject.
fn running_free(home: &Path, issue: i64) {
    let (lock, _) = start_by(home, issue, "alice", "label", 1000);
    beat(home, 1010, issue, true, "running", "build", "");
    drop(lock);
}

/// A running cycle of issue `issue`, started at 1000 and last heard from at
/// 1010, with its lock held by the returned file.
fn running(home: &Path, issue: i64) -> File {
    let (lock, _) = start_by(home, issue, "alice", "label", 1000);
    beat(home, 1010, issue, true, "running", "build", "");
    lock
}

/// The row of the pair the call at the wall time `now` is to find on record, for
/// a host that has simply been running since the beat's wall time `since`.
///
/// A step is found between the readings of the store's own clock, and the store
/// reads that clock from the instant its program began -- which, for a test, is the
/// instant the test binary began, a moment no test can name and no row can quote.
/// So a test does not name the store's reading; it names the two `now`s the host's
/// clock was given, and the row says what the store's clock read when it saw the
/// pair before this one, which is the moment of the beat's call and is `clock_of`
/// less the seconds the host has run. The two clocks of such a host advance
/// together, and the next call finds no step.
fn agreed_pair(home: &Path, now: i64, since: i64) {
    // The pair the call at `now` is to find: the wall time it will be measured
    // from, `since`, and the reading of the store's clock `now - since` short of
    // what the call at `now` itself reads. The two pairs then advance together --
    // which is what a host that has only been running does -- and the step the
    // call finds is none. `began` is the test's reading of the store's clock, the
    // one clock the calls read, so a test can name a reading of it and no other.
    let got = crate::clock::pair(since, began() - (now - since));
    note_pair(home, got.wall, got.mono, got.taken_at);
}

/// What the store's clock read for the beat of the fixture: the seconds of this
/// process at the moment the first call of a test was made, which the beat's own
/// call recorded and from which every later pair of a host that has simply been
/// running is counted. The verbs read the store's clock themselves, so a test can
/// only give a pair the reading of the clock the calls use: this one.
fn began() -> i64 {
    *BEAT.get_or_init(crate::clock::mono)
}

static BEAT: std::sync::OnceLock<i64> = std::sync::OnceLock::new();

/// What the store's own clock reads at this instant: the whole seconds since the
/// test process began, which is where the clock of every pair of the test's calls
/// began. It is the one clock whose two readings mean anything, and the verbs read
/// it themselves when they take a pair, so a test names nothing else.
fn clock_of() -> i64 {
    crate::clock::mono()
}

/// The monotonic reading of the host that was saved: a moment of the store's run
/// before any call of the test, which is what a pair kept across a save is.
const SAVED_MONO: i64 = 100;

/// The pair of the host a test's calls are made against: the beat's pair, as the
/// beat left it, and the pair of the call at `now` as that call will take it. Both
/// belong to this process, which is the only clock whose two readings mean
/// anything, and both have advanced together, so a call at `now` finds no step and
/// its age rules act.
///
/// A test says so whenever it means a host that has simply been running, because
/// the verbs keep the pair themselves and the row the last of them left is what the
/// next reads: a fixture whose calls span hours of the host's wall clock would
/// otherwise be read as a host whose wall clock ran ahead of the store's by those
/// hours, which is the step of a restore and holds the age rules off.
fn clocks_of_the_fixture(home: &Path, now: i64) {
    agreed_pair(home, now, 1010);
}

/// The row of a host whose age rules are held off: a step was seen `ago` of the
/// store's own seconds ago and the clocks have agreed since, so a call at `now`
/// finds nothing afresh and the hold of that step is what decides whether its
/// rules act. `ago` short of the heartbeat interval holds them off; the interval or
/// more lets them act.
fn held(home: &Path, now: i64, ago: i64) {
    // The clocks agreed when the step was seen, `ago` of the store's own seconds
    // before the call at `now`, and have not moved since: the pair the call finds,
    // and the instant the hold counts from.
    let got = crate::clock::pair(now, began() - ago);
    note_pair(home, got.wall, got.mono, got.taken_at);
}

#[test]
fn publish_writes_the_clock_entry_with_the_views_it_publishes() {
    let home = home("publish-clock");
    // Nothing heard from and no lock held: the cycle is lost as ever, and this
    // test is about the clock file alone.
    running_free(&home, 60);
    // The beat's pair, read from the row the beat left. The publish's pair is
    // taken from it: the same reading of the store's clock, and 90 s later on the
    // wall clock, which is the step of a host that has been running -- the wall
    // clock having advanced while the store's clock stood still between the two
    // calls, as it does when the calls are quick. The test names the pair so the
    // step it asserts is one it chose and not one the run of the test leaves.
    let heard = kept(&home).expect("the beat kept no pair");
    note_pair(&home, heard.wall, heard.mono, heard.taken_at);
    let v = publish(&home, 1100, &[]).unwrap();
    assert!(
        v["written"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n == "clock.json"),
        "{v}"
    );
    let entry: Value = serde_json::from_str(&read(&home, "clock.json")).unwrap();
    assert_eq!(entry["at"], 1100, "the newest pair is the one publish read");
    assert_eq!(entry["wall"], 1100);
    // The wall reading of the pair is the verb's own `now`, so a reader can
    // tell a pair of this minute from one of last night; the monotonic reading
    // is the store process's own.
    assert!(entry["mono"].is_number());
    // The store held no pair when this call read its own, so there is nothing to
    // compare the clocks with.
    assert_eq!(
        entry["step"],
        1100 - heard.wall,
        "the step a host nobody saved takes"
    );
    assert_eq!(entry["seen"], heard.mono, "measured from the beat's pair");
    // The pair this call left is on record for the next call to measure
    // against: a minute later on both clocks, so the next entry says nothing.
    assert_eq!(kept(&home).map(|p| p.wall), Some(1100));
    // A minute later on the wall clock, and a minute later on the store's own:
    // the two advance together, so the entry of this publish names no step.
    clocks_of_the_fixture(&home, 1160);
    publish(&home, 1160, &[]).unwrap();
    let again: Value = serde_json::from_str(&read(&home, "clock.json")).unwrap();
    assert_eq!(again["at"], 1160);
    assert_eq!(again["step"], Value::Null, "{again}");
    assert_eq!(again["seen"], Value::Null, "{again}");
    // The host saved an hour before a call is another matter: the wall clock
    // has moved 3700 s since the pair on record while the monotonic clock did
    // not move at all, and the entry names the wall clock's own advance and the
    // reading the step was measured from.
    let up = clock_of();
    saved_home(&home, 4860 - 3700, up);
    publish(&home, 4860, &[]).unwrap();
    let stepped: Value = serde_json::from_str(&read(&home, "clock.json")).unwrap();
    assert_eq!(stepped["at"], 4860);
    assert_eq!(stepped["step"], 3700, "{stepped}");
    assert_eq!(stepped["seen"], up, "the pair of the saved host");
    // The pair of the call that saw the step is on record now, so the next call
    // measures the two clocks against it and they agree: a host that has been
    // running since the wake has no step to report.
    clocks_of_the_fixture(&home, 4920);
    publish(&home, 4920, &[]).unwrap();
    let after: Value = serde_json::from_str(&read(&home, "clock.json")).unwrap();
    assert_eq!(after["at"], 4920);
    assert_eq!(after["step"], Value::Null, "{after}");
    assert_eq!(after["seen"], Value::Null, "{after}");
    // Nothing hidden is left behind by the new file.
    assert!(left(&home, "").iter().all(|n| !n.starts_with('.')));
}

#[test]
fn a_step_of_an_hour_keeps_a_running_cycle_running() {
    // The host was saved at the wall time 1100 and woken an hour later: the
    // heartbeat of the cycle that is alive and holds its lock says 1010, an age
    // of nearly two hours on the stepped clock.
    let home = home("clock-step-running");
    let _lock = running(&home, 61);
    saved_home(&home, 1100, SAVED_MONO);
    publish(&home, 4700, &[]).unwrap();
    let got: Value = serde_json::from_str(&read(&home, "running.json")).unwrap();
    assert_eq!(got["cycles"][0]["issue"], 61, "the step made it lost");
    assert_eq!(got["last"], Value::Null, "the step made an end");
    let e: Value = serde_json::from_str(&read(&home, "endings.json")).unwrap();
    assert_eq!(e, serde_json::json!([]), "the step wrote an end");
}

#[test]
fn a_cycle_whose_lock_is_free_is_still_lost_after_the_step() {
    let home = home("clock-step-lost");
    let lock = running(&home, 62);
    saved_home(&home, 1100, SAVED_MONO);
    // Held: running, however big the step.
    publish(&home, 4700, &[]).unwrap();
    let got: Value = serde_json::from_str(&read(&home, "running.json")).unwrap();
    assert_eq!(got["cycles"][0]["issue"], 62);
    // The lock free and the step held off: still running. The rule waits one
    // heartbeat interval rather than acting on an age the step made. The pair of
    // the step is on record, and this call is inside the interval the hold
    // lasts: it is the hold and nothing else that keeps the cycle on the page.
    drop(lock);
    held(&home, 4700 + LOST_AFTER - 10, LOST_AFTER - 10);
    publish(&home, 4700 + LOST_AFTER - 10, &[]).unwrap();
    let got: Value = serde_json::from_str(&read(&home, "running.json")).unwrap();
    assert_eq!(
        got["cycles"][0]["issue"], 62,
        "lost inside the interval the step holds the rule off"
    );
    // Past the interval the rule acts again, and a cycle whose lock is free is
    // lost: the step costs the rule an interval, not the cycle its place on the
    // page. The step of the pair on record was seen at the interval's edge, so
    // this call is past it and the rule acts again.
    held(&home, 4700 + 2 * LOST_AFTER, LOST_AFTER);
    publish(&home, 4700 + 2 * LOST_AFTER, &[]).unwrap();
    let got: Value = serde_json::from_str(&read(&home, "running.json")).unwrap();
    assert_eq!(got["cycles"], serde_json::json!([]), "still shown running");
    assert_eq!(got["last"]["status"], "lost");
    assert_eq!(got["last"]["updatedAt"], 1010 + LOST_AFTER);
}

#[test]
fn a_step_holds_the_lost_end_of_a_start_off() {
    // The attempt that never ended is kept among the ends as lost by the start
    // that replaces it: an age rule, held off while a step is.
    let home = home("clock-step-start");
    let (lock, attempt) = start_by(&home, 63, "alice", "label", 1000);
    beat(&home, 1010, 63, true, "running", "build", "");
    // The host was saved at 1100 and woken an hour later; the start that
    // replaces the unended attempt runs after the wake.
    saved_home(&home, 1100, SAVED_MONO);
    drop(lock);
    approve(&home, 63, "alice", "label");
    let _lock = hold(&home, 63);
    // The view is read after the step, and holds: no end is written for the
    // attempt that never ended.
    held(&home, 4700, LOST_AFTER - 10);
    ok_at(&home, 4700, &["cycle-start", "63"]);
    publish(&home, 4700, &[]).unwrap();
    let e: Value = serde_json::from_str(&read(&home, "endings.json")).unwrap();
    assert_eq!(e, serde_json::json!([]), "a lost end on a held step");
    // The attempt moved on: the start itself is not the rule that waits.
    assert_eq!(
        one::<i64>(&home, "SELECT attempt FROM cycles WHERE issue = 63"),
        attempt + 1
    );
    // The same start once the step has aged out writes the end as it always
    // did: the rule waits for the interval, it is not turned off. The pair of
    // the start above is on record, so the clocks agree again.
    let (lock, _) = start_by(&home, 64, "alice", "label", 4700);
    beat(&home, 4710, 64, true, "running", "build", "");
    drop(lock);
    approve(&home, 64, "alice", "label");
    let _lock = hold(&home, 64);
    // Past the interval the rule acts again.
    held(&home, 4700 + 3 * LOST_AFTER, LOST_AFTER);
    ok_at(&home, 4700 + 3 * LOST_AFTER, &["cycle-start", "64"]);
    publish(&home, 4700 + 3 * LOST_AFTER, &[]).unwrap();
    let e: Value = serde_json::from_str(&read(&home, "endings.json")).unwrap();
    assert_eq!(
        e,
        serde_json::json!([{"issue": 64, "status": "lost", "step": "build",
                            "startedAt": 4700, "updatedAt": 4710 + LOST_AFTER}])
    );
}
