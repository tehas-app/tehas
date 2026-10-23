//! The cycle lock probe on its own.

use crate::lock::{self, Lock};
use crate::verbs_tests::{hold, modes_bind, scratch};
use std::fs;
use std::os::unix::fs::PermissionsExt;

fn rc(l: &Lock) -> (u8, bool) {
    (l.rc, l.file)
}

#[test]
fn an_absent_lock_is_free_unless_a_cycle_is_on_record_as_running() {
    let home = scratch("probe-absent");
    // No state/cycles at all.
    assert_eq!(rc(&lock::probe(&home, 3, false)), (0, false));
    let l = lock::probe(&home, 3, true);
    assert_eq!(rc(&l), (2, false));
    assert_eq!(
        l.why.as_deref(),
        Some("no lock file for a cycle on record as running")
    );
    // The directory, but not the file.
    fs::create_dir_all(lock::dir(&home)).unwrap();
    assert_eq!(rc(&lock::probe(&home, 3, false)), (0, false));
    assert_eq!(rc(&lock::probe(&home, 3, true)), (2, false));
}

#[test]
fn a_lock_file_is_free_or_held_as_flock_finds_it() {
    let home = scratch("probe-held");
    let f = hold(&home, 4);
    assert_eq!(lock::probe(&home, 4, true).state(), "held");
    assert_eq!(rc(&lock::probe(&home, 4, false)), (1, true));
    drop(f);
    assert_eq!(rc(&lock::probe(&home, 4, true)), (0, true));
    // The probe gives the lock back: a holder can take it after.
    let f = hold(&home, 4);
    assert_eq!(lock::probe(&home, 4, true).rc, 1);
    drop(f);
}

#[test]
fn the_probe_never_creates_a_lock_file() {
    let home = scratch("probe-creates");
    fs::create_dir_all(lock::dir(&home)).unwrap();
    lock::probe(&home, 5, false);
    lock::probe(&home, 5, true);
    assert!(!lock::path(&home, 5).exists());
    assert_eq!(fs::read_dir(lock::dir(&home)).unwrap().count(), 0);
}

#[test]
fn a_lock_that_is_a_link_a_directory_or_not_in_a_directory_is_probe_rc_2() {
    let home = scratch("probe-shapes");
    fs::create_dir_all(lock::dir(&home)).unwrap();
    fs::create_dir(lock::path(&home, 6)).unwrap();
    assert_eq!(rc(&lock::probe(&home, 6, false)), (2, true));
    let target = home.join("elsewhere.lock");
    fs::write(&target, b"").unwrap();
    std::os::unix::fs::symlink(&target, lock::path(&home, 7)).unwrap();
    assert_eq!(rc(&lock::probe(&home, 7, false)), (2, true));
    // state/cycles itself a file, or a link to a directory.
    let other = scratch("probe-dir-shapes");
    fs::create_dir_all(other.join("state")).unwrap();
    fs::write(lock::dir(&other), b"").unwrap();
    let l = lock::probe(&other, 1, false);
    assert_eq!(l.rc, 2);
    assert_eq!(l.why.as_deref(), Some("state/cycles is not a directory"));
    fs::remove_file(lock::dir(&other)).unwrap();
    let real = other.join("real");
    fs::create_dir(&real).unwrap();
    std::os::unix::fs::symlink(&real, lock::dir(&other)).unwrap();
    assert_eq!(lock::probe(&other, 1, false).rc, 2);
}

#[test]
fn listed_names_only_issue_lock_files() {
    let home = scratch("listed");
    assert_eq!(lock::listed(&home), Ok(vec![]));
    let d = lock::dir(&home);
    fs::create_dir_all(&d).unwrap();
    for name in [
        "12.lock",
        "3.lock",
        "012.lock",
        "x.lock",
        "plan-3.json.lock",
        "5.lock.new",
        "0.lock",
        "12.taken",
    ] {
        fs::write(d.join(name), b"").unwrap();
    }
    assert_eq!(lock::listed(&home), Ok(vec![3, 12]));
    if modes_bind() {
        fs::set_permissions(&d, fs::Permissions::from_mode(0o000)).unwrap();
        assert!(lock::listed(&home).is_err());
        fs::set_permissions(&d, fs::Permissions::from_mode(0o700)).unwrap();
    }
}

#[test]
fn the_lock_object_carries_its_state_and_why() {
    let home = scratch("lock-json");
    let v: serde_json::Value = serde_json::from_str(&lock::probe(&home, 1, true).json(42)).unwrap();
    assert_eq!(v["state"], "unknown");
    assert_eq!(v["probe_rc"], 2);
    assert_eq!(v["file"], false);
    assert_eq!(v["seen_at"], 42);
    assert!(v["why"].is_string());
    let v: serde_json::Value =
        serde_json::from_str(&lock::probe(&home, 1, false).json(42)).unwrap();
    assert_eq!(v["state"], "free");
    assert!(v.get("why").is_none());
}
