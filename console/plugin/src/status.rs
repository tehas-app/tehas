//! What a hook's end posts (docs/specs/2026-11-21-plugins-hooks.md, "What
//! tehas posts"): one commit status on the head the hook read, and never a
//! comment, since a cycle's agents read the pull request where tehas cannot
//! filter what they read. Then, when the pull request's head moved while the
//! hook ran, a request for the new head.
//!
//! A status is not written through the action journal: the poll that settles
//! the journal's rows knows no status, and a status written twice is the
//! same status, since GitHub keeps the newest of a context. So one retry and
//! a log line.

use crate::host::Host;
use crate::run::Setup;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use tehas_core::file::{write_atomic, Temp};
use tehas_core::plugin::hook::{open_bot_pull, HookAsk};
use tehas_core::question::q_clean;
use tehas_core::settings::edge_name;
use tehas_core::settings::rules::switch;
use tehas_ops::plugin::runs;

/// The most characters of a description GitHub keeps.
pub const DESCRIPTION_MAX: usize = 140;
/// The description of a hook that failed, ran out of time or left a verdict
/// that breaks the schema.
pub const NOT_FINISHED: &str = "the plugin's hook did not finish";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub context: String,
    pub state: &'static str,
    pub description: String,
    pub target_url: Option<String>,
}

/// The gate's page of the hook's findings, with `view` granted and a domain
/// to name it under.
pub fn target(name: &str, pr: u64, view: bool, get: &dyn Fn(&str) -> String) -> Option<String> {
    let domain = get("TEHAS_DOMAIN");
    if !view || domain.is_empty() {
        return None;
    }
    let one_level = switch(&get("TEHAS_EDGE_ONE_LEVEL")) == Some(true);
    let host = edge_name(&format!("plugin-{name}"), &domain, one_level);
    Some(format!("https://{host}/_hooks/{pr}"))
}

/// The status of a hook's end, `end` as `hooks/<pr>.verdict.json` holds it:
/// `success` for a verdict read, `pass` or `note`, and `error` for anything
/// else. v1 never fails a status.
pub fn of(name: &str, end: &Value, target_url: Option<String>) -> Status {
    let word = end.pointer("/verdict/verdict").and_then(Value::as_str);
    let summary = end.pointer("/verdict/summary").and_then(Value::as_str);
    let read = end.get("result").and_then(Value::as_str) == Some("done")
        && matches!(word, Some("pass" | "note"));
    let (state, description) = match summary.filter(|_| read) {
        Some(s) => ("success", q_clean(s, DESCRIPTION_MAX)),
        None => ("error", NOT_FINISHED.to_string()),
    };
    Status {
        context: format!("tehas/plugin/{name}"),
        state,
        description,
        target_url,
    }
}

/// The words of gh that post `s` on `head`. Each field goes with `-f`, which
/// gh takes as text and never as a file to read.
pub fn post_args(repo: &str, head: &str, s: &Status) -> Vec<String> {
    let mut a: Vec<String> = vec![
        "api".into(),
        "-X".into(),
        "POST".into(),
        format!("repos/{repo}/statuses/{head}"),
        "-f".into(),
        format!("state={}", s.state),
        "-f".into(),
        format!("context={}", s.context),
        "-f".into(),
        format!("description={}", s.description),
    ];
    if let Some(u) = &s.target_url {
        a.extend(["-f".into(), format!("target_url={u}")]);
    }
    a
}

/// What became of a post.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Posted {
    Written,
    /// GitHub refused the App the write: why, as gh said it.
    Refused(String),
    /// Two tries failed: the last one's reason.
    Failed(String),
}

/// A 403 on a status is the App without "Commit statuses: Read and write".
/// A rate limit's 403 is said as a rate limit, not as the status.
fn lacks_permission(why: &str) -> bool {
    why.contains("HTTP 403") || why.contains("Resource not accessible by integration")
}

pub fn post(h: &mut dyn Host, args: &[String]) -> Posted {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut last = String::new();
    for _ in 0..2 {
        match h.github(&words) {
            Ok(_) => return Posted::Written,
            Err(why) if lacks_permission(&why) => return Posted::Refused(why),
            Err(why) => last = why,
        }
    }
    Posted::Failed(last)
}

/// `hooks/<pr>.verdict.json` when it is of this pull request and head; a
/// hook that left none did not finish.
fn end_of(home: &Path, name: &str, pr: u64, head: &str) -> Value {
    let read = fs::read(runs::verdict(home, name, pr))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .filter(|v| v["pr"] == pr && v["head"] == head);
    read.unwrap_or_else(
        || json!({"api": 1, "pr": pr, "head": head, "result": "failed", "verdict": null}),
    )
}

fn say(line: &str) {
    tehas_core::outln!("{line}");
}

/// A reason after the words it follows: gh's reasons start with `: `, the
/// runner's own do not.
fn said(why: &str) -> String {
    if why.starts_with(": ") {
        why.to_string()
    } else {
        format!(": {why}")
    }
}

/// After the hook of `name` on `req.pr` ended, having read `head`: GitHub
/// asked for the pull request's head, the status posted on `head`, and a
/// head that moved asked for again. `view` is whether the plugin has its
/// view, so the status links to the findings.
pub fn after(h: &mut dyn Host, s: &Setup, name: &str, view: bool, req: &HookAsk, head: &str) {
    let get = s.get;
    let pr = req.pr;
    let repo = get("TEHAS_REPO");
    if !tehas_ops::forge::repo_shaped(&repo) {
        say(&format!(
            "plugin {name}: no status on #{pr}: TEHAS_REPO names no repository"
        ));
        return;
    }
    let mut bot = get("TEHAS_BOT_LOGIN");
    if bot.is_empty() {
        bot = tehas_core::settings::bot_login(s.home).unwrap_or_default();
    }
    let path = format!("repos/{repo}/pulls/{pr}");
    let now = match h.github(&["api", &path]) {
        Ok(out) => {
            let pull = serde_json::from_str::<Value>(&out).unwrap_or(Value::Null);
            open_bot_pull(&pull, &repo, &bot)
        }
        Err(why) => {
            say(&format!(
                "plugin {name}: GitHub did not say the head of #{pr}{}: its status goes on {head}",
                said(&why)
            ));
            None
        }
    };
    let end = end_of(s.home, name, pr, head);
    let status = of(name, &end, target(name, pr, view, get));
    let unwritten = runs::unwritten(s.home, name);
    match post(h, &post_args(&repo, head, &status)) {
        Posted::Written => {
            let _ = fs::remove_file(&unwritten);
            say(&format!(
                "plugin {name}: status {} on {head} of #{pr}",
                status.state
            ));
        }
        Posted::Refused(why) => {
            let line = format!(
                "GitHub refused the status on #{pr}{}: the App needs Commit statuses: Read and write",
                said(&why)
            );
            say(&format!("plugin {name}: {line}"));
            if let Err(e) = write_atomic(
                &unwritten,
                format!("{line}\n").as_bytes(),
                0o600,
                &Temp::Pid,
            ) {
                say(&format!("plugin {name}: {}: {e}", unwritten.display()));
            }
        }
        Posted::Failed(why) => say(&format!(
            "plugin {name}: the status on #{pr} could not be written (2 tries, the last{})",
            said(&why)
        )),
    }
    let Some((moved, base)) = now.filter(|(at, _)| at != head) else {
        return;
    };
    say(&format!(
        "plugin {name}: #{pr} moved to {moved} while its hook read {head}: asked again"
    ));
    let again = HookAsk {
        pr,
        head: moved,
        base,
        issue: req.issue,
        trigger: req.trigger,
    };
    if let Err(why) = runs::ask_hook(h.exec(), s.home, name, &again) {
        say(&format!("plugin {name}: #{pr} was not asked again: {why}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const H: &str = "89abcdef0123456789abcdef0123456789abcdef";

    fn end(result: &str, verdict: Value) -> Value {
        json!({"api": 1, "pr": 42, "head": H, "result": result, "verdict": verdict})
    }

    #[test]
    fn a_verdict_read_is_a_success_and_anything_else_an_error() {
        let pass = of(
            "floorplan",
            &end(
                "done",
                json!({"verdict": "pass", "summary": "Nothing new.", "findings": []}),
            ),
            None,
        );
        assert_eq!(
            pass,
            Status {
                context: "tehas/plugin/floorplan".into(),
                state: "success",
                description: "Nothing new.".into(),
                target_url: None,
            }
        );
        let note = of(
            "floorplan",
            &end(
                "done",
                json!({"verdict": "note", "summary": "One new finding.", "findings": []}),
            ),
            None,
        );
        assert_eq!(
            (note.state, note.description.as_str()),
            ("success", "One new finding.")
        );
        for e in [
            end("failed", Value::Null),
            end("timeout", Value::Null),
            end("done", Value::Null),
            end("done", json!({"verdict": "fail", "summary": "x"})),
            json!({}),
        ] {
            let s = of("floorplan", &e, None);
            assert_eq!(
                (s.state, s.description.as_str()),
                ("error", NOT_FINISHED),
                "{e}"
            );
        }
    }

    #[test]
    fn a_summary_is_cleaned_and_cut() {
        let summary = format!(
            "- [ ] see @octocat <!-- tehas:park --> {}",
            "the map ".repeat(30)
        );
        let s = of(
            "floorplan",
            &end(
                "done",
                json!({"verdict": "note", "summary": summary.as_str()}),
            ),
            None,
        );
        assert_eq!(s.description, q_clean(&summary, DESCRIPTION_MAX));
        assert_eq!(s.description.chars().count(), DESCRIPTION_MAX);
        assert!(!s.description.contains("@octocat"));
        assert!(!s.description.contains("<!--"));
        assert!(!s.description.contains("[ ]"));
    }

    fn vars(pairs: &[(&str, &str)]) -> impl Fn(&str) -> String {
        let map: BTreeMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |name: &str| map.get(name).cloned().unwrap_or_default()
    }

    #[test]
    fn the_target_is_the_gates_page_only_with_a_view() {
        let get = vars(&[("TEHAS_DOMAIN", "example.org")]);
        assert_eq!(
            target("floorplan", 42, true, &get).as_deref(),
            Some("https://plugin-floorplan.example.org/_hooks/42")
        );
        assert_eq!(target("floorplan", 42, false, &get), None);
        let one = vars(&[
            ("TEHAS_DOMAIN", "demo.example.org"),
            ("TEHAS_EDGE_ONE_LEVEL", "on"),
        ]);
        assert_eq!(
            target("floorplan", 42, true, &one).as_deref(),
            Some("https://plugin-floorplan-demo.example.org/_hooks/42")
        );
        assert_eq!(target("floorplan", 42, true, &vars(&[])), None);
    }

    #[test]
    fn a_status_is_posted_with_its_fields_and_a_target_only_when_it_has_one() {
        let mut s = of(
            "floorplan",
            &end("done", json!({"verdict": "pass", "summary": "ok"})),
            None,
        );
        let statuses = format!("repos/acme/widget/statuses/{H}");
        assert_eq!(
            post_args("acme/widget", H, &s),
            [
                "api",
                "-X",
                "POST",
                statuses.as_str(),
                "-f",
                "state=success",
                "-f",
                "context=tehas/plugin/floorplan",
                "-f",
                "description=ok",
            ]
        );
        s.target_url = Some("https://plugin-floorplan.example.org/_hooks/42".into());
        let a = post_args("acme/widget", H, &s);
        assert_eq!(
            a[a.len() - 2..],
            [
                "-f",
                "target_url=https://plugin-floorplan.example.org/_hooks/42"
            ]
        );
        assert!(lacks_permission(
            ": HTTP 403: Resource not accessible by integration"
        ));
        assert!(!lacks_permission(": rate limit"));
    }
}
