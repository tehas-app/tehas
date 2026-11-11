//! The starts of a decision, as start_cycles in the bash dispatcher did them:
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
use crate::read::run::{gh_env, run_grace_env, run_quiet};
use crate::settings::account;
use serde_json::{json, Map, Value};
use std::io::Write as _;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The name the status reads to know the poll ran it, and the value that means
/// so. It is one of the names the load of the settings keeps.
const FROM_POLL: (&str, &str) = ("TEHAS_FROM_POLL", "1");

/// What a start reads off the host and the settings.
pub struct Host {
    /// hostname, for the comments.
    pub name: String,
    /// TEHAS_WORK_COMMENTS is 1: the approval names the comments' links. The
    /// text of the comments a comments retry is given does not depend on it.
    pub comments: bool,
    /// TEHAS_ALLOWED_LABELERS.
    pub allowed: Vec<String>,
    /// TEHAS_MILLWRIGHT_LOGIN, as `settings::account` has it.
    pub millwright: String,
    /// The granting labels this poll found not to be the ones pinned
    /// (`crate::pins`): no application of them counts.
    pub refused_labels: Vec<String>,
    /// The bot's login, whose pull requests a retry's comments are read on.
    pub bot: String,
    /// TEHAS_OWNER: the person a claimed issue is assigned to, or empty.
    pub owner: String,
    pub systemctl: String,
    /// TEHAS_CYCLE_START_CMD and TEHAS_CYCLE_STOP_CMD, which a test sets.
    pub start_cmd: Option<String>,
    pub stop_cmd: Option<String>,
    /// TEHAS_CYCLE_START_WAIT.
    pub start_wait: Duration,
    /// TEHAS_STATUS_CMD, run once the starts are done, or `tehasbot status`.
    pub status_cmd: String,
    pub status_args: Vec<String>,
    /// TEHAS_HEALTH_CMD, run after the poll wrote the demand.
    pub health_cmd: String,
    pub health_args: Vec<String>,
    pub console_cmd: String,
    /// Run by the end of every poll past the bot's login: the console's
    /// `tidy run`, or TEHAS_TIDY_CMD with no arguments.
    pub tidy_cmd: String,
    pub tidy_args: Vec<String>,
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
    /// tehas index: bin/tehas and its word, or TEHAS_INDEX_CMD alone, and
    /// TEHAS_INDEX_START_CMD, a test's.
    pub index_cmd: String,
    pub index_args: Vec<String>,
    pub index_start_cmd: Option<String>,
    /// TEHAS_FETCH_WAIT: the bound of the checkout's fetch.
    pub fetch_wait: Duration,
    /// TEHAS_PLAN_CMD, or `bin/tehas plan`: what a turn's unit runs.
    pub plan_cmd: Vec<String>,
    /// TEHAS_PLAN_START_CMD, which a test sets in place of systemd-run.
    pub plan_start_cmd: Option<String>,
    /// libexec/tehas-cycle, when the turn runs the install's `tehas plan`:
    /// the turn then runs from its own copy of the code (`tehas-cycle pin`).
    pub pin_cmd: Option<String>,
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
    /// TEHAS_READ_ONLY is 1: a start that worked leaves its record in
    /// state/read-only, the queue state the labels it did not write would
    /// have held (#608).
    pub read_only: bool,
}

/// The two time limits of a run of a host command, as `timeout -k` gives
/// them: how long it has to end, and how long it then has after TERM before
/// it is killed. The poll holds `state/dispatch.lock` while it runs a host
/// command, and the commands bound themselves only one call at a time -
/// `console tidy run` each gh by `TEHAS_TIDY_WAIT`, `tehasbot health` each call of
/// the store by `tehas_store_bound` - so the poll bounds the whole run. The
/// bound is the status's, and not a short one: a health file the poll cut off
/// at two seconds is a worse report than one written late, which is what the
/// bash did with a bound of nothing at all.
struct Term {
    bound: Duration,
    grace: Duration,
}

impl Term {
    /// A host command of the poll: the same bound the status is run under.
    const HOST: Term = Term {
        bound: Duration::from_secs(120),
        grace: Duration::from_secs(5),
    };
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

/// `issue_alone`: the tick on `Retry with the issue text only`, whose start
/// names no comment of the issue or of the pull request. Its absence is the
/// comments retry, which is what a record written before the box existed
/// holds. A value that is not a bool is no answer: the reader says so rather
/// than taking it for the comments retry that reads the issue's comments.
fn issue_alone(a: &Value, log: &mut Vec<String>, n: u64) -> bool {
    match a.get("issue_alone") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::Bool(true)) => true,
        Some(v) => {
            log.push(format!(
                "WARN: #{}: its start names issue_alone {v}, which is neither true nor false: the record is written as a comments retry's, and its comments are read",
                n
            ));
            false
        }
    }
}

/// The comment window of the bash dispatcher: of the comments as the REST API
/// lists them, those an allowed account wrote at or after `since` and at or
/// before `at`, and that nobody edited after `at`, in the order written.
/// None when the answer is not a list of comments.
pub(crate) fn window(
    list: &Value,
    since: &str,
    at: &str,
    who: &dyn Fn(&str) -> bool,
) -> Option<Vec<Value>> {
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
pub(crate) fn context(c: &Value, source: &str) -> Value {
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

/// The most text of comments a record holds, in bytes of their text: the
/// store takes an input of 1 MiB, and the blob a cycle is given holds 64 KB.
const RECORD_TEXT: usize = 512 * 1024;

/// One comment of a comments retry as the record keeps it, in the item shape
/// of the comments blob (trail-openers/pi-rukas#2020): `source`, `id`, `by`,
/// `at`, `url` and `text`, plus `pr`, `path` and `line` where they apply.
/// The text is cut at 4000 characters, without carriage returns.
pub(crate) fn item(c: &Value, source: &str, pr: Option<u64>) -> Value {
    let text: String = c
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or("")
        .replace('\r', "")
        .chars()
        .take(4000)
        .collect();
    let mut m = Map::new();
    m.insert("source".into(), json!(source));
    m.insert("id".into(), c.get("id").cloned().unwrap_or(Value::Null));
    if let Some(pr) = pr {
        m.insert("pr".into(), json!(pr));
    }
    if source == "review-comment" {
        if let Some(path) = c.get("path").filter(|p| p.is_string()) {
            m.insert("path".into(), path.clone());
        }
        if let Some(line) = c
            .get("line")
            .filter(|l| l.is_u64())
            .or_else(|| c.get("original_line").filter(|l| l.is_u64()))
        {
            m.insert("line".into(), line.clone());
        }
    }
    m.insert(
        "by".into(),
        c.pointer("/user/login").cloned().unwrap_or(Value::Null),
    );
    m.insert(
        "at".into(),
        c.get("created_at").cloned().unwrap_or(Value::Null),
    );
    m.insert(
        "url".into(),
        c.get("html_url").cloned().unwrap_or(json!("")),
    );
    m.insert("text".into(), json!(text));
    Value::Object(m)
}

/// A review in the shape `window` reads: GitHub stamps it once, when it was
/// submitted, and keeps no time of an edit. A review with no body says
/// nothing to give, and one still pending was never submitted.
fn review_as_comment(r: &Value) -> Option<Value> {
    let at = r.get("submitted_at").and_then(Value::as_str)?;
    let body = r.get("body").and_then(Value::as_str)?;
    if body.trim().is_empty() {
        return None;
    }
    let mut c = r.clone();
    c["created_at"] = json!(at);
    c["updated_at"] = json!(at);
    Some(c)
}

/// Whether a permission answer of `collaborators/{login}/permission` lets the
/// account write: write, maintain or admin. `role_name` names maintain, which
/// `permission` folds into write.
fn may_write(answer: &Value) -> Option<bool> {
    let role = answer
        .get("role_name")
        .and_then(Value::as_str)
        .or_else(|| answer.get("permission").and_then(Value::as_str))?;
    Some(matches!(role, "admin" | "maintain" | "write"))
}

/// The newest items whose text fits `RECORD_TEXT`, oldest first, and how many
/// older ones were left out.
fn bounded(items: Vec<Value>) -> (Vec<Value>, usize) {
    let mut used = 0;
    let mut keep = 0;
    for i in items.iter().rev() {
        let len = i.get("text").and_then(Value::as_str).map_or(0, str::len);
        if used + len > RECORD_TEXT {
            break;
        }
        used += len;
        keep += 1;
    }
    let dropped = items.len() - keep;
    (items.into_iter().skip(dropped).collect(), dropped)
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

/// The comment that lifts a park's hold off the millwright's `tehas:ready` at
/// `at`: the note `fixer_note` would put on the record, when it carries the
/// `action=change` marker within the text a cycle is given and no other
/// account saved it. `action=retry` asks for the same run again, so a newer
/// one of those takes the lift away (#1130).
///
/// <list> is the poll's GraphQL read of the issue's comments, put here in the
/// REST shape `window` and `context` read. None when that read or `at` is no
/// answer, so a failed read is never taken for no note.
pub(crate) fn change_note(list: &Value, n: u64, fixer: &str, at: &str) -> Option<Option<Value>> {
    let nodes = list
        .pointer("/data/repository/issue/comments/nodes")
        .and_then(Value::as_array)
        .or_else(|| list.as_array())?;
    let since = iso(u64::try_from(epoch(at)? - 600).ok()?);
    let marker = regex::Regex::new(&format!(
        "<!-- tehas:millwright issue={n} action=(retry|change) -->"
    ))
    .ok()?;
    let rest: Vec<Value> = nodes
        .iter()
        .filter_map(|x| {
            let id = x
                .get("databaseId")
                .or_else(|| x.get("id"))
                .cloned()
                .unwrap_or(Value::Null);
            let login = x
                .pointer("/author/login")
                .or_else(|| x.pointer("/user/login"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let (body, created) = (
                x.get("body").and_then(Value::as_str)?,
                x.get("createdAt")
                    .or_else(|| x.get("created_at"))
                    .and_then(Value::as_str)?,
            );
            // GitHub answers a comment nobody saved after it with no stamp at
            // all, and REST with a stamp equal to the first: either is the
            // comment as its author wrote it.
            let edited = x
                .get("lastEditedAt")
                .or_else(|| x.get("updated_at"))
                .cloned()
                .unwrap_or(Value::Null);
            let editor = x
                .pointer("/editor/login")
                .and_then(Value::as_str)
                .unwrap_or(login);
            let mut c = json!({"id": id, "body": body, "user": {"login": login},
                "created_at": created});
            c["updated_at"] = if edited.is_null() {
                json!(created)
            } else {
                edited
            };
            c["outsider"] = json!(account(editor) != account(login));
            Some(c)
        })
        .collect();
    let who = |l: &str| !fixer.is_empty() && account(l) == fixer;
    let Some(newest) = window(&Value::Array(rest), &since, at, &who)?
        .into_iter()
        .rfind(|c| marker.is_match(c.get("body").and_then(Value::as_str).unwrap_or("")))
    else {
        return Some(None);
    };
    let note = context(&newest, "millwright");
    let change = format!("<!-- tehas:millwright issue={n} action=change -->");
    let lifts = newest["outsider"] == json!(false)
        && note["text"].as_str().is_some_and(|t| t.contains(&change));
    Some(lifts.then_some(note))
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
                Then::Started => self.read_only_record(a, log),
                Then::Skipped => {}
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

    /// A read-only host keeps the start that worked on record, so the next
    /// poll does not start the issue again: its `tehas:ready` is still on it.
    fn read_only_record(&self, a: &Value, log: &mut Vec<String>) {
        if !self.host.read_only {
            return;
        }
        let n = a.get("start").and_then(Value::as_u64).unwrap_or(0);
        let how = if a.get("refresh_pr").is_some() {
            "refresh"
        } else {
            "cycle"
        };
        if tehas_core::read_only::record(&self.home, n, how, &crate::decide::iso_now()).is_err() {
            log.push(format!(
                "WARN: read-only: the record of the start of #{n} could not be written"
            ));
        }
    }

    /// The status command, once the poll's work is done; what it printed.
    /// The token, when the poll holds one, goes to it in its environment
    /// alone: never an argument, never written, and the poll's log holds only
    /// what the command itself prints. It also gets `TEHAS_FROM_POLL=1`, the
    /// one name of its own: the status reads it and leaves
    /// `health.json` to the poll, which runs health a moment after this ends.
    /// The token cannot carry that word: a host with a machine-user token, or
    /// a poll whose token read failed, holds none, and the status would then
    /// run health and so would the poll.
    pub fn status(&mut self, printed: &mut Vec<String>) {
        let cmd = self.host.status_cmd.clone();
        let args = self.host.status_args.clone();
        self.status_of_grace(
            &cmd,
            &args,
            printed,
            &mut Vec::new(),
            &Term::HOST,
            &[FROM_POLL],
        );
    }

    /// The health command, run by the poll after it wrote the demand, under
    /// the same bound as the status's: the poll holds its lock while health
    /// reads the store. What it printed goes in `printed` and the poll's log,
    /// so a run that could not be given a file to write says so there too. It
    /// is not the status, so it gets no `TEHAS_FROM_POLL`: the name is the
    /// status's alone. Its exit status, None when it did not end in its bound.
    pub fn health(&mut self, printed: &mut Vec<String>, log: &mut Vec<String>) -> Option<i32> {
        let cmd = self.host.health_cmd.clone();
        let args = self.host.health_args.clone();
        self.status_of_grace(&cmd, &args, printed, log, &Term::HOST, &[])
    }

    /// The host's command `cmd` run under a bound, with the grace between
    /// TERM and KILL its caller gives, and its output in a file the poll's log
    /// then prints - as status's, tidy's and health's. The poll's token goes
    /// with it as `TEHAS_GH_TOKEN`, the one name the poll adds: the grant is
    /// the poll's own environment plus that name, since `Command::env` does not
    /// clear what the poll holds. The token is never an argument of the command
    /// and the poll never writes it anywhere; what the command prints is its
    /// stdout, which the poll's log prints as the bash's did, so a host command
    /// the poll runs must print no secret of its own.
    ///
    /// Its exit status, None when it did not end in its bound.
    pub fn status_of(
        &mut self,
        cmd: &str,
        args: &[String],
        printed: &mut Vec<String>,
    ) -> Option<i32> {
        self.status_of_grace(cmd, args, printed, &mut Vec::new(), &Term::HOST, &[])
    }

    /// status_of under the bound and the TERM-to-KILL grace of `term`: the
    /// pair a `timeout -k` of those two durations would give, plus `extra`
    /// names beyond the token the poll gives every host command it runs.
    fn status_of_grace(
        &mut self,
        cmd: &str,
        args: &[String],
        printed: &mut Vec<String>,
        log: &mut Vec<String>,
        term: &Term,
        extra: &[(&str, &str)],
    ) -> Option<i32> {
        // Its output through a file: a pipe would wait for whatever it left
        // running. With no file to write it still runs, and the line says so:
        // the caller cannot tell a command that printed nothing from one whose
        // output the poll could not keep.
        let mut token = gh_env(self.calls.gh_token.as_deref());
        token.extend_from_slice(extra);
        let out = self
            .scratch("status")
            .and_then(|p| std::fs::File::create(&p).ok().map(|f| (p, f)));
        let Some((path, f)) = out else {
            log.push(format!("WARN: no file to write in for {cmd}"));
            return run_grace_env(cmd, args, None, term.bound, term.grace, &token);
        };
        let rc = run_grace_env(cmd, args, Some(f), term.bound, term.grace, &token);
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
        // #1031: an easy start is a refresh the poll chose to run as an easy
        // merge. Until part 3 the container still runs the full refresh, so
        // what differs is this line and the mark on the approval record.
        let easy = refresh_pr.is_some() && a.get("easy") == Some(&json!(true));
        if easy {
            log.push(format!(
                "start #{n}: easy merge of PR #{pr}",
                pr = refresh_pr.unwrap_or(0)
            ));
        }
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
        let mut left_out = Vec::new();
        let recorded = match self.approval(n, a, &mut left_out, log) {
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
        // #806: the owner is assigned in the same call, so GitHub shows the
        // issue as taken. A refresh's issue was assigned when it was first
        // claimed. An assign GitHub refuses fails the whole edit, so the mark
        // is made again without it: the claim never fails on the assign.
        let owner = self.host.owner.clone();
        let assign = !owner.is_empty() && refresh_pr.is_none();
        let mut wrote = if assign {
            let mut with = args.clone();
            with.extend(["--add-assignee".into(), owner.clone()]);
            self.write(n, "label-add", with, None, None)
        } else {
            self.write(n, "label-add", args.clone(), None, None)
        };
        if assign && matches!(wrote, Wrote::Failed(_)) {
            log.push(format!(
                "WARN: could not assign #{n} to {owner}, marking it running without an assignee"
            ));
            wrote = self.write(n, "label-add", args, None, None);
        }
        match wrote {
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
        // A record starts one cycle, and any start of the issue spends it.
        self.plan_start_drop(n);
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
        let branch = a
            .pointer("/refresh_of/branch")
            .and_then(Value::as_str)
            .unwrap_or("");
        // The line of an easy start: the issue is marked running and its unit
        // is up, and the comment says what the cycle is for. Until part 3 the
        // container still runs the full refresh, so the comment says that too.
        if easy {
            let url = text(a, "refresh_url");
            self.say(n, &format!("Tehas chose the easy merge of this issue on `{host}` at {when} UTC: it merges `{branch}` into {url} with no model and no container. This cycle is the full refresh kept in reserve for what the easy merge cannot do. It rests on @{by}'s {how}. This issue is owned by the cycle until `tehas:running` is removed."), log);
        } else if refresh_pr.is_some() {
            let _ = std::fs::remove_file(self.home.join(format!("state/refresh/{n}.json")));
            let url = text(a, "refresh_url");
            self.say(n, &format!("Tehas refresh started on `{host}` at {when} UTC: {url} conflicts with `{branch}`. This cycle merges that branch into it, resolves the conflicts and pushes to the same pull request. It rests on @{by}'s {how}. This issue is owned by the cycle until `tehas:running` is removed."), log);
        } else {
            let left = if left_out.is_empty() {
                String::new()
            } else {
                let names: Vec<String> = left_out.iter().map(|l| format!("`{l}`")).collect();
                format!(" The comments of {} are left out: their permission on the repository could not be read.", names.join(", "))
            };
            self.say(n, &format!("Tehas cycle started on `{host}` at {when} UTC (authorized by @{by}'s {how}). This issue is owned by the cycle until `tehas:running` is removed.{left}"), log);
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
    /// `left_out` gets the authors of a comments retry whose permission on
    /// the repository could not be read, so the start comment can name them.
    fn approval(
        &mut self,
        n: u64,
        a: &Value,
        left_out: &mut Vec<String>,
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
        if let Some(d) = a.get("driver") {
            input.insert("driver".into(), d.clone());
        }
        let mut retry = Value::Null;
        let mut from: Vec<Value> = Vec::new();
        let alone = issue_alone(a, log, n);
        // The comment policy of the links, derived once: the host fetches what
        // comments link, and the tick of the issue text only asks for none of
        // them, so its record reads as the one of an instance with comments off.
        let comments = self.host.comments && !alone;
        let mut list = None;
        if comments {
            let Some(l) = self.comments(n) else {
                log.push(format!("ERROR: the comments of #{n} could not be read"));
                return Err(none());
            };
            let allowed = self.host.allowed.clone();
            let who = |l: &str| allowed.iter().any(|x| x == l);
            let Some(w) = window(&l, "", &given_at, &who) else {
                log.push(format!("ERROR: the comments of #{n} could not be read"));
                return Err(none());
            };
            from = comment_links(&w, "comment");
            list = Some(l);
        }
        // The tick of the comments retry is its own opt-in: what allowed
        // accounts wrote up to it goes on the record whatever
        // TEHAS_WORK_COMMENTS says.
        if given == "retry-tick" && !alone {
            let list = match list.take() {
                Some(l) => l,
                None => match self.comments(n) {
                    Some(l) => l,
                    None => {
                        log.push(format!("ERROR: the comments of #{n} could not be read"));
                        return Err(none());
                    }
                },
            };
            let Some(r) = self.retry_comments(n, &list, &given_at, log) else {
                log.push(format!(
                    "ERROR: the comments on the pull request of #{n} could not be read"
                ));
                return Err(none());
            };
            *left_out = r["left_out"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            retry = r;
        }
        // A start of the issue text only names no comment of an issue or a
        // pull request, so nothing of the earlier cycle goes with it: the note
        // of the fixer or of a refresh stays out of its record whatever asks.
        if !alone {
            if let Some(note) = a.get("park_lifted").filter(|_| given == "millwright") {
                retry = json!({"links": [], "context": [note]});
            } else if given == "millwright" {
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
                    "text": format!("This pull request conflicts with `{b}`. Merge `{b}` into its branch, resolve the conflicts so the change still does what the issue asks, run the gate in `.pi/verify-cmd`, and push to the same branch. Do no other work.")}]});
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
        // Why a park's hold let this start through. `rests_on` keeps naming
        // the person's approval (#1130).
        if let Some(note) = a.get("park_lifted") {
            input.insert("park_lifted".into(), note.clone());
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
            // The store keeps one form of an App's login.
            account(text(a, "by")).into(),
            "--how".into(),
            given.clone(),
            "--body-sha".into(),
            text(a, "body_sha256").into(),
            "--input-file".into(),
            f,
        ];
        if matches!(given.as_str(), "label" | "millwright") {
            let label = a
                .get("approved_label")
                .and_then(Value::as_str)
                .unwrap_or("tehas:ready");
            args.extend(["--label".into(), label.into()]);
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
            // The mark of the easy merge, so the store can answer next time
            // whether this approval has had the one it is owed (#1031).
            if a.get("easy") == Some(&json!(true)) {
                args.push("--easy".into());
            }
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

    /// What a comments retry is given, as {links, context, left_out}: the
    /// text of every comment on the issue (`list`) and on the bot's pull
    /// requests for it - conversation comments, review bodies and inline
    /// review comments - written at or before the tick `at` and not edited
    /// after it, by an account of TEHAS_ALLOWED_LABELERS that may write to the
    /// repository at the tick. The bot's own comments are left out. `links`
    /// are those of the pull requests' conversation comments; `left_out` the
    /// authors whose permission could not be read, said in the log here. None
    /// when a list could not be read.
    fn retry_comments(
        &mut self,
        n: u64,
        list: &Value,
        at: &str,
        log: &mut Vec<String>,
    ) -> Option<Value> {
        let repo = self.repo.clone();
        let bot = self.host.bot.clone();
        let allowed = self.host.allowed.clone();
        let who =
            |l: &str| !tehas_core::accounts::factorys(l, &bot) && allowed.iter().any(|x| x == l);
        let mut items: Vec<Value> = Vec::new();
        for c in window(list, "", at, &who)? {
            items.push(item(&c, "issue-comment", None));
        }
        let authors = [bot.clone()];
        let mut mine: Vec<u64> = Vec::new();
        for author in &authors {
            let prs = self
                .calls
                .gh(&[
                    "pr",
                    "list",
                    "--repo",
                    &repo,
                    "--author",
                    author,
                    "--state",
                    "all",
                    "--json",
                    "number,body,headRefName",
                    "--limit",
                    "100",
                ])
                .ok()?;
            mine.extend(
                prs.as_array()?
                    .iter()
                    .filter(|p| p.is_object() && names_issue(p, n))
                    .filter_map(|p| p.get("number").and_then(Value::as_u64)),
            );
        }
        mine.sort_unstable();
        mine.dedup();
        let mut said = Vec::new();
        for pr in mine {
            let talk = window(&self.comments(pr)?, "", at, &who)?;
            said.extend(talk.iter().cloned());
            for c in talk {
                items.push(item(&c, "pr-comment", Some(pr)));
            }
            let path = format!("repos/{repo}/pulls/{pr}/reviews?per_page=100");
            let reviews = self.calls.gh(&["api", &path, "--paginate"]).ok()?;
            let reviews: Vec<Value> = reviews
                .as_array()?
                .iter()
                .filter_map(review_as_comment)
                .collect();
            for c in window(&Value::Array(reviews), "", at, &who)? {
                items.push(item(&c, "review", Some(pr)));
            }
            let path = format!("repos/{repo}/pulls/{pr}/comments?per_page=100");
            let inline = self.calls.gh(&["api", &path, "--paginate"]).ok()?;
            for c in window(&inline, "", at, &who)? {
                items.push(item(&c, "review-comment", Some(pr)));
            }
        }
        // Who may write, asked once per author of this tick.
        let mut authors: Vec<String> = items
            .iter()
            .filter_map(|i| i["by"].as_str().map(str::to_string))
            .collect();
        authors.sort();
        authors.dedup();
        let mut keep = Vec::new();
        let mut left_out = Vec::new();
        for login in authors {
            let path = format!("repos/{repo}/collaborators/{login}/permission");
            match self
                .calls
                .gh(&["api", &path])
                .ok()
                .as_ref()
                .and_then(may_write)
            {
                Some(true) => keep.push(login),
                Some(false) => {}
                None => {
                    log.push(format!("WARN: #{n}: the permission of {login} on {repo} could not be read, so its comments are left out of the retry"));
                    left_out.push(login);
                }
            }
        }
        let key = |i: &Value| {
            (
                i["at"].as_str().unwrap_or("").to_string(),
                i["id"].as_f64().unwrap_or(0.0),
            )
        };
        let mut given: Vec<Value> = items
            .into_iter()
            .filter(|i| {
                i["by"]
                    .as_str()
                    .is_some_and(|b| keep.iter().any(|k| k == b))
            })
            .collect();
        given.sort_by(|x, y| {
            let (a, b) = (key(x), key(y));
            a.0.cmp(&b.0).then(a.1.total_cmp(&b.1))
        });
        let (given, dropped) = bounded(given);
        if dropped > 0 {
            log.push(format!("WARN: #{n}: {dropped} older comments are left out of the retry's record, which holds {RECORD_TEXT} bytes of their text"));
        }
        let said: Vec<Value> = said
            .into_iter()
            .filter(|c| {
                c.pointer("/user/login")
                    .and_then(Value::as_str)
                    .is_some_and(|b| keep.iter().any(|k| k == b))
            })
            .collect();
        Some(
            json!({"links": comment_links(&said, "pr-comment"), "context": given, "left_out": left_out}),
        )
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
        let who = |l: &str| !fixer.is_empty() && account(l) == fixer;
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

#[cfg(test)]
mod tests {
    use super::*;

    const AT: &str = "2026-11-10T10:00:00Z";

    /// The poll's GraphQL read of one comment.
    fn comment(by: &str, body: &str) -> Value {
        json!({"databaseId": 5, "author": {"login": by}, "body": body,
            "createdAt": "2026-11-10T09:59:00Z", "lastEditedAt": null})
    }

    fn list(by: &str, body: &str) -> Value {
        json!({"data": {"repository": {"issue": {"comments": {"nodes": [comment(by, body)]}}}}})
    }

    const CHANGE: &str = "<!-- tehas:millwright issue=5 action=change -->\nTry the other branch.";

    /// The millwright is the factory's own app account, so a marker the bot
    /// wrote lifts a hold, and one another account wrote does not.
    #[test]
    fn a_bot_marker_lifts_a_hold() {
        let lifted = change_note(&list("acme-factory", CHANGE), 5, "acme-factory", AT);
        let note = lifted.expect("the read is an answer").expect("a note");
        assert!(note["text"].as_str().unwrap().contains("action=change"));
        // REST names the same app `<slug>[bot]`: one account.
        let rest = json!([{"id": 5, "user": {"login": "acme-factory[bot]"}, "body": CHANGE,
            "created_at": "2026-11-10T09:59:00Z", "updated_at": "2026-11-10T09:59:00Z"}]);
        assert!(change_note(&rest, 5, "acme-factory", AT).unwrap().is_some());
        // Another account's marker lifts nothing, and a retry marker is no change.
        let other = change_note(&list("mallory", CHANGE), 5, "acme-factory", AT);
        assert_eq!(other, Some(None));
        let retry = CHANGE.replace("action=change", "action=retry");
        let retried = change_note(&list("acme-factory", &retry), 5, "acme-factory", AT);
        assert_eq!(retried, Some(None));
    }
}
