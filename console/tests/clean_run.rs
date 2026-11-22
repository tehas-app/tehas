//! `tehasbot clean` as a run: a failing docker, the settings and arguments it
//! refuses before any docker call, its help, its lock, the console it needs,
//! its log, and step 6, the logs and the state store.

#[path = "clean/mod.rs"]
mod common;

use common::*;
use std::fs;
use std::path::Path;
use std::time::Duration;

#[test]
fn a_failing_docker_is_a_warning_the_other_steps_still_run_and_the_exit_is_1() {
    let h = Host::new("docker-fails");
    h.workspace(7);
    write(h.tmp("/dl/tool.tar.gz"), "old\n");
    age(&[h.tmp("/dl/tool.tar.gz")]);
    h.set_env(&["DOCKER_RC=1"]);
    let r = h.clean(&[]);
    rc(&r, 1);
    has(&r.out, "WARN: clean: docker builder prune failed");
    gone(h.ws("-7/target"));
    gone(h.tmp("/dl/tool.tar.gz"));
}

#[test]
fn a_prune_that_does_not_finish_in_time_is_a_warning_and_the_scratch_step_still_runs() {
    let h = Host::new("prune-timeout");
    h.workspace(7);
    write(h.tmp("/dl/tool.tar.gz"), "old\n");
    age(&[h.tmp("/dl/tool.tar.gz")]);
    // What timeout(1) answers when it ran out.
    h.set_env(&["DOCKER_RC=124"]);
    let r = h.clean(&[]);
    rc(&r, 1);
    has(
        &r.out,
        "WARN: clean: docker builder prune did not finish in 600 seconds",
    );
    gone(h.tmp("/dl/tool.tar.gz"));
}

#[test]
fn a_bad_tehas_clean_after_stops_the_run_before_any_docker_call() {
    let h = Host::new("bad-after");
    h.workspace(7);
    h.minimal(&["TEHAS_CLEAN_AFTER=soon"]);
    rc(&h.clean(&[]), 78);
    assert_eq!(h.docker(), "");
    there(h.ws("-7/target"));
}

#[test]
fn a_tehas_clean_dirs_of_git_stops_the_run_before_any_docker_call() {
    let h = Host::new("dirs-git");
    h.workspace(7);
    h.minimal(&["TEHAS_CLEAN_DIRS='.git target'"]);
    rc(&h.clean(&[]), 78);
    assert_eq!(h.docker(), "");
    there(h.ws("-7/target"));
    there(h.ws("-7/.git/HEAD"));
}

#[test]
fn an_unknown_argument_is_64_and_changes_nothing() {
    let h = Host::new("unknown");
    h.workspace(7);
    rc(&h.clean(&["--yes"]), 64);
    assert_eq!(h.docker(), "");
    there(h.ws("-7/target"));
}

#[test]
fn a_second_argument_is_64_whatever_the_first_and_runs_nothing() {
    let h = Host::new("second-arg");
    h.workspace(7);
    for first in ["--dry-run", "--help"] {
        let r = h.clean(&[first, "--yes"]);
        rc(&r, 64);
        has(&r.out, "tehasbot clean: unknown argument: --yes");
        has(&r.out, "Usage:");
    }
    assert_eq!(h.docker(), "");
}

#[test]
fn an_image_name_docker_cannot_list_is_78_before_any_docker_call() {
    let h = Host::new("bad-image");
    h.workspace(7);
    h.minimal(&["PI_ENSEMBLE_IMAGE=tehas/pi-rukas:a:b"]);
    let r = h.clean(&[]);
    rc(&r, 78);
    has(
        &r.out,
        "tehas: PI_ENSEMBLE_IMAGE=tehas/pi-rukas:a:b names no image docker can list",
    );
    has(
        &r.out,
        "tehas clean: PI_ENSEMBLE_IMAGE=tehas/pi-rukas:a:b is not an image name",
    );
    assert_eq!(h.docker(), "");
    there(h.ws("-7/target"));
}

#[test]
fn the_help_text_says_what_it_removes_its_usage_and_its_exits() {
    let h = Host::new("help");
    let r = h.clean(&["--help"]);
    rc(&r, 0);
    has(&r.out, "tehasbot clean");
    has(&r.out, "Usage:");
    has(&r.out, "Exit:");
    lacks(&r.out, "unknown argument");
}

#[test]
fn a_second_run_while_one_holds_the_lock_does_nothing_and_says_so() {
    let h = Host::new("locked");
    let _held = hold(h.clean_lock());
    h.workspace(7);
    let r = h.clean(&[]);
    rc(&r, 3);
    has(&r.out, "another run holds the lock");
    there(h.ws("-7/target"));
}

#[test]
fn a_host_with_no_console_beside_the_command_removes_nothing_78_and_says_to_install() {
    let h = Host::new("no-console");
    h.workspace(7);
    fs::remove_file(h.console()).unwrap();
    let r = h.clean(&[]);
    rc(&r, 78);
    has(
        &r.out,
        &format!(
            "tehasbot: no console at {}: run install.sh",
            h.console().display()
        ),
    );
    assert_eq!(h.docker(), "");
    there(h.ws("-7/target"));
}

#[test]
fn an_instance_with_no_settings_at_all_is_refused_before_anything_runs() {
    let h = Host::new("no-instance");
    h.no_instance();
    rc(&h.clean(&[]), 78);
    assert_eq!(h.docker(), "");
}

#[test]
fn what_a_timer_run_prints_also_goes_to_the_log_a_person_reads() {
    // clean.log is the only file a removal is written to; a warning reaches
    // it by another path than a removal does.
    let h = Host::new("log");
    h.workspace(7);
    h.set_env(&["DOCKER_RC=1"]);
    let r = h.clean(&[]);
    rc(&r, 1);
    let removed = format!("removed {}-7/target: build output of #7", h.wd());
    has(&r.out, "WARN: clean: docker builder prune failed");
    has(&h.log(), "WARN: clean: docker builder prune failed");
    has(&r.out, &removed);
    assert_eq!(count(&h.log(), &removed), 1);
}

/// The names of the files under `dir`, sorted.
fn files_of(dir: &Path) -> Vec<String> {
    let mut names = vec![];
    for e in fs::read_dir(dir).unwrap() {
        let e = e.unwrap();
        let ft = e.file_type().unwrap();
        if ft.is_dir() {
            names.extend(files_of(&e.path()));
        } else if ft.is_file() {
            names.push(e.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    names
}

/// The daily backups of the store: `tehas-<digit>*.db` under `dir`.
fn backups(dir: &Path) -> usize {
    files_of(dir)
        .iter()
        .filter(|n| {
            n.strip_prefix("tehas-")
                .is_some_and(|r| r.starts_with(|c: char| c.is_ascii_digit()) && r.ends_with(".db"))
        })
        .count()
}

#[test]
fn step_6_logs_of_cycles_planning_turns_and_index_runs_over_7_days_go_and_an_analyst_log_an_earlier_version_left(
) {
    let h = Host::new("logs");
    for f in [
        "work-3-261001-000000.jsonl",
        "work-3-261001-000000.jsonl.err",
        "dispatch-work-3.log",
        "index-261001-000000.log",
        "improve-analyst-261001-000000.log",
        "plan-4-261001-000000.jsonl",
        "dispatch-plan-4.log",
        "dispatch.log",
        "clean.log",
    ] {
        touch_ago(h.logs(&format!("/{f}")), Duration::from_secs(8 * 86400));
    }
    touch_ago(
        h.logs("/work-5-261004-000000.jsonl"),
        Duration::from_secs(6 * 86400),
    );
    let r = h.clean(&["--dry-run"]);
    has(
        &r.out,
        "would remove logs/plan-4-261001-000000.jsonl: a log 7 days old",
    );
    there(h.logs("/plan-4-261001-000000.jsonl"));
    assert!(!h.backups().is_dir(), "a dry run makes no backup");
    let r = h.clean(&[]);
    rc(&r, 0);
    has(&r.out, "removed logs/dispatch-plan-4.log: a log 7 days old");
    assert_eq!(
        files_of(Path::new(&h.logs(""))),
        ["clean.log", "dispatch.log", "work-5-261004-000000.jsonl"]
    );
}

#[test]
fn step_6_the_state_store_is_backed_up_once_a_day_and_pruned() {
    let h = Host::new("backup");
    let r = h.clean(&[]);
    rc(&r, 0);
    assert_eq!(backups(&h.backups()), 1);
    has(&r.out, "clean: store upkeep: ");
    h.clean(&[]);
    assert_eq!(backups(&h.backups()), 1);
}

#[test]
fn step_6_no_state_store_is_a_warning_and_the_other_steps_still_run() {
    let h = Host::new("no-store");
    h.no_store();
    let r = h.clean(&[]);
    rc(&r, 1);
    has(
        &r.out,
        "WARN: clean: the state store is not installed: no logs removed, no backup",
    );
    has(&r.out, "clean: freed ");
}
