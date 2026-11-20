//! Issues that link images, what each retry box carries, and how holds are
//! logged, where the poll's process says more than its decision: whether the
//! model is asked, what it is sent, the record of a start and what the poll
//! does with a file or a probe that misbehaves.

mod common;

use common::*;
use serde_json::{json, Value};
use std::time::Instant;

/// The storage the images of these cases link.
const A: &str = "https://github.com/user-attachments/assets";
const MODELS: &str = r#"{"data":[{"id":"test-model"}]}"#;

fn world(name: &str) -> World {
    let w = World::new(name);
    w.mesh_curl();
    w
}

/// instance.env: the minimal instance and more lines.
fn inst(w: &World, more: &[&str]) {
    let mut lines = MINIMAL.to_vec();
    lines.extend_from_slice(more);
    w.instance(&lines);
}

/// The model as models.json declares it, with the input it takes.
fn declares(w: &World, input: Value) {
    let v = json!({"providers": {"test-provider": {"baseUrl": "http://mesh.test/v1",
        "models": [{"id": "other", "input": ["text", "image"]}, {"id": "test-model", "input": input}]}}});
    w.fs("home/.pi/agent/models.json", &v.to_string());
}

/// An instance that gives a cycle images, and a model that offers what
/// models.json declares for it.
fn imaging(w: &World, input: Value) {
    w.reset();
    inst(w, &["TEHAS_WORK_IMAGES=1"]);
    w.mesh(Some(MODELS));
    declares(w, input);
    w.rm_fx("reads-images");
}

/// An instance that gives a cycle nothing of what an issue attaches.
fn offline(w: &World) {
    w.reset();
    inst(w, &["TEHAS_WORK_IMAGES=0", "TEHAS_WORK_FILES=0"]);
    w.mesh(Some(MODELS));
}

/// An issue whose text links an attachment in GitHub's storage: the answer
/// of the list the poll reads and the text it approves.
fn pictured(w: &World, n: u64) {
    let body = format!("It looks like this:\n\n![shot]({A}/0f3e-41)");
    w.fx_json(
        &format!("work-{n}.json"),
        &json!({"number": n, "state": "open", "body": body}),
    );
    w.fx_json(
        &format!("edit-{n}.json"),
        &json!({"data": {"repository": {"issue": {"lastEditedAt": null, "editor": null, "body": body}}}}),
    );
}

/// An issue whose text says a file goes there and links no attachment.
fn worded(w: &World, n: u64) {
    w.fx_json(
        &format!("work-{n}.json"),
        &json!({"number": n, "state": "open",
            "body": "The screenshot is the place; this text deliberately does not say it. SCREENSHOT GOES HERE."}),
    );
}

fn image_asks(w: &World) -> usize {
    w.calls("curl")
        .lines()
        .filter(|l| l.contains("chat/completions"))
        .count()
}

fn why(w: &World) -> Value {
    w.dispatch_state()["why"].clone()
}

fn started(w: &World) -> String {
    w.calls("fake-start").trim_end().to_string()
}

/// A park comment's body, as the shell's command substitution gave it.
fn park(boxes: &[&str]) -> String {
    park_body(boxes).trim_end().to_string()
}

/// The commit and the answer the sandbox image's label gives about its
/// pi-rukas; None for no label.
fn image_says(w: &World, reads: Option<&str>) {
    w.rm_fx("image.label");
    if let Some(r) = reads {
        w.fx("image.label", &format!("{}|{r}\n", "c".repeat(40)));
    }
}

/// An instance that reads comments, with a model that reads the images it
/// is given.
fn commented(w: &World) {
    imaging(w, json!(["text", "image"]));
    w.touch("reads-images");
}

/// A parked issue with a pull request of the bot's: one comment an allowed
/// account wrote on it after the park, an issue whose body links an image,
/// and the instance that reads comments and takes images. `ticked` is
/// clean, alone or old (the two boxes a cycle posted before).
fn reviewed(w: &World, ticked: &str, login: &str) {
    w.fx(
        "prlist.json",
        r#"[{"number":30,"headRefName":"feature/issue-7-search","body":""}]"#,
    );
    w.fx_json(
        "icomments-30.json",
        &json!([icomment(
            71,
            login,
            "2026-01-01T00:20:00Z",
            "2026-01-01T00:20:00Z",
            &format!("The button goes here: ![shot]({A}/7171)")
        )]),
    );
    w.fx("icomments-7.json", "[]");
    let body = match ticked {
        "clean" => park(&["x", " ", " "]),
        "alone" => park(&[" ", "x", " "]),
        _ => park(&["x", " "]),
    };
    w.comments(7, &body, "alice", json!([]));
    pictured(w, 7);
    inst(w, &["TEHAS_WORK_IMAGES=1", "TEHAS_WORK_COMMENTS=1"]);
}

fn mark(w: &World, filter: &str) -> Value {
    serde_json::from_str(&w.at_mark(filter)).unwrap()
}

/// The first bytes base64 text stands for.
fn unbase64(text: &str) -> Vec<u8> {
    const ABC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let sextets: Vec<u32> = text
        .bytes()
        .take_while(|&b| b != b'=')
        .map(|b| ABC.iter().position(|&c| c == b).unwrap() as u32)
        .collect();
    sextets
        .chunks(4)
        .flat_map(|c| {
            let n = c
                .iter()
                .enumerate()
                .fold(0, |n, (i, s)| n | s << (18 - 6 * i));
            let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
            bytes[..c.len().saturating_sub(1)].to_vec()
        })
        .collect()
}

// The whole poll, the read layer's image probe included, never names the
// endpoint it asked.
#[test]
fn an_issue_that_links_an_image_waits_while_the_model_does_not_answer_an_image() {
    let w = world("images-unanswered");
    imaging(&w, json!(["text", "image"]));
    pictured(&w, 7);
    let r = w.poll();
    assert_eq!(r.rc, 0);
    assert_eq!(started(&w), "");
    lacks(&w.gh(), "issue edit");
    has(
        &r.out,
        "hold #7: its text links an attachment and TEHAS_WORK_IMAGES is 1, but test-model does not answer an image",
    );
    lacks(&r.out, "mesh.test");
}

#[test]
fn without_images_for_a_cycle_the_model_is_not_asked_to_read_one() {
    let w = world("images-off");
    w.mesh(Some(MODELS));
    w.poll();
    assert_eq!(started(&w), "7");
    lacks(&w.calls("curl"), "chat/completions");
}

#[test]
fn with_images_for_a_cycle_an_issue_that_links_one_starts_when_the_model_declares_image_input_and_answers_one(
) {
    let w = world("images-on");
    imaging(&w, json!(["text", "image"]));
    pictured(&w, 7);
    w.touch("reads-images");
    w.poll();
    assert_eq!(started(&w), "7");
    assert_eq!(why(&w), json!(""));
    let body: Value =
        serde_json::from_str(&std::fs::read_to_string(w.t().join("calls/curl-body")).unwrap())
            .unwrap();
    let types: Vec<&Value> = body["messages"][0]["content"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| &c["type"])
        .collect();
    assert_eq!(
        json!([body["model"], body["max_tokens"], types]),
        json!(["test-model", 1, ["text", "image_url"]])
    );
    let url = body["messages"][0]["content"][1]["image_url"]["url"]
        .as_str()
        .unwrap();
    let data = url.split(',').nth(1).unwrap();
    assert_eq!(
        unbase64(data)[..8],
        [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]
    );
}

#[test]
fn an_issue_that_links_no_image_starts_while_the_model_cannot_read_one_and_the_model_is_not_asked()
{
    let w = world("images-none");
    for input in [json!(["text"]), json!(["text", "image"])] {
        imaging(&w, input);
        w.poll();
        assert_eq!(started(&w), "7");
        assert_eq!(image_asks(&w), 0);
        assert_eq!(why(&w), json!(""));
        lacks(&w.gh(), "images are off");
    }
}

#[test]
fn an_issue_that_links_an_image_waits_while_the_model_does_not_declare_image_input_and_it_is_not_asked(
) {
    let w = world("images-undeclared");
    for input in [json!(["text"]), Value::Null] {
        imaging(&w, input);
        pictured(&w, 7);
        w.touch("reads-images");
        let r = w.poll();
        assert_eq!(r.rc, 0);
        assert_eq!(started(&w), "");
        lacks(&w.gh(), "issue edit");
        has(&r.out, "hold #7: its text links an attachment and TEHAS_WORK_IMAGES is 1, but test-model does not declare image input in models.json");
        assert_eq!(image_asks(&w), 0);
        assert_eq!(why(&w), json!(""));
    }
}

#[test]
fn the_model_is_asked_to_read_an_image_once_a_poll() {
    let w = world("images-once");
    imaging(&w, json!(["text", "image"]));
    w.slots(3, &["TEHAS_WORK_IMAGES=1"]);
    w.ready(&[7, 8, 9]);
    pictured(&w, 7);
    pictured(&w, 8);
    let r = w.poll();
    assert_eq!(image_asks(&w), 1);
    assert_eq!(started(&w), "9");
    has(&r.out, "hold #8:");
}

#[test]
fn with_every_slot_taken_no_image_request_is_sent() {
    let w = world("images-full");
    imaging(&w, json!(["text", "image"]));
    w.slots(1, &["TEHAS_WORK_IMAGES=1"]);
    pictured(&w, 7);
    w.touch("reads-images");
    w.hold(9);
    w.poll();
    assert_eq!(image_asks(&w), 0);
    assert_eq!(started(&w), "");
    assert_eq!(why(&w), json!("count"));
}

#[test]
fn a_retry_of_an_issue_that_links_an_image_waits_for_a_model_that_reads_one_and_the_tick_stays() {
    let w = world("images-retry");
    w.parked("failed", "alice");
    inst(&w, &["TEHAS_WORK_IMAGES=1"]);
    w.mesh(Some(MODELS));
    declares(&w, json!(["text"]));
    w.comments(7, &park(&["x", " "]), "alice", json!([]));
    pictured(&w, 7);
    let r = w.poll();
    assert_eq!(r.rc, 0);
    assert_eq!(started(&w), "");
    lacks(&w.gh(), "issue edit");
    lacks(&w.gh(), "api -X PATCH");
    has(&r.out, "hold #7: its text links an attachment");
}

#[test]
fn a_planning_session_is_not_held_by_the_image_part_of_the_check() {
    let w = world("images-plan");
    imaging(&w, json!(["text"]));
    w.fx("ready.json", "[]");
    w.fx(
        "plans.json",
        r#"[{"number":7,"labels":[{"name":"tehas:plan"}]}]"#,
    );
    w.fx(
        "events-7.json",
        r#"[{"event":"labeled","label":{"name":"tehas:plan"},"actor":{"login":"alice"}}]"#,
    );
    pictured(&w, 7);
    w.poll();
    assert_eq!(w.plan_calls().lines().count(), 1);
    assert_eq!(image_asks(&w), 0);
}

#[test]
fn the_hold_needs_neither_images_for_a_cycle_nor_a_model_that_reads_one() {
    let w = world("images-offline");
    offline(&w);
    worded(&w, 7);
    let r = w.poll();
    assert_eq!(started(&w), "");
    lacks(&w.gh(), "issue edit");
    assert_eq!(image_asks(&w), 0);
    has(
        &r.out,
        "hold #7: its text says SCREENSHOT GOES HERE and links no attachment",
    );
}

#[test]
fn a_retry_held_on_its_image_holds_that_issue_not_the_poll_the_next_ready_issue_starts() {
    let w = world("images-retry-next");
    w.parked("failed", "alice");
    w.slots(3, &["TEHAS_WORK_IMAGES=1"]);
    w.mesh(Some(MODELS));
    declares(&w, json!(["text"]));
    w.comments(7, &park(&["x", " "]), "alice", json!([]));
    pictured(&w, 7);
    w.ready(&[8]);
    let r = w.poll();
    assert_eq!(r.rc, 0);
    assert_eq!(started(&w), "8");
    has(&r.out, "hold #7: its text links an attachment");
}

// The comments of the issue itself are the park's question, so the REST list
// the poll reads for a clean start is named and empty: only the pull
// request's comments go into the record.
#[test]
fn a_start_by_the_clean_start_box_carries_the_comment_an_allowed_account_wrote_after_the_park() {
    let w = world("images-clean");
    commented(&w);
    w.parked("failed", "alice");
    reviewed(&w, "clean", "alice");
    assert_eq!(w.poll().rc, 0);
    assert_eq!(started(&w), "7");
    has(
        &w.gh(),
        "pr list --repo acme/widget --author tehas-bot[bot] --state all",
    );
    assert_eq!(
        mark(&w, ".comment_links"),
        json!([{"link": format!("{A}/7171"), "source": "pr-comment:71", "by": "alice"}])
    );
    assert_eq!(w.at_mark(".retry_context | length"), "1");
    has(&w.at_mark(".retry_context[0].text"), "The button goes here");
}

#[test]
fn a_comment_by_a_listed_login_with_another_user_id_is_not_carried() {
    let w = world("images-uid");
    commented(&w);
    w.parked("failed", "alice");
    reviewed(&w, "clean", "alice");
    let mut list: Value =
        serde_json::from_str(&std::fs::read_to_string(w.fx.join("icomments-30.json")).unwrap())
            .unwrap();
    list[0]["user"]["id"] = json!(99);
    w.fx_json("icomments-30.json", &list);
    let r = w.poll();
    assert_eq!(r.rc, 0);
    assert_eq!(started(&w), "7");
    has(
        &r.out,
        &format!(
            "tehas: alice acted with the user id 99, not the recorded {}, so it does not count",
            uid("alice")
        ),
    );
    assert_eq!(w.at_mark(".comment_links | length"), "0");
    assert_eq!(w.at_mark(".retry_context | length"), "0");
}

#[test]
fn a_start_by_the_issue_alone_box_names_no_comment_link_and_no_retry_context() {
    let w = world("images-alone");
    commented(&w);
    w.parked("failed", "alice");
    reviewed(&w, "alone", "alice");
    assert_eq!(w.poll().rc, 0);
    assert_eq!(started(&w), "7");
    assert_eq!(w.at_mark(".how"), "retry-tick");
    lacks(&w.gh(), "pr list");
    assert_eq!(
        w.at_mark(r#"[has("comment_links"), has("retry_context")]"#),
        "[false,false]"
    );
}

#[test]
fn both_retries_give_the_cycle_the_image_the_issues_own_body_links() {
    let w = world("images-both");
    for ticked in ["clean", "alone"] {
        commented(&w);
        w.parked("failed", "alice");
        reviewed(&w, ticked, "alice");
        w.poll();
        assert_eq!(started(&w), "7", "{ticked}");
        assert_eq!(
            mark(&w, ".links"),
            json!([format!("{A}/0f3e-41")]),
            "{ticked}"
        );
    }
}

#[test]
fn an_issue_alone_tick_whose_pull_request_cannot_be_listed_still_starts_nothing_of_it_is_read() {
    let w = world("images-alone-pr");
    commented(&w);
    w.parked("failed", "alice");
    reviewed(&w, "alone", "alice");
    w.touch("pr-fails");
    let r = w.poll();
    assert_eq!(r.rc, 0);
    assert_eq!(started(&w), "7");
    lacks(&r.out, "ERROR");
}

#[test]
fn a_clean_start_tick_whose_pull_request_cannot_be_listed_starts_nothing_as_it_did() {
    let w = world("images-clean-pr");
    commented(&w);
    w.parked("failed", "alice");
    reviewed(&w, "clean", "alice");
    w.touch("pr-fails");
    let r = w.poll();
    assert_eq!(r.rc, 1);
    assert_eq!(started(&w), "");
    has(
        &r.out,
        "ERROR: the comments on the pull request of #7 could not be read",
    );
}

#[test]
fn a_park_of_the_old_two_boxes_still_gives_the_clean_start_its_pull_requests_comments() {
    let w = world("images-old");
    commented(&w);
    w.parked("failed", "alice");
    reviewed(&w, "old", "alice");
    assert_eq!(w.poll().rc, 0);
    assert_eq!(started(&w), "7");
    has(
        &w.gh(),
        "pr list --repo acme/widget --author tehas-bot[bot] --state all",
    );
    assert_eq!(
        mark(&w, ".comment_links"),
        json!([{"link": format!("{A}/7171"), "source": "pr-comment:71", "by": "alice"}])
    );
    assert_eq!(w.at_mark(".retry_context | length"), "1");
}

#[test]
fn under_the_issue_alone_box_a_start_reads_neither_the_issues_comments_nor_the_pull_request() {
    let w = world("images-alone-off");
    commented(&w);
    w.parked("failed", "alice");
    reviewed(&w, "alone", "alice");
    inst(&w, &["TEHAS_WORK_IMAGES=1", "TEHAS_WORK_COMMENTS=0"]);
    assert_eq!(w.poll().rc, 0);
    assert_eq!(started(&w), "7");
    lacks(&w.gh(), "pr list");
    assert_eq!(
        w.at_mark(r#"[has("comment_links"), has("retry_context")]"#),
        "[false,false]"
    );
}

// The hold comes before either start, so it asks for no pull request and
// changes no label.
#[test]
fn an_issue_that_links_an_attachment_waits_for_a_model_that_reads_one_under_either_retry_box() {
    let w = world("images-either");
    for ticked in ["clean", "alone"] {
        w.parked("failed", "alice");
        inst(&w, &["TEHAS_WORK_IMAGES=1"]);
        w.mesh(Some(MODELS));
        declares(&w, json!(["text"]));
        let body = if ticked == "clean" {
            park(&["x", " ", " "])
        } else {
            park(&[" ", "x", " "])
        };
        w.comments(7, &body, "alice", json!([]));
        pictured(&w, 7);
        let r = w.poll();
        assert_eq!(started(&w), "", "{ticked}");
        lacks(&w.gh(), "issue edit");
        lacks(&w.gh(), "api -X PATCH");
        has(&r.out, "hold #7: its text links an attachment");
    }
}

#[test]
fn a_retry_of_an_issue_whose_text_promises_a_file_waits_and_the_tick_stays() {
    let w = world("images-retry-worded");
    w.parked("failed", "alice");
    inst(&w, &["TEHAS_WORK_IMAGES=1"]);
    w.mesh(Some(MODELS));
    declares(&w, json!(["text", "image"]));
    w.comments(7, &park(&["x", " "]), "alice", json!([]));
    worded(&w, 7);
    w.touch("reads-images");
    let r = w.poll();
    assert_eq!(r.rc, 0);
    assert_eq!(started(&w), "");
    lacks(&w.gh(), "issue edit");
    lacks(&w.gh(), "api -X PATCH");
    has(&r.out, "hold #7: its text says SCREENSHOT GOES HERE");
}

fn sandboxed(w: &World) {
    w.reset();
    inst(w, &["TEHAS_WORK_IMAGES=1"]);
    w.mesh(Some(MODELS));
    declares(w, json!(["text", "image"]));
    w.touch("reads-images");
}

#[test]
fn an_issue_that_links_an_image_waits_while_the_sandbox_image_says_its_pi_rukas_does_not_read_them_and_the_model_is_not_asked(
) {
    let w = world("images-rukas-0");
    sandboxed(&w);
    pictured(&w, 7);
    image_says(&w, Some("0"));
    let r = w.poll();
    assert_eq!(started(&w), "");
    lacks(&w.gh(), "issue edit");
    assert_eq!(image_asks(&w), 0);
    has(&r.out, "hold #7: its text links an attachment and TEHAS_WORK_IMAGES is 1, but the pi-rukas in the sandbox image cccccccccccc does not read them");
    assert_eq!(why(&w), json!(""));
}

#[test]
fn an_image_that_reads_them_is_held_for_nothing_and_the_model_is_still_asked() {
    let w = world("images-rukas-1");
    sandboxed(&w);
    pictured(&w, 7);
    image_says(&w, Some("1"));
    w.poll();
    assert_eq!(started(&w), "7");
    assert_eq!(why(&w), json!(""));
    assert_eq!(image_asks(&w), 1);
}

#[test]
fn a_dispatch_json_that_cannot_be_read_does_not_stop_the_poll() {
    let w = world("images-bad-state");
    w.home_file("live/dispatch.json", "{\"time\":\n");
    assert_eq!(w.poll().rc, 0);
    assert_eq!(started(&w), "7");
    assert_eq!(why(&w), json!(""));
}

/// The harness's systemctl, with a slice probe that hangs for 120 seconds.
const SYSTEMCTL_HANGS: &str = r#"case "$1 $2" in
  "show tehas.slice")
    [ ! -e "$T/fx/slice-hangs" ] || exec sleep 120
    cat "$T/fx/slice.show" ;;
  "--user show")
    if [ -e "$T/fx/plan-runs" ] && [ "${6:-}" = "tehas-plan-$(<"$T/fx/plan-runs").service" ]; then echo active; else echo inactive; fi ;;
esac"#;

// TEHAS_SLICE_WAIT is 1. A loaded runner makes the poll slower, but not by a
// minute: a poll under 60 seconds was cut short by the bound, not by luck.
#[test]
fn a_slice_probe_that_hangs_holds_within_its_bound() {
    let w = world("images-slice-hangs");
    w.touch("slice-hangs");
    w.stub("systemctl", SYSTEMCTL_HANGS);
    w.env("TEHAS_SLICE_WAIT", "1");
    let began = Instant::now();
    let r = w.poll();
    assert!(began.elapsed().as_secs() < 60, "{:?}", began.elapsed());
    assert_eq!(r.rc, 0);
    has(&r.out, "holding: systemctl gave no answer");
    assert_eq!(started(&w), "");
    lacks(&w.gh(), "issue edit");
}
