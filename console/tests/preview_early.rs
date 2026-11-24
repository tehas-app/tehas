//! `tehasbot preview` where it reaches no GitHub: a run that stops at Docker
//! or at a network that is not safe still writes the dashboard's list, and a
//! host with no console beside the command builds nothing. Run as a host runs
//! it, on the docker and df stubs of console/tests/preview/; gh is not a stub,
//! so a call to it fails the case. Moved from test/preview_notice_errors_test.sh,
//! whose cases that need gh to answer stay there until the run reads GitHub
//! through the ports (T4.9).

mod common;
#[path = "preview/mod.rs"]
mod preview;

use common::*;
use preview::*;
use std::fs;

const SHA: &str = "0000000000000000000000000000000000000012";

/// A preview host whose gh is the deny list's.
fn no_github(name: &str) -> World {
    let w = host(name);
    fs::remove_file(w.t().join("bin/gh")).unwrap();
    w
}

/// left <number> <commit> of the shell: what a run that served it leaves, a
/// state, a container and an image, with no run that asks GitHub.
fn left(w: &World, n: &str, sha: &str) {
    let dir = pdir(w).join(n);
    fs::create_dir_all(&dir).unwrap();
    let state = format!(
        r#"{{"number": {n}, "sha": "{sha}", "state": "serving", "reason": "", "time": "x"}}"#
    );
    fs::write(dir.join("state.json"), state).unwrap();
    put(w, &format!("docker/container-{n}"), &format!("{sha}\n"));
    put(w, &format!("docker/image-{n}"), &format!("sha256:img{n}\n"));
}

/// A list written long ago, with a figure that is not today's.
fn stale_list(w: &World) {
    let list = previews_json(w);
    fs::create_dir_all(list.parent().unwrap()).unwrap();
    fs::write(
        &list,
        r#"{"generatedAt": "2026-01-01T00:00:00Z", "disk": {"freeBytes": 1}, "previews": []}"#,
    )
    .unwrap();
}

/// stopped <fixture> of the shell: a serving preview, a list written long
/// ago, and a run that stops at that fixture with 2 GB free. The list is
/// written again, and the preview is still in it and still there. The host
/// and what the run said.
fn stopped(name: &str, fixture: &str) -> (World, Run) {
    let w = no_github(name);
    left(&w, "12", SHA);
    stale_list(&w);
    touch(&w, fixture);
    put(&w, "free", "2097152\n");
    let r = preview(&w, "");
    rc(&r, 0);
    let list = previews_json(&w);
    assert_eq!(
        jq_r(r#".generatedAt | test("^2026-01-01") | not"#, &list),
        "true"
    );
    assert_eq!(
        jq_r("(now - (.generatedAt | fromdateiso8601)) < 120", &list),
        "true"
    );
    assert_eq!(
        jq_r(
            r#".previews[0] | "\(.number) \(.state) \(.address)""#,
            &list
        ),
        "12 serving https://uat-12.example.org/"
    );
    assert_eq!(container_of(&w, "12"), SHA);
    (w, r)
}

#[test]
fn a_run_that_stops_at_a_network_that_is_not_safe_still_writes_the_list_with_the_free_space() {
    let (w, _) = stopped("net-unsafe", "net-internal");
    put(&w, "net-internal", "false\n");
    let r = preview(&w, "");
    rc(&r, 0);
    has(&r.out, "(it is not internal); nothing was built");
    assert_eq!(
        jq_file(&["-cS"], ".disk", &previews_json(&w)),
        r#"{"freeBytes":2147483648,"low":true,"minFreeBytes":5368709120}"#
    );
    assert_eq!(builds(&w), 0);
}

#[test]
fn a_run_that_stops_because_docker_does_not_answer_still_writes_the_list_with_no_figure_and_not_as_healthy(
) {
    let (w, r) = stopped("docker-down", "docker-down");
    has(
        &r.out,
        "Docker does not answer; nothing was built or removed",
    );
    assert_eq!(
        jq_file(&["-cS"], ".disk", &previews_json(&w)),
        r#"{"freeBytes":null,"low":true,"minFreeBytes":5368709120}"#
    );
}

#[test]
fn one_empty_state_stops_no_list_the_run_that_stops_early_still_writes_it() {
    let w = no_github("empty-state");
    left(&w, "12", SHA);
    fs::create_dir_all(pdir(&w).join("13")).unwrap();
    fs::write(pdir(&w).join("13/state.json"), "").unwrap();
    let list = previews_json(&w);
    fs::create_dir_all(list.parent().unwrap()).unwrap();
    fs::write(&list, r#"{"generatedAt": "then", "previews": []}"#).unwrap();
    touch(&w, "docker-down");
    let r = preview(&w, "");
    rc(&r, 0);
    has(
        &r.out,
        "Docker does not answer; nothing was built or removed",
    );
    assert_ne!(jq_r(".generatedAt", &list), "then");
    assert_eq!(jq_file(&["-c"], "[.previews[].number]", &list), "[12]");
}

// The notice is written by the console beside the command. A host without one
// is a host install.sh did not finish: nothing is built that it could not tell.
#[test]
fn a_host_with_no_console_beside_the_command_builds_nothing_and_says_to_install() {
    let w = no_github("no-console");
    fs::remove_file(console(&w)).unwrap();
    let r = preview(&w, "");
    rc(&r, 78);
    has(
        &r.out,
        &format!(
            "tehasbot: no console at {}: run install.sh",
            console(&w).display()
        ),
    );
    assert_eq!(count(&pdir(&w), "*"), 0);
    assert_eq!(builds(&w), 0);
}
