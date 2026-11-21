//! tehas update from the platform checkout: what an update is asked to do,
//! and the follow-ups it owes.

#[path = "update/mod.rs"]
mod common;

use common::{git, has, lacks, s, uid, Host, Run};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// A host with the minimal instance and a platform `behind` commits behind.
fn host(behind: u32, body: &str) -> Host {
    let mut h = Host::new("checkout");
    h.instance();
    h.platform(behind, body);
    h
}

fn chmod(p: &Path, mode: u32) {
    fs::set_permissions(p, fs::Permissions::from_mode(mode)).unwrap();
}

fn branch(h: &Host) -> String {
    git(
        Some(&h.checkout),
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
    )
}

const REPORT_OK: &str = r#"echo "Instance:     $TEHAS_INSTANCE_DIR OK"
echo "Installed:    OK""#;

#[test]
fn check_reports_follow_ups_an_earlier_run_left_undone_and_starts_none() {
    let h = host(0, REPORT_OK);
    h.record(&["edge --force-recreate"]);
    let before = h.head_sha();
    let r = h.update(&["--check"]);
    assert_eq!(r.rc, 3, "{}", r.out);
    has(&r.out, "follow-up(s) an earlier run did not finish");
    has(&r.out, "next: edge --force-recreate");
    has(&r.out, "Installed:    OK");
    assert_eq!(h.head_sha(), before);
    assert_eq!(h.calls("docker"), "");
    // Nothing was started and nothing was taken back off the list.
    assert_eq!(h.owed_steps(), "edge --force-recreate");
}

#[test]
fn a_checkout_on_a_branch_that_is_not_the_remotes_default_is_not_pulled_or_installed() {
    let h = host(0, "echo \"installed: nothing\"");
    h.push_branch("other");
    git(Some(&h.checkout), &["fetch", "-q", "origin"]);
    git(Some(&h.checkout), &["checkout", "-q", "other"]);
    let before = h.head_sha();
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        &format!(
            "checkout: {} is on branch other; tehas update installs only the default branch of origin, main",
            s(&h.checkout)
        ),
    );
    has(&r.out, "run ./install.sh in it by hand to install other");
    assert_eq!(branch(&h), "other");
    assert_eq!(h.head_sha(), before);
    assert_eq!(h.calls("install"), "");
}

#[test]
fn a_checkout_that_does_not_know_the_remotes_default_branch_is_not_pulled() {
    let h = host(1, "echo \"installed: nothing\"");
    git(
        Some(&h.checkout),
        &["symbolic-ref", "--delete", "refs/remotes/origin/HEAD"],
    );
    let before = h.head_sha();
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "does not know the default branch of origin; nothing was changed",
    );
    has(&r.out, "remote set-head origin --auto");
    assert_eq!(h.head_sha(), before);
    assert_eq!(h.calls("install"), "");
}

#[test]
fn a_dirty_checkout_changes_nothing_and_runs_no_install_sh() {
    let h = asking(0);
    let before = h.state();
    fs::write(h.checkout.join("libexec/new-thing"), "half-finished work\n").unwrap();
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "has local changes; nothing was changed");
    has(&r.out, "libexec/new-thing");
    assert_eq!(h.state(), before);
    assert_eq!(h.calls("install"), "");
    assert_eq!(h.calls("build"), "");
}

/// A host asking for every follow-up, `behind` commits behind.
fn asking(behind: u32) -> Host {
    let mut h = Host::new("checkout");
    h.instance();
    let ask = h.ask_all();
    h.platform(behind, &ask);
    h
}

/// A held platform one commit behind, and its state before.
fn held() -> (Host, String) {
    let h = asking(1);
    let before = h.state();
    fs::create_dir_all(h.home.join("state")).unwrap();
    fs::write(h.home.join("state/update-hold"), "").unwrap();
    (h, before)
}

#[test]
fn a_held_platform_is_not_fetched_merged_or_installed_and_the_hold_is_named() {
    let (h, before) = held();
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, &format!("{}/state/update-hold exists", s(&h.home)));
    assert_eq!(h.state(), before);
    assert_eq!(h.calls("timeout"), "");
    assert_eq!(h.calls("install"), "");
    assert_eq!(h.calls("build"), "");
}

#[test]
fn check_on_a_held_platform_says_so_and_still_reports() {
    let (h, before) = held();
    let r = h.update(&["--check"]);
    assert_eq!(r.rc, 3, "{}", r.out);
    has(
        &r.out,
        "an update will not move the checkout until it is removed",
    );
    has(&r.out, "checkout: would fast-forward");
    assert_eq!(
        h.state().split(' ').nth(1),
        before.split(' ').nth(1),
        "{}",
        r.out
    );
}

#[test]
fn a_checkout_with_commits_of_its_own_is_not_fast_forwarded() {
    let h = asking(0);
    fs::write(h.checkout.join("notes"), "local\n").unwrap();
    git(Some(&h.checkout), &["add", "-A"]);
    git(Some(&h.checkout), &["commit", "-qm", "local work"]);
    h.push_one("remote work");
    let before = h.head_sha();
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "cannot fast-forward to origin/main");
    assert_eq!(h.head_sha(), before);
    assert_eq!(h.calls("install"), "");
}

#[test]
fn a_remote_that_cannot_be_reached_stops_before_install_sh() {
    let h = host(0, "echo \"installed: nothing\"");
    let nowhere = s(&h.join("no-such-place"));
    git(
        Some(&h.checkout),
        &["remote", "set-url", "origin", &nowhere],
    );
    let before = h.head_sha();
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "could not fetch from origin; nothing was changed");
    assert_eq!(h.head_sha(), before);
    assert_eq!(h.calls("install"), "");
}

#[test]
fn a_checkout_on_a_branch_with_no_commit_yet_says_what_is_missing() {
    let h = host(0, "echo \"installed: nothing\"");
    git(
        Some(&h.checkout),
        &["checkout", "-q", "--orphan", "brand-new"],
    );
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "on branch brand-new with no commit yet; nothing was changed",
    );
    lacks(&r.out, "cannot fast-forward");
    assert_eq!(branch(&h), "brand-new");
    assert_eq!(h.calls("install"), "");
}

#[test]
fn a_detached_checkout_is_refused_rather_than_pulled_into() {
    let h = host(0, "echo \"installed: nothing\"");
    git(Some(&h.checkout), &["checkout", "-q", "--detach", "HEAD"]);
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "not on a branch");
    assert_eq!(h.calls("install"), "");
}

#[test]
fn a_failed_install_stops_before_the_follow_ups() {
    let h = host(
        2,
        r#"echo "installed: libexec/dispatch"
echo "tehas: /x is missing from this checkout" >&2
exit 1"#,
    );
    let before = h.head_sha();
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "install: install.sh exited 1; no follow-up was run");
    // The pull happened: the checkout moved, and nothing after install.sh ran.
    assert_ne!(
        h.head_sha(),
        before,
        "the checkout did not move to origin/main"
    );
    assert_eq!(h.calls("install"), "install ");
    assert_eq!(h.calls("build"), "");
    assert_eq!(h.calls("docker"), "");
}

#[test]
fn invalid_instance_settings_found_by_install_sh_are_passed_through_as_78() {
    let h = host(
        0,
        r#"echo "tehas: models.json does not list model x" >&2
exit 78"#,
    );
    let r = h.update(&[]);
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "install: install.sh exited 78");
    assert_eq!(h.calls("build"), "");
}

#[test]
fn settings_the_pulled_checkout_adds_are_answered_before_anything_is_installed() {
    let h = asking(1);
    h.push_settings_change();
    let r = h.update(&[]);
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "the checkout this update pulled asks for settings this instance does not have",
    );
    has(&r.out, "TEHAS_NEW_SETTING is not set");
    assert_eq!(h.calls("install"), "");
    assert_eq!(h.calls("build"), "");
    assert_eq!(h.calls("docker"), "");
}

#[test]
fn a_checkout_whose_config_sh_predates_the_settings_helpers_defers_to_install_sh() {
    let h = host(1, "echo \"installed: nothing\"");
    h.push_old_config();
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "does not define tehas_load; install.sh will check the settings itself",
    );
    lacks(&r.out, "asks for settings this instance does not have");
    assert_eq!(h.calls("install"), "install ");
}

#[test]
fn a_pulled_checkout_has_no_console_beside_it_so_install_sh_checks_the_settings() {
    let h = host(1, "echo \"installed: nothing\"");
    h.push_settings_change();
    fs::create_dir_all(h.home.join("libexec")).unwrap();
    fs::copy(h.console(), h.home.join("libexec/console")).unwrap();
    let r = h.sh(r#"unset TEHAS_SETTINGS_CMD; "$TEHAS_CONSOLE" update run "$TEHAS_CONSOLE""#);
    has(
        &r.out,
        "this checkout has no console beside it; install.sh will check the settings itself",
    );
    lacks(&r.out, "asks for settings this instance does not have");
    assert_eq!(h.calls("install"), "install ");
}

#[test]
fn check_reports_what_would_change_and_leaves_the_checkout_as_it_was() {
    let mut h = Host::new("checkout");
    h.instance();
    let body = format!(
        r#"echo "Instance:     $TEHAS_INSTANCE_DIR OK"
echo "Installed:    DRIFT (1 files)"
echo "  differs: {}/libexec/dispatch"
exit 1"#,
        s(&h.home)
    );
    h.platform(2, &body);
    let before = h.head_sha();
    let r = h.update(&["--check"]);
    assert_eq!(r.rc, 3, "{}", r.out);
    has(&r.out, "checkout: would fast-forward");
    has(&r.out, "DRIFT (1 files)");
    lacks(&r.out, "installed:");
    // install.sh --check answers 1 when it found drift. That is its report,
    // not a failed step: in check mode nothing was meant to run.
    lacks(&r.out, "no follow-up was run");
    has(
        &r.out,
        "install: install.sh --check found something to update",
    );
    lacks(&r.out, "does not fix");
    // Nothing moved: no merge, no file changed, nothing started.
    assert_eq!(h.head_sha(), before);
    assert_eq!(git(Some(&h.checkout), &["status", "--porcelain"]), "");
    assert_eq!(h.calls("install"), "install --check");
    assert_eq!(h.calls("build"), "");
    assert_eq!(h.calls("docker"), "");
}

#[test]
fn check_exits_3_when_only_the_checkout_is_behind() {
    let h = host(2, REPORT_OK);
    let before = h.head_sha();
    let r = h.update(&["--check"]);
    assert_eq!(r.rc, 3, "{}", r.out);
    has(&r.out, &format!("checkout: would fast-forward {before}"));
    has(&r.out, "Installed:    OK");
    assert_eq!(h.head_sha(), before);
    assert_eq!(h.calls("install"), "install --check");
}

#[test]
fn check_names_what_an_update_does_not_fix_apart_from_something_to_update_exit_1() {
    for finding in [
        "Dependencies: NOT OK",
        "Slice:        MISSING (sudo ./bootstrap.sh slice)",
        "Login:        NOT CURRENT",
        "Tests:        FAILED",
        "Cycle token:  CANNOT PUSH (acme-factory[bot] was refused a dry-run push to origin)",
    ] {
        let h = host(
            0,
            &format!(
                r#"echo "Instance:     $TEHAS_INSTANCE_DIR OK"
echo "{finding}"
echo "Installed:    OK"
exit 1"#
            ),
        );
        let r = h.update(&["--check"]);
        assert_eq!(r.rc, 1, "{}", r.out);
        has(&r.out, finding);
        has(
            &r.out,
            "install: install.sh --check found something tehas update does not fix",
        );
        lacks(&r.out, "found something to update");
    }
}

#[test]
fn check_on_a_host_that_is_up_to_date_reports_it_and_exits_0() {
    let h = host(0, REPORT_OK);
    let before = h.head_sha();
    let r = h.update(&["--check"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        &format!("checkout: main is up to date at {}", h.head_sha()),
    );
    assert_eq!(h.head_sha(), before);
    assert_eq!(h.calls("install"), "install --check");
}

#[test]
fn a_follow_up_that_names_no_step_this_command_knows_is_said_out_loud_not_run() {
    let h = host(
        0,
        r#"echo "next: rm -rf something"
echo "next: docker compose -f /somewhere/else.compose.yaml up -d""#,
    );
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "WARN: 2 step line(s) name no step this command knows",
    );
    has(&r.out, "next: rm -rf something");
    assert_eq!(h.calls("docker"), "");
    assert_eq!(h.calls("build"), "");
}

#[test]
fn a_step_line_whose_names_are_not_unit_names_never_reaches_systemctl() {
    let h = host(0, r#"echo "next: units --help tehas-dispatch.timer""#);
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "WARN: 1 step line(s) name no step this command knows",
    );
    has(&r.out, "next: units --help tehas-dispatch.timer");
    assert_eq!(h.calls("systemctl"), "");
}

#[test]
fn a_units_step_that_names_no_unit_is_not_run() {
    let h = host(0, r#"echo "next: units""#);
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "WARN: 1 step line(s) name no step this command knows",
    );
    has(&r.out, "next: units");
    assert_eq!(h.calls("systemctl"), "");
}

#[test]
fn a_step_asking_for_an_argument_it_was_not_given_is_not_run() {
    let h = host(0, r#"echo "next: docs restart""#);
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "WARN: 1 step line(s) name no step this command knows",
    );
    assert_eq!(h.calls("docker"), "");
}

// docker compose up -d without --force-recreate leaves a running container
// serving the config it loaded at its start, so a bare docs would renew
// nothing and be counted as done.
#[test]
fn a_compose_step_without_its_flag_is_not_run_and_stays_owed() {
    let h = host(0, r#"echo "next: docs""#);
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "WARN: 1 step line(s) name no step this command knows",
    );
    has(&r.out, "next: docs");
    assert_eq!(h.calls("docker"), "");
    assert_eq!(h.owed_steps(), "docs");
}

#[test]
fn install_shs_diagnostics_are_not_read_as_follow_ups() {
    let h = host(
        0,
        r#"echo "installed: libexec/dispatch"
echo "next: sandbox" >&2
echo "tehas: a step on this host did not run" >&2"#,
    );
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    // stderr stays stderr, and is not parsed for steps.
    has(&r.out, "tehas: a step on this host did not run");
    has(&r.out, "next: sandbox");
    lacks(&r.out, "WARN:");
    assert_eq!(h.calls("build"), "");
    assert_eq!(h.calls("docker"), "");
}

#[test]
fn a_status_of_install_sh_this_command_does_not_know_is_reported_as_1() {
    let h = host(
        0,
        r#"echo "bash: Permission denied" >&2
exit 126"#,
    );
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "install: install.sh exited 126; no follow-up was run",
    );
    assert_eq!(h.calls("build"), "");
    assert_eq!(h.calls("docker"), "");
}

#[test]
fn a_step_recorded_under_the_protolabs_old_id_renews_the_edge_router() {
    let h = host(1, "echo \"installed: nothing\"");
    h.record(&["protolab --force-recreate"]);
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "edge: updating the container");
    has(
        &h.calls("docker"),
        &format!(
            "compose -f {}/edge/router.compose.yaml up -d --force-recreate",
            s(&h.home)
        ),
    );
    assert_eq!(h.owed_steps(), "");
}

#[test]
fn a_compose_file_that_is_not_installed_stops_the_update() {
    let h = host(
        1,
        r#"echo "next: sandbox"
echo "next: edge --force-recreate""#,
    );
    fs::remove_file(h.home.join("edge/router.compose.yaml")).unwrap();
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        &format!(
            "edge: {}/edge/router.compose.yaml is not installed, left alone",
            s(&h.home)
        ),
    );
    lacks(&r.out, "edge: updating the container");
    assert_eq!(h.calls("docker"), "");
    // The build that came before the edge router ran, and the failure stopped
    // the run after it.
    assert_eq!(h.calls("build"), "build");
    assert_eq!(h.owed_steps(), "edge --force-recreate");
}

#[test]
fn a_sandbox_build_that_is_not_installed_stops_the_update() {
    let h = host(0, r#"echo "next: sandbox""#);
    chmod(&h.home.join("sandbox/build.sh"), 0o644);
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        &format!("sandbox: {}/sandbox/build.sh is not installed", s(&h.home)),
    );
    assert_eq!(h.calls("build"), "");
    assert_eq!(h.calls("docker"), "");
}

#[test]
fn a_branch_origin_does_not_have_stops_before_install_sh() {
    let h = host(0, "echo \"installed: nothing\"");
    let c = Some(h.checkout.as_path());
    git(c, &["checkout", "-q", "-b", "local-only"]);
    // A default branch the remote has since lost, so the branch passes as the
    // default.
    git(
        c,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/local-only",
        ],
    );
    let before = h.head_sha();
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        &format!(
            "checkout: origin has no branch local-only in {}; nothing was changed",
            s(&h.checkout)
        ),
    );
    assert_eq!(branch(&h), "local-only");
    assert_eq!(h.head_sha(), before);
    assert_eq!(h.calls("install"), "");
}

/// A run with `p` at `mode`, put back to `back` before anything is checked.
fn run_with_mode(h: &Host, p: &Path, mode: u32, back: u32) -> Run {
    chmod(p, mode);
    let r = h.update(&[]);
    chmod(p, back);
    r
}

// An unreadable record is not "nothing owed": the next write would replace
// steps somebody else recorded.
#[test]
fn a_follow_up_record_that_cannot_be_read_stops_the_update_before_anything_runs() {
    let h = asking(1);
    let before = h.state();
    h.record(&["sandbox", "docs --force-recreate"]);
    let r = run_with_mode(&h, &h.owed, 0o000, 0o644);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        &format!("cannot read the follow-up record {}", s(&h.owed)),
    );
    has(&r.out, "nothing was run");
    // Nothing ran: no merge, no install.sh, no step.
    assert_eq!(h.state(), before);
    assert_eq!(h.calls("install"), "");
    assert_eq!(h.calls("build"), "");
    assert_eq!(h.calls("docker"), "");
    // The steps that were owed are still there.
    assert_eq!(h.owed_steps(), "sandbox\ndocs --force-recreate");
}

#[test]
fn a_record_that_turned_unreadable_after_install_sh_stops_the_follow_ups() {
    let h = host(
        1,
        r#"echo "next: sandbox"
chmod 000 "$TEHAS_HOME/state/update-pending""#,
    );
    h.record(&["docs --force-recreate"]);
    let r = h.update(&[]);
    chmod(&h.owed, 0o644);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "cannot be read; no follow-up was run");
    assert_eq!(h.calls("build"), "");
    assert_eq!(h.calls("docker"), "");
    assert_eq!(h.owed_steps(), "docs --force-recreate");
}

#[test]
fn a_record_that_cannot_be_written_stops_the_run_before_any_step() {
    let h = host(1, r#"echo "next: sandbox""#);
    let state = h.home.join("state");
    fs::create_dir_all(&state).unwrap();
    // The lock is there from an earlier run; only the record cannot be written.
    fs::write(state.join("update.lock"), "").unwrap();
    let r = run_with_mode(&h, &state, 0o500, 0o755);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        &format!("could not make a temporary file beside {}", s(&h.owed)),
    );
    assert_eq!(h.calls("build"), "");
}

#[test]
fn a_state_directory_where_the_lock_cannot_be_made_stops_the_run_before_git() {
    let h = host(1, r#"echo "next: sandbox""#);
    let before = h.state();
    let state = h.home.join("state");
    fs::create_dir_all(&state).unwrap();
    let r = run_with_mode(&h, &state, 0o500, 0o755);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        &format!(
            "tehas update: cannot open {}/update.lock; nothing was changed",
            s(&state)
        ),
    );
    assert_eq!(h.state(), before);
    assert_eq!(h.calls("install"), "");
}

#[test]
fn a_failed_run_leaves_the_record_and_nothing_else_in_state() {
    let h = asking(1);
    h.docker_no_answer();
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "2 of 4 follow-up(s) are not done");
    // Each rewrite goes through a temporary file moved over the record, and
    // the lock is kept. The ids are what the install recorded.
    let mut names: Vec<String> = fs::read_dir(h.home.join("state"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["account-ids", "update-pending", "update.lock"]);
    assert_eq!(
        h.owed_steps(),
        "docs --force-recreate\nedge --force-recreate"
    );
}

#[test]
fn an_update_asks_github_for_the_ids_and_runs_on_when_they_are_the_recorded_ones() {
    let h = host(0, "echo \"installed: nothing\"");
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&h.calls("gh"), "api users/alice --jq .id");
    assert_eq!(h.calls("install"), "install ");
}

#[test]
fn an_id_that_changed_stops_the_update_with_78_before_anything_moves_and_accept_id_records_it() {
    let h = host(1, "echo \"installed: nothing\"");
    let before = h.head_sha();
    let ids = h.home.join("state/account-ids");
    fs::write(&ids, "alice 7\n").unwrap();
    let r = h.update(&[]);
    assert_eq!(r.rc, 78, "{}", r.out);
    let alice = uid("alice");
    has(
        &r.out,
        &format!("tehas: the login alice now has the user id {alice}, not the recorded 7"),
    );
    has(&r.out, "tehas update --accept-id alice");
    assert_eq!(h.head_sha(), before);
    assert_eq!(h.calls("install"), "");
    assert_eq!(fs::read_to_string(&ids).unwrap(), "alice 7\n");
    let r = h.update(&["--accept-id", "alice"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        &format!("accounts: alice is recorded with the user id {alice} (it was 7)"),
    );
    let line: Vec<String> = fs::read_to_string(&ids)
        .unwrap()
        .lines()
        .filter(|l| l.starts_with("alice "))
        .map(str::to_string)
        .collect();
    assert_eq!(line, [format!("alice {alice}")]);
    assert_eq!(h.calls("install"), "install ");
}

#[test]
fn accept_id_of_a_login_neither_setting_names_is_refused_with_64() {
    let h = host(0, "echo \"installed: nothing\"");
    let r = h.update(&["--accept-id", "mallory"]);
    assert_eq!(r.rc, 64, "{}", r.out);
    has(
        &r.out,
        "--accept-id mallory names no login of TEHAS_ALLOWED_LABELERS or the millwright",
    );
    assert_eq!(h.calls("install"), "");
}

#[test]
fn a_login_that_no_longer_resolves_stops_the_update_with_78_and_is_named() {
    let h = host(0, "echo \"installed: nothing\"");
    h.gh_fail(1, 1, "gh: Not Found (HTTP 404)");
    let r = h.update(&[]);
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "tehas: the login alice does not resolve to a GitHub account",
    );
    assert_eq!(h.calls("install"), "");
}
