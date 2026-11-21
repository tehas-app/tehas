//! The platform line of the factory's health: the checkout a host runs from,
//! and the release it installed. tehasbot status reads both and writes them
//! to the health file; tehas status prints the line.

mod common;

use common::*;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::Command;

/// git with a name and a branch of its own, quiet.
fn g(args: &[&str], date: Option<&str>) {
    let mut c = Command::new("git");
    c.args([
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@example.invalid",
        "-c",
        "init.defaultBranch=main",
    ])
    .args(args);
    if let Some(d) = date {
        c.env("GIT_COMMITTER_DATE", d).env("GIT_AUTHOR_DATE", d);
    }
    let out = c.output().unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn p(path: &Path) -> String {
    path.display().to_string()
}

/// The platform checkout: a clone of a local origin, which a case moves ahead.
fn platform_setup(w: &World) {
    let t = w.t();
    for d in ["origin", "origin.git"] {
        let _ = fs::remove_dir_all(t.join(d));
    }
    let _ = fs::remove_dir_all(w.home.join("platform"));
    let _ = fs::remove_file(w.home.join("state/platform-fetch"));
    let origin = p(&t.join("origin"));
    let bare = p(&t.join("origin.git"));
    g(&["init", "-q", &origin], None);
    g(
        &["-C", &origin, "commit", "-q", "--allow-empty", "-m", "one"],
        None,
    );
    g(&["clone", "-q", "--bare", &origin, &bare], None);
    g(&["clone", "-q", &bare, &p(&w.home.join("platform"))], None);
}

fn origin_ahead(w: &World, commits: usize, date: &str) {
    let origin = p(&w.t().join("origin"));
    for i in 0..commits {
        let m = format!("ahead {i}");
        g(
            &["-C", &origin, "commit", "-q", "--allow-empty", "-m", &m],
            Some(date),
        );
    }
    g(
        &[
            "-C",
            &origin,
            "push",
            "-q",
            &p(&w.t().join("origin.git")),
            "main",
        ],
        None,
    );
}

fn head(w: &World) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(w.home.join("platform"))
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// `date -u -d <when> +<fmt>`.
fn date(when: &str, fmt: &str) -> String {
    let out = Command::new("date")
        .args(["-u", "-d", when, &format!("+{fmt}")])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// `$status && $summary`, the two outputs as one.
fn status_then_summary(w: &World) -> Run {
    let s = w.status();
    if s.rc != 0 {
        return s;
    }
    let r = w.summary(&[]);
    let out = [s.out, r.out]
        .into_iter()
        .filter(|o| !o.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    Run { rc: r.rc, out }
}

fn health(w: &World) -> Value {
    w.live_json("health.json")
}

fn platform(w: &World) -> Value {
    w.live_json("platform.json")
}

fn platform_bare(w: &World) -> Value {
    let mut v = platform(w);
    let o = v.as_object_mut().unwrap();
    o.remove("generatedAt");
    o.remove("fetchedAt");
    v
}

/// The record of the last fetch is an hour and a second older.
fn fetch_an_hour_ago(w: &World) {
    let text = w.read_home("state/platform-fetch");
    let last: i64 = text.split_whitespace().next().unwrap().parse().unwrap();
    let then = last - 3601;
    w.home_file("state/platform-fetch", &format!("{then} {then}\n"));
}

fn release_calls(w: &World) -> usize {
    w.gh()
        .lines()
        .filter(|l| l.starts_with("api repos/") && l.contains("/releases"))
        .count()
}

#[test]
fn platform_an_up_to_date_checkout_is_said_and_passes() {
    let w = World::new("platform-up");
    platform_setup(&w);
    let r = status_then_summary(&w);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(health(&w)["status"], "pass");
    has(
        &r.out,
        &format!(
            "platform  {}, up to date with origin/main, fetched ",
            head(&w)
        ),
    );
    assert_eq!(health(&w)["checks"]["platform"]["behind"], 0);
}

#[test]
fn platform_the_first_poll_with_no_record_of_a_fetch_prints_nothing() {
    let w = World::new("platform-first");
    platform_setup(&w);
    let r = w.status();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(r.out, "");
}

#[test]
fn platform_behind_for_more_than_a_day_is_a_warning_that_says_how_far() {
    let w = World::new("platform-behind");
    platform_setup(&w);
    origin_ahead(&w, 2, &date("-2 days", "%FT%TZ"));
    let r = status_then_summary(&w);
    assert_eq!(r.rc, 0, "{}", r.out);
    let day = date("-2 days", "%F");
    assert_eq!(health(&w)["status"], "warn");
    assert_eq!(
        health(&w)["output"],
        format!("The platform is 2 commits behind origin/main since {day}: run tehas update.")
    );
    has(
        &r.out,
        &format!(
            "platform  {}, 2 commits behind origin/main since {day}",
            head(&w)
        ),
    );
}

#[test]
fn platform_behind_for_less_than_a_day_is_said_and_passes() {
    let w = World::new("platform-hours");
    platform_setup(&w);
    origin_ahead(&w, 1, &date("-2 hours", "%FT%TZ"));
    let r = status_then_summary(&w);
    assert_eq!(health(&w)["status"], "pass");
    has(&r.out, "1 commits behind origin/main");
}

#[test]
fn platform_the_checkout_is_fetched_at_most_once_an_hour() {
    let w = World::new("platform-hourly");
    platform_setup(&w);
    w.status();
    origin_ahead(&w, 1, &date("-2 days", "%FT%TZ"));
    w.status();
    assert_eq!(platform(&w)["behind"], 0);
    fetch_an_hour_ago(&w);
    w.status();
    assert_eq!(platform(&w)["behind"], 1);
}

#[test]
fn platform_a_fetch_that_fails_is_no_error_and_is_not_tried_again_for_an_hour() {
    let w = World::new("platform-fetch-fails");
    platform_setup(&w);
    fs::rename(w.t().join("origin.git"), w.t().join("origin.gone")).unwrap();
    w.home_file("state/platform-fetch", "1 1\n");
    let r = w.status();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(r.out, "");
    assert_eq!(platform(&w)["fetchedAt"], "1970-01-01T00:00:01Z");
    let text = w.read_home("state/platform-fetch");
    let last: i64 = text.split_whitespace().next().unwrap().parse().unwrap();
    assert!(epoch("now") - last < 60, "{text}");
}

#[test]
fn platform_with_github_down_the_checkout_is_still_read() {
    let w = World::new("platform-gh-down");
    platform_setup(&w);
    w.touch("gh-down");
    let r = w.status();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(platform(&w)["behind"], 0);
}

#[test]
fn platform_no_checkout_no_file_and_no_line() {
    let w = World::new("platform-none");
    let _ = fs::remove_dir_all(w.home.join("platform"));
    fs::create_dir_all(&w.live).unwrap();
    fs::write(w.live_file("platform.json"), "{}\n").unwrap();
    let r = status_then_summary(&w);
    assert!(!w.live_file("platform.json").exists());
    lacks(&r.out, "platform");
    assert_eq!(health(&w)["checks"]["platform"], Value::Null);
}

/// A host that runs a release: ~/tehas/current is a link to a version with a
/// VERSION, and the releases are what the gh stub answers from rel.json.
fn release_setup(w: &World, installed: &[&str]) {
    for d in ["current", "versions.env"] {
        let _ = fs::remove_file(w.home.join(d));
    }
    for d in ["versions", "platform", "state"] {
        let _ = fs::remove_dir_all(w.home.join(d));
    }
    w.rm_fx("rel.json");
    fs::create_dir_all(w.home.join("versions")).unwrap();
    fs::create_dir_all(w.home.join("state")).unwrap();
    // The release repository, as the host's versions.env gives it.
    fs::copy(
        root().join("host/versions.env"),
        w.home.join("versions.env"),
    )
    .unwrap();
    for v in installed {
        w.home_file(&format!("versions/{v}/VERSION"), &format!("{v}\n"));
    }
    std::os::unix::fs::symlink(format!("versions/{}", installed[0]), w.home.join("current"))
        .unwrap();
    w.store_db();
    w.store_ok(&["backup"]);
}

/// The published releases, newest first, as GitHub answers the list.
fn releases(w: &World, tags: &[&str]) {
    let list: Vec<Value> = tags
        .iter()
        .map(|t| json!({"tag_name": t, "draft": false, "prerelease": false}))
        .collect();
    w.fx("rel.json", &format!("{}\n", Value::from(list)));
}

/// Two hours ago, the host read `newest`.
fn read_two_hours_ago(w: &World, newest: &str) {
    let then = epoch("now") - 7200;
    w.home_file("state/platform-fetch", &format!("{then} {then}\n"));
    w.home_file("state/platform-newest", &format!("{newest}\n"));
}

#[test]
fn platform_a_release_host_names_the_release_and_the_newest_and_passes() {
    let w = World::new("release-names");
    release_setup(&w, &["v0.9.0"]);
    releases(&w, &["v0.9.0"]);
    let r = status_then_summary(&w);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(health(&w)["status"], "pass");
    assert_eq!(
        platform_bare(&w),
        json!({"release":"v0.9.0","newest":"v0.9.0","behind":0,"upstream":"releases"})
    );
    has(&r.out, "platform  v0.9.0, newest v0.9.0, fetched ");
    assert_eq!(health(&w)["checks"]["platform"]["release"], "v0.9.0");
    assert_eq!(health(&w)["checks"]["platform"]["commit"], Value::Null);
}

#[test]
fn platform_a_newer_release_is_said_as_one_that_is_out() {
    let w = World::new("release-newer");
    release_setup(&w, &["v0.9.0"]);
    releases(&w, &["v0.9.1", "v0.9.0"]);
    let r = status_then_summary(&w);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(health(&w)["status"], "pass");
    has(&r.out, "platform  v0.9.0, v0.9.1 is out, fetched ");
    assert_eq!(health(&w)["checks"]["platform"]["newest"], "v0.9.1");
}

#[test]
fn platform_releases_that_cannot_be_read_leave_the_newest_unknown_and_nothing_else_changes() {
    let w = World::new("release-unread");
    release_setup(&w, &["v0.9.0"]);
    // Neither gh nor curl answers: the list has no newest.
    w.set_env(&["GH_AUTH=1", "CURL_RC=22"]);
    let r = status_then_summary(&w);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(health(&w)["status"], "pass");
    assert_eq!(
        platform_bare(&w),
        json!({"release":"v0.9.0","newest":null,"behind":0,"upstream":"releases"})
    );
    has(&r.out, "platform  v0.9.0, newest unknown, never fetched");
}

#[test]
fn platform_the_releases_are_read_at_most_once_an_hour() {
    let w = World::new("release-hourly");
    release_setup(&w, &["v0.9.0"]);
    releases(&w, &["v0.9.0"]);
    w.status();
    assert_eq!(release_calls(&w), 1);
    releases(&w, &["v0.9.1", "v0.9.0"]);
    w.status();
    assert_eq!(release_calls(&w), 1);
    assert_eq!(platform(&w)["newest"], "v0.9.0");
    assert_eq!(w.read_home("state/platform-newest").trim_end(), "v0.9.0");
    fetch_an_hour_ago(&w);
    let r = status_then_summary(&w);
    assert_eq!(release_calls(&w), 2);
    assert_eq!(platform(&w)["newest"], "v0.9.1");
    has(&r.out, "platform  v0.9.0, v0.9.1 is out, fetched ");
}

#[test]
fn platform_a_release_host_with_no_record_of_a_read_says_it_was_never_fetched() {
    let w = World::new("release-never");
    release_setup(&w, &["v0.9.0"]);
    let r = status_then_summary(&w);
    has(&r.out, "platform  v0.9.0, newest unknown, never fetched");
    assert_eq!(platform(&w)["fetchedAt"], Value::Null);
}

#[test]
fn platform_a_release_host_keeps_the_newest_of_the_last_read_when_github_goes_down() {
    let w = World::new("release-down");
    release_setup(&w, &["v0.9.0", "v0.9.1"]);
    releases(&w, &["v0.9.1", "v0.9.0"]);
    w.status();
    assert_eq!(platform(&w)["newest"], "v0.9.1");
    fetch_an_hour_ago(&w);
    w.set_env(&["GH_AUTH=1"]);
    w.rm_fx("rel.json");
    let r = status_then_summary(&w);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(platform(&w)["newest"], "v0.9.1");
    has(&r.out, "platform  v0.9.0, v0.9.1 is out, fetched ");
}

// A cached newest is an answer from an earlier read, so it is only believed
// while it is not behind what the host runs.

#[test]
fn platform_a_cached_newest_older_than_the_installed_release_is_not_said() {
    let w = World::new("release-cached-old");
    release_setup(&w, &["v0.10.0", "v0.9.0"]);
    releases(&w, &["v0.9.0"]);
    // The read of an hour ago, before the host moved to v0.10.0.
    read_two_hours_ago(&w, "v0.9.0");
    w.set_env(&["GH_AUTH=1", "CURL_RC=22"]);
    let r = status_then_summary(&w);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(platform(&w)["newest"], Value::Null);
    lacks(&r.out, "is out");
    has(&r.out, "platform  v0.10.0, newest unknown, fetched ");
}

#[test]
fn platform_a_cached_newest_older_than_the_installed_release_is_read_again() {
    let w = World::new("release-cached-again");
    release_setup(&w, &["v0.10.0", "v0.9.0"]);
    releases(&w, &["v0.10.0"]);
    read_two_hours_ago(&w, "v0.9.0");
    let r = status_then_summary(&w);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(platform(&w)["newest"], "v0.10.0");
    assert_eq!(w.read_home("state/platform-newest").trim_end(), "v0.10.0");
    assert_eq!(release_calls(&w), 1);
    lacks(&r.out, "is out");
    has(&r.out, "platform  v0.10.0, newest v0.10.0, fetched ");
}

// The words of the line, whatever the two versions are: only a newest greater
// than the installed release is out.

#[test]
fn platform_the_same_release_installed_and_newest_says_no_is_out() {
    let w = World::new("release-same");
    release_setup(&w, &["v0.10.0"]);
    releases(&w, &["v0.10.0"]);
    let r = status_then_summary(&w);
    assert_eq!(r.rc, 0, "{}", r.out);
    lacks(&r.out, "is out");
    has(&r.out, "platform  v0.10.0, newest v0.10.0, fetched ");
}

#[test]
fn platform_a_newest_published_behind_the_installed_release_says_no_is_out() {
    let w = World::new("release-behind");
    release_setup(&w, &["v0.10.0"]);
    releases(&w, &["v0.9.0"]);
    let r = status_then_summary(&w);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(platform(&w)["newest"], "v0.9.0");
    lacks(&r.out, "is out");
    has(&r.out, "platform  v0.10.0, newest v0.9.0, fetched ");
}

/// A VERSION the host wrote without the v is shown as it is: it is no triple
/// beside v0.10.0, so it is never said to be out.
#[test]
fn platform_an_installed_version_with_no_v_is_shown_as_it_is_never_as_one_that_is_out() {
    let w = World::new("release-no-v");
    release_setup(&w, &["0.10.0"]);
    releases(&w, &["v0.10.0"]);
    let r = status_then_summary(&w);
    assert_eq!(r.rc, 0, "{}", r.out);
    lacks(&r.out, "is out");
    has(&r.out, "platform  0.10.0, newest v0.10.0, fetched ");
}

#[test]
fn platform_a_checkout_host_with_a_current_that_is_not_a_release_stays_on_the_checkout() {
    let w = World::new("platform-not-release");
    platform_setup(&w);
    // The installed version is read from a link with a VERSION and nothing
    // else, which keeps a host whose current is not such a link on its
    // checkout.
    w.home_file("versions/v0.9.0/VERSION", "v0.9.0\n");
    w.home_file("current", "");
    let r = status_then_summary(&w);
    has(
        &r.out,
        &format!(
            "platform  {}, up to date with origin/main, fetched ",
            head(&w)
        ),
    );
    assert_eq!(health(&w)["checks"]["platform"]["release"], Value::Null);
}
