//! What tehas reads of pi-rukas's headless runner's --json result and NDJSON
//! progress: the runner verbs of tehas-cycle, run as the built program.

mod headless;

use headless::*;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use tehas_core::testdir::TestDir;

fn run(log: &Path, issue: &str) -> Out {
    exec(&["runner", "run", &log.to_string_lossy(), issue], None)
}

fn outcome(result: &Path, issue: &str) -> Out {
    exec(
        &["runner", "outcome", &result.to_string_lossy(), issue],
        None,
    )
}

/// `runner progress <log>`.
fn progress(log: &Path) -> String {
    exec(&["runner", "progress", &log.to_string_lossy()], None).out
}

/// `runner progress <log> <result> <issue>`.
fn progress_of(log: &Path, result: &Path, issue: &str) -> String {
    let (log, result) = (log.to_string_lossy(), result.to_string_lossy());
    exec(&["runner", "progress", &log, &result, issue], None).out
}

fn error_only(result: &Path) -> i32 {
    exec(&["runner", "error-only", &result.to_string_lossy()], None).code
}

/// The raw text of a key of a compact JSON object, as `jq -c .key` prints it
/// of the programs' compact output: in the order the program wrote it. `null`
/// when the key is not there.
fn field(json: &str, key: &str) -> String {
    let b = json.as_bytes();
    let want = format!("\"{key}\":");
    let (mut depth, mut i, mut in_str) = (0usize, 0usize, false);
    while i < b.len() {
        let c = b[i];
        if in_str {
            match c {
                b'\\' => i += 1,
                b'"' => in_str = false,
                _ => {}
            }
            i += 1;
            continue;
        }
        match c {
            b'{' | b'[' => depth += 1,
            b'}' | b']' => depth -= 1,
            b'"' if depth == 1 && json[i..].starts_with(&want) => {
                let start = i + want.len();
                return json[start..value_end(json, start)].to_string();
            }
            b'"' => in_str = true,
            _ => {}
        }
        i += 1;
    }
    "null".into()
}

/// Where the JSON value that starts at `start` ends.
fn value_end(json: &str, start: usize) -> usize {
    let b = json.as_bytes();
    let (mut depth, mut i, mut in_str) = (0usize, start, false);
    while i < b.len() {
        let c = b[i];
        if in_str {
            match c {
                b'\\' => i += 1,
                b'"' => {
                    in_str = false;
                    if depth == 0 {
                        return i + 1;
                    }
                }
                _ => {}
            }
            i += 1;
            continue;
        }
        match c {
            b'"' => in_str = true,
            b'{' | b'[' => depth += 1,
            b'}' | b']' if depth == 0 => return i,
            b'}' | b']' => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            b',' if depth == 0 => return i,
            _ => {}
        }
        i += 1;
    }
    b.len()
}

/// A string key as `jq -r .key` prints it.
fn text(json: &str, key: &str) -> String {
    let raw = field(json, key);
    serde_json::from_str::<String>(&raw).unwrap_or(raw)
}

/// The fields as `jq -c '[.a, .b]'` prints them.
fn fields(json: &str, keys: &[&str]) -> String {
    let all: Vec<String> = keys.iter().map(|k| field(json, k)).collect();
    format!("[{}]", all.join(","))
}

/// v0.14.0's result with each edit made, written to `r.json` in `t`.
fn result_with(t: &TestDir, edits: &[(&str, Option<&str>)]) -> PathBuf {
    let mut r = read(sx().join("result.json"));
    for (pointer, to) in edits {
        r = edit(&r, pointer, *to);
    }
    let p = t.join("r.json");
    fs::write(&p, r).unwrap();
    p
}

/// The log of a run whose runner died: one line per stable step-started line,
/// in the order the runner wrote them.
fn steps_log(t: &TestDir, name: &str, steps: &[&str]) -> PathBuf {
    let f = t.join(format!("{name}.jsonl"));
    let lines: String = steps
        .iter()
        .map(|s| format!("{{\"v\":1,\"kind\":\"step-started\",\"issue\":7,\"event\":{{\"step\":\"{s}\"}}}}\n"))
        .collect();
    fs::write(&f, lines).unwrap();
    f
}

/// The three lines `runner run` answers of a run, each ended by `|`. The log's
/// first step is the issue's number, as the shell helper wrote it.
fn run_of(issue: &str, steps: &[&str]) -> String {
    let t = scratch();
    let mut all = vec![issue];
    all.extend_from_slice(steps);
    let f = steps_log(&t, "run", &all);
    run(&f, issue).raw.replace('\n', "|")
}

fn lines(t: &TestDir, name: &str, lines: &[&str]) -> PathBuf {
    let f = t.join(name);
    fs::write(
        &f,
        lines.iter().map(|l| format!("{l}\n")).collect::<String>(),
    )
    .unwrap();
    f
}

fn progress_jsonl(dir: &Path) -> PathBuf {
    dir.join("progress.jsonl")
}

#[test]
fn the_usage_per_role_and_the_failure_of_a_schema_5_run_come_from_its_result() {
    let t = scratch();
    let r = result_with(
        &t,
        &[(
            "/issues/0/usage/byRole",
            Some(r#"{"ops":{"input":0,"output":0,"cacheRead":0,"turns":0,"dispatches":1}}"#),
        )],
    );
    let p = progress_of(&progress_jsonl(&sx()), &r, "7");
    assert_eq!(
        field(&p, "roles"),
        r#"[{"role":"ops","dispatches":1,"turns":0,"input":0,"output":0,"cacheRead":0}]"#
    );
    assert_eq!(
        field(&p, "failure"),
        r#"{"kind":"cap","step":"explore","role":"","cause":""}"#
    );
    assert_eq!(text(&p, "reason"), "explore-bodies-empty");
    assert_eq!(
        field(&p, "from"),
        r#"{"roles":"result","failure":"result"}"#
    );
}

#[test]
fn a_provider_failure_gives_its_kind_and_the_providers_own_message() {
    let t = scratch();
    let r = result_with(
        &t,
        &[
            (
                "/issues/0/failure",
                Some(
                    r#"{"kind":"provider","step":"develop","role":"developer","cause":"rate-limited:429",
  "message":"The provider refused","providerMessage":"429 Too Many Requests"}"#,
                ),
            ),
            (
                "/issues/0/usage/byRole",
                Some(
                    r#"{"developer":{"input":5,"output":2,"cacheRead":1,"turns":3,"dispatches":2},"a b":{"dispatches":"x"},"bad":7}"#,
                ),
            ),
        ],
    );
    let p = progress_of(&progress_jsonl(&sx()), &r, "7");
    assert_eq!(
        field(&p, "failure"),
        r#"{"kind":"provider","step":"develop","role":"developer","cause":"rate-limited:429"}"#
    );
    assert_eq!(text(&p, "reason"), "429 Too Many Requests");
    assert_eq!(
        field(&p, "roles"),
        r#"[{"role":"developer","dispatches":2,"turns":3,"input":5,"output":2,"cacheRead":1},{"role":"ab","dispatches":0,"turns":0,"input":0,"output":0,"cacheRead":0}]"#
    );
}

#[test]
fn a_result_with_no_failure_has_none() {
    let t = scratch();
    let r = result_with(&t, &[("/issues/0/failure", None)]);
    let p = progress_of(&progress_jsonl(&sx()), &r, "7");
    let said = format!(
        "[{},{},{}]",
        field(&p, "failure"),
        field(&p, "reason"),
        field(&field(&p, "from"), "failure")
    );
    assert_eq!(said, r#"[null,"","result"]"#);
}

#[test]
fn a_run_whose_runner_wrote_no_result_gives_the_progress_lines() {
    let p = progress(&progress_jsonl(&nx()));
    assert_eq!(
        fields(&p, &["roles", "failure", "from"]),
        r#"[[],null,{"roles":"progress","failure":"progress"}]"#
    );
    let reason = text(&p, "reason");
    assert!(reason.contains("cancelled (abort signal)"), "{reason}");
}

#[test]
fn no_result_gives_the_progress_lines_another_issue_a_schema_3_result_or_a_link_is_a_bad_result() {
    let t = scratch();
    let log = progress_jsonl(&nx());
    let p = progress_of(&log, &t.join("missing.json"), "7");
    assert_eq!(
        field(&p, "from"),
        r#"{"roles":"progress","failure":"progress"}"#
    );
    let link = t.join("link.json");
    symlink(sx().join("result.json"), &link).unwrap();
    for (result, n) in [
        (sx().join("result.json"), "8"),
        (nx().join("result.json"), "7"),
        (link, "7"),
    ] {
        let p = progress_of(&log, &result, n);
        assert_eq!(
            field(&p, "from"),
            r#"{"roles":"bad-result","failure":"bad-result"}"#,
            "{}",
            result.display()
        );
    }
    let null = Path::new("/dev/null");
    let p = progress_of(null, null, "7");
    assert_eq!(
        fields(&p, &["roles", "reason", "failure"]),
        r#"[[],"",null]"#
    );
}

#[test]
fn the_runs_last_step_its_pushed_branch_and_whether_its_log_was_read_whole() {
    // The step of the last step-started line, and the step right after
    // commit-pr as the branch, which is this issue's own and so is said.
    assert_eq!(
        run_of(
            "7",
            &[
                "explore",
                "plan",
                "develop",
                "commit-pr",
                "feature/issue-7-checks-instance"
            ]
        ),
        "feature/issue-7-checks-instance|feature/issue-7-checks-instance|whole|"
    );
    // No commit-pr at all: the step of the run stands, and GitHub is asked
    // nothing.
    assert_eq!(
        run_of("7", &["explore", "plan", "develop"]),
        "develop||whole|"
    );
    // A run whose steps stop at commit-pr pushed nothing, so its last step is
    // no branch of a push.
    assert_eq!(
        run_of("7", &["explore", "develop", "commit-pr"]),
        "commit-pr||whole|"
    );
}

#[test]
fn the_branch_of_the_last_commitpr_so_a_run_that_pushed_twice_ends_on_its_last_push() {
    assert_eq!(
        run_of(
            "7",
            &[
                "commit-pr",
                "feature/issue-7-first",
                "develop",
                "commit-pr",
                "feature/issue-7-second"
            ]
        ),
        "feature/issue-7-second|feature/issue-7-second|whole|"
    );
}

#[test]
fn a_branch_of_another_issue_one_of_no_slug_and_a_step_that_is_no_branch() {
    assert_eq!(
        run_of("7", &["commit-pr", "feature/issue-8-other"]),
        "feature/issue-8-other||whole|"
    );
    assert_eq!(run_of("7", &["commit-pr", "develop"]), "develop||whole|");
}

#[test]
fn a_step_name_over_the_bound_of_100_characters_is_no_step_of_a_run() {
    // The bound is the one tehas-cycle runner holds, and it is wider than the
    // grammar of a branch (runner::branch_of) in one way and narrower in
    // another. A name of 101 characters is no step at all, whatever its shape,
    // so the run says only its commit-pr. A name of the most characters a step
    // may be, but with a slug past 80, is a step and no branch of this issue,
    // so GitHub is asked about nothing. A name of the widest branch the grammar
    // takes is both at once.
    assert_eq!(
        run_of("7", &["commit-pr", &"x".repeat(101)]),
        "commit-pr||whole|"
    );
    let z = "z".repeat(100);
    assert_eq!(run_of("7", &["commit-pr", &z]), format!("{z}||whole|"));
    let wide = format!("feature/issue-7-{}", "y".repeat(84));
    assert_eq!(
        run_of("7", &["commit-pr", &wide]),
        format!("{wide}||whole|")
    );
    let widest = format!("feature/issue-7-{}", "y".repeat(80));
    assert_eq!(
        run_of("7", &["commit-pr", &widest]),
        format!("{widest}|{widest}|whole|")
    );
}

#[test]
fn an_empty_log_an_unreadable_one_and_no_issue_number_say_nothing() {
    let t = scratch();
    let empty = t.join("empty.jsonl");
    fs::write(&empty, "").unwrap();
    assert_eq!(run(&empty, "7").raw.replace('\n', "|"), "||whole|");
    // Not a number: no issue's branch is asked about, and nothing of the run
    // is said for it. A missing file is answered the same way, and said of in
    // the log.
    let o = run(&empty, "abc");
    assert_eq!((o.out.as_str(), o.code), ("", 1));
    let o = run(&t.join("nothing.jsonl"), "7");
    assert_eq!((o.out.as_str(), o.code), ("", 1));
    let link = t.join("run-link.jsonl");
    symlink(progress_jsonl(&fx()), &link).unwrap();
    let o = run(&link, "7");
    assert_eq!((o.out.as_str(), o.code), ("", 1));
}

#[test]
fn a_log_bigger_than_the_200_kb_tehas_reads_says_it_was_read_from_its_end() {
    let t = scratch();
    // As Python's json.dumps writes the lines, with its separators.
    let mut log = format!(
        "{{\"v\": 1, \"kind\": \"log\", \"message\": \"{}\"}}\n",
        "y".repeat(100)
    )
    .repeat(2500);
    for s in ["commit-pr", "feature/issue-7-late"] {
        log += &format!(
            "{{\"v\": 1, \"kind\": \"step-started\", \"event\": {{\"step\": \"{s}\"}}}}\n"
        );
    }
    let big = t.join("big.jsonl");
    fs::write(&big, log).unwrap();
    assert_eq!(
        run(&big, "7").raw.replace('\n', "|"),
        "feature/issue-7-late|feature/issue-7-late|truncated|"
    );
}

#[test]
fn the_progress_of_the_fixture_has_no_completed_dispatch_and_the_reason_of_its_one_failed_dispatch()
{
    let p = progress(&progress_jsonl(&fx()));
    assert_eq!(field(&p, "roles"), "[]");
    let reason = text(&p, "reason");
    assert!(reason.contains("cancelled (abort signal)"), "{reason}");
}

#[test]
fn a_caps_evidence_is_the_reason_before_a_failed_dispatchs() {
    let t = scratch();
    let p = lines(
        &t,
        "p.jsonl",
        &[
            r#"{"v":1,"kind":"dispatch-failed","unstable":true,"event":{"errorTail":"exit 1"}}"#,
            r#"{"v":1,"kind":"cap-hit","event":{"cap":"x","evidence":"git says no"}}"#,
        ],
    );
    assert_eq!(text(&progress(&p), "reason"), "git says no");
    let none = progress(Path::new("/dev/null"));
    assert_eq!(
        fields(&none, &["roles", "reason", "reviewHeld"]),
        r#"[[],"",false]"#
    );
}

#[test]
fn a_stable_caphit_that_ends_the_review_on_its_round_cap_and_goes_on_to_ci_holds_the_review() {
    let t = scratch();
    assert_eq!(
        field(&progress(&progress_jsonl(&fx())), "reviewHeld"),
        "false"
    );
    let p = lines(
        &t,
        "p.jsonl",
        &[r#"{"v":1,"kind":"cap-hit","key":"7:1:a","event":{"cap":"round-cap","nextStep":"ci"}}"#],
    );
    assert_eq!(field(&progress(&p), "reviewHeld"), "true");
    let p = lines(
        &t,
        "p.jsonl",
        &[
            r#"{"v":1,"kind":"cap-hit","key":"7:1:a","event":{"cap":"round-cap","nextStep":"handoff"}}"#,
            r#"{"v":1,"kind":"cap-hit","unstable":true,"event":{"cap":"round-cap","nextStep":"ci"}}"#,
        ],
    );
    assert_eq!(field(&progress(&p), "reviewHeld"), "false");
}

#[test]
fn the_outcome_carries_the_handoffs_body_as_rendered_and_an_intent_parks_reason_as_a_word() {
    let o = outcome(&sx().join("result.json"), "7").out;
    let body = text(&o, "handoffBody");
    assert!(
        body.contains("<!-- pi-rukas:handoff issue=7 cap=explore-bodies-empty -->\n## "),
        "{body}"
    );
    let t = scratch();
    let r = result_with(
        &t,
        &[
            ("/issues/0/parkReason", Some(r#""too-large\n<b>""#)),
            ("/issues/0/handoff/body", Some("7")),
        ],
    );
    let o = outcome(&r, "7").out;
    assert_eq!(
        fields(&o, &["parkReason", "handoffBody"]),
        r#"["too-largeb",""]"#
    );
    let long = serde_json::to_string(&"x".repeat(40000)).unwrap();
    let r = result_with(&t, &[("/issues/0/handoff/body", Some(&long))]);
    let o = outcome(&r, "7").out;
    assert_eq!(text(&o, "handoffBody").chars().count(), 30000);
}

#[test]
fn the_outcome_of_the_fixture_is_its_handoff() {
    let o = outcome(&sx().join("result.json"), "7").out;
    let mut said: Vec<String> = [
        "status",
        "cap",
        "failedStep",
        "lastStep",
        "exitCode",
        "resumable",
        "buildVersion",
    ]
    .iter()
    .map(|k| field(&o, k))
    .collect();
    said.push(field(&field(&o, "usage"), "dispatches"));
    let said = format!("[{}]", said.join(","));
    assert_eq!(
        said,
        r#"["handoff","explore-bodies-empty","explore","handoff",10,false,"0.14.0",0]"#
    );
    assert_eq!(
        text(&o, "buildCommit"),
        "c5ddc0ab9de62edeaac948263d7f8a8f5b123066"
    );
}

#[test]
fn another_issue_another_schema_or_no_file_is_no_outcome() {
    assert_eq!(outcome(&sx().join("result.json"), "8").code, 4);
    let t = scratch();
    for v in ["2", "3", "4", "6"] {
        let r = result_with(&t, &[("/schemaVersion", Some(v))]);
        assert_eq!(outcome(&r, "7").out, "", "schema {v}");
    }
    assert_eq!(outcome(&fx().join("result.json"), "7").out, "");
    assert_eq!(outcome(&nx().join("result.json"), "7").out, "");
    assert_eq!(outcome(&t.join("missing.json"), "7").out, "");
}

#[test]
fn an_exit_code_that_is_missing_or_not_a_number_is_null_not_0() {
    let t = scratch();
    let r = result_with(&t, &[("/issues/0/exitCode", None)]);
    assert_eq!(field(&outcome(&r, "7").out, "exitCode"), "null");
    let r = result_with(&t, &[("/issues/0/exitCode", Some(r#""0""#))]);
    assert_eq!(field(&outcome(&r, "7").out, "exitCode"), "null");
}

#[test]
fn a_schema_5_result_with_no_issue_entry_is_error_only_and_one_with_entries_is_not() {
    let t = scratch();
    let r = t.join("r.json");
    fs::write(
        &r,
        "{\"schemaVersion\":5,\"issues\":[],\"error\":{\"code\":\"refused\"}}\n",
    )
    .unwrap();
    assert_eq!(error_only(&r), 0);
    fs::write(
        &r,
        "{\"schemaVersion\":5,\"error\":{\"code\":\"refused\"}}\n",
    )
    .unwrap();
    assert_eq!(error_only(&r), 0);
    assert_eq!(error_only(&sx().join("result.json")), 1);
    for bad in [
        r#"{"schemaVersion":3,"issues":[]}"#,
        r#"{"schemaVersion":5,"issues":{}}"#,
        "[]",
        "not json",
    ] {
        fs::write(&r, format!("{bad}\n")).unwrap();
        assert_eq!(error_only(&r), 1, "{bad}");
    }
}

#[test]
fn the_outcomes_text_is_plain_characters_and_its_numbers_are_numbers() {
    let t = scratch();
    let r = result_with(
        &t,
        &[
            ("/issues/0/status", Some(r#""handoff\n<!-- x -->""#)),
            ("/issues/0/branch", Some(r#""a b;`c`""#)),
            ("/issues/0/usage/input", Some(r#""9; rm""#)),
        ],
    );
    let o = outcome(&r, "7").out;
    let said = format!(
        "[{},{},{}]",
        field(&o, "status"),
        field(&o, "branch"),
        field(&field(&o, "usage"), "input")
    );
    assert_eq!(said, r#"["handoff--x--","abc",0]"#);
}
