//! The lock files of cycles that ended, and the store a restored backup
//! leaves.

mod common;

use common::*;
use std::fs;
use std::process::Command;

/// A cycle of #`n` that ended `days` days ago, its lock file left.
fn ended_days_ago(w: &World, n: u64, days: i64) {
    let ns = n.to_string();
    let end = tehas_core::time::now() - days * 86400;
    w.hold(n);
    let r = w.store_at(end - 60, &["cycle-start", &ns, "--by-hand"]);
    assert_eq!(
        r.rc, 0,
        "the store did not start the record of #{n}: {}",
        r.out
    );
    w.release();
    let r = w.store_at(
        end,
        &["cycle-end", &ns, "--attempt", "1", "--status", "merged"],
    );
    assert_eq!(
        r.rc, 0,
        "the store did not end the record of #{n}: {}",
        r.out
    );
    // As old as its cycle: a lock touched in the last minute may have a waiter.
    let ok = Command::new("touch")
        .args(["-d", &format!("{days} days ago")])
        .arg(w.cycles_dir().join(format!("{n}.lock")))
        .status()
        .unwrap();
    assert!(ok.success());
}

#[test]
fn the_lock_of_an_issue_nothing_has_used_for_30_days_goes_and_one_used_lately_stays() {
    let w = World::new("locks-30-days");
    ended_days_ago(&w, 31, 40);
    ended_days_ago(&w, 32, 10);
    let r = w.dispatch(&["--dry-run"]);
    has(&r.out, "would remove the lock of #31: unused for 30 days");
    let locks: Vec<String> = w
        .files(&w.cycles_dir())
        .into_iter()
        .filter(|f| f == "31.lock" || f == "32.lock")
        .collect();
    assert_eq!(locks, ["31.lock", "32.lock"]);
    let r = w.poll();
    has(&r.out, "removed the lock of #31: unused for 30 days");
    lacks(&r.out, "#32: unused");
    assert!(!w.cycles_dir().join("31.lock").exists());
    assert!(w.cycles_dir().join("32.lock").exists());
}

#[test]
fn a_backup_restored_into_a_fresh_home_is_a_store_a_poll_runs_on() {
    let w = World::new("locks-backup");
    ended_days_ago(&w, 33, 1);
    let r = w.store(&["backup"]);
    assert_eq!(r.rc, 0, "the store made no backup: {}", r.out);
    let backups = w.home.join("state/backup");
    let copy = w
        .files(&backups)
        .into_iter()
        .find(|f| {
            f.strip_prefix("tehas-")
                .is_some_and(|r| r.starts_with(|c: char| c.is_ascii_digit()) && r.ends_with(".db"))
        })
        .expect("a backup");
    let restored = w.t().join("restored.db");
    fs::copy(backups.join(copy), &restored).unwrap();
    w.reset();
    let state = w.home.join("state");
    fs::copy(&restored, state.join("tehas.db")).unwrap();
    for f in ["tehas.db-wal", "tehas.db-shm"] {
        let _ = fs::remove_file(state.join(f));
    }
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(state.join("tehas.db"), fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(w.store_ok(&["check"])["integrity"], "ok");
    assert_eq!(
        w.row(&["cycle-show", "33"])["cycle"]["end_reason"],
        "merged"
    );
    assert_eq!(w.dispatch(&["--dry-run"]).rc, 0);
}

// The poll asks the store once which locks are held, so a host with a long
// history of issues probes no more than one with none. A probe never runs
// `true`, which flock would look for through every directory of PATH.
#[test]
fn a_thousand_old_lock_files_cost_the_poll_no_probe_each_and_the_two_held_ones_still_count() {
    let w = World::new("locks-thousand");
    fs::create_dir_all(w.cycles_dir()).unwrap();
    for n in 1001..=2000 {
        fs::write(w.cycles_dir().join(format!("{n}.lock")), "").unwrap();
    }
    w.hold(7);
    w.hold(8);
    w.stub("flock", r#"exec "$(command -pv flock)" "$@""#);
    w.stub("true", "");
    let r = w.dispatch(&["--dry-run"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.calls("true"), "");
    let probes = w.calls("flock").lines().count();
    assert!(
        probes < 20,
        "the poll ran flock {probes} times over 1,002 lock files"
    );
    w.release();
}
