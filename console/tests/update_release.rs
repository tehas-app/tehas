//! tehas update from releases (ops/src/update/release.rs): which version is
//! the target, the notes between, the two checks of a download, the instance
//! checked against the target's own rules, the switch of current, the image a
//! version ran with, and the last three versions kept. A local release
//! directory stands in for GitHub: gh and curl answer from it.

#[path = "update/mod.rs"]
mod common;
mod releases;

use common::{git, has, lacks, root, s, Host, Run, MINIMAL};
use releases::*;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::Command;

/// install.sh of a release: it records its tag and arguments; INSTALL_RC
/// fails it after the switch, INSTALL_SAYS is what it says then.
fn install_of(tag: &str) -> String {
    format!(
        "printf \"install {tag} %s\\n\" \"${{*:-}}\" >>\"$T/calls/install\"\n[ -z \"${{INSTALL_SAYS:-}}\" ] || echo \"$INSTALL_SAYS\" >&2\n[ -z \"${{INSTALL_RC:-}}\" ] || exit \"$INSTALL_RC\"\n"
    )
}

/// A release of `tag` for `machine`, with notes of its own or the default.
fn make_release_for(h: &Host, tag: &str, body: Option<&str>, machine: &str) {
    pack(
        h,
        &Rel {
            tag,
            body,
            pre: false,
            arch: arch_of(machine).expect("no release is packed for this machine"),
            key: true,
            install: install_of(tag),
        },
    );
}

fn make_release(h: &Host, tag: &str) {
    make_release_for(h, tag, None, &machine());
}

/// The releases every case starts from.
fn releases_at_home(h: &Host) {
    make_release(h, "v1.0.0");
    make_release_for(
        h,
        "v1.1.0",
        Some("### Changes\n- the dispatcher names its reason\n### Adopter action\nSet TEHAS_EXAMPLE_KNOB in instance.env."),
        &machine(),
    );
    make_release(h, "v1.2.0");
}

fn sh_ok(dir: &Path, cmd: &str) {
    let out = Command::new("bash")
        .args(["-c", cmd])
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "{cmd}: {out:?}");
}

/// A home installed from the release `tag` of `machine`, with the installed
/// key and versions.env.
fn host_at_for(h: &Host, tag: &str, machine: &str) {
    let th = &h.home;
    let name = format!("tehas-{tag}-linux-{}.tar.gz", arch_of(machine).unwrap());
    let _ = fs::remove_dir_all(th);
    let _ = fs::remove_dir_all(h.join("src"));
    h.clear_calls();
    for d in ["versions", "release", "state"] {
        fs::create_dir_all(th.join(d)).unwrap();
    }
    fs::write(th.join(".tehas-home"), "").unwrap();
    fs::copy(
        root().join("host/release/minisign.pub"),
        th.join("release/minisign.pub"),
    )
    .unwrap();
    fs::copy(root().join("host/versions.env"), th.join("versions.env")).unwrap();
    sh_ok(
        &th.join("versions"),
        &format!(
            "tar -xzf '{}' && mv 'tehas-{tag}' '{tag}'",
            s(&rel_dir(h).join(tag).join(name))
        ),
    );
    symlink(format!("versions/{tag}"), th.join("current")).unwrap();
    fs::write(th.join("state/release-history"), format!("{tag}\n")).unwrap();
}

fn host_at(h: &Host, tag: &str) {
    host_at_for(h, tag, &machine());
}

/// No current, so the host runs from its platform checkout; with a tag, the
/// checkout is a repository whose HEAD carries it.
fn checkout_host(h: &Host, tag: Option<&str>) {
    host_at(h, "v1.0.0");
    fs::remove_file(h.home.join("current")).unwrap();
    if let Some(tag) = tag {
        let p = h.home.join("platform");
        git(None, &["init", "-q", "--initial-branch=main", &s(&p)]);
        let d = Some(p.as_path());
        git(d, &["config", "user.name", "test"]);
        git(d, &["config", "user.email", "test@example.com"]);
        fs::write(p.join("install.sh"), "").unwrap();
        git(d, &["add", "-A"]);
        git(d, &["commit", "-qm", "first"]);
        git(d, &["tag", tag]);
    }
}

/// Releases and a host at v1.0.0 for a machine whose uname -m answers
/// `machine`; a machine no release exists for gets the x86_64 ones.
fn host_at_arch(h: &Host, machine: &str) {
    let asset = arch_of(machine).unwrap_or("x86_64");
    make_release_for(h, "v1.0.0", None, asset);
    make_release_for(h, "v1.2.0", None, asset);
    host_at_for(h, "v1.0.0", asset);
}

/// MINIMAL, TEHAS_UPDATE_FROM=release, then `extra`.
fn instance_rel(h: &mut Host, extra: &[&str]) {
    let mut all: Vec<&str> = MINIMAL.to_vec();
    all.push("TEHAS_UPDATE_FROM=release");
    all.extend(extra);
    instance(h, &all);
}

/// A host with the releases at home and its stubs.
fn host() -> Host {
    let h = Host::new("release");
    releases_at_home(&h);
    stubs(&h);
    h
}

fn stubs(h: &Host) {
    h.gh(GH);
    h.stub(
        "curl",
        r#"out="" url=""; prev=""
for a in "$@"; do [ "$prev" != -o ] || out=$a; prev=$a; url=$a; done
case "$url" in
  https://api.github.com/*) cat "$T/releases/list.json" ;;
  */releases/download/*) tag=${url%/*}; tag=${tag##*/}; cp "$T/releases/$tag/${url##*/}" "$out" ;;
  *) exit 22 ;;
esac"#,
    );
    h.stub("minisign", MINISIGN);
    // IMAGE_LAYER is the tehas.layer label of an image that is there; the uid
    // tag names sha256:before.
    h.stub(
        "docker",
        r#"case "$1 $2" in "image inspect")
  [ "${IMAGE_RC:-1}" != 0 ] || case "$*" in
    *tehas.layer*) printf "%s\n" "${IMAGE_LAYER:-}" ;;
    *.Id*) echo sha256:before ;;
  esac
  exit "${IMAGE_RC:-1}" ;; esac"#,
    );
    h.stub("systemctl", "");
}

fn upd(h: &Host, args: &str) -> Run {
    update_with(h, "", args)
}

fn uid() -> String {
    let out = Command::new("id").arg("-u").output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn versions(h: &Host) -> String {
    let mut v: Vec<String> = fs::read_dir(h.home.join("versions"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v.join("\n")
}

fn key() -> String {
    fs::read_to_string(root().join("host/release/minisign.pub"))
        .unwrap()
        .trim_end_matches('\n')
        .to_string()
}

fn is_link(p: &Path) -> bool {
    p.symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

#[test]
fn check_names_the_installed_and_the_newest_version_and_the_notes_between_and_changes_nothing() {
    let mut h = host();
    instance_rel(&mut h, &[]);
    host_at(&h, "v1.0.0");
    let r = h.update(&["--check"]);
    assert_eq!(r.rc, 3, "{}", r.out);
    has(&r.out, "release: installed v1.0.0, newest v1.2.0");
    has(&r.out, "== v1.1.0");
    has(&r.out, "Set TEHAS_EXAMPLE_KNOB in instance.env.");
    has(&r.out, "== v1.2.0");
    lacks(&r.out, "== v1.0.0");
    has(&r.out, "release: would switch to v1.2.0");
    assert_eq!(current(&h), "versions/v1.0.0");
    lacks(&h.calls("gh"), "release download");
    assert_eq!(h.calls("install"), "install v1.0.0 --check");
    assert_eq!(versions(&h), "v1.0.0");
}

/// The host of the download cases: at v1.0.0, then updated to v1.2.0.
fn updated() -> (Host, Run) {
    let mut h = host();
    instance_rel(&mut h, &[]);
    host_at(&h, "v1.0.0");
    let r = h.update(&[]);
    (h, r)
}

#[test]
fn an_update_downloads_the_newest_checks_it_switches_current_and_installs_from_it() {
    let (h, r) = updated();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "release: v1.2.0 downloaded, checked against its .sha256 and its signature",
    );
    has(&r.out, "release: current is v1.2.0, was v1.0.0");
    assert_eq!(current(&h), "versions/v1.2.0");
    assert_eq!(
        fs::read_to_string(h.home.join("versions/v1.2.0/VERSION")).unwrap(),
        "v1.2.0\n"
    );
    assert_eq!(h.calls("install"), "install v1.2.0 ");
    has(
        &h.calls("gh"),
        "release download v1.2.0 --repo tehas-app/tehas",
    );
    has(
        &h.calls("docker"),
        &format!(
            "tag tehas/pi-rukas:uid{} tehas/pi-rukas:release-v1.2.0",
            uid()
        ),
    );
    let tmp: Vec<_> = fs::read_dir(h.home.join("tmp"))
        .map(|d| d.collect())
        .unwrap_or_default();
    assert!(tmp.is_empty(), "{tmp:?}");
}

#[test]
fn the_same_version_again_downloads_nothing_and_installs_from_current() {
    let (h, r) = updated();
    assert_eq!(r.rc, 0, "{}", r.out);
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "release: v1.2.0 is the version installed");
    assert_eq!(
        h.calls("gh")
            .lines()
            .filter(|l| l.contains("release download"))
            .count(),
        1
    );
}

#[test]
fn a_tarball_that_does_not_match_its_sha256_is_not_unpacked_and_nothing_is_switched() {
    let mut h = host();
    instance_rel(&mut h, &[]);
    host_at(&h, "v1.0.0");
    common::append(
        &rel_dir(&h).join("v1.2.0").join(asset_of("v1.2.0")),
        "tampered\n",
    );
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "does not match its .sha256");
    has(
        &r.out,
        "release: v1.2.0 was not installed; nothing was changed",
    );
    assert_eq!(current(&h), "versions/v1.0.0");
    assert_eq!(versions(&h), "v1.0.0");
    assert_eq!(h.calls("install"), "");
}

#[test]
fn a_tarball_the_release_key_did_not_sign_is_not_unpacked() {
    let mut h = host();
    instance_rel(&mut h, &[]);
    host_at(&h, "v1.0.0");
    fs::write(
        rel_dir(&h)
            .join("v1.2.0")
            .join(format!("{}.minisig", asset_of("v1.2.0"))),
        "forged\n",
    )
    .unwrap();
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "is not signed by the release key built into tehas (host/release/minisign.pub)",
    );
    assert_eq!(current(&h), "versions/v1.0.0");
    assert_eq!(versions(&h), "v1.0.0");
}

#[test]
fn the_key_checked_with_is_the_one_built_into_tehas_not_the_hosts_or_the_downloads() {
    let mut h = host();
    instance_rel(&mut h, &[]);
    host_at(&h, "v1.0.0");
    fs::write(
        h.home.join("release/minisign.pub"),
        "untrusted comment: a key a release left\n",
    )
    .unwrap();
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(h.calls("minisign-key"), key());
    lacks(
        &h.calls("minisign"),
        &s(&h.home.join("release/minisign.pub")),
    );
    assert_eq!(current(&h), "versions/v1.2.0");
    host_at(&h, "v1.0.0");
    fs::remove_file(h.home.join("release/minisign.pub")).unwrap();
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(current(&h), "versions/v1.2.0");
}

#[test]
fn with_no_minisign_the_download_is_refused_not_taken_on_its_sha256_alone() {
    let mut h = host();
    fs::remove_file(h.join("bin/minisign")).unwrap();
    instance_rel(&mut h, &[]);
    host_at(&h, "v1.0.0");
    let r = h.sh("PATH=$T/bin:/usr/bin:/bin; \"$TEHAS_CONSOLE\" update run \"$TEHAS_CONSOLE\"");
    let own = std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .any(|d| Path::new(d).join("minisign").is_file());
    if own {
        eprintln!("SKIP: this host has a minisign of its own");
        return;
    }
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "minisign is not installed");
    assert_eq!(current(&h), "versions/v1.0.0");
}

#[test]
fn a_release_that_asks_for_a_setting_the_instance_lacks_is_not_switched_to_and_its_example_value_is_named(
) {
    let mut h = host();
    make_release(&h, "v1.3.0");
    // The target's own lib/config.sh asks for a name no instance has.
    let src = h.join("src/tehas-v1.3.0");
    common::append(
        &src.join("lib/config.sh"),
        r#"tehas_require_instance_settings() {
  [ -n "${TEHAS_NEW_KNOB:-}" ] || { tehas_err "TEHAS_NEW_KNOB is not set (see $TEHAS_INSTANCE_DIR/instance.env)"; exit 78; }
}
"#,
    );
    fs::create_dir_all(src.join("instances/example")).unwrap();
    common::append(
        &src.join("instances/example/instance.env"),
        "TEHAS_NEW_KNOB=42\n",
    );
    repack(&h, "v1.3.0", &asset_of("v1.3.0"));
    instance_rel(&mut h, &[]);
    host_at(&h, "v1.0.0");
    let r = h.update(&[]);
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "TEHAS_NEW_KNOB is not set");
    has(&r.out, "example: TEHAS_NEW_KNOB=42");
    has(
        &r.out,
        "settings: v1.3.0 asks for settings this instance does not have; nothing was switched",
    );
    assert_eq!(current(&h), "versions/v1.0.0");
    assert_eq!(h.calls("install"), "");
}

#[test]
fn a_tehas_key_the_target_release_never_names_is_warned_about() {
    let mut h = host();
    instance_rel(&mut h, &["TEHAS_RETIRED_KNOB=1"]);
    host_at(&h, "v1.0.0");
    // The current version knows it, so loading the settings is not refused.
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        &format!(
            "WARN: {}/instance/instance.env sets TEHAS_RETIRED_KNOB, which v1.2.0 does not know",
            s(h.t())
        ),
    );
    lacks(&r.out, "sets TEHAS_UPDATE_FROM");
}

#[test]
fn to_goes_back_to_a_version_on_disk_without_a_download_and_reuses_its_image() {
    let (h, r) = updated();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(current(&h), "versions/v1.2.0");
    fs::write(h.join("calls/gh"), "").unwrap();
    let r = update_with(&h, "IMAGE_RC=0", "--to v1.0.0");
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "release: going back to v1.0.0 undoes these releases:",
    );
    has(
        &r.out,
        "release: v1.0.0 is on this host from before; it is not downloaded again",
    );
    has(&r.out, "sandbox: the image v1.0.0 ran with is in use again");
    assert_eq!(current(&h), "versions/v1.0.0");
    lacks(&h.calls("gh"), "release download");
    has(
        &h.calls("docker"),
        &format!(
            "tag tehas/pi-rukas:release-v1.0.0 tehas/pi-rukas:uid{}",
            uid()
        ),
    );
}

#[test]
fn an_install_sh_that_fails_after_the_switch_moves_current_and_the_image_tag_back() {
    let (h, r) = updated();
    assert_eq!(r.rc, 0, "{}", r.out);
    let refusal = format!(
        "tehas: this release cannot migrate {}/state/tehas.db; nothing was installed",
        s(&h.home)
    );
    let r = update_with(
        &h,
        &format!("IMAGE_RC=0 INSTALL_RC=78 INSTALL_SAYS='{refusal}'"),
        "--to v1.0.0",
    );
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "sandbox: the image v1.0.0 ran with is in use again");
    has(
        &r.out,
        "install: install.sh exited 78; no follow-up was run",
    );
    has(
        &r.out,
        "release: install.sh did not finish v1.0.0, so current is v1.2.0 again",
    );
    assert_eq!(current(&h), "versions/v1.2.0");
    assert_eq!(
        fs::read_to_string(h.home.join("state/release-history"))
            .unwrap()
            .lines()
            .last(),
        Some("v1.2.0")
    );
    has(
        &h.calls("docker"),
        &format!("tag sha256:before tehas/pi-rukas:uid{}", uid()),
    );
    // The version current names again is the one a plain update installs.
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "release: v1.2.0 is the version installed");
}

#[test]
fn a_checkout_host_whose_first_install_from_a_release_fails_runs_from_its_checkout_again() {
    let mut h = host();
    instance_rel(&mut h, &[]);
    checkout_host(&h, Some("v1.2.0"));
    let r = update_with(
        &h,
        "INSTALL_RC=1 INSTALL_SAYS='tehas: a poll is running; nothing was installed'",
        "",
    );
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "release: install.sh did not finish v1.2.0, so current is the platform checkout again",
    );
    assert!(!is_link(&h.home.join("current")));
}

#[test]
fn an_install_sh_that_fails_after_it_began_to_copy_leaves_current_on_the_new_version() {
    let (h, r) = updated();
    assert_eq!(r.rc, 0, "{}", r.out);
    let r = update_with(
        &h,
        "IMAGE_RC=0 INSTALL_RC=1 INSTALL_SAYS='tehas: the store could not migrate db; the files are installed, the database was left as it was'",
        "--to v1.0.0",
    );
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "release: install.sh stopped after it began to install v1.0.0, so current stays on it",
    );
    lacks(&r.out, "did not finish");
    assert_eq!(current(&h), "versions/v1.0.0");
    lacks(&h.calls("docker"), "tag sha256:before");
}

#[test]
fn to_does_not_reuse_an_image_built_from_another_sandbox_dockerfile() {
    let (h, r) = updated();
    assert_eq!(r.rc, 0, "{}", r.out);
    let file = h.join("instance/sandbox.Dockerfile");
    fs::write(&file, "ARG TEHAS_BASE\nFROM $TEHAS_BASE\n").unwrap();
    let r = update_with(&h, "IMAGE_RC=0 IMAGE_LAYER=sha256:old", "--to v1.0.0");
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "sandbox: the image v1.0.0 ran with was built from another sandbox.Dockerfile, so it is not put back");
    lacks(&r.out, "is in use again");
    lacks(
        &h.calls("docker"),
        "tag tehas/pi-rukas:release-v1.0.0 tehas/pi-rukas:uid",
    );
    // Built from the file as it is, the image is put back.
    let out = Command::new(h.console())
        .args(["sandbox", "layer"])
        .arg(&file)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let sum = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let r = upd(&h, "--to v1.2.0");
    assert_eq!(r.rc, 0, "{}", r.out);
    let r = update_with(&h, &format!("IMAGE_RC=0 IMAGE_LAYER={sum}"), "--to v1.0.0");
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "sandbox: the image v1.0.0 ran with is in use again");
}

/// A host at v1.0.0 with the release instance.
fn at_v1() -> Host {
    let mut h = host();
    instance_rel(&mut h, &[]);
    host_at(&h, "v1.0.0");
    h
}

#[test]
fn to_a_version_that_is_not_published_changes_nothing() {
    let h = at_v1();
    let r = h.update(&["--to", "v9.9.9"]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "there is no published release v9.9.9");
    assert_eq!(current(&h), "versions/v1.0.0");
}

#[test]
fn to_needs_a_version_shaped_vx_y_z() {
    let h = at_v1();
    let r = h.update(&["--to", "latest"]);
    assert_eq!(r.rc, 64, "{}", r.out);
    has(&r.out, "--to takes a version vX.Y.Z");
    let r = h.update(&["--to"]);
    assert_eq!(r.rc, 64, "{}", r.out);
}

#[test]
fn to_on_a_host_that_updates_from_its_checkout_is_refused() {
    let mut h = host();
    let mut lines: Vec<&str> = MINIMAL.to_vec();
    lines.push("TEHAS_UPDATE_FROM=git");
    instance(&mut h, &lines);
    host_at(&h, "v1.0.0");
    fs::create_dir_all(h.home.join("platform/.git")).unwrap();
    fs::write(h.home.join("platform/install.sh"), "").unwrap();
    let r = h.update(&["--to", "v1.0.0"]);
    assert_eq!(r.rc, 64, "{}", r.out);
    has(&r.out, "TEHAS_UPDATE_FROM=git");
}

#[test]
fn a_hold_stops_a_plain_update_but_not_to() {
    let h = at_v1();
    fs::write(h.home.join("state/update-hold"), "").unwrap();
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "update-hold exists");
    let r = h.update(&["--to", "v1.1.0"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(current(&h), "versions/v1.1.0");
}

/// A host at v1.0.0 whose instance has no TEHAS_UPDATE_FROM.
fn at_v1_plain() -> Host {
    let mut h = host();
    instance(&mut h, MINIMAL);
    host_at(&h, "v1.0.0");
    h
}

#[test]
fn without_the_setting_current_means_releases_neither_current_nor_a_checkout_stops_with_78() {
    let h = at_v1_plain();
    let r = h.update(&["--check"]);
    has(&r.out, "release: installed v1.0.0, newest v1.2.0");
    fs::remove_file(h.home.join("current")).unwrap();
    let r = h.update(&["--check"]);
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "set TEHAS_UPDATE_FROM=release");
}

#[test]
fn tehas_update_from_takes_git_or_release_only() {
    let mut h = at_v1_plain();
    fs::remove_file(h.home.join("current")).unwrap();
    let mut lines: Vec<&str> = MINIMAL.to_vec();
    lines.push("TEHAS_UPDATE_FROM=tarball");
    instance(&mut h, &lines);
    let r = h.update(&["--check"]);
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "TEHAS_UPDATE_FROM is not valid: expected git or release",
    );
    lines.pop();
    lines.push("TEHAS_RELEASE_REPO=no-slash");
    instance(&mut h, &lines);
    let r = h.update(&["--check"]);
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "TEHAS_RELEASE_REPO is not valid: expected owner/name",
    );
}

#[test]
fn gh_not_signed_in_the_list_and_the_download_come_from_the_public_address() {
    let h = at_v1();
    let r = update_with(&h, "GH_AUTH=1", "");
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &h.calls("curl"),
        "https://api.github.com/repos/tehas-app/tehas/releases?per_page=100",
    );
    has(
        &h.calls("curl"),
        &format!(
            "https://github.com/tehas-app/tehas/releases/download/v1.2.0/{}.minisig",
            asset_of("v1.2.0")
        ),
    );
    assert_eq!(current(&h), "versions/v1.2.0");
}

#[test]
fn tehas_release_repo_in_the_instance_names_where_releases_come_from() {
    let mut h = host();
    instance_rel(&mut h, &["TEHAS_RELEASE_REPO=someone/fork"]);
    host_at(&h, "v1.0.0");
    h.update(&["--check"]);
    has(&h.calls("gh"), "api repos/someone/fork/releases");
}

#[test]
fn a_versions_env_on_the_host_does_not_name_where_releases_come_from() {
    let h = at_v1();
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
    h.update(&["--check"]);
    lacks(&h.calls("gh"), "someone/else");
    let named: Vec<String> = fs::read_to_string(root().join("host/versions.env"))
        .unwrap()
        .lines()
        .filter_map(|l| l.strip_prefix("TEHAS_RELEASE_REPO=").map(str::to_string))
        .collect();
    has(
        &h.calls("gh"),
        &format!("api repos/{}/releases", named.join("\n")),
    );
    fs::remove_file(&venv).unwrap();
    let r = h.update(&["--check"]);
    assert_eq!(r.rc, 3, "{}", r.out);
    has(&r.out, "release: would switch to v1.2.0");
}

#[test]
fn a_checkout_tagged_v1_0_0_is_shown_only_the_notes_after_its_own_tag_and_still_would_switch() {
    let mut h = host();
    instance_rel(&mut h, &[]);
    checkout_host(&h, Some("v1.0.0"));
    let r = h.update(&["--check"]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "release: installed none, this host ran from its platform checkout at v1.0.0, newest v1.2.0",
    );
    has(&r.out, "== v1.1.0");
    has(&r.out, "Set TEHAS_EXAMPLE_KNOB in instance.env.");
    has(&r.out, "== v1.2.0");
    lacks(&r.out, "== v1.0.0");
    has(&r.out, "release: would switch to v1.2.0");
}

#[test]
fn a_checkout_at_the_newest_tag_is_shown_no_notes_and_still_would_switch() {
    let mut h = host();
    make_release(&h, "v1.3.0");
    instance_rel(&mut h, &[]);
    checkout_host(&h, Some("v1.3.0"));
    let r = h.update(&["--check"]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "release: installed none, this host ran from its platform checkout at v1.3.0, newest v1.3.0",
    );
    lacks(&r.out, "== v1");
    has(&r.out, "release: would switch to v1.3.0");
}

#[test]
fn a_checkout_tagged_a_prerelease_newer_than_the_newest_release_moves_to_that_prerelease_not_back()
{
    let mut h = host();
    make_release(&h, "v1.3.0-beta.1");
    instance_rel(&mut h, &[]);
    checkout_host(&h, Some("v1.3.0-beta.1"));
    let r = h.update(&["--check"]);
    has(
        &r.out,
        "release: the checkout's v1.3.0-beta.1 is newer than the newest release",
    );
    has(&r.out, "release: would switch to v1.3.0-beta.1");
    lacks(&r.out, "going back");
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(current(&h), "versions/v1.3.0-beta.1");
    assert_eq!(h.calls("install"), "install v1.3.0-beta.1 ");
}

#[test]
fn a_host_on_a_prerelease_ahead_of_the_newest_release_is_not_offered_the_older_release() {
    let mut h = host();
    make_release(&h, "v1.3.0-beta.1");
    instance_rel(&mut h, &[]);
    host_at(&h, "v1.3.0-beta.1");
    let r = h.update(&["--check"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "release: installed v1.3.0-beta.1, a prerelease ahead of the newest release v1.2.0",
    );
    has(
        &r.out,
        "release: v1.3.0-beta.1 is the newest prerelease of v1.3.0; a plain tehas update keeps it",
    );
    lacks(&r.out, "newest v1.2.0");
    lacks(&r.out, "moves it back");
    lacks(&r.out, "would switch");
    make_release(&h, "v1.3.0-beta.2");
    let r = h.update(&["--check"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "release: the newest prerelease of v1.3.0 is v1.3.0-beta.2; tehas update --to v1.3.0-beta.2 moves to it");
    lacks(&r.out, "would switch");
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(current(&h), "versions/v1.3.0-beta.1");
    lacks(&h.calls("gh"), "release download");
}

#[test]
fn a_checkout_tagged_newer_than_any_release_and_not_published_changes_nothing() {
    let mut h = host();
    instance_rel(&mut h, &[]);
    checkout_host(&h, Some("v1.5.0"));
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "release: the checkout's v1.5.0 is newer than the newest release v1.2.0 and is not published");
    lacks(&h.calls("gh"), "release download");
    assert!(!is_link(&h.home.join("current")));
}

#[test]
fn a_checkout_with_no_tag_is_shown_every_release_as_before() {
    let mut h = host();
    instance_rel(&mut h, &[]);
    checkout_host(&h, None);
    let r = h.update(&["--check"]);
    assert_eq!(r.rc, 3, "{}", r.out);
    has(
        &r.out,
        "release: installed none, this host ran from its platform checkout, newest v1.2.0",
    );
    has(&r.out, "== v1.0.0");
    has(&r.out, "== v1.1.0");
    has(&r.out, "== v1.2.0");
    has(&r.out, "release: would switch to v1.2.0");
}

#[test]
fn no_checkout_on_the_host_is_shown_every_release_as_before() {
    let h = at_v1();
    fs::remove_file(h.home.join("current")).unwrap();
    let r = h.update(&["--check"]);
    assert_eq!(r.rc, 3, "{}", r.out);
    has(
        &r.out,
        "release: installed none, this host ran from its platform checkout, newest v1.2.0",
    );
    has(&r.out, "== v1.0.0");
    has(&r.out, "release: would switch to v1.2.0");
}

#[test]
fn an_update_from_a_tagged_checkout_at_the_newest_release_still_downloads_and_switches() {
    let mut h = host();
    instance_rel(&mut h, &[]);
    checkout_host(&h, Some("v1.2.0"));
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "this host ran from its platform checkout at v1.2.0");
    lacks(&r.out, "== v1");
    has(
        &r.out,
        "release: current is v1.2.0, was the platform checkout",
    );
    assert_eq!(current(&h), "versions/v1.2.0");
    has(&h.calls("gh"), "release download v1.2.0");
    assert_eq!(h.calls("install"), "install v1.2.0 ");
}

// Releases are built for x86_64 only, so an arm machine stops the same way.
fn unsupported(machine: &str) {
    let mut h = host();
    instance_rel(&mut h, &[]);
    host_at_arch(&h, machine);
    h.stub("uname", &format!("echo {machine}"));
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        &format!("releases are published for x86_64 only, and uname -m says {machine}"),
    );
    lacks(&r.out, "downloaded, checked");
    // No directory to download into was made.
    lacks(&r.out, &s(&h.home.join("tmp")));
    lacks(&h.calls("gh"), "release download");
    lacks(&h.calls("curl"), "releases/download");
    assert_eq!(h.calls("install"), "");
    assert_eq!(current(&h), "versions/v1.0.0");
    assert_eq!(versions(&h), "v1.0.0");
}

#[test]
fn an_unsupported_architecture_riscv64_stops_before_the_download_and_names_what_uname_m_said() {
    unsupported("riscv64");
}

#[test]
fn an_unsupported_architecture_aarch64_stops_before_the_download_and_names_what_uname_m_said() {
    unsupported("aarch64");
}

#[test]
fn an_unsupported_architecture_arm64_stops_before_the_download_and_names_what_uname_m_said() {
    unsupported("arm64");
}

// The machines a release exists for: the asset asked for is the x86_64 one.
fn supported(machine: &str) {
    let mut h = host();
    instance_rel(&mut h, &[]);
    host_at_arch(&h, machine);
    h.stub("uname", &format!("echo {machine}"));
    let r = h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &h.calls("gh"),
        "release download v1.2.0 --repo tehas-app/tehas",
    );
    has(
        &h.calls("gh"),
        &format!("tehas-v1.2.0-linux-{}.tar.gz", arch_of(machine).unwrap()),
    );
    assert_eq!(current(&h), "versions/v1.2.0");
    assert_eq!(
        fs::read_to_string(h.home.join("versions/v1.2.0/VERSION")).unwrap(),
        "v1.2.0\n"
    );
}

#[test]
fn on_a_machine_whose_uname_m_says_x86_64_the_download_is_of_that_asset() {
    supported("x86_64");
}

#[test]
fn on_a_machine_whose_uname_m_says_amd64_the_download_is_of_that_asset() {
    supported("amd64");
}

#[test]
fn a_machine_whose_uname_answers_nothing_asks_for_nothing_and_stops_with_the_reason() {
    let mut h = host();
    instance_rel(&mut h, &[]);
    host_at_arch(&h, "riscv64");
    h.stub("uname", "exit 127");
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "uname -m says nothing");
    lacks(&h.calls("gh"), "release download");
    assert_eq!(current(&h), "versions/v1.0.0");
}

#[test]
fn the_version_current_names_and_the_two_switched_to_before_it_stay_older_ones_go() {
    let mut h = host();
    instance_rel(&mut h, &[]);
    host_at(&h, "v1.0.0");
    make_release(&h, "v1.3.0");
    make_release(&h, "v1.4.0");
    let mut r = None;
    for v in ["v1.1.0", "v1.2.0", "v1.3.0", "v1.4.0"] {
        let run = h.update(&["--to", v]);
        assert_eq!(run.rc, 0, "{}", run.out);
        r = Some(run);
    }
    assert_eq!(versions(&h), "v1.2.0\nv1.3.0\nv1.4.0");
    has(
        &r.unwrap().out,
        "release: removed v1.1.0, older than the last three",
    );
    has(&h.calls("docker"), "image rm tehas/pi-rukas:release-v1.0.0");
}

#[test]
fn a_tarball_holding_a_member_outside_its_top_directory_is_refused() {
    let mut h = host();
    instance_rel(&mut h, &[]);
    host_at(&h, "v1.0.0");
    let evil = h.join("evil");
    fs::create_dir_all(evil.join("tehas-v1.2.0")).unwrap();
    fs::write(evil.join("tehas-v1.2.0/VERSION"), "v1.2.0\n").unwrap();
    fs::write(evil.join("outside"), "x\n").unwrap();
    let out = rel_dir(&h).join("v1.2.0");
    let name = asset_of("v1.2.0");
    sh_ok(
        &evil,
        &format!("tar -czf '{}/{name}' tehas-v1.2.0 outside", s(&out)),
    );
    sh_ok(&out, &format!("sha256sum '{name}' >'{name}.sha256'"));
    let r = h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "holds what a release does not: outside");
    assert_eq!(versions(&h), "v1.0.0");
}
