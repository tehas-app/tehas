//! The cases of test/cycle_*_test.sh as fixtures: what each cycle left, as
//! libexec/cycle read it, and what the bash then did. A suite is one file in
//! tests/fixtures/, made from a run of that test file against libexec/cycle
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
        said.contains("this tehas cannot run the lean driver yet"),
        "{said}"
    );
    assert!(said.contains("take `tehas:lean` off"), "{said}");
}

#[test]
fn another_78_on_a_lean_start_is_a_cycle_that_stopped() {
    let (end, said) = refused(78, false);
    assert!(!end.contains(&"--never-ran".to_string()), "{end:?}");
    assert!(said.contains("stopped without a pull request"), "{said}");
    assert!(!said.contains("lean"), "{said}");
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
