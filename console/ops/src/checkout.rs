//! `tehasbot mcp fetch`: brings the shared checkout of the MCP server up to
//! its upstream (docs/specs/2026-11-21-mcp-checkout.md).
//!
//! `tehas-mcp-fetch.socket` starts it when the server, at a client's
//! handshake, connects to the socket. It fetches `origin` and moves the
//! checkout's branch forward to its upstream, never a merge or a reset, with
//! no hook run. At most one fetch a minute, and one at a time: a request in
//! that minute, or while another runs, does nothing. Each fetch leaves its
//! report in `~/tehas/mcp/checkout.json`, which the server gives with every
//! git answer.
//!
//! Exit: 0 fetched, nothing to do, or a fetch ran a moment ago; 1 the fetch
//! or the move failed, which the report says; 78 TEHAS_MCP_CHECKOUT is
//! malformed.

use serde_json::json;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tehas_core::file::{write_atomic, Temp};
use tehas_core::{call, mask, time};

/// The report, under the home.
pub const REPORT: &str = "mcp/checkout.json";
const LOCK: &str = "state/mcp-fetch.lock";
/// Seconds after a fetch in which a request does nothing.
pub const QUIET: i64 = 60;
const BOUND: Duration = Duration::from_secs(120);
const ERROR_MAX: usize = 400;

/// What a run did.
#[derive(Debug, PartialEq)]
pub enum Done {
    /// The checkout is at its upstream's commit.
    Fetched(String),
    /// A fetch ran within `QUIET` seconds, or another runs now.
    Quiet,
    Failed(String),
}

/// One run for the checkout at `dir`, its report under `home`, at `now`.
pub fn fetch(home: &Path, dir: &Path, now: i64) -> Done {
    let Ok(lock) = File::create(home.join(LOCK)) else {
        return Done::Failed(format!("{} cannot be made", home.join(LOCK).display()));
    };
    if lock.try_lock().is_err() {
        return Done::Quiet;
    }
    let report = home.join(REPORT);
    if last(&report).is_some_and(|t| now - t < QUIET) {
        return Done::Quiet;
    }
    let done = steps(dir);
    let body = match &done {
        Done::Fetched(head) => json!({"api": 1, "time": time::iso(now), "ok": true, "head": head}),
        Done::Failed(why) => json!({"api": 1, "time": time::iso(now), "ok": false, "error": why}),
        Done::Quiet => unreachable!("steps fetch or fail"),
    };
    if let Err(e) = write_atomic(&report, format!("{body}\n").as_bytes(), 0o600, &Temp::Pid) {
        return Done::Failed(format!("{} not written: {e}", report.display()));
    }
    done
}

/// The time of the last report, when it has one.
fn last(report: &Path) -> Option<i64> {
    let v: serde_json::Value = serde_json::from_slice(&fs::read(report).ok()?).ok()?;
    time::from_iso(v["time"].as_str()?)
}

/// The fetch and the move. Hooks are off for both, so nothing the
/// checkout's own directory holds is run.
fn steps(dir: &Path) -> Done {
    let git = |args: &[&str]| {
        let mut all = vec![
            "-C".to_string(),
            dir.display().to_string(),
            "-c".to_string(),
            "core.hooksPath=/dev/null".to_string(),
        ];
        all.extend(args.iter().map(|a| a.to_string()));
        call::run("git", &all, BOUND)
    };
    let fail = |what: &str, ran: &call::Ran| {
        let said = ran.err.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
        let line: String = mask::mask(&format!("{what}: {said}"))
            .chars()
            .take(ERROR_MAX)
            .collect();
        Done::Failed(line.trim_end_matches([':', ' ']).to_string())
    };
    let ran = git(&["fetch", "--quiet", "--prune", "origin"]);
    if !ran.ok() {
        return fail("git fetch origin failed", &ran);
    }
    let ran = git(&["merge", "--ff-only", "--quiet", "@{upstream}"]);
    if !ran.ok() {
        return fail("the branch could not move forward to its upstream", &ran);
    }
    let ran = git(&["rev-parse", "HEAD"]);
    if !ran.ok() {
        return fail("HEAD could not be read", &ran);
    }
    Done::Fetched(ran.out.trim().to_string())
}

/// `tehasbot mcp fetch`.
pub fn main(home: &str, checkout: &str) -> u8 {
    if checkout.is_empty() {
        return 0;
    }
    if !tehas_core::settings::rules::check("TEHAS_MCP_CHECKOUT", checkout).is_ok() {
        eprintln!("tehasbot mcp fetch: TEHAS_MCP_CHECKOUT is not an absolute path");
        return 78;
    }
    match fetch(Path::new(home), &PathBuf::from(checkout), time::now()) {
        Done::Fetched(_) | Done::Quiet => 0,
        Done::Failed(why) => {
            eprintln!("tehasbot mcp fetch: {why}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tehas_core::testdir::TestDir;

    fn git(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Ada")
            .env("GIT_AUTHOR_EMAIL", "ada@example.org")
            .env("GIT_COMMITTER_NAME", "Ada")
            .env("GIT_COMMITTER_EMAIL", "ada@example.org")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    fn commit(dir: &Path, text: &str) -> String {
        fs::write(dir.join("a.txt"), text).unwrap();
        git(dir, &["add", "a.txt"]);
        git(dir, &["commit", "-q", "-m", text]);
        git(dir, &["rev-parse", "HEAD"])
    }

    /// An upstream, a home, and a clone of the upstream in it.
    fn setup(name: &str) -> (TestDir, PathBuf, PathBuf) {
        let t = TestDir::new(name);
        let up = t.join("up");
        fs::create_dir_all(&up).unwrap();
        git(&up, &["init", "-q", "-b", "main"]);
        commit(&up, "one");
        let home = t.join("home");
        fs::create_dir_all(home.join("mcp")).unwrap();
        fs::create_dir_all(home.join("state")).unwrap();
        git(&t, &["clone", "-q", "up", "clone"]);
        let clone = t.join("clone");
        (t, home, clone)
    }

    fn report(home: &Path) -> serde_json::Value {
        serde_json::from_slice(&fs::read(home.join(REPORT)).unwrap()).unwrap()
    }

    #[test]
    fn a_fetch_moves_the_branch_to_its_upstream_and_reports_it() {
        let (t, home, clone) = setup("ops-checkout-fetch");
        let new = commit(&t.join("up"), "two");
        assert_eq!(fetch(&home, &clone, 1000), Done::Fetched(new.clone()));
        assert_eq!(git(&clone, &["rev-parse", "HEAD"]), new);
        let r = report(&home);
        assert_eq!(r["ok"], true);
        assert_eq!(r["head"], new.as_str());
        assert_eq!(r["time"], time::iso(1000).as_str());
    }

    #[test]
    fn a_request_within_a_minute_of_a_fetch_does_nothing() {
        let (t, home, clone) = setup("ops-checkout-quiet");
        let first = git(&clone, &["rev-parse", "HEAD"]);
        assert_eq!(fetch(&home, &clone, 1000), Done::Fetched(first.clone()));
        let new = commit(&t.join("up"), "two");
        assert_eq!(fetch(&home, &clone, 1000 + QUIET - 1), Done::Quiet);
        assert_eq!(git(&clone, &["rev-parse", "HEAD"]), first);
        assert_eq!(fetch(&home, &clone, 1000 + QUIET), Done::Fetched(new));
    }

    #[test]
    fn a_fetch_while_another_runs_does_nothing() {
        let (_t, home, clone) = setup("ops-checkout-locked");
        let held = File::create(home.join(LOCK)).unwrap();
        held.lock().unwrap();
        assert_eq!(fetch(&home, &clone, 1000), Done::Quiet);
        assert!(!home.join(REPORT).exists());
    }

    // A branch that went its own way is never merged or reset: the fetch
    // fails, says so, and leaves the commit where it was.
    #[test]
    fn a_branch_that_cannot_move_forward_is_left_and_reported() {
        let (t, home, clone) = setup("ops-checkout-diverged");
        commit(&t.join("up"), "two");
        let mine = commit(&clone, "mine");
        let Done::Failed(why) = fetch(&home, &clone, 1000) else {
            panic!("a diverged branch moved");
        };
        assert!(
            why.starts_with("the branch could not move forward"),
            "{why}"
        );
        assert_eq!(git(&clone, &["rev-parse", "HEAD"]), mine);
        assert_eq!(report(&home)["ok"], false);
        assert_eq!(report(&home)["error"], why.as_str());
    }

    #[test]
    fn no_hook_of_the_checkout_is_run() {
        let (t, home, clone) = setup("ops-checkout-hooks");
        let marker = t.join("ran");
        for hook in ["post-merge", "reference-transaction", "post-checkout"] {
            let path = clone.join(".git/hooks").join(hook);
            fs::write(&path, format!("#!/bin/sh\ntouch {}\n", marker.display())).unwrap();
            tehas_core::testing::write_exe(&path, fs::read_to_string(&path).unwrap()).unwrap();
        }
        // git itself runs them: a plain fetch updates a ref.
        commit(&t.join("up"), "two");
        git(&clone, &["fetch", "-q", "origin"]);
        assert!(marker.exists(), "the fixture's hook does not run");
        fs::remove_file(&marker).unwrap();
        commit(&t.join("up"), "three");
        assert!(matches!(fetch(&home, &clone, 1000), Done::Fetched(_)));
        assert!(!marker.exists(), "a hook of the checkout ran");
    }

    #[test]
    fn an_upstream_that_is_gone_is_reported() {
        let (t, home, clone) = setup("ops-checkout-gone");
        fs::remove_dir_all(t.join("up")).unwrap();
        let Done::Failed(why) = fetch(&home, &clone, 1000) else {
            panic!("a fetch with no upstream worked");
        };
        assert!(why.starts_with("git fetch origin failed"), "{why}");
    }
}
