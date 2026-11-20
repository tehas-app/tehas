//! The session id a turn's file carries, the words of a turn the store is
//! given, and the analytics of a planning turn.

mod common;

use common::*;
use regex::Regex;
use serde_json::{json, Value};
use std::fs;

fn v(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{e}: {text}"))
}

/// `filter` of #7's session on record, through jq.
fn rec_jq(w: &World, filter: &str) -> Value {
    v(&jq(filter, &w.rec().to_string()))
}

/// The analytics program, where install.sh puts it. A call notes the record
/// as the program reads it: its facts.
fn plan_analytics(w: &World, body: &str) {
    w.stub(
        "analytics",
        &format!(
            "[ \"$1 $3\" != \"plan --facts\" ] || cp \"$4\" \"$T/calls/record.at-analytics\" 2>/dev/null\n{body}"
        ),
    );
    let libexec = w.home.join("libexec");
    fs::create_dir_all(&libexec).unwrap();
    fs::rename(w.t().join("bin/analytics"), libexec.join("analytics")).unwrap();
}

/// The plan calls of analytics, without the facts file's name, which is new
/// each time.
fn plan_analytics_calls(w: &World) -> String {
    let facts = Regex::new(r" --facts [^ ]*").unwrap();
    w.calls("analytics")
        .lines()
        .filter(|l| l.starts_with("plan"))
        .map(|l| facts.replace_all(l, "").into_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

fn e64(c: char) -> String {
    std::iter::repeat_n(c, 64).collect()
}

#[test]
fn a_session_id_that_is_not_a_string_in_a_failed_turn_s_file_is_no_session() {
    let w = World::new("plan-answers");
    w.to_plan();
    w.fx("plan-fails", "{\"session\":{\"a\":1}}\n");
    w.poll();
    has(&w.gh(), "could not start");
    assert_eq!(w.rec_open(), "none");
}

#[test]
fn a_first_turn_whose_session_id_is_longer_than_the_store_keeps_is_no_session_and_nothing_is_asked()
{
    let w = World::new("plan-answers");
    w.to_plan();
    w.fx_json(
        "plan-result.json",
        &json!({"session": "a".repeat(200), "text": "Two ways.",
            "question": {"question": "Adopt?", "options": [{"label": "Adopt all"}]}}),
    );
    w.poll();
    has(&w.gh(), "could not start");
    lacks(&w.gh(), "kind=plan issue=7");
    assert_eq!(w.rec_open(), "none");
}

#[test]
fn line_ends_in_what_a_turn_names_are_no_words_of_the_store_s_command_the_bad_address_is_not_filed()
{
    let w = World::new("plan-answers");
    w.to_plan();
    w.fx_json(
        "plan-result.json",
        &json!({"session": "abc-123", "text": "Two ways.",
            "question": {"question": "Adopt?",
                "options": [{"label": "Adopt\nall --filed x"}, {"label": "Change some"}]},
            "filed": ["https://github.com/acme\nwidget/issues/41",
                "https://github.com/acme/widget/issues/42\n",
                "https://github.com/acme/widget/issues/43"]}),
    );
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        rec_jq(&w, "[.session, .turns, .running, .options, .filed]"),
        json!([
            "abc-123",
            1,
            false,
            ["Adopt all --filed x", "Change some"],
            ["https://github.com/acme/widget/issues/43"]
        ])
    );
    let mut rc = w.record().into_os_string();
    rc.push(".rc");
    assert!(!std::path::Path::new(&rc).exists());
}

#[test]
fn a_planning_turn_that_ends_is_recorded_in_analytics_from_its_record_before_it_is_rewritten() {
    let w = World::new("plan-answers");
    w.to_plan();
    w.rm_fx("image.label");
    plan_analytics(&w, "");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(plan_analytics_calls(&w), "plan 7 --rc 0");
    let at = fs::read_to_string(w.t().join("calls/record.at-analytics")).unwrap_or_default();
    assert_eq!(
        v(&jq(
            "[.running, .turns, (.startedAt | type), .by, .label]",
            &at
        )),
        json!([true, 0, "string", "alice", "tehas:plan"])
    );
    assert_eq!(w.rec()["turns"], json!(1));
}

#[test]
fn with_the_pi_rukas_commit_of_the_image_s_label() {
    let w = World::new("plan-answers");
    w.to_plan();
    w.fx("image.label", "abcdef1|1\n");
    plan_analytics(&w, "");
    w.poll();
    assert_eq!(
        plan_analytics_calls(&w),
        "plan 7 --rc 0 --rukas-image abcdef1"
    );
}

#[test]
fn with_the_image_s_id_and_when_it_was_built() {
    let w = World::new("plan-answers");
    w.to_plan();
    w.fx(
        "image.label",
        &format!("abcdef1|1|sha256:{}|2026-10-29T14:44:16.5Z\n", e64('e')),
    );
    plan_analytics(&w, "");
    w.poll();
    assert_eq!(
        plan_analytics_calls(&w),
        format!(
            "plan 7 --rc 0 --rukas-image abcdef1 --image sha256:{} --image-built 2026-10-29T14:44:16.5Z",
            e64('e')
        )
    );
}

#[test]
fn the_image_is_the_one_the_turn_started_in_not_the_one_the_image_name_points_at_as_it_ends() {
    let w = World::new("plan-answers");
    w.to_plan();
    w.fx(
        "image.label",
        &format!("abcdef1|1|sha256:{}|2026-10-29T14:44:16.5Z\n", e64('e')),
    );
    w.fx(
        "image.during",
        &format!("1234567|1|sha256:{}|2026-10-29T15:00:00Z\n", e64('f')),
    );
    plan_analytics(&w, "");
    w.poll();
    assert_eq!(
        plan_analytics_calls(&w),
        format!(
            "plan 7 --rc 0 --rukas-image abcdef1 --image sha256:{} --image-built 2026-10-29T14:44:16.5Z",
            e64('e')
        )
    );
    assert!(!w
        .files(&w.home.join("state"))
        .contains(&"plan-7.json.image".to_string()));
}

#[test]
fn an_image_id_of_another_shape_is_left_out_and_so_is_its_time() {
    let w = World::new("plan-answers");
    w.to_plan();
    w.fx(
        "image.label",
        "abcdef1|1|sha256:short|2026-10-29T14:44:16.5Z\n",
    );
    plan_analytics(&w, "");
    w.poll();
    assert_eq!(
        plan_analytics_calls(&w),
        "plan 7 --rc 0 --rukas-image abcdef1"
    );
}

#[test]
fn a_turn_that_fails_and_a_turn_cut_off_are_recorded_with_how_they_ended() {
    let w = World::new("plan-answers");
    w.to_plan();
    w.touch("plan-fails");
    plan_analytics(&w, "");
    w.poll();
    assert_eq!(plan_analytics_calls(&w), "plan 7 --rc 2");
    w.rm_fx("plan-fails");
    w.planning();
    w.t_plan(
        7,
        json!({"turns": 1, "options": ["Adopt all", "Change some"], "running": true,
            "startedAt": "2026-01-01T00:00:00Z"}),
    );
    plan_analytics(&w, "");
    w.poll();
    assert_eq!(plan_analytics_calls(&w), "plan 7 --rc interrupted");
}

#[test]
fn with_tehas_analytics_off_a_planning_turn_is_not_recorded() {
    let w = World::new("plan-answers");
    w.to_plan();
    plan_analytics(&w, "");
    let mut lines = MINIMAL.to_vec();
    lines.push("TEHAS_ANALYTICS=off");
    w.instance(&lines);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.calls("analytics"), "");
    has(
        &w.gh(),
        "--remove-label tehas:planning --add-label tehas:question",
    );
}

#[test]
fn a_failing_or_hanging_analytics_changes_nothing_in_the_turn() {
    let w = World::new("plan-answers");
    // The start comment names the minute: two runs either side of one differ
    // only there.
    let minute = Regex::new(r" at [0-9]{4}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2} UTC").unwrap();
    let gh_calls = |w: &World| minute.replace_all(&w.gh(), " at <time> UTC").into_owned();
    w.to_plan();
    w.poll();
    let without_gh = gh_calls(&w);
    let without_record = rec_jq(&w, "del(.startedAt)");
    for body in ["exit 70", "exec sleep 30"] {
        w.to_plan();
        plan_analytics(&w, &format!("[ \"$1\" != plan ] || {body}"));
        w.env("TEHAS_ANALYTICS_WAIT", "1");
        let r = w.poll();
        assert_eq!(r.rc, 0, "{}", r.out);
        has(&r.out, "WARN analytics plan: exit");
        assert_eq!(gh_calls(&w), without_gh);
        assert_eq!(rec_jq(&w, "del(.startedAt)"), without_record);
    }
}

// The decision is a fixture case too; this holds the record and the dry run.
#[test]
fn tehas_ready_on_an_issue_with_an_open_planning_session_starts_no_cycle() {
    let w = World::new("plan-answers");
    w.planning();
    w.comments(7, &plan_body(" ", " ", " ", 1, ""), "", json!([]));
    w.fx(
        "ready.json",
        r#"[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"tehas:question"}]}]"#,
    );
    let before = w.rec();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.calls("fake-start"), "");
    lacks(&w.gh(), "issue edit");
    has(
        &r.out,
        "skip #7: a planning session is open on it; end it first",
    );
    assert_eq!(w.rec(), before);
    let r = w.dispatch(&["--dry-run"]);
    has(
        &r.out,
        "skip #7: a planning session is open on it; end it first",
    );
    lacks(&r.out, "would run a cycle");
}
