//! `tehasbot preview` and the preview of the default branch: its build, its
//! names, its limits, GitHub not answering, and the sweep leaving it alone.
//! Run as a host runs it, on the stubs of console/tests/preview/. Moved from
//! test/preview_main_test.sh.

mod common;
#[path = "preview/mod.rs"]
mod preview;

use common::*;
use preview::*;

/// GitHub names main as the default branch. Without it a run has nothing to
/// say about the default branch.
fn with_main(w: &World) {
    put(w, "default-branch", "main\n");
}

/// The first case: the head of main gets a preview. Its commit.
fn head(w: &World) -> String {
    with_main(w);
    let m = commit(w, "main");
    pulls(w, &[]);
    let r = preview(w, "");
    rc(&r, 0);
    assert_eq!(state_of(w, "main"), "serving");
    assert_eq!(jq_r(".sha", &pdir(w).join("main/state.json")), m);
    has(&r.out, "main: serving");
    assert_eq!(
        run_args(w, "main"),
        format!("run --detach --name tehas-preview-main --network tehas-previews --read-only --tmpfs /tmp --tmpfs /config --tmpfs /data --cap-drop ALL --cap-add NET_BIND_SERVICE --security-opt no-new-privileges:true --memory 256m --cpus 1 --pids-limit 128 --log-opt max-size=10m --log-opt max-file=2 --restart unless-stopped --label tehas.preview=main --label tehas.sha={m} --label tehas.repo=acme/widget tehas-preview:main ")
    );
    has(&grep(&docker(w), "^build "), "-t tehas-preview:main");
    assert_eq!(dockerfiles_in_context(w), 1);
    assert_eq!(
        git_in(
            &pdir(w).join("repo.git"),
            &["rev-parse", "refs/preview/main"]
        ),
        m
    );
    assert!(log_of(w, "main").is_file());
    m
}

/// The second case: a pull request beside it.
fn with_a_pull_request(w: &World) {
    let a = commit(w, "feature");
    pulls(w, &[&pull(w, 12, &a)]);
    preview(w, "");
    assert_eq!(
        jq_r(
            r#".previews | map(.name // .number | tostring) | join(" ")"#,
            &previews_json(w)
        ),
        "main 12"
    );
    assert_eq!(
        jq_r(
            r#".previews[0] | "\(.number) \(.name) \(.state) \(.address)""#,
            &previews_json(w)
        ),
        "null main serving https://uat-main.example.org/"
    );
    assert_eq!(
        jq_r(".previews[1].address", &previews_json(w)),
        "https://uat-12.example.org/"
    );
}

/// An unchanged head, after the cases before.
fn unchanged_head(w: &World) {
    rm_calls(w, "docker");
    preview(w, "");
    assert_eq!(builds(w), 0);
    assert_eq!(runs(w), 0);
}

/// A new commit of main, after the cases before. Its commit.
fn new_head(w: &World) -> String {
    let old = cat(&fx_file(w, "docker/image-main"));
    let m2 = commit(w, "main");
    preview(w, "");
    assert_eq!(container_of(w, "main"), m2);
    has(&docker(w), &format!("image rm {old}"));
    m2
}

#[test]
fn the_head_of_the_default_branch_gets_a_preview_run_with_the_limits_of_a_pull_request_s() {
    let w = host("main-head");
    head(&w);
}

#[test]
fn the_list_for_the_dashboard_has_it_first_with_its_address_before_the_pull_requests() {
    let w = host("main-list");
    head(&w);
    with_a_pull_request(&w);
}

#[test]
fn a_pull_request_s_state_is_unchanged_by_it_a_number_and_no_name() {
    let w = host("main-number");
    head(&w);
    with_a_pull_request(&w);
    let state = pdir(&w).join("12/state.json");
    assert_eq!(jq_r(".number | type", &state), "number");
    assert_eq!(jq_r(r#"has("name")"#, &state), "false");
}

#[test]
fn an_unchanged_head_builds_nothing() {
    let w = host("main-unchanged");
    head(&w);
    with_a_pull_request(&w);
    unchanged_head(&w);
}

#[test]
fn a_new_commit_is_built_and_the_old_container_and_image_go() {
    let w = host("main-new");
    head(&w);
    with_a_pull_request(&w);
    unchanged_head(&w);
    new_head(&w);
}

#[test]
fn a_failed_build_keeps_the_old_preview_serving_and_is_not_tried_again_for_that_commit() {
    let w = host("main-failed");
    head(&w);
    with_a_pull_request(&w);
    unchanged_head(&w);
    let m2 = new_head(&w);
    commit(&w, "main");
    touch(&w, "build-fails");
    preview(&w, "");
    assert_eq!(state_of(&w, "main"), "build-failed");
    assert_eq!(container_of(&w, "main"), m2);
    rm_calls(&w, "docker");
    preview(&w, "");
    assert_eq!(builds(&w), 0);
    unput(&w, "build-fails");
    commit(&w, "main");
    preview(&w, "");
    assert_eq!(state_of(&w, "main"), "serving");
}

#[test]
fn a_head_without_the_recipe_has_no_main_preview_and_the_state_says_no_recipe() {
    let w = host("main-no-recipe");
    with_main(&w);
    commit_as(&w, "main", "norecipe");
    preview(&w, "");
    assert_eq!(state_of(&w, "main"), "no-recipe");
    assert_eq!(count(&fx_file(&w, "docker"), "*-main"), 0);
    assert_eq!(jq_r(".previews[0].address", &previews_json(&w)), "null");
}

#[test]
fn a_head_that_loses_the_recipe_takes_the_running_preview_down() {
    let w = host("main-loses");
    with_main(&w);
    commit(&w, "main");
    preview(&w, "");
    commit_as(&w, "main", "norecipe");
    preview(&w, "");
    assert_eq!(state_of(&w, "main"), "no-recipe");
    assert_eq!(count(&fx_file(&w, "docker"), "*-main"), 0);
}

#[test]
fn it_does_not_count_against_the_limit_of_previews() {
    let w = host("main-limit");
    with_main(&w);
    w.minimal(&[
        "TEHAS_PREVIEWS=1",
        "TEHAS_DOMAIN=example.org",
        "TEHAS_PREVIEW_MAX=1",
    ]);
    commit(&w, "main");
    let a = commit(&w, "one");
    pulls(&w, &[&pull(&w, 21, &a)]);
    preview(&w, "");
    assert_eq!(state_of(&w, "main"), "serving");
    assert_eq!(state_of(&w, "21"), "serving");
}

#[test]
fn a_full_disk_builds_nothing_for_it_as_for_the_others() {
    let w = host("main-disk");
    with_main(&w);
    commit(&w, "main");
    put(&w, "free", "1000\n");
    preview(&w, "");
    assert_eq!(state_of(&w, "main"), "no-space");
    assert_eq!(builds(&w), 0);
}

#[test]
fn no_notice_is_written_for_it() {
    let w = host("main-notice");
    with_main(&w);
    commit(&w, "main");
    preview(&w, "");
    assert_eq!(writes(&w), "");
}

#[test]
fn github_not_answering_leaves_a_running_main_preview_running() {
    let w = host("main-github");
    with_main(&w);
    let m = commit(&w, "main");
    preview(&w, "");
    rm_calls(&w, "docker");
    touch(&w, "branch-fails");
    commit(&w, "main");
    let r = preview(&w, "");
    rc(&r, 0);
    assert_eq!(container_of(&w, "main"), m);
    assert_eq!(state_of(&w, "main"), "serving");
    assert_eq!(builds(&w), 0);
    has(
        &r.out,
        "GitHub did not say what the default branch is at, left as it is",
    );
    unput(&w, "branch-fails");
    touch(&w, "repo-fails");
    let r = preview(&w, "");
    assert_eq!(container_of(&w, "main"), m);
    has(&r.out, "left as it is");
}

#[test]
fn a_default_branch_name_that_git_would_take_for_an_option_is_not_fetched() {
    let w = host("main-option");
    put(&w, "default-branch", "--upload-pack=x\n");
    let r = preview(&w, "");
    rc(&r, 0);
    has(&r.out, "left as it is");
    assert_eq!(builds(&w), 0);
}

#[test]
fn a_list_of_pull_requests_that_cannot_be_read_does_not_stop_the_main_preview() {
    let w = host("main-no-list");
    with_main(&w);
    commit(&w, "main");
    unput(&w, "pulls.json");
    preview(&w, "");
    assert_eq!(state_of(&w, "main"), "serving");
}

#[test]
fn a_repository_answer_without_a_default_branch_leaves_it_alone() {
    let w = host("main-none");
    let a = commit(&w, "one");
    pulls(&w, &[&pull(&w, 22, &a)]);
    preview(&w, "");
    assert_eq!(state_of(&w, "22"), "serving");
    assert_eq!(count(&pdir(&w), "main"), 0);
}

#[test]
fn sweep_the_container_image_ref_and_state_of_main_stay_and_it_is_not_counted() {
    let w = host("main-sweep");
    with_main(&w);
    let m = commit(&w, "main");
    let a = commit(&w, "one");
    pulls(&w, &[&pull(&w, 31, &a)]);
    preview(&w, "");
    close(&w, 31);
    let r = preview(&w, "--sweep");
    rc(&r, 0);
    assert_eq!(container_of(&w, "main"), m);
    assert_eq!(count(&fx_file(&w, "docker"), "image-main"), 1);
    assert_eq!(count(&pdir(&w), "main"), 1);
    assert_eq!(
        git_in(
            &pdir(&w).join("repo.git"),
            &["rev-parse", "refs/preview/main"]
        ),
        m
    );
    assert_eq!(count(&fx_file(&w, "docker"), "*-31"), 0);
    lacks(&r.out, "main: swept");
    lacks(&r.out, "container abmain");
    assert_eq!(jq_r(".removed", &pdir(&w).join("sweep.json")), "1");
}

#[test]
fn sweep_it_keeps_main_when_github_does_not_answer_at_all() {
    let w = host("main-sweep-down");
    with_main(&w);
    let m = commit(&w, "main");
    pulls(&w, &[]);
    preview(&w, "");
    touch(&w, "gh-down");
    touch(&w, "repo-fails");
    preview(&w, "--sweep");
    assert_eq!(container_of(&w, "main"), m);
    assert_eq!(count(&fx_file(&w, "docker"), "image-main"), 1);
}

#[test]
fn sweep_a_container_labelled_main_for_another_repository_is_still_swept() {
    let w = host("main-sweep-other");
    with_main(&w);
    commit(&w, "main");
    pulls(&w, &[]);
    preview(&w, "");
    put(&w, "label-repo", "other/place\n");
    preview(&w, "--sweep");
    assert_eq!(count(&fx_file(&w, "docker"), "container-main"), 0);
}
