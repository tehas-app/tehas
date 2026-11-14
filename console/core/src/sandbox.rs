//! What a sandbox container is given, in one place so that a cycle, a planning
//! turn, a manual session and a review cannot drift apart. AGENTS.md rule 4:
//! a change here is a security change, and test/fixtures/argv pins the result
//! word by word.
//!
//! A kind is a closed list and takes no free-form variable or mount. Not given
//! to any container, and not a setting: the Docker socket, SSH keys,
//! ~/.config/tehas, the product checkout, the rest of the host environment. Of
//! ~/.config/tehas/env a container is given the bot's token and the model
//! endpoints' tokens that models.json names, by name, never by value.
//!
//! Every list here is docker options, one word each.

use crate::models::{self, ModelFile};
use crate::slice::{self, Load, Probe, Usable};
use std::env;
use std::fs;
use std::process::{Command, Stdio};

/// Where a container has its own copy of the code index. A path inside the
/// container, not a setting. The host directory behind it belongs to one
/// container and goes with it.
pub const CACHE_DIR: &str = "/home/vscode/.tehas-cache";

/// Where a manual session has its cargo build output. A path off the image's
/// home would build into the container's layer, which the session throws away.
/// A cycle and a planning turn get no CARGO_TARGET_DIR: cargo builds in the
/// workspace, the read-write mount, so on the host's disk.
pub const TARGET_DIR: &str = "/home/vscode/.tehas-target";

// tehas setup asks the host's git for it too, and lib/github-app.sh gives a
// login shell's git the same key.
pub const HELPER_KEY: &str = "credential.https://github.com.helper";
const HELPER: &str = "!gh auth git-credential";

const REVIEW_LEAVES_OUT: [&str; 8] = [
    "PI_ENSEMBLE_EXTRA_IMAGE_DIRS",
    "PI_ENSEMBLE_PLAN_ISSUE",
    "PI_ENSEMBLE_PLAN_SESSION_DIR",
    "PI_ENSEMBLE_PLAN_CACHE_TTL_MS",
    "PI_ENSEMBLE_MAX_WORKSTREAMS",
    "PI_ENSEMBLE_VERIFY_TIMEOUT_MS",
    "PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS",
    "PI_ENSEMBLE_VERIFY_SUITE_TIMEOUT_MS",
];

// A cycle's own: `tehas work` gives these itself, or not at all, whatever the
// instance says. An instance's image directory would name a path no one mounted.
const CYCLE_LEAVES_OUT: [&str; 7] = [
    "PI_ENSEMBLE_EXTRA_IMAGE_DIRS",
    "PI_ENSEMBLE_HANDOFF_CHANNEL",
    "PI_ENSEMBLE_HANDOFF_TIMEOUT_MS",
    "PI_ENSEMBLE_CYCLE_DEADLINE",
    "PI_ENSEMBLE_CYCLE_BUDGET_MS",
    "PI_ENSEMBLE_VERIFY_CONCURRENCY",
    "PI_ENSEMBLE_SPAWN_TIMEOUT_MS",
];

/// The runner by its path in the image: the link to it on PATH does not find
/// its own source (pi-rukas v0.12.80).
pub const RUNNER: &str = "/opt/pi-rukas/bin/pi-rukas-headless";

/// The handoff allowance of a cycle, in seconds: the one budget its handoff has,
/// given to pi-rukas and kept clear of the stop by the deadline.
pub const HANDOFF_SECONDS: u64 = 480;

/// The containers that run an agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Cycle,
    Turn,
    Session,
    Review,
}

impl Kind {
    pub fn parse(word: &str) -> Option<Kind> {
        match word {
            "cycle" => Some(Kind::Cycle),
            "turn" => Some(Kind::Turn),
            "session" => Some(Kind::Session),
            "review" => Some(Kind::Review),
            _ => None,
        }
    }

    // A run that writes nothing to the tracker is given no token to write
    // with: its name is not among the variables at all, and the GitHub App's
    // token directory is not mounted. This is the one place that decides it.
    fn token(self) -> bool {
        self != Kind::Review
    }

    // The directory under $TEHAS_HOME of the app token a container reads. A
    // cycle gets its own token, narrowed to the product repository (#1814),
    // and a planning turn and a session get one of the same body in a
    // directory of their own. The app's own token goes to no container.
    fn app_dir(self) -> Option<&'static str> {
        match self {
            Kind::Turn | Kind::Session => Some("state/github-session"),
            Kind::Cycle => Some("state/github-cycle"),
            Kind::Review => None,
        }
    }
}

/// Where a cycle's container has its issue relay, read and written by both
/// sides: host/sandbox/gh-shim asks there and the host answers.
pub const RELAY_DIR: &str = "/run/tehas/relay";

/// The variable that holds the token a cycle's containers are given as their
/// GH_TOKEN: `tehas work` sets it to the app's narrowed cycle token (#1814).
pub const CYCLE_TOKEN: &str = "GH_CYCLE_TOKEN";

/// A docker client that starts a cycle's container: its GH_TOKEN, which the
/// container takes by name, is the cycle token. Without one it has none, so the
/// host's token cannot reach the container.
pub fn give_cycle_token(cmd: &mut Command) -> &mut Command {
    match env::var(CYCLE_TOKEN) {
        Ok(t) if !t.is_empty() => cmd.env("GH_TOKEN", t),
        _ => cmd.env_remove("GH_TOKEN"),
    }
}

fn pair(flag: &str, words: Vec<String>) -> Vec<String> {
    words
        .into_iter()
        .flat_map(|w| [flag.to_string(), w])
        .collect()
}

/// Whose name a container's git carries, and the tree it works in.
pub struct Identity<'a> {
    pub tree: &'a str,
    pub name: &'a str,
    pub email: &'a str,
}

/// The -e options. The same variables as the pi-rukas wrapper sets, without the
/// host's whole environment: the bot token by name, the PI_ENSEMBLE_* settings
/// in `ensemble`, git's configuration, and in `tokens` the model endpoints'
/// tokens by name: Pi reads "$TEHAS_MODEL_KEY_<NAME>" from its environment.
///
/// git inside the sandbox has no configuration of its own: no credential helper
/// and no identity. Without the helper the driver commits but cannot push, and
/// a cycle parks after all the work is done. git is handed the bot's token
/// through gh (which reads GH_TOKEN) and an identity, as environment-level
/// config: it outranks any file, reaches every agent process, and writes
/// nothing into the tree.
pub fn env_options(
    kind: Kind,
    who: &Identity,
    ensemble: &[String],
    tokens: &[String],
) -> Vec<String> {
    let mut vars = Vec::new();
    if kind.token() {
        vars.push("GH_TOKEN".to_string());
    }
    let read_only = crate::read_only::on();
    vars.extend([
        "PI_ENSEMBLE_SANDBOX_MODE=1".to_string(),
        format!("PI_ENSEMBLE_WORKSPACE_ROOT={}", who.tree),
        format!("CBM_CACHE_DIR={CACHE_DIR}"),
        format!("GIT_CONFIG_COUNT={}", if read_only { 6 } else { 4 }),
        format!("GIT_CONFIG_KEY_0={HELPER_KEY}"),
        "GIT_CONFIG_VALUE_0=".to_string(),
        format!("GIT_CONFIG_KEY_1={HELPER_KEY}"),
        format!("GIT_CONFIG_VALUE_1={HELPER}"),
        "GIT_CONFIG_KEY_2=user.name".to_string(),
        format!("GIT_CONFIG_VALUE_2={}", who.name),
        "GIT_CONFIG_KEY_3=user.email".to_string(),
        format!("GIT_CONFIG_VALUE_3={}", who.email),
        "GIT_TERMINAL_PROMPT=0".to_string(),
    ]);
    if read_only {
        vars.extend(read_only_variables(who.tree));
    }
    vars.extend_from_slice(ensemble);
    vars.extend_from_slice(tokens);
    if kind == Kind::Session {
        vars.push(format!("CARGO_TARGET_DIR={TARGET_DIR}"));
    }
    pair("-e", vars)
}

/// Where a push to a URL of github.com goes on a read-only instance: nowhere a
/// push can land, so it fails before it leaves the container.
const NO_PUSH: &str = "file:///nonexistent/tehas-read-only/";

/// What a container of a read-only instance (TEHAS_READ_ONLY=1, #608) is given
/// besides: git's pushes to origin land in the workspace's own repository,
/// which is where the cycle's branch stays, and a push that names a URL of
/// github.com fails. TEHAS_READ_ONLY=1 has the image's gh refuse every call
/// that may write (host/sandbox/gh-shim). The keys follow the four of
/// [`env_options`], whose GIT_CONFIG_COUNT counts them.
pub fn read_only_variables(tree: &str) -> Vec<String> {
    vec![
        "GIT_CONFIG_KEY_4=remote.origin.pushurl".to_string(),
        format!("GIT_CONFIG_VALUE_4={tree}"),
        format!("GIT_CONFIG_KEY_5=url.{NO_PUSH}.pushInsteadOf"),
        "GIT_CONFIG_VALUE_5=https://github.com/".to_string(),
        "TEHAS_READ_ONLY=1".to_string(),
    ]
}

/// The PI_ENSEMBLE_* the host has set, in the order of its environment. Not the
/// settings of pi-rukas's own docker wrapper, which no container runs: the
/// image, the aliases (given as --add-host) and the socket and SSH switches.
/// The runner does not list them as inputs. A review is given none of the names
/// of a planning turn or of a cycle's gate: it writes nothing to the tracker. A
/// session's image directory is its own command's to give, and an instance's
/// would name a path the container does not have.
pub fn ensemble(kind: Kind) -> Vec<String> {
    let names = env::vars_os().filter_map(|(name, _)| name.into_string().ok());
    names
        .filter(|name| name.starts_with("PI_ENSEMBLE_"))
        .filter(|name| {
            !matches!(
                name.as_str(),
                "PI_ENSEMBLE_IMAGE"
                    | "PI_ENSEMBLE_HOST_ALIASES"
                    | "PI_ENSEMBLE_NO_DOCKER_SOCKET"
                    | "PI_ENSEMBLE_NO_SSH"
            )
        })
        .filter(|name| kind != Kind::Review || !REVIEW_LEAVES_OUT.contains(&name.as_str()))
        .filter(|name| kind != Kind::Session || name != "PI_ENSEMBLE_EXTRA_IMAGE_DIRS")
        .filter(|name| kind != Kind::Cycle || !CYCLE_LEAVES_OUT.contains(&name.as_str()))
        .collect()
}

/// The -e options only a cycle has. pi-rukas posts no handoff comment itself:
/// tehasbot cycle posts the one comment and labels the issue. `verify` is how
/// many gate runs it makes at once, `child_seconds` one child's bound and
/// `deadline` the time after which no step starts, in milliseconds since the
/// epoch, when there is room for one.
pub fn cycle_variables(verify: &str, child_seconds: u64, deadline: Option<i64>) -> Vec<String> {
    let mut vars = vec![
        "PI_ENSEMBLE_HANDOFF_CHANNEL=host".to_string(),
        format!("PI_ENSEMBLE_HANDOFF_TIMEOUT_MS={}", HANDOFF_SECONDS * 1000),
        format!("PI_ENSEMBLE_VERIFY_CONCURRENCY={verify}"),
        format!("PI_ENSEMBLE_SPAWN_TIMEOUT_MS={}", child_seconds * 1000),
    ];
    vars.extend(deadline.map(|d| format!("PI_ENSEMBLE_CYCLE_DEADLINE={d}")));
    pair("-e", vars)
}

/// The container that asks whether the sandbox can push, before a cycle spends
/// an hour of model time: a dry run needs the same credentials a real push does
/// and creates nothing. It is outside the slice: a container of a few seconds.
/// `network` is the cycle's [`network`], so it reaches only what the cycle can.
pub fn push_check(
    security: &[String],
    user: &str,
    env: &[String],
    network: &[String],
    tree: &str,
    host: &str,
    image: &str,
) -> Vec<String> {
    let mut w = words(&["docker", "run", "--rm"]);
    w.extend_from_slice(security);
    w.extend(words(&["--user", user, "-e", "HOME=/home/vscode"]));
    w.extend_from_slice(env);
    w.extend_from_slice(network);
    w.extend(words(&["-w", tree, "-v", &format!("{host}:{tree}")]));
    w.extend(words(&["--entrypoint", "git", image]));
    w.extend(words(&["push", "--dry-run", "origin"]));
    w.push("HEAD:refs/heads/tehas-preflight".to_string());
    w
}

/// The container that installs the product's dependencies in a cycle's
/// workspace before the cycle starts: <script> is [`crate::install::script`].
/// It has the cycle's limits and the workspace at the cycle's path, and none of
/// the cycle's variables: an install runs code no one reviewed. `held` is the
/// cycle's limits and then its security options; `network` is the cycle's
/// [`network`], so it reaches only what the cycle can.
pub fn install_run(
    held: &[String],
    user: &str,
    network: &[String],
    tree: &str,
    host: &str,
    image: &str,
    script: &str,
) -> Vec<String> {
    let mut w = words(&["docker", "run", "--rm"]);
    w.extend_from_slice(held);
    w.extend(words(&["--user", user, "-e", "HOME=/home/vscode"]));
    w.extend_from_slice(network);
    w.extend(words(&["-w", tree, "-v", &format!("{host}:{tree}")]));
    w.extend(words(&["--entrypoint", "bash", image]));
    w.extend(words(&["-c", script]));
    w
}

/// What a cycle's container is started from, beside its image.
pub struct CycleRun<'a> {
    pub name: &'a str,
    pub limits: &'a [String],
    pub security: &'a [String],
    pub user: &'a str,
    pub cid_file: &'a str,
    pub tree: &'a str,
    pub mounts: &'a [String],
    /// The issue's images, read-only, and the variable that names them.
    pub images: &'a [String],
    pub env: &'a [String],
    pub hosts: &'a [String],
    pub image: &'a str,
    /// The runner's verb: `work`, or `work-lean` for driver lean.
    pub verb: &'a str,
    pub issue: u64,
    pub flags: &'a [String],
}

/// The whole command of a cycle's container. It has an init: Pi collects only
/// its own children, and a process that outlives its parent would count against
/// the pids limit until the container ends.
pub fn cycle_run(c: &CycleRun) -> Vec<String> {
    let mut w = words(&["docker", "run", "--rm", "--init", "--name", c.name]);
    w.extend_from_slice(c.limits);
    w.extend_from_slice(c.security);
    w.extend(words(&[
        "--user",
        c.user,
        "--cidfile",
        c.cid_file,
        "-w",
        c.tree,
    ]));
    w.extend_from_slice(c.mounts);
    w.extend_from_slice(c.images);
    w.extend_from_slice(c.env);
    w.extend_from_slice(c.hosts);
    w.extend(words(&[c.image, RUNNER, c.verb, &c.issue.to_string()]));
    w.extend_from_slice(c.flags);
    w
}

/// The check of what an image supports.
pub fn capabilities(image: &str) -> Vec<String> {
    words(&[
        "docker",
        "run",
        "--rm",
        "--init",
        image,
        RUNNER,
        "--capabilities",
    ])
}

fn words(list: &[&str]) -> Vec<String> {
    list.iter().map(|w| w.to_string()).collect()
}

/// `.a.b` as jq reads it: None where it would stop, on a value that is neither
/// an object nor null.
fn at<'a>(v: &'a serde_json::Value, path: &[&str]) -> Option<&'a serde_json::Value> {
    use serde_json::Value;
    static NULL: Value = Value::Null;
    let mut cur = v;
    for k in path {
        cur = match cur {
            Value::Object(o) => o.get(*k).unwrap_or(&NULL),
            Value::Null => &NULL,
            _ => return None,
        };
    }
    Some(cur)
}

fn merged(a: serde_json::Value, b: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match (a, b) {
        (Value::Object(mut a), Value::Object(b)) => {
            for (k, v) in b {
                let old = a.remove(&k).unwrap_or(Value::Null);
                a.insert(k, merged(old, v));
            }
            Value::Object(a)
        }
        (_, b) => b,
    }
}

/// The most of the product's .pi/settings.json that is read, in bytes.
const PROJECT_SETTINGS_MAX: u64 = 64 * 1024;

/// The product's .pi/settings.json in the workspace <host>, which a container
/// has written by now: None when there is none, Err when .pi or the file is a
/// link or something other than a plain file, or larger than the bound.
fn project_settings(host: &str) -> Result<Option<Vec<u8>>, String> {
    use std::io::Read;
    let dir = format!("{host}/.pi");
    let file = format!("{dir}/settings.json");
    let plain = |p: &str, dir: bool| {
        fs::symlink_metadata(p).map(|m| if dir { m.is_dir() } else { m.is_file() })
    };
    match (plain(&dir, true), plain(&file, false)) {
        (Err(_), _) | (Ok(true), Err(_)) => return Ok(None),
        (Ok(true), Ok(true)) => {}
        _ => return Err(format!("{file} is not a plain file")),
    }
    let not_read = || format!("{file} could not be read");
    let f = fs::File::open(&file).map_err(|_| not_read())?;
    if !f.metadata().is_ok_and(|m| m.is_file()) {
        return Err(format!("{file} is not a plain file"));
    }
    let mut text = Vec::new();
    f.take(PROJECT_SETTINGS_MAX + 1)
        .read_to_end(&mut text)
        .map_err(|_| not_read())?;
    if text.len() as u64 > PROJECT_SETTINGS_MAX {
        return Err(format!(
            "{file} is larger than {PROJECT_SETTINGS_MAX} bytes"
        ));
    }
    Ok(Some(text))
}

fn first_json(file: &str) -> Option<serde_json::Value> {
    first_json_of(&fs::read(file).ok()?)
}

fn first_json_of(text: &[u8]) -> Option<serde_json::Value> {
    use serde_json::Value;
    let mut all = serde_json::Deserializer::from_slice(text).into_iter::<Value>();
    let first = all.next().transpose().ok()?;
    match first.unwrap_or(Value::Null) {
        Value::Null | Value::Bool(false) => Some(Value::Object(Default::default())),
        v => Some(v),
    }
}

/// The provider and agent retry and timeout a container's Pi runs with, as one
/// line for the cycle's log: the host's settings.json that a container is
/// mounted under the product's own .pi/settings.json in <host>, merged the way
/// Pi merges them. A value neither sets is Pi's default.
pub fn retry_line(host: &str) -> String {
    let global = format!("{}/settings.json", agent_dir());
    if !std::path::Path::new(&global).is_file() {
        return format!("provider retry: unknown, {global} is missing");
    }
    let (project, text) = match project_settings(host) {
        Ok(Some(text)) => (format!("{host}/.pi/settings.json"), text),
        Ok(None) => ("/dev/null".to_string(), Vec::new()),
        Err(why) => return format!("provider retry: unknown, {why}"),
    };
    let not_json = || format!("provider retry: unknown, {global} or {project} is not JSON");
    let (Some(g), Some(p)) = (first_json(&global), first_json_of(&text)) else {
        return not_json();
    };
    if !g.is_object() || !p.is_object() {
        return not_json();
    }
    retry_fields(&merged(g, p)).unwrap_or_else(not_json)
}

/// The line of retry_line from the merged settings; None when a key on a
/// path is under something that is not an object.
fn retry_fields(s: &serde_json::Value) -> Option<String> {
    use serde_json::Value;
    let v = |path: &[&str]| {
        at(s, path).map(|x| match x {
            Value::Null => "unset".to_string(),
            Value::String(t) => t.clone(),
            other => other.to_string(),
        })
    };
    // Pi 1.5 reads it at the top level; settings made for 1.4 nest it under
    // retry.provider, so a host mid-upgrade has either.
    let idle: &[&str] = match at(s, &["httpIdleTimeoutMs"]) {
        Some(Value::Null) => &["retry", "provider", "httpIdleTimeoutMs"],
        _ => &["httpIdleTimeoutMs"],
    };
    let fields = [
        ("timeoutMs", v(&["retry", "provider", "timeoutMs"])),
        ("maxRetries", v(&["retry", "provider", "maxRetries"])),
        (
            "maxRetryDelayMs",
            v(&["retry", "provider", "maxRetryDelayMs"]),
        ),
        ("httpIdleTimeoutMs", v(idle)),
        ("agentMaxRetries", v(&["retry", "maxRetries"])),
        ("agentBaseDelayMs", v(&["retry", "baseDelayMs"])),
        ("agentMaxDelayMs", v(&["retry", "maxAgentDelayMs"])),
    ];
    let mut line = "provider retry:".to_string();
    for (name, value) in fields {
        line.push_str(&format!(" {name}={}", value?));
    }
    Some(line)
}

/// The tokens models.json names that are set.
pub fn tokens() -> Vec<String> {
    let names = models::key_names(&ModelFile::read(&models::host_path()).json);
    names.into_iter().filter(|name| set(name)).collect()
}

fn set(name: &str) -> bool {
    env::var_os(name).is_some_and(|v| !v.is_empty())
}

/// The directories of one container. <tree> is the path inside the container
/// and <host> the directory mounted there. <state> is a host directory of this
/// container's own: its copy of the code index, its memory and its sessions go
/// under it. <runs> is the host directory of its transcripts. All are named by
/// the caller: there is no default that lets one container's writes be
/// another's input.
pub struct Paths<'a> {
    pub tree: &'a str,
    pub host: &'a str,
    pub state: &'a str,
    pub runs: &'a str,
}

/// The -v options. The two cache volumes pi-rukas mounts are not mounted: what
/// a container writes under its home, caches of packages among them, stays in
/// its own layer and goes when the container is removed. Pi's settings.json is
/// the one install.sh puts in ~/.pi/agent: its provider retry and timeout.
/// models.json is the container's own, <state>/models.json: the host's with
/// each provider on the endpoint picked now.
///
/// `app` is the host directory of the GitHub App's token when the instance
/// names an app: the image's gh (host/sandbox/gh-shim) reads the token the host
/// keeps fresh there. Not the key, which stays in ~/.config/tehas. For a cycle
/// it is the directory of the narrowed cycle token, which the caller names.
pub fn mount_options(kind: Kind, p: &Paths, agent: &str, app: Option<&str>) -> Vec<String> {
    let (tree, host, state, runs) = (p.tree, p.host, p.state, p.runs);
    let pi = "/home/vscode/.pi/agent";
    let mut mounts = vec![
        format!("{host}:{tree}"),
        format!("{state}/cache:{CACHE_DIR}"),
        format!("{state}/memory:/home/vscode/.vipune"),
        format!("{state}/sessions:{pi}/sessions"),
        format!("{runs}:{pi}/ensemble-runs"),
        format!("{state}/models.json:{pi}/models.json:ro"),
        format!("{agent}/mcp.json:{pi}/mcp.json:ro"),
        format!("{agent}/settings.json:{pi}/settings.json:ro"),
    ];
    if let (Some(app), true) = (app, kind.app_dir().is_some()) {
        mounts.push(format!("{app}:/run/tehas/github:ro"));
    }
    // A cycle's token reads issues only; its issue writes go to the host
    // through this directory (console/cycle/src/relay.rs). Its own, in its
    // state, so no cycle sees another's requests.
    if kind == Kind::Cycle {
        mounts.push(format!("{state}/relay:{RELAY_DIR}"));
    }
    // After the shared list, so this is the one build output the session has.
    if kind == Kind::Session {
        mounts.push(format!("{state}/target:{TARGET_DIR}"));
    }
    pair("-v", mounts)
}

fn agent_dir() -> String {
    format!("{}/.pi/agent", env::var("HOME").unwrap_or_default())
}

/// Whether Pi's settings.json and models.json are there to be mounted, and the
/// tokens models.json names are set. Commands ask before they call Docker or
/// git or write anything: a missing file is a failure, not a default, since
/// Docker would make a root-owned directory in its place. Err is the line for
/// standard error.
pub fn settings_ok() -> Result<(), String> {
    let agent = agent_dir();
    for f in ["settings.json", "models.json"] {
        if !std::path::Path::new(&format!("{agent}/{f}")).is_file() {
            return Err(format!("tehas: no {agent}/{f}: run install.sh"));
        }
    }
    models::host_keys_ok().map_err(|e| format!("tehas: {e}"))
}

/// The -v options with what they name made on the host, because Docker would
/// create a missing mount source as a root-owned directory. The caller makes
/// <host>; `tehas-cycle sandbox mounts` writes <state>/models.json. Asks [`settings_ok`] again, and says
/// why on Err.
pub fn prepared_mounts(kind: Kind, p: &Paths) -> Result<Vec<String>, String> {
    if [p.tree, p.host, p.state, p.runs]
        .iter()
        .any(|d| d.is_empty())
    {
        return Err(String::new());
    }
    settings_ok()?;
    let mut dirs = vec![
        p.runs.to_string(),
        format!("{}/cache", p.state),
        format!("{}/memory", p.state),
        format!("{}/sessions", p.state),
    ];
    if kind == Kind::Cycle {
        dirs.push(format!("{}/relay", p.state));
    }
    let app = kind
        .app_dir()
        .map(|dir| format!("{}/{dir}", env::var("TEHAS_HOME").unwrap_or_default()));
    dirs.extend(app.clone());
    for dir in dirs {
        fs::create_dir_all(&dir).map_err(|e| format!("tehas: cannot make {dir}: {e}"))?;
    }
    Ok(mount_options(kind, p, &agent_dir(), app.as_deref()))
}

/// The account a container runs as, `<uid>:<gid>`, as `id` says it.
pub fn user() -> String {
    let id = |flag: &str| {
        Command::new("id")
            .arg(flag)
            .stderr(Stdio::null())
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim_end().to_string())
            .unwrap_or_default()
    };
    format!("{}:{}", id("-u"), id("-g"))
}

// One call of git, stdio as this program's. True when it succeeded.
fn ran(cmd: &mut Command) -> bool {
    cmd.status().is_ok_and(|s| s.success())
}

fn git(dir: &str) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(dir);
    cmd
}

// What one git call printed, when it succeeded.
fn git_says(dir: &str, args: &[&str]) -> Option<String> {
    let out = git(dir).args(args).stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| {
        String::from_utf8_lossy(&out.stdout)
            .trim_end_matches('\n')
            .to_string()
    })
}

/// Make <tree>: a clone at the tip the product checkout <src> last fetched for
/// <branch>, with the product's address as origin. Nothing is fetched from the
/// network. The host runs git in <tree> here and only here: once a container
/// has written a tree, anything in it, hooks and git configuration included,
/// may be an agent's. The caller decides whether <tree> may be made.
pub fn clone(src: &str, tree: &str, branch: &str) -> bool {
    let tip = format!("refs/remotes/origin/{branch}");
    let verify = format!("{tip}^{{commit}}");
    let Some(base) = git_says(src, &["rev-parse", "-q", "--verify", &verify]) else {
        eprintln!("tehas: {src} has no origin/{branch}");
        return false;
    };
    let Some(url) = git_says(src, &["config", "--get", "remote.origin.url"]) else {
        return false;
    };
    let all = "+refs/remotes/origin/*:refs/remotes/origin/*";
    let upstream = format!("--set-upstream-to=origin/{branch}");
    ran(Command::new("git").args(["init", "-q", "-b", branch, tree]))
        && ran(git(tree).args(["remote", "add", "origin", &url]))
        && ran(Command::new("timeout")
            .args(["-k", "10", "300", "git", "-C", tree, "fetch"])
            .args(["-q", "--no-tags", src, all]))
        && ran(git(tree).args(["symbolic-ref", "refs/remotes/origin/HEAD", &tip]))
        && ran(git(tree).args(["checkout", "-q", "-B", branch, &base]))
        && ran(git(tree).args(["branch", "-q", &upstream, branch]))
}

/// Put the host's code index in <state>/cache, the copy this container is given.
/// The index is one sqlite file per tree that was indexed. Not copied: the
/// indexer's own logs, which a container has no use for, and what a run that was
/// stopped left behind (a staging file about half the size of the index it was
/// writing, read by nothing). A host that never indexed anything has nothing to
/// give: that is not a failure, and a container then searches by reading files.
pub fn seed(state: &str, index: &str) -> bool {
    let cache = format!("{state}/cache");
    if fs::create_dir_all(&cache).is_err() {
        return false;
    }
    let Ok(entries) = fs::read_dir(index) else {
        return !std::path::Path::new(index).is_dir();
    };
    let mut files: Vec<_> = entries
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            !(name.starts_with('.')
                || name == "logs"
                || name.contains(".db.stage.")
                || name.ends_with(".tmp"))
        })
        .map(|e| e.path())
        .collect();
    files.sort();
    let copied = files.iter().all(|f| {
        Command::new("cp")
            .arg("-a")
            .arg(f)
            .arg(format!("{cache}/"))
            .status()
            .is_ok_and(|s| s.success())
    });
    if !copied {
        let _ = fs::remove_dir_all(&cache);
        let _ = fs::create_dir_all(&cache);
    }
    copied
}

/// The --add-host options: the names the sandbox cannot resolve by itself, from
/// PI_ENSEMBLE_HOST_ALIASES as lib/config.sh checked it.
pub fn hosts(aliases: &str) -> Vec<String> {
    aliases
        .lines()
        .next()
        .unwrap_or("")
        .split(',')
        .filter(|p| !p.is_empty())
        .map(|p| format!("--add-host={p}"))
        .collect()
}

/// The network options of an agent's container: the --add-host options, and
/// with TEHAS_EGRESS=1 the internal network and the proxy of crate::egress,
/// which is started here if it is not up. Err is the line that says why the
/// proxy is not up: the container is not started on an open network instead.
pub fn network(aliases: &str) -> Result<Vec<String>, String> {
    let mut w = hosts(aliases);
    if crate::egress::on() {
        crate::egress::ensure(aliases)?;
        w.extend(crate::egress::options());
    }
    Ok(w)
}

/// What a container is not allowed to do, so that an agent's code cannot give
/// itself more of the host than it was given: every capability Docker grants
/// by default is taken away, and a program cannot gain rights through its mode
/// or file capabilities. The same ten words for a cycle, a planning turn, a
/// manual session, a review and the push check before a cycle. host/docs/host.md
/// documents them in a table, and test/work_caps_test.sh compares that table
/// with what a command is given, so adding one option is an edit here, a row
/// there, and nothing else.
///
/// Only CHOWN and FOWNER are given back:
///
///   CHOWN    npm's install drops a package to the unprivileged user with
///            chown(2) and lchown(2), and Playwright's install of Chromium
///            copies a cache tree that way. Both run in a gate of the product.
///            Without it the gate fails on EPERM.
///   FOWNER   npm moves its staging directory onto its destination with rename,
///            and rename of a file one does not own is refused even when the
///            directory is writable. tar's unpack, cargo's install of a built
///            crate and a gate's own mv do the same.
///
/// Nothing else in the list is named. In particular:
///
///   SETUID, SETGID   nothing a cycle runs becomes another user. The image has
///                    no sudo, the container runs as one uid and tehas gives it
///                    no setuid program to reach another. Playwright's Chromium
///                    runs its own sandbox only with a capability that would
///                    let it make a setuid helper, and it is run with
///                    --no-sandbox instead, which pi-rukas's browser tool
///                    passes. no-new-privileges does not break anything a cycle
///                    does: it stops a program gaining rights, and nothing in
///                    the image is meant to have any.
///   NET_RAW          the agents answer over HTTP with curl and fetch through
///                    git, npm and cargo, none of which opens a raw socket. It
///                    is off Docker's own default list too.
///   SYS_ADMIN        the one capability that matters for a container's escape,
///                    and the one Chromium's sandbox would want. Nothing here
///                    mounts anything: tehas makes every mount on the host
///                    before it starts a container.
///
/// The root filesystem stays writable, unlike a preview's: a cycle's gate
/// installs into its own tree and its caches, and a read-only root would need
/// one --tmpfs per directory a tool writes in, which is a list nobody can pin.
/// One --tmpfs is given all the same, for /tmp: not to hide the root but to
/// bound it. The writable layer it replaces sits on the host's root disk with
/// no limit of its own; on 2026-10-20 one cycle filled the host's disk with a
/// log under /tmp and four others died with it. Why the bit stays exec, what
/// the container's own TMPDIR is, and what the size means are in the manual, at
/// "What a container may not do" in host/docs/host.md. The size is half the
/// container's --memory: tmpfs pages count against that limit and cannot be
/// reclaimed, so a /tmp as large as the limit ends in OOM kills of the agents
/// (#761). At half, a full /tmp is ENOSPC for its writer and the other half is
/// left to the processes. size=0 would be no bound at all, so a ceiling under
/// 10 MiB is no ceiling, and gives no options.
pub fn security(ceiling_mb: u64) -> Option<Vec<String>> {
    if !(10..=999_999_999).contains(&ceiling_mb) {
        return None;
    }
    let tmp = format!(
        "/tmp:rw,exec,nosuid,nodev,size={}m,mode=1777",
        ceiling_mb / 2
    );
    Some(
        [
            "--cap-drop",
            "ALL",
            "--cap-add",
            "CHOWN",
            "--cap-add",
            "FOWNER",
            "--security-opt",
            "no-new-privileges:true",
            "--tmpfs",
            &tmp,
        ]
        .map(String::from)
        .to_vec(),
    )
}

/// Which of a container's two lists is not the shape [`slice::limits`] and
/// [`security`] make.
#[derive(Debug, PartialEq, Eq)]
pub enum Misshaped {
    Limits,
    Security,
}

/// The one check of the words a container is started with, beside the code
/// that makes them: fourteen words of limits under tehas.slice, then the ten
/// security options with a /tmp of a size in whole megabytes that is no zero.
/// A list that drifted starts nothing (AGENTS.md rule 4).
pub fn shape(limits: &[String], security: &[String]) -> Result<(), Misshaped> {
    let l = limits;
    if l.len() != 14 || l[1] != "tehas.slice" || l[12] != "--cpu-shares" {
        return Err(Misshaped::Limits);
    }
    let s = security;
    let size = s.get(9).and_then(|w| {
        w.strip_prefix("/tmp:rw,exec,nosuid,nodev,size=")?
            .strip_suffix("m,mode=1777")
    });
    let fixed = [
        "--cap-drop",
        "ALL",
        "--cap-add",
        "CHOWN",
        "--cap-add",
        "FOWNER",
        "--security-opt",
        "no-new-privileges:true",
        "--tmpfs",
    ];
    let shaped = s.len() == 10
        && s[..9] == fixed
        && size.is_some_and(|m| {
            !m.is_empty() && m.bytes().all(|b| b.is_ascii_digit()) && !m.starts_with('0')
        });
    if shaped {
        Ok(())
    } else {
        Err(Misshaped::Security)
    }
}

/// What a container starts with, once the slice said it can be used.
#[derive(Debug, PartialEq)]
pub struct Taken {
    /// The fourteen words of its limits.
    pub limits: Vec<String>,
    /// The ten words of [`security`].
    pub security: Vec<String>,
    pub slice: Usable,
}

/// The limits and the security options from a reading of the host, or the
/// words a person is told, led by <who>. <what> names what does not start.
pub fn take(r: &impl Probe, load: &Load, who: &str, what: &str) -> Result<Taken, String> {
    let slice =
        slice::usable(r, &load.reserve, &load.containers).map_err(|why| format!("{who}: {why}"))?;
    taken(slice, load, who, what)
}

fn taken(slice: Usable, load: &Load, who: &str, what: &str) -> Result<Taken, String> {
    let limits = slice::limits(&slice, load)
        .ok_or_else(|| format!("{who}: the container limits could not be made, not starting"))?;
    let security = security(slice.ceiling_mb).ok_or_else(|| {
        format!("{who}: tehas_sandbox_security gave 0 words, wants 0: not starting {what}")
    })?;
    Ok(Taken {
        limits,
        security,
        slice,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_products_settings_are_read_only_as_a_plain_file_of_a_bounded_size() {
        use std::os::unix::fs::symlink;
        let d = std::env::temp_dir().join(format!("tehas-project-settings-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        let ws = d.join("ws");
        fs::create_dir_all(&ws).unwrap();
        let host = ws.to_str().unwrap();
        assert_eq!(project_settings(host), Ok(None));
        fs::create_dir_all(ws.join(".pi")).unwrap();
        let file = ws.join(".pi/settings.json");
        fs::write(&file, "{}").unwrap();
        assert_eq!(project_settings(host), Ok(Some(b"{}".to_vec())));
        fs::write(&file, vec![b' '; PROJECT_SETTINGS_MAX as usize + 1]).unwrap();
        assert!(project_settings(host)
            .unwrap_err()
            .contains("is larger than"));
        fs::remove_file(&file).unwrap();
        fs::write(d.join("elsewhere.json"), "{}").unwrap();
        symlink(d.join("elsewhere.json"), &file).unwrap();
        assert!(project_settings(host)
            .unwrap_err()
            .contains("is not a plain file"));
        fs::remove_dir_all(ws.join(".pi")).unwrap();
        fs::create_dir_all(d.join("other")).unwrap();
        fs::write(d.join("other/settings.json"), "{}").unwrap();
        symlink(d.join("other"), ws.join(".pi")).unwrap();
        assert!(project_settings(host)
            .unwrap_err()
            .contains("is not a plain file"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn the_idle_timeout_is_read_at_the_top_level_and_under_retry_provider() {
        use serde_json::json;
        let idle = |s: serde_json::Value| {
            let line = retry_fields(&s).unwrap();
            line.split(' ')
                .find_map(|w| w.strip_prefix("httpIdleTimeoutMs="))
                .unwrap()
                .to_string()
        };
        assert_eq!(idle(json!({"httpIdleTimeoutMs": 120000})), "120000");
        assert_eq!(
            idle(json!({"retry": {"provider": {"httpIdleTimeoutMs": 90000}}})),
            "90000"
        );
        assert_eq!(
            idle(json!({"httpIdleTimeoutMs": 5, "retry": {"provider": {"httpIdleTimeoutMs": 7}}})),
            "5"
        );
        assert_eq!(idle(json!({"retry": {"provider": {}}})), "unset");
        assert_eq!(idle(json!({})), "unset");
        assert_eq!(retry_fields(&json!({"retry": 1})), None);
    }

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(String::from).collect()
    }

    /// The read-only variables as a container's git takes them: a push to
    /// origin, whose URL is GitHub's, lands in the repository itself, with no
    /// network (a push that left would fail offline), and a push to a URL of
    /// github.com fails.
    #[test]
    fn a_read_only_push_to_origin_stays_in_the_workspace_and_one_to_github_fails() {
        let d = crate::testdir::TestDir::new("read-only-push");
        let tree = d.join("widget");
        let t = tree.to_str().unwrap();
        let git = |args: &[&str]| {
            let mut c = Command::new("git");
            c.args(["-C", t]).args(args);
            c.env_clear()
                .env("PATH", std::env::var_os("PATH").unwrap_or_default());
            c.env("HOME", d.join("home"))
                .env("GIT_TERMINAL_PROMPT", "0");
            c.env("GIT_CONFIG_NOSYSTEM", "1");
            c.env("GIT_CONFIG_COUNT", "6");
            for (i, w) in [
                "user.name=A Bot".to_string(),
                "user.email=bot@example.org".to_string(),
                "core.hooksPath=/dev/null".to_string(),
                "init.defaultBranch=main".to_string(),
            ]
            .iter()
            .enumerate()
            {
                let (k, v) = w.split_once('=').unwrap();
                c.env(format!("GIT_CONFIG_KEY_{i}"), k)
                    .env(format!("GIT_CONFIG_VALUE_{i}"), v);
            }
            for w in read_only_variables(t) {
                if let Some((k, v)) = w.split_once('=') {
                    if k.starts_with("GIT_CONFIG_") {
                        c.env(k, v);
                    }
                }
            }
            c.output().unwrap()
        };
        fs::create_dir_all(&tree).unwrap();
        assert!(git(&["init", "-q"]).status.success());
        git(&[
            "remote",
            "add",
            "origin",
            "https://github.com/example-owner/example-product.git",
        ]);
        assert!(git(&["commit", "-q", "--allow-empty", "-m", "one"])
            .status
            .success());
        let pushed = git(&["push", "-q", "origin", "HEAD:refs/heads/feature/issue-7"]);
        assert!(
            pushed.status.success(),
            "{}",
            String::from_utf8_lossy(&pushed.stderr)
        );
        assert!(
            git(&["rev-parse", "-q", "--verify", "refs/heads/feature/issue-7"])
                .status
                .success()
        );
        let dry = git(&[
            "push",
            "--dry-run",
            "origin",
            "HEAD:refs/heads/tehas-preflight",
        ]);
        assert!(dry.status.success());
        let url = git(&[
            "push",
            "-q",
            "https://github.com/example-owner/example-product.git",
            "HEAD:refs/heads/x",
        ]);
        assert!(!url.status.success());
        assert!(String::from_utf8_lossy(&url.stderr).contains("nonexistent/tehas-read-only"));
    }

    #[test]
    fn the_read_only_variables_follow_the_four_keys_and_name_the_setting() {
        let v = read_only_variables("/w/widget");
        assert_eq!(v[0], "GIT_CONFIG_KEY_4=remote.origin.pushurl");
        assert_eq!(v[1], "GIT_CONFIG_VALUE_4=/w/widget");
        assert!(v[2].starts_with("GIT_CONFIG_KEY_5=url.file:///nonexistent/"));
        assert_eq!(v[3], "GIT_CONFIG_VALUE_5=https://github.com/");
        assert_eq!(v[4], "TEHAS_READ_ONLY=1");
    }

    const WHO: Identity = Identity {
        tree: "/w/widget",
        name: "A Bot",
        email: "bot@example.com",
    };

    // The whole list of a kind, literally: what test/fixtures/argv holds for
    // the same inputs. name holds a space, so it is compared by word.
    fn env_of(kind: Kind) -> Vec<String> {
        env_options(
            kind,
            &WHO,
            &["PI_ENSEMBLE_SUBAGENT_MODEL".to_string()],
            &["TEHAS_MODEL_KEY_MAIN".to_string()],
        )
    }

    fn shared_env() -> Vec<String> {
        let mut e = words(
            "-e PI_ENSEMBLE_SANDBOX_MODE=1 -e PI_ENSEMBLE_WORKSPACE_ROOT=/w/widget \
             -e CBM_CACHE_DIR=/home/vscode/.tehas-cache -e GIT_CONFIG_COUNT=4 \
             -e GIT_CONFIG_KEY_0=credential.https://github.com.helper -e GIT_CONFIG_VALUE_0= \
             -e GIT_CONFIG_KEY_1=credential.https://github.com.helper",
        );
        e.extend(["-e", "GIT_CONFIG_VALUE_1=!gh auth git-credential"].map(String::from));
        e.extend(words("-e GIT_CONFIG_KEY_2=user.name"));
        e.extend(["-e", "GIT_CONFIG_VALUE_2=A Bot"].map(String::from));
        e.extend(words(
            "-e GIT_CONFIG_KEY_3=user.email -e GIT_CONFIG_VALUE_3=bot@example.com \
             -e GIT_TERMINAL_PROMPT=0 -e PI_ENSEMBLE_SUBAGENT_MODEL -e TEHAS_MODEL_KEY_MAIN",
        ));
        e
    }

    #[test]
    fn a_cycle_and_a_turn_are_given_the_token_by_name_and_first() {
        let mut want = words("-e GH_TOKEN");
        want.extend(shared_env());
        assert_eq!(env_of(Kind::Cycle), want);
        assert_eq!(env_of(Kind::Turn), want);
    }

    #[test]
    fn a_review_has_no_token_in_any_form() {
        let env = env_of(Kind::Review);
        assert_eq!(env, shared_env());
        assert!(!env.iter().any(|w| w.starts_with("GH_TOKEN")));
    }

    #[test]
    fn a_session_is_given_its_build_output_last() {
        let mut want = words("-e GH_TOKEN");
        want.extend(shared_env());
        want.extend(words("-e CARGO_TARGET_DIR=/home/vscode/.tehas-target"));
        assert_eq!(env_of(Kind::Session), want);
    }

    const PATHS: Paths = Paths {
        tree: "/in",
        host: "/out",
        state: "/s",
        runs: "/r",
    };

    const EIGHT: &str = "-v /out:/in -v /s/cache:/home/vscode/.tehas-cache \
        -v /s/memory:/home/vscode/.vipune -v /s/sessions:/home/vscode/.pi/agent/sessions \
        -v /r:/home/vscode/.pi/agent/ensemble-runs \
        -v /s/models.json:/home/vscode/.pi/agent/models.json:ro \
        -v /a/mcp.json:/home/vscode/.pi/agent/mcp.json:ro \
        -v /a/settings.json:/home/vscode/.pi/agent/settings.json:ro";

    #[test]
    fn every_kind_has_the_eight_mounts_in_order() {
        for kind in [Kind::Turn, Kind::Review] {
            assert_eq!(mount_options(kind, &PATHS, "/a", None), words(EIGHT));
        }
    }

    #[test]
    fn only_a_cycle_has_the_relay_and_it_is_its_own_state() {
        assert_eq!(
            mount_options(Kind::Cycle, &PATHS, "/a", None),
            words(&format!("{EIGHT} -v /s/relay:/run/tehas/relay"))
        );
        for kind in [Kind::Turn, Kind::Session, Kind::Review] {
            let all = mount_options(kind, &PATHS, "/a", Some("/g")).join(" ");
            assert!(!all.contains("relay"), "{kind:?}");
        }
    }

    #[test]
    fn a_token_directory_is_read_only_and_never_a_reviews() {
        let want = format!("{EIGHT} -v /g:/run/tehas/github:ro");
        assert_eq!(
            mount_options(Kind::Turn, &PATHS, "/a", Some("/g")),
            words(&want)
        );
        assert_eq!(
            mount_options(Kind::Cycle, &PATHS, "/a", Some("/g")),
            words(&format!("{want} -v /s/relay:/run/tehas/relay"))
        );
        assert_eq!(
            mount_options(Kind::Review, &PATHS, "/a", Some("/g")),
            words(EIGHT)
        );
    }

    #[test]
    fn a_container_reads_a_narrowed_token_never_the_apps() {
        assert_eq!(Kind::Cycle.app_dir(), Some("state/github-cycle"));
        assert_eq!(Kind::Turn.app_dir(), Some("state/github-session"));
        assert_eq!(Kind::Session.app_dir(), Some("state/github-session"));
        assert_eq!(Kind::Review.app_dir(), None);
    }

    #[test]
    fn a_sessions_build_output_comes_after_the_shared_list() {
        let want =
            format!("{EIGHT} -v /g:/run/tehas/github:ro -v /s/target:/home/vscode/.tehas-target");
        assert_eq!(
            mount_options(Kind::Session, &PATHS, "/a", Some("/g")),
            words(&want)
        );
    }

    #[test]
    fn nothing_that_controls_the_host_is_in_any_list() {
        for kind in [Kind::Cycle, Kind::Turn, Kind::Session, Kind::Review] {
            let mut all = env_of(kind);
            all.extend(mount_options(kind, &PATHS, "/a", Some("/g")));
            let all = all.join(" ");
            for word in ["docker.sock", ".ssh", ".config/tehas", "--privileged"] {
                assert!(!all.contains(word), "{kind:?} has {word}");
            }
        }
    }

    #[test]
    fn the_seed_leaves_out_the_logs_and_what_a_stopped_run_left() {
        let at = crate::testdir::TestDir::new("sandbox-seed");
        let index = at.join("index");
        fs::create_dir_all(index.join("logs")).unwrap();
        for f in ["widget.db", "widget.db.stage.1", "other.tmp", "_config.db"] {
            fs::write(index.join(f), "x").unwrap();
        }
        let state = at.join("state").display().to_string();
        assert!(seed(&state, &index.display().to_string()));
        let mut got: Vec<String> = fs::read_dir(format!("{state}/cache"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        got.sort();
        assert_eq!(got, ["_config.db", "widget.db"]);
        assert!(seed(&state, &at.join("no-index").display().to_string()));
    }

    #[test]
    fn host_aliases_are_one_option_each() {
        assert_eq!(
            hosts("models:192.0.2.9,,b:192.0.2.10"),
            ["--add-host=models:192.0.2.9", "--add-host=b:192.0.2.10"]
        );
        assert!(hosts("").is_empty());
    }

    #[test]
    fn the_security_options_are_the_ten_documented_words() {
        assert_eq!(
            security(2048).unwrap(),
            words(
                "--cap-drop ALL --cap-add CHOWN --cap-add FOWNER \
                 --security-opt no-new-privileges:true \
                 --tmpfs /tmp:rw,exec,nosuid,nodev,size=1024m,mode=1777"
            )
        );
    }

    #[test]
    fn what_security_and_the_limits_make_is_the_one_shape_that_passes() {
        let fourteen = words(
            "--cgroup-parent tehas.slice --memory 2048m --memory-swap 2048m \
             --memory-reservation 2g --cpus 3.75 --pids-limit 4096 --cpu-shares 1024",
        );
        for ceiling in [10, 2048, 5478, 999_999_999] {
            assert_eq!(
                shape(&fourteen, &security(ceiling).unwrap()),
                Ok(()),
                "{ceiling}"
            );
        }
        let ten = security(2048).unwrap();
        for bad in [
            "",
            "--cgroup-parent tehas.slice --memory 2048m",
            "--cgroup-parent other.slice --memory 2048m --memory-swap 2048m \
             --memory-reservation 2g --cpus 3.75 --pids-limit 4096 --cpu-shares 1024",
            "--memory 2048m --cgroup-parent tehas.slice --memory-swap 2048m \
             --memory-reservation 2g --cpus 3.75 --pids-limit 4096 --cpu-shares 1024",
        ] {
            assert_eq!(shape(&words(bad), &ten), Err(Misshaped::Limits), "{bad}");
        }
        let lead = "--cap-drop ALL --cap-add CHOWN --cap-add FOWNER \
                    --security-opt no-new-privileges:true --tmpfs";
        for bad in [
            String::new(),
            "--cap-drop ALL --cap-add CHOWN --cap-add FOWNER \
             --security-opt no-new-privileges:true"
                .to_string(),
            "--cap-drop NET_RAW --cap-add CHOWN --cap-add FOWNER \
             --security-opt no-new-privileges:true \
             --tmpfs /tmp:rw,exec,nosuid,nodev,size=1024m,mode=1777"
                .to_string(),
            format!("{lead} /tmp:rw"),
            format!("{lead} /tmp:rw,exec,size=1024m,mode=1777"),
            format!("{lead} /tmp:rw,exec,nosuid,nodev,size=0m,mode=1777"),
            format!("{lead} /tmp:rw,exec,nosuid,nodev,size=01024m,mode=1777"),
            format!("{lead} /tmp:rw,exec,nosuid,nodev,size=12xm,mode=1777"),
            format!("{lead} /tmp:rw,exec,nosuid,nodev,size=m,mode=1777"),
            format!("{lead} /tmp:rw,exec,nosuid,nodev,size=1024m,mode=1777 --privileged"),
        ] {
            assert_eq!(
                shape(&fourteen, &words(&bad)),
                Err(Misshaped::Security),
                "{bad}"
            );
        }
    }

    // From test/work_caps_test.sh.
    #[test]
    fn the_tmp_tmpfs_is_half_the_ceiling_and_no_ceiling_gives_no_options() {
        for (ceiling, size) in [(2048, "1024m"), (5478, "2739m"), (10, "5m")] {
            let sec = security(ceiling).unwrap();
            assert!(sec[9].contains(&format!("size={size},")), "{ceiling}");
        }
        for ceiling in [0, 1, 9, 1_000_000_000] {
            assert_eq!(security(ceiling), None);
        }
    }

    fn host(max: &str) -> slice::Reading {
        let mut show = serde_json::Map::new();
        show.insert("FragmentPath".into(), "/etc/tehas.slice".into());
        show.insert("ActiveState".into(), "active".into());
        show.insert("MemoryMax".into(), max.into());
        slice::Reading {
            show: Some(show),
            docker: Some("systemd 2".into()),
            controllers: Some("cpu memory pids".into()),
            cpus: Some("16".into()),
            root: "/sys/fs/cgroup".into(),
            ..slice::Reading::default()
        }
    }

    fn load() -> Load {
        Load {
            reserve: "2g".into(),
            containers: "4".into(),
            cycles: "4".into(),
            ..Load::default()
        }
    }

    #[test]
    fn a_good_host_gives_fourteen_words_and_ten() {
        let t = take(&host("8589934592"), &load(), "tehas work", "a cycle").unwrap();
        assert_eq!(t.limits.len(), 14);
        assert_eq!(t.limits[1], "tehas.slice");
        assert_eq!(t.limits[3], "2048m");
        assert_eq!(t.security.len(), 10);
        assert!(t.security[9].contains("size=1024m,"));
    }

    #[test]
    fn a_slice_that_cannot_be_used_says_why_after_the_command() {
        let mut r = host("8589934592");
        r.show = None;
        assert_eq!(
            take(&r, &load(), "tehas work", "a cycle"),
            Err("tehas work: systemctl gave no answer".to_string())
        );
    }

    #[test]
    fn a_concurrency_that_is_no_number_starts_nothing() {
        let mut l = load();
        l.cycles = String::new();
        assert_eq!(
            take(&host("8589934592"), &l, "tehas plan", "a turn"),
            Err("tehas plan: the container limits could not be made, not starting".to_string())
        );
    }

    #[test]
    fn a_ceiling_under_ten_mib_starts_nothing() {
        let mut l = load();
        l.reserve = "6m".into();
        assert_eq!(
            take(&host("33554432"), &l, "tehas plan", "a turn"),
            Err(
                "tehas plan: tehas_sandbox_security gave 0 words, wants 0: not starting a turn"
                    .to_string()
            )
        );
    }
}
