//! The start of a planning turn, as lib/planning.sh's plan_turn and
//! plan_launch do it, and the three spends of a park's retry rule 4a and 5a
//! make. A first turn's brief is written before the session begins on
//! record; the issue is marked planning before the turn's unit starts.
//!
//! What a poll does before a turn (the checkout's refresh) is the poll's,
//! part 4 of #254.

use super::{text, word, Writer, Wrote};
use crate::read::host::held;
use crate::read::run::run_quiet;
use serde_json::Value;
use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The script the turn's unit runs, plan_launch's: the turn's lock held
/// for its length, its exit status kept, and a poll asked for at its end.
const TURN: &str = r#"log=$1 rc=$2 again=$3 lock=$4; shift 4
    exec 9>>"$lock" && flock -n 9 || { echo "a planning turn of this issue is already running" >>"$log"; exit 75; }
    "$@" 9>&- >>"$log" 2>&1; echo "$?" >"$rc.new" && mv "$rc.new" "$rc"; : >"$again""#;

/// How a turn's start ended: the poll goes on, or ends with a status.
pub(super) enum Then {
    Go,
    End(i32),
}

impl Writer {
    /// A new file of $TEHAS_HOME/tmp named for `what`, readable by the tehas
    /// user alone, as mktemp makes it.
    fn private_file(&self, what: &str, body: &str) -> Option<PathBuf> {
        for i in 0..20 {
            let path = PathBuf::from(format!("{}{i}", self.scratch(what)?.display()));
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

    /// The park_retry action: the outage's retry and a skipped refine are
    /// spent now; a refine about to start is spent once its brief is
    /// written, by the turn that follows.
    /// False for a spend of no kind it knows, which is left.
    pub(super) fn park_retry(&mut self, n: u64, a: &Value, log: &mut Vec<String>) -> bool {
        let what = match text(a, "how") {
            "refine" => {
                self.spend = Some(n);
                return true;
            }
            "outage" => "the retry of #N after a provider outage",
            "skipped" => "the refine of #N",
            _ => return false,
        };
        let tries = if text(a, "how") == "outage" { 1 } else { 3 };
        if let Err(f) = self.store_try(tries, &["park-retry", &n.to_string()]) {
            if word(&f) != "unparked" {
                log.push(format!(
                    "WARN: {} could not be spent on record ({})",
                    what.replace("#N", &format!("#{n}")),
                    word(&f)
                ));
            }
        }
        true
    }

    /// plan_refused: a turn the store would not begin. A refusal is the
    /// issue's to wait out; busy ends the poll for the next to try again; a
    /// store that cannot be used ends it with 78.
    fn plan_refused(n: u64, what: &str, f: &Value, log: &mut Vec<String>) -> Then {
        if f.get("rc") == Some(&serde_json::json!(1)) {
            let w = word(f);
            if matches!(w.as_str(), "busy" | "timeout") {
                log.push(format!(
                    "ERROR: could not write the record of #{n}, not starting a turn ({w})"
                ));
                return Then::End(1);
            }
            log.push(format!("skip #{n}: {what}: {}", text(f, "line")));
            return Then::Go;
        }
        log.push(format!(
            "ERROR: the state store cannot be used: {}",
            text(f, "line")
        ));
        Then::End(78)
    }

    /// plan_keep_make: the keep directory, in a directory of workspaces
    /// tehas made, marked before anything is in it.
    fn keep_make(&self, keep: &Path, log: &mut Vec<String>) -> bool {
        let Some(works) = keep.parent() else {
            return false;
        };
        if works.exists() && !works.join(".tehas-workspaces").is_file() {
            log.push(format!(
                "ERROR: {} exists and was not made by tehas, not touching it",
                works.display()
            ));
            return false;
        }
        std::fs::create_dir_all(works).is_ok()
            && std::fs::write(works.join(".tehas-workspaces"), "").is_ok()
            && std::fs::create_dir_all(keep).is_ok()
    }

    /// The plan_turn action, first or next; the status command after a turn
    /// that was started or waits.
    pub(super) fn plan_turn(&mut self, n: u64, a: &Value, log: &mut Vec<String>) -> Then {
        let first = text(a, "kind") == "first";
        let label = match text(a, "label") {
            "" => "tehas:plan".to_string(),
            l => l.to_string(),
        };
        let by = text(a, "by").to_string();
        let keep = self.keep(n);
        let out = self.st(n, ".out");
        let mut turns = 0;
        let mut sid = String::new();
        let undo: Vec<String>;
        let input: PathBuf;
        if first {
            // plan_brief: the approved text, kept for the turn's unit.
            let mut brief = format!("# {}\n\n{}\n", text(a, "title"), text(a, "body"));
            if let Some(extra) = a.get("brief_extra").and_then(Value::as_str) {
                let more = std::fs::read_to_string(extra).unwrap_or_default();
                brief += &format!("\n\n---\n\n{}\n\nThis request is issue #{n} itself. The issue you file replaces #{n}: #{n} is not a duplicate of this plan.\n", more.trim_end_matches('\n'));
            }
            let Some(file) = self.private_file(&format!("plan-{n}.brief"), &brief) else {
                log.push(format!("ERROR: could not write the brief of #{n}"));
                return Then::End(1);
            };
            if self.spend == Some(n) {
                self.spend = None;
                if let Err(f) = self.store_try(3, &["park-retry", &n.to_string()]) {
                    let _ = std::fs::remove_file(&file);
                    log.push(format!(
                        "ERROR: the refine of #{n} could not be spent on record ({}), not refining it",
                        word(&f)
                    ));
                    return Then::End(1);
                }
            }
            if let Some(line) = a.get("log").and_then(Value::as_str) {
                log.push(line.to_string());
            }
            let mut args: Vec<String> = vec![
                "plan-begin".into(),
                n.to_string(),
                "--by".into(),
                by.clone(),
                "--label".into(),
                label.clone(),
                "--brief-file".into(),
                file.to_string_lossy().into_owned(),
            ];
            if a.get("auto")
                .and_then(Value::as_str)
                .is_some_and(|x| !x.is_empty())
            {
                args.push("--auto".into());
            }
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            if let Err(f) = self.calls.store(&refs) {
                let _ = std::fs::remove_file(&file);
                return self.go(Self::plan_refused(
                    n,
                    "its planning session could not begin",
                    &f,
                    log,
                ));
            }
            let _ = std::fs::remove_file(&out);
            let _ = std::fs::remove_file(self.st(n, ".rc"));
            let _ = std::fs::remove_dir_all(&keep);
            undo = vec!["plan-begin".into(), n.to_string(), "--undo".into()];
            input = file;
        } else {
            // The answer goes to the store as it is; the turn is given it from
            // there.
            let Some(file) = self.private_file("plan-answer", &format!("{}\n", text(a, "answer")))
            else {
                log.push(format!(
                    "ERROR: could not write the answer for #{n}, not starting a turn"
                ));
                return Then::End(1);
            };
            if let Some(line) = a.get("log").and_then(Value::as_str) {
                log.push(line.to_string());
            }
            let show = match self.calls.store(&["plan-show", &n.to_string()]) {
                Ok(v) => v,
                Err(f) => {
                    let _ = std::fs::remove_file(&file);
                    return self.go(Self::plan_refused(
                        n,
                        "its planning record could not be read",
                        &f,
                        log,
                    ));
                }
            };
            turns = show
                .pointer("/planning/turns")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            sid = show
                .pointer("/planning/session")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if turns >= self.host.plan_max_turns {
                let _ = std::fs::remove_file(&file);
                let hand = show.pointer("/planning/auto") == Some(&serde_json::json!(true));
                let max = self.host.plan_max_turns;
                self.plan_end(n, &format!("This planning session reached its limit of {max} turns and was ended. Apply `tehas:plan` to start a new one."), false, hand, log);
                return self.go(Then::Go);
            }
            let next = (turns + 1).to_string();
            let f = file.to_string_lossy().into_owned();
            let mut args = vec![
                "plan-answer".to_string(),
                n.to_string(),
                "--turn".into(),
                next.clone(),
                "--by".into(),
                by.clone(),
                "--text-file".into(),
                f,
            ];
            if !self.host.millwright.is_empty() && by == self.host.millwright {
                args.push("--millwright".into());
            }
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            let answered = self.calls.store(&refs);
            let _ = std::fs::remove_file(&file);
            let answered = match answered {
                Ok(v) => v,
                Err(f) => {
                    return self.go(Self::plan_refused(
                        n,
                        "the answer could not begin a turn",
                        &f,
                        log,
                    ));
                }
            };
            undo = vec![
                "plan-answer".into(),
                n.to_string(),
                "--turn".into(),
                next,
                "--undo".into(),
            ];
            // The answer the turn is given is the one on record, with what the
            // session filed above it.
            let mut filed: Vec<&str> = answered
                .pointer("/planning/filed")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            filed.sort_unstable();
            filed.dedup();
            let said = answered
                .pointer("/planning/answer")
                .and_then(Value::as_str)
                .unwrap_or("")
                // As $(tehas_store_get ...) reads it, without its line ends.
                .trim_end_matches('\n');
            let body = if filed.is_empty() {
                format!("{said}\n")
            } else {
                format!(
                    "Already filed in this session: {}\n\n{said}\n",
                    filed.join(", ")
                )
            };
            let answer = keep.join("answer.md");
            if !(self.keep_make(&keep, log) && std::fs::write(&answer, body).is_ok()) {
                self.put_back(n, &undo, log);
                log.push(format!(
                    "ERROR: could not write the answer for #{n}, not starting a turn"
                ));
                return Then::End(1);
            }
            input = answer;
        }
        // The turn runs only once the issue is marked as taken.
        let edit = vec![
            "issue".into(),
            "edit".into(),
            n.to_string(),
            "--repo".into(),
            self.repo.clone(),
            "--add-label".into(),
            "tehas:planning".into(),
            "--remove-label".into(),
            label.clone(),
            "--remove-label".into(),
            "tehas:question".into(),
            "--remove-label".into(),
            "needs-human-attention".into(),
        ];
        if !matches!(self.write(n, "label-add", edit, None, None), Wrote::Done(_)) {
            self.put_back(n, &undo, log);
            if first {
                let _ = std::fs::remove_file(&input);
            }
            log.push(format!(
                "ERROR: could not mark #{n} as planning, not starting a turn"
            ));
            return Then::End(1);
        }
        let host = self.host.name.clone();
        let when = crate::decide::iso(now()).replacen('T', " ", 1)[..16].to_string();
        let path = input.to_string_lossy().into_owned();
        let mut args: Vec<String>;
        if first {
            if let Some(auto) = a.get("auto").and_then(Value::as_str).filter(|x| !x.is_empty()) {
                self.say(n, &format!("Tehas planning session started on `{host}` at {when} UTC to refine this issue: its cycle parked as `{auto}`, and the start of that cycle by @{by} authorizes one refine. Each turn ends in a comment here. Answer by ticking a box or by replying. To stop it, take `tehas:question` off while a question waits, or close the issue."), log);
            } else {
                self.say(n, &format!("Tehas planning session started on `{host}` at {when} UTC (authorized by @{by}'s `{label}` label). Each turn ends in a comment here. Answer by ticking a box or by replying."), log);
            }
            args = vec!["--brief".into(), path];
        } else {
            args = vec!["--session".into(), sid.clone(), "--answer".into(), path];
            if let Some(c) = a.get("reply").filter(|c| !c.is_null()) {
                let c = match c {
                    Value::String(x) => x.clone(),
                    v => v.to_string(),
                };
                if !c.is_empty() {
                    args.extend(["--comment".into(), c]);
                }
            }
        }
        self.refresh_checkout(log);
        let _ = std::fs::remove_file(&out);
        let _ = std::fs::remove_file(self.st(n, ".rc"));
        let mut cmd = vec![
            self.host.plan_cmd.clone(),
            n.to_string(),
            "--out".into(),
            out.to_string_lossy().into_owned(),
            "--keep".into(),
            keep.to_string_lossy().into_owned(),
        ];
        cmd.append(&mut args);
        if !self.plan_launch(n, &cmd) {
            log.push(format!("ERROR: could not start the planning turn of #{n}"));
            self.plan_finish(n, "not started", turns, &sid, log);
            return self.go(Then::Go);
        }
        if self.st(n, ".rc").is_file() {
            self.plan_collect(n, log);
        } else {
            log.push(format!(
                "planning #{n}: turn {} runs as tehas-plan-{n}.service",
                turns + 1
            ));
        }
        self.go(Then::Go)
    }

    /// A turn that did not end the poll: the status command runs after it.
    fn go(&mut self, then: Then) -> Then {
        if let Then::Go = then {
            self.status_due = true;
        }
        then
    }

    fn put_back(&mut self, n: u64, undo: &[String], log: &mut Vec<String>) {
        let refs: Vec<&str> = undo.iter().map(String::as_str).collect();
        if let Err(f) = self.calls.store(&refs) {
            log.push(format!(
                "WARN: could not put back the record of #{n} ({})",
                word(&f)
            ));
        }
    }

    /// plan_launch: the turn as its own unit, which outlives the poll. A
    /// moment for the unit to take its lock, so the status after shows it.
    fn plan_launch(&mut self, n: u64, cmd: &[String]) -> bool {
        let st = self.st(n, "");
        let mut turn: Vec<String> = vec![
            "/bin/bash".into(),
            "-c".into(),
            TURN.into(),
            "plan-turn".into(),
            self.home
                .join(format!("logs/dispatch-plan-{n}.log"))
                .to_string_lossy()
                .into_owned(),
            format!("{}.rc", st.display()),
            self.home
                .join("state/dispatch.again")
                .to_string_lossy()
                .into_owned(),
            format!("{}.lock", st.display()),
        ];
        turn.extend(cmd.iter().cloned());
        if let Some(start) = &self.host.plan_start_cmd {
            let mut args = vec![n.to_string()];
            args.extend(turn);
            return run_quiet(start, &args, None, Duration::from_secs(300)) == Some(0);
        }
        let mut args: Vec<String> = vec![
            "--user".into(),
            "--quiet".into(),
            "--collect".into(),
            "--unit".into(),
            format!("tehas-plan-{n}"),
            "-p".into(),
            format!("RuntimeMaxSec={}", self.host.plan_timeout + 900),
            "/bin/bash".into(),
            "-c".into(),
            r#"[ ! -r "$HOME/tehas/env.sh" ] || . "$HOME/tehas/env.sh"; exec "$@""#.into(),
            "env-sh".into(),
        ];
        args.extend(turn);
        if run_quiet(&self.host.systemd_run, &args, None, Duration::from_secs(60)) != Some(0) {
            return false;
        }
        let lock = PathBuf::from(format!("{}.lock", st.display()));
        let rc = self.st(n, ".rc");
        for _ in 0..10 {
            if rc.exists() || held(&lock) {
                return true;
            }
            std::thread::sleep(self.host.launch_poll);
        }
        true
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
