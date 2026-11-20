//! `tehas-dispatch poll` (src/poll.rs), run as `tehasbot dispatch` runs it once
//! its settings are loaded. Its frame: the dispatcher's lock, the checks
//! before any call, what it publishes and the end of every poll that held
//! the lock.

mod common;

use common::{has, lacks, root, Run, World};
use std::fs::{self, File, OpenOptions};
use std::process::Command;

/// The world, with the health a host runs built beside it; `$dispatch` runs
/// keep the fake one.
fn world() -> World {
    let w = World::new("poll");
    w.real_health();
    w.env_always("TEHAS_HEALTH_CMD", "fake-health");
    w
}

fn health(w: &World) -> String {
    w.t()
        .join("health-tree/libexec/health")
        .to_string_lossy()
        .into_owned()
}

/// The poll with the instance loaded and the real health named, as a host
/// runs it: `rpoll` of the shell.
fn rpoll(w: &World, args: &str) -> Run {
    w.sh(&format!(
        ". \"{}/lib/config.sh\" && tehas_load && TEHAS_HEALTH_CMD=\"{}\" \"{}\" poll {args}",
        root().display(),
        health(w),
        w.dispatcher().display()
    ))
}

/// The log without the time of each line.
fn untimed(out: &str) -> String {
    out.lines()
        .map(|l| match l.split_once("Z ") {
            Some((t, rest)) if t.chars().all(|c| c.is_ascii_digit() || "T:-".contains(c)) => rest,
            _ => l,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The dispatcher's lock, held by the test until the file is dropped.
fn hold_dispatch(w: &World) -> File {
    fs::create_dir_all(w.home.join("state")).unwrap();
    let f = OpenOptions::new()
        .append(true)
        .create(true)
        .open(w.home.join("state/dispatch.lock"))
        .unwrap();
    f.try_lock().unwrap();
    f
}

fn exists(w: &World, rel: &str) -> bool {
    w.home.join(rel).exists()
}

fn now() -> i64 {
    tehas_core::time::now()
}

/// The token program of a host of the GitHub App: it prints ghs_poll, and
/// writes a line to `runs` each time when one is named.
fn token_program(w: &World, runs: Option<&str>) {
    fs::create_dir_all(w.home.join("bin")).unwrap();
    fs::create_dir_all(w.home.join("state/github")).unwrap();
    w.home_file("state/github/token", "ghs_poll\n");
    w.home_file("state/github/token.expires", &format!("{}\n", now() + 3000));
    let count = runs
        .map(|r| format!("echo run >>'{r}'\n"))
        .unwrap_or_default();
    w.home_file(
        "bin/tehasbot",
        &format!(
            "#!/usr/bin/env bash\n[ \"$*\" = github-token ] || exit 64\n{count}printf \"%s\\n\" ghs_poll\n"
        ),
    );
    chmod_x(&w.home.join("bin/tehasbot"));
}

fn chmod_x(p: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(p, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn a_poll_that_finds_the_lock_held_changes_nothing() {
    let w = world();
    let lock = hold_dispatch(&w);
    let r = rpoll(&w, "");
    drop(lock);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.gh(), "");
    assert!(!exists(&w, "state/dispatch.taken"));
}

#[test]
fn a_lock_held_for_longer_than_half_an_hour_is_said_once() {
    let w = world();
    w.home_file("state/dispatch.taken", &format!("{}\n", now() - 4000));
    let lock = hold_dispatch(&w);
    let r = rpoll(&w, "");
    has(&r.out, "WARN: the dispatcher's lock has been held since");
    let r = rpoll(&w, "");
    drop(lock);
    lacks(&r.out, "WARN: the dispatcher's lock");
}

#[test]
fn a_store_that_is_not_installed_or_records_of_an_older_tehas_end_the_poll_with_78() {
    let w = world();
    fs::remove_file(w.home.join("libexec/store")).unwrap();
    let r = rpoll(&w, "");
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "ERROR: the state store is not installed (run install.sh)",
    );
    w.reset();
    w.home_file("state/cycles/7.approved.json", "");
    let r = rpoll(&w, "");
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, &format!("ERROR: records of an older tehas at {}/state/cycles/7.approved.json; this release does not import them: move them out of state/", w.home.display()));
    fs::remove_file(w.home.join("state/cycles/7.approved.json")).unwrap();
}

#[test]
fn a_planning_bound_the_poll_cannot_read_ends_it_with_78_before_the_lock_and_any_call() {
    // A poll started with no load reads both bounds itself; the load refuses
    // the same words first (test/config_test.sh).
    let w = world();
    let _ = fs::remove_file(w.home.join("state/dispatch.taken"));
    let loaded = |var: &str| {
        w.sh(&format!(
            ". \"{}/lib/config.sh\" && tehas_load >/dev/null 2>&1; TEHAS_HEALTH_CMD=\"{}\" {var} \"{}\" poll",
            root().display(),
            health(&w),
            w.dispatcher().display()
        ))
    };
    let r = loaded("TEHAS_PLAN_MAX_TURNS=two");
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "ERROR: TEHAS_PLAN_MAX_TURNS is not valid: expected a positive whole number",
    );
    assert_eq!(w.gh(), "");
    assert!(!exists(&w, "state/dispatch.taken"));
    let r = loaded("TEHAS_PLAN_TIMEOUT=7201");
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "ERROR: TEHAS_PLAN_TIMEOUT (7201) is more than 7200");
    assert_eq!(w.gh(), "");
}

#[test]
fn a_dry_run_logs_what_a_poll_would_do() {
    let w = world();
    w.fx("ready.json", "[]");
    let b = untimed(&w.dispatch(&["--dry-run"]).out);
    let r = rpoll(&w, "--dry-run");
    assert_eq!(untimed(&r.out), b);
    assert_eq!(r.rc, 0, "{}", r.out);
}

#[test]
fn a_poll_that_starts_a_cycle_journaled_its_children_without_the_polls_lock_its_end_recorded() {
    let w = world();
    let r = rpoll(&w, "");
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "start #7 (labelled by alice)");
    assert!(w.t().join("calls/fake-start").exists());
    assert!(!w.t().join("calls/start.fd8").exists());
    has(
        &w.gh(),
        "issue edit 7 --repo acme/widget --add-label tehas:running",
    );
    // Every write of the start was journaled and settled: none is left open.
    assert_eq!(w.row(&["journal-open"])["actions"], serde_json::json!([]));
    assert!(!w.row(&["poll-show"])["last"].is_null());
    assert!(w.t().join("calls/tidy.after-start").exists());
    assert!(w.dispatch_state()["time"].is_string());
}

#[test]
fn label_events_github_left_out_of_the_page_are_read_one_issue_at_a_time() {
    // GitHub's page can answer an empty timeline for an issue that has labels,
    // without an error. Read as it is, an allowed account's label has nobody
    // behind it, and the poll would take it off.
    let w = world();
    w.touch("page-drops-events");
    let r = rpoll(&w, "");
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "start #7 (labelled by alice)");
    lacks(&r.out, "'unknown'");
    has(
        &w.gh(),
        "api repos/acme/widget/issues/7/events?per_page=100 --paginate",
    );
}

#[test]
fn the_queues_page_asks_for_the_size_the_dropped_page_guard_rests_on() {
    // A page over GitHub's budget for nested items comes back with its
    // timelines cut short and no error (#928); the words of the call pin the
    // number.
    let w = world();
    let r = rpoll(&w, "");
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&w.gh(), "issues(states: OPEN, first: 25, after: $cursor)");
}

#[test]
fn the_page_gave_no_timelines_and_the_issue_a_cycle_holds_is_not_re_read() {
    // A running cycle's issue carries the bot's own tehas:running on every
    // poll and no rule reads that mark's timeline, so the poll must not spend
    // a paginated read on it.
    let w = world();
    w.touch("page-drops-events");
    w.fx(
        "events-7.json",
        r#"[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"}}]"#,
    );
    w.hold(9);
    let r = rpoll(&w, "");
    w.release();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&w.gh(), "issues/7/events?per_page=100 --paginate");
    lacks(&w.gh(), "issues/9/events?per_page=100 --paginate");
}

#[test]
fn a_poll_that_starts_a_cycle_logs_its_lines_line_for_line() {
    let w = world();
    let b = untimed(&w.poll().out);
    w.reset();
    let r = rpoll(&w, "");
    assert_eq!(untimed(&r.out), b);
}

#[test]
fn a_poll_takes_the_ask_for_it_and_a_dry_run_leaves_it() {
    let w = world();
    w.fx("ready.json", "[]");
    w.home_file("state/dispatch.again", "");
    rpoll(&w, "--dry-run");
    assert!(exists(&w, "state/dispatch.again"));
    rpoll(&w, "");
    assert!(!exists(&w, "state/dispatch.again"));
}

#[test]
fn a_lock_nothing_used_for_30_days_is_swept_and_said() {
    let w = world();
    w.fx("ready.json", "[]");
    let lock = w.cycles_dir().join("5.lock");
    w.home_file("state/cycles/5.lock", "");
    let st = Command::new("touch")
        .args(["-d", "40 days ago"])
        .arg(&lock)
        .status()
        .unwrap();
    assert!(st.success());
    let r = rpoll(&w, "--dry-run");
    has(&r.out, "would remove the lock of #5: unused for 30 days");
    assert!(lock.exists());
    let r = rpoll(&w, "");
    has(&r.out, "removed the lock of #5: unused for 30 days");
    assert!(!lock.exists());
}

#[test]
fn a_real_poll_writes_its_demand_with_its_time_and_its_cost_counts_its_start() {
    let w = world();
    rpoll(&w, "");
    let d = w.json_home("live/demand.json");
    assert!(d["time"].is_string(), "{d}");
    assert_eq!(d["waiting"], 1);
    assert_eq!(w.row(&["poll-show"])["last"]["starts"], 1);
}

#[test]
fn a_read_the_poll_cannot_make_ends_the_poll_with_ghs_own_words() {
    let w = world();
    // The read of the queue (src/read/run.rs) owes a 5xx one retry, so both
    // answers are armed and the poll ends on the second.
    w.gh_fail(1, 1, "HTTP 502: Bad Gateway");
    w.gh_fail(2, 1, "HTTP 502: Bad Gateway");
    // The host's shim neither retries nor waits a 5xx (test/gh_shim_test.sh),
    // and is told which gh to ask.
    w.env("TEHAS_GH_BIN", &w.t().join("bin/gh").to_string_lossy());
    w.env("TEHAS_GH_RETRY_WAIT", "0");
    // A host of the GitHub App: the poll takes a token before its first call.
    // Neither ghs_poll nor ghs_fresh<n> may reach its log.
    token_program(&w, None);
    w.env("TEHAS_GITHUB_APP_ID", "5452597");
    w.env("TEHAS_GITHUB_APP_INSTALLATION", "174742089");
    w.env("TEHAS_BOT_LOGIN", "tehas-app-bot[bot]");
    let r = rpoll(&w, "");
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "ERROR: gh api graphql failed: HTTP 502");
    // The outage was the call the case armed, and the read was made twice.
    let gh = w.gh();
    has(&gh, "api graphql");
    let first: Vec<&str> = gh.lines().next().unwrap_or("").split(' ').take(2).collect();
    assert_eq!(first.join(" "), "api graphql");
    assert_eq!(fs::read_to_string(w.t().join("gh-n")).unwrap().trim(), "2");
    lacks(&r.out, "ghs_poll");
    lacks(&r.out, "ghs_fresh");
    lacks(&r.out, "TEHAS_GH_TOKEN");
    // The failed read is on the host's record that health counts, once.
    let failures = w.read_home("state/github-failures");
    let logged = r
        .out
        .lines()
        .find_map(|l| l.find("ERROR: gh api graphql failed").map(|i| &l[i..]))
        .unwrap();
    let fields: Vec<&str> = failures
        .lines()
        .map(|l| l.split('\t').nth(1).unwrap_or(""))
        .collect();
    assert_eq!(fields, vec![logged], "{failures}");
    lacks(&failures, "ghs_");
    assert_eq!(w.started(), "");
    assert!(!w.t().join("calls/tidy.after-start").exists());
    assert!(!w.demand_file().exists());
}

#[test]
fn the_polls_token_reaches_the_real_tidy_status_and_health_the_token_program_runs_once() {
    let w = world();
    // The host's shim, its gh this run's stub, and a token program that
    // counts its runs. Only the poll itself asks for a token.
    fs::create_dir_all(w.home.join("shim")).unwrap();
    fs::create_dir_all(w.home.join("libexec")).unwrap();
    fs::copy(root().join("host/shim/gh"), w.home.join("shim/gh")).unwrap();
    fs::create_dir_all(w.home.join("bin")).unwrap();
    std::os::unix::fs::symlink(w.t().join("bin/gh"), w.home.join("bin/gh")).unwrap();
    let runs = w.t().join("token-runs");
    token_program(&w, Some(&runs.to_string_lossy()));
    let tidy = w.t().join("real-tidy");
    fs::write(
        &tidy,
        format!(
            "#!/usr/bin/env bash\nexec '{}' tidy run\n",
            w.console().display()
        ),
    )
    .unwrap();
    let status = w.t().join("health-tree/libexec/status");
    fs::write(
        &status,
        "#!/usr/bin/env bash\nexec \"$(dirname \"$0\")/../bin/tehasbot\" status\n",
    )
    .unwrap();
    chmod_x(&tidy);
    chmod_x(&status);
    w.slots(1, &["TEHAS_GITHUB_APP_ID=5452597"]);
    w.home_file("state/github/login", "tehas-app-bot[bot]\n");
    w.env("TEHAS_TIDY_CMD", &tidy.to_string_lossy());
    w.env("TEHAS_STATUS_CMD", &status.to_string_lossy());
    // The shim first on PATH, as host/env.sh puts it.
    let r = w.sh(&format!(
        "PATH={}/shim:$PATH; . \"{}/lib/config.sh\" && tehas_load && TEHAS_HEALTH_CMD=\"{}\" \"{}\" poll",
        w.home.display(),
        root().display(),
        health(&w),
        w.dispatcher().display()
    ));
    has(&r.out, "start #7 (labelled by alice)");
    has(&w.gh(), "is:closed");
    assert_eq!(fs::read_to_string(&runs).unwrap().lines().count(), 1);
}

#[test]
fn a_write_a_dead_poll_left_in_the_journal_is_settled_first_and_decided_again() {
    let w = world();
    let at = w.row(&["poll-begin"])["at"].to_string();
    let at = at.trim_matches('"');
    let rec = w.store(&[
        "journal-record",
        "--poll",
        at,
        "--kind",
        "label-add",
        "--issue",
        "7",
        "--args",
        r#"{"gh":["issue","edit","7","--repo","acme/widget","--add-label","tehas:running"]}"#,
        "--before",
        at,
    ]);
    assert_eq!(rec.rc, 0, "{}", rec.out);
    let r = rpoll(&w, "");
    has(&r.out, "journal: action 1 (label-add on #7");
    has(
        &r.out,
        "did not happen (it was never attempted); this poll decides it again",
    );
    has(&r.out, "start #7 (labelled by alice)");
    assert_eq!(w.row(&["journal-open"])["actions"], serde_json::json!([]));
}

/// The analytics program where install.sh puts it, a stub that records its
/// calls, and analytics on.
fn analytics_stub(w: &World) {
    w.stub("analytics", "");
    fs::create_dir_all(w.home.join("libexec")).unwrap();
    fs::rename(
        w.t().join("bin/analytics"),
        w.home.join("libexec/analytics"),
    )
    .unwrap();
    w.slots(1, &["TEHAS_ANALYTICS=on"]);
}

#[test]
fn a_poll_that_starts_nothing_runs_no_analytics_and_a_poll_that_starts_one_samples_it_and_prunes_nothing(
) {
    // An idle poll reads the analytics database not at all, and the end of a
    // poll that started something still samples the queue (#1769).
    let w = world();
    analytics_stub(&w);
    w.fx("control.json", r#"{"labels":[{"name":"tehas:pause"}]}"#);
    let r = rpoll(&w, "");
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    assert_eq!(w.calls("analytics"), "");
    w.reset();
    analytics_stub(&w);
    let r = rpoll(&w, "");
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
    // The events of the start, then the one sample of the queue.
    assert_eq!(
        w.calls("analytics"),
        "event 7 ready\nevent 7 started\nsample\n"
    );
}
