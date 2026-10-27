//! The starts of a decision, as start_cycles in libexec/dispatch does them:
//! for each, the refresh's mark, the approval on record with its inputs, the
//! attempt before, the park's question off the record, the mark (A1: the
//! issue is marked running before its unit starts), the park's retry spent,
//! the comment, the unit and the wait for its start. A start that gives up
//! puts back what it changed. The poll ends with the starts: 1 when one gave
//! up, 78 when the store could not be used before any issue was marked.
//!
//! What the poll does before its starts (systemd's answer, the code index,
//! the checkout) is the poll's, part 4 of #254.

use super::{text, word, Writer, Wrote};
use crate::decide::iso;
use crate::read::host::{epoch, held};
use crate::read::run::run_quiet;
use serde_json::{json, Map, Value};
use std::io::Write as _;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// What a start reads off the host and the settings.
pub struct Host {
    /// hostname, for the comments.
    pub name: String,
    /// TEHAS_WORK_COMMENTS is 1: the approval names the comments' links.
    pub comments: bool,
    /// TEHAS_ALLOWED_LABELERS.
    pub allowed: Vec<String>,
    /// TEHAS_MILLWRIGHT_LOGIN.
    pub millwright: String,
    /// The bot's login, whose pull requests a retry's comments are read on.
    pub bot: String,
    pub systemctl: String,
    /// TEHAS_CYCLE_START_CMD and TEHAS_CYCLE_STOP_CMD, which a test sets.
    pub start_cmd: Option<String>,
    pub stop_cmd: Option<String>,
    /// TEHAS_CYCLE_START_WAIT.
    pub start_wait: Duration,
    /// TEHAS_STATUS_CMD, run once the starts are done.
    pub status_cmd: String,
    /// $TEHAS_HOME/libexec/analytics when TEHAS_ANALYTICS is on.
    pub analytics: Option<String>,
    pub analytics_wait: Duration,
    /// TEHAS_WORK_DIR, whose siblings are the issues' workspaces.
    pub work_dir: String,
    /// The tehas user's home, where ensemble-runs is.
    pub user_home: String,
    /// TEHAS_CYCLE_DONE_WAIT: how long the stop of a finished cycle waits.
    pub done_wait: Duration,
    /// PI_ENSEMBLE_IMAGE, whose pi-rukas a planning turn's end names.
    pub image: String,
    /// TEHAS_WORK_REPO, the product's checkout, and its default branch.
    pub checkout: String,
    pub branch: String,
    pub git: String,
    /// TEHAS_INDEX_CMD, and TEHAS_INDEX_START_CMD, a test's.
    pub index_cmd: String,
    pub index_start_cmd: Option<String>,
    /// TEHAS_FETCH_WAIT: the bound of the checkout's fetch.
    pub fetch_wait: Duration,
    /// TEHAS_PLAN_CMD: the program a turn's unit runs, libexec/plan.
    pub plan_cmd: String,
    /// TEHAS_PLAN_START_CMD, which a test sets in place of systemd-run.
    pub plan_start_cmd: Option<String>,
    /// TEHAS_PLAN_TIMEOUT, in seconds: the turn's own limit.
    pub plan_timeout: u64,
    /// TEHAS_PLAN_MAX_TURNS.
    pub plan_max_turns: u64,
    pub systemd_run: String,
    /// How often the wait for a stopped cycle looks at its lock. Production:
    /// 500 ms, as the bash stop polls.
    pub lock_poll: Duration,
    /// How often the wait for a launched turn's unit looks for its lock or
    /// its rc. Production: 500 ms, as the bash plan polls.
    pub launch_poll: Duration,
}

/// How one start ended.
enum Then {
    Started,
    /// Not started and nothing to put back: the next issue.
    Skipped,
    /// Gave up: the poll ends with 1.
    Failed,
    /// The store cannot be used: no issue after this one starts.
    Unusable,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// An id as a store argument: a number or a string, as jq -r prints it.
fn id(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// `issue_alone`: the tick on `Retry from the issue alone`, whose start names
/// no comment of the issue or of the pull request. Its absence is a clean
/// start, which is what a record written before the box existed holds. A
/// value that is not a bool is no answer: the reader says so rather than
/// taking it for the clean start that reads the issue's comments.
fn issue_alone(a: &Value, log: &mut Vec<String>, n: u64) -> bool {
    match a.get("issue_alone") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::Bool(true)) => true,
        Some(v) => {
            log.push(format!(
                "WARN: #{}: its start names issue_alone {v}, which is neither true nor false: the record is written as a clean start's, and its comments are read",
                n
            ));
            false
        }
    }
}

/// The comment window of libexec/dispatch: of the comments as the REST API
/// lists them, those an allowed account wrote at or after `since` and at or
/// before `at`, and that nobody edited after `at`, in the order written.
/// None when the answer is not a list of comments.
fn window(list: &Value, since: &str, at: &str, who: &dyn Fn(&str) -> bool) -> Option<Vec<Value>> {
    let all = list.as_array()?;
    let s = |c: &Value, k: &str| c.get(k).and_then(Value::as_str).map(str::to_string);
    let mut out: Vec<Value> = all
        .iter()
        .filter(|c| {
            let (Some(created), Some(updated)) = (s(c, "created_at"), s(c, "updated_at")) else {
                return false;
            };
            let login = c
                .pointer("/user/login")
                .and_then(Value::as_str)
                .unwrap_or("");
            c.get("id").is_some_and(Value::is_number)
                && c.get("body").is_some_and(Value::is_string)
                && !login.is_empty()
                && who(login)
                && !at.is_empty()
                && created.as_str() >= since
                && created.as_str() <= at
                && updated.as_str() <= at
        })
        .cloned()
        .collect();
    out.sort_by(|a, b| {
        let k = |c: &Value| {
            (
                s(c, "created_at").unwrap_or_default(),
                c.get("id").and_then(Value::as_f64).unwrap_or(0.0),
            )
        };
        let (x, y) = (k(a), k(b));
        x.0.cmp(&y.0).then(x.1.total_cmp(&y.1))
    });
    Some(out)
}

/// The links of the comments, each link once, with where it came from.
fn comment_links(comments: &[Value], source: &str) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for c in comments {
        let body = c.get("body").and_then(Value::as_str).unwrap_or("");
        for link in crate::text::link_list(body) {
            if !out.iter().any(|x| x["link"] == json!(link)) {
                out.push(
                    json!({"link": link, "source": format!("{source}:{}", c["id"]),
                    "by": c.pointer("/user/login").cloned().unwrap_or(Value::Null)}),
                );
            }
        }
    }
    out
}

/// A comment as retry_context has it: its first 4000 characters, without
/// carriage returns.
fn context(c: &Value, source: &str) -> Value {
    let body: String = c
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or("")
        .replace('\r', "")
        .chars()
        .take(4000)
        .collect();
    json!({"source": format!("{source}:{}", c["id"]), "by": c.pointer("/user/login").cloned().unwrap_or(Value::Null),
        "at": c.get("created_at").cloned().unwrap_or(Value::Null), "text": body})
}

/// tehas_names_issue: a pull request is the issue's when its head is the
/// issue's branch or its body closes it.
fn names_issue(pr: &Value, n: u64) -> bool {
    use regex::Regex;
    let head = pr.get("headRefName").and_then(Value::as_str).unwrap_or("");
    let branch = format!("feature/issue-{n}");
    if head == branch || head.starts_with(&format!("{branch}-")) {
        return true;
    }
    let body = pr.get("body").and_then(Value::as_str).unwrap_or("");
    Regex::new(&format!(
        r"(?i)\b(close[sd]?|fix(e[sd])?|resolve[sd]?):?\s+#{n}([^0-9]|$)"
    ))
    .is_ok_and(|re| re.is_match(body))
}

impl Writer {
    /// start_cycles: a run of start actions. The poll's exit status when it
    /// ends there, None when every start was done or skipped.
    pub(super) fn starts(
        &mut self,
        starts: &[Value],
        log: &mut Vec<String>,
        printed: &mut Vec<String>,
    ) -> Option<i32> {
        // Cycles start from the checkout's tip: refreshed here when the
        // index's gate did not, as when a cycle runs.
        if !self.fetched {
            let mut lines = Vec::new();
            self.refresh_checkout(&mut lines);
            log.extend(lines);
        }
        let mut failed = false;
        let mut marked = false;
        for a in starts {
            match self.start(a, &mut marked, log) {
                Then::Started | Then::Skipped => {}
                Then::Failed => failed = true,
                Then::Unusable => {
                    if !marked {
                        return Some(78);
                    }
                    failed = true;
                    break;
                }
            }
        }
        self.status(printed);
        failed.then_some(1)
    }

    /// The status command, once the poll's work is done; what it printed.
    pub fn status(&mut self, printed: &mut Vec<String>) {
        let cmd = self.host.status_cmd.clone();
        self.status_of(&cmd, &[], printed);
    }

    /// A command of the host whose output the poll's log gets, as status's
    /// and tidy's.
    /// Its exit status, None when it did not end in its bound.
    pub fn status_of(
        &mut self,
        cmd: &str,
        args: &[String],
        printed: &mut Vec<String>,
    ) -> Option<i32> {
        // Its output through a file: a pipe would wait for whatever it left
        // running. With no file to write, it runs and its output is lost.
        let out = self
            .scratch("status")
            .and_then(|p| std::fs::File::create(&p).ok().map(|f| (p, f)));
        let Some((path, f)) = out else {
            return run_quiet(cmd, args, None, Duration::from_secs(120));
        };
        let rc = run_quiet(cmd, args, Some(f), Duration::from_secs(120));
        let said = std::fs::read_to_string(&path).unwrap_or_default();
        printed.extend(said.lines().map(str::to_string));
        let _ = std::fs::remove_file(&path);
        rc
    }

    fn start(&mut self, a: &Value, marked: &mut bool, log: &mut Vec<String>) -> Then {
        let n = a.get("start").and_then(Value::as_u64).unwrap_or(0);
        let by = text(a, "by").to_string();
        let given = text(a, "given").to_string();
        log.push(match a.get("log").and_then(Value::as_str) {
            Some(line) => line.to_string(),
            None => format!("start #{n} ({} by {by})", text(a, "verb")),
        });
        let refresh_pr = a.get("refresh_pr").and_then(Value::as_u64);
        if let Some(pr) = refresh_pr {
            let args = vec![
                "api".into(),
                "-X".into(),
                "POST".into(),
                format!("repos/{}/issues/{pr}/labels", self.repo),
                "-f".into(),
                "labels[]=tehas:refreshing".into(),
            ];
            let w = self.write(pr, "refresh-begin", args, None, None);
            if !matches!(w, Wrote::Done(_)) {
                if matches!(w, Wrote::Unrecorded) {
                    log.push(Writer::unrecorded(pr));
                }
                log.push(format!(
                    "ERROR: could not mark pull request #{pr} as refreshing, not starting a cycle"
                ));
                return Then::Failed;
            }
        }
        let recorded = match self.approval(n, a, log) {
            Ok(at) => at,
            Err((w, line)) => {
                return match w.as_str() {
                    "used" => {
                        let at = text(a, "at");
                        let at = if at.is_empty() { "now" } else { at };
                        log.push(format!(
                            "skip #{n}: its approval of {at} started a cycle already"
                        ));
                        Then::Skipped
                    }
                    "" | "busy" | "timeout" | "conflict" | "input" => {
                        let said = if w.is_empty() {
                            String::new()
                        } else {
                            format!(" ({w})")
                        };
                        log.push(format!(
                            "ERROR: could not write the approval record of #{n}{said}, not starting a cycle"
                        ));
                        Then::Failed
                    }
                    _ => {
                        log.push(format!(
                            "ERROR: the state store cannot be used, not starting a cycle for #{n}: {line}"
                        ));
                        Then::Unusable
                    }
                };
            }
        };
        let before = match self.calls.store(&["cycle-show", &n.to_string()]) {
            Ok(v) => match v.pointer("/cycle/attempt").cloned().unwrap_or(json!(0)) {
                Value::Number(x) if x.as_u64().is_some_and(|a| a < 1_000_000_000) => x.as_u64(),
                Value::Null => Some(0),
                _ => None,
            }
            .ok_or_else(String::new),
            Err(f) => Err(word(&f)),
        };
        let before = match before {
            Ok(b) => b,
            Err(w) => {
                log.push(format!(
                    "ERROR: #{n}: the store could not say which attempt is on record ({w}), not starting a cycle"
                ));
                self.restore(n, recorded, log);
                return Then::Failed;
            }
        };
        let (park_now, unasked) = match self.park_unask(n, log) {
            Ok(p) => p,
            Err(w) => {
                log.push(format!(
                    "ERROR: #{n}: the question of its park could not be taken off the record ({w}), not starting a cycle"
                ));
                self.restore(n, recorded, log);
                return Then::Failed;
            }
        };
        let accepted = match given.as_str() {
            "refresh" | "outage-retry" => Some(recorded),
            _ if !text(a, "at").is_empty() => Some(recorded),
            _ => None,
        };
        let mut event = vec!["event".to_string(), n.to_string(), "ready".into()];
        if let Some(at) = accepted {
            event.extend(["--at".into(), at.to_string()]);
        }
        self.analytics(&event, log);
        // A1: the issue is marked as taken before anything starts; a mark
        // that fails leaves it in its queue for the next poll.
        let mut args: Vec<String> = vec![
            "issue".into(),
            "edit".into(),
            n.to_string(),
            "--repo".into(),
            self.repo.clone(),
            "--add-label".into(),
            "tehas:running".into(),
        ];
        for l in super::strings(a, "remove") {
            args.extend(["--remove-label".into(), l]);
        }
        match self.write(n, "label-add", args, None, None) {
            Wrote::Done(_) => {}
            other => {
                if matches!(other, Wrote::Unrecorded) {
                    log.push(Writer::unrecorded(n));
                }
                self.restore(n, recorded, log);
                self.park_reask(n, unasked.as_deref(), log);
                log.push(format!(
                    "ERROR: could not mark #{n} as running, not starting a cycle"
                ));
                return Then::Failed;
            }
        }
        *marked = true;
        self.starts += 1;
        if park_now {
            if let Err(f) = self.store_try(3, &["park-retry", &n.to_string()]) {
                if word(&f) != "unparked" {
                    log.push(format!(
                        "WARN: #{n}: the retry of its park could not be spent ({})",
                        word(&f)
                    ));
                }
            }
        }
        let host = self.host.name.clone();
        let when = iso(now() as u64).replacen('T', " ", 1);
        let when = &when[..16];
        let how = text(a, "how");
        if refresh_pr.is_some() {
            let _ = std::fs::remove_file(self.home.join(format!("state/refresh/{n}.json")));
            let branch = a
                .pointer("/refresh_of/branch")
                .and_then(Value::as_str)
                .unwrap_or("");
            let url = text(a, "refresh_url");
            self.say(n, &format!("Tehas refresh started on `{host}` at {when} UTC: {url} conflicts with `{branch}`. This cycle merges that branch into it, resolves the conflicts and pushes to the same pull request. It rests on @{by}'s {how}. This issue is owned by the cycle until `tehas:running` is removed."), log);
        } else {
            self.say(n, &format!("Tehas cycle started on `{host}` at {when} UTC (authorized by @{by}'s {how}). This issue is owned by the cycle until `tehas:running` is removed."), log);
        }
        if self.unit_start(n, before, log) {
            self.analytics(&["event".into(), n.to_string(), "started".into()], log);
            return Then::Started;
        }
        // A unit that comes up after this finds its approval gone and runs
        // nothing; when the approval stays on record, the unit is stopped.
        if !self.restore(n, recorded, log) && !self.unit_stop(n) {
            log.push(format!(
                "WARN: could not stop tehas-cycle@{n}.service: a late start may still use its approval"
            ));
        }
        log.push(format!("ERROR: the cycle for #{n} did not start"));
        let args = vec![
            "issue".into(),
            "edit".into(),
            n.to_string(),
            "--repo".into(),
            self.repo.clone(),
            "--remove-label".into(),
            "tehas:running".into(),
            "--add-label".into(),
            "needs-human-attention".into(),
        ];
        if !matches!(self.write(n, "label-add", args, None, None), Wrote::Done(_)) {
            log.push(format!("WARN: label change on #{n} failed"));
        }
        self.say(n, &format!("Tehas could not start the cycle for this issue on `{host}`: `tehas-cycle@{n}.service` did not come up. See `journalctl --user -u tehas-cycle@{n}` and `~/tehas/logs/dispatch.log` there. To retry, apply `tehas:ready`."), log);
        Then::Failed
    }

    /// approved_record: the approval on record with its inputs. Its time,
    /// or the store's word and line; an empty word is a read that failed.
    fn approval(
        &mut self,
        n: u64,
        a: &Value,
        log: &mut Vec<String>,
    ) -> Result<i64, (String, String)> {
        let none = || (String::new(), String::new());
        if a.get("text") != Some(&json!(true)) {
            return Err(none());
        }
        let given = text(a, "given").to_string();
        let given_at = text(a, "at").to_string();
        let mut input = Map::new();
        input.insert("links".into(), a.get("links").cloned().unwrap_or(json!([])));
        let mut retry = Value::Null;
        let mut from: Vec<Value> = Vec::new();
        // The comment policy of this start, derived once: the host reads
        // comments, and the tick of the issue alone asks for none of them,
        // so its record reads as the one of an instance with comments off.
        // The bash derives the same single value in approved_record.
        let comments = self.host.comments && !issue_alone(a, log, n);
        if comments {
            let Some(list) = self.comments(n) else {
                log.push(format!("ERROR: the comments of #{n} could not be read"));
                return Err(none());
            };
            let allowed = self.host.allowed.clone();
            let who = |l: &str| allowed.iter().any(|x| x == l);
            let Some(w) = window(&list, "", &given_at, &who) else {
                log.push(format!("ERROR: the comments of #{n} could not be read"));
                return Err(none());
            };
            from = comment_links(&w, "comment");
            if given == "retry-tick" {
                let since = text(a, "since").to_string();
                let Some(r) = self.pr_comments(n, &since, &given_at) else {
                    log.push(format!(
                        "ERROR: the comments on the pull request of #{n} could not be read"
                    ));
                    return Err(none());
                };
                retry = r;
            }
        }
        // A start of the issue alone names no comment of an issue or a pull
        // request, so nothing of the earlier cycle goes with it: the note of
        // the fixer or of a refresh stays out of its record whatever asks.
        if !issue_alone(a, log, n) {
            if given == "millwright" {
                let Some(r) = self.fixer_note(n, &given_at) else {
                    log.push(format!(
                        "ERROR: the comments of #{n} could not be read for the fixer's"
                    ));
                    return Err(none());
                };
                retry = r;
            }
            if given == "refresh" {
                let b = a
                    .pointer("/refresh_of/branch")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                retry = json!({"links": [], "context": [{"source": "refresh", "by": "tehas", "at": iso(now() as u64),
                    "text": format!("This pull request conflicts with `{b}`. Merge `{b}` into its branch, resolve the conflicts so the change still does what the issue asks, run the gate, and push to the same branch. Do no other work.")}]});
            }
        }
        if comments {
            from.extend(
                retry
                    .get("links")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
            );
            input.insert("comment_links".into(), Value::Array(from));
        }
        if retry
            .get("context")
            .and_then(Value::as_array)
            .is_some_and(|c| !c.is_empty())
        {
            input.insert("retry_context".into(), retry["context"].clone());
        }
        if given == "refresh" {
            input.insert(
                "refresh_of".into(),
                a.get("refresh_of").cloned().unwrap_or(Value::Null),
            );
        }
        let at = match given.as_str() {
            "refresh" | "outage-retry" => None,
            _ => epoch(&given_at),
        }
        .filter(|t| (1..100_000_000_000).contains(t))
        .unwrap_or_else(now);
        let Some(file) = self.input_file(&Value::Object(input).to_string()) else {
            return Err(none());
        };
        let f = file.to_string_lossy().into_owned();
        let mut args: Vec<String> = vec![
            "approval-record".into(),
            n.to_string(),
            "--at".into(),
            at.to_string(),
            "--by".into(),
            text(a, "by").into(),
            "--how".into(),
            given.clone(),
            "--body-sha".into(),
            text(a, "body_sha256").into(),
            "--input-file".into(),
            f,
        ];
        if matches!(given.as_str(), "label" | "millwright") {
            args.extend(["--label".into(), "tehas:ready".into()]);
        }
        if let Some(t) = a.get("title_sha256").and_then(Value::as_str) {
            args.extend(["--title-sha".into(), t.into()]);
        }
        let comment = id(a.get("comment"));
        if !comment.is_empty()
            && !comment.starts_with('0')
            && comment.len() <= 16
            && comment.bytes().all(|b| b.is_ascii_digit())
        {
            args.extend(["--comment-id".into(), comment]);
        }
        if let Some(pr) = a.get("refresh_pr").and_then(Value::as_u64) {
            let head = a
                .pointer("/refresh_of/head_sha")
                .and_then(Value::as_str)
                .unwrap_or("");
            args.extend([
                "--refresh-pr".into(),
                pr.to_string(),
                "--refresh-head".into(),
                head.into(),
            ]);
        }
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let r = self.calls.store(&refs);
        let _ = std::fs::remove_file(&file);
        match r {
            Ok(_) => Ok(at),
            Err(f) => {
                let w = word(&f);
                let w = if w.is_empty() { "unusable".into() } else { w };
                Err((
                    w,
                    f.get("line")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                ))
            }
        }
    }

    /// A new name in $TEHAS_HOME/tmp, made 0700 when it is not there; None
    /// when the directory is a link or no directory.
    pub fn scratch(&self, what: &str) -> Option<PathBuf> {
        let dir = self.home.join("tmp");
        if !dir.exists() {
            let _ = std::fs::DirBuilder::new().mode(0o700).create(&dir);
        }
        let m = std::fs::symlink_metadata(&dir).ok()?;
        if !m.is_dir() {
            return None;
        }
        // As the bash keeps it: the tehas user's alone.
        if m.permissions().mode() & 0o777 != 0o700 {
            let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
        }
        let tag = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        Some(dir.join(format!("{what}.{}{tag:x}", std::process::id())))
    }

    /// The approval's inputs in a file of $TEHAS_HOME/tmp, readable by the
    /// tehas user alone.
    fn input_file(&self, body: &str) -> Option<PathBuf> {
        for i in 0..20 {
            let path = PathBuf::from(format!("{}{i}", self.scratch("approval-input")?.display()));
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
            {
                if f.write_all(body.as_bytes()).is_ok() {
                    return Some(path);
                }
                let _ = std::fs::remove_file(&path);
                return None;
            }
        }
        None
    }

    fn comments(&mut self, n: u64) -> Option<Value> {
        let path = format!("repos/{}/issues/{n}/comments?per_page=100", self.repo);
        self.calls.gh(&["api", &path, "--paginate"]).ok()
    }

    /// pr_comments: what an allowed account said on the bot's pull requests
    /// of the issue between the park and the tick, as {links, context}.
    fn pr_comments(&mut self, n: u64, since: &str, at: &str) -> Option<Value> {
        let repo = self.repo.clone();
        let bot = self.host.bot.clone();
        let prs = self
            .calls
            .gh(&[
                "pr",
                "list",
                "--repo",
                &repo,
                "--author",
                &bot,
                "--state",
                "all",
                "--json",
                "number,body,headRefName",
                "--limit",
                "100",
            ])
            .ok()?;
        let mut mine: Vec<u64> = prs
            .as_array()?
            .iter()
            .filter(|p| p.is_object() && names_issue(p, n))
            .filter_map(|p| p.get("number").and_then(Value::as_u64))
            .collect();
        mine.sort_unstable();
        let mut all: Vec<Value> = Vec::new();
        for pr in mine {
            let list = self.comments(pr)?;
            all.extend(list.as_array()?.iter().cloned());
        }
        if since.is_empty() {
            all.clear();
        }
        let allowed = self.host.allowed.clone();
        let who = |l: &str| allowed.iter().any(|x| x == l);
        let w = window(&Value::Array(all), since, at, &who)?;
        let ctx: Vec<Value> = w
            .iter()
            .take(20)
            .map(|c| context(c, "pr-comment"))
            .collect();
        Some(json!({"links": comment_links(&w, "pr-comment"), "context": ctx}))
    }

    /// fixer_note: the millwright's comment behind its tehas:ready, written
    /// in the ten minutes before the label and edited by nobody after it.
    fn fixer_note(&mut self, n: u64, at: &str) -> Option<Value> {
        let list = self.comments(n)?;
        let since = iso(u64::try_from(epoch(at)? - 600).ok()?);
        let marker = regex::Regex::new(&format!(
            "<!-- tehas:millwright issue={n} action=(retry|change) -->"
        ))
        .ok()?;
        let fixer = self.host.millwright.clone();
        let who = |l: &str| l == fixer;
        let w = window(&list, &since, at, &who)?;
        let last: Vec<Value> = w
            .iter()
            .rfind(|c| {
                c.get("body")
                    .and_then(Value::as_str)
                    .is_some_and(|b| marker.is_match(b))
            })
            .map(|c| context(c, "millwright"))
            .into_iter()
            .collect();
        Some(json!({"links": [], "context": last}))
    }

    /// park_unask: the question of the issue's park off the record. Whether
    /// it has a park, and the question taken; Err with the store's word.
    fn park_unask(
        &mut self,
        n: u64,
        log: &mut Vec<String>,
    ) -> Result<(bool, Option<String>), String> {
        let park = self
            .calls
            .store(&["park-show", &n.to_string()])
            .map_err(|f| word(&f))?;
        let Some(p) = park.get("park").filter(|p| !p.is_null()) else {
            return Ok((false, None));
        };
        let q = id(p.get("question"));
        if q.is_empty() {
            return Ok((true, None));
        }
        match self.calls.store(&[
            "question-answered",
            &n.to_string(),
            "--id",
            &q,
            "--withdraw",
        ]) {
            Ok(_) => Ok((true, Some(q))),
            Err(f) if word(&f) == "unknown" => Ok((true, None)),
            Err(f) => {
                // A withdraw that timed out may have been done all the same:
                // the question goes back.
                if word(&f) == "timeout" {
                    self.park_reask(n, Some(&q), log);
                }
                Err(word(&f))
            }
        }
    }

    /// park_reask: the question a start took off the record, put back.
    fn park_reask(&mut self, n: u64, q: Option<&str>, log: &mut Vec<String>) -> bool {
        let Some(q) = q else {
            return true;
        };
        match self.store_try(
            3,
            &[
                "question-asked",
                &n.to_string(),
                "--id",
                q,
                "--kind",
                "park",
            ],
        ) {
            Ok(_) => true,
            Err(f) => {
                log.push(format!(
                    "WARN: could not put the park question of #{n} back on the record ({}): its boxes start nothing now",
                    word(&f)
                ));
                false
            }
        }
    }

    /// restore_record: the approval this poll recorded, off the record again.
    fn restore(&mut self, n: u64, at: i64, log: &mut Vec<String>) -> bool {
        match self.store_try(
            3,
            &[
                "approval-record",
                &n.to_string(),
                "--undo",
                "--at",
                &at.to_string(),
            ],
        ) {
            Ok(_) => true,
            Err(f) => {
                log.push(format!(
                    "WARN: could not take the approval of #{n} off the record ({})",
                    word(&f)
                ));
                false
            }
        }
    }

    /// start_cycle: the unit started, and its start on record as an attempt
    /// later than `before`, within the wait.
    fn unit_start(&mut self, n: u64, before: u64, log: &mut Vec<String>) -> bool {
        // systemctl start waits for the unit. The bound is the poll's: a start
        // past it gave up, and its late unit finds no approval on record.
        let rc = match &self.host.start_cmd {
            Some(cmd) => run_quiet(cmd, &[n.to_string()], None, Duration::from_secs(300)),
            None => run_quiet(
                &self.host.systemctl,
                &[
                    "--user".into(),
                    "start".into(),
                    format!("tehas-cycle@{n}.service"),
                ],
                None,
                Duration::from_secs(300),
            ),
        };
        if rc != Some(0) {
            return false;
        }
        let began = Instant::now();
        while began.elapsed() < self.host.start_wait {
            match self.calls.store(&["cycle-show", &n.to_string()]) {
                Ok(v) => {
                    if v.pointer("/cycle/attempt")
                        .and_then(Value::as_u64)
                        .is_some_and(|a| a > before)
                    {
                        return true;
                    }
                }
                Err(f) if matches!(word(&f).as_str(), "busy" | "timeout") => {}
                Err(_) => break,
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        if held(&self.home.join(format!("state/cycles/{n}.lock"))) {
            log.push(format!(
                "WARN: the cycle for #{n} holds its lock but did not say so in {}s; taking it as started",
                self.host.start_wait.as_secs()
            ));
            return true;
        }
        false
    }

    /// stop_unit: the issue's unit stopped and waited for.
    fn unit_stop(&mut self, n: u64) -> bool {
        let rc = match &self.host.stop_cmd {
            Some(cmd) => run_quiet(cmd, &[n.to_string()], None, Duration::from_secs(30)),
            None => run_quiet(
                &self.host.systemctl,
                &[
                    "--user".into(),
                    "stop".into(),
                    format!("tehas-cycle@{n}.service"),
                ],
                None,
                Duration::from_secs(30),
            ),
        };
        rc == Some(0)
    }

    /// tehas_analytics: an event for the factory's analytics; a failure is
    /// a warning.
    pub fn analytics(&mut self, args: &[String], log: &mut Vec<String>) {
        let Some(bin) = self.host.analytics.clone() else {
            return;
        };
        let rc = run_quiet(&bin, args, None, self.host.analytics_wait);
        if rc != Some(0) {
            log.push(format!(
                "WARN analytics {}: exit {}",
                args.first().map_or("", String::as_str),
                rc.unwrap_or(124)
            ));
        }
    }
}
