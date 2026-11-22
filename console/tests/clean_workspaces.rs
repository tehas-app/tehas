//! `tehasbot clean`, step 1: the build output of an issue's kept trees, its
//! workspace, the tree a retry moved aside and the worktrees of its cycles,
//! under the issue's lock and its one age rule.

#[path = "clean/mod.rs"]
mod common;

use common::*;
use std::os::unix::fs::symlink;

#[test]
fn an_idle_kept_workspace_loses_its_build_output_and_keeps_the_rest() {
    let h = Host::new("idle");
    h.workspace(7);
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
    let w = h.wd();
    has(&r.out, &format!("removed {w}-7/target: build output of #7"));
    has(
        &r.out,
        &format!("removed {w}-7/node_modules: build output of #7"),
    );
    has(
        &r.out,
        &format!("removed {w}-7/nested/target: build output of #7"),
    );
    has(
        &r.out,
        &format!("removed {w}-7.state/cache: the code index copy of #7"),
    );
    has(&r.out, "freed");
    assert_eq!(count(&r.out, "clean:"), h.log().lines().count());
}

#[test]
fn build_output_under_a_nested_tree_of_the_workspace_goes_too() {
    let h = Host::new("nested");
    h.workspace(7);
    write(h.ws("-7/pkg/ext/target/app"), "x\n");
    age(&[
        h.ws("-7/pkg"),
        h.ws("-7/pkg/ext"),
        h.ws("-7/pkg/ext/target"),
    ]);
    // A new name is a write in the workspace, whose age the rule reads.
    age(&[h.ws("-7")]);
    rc(&h.clean(&[]), 0);
    gone(h.ws("-7/pkg/ext/target"));
}

#[test]
fn a_workspace_touched_within_the_age_is_left_alone() {
    let h = Host::new("young");
    h.workspace_young(7, &[&h.ws("-7")]);
    let r = h.clean(&[]);
    rc(&r, 0);
    there(h.ws("-7/target"));
    there(h.ws("-7/node_modules"));
    there(h.ws("-7.state/cache"));
    lacks(&r.out, &format!("removed {}-7/target", h.wd()));
}

#[test]
fn a_workspace_whose_issue_has_a_running_cycle_is_left_alone() {
    let h = Host::new("running");
    h.workspace(7);
    let _held = hold(h.cycle_lock(7));
    rc(&h.clean(&[]), 0);
    there(h.ws("-7/target"));
    there(h.ws("-7.state/cache"));
}

#[test]
fn a_workspace_whose_lock_another_holds_is_left_alone() {
    let h = Host::new("held");
    // The lock is taken before the fixture is made, and held while it is.
    write(h.cycle_lock(8), "");
    let _held = hold(h.cycle_lock(8));
    write(h.ws("-8/target/app"), "x\n");
    mkdir(h.ws("-8/src"));
    write(h.ws("-8.state/cache/index.db"), "idx\n");
    age(&[h.cycle_lock(8)]);
    age(&[
        h.ws("-8"),
        h.ws("-8/target"),
        h.ws("-8.state"),
        h.ws("-8.state/cache"),
    ]);
    age(&[h.cycle_lock(8)]);
    let r = h.clean(&[]);
    rc(&r, 0);
    there(h.ws("-8/target"));
    there(h.ws("-8.state/cache"));
    has(&r.out, "kept the build output of #8: its lock is held");
}

#[test]
fn a_symlink_named_like_build_output_is_neither_followed_nor_removed() {
    let h = Host::new("symlink");
    h.workspace(7);
    write(h.t("/outside/file"), "stay\n");
    symlink(h.t("/outside"), h.ws("-7/target-link")).unwrap();
    // As ln -s does onto a directory that is there: the link is made in it.
    symlink(h.t("/outside"), h.ws("-7/node_modules/outside")).unwrap();
    rc(&h.clean(&[]), 0);
    there(h.t("/outside/file"));
    link(h.ws("-7/target-link"));
}

// The roots of an issue beside its workspace: the tree a retry moved aside,
// and the worktrees a cycle makes under a dot-name of either.

#[test]
fn the_build_output_under_a_worktree_of_a_cycle_goes_and_its_dot_names_stay() {
    let h = Host::new("wt");
    h.workspace(7);
    let wt = h.worktree("", "");
    age(&[h.ws("-7")]);
    let r = h.clean(&[]);
    rc(&r, 0);
    gone(format!("{wt}/console/target"));
    there(format!("{wt}/src/main.sh"));
    assert!(
        h.worktree_head(&wt).is_some(),
        "git names the tree's commit"
    );
    there(format!("{wt}/.pi/agent/state.json"));
    has(
        &r.out,
        &format!("removed {wt}/console/target: build output of #7"),
    );
}

#[test]
fn the_build_output_under_a_second_worktree_of_the_same_issue_goes_too() {
    let h = Host::new("wt2");
    h.workspace(7);
    h.worktree("", "");
    age(&[h.ws("-7")]);
    let alpha = h.worktree("", "issue-7-alpha");
    let beta = h.worktree("", "issue-7-beta");
    age(&[h.ws("-7")]);
    let r = h.clean(&[]);
    rc(&r, 0);
    gone(format!("{alpha}/console/target"));
    gone(format!("{beta}/console/target"));
    has(
        &r.out,
        &format!("removed {beta}/console/target: build output of #7"),
    );
}

#[test]
fn the_build_output_of_the_tree_a_retry_moved_aside_goes_and_what_it_holds_stays() {
    let h = Host::new("retry");
    h.retry_tree(7);
    let r = h.clean(&[]);
    rc(&r, 0);
    for g in ["target", "node_modules", "sub/target", "nested/target"] {
        gone(h.ws(&format!("-7.before-retry/{g}")));
    }
    for k in ["src/main.sh", "uncommitted", ".git/HEAD", "keep/notes"] {
        there(h.ws(&format!("-7.before-retry/{k}")));
    }
    let w = h.wd();
    has(
        &r.out,
        &format!("removed {w}-7.before-retry/target: build output of #7"),
    );
    has(
        &r.out,
        &format!("removed {w}-7.before-retry/nested/target: build output of #7"),
    );
    lacks(&r.out, "kept the build output of #7");
}

#[test]
fn one_run_of_one_issue_takes_the_build_output_of_both_its_trees() {
    let h = Host::new("both");
    h.workspace(7);
    h.retry_tree(7);
    age(&[h.ws("-7"), h.ws("-7.before-retry")]);
    let r = h.clean(&[]);
    rc(&r, 0);
    gone(h.ws("-7/target"));
    gone(h.ws("-7.before-retry/target"));
    there(h.ws("-7/src/main.sh"));
    there(h.ws("-7.before-retry/src/main.sh"));
    let w = h.wd();
    has(
        &r.out,
        &format!("removed {w}-7.before-retry/target: build output of #7"),
    );
    has(&r.out, &format!("removed {w}-7/target: build output of #7"));
}

#[test]
fn a_worktree_under_the_tree_a_retry_moved_aside_is_a_root_of_its_own() {
    let h = Host::new("retry-wt");
    h.workspace(7);
    let wt = h.worktree(".before-retry", "");
    let r = h.clean(&[]);
    rc(&r, 0);
    gone(format!("{wt}/console/target"));
    assert!(
        h.worktree_head(&wt).is_some(),
        "git names the tree's commit"
    );
    has(
        &r.out,
        &format!("removed {wt}/console/target: build output of #7"),
    );
}

#[test]
fn a_link_among_the_worktrees_is_not_walked_and_what_it_holds_stays() {
    let h = Host::new("wt-link");
    h.workspace(7);
    let wt = h.worktree("", "issue-7-default");
    let out = h.t("/outside");
    write(format!("{out}/console/target/app"), "big\n");
    write(format!("{out}/src/main.sh"), "stay\n");
    symlink(&out, h.ws(&format!("-7/{WORKTREES}/linked"))).unwrap();
    age(&[
        out.clone(),
        format!("{out}/console"),
        format!("{out}/console/target"),
        format!("{out}/src"),
    ]);
    age(&[h.ws("-7")]);
    let r = h.clean(&[]);
    rc(&r, 0);
    there(format!("{out}/console/target/app"));
    there(format!("{out}/src/main.sh"));
    link(h.ws(&format!("-7/{WORKTREES}/linked")));
    gone(format!("{wt}/console/target"));
    lacks(&r.out, &format!("removed {out}/console/target"));
}

#[test]
fn an_empty_worktrees_directory_is_no_root_and_says_nothing_of_its_own() {
    let h = Host::new("wt-empty");
    h.workspace(7);
    mkdir(h.ws(&format!("-7/{WORKTREES}")));
    mkdir(h.ws(&format!("-7.before-retry/{WORKTREES}")));
    age(&[
        h.ws("-7"),
        h.ws(&format!("-7/{WORKTREES}")),
        h.ws("-7.before-retry"),
        h.ws(&format!("-7.before-retry/{WORKTREES}")),
    ]);
    let r = h.clean(&[]);
    rc(&r, 0);
    has(
        &r.out,
        &format!("removed {}-7/target: build output of #7", h.wd()),
    );
    lacks(&r.out, &format!("{WORKTREES}: build output of #7"));
}

#[test]
fn a_worktree_of_an_issue_touched_within_the_age_is_left_with_the_rest() {
    let h = Host::new("wt-young");
    h.workspace(7);
    let wt = h.worktree("", "");
    // The tree of a retry is one of the paths the rule reads: here a young
    // file of that name.
    touch(h.ws("-7.before-retry"));
    let r = h.clean(&[]);
    rc(&r, 0);
    there(format!("{wt}/console/target"));
    there(h.ws("-7/target"));
    lacks(&r.out, &format!("removed {wt}/console/target"));
}

#[test]
fn a_worktree_of_an_issue_whose_cycle_runs_is_left_alone() {
    let h = Host::new("wt-running");
    h.workspace(7);
    let wt = h.worktree("", "");
    let _held = hold(h.cycle_lock(7));
    rc(&h.clean(&[]), 0);
    there(format!("{wt}/console/target"));
    there(h.ws("-7/target"));
}

#[test]
fn an_idle_issue_with_no_build_output_at_all_says_so_and_not_that_a_lock_is_held() {
    let h = Host::new("bare");
    h.bare_workspace(7);
    let r = h.clean(&[]);
    rc(&r, 0);
    has(&r.out, "nothing to remove in #7");
    lacks(&r.out, "kept the build output of #7: its lock is held");
    for k in ["src/main.sh", ".git/HEAD", "keep/notes"] {
        there(h.ws(&format!("-7/{k}")));
    }
}

#[test]
fn an_issue_with_nothing_to_remove_in_the_dry_run_says_nothing_of_a_removal() {
    let h = Host::new("bare-dry");
    h.bare_workspace(7);
    std::fs::remove_file(h.cycle_lock(7)).unwrap();
    let r = h.clean(&["--dry-run"]);
    rc(&r, 0);
    lacks(&r.out, &format!("would remove {}-7", h.wd()));
    lacks(&r.out, "kept the build output of #7: its lock is held");
}

#[test]
fn one_issue_that_removed_under_its_lock_says_nothing_of_the_idle_one_beside_it() {
    let h = Host::new("two-issues");
    h.workspace(7);
    h.bare_workspace(8);
    let r = h.clean(&[]);
    rc(&r, 0);
    has(
        &r.out,
        &format!("removed {}-7/target: build output of #7", h.wd()),
    );
    has(&r.out, "nothing to remove in #8");
    lacks(&r.out, "kept the build output of #7: its lock is held");
    lacks(&r.out, "kept the build output of #8: its lock is held");
    gone(h.ws("-7/target"));
    there(h.ws("-8/src/main.sh"));
}

#[test]
fn a_worktree_whose_git_record_is_touched_within_the_age_is_left_with_its_build_output() {
    let h = Host::new("wt-record");
    h.workspace(7);
    let wt = h.worktree("", "");
    touch(h.gitdir(&wt));
    age(&[h.ws("-7")]);
    let r = h.clean(&[]);
    rc(&r, 0);
    there(format!("{wt}/console/target"));
    there(h.ws("-7/target"));
    lacks(&r.out, &format!("removed {wt}/console/target"));
    has(&r.out, &format!("younger than {AFTER} hours"));
}

#[test]
fn a_directory_whose_git_file_only_claims_a_gitdir_is_no_root_and_the_issue_says_so() {
    let h = Host::new("forged");
    h.workspace(7);
    let forged = h.ws(&format!("-7/{WORKTREES}/forged"));
    let someone = h.th("/dev/someone.state");
    mkdir(format!("{forged}/target"));
    write(format!("{forged}/.git"), &format!("gitdir: {someone}\n"));
    write(format!("{forged}/target/app"), "big\n");
    write(format!("{someone}/build/app"), "big\n");
    age(&[
        h.ws(&format!("-7/{WORKTREES}")),
        forged.clone(),
        format!("{forged}/target"),
        format!("{forged}/.git"),
        h.th("/dev"),
        someone.clone(),
        format!("{someone}/build"),
        format!("{someone}/build/app"),
    ]);
    age(&[h.ws("-7")]);
    h.df_reset();
    let r = h.clean(&[]);
    rc(&r, 0);
    there(format!("{forged}/target"));
    there(format!("{someone}/build/app"));
    there(h.ws("-7/target"));
    has(
        &r.out,
        "kept the build output of #7: git could not name the gitdir of one of its worktrees",
    );
    lacks(&r.out, "younger than");
    lacks(&r.out, "clean: removed");
}

#[test]
fn an_idle_issue_whose_lock_file_is_gone_is_left_whole_and_says_so() {
    let h = Host::new("no-lock");
    h.workspace(7);
    std::fs::remove_file(h.cycle_lock(7)).unwrap();
    // The trees dated again once the lock is gone, so the age rule passes.
    age(&[h.ws("-7"), h.ws("-7.state"), h.ws("-7.state/cache")]);
    for d in [
        "target",
        "node_modules",
        "sub",
        "sub/target",
        "nested",
        "nested/target",
    ] {
        age(&[h.ws(&format!("-7/{d}"))]);
    }
    let r = h.clean(&[]);
    rc(&r, 0);
    there(h.ws("-7/target"));
    there(h.ws("-7.state/cache"));
    has(
        &r.out,
        "kept the build output of #7: no lock to hold, so nothing of it was removed",
    );
    lacks(&r.out, "nothing to remove in #7");
}

#[test]
fn a_worktree_whose_tree_is_gone_is_named_in_the_log_and_nothing_of_the_issue_goes() {
    let h = Host::new("wt-gone");
    h.workspace(7);
    h.worktree("", "");
    // A .git file whose gitdir is no longer there.
    let t = h.ws(&format!("-7/{WORKTREES}/issue-7-default"));
    let _ = std::fs::remove_file(format!("{t}/.git"));
    write(
        format!("{t}/.git"),
        &format!("gitdir: {}/.worktrees/issue-7-default/gone\n", h.ws("-7")),
    );
    age(&[
        h.ws("-7"),
        h.ws(&format!("-7/{WORKTREES}")),
        t.clone(),
        format!("{t}/.git"),
    ]);
    let r = h.clean(&[]);
    rc(&r, 0);
    there(h.ws("-7/target"));
    there(h.ws("-7.state/cache"));
    has(
        &r.out,
        "kept the build output of #7: git could not name the gitdir of one of its worktrees",
    );
    lacks(&r.out, "younger than");
    lacks(&r.out, "clean: removed");
}

#[test]
fn a_worktree_whose_git_names_the_containers_path_is_dated_by_its_own_workspace() {
    let h = Host::new("ct-dir");
    h.workspace(7);
    let wt = h.container_worktree(Gitdir::Dir);
    let r = h.clean(&[]);
    rc(&r, 0);
    gone(format!("{wt}/target"));
    gone(h.ws("-7/target"));
    there(format!("{wt}/.git"));
    lacks(&r.out, "git could not name the gitdir");
    has(&r.out, &format!("removed {wt}/target: build output of #7"));
}

#[test]
fn a_worktree_whose_git_names_the_containers_path_and_has_no_record_is_cleaned() {
    let h = Host::new("ct-none");
    h.workspace(7);
    let wt = h.container_worktree(Gitdir::None);
    let r = h.clean(&[]);
    rc(&r, 0);
    gone(format!("{wt}/target"));
    lacks(&r.out, "git could not name the gitdir");
    has(&r.out, &format!("removed {wt}/target: build output of #7"));
}

#[test]
fn a_worktree_whose_git_names_the_containers_path_and_whose_record_is_young_is_kept() {
    let h = Host::new("ct-young");
    h.workspace(7);
    let wt = h.container_worktree(Gitdir::Dir);
    touch(h.ws("-7/.git/worktrees/issue-7-default"));
    let r = h.clean(&[]);
    rc(&r, 0);
    there(format!("{wt}/target"));
    there(h.ws("-7/target"));
    has(&r.out, &format!("younger than {AFTER} hours"));
}

#[test]
fn a_worktree_whose_container_gitdir_leads_outside_the_work_dir_is_refused() {
    let h = Host::new("ct-link");
    h.workspace(7);
    mkdir(h.t("/outside"));
    let wt = h.container_worktree(Gitdir::Link(h.t("/outside")));
    let r = h.clean(&[]);
    rc(&r, 0);
    there(format!("{wt}/target"));
    there(h.ws("-7/target"));
    lacks(&r.out, "clean: removed");
}

#[test]
fn a_helper_a_worktrees_config_names_is_not_run_and_its_build_output_still_goes() {
    let h = Host::new("wt-helper");
    h.workspace(7);
    let wt = h.worktree("", "");
    let gd = h.gitdir(&wt);
    let fsmonitor = format!("touch {}; false", h.t("/ran-fsmonitor"));
    let ssh = format!("touch {}", h.t("/ran-ssh"));
    h.git(&gd, &["config", "core.fsmonitor", &fsmonitor]);
    h.git(&gd, &["config", "core.sshCommand", &ssh]);
    age(&[gd]);
    age(&[h.ws("-7")]);
    let r = h.clean(&[]);
    rc(&r, 0);
    gone(h.t("/ran-fsmonitor"));
    gone(h.t("/ran-ssh"));
    gone(format!("{wt}/console/target"));
    lacks(&r.out, "git could not name the gitdir");
    has(
        &r.out,
        &format!("removed {wt}/console/target: build output of #7"),
    );
}

#[test]
fn the_dry_run_names_the_new_roots_it_would_take_and_removes_none_of_them() {
    let h = Host::new("roots-dry");
    h.workspace(7);
    h.retry_tree(7);
    age(&[h.ws("-7"), h.ws("-7.before-retry")]);
    let wt = h.worktree("", "");
    let retry_wt = h.worktree(".before-retry", "");
    h.worktree(".before-retry", "issue-7-beta");
    let r = h.clean(&["--dry-run"]);
    rc(&r, 0);
    there(format!("{wt}/console/target"));
    there(format!("{retry_wt}/console/target"));
    there(h.ws("-7.before-retry/target"));
    there(h.ws("-7/target"));
    let w = h.wd();
    has(
        &r.out,
        &format!("would remove {wt}/console/target: build output of #7"),
    );
    has(
        &r.out,
        &format!("would remove {retry_wt}/console/target: build output of #7"),
    );
    has(
        &r.out,
        &format!("would remove {w}-7.before-retry/target: build output of #7"),
    );
    has(
        &r.out,
        &format!("would remove {w}-7/target: build output of #7"),
    );
    lacks(&r.out, "clean: removed");
}
