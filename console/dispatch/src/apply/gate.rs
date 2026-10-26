//! What a start passes before any label changes, as libexec/dispatch does
//! it: systemd's answer for the cycle's unit (units_ok), the code index
//! (index_gate, with index_busy, index_report and index_launch), and the
//! checkout's refresh (refresh_checkout), which a start and a planning turn
//! run first. Either gate can end the poll: units_ok with 1, the index with
//! 0 while it is refreshed.

use super::Writer;
use crate::read::host::held;
use crate::read::run::{run, run_grace, run_quiet};
use serde_json::Value;
use std::time::Duration;

/// The script tehas-index.service runs, index_launch's: the index's exit
/// status kept, and a poll asked for at its end.
const INDEX: &str = r#"log=$1 rc=$2 again=$3; shift 3; "$@" >>"$log" 2>&1; echo "$?" >"$rc.new" && mv "$rc.new" "$rc"; : >"$again""#;

impl Writer {
    fn git(&self, args: &[&str]) -> crate::read::run::Ran {
        let mut all = vec!["-C".to_string(), self.host.checkout.clone()];
        all.extend(args.iter().map(|a| a.to_string()));
        run(&self.host.git, &all, Duration::from_secs(60))
    }

    /// units_ok: systemd can load the cycle's unit; without its answer every
    /// issue picked would be marked and then parked, at every poll.
    pub(super) fn units(&mut self, n: u64, log: &mut Vec<String>) -> bool {
        if self.host.start_cmd.is_some() {
            return true;
        }
        let r = run(
            &self.host.systemctl,
            &[
                "--user".into(),
                "show".into(),
                "-p".into(),
                "LoadState".into(),
                "--value".into(),
                format!("tehas-cycle@{n}.service"),
            ],
            Duration::from_secs(30),
        );
        let state = if r.ok() {
            r.out.trim().to_string()
        } else {
            String::new()
        };
        if state == "loaded" {
            return true;
        }
        let said = if state.is_empty() {
            "no answer"
        } else {
            &state
        };
        log.push(format!("ERROR: systemd cannot load tehas-cycle@.service (LoadState: {said}): run install.sh and its units step, and run tehas dispatch as the tehas user's own session"));
        false
    }

    /// refresh_checkout: the checkout on its branch when it is clean, and
    /// fast-forwarded, bounded: it runs under the poll's lock.
    pub(super) fn refresh_checkout(&mut self, log: &mut Vec<String>) {
        let (checkout, branch) = (self.host.checkout.clone(), self.host.branch.clone());
        let status = self.git(&["status", "--porcelain", "--untracked-files=no"]);
        if status.out.trim().is_empty() {
            if !self.git(&["checkout", "-q", &branch]).ok() {
                log.push(format!("WARN: could not switch {checkout} to {branch}"));
            }
        } else {
            log.push(format!(
                "WARN: {checkout} has uncommitted changes, leaving its branch alone"
            ));
        }
        // timeout -k 10, a group's: the fetch's helpers go with it.
        let fetched = run_grace(
            &self.host.git,
            &[
                "-C".into(),
                checkout.clone(),
                "fetch".into(),
                "-q".into(),
                "origin".into(),
            ],
            None,
            self.host.fetch_wait,
            Duration::from_secs(10),
        ) == Some(0);
        if !fetched
            || !self
                .git(&["merge", "-q", "--ff-only", &format!("origin/{branch}")])
                .ok()
        {
            log.push(format!("WARN: could not fast-forward {checkout}"));
        }
    }

    /// index_busy: an index run holds its lock, or its unit runs and has not
    /// taken it yet. One that left its exit status has ended.
    fn index_busy(&self) -> bool {
        let state = self.home.join("state");
        if held(&state.join("index.lock")) {
            return true;
        }
        if state.join("index.rc").is_file() {
            return false;
        }
        let r = run(
            &self.host.systemctl,
            &[
                "--user".into(),
                "show".into(),
                "-p".into(),
                "ActiveState".into(),
                "--value".into(),
                "tehas-index.service".into(),
            ],
            Duration::from_secs(30),
        );
        r.ok() && matches!(r.out.trim(), "active" | "activating")
    }

    /// index_report: how the last index run ended, said once. False when no
    /// run ended since the last report.
    fn index_report(&self, log: &mut Vec<String>) -> bool {
        let rc = self.home.join("state/index.rc");
        if !rc.is_file() {
            return false;
        }
        // One that cannot be read is said as a failed run, and goes.
        let b = std::fs::read(&rc).unwrap_or_default();
        let code = String::from_utf8_lossy(&b[..b.len().min(8)]).to_string();
        let _ = std::fs::remove_file(&rc);
        if code.trim_end_matches('\n') != "0" {
            log.push(format!(
                "WARN: the code index was not refreshed, see {}",
                self.home.join("logs/dispatch-index.log").display()
            ));
        }
        true
    }

    /// tehas_index_state: current when state/index.json names the commit and
    /// an index database is there, failed when the last run failed at it,
    /// else due.
    fn index_state(&self, base: &str) -> &'static str {
        let state = self.home.join("state");
        let named = std::fs::read_to_string(state.join("index.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v.get("commit").and_then(Value::as_str).map(str::to_string));
        if named.as_deref() == Some(base) {
            let db = std::fs::read_dir(state.join("index"))
                .into_iter()
                .flatten()
                .flatten()
                .any(|e| {
                    let n = e.file_name().to_string_lossy().into_owned();
                    !n.starts_with('.')
                        && n.ends_with(".db")
                        && n != "_config.db"
                        && e.path().is_file()
                });
            if db {
                return "current";
            }
        }
        let failed = std::fs::read(state.join("index.failed")).unwrap_or_default();
        // As $(head -c 64 ...) reads it: without its line ends.
        if String::from_utf8_lossy(&failed[..failed.len().min(64)]).trim_end_matches('\n') == base {
            "failed"
        } else {
            "due"
        }
    }

    /// index_launch: tehas index --before-cycle as tehas-index.service, which
    /// asks for a poll when it ends.
    fn index_launch(&self) -> bool {
        let _ = std::fs::remove_file(self.home.join("state/index.rc"));
        let p = |x: &str| self.home.join(x).to_string_lossy().into_owned();
        let cmd: Vec<String> = vec![
            "/bin/bash".into(),
            "-c".into(),
            INDEX.into(),
            "index-run".into(),
            p("logs/dispatch-index.log"),
            p("state/index.rc"),
            p("state/dispatch.again"),
            self.host.index_cmd.clone(),
            "--before-cycle".into(),
        ];
        if let Some(start) = &self.host.index_start_cmd {
            return run_quiet(start, &cmd, None, Duration::from_secs(300)) == Some(0);
        }
        let mut args: Vec<String> = vec![
            "--user".into(),
            "--quiet".into(),
            "--collect".into(),
            "--unit".into(),
            "tehas-index".into(),
            "-p".into(),
            "RuntimeMaxSec=2400".into(),
            "/bin/bash".into(),
            "-c".into(),
            r#"[ ! -r "$HOME/tehas/env.sh" ] || . "$HOME/tehas/env.sh"; exec "$@""#.into(),
            "env-sh".into(),
        ];
        args.extend(cmd);
        run_quiet(&self.host.systemd_run, &args, None, Duration::from_secs(60)) == Some(0)
    }

    /// index_gate: true when the cycles may start now; false when they wait
    /// for the index, which ends the poll with 0.
    /// The cycles counted are the locks held now, after this poll's stops,
    /// as the bash counts them again: a cycle that kept its lock is one.
    pub(super) fn index_gate(&mut self, log: &mut Vec<String>) -> bool {
        let running = crate::read::host::running(&self.home);
        if !running.is_empty() {
            let list: Vec<String> = running.iter().map(|n| format!("#{n}")).collect();
            log.push(format!(
                "the code index stays as it is: a cycle is running on {}",
                list.join(", ")
            ));
            return true;
        }
        self.refresh_checkout(log);
        self.fetched = true;
        if self.index_busy() {
            log.push("holding: the cycles wait for the code index, which tehas-index.service is refreshing".into());
            return false;
        }
        self.index_report(log);
        let branch = format!("refs/remotes/origin/{}^{{commit}}", self.host.branch);
        let base = self.git(&["rev-parse", "-q", "--verify", &branch]);
        if !base.ok() {
            return true;
        }
        if self.index_state(base.out.trim()) != "due" {
            return true;
        }
        if !self.index_launch() {
            log.push(
                "WARN: the code index was not refreshed: tehas-index.service did not start".into(),
            );
            return true;
        }
        // A launcher that ran the index to its end: the poll it asked for is
        // this one.
        if self.index_report(log) {
            let _ = std::fs::remove_file(self.home.join("state/dispatch.again"));
            return true;
        }
        log.push("holding: the code index is refreshed first, as tehas-index.service; the cycles start on the poll after it".into());
        false
    }
}
