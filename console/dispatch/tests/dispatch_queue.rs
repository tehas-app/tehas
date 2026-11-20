//! tehas:blocked, the demand and the fault of a sandbox that cannot push,
//! where the poll's process says more than its decision: what gh is asked,
//! the store's record of a start, the files the poll writes, the console
//! that reads them and the push probe.

mod common;

use common::*;
use serde_json::{json, Value};

/// One comment, written at second `id` (10-59), edited by `editor` when
/// one is named (bnote).
fn bnote(id: u64, author: &str, body: &str, editor: &str) -> Value {
    let ed = !editor.is_empty();
    json!({"databaseId": id, "author": {"login": author}, "body": body,
        "createdAt": format!("2026-10-01T01:00:{id}Z"),
        "lastEditedAt": if ed { json!("2026-10-01T02:00:00Z") } else { Value::Null },
        "editor": if ed { json!({"login": editor}) } else { Value::Null }})
}

fn notes(w: &World, n: u64, list: &[Value]) {
    w.fx_json(
        &format!("comments-{n}.json"),
        &json!({"data": {"repository": {"issue": {"comments": {"nodes": list}}}}}),
    );
}

/// The blockers as GitHub gives them; an empty state is a number it does
/// not know.
fn states(w: &World, list: &[(u64, &str)]) {
    let mut repo = serde_json::Map::new();
    for (n, s) in list {
        let v = if s.is_empty() {
            Value::Null
        } else {
            json!({"state": s})
        };
        repo.insert(format!("b{n}"), v);
    }
    w.fx_json("blockers.json", &json!({"data": {"repository": repo}}));
}

/// tehas:ready applied by `login` at `at` is the issue's only event.
fn ready_at(w: &World, n: u64, login: &str, at: &str) {
    w.fx_json(
        &format!("events-{n}.json"),
        &json!([{"event": "labeled", "label": {"name": "tehas:ready"},
            "actor": {"login": login}, "created_at": at}]),
    );
}

/// The issue carries tehas:blocked and these labels, and no tehas:ready.
fn alone(w: &World, n: u64, labels: &[&str]) {
    w.fx("ready.json", "[]");
    let mut all = vec![json!({"name": "tehas:blocked"})];
    all.extend(labels.iter().map(|l| json!({"name": l})));
    w.fx_json("blocked.json", &json!([{"number": n, "labels": all}]));
}

/// tehas:blocked by `login` is the issue's only label event.
fn blocked_by(w: &World, n: u64, login: &str) {
    w.fx_json(
        &format!("events-{n}.json"),
        &json!([{"event": "labeled", "label": {"name": "tehas:blocked"},
            "actor": {"login": login}, "created_at": "2026-10-01T00:01:00Z"}]),
    );
}

/// #7 carries tehas:blocked beside tehas:ready, applied by `by`, and a
/// comment of `author`'s names its blockers.
fn blocked(w: &World, by: &str, author: &str, marker: &str) {
    w.queue(&["7 tehas:blocked"]);
    w.label_events(7, &[&format!("tehas:blocked={by}")]);
    notes(w, 7, &[bnote(10, author, marker, "")]);
}

fn starts_json(w: &World) -> String {
    w.read_home("state/starts.json")
}

/// The console of a host, as a shell runs it.
fn console(w: &World, words: &str) -> Run {
    w.sh(&format!("'{}' {words}", w.console().display()))
}

/// The time `ago` before now, as GitHub writes one.
fn iso_ago(ago: &str) -> String {
    let out = std::process::Command::new("date")
        .args(["-u", "-d", ago, "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

const DENIED: &str = "remote: Permission to acme/widget denied to tehas-bot";

/// The host file a start that could not push leaves, from `at`.
fn push_fault(w: &World, at: &str) {
    w.home_file(
        "state/push-fault.json",
        &format!(
            "{}\n",
            json!({"at": at, "login": "tehas-bot", "line": DENIED})
        ),
    );
}

fn fault(w: &World) -> Value {
    w.json_home("state/push-fault.json")
}

fn why(w: &World) -> Value {
    w.dispatch_state()["why"].clone()
}

fn pause(w: &World) {
    w.pause_flag(r#"{"by":"boss","at":"2026-10-15T10:00:00Z","reason":"weekend"}"#);
}

#[test]
fn the_poll_records_why_each_issue_would_not_start_and_tehas_order_and_tehas_blockers_read_it() {
    let w = World::new("queue-record");
    w.queue(&["7 tehas:blocked", "8"]);
    w.label_events(7, &["tehas:blocked=alice"]);
    notes(
        &w,
        7,
        &[bnote(
            10,
            "alice",
            "Blocked by #12.\n\n<!-- tehas:blocked by=12 -->",
            "",
        )],
    );
    states(&w, &[(12, "OPEN")]);
    assert_eq!(w.poll().rc, 0);
    assert_eq!(w.started(), "8 ");
    let record = starts_json(&w);
    assert_eq!(
        jq(
            r#"[.issues[] | [.issue, (.reason.code // "start")]]"#,
            &record
        ),
        r#"[[7,"blocked"],[8,"start"]]"#
    );
    assert_eq!(
        jq(".edges", &record),
        r#"[{"by":"alice","issue":7,"kind":"marker","on":12}]"#
    );
    assert_eq!(
        jq("[.holds[] | [.issue, .reason.code]]", &record),
        r#"[[12,"no-approval"]]"#
    );
    let c = w.console().display().to_string();
    let r = w.sh(&format!(
        "'{c}' tehas order set --issues 7,8,9 >/dev/null && '{c}' tehas order --json"
    ));
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(jq("[.issues, .only]", &r.out), "[[7,8,9],false]");
    assert_eq!(
        jq(
            r#"[.starts.issues[] | [.issue, .start, (.reason.code // "")]]"#,
            &r.out
        ),
        r#"[[7,false,"blocked"],[8,true,""],[9,false,"unseen"]]"#
    );
    has(&jq(".starts.issues[0].reason.text", &r.out), "#12 is open");
    let r = console(&w, "tehas blockers --json");
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        jq(
            "[.state, .cycles, [.chains[] | [.issue, .cycle, [.ends[] | [.issue, .reason.code]]]]]",
            &r.out
        ),
        r#"["ok",[],[[7,false,[[12,"no-approval"]]]]]"#
    );
    has(
        &console(&w, "tehas blockers").out,
        "#7 waits on #12: it is open, and no allowed account made it ready",
    );
}

#[test]
fn a_dry_run_writes_no_record_and_with_none_tehas_blockers_says_not_yet() {
    let w = World::new("queue-dry-record");
    blocked(&w, "alice", "alice", "<!-- tehas:blocked by=12 -->");
    states(&w, &[(12, "OPEN")]);
    let _ = std::fs::remove_file(w.home.join("state/starts.json"));
    assert_eq!(w.dispatch(&["--dry-run"]).rc, 0);
    assert!(!w.home.join("state/starts.json").exists());
    let r = console(&w, "tehas blockers --json");
    assert_eq!(r.rc, 3);
    assert_eq!(r.out, r#"{"api":1,"state":"none"}"#);
}

#[test]
fn an_open_blocked_by_link_holds_a_ready_issue_and_the_slot_goes_to_the_next_one() {
    let w = World::new("queue-link");
    w.queue(&["7", "8"]);
    w.fx(
        "work-7.json",
        r#"{"number":7,"state":"open","issue_dependencies_summary":{"blocked_by":1}}"#,
    );
    let r = w.poll();
    assert_eq!(r.rc, 0);
    assert_eq!(w.started(), "8 ");
    has(&w.gh(), "issueDependenciesSummary { blockedBy }");
    has(
        &r.out,
        "wait #7: a blocked-by link holds it: 1 blocker is open",
    );
}

#[test]
fn a_blocked_issue_starts_once_every_blocker_it_names_is_closed() {
    let w = World::new("queue-closed");
    blocked(&w, "alice", "alice", "<!-- tehas:blocked by=12,34 -->");
    states(&w, &[(12, "CLOSED"), (34, "CLOSED")]);
    w.poll();
    assert_eq!(w.started(), "7 ");
    has(&w.gh(), "b12: issueOrPullRequest(number: 12)");
    has(&w.gh(), "b34: issueOrPullRequest(number: 34)");
}

#[test]
fn a_blocked_issue_that_starts_is_recorded_as_the_ready_labels_not_the_blocked_labels() {
    let w = World::new("queue-recorded");
    w.slots(1, &[]);
    blocked(&w, "tehas-bot", "tehas-bot", "<!-- tehas:blocked by=12 -->");
    states(&w, &[(12, "CLOSED")]);
    let r = w.poll();
    assert_eq!(w.started(), "7 ");
    assert_eq!(
        w.row(&["cycle-show", "7"])["cycle"]["started_by"],
        json!("alice")
    );
    assert_eq!(
        w.approval(r#""\(.by) \(.at)""#, 7),
        "alice 2026-10-01T00:00:00Z"
    );
    lacks(&r.out, "authorized by @tehas-bot");
}

#[test]
fn the_fixers_tehas_blocked_and_its_comment_count() {
    let w = World::new("queue-fixer");
    w.slots(1, &[]);
    blocked(&w, "tehas-bot", "tehas-bot", "<!-- tehas:blocked by=12 -->");
    states(&w, &[(12, "OPEN")]);
    let r = w.poll();
    assert_eq!(w.started(), "");
    has(&r.out, "wait #7: tehas:blocked holds it: #12 is open");
}

// A GitHub App as the fixer: its label events are REST's, <slug>[bot], and
// its comments GraphQL's, <slug>. Either form of the setting is that one
// account.
#[test]
fn the_app_as_the_fixer_its_tehas_blocked_and_its_comment_count() {
    let w = World::new("queue-app-fixer");
    w.slots(1, &[]);
    blocked(
        &w,
        "tehas-bot[bot]",
        "tehas-bot",
        "<!-- tehas:blocked by=12 -->",
    );
    states(&w, &[(12, "OPEN")]);
    let r = w.poll();
    assert_eq!(w.started(), "");
    has(&r.out, "wait #7: tehas:blocked holds it: #12 is open");
}

#[test]
fn the_app_as_the_fixer_a_comment_by_a_name_like_its_own_names_no_blockers() {
    let w = World::new("queue-app-like");
    w.slots(1, &[]);
    blocked(
        &w,
        "tehas-bot[bot]",
        "tehas-bot2",
        "<!-- tehas:blocked by=12 -->",
    );
    states(&w, &[(12, "OPEN")]);
    let r = w.poll();
    assert_eq!(w.started(), "");
    has(&r.out, "wait #7: tehas:blocked holds it: no comment of an allowed account or the millwright names its blockers");
}

#[test]
fn tehas_blocked_alone_starts_the_issue_once_its_blockers_close_and_comes_off() {
    let w = World::new("queue-alone");
    alone(&w, 7, &[]);
    blocked_by(&w, 7, "alice");
    notes(
        &w,
        7,
        &[bnote(10, "alice", "<!-- tehas:blocked by=12 -->", "")],
    );
    states(&w, &[(12, "CLOSED")]);
    assert_eq!(w.poll().rc, 0);
    assert_eq!(w.started(), "7 ");
    let gh = w.gh();
    has(
        &gh,
        "issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:blocked",
    );
    lacks(&gh, "--remove-label tehas:ready");
    has(&gh, "authorized by @alice's `tehas:blocked` label");
    assert_eq!(
        w.approval(r#""\(.by) \(.at)""#, 7),
        "alice 2026-10-01T00:01:00Z"
    );
}

#[test]
fn tehas_blocked_alone_from_the_fixer_holds_the_issue_but_does_not_start_it() {
    let w = World::new("queue-alone-fixer");
    w.slots(1, &[]);
    alone(&w, 7, &[]);
    blocked_by(&w, 7, "tehas-bot");
    notes(
        &w,
        7,
        &[bnote(10, "tehas-bot", "<!-- tehas:blocked by=12 -->", "")],
    );
    states(&w, &[(12, "CLOSED")]);
    w.poll();
    assert_eq!(w.started(), "");
    lacks(&w.gh(), "issue edit 7 ");
}

#[test]
fn demand_a_poll_with_no_room_counts_the_waiting_issues_and_the_oldest_acceptance() {
    let w = World::new("queue-demand");
    w.ready(&[7, 8]);
    ready_at(&w, 7, "alice", "2026-10-17T10:00:00Z");
    ready_at(&w, 8, "alice", "2026-10-17T09:00:00Z");
    w.hold(5);
    let r = w.poll();
    w.release();
    assert_eq!(r.rc, 0);
    assert_eq!(w.calls("fake-start"), "");
    let demand = std::fs::read_to_string(w.demand_file()).unwrap();
    assert_eq!(
        jq("{waiting, oldest}", &demand),
        r#"{"waiting":2,"oldest":"2026-10-17T09:00:00Z"}"#
    );
    assert_eq!(jq(".time | fromdateiso8601 | type", &demand), r#""number""#);
}

// The one case that reads health's output, so it runs the real command.
#[test]
fn demand_the_health_file_carries_it_after_the_poll() {
    let w = World::new("queue-demand-health");
    w.real_health();
    ready_at(&w, 7, "alice", "2026-10-17T10:00:00Z");
    w.hold(5);
    w.poll();
    w.release();
    assert_eq!(
        jq(
            ".checks.demand | {waiting, oldest}",
            &w.read_home("live/health.json")
        ),
        r#"{"waiting":1,"oldest":"2026-10-17T10:00:00Z"}"#
    );
}

#[test]
fn demand_the_fixers_label_waits_like_a_persons() {
    let w = World::new("queue-demand-fixer");
    w.slots(1, &[]);
    ready_at(&w, 7, "tehas-bot", "2026-10-17T10:00:00Z");
    w.hold(5);
    w.poll();
    w.release();
    assert_eq!(
        jq(
            ".waiting",
            &std::fs::read_to_string(w.demand_file()).unwrap()
        ),
        "1"
    );
}

// The fault of a minute ago is well inside the ten minutes the poll holds
// on; one of an hour ago is past the wait, so the poll asks the sandbox
// again.

#[test]
fn a_push_fault_older_than_ten_minutes_is_the_polls_to_end_it_asks_the_sandbox_and_starts() {
    let w = World::new("queue-probe-ok");
    w.slots(3, &[]);
    w.ready(&[7, 8]);
    push_fault(&w, &iso_ago("-1 hour"));
    w.env(
        "TEHAS_PUSH_PROBE_CMD",
        &w.t().join("bin/probe-ok").to_string_lossy(),
    );
    w.stub("probe-ok", "exit 0");
    let r = w.poll();
    assert_eq!(r.rc, 0);
    assert_eq!(w.started(), "7 8 ");
    has(&r.out, "push probe: the sandbox can push: the fault is off");
    assert_eq!(w.calls("probe-ok").lines().count(), 1);
    lacks(&r.out, "holding: the sandbox cannot push");
    assert_eq!(why(&w), json!(""));
    assert!(
        !w.home.join("state/push-fault.json").exists(),
        "a passing probe left the fault on record"
    );
}

// The snapshot of this poll is read before the probe, so the line of the
// hold it decided on is git's own word of the start an hour ago; the login
// of the renewed fault is the settings' answer of state/github/login.
#[test]
fn a_push_fault_older_than_ten_minutes_and_a_probe_that_is_refused_holds_and_renews_the_fault() {
    let w = World::new("queue-probe-bad");
    w.slots(3, &[]);
    w.ready(&[7, 8]);
    push_fault(&w, &iso_ago("-1 hour"));
    w.env(
        "TEHAS_PUSH_PROBE_CMD",
        &w.t().join("bin/probe-bad").to_string_lossy(),
    );
    w.stub(
        "probe-bad",
        &format!("printf '%s\\n' '{DENIED}' >&2\nexit 1"),
    );
    let r = w.poll();
    assert_eq!(r.rc, 0);
    assert_eq!(w.calls("fake-start"), "");
    has(
        &r.out,
        &format!("push probe: the sandbox cannot push: {DENIED}"),
    );
    let f = fault(&w);
    assert_eq!(
        format!(
            "{} {}",
            f["login"].as_str().unwrap(),
            f["line"].as_str().unwrap()
        ),
        format!("tehas-bot[bot] {DENIED}")
    );
    // The renewed fault holds the next poll the same way, in git's words and
    // the bot's name.
    has(
        &r.out,
        &format!("holding: the sandbox cannot push: {DENIED} (tehas-bot[bot])"),
    );
    assert_eq!(why(&w), json!("lane"));
    assert_eq!(w.calls("probe-bad").lines().count(), 1);
    // The time is the probe's own: the next poll waits the wait out.
    let now = epoch("now");
    let at = epoch(f["at"].as_str().unwrap());
    assert!((at - now).abs() <= 2, "stale: {}", at - now);
}

#[test]
fn a_push_fault_younger_than_ten_minutes_asks_no_probe_and_starts_nothing() {
    let w = World::new("queue-probe-young");
    w.slots(3, &[]);
    w.ready(&[7, 8]);
    push_fault(&w, &iso_ago("-1 minute"));
    w.env(
        "TEHAS_PUSH_PROBE_CMD",
        &w.t().join("bin/probe-ok").to_string_lossy(),
    );
    w.stub("probe-ok", "exit 0");
    assert_eq!(w.poll().rc, 0);
    assert_eq!(w.calls("fake-start"), "");
    assert_eq!(
        w.calls("probe-ok"),
        "",
        "the wait holds, so the poll asks nothing"
    );
    assert_eq!(why(&w), json!("lane"));
}

#[test]
fn a_probe_the_host_could_not_run_records_nothing_and_the_starts_decide_as_before() {
    let w = World::new("queue-probe-missing");
    w.slots(3, &[]);
    w.ready(&[7, 8]);
    let at = iso_ago("-1 hour");
    push_fault(&w, &at);
    // A stand-in that cannot be run: the probe asked nothing of the sandbox.
    w.env(
        "TEHAS_PUSH_PROBE_CMD",
        &w.t().join("bin/probe-missing").to_string_lossy(),
    );
    let r = w.poll();
    assert_eq!(r.rc, 0);
    has(&r.out, "no fault recorded");
    lacks(&r.out, "holding: the sandbox cannot push");
    assert_eq!(w.started(), "7 8 ");
    assert_eq!(
        fault(&w)["at"],
        json!(at),
        "the fault keeps the time the start wrote"
    );
}

#[test]
fn a_push_fault_past_the_wait_asks_no_probe_while_nothing_is_ready_or_the_factory_is_paused() {
    let w = World::new("queue-probe-idle");
    w.slots(3, &[]);
    w.fx("ready.json", "[]");
    let at = iso_ago("-1 hour");
    push_fault(&w, &at);
    w.env(
        "TEHAS_PUSH_PROBE_CMD",
        &w.t().join("bin/probe-ok").to_string_lossy(),
    );
    w.stub("probe-ok", "exit 0");
    assert_eq!(w.poll().rc, 0);
    assert_eq!(
        w.calls("probe-ok").lines().count(),
        0,
        "nothing ready: no start for the probe to free"
    );
    w.ready(&[7, 8]);
    pause(&w);
    assert_eq!(w.poll().rc, 0);
    assert_eq!(
        w.calls("probe-ok").lines().count(),
        0,
        "paused: no start for the probe to free"
    );
    assert_eq!(w.calls("fake-start"), "");
    assert_eq!(fault(&w)["at"], json!(at));
}

#[test]
fn with_no_fault_on_record_the_queue_starts_and_nothing_is_said_of_it() {
    let w = World::new("queue-no-fault");
    w.slots(3, &[]);
    w.ready(&[7, 8]);
    let r = w.poll();
    assert_eq!(r.rc, 0);
    assert_eq!(w.started(), "7 8 ");
    lacks(&r.out, "the sandbox cannot push");
}
