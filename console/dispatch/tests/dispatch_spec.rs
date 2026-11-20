//! The `Spec:` line of an issue body, which the spec-pipe feature reads at a
//! start. The feature is still being built, so a case turns it on with
//! TEHAS_TEST_FEATURES, as a test build may.

mod common;

use common::{count_lines, has, lacks, World};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const LINK: &str = "https://github.com/user-attachments/files/12/spec.md";
const SPEC: &str = "# Faster search\n\nR1: under 100 ms.\n";

/// curl hands out fx/spec.md for any link, or fails with fx/curl.rc after
/// the HTTP status in fx/curl.http, as curl -f prints it on a refused answer.
const CURL: &str = r#"out=""
while [ $# -gt 0 ]; do
  case "$1" in -o) out=$2; shift ;; esac
  shift
done
if [ -f "$T/fx/curl.rc" ]; then
  printf "text/plain\n%s" "$(cat "$T/fx/curl.http" 2>/dev/null || echo 000)"
  exit "$(cat "$T/fx/curl.rc")"
fi
cp "$T/fx/spec.md" "$out"
printf "text/markdown\n200""#;

fn sha(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

fn sum() -> String {
    sha(SPEC)
}

/// The sha256 of no spec: 64 a's.
fn other() -> String {
    "a".repeat(64)
}

fn world() -> World {
    let w = World::new("spec");
    w.fx("spec.md", SPEC);
    w.stub("curl", CURL);
    w
}

/// The body of issue 7, ready and labelled by alice.
fn body(w: &World, text: &str) {
    w.fx_json(
        "issue-7.json",
        &json!({"title": "Faster search", "body": text}),
    );
}

/// The instance switches the feature on and the run may use it.
fn pipe_on(w: &World) {
    w.slots(1, &["TEHAS_SPEC_PIPE=on"]);
    w.env("TEHAS_TEST_FEATURES", "spec-pipe");
}

fn current(w: &World) -> Value {
    w.row(&["approval-current", "7"])
}

#[test]
fn with_the_feature_off_a_body_whose_spec_line_is_malformed_starts_as_before() {
    let w = world();
    body(&w, "Make it fast.\n\nSpec: docs/specs/search.md");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
}

#[test]
fn with_the_feature_off_a_spec_line_fetches_nothing_and_pins_nothing() {
    let w = world();
    body(&w, &format!("Spec: {LINK} sha256:{}", sum()));
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
    assert_eq!(w.calls("curl"), "");
    assert_eq!(current(&w)["current"]["input"]["spec"], Value::Null);
}

#[test]
fn with_the_feature_on_a_spec_of_the_bytes_its_line_names_starts_and_the_approval_keeps_the_pin() {
    let w = world();
    pipe_on(&w);
    body(
        &w,
        &format!("Make it fast.\n\nSpec: {LINK} sha256:{}", sum()),
    );
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
    has(&w.calls("curl"), LINK);
    assert_eq!(
        current(&w)["current"]["input"]["spec"],
        json!({"link": LINK, "sha256": sum()})
    );
    let left = w.sh(r#"find "$TEHAS_HOME/tmp" -name 'spec.*' 2>/dev/null"#);
    assert_eq!(left.out, "");
}

#[test]
fn with_the_feature_on_a_spec_changed_since_its_line_was_written_starts_nothing_and_says_why_once()
{
    let w = world();
    pipe_on(&w);
    body(&w, &format!("Spec: {LINK} sha256:{}", other()));
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    let s = sum();
    has(&r.out, &format!("skip #7: the bytes of its spec are not the ones its `Spec:` line names: they have sha256 {s}"));
    let gh = w.gh();
    has(
        &gh,
        "issue edit 7 --repo acme/widget --remove-label tehas:ready",
    );
    has(&gh, &format!("issue comment 7 --repo acme/widget --body Tehas took `tehas:ready` off this issue and started nothing: the bytes of its spec are not the ones its `Spec:` line names: they have sha256 {s}, so the spec is not the one that was approved. A person attaches the spec"));
    assert_eq!(count_lines(&gh, "issue comment 7 "), 1, "{gh}");
    lacks(&gh, "--add-label tehas:running");
    assert_eq!(current(&w)["current"], Value::Null);
}

#[test]
fn with_the_feature_on_a_spec_link_github_refuses_starts_nothing() {
    let w = world();
    pipe_on(&w);
    w.fx("curl.rc", "22\n");
    w.fx("curl.http", "404\n");
    body(&w, &format!("Spec: {LINK} sha256:{}", sum()));
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    has(
        &r.out,
        "skip #7: GitHub refused the link of its `Spec:` line (HTTP 404)",
    );
    has(
        &w.gh(),
        "issue edit 7 --repo acme/widget --remove-label tehas:ready",
    );
}

#[test]
fn with_the_feature_on_a_spec_link_github_answers_with_a_5xx_changes_nothing_and_ends_the_poll_with_1(
) {
    let w = world();
    pipe_on(&w);
    w.fx("curl.rc", "22\n");
    w.fx("curl.http", "503\n");
    body(&w, &format!("Spec: {LINK} sha256:{}", sum()));
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    assert_eq!(w.started(), "");
    has(&r.out, "ERROR: the spec of #7 could not be fetched (curl exited 22, HTTP 503), not starting a cycle");
    lacks(&w.gh(), "issue edit 7");
    lacks(&w.gh(), "issue comment 7");
}

#[test]
fn with_the_feature_on_a_spec_fetch_that_fails_on_the_way_changes_nothing_and_ends_the_poll_with_1()
{
    let w = world();
    pipe_on(&w);
    w.fx("curl.rc", "7\n");
    body(&w, &format!("Spec: {LINK} sha256:{}", sum()));
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    assert_eq!(w.started(), "");
    has(
        &r.out,
        "ERROR: the spec of #7 could not be fetched (curl exited 7), not starting a cycle",
    );
    lacks(&w.gh(), "issue edit 7");
    lacks(&w.gh(), "issue comment 7");
}

#[test]
fn with_the_feature_on_a_host_start_whose_spec_is_not_the_one_approved_is_not_picked_again() {
    let w = world();
    pipe_on(&w);
    let text = format!("Spec: {LINK} sha256:{}", other());
    body(&w, &text);
    w.t_approve(
        7,
        "host",
        "host",
        1_760_000_000,
        &["--body-sha", &sha(&text)],
    );
    w.fx("work-7.json", r#"{"state":"open"}"#);
    w.ready(&[8]);
    w.fx("issue-8.json", r#"{"title":"Other","body":"No spec."}"#);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    has(
        &r.out,
        "skip #7: the bytes of its spec are not the ones its `Spec:` line names",
    );
    has(&r.out, "the host's approval of #7 of 2025-10-09T08:53:20Z is off the record: run `tehas start 7` again");
    assert_eq!(current(&w)["approval"], Value::Null);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "8 ");
    let fetched = w.calls("curl").lines().filter(|l| l.contains(LINK)).count();
    assert_eq!(fetched, 1, "{}", w.calls("curl"));
    lacks(&r.out, "#7");
}

#[test]
fn with_the_feature_on_a_body_with_no_spec_line_starts() {
    let w = world();
    pipe_on(&w);
    body(&w, "Make it fast.");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
}

#[test]
fn with_the_feature_on_a_malformed_spec_line_starts_nothing_and_says_why_once() {
    let w = world();
    pipe_on(&w);
    body(&w, "Make it fast.\n\nSpec: docs/specs/search.md");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    has(
        &r.out,
        "skip #7: its `Spec:` line is not a link and its sha256",
    );
    let gh = w.gh();
    has(
        &gh,
        "issue edit 7 --repo acme/widget --remove-label tehas:ready",
    );
    has(&gh, "issue comment 7 --repo acme/widget --body Tehas took `tehas:ready` off this issue and started nothing: its `Spec:` line is not a link and its sha256. A person writes the one line `Spec: <link of the attached file> sha256:<64 hex digits of its bytes>`, or takes it out, and applies `tehas:ready` again.");
    assert_eq!(count_lines(&gh, "issue comment 7 "), 1, "{gh}");
}

#[test]
fn with_the_feature_on_a_spec_link_that_is_no_file_attached_on_github_starts_nothing() {
    let w = world();
    pipe_on(&w);
    body(
        &w,
        &format!("Spec: https://example.com/spec.md sha256:{}", sum()),
    );
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    has(
        &r.out,
        "skip #7: the link of its `Spec:` line is not a file attached on GitHub",
    );
    lacks(&w.calls("curl"), "example.com");
}

#[test]
fn with_the_feature_on_two_spec_lines_start_nothing() {
    let w = world();
    pipe_on(&w);
    let line = format!("Spec: {LINK} sha256:{}", sum());
    body(&w, &format!("{line}\n{line}"));
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    has(&r.out, "skip #7: it has more than one `Spec:` line");
}

#[test]
fn the_switch_set_on_without_the_test_variable_is_refused_with_78_before_any_call() {
    let w = world();
    w.slots(1, &["TEHAS_SPEC_PIPE=on"]);
    body(&w, "Make it fast.");
    let r = w.poll();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "TEHAS_SPEC_PIPE is on and spec-pipe is still being built in this release",
    );
    assert_eq!(w.gh(), "");
    assert_eq!(w.started(), "");
}
