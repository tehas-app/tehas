//! `tehas update` as a run of the console: the checkout steps, what
//! install.sh is asked to do, the follow-ups an update runs, the lock, the
//! pi-rukas image and `--pending`. git runs for real against scratch
//! repositories; the cases with this repository's real install.sh check both
//! sides of the follow-up protocol against its producer.

#[path = "update/mod.rs"]
mod common;

use common::*;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Once;
use std::time::{Duration, Instant};
use tehas_core::testing::write_exe;

const UNITS: &str = "tehas-dispatch.timer tehas-energy.service tehas-top.service tehas-mcp.service tehas-mcp-fetch.socket tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-clean.timer";

/// The pi-rukas head of the image cases: forty b.
const IMAGE_HEAD: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

/// A host with the minimal instance and a checkout `behind` its remote.
fn host(name: &str, behind: u32, body: &str) -> Host {
    let mut h = Host::new(name);
    h.instance();
    h.platform(behind, body);
    h
}

/// The calls of the timeout stub with the bound `-k 60 <secs> ` cut.
fn bounded(h: &Host) -> String {
    h.calls("timeout")
        .lines()
        .map(|l| match l.strip_prefix("-k 60 ") {
            Some(rest) => match rest.split_once(' ') {
                Some((n, cmd)) if n.bytes().all(|b| b.is_ascii_digit()) => cmd,
                _ => l,
            },
            None => l,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn lock(h: &Host) -> String {
    s(&h.home.join("state/update.lock"))
}

/// libexec/update run with the lock held by the snippet's shell.
fn held_run(h: &Host, wait0: bool, args: &str) -> Run {
    fs::create_dir_all(h.home.join("state")).unwrap();
    let no_wait = if wait0 {
        "export TEHAS_UPDATE_LOCK_WAIT=0; "
    } else {
        ""
    };
    h.sh(&format!(
        "{no_wait}exec 8>>'{}'; flock -n 8 || exit 9; '{}' {args}",
        lock(h),
        h.update_script()
    ))
}

/// libexec/update with `console` as TEHAS_CONSOLE.
fn update_with(h: &Host, console: &Path) -> Run {
    h.sh(&format!(
        "export TEHAS_CONSOLE='{}'; exec '{}'",
        console.display(),
        h.update_script()
    ))
}

/// A console in `path` that answers `update api` with `api`, prints `finish`
/// first for `update finish`, and is the real one otherwise.
fn stand_in(h: &Host, path: &Path, api: &str, finish: &str) {
    write_exe(
        path,
        format!(
            "#!/usr/bin/env bash\ncase \"$1 $2\" in\n  'update api') {api} ;;\n  'update finish') echo '{finish}' ;;\nesac\nexec '{}' \"$@\"\n",
            h.console().display()
        ),
    )
    .unwrap();
}

/// The install body that puts console.next in the place of console.now.
fn new_console(h: &Host) -> String {
    format!(
        "cp '{}' '{}'",
        h.join("console.next").display(),
        h.join("console.now").display()
    )
}

/// tehas-cycle of this build: the real sandbox build asks it what the base
/// image can do. Built once when no build above the harness holds one.
fn cycle_bin() -> PathBuf {
    fn above() -> Option<PathBuf> {
        let exe = std::env::current_exe().unwrap();
        exe.ancestors()
            .skip(1)
            .map(|d| d.join("tehas-cycle"))
            .find(|p| p.is_file())
    }
    if let Some(p) = above() {
        return p;
    }
    static BUILT: Once = Once::new();
    BUILT.call_once(|| {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let target = std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target"));
        let out = Command::new("cargo")
            .args([
                "build",
                "--locked",
                "--offline",
                "--quiet",
                "--manifest-path",
            ])
            .arg(&manifest)
            .arg("--target-dir")
            .arg(&target)
            .args(["-p", "tehas-cycle"])
            .args((!cfg!(debug_assertions)).then_some("--release"))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "tehas-cycle could not be built: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    });
    above().expect("tehas-cycle above the test binary")
}

/// A host with this repository's real install.sh and the instance it accepts.
fn real(name: &str) -> Host {
    let mut h = Host::new(name);
    h.instance_models();
    h.env("CYCLE_BIN", &s(&cycle_bin()));
    h.real_platform();
    h
}

/// The host of one install before this run.
fn installed_once(h: &Host) {
    fs::create_dir_all(h.home.join("state")).unwrap();
    fs::write(h.home.join("state/first-install"), "").unwrap();
}

/// build.sh that records its arguments and exits `rc`, and a versions.env
/// that follows the release.
fn rukas_build(h: &Host, rc: i32) {
    write_exe(
        h.home.join("sandbox/build.sh"),
        format!(
            "#!/usr/bin/env bash\necho \"build $*\" >>\"{}\"\necho \"pi-rukas: stub\"\nexit {rc}\n",
            h.join("calls/build").display()
        ),
    )
    .unwrap();
    fs::write(h.home.join("versions.env"), "PI_RUKAS_FOLLOW=release\n").unwrap();
}

#[test]
fn missing_settings_stop_it_before_git_install_sh_docker_or_systemctl() {
    let mut h = Host::new("missing");
    h.instance();
    fs::remove_dir_all(h.instance_dir()).unwrap();
    h.platform(2, "");
    let r = h.update(&[]);
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "no instance.env in");
    assert_eq!(h.state(), format!("main {} none", h.head_sha()));
    assert_eq!(h.calls("install"), "");
}

#[test]
fn a_required_setting_left_commented_out_stops_it_before_git_and_install_sh() {
    let mut h = Host::new("commented");
    h.instance_with(&MINIMAL[..4]);
    h.platform(2, "");
    let before = h.state();
    let r = h.update(&[]);
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "tehas: PI_ENSEMBLE_SUBAGENT_MODEL is not set");
    assert_eq!(h.state(), before);
    assert_eq!(h.calls("install"), "");
}

#[test]
fn a_malformed_tehas_allowed_labelers_is_refused_before_the_checkout_moves() {
    let mut h = Host::new("labelers");
    let mut lines = MINIMAL.to_vec();
    lines.push("TEHAS_ALLOWED_LABELERS=\"alice  bob\"");
    h.instance_with(&lines);
    h.platform(2, "");
    let before = h.state();
    let r = h.update(&[]);
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "TEHAS_ALLOWED_LABELERS is not valid: expected GitHub logins separated by single spaces",
    );
    assert_eq!(h.state(), before);
    assert_eq!(h.calls("install"), "");
}

#[test]
fn check_lists_every_setting_tehas_1_0_renamed_or_removed_not_only_the_first() {
    let mut h = Host::new("renamed");
    let mut lines = MINIMAL.to_vec();
    lines.extend([
        "TEHAS_TICKET_CONCURRENCY=3",
        "TEHAS_OWNER=alice",
        "TEHAS_INDEXING=on",
    ]);
    h.instance_with(&lines);
    h.platform(2, "");
    let before = h.state();
    let r = h.update(&["--check"]);
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "tehas: TEHAS_INDEXING is no longer read: tehas 1.0 renamed it TEHAS_SITE_INDEXING",
    );
    has(
        &r.out,
        "tehas: TEHAS_TICKET_CONCURRENCY is no longer read: tehas 1.0 renamed it TEHAS_MAX_CYCLES",
    );
    has(
        &r.out,
        "tehas: TEHAS_OWNER is no longer read: tehas 1.0 renamed it TEHAS_ASSIGNEE",
    );
    assert_eq!(
        r.out
            .lines()
            .filter(|l| l.contains("TEHAS_OWNER is no longer read"))
            .count(),
        1
    );
    assert_eq!(h.state(), before);
    assert_eq!(h.calls("install"), "");
}

#[test]
fn an_unknown_argument_is_refused_before_anything_is_read() {
    let h = host("unknown", 0, "");
    let r = h.update(&["--dryrun"]);
    assert_eq!(r.rc, 64, "{}", r.out);
    has(&r.out, "tehas update: unknown argument: --dryrun");
    assert_eq!(h.calls("install"), "");
}

#[test]
fn help_with_another_argument_is_refused_naming_what_it_was_given() {
    let h = host("help-more", 0, "");
    let r = h.update(&["--check", "--help"]);
    assert_eq!(r.rc, 64, "{}", r.out);
    has(
        &r.out,
        "tehas update: --help takes no other argument, got --check --help",
    );
    assert_eq!(h.calls("install"), "");
}

#[test]
fn help_prints_the_usage_with_no_instance_and_no_checkout() {
    let mut h = Host::new("help");
    h.instance();
    fs::remove_dir_all(h.instance_dir()).unwrap();
    let r = h.update(&["--help"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "tehas update --check");
}

#[test]
fn no_platform_checkout_exit_78_saying_where_it_looked() {
    let mut h = Host::new("no-checkout");
    h.instance();
    let r = h.update(&[]);
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        &format!("no platform checkout at {}", h.checkout.display()),
    );
}

#[test]
fn a_checkout_that_is_a_git_worktree_is_a_checkout_too() {
    let h = host("worktree", 0, "echo \"installed: nothing\"");
    let main = h.join("main");
    fs::rename(&h.checkout, &main).unwrap();
    git(Some(&main), &["checkout", "-q", "-b", "hold"]);
    git(
        Some(&main),
        &["worktree", "add", "-q", &s(&h.checkout), "main"],
    );
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        &format!("checkout: main is up to date at {}", h.head_sha()),
    );
    assert_eq!(h.calls("install"), "install ");
}

#[test]
fn an_up_to_date_checkout_fetches_installs_and_runs_no_follow_up() {
    let h = host("current", 0, "echo \"tehas is up to date\"");
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        &format!("checkout: main is up to date at {}", h.head_sha()),
    );
    has(&r.out, "tehas is up to date");
    let head = h.head_sha();
    assert_eq!(h.state(), format!("main {head} {head}"));
    assert_eq!(h.calls("install"), "install ");
    assert_eq!(h.calls("build"), "");
    assert_eq!(h.calls("docker"), "");
    assert_eq!(h.calls("systemctl"), "");
}

#[test]
fn tehas_poll_secs_writes_the_timers_drop_in_once_and_the_default_removes_it() {
    let mut h = Host::new("poll");
    let dropin = h.join("home/.config/systemd/user/tehas-dispatch.timer.d/poll.conf");
    let mut lines = MINIMAL.to_vec();
    lines.push("TEHAS_POLL_SECS=300");
    h.instance_with(&lines);
    h.platform(0, "echo \"tehas is up to date\"");
    let restart = "--user daemon-reload\n--user is-active tehas-dispatch.timer\n--user restart tehas-dispatch.timer";
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "update: tehas-dispatch.timer polls every 300 s");
    let text = fs::read_to_string(&dropin).unwrap();
    assert_eq!(text.lines().filter(|l| l.ends_with("=300s")).count(), 3);
    assert_eq!(
        text.lines()
            .filter(|l| {
                l.strip_prefix("On")
                    .and_then(|r| r.strip_suffix("Sec="))
                    .is_some_and(|m| m.bytes().all(|b| b.is_ascii_alphabetic()))
            })
            .count(),
        3
    );
    assert_eq!(h.calls("systemctl"), restart);
    assert_eq!(h.owed_steps(), "");
    h.clear_calls();
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    lacks(&r.out, "polls every");
    assert_eq!(h.calls("systemctl"), "");
    h.instance();
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "update: tehas-dispatch.timer polls every 120 s again",
    );
    assert!(!dropin.exists());
    assert_eq!(h.calls("systemctl"), restart);
}

#[test]
fn a_tehas_poll_secs_out_of_bounds_stops_the_update_before_the_drop_in() {
    let mut h = Host::new("poll-bounds");
    let dropin = h.join("home/.config/systemd/user/tehas-dispatch.timer.d/poll.conf");
    let mut lines = MINIMAL.to_vec();
    lines.push("TEHAS_POLL_SECS=10");
    h.instance_with(&lines);
    h.platform(0, "echo \"tehas is up to date\"");
    let r = h.update(&[]);
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "TEHAS_POLL_SECS is not valid: expected a whole number of seconds from 30 to 3600",
    );
    assert!(!dropin.exists());
    assert_eq!(h.calls("systemctl"), "");
}

#[test]
fn a_second_update_while_one_runs_exits_1_and_calls_nothing() {
    let mut h = Host::new("second");
    h.instance();
    h.platform(2, &h.ask_all());
    let before = h.state();
    // A wait of 0 that never reached the command would wait the 10 s default.
    let start = Instant::now();
    let r = held_run(&h, true, "");
    assert_eq!(r.rc, 1, "{}", r.out);
    assert!(start.elapsed() < Duration::from_millis(5000));
    has(
        &r.out,
        &format!(
            "tehas update: another update is running ({}), still held after 0s; nothing was changed",
            lock(&h)
        ),
    );
    assert_eq!(h.state(), before);
    assert_eq!(h.calls("install"), "");
    assert_eq!(h.calls("build"), "");
    assert_eq!(h.calls("docker"), "");
    assert_eq!(h.calls("systemctl"), "");
}

#[test]
fn check_takes_no_lock() {
    let mut h = Host::new("check-lock");
    h.instance();
    h.platform(2, &h.ask_all());
    let r = held_run(&h, false, "--check");
    lacks(&r.out, "another update is running");
    assert_eq!(h.calls("install"), "install --check");
}

#[test]
fn an_update_that_finds_the_lock_held_waits_for_it_and_runs_when_it_is_let_go() {
    let h = host("wait", 0, "");
    fs::create_dir_all(h.home.join("state")).unwrap();
    let ready = h.join("holder-ready");
    let mut holder = Command::new("bash")
        .arg("-c")
        .arg(format!(
            "exec 8>>'{}'; flock -n 8; : >'{}'; sleep 1",
            lock(&h),
            ready.display()
        ))
        .spawn()
        .unwrap();
    while !ready.exists() {
        std::thread::sleep(Duration::from_millis(10));
    }
    let start = Instant::now();
    let r = h.sh(&format!(
        "export TEHAS_UPDATE_LOCK_WAIT=5; '{}'",
        h.update_script()
    ));
    holder.wait().unwrap();
    assert_eq!(r.rc, 0, "{}", r.out);
    // Under the 10 s default: the wait the case names is the one that ran.
    assert!(start.elapsed() < Duration::from_millis(9000));
    lacks(&r.out, "another update is running");
    assert_eq!(h.calls("install"), "install ");
}

#[test]
fn the_wait_rule_takes_0_through_300_is_10_unset_and_refuses_anything_else_with_78() {
    let h = Host::new("wait-rule");
    let ask = |v: Option<&str>| {
        let mut c = Command::new(h.console());
        c.args(["update", "wait", "update"])
            .env_remove("TEHAS_UPDATE_LOCK_WAIT");
        if let Some(v) = v {
            c.env("TEHAS_UPDATE_LOCK_WAIT", v);
        }
        run(c)
    };
    let mut said = vec![];
    for v in [Some("0"), Some("300"), None] {
        let r = ask(v);
        assert_eq!(r.rc, 0, "{}", r.out);
        said.push(r.out);
    }
    assert_eq!(said.join("\n"), "0\n300\n10");
    for v in ["301", "05", "soon"] {
        let r = ask(Some(v));
        assert_eq!(r.rc, 78, "{}", r.out);
        assert_eq!(
            r.out,
            "tehas: TEHAS_UPDATE_LOCK_WAIT is not valid: expected a whole number of seconds from 0 to 300"
        );
    }
}

#[test]
fn tehas_update_with_a_wait_it_cannot_read_stops_with_78_before_the_lock_and_the_install() {
    let h = host("wait-bad", 0, "");
    fs::create_dir_all(h.home.join("state")).unwrap();
    let r = h.sh(&format!(
        "export TEHAS_UPDATE_LOCK_WAIT=soon; '{}'",
        h.update_script()
    ));
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "TEHAS_UPDATE_LOCK_WAIT is not valid");
    assert_eq!(h.calls("install"), "");
}

#[test]
fn install_sh_is_handed_the_lock_the_update_holds() {
    let h = host(
        "handed",
        0,
        r#"if [ "/dev/fd/${TEHAS_UPDATE_LOCK_FD:-x}" -ef "$TEHAS_HOME/state/update.lock" ] && flock -n "$TEHAS_UPDATE_LOCK_FD"; then echo "the lock is ours"; fi
( exec 6>>"$TEHAS_HOME/state/update.lock"; flock -n 6 ) || echo "held against others""#,
    );
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "the lock is ours");
    has(&r.out, "held against others");
}

#[test]
fn install_sh_is_the_only_child_given_the_locks_descriptor() {
    let mut h = Host::new("only-child");
    h.instance();
    h.platform(1, &h.ask_all());
    let held = h.join("calls/held");
    h.stub(
        "timeout",
        &format!(
            "case \"$*\" in\n  *install.sh*) ;;\n  *) [ ! -e /proc/self/fd/7 ] || echo \"$*\" >>'{}' ;;\nesac\nexec {} \"$@\"",
            held.display(),
            which("timeout")
        ),
    );
    let console = h.join("console.held");
    write_exe(
        &console,
        format!(
            "#!/usr/bin/env bash\n[ ! -e /proc/self/fd/7 ] || echo \"console $1 $2\" >>'{}'\nexec '{}' \"$@\"\n",
            held.display(),
            h.console().display()
        ),
    )
    .unwrap();
    let r = update_with(&h, &console);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(h.owed_steps(), "");
    assert_eq!(h.calls("held"), "");
}

#[test]
fn a_child_the_update_starts_and_that_outlives_it_does_not_keep_the_lock() {
    let h = host("outlives", 0, "echo \"next: units tehas-dispatch.timer\"");
    h.stub(
        "systemctl",
        r#"if [ "$2" = is-active ]; then echo active; fi
sleep 30 >/dev/null 2>&1 &
echo $! >>"$T/calls/sleepers""#,
    );
    let r = h.update(&[]);
    let free = h.sh(&format!("exec 6>>'{}' && flock -n 6", lock(&h)));
    for pid in h.calls("sleepers").lines() {
        let _ = Command::new("kill").arg(pid).output();
    }
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "systemd: restarted tehas-dispatch.timer");
    assert_eq!(free.rc, 0, "a child of the update still holds {}", lock(&h));
}

#[test]
fn the_console_install_sh_installed_finishes_the_update_that_installed_it() {
    let mut h = Host::new("finish-new");
    h.instance();
    let now = h.join("console.now");
    stand_in(&h, &now, "echo 1; exit 0", "finished by the console before");
    stand_in(
        &h,
        &h.join("console.next"),
        "echo 1; exit 0",
        "finished by the console installed",
    );
    h.platform(
        0,
        &format!(
            "{}\necho 'next: units tehas-dispatch.timer'",
            new_console(&h)
        ),
    );
    let r = update_with(&h, &now);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "finished by the console installed");
    lacks(&r.out, "finished by the console before");
    has(&h.calls("systemctl"), "--user restart tehas-dispatch.timer");
    assert_eq!(h.owed_steps(), "");
}

#[test]
fn a_console_that_cannot_finish_an_update_leaves_it_to_the_one_install_sh_kept() {
    let mut h = Host::new("finish-kept");
    h.instance();
    let now = h.join("console.now");
    stand_in(&h, &now, "echo 1; exit 0", "finished by the console before");
    // What --to a version below this one installs: a console with no update verbs.
    stand_in(
        &h,
        &h.join("console.next"),
        "exit 2",
        "finished by the console installed",
    );
    h.platform(
        0,
        &format!(
            "cp '{}' '{}.prev'\n{}\necho 'next: units tehas-dispatch.timer'",
            now.display(),
            now.display(),
            new_console(&h)
        ),
    );
    let r = update_with(&h, &now);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "finished by the console before");
    lacks(&r.out, "finished by the console installed");
    has(&h.calls("systemctl"), "--user restart tehas-dispatch.timer");
}

#[test]
fn with_no_console_that_can_finish_what_install_sh_did_stays_owed_and_the_run_is_red() {
    let mut h = Host::new("finish-none");
    h.instance();
    let now = h.join("console.now");
    stand_in(&h, &now, "echo 1; exit 0", "finished by the console before");
    stand_in(
        &h,
        &h.join("console.next"),
        "exit 2",
        "finished by the console installed",
    );
    h.platform(
        0,
        &format!(
            "{}\n\"$TEHAS_CONSOLE\" update steps record sandbox/Dockerfile\necho 'installed: libexec/console'\necho 'next: sandbox'",
            new_console(&h)
        ),
    );
    let r = update_with(&h, &now);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "installed: libexec/console");
    has(
        &r.out,
        &format!(
            "neither {} nor {}.prev can finish this update",
            now.display(),
            now.display()
        ),
    );
    assert_eq!(h.owed_steps(), "sandbox");
    assert_eq!(h.calls("build"), "");
}

#[test]
fn a_later_line_of_install_sh_is_printed_and_is_no_step() {
    let h = host(
        "later",
        0,
        "echo \"installed: libexec/update\"\necho \"later: ./install.sh again when the running cycle has ended\"",
    );
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "later: ./install.sh again when the running cycle has ended",
    );
    lacks(&r.out, "WARN");
    assert_eq!(h.owed_steps(), "");
}

#[test]
fn a_fast_forward_runs_the_follow_ups_install_sh_asked_for() {
    let mut h = Host::new("forward");
    h.instance();
    h.platform(1, &h.ask_all());
    let before = h.head_sha();
    let th = s(&h.home);
    let co = s(&h.checkout);
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        &format!(
            "checkout: fast-forwarded {before} -> {} (1 commits)",
            h.head_sha()
        ),
    );
    has(&r.out, "installed: libexec/update");
    // In the order install.sh printed them, and not echoed back as work.
    assert_eq!(
        step_order(&r.out),
        "systemd: restarted\nsandbox: building\ndocs: updating\nedge: updating"
    );
    lacks(&r.out, "next:");
    assert_eq!(h.calls("build"), "build");
    let docker = format!(
        "ps --quiet --filter label=com.docker.compose.project=tehas-docs
compose -f {th}/edge/docs.compose.yaml up -d --force-recreate
ps --quiet --filter label=com.docker.compose.project=tehas-edge
compose -f {th}/edge/router.compose.yaml up -d --force-recreate"
    );
    assert_eq!(h.calls("docker"), docker);
    let active: String = UNITS
        .split(' ')
        .map(|u| format!("--user is-active {u}\n"))
        .collect();
    let systemctl = format!("--user daemon-reload\n{active}--user restart {UNITS}");
    assert_eq!(h.calls("systemctl"), systemctl);
    // Every call outside the command went through a timeout, install.sh included.
    let sys: Vec<String> = systemctl
        .lines()
        .map(|l| format!("systemctl {l}"))
        .collect();
    let dock: Vec<String> = docker.lines().map(|l| format!("docker {l}")).collect();
    assert_eq!(
        bounded(&h),
        format!(
            "git -C {co} fetch --prune origin\n{co}/install.sh\n{}\n{th}/sandbox/build.sh\n{}",
            sys.join("\n"),
            dock.join("\n")
        )
    );
    assert_eq!(h.owed_steps(), "");
}

#[test]
fn a_container_that_is_not_running_is_left_alone_a_unit_that_is_not_active_too() {
    let mut h = Host::new("left-alone");
    h.instance();
    h.platform(1, &h.ask_all());
    h.docker_up_docs();
    h.stub(
        "systemctl",
        r#"if [ "$2" = is-active ]; then
  case "$3" in *.timer ) echo active ;; * ) echo inactive; exit 3 ;; esac
fi"#,
    );
    let th = s(&h.home);
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "docs: updating the container");
    has(&r.out, "edge: not running on this host, left alone");
    assert_eq!(
        h.calls("docker"),
        format!(
            "ps --quiet --filter label=com.docker.compose.project=tehas-docs
compose -f {th}/edge/docs.compose.yaml up -d --force-recreate
ps --quiet --filter label=com.docker.compose.project=tehas-edge"
        )
    );
    let active: String = UNITS
        .split(' ')
        .map(|u| format!("--user is-active {u}\n"))
        .collect();
    assert_eq!(
        h.calls("systemctl"),
        format!("--user daemon-reload\n{active}--user restart tehas-dispatch.timer tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-clean.timer")
    );
    assert_eq!(h.owed_steps(), "");
}

#[test]
fn every_state_of_systemds_active_family_restarts_the_unit() {
    let mut h = Host::new("activating");
    h.instance();
    h.platform(1, &h.ask_all());
    h.stub(
        "systemctl",
        r#"if [ "$2" = is-active ]; then
  case "$3" in
    tehas-dispatch.timer) echo activating ;;
    tehas-energy.service) echo reloading ;;
    *) echo active ;;
  esac
fi"#,
    );
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, &format!("systemd: restarted {UNITS}"));
    lacks(&r.out, "no unit was restarted");
}

#[test]
fn a_unit_systemd_does_not_know_is_an_answer_and_leaves_that_unit_alone() {
    let mut h = Host::new("unknown-unit");
    h.instance();
    h.platform(1, &h.ask_all());
    h.stub(
        "systemctl",
        r#"if [ "$2" = is-active ]; then
  case "$3" in tehas-dispatch.timer ) echo unknown; exit 3 ;; * ) echo active ;; esac
fi"#,
    );
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "systemd: restarted tehas-energy.service tehas-top.service tehas-mcp.service tehas-mcp-fetch.socket tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-clean.timer",
    );
    lacks(&r.out, "no unit was restarted");
}

#[test]
fn a_docker_query_that_fails_stops_the_update_instead_of_reading_as_not_running() {
    let mut h = Host::new("docker-fails");
    h.instance();
    h.platform(1, &h.ask_all());
    h.docker_no_answer();
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "docs: docker ps failed; the step is not done");
    has(&r.out, "could not connect to the Docker daemon");
    lacks(&r.out, "not running on this host");
    // The units and the sandbox build ran; the edge renewal was never reached.
    assert_eq!(
        h.calls("docker"),
        "ps --quiet --filter label=com.docker.compose.project=tehas-docs"
    );
    assert_eq!(h.calls("build"), "build");
    has(&r.out, "systemd: restarted tehas-dispatch.timer");
    assert_eq!(
        h.owed_steps(),
        "docs --force-recreate\nedge --force-recreate"
    );
}

#[test]
fn a_systemd_query_that_fails_stops_the_update_before_any_other_step() {
    let mut h = Host::new("systemd-fails");
    h.instance();
    h.platform(1, &h.ask_all());
    h.stub(
        "systemctl",
        r#"if [ "$2" = is-active ]; then echo "Failed to connect to bus: No such file or directory" >&2; exit 1; fi"#,
    );
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "systemd: systemctl is-active gave no answer for tehas-dispatch.timer; no unit was restarted",
    );
    has(&r.out, "Failed to connect to bus");
    lacks(&r.out, "none of them runs on this host");
    assert_eq!(h.calls("docker"), "");
    assert_eq!(h.calls("build"), "");
    assert_eq!(
        h.owed_steps(),
        format!("units {UNITS}\nsandbox\ndocs --force-recreate\nedge --force-recreate")
    );
}

#[test]
fn a_systemctl_that_prints_an_error_and_exits_0_is_not_read_as_active() {
    let mut h = Host::new("systemd-noise");
    h.instance();
    h.platform(1, &h.ask_all());
    h.systemctl_no_answer();
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "systemd: systemctl is-active gave no answer for tehas-dispatch.timer; no unit was restarted",
    );
    has(&r.out, "systemd is not running in this container");
    lacks(&r.out, "systemd: restarted");
    lacks(&r.out, "none of them runs on this host");
    assert_eq!(
        h.calls("systemctl"),
        "--user daemon-reload\n--user is-active tehas-dispatch.timer"
    );
}

#[test]
fn a_call_that_hits_its_timeout_fails_saying_so() {
    let mut h = Host::new("timeout");
    h.instance();
    h.platform(1, &h.ask_all());
    h.stub("timeout", "echo \"timed out\"; exit 124");
    let before = h.state();
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "could not fetch from origin; nothing was changed");
    has(&r.out, "the fetch gave no answer in 120s");
    assert_eq!(h.state(), before);
    assert_eq!(h.calls("install"), "");
}

#[test]
fn a_step_that_fails_is_recorded_and_the_next_run_finishes_it() {
    let mut h = Host::new("recorded");
    h.instance();
    h.platform(1, &h.ask_all());
    h.docker_no_answer();
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "docs: docker ps failed; the step is not done");
    has(
        &r.out,
        &format!(
            "2 of 4 follow-up(s) are not done, recorded in {}",
            h.owed.display()
        ),
    );
    assert_eq!(
        h.owed_steps(),
        "docs --force-recreate\nedge --force-recreate"
    );
    // install.sh's one-shot list was spent by the run that copied the files.
    h.docker_up();
    h.push_install("echo \"tehas is up to date\"");
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        &format!(
            "follow-up(s) an earlier run did not finish (see {})",
            h.owed.display()
        ),
    );
    has(&r.out, "next: docs");
    has(&r.out, "docs: updating the container");
    has(&r.out, "edge: updating the container");
    // The steps that already ran are not run again.
    assert_eq!(h.calls("build"), "build");
    assert_eq!(h.owed_steps(), "");
}

#[test]
fn the_real_install_sh_through_tehas_update_runs_its_follow_ups_in_its_order() {
    let h = real("real");
    // A host of one install before, or install.sh makes the labels itself.
    installed_once(&h);
    let th = s(&h.home);
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, &format!("installed: {th}/libexec/update"));
    has(&r.out, &format!("systemd: restarted {UNITS}"));
    has(&r.out, "docs: updating the container");
    has(&r.out, "edge: updating the container");
    assert_eq!(
        step_order(&r.out),
        "systemd: restarted\nsandbox: building\ndocs: updating\nedge: updating\nlabels: making"
    );
    lacks(&r.out, "next:");
    assert_eq!(h.owed_steps(), "");
    // Every label of the table is made in the instance's repository, by gh.
    let gh = h.calls("gh");
    let made: BTreeSet<&str> = gh
        .lines()
        .filter_map(|l| l.strip_prefix("label create "))
        .map(|r| r.split(' ').next().unwrap())
        .collect();
    let table = fs::read_to_string(root().join("host/labels.tsv")).unwrap();
    let names: BTreeSet<&str> = table
        .lines()
        .map(|l| l.split('\t').next().unwrap())
        .collect();
    assert_eq!(made.len(), names.len());
    let repos: BTreeSet<&str> = gh
        .lines()
        .filter_map(|l| l.strip_prefix("label create "))
        .filter_map(|r| r.split_once(' '))
        .filter_map(|(_, rest)| rest.strip_prefix("--repo "))
        .map(|r| r.split(' ').next().unwrap())
        .collect();
    assert_eq!(
        repos.into_iter().collect::<Vec<_>>().join("\n"),
        "acme/widget"
    );
    // Nothing of the host's real /etc or /usr/local/bin was read.
    lacks(&r.out, "Login:");
}

#[test]
fn changed_unit_files_are_not_reported_as_applied_when_systemd_did_not_reload() {
    let h = real("reload");
    h.stub(
        "systemctl",
        r#"if [ "$2" = daemon-reload ]; then echo "Failed to connect to bus: No such file or directory" >&2; exit 1; fi
if [ "$2" = is-active ]; then echo active; fi"#,
    );
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "systemd: systemctl --user daemon-reload did not run; the changed unit files are not in effect, no unit was restarted, and the step is not done",
    );
    has(&r.out, "Failed to connect to bus");
    lacks(&r.out, "systemd: restarted");
    lacks(&r.out, "docs: updating the container");
    assert_eq!(
        h.owed_steps(),
        format!("{}\nlabels", h.next_lines().join("\n"))
    );
    // The next run, with systemd answering, finishes what was recorded.
    h.systemctl_active();
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        &format!(
            "follow-up(s) an earlier run did not finish (see {})",
            h.owed.display()
        ),
    );
    has(&r.out, &format!("systemd: restarted {UNITS}"));
    has(&r.out, "docs: updating the container");
    assert_eq!(h.owed_steps(), "");
}

#[test]
fn tehas_update_prints_install_shs_later_line_for_a_stale_login_copy_and_runs_nothing_for_it() {
    let h = real("login");
    h.login_setup();
    h.login_age();
    let before = h.login_state();
    installed_once(&h);
    let dev_bin = s(&h.dev_bin);
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "later: sudo <root-owned copy>/bootstrap.sh dev boss",
    );
    lacks(
        &r.out,
        &format!("sudo {}/bootstrap.sh", h.checkout.display()),
    );
    has(
        &r.out,
        &format!("{dev_bin}/tehas is not this checkout's host/login/tehas"),
    );
    assert_eq!(h.calls("sudo"), "");
    assert_eq!(h.login_state(), before);
    // --check says it too, as install.sh printed it.
    let r = h.update(&["--check"]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "Login:        NOT CURRENT");
    has(&r.out, &format!("differs: {dev_bin}/tehas"));
    has(&r.out, "sudo <root-owned copy>/bootstrap.sh dev boss");
    assert_eq!(h.calls("sudo"), "");
    assert_eq!(h.login_state(), before);
}

#[test]
fn a_checkout_that_no_longer_has_the_login_source_says_so_not_that_the_host_differs() {
    let h = real("login-gone");
    h.login_setup();
    h.login_age();
    let before = h.login_state();
    h.push_delete("host/login/tehas");
    let dev_bin = s(&h.dev_bin);
    let missing = format!(
        "{}/host/login/tehas is missing from this checkout",
        h.checkout.display()
    );
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, &missing);
    has(&r.out, "no follow-up was run");
    lacks(&r.out, &format!("differs: {dev_bin}/tehas"));
    lacks(&r.out, "sudo <root-owned copy>/bootstrap.sh dev boss");
    assert_eq!(h.owed_steps(), "");
    assert_eq!(h.login_state(), before);
    let r = h.update(&["--check"]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, &missing);
    lacks(&r.out, "Login:");
    lacks(&r.out, &format!("differs: {dev_bin}/tehas"));
}

#[test]
fn a_step_line_no_version_here_knows_stays_owed_until_something_knows_it() {
    let h = host("unknown-step", 0, "echo \"next: renew-dashboard --now\"");
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "WARN: 1 step line(s) name no step this command knows",
    );
    assert_eq!(h.owed_steps(), "renew-dashboard --now");
    h.push_install("echo \"tehas is up to date\"");
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "WARN: 1 step line(s) name no step this command knows",
    );
    has(&r.out, "next: renew-dashboard --now");
    assert_eq!(h.owed_steps(), "renew-dashboard --now");
}

#[test]
fn steps_an_install_sh_that_died_partway_recorded_are_run_by_the_next_one() {
    let h = host(
        "died",
        1,
        r#"echo "installed: libexec/dispatch"
"$TEHAS_CONSOLE" update steps record sandbox/Dockerfile edge/docs.Caddyfile
echo "tehas: /x is missing from this checkout" >&2
exit 1"#,
    );
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "install: install.sh exited 1; no follow-up was run");
    assert_eq!(h.calls("build"), "");
    assert_eq!(h.owed_steps(), "sandbox\ndocs --force-recreate");
    h.push_install("echo \"tehas is up to date\"");
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "sandbox: building the image");
    has(&r.out, "docs: updating the container");
    assert_eq!(h.owed_steps(), "");
}

/// A host whose install.sh never answers, and a timeout that answers 124 for it.
fn install_hangs(name: &str) -> Host {
    let h = host(name, 0, "sleep 3600");
    h.stub(
        "timeout",
        &format!(
            "case \"$*\" in\n  *install.sh*) echo 'no answer'; exit 124 ;;\n  *) exec {} \"$@\" ;;\nesac",
            which("timeout")
        ),
    );
    h
}

#[test]
fn an_install_sh_that_stops_answering_is_stopped_at_its_limit() {
    let h = install_hangs("install-hangs");
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "install: install.sh gave no answer in 1800s and was stopped",
    );
    assert_eq!(h.owed_steps(), "");
}

#[test]
fn an_install_sh_check_that_stops_answering_claims_no_copies() {
    let h = install_hangs("check-hangs");
    let before = h.head_sha();
    let r = h.update(&["--check"]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "install: install.sh --check gave no answer in 1800s and was stopped; it changes nothing, so nothing was installed",
    );
    lacks(&r.out, "what it had copied");
    assert_eq!(h.head_sha(), before);
}

#[test]
fn a_step_owed_from_an_earlier_run_runs_where_its_id_says_it_runs() {
    let h = host("owed-order", 1, "echo \"next: units tehas-dispatch.timer\"");
    h.record(&["docs --force-recreate"]);
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    // systemd first, whatever order the two were recorded in.
    assert_eq!(step_order(&r.out), "systemd: restarted\ndocs: updating");
    assert_eq!(h.owed_steps(), "");
}

#[test]
fn an_owed_units_line_from_before_a_unit_was_retired_restarts_the_current_units_and_leaves_nothing_owed(
) {
    let h = host("retired", 1, "echo \"tehas is up to date\"");
    // What the version before wrote, the retired timers among them.
    h.record(&["units tehas-dispatch.timer tehas-refresh.timer tehas-energy.service tehas-top.service tehas-mcp.service tehas-mcp-fetch.socket tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer tehas-review.timer tehas-clean.timer"]);
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "systemd: restarted tehas-dispatch.timer tehas-energy.service tehas-top.service tehas-mcp.service tehas-mcp-fetch.socket tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-review.timer tehas-clean.timer",
    );
    lacks(&r.out, "tehas-refresh.timer");
    lacks(&r.out, "tehas-improve.timer");
    assert_eq!(h.owed_steps(), "");
}

#[test]
fn the_compose_files_pin_the_project_names_the_liveness_check_filters_on() {
    for (file, name) in [
        ("docs.compose.yaml", "tehas-docs"),
        ("router.compose.yaml", "tehas-edge"),
    ] {
        has(
            &fs::read_to_string(root().join("host/edge").join(file)).unwrap(),
            &format!("name: {name}"),
        );
    }
}

#[test]
fn an_up_to_date_checkout_still_moves_the_image_to_the_pi_rukas_commit_it_follows() {
    let h = host("rukas-moves", 0, "");
    rukas_build(&h, 0);
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(h.calls("build"), "build --if-moved");
    has(
        &h.calls("timeout"),
        &format!("{}/sandbox/build.sh --if-moved", h.home.display()),
    );
}

#[test]
fn a_run_that_built_the_sandbox_image_does_not_look_at_the_head_again() {
    let h = host("rukas-built", 1, "echo \"next: sandbox\"");
    rukas_build(&h, 0);
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(h.calls("build"), "build ");
}

#[test]
fn an_image_that_cannot_be_moved_fails_the_update() {
    let h = host("rukas-fails", 0, "");
    rukas_build(&h, 1);
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "pi-rukas: the image was not moved to the commit versions.env follows",
    );
}

#[test]
fn a_build_that_ignores_its_term_is_killed_and_the_update_still_answers() {
    let h = host("rukas-term", 0, "");
    rukas_build(&h, 0);
    write_exe(
        h.home.join("sandbox/build.sh"),
        "#!/usr/bin/env bash\ntrap '' TERM\nsleep 30\n",
    )
    .unwrap();
    // The build's limit cut to a second and its grace to one more.
    let rt = which("timeout");
    h.stub(
        "timeout",
        &format!(
            "case \"$*\" in\n  *build.sh*) if [ \"$1\" = -k ]; then shift 3; exec {rt} -k 1 1 \"$@\"; fi\n    shift; exec {rt} 1 \"$@\" ;;\n  *) exec {rt} \"$@\" ;;\nesac"
        ),
    );
    let start = Instant::now();
    let r = h.update(&[]);
    assert!(start.elapsed() < Duration::from_millis(15000));
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &h.calls("timeout"),
        &format!(
            "-k 60 1800 {}/sandbox/build.sh --if-moved",
            h.home.display()
        ),
    );
    has(
        &r.out,
        "pi-rukas: the image was not moved to the commit versions.env follows",
    );
}

#[test]
fn check_says_the_commit_moved_and_builds_nothing() {
    let h = host("rukas-check", 0, "");
    rukas_build(&h, 3);
    let r = h.update(&["--check"]);
    assert_eq!(r.rc, 3, "{}", r.out);
    assert_eq!(h.calls("build"), "build --check");
    has(&r.out, "pi-rukas: stub");
}

#[test]
fn check_whose_build_check_failed_exits_1_and_78_for_its_settings() {
    for (build, want) in [(1, 1), (124, 1), (78, 78)] {
        let h = host("rukas-check-fails", 0, "");
        rukas_build(&h, build);
        let r = h.update(&["--check"]);
        assert_eq!(r.rc, want, "build {build}: {}", r.out);
    }
}

#[test]
fn a_versions_env_with_no_pi_rukas_follow_leaves_the_image_alone() {
    let h = host("rukas-commit", 0, "");
    fs::write(
        h.home.join("versions.env"),
        "PI_RUKAS_COMMIT=0123456789abcdef0123456789abcdef01234567\n",
    )
    .unwrap();
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(h.calls("build"), "");
}

#[test]
fn an_image_past_the_hosts_clone_asks_for_bootstrap_sh_user_on_a_later_line() {
    let h = host("rukas-past", 0, "");
    rukas_build(&h, 0);
    h.rukas_clone(IMAGE_HEAD);
    h.rukas_image(IMAGE_HEAD);
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        &format!(
            "later: {}/bootstrap.sh user   (pi-rukas moved to {}",
            h.checkout.display(),
            &IMAGE_HEAD[..7]
        ),
    );
    has(&r.out, "until it runs, --check says Dependencies: NOT OK)");
    // The image step still ran, and this line is not a step.
    assert_eq!(h.calls("build"), "build --if-moved");
    assert_eq!(h.owed_steps(), "");
}

#[test]
fn a_clone_on_the_images_commit_asks_for_nothing() {
    let h = host("rukas-same", 0, "");
    rukas_build(&h, 0);
    let head = h.rukas_clone(IMAGE_HEAD);
    h.rukas_image(&head);
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        head,
        git(Some(&h.home.join("repos/pi-rukas")), &["rev-parse", "HEAD"])
    );
    lacks(&r.out, "pi-rukas moved to");
    lacks(&r.out, "bootstrap.sh user");
}

#[test]
fn a_host_with_no_clone_and_one_with_no_label_on_its_image_say_nothing_and_end_0() {
    for (clone, image) in [("none", IMAGE_HEAD), (IMAGE_HEAD, "none")] {
        let h = host("rukas-none", 0, "");
        rukas_build(&h, 0);
        h.rukas_clone(clone);
        h.rukas_image(image);
        let r = h.update(&[]);
        assert_eq!(r.rc, 0, "{}", r.out);
        lacks(&r.out, "pi-rukas moved to");
        lacks(&r.out, "bootstrap.sh user");
    }
}

#[test]
fn the_line_of_a_run_that_copied_versions_env_is_not_printed_twice() {
    let mut h = Host::new("rukas-once");
    h.instance();
    let body = format!(
        "echo 'later: {}/current/bootstrap.sh user   (versions.env changed; until it runs, --check says Dependencies: NOT OK)'",
        h.home.display()
    );
    h.platform(0, &body);
    rukas_build(&h, 0);
    h.rukas_clone(IMAGE_HEAD);
    h.rukas_image(IMAGE_HEAD);
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        r.out
            .lines()
            .filter(|l| l.contains("bootstrap.sh user"))
            .count(),
        1
    );
    has(&r.out, "versions.env changed");
    lacks(&r.out, "pi-rukas moved to");
}

#[test]
fn pending_runs_the_steps_the_record_lists_in_step_order_and_clears_it() {
    let h = host(
        "pending",
        0,
        "echo \"install.sh must not run in this case\"",
    );
    h.record(&["docs --force-recreate", "sandbox"]);
    let th = s(&h.home);
    let r = h.update(&["--pending"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        &format!(
            "2 follow-up(s) an earlier run did not finish (see {})",
            h.owed.display()
        ),
    );
    has(&r.out, "next: docs --force-recreate");
    assert_eq!(step_order(&r.out), "sandbox: building\ndocs: updating");
    assert_eq!(h.calls("build"), "build");
    assert_eq!(
        h.calls("docker"),
        format!(
            "ps --quiet --filter label=com.docker.compose.project=tehas-docs
compose -f {th}/edge/docs.compose.yaml up -d --force-recreate"
        )
    );
    assert_eq!(h.owed_steps(), "");
    // Every call went through a timeout, and none was git's.
    assert_eq!(
        bounded(&h),
        format!(
            "{th}/sandbox/build.sh
docker ps --quiet --filter label=com.docker.compose.project=tehas-docs
docker compose -f {th}/edge/docs.compose.yaml up -d --force-recreate"
        )
    );
}

#[test]
fn pending_runs_with_no_checkout_and_no_current_and_calls_no_git() {
    let h = host("pending-no-git", 0, "");
    fs::remove_dir_all(&h.checkout).unwrap();
    h.stub(
        "git",
        "echo \"git: a --pending run must not call this\" >&2; exit 128",
    );
    h.record(&["docs --force-recreate", "sandbox"]);
    let r = h.update(&["--pending"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(h.calls("git"), "");
    has(&r.out, "sandbox: building the image");
    has(&r.out, "docs: updating the container");
    assert_eq!(h.owed_steps(), "");
}

#[test]
fn pending_runs_no_install_sh_and_reads_no_release() {
    let h = host("pending-no-install", 0, "echo \"install.sh ran\"");
    h.stub("git", "");
    h.record(&["sandbox"]);
    let r = h.update(&["--pending"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(h.calls("install"), "");
    lacks(&r.out, "install.sh ran");
    assert_eq!(h.calls("build"), "build");
}

#[test]
fn a_checkout_at_a_detached_head_does_not_stop_pending() {
    let h = host("detached", 0, "");
    git(Some(&h.checkout), &["checkout", "-q", "--detach", "HEAD"]);
    assert!(
        git_try(
            Some(&h.checkout),
            &["symbolic-ref", "--quiet", "--short", "HEAD"]
        )
        .is_none(),
        "the fixture left the checkout on a branch"
    );
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        &format!(
            "{} is not on a branch (detached HEAD); nothing was changed",
            h.checkout.display()
        ),
    );
    h.record(&["docs --force-recreate", "sandbox"]);
    let r = h.update(&["--pending"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    lacks(&r.out, "not on a branch");
    has(&r.out, "docs: updating the container");
    assert_eq!(h.owed_steps(), "");
}

#[test]
fn pending_with_an_empty_record_says_nothing_is_owed() {
    let h = host("pending-empty", 0, "");
    let r = h.update(&["--pending"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "update: nothing is owed");
    assert_eq!(h.calls("build"), "");
    assert_eq!(h.calls("docker"), "");
    assert_eq!(h.calls("systemctl"), "");
    assert_eq!(h.calls("install"), "");
}

#[test]
fn pending_with_a_record_of_a_step_no_version_here_knows_is_red_not_empty() {
    let h = host("pending-unknown", 0, "");
    h.record(&["renew-dashboard --now"]);
    let r = h.update(&["--pending"]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "WARN: 1 step line(s) name no step this command knows",
    );
    lacks(&r.out, "nothing is owed");
    assert_eq!(h.owed_steps(), "renew-dashboard --now");
}

#[test]
fn pending_takes_the_update_lock_as_a_real_run_does() {
    let h = host("pending-lock", 0, "");
    h.record(&["sandbox"]);
    let start = Instant::now();
    let r = held_run(&h, true, "--pending");
    assert_eq!(r.rc, 1, "{}", r.out);
    assert!(start.elapsed() < Duration::from_millis(5000));
    has(
        &r.out,
        &format!(
            "another update is running ({}), still held after 0s; nothing was changed",
            lock(&h)
        ),
    );
    assert_eq!(h.calls("build"), "");
    assert_eq!(h.owed_steps(), "sandbox");
}

#[test]
fn pending_with_check_or_to_is_refused_before_the_settings_are_read() {
    let mut h = Host::new("pending-args");
    h.record(&["sandbox"]);
    // No instance at all: the refusal is about the arguments, so 64, not 78.
    let r = h.update(&["--pending", "--check"]);
    assert_eq!(r.rc, 64, "{}", r.out);
    has(
        &r.out,
        "--pending runs the recorded follow-ups, and does not go with --check",
    );
    assert_eq!(h.owed_steps(), "sandbox");
    h.instance();
    h.platform(0, "");
    h.record(&["sandbox"]);
    for args in [
        &["--check", "--pending"][..],
        &["--pending", "--to", "v1.2.3"],
        &["--to", "v1.2.3", "--pending"],
    ] {
        let r = h.update(args);
        assert_eq!(r.rc, 64, "{args:?}: {}", r.out);
        has(
            &r.out,
            "--pending runs the recorded follow-ups, and does not go with",
        );
    }
    assert_eq!(h.owed_steps(), "sandbox");
}

#[test]
fn a_step_that_fails_under_pending_stays_recorded_for_the_next_run() {
    let h = host("pending-fails", 0, "");
    h.docker_no_answer();
    h.record(&["docs --force-recreate", "sandbox"]);
    let r = h.update(&["--pending"]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "docs: docker ps failed; the step is not done");
    assert_eq!(h.calls("build"), "build");
    assert_eq!(h.owed_steps(), "docs --force-recreate");
}

#[test]
fn pending_runs_the_steps_with_the_platform_held() {
    let h = host("pending-held", 0, "");
    fs::create_dir_all(h.home.join("state")).unwrap();
    fs::write(h.home.join("state/update-hold"), "").unwrap();
    h.record(&["docs --force-recreate"]);
    let r = h.update(&["--pending"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    lacks(&r.out, "the platform is held");
    has(&r.out, "docs: updating the container");
    assert_eq!(h.owed_steps(), "");
}

#[test]
fn pending_runs_the_labels_step_once_and_clears_the_record() {
    let h = host("pending-labels", 0, "");
    h.record(&["labels"]);
    let r = h.update(&["--pending"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "labels: making the tehas labels");
    assert_eq!(h.calls("tehas"), "labels");
    // Through a timeout, as every other outside call of a run.
    assert_eq!(
        bounded(&h),
        format!("{}/bin/tehas labels", h.home.display())
    );
    assert_eq!(h.owed_steps(), "");
}

#[test]
fn the_labels_step_runs_after_the_other_steps_and_a_failing_one_keeps_it_owed() {
    let h = host("labels-last", 0, "");
    h.record(&["docs --force-recreate", "labels"]);
    let r = h.update(&["--pending"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(step_order(&r.out), "docs: updating\nlabels: making");
    assert_eq!(h.calls("tehas"), "labels");
    assert_eq!(h.owed_steps(), "");
    h.platform(0, "");
    h.labels_fail();
    h.record(&["labels"]);
    let r = h.update(&["--pending"]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "labels: tehas labels exited 1; the step is not done",
    );
    has(&r.out, "could not make tehas:ready");
    assert_eq!(h.owed_steps(), "labels");
    // The next run, with the host's command answering again.
    h.labels_ok();
    let _ = fs::remove_file(h.join("calls/tehas"));
    let r = h.update(&["--pending"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(h.calls("tehas"), "labels");
    assert_eq!(h.owed_steps(), "");
}

#[test]
fn the_labels_step_names_the_hosts_command_and_is_owed_when_it_is_not_there() {
    let h = host("labels-gone", 0, "");
    h.record(&["labels"]);
    fs::remove_file(h.home.join("bin/tehas")).unwrap();
    let r = h.update(&["--pending"]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        &format!("labels: {}/bin/tehas is not installed", h.home.display()),
    );
    assert_eq!(h.calls("tehas"), "");
    assert_eq!(h.owed_steps(), "labels");
}
