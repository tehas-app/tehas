//! What the spec pipe hands a cycle of what was approved: the spec its
//! approval pinned and the approved title and body, and the runner it needs
//! for that. The feature is still being built, so a test turns it on with
//! TEHAS_TEST_FEATURES, as a test build may.

mod work;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use work::{absent, has, World, A, F, PNG, WANT};

const SPEC: &str = "# Faster search\n\nR1: under 100 ms.\n";
const TITLE: &str = "Faster search";

fn link() -> String {
    format!("{F}/12/search.md")
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn sum() -> String {
    sha(SPEC.as_bytes())
}

fn body() -> String {
    format!("Make it fast.\n\nSpec: {} sha256:{}", link(), sum())
}

/// The approval pins the spec.
fn pin_spec(w: &World) {
    let (l, s) = (link(), sum());
    w.record_edit(|v| v["spec"] = json!({"link": l, "sha256": s}));
}

/// Nothing given of the issue's files unless the settings say so, the spec on
/// the net, the issue and its approval with the pin, and a runner that reads
/// the handoff.
fn spec_case(w: &World, more: &[&str]) {
    w.images_off();
    let mut all = vec!["TEHAS_WORK_IMAGES=0", "TEHAS_WORK_FILES=0"];
    all.extend_from_slice(more);
    w.minimal(&all);
    w.net(&link(), SPEC);
    w.issue_is(&body(), "OWNER", TITLE);
    w.record_is("alice", &body(), &[]);
    w.record_binds_title(TITLE);
    pin_spec(w);
    w.caps_good();
    w.caps_edit(|v| v["features"]["issueSpec"] = json!(true));
}

/// The same with the feature on.
fn pipe_on(w: &World, more: &[&str]) {
    let mut all = vec!["TEHAS_SPEC_PIPE=on"];
    all.extend_from_slice(more);
    spec_case(w, &all);
    w.env("TEHAS_TEST_FEATURES", "spec-pipe");
}

fn world(name: &str) -> World {
    let w = World::new(name);
    w.stub_github();
    w
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

fn names(v: Vec<String>) -> Vec<String> {
    v.into_iter()
        .map(|e| e.split_once('=').map_or(e.clone(), |(n, _)| n.to_string()))
        .collect()
}

fn lines(s: &str) -> Vec<String> {
    s.lines().map(String::from).collect()
}

/// The entries under the attachments, as `find . -mindepth 1` names them,
/// sorted, each followed by a space.
fn given(imgs: &Path) -> String {
    let mut v: Vec<String> = fs::read_dir(imgs)
        .unwrap()
        .flatten()
        .map(|e| format!("./{} ", e.file_name().to_string_lossy()))
        .collect();
    v.sort();
    v.concat()
}

fn docker_runs(w: &World) -> usize {
    fs::read_dir(w.path("calls"))
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("docker.args."))
        .count()
}

fn inside_images(w: &World) -> String {
    format!("{}.images", w.inside.display())
}

#[test]
fn with_the_feature_off_a_pinned_spec_is_not_fetched_and_the_cycle_is_given_nothing_more() {
    let w = world("spec-off");
    spec_case(&w, &[]);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(w.calls("curl"), "");
    assert!(absent(&w.imgs));
    assert_eq!(sorted(w.after("-v")), sorted(lines(&w.mounts_off())));
    assert_eq!(sorted(names(w.after("-e"))), sorted(lines(WANT)));
}

#[test]
fn with_the_feature_off_the_switch_alone_does_nothing_and_images_are_given_as_before() {
    let w = world("spec-switch");
    spec_case(&w, &["TEHAS_SPEC_PIPE=on", "TEHAS_WORK_IMAGES=1"]);
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "spec-pipe");
    assert_eq!(format!("{}{}", w.calls("curl"), w.gh()), "");
    spec_case(&w, &["TEHAS_WORK_IMAGES=1"]);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(
        w.manifest(r#"[has("spec"), has("approvedText")]"#),
        "[false,false]"
    );
    assert_eq!(given(&w.imgs), "./manifest.json ");
}

#[test]
fn with_the_feature_on_a_runner_that_does_not_read_the_spec_starts_nothing_78_named_nothing_fetched(
) {
    let w = world("spec-caps");
    pipe_on(&w, &[]);
    w.caps_good();
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "the feature issueSpec");
    assert_eq!(w.calls("curl"), "");
    assert!(w.cycle_args().is_empty());
    assert!(absent(&w.imgs));
}

#[test]
fn with_the_feature_on_the_cycle_is_given_the_spec_of_the_pinned_bytes_and_the_approved_text_in_one_read_only_directory(
) {
    let w = world("spec-on");
    pipe_on(&w, &[]);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    let inside = inside_images(&w);
    let mut mounts = lines(&w.mounts_off());
    mounts.push(format!("{}:{inside}:ro", w.imgs.display()));
    assert_eq!(sorted(w.after("-v")), sorted(mounts));
    let mut want = lines(WANT);
    want.push("PI_ENSEMBLE_EXTRA_IMAGE_DIRS".into());
    assert_eq!(sorted(names(w.after("-e"))), sorted(want));
    let dirs: Vec<String> = w
        .after("-e")
        .into_iter()
        .filter(|e| e.starts_with("PI_ENSEMBLE_EXTRA_IMAGE_DIRS"))
        .collect();
    assert_eq!(dirs, [format!("PI_ENSEMBLE_EXTRA_IMAGE_DIRS={inside}")]);
    assert_eq!(given(&w.imgs), "./approved.json ./manifest.json ./spec.md ");
    assert_eq!(fs::read(w.imgs.join("spec.md")).unwrap(), SPEC.as_bytes());
    assert_eq!(
        w.manifest(".spec"),
        format!(
            "{{\"file\":\"spec.md\",\"link\":\"{}\",\"name\":\"search.md\",\"bytes\":{},\"sha256\":\"{}\",\"block\":1}}",
            link(),
            SPEC.len(),
            sum()
        )
    );
    assert_eq!(
        w.manifest("[.version, .issue, .approval.by, .images, (keys_unsorted | .[-2:])]"),
        r#"[1,7,"alice",[],["spec","approvedText"]]"#
    );
    let approved = fs::read(w.imgs.join("approved.json")).unwrap();
    let a: Value = serde_json::from_slice(&approved).unwrap();
    assert_eq!(
        json!([a["number"], a["title"], a["body"]]),
        json!([7, TITLE, body()])
    );
    assert_eq!(
        w.manifest(".approvedText | [.file, .bytes, .sha256]"),
        format!(
            "[\"approved.json\",{},\"{}\"]",
            approved.len(),
            sha(&approved)
        )
    );
    has(
        &r.out,
        &format!(
            "tehas work: the cycle is given its spec {} (sha256 {}) and the approved title and body",
            link(),
            sum()
        ),
    );
    assert_eq!(
        w.calls("curl")
            .lines()
            .filter(|l| l.contains(&link()))
            .count(),
        1
    );
    has(
        &w.calls("curl-headers"),
        &format!("{} Authorization: Bearer ghs_app", link()),
    );
}

#[test]
fn with_the_feature_on_a_spec_whose_bytes_changed_since_the_approval_starts_nothing_exit_4_said_on_the_issue(
) {
    let w = world("spec-changed");
    pipe_on(&w, &[]);
    w.net(&link(), "# Faster search\n\nR1: under 1 s.\n");
    let r = w.work7();
    assert_eq!(r.rc, 4, "{}", r.out);
    assert_eq!(docker_runs(&w), 0);
    assert!(absent(&w.imgs));
    has(
        &r.out,
        "tehas work: the bytes of its spec are not the ones approved: they have sha256",
    );
    has(
        &w.gh(),
        "issue comment 7 --repo acme/widget --body Tehas started no cycle: the bytes of its spec are not the ones approved",
    );
}

#[test]
fn with_the_feature_on_a_spec_that_cannot_be_fetched_starts_nothing() {
    let w = world("spec-unfetched");
    pipe_on(&w, &[]);
    let key: String = link()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    w.rm(&format!("fx/net/{key}"));
    let r = w.work7();
    assert_eq!(r.rc, 4, "{}", r.out);
    assert_eq!(docker_runs(&w), 0);
    has(
        &r.out,
        "tehas work: its spec could not be fetched (curl exited 22), not starting a cycle",
    );
}

#[test]
fn with_the_feature_on_an_approval_with_no_spec_hands_over_the_approved_text_alone_on_a_runner_that_need_not_read_it(
) {
    let w = world("spec-none");
    pipe_on(&w, &[]);
    w.record_edit(|v| {
        v.as_object_mut().unwrap().remove("spec");
    });
    w.caps_good();
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(w.calls("curl"), "");
    assert_eq!(given(&w.imgs), "./approved.json ./manifest.json ");
    assert_eq!(
        w.manifest(r#"[has("spec"), .approvedText.file]"#),
        r#"[false,"approved.json"]"#
    );
    has(
        &w.after("-v").join("\n"),
        &format!("{}:{}:ro", w.imgs.display(), inside_images(&w)),
    );
    has(
        &r.out,
        "tehas work: the cycle is given the approved title and body",
    );
}

#[test]
fn with_the_feature_on_and_images_given_the_spec_joins_the_images_in_the_same_manifest() {
    let w = world("spec-images");
    pipe_on(&w, &["TEHAS_WORK_IMAGES=1"]);
    let shot = format!("{A}/1111");
    w.net(&shot, PNG);
    let with = format!("{} ![shot]({shot})", body());
    w.issue_is(&with, "OWNER", TITLE);
    w.record_is("alice", &with, &[&shot]);
    w.record_binds_title(TITLE);
    pin_spec(&w);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(
        given(&w.imgs),
        "./approved.json ./image-1.png ./manifest.json ./spec.md "
    );
    assert_eq!(
        w.manifest("[(.images | map(.file)), .spec.sha256, .approvedText.file]"),
        format!("[[\"image-1.png\"],\"{}\",\"approved.json\"]", sum())
    );
    let kept: Value = serde_json::from_str(&fs::read_to_string(&w.kept).unwrap()).unwrap();
    assert!(kept.get("spec").is_none());
}

#[test]
fn with_the_feature_on_a_run_by_hand_is_its_own_approval_and_is_handed_nothing() {
    let w = world("spec-hand");
    pipe_on(&w, &[]);
    w.store_home();
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(w.calls("curl"), "");
    assert!(absent(&w.imgs));
}
