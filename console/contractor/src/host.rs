//! The contractor's world on a host: its tehas install under `TEHAS_HOME`, the
//! factory over the job channel, and the cycle run by the same `tehas work` a
//! factory runs.
//!
//! What one job leaves on the host, all of it wiped when the job ends and when
//! the program starts:
//!
//! | Path | What |
//! | --- | --- |
//! | `state/outsource/job.json` | the factory's address and the job's token (0600) |
//! | `state/outsource/instance/instance.env` | the job's settings (0600) |
//! | `state/github-cycle/token` | the cycle's token, as the container reads it (0600) |
//! | `~/.pi/agent/models.json` | the job's models.json, every provider behind the factory |
//! | `repos/<name>` | the product, cloned for the job |
//! | `work/`, `state/cycles/` | the cycle's workspace and notes |

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde_json::{json, Value};

use crate::http::{self, As, Reply};
use crate::identity::Identity;
use crate::job::{Job, Work, World};

/// How long one call of the channel may take.
const CALL_WAIT: Duration = Duration::from_secs(120);

/// The files of a contractor under its tehas home and its user's home.
#[derive(Debug, Clone)]
pub struct Paths {
    pub home: PathBuf,
    pub user_home: PathBuf,
}

impl Paths {
    pub fn outsource(&self) -> PathBuf {
        self.home.join("state/outsource")
    }
    pub fn job_file(&self) -> PathBuf {
        self.outsource().join("job.json")
    }
    pub fn instance(&self) -> PathBuf {
        self.outsource().join("instance")
    }
    pub fn drain_file(&self) -> PathBuf {
        self.outsource().join("drain")
    }
    pub fn token_dir(&self) -> PathBuf {
        self.home.join("state/github-cycle")
    }
    pub fn models(&self) -> PathBuf {
        self.user_home.join(".pi/agent/models.json")
    }
    pub fn cycles(&self) -> PathBuf {
        self.home.join("state/cycles")
    }

    /// Everything a job leaves, in the order it is removed.
    pub fn left_by_a_job(&self) -> Vec<PathBuf> {
        vec![
            self.job_file(),
            self.instance(),
            self.token_dir(),
            self.models(),
            self.home.join("repos"),
            self.home.join("work"),
            self.cycles(),
        ]
    }
}

/// A file made whole with mode 0600, by a rename.
pub fn write_private(file: &Path, bytes: &[u8]) -> Result<(), String> {
    let dir = file.parent().ok_or("a file with no directory")?;
    fs::create_dir_all(dir).map_err(|e| format!("cannot make {}: {e}", dir.display()))?;
    let new = file.with_extension("new");
    let _ = fs::remove_file(&new);
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&new)
        .map_err(|e| format!("cannot write {}: {e}", new.display()))?;
    out.write_all(bytes)
        .map_err(|e| format!("cannot write {}: {e}", new.display()))?;
    fs::rename(&new, file).map_err(|e| format!("cannot keep {}: {e}", file.display()))
}

/// One line of an instance file: the value in single quotes, as the load
/// reads a shell word.
fn env_line(name: &str, value: &str) -> String {
    format!("{name}='{}'\n", value.replace('\'', "'\\''"))
}

/// The instance file of a job: its settings, and the job's token under the
/// name each provider's models.json entry reads its key from.
pub fn instance_env(job: &Job) -> String {
    let mut text = String::new();
    for (name, value) in &job.settings {
        if let Some(v) = value.as_str() {
            text.push_str(&env_line(name, v));
        }
    }
    if let Some(providers) = job.models["providers"].as_object() {
        for name in providers.keys() {
            text.push_str(&env_line(
                &tehas_core::models::proxy_key_name(name),
                &job.token,
            ));
        }
    }
    text
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// git given a token by its environment, never by its command line.
fn git_with(token: &str) -> Command {
    let mut cmd = Command::new("timeout");
    cmd.args(["-k", "5", "600", "git"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "http.https://github.com/.extraheader")
        .env(
            "GIT_CONFIG_VALUE_0",
            format!(
                "AUTHORIZATION: basic {}",
                base64(format!("x-access-token:{token}").as_bytes())
            ),
        )
        .stdin(Stdio::null());
    cmd
}

/// The contractor's world on this host.
pub struct Host {
    pub id: Identity,
    pub paths: Paths,
    /// This program, which a cycle's renewer and relay call back.
    pub program: PathBuf,
    /// The command that runs a cycle, before the issue's number.
    pub work: Vec<String>,
    /// Where a product is cloned from: `<git_base>/<owner>/<name>.git`.
    pub git_base: String,
}

impl Host {
    /// The repository's name, the directory its checkout has under `repos`.
    fn product(job: &Job) -> Option<(String, String)> {
        let repo = job.settings.get("TEHAS_REPO")?.as_str()?.to_string();
        let name = repo.split_once('/')?.1.to_string();
        Some((repo, name))
    }

    fn prepare(&self, job: &Job) -> Result<(), String> {
        let p = &self.paths;
        let job_file = json!({ "factory": self.id.factory, "token": job.token, "job": job.id });
        write_private(&p.job_file(), job_file.to_string().as_bytes())?;
        let _ = fs::set_permissions(p.outsource(), fs::Permissions::from_mode(0o700));
        write_private(
            &p.instance().join("instance.env"),
            instance_env(job).as_bytes(),
        )?;
        let models = serde_json::to_string_pretty(&job.models).unwrap_or_default();
        write_private(&p.models(), format!("{models}\n").as_bytes())?;
        let (repo, name) = Host::product(job).ok_or("the job names no TEHAS_REPO")?;
        let token = fetch_token(&self.id.factory, &job.token, &p.token_dir())?;
        let into = p.home.join("repos").join(&name);
        let cloned = git_with(&token)
            .args(["clone", "--quiet", &format!("{}/{repo}.git", self.git_base)])
            .arg(&into)
            .stdout(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if !cloned {
            return Err(format!("{repo} could not be cloned"));
        }
        Ok(())
    }
}

/// Ask the factory for the cycle's token, write it where the container reads
/// it, and give it back.
pub fn fetch_token(factory: &str, job_token: &str, dir: &Path) -> Result<String, String> {
    match http::post(factory, "token", As::Job(job_token), &json!({}), CALL_WAIT) {
        Reply::Ok(v) => {
            let token = v["token"]
                .as_str()
                .filter(|t| !t.is_empty())
                .ok_or("the factory answered no token")?;
            let expires = v["expires"].as_i64().unwrap_or_default();
            fs::create_dir_all(dir).map_err(|e| format!("cannot make {}: {e}", dir.display()))?;
            let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
            write_private(
                &dir.join("token.expires"),
                format!("{expires}\n").as_bytes(),
            )?;
            write_private(&dir.join("token"), format!("{token}\n").as_bytes())?;
            Ok(token.to_string())
        }
        Reply::No(_, word) => Err(format!("the factory refused a token: {word}")),
        Reply::Down(why) => Err(format!("the factory did not answer: {why}")),
    }
}

/// The running job's address and token, as this host keeps them for the
/// renewer and the relay of its cycle.
pub fn job_of(paths: &Paths) -> Option<(String, String)> {
    let v: Value = serde_json::from_slice(&fs::read(paths.job_file()).ok()?).ok()?;
    Some((
        v["factory"].as_str()?.to_string(),
        v["token"].as_str()?.to_string(),
    ))
}

struct HostWork {
    child: Child,
    cid: PathBuf,
}

impl Work for HostWork {
    fn done(&mut self) -> Option<i32> {
        self.child
            .try_wait()
            .ok()
            .flatten()
            .map(|s| s.code().unwrap_or(137))
    }
    fn stop(&mut self) {
        if let Some(pid) = rustix::process::Pid::from_raw(self.child.id() as i32) {
            let _ = rustix::process::kill_process(pid, rustix::process::Signal::TERM);
        }
    }
    fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        // The container outlives a killed tehas work: it is stopped here.
        if let Ok(cid) = fs::read_to_string(&self.cid) {
            let _ = Command::new("timeout")
                .args(["-k", "5", "60", "docker", "stop", cid.trim()])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .status();
        }
    }
}

impl World for Host {
    fn ask(&self, call: &str) -> Reply {
        let who = As::Contractor {
            name: &self.id.name,
            key: &self.id.key,
        };
        http::post(&self.id.factory, call, who, &json!({}), CALL_WAIT)
    }

    fn call(&self, call: &str, token: &str, body: &Value) -> Reply {
        http::post(&self.id.factory, call, As::Job(token), body, CALL_WAIT)
    }

    fn start(&self, job: &Job) -> Result<Box<dyn Work>, String> {
        self.wipe();
        self.prepare(job)?;
        let (first, rest) = self.work.split_first().ok_or("no work command")?;
        let child = Command::new(first)
            .args(rest)
            .arg(job.issue.to_string())
            .env("TEHAS_HOME", &self.paths.home)
            .env("TEHAS_INSTANCE_DIR", self.paths.instance())
            .env("TEHAS_CONTRACTOR", &self.program)
            .env("HOME", &self.paths.user_home)
            .stdin(Stdio::null())
            .spawn()
            .map_err(|e| format!("{first} could not start: {e}"))?;
        Ok(Box::new(HostWork {
            child,
            cid: self.paths.cycles().join(format!("{}.cid", job.issue)),
        }))
    }

    fn result(&self, job: &Job) -> Option<(Value, i64)> {
        let dir = self.paths.cycles();
        let result: Value =
            serde_json::from_slice(&fs::read(dir.join(format!("{}.result.json", job.issue))).ok()?)
                .ok()?;
        let exit = fs::read_to_string(dir.join(format!("{}.runner_exit", job.issue)))
            .ok()
            .and_then(|t| t.trim().parse().ok())
            .unwrap_or(1);
        Some((result, exit))
    }

    fn salvage(&self, job: &Job) -> bool {
        let Some((_, name)) = Host::product(job) else {
            return false;
        };
        let ws = self
            .paths
            .home
            .join("work")
            .join(format!("{name}-{}", job.issue));
        let branch = Command::new("git")
            .arg("-C")
            .arg(&ws)
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        let base = job
            .settings
            .get("TEHAS_BRANCH")
            .and_then(Value::as_str)
            .unwrap_or("main");
        // Nothing of the cycle's own to push: no branch of its making.
        if branch.is_empty() || branch == "HEAD" || branch == base {
            return true;
        }
        let Ok(token) = fs::read_to_string(self.paths.token_dir().join("token")) else {
            return false;
        };
        git_with(token.trim())
            .arg("-C")
            .arg(&ws)
            .args(["push", "--quiet", "origin"])
            .arg(format!("HEAD:refs/heads/{branch}"))
            .stdout(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    fn wipe(&self) {
        for path in self.paths.left_by_a_job() {
            let _ = if path.is_dir() {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            };
        }
    }

    fn draining(&self) -> bool {
        self.paths.drain_file().exists()
    }

    fn now(&self) -> u64 {
        u64::try_from(tehas_core::time::now()).unwrap_or_default()
    }

    fn pause(&self, seconds: u64) {
        std::thread::sleep(Duration::from_secs(seconds));
    }

    fn say(&self, line: &str) {
        eprintln!("tehas outsource contractor: {line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_is_the_standard_alphabet_with_padding() {
        assert_eq!(base64(b"x-access-token:t"), "eC1hY2Nlc3MtdG9rZW46dA==");
        assert_eq!(base64(b"abc"), "YWJj");
        assert_eq!(base64(b"ab"), "YWI=");
    }

    #[test]
    fn a_job_s_instance_file_holds_its_settings_and_its_token_under_each_provider() {
        let job = Job::of(&json!({
            "job": 1, "issue": 7, "attempt": 1, "timeout": 60, "token": "tok",
            "settings": { "TEHAS_ALLOWED_LABELERS": "alice bob", "TEHAS_REPO": "acme/it's" },
            "models": { "providers": { "main-gw": {}, "other": {} } }
        }))
        .unwrap();
        let text = instance_env(&job);
        assert!(text.contains("TEHAS_ALLOWED_LABELERS='alice bob'\n"));
        assert!(text.contains("TEHAS_REPO='acme/it'\\''s'\n"));
        assert!(text.contains("PI_ENSEMBLE_API_KEY_MAIN_GW='tok'\n"));
        assert!(text.contains("PI_ENSEMBLE_API_KEY_OTHER='tok'\n"));
    }
}
