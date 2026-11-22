//! An issue that links what the cycle is not given.

mod work;

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tehas_core::home::Home;
use work::{git_out, has, lacks, World, A, F, GIF, PNG, WANT};

fn world(name: &str) -> World {
    let w = World::new(&format!("files-{name}"));
    w.stub_github();
    w
}

fn there(p: &Path) -> bool {
    p.symlink_metadata().is_ok()
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

fn names(v: Vec<String>) -> Vec<String> {
    sorted(
        v.into_iter()
            .map(|e| e.split('=').next().unwrap().to_string())
            .collect(),
    )
}

/// jq -c `filter` of the manifest the host keeps, or what jq says.
fn kept(w: &World, filter: &str) -> String {
    let out = Command::new("jq")
        .args(["-c", filter])
        .arg(&w.kept)
        .output()
        .unwrap();
    let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    s.trim_end_matches('\n').to_string()
}

fn kept_r(w: &World, filter: &str) -> String {
    let out = Command::new("jq")
        .args(["-r", filter])
        .arg(&w.kept)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .trim_end_matches('\n')
        .to_string()
}

/// The error lines of the run logs of #7.
fn gapline(w: &World) -> String {
    let Ok(d) = fs::read_dir(w.home.join("logs")) else {
        return String::new();
    };
    let mut files: Vec<PathBuf> = d
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let n = p.file_name().unwrap().to_string_lossy();
            n.starts_with("work-7-") && n.ends_with(".jsonl.err")
        })
        .collect();
    files.sort();
    let all: String = files
        .iter()
        .map(|p| fs::read_to_string(p).unwrap_or_default())
        .collect();
    all.trim_end_matches('\n').to_string()
}

/// The manifests kept for #7 under the cycles' state, any suffix.
fn kept_manifests(w: &World) -> usize {
    fn walk(dir: &Path, n: &mut usize) {
        let Ok(d) = fs::read_dir(dir) else { return };
        for e in d.flatten() {
            let p = e.path();
            if e.file_name()
                .to_string_lossy()
                .starts_with("7.manifest.json")
            {
                *n += 1;
            }
            if p.is_dir() && !p.is_symlink() {
                walk(&p, n);
            }
        }
    }
    let mut n = 0;
    walk(&Home::new(&w.home).cycles_dir(), &mut n);
    n
}

fn docker_runs(w: &World) -> usize {
    fs::read_dir(w.path("calls"))
        .map(|d| {
            d.flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with("docker.args."))
                .count()
        })
        .unwrap_or(0)
}

/// Every docker run recorded, its words one per line.
fn all_docker_words(w: &World) -> String {
    let mut names: Vec<String> = fs::read_dir(w.path("calls"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("docker.args."))
        .collect();
    names.sort();
    names.iter().map(|n| w.calls(n)).collect()
}

fn mounts_with(w: &World, extra: &[String]) -> Vec<String> {
    sorted(
        w.mounts_off()
            .lines()
            .map(String::from)
            .chain(extra.iter().cloned())
            .collect(),
    )
}

fn want_with(extra: &[&str]) -> Vec<String> {
    sorted(
        WANT.lines()
            .chain(extra.iter().copied())
            .map(String::from)
            .collect(),
    )
}

const NONE_GIVEN: &str = "tehas work: the approval record of issue 7 (approved 2026-10-14T00:00:00Z by alice) lists 2 attachments and this instance gives a cycle none";

// --- An issue that links what the cycle is not given -------------------------

#[test]
fn with_nothing_given_the_links_of_the_approval_record_are_said_and_kept_and_only_the_issues_text_is_asked_for(
) {
    let w = world("nothing-given");
    w.images_off();
    let body = format!("![shot]({A}/1111) [spec.md]({F}/31/spec.md)");
    let (l1, l2) = (format!("{A}/1111"), format!("{F}/31/spec.md"));
    w.record_is("alice", &body, &[&l1, &l2]);
    w.issue_is(&body, "NONE", "");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(w.gh(), "api repos/acme/widget/issues/7\n");
    assert_eq!(w.calls("curl"), "");
    assert!(!there(&w.imgs));
    assert_eq!(sorted(w.after("-v")), mounts_with(&w, &[]));
    assert_eq!(names(w.after("-e")), want_with(&[]));
    has(&r.out, NONE_GIVEN);
    assert_eq!(gapline(&w), format!("{NONE_GIVEN} (TEHAS_WORK_IMAGES=0): no directory is mounted and PI_ENSEMBLE_EXTRA_IMAGE_DIRS is not set"));
    assert_eq!(
        kept(&w, "{images, approval}"),
        r#"{"images":[],"approval":{"at":"2026-10-14T00:00:00Z","by":"alice","how":"label"}}"#
    );
    assert_eq!(
        kept(&w, "[.leftOut[] | [.link, .source]]"),
        format!("[[\"{l1}\",\"body\"],[\"{l2}\",\"body\"]]")
    );
    has(
        &kept_r(&w, ".leftOut[0].reason"),
        "no images (TEHAS_WORK_IMAGES=0)",
    );
    assert_eq!(kept_r(&w, ".linksUnavailable"), "This instance gives a cycle none (TEHAS_WORK_IMAGES=0): no directory is mounted and PI_ENSEMBLE_EXTRA_IMAGE_DIRS is not set.");
    assert_eq!(kept_manifests(&w), 1);
}

#[test]
fn with_nothing_given_an_instance_line_for_the_images_variable_is_not_passed_on_as_the_line_says() {
    let w = world("images-var-off");
    w.images_off();
    w.minimal(&[
        "TEHAS_WORK_IMAGES=0",
        "PI_ENSEMBLE_EXTRA_IMAGE_DIRS=/elsewhere",
    ]);
    let l = format!("{A}/1111");
    let body = format!("![shot]({l})");
    w.record_is("alice", &body, &[&l]);
    w.issue_is(&body, "NONE", "");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    let e = w.after("-e");
    assert_eq!(
        e.iter()
            .filter(|v| v.starts_with("PI_ENSEMBLE_EXTRA_IMAGE_DIRS"))
            .count(),
        0
    );
    lacks(&e.join("\n"), "/elsewhere");
    lacks(&all_docker_words(&w), "/elsewhere");
    assert_eq!(names(e), want_with(&[]));
    has(&gapline(&w), "PI_ENSEMBLE_EXTRA_IMAGE_DIRS is not set");
}

#[test]
fn one_link_is_said_as_one_attachment() {
    let w = world("one-link");
    w.images_off();
    let l = format!("{A}/1111");
    let body = format!("![shot]({l})");
    w.record_is("alice", &body, &[&l]);
    w.issue_is(&body, "NONE", "");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    has(
        &r.out,
        "lists 1 attachment and this instance gives a cycle none",
    );
    assert_eq!(kept(&w, ".leftOut | length"), "1");
}

#[test]
fn what_the_body_gained_since_the_approval_is_not_counted_as_attachments_the_start_stops_and_nothing_is_said_about_a_cycle_given_none(
) {
    let w = world("body-gained");
    w.images_off();
    let l = format!("{A}/1111");
    w.record_is("alice", &format!("![shot]({l})"), &[&l]);
    w.issue_is(
        &format!("![shot]({l}) ![added since]({A}/2222)"),
        "NONE",
        "",
    );
    let r = w.work7();
    assert_eq!(r.rc, 4, "{}", r.out);
    has(
        &r.out,
        "the issue's text is not the text that was approved: it changed after the approval",
    );
    assert_eq!(gapline(&w), "");
    assert_eq!(kept_manifests(&w), 0);
    lacks(&w.gh(), "2222");
    assert_eq!(w.calls("curl"), "");
}

#[test]
fn the_manifest_lists_at_most_100_links_and_says_how_many_more_the_record_lists() {
    let w = world("hundred");
    w.images_off();
    let links: Vec<String> = (1..=130).map(|i| format!("{A}/{i}")).collect();
    let body: String = (1..=130).map(|i| format!("![x{i}]({A}/{i}) ")).collect();
    let refs: Vec<&str> = links.iter().map(String::as_str).collect();
    w.record_is("alice", &body, &refs);
    w.issue_is(&body, "NONE", "");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    has(
        &r.out,
        "lists 130 attachments and this instance gives a cycle none",
    );
    has(&r.out, "its manifest lists the first 100 of them");
    assert_eq!(kept(&w, ".leftOut | length"), "100");
    assert_eq!(kept(&w, ".leftOutMore"), "30");
    assert_eq!(kept_r(&w, ".leftOut[99].link"), format!("{A}/100"));
    lacks(&kept_r(&w, ".leftOut[].link"), &format!("{A}/101"));
    assert!(fs::metadata(&w.kept).unwrap().len() < 262144);
    has(&gapline(&w), "its manifest lists the first 100 of them");
    lacks(&gapline(&w), "could not write");
}

#[test]
fn a_manifest_path_that_is_a_link_is_not_written_through_and_the_cycle_starts() {
    let w = world("manifest-link");
    w.images_off();
    let l = format!("{A}/1111");
    let body = format!("![shot]({l})");
    w.record_is("alice", &body, &[&l]);
    w.issue_is(&body, "NONE", "");
    fs::create_dir_all(w.kept.parent().unwrap()).unwrap();
    let elsewhere = w.path("elsewhere");
    fs::write(&elsewhere, r#"{"images": [], "leftOut": []}"#).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &w.kept).unwrap();
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    lacks(&r.out, "gives a cycle none");
    assert_eq!(
        fs::read_to_string(&elsewhere).unwrap(),
        r#"{"images": [], "leftOut": []}"#
    );
    let mut new = w.kept.clone().into_os_string();
    new.push(".new");
    assert!(!there(Path::new(&new)));
}

#[test]
fn an_issue_never_dispatched_has_no_record_so_nothing_is_said_nothing_is_written_and_the_run_says_it_is_its_own_approval(
) {
    let w = world("never-dispatched");
    w.images_off();
    w.issue_is(&format!("![shot]({A}/1111)"), "OWNER", "");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    lacks(&r.out, "gives a cycle none");
    has(
        &r.out,
        "tehas work: no approval is recorded on this host for #7; a run by hand is its own approval",
    );
    assert_eq!(gapline(&w), "");
    assert_eq!(kept_manifests(&w), 0);
    assert_eq!(format!("{}{}", w.gh(), w.calls("curl")), "");
}

#[test]
fn an_issue_that_links_nothing_is_given_nothing_and_says_nothing() {
    let w = world("links-nothing");
    w.images_off();
    w.record_is("alice", "Words only.", &[]);
    w.issue_is("Words only.", "NONE", "");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    lacks(&r.out, "gives a cycle none");
    assert_eq!(kept_manifests(&w), 0);
}

#[test]
fn an_approval_the_store_cannot_read_starts_no_cycle_exit_4_nothing_kept_and_the_issue_is_told() {
    let w = world("store-busy");
    w.images_off();
    let l = format!("{A}/1111");
    let body = format!("![shot]({l})");
    w.record_is("alice", &body, &[&l]);
    w.store_answers(
        "approval-view",
        1,
        "store: busy #7: the database is locked",
        0,
    );
    w.issue_is(&body, "NONE", "");
    let r = w.work7();
    assert_eq!(r.rc, 4, "{}", r.out);
    lacks(&r.out, "gives a cycle none");
    has(
        &r.out,
        "tehas work: the approval of this issue could not be read, not starting a cycle",
    );
    assert_eq!(docker_runs(&w), 0);
    assert_eq!(gapline(&w), "");
    assert_eq!(kept_manifests(&w), 0);
}

#[test]
fn a_manifest_kept_for_the_issue_that_is_not_json_says_so_once_and_the_cycle_starts() {
    let w = world("manifest-bad");
    w.images_off();
    let l = format!("{A}/1111");
    let body = format!("![shot]({l})");
    w.record_is("alice", &body, &[&l]);
    w.issue_is(&body, "NONE", "");
    fs::create_dir_all(w.kept.parent().unwrap()).unwrap();
    fs::write(&w.kept, "{images\n").unwrap();
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    lacks(&r.out, "gives a cycle none");
    has(
        &r.out,
        "the approval record of issue 7, or the manifest kept for it, could not be read",
    );
    assert_eq!(gapline(&w), "tehas work: the approval record of issue 7, or the manifest kept for it, could not be read (attach: left-out exited 1): nothing is said about what this cycle was not given");
    assert_eq!(kept_manifests(&w), 1);
}

#[test]
fn a_link_of_the_record_that_is_not_of_githubs_storage_is_not_counted() {
    let w = world("not-github");
    w.images_off();
    w.record_is(
        "alice",
        "see https://example.com/x.png",
        &["https://example.com/x.png"],
    );
    w.issue_is("see https://example.com/x.png", "NONE", "");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    lacks(&r.out, "gives a cycle none");
    assert_eq!(kept_manifests(&w), 0);
}

#[test]
fn a_record_written_while_the_instance_took_links_from_comments_keeps_them_each_with_its_source() {
    let w = world("comment-links");
    w.images_off();
    let l = format!("{A}/1111");
    let body = format!("![shot]({l})");
    w.record_is("alice", &body, &[&l]);
    w.record_edit(|v| {
        v["comment_links"] = serde_json::json!([
            {"link": format!("{A}/2222"), "source": "comment:51", "by": "alice"},
            {"link": "https://example.com/y.png", "source": "comment:52", "by": "alice"}
        ])
    });
    w.issue_is(&body, "NONE", "");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    has(&r.out, "lists 2 attachments");
    assert_eq!(
        kept(&w, "[.leftOut[] | [.link, .source]]"),
        format!("[[\"{l}\",\"body\"],[\"{A}/2222\",\"comment:51\"]]")
    );
}

/// The issue and its approval: one image at `A/1111`.
fn one_image(w: &World) {
    let l = format!("{A}/1111");
    let body = format!("![shot]({l})");
    w.issue_is(&body, "NONE", "");
    w.record_is("alice", &body, &[&l]);
}

#[test]
fn the_manifest_of_a_cycle_given_nothing_does_not_hold_a_later_cycle_that_is_given_the_image() {
    let w = world("gap-then-image");
    w.images_off();
    one_image(&w);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    let gap = fs::read(&w.kept).unwrap();
    w.images_on(&[]);
    w.net(&format!("{A}/1111"), PNG);
    one_image(&w);
    fs::write(&w.kept, &gap).unwrap();
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(
        w.manifest(".images | map(.link)"),
        format!("[\"{A}/1111\"]")
    );
}

#[test]
fn the_manifest_of_a_cycle_given_nothing_keeps_what_an_earlier_cycle_of_the_same_approval_recorded()
{
    let w = world("gap-keeps");
    w.images_on(&[]);
    w.net(&format!("{A}/1111"), PNG);
    one_image(&w);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    let sum = sha(PNG);
    let give1 = fs::read(&w.kept).unwrap();
    w.images_off();
    fs::create_dir_all(w.kept.parent().unwrap()).unwrap();
    fs::write(&w.kept, &give1).unwrap();
    one_image(&w);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    has(
        &r.out,
        "lists 1 attachment and this instance gives a cycle none",
    );
    assert_eq!(
        kept(&w, "[.images[] | [.file, .link, .sha256]]"),
        format!("[[\"image-1.png\",\"{A}/1111\",\"{sum}\"]]")
    );
    assert_eq!(kept(&w, "[.leftOut[].link]"), format!("[\"{A}/1111\"]"));
    let gap = fs::read(&w.kept).unwrap();
    // The bytes an earlier cycle fetched are still held against this approval:
    // a cycle given the image again is stopped when the link serves other bytes.
    w.images_on(&[]);
    w.net(&format!("{A}/1111"), GIF);
    one_image(&w);
    fs::write(&w.kept, &gap).unwrap();
    let r = w.work7();
    assert_eq!(r.rc, 4, "{}", r.out);
    has(
        &r.out,
        &format!("{A}/1111 gave other bytes than when this approval was first fetched"),
    );
}

#[test]
fn a_cycle_the_push_check_stopped_leaves_no_copy_of_the_index_beside_its_workspace() {
    let w = world("push-stopped");
    w.images_on(&[]);
    w.issue_is("Words only.", "", "");
    let _ = fs::remove_dir_all(&w.hostindex);
    fs::create_dir_all(&w.hostindex).unwrap();
    fs::write(w.hostindex.join("widget.db"), "index\n").unwrap();
    w.env("PUSH_RC", "1");
    let r = w.work7();
    assert_eq!(r.rc, 3, "{}", r.out);
    has(&r.out, "cannot push to origin");
    assert!(there(&w.ws));
    assert!(!there(&w.state));
}

// --- pi-rukas's own state in the clone ---------------------------------------

#[test]
fn the_clone_keeps_pi_rukass_state_files_out_of_git_so_the_product_needs_no_gitignore_lines() {
    let w = world("exclude");
    w.images_off();
    w.issue_is("a plain issue", "NONE", "");
    w.work7();
    let exclude = fs::read_to_string(w.ws.join(".git/info/exclude")).unwrap();
    let want = [
        "/.pi/work-state/",
        "/.pi/permissions.json",
        "/.pi/decisions.json",
    ];
    assert_eq!(exclude.lines().filter(|l| want.contains(l)).count(), 3);
    fs::create_dir_all(w.ws.join(".pi/work-state")).unwrap();
    for f in [
        ".pi/work-state/7.json",
        ".pi/permissions.json",
        ".pi/decisions.json",
    ] {
        fs::write(w.ws.join(f), "").unwrap();
    }
    assert_eq!(git_out(&w.ws, &["status", "--porcelain"]), "");
}
