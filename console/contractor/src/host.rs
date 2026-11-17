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
//! | `state/outsource/salvage/` | the repository a salvage push is made from |
//! | `state/github-cycle/token` | the cycle's token, as the container reads it (0600) |
//! | `~/.pi/agent/models.json` | the job's models.json, every provider behind the factory |
//! | `repos/<name>` | the product, cloned for the job |
//! | `work/`, `state/cycles/` | the cycle's workspace and notes |
//!
//! The wipe removes whole directories a factory install keeps too, so the
//! program refuses a home that holds a factory ([`foreign`]), and removes a
//! `models.json` only when it wrote it.
//!
//! No git runs in the workspace: the cycle's container wrote all of it, its
//! hooks and `.git/config` included. A salvage push fetches the issue's branch
//! out of it into a repository of the contractor's own and pushes from there,
//! to the repository's address and never to one the workspace names.

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

/// Where the salvage repository keeps the workspace's branches.
const KEPT: &str = "refs/kept/heads/";

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
    /// Left beside the job's files when this program wrote `models.json`.
    pub fn models_made(&self) -> PathBuf {
        self.outsource().join("models.made")
    }
    pub fn salvage(&self) -> PathBuf {
        self.outsource().join("salvage")
    }
    pub fn cycles(&self) -> PathBuf {
        self.home.join("state/cycles")
    }

    /// Everything a job leaves, in the order it is removed. `models.json` is
    /// not on it: it goes only with [`Paths::models_made`].
    pub fn left_by_a_job(&self) -> Vec<PathBuf> {
        vec![
            self.job_file(),
            self.instance(),
            self.salvage(),
            self.token_dir(),
            self.home.join("repos"),
            self.home.join("work"),
            self.cycles(),
        ]
    }
}

/// Why this host is no place to run jobs, or None. The wipe removes
/// `repos`, `work` and `state/cycles`, which a factory's install keeps too, so
/// a home that holds a factory is refused before anything is removed, and so
/// is a `models.json` this program did not write.
pub fn foreign(paths: &Paths) -> Option<String> {
    let there = |p: &Path| fs::symlink_metadata(p).is_ok();
    let config = paths.user_home.join(".config/tehas");
    let factory = [
        (paths.home.join("state/tehas.db"), "a factory's state store"),
        (paths.home.join("settings"), "a factory's settings checkout"),
        (config.join("instance-dir"), "a factory's instance pointer"),
        (
            config.join("instance.env"),
            "a factory's instance directory",
        ),
    ];
    if let Some((path, what)) = factory.iter().find(|(p, _)| there(p)) {
        return Some(format!(
            "{} is {what}: a contractor runs on a host of its own, never beside a factory",
            path.display()
        ));
    }
    if there(&paths.models()) && !there(&paths.models_made()) {
        return Some(format!(
            "{} is not this program's: a contractor holds no models of its own; move it away",
            paths.models().display()
        ));
    }
    None
}

/// A job's models.json with every provider's `baseUrl`, and a model's own
/// where it has one, at the factory's address this host joined with: the
/// address the proxy binds may be one only the factory can reach.
pub fn for_factory(models: &Value, factory: &str) -> Value {
    let url = format!("http://{}/v1", http::host_port(factory));
    let mut out = models.clone();
    let set = |o: &mut Value| {
        if o.get("baseUrl").is_some_and(Value::is_string) {
            o["baseUrl"] = Value::String(url.clone());
        }
    };
    if let Some(providers) = out.get_mut("providers").and_then(Value::as_object_mut) {
        for provider in providers.values_mut() {
            set(provider);
            if let Some(list) = provider.get_mut("models").and_then(Value::as_array_mut) {
                list.iter_mut().for_each(set);
            }
        }
    }
    out
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

/// git with no hooks, no fsmonitor and no system config, bounded in time. Its
/// calls are made in the contractor's own repositories; the pins are a second
/// guard.
fn git_pinned() -> Command {
    let mut cmd = Command::new("timeout");
    cmd.args(["-k", "5", "600", "git"])
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
        ])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null());
    cmd
}

/// git given a token by its environment, never by its command line.
fn git_with(token: &str) -> Command {
    let mut cmd = git_pinned();
    cmd.env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "http.https://github.com/.extraheader")
        .env(
            "GIT_CONFIG_VALUE_0",
            format!(
                "AUTHORIZATION: basic {}",
                base64(format!("x-access-token:{token}").as_bytes())
            ),
        );
    cmd
}

fn ran(cmd: &mut Command) -> bool {
    cmd.stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
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
        let models = for_factory(&job.models, &self.id.factory);
        let models = serde_json::to_string_pretty(&models).unwrap_or_default();
        write_private(&p.models_made(), b"")?;
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
        if let Some(why) = foreign(&self.paths) {
            return Err(why);
        }
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
        let Some((repo, name)) = Host::product(job) else {
            return false;
        };
        let ws = self
            .paths
            .home
            .join("work")
            .join(format!("{name}-{}", job.issue));
        if !ws.is_dir() {
            return true;
        }
        let own = self.paths.salvage();
        let _ = fs::remove_dir_all(&own);
        if fs::create_dir_all(&own).is_err()
            || !ran(git_pinned().arg("-C").arg(&own).args(["init", "--quiet"]))
        {
            return false;
        }
        // The serving side of this fetch is upload-pack, which git keeps safe
        // on a repository nobody trusts; nothing else runs in the workspace.
        let fetched = ran(git_pinned()
            .arg("-C")
            .arg(&own)
            .args(["fetch", "--quiet", "--no-tags"])
            .arg(&ws)
            .arg(format!("+refs/heads/*:{KEPT}*")));
        if !fetched {
            return false;
        }
        let names = git_pinned()
            .arg("-C")
            .arg(&own)
            .args(["for-each-ref", "--format=%(refname:lstrip=3)", KEPT])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default();
        let issue = u64::try_from(job.issue).unwrap_or_default();
        // Nothing of the cycle's own to push: no branch of the issue.
        let Some(branch) = tehas_cycle::finish::run::issue_branch_of(&names, issue) else {
            return true;
        };
        let Ok(token) = fs::read_to_string(self.paths.token_dir().join("token")) else {
            return false;
        };
        ran(git_with(token.trim())
            .arg("-C")
            .arg(&own)
            .args(["push", "--quiet", "--no-verify"])
            .arg(format!("{}/{repo}.git", self.git_base))
            .arg(format!("{KEPT}{branch}:refs/heads/{branch}")))
    }

    fn wipe(&self) {
        if fs::symlink_metadata(self.paths.models_made()).is_ok() {
            let _ = fs::remove_file(self.paths.models());
            let _ = fs::remove_file(self.paths.models_made());
        }
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
    use tehas_core::testdir::TestDir;

    fn paths_in(d: &TestDir) -> Paths {
        let user_home = d.path().join("user");
        Paths {
            home: user_home.join("tehas"),
            user_home,
        }
    }

    fn host_in(d: &TestDir, git_base: String) -> Host {
        Host {
            id: Identity {
                factory: "192.0.2.7:8090".to_string(),
                name: "box1".to_string(),
                key: "k".to_string(),
            },
            paths: paths_in(d),
            program: PathBuf::from("/nonexistent"),
            work: vec![],
            git_base,
        }
    }

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(ok.status.success(), "git {args:?}: {ok:?}");
    }

    fn heads(bare: &Path) -> String {
        let o = Command::new("git")
            .arg("-C")
            .arg(bare)
            .args(["for-each-ref", "--format=%(refname)", "refs/heads/"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&o.stdout).into_owned()
    }

    #[test]
    fn a_home_that_holds_a_factory_is_refused_and_left_as_it_was() {
        for marker in [
            "tehas/state/tehas.db",
            "tehas/settings",
            ".config/tehas/instance-dir",
            ".config/tehas/instance.env",
        ] {
            let d = TestDir::new("contractor-foreign");
            let p = paths_in(&d);
            let file = p.user_home.join(marker);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(&file, "x").unwrap();
            fs::create_dir_all(p.home.join("repos/widget")).unwrap();
            let why = foreign(&p).unwrap_or_default();
            assert!(why.contains("never beside a factory"), "{marker}: {why}");
            let host = host_in(&d, String::new());
            let job = Job::of(&json!({
                "job": 1, "issue": 7, "attempt": 1, "timeout": 60, "token": "t",
                "settings": { "TEHAS_REPO": "acme/widget" }
            }))
            .unwrap();
            assert!(host.start(&job).is_err());
            assert!(p.home.join("repos/widget").is_dir(), "{marker}");
            assert!(file.exists(), "{marker}");
        }
        let d = TestDir::new("contractor-foreign-clean");
        assert_eq!(foreign(&paths_in(&d)), None);
    }

    #[test]
    fn a_models_json_this_program_did_not_write_is_refused_and_never_wiped() {
        let d = TestDir::new("contractor-models");
        let p = paths_in(&d);
        fs::create_dir_all(p.models().parent().unwrap()).unwrap();
        fs::write(p.models(), "{}").unwrap();
        assert!(foreign(&p)
            .unwrap_or_default()
            .contains("not this program's"));
        let host = host_in(&d, String::new());
        host.wipe();
        assert!(p.models().exists());
        // Written by a job, it is the job's, and the wipe takes it.
        write_private(&p.models_made(), b"").unwrap();
        assert_eq!(foreign(&p), None);
        host.wipe();
        assert!(!p.models().exists() && !p.models_made().exists());
    }

    #[test]
    fn a_job_s_models_point_at_the_factory_address_this_host_joined_with() {
        let models = json!({ "providers": {
            "main": { "baseUrl": "http://127.0.0.1:8090/v1", "apiKey": "${K}",
                      "models": [ { "id": "a", "baseUrl": "http://0.0.0.0:8090/v1" }, { "id": "b" } ] },
            "other": { "api": "x" }
        }});
        let got = for_factory(&models, "http://192.0.2.7:8090");
        let main = &got["providers"]["main"];
        assert_eq!(main["baseUrl"], "http://192.0.2.7:8090/v1");
        assert_eq!(main["models"][0]["baseUrl"], "http://192.0.2.7:8090/v1");
        assert!(main["models"][1].get("baseUrl").is_none());
        assert_eq!(main["apiKey"], "${K}");
        assert!(got["providers"]["other"].get("baseUrl").is_none());
    }

    #[test]
    fn a_salvage_push_runs_no_git_in_the_workspace_and_goes_to_the_repository() {
        let d = TestDir::new("contractor-salvage");
        let git_root = d.path().join("git");
        let origin = git_root.join("acme/widget.git");
        let decoy = d.path().join("decoy.git");
        for bare in [&origin, &decoy] {
            fs::create_dir_all(bare).unwrap();
            git(bare, &["init", "--quiet", "--bare", "-b", "main"]);
        }
        let host = host_in(&d, format!("file://{}", git_root.display()));
        let p = &host.paths;
        // The workspace as a hostile cycle leaves it: a hook in .git/hooks and
        // in a hooks path of its own, a push URL of its own.
        let ws = p.home.join("work/widget-7");
        fs::create_dir_all(&ws).unwrap();
        git(&ws, &["init", "--quiet", "-b", "main"]);
        fs::write(ws.join("README"), "x\n").unwrap();
        git(&ws, &["add", "README"]);
        git(
            &ws,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.invalid",
                "commit",
                "--quiet",
                "-m",
                "x",
            ],
        );
        git(
            &ws,
            &["checkout", "--quiet", "-b", "feature/issue-7-a-change"],
        );
        git(&ws, &["branch", "other"]);
        let ran = d.path().join("hook-ran");
        let hook = format!("#!/bin/sh\ntouch {}\n", ran.display());
        for dir in [ws.join(".git/hooks"), ws.join("myhooks")] {
            fs::create_dir_all(&dir).unwrap();
            for name in ["pre-push", "post-checkout", "reference-transaction"] {
                tehas_core::testing::write_exe(dir.join(name), &hook).unwrap();
            }
        }
        git(
            &ws,
            &[
                "config",
                "core.hooksPath",
                ws.join("myhooks").to_str().unwrap(),
            ],
        );
        git(&ws, &["remote", "add", "origin", decoy.to_str().unwrap()]);
        git(
            &ws,
            &["config", "remote.origin.pushurl", decoy.to_str().unwrap()],
        );
        fs::remove_file(&ran).ok();
        fs::create_dir_all(p.token_dir()).unwrap();
        fs::write(p.token_dir().join("token"), "t\n").unwrap();
        let job = Job::of(&json!({
            "job": 1, "issue": 7, "attempt": 1, "timeout": 60, "token": "t",
            "settings": { "TEHAS_REPO": "acme/widget" }
        }))
        .unwrap();
        assert!(host.salvage(&job));
        assert!(!ran.exists(), "a hook of the workspace ran");
        assert_eq!(heads(&origin), "refs/heads/feature/issue-7-a-change\n");
        assert_eq!(heads(&decoy), "");
        // A workspace with no branch of the issue has nothing to push.
        git(&ws, &["checkout", "--quiet", "main"]);
        git(&ws, &["branch", "-D", "feature/issue-7-a-change"]);
        fs::remove_file(&ran).ok();
        assert!(host.salvage(&Job {
            issue: 8,
            ..job.clone()
        }));
        assert!(!ran.exists());
        assert_eq!(heads(&origin), "refs/heads/feature/issue-7-a-change\n");
    }

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
