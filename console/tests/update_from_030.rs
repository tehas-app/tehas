//! The update of a host on 0.30.x to this release. The 0.30.x updater lists
//! the releases of the repository its versions.env names, checks the download
//! with the key in ~/tehas/release/minisign.pub, asks this release's
//! lib/config.sh whether the instance holds, and switches current. From then
//! on this release's own update runs on the home 0.30.x left, and it reads
//! neither of those two files. test/fixtures/install-0.30 is what a v0.30.6
//! install leaves in the home and settings an instance of 0.30.x may set that
//! this release renamed or removed.

#[path = "update/mod.rs"]
mod common;
mod releases;

use common::{has, lacks, root, Host, MINIMAL};
use releases::*;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

fn old() -> PathBuf {
    root().join("test/fixtures/install-0.30")
}

fn make_release(h: &Host, tag: &str, pre: bool) {
    let body = format!("### Changes\n- {tag}");
    pack(
        h,
        &Rel {
            tag,
            body: Some(&body),
            pre,
            arch: arch_of(&machine()).unwrap_or("x86_64"),
            key: false,
            install: format!("printf \"install {tag}\\n\" >>\"$T/calls/install\"\n"),
        },
    );
}

fn home_030(h: &Host) {
    old_home(h, &old(), "v0.30.6");
}

fn stubs(h: &Host) {
    h.gh(GH);
    h.stub("minisign", MINISIGN);
    h.stub(
        "docker",
        r#"case "$1 $2" in "image inspect") exit 1 ;; esac"#,
    );
    h.stub("systemctl", "");
}

/// The 0.30.x instance with each setting row of api/renames-1.0.tsv applied:
/// a renamed setting under its new name, a removed one gone.
fn renamed_030() -> Vec<String> {
    let tsv = fs::read_to_string(root().join("api/renames-1.0.tsv")).unwrap();
    let new: HashMap<&str, &str> = tsv
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            (f[0] == "setting").then(|| (f[1], f.get(2).copied().unwrap_or("")))
        })
        .collect();
    lines(&old().join("instance.env"))
        .into_iter()
        .filter_map(|l| {
            let (key, rest) = l.split_once('=').unwrap_or((&l, ""));
            match new.get(key) {
                Some(&"") => None,
                Some(n) => Some(format!("{n}={rest}")),
                None => Some(l.clone()),
            }
        })
        .collect()
}

/// MINIMAL, TEHAS_UPDATE_FROM=release, then these lines.
fn instance_of(h: &mut Host, extra: &[String]) {
    let mut all: Vec<&str> = MINIMAL.to_vec();
    all.push("TEHAS_UPDATE_FROM=release");
    all.extend(extra.iter().map(String::as_str));
    instance(h, &all);
}

#[test]
fn a_0_30_x_host_lists_releases_where_this_release_is_published_and_checks_them_with_its_key() {
    assert_eq!(
        setting(&old().join("versions.env"), "TEHAS_RELEASE_REPO"),
        pinned()
    );
    assert_eq!(
        fs::read_to_string(old().join("release/minisign.pub")).unwrap(),
        fs::read_to_string(root().join("host/release/minisign.pub")).unwrap()
    );
}

#[test]
fn this_releases_settings_check_refuses_a_0_30_x_instance_that_sets_a_renamed_setting() {
    let mut h = Host::new("from030");
    home_030(&h);
    instance_of(&mut h, &lines(&old().join("instance.env")));
    let r = settings_hold(&h, &root());
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "TEHAS_EASY_MERGE is no longer read");
    has(
        &r.out,
        &h.instance_dir().join("instance.env").display().to_string(),
    );
}

#[test]
fn this_releases_settings_check_lists_every_renamed_removed_or_building_line_of_a_0_30_x_host_at_once(
) {
    let mut h = Host::new("from030");
    home_030(&h);
    let mut extra = lines(&old().join("instance.env"));
    extra.push("TEHAS_OVERLAP_HOLD=1".into());
    extra.push("TEHAS_REVIEW_REPO=acme/backlog".into());
    instance_of(&mut h, &extra);
    let env = h.join("home/.config/tehas/env");
    fs::create_dir_all(env.parent().unwrap()).unwrap();
    fs::write(&env, "TEHAS_BOT_LOGIN=acme[bot]\n").unwrap();
    let r = settings_hold(&h, &root());
    assert_eq!(r.rc, 78, "{}", r.out);
    for name in [
        "TEHAS_TICKET_CONCURRENCY",
        "TEHAS_CONCURRENCY",
        "TEHAS_REFRESH_MAX",
        "TEHAS_OWNER",
        "TEHAS_INDEXING",
        "TEHAS_EASY_MERGE",
        "TEHAS_CYCLE_REPORT",
        "TEHAS_OVERLAP_HOLD",
    ] {
        has(&r.out, &format!("tehas: {name} is no longer read: "));
    }
    let dir = h.instance_dir().display().to_string();
    has(&r.out, &format!("tehas: TEHAS_REVIEW_REPO is set and review is still being built in this release: take the line out (see {dir}/instance.env)"));
    has(&r.out, &format!("tehas: TEHAS_BOT_LOGIN is no longer read: the bot is the GitHub App's login, kept in state/github/login, so take the line out (see {})", env.display()));
    assert_eq!(
        r.out.lines().filter(|l| l.starts_with("tehas: ")).count(),
        10,
        "{}",
        r.out
    );
}

#[test]
fn with_the_renames_applied_this_releases_settings_check_holds() {
    let mut h = Host::new("from030");
    home_030(&h);
    instance_of(&mut h, &renamed_030());
    let r = settings_hold(&h, &root());
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &fs::read_to_string(h.instance_dir().join("instance.env")).unwrap(),
        "TEHAS_MAX_CYCLES=3",
    );
}

#[test]
fn this_release_updates_the_home_0_30_x_left_from_the_built_in_repository_and_key() {
    let mut h = Host::new("from030");
    make_release(&h, "v0.30.6", false);
    make_release(&h, "v1.0.0", false);
    stubs(&h);
    home_030(&h);
    instance_of(&mut h, &renamed_030());
    // What 0.30.x left in the home names another repository and another key.
    let venv = h.home.join("versions.env");
    let text: String = fs::read_to_string(&venv)
        .unwrap()
        .lines()
        .map(|l| {
            if l.starts_with("TEHAS_RELEASE_REPO=") {
                "TEHAS_RELEASE_REPO=someone/else\n".to_string()
            } else {
                format!("{l}\n")
            }
        })
        .collect();
    fs::write(&venv, text).unwrap();
    fs::write(
        h.home.join("release/minisign.pub"),
        "untrusted comment: a key a release left\n",
    )
    .unwrap();
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "release: current is v1.0.0, was v0.30.6");
    assert_eq!(current(&h), "versions/v1.0.0");
    let pinned = pinned();
    has(&h.calls("gh"), &format!("api repos/{pinned}/releases"));
    has(
        &h.calls("gh"),
        &format!("release download v1.0.0 --repo {pinned}"),
    );
    lacks(&h.calls("gh"), "someone/else");
    assert_eq!(
        h.calls("minisign-key"),
        fs::read_to_string(root().join("host/release/minisign.pub"))
            .unwrap()
            .trim_end_matches('\n')
    );
    assert_eq!(h.calls("install"), "install v1.0.0");
}

#[test]
fn from_the_0_30_x_layout_a_prerelease_is_installed_only_when_to_names_it() {
    let mut h = Host::new("from030");
    make_release(&h, "v0.30.6", false);
    make_release(&h, "v1.0.0", false);
    make_release(&h, "v1.0.1-rc.1", true);
    stubs(&h);
    // The instance of the case before, whose app world home_030 takes away.
    instance_of(&mut h, &renamed_030());
    home_030(&h);
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(current(&h), "versions/v1.0.0");
    home_030(&h);
    let r = h.update(&["--to", "v1.0.1-rc.1"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(current(&h), "versions/v1.0.1-rc.1");
}
