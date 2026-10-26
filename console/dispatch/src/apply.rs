//! apply: what a decision's actions do, in their order, as libexec/dispatch
//! does them. Every write to GitHub goes through the action journal of the
//! state store (section 5 of its spec): intended, attempted, then settled
//! with its typed result. A write the journal could not record is not made:
//! the next poll finds the same state and decides it again.
//!
//! This part applies the writes that stand alone: the label changes with
//! the comment, log line and answer that follow one that worked, the
//! unticks, the refreshing mark taken off a pull request, the told and said
//! records, the files of the hold, the gate and the branch check, and the
//! containers left behind. Starts, stops, sweeps and planning turns are the
//! later parts of #254, and come back in `left`.

use crate::read::run::{run, run_input, Calls};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;

/// The calls of one poll's writes.
pub struct Writer {
    pub repo: String,
    pub home: PathBuf,
    pub gh: String,
    pub docker: String,
    /// The store's calls, journal and records.
    pub calls: Calls,
    /// The poll's row, from poll-begin: what each action names.
    pub poll_at: i64,
    /// The read the actions rest on.
    pub before: i64,
    pub bound: Duration,
}

/// What applying a decision did: the lines the poll logs, in order, and the
/// actions this part does not apply.
#[derive(Debug, Default)]
pub struct Applied {
    pub log: Vec<String>,
    pub left: Vec<Value>,
}

/// How a journaled call ended.
enum Wrote {
    Done,
    Failed,
    /// Not made: the journal could not record it.
    Unrecorded,
}

fn word(v: &Value) -> String {
    v.get("word")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn text<'a>(a: &'a Value, k: &str) -> &'a str {
    a.get(k).and_then(Value::as_str).unwrap_or("")
}

fn strings(a: &Value, k: &str) -> Vec<String> {
    a.get(k)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

impl Writer {
    pub fn apply(&mut self, actions: &[Value]) -> Applied {
        let mut out = Applied::default();
        for a in actions {
            if !self.one(a, &mut out.log) {
                out.left.push(a.clone());
            }
        }
        out
    }

    /// One action; false for one this part leaves.
    fn one(&mut self, a: &Value, log: &mut Vec<String>) -> bool {
        let n = a.get("issue").and_then(Value::as_u64);
        if let (Some(n), Some(_)) = (n, a.get("remove")) {
            self.label(n, a, log);
        } else if let (Some(n), Some(key)) = (n, a.get("refresh_said").and_then(Value::as_str)) {
            self.say(n, text(a, "say"), log);
            let path = self.home.join(format!("state/refresh/{n}.said"));
            if std::fs::write(&path, key).is_err() {
                log.push(format!("WARN: could not write {}", path.display()));
            }
        } else if let (Some(n), Some(_)) = (n, a.get("park_told")) {
            self.say(n, text(a, "say"), log);
            if let Err(f) = self.calls.store(&["park-told", &n.to_string()]) {
                log.push(format!(
                    "WARN: could not record the hold on the park of #{n} ({})",
                    word(&f)
                ));
            }
        } else if let (Some(n), Some(say)) = (n, a.get("say").and_then(Value::as_str)) {
            self.say(n, say, log);
        } else if let Some(n) = a.get("untick").and_then(Value::as_u64) {
            self.untick(n, a, log);
        } else if let Some(pr) = a.get("pr").and_then(Value::as_u64) {
            self.unmark(pr, a, log);
        } else if let Some(n) = a.get("plan_note").and_then(Value::as_u64) {
            self.plan_note(n, a, log);
        } else if let Some(n) = a.get("refresh_drop").and_then(Value::as_u64) {
            let _ = std::fs::remove_file(self.home.join(format!("state/refresh/{n}.json")));
        } else if let Some(held) = a.get("refresh_held").and_then(Value::as_str) {
            let path = self.home.join("state/refresh/.held");
            if held.is_empty() {
                let _ = std::fs::remove_file(&path);
            } else if std::fs::write(&path, held).is_err() {
                log.push(format!("WARN: could not write {}", path.display()));
            }
        } else if let Some(held) = a.get("gate_held").and_then(Value::as_bool) {
            let path = self.home.join("state/low-gate.held");
            if held {
                if std::fs::write(&path, "").is_err() {
                    log.push(format!("WARN: could not write {}", path.display()));
                }
            } else if let Err(e) = std::fs::remove_file(&path) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    log.push(format!("WARN: could not remove {}", path.display()));
                }
            }
        } else if let Some(note) = a.get("branch_note").and_then(Value::as_str) {
            let path = self.home.join("state/branch-check");
            if std::fs::write(&path, format!("{note}\n")).is_err() {
                log.push(format!("WARN: could not write {}", path.display()));
            }
        } else if let Some(name) = a.get("reap").and_then(Value::as_str) {
            let r = run(
                &self.docker,
                &["rm".into(), "-f".into(), name.into()],
                Duration::from_secs(60),
            );
            if r.ok() && !r.out.trim().is_empty() {
                log.push(format!("removed the container {name}: {}", text(a, "why")));
            }
        } else {
            return false;
        }
        true
    }

    /// A write to GitHub, journaled: recorded before it is made, settled
    /// after. `input` goes to gh on standard input.
    /// `done_if`: words in a failed call's answer that mean the change is
    /// there already, settled empty.
    fn write(
        &mut self,
        issue: u64,
        kind: &str,
        args: Vec<String>,
        input: Option<&str>,
        done_if: Option<&str>,
    ) -> Wrote {
        // The body by its sum and length: the settling poll finds a comment
        // by them, and the store's arguments stay short and hold no text.
        let body = input;
        let input = input.map(|b| {
            use sha2::{Digest, Sha256};
            let sum: String = Sha256::digest(b.as_bytes())
                .iter()
                .map(|x| format!("{x:02x}"))
                .collect();
            json!({"sha256": sum, "bytes": b.len()})
        });
        let call = json!({"repo": self.repo, "gh": args, "input": input});
        let recorded = self.calls.store(&[
            "journal-record",
            "--poll",
            &self.poll_at.to_string(),
            "--kind",
            kind,
            "--issue",
            &issue.to_string(),
            "--args",
            &call.to_string(),
            "--before",
            &self.before.to_string(),
        ]);
        let Some(id) = recorded
            .ok()
            .and_then(|v| v.get("action").and_then(Value::as_i64))
        else {
            return Wrote::Unrecorded;
        };
        let id = id.to_string();
        if self
            .calls
            .store(&["journal-attempt", "--action", &id])
            .is_err()
        {
            return Wrote::Unrecorded;
        }
        let r = run_input(&self.gh, &args, body, self.bound);
        let there = done_if.is_some_and(|w| r.out.contains(w) || r.err.contains(w));
        let result = match r.status {
            Some(0) => "ok",
            None => "timed-out",
            Some(_) if there => "empty",
            Some(_) => "read-failed",
        };
        // A settle that fails leaves the action attempted: the next poll
        // settles it from a fresh read.
        let _ = self
            .calls
            .store(&["journal-settle", "--action", &id, "--result", result]);
        if r.ok() || (there && r.status.is_some()) {
            Wrote::Done
        } else {
            Wrote::Failed
        }
    }

    fn unrecorded(n: u64) -> String {
        format!("WARN: the journal could not record a write on #{n}, so it was not made")
    }

    /// say: one comment, a warning when it fails, never the poll's end.
    fn say(&mut self, n: u64, body: &str, log: &mut Vec<String>) {
        let args = vec![
            "issue".into(),
            "comment".into(),
            n.to_string(),
            "--repo".into(),
            self.repo.clone(),
            "--body-file".into(),
            "-".into(),
        ];
        match self.write(n, "comment-post", args, Some(body), None) {
            Wrote::Done => {}
            Wrote::Failed => log.push(format!("WARN: comment on #{n} failed")),
            Wrote::Unrecorded => log.push(Self::unrecorded(n)),
        }
    }

    /// A label change, and what follows one that worked (A6): its line, its
    /// comment, the answer of its question.
    fn label(&mut self, n: u64, a: &Value, log: &mut Vec<String>) {
        let remove = strings(a, "remove");
        let add = strings(a, "add");
        let mut args: Vec<String> = vec![
            "issue".into(),
            "edit".into(),
            n.to_string(),
            "--repo".into(),
            self.repo.clone(),
        ];
        for l in &add {
            args.extend(["--add-label".into(), l.clone()]);
        }
        for l in &remove {
            args.extend(["--remove-label".into(), l.clone()]);
        }
        let kind = if add.is_empty() {
            "label-remove"
        } else {
            "label-add"
        };
        match self.write(n, kind, args, None, None) {
            Wrote::Done => {}
            Wrote::Failed => {
                log.push(text(a, "fail").to_string());
                return;
            }
            Wrote::Unrecorded => {
                log.push(Self::unrecorded(n));
                return;
            }
        }
        if let Some(line) = a.get("log").and_then(Value::as_str) {
            log.push(line.to_string());
        }
        if let Some(say) = a.get("say").and_then(Value::as_str) {
            self.say(n, say, log);
        }
        if let Some(q) = a.get("answered") {
            let id = q.get("id").map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            });
            let by = text(q, "by").to_string();
            let mut last = Ok(Value::Null);
            for i in 0..3 {
                last = self.calls.store(&[
                    "question-answered",
                    &n.to_string(),
                    "--id",
                    id.as_deref().unwrap_or(""),
                    "--by",
                    &by,
                ]);
                // tehas_store_try: again only on busy or timeout.
                match &last {
                    Err(f) if i < 2 && matches!(word(f).as_str(), "busy" | "timeout") => {
                        std::thread::sleep(Duration::from_secs(1));
                    }
                    _ => break,
                }
            }
            if let Err(f) = last {
                log.push(format!(
                    "WARN: could not record the answer to the question of #{n} ({})",
                    word(&f)
                ));
            }
        }
    }

    /// untick: the box taken out of the question's comment, then its note.
    fn untick(&mut self, n: u64, a: &Value, log: &mut Vec<String>) {
        let comment = match a.get("comment") {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Number(c)) => c.to_string(),
            _ => String::new(),
        };
        let args = vec![
            "api".into(),
            "-X".into(),
            "PATCH".into(),
            format!("repos/{}/issues/comments/{comment}", self.repo),
            "--input".into(),
            "-".into(),
        ];
        let body = json!({"body": text(a, "body")}).to_string();
        match self.write(n, "comment-edit", args, Some(&body), None) {
            Wrote::Done => {
                if let Some(say) = a.get("say").and_then(Value::as_str) {
                    self.say(n, say, log);
                }
            }
            Wrote::Failed => log.push(text(a, "fail").to_string()),
            Wrote::Unrecorded => log.push(Self::unrecorded(n)),
        }
    }

    /// refresh_unmark: tehas:refreshing off a pull request; one already
    /// without it is done.
    fn unmark(&mut self, pr: u64, a: &Value, log: &mut Vec<String>) {
        let args = vec![
            "api".into(),
            "-X".into(),
            "DELETE".into(),
            format!("repos/{}/issues/{pr}/labels/tehas:refreshing", self.repo),
        ];
        let worked = match self.write(pr, "refresh-end", args, None, Some("Not Found")) {
            Wrote::Done => true,
            Wrote::Failed => false,
            Wrote::Unrecorded => {
                log.push(Self::unrecorded(pr));
                return;
            }
        };
        if worked {
            if let Some(line) = a.get("log").and_then(Value::as_str) {
                log.push(line.to_string());
            }
        } else {
            log.push(format!(
                "WARN: label change on pull request #{pr} failed: --remove-label tehas:refreshing"
            ));
        }
    }

    /// A note of a planning session, said once: the store answers whether
    /// this is the first time.
    fn plan_note(&mut self, n: u64, a: &Value, log: &mut Vec<String>) {
        let id = match a.get("id") {
            Some(Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => String::new(),
        };
        match self.calls.store(&[
            "plan-note",
            &n.to_string(),
            "--kind",
            text(a, "kind"),
            "--id",
            &id,
        ]) {
            Ok(v) => {
                if v.get("said") == Some(&json!(true)) {
                    self.say(n, text(a, "say"), log);
                }
            }
            Err(f) => log.push(format!(
                "WARN: the record of #{n} could not be updated ({})",
                word(&f)
            )),
        }
    }
}

#[cfg(test)]
#[path = "apply_tests.rs"]
mod tests;
