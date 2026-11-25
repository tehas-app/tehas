//! `tehasbot preview`: settings, the first build and run, a new commit, a
//! failed build, the volumes a recipe declares, and no recipe. Run as a host
//! runs it, on the stubs of console/tests/preview/. Moved from
//! test/preview_build_test.sh.

mod common;
#[macro_use]
#[path = "preview/mod.rs"]
mod preview;

use common::*;
use preview::*;

#[test]
fn invalid_settings_exit_78_before_github_or_docker_is_asked() {
    let w = host("invalid");
    w.instance(&["TEHAS_CONTROL_ISSUE=1"]);
    let r = preview(&w, "");
    rc(&r, 78);
    assert_eq!(format!("{}{}", gh_calls(&w), docker(&w)), "");
}

#[test]
fn an_instance_without_previews_exit_78_nothing_asked_nothing_written() {
    let w = host("no-previews");
    w.minimal(&["TEHAS_DOMAIN=example.org"]);
    let r = preview(&w, "");
    rc(&r, 78);
    has(&r.out, "TEHAS_PREVIEWS is not 1");
    assert_eq!(format!("{}{}", gh_calls(&w), docker(&w)), "");
    assert_eq!(count(&w.home.join("preview"), "*"), 0);
}

#[test]
fn the_console_without_the_settings_exit_78_nothing_asked_nothing_written() {
    let w = host("no-settings");
    let c = console(&w);
    let c = c.display();
    let r = sh(
        &w,
        &format!("{c} preview run; echo run=$?; {c} preview sweep; echo sweep=$?"),
    );
    has(&r.out, "run=78");
    has(&r.out, "sweep=78");
    assert_eq!(format!("{}{}", gh_calls(&w), docker(&w)), "");
    assert_eq!(count(&w.home.join("preview"), "*"), 0);
}

#[test]
fn the_console_without_a_usable_domain_exit_78_nothing_asked_nothing_written() {
    let w = host("no-domain");
    let c = console(&w);
    let c = c.display();
    let settings = format!(
        "TEHAS_HOME={} TEHAS_REPO=acme/widget TEHAS_PREVIEWS=1 TEHAS_PREVIEW_MAX=5 TEHAS_PREVIEW_MIN_FREE_MB=1",
        w.home.display()
    );
    for domain in [
        "",
        "TEHAS_DOMAIN=",
        "TEHAS_DOMAIN=EXAMPLE.org",
        "TEHAS_DOMAIN=example",
        "TEHAS_DOMAIN=example.org/x",
        "TEHAS_DOMAIN=-a.example.org",
    ] {
        let r = sh(
            &w,
            &format!(
                "env {settings} {domain} {c} preview run; echo run=$?; env {settings} {domain} {c} preview sweep; echo sweep=$?"
            ),
        );
        has(&r.out, "run=78");
        has(&r.out, "sweep=78");
        has(&r.out, "TEHAS_DOMAIN");
    }
    assert_eq!(format!("{}{}", gh_calls(&w), docker(&w)), "");
    assert_eq!(count(&w.home.join("preview"), "*"), 0);
    // The same call with a domain gets past the check, so the 78 above is the domain's.
    let r = sh(
        &w,
        &format!("env {settings} TEHAS_DOMAIN=example.org {c} preview run; echo run=$?"),
    );
    lacks(&r.out, "run=78");
    has(&docker(&w), "info");
}

#[test]
fn an_argument_it_does_not_know_exit_64() {
    let w = host("unknown-arg");
    let r = preview(&w, "--everything");
    rc(&r, 64);
}

/// The first case: pull request 12 gets a preview. Its commit.
fn first_preview(w: &World) -> String {
    let a = commit(w, "feature");
    pulls(w, &[&pull(w, 12, &a)]);
    let r = preview(w, "");
    rc(&r, 0);
    assert_eq!(state_of(w, "12"), "serving");
    assert_eq!(
        run_args(w, "12"),
        format!("run --detach --name tehas-preview-12 --network tehas-previews --read-only --tmpfs /tmp --tmpfs /config --tmpfs /data --cap-drop ALL --cap-add NET_BIND_SERVICE --security-opt no-new-privileges:true --memory 256m --cpus 1 --pids-limit 128 --log-opt max-size=10m --log-opt max-file=2 --restart unless-stopped --label tehas.preview=12 --label tehas.sha={a} --label tehas.repo=acme/widget tehas-preview:12 ")
    );
    has(&r.out, "#12: serving");
    lacks(&r.out, "No such file");
    a
}

/// An unchanged commit, after the first case.
fn unchanged(w: &World) {
    rm_calls(w, "docker");
    preview(w, "");
    assert_eq!(builds(w), 0);
    assert_eq!(runs(w), 0);
}

/// A new commit, after the one before. Its commit.
fn new_commit(w: &World) -> String {
    let old = cat(&fx_file(w, "docker/image-12"));
    let a2 = commit(w, "feature");
    pulls(w, &[&pull(w, 12, &a2)]);
    let r = preview(w, "");
    assert_eq!(container_of(w, "12"), a2);
    has(&docker(w), "rm -f -v tehas-preview-12");
    has(&docker(w), &format!("image rm {old}"));
    lacks(&r.out, "No such file");
    a2
}

#[test]
fn a_pull_request_of_the_repository_gets_a_preview_started_with_exactly_these() {
    let w = host("first");
    first_preview(&w);
}

#[test]
fn the_container_is_given_nothing_of_the_host() {
    let w = host("nothing-of-host");
    first_preview(&w);
    let file = w.t().join("calls/docker.run.12");
    let args = cat(&file);
    let lines = std::fs::read_to_string(&file).unwrap();
    for no in [
        "-v",
        "--volume",
        "--mount",
        "-e",
        "--env",
        "--env-file",
        "-p",
        "--publish",
        "--privileged",
        "--device",
        "--pid",
        "--network=host",
    ] {
        assert_eq!(lines.lines().filter(|l| *l == no).count(), 0, "{no}");
    }
    let home = w.t().join("home").display().to_string();
    for no in ["docker.sock", "sekret-token", ".ssh", ".config", &home] {
        lacks(&args, no);
    }
}

#[test]
fn the_build_reads_the_commit_as_an_archive_with_the_recipe_of_that_commit_and_no_token() {
    let w = host("archive");
    first_preview(&w);
    let b = grep(&docker(&w), "^build ");
    has(&b, "-f preview/Dockerfile");
    has(&b, "-t tehas-preview:12");
    lacks(&b, "sekret-token");
    lacks(&b, "--build-arg");
    lacks(&b, "--secret");
    assert_eq!(dockerfiles_in_context(&w), 1);
}

#[test]
fn the_build_has_no_network_and_runs_in_the_slice() {
    let w = host("no-network");
    first_preview(&w);
    let b = grep(&docker(&w), "^build ");
    has(
        &b,
        "build --quiet --network none --cgroup-parent tehas.slice ",
    );
    lacks(&b, "--network host");
    lacks(&b, "--network default");
}

#[test]
fn the_list_for_the_dashboard_has_the_address() {
    let w = host("list");
    first_preview(&w);
    assert_eq!(
        jq_r(
            r#".previews[0] | "\(.number) \(.state) \(.address) \(.title)""#,
            &previews_json(&w)
        ),
        "12 serving https://uat-12.example.org/ Change 12"
    );
}

#[test]
fn the_notice_gives_the_address_the_list_gives() {
    let w = host("notice");
    first_preview(&w);
    assert_eq!(
        jq_r(".previews[0].address", &previews_json(&w)),
        "https://uat-12.example.org/"
    );
    has(&writes(&w), "https://uat-12.example.org/");
}

#[test]
fn an_unchanged_commit_builds_nothing() {
    let w = host("unchanged");
    first_preview(&w);
    unchanged(&w);
}

#[test]
fn a_new_commit_is_built_and_the_old_container_and_image_go() {
    let w = host("new-commit");
    first_preview(&w);
    unchanged(&w);
    new_commit(&w);
}

#[test]
fn the_fetch_follows_the_product_checkout_s_origin_after_the_repository_moved() {
    let w = host("moved");
    first_preview(&w);
    unchanged(&w);
    new_commit(&w);
    let repo = pdir(&w).join("repo.git");
    let away = w.t().join("moved-away").display().to_string();
    git_in(&repo, &["remote", "set-url", "origin", &away]);
    let a3 = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a3)]);
    preview(&w, "");
    assert_eq!(
        git_in(&repo, &["config", "--get", "remote.origin.url"]),
        upstream(&w).display().to_string()
    );
    assert_eq!(container_of(&w, "12"), a3);
}

#[test]
fn the_label_is_the_commit_that_was_fetched_when_the_branch_moved_after_github_answered() {
    let w = host("label");
    let a = commit(&w, "feature");
    let listed = pull(&w, 12, &a);
    let moved = commit(&w, "feature");
    git_in(&upstream(&w), &["update-ref", "refs/pull/12/head", &moved]);
    pulls(&w, &[&listed]);
    preview(&w, "");
    assert_eq!(container_of(&w, "12"), moved);
}

#[test]
fn a_failed_build_keeps_the_old_preview_serving_and_is_not_tried_again_for_that_commit() {
    let w = host("failed-build");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    preview(&w, "");
    let a2 = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a2)]);
    touch(&w, "build-fails");
    preview(&w, "");
    assert_eq!(state_of(&w, "12"), "build-failed");
    assert_eq!(container_of(&w, "12"), a);
    has(&cat(&log_of(&w, "12")), "step 2 failed");
    rm_calls(&w, "docker");
    preview(&w, "");
    assert_eq!(builds(&w), 0);
    unput(&w, "build-fails");
    let a3 = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a3)]);
    preview(&w, "");
    assert_eq!(state_of(&w, "12"), "serving");
}

#[test]
fn a_recipe_that_declares_a_volume_elsewhere_is_not_run_and_the_old_preview_keeps_serving() {
    let w = host("volume-elsewhere");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    preview(&w, "");
    let a2 = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a2)]);
    put(&w, "volumes", "{\"/config\":{},\"/stuff\":{}}\n");
    rm_calls(&w, "docker");
    let r = preview(&w, "");
    rc(&r, 0);
    assert_eq!(state_of(&w, "12"), "build-failed");
    assert_eq!(
        reason_of(&w, "12"),
        "The recipe declares a volume outside /tmp, /config and /data. Its path is in logs/preview-12.log on the host."
    );
    assert_eq!(container_of(&w, "12"), a);
    assert_eq!(runs(&w), 0);
    assert_eq!(grep_c(&docker(&w), "^rm "), 0);
    // The image that was built goes, and the commit is not tried again.
    has(&docker(&w), "image rm tehas-preview:12");
    assert_eq!(count(&fx_file(&w, "docker"), "image-12"), 0);
    assert_eq!(count(&pdir(&w).join("12"), ".retry"), 0);
    // The path is text of the recipe's own. It stays in this pull request's build
    // log, and never reaches its description, where the bot's own account would be
    // quoting someone else's commit.
    has(
        &cat(&log_of(&w, "12")),
        "The recipe declares a volume at /stuff.",
    );
    lacks(&writes(&w), "/stuff");
    has(
        &writes(&w),
        "The recipe declares a volume outside /tmp, /config and /data",
    );
    rm_calls(&w, "docker");
    preview(&w, "");
    assert_eq!(builds(&w), 0);
}

/// Declared volumes that are not the three directories: nothing is run.
fn volumes_elsewhere(odd: &str) {
    let w = host("odd-volumes");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    put(&w, "volumes", &format!("{odd}\n"));
    preview(&w, "");
    assert_eq!(state_of(&w, "12"), "build-failed");
    has(&reason_of(&w, "12"), "The recipe declares a volume outside");
    assert_eq!(runs(&w), 0);
}

// Two of the shell's case names are the same once written as a name: the
// second of each pair ends in _2.
cases!(volumes_elsewhere {
    declared_volumes_of_tmp_are_not_the_three_directories_nothing_is_run: r#"{"/tmp/":{}}"#;
    declared_volumes_of_data_x_are_not_the_three_directories_nothing_is_run: r#"{"/data/x":{}}"#;
    declared_volumes_of_are_not_the_three_directories_nothing_is_run: r#"{"":{}}"#;
    declared_volumes_of_are_not_the_three_directories_nothing_is_run_2: r#"{"/":{}}"#;
    declared_volumes_of_var_lib_docker_data_are_not_the_three_directories_nothing_is_run: r#"{"/var/lib/docker":{},"/data":{}}"#;
});

#[test]
fn a_volume_s_path_reaches_the_build_log_without_its_control_characters_cut_short() {
    let w = host("volume-path");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    put(
        &w,
        "volumes",
        &format!("{{\"/a\\nb\\u001b[31m{}\":{{}}}}\n", "x".repeat(300)),
    );
    let r = preview(&w, "");
    assert_eq!(state_of(&w, "12"), "build-failed");
    assert_eq!(
        reason_of(&w, "12"),
        "The recipe declares a volume outside /tmp, /config and /data. Its path is in logs/preview-12.log on the host."
    );
    let log = cat(&log_of(&w, "12"));
    assert_eq!(grep_c(&log, "declares a volume at"), 1);
    has(
        &log,
        &format!(
            "The recipe declares a volume at /ab[31m{}. A preview may write to /tmp, /config and /data only.",
            "x".repeat(73)
        ),
    );
    lacks(&r.out, "\x1b");
    assert_eq!(grep_c(&r.out, "^#12: build-failed"), 1);
}

/// Declared volumes that what is in memory covers: the preview serves.
fn volumes_covered(fine: &str) {
    let w = host("fine-volumes");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    put(&w, "volumes", &format!("{fine}\n"));
    preview(&w, "");
    assert_eq!(state_of(&w, "12"), "serving");
    assert_eq!(runs(&w), 1);
}

cases!(volumes_covered {
    declared_volumes_of_null_are_covered_by_what_is_in_memory_the_preview_serves: "null";
    declared_volumes_of_config_data_are_covered_by_what_is_in_memory_the_preview_serves: r#"{"/config":{},"/data":{}}"#;
    declared_volumes_of_tmp_are_covered_by_what_is_in_memory_the_preview_serves: r#"{"/tmp":{}}"#;
    declared_volumes_of_are_covered_by_what_is_in_memory_the_preview_serves: "{}";
});

/// Volumes that cannot be read: nothing is run.
fn volumes_unread(odd: &str) {
    let w = host("unread-volumes");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    put(&w, "volumes", &format!("{odd}\n"));
    preview(&w, "");
    assert_eq!(state_of(&w, "12"), "build-failed");
    assert_eq!(
        reason_of(&w, "12"),
        "The image's volumes could not be read."
    );
    assert_eq!(runs(&w), 0);
    assert_eq!(count(&fx_file(&w, "docker"), "image-12"), 0);
}

cases!(volumes_unread {
    volumes_that_read_as_are_not_read_nothing_is_run: "";
    volumes_that_read_as_stuff_are_not_read_nothing_is_run: r#""/stuff""#;
    volumes_that_read_as_stuff_are_not_read_nothing_is_run_2: r#"["/stuff"]"#;
    volumes_that_read_as_not_json_are_not_read_nothing_is_run: "not json";
    volumes_that_read_as_true_are_not_read_nothing_is_run: "true";
    volumes_that_read_as_null_null_are_not_read_nothing_is_run: "null null";
});

#[test]
fn volumes_docker_does_not_tell_are_not_read_nothing_is_run() {
    let w = host("volumes-fail");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    touch(&w, "volumes-fails");
    preview(&w, "");
    assert_eq!(state_of(&w, "12"), "build-failed");
    assert_eq!(
        reason_of(&w, "12"),
        "The image's volumes could not be read."
    );
    assert_eq!(runs(&w), 0);
}

#[test]
fn the_volumes_are_asked_for_after_the_build_and_before_anything_is_removed_or_run() {
    let w = host("volumes-order");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    preview(&w, "");
    let a2 = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a2)]);
    rm_calls(&w, "docker");
    preview(&w, "");
    // grep -n -e '^build ' -e 'Config.Volumes' -e '^rm ' -e '^run ', then
    // sed 's/^[0-9]*:\([a-z]*\) .*/\1/' and tr '\n' ' '.
    let pick = regex::Regex::new("^build |Config.Volumes|^rm |^run ").unwrap();
    let word = regex::Regex::new("^[0-9]*:([a-z]*) .*").unwrap();
    let got: String = docker(&w)
        .lines()
        .enumerate()
        .filter(|(_, l)| pick.is_match(l))
        .map(|(i, l)| {
            let numbered = format!("{}:{l}", i + 1);
            format!("{} ", word.replace(&numbered, "$1"))
        })
        .collect();
    assert_eq!(got, "build image rm run ");
}

#[test]
fn a_log_that_cannot_be_written_stops_nothing() {
    let w = host("dev-full");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    let r = preview(&w, ">/dev/full");
    rc(&r, 0);
    assert_eq!(state_of(&w, "12"), "serving");
    has(&writes(&w), "https://uat-12.example.org/");
}

#[test]
fn a_commit_without_the_recipe_has_no_preview_and_an_older_one_of_it_stops() {
    let w = host("no-recipe");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    preview(&w, "");
    let a2 = commit_as(&w, "feature", "none");
    pulls(&w, &[&pull(&w, 12, &a2)]);
    preview(&w, "");
    assert_eq!(state_of(&w, "12"), "no-recipe");
    assert_eq!(count(&fx_file(&w, "docker"), "container-12"), 0);
    assert_eq!(jq_r(".previews[0].address", &previews_json(&w)), "null");
}
