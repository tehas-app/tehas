//! How full this host is and what fills it, for `tehas disk`
//! and, published, for `factory_disk` (console/mcp/src/disk.rs).
//!
//!   console disk [<home>] [--full] [--json] [--publish]
//!     [--wait <seconds>] [--walk-wait <seconds>]
//!     [--max-entries <n>] [--max-depth <n>]
//!   console disk free <path> [<seconds>]
//!
//! `free` is the one reading a command holds work on: the free kilobytes of the
//! file system of `<path>` and of Docker's, the smaller, as a line. No figure
//! prints nothing and exits 1 (`tehas_core::disk`). `<seconds>` bounds each call,
//! 10 when it is empty or no number from 1 to 99.
//!
//! `<home>` is the instance's, read from its own settings: `$TEHAS_HOME`, or
//! `$HOME/tehas` with neither, as `lib/config.sh` resolves it, so
//! `bin/tehas` loads nothing for it. A run may name it
//! anyway, and then the instance is asked nothing - `TEHAS_SCRATCH_DIR` and
//! `TEHAS_DISK_WAIT` come of the file only for a run that named nothing, so a
//! case plants its own home and names the bounds it walks under.
//!
//! One entry per file system the factory cares about, each named once and
//! holding every one of the paths that stands on it: `<home>`, the directory
//! Docker keeps its images in (`/var/lib/docker` when Docker is not there at
//! all, which is the host of that path), `/tmp`, and `TEHAS_SCRATCH_DIR` of
//! the instance when it names one. Two paths of one file system are one entry that names both, so
//! the same free bytes are never counted twice and a person is not told a
//! thing twice about the disk they are standing on.
//!
//! A figure that cannot be read is `null` with the reason it could not, never
//! 0: no figure is never the same as plenty, which is what `tehas_core::disk`
//! says of the figure the dispatcher holds a host on.
//!
//! `--full` adds what fills them, biggest first: each entry of `<home>` and of
//! `$HOME/.pi/agent`, each clone's `dev/<name>.state/target`, `docker system
//! df`, and the containers left stopped. Every outside call runs under
//! `timeout`, and every walk has a bound of entries, of depth and of wall time:
//! a place that ran out of one says which bound it ran out of, and its figure
//! says it is a part of what is there. Nothing is removed by any of this.
//!
//! `--publish` writes the same object it would print to `<home>/live/disk.json`
//! whole, through a side file renamed into place, as the poll's report is
//! written: a reader sees a whole file or the one before it. The command's own
//! reads write nothing, so a run without `--publish` leaves the host as it was.
//!
//! `TEHAS_INSTANCE_DIR` says where the instance's files are, as
//! `lib/config.sh` reads it: that name, else the one line of
//! `~/.config/tehas/instance-dir`, else `~/.config/tehas`.
//!
//! Exit: 0 the answer was printed, and written with `--publish`; 1 the file
//! could not be written; 64 a usage error; 78 the instance settings are
//! missing or name a scratch disk that is no absolute path, which reads
//! nothing of the host.

use crate::store::bounded;
use serde_json::{json, Map, Value};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tehas_core::disk::figure;
use tehas_core::time;

/// The walls of one run: the seconds an outside call is given, and the seconds
/// a walk of one place is given. Both are `TEHAS_DISK_WAIT`, the bound the
/// factory has always put on a read of the host's space - `console disk free`
/// gives it its `df` and `docker info` and the dispatcher's hold the same -
/// doubled for a walk, which reads a whole tree where a call reads one line.
/// One run's bounds, read once. A test builds its own, so a case names the
/// bounds it walks under and is not at the mercy of the host's environment.
#[derive(Clone, Debug)]
pub struct Bounds {
    pub wait: u64,
    pub walk: u64,
}

impl Bounds {
    /// The bounds of this run. A `df`, `docker info` or `docker ps` that has
    /// stopped answering is no answer, and a tree that stopped answering is no
    /// figure: the answer says so rather than the command dying, so the bound
    /// is asked of the environment here and nowhere else. The setting is the
    /// one `lib/config.sh` passes through, as the dispatcher's hold reads it.
    fn of_this_run() -> Bounds {
        let wait = bounded_by("TEHAS_DISK_WAIT", 10);
        Bounds {
            wait,
            walk: (wait * 2).min(99),
        }
    }
}

fn bounded_by(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|n| (1..=99).contains(n))
        .unwrap_or(default)
}

/// The entries of one directory a walk may look at, the depth it may go to,
/// and the bytes of a file it will still add up. A tree of a million names is
/// not what a person wants to know about their disk.
const MAX_ENTRIES: usize = 2000;
const MAX_DEPTH: usize = 6;
/// The most places `--full` prints for one root, biggest first.
const MAX_PLACES: usize = 12;
/// The most of `docker system df` an answer holds, and of one container's
/// command. Both are far past what a host prints.
const DOCKER_BOUND: u64 = 256 * 1024;
const COMMAND_MAX: usize = 200;
/// The most bytes of the published file. One run's answer is a few kilobytes;
/// a file over it is not one and is not published or read.
const PUBLISH_BOUND: u64 = 256 * 1024;
const USAGE: &str = "usage: console disk [<home>] [--full] [--json] [--publish]
       [--scratch <dir>] [--wait <s>] [--walk-wait <s>] [--max-entries <n>] [--max-depth <n>]
       console disk free <path> [<seconds>]";

/// The bound of one call of `free`, in whole seconds, 10 for anything else.
fn free_wait(given: &str) -> u64 {
    let word = (1..=2).contains(&given.len())
        && !given.starts_with('0')
        && given.bytes().all(|b| b.is_ascii_digit());
    word.then(|| given.parse().ok()).flatten().unwrap_or(10)
}

fn free(given: &[String]) -> u8 {
    let (path, wait) = match given {
        [path] => (path, ""),
        [path, wait] => (path, wait.as_str()),
        _ => return usage(),
    };
    if path.is_empty() {
        return 1;
    }
    let wait = Duration::from_secs(free_wait(wait));
    match tehas_core::disk::free_kb(path, |program, args| run(program, args, wait).ok()) {
        Some(kb) => {
            println!("{kb}");
            0
        }
        None => 1,
    }
}

pub fn main(given: &[String]) -> u8 {
    if given.first().is_some_and(|verb| verb == "free") {
        return free(&given[1..]);
    }
    // A run that named no home is the command a person typed: the console reads
    // the instance's own settings for it, for the scratch disk and for the
    // bound, as the settings library does, and refuses 78 when they cannot be
    // read - which is the one place the settings of the host are read for this
    // command, so its wrapper is a line of `exec`. A run that named one is the
    // console's own verb and asks the instance nothing: every name after it is
    // an option, the first of them the one the wrapper of an older console
    // gives, and the home it names is never a setting of its own.
    let (named, rest) = match given.split_first() {
        // An argument that is an option is not the home, and a run that names
        // none is the one that reads the instance.
        Some((first, rest)) if !first.starts_with("--") => (Some(PathBuf::from(first)), rest),
        _ => (None, given),
    };
    let mut home = named.unwrap_or_default();
    let mut opts = Options::default();
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--full" => opts.full = true,
            "--json" => opts.json = true,
            "--publish" => opts.publish = true,
            "--scratch" => match it.next().filter(|s| s.starts_with('/')).map(PathBuf::from) {
                Some(dir) => opts.scratch = Some(dir),
                None => return usage(),
            },
            "--wait" => match it.next().and_then(|v| v.parse::<u64>().ok()) {
                Some(n) if (1..=99).contains(&n) => opts.bounds.wait = n,
                _ => return usage(),
            },
            "--walk-wait" => match it.next().and_then(|v| v.parse::<u64>().ok()) {
                Some(n) if (1..=99).contains(&n) => opts.bounds.walk = n,
                _ => return usage(),
            },
            "--max-entries" => match it.next().and_then(|v| v.parse::<usize>().ok()) {
                Some(n) if (1..=100_000).contains(&n) => opts.entries = n,
                _ => return usage(),
            },
            "--max-depth" => match it.next().and_then(|v| v.parse::<usize>().ok()) {
                Some(n) if (1..=64).contains(&n) => opts.depth = n,
                _ => return usage(),
            },
            _ => return usage(),
        }
    }
    if home.as_os_str().is_empty() {
        // Nothing was named: the home, the scratch disk and the bound are the
        // instance's, and a run that cannot read them reads no host.
        match Instance::of_this_run() {
            Ok(found) => {
                home = found.home.clone();
                opts.scratch = found.scratch.clone();
                opts.bounds = Bounds {
                    wait: found.wait,
                    walk: (found.wait * 2).min(99),
                };
            }
            Err(why) => {
                eprintln!("console disk: {why}");
                return 78;
            }
        }
    }
    if home.as_os_str().is_empty() || !home.is_absolute() {
        eprintln!("console disk: <home> is an absolute path");
        return 78;
    }
    let host = Host::of_this_run(&home);
    // The published file is written first, from its own reading of the host:
    // what a program reads afterwards is the answer the run made, and the
    // figure a person sees beside it is the same one.
    if opts.publish {
        let file = published(&host, &opts);
        if let Err(e) = put(&home.join("live/disk.json"), &file) {
            eprintln!("console disk: live/disk.json not written: {e}");
            return 1;
        }
    }
    let answer = read(&host, &opts);
    if opts.json {
        println!("{answer}");
    } else {
        for line in lines(&answer) {
            println!("{line}");
        }
    }
    0
}

fn usage() -> u8 {
    eprintln!("{USAGE}");
    64
}

/// Where one instance keeps its settings, as `lib/config.sh` reads it: the
/// name in `TEHAS_INSTANCE_DIR`, else the one line of `~/.config/tehas/
/// instance-dir`, else `~/.config/tehas`. An empty name counts as none: the
/// settings library exports the name whether or not the run was given one.
pub fn instance_dir() -> Option<PathBuf> {
    let named = |name: &str| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if let Some(dir) = named("TEHAS_INSTANCE_DIR") {
        return Some(dir);
    }
    let home = std::env::var_os("HOME").filter(|v| !v.is_empty())?;
    let pointer = PathBuf::from(&home).join(".config/tehas/instance-dir");
    if let Ok(one) = fs::read_to_string(&pointer) {
        let line = one.lines().next().unwrap_or("").trim();
        if !line.is_empty() {
            return Some(PathBuf::from(line));
        }
    }
    Some(PathBuf::from(home).join(".config/tehas"))
}

/// The instance of this run, or why it is not one. The file is read and
/// never run as shell: it is the operator's, and a program has no business
/// executing what it reads.
pub struct Instance {
    pub home: PathBuf,
    pub scratch: Option<PathBuf>,
    pub wait: u64,
}

/// One `KEY=value` line of instance.env, whose value may be quoted as shell
/// does, or nothing when the line names another setting or holds no `=`.
pub fn setting(text: &str, name: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim().strip_prefix("export ").unwrap_or(line).trim();
        let (key, value) = line.split_once('=')?;
        if key.trim() != name {
            continue;
        }
        let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
        return (!value.is_empty()).then(|| value.to_string());
    }
    None
}

/// A path of the instance's own: absolute, of the characters the settings
/// allow, and no `..` in it. The reading that uses it only ever stats and
/// walks it, but a path is not read on a guess.
fn setting_path(text: &str, name: &str) -> Result<Option<PathBuf>, String> {
    let Some(value) = setting(text, name) else {
        return Ok(None);
    };
    let ok = value.len() > 1
        && value.starts_with('/')
        && !value.contains("//")
        && !value.contains("..")
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-/".contains(&b));
    if !ok {
        return Err(format!("{name} is not an absolute path: {value}"));
    }
    Ok(Some(PathBuf::from(value)))
}

impl Instance {
    /// The instance of this run, or why it is not one. The file is read and
    /// never run as shell: it is the operator's, and a program has no business
    /// executing what it reads.
    fn of_this_run() -> Result<Instance, String> {
        let dir = instance_dir().ok_or_else(|| "HOME is not set".to_string())?;
        let file = dir.join("instance.env");
        let text = fs::read_to_string(&file)
            .map_err(|_| format!("{} could not be read (run tehas setup)", file.display()))?;
        let home = match std::env::var_os("TEHAS_HOME") {
            // A name of its own, even an empty one, is the caller's: the
            // settings library exports TEHAS_HOME, so an empty one is a host
            // that named the home and got nothing for it.
            Some(from_env) => PathBuf::from(from_env),
            None => setting(&text, "TEHAS_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    let home = std::env::var_os("HOME").unwrap_or_default();
                    PathBuf::from(home).join("tehas")
                }),
        };
        let wait = setting(&text, "TEHAS_DISK_WAIT")
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|n| (1..=99).contains(n))
            .unwrap_or(10);
        Ok(Instance {
            home,
            scratch: setting_path(&text, "TEHAS_SCRATCH_DIR")?,
            wait,
        })
    }
}

/// What one run of the reading is told: the bounds, which a test turns down
/// and a host leaves alone.
#[derive(Clone, Debug)]
pub struct Options {
    pub full: bool,
    pub json: bool,
    pub publish: bool,
    /// `TEHAS_SCRATCH_DIR`, when the instance named one: the fourth file
    /// system the answer names, read from the instance's own file.
    /// A test builds its own, so a case names the disk it plants.
    pub scratch: Option<PathBuf>,
    /// The two walls of this run, taken once so every call of one answer is
    /// bounded the same way.
    pub bounds: Bounds,
    pub entries: usize,
    pub depth: usize,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            full: false,
            json: false,
            publish: false,
            scratch: None,
            bounds: Bounds::of_this_run(),
            entries: MAX_ENTRIES,
            depth: MAX_DEPTH,
        }
    }
}

/// One file system, named by every path of the host that stands on it.
#[derive(Debug)]
pub struct Fs {
    pub paths: Vec<String>,
    /// What `df` names as the file system's own, as `/` or `/dev/sda1`; None
    /// when the figure cannot be read at all, which is the shape of a read
    /// that answered nothing.
    pub mount: Option<String>,
    pub free: Option<u64>,
    pub total: Option<u64>,
    /// Why a figure is not there: `df` gave no second line, its word was no
    /// number, or the call did not answer.
    pub why: Option<String>,
}

impl Fs {
    fn value(&self) -> Value {
        let mut out = Map::new();
        out.insert("paths".into(), json!(self.paths));
        insert(&mut out, "mount", self.mount.as_deref().map(Value::from));
        insert(&mut out, "freeBytes", self.free.map(Value::from));
        insert(&mut out, "totalBytes", self.total.map(Value::from));
        insert(&mut out, "why", self.why.as_deref().map(Value::from));
        Value::Object(out)
    }
}

/// A key with no value is left out, as the other commands of the API leave one
/// out; `why` is the only way a null says anything, so it goes in with the
/// nulls it explains and never on its own.
fn insert(out: &mut Map<String, Value>, key: &str, v: Option<Value>) {
    if let Some(v) = v {
        out.insert(key.to_string(), v);
    }
}

/// What one place of the host holds, and what stopped the walk from saying
/// more of it.
#[derive(Debug)]
pub struct Place {
    pub path: String,
    pub bytes: Option<u64>,
    /// `whole`, `entries`, `depth`, `time` or `unreadable`: the bound the walk
    /// ran out of, or the reason it has no figure.
    pub cut: &'static str,
}

impl Place {
    fn value(&self) -> Value {
        let mut out = Map::new();
        out.insert("path".into(), json!(self.path));
        insert(&mut out, "bytes", self.bytes.map(Value::from));
        if self.cut != "whole" {
            out.insert("cut".into(), json!(self.cut));
        }
        Value::Object(out)
    }
}

/// Where the factory's things are, which is what the answer is read from. The
/// reading is given all three, so a case names the trees it plants.
#[derive(Clone, Debug)]
pub struct Host {
    pub home: PathBuf,
    pub agent: PathBuf,
}

impl Host {
    /// The host of a run: the home it was given, the agent directory of the
    /// account that ran it, and the scratch disk the instance named.
    fn of_this_run(home: &Path) -> Host {
        Host {
            home: home.to_path_buf(),
            agent: std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|h| h.join(".pi/agent"))
                .unwrap_or_else(|| PathBuf::from("/nonexistent/.pi/agent")),
        }
    }
}

/// The paths whose file systems the answer names, in that order, each with the
/// name the entry gives it when it is the first one on its file system.
fn watched(host: &Host, opts: &Options, docker_dir: &str) -> Vec<PathBuf> {
    let mut paths = vec![
        host.home.clone(),
        PathBuf::from(docker_dir),
        PathBuf::from("/tmp"),
    ];
    if let Some(scratch) = &opts.scratch {
        paths.push(scratch.clone());
    }
    paths
}

/// The watched paths, grouped by the file system they stand on: one entry per
/// file system, holding its paths in the order they were asked, and one entry
/// of its own for a path whose device cannot be told.
fn file_systems(paths: &[PathBuf], opts: &Options) -> Vec<Fs> {
    let mut out: Vec<Fs> = Vec::new();
    for path in paths {
        let text = path.to_string_lossy().to_string();
        let dev = fs::metadata(path).ok().map(|m| m.dev());
        if let Some(dev) = dev {
            if let Some(existing) = out.iter_mut().find(|f| same_dev(f, dev)) {
                existing.paths.push(text);
                continue;
            }
        }
        out.push(one(path, &text, opts));
    }
    out
}

/// Whether this entry was opened by a path of that device, which is the one
/// thing that makes two entries the same file system. The entry of a path that
/// could not be looked at is of no device at all - it carries no `dev`, so a
/// path that can be told is never folded into it, whatever `df` later says of
/// the name.
fn same_dev(entry: &Fs, dev: u64) -> bool {
    entry
        .paths
        .iter()
        .any(|p| fs::metadata(p).ok().is_some_and(|m| m.dev() == dev))
}

/// One file system: `df` of the path that opened the entry, once.
fn one(path: &Path, text: &str, opts: &Options) -> Fs {
    match df(path, Duration::from_secs(opts.bounds.wait)) {
        Ok(reading) => Fs {
            paths: vec![text.to_string()],
            mount: reading.mount,
            free: reading.free,
            total: reading.total,
            why: reading.why,
        },
        Err(why) => Fs {
            paths: vec![text.to_string()],
            mount: None,
            free: None,
            total: None,
            why: Some(why),
        },
    }
}

/// What `df -Pk` says of one file system, or why it says nothing. The fourth
/// and second columns of its second line are free and total in 1024-blocks;
/// a word that is no whole number is no figure (`tehas_core::disk::figure`), so
/// the caller's arithmetic cannot be handed a figure that wraps into another.
struct Df {
    mount: Option<String>,
    free: Option<u64>,
    total: Option<u64>,
    why: Option<String>,
}

fn df(path: &Path, wait: Duration) -> Result<Df, String> {
    let said = run("df", &["-Pk", &path.to_string_lossy()], wait)?;
    let Some(line) = said.lines().nth(1) else {
        return Err(format!("{}: df gave no figure", path.display()));
    };
    let cols: Vec<&str> = line.split_whitespace().collect();
    if cols.len() < 4 {
        return Err(format!("{}: df gave no figure", path.display()));
    }
    let (Some(free), Some(total)) = (figure(cols[3]), figure(cols[1])) else {
        return Err(format!(
            "{}: df's figure is not a number of bytes",
            path.display()
        ));
    };
    Ok(Df {
        mount: cols.first().map(|s| s.to_string()),
        free: Some(free.saturating_mul(1024)),
        total: Some(total.saturating_mul(1024)),
        why: None,
    })
}

/// One outside call, under `timeout -k`, its standard input closed. None for
/// one that could not be run or did not answer in time, with the words the
/// answer gives for it.
fn run(program: &str, args: &[&str], wait: Duration) -> Result<String, String> {
    let secs = wait.as_secs().to_string();
    let output = bounded("2", &secs, program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    let Ok(output) = output else {
        return Err(format!("{program} could not be run"));
    };
    if !output.status.success() {
        return Err(match output.status.code() {
            // timeout's own two: out of time, then killed for ignoring TERM.
            Some(124) | Some(137) => format!("{program} did not answer in {secs} s"),
            _ => format!("{program} answered nothing"),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The directory Docker keeps its images in, as Docker itself names it. The
/// host of that path is what is read when Docker is not there at all: what
/// Docker says is never guessed for it.
fn docker_root_dir(wait: Duration) -> String {
    match run("docker", &["info", "--format", "{{.DockerRootDir}}"], wait) {
        Ok(said) => {
            let line = said.lines().next().unwrap_or("").trim();
            if line.starts_with('/') && line.len() <= 200 {
                line.to_string()
            } else {
                "/var/lib/docker".to_string()
            }
        }
        Err(_) => "/var/lib/docker".to_string(),
    }
}

/// What fills the host, biggest first, over both roots and each clone's build
/// output. The places of one root are measured with one walk each, so a tree
/// is read once and not once per entry.
///
/// The build output of a clone is read from `<home>/dev`, so the entries of
/// that one directory are not asked of again as places of the home: `dev`
/// itself is not a place, and its names appear as the clones' targets, which
/// is the size a person asks about. Everything else of the home is listed
/// beside it.
fn places(host: &Host, opts: &Options) -> Vec<Place> {
    let mut out = Vec::new();
    out.extend(places_of(&host.home, opts, &["dev".to_string()]));
    out.extend(places_of(&host.agent, opts, &[]));
    out.extend(clones(host, opts));
    // The places that could be measured come first, biggest first; the ones
    // that could not follow, by name, so an unmeasured directory is never made
    // to look smaller than one that was measured.
    out.sort_by(|a, b| {
        b.bytes
            .is_some()
            .cmp(&a.bytes.is_some())
            .then_with(|| b.bytes.cmp(&a.bytes))
            .then_with(|| a.path.cmp(&b.path))
    });
    out.truncate(MAX_PLACES);
    out
}

/// Each entry of a directory, as far as the bounds reach: its bytes, and which
/// bound the reading of the root ran out of - `entries`, `depth` or `time` -
/// or the reason one name could not be opened. A dot-name is not entered and
/// no symlink is followed, which is the rule of the cleaner's walk: an entry
/// that leads out of the file system adds nothing of the other one.
fn places_of(root: &Path, opts: &Options, skip: &[String]) -> Vec<Place> {
    let began = Instant::now();
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut out: Vec<Place> = Vec::new();
    // What stopped the reading of this root, of the three that can: too many
    // names to count, a name deeper than the walk goes, or no time left.
    let mut cut: &'static str = "whole";
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || skip.contains(&name) {
            continue;
        }
        // The bound of names is counted on the entries themselves, not on what
        // was measured: a name that could not be read still holds the room a
        // bound is meant to bound.
        if out.len() >= opts.entries {
            cut = "entries";
            break;
        }
        if began.elapsed() >= Duration::from_secs(opts.bounds.walk) {
            cut = "time";
            break;
        }
        let path = entry.path();
        let (bytes, why) = du(&path, opts);
        // A name that cannot be opened is named with no figure and is not
        // added into the figures of the others; the rest of the directory is
        // still measured. A walk of its own that ran out of depth or of time
        // is the root's reading running out too, so the root is said not to be
        // the whole of it.
        if why == "depth" || why == "time" {
            cut = why;
        }
        out.push(Place {
            path: path.to_string_lossy().to_string(),
            bytes,
            cut: why,
        });
    }
    // A root that said nothing at all and had a bound to say it with is named
    // once, with no figure: silence about a directory that could not be read
    // would read as a disk with nothing on it.
    if out.is_empty() && cut != "whole" {
        out.push(Place {
            path: root.to_string_lossy().to_string(),
            bytes: None,
            cut,
        });
    }
    out
}

/// The bytes under one path, and the bound the walk ran out of: `whole`,
/// `entries`, `depth`, `time` or `unreadable`. Only regular files are counted
/// and no link is followed, so a tree that points out of the file system adds
/// nothing it did not already count.
fn du(path: &Path, opts: &Options) -> (Option<u64>, &'static str) {
    let began = Instant::now();
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => return (None, "unreadable"),
    };
    if meta.file_type().is_symlink() {
        // The place named is a link: it is named and not entered, whatever it
        // points at, and has no bytes of this tree to give.
        return (None, "unreadable");
    }
    if !meta.is_dir() {
        return (Some(meta.len()), "whole");
    }
    match walk(path, 1, opts, &began) {
        Walk::Done(bytes, _) => (Some(bytes), "whole"),
        Walk::Cut(why) => (None, why),
    }
}

enum Walk {
    /// The bytes reached, and the name of the first entry of this directory
    /// that could not be looked at, if any.
    Done(u64, Option<&'static str>),
    /// The bound or the failure that ended the reading, of which no part of
    /// what came before can be called the whole of the place.
    Cut(&'static str),
}

fn walk(dir: &Path, depth: usize, opts: &Options, began: &Instant) -> Walk {
    if depth > opts.depth {
        return Walk::Cut("depth");
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Walk::Cut("unreadable"),
    };
    let mut total = 0u64;
    let mut seen = 0usize;
    // Two kinds of bad news: a name of this directory that cannot be looked
    // at, which is that name's own and is carried up so the reader of the
    // directory can say it, and a bound the reading ran out of, which ends it.
    let mut broken: Option<&'static str> = None;
    for entry in entries {
        let Ok(entry) = entry else {
            broken = Some("unreadable");
            continue;
        };
        seen += 1;
        if seen > opts.entries {
            return Walk::Cut("entries");
        }
        if began.elapsed() >= Duration::from_secs(opts.bounds.walk) {
            return Walk::Cut("time");
        }
        let path = entry.path();
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => {
                broken = Some("unreadable");
                continue;
            }
        };
        // A link is named and not entered: what it points at may be another
        // file system, whose bytes belong to an entry of their own.
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            match walk(&path, depth + 1, opts, began) {
                Walk::Done(bytes, of) => {
                    total = total.saturating_add(bytes);
                    broken = broken.or(of);
                }
                Walk::Cut(why) => return Walk::Cut(why),
            }
            continue;
        }
        if meta.is_file() {
            total = total.saturating_add(meta.len());
            continue;
        }
        // A device, a socket or a fifo of its own is not bytes of the tree:
        // its size is the size of its inode, and is not what filled a disk.
        broken = broken.or(Some("unreadable"));
    }
    Walk::Done(total, broken)
}

/// What `<home>/dev` holds, once: the directory's entries, and for each clone
/// the `target` beside its tree - `<name>.state/target`, the one build output
/// of `tehas dev` the cleaner never takes - in place of the clone's state
/// directory, which is the same bytes named twice. A clone with no build
/// output is named by its directory alone; a name that is no name is skipped
/// rather than an error.
fn clones(host: &Host, opts: &Options) -> Vec<Place> {
    let dev = host.home.join("dev");
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(&dev) else {
        return out;
    };
    let began = Instant::now();
    let mut cut: &'static str = "whole";
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if out.len() >= opts.entries {
            cut = "entries";
            break;
        }
        if began.elapsed() >= Duration::from_secs(opts.bounds.walk) {
            cut = "time";
            break;
        }
        let path = entry.path();
        // The build output of a clone, if it has any: the bytes that answer
        // "what did the sessions build" are in the target, not in a second
        // entry for the tree that holds it.
        let target = match name.strip_suffix(".state") {
            Some(clone) if !clone.is_empty() => path.join("target"),
            _ => continue,
        };
        let place = match du(&target, opts) {
            (Some(bytes), why) => Place {
                path: target.to_string_lossy().to_string(),
                bytes: Some(bytes),
                cut: why,
            },
            // No build output, or none that could be told from the tree: the
            // clone's directory is the place, with what was learned of it.
            _ => Place {
                path: path.to_string_lossy().to_string(),
                ..du_place(&path, opts)
            },
        };
        if place.cut == "depth" || place.cut == "time" {
            cut = place.cut;
        }
        out.push(place);
    }
    if out.is_empty() && cut != "whole" {
        out.push(Place {
            path: dev.to_string_lossy().to_string(),
            bytes: None,
            cut,
        });
    }
    out
}

/// The figure of one path and the bound its reading ran out of, for a place
/// whose name is already known.
fn du_place(path: &Path, opts: &Options) -> Place {
    let (bytes, cut) = du(path, opts);
    Place {
        path: path.to_string_lossy().to_string(),
        bytes,
        cut,
    }
}

/// `docker system df`, in Docker's own words: its four lines, each cut to one
/// line of at most 200 characters, and a word for why it has none.
fn docker_df(wait: Duration) -> Value {
    match run("docker", &["system", "df"], wait) {
        Ok(said) => {
            let lines: Vec<String> = said
                .lines()
                .filter(|l| !l.trim().is_empty())
                .take(12)
                .map(|l| l.chars().take(COMMAND_MAX).collect())
                .collect();
            if lines.is_empty() {
                return json!({"state": "none"});
            }
            json!({"state": "read", "lines": lines})
        }
        Err(_) => json!({"state": "unreadable"}),
    }
}

/// The containers left stopped: Docker's own id, image and command, each cut,
/// and how many of them the list holds.
fn stopped_containers(wait: Duration) -> (u64, Vec<Value>) {
    let Ok(said) = run(
        "docker",
        &[
            "ps",
            "--filter",
            "status=exited",
            "--format",
            "{{.ID}}\t{{.Image}}\t{{.Command}}",
        ],
        wait,
    ) else {
        return (0, Vec::new());
    };
    let mut out = Vec::new();
    for line in said
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(DOCKER_BOUND.min(50) as usize)
    {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 2 {
            continue;
        }
        out.push(json!({
            "id": cols[0].chars().take(64).collect::<String>(),
            "image": cols[1].chars().take(COMMAND_MAX).collect::<String>(),
            "command": cols.get(2).map(|c| c.chars().take(COMMAND_MAX).collect::<String>()),
        }));
    }
    let n = out.len() as u64;
    (n, out)
}

/// The reading, as one object: the file systems, and with `--full` or
/// `--publish` the places that fill them and the containers Docker left
/// stopped. `--publish` is one run, so the published file names the stopped
/// containers beside the places: asking the daemon the same question twice for
/// one answer would be two calls where one does, and a person who asked for
/// `--full` is given the same object whether or not it is being published.
fn answer(host: &Host, opts: &Options) -> Value {
    let docker_dir = docker_root_dir(Duration::from_secs(opts.bounds.wait));
    let paths = watched(host, opts, &docker_dir);
    let mut out = Map::new();
    out.insert("api".into(), json!(1));
    out.insert("generatedAt".into(), json!(time::iso(time::now())));
    insert(&mut out, "dockerRoot", Some(Value::from(docker_dir)));
    out.insert(
        "filesystems".into(),
        Value::Array(file_systems(&paths, opts).iter().map(Fs::value).collect()),
    );
    if opts.full {
        let mut full = Map::new();
        full.insert(
            "places".into(),
            Value::Array(places(host, opts).iter().map(Place::value).collect()),
        );
        full.insert(
            "docker".into(),
            docker_df(Duration::from_secs(opts.bounds.wait)),
        );
        // The containers left stopped belong to the published answer only: a
        // person who asked what fills the disk asked about the trees, and the
        // count of what the daemon still holds back is a second question,
        // asked of the daemon and not of the disk.
        if opts.publish {
            let (stopped, containers) = stopped_containers(Duration::from_secs(opts.bounds.wait));
            full.insert("stoppedContainers".into(), json!(stopped));
            if !containers.is_empty() {
                full.insert("containers".into(), Value::Array(containers));
            }
        }
        out.insert("full".into(), Value::Object(full));
    }
    Value::Object(out)
}

/// The answer `tehas disk` prints: the file systems and, with `--full`, what
/// fills them. The containers Docker left stopped are not asked for: the
/// figures of the daemon are the sizes of images and layers, which a person
/// reads as `docker system df` prints them, and the answer they want is what
/// fills the trees. `--publish` names them, and asks the daemon once.
pub fn read(host: &Host, opts: &Options) -> Value {
    answer(host, opts)
}

/// The answer `--publish` writes, which is the same reading with the
/// containers Docker left stopped named beside it: the file is one run's whole
/// answer, and the daemon is asked the one question once for it.
pub fn published(host: &Host, opts: &Options) -> Value {
    answer(
        host,
        &Options {
            publish: true,
            full: true,
            ..opts.clone()
        },
    )
}

/// The file is put whole: written beside itself and renamed, as the poll's
/// report and live/epics.json are. A reader sees a whole file or the one
/// before it, and a run that could not write leaves the last answer there.
fn put(out: &Path, answer: &Value) -> std::io::Result<()> {
    let text = format!("{answer}\n");
    if text.len() as u64 > PUBLISH_BOUND {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "the answer is bigger than the file may hold",
        ));
    }
    let parent = out
        .parent()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no directory"))?;
    fs::create_dir_all(parent)?;
    let mut side = out.as_os_str().to_owned();
    side.push(format!(".{}.new", std::process::id()));
    let side = PathBuf::from(side);
    fs::write(&side, &text).inspect_err(|_| {
        let _ = fs::remove_file(&side);
    })?;
    fs::rename(&side, out).inspect_err(|_| {
        let _ = fs::remove_file(&side);
    })
}

/// The lines a person is given: one per file system, then what fills them.
pub fn lines(answer: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for fs in answer["filesystems"].as_array().into_iter().flatten() {
        let paths: Vec<&str> = fs["paths"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let name = paths.first().copied().unwrap_or("?");
        let mount = fs["mount"].as_str().unwrap_or("unknown");
        match (fs["freeBytes"].as_u64(), fs["totalBytes"].as_u64()) {
            (Some(free), Some(total)) => out.push(format!(
                "disk      {}: {mount}, {} of {} free ({})",
                name,
                gib(free),
                gib(total),
                used(free, total)
            )),
            _ => out.push(format!(
                "disk      {}: {}",
                name,
                fs["why"].as_str().unwrap_or("no figure")
            )),
        }
        if paths.len() > 1 {
            out.push(format!(
                "          one file system also holds {}",
                paths[1..].join(", ")
            ));
        }
    }
    if answer["full"]["places"].is_array() {
        for place in answer["full"]["places"].as_array().into_iter().flatten() {
            let path = place["path"].as_str().unwrap_or("?");
            let said = match place["bytes"].as_u64() {
                Some(b) => format!("{}{}", gib(b), cut_word(place["cut"].as_str())),
                None => "could not be measured".to_string(),
            };
            out.push(format!("fills     {path}: {said}"));
        }
        if let Some(n) = answer["full"]["stoppedContainers"].as_u64() {
            out.push(format!("fills     containers left stopped: {n}"));
        }
        match answer["full"]["docker"]["state"].as_str() {
            Some("read") => out.push("fills     docker system df:".to_string()),
            _ => out.push("fills     docker system df: no answer".to_string()),
        }
        for line in answer["full"]["docker"]["lines"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            out.push(format!("          {line}"));
        }
    }
    out
}

/// The name of a bound that was reached, in the words that say the figure is
/// not the whole of the place.
fn cut_word(cut: Option<&str>) -> String {
    match cut {
        None | Some("whole") => String::new(),
        Some("time") => " so far, ran out of time".to_string(),
        Some("entries") => " so far, ran out of entries".to_string(),
        Some("depth") => " so far, ran out of depth".to_string(),
        Some(other) => format!(" ({other})"),
    }
}

/// Bytes in GiB, one decimal, as the factory's other figures are said.
fn gib(bytes: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    if bytes >= 10 * (1u64 << 30) {
        format!("{:.0} GiB", bytes as f64 / GIB)
    } else {
        format!("{:.1} GiB", bytes as f64 / GIB)
    }
}

/// The share of a file system in use, as `df` prints its Capacity.
fn used(free: u64, total: u64) -> String {
    if total == 0 {
        return "?".to_string();
    }
    let used = total.saturating_sub(free);
    format!("{}% used", (used * 100) / total)
}

#[cfg(test)]
mod tests {
    use super::{
        cut_word, du, file_systems, gib, lines, one, places, published, put, read, used, Bounds,
        Host, Options, Place, PUBLISH_BOUND,
    };
    use super::{free, free_wait};
    use serde_json::{json, Value};
    use std::fs;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::path::{Path, PathBuf};

    struct Dir(PathBuf);

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// A scratch home of this case's own. Nothing here writes outside it, and
    /// nothing here removes anything of the host's.
    fn dir(name: &str) -> Dir {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let d = std::env::temp_dir().join(format!(
            "tehas-disk-{}-{name}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        Dir(d)
    }

    /// The host a reading is given, for a home this case made: the agent
    /// directory beside it, so a case plants both trees and names them.
    fn host(home: &Path) -> Host {
        Host {
            home: home.to_path_buf(),
            agent: home.join(".pi/agent"),
        }
    }

    fn opts() -> Options {
        Options {
            bounds: Bounds { wait: 2, walk: 2 },
            ..Options::default()
        }
    }

    fn file(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![b'x'; bytes]).unwrap();
    }

    // The two paths of one file system are one entry that names both: the
    // common case of a host whose home, /tmp and Docker root are all of the
    // same disk, which is why the reading groups by device at all.
    #[test]
    fn the_bound_of_free_is_a_number_from_1_to_99_and_10_otherwise() {
        assert_eq!(free_wait("3"), 3);
        assert_eq!(free_wait("99"), 99);
        for word in ["", "0", "07", "100", "+5", "abc", "٣"] {
            assert_eq!(free_wait(word), 10, "{word}");
        }
    }

    #[test]
    fn free_of_no_path_is_no_figure_and_of_too_much_a_usage_error() {
        assert_eq!(free(&[String::new()]), 1);
        assert_eq!(free(&[]), 64);
        assert_eq!(free(&["/a".into(), "1".into(), "x".into()]), 64);
    }

    #[test]
    fn two_paths_of_one_file_system_are_one_entry_that_names_both() {
        let d = dir("same");
        let home = d.0.join("tehas");
        let scratch = d.0.join("scratch");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&scratch).unwrap();
        let got = file_systems(&[home.clone(), scratch.clone()], &opts());
        assert_eq!(got.len(), 1, "one device, one entry: {got:?}");
        assert_eq!(
            got[0].paths,
            vec![home.to_string_lossy(), scratch.to_string_lossy()],
            "both names, each once"
        );
        assert!(got[0].free.is_some(), "{:?}", got[0]);
        assert!(got[0].total.is_some(), "{:?}", got[0]);
        // The figures are the file system's, not the first path's: the entry
        // says what is free on the disk both of them stand on.
        assert_eq!(got[0].paths.len(), 2);
    }

    // A path that cannot be looked at is an entry of its own with no figure
    // and the reason, and does not take another path with it: the figure of
    // the disk it might have been on is no answer for it.
    #[test]
    fn a_path_that_cannot_be_told_is_its_own_entry_with_a_reason() {
        let d = dir("gone");
        let there = d.0.join("tehas");
        fs::create_dir_all(&there).unwrap();
        let gone = d.0.join("nothing");
        let got = file_systems(&[there.clone(), gone.clone()], &opts());
        assert_eq!(got.len(), 2, "the unreadable one stands alone: {got:?}");
        assert_eq!(got[0].paths, vec![there.to_string_lossy()]);
        assert_eq!(got[1].paths, vec![gone.to_string_lossy()]);
        assert_eq!(got[1].free, None);
        assert_eq!(got[1].total, None);
        assert!(got[1].why.is_some(), "{:?}", got[1]);
    }

    // A figure df does not give is null with a reason, never 0: the same rule
    // the dispatcher's hold reads by, that a disk whose size is unknown is not
    // a disk with room on it.
    #[test]
    fn a_df_that_fails_is_no_figure_and_says_why() {
        // A path df cannot answer for: no such file, and nothing to stat.
        let d = dir("df-fails");
        let gone = d.0.join("no-such-place");
        let got = one(&gone, "/no/such/place", &opts());
        assert_eq!(got.free, None);
        assert_eq!(got.total, None);
        assert_eq!(got.mount, None);
        let why = got.why.clone().unwrap_or_default();
        assert!(why.contains("df"), "{why}");
        // No zero figure is ever served: a caller that saw 0 free would hold
        // every start on a host whose df is merely broken.
        assert_ne!(got.free, Some(0));
    }

    // df's own words that are no number are no figure, which is the guard the
    // bash keeps: a figure the caller's arithmetic wraps is worse than none.
    #[test]
    fn a_word_that_is_no_number_is_no_figure() {
        for word in ["-", "abc", "01", "-4", "99999999999999999999", ""] {
            let line = format!("/dev/x 100 1 {word} 1% /");
            let cols: Vec<&str> = line.split_whitespace().collect();
            let number = |w: &str| -> Option<u64> {
                (!w.is_empty()
                    && w.len() <= 18
                    && (w == "0" || (!w.starts_with('0') && w.bytes().all(|b| b.is_ascii_digit()))))
                .then(|| w.parse().ok())
                .flatten()
            };
            assert_eq!(number(cols[3]), None, "{word} is no figure");
        }
        assert_eq!(
            // A real line: free and total as 1024-blocks, made into bytes.
            {
                let cols = ["overlay", "100", "1", "40", "1%", "/"];
                (cols[3], cols[1])
            },
            ("40", "100")
        );
    }

    // --full lists a clone's target and orders the places by size, the biggest
    // first: the question the command exists to answer.
    #[test]
    fn full_lists_a_clone_s_target_and_orders_by_size() {
        let d = dir("full");
        let home = d.0.join("tehas");
        fs::create_dir_all(home.join("dev/alpha.state/target/debug")).unwrap();
        file(&home.join("dev/alpha.state/target/debug/lib.rmeta"), 4000);
        file(&home.join("logs"), 100);
        let host = Host {
            home: home.clone(),
            agent: d.0.join("agent"),
        };
        // Wide bounds: this case is about which place is biggest, not about a
        // bound, and a place the bounds cut is not the whole of it.
        let wide = Options {
            entries: 100,
            depth: 20,
            ..opts()
        };
        fs::create_dir_all(&host.agent).unwrap();
        file(&host.agent.join("runs.ndjson"), 10);
        let got = places(&host, &wide);
        let paths: Vec<&str> = got.iter().map(|p| p.path.as_str()).collect();
        let target = home.join("dev/alpha.state/target");
        assert!(
            paths.contains(&target.to_string_lossy().as_ref()),
            "the clone's target is named: {paths:?}"
        );
        // The measured places come first, biggest first, and none of them is
        // the clone's tree above its build output: `dev` itself is not a place
        // of the answer, only what a session built inside it is.
        let measured: Vec<&Place> = got.iter().filter(|p| p.bytes.is_some()).collect();
        let bytes: Vec<u64> = measured.iter().filter_map(|p| p.bytes).collect();
        let mut sorted = bytes.clone();
        sorted.sort_unstable_by(|a, b| b.cmp(a));
        assert_eq!(bytes, sorted, "biggest first: {got:?}");
        assert_eq!(measured[0].path, target.to_string_lossy());
        assert_eq!(measured[0].bytes, Some(4000));
        let paths: Vec<&str> = got.iter().map(|p| p.path.as_str()).collect();
        for not_a_place in [
            home.join("dev").to_string_lossy().as_ref(),
            home.join("dev/alpha.state").to_string_lossy().as_ref(),
        ] {
            assert!(
                !paths.contains(&not_a_place),
                "{not_a_place} is no place: {paths:?}"
            );
        }
    }

    // A place over its bound says which bound it ran out of rather than
    // nothing: the reading gives up, and says so, instead of stopping.
    #[test]
    fn a_place_over_its_bound_says_it_ran_out_of_time() {
        let d = dir("time");
        // Every walk of this case is out of time before it looks at a name.
        let tight = Options {
            bounds: Bounds { wait: 2, walk: 0 },
            ..opts()
        };
        file(&d.0.join("a/one"), 10);
        let (bytes, cut) = du(&d.0.join("a"), &tight);
        assert_eq!(cut, "time", "an over-bound walk says so");
        assert_eq!(bytes, None, "no part figure is called the whole");
        // Too many names, too deep, and a name that cannot be opened, each
        // name themselves the same way.
        let few = Options {
            entries: 1,
            ..opts()
        };
        file(&d.0.join("b/one"), 1);
        file(&d.0.join("b/two"), 1);
        assert_eq!(du(&d.0.join("b"), &few).1, "entries");
        let shallow = Options { depth: 1, ..opts() };
        file(&d.0.join("c/one/two"), 1);
        assert_eq!(du(&d.0.join("c"), &shallow).1, "depth");
        // A directory that cannot be opened is the bound of its parent's
        // reading, and says which one it is.
        let blocked = d.0.join("blocked");
        fs::create_dir_all(&blocked).unwrap();
        fs::set_permissions(&blocked, fs::Permissions::from_mode(0o000)).unwrap();
        assert_eq!(du(&blocked, &opts()), (None, "unreadable"));
        fs::set_permissions(&blocked, fs::Permissions::from_mode(0o755)).unwrap();
    }

    // A plain file is its own bytes, and a whole walk is `whole`: the figure
    // a person is told as it stands.
    #[test]
    fn a_walk_adds_the_regular_files_it_can_reach() {
        let d = dir("walk");
        file(&d.0.join("a/one"), 30);
        file(&d.0.join("a/b/two"), 70);
        let (bytes, cut) = du(&d.0.join("a"), &opts());
        assert_eq!(bytes, Some(100));
        assert_eq!(cut, "whole");
        assert_eq!(du(&d.0.join("a/one"), &opts()), (Some(30), "whole"));
        // A link is named and not entered, so a tree that points at another
        // file system adds nothing of it, and a link whose target is gone does
        // not make the directory that holds it unreadable.
        let link_dir = d.0.join("linkdir");
        fs::create_dir_all(&link_dir).unwrap();
        symlink("/nonexistent-outside-the-tree", link_dir.join("gone")).unwrap();
        symlink("/tmp", link_dir.join("other-disk")).unwrap();
        file(&link_dir.join("own"), 4);
        assert_eq!(
            du(&link_dir, &opts()),
            (Some(4), "whole"),
            "the two links add nothing and stop nothing"
        );
        // A link given as the place itself has no figure: it is named, not
        // followed, and nothing it points at is counted as bytes of this tree.
        let outside = d.0.join("outside");
        symlink("/tmp", &outside).unwrap();
        assert_eq!(du(&outside, &opts()), (None, "unreadable"));
    }

    // The object the command prints: api first, the file systems once, and the
    // figures of a host that answered. Nothing here is 0 because it was
    // missing.
    #[test]
    fn the_answer_is_one_object_with_its_api_and_one_entry_each() {
        let d = dir("answer");
        let home = d.0.join("tehas");
        fs::create_dir_all(&home).unwrap();
        let opts = opts();
        let answer = read(&host(&home), &opts);
        assert_eq!(answer["api"], 1);
        assert!(answer["generatedAt"].as_str().unwrap().ends_with('Z'));
        let entries = answer["filesystems"].as_array().unwrap();
        // Of the four paths the answer is read from, the scratch disk is the
        // one a host of these tests has none of: what is there is named, and
        // what the settings do not name is no entry at all.
        let asked: Vec<String> = entries
            .iter()
            .flat_map(|e| e["paths"].as_array().unwrap().iter())
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        assert!(
            asked.contains(&home.to_string_lossy().to_string()),
            "{asked:?}"
        );
        assert!(!asked.iter().any(|p| p.ends_with("scratch")), "{asked:?}");
        // Each is answered either with its figures or with a reason, never
        // with both and never with neither.
        for entry in entries {
            let figure = entry["freeBytes"].is_number() && entry["totalBytes"].is_number();
            let reason = entry["why"].is_string();
            assert!(figure ^ reason, "{entry}");
        }
        assert!(answer.get("full").is_none(), "no --full, no places");
        // The object is one line of JSON, which is what --json promises.
        let text = answer.to_string();
        assert!(!text.contains('\n'));
    }

    // A home that cannot be looked at answers with the reason, not a figure:
    // the same refusal the dispatcher's hold makes of a broken df.
    #[test]
    fn a_home_that_is_not_there_has_no_figure_and_a_reason() {
        let d = dir("no-home");
        let gone = d.0.join("no-home-at-all");
        let answer = read(&host(&gone), &opts());
        let entries = answer["filesystems"].as_array().unwrap();
        let home = &entries[0];
        assert_eq!(
            home["paths"][0],
            Value::from(gone.to_string_lossy().to_string())
        );
        assert_eq!(home["freeBytes"], Value::Null, "{home}");
        assert!(home["why"].is_string(), "{home}");
        assert!(home.get("freeBytes").is_some() || home.get("why").is_some());
    }

    // The lines a person reads: one per file system, the paths that share it
    // named on the line after, and the reason where a figure is not there.
    #[test]
    fn the_lines_name_each_file_system_once() {
        let answer = json!({
            "api": 1,
            "filesystems": [
                {"paths": ["/home/boss/tehas", "/tmp"], "mount": "overlay",
                 "freeBytes": 32_212_254_720u64, "totalBytes": 64_424_509_440u64},
                {"paths": ["/mnt/scratch"], "why": "/mnt/scratch: df gave no figure"},
            ],
        });
        let lines = lines(&answer);
        assert_eq!(lines.len(), 3, "{lines:?}");
        assert!(
            lines[0].starts_with(
                "disk      /home/boss/tehas: overlay, 30 GiB of 60 GiB free (50% used)"
            ),
            "{}",
            lines[0]
        );
        assert_eq!(lines[1], "          one file system also holds /tmp");
        assert_eq!(
            lines[2],
            "disk      /mnt/scratch: /mnt/scratch: df gave no figure"
        );
    }

    // The published file is put whole, and nothing of the tree that name
    // belongs to is touched by a run that could not write it: a reader sees a
    // whole answer or the last one, never a half and never a file somebody
    // else left under the name.
    #[test]
    fn the_published_file_is_put_whole_and_a_failed_run_leaves_the_last_one() {
        let d = dir("publish");
        let out = d.0.join("live/disk.json");
        put(&out, &json!({"api": 1})).unwrap();
        assert_eq!(fs::read_to_string(&out).unwrap(), "{\"api\":1}\n");
        // A second run replaces it, and leaves no side file behind.
        put(&out, &json!({"api": 1, "x": 2})).unwrap();
        assert_eq!(fs::read_to_string(&out).unwrap(), "{\"api\":1,\"x\":2}\n");
        assert_eq!(fs::read_dir(d.0.join("live")).unwrap().count(), 1);
        // An answer too big for the file is refused before anything is put,
        // and the file a reader may open at any moment is still the whole of
        // the last answer.
        let big = json!({"blob": "x".repeat(PUBLISH_BOUND as usize + 10)});
        assert!(put(&out, &big).is_err());
        assert_eq!(fs::read_to_string(&out).unwrap(), "{\"api\":1,\"x\":2}\n");
        assert_eq!(fs::read_dir(d.0.join("live")).unwrap().count(), 1);
        // A name that holds a directory is no file: the run says so, and the
        // tree beside it is left as it was.
        let as_dir = d.0.join("as-dir");
        fs::create_dir_all(&as_dir).unwrap();
        fs::write(d.0.join("elsewhere"), "do not touch").unwrap();
        assert!(put(&as_dir, &json!({})).is_err());
        assert_eq!(
            fs::read_to_string(d.0.join("elsewhere")).unwrap(),
            "do not touch"
        );
        // A name that holds a link is not written through: the side file is
        // renamed over the name, so the answer becomes a file of this run's
        // own and the tree the link pointed at is left as it was.
        let target = d.0.join("elsewhere");
        let as_link = d.0.join("as-link");
        symlink(&target, &as_link).unwrap();
        assert!(put(&as_link, &json!({"api": 9})).is_ok());
        assert_eq!(fs::read_to_string(&target).unwrap(), "do not touch");
        assert_eq!(fs::read_to_string(&as_link).unwrap(), "{\"api\":9}\n");
        assert!(fs::symlink_metadata(&as_link)
            .unwrap()
            .file_type()
            .is_file());
    }

    // The published answer is the reading of one run: the file systems, what
    // fills them, and the containers the daemon still holds back, named once,
    // with no second question asked for the same figures.
    #[test]
    fn the_published_answer_names_the_places_and_the_stopped_containers() {
        let d = dir("published");
        let home = d.0.join("tehas");
        file(&home.join("logs/one.log"), 900);
        file(&home.join("dev/alpha.state/target/x"), 100);
        let published = published(&host(&home), &opts());
        assert_eq!(published["api"], 1);
        assert!(published["full"]["places"].is_array(), "{published}");
        // The count is there whether or not the host has Docker: no answer is
        // none, which is what the file says of a host without it.
        assert!(published["full"]["stoppedContainers"].is_number());
        // The answer a person is given asks the daemon nothing about what it
        // holds back: the same reading, without the container question.
        let printed = read(&host(&home), &opts());
        assert!(printed["full"].is_null(), "no --full, no places: {printed}");
        let full = read(
            &host(&home),
            &Options {
                full: true,
                ..opts()
            },
        );
        assert!(full["full"]["places"].is_array());
        assert!(
            full["full"].get("stoppedContainers").is_none(),
            "the person's answer is not the file's: {full}"
        );
    }

    // The scratch disk is the fourth file system, and it is named only when
    // the settings named it: a host with no TEHAS_SCRATCH_DIR has no entry,
    // and one whose path cannot be read has an entry that says so.
    #[test]
    fn the_scratch_disk_is_named_only_when_the_settings_name_one() {
        let d = dir("scratch");
        let home = d.0.join("tehas");
        let scratch = d.0.join("mnt/scratch");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&scratch).unwrap();
        let named = Options {
            scratch: Some(scratch.clone()),
            ..opts()
        };
        let entries = read(&host(&home), &named)["filesystems".to_string()]
            .as_array()
            .unwrap()
            .clone();
        let asked: Vec<String> = entries
            .iter()
            .flat_map(|e| e["paths"].as_array().unwrap().iter())
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        assert!(
            asked.contains(&scratch.to_string_lossy().to_string()),
            "the disk the settings named is named: {asked:?}"
        );
        // A path that is not there is its own entry, with a reason and no
        // figure: a scratch disk that was never mounted is not a disk at all.
        let missing = Options {
            scratch: Some(d.0.join("mnt/never-mounted")),
            ..opts()
        };
        let entries = read(&host(&home), &missing)["filesystems".to_string()]
            .as_array()
            .unwrap()
            .clone();
        let named = d.0.join("mnt/never-mounted");
        let gone = entries
            .iter()
            .find(|e| e["paths"][0] == named.to_string_lossy().as_ref())
            .unwrap_or_else(|| panic!("the named path has its own entry: {entries:?}"));
        assert_eq!(gone["freeBytes"], Value::Null, "{gone}");
        assert!(gone["why"].is_string(), "{gone}");
    }

    // The size of a thing is said the way the factory says a size, and the
    // share of it in use the way df says it: figures a person compares with
    // tehas status and the previews' line.
    #[test]
    fn a_size_is_said_in_gibibytes_and_its_use_in_per_cent() {
        assert_eq!(gib(32_212_254_720), "30 GiB");
        assert_eq!(gib(5 * (1 << 30)), "5.0 GiB");
        assert_eq!(gib(1024), "0.0 GiB");
        assert_eq!(used(30 << 30, 60 << 30), "50% used");
        assert_eq!(used(0, 0), "?");
        assert_eq!(cut_word(None), "");
        assert_eq!(cut_word(Some("time")), " so far, ran out of time");
    }
}
