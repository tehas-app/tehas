//! What stands between an approved start and its container, in three steps of
//! `tehas-cycle work` (src/work.rs). It holds the issue's text, fetches its
//! attachments and says what a cycle was not given between them, and each
//! refusal keeps its place in that order.
//!
//!   check   the bot's identity, the variables of the container, what the image
//!           supports. Nothing is moved or fetched before it passes.
//!   tree    a fresh workspace, the product's gate, the push check, the
//!           product's install step, the code index and the container's mounts.
//!   record  the cycle's image and settings on its record, and the words of the
//!           docker run, which the watch is given.
//!
//! What one step works out for the next is in state/cycles/<issue>.prepared,
//! under the issue's lock. It holds names of variables, not their values.
//!
//! A stop does not end the step in hand. It goes on with the command it is in
//! and starts no other once a stop came ([`Stop`]), so a stop costs one command.
//!
//! The workspace is $TEHAS_HOME/work/<name>-<issue>, a fresh clone for every
//! cycle. One a stopped cycle left may hold the only copy of commits that were
//! never pushed: it is moved aside, not removed, no container is given it and
//! the host runs nothing in it. The host runs git only in a tree no container
//! has touched.

use crate::start::{self, is_record, record_value, Driver};
use crate::{caps, conflict, host, improve};
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use tehas_core::install::{self, Step};
use tehas_core::models;
use tehas_core::sandbox::{self, Identity, Kind, Paths, HANDOFF_SECONDS};
use tehas_core::slice::{self, Load};
use tehas_core::{lock, time};

/// The words of the limits, then of the security options, as the slice gave them.
const LIMIT_WORDS: usize = 14;

/// What of the protocol a killed cycle leaves as its evidence.
const EVIDENCE: [&str; 7] = ["cid", "oom", "forks", "stalled", "timed", "signal", "disk"];

const PROTOCOL: [&str; 15] = [
    "cid",
    "oom",
    "forks",
    "stalled",
    "timed",
    "signal",
    "disk",
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

/// The exit status of a start that was stopped: 128 plus TERM.
pub const STOPPED: i32 = 143;

/// A TERM, INT or HUP that came to tehas work, as its handlers set it, and
/// the number of the last one.
#[derive(Clone, Default)]
pub struct Stop(Arc<AtomicBool>, Arc<AtomicUsize>);

impl Stop {
    /// The flag the signal handlers set and the watch reads.
    pub fn flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.0)
    }

    /// The number of the signal that came last, 0 before one.
    pub fn which(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.1)
    }

    /// Whether a stop came, said when it did.
    pub fn came(&self) -> bool {
        let came = self.0.load(Ordering::SeqCst);
        if came {
            eprintln!("tehas work: stopped, not going on with the start");
        }
        came
    }
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
    pub driver: Driver,
    /// state/cycles/<issue>.lean-refused: left when the image cannot run lean.
    pub lean_note: PathBuf,
    pub caps_wait: u64,
    pub console: String,
    /// The limits and the security options of the slice.
    pub words: Vec<String>,
    pub stop: Stop,
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
    let hosts = match sandbox::network(&env("PI_ENSEMBLE_HOST_ALIASES")) {
        Ok(hosts) => hosts,
        Err(why) => {
            eprintln!("tehas work: {why}, not starting a cycle");
            return 78;
        }
    };
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
    // Lean names the same two features its own way.
    let lean = c.driver == Driver::Lean;
    let (channel, by_deadline) = if lean {
        ("leanHandoffHost", " leanDeadline")
    } else {
        ("handoffChannel", " cycleDeadline")
    };
    let mut features = channel.to_string();
    let last = deadline(time::now(), limit);
    match last {
        Some(_) => features.push_str(by_deadline),
        None => say(&format!(
            "tehas work: the time limit of {limit}s leaves no room for a deadline before it, the cycle has none"
        )),
    }
    let last = last.map(|s| s * 1000);
    vars.extend(sandbox::cycle_variables(&verify.to_string(), child, last));
    // Whether the image's runner reads a retry's comments, for the boxes of a
    // park of this cycle; the polyfill's label is read with the record. The
    // polyfill does not reach the repeat-park guard, so a comments retry it
    // alone carries runs without the guard where the image takes that input.
    let native = caps_rc == 0 && caps::mode(&answer, "issueComments", None).0 == "native";
    let guard = "PI_ENSEMBLE_REPEAT_PARK";
    let unguard = !native
        && crate::gives::comments_retry(&c.record)
        && caps::unsupported(&answer, c.driver.verb(), "", guard, "").as_deref() == Some("");
    let tree = tree_of(c.issue);
    let works = Path::new(&tree).parent().unwrap_or(Path::new(""));
    if works.exists() && !works.join(".tehas-workspaces").is_file() {
        eprintln!(
            "tehas work: {} exists and was not made by tehas, not touching it",
            works.display()
        );
        return 78;
    }
    let rec = is_record(&c.record)
        .then(|| record_value(Path::new(&c.record)))
        .flatten();
    let how = rec.as_ref().and_then(|r| r.get("how"));
    let refresh = how.and_then(Value::as_str) == Some("refresh");
    // A refresh on an image that cannot verify its merge is first asked
    // whether it conflicts (src/conflict.rs).
    let judge = refresh && caps::mode(&answer, conflict::VERIFIES_MERGE, None).0 != "native";
    let refresh_of = rec
        .as_ref()
        .and_then(|r| r.get("refresh_of"))
        .filter(|_| judge)
        .map(|of| json!({"pr": of.get("pr"), "head": of.get("head_sha")}))
        .unwrap_or(Value::Null);
    // A lean restart or refresh is refused below unless the image lists the
    // flag for work-lean.
    let flags = run_flags(limit, c.restart, refresh);
    if c.restart && !lean {
        features.push_str(" restartClearsAttention");
    }
    let verb = c.driver.verb();
    let mut names = String::new();
    for word in vars.iter().filter(|w| w.starts_with("PI_ENSEMBLE_")) {
        names.push(' ');
        names.push_str(word.split('=').next().unwrap_or(word));
    }
    if env("TEHAS_WORK_IMAGES") == "1"
        || env("TEHAS_WORK_FILES") == "1"
        || crate::gives::comments_given(&c.record)
    {
        names.push_str(" PI_ENSEMBLE_EXTRA_IMAGE_DIRS");
    }
    let lacking = (caps_rc == 0)
        .then(|| caps::unsupported(&answer, verb, &flags.join(" "), &names, &features))
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
        if lean {
            // tehasbot cycle parks a start that never ran, and classic does not
            // run in its place.
            let _ = fs::File::create(&c.lean_note);
            eprintln!("    (driver lean, {verb}; it does not run classic in its place)");
        }
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
    if c.stop.came() {
        return STOPPED;
    }
    let state = json!({"limits": limits, "security": security, "env": vars,
        "hosts": hosts, "flags": flags, "verb": verb,
        "comments": {"native": native, "unguard": unguard}, "refresh": refresh_of});
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
    pub stop: Stop,
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
    let (moved, lines) = match swap_aside(tree) {
        Ok(lines) => (true, lines),
        Err(lines) => (false, lines),
    };
    lines.iter().for_each(|l| say(l));
    moved
}

/// Moves `tree` to `<tree>.before-retry`. The tree already there is first
/// renamed out of the way and removed only once `tree` is in its place; when
/// that move fails it is put back. The lines say what was replaced or kept.
fn swap_aside(tree: &str) -> Result<Vec<String>, Vec<String>> {
    let kept = format!("{tree}.before-retry");
    let old = format!("{kept}.old");
    if !Path::new(&kept).exists() {
        return fs::rename(tree, &kept).map(|()| Vec::new()).map_err(|e| {
            vec![format!(
                "tehas work: could not move {tree} aside to {kept}: {e}"
            )]
        });
    }
    let mut lines = Vec::new();
    // Left only by a start killed between the two renames below.
    if Path::new(&old).exists() {
        if !ran(Command::new("rm").args(["-rf", &old])) {
            lines.push(format!(
                "tehas work: could not remove {old}, left by an earlier start"
            ));
            return Err(lines);
        }
        lines.push(format!(
            "tehas work: removed {old}, left by an earlier start"
        ));
    }
    if let Err(e) = fs::rename(&kept, &old) {
        lines.push(format!(
            "tehas work: could not move {kept} out of the way: {e}"
        ));
        return Err(lines);
    }
    if let Err(e) = fs::rename(tree, &kept) {
        lines.push(format!(
            "tehas work: could not move {tree} aside to {kept}: {e}"
        ));
        if let Err(e) = fs::rename(&old, &kept) {
            lines.push(format!(
                "tehas work: could not put {old} back at {kept}: {e}"
            ));
        }
        return Err(lines);
    }
    if !ran(Command::new("rm").args(["-rf", &old])) {
        // Both trees go back where they were and the start stops: a tree
        // under a name no clean knows would be kept for good.
        lines.push(format!("tehas work: could not remove {old}"));
        let back = fs::rename(&kept, tree).and_then(|()| fs::rename(&old, &kept));
        if let Err(e) = back {
            lines.push(format!(
                "tehas work: could not put the workspaces back at {tree} and {kept}: {e}"
            ));
        }
        return Err(lines);
    }
    lines.push(format!(
        "tehas work: replaced what the cycle before last left at {kept} with what the last cycle left"
    ));
    Ok(lines)
}

/// The workspace of the cycle before, moved aside by this start. A start that
/// ends before its container runs drops it, and the workspace is put back in
/// place of this start's clone: the clone holds no commits, and the end of the
/// cycle removes what is under the name beside.
#[derive(Default)]
pub struct Aside {
    tree: Option<String>,
}

impl Aside {
    /// aside(), what it moves held here.
    fn take(&mut self, tree: &str) -> bool {
        let before = Path::new(tree).exists();
        if !aside(tree) {
            return false;
        }
        if before {
            self.tree = Some(tree.to_string());
        }
        true
    }

    /// The container runs: what was moved aside stays there.
    pub fn started(mut self) {
        self.tree = None;
    }
}

impl Drop for Aside {
    fn drop(&mut self) {
        let Some(tree) = self.tree.take() else {
            return;
        };
        if !put_back(&tree) {
            say(&format!(
                "tehas work: could not put the workspace of the last cycle back at {tree}, it is at {tree}.before-retry"
            ));
        }
    }
}

/// The kept workspace in place of the clone beside it.
fn put_back(tree: &str) -> bool {
    let kept = format!("{tree}.before-retry");
    Path::new(&kept).is_dir()
        && ran(Command::new("rm").args(["-rf", tree]))
        && ran(Command::new("mv").args([&kept, tree]))
}

/// The workspace of this cycle, at the tip the product checkout last fetched for
/// the branch. pi-rukas writes the three names into it, and a product needs no
/// .gitignore lines for them.
fn make_tree(tree: &str, state_dir: &str, branch: &str, set: &mut Aside) -> bool {
    let works = Path::new(tree).parent().unwrap_or(Path::new(""));
    let marked = fs::create_dir_all(works).is_ok()
        && fs::File::create(works.join(".tehas-workspaces")).is_ok();
    if !marked || !set.take(tree) {
        return false;
    }
    let exclude = |tree: &str| {
        OpenOptions::new()
            .append(true)
            .create(true)
            .open(format!("{tree}/.git/info/exclude"))
            .and_then(|mut f| {
                f.write_all(b"/.pi/work-state/\n/.pi/lean-state/\n/.pi/permissions.json\n/.pi/decisions.json\n")
            })
            .is_ok()
    };
    ran(Command::new("rm").args(["-rf", state_dir]))
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
    let git = |args: &[&str]| git_out(tree, args);
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

fn git_out(tree: &str, args: &[&str]) -> String {
    let mut words = vec!["git".to_string(), "-C".to_string(), tree.to_string()];
    words.extend(args.iter().map(|a| a.to_string()));
    let out = bounded(10, 60, &words).stderr(Stdio::null()).output();
    out.map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// The product's dependencies, installed in the workspace in the sandbox
/// before the cycle starts, so a worktree pi-rukas makes there finds them in
/// its main checkout. None when they were; else the status to exit with.
fn install(state: &Value, tree: &str, steps: &[Step], preflight: &Path) -> Option<i32> {
    let words = sandbox::install_run(
        &[list(state, "limits"), list(state, "security")].concat(),
        &sandbox::user(),
        &list(state, "hosts"),
        &env("TEHAS_WORK_DIR"),
        tree,
        &env("PI_ENSEMBLE_IMAGE"),
        &install::script(steps),
    );
    // What the install says is read back from where it starts, so a failure is
    // told by its own lines and not the push check's before it.
    let start = fs::metadata(preflight).map_or(0, |m| m.len());
    let opened = OpenOptions::new()
        .append(true)
        .create(true)
        .open(preflight)
        .and_then(|log| Ok((log.try_clone()?, log)));
    let rc = match opened {
        Ok((err, log)) => code(
            bounded(10, install::WAIT, &words)
                .stdout(log)
                .stderr(err)
                .status(),
        ),
        Err(_) => 127,
    };
    if rc == 0 {
        return None;
    }
    let text = fs::read(preflight).unwrap_or_default();
    let own = usize::try_from(start).map_or(&[][..], |at| text.get(at..).unwrap_or(&[]));
    let text = String::from_utf8_lossy(own);
    let lacking = install::missing(&text);
    if rc == install::MISSING && !lacking.is_empty() {
        for tool in lacking {
            say(&format!(
                "tehas work: no {tool} in the sandbox image, which the product's install needs; not starting a cycle"
            ));
        }
        return Some(78);
    }
    if matches!(rc, 124 | 137) {
        let line = format!("the install did not end within {}s", install::WAIT);
        to_preflight(preflight, &line);
    }
    say(&format!(
        "tehas work: the product's install failed (exit {rc}), not starting a cycle:"
    ));
    let lines: Vec<&str> = text.lines().collect();
    for line in &lines[lines.len().saturating_sub(5)..] {
        say(&format!("    {line}"));
    }
    Some(78)
}

/// Can the sandbox push at all? The client passes the timeout's TERM on to git
/// in the container. None only when the check ran and passed; else the status
/// to exit with.
fn push_check(t: &Tree, state: &Value, tree: &str, preflight: &Path) -> Option<i32> {
    let image = env("PI_ENSEMBLE_IMAGE");
    let words = sandbox::push_check(
        &list(state, "security"),
        &sandbox::user(),
        &list(state, "env"),
        &list(state, "hosts"),
        &env("TEHAS_WORK_DIR"),
        tree,
        &image,
    );
    let opened = fs::File::create(preflight).and_then(|log| Ok((log.try_clone()?, log)));
    let Ok((err, log)) = opened else {
        say("tehas work: the sandbox cannot push to origin, not starting a cycle:");
        say(&format!(
            "    the log of the push check could not be made at {}",
            preflight.display()
        ));
        return Some(3);
    };
    let rc = code(
        sandbox::give_cycle_token(&mut bounded(10, t.push_wait, &words))
            .stdout(log)
            .stderr(err)
            .status(),
    );
    if rc == 0 {
        return None;
    }
    if matches!(rc, 124 | 137) {
        let line = format!("the push check did not end within {}s", t.push_wait);
        to_preflight(preflight, &line);
    }
    say("tehas work: the sandbox cannot push to origin, not starting a cycle:");
    for line in last_lines(preflight, 3, &format!("the push check (exit {rc})")) {
        say(&format!("    {line}"));
    }
    Some(3)
}

/// The last `n` lines of the log, or a line that says it could not be read or
/// that `what` wrote nothing into it.
fn last_lines(log: &Path, n: usize, what: &str) -> Vec<String> {
    let text = match fs::read(log) {
        Ok(t) => t,
        Err(e) => return vec![format!("its log {} could not be read: {e}", log.display())],
    };
    let text = String::from_utf8_lossy(&text);
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
    if lines.is_empty() {
        return vec![format!("{what} wrote nothing into {}", log.display())];
    }
    lines[lines.len().saturating_sub(n)..].to_vec()
}

/// A line of the preflight log; a line that cannot be written there is said.
fn to_preflight(preflight: &Path, line: &str) {
    if !append(preflight, line) {
        eprintln!(
            "tehas work: could not write to {}: {line}",
            preflight.display()
        );
    }
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
fn mounts(tree: &str, state_dir: &str, runs: &str) -> Option<Vec<String>> {
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
    let state = Path::new(state_dir);
    models::write_for_container(&state.join("models.json"), &state.join("endpoints"))
        .map(|()| words)
}

/// A refresh the image cannot finish when it conflicts: Some(78) when it does,
/// with the files in state/cycles/<issue>.refresh-conflict. A merge git
/// cannot judge runs the refresh, as before.
fn refresh_conflict(home: &Path, t: &Tree, state: &Value, tree: &str) -> Option<i32> {
    let of = state.get("refresh").filter(|v| v.is_object())?;
    let pr = of.get("pr").and_then(Value::as_u64).unwrap_or(0);
    let head = of.get("head").and_then(Value::as_str).unwrap_or("");
    let Some(paths) = conflict::conflicts(tree, &t.branch, head) else {
        say(&format!(
            "tehas work: whether pull request #{pr} conflicts with origin/{} could not be read, the refresh runs",
            t.branch
        ));
        return None;
    };
    if paths.is_empty() {
        return None;
    }
    let file = home.join(format!("state/cycles/{}.{}", t.issue, conflict::NOTE));
    if fs::write(&file, conflict::note(pr, &t.branch, &paths)).is_err() {
        eprintln!(
            "tehas work: could not write {}, not starting a cycle",
            file.display()
        );
        return Some(78);
    }
    eprintln!(
        "tehas work: pull request #{pr} conflicts with origin/{} in {} file(s), and the image's pi-rukas cannot finish such a refresh (no feature {}), not starting a cycle:",
        t.branch,
        paths.len(),
        conflict::VERIFIES_MERGE
    );
    for p in paths.iter().take(20) {
        eprintln!("    {}", p.escape_debug());
    }
    Some(78)
}

/// The second step. Its exit status. What it moves aside is in `set`.
pub fn tree(home: &Path, t: &Tree, set: &mut Aside) -> i32 {
    let Some(mut state) = read_state(&t.file) else {
        eprintln!("tehas work: could not prepare the sandbox, not starting a cycle");
        return 78;
    };
    let tree = tree_of(t.issue);
    let state_dir = format!("{tree}.state");
    if t.stop.came() {
        return STOPPED;
    }
    if !make_tree(&tree, &state_dir, &t.branch, set) {
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
            "tehas work: {tree} holds what the last cycle left, or this start's clone when it left none, and {tree}.before-retry what the cycle before it left, if any"
        );
        return 78;
    }
    if let Some(rc) = refresh_conflict(home, t, &state, &tree) {
        return rc;
    }
    if t.stop.came() {
        return STOPPED;
    }
    let preflight = on(&t.log, ".preflight");
    let Some(steps) = install::steps(&tree, "HEAD") else {
        eprintln!("tehas work: could not list the files of {tree}, not starting a cycle");
        return 78;
    };
    if let Some(rc) = push_check(t, &state, &tree, &preflight) {
        return rc;
    }
    if !steps.is_empty() {
        if t.stop.came() {
            return STOPPED;
        }
        for step in &steps {
            let line = format!("install {}", step.line());
            to_preflight(&preflight, &line);
            say(&format!("tehas work: {line}"));
        }
        if let Some(rc) = install(&state, &tree, &steps, &preflight) {
            return rc;
        }
    }
    if t.stop.came() {
        return STOPPED;
    }
    let retry = sandbox::retry_line(&tree);
    to_preflight(&preflight, &retry);
    say(&format!("tehas work: {retry}"));
    // Asked for only once this cycle can start: a cycle that never began leaves
    // no copy behind. A copy that fails is not worth stopping a cycle for.
    let index = home.join("state/index");
    if !sandbox::seed(&state_dir, &index.to_string_lossy()) {
        say(&format!(
            "tehas work: the code index could not be copied into {state_dir}/cache, this cycle searches without one"
        ));
    }
    if t.stop.came() {
        return STOPPED;
    }
    let runs = format!("{}/.pi/agent/ensemble-runs/issue-{}", env("HOME"), t.issue);
    let Some(words) = mounts(&tree, &state_dir, &runs) else {
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
        to_preflight(&preflight, &line);
        say(&format!("tehas work: {line}"));
    }
    if t.stop.came() {
        return STOPPED;
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
    pub stop: Stop,
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
    image_of(&String::from_utf8_lossy(&out.stdout))
}

/// The id and the commit in the line `inspect` printed. None for anything that
/// is not an image id.
fn image_of(text: &str) -> Option<(String, Option<String>)> {
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
/// record that is not this start's, or has ended, is no start to run. A busy
/// store is asked once more a second later; one still busy costs the record,
/// not the cycle. False stops the start.
fn on_record(home: &Path, r: &Record, state: &Value, seen: &Seen) -> bool {
    let Some((id, commit)) = seen else {
        return true;
    };
    let n = r.issue.to_string();
    let mut args = vec!["cycle-image", &n, "--attempt", &r.attempt, "--image", id];
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
    let done = host::store_try(home, 2, &args);
    if let Some(p) = &settings {
        let _ = fs::remove_file(p);
    }
    let Err(refused) = done else {
        return true;
    };
    // The record has moved on under a stop: nothing to tell the issue.
    if r.stop.came() {
        return false;
    }
    if matches!(refused.word.as_str(), "busy" | "timeout") {
        eprintln!(
            "tehas work: the image of #{n} is not on its record: the state store did not answer twice ({}), the container runs {id} all the same",
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

/// What `inspect` found of the image: its id and the commit of its label.
type Seen = Option<(String, Option<String>)>;

/// The image the container runs: the id the start inspected and put on its
/// record, so a rebuild after the inspect cannot start an image the record does
/// not name. The tag when nothing was inspected.
fn run_image(seen: &Seen, tag: &str) -> String {
    match seen {
        Some((id, _)) if id.starts_with("sha256:") => id.clone(),
        _ => tag.to_string(),
    }
}

/// The third step: the words of the docker run, or the exit status.
pub fn record(home: &Path, r: &Record) -> Result<Vec<String>, i32> {
    let Some(state) = read_state(&r.file) else {
        eprintln!("tehas work: could not prepare the sandbox, not starting a cycle");
        return Err(78);
    };
    let cycles = home.join("state/cycles");
    let file = |name: &str| cycles.join(format!("{}.{name}", r.issue));
    if r.stop.came() {
        return Err(STOPPED);
    }
    // Docker refuses a cid file that is already there. One is left only by a
    // cycle that was killed outright, and so is a count of kills: said before
    // it goes.
    for line in wipe_protocol(&file, r.issue) {
        eprintln!("{line}");
    }
    // A container under this name outlived the client that started it: this
    // command holds the issue's lock, so no cycle owns it. It would work on with
    // the bot's token, and Docker would refuse the name.
    let words = ["docker", "rm", "-f", &r.name].map(String::from);
    let out = bounded(10, r.stop_wait, &words).output();
    if let Some(line) = removed_said(&r.name, &out) {
        eprintln!("{line}");
    }
    if r.stop.came() {
        return Err(STOPPED);
    }
    // Inspected once: the id the record gets is the one the container runs.
    let seen = (!r.attempt.is_empty()).then(|| inspect(r)).flatten();
    if !r.attempt.is_empty() && !on_record(home, r, &state, &seen) {
        return Err(4);
    }
    let image = run_image(&seen, &env("PI_ENSEMBLE_IMAGE"));
    let mut env_words = list(&state, "env");
    let mode = if state.pointer("/comments/native") == Some(&Value::Bool(true)) {
        "native"
    } else if crate::gives::polyfill_label(&image) {
        "polyfill"
    } else {
        "off"
    };
    if fs::write(file("comments"), format!("{mode}\n")).is_err() {
        // What is left there is the word of a cycle before.
        let _ = fs::remove_file(file("comments"));
        eprintln!("tehas work: could not write the image's word on comments, a park of this cycle offers no comments retry");
    }
    if mode == "polyfill" && state.pointer("/comments/unguard") == Some(&Value::Bool(true)) {
        env_words.extend(["-e".to_string(), "PI_ENSEMBLE_REPEAT_PARK=0".to_string()]);
    }
    // A log is named to the second, so a cycle of this issue can be given the
    // name one before it used: what err.txt of the record copies is only what
    // this cycle writes into it. The line of the attachments' gap is one of them.
    let err = on(&r.log, ".err");
    if let Err(e) = start_err(&err, &r.gap) {
        eprintln!(
            "tehas work: could not start {} afresh ({e}), what it holds may be a cycle's before",
            err.display()
        );
    }
    let cid = file("cid").to_string_lossy().into_owned();
    let Some(verb) = state.get("verb").and_then(Value::as_str) else {
        eprintln!("tehas work: could not prepare the sandbox, not starting a cycle");
        return Err(78);
    };
    Ok(run_words(r, &state, &env_words, &cid, &image, verb))
}

/// The stderr file of the cycle's log, holding `gap` alone. What was there
/// stays until the new text is written in full.
fn start_err(err: &Path, gap: &str) -> std::io::Result<()> {
    if gap.is_empty() {
        return match fs::remove_file(err) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    let new = on(err, ".new");
    fs::write(&new, format!("{gap}\n"))
        .and_then(|()| fs::rename(&new, err))
        .inspect_err(|_| {
            let _ = fs::remove_file(&new);
        })
}

/// Removes what a cycle before left of the protocol. The lines name what a
/// killed cycle left, the counts in it, and what could not be removed.
fn wipe_protocol(file: &dyn Fn(&str) -> PathBuf, issue: u64) -> Vec<String> {
    let mut left = Vec::new();
    let mut lines = Vec::new();
    for name in PROTOCOL {
        let path = file(name);
        if fs::symlink_metadata(&path).is_err() {
            continue;
        }
        if EVIDENCE.contains(&name) {
            let text = fs::read_to_string(&path).unwrap_or_default();
            let first: String = text
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .chars()
                .take(80)
                .collect();
            left.push(if first.is_empty() {
                name.to_string()
            } else {
                format!("{name} ({first})")
            });
        }
        if let Err(e) = fs::remove_file(&path) {
            lines.push(format!(
                "tehas work: could not remove {}: {e}",
                path.display()
            ));
        }
    }
    if !left.is_empty() {
        lines.insert(
            0,
            format!(
                "tehas work: the last cycle of #{issue} ended without reading what it left, removed now: {}",
                left.join(", ")
            ),
        );
    }
    lines
}

/// The line on `docker rm -f` of the cycle's name: a container that outlived
/// its cycle, or why it could not be removed. None when there was none.
fn removed_said(name: &str, out: &std::io::Result<std::process::Output>) -> Option<String> {
    let out = match out {
        Ok(o) => o,
        Err(e) => {
            return Some(format!(
                "tehas work: could not run docker rm -f {name}: {e}"
            ))
        }
    };
    let err = String::from_utf8_lossy(&out.stderr);
    if out.status.success() {
        return Some(format!(
            "tehas work: removed the container {name}, which outlived the cycle that started it"
        ));
    }
    if err.contains("No such container") {
        return None;
    }
    let why = err.lines().next().unwrap_or("").trim();
    let rc = out
        .status
        .code()
        .map_or("a signal".to_string(), |c| format!("exit {c}"));
    Some(format!(
        "tehas work: could not remove the container {name} ({rc}): {why}"
    ))
}

/// The words of the cycle's docker run of `verb`, of `image`, with the
/// environment `env_words`.
fn run_words(
    r: &Record,
    state: &Value,
    env_words: &[String],
    cid: &str,
    image: &str,
    verb: &str,
) -> Vec<String> {
    sandbox::cycle_run(&sandbox::CycleRun {
        name: &r.name,
        limits: &list(state, "limits"),
        security: &list(state, "security"),
        user: &sandbox::user(),
        cid_file: cid,
        tree: &env("TEHAS_WORK_DIR"),
        mounts: &list(state, "mounts"),
        images: &r.images,
        env: env_words,
        hosts: &list(state, "hosts"),
        image,
        verb,
        issue: r.issue,
        flags: &list(state, "flags"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn a_failed_move_aside_keeps_the_tree_already_there() {
        let d = tehas_core::testdir::TestDir::new("prepare-swap-fail");
        fs::create_dir_all(d.join("w.before-retry")).unwrap();
        fs::write(d.join("w.before-retry/old"), "x").unwrap();
        let tree = d.join("w");
        let lines = swap_aside(tree.to_str().unwrap()).unwrap_err();
        assert!(d.join("w.before-retry/old").exists());
        assert!(!d.join("w.before-retry.old").exists());
        assert!(lines[0].contains("could not move"), "{lines:?}");
    }

    #[test]
    fn an_aside_that_cannot_be_removed_is_put_back_and_the_start_stops() {
        let d = tehas_core::testdir::TestDir::new("prepare-swap-held");
        fs::create_dir_all(d.join("w")).unwrap();
        fs::write(d.join("w/new"), "x").unwrap();
        fs::create_dir_all(d.join("w.before-retry/held")).unwrap();
        let ro = |mode| {
            fs::set_permissions(d.join("w.before-retry"), fs::Permissions::from_mode(mode)).unwrap()
        };
        ro(0o555);
        let held = d.join("w.before-retry/probe");
        if fs::write(&held, "").is_ok() {
            // Root writes anyway: nothing to see.
            ro(0o755);
            return;
        }
        let lines = swap_aside(d.join("w").to_str().unwrap()).unwrap_err();
        ro(0o755);
        assert!(d.join("w/new").exists(), "{lines:?}");
        assert!(d.join("w.before-retry/held").is_dir(), "{lines:?}");
        assert!(!d.join("w.before-retry.old").exists(), "{lines:?}");
        assert!(lines[0].contains("could not remove"), "{lines:?}");
    }

    #[test]
    fn a_replaced_aside_is_said_and_a_stale_one_removed() {
        let d = tehas_core::testdir::TestDir::new("prepare-swap-said");
        fs::create_dir_all(d.join("w")).unwrap();
        fs::write(d.join("w/new"), "x").unwrap();
        fs::create_dir_all(d.join("w.before-retry")).unwrap();
        fs::create_dir_all(d.join("w.before-retry.old")).unwrap();
        let lines = swap_aside(d.join("w").to_str().unwrap()).unwrap();
        assert!(d.join("w.before-retry/new").exists());
        assert!(!d.join("w.before-retry.old").exists());
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].contains("removed") && lines[0].contains(".old"));
        assert!(lines[1].contains("replaced what the cycle before last left"));
        // Nothing beside: a plain move, nothing to say.
        fs::create_dir_all(d.join("v")).unwrap();
        assert!(swap_aside(d.join("v").to_str().unwrap())
            .unwrap()
            .is_empty());
        assert!(d.join("v.before-retry").is_dir());
    }

    #[test]
    fn what_a_killed_cycle_left_is_said_before_it_is_removed() {
        let d = tehas_core::testdir::TestDir::new("prepare-wipe");
        let file = |name: &str| d.join(format!("7.{name}"));
        assert!(wipe_protocol(&file, 7).is_empty());
        fs::write(file("cid"), "abc123\n").unwrap();
        fs::write(file("oom"), "2\n").unwrap();
        fs::write(file("signal"), "").unwrap();
        fs::write(file("status"), "running\n").unwrap();
        let lines = wipe_protocol(&file, 7);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains("#7"), "{lines:?}");
        assert!(
            lines[0].ends_with("cid (abc123), oom (2), signal"),
            "{lines:?}"
        );
        for name in PROTOCOL {
            assert!(!file(name).exists(), "{name}");
        }
    }

    #[test]
    fn the_err_file_keeps_its_text_until_the_new_is_written() {
        let d = tehas_core::testdir::TestDir::new("prepare-err");
        let err = d.join("log.err");
        fs::write(&err, "old\n").unwrap();
        start_err(&err, "gap").unwrap();
        assert_eq!(fs::read_to_string(&err).unwrap(), "gap\n");
        start_err(&err, "").unwrap();
        assert!(!err.exists());
        start_err(&err, "").unwrap();
        // A directory in the way: the write fails and says so.
        fs::create_dir_all(&err).unwrap();
        fs::write(err.join("kept"), "x").unwrap();
        assert!(start_err(&err, "gap").is_err());
        assert!(err.join("kept").exists());
        assert!(!on(&err, ".new").exists());
    }

    #[test]
    fn docker_rm_of_the_cycle_name_is_said_unless_there_was_none() {
        let sh = |script: &str| Command::new("sh").args(["-c", script]).output();
        assert!(removed_said("c", &sh("echo c"))
            .unwrap()
            .contains("removed the container c"));
        let none = sh("echo 'Error response from daemon: No such container: c' >&2; exit 1");
        assert_eq!(removed_said("c", &none), None);
        let refused = removed_said("c", &sh("echo 'daemon not running' >&2; exit 1")).unwrap();
        assert!(
            refused.ends_with("(exit 1): daemon not running"),
            "{refused}"
        );
        let gone = Err(std::io::Error::other("no timeout"));
        assert!(removed_said("c", &gone).unwrap().contains("could not run"));
    }

    #[test]
    fn the_last_lines_of_a_log_say_when_it_is_empty_or_unreadable() {
        let d = tehas_core::testdir::TestDir::new("prepare-last");
        let log = d.join("p");
        assert!(last_lines(&log, 3, "it")[0].contains("could not be read"));
        fs::write(&log, "").unwrap();
        assert_eq!(
            last_lines(&log, 3, "it")[0],
            format!("it wrote nothing into {}", log.display())
        );
        fs::write(&log, "a\nb\nc\nd\n").unwrap();
        assert_eq!(last_lines(&log, 3, "it"), ["b", "c", "d"]);
    }

    #[test]
    fn each_name_of_protocol_is_a_cycle_note() {
        for name in EVIDENCE {
            assert!(PROTOCOL.contains(&name), "{name}");
        }
        for name in PROTOCOL {
            assert!(tehas_core::cycle_notes::is_note(name), "{name}");
        }
    }

    #[test]
    fn the_container_runs_the_image_its_record_got_and_the_tag_without_one() {
        let id = format!("sha256:{}", "e".repeat(64));
        let r = Record {
            issue: 7,
            file: PathBuf::new(),
            attempt: "1".into(),
            log: PathBuf::new(),
            name: "tehas-7".into(),
            stop_wait: 1,
            text_wait: 1,
            gap: String::new(),
            images: Vec::new(),
            stop: Stop(Arc::new(AtomicBool::new(false)), Arc::default()),
        };
        let tag = "tehas-sandbox:latest";
        // The image is the word before the runner, as the table of runs reads it.
        let image = |seen: &Seen| {
            let words = run_words(&r, &json!({}), &[], "cid", &run_image(seen, tag), "work");
            let at = words.iter().position(|w| w == sandbox::RUNNER).unwrap();
            (words[at - 1].clone(), words)
        };
        let seen = image_of(&format!("{id} abcdef1\n"));
        assert_eq!(seen, Some((id.clone(), Some("abcdef1".into()))));
        let (ran, words) = image(&seen);
        assert_eq!(ran, id);
        assert!(!words.iter().any(|w| w == tag), "{words:?}");
        // An inspect that fails, or prints no id, leaves the tag.
        for text in [
            "",
            "Error: No such image: tehas-sandbox:latest",
            "sha256:abc",
        ] {
            assert_eq!(image(&image_of(text)).0, tag, "{text}");
        }
        assert_eq!(image(&None).0, tag);
    }

    /// A store that answers busy to the first `busy` calls of cycle-image, and
    /// writes the verb of every call to `calls`.
    fn busy_store(name: &str, busy: u32) -> tehas_core::testdir::TestDir {
        let h = tehas_core::testdir::TestDir::new(name);
        fs::create_dir_all(h.join("libexec")).unwrap();
        let script = format!(
            "#!/bin/sh\necho \"$1\" >> \"$TEHAS_HOME/calls\"\n\
             [ \"$1\" = cycle-image ] || exit 78\n\
             n=$(grep -c '^cycle-image$' \"$TEHAS_HOME/calls\")\n\
             if [ \"$n\" -le {busy} ]; then echo 'store: busy: the database is locked' >&2; exit 1; fi\n\
             printf '%s\\n' '{{\"api\":1,\"ok\":true,\"verb\":\"cycle-image\"}}'\n"
        );
        tehas_core::testing::write_exe(h.join("libexec/store"), script).unwrap();
        h
    }

    #[test]
    fn a_busy_store_is_asked_once_more_for_the_image_and_the_cycle_starts() {
        let r = Record {
            issue: 7,
            file: PathBuf::new(),
            attempt: "1".into(),
            log: PathBuf::new(),
            name: "tehas-7".into(),
            stop_wait: 1,
            text_wait: 1,
            gap: String::new(),
            images: Vec::new(),
            stop: Stop::default(),
        };
        let seen: Seen = Some((format!("sha256:{}", "e".repeat(64)), None));
        // Busy once: the second ask puts the image on the record. Busy twice:
        // the record goes without it, and only the record pays.
        for busy in [1, 2] {
            let h = busy_store(&format!("prepare-busy-{busy}"), busy);
            assert!(on_record(h.path(), &r, &json!({}), &seen), "{busy}");
            let calls = fs::read_to_string(h.join("calls")).unwrap();
            let asked = calls.lines().filter(|l| *l == "cycle-image").count();
            assert_eq!(asked, 2, "{busy}: {calls}");
        }
    }

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
    fn a_push_log_that_cannot_be_made_is_no_pass() {
        let t = Tree {
            issue: 7,
            file: PathBuf::new(),
            branch: "main".to_string(),
            log: PathBuf::new(),
            push_wait: 1,
            console: String::new(),
            stop: Stop::default(),
        };
        let nowhere = Path::new("/nonexistent/tehas/work.jsonl.preflight");
        assert_eq!(push_check(&t, &json!({}), "/nonexistent", nowhere), Some(3));
    }

    #[test]
    fn a_stop_is_the_flag_a_handler_sets() {
        let stop = Stop::default();
        assert!(!stop.came());
        stop.flag().store(true, Ordering::SeqCst);
        assert!(stop.came());
    }

    #[test]
    fn a_tree_moved_aside_replaces_the_one_before_it() {
        let d = tehas_core::testdir::TestDir::new("prepare");
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
    }

    /// The workspace of a cycle that ran, then the start after it: whether
    /// that start ran its container, and what is left under each name.
    #[test]
    fn a_start_that_never_ran_puts_back_the_workspace_it_moved_aside() {
        for (ran, at_tree, beside) in [(false, "finished", None), (true, "clone", Some("finished"))]
        {
            let d = tehas_core::testdir::TestDir::new("prepare-aside");
            let tree = d.join("w");
            let tree = tree.to_str().unwrap();
            fs::create_dir_all(d.join("w")).unwrap();
            fs::write(d.join("w/finished"), "x").unwrap();
            let mut set = Aside::default();
            assert!(set.take(tree));
            fs::create_dir_all(d.join("w")).unwrap();
            fs::write(d.join("w/clone"), "x").unwrap();
            if ran {
                set.started();
            } else {
                drop(set);
            }
            assert!(d.join("w").join(at_tree).exists(), "{ran}");
            let other = if at_tree == "clone" {
                "finished"
            } else {
                "clone"
            };
            assert!(!d.join("w").join(other).exists(), "{ran}");
            match beside {
                Some(f) => assert!(d.join("w.before-retry").join(f).exists(), "{ran}"),
                None => assert!(!d.join("w.before-retry").exists(), "{ran}"),
            }
        }
    }

    #[test]
    fn a_start_that_moved_nothing_aside_keeps_its_clone() {
        let d = tehas_core::testdir::TestDir::new("prepare-aside-none");
        fs::create_dir_all(d.join("w")).unwrap();
        fs::write(d.join("w/clone"), "x").unwrap();
        let tree = d.join("nothing-yet");
        let mut set = Aside::default();
        assert!(set.take(tree.to_str().unwrap()));
        fs::create_dir_all(&tree).unwrap();
        drop(set);
        assert!(tree.exists());
        assert!(d.join("w/clone").exists());
        let tree = d.join("w");
        assert!(!put_back(tree.to_str().unwrap()));
        assert!(d.join("w/clone").exists());
    }
}
