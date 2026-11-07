//! The facts of an ended cycle read from the host for its report: the files
//! of the protocol, the newest run log, the cycle's record and its approval on
//! the state store, the workspace, and when docker says the image on the record
//! was built.

use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tehas_core::call;

use crate::runner;

/// The files of the protocol a report reads, state/cycles/<issue>.<name>.
/// tehas work writes them and tehasbot cycle reads and removes them; a cycle
/// killed outright leaves them for the next cycle of the issue, and both clear
/// them before they start. What each is proof of:
///
///   runner_exit   0 when tehas work watched the container to its end, and the
///                 runner's own exit status for the comment. Its existence, not
///                 its content, is the proof: an empty file still says the run
///                 was watched.
///   status        the one word `killed`, only when tehas work saw the
///                 container end with 137. The only account of the end when the
///                 runner wrote no result: a payload read for its word.
///   progress.json the summary tehas work made of the run's NDJSON. It says the
///                 run log of the issue is this cycle's: tehas work writes no
///                 other file after the container ended, so one no older than
///                 the cycle's start was written by this cycle. An older one is
///                 a cycle that never reported, and its log is read for nothing.
///   nostart       one line: the container's exit status, 1 to 999, plain
///                 digits. Anything else is no note. Left only when the
///                 container was watched to its end, wrote neither a result nor
///                 a line of its stream, and nobody stopped it. It says no runner
///                 started, as opposed to one that died part way; tehasbot cycle
///                 makes the cap runner-never-started of it. Memory kills are
///                 counted in `oom`, as for every other end.
///   lean-refused  tehas work's 78 was for driver lean, not another 78.
///   refresh-conflict  tehas work's 78 was for a refresh whose pull request
///                 conflicts, on an image that cannot finish one: JSON naming
///                 the pull request, the branch and the files (src/conflict.rs).
///   comments      one word: how the image gives a retry's comments, `native`,
///                 `polyfill` or `off` (gives::comments_mode). Only with
///                 `native` or `polyfill` does a park offer the comments retry.
///   signal        one line: the signal that came to tehas work (TERM, INT, HUP,
///                 or - when not known) and the step in flight, or -. Left only
///                 when it stopped the container on that signal; tehasbot
///                 cycle makes the cap `signal` of it.
///   disk          `cap <held> <max>` or `floor <held> <free> <floor>`, in
///                 bytes, then the step in flight: tehas work stopped the
///                 cycle for its scratch's disk (src/scratch.rs).
pub const FILES: [&str; 16] = [
    "result.json",
    "progress.json",
    "runner_exit",
    "status",
    "nostart",
    "lean-refused",
    "refresh-conflict",
    "comments",
    "oom",
    "forks",
    "stalled",
    "timed",
    "signal",
    "disk",
    "stopped",
    "done",
];

/// What the start of the cycle put on record, as libexec/cycle holds it.
pub struct Start {
    pub issue: u64,
    pub attempt: u64,
    pub rc: i64,
    pub started: i64,
    pub signalled: bool,
    pub by: String,
    pub how: String,
    pub approval_at: String,
    pub requeued: bool,
    /// When the report began, as libexec/cycle read it; None reads the clock.
    pub now: Option<i64>,
    /// The cycle's console and status programs, for the report.
    pub console: String,
    pub status_cmd: String,
    pub status_args: Vec<String>,
}

/// A file as the facts hold it: its text, {"link": true}, or nothing.
///
/// A protocol file is read whole, and its size decides nothing. A file is
/// seen or not seen, as `[ -f ]` sees it: the result of a multi-issue run, with
/// the handoff bodies of each issue, is the cycle's own account of its end, and
/// reading it as absent would report a handoff as a cycle that wrote nothing.
/// The one file of a cycle with no ceiling is its run log, and that is read at
/// `runner::LOG_TAIL`.
fn file(p: &Path) -> Option<Value> {
    let m = fs::symlink_metadata(p).ok()?;
    if m.file_type().is_symlink() {
        return Some(json!({"link": true}));
    }
    if !m.is_file() {
        return None;
    }
    let b = fs::read(p).ok()?;
    Some(Value::String(String::from_utf8_lossy(&b).into_owned()))
}

/// A failed call of the store: its word, and its line for a log.
#[derive(Clone, Debug, PartialEq)]
pub struct Refused {
    pub word: String,
    pub line: String,
}

/// One verb of the state store, bounded and with its failure as
/// tehas_core::call keeps it.
pub fn store_value(home: &Path, args: &[&str]) -> Result<Value, Value> {
    let program = home.join("libexec/store");
    let program = program.to_string_lossy();
    if !call::store_executable(&program) {
        let line = format!("store: absent: {program} is not an executable file");
        return Err(call::store_failed(78, "absent", &line));
    }
    let extra = [("TEHAS_HOME", home.to_str().unwrap_or(""))];
    call::store(
        &program,
        args,
        Duration::from_secs(20),
        Duration::from_secs(2),
        &extra,
    )
}

fn refused(f: Value) -> Refused {
    let word = |k: &str| f.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    Refused {
        word: word("word"),
        line: word("line"),
    }
}

/// One verb of the state store, its JSON answer or the word of its failure.
pub fn store(home: &Path, args: &[&str]) -> Result<Value, String> {
    store_value(home, args).map_err(|f| refused(f).word)
}

/// One verb of the state store, the word and line of its failure.
pub fn store_said(home: &Path, args: &[&str]) -> Result<Value, Refused> {
    store_value(home, args).map_err(refused)
}

/// tehas_store_try: asked again, a second apart, while the store is busy or
/// did not answer in time.
pub fn store_try(home: &Path, tries: u32, args: &[&str]) -> Result<Value, Refused> {
    call::store_try(tries, Duration::from_secs(1), || store_value(home, args)).map_err(refused)
}

/// The facts of the cycle `s`, read from the host under `home`.
pub fn facts(home: &Path, s: &Start) -> Value {
    let env = |k: &str| std::env::var(k).unwrap_or_default();
    let cycles = home.join("state/cycles");
    let issue = s.issue;
    let mut files = Map::new();
    for name in FILES {
        if let Some(v) = file(&cycles.join(format!("{issue}.{name}"))) {
            files.insert(name.to_string(), v);
        }
    }
    let workdir = format!("{}-{issue}", env("TEHAS_WORK_DIR"));
    let parent = Path::new(&workdir)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let is_dir = |p: &str| fs::metadata(p).is_ok_and(|m| m.is_dir());
    let host = fs::read_to_string("/proc/sys/kernel/hostname")
        .unwrap_or_default()
        .trim()
        .to_string();
    let mut f = json!({
        "issue": issue, "repo": env("TEHAS_REPO"), "home": env("HOME"), "hostname": host,
        "workdir": workdir,
        "workspaces_marked": parent.join(".tehas-workspaces").is_file(),
        "workdir_dir": is_dir(&workdir),
        "before_retry_dir": is_dir(&format!("{workdir}.before-retry")),
        "rc": s.rc, "signalled": s.signalled, "started": s.started,
        "now": s.now.unwrap_or_else(tehas_core::time::now),
        "start": {"attempt": s.attempt, "by": s.by, "how": s.how,
                  "approval_at": s.approval_at.parse::<i64>().ok(), "requeued": s.requeued},
        "files": files,
    });
    if let Some(log) = newest_log(&home.join("logs"), issue) {
        f["run_log"] = run_log(&log);
    }
    let n = issue.to_string();
    f["record"] = store(home, &["cycle-show", &n]).unwrap_or_else(|w| json!({"error": w}));
    f["approval"] = store(home, &["approval-current", &n]).unwrap_or_else(|w| json!({"error": w}));
    f["history"] = store(home, &["history-show", &n]).unwrap_or_else(|w| json!({"error": w}));
    if let Some(state) =
        crate::last_attempt::state(Path::new(f["workdir"].as_str().unwrap_or("")), issue)
    {
        f["work_state"] = state;
    }
    let (id, _) = crate::report::record_image(&f["record"], s.attempt);
    if !id.is_empty() {
        f["image_built"] = json!(built(&id));
    }
    f
}

/// The newest run log as the facts hold it: its name and its text, of a log
/// longer than `LOG_TAIL` only its last `LOG_TAIL` bytes with `truncated`, or
/// its name and `unreadable` when it cannot be read.
fn run_log(p: &Path) -> Value {
    let mut v = json!({"name": p.to_string_lossy()});
    match runner::log_tail(p) {
        Some((bytes, whole)) => {
            v["text"] = json!(String::from_utf8_lossy(&bytes).into_owned());
            if !whole {
                v["truncated"] = json!(true);
            }
        }
        None => v["unreadable"] = json!(true),
    }
    v
}

/// When the image `id` was built, as docker prints .Created, or nothing.
fn built(id: &str) -> String {
    let out = Command::new("timeout")
        .args(["-k", "5", "30", "docker", "image", "inspect", "--format"])
        .args(["{{.Created}}", id])
        .output();
    match out {
        Ok(o) if o.status.success() => crate::facts::line(&String::from_utf8_lossy(&o.stdout)),
        _ => String::new(),
    }
}

/// The newest logs/work-<issue>-*.jsonl by its name, a regular file and no
/// link: tehas work names a log for the UTC time of its start.
fn newest_log(logs: &Path, issue: u64) -> Option<PathBuf> {
    let prefix = format!("work-{issue}-");
    fs::read_dir(logs)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            name.starts_with(&prefix)
                && name.ends_with(".jsonl")
                && fs::symlink_metadata(p).is_ok_and(|m| m.is_file())
        })
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::LOG_TAIL;
    use tehas_core::testdir::TestDir;

    /// At least `n` bytes of what a runner writes between its steps: one line,
    /// so the steps after it are still whole lines.
    fn written(n: usize) -> String {
        "y".repeat(n) + "\n"
    }

    #[test]
    fn a_protocol_file_is_held_whole_and_its_size_decides_nothing() {
        let d = TestDir::new("host-file-whole");
        let small = d.join("7.progress.json");
        fs::write(&small, "{\"roles\":[]}\n").unwrap();
        assert_eq!(file(&small), Some(Value::String("{\"roles\":[]}\n".into())));
        // The result of a multi-issue run with a long handoff body: bigger than
        // the run log tehas reads only its end of. It is the cycle's own
        // account of its end, not a file that is not there.
        let body = "y".repeat(1 << 20);
        let big = d.join("7.result.json");
        fs::write(&big, format!("{{\"handoffBody\":\"{body}\"}}\n")).unwrap();
        assert!(fs::metadata(&big).unwrap().len() > LOG_TAIL);
        let held = file(&big).expect("a big result is not held as an absent one");
        assert_eq!(
            held.as_str().unwrap(),
            format!("{{\"handoffBody\":\"{body}\"}}\n")
        );
        assert_eq!(file(&d.join("7.stopped")), None);
    }

    #[test]
    fn a_run_log_past_the_bound_is_held_as_its_end_and_says_truncated() {
        let d = TestDir::new("host-run-log-tail");
        let p = d.join("work-7-261101-093921.jsonl");
        // explore is past the end the facts hold; commit-pr and the branch of
        // the push are in it.
        fs::write(
            &p,
            format!(
                "{{\"v\":1,\"kind\":\"step-started\",\"event\":{{\"step\":\"explore\"}}}}\n{}{{\"v\":1,\"kind\":\"step-started\",\"event\":{{\"step\":\"commit-pr\"}}}}\n",
                written(LOG_TAIL as usize)
            ),
        )
        .unwrap();
        let v = run_log(&p);
        let held = v["text"].as_str().unwrap();
        assert_eq!(v["name"], p.to_string_lossy().as_ref());
        assert_eq!(v["truncated"], true);
        assert_eq!(held.len(), LOG_TAIL as usize);
        assert!(!held.contains("explore"), "the head was held");
        assert!(held.ends_with("\"step\":\"commit-pr\"}}\n"));
    }

    #[test]
    fn a_run_log_within_the_bound_is_held_whole_and_a_missing_one_says_unreadable() {
        let d = TestDir::new("host-run-log-whole");
        let p = d.join("work-7-261101-093921.jsonl");
        fs::write(&p, "{\"v\":1,\"kind\":\"log\",\"message\":\"a\"}\n").unwrap();
        let v = run_log(&p);
        assert_eq!(
            v["text"].as_str().unwrap(),
            "{\"v\":1,\"kind\":\"log\",\"message\":\"a\"}\n"
        );
        assert!(v.get("truncated").is_none());
        let gone = run_log(&d.join("work-7-261101-094123.jsonl"));
        assert_eq!(gone["unreadable"], true);
        assert!(gone.get("text").is_none());
    }
}
