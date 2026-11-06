//! A cycle's report, decided: what is done once tehas work has
//! ended, from what the cycle left (`Facts`) and the questions it asks
//! GitHub on the way (`Ask`). It writes nothing. Its answer is the lines it
//! logs while it reads, the writes in their order, and what goes on the
//! cycle's record, in the analytics and to the improvement loop.
//!
//! The order of the bash report it replaced is kept, and so are its words:
//! tests/fixtures holds the bash's reports, traced, and each is checked line by
//! line.

use crate::after::After;
use crate::facts::{first_bytes, line, two_lines, words, Facts, File};
use crate::improve::Ended;
use crate::last_attempt;
use crate::runner::{self, cut, kept, names_issue};
use serde_json::{json, Map, Value};
use tehas_core::question::{q_clean, q_render, Kind};

/// What a report asks GitHub, as the bash asks it: each with its tries and
/// bounds, which are the caller's.
pub trait Ask {
    /// The bot's pull requests on a branch, in any state; None when GitHub
    /// would not list them.
    fn prs(&mut self, branch: &str) -> Option<Vec<Value>>;
    /// Whether the issue carries needs-human-attention now; None when its
    /// labels could not be read.
    fn attention(&mut self) -> Option<bool>;
    /// The issue as GitHub's REST API answers it; None when it could not be
    /// read.
    fn issue(&mut self) -> Option<Value>;
}

/// GitHub's answers as facts hold them under "github": "prs", the list (null
/// when GitHub would not give it), "attention", whether the issue carries
/// needs-human-attention (null when its labels could not be read), and
/// "issue", the issue itself.
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

    fn issue(&mut self) -> Option<Value> {
        self.0.get("issue").filter(|v| !v.is_null()).cloned()
    }
}

/// Whether the image this cycle ran gives a retry the comments, as tehas work
/// wrote it in `<issue>.comments`. No note offers no comments retry.
pub fn comments_box(f: &Facts) -> bool {
    matches!(
        f.file("comments").text().map(str::trim),
        Some("native" | "polyfill")
    )
}

/// What the end comment says of the comments a comments retry was given: the
/// runner's own count from its result's `comments` field, or that the
/// polyfill gave them to the cycle's agents. Nothing for any other start.
pub fn comments_said(f: &Facts) -> String {
    let given = f
        .approval
        .as_ref()
        .ok()
        .and_then(|a| a.get("current"))
        .is_some_and(|c| {
            c.get("used_by").and_then(Value::as_u64) == Some(f.attempt)
                && c.get("how").and_then(Value::as_str) == Some("retry-tick")
                && c.pointer("/input/retry_context")
                    .and_then(Value::as_array)
                    .is_some_and(|a| !a.is_empty())
        });
    if !given {
        return String::new();
    }
    let mode = f.file("comments").text().map(str::trim).unwrap_or("");
    if mode == "polyfill" {
        return " The comments of the issue and its pull requests were given to the cycle's agents.".into();
    }
    if mode != "native" {
        return " The sandbox image's runner does not read comments, so the cycle did not see the comments it was given.".into();
    }
    let read = f
        .file("result.json")
        .text()
        .and_then(runner::values)
        .and_then(|all| {
            all.iter().rev().find_map(|r| {
                let per_issue = r.get("issues").and_then(|is| match is {
                    Value::Array(a) => a
                        .iter()
                        .find(|e| e.get("issue").and_then(Value::as_u64) == Some(f.issue))
                        .and_then(|e| e.get("comments")),
                    _ => None,
                });
                per_issue
                    .or_else(|| r.get("comments"))
                    .filter(|c| c.is_object())
                    .cloned()
            })
        });
    let Some(c) = read else {
        return " The cycle's runner did not say whether it read the comments it was given.".into();
    };
    let n = |k: &str| c.get(k).and_then(Value::as_u64).unwrap_or(0);
    if c.get("read") == Some(&Value::Bool(true)) {
        let dropped = match n("dropped") {
            0 => String::new(),
            d => format!(", {d} older ones left out for room"),
        };
        return format!(
            " The cycle was given {} comments of the issue and its pull requests{dropped}.",
            n("count")
        );
    }
    let reason = c
        .get("reason")
        .and_then(Value::as_str)
        .map(|r| {
            kept(&first_bytes(r, 200), |ch| {
                ch.is_ascii_alphanumeric() || " _.,:-".contains(ch)
            })
        })
        .filter(|r| !r.is_empty())
        .unwrap_or_else(|| "no reason given".to_string());
    format!(" The cycle's runner did not read the comments it was given: {reason}.")
}

/// The caps tehas sets for a stop of its own, and no other command does.
pub const OWN_CAPS: [&str; 5] = [
    "wall-clock",
    "stall",
    "disk",
    "runner-never-started",
    "signal",
];

/// The label that chooses the lean driver.
const LEAN: &str = "tehas:lean";

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
    /// The host's line when this park repeats the one before it.
    pub repeat: String,
    /// It ended at child-timeout:develop with no stop: once it is on record
    /// the issue is read, for `last_attempt`.
    pub develop: bool,
    /// What that read decided: no retry box and these words, or this body
    /// for the issue.
    pub no_retry: bool,
    /// The image gives a retry the comments: the park offers that retry.
    pub comments: bool,
    pub say: String,
    pub edit: Option<String>,
    /// The host's line above the boxes of its question.
    pub lead: String,
}

impl Park {
    /// The text of the park once `stuck` (the label stayed) and `on_record`
    /// (the store took the park) are known.
    pub fn parked(&self, stuck: bool, on_record: bool) -> String {
        let mut p = self.text.clone();
        if !self.say.is_empty() {
            p += &format!(" {}", self.say);
        }
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
        p + &self.repeat
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
        let out = json!({"text": text, "question": null, "marker": self.marker,
                         "park": self.park_marker, "retry": !self.no_retry,
                         "comments": self.comments});
        let body = q_render(Kind::Park, issue, 1, &out, &self.lead);
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

    /// A develop timeout leaves a note in the body, which the next cycle
    /// reads; a second one on the same text offers no retry. The park is on
    /// record before the issue is read. An issue that cannot be read changes
    /// nothing.
    pub fn last_attempt(&mut self, f: &Facts, ask: &mut dyn Ask) {
        if !self.develop {
            return;
        }
        let Some(issue) = ask.issue() else {
            return;
        };
        let result = f.file("result.json").text();
        match last_attempt::of_issue(f.issue, &issue, result, f.work_state.as_ref()) {
            Some(last_attempt::Action::Repeat) => {
                self.no_retry = true;
                self.say = last_attempt::REPEAT.to_string();
            }
            Some(last_attempt::Action::Edit(body)) => self.edit = Some(body),
            _ => {}
        }
    }
}

/// `Park::last_attempt` of a report that writes nothing.
pub fn last_attempts(r: &mut Report, f: &Facts, ask: &mut dyn Ask) {
    for a in &mut r.actions {
        if let Action::Park(p) = a {
            p.last_attempt(f, ask);
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
    /// progress are the caller's to add, for each of `files`, at `files_at`.
    pub end: Vec<String>,
    pub files: Vec<String>,
    pub files_at: usize,
    /// The analytics line, without its time.
    pub history: Value,
    /// The arguments of `analytics run` after the issue, the attempt and the
    /// line.
    pub analytics: Vec<String>,
    /// The arguments of the improvement record.
    pub improve: Vec<String>,
    /// Whether the status runs after it (not after a stop).
    pub status_run: bool,
    /// What GitHub was asked, in order: `prs <branch>`, `attention`.
    pub asked: Vec<String>,
    /// Which arm of the report this cycle took, for the fixtures.
    pub arm: &'static str,
    /// What follows the end; the caller adds the approval and its programs.
    pub after: After,
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
    let low = |s: &str| {
        cut(
            &kept(s, |c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'
            }),
            40,
        )
    };
    let s = if step == "-" { "" } else { step };
    let sig = tehas_core::question::park_sig(step, cap, park_reason, kind, cause);
    format!(
        "<!-- tehas:park issue={issue} step={} kind={} provider={} sig={sig} -->",
        low(s),
        low(kind),
        u8::from(provider)
    )
}

/// q_park_repeat: the line a repeating park says of itself, in the host's
/// own words. Nothing of a failure's free text reaches the comment.
fn repeat_line(step: &str, cap: &str) -> String {
    let step = cut(
        &kept(step, |c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'
        }),
        40,
    );
    let cap = cut(
        &kept(cap, |c| c.is_ascii_alphanumeric() || "_.:-".contains(c)),
        80,
    );
    let mut line =
        " This park repeats the one before it: number 1 of the parks in a row that stopped"
            .to_string();
    if !step.is_empty() {
        line += &format!(" at step `{step}`");
    }
    if !cap.is_empty() {
        line += &format!(" on the cap `{cap}`");
    }
    line + ". A retry on the same text will likely stop the same way and spend another slot: change the issue first, or ask for a smaller change with `tehas:plan`."
}

/// Whether the issue's last end is a park of this one's signature and no
/// provider outage. Unreadable history says nothing.
fn repeats(f: &Facts, sig: &str, step: &str) -> bool {
    let Some(rows) = f
        .history
        .as_ref()
        .ok()
        .and_then(|h| h.get("rows"))
        .and_then(Value::as_array)
    else {
        return false;
    };
    let Some(p) = rows
        .iter()
        .rfind(|r| r.get("kind").and_then(Value::as_str) == Some("cycle"))
    else {
        return false;
    };
    let word = |k: &str| p.get(k).and_then(Value::as_str).unwrap_or("");
    let low = |s: &str| {
        cut(
            &kept(s, |c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'
            }),
            40,
        )
    };
    let wanted = low(if step == "-" { "" } else { step });
    p.get("parked") == Some(&Value::Bool(true))
        && p.get("provider") != Some(&Value::Bool(true))
        && low(word("step")) == wanted
        && tehas_core::question::park_sig(
            word("step"),
            word("cap"),
            word("park_reason"),
            word("failure_kind"),
            word("failure_cause"),
        ) == sig
}

/// The report of a cycle that ended.
/// The image on the record of this attempt and the pi-rukas commit of its
/// label, each empty when it is not there or of another shape.
pub fn record_image(rec: &Value, attempt: u64) -> (String, String) {
    let (mut id, mut rukas) = (String::new(), String::new());
    if let Some(c) = rec.get("cycle").filter(|c| {
        c.is_object()
            && c.get("attempt").and_then(Value::as_u64) == Some(attempt)
            && !c.get("image").is_none_or(Value::is_null)
    }) {
        let line = format!("{} {}", text(c, "image"), text(c, "rukas_commit"));
        let ws = words(&line, None);
        id = ws.first().cloned().unwrap_or_default();
        rukas = ws.get(1).cloned().unwrap_or_default();
    }
    if !re(r"^sha256:[0-9a-f]{64}$", &id) {
        id.clear();
    }
    if !re(r"^[0-9a-f]{7,40}$", &rukas) {
        rukas.clear();
    }
    (id, rukas)
}

pub fn report(f: &Facts, ask: &mut dyn Ask) -> Report {
    let mut r = Report::default();
    let issue = f.issue;
    let warn = |r: &mut Report, s: String| r.log.push(format!("WARN: {s}"));
    let mins = (f.now - f.started) / 60;
    // A lean runner ran: tehas work saw it end.
    let lean = lean_start(f) && f.file("runner_exit").text().is_some();

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
    let (handoff, lean_posted) = {
        let b = text(&outcome, "handoffBody");
        let marker = regex::Regex::new(r"\A<!-- pi-rukas:handoff [^\n]*-->\n*").unwrap();
        let b = v(&marker.replace(&b, ""));
        // Lean on the forge channel posted this itself: one comment, not two.
        let posted = lean && !b.is_empty() && text(&outcome, "handoffChannel") == "forge";
        if posted {
            (
                "Lean posted its report on this issue itself, so it is not repeated here.".into(),
                true,
            )
        } else {
            (b, false)
        }
    };
    let pr_state = text(&outcome, "prState");
    let mut park_reason = v(&cut(
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
    // The two stop files of tehas work, each read ONCE: its seconds go with
    // its step, so the step below and the stop below cannot disagree. A first
    // word that is no count of seconds is no stop there and here, so neither
    // the step nor the stop of that file counts.
    let secs = |s: &str| re(r"^(0|[1-9][0-9]{0,8})$", s);
    let stalled = match f.file("stalled").text() {
        Some(t) => {
            let (a, b) = two_lines(t);
            let at = cut(
                &kept(&b, |c| c.is_ascii_alphanumeric() || "_.-".contains(c)),
                40,
            );
            if !secs(&a) {
                ("0".to_string(), String::new())
            } else {
                let s = if at.is_empty() || at == "-" {
                    String::new()
                } else {
                    at
                };
                (a, s)
            }
        }
        None => ("0".into(), String::new()),
    };
    let timed = match f.file("timed").text() {
        Some(t) => {
            let ws = words(t, Some(200));
            let n = ws.first().cloned().unwrap_or_default();
            let by = ws.get(1).cloned().unwrap_or_default();
            let at = ws.get(2).cloned().unwrap_or_default();
            if !secs(&n) {
                ("0".to_string(), String::new(), String::new())
            } else {
                let s = if re(r"^[A-Za-z0-9_.:-]{1,40}$", &at) && at != "-" {
                    at
                } else {
                    String::new()
                };
                (n, by, s)
            }
        }
        None => ("0".into(), String::new(), String::new()),
    };
    // The signal note: the signal's name, or - when not known, and the step.
    let signal = f.file("signal").text().map(|t| {
        let ws = words(t, Some(200));
        let word = |i: usize| ws.get(i).cloned().unwrap_or_default();
        let (name, at) = (word(0), word(1));
        let name = if re(r"^(TERM|INT|HUP)$", &name) {
            name
        } else {
            String::new()
        };
        let at = if re(r"^[A-Za-z0-9_.:-]{1,40}$", &at) && at != "-" {
            at
        } else {
            String::new()
        };
        (name, at)
    });
    // The disk stop (src/scratch.rs): `cap <held> <max>` or
    // `floor <held> <free> <floor>`, then the step in flight.
    let disk: Option<(String, Vec<u64>, String)> = f.file("disk").text().and_then(|t| {
        let (a, b) = two_lines(t);
        let ws: Vec<&str> = a.split(' ').collect();
        let figures: Option<Vec<u64>> = ws.iter().skip(1).map(|w| w.parse().ok()).collect();
        let figures = figures?;
        let shaped = matches!((ws[0], figures.len()), ("cap", 2) | ("floor", 3));
        let at = cut(
            &kept(&b, |c| c.is_ascii_alphanumeric() || "_.-".contains(c)),
            40,
        );
        let at = if at == "-" { String::new() } else { at };
        shaped.then(|| (ws[0].to_string(), figures, at))
    });
    let (stalled_secs, stalled_step) = &stalled;
    let (timed_secs, timed_by, timed_step) = &timed;
    let disk_step = disk.as_ref().map_or("", |d| d.2.as_str());
    let mut step_wanted = step == "-";
    if !disk_step.is_empty() {
        step_wanted = false;
    }
    if !nostart_rc.is_empty() {
        step_wanted = false;
    }
    if !stalled_step.is_empty() || !timed_step.is_empty() {
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
                    // The host read the log, so it says whether it holds the
                    // whole of it: only a whole log can say there is no later
                    // step than the one its end names.
                    log_read = runner::read_word(l.whole()).to_string();
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
    if step == "-" && !stalled_step.is_empty() {
        step = stalled_step.clone();
        step_from = "stalled";
    }
    if let Some((_, at)) = &signal {
        if step == "-" && !at.is_empty() {
            step = at.clone();
            step_from = "signal";
        }
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
    killed += &comments_said(f);
    let forks = count("forks");
    let (mut image_id, mut image_rukas) = (String::new(), String::new());
    match &f.record {
        Ok(rec) => (image_id, image_rukas) = record_image(rec, f.attempt),
        Err(word) => warn(
            &mut r,
            format!("the record of #{issue} could not be read for its image ({word})"),
        ),
    }
    let s60 = |s: &str| s.parse::<i64>().unwrap_or(0) / 60;
    if stalled_secs != "0" {
        if ["running", "unknown", "interrupted"].contains(&status.as_str()) {
            status = "stalled".into();
        }
        killed += &format!(
            " None of its logs changed for {} min at step `{step}`, so tehas stopped it before its time limit.",
            s60(stalled_secs)
        );
    }
    // The time limit, from the one read of the timed file above.
    if timed_secs != "0" {
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
                s60(timed_secs)
            );
        } else {
            killed += &format!(
                " It ran past its time limit of {} min at step `{step}`, and the runner did not stop on its own, so tehas stopped it.",
                s60(timed_secs)
            );
        }
    }
    let mib = |b: u64| b >> 20;
    let disk_why = disk.as_ref().map(|(word, n, _)| match word.as_str() {
        "cap" => format!(
            "its scratch held {} MiB, over TEHAS_SCRATCH_MAX of {} MiB",
            mib(n[0]),
            mib(n[1])
        ),
        _ => format!(
            "the scratch's disk had {} MiB free, under TEHAS_CYCLE_STOP_FREE of {} MiB, and its scratch was the largest there ({} MiB)",
            mib(n[1]),
            mib(n[2]),
            mib(n[0])
        ),
    });
    if let Some(why) = &disk_why {
        if ["running", "unknown", "interrupted"].contains(&status.as_str()) {
            status = "stopped".into();
        }
        if step == "-" && !disk_step.is_empty() {
            step = disk_step.to_string();
            step_from = "disk";
        }
        killed += &format!(" At step `{step}` {why}, so tehas stopped it.");
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
    // Lean writes no step-started line to name its branch: its own default.
    if branch.is_empty() && lean {
        branch = format!("lean/issue-{issue}");
    }
    let build = if re(r"^[0-9a-f]{40}$", &build) {
        build
    } else {
        String::new()
    };
    let [review_held, mut reason, failure_from, failure_kind, failure_role, failure_cause] =
        runner::progress_fields(progress);
    let provider = failure_from == "result" && failure_kind == "provider";
    // A stop of tehas's own is its own cap.
    let mut own = "";
    let mut stop_why = String::new();
    if nostart_seen && no_result {
        own = "runner-never-started";
        stop_why = format!("the container ended with the exit status {nostart_rc} and wrote no line of its stream, so the driver wrote no event and no step started");
    } else if timed_secs != "0" && (cap.is_empty() || status == "timeout") {
        own = "wall-clock";
        stop_why = if timed_by == "runner" {
            format!("the cycle reached its time limit, TEHAS_WORK_TIMEOUT={timed_secs}s, at step {step}")
        } else {
            format!("tehas work stopped the container at the time limit of {timed_secs}s")
        };
    } else if stalled_secs != "0" && (cap.is_empty() || status == "stalled") {
        own = "stall";
        stop_why =
            format!("tehas work stopped the container when no log changed for {stalled_secs}s");
    } else if let Some(why) = disk_why
        .as_ref()
        .filter(|_| cap.is_empty() || status == "stopped")
    {
        own = "disk";
        stop_why = format!("tehas work stopped the container: {why}");
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
    } else if let Some((name, _)) = signal.as_ref().filter(|_| {
        cap.is_empty() && ["running", "unknown", "interrupted"].contains(&status.as_str())
    }) {
        own = "signal";
        stop_why = if name.is_empty() {
            "a stop signal came to tehas work and it stopped the container".to_string()
        } else {
            format!("SIG{name} came to tehas work and it stopped the container")
        };
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
    // #1411: refused starts of a process are the cause of a stop that names
    // none. A cap the run had, and a person's stop, come first.
    let process_limit = cap.is_empty() && forks != "0" && !stopped_by_person;
    if process_limit {
        let at = kept(&step, |c| c.is_ascii_alphanumeric() || "_.-".contains(c));
        cap = if at.is_empty() || at == "-" {
            "process-limit".to_string()
        } else {
            cut(&format!("process-limit:{at}"), 80)
        };
        let tail = if reason.is_empty() {
            String::new()
        } else {
            format!("; {reason}")
        };
        let in_flight = if step == "-" {
            String::new()
        } else {
            format!(", step {step} in flight")
        };
        reason = cut_bytes(
            &format!("its container reached its limit of processes and refused {forks} starts of a process{in_flight}{tail}"),
            200,
        );
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
    let pr_draft = !pr.is_empty()
        && prs.iter().any(|p| {
            p.get("url").and_then(Value::as_str) == Some(pr.as_str())
                && p.get("isDraft").and_then(Value::as_bool) == Some(true)
        });
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
            "stalled" | "timed" | "disk" | "signal" => {
                summary = format!("status `{status}`, last step `{step}` tehas work saw in flight, {mins} min")
            }
            "nostart" => {
                summary = format!("status `{status}`, no step started, the container of the cycle ended before the driver wrote an event, {mins} min")
            }
            _ => {}
        }
    }
    if lean {
        summary += ", driver `lean`";
    }
    let mut why = String::new();
    if OWN_CAPS.contains(&cap.as_str()) {
        why = format!(" tehas stopped the cycle: the cap `{cap}`.");
    } else if process_limit {
        why = format!(" Its cause is recorded as the cap `{cap}`.");
    } else if !cap.is_empty() {
        let pr_why = if park_reason.is_empty() {
            String::new()
        } else {
            format!(" (`{park_reason}`)")
        };
        why = format!(" pi-rukas stopped on the cap `{cap}`{pr_why}.");
    }
    if cap == "deadline" {
        why += " It reached its deadline before the time limit and started no new step.";
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
        .unwrap_or_default();
    if done_why == "quiet" {
        if park_reason.is_empty() {
            park_reason = "quiet".into();
        }
        why += " The dispatcher stopped it: nothing was heard from it for longer than its quiet bound.";
    }
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
    let sig =
        tehas_core::question::park_sig(&step, &cap, &park_reason, &failure_kind, &failure_cause);
    let base = Park {
        handoff: handoff.clone(),
        usage: usage.clone(),
        marker: marker.clone(),
        park_marker: pm,
        repeat: if !provider && repeats(f, &sig, &step) {
            repeat_line(&step, &cap)
        } else {
            String::new()
        },
        develop: cap == "child-timeout:develop" && !f.signalled,
        comments: comments_box(f),
        ..Park::default()
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
    let mut rc = f.rc;
    // tehas work refused the start: with 4, or with 78 and its note on lean.
    let lean_refused = rc == 78 && matches!(f.file("lean-refused"), File::Text(_));
    let never_started = rc == 4 || lean_refused;
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
            r.arm = "merged";
            r.actions.push(comment(&|s| {
                format!("Tehas cycle finished: {summary}. The change was merged: {merged_pr}{killed}{s}")
            }));
            r.actions.push(tidy);
        } else {
            r.arm = "merged-outside";
            r.actions.push(comment(&|s| {
                format!("Tehas cycle ended: its pull request {merged_pr} was merged while the cycle ran (last step `{step}`, {mins} min, runner exit {runner_rc}). Nothing more is needed here. {kept_said}.{killed}{s}")
            }));
        }
        // A merged pull request is the cycle's end, whatever the runner was
        // stopped in: the record says merged and 0, the runner's exit stays
        // in --runner-exit and the comment.
        status = "merged".into();
        rc = 0;
        pr = merged_pr.clone();
    } else if unread {
        r.arm = "prs-unread";
        parked_now = true;
        r.actions.push(park(park_args("", &cap, &status, &step, &pr), format!(
            "Tehas cycle stopped, and its pull request could not be looked up: GitHub did not list the pull requests of the branch `{}` ({summary}, runner exit {runner_rc}). Look for one there before you retry. {kept_said}; {log_where}.{killed}",
            said_branch(&branch)
        ), String::new()));
    } else if rc == 0 && status == "merged" {
        r.arm = "merged-claimed";
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
        && waits_only_for_a_merge(&cap)
        && review_held == "false"
        && !(lean && (pr_draft || pr_state != "ready"))
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
        r.arm = "awaiting-merge";
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
        if lean && pr_draft {
            why = format!(
                " Lean left its pull request a draft, so it does not wait only for a merge.{why}"
            );
        } else if lean && pr_state != "ready" {
            why = format!(" Lean says its pull request is `{}`, not `ready`, so it does not wait only for a merge.{why}", if pr_state.is_empty() { "unknown" } else { &pr_state });
        } else if cap.is_empty() {
            why = " The result names no cap, so it is not taken for a pull request that waits only for a merge.".into();
        }
        r.arm = "handoff-parked";
        parked_now = true;
        r.actions.push(park(park_args("", &cap, &status, &step, &pr), format!(
            "Tehas cycle handed off with its pull request open: {summary} (runner exit {runner_rc}).{why}{held} Read the pull request before you retry: {pr}. {kept_said}; {log_where}.{killed}{foreign}"
        ), String::new()));
    } else if done_why == "closed" {
        r.arm = "issue-closed";
        r.actions.push(running());
        r.actions.push(comment(&|s| {
            format!("Tehas stopped the cycle: this issue was closed while it ran ({summary}). Nothing more is needed here. {kept_said}; {log_where}.{killed}{s}")
        }));
        status = "closed".into();
    } else if pr.is_empty() && !closed_pr.is_empty() && !never_started {
        r.arm = "pr-closed";
        r.actions.push(to("needs-human-attention"));
        r.actions.push(comment(&|s| {
            format!(
                "{marker_line}{}",
                q_clean(&format!("Tehas cycle stopped: its pull request {closed_pr} was closed without being merged ({summary}, runner exit {runner_rc}).{why} {kept_said}; {log_where}.{killed}{s} To retry, apply `tehas:ready`.{handoff_part}"), 60000)
            )
        }));
    } else if died_with_open_pr {
        r.arm = "died-with-pr";
        r.actions.push(to("needs-human-attention"));
        r.actions.push(comment(&|s| {
            format!(
                "{marker_line}{}",
                q_clean(&format!("Tehas cycle handed off with its pull request open and its review unfinished: {summary} (runner exit {runner_rc}). Its runner wrote no result, so tehas read its run log for the step it was in and GitHub for the bot's pull request on its branch: {pr}. Read the pull request before you retry: a retry starts over and redoes what it already holds. {kept_said}; {log_where}.{killed}{s}"), 60000)
            )
        }));
        status = "handoff".into();
    } else if lean && rc == 11 && status == "aborted" {
        r.arm = "not-ready";
        parked_now = true;
        let fix = if lean_posted || handoff.contains("pi-spec") || handoff.is_empty() {
            " The issue needs a `pi-spec v1` block that lean accepts: fix the block, or run /plan to write it, then retry."
        } else {
            ""
        };
        let rules = if lean_posted || handoff.is_empty() {
            " Lean's own comment on this issue lists the rules it failed."
        } else {
            " The rules it failed follow."
        };
        r.actions.push(park(park_args("not-ready", &cap, &status, &step, &pr), format!(
            "Tehas cycle stopped: lean found this issue not ready to build from ({summary}, runner exit {runner_rc}).{fix}{rules} {kept_said}; {log_where}.{killed}"
        ), String::new()));
    } else {
        let handoff_said = if handoff.is_empty() {
            ""
        } else {
            " The handoff of pi-rukas follows."
        };
        let stopped = if pr.is_empty() {
            "without a pull request".to_string()
        } else {
            let n = pr
                .rsplit_once("/pull/")
                .map(|(_, n)| n)
                .filter(|n| re(r"^[1-9][0-9]{0,9}$", n));
            let draft = if pr_draft { " (draft)" } else { "" };
            match n {
                Some(n) => format!("with pull request #{n} open{draft}"),
                None => format!("with its pull request open{draft}"),
            }
        };
        let mut parked = format!(
            "Tehas cycle stopped {stopped}: {summary} (runner exit {runner_rc}).{why}{handoff_said} {kept_said}; {log_where}.{killed}{open_pr}{foreign}"
        );
        let mut park_why = "";
        let mut auto = String::new();
        let mut lead = String::new();
        r.arm = "parked";
        if never_started {
            r.arm = "never-ran";
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
            if rc != 4 {
                parked = format!("Tehas started no cycle: this start's driver is `lean`, and the sandbox image cannot run it as this start asks (runner exit {runner_rc}). It does not run classic in its place, and nothing of it ran. The log names what the image lacks. To run it on classic, take `tehas:lean` off, then retry; {log_where}.");
                park_why = tehas_core::question::LEAN_REFUSED;
                lead = format!("Take `{LEAN}` off this issue, then tick a box.");
            }
        } else if provider {
            r.arm = "provider";
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
        let mut p = park(park_args(park_why, &cap, &status, &step, &pr), parked, auto);
        if let Action::Park(b) = &mut p {
            b.lead = lead;
        }
        r.actions.push(p);
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
    if stalled_secs != "0" {
        end.extend(["--stalled".into(), stalled_secs.clone()]);
    }
    if timed_secs != "0" {
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
    r.files_at = end.len();
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
        if !f.image_built.is_empty() {
            r.analytics
                .extend(["--image-built".into(), f.image_built.clone()]);
        }
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
    r.after = After {
        ended: Ended {
            issue,
            started: f.started,
            rc,
            status: status.clone(),
            step: step.clone(),
            pr: pr.clone(),
            kills: kills.clone(),
            image: image_id.clone(),
            rukas: image_rukas.clone(),
            cap: cap.clone(),
            reason: reason.clone(),
            build: build.clone(),
        },
        attempt: f.attempt,
        park_reason,
        failure_kind,
        failure_cause,
        provider,
        parked: parked_now,
        by: f.start_by.clone(),
        how: f.start_how.clone(),
        image_built: f.image_built.clone(),
        forks: forks.clone(),
        signalled: f.signalled,
        ..After::default()
    };
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
                "edit": p.edit.is_some(),
            }}),
            Action::PrComment { pr, body } => json!({"pr_comment": {"pr": pr, "body": body}}),
            Action::Tidy { remove, warn } => json!({"tidy": {"remove": remove, "warn": warn}}),
            Action::RefreshOff { pr } => json!({"refresh_off": pr}),
        })
        .collect();
    json!({
        "log": r.log, "actions": actions, "done": r.done, "end": r.end, "files": r.files,
        "history": r.history, "analytics": r.analytics, "improve": r.improve,
        "status_run": r.status_run, "asked": r.asked, "arm": r.arm,
    })
}

/// Whether the approval this start used chose the lean driver.
fn lean_start(f: &Facts) -> bool {
    let Ok(a) = &f.approval else {
        return false;
    };
    a.get("current")
        .filter(|c| c.get("used_by").and_then(Value::as_u64) == Some(f.attempt))
        .and_then(|c| c.pointer("/input/driver"))
        == Some(&json!("lean"))
}

/// The caps of a handoff whose open pull request waits only for a merge.
/// `lens-fix-reposition` is a second review round whose fix could not be laid
/// on the branch: the branch already holds the first round's fix, reviewed and
/// pushed, so a park there threw a finished pull request back to a person. It
/// was the top cap from 2026-10-31, 32 parks, all with the pull request open.
fn waits_only_for_a_merge(cap: &str) -> bool {
    matches!(cap, "awaiting-human-merge" | "lens-fix-reposition")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A cycle that wrote no result: the report reads only what tehas work
    /// left beside it. `notes` are the files of state/cycles/<issue>.*.
    fn notes(notes: Vec<(&str, &str)>) -> Report {
        let mut files = serde_json::Map::new();
        files.insert(
            "progress.json".into(),
            json!("{\"roles\":[],\"reason\":\"\",\"reviewHeld\":false}\n"),
        );
        for (name, text) in notes {
            files.insert(name.into(), json!(text));
        }
        let facts = json!({
            "issue": 7,
            "repo": "acme/widget",
            "home": "/home/tehas",
            "rc": 2,
            "started": 1000,
            "now": 4600,
            "start": { "attempt": 1 },
            "files": files,
            "record": { "cycle": { "attempt": 1 } },
            "github": { "prs": [], "attention": false },
        });
        let f = Facts::from_json(&facts).expect("the facts read");
        report(&f, &mut Given(facts["github"].clone()))
    }

    fn cap_disk(r: &Report) -> bool {
        r.end.windows(2).any(|w| w[0] == "--cap" && w[1] == "disk")
    }

    #[test]
    fn a_disk_stop_is_its_own_cap() {
        let g = 1u64 << 30;
        let r = notes(vec![(
            "disk",
            &format!("cap {} {}\nverify\n", 21 * g, 20 * g),
        )]);
        assert!(r.done.contains("status=stopped"), "{}", r.done);
        assert!(cap_disk(&r), "{:?}", r.end);
        let said = comments(&r).join("\n");
        assert!(
            said.contains("over TEHAS_SCRATCH_MAX of 20480 MiB"),
            "{said}"
        );
        assert!(said.contains("`verify`"), "{said}");
        let r = notes(vec![(
            "disk",
            &format!("floor {} {} {}\n-\n", 3 * g, g, 5 * g),
        )]);
        assert!(cap_disk(&r), "{:?}", r.end);
        let said = comments(&r).join("\n");
        assert!(said.contains("1024 MiB free"), "{said}");
        // A note of another shape is no stop.
        for bad in ["cap 1\n", "floor 1 2\n", "cap x 2\n", "full 1 2\n", "\n"] {
            let r = notes(vec![("disk", bad)]);
            assert!(!cap_disk(&r), "{bad}: {:?}", r.end);
        }
    }

    /// The value of `--stalled` in the store's arguments, if it is there.
    fn stalled_arg(r: &Report) -> Option<&str> {
        r.end
            .iter()
            .position(|a| a == "--stalled")
            .and_then(|i| r.end.get(i + 1))
            .map(String::as_str)
    }

    /// What the report asks GitHub to write, in order.
    fn comments(r: &Report) -> Vec<String> {
        r.actions
            .iter()
            .filter_map(|a| match a {
                Action::Comment { ok, .. } => Some(ok.clone()),
                Action::Park(p) => Some(p.text.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_stall_file_of_its_own_names_the_stop_and_the_step_of_one_read() {
        let r = notes(vec![("stalled", "1800\nstall-step\n")]);
        assert_eq!(stalled_arg(&r), Some("1800"));
        assert!(r.done.contains("status=stalled"), "{}", r.done);
        assert!(r.improve.iter().any(|a| a == "stalled"), "{:?}", r.improve);
        assert!(
            comments(&r)
                .iter()
                .any(|c| c.contains("last step `stall-step` tehas work saw in flight")),
            "{:?}",
            comments(&r)
        );
    }

    // #1207: the two reads of a stop file disagreed when its seconds were no
    // count, so the report said a stalled cycle whose step came of the same
    // unreadable file. One read settles both.
    #[test]
    fn a_stall_file_of_no_count_of_seconds_names_neither_stop_nor_step() {
        let r = notes(vec![("stalled", "nope\nstall-step\n")]);
        assert_eq!(stalled_arg(&r), None, "{:?}", r.end);
        assert!(!r.done.contains("status=stalled"), "{}", r.done);
        assert!(
            comments(&r).iter().all(|c| !c.contains("stall-step")),
            "{:?}",
            comments(&r)
        );
        assert!(!r.improve.iter().any(|a| a == "stalled"), "{:?}", r.improve);
    }

    #[test]
    fn a_signal_note_is_the_cap_signal_and_its_reason_names_the_signal_and_step() {
        let r = notes(vec![("signal", "TERM verify\n")]);
        assert_eq!(r.after.ended.cap, "signal");
        assert_eq!(
            r.after.ended.reason,
            "SIGTERM came to tehas work and it stopped the container, step verify in flight"
        );
        assert_eq!(r.after.ended.step, "verify");
        assert!(
            r.end.windows(2).any(|w| w == ["--cap", "signal"]),
            "{:?}",
            r.end
        );
        assert!(
            comments(&r)
                .iter()
                .any(|c| c.contains("tehas stopped the cycle: the cap `signal`")),
            "{:?}",
            comments(&r)
        );
    }

    #[test]
    fn a_signal_note_of_no_known_signal_or_step_still_names_the_stop() {
        for text in ["- -\n", "KILL `rm`;x\n", "\n"] {
            let r = notes(vec![("signal", text)]);
            assert_eq!(r.after.ended.cap, "signal", "{text:?}");
            assert_eq!(
                r.after.ended.reason,
                "a stop signal came to tehas work and it stopped the container, step - in flight",
                "{text:?}"
            );
        }
    }

    #[test]
    fn a_persons_stop_beside_a_signal_note_keeps_its_own_reason_and_no_cap() {
        let r = notes(vec![
            ("signal", "TERM develop\n"),
            ("stopped", "1792000000 alice\n"),
        ]);
        assert_eq!(r.after.ended.cap, "");
        assert!(
            r.after
                .ended
                .reason
                .starts_with("tehas stop ended the cycle, run by alice"),
            "{}",
            r.after.ended.reason
        );
    }

    #[test]
    fn a_stall_beside_a_signal_note_is_the_stall() {
        let r = notes(vec![
            ("signal", "TERM develop\n"),
            ("stalled", "600\ndevelop\n"),
        ]);
        assert_eq!(r.after.ended.cap, "stall");
    }

    #[test]
    fn a_stall_file_of_zero_seconds_names_no_stop_and_keeps_its_step() {
        let r = notes(vec![("stalled", "0\nstall-step\n")]);
        assert_eq!(stalled_arg(&r), None, "{:?}", r.end);
        assert!(!r.done.contains("status=stalled"), "{}", r.done);
        assert!(
            comments(&r).iter().any(|c| c.contains("stall-step")),
            "{:?}",
            comments(&r)
        );
    }

    #[test]
    fn a_timed_file_of_no_count_of_seconds_names_neither_stop_nor_step() {
        let r = notes(vec![("timed", "nope tehas timed-step\n")]);
        assert!(!r.end.iter().any(|a| a == "--timed"), "{:?}", r.end);
        assert!(!r.done.contains("status=timeout"), "{}", r.done);
        assert!(
            comments(&r).iter().all(|c| !c.contains("timed-step")),
            "{:?}",
            comments(&r)
        );
    }

    #[test]
    fn a_timed_file_of_its_own_names_the_stop_and_the_step_of_one_read() {
        let r = notes(vec![("timed", "21600 runner timed-step\n")]);
        assert!(r.end.iter().any(|a| a == "--timed"), "{:?}", r.end);
        assert!(r.done.contains("status=timeout"), "{}", r.done);
        assert!(
            comments(&r)
                .iter()
                .any(|c| c.contains("last step `timed-step` tehas work saw in flight")),
            "{:?}",
            comments(&r)
        );
    }

    #[test]
    fn a_stop_file_that_is_a_link_is_no_stop_and_no_step() {
        // Facts::from_json gives a link no text, as the reads of the bash did
        // for a file that is not a regular one.
        let facts = json!({
            "issue": 7,
            "repo": "acme/widget",
            "rc": 2,
            "started": 1000,
            "now": 4600,
            "start": { "attempt": 1 },
            "files": { "stalled": { "link": true }, "timed": { "link": true } },
            "record": { "cycle": { "attempt": 1 } },
            "github": { "prs": [], "attention": false },
        });
        let f = Facts::from_json(&facts).expect("the facts read");
        let r = report(&f, &mut Given(facts["github"].clone()));
        assert_eq!(stalled_arg(&r), None, "{:?}", r.end);
        assert!(!r.end.iter().any(|a| a == "--timed"), "{:?}", r.end);
        assert!(
            !r.done.contains("stalled") && !r.done.contains("timeout"),
            "{}",
            r.done
        );
    }

    /// A cycle that handed off at `commit` with a draft pull request open,
    /// its runner exit 30, after its container refused `forks` starts.
    fn refused(forks: &str, cap: Option<&str>) -> Report {
        let mut entry = json!({"issue": 7, "status": "handoff", "failedStep": "commit", "branch": "feature/x", "handoff": {"body": "The review roles did not run."}});
        if let Some(c) = cap {
            entry["cap"] = json!(c);
        }
        let result = json!({"schemaVersion": runner::RESULT_SCHEMA, "issues": [entry]});
        let pr = json!({"url": "https://github.com/acme/widget/pull/9", "state": "OPEN", "isDraft": true,
                        "body": "Fixes #7", "title": "x", "headRefName": "feature/x"});
        let facts = json!({
            "issue": 7,
            "repo": "acme/widget",
            "home": "/home/tehas",
            "rc": 10,
            "started": 1000,
            "now": 2620,
            "start": { "attempt": 1 },
            "files": {
                "result.json": result.to_string(),
                "progress.json": "{\"roles\":[],\"reason\":\"\",\"reviewHeld\":false}\n",
                "runner_exit": "30\n",
                "forks": forks,
            },
            "record": { "cycle": { "attempt": 1 } },
            "github": { "prs": [pr], "attention": false },
        });
        let f = Facts::from_json(&facts).expect("the facts read");
        report(&f, &mut Given(facts["github"].clone()))
    }

    fn end_cap(r: &Report) -> Option<&str> {
        r.end
            .iter()
            .position(|a| a == "--cap")
            .and_then(|i| r.end.get(i + 1))
            .map(String::as_str)
    }

    // #1411: a stop on the process limit read like a review park, `cap=-`.
    #[test]
    fn refused_starts_with_no_cap_are_the_cause_of_the_stop() {
        let r = refused("6\n", None);
        assert!(r.done.contains("forks_refused=6"), "{}", r.done);
        assert_eq!(end_cap(&r), Some("process-limit:commit"), "{:?}", r.end);
        assert_eq!(r.history["cap"], "process-limit:commit", "{}", r.history);
        assert_eq!(r.after.ended.cap, "process-limit:commit");
        assert!(
            r.after
                .ended
                .reason
                .contains("refused 6 starts of a process, step commit in flight"),
            "{}",
            r.after.ended.reason
        );
        let park = r
            .actions
            .iter()
            .find_map(|a| match a {
                Action::Park(p) => Some(p.clone()),
                _ => None,
            })
            .expect("a park");
        assert!(
            park.args
                .windows(2)
                .any(|w| w == ["--reason", "process-limit:commit"]),
            "{:?}",
            park.args
        );
        assert!(
            park.marker.contains("cap=process-limit:commit"),
            "{}",
            park.marker
        );
        assert!(
            park.text
                .contains("Its cause is recorded as the cap `process-limit:commit`."),
            "{}",
            park.text
        );
    }

    #[test]
    fn a_cap_the_run_named_stays_with_refused_starts() {
        let r = refused("6\n", Some("awaiting-human-merge"));
        assert_eq!(end_cap(&r), Some("awaiting-human-merge"), "{:?}", r.end);
        assert_eq!(r.history["cap"], "awaiting-human-merge");
        assert!(
            !r.after.ended.reason.contains("limit of processes"),
            "{}",
            r.after.ended.reason
        );
    }

    #[test]
    fn no_refused_starts_name_no_process_limit() {
        let r = refused("0\n", None);
        assert_eq!(end_cap(&r), None, "{:?}", r.end);
        assert!(r.history.get("cap").is_none(), "{}", r.history);
    }
}
