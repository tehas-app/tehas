//! `tehasbot preview`: the previews' network and Docker's version, a new
//! title, git's log, and the free-space floor. Run as a host runs it, on the
//! stubs of console/tests/preview/. Moved from test/preview_network_test.sh.

mod common;
#[macro_use]
#[path = "preview/mod.rs"]
mod preview;

use common::*;
use preview::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;

/// With that one thing wrong, nothing is built or run, GitHub is not asked,
/// and the line names what is wrong (the shell's `unsafe`).
fn unsafe_network(w: &World, fixture: &str, content: &str, reason: &str) {
    reset(w);
    let a = commit(w, "feature");
    pulls(w, &[&pull(w, 12, &a)]);
    put(w, fixture, &format!("{content}\n"));
    let r = preview(w, "");
    rc(&r, 0);
    has(
        &r.out,
        &format!(
            "the network tehas-previews does not keep the host out ({reason}); nothing was built"
        ),
    );
    assert_eq!(format!("{} {}", builds(w), runs(w)), "0 0");
    assert_eq!(gh_calls(w), "");
    assert_eq!(state_of(w, "12"), "");
    assert_eq!(grep_c(&docker(w), "^network create"), 0);
}

#[test]
fn a_network_that_is_not_internal_gets_nothing_built() {
    let w = host("net-internal");
    unsafe_network(&w, "net-internal", "false", "it is not internal");
}

#[test]
fn a_network_that_cannot_be_read_gets_nothing_built() {
    let w = host("net-unread");
    unsafe_network(&w, "net-inspect-fails", "", "it could not be read");
}

#[test]
fn a_network_that_is_internal_and_gives_the_host_an_address_gets_nothing_built() {
    let w = host("net-gateway");
    unsafe_network(&w, "net-gateway", "", "it is not isolated from the host");
    unsafe_network(&w, "net-gateway", "nat", "it is not isolated from the host");
}

#[test]
fn a_network_with_ipv6_gets_nothing_built_the_host_has_an_address_of_that_kind_on_it() {
    let w = host("net-ipv6");
    unsafe_network(&w, "net-ipv6", "true", "it has IPv6");
    unsafe_network(&w, "net-ipv6", "", "it has IPv6");
}

#[test]
fn a_network_of_another_driver_gets_nothing_built_whatever_option_it_echoes() {
    let w = host("net-driver");
    unsafe_network(&w, "net-driver", "ipvlan", "its driver is not bridge");
}

/// A Docker too old for the option that keeps the host out.
fn docker_too_old(old: &str) {
    let w = host("docker-old");
    unsafe_network(
        &w,
        "docker-version",
        old,
        &format!("Docker {old} is older than 27.1"),
    );
}

cases!(docker_too_old {
    docker_26_1_4_does_not_know_the_option_that_keeps_the_host_out_nothing_is_built: "26.1.4";
    docker_27_0_3_does_not_know_the_option_that_keeps_the_host_out_nothing_is_built: "27.0.3";
    docker_27_0_does_not_know_the_option_that_keeps_the_host_out_nothing_is_built: "27.0";
    docker_19_03_15_does_not_know_the_option_that_keeps_the_host_out_nothing_is_built: "19.03.15";
});

/// A Docker version that is no version.
fn docker_version_odd(odd: &str) {
    let w = host("docker-odd");
    unsafe_network(
        &w,
        "docker-version",
        odd,
        "Docker's version could not be read",
    );
}

cases!(docker_version_odd {
    a_docker_version_of_is_not_one_nothing_is_built: "";
    a_docker_version_of_latest_is_not_one_nothing_is_built: "latest";
    a_docker_version_of_27_is_not_one_nothing_is_built: "27";
    a_docker_version_of_27_x_1_is_not_one_nothing_is_built: "27.x.1";
    a_docker_version_of_v29_1_3_is_not_one_nothing_is_built: "v29.1.3";
    a_docker_version_of_29_1_3_id_is_not_one_nothing_is_built: "29.1.3; id";
});

#[test]
fn a_docker_that_does_not_say_its_version_gets_nothing_built() {
    let w = host("docker-silent");
    unsafe_network(
        &w,
        "version-fails",
        "",
        "Docker's version could not be read",
    );
}

/// A Docker that is new enough.
fn docker_new_enough(new: &str) {
    let w = host("docker-new");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    put(&w, "docker-version", &format!("{new}\n"));
    preview(&w, "");
    assert_eq!(state_of(&w, "12"), "serving");
}

cases!(docker_new_enough {
    docker_27_1_0_is_new_enough: "27.1.0";
    docker_27_10_0_is_new_enough: "27.10.0";
    docker_28_0_1_is_new_enough: "28.0.1";
    docker_29_1_3_is_new_enough: "29.1.3";
    docker_100_0_is_new_enough: "100.0";
    docker_27_1_0_rc1_is_new_enough: "27.1.0-rc1";
});

#[test]
fn an_old_docker_is_not_asked_to_make_the_network() {
    let w = host("old-no-create");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    touch(&w, "net-missing");
    put(&w, "docker-version", "26.1.4\n");
    let r = preview(&w, "");
    has(
        &r.out,
        "(Docker 26.1.4 is older than 27.1); nothing was built",
    );
    assert_eq!(grep_c(&docker(&w), "^network create"), 0);
}

#[test]
fn a_host_without_the_network_gets_it_internal_and_with_no_address_for_the_host() {
    let w = host("net-create");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    touch(&w, "net-missing");
    preview(&w, "");
    assert_eq!(
        grep(&docker(&w), "^network create"),
        "network create --internal --ipv6=false -o com.docker.network.bridge.gateway_mode_ipv4=isolated tehas-previews"
    );
    assert_eq!(state_of(&w, "12"), "serving");
}

#[test]
fn a_console_that_does_not_answer_gets_nothing_built() {
    let w = host("console-silent");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    let c = console(&w);
    let kept = w.t().join("console.kept");
    fs::rename(&c, &kept).unwrap();
    fs::write(&c, "#!/bin/sh\nexit 9\n").unwrap();
    fs::set_permissions(&c, fs::Permissions::from_mode(0o755)).unwrap();
    let r = preview(&w, "");
    fs::remove_file(&c).unwrap();
    fs::rename(&kept, &c).unwrap();
    rc(&r, 9);
    assert_eq!(format!("{} {}", builds(&w), runs(&w)), "0 0");
    assert_eq!(docker(&w), "");
}

#[test]
fn a_pull_request_that_got_a_new_title_has_it_in_its_state_and_the_state_began_when_it_did() {
    let w = host("title");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    preview(&w, "");
    let state = pdir(&w).join("12/state.json");
    jq_edit(&w, &[], r#".time = "2026-01-01T00:00:00Z""#, &state);
    let renamed = jq_run(
        &["-c", r#".title = "A better name""#],
        Some(&format!("{}\n", pull(&w, 12, &a))),
    );
    pulls(&w, &[&renamed]);
    rm_calls(&w, "docker");
    rm_calls(&w, "gh-writes");
    let r = preview(&w, "");
    assert_eq!(
        jq_r(r#""\(.state) \(.title) \(.time) \(.sha)""#, &state),
        format!("serving A better name 2026-01-01T00:00:00Z {a}")
    );
    assert_eq!(
        jq_r(".previews[0].title", &previews_json(&w)),
        "A better name"
    );
    assert_eq!(format!("{} {}", builds(&w), runs(&w)), "0 0");
    assert_eq!(writes(&w), "");
    lacks(&r.out, "#12");
}

#[test]
fn a_new_title_reaches_a_state_that_is_not_serving_too_and_nothing_is_built_for_it() {
    let w = host("title-failed");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    touch(&w, "build-fails");
    preview(&w, "");
    unput(&w, "build-fails");
    let state = pdir(&w).join("12/state.json");
    let before = jq_file(&["-c"], "del(.title)", &state);
    let renamed = jq_run(
        &["-c", r#".title = "A better name""#],
        Some(&format!("{}\n", pull(&w, 12, &a))),
    );
    pulls(&w, &[&renamed]);
    rm_calls(&w, "docker");
    preview(&w, "");
    assert_eq!(
        jq_r(r#""\(.state) \(.title)""#, &state),
        "build-failed A better name"
    );
    assert_eq!(jq_file(&["-c"], "del(.title)", &state), before);
    assert_eq!(builds(&w), 0);
    // A state that cannot be read is not a reason to stop: the commit is built.
    fs::write(&state, "not json\n").unwrap();
    let r = preview(&w, "");
    rc(&r, 0);
    assert_eq!(
        jq_r(r#""\(.state) \(.title)""#, &state),
        "serving A better name"
    );
}

/// The git of the test's PATH, by its path.
fn real_git() -> String {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .map(|d| std::path::Path::new(d).join("git"))
        .find(|p| p.is_file())
        .expect("git on the PATH")
        .display()
        .to_string()
}

#[test]
fn what_git_says_of_an_archive_that_fails_is_in_the_pull_request_s_own_log() {
    let w = host("archive-fails");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    w.stub(
        "git",
        &format!(
            "case \" $* \" in *\" archive \"*) echo 'fatal: the archive broke' >&2; exit 128 ;; esac\nexec '{}' \"$@\"",
            real_git()
        ),
    );
    let r = preview(&w, "");
    fs::remove_file(w.t().join("bin/git")).unwrap();
    assert_eq!(state_of(&w, "12"), "build-failed");
    has(&cat(&log_of(&w, "12")), "fatal: the archive broke");
    lacks(&r.out, "fatal: the archive broke");
    assert_eq!(runs(&w), 0);
}

#[test]
fn a_pull_request_number_that_is_not_a_whole_number_is_never_built() {
    let w = host("number");
    let a = commit(&w, "feature");
    pulls(
        &w,
        &[&format!(
            r#"{{"number":12.5,"title":"x","html_url":"u","author_association":"MEMBER","head":{{"sha":"{a}","repo":{{"full_name":"acme/widget"}}}}}}"#
        )],
    );
    preview(&w, "");
    assert_eq!(builds(&w), 0);
    assert_eq!(count(&pdir(&w), "12*"), 0);
}

#[test]
fn a_run_that_died_while_building_is_built_again() {
    let w = host("died");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    fs::create_dir_all(pdir(&w).join("12")).unwrap();
    let state = jq_run(
        &[
            "-n",
            "--arg",
            "sha",
            &a,
            r#"{number: 12, sha: $sha, state: "building", reason: "", time: "x"}"#,
        ],
        None,
    );
    fs::write(pdir(&w).join("12/state.json"), format!("{state}\n")).unwrap();
    preview(&w, "");
    assert_eq!(state_of(&w, "12"), "serving");
}

/// The first floor case: 1 GB free. Its commit.
fn under_the_floor(w: &World) -> String {
    let a = commit(w, "feature");
    pulls(w, &[&pull(w, 12, &a)]);
    put(w, "free", "1048576\n"); // 1 GB
    preview(w, "");
    assert_eq!(builds(w), 0);
    assert_eq!(state_of(w, "12"), "no-space");
    has(&reason_of(w, "12"), "1.0 GB is free and 5.0 GB is needed");
    assert_eq!(
        jq_file(&["-cS"], ".disk", &previews_json(w)),
        r#"{"freeBytes":1073741824,"low":true,"minFreeBytes":5368709120}"#
    );
    a
}

/// With room again, after the case before.
fn room_again(w: &World) {
    unput(w, "free");
    preview(w, "");
    assert_eq!(state_of(w, "12"), "serving");
    assert_eq!(jq_r(".disk.low", &previews_json(w)), "false");
}

/// Under the floor again, serving, after the cases before.
fn serving_under_the_floor(w: &World, a: &str) {
    put(w, "free", "1048576\n");
    preview(w, "");
    assert_eq!(state_of(w, "12"), "serving");
    assert_eq!(container_of(w, "12"), a);
}

#[test]
fn under_the_floor_nothing_is_built_and_the_state_says_how_much_is_free() {
    let w = host("floor");
    under_the_floor(&w);
}

#[test]
fn with_room_again_the_same_commit_is_built() {
    let w = host("floor-room");
    under_the_floor(&w);
    room_again(&w);
}

#[test]
fn a_preview_already_serving_stays_up_under_the_floor() {
    let w = host("floor-serving");
    let a = under_the_floor(&w);
    room_again(&w);
    serving_under_the_floor(&w, &a);
}

#[test]
fn removal_still_runs_under_the_floor() {
    let w = host("floor-removal");
    let a = under_the_floor(&w);
    room_again(&w);
    serving_under_the_floor(&w, &a);
    close(&w, 12);
    pulls(&w, &[]);
    preview(&w, "");
    assert_eq!(count(&fx_file(&w, "docker"), "container-12"), 0);
    assert_eq!(count(&pdir(&w), "12"), 0);
}

#[test]
fn space_that_cannot_be_read_builds_nothing_and_says_so() {
    let w = host("df-fails");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    touch(&w, "df-fails");
    preview(&w, "");
    assert_eq!(builds(&w), 0);
    assert_eq!(state_of(&w, "12"), "no-space");
    has(&reason_of(&w, "12"), "could not be read");
    let got: String = jq_file(&["-c"], ".disk.freeBytes, .disk.low", &previews_json(&w))
        .lines()
        .map(|l| format!("{l} "))
        .collect();
    assert_eq!(got, "null true ");
}

#[test]
fn the_figures_are_written_with_nothing_to_build() {
    let w = host("figures");
    pulls(&w, &[]);
    preview(&w, "");
    assert_eq!(jq_r(".disk.freeBytes", &previews_json(&w)), "42949672960");
}
