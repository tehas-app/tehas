//! What the runs of a plugin share between the one who asks for a run and
//! the runner (docs/specs/2026-11-21-plugins.md, "Runs"): the files under
//! `~/tehas/plugins/<name>/`, the lock a run holds, and the start of its unit.
//!
//! - `refresh.lock`: held by a refresh for its whole run, and shared by a
//!   hook, so a refresh and a hook of one plugin never overlap.
//! - `refresh.json`: what asked for the next refresh, `{trigger, by}`.
//! - `head`: the head of the last refresh that began, which the tick reads.
//! - `last.log`: the last refresh's log, at most 1 MiB.
//! - `hooks/<pr>.json`: the request of a hook (`tehas_core::plugin::hook`);
//!   `hooks/<pr>.claimed.json` the one a hook took and runs;
//!   `hooks/<pr>.verdict.json` the verdict read; `hooks/<pr>.log` its log.
//! - `status-unwritten`: why the last hook's commit status was not written.
//! - `calls/slot-0.lock` and `calls/slot-1.lock`: one held by each call of a
//!   tool while it runs, and `calls/by-<caller>.lock` by each caller's.

use super::exec::{Cmd, Exec};
use super::prepare;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tehas_core::file::{write_atomic, Temp};
use tehas_core::lock::{held_in, probe, Probe};
use tehas_core::plugin::hook;
use tehas_core::plugin::run::Trigger;

/// The most bytes of a run's log that are kept.
pub const LOG_MAX: u64 = 1 << 20;

/// The refusal a run whose `/state` passed the disk limit leaves. Only
/// `tehas plugin run` takes it off.
pub const DISK_REFUSAL: &str =
    "its /state passed the disk limit and was emptied: tehas plugin run starts it again";

/// The lock a hook on `pr` holds while its unit runs.
pub fn hook_lock(home: &Path, name: &str, pr: u64) -> PathBuf {
    hooks(home, name).join(format!("{pr}.lock"))
}

/// Whether a lock is held; one that cannot be probed counts as held.
pub fn held(file: &Path) -> bool {
    matches!(probe(file), Probe::Held | Probe::Unknown(_))
}

pub fn calls(home: &Path, name: &str) -> PathBuf {
    prepare::dir(home, name).join("calls")
}

/// Whether a call of the plugin runs, so the sweep leaves its run alone.
pub fn calling(home: &Path, name: &str) -> bool {
    held_in(&calls(home, name))
        .iter()
        .any(|l| l.starts_with("slot-"))
}

pub fn refresh_lock(home: &Path, name: &str) -> PathBuf {
    prepare::dir(home, name).join("refresh.lock")
}

pub fn request(home: &Path, name: &str) -> PathBuf {
    prepare::dir(home, name).join("refresh.json")
}

pub fn tried(home: &Path, name: &str) -> PathBuf {
    prepare::dir(home, name).join("head")
}

pub fn hooks(home: &Path, name: &str) -> PathBuf {
    hook::dir(home, name)
}

pub fn hook_request(home: &Path, name: &str, pr: u64) -> PathBuf {
    hook::request(home, name, pr)
}

pub fn verdict(home: &Path, name: &str, pr: u64) -> PathBuf {
    hooks(home, name).join(format!("{pr}.verdict.json"))
}

/// The last log of a refresh, or of the hook on `pr`.
pub fn log(home: &Path, name: &str, pr: Option<u64>) -> PathBuf {
    match pr {
        None => prepare::dir(home, name).join("last.log"),
        Some(pr) => hooks(home, name).join(format!("{pr}.log")),
    }
}

/// Whether a run of the plugin holds its lock. One that cannot be probed
/// counts as running.
pub fn running(home: &Path, name: &str) -> bool {
    held(&refresh_lock(home, name))
}

pub fn trigger(word: &str) -> Option<Trigger> {
    [
        Trigger::Timer,
        Trigger::Host,
        Trigger::Action,
        Trigger::Cycle,
    ]
    .into_iter()
    .find(|t| t.word() == word)
}

/// Who a run is by, as the store keeps it: a login, `timer` or `host`.
pub fn is_by(s: &str) -> bool {
    (1..=39).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// What asked for a refresh.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asked {
    pub trigger: Trigger,
    pub by: String,
}

pub fn read_request(home: &Path, name: &str) -> Option<Asked> {
    let v: Value = serde_json::from_slice(&std::fs::read(request(home, name)).ok()?).ok()?;
    let by = v.get("by")?.as_str().filter(|b| is_by(b))?.to_string();
    Some(Asked {
        trigger: trigger(v.get("trigger")?.as_str()?)?,
        by,
    })
}

/// The unit of a refresh, or of the hook on `pr`.
pub fn unit(name: &str, pr: Option<u64>) -> String {
    match pr {
        None => format!("tehas-plugin@{name}.service"),
        Some(pr) => hook::unit(name, pr),
    }
}

/// `systemctl --user start --no-block`: the run is the unit's, and this
/// waits for nothing.
pub fn start(x: &mut dyn Exec, unit: &str) -> Result<(), String> {
    let args = ["--user", "start", "--no-block", unit];
    let d = x.run(&Cmd::new(
        "systemctl",
        args.iter().map(|a| a.to_string()).collect(),
    ));
    if d.ok {
        Ok(())
    } else {
        Err(format!("systemctl could not start {unit}: {}", d.err))
    }
}

/// Writes what asks for the refresh, then starts its unit.
pub fn ask_refresh(x: &mut dyn Exec, home: &Path, name: &str, asked: &Asked) -> Result<(), String> {
    let body = json!({"trigger": asked.trigger.word(), "by": asked.by});
    write_atomic(
        &request(home, name),
        format!("{body}\n").as_bytes(),
        0o600,
        &Temp::Pid,
    )
    .map_err(|e| format!("cannot write the request: {e}"))?;
    start(x, &unit(name, None))
}

pub use tehas_core::plugin::hook::HookAsk;

pub fn read_hook(home: &Path, name: &str, pr: u64) -> Option<HookAsk> {
    hook::read_request(home, name, pr)
}

/// `hooks/<pr>.claimed.json`: the request a hook took and runs.
pub fn hook_claimed(home: &Path, name: &str, pr: u64) -> PathBuf {
    hooks(home, name).join(format!("{pr}.claimed.json"))
}

/// Takes the request of the hook on `pr` by one rename, so a request written
/// after it stays for the next take. None: no request, or one that does not
/// read.
pub fn claim_hook(home: &Path, name: &str, pr: u64) -> Option<HookAsk> {
    let claimed = hook_claimed(home, name, pr);
    std::fs::rename(hook_request(home, name, pr), &claimed).ok()?;
    HookAsk::read(&std::fs::read(&claimed).ok()?).filter(|h| h.pr == pr)
}

/// Writes the request of a hook, replacing one there, then starts its unit.
/// A hook running on the same pull request finishes and then runs the newer
/// request.
pub fn ask_hook(x: &mut dyn Exec, home: &Path, name: &str, ask: &HookAsk) -> Result<(), String> {
    hook::write_request(home, name, ask)?;
    start(x, &unit(name, Some(ask.pr)))
}

/// `status-unwritten`: why the last commit status of a hook could not be
/// written, which the runner leaves and takes away.
pub fn unwritten(home: &Path, name: &str) -> PathBuf {
    prepare::dir(home, name).join("status-unwritten")
}

/// The line `status-unwritten` holds, if there is one.
pub fn status_unwritten(home: &Path, name: &str) -> Option<String> {
    let text = std::fs::read_to_string(unwritten(home, name)).ok()?;
    let line = text.lines().next()?.trim();
    (!line.is_empty()).then(|| line.chars().take(200).collect())
}

/// The newest run of `runs` (the store's `plugin-runs --no-calls`, newest
/// first) and the tokens of the runs that started at or after `since`.
pub fn last_and_today(runs: &[Value], since: i64) -> (Option<Value>, u64) {
    let last = runs.first().map(|r| {
        json!({
            "kind": r["kind"],
            "trigger": r["trigger"],
            "start": r["start"],
            "end": r["end"],
            "result": r["result"],
            "tokens": r["tokens"],
        })
    });
    let today = runs
        .iter()
        .filter(|r| r["start"].as_i64().is_some_and(|s| s >= since))
        .filter_map(|r| r["tokens"].as_u64())
        .sum();
    (last, today)
}

#[cfg(test)]
mod tests {
    use super::super::exec::fake::{ok, Fake};
    use super::*;
    use tehas_core::testdir::TestDir;

    const H: &str = "89abcdef0123456789abcdef0123456789abcdef";
    const B: &str = "fedcba9876543210fedcba9876543210fedcba98";

    #[test]
    fn a_refresh_is_asked_by_its_request_and_its_unit() {
        let home = TestDir::new("plugin-runs-ask");
        std::fs::create_dir_all(prepare::dir(&home, "demo")).unwrap();
        let mut x = Fake::new();
        x.on("systemctl", ok(""));
        let asked = Asked {
            trigger: Trigger::Timer,
            by: "timer".into(),
        };
        ask_refresh(&mut x, &home, "demo", &asked).unwrap();
        assert_eq!(
            x.ran,
            ["systemctl --user start --no-block tehas-plugin@demo.service"]
        );
        assert_eq!(read_request(&home, "demo"), Some(asked));
        assert!(!running(&home, "demo"));
        let f = std::fs::File::create(refresh_lock(&home, "demo")).unwrap();
        f.lock().unwrap();
        assert!(running(&home, "demo"));
    }

    #[test]
    fn a_hook_request_reads_back_and_its_unit_names_the_pull_request() {
        let home = TestDir::new("plugin-runs-hook");
        let mut x = Fake::new();
        x.on("systemctl", ok(""));
        let ask = HookAsk {
            pr: 42,
            head: H.into(),
            base: B.into(),
            issue: Some(7),
            trigger: Trigger::Cycle,
        };
        ask_hook(&mut x, &home, "demo", &ask).unwrap();
        assert_eq!(
            x.ran,
            ["systemctl --user start --no-block tehas-plugin-hook@demo-42.service"]
        );
        assert_eq!(read_hook(&home, "demo", 42), Some(ask.clone()));
        assert_eq!(read_hook(&home, "demo", 43), None);
        let mut timer = ask.json();
        timer["trigger"] = json!("timer");
        assert_eq!(HookAsk::read(timer.to_string().as_bytes()), None);
        let mut short = ask.json();
        short["head"] = json!("89abcdef");
        assert_eq!(HookAsk::read(short.to_string().as_bytes()), None);
    }

    #[test]
    fn a_claim_takes_the_request_and_leaves_a_later_one() {
        let home = TestDir::new("plugin-runs-claim");
        let mut x = Fake::new();
        x.on("systemctl", ok(""));
        let ask = HookAsk {
            pr: 42,
            head: H.into(),
            base: B.into(),
            issue: None,
            trigger: Trigger::Host,
        };
        assert_eq!(claim_hook(&home, "demo", 42), None);
        ask_hook(&mut x, &home, "demo", &ask).unwrap();
        assert_eq!(claim_hook(&home, "demo", 42), Some(ask.clone()));
        assert_eq!(read_hook(&home, "demo", 42), None);
        assert_eq!(claim_hook(&home, "demo", 42), None);
        // Asked again after the claim: the new request is there to take.
        ask_hook(&mut x, &home, "demo", &ask).unwrap();
        assert_eq!(read_hook(&home, "demo", 42), Some(ask.clone()));
        assert_eq!(claim_hook(&home, "demo", 42), Some(ask));
    }

    #[test]
    fn the_last_run_and_the_tokens_of_today() {
        let runs = [
            json!({"kind": "refresh", "trigger": "timer", "start": 200, "end": 260, "result": "done", "tokens": 30, "by": "timer"}),
            json!({"kind": "hook", "trigger": "cycle", "start": 150, "end": null, "result": null, "tokens": 5}),
            json!({"kind": "refresh", "trigger": "host", "start": 50, "end": 60, "result": "failed", "tokens": 100}),
        ];
        let (last, today) = last_and_today(&runs, 100);
        assert_eq!(
            last,
            Some(
                json!({"kind": "refresh", "trigger": "timer", "start": 200, "end": 260, "result": "done", "tokens": 30})
            )
        );
        assert_eq!(today, 35);
        assert_eq!(last_and_today(&[], 0), (None, 0));
    }

    #[test]
    fn a_status_left_unwritten_is_read_by_its_first_line() {
        let home = TestDir::new("plugin-runs-unwritten");
        std::fs::create_dir_all(prepare::dir(&home, "demo")).unwrap();
        assert_eq!(status_unwritten(&home, "demo"), None);
        std::fs::write(unwritten(&home, "demo"), "HTTP 403\nmore\n").unwrap();
        assert_eq!(status_unwritten(&home, "demo"), Some("HTTP 403".into()));
        std::fs::write(unwritten(&home, "demo"), "\n").unwrap();
        assert_eq!(status_unwritten(&home, "demo"), None);
    }
}
