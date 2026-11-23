//! `console tidy run` where it reaches no GitHub: the settings and the words
//! it refuses before its first call, and its help. It runs as a poll runs it,
//! with the settings tehas_load left in the environment. gh is not stubbed, so
//! a call to it fails the case. The cases that read and edit the issues are in
//! test/tidy_test.sh until ops moves onto the ports' fakes.

mod common;

use common::*;
use std::fs;

/// A host with the minimal instance and gh unstubbed.
fn host(name: &str) -> World {
    let w = World::new(&format!("tidy-{name}"));
    fs::remove_file(w.t().join("bin/gh")).unwrap();
    w
}

/// The console's tidy as a poll runs it, with these words.
fn tidy(w: &World, words: &str) -> Run {
    w.sh(&format!(
        r#". "$T_ROOT/lib/config.sh"; tehas_load || exit 78; exec "$TEHAS_CONSOLE" tidy run {words}"#
    ))
}

#[track_caller]
fn rc(r: &Run, want: i32) {
    assert_eq!(r.rc, want, "exit {} and not {want}:\n{}", r.rc, r.out);
}

#[test]
fn without_the_repository_nothing_is_called_exit_78() {
    let w = host("no-repo");
    w.instance(&["TEHAS_GITHUB_APP_ID=1", "TEHAS_CONTROL_ISSUE=1"]);
    let r = tidy(&w, "");
    rc(&r, 78);
    has(&r.out, "TEHAS_REPO is not set");
}

#[test]
fn without_the_control_issue_nothing_is_called_exit_78() {
    let w = host("no-control");
    w.instance(&["TEHAS_GITHUB_APP_ID=1", "TEHAS_REPO=acme/widget"]);
    let r = tidy(&w, "");
    rc(&r, 78);
    has(&r.out, "TEHAS_CONTROL_ISSUE is not set");
}

#[test]
fn a_tehas_tidy_wait_that_is_no_number_of_seconds_stops_it_before_the_first_call_exit_78() {
    for bad in ["0", "007", "soon", "100000"] {
        let w = host(&format!("wait-{bad}"));
        w.set_env(&[&format!("TEHAS_TIDY_WAIT={bad}")]);
        let r = tidy(&w, "");
        rc(&r, 78);
        has(
            &r.out,
            &format!("TEHAS_TIDY_WAIT is not a number of seconds: {bad}"),
        );
    }
}

#[test]
fn an_unknown_argument_is_refused_before_the_settings_are_read_exit_64() {
    let w = host("unknown");
    w.instance(&["TEHAS_GITHUB_APP_ID=1"]);
    let r = tidy(&w, "--dryrun");
    rc(&r, 64);
    has(&r.out, "tehas tidy: unknown argument: --dryrun");
    rc(&tidy(&w, "--dry-run --help"), 64);
}

#[test]
fn help_prints_the_usage_and_calls_nothing() {
    let w = host("help");
    w.instance(&["TEHAS_GITHUB_APP_ID=1"]);
    let r = tidy(&w, "--help");
    rc(&r, 0);
    has(&r.out, "Usage:");
}
