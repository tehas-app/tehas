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
//! The clone and the salvage push go through the job's local end of the
//! factory's egress route ([`crate::egress`]): the factory's listener is this
//! program's one connection for a job.
//!
//! The job names the factory's account. Before the cycle's container starts,
//! `tehas work` asks this program (`own`) to hand the folders the container
//! writes to that account: the workspace, the code index cache, the relay
//! folder and the rest of the cycle's writable mounts. Writable is not
//! enough: the code index refuses a cache folder it does not own. They are
//! handed over only when the container runs as that account, which `tehas
//! work` names with `--as`; otherwise they stay with the account it runs as.
//! Only folders under `work/` and the agents' runs folder are handed over,
//! and a link is never followed.
//!
//! No git runs in the workspace: the cycle's container wrote all of it, its
//! hooks and `.git/config` included. A salvage push fetches the issue's branch
//! out of it into a repository of the contractor's own and pushes from there,
//! to the repository's address and never to one the workspace names.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{lchown, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde_json::{json, Value};

use crate::egress::Local;
use crate::http::{self, As, Reply};
use crate::identity::Identity;
use crate::job::{Job, Owner, Work, World};

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
    /// Where the cycle keeps its agents' runs, `ensemble-runs/issue-<n>`.
    pub fn runs(&self) -> PathBuf {
        self.user_home.join(".pi/agent/ensemble-runs")
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

/// The instance file of a job: its settings, and the job's model token under
/// the name each provider's models.json entry reads its key from. The job
/// token is never written here: the container gets the model scope only.
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
                &job.model_token,
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

/// git given a token by its environment, never by its command line, and
/// sent through `proxy`, the job's local end of the factory's egress route
/// ([`crate::egress`]). No proxy variable of this user's and no `NO_PROXY`
/// takes it around that.
pub fn git_with(token: &str, proxy: &str) -> Command {
    let mut cmd = git_pinned();
    for name in [
        "NO_PROXY",
        "no_proxy",
        "ALL_PROXY",
        "all_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
    ] {
        cmd.env_remove(name);
    }
    cmd.env("GIT_CONFIG_COUNT", "2")
        .env("GIT_CONFIG_KEY_0", "http.https://github.com/.extraheader")
        .env(
            "GIT_CONFIG_VALUE_0",
            format!(
                "AUTHORIZATION: basic {}",
                base64(format!("x-access-token:{token}").as_bytes())
            ),
        )
        .env("GIT_CONFIG_KEY_1", "http.proxy")
        .env("GIT_CONFIG_VALUE_1", proxy);
    cmd
}

/// The job's local end of the egress route, or why there is none.
fn local_end(factory: &str, job: &Job) -> Result<Local, String> {
    Local::start(factory, &job.model_token)
        .map_err(|e| format!("the job's way out could not start: {e}"))
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
        let kept = job_file(&self.id.factory, job);
        write_private(&p.job_file(), kept.to_string().as_bytes())?;
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
        let local = local_end(&self.id.factory, job)?;
        let cloned = git_with(&token, &local.url())
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

/// What the job file keeps for the cycle's calls back: the factory's address,
/// the job's token and the account its folders are handed to.
pub fn job_file(factory: &str, job: &Job) -> Value {
    let mut v = json!({ "factory": factory, "token": job.token, "job": job.id });
    if let Some(o) = job.owner {
        v["uid"] = json!(o.uid);
        v["gid"] = json!(o.gid);
    }
    v
}

/// The account the running job's folders are handed to: Ok(None) for a job
/// that names none, Err when no job is running here.
pub fn owner_of(paths: &Paths) -> Result<Option<Owner>, String> {
    let text = fs::read(paths.job_file()).map_err(|_| "no job is running here".to_string())?;
    let v: Value =
        serde_json::from_slice(&text).map_err(|_| "the job file is not JSON".to_string())?;
    Ok(Owner::of(&v))
}

/// The folders of `asked` that are handed over, each as its real path: a
/// directory, not a link, inside the cycle's workspaces or its agents' runs
/// and not that root itself. Err names the first that is not.
pub fn ownership_plan(paths: &Paths, asked: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let roots: Vec<PathBuf> = [paths.home.join("work"), paths.runs()]
        .iter()
        .filter_map(|r| fs::canonicalize(r).ok())
        .collect();
    asked
        .iter()
        .map(|dir| {
            let refused = |why: &str| format!("{} is not handed over: {why}", dir.display());
            let meta = fs::symlink_metadata(dir).map_err(|_| refused("it is not there"))?;
            if !meta.is_dir() {
                return Err(refused("it is no folder"));
            }
            let real = fs::canonicalize(dir).map_err(|_| refused("it is not there"))?;
            if roots.iter().any(|r| real.starts_with(r) && &real != r) {
                Ok(real)
            } else {
                Err(refused("it is not a folder of the cycle"))
            }
        })
        .collect()
}

/// `dir` and everything in it to `owner`. A link is changed itself, never
/// what it points at, and is not followed into.
pub fn hand_over(dir: &Path, owner: Owner) -> Result<(), String> {
    lchown(dir, Some(owner.uid), Some(owner.gid))
        .map_err(|e| format!("cannot hand {} over: {e}", dir.display()))?;
    let meta = fs::symlink_metadata(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    if meta.is_dir() {
        let entries = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
            hand_over(&entry.path(), owner)?;
        }
    }
    Ok(())
}

/// What `tehas-contractor own` did.
#[derive(Debug, PartialEq, Eq)]
pub enum Handed {
    /// The folders now belong to this account.
    To(Owner),
    /// The job names no account: the folders stay as they are.
    Unnamed,
    /// The container runs as another account than the job names: the folders
    /// stay as they are, so the container still owns what it writes.
    Elsewhere { job: Owner, container: Owner },
}

/// What `tehas-contractor own` does: the running job's folders `asked` to the
/// account the job names, when the container runs as that account. A folder
/// outside the cycle is refused before anything is changed; a file that
/// cannot be handed over stops it there, and what was done stays done.
pub fn own(paths: &Paths, container: Owner, asked: &[PathBuf]) -> Result<Handed, String> {
    let owner = owner_of(paths)?;
    let plan = ownership_plan(paths, asked)?;
    let Some(owner) = owner else {
        return Ok(Handed::Unnamed);
    };
    if owner != container {
        return Ok(Handed::Elsewhere {
            job: owner,
            container,
        });
    }
    for dir in &plan {
        hand_over(dir, owner)?;
    }
    Ok(Handed::To(owner))
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

/// How long the image's labels may take to read.
const INSPECT_WAIT: &str = "30";

/// What this box says it has with each hello: the id and the pi-rukas label
/// of the sandbox image its cycles run (PI_ENSEMBLE_IMAGE, as `tehas work`
/// reads it), its free memory in MiB and the free disk of its tehas home in
/// KiB. What cannot be read is left out, and the factory fits no cycle to a
/// box that left out what a cycle needs.
pub fn report(home: &Path, image: &str) -> Value {
    let mut out = serde_json::Map::new();
    if !image.is_empty() {
        let inspected = Command::new("timeout")
            .args([
                "-k",
                "5",
                INSPECT_WAIT,
                "docker",
                "image",
                "inspect",
                "--format",
            ])
            .arg("{{.Id}} {{index .Config.Labels \"tehas.rukas\"}}")
            .arg(image)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success());
        if let Some(o) = inspected {
            let (id, rukas) = image_of(&String::from_utf8_lossy(&o.stdout));
            out.extend(id.map(|v| ("image".to_string(), json!(v))));
            out.extend(rukas.map(|v| ("rukas".to_string(), json!(v))));
        }
    }
    if let Some(mb) = fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|t| mem_mb(&t))
    {
        out.insert("mem_mb".to_string(), json!(mb));
    }
    let df = Command::new("df")
        .arg("-Pk")
        .arg(home)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success());
    if let Some(kb) = df.and_then(|o| disk_kb(&String::from_utf8_lossy(&o.stdout))) {
        out.insert("disk_kb".to_string(), json!(kb));
    }
    Value::Object(out)
}

/// The id and the `tehas.rukas` label of `docker image inspect`'s one line.
/// A label docker has no value for prints `<no value>`, which is none.
pub fn image_of(text: &str) -> (Option<String>, Option<String>) {
    let mut words = text.split_whitespace();
    let id = words.next().filter(|w| w.starts_with("sha256:"));
    let rukas = words.next().filter(|w| !w.starts_with('<'));
    (id.map(str::to_string), rukas.map(str::to_string))
}

/// MemAvailable of /proc/meminfo, in MiB.
pub fn mem_mb(meminfo: &str) -> Option<u64> {
    meminfo.lines().find_map(|l| {
        let kb = l.strip_prefix("MemAvailable:")?.trim().strip_suffix("kB")?;
        kb.trim().parse::<u64>().ok().map(|kb| kb / 1024)
    })
}

/// The available KiB of `df -Pk`'s one file system.
pub fn disk_kb(df: &str) -> Option<u64> {
    df.lines().nth(1)?.split_whitespace().nth(3)?.parse().ok()
}

impl World for Host {
    fn ask(&self, call: &str) -> Reply {
        let who = As::Contractor {
            name: &self.id.name,
            key: &self.id.key,
        };
        let body = if call == "hello" {
            let image = std::env::var("PI_ENSEMBLE_IMAGE").unwrap_or_default();
            json!({ "report": report(&self.paths.home, &image) })
        } else {
            json!({})
        };
        http::post(&self.id.factory, call, who, &body, CALL_WAIT)
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
        let Ok(local) = local_end(&self.id.factory, job) else {
            return false;
        };
        ran(git_with(token.trim(), &local.url())
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
                "job": 1, "issue": 7, "attempt": 1, "timeout": 60, "token": "t", "model_token": "m",
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
            "job": 1, "issue": 7, "attempt": 1, "timeout": 60, "token": "t", "model_token": "m",
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

    fn job_with(owner: Option<Owner>) -> Job {
        let mut job = Job::of(&json!({
            "job": 4, "issue": 7, "attempt": 1, "timeout": 60, "token": "t", "model_token": "m"
        }))
        .unwrap();
        job.owner = owner;
        job
    }

    fn me(d: &TestDir) -> Owner {
        use std::os::unix::fs::MetadataExt;
        let f = d.path().join("me");
        fs::write(&f, "").unwrap();
        let m = fs::metadata(&f).unwrap();
        Owner {
            uid: m.uid(),
            gid: m.gid(),
        }
    }

    /// A group this user is in besides its own, which it may give a file.
    fn other_group(primary: u32) -> Option<u32> {
        let out = Command::new("id").arg("-G").output().ok()?;
        String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .filter_map(|g| g.parse().ok())
            .find(|g| *g != primary)
    }

    #[test]
    fn the_job_file_keeps_the_account_the_job_names() {
        let o = Owner {
            uid: 1001,
            gid: 1002,
        };
        let kept = job_file("192.0.2.7:8090", &job_with(Some(o)));
        assert_eq!(kept["uid"], 1001);
        assert_eq!(kept["gid"], 1002);
        assert_eq!(Owner::of(&kept), Some(o));
        assert_eq!(kept["token"], "t");
        let none = job_file("192.0.2.7:8090", &job_with(None));
        assert!(
            none.get("uid").is_none() && none.get("gid").is_none(),
            "{none}"
        );
    }

    #[test]
    fn only_real_folders_of_the_cycle_are_planned_to_be_handed_over() {
        let d = TestDir::new("contractor-own-plan");
        let p = paths_in(&d);
        let ws = p.home.join("work/widget-7");
        let cache = p.home.join("work/widget-7.state/cache");
        let runs = p.runs().join("issue-7");
        let outside = d.path().join("outside");
        for dir in [&ws, &cache, &runs, &outside] {
            fs::create_dir_all(dir).unwrap();
        }
        let link = p.home.join("work/widget-7.state/relay");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        let file = p.home.join("work/widget-7.state/models.json");
        fs::write(&file, "{}").unwrap();
        let real = |q: &Path| fs::canonicalize(q).unwrap();
        assert_eq!(
            ownership_plan(&p, &[ws.clone(), cache.clone(), runs.clone()]).unwrap(),
            vec![real(&ws), real(&cache), real(&runs)]
        );
        for (asked, why) in [
            (p.home.join("work"), "not a folder of the cycle"),
            (p.runs(), "not a folder of the cycle"),
            (outside.clone(), "not a folder of the cycle"),
            (
                p.home.join("work/../../../outside"),
                "not a folder of the cycle",
            ),
            (p.home.clone(), "not a folder of the cycle"),
            (link.clone(), "no folder"),
            (file.clone(), "no folder"),
            (p.home.join("work/widget-8"), "not there"),
        ] {
            let got = ownership_plan(&p, &[ws.clone(), asked.clone()]);
            assert!(
                got.as_ref().is_err_and(|e| e.contains(why)),
                "{}: {got:?}",
                asked.display()
            );
        }
    }

    #[test]
    fn a_job_s_folders_go_to_its_account_and_a_link_out_of_them_is_not_followed() {
        let d = TestDir::new("contractor-own");
        let p = paths_in(&d);
        let mine = me(&d);
        let ws = p.home.join("work/widget-7");
        let relay = p.home.join("work/widget-7.state/relay");
        fs::create_dir_all(ws.join("src/deep")).unwrap();
        fs::create_dir_all(&relay).unwrap();
        fs::write(ws.join("src/deep/a.rs"), "x").unwrap();
        let outside = d.path().join("outside.txt");
        fs::write(&outside, "x").unwrap();
        std::os::unix::fs::symlink(&outside, ws.join("src/out")).unwrap();
        let asked = [ws.clone(), relay.clone()];

        // No job running: nothing to hand over to.
        assert!(own(&p, mine, &asked).is_err_and(|e| e.contains("no job")));
        // A job that names no account leaves the folders as they are.
        write_private(
            &p.job_file(),
            job_file("f", &job_with(None)).to_string().as_bytes(),
        )
        .unwrap();
        assert_eq!(own(&p, mine, &asked), Ok(Handed::Unnamed));

        // A group of this user's own other than its first, where it has one,
        // shows the change; otherwise its own account, which must still pass.
        let to = Owner {
            uid: mine.uid,
            gid: other_group(mine.gid).unwrap_or(mine.gid),
        };
        write_private(
            &p.job_file(),
            job_file("f", &job_with(Some(to))).to_string().as_bytes(),
        )
        .unwrap();
        use std::os::unix::fs::MetadataExt;
        let owner = |q: &Path| {
            let m = fs::symlink_metadata(q).unwrap();
            (m.uid(), m.gid())
        };
        // One refused folder and nothing is handed over.
        assert!(own(&p, to, &[ws.clone(), d.path().to_path_buf()]).is_err());
        assert_eq!(owner(&ws), (mine.uid, mine.gid));
        // A container that runs as another account than the job names keeps
        // the folders: it must own what it writes.
        let other = Owner {
            uid: to.uid + 1,
            gid: to.gid,
        };
        assert_eq!(
            own(&p, other, &asked),
            Ok(Handed::Elsewhere {
                job: to,
                container: other
            })
        );
        assert_eq!(owner(&ws), (mine.uid, mine.gid));
        assert_eq!(own(&p, to, &asked), Ok(Handed::To(to)));
        for q in [
            ws.clone(),
            ws.join("src"),
            ws.join("src/deep"),
            ws.join("src/deep/a.rs"),
            ws.join("src/out"),
            relay.clone(),
        ] {
            assert_eq!(owner(&q), (to.uid, to.gid), "{}", q.display());
        }
        assert_eq!(owner(&outside), (mine.uid, mine.gid));

        // An account this user may not give a file to is an error, said.
        if mine.uid != 0 {
            let err = hand_over(
                &ws,
                Owner {
                    uid: mine.uid + 1,
                    gid: mine.gid,
                },
            );
            assert!(err.is_err_and(|e| e.contains("cannot hand")));
        }
    }

    #[test]
    fn git_goes_through_the_local_end_and_no_proxy_variable_of_the_user_s() {
        let cmd = git_with("ghs_x", "http://127.0.0.1:4711");
        let envs: Vec<(String, Option<String>)> = cmd
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect();
        let get = |k: &str| envs.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        assert_eq!(get("GIT_CONFIG_COUNT"), Some(Some("2".into())));
        assert_eq!(get("GIT_CONFIG_KEY_1"), Some(Some("http.proxy".into())));
        assert_eq!(
            get("GIT_CONFIG_VALUE_1"),
            Some(Some("http://127.0.0.1:4711".into()))
        );
        for name in [
            "NO_PROXY",
            "no_proxy",
            "HTTPS_PROXY",
            "https_proxy",
            "ALL_PROXY",
        ] {
            assert_eq!(get(name), Some(None), "{name}");
        }
        let args: Vec<_> = cmd.get_args().collect();
        assert!(!args.iter().any(|a| a.to_string_lossy().contains("ghs_x")));
    }

    #[test]
    fn base64_is_the_standard_alphabet_with_padding() {
        assert_eq!(base64(b"x-access-token:t"), "eC1hY2Nlc3MtdG9rZW46dA==");
        assert_eq!(base64(b"abc"), "YWJj");
        assert_eq!(base64(b"ab"), "YWI=");
    }

    #[test]
    fn a_job_s_instance_file_holds_its_settings_and_its_model_token_under_each_provider() {
        let job = Job::of(&json!({
            "job": 1, "issue": 7, "attempt": 1, "timeout": 60, "token": "tok", "model_token": "mtok",
            "settings": { "TEHAS_ALLOWED_LABELERS": "alice bob", "TEHAS_REPO": "acme/it's" },
            "models": { "providers": { "main-gw": {}, "other": {} } }
        }))
        .unwrap();
        let text = instance_env(&job);
        assert!(text.contains("TEHAS_ALLOWED_LABELERS='alice bob'\n"));
        assert!(text.contains("TEHAS_REPO='acme/it'\\''s'\n"));
        assert!(text.contains("PI_ENSEMBLE_API_KEY_MAIN_GW='mtok'\n"));
        assert!(text.contains("PI_ENSEMBLE_API_KEY_OTHER='mtok'\n"));
        // The job token is the contractor program's and never the container's.
        assert!(!text.contains("'tok'"), "{text}");
    }

    #[test]
    fn a_report_reads_the_image_line_the_memory_and_the_disk() {
        let id = format!("sha256:{}", "a".repeat(64));
        assert_eq!(
            image_of(&format!("{id} v0.14.50\n")),
            (Some(id.clone()), Some("v0.14.50".to_string()))
        );
        assert_eq!(image_of(&format!("{id} <no value>\n")), (Some(id), None));
        assert_eq!(image_of(""), (None, None));
        let meminfo = "MemTotal:       65805312 kB\nMemFree:         1000000 kB\nMemAvailable:   32768000 kB\n";
        assert_eq!(mem_mb(meminfo), Some(32000));
        assert_eq!(mem_mb("MemTotal: 1 kB\n"), None);
        let df = "Filesystem     1024-blocks      Used Available Capacity Mounted on\n/dev/sda1        100000000  40000000  60000000      40% /\n";
        assert_eq!(disk_kb(df), Some(60_000_000));
        assert_eq!(disk_kb("Filesystem\n"), None);
    }

    #[test]
    fn a_report_of_this_box_has_its_memory_and_disk_and_no_image_unnamed() {
        let d = TestDir::new("contractor-report");
        let r = report(d.path(), "");
        assert!(r["mem_mb"].as_u64().is_some(), "{r}");
        assert!(r["disk_kb"].as_u64().is_some(), "{r}");
        assert!(r.get("image").is_none(), "{r}");
    }
}
