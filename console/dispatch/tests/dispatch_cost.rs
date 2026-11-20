//! Each poll records what it cost in the state store.

mod common;

use common::{has, lacks, root, World};
use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// What the last poll recorded, as poll-show answers now.
fn cost(w: &World) -> Value {
    let r = w.store_at(now(), &["poll-show"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    serde_json::from_str(&r.out).unwrap()
}

/// The idle polls of the last `since` seconds, as the health check reads
/// them, at `at`.
fn idle_at(w: &World, at: i64, since: &str) -> Value {
    let r = w.store_at(at, &["poll-idle", "--since", since]);
    assert_eq!(r.rc, 0, "{}", r.out);
    serde_json::from_str(&r.out).unwrap()
}

fn idle(w: &World, since: &str) -> Value {
    idle_at(w, now(), since)
}

/// The gh calls the stub recorded: the lines of a multi-line argument after
/// a call are not calls.
fn gh_calls(w: &World) -> u64 {
    w.gh()
        .lines()
        .filter(|l| {
            ["repo ", "api ", "issue ", "pr ", "label "]
                .iter()
                .any(|p| l.starts_with(p))
        })
        .count() as u64
}

fn log(w: &World) -> PathBuf {
    w.home.join("state/github-writes.log")
}

fn paused(w: &World) {
    w.fx("control.json", r#"{"labels":[{"name":"tehas:pause"}]}"#);
    w.fx(
        "events-1.json",
        r#"[{"event":"labeled","label":{"name":"tehas:pause"},"actor":{"login":"alice"}}]"#,
    );
}

#[test]
fn a_poll_records_its_wall_and_cpu_time_its_gh_calls_and_the_shims_writes() {
    let w = World::new("cost");
    fs::write(log(&w), "").unwrap();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
    let c = cost(&w);
    assert!(c["last"]["wall_ms"].as_f64().unwrap() > 0.0, "{c}");
    assert!(c["last"]["cpu_ms"].as_f64().unwrap() > 0.0, "{c}");
    assert_eq!(c["last"]["gh"].as_u64(), Some(gh_calls(&w)));
    let text = fs::read_to_string(log(&w)).unwrap();
    let writes = text.lines().count() as u64;
    assert!(writes > 0);
    assert_eq!(c["last"]["writes"].as_u64(), Some(writes));
    let edits = text
        .lines()
        .filter(|l| l.contains("\tissue edit\t"))
        .count() as u64;
    assert_eq!(c["last"]["kinds"]["edit"].as_u64(), Some(edits));
    assert_eq!(c["hour"]["polls"].as_u64(), Some(1));
    has(c["line"].as_str().unwrap(), "poll      last ");
    lacks(&r.out, "WARN: the cost of the poll");
    // One issue picked, so the poll's own row says one start, whatever the
    // late cycle's record says after the trap ran.
    assert_eq!(c["last"]["starts"].as_u64(), Some(1));
    assert_eq!(idle(&w, "3600")["polls"].as_u64(), Some(0));
}

#[test]
fn a_poll_that_starts_no_cycle_says_0_starts_and_is_one_of_the_idle_polls() {
    let w = World::new("cost");
    // No write log: the host has no GitHub App, so the idle polls have no
    // writes.
    let _ = fs::remove_file(log(&w));
    paused(&w);
    w.poll();
    assert_eq!(w.started(), "");
    assert_eq!(cost(&w)["last"]["starts"].as_u64(), Some(0));
    // No write figure with no write log: the figure is left out rather than
    // said as 0.
    let hour = idle(&w, "3600");
    assert_eq!(hour["polls"].as_u64(), Some(1));
    assert!(hour.get("writes").is_none(), "{hour}");
    assert_eq!(idle(&w, "60")["polls"].as_u64(), Some(1));
    // A window of no seconds holds nothing, and a window that stops before
    // the poll ended holds nothing either.
    assert_eq!(idle(&w, "0")["polls"].as_u64(), Some(0));
    assert_eq!(idle_at(&w, 1, "3600")["polls"].as_u64(), Some(0));
}

#[test]
fn without_a_write_log_the_writes_are_not_on_record_and_the_calls_still_are() {
    let w = World::new("cost");
    let _ = fs::remove_file(log(&w));
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    let c = cost(&w);
    assert!(c["last"]["writes"].is_null(), "{c}");
    assert_eq!(c["last"]["gh"].as_u64(), Some(gh_calls(&w)));
    // The row of a poll that started cycles is not an idle one.
    assert_eq!(idle(&w, "3600")["polls"].as_u64(), Some(0));
}

/// The shim of host/shim/gh in an installed home, beside the stub gh that
/// stays on PATH: the shim runs $TEHAS_HOME/bin/gh, a copy of the stub.
fn install_shim(w: &World) {
    let h = &w.home;
    for d in ["bin", "shim", "libexec"] {
        fs::create_dir_all(h.join(d)).unwrap();
    }
    let shim = h.join("shim/gh");
    fs::copy(root().join("host/shim/gh"), &shim).unwrap();
    fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).unwrap();
    let console = h.join("libexec/console");
    let _ = fs::remove_file(&console);
    std::os::unix::fs::symlink(w.console(), &console).unwrap();
    fs::copy(w.t().join("bin/gh"), h.join("bin/gh")).unwrap();
}

#[test]
fn the_writes_of_a_poll_count_a_graphql_mutation_and_not_its_queries() {
    let w = World::new("cost");
    let log = log(&w);
    fs::write(&log, "").unwrap();
    install_shim(&w);
    let shim = |args: &str| {
        let r = w.sh(&format!(
            "TEHAS_GH_TOKEN=ghs_t TEHAS_HOME=\"$T/home/tehas\" TEHAS_GITHUB_WRITES_LOG='{}' \"$T/home/tehas/shim/gh\" api graphql {args}",
            log.display()
        ));
        assert_eq!(r.rc, 0, "{}", r.out);
    };
    // The two ways the poll asks: the document of -f alone, and beside the
    // variables of -F.
    shim("-f 'query=query { viewer { login } }' -F owner=acme -F name=widget");
    shim(
        "-F owner=acme -F name=widget -f 'query=query($owner: String!, $name: String!) {
  repository(owner: $owner, name: $name) { pullRequests(states: OPEN, first: 100) { nodes { mutation } } } }'",
    );
    assert_eq!(fs::read_to_string(&log).unwrap(), "");
    // A mutation, named first and named after a name of its own.
    shim("-f 'query=mutation { addLabels(subjectId: 1) { clientMutationId } }'");
    shim("-f 'query=AddLabel mutation($n: Int!) { updateLabels(subjectId: $n) { x } }'");
    let text = fs::read_to_string(&log).unwrap();
    assert_eq!(text.lines().count(), 2, "{text}");
    let last = text.lines().last().unwrap();
    assert_eq!(last.split('\t').nth(1), Some("api POST"), "{last}");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert!(cost(&w)["last"]["writes"].as_u64().unwrap() >= 2);
}

#[test]
fn a_dry_run_records_nothing() {
    let w = World::new("cost");
    let r = w.dispatch(&["--dry-run"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert!(cost(&w)["last"].is_null());
    assert_eq!(idle(&w, "3600")["polls"].as_u64(), Some(0));
}

#[test]
fn a_poll_that_stops_early_records_its_cost_too() {
    let w = World::new("cost");
    paused(&w);
    w.poll();
    assert_eq!(w.started(), "");
    let c = cost(&w);
    assert!(c["last"]["gh"].as_u64().unwrap() > 0, "{c}");
    assert_eq!(c["last"]["starts"].as_u64(), Some(0));
}
