//! The note a develop timeout leaves in its issue's body (#832), and the
//! repeat it reveals (#833). A cycle reads the issue's body and nothing of its
//! comments, so what the last attempt learned goes there.
//!
//! The note is made from what the driver recorded, never from an agent's
//! words: the runner's result and the work-state file the driver left in the
//! workspace. It sits between two markers, and its opening marker carries the
//! sha256 of the body without it. A body whose note still names the body
//! around it has not changed since the note was written: a second timeout on
//! that text is a repeat, and another retry would run into the same bound.
//!
//! Pure: it reads the two files and the body it is given, and changes nothing.

use regex::Regex;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

pub const CAP: &str = "child-timeout:develop";
const OPEN: &str = "<!-- tehas:last-attempt base=";
const CLOSE: &str = "<!-- /tehas:last-attempt -->";
const MAX_PATHS: usize = 15;
/// What a repeat's park says, after its own words.
pub const REPEAT: &str = "It ran out of time in develop twice on the same text: split it with `tehas:plan`, or make the body smaller, then apply `tehas:ready`.";
const MAX_STATE: u64 = 4 << 20;

/// What libexec/cycle does with the issue's body.
#[derive(Debug, PartialEq)]
pub enum Action {
    /// Not a develop timeout, or no body to write to.
    Nothing,
    /// Write this body: the issue's own text and a fresh note.
    Edit(String),
    /// The note of the last timeout is there and the text has not changed.
    Repeat,
}

/// The body without its note, and the base the note named.
pub fn strip(body: &str) -> (String, Option<String>) {
    let body = body.replace('\r', "");
    let Some(start) = body.find(OPEN) else {
        return (body.trim_end().to_string(), None);
    };
    let Some(len) = body[start..].find(CLOSE) else {
        return (body.trim_end().to_string(), None);
    };
    let end = start + len + CLOSE.len();
    let base = body[start + OPEN.len()..]
        .split(' ')
        .next()
        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(str::to_string);
    let rest = format!(
        "{}\n\n{}",
        body[..start].trim_end(),
        body[end..].trim_start()
    );
    (rest.trim().to_string(), base)
}

/// What a develop timeout does with the issue: GitHub's answer, the text of
/// the result, and the run's state. None when either cannot be read.
pub fn of_issue(
    n: u64,
    issue: &Value,
    result: Option<&str>,
    state: Option<&Value>,
) -> Option<Action> {
    let r: Value = serde_json::from_str(result?).ok()?;
    let entry = r
        .get("issues")
        .and_then(Value::as_array)
        .and_then(|l| {
            l.iter()
                .find(|e| e.get("issue").and_then(Value::as_u64) == Some(n))
        })
        .cloned()
        .unwrap_or(Value::Null);
    let body = match issue.get("body") {
        Some(Value::String(b)) => b.as_str(),
        Some(Value::Null) => "",
        _ => return None,
    };
    Some(decide(body, &entry, state))
}

pub fn sha(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

pub fn decide(body: &str, result: &Value, state: Option<&Value>) -> Action {
    if result.get("cap").and_then(Value::as_str) != Some(CAP) {
        return Action::Nothing;
    }
    let (rest, base) = strip(body);
    let sum = sha(&rest);
    if base.as_deref() == Some(sum.as_str()) {
        return Action::Repeat;
    }
    Action::Edit(format!("{rest}\n\n{}", note(&sum, result, state)))
}

fn minutes(ms: u64) -> String {
    let s = ms / 1000;
    format!("{}m{:02}s", s / 60, s % 60)
}

/// A path as the plan declared it, or nothing when it is not a plain
/// repository path: it goes into a public issue unquoted by any agent.
fn clean_path(p: &str) -> Option<&str> {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_.-][A-Za-z0-9_./-]{0,199}$").unwrap());
    (RE.is_match(p) && !p.contains("..")).then_some(p)
}

fn note(base: &str, result: &Value, state: Option<&Value>) -> String {
    let bound = result
        .pointer("/failure/errorTail")
        .and_then(Value::as_str)
        .and_then(|t| {
            static RE: LazyLock<Regex> =
                LazyLock::new(|| Regex::new(r"killed after ([0-9]{1,10})ms").unwrap());
            RE.captures(t)?.get(1)?.as_str().parse::<u64>().ok()
        })
        .map_or("its time bound".to_string(), |ms| {
            format!("its time bound of {} minutes", ms / 60000)
        });
    let mut out = format!(
        "{OPEN}{base} -->\n### The last attempt ran out of time\n\n\
         The last cycle on this text was stopped in develop: the developer ran past {bound}. \
         Its work was not pushed, and the next cycle starts from a clean checkout.\n"
    );
    let mut lines = Vec::new();
    let events = state
        .and_then(|s| s.get("eventLog"))
        .and_then(Value::as_array);
    let mut steps: Vec<(String, u64)> = Vec::new();
    let mut verify = None;
    for e in events.into_iter().flatten() {
        let kind = e.get("kind").and_then(Value::as_str).unwrap_or("");
        let ms = e.get("ms").and_then(Value::as_u64);
        let step = e.get("step").and_then(Value::as_str).unwrap_or("");
        match kind {
            "dispatch-completed" | "dispatch-failed" if !step.is_empty() => {
                let ms = ms.unwrap_or(0);
                match steps.iter_mut().find(|(s, _)| s == step) {
                    Some((_, t)) => *t += ms,
                    None => steps.push((step.to_string(), ms)),
                }
            }
            "verify-completed" => verify = e.get("ok").and_then(Value::as_bool),
            _ => {}
        }
    }
    steps.retain(|(s, _)| s.bytes().all(|b| b.is_ascii_lowercase() || b == b'-'));
    if !steps.is_empty() {
        let s: Vec<String> = steps
            .iter()
            .map(|(s, ms)| format!("{s} {}", minutes(*ms)))
            .collect();
        lines.push(format!("- Steps: {}", s.join(", ")));
    }
    if let Some(ok) = verify {
        lines.push(format!(
            "- The last run of the gate {}",
            if ok { "passed" } else { "failed" }
        ));
    }
    if let Some(snap) = state.and_then(|s| s.pointer("/pipelineState/handoffSnapshot")) {
        let ahead: u64 = snap
            .get("committedWork")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|w| w.get("ahead").and_then(Value::as_u64))
            .sum();
        let open = snap
            .get("modifiedFiles")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        lines.push(format!(
            "- Left behind: {ahead} commit(s) ahead of the base, {open} file(s) changed and not committed"
        ));
    }
    let mut paths: Vec<&str> = Vec::new();
    let deliverables = state
        .and_then(|s| s.pointer("/pipelineState/normalisedSpec/deliverables"))
        .and_then(Value::as_array);
    let count = deliverables.map_or(0, Vec::len);
    for d in deliverables.into_iter().flatten() {
        for p in d
            .get("paths")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(p) = p.as_str().and_then(clean_path) {
                if !paths.contains(&p) {
                    paths.push(p);
                }
            }
        }
    }
    if count > 0 {
        lines.push(format!(
            "- Its plan had {count} part(s) over {} file(s):",
            paths.len()
        ));
        for p in paths.iter().take(MAX_PATHS) {
            lines.push(format!("  - `{p}`"));
        }
        if paths.len() > MAX_PATHS {
            lines.push(format!("  - and {} more", paths.len() - MAX_PATHS));
        }
    }
    if !lines.is_empty() {
        out.push('\n');
        out.push_str(&lines.join("\n"));
        out.push('\n');
    }
    out.push_str(
        "\nPlan a smaller change: finish one part well, or say in this issue what to leave out. \
         This section is the factory's; edit or delete it as you like.\n",
    );
    out.push_str(CLOSE);
    out
}

/// The driver's work-state file of the issue in the workspace, when it is a
/// plain file under it of a sane size. The workspace was written in the
/// container, so a link anywhere on the way is not followed.
pub fn state(workspace: &Path, issue: u64) -> Option<Value> {
    let root = workspace.canonicalize().ok()?;
    let p = workspace.join(format!(".pi/work-state/{issue}.json"));
    let real = p.canonicalize().ok()?;
    if real != root.join(format!(".pi/work-state/{issue}.json")) {
        return None;
    }
    let m = fs::symlink_metadata(&real).ok()?;
    if !m.is_file() || m.len() > MAX_STATE {
        return None;
    }
    serde_json::from_slice(&fs::read(&real).ok()?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn timeout() -> Value {
        json!({"cap": CAP, "failure": {"errorTail": "[pi-rukas] killed after 5400000ms child-bound"}})
    }

    fn work_state() -> Value {
        json!({
            "eventLog": [
                {"kind": "dispatch-completed", "step": "explore", "ms": 169000},
                {"kind": "dispatch-completed", "step": "plan", "ms": 264000},
                {"kind": "dispatch-completed", "step": "develop", "ms": 3931000},
                {"kind": "verify-completed", "step": "develop", "ok": false},
                {"kind": "dispatch-failed", "step": "develop", "ms": 6000000}
            ],
            "pipelineState": {
                "handoffSnapshot": {"modifiedFiles": ["a", "b"], "committedWork": [{"ahead": 1}]},
                "normalisedSpec": {"deliverables": [
                    {"id": "d1", "paths": ["libexec/drain", "test/drain_test.sh"]},
                    {"id": "d2", "paths": ["libexec/drain", "../etc/passwd", "host/docs/tools.md", "a b"]}
                ]}
            }
        })
    }

    #[test]
    fn another_cap_writes_nothing() {
        let r = json!({"cap": "child-timeout:explore"});
        assert_eq!(decide("Text.", &r, None), Action::Nothing);
        assert_eq!(decide("Text.", &json!({}), None), Action::Nothing);
    }

    #[test]
    fn a_timeout_adds_a_note_on_the_body() {
        let Action::Edit(body) = decide("Fix it.\r\n\r\n", &timeout(), Some(&work_state())) else {
            panic!("no edit");
        };
        let base = sha("Fix it.");
        assert!(
            body.starts_with(&format!("Fix it.\n\n{OPEN}{base} -->\n")),
            "{body}"
        );
        assert!(body.ends_with(CLOSE), "{body}");
        assert!(body.contains("time bound of 90 minutes"), "{body}");
        assert!(
            body.contains("- Steps: explore 2m49s, plan 4m24s, develop 165m31s"),
            "{body}"
        );
        assert!(body.contains("- The last run of the gate failed"), "{body}");
        assert!(
            body.contains("1 commit(s) ahead of the base, 2 file(s)"),
            "{body}"
        );
        assert!(
            body.contains("- Its plan had 2 part(s) over 3 file(s):"),
            "{body}"
        );
        assert!(body.contains("  - `host/docs/tools.md`"), "{body}");
        assert!(!body.contains("passwd") && !body.contains("a b"), "{body}");
    }

    #[test]
    fn without_a_work_state_the_note_is_short() {
        let Action::Edit(body) = decide("Fix it.", &json!({"cap": CAP}), None) else {
            panic!("no edit");
        };
        assert!(body.contains("ran past its time bound."), "{body}");
        assert!(!body.contains("- Steps"), "{body}");
    }

    #[test]
    fn the_same_text_with_its_note_is_a_repeat() {
        let Action::Edit(body) = decide("Fix it.", &timeout(), Some(&work_state())) else {
            panic!("no edit");
        };
        assert_eq!(decide(&body, &timeout(), None), Action::Repeat);
        // GitHub may hand the body back with carriage returns.
        assert_eq!(
            decide(&body.replace('\n', "\r\n"), &timeout(), None),
            Action::Repeat
        );
    }

    #[test]
    fn an_edit_outside_the_note_replaces_it() {
        let Action::Edit(body) = decide("Fix it.", &timeout(), None) else {
            panic!("no edit");
        };
        let edited = body.replace("Fix it.", "Fix only the drain.");
        let Action::Edit(again) = decide(&edited, &timeout(), None) else {
            panic!("not a fresh note");
        };
        assert_eq!(again.matches(OPEN).count(), 1, "{again}");
        assert!(again.starts_with("Fix only the drain."), "{again}");
    }

    #[test]
    fn a_note_in_the_middle_comes_out_whole() {
        let body = format!(
            "Top.\n\n{OPEN}{} -->\nold\n{CLOSE}\n\nBottom.",
            "0".repeat(64)
        );
        assert_eq!(
            strip(&body),
            ("Top.\n\nBottom.".to_string(), Some("0".repeat(64)))
        );
        assert_eq!(strip(&format!("Top.\n{OPEN}x")).1, None);
    }

    #[test]
    fn many_paths_are_cut() {
        let paths: Vec<String> = (0..20).map(|i| format!("lib/f{i}.sh")).collect();
        let s = json!({"pipelineState": {"normalisedSpec": {"deliverables": [{"paths": paths}]}}});
        let Action::Edit(body) = decide("X", &timeout(), Some(&s)) else {
            panic!("no edit");
        };
        assert!(body.contains("  - and 5 more"), "{body}");
        assert!(!body.contains("f15.sh"), "{body}");
    }

    #[test]
    fn the_state_file_is_read_only_through_no_link() {
        let d = std::env::temp_dir().join(format!("tehas-last-attempt-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d.join("ws/.pi/work-state")).unwrap();
        fs::write(d.join("ws/.pi/work-state/7.json"), r#"{"eventLog":[]}"#).unwrap();
        assert!(state(&d.join("ws"), 7).is_some());
        fs::create_dir_all(d.join("other/.pi/work-state")).unwrap();
        fs::write(d.join("other/.pi/work-state/8.json"), "{}").unwrap();
        fs::create_dir_all(d.join("ws2")).unwrap();
        std::os::unix::fs::symlink(d.join("other/.pi"), d.join("ws2/.pi")).unwrap();
        assert!(state(&d.join("ws2"), 8).is_none());
        assert!(state(&d.join("ws"), 9).is_none());
        let _ = fs::remove_dir_all(&d);
    }
}
