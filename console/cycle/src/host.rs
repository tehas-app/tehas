//! The facts of an ended cycle read from the host, as libexec/cycle reads
//! them for its report: the files of the protocol, the newest run log, the
//! cycle's record and its approval on the state store, the workspace, and
//! when docker says the image on the record was built.
//! And GitHub's two answers, asked with gh as the bash asks them, once each.

use crate::report::Ask;
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The files of the protocol a report reads, state/cycles/<issue>.<name>.
pub const FILES: [&str; 11] = [
    "result.json",
    "progress.json",
    "runner_exit",
    "status",
    "nostart",
    "oom",
    "forks",
    "stalled",
    "timed",
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
}

/// A file as the facts hold it: its text, {"link": true}, or nothing.
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

/// One verb of the state store, its JSON answer or the word of its failure.
pub fn store(home: &Path, args: &[&str]) -> Result<Value, String> {
    let out = Command::new("timeout")
        .args(["-k", "2", "20"])
        .arg(home.join("libexec/store"))
        .args(args)
        .env("TEHAS_HOME", home)
        .output()
        .map_err(|_| "absent".to_string())?;
    if !out.status.success() {
        let said = String::from_utf8_lossy(&out.stderr);
        let word = said
            .split_whitespace()
            .nth(1)
            .unwrap_or("unknown")
            .trim_end_matches(':')
            .to_string();
        return Err(word);
    }
    serde_json::from_slice(&out.stdout).map_err(|_| "unreadable".to_string())
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
        "now": tehas_core::time::now(),
        "start": {"attempt": s.attempt, "by": s.by, "how": s.how,
                  "approval_at": s.approval_at.parse::<i64>().ok(), "requeued": s.requeued},
        "files": files,
    });
    if let Some(log) = newest_log(&home.join("logs"), issue) {
        let text = fs::read(&log)
            .ok()
            .map(|b| String::from_utf8_lossy(&b).into_owned());
        f["run_log"] = json!({"name": log.to_string_lossy(), "text": text});
    }
    let n = issue.to_string();
    f["record"] = store(home, &["cycle-show", &n]).unwrap_or_else(|w| json!({"error": w}));
    f["approval"] = store(home, &["approval-current", &n]).unwrap_or_else(|w| json!({"error": w}));
    let (id, _) = crate::report::record_image(&f["record"], s.attempt);
    if !id.is_empty() {
        f["image_built"] = json!(built(&id));
    }
    f
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

/// GitHub asked with gh, once each, as the shadow asks it.
pub struct Live {
    pub repo: String,
    pub issue: u64,
    /// What GitHub answered, kept for the record of the shadow.
    pub said: Map<String, Value>,
}

fn gh(args: &[&str]) -> Option<Vec<u8>> {
    let out = Command::new("timeout")
        .args(["-k", "5", "20", "gh"])
        .args(args)
        .output()
        .ok()?;
    out.status.success().then_some(out.stdout)
}

impl Ask for Live {
    fn prs(&mut self, branch: &str) -> Option<Vec<Value>> {
        let got = gh(&[
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
        ])
        .and_then(|o| serde_json::from_slice::<Value>(&o).ok())
        .and_then(|v| v.as_array().cloned());
        self.said.insert(
            "prs".into(),
            got.clone().map(Value::Array).unwrap_or(Value::Null),
        );
        got
    }

    fn attention(&mut self) -> Option<bool> {
        let got = gh(&["api", &format!("repos/{}/issues/{}", self.repo, self.issue)])
            .and_then(|o| serde_json::from_slice::<Value>(&o).ok())
            .filter(Value::is_object)
            .map(|v| {
                v.get("labels").and_then(Value::as_array).is_some_and(|l| {
                    l.iter().any(|x| {
                        x.get("name").and_then(Value::as_str) == Some("needs-human-attention")
                    })
                })
            });
        self.said.insert(
            "attention".into(),
            got.map(Value::Bool).unwrap_or(Value::Null),
        );
        got
    }
}
