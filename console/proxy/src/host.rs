//! The factory of the job channel on a host: the state store under
//! `TEHAS_HOME`, the narrowed cycle token `tehasbot github-token --cycle`
//! keeps, gh for a relayed write, and the files a cycle's report reads.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{json, Value};
use tehas_cycle::relay::{self, Scope, Via};

use crate::channel::{self, Factory};

pub struct OfHost {
    pub home: PathBuf,
    /// The repository the cycles of this factory write to.
    pub repo: String,
    /// The proxy's own address, which a job's models.json names; the
    /// contractor puts the address it joined with in its place.
    pub addr: String,
    pub timeout: i64,
    /// The names `TEHAS_OUTSOURCE_CONTRACTORS` lists.
    pub contractors: Vec<String>,
}

/// How long the token command may take.
const TOKEN_WAIT: &str = "60";

impl OfHost {
    fn cycles(&self) -> PathBuf {
        self.home.join("state").join("cycles")
    }
}

impl Factory for OfHost {
    fn store(&self, args: &[&str]) -> Option<Value> {
        let text = store::call(&self.home, args, &mut std::io::empty()).ok()?;
        serde_json::from_str(&text).ok()
    }

    fn cycle_token(&self) -> Option<(String, i64)> {
        let out = Command::new("timeout")
            .args(["-k", "5", TOKEN_WAIT])
            .arg(self.home.join("bin/tehasbot"))
            .args(["github-token", "--cycle"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success())?;
        let token = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let expires = fs::read_to_string(self.home.join("state/github-cycle/token.expires"))
            .ok()
            .and_then(|t| t.trim().parse().ok())
            .unwrap_or_default();
        (!token.is_empty()).then_some((token, expires))
    }

    fn relay(
        &self,
        args: &[String],
        input: Option<&str>,
        scope: &mut Scope,
    ) -> Result<(String, String, i32), String> {
        relay::answer_request(args, input, &self.repo, &Via::from_env(&self.home), scope)
    }

    fn keep(&self, issue: i64, result: &[u8], exit: i64) -> Result<(), String> {
        let dir = self.cycles();
        fs::create_dir_all(&dir).map_err(|e| format!("no cycles directory: {e}"))?;
        keep_file(&dir, &format!("{issue}.result.json"), result)?;
        keep_file(
            &dir,
            &format!("{issue}.runner_exit"),
            format!("{exit}\n").as_bytes(),
        )
    }

    fn carried(&self) -> Value {
        let names: Vec<String> = std::env::vars_os()
            .filter_map(|(k, _)| k.into_string().ok())
            .collect();
        let get = |n: &str| std::env::var(n).unwrap_or_default();
        let models = models_for_job(&self.addr);
        let (uid, gid) = account(&tehas_core::sandbox::user());
        json!({
            "settings": channel::settings_of(&names, &get),
            "models": models,
            "uid": uid,
            "gid": gid,
        })
    }

    fn timeout(&self) -> i64 {
        self.timeout
    }

    fn random(&self) -> Option<String> {
        crate::jobs::random()
    }

    fn allowed(&self, name: &str) -> bool {
        self.contractors.iter().any(|c| c == name)
    }
}

/// The uid and gid of `<uid>:<gid>`, the account this factory's containers run
/// as. Null for a number `id` did not give.
pub fn account(user: &str) -> (Value, Value) {
    let (uid, gid) = user.split_once(':').unwrap_or_default();
    let n = |w: &str| w.parse::<u32>().map_or(Value::Null, |n| json!(n));
    (n(uid), n(gid))
}

/// The instance's models.json with every provider behind the proxy at `addr`,
/// as a job's container is given it. Null when the instance's file cannot be
/// read: the job then carries no models and its cycle stops before Docker.
pub fn models_for_job(addr: &str) -> Value {
    let Ok(text) = fs::read_to_string(tehas_core::models::host_path()) else {
        return Value::Null;
    };
    let never = |_: &[String], _: &str| Err(tehas_core::models::NoAnswer::NotRun);
    let url = format!("http://{addr}/v1");
    tehas_core::models::resolved(&text, &[], &never, Some(&url))
        .and_then(|(doc, _)| serde_json::from_str(&doc.pretty()).ok())
        .unwrap_or(Value::Null)
}

/// Write `name` in `dir` whole, by a rename, so a reader sees all of it or none.
fn keep_file(dir: &Path, name: &str, bytes: &[u8]) -> Result<(), String> {
    let new = dir.join(format!(".{name}.new"));
    fs::write(&new, bytes).map_err(|e| format!("{name} could not be written: {e}"))?;
    fs::rename(&new, dir.join(name)).map_err(|e| format!("{name} could not be kept: {e}"))
}
