//! The end of a planning turn, as lib/planning.sh does it: the turn's exit
//! status collected (plan_collect, plan_finish), its question posted
//! (plan_post), a plan written onto its issue (plan_update), a refine that
//! filed replacing its issue (plan_replace), a
//! turn cut off (plan_recover) or a session Pi lost (plan_lost), and a
//! session ended (plan_end, plan_close). The start of a turn is the next
//! part of #254.

use super::{text, word, Writer, Wrote};
use crate::decide::{iso, shape};
use regex::Regex;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use tehas_core::question::{self, Kind, MAX_OPTIONS};

/// plan_filed_re: an address of an issue a turn filed.
fn filed_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"^https://github\.com/[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+/issues/[0-9]+\z").unwrap()
    })
}

/// plan_sid_ok: a session id the store keeps.
fn sid_ok(s: &str) -> bool {
    (1..=128).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// A text of the outcome, a string or nothing, as `.k // ""` reads it.
fn s<'a>(v: &'a Value, path: &str) -> &'a str {
    v.pointer(path).and_then(Value::as_str).unwrap_or("")
}

/// q_render for a planning turn: the body of its question comment.
pub fn q_render(issue: u64, turn: u64, out: &Value, line: &str) -> String {
    question::q_render(Kind::Plan, issue, turn, out, line)
}

/// plan_no_later_parts: true only when the plan has a Later parts section
/// of nothing but None.
pub fn no_later_parts(body: &str) -> bool {
    let none = |t: &str| {
        let t: String = t
            .to_lowercase()
            .chars()
            .filter(|c| !matches!(c, '-' | '*' | '_' | '.' | ':' | '(' | ')') && !c.is_whitespace())
            .collect();
        t.is_empty() || t == "none"
    };
    let bold = Regex::new(r"^\*\*[^*]+\*\*[[:space:]]*:?[[:space:]]*$").unwrap();
    let (mut seen, mut in_it, mut more) = (false, false, false);
    for raw in body.replace('\r', "").split('\n') {
        let line = raw.to_lowercase();
        let head =
            line.trim_start_matches(|c: char| matches!(c, '#' | '*' | '_') || c.is_whitespace());
        if let Some(rest) = head.strip_prefix("later parts") {
            seen = true;
            in_it = true;
            let rest =
                rest.trim_start_matches(|c: char| matches!(c, '*' | '_') || c.is_whitespace());
            let rest = match rest
                .strip_prefix('(')
                .and_then(|r| r.find(')').map(|i| &r[i + 1..]))
            {
                Some(after) => after,
                None => rest,
            };
            if !none(rest) {
                more = true;
            }
            continue;
        }
        if in_it && (line.starts_with('#') || bold.is_match(&line)) {
            in_it = false;
        }
        if in_it && !none(raw) {
            more = true;
        }
    }
    seen && !more
}

/// tehas_plan_shape, whole: the session as plan_view gives it, which the
/// analytics program reads. decide's shape is its answers and notes; this
/// adds the session, its turns and options, its states and its times.
pub fn plan_view(row: &Value) -> Value {
    let mut v = shape(row);
    if v.is_null() {
        return v;
    }
    let k = |x: &str| row.get(x).cloned().unwrap_or(Value::Null);
    v["session"] = match row.get("session") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => json!(""),
        Some(x) => x.clone(),
    };
    for x in ["turns", "pending", "failed", "running"] {
        v[x] = k(x);
    }
    v["options"] = match row.get("options") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => json!([]),
        Some(x) => x.clone(),
    };
    if let Some(at) = row.get("started_at").and_then(Value::as_u64) {
        v["startedAt"] = json!(iso(at));
    }
    if let Some(c) = row.get("comment_id").filter(|c| !c.is_null()) {
        v["comment"] = c.clone();
    }
    if row.get("lost") == Some(&json!(true)) {
        v["lost"] = json!(true);
    }
    if let Some(f) = row
        .get("filed")
        .filter(|f| f.as_array().is_some_and(|a| !a.is_empty()))
    {
        v["filed"] = f.clone();
    }
    v
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Writer {
    pub(super) fn st(&self, n: u64, end: &str) -> PathBuf {
        self.home.join(format!("state/plan-{n}.json{end}"))
    }

    pub(super) fn keep(&self, n: u64) -> PathBuf {
        PathBuf::from(format!("{}-plan-{n}.keep", self.host.work_dir))
    }

    fn outcome(&self, n: u64) -> Value {
        std::fs::read_to_string(self.st(n, ".out"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(Value::Null)
    }

    /// plan-show: the session on record, or the warning that it could not
    /// be read.
    fn plan_show(&mut self, n: u64, log: &mut Vec<String>) -> Option<Value> {
        match self.calls.store(&["plan-show", &n.to_string()]) {
            Ok(v) => Some(v),
            Err(f) => {
                log.push(format!(
                    "WARN: the planning record of #{n} could not be read ({}); the next poll tries again",
                    word(&f)
                ));
                None
            }
        }
    }

    /// plan_files_rm: the files beside a session that ended.
    fn plan_files_rm(&self, n: u64) {
        let _ = std::fs::remove_file(self.st(n, ".out"));
        let _ = std::fs::remove_file(self.st(n, ".rc"));
        let _ = std::fs::remove_dir_all(self.keep(n));
        self.plan_brief_rm(n);
    }

    fn plan_brief_rm(&self, n: u64) {
        let prefix = format!("plan-{n}.brief.");
        for e in std::fs::read_dir(self.home.join("tmp"))
            .into_iter()
            .flatten()
            .flatten()
        {
            if e.file_name().to_string_lossy().starts_with(&prefix) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }

    /// A label change on an issue, journaled; false when it did not land.
    fn edit_labels(&mut self, n: u64, flags: &[(&str, &str)]) -> bool {
        let mut args: Vec<String> = vec![
            "issue".into(),
            "edit".into(),
            n.to_string(),
            "--repo".into(),
            self.repo.clone(),
        ];
        for (f, l) in flags {
            args.extend([f.to_string(), l.to_string()]);
        }
        let kind = if flags.iter().any(|(f, _)| *f == "--add-label") {
            "label-add"
        } else {
            "label-remove"
        };
        matches!(self.write(n, kind, args, None, None), Wrote::Done(_))
    }

    /// The plan_collect action: a turn that left its exit status is
    /// finished; one that did not was cut off.
    pub(super) fn plan_collect_action(&mut self, n: u64, log: &mut Vec<String>) {
        if self.st(n, ".rc").is_file() {
            self.plan_collect(n, log);
        } else {
            self.reap(
                &format!("tehas-plan-{n}"),
                &format!("the planning turn of #{n} was cut off"),
                log,
            );
            self.plan_recover(n, log);
        }
    }

    pub(super) fn plan_collect(&mut self, n: u64, log: &mut Vec<String>) {
        let rc = std::fs::read_to_string(self.st(n, ".rc")).unwrap_or_default();
        let rc = rc.trim_end_matches('\n');
        let rc = if !rc.is_empty() && rc.bytes().all(|b| b.is_ascii_digit()) {
            rc.to_string()
        } else {
            "not readable".to_string()
        };
        let Some(show) = self.plan_show(n, log) else {
            return;
        };
        let turns = show
            .pointer("/planning/turns")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let sid = s(&show, "/planning/session").to_string();
        self.plan_finish(n, &rc, turns, &sid, log);
    }

    fn plan_recover(&mut self, n: u64, log: &mut Vec<String>) {
        let _ = std::fs::remove_file(self.st(n, ".out"));
        let Some(show) = self.plan_show(n, log) else {
            return;
        };
        if show.pointer("/planning/lost") == Some(&json!(true)) {
            let hand = show.pointer("/planning/auto") == Some(&json!(true));
            self.plan_lost(n, hand, log);
            return;
        }
        log.push(format!("planning #{n}: a turn was cut off"));
        let turns = show
            .pointer("/planning/turns")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let sid = s(&show, "/planning/session").to_string();
        self.plan_finish(n, "interrupted", turns, &sid, log);
    }

    fn plan_lost(&mut self, n: u64, hand: bool, log: &mut Vec<String>) -> bool {
        let host = self.host.name.clone();
        self.plan_end(n, &format!("Tehas ended this planning session: pi-rukas has no record of it on `{host}` any more, so the last answer was not given to the planner. Apply `tehas:plan` again to start a new session."), true, hand, log)
    }

    /// plan_ended: the turn's end on record, then its exit status off. The
    /// store's answer, or None with the status kept for the next poll.
    fn plan_ended(&mut self, n: u64, args: &[String], log: &mut Vec<String>) -> Option<Value> {
        let mut all = vec!["plan-end".to_string(), n.to_string()];
        all.extend(args.iter().cloned());
        let refs: Vec<&str> = all.iter().map(String::as_str).collect();
        match self.calls.store(&refs) {
            Ok(v) => {
                let _ = std::fs::remove_file(self.st(n, ".rc"));
                Some(v)
            }
            Err(f) => {
                log.push(format!(
                    "WARN: the end of the planning turn of #{n} could not be recorded ({}); the next poll tries again",
                    word(&f)
                ));
                None
            }
        }
    }

    /// tehas_analytics_plan: the turn's end for the factory's analytics,
    /// with the session as plan_view gives it and the image's pi-rukas.
    fn analytics_plan(&mut self, n: u64, show: &Value, rc: &str, log: &mut Vec<String>) {
        if self.host.analytics.is_none() {
            return;
        }
        let facts = show.get("planning").map(plan_view).unwrap_or(Value::Null);
        let image = image_facts(&self.image_line());
        // A new file only the tehas user reads, as mktemp makes it.
        use std::io::Write as _;
        use std::os::unix::fs::OpenOptionsExt;
        let made = self.scratch("analytics-facts").and_then(|path| {
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
                .ok()?;
            if f.write_all(format!("{facts}\n").as_bytes()).is_ok() {
                Some(path)
            } else {
                let _ = std::fs::remove_file(&path);
                None
            }
        });
        let Some(path) = made else {
            log.push("WARN analytics plan: no facts written".into());
            return;
        };
        let mut args: Vec<String> = vec![
            "plan".into(),
            n.to_string(),
            "--facts".into(),
            path.to_string_lossy().into_owned(),
            "--rc".into(),
            rc.replace(' ', "-"),
        ];
        for (flag, v) in [
            ("--rukas-image", image.commit),
            ("--image", image.id),
            ("--image-built", image.built),
        ] {
            if !v.is_empty() {
                args.extend([flag.into(), v]);
            }
        }
        self.analytics(&args, log);
        let _ = std::fs::remove_file(&path);
    }

    /// What docker says of the sandbox image, one line, or nothing.
    fn image_line(&self) -> String {
        let r = crate::read::run::run(
            &self.docker,
            &[
                "image".into(),
                "inspect".into(),
                "--format".into(),
                "{{index .Config.Labels \"tehas.rukas\"}}|{{index .Config.Labels \"tehas.rukas.images\"}}|{{.Id}}|{{.Created}}"
                    .into(),
                self.host.image.clone(),
            ],
            std::time::Duration::from_secs(30),
        );
        match r.ok() {
            true => r.out.lines().next().unwrap_or("").to_string(),
            false => String::new(),
        }
    }

    pub(super) fn plan_finish(
        &mut self,
        n: u64,
        rc: &str,
        turns0: u64,
        sid: &str,
        log: &mut Vec<String>,
    ) -> bool {
        let turns = turns0 + 1;
        let Some(show) = self.plan_show(n, log) else {
            return false;
        };
        let label = s(&show, "/planning/label").to_string();
        self.analytics_plan(n, &show, rc, log);
        let how: Vec<String> = match rc {
            "0" => vec![
                "--outcome".into(),
                "finished".into(),
                "--rc".into(),
                "0".into(),
            ],
            "interrupted" => vec!["--outcome".into(), "interrupted".into()],
            "not started" => vec!["--outcome".into(), "not-started".into()],
            "not readable" => vec!["--outcome".into(), "not-readable".into()],
            _ => vec![
                "--outcome".into(),
                "failed".into(),
                "--rc".into(),
                rc.into(),
            ],
        };
        let why = match rc {
            "interrupted" => "was cut off, for example by a restart of the host".to_string(),
            "not started" => "could not be started as a unit".to_string(),
            "not readable" => "left an exit status that cannot be read".to_string(),
            _ => format!("exited with status {rc}"),
        };
        let numeric = !rc.is_empty() && rc.bytes().all(|b| b.is_ascii_digit());
        let out = self.outcome(n);
        let out_path = self.st(n, ".out").to_string_lossy().into_owned();
        let turn = vec!["--turn".to_string(), turns.to_string()];
        if rc != "0" && out.get("lost") == Some(&json!(true)) {
            let mut args = turn.clone();
            args.extend(["--outcome".into(), "lost".into()]);
            if numeric {
                args.extend(["--rc".into(), rc.into()]);
            }
            let Some(ended) = self.plan_ended(n, &args, log) else {
                return false;
            };
            log.push(format!("planning #{n}: Pi no longer has session {sid}"));
            let hand = ended.pointer("/planning/auto") == Some(&json!(true));
            return self.plan_lost(n, hand, log);
        }
        let session = if rc == "0" {
            out.get("session")
                .and_then(Value::as_str)
                .filter(|x| sid_ok(x))
                .unwrap_or("")
        } else {
            ""
        };
        if !session.is_empty() {
            let mut args = turn.clone();
            args.extend(how.iter().cloned());
            args.extend(["--session".into(), session.into()]);
            let options = out
                .pointer("/question/options")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for o in options.iter().take(MAX_OPTIONS) {
                let label = match o.get("label").filter(|_| o.is_object()) {
                    None | Some(Value::Null) | Some(Value::Bool(false)) => "-".to_string(),
                    Some(Value::String(x)) => x.clone(),
                    Some(v) => v.to_string(),
                };
                args.extend([
                    "--option".into(),
                    label.replace(['\r', '\n'], " ").replace('\0', ""),
                ]);
            }
            for f in out
                .get("filed")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(u) = f.as_str().filter(|u| filed_re().is_match(u)) {
                    args.extend(["--filed".into(), u.into()]);
                }
            }
            args.extend(["--out".into(), out_path]);
            if self.plan_ended(n, &args, log).is_none() {
                return false;
            }
            self.plan_brief_rm(n);
            self.plan_refined(n, &label, &out, log);
            self.plan_post(n, log);
            log.push(format!("done planning #{n} turn={turns} rc={rc}"));
            return true;
        }
        let sid = if sid.is_empty() {
            out.get("session").and_then(Value::as_str).unwrap_or("")
        } else {
            sid
        };
        let sid = if sid_ok(sid) {
            sid.to_string()
        } else {
            String::new()
        };
        let host = self.host.name.clone();
        if !sid.is_empty() {
            let mut args = turn.clone();
            args.extend(how.iter().cloned());
            if rc != "0" {
                args.extend(["--session".into(), sid.clone()]);
            }
            args.extend(["--out".into(), out_path]);
            if self.plan_ended(n, &args, log).is_none() {
                return false;
            }
            self.plan_brief_rm(n);
            let said = json!({"text": format!("This planning turn did not finish: it {why}. The log is `~/tehas/logs/dispatch-plan-{n}.log` on `{host}`. Reply in a comment to run the turn again with your reply as the next message, or end the session and apply `tehas:plan` to start a new one."), "question": null});
            let _ = std::fs::write(self.st(n, ".out"), format!("{said}\n"));
            self.plan_post(n, log);
            log.push(format!("done planning #{n} turn={turns} rc={rc}"));
            return true;
        }
        // No session came into being: the issue goes to a person.
        if !self.edit_labels(
            n,
            &[
                ("--remove-label", "tehas:plan"),
                ("--remove-label", "tehas:planning"),
                ("--add-label", "needs-human-attention"),
            ],
        ) {
            log.push(format!("WARN: could not hand #{n} to a person after a planning turn that left no session; the next poll tries again"));
            return false;
        }
        let mut args = turn;
        args.extend(how);
        if self.plan_ended(n, &args, log).is_none() {
            return false;
        }
        self.say(n, &format!("The planning session could not start (the runner {why}). The log is `~/tehas/logs/dispatch-plan-{n}.log` on `{host}`. Apply `tehas:plan` again to retry."), log);
        self.plan_files_rm(n);
        log.push(format!(
            "done planning #{n} turn={turns} rc={rc}, no session"
        ));
        true
    }

    /// plan_refined: a line in state/refined.jsonl for each issue a refine
    /// filed, the last 500 kept.
    fn plan_refined(&mut self, n: u64, label: &str, out: &Value, log: &mut Vec<String>) {
        if label != "tehas:refine" {
            return;
        }
        let at = iso(now());
        let lines: Vec<String> = out
            .get("filed")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|u| filed_re().is_match(u))
            .filter_map(|u| {
                let k: u64 = u.rsplit('/').next()?.parse().ok()?;
                Some(format!(
                    r#"{{"at":{},"source":{n},"filed":{k},"url":{}}}"#,
                    json!(at),
                    json!(u)
                ))
            })
            .collect();
        if lines.is_empty() {
            return;
        }
        let ledger = self.home.join("state/refined.jsonl");
        use std::io::Write as _;
        let wrote = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&ledger)
            .and_then(|mut f| f.write_all(format!("{}\n", lines.join("\n")).as_bytes()));
        if wrote.is_err() {
            log.push(format!(
                "WARN: could not note in {} what refining #{n} filed",
                ledger.display()
            ));
        }
        let all = std::fs::read_to_string(&ledger).unwrap_or_default();
        let kept: Vec<&str> = all.lines().collect();
        if kept.len() <= 500 {
            return;
        }
        let new = ledger.with_extension("jsonl.new");
        let tail = kept[kept.len() - 500..].join("\n") + "\n";
        if std::fs::write(&new, tail).is_err() || std::fs::rename(&new, &ledger).is_err() {
            let _ = std::fs::remove_file(&new);
            log.push(format!(
                "WARN: could not trim {} to its last 500 lines",
                ledger.display()
            ));
        }
    }

    /// The plan_post action, and the post after a turn: its question, then
    /// its labels, then the record.
    pub(super) fn plan_post(&mut self, n: u64, log: &mut Vec<String>) -> bool {
        let Some(show) = self.plan_show(n, log) else {
            return false;
        };
        let p = show.get("planning").cloned().unwrap_or(Value::Null);
        let turns = p.get("turns").and_then(Value::as_u64).unwrap_or(0);
        let comment = match p.get("comment_id") {
            None | Some(Value::Null) | Some(Value::Bool(false)) => String::new(),
            Some(Value::String(x)) => x.clone(),
            Some(v) => v.to_string(),
        };
        let failed = p.get("failed") == Some(&json!(true));
        let label = text(&p, "label").to_string();
        let out = self.outcome(n);
        let mut filed: Vec<&str> = out
            .get("filed")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|u| filed_re().is_match(u))
            .collect();
        filed.sort_unstable();
        filed.dedup();
        let what = filed.join(", ");
        if let (Some(title), Some(body)) = (
            out.pointer("/update/title").and_then(Value::as_str),
            out.pointer("/update/body").and_then(Value::as_str),
        ) {
            return self.plan_update(n, title, body, s(&out, "/text"), log);
        }
        if !what.is_empty() && label == "tehas:refine" {
            return self.plan_replace(n, &what, &out, log);
        }
        if comment.is_empty() {
            let line = if what.is_empty() {
                String::new()
            } else {
                format!("Filed in this turn: {what}")
            };
            let Some(id) = self.post_question(n, turns, &out, &line) else {
                log.push(format!(
                    "WARN: the planning turn of #{n} could not be posted; the next poll tries again"
                ));
                return false;
            };
            if let Err(f) = self.store_try(
                3,
                &[
                    "question-asked",
                    &n.to_string(),
                    "--id",
                    &id,
                    "--kind",
                    "plan",
                ],
            ) {
                log.push(format!(
                    "WARN: the record of #{n} could not be updated after its question was posted ({})",
                    word(&f)
                ));
                return false;
            }
        }
        let mut flags = vec![
            ("--remove-label", "tehas:planning"),
            ("--add-label", "tehas:question"),
        ];
        if failed {
            flags.push(("--add-label", "needs-human-attention"));
        }
        if !self.edit_labels(n, &flags) {
            log.push(format!("WARN: the labels of #{n} could not be changed after its question was posted; the next poll tries again"));
            return false;
        }
        if let Err(f) = self.calls.store(&["plan-end", &n.to_string(), "--posted"]) {
            log.push(format!(
                "WARN: the record of #{n} could not be updated ({})",
                word(&f)
            ));
            return false;
        }
        let _ = std::fs::remove_file(self.st(n, ".out"));
        true
    }

    /// plan_update: a drafted plan the operator chose becomes the issue's
    /// text, and the session is over. Writing it again is harmless, so a
    /// poll that fails halfway is repeated whole.
    fn plan_update(
        &mut self,
        n: u64,
        title: &str,
        body: &str,
        said: &str,
        log: &mut Vec<String>,
    ) -> bool {
        let args = vec![
            "issue".into(),
            "edit".into(),
            n.to_string(),
            "--repo".into(),
            self.repo.clone(),
            "--title".into(),
            title.into(),
            "--body-file".into(),
            "-".into(),
        ];
        if !matches!(
            self.write(n, "issue-edit", args, Some(body), None),
            Wrote::Done(_)
        ) {
            log.push(format!(
                "WARN: the plan could not be written onto #{n}; the next poll tries again"
            ));
            return false;
        }
        self.plan_end(n, said, false, false, log)
    }

    /// post_question: the turn's question comment, and its id.
    fn post_question(&mut self, n: u64, turn: u64, out: &Value, line: &str) -> Option<String> {
        if !out.is_object() {
            return None;
        }
        let body = q_render(n, turn, out, line);
        let args = vec![
            "api".into(),
            "-X".into(),
            "POST".into(),
            format!("repos/{}/issues/{n}/comments", self.repo),
            "--input".into(),
            "-".into(),
            "--jq".into(),
            ".id".into(),
        ];
        let input = json!({"body": body}).to_string();
        let Wrote::Done(said) = self.write(n, "comment-post", args, Some(&input), None) else {
            return None;
        };
        // gh prints the id alone, as --jq .id has it.
        let id = said.trim().parse::<u64>().ok()?;
        Some(id.to_string())
    }

    /// plan_replace: a refine that filed replaces its issue. The mark on the
    /// original is the step a poll that failed halfway does not repeat.
    fn plan_replace(&mut self, n: u64, what: &str, out: &Value, log: &mut Vec<String>) -> bool {
        let repo = self.repo.clone();
        let have: Vec<String> = match self.calls.gh(&[
            "issue",
            "view",
            &n.to_string(),
            "--repo",
            &repo,
            "--json",
            "labels",
        ]) {
            Ok(v) => v
                .get("labels")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|l| l.get("name").and_then(Value::as_str).map(str::to_string))
                .collect(),
            Err(_) => {
                log.push(format!(
                    "WARN: the labels of #{n} could not be read; the next poll replaces it"
                ));
                return false;
            }
        };
        let (mut later, mut new) = (false, false);
        let mut mine = Vec::new();
        let mut said = Vec::new();
        let home = format!("https://github.com/{}", repo.to_lowercase());
        for url in what.split(',').map(str::trim).filter(|u| !u.is_empty()) {
            let m = url.rsplit('/').next().unwrap_or("").to_string();
            let prefix = url.rsplit_once("/issues/").map_or(url, |(p, _)| p);
            if prefix.to_lowercase() != home {
                said.push(url.to_string());
                later = true;
                continue;
            }
            mine.push(m.clone());
            said.push(format!("#{m}"));
            let body = match self
                .calls
                .gh(&["issue", "view", &m, "--repo", &repo, "--json", "body"])
            {
                Ok(v) if v.get("body").is_some_and(|b| b.is_string() || b.is_null()) => v
                    .get("body")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim_end_matches('\n')
                    .to_string(),
                _ => {
                    later = true;
                    log.push(format!(
                        "WARN: could not read #{m}, which refining #{n} filed"
                    ));
                    continue;
                }
            };
            if !no_later_parts(&body) {
                later = true;
            }
            let first = body.replace('\r', "");
            if first.split('\n').next() == Some(format!("Replaces #{n}").as_str()) {
                continue;
            }
            new = true;
            let mn: u64 = m.parse().unwrap_or(0);
            let args = vec![
                "issue".into(),
                "edit".into(),
                m.clone(),
                "--repo".into(),
                repo.clone(),
                "--body-file".into(),
                "-".into(),
            ];
            let text = format!("Replaces #{n}\n\n{body}");
            if !matches!(
                self.write(mn, "issue-edit", args, Some(&text), None),
                Wrote::Done(_)
            ) {
                log.push(format!(
                    "WARN: could not say on #{m} that it replaces #{n}; the next poll replaces it"
                ));
                return false;
            }
            let id = self
                .calls
                .gh(&["api", &format!("repos/{repo}/issues/{m}")])
                .ok()
                .and_then(|v| v.get("id").and_then(Value::as_u64));
            let linked = id.is_some_and(|id| {
                let args = vec![
                    "api".into(),
                    "-X".into(),
                    "POST".into(),
                    format!("repos/{repo}/issues/{n}/sub_issues"),
                    "-F".into(),
                    format!("sub_issue_id={id}"),
                ];
                matches!(
                    self.write(n, "sub-issue-add", args, None, None),
                    Wrote::Done(_)
                )
            });
            if !linked {
                log.push(format!("WARN: could not link #{m} as a sub-issue of #{n}"));
            }
        }
        if mine.is_empty() {
            later = true;
        }
        let has = |l: &str| have.iter().any(|x| x == l);
        let mut mark = "";
        if has("tehas:refined") {
            mark = "tehas:refined";
        }
        if has("tehas:replaced") {
            mark = "tehas:replaced";
        }
        if new || mark.is_empty() {
            let (m, other) = if later {
                ("tehas:refined", "tehas:replaced")
            } else {
                ("tehas:replaced", "tehas:refined")
            };
            mark = m;
            if !self.edit_labels(
                n,
                &[
                    ("--remove-label", "tehas:question"),
                    ("--remove-label", "tehas:planning"),
                    ("--remove-label", other),
                    ("--add-label", mark),
                ],
            ) {
                log.push(format!(
                    "WARN: could not mark #{n} {mark}; the next poll replaces it"
                ));
                return false;
            }
            let words = s(out, "/text").trim_end_matches('\n');
            let tail = if words.is_empty() {
                String::new()
            } else {
                format!("\n\n{words}")
            };
            self.say(n, &format!("Replaced by {}.{tail}", said.join(", ")), log);
        }
        if mark == "tehas:replaced" {
            let args = vec![
                "issue".into(),
                "close".into(),
                n.to_string(),
                "--repo".into(),
                repo.clone(),
                "--reason".into(),
                "not planned".into(),
            ];
            if !matches!(
                self.write(n, "issue-close", args, None, None),
                Wrote::Done(_)
            ) {
                log.push(format!("WARN: could not close #{n}"));
            }
        }
        if !self.plan_close(n, log) {
            return false;
        }
        log.push(format!("planning #{n} ended: {mark} by {what}"));
        true
    }

    /// plan_end: the planning labels off, the session closed, then said. A
    /// lost session is closed before it is said, so it is said once.
    pub(super) fn plan_end(
        &mut self,
        n: u64,
        said: &str,
        lost: bool,
        hand: bool,
        log: &mut Vec<String>,
    ) -> bool {
        let person = if hand {
            "--add-label"
        } else {
            "--remove-label"
        };
        if !self.edit_labels(
            n,
            &[
                ("--remove-label", "tehas:question"),
                ("--remove-label", "tehas:planning"),
                ("--remove-label", "tehas:plan"),
                (person, "needs-human-attention"),
            ],
        ) {
            log.push(format!(
                "WARN: could not take the planning labels off #{n}, so its session is not ended yet"
            ));
            return false;
        }
        if !self.plan_close(n, log) {
            if lost {
                return false;
            }
            self.say(n, said, log);
            return true;
        }
        self.say(n, said, log);
        log.push(format!("planning #{n} ended"));
        true
    }

    /// plan_close: the session ended on record, and its files gone.
    pub(super) fn plan_close(&mut self, n: u64, log: &mut Vec<String>) -> bool {
        if let Err(f) = self.calls.store(&["plan-end", &n.to_string(), "--close"]) {
            log.push(format!(
                "WARN: the session of #{n} could not be ended on the state store ({}); the next poll tries again",
                word(&f)
            ));
            return false;
        }
        self.plan_files_rm(n);
        true
    }
}

/// The sandbox image as a planning turn's record names it.
#[derive(Debug, Default, PartialEq)]
struct ImageFacts {
    commit: String,
    id: String,
    built: String,
}

/// Reads `<tehas.rukas>|<tehas.rukas.images>|<id>|<created>`. A commit or id
/// of another shape is left out, and the time goes with the id; the
/// analytics program reads the time and leaves out one it cannot read.
fn image_facts(line: &str) -> ImageFacts {
    let hex = |s: &str| s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    let mut f = line.split('|');
    let commit = f.next().unwrap_or("");
    let _images = f.next();
    let id = f.next().unwrap_or("");
    let built = f.next().unwrap_or("");
    let mut out = ImageFacts::default();
    if (7..=40).contains(&commit.len()) && hex(commit) {
        out.commit = commit.to_string();
    }
    if id
        .strip_prefix("sha256:")
        .is_some_and(|h| h.len() == 64 && hex(h))
    {
        out.id = id.to_string();
        if built.len() <= 40 && !built.is_empty() && !built.starts_with('-') {
            out.built = built.to_string();
        }
    }
    out
}

#[cfg(test)]
mod image_tests {
    use super::*;

    #[test]
    fn the_image_line_gives_commit_id_and_build_time() {
        let id = format!("sha256:{}", "e".repeat(64));
        assert_eq!(
            image_facts(&format!("abcdef1|1|{id}|2026-10-29T14:44:16.5Z")),
            ImageFacts {
                commit: "abcdef1".into(),
                id: id.clone(),
                built: "2026-10-29T14:44:16.5Z".into(),
            }
        );
        // An image built before the labels: no commit, still an id.
        assert_eq!(image_facts(&format!("||{id}|2026-10-29T14:44:16Z")).id, id);
        // The old two-field answer, and an id of another shape, give no id
        // and so no time.
        for line in [
            "abcdef1|1".to_string(),
            "abcdef1|1|sha256:short|2026-10-29T14:44:16Z".into(),
            format!("abcdef1|1|{}|2026-10-29T14:44:16Z", "e".repeat(64)),
            format!("abcdef1|1|sha256:{}|2026-10-29T14:44:16Z", "E".repeat(64)),
        ] {
            let f = image_facts(&line);
            assert_eq!(f.commit, "abcdef1", "{line}");
            assert!(f.id.is_empty() && f.built.is_empty(), "{line}");
        }
        assert_eq!(image_facts(&format!("xyz|1|{id}|-x")).built, "");
        assert_eq!(image_facts(""), ImageFacts::default());
    }
}
