//! The update of a host on 0.32.x to this release. 0.31.x and 0.32.x refuse
//! every setting 1.0 renamed or removed, so an instance that loads there holds
//! here, and they take an alpha or a beta when --to names it.
//! test/fixtures/install-0.32 is what a v0.32.0 install leaves in the home
//! and an instance that v0.32.0 loads.

mod common;
mod releases;

use common::{has, lacks, root, Host, MINIMAL};
use releases::*;
use std::fs;
use std::path::PathBuf;

fn old() -> PathBuf {
    root().join("test/fixtures/install-0.32")
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

fn home_032(h: &Host) {
    old_home(h, &old(), "v0.32.0");
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

/// MINIMAL, TEHAS_UPDATE_FROM=release, the 0.32.x instance, then `extra`.
fn instance_032(h: &mut Host, extra: &[&str]) {
    let old_lines = lines(&old().join("instance.env"));
    let mut all: Vec<&str> = MINIMAL.to_vec();
    all.push("TEHAS_UPDATE_FROM=release");
    all.extend(old_lines.iter().map(String::as_str));
    all.extend(extra);
    instance(h, &all);
}

/// The releases of the update cases: v0.32.0 and v1.0.0.
fn releases(h: &Host) {
    make_release(h, "v0.32.0", false);
    make_release(h, "v1.0.0", false);
}

#[test]
fn a_0_32_x_host_lists_releases_where_this_release_is_published_and_checks_them_with_its_key() {
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
fn an_instance_that_0_32_x_loads_holds_on_this_releases_settings_check_unchanged() {
    let mut h = Host::new("from032");
    home_032(&h);
    instance_032(&mut h, &[]);
    let r = settings_hold(&h, &root());
    assert_eq!(r.rc, 0, "{}", r.out);
}

#[test]
fn this_release_updates_the_home_0_32_x_left_from_the_built_in_repository_and_key() {
    let mut h = Host::new("from032");
    releases(&h);
    stubs(&h);
    home_032(&h);
    instance_032(&mut h, &[]);
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "release: current is v1.0.0, was v0.32.0");
    assert_eq!(current(&h), "versions/v1.0.0");
    let pinned = pinned();
    has(&h.calls("gh"), &format!("api repos/{pinned}/releases"));
    has(
        &h.calls("gh"),
        &format!("release download v1.0.0 --repo {pinned}"),
    );
    assert_eq!(
        h.calls("minisign-key"),
        fs::read_to_string(root().join("host/release/minisign.pub"))
            .unwrap()
            .trim_end_matches('\n')
    );
    assert_eq!(h.calls("install"), "install v1.0.0");
}

#[test]
fn from_the_0_32_x_layout_a_beta_is_installed_only_when_to_names_it() {
    let mut h = Host::new("from032");
    releases(&h);
    make_release(&h, "v1.0.1-beta.1", true);
    stubs(&h);
    // The instance of the case before, whose app world home_032 takes away.
    instance_032(&mut h, &[]);
    home_032(&h);
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(current(&h), "versions/v1.0.0");
    home_032(&h);
    let r = h.update(&["--to", "v1.0.1-beta.1"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(current(&h), "versions/v1.0.1-beta.1");
}

// The known limit of the update docs: a manager that writes a 0.30 name into
// instance.env stops the update. Its version under the 0.30 name is ignored.
#[test]
fn a_0_30_name_a_manager_writes_into_instance_env_stops_the_update_with_78_and_switches_nothing() {
    let mut h = Host::new("from032");
    releases(&h);
    make_release(&h, "v1.0.1-beta.1", true);
    stubs(&h);
    home_032(&h);
    let env = h.join("home/.config/tehas/env");
    fs::create_dir_all(env.parent().unwrap()).unwrap();
    fs::write(&env, "TEHAS_INDUSTRY_VERSION=v0.36.0\n").unwrap();
    instance_032(&mut h, &["TEHAS_TICKET_CONCURRENCY=3"]);
    let r = h.update(&[]);
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "TEHAS_TICKET_CONCURRENCY is no longer read: tehas 1.0 renamed it TEHAS_MAX_CYCLES",
    );
    lacks(&r.out, "TEHAS_INDUSTRY_VERSION");
    assert_eq!(current(&h), "versions/v0.32.0");
    assert_eq!(h.calls("install"), "");
}
