//! `tehas-cycle prepare`: what stands between an approved start and its
//! container. Three steps, because libexec/work holds the issue's text, fetches
//! its attachments and says what a cycle was not given in bash between them, and
//! each refusal keeps its place in that order.
//!
//!   check   the bot's identity, the variables of the container, what the image
//!           supports. Nothing is moved or fetched before it passes.
//!   tree    a fresh workspace, the product's gate, the push check, the code
//!           index and the container's mounts.
//!   record  the cycle's image and settings on its record, and the words of the
//!           docker run, which the watch is given.
//!
//! What one step works out for the next is in state/cycles/<issue>.prepared,
//! under the issue's lock. It holds names of variables, not their values.
//!
//! The workspace is $TEHAS_HOME/work/<name>-<issue>, a fresh clone for every
//! cycle. One a stopped cycle left may hold the only copy of commits that were
//! never pushed: it is moved aside, not removed, no container is given it and
//! the host runs nothing in it. The host runs git only in a tree no container
//! has touched.

use crate::start::{self, is_record, record_value};
use crate::{caps, host, improve};
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tehas_core::sandbox::{self, Identity, Kind, Paths, HANDOFF_SECONDS};
use tehas_core::slice::{self, Load};
use tehas_core::{lock, time};

/// The words of the limits, then of the security options, as the slice gave them.
const LIMIT_WORDS: usize = 14;

const PROTOCOL: [&str; 13] = [
    "cid",
    "oom",
    "forks",
    "stalled",
    "timed",
    "result.json",
    "progress.json",
    "runner_exit",
    "status",
    "nostart",
    "nostart.new",
    "runner_exit.new",
    "status.new",
];

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}

fn say(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

/// A word of one line a program printed, as `$(...)` gives it.
fn printed(mut cmd: Command) -> String {
    let out = cmd.stderr(Stdio::inherit()).stdin(Stdio::null()).output();
    out.map(|o| {
        String::from_utf8_lossy(&o.stdout)
            .trim_end_matches('\n')
            .to_string()
    })
    .unwrap_or_default()
}

fn git_identity(field: &str) -> String {
    let mut git = Command::new("git");
    git.args(["config", "--global", field]);
    printed(git)
}

/// The rule of AGENTS.md: a run with a bound, as timeout runs it.
fn bounded(grace: u64, wait: u64, words: &[String]) -> Command {
    let mut cmd = Command::new("timeout");
    cmd.args(["-k", &grace.to_string(), &wait.to_string()]);
    cmd.args(words);
    cmd.stdin(Stdio::null());
    cmd
}

fn code(status: std::io::Result<std::process::ExitStatus>) -> i32 {
    status.ok().and_then(|s| s.code()).unwrap_or(127)
}

/// The path of the workspace on the host.
fn tree_of(issue: u64) -> String {
    format!("{}-{issue}", env("TEHAS_WORK_DIR"))
}

fn write_state(file: &Path, state: &Value) -> bool {
    let wrote = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(file)
        .and_then(|mut f| f.write_all(state.to_string().as_bytes()));
    wrote.is_ok()
}

fn read_state(file: &Path) -> Option<Value> {
    serde_json::from_slice(&fs::read(file).ok()?).ok()
}

fn list(state: &Value, key: &str) -> Vec<String> {
    let items = state.get(key).and_then(Value::as_array);
    items
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(String::from)
        .collect()
}

/// The deadline of a cycle, in seconds since the epoch: the time after which it
/// starts no new step and parks. None when the limit leaves no room for the
/// handoff before it.
pub fn deadline(now: i64, limit: u64) -> Option<i64> {
    (HANDOFF_SECONDS < limit).then(|| now + limit as i64 - HANDOFF_SECONDS as i64)
}

/// The flags of the runner. A refresh passes --continue-pr: the cycle checks out
/// the issue's open pull request and pushes to the same branch.
pub fn run_flags(limit: u64, restart: bool, refresh: bool) -> Vec<String> {
    let mut flags = vec![
        "--timeout".to_string(),
        limit.div_ceil(60).to_string(),
        "--json".to_string(),
    ];
    if restart {
        flags.push("--restart".to_string());
    }
    if refresh {
        flags.push("--continue-pr".to_string());
    }
    flags
}

/// What `prepare check` is given.
pub struct Check {
    pub issue: u64,
    pub file: PathBuf,
    pub record: String,
    pub restart: bool,
    pub caps_wait: u64,
    pub console: String,
    /// The limits and the security options of the slice.
    pub words: Vec<String>,
}

fn scratch(c: &Check, security: &mut Vec<String>) -> Option<()> {
    let dir = env("TEHAS_SCRATCH_DIR");
    if dir.is_empty() {
        return Some(());
    }
    let work = env("TEHAS_WORK_DIR");
    let name = work.rsplit('/').next().unwrap_or("");
    let out = Command::new(&c.console)
        .args(["clean", "scratch", &dir, &format!("{name}-{}", c.issue)])
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&out.stdout);
    security.truncate(security.len().saturating_sub(2));
    security.extend([
        "-v".to_string(),
        format!("{}:/tmp", path.trim_end_matches('\n')),
    ]);
    Some(())
}

/// The first step. Its exit status.
pub fn check(c: &Check) -> i32 {
    let (name, email) = (git_identity("user.name"), git_identity("user.email"));
    if name.is_empty() || email.is_empty() {
        let mut id = Command::new("id");
        id.arg("-un");
        eprintln!(
            "tehas work: git user.name and user.email are not set for {}",
            printed(id)
        );
        return 64;
    }
    let (limits, security) = c.words.split_at(c.words.len().min(LIMIT_WORDS));
    let mut security = security.to_vec();
    let work = env("TEHAS_WORK_DIR");
    let who = Identity {
        tree: &work,
        name: &name,
        email: &email,
    };
    let kind = Kind::Cycle;
    let mut vars = sandbox::env_options(kind, &who, &sandbox::ensemble(kind), &sandbox::tokens());
    let hosts = sandbox::hosts(&env("PI_ENSEMBLE_HOST_ALIASES"));
    if scratch(c, &mut security).is_none() {
        return 78;
    }
    let Some(verify) = slice::verify(limits, &Load::from_env()) else {
        eprintln!("tehas work: the container limits could not be made, not starting");
        return 78;
    };
    let limit: u64 = env("TEHAS_WORK_TIMEOUT").parse().unwrap_or(0);
    let child: u64 = env("TEHAS_CHILD_TIMEOUT").parse().unwrap_or(0);
    let image = env("PI_ENSEMBLE_IMAGE");
    // What the image supports, asked before anything is moved or fetched, and
    // checked below, once everything the cycle is given is known.
    let asked = bounded(10, c.caps_wait, &sandbox::capabilities(&image))
        .stderr(Stdio::null())
        .output();
    let (answer, caps_rc) = match asked {
        Ok(o) => (
            String::from_utf8_lossy(&o.stdout).into_owned(),
            o.status.code().unwrap_or(127),
        ),
        Err(_) => (String::new(), 127),
    };
    // No step starts after the deadline: the cycle parks with a handoff instead.
    let mut features = "handoffChannel".to_string();
    let last = deadline(time::now(), limit);
    match last {
        Some(_) => features.push_str(" cycleDeadline"),
        None => say(&format!(
            "tehas work: the time limit of {limit}s leaves no room for a deadline before it, the cycle has none"
        )),
    }
    let last = last.map(|s| s * 1000);
    vars.extend(sandbox::cycle_variables(&verify.to_string(), child, last));
    let tree = tree_of(c.issue);
    let works = Path::new(&tree).parent().unwrap_or(Path::new(""));
    if works.exists() && !works.join(".tehas-workspaces").is_file() {
        eprintln!(
            "tehas work: {} exists and was not made by tehas, not touching it",
            works.display()
        );
        return 78;
    }
    let how = is_record(&c.record)
        .then(|| record_value(Path::new(&c.record)))
        .flatten();
    let how = how.as_ref().and_then(|r| r.get("how"));
    let refresh = how.and_then(Value::as_str) == Some("refresh");
    let flags = run_flags(limit, c.restart, refresh);
    if c.restart {
        features.push_str(" restartClearsAttention");
    }
    let mut names = String::new();
    for word in vars.iter().filter(|w| w.starts_with("PI_ENSEMBLE_")) {
        names.push(' ');
        names.push_str(word.split('=').next().unwrap_or(word));
    }
    if env("TEHAS_WORK_IMAGES") == "1" || env("TEHAS_WORK_FILES") == "1" {
        names.push_str(" PI_ENSEMBLE_EXTRA_IMAGE_DIRS");
    }
    let lacking = (caps_rc == 0)
        .then(|| caps::unsupported(&answer, &flags.join(" "), &names, &features))
        .flatten();
    let Some(lacking) = lacking else {
        eprintln!(
            "tehas work: {image} gave no --capabilities answer (exit {caps_rc}), not starting a cycle"
        );
        return 78;
    };
    if !lacking.trim_end_matches('\n').is_empty() {
        eprintln!(
            "tehas work: {image} does not support what this cycle is given, not starting a cycle:"
        );
        for line in lacking.trim_end_matches('\n').lines() {
            eprintln!("    {line}");
        }
        return 78;
    }
    // A workaround the instance carries for a pi-rukas defect ends with the
    // release that fixes it.
    let expired = caps::expired(&answer, &env("TEHAS_RUKAS_WORKAROUNDS"));
    if !expired.is_empty() {
        eprintln!(
            "tehas work: TEHAS_RUKAS_WORKAROUNDS names a workaround this image no longer needs, not starting a cycle:"
        );
        for line in expired {
            eprintln!("    {line}");
        }
        eprintln!(
            "    Remove the setting and its entry from {}/instance.env.",
            env("TEHAS_INSTANCE_DIR")
        );
        return 78;
    }
    let state = json!({"limits": limits, "security": security, "env": vars,
        "hosts": hosts, "flags": flags});
    if !write_state(&c.file, &state) {
        eprintln!("tehas work: could not prepare the sandbox, not starting a cycle");
        return 78;
    }
    0
}

/// What `prepare tree` is given.
pub struct Tree {
    pub issue: u64,
    pub file: PathBuf,
    pub branch: String,
    pub log: PathBuf,
    pub push_wait: u64,
    pub console: String,
}

fn ran(cmd: &mut Command) -> bool {
    cmd.status().is_ok_and(|s| s.success())
}

/// The file or directory of the protocol beside `path`, its name run on.
fn on(path: &Path, end: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(end);
    PathBuf::from(s)
}

/// Replaces what a stopped cycle left beside the workspace with what is in it.
/// Only a directory this command made is ever moved or removed.
fn aside(tree: &str) -> bool {
    if !Path::new(tree).exists() {
        return true;
    }
    let kept = format!("{tree}.before-retry");
    ran(Command::new("rm").args(["-rf", &kept])) && ran(Command::new("mv").args([tree, &kept]))
}

/// The workspace of this cycle, at the tip the product checkout last fetched for
/// the branch. pi-rukas writes the three names into it, and a product needs no
/// .gitignore lines for them.
fn make_tree(tree: &str, state_dir: &str, branch: &str) -> bool {
    let works = Path::new(tree).parent().unwrap_or(Path::new(""));
    let marked = fs::create_dir_all(works).is_ok()
        && fs::File::create(works.join(".tehas-workspaces")).is_ok();
    let exclude = |tree: &str| {
        OpenOptions::new()
            .append(true)
            .create(true)
            .open(format!("{tree}/.git/info/exclude"))
            .and_then(|mut f| {
                f.write_all(b"/.pi/work-state/\n/.pi/permissions.json\n/.pi/decisions.json\n")
            })
            .is_ok()
    };
    marked
        && aside(tree)
        && ran(Command::new("rm").args(["-rf", state_dir]))
        && sandbox::clone(&env("TEHAS_WORK_REPO"), tree, branch)
        && exclude(tree)
}

/// The first line that is no comment and not empty: what a product names as its
/// gate in .pi/verify-cmd.
pub fn gate_line(text: &str) -> Option<&str> {
    text.lines()
        .find(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
}

/// Whether a product names a gate, or holds a stack pi-rukas can guess one of.
/// Only the commit the clone was made at is read, so nothing uncommitted can
/// park a cycle or start one. A start-cost guard, not proof a real gate exists.
fn has_gate(tree: &str, branch: &str) -> bool {
    let at = format!("origin/{branch}");
    let git = |args: &[&str]| {
        let mut words = vec!["git".to_string(), "-C".to_string(), tree.to_string()];
        words.extend(args.iter().map(|a| a.to_string()));
        let out = bounded(10, 60, &words).stderr(Stdio::null()).output();
        out.map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default()
    };
    gate_line(&git(&["show", &format!("{at}:.pi/verify-cmd")])).is_some()
        || !git(&[
            "ls-tree",
            "--name-only",
            &at,
            "--",
            "package.json",
            "Cargo.toml",
        ])
        .is_empty()
}

/// Can the sandbox push at all? The client passes the timeout's TERM on to git
/// in the container. None when it can; else the status to exit with.
fn push_check(t: &Tree, state: &Value, tree: &str, preflight: &Path) -> Option<i32> {
    let image = env("PI_ENSEMBLE_IMAGE");
    let words = sandbox::push_check(
        &list(state, "security"),
        &sandbox::user(),
        &list(state, "env"),
        &env("TEHAS_WORK_DIR"),
        tree,
        &image,
    );
    let log = fs::File::create(preflight).ok()?;
    let err = log.try_clone().ok()?;
    let rc = code(
        bounded(10, t.push_wait, &words)
            .stdout(log)
            .stderr(err)
            .status(),
    );
    if rc == 0 {
        return None;
    }
    if matches!(rc, 124 | 137) {
        let line = format!("the push check did not end within {}s", t.push_wait);
        let _ = append(preflight, &line);
    }
    say("tehas work: the sandbox cannot push to origin, not starting a cycle:");
    let text = fs::read(preflight).unwrap_or_default();
    let text = String::from_utf8_lossy(&text);
    let lines: Vec<&str> = text.lines().collect();
    for line in &lines[lines.len().saturating_sub(3)..] {
        say(&format!("    {line}"));
    }
    Some(3)
}

fn append(file: &Path, line: &str) -> bool {
    let done = OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)
        .and_then(|mut f| writeln!(f, "{line}"));
    done.is_ok()
}

/// The mounts, with what they name made on the host, and the container's own
/// models.json on the endpoint picked now. None when it could not be done, said.
fn mounts(t: &Tree, tree: &str, state_dir: &str, runs: &str) -> Option<Vec<String>> {
    let work = env("TEHAS_WORK_DIR");
    let paths = Paths {
        tree: &work,
        host: tree,
        state: state_dir,
        runs,
    };
    let words = match sandbox::prepared_mounts(Kind::Cycle, &paths) {
        Ok(w) => w,
        Err(why) => {
            if !why.is_empty() {
                eprintln!("{why}");
            }
            return None;
        }
    };
    let models = format!("{state_dir}/models.json");
    let endpoints = format!("{state_dir}/endpoints");
    ran(Command::new(&t.console).args(["improve", "models", &models, &endpoints])).then_some(words)
}

/// The second step. Its exit status.
pub fn tree(home: &Path, t: &Tree) -> i32 {
    let Some(mut state) = read_state(&t.file) else {
        eprintln!("tehas work: could not prepare the sandbox, not starting a cycle");
        return 78;
    };
    let tree = tree_of(t.issue);
    let state_dir = format!("{tree}.state");
    if !make_tree(&tree, &state_dir, &t.branch) {
        eprintln!("tehas work: could not make the workspace {tree}, not starting a cycle");
        return 78;
    }
    if !has_gate(&tree, &t.branch) {
        eprintln!(
            "tehas work: {} has no .pi/verify-cmd and no package.json or Cargo.toml in origin/{}, so a cycle could verify nothing",
            env("TEHAS_REPO"),
            t.branch
        );
        eprintln!(
            "tehas work: {tree} is left in place with what it holds, and {tree}.before-retry holds what the last cycle left, if any"
        );
        return 78;
    }
    let preflight = on(&t.log, ".preflight");
    if let Some(rc) = push_check(t, &state, &tree, &preflight) {
        return rc;
    }
    let retry = sandbox::retry_line(&tree);
    let _ = append(&preflight, &retry);
    say(&format!("tehas work: {retry}"));
    // Asked for only once this cycle can start: a cycle that never began leaves
    // no copy behind. A copy that fails is not worth stopping a cycle for.
    let index = home.join("state/index");
    if !sandbox::seed(&state_dir, &index.to_string_lossy()) {
        say(&format!(
            "tehas work: the code index could not be copied into {state_dir}/cache, this cycle searches without one"
        ));
    }
    let runs = format!("{}/.pi/agent/ensemble-runs/issue-{}", env("HOME"), t.issue);
    let Some(words) = mounts(t, &tree, &state_dir, &runs) else {
        eprintln!("tehas work: could not prepare the sandbox, not starting a cycle");
        return 78;
    };
    state["mounts"] = json!(words);
    let endpoints = fs::read_to_string(format!("{state_dir}/endpoints")).unwrap_or_default();
    for provider in endpoints
        .lines()
        .filter_map(|l| l.split_whitespace().next())
    {
        let line = format!("model endpoint: provider {provider} is on its backup");
        let _ = append(&preflight, &line);
        say(&format!("tehas work: {line}"));
    }
    if !write_state(&t.file, &state) {
        eprintln!("tehas work: could not prepare the sandbox, not starting a cycle");
        return 78;
    }
    0
}

/// What `prepare record` is given.
pub struct Record {
    pub issue: u64,
    pub file: PathBuf,
    pub attempt: String,
    pub log: PathBuf,
    pub name: String,
    pub stop_wait: u64,
    pub text_wait: u64,
    pub gap: String,
    /// The issue's images, as -v and -e options.
    pub images: Vec<String>,
}

/// How many cycles hold their lock, this one with them.
fn running(home: &Path) -> usize {
    let held = host::store(home, &["locks-held"]).ok();
    let held = held
        .as_ref()
        .and_then(|v| v.get("held")?.as_array().map(Vec::len));
    held.unwrap_or_else(|| lock::held_in(&home.join("state/cycles")).len())
}

/// The id of the image, and the pi-rukas commit of its label when it has one.
fn inspect(r: &Record) -> Option<(String, Option<String>)> {
    let words: Vec<String> = [
        "docker",
        "image",
        "inspect",
        "--format",
        "{{.Id}} {{index .Config.Labels \"tehas.rukas\"}}",
        &env("PI_ENSEMBLE_IMAGE"),
    ]
    .iter()
    .map(|w| w.to_string())
    .collect();
    let out = bounded(5, r.stop_wait, &words)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut fields = text.split_whitespace();
    let id = fields.next()?;
    let hex = |s: &str| s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    let digest = id.strip_prefix("sha256:")?;
    if digest.len() != 64 || !hex(digest) {
        return None;
    }
    let commit = fields
        .next()
        .filter(|c| (7..=40).contains(&c.len()) && hex(c));
    Some((id.to_string(), commit.map(String::from)))
}

/// The image the cycle runs, as the container starts: a rebuild or a new pin
/// while the cycle runs does not change what it ran. On the cycle's record with
/// what the container is given of the settings, and how many cycles run now. A
/// record that is not this start's, or has ended, is no start to run; a store
/// that is only busy costs the record, not the cycle. False stops the start.
fn on_record(home: &Path, r: &Record, state: &Value) -> bool {
    let Some((id, commit)) = inspect(r) else {
        return true;
    };
    let n = r.issue.to_string();
    let mut args = vec!["cycle-image", &n, "--attempt", &r.attempt, "--image", &id];
    if let Some(c) = &commit {
        args.extend(["--rukas", c]);
    }
    let mut options = list(state, "env");
    options.extend_from_slice(&r.images);
    let text = improve::settings(&running(home).to_string(), &options);
    let settings = start::private_file(home, "settings", format!("{text}\n").as_bytes());
    let path = settings.as_ref().map(|p| p.to_string_lossy().into_owned());
    match &path {
        Some(p) => args.extend(["--settings", p]),
        None => {
            eprintln!("tehas work: the settings of this cycle could not be written for its record")
        }
    }
    let done = host::store_said(home, &args);
    if let Some(p) = &settings {
        let _ = fs::remove_file(p);
    }
    let Err(refused) = done else {
        return true;
    };
    if matches!(refused.word.as_str(), "busy" | "timeout") {
        eprintln!(
            "tehas work: the image of #{n} is not on its record: the state store did not answer ({})",
            refused.word
        );
        return true;
    }
    eprintln!(
        "tehas work: the state store did not take the image of this start ({}), not starting a cycle",
        refused.line
    );
    start::tell(
        r.issue,
        &env("TEHAS_REPO"),
        r.text_wait,
        "its record on this host could not be updated as it started",
    );
    false
}

/// The third step. Its exit status. Prints the words of the docker run, each
/// ended by a NUL.
pub fn record(home: &Path, r: &Record) -> i32 {
    let Some(state) = read_state(&r.file) else {
        eprintln!("tehas work: could not prepare the sandbox, not starting a cycle");
        return 78;
    };
    let cycles = home.join("state/cycles");
    let file = |name: &str| cycles.join(format!("{}.{name}", r.issue));
    // Docker refuses a cid file that is already there. One is left only by a
    // cycle that was killed outright, and so is a count of kills.
    for name in PROTOCOL {
        let _ = fs::remove_file(file(name));
    }
    // A container under this name outlived the client that started it: this
    // command holds the issue's lock, so no cycle owns it. It would work on with
    // the bot's token, and Docker would refuse the name.
    let words = ["docker", "rm", "-f", &r.name].map(String::from);
    let _ = bounded(10, r.stop_wait, &words)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if !r.attempt.is_empty() && !on_record(home, r, &state) {
        return 4;
    }
    // A log is named to the second, so a cycle of this issue can be given the
    // name one before it used: what err.txt of the record copies is only what
    // this cycle writes into it. The line of the attachments' gap is one of them.
    let err = on(&r.log, ".err");
    let _ = fs::remove_file(&err);
    if !r.gap.is_empty() && !append(&err, &r.gap) {
        eprintln!("tehas work: could not write {}", err.display());
    }
    let cid = file("cid").to_string_lossy().into_owned();
    let run = sandbox::cycle_run(&sandbox::CycleRun {
        name: &r.name,
        limits: &list(&state, "limits"),
        security: &list(&state, "security"),
        user: &sandbox::user(),
        cid_file: &cid,
        tree: &env("TEHAS_WORK_DIR"),
        mounts: &list(&state, "mounts"),
        images: &r.images,
        env: &list(&state, "env"),
        hosts: &list(&state, "hosts"),
        image: &env("PI_ENSEMBLE_IMAGE"),
        issue: r.issue,
        flags: &list(&state, "flags"),
    });
    let mut out = std::io::stdout().lock();
    for word in run {
        if out.write_all(word.as_bytes()).is_err() || out.write_all(b"\0").is_err() {
            return 78;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_deadline_keeps_the_handoff_clear_of_the_limit() {
        for (limit, want) in [
            (21600, Some(1000 + 21600 - 480)),
            (481, Some(1001)),
            (480, None),
            (479, None),
            (1, None),
            (0, None),
        ] {
            assert_eq!(deadline(1000, limit), want, "{limit}");
        }
    }

    #[test]
    fn the_runner_is_given_whole_minutes_at_least_one() {
        assert_eq!(
            run_flags(21600, false, false),
            ["--timeout", "360", "--json"]
        );
        assert_eq!(run_flags(1, false, false)[1], "1");
        assert_eq!(run_flags(61, false, false)[1], "2");
        assert_eq!(
            run_flags(60, true, true),
            ["--timeout", "1", "--json", "--restart", "--continue-pr"]
        );
    }

    #[test]
    fn a_gate_is_the_first_line_that_says_something() {
        assert_eq!(
            gate_line("# a comment\n\n  \nmake gate\nother\n"),
            Some("make gate")
        );
        assert_eq!(gate_line("  # indented\n\t\n"), None);
        assert_eq!(gate_line(""), None);
    }

    #[test]
    fn a_tree_moved_aside_replaces_the_one_before_it() {
        let d = std::env::temp_dir().join(format!("tehas-prepare-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d.join("w")).unwrap();
        fs::write(d.join("w/new"), "x").unwrap();
        fs::create_dir_all(d.join("w.before-retry")).unwrap();
        fs::write(d.join("w.before-retry/old"), "x").unwrap();
        let tree = d.join("w");
        assert!(aside(tree.to_str().unwrap()));
        assert!(!tree.exists());
        assert!(d.join("w.before-retry/new").exists());
        assert!(!d.join("w.before-retry/old").exists());
        assert!(aside(tree.to_str().unwrap()));
        assert!(d.join("w.before-retry/new").exists());
        let _ = fs::remove_dir_all(&d);
    }
}
