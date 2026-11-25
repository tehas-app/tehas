//! The calls of the analytics program, $TEHAS_HOME/libexec/analytics, for
//! tehasbot status: its issue events, and the files it writes for the MCP
//! server. sessions.jsonl, the last thirty days, and outcomes.jsonl, how the
//! runs ended for a year back, are private files under mcp/ (never live/):
//! the program makes the directory (700) and writes both itself (600,
//! renamed into place).
//!
//! Every call has a time limit, TEHAS_ANALYTICS_WAIT seconds or five. A call
//! that fails or runs out of time is one line on standard error, "WARN
//! analytics <subcommand>: exit N" with 124 at the limit and 137 a second later
//! for a program that ignores TERM, and changes nothing else. What the program
//! says on standard error goes there too. Nothing runs unless TEHAS_ANALYTICS
//! is on and the program is installed.
//!
//! Off, or no program: none of the files. A file whose write fails stays
//! while it is younger than KEEP seconds: one poll that meets a locked
//! database does not empty it, and a database that keeps failing does not
//! show old figures for long.

use std::env;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use tehas_core::time;

const LIMIT: u64 = 5;
const KEEP: i64 = 600;

/// A line on standard error, as every line of tehasbot status is.
pub fn log(line: &str) {
    eprintln!("{} {line}", time::iso(time::now()));
}

/// 1 to 999 seconds, else the limit.
fn seconds(given: Option<&str>) -> u64 {
    given
        .filter(|w| {
            (1..=3).contains(&w.len())
                && !w.starts_with('0')
                && w.bytes().all(|b| b.is_ascii_digit())
        })
        .and_then(|w| w.parse().ok())
        .unwrap_or(LIMIT)
}

fn run_status(mut cmd: Command) -> i32 {
    match cmd.status() {
        Ok(s) => s
            .code()
            .or_else(|| s.signal().map(|n| 128 + n))
            .unwrap_or(1),
        Err(_) => 1,
    }
}

fn installed(bin: &Path) -> bool {
    fs::metadata(bin).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

pub struct Analytics {
    home: PathBuf,
    bin: PathBuf,
    wait: u64,
    on: bool,
}

impl Analytics {
    pub fn new(home: &Path) -> Analytics {
        let bin = home.join("libexec/analytics");
        Analytics {
            home: home.to_path_buf(),
            wait: seconds(env::var("TEHAS_ANALYTICS_WAIT").ok().as_deref()),
            on: env::var("TEHAS_ANALYTICS").as_deref() == Ok("on") && installed(&bin),
            bin,
        }
    }

    /// The seconds a call may take.
    pub fn wait(&self) -> Duration {
        Duration::from_secs(self.wait)
    }

    fn timed(&self) -> Vec<String> {
        let mut words: Vec<String> = ["-k", "1"].map(String::from).to_vec();
        words.push(self.wait.to_string());
        words.push(self.bin.to_string_lossy().into_owned());
        words
    }

    /// The program's exit, None where it is not to be called. `private` runs
    /// it under umask 077, the mode of what it writes. A shell sets that:
    /// nothing here sets it between fork and exec.
    fn call(&self, args: &[&str], private: bool) -> Option<i32> {
        if !self.on {
            return None;
        }
        let mut cmd = if private {
            let mut sh = Command::new("sh");
            sh.args(["-c", "umask 077; exec \"$@\"", "sh", "timeout"]);
            sh
        } else {
            Command::new("timeout")
        };
        cmd.args(self.timed())
            .args(args)
            .env("TEHAS_HOME", &self.home)
            .stdin(Stdio::null())
            .stdout(Stdio::null());
        let rc = run_status(cmd);
        if rc != 0 {
            log(&format!("WARN analytics {}: exit {rc}", args[0]));
        }
        Some(rc)
    }

    /// One issue event of the factory's analytics.
    pub fn event(&self, issue: &str, event: &str, at: Option<i64>) {
        let at = at.map(|t| t.to_string());
        let mut args = vec!["event", issue, event];
        if let Some(at) = &at {
            args.extend(["--at", at]);
        }
        self.call(&args, false);
    }

    /// The MCP server's files, in the order a run asks for them.
    pub fn publish(&self) {
        let mcp = self.home.join("mcp");
        self.private("sessions", &mcp);
        self.private("outcomes", &mcp);
    }

    /// <subcommand> writes <dir>/<subcommand>.jsonl itself.
    fn private(&self, subcommand: &str, dir: &Path) {
        let file = dir.join(format!("{subcommand}.jsonl"));
        let dir = dir.to_string_lossy();
        match self.call(&[subcommand, "--out", &dir], true) {
            Some(0) => {}
            None => {
                let _ = fs::remove_file(&file);
            }
            Some(_) => keep_or_remove(&file),
        }
    }
}

/// A file no run has written lately goes.
fn keep_or_remove(file: &Path) {
    let Ok(meta) = fs::metadata(file) else {
        return;
    };
    if time::now() - meta.mtime() >= KEEP {
        let _ = fs::remove_file(file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Host(tehas_core::testdir::TestDir);

    impl Host {
        /// A home whose program is the script `body`, which writes its
        /// arguments and TEHAS_HOME in calls.
        fn new(name: &str, body: &str) -> Host {
            let home = tehas_core::testdir::TestDir::new(&format!("pub-{name}"));
            fs::create_dir_all(home.join("libexec")).unwrap();
            let host = Host(home);
            host.program(body);
            host
        }

        fn program(&self, body: &str) {
            let bin = self.0.join("libexec/analytics");
            let calls = self.0.join("calls");
            let script = format!(
                "#!/bin/sh\necho \"$* $TEHAS_HOME\" >>{}\n{body}\n",
                calls.display()
            );
            tehas_core::testing::write_exe(bin, script).unwrap();
        }

        fn analytics(&self, on: bool, wait: u64) -> Analytics {
            Analytics {
                home: self.0.to_path_buf(),
                bin: self.0.join("libexec/analytics"),
                wait,
                on,
            }
        }

        fn calls(&self) -> String {
            fs::read_to_string(self.0.join("calls")).unwrap_or_default()
        }
    }

    #[test]
    fn the_limit_is_a_few_seconds_unless_a_wait_is_a_number() {
        assert_eq!(LIMIT, 5);
        assert_eq!(seconds(None), 5);
        for bad in ["soon", "", "0", "1000", "05", "-1", "1 "] {
            assert_eq!(seconds(Some(bad)), 5, "{bad:?}");
        }
        assert_eq!(seconds(Some("1")), 1);
        assert_eq!(seconds(Some("999")), 999);
    }

    #[test]
    fn a_call_runs_the_program_under_the_limit_with_its_arguments_and_home() {
        let h = Host::new("call", "echo printed");
        let a = h.analytics(true, 7);
        assert_eq!(a.timed()[..3], ["-k", "1", "7"]);
        a.event("7", "ready", None);
        a.event("8", "merged", Some(100));
        let home = h.0.display();
        assert_eq!(
            h.calls(),
            format!("event 7 ready {home}\nevent 8 merged --at 100 {home}\n")
        );
    }

    #[test]
    fn a_failing_program_is_an_exit_and_a_hanging_one_the_limit() {
        let h = Host::new("fail", "exit 70");
        let a = h.analytics(true, 1);
        assert_eq!(a.call(&["sample"], false), Some(70));
        h.program("exec sleep 30");
        let at = std::time::Instant::now();
        assert_eq!(a.call(&["sample"], false), Some(124));
        assert!(at.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn off_or_not_installed_the_program_is_not_called() {
        let h = Host::new("off", "");
        assert_eq!(h.analytics(false, 5).call(&["x"], false), None);
        assert_eq!(h.calls(), "");
        let bin = h.0.join("libexec/analytics");
        assert!(installed(&bin));
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(!installed(&bin));
        fs::remove_file(&bin).unwrap();
        assert!(!installed(&bin));
        fs::create_dir(&bin).unwrap();
        assert!(!installed(&bin));
    }

    #[test]
    fn a_private_call_runs_under_a_closed_umask() {
        let h = Host::new("umask", "umask >>\"$TEHAS_HOME/mask\"");
        h.analytics(true, 5).call(&["sessions"], true);
        assert_eq!(fs::read_to_string(h.0.join("mask")).unwrap().trim(), "0077");
    }

    #[test]
    fn a_file_younger_than_the_keep_stays_and_an_older_one_goes() {
        let h = Host::new("keep", "");
        let file = h.0.join("f");
        fs::write(&file, "x").unwrap();
        keep_or_remove(&file);
        assert!(file.exists());
        let old = std::time::SystemTime::now() - Duration::from_secs(KEEP as u64);
        fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(old)
            .unwrap();
        keep_or_remove(&file);
        assert!(!file.exists());
        keep_or_remove(&file);
    }
}
