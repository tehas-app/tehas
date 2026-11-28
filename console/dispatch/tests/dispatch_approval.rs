//! Who may label an issue, and the approval record a start writes. The pure
//! rules of the same suite are the fixtures of tests/fixtures/approval.json.

mod common;

use common::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use tehas_ports::{Act, Mark};

const A: &str = "https://github.com/user-attachments/assets";
const IMAGES: &[&str] = &["TEHAS_MAX_CYCLES=1", "TEHAS_WORK_IMAGES=1"];

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
    assert_eq!(w.notes_any(), 0);
    has(
        &r.out,
        "WARN: label change on #7 failed: --remove-label tehas:ready",
    );
    w.rm_fx("edit-fails");
    let _ = fs::remove_file(w.t().join("calls/gh"));
    w.poll();
    assert_eq!(w.notes_any(), 1);
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

/// The dispatcher matches the allowed list in any ASCII case, as
/// `console label allowed` does: a case variant of a listed login with the
/// recorded id counts. The ports' adapter folds a login when it names an
/// account, so the log says it as the list spells it; the vouch above it
/// keeps the case GitHub gave.
#[test]
fn a_case_variant_of_a_listed_login_with_the_recorded_user_id_counts() {
    let w = World::new("ap-uid-case");
    ready_by(&w, json!({"login": "Alice", "id": uid("alice")}));
    let r = w.dispatch(&["--dry-run"]);
    lacks(&r.out, "does not count");
    lacks(&r.out, "not an allowed account");
    has(&r.out, "would run a cycle for #7 (labelled by alice)");
}

/// The vouch folds case: a case variant with another id is doubted.
#[test]
fn a_case_variant_of_a_listed_login_with_another_user_id_is_warned_about() {
    let w = World::new("ap-uid-case-other");
    ready_by(&w, json!({"login": "Alice", "id": 99}));
    let r = w.dispatch(&["--dry-run"]);
    has(
        &r.out,
        &format!(
            "tehas: Alice acted with the user id 99, not the recorded {}, so it does not count",
            uid("alice")
        ),
    );
    has(&r.out, "was applied by 'alice#99', not an allowed account");
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

/// A file of before bots were keyed whole keeps the bot under its bare slug,
/// which may be an account of that name: the poll stops with 78 and names
/// the record, in 1.x lines and in realm lines alike.
#[test]
fn a_bot_kept_only_under_its_bare_slug_stops_the_poll_until_the_ids_are_recorded() {
    let w = World::new("ap-uid-bare-bot");
    for text in [
        format!("alice {}\ntehas-bot {}\n", uid("alice"), uid("tehas-bot")),
        format!(
            "code-host alice {}\ncode-host tehas-bot {}\n",
            uid("alice"),
            uid("tehas-bot")
        ),
    ] {
        w.home_file(tehas_core::accounts::FILE, &text);
        let r = w.dispatch(&["--dry-run"]);
        assert_eq!(r.rc, 78, "{text:?}: {}", r.out);
        has(
            &r.out,
            "may be the bot tehas-bot[bot] or another account of that name",
        );
        has(&r.out, "Run console ids record");
        lacks(&r.out, "would run a cycle");
    }
    // Both kept side by side, each by its whole login, the poll runs.
    w.home_file(
        tehas_core::accounts::FILE,
        &format!(
            "code-host alice {}\ncode-host tehas-bot 5\ncode-host tehas-bot[bot] {}\n",
            uid("alice"),
            uid("tehas-bot")
        ),
    );
    let r = w.dispatch(&["--dry-run"]);
    assert_ne!(r.rc, 78, "{}", r.out);
    lacks(&r.out, "Run console ids record");
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
        json!({"at": "2026-01-01T00:00:00Z", "by": "alice", "how": "approved", "body_sha256": sum,
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
fn the_record_names_no_comment_and_none_are_read() {
    let w = World::new("ap-no-comments");
    title_world(&w);
    assert_eq!(w.approval(r#"has("comment_links")"#, 7), "false");
    lacks(&w.gh(), "comments?per_page=100 --paginate");
    lacks(&w.gh(), "comments?per_page=100&page=1");
}

/// An instance that still sets TEHAS_WORK_COMMENTS: alice's two comments
/// before the label, an outsider's, one after the label and one edited after
/// it, and a poll.
fn comments_world(w: &World) -> Run {
    let mut lines = IMAGES.to_vec();
    lines.push("TEHAS_WORK_COMMENTS=1");
    instance(w, &lines);
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
fn an_instance_that_still_sets_comments_records_no_link_of_a_comment() {
    let w = World::new("ap-comments");
    let r = comments_world(&w);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "7 ");
    assert_eq!(
        val(&w.at_mark(r#"[.links, has("comment_links")]"#)),
        json!([[format!("{A}/1111")], false])
    );
    lacks(&w.gh(), "issues/7/comments?per_page=100 --paginate");
    lacks(&w.gh(), "issues/7/comments?per_page=100&page=1");
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
/// images on unless `off`.
fn retry_world(w: &World, off: bool, boxes: &[&str]) {
    retry_world_after(w, off, boxes, vec![]);
}

/// As `retry_world`, with `after` written as the comments the issue got
/// after the question.
fn retry_world_after(w: &World, off: bool, boxes: &[&str], after: Vec<Value>) {
    w.parked("failed", "alice");
    if !off {
        instance(w, IMAGES);
    }
    w.comments(7, &park_body(boxes), "alice", json!(after));
}

/// The comment `who` wrote at `created`, in the shape of the read's nodes.
/// The read numbers it, so a list of them agrees with itself.
fn gcomment(who: &str, created: &str, body: &str) -> Value {
    json!({"author": {"login": who, "databaseId": uid(who)}, "body": body,
        "createdAt": created})
}

#[test]
fn a_retry_ticks_record_takes_the_comments_written_before_the_tick() {
    let w = World::new("ap-retry");
    retry_world_after(
        &w,
        false,
        &["x", " "],
        vec![
            gcomment("alice", "2026-01-01T00:30:00Z", &format!("![r]({A}/9191)")),
            gcomment("alice", "2026-01-01T01:00:01Z", &format!("![s]({A}/9292)")),
        ],
    );
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    // The read numbers the comments: 4002 is alice's before the tick and
    // 4003 the reply that ticked, itself written at the tick and so within
    // the window. The comment of 01:00:01 is after it and stays out.
    assert_eq!(
        w.at_mark(r#"[.how, (.retry_context | map(.id)), has("comment_links")]"#),
        r#"["reply",[4002,4003],false]"#
    );
}

// The box of the issue alone: the start names neither the issue's comments
// nor its pull request's, so neither read is asked for.
#[test]
fn a_retry_tick_of_the_issue_alone_reads_no_comment() {
    let w = World::new("ap-retry-alone");
    retry_world(&w, false, &[" ", "x", " "]);
    issue_comments(&w, 7, &[]);
    w.fx_json(
        "pulls.json",
        &json!([{"number": 30, "headRefName": "feature/issue-7-search", "body": "",
            "author": {"login": "tehas-bot[bot]", "databaseId": BOT}}]),
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
        r#"["alice","reply",4001]"#
    );
    assert_eq!(
        w.at_mark(r#"[has("comment_links"), has("retry_context")]"#),
        "[false,false]"
    );
    lacks(&w.gh(), "pr list");
    lacks(&w.gh(), "issues/7/comments?per_page=100 --paginate");
    // The reads of the ports, too, are not asked for: the list of the
    // bot's changes and the notes of the issue.
    lacks(&w.gh(), "states: [OPEN, CLOSED, MERGED]");
}

/// The code host's id of the instance bot, the viewer of a read of its own
/// changes: the sum of the characters of its login, the id these worlds
/// record for it.
const BOT: u64 = 903;

/// A retry tick, the cycle's pull request #30 commented on
/// by alice, mallory and bob, another issue's #31 and a second of #7's, #32.
fn pr_world(w: &World) -> Run {
    retry_world(w, false, &["x", " "]);
    issue_comments(w, 7, &[]);
    // The bot's changes the retry lists: #30 whose head names the issue,
    // #31 of another issue, and #32 whose body closes this one.
    w.fx_json(
        "pulls.json",
        &json!([{"number": 30, "headRefName": "feature/issue-7-search", "body": "",
            "author": {"login": "tehas-bot[bot]", "databaseId": BOT}},
            {"number": 31, "headRefName": "feature/issue-8-other", "body": "",
            "author": {"login": "tehas-bot[bot]", "databaseId": BOT}},
            // #32 closes the issue with its body. The poll's own list of
            // open pull requests reads it too, so a case of it says what
            // the hold on an open one would have made a person do first:
            // it is closed, and its comment is still the conversation.
            {"number": 32, "headRefName": "x", "body": "Fixes #7", "state": "CLOSED",
            "author": {"login": "tehas-bot[bot]", "databaseId": BOT}}]),
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
    let mut more = IMAGES.to_vec();
    more.push(r#"TEHAS_ALLOWED_LABELERS="alice bob""#);
    instance(w, &more);
    w.poll()
}

#[test]
fn a_retry_tick_takes_what_allowed_accounts_wrote_on_the_cycles_pull_request_up_to_the_tick() {
    let w = World::new("ap-retry-pr");
    let r = pr_world(&w);
    assert_eq!(r.rc, 0, "{}\nGH:\n{}", r.out, w.gh());
    assert_eq!(w.started(), "7 ", "{}", r.out);
    assert_eq!(w.at_mark(r#"has("comment_links")"#), "false");
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
            ["pr-comment", 91, 32, "bob", "second PR"],
            // The reply that ticked the box is a comment of the issue by
            // an allowed account, written at the tick: the read of the
            // notes gives it with the rest, as the issue has it.
            ["issue-comment", 4002, null, "alice", "retry"]
        ])
    );
    has(
        &w.gh(),
        "states: [OPEN, CLOSED, MERGED]",
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
    instance(&w, IMAGES);
    issue_comments(&w, 7, &[]);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    lacks(&w.gh(), "pr list");
    assert_eq!(w.approval(r#"has("retry_context")"#, 7), "false");
}

#[test]
fn with_images_off_a_retry_tick_still_takes_the_comments_text_and_no_link() {
    let w = World::new("ap-retry-off");
    retry_world_after(
        &w,
        true,
        &["x", " "],
        vec![gcomment(
            "alice",
            "2026-01-01T00:30:00Z",
            &format!("said ![r]({A}/9191)"),
        )],
    );
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&w.gh(), "states: [OPEN, CLOSED, MERGED]");
    // alice's comment and the reply that ticked the box, both before or at
    // the tick: the text is taken and no link of a comment is named.
    assert_eq!(
        w.approval(
            r#"[has("comment_links"), (.retry_context | map(.source))]"#,
            7
        ),
        r#"[false,["issue-comment","issue-comment"]]"#
    );
}

#[test]
fn pull_requests_that_cannot_be_listed_start_no_retry_and_change_no_label_exit_1() {
    let w = World::new("ap-pr-fails");
    retry_world(&w, false, &["x", " "]);
    issue_comments(&w, 7, &[]);
    w.touch("pr-fails");
    w.touch("pulls-fails");
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "ERROR: the comments on the pull request of #7 could not be read",
    );
    lacks(&w.gh(), "issue edit");
    assert_eq!(w.marks(None), 0);
    assert_eq!(w.started(), "");
}

#[test]
fn comments_that_cannot_be_read_start_nothing_and_change_no_label_exit_1() {
    let w = World::new("ap-comments-fail");
    retry_world(&w, false, &["x", " "]);
    w.touch("icomments-fails");
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    // The poll reads the issue's comments once, for the tick and the retry
    // alike, so a site failure is said where the read is asked for.
    has(
        &r.out,
        "unreadable comments of #7: the site failed; it may have been sent",
    );
    lacks(&w.gh(), "issue edit");
    assert_eq!(w.marks(None), 0);
    assert_eq!(w.started(), "");
    // The tick stays unasked, so the same poll can be asked again with a
    // pull request of the bot's whose conversation the site answers in no
    // shape a read takes: the retry says so and starts nothing.
    std::fs::remove_file(w.fx.join("icomments-fails")).unwrap();
    w.fx_json(
        "pulls.json",
        &json!([{"number": 30, "headRefName": "feature/issue-7-search", "body": "",
            "author": {"login": "tehas-bot[bot]", "databaseId": BOT}}]),
    );
    w.fx("icomments-30.json", r#"{"not": "a list"}"#);
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "ERROR: the comments on the pull request of #7 could not be read",
    );
    lacks(&w.gh(), "issue edit");
    assert_eq!(w.marks(None), 0);
    assert_eq!(w.started(), "");
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
    assert_eq!(w.marks(None), 0);
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
    assert_eq!(w.marks(None), 0);
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
    assert_eq!(w.marks(Some(8)), 0);
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
fn an_empty_labels_answer_keeps_the_file_empty_and_a_later_label_needs_a_person_to_pin_it() {
    // The first poll read the marks and got the empty answer: the empty
    // list is the answer, so the pins file is kept, empty. A first
    // application of a granting label after that counts for nothing until
    // a person pins it.
    let w = World::new("ap-pins-empty");
    w.fx("labels.json", "[]");
    w.fx("events-7.json", "[]");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    assert_eq!(pins(&w), "");
    unpinned(&w);
    let r = w.poll();
    assert_eq!(w.started(), "");
    has(&r.out, "has no pin in");
    lacks(&r.out, "pinned the label");
    assert_eq!(pins(&w), "");
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
    assert_eq!(w.offs(7, Mark::Act(Act::Approve)), 1, "{gh}");
    assert!(w.said_on(7, "Tehas took"), "{:?}", w.said(7));
    has(
        &gh,
        "It was applied by `mallory`, an account that may not authorise work in this factory",
    );
    assert_eq!(w.notes_any(), 1, "{gh}");
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
    assert_eq!(w.marks(None), 0);
    assert_eq!(w.notes_any(), 0);
}

/// A page whose timeline takes back a mark its labels still show is an
/// answer the board adapter does not trust: GitHub's two lists never
/// disagree, so such an issue reads as unread and the next poll reads it
/// again. Nothing starts and nothing is written meanwhile, which is all
/// the old skip line did for it.
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
    lacks(&r.out, "run a cycle for #7");
    lacks(&r.out, "would take tehas:ready off #7");
    lacks(&r.out, "not an allowed account");
    lacks(&w.gh(), "issue edit 7");
    assert_eq!(w.marks(Some(7)), 0);
    assert_eq!(w.notes(7), 0);
}
