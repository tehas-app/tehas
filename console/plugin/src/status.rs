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
use tehas_core::plugin::hook::HookAsk;
use tehas_core::question::q_clean;
use tehas_core::settings::edge_name;
use tehas_core::settings::rules::switch;
use tehas_ops::plugin::runs;
use tehas_ports::{
    AccountKind, Actor, Bound, Caller, ChangeNo, ChangeState, CommitId, Outcome, Scope, Status,
    StatusState, WebLink,
};

/// The most characters of a description GitHub keeps.
pub const DESCRIPTION_MAX: usize = 140;
/// The description of a hook that failed, ran out of time or left a verdict
/// that breaks the schema.
pub const NOT_FINISHED: &str = "the plugin's hook did not finish";

/// The factory's account, which reads the pull request and writes its status.
const OURS: Actor = Actor::Ours(Scope::Full);
const CALLER: Caller = Caller::new("plugin.status");
/// A pull request is read for its digest alone: one of each list is enough.
fn one() -> Bound {
    Bound::new(1).unwrap_or(tehas_ports::ALL_MAX)
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
/// passed for a verdict read, `pass` or `note`, and errored for anything
/// else. v1 never fails a status.
pub fn of(name: &str, end: &Value, target_url: Option<String>) -> Status {
    let word = end.pointer("/verdict/verdict").and_then(Value::as_str);
    let summary = end.pointer("/verdict/summary").and_then(Value::as_str);
    let read = end.get("result").and_then(Value::as_str) == Some("done")
        && matches!(word, Some("pass" | "note"));
    let (state, description) = match summary.filter(|_| read) {
        Some(s) => (StatusState::Passed, q_clean(s, DESCRIPTION_MAX)),
        None => (StatusState::Errored, NOT_FINISHED.to_string()),
    };
    Status {
        name: format!("tehas/plugin/{name}"),
        state,
        about: description,
        link: target_url.and_then(|u| WebLink::new(u).ok()),
    }
}

/// What became of a post.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Posted {
    Written,
    /// The code host refused the factory the write: why, in our words.
    Refused(String),
    /// Two tries failed: the last one's reason.
    Failed(String),
}

/// A status's state in our words, for the log.
fn state_word(state: StatusState) -> &'static str {
    match state {
        StatusState::Pending => "pending",
        StatusState::Passed => "passed",
        StatusState::Failed => "failed",
        StatusState::Errored => "errored",
    }
}

/// One status on `head`, written as the factory: tried once more unless
/// the code host refused it, as a status written twice is the same status.
pub fn post(h: &mut dyn Host, head: &CommitId, status: &Status) -> Posted {
    let mut writes = match h.site().code_host_writes(&OURS, CALLER) {
        Ok(w) => w,
        Err(o) => return Posted::Failed(o.line().to_string()),
    };
    let mut last = String::new();
    for _ in 0..2 {
        match writes.set_status(head, status) {
            Ok(()) => return Posted::Written,
            Err(o @ Outcome::Forbidden) => return Posted::Refused(o.line().to_string()),
            Err(o) => last = o.line().to_string(),
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

/// A reason after the words it follows: `: ` and the reason, unless it
/// starts with one.
pub(crate) fn said(why: &str) -> String {
    if why.starts_with(": ") {
        why.to_string()
    } else {
        format!(": {why}")
    }
}

/// The head and base of `pr` as the code host gives them now, both from one
/// read. None: it is no open pull request of the factory's, from a branch of
/// the repository itself.
pub fn pull(h: &mut dyn Host, pr: u64) -> Result<Option<(String, String)>, String> {
    let Some(no) = ChangeNo::new(pr) else {
        return Ok(None);
    };
    let read = h
        .site()
        .code_host(&OURS, CALLER)
        .and_then(|mut c| c.change(no, one()));
    let digest = match read {
        Ok(c) => c.digest,
        Err(Outcome::NotFound) => return Ok(None),
        Err(o) => return Err(o.line().to_string()),
    };
    let ours = digest
        .author
        .as_ref()
        .is_some_and(|a| a.kind == AccountKind::Ours);
    let open = digest.state == ChangeState::Open && ours && !digest.from_fork;
    Ok(open.then(|| {
        (
            digest.head_commit.as_str().to_string(),
            digest.base_commit.as_str().to_string(),
        )
    }))
}

/// After the hook of `name` on `req.pr` ended, having read `head`: GitHub
/// asked for the pull request's head, the status posted on `head`, and a
/// head that moved asked for again. `view` is whether the plugin has its
/// view, so the status links to the findings.
pub fn after(h: &mut dyn Host, s: &Setup, name: &str, view: bool, req: &HookAsk, head: &str) {
    let get = s.get;
    let pr = req.pr;
    if tehas_core::settings::rules::check("TEHAS_REPO", &get("TEHAS_REPO")).is_err() {
        say(&format!(
            "plugin {name}: no status on #{pr}: TEHAS_REPO names no repository"
        ));
        return;
    }
    let Ok(at) = CommitId::new(head) else {
        say(&format!(
            "plugin {name}: no status on #{pr}: {head} is no commit"
        ));
        return;
    };
    let now = match pull(h, pr) {
        Ok(now) => now,
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
    match post(h, &at, &status) {
        Posted::Written => {
            let _ = fs::remove_file(&unwritten);
            say(&format!(
                "plugin {name}: status {} on {head} of #{pr}",
                state_word(status.state)
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
        event: req.event.clone(),
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
                name: "tehas/plugin/floorplan".into(),
                state: StatusState::Passed,
                about: "Nothing new.".into(),
                link: None,
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
            (note.state, note.about.as_str()),
            (StatusState::Passed, "One new finding.")
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
                (s.state, s.about.as_str()),
                (StatusState::Errored, NOT_FINISHED),
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
        assert_eq!(s.about, q_clean(&summary, DESCRIPTION_MAX));
        assert_eq!(s.about.chars().count(), DESCRIPTION_MAX);
        assert!(!s.about.contains("@octocat"));
        assert!(!s.about.contains("<!--"));
        assert!(!s.about.contains("[ ]"));
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
    fn a_status_links_the_target_only_when_it_has_one() {
        let end = end("done", json!({"verdict": "pass", "summary": "ok"}));
        assert_eq!(of("floorplan", &end, None).link, None);
        let target = "https://plugin-floorplan.example.org/_hooks/42";
        assert_eq!(
            of("floorplan", &end, Some(target.into())).link,
            Some(WebLink::new(target).unwrap())
        );
    }
}
