//! What tehas status prints of the health file tehasbot status wrote: the
//! lines of the summary, the object of --json, the holds on a factory and
//! tehas:all-hands on the control issue.

mod common;

use common::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use tehas_core::time::{iso, now};

/// `$status && $summary <words>`, the two outputs as one.
fn status_then(w: &World, words: &[&str]) -> Run {
    let s = w.status();
    if s.rc != 0 {
        return s;
    }
    let r = w.summary(words);
    let out = [s.out, r.out]
        .into_iter()
        .filter(|o| !o.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    Run { rc: r.rc, out }
}

/// A JSON file of live/, changed in place.
fn edit(w: &World, name: &str, f: impl FnOnce(&mut Value)) {
    let mut v = w.live_json(name);
    assert!(v.is_object(), "no {name}");
    f(&mut v);
    fs::write(w.live_file(name), v.to_string()).unwrap();
}

fn health_time(w: &World, time: Value) {
    edit(w, "health.json", |v| v["time"] = time);
}

/// What the dispatcher's last poll wrote of the demand: `waiting`, the oldest
/// accepted `oldest` seconds ago, and the file written `age` seconds ago.
fn demand(w: &World, waiting: u64, oldest: i64, age: i64) {
    fs::create_dir_all(&w.live).unwrap();
    let mut v = json!({"time": iso(now() - age), "waiting": waiting});
    if waiting > 0 {
        v["oldest"] = json!(iso(now() - oldest));
    }
    fs::write(w.live_file("demand.json"), v.to_string()).unwrap();
}

/// The files of live/ and what they hold, as `cksum ./*` tells them apart.
fn sums(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(dir)
        .unwrap()
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| {
            (
                e.file_name().to_string_lossy().into_owned(),
                fs::read(e.path()).unwrap(),
            )
        })
        .collect()
}

/// The types status --json promises, for every object it prints.
#[track_caller]
fn shape(out: &str) {
    let v: Value = serde_json::from_str(out).unwrap_or_else(|e| panic!("{e}:\n{out}"));
    let null_or = |x: &Value, f: &dyn Fn(&Value) -> bool| x.is_null() || f(x);
    let checks = [
        ("api", v["api"] == 1),
        ("time", v["time"].is_string()),
        ("age", v["age"].is_number()),
        ("stale", v["stale"].is_boolean()),
        (
            "state",
            ["pass", "warn", "fail"].contains(&v["state"].as_str().unwrap_or("")),
        ),
        (
            "reasons",
            v["reasons"]
                .as_array()
                .is_some_and(|a| a.iter().all(Value::is_string)),
        ),
        (
            "paused",
            v["paused"]["local"].is_boolean() && v["paused"]["label"].is_boolean(),
        ),
        ("allHands", v["allHands"].is_boolean()),
        ("limit", v["limit"]["state"].is_string()),
        (
            "running",
            v["running"].as_array().is_some_and(|a| {
                a.iter()
                    .all(|x| x["issue"].is_number() && x["since"].is_string())
            }),
        ),
        (
            "planning",
            v["planning"].as_array().is_some_and(|a| {
                a.iter()
                    .all(|x| x["issue"].is_number() && x["turn"].is_number())
            }),
        ),
        (
            "demand",
            null_or(&v["demand"], &|d| d["waiting"].is_number()),
        ),
        (
            "hold",
            null_or(&v["hold"], &|h| {
                h["why"].is_string() && h["text"].is_string()
            }),
        ),
        ("room", null_or(&v["room"], &Value::is_object)),
    ];
    for (key, ok) in checks {
        assert!(ok, "{key} is not of the promised shape:\n{out}");
    }
}

fn field(v: &Value, keys: &[&str]) -> Value {
    keys.iter()
        .map(|k| (k.to_string(), v[*k].clone()))
        .collect()
}

#[test]
fn summary_a_health_file_older_than_ten_minutes_is_not_believed_exit_3() {
    let w = World::new("summary-old");
    w.status();
    health_time(&w, json!(iso(now() - 3 * 3600)));
    let r = w.summary(&[]);
    assert_eq!(r.rc, 3, "{}", r.out);
    has(
        &r.out,
        "factory   stale: the dispatcher last reported 3 h ago",
    );
    has(&r.out, "It said pass: Idle, taking work.");
    has(&r.out, "queue     2 in line");
}

#[test]
fn summary_nine_minutes_old_is_still_believed() {
    let w = World::new("summary-nine");
    w.status();
    health_time(&w, json!(iso(now() - 9 * 60)));
    let r = w.summary(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "factory   pass: Idle, taking work. (9 min ago)");
}

#[test]
fn summary_a_time_that_cannot_be_read_counts_as_old() {
    let w = World::new("summary-unread");
    w.status();
    health_time(&w, json!("yesterday"));
    let r = w.summary(&[]);
    assert_eq!(r.rc, 3, "{}", r.out);
    has(
        &r.out,
        "factory   stale: the dispatcher last reported at an unknown time",
    );
}

#[test]
fn summary_no_snapshot_yet_fails_exit_3_with_the_reason() {
    let w = World::new("summary-no-snapshot");
    let h = w.health();
    let r = w.summary(&[]);
    let out = format!("{}\n{}", h.out, r.out);
    assert_eq!(r.rc, 3, "{out}");
    has(&out, "factory   fail: No queue snapshot yet");
    has(&out, "queue     no snapshot yet");
}

#[test]
fn summary_no_health_file_is_exit_3_with_one_line_and_nothing_is_written() {
    let w = World::new("summary-no-health");
    let r = w.summary(&[]);
    assert_eq!(r.rc, 3, "{}", r.out);
    has(
        &r.out,
        &format!(
            "tehas status: no health file at {} yet",
            w.live_file("health.json").display()
        ),
    );
    assert!(!w.live.exists());
}

#[test]
fn summary_a_health_file_that_is_not_json_is_the_same() {
    let w = World::new("summary-not-json");
    fs::create_dir_all(&w.live).unwrap();
    fs::write(w.live_file("health.json"), "not json\n").unwrap();
    let r = w.summary(&[]);
    assert_eq!(r.rc, 3, "{}", r.out);
    has(&r.out, "no health file");
}

#[test]
fn summary_reads_nothing_but_the_health_file_no_settings_no_gh_no_write() {
    let w = World::new("summary-reads");
    w.status();
    w.clear_calls();
    w.instance(&[r#"echo "settings were read" >&2; exit 9"#]);
    let before = sums(&w.live);
    let r = w.summary(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    lacks(&r.out, "settings were read");
    assert_eq!(w.gh(), "");
    assert_eq!(sums(&w.live), before);
}

#[test]
fn summary_takes_no_argument_but_json() {
    let w = World::new("summary-args");
    let r = w.summary(&["--jsn"]);
    assert_eq!(r.rc, 64, "{}", r.out);
    has(&r.out, "usage: tehas status [--json]");
    let r = w.summary(&["--json", "--json"]);
    assert_eq!(r.rc, 64, "{}", r.out);
}

#[test]
fn status_json_a_fresh_idle_factory_one_object_of_the_promised_shape() {
    let w = World::new("json-idle");
    demand(&w, 0, 0, 30);
    let r = status_then(&w, &["--json"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    let objects = serde_json::Deserializer::from_str(&r.out)
        .into_iter::<Value>()
        .count();
    assert_eq!(objects, 1, "{}", r.out);
    shape(&r.out);
    let v = json_of(&r);
    assert_eq!(
        field(
            &v,
            &[
                "state", "stale", "reasons", "paused", "allHands", "limit", "running", "demand",
                "hold"
            ]
        ),
        json!({"state":"pass","stale":false,"reasons":[],"paused":{"local":false,"label":false},"allHands":false,"limit":{"state":"none"},"running":[],"demand":{"waiting":0},"hold":null})
    );
    assert!(v["age"].as_f64().unwrap() < 60.0, "{v}");
}

#[test]
fn status_json_a_read_only_instance_says_so_the_snapshot_lists_what_its_starts_left_and_no_other_instance_has_the_key(
) {
    let w = World::new("json-read-only");
    demand(&w, 0, 0, 30);
    let r = status_then(&w, &["--json"]);
    assert_eq!(json_of(&r)["readOnly"], Value::Null);
    assert_eq!(w.live_json("status.json")["readOnly"], Value::Null);
    w.reset();
    demand(&w, 0, 0, 30);
    w.minimal(&["TEHAS_READ_ONLY=1"]);
    // The workspace of #7, beside TEHAS_WORK_DIR, the default <home>/work/widget.
    w.home_file(
        "state/read-only/7.json",
        "{\"issue\":7,\"how\":\"cycle\",\"at\":\"2026-11-06T10:00:00Z\"}\n",
    );
    w.home_file(
        "work/widget-7/.git/tehas-read-only-pr.md",
        "# Fix the thing\n\nBranch: feature/issue-7\n\nThe body.\n",
    );
    let r = status_then(&w, &["--json"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    shape(&r.out);
    assert_eq!(json_of(&r)["readOnly"], true);
    let s = w.live_json("status.json");
    assert_eq!(
        field(&s["readOnly"]["starts"][0], &["issue", "how", "pr"]),
        json!({"how":"cycle","issue":7,"pr":{"body":"The body.","branch":"feature/issue-7","title":"Fix the thing"}})
    );
    assert_eq!(s["readOnly"]["on"], true);
}

#[test]
fn status_json_the_demand_with_how_long_the_oldest_has_waited() {
    let w = World::new("json-demand");
    demand(&w, 2, 5400, 30);
    let r = status_then(&w, &["--json"]);
    let v = json_of(&r);
    assert_eq!(v["demand"]["waiting"], 2);
    let wait = v["demand"]["oldestWait"].as_f64().unwrap();
    assert!((5400.0..5460.0).contains(&wait), "{v}");
}

#[test]
fn status_a_hold_on_refreshes_is_in_the_demand_and_in_the_text() {
    let w = World::new("refresh-hold");
    demand(&w, 0, 0, 30);
    let hold = "the pi-rukas in the sandbox image does not take --continue-pr";
    edit(&w, "demand.json", |v| v["refreshHold"] = json!(hold));
    let r = status_then(&w, &["--json"]);
    assert_eq!(json_of(&r)["demand"]["refreshHold"], hold);
    shape(&r.out);
    let r = status_then(&w, &[]);
    has(&r.out, &format!("refresh   held: {hold}"));
}

#[test]
fn status_issues_tehas_blocked_holds_by_the_label_alone_are_in_the_demand_and_in_the_text() {
    let w = World::new("label-alone");
    demand(&w, 0, 0, 30);
    edit(&w, "demand.json", |v| v["labelAlone"] = json!([7, 9]));
    let r = status_then(&w, &["--json"]);
    assert_eq!(json_of(&r)["demand"]["labelAlone"], json!([7, 9]));
    shape(&r.out);
    let r = status_then(&w, &[]);
    has(
        &r.out,
        "blocked   by the label alone, no blockers named: #7, #9.",
    );
}

#[test]
fn status_no_hold_on_refreshes_no_line_and_no_field() {
    let w = World::new("no-refresh-hold");
    demand(&w, 0, 0, 30);
    let r = status_then(&w, &[]);
    lacks(&r.out, "refresh   ");
    let r = w.summary(&["--json"]);
    let d = &json_of(&r)["demand"];
    assert!(d.is_object(), "{d}");
    assert!(d.get("refreshHold").is_none(), "{d}");
    assert!(d.get("labelAlone").is_none(), "{d}");
}

#[test]
fn status_json_no_demand_from_the_last_ten_minutes_is_null() {
    let w = World::new("json-old-demand");
    demand(&w, 2, 5400, 700);
    let r = status_then(&w, &["--json"]);
    assert_eq!(json_of(&r)["demand"], Value::Null);
    shape(&r.out);
}

#[test]
fn status_json_a_busy_factory_lists_its_running_cycles() {
    let w = World::new("json-busy");
    w.cycle(true, "running", "develop");
    let r = status_then(&w, &["--json"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    let v = json_of(&r);
    let running: Vec<Value> = v["running"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| json!({"issue": x["issue"], "step": x["progress"]["step"]}))
        .collect();
    assert_eq!(Value::from(running), json!([{"issue":12,"step":"develop"}]));
    let since = v["running"][0]["since"].as_str().unwrap();
    assert_eq!(
        jq("fromdateiso8601 | type", &json!(since).to_string()),
        "\"number\""
    );
    shape(&r.out);
}

#[test]
fn status_json_a_held_factory_says_why_and_the_room_under_each_cap() {
    let w = World::new("json-held");
    w.held(
        "count",
        "holding: 1 cycles are running (limit 1)",
        json!(0),
        json!(3),
    );
    let r = status_then(&w, &["--json"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    let v = json_of(&r);
    assert_eq!(
        v["hold"],
        json!({"why":"count","text":"holding: 1 cycles are running (limit 1)"})
    );
    assert_eq!(v["room"], json!({"cycles":0,"maxAwaiting":3,"slice":3}));
    shape(&r.out);
}

/// A gate whose runs finished without saying they passed holds the same way
/// as a red branch, under a word of its own: the one tells a person to fix
/// the branch, the other to name the right check.
#[test]
fn status_an_unknown_gate_is_the_hold_in_the_text_and_in_json() {
    let w = World::new("unknown-gate");
    demand(&w, 0, 0, 30);
    let text = r#"holding: the check "build / verify (linux)" on main at 0123456789ab finished without saying it passed: cycles start"#;
    w.held("unknown-gate", text, json!(1), json!(3));
    let r = status_then(&w, &[]);
    has(
        &r.out,
        r#"slice     holding: the check "build / verify (linux)" on main at 0123456789ab"#,
    );
    let r = status_then(&w, &["--json"]);
    assert_eq!(
        json_of(&r)["hold"],
        json!({"why":"unknown-gate","text":text})
    );
}

#[test]
fn status_a_red_branch_is_the_hold_in_the_text_and_in_json() {
    let w = World::new("red");
    w.held(
        "red",
        "holding: main is red at 0123456789ab",
        json!(1),
        json!(3),
    );
    let r = status_then(&w, &[]);
    has(&r.out, "slice     holding: main is red at 0123456789ab (");
    let r = status_then(&w, &["--json"]);
    assert_eq!(
        json_of(&r)["hold"],
        json!({"why":"red","text":"holding: main is red at 0123456789ab"})
    );
}

#[test]
fn status_json_room_the_last_poll_did_not_read_is_left_out() {
    let w = World::new("json-room");
    w.held("", "", json!(1), Value::Null);
    let r = status_then(&w, &["--json"]);
    let v = json_of(&r);
    assert_eq!(v["hold"], Value::Null);
    assert_eq!(v["room"], json!({"cycles":1,"slice":3}));
}

#[test]
fn status_json_a_paused_factory_by_the_flag_or_by_the_label_is_held_by_the_pause() {
    let w = World::new("json-paused");
    w.home_file(
        "state/pause.json",
        r#"{"by":"boss","at":"2026-10-15T10:00:00Z","reason":"waiting for the gate"}"#,
    );
    let r = status_then(&w, &["--json"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    let v = json_of(&r);
    assert_eq!(
        json!([v["state"], v["paused"], v["hold"]["why"]]),
        json!(["warn", {"local":true,"label":false}, "pause"])
    );
    has(v["hold"]["text"].as_str().unwrap(), "waiting for the gate");
    lacks(&r.out, "boss");
    assert_eq!(v["reasons"].as_array().unwrap().len(), 1);
    shape(&r.out);
    w.reset();
    w.fx("paused", "true\n");
    let r = status_then(&w, &["--json"]);
    let v = json_of(&r);
    assert_eq!(
        json!([v["paused"], v["hold"]["why"]]),
        json!([{"local":false,"label":true}, "pause"])
    );
}

#[test]
fn status_json_a_failed_cycle_is_exit_3_as_the_text_is() {
    let w = World::new("json-failed");
    w.cycle(false, "aborted", "develop");
    let r = status_then(&w, &["--json"]);
    assert_eq!(r.rc, 3, "{}", r.out);
    let v = json_of(&r);
    assert_eq!(v["state"], "fail");
    has(v["reasons"][0].as_str().unwrap(), "#12 ended as aborted");
}

#[test]
fn status_json_a_stale_health_file_is_exit_3_and_says_so() {
    let w = World::new("json-stale");
    w.status();
    health_time(&w, json!(iso(now() - 3 * 3600)));
    let r = w.summary(&["--json"]);
    assert_eq!(r.rc, 3, "{}", r.out);
    let v = json_of(&r);
    assert_eq!(v["stale"], true);
    assert!(v["age"].as_f64().unwrap() >= 10800.0, "{v}");
    shape(&r.out);
}

#[test]
fn status_json_no_health_file_is_exit_3_with_nothing_on_standard_output() {
    let w = World::new("json-no-health");
    let r = w.sh(&format!("{} --json 2>/dev/null", w.summary_cmd()));
    assert_eq!(r.rc, 3, "{}", r.out);
    assert_eq!(r.out, "");
}

#[test]
fn status_json_reads_nothing_but_the_health_file_no_settings_no_gh_no_write() {
    let w = World::new("json-reads");
    demand(&w, 1, 60, 30);
    w.status();
    w.clear_calls();
    w.instance(&[r#"echo "settings were read" >&2; exit 9"#]);
    let before = sums(&w.live);
    let r = w.summary(&["--json"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    lacks(&r.out, "settings were read");
    assert_eq!(w.gh(), "");
    assert_eq!(sums(&w.live), before);
}

// tehas:all-hands on the control issue: reported, and nothing else.

/// The label's last event, by `login` (none for ""), and the label on the
/// issue unless the event is `unlabeled`.
fn all_hands(w: &World, login: &str, event: &str) {
    if event != "unlabeled" {
        w.touch("allhands");
    }
    let actor = if login.is_empty() {
        Value::Null
    } else {
        json!({"login": login})
    };
    let events = json!([{"event": event, "label": {"name": "tehas:all-hands"},
        "actor": actor, "created_at": "2026-10-17T10:00:00Z"}]);
    w.fx("events.json", &format!("{events}\n"));
}

#[test]
fn all_hands_applied_by_an_allowed_account_it_is_reported_and_changes_nothing_else() {
    let w = World::new("all-hands-on");
    all_hands(&w, "alice", "labeled");
    let r = status_then(&w, &[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.live_json("status.json")["allHands"], true);
    let h = w.live_json("health.json");
    assert_eq!(h["checks"]["allHands"], true);
    assert_eq!(h["status"], "pass");
    has(&r.out, "all-hands tehas:all-hands is on the control issue");
    let v = json_of(&w.summary(&["--json"]));
    assert_eq!(
        json!([v["allHands"], v["state"], v["hold"]]),
        json!([true, "pass", null])
    );
}

#[test]
fn all_hands_applied_by_anyone_else_it_is_ignored_and_said_once() {
    let w = World::new("all-hands-other");
    all_hands(&w, "mallory", "labeled");
    let r = w.status();
    assert_eq!(w.live_json("status.json")["allHands"], false);
    has(
        &r.out,
        "tehas:all-hands on #1 was applied by 'mallory', not an allowed account: ignored",
    );
    let r = w.status();
    assert_eq!(r.out, "");
    let r = w.summary(&[]);
    lacks(&r.out, "all-hands");
}

#[test]
fn all_hands_not_on_the_issue_is_off_whatever_its_last_event_says() {
    let w = World::new("all-hands-off");
    all_hands(&w, "alice", "unlabeled");
    w.status();
    assert_eq!(w.live_json("status.json")["allHands"], false);
    w.reset();
    all_hands(&w, "alice", "labeled");
    w.rm_fx("allhands");
    w.status();
    assert_eq!(w.live_json("status.json")["allHands"], false);
}

#[test]
fn all_hands_on_the_issue_with_events_that_cannot_be_read_is_unknown_and_left_out_of_json() {
    let w = World::new("all-hands-unknown");
    all_hands(&w, "alice", "labeled");
    w.rm_fx("events.json");
    let r = w.status();
    has(
        &r.out,
        "WARN status: gh api repos/acme/widget/issues/1/events?per_page=100 failed",
    );
    let r = w.summary(&["--json"]);
    assert_eq!(w.live_json("status.json")["allHands"], Value::Null);
    assert!(json_of(&r).get("allHands").is_none(), "{}", r.out);
    w.reset();
    w.touch("control-down");
    w.status();
    assert_eq!(w.live_json("status.json")["allHands"], Value::Null);
}

#[test]
fn all_hands_a_snapshot_older_than_ten_minutes_does_not_say_it() {
    let w = World::new("all-hands-old");
    all_hands(&w, "alice", "labeled");
    w.status();
    edit(&w, "status.json", |v| {
        v["generatedAt"] = json!(iso(now() - 20 * 60))
    });
    w.health();
    assert_eq!(
        w.live_json("health.json")["checks"]["allHands"],
        Value::Null
    );
}

#[test]
fn all_hands_a_paused_factory_still_reports_it() {
    let w = World::new("all-hands-paused");
    all_hands(&w, "alice", "labeled");
    w.fx("paused", "true\n");
    let r = status_then(&w, &["--json"]);
    let v = json_of(&r);
    assert_eq!(
        json!([v["allHands"], v["paused"]["label"]]),
        json!([true, true])
    );
}
