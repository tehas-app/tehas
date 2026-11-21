//! The text files an issue links, and an issue that links what the cycle is
//! not given.

mod work;

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use work::{
    count_of, git_out, has, lacks, security_of, unknown_options, World, A, DOCKER_OPTS, F, GIF,
    MINIMAL, PNG, WANT,
};

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
    walk(&w.home.join("state/cycles"), &mut n);
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

fn images_mount(w: &World) -> String {
    format!("{}:{}.images:ro", w.imgs.display(), w.inside.display())
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
    assert_eq!(gapline(&w), format!("{NONE_GIVEN} (TEHAS_WORK_IMAGES=0, TEHAS_WORK_FILES=0): no directory is mounted and PI_ENSEMBLE_EXTRA_IMAGE_DIRS is not set"));
    assert_eq!(
        kept(&w, "{images, approval}"),
        r#"{"images":[],"approval":{"at":"2026-10-14T00:00:00Z","by":"alice","how":"label"}}"#
    );
    assert_eq!(
        kept(&w, "[.leftOut[] | [.link, .source]]"),
        format!("[[\"{l1}\",\"body\"],[\"{l2}\",\"body\"]]")
    );
    has(&kept_r(&w, ".leftOut[0].reason"), "no images and no files");
    assert_eq!(kept_r(&w, ".linksUnavailable"), "This instance gives a cycle none (TEHAS_WORK_IMAGES=0, TEHAS_WORK_FILES=0): no directory is mounted and PI_ENSEMBLE_EXTRA_IMAGE_DIRS is not set.");
    assert_eq!(kept_manifests(&w), 1);
}

#[test]
fn with_nothing_given_an_instance_line_for_the_images_variable_is_not_passed_on_as_the_line_says() {
    let w = world("images-var-off");
    w.images_off();
    w.minimal(&[
        "TEHAS_WORK_IMAGES=0",
        "TEHAS_WORK_FILES=0",
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

// --- The issue's text files ----------------------------------------------------

fn files_on(w: &World, more: &[&str]) {
    w.images_on(more);
    let mut all: Vec<&str> = MINIMAL.to_vec();
    all.push("TEHAS_WORK_FILES=1");
    all.extend_from_slice(more);
    w.instance(&all);
}

fn files(w: &World) -> String {
    w.manifest("[.files[] | [.file, .link, .name, .type, .bytes]]")
}

fn left(w: &World) -> String {
    w.manifest("[.leftOut[] | [.link, .reason]]")
}

/// What the cycle's directory holds, as `find . -mindepth 1` lists it, each
/// entry followed by a space.
fn listing(w: &World) -> String {
    let mut v: Vec<String> = fs::read_dir(&w.imgs)
        .unwrap()
        .flatten()
        .map(|e| format!("./{}", e.file_name().to_string_lossy()))
        .collect();
    v.sort();
    v.iter().map(|e| format!("{e} ")).collect()
}

#[test]
fn a_files_setting_that_is_not_0_or_1_starts_nothing_and_asks_nobody() {
    let w = world("files-yes");
    files_on(&w, &[]);
    w.minimal(&["TEHAS_WORK_FILES=yes"]);
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    assert_eq!(
        format!("{}{}{}", w.gh(), w.calls("curl"), w.calls("docker")),
        ""
    );
}

/// The start of a cycle given one file, spec.md, and no image.
fn files_alone(name: &str) -> World {
    let w = world(name);
    files_on(&w, &[]);
    w.net(&format!("{F}/31/spec.md"), "# Spec\nDo it.\n");
    w.issue_is(&format!("[spec.md]({F}/31/spec.md)"), "", "");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    w
}

#[test]
fn with_files_alone_the_cycle_gets_the_one_directory_read_only_and_the_one_variable_as_with_images()
{
    let w = files_alone("files-alone");
    assert_eq!(sorted(w.after("-v")), mounts_with(&w, &[images_mount(&w)]));
    assert_eq!(
        names(w.after("-e")),
        want_with(&["PI_ENSEMBLE_EXTRA_IMAGE_DIRS"])
    );
    let dirs: Vec<String> = w
        .after("-e")
        .into_iter()
        .filter(|e| e.starts_with("PI_ENSEMBLE_EXTRA_IMAGE_DIRS"))
        .collect();
    assert_eq!(
        dirs,
        [format!(
            "PI_ENSEMBLE_EXTRA_IMAGE_DIRS={}.images",
            w.inside.display()
        )]
    );
    let ca = w.cycle_args();
    assert_eq!(
        unknown_options(
            &ca,
            &w.image,
            &format!("--name {DOCKER_OPTS} --user --cidfile -w -v -e"),
            "--rm -i --init"
        ),
        Vec::<String>::new()
    );
    assert_eq!(count_of(&ca, "--tmpfs"), 1);
    assert_eq!(
        security_of(&w.preflight_args(), "-v"),
        [format!("{}:{}", w.ws.display(), w.inside.display())]
    );
}

#[test]
fn with_files_alone_the_comment_says_nothing_of_images() {
    let w = files_alone("files-alone-comment");
    has(
        &w.gh(),
        "Tehas fetched 1 file from the issue text for this cycle",
    );
    lacks(&w.gh(), "image from the issue text");
    lacks(&w.gh(), "sandbox image does not say");
}

/// The start of a cycle given six text files of an issue that links nine.
fn many_files(name: &str) -> World {
    let w = world(name);
    files_on(&w, &[]);
    w.net(&format!("{F}/31/spec.md"), "# Spec\n");
    w.net(&format!("{F}/32/Data.CSV"), "a,b\n1,2\n");
    w.net(&format!("{F}/33/fix.json"), r#"{"a": 1}"#);
    w.net(&format!("{F}/34/ci.yml"), "on: push\n");
    w.net(&format!("{F}/35/page.html"), "<p>hi</p>");
    w.net(&format!("{F}/36/run.log"), "ok\n");
    w.net(&format!("{F}/37/all.zip"), "PK");
    w.net(&format!("{A}/1111"), PNG);
    w.net(&format!("{F}/38/shot.png"), PNG);
    w.issue_is(&format!("[spec.md]({F}/31/spec.md) [Data.CSV]({F}/32/Data.CSV) [fix.json]({F}/33/fix.json) [ci.yml]({F}/34/ci.yml)
[page.html]({F}/35/page.html) [run.log]({F}/36/run.log) [all.zip]({F}/37/all.zip) ![s]({A}/1111) [shot.png]({F}/38/shot.png)"), "", "");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    w
}

#[test]
fn text_files_are_taken_by_their_name_under_names_of_tehass_own_listed_with_their_name_type_size_and_hash(
) {
    let w = many_files("text-files");
    assert_eq!(
        listing(&w),
        "./file-1.md ./file-2.csv ./file-3.json ./file-4.yml ./file-5.html ./file-6.log ./manifest.json "
    );
    assert_eq!(files(&w), format!("[[\"file-1.md\",\"{F}/31/spec.md\",\"spec.md\",\"text/markdown\",7],[\"file-2.csv\",\"{F}/32/Data.CSV\",\"Data.CSV\",\"text/csv\",8],[\"file-3.json\",\"{F}/33/fix.json\",\"fix.json\",\"application/json\",8],[\"file-4.yml\",\"{F}/34/ci.yml\",\"ci.yml\",\"application/yaml\",9],[\"file-5.html\",\"{F}/35/page.html\",\"page.html\",\"text/html\",9],[\"file-6.log\",\"{F}/36/run.log\",\"run.log\",\"text/plain\",3]]"));
    assert_eq!(
        w.manifest(".files[0].sha256"),
        format!("\"{}\"", sha(b"# Spec\n"))
    );
    assert_eq!(w.manifest(".images"), "[]");
}

#[test]
fn with_images_off_an_image_and_another_type_are_left_out_and_not_fetched() {
    let w = many_files("images-off-left");
    let other = "Not a md, txt, csv, json, yaml, log, html, css or js file. This instance gives a cycle no images.";
    assert_eq!(
        left(&w),
        format!("[[\"{F}/37/all.zip\",\"{other}\"],[\"{A}/1111\",\"{other}\"],[\"{F}/38/shot.png\",\"{other}\"]]")
    );
    let curl = w.calls("curl");
    lacks(&curl, "all.zip");
    lacks(&curl, &format!("{A}/1111"));
    lacks(&curl, "shot.png");
}

#[test]
fn the_comment_says_the_files_were_fetched_and_are_not_shown_yet_with_each_hash() {
    let w = many_files("files-comment");
    has(&w.gh(), "--body Tehas fetched 6 files from the issue text for this cycle and lists them in its manifest. The pi-rukas in the sandbox image does not read them yet: its agents are not shown them.");
    has(
        &w.gh(),
        &format!("- {F}/31/spec.md: file-1.md, sha256 {}", sha(b"# Spec\n")),
    );
    lacks(&w.gh(), "Tehas gave this cycle");
}

#[test]
fn bytes_that_are_not_utf_8_text_or_hold_a_nul_are_left_out_with_the_reason() {
    let w = world("not-utf8");
    files_on(&w, &[]);
    w.net(&format!("{F}/31/bad.txt"), b"caf\xe9\n");
    w.net(&format!("{F}/32/nul.txt"), b"a\x00b");
    w.net(&format!("{F}/33/ok.txt"), b"caf\xc3\xa9\n");
    w.issue_is(
        &format!("[bad.txt]({F}/31/bad.txt) [nul.txt]({F}/32/nul.txt) [ok.txt]({F}/33/ok.txt)"),
        "",
        "",
    );
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    let why = "Not valid UTF-8 text, or it holds a NUL byte.";
    assert_eq!(
        left(&w),
        format!("[[\"{F}/31/bad.txt\",\"{why}\"],[\"{F}/32/nul.txt\",\"{why}\"]]")
    );
    assert_eq!(listing(&w), "./file-1.txt ./manifest.json ");
}

#[test]
fn a_file_may_take_1_mb_and_all_files_4_mb_each_fetch_is_held_to_what_is_left() {
    let w = world("sizes");
    files_on(&w, &[]);
    let mut body = String::new();
    for i in 1..=5 {
        w.net(&format!("{F}/4{i}/f{i}.txt"), vec![b'a'; 900000]);
        body.push_str(&format!(" [f{i}.txt]({F}/4{i}/f{i}.txt)"));
    }
    w.net(&format!("{F}/49/big.txt"), vec![b'a'; 1100000]);
    w.issue_is(&format!("[big.txt]({F}/49/big.txt){body}"), "", "");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(
        w.manifest(".files | map(.name)"),
        r#"["f1.txt","f2.txt","f3.txt","f4.txt"]"#
    );
    assert_eq!(
        left(&w),
        format!("[[\"{F}/49/big.txt\",\"Over the 1 MB limit of a file.\"],[\"{F}/45/f5.txt\",\"Over the 4 MB limit of an issue's files.\"]]")
    );
    let re = regex::Regex::new(r"--max-filesize [0-9]*").unwrap();
    let sizes: String = re
        .find_iter(&w.calls("curl"))
        .map(|m| format!("{} ", m.as_str()))
        .collect();
    assert_eq!(sizes, "--max-filesize 1048576 --max-filesize 1048576 --max-filesize 1048576 --max-filesize 1048576 --max-filesize 1048576 --max-filesize 594304 ");
}

#[test]
fn at_most_sixteen_files_the_seventeenth_is_left_out_and_not_fetched() {
    let w = world("sixteen");
    files_on(&w, &[]);
    let mut body = String::new();
    for i in 1..=17 {
        w.net(&format!("{F}/5{i}/n{i}.txt"), "x");
        body.push_str(&format!(" [n{i}.txt]({F}/5{i}/n{i}.txt)"));
    }
    w.issue_is(&body, "", "");
    w.work7();
    assert_eq!(
        w.manifest("[(.files | length), (.leftOut | map([.link, .reason]))]"),
        format!("[16,[[\"{F}/517/n17.txt\",\"More than 16 files.\"]]]")
    );
    lacks(&w.calls("curl"), &format!("{F}/517/n17.txt"));
}

#[test]
fn an_outsiders_issue_is_given_no_file_without_an_approval_and_nothing_is_fetched() {
    let w = world("outsider");
    files_on(&w, &[]);
    w.net(&format!("{F}/31/spec.md"), "# Spec\n");
    w.issue_is(&format!("[spec.md]({F}/31/spec.md)"), "NONE", "");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(w.calls("curl"), "");
    assert_eq!(w.manifest(".files"), "[]");
}

#[test]
fn a_file_that_cannot_be_fetched_starts_no_cycle_exit_4_and_the_comment_names_files() {
    let w = world("cannot-fetch");
    files_on(&w, &[]);
    w.issue_is(&format!("[spec.md]({F}/31/spec.md)"), "", "");
    let r = w.work7();
    assert_eq!(r.rc, 4, "{}", r.out);
    assert_eq!(docker_runs(&w), 0);
    has(&w.gh(), &format!("Tehas started no cycle: {F}/31/spec.md could not be fetched. This instance gives a cycle the files an issue links"));
}

/// The start of a cycle given one file and one image.
fn both(name: &str) -> World {
    let w = world(name);
    files_on(&w, &["TEHAS_WORK_IMAGES=1"]);
    w.net(&format!("{F}/31/spec.md"), "# Spec\n");
    w.net(&format!("{A}/1111"), PNG);
    w.reads_image("1");
    w.issue_is(&format!("[spec.md]({F}/31/spec.md) ![s]({A}/1111)"), "", "");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    w
}

#[test]
fn with_images_and_files_both_on_there_is_still_one_directory_and_one_variable_and_both_kinds_are_in_it(
) {
    let w = both("both");
    assert_eq!(listing(&w), "./file-1.md ./image-1.png ./manifest.json ");
    assert_eq!(sorted(w.after("-v")), mounts_with(&w, &[images_mount(&w)]));
    assert_eq!(
        names(w.after("-e")),
        want_with(&["PI_ENSEMBLE_EXTRA_IMAGE_DIRS"])
    );
    has(
        &w.gh(),
        "--body Tehas gave this cycle 1 image from the issue text. Tehas fetched 1 file from the issue text",
    );
}

#[test]
fn a_file_of_the_same_approval_that_gives_other_bytes_stops_the_start() {
    let w = both("other-bytes");
    let l = format!("{F}/31/spec.md");
    w.record_is("alice", &format!("[spec.md]({l}) ![s]({A}/1111)"), &[&l]);
    let _ = fs::remove_file(&w.kept);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    w.net(&l, "# Other\n");
    let r = w.work7();
    assert_eq!(r.rc, 4, "{}", r.out);
    has(
        &r.out,
        &format!("{l} gave other bytes than when this approval was first fetched"),
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
