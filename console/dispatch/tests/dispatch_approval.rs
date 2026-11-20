//! Who may label an issue, and the approval record a start writes. The pure
//! rules of the same suite are the fixtures of tests/fixtures/approval.json.

mod common;

use common::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

const A: &str = "https://github.com/user-attachments/assets";
const COMMENTS: &[&str] = &[
    "TEHAS_MAX_CYCLES=1",
    "TEHAS_WORK_IMAGES=1",
    "TEHAS_WORK_COMMENTS=1",
];

fn sha(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

fn val(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{e}: {text}"))
}

/// MINIMAL and these lines.
fn instance(w: &World, more: &[&str]) {
    let mut lines = MINIMAL.to_vec();
    lines.extend_from_slice(more);
    w.instance(&lines);
}

fn ready_by(w: &World, actor: Value) {
    w.fx_json(
        "events-7.json",
        &json!([{"event": "labeled", "label": {"name": "tehas:ready"}, "actor": actor}]),
    );
}

fn issue_comments(w: &World, n: u64, list: &[Value]) {
    w.fx_json(&format!("icomments-{n}.json"), &json!(list));
}

fn edit_of(w: &World, body: &str, title: Option<&str>) {
    let mut issue = json!({"lastEditedAt": null, "editor": null, "body": body});
    if let Some(t) = title {
        issue["title"] = json!(t);
    }
    w.fx_json(
        "edit-7.json",
        &json!({"data": {"repository": {"issue": issue}}}),
    );
}

fn approval_current(w: &World) -> Value {
    let v = w.row(&["approval-current", "7"]);
    json!([v["approval"], v["current"]])
}

#[test]
fn while_the_label_cannot_be_taken_off_nothing_is_said_on_the_issue_and_it_is_said_once_in_the_end()
{
    let w = World::new("ap-edit-fails");
    ready_by(&w, json!({"login": "mallory"}));
    w.touch("edit-fails");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(count_lines(&w.gh(), "issue comment "), 0);
    has(
        &r.out,
        "WARN: label change on #7 failed: --remove-label tehas:ready",
    );
    w.rm_fx("edit-fails");
    let _ = fs::remove_file(w.t().join("calls/gh"));
    w.poll();
    assert_eq!(count_lines(&w.gh(), "issue comment "), 1);
}

#[test]
fn a_label_from_a_listed_login_with_the_recorded_user_id_counts() {
    let w = World::new("ap-uid");
    ready_by(&w, json!({"login": "alice", "id": uid("alice")}));
    let r = w.dispatch(&["--dry-run"]);
    has(&r.out, "would run a cycle for #7 (labelled by alice)");
    lacks(&r.out, "does not count");
}

#[test]
fn a_label_from_a_listed_login_with_another_user_id_does_not_count_and_is_warned_about() {
    // The login was renamed and taken by someone else: the label is not alice's.
    let w = World::new("ap-uid-other");
    ready_by(&w, json!({"login": "alice", "id": 99}));
    let r = w.dispatch(&["--dry-run"]);
    has(
        &r.out,
        &format!(
            "tehas: alice acted with the user id 99, not the recorded {}, so it does not count",
            uid("alice")
        ),
    );
    has(&r.out, "tehas update --accept-id alice");
    has(
        &r.out,
        "skip #7: tehas:ready was applied by 'alice#99', not an allowed account",
    );
    lacks(&r.out, "would run a cycle");
}

#[test]
fn a_label_event_whose_account_carries_no_user_id_does_not_count() {
    let w = World::new("ap-uid-none");
    ready_by(&w, json!({"login": "alice", "id": null}));
    let r = w.dispatch(&["--dry-run"]);
    has(
        &r.out,
        "tehas: an action of alice carries no user id, so it does not count",
    );
    has(
        &r.out,
        "skip #7: tehas:ready was applied by 'alice#', not an allowed account",
    );
}

#[test]
fn with_no_user_id_recorded_a_listed_login_authorises_nothing_until_tehas_update_records_one() {
    let w = World::new("ap-uid-unrecorded");
    fs::remove_file(w.home.join("state/account-ids")).unwrap();
    let r = w.dispatch(&["--dry-run"]);
    has(&r.out, "no user id is recorded for alice");
    has(
        &r.out,
        &format!(
            "skip #7: tehas:ready was applied by 'alice#{}', not an allowed account",
            uid("alice")
        ),
    );
    lacks(&r.out, "would run a cycle");
}

#[test]
fn a_labeller_list_with_stray_spaces_is_refused_not_guessed_at() {
    let w = World::new("ap-spaces");
    for odd in ["alice  bob", "alice ", " alice", " "] {
        w.reset();
        let line = format!("TEHAS_ALLOWED_LABELERS='{odd}'");
        instance(&w, &[&line]);
        ready_by(&w, Value::Null);
        let r = w.dispatch(&["--dry-run"]);
        assert_eq!(r.rc, 78, "{odd:?}: {}", r.out);
        lacks(&r.out, "would run a cycle");
        assert_eq!(w.gh(), "", "{odd:?}");
    }
}

/// #7 labelled by alice at 2026-01-01 with a body of two images, the first
/// named twice, and a link that is no attachment, then a poll.
fn record_world(w: &World) -> Run {
    w.fx(
        "events-7.json",
        r#"[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-01T00:00:00Z"}]"#,
    );
    edit_of(
        w,
        &format!(
            "See ![a]({A}/1111)\r\nand ![b]({A}/2222) and again {A}/1111, not https://example.com/c.png"
        ),
        Some("Faster search"),
    );
    w.poll()
}

#[test]
fn a_start_writes_the_approval_record_before_the_label_changes_who_when_how_the_bodys_hash_and_its_links(
) {
    let w = World::new("ap-record");
    let r = record_world(&w);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
    let sum = sha(&format!(
        "See ![a]({A}/1111)\nand ![b]({A}/2222) and again {A}/1111, not https://example.com/c.png"
    ));
    let filter = "{at, by, how, body_sha256, links, title_sha256}";
    assert_eq!(
        val(&w.at_mark(filter)),
        json!({"at": "2026-01-01T00:00:00Z", "by": "alice", "how": "label", "body_sha256": sum,
            "links": [format!("{A}/1111"), format!("{A}/2222")], "title_sha256": sha("Faster search")})
    );
    // The start used the approval it found: the same one, now its readers'.
    assert_eq!(val(&w.approval(filter, 7)), val(&w.at_mark(filter)));
    assert_eq!(w.at_mark(".label"), "tehas:ready");
}

/// Words. under a title with a line end in it, and a poll.
fn title_world(w: &World) {
    edit_of(w, "Words.", Some("Faster\r\nsearch"));
    w.poll();
}

#[test]
fn the_titles_hash_drops_line_ends_as_the_bodys_does() {
    let w = World::new("ap-title");
    title_world(&w);
    assert_eq!(w.approval(".title_sha256", 7), sha("Fastersearch"));
}

#[test]
fn without_the_comments_setting_the_record_names_no_comment_and_none_are_read() {
    let w = World::new("ap-no-comments");
    title_world(&w);
    assert_eq!(w.approval(r#"has("comment_links")"#, 7), "false");
    lacks(&w.gh(), "comments?per_page");
}

/// Comments on: alice's two before the label, an outsider's, one after the
/// label and one edited after it, and a poll.
fn comments_world(w: &World) -> Run {
    instance(w, COMMENTS);
    w.fx(
        "events-7.json",
        r#"[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-01T00:00:00Z"}]"#,
    );
    edit_of(w, &format!("![a]({A}/1111)"), None);
    issue_comments(
        w,
        7,
        &[
            icomment(
                52,
                "alice",
                "2025-12-31T10:00:00Z",
                "2025-12-31T10:00:00Z",
                &format!("later ![c]({A}/5555) and ![a]({A}/1111)"),
            ),
            icomment(
                51,
                "alice",
                "2025-12-31T09:00:00Z",
                "2025-12-31T09:00:00Z",
                &format!("first ![b]({A}/4444)"),
            ),
            icomment(
                53,
                "mallory",
                "2025-12-31T11:00:00Z",
                "2025-12-31T11:00:00Z",
                &format!("an outsider's ![m]({A}/6666)"),
            ),
            icomment(
                54,
                "alice",
                "2026-01-01T00:00:01Z",
                "2026-01-01T00:00:01Z",
                &format!("after the label ![l]({A}/7777)"),
            ),
            icomment(
                55,
                "alice",
                "2025-12-31T12:00:00Z",
                "2026-01-01T00:30:00Z",
                &format!("edited after the label ![e]({A}/8888)"),
            ),
        ],
    );
    w.poll()
}

#[test]
fn with_comments_on_the_record_takes_the_links_of_comments_an_allowed_account_wrote_before_the_label(
) {
    let w = World::new("ap-comments");
    let r = comments_world(&w);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
    assert_eq!(
        val(&w.at_mark("[.links, .comment_links]")),
        json!([[format!("{A}/1111")], [
            {"link": format!("{A}/4444"), "source": "comment:51", "by": "alice"},
            {"link": format!("{A}/5555"), "source": "comment:52", "by": "alice"},
            {"link": format!("{A}/1111"), "source": "comment:52", "by": "alice"}]])
    );
}

#[test]
fn an_outsiders_comment_one_written_after_the_label_and_one_edited_after_it_are_not_in_the_record()
{
    let w = World::new("ap-comments-not");
    comments_world(&w);
    let a = w.approval(".", 7);
    for l in ["6666", "7777", "8888"] {
        lacks(&a, &format!("{A}/{l}"));
    }
}

/// #7 parked on alice's label, its clean-start box ticked by alice, with
/// comments on unless `off`.
fn retry_world(w: &World, off: bool, boxes: &[&str]) {
    w.parked("failed", "alice");
    if !off {
        instance(w, COMMENTS);
    }
    w.comments(7, &park_body(boxes), "alice", json!([]));
}

#[test]
fn a_retry_ticks_record_takes_the_comments_written_before_the_tick() {
    let w = World::new("ap-retry");
    retry_world(&w, false, &["x", " "]);
    issue_comments(
        &w,
        7,
        &[
            icomment(
                61,
                "alice",
                "2026-01-01T00:30:00Z",
                "2026-01-01T00:30:00Z",
                &format!("![r]({A}/9191)"),
            ),
            icomment(
                62,
                "alice",
                "2026-01-01T01:00:01Z",
                "2026-01-01T01:00:01Z",
                &format!("![s]({A}/9292)"),
            ),
        ],
    );
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        w.at_mark("[.how, (.comment_links | map(.source))]"),
        r#"["retry-tick",["comment:61"]]"#
    );
}

// The box of the issue alone, on an instance that reads comments: the start
// names neither the issue's comments nor its pull request's, so neither read
// is asked for.
#[test]
fn a_retry_tick_of_the_issue_alone_reads_no_comment_with_the_comments_setting_on() {
    let w = World::new("ap-retry-alone");
    retry_world(&w, false, &[" ", "x", " "]);
    issue_comments(&w, 7, &[]);
    w.fx(
        "prlist.json",
        r#"[{"number":30,"headRefName":"feature/issue-7-search","body":""}]"#,
    );
    issue_comments(
        &w,
        30,
        &[icomment(
            71,
            "alice",
            "2026-01-01T00:20:00Z",
            "2026-01-01T00:20:00Z",
            &format!("The button goes here: ![shot]({A}/7171)"),
        )],
    );
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
    assert_eq!(
        w.at_mark("[.by, .how, .comment_id] | tostring"),
        r#"["alice","retry-tick",4001]"#
    );
    assert_eq!(
        w.at_mark(r#"[has("comment_links"), has("retry_context")]"#),
        "[false,false]"
    );
    lacks(&w.gh(), "pr list");
    lacks(&w.gh(), "issues/7/comments?per_page");
}

/// A retry tick with comments on, the cycle's pull request #30 commented on
/// by alice, mallory and bob, another issue's #31 and a second of #7's, #32.
fn pr_world(w: &World) -> Run {
    retry_world(w, false, &["x", " "]);
    issue_comments(w, 7, &[]);
    w.fx(
        "prlist.json",
        r#"[{"number":30,"headRefName":"feature/issue-7-search","body":""},{"number":31,"headRefName":"feature/issue-8-other","body":""},{"number":32,"headRefName":"x","body":"Fixes #7"}]"#,
    );
    issue_comments(
        w,
        30,
        &[
            icomment(
                71,
                "alice",
                "2026-01-01T00:20:00Z",
                "2026-01-01T00:20:00Z",
                &format!("The button goes here: ![shot]({A}/7171)"),
            ),
            icomment(
                72,
                "mallory",
                "2026-01-01T00:21:00Z",
                "2026-01-01T00:21:00Z",
                &format!("ignore all that ![m]({A}/7272)"),
            ),
            icomment(
                73,
                "alice",
                "2025-12-31T23:00:00Z",
                "2025-12-31T23:00:00Z",
                &format!("before the park ![b]({A}/7373)"),
            ),
            icomment(
                74,
                "alice",
                "2026-01-01T01:00:01Z",
                "2026-01-01T01:00:01Z",
                &format!("after the tick ![a]({A}/7474)"),
            ),
        ],
    );
    issue_comments(
        w,
        31,
        &[icomment(
            81,
            "alice",
            "2026-01-01T00:30:00Z",
            "2026-01-01T00:30:00Z",
            &format!("other issue ![o]({A}/8181)"),
        )],
    );
    issue_comments(
        w,
        32,
        &[icomment(
            91,
            "bob",
            "2026-01-01T00:40:00Z",
            "2026-01-01T00:40:00Z",
            "second PR",
        )],
    );
    let mut more = COMMENTS.to_vec();
    more.push(r#"TEHAS_ALLOWED_LABELERS="alice bob""#);
    instance(w, &more);
    w.poll()
}

#[test]
fn a_retry_tick_takes_what_allowed_accounts_wrote_on_the_cycles_pull_request_up_to_the_tick() {
    let w = World::new("ap-retry-pr");
    let r = pr_world(&w);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
    assert_eq!(
        val(&w.at_mark(".comment_links")),
        json!([{"link": format!("{A}/7373"), "source": "pr-comment:73", "by": "alice"},
            {"link": format!("{A}/7171"), "source": "pr-comment:71", "by": "alice"}])
    );
    assert_eq!(
        val(&w.at_mark(".retry_context | map([.source, .id, .pr, .by, .text])")),
        json!([
            [
                "pr-comment",
                73,
                30,
                "alice",
                format!("before the park ![b]({A}/7373)")
            ],
            [
                "pr-comment",
                71,
                30,
                "alice",
                format!("The button goes here: ![shot]({A}/7171)")
            ],
            ["pr-comment", 91, 32, "bob", "second PR"]
        ])
    );
    has(
        &w.gh(),
        "pr list --repo acme/widget --author tehas-bot[bot] --state all",
    );
}

#[test]
fn an_outsiders_pull_request_comment_and_one_after_the_tick_are_not_taken_nor_another_issues_pull_request(
) {
    let w = World::new("ap-retry-pr-not");
    pr_world(&w);
    let a = w.approval(".", 7);
    for l in ["7272", "7474", "8181"] {
        lacks(&a, &format!("{A}/{l}"));
    }
    lacks(&a, "ignore all that");
    lacks(&w.gh(), "issues/31/comments");
}

#[test]
fn a_start_by_label_reads_no_pull_request() {
    let w = World::new("ap-label-no-pr");
    instance(&w, COMMENTS);
    issue_comments(&w, 7, &[]);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    lacks(&w.gh(), "pr list");
    assert_eq!(w.approval(r#"has("retry_context")"#, 7), "false");
}

#[test]
fn with_comments_off_a_retry_tick_still_takes_the_comments_text_and_no_link() {
    let w = World::new("ap-retry-off");
    retry_world(&w, true, &["x", " "]);
    issue_comments(
        &w,
        7,
        &[icomment(
            61,
            "alice",
            "2026-01-01T00:30:00Z",
            "2026-01-01T00:30:00Z",
            &format!("said ![r]({A}/9191)"),
        )],
    );
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&w.gh(), "pr list");
    assert_eq!(
        w.approval(
            r#"[has("comment_links"), (.retry_context | map(.source))]"#,
            7
        ),
        r#"[false,["issue-comment"]]"#
    );
}

#[test]
fn pull_requests_that_cannot_be_listed_start_no_retry_and_change_no_label_exit_1() {
    let w = World::new("ap-pr-fails");
    retry_world(&w, false, &["x", " "]);
    issue_comments(&w, 7, &[]);
    w.touch("pr-fails");
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "ERROR: the comments on the pull request of #7 could not be read",
    );
    lacks(&w.gh(), "issue edit");
    assert_eq!(w.started(), "");
}

#[test]
fn comments_that_cannot_be_read_start_nothing_and_change_no_label_exit_1() {
    let w = World::new("ap-comments-fail");
    instance(&w, COMMENTS);
    w.touch("icomments-fails");
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "ERROR: the comments of #7 could not be read");
    has(
        &r.out,
        "ERROR: could not write the approval record of #7, not starting a cycle",
    );
    lacks(&w.gh(), "issue edit");
    assert_eq!(w.started(), "");
    w.fx("icomments-7.json", r#"{"not": "a list"}"#);
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    lacks(&w.gh(), "issue edit");
}

#[test]
fn an_issue_with_no_body_is_approved_with_the_hash_of_nothing_and_no_links() {
    let w = World::new("ap-no-body");
    w.poll();
    assert_eq!(
        w.approval("[.body_sha256, .links]", 7),
        format!(r#"["{}",[]]"#, sha(""))
    );
}

#[test]
fn a_dry_run_writes_no_record() {
    let w = World::new("ap-dry");
    w.dispatch(&["--dry-run"]);
    assert_eq!(approval_current(&w), json!([null, null]));
}

#[test]
fn an_approval_the_store_does_not_take_starts_nothing_and_changes_no_label_exit_1() {
    let w = World::new("ap-store-busy");
    w.store_answers(
        "approval-record",
        1,
        "store: busy #7: the database is locked",
        0,
    );
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "ERROR: could not write the approval record of #7 (busy), not starting a cycle",
    );
    lacks(&w.gh(), "issue edit");
    assert_eq!(w.started(), "");
}

#[test]
fn a_store_that_cannot_be_used_stops_the_poll_at_78_before_the_first_start_changes_a_label() {
    let w = World::new("ap-store-corrupt");
    w.store_refuses(
        "approval-record 7",
        78,
        "store: corrupt: the file is not a database",
    );
    let r = w.poll();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "ERROR: the state store cannot be used, not starting a cycle for #7",
    );
    lacks(&w.gh(), "issue edit");
    assert_eq!(w.started(), "");
}

#[test]
fn after_a_start_a_store_that_cannot_be_used_ends_the_poll_with_exit_1_and_starts_no_more() {
    let w = World::new("ap-store-corrupt-2");
    w.slots(2, &[]);
    w.ready(&[3, 8]);
    w.store_refuses(
        "approval-record 8",
        78,
        "store: corrupt: the file is not a database",
    );
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    assert_eq!(w.started(), "3 ");
    has(
        &r.out,
        "ERROR: the state store cannot be used, not starting a cycle for #8",
    );
    lacks(&w.gh(), "issue edit 8");
}

// The seam between the two commands. A cycle the poll started can find its
// issue closed when it asks, and refuse it. It claimed its place before it
// asked, so the issue keeps only tehas:running, which the next poll clears in
// silence.
#[test]
fn a_cycle_that_refused_its_number_leaves_nothing_a_person_has_to_clean_up() {
    let w = World::new("ap-refused");
    let r = w.poll();
    has(&r.out, "start #7 (labelled by alice)");
    lacks(&r.out, "did not start");
    lacks(&w.gh(), "could not start the cycle");
    w.fx("marked.json", r#"[{"number":7,"state":"CLOSED"}]"#);
    w.fx("work-7.json", r#"{"number":7,"state":"closed"}"#);
    w.fx("ready.json", "[]");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "clear #7: it is closed, marked tehas:running, and no cycle holds its lock",
    );
    lacks(&r.out, "did not start");
    lacks(&w.gh(), "needs-human-attention");
    lacks(&w.gh(), "To retry, apply");
}

// --- Label identity ----------------------------------------------------------
// A grant by label counts only for the label pinned in state/labels: with no
// file the poll pins the granting tehas: labels it finds, after that it pins
// nothing.

fn pins(w: &World) -> String {
    w.read_home("state/labels")
}

fn pin(w: &World, text: &str) {
    w.home_file("state/labels", text);
}

fn labelled_as(w: &World, login: &str, id: Option<&str>) {
    let mut label = json!({"name": "tehas:ready"});
    if let Some(id) = id {
        label["id"] = json!(id);
    }
    w.fx_json(
        "events-7.json",
        &json!([{"event": "labeled", "label": label, "actor": {"login": login}}]),
    );
}

fn unpinned(w: &World) {
    w.fx(
        "labels.json",
        r#"[{"id":"LA_ready","name":"tehas:ready"},{"id":"LA_run","name":"tehas:running"}]"#,
    );
    labelled_as(w, "alice", Some("LA_ready"));
}

#[test]
fn with_no_pins_a_dry_run_says_what_it_would_pin_and_writes_nothing() {
    let w = World::new("ap-pins-dry");
    unpinned(&w);
    let r = w.dispatch(&["--dry-run"]);
    has(
        &r.out,
        "would pin the label tehas:ready as LA_ready in state/labels",
    );
    has(&r.out, "would run a cycle for #7 (labelled by alice)");
    assert!(!w.home.join("state/labels").exists());
}

#[test]
fn with_no_pins_the_poll_pins_the_label_on_first_sight_and_the_real_label_starts_the_cycle() {
    let w = World::new("ap-pins-first");
    unpinned(&w);
    w.dispatch(&["--dry-run"]);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
    has(
        &r.out,
        "pinned the label tehas:ready as LA_ready in state/labels",
    );
    assert_eq!(pins(&w).trim_end(), "tehas:ready\tLA_ready");
}

#[test]
fn once_the_file_is_kept_a_label_missing_when_it_was_written_is_not_pinned_when_one_appears() {
    let w = World::new("ap-pins-kept");
    pin(&w, "tehas:plan\tLA_plan\n");
    w.fx("labels.json", r#"[{"id":"LA_bug","name":"tehas:ready"}]"#);
    labelled_as(&w, "alice", Some("LA_bug"));
    let r = w.poll();
    assert_eq!(w.started(), "");
    has(
        &r.out,
        "refused: the label tehas:ready (LA_bug) has no pin in state/labels",
    );
    lacks(&r.out, "pinned the label");
    assert_eq!(pins(&w).trim_end(), "tehas:plan\tLA_plan");
}

#[test]
fn the_pinned_label_grants_as_before_and_the_pins_stay_as_they_are() {
    let w = World::new("ap-pins-grant");
    pin(&w, "tehas:ready\tLA_ready\n");
    w.fx("labels.json", r#"[{"id":"LA_ready","name":"tehas:ready"}]"#);
    labelled_as(&w, "alice", Some("LA_ready"));
    let r = w.poll();
    assert_eq!(w.started(), "7 ");
    lacks(&r.out, "pinned the label");
    lacks(&r.out, "refused");
}

#[test]
fn a_persons_other_label_renamed_to_tehas_ready_grants_nothing_and_both_ids_are_logged() {
    let w = World::new("ap-pins-other");
    pin(&w, "tehas:ready\tLA_ready\n");
    w.fx("labels.json", r#"[{"id":"LA_ready","name":"tehas:ready"}]"#);
    labelled_as(&w, "alice", Some("LA_bug"));
    let r = w.poll();
    assert_eq!(w.started(), "");
    has(
        &r.out,
        "refused: an application of tehas:ready on #7 is of another label than the one pinned in state/labels (pinned LA_ready, event LA_bug)",
    );
    lacks(&r.out, "labelled by alice");
    assert_eq!(pins(&w).trim_end(), "tehas:ready\tLA_ready");
}

#[test]
fn a_pinned_label_the_polls_read_did_not_name_grants_nothing_that_poll() {
    let w = World::new("ap-pins-unread");
    pin(&w, "tehas:ready\tLA_ready\n");
    w.fx("labels.json", "[]");
    labelled_as(&w, "alice", None);
    let r = w.poll();
    assert_eq!(w.started(), "");
    has(
        &r.out,
        "refused: the pinned label tehas:ready was not among the labels this poll read",
    );
}

#[test]
fn the_label_now_called_tehas_ready_is_not_the_pinned_one_no_application_of_it_counts() {
    let w = World::new("ap-pins-renamed");
    pin(&w, "tehas:ready\tLA_ready\n");
    w.fx("labels.json", r#"[{"id":"LA_bug","name":"tehas:ready"}]"#);
    labelled_as(&w, "alice", None);
    let r = w.poll();
    assert_eq!(w.started(), "");
    has(
        &r.out,
        "refused: the label tehas:ready is not the one pinned in state/labels (pinned LA_ready, now LA_bug)",
    );
    assert_eq!(pins(&w).trim_end(), "tehas:ready\tLA_ready");
}

/// The list names a GitHub account called unknown, whose id is recorded.
fn unknown_listed(w: &World) {
    let ids = w.read_home("state/account-ids");
    w.home_file(
        "state/account-ids",
        &format!("{ids}unknown {}\n", uid("unknown")),
    );
    instance(
        w,
        &[
            "TEHAS_MAX_CYCLES=1",
            r#"TEHAS_ALLOWED_LABELERS="alice unknown""#,
        ],
    );
    pin(w, "tehas:ready\tLA_ready\n");
    w.fx("labels.json", r#"[{"id":"LA_ready","name":"tehas:ready"}]"#);
}

// An application the pins refuse is logged as unknown's but is nobody's.
#[test]
fn with_the_account_unknown_on_the_list_a_refused_application_grants_nothing() {
    let w = World::new("ap-unknown-refused");
    unknown_listed(&w);
    labelled_as(&w, "mallory", Some("LA_bug"));
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    has(
        &r.out,
        "refused: an application of tehas:ready on #7 is of another label than the one pinned in state/labels (pinned LA_ready, event LA_bug)",
    );
    has(
        &r.out,
        "skip #7: tehas:ready was applied by 'unknown', not an allowed account",
    );
}

#[test]
fn the_account_unknown_on_the_list_grants_with_the_pinned_label() {
    let w = World::new("ap-unknown-grants");
    unknown_listed(&w);
    labelled_as(&w, "unknown", Some("LA_ready"));
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
    lacks(&r.out, "refused");
}

fn count_exact(text: &str, line: &str) -> usize {
    text.lines().filter(|l| *l == line).count()
}

#[test]
fn a_tehas_ready_applied_by_mallory_is_taken_off_the_issue_and_said_once() {
    let w = World::new("ap-outsider");
    ready_by(&w, json!({"login": "mallory"}));
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    has(
        &r.out,
        "skip #7: tehas:ready was applied by 'mallory', not an allowed account",
    );
    has(
        &r.out,
        "clear #7: tehas:ready off, applied by 'mallory' who may not authorise work here",
    );
    let gh = w.gh();
    assert_eq!(
        count_exact(
            &gh,
            "issue edit 7 --repo acme/widget --remove-label tehas:ready"
        ),
        1,
        "{gh}"
    );
    has(&gh, "issue comment 7 --repo acme/widget --body Tehas took");
    has(
        &gh,
        "It was applied by `mallory`, an account that may not authorise work in this factory",
    );
    assert_eq!(count_lines(&gh, "issue comment "), 1, "{gh}");
}

#[test]
fn a_dry_run_says_the_label_would_come_off_and_changes_nothing() {
    let w = World::new("ap-outsider-dry");
    ready_by(&w, json!({"login": "mallory"}));
    let r = w.dispatch(&["--dry-run"]);
    has(
        &r.out,
        "would take tehas:ready off #7: it was applied by 'mallory'",
    );
    lacks(&w.gh(), "issue edit");
    lacks(&w.gh(), "issue comment");
}

#[test]
fn an_approval_taken_off_after_it_was_given_starts_nothing_while_the_labels_still_list_it() {
    let w = World::new("ap-taken-off");
    w.fx_json(
        "events-7.json",
        &json!([
            {"event": "labeled", "label": {"name": "tehas:ready"}, "actor": {"login": "alice"},
                "created_at": "2026-10-01T00:00:00Z"},
            {"event": "unlabeled", "label": {"name": "tehas:ready"}, "actor": {"login": "alice"},
                "created_at": "2026-10-01T00:01:00Z"}
        ]),
    );
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    has(
        &r.out,
        "skip #7: tehas:ready was taken off by 'alice' since it was applied",
    );
    lacks(&r.out, "not an allowed account");
    lacks(&w.gh(), "issue edit 7");
    lacks(&w.gh(), "issue comment 7");
}
