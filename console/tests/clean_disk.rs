//! `tehasbot clean` on a host short of space: under TEHAS_CLEAN_MIN_FREE,
//! steps 1 and 2 take build output and the build cache whatever their age.
//! The other steps keep the age, a held lock still holds, and a host whose
//! free space cannot be read changes nothing early. Each case names its
//! reading of the disk; the stub answers 40 GiB, above the default 30g.

#[path = "clean/mod.rs"]
mod common;

use common::*;

/// The roots of issue 7 young again, its lock too.
fn young_again(h: &Host) {
    touch(h.ws("-7"));
    touch(h.ws("-7.state"));
    touch(h.ws("-7.state/cache"));
    touch(h.cycle_lock(7));
}

#[test]
fn the_docker_step_is_asked_for_the_cache_of_the_instances_age() {
    let h = Host::new("cache-age");
    h.minimal(&["TEHAS_CLEAN_AFTER=6"]);
    rc(&h.clean(&[]), 0);
    has(&h.docker(), "builder prune -f --filter until=6h");
}

#[test]
fn a_young_workspace_loses_its_build_output_when_the_disk_is_under_the_minimum() {
    let h = Host::new("low-young");
    h.workspace_young(
        7,
        &[&h.ws("-7"), &h.ws("-7.state"), &h.ws("-7.state/cache")],
    );
    h.df_free(12);
    let r = h.clean(&[]);
    rc(&r, 0);
    for g in ["target", "node_modules", "sub/target", "nested/target"] {
        gone(h.ws(&format!("-7/{g}")));
    }
    for k in ["src/main.sh", "uncommitted", ".git/HEAD", "keep/notes"] {
        there(h.ws(&format!("-7/{k}")));
    }
    for k in ["memory/learned", "sessions/one.jsonl"] {
        there(h.ws(&format!("-7.state/{k}")));
    }
    gone(h.ws("-7.state/cache"));
    has(&r.out, "low on disk: 12.0 GiB free, under TEHAS_CLEAN_MIN_FREE=30g: build output and build cache go whatever their age");
    lacks(&r.out, "younger than");
}

#[test]
fn under_the_minimum_the_cache_is_pruned_whole_and_the_rest_of_the_run_keeps_its_age() {
    let h = Host::new("low-cache");
    h.workspace_young(7, &[&h.ws("-7")]);
    write(h.tmp("/dl/tool.tar.gz"), "old\n");
    write(h.tmp("/dl/tool2.tar.gz"), "new\n");
    age(&[h.tmp("/dl/tool.tar.gz")]);
    h.df_free(4);
    rc(&h.clean(&[]), 0);
    has(&h.docker(), "builder prune -f --filter until=0h");
    // Step 3 keeps the age.
    gone(h.tmp("/dl/tool.tar.gz"));
    there(h.tmp("/dl/tool2.tar.gz"));
}

#[test]
fn a_young_workspace_keeps_its_build_output_at_the_minimum_as_it_does_above_it() {
    let h = Host::new("at-min");
    h.workspace(7);
    young_again(&h);
    h.df_free(30);
    let r = h.clean(&[]);
    rc(&r, 0);
    there(h.ws("-7/target"));
    there(h.ws("-7.state/cache"));
    has(&h.docker(), "builder prune -f --filter until=24h");
    lacks(&r.out, "low on disk");
}

#[test]
fn one_gibibyte_under_the_minimum_takes_the_young_build_output() {
    let h = Host::new("under-min");
    h.workspace(7);
    young_again(&h);
    h.df_free(29);
    let r = h.clean(&[]);
    rc(&r, 0);
    gone(h.ws("-7/target"));
    gone(h.ws("-7.state/cache"));
    has(&h.docker(), "builder prune -f --filter until=0h");
    has(
        &r.out,
        "low on disk: 29.0 GiB free, under TEHAS_CLEAN_MIN_FREE=30g",
    );
}

#[test]
fn the_minimum_read_in_the_kilobytes_df_prints_is_the_same_point() {
    let h = Host::new("min-kb");
    h.workspace(7);
    young_again(&h);
    h.df_reset();
    h.fx("free", &format!("{}\n", kb(30)));
    let r = h.clean(&[]);
    rc(&r, 0);
    there(h.ws("-7/target"));
    lacks(&r.out, "low on disk");
}

#[test]
fn above_the_minimum_nothing_goes_early_and_no_line_of_it_is_logged() {
    let h = Host::new("above-min");
    h.workspace(7);
    young_again(&h);
    h.df_free(100);
    let r = h.clean(&[]);
    rc(&r, 0);
    there(h.ws("-7/target"));
    there(h.ws("-7.state/cache"));
    has(&h.docker(), "builder prune -f --filter until=24h");
    lacks(&r.out, "low on disk");
    has(&r.out, &format!("younger than {AFTER} hours"));
}

#[test]
fn the_file_system_docker_keeps_its_images_on_decides_too() {
    // Room for the home, none for the images: the lesser is what is read.
    let h = Host::new("docker-fs");
    h.df_free_of(100, 20);
    h.workspace(7);
    let r = h.clean(&[]);
    rc(&r, 0);
    gone(h.ws("-7/target"));
    assert_eq!(count(&r.out, "low on disk"), 1);
    has(
        &r.out,
        "low on disk: 20.0 GiB free, under TEHAS_CLEAN_MIN_FREE=30g",
    );
}

#[test]
fn a_held_lock_keeps_its_build_output_when_the_disk_is_low() {
    let h = Host::new("held-low");
    h.workspace(7);
    let held = hold(h.cycle_lock(7));
    h.df_free(2);
    let r = h.clean(&[]);
    rc(&r, 0);
    drop(held);
    there(h.ws("-7/target"));
    there(h.ws("-7.state/cache"));
    has(&r.out, "kept the build output of #7: its lock is held");
}

#[test]
fn a_held_lock_keeps_its_build_output_when_the_disk_has_room() {
    let h = Host::new("held-room");
    h.workspace(7);
    let held = hold(h.cycle_lock(7));
    h.df_free(100);
    let r = h.clean(&[]);
    rc(&r, 0);
    drop(held);
    there(h.ws("-7/target"));
    there(h.ws("-7.state/cache"));
    has(&r.out, "kept the build output of #7: its lock is held");
}

#[test]
fn a_df_that_fails_changes_nothing() {
    let h = Host::new("df-fails");
    h.workspace_young(
        7,
        &[&h.ws("-7"), &h.ws("-7.state"), &h.ws("-7.state/cache")],
    );
    h.df_free(2);
    h.fx("df-fails", "");
    let r = h.clean(&[]);
    rc(&r, 0);
    there(h.ws("-7/target"));
    there(h.ws("-7.state/cache"));
    has(&h.docker(), "builder prune -f --filter until=24h");
    lacks(&r.out, "low on disk");
}

#[test]
fn a_df_that_answers_no_number_changes_nothing() {
    let h = Host::new("df-word");
    h.workspace_young(7, &[&h.ws("-7")]);
    h.fx("free", "not-a-figure\n");
    let r = h.clean(&[]);
    rc(&r, 0);
    there(h.ws("-7/target"));
    has(&h.docker(), "builder prune -f --filter until=24h");
    lacks(&r.out, "low on disk");
}

#[test]
fn the_age_of_the_run_is_one_reading_of_the_disk() {
    let h = Host::new("one-reading");
    h.workspace_young(7, &[&h.ws("-7")]);
    h.df_free(1);
    let r = h.clean(&[]);
    rc(&r, 0);
    assert_eq!(count(&r.out, "low on disk"), 1);
    has(&h.docker(), "builder prune -f --filter until=0h");
}

#[test]
fn a_dry_run_low_on_disk_logs_the_early_removals_and_removes_nothing() {
    let h = Host::new("low-dry");
    h.workspace(7);
    h.df_free(8);
    let r = h.clean(&["--dry-run"]);
    rc(&r, 0);
    there(h.ws("-7/target"));
    there(h.ws("-7.state/cache"));
    has(
        &r.out,
        &format!("would remove {}-7/target: build output of #7", h.wd()),
    );
    lacks(&h.docker(), "builder prune");
    has(
        &r.out,
        "would prune docker's build cache of 0 hours and older",
    );
}

#[test]
fn the_instances_own_minimum_is_the_point_the_cleaner_reads() {
    let h = Host::new("own-min");
    h.workspace_young(7, &[&h.ws("-7")]);
    h.minimal(&["TEHAS_CLEAN_MIN_FREE=6g"]);
    h.df_free(12);
    let r = h.clean(&[]);
    rc(&r, 0);
    there(h.ws("-7/target"));
    has(&h.docker(), "builder prune -f --filter until=24h");
    lacks(&r.out, "low on disk");
}
