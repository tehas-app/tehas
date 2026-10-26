//! A cycle's report, decided: what libexec/cycle does once tehas work has
//! ended, from what the cycle left (`Facts`) and the two questions it asks
//! GitHub on the way (`Ask`). It writes nothing. Its answer is the lines it
//! logs while it reads, the writes in their order, and what goes on the
//! cycle's record, in the analytics and to the improvement loop.
//!
//! The order of the bash is kept, and so are its words: the report of the
//! Rust and the report of the bash are compared line by line.

use crate::facts::{first_bytes, line, two_lines, words, Facts, File};
use crate::runner::{self, cut, kept, names_issue};
use serde_json::{json, Map, Value};
use tehas_core::question::{q_clean, q_render, Kind};

/// The two questions a report asks GitHub, as the bash asks them: each
/// with its tries and bounds, which are the caller's.
pub trait Ask {
    /// The bot's pull requests on a branch, in any state; None when GitHub
    /// would not list them.
    fn prs(&mut self, branch: &str) -> Option<Vec<Value>>;
    /// Whether the issue carries needs-human-attention now; None when its
    /// labels could not be read.
    fn attention(&mut self) -> Option<bool>;
}

/// GitHub's answers as facts hold them under "github": "prs", the list (null
/// when GitHub would not give it), and "attention", whether the issue
/// carries needs-human-attention (null when its labels could not be read).
pub struct Given(pub Value);

impl Ask for Given {
    fn prs(&mut self, _branch: &str) -> Option<Vec<Value>> {
        match self.0.get("prs") {
            Some(Value::Array(a)) => Some(a.clone()),
            None => Some(vec![]),
            _ => None,
        }
    }

    fn attention(&mut self) -> Option<bool> {
        match self.0.get("attention") {
            Some(Value::Bool(b)) => Some(*b),
            None => Some(false),
            _ => None,
        }
    }
}

/// The caps tehas sets for a stop of its own (tehas_own_caps).
pub const OWN_CAPS: [&str; 3] = ["wall-clock", "stall", "runner-never-started"];

/// What a label that could not be changed adds to a comment.
pub const STUCK: &str =
    " The label of this issue could not be changed. The next poll of the dispatcher hands it to a human.";

/// A park: the label, the record, then the question or the comment.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Park {
    /// The arguments of `store park`.
    pub args: Vec<String>,
    /// What the park says, before what its record and its label add.
    pub text: String,
    /// Said only when the park is on record.
    pub auto: String,
    pub handoff: String,
    pub usage: String,
    /// pi-rukas's handoff marker and tehas's park marker.
    pub marker: String,
    pub park_marker: String,
}

impl Park {
    /// The text of the park once `stuck` (the label stayed) and `on_record`
    /// (the store took the park) are known.
    pub fn parked(&self, stuck: bool, on_record: bool) -> String {
        let mut p = self.text.clone();
        if !self.auto.is_empty() {
            if on_record {
                p += &self.auto;
            } else {
                p += " Its park could not be recorded, so it waits for a person.";
            }
        }
        if stuck {
            p += STUCK;
        }
        p
    }

    /// The question of a park on record: the text the question file holds,
    /// and the body posted.
    pub fn question(&self, issue: u64, stuck: bool) -> (String, String) {
        let text = format!(
            "{}{}\n\n{}",
            self.parked(stuck, true),
            self.handoff_part(),
            self.usage
        );
        let out = json!({"text": text, "question": null, "marker": self.marker, "park": self.park_marker});
        let body = q_render(Kind::Park, issue, 1, &out, "");
        (text, body)
    }

    /// The comment of a park that asks no question: one not on record, or
    /// one whose question could not be posted. It has no boxes.
    pub fn comment(&self, stuck: bool, on_record: bool) -> String {
        let marker = if self.marker.is_empty() {
            String::new()
        } else {
            format!("{}\n", self.marker)
        };
        let words = q_clean(
            &format!(
                "{} To retry, apply `tehas:ready`.{}\n\n{}",
                self.parked(stuck, on_record),
                self.handoff_part(),
                self.usage
            ),
            60000,
        );
        let park = if self.park_marker.is_empty() {
            String::new()
        } else {
            format!("\n\n{}", self.park_marker)
        };
        format!("{marker}{words}{park}")
    }

    fn handoff_part(&self) -> String {
        if self.handoff.is_empty() {
            String::new()
        } else {
            format!("\n\n{}", self.handoff)
        }
    }
}

/// One write of the report, in its order.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// A label change on the issue, tried three times (once after a stop).
    /// One that works takes the stuck words out of what follows.
    Relabel {
        remove: Vec<String>,
        add: Vec<String>,
    },
    /// A comment on the issue: `ok` after a label change that worked,
    /// `stuck` after one that did not.
    Comment {
        ok: String,
        stuck: String,
    },
    Park(Box<Park>),
    /// A comment on the pull request.
    PrComment {
        pr: String,
        body: String,
    },
    /// The workspace removed, with the one moved aside; `warn` instead when
    /// its directory is not one tehas work marked.
    Tidy {
        remove: Vec<String>,
        warn: String,
    },
    /// tehas:refreshing taken off the pull request a refresh marked.
    RefreshOff {
        pr: String,
    },
}

/// A cycle's report.
#[derive(Clone, Debug, Default)]
pub struct Report {
    /// The lines logged while the report reads, before any write.
    pub log: Vec<String>,
    pub actions: Vec<Action>,
    /// The line logged once the writes are done.
    pub done: String,
    /// The arguments of `store cycle-end`; the paths of the result and the
    /// progress are the caller's to add, for each of `files`.
    pub end: Vec<String>,
    pub files: Vec<String>,
    /// The analytics line, without its time.
    pub history: Value,
    /// The arguments of `analytics run` after the issue, the attempt and the
    /// line.
    pub analytics: Vec<String>,
    /// The arguments of tehas_improve_record.
    pub improve: Vec<String>,
    /// Whether the status runs after it (not after a stop).
    pub status_run: bool,
    /// What GitHub was asked, in order: `prs <branch>`, `attention`.
    pub asked: Vec<String>,
}

fn re(pattern: &str, s: &str) -> bool {
    regex::Regex::new(pattern).unwrap().is_match(s)
}

/// `tr -cd 'A-Za-z0-9_./-'`, how a branch is said.
fn said_branch(b: &str) -> String {
    kept(b, |c| c.is_ascii_alphanumeric() || "_./-".contains(c))
}

/// An epoch second as `date -u -d @s +%FT%TZ`.
fn iso(secs: i64) -> String {
    tehas_core::time::iso(secs)
}

/// The fields jq gives as `def v`: no NUL, no line ends at the end.
fn v(s: &str) -> String {
    s.replace('\0', "").trim_end_matches('\n').to_string()
}

fn text(o: &Value, k: &str) -> String {
    match o.get(k) {
        Some(Value::String(s)) => s.clone(),
        None | Some(Value::Null) | Some(Value::Bool(false)) => String::new(),
        Some(x) => x.to_string(),
    }
}

/// tehas_park_marker: the fields of a park for the millwright, its free
/// text in the hash only.
pub fn park_marker(
    issue: u64,
    step: &str,
    cap: &str,
    park_reason: &str,
    kind: &str,
    cause: &str,
    provider: bool,
) -> String {
    use sha2::{Digest, Sha256};
    let low = |s: &str| {
        cut(
            &kept(s, |c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'
            }),
            40,
        )
    };
    let s = if step == "-" { "" } else { step };
    let sig = Sha256::digest(format!("{step}|{cap}|{park_reason}|{kind}|{cause}").as_bytes());
    let sig: String = sig.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "<!-- tehas:park issue={issue} step={} kind={} provider={} sig={sig} -->",
        low(s),
        low(kind),
        u8::from(provider)
    )
}

/// The report of a cycle that ended.
pub fn report(f: &Facts, ask: &mut dyn Ask) -> Report {
    let mut r = Report::default();
    let issue = f.issue;
    let warn = |r: &mut Report, s: String| r.log.push(format!("WARN: {s}"));
    let mins = (f.now - f.started) / 60;

    // The runner's result and its progress. Not through a link.
    let mut result = f.file("result.json").clone();
    if result == File::Link {
        warn(
            &mut r,
            format!("the result of #{issue} is a link, it is not read"),
        );
    }
    let progress = match f.file("progress.json") {
        File::Link => None,
        p => p.text(),
    };
    // A link reads as /dev/null: it is there, and holds nothing.
    let progress_seen = matches!(f.file("progress.json"), File::Link | File::Text(_));
    let no_result = !matches!(&result, File::Text(t) if !t.is_empty());
    if no_result {
        result = File::Absent;
        warn(
            &mut r,
            format!("the result of #{issue} holds nothing, so its step is read from its run log"),
        );
    }
    let outcome = result
        .text()
        .and_then(|t| runner::outcome(t, issue))
        .unwrap_or_else(|| json!({}));
    let w = |s: &str| kept(s, |c| c.is_ascii_alphanumeric() || "_.-".contains(c));
    let mut status = {
        let s = match outcome.get("status") {
            None | Some(Value::Null) | Some(Value::Bool(false)) => "unknown".to_string(),
            Some(x) => text(&json!({"x": x}), "x"),
        };
        v(&w(&s))
    };
    let mut step = v(&w([outcome.get("failedStep"), outcome.get("lastStep")]
        .iter()
        .filter_map(|x| x.and_then(Value::as_str))
        .find(|s| !s.is_empty())
        .unwrap_or("-")));
    let result_branch = v(&text(&outcome, "branch"));
    let build = v(&text(&outcome, "buildCommit"));
    let handoff = {
        let b = text(&outcome, "handoffBody");
        let marker = regex::Regex::new(r"\A<!-- pi-rukas:handoff [^\n]*-->\n*").unwrap();
        v(&marker.replace(&b, ""))
    };
    let park_reason = v(&cut(
        &kept(&text(&outcome, "parkReason"), |c| {
            c.is_ascii_lowercase() || c == '-'
        }),
        40,
    ));
    let mut cap = v(&cut(
        &kept(&text(&outcome, "cap"), |c| {
            c.is_ascii_alphanumeric() || "_.:-".contains(c)
        }),
        80,
    ));
    if status.is_empty() {
        status = "unknown".into();
    }
    if step.is_empty() {
        step = "-".into();
    }
    // tehas work's word for how the container ended, for a cycle that wrote
    // no result.
    let mut work_status = String::new();
    if no_result {
        if let Some(t) = f.file("status").text() {
            work_status = line(t);
        }
        if work_status == "killed" {
            status = "killed".into();
        }
    }
    let watched = f.file("runner_exit").text().is_some();
    // The note of a container that never started.
    let mut nostart_rc = String::new();
    if let Some(t) = f.file("nostart").text() {
        nostart_rc = words(t, Some(100)).first().cloned().unwrap_or_default();
        if !re(r"^[1-9][0-9]{0,2}$", &nostart_rc) {
            nostart_rc.clear();
        }
    }
    let nostart_seen = !nostart_rc.is_empty();
    let read_log_steps = no_result && watched && progress_seen;
    // The step of tehas work's stall file and of its timed file.
    let secs = |s: &str| re(r"^(0|[1-9][0-9]{0,8})$", s);
    let (stall_secs, mut stall_step) = match f.file("stalled").text() {
        Some(t) => {
            let (a, b) = two_lines(t);
            let mut s = cut(
                &kept(&b, |c| c.is_ascii_alphanumeric() || "_.-".contains(c)),
                40,
            );
            if s == "-" {
                s.clear();
            }
            (a, s)
        }
        None => (String::new(), String::new()),
    };
    let (timed_secs, mut timed_step_file) = match f.file("timed").text() {
        Some(t) => {
            let ws = words(t, Some(200));
            let s = ws.get(2).cloned().unwrap_or_default();
            let ok = re(r"^[A-Za-z0-9_.:-]{1,40}$", &s) && s != "-";
            (
                ws.first().cloned().unwrap_or_default(),
                if ok { s } else { String::new() },
            )
        }
        None => (String::new(), String::new()),
    };
    let mut step_wanted = step == "-";
    if !nostart_rc.is_empty() {
        step_wanted = false;
    }
    if !secs(&stall_secs) {
        stall_step.clear();
    }
    if !secs(&timed_secs) {
        timed_step_file.clear();
    }
    if !stall_step.is_empty() || !timed_step_file.is_empty() {
        step_wanted = false;
    }
    // The runner's own status, as tehas work saw it.
    let runner_rc: i64 = if watched {
        let t = line(f.file("runner_exit").text().unwrap_or(""));
        if re(r"^(0|[1-9][0-9]{0,2})$", &t) {
            t.parse().unwrap_or(f.rc)
        } else {
            f.rc
        }
    } else {
        f.rc
    };
    // The run log of a runner that wrote no result.
    let mut log_branch = String::new();
    let mut log_read = String::new();
    let mut log_words = false;
    let mut log_from = "result";
    if read_log_steps {
        if let Some(l) = &f.run_log {
            match &l.text {
                Some(t) => {
                    let run = runner::run(t, issue);
                    log_read = run.read.clone();
                    log_words = true;
                    if step_wanted && !run.step.is_empty() {
                        step = run.step;
                        log_from = "log";
                    }
                    log_branch = run.branch;
                }
                None => warn(
                    &mut r,
                    format!(
                        "the run log {} of #{issue} could not be read, so its step and its branch are not known",
                        l.name
                    ),
                ),
            }
        }
    }
    if step.is_empty() {
        step = "-".into();
    }
    let mut step_from = log_from;
    if step == "-" {
        step_from = "result";
    }
    if !nostart_rc.is_empty() && no_result {
        step = "no-step".into();
        step_from = "nostart";
    }
    if step == "-" && !stall_step.is_empty() {
        step = stall_step.clone();
        step_from = "stalled";
    }
    if f.rc == 137 && !watched && (status == "running" || status == "unknown") {
        status = "killed".into();
    }
    let count = |name: &str| {
        let n = f
            .file(name)
            .text()
            .map(|t| first_bytes(t, 16))
            .unwrap_or_default();
        if secs(&n) {
            n
        } else {
            "0".to_string()
        }
    };
    let kills = count("oom");
    let mut killed = String::new();
    if kills != "0" {
        killed = format!(" {kills} processes were killed for memory in its container.");
    }
    let forks = count("forks");
    // The stall file read again for its stop: the seconds, and the step only
    // with a stop of its own.
    let (stalled, stop_step) = match f.file("stalled").text() {
        Some(t) => {
            let (a, b) = two_lines(t);
            let n = if secs(&a) { a } else { "0".into() };
            let at = cut(
                &kept(&b, |c| c.is_ascii_alphanumeric() || "_.-".contains(c)),
                40,
            );
            let s = if n != "0" && !at.is_empty() && at != "-" {
                at
            } else {
                String::new()
            };
            (n, s)
        }
        None => ("0".into(), String::new()),
    };
    if step == "-" && !stop_step.is_empty() {
        step = stop_step;
        step_from = "stalled";
    }
    // The image on the record of this attempt.
    let (mut image_id, mut image_rukas) = (String::new(), String::new());
    match &f.record {
        Ok(rec) => {
            if let Some(c) = rec.get("cycle").filter(|c| {
                c.is_object()
                    && c.get("attempt").and_then(Value::as_u64) == Some(f.attempt)
                    && !c.get("image").is_none_or(Value::is_null)
            }) {
                let line = format!("{} {}", text(c, "image"), text(c, "rukas_commit"));
                let ws = words(&line, None);
                image_id = ws.first().cloned().unwrap_or_default();
                image_rukas = ws.get(1).cloned().unwrap_or_default();
            }
            if !re(r"^sha256:[0-9a-f]{64}$", &image_id) {
                image_id.clear();
            }
            if !re(r"^[0-9a-f]{7,40}$", &image_rukas) {
                image_rukas.clear();
            }
        }
        Err(word) => warn(
            &mut r,
            format!("the record of #{issue} could not be read for its image ({word})"),
        ),
    }
    let s60 = |s: &str| s.parse::<i64>().unwrap_or(0) / 60;
    if stalled != "0" {
        if ["running", "unknown", "interrupted"].contains(&status.as_str()) {
            status = "stalled".into();
        }
        killed += &format!(
            " None of its logs changed for {} min at step `{step}`, so tehas stopped it before its time limit.",
            s60(&stalled)
        );
    }
    // The time limit.
    let (mut timed, mut timed_by, mut timed_step) = ("0".to_string(), String::new(), String::new());
    if let Some(t) = f.file("timed").text() {
        let ws = words(t, Some(200));
        timed = ws.first().cloned().unwrap_or_default();
        timed_by = ws.get(1).cloned().unwrap_or_default();
        timed_step = ws.get(2).cloned().unwrap_or_default();
        if !secs(&timed) {
            timed = "0".into();
        }
        if !(re(r"^[A-Za-z0-9_.:-]{1,40}$", &timed_step) && timed_step != "-") {
            timed_step.clear();
        }
    }
    if timed != "0" {
        if ["running", "unknown", "interrupted"].contains(&status.as_str()) {
            status = "timeout".into();
        }
        if step == "-" && !timed_step.is_empty() {
            step = timed_step.clone();
            step_from = "timed";
        }
        if timed_by == "runner" {
            killed += &format!(
                " It reached its time limit of {} min at step `{step}`, and the runner stopped there.",
                s60(&timed)
            );
        } else {
            killed += &format!(
                " It ran past its time limit of {} min at step `{step}`, and the runner did not stop on its own, so tehas stopped it.",
                s60(&timed)
            );
        }
    }
    // The marker of `tehas stop`.
    let mut stopped_by = String::new();
    let mut stopped_at_shown = String::new();
    let mut stopped_by_person = false;
    if let Some(t) = f.file("stopped").text() {
        let ws = words(t, Some(200));
        let at = ws.first().cloned().unwrap_or_default();
        stopped_by = ws.get(1).cloned().unwrap_or_default();
        if !re(r"^[A-Za-z0-9._-]{1,64}$", &stopped_by) {
            stopped_by.clear();
        }
        if re(r"^(0|[1-9][0-9]{0,9})$", &at) {
            stopped_at_shown = iso(at.parse().unwrap_or(0));
        }
    }
    if !stopped_at_shown.is_empty() {
        if stopped_by.is_empty() {
            stopped_by = "name not recorded".into();
        }
        stopped_by_person = true;
        killed += &format!(
            " A person stopped it with `tehas stop` ({stopped_by}, at {stopped_at_shown})."
        );
    }
    if forks != "0" {
        killed += &format!(
            " Its container reached its limit of processes and {forks} starts of a process were refused: what the cycle ran after that may not have run."
        );
    }
    let mut branch = result_branch;
    if !(runner::branch_of(issue, &branch) || re(r"^[A-Za-z0-9][A-Za-z0-9._/-]{0,79}$", &branch)) {
        branch.clear();
    }
    if branch.is_empty() {
        branch = log_branch;
    }
    let build = if re(r"^[0-9a-f]{40}$", &build) {
        build
    } else {
        String::new()
    };
    let [review_held, mut reason, failure_from, failure_kind, failure_role, failure_cause] =
        runner::progress_fields(progress);
    let mut provider = false;
    if failure_from == "result" {
        provider = failure_kind == "provider";
    } else {
        if cap.starts_with("step-failed:") && reason.is_empty() {
            warn(
                &mut r,
                format!(
                    "#{issue} ended at {cap} and no reason was found in its progress: an outage is not told from a failure (pi-rukas#1367)"
                ),
            );
        }
        if (cap.starts_with("step-failed:") || cap == "adversarial-infra-failure")
            && re(
                r"(?i)provider|time(d)? ?out|connection|ECONNRE|ETIMEDOUT|(^|[^0-9])5[0-9]{2}([^0-9]|$)",
                &reason,
            )
        {
            provider = true;
        }
    }
    // A stop of tehas's own is its own cap.
    let mut own = "";
    let mut stop_why = String::new();
    if nostart_seen && no_result {
        own = "runner-never-started";
        stop_why = format!("the container ended with the exit status {nostart_rc} and wrote no line of its stream, so the driver wrote no event and no step started");
    } else if timed != "0" && (cap.is_empty() || status == "timeout") {
        own = "wall-clock";
        stop_why = if timed_by == "runner" {
            format!("the cycle reached its time limit, TEHAS_WORK_TIMEOUT={timed}s, at step {step}")
        } else {
            format!("tehas work stopped the container at the time limit of {timed}s")
        };
    } else if stalled != "0" && (cap.is_empty() || status == "stalled") {
        own = "stall";
        stop_why = format!("tehas work stopped the container when no log changed for {stalled}s");
    } else if stopped_by_person && cap.is_empty() {
        let tail = if reason.is_empty() {
            String::new()
        } else {
            format!("; {reason}")
        };
        reason = cut_bytes(
            &format!("tehas stop ended the cycle, run by {stopped_by} at {stopped_at_shown}, step {step} in flight{tail}"),
            200,
        );
    }
    if !own.is_empty() {
        if !((own == "wall-clock" && timed_by == "runner") || own == "runner-never-started") {
            stop_why += &format!(", step {step} in flight");
        }
        if !cap.is_empty() {
            stop_why += &format!("; the runner had named the cap {cap}");
        }
        let tail = if reason.is_empty() {
            String::new()
        } else {
            format!("; {reason}")
        };
        reason = cut_bytes(&format!("{stop_why}{tail}"), 200);
        cap = own.to_string();
    }
    let (usage, unreadable) = runner::usage(&outcome, progress);
    if unreadable {
        warn(
            &mut r,
            format!("the progress of #{issue} could not be read, so its rows are not summed into the totals"),
        );
    }
    // The bot's pull requests on the branch.
    let mut prs: Vec<Value> = vec![];
    let mut unread = false;
    if !branch.is_empty() {
        r.asked.push(format!("prs {branch}"));
        match ask.prs(&branch) {
            Some(p) => prs = p,
            None => {
                unread = true;
                warn(
                    &mut r,
                    format!(
                        "the pull requests of {} could not be read",
                        said_branch(&branch)
                    ),
                );
            }
        }
    }
    let url_in = |st: &str| {
        prs.iter()
            .filter(|p| {
                p.is_object()
                    && text(p, "state").to_ascii_uppercase() == st
                    && names_issue(p, issue)
            })
            .filter_map(|p| p.get("url").and_then(Value::as_str))
            .find(|u| re(r"^https://[A-Za-z0-9./_-]+$", u))
            .map(v)
            .unwrap_or_default()
    };
    let mut pr = url_in("OPEN");
    let merged_pr = url_in("MERGED");
    let closed_pr = url_in("CLOSED");
    let mine: Vec<bool> = prs
        .iter()
        .filter(|p| p.is_object())
        .map(|p| names_issue(p, issue))
        .collect();
    let foreign = if mine.iter().any(|m| !m) && !mine.iter().any(|m| *m) {
        format!(
            " The branch `{}` has a pull request of the bot that does not name this issue, so it is not taken as this cycle's.",
            said_branch(&branch)
        )
    } else {
        String::new()
    };

    let mut summary = format!("status `{status}`, last step `{step}`, {mins} min");
    if step != "-" {
        match step_from {
            "log" => {
                summary = format!("status `{status}`, last step `{step}` read from its run log, {mins} min")
            }
            "stalled" | "timed" => {
                summary = format!("status `{status}`, last step `{step}` tehas work saw in flight, {mins} min")
            }
            "nostart" => {
                summary = format!("status `{status}`, no step started, the container of the cycle ended before the driver wrote an event, {mins} min")
            }
            _ => {}
        }
    }
    let mut why = String::new();
    if OWN_CAPS.contains(&cap.as_str()) {
        why = format!(" tehas stopped the cycle: the cap `{cap}`.");
    } else if !cap.is_empty() {
        let pr_why = if park_reason.is_empty() {
            String::new()
        } else {
            format!(" (`{park_reason}`)")
        };
        why = format!(" pi-rukas stopped on the cap `{cap}`{pr_why}.");
    }
    if cap == "deadline" {
        why += " It reached its deadline, a quarter of its time limit before the end, and started no new step.";
    }
    if own.is_empty()
        && failure_from == "result"
        && !failure_kind.is_empty()
        && failure_kind != "cap"
        && !provider
    {
        let role = if failure_role.is_empty() {
            String::new()
        } else {
            format!(", role `{failure_role}`")
        };
        let cause = if failure_cause.is_empty() {
            String::new()
        } else {
            format!(", cause `{failure_cause}`")
        };
        let rs = if reason.is_empty() {
            String::new()
        } else {
            format!(": {reason}")
        };
        why += &format!(" It failed: kind `{failure_kind}`{role}{cause}{rs}.");
    }
    let marker = if handoff.is_empty() {
        String::new()
    } else {
        format!(
            "<!-- pi-rukas:handoff issue={issue} cap={} -->",
            if cap.is_empty() { "none" } else { &cap }
        )
    };
    let done_why = f
        .file("done")
        .text()
        .map(|t| first_bytes(t, 16))
        .filter(|w| ["closed", "merged", "pr-closed"].contains(&w.as_str()))
        .unwrap_or_default();
    // The approval this start used.
    let (mut refreshed, mut base_at, mut base_by) = (String::new(), String::new(), String::new());
    match &f.approval {
        Ok(a) => {
            let cur = a.get("current").filter(|c| {
                c.is_object() && c.get("used_by").and_then(Value::as_u64) == Some(f.attempt)
            });
            if let Some(c) = cur {
                let dash = |x: Option<&Value>| match x {
                    None | Some(Value::Null) | Some(Value::Bool(false)) => "-".to_string(),
                    Some(Value::String(s)) => s.clone(),
                    Some(o) => o.to_string(),
                };
                let rp = if c.get("how").and_then(Value::as_str) == Some("refresh") {
                    match c.get("refresh_pr") {
                        None | Some(Value::Null) | Some(Value::Bool(false)) => c
                            .pointer("/input/refresh_of/pr")
                            .cloned()
                            .unwrap_or(Value::Null),
                        Some(x) => x.clone(),
                    }
                } else {
                    Value::Null
                };
                let rests = a.get("rests_on").cloned().unwrap_or(Value::Null);
                let joined = format!(
                    "{} {} {}",
                    dash(Some(&rp)),
                    dash(rests.get("at")),
                    dash(rests.get("by"))
                );
                let ws = words(&joined, None);
                refreshed = ws.first().cloned().unwrap_or_default();
                base_at = ws.get(1).cloned().unwrap_or_default();
                base_by = ws.get(2..).map(|x| x.join(" ")).unwrap_or_default();
            }
        }
        Err(word) => warn(
            &mut r,
            format!("the approval of #{issue} could not be read for its report ({word})"),
        ),
    }
    if !re(r"^[1-9][0-9]{0,9}$", &refreshed) {
        refreshed.clear();
    }
    if !re(r"^[1-9][0-9]{0,10}$", &base_at) {
        base_at.clear();
    }

    let mut parked_now = false;
    let mut never_ran = false;
    let died_with_open_pr = log_words
        && log_read == "whole"
        && !pr.is_empty()
        && merged_pr.is_empty()
        && closed_pr.is_empty();
    let pm = park_marker(
        issue,
        &step,
        &cap,
        &park_reason,
        &failure_kind,
        &failure_cause,
        provider,
    );
    let base = Park {
        args: vec![],
        text: String::new(),
        auto: String::new(),
        handoff: handoff.clone(),
        usage: usage.clone(),
        marker: marker.clone(),
        park_marker: pm,
    };
    // park_args: the record of a park, by tehas's word for it or else the
    // cap, else the status.
    let park_args = |word: &str, cap: &str, status: &str, step: &str, pr: &str| {
        let mut why = word.to_string();
        if why.is_empty() {
            why = if cap.is_empty() { status } else { cap }.to_string();
            if why == "provider-outage" {
                why = "cap:provider-outage".into();
            }
        }
        let mut args = vec![
            issue.to_string(),
            "--attempt".into(),
            f.attempt.to_string(),
            "--reason".into(),
            why,
        ];
        if !step.is_empty() {
            args.extend(["--step".into(), step.to_string()]);
        }
        if let Some(n) = pr.strip_prefix(&format!("https://github.com/{}/pull/", f.repo)) {
            if re(r"^[1-9][0-9]{0,9}$", n) {
                args.extend(["--pr".into(), n.to_string()]);
            }
        }
        args
    };
    let park = |args: Vec<String>, text: String, auto: String| {
        Action::Park(Box::new(Park {
            args,
            text,
            auto,
            ..base.clone()
        }))
    };
    let running = || Action::Relabel {
        remove: vec!["tehas:running".into()],
        add: vec![],
    };
    let to = |label: &str| Action::Relabel {
        remove: vec!["tehas:running".into()],
        add: vec![label.into()],
    };
    let comment = |body: &dyn Fn(&str) -> String| Action::Comment {
        ok: body(""),
        stuck: body(STUCK),
    };
    let log_where = format!(
        "the run log is `~/tehas/logs/dispatch-work-{issue}.log` on `{}`",
        f.hostname
    );
    let mut kept_at = if f.workdir_dir {
        f.workdir.clone()
    } else if f.before_retry_dir {
        format!("{}.before-retry", f.workdir)
    } else {
        String::new()
    };
    if !f.home.is_empty() {
        if let Some(rest) = kept_at.strip_prefix(&format!("{}/", f.home)) {
            kept_at = format!("~/{rest}");
        }
    }
    let kept_said = if kept_at.is_empty() {
        "This cycle left no workspace on this host".to_string()
    } else {
        format!("The workspace is kept at `{kept_at}`")
    };
    let parent = f
        .workdir
        .rsplit_once('/')
        .map(|(p, _)| p.to_string())
        .unwrap_or_default();
    let tidy = Action::Tidy {
        remove: if f.workspaces_marked {
            vec![f.workdir.clone(), format!("{}.before-retry", f.workdir)]
        } else {
            vec![]
        },
        warn: if f.workspaces_marked {
            String::new()
        } else {
            format!(
                "WARN: {parent} is not marked by tehas work, leaving the workspace {}",
                f.workdir
            )
        },
    };
    let rc = f.rc;
    let handoff_part = if handoff.is_empty() {
        String::new()
    } else {
        format!("\n\n{handoff}")
    };
    let marker_line = if marker.is_empty() {
        String::new()
    } else {
        format!("{marker}\n")
    };
    let open_pr = if pr.is_empty() {
        String::new()
    } else {
        format!(" Its branch has an open pull request: {pr}")
    };

    if !merged_pr.is_empty() {
        r.actions.push(running());
        if rc == 0 && status == "merged" {
            r.actions.push(comment(&|s| {
                format!("Tehas cycle finished: {summary}. The change was merged: {merged_pr}{killed}{s}")
            }));
            r.actions.push(tidy);
        } else {
            r.actions.push(comment(&|s| {
                format!("Tehas cycle ended: its pull request {merged_pr} was merged while the cycle ran (last step `{step}`, {mins} min, runner exit {runner_rc}). Nothing more is needed here. {kept_said}.{killed}{s}")
            }));
        }
        status = "merged".into();
        pr = merged_pr.clone();
    } else if unread {
        parked_now = true;
        r.actions.push(park(park_args("", &cap, &status, &step, &pr), format!(
            "Tehas cycle stopped, and its pull request could not be looked up: GitHub did not list the pull requests of the branch `{}` ({summary}, runner exit {runner_rc}). Look for one there before you retry. {kept_said}; {log_where}.{killed}",
            said_branch(&branch)
        ), String::new()));
    } else if rc == 0 && status == "merged" {
        parked_now = true;
        let b = if branch.is_empty() {
            "none".to_string()
        } else {
            branch.clone()
        };
        r.actions.push(park(park_args("", &cap, &status, &step, &pr), format!(
            "Tehas cycle stopped: its state says `merged`, but the bot has no merged pull request for this issue on `{}` ({summary}). Nothing was removed; {log_where}.{killed}{open_pr}{foreign}",
            said_branch(&b)
        ), String::new()));
    } else if (rc == 10 || rc == 11)
        && status == "handoff"
        && !pr.is_empty()
        && cap == "awaiting-human-merge"
        && review_held == "false"
        && {
            r.asked.push("attention".into());
            match ask.attention() {
                Some(carries) => !carries,
                None => {
                    warn(&mut r, format!("the labels of #{issue} could not be read, so its pull request is not said to wait only for a merge"));
                    false
                }
            }
        }
    {
        r.actions.push(to("tehas:awaiting-merge"));
        r.actions.push(comment(&|s| {
            format!("Tehas cycle finished: {summary}. Pull request waiting for a human: {pr}{killed}{s}")
        }));
        let u = if usage.is_empty() {
            "No token usage was recorded for this cycle.".to_string()
        } else {
            usage.clone()
        };
        r.actions.push(Action::PrComment {
            pr: pr.clone(),
            body: format!(
                "{marker_line}{}",
                q_clean(
                    &format!("Tehas cycle for #{issue}: {summary}.{handoff_part}\n\n{u}"),
                    60000
                )
            ),
        });
        r.actions.push(tidy);
    } else if (rc == 10 || rc == 11) && status == "handoff" && !pr.is_empty() {
        let held = if review_held != "false" && cap == "awaiting-human-merge" {
            " Its review ran out of rounds with findings still open, so it is not waiting only for a merge."
        } else {
            ""
        };
        if cap.is_empty() {
            why = " The result names no cap, so it is not taken for a pull request that waits only for a merge.".into();
        }
        parked_now = true;
        r.actions.push(park(park_args("", &cap, &status, &step, &pr), format!(
            "Tehas cycle handed off with its pull request open: {summary} (runner exit {runner_rc}).{why}{held} Read the pull request before you retry: {pr}. {kept_said}; {log_where}.{killed}{foreign}"
        ), String::new()));
    } else if done_why == "closed" {
        r.actions.push(running());
        r.actions.push(comment(&|s| {
            format!("Tehas stopped the cycle: this issue was closed while it ran ({summary}). Nothing more is needed here. {kept_said}; {log_where}.{killed}{s}")
        }));
        status = "closed".into();
    } else if pr.is_empty() && !closed_pr.is_empty() && rc != 4 {
        r.actions.push(to("needs-human-attention"));
        r.actions.push(comment(&|s| {
            format!(
                "{marker_line}{}",
                q_clean(&format!("Tehas cycle stopped: its pull request {closed_pr} was closed without being merged ({summary}, runner exit {runner_rc}).{why} {kept_said}; {log_where}.{killed}{s} To retry, apply `tehas:ready`.{handoff_part}"), 60000)
            )
        }));
    } else if died_with_open_pr {
        r.actions.push(to("needs-human-attention"));
        r.actions.push(comment(&|s| {
            format!(
                "{marker_line}{}",
                q_clean(&format!("Tehas cycle handed off with its pull request open and its review unfinished: {summary} (runner exit {runner_rc}). Its runner wrote no result, so tehas read its run log for the step it was in and GitHub for the bot's pull request on its branch: {pr}. Read the pull request before you retry: a clean start redoes what it already holds. {kept_said}; {log_where}.{killed}{s}"), 60000)
            )
        }));
        status = "handoff".into();
    } else {
        let handoff_said = if handoff.is_empty() {
            ""
        } else {
            " The handoff of pi-rukas follows."
        };
        let mut parked = format!(
            "Tehas cycle stopped without a pull request: {summary} (runner exit {runner_rc}).{why}{handoff_said} {kept_said}; {log_where}.{killed}{open_pr}{foreign}"
        );
        let mut park_why = "";
        let mut auto = String::new();
        if rc == 4 {
            never_ran = true;
            let (mut by, mut epoch) = (f.start_by.clone(), f.approval_at.clone());
            match f.start_how.as_str() {
                "refresh" => {
                    by = base_by.clone();
                    epoch = base_at.clone();
                }
                "outage-retry" => epoch.clear(),
                _ => {}
            }
            if !re(r"^[A-Za-z0-9-]{1,39}$", &by) {
                by.clear();
            }
            let at = epoch.parse::<i64>().ok().map(iso).unwrap_or_default();
            let said = if !by.is_empty() && !at.is_empty() {
                format!(" @{by} approved this start at {at}; tehas keeps no record of an approval that started nothing, so a fresh `tehas:ready` asks for another start.")
            } else if !by.is_empty() {
                format!(" @{by} approved this start; tehas keeps no record of an approval that started nothing, so a fresh `tehas:ready` asks for another start.")
            } else {
                " No approval of this start is recorded on this host: a fresh `tehas:ready` is needed.".to_string()
            };
            parked = format!("Tehas started no cycle: an image or file the issue links could not be given to it, or the issue could not be read for them (runner exit {runner_rc}). Nothing of it ran: no workspace, no container, no step, no usage.{said} The comment before this one names it; {log_where}.");
        } else if provider {
            park_why = "provider-outage";
            let rs = if reason.is_empty() {
                String::new()
            } else {
                format!(": {reason}")
            };
            parked = format!("Tehas cycle parked by a provider outage: {summary} (runner exit {runner_rc}), cap `{cap}`{rs}; {log_where}.{killed}{open_pr}{foreign}");
            if f.requeued {
                parked += " It was the retry tehas made on its own after an outage, so it waits for a person.";
            } else if f.start_how == "hand" || f.start_by.is_empty() {
                parked += " It was started by hand, so it waits for a person.";
            } else {
                auto = " Tehas starts it again once on its own, when the model endpoint answers."
                    .into();
            }
        }
        parked_now = true;
        r.actions.push(park(
            park_args(park_why, &cap, &status, &step, &pr),
            parked,
            auto,
        ));
    }
    if !refreshed.is_empty() {
        r.actions.push(Action::RefreshOff {
            pr: refreshed.clone(),
        });
    }
    let pr_said = if pr.is_empty() { "none" } else { &pr };
    r.done = format!(
        "done #{issue} rc={rc} status={status} step={step} minutes={mins} pr={pr_said} oom_kills={kills} forks_refused={forks}"
    );
    if !closed_pr.is_empty() && pr.is_empty() && merged_pr.is_empty() && status != "closed" {
        parked_now = true;
    }
    // The end on the cycle's record.
    let mut end: Vec<String> = vec![
        issue.to_string(),
        "--attempt".into(),
        f.attempt.to_string(),
        "--status".into(),
        status.clone(),
        "--step".into(),
        step.clone(),
        "--rc".into(),
        rc.to_string(),
        "--oom".into(),
        kills.clone(),
        "--forks".into(),
        forks.clone(),
    ];
    if watched {
        end.extend(["--runner-exit".into(), runner_rc.to_string()]);
    }
    if work_status == "killed" {
        end.extend(["--host-saw".into(), "killed".into()]);
    }
    if stalled != "0" {
        end.extend(["--stalled".into(), stalled.clone()]);
    }
    if timed != "0" {
        end.push("--timed".into());
    }
    if !branch.is_empty() {
        end.extend(["--branch".into(), branch.clone()]);
    }
    if let Some(n) = pr.strip_prefix(&format!("https://github.com/{}/pull/", f.repo)) {
        if re(r"^[1-9][0-9]{0,9}$", n) {
            end.extend(["--pr".into(), n.to_string()]);
        }
    }
    for (k, name) in [("result", "result.json"), ("progress", "progress.json")] {
        if f.file(name) != &File::Absent {
            r.files.push(k.to_string());
        }
    }
    let mut opt = |flag: &str, val: &str| {
        if !val.is_empty() {
            end.extend([flag.to_string(), val.to_string()]);
        }
    };
    opt("--cap", &cap);
    opt("--park-reason", &park_reason);
    opt("--failure-kind", &failure_kind);
    opt("--failure-cause", &failure_cause);
    if provider {
        end.push("--provider".into());
    }
    if parked_now {
        end.push("--parked".into());
    }
    if never_ran {
        end.push("--never-ran".into());
    }
    r.end = end;
    let mut h = Map::new();
    h.insert("started".into(), json!(f.started));
    h.insert("rc".into(), json!(rc));
    for (k, val) in [
        ("status", &status),
        ("step", &step),
        ("cap", &cap),
        ("park_reason", &park_reason),
        ("failure_kind", &failure_kind),
        ("failure_cause", &failure_cause),
    ] {
        if !val.is_empty() {
            h.insert(k.into(), json!(val));
        }
    }
    h.insert("provider".into(), json!(provider));
    h.insert("parked".into(), json!(parked_now));
    for (k, val) in [("pr", &pr), ("by", &f.start_by), ("how", &f.start_how)] {
        if !val.is_empty() {
            h.insert(k.into(), json!(val));
        }
    }
    r.history = Value::Object(h);
    if !image_id.is_empty() {
        r.analytics.extend(["--image".into(), image_id.clone()]);
    }
    if !image_rukas.is_empty() {
        r.analytics
            .extend(["--rukas-image".into(), image_rukas.clone()]);
    }
    r.analytics.extend([
        "--oom".into(),
        kills.clone(),
        "--forks".into(),
        forks.clone(),
    ]);
    r.improve = vec![
        issue.to_string(),
        f.started.to_string(),
        rc.to_string(),
        status,
        step,
        pr,
        kills,
        image_id,
        image_rukas,
        cap,
        reason,
        build,
    ];
    r.status_run = !f.signalled;
    r
}

/// The first `n` bytes, as `head -c` cuts them.
fn cut_bytes(s: &str, n: usize) -> String {
    let b = &s.as_bytes()[..s.len().min(n)];
    String::from_utf8_lossy(b).into_owned()
}

/// The report as JSON. A park is shown with its question and with the
/// comment it posts when the question is not posted (`comment`) or the park
/// not taken (`comment_unrecorded`), each as it reads after a label change
/// that worked (`ok`) and one that did not (`stuck`).
pub fn to_json(r: &Report, issue: u64) -> Value {
    let both = |f: &dyn Fn(bool) -> String| json!({"ok": f(false), "stuck": f(true)});
    let actions: Vec<Value> = r
        .actions
        .iter()
        .map(|a| match a {
            Action::Relabel { remove, add } => json!({"relabel": {"remove": remove, "add": add}}),
            Action::Comment { ok, stuck } => json!({"comment": {"ok": ok, "stuck": stuck}}),
            Action::Park(p) => json!({"park": {
                "args": p.args,
                "question": both(&|s| p.question(issue, s).1),
                "comment": both(&|s| p.comment(s, true)),
                "comment_unrecorded": both(&|s| p.comment(s, false)),
            }}),
            Action::PrComment { pr, body } => json!({"pr_comment": {"pr": pr, "body": body}}),
            Action::Tidy { remove, warn } => json!({"tidy": {"remove": remove, "warn": warn}}),
            Action::RefreshOff { pr } => json!({"refresh_off": pr}),
        })
        .collect();
    json!({
        "log": r.log, "actions": actions, "done": r.done, "end": r.end, "files": r.files,
        "history": r.history, "analytics": r.analytics, "improve": r.improve,
        "status_run": r.status_run, "asked": r.asked,
    })
}
