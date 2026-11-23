//! `tehasbot preview`: a container that does not start or stops, and the
//! moment a run waits before it asks. Run as a host runs it, on the stubs of
//! console/tests/preview/. Moved from test/preview_settle_test.sh.

mod common;
#[macro_use]
#[path = "preview/mod.rs"]
mod preview;

use common::*;
use preview::*;

#[test]
fn a_container_that_did_not_start_is_marked_and_tried_again_at_the_next_run() {
    let w = host("run-fails");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    touch(&w, "run-fails");
    preview(&w, "");
    assert_eq!(state_of(&w, "12"), "build-failed");
    has(&reason_of(&w, "12"), "container did not start");
    unput(&w, "run-fails");
    preview(&w, "");
    assert_eq!(state_of(&w, "12"), "serving");
}

#[test]
fn a_recipe_less_commit_after_a_failed_start_leaves_nothing_to_retry() {
    let w = host("recipe-less");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    touch(&w, "run-fails");
    preview(&w, "");
    unput(&w, "run-fails");
    let a2 = commit_as(&w, "feature", "none");
    pulls(&w, &[&pull(&w, 12, &a2)]);
    preview(&w, "");
    assert_eq!(state_of(&w, "12"), "no-recipe");
    assert_eq!(count(&pdir(&w).join("12"), ".retry"), 0);
    rm_calls(&w, "docker");
    preview(&w, "");
    assert_eq!(format!("{} {}", builds(&w), runs(&w)), "0 0");
    lacks(&docker(&w), "rm -f -v tehas-preview-12");
}

#[test]
fn a_container_that_never_starts_is_tried_three_more_times_then_left_until_a_new_commit() {
    let w = host("never-starts");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    touch(&w, "run-fails");
    for _ in 1..=5 {
        preview(&w, "");
    }
    assert_eq!(grep_c(&docker(&w), "^build "), 4);
    assert_eq!(state_of(&w, "12"), "build-failed");
    unput(&w, "run-fails");
    let a2 = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a2)]);
    preview(&w, "");
    assert_eq!(state_of(&w, "12"), "serving");
}

#[test]
fn a_container_is_asked_whether_it_still_runs_a_moment_after_its_start() {
    let w = host("asked");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    preview(&w, "");
    assert_eq!(state_of(&w, "12"), "serving");
    assert_eq!(calls(&w, "sleep"), "0");
    assert_eq!(
        grep_c(
            &docker(&w),
            r"^inspect --format \{\{.State.Running\}\} \{\{.RestartCount\}\} tehas-preview-12$"
        ),
        1
    );
    // grep -n '^run \|^inspect ' | cut -d' ' -f1 | cut -d: -f2 | tr '\n' ' '
    let got: String = docker(&w)
        .lines()
        .enumerate()
        .filter(|(_, l)| l.starts_with("run ") || l.starts_with("inspect "))
        .map(|(i, l)| {
            let numbered = format!("{}:{l}", i + 1);
            let field = numbered.split(' ').next().unwrap().to_string();
            format!("{} ", field.split(':').nth(1).unwrap_or(""))
        })
        .collect();
    assert_eq!(got, "run inspect ");
}

#[test]
fn the_moment_is_five_seconds_where_nothing_says_otherwise() {
    let w = host("five");
    w.set_env(&["GH_TOKEN=sekret-token"]);
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    preview(&w, "");
    assert_eq!(calls(&w, "sleep"), "5");
    assert_eq!(state_of(&w, "12"), "serving");
}

/// The variable can shorten the wait and nothing else.
fn wait_of(asked: &str, waited: &str) {
    let w = host("wait");
    w.set_env(&[
        "GH_TOKEN=sekret-token",
        &format!("TEHAS_PREVIEW_SETTLE={asked}"),
    ]);
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    preview(&w, "");
    assert_eq!(calls(&w, "sleep"), waited);
    assert_eq!(state_of(&w, "12"), "serving");
}

cases!(wait_of {
    a_wait_of_60_asked_for_is_a_wait_of_5: "60", "5";
    a_wait_of_6_asked_for_is_a_wait_of_5: "6", "5";
    a_wait_of_5_asked_for_is_a_wait_of_5: "5", "5";
    a_wait_of_4_asked_for_is_a_wait_of_4: "4", "4";
    a_wait_of_0_asked_for_is_a_wait_of_0: "0", "0";
    a_wait_of_00_asked_for_is_a_wait_of_5: "00", "5";
    a_wait_of_1_asked_for_is_a_wait_of_5: "-1", "5";
    a_wait_of_0_5_asked_for_is_a_wait_of_5: "0.5", "5";
    a_wait_of_soon_asked_for_is_a_wait_of_5: "soon", "5";
    a_wait_of_asked_for_is_a_wait_of_5: "", "5";
});

/// A container that says `answer` after its start.
fn stopped_after_start(answer: &str) {
    let w = host("settle");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    // The id this build answers with: the preview had no image before this run,
    // so the file names the image of the build and nothing that was there before.
    let served = image_of(&w, &a);
    put(&w, "settle", &format!("{answer}\n"));
    let r = preview(&w, "");
    rc(&r, 0);
    assert_eq!(state_of(&w, "12"), "build-failed");
    assert_eq!(
        reason_of(&w, "12"),
        r#"The container started and stopped. See "docker logs" in logs/preview-12.log on the host."#
    );
    assert_eq!(count(&fx_file(&w, "docker"), "container-12"), 0);
    // The image it built goes with it, asked for by the id the build answered
    // with and not by the tag a stopped container still holds, and nothing is
    // built for that commit again.
    assert_eq!(count(&fx_file(&w, "docker"), "image-12"), 0);
    has(&docker(&w), &format!("image rm {served}"));
    lacks(&docker(&w), "image rm tehas-preview:12");
    // Its last lines are kept before it goes.
    let calls = docker(&w);
    let kept: Vec<&str> = calls
        .lines()
        .filter(|l| *l == "logs --tail 40 tehas-preview-12" || *l == "rm -f -v tehas-preview-12")
        .collect();
    let last: String = kept[kept.len().saturating_sub(2)..]
        .iter()
        .map(|l| format!("{l} "))
        .collect();
    assert_eq!(
        last,
        "logs --tail 40 tehas-preview-12 rm -f -v tehas-preview-12 "
    );
    has(&cat(&log_of(&w, "12")), "Read-only file system");
    assert_eq!(
        jq_r(
            r#".previews[0] | "\(.state) \(.address)""#,
            &previews_json(&w)
        ),
        "build-failed null"
    );
    assert_eq!(writes_lines(&w), "1");
    has(&writes(&w), "Unable to create a preview");
    has(&writes(&w), "The container started and stopped.");
    lacks(&writes(&w), "https://uat-12.example.org/");
}

cases!(stopped_after_start {
    a_container_that_says_false_0_after_its_start_is_not_called_serving: "false 0";
    a_container_that_says_true_2_after_its_start_is_not_called_serving: "true 2";
    a_container_that_says_after_its_start_is_not_called_serving: "";
});

#[test]
fn a_container_that_stopped_at_once_is_not_started_again_nor_built_again() {
    let w = host("stopped-once");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    put(&w, "settle", "false 0\n");
    preview(&w, "");
    unput(&w, "settle");
    assert_eq!(count(&pdir(&w).join("12"), ".retry"), 0);
    rm_calls(&w, "docker");
    preview(&w, "");
    assert_eq!(format!("{} {}", builds(&w), runs(&w)), "0 0");
    assert_eq!(state_of(&w, "12"), "build-failed");
    assert_eq!(image_id(&w, "12"), "");
    // A new commit, which is a different commit, is built and serves: the stopped
    // one holds nothing back for it.
    let a2 = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a2)]);
    preview(&w, "");
    assert_eq!(state_of(&w, "12"), "serving");
    assert_eq!(container_of(&w, "12"), a2);
    assert_eq!(builds(&w), 1);
}

#[test]
fn a_container_that_stopped_at_once_leaves_no_image_is_not_started_again_and_is_built_once() {
    let w = host("built-once");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    put(&w, "settle", "false 0\n");
    for _ in 1..=5 {
        preview(&w, "");
    }
    assert_eq!(
        format!("{} {} {}", builds(&w), runs(&w), image_rm_calls(&w)),
        "1 1 1"
    );
    assert_eq!(state_of(&w, "12"), "build-failed");
    assert_eq!(count(&pdir(&w).join("12"), ".retry"), 0);
    assert_eq!(image_id(&w, "12"), "");
    // The one image it was told to remove is the image of that build, named by its
    // id: nothing else on the host is touched, and the docker stub removed it.
    let built = image_of(&w, &a);
    assert_eq!(grep(&docker(&w), "^image rm"), format!("image rm {built}"));
    assert_eq!(image_rm_unexpected(&w), "");
    assert_eq!(writes_lines(&w), "1");
}

#[test]
fn a_stopped_commit_takes_its_own_image_and_the_image_of_the_commit_before_it_by_their_ids() {
    let w = host("two-images");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    preview(&w, "");
    let served = image_id(&w, "12");
    let first = image_of(&w, &a);
    assert_eq!(served, first, "the first build made another image");
    let a2 = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a2)]);
    let stopped = image_of(&w, &a2);
    put(&w, "settle", "false 0\n");
    rm_calls(&w, "docker");
    preview(&w, ""); // its build made a new image and its container stopped
                     // The build moved the tag off the image of the commit that served, and that
                     // commit's container went before the new one was started: nothing runs it and
                     // nothing names it, so it goes with the image of the stopped commit. Each is
                     // named by its id once, and never by the tag.
    assert_eq!(
        format!(
            "{} {} {}",
            image_rm_calls_of(&w, &stopped),
            image_rm_calls_of(&w, &served),
            image_rm_calls(&w)
        ),
        "1 1 2"
    );
    lacks(&docker(&w), "image rm tehas-preview:12");
    assert_eq!(image_rm_unexpected(&w), "");
}

#[test]
fn a_stopped_commit_leaves_no_image_and_nothing_serving_until_a_new_commit_serves() {
    let w = host("nothing-serving");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    preview(&w, "");
    let a2 = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a2)]);
    put(&w, "settle", "false 0\n");
    preview(&w, ""); // its build made a new image and its container stopped
    assert_eq!(state_of(&w, "12"), "build-failed");
    assert_eq!(image_id(&w, "12"), "");
    assert_eq!(untagged(&w, "12"), "");
    // The container of the earlier commit went before this one was started.
    assert_eq!(count(&fx_file(&w, "docker"), "container-12"), 0);
    assert_eq!(
        jq_r(
            r#".previews[0] | "\(.state) \(.address)""#,
            &previews_json(&w)
        ),
        "build-failed null"
    );
    // And it stays so at the next run: the same commit is not built again.
    rm_calls(&w, "docker");
    preview(&w, "");
    assert_eq!(format!("{} {}", builds(&w), runs(&w)), "0 0");
    assert_eq!(count(&fx_file(&w, "docker"), "container-12"), 0);
    let a3 = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a3)]);
    let next = image_of(&w, &a3);
    unput(&w, "settle");
    preview(&w, "");
    assert_eq!(state_of(&w, "12"), "serving");
    assert_eq!(container_of(&w, "12"), a3);
    assert_eq!(image_id(&w, "12"), next);
    assert_eq!(untagged(&w, "12"), "");
}

#[test]
fn a_container_that_was_not_created_keeps_its_own_reason_and_is_not_asked_or_logged() {
    let w = host("not-created");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    touch(&w, "run-fails");
    preview(&w, "");
    assert_eq!(
        reason_of(&w, "12"),
        "The container did not start. See logs/preview-12.log on the host."
    );
    assert_eq!(grep_c(&docker(&w), "^inspect |^logs "), 0);
}
