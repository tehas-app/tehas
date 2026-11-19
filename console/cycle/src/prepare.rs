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

use crate::start::{self, is_record, record_value};
use crate::{caps, conflict, host, ran_with};
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use tehas_core::driver::Driver;
use tehas_core::exit::work::{NOT_STARTED, NO_PUSH};
use tehas_core::install::{self, Step};
use tehas_core::models;
use tehas_core::sandbox::{self, Identity, Kind, Paths, HANDOFF_SECONDS};
use tehas_core::slice::{self, Load};
use tehas_core::{lock, time};

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
/// the issue's open pull request and pushes to the same branch. A restart passes
/// --restart to classic only: work-lean takes no such flag and has no attention
/// gate, and every cycle is a fresh clone, so a lean run never finds a state
/// file to discard. The dispatcher takes needs-human-attention off a lean start
/// itself; on classic the cycle takes it off before the runner starts.
pub fn run_flags(limit: u64, restart: bool, refresh: bool, driver: Driver) -> Vec<String> {
    let mut flags = vec![
        "--timeout".to_string(),
        limit.div_ceil(60).to_string(),
        "--json".to_string(),
    ];
    if restart && !driver.never_takes().contains(&"--restart") {
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
    /// state/cycles/<issue>.refused: why the check refused the start, for the
    /// park (src/host.rs says its lines).
    pub refused_note: PathBuf,
    pub caps_wait: u64,
    pub console: String,
    /// The limits and the security options of the slice.
    pub words: Vec<String>,
    pub stop: Stop,
}

/// The cycle's scratch made empty and mounted at /tmp. Err is the path that
/// could not be made.
fn scratch(c: &Check, security: &mut Vec<String>) -> Result<(), String> {
    let dir = env("TEHAS_SCRATCH_DIR");
    if dir.is_empty() {
        return Ok(());
    }
    let name = crate::scratch::name(&env("TEHAS_WORK_DIR"), c.issue);
    let unmade = || format!("{}/{name}", dir.trim_end_matches('/'));
    let out = Command::new(&c.console)
        .args(["clean", "scratch", &dir, &name])
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()
        .map_err(|_| unmade())?;
    if !out.status.success() {
        return Err(unmade());
    }
    let path = String::from_utf8_lossy(&out.stdout);
    security.truncate(security.len().saturating_sub(2));
    security.extend([
        "-v".to_string(),
        format!("{}:/tmp", path.trim_end_matches('\n')),
    ]);
    Ok(())
}

/// A 78 of the check, with its cause left for the report: standard error goes
/// to the run log only.
fn refuse(c: &Check, cause: &str) -> i32 {
    let _ = fs::write(&c.refused_note, cause);
    78
}

/// The mark of a line of a `caps-lacking` note for a flag tehas passed that the
/// driver never takes ([`Driver::never_takes`]): a fault of tehas, not of the
/// image.
const NEVER_TAKEN: &str = "never-taken ";

/// The lines of an unsupported answer, each flag the driver never takes marked
/// with [`NEVER_TAKEN`].
pub fn mark_never_taken(lacking: &str, driver: Driver) -> String {
    lacking
        .lines()
        .map(|l| match l.strip_prefix("the flag ") {
            Some(f) if driver.never_takes().contains(&f) => format!("{NEVER_TAKEN}{l}\n"),
            _ => format!("{l}\n"),
        })
        .collect()
}

/// What a park says of the note [`refuse`] left.
#[derive(Debug, PartialEq, Eq)]
pub struct Refused {
    /// The sentences of the park.
    pub said: String,
    /// A few words of why, for the factory's line.
    pub short: String,
    /// Tehas passed what the driver never takes.
    pub fault: bool,
}

impl Refused {
    fn of(said: String, short: &str) -> Refused {
        Refused {
            said,
            short: short.to_string(),
            fault: false,
        }
    }
}

/// A path as TEHAS_SCRATCH_DIR and a scratch name make it: absolute, of
/// letters, digits and `/._-`, with no `..`.
fn scratch_path(p: &str) -> bool {
    p.starts_with('/')
        && p.len() <= 300
        && !p.contains("..")
        && p.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/._-".contains(&b))
}

/// The words a park says of the note [`refuse`] left, or None when the text is
/// not one. Only words the check wrote are said, each line checked again.
pub fn refused_said(text: &str) -> Option<Refused> {
    let mut lines = text.lines();
    let first = lines.next()?;
    if first == "tree-not-ours" {
        return Some(Refused::of("The directory that holds the workspaces exists and was not made by tehas, so tehas work did not touch it.".into(), "the directory of the workspaces is not tehas's"));
    }
    if let Some(path) = first.strip_prefix("scratch-unmade ") {
        return scratch_path(path).then(|| {
            Refused::of(
                format!("The cycle's scratch `{path}` could not be emptied and made, so no container was started. A `TEHAS_SCRATCH_DIR` that is not there, or something in the folder the factory's account cannot remove, such as a directory of another account, stops every start of this issue until it is put right on the host. The run log on the host has the line `tehas clean: {path} could not be made`."),
                "the cycle's scratch could not be made",
            )
        });
    }
    if let Some(rc) = first.strip_prefix("caps-unreadable ") {
        let ok = !rc.is_empty() && rc.len() <= 3 && rc.bytes().all(|b| b.is_ascii_digit());
        return ok.then(|| {
            Refused::of(
                format!("The sandbox image gave no `--capabilities` answer that tehas could read (exit {rc})."),
                "the sandbox image gave no capabilities answer",
            )
        });
    }
    if let Some(rc) = first.strip_prefix("install-failed ") {
        return install_said(rc, lines.next());
    }
    if first == "install-lacks" {
        let tools: Vec<String> = lines
            .filter(|l| {
                !l.is_empty()
                    && l.len() <= 40
                    && l.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            })
            .take(10)
            .map(|l| format!("`{l}`"))
            .collect();
        if tools.is_empty() {
            return None;
        }
        let tools = tools.join(", ");
        return Some(Refused::of(
            "Rebuild the sandbox image with it, then retry.".into(),
            &format!("the product's install failed: the sandbox image has no {tools}"),
        ));
    }
    if first != "caps-lacking" {
        return None;
    }
    let (mut lacks, mut never) = (Vec::new(), Vec::new());
    for l in lines
        .filter(|l| {
            !l.is_empty()
                && l.len() <= 200
                && l.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b" ._,-".contains(&b))
        })
        .take(20)
    {
        match l.strip_prefix(NEVER_TAKEN) {
            Some(rest) if !rest.is_empty() => never.push(format!("`{rest}`")),
            Some(_) => {}
            None => lacks.push(format!("`{l}`")),
        }
    }
    let mut said = Vec::new();
    if !never.is_empty() {
        said.push(format!(
            "Tehas passed what this driver never takes: {}. That is a fault in tehas, not in the sandbox image or the issue.",
            never.join(", ")
        ));
    }
    if !lacks.is_empty() {
        said.push(format!(
            "The sandbox image does not support what this cycle is given: {}.",
            lacks.join(", ")
        ));
    }
    if said.is_empty() {
        return None;
    }
    let fault = !never.is_empty();
    Some(Refused {
        said: said.join(" "),
        short: if fault {
            "tehas passed a flag the driver never takes"
        } else {
            "the sandbox image lacks what it was given"
        }
        .to_string(),
        fault,
    })
}

/// The words of an `install-failed <rc>` note, with Docker's refusal of the
/// CPU quota when its second line is `docker-cpus <asked> <counted>`.
fn install_said(rc: &str, cpus: Option<&str>) -> Option<Refused> {
    let digits = |w: &str| !w.is_empty() && w.len() <= 3 && w.bytes().all(|b| b.is_ascii_digit());
    if !digits(rc) {
        return None;
    }
    let refused = cpus
        .and_then(|l| l.strip_prefix("docker-cpus "))
        .and_then(|l| l.split_once(' '))
        .filter(|(asked, counted)| {
            asked
                .split_once('.')
                .is_some_and(|(a, b)| digits(a) && b.len() == 2 && digits(b))
                && digits(counted)
        });
    Some(match refused {
        Some((asked, counted)) => Refused::of(
            "The Docker daemon counts the CPUs the host had when it started: restart it on the host, then retry.".into(),
            &format!("the product's install failed: docker refused --cpus {asked}, it counts {counted} CPUs"),
        ),
        None if matches!(rc, "124" | "137") => Refused::of(
            "The run log on the host holds what it printed.".into(),
            &format!("the product's install did not end within {}s", install::WAIT),
        ),
        None => Refused::of(
            "The run log on the host holds its last lines.".into(),
            &format!("the product's install failed (exit {rc})"),
        ),
    })
}

/// The CPUs Docker said it counts when it refused a container's `--cpus`:
/// "range of CPUs is from 0.01 to 16.00, as there are only 16 CPUs available".
fn docker_counted(said: &str) -> Option<u64> {
    said.lines().find_map(|l| {
        let (_, rest) = l.split_once("as there are only ")?;
        let n = rest.strip_prefix(char::is_whitespace).unwrap_or(rest);
        let (n, tail) = n.split_once(' ')?;
        tail.starts_with("CPUs available")
            .then(|| n.parse().ok())
            .flatten()
    })
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
    let Some((limits, security)) = slice::split(&c.words) else {
        eprintln!("tehas work: the container limits could not be made, not starting");
        return 78;
    };
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
    if let Err(path) = scratch(c, &mut security) {
        return refuse(c, &format!("scratch-unmade {path}\n"));
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
        return refuse(c, "tree-not-ours\n");
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
    // Every refresh works on its pull request's branch, which may have started
    // beyond the workspace's depth: the tree step deepens it to hold that start.
    let continues = rec
        .as_ref()
        .filter(|_| refresh)
        .and_then(|r| r.get("refresh_of"))
        .map(|of| json!({"pr": of.get("pr"), "head": of.get("head_sha")}))
        .unwrap_or(Value::Null);
    // A lean refresh is refused below unless the image lists --continue-pr for
    // work-lean. A lean restart passes no flag (run_flags).
    let flags = run_flags(limit, c.restart, refresh, c.driver);
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
        || crate::gives::hands_over(&c.record)
    {
        names.push_str(" PI_ENSEMBLE_EXTRA_IMAGE_DIRS");
    }
    // A spec the runner does not read would be dropped without a word.
    if crate::gives::hands_over(&c.record) && crate::gives::spec_pin(&c.record).is_some() {
        features.push(' ');
        features.push_str(caps::READS_SPEC);
    }
    let mut lacking = (caps_rc == 0)
        .then(|| caps::unsupported(&answer, verb, &flags.join(" "), &names, &features))
        .flatten();
    // An input the runner does not list passes when a polyfill the image
    // carries declares it. The labels are asked only then.
    if lacking.as_deref().is_some_and(|l| l.contains("the input ")) {
        let declared = caps::polyfill_inputs(&answer, &crate::gives::image_labels(&image));
        let kept: Vec<&str> = names
            .split(' ')
            .filter(|n| match declared.iter().find(|(i, _)| i == n) {
                Some((i, p)) => {
                    eprintln!("tehas work: the input {i} is read by the polyfill {p} of {image}");
                    false
                }
                None => true,
            })
            .collect();
        lacking = caps::unsupported(&answer, verb, &flags.join(" "), &kept.join(" "), &features);
    }
    let Some(lacking) = lacking else {
        eprintln!(
            "tehas work: {image} gave no --capabilities answer (exit {caps_rc}), not starting a cycle"
        );
        return refuse(c, &format!("caps-unreadable {caps_rc}\n"));
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
        let lacking = mark_never_taken(&lacking, c.driver);
        for line in lacking.lines() {
            match line.strip_prefix(NEVER_TAKEN) {
                Some(l) => eprintln!(
                    "    {l} (tehas passed it, and {verb} never takes it: a fault of tehas, not of the image)"
                ),
                None => eprintln!("    {line}"),
            }
        }
        return refuse(c, &format!("caps-lacking\n{lacking}"));
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
        "comments": {"native": native, "unguard": unguard}, "refresh": refresh_of,
        "continues": continues});
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

/// The log of the push check, beside the cycle's own log.
pub fn preflight_of(log: &Path) -> PathBuf {
    on(log, ".preflight")
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
fn make_tree(tree: &str, state_dir: &str, branch: &str, depth: u32, set: &mut Aside) -> bool {
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
        && sandbox::clone(&env("TEHAS_WORK_REPO"), tree, branch, depth)
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
/// A failure leaves its cause in `note` for the park.
fn install(
    state: &Value,
    tree: &str,
    steps: &[Step],
    preflight: &Path,
    note: &Path,
) -> Option<i32> {
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
    // A log that cannot be opened is said as that, with the fault of the
    // system on it: it is not the install's own failure, and the lines of the
    // step before it are not quoted as its output (#2227).
    let (err, log) = match opened {
        Ok(logs) => logs,
        Err(e) => {
            eprintln!(
                "tehas work: the preflight log {} could not be opened for the product's install ({}), not starting a cycle",
                preflight.display(),
                tehas_core::mask::mask(&e.to_string())
            );
            return Some(78);
        }
    };
    let rc = code(
        bounded(10, install::WAIT, &words)
            .stdout(log)
            .stderr(err)
            .status(),
    );
    if rc == 0 {
        return None;
    }
    let text = fs::read(preflight).unwrap_or_default();
    let own = usize::try_from(start).map_or(&[][..], |at| text.get(at..).unwrap_or(&[]));
    let text = String::from_utf8_lossy(own);
    let lacking = install::missing(&text);
    if rc == install::MISSING && !lacking.is_empty() {
        let _ = fs::write(note, format!("install-lacks\n{}\n", lacking.join("\n")));
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
    let mut cause = format!("install-failed {rc}\n");
    let limits = list(state, "limits");
    let asked = limits.windows(2).find(|w| w[0] == "--cpus").map(|w| &w[1]);
    if let (Some(asked), Some(counted)) = (asked, docker_counted(&text)) {
        cause.push_str(&format!("docker-cpus {asked} {counted}\n"));
    }
    let _ = fs::write(note, cause);
    say(&format!(
        "tehas work: the product's install failed (exit {rc}), not starting a cycle:"
    ));
    let lines: Vec<&str> = text.lines().collect();
    for line in &lines[lines.len().saturating_sub(5)..] {
        say(&format!("    {line}"));
    }
    Some(78)
}

/// The fault of a sandbox that cannot push, one host fact with no issue in its
/// name. The poll holds starts on it and re-probes it; status and health say it.
pub const PUSH_FAULT: &str = tehas_core::sandbox::PUSH_FAULT;

/// Why the sandbox cannot push, as `git` said it: the first line of its log,
/// with what the mask of the host hides replaced. An empty log says nothing.
fn fault_line(text: &[u8]) -> String {
    let text = String::from_utf8_lossy(text);
    text.lines()
        .find(|l| !l.trim().is_empty())
        .map(tehas_core::mask::mask)
        .unwrap_or_default()
}

/// The record put on the file, or taken off it. A check that passed removes the
/// fault: the next start is judged on its own check, not on this one's.
fn record_fault(home: &Path, line: &str, at: i64) {
    let login = tehas_core::settings::bot_login(home).unwrap_or_default();
    tehas_core::sandbox::write_push_fault(home, line, at, &login);
}

/// Can the sandbox push at all? The client passes the timeout's TERM on to git
/// in the container. None only when the check ran and passed; else the status
/// to exit with. A check the host could not run is not a fault of the sandbox:
/// it records none.
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
    // A name that cannot be opened for append is the host's fault, and no check
    // that wrote no words of its own records a push fault, so the line names the
    // file it was given: it is not the 3 of a sandbox that refused to push, and
    // it is not told as one (#2227). The name is opened for append, and not
    // made new: the lines the product's install wrote stand in the log of the
    // start that wrote them, and a start that ran before wrote none.
    let opened = OpenOptions::new()
        .append(true)
        .create(true)
        .open(preflight)
        .and_then(|log| Ok((log.try_clone()?, log)));
    let (err, log) = match opened {
        Ok(logs) => logs,
        Err(e) => {
            eprintln!(
                "tehas work: the preflight log {} could not be opened for the push check of the sandbox ({}), not starting a cycle",
                preflight.display(),
                tehas_core::mask::mask(&e.to_string())
            );
            return Some(78);
        }
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
    Some(i32::from(NO_PUSH))
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

/// The first line of that log, which is what a fault names: git's own words,
/// masked. Empty when the log wrote nothing, which is the caller's word that no
/// check of the sandbox ran, as opposed to one that was refused.
pub fn preflight_line(preflight: &Path) -> String {
    fault_line(&fs::read(preflight).unwrap_or_default())
}

/// A sandbox that cannot push is the factory's fault, not the issue's: the host
/// says so in one file, and the start that found it ends its own record as
/// `push-fault` and gets the short comment, never a park with retry boxes. The
/// store keeps the word as it was given (`an_end` accepts it); its readers name
/// it in part 3. The issue keeps `tehas:ready`: nothing here changes a label.
/// A check the host could not run at all records no fault: that is no refusal
/// of a push, and its own words say what stopped it. The check itself cannot
/// tell the two apart, so its caller asks for the log with `preflight_line`.
pub fn push_fault(home: &Path, issue: u64, line: &str) {
    if line.is_empty() {
        return;
    }
    record_fault(home, line, time::now());
    let attempt = env("TEHAS_CYCLE_ATTEMPT");
    let n = issue.to_string();
    if is_attempt(&attempt) {
        let end = [
            "cycle-end",
            &n,
            "--attempt",
            &attempt,
            "--status",
            "push-fault",
        ];
        if let Err(refused) = host::store_said(home, &end) {
            eprintln!(
                "tehas work: the push fault of #{issue} is not on its record ({}): cycle-end {n} --attempt {attempt} --status push-fault",
                refused.line
            );
        }
    }
    start::tell(
        issue,
        &env("TEHAS_REPO"),
        // The wait of every comment a start posts, not the shorter one of the
        // report: the host is what is broken, so GitHub is worth asking again.
        60,
        "the sandbox cannot push to the repository, which is a fault of this host",
    );
}

/// The attempt of a start made by the supervisor: digits, no sign, no leading
/// zero, at most nine of them. Nothing else names a record this start owns, so
/// a start run by hand ends none.
fn is_attempt(s: &str) -> bool {
    tehas_core::grammar::count(s).is_some_and(|n| n > 0)
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

/// The host folders the container writes: the sources of its read-write
/// mounts ([`sandbox::mount_options`]).
pub fn writable(tree: &str, state_dir: &str, runs: &str) -> Vec<String> {
    let state = |d: &str| format!("{state_dir}/{d}");
    vec![
        tree.to_string(),
        state("cache"),
        state("memory"),
        state("sessions"),
        runs.to_string(),
        state("relay"),
    ]
}

/// The account the cycle's container runs as, its `--user`.
fn container_user() -> String {
    sandbox::user()
}

/// On a contractor, the container must own what it writes: the code index
/// refuses a cache folder it does not own, even one anyone may write. The
/// contractor program hands the folders to the factory's account the job
/// names, only when it is `user`, the account the container runs as.
fn handed(program: &Path, user: &str, dirs: &[String]) -> bool {
    Command::new(program)
        .args(["own", "--as", user])
        .args(dirs)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// What to say of a refresh whose read of the conflict check cannot be judged:
/// a record that holds no pull request, a head that is no commit, or a read of
/// `merge-tree` that gave no answer. The words say which (#2227).
pub fn unjudged(head: &str, pr: u64, said: Option<&str>) -> Option<String> {
    if pr == 0 {
        return Some("its record holds no pull request".to_string());
    }
    if !is_commit(head) {
        return Some(format!(
            "the head of the request is no commit ({})",
            head.chars()
                .filter(|c| !c.is_control() && !c.is_whitespace())
                .take(20)
                .collect::<String>()
        ));
    }
    // The words of the read are the reason already: what it wrote of the sides
    // it could not merge, or the fault of the read itself.
    said.map(str::to_string)
}

/// Whether `head` is the 40 hexadecimal digits of a commit.
pub fn is_commit(head: &str) -> bool {
    tehas_core::grammar::commit_id(head)
}

/// A refresh the image cannot finish when it conflicts: Some(78) when it does,
/// with the files in state/cycles/<issue>.refresh-conflict, and Some(78) for a
/// record that holds no pull request, which starts no container
/// either. A merge git cannot judge for a reason it cannot name runs the
/// refresh, as before, with git's words in the log.
fn refresh_conflict(home: &Path, t: &Tree, state: &Value, tree: &str) -> Option<i32> {
    let of = state.get("refresh").filter(|v| v.is_object())?;
    let pr = of.get("pr").and_then(Value::as_u64).unwrap_or(0);
    let head = of.get("head").and_then(Value::as_str).unwrap_or("");
    // A record that holds no pull request is a record nobody read: an unjudged
    // refresh is the run the note exists to prevent, and no note is written for
    // a conflict nobody saw, so it starts no container. The head is left to
    // git: a head the checkout does not hold is no fault of the record.
    // A read that cannot be judged stops the start: an unjudged refresh is the
    // run the note exists to prevent, and no note is written for a conflict
    // nobody saw. The three ways a read judges nothing are named apart, so the
    // line says which of them the host is in (#2227).
    let judged = conflict::conflicts(tree, &t.branch, head);
    let paths = match judged.paths() {
        Some(paths) if !paths.is_empty() || judged.judged() => paths,
        _ => {
            let why = unjudged(head, pr, judged.said().as_deref()).unwrap_or_default();
            eprintln!(
                "tehas work: whether pull request #{pr} conflicts with origin/{} cannot be judged: {why}, not starting a cycle",
                t.branch
            );
            return Some(78);
        }
    };
    if paths.is_empty() {
        // A git with no such read of its own is the shape of the host and no
        // fault of the request: the refresh runs, as it did before a read that
        // judged nothing could be told from one, and the log says what git said.
        if let Some(why) = judged.said() {
            say(&format!(
                "tehas work: git has no conflict check of its own: {why}, the refresh runs",
            ));
        }
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

/// A refresh's workspace deepened to hold its pull request's head and the
/// commit its branch started from, the merge base with the branch, as the
/// product checkout has them (tehas_core::depth::reach). Some(78) when it
/// cannot, with the reason: a shallow tree without that start would answer
/// `merge-base --is-ancestor` with a wrong no. None for any other start.
fn continued(state: &Value, tree: &str, branch: &str, depth: u32) -> Option<i32> {
    let of = state.get("continues").filter(|v| v.is_object())?;
    let pr = of.get("pr").and_then(Value::as_u64).unwrap_or(0);
    let head = of.get("head").and_then(Value::as_str).unwrap_or("");
    let src = env("TEHAS_WORK_REPO");
    let tip = format!("refs/remotes/origin/{branch}");
    let held = if is_commit(head) {
        tehas_core::depth::merge_base(&src, &tip, head)
            .ok_or_else(|| format!("{src} holds no merge base of origin/{branch} and {head}"))
            .and_then(|base| tehas_core::depth::reach(&src, tree, &[&tip, head], &base, depth))
    } else {
        Err("the head of the request is no commit".to_string())
    };
    let why = held.err()?;
    eprintln!(
        "tehas work: the workspace of the refresh of pull request #{pr} cannot hold the commit its branch started from: {why}, not starting a cycle"
    );
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
    let depth = match tehas_core::depth::from_env() {
        Ok(depth) => depth,
        Err(line) => {
            eprintln!("{line}, not starting a cycle");
            return 78;
        }
    };
    if !make_tree(&tree, &state_dir, &t.branch, depth, set) {
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
    if let Some(rc) = continued(&state, &tree, &t.branch, depth) {
        return rc;
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
        // A check the sandbox refused is the host's fault, not the issue's. One
        // that could not be run at all is not: it wrote no words of its own, and
        // the filesystem fault that stopped it is no refusal of a push.
        if rc == i32::from(NO_PUSH) {
            push_fault(home, t.issue, &preflight_line(&preflight));
        }
        return rc;
    }
    // The host can push: whatever an earlier start recorded of it is gone.
    record_fault(home, "", 0);
    if !steps.is_empty() {
        if t.stop.came() {
            return STOPPED;
        }
        for step in &steps {
            let line = format!("install {}", step.line());
            to_preflight(&preflight, &line);
            say(&format!("tehas work: {line}"));
        }
        let note = home.join(format!("state/cycles/{}.refused", t.issue));
        if let Some(rc) = install(&state, &tree, &steps, &preflight, &note) {
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
    if let Some(program) = crate::work::contractor() {
        if !handed(
            &program,
            &container_user(),
            &writable(&tree, &state_dir, &runs),
        ) {
            eprintln!("tehas work: the cycle's folders could not be handed to the factory's account, not starting a cycle");
            return 78;
        }
    }
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
    let text = ran_with::settings(&running(home).to_string(), &options);
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
        return Err(i32::from(NOT_STARTED));
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
        user: &container_user(),
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
    fn the_folders_handed_over_are_every_read_write_mount_source_of_the_cycle() {
        let paths = Paths {
            tree: "/in/work/widget",
            host: "/h/work/widget-7",
            state: "/h/work/widget-7.state",
            runs: "/u/.pi/agent/ensemble-runs/issue-7",
        };
        let words = sandbox::mount_options(Kind::Cycle, &paths, "/u/.pi/agent", Some("/h/app"));
        let rw: Vec<String> = words
            .chunks(2)
            .filter(|w| w[0] == "-v" && !w[1].ends_with(":ro"))
            .map(|w| w[1].split(':').next().unwrap().to_string())
            .collect();
        assert_eq!(writable(paths.host, paths.state, paths.runs), rw);
    }

    /// The tests that call `push_fault` read and set the process's
    /// `TEHAS_CYCLE_ATTEMPT`, `TEHAS_FAULT_CALLS` and `TEHAS_REPO`: one at a time,
    /// or a test that removes the attempt ends another's record for it.
    static FAULT_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn fault_env() -> std::sync::MutexGuard<'static, ()> {
        FAULT_ENV.lock().unwrap_or_else(|e| e.into_inner())
    }

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

    /// A record that holds no pull request, a head that is no commit, and a
    /// `merge-tree` that said nothing are each named as unjudged; a read that
    /// judged a clean merge is not.
    #[test]
    fn a_refresh_whose_read_judged_nothing_is_named_by_what_it_lacked() {
        let head = "a".repeat(40);
        assert_eq!(
            unjudged("", 0, None).unwrap(),
            "its record holds no pull request",
            "the pull request is named first"
        );
        assert_eq!(
            unjudged("HEAD", 50, None).unwrap(),
            "the head of the request is no commit (HEAD)"
        );
        assert_eq!(
            unjudged(&head.to_uppercase(), 50, None).unwrap(),
            format!(
                "the head of the request is no commit ({})",
                head.to_uppercase().chars().take(20).collect::<String>()
            ),
            "a head of 40 capitals is named by the first 20 of them"
        );
        // A read that said a reason of its own names it, whatever the head.
        assert_eq!(
            unjudged(&head, 50, Some("git did not answer within 120s")).unwrap(),
            "git did not answer within 120s",
            "the words of the read are the reason, said as git said them"
        );
        // A head git could not hold at all, and the head of a read that judged a
        // clean merge, are both judged by the read: nothing to name of them.
        assert_eq!(unjudged(&head, 50, None), None);
        assert!(is_commit(&head));
        assert!(!is_commit(&"a".repeat(41)));
        assert!(!is_commit(&head[..39]));
        assert!(!is_commit(&head.to_uppercase()));
        assert!(!is_commit(""));
    }

    /// An install that cannot open its preflight log says that, with the path
    /// and the fault of the system, and stops the start with 78: it is not the
    /// exit 127 of a program that was not there, and the lines the step before
    /// it wrote are not quoted as the install's own output (#2227). The words
    /// of the message are read back by the case in test/work_caps_test.sh.
    #[test]
    fn an_install_whose_log_cannot_be_opened_stops_the_start() {
        let d = tehas_core::testdir::TestDir::new("install-log");
        // The log of the case is a directory: nothing opens it for append.
        let preflight = d.join("work-7.jsonl.preflight");
        fs::create_dir_all(&preflight).unwrap();
        let step = Step {
            dir: "extension".into(),
            tool: "bun".into(),
            command: "bun install --frozen-lockfile".into(),
        };
        assert_eq!(
            install(
                &json!({}),
                "no-such-tree",
                &[step],
                &preflight,
                &d.join("7.refused")
            ),
            Some(78)
        );
    }

    /// Docker's refusal of a quota above the CPUs its daemon counts is read
    /// for that count, and nothing else is.
    #[test]
    fn the_cpus_docker_counts_are_read_off_its_refusal() {
        let said = "install extension/\ndocker: Error response from daemon: range of CPUs is from 0.01 to 16.00, as there are only 16 CPUs available\nRun 'docker run --help' for more information\n";
        assert_eq!(docker_counted(said), Some(16));
        for said in [
            "",
            "error: lockfile had changes",
            "as there are only many CPUs available",
            "as there are only 16 cores available",
        ] {
            assert_eq!(docker_counted(said), None, "{said:?}");
        }
    }

    #[test]
    fn an_install_note_is_said_in_a_few_words() {
        let r = refused_said("install-failed 125\ndocker-cpus 47.00 16\n").unwrap();
        assert_eq!(
            r.short,
            "the product's install failed: docker refused --cpus 47.00, it counts 16 CPUs"
        );
        assert!(r.said.contains("restart it on the host"), "{}", r.said);
        assert_eq!(
            refused_said("install-failed 1\n").unwrap().short,
            "the product's install failed (exit 1)"
        );
        assert_eq!(
            refused_said("install-lacks\nbun\nuv\n").unwrap().short,
            "the product's install failed: the sandbox image has no `bun`, `uv`"
        );
        for bad in [
            "install-failed \n",
            "install-failed 1234\n",
            "install-lacks\n\n",
        ] {
            assert_eq!(refused_said(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn a_refusal_note_is_said_only_in_the_words_the_check_wrote() {
        let said = |t: &str| refused_said(t).unwrap().said;
        assert_eq!(
            said("caps-unreadable 127\n"),
            "The sandbox image gave no `--capabilities` answer that tehas could read (exit 127)."
        );
        assert!(said("tree-not-ours\n").contains("was not made by tehas"));
        let r = refused_said("scratch-unmade /mnt/scratch/widget-7\n").unwrap();
        assert_eq!(r.short, "the cycle's scratch could not be made");
        assert!(
            r.said
                .contains("`/mnt/scratch/widget-7` could not be emptied"),
            "{}",
            r.said
        );
        assert_eq!(
            said("caps-lacking\nthe flag --restart\nthe feature leanHandoffHost\n"),
            "The sandbox image does not support what this cycle is given: `the flag --restart`, `the feature leanHandoffHost`."
        );
        // A line the check did not write is left out, and a note with none
        // of its lines, or another first line, is no note.
        assert_eq!(
            said("caps-lacking\nthe input A\n`@x` <b>\n"),
            "The sandbox image does not support what this cycle is given: `the input A`."
        );
        for t in [
            "",
            "caps-lacking\n",
            "caps-lacking\n@x\n",
            "caps-lacking\nnever-taken \n",
            "caps-unreadable\n",
            "caps-unreadable 12a\n",
            "caps-unreadable 1000\n",
            "scratch-unmade \n",
            "scratch-unmade scratch/w-7\n",
            "scratch-unmade /a/../b\n",
            "scratch-unmade /a/`b`\n",
            "something else\n",
        ] {
            assert_eq!(refused_said(t), None, "{t:?}");
        }
    }

    /// A flag tehas passed that the driver never takes is tehas's fault, said
    /// apart from what the image lacks.
    #[test]
    fn a_flag_the_driver_never_takes_is_said_as_a_fault_of_tehas() {
        let note = mark_never_taken(
            "the flag --restart\nthe flag --continue-pr\nthe input A\n",
            Driver::Lean,
        );
        assert_eq!(
            note,
            "never-taken the flag --restart\nthe flag --continue-pr\nthe input A\n"
        );
        // Classic takes a restart: a missing one is the image's.
        assert_eq!(
            mark_never_taken("the flag --restart\n", Driver::Classic),
            "the flag --restart\n"
        );
        let r = refused_said(&format!("caps-lacking\n{note}")).unwrap();
        assert!(r.fault);
        assert_eq!(r.short, "tehas passed a flag the driver never takes");
        assert_eq!(
            r.said,
            "Tehas passed what this driver never takes: `the flag --restart`. That is a fault in tehas, not in the sandbox image or the issue. The sandbox image does not support what this cycle is given: `the flag --continue-pr`, `the input A`."
        );
        let r = refused_said("caps-lacking\nthe input A\n").unwrap();
        assert!(!r.fault);
        assert_eq!(r.short, "the sandbox image lacks what it was given");
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
            run_flags(21600, false, false, Driver::Classic),
            ["--timeout", "360", "--json"]
        );
        assert_eq!(run_flags(1, false, false, Driver::Classic)[1], "1");
        assert_eq!(run_flags(61, false, false, Driver::Classic)[1], "2");
        assert_eq!(
            run_flags(60, true, true, Driver::Classic),
            ["--timeout", "1", "--json", "--restart", "--continue-pr"]
        );
    }

    #[test]
    fn a_lean_restart_passes_no_restart_flag() {
        // work-lean takes no --restart, and every cycle is a fresh clone, so a
        // lean run has no state file to discard.
        assert_eq!(
            run_flags(21600, true, false, Driver::Lean),
            ["--timeout", "360", "--json"]
        );
        assert_eq!(
            run_flags(21600, true, false, Driver::Classic),
            ["--timeout", "360", "--json", "--restart"]
        );
        // A lean refresh still asks for --continue-pr, which the image's
        // answer then takes or refuses.
        assert_eq!(
            run_flags(60, true, true, Driver::Lean),
            ["--timeout", "1", "--json", "--continue-pr"]
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

    /// The store refused a call whose arguments are these: a start of the
    /// supervisor ends its own record as `push-fault`. The word itself is kept
    /// by the store because `an_end` counts it an end; `console/store` has its
    /// own test of a word it does not name. With no attempt of a dispatch, no
    /// record is ended: a run by hand owns none.
    #[test]
    fn a_push_fault_ends_the_start_of_the_supervisor() {
        let _env = fault_env();
        let d = tehas_core::testdir::TestDir::new("push-fault-store");
        fs::create_dir_all(d.join("libexec")).unwrap();
        tehas_core::testing::write_exe(
            d.join("libexec/store"),
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >>\"$TEHAS_FAULT_CALLS\"\n".to_string()
                + "echo '{\"api\":1,\"verb\":\"cycle-end\",\"ok\":true}'\n",
        )
        .unwrap();
        let calls = d.join("calls");
        fs::write(&calls, "").unwrap();
        let git = "error: failed to push some refs\n";
        std::env::set_var("TEHAS_FAULT_CALLS", &calls);
        for attempt in ["", "3", "x"] {
            if attempt.is_empty() {
                std::env::remove_var("TEHAS_CYCLE_ATTEMPT");
            } else {
                std::env::set_var("TEHAS_CYCLE_ATTEMPT", attempt);
            }
            push_fault(d.path(), 7, git.trim_end());
        }
        std::env::remove_var("TEHAS_CYCLE_ATTEMPT");
        std::env::remove_var("TEHAS_FAULT_CALLS");
        // Only the attempt of a dispatch reached the store, and once.
        assert_eq!(
            fs::read_to_string(&calls).unwrap().trim_end(),
            "cycle-end 7 --attempt 3 --status push-fault"
        );
        let v: Value =
            serde_json::from_str(fs::read_to_string(d.join(PUSH_FAULT)).unwrap().trim_end())
                .unwrap();
        assert_eq!(v["line"], "error: failed to push some refs");
        assert_eq!(v["login"], "");
        assert_eq!(v.as_object().unwrap().len(), 3);
        assert!(tehas_core::time::from_iso(v["at"].as_str().unwrap()).is_some());
    }

    #[test]
    fn a_push_log_that_cannot_be_made_is_no_pass() {
        let _env = fault_env();
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
        // A log that cannot be opened is the host's fault and said in its own
        // words, with 78 like every other fault of the host: it is not told as a
        // sandbox that refused to push, and it records no push fault (#2227).
        assert_eq!(
            push_check(&t, &json!({}), "/nonexistent", nowhere),
            Some(78)
        );
        // The log of a check the host could not run wrote no words of its own,
        // so it names no fault: a filesystem fault is no refusal of a push, and
        // no file is left to make the host look unable to push.
        assert_eq!(preflight_line(nowhere), "");
        let d = tehas_core::testdir::TestDir::new("push-fault-unmade");
        fs::create_dir_all(d.join("state")).unwrap();
        fs::write(d.join(PUSH_FAULT), "a fault of an earlier start\n").unwrap();
        push_fault(d.path(), 7, &preflight_line(nowhere));
        assert_eq!(
            fs::read_to_string(d.join(PUSH_FAULT)).unwrap().trim_end(),
            "a fault of an earlier start"
        );
    }

    /// The three fields of the fault, its mode, and the word that ends the
    /// start that found it: the store keeps `push-fault` because
    /// `tehas_core::grammar::an_end` counts it an end.
    #[test]
    fn a_failed_push_check_records_the_fault_and_a_passing_one_removes_it() {
        let _env = fault_env();
        use std::os::unix::fs::PermissionsExt;
        let d = tehas_core::testdir::TestDir::new("push-fault");
        let preflight = d.join("logs/work-7.preflight");
        fs::create_dir_all(d.join("logs")).unwrap();
        fs::create_dir_all(d.join("state/github")).unwrap();
        fs::write(d.join("state/github/login"), "acme-app[bot]\n").unwrap();
        fs::write(
            &preflight,
            "\nremote: Permission to acme/widget denied to acme-app[bot].\n",
        )
        .unwrap();
        push_fault(d.path(), 7, &preflight_line(&preflight));
        let file = d.join(PUSH_FAULT);
        let v: Value = serde_json::from_str(fs::read_to_string(&file).unwrap().trim_end()).unwrap();
        assert_eq!(v["login"], "acme-app[bot]");
        assert_eq!(
            v["line"],
            "remote: Permission to acme/widget denied to acme-app[bot]."
        );
        let at = v["at"].as_str().unwrap();
        assert!(tehas_core::time::from_iso(at).is_some(), "{at}");
        assert_eq!(
            fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(tehas_core::grammar::an_end("push-fault"));
        // A check that passed takes the fault away, so the next start is
        // judged on its own check and not on this one's.
        record_fault(d.path(), "", 0);
        assert!(!file.exists());
    }

    /// The fault is one fact of the host: its name holds no issue, and a start
    /// run by hand records it as much as a start of the dispatch does.
    #[test]
    fn the_fault_of_a_run_by_hand_names_the_host_and_no_attempt() {
        let _env = fault_env();
        std::env::set_var("TEHAS_REPO", "acme/widget");
        std::env::remove_var("TEHAS_CYCLE_ATTEMPT");
        let d = tehas_core::testdir::TestDir::new("push-fault-no-attempt");
        let preflight = d.join("work-7.preflight");
        fs::write(&preflight, "fatal: could not read from remote repository\n").unwrap();
        push_fault(d.path(), 7, &preflight_line(&preflight));
        let v: Value =
            serde_json::from_str(fs::read_to_string(d.join(PUSH_FAULT)).unwrap().trim_end())
                .unwrap();
        assert_eq!(v["line"], "fatal: could not read from remote repository");
        assert!(!d.join("state/cycles").exists());
        std::env::remove_var("TEHAS_REPO");
    }

    #[test]
    fn the_fault_line_is_the_first_that_says_something_and_is_masked() {
        assert_eq!(fault_line(b"\n\n  \n"), "");
        assert_eq!(fault_line(b""), "");
        assert_eq!(fault_line(b"one\ntwo\n"), "one");
        // A token in git's words never reaches the host's file: its shape is
        // hidden, as the mask of the host hides it.
        assert_eq!(
            fault_line(b"ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n"),
            "ghp_***"
        );
    }

    #[test]
    fn only_a_start_of_the_supervisor_has_an_attempt_to_end() {
        for yes in ["1", "2", "999999999"] {
            assert!(is_attempt(yes), "{yes}");
        }
        for no in ["", "0", "01", "-1", "1 ", "x", "1.0", "10000000000"] {
            assert!(!is_attempt(no), "{no}");
        }
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
