//! The cases of test/cycle_*_test.sh as fixtures: what each cycle left, as
//! the bash cycle read it, and what the bash then did. A suite is one file in
//! tests/fixtures/, made from a run of that test file against the bash cycle
//! with each report traced (the pull request of #256 part 1 says how).
//!
//! Each case gives the facts, the GitHub answers the bash had, how its label
//! changes went (`met.relabel`), whether its park went on record
//! (`met.recorded`) and whether its question was posted (`met.posted`). The
//! report must log the same warnings, end the record with the same arguments,
//! park with the same ones, say the same `done` line, and make the same writes
//! to GitHub in the same order.

use serde_json::Value;
use std::fs;
use tehas_cycle::report::{last_attempts, report, Action, Given};
use tehas_cycle::Facts;

const BOXES: &str = "The boxes above cannot be used: the label `tehas:question` could not be added (run `tehas labels` on the host). To retry, apply `tehas:ready`.";

/// The writes of a report under what the bash met: its label changes, each
/// counted once however often it was tried, its park on record or not, and
/// its question posted or not.
fn writes(
    actions: &[Action],
    issue: u64,
    repo: &str,
    mut met: Vec<bool>,
    recorded: bool,
    posted: bool,
) -> Vec<(String, String)> {
    let mut out = vec![];
    let mut stuck = true;
    met.reverse();
    let mut relabel = |out: &mut Vec<(String, String)>, remove: &[&str], add: &[&str]| {
        let mut a: Vec<String> = vec![];
        for r in remove {
            a.extend(["--remove-label".into(), r.to_string()]);
        }
        for x in add {
            a.extend(["--add-label".into(), x.to_string()]);
        }
        out.push(("relabel".into(), a.join(" ")));
        met.pop().unwrap_or(true)
    };
    for a in actions {
        match a {
            Action::Relabel { remove, add } => {
                let r: Vec<&str> = remove.iter().map(String::as_str).collect();
                let x: Vec<&str> = add.iter().map(String::as_str).collect();
                if relabel(&mut out, &r, &x) {
                    stuck = false;
                }
            }
            Action::Comment { ok, stuck: s } => {
                out.push(("comment".into(), if stuck { s } else { ok }.clone()))
            }
            Action::Park(p) => {
                if relabel(&mut out, &["tehas:running"], &["needs-human-attention"]) {
                    stuck = false;
                }
                if p.edit.is_some() {
                    out.push(("body_edit".into(), format!("repos/{repo}/issues/{issue}")));
                }
                if recorded {
                    out.push(("question".into(), p.question(issue, stuck).1));
                }
                if recorded && posted {
                    if !relabel(&mut out, &[], &["tehas:question"]) {
                        out.push(("comment".into(), BOXES.into()));
                    }
                } else {
                    out.push(("comment".into(), p.comment(stuck, recorded)));
                }
            }
            Action::PrComment { pr, body } => {
                out.push(("pr_comment".into(), format!("{pr}\n{body}")))
            }
            Action::Tidy { .. } => {}
            Action::RefreshOff { pr } => out.push((
                "refresh_off".into(),
                format!("repos/{repo}/issues/{pr}/labels/tehas:refreshing"),
            )),
        }
    }
    out
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .map(|x| x.as_str().unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn the_reports_of_the_bash_cases() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");
    let mut names: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    names.sort();
    let mut failed = vec![];
    let mut count = 0;
    let mut arms = std::collections::BTreeSet::new();
    for path in names {
        let suite = path.file_stem().unwrap().to_string_lossy().to_string();
        let cases: Vec<Value> = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        for c in cases {
            count += 1;
            let name = format!("{suite}: {}", c["case"].as_str().unwrap_or("?"));
            let facts = Facts::from_json(&c["facts"]).unwrap();
            let mut gh = Given(c["facts"]["github"].clone());
            let mut r = report(&facts, &mut gh);
            last_attempts(&mut r, &facts, &mut gh);
            arms.insert(r.arm);
            let e = &c["expect"];
            let mut diff = vec![];
            if let Some(done) = e["done"].as_str() {
                if r.done != done {
                    diff.push(format!("done: {} != {done}", r.done));
                }
            }
            if r.end != strings(&e["end"]) {
                diff.push(format!("end: {:?} != {:?}", r.end, strings(&e["end"])));
            }
            let park = r.actions.iter().find_map(|a| match a {
                Action::Park(p) => Some(p.args.clone()),
                _ => None,
            });
            let want = (!e["park"].is_null()).then(|| strings(&e["park"]));
            if park != want {
                diff.push(format!("park: {park:?} != {want:?}"));
            }
            if r.log != strings(&e["log"]) {
                diff.push(format!("log: {:?} != {:?}", r.log, strings(&e["log"])));
            }
            let met: Vec<bool> = strings(&c["met"]["relabel"])
                .iter()
                .map(|s| s == "ok")
                .collect();
            let got = writes(
                &r.actions,
                facts.issue,
                &facts.repo,
                met,
                c["met"]["recorded"].as_bool().unwrap_or(false),
                c["met"]["posted"].as_bool().unwrap_or(false),
            );
            let want: Vec<(String, String)> = e["writes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|w| (w[0].as_str().unwrap().into(), w[1].as_str().unwrap().into()))
                .collect();
            if got != want {
                diff.push(format!("writes:\n  rust {got:?}\n  bash {want:?}"));
            }
            if !diff.is_empty() {
                failed.push(format!("{name}\n  {}", diff.join("\n  ")));
            }
        }
    }
    assert!(count > 0, "no fixtures in {dir}");
    // Every way a report can end is traced from the bash.
    for arm in [
        "merged",
        "merged-outside",
        "prs-unread",
        "merged-claimed",
        "awaiting-merge",
        "handoff-parked",
        "issue-closed",
        "pr-closed",
        "died-with-pr",
        "parked",
        "never-ran",
        "provider",
    ] {
        assert!(arms.contains(arm), "no fixture of the arm {arm}");
    }
    assert!(
        failed.is_empty(),
        "{} of {count} cases differ:\n{}",
        failed.len(),
        failed.join("\n")
    );
}

/// The report of a case of tests/fixtures/park.json that exited 4, with `rc`
/// in its place, with the driver lean on its approval and with or without the
/// note tehas work leaves when it refuses lean.
fn refused(rc: i64, note: bool) -> (Vec<String>, String) {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/park.json");
    let cases: Vec<Value> = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    let mut c = cases.into_iter().find(|c| c["facts"]["rc"] == 4).unwrap();
    c["facts"]["rc"] = rc.into();
    c["facts"]["approval"]["current"]["input"] = serde_json::json!({"driver": "lean"});
    if note {
        c["facts"]["files"]["lean-refused"] = "".into();
    }
    let facts = Facts::from_json(&c["facts"]).unwrap();
    let r = report(&facts, &mut Given(c["facts"]["github"].clone()));
    let said = r.actions.iter().find_map(|a| match a {
        Action::Park(p) => Some(format!("{p:?}")),
        _ => None,
    });
    (r.end, said.unwrap())
}

#[test]
fn a_lean_start_refused_with_78_is_parked_as_one_that_never_ran() {
    let (end, said) = refused(78, true);
    assert!(end.contains(&"--never-ran".to_string()), "{end:?}");
    assert!(
        said.contains("the sandbox image cannot run it as this start asks"),
        "{said}"
    );
    assert!(said.contains("take `tehas:lean` off"), "{said}");
}

/// The park of a refused lean start is on record as one, so the poll can hold
/// its Retry boxes while `tehas:lean` is on, and its question says the label
/// comes off before a box is ticked.
#[test]
fn the_park_of_a_refused_lean_start_puts_the_label_above_its_boxes() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/park.json");
    let cases: Vec<Value> = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    let parks = |rc: i64, note: bool| {
        let mut c = cases
            .iter()
            .find(|c| c["facts"]["rc"] == 4)
            .unwrap()
            .clone();
        c["facts"]["rc"] = rc.into();
        c["facts"]["approval"]["current"]["input"] = serde_json::json!({"driver": "lean"});
        if note {
            c["facts"]["files"]["lean-refused"] = "".into();
        }
        let facts = Facts::from_json(&c["facts"]).unwrap();
        let r = report(&facts, &mut Given(c["facts"]["github"].clone()));
        r.actions
            .into_iter()
            .find_map(|a| match a {
                Action::Park(p) => Some(*p),
                _ => None,
            })
            .unwrap()
    };
    let lead = "Take `tehas:lean` off this issue, then tick a box.";
    let p = parks(78, true);
    let reason = p.args.iter().position(|a| a == "--reason").unwrap();
    assert_eq!(p.args[reason + 1], "lean-refused", "{:?}", p.args);
    let (_, body) = p.question(7, false);
    let at = body.find(lead).unwrap_or_else(|| panic!("{body}"));
    let boxes = body.find("- [ ] Retry with the issue text only").unwrap();
    assert!(at < boxes, "{body}");
    for p in [parks(4, false), parks(78, false)] {
        assert!(
            !p.args.contains(&"lean-refused".to_string()),
            "{:?}",
            p.args
        );
        assert!(!p.question(7, false).1.contains(lead));
    }
}

#[test]
fn another_78_on_a_lean_start_is_a_cycle_that_stopped() {
    let (end, said) = refused(78, false);
    assert!(!end.contains(&"--never-ran".to_string()), "{end:?}");
    assert!(said.contains("stopped without a pull request"), "{said}");
    assert!(!said.contains("lean"), "{said}");
}

/// The park of the classic 78 of tests/fixtures/park.json, with the note
/// tehas work leaves for a refresh whose pull request conflicts, if any.
fn refresh_parked(note: Option<&str>) -> (Vec<String>, tehas_cycle::report::Park) {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/park.json");
    let cases: Vec<Value> = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    let mut c = cases
        .into_iter()
        .find(|c| {
            c["case"]
                == "a 78 on a classic start stays a stopped cycle, with its approval on record"
        })
        .unwrap();
    if let Some(n) = note {
        c["facts"]["files"]["refresh-conflict"] = n.into();
    }
    let facts = Facts::from_json(&c["facts"]).unwrap();
    let r = report(&facts, &mut Given(c["facts"]["github"].clone()));
    let park = r
        .actions
        .into_iter()
        .find_map(|a| match a {
            Action::Park(p) => Some(*p),
            _ => None,
        })
        .unwrap();
    (r.end, park)
}

/// #1727: a refresh that conflicts on an image that cannot finish one is a
/// stopped refresh, its approval on record so no refresh of it starts again,
/// and its park names the files.
#[test]
fn a_refresh_that_conflicts_is_parked_naming_its_files() {
    let note = r#"{"pr":1453,"branch":"main","paths":["console/dispatch/src/text.rs"],"more":0}"#;
    let (end, p) = refresh_parked(Some(note));
    assert!(!end.contains(&"--never-ran".to_string()), "{end:?}");
    assert!(end.contains(&"--parked".to_string()), "{end:?}");
    let reason = p.args.iter().position(|a| a == "--reason").unwrap();
    assert_eq!(p.args[reason + 1], "refresh-conflict", "{:?}", p.args);
    let (_, body) = p.question(7, false);
    assert!(
        body.contains("Tehas ran no refresh: pull request #1453 conflicts with `main` in `console/dispatch/src/text.rs`."),
        "{body}"
    );
    assert!(!body.contains("stopped without a pull request"), "{body}");
    // A note that is not one, or none, is the 78 it was.
    for note in [None, Some("not json")] {
        let (_, p) = refresh_parked(note);
        assert!(
            !p.args.contains(&"refresh-conflict".to_string()),
            "{:?}",
            p.args
        );
        assert!(p
            .question(7, false)
            .1
            .contains("stopped without a pull request"));
    }
}

/// The park of the classic 78 of tests/fixtures/park.json, with the note of
/// why tehas work's check refused the start, if any, and the driver lean on its
/// approval with the note of a refused lean start, or not.
fn check_refused(note: Option<&str>, lean: bool) -> (Vec<String>, String) {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/park.json");
    let cases: Vec<Value> = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    let mut c = if lean {
        let mut c = cases.into_iter().find(|c| c["facts"]["rc"] == 4).unwrap();
        c["facts"]["rc"] = 78.into();
        c["facts"]["approval"]["current"]["input"] = serde_json::json!({"driver": "lean"});
        c["facts"]["files"]["lean-refused"] = "".into();
        c
    } else {
        cases
            .into_iter()
            .find(|c| {
                c["case"]
                    == "a 78 on a classic start stays a stopped cycle, with its approval on record"
            })
            .unwrap()
    };
    if let Some(n) = note {
        c["facts"]["files"]["refused"] = n.into();
    }
    let facts = Facts::from_json(&c["facts"]).unwrap();
    let r = report(&facts, &mut Given(c["facts"]["github"].clone()));
    let p = r
        .actions
        .into_iter()
        .find_map(|a| match a {
            Action::Park(p) => Some(*p),
            _ => None,
        })
        .unwrap();
    (p.args.clone(), p.question(7, false).1)
}

/// A start the check refused says why in its park, which the run log on the
/// host alone said before: each of its three causes, on classic and on lean.
#[test]
fn a_start_the_check_refused_names_its_cause_in_its_park() {
    let (args, none) = check_refused(None, false);
    assert!(!none.contains("refused this start"), "{none}");
    for (note, words) in [
        (
            "caps-unreadable 127\n",
            "gave no `--capabilities` answer that tehas could read (exit 127).",
        ),
        (
            "caps-lacking\nthe input PI_ENSEMBLE_SUBAGENT_MODEL\n",
            "does not support what this cycle is given: `the input PI_ENSEMBLE_SUBAGENT_MODEL`.",
        ),
        ("tree-not-ours\n", "exists and was not made by tehas"),
    ] {
        let (a, body) = check_refused(Some(note), false);
        assert_eq!(a, args, "the park's reasons stay as they were");
        assert!(body.contains("stopped without a pull request"), "{body}");
        assert!(
            body.contains("Tehas work refused this start before anything ran."),
            "{body}"
        );
        assert!(body.contains(words), "{note:?}: {body}");
    }
    // A note that is not one says nothing.
    let (_, body) = check_refused(Some("caps-unreadable x\n"), false);
    assert!(!body.contains("refused this start"), "{body}");

    let (_, lean) = check_refused(None, true);
    assert!(
        lean.contains("The log names what the image lacks."),
        "{lean}"
    );
    let (args, lean) = check_refused(Some("caps-lacking\nthe feature leanHandoffHost\n"), true);
    let reason = args.iter().position(|a| a == "--reason").unwrap();
    assert_eq!(args[reason + 1], "lean-refused", "{args:?}");
    assert!(
        lean.contains("does not support what this cycle is given: `the feature leanHandoffHost`. To run it on classic"),
        "{lean}"
    );
    assert!(!lean.contains("The log names"), "{lean}");
}

/// All cases of a suite whose question or comment says these words.
fn said(suite: &str, words: &str) -> Vec<Value> {
    let path = format!("{}/tests/fixtures/{suite}.json", env!("CARGO_MANIFEST_DIR"));
    let cases: Vec<Value> = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    cases
        .into_iter()
        .filter(|c| c["expect"]["writes"].to_string().contains(words))
        .collect()
}

#[test]
fn a_repeat_park_says_so_in_the_host_s_words() {
    let cases = said("park", "This park repeats the one before it");
    assert!(!cases.is_empty());
    for c in cases {
        assert!(c["facts"]["history"].is_object(), "{}", c["case"]);
    }
}

#[test]
fn a_history_that_cannot_be_read_says_nothing_of_a_repeat() {
    let mut c = said("park", "This park repeats the one before it").remove(0);
    c["facts"]["history"] = serde_json::json!({"error": "busy"});
    let facts = Facts::from_json(&c["facts"]).unwrap();
    let r = report(&facts, &mut Given(c["facts"]["github"].clone()));
    let said = format!("{:?}", r.actions);
    assert!(!said.contains("repeats the one before it"), "{said}");
}

#[test]
fn a_second_develop_timeout_on_the_same_text_offers_no_retry() {
    let cases = said("park", "It ran out of time in develop twice");
    assert!(!cases.is_empty());
    for c in cases {
        let w = c["expect"]["writes"].to_string();
        assert!(!w.contains("Retry from a clean start"), "{}", c["case"]);
        assert!(w.contains("Leave it parked"), "{}", c["case"]);
    }
}

#[test]
fn the_note_of_a_develop_timeout_is_written_after_the_park_is_on_record() {
    let cases: Vec<Value> = {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/park.json");
        let all: Vec<Value> = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        all.into_iter()
            .filter(|c| c["expect"]["writes"].to_string().contains("body_edit"))
            .collect()
    };
    assert!(!cases.is_empty());
    let facts = Facts::from_json(&cases[0]["facts"]).unwrap();
    let mut gh = Given(cases[0]["facts"]["github"].clone());
    let mut r = report(&facts, &mut gh);
    let edits = |r: &tehas_cycle::Report| {
        r.actions.iter().any(|a| match a {
            Action::Park(p) => p.edit.is_some(),
            _ => false,
        })
    };
    assert!(!edits(&r), "the decision reads no issue");
    last_attempts(&mut r, &facts, &mut gh);
    assert!(edits(&r));
}

#[test]
fn a_stop_for_being_quiet_is_a_park_with_its_own_reason() {
    let cases = said(
        "record",
        "nothing was heard from it for longer than its quiet bound",
    );
    assert!(!cases.is_empty());
    for c in cases {
        assert!(c["expect"]["end"]
            .to_string()
            .contains(r#""--park-reason","quiet""#));
    }
}

#[test]
fn a_reason_that_reads_like_an_outage_is_no_outage_without_its_kind() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/park.json");
    let cases: Vec<Value> = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    let mut c = cases
        .into_iter()
        .find(|c| {
            c["expect"]["writes"]
                .to_string()
                .contains("stopped without a pull request")
        })
        .unwrap();
    c["facts"]["files"]["result.json"] = serde_json::json!(
        r#"{"schemaVersion":5,"issues":[{"issue":7,"status":"aborted","failedStep":"x","cap":"step-failed:x"}]}"#
    );
    c["facts"]["files"]["progress.json"] =
        serde_json::json!(r#"{"reason":"Connection error. 502"}"#);
    let facts = Facts::from_json(&c["facts"]).unwrap();
    let r = report(&facts, &mut Given(c["facts"]["github"].clone()));
    assert!(r.log.is_empty(), "{:?}", r.log);
    assert!(!r.end.contains(&"--provider".to_string()), "{:?}", r.end);
    assert_eq!(r.arm, "parked");
}

/// What the park of the stop case "ended as running" says, with the bot's
/// pull requests on its branch as given.
fn stop_with(prs: Value) -> String {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/stop.json");
    let cases: Vec<Value> = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    let mut c = cases
        .into_iter()
        .find(|c| {
            c["case"]
                .as_str()
                .unwrap_or("")
                .contains("ended as running")
        })
        .unwrap();
    c["facts"]["github"]["prs"] = prs;
    let facts = Facts::from_json(&c["facts"]).unwrap();
    let r = report(&facts, &mut Given(c["facts"]["github"].clone()));
    assert_eq!(r.arm, "parked");
    r.actions
        .iter()
        .find_map(|a| match a {
            Action::Park(p) => Some(p.comment(false, true)),
            _ => None,
        })
        .unwrap()
}

/// The bot's pull request on the branch: #1747 reads its `createdAt`, so a
/// pull request built by a test has to carry one. It is a minute after the
/// 1793526000 the stop fixtures give their cycle, which is the earliest one
/// the report takes as this cycle's own.
fn open_pr(draft: Option<bool>) -> Value {
    let mut p = serde_json::json!({"url": "https://github.com/acme/widget/pull/67", "state": "OPEN", "body": "Fixes #7", "headRefName": "feature/x", "createdAt": "2026-11-01T09:41:00Z"});
    if let Some(d) = draft {
        p["isDraft"] = d.into();
    }
    serde_json::json!([p])
}

/// #1747: the merged or closed pull request of an attempt before this one is
/// not its own, so the stop of the cycle says it has none. An open one is the
/// branch's live pull request whoever opened it, and is named.
#[test]
fn a_stop_of_a_retry_names_no_pull_request_of_an_earlier_attempt() {
    let mut prs = open_pr(Some(false));
    prs[0]["createdAt"] = "2026-11-01T09:39:00Z".into();
    let said = stop_with(prs);
    assert!(
        said.contains("Tehas cycle stopped with pull request #67 open"),
        "OPEN: {said}"
    );
    for state in ["MERGED", "CLOSED"] {
        let mut prs = open_pr(Some(false));
        prs[0]["state"] = state.into();
        prs[0]["createdAt"] = "2026-11-01T09:39:00Z".into();
        let said = stop_with(prs);
        assert!(
            said.contains("Tehas cycle stopped without a pull request"),
            "{state}: {said}"
        );
    }
}

#[test]
fn a_stop_with_an_open_pull_request_names_it() {
    for draft in [None, Some(false)] {
        let said = stop_with(open_pr(draft));
        assert!(
            said.contains("Tehas cycle stopped with pull request #67 open: status `running`"),
            "{said}"
        );
        assert!(!said.contains("without a pull request"), "{said}");
        assert!(!said.contains("(draft)"), "{said}");
    }
}

#[test]
fn a_stop_with_a_draft_pull_request_says_it_is_a_draft() {
    let said = stop_with(open_pr(Some(true)));
    assert!(
        said.contains("Tehas cycle stopped with pull request #67 open (draft): status `running`"),
        "{said}"
    );
    assert!(
        said.contains(
            "Its branch has an open pull request: https://github.com/acme/widget/pull/67"
        ),
        "{said}"
    );
}

#[test]
fn a_stop_with_no_pull_request_says_so() {
    let said = stop_with(serde_json::json!([]));
    assert!(
        said.contains("Tehas cycle stopped without a pull request: status `running`"),
        "{said}"
    );
    assert!(!said.contains("open pull request"), "{said}");
}

#[test]
fn no_comment_says_without_a_pull_request_and_names_one() {
    for suite in [
        "crash", "handoff", "park", "record", "result", "start", "stop",
    ] {
        let path = format!("{}/tests/fixtures/{suite}.json", env!("CARGO_MANIFEST_DIR"));
        let cases: Vec<Value> = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        for c in cases {
            let facts = Facts::from_json(&c["facts"]).unwrap();
            let r = report(&facts, &mut Given(c["facts"]["github"].clone()));
            for a in &r.actions {
                let said = match a {
                    Action::Park(p) => p.comment(false, true),
                    _ => continue,
                };
                assert!(
                    !(said.contains("without a pull request")
                        && said.contains("open pull request:")),
                    "{suite}: {}: {said}",
                    c["case"]
                );
            }
        }
    }
}

/// A `prs` entry of the bot on the branch of the cases below, opened at `at`.
fn earlier_pr(state: &str, at: &str) -> Value {
    serde_json::json!([{
        "url": "https://github.com/acme/widget/pull/4",
        "state": state,
        "body": "Fixes #7",
        "headRefName": "feature/issue-7-x",
        "createdAt": at
    }])
}

/// What the report says on the issue: its comments, and the text of its parks.
fn on_issue(r: &tehas_cycle::Report) -> String {
    r.actions
        .iter()
        .map(|a| match a {
            Action::Comment { ok, .. } => ok.clone(),
            Action::Park(p) => p.comment(false, true),
            _ => String::new(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// #1747: a retried cycle runs on the branch of the attempt before it, and the
/// list of the bot's pull requests is taken with `--state all`. A cap park, or
/// an outage park, must stay what it is when that list holds one pull request
/// of the earlier attempt: the arm is the park's own, the status is never
/// `merged`, and the earlier pull request is nowhere named. The same pull
/// request opened at or after `started` is the cycle's own and is named, which
/// is the only difference between the two.
#[test]
fn a_park_of_a_retry_is_not_made_by_an_earlier_attempts_pull_request() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/park.json");
    let cases: Vec<Value> = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    // The cap park of the bash cases: a step that failed for a reason of its
    // own, with its branch and its handoff.
    let mut c = cases
        .into_iter()
        .find(|c| {
            c["case"]
                .as_str()
                .unwrap_or("")
                .starts_with("a step that failed for another reason is parked as before")
        })
        .unwrap();
    c["facts"]["github"]["prs"] = serde_json::json!([]);
    c["facts"]["github"]["attention"] = serde_json::json!(false);
    let plain = Facts::from_json(&c["facts"]).unwrap();
    let plain = report(&plain, &mut Given(c["facts"]["github"].clone()));
    assert_eq!(plain.arm, "parked", "{}", plain.arm);
    for state in ["CLOSED", "MERGED"] {
        let mut with = c.clone();
        with["facts"]["github"]["prs"] = earlier_pr(state, "2026-11-01T09:38:00Z");
        let facts = Facts::from_json(&with["facts"]).unwrap();
        let r = report(&facts, &mut Given(with["facts"]["github"].clone()));
        assert_eq!(r.arm, plain.arm, "{state}: {}", r.arm);
        assert_eq!(r.done, plain.done, "{state}: {}", r.done);
        assert!(!r.arm.starts_with("merged"), "{state}: {}", r.arm);
        assert!(!r.arm.starts_with("pr-closed"), "{state}: {}", r.arm);
        assert!(!r.done.contains("status=merged"), "{state}: {}", r.done);
        assert!(
            !on_issue(&r).contains("pull/4"),
            "{state}: {}",
            on_issue(&r)
        );
        // The same pull request opened after the cycle took its lock is its
        // own: it is the one thing that makes the earlier one wrong.
        let mut own = c.clone();
        own["facts"]["github"]["prs"] = earlier_pr(state, "2026-11-01T09:40:00Z");
        let facts = Facts::from_json(&own["facts"]).unwrap();
        let own = report(&facts, &mut Given(own["facts"]["github"].clone()));
        assert!(
            on_issue(&own).contains("pull/4"),
            "{state}: the cycle's own pull request is said nowhere: {}",
            on_issue(&own)
        );
    }
}

/// The refresh case of the handoff suite that pushed to its pull request, with
/// that pull request opened before the refresh started, as every refreshed
/// pull request is: an earlier cycle opened it.
fn refresh_of_an_older_pr(state: &str) -> (Value, tehas_cycle::Report) {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/handoff.json");
    let cases: Vec<Value> = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    let mut c = cases
        .into_iter()
        .find(|c| {
            c["case"].as_str().unwrap_or("")
                == "a refresh takes tehas:refreshing off its pull request when it ends: pushed"
        })
        .unwrap();
    c["facts"]["github"]["prs"][0]["createdAt"] = "2026-10-20T08:00:00Z".into();
    c["facts"]["github"]["prs"][0]["state"] = state.into();
    let facts = Facts::from_json(&c["facts"]).unwrap();
    let r = report(&facts, &mut Given(c["facts"]["github"].clone()));
    (c, r)
}

/// #1747 bounds only what a merged or closed pull request decides. A refresh
/// pushes to the open pull request an earlier cycle opened, so that one is
/// still the cycle's: its handoff names it and takes tehas:refreshing off it.
#[test]
fn a_refresh_ends_on_the_open_pull_request_an_earlier_cycle_opened() {
    let (c, r) = refresh_of_an_older_pr("OPEN");
    assert_eq!(r.done, c["expect"]["done"].as_str().unwrap(), "{}", r.done);
    assert!(r.done.contains("pr=https://github.com/acme/widget/pull/9"));
    assert!(
        r.actions
            .iter()
            .any(|a| matches!(a, Action::RefreshOff { pr } if pr == "9")),
        "{:?}",
        r.actions
    );
}

/// The pull request a refresh was approved for, merged while it ran, is still
/// the cycle's end however long ago it was opened.
#[test]
fn a_refresh_whose_older_pull_request_merged_is_merged_outside() {
    let (_, r) = refresh_of_an_older_pr("MERGED");
    assert_eq!(r.arm, "merged-outside", "{}", r.arm);
    assert!(r.done.contains("status=merged"), "{}", r.done);
}

/// Another older pull request of the branch, merged, is still not the
/// refresh's: only the number its approval names is exempt.
#[test]
fn a_refresh_takes_no_other_older_merged_pull_request() {
    let (mut c, _) = refresh_of_an_older_pr("MERGED");
    c["facts"]["github"]["prs"][0]["url"] = "https://github.com/acme/widget/pull/90".into();
    let facts = Facts::from_json(&c["facts"]).unwrap();
    let r = report(&facts, &mut Given(c["facts"]["github"].clone()));
    assert_ne!(r.arm, "merged-outside", "{}", r.arm);
    assert!(!r.done.contains("status=merged"), "{}", r.done);
}
