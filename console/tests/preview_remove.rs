//! `tehasbot preview`: the limit on previews, removal, and what a run does
//! when GitHub, Docker or a fetch fails. Run as a host runs it, on the stubs
//! of console/tests/preview/. Moved from test/preview_remove_test.sh.

mod common;
#[path = "preview/mod.rs"]
mod preview;

use common::*;
use preview::*;
use std::fs;

/// The commits of the limit cases, as the shell's variables name them.
struct Limit {
    b: String,
    c: String,
    a4: String,
}

/// The first case: two previews under a limit of two, then a third pull
/// request that pushes the oldest over it.
fn newest(w: &World) -> (String, String, String) {
    w.minimal(&[
        "TEHAS_PREVIEWS=1",
        "TEHAS_DOMAIN=example.org",
        "TEHAS_PREVIEW_MAX=2",
    ]);
    let a = commit(w, "one");
    let b = commit(w, "two");
    let c = commit(w, "three");
    pulls(w, &[&pull(w, 22, &b), &pull(w, 21, &a)]);
    preview(w, "");
    assert_eq!(
        format!("{} {}", state_of(w, "22"), state_of(w, "21")),
        "serving serving"
    );
    assert_eq!(container_of(w, "21"), a);
    // A newer one is opened: the oldest falls over the limit, and what ran of it stops.
    pulls(w, &[&pull(w, 23, &c), &pull(w, 22, &b), &pull(w, 21, &a)]);
    preview(w, "");
    assert_eq!(
        format!(
            "{} {} {}",
            state_of(w, "23"),
            state_of(w, "22"),
            state_of(w, "21")
        ),
        "serving serving over-limit"
    );
    assert_eq!(count(&fx_file(w, "docker"), "container-21"), 0);
    assert_eq!(count(&fx_file(w, "docker"), "image-21"), 0);
    has(&docker(w), "rm -f -v tehas-preview-21");
    assert_eq!(
        jq_r(
            ".previews[] | select(.number == 21) | .address",
            &previews_json(w)
        ),
        "null"
    );
    (a, b, c)
}

/// The second case, after the first: over the limit is stopped once.
fn stopped_once(w: &World, b: String, c: String) -> Limit {
    let began = jq_r(".time", &pdir(w).join("21/state.json"));
    rm_calls(w, "docker");
    preview(w, "");
    let r = preview(w, "");
    assert_eq!(state_of(w, "21"), "over-limit");
    assert_eq!(
        grep_c(
            &docker(w),
            "rm -f -v tehas-preview-21|image rm -f tehas-preview:21"
        ),
        0
    );
    assert_eq!(jq_r(".time", &pdir(w).join("21/state.json")), began);
    lacks(&r.out, "#21");
    // A new commit of it is a new state, and what is there of the number goes again.
    let a4 = commit(w, "one");
    pulls(w, &[&pull(w, 23, &c), &pull(w, 22, &b), &pull(w, 21, &a4)]);
    preview(w, "");
    assert_eq!(
        jq_r(r#""\(.state) \(.sha)""#, &pdir(w).join("21/state.json")),
        format!("over-limit {a4}")
    );
    assert_eq!(grep_c(&docker(w), "rm -f -v tehas-preview-21"), 1);
    Limit { b, c, a4 }
}

/// The third case, after the second: a container that did not go.
fn stopped_again(w: &World, l: &Limit) {
    put(w, "rm-fails", "tehas-preview-22\n");
    let d = commit(w, "four");
    pulls(
        w,
        &[
            &pull(w, 24, &d),
            &pull(w, 23, &l.c),
            &pull(w, 22, &l.b),
            &pull(w, 21, &l.a4),
        ],
    );
    preview(w, "");
    assert_eq!(state_of(w, "22"), "over-limit");
    assert_eq!(container_of(w, "22"), l.b);
    unput(w, "rm-fails");
    rm_calls(w, "docker");
    preview(w, "");
    assert_eq!(state_of(w, "22"), "over-limit");
    assert_eq!(
        docker(w)
            .lines()
            .filter(|x| *x == "rm -f -v tehas-preview-22")
            .count(),
        1
    );
    assert_eq!(count(&fx_file(w, "docker"), "container-22"), 0);
    rm_calls(w, "docker");
    preview(w, "");
    assert_eq!(grep_c(&docker(w), "rm -f -v tehas-preview-22"), 0);
    pulls(
        w,
        &[&pull(w, 23, &l.c), &pull(w, 22, &l.b), &pull(w, 21, &l.a4)],
    );
    preview(w, "");
    assert_eq!(
        format!(
            "{} {} {}",
            state_of(w, "23"),
            state_of(w, "22"),
            state_of(w, "21")
        ),
        "serving serving over-limit"
    );
}

/// The fourth case, after the third: the merged pull requests go.
fn merged(w: &World, l: &Limit) {
    pulls(w, &[&pull(w, 23, &l.c)]);
    close(w, 21);
    close(w, 22);
    close(w, 24);
    let r = preview(w, "");
    assert_eq!(count(&pdir(w), "21") + count(&pdir(w), "22"), 0);
    assert_eq!(count(&fx_file(w, "docker"), "*-22"), 0);
    assert_eq!(
        git_in(&pdir(w).join("repo.git"), &["for-each-ref", "refs/preview"])
            .lines()
            .count(),
        1
    );
    has(&r.out, "#22: pull request no longer open, removed");
    assert_eq!(
        jq_r(r#"[.previews[].number] | join(" ")"#, &previews_json(w)),
        "23"
    );
}

/// The fifth case, after the fourth: containers of other numbers and
/// repositories.
fn by_numbers(w: &World) {
    put(
        w,
        "extra-containers",
        "c0de0c 77 other/thing foreign\nc0de0d 78 ACME/Widget ours\nc0de0e 1234567890 acme/widget big\nc0de0f 079 acme/widget zero\n",
    );
    rm_calls(w, "docker");
    let r = preview(w, "");
    rc(&r, 0);
    lacks(&r.out, "#77");
    lacks(&docker(w), "tehas-preview-77");
    lacks(&docker(w), "tehas-preview:77");
    has(&r.out, "#78: pull request no longer open, removed");
    has(&docker(w), "rm -f -v tehas-preview-78");
    // A number of ten digits, or with a zero in front, is not a pull request's.
    lacks(&r.out, "#1234567890");
    lacks(&r.out, "#079");
    lacks(&r.out, "#79");
    assert_eq!(state_of(w, "23"), "serving");
    unput(w, "extra-containers");
}

/// The sixth case, after the fifth: GitHub down.
fn github_down(w: &World, l: &Limit) {
    touch(w, "gh-down");
    let r = preview(w, "");
    rc(&r, 0);
    assert_eq!(state_of(w, "23"), "serving");
    assert_eq!(container_of(w, "23"), l.c);
    unput(w, "gh-down");
}

/// The cases of the limit from the first to `last`, in order, as the shell
/// file runs them: each starts from what the one before left.
fn limit_cases(w: &World, last: u32) {
    let (_, b, c) = newest(w);
    if last < 2 {
        return;
    }
    let l = stopped_once(w, b, c);
    if last >= 3 {
        stopped_again(w, &l);
    }
    if last >= 4 {
        merged(w, &l);
    }
    if last >= 5 {
        by_numbers(w);
    }
    if last >= 6 {
        github_down(w, &l);
    }
}

#[test]
fn only_the_newest_pull_requests_get_a_preview() {
    let w = host("limit");
    limit_cases(&w, 1);
}

#[test]
fn a_pull_request_that_is_over_the_limit_is_stopped_once_not_at_every_run() {
    let w = host("limit-once");
    limit_cases(&w, 2);
}

#[test]
fn a_pull_request_over_the_limit_whose_container_did_not_go_is_stopped_again() {
    let w = host("limit-again");
    limit_cases(&w, 3);
}

#[test]
fn a_merged_pull_request_is_removed_container_image_ref_and_directory() {
    let w = host("merged");
    limit_cases(&w, 4);
}

#[test]
fn the_run_removes_by_the_numbers_of_this_repository_s_containers_only() {
    let w = host("numbers");
    limit_cases(&w, 5);
}

#[test]
fn github_down_removes_nothing() {
    let w = host("github-down");
    limit_cases(&w, 6);
}

#[test]
fn an_answer_that_is_not_a_list_removes_nothing() {
    let w = host("not-a-list");
    limit_cases(&w, 6);
    put(&w, "pulls.json", "{\"message\":\"Bad credentials\"}\n");
    let r = preview(&w, "");
    rc(&r, 0);
    assert_eq!(state_of(&w, "23"), "serving");
}

#[test]
fn docker_down_builds_and_removes_nothing_and_says_so() {
    let w = host("docker-down");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    preview(&w, "");
    assert_eq!(state_of(&w, "12"), "serving");
    // The pull request is closed and another opened while Docker is away.
    let b = commit(&w, "other");
    pulls(&w, &[&pull(&w, 13, &b)]);
    touch(&w, "docker-down");
    clear_calls(&w);
    let r = preview(&w, "");
    rc(&r, 0);
    has(
        &r.out,
        "Docker does not answer; nothing was built or removed",
    );
    assert_eq!(state_of(&w, "12"), "serving");
    assert_eq!(container_of(&w, "12"), a);
    assert_eq!(count(&fx_file(&w, "docker"), "image-12"), 1);
    assert_eq!(count(&pdir(&w), "13"), 0);
    assert_eq!(
        docker(&w)
            .lines()
            .filter(|l| !l.starts_with("info "))
            .count(),
        0
    );
    assert_eq!(gh_calls(&w), "");
    assert_eq!(
        jq_r(
            r#".previews[0] | "\(.number) \(.state)""#,
            &previews_json(&w)
        ),
        "12 serving"
    );
}

#[test]
fn an_empty_answer_from_github_removes_nothing() {
    let w = host("empty-answer");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    preview(&w, "");
    put(&w, "pulls.json", "");
    let r = preview(&w, "");
    rc(&r, 0);
    assert_eq!(state_of(&w, "12"), "serving");
    assert_eq!(container_of(&w, "12"), a);
    lacks(&r.out, "removed");
}

#[test]
fn a_fetch_that_fails_writes_no_state_and_is_tried_again() {
    let w = host("fetch-fails");
    let a = commit(&w, "feature");
    pulls(&w, &[&pull(&w, 12, &a)]);
    let up = upstream(&w);
    let away = w.t().join("upstream.away");
    fs::rename(&up, &away).unwrap();
    let r = preview(&w, "");
    has(
        &r.out,
        "#12: the commit could not be fetched, tried again at the next run",
    );
    assert_eq!(state_of(&w, "12"), "");
    fs::rename(&away, &up).unwrap();
    preview(&w, "");
    assert_eq!(state_of(&w, "12"), "serving");
}
