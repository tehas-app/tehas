//! tehas work and the dispatcher's approval record: the text a cycle works on
//! is the text somebody approved, and the links and comments the record names
//! are what the cycle is given.

mod work;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use tehas_core::home::Home;
use work::{has, lacks, World, A, GIF, JPG, PNG, WANT};

fn world(name: &str) -> World {
    let w = World::new(name);
    w.stub_github();
    w
}

/// What a stub was called with, as `$(t_calls name)` gives it.
fn calls(w: &World, name: &str) -> String {
    w.calls(name).trim_end_matches('\n').to_string()
}

fn gh(w: &World) -> String {
    calls(w, "gh")
}

fn curl(w: &World) -> String {
    calls(w, "curl")
}

/// How many docker calls ran something: a container started for each.
fn containers(w: &World) -> usize {
    fs::read_dir(w.path("calls"))
        .map(|d| {
            d.flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with("docker.args."))
                .count()
        })
        .unwrap_or(0)
}

/// jq -c `filter` of `file`, slurped when `slurp`, or what jq says.
fn jq(file: &Path, slurp: bool, filter: &str) -> String {
    let mut c = Command::new("jq");
    c.arg("-c");
    if slurp {
        c.arg("-s");
    }
    let out = c.arg(filter).arg(file).output().unwrap();
    let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    s.trim_end_matches('\n').to_string()
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn same(a: &Path, b: &Path) -> bool {
    fs::read(a)
        .ok()
        .is_some_and(|x| fs::read(b).ok() == Some(x))
}

fn denied_text() -> &'static str {
    "Tehas started no cycle: the issue's text is not the text that was approved"
}

// --- The text a cycle works on -----------------------------------------------

#[test]
fn with_nothing_given_a_record_whose_hash_matches_the_issue_starts_the_cycle_and_the_issue_is_read_for_its_text_alone(
) {
    let w = world("text-match");
    w.images_off();
    w.issue_is("Words only.", "NONE", "");
    w.record_is("alice", "Words only.", &[]);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(gh(&w), "api repos/acme/widget/issues/7");
    assert_eq!(curl(&w), "");
}

#[test]
fn with_nothing_given_a_body_changed_after_the_approval_starts_no_cycle_exit_4_no_container_and_the_issue_says_what_changed(
) {
    let w = world("text-changed");
    w.images_off();
    w.issue_is("Words only, edited after the label.", "NONE", "");
    w.record_is("alice", "Words only.", &[]);
    let r = w.work7();
    assert_eq!(r.rc, 4, "{}", r.out);
    assert_eq!(containers(&w), 0);
    assert!(!w.imgs.exists());
    has(&r.out, "tehas work: the issue's text is not the text that was approved: it changed after the approval, not starting a cycle");
    has(&gh(&w), "issue comment 7 --repo acme/widget --body Tehas started no cycle: the issue's text is not the text that was approved: it changed after the approval. To retry, apply `tehas:ready`.");
}

#[test]
fn without_the_state_store_a_start_stops_at_78_before_github_is_asked() {
    let w = world("no-store");
    w.images_off();
    w.issue_is("Words only.", "NONE", "");
    w.record_is("alice", "Words only.", &[]);
    fs::remove_file(w.home.join("libexec/store")).unwrap();
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "tehas work: ERROR: the state store is not installed (run install.sh)",
    );
    assert_eq!(gh(&w), "");
    assert_eq!(containers(&w), 0);
}

// --- The title of the issue --------------------------------------------------

#[test]
fn with_nothing_given_a_record_that_binds_the_title_starts_the_cycle_on_the_one_read_of_the_issue()
{
    let w = world("title-bound");
    w.images_off();
    w.issue_is("Words only.", "NONE", "Faster search");
    w.record_is("alice", "Words only.", &[]);
    w.record_binds_title("Faster search");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(w.issue_reads(), 1);
}

#[test]
fn with_nothing_given_a_title_renamed_after_the_approval_starts_no_cycle_exit_4_no_container_and_the_issue_is_told_it_was_the_title(
) {
    let w = world("title-renamed");
    w.images_off();
    w.issue_is("Words only.", "NONE", "Faster search, renamed");
    w.record_is("alice", "Words only.", &[]);
    w.record_binds_title("Faster search");
    let r = w.work7();
    assert_eq!(r.rc, 4, "{}", r.out);
    assert_eq!(containers(&w), 0);
    has(&r.out, "tehas work: the issue's title is not the title that was approved: it changed after the approval, not starting a cycle");
    lacks(&r.out, "the issue's text is not the text");
    has(&gh(&w), "issue comment 7 --repo acme/widget --body Tehas started no cycle: the issue's title is not the title that was approved: it changed after the approval. To retry, apply `tehas:ready`.");
}

#[test]
fn with_nothing_given_an_answer_that_names_no_title_is_a_failed_read_exit_4_not_said_as_a_rename() {
    let w = world("title-none");
    w.images_off();
    w.issue_is("Words only.", "NONE", "");
    w.record_is("alice", "Words only.", &[]);
    w.record_binds_title("Faster search");
    let r = w.work7();
    assert_eq!(r.rc, 4, "{}", r.out);
    has(
        &r.out,
        "tehas work: the issue's text could not be read for its title, not starting a cycle",
    );
    lacks(&r.out, "changed after the approval");
}

#[test]
fn with_nothing_given_a_record_that_names_no_title_is_checked_on_the_body_alone_a_rename_starts_the_cycle(
) {
    let w = world("title-unbound");
    w.images_off();
    w.issue_is("Words only.", "NONE", "Renamed after the label");
    w.record_is("alice", "Words only.", &[]);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
}

const STORE_FAULTS: &[(i32, &str)] = &[
    (78, "store: corrupt: the file is not a database"),
    (1, "store: busy #7: the database is locked"),
];

#[test]
fn with_nothing_given_approvals_the_store_cannot_read_start_no_cycle_exit_4_and_nothing_falls_back_to_running_anyway(
) {
    let w = world("approval-unread");
    for (rc, line) in STORE_FAULTS {
        w.images_off();
        w.issue_is("Words only.", "NONE", "");
        w.record_is("alice", "Words only.", &[]);
        w.store_answers("approval-view", *rc, line, 0);
        let r = w.work7();
        assert_eq!(r.rc, 4, "{line}: {}", r.out);
        assert_eq!(containers(&w), 0);
        has(
            &r.out,
            "tehas work: the approval of this issue could not be read, not starting a cycle",
        );
        lacks(&r.out, "a run by hand");
        let calls = gh(&w);
        lacks(&calls, "events");
        lacks(&calls, &w.t().join("home").to_string_lossy());
        has(&calls, "Tehas started no cycle: the approval of this issue could not be read. To retry, apply `tehas:ready`.");
    }
}

#[test]
fn with_nothing_given_an_issue_that_cannot_be_read_starts_no_cycle_exit_4_and_it_is_not_said_as_a_change(
) {
    let w = world("issue-unread");
    w.images_off();
    w.record_is("alice", "Words only.", &[]);
    let _ = fs::remove_file(w.fx.join("issue.json"));
    let r = w.work7();
    assert_eq!(r.rc, 4, "{}", r.out);
    assert_eq!(containers(&w), 0);
    has(&r.out, "tehas work: the issue could not be read for its text: GitHub refused the read, not starting a cycle");
    lacks(&r.out, "changed after the approval");
    has(&gh(&w), "Tehas started no cycle: the issue could not be read for its text: GitHub refused the read. To retry, apply `tehas:ready`.");
}

#[test]
fn with_nothing_given_and_no_record_the_cycle_starts_and_the_line_says_a_run_by_hand_is_its_own_approval(
) {
    let w = world("no-record");
    w.images_off();
    w.issue_is("Words only.", "NONE", "");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    has(
        &r.out,
        "tehas work: no approval is recorded on this host for #7; a run by hand is its own approval",
    );
    lacks(&r.out, "changed");
    assert_eq!(gh(&w), "");
}

#[test]
fn with_nothing_given_an_approval_that_started_nothing_is_no_approval_the_run_is_by_hand() {
    let w = world("approval-unused");
    w.images_off();
    w.issue_is("Words only.", "NONE", "");
    w.approve(7, "alice", "approved", 1_760_000_000, &[]);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    has(
        &r.out,
        "tehas work: no approval is recorded on this host for #7; a run by hand is its own approval",
    );
    assert_eq!(gh(&w), "");
}

#[test]
fn with_nothing_given_an_issue_read_that_does_not_end_in_time_starts_no_cycle_exit_4_named_as_the_timeout_it_is(
) {
    let w = world("issue-slow");
    w.images_off();
    w.issue_is("Words only.", "NONE", "");
    w.record_is("alice", "Words only.", &[]);
    w.gh_stub(
        r#"case "$1 $2" in
  "api repos/acme/widget/issues/7") "$(command -pv sleep)" 20 ;;
esac"#,
    );
    w.env("TEHAS_TEXT_WAIT", "1");
    let r = w.work7();
    assert_eq!(r.rc, 4, "{}", r.out);
    has(&r.out, "tehas work: the issue could not be read for its text: no answer within 1s, not starting a cycle");
    lacks(&r.out, "changed after the approval");
}

#[test]
fn with_nothing_given_a_number_that_is_a_pull_request_starts_no_cycle_exit_4_before_its_text_is_hashed(
) {
    let w = world("issue-pr");
    w.images_off();
    w.record_is("alice", "Words only.", &[]);
    let v = json!({"number": 7, "state": "open", "title": "A pull request", "body": "Words only.",
        "author_association": "NONE",
        "pull_request": {"url": "https://api.github.com/repos/acme/widget/pulls/7"}});
    w.fx("issue.json", format!("{v}\n"));
    let r = w.work7();
    assert_eq!(r.rc, 4, "{}", r.out);
    has(
        &r.out,
        "tehas work: #7 is not an open issue, as GitHub answers for it, not starting a cycle",
    );
    lacks(&r.out, "changed after the approval");
}

#[test]
fn with_nothing_given_a_body_that_differs_only_in_line_ends_is_the_approved_text_the_cycle_starts_on_one_read(
) {
    let w = world("line-ends-off");
    w.images_off();
    w.record_is("alice", "line one\nWords only.", &[]);
    w.issue_is("line one\nWords only.", "NONE", "");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(w.issue_reads(), 1);
}

// --- The links of the record -------------------------------------------------

fn two_links() -> String {
    format!("![a]({A}/1111) ![b]({A}/2222)")
}

/// A record that names one of the body's two links, and a start of it.
fn record_links(w: &World) -> work::Run {
    w.images_on(&[]);
    w.net(&format!("{A}/1111"), PNG);
    w.net(&format!("{A}/2222"), JPG);
    let body = two_links();
    w.issue_is(&body, "NONE", "");
    w.record_is("alice", &body, &[&format!("{A}/1111")]);
    w.work7()
}

fn kept_first(w: &World) -> PathBuf {
    w.path("kept.first")
}

/// record_links, its manifest kept aside, and the same approval run again.
fn run_again(w: &World) -> work::Run {
    record_links(w);
    assert!(same(&w.kept, &w.imgs.join("manifest.json")));
    fs::copy(&w.kept, kept_first(w)).unwrap();
    w.clear_calls();
    w.work7()
}

#[test]
fn with_a_record_the_links_are_the_record_s_one_the_body_gained_since_is_not_fetched_and_no_events_are_read(
) {
    let w = world("links-record");
    let r = record_links(&w);
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(
        w.manifest(".images | map(.link)"),
        format!("[\"{A}/1111\"]")
    );
    lacks(&curl(&w), &format!("{A}/2222"));
    lacks(&gh(&w), "events");
    assert_eq!(w.issue_reads(), 1);
}

#[test]
fn each_image_is_listed_with_its_sha256_and_the_manifest_names_the_approval() {
    let w = world("links-sha");
    record_links(&w);
    assert_eq!(
        w.manifest(".images[0].sha256"),
        format!("\"{}\"", sha256_hex(PNG))
    );
    assert_eq!(
        w.manifest(".approval"),
        r#"{"at":"2026-10-14T00:00:00Z","by":"alice","how":"approved"}"#
    );
}

#[test]
fn the_comment_lists_each_image_with_its_sha256() {
    let w = world("links-comment");
    record_links(&w);
    has(
        &gh(&w),
        &format!("- {A}/1111: image-1.png, sha256 {}", sha256_hex(PNG)),
    );
}

#[test]
fn the_manifest_is_kept_in_the_state_directory_and_a_second_run_of_the_same_approval_yields_the_same_one(
) {
    let w = world("links-kept");
    let r = run_again(&w);
    assert_eq!(r.rc, 2, "{}", r.out);
    assert!(same(&w.kept, &kept_first(&w)));
}

#[test]
fn a_link_that_gives_other_bytes_for_the_same_approval_starts_no_cycle_exit_4_said_on_the_issue() {
    let w = world("links-other-bytes");
    run_again(&w);
    w.net(&format!("{A}/1111"), GIF);
    w.clear_calls();
    let r = w.work7();
    assert_eq!(r.rc, 4, "{}", r.out);
    has(&r.out, &format!("tehas work: {A}/1111 gave other bytes than when this approval was first fetched, not starting a cycle"));
    assert_eq!(containers(&w), 0);
    has(
        &gh(&w),
        &format!(
            "issue comment 7 --repo acme/widget --body Tehas started no cycle: {A}/1111 gave other bytes"
        ),
    );
    assert!(same(&w.kept, &kept_first(&w)));
}

#[test]
fn a_new_approval_of_the_same_link_is_not_held_to_the_bytes_of_the_old_one() {
    let w = world("links-new-approval");
    run_again(&w);
    w.images_on(&["TEHAS_ALLOWED_LABELERS=\"alice bob\""]);
    let body = two_links();
    w.record_is("bob", &body, &[&format!("{A}/1111")]);
    w.net(&format!("{A}/1111"), GIF);
    w.issue_is(&body, "NONE", "");
    fs::copy(kept_first(&w), &w.kept).unwrap();
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(w.manifest(".images | map(.file)"), r#"["image-1.gif"]"#);
}

#[test]
fn a_body_changed_since_the_approval_starts_no_cycle_exit_4_said_on_the_issue_nothing_fetched() {
    let w = world("links-changed");
    w.images_on(&[]);
    w.net(&format!("{A}/1111"), PNG);
    w.issue_is(&format!("![a]({A}/1111) and an edit"), "OWNER", "");
    w.record_is("alice", &format!("![a]({A}/1111)"), &[&format!("{A}/1111")]);
    let r = w.work7();
    assert_eq!(r.rc, 4, "{}", r.out);
    has(&r.out, "tehas work: the issue's text is not the text that was approved: it changed after the approval, not starting a cycle");
    assert_eq!(curl(&w), "");
    assert_eq!(containers(&w), 0);
    has(&gh(&w), denied_text());
}

#[test]
fn line_ends_do_not_count_in_the_hash() {
    let w = world("links-line-ends");
    w.images_on(&[]);
    w.net(&format!("{A}/1111"), PNG);
    w.issue_is(&format!("line one\r\n![a]({A}/1111)"), "OWNER", "");
    w.record_is(
        "alice",
        &format!("line one\n![a]({A}/1111)"),
        &[&format!("{A}/1111")],
    );
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(w.issue_reads(), 1);
}

#[test]
fn an_approval_that_cannot_be_read_starts_no_cycle_exit_4() {
    let w = world("links-unread");
    w.images_on(&[]);
    w.issue_is("Words only.", "OWNER", "");
    w.record_is("alice", "Words only.", &[]);
    w.store_answers(
        "approval-view",
        78,
        "store: unreadable: the store holds no schema",
        0,
    );
    let r = w.work7();
    assert_eq!(r.rc, 4, "{}", r.out);
    has(&r.out, "the approval of this issue could not be read");
    assert_eq!(containers(&w), 0);
    assert_eq!(curl(&w), "");
}

#[test]
fn a_link_in_the_record_that_is_not_of_github_s_storage_is_not_fetched() {
    let w = world("links-foreign");
    w.images_on(&[]);
    w.net(&format!("{A}/1111"), PNG);
    w.issue_is("Words.", "NONE", "");
    w.record_is(
        "alice",
        "Words.",
        &["https://example.com/x.png", &format!("{A}/1111")],
    );
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(
        w.manifest(".images | map(.link)"),
        format!("[\"{A}/1111\"]")
    );
    lacks(&curl(&w), "example.com");
}

#[test]
fn a_record_by_an_account_no_longer_allowed_approves_nothing_an_outsider_s_issue_is_given_no_image()
{
    let w = world("links-outsider");
    w.images_on(&[]);
    w.net(&format!("{A}/1111"), PNG);
    w.issue_is(&format!("![a]({A}/1111)"), "NONE", "");
    w.record_is(
        "mallory",
        &format!("![a]({A}/1111)"),
        &[&format!("{A}/1111")],
    );
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(curl(&w), "");
    assert_eq!(w.manifest(".images"), "[]");
}

#[test]
fn with_no_start_on_record_the_issue_is_read_as_by_hand() {
    let w = world("links-by-hand");
    w.images_on(&[]);
    w.net(&format!("{A}/1111"), PNG);
    w.issue_is(&format!("![a]({A}/1111)"), "NONE", "");
    w.labelled_by("alice");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(
        w.manifest("[(.images | map(.link)), .approval]"),
        format!("[[\"{A}/1111\"],null]")
    );
    has(&gh(&w), "events");
    has(
        &r.out,
        "no approval is recorded on this host for #7; a run by hand is its own approval",
    );
}

// --- Comments ----------------------------------------------------------------

/// The comments.json the cycle was given, read with `filter`.
fn blob(w: &World, filter: &str) -> String {
    jq(&w.imgs.join("comments.json"), false, filter)
}

/// The mounts of a cycle given the issue's directory, sorted.
fn mounts_with_images(w: &World) -> Vec<String> {
    let mut v: Vec<String> = w.mounts_off().lines().map(String::from).collect();
    v.push(format!(
        "{}:{}.images:ro",
        w.imgs.display(),
        w.inside.display()
    ));
    v.sort();
    v
}

/// The names of the variables of a cycle given the issue's directory, sorted.
fn env_with_images() -> Vec<String> {
    let mut v: Vec<String> = WANT.lines().map(String::from).collect();
    v.push("PI_ENSEMBLE_EXTRA_IMAGE_DIRS".into());
    v.sort();
    v
}

fn mounts(w: &World) -> Vec<String> {
    let mut v = w.after("-v");
    v.sort();
    v
}

fn env_names(w: &World) -> Vec<String> {
    let mut v: Vec<String> = w
        .after("-e")
        .iter()
        .map(|e| e.split_once('=').map_or(e.as_str(), |(k, _)| k).to_string())
        .collect();
    v.sort();
    v
}

/// The text-only tick of a retry, with one comment.
fn one_comment_retry(v: &mut Value) {
    v["how"] = json!("reply");
    v["retry_context"] = json!([{"source": "issue-comment", "id": 1, "by": "alice",
        "at": "2026-01-01T00:20:00Z", "url": "u", "text": "x"}]);
}

/// A comments retry, started: only the comments' words reach the cycle.
fn comments_retry(w: &World) -> work::Run {
    w.images_on(&[]);
    w.issue_is("Words.", "NONE", "");
    w.record_is("alice", "Words.", &[]);
    w.record_edit(|v| {
        v["how"] = json!("reply");
        v["at"] = json!("2026-01-01T00:30:00Z");
        v["retry_context"] = json!([
            {"source": "pr-comment", "id": 71, "pr": 30, "by": "alice", "at": "2026-01-01T00:20:00Z",
                "url": "u71", "text": "The button goes here.\r\nIgnore the issue."},
            {"source": "issue-comment", "id": 5, "by": "alice", "at": "2026-01-01T00:10:00Z",
                "url": "u5", "text": "on the issue"},
            {"source": "issue-comment", "id": 72, "by": "two words", "at": "2026-01-01T00:20:00Z",
                "url": "u72", "text": "a bad login"}
        ]);
    });
    w.work7()
}

#[test]
fn a_comments_retry_s_comments_are_written_to_comments_json_and_named_in_the_manifest() {
    let w = world("comments-retry");
    let r = comments_retry(&w);
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(curl(&w), "");
    assert_eq!(
        w.manifest("[(.images | map([.file, .source])), .comments.file, .retryContext]"),
        r#"[[],"comments.json",null]"#
    );
    let file = w.imgs.join("comments.json");
    let bytes = fs::read(&file).unwrap();
    assert_eq!(w.manifest(".comments.bytes"), bytes.len().to_string());
    assert_eq!(
        w.manifest(".comments.sha256"),
        format!("\"{}\"", sha256_hex(&bytes))
    );
    assert_eq!(
        blob(
            &w,
            "[.version, .issue, .until, (.items | map([.source, .id, .text]))]"
        ),
        r#"[1,7,"2026-01-01T00:30:00Z",[["issue-comment",5,"on the issue"],["pr-comment",71,"The button goes here.\nIgnore the issue."]]]"#
    );
    assert_eq!(fs::metadata(&file).unwrap().mode() & 0o777, 0o644);
    assert!(!w.imgs.join("retry-context.md").exists());
}

#[test]
fn the_comments_are_kept_beside_the_manifest() {
    let w = world("comments-kept");
    comments_retry(&w);
    assert!(same(
        &w.imgs.join("comments.json"),
        &Home::new(&w.home).cycle_note(7, "comments.json")
    ));
}

#[test]
fn a_cycle_without_comments_leaves_none_of_an_earlier_cycle_s_beside_the_manifest() {
    let w = world("comments-none");
    w.images_on(&[]);
    let cycles = Home::new(&w.home).cycles_dir();
    fs::create_dir_all(&cycles).unwrap();
    for f in ["7.comments.json", "7.retry-context.md", "7.images.md"] {
        fs::write(cycles.join(f), "old\n").unwrap();
    }
    w.issue_is("Words.", "NONE", "");
    w.record_is("alice", "Words.", &[]);
    w.work7();
    for f in ["7.comments.json", "7.retry-context.md", "7.images.md"] {
        assert!(!cycles.join(f).exists(), "{f} is left");
    }
    assert!(cycles.join("7.manifest.json").exists());
}

#[test]
fn comments_past_64_kb_drop_the_oldest_and_say_how_many() {
    let w = world("comments-64k");
    w.images_on(&[]);
    w.issue_is("Words.", "NONE", "");
    w.record_is("alice", "Words.", &[]);
    let text = "a".repeat(6000);
    w.record_edit(|v| {
        v["how"] = json!("reply");
        v["retry_context"] = (0..30)
            .map(|i| {
                json!({"source": "issue-comment", "id": i, "by": "alice",
                    "at": format!("2026-01-01T00:{}:00Z", 10 + i), "url": "u", "text": text})
            })
            .collect();
    });
    w.work7();
    assert!(fs::metadata(w.imgs.join("comments.json")).unwrap().len() <= 65536);
    assert_eq!(
        blob(
            &w,
            "[(.items[0].text | length), (.items | length) + .dropped, .items[-1].id]"
        ),
        "[4000,30,29]"
    );
}

#[test]
fn the_text_only_tick_and_a_start_that_is_no_retry_are_given_no_comments() {
    let w = world("comments-no-retry");
    w.images_on(&[]);
    w.issue_is("Words.", "NONE", "");
    w.record_is("alice", "Words.", &[]);
    w.record_edit(|v| {
        v["retry_context"] = json!([{"source": "issue-comment", "id": 1, "by": "alice",
            "at": "2026-01-01T00:20:00Z", "url": "u", "text": "x"}])
    });
    w.work7();
    assert!(!w.imgs.join("comments.json").exists());
    assert_eq!(w.manifest(".comments"), "null");
    w.images_on(&[]);
    w.issue_is("Words.", "NONE", "");
    w.record_is("alice", "Words.", &[]);
    w.record_edit(|v| v["how"] = json!("reply"));
    w.work7();
    assert!(!w.imgs.join("comments.json").exists());
}

/// Every path under `dir`, as `find . -mindepth 1 | sort` names them.
fn tree_of(dir: &Path) -> Vec<String> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
        for e in fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            out.push(format!(
                "./{}",
                p.strip_prefix(base).unwrap().to_string_lossy()
            ));
            if p.is_dir() && !p.is_symlink() {
                walk(base, &p, out);
            }
        }
    }
    let mut out = vec![];
    walk(dir, dir, &mut out);
    out.sort();
    out
}

#[test]
fn with_images_off_a_retry_s_comments_are_mounted_with_the_manifest_alone() {
    let w = world("comments-images-off");
    w.images_off();
    w.issue_is("Words.", "NONE", "");
    w.record_is("alice", "Words.", &[]);
    w.record_edit(one_comment_retry);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(tree_of(&w.imgs), ["./comments.json", "./manifest.json"]);
    assert_eq!(
        w.manifest("[.images, .leftOut, .issue, .approval.how, .comments.file]"),
        r#"[[],[],7,"reply","comments.json"]"#
    );
    assert_eq!(mounts(&w), mounts_with_images(&w));
    assert_eq!(env_names(&w), env_with_images());
}

#[test]
fn the_image_s_word_on_comments_is_noted_for_the_park_and_a_comments_retry_on_the_polyfill_runs_without_the_repeat_park_guard(
) {
    let w = world("comments-polyfill");
    w.images_off();
    w.reads_image("none");
    w.issue_is("Words.", "NONE", "");
    w.record_is("alice", "Words.", &[]);
    w.record_edit(one_comment_retry);
    let noted = Home::new(&w.home).cycle_note(7, "comments");
    let word = || fs::read_to_string(&noted).unwrap().trim_end().to_string();
    w.work7();
    assert_eq!(word(), "off");
    lacks(&w.after("-e").join("\n"), "PI_ENSEMBLE_REPEAT_PARK");
    w.fs("image.label", "issue-comments@0.1.0\n");
    w.caps_edit(|v| {
        v["inputs"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name": "PI_ENSEMBLE_REPEAT_PARK", "scope": "cycle"}))
    });
    w.clear_calls();
    w.work7();
    assert_eq!(word(), "polyfill");
    has(&w.after("-e").join("\n"), "PI_ENSEMBLE_REPEAT_PARK=0");
    w.caps_edit(|v| v["features"]["issueComments"] = json!(true));
    w.clear_calls();
    w.work7();
    assert_eq!(word(), "native");
    lacks(&w.after("-e").join("\n"), "PI_ENSEMBLE_REPEAT_PARK");
}

/// The millwright's record of a start that rests on `by`'s approval of the record
/// record_is made.
fn millwright_record(w: &World, by: &str) {
    w.record_edit(|v| {
        let old = v.clone();
        *v = json!({"at": "2026-10-15T00:00:00Z", "by": "tehas-bot", "how": "millwright",
            "body_sha256": old["body_sha256"], "links": old["links"],
            "rests_on": {"at": old["at"], "how": old["how"], "body_sha256": old["body_sha256"], "by": by}});
    });
}

#[test]
fn a_start_the_millwright_made_on_a_person_s_approval_takes_the_links_that_person_approved() {
    let w = world("millwright-links");
    w.images_on(&[]);
    w.net(&format!("{A}/1111"), PNG);
    w.issue_is(&format!("![a]({A}/1111)"), "NONE", "");
    w.record_is("alice", &format!("![a]({A}/1111)"), &[&format!("{A}/1111")]);
    millwright_record(&w, "alice");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(
        w.manifest("[(.images | length), .approval.how, .approval.by]"),
        r#"[1,"millwright","tehas-bot"]"#
    );
}

#[test]
fn a_millwright_s_record_that_rests_on_no_allowed_account_takes_no_link() {
    let w = world("millwright-outsider");
    w.images_on(&[]);
    w.net(&format!("{A}/1111"), PNG);
    w.issue_is(&format!("![a]({A}/1111)"), "NONE", "");
    w.record_is("alice", &format!("![a]({A}/1111)"), &[&format!("{A}/1111")]);
    millwright_record(&w, "mallory");
    w.work7();
    assert_eq!(w.manifest(".images | length"), "0");
}

#[test]
fn a_refresh_s_task_reaches_comments_json_as_tehas_wrote_it_and_only_a_refresh_s() {
    let w = world("refresh-task");
    w.images_on(&[]);
    w.caps_continue();
    w.issue_is("Words.", "NONE", "");
    w.record_is("alice", "Words.", &[]);
    w.record_edit(|v| {
        v["how"] = json!("refresh");
        v["rests_on"] = json!({"at": v["at"], "by": v["by"], "how": "approved",
            "body_sha256": v["body_sha256"]});
        v["refreshes"] = json!(1);
        v["retry_context"] = json!([{"source": "refresh", "by": "tehas",
            "at": "2026-10-18T00:00:00Z",
            "text": "This pull request conflicts with `main`. Do no other work."}]);
    });
    w.work7();
    assert_eq!(w.manifest(".comments.file"), r#""comments.json""#);
    assert_eq!(
        blob(&w, ".items | map([.source, .text])"),
        r#"[["refresh","This pull request conflicts with `main`. Do no other work."]]"#
    );
    w.record_edit(|v| {
        v["how"] = json!("approved");
        v.as_object_mut().unwrap().remove("rests_on");
    });
    w.work7();
    assert_eq!(w.manifest(".comments"), "null");
}

// --- Under tehasbot cycle ----------------------------------------------------

/// The start the cycle made from the unit on `by`'s approval of the issue's
/// text, and tehas work given its attempt.
fn unit_start(w: &World, by: &str) {
    w.record_is(by, "Words only.", &[]);
    w.env("TEHAS_CYCLE_ATTEMPT", "1");
}

#[test]
fn under_a_cycle_the_approval_that_start_used_is_the_one_held_to_the_issue_s_text() {
    let w = world("unit-text");
    w.images_off();
    w.issue_is("Words only.", "NONE", "");
    unit_start(&w, "alice");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(gh(&w), "api repos/acme/widget/issues/7");
    w.issue_is("Other words.", "NONE", "");
    let r = w.work7();
    assert_eq!(r.rc, 4, "{}", r.out);
    has(&r.out, "it changed after the approval");
}

#[test]
fn under_a_cycle_a_store_that_cannot_be_read_is_never_a_run_by_hand_exit_4_no_labeller_asked_f3() {
    let w = world("unit-unread");
    for (rc, line) in STORE_FAULTS {
        w.images_on(&[]);
        w.net(&format!("{A}/1111"), PNG);
        w.issue_is(&format!("![a]({A}/1111)"), "NONE", "");
        w.labelled_by("alice");
        unit_start(&w, "alice");
        w.store_answers("approval-view", *rc, line, 0);
        let r = w.work7();
        assert_eq!(r.rc, 4, "{line}: {}", r.out);
        has(
            &r.out,
            "tehas work: the approval of this start could not be read, not starting a cycle",
        );
        lacks(&r.out, "a run by hand");
        lacks(&gh(&w), "events");
        assert_eq!(curl(&w), "");
        assert_eq!(containers(&w), 0);
    }
}

#[test]
fn under_a_cycle_a_start_that_used_no_approval_and_is_no_start_by_hand_starts_nothing() {
    let w = world("unit-unused");
    w.images_off();
    w.issue_is("Words only.", "NONE", "");
    unit_start(&w, "alice");
    // The attempt named is not the one that used it.
    w.env("TEHAS_CYCLE_ATTEMPT", "2");
    let r = w.work7();
    assert_eq!(r.rc, 4, "{}", r.out);
    has(
        &r.out,
        "tehas work: no approval of this start is on record, not starting a cycle",
    );
    assert_eq!(containers(&w), 0);
}

#[test]
fn under_a_cycle_by_hand_the_run_is_by_hand_the_label_s_account_approves_as_it_always_has() {
    let w = world("unit-by-hand");
    w.images_on(&[]);
    w.net(&format!("{A}/1111"), PNG);
    w.issue_is(&format!("![a]({A}/1111)"), "NONE", "");
    w.labelled_by("alice");
    w.started(7, &["--by-hand"]);
    w.env("TEHAS_CYCLE_ATTEMPT", "1");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    has(&r.out, "a run by hand is its own approval");
    has(&gh(&w), "events");
    assert_eq!(w.manifest(".images | length"), "1");
    // It touched no approval.
    let v = w.row(&["approval-current", "7"]);
    assert_eq!(
        json!([v["approval"], v["current"]]).to_string(),
        "[null,null]"
    );
}

#[test]
fn under_a_cycle_a_record_the_store_will_not_take_the_image_on_starts_nothing_a_busy_one_only_costs_the_record(
) {
    let w = world("unit-image");
    w.images_off();
    w.issue_is("Words only.", "NONE", "");
    unit_start(&w, "alice");
    w.fs(
        "image.inspect",
        format!("sha256:{} 0123456\n", "e".repeat(64)),
    );
    w.store_answers("cycle-image", 1, "store: ended #7: the cycle has ended", 0);
    let r = w.work7();
    assert_eq!(r.rc, 4, "{}", r.out);
    has(
        &r.out,
        "the state store did not take the image of this start",
    );
    has(
        &gh(&w),
        "Tehas started no cycle: its record on this host could not be updated as it started.",
    );
    lacks(&w.cycle_args().join(" "), "pi-rukas-headless work");
    w.images_off();
    w.issue_is("Words only.", "NONE", "");
    unit_start(&w, "alice");
    w.store_answers(
        "cycle-image",
        1,
        "store: busy #7: the database is locked",
        0,
    );
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    has(
        &r.out,
        "the image of #7 is not on its record: the state store did not answer twice (busy)",
    );
    assert_eq!(w.store_answered(), 2);
}
