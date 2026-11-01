//! The report's shadow: the Rust decides a cycle's report beside the bash's,
//! from the same files, and says where the two differ. Opt-in, for the host
//! where it is being proven: libexec/cycle runs it only while
//! state/cycle-shadow.on is there.
//!
//! `begin`, just before the bash reads the cycle's files, reads them too,
//! asks GitHub its own questions, decides, and keeps all of it in
//! state/cycles/shadow-<issue>.json. While that file is there the bash keeps
//! what it does beside it: each call of gh (shadow-<issue>.gh), the park
//! (shadow-<issue>.park) and the end (shadow-<issue>.end) on record. The
//! names start with a letter, so a poll never takes one for the record of an
//! older tehas (#923).
//! `compare`, after the bash's report, sets the two side by side, adds one
//! line to logs/cycle-shadow.log, and removes the files. A report that
//! differed keeps its shadow in state/cycle-shadow/, the newest 50.

use crate::host::{self, Live, Start};
use crate::report::{last_attempts, report, to_json};
use crate::Facts;
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// The comment a park posts when its question's label could not be added.
const BOXES: &str = "The boxes above cannot be used: the label `tehas:question` could not be added (run `tehas labels` on the host). To retry, apply `tehas:ready`.";

/// The end of one call in shadow-<issue>.gh: each argument ends in a NUL,
/// and the call in this one.
const CALL_END: &str = "\u{1e}";

const KEEP: usize = 50;

fn base(home: &Path, issue: u64) -> PathBuf {
    home.join(format!("state/cycles/shadow-{issue}"))
}

fn with(p: &Path, ext: &str) -> PathBuf {
    PathBuf::from(format!("{}{ext}", p.display()))
}

/// The shadow's files of an issue, removed.
fn clear(b: &Path) {
    for ext in [".json", ".gh", ".park", ".end"] {
        let _ = fs::remove_file(with(b, ext));
    }
}

/// Decide the report of the cycle `s` from the host as it is now, and keep
/// the decision for `compare`.
pub fn begin(home: &Path, s: &Start) -> Result<(), String> {
    let t0 = Instant::now();
    let b = base(home, s.issue);
    clear(&b);
    let facts_json = host::facts(home, s);
    let facts = Facts::from_json(&facts_json)?;
    let mut gh = Live {
        repo: facts.repo.clone(),
        issue: s.issue,
        said: Default::default(),
    };
    let mut r = report(&facts, &mut gh);
    last_attempts(&mut r, &facts, &mut gh);
    let mut facts_json = facts_json;
    facts_json["github"] = Value::Object(gh.said);
    let kept = json!({
        "facts": facts_json,
        "report": to_json(&r, s.issue),
        "seconds": (t0.elapsed().as_secs_f64() * 10.0).round() / 10.0,
    });
    fs::write(with(&b, ".json"), kept.to_string()).map_err(|e| e.to_string())
}

/// Text cut for the log line.
fn short(s: &str) -> String {
    let c: String = s.chars().take(300).collect();
    if c.len() < s.len() {
        format!("{c}...")
    } else {
        c
    }
}

/// The arguments of a file printf '%s\0' wrote, None when there is no file.
fn args(p: &Path) -> Option<Vec<String>> {
    let b = fs::read(p).ok()?;
    let t = String::from_utf8_lossy(&b).into_owned();
    let mut a: Vec<String> = t.split('\0').map(str::to_string).collect();
    if a.last().is_some_and(String::is_empty) {
        a.pop();
    }
    Some(a)
}

/// The writes of the bash to GitHub, in order, from its calls of gh: one
/// write however often it was tried.
pub fn bash_writes(calls: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = vec![];
    for call in calls.split(&format!("{CALL_END}\0")) {
        let a: Vec<&str> = call.split('\0').collect();
        let a: Vec<&str> = match a.split_last() {
            Some((&"", rest)) => rest.to_vec(),
            _ => a,
        };
        let after = |flag: &str| {
            a.iter()
                .position(|x| *x == flag)
                .and_then(|i| a.get(i + 1))
                .map(|s| s.to_string())
        };
        let w = match a.as_slice() {
            ["issue", "edit", ..] if a.len() >= 5 => ("relabel", a[5..].join(" ")),
            ["issue", "comment", ..] => ("comment", after("--body").unwrap_or_default()),
            ["api", path, ..] if path.ends_with("/comments") && a.contains(&"-f") => (
                "question",
                after("-f")
                    .unwrap_or_default()
                    .trim_start_matches("body=")
                    .to_string(),
            ),
            ["pr", "comment", pr, ..] => (
                "pr_comment",
                format!("{pr}\n{}", after("--body").unwrap_or_default()),
            ),
            ["api", "-X", "PATCH", path, ..] => ("body_edit", path.to_string()),
            ["api", "-X", "DELETE", path, ..] => ("refresh_off", path.to_string()),
            _ => continue,
        };
        let w = (w.0.to_string(), w.1);
        if matches!(w.0.as_str(), "relabel" | "refresh_off" | "comment") && out.last() == Some(&w) {
            continue;
        }
        out.push(w);
    }
    out
}

enum Want {
    One(&'static str, Vec<String>),
    Maybe(&'static str, String),
}

fn both(v: &Value) -> Vec<String> {
    ["ok", "stuck"]
        .iter()
        .filter_map(|k| v.get(*k).and_then(Value::as_str).map(str::to_string))
        .collect()
}

fn s(v: &Value) -> String {
    v.as_str().unwrap_or("").to_string()
}

/// What the Rust's report writes, as alternatives where the outcome of a
/// label change decides the words; how its park went is read off the bash's
/// writes, as only the bash met it.
fn wanted(report: &Value, repo: &str, bash: &[(String, String)]) -> Vec<Want> {
    let mut out = vec![];
    let rel = |remove: &[Value], add: &[Value]| {
        let mut a = vec![];
        for r in remove {
            a.push(format!("--remove-label {}", s(r)));
        }
        for x in add {
            a.push(format!("--add-label {}", s(x)));
        }
        a.join(" ")
    };
    let qi = bash.iter().position(|(k, _)| k == "question");
    let recorded = qi.is_some();
    let posted = qi.is_some_and(|i| {
        !bash
            .get(i + 1)
            .is_some_and(|(k, t)| k == "comment" && t != BOXES)
    });
    let empty = vec![];
    for a in report["actions"].as_array().unwrap_or(&empty) {
        if let Some(r) = a.get("relabel") {
            let remove = r["remove"].as_array().unwrap_or(&empty);
            let add = r["add"].as_array().unwrap_or(&empty);
            out.push(Want::One("relabel", vec![rel(remove, add)]));
        } else if let Some(c) = a.get("comment") {
            out.push(Want::One("comment", both(c)));
        } else if let Some(p) = a.get("park") {
            out.push(Want::One(
                "relabel",
                vec!["--remove-label tehas:running --add-label needs-human-attention".into()],
            ));
            if p["edit"] == json!(true) {
                let issue = s(&report["end"][0]);
                out.push(Want::One(
                    "body_edit",
                    vec![format!("repos/{repo}/issues/{issue}")],
                ));
            }
            if recorded {
                out.push(Want::One("question", both(&p["question"])));
            }
            if recorded && posted {
                out.push(Want::One(
                    "relabel",
                    vec!["--add-label tehas:question".into()],
                ));
                out.push(Want::Maybe("comment", BOXES.into()));
            } else if recorded {
                out.push(Want::One("comment", both(&p["comment"])));
            } else {
                out.push(Want::One("comment", both(&p["comment_unrecorded"])));
            }
        } else if let Some(c) = a.get("pr_comment") {
            out.push(Want::One(
                "pr_comment",
                vec![format!("{}\n{}", s(&c["pr"]), s(&c["body"]))],
            ));
        } else if let Some(pr) = a.get("refresh_off") {
            out.push(Want::One(
                "refresh_off",
                vec![format!(
                    "repos/{repo}/issues/{}/labels/tehas:refreshing",
                    s(pr)
                )],
            ));
        }
    }
    out
}

/// Whether the bash's writes are the ones wanted, in order.
fn same_writes(want: &[Want], bash: &[(String, String)]) -> bool {
    let mut i = 0;
    for w in want {
        let next = bash.get(i);
        match w {
            Want::One(k, alt) => match next {
                Some((bk, bt)) if bk == k && alt.contains(bt) => i += 1,
                _ => return false,
            },
            Want::Maybe(k, t) => {
                if next.is_some_and(|(bk, bt)| bk == k && bt == t) {
                    i += 1;
                }
            }
        }
    }
    i == bash.len()
}

/// The end on record without the paths of the result and the progress.
fn without_files(end: Vec<String>) -> Vec<String> {
    let mut out = vec![];
    let mut it = end.into_iter();
    while let Some(a) = it.next() {
        if a == "--result" || a == "--progress" {
            it.next();
            continue;
        }
        out.push(a);
    }
    out
}

/// Set the bash's report beside the shadow's, log the line, and remove the
/// shadow's files. Nothing to compare is no error: the shadow did not begin.
pub fn compare(home: &Path, issue: u64) -> Result<Option<Value>, String> {
    let b = base(home, issue);
    let Ok(text) = fs::read_to_string(with(&b, ".json")) else {
        clear(&b);
        return Ok(None);
    };
    let kept: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let rep = &kept["report"];
    let repo = s(&kept["facts"]["repo"]);
    let calls = fs::read(with(&b, ".gh"))
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let bash = bash_writes(&calls);
    let want = wanted(rep, &repo, &bash);
    let mut differs = vec![];
    let mut line = json!({
        "time": tehas_core::time::iso(tehas_core::time::now()),
        "issue": issue,
        "attempt": kept["facts"]["start"]["attempt"],
    });
    let rust_end: Vec<String> = rep["end"]
        .as_array()
        .map(|a| a.iter().map(s).collect())
        .unwrap_or_default();
    let bash_end = args(&with(&b, ".end")).map(without_files);
    if bash_end.as_ref() != Some(&rust_end) {
        differs.push("end");
        line["end"] = json!({"bash": bash_end, "rust": rust_end});
    }
    let rust_park: Option<Vec<String>> = rep["actions"]
        .as_array()
        .and_then(|a| a.iter().find_map(|x| x.get("park")))
        .map(|p| {
            p["args"]
                .as_array()
                .map(|a| a.iter().map(s).collect())
                .unwrap_or_default()
        });
    let bash_park = args(&with(&b, ".park"));
    if bash_park != rust_park {
        differs.push("park");
        line["park"] = json!({"bash": bash_park, "rust": rust_park});
    }
    if !same_writes(&want, &bash) {
        differs.push("writes");
        let first = |w: &Want| match w {
            Want::One(k, alt) => json!([k, short(alt.first().map(String::as_str).unwrap_or(""))]),
            Want::Maybe(k, t) => json!([k, short(t), "maybe"]),
        };
        line["writes"] = json!({
            "bash": bash.iter().map(|(k, t)| json!([k, short(t)])).collect::<Vec<_>>(),
            "rust": want.iter().map(first).collect::<Vec<_>>(),
        });
    }
    line["same"] = json!(differs.is_empty());
    line["differs"] = json!(differs);
    line["rust_log"] = rep["log"].clone();
    line["done"] = rep["done"].clone();
    line["arm"] = rep["arm"].clone();
    line["seconds"] = kept["seconds"].clone();
    if !differs.is_empty() {
        keep(home, issue, &kept, &calls);
    }
    let log = home.join("logs/cycle-shadow.log");
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
        .map_err(|e| format!("{}: {e}", log.display()))?;
    writeln!(f, "{line}").map_err(|e| e.to_string())?;
    clear(&b);
    Ok(Some(line))
}

/// A shadow that differed, kept for a person to read with what the bash
/// did, in state/cycle-shadow/, the newest KEEP of them.
fn keep(home: &Path, issue: u64, kept: &Value, calls: &str) {
    let dir = home.join("state/cycle-shadow");
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let b = base(home, issue);
    let mut v = kept.clone();
    v["bash"] = json!({
        "gh": calls.split(&format!("{CALL_END}\0")).filter(|c| !c.is_empty())
            .map(|c| c.split('\0').filter(|a| !a.is_empty()).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "park": args(&with(&b, ".park")),
        "end": args(&with(&b, ".end")),
    });
    let name = format!(
        "{}-{issue}-{}.json",
        tehas_core::time::now(),
        kept["facts"]["start"]["attempt"]
    );
    let _ = fs::write(dir.join(name), v.to_string());
    let mut all: Vec<PathBuf> = fs::read_dir(&dir)
        .map(|d| d.filter_map(Result::ok).map(|e| e.path()).collect())
        .unwrap_or_default();
    all.sort();
    while all.len() > KEEP {
        let _ = fs::remove_file(all.remove(0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(args: &[&str]) -> String {
        args.iter().map(|a| format!("{a}\0")).collect::<String>() + CALL_END + "\0"
    }

    #[test]
    fn the_bash_writes_are_read_off_its_calls_of_gh() {
        let calls = [
            call(&["pr", "list", "--repo", "a/b", "--head", "x"]),
            call(&[
                "issue",
                "edit",
                "7",
                "--repo",
                "a/b",
                "--remove-label",
                "tehas:running",
            ]),
            call(&[
                "issue",
                "edit",
                "7",
                "--repo",
                "a/b",
                "--remove-label",
                "tehas:running",
            ]),
            call(&[
                "issue",
                "comment",
                "7",
                "--repo",
                "a/b",
                "--body",
                "line one\nline two",
            ]),
            call(&[
                "api",
                "repos/a/b/issues/7/comments",
                "-f",
                "body=Q",
                "--jq",
                ".id",
            ]),
        ]
        .concat();
        assert_eq!(
            bash_writes(&calls),
            vec![
                ("relabel".into(), "--remove-label tehas:running".into()),
                ("comment".into(), "line one\nline two".into()),
                ("question".into(), "Q".into()),
            ]
        );
    }

    #[test]
    fn the_note_left_in_a_body_is_a_write_of_its_own() {
        let calls = call(&["api", "-X", "PATCH", "repos/a/b/issues/7", "--input", "-"]);
        assert_eq!(
            bash_writes(&calls),
            vec![("body_edit".into(), "repos/a/b/issues/7".into())]
        );
        let rep = json!({"end": ["7"], "actions": [{"park": {"args": [], "edit": true,
            "question": {"ok": "Q", "stuck": "Qs"}, "comment": {"ok": "C", "stuck": "Cs"},
            "comment_unrecorded": {"ok": "U", "stuck": "Us"}}}]});
        let c = |k: &str, t: &str| (k.to_string(), t.to_string());
        let park = c(
            "relabel",
            "--remove-label tehas:running --add-label needs-human-attention",
        );
        let edit = c("body_edit", "repos/a/b/issues/7");
        let label = c("relabel", "--add-label tehas:question");
        let with = vec![park.clone(), edit, c("question", "Q"), label.clone()];
        assert!(same_writes(&wanted(&rep, "a/b", &with), &with));
        let without = vec![park, c("question", "Q"), label];
        assert!(!same_writes(&wanted(&rep, "a/b", &without), &without));
    }

    #[test]
    fn a_comment_matches_either_of_its_words() {
        let rep = json!({"actions": [
            {"relabel": {"remove": ["tehas:running"], "add": []}},
            {"comment": {"ok": "A", "stuck": "A stuck"}}]});
        let bash = vec![
            (
                "relabel".to_string(),
                "--remove-label tehas:running".to_string(),
            ),
            ("comment".to_string(), "A stuck".to_string()),
        ];
        assert!(same_writes(&wanted(&rep, "a/b", &bash), &bash));
        let other = vec![bash[0].clone(), ("comment".to_string(), "B".to_string())];
        assert!(!same_writes(&wanted(&rep, "a/b", &other), &other));
    }

    #[test]
    fn a_park_whose_question_was_not_posted_says_its_comment() {
        let rep = json!({"actions": [{"park": {"args": [],
            "question": {"ok": "Q", "stuck": "Qs"}, "comment": {"ok": "C", "stuck": "Cs"},
            "comment_unrecorded": {"ok": "U", "stuck": "Us"}}}]});
        let r = |t: &str| ("relabel".to_string(), t.to_string());
        let c = |k: &str, t: &str| (k.to_string(), t.to_string());
        let park = r("--remove-label tehas:running --add-label needs-human-attention");
        for bash in [
            vec![
                park.clone(),
                c("question", "Q"),
                r("--add-label tehas:question"),
            ],
            vec![
                park.clone(),
                c("question", "Q"),
                r("--add-label tehas:question"),
                c("comment", BOXES),
            ],
            vec![park.clone(), c("question", "Q"), c("comment", "C")],
            vec![park.clone(), c("comment", "Us")],
        ] {
            assert!(same_writes(&wanted(&rep, "a/b", &bash), &bash), "{bash:?}");
        }
    }
}
