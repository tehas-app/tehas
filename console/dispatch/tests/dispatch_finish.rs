//! tehasbot dispatch without the test seams, filling the slots, stopping
//! finished cycles and the retry after a provider outage: the cases that
//! need the process. The decisions are the fixtures of finish.json.

mod common;

use common::*;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn world() -> World {
    let w = World::new("finish");
    w.mesh_curl();
    w
}

/// The stand-ins a bare tree must not call: a poll there runs the commands
/// beside it.
const BESIDE: &[&str] = &[
    "tehas",
    "tehasbot",
    "tehas-work",
    "tehas-status",
    "tehas-index",
    "tehas-plan",
];

/// Outside the tests nothing names the scripts: the dispatcher runs the ones
/// in its own directory, whatever PATH holds, and starts a cycle through
/// systemd. Only tehasbot status, tehas plan and tehas index are stubbed.
fn bare_tree(w: &World) -> PathBuf {
    let tree = w.t().join("tree");
    let _ = fs::remove_dir_all(&tree);
    for d in ["lib", "bin", "libexec"] {
        fs::create_dir_all(tree.join(d)).unwrap();
    }
    for e in fs::read_dir(root().join("lib")).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "sh") {
            fs::copy(&p, tree.join("lib").join(p.file_name().unwrap())).unwrap();
        }
    }
    for verb in ["tehas", "tehasbot"] {
        fs::copy(
            root().join("bin").join(verb),
            tree.join("bin").join(format!("{verb}.real")),
        )
        .unwrap();
    }
    fs::copy(w.console(), tree.join("libexec/console")).unwrap();
    fs::copy(w.dispatcher(), tree.join("libexec/tehas-dispatch")).unwrap();
    let t = w.t().display().to_string();
    let wrapper = |name: &str, lines: &[String]| {
        let p = tree.join("bin").join(name);
        fs::write(&p, format!("#!/usr/bin/env bash\n{}\n", lines.join("\n"))).unwrap();
        fs::set_permissions(&p, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    };
    // Only the poll itself is the real one.
    wrapper(
        "tehasbot",
        &[
            format!("[ \"$1\" != status ] || exec '{t}/bin/fake-status'"),
            r#"[ "$1" != dispatch ] || exec "$(dirname "$0")/tehasbot.real" "$@""#.into(),
        ],
    );
    wrapper(
        "tehas",
        &[
            format!("[ \"$1\" != plan ] || exec '{t}/bin/fake-plan' \"${{@:2}}\""),
            format!("[ \"$1\" != index ] || exec '{t}/bin/fake-index' \"${{@:2}}\""),
            r#"exec "$(dirname "$0")/tehas.real" "$@""#.into(),
        ],
    );
    for name in BESIDE {
        w.stub(name, "");
    }
    // Runs the unit's command at once, after its own options.
    w.stub(
        "systemd-run",
        r#"while [ "$#" -gt 0 ]; do case "$1" in --unit | -p) shift 2 ;; -*) shift ;; *) break ;; esac; done
"$@""#,
    );
    tree
}

/// A poll of the bare tree, with none of the test's commands named: the
/// cgroup root is the one seam left.
fn bare_poll(w: &World, tree: &std::path::Path) -> Run {
    let unset: String = [
        "TEHAS_CYCLE_START_CMD",
        "TEHAS_CYCLE_START_WAIT",
        "TEHAS_STATUS_CMD",
        "TEHAS_TIDY_CMD",
        "TEHAS_INDEX_CMD",
        "TEHAS_INDEX_START_CMD",
        "TEHAS_PLAN_CMD",
        "TEHAS_PLAN_START_CMD",
        "TEHAS_DISPATCH_CMD",
        "TEHAS_HEALTH_CMD",
    ]
    .iter()
    .map(|k| format!("-u {k} "))
    .collect();
    w.sh(&format!(
        "exec env {unset}'{}/bin/tehasbot' dispatch",
        tree.display()
    ))
}

/// systemd that loads the unit and starts a cycle as the unit would.
fn systemd_loads(w: &World) {
    w.stub(
        "systemctl",
        r#"case "$1 $2" in
  "show tehas.slice") cat "$T/fx/slice.show" ;;
  "--user show") echo loaded ;;
  "--user start") n=${3#tehas-cycle@}
    n=${n%.service}
    flock -w 5 "$TEHAS_HOME/state/cycles/$n.lock" "$T/store-bin" cycle-start "$n" >/dev/null ;;
esac"#,
    );
}

#[test]
fn without_the_test_seams_it_runs_the_scripts_beside_it_and_starts_the_unit() {
    let w = world();
    let tree = bare_tree(&w);
    systemd_loads(&w);
    let r = bare_poll(&w, &tree);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        w.calls("systemctl").trim_end(),
        "show tehas.slice -p FragmentPath -p ActiveState -p MemoryMax
--user show -p LoadState --value tehas-cycle@7.service
--user show -p ActiveState --value tehas-index.service
--user start tehas-cycle@7.service"
    );
    // The disk hold comes after the slice's, so the poll asks Docker twice.
    assert_eq!(
        w.calls("docker").trim_end(),
        "info --format {{.CgroupDriver}} {{.CgroupVersion}}
info --format {{.DockerRootDir}}"
    );
    has(&w.calls("fake-index"), "--before-cycle");
    has(
        &w.calls("systemd-run"),
        "--user --quiet --collect --unit tehas-index -p RuntimeMaxSec=2400 /bin/bash -c",
    );
    assert!(!w.home.join("state/dispatch.again").exists());
    // Three, not four: a poll that starts an index rather than a cycle cannot
    // change the queue (#871).
    assert_eq!(w.calls("fake-status").lines().count(), 3);
    for name in BESIDE {
        assert_eq!(w.calls(name), "", "{name}");
    }
}

#[test]
fn without_the_test_seams_a_planning_turn_runs_the_script_beside_it_too() {
    let w = world();
    w.to_plan();
    let tree = bare_tree(&w);
    systemd_loads(&w);
    let r = bare_poll(&w, &tree);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &w.plan_calls(),
        &format!(
            "7 --out {}.out --keep {} --brief {}/tmp/plan-7.brief.",
            w.record().display(),
            w.keep().display(),
            w.home.display()
        ),
    );
    // The unit is the whole limit plus the grace of apply/turn.rs.
    has(
        &w.calls("systemd-run"),
        "--unit tehas-plan-7 -p RuntimeMaxSec=",
    );
    has(&w.gh(), "- [ ] Adopt all");
    for name in ["tehas", "tehasbot", "tehas-plan"] {
        assert_eq!(w.calls(name), "", "{name}");
    }
}

#[test]
fn a_unit_systemd_cannot_load_changes_no_label_and_starts_nothing_exit_1() {
    let w = world();
    let tree = bare_tree(&w);
    w.stub(
        "systemctl",
        r#"case "$1 $2" in
  "show tehas.slice") cat "$T/fx/slice.show" ;;
  "--user show") echo not-found ;;
esac"#,
    );
    let r = bare_poll(&w, &tree);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "ERROR: systemd cannot load tehas-cycle@.service (LoadState: not-found): run install.sh and its units step, and run tehas dispatch as the tehas user's own session");
    lacks(&w.gh(), "issue edit");
    lacks(&w.gh(), "issue comment");
    lacks(&w.calls("systemctl"), "start");
    assert_eq!(w.calls("fake-index"), "");
}

#[test]
fn a_systemd_that_gives_no_answer_changes_no_label_and_starts_nothing_exit_1() {
    let w = world();
    let tree = bare_tree(&w);
    w.stub(
        "systemctl",
        r#"case "$1 $2" in
  "show tehas.slice") cat "$T/fx/slice.show" ;;
  "--user show") exit 1 ;;
esac"#,
    );
    let r = bare_poll(&w, &tree);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "ERROR: systemd cannot load tehas-cycle@.service (LoadState: no answer): run install.sh",
    );
    lacks(&w.gh(), "issue edit");
    lacks(&w.gh(), "issue comment");
    lacks(&w.calls("systemctl"), "start");
}

#[test]
fn a_pi_rukas_no_cycle_has_run_before_starts_as_many_cycles_as_the_slots_allow() {
    let w = world();
    w.slots(4, &[]);
    w.fx("image.label", &format!("{}|1\n", "a".repeat(40)));
    w.ready(&[7, 8, 9, 10, 11, 12]);
    let r = w.poll();
    assert_eq!(w.started(), "7 8 9 10 ");
    lacks(&r.out, "is new on this host");
    assert!(!w.home.join("state/rukas-proven").exists());
}

#[test]
fn with_one_cycle_running_and_five_waiting_one_poll_starts_three() {
    let w = world();
    w.slots(4, &[]);
    w.ready(&[7, 8, 9, 10, 11]);
    w.hold(12);
    let r = w.poll();
    assert_eq!(w.started(), "7 8 9 ");
    lacks(&r.out, "holding");
}

/// The bot's own pull request of an issue in a state, opened at a time.
fn bot_pr(w: &World, n: u64, state: &str, at: &str) {
    w.fx_json(
        "prlist.json",
        &json!([{"url": "https://github.com/acme/widget/pull/90", "state": state,
            "body": format!("Fixes #{n}"), "headRefName": format!("feature/issue-{n}-x"),
            "createdAt": at}]),
    );
}

#[test]
fn an_open_pull_request_one_of_an_earlier_cycle_or_one_of_another_issue_stops_nothing() {
    let w = world();
    w.stub("fake-stop", "");
    w.env("TEHAS_CYCLE_STOP_CMD", "fake-stop");
    w.ready(&[7]);
    // A running cycle: its lock held, its start on record by hand.
    w.hold(9);
    let r = w.store(&["cycle-start", "9", "--by-hand"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    for (n, state, at) in [
        (9, "OPEN", "2099-01-01T00:00:00Z"),
        (9, "MERGED", "2000-01-01T00:00:00Z"),
        (8, "MERGED", "2099-01-01T00:00:00Z"),
    ] {
        bot_pr(&w, n, state, at);
        let r = w.dispatch(&["--dry-run"]);
        lacks(&r.out, "would stop");
    }
    w.poll();
    assert_eq!(w.calls("fake-stop"), "");
    assert_eq!(w.calls("fake-start"), "");
    has(
        &w.gh(),
        "pr list --repo acme/widget --author tehas-bot[bot] --state all",
    );
}

#[test]
fn an_open_issue_marked_tehas_planning_with_no_session_goes_to_a_person() {
    let w = world();
    w.fx("ready.json", "[]");
    w.fx(
        "work-9.json",
        r#"{"number":9,"state":"open","labels":[{"name":"tehas:planning"}]}"#,
    );
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "park #9: it is marked tehas:planning and no planning session is open on it",
    );
    has(
        &w.gh(),
        "issue edit 9 --repo acme/widget --remove-label tehas:planning --add-label needs-human-attention",
    );
    has(&w.gh(), "issue comment 9");
}

#[test]
fn with_no_cycle_running_the_bot_s_pull_requests_are_not_asked_for() {
    let w = world();
    w.poll();
    lacks(&w.gh(), "pr list");
}

// --- A cycle a provider outage parked ---------------------------------------

const MESH: &str = r#"{"data":[{"id":"test-model"}]}"#;

/// A verb of the store under the issue's lock.
fn store_locked(w: &World, n: u64, args: &[&str]) {
    fs::create_dir_all(w.cycles_dir()).unwrap();
    let out = Command::new("flock")
        .args(["-w", "5"])
        .arg(w.cycles_dir().join(format!("{n}.lock")))
        .arg(w.home.join("libexec/store"))
        .args(args)
        .env("TEHAS_HOME", &w.home)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success(), "store {args:?}: {out:?}");
}

/// Issue 7 was parked on `reason` after a cycle `by` started at 02:00 ("-"
/// for by hand), its question unticked, and the endpoint answers again.
fn outage_parked(by: &str, reason: &str) -> World {
    let w = world();
    w.fx("ready.json", "[]");
    w.fx(
        "asking.json",
        r#"[{"number":7,"labels":[{"name":"needs-human-attention"},{"name":"tehas:question"}]}]"#,
    );
    w.comments(7, &park_body(&[" ", " "]), "", json!([]));
    w.mesh(Some(MESH));
    let s = epoch("2026-01-01T02:00:00Z");
    if by == "-" {
        w.t_started(7, Some(s), &["--by-hand"]);
    } else {
        w.t_approve(7, by, "label", s - 60, &[]);
        w.t_started(7, Some(s), &[]);
    }
    store_locked(
        &w,
        7,
        &[
            "cycle-end",
            "7",
            "--attempt",
            "1",
            "--status",
            "handoff",
            "--cap",
            "step-failed:adversarial",
            "--provider",
            "--parked",
        ],
    );
    w.t_park(7, reason, "4001", &[]);
    w.fx(
        "issue-7.json",
        r#"{"state":"OPEN","labels":[{"name":"needs-human-attention"},{"name":"tehas:question"}]}"#,
    );
    w
}

fn cycle(w: &World, filter: &str) -> String {
    jq(
        &format!("{filter} | tostring"),
        &w.row(&["cycle-show", "7"]).to_string(),
    )
    .trim_matches('"')
    .to_string()
}

fn park_is(w: &World, filter: &str) -> String {
    jq(filter, &w.park_of().to_string())
}

#[test]
fn a_cycle_a_provider_outage_parked_starts_again_once_without_a_tick_and_the_issue_is_told() {
    let w = outage_parked("alice", "provider-outage");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.calls("fake-start").trim_end(), "7");
    has(
        &w.gh(),
        "issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:question",
    );
    lacks(&w.gh(), "--remove-label needs-human-attention");
    has(&w.gh(), "Tehas starts this issue again on its own: its last cycle was parked by a provider outage, and the model endpoint answers again. It does this once.");
    has(
        &w.gh(),
        "authorized by @alice's start of the cycle a provider outage parked",
    );
    has(&r.out, "start #7 (requeued by alice)");
    lacks(&w.gh(), "api -X PATCH");
}

#[test]
fn the_park_is_used_up_and_the_start_is_marked_as_the_retry_for_its_cycle() {
    let w = outage_parked("alice", "provider-outage");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.park_of(), Value::Null);
    assert_eq!(
        w.row(&["question-known", "7", "--id", "4001"])["recorded"],
        json!(false)
    );
    assert_eq!(
        cycle(
            &w,
            r#""\(.cycle.attempt) \(.cycle.requeued) \(.cycle.started_by) \(.cycle.how)""#
        ),
        "2 true alice outage-retry"
    );
    assert_eq!(w.park_q(7), "none");
    // On record at the time the poll decided it, not at the time of the
    // approval it rests on, which started a cycle already.
    assert_eq!(w.approval(r#""\(.how) \(.by)""#, 7), "outage-retry alice");
    let at = w.row(&["approval-current", "7"])["current"]["at"]
        .as_i64()
        .unwrap();
    assert!(at > epoch("2026-01-01T02:00:00Z"), "{at}");
}

#[test]
fn a_start_a_person_asked_for_is_not_the_retry_and_says_who_asked() {
    let w = world();
    w.mesh(Some(MESH));
    w.t_ended(7, "bob", "outage-retry", &["--provider", "--parked"]);
    w.t_park(7, "provider-outage", "-", &[]);
    w.poll();
    assert_eq!(w.calls("fake-start").trim_end(), "7");
    assert_eq!(w.park_of(), Value::Null);
    assert_eq!(
        cycle(
            &w,
            r#""\(.cycle.attempt) \(.cycle.requeued) \(.cycle.started_by)""#
        ),
        "2 false alice"
    );
}

#[test]
fn an_outage_park_of_a_cycle_started_by_hand_asks_for_no_retry_nothing_starts_nothing_is_said() {
    let w = outage_parked("-", "provider-outage");
    let r = w.poll();
    assert_eq!(w.calls("fake-start"), "");
    lacks(&w.gh(), "issue edit");
    lacks(&r.out, "provider outage");
    assert_eq!(park_is(&w, "[.automatic, .retries]"), "[false,0]");
}

#[test]
fn an_issue_a_person_labelled_tehas_ready_is_left_to_that_label() {
    let w = outage_parked("alice", "provider-outage");
    w.fx(
        "issue-7.json",
        r#"{"state":"OPEN","labels":[{"name":"needs-human-attention"},{"name":"tehas:ready"}]}"#,
    );
    w.fx(
        "ready.json",
        r#"[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"needs-human-attention"}]}]"#,
    );
    let r = w.poll();
    assert_eq!(w.calls("fake-start").trim_end(), "7");
    has(&r.out, "start #7 (labelled by alice)");
    assert_eq!(w.park_of(), Value::Null);
    assert_eq!(cycle(&w, ".cycle.requeued"), "false");
}
