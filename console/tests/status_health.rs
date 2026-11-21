//! The checks of tehasbot health that read the host's files: the limit tehas
//! limit sets, the fault of a sandbox that cannot push, the disk of the
//! previews, the slice the last poll reported, the holds it names, the image
//! cycles run, the cycles that stopped writing, the backup, the cost of the
//! polls and the test features.

mod common;

use common::*;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::Command;

/// tehasbot status, then tehas status with `words`, joined by `sep`.
fn then_summary(w: &World, sep: &str, words: &str) -> Run {
    w.sh(&format!(
        "{} {sep} {} {words}",
        w.status_cmd(),
        w.summary_cmd()
    ))
}

fn and_summary(w: &World) -> Run {
    then_summary(w, "&&", "")
}

fn health_file(w: &World) -> Value {
    w.live_json("health.json")
}

fn health_status(w: &World) -> String {
    health_file(w)["status"].as_str().unwrap_or("").to_string()
}

fn health_output(w: &World) -> String {
    health_file(w)["output"].as_str().unwrap_or("").to_string()
}

/// A JSON file of live/, changed in place.
fn edit_live(w: &World, name: &str, f: impl FnOnce(&mut Value)) {
    let mut v = w.live_json(name);
    f(&mut v);
    fs::write(w.live_file(name), v.to_string()).unwrap();
}

/// How many files named health.down are under `dir`.
fn downs(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| {
            let p = e.path();
            if p.is_dir() {
                downs(&p)
            } else {
                usize::from(e.file_name() == "health.down")
            }
        })
        .sum()
}

fn my_uid() -> String {
    let out = Command::new("id").arg("-u").output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// The limit tehas limit sets, as health reads it from state/limit.json.
fn set_limit(w: &World, text: &str) {
    w.home_file("state/limit.json", &format!("{text}\n"));
}

/// The fault of a sandbox that cannot push, as the start whose push check was
/// refused leaves it (console/cycle/src/prepare.rs).
fn set_push_fault(w: &World, text: &str) {
    w.home_file("state/push-fault.json", &format!("{text}\n"));
}

/// live/previews.json with its disk figures, written `age` seconds ago.
fn disk(w: &World, low: bool, age: i64) {
    let v = json!({
        "generatedAt": tehas_core::time::iso(tehas_core::time::now() - age),
        "disk": {"freeBytes": 1073741824u64, "minFreeBytes": 5368709120u64, "low": low},
        "previews": [],
    });
    w.home_file("live/previews.json", &format!("{v}\n"));
}

/// What the dispatcher's last poll wrote about the slice, `age` seconds ago.
/// 4 GiB of 20 GiB are promised.
fn poll(w: &World, why: &str, hold: &str, age: i64) {
    let v = json!({
        "time": tehas_core::time::iso(tehas_core::time::now() - age),
        "why": why, "hold": hold, "count": 1, "room": 8,
        "committed": 4294967296u64, "max": 21474836480u64, "reserve": "2g",
    });
    w.home_file("live/dispatch.json", &format!("{v}\n"));
}

#[test]
fn limit_a_limit_is_reported_in_the_lines_and_in_json_its_reason_only_in_the_lines() {
    let w = World::new("limit");
    set_limit(
        &w,
        r#"{"v":1,"cycles":2,"plan":false,"until":"2036-01-01T00:00:00Z","reason":"held for alpha"}"#,
    );
    let r = and_summary(&w);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "limit     2 cycles at once, planning off, until 2036-01-01T00:00:00Z: held for alpha",
    );
    assert_eq!(health_status(&w), "pass");
    let r = w.summary(&["--json"]);
    assert_eq!(
        json_of(&r)["limit"],
        json!({"state":"ok","cycles":2,"plan":false,"until":"2036-01-01T00:00:00Z"})
    );
}

#[test]
fn limit_no_limit_prints_no_line_and_json_says_none() {
    let w = World::new("no-limit");
    let r = and_summary(&w);
    lacks(&r.out, "limit ");
    let r = w.summary(&["--json"]);
    assert_eq!(json_of(&r)["limit"], json!({"state":"none"}));
}

#[test]
fn limit_expired_or_unreadable_is_a_warning_that_says_nothing_starts() {
    let w = World::new("limit-expired");
    set_limit(
        &w,
        r#"{"v":1,"cycles":2,"plan":true,"until":"2020-01-01T00:00:00Z","reason":""}"#,
    );
    let r = then_summary(&w, "&&", "--json");
    assert_eq!(r.rc, 0, "{}", r.out);
    let v = json_of(&r);
    assert_eq!(
        json!([v["state"], v["limit"]["state"]]),
        json!(["warn", "expired"])
    );
    has(
        v["reasons"][0].as_str().unwrap_or(""),
        "Nothing starts: the limit expired at 2020-01-01T00:00:00Z",
    );
    w.reset();
    set_limit(&w, "not a limit");
    let r = then_summary(&w, "&&", "--json");
    let v = json_of(&r);
    assert_eq!(
        json!([v["state"], v["limit"]]),
        json!(["warn", {"state":"unreadable"}])
    );
    has(
        v["reasons"][0].as_str().unwrap_or(""),
        "Nothing starts: the limit cannot be read",
    );
}

#[test]
fn push_a_sandbox_that_cannot_push_is_a_fail_in_the_lines_the_file_and_json() {
    let w = World::new("push");
    set_push_fault(
        &w,
        r#"{"at":"2026-10-17T13:00:00Z","login":"tehas-bot","line":"remote: Permission to acme/widget denied to tehas-bot"}"#,
    );
    let r = and_summary(&w);
    has(
        &r.out,
        "push      the sandbox cannot push: remote: Permission to acme/widget denied to tehas-bot (tehas-bot)",
    );
    assert_eq!(health_status(&w), "fail");
    has(
        &health_output(&w),
        "The sandbox cannot push: remote: Permission to acme/widget denied to tehas-bot (tehas-bot)",
    );
    assert_eq!(
        health_file(&w)["checks"]["pushFault"]["line"],
        "remote: Permission to acme/widget denied to tehas-bot"
    );
    let r = then_summary(&w, "&&", "--json");
    let v = json_of(&r);
    assert_eq!(
        json!([v["state"], v["pushFault"]["login"]]),
        json!(["fail", "tehas-bot"])
    );

    w.reset();
    set_push_fault(
        &w,
        r#"{"at":"2026-10-17T13:00:00Z","login":"tehas-bot","line":"denied"}"#,
    );
    w.status();
    assert_eq!(health_status(&w), "fail");
    // The fault off again, and the host passes as it did before.
    fs::remove_file(w.home.join("state/push-fault.json")).unwrap();
    let r = w.status();
    assert_eq!(health_status(&w), "pass");
    lacks(&r.out, "the sandbox cannot push");
}

#[test]
fn push_a_fault_that_says_nothing_still_fails_with_a_line_of_the_hosts_own() {
    let w = World::new("push-nothing");
    set_push_fault(&w, "not json");
    let r = and_summary(&w);
    assert_eq!(health_status(&w), "fail");
    has(
        &r.out,
        "push      the sandbox cannot push: the sandbox cannot push to origin",
    );
    w.reset();
    set_push_fault(&w, r#"{"at":"2026-10-17T13:00:00Z","login":"","line":""}"#);
    w.status();
    has(
        &health_output(&w),
        "The sandbox cannot push: the sandbox cannot push to origin.",
    );
}

#[test]
fn push_a_sandbox_that_cannot_push_is_said_before_a_slice_hold_or_a_limit() {
    let w = World::new("push-first");
    set_push_fault(
        &w,
        r#"{"at":"2026-10-17T13:00:00Z","login":"tehas-bot","line":"denied"}"#,
    );
    set_limit(
        &w,
        r#"{"v":1,"cycles":2,"plan":true,"until":"2020-01-01T00:00:00Z","reason":""}"#,
    );
    w.status();
    has(
        &health_output(&w),
        "The sandbox cannot push: denied (tehas-bot)",
    );
}

#[test]
fn push_control_characters_of_the_faults_line_cannot_reach_the_terminal() {
    let w = World::new("push-control");
    // The escape and the override as the JSON escapes of them: a raw control
    // character is not valid in a JSON string.
    set_push_fault(
        &w,
        r#"{"at":"2026-10-17T13:00:00Z","login":"tehas-bot","line":"denied\u001b[31mred\u202e"}"#,
    );
    let r = w.sh(&format!("{} && {}", w.health_cmd(), w.summary_cmd()));
    has(
        &r.out,
        "push      the sandbox cannot push: denied [31mred (tehas-bot)",
    );
    assert_eq!(
        r.out
            .lines()
            .filter(|l| l.contains("push      the sandbox cannot push"))
            .count(),
        1,
        "{}",
        r.out
    );
}

#[test]
fn limit_the_room_under_it_from_the_last_poll() {
    let w = World::new("limit-room");
    w.held(
        "limit",
        "holding: the limit allows 1 cycle at once and 1 is running",
        json!(1),
        json!(2),
    );
    edit_live(&w, "dispatch.json", |v| v["limitRoom"] = json!(0));
    let r = then_summary(&w, "&&", "--json");
    let v = json_of(&r);
    assert_eq!(
        v["room"],
        json!({"cycles":1,"maxAwaiting":2,"slice":3,"limit":0})
    );
    assert_eq!(
        v["hold"],
        json!({"why":"limit","text":"holding: the limit allows 1 cycle at once and 1 is running"})
    );
}

#[test]
fn health_a_host_with_no_previews_has_no_disk_check() {
    let w = World::new("no-previews");
    w.status();
    assert!(health_file(&w)["checks"]["disk"].is_null());
}

fn disk_figures(w: &World) -> Value {
    let d = &health_file(w)["checks"]["disk"];
    json!({"freeBytes": d["freeBytes"], "minFreeBytes": d["minFreeBytes"], "low": d["low"]})
}

#[test]
fn health_low_disk_is_a_warning_with_the_figures() {
    let w = World::new("low-disk");
    disk(&w, true, 60);
    w.status();
    assert_eq!(health_status(&w), "warn");
    has(
        &health_output(&w),
        "low on disk: previews are not being built",
    );
    assert_eq!(
        disk_figures(&w),
        json!({"freeBytes":1073741824u64,"minFreeBytes":5368709120u64,"low":true})
    );
    assert_eq!(downs(&w.live), 0);
}

#[test]
fn health_free_space_that_could_not_be_read_is_a_warning_that_says_so() {
    let w = World::new("free-unread");
    disk(&w, true, 60);
    edit_live(&w, "previews.json", |v| {
        v["disk"]["freeBytes"] = Value::Null
    });
    w.status();
    assert_eq!(health_status(&w), "warn");
    assert_eq!(
        health_output(&w),
        "The host's free disk space could not be read: previews are not being built."
    );
    assert_eq!(
        disk_figures(&w),
        json!({"freeBytes":null,"minFreeBytes":5368709120u64,"low":true})
    );
}

#[test]
fn health_a_figure_with_low_disk_keeps_the_sentence_about_low_disk() {
    let w = World::new("figure-low");
    disk(&w, true, 60);
    w.status();
    assert_eq!(
        health_output(&w),
        "The host is low on disk: previews are not being built."
    );
}

#[test]
fn health_no_figure_and_not_low_is_not_a_warning() {
    let w = World::new("no-figure");
    disk(&w, false, 60);
    edit_live(&w, "previews.json", |v| {
        v["disk"]["freeBytes"] = Value::Null
    });
    w.status();
    assert_eq!(health_status(&w), "pass");
}

#[test]
fn health_enough_disk_passes() {
    let w = World::new("enough-disk");
    disk(&w, false, 60);
    w.status();
    assert_eq!(health_status(&w), "pass");
}

#[test]
fn health_a_figure_older_than_half_an_hour_is_not_believed() {
    let w = World::new("old-figure");
    disk(&w, true, 3600);
    w.status();
    assert_eq!(health_status(&w), "pass");
    assert!(health_file(&w)["checks"]["disk"].is_null());
}

#[test]
fn health_a_previews_json_of_bad_gives_no_disk_check_and_no_warning() {
    let w = World::new("bad-previews");
    for bad in [
        "not json",
        r#"{"generatedAt":"yesterday","disk":{"low":true}}"#,
        r#"{"disk":{"low":true}}"#,
        "[]",
        r#"{"generatedAt":7,"disk":"full"}"#,
    ] {
        w.reset();
        w.home_file("live/previews.json", &format!("{bad}\n"));
        w.status();
        assert_eq!(health_status(&w), "pass", "{bad}");
        assert!(health_file(&w)["checks"]["disk"].is_null(), "{bad}");
        lacks(&health_output(&w), "disk");
        assert_eq!(downs(&w.live), 0, "{bad}");
    }
}

#[test]
fn health_a_failed_cycle_still_wins_over_low_disk() {
    let w = World::new("failed-wins");
    disk(&w, true, 60);
    w.cycle(false, "died", "verify");
    w.status();
    assert_eq!(health_status(&w), "fail");
}

#[test]
fn health_carries_checks_slice_from_live_dispatch_json() {
    let w = World::new("slice");
    poll(&w, "", "", 40);
    w.status();
    assert_eq!(health_status(&w), "pass");
    let mut slice = health_file(&w)["checks"]["slice"].clone();
    let time = slice.as_object_mut().unwrap().remove("time").unwrap();
    assert_eq!(
        slice,
        json!({"why":"","hold":"","count":1,"room":8,"committed":4294967296u64,"max":21474836480u64,"reserve":"2g"})
    );
    assert_eq!(time, w.live_json("dispatch.json")["time"]);
}

#[test]
fn health_a_hold_for_why_is_a_warning_with_the_hold_as_output() {
    let w = World::new("hold-why");
    let hold = "holding: no slice file on this host (sudo ./bootstrap.sh slice)";
    for why in ["slice", "unreadable"] {
        w.reset();
        poll(&w, why, hold, 40);
        let r = then_summary(&w, ";", "");
        assert_eq!(r.rc, 0, "{why}: {}", r.out);
        assert_eq!(health_status(&w), "warn", "{why}");
        assert_eq!(health_output(&w), hold);
        assert_eq!(downs(&w.live), 0);
        has(&r.out, &format!("factory   warn: {hold}"));
        has(&r.out, &format!("slice     {hold} (4"));
    }
}

// The shell's case of this name held the two of the loop below it.
#[test]
fn health_a_hold_on_the_disk_is_a_warning_with_the_hold_as_output() {
    let w = World::new("hold-disk");
    // A full disk, and a disk whose figure cannot be read, both hold every
    // start, so /health warns and tehas status carries the line the poll
    // logged.
    for hold in [
        "holding: 1.0 GiB free on the disk, under TEHAS_CYCLE_MIN_FREE=10g: no cycle starts",
        "holding: the free disk space could not be read: no cycle starts",
    ] {
        w.reset();
        poll(&w, "disk", hold, 40);
        let r = then_summary(&w, ";", "");
        assert_eq!(r.rc, 0, "{hold}: {}", r.out);
        assert_eq!(health_status(&w), "warn", "{hold}");
        assert_eq!(health_output(&w), hold);
        assert_eq!(health_file(&w)["checks"]["slice"]["why"], "disk");
        assert_eq!(downs(&w.live), 0);
        has(&r.out, &format!("factory   warn: {hold}"));
        has(&r.out, &format!("slice     {hold} (4"));
    }
}

#[test]
fn health_a_hold_for_why_is_not_a_warning_and_tehas_status_still_says_it() {
    let w = World::new("hold-pass");
    for why in ["count", "awaiting", "pressure", "room"] {
        w.reset();
        poll(&w, why, "holding: the reason", 40);
        let r = and_summary(&w);
        assert_eq!(r.rc, 0, "{why}: {}", r.out);
        assert_eq!(health_status(&w), "pass", "{why}");
        has(&r.out, "factory   pass: Idle, taking work.");
        has(&r.out, "slice     holding: the reason (4");
    }
}

#[test]
fn health_no_resources_is_a_warning_said_by_the_factory_line_and_the_slice_keeps_its_numbers() {
    let w = World::new("no-resources");
    poll(
        &w,
        "resources",
        "holding: no resources: the model endpoint does not answer",
        40,
    );
    let r = then_summary(&w, ";", "");
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(health_status(&w), "warn");
    assert_eq!(
        health_output(&w),
        "No resources: the model endpoint does not answer."
    );
    assert_eq!(health_file(&w)["checks"]["slice"]["why"], "resources");
    assert!(!w.live_file("health.down").exists());
    assert_eq!(w.live_json("status.json")["noResources"], true);
    has(
        &r.out,
        "factory   warn: No resources: the model endpoint does not answer.",
    );
    has(&r.out, "slice     room for 8 cycles");
}

#[test]
fn status_no_resources_is_not_said_from_a_poll_older_than_ten_minutes_or_from_any_other_hold() {
    let w = World::new("no-resources-old");
    poll(
        &w,
        "resources",
        "holding: no resources: the model endpoint does not answer",
        700,
    );
    w.status();
    assert_eq!(w.live_json("status.json")["noResources"], false);
    assert_eq!(health_status(&w), "pass");
    poll(
        &w,
        "room",
        "holding: tehas.slice has no room for another cycle",
        40,
    );
    w.status();
    assert_eq!(w.live_json("status.json")["noResources"], false);
    fs::remove_file(w.live_file("dispatch.json")).unwrap();
    w.status();
    assert_eq!(w.live_json("status.json")["noResources"], false);
}

#[test]
fn health_a_paused_factory_is_said_before_no_resources() {
    let w = World::new("paused-resources");
    poll(
        &w,
        "resources",
        "holding: no resources: the model endpoint does not answer",
        40,
    );
    w.fx("paused", "true\n");
    w.status();
    assert_eq!(health_output(&w), "The factory is paused.");
}

#[test]
fn health_a_paused_factory_and_a_failed_cycle_are_said_before_a_slice_hold() {
    let w = World::new("paused-slice");
    poll(
        &w,
        "slice",
        "holding: no slice file on this host (sudo ./bootstrap.sh slice)",
        40,
    );
    w.fx("paused", "true\n");
    w.status();
    assert_eq!(health_output(&w), "The factory is paused.");
    w.fx("paused", "false\n");
    w.cycle(false, "aborted", "develop");
    w.status();
    assert_eq!(health_status(&w), "fail");
}

#[test]
fn health_a_dispatch_json_older_than_ten_minutes_gives_no_slice_check_and_no_warning() {
    let w = World::new("dispatch-old");
    poll(
        &w,
        "slice",
        "holding: no slice file on this host (sudo ./bootstrap.sh slice)",
        660,
    );
    let r = and_summary(&w);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert!(health_file(&w)["checks"]["slice"].is_null());
    assert_eq!(health_status(&w), "pass");
    has(&r.out, "slice     unknown: no poll has reported in 10 min");
}

#[test]
fn health_a_dispatch_json_of_bad_does_not_break_the_health_write() {
    let w = World::new("dispatch-bad");
    for bad in [
        r#"{"time":"#,
        "not json",
        "[]",
        r#"{"why":"slice","hold":"x"}"#,
        r#"{"time":"yesterday","why":"slice","hold":"x"}"#,
        r#"{"time":7,"why":"slice"}"#,
        "{} {}",
    ] {
        w.reset();
        w.home_file("live/dispatch.json", &format!("{bad}\n"));
        let r = and_summary(&w);
        assert_eq!(r.rc, 0, "{bad}: {}", r.out);
        assert_eq!(health_status(&w), "pass", "{bad}");
        assert!(health_file(&w)["checks"]["slice"].is_null(), "{bad}");
        has(&r.out, "slice     unknown");
    }
}

#[test]
fn summary_the_slice_line_says_the_room_what_is_promised_the_reserve_and_its_age() {
    let w = World::new("slice-line");
    // No checkout on this host, so the platform line stays out and the slice
    // line is the last of the four.
    let _ = fs::remove_dir_all(w.home.join("platform"));
    poll(&w, "", "", 40);
    let r = and_summary(&w);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(r.out.lines().count(), 4, "{}", r.out);
    has(
        &r.out,
        "slice     room for 8 cycles (4 GiB of 20 GiB promised, reserve 2g, 4",
    );
    has(r.out.lines().last().unwrap_or(""), " s ago)");
    poll(&w, "", "", 300);
    edit_live(&w, "dispatch.json", |v| {
        v["committed"] = json!(4831838208u64)
    });
    let r = and_summary(&w);
    has(
        &r.out,
        "slice     room for 8 cycles (4.5 GiB of 20 GiB promised, reserve 2g, 5 min ago)",
    );
}

#[test]
fn summary_with_a_refresh_lane_the_slice_line_says_how_much_of_it_is_free() {
    let w = World::new("refresh-lane");
    let _ = fs::remove_dir_all(w.home.join("platform"));
    poll(&w, "", "", 40);
    edit_live(&w, "dispatch.json", |v| v["refreshRoom"] = json!(1));
    let r = and_summary(&w);
    has(
        &r.out,
        "slice     room for 8 cycles (4 GiB of 20 GiB promised, reserve 2g, refresh lane 1 free, 4",
    );
    assert_eq!(health_file(&w)["checks"]["slice"]["refreshRoom"], 1);
}

#[test]
fn summary_a_health_file_written_before_the_slice_was_known_says_unknown() {
    let w = World::new("slice-unknown");
    w.status();
    edit_live(&w, "health.json", |v| {
        v["checks"].as_object_mut().unwrap().remove("slice");
    });
    let r = w.summary(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "slice     unknown: no poll has reported in 10 min");
}

#[test]
fn health_and_summary_read_files_only_and_ask_systemd_only_for_the_failed_units() {
    let w = World::new("files-only");
    w.stub("systemctl", "");
    w.stub("docker", "");
    poll(&w, "room", "holding: the reason", 40);
    and_summary(&w);
    assert_eq!(
        w.calls("systemctl").trim_end(),
        "--user --failed --plain --no-legend tehas-*"
    );
    // Only status asks docker, for the labels of the image cycles run
    // (console/ops/src/rukas.rs).
    assert_eq!(
        w.calls("docker").trim_end(),
        format!(
            "image inspect --format {{{{.Id}}}} {{{{index .Config.Labels \"tehas.rukas\"}}}} {{{{index .Config.Labels \"tehas.layer\"}}}} tehas/pi-rukas:uid{}",
            my_uid()
        )
    );
    fs::remove_file(w.t().join("calls/docker")).unwrap();
    w.sh(&format!("{} && {}", w.health_cmd(), w.summary_cmd()));
    assert_eq!(w.calls("docker"), "");
}

/// A docker whose image has `inspect` for its id and labels and runs
/// pi-rukas `version`.
fn docker(w: &World, inspect: &str, version: &str) {
    w.stub(
        "docker",
        &format!(
            r#"case "$1 $2" in
  'image inspect') echo '{inspect}' ;;
  'run --pull=never') echo '{{"version":"{version}"}}' ;;
esac"#
        ),
    );
}

fn rukas_fields(w: &World, keys: &[&str]) -> Value {
    let v = w.live_json("rukas.json");
    Value::Object(keys.iter().map(|k| (k.to_string(), v[k].clone())).collect())
}

#[test]
fn status_writes_the_pi_rukas_the_image_runs_and_health_warns_while_it_is_behind() {
    let w = World::new("rukas-behind");
    let (a, b) = ("a".repeat(40), "b".repeat(40));
    docker(&w, &format!("sha256:1 {a}"), "0.14.53");
    w.set_env(&[&format!("PI_RUKAS_FOLLOW=commit:{b}")]);
    let r = w.status();
    w.set_env(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        rukas_fields(&w, &["follow", "target", "image", "imageVersion", "behind"]),
        json!({"follow": format!("commit:{b}"), "target": b, "image": a, "imageVersion": "v0.14.53", "behind": true})
    );
    assert_eq!(
        health_file(&w)["checks"]["rukas"]["imageVersion"],
        "v0.14.53"
    );
    assert_eq!(health_status(&w), "warn");
    assert_eq!(
        health_output(&w),
        "The sandbox image runs pi-rukas v0.14.53 and the host follows bbbbbbbb: run tehas update."
    );
    has(
        &w.calls("docker"),
        &format!(
            "run --pull=never --rm --init --network none tehas/pi-rukas:uid{} /opt/pi-rukas/bin/pi-rukas-headless --capabilities",
            my_uid()
        ),
    );
}

#[test]
fn status_compares_the_images_instance_layer_with_sandbox_dockerfile_and_health_warns_while_it_moved(
) {
    let w = World::new("rukas-layer");
    let b = "b".repeat(40);
    let inst = w.t().join("instance");
    fs::write(inst.join("sandbox.Dockerfile"), "FROM x\nCOPY tools /t\n").unwrap();
    fs::create_dir_all(inst.join("tools")).unwrap();
    fs::write(inst.join("tools/pin"), "1\n").unwrap();
    docker(&w, &format!("sha256:1 {b} sha256:old"), "0.14.54");
    w.set_env(&[&format!("PI_RUKAS_FOLLOW=commit:{b}")]);
    let r = w.status();
    assert_eq!(r.rc, 0, "{}", r.out);
    let sum = w.live_json("rukas.json")["instanceLayer"]
        .as_str()
        .unwrap_or("")
        .to_string();
    assert_eq!(
        rukas_fields(&w, &["behind", "layer", "layerMoved"]),
        json!({"behind":false,"layer":"sha256:old","layerMoved":true})
    );
    let hex = sum.strip_prefix("sha256:").unwrap_or("");
    assert!(
        hex.len() == 64
            && hex
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
        "{sum}"
    );
    let rk = &health_file(&w)["checks"]["rukas"];
    assert_eq!(
        json!([
            rk["layer"],
            rk["instanceLayer"] == sum.as_str(),
            rk["layerMoved"]
        ]),
        json!(["sha256:old", true, true])
    );
    assert_eq!(health_status(&w), "warn");
    assert_eq!(
        health_output(&w),
        "The sandbox image was built from another instance layer than sandbox.Dockerfile is now: run tehas update."
    );
    // The image built from that layer is current.
    docker(&w, &format!("sha256:1 {b} {sum}"), "0.14.54");
    w.status();
    assert_eq!(health_file(&w)["checks"]["rukas"]["layerMoved"], false);
    assert_eq!(health_status(&w), "pass");
    // A file the layer copies changed since the build.
    fs::write(inst.join("tools/pin"), "2\n").unwrap();
    w.status();
    assert_eq!(health_file(&w)["checks"]["rukas"]["layerMoved"], true);
}

#[test]
fn an_image_built_from_a_release_newer_than_the_target_cached_is_current_and_one_older_is_behind() {
    let w = World::new("rukas-release");
    let (b, c) = ("b".repeat(40), "c".repeat(40));
    // The target was asked a minute ago, at v0.14.54; v0.14.55 came out since
    // and the image was built from it.
    w.home_file(
        "state/rukas-target",
        &format!(
            "{{\"at\":{},\"follow\":\"release:0\",\"repo\":\"https://github.com/up/pi-rukas.git\",\"commit\":\"{b}\",\"what\":\"release v0.14.54\"}}\n",
            tehas_core::time::now() - 60
        ),
    );
    w.stub(
        "git",
        &format!(
            r#"case "$1" in
  ls-remote) printf '%s\trefs/tags/v0.14.54\n%s\trefs/tags/v0.14.55\n' {b} {c} ;;
  *) command -p git "$@" ;;
esac"#
        ),
    );
    docker(&w, &format!("sha256:2 {c}"), "0.14.55");
    w.set_env(&[
        "PI_RUKAS_FOLLOW=release:0",
        "PI_RUKAS_REPO=https://github.com/up/pi-rukas.git",
    ]);
    let r = w.status();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        rukas_fields(&w, &["target", "targetVersion", "imageVersion", "behind"]),
        json!({"target": c, "targetVersion":"v0.14.55","imageVersion":"v0.14.55","behind":false})
    );
    assert_eq!(health_file(&w)["checks"]["rukas"]["behind"], false);
    // An image a release older than the target is still behind it.
    docker(&w, &format!("sha256:1 {b}"), "0.14.54");
    w.status();
    assert_eq!(
        rukas_fields(&w, &["targetVersion", "imageVersion", "behind"]),
        json!({"targetVersion":"v0.14.55","imageVersion":"v0.14.54","behind":true})
    );
    assert_eq!(health_file(&w)["checks"]["rukas"]["behind"], true);
}

#[test]
fn a_lost_cycle_ends_two_minutes_after_its_last_heartbeat() {
    let w = World::new("lost");
    w.one(9, true, "running", "develop", Some(900), Some(300));
    w.one(12, false, "merged", "review", Some(600), Some(200));
    let r = then_summary(&w, ";", "");
    assert_eq!(r.rc, 3, "{}", r.out);
    has(&r.out, "The last cycle failed: #9 ended as lost.");
    let last = &w.live_json("running.json")["last"];
    assert_eq!(json!([last["issue"], last["status"]]), json!([9, "lost"]));
    let mut names: Vec<String> = fs::read_dir(w.live_file("cycles"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".json"))
        .collect();
    names.sort();
    assert_eq!(names, ["9.json"]);
}

#[test]
fn health_warns_when_the_state_store_has_no_backup_or_only_one_more_than_two_days_old() {
    let w = World::new("backup");
    w.status();
    assert_eq!(health_status(&w), "pass");
    let age = health_file(&w)["checks"]["backup"]["ageSeconds"].as_f64();
    assert!(age.is_some_and(|a| a < 60.0), "{age:?}");
    let backup = w.home.join("state/backup");
    fs::remove_dir_all(&backup).unwrap();
    w.health();
    assert_eq!(health_status(&w), "warn");
    assert_eq!(
        health_output(&w),
        "There is no backup of the state store: tehasbot clean makes one a day."
    );
    assert!(health_file(&w)["checks"]["backup"].is_null());
    w.store_ok(&["backup"]);
    let then = std::time::SystemTime::now() - std::time::Duration::from_secs(72 * 3600);
    for e in fs::read_dir(&backup).unwrap().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with("tehas-") && name.ends_with(".db") {
            fs::File::options()
                .write(true)
                .open(e.path())
                .unwrap()
                .set_modified(then)
                .unwrap();
        }
    }
    w.health();
    assert_eq!(health_status(&w), "warn");
    assert_eq!(
        health_output(&w),
        "The newest backup of the state store is 72 h old."
    );
}

/// One poll of the hour on record, whose wall time is the second it names,
/// asked of the store's own clock. No `writes` is no write log at all, which
/// leaves the figure off the answer; 0 is a log of none.
fn poll_record(w: &World, cpu: i64, wall: i64, writes: Option<u32>, starts: u32) {
    let now = tehas_core::time::now();
    let log = w.t().join("poll.writes");
    let _ = fs::remove_file(&log);
    let path = match writes {
        Some(n) => {
            let at = tehas_core::time::iso(now);
            fs::write(&log, format!("{at}\tissue edit\t\n").repeat(n as usize)).unwrap();
            log
        }
        None => w.t().join("no-such.log"),
    };
    let r = w.store_at(
        now,
        &[
            "poll-record",
            "--started",
            &(now - wall).to_string(),
            "--cpu-ms",
            &(cpu * 1000).to_string(),
            "--gh",
            "0",
            "--writes-log",
            &path.to_string_lossy(),
            "--starts",
            &starts.to_string(),
        ],
    );
    assert_eq!(r.rc, 0, "no poll recorded: {}", r.out);
}

fn poll_cost(w: &World) -> Value {
    health_file(w)["checks"]["poll_cost"].clone()
}

#[test]
fn an_idle_poll_within_the_budget_is_no_warning_and_the_check_is_on_record() {
    let w = World::new("poll-within");
    poll_record(&w, 3, 20, None, 0);
    w.status();
    assert_eq!(health_status(&w), "pass");
    assert_eq!(
        poll_cost(&w),
        json!({"over":[],"budget":"cpu=5,wall=30,writes=0"})
    );
}

#[test]
fn an_idle_poll_over_its_cpu_budget_warns_and_names_only_that_figure() {
    let w = World::new("poll-cpu");
    poll_record(&w, 6, 20, None, 0);
    w.status();
    assert_eq!(health_status(&w), "warn");
    assert_eq!(poll_cost(&w)["over"], json!(["cpu"]));
    assert_eq!(
        health_output(&w),
        "An idle poll is over its budget on cpu (TEHAS_POLL_BUDGET=cpu=5,wall=30,writes=0): of the last hour's 1 idle poll, cpu had a median of 6 s against 5 s and 1 over it."
    );
}

#[test]
fn an_idle_poll_over_all_three_budgets_names_them_in_the_order_of_the_spec() {
    let w = World::new("poll-three");
    poll_record(&w, 6, 40, Some(2), 0);
    w.status();
    assert_eq!(health_status(&w), "warn");
    assert_eq!(poll_cost(&w)["over"], json!(["cpu", "wall", "writes"]));
}

#[test]
fn a_poll_of_a_write_log_that_holds_none_passes_the_writes_of_the_default_budget() {
    let w = World::new("poll-none");
    poll_record(&w, 3, 20, Some(0), 0);
    w.status();
    assert_eq!(health_status(&w), "pass");
    assert_eq!(poll_cost(&w)["over"], json!([]));
}

#[test]
fn one_write_is_over_the_writes_the_default_budget_allows_which_are_none() {
    let w = World::new("poll-one-write");
    poll_record(&w, 3, 20, Some(1), 0);
    w.status();
    assert_eq!(health_status(&w), "warn");
    assert_eq!(poll_cost(&w)["over"], json!(["writes"]));
}

// The answer of the verb, for those arguments only, gives an over figure so
// the case does not lean on the store's arithmetic.
#[test]
fn the_budget_of_the_instance_is_what_the_store_is_asked_for_and_the_health_file_says_it() {
    let w = World::new("poll-budget");
    w.store_answers(
        "poll-idle --since 3600 --budget cpu=10,wall=10",
        0,
        r#"{"api":1,"ok":true,"verb":"poll-idle","polls":1,"cpu_ms":6000,"wall_ms":20000,"writes":0,"over":["cpu"]}"#,
        0,
    );
    let dir = w.t().join("budget-instance");
    w.set_env(&[&format!("TEHAS_INSTANCE_DIR={}", dir.display())]);
    let mut lines = MINIMAL.to_vec();
    lines.push("TEHAS_POLL_BUDGET=cpu=10,wall=10");
    w.fx(
        "budget-instance/instance.env",
        &format!("{}\n", lines.join("\n")),
    );
    poll_record(&w, 3, 20, None, 0);
    w.status();
    assert_eq!(health_status(&w), "warn");
    assert_eq!(
        poll_cost(&w),
        json!({"over":["cpu"],"budget":"cpu=10,wall=10"})
    );
    assert_eq!(w.store_answered(), 1);
}

#[test]
fn a_poll_that_started_a_cycle_is_not_an_idle_poll_and_costs_no_warning() {
    let w = World::new("poll-started");
    poll_record(&w, 60, 60, Some(3), 1);
    w.status();
    assert_eq!(health_status(&w), "pass");
    assert_eq!(poll_cost(&w)["over"], json!([]));
    assert_eq!(poll_cost(&w)["budget"], "cpu=5,wall=30,writes=0");
}

#[test]
fn the_stores_refusal_is_no_warning_and_the_health_file_says_there_was_no_check() {
    let w = World::new("poll-refusal");
    poll_record(&w, 6, 60, None, 0);
    w.store_answers("poll-idle", 1, "store: busy: the store is held", 0);
    w.status();
    assert_eq!(health_status(&w), "pass");
    assert!(poll_cost(&w).is_null());
}

// Only a build with debug assertions, the tests', and a lab build read the
// variable; a lab build warns with it unset too (console/ops/src/health.rs).
#[test]
fn test_features_the_variable_set_is_a_warning_a_line_and_a_field_and_unset_is_none() {
    let w = World::new("test-features");
    w.set_env(&["TEHAS_TEST_FEATURES=outsource nothing"]);
    let r = and_summary(&w);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "features  a lab build: TEHAS_TEST_FEATURES names outsource, still being built",
    );
    let h = health_file(&w);
    let want = json!({"read":true,"features":["outsource"],"lab":false});
    assert_eq!(
        json!([h["status"], h["checks"]["testFeatures"]]),
        json!(["warn", want])
    );
    let r = w.summary(&["--json"]);
    assert_eq!(json_of(&r)["testFeatures"], want);
    w.set_env(&[]);
    let r = then_summary(&w, "&&", "--json");
    let v = json_of(&r);
    assert_eq!(
        json!([v["state"], v["testFeatures"]]),
        json!(["pass", null])
    );
}
