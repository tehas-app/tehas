//! A cycle's report applied: the writes of `report`, in its order, with the
//! tries and bounds of libexec/cycle. Every write to GitHub goes through the
//! action journal, and each try is a row of its own. Nothing decides a report
//! again and tehas:running left on an issue parks it for good, so a write the
//! journal cannot record is made with no row. Then the end on the cycle's
//! record, 5 tries, and what follows it. Run after tehas work with the signals
//! the report ignores already ignored, and with the cycle's lock held.

use crate::after::{self, log};
use crate::facts::Facts;
use crate::host::{self, Start};
use crate::report::{report, Action, Ask, Park, Report};
use crate::shadow::BOXES;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;
use tehas_core::call::{run_full, store_try, GRACE};
use tehas_core::journal::{Journal, Wrote};

/// A gh call of the report: long past any answer. The bash had no bound.
const GH_BOUND: Duration = Duration::from_secs(120);
/// The read of a develop timeout's issue and the write of its note.
const NOTE_BOUND: Duration = Duration::from_secs(30);

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}

/// The bounds of the report's calls to GitHub. After a stop every call is one
/// bounded try, and KILL: the report ignores TERM, and gh inherits that.
#[derive(Clone, Copy)]
struct Gh {
    tries: u32,
    retry_wait: Duration,
    bound: Duration,
    grace: Duration,
}

impl Gh {
    fn of(signalled: bool) -> Gh {
        let wait = env("TEHAS_CYCLE_RETRY_WAIT").parse().unwrap_or(5);
        if !signalled {
            return Gh {
                tries: 3,
                retry_wait: Duration::from_secs(wait),
                bound: GH_BOUND,
                grace: GRACE,
            };
        }
        Gh {
            tries: 1,
            retry_wait: Duration::ZERO,
            bound: Duration::from_secs(stop_wait()),
            grace: Duration::ZERO,
        }
    }
}

/// TEHAS_CYCLE_STOP_GH_WAIT, 20 when it is no number of one to four digits.
fn stop_wait() -> u64 {
    let v = env("TEHAS_CYCLE_STOP_GH_WAIT");
    let digits =
        (1..=4).contains(&v.len()) && !v.starts_with('0') && v.bytes().all(|b| b.is_ascii_digit());
    if digits {
        v.parse().unwrap_or(20)
    } else {
        20
    }
}

fn strs(v: &[String]) -> Vec<&str> {
    v.iter().map(String::as_str).collect()
}

/// GitHub asked with gh, with the report's tries.
struct Tried {
    repo: String,
    issue: u64,
    gh: Gh,
}

impl Tried {
    fn call(&self, args: &[String], bound: Duration, grace: Duration) -> Option<Value> {
        let r = run_full("gh", args, None, bound, grace, &[]);
        r.ok().then(|| serde_json::from_str(&r.out).ok()).flatten()
    }

    fn tried(&self, args: &[String], fit: fn(&Value) -> bool) -> Option<Value> {
        for t in 1..=self.gh.tries {
            let got = self.call(args, self.gh.bound, self.gh.grace).filter(fit);
            if got.is_some() {
                return got;
            }
            if t < self.gh.tries {
                std::thread::sleep(self.gh.retry_wait);
            }
        }
        None
    }

    fn issue_args(&self) -> Vec<String> {
        vec![
            "api".into(),
            format!("repos/{}/issues/{}", self.repo, self.issue),
        ]
    }
}

impl Ask for Tried {
    fn prs(&mut self, branch: &str) -> Option<Vec<Value>> {
        let args: Vec<String> = [
            "pr",
            "list",
            "--repo",
            &self.repo,
            "--head",
            branch,
            "--author",
            "@me",
            "--state",
            "all",
            "--json",
            "url,state,body,headRefName",
            "--limit",
            "20",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        self.tried(&args, Value::is_array)
            .and_then(|v| v.as_array().cloned())
    }

    fn attention(&mut self) -> Option<bool> {
        let v = self.tried(&self.issue_args(), Value::is_object)?;
        Some(v.get("labels").and_then(Value::as_array).is_some_and(|l| {
            l.iter()
                .any(|x| x.get("name").and_then(Value::as_str) == Some("needs-human-attention"))
        }))
    }

    fn issue(&mut self) -> Option<Value> {
        self.call(&self.issue_args(), NOTE_BOUND, GRACE)
    }
}

/// The writes of one report.
struct Writer<'a> {
    home: &'a Path,
    repo: String,
    issue: u64,
    attempt: u64,
    gh: Gh,
    before: i64,
    poll_at: i64,
    no_row: bool,
    /// The journal failed a write. The rest go with no row: a hung store
    /// must not spend the time a stop leaves.
    unjournaled: bool,
    /// The label stayed: the next comment says so.
    stuck: bool,
}

impl Writer<'_> {
    fn write(
        &mut self,
        issue: u64,
        kind: &str,
        args: Vec<String>,
        input: Option<&str>,
        done_if: Option<&str>,
    ) -> Wrote {
        self.write_within(issue, kind, args, input, done_if, self.gh.bound)
    }

    fn write_within(
        &mut self,
        issue: u64,
        kind: &str,
        args: Vec<String>,
        input: Option<&str>,
        done_if: Option<&str>,
        bound: Duration,
    ) -> Wrote {
        let home = self.home;
        // A busy store must not lose a report: nothing decides it again.
        let mut store =
            |a: &[&str]| store_try(3, Duration::from_secs(1), || host::store_value(home, a));
        let mut journal = Journal {
            repo: &self.repo,
            gh: "gh",
            before: self.before,
            bound,
            grace: self.gh.grace,
            token: None,
            poll_at: &mut self.poll_at,
            row_not_begun: &mut self.no_row,
            cycle: Some((self.issue, self.attempt)),
        };
        if !self.unjournaled {
            let wrote = journal.write(&mut store, issue, kind, args.clone(), input, done_if);
            if wrote != Wrote::Unrecorded {
                return wrote;
            }
            self.unjournaled = true;
            log(&format!(
                "WARN: the journal will not record the report of #{}: its writes go to GitHub with no row",
                self.issue
            ));
        }
        journal.unrecorded(args, input, done_if)
    }

    /// A write tried as the bash tried it, `warn` logged when all tries fail.
    fn tried(&mut self, warn: &str, kind: &str, args: Vec<String>, input: Option<&str>) -> bool {
        let n = self.issue;
        for t in 1..=self.gh.tries {
            if matches!(
                self.write(n, kind, args.clone(), input, None),
                Wrote::Done(_)
            ) {
                return true;
            }
            if t < self.gh.tries {
                std::thread::sleep(self.gh.retry_wait);
            }
        }
        log(warn);
        false
    }

    fn relabel(&mut self, remove: &[String], add: &[String]) -> bool {
        let mut flags: Vec<String> = vec![];
        for l in remove {
            flags.extend(["--remove-label".to_string(), l.clone()]);
        }
        for l in add {
            flags.extend(["--add-label".to_string(), l.clone()]);
        }
        let warn = format!(
            "WARN: label change on #{} failed: {}",
            self.issue,
            flags.join(" ")
        );
        let mut args: Vec<String> = vec![
            "issue".into(),
            "edit".into(),
            self.issue.to_string(),
            "--repo".into(),
            self.repo.clone(),
        ];
        args.extend(flags);
        let kind = if add.is_empty() {
            "label-remove"
        } else {
            "label-add"
        };
        self.tried(&warn, kind, args, None)
    }

    fn comment(&mut self, body: &str) {
        let warn = format!("WARN: comment on #{} failed", self.issue);
        let args = vec![
            "issue".into(),
            "comment".into(),
            self.issue.to_string(),
            "--repo".into(),
            self.repo.clone(),
            "--body-file".into(),
            "-".into(),
        ];
        self.tried(&warn, "comment-post", args, Some(body));
    }

    fn park(&mut self, p: &mut Park, f: &Facts, ask: &mut dyn Ask) {
        let n = self.issue;
        let running = ["tehas:running".to_string()];
        let attention = ["needs-human-attention".to_string()];
        if self.relabel(&running, &attention) {
            self.stuck = false;
        }
        // On record before the issue is told: a question whose park is not on
        // record would offer boxes that start nothing.
        let mut park = vec!["park"];
        park.extend(strs(&p.args));
        let on_record = match host::store_try(self.home, 3, &park) {
            Ok(_) => true,
            Err(e) => {
                log(&format!(
                    "WARN: could not record the park of #{n} ({}: {}), so it asks no question",
                    e.word, e.line
                ));
                false
            }
        };
        p.last_attempt(f, ask);
        if let Some(body) = p.edit.clone() {
            let args = vec![
                "api".into(),
                "-X".into(),
                "PATCH".into(),
                format!("repos/{}/issues/{n}", self.repo),
                "--input".into(),
                "-".into(),
            ];
            let input = json!({"body": body}).to_string();
            let done = self.write_within(n, "issue-edit", args, Some(&input), None, NOTE_BOUND);
            if !matches!(done, Wrote::Done(_)) {
                log(&format!(
                    "WARN: could not leave the last attempt's note in the body of #{n}"
                ));
            }
        }
        if on_record {
            let (_, body) = p.question(n, self.stuck);
            if let Some(id) = self.post_question(&body) {
                self.asked(&id, &body);
                return;
            }
        }
        let said = p.comment(self.stuck, on_record);
        self.comment(&said);
    }

    /// The question's id, None when it could not be posted.
    fn post_question(&mut self, body: &str) -> Option<String> {
        let n = self.issue;
        let args = vec![
            "api".into(),
            format!("repos/{}/issues/{n}/comments", self.repo),
            "--input".into(),
            "-".into(),
            "--jq".into(),
            ".id".into(),
        ];
        let input = json!({"body": body}).to_string();
        let Wrote::Done(out) = self.write(n, "comment-post", args, Some(&input), None) else {
            return None;
        };
        let id = out.trim().to_string();
        let digits = (1..=16).contains(&id.len())
            && !id.starts_with('0')
            && id.bytes().all(|b| b.is_ascii_digit());
        digits.then_some(id)
    }

    /// On record before the label shows it: a tick counts only in the
    /// question on record. The poll reads questions only on issues with the
    /// label, so without it the boxes do nothing: say so.
    fn asked(&mut self, id: &str, body: &str) {
        let (home, n) = (self.home, self.issue.to_string());
        let sum = crate::last_attempt::sha(body);
        let asked = [
            "question-asked",
            &n,
            "--id",
            id,
            "--kind",
            "park",
            "--body-sha",
            &sum,
        ];
        if let Err(e) = host::store_try(home, 3, &asked) {
            log(&format!(
                "WARN: could not record the park question of #{n}, its boxes will not work ({})",
                e.word
            ));
        }
        if !self.relabel(&[], &["tehas:question".to_string()]) {
            let withdraw = ["question-answered", &n, "--id", id, "--withdraw"];
            if let Err(e) = host::store_try(home, 3, &withdraw) {
                if e.word != "unknown" {
                    log(&format!(
                        "WARN: could not take the park question of #{n} off the record ({})",
                        e.word
                    ));
                }
            }
            self.comment(BOXES);
        }
    }

    fn pr_comment(&mut self, pr: &str, body: &str) {
        let n = pr
            .rsplit('/')
            .next()
            .and_then(|t| t.parse().ok())
            .unwrap_or(self.issue);
        let args = vec![
            "pr".into(),
            "comment".into(),
            pr.to_string(),
            "--body-file".into(),
            "-".into(),
        ];
        if !matches!(
            self.write(n, "comment-post", args, Some(body), None),
            Wrote::Done(_)
        ) {
            log(&format!("WARN: comment on {pr} failed"));
        }
    }

    /// tehas:refreshing off its pull request, through the REST issues
    /// endpoint: `gh pr edit` asks GitHub a GraphQL question the bot's token
    /// may not answer (issue #120). A 404 says the label is not there, which
    /// is what this wants.
    fn refresh_off(&mut self, pr: &str) {
        let n = pr.parse().unwrap_or(self.issue);
        let args: Vec<String> = vec![
            "api".into(),
            "-X".into(),
            "DELETE".into(),
            format!("repos/{}/issues/{pr}/labels/tehas:refreshing", self.repo),
        ];
        for t in 1..=self.gh.tries {
            if matches!(
                self.write(n, "refresh-end", args.clone(), None, Some("Not Found")),
                Wrote::Done(_)
            ) {
                return;
            }
            if t < self.gh.tries {
                std::thread::sleep(self.gh.retry_wait);
            }
        }
        log(&format!(
            "WARN: label change on pull request #{pr} failed: --remove-label tehas:refreshing"
        ));
    }
}

/// `rm -rf`: gone or never there.
fn remove_all(p: &str) -> bool {
    match fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => fs::remove_dir_all(p).is_ok(),
        Ok(_) => fs::remove_file(p).is_ok(),
        Err(_) => true,
    }
}

/// The cycle's images and its own state: only the container read them. The
/// workspace stays for a person; what its agents wrote beside it does not.
fn clear_workspace(workdir: &str) {
    let marked = Path::new(workdir)
        .parent()
        .is_some_and(|p| p.join(".tehas-workspaces").is_file());
    if !marked {
        return;
    }
    for (ext, what) in [(".images", "the images"), (".state", "the cycle's state")] {
        let p = format!("{workdir}{ext}");
        if !remove_all(&p) {
            log(&format!("WARN: could not remove {what} {p}"));
        }
    }
}

/// The notes of the protocol, read once: the next cycle of the issue must not
/// take them for its own.
fn clear_notes(home: &Path, issue: u64) {
    for name in [
        "oom",
        "forks",
        "stalled",
        "timed",
        "stopped",
        "nostart",
        "done",
        "runner_exit",
        "status",
    ] {
        let _ = fs::remove_file(home.join(format!("state/cycles/{issue}.{name}")));
    }
}

/// The pull request's title. A failure is a warning.
fn sync_title(console: &str, pr: &str, signalled: bool) {
    let wait = if signalled { stop_wait() } else { 120 };
    let out = Command::new("timeout")
        .args([
            "-k",
            "5",
            &wait.to_string(),
            console,
            "pr-title",
            "sync",
            pr,
        ])
        .stdin(std::process::Stdio::null())
        .output();
    let (ok, said) = match out {
        Ok(o) => (
            o.status.success(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            ),
        ),
        Err(_) => (false, String::new()),
    };
    if !ok {
        let why = said.trim_end();
        let why = if why.is_empty() {
            "it took too long"
        } else {
            why
        };
        log(&format!(
            "WARN: the title of pull request #{pr} was not kept: {why}"
        ));
    }
}

/// `store cycle-end`, with the paths of the result and the progress in their
/// place. False when the end is not on record.
fn end(home: &Path, r: &Report, f: &Facts) -> bool {
    let issue = f.issue;
    let cycles = home.join("state/cycles");
    let mut args: Vec<String> = r.end[..r.files_at].to_vec();
    for k in &r.files {
        args.push(format!("--{k}"));
        args.push(
            cycles
                .join(format!("{issue}.{k}.json"))
                .to_string_lossy()
                .into_owned(),
        );
    }
    args.extend_from_slice(&r.end[r.files_at..]);
    let mut call = vec!["cycle-end"];
    call.extend(strs(&args));
    match host::store_try(home, 5, &call) {
        Ok(v) => {
            for k in ["result", "progress"] {
                let kept = v.get(k).and_then(Value::as_str).unwrap_or("");
                if ["refused", "too-large", "unreadable", "stale"].contains(&kept) {
                    log(&format!(
                        "WARN: the {k} of #{issue} is not on record: {kept}"
                    ));
                }
            }
            true
        }
        Err(e) => {
            log(&format!(
                "ERROR: the end of #{issue} is not on record ({}: {}): cycle-end {}",
                e.word,
                e.line,
                args.join(" ")
            ));
            false
        }
    }
}

/// The program `name` of this cycle's code: the command a test names, else
/// beside tehas-cycle in the copy a pinned cycle runs from.
fn program(home: &Path, var: &str, name: &str) -> String {
    let given = env(var);
    if !given.is_empty() {
        return given;
    }
    let code = env("TEHAS_CODE");
    let root = if code.is_empty() {
        home.to_path_buf()
    } else {
        Path::new(&code).to_path_buf()
    };
    root.join("libexec")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

/// Report the cycle `s`. The exit status of the cycle: 0, or 70 when its end
/// is not on record.
pub fn run(home: &Path, s: &Start) -> i32 {
    let issue = s.issue;
    let workdir = format!("{}-{issue}", env("TEHAS_WORK_DIR"));
    let now = s.now.unwrap_or_else(tehas_core::time::now);
    let facts_json = host::facts(home, s);
    clear_workspace(&workdir);
    let facts = match Facts::from_json(&facts_json) {
        Ok(f) => f,
        Err(e) => {
            log(&format!("ERROR: the end of #{issue} cannot be read: {e}"));
            return 70;
        }
    };
    let gh = Gh::of(s.signalled);
    let mut ask = Tried {
        repo: facts.repo.clone(),
        issue,
        gh,
    };
    let mut r = report(&facts, &mut ask);
    clear_notes(home, issue);
    for line in &r.log {
        log(line);
    }
    let mut w = Writer {
        home,
        repo: facts.repo.clone(),
        issue,
        attempt: s.attempt,
        gh,
        before: now,
        poll_at: 0,
        no_row: false,
        unjournaled: false,
        stuck: true,
    };
    for a in &mut r.actions {
        match a {
            Action::Relabel { remove, add } => {
                if w.relabel(remove, add) {
                    w.stuck = false;
                }
            }
            Action::Comment { ok, stuck } => {
                let said = if w.stuck { stuck.clone() } else { ok.clone() };
                w.comment(&said);
            }
            Action::Park(p) => w.park(p, &facts, &mut ask),
            Action::PrComment { pr, body } => w.pr_comment(pr, body),
            Action::Tidy { remove, warn } => {
                if !warn.is_empty() {
                    log(warn);
                } else if !remove.iter().fold(true, |ok, p| remove_all(p) && ok) {
                    log(&format!("WARN: could not remove the workspace {workdir}"));
                }
            }
            Action::RefreshOff { pr } => w.refresh_off(pr),
        }
    }
    log(&r.done);
    let console = program(home, "TEHAS_CONSOLE_CMD", "console");
    let prefix = format!("https://github.com/{}/pull/", facts.repo);
    if let Some(n) = r.after.ended.pr.strip_prefix(&prefix) {
        sync_title(&console, n, s.signalled);
    }
    let approved = after::approved(home, issue, s.attempt);
    let on_record = end(home, &r, &facts);
    let mut a = r.after.clone();
    a.approved = approved;
    a.console = console;
    a.status_cmd = program(home, "TEHAS_STATUS_CMD", "status");
    a.workdir = workdir;
    after::after(home, &a);
    if on_record {
        0
    } else {
        70
    }
}
