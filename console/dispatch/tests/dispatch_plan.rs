//! Planning sessions: how they start, what their briefs say, and the labels
//! that ask for them.

mod common;

use common::*;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

fn v(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{e}: {text}"))
}

/// `filter` of #7's session on record, through jq.
fn rec_jq(w: &World, filter: &str) -> Value {
    v(&jq(filter, &w.rec().to_string()))
}

fn last_plan_row(w: &World) -> Value {
    let h = w.row(&["history-show", "7", "--kind", "plan"]);
    h["rows"]
        .as_array()
        .and_then(|r| r.last())
        .cloned()
        .unwrap_or(Value::Null)
}

/// The files under `dir`, at any depth, whose names start with `start`.
fn found(dir: &Path, start: &str) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| {
            let p = e.path();
            usize::from(e.file_name().to_string_lossy().starts_with(start))
                + if p.is_dir() { found(&p, start) } else { 0 }
        })
        .sum()
}

fn holding(text: &str, want: &str) -> usize {
    text.lines().filter(|l| l.contains(want)).count()
}

fn exactly(text: &str, want: &str) -> usize {
    text.lines().filter(|l| *l == want).count()
}

fn brief_line(w: &World) -> String {
    let record = w.record().display().to_string();
    let keep = w.keep().display().to_string();
    format!(
        "7 --out {record}.out --keep {keep} --brief {}/tmp/plan-7.brief.",
        w.home.display()
    )
}

fn lib_text(name: &str) -> String {
    fs::read_to_string(root().join("lib").join(name)).unwrap()
}

/// The refine brief as $(cat) gives it.
fn refine_brief() -> String {
    lib_text("refine-brief.md")
        .trim_end_matches('\n')
        .to_string()
}

fn out_left(w: &World) -> bool {
    let mut p = w.record().into_os_string();
    p.push(".out");
    Path::new(&p).exists()
}

fn first_lines(text: &str, n: usize) -> String {
    text.lines().take(n).collect::<Vec<_>>().join("\n")
}

#[test]
fn a_turn_that_hands_back_a_plan_for_its_issue_writes_it_there_and_ends_the_session_said_once() {
    let w = World::new("plan");
    w.to_plan();
    w.fx_json(
        "plan-result.json",
        &json!({"session": "abc-123", "text": "This issue now holds the plan.\n\n> # Faster search",
            "question": null,
            "update": {"title": "feat: faster search", "body": "## Outcome\n\nSearch returns results faster."}}),
    );
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    let gh = w.gh();
    has(
        &gh,
        "issue edit 7 --repo acme/widget --title feat: faster search --body ## Outcome\n\nSearch returns results faster.",
    );
    has(&gh, "--remove-label tehas:question --remove-label tehas:planning --remove-label tehas:plan --remove-label needs-human-attention");
    assert_eq!(holding(&gh, "This issue now holds the plan."), 1);
    lacks(&gh, "tehas:question kind=plan");
    assert_eq!(w.rec_open(), "none");
    assert!(!out_left(&w));
}

#[test]
fn a_plan_that_cannot_be_written_onto_its_issue_is_written_by_the_next_poll_and_the_session_ends_then(
) {
    let w = World::new("plan");
    w.to_plan();
    w.fx_json(
        "plan-result.json",
        &json!({"session": "abc-123", "text": "This issue now holds the plan.", "question": null,
            "update": {"title": "feat: faster search", "body": "Search returns results faster."}}),
    );
    w.fx("edit-fails-on", "--title feat: faster search\n");
    let r = w.poll();
    has(
        &r.out,
        "WARN: the plan could not be written onto #7; the next poll tries again",
    );
    lacks(&w.gh(), "This issue now holds the plan.");
    assert_eq!(w.rec_open(), "open");
    // The label came off when the turn started.
    w.fx("plans.json", "[]");
    w.rm_fx("edit-fails-on");
    let _ = fs::remove_file(w.t().join("calls/gh"));
    w.poll();
    has(
        &w.gh(),
        "issue edit 7 --repo acme/widget --title feat: faster search --body Search returns results faster.",
    );
    assert_eq!(holding(&w.gh(), "This issue now holds the plan."), 1);
    assert_eq!(w.rec_open(), "none");
}

#[test]
fn an_issue_with_no_body_is_planned_from_its_title() {
    let w = World::new("plan");
    w.to_plan();
    w.fx("issue-7.json", r#"{"title":"Faster search","body":null}"#);
    w.poll();
    has(&w.plan_calls(), "--brief");
    assert_eq!(w.rec_brief().lines().next(), Some("# Faster search"));
}

#[test]
fn a_refining_session_keeps_its_label_on_record_in_each_line_of_history_and_in_the_analytics_how() {
    let w = World::new("plan");
    w.to_refine();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.rec()["label"], json!("tehas:refine"));
    assert_eq!(last_plan_row(&w)["label"], json!("tehas:refine"));
}

// A person who applies the label to a parked issue asks for a session, as
// tehas:ready there starts a cycle. tehas:question is no bar to the ask, and
// the start takes it off, so the park's boxes answer for nothing after it.
#[test]
fn tehas_refine_on_a_parked_issue_starts_a_session_with_the_refine_brief_and_takes_tehas_question_off(
) {
    let w = World::new("plan");
    w.parked_plan("refine", "alice");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&w.plan_calls(), &brief_line(&w));
    has(&w.rec_brief(), &format!("---\n\n{}", refine_brief()));
    has(
        &r.out,
        "start planning #7 (labelled by alice, to refine it)",
    );
    has(&w.gh(), "issue edit 7 --repo acme/widget --add-label tehas:planning --remove-label tehas:refine --remove-label tehas:question --remove-label needs-human-attention");
    // A retry that was not ticked answers for nothing, and the park's id goes with
    // the label: the boxes of the old park must not answer for the new session.
    assert_eq!(w.park_q(7), "none");
    assert_eq!(last_plan_row(&w)["label"], json!("tehas:refine"));
}

#[test]
fn tehas_plan_on_a_parked_issue_starts_a_session_from_the_issue_s_text_and_needs_human_attention_is_the_cycle_s_to_clear(
) {
    let w = World::new("plan");
    w.parked_plan("plan", "alice");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&w.plan_calls(), &brief_line(&w));
    let brief = w.rec_brief();
    assert_eq!(first_lines(&brief, 3), "# Faster search\n\nMake it fast.");
    let head: String = lib_text("refine-brief.md")
        .bytes()
        .take(40)
        .map(char::from)
        .collect();
    lacks(&brief, &head);
    lacks(&brief, "is not a duplicate of this plan");
    has(&w.gh(), "issue edit 7 --repo acme/widget --add-label tehas:planning --remove-label tehas:plan --remove-label tehas:question --remove-label needs-human-attention");
    has(&w.gh(), "authorized by @alice's `tehas:plan` label");
    assert_eq!(w.park_q(7), "none");
}

#[test]
fn a_session_whose_start_the_store_will_not_take_does_not_start_and_its_park_s_question_stays() {
    let w = World::new("plan");
    w.parked_plan("refine", "alice");
    w.store_refuses("plan-begin 7", 1, "store: busy #7: the database is locked");
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "ERROR: could not write the record of #7, not starting a turn (busy)",
    );
    assert_eq!(found(&w.home.join("tmp"), "plan-7.brief."), 0);
    assert_eq!(w.plan_calls(), "");
    lacks(&w.gh(), "issue edit");
    assert_eq!(w.park_q(7), "4001");
}

#[test]
fn a_refining_turn_that_files_an_issue_notes_it_in_the_ledger_of_refined_issues_a_planning_one_does_not(
) {
    let w = World::new("plan");
    let ledger = w.home.join("state/refined.jsonl");
    w.to_refine();
    w.plan_files(&["https://github.com/acme/widget/issues/41"]);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        jq(
            "[.source, .filed, .url]",
            &fs::read_to_string(&ledger).unwrap_or_default()
        ),
        r#"[7,41,"https://github.com/acme/widget/issues/41"]"#
    );
    assert_eq!(
        last_plan_row(&w)["filed"],
        json!(["https://github.com/acme/widget/issues/41"])
    );
    let _ = fs::remove_file(&ledger);
    w.to_plan();
    w.plan_files(&["https://github.com/acme/widget/issues/42"]);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert!(!ledger.exists());
    assert_eq!(last_plan_row(&w)["label"], json!("tehas:plan"));
}

#[test]
fn the_ledger_of_refined_issues_keeps_its_last_500_lines_as_the_ledger_of_sizes_does() {
    let w = World::new("plan");
    w.to_refine();
    let ledger = w.home.join("state/refined.jsonl");
    let lines: String = (1..=500)
        .map(|i| format!("{{\"at\":\"x\",\"source\":0,\"filed\":{i},\"url\":\"u\"}}\n"))
        .collect();
    fs::write(&ledger, lines).unwrap();
    assert_eq!(fs::read_to_string(&ledger).unwrap().lines().count(), 500);
    w.plan_files(&["https://github.com/acme/widget/issues/41"]);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    // The new line is on it, and the file is no longer than the 500 it keeps.
    let text = fs::read_to_string(&ledger).unwrap();
    assert_eq!(text.lines().count(), 500);
    let filed: Vec<Value> = text.lines().map(|l| v(l)["filed"].clone()).collect();
    assert_eq!(json!([filed.first(), filed.last()]), json!([2, 41]));
}

#[test]
fn the_refine_brief_asks_for_one_part_sized_for_one_cycle_grounded_in_the_code_with_exact_paths() {
    let text = lib_text("refine-brief.md");
    let brief = text.replace('\n', " ");
    has(&brief, "a single unattended `/work` cycle finishes");
    has(
        &brief,
        "plan the first part only, the one the others build on",
    );
    has(&brief, "under \"Later parts\"");
    assert!(text.len() < 4000, "{}", text.len());
}

#[test]
fn the_issue_brief_which_every_session_is_given_asks_for_a_plan_grounded_in_the_code_with_exact_paths(
) {
    let text = lib_text("issue-brief.md");
    let brief = text.replace('\n', " ");
    has(&brief, "Ground it in the code first");
    has(&brief, "[paths: `a`, `b`]");
    has(&brief, "`.github/` or `.pi/`");
    has(
        &brief,
        "Classify the plan in its title, as a conventional-commit type",
    );
    has(&brief, "Keep a type the    request's title already has.");
    has(&brief, "no inventory of the request");
    assert!(text.len() < 4000, "{}", text.len());
}

#[test]
fn without_the_refine_brief_on_the_host_no_session_starts_and_it_is_logged() {
    let w = World::new("plan");
    w.to_refine();
    let short = w.t().join("norefine");
    for d in ["bin", "libexec", "lib"] {
        fs::create_dir_all(short.join(d)).unwrap();
    }
    for e in fs::read_dir(root().join("lib")).unwrap().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.ends_with(".sh") {
            fs::copy(e.path(), short.join("lib").join(&name)).unwrap();
        }
    }
    for e in fs::read_dir(root().join("libexec")).unwrap().flatten() {
        fs::copy(e.path(), short.join("libexec").join(e.file_name())).unwrap();
    }
    for verb in ["tehas", "tehasbot"] {
        fs::copy(root().join("bin").join(verb), short.join("bin").join(verb)).unwrap();
    }
    fs::copy(w.console(), short.join("libexec/console")).unwrap();
    // The Rust poll finds the brief beside its own program, as on a host.
    fs::copy(w.dispatcher(), short.join("libexec/tehas-dispatch")).unwrap();
    let r =
        w.sh("unset TEHAS_DISPATCH_CMD TEHAS_HEALTH_CMD; \"$T/norefine/bin/tehasbot\" dispatch");
    assert_eq!(w.plan_calls(), "");
    has(
        &r.out,
        &format!(
            "ERROR: no refine brief at {}/libexec/../lib/refine-brief.md, not refining #7",
            short.display()
        ),
    );
}

#[test]
fn a_plan_label_from_the_bot_or_a_stranger_starts_nothing_and_is_taken_off_the_issue() {
    let w = World::new("plan");
    for other in ["tehas-bot", "mallory"] {
        w.to_plan();
        w.fx_json(
            "events-7.json",
            &json!([{"event": "labeled", "label": {"name": "tehas:plan"}, "actor": {"login": other}}]),
        );
        let r = w.poll();
        assert_eq!(w.plan_calls(), "");
        has(
            &r.out,
            &format!("skip #7: tehas:plan was applied by '{other}', not an allowed account"),
        );
        has(
            &r.out,
            &format!(
                "clear #7: tehas:plan off, applied by '{other}' who may not authorise work here"
            ),
        );
        assert_eq!(
            exactly(
                &w.gh(),
                "issue edit 7 --repo acme/widget --remove-label tehas:plan"
            ),
            1
        );
        assert_eq!(count_lines(&w.gh(), "issue comment "), 1);
    }
}

#[test]
fn the_poll_s_lock_is_not_handed_to_a_planning_turn() {
    let w = World::new("plan");
    w.to_plan();
    w.fx(
        "awaiting.json",
        r#"[{"number":1},{"number":2},{"number":3}]"#,
    );
    w.poll();
    has(&w.plan_calls(), "--brief");
    assert!(!w.t().join("calls/plan.fd8").exists());
}

// The decision of the next three is a fixture case too; these hold what the
// poll writes for it.
#[test]
fn a_plan_label_from_an_allowed_account_starts_a_session_from_the_issue_s_text() {
    let w = World::new("plan");
    w.to_plan();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&w.plan_calls(), &brief_line(&w));
    lacks(&w.plan_calls(), "--kind refine");
    assert_eq!(w.rec_brief(), "# Faster search\n\nMake it fast.");
    let gh = w.gh();
    has(
        &gh,
        "issue edit 7 --repo acme/widget --add-label tehas:planning --remove-label tehas:plan",
    );
    has(&gh, "authorized by @alice's `tehas:plan` label");
    has(&gh, "- [ ] Adopt all");
    has(&gh, "<!-- tehas:question kind=plan issue=7 turn=1 -->");
    has(
        &gh,
        "--remove-label tehas:planning --add-label tehas:question",
    );
    assert_eq!(
        rec_jq(&w, "del(.startedAt)"),
        json!({"session": "abc-123", "turns": 1, "options": ["Adopt all", "Change some"],
            "pending": false, "failed": false, "running": false, "comment": 4002, "by": "alice",
            "label": "tehas:plan", "answers": []})
    );
    assert!(!out_left(&w));
    // The brief was the turn's until it ended; the record keeps the text.
    assert_eq!(found(&w.home.join("tmp"), "plan-7.brief."), 0);
    assert_eq!(w.calls("fake-start"), "");
}

#[test]
fn a_refine_label_from_an_allowed_account_starts_a_session_with_the_refine_brief_after_the_issue_s_text(
) {
    let w = World::new("plan");
    w.to_refine();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&w.plan_calls(), &brief_line(&w));
    let brief = w.rec_brief();
    assert_eq!(first_lines(&brief, 3), "# Faster search\n\nMake it fast.");
    has(&brief, &format!("---\n\n{}", refine_brief()));
    assert_eq!(
        brief.lines().last(),
        Some("This request is issue #7 itself. The issue you file replaces #7: #7 is not a duplicate of this plan.")
    );
    has(
        &r.out,
        "start planning #7 (labelled by alice, to refine it)",
    );
    has(
        &w.gh(),
        "issue edit 7 --repo acme/widget --add-label tehas:planning --remove-label tehas:refine",
    );
    lacks(&w.gh(), "--remove-label tehas:plan ");
    has(&w.gh(), "authorized by @alice's `tehas:refine` label");
}

// tehas:question excludes none of them; the park's boxes are read first, so a
// ticked retry still gets its cycle and its tick is undone with the start.
#[test]
fn a_ticked_retry_of_a_parked_issue_starts_its_cycle_whatever_planning_label_it_carries() {
    let w = World::new("plan");
    w.parked_plan("refine", "alice");
    w.comments(7, &park_body(&["x", " "]), "alice", json!([]));
    w.poll();
    assert_eq!(w.calls("fake-start").trim_end(), "7");
    assert_eq!(w.plan_calls(), "");
    assert_eq!(w.park_q(7), "none");
    let gh = w.gh();
    has(
        &gh,
        "issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:question",
    );
    // The tick is undone with the start, so it cannot fire again later.
    has(&gh, "api -X PATCH repos/acme/widget/issues/comments/4001");
    has(&gh, "- [ ] Retry from a clean start");
    // The ask for a session waits for the poll after the cycle: its label stays.
    lacks(&gh, "--remove-label tehas:refine");
}
