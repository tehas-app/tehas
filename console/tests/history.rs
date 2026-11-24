//! `tehas history` end to end: the cycles and planning turns that ended, in
//! the order they were written, after a cursor, for one issue, with the park
//! fields. Run as a host runs it: bin/tehas, this build's console, the real
//! store. History asks GitHub for nothing: gh is a stub that answers nothing,
//! and neither it nor git is called. Moved from test/history_test.sh.

mod common;

use common::*;
use std::fs;
use std::path::Path;
use tehas_core::time::iso;

/// The cursors below name these seconds, so a case says what it means.
const T0: i64 = 1760700000;

fn host(name: &str) -> World {
    let w = World::new(&format!("history-{name}"));
    w.gh_stub("");
    w.stub("git", "");
    w
}

fn history(w: &World, words: &[&str]) -> Run {
    w.tehas("history", words)
}

/// tehas history --json with these words.
fn run_json(w: &World, words: &[&str]) -> Run {
    let mut all = vec!["--json"];
    all.extend_from_slice(words);
    history(w, &all)
}

/// bin/tehas history as a shell snippet.
fn history_cmd() -> String {
    format!("{} history", root().join("bin/tehas").display())
}

/// jq -r of a filter that gives one string, or its JSON otherwise.
fn raw(filter: &str, input: &str) -> String {
    let s = jq(filter, input);
    serde_json::from_str::<String>(&s).unwrap_or(s)
}

/// The cursor of each line listed, in the order it was listed.
fn cursors(out: &str) -> String {
    raw("[.runs[].cursor] | @tsv", out)
}

const MERGED: &str = r#"{"kind":"work","rc":0,"status":"merged"}"#;
const PLANNED: &str = r#"{"kind":"plan","rc":0,"status":"finished","turn":1}"#;
const FULL: &str = r#"{"kind":"work","started":1760696400,"rc":10,"status":"handoff","step":"review","cap":"awaiting-human-merge","park_reason":"merge","failure_kind":"review","failure_cause":"findings-open","provider":false,"parked":true,"pr":"https://github.com/acme/widget/pull/9","by":"alice","how":"approved"}"#;
const PARKED: &str = r#"{"kind":"work","rc":10,"status":"failed","step":"develop","cap":"step-failed:develop","park_reason":"","failure_kind":"verify-failed","failure_cause":"exit-1","parked":true}"#;

/// The line of FULL as history prints it.
fn full_line() -> String {
    format!(
        r#"{{"cursor":"1760700000-12-1","kind":"work","issue":12,"cycle":"12-1","ended":"{}","started":"{}","status":10,"outcome":"handoff","end":"handoff","progress":{{"step":"review"}},"cap":"awaiting-human-merge","parkReason":"merge","failureKind":"review","failureCause":"findings-open","provider":false,"parked":true,"pr":"https://github.com/acme/widget/pull/9","change":9,"by":"alice","how":"approved"}}"#,
        iso(T0),
        iso(1760696400)
    )
}

#[test]
fn one_cycle_the_object_its_fields_and_their_types() {
    let w = host("one");
    w.history_end(12, T0, FULL);
    let r = run_json(&w, &[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        jq("del(.runs)", &r.out),
        r#"{"api":1,"next":"1760700000-12-1"}"#
    );
    assert_eq!(jq(".runs[0]", &r.out), full_line());
    assert_eq!(
        jq(
            ".runs[0] | [(.status|type), (.parked|type), (.provider|type), (.issue|type), (.turn|type)]",
            &r.out
        ),
        r#"["number","boolean","boolean","number","null"]"#
    );
    assert_eq!(jq(".api", &r.out), "1");
    assert_eq!(w.gh(), "");
    assert_eq!(w.calls("git"), "");
}

#[test]
fn the_object_is_printed_byte_for_byte_keys_in_their_order() {
    let w = host("bytes");
    w.history_end(12, T0, FULL);
    let refine = label_of("Act::Refine");
    w.plan_end(
        7,
        T0 + 300,
        &format!(
            r#"{{"kind":"plan","started":1760700000,"rc":0,"status":"finished","turn":1,"by":"alice","label":"{refine}","filed":["https://github.com/acme/widget/issues/41"],"auto":true}}"#
        ),
    );
    let r = run_json(&w, &[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        r.out,
        format!(
            r#"{{"api":1,"runs":[{},{{"cursor":"1760700300-7-2","kind":"plan","issue":7,"ended":"{}","started":"{}","status":0,"outcome":"finished","turn":1,"by":"alice","label":"{refine}","auto":true,"filed":["https://github.com/acme/widget/issues/41"]}}],"next":"1760700300-7-2"}}"#,
            full_line(),
            iso(T0 + 300),
            iso(1760700000)
        )
    );
}

#[test]
fn a_field_the_line_does_not_have_is_left_out_never_null() {
    let w = host("absent");
    w.history_end(12, T0, MERGED);
    let r = run_json(&w, &[]);
    assert_eq!(
        jq(".runs[0]", &r.out),
        format!(
            r#"{{"cursor":"1760700000-12-1","kind":"work","issue":12,"cycle":"12-1","ended":"{}","started":"{}","status":0,"outcome":"merged","end":"merged","provider":false,"parked":false,"how":"hand"}}"#,
            iso(T0),
            iso(T0 - 60)
        )
    );
    assert_eq!(
        jq(
            r#".runs[0] | [has("progress"), has("cap"), has("parkReason"), has("pr"), has("by")]"#,
            &r.out
        ),
        "[false,false,false,false,false]"
    );
}

#[test]
fn oldest_first_across_issues_with_the_cursor_of_each() {
    let w = host("order");
    w.history_end(9, T0 - 10, r#"{"kind":"work","rc":1,"status":"failed"}"#);
    w.history_end(7, T0, MERGED);
    w.history_end(12, T0 + 600, MERGED);
    let r = run_json(&w, &[]);
    assert_eq!(
        cursors(&r.out),
        "1760699990-9-1\t1760700000-7-2\t1760700600-12-3"
    );
    assert_eq!(raw(".next", &r.out), "1760700600-12-3");
}

#[test]
fn lines_that_ended_in_the_same_second_are_listed_in_the_order_they_were_written_none_repeated_or_skipped(
) {
    let w = host("same-second");
    for n in [3, 11, 4] {
        w.history_end(n, T0, MERGED);
    }
    let r = run_json(&w, &[]);
    assert_eq!(
        cursors(&r.out),
        "1760700000-3-1\t1760700000-11-2\t1760700000-4-3"
    );
    assert_eq!(jq("[.runs[].cursor] | unique | length", &r.out), "3");
    // A program keeps "next" and asks again: what it has read is never
    // offered again, and nothing that ended later is missed.
    let mid = raw(".runs[1].cursor", &r.out);
    let last = raw(".next", &r.out);
    let r = run_json(&w, &["--after", &mid]);
    assert_eq!(cursors(&r.out), "1760700000-4-3");
    let r = run_json(&w, &["--after", &last]);
    assert_eq!(
        jq("{runs, next}", &r.out),
        format!(r#"{{"runs":[],"next":"{last}"}}"#)
    );
}

#[test]
fn after_a_cursor_before_everything_lists_all_of_them_once() {
    let w = host("all");
    for n in 1..=3 {
        w.history_end(n, T0 + n as i64, MERGED);
    }
    let r = run_json(&w, &["--after", &format!("{}-1", T0 - 10)]);
    assert_eq!(raw("[.runs[].issue] | @tsv", &r.out), "1\t2\t3");
    assert_eq!(raw(".next", &r.out), "1760700003-3-3");
}

#[test]
fn an_old_cursor_without_the_place_is_read_as_before_on_the_seconds_then_the_issue() {
    let w = host("old-cursor");
    w.history_end(3, T0, MERGED);
    w.history_end(4, T0, MERGED);
    let r = run_json(&w, &["--after", "1760700000-3"]);
    assert_eq!(cursors(&r.out), "1760700000-4-2");
    assert_eq!(raw(".next", &r.out), "1760700000-4-2");
}

// The old cursor named the seconds and the issue only: a program that read #12
// kept 1760700000-12, and #7, written a moment later for the same second, was
// never listed to it.
#[test]
fn a_line_written_after_a_read_for_the_same_second_and_a_lower_issue_is_listed_by_the_next_read() {
    let w = host("later-lower");
    w.history_end(12, T0, MERGED);
    let next = raw(".next", &run_json(&w, &[]).out);
    w.history_end(7, T0, MERGED);
    let r = run_json(&w, &["--after", &next]);
    assert_eq!(cursors(&r.out), "1760700000-7-2");
}

#[test]
fn a_cycle_and_a_planning_turn_of_one_issue_in_one_second_each_have_a_cursor_of_their_own() {
    let w = host("both-one-second");
    w.history_end(12, T0, MERGED);
    w.plan_end(12, T0, PLANNED);
    let r = run_json(&w, &[]);
    assert_eq!(cursors(&r.out), "1760700000-12-1\t1760700000-12-2");
    let r = run_json(&w, &["--after", "1760700000-12-1"]);
    assert_eq!(raw("[.runs[].kind] | @tsv", &r.out), "plan");
}

#[test]
fn issue_lists_one_issue_s_lines_from_its_cycle_and_its_planning_turns() {
    let w = host("issue");
    w.plan_end(12, T0 - 60, PLANNED);
    w.history_end(12, T0, MERGED);
    w.history_end(121, T0, r#"{"kind":"work","rc":1,"status":"failed"}"#);
    let r = run_json(&w, &["--issue", "12"]);
    assert_eq!(
        raw(r#"[.runs[] | "\(.kind):\(.issue)"] | @tsv"#, &r.out),
        "plan:12\twork:12"
    );
    assert_eq!(raw("[.runs[].issue] | unique | @tsv", &r.out), "12");
    let r = run_json(&w, &["--issue", "121"]);
    assert_eq!(raw(".runs[0].kind", &r.out), "work");
    assert_eq!(run_json(&w, &["--issue", "0"]).rc, 64);
}

#[test]
fn a_planning_turn_is_listed_the_same_way_with_kind_plan() {
    let w = host("plan");
    w.plan_end(
        7,
        T0 + 300,
        r#"{"kind":"plan","started":1760700000,"rc":0,"status":"finished","turn":1,"by":"alice"}"#,
    );
    let r = run_json(&w, &[]);
    assert_eq!(
        jq(".runs[0]", &r.out),
        format!(
            r#"{{"cursor":"1760700300-7-1","kind":"plan","issue":7,"ended":"{}","started":"{}","status":0,"outcome":"finished","turn":1,"by":"alice","label":"{}"}}"#,
            iso(T0 + 300),
            iso(1760700000),
            label_of("Act::Plan")
        )
    );
    assert_eq!(raw(".next", &r.out), "1760700300-7-1");
}

#[test]
fn a_refining_turn_names_its_label_and_the_issues_it_filed_as_json_and_as_text() {
    let w = host("refine");
    let refine = label_of("Act::Refine");
    w.plan_end(
        7,
        T0 + 300,
        &format!(
            r#"{{"kind":"plan","rc":0,"status":"finished","turn":1,"by":"alice","label":"{refine}","filed":["https://github.com/acme/widget/issues/41","not an issue"]}}"#
        ),
    );
    let r = run_json(&w, &[]);
    assert_eq!(
        jq(".runs[0] | [.label, .filed]", &r.out),
        format!(r#"["{refine}",["https://github.com/acme/widget/issues/41"]]"#)
    );
    let r = history(&w, &[]);
    has(
        &r.out,
        "plan  #7  finished  turn 1  refine  filed https://github.com/acme/widget/issues/41",
    );
}

#[test]
fn a_turn_of_a_refine_the_poll_started_on_its_own_says_auto_and_a_labelled_one_leaves_it_out() {
    let w = host("auto");
    let refine = label_of("Act::Refine");
    w.plan_end(
        7,
        T0 + 300,
        &format!(
            r#"{{"kind":"plan","rc":0,"status":"finished","turn":1,"by":"alice","label":"{refine}","auto":true}}"#
        ),
    );
    w.plan_end(
        8,
        T0 + 400,
        &format!(
            r#"{{"kind":"plan","rc":0,"status":"finished","turn":1,"by":"alice","label":"{refine}"}}"#
        ),
    );
    let r = run_json(&w, &[]);
    assert_eq!(
        jq("[.runs[] | [.issue, .auto]]", &r.out),
        "[[7,true],[8,null]]"
    );
    let r = history(&w, &[]);
    has(&r.out, "plan  #7  finished  turn 1  refine (auto)");
    lacks(&r.out, "#8  finished  turn 1  refine (auto)");
}

#[test]
fn both_kinds_together_and_a_turn_whose_unit_left_no_readable_status() {
    let w = host("both");
    w.history_end(12, T0, MERGED);
    w.plan_end(
        12,
        T0,
        r#"{"kind":"plan","status":"not-started","turn":1,"by":"alice"}"#,
    );
    let r = run_json(&w, &[]);
    assert_eq!(
        raw(
            r#"[.runs[] | "\(.kind) \(.outcome) \(.status // "none")"] | @tsv"#,
            &r.out
        ),
        "work merged 0\tplan not-started none"
    );
}

#[test]
fn the_files_the_planning_history_was_kept_in_before_the_store_are_not_read() {
    let w = host("old-files");
    let planted = format!(
        "{{\"kind\":\"plan\",\"at\":\"{}\",\"rc\":0,\"status\":\"finished\"}}\n",
        iso(T0)
    );
    let cycles = w.at().cycles_dir();
    fs::create_dir_all(&cycles).unwrap();
    fs::write(cycles.join("plan-9.history.jsonl"), &planted).unwrap();
    fs::write(w.at().state_dir().join("plan-6.history.jsonl"), &planted).unwrap();
    let r = run_json(&w, &[]);
    assert_eq!(jq(".runs", &r.out), "[]");
    assert_eq!(jq(".next", &r.out), "null");
}

#[test]
fn a_park_of_a_run_of_parks_shows_its_count_as_json_and_as_text() {
    let w = host("repeat");
    // Two parks of #12 that stopped exactly alike: the second is number 1 of
    // the run, because the end before it is the park that started it. #11
    // stopped the same way but on an issue of its own, which no run of #12
    // counts.
    w.history_end(12, T0, PARKED);
    w.history_end(12, T0 + 60, PARKED);
    w.history_end(11, T0 + 120, PARKED);
    let r = run_json(&w, &[]);
    assert_eq!(
        jq("[.runs[] | [.issue, .parked, .repeat]]", &r.out),
        "[[12,true,null],[12,true,1],[11,true,null]]"
    );
    let r = history(&w, &[]);
    has(
        &r.out,
        "  work  #12  failed  rc 10  step develop  parked  repeat 1",
    );
    lacks(&r.out, "#11  failed  rc 10  step develop  parked  repeat");
    lacks(
        &r.out,
        "1760700000-12-1  work  #12  failed  rc 10  step develop  parked  repeat",
    );
}

fn repeats(w: &World) -> String {
    jq("[.runs[] | .repeat]", &run_json(w, &[]).out)
}

#[test]
fn a_merge_between_two_parks_alike_breaks_the_run() {
    // The park after it is a first park, whatever came before the merge.
    let w = host("merge-breaks");
    w.history_end(12, T0, PARKED);
    w.history_end(12, T0 + 60, MERGED);
    w.history_end(12, T0 + 120, PARKED);
    assert_eq!(repeats(&w), "[null,null,null]");
}

#[test]
fn a_park_of_a_provider_outage_neither_repeats_nor_starts_a_run() {
    // It has its own rule of retries.
    let w = host("provider");
    w.history_end(12, T0, PARKED);
    w.history_end(
        12,
        T0 + 60,
        r#"{"kind":"work","rc":10,"status":"failed","step":"develop","cap":"step-failed:develop","park_reason":"","failure_kind":"verify-failed","failure_cause":"exit-1","parked":true,"provider":true}"#,
    );
    w.history_end(12, T0 + 120, PARKED);
    assert_eq!(repeats(&w), "[null,null,1]");
}

#[test]
fn an_end_that_is_no_park_starts_no_run() {
    let w = host("not-a-park");
    w.history_end(12, T0, r#"{"kind":"work","rc":10,"status":"handoff"}"#);
    w.history_end(12, T0 + 60, PARKED);
    assert_eq!(repeats(&w), "[null,null]");
}

#[test]
fn a_park_of_another_issue_is_no_part_of_the_run() {
    let w = host("other-issue");
    w.history_end(12, T0, PARKED);
    w.history_end(11, T0 + 60, PARKED);
    w.history_end(12, T0 + 120, PARKED);
    assert_eq!(
        jq("[.runs[] | [.issue, .repeat]]", &run_json(&w, &[]).out),
        "[[12,null],[11,null],[12,1]]"
    );
}

#[test]
fn a_line_of_before_the_count_was_kept_says_nothing_of_a_repeat() {
    let w = host("no-count");
    w.history_end(
        12,
        T0,
        r#"{"kind":"work","rc":10,"status":"failed","step":"develop","parked":true}"#,
    );
    assert_eq!(
        jq(r#".runs[0] | has("repeat")"#, &run_json(&w, &[]).out),
        "false"
    );
    assert_eq!(
        history(&w, &[]).out,
        "1760700000-12-1  work  #12  failed  rc 10  step develop  parked"
    );
}

#[test]
fn the_count_counts_every_park_the_store_keeps_whatever_the_page() {
    // The run begins with a park the page does not hold: the two it holds
    // still count 1 and 2, as the whole history and cycle.parked count them.
    let w = host("page");
    for e in [T0, T0 + 60, T0 + 120] {
        w.history_end(12, e, PARKED);
    }
    let r = run_json(&w, &["--after", "1760700000-12-1"]);
    assert_eq!(jq("[.runs[] | .repeat]", &r.out), "[1,2]");
}

#[test]
fn a_repeat_is_a_whole_number_of_a_cycle_line_only() {
    let w = host("repeat-type");
    w.history_end(
        12,
        T0,
        r#"{"kind":"work","rc":10,"status":"failed","parked":true,"repeat":false}"#,
    );
    w.plan_end(
        11,
        T0 + 60,
        r#"{"kind":"plan","rc":0,"status":"finished","turn":1,"repeat":2}"#,
    );
    assert_eq!(
        raw(
            r#"[.runs[] | has("repeat")] | @tsv"#,
            &run_json(&w, &[]).out
        ),
        "false\tfalse"
    );
}

#[test]
fn nothing_to_list_an_empty_list_and_no_cursor() {
    let w = host("empty");
    let r = run_json(&w, &[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(r.out, r#"{"api":1,"runs":[],"next":null}"#);
}

#[test]
fn json_prints_one_object_and_nothing_else() {
    let w = host("one-object");
    w.history_end(12, T0, MERGED);
    let r = w.sh(&format!("{} --json 2>/dev/null", history_cmd()));
    let objects = serde_json::Deserializer::from_str(&r.out)
        .into_iter::<serde_json::Value>()
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|e| panic!("{e}: {}", r.out));
    assert_eq!(objects.len(), 1, "{}", r.out);
}

#[test]
fn without_json_one_line_each_oldest_first_and_the_plan_s_turn() {
    let w = host("text");
    w.plan_end(12, T0 - 60, PLANNED);
    w.history_end(
        12,
        T0,
        r#"{"kind":"work","rc":10,"status":"handoff","step":"review","parked":true,"pr":"https://github.com/acme/widget/pull/9"}"#,
    );
    let r = history(&w, &[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        r.out,
        "1760699940-12-1  plan  #12  finished  turn 1
1760700000-12-2  work  #12  handoff  rc 10  step review  parked  pr https://github.com/acme/widget/pull/9"
    );
}

#[test]
fn a_reader_that_goes_away_is_exit_141_with_nothing_on_standard_error_as_json_and_as_text() {
    let w = host("reader-gone");
    w.history_end(12, T0, MERGED);
    for flag in ["--json", ""] {
        let r = w.sh(&format!(
            r#"{{ sleep 0.3; {} {flag} 2>"$T/err"; }} | true; exit ${{PIPESTATUS[0]}}"#,
            history_cmd()
        ));
        assert_eq!(r.rc, 141, "{flag}: {}", r.out);
        assert_eq!(
            fs::read_to_string(w.t().join("err")).unwrap_or_default(),
            ""
        );
    }
}

#[test]
fn a_factory_with_no_history_of_its_own_says_nothing_and_exits_0() {
    let w = host("none");
    let r = history(&w, &[]);
    assert_eq!((r.rc, r.out.as_str()), (0, ""));
}

#[test]
fn an_argument_it_does_not_know_is_exit_64_and_reads_nothing() {
    let w = host("args");
    w.history_end(12, T0, MERGED);
    for arg in [
        "--json --json",
        "--after",
        "--after 12",
        "--after 1760700000-0",
        "--after 1760700000-",
        "--issue",
        "--issue 7 --issue 8",
        "--jsn",
        "--after x-1",
        "--after -1-1",
        "7",
    ] {
        let words: Vec<&str> = arg.split_whitespace().collect();
        let r = history(&w, &words);
        assert_eq!(r.rc, 64, "{arg}: {}", r.out);
        has(&r.out, "usage: tehas history");
    }
}

#[test]
fn a_word_that_is_not_text_is_a_bad_argument_too_and_help_among_others_is_one() {
    let w = host("not-text");
    let r = w.sh(&format!(r"{} --after $'\xff'", history_cmd()));
    assert_eq!(r.rc, 64, "{}", r.out);
    has(&r.out, "usage: tehas history");
    let r = history(&w, &["--json", "--help"]);
    assert_eq!(r.rc, 64, "{}", r.out);
    has(&r.out, "usage: tehas history");
}

#[test]
fn its_text_is_the_same_for_help_and_for_help_alone() {
    let w = host("help");
    let r = w.tehas("help", &["history"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "Usage: tehas history [--json] [--after CURSOR] [--issue N]",
    );
    let again = history(&w, &["--help"]);
    assert_eq!(again.rc, 0, "{}", again.out);
    assert_eq!(again.out, r.out);
}

// A cursor is what this command printed. One wider than anything it can print
// belongs to no listing, and an empty listing would be read as "nothing has
// ended" by a program that keeps it and asks again.
#[test]
fn a_cursor_this_command_could_never_print_is_a_bad_argument() {
    let w = host("wide");
    w.history_end(12, T0, MERGED);
    for cursor in [
        "9999999999999999-1",
        "123456789012345-1",
        "1760700000-1234567890",
        "-1",
        "1760700000-1-0",
        "1760700000-1-1234567890123456",
        "1760700000-1-",
        "1760700000-1-1-1",
        "17607000000000-1",
    ] {
        let r = run_json(&w, &["--after", cursor]);
        assert_eq!(r.rc, 64, "{cursor}: {}", r.out);
        has(&r.out, "usage: tehas history");
    }
    // The widest cursor it can print is taken: 10 digits of seconds, 9 of
    // issue and 15 of place.
    let widest = "9999999999-999999999-999999999999999";
    let r = run_json(&w, &["--after", widest]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        jq("{runs, next}", &r.out),
        format!(r#"{{"runs":[],"next":"{widest}"}}"#)
    );
}

#[test]
fn a_store_that_cannot_answer_prints_nothing_1_when_busy_78_when_it_cannot_be_used_or_is_not_there()
{
    let w = host("store");
    w.history_end(12, T0, MERGED);
    w.plan_end(12, T0, PLANNED);
    let quiet = format!("{} --json 2>/dev/null", history_cmd());
    w.store_answers(
        "history-show",
        1,
        "store: busy #0: the database is locked",
        0,
    );
    let r = w.sh(&quiet);
    assert_eq!((r.rc, r.out.as_str()), (1, ""));
    w.store_answers(
        "history-show",
        78,
        "store: corrupt: the file is not a database",
        0,
    );
    let r = w.sh(&quiet);
    assert_eq!((r.rc, r.out.as_str()), (78, ""));
    fs::remove_file(w.home.join("libexec/store")).unwrap();
    let r = history(&w, &[]);
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "ERROR: the state store is not installed (run install.sh)",
    );
    lacks(&r.out, "plan");
}

#[test]
fn without_settings_it_is_exit_78_and_prints_no_object() {
    let w = host("no-settings");
    w.no_instance();
    let r = w.sh(&format!("{} --json 2>/dev/null", history_cmd()));
    assert_eq!((r.rc, r.out.as_str()), (78, ""));
}

#[test]
fn a_bad_argument_is_64_whatever_the_settings_hold() {
    let w = host("bad-no-settings");
    w.no_instance();
    assert_eq!(run_json(&w, &["--issue", "0"]).rc, 64);
}

#[test]
fn without_a_repository_to_name_its_pull_requests_it_is_exit_78_and_reads_nothing() {
    let w = host("no-repo");
    w.history_end(
        12,
        T0,
        r#"{"kind":"work","rc":0,"status":"merged","pr":"https://github.com/acme/widget/pull/9"}"#,
    );
    w.instance(&[
        "TEHAS_GITHUB_APP_ID=1",
        "TEHAS_CONTROL_ISSUE=1",
        "TEHAS_ALLOWED_LABELERS=alice",
    ]);
    w.store_answers(
        "history-show",
        0,
        r#"{"api":1,"ok":true,"verb":"history-show","rows":[]}"#,
        0,
    );
    let r = run_json(&w, &[]);
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "TEHAS_REPO is not set");
    assert_eq!(w.store_answered(), 0);
}

#[test]
fn without_a_console_there_is_nothing_to_run_exit_78() {
    let w = host("no-console");
    let r = w.sh(&format!(
        r#"TEHAS_CONSOLE="$T/no-console" {} --json"#,
        history_cmd()
    ));
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        &format!("no console at {}", w.t().join("no-console").display()),
    );
}

/// Every path under `dir`, relative to it, in byte order.
fn tree(dir: &Path) -> Vec<String> {
    fn go(base: &Path, d: &Path, acc: &mut Vec<String>) {
        for e in fs::read_dir(d).unwrap() {
            let p = e.unwrap().path();
            acc.push(p.strip_prefix(base).unwrap().to_string_lossy().into_owned());
            if p.is_dir() && !p.is_symlink() {
                go(base, &p, acc);
            }
        }
    }
    let mut acc = vec![];
    go(dir, dir, &mut acc);
    acc.sort();
    acc
}

#[test]
fn history_writes_nothing() {
    let w = host("readonly");
    w.history_end(12, T0, MERGED);
    w.plan_end(12, T0, PLANNED);
    // The store's own files of its log (-wal, -shm) are SQLite's, not a write.
    let files = || -> Vec<String> {
        tree(&w.home)
            .into_iter()
            .filter(|f| !f.ends_with("tehas.db-wal") && !f.ends_with("tehas.db-shm"))
            .collect()
    };
    let before = files();
    let rows = || w.store_ok(&["history-show"])["rows"].to_string();
    let sum = rows();
    let r = run_json(&w, &[]);
    assert_eq!(jq(".runs | length", &r.out), "2", "{}", r.out);
    history(&w, &[]);
    assert_eq!(files(), before);
    assert_eq!(rows(), sum);
}

#[test]
fn reading_takes_no_lock_a_running_cycle_holds() {
    let w = host("lock");
    w.history_end(12, T0, MERGED);
    let lock = w.at().cycle_note(12, "lock");
    fs::write(&lock, "").unwrap();
    let r = w.sh(&format!(
        "flock '{}' {} --json",
        lock.display(),
        history_cmd()
    ));
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(jq(".runs | length", &r.out), "1");
}
