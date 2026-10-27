//! The stops of a decision, as libexec/dispatch does them: a cycle that has
//! not been heard from (the quiet loop), a cycle whose work is done, and the
//! workspaces of issues no cycle has run on for 7 days.

use super::{text, word, Writer};
use crate::decide::{iso, quiet_due, QUIET_ASKS, QUIET_BOUND};
use crate::read::host::held;
use crate::read::run::run_quiet;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// What a stop does next.
pub(super) enum Then {
    Go,
    /// The store cannot be used: the poll ends with 78.
    End,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Writer {
    /// quiet_store_usable: a store that refused (1) leaves the poll going;
    /// any other failure ends it.
    fn usable(f: &Value, log: &mut Vec<String>) -> Then {
        if f.get("rc") == Some(&serde_json::json!(1)) {
            return Then::Go;
        }
        log.push(format!(
            "ERROR: the state store cannot be used: {}",
            text(f, "line")
        ));
        Then::End
    }

    /// The quiet loop for one cycle: the episode on record first, then what
    /// the record's answer says is due.
    pub(super) fn quiet(&mut self, n: u64, a: &Value, log: &mut Vec<String>) -> Then {
        let said_for = match a.get("said_for") {
            Some(Value::Number(x)) => x.to_string(),
            Some(Value::String(s)) => s.clone(),
            _ => String::new(),
        };
        let heard: u64 = said_for.parse().unwrap_or(0);
        let why = format!(
            "the cycle for #{n} holds its lock but has not been heard from since {}, more than {QUIET_BOUND}s",
            iso(heard)
        );
        let unit = format!("tehas-cycle@{n}.service");
        let note = match self.calls.store(&[
            "cycle-note",
            &n.to_string(),
            "--kind",
            "quiet",
            "--said-for",
            &said_for,
        ]) {
            Ok(v) => v,
            Err(f) => {
                if let Then::End = Self::usable(&f, log) {
                    return Then::End;
                }
                match word(&f).as_str() {
                    "ended" | "nocycle" => log.push(format!("WARN: {why}, and the state store has no open record of it, so the poll does not stop it: if it stays, a person can, with tehas stop {n} or systemctl --user stop {unit}")),
                    w => log.push(format!("WARN: {why}, but the state store could not record it ({w}), so the poll asks nothing this time")),
                }
                return Then::Go;
            }
        };
        let number = |k: &str, max: u32| match note.get(k) {
            None | Some(Value::Null) if k == "stop_asked_at" => Some(0),
            Some(Value::Number(x)) => x.as_u64().filter(|v| v.to_string().len() <= max as usize),
            _ => None,
        };
        let (Some(times), Some(asked)) = (number("times", 6), number("stop_asked_at", 12)) else {
            log.push(format!("WARN: {why}, but the state store's record of it does not read as one, so the poll asks nothing this time"));
            return Then::Go;
        };
        match quiet_due(times, asked, now()) {
            "wait" => return Then::Go,
            "stop" => log.push(format!("WARN: {why}: stopping {unit}")),
            "again" => log.push(format!("WARN: {why}: the poll asks {unit} to stop again")),
            _ => {
                log.push(format!("WARN: {why}: the poll asked {QUIET_ASKS} times to stop {unit} and asks no more: it needs a person, who can stop it with systemctl --user stop {unit}"));
                let mut args = vec!["cycle-stop-asked".to_string(), n.to_string()];
                if times != QUIET_ASKS {
                    args.push("--said-only".into());
                }
                let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                if let Err(f) = self.calls.store(&refs) {
                    if let Then::End = Self::usable(&f, log) {
                        return Then::End;
                    }
                    log.push(format!(
                        "WARN: the state store did not record that the poll asks no more of {unit} ({})",
                        word(&f)
                    ));
                }
                return Then::Go;
            }
        }
        if let Err(f) = self.calls.store(&["cycle-stop-asked", &n.to_string()]) {
            if let Then::End = Self::usable(&f, log) {
                return Then::End;
            }
            log.push(format!(
                "WARN: the state store did not record the stop of {unit} ({}): the poll asks again",
                word(&f)
            ));
        }
        // Without waiting for it: the cycle reports as it stops.
        let rc = match &self.host.stop_cmd {
            Some(cmd) => run_quiet(cmd, &[n.to_string()], None, Duration::from_secs(30)),
            None => run_quiet(
                &self.host.systemctl,
                &[
                    "--user".into(),
                    "--no-block".into(),
                    "stop".into(),
                    unit.clone(),
                ],
                None,
                Duration::from_secs(30),
            ),
        };
        if rc != Some(0) {
            log.push(format!(
                "WARN: could not stop {unit} (exit {}): it holds its slot, and the poll asks again",
                rc.unwrap_or(124)
            ));
        }
        Then::Go
    }

    /// A cycle whose work is done: told why in <issue>.done, stopped, and
    /// waited for until it lets its lock go.
    pub(super) fn stop_done(&mut self, n: u64, a: &Value, log: &mut Vec<String>) {
        let done = self.home.join(format!("state/cycles/{n}.done"));
        if std::fs::write(&done, format!("{}\n", text(a, "why"))).is_err() {
            log.push(format!("WARN: could not write the reason for #{n}"));
        }
        let unit = format!("tehas-cycle@{n}.service");
        let rc = match &self.host.stop_cmd {
            Some(cmd) => run_quiet(cmd, &[n.to_string()], None, self.host.done_wait),
            None => run_quiet(
                &self.host.systemctl,
                &["--user".into(), "stop".into(), unit.clone()],
                None,
                self.host.done_wait,
            ),
        };
        if rc != Some(0) {
            log.push(format!("WARN: could not stop {unit}"));
        }
        let lock = self.home.join(format!("state/cycles/{n}.lock"));
        for _ in 0..20 {
            if !held(&lock) {
                return;
            }
            std::thread::sleep(self.host.lock_poll);
        }
        if held(&lock) {
            log.push(format!(
                "WARN: the cycle for #{n} still holds its lock after its stop"
            ));
        }
    }

    /// The names an issue has on this host, as libexec/stop builds them.
    fn leftovers(&self, n: u64) -> Vec<PathBuf> {
        let work = Path::new(&self.host.work_dir);
        let mut out = Vec::new();
        if let (Some(works), Some(base)) = (work.parent(), work.file_name()) {
            if works.join(".tehas-workspaces").is_file() {
                let base = base.to_string_lossy();
                for end in ["", ".before-retry", ".images", ".state"] {
                    out.push(works.join(format!("{base}-{n}{end}")));
                }
            }
        }
        let runs = Path::new(&self.host.user_home).join(".pi/agent/ensemble-runs");
        out.push(runs.join(format!("issue-{n}")));
        out.push(runs.join(format!("plan-{n}")));
        out.push(self.home.join(format!("live/console-{n}.txt")));
        out
    }

    /// sweep_workspaces for one issue the decision found unused: its names
    /// removed under its lock, the first that cannot be ending it.
    pub(super) fn sweep(&mut self, n: u64, a: &Value, log: &mut Vec<String>) {
        let lock = self.home.join(format!("state/cycles/{n}.lock"));
        let taken = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&lock)
            .ok()
            .filter(|f| f.try_lock().is_ok());
        let Some(_held) = taken else {
            log.push(format!(
                "WARN: could not take the lock of #{n}, so nothing of it was removed"
            ));
            return;
        };
        let mut said = false;
        for e in self.leftovers(n) {
            let Ok(m) = std::fs::symlink_metadata(&e) else {
                continue;
            };
            let gone = if m.is_dir() {
                std::fs::remove_dir_all(&e)
            } else {
                std::fs::remove_file(&e)
            };
            if gone.is_ok() {
                said = true;
            } else {
                let shown = e.strip_prefix(&self.home).unwrap_or(&e);
                log.push(format!(
                    "WARN: could not remove {} of #{n}",
                    shown.display()
                ));
                said = false;
                break;
            }
        }
        if said {
            if let Some(line) = a.get("log").and_then(Value::as_str) {
                log.push(line.to_string());
            }
        }
    }
}
