//! The locks of `tehas-cycle cycle`, `work` and `index`: each takes its own,
//! with the waits and exit statuses of the scripts it replaced (#1588), and
//! tehas work takes the lock its parent handed down as its own.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use std::time::{Duration, Instant};
use tehas_core::testdir::TestDir;
use tehas_core::testing::write_exe;
use tehas_cycle::lock;

fn tehas_cycle(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tehas-cycle"))
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("TEHAS_HOME", home)
        .output()
        .unwrap()
}

fn said(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr)
}

/// A home whose state store is installed, as the cycle asks before its lock.
fn home_with_store(name: &str) -> TestDir {
    let h = TestDir::new(name);
    fs::create_dir_all(h.join("libexec")).unwrap();
    fs::create_dir_all(h.join("state/cycles")).unwrap();
    write_exe(h.join("libexec/store"), "#!/bin/sh\nexit 78\n").unwrap();
    fs::write(h.join("state/tehas.db"), "").unwrap();
    h
}

const CYCLE: [&str; 8] = [
    "cycle",
    "7",
    "--work",
    "/no/work",
    "--status-cmd",
    "/no/status",
    "--console",
    "/no/console",
];

#[test]
fn a_second_index_run_is_3_and_runs_nothing() {
    let h = TestDir::new("locks-index");
    fs::create_dir_all(h.join("state")).unwrap();
    let _held = lock::take(&h.join("state/index.lock"), Duration::ZERO)
        .unwrap()
        .unwrap();
    for args in [
        &["index"][..],
        &["index", "--before-cycle"],
        &["index", "main", "abc"],
    ] {
        let out = tehas_cycle(&h, args);
        assert_eq!(out.status.code(), Some(3), "{args:?} {out:?}");
        assert!(
            said(&out).contains("another index run holds its lock, nothing was indexed"),
            "{out:?}"
        );
        assert!(!h.join("logs").exists());
    }
}

#[test]
fn an_index_run_makes_its_lock_and_holds_it() {
    let h = TestDir::new("locks-index-free");
    // No settings: the run stops at 78 past the lock, which it took.
    let out = tehas_cycle(&h, &["index"]);
    assert_eq!(out.status.code(), Some(78), "{out:?}");
    assert!(h.join("state/index.lock").is_file());
    assert!(lock::take(&h.join("state/index.lock"), Duration::ZERO)
        .unwrap()
        .is_some());
}

#[test]
fn a_second_tehas_work_of_the_issue_is_5_and_touches_nothing() {
    let h = home_with_store("locks-work");
    let _held = lock::take(&h.join("state/cycles/7.lock"), Duration::ZERO)
        .unwrap()
        .unwrap();
    let out = tehas_cycle(&h, &["work", "7", "--console", "/no/console"]);
    assert_eq!(out.status.code(), Some(5), "{out:?}");
    assert_eq!(
        said(&out).trim(),
        "tehas work: a cycle for #7 is running, not starting another"
    );
    let left: Vec<_> = fs::read_dir(h.join("state/cycles")).unwrap().collect();
    assert_eq!(left.len(), 1);
}

// The parent holds the lock and hands it down, as tehas-cycle cycle does:
// the run goes on past the lock, to the settings it has none of.
#[test]
fn tehas_work_takes_the_lock_its_parent_handed_down() {
    let h = home_with_store("locks-work-inherited");
    let held = lock::take(&h.join("state/cycles/8.lock"), Duration::ZERO)
        .unwrap()
        .unwrap();
    lock::hand_down(&held).unwrap();
    let out = tehas_cycle(&h, &["work", "8", "--console", "/no/console"]);
    drop(held);
    assert_ne!(out.status.code(), Some(5), "{out:?}");
    assert!(!said(&out).contains("a cycle for #8 is running"), "{out:?}");
}

#[test]
fn a_cycle_without_its_store_is_78_before_its_lock() {
    let h = TestDir::new("locks-cycle-store");
    let out = tehas_cycle(&h, &CYCLE);
    assert_eq!(out.status.code(), Some(78), "{out:?}");
    assert!(said(&out).contains("ERROR: the state store is not installed (run install.sh)"));
    assert!(!h.join("state/cycles/7.lock").exists());
}

#[test]
fn a_second_cycle_of_the_issue_waits_five_seconds_and_is_3() {
    let h = home_with_store("locks-cycle");
    let _held = lock::take(&h.join("state/cycles/7.lock"), Duration::ZERO)
        .unwrap()
        .unwrap();
    let began = Instant::now();
    let out = tehas_cycle(&h, &CYCLE);
    assert!(began.elapsed() >= Duration::from_secs(5));
    assert_eq!(out.status.code(), Some(3), "{out:?}");
    assert!(said(&out).contains("a cycle for #7 is already running"));
}
