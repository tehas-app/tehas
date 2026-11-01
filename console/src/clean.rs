//! What `tehasbot clean` removes, for libexec/clean. The shell loads the
//! settings and reads the free disk; this names the two images, takes the
//! run's lock and runs the seven steps.
//!
//!   console clean image <PI_ENSEMBLE_IMAGE>
//!     The sandbox image without its tag. Exit 78 when that names no image.
//!
//!   console clean run <dry 0|1> <home> <work dir> <after hours>
//!     <min free MiB> <min free as set> <free KiB, or empty> <clean dirs>
//!     <sandbox image> <log file> <scratch dir>
//!     Exit 0 every step worked; 1 a step failed or the log could not be
//!     written; 3 another run holds the lock; 64 a usage error.
//!   console clean usage <the arguments of tehasbot clean>
//!     The help text, or an unknown argument and exit 64.
//!   console clean held <the image's tags, "<tag> <id>" per line>
//!     The ids something holds, one per line, for the sandbox build. Exit 1
//!     with nothing on stdout when docker would not say.
//!   console clean target <PI_RUKAS_FOLLOW> <PI_RUKAS_REPO>
//!     "<commit> <what it is>", as tehas_rukas_target of lib/rukas.sh prints
//!     it. Exit 1 with nothing printed when there is no answer.
//!
//! Only what has not been used for <after> hours goes. When the free disk is
//! under the minimum, steps 1 and 2 take build output and the build cache
//! whatever their age; a workspace whose lock a cycle holds is still never
//! touched.
//!
//! A cycle's scratch on the host's scratch disk goes with its lock free.
//!
//! 1. Build output in kept workspaces: every directory under an issue's
//!    workspace, the tree a retry moved aside, and each git worktree at depth
//!    two of either, whose name is one of the clean dirs; and
//!    `<name>-<n>.state/cache`. Dot-names are never entered, no symlink is
//!    followed, no other file system is walked, and each removal holds the
//!    issue's lock. The one git call, which names a worktree's gitdir for the
//!    age rule, runs bounded, by absolute path, under a cleared environment
//!    and no config, since a container can write the tree it reads.
//! 2. Docker's build cache: `docker builder prune` of that age.
//! 3. Host scratch: files of that age directly under tmp/dl and
//!    tmp/node-compile-cache, tmp/analytics-facts.* and tmp/health.* files of
//!    that age, the
//!    copy of the sandbox recipe (and its temporary) of that age a killed build
//!    left beside the build script, and tmp/tehas-test.* directories.
//! 4. Dated tags of the sandbox image of that age, except the newest and any
//!    a container or a uid tag holds.
//! 5. `sha-` tags of the base image's repository of that age, which no
//!    container was made from and no kept tag (uid, release-) stands on, each
//!    removed by its own name. Never `docker image prune`.
//! 6. `store upkeep`: old logs, a daily backup of the state store, prune.
//! 7. The build output of a manual session: `<name>.state/target` of every
//!    clone under `dev`, a plain directory of the run's own user and no link.
//!    Above the minimum free every clone keeps its build; under it a build of
//!    that age goes. Nothing else under `<name>.state` is
//!    named: the clone, its memory, sessions, runs, transcripts, author and
//!    images stay. The lock `tehas dev` holds for a session,
//!    `state/dev/<name>.lock` and no lock file of `state/cycles`, is what marks
//!    a session open, and this step holds it through the removal; one that is
//!    a link, a directory or a file no one can open is the same as held, and so
//!    is a docker that will not say whether the session's container stands.
//!
//! One line per removal and a closing line with the bytes freed go to the log
//! and to standard output; a warning goes to the log and standard error. A dry
//! run names each path it would take with the bytes `bytes_of` counts for it,
//! and closes with `would free <n> bytes`; the figure leaves out what only
//! docker or the store can measure (the build cache, the image tags and the
//! store's upkeep), and says so. A dry run takes no issue's lock: it removes
//! nothing, so it holds nothing.

use crate::store::{self, bounded, merged};
use serde_json::json;
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};
use tehas_core::{rukas, time};

const USAGE: &str = "usage: console clean run <dry 0|1> <home> <work dir> <after hours> <min free MiB> <min free> <free KiB> <dirs> <sandbox image> <log file> <scratch dir>\n       console clean image <PI_ENSEMBLE_IMAGE>\n       console clean usage <arguments>\n       console clean held <tags>\n       console clean target <follow> <repo>\n       console clean ended <workspace> <dirs> <log file> <scratch of the cycle>\n       console clean scratch <scratch dir> <name>\n       console clean session <home> <clone name> [<seconds>]";

/// What `tehasbot clean --help` prints.
const HELP: &str = r#"Remove what the host can make again: build output in kept workspaces,
Docker's build cache, old sandbox and base images, and old files under
~/tehas/tmp.

A full disk stops previews and fails clones. A kept workspace is never built
in again, since every cycle clones fresh. A cycle takes its build output as it
ends (libexec/cycle); this takes what a killed cycle left.

Only what has not been used for TEHAS_CLEAN_AFTER hours (24 by default)
goes. When the free disk is under TEHAS_CLEAN_MIN_FREE (30g by default),
steps 1 and 2 go whatever the age; a lock a cycle or a `tehas dev` holds is
still never touched.

  1. build output in kept workspaces   directories named in TEHAS_CLEAN_DIRS
                                       anywhere in an issue's workspace, the
                                       tree a retry moved aside and a
                                       cycle's worktrees, and the cycle's
                                       copy of the code index, under the
                                       issue's lock. No dot-name is entered
                                       and no symlink followed.
  2. Docker's build cache              `docker builder prune` of that age,
                                       the whole host daemon's
  3. host scratch                      under $TEHAS_HOME/tmp: dl, node-
                                       compile-cache, analytics-facts.*,
                                       tehas-test.* dirs, a staged recipe
                                       and the /tmp of a killed cycle on
                                       TEHAS_SCRATCH_DIR, under its lock
  4. old sandbox images                dated tags of that age, except the
                                       newest and any a container or a uid
                                       tag holds
  5. the old base images               `sha-` tags of the base's repository
                                       of that age that nothing on the host
                                       stands on, each by its own name
  6. logs and the state store          `store upkeep`: old logs, a daily
                                       backup, prune
  7. a manual session's build output   <name>.state/target under
                                       $TEHAS_HOME/dev, of that age, only
                                       under the minimum free and with no
                                       session open, under the clone's lock

The steps are the console's (console/src/clean.rs); this checks the settings,
reads the free disk and names the two images.

Docker volumes are never touched, a person's call (see the host manual).

One line per removal, and a closing line with the bytes freed, go to
$TEHAS_HOME/logs/clean.log. A docker step that fails is logged as a warning
and the other steps still run. `tehasbot clean` runs it by hand;
tehas-clean.timer runs it every hour. The pause label does not stop it: it
starts no work.

Usage:
  tehasbot clean              one run
  tehasbot clean --dry-run    log what it would remove and remove nothing
  tehasbot clean -h|--help    print this text and exit 0

Exit: 0 the run finished with every step working; 1 a step failed, or the log
could not be written (what could be removed was removed); 3 another run holds
its lock, which changes nothing; 64 an argument this command does not know;
78 instance settings are missing or malformed, or no console is installed,
before any docker, find or rm call.
"#;

pub fn main(given: &[String]) -> u8 {
    if given.first().is_some_and(|v| v == "ended") {
        return ended(&given[1..]);
    }
    if given.first().is_some_and(|v| v == "session") {
        return open_session(&given[1..]);
    }
    if given.first().is_some_and(|v| v == "scratch") {
        return scratch_made(&given[1..]);
    }
    if given.first().is_some_and(|v| v == "image") {
        return image(&given[1..]);
    }
    if given.first().is_some_and(|v| v == "usage") {
        return usage(&given[1..]);
    }
    if given.first().is_some_and(|v| v == "held") {
        return held(&given[1..]);
    }
    if given.first().is_some_and(|v| v == "target") {
        return target(&given[1..]);
    }
    let [verb, dry, home, work, after, min_mb, min_word, free_kb, dirs, sandbox, logfile, tmp_disk] =
        given
    else {
        eprintln!("{USAGE}");
        return 64;
    };
    let (Ok(after), Ok(min_mb)) = (after.parse::<u64>(), min_mb.parse::<u64>()) else {
        eprintln!("{USAGE}");
        return 64;
    };
    if verb != "run" || !(dry == "0" || dry == "1") {
        eprintln!("{USAGE}");
        return 64;
    }
    let pulled = pulled_base(Path::new(home)).unwrap_or_default();
    let (works, base) = match work.rsplit_once('/') {
        Some((w, b)) => (w.to_string(), b.to_string()),
        None => (work.clone(), work.clone()),
    };
    let mut run = Run {
        dry: dry == "1",
        home: PathBuf::from(home),
        works,
        base,
        after,
        build_after: after,
        low: false,
        dirs: dirs.split_whitespace().map(String::from).collect(),
        sandbox: sandbox.clone(),
        pulled,
        logfile: PathBuf::from(logfile),
        tmp_disk: tmp_disk.clone(),
        freed: 0,
        rc: 0,
    };
    run.main(min_mb, min_word, free_kb)
}

/// The sandbox image's name for the run, or 78 and what is wrong with it.
fn image(given: &[String]) -> u8 {
    let [image] = given else {
        eprintln!("{USAGE}");
        return 64;
    };
    let Some(repo) = image_repo(image) else {
        eprintln!("tehas: PI_ENSEMBLE_IMAGE={image} names no image docker can list");
        eprintln!("tehas clean: PI_ENSEMBLE_IMAGE={image} is not an image name");
        return 78;
    };
    println!("{repo}");
    0
}

/// The arguments the command does not run with: the help text when that is
/// all that was asked, else the first word it does not know.
fn usage(given: &[String]) -> u8 {
    if let [help] = given {
        if help == "-h" || help == "--help" {
            print!("{HELP}");
            return 0;
        }
    }
    let word = match given.first().map(String::as_str) {
        Some("-h" | "--help" | "--dry-run" | "") => given.get(1),
        _ => given.first(),
    };
    let word = word.map_or("", String::as_str);
    eprint!("tehasbot clean: unknown argument: {word}\n{HELP}");
    64
}

/// `images_held` for the sandbox build, so the two cannot drift apart on what
/// is safe to remove.
fn held(given: &[String]) -> u8 {
    let [tags] = given else {
        eprintln!("{USAGE}");
        return 64;
    };
    let Some(ids) = images_held(tags) else {
        return 1;
    };
    for id in ids {
        println!("{id}");
    }
    0
}

/// `rukas::target` on the real upstream, for test/rukas_test.sh to hold beside
/// lib/rukas.sh.
fn target(given: &[String]) -> u8 {
    let [follow, repo] = given else {
        eprintln!("{USAGE}");
        return 64;
    };
    match rukas::target(follow, repo, ls_remote) {
        Some((commit, what)) => {
            println!("{commit} {what}");
            0
        }
        None => 1,
    }
}

/// `git ls-remote`, bounded: the cleaner runs unattended on a timer and may
/// not wait on a network that does not answer.
fn ls_remote(args: &[&str]) -> Option<String> {
    let said = bounded("5", "30", "git")
        .arg("ls-remote")
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    said.status
        .success()
        .then(|| String::from_utf8_lossy(&said.stdout).into_owned())
}

/// The base image the host's pins name. The build pulls the same name, and
/// `home`'s versions.env is the one file read: an unreadable file names no base.
fn pulled_base(home: &Path) -> Option<String> {
    let versions = fs::read(home.join("versions.env")).ok()?;
    rukas::base_image(&String::from_utf8_lossy(&versions), ls_remote)
}

/// The name the build tags and the dated tags are listed under: `image`
/// without its tag. An instance that points PI_ENSEMBLE_IMAGE at another
/// repository keeps the build's tag for itself, and the dated tags of the
/// build's name go uncleaned. Only the part after the last slash may hold a
/// tag, so a repository that carries a registry and its port keeps it.
fn image_repo(image: &str) -> Option<String> {
    fn word(s: &str) -> bool {
        let wordy = |b: u8| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-');
        s.as_bytes().first().is_some_and(u8::is_ascii_alphanumeric) && s.bytes().all(wordy)
    }
    let (prefix, tail) = image.rsplit_once('/').map_or(("", image), |(p, t)| (p, t));
    let (name, tag) = tail.split_once(':').unwrap_or((tail, "a"));
    if !word(name) || !word(tag) {
        return None;
    }
    if image.contains('/') {
        Some(format!("{prefix}/{name}"))
    } else {
        Some(name.to_string())
    }
}

/// What a cycle does as it ends, under the issue's lock it still holds. No
/// cycle builds in a kept workspace again, so its build output goes now and
/// not a day later. A workspace already gone leaves nothing to do.
fn ended(given: &[String]) -> u8 {
    let [workspace, dirs, logfile, tmp] = given else {
        eprintln!("{USAGE}");
        return 64;
    };
    let mut run = Run {
        dry: false,
        home: PathBuf::new(),
        works: String::new(),
        base: String::new(),
        after: 0,
        build_after: 0,
        low: false,
        dirs: dirs.split_whitespace().map(String::from).collect(),
        sandbox: String::new(),
        pulled: String::new(),
        logfile: PathBuf::from(logfile),
        tmp_disk: String::new(),
        freed: 0,
        rc: 0,
    };
    run.ended(Path::new(workspace));
    if !tmp.is_empty() && !remove(Path::new(tmp)) {
        run.warn(&format!("could not remove {tmp}"));
        run.rc = 1;
    }
    run.rc
}

/// A cycle's own /tmp on the host's scratch disk, made empty and printed. A
/// directory that cannot be used is exit 78, before Docker is called.
fn scratch_made(given: &[String]) -> u8 {
    let [dir, name] = given else {
        eprintln!("{USAGE}");
        return 64;
    };
    let root = Path::new(dir);
    let p = root.join(name);
    let made = root.is_absolute()
        && root.is_dir()
        && remove(&p)
        && fs::create_dir(&p).is_ok()
        && fs::set_permissions(&p, fs::Permissions::from_mode(0o1777)).is_ok();
    if !made {
        eprintln!("tehas clean: {} could not be made", p.display());
        return 78;
    }
    out(&format!("{}\n", p.display()));
    0
}

/// `console clean session`: the lock of a manual session, prepared and named.
/// The cleaner's step for a clone's build output waits behind
/// `state/dev/<name>.lock` and no file of `state/cycles`, so this makes the
/// directory the session's cargo build output is mounted at - a plain directory
/// of the account the session runs as, never a link or a tree another account
/// left - makes the lock, and prints its path. `tehas dev` holds the lock itself
/// for the session's life, and ends when the container is gone. The wait is
/// spent in this call, which is what says who else holds the lock; the wait of
/// the shell that comes back is for a lock that went and came. Standard output
/// carries the path and nothing else, the words go to standard error.
fn open_session(given: &[String]) -> u8 {
    let [home, name, wait @ ..] = given else {
        eprintln!("{USAGE}");
        return 64;
    };
    // The path of the lock is the contract between this verb and libexec/dev:
    // one plain word of a name, so a clone cannot name another directory or
    // leave the tree of clones.
    if name.is_empty()
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        eprintln!("tehas clean: not a name for a clone: {name}");
        return 64;
    }
    let secs = match wait {
        [] => 300u64,
        [one] => match one.parse::<u64>() {
            Ok(0) | Err(_) => {
                eprintln!("tehas clean: not a number of seconds to wait: {one}");
                return 64;
            }
            Ok(n) => n,
        },
        _ => {
            eprintln!("{USAGE}");
            return 64;
        }
    };
    let devs = Path::new(home).join("dev");
    let state = devs.join(format!("{name}.state"));
    let target = state.join("target");
    let lock = Path::new(home)
        .join("state")
        .join("dev")
        .join(format!("{name}.lock"));
    let shown = lock.display().to_string();
    // Made apart, and the build output with a plain create: a create of one
    // path of a tree makes the directory beside it too, and then nothing left
    // at that path by a hand could be refused as the shape it is. The shape and
    // the owner are asked on their own, not taken on trust from the create,
    // because on a kept clone the directory is there already, which is the
    // normal path.
    let made = |p: &Path| match fs::create_dir(p) {
        Ok(()) => true,
        Err(e) => e.kind() == io::ErrorKind::AlreadyExists,
    };
    let prepared = fs::create_dir_all(devs).is_ok()
        && fs::create_dir_all(lock.parent().unwrap_or(Path::new(home))).is_ok()
        && made(&state)
        && fs::symlink_metadata(&state).is_ok_and(|m| m.is_dir())
        && made(&target)
        && fs::symlink_metadata(&target).is_ok_and(|m| m.is_dir() && m.uid() == the_uid());
    if !prepared {
        eprintln!(
            "tehas clean: {} is not a directory of this account, or could not be made",
            target.display()
        );
        return 78;
    }
    // The wait, spent here: a session that is merely running must not hold the
    // shell that asked past its own life, and the answer names who held it.
    let deadline = Instant::now() + Duration::from_secs(secs);
    let mut said = false;
    loop {
        match take_clone_lock(&lock) {
            Ok(locked) => {
                drop(locked);
                out(&format!("{shown}\n"));
                return 0;
            }
            Err(why) => {
                if Instant::now() >= deadline {
                    eprintln!("tehas clean: {shown} is held{why}");
                    return 78;
                }
                if !said {
                    eprintln!("tehas clean: {shown} is held{why}, waiting {secs} seconds for it");
                    said = true;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

/// The lock of one clone, taken once: the file that holds it, or what to say
/// beside `is held`. A link, a directory or a file that cannot be opened is
/// never taken, since a lock is held by descriptor and a lock of a name that is
/// no file proves nothing either way.
fn take_clone_lock(path: &Path) -> Result<File, String> {
    let f = OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .map_err(|e| format!(": it could not be opened ({e})"))?;
    if !fs::symlink_metadata(path).is_ok_and(|m| m.is_file()) {
        return Err(": its name is no regular file".to_string());
    }
    f.try_lock().map_err(|_| lock_holders(path))?;
    Ok(f)
}

/// Who holds a clone's lock, as the process table can say it: the login and the
/// command of each process with the file open. `fuser` answers the pids that
/// `ps` is asked about, and an answer of none is no word at all, which is what
/// the line beside it reads as "held, by nobody it could name".
fn lock_holders(path: &Path) -> String {
    let (Some(fuser), Some(ps)) = (which("fuser"), which("ps")) else {
        return String::new();
    };
    let pids = Command::new(fuser)
        .arg(path)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    // The asker has the file open too, and it is no holder.
    let me = std::process::id().to_string();
    let pids: Vec<&str> = pids
        .split_whitespace()
        .filter(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) && *p != me)
        .collect();
    if pids.is_empty() {
        return String::new();
    }
    let named = Command::new(ps)
        .args(["-o", "user=,comm="])
        .args(pids)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .split('\n')
                .filter(|l| !l.trim().is_empty())
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default();
    if named.is_empty() {
        return String::new();
    }
    format!(" by {named}")
}

struct Run {
    dry: bool,
    home: PathBuf,
    works: String,
    base: String,
    after: u64,
    build_after: u64,
    /// The free disk was under TEHAS_CLEAN_MIN_FREE when the run read it.
    low: bool,
    dirs: Vec<String>,
    sandbox: String,
    pulled: String,
    logfile: PathBuf,
    tmp_disk: String,
    freed: u64,
    rc: u8,
}

fn stamp() -> String {
    time::iso(time::now())
}

fn out(line: &str) {
    let _ = io::stdout().write_all(line.as_bytes());
    let _ = io::stdout().flush();
}

/// Bytes as the dispatcher's hold prints them: one decimal, 1.5 GiB.
fn gib(b: u64) -> String {
    let g = 1_073_741_824;
    format!("{}.{} GiB", b / g, b % g * 10 / g)
}

/// The first line of what a command said, as `head -n 1 <<<"${out:-default}"`.
fn first_line(said: &str, default: &str) -> String {
    let said = said.trim_end_matches('\n');
    if said.is_empty() {
        return default.to_string();
    }
    said.split('\n').next().unwrap_or("").to_string()
}

/// What docker printed on standard output, its trailing newlines cut, when it
/// answered 0. Bounded as the shell bounded it, standard error dropped.
fn docker_list(args: &[&str]) -> Option<String> {
    let o = bounded("30", "600", "docker")
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    o.status.success().then(|| {
        String::from_utf8_lossy(&o.stdout)
            .trim_end_matches('\n')
            .to_string()
    })
}

/// The first line docker printed, whatever its status.
fn docker_first(bound: bool, args: &[&str]) -> String {
    let mut cmd = if bound {
        bounded("30", "600", "docker")
    } else {
        Command::new("docker")
    };
    let said = cmd
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    said.lines().next().unwrap_or("").to_string()
}

/// The ids a container was made from, and those every uid tag of <lines>
/// ("<tag> <id>") names: what a sandbox build and this run must not remove.
/// None when docker would not say.
fn images_held(lines: &str) -> Option<Vec<String>> {
    let ps = Command::new("docker")
        .args(["ps", "-aq", "--no-trunc"])
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !ps.status.success() {
        return None;
    }
    let ids = String::from_utf8_lossy(&ps.stdout).into_owned();
    let mut held = Vec::new();
    if !ids.trim().is_empty() {
        let inspect = Command::new("docker")
            .args(["inspect", "--format", "{{.Image}}"])
            .args(ids.split_whitespace())
            .stdin(Stdio::null())
            .output()
            .ok()?;
        if !inspect.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&inspect.stdout);
        let text = text.trim_end_matches('\n');
        if !text.is_empty() {
            held.extend(text.split('\n').map(String::from));
        }
    }
    for line in lines.lines() {
        let mut f = line.split_whitespace();
        if f.next().is_some_and(|t| t.starts_with("uid")) {
            held.push(f.next().unwrap_or("").to_string());
        }
    }
    Some(held)
}

/// A creation time of the shape docker answers, as the shell's regex took it.
fn stamp_shaped(t: &str) -> bool {
    let b = t.as_bytes();
    let digits = |r: std::ops::Range<usize>| {
        b.get(r.clone())
            .is_some_and(|s| s.iter().all(u8::is_ascii_digit))
    };
    if b.len() < 19
        || !digits(0..4)
        || b[4] != b'-'
        || !digits(5..7)
        || b[7] != b'-'
        || !digits(8..10)
        || b[10] != b'T'
        || !digits(11..13)
        || b[13] != b':'
        || !digits(14..16)
        || b[16] != b':'
        || !digits(17..19)
    {
        return false;
    }
    let mut i = 19;
    if b.get(i) == Some(&b'.') {
        let n = b[i + 1..].iter().take_while(|c| c.is_ascii_digit()).count();
        if n == 0 {
            return false;
        }
        i += 1 + n;
    }
    match &b[i..] {
        [] | [b'Z'] => true,
        [b'+' | b'-', zone @ ..] => {
            let zone: Vec<u8> = zone.iter().copied().filter(|c| *c != b':').collect();
            let colons = b[i + 1..].iter().filter(|c| **c == b':').count();
            zone.len() == 4
                && zone.iter().all(u8::is_ascii_digit)
                && (colons == 0 || (colons == 1 && b.get(i + 3) == Some(&b':')))
        }
        _ => false,
    }
}

/// Whether a dated tag line of the sandbox image: `YYMMDD-HHMM[-product] `.
fn dated(line: &str) -> bool {
    let b = line.as_bytes();
    let digits = |s: &[u8]| s.iter().all(u8::is_ascii_digit);
    b.len() > 11
        && digits(&b[0..6])
        && b[6] == b'-'
        && digits(&b[7..11])
        && (b[11] == b' ' || line[11..].starts_with("-product "))
}

fn is_id(s: &str) -> bool {
    let hex = s.strip_prefix("sha256:").unwrap_or(s);
    hex.len() == 64 && hex.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}

/// The issue number of a name under the work dir: `<base>-<n>` or
/// `<base>-<n>.before-retry`.
fn issue_of(name: &str, base: &str) -> Option<String> {
    let rest = name.strip_prefix(base)?.strip_prefix('-')?;
    let n = rest.strip_suffix(".before-retry").unwrap_or(rest);
    let b = n.as_bytes();
    (!b.is_empty() && b.len() <= 10 && b[0] != b'0' && b.iter().all(u8::is_ascii_digit))
        .then(|| n.to_string())
}

/// The repository of an image name: up to the @ of a digest, then up to the
/// last colon.
fn repo_of(pulled: &str) -> &str {
    let r = pulled.split('@').next().unwrap_or(pulled);
    r.rsplit_once(':').map_or(r, |(a, _)| a)
}

/// The account of this run: what `stat -c %u` answers and what a directory of
/// the session's own user carries.
fn the_uid() -> u32 {
    // SAFETY: getuid takes no argument and has no preconditions.
    unsafe { libc::getuid() }
}

fn is_link(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink())
}

fn real_dir(p: &Path) -> bool {
    p.is_dir() && !is_link(p)
}

fn plain_file(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok_and(|m| m.is_file())
}

/// Whether <t> is the name of a sandbox build's staged recipe: the name docker
/// is given, or the `mktemp` temporary it renames from. The temporary's suffix is
/// the six characters `mktemp XXXXXX` makes, each one of its alphabet, so a name
/// of six bytes that is no mktemp name is not taken for one. The same shape
/// `host/sandbox/build.sh` sweeps and `.gitignore` ignores.
fn staged_recipe_shaped(n: &str) -> bool {
    n == "sandbox-uid.Dockerfile"
        || n.strip_prefix("sandbox-uid.Dockerfile.")
            .is_some_and(|s| s.len() == 6 && s.bytes().all(|b| b.is_ascii_alphanumeric()))
}

/// Written within <hours>, as `find -newermt "-<hours> hours"` finds it; None
/// when it cannot be read.
fn newer(p: &Path, hours: u64) -> Option<bool> {
    let m = fs::symlink_metadata(p).ok()?.modified().ok()?;
    let since = SystemTime::now().checked_sub(Duration::from_secs(hours * 3600))?;
    Some(m > since)
}

/// The bytes a path holds, as `du -sb` counts a directory and `stat -c %s` a
/// file. No link is followed and a file of many names counts once.
fn bytes_of(p: &Path) -> u64 {
    if !p.is_dir() {
        return fs::symlink_metadata(p).map_or(0, |m| m.len());
    }
    let mut total = 0;
    let mut seen = HashSet::new();
    let mut stack = vec![p.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(m) = fs::symlink_metadata(&d) else {
            continue;
        };
        if m.nlink() > 1 && !m.is_dir() && !seen.insert((m.dev(), m.ino())) {
            continue;
        }
        total += m.len();
        if m.is_dir() {
            if let Ok(entries) = fs::read_dir(&d) {
                stack.extend(entries.flatten().map(|e| e.path()));
            }
        }
    }
    total
}

/// `rm -rf`: true when nothing is left at the path.
fn remove(p: &Path) -> bool {
    let r = match fs::symlink_metadata(p) {
        Err(e) => return e.kind() == io::ErrorKind::NotFound,
        Ok(m) if m.is_dir() => fs::remove_dir_all(p),
        Ok(_) => fs::remove_file(p),
    };
    r.is_ok() || !p.exists() && !is_link(p)
}

fn starts_dot(p: &Path) -> bool {
    p.file_name()
        .is_some_and(|n| n.as_bytes().first() == Some(&b'.'))
}

/// The build output of one tree: every directory named one of <names>, the
/// root included, as `find -P <root> -xdev \( -name '.*' -prune \) -o \(
/// -type d -name ... -print \)` lists it. Nothing is removed here, so the dry
/// run and the run ask the same question.
fn build_output(root: &Path, names: &[String]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if !root.is_dir() {
        return found;
    }
    let Ok(top) = fs::symlink_metadata(root) else {
        return found;
    };
    let dev = top.dev();
    let mut stack = vec![(root.to_path_buf(), top)];
    while let Some((p, m)) = stack.pop() {
        if starts_dot(&p) || !m.is_dir() {
            continue;
        }
        if p.file_name()
            .is_some_and(|n| names.iter().any(|d| d.as_bytes() == n.as_bytes()))
        {
            found.push(p.clone());
        }
        if m.dev() != dev {
            continue;
        }
        if let Ok(entries) = fs::read_dir(&p) {
            let mut below: Vec<_> = entries
                .flatten()
                .filter_map(|e| fs::symlink_metadata(e.path()).ok().map(|m| (e.path(), m)))
                .collect();
            below.reverse();
            stack.extend(below);
        }
    }
    found
}

/// Whether git may be asked of a directory: a real directory on the root's
/// device whose .git is a file with a gitdir line. The bytes are a
/// container's, so this only picks what git is asked about.
fn is_worktree(d: &Path, dev: u64) -> bool {
    if !real_dir(d) {
        return false;
    }
    let dotgit = d.join(".git");
    if !dotgit.is_file() {
        return false;
    }
    let Ok(text) = fs::read(&dotgit) else {
        return false;
    };
    text.split(|c| *c == b'\n')
        .any(|l| l.starts_with(b"gitdir:"))
        && fs::symlink_metadata(d).is_ok_and(|m| m.dev() == dev)
}

/// The workspace, the tree a retry moved aside, and each worktree two levels
/// under either: where the runner puts a cycle's worktrees, under a dot-name
/// the walk never enters.
fn roots_of(p: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let retry = PathBuf::from(format!("{}.before-retry", p.display()));
    for t in [p.to_path_buf(), retry] {
        if !t.is_dir() {
            continue;
        }
        roots.push(t.clone());
        let Ok(m) = fs::symlink_metadata(&t) else {
            continue;
        };
        if m.file_type().is_symlink() {
            continue;
        }
        let dev = m.dev();
        let Ok(level1) = fs::read_dir(&t) else {
            continue;
        };
        for e1 in level1.flatten() {
            let one = e1.path();
            if !fs::symlink_metadata(&one).is_ok_and(|m| m.is_dir() && m.dev() == dev) {
                continue;
            }
            let Ok(level2) = fs::read_dir(&one) else {
                continue;
            };
            for e2 in level2.flatten() {
                let w = e2.path();
                if fs::symlink_metadata(&w).is_ok_and(|m| m.is_dir()) && is_worktree(&w, dev) {
                    roots.push(w);
                }
            }
        }
    }
    roots
}

/// The absolute path `command -v` would answer.
fn which(name: &str) -> Option<String> {
    let path = std::env::var("PATH").ok()?;
    path.split(':').find_map(|dir| {
        let p = Path::new(dir).join(name);
        let m = fs::metadata(&p).ok()?;
        (m.is_file() && m.permissions().mode() & 0o111 != 0).then(|| p.display().to_string())
    })
}

impl Run {
    fn ended(&mut self, workspace: &Path) {
        let found: Vec<PathBuf> = roots_of(workspace)
            .iter()
            .flat_map(|root| build_output(root, &self.dirs))
            .collect();
        for dir in found {
            if !real_dir(&dir) {
                continue;
            }
            let shown = dir.display().to_string();
            let size = bytes_of(&dir);
            if remove(&dir) {
                self.log(&format!(
                    "removed {shown}: build output of a cycle that ended, {size} bytes"
                ));
            } else {
                self.warn(&format!("could not remove {shown}"));
                self.rc = 1;
            }
        }
    }

    fn log(&mut self, msg: &str) {
        let line = format!("{} clean: {msg}\n", stamp());
        let wrote = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.logfile)
            .and_then(|mut f| f.write_all(line.as_bytes()));
        if wrote.is_err() {
            eprintln!(
                "{} clean: the log {} could not be written",
                stamp(),
                self.logfile.display()
            );
            self.rc = 1;
        }
        out(&line);
    }

    fn warn(&self, msg: &str) {
        let line = format!("{} WARN: clean: {msg}\n", stamp());
        let _ = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.logfile)
            .and_then(|mut f| f.write_all(line.as_bytes()));
        let _ = io::stderr().write_all(line.as_bytes());
    }

    fn main(&mut self, min_mb: u64, min_word: &str, free_kb: &str) -> u8 {
        let state = self.home.join("state");
        let logs = self.logfile.parent().map(Path::to_path_buf);
        if logs.is_some_and(|l| fs::create_dir_all(l).is_err())
            || fs::create_dir_all(&state).is_err()
        {
            eprintln!("tehas clean: {} could not be made", state.display());
            return 1;
        }
        // One run at a time. A run that found another working leaves it to that one.
        let lock = OpenOptions::new()
            .append(true)
            .create(true)
            .open(state.join("clean.lock"));
        let held = lock.as_ref().map(File::try_lock);
        if !matches!(held, Ok(Ok(()))) {
            if let Ok(Err(TryLockError::WouldBlock)) = held {
                out(&format!(
                    "{} clean: another run holds the lock, nothing was removed\n",
                    stamp()
                ));
                return 3;
            }
            eprintln!(
                "tehas clean: {} could not be locked",
                state.join("clean.lock").display()
            );
            return 1;
        }
        // Read once, so steps 1 and 2 answer for one moment. A figure that
        // cannot be read changes nothing.
        if let Ok(kb) = free_kb.parse::<u64>() {
            if kb < min_mb * 1024 {
                self.build_after = 0;
                self.low = true;
                self.log(&format!("low on disk: {} free, under TEHAS_CLEAN_MIN_FREE={min_word}: build output and build cache go whatever their age", gib(kb * 1024)));
            }
        }
        // Every step runs, whatever the ones before it did.
        let steps = [
            self.workspaces(),
            self.build_cache(),
            self.scratch(),
            self.cycle_scratch(),
            self.sandbox_images(),
            self.base_images(),
            self.upkeep(),
            self.clone_targets(),
        ];
        if steps.contains(&false) {
            self.rc = 1;
        }
        let freed = self.freed;
        if self.dry {
            self.log(&format!(
                "would free {freed} bytes, which leaves out the build cache, the image tags and the store's upkeep, whose size is not known beforehand"
            ));
        } else {
            self.log(&format!("freed {freed} bytes"));
        }
        self.rc
    }

    /// The /tmp a cycle killed before its end left on the scratch disk. Under
    /// the issue's lock, and only where the lock is there to hold.
    fn cycle_scratch(&mut self) -> bool {
        if self.tmp_disk.is_empty() {
            return true;
        }
        let Ok(entries) = fs::read_dir(&self.tmp_disk) else {
            self.warn(&format!("could not read {}", self.tmp_disk));
            return false;
        };
        let mut ok = true;
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Some(n) = issue_of(&name, &self.base) else {
                continue;
            };
            if self.cycle_running(&n) || !self.cycle_lock(&n).exists() {
                continue;
            }
            let p = e.path();
            let shown = p.display().to_string();
            let size = bytes_of(&p);
            if self.dry {
                self.log(&format!(
                    "would remove {shown}: a cycle's scratch, {size} bytes"
                ));
                self.freed += size;
                continue;
            }
            if self.remove_under_lock(&n, &p) {
                self.freed += size;
                self.log(&format!("removed {shown}: a cycle's scratch, {size} bytes"));
            } else if p.exists() {
                self.warn(&format!("could not remove {shown}"));
                ok = false;
            }
        }
        ok
    }

    fn cycle_lock(&self, n: &str) -> PathBuf {
        self.home
            .join("state")
            .join("cycles")
            .join(format!("{n}.lock"))
    }

    /// A cycle holds the issue's lock, or the probe could not tell: the cases
    /// the cleaner reads as a cycle at work. Core answers for the probe.
    fn cycle_running(&self, n: &str) -> bool {
        matches!(
            tehas_core::lock::probe(&self.cycle_lock(n)),
            tehas_core::lock::Probe::Held | tehas_core::lock::Probe::Unknown(_)
        )
    }

    /// Only when the probe can tell the lock is free: no lock, or one it took.
    /// A link or a directory at the path, and a lock that cannot be opened, are
    /// not free: a lock is held by descriptor, so a name of its own proves
    /// nothing about the file the cycle locked.
    fn lock_free(&self, n: &str) -> bool {
        matches!(
            tehas_core::lock::probe(&self.cycle_lock(n)),
            tehas_core::lock::Probe::Absent | tehas_core::lock::Probe::Free
        )
    }

    /// The issue's lock, held for the one removal, as the sweep holds it. A
    /// lock that is not a plain file is never made by taking it.
    fn remove_under_lock(&self, n: &str, p: &Path) -> bool {
        if !self.lock_free(n) {
            return false;
        }
        let lock = self.cycle_lock(n);
        if !plain_file(&lock) {
            return false;
        }
        let Ok(f) = File::open(&lock) else {
            return false;
        };
        if f.try_lock().is_err() {
            return false;
        }
        remove(p)
    }

    /// The gitdir of a worktree, asked of git: Err 1 when git answers nothing
    /// this may date, 2 when git gives no answer, 3 for a stale record of a
    /// cycle's worktree. A cycle makes worktrees in its container, where the
    /// workspace is mounted at the shared checkout's path, so such a gitdir is
    /// read under the tree's own workspace.
    fn gitdir_of(&self, p: &Path) -> Result<PathBuf, u8> {
        let dotgit = p.join(".git");
        if !dotgit.is_file() {
            return Err(2);
        }
        let (Some(git), Some(timeout)) = (which("git"), which("timeout")) else {
            return Err(2);
        };
        if !(git.len() > 1 && git.starts_with('/') && timeout.len() > 1 && timeout.starts_with('/'))
        {
            return Err(2);
        }
        let works = &self.works;
        let asked = Command::new(&timeout)
            .args(["-k", "5", "30", &git, "-C"])
            .arg(p)
            .args([
                "-c",
                "core.fsmonitor=false",
                "rev-parse",
                "--absolute-git-dir",
            ])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", "/nonexistent")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_COUNT", "0")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map_err(|_| 2u8)?;
        let mut gitdir = String::from_utf8_lossy(&asked.stdout)
            .trim_end_matches('\n')
            .to_string();
        if !asked.status.success() {
            let text = fs::read(&dotgit).unwrap_or_default();
            let first = text.split(|c| *c == b'\n').next().unwrap_or(b"");
            gitdir = String::from_utf8_lossy(first.strip_prefix(b"gitdir: ").unwrap_or(b""))
                .into_owned();
            let records = format!("{works}/{}/.git/worktrees/", self.base);
            if !(gitdir.len() > records.len()
                && gitdir.starts_with(&records)
                && !Path::new(&gitdir).exists())
            {
                return Err(2);
            }
            let tree = p.to_str().ok_or(2u8)?;
            let rel = tree.strip_prefix(&format!("{works}/")).unwrap_or(tree);
            let own = rel.split('/').next().unwrap_or(rel);
            let under = &gitdir[format!("{works}/{}/", self.base).len()..];
            gitdir = fs::canonicalize(format!("{works}/{own}/{under}"))
                .map_err(|_| 3u8)?
                .to_str()
                .ok_or(3u8)?
                .to_string();
        }
        if !gitdir.starts_with('/') || !gitdir.starts_with(&format!("{works}/")) {
            return Err(1);
        }
        let gitdir = PathBuf::from(gitdir);
        if !gitdir.is_dir() {
            return Err(1);
        }
        Ok(gitdir)
    }

    /// The age rule of an issue: 0 when nothing of these paths, their
    /// gitdirs, or the issue's lock was written within the age; 1 when one
    /// was or could not be read; 2 for a worktree git could not name a gitdir
    /// of, the only proof a cycle is at work in it.
    fn untouched(&self, n: &str, paths: &[PathBuf]) -> u8 {
        let lock = self.cycle_lock(n);
        for p in paths.iter().chain([&lock]) {
            if !p.exists() {
                continue;
            }
            if newer(p, self.build_after) != Some(false) {
                return 1;
            }
            if p.join(".git").is_file() {
                match self.gitdir_of(p) {
                    Err(3) => continue,
                    Err(rc) => return rc,
                    Ok(g) => {
                        if newer(&g, self.build_after) != Some(false) {
                            return 1;
                        }
                    }
                }
            }
        }
        0
    }

    // --- 1. build output in kept workspaces
    fn workspaces(&mut self) -> bool {
        let works = PathBuf::from(&self.works);
        if !works.is_dir() || self.dirs.is_empty() {
            return true;
        }
        let mut names: Vec<String> = fs::read_dir(&works)
            .map(|d| {
                d.flatten()
                    .filter_map(|e| e.file_name().to_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        let mut seen = HashSet::new();
        let mut ok = true;
        for name in names {
            let Some(n) = issue_of(&name, &self.base) else {
                continue;
            };
            if !seen.insert(n.clone()) {
                continue;
            }
            let p = works.join(format!("{}-{n}", self.base));
            let state = PathBuf::from(format!("{}.state", p.display()));
            let retry = PathBuf::from(format!("{}.before-retry", p.display()));
            if self.cycle_running(&n) {
                self.log(&format!("kept the build output of #{n}: its lock is held"));
                continue;
            }
            let roots = roots_of(&p);
            let mut aged = vec![p.clone(), state.clone(), retry];
            aged.extend(roots.iter().cloned());
            match self.untouched(&n, &aged) {
                0 => {}
                rc => {
                    if !p.exists() {
                        continue;
                    }
                    let why = if rc == 2 {
                        "git could not name the gitdir of one of its worktrees".to_string()
                    } else {
                        format!("something of it is younger than {} hours", self.build_after)
                    };
                    self.log(&format!("kept the build output of #{n}: {why}"));
                    continue;
                }
            }
            // Asked of the file, not of a lock: taking a lock that is not
            // there makes one, and a removal under it would prove nothing.
            let lock_refusal = if !self.cycle_lock(&n).exists() {
                1
            } else if !self.lock_free(&n) {
                2
            } else {
                0
            };
            let (mut took, mut met, mut refused) = (false, false, false);
            let mut targets: Vec<(PathBuf, String)> = Vec::new();
            for root in &roots {
                for found in build_output(root, &self.dirs) {
                    targets.push((found, format!("build output of #{n}")));
                }
            }
            let cache = state.join("cache");
            if real_dir(&state) && real_dir(&cache) {
                targets.push((cache, format!("the code index copy of #{n}")));
            }
            for (found, what) in targets {
                if !real_dir(&found) {
                    continue;
                }
                met = true;
                let shown = found.display().to_string();
                let kb = bytes_of(&found);
                if self.dry {
                    let hours = self.build_after;
                    self.log(&format!(
                        "would remove {shown}: {what}, untouched for {hours} hours, {kb} bytes"
                    ));
                    // The figure of the dry run is the sum of what it names, so
                    // the same line moves the same counter a real run moves.
                    self.freed += kb;
                    continue;
                }
                if lock_refusal != 0 {
                    refused = true;
                    continue;
                }
                if self.remove_under_lock(&n, &found) {
                    took = true;
                    self.freed += kb;
                    self.log(&format!("removed {shown}: {what}, {kb} bytes"));
                } else if found.exists() {
                    refused = true;
                    self.warn(&format!("could not remove {shown}"));
                    ok = false;
                }
            }
            // One line for an issue from which nothing went. A lock that
            // exists and could not be taken is told by its warning.
            if took || self.dry {
                continue;
            }
            if met {
                if refused && lock_refusal == 1 {
                    self.log(&format!("kept the build output of #{n}: no lock to hold, so nothing of it was removed"));
                }
            } else if !refused {
                self.log(&format!("nothing to remove in #{n}"));
            }
        }
        ok
    }

    // --- 2. Docker's build cache, the whole host's, by its age alone.
    fn build_cache(&mut self) -> bool {
        if self.dry {
            let hours = self.build_after;
            self.log(&format!(
                "would prune docker's build cache of {hours} hours and older, whose size is not known beforehand"
            ));
            return true;
        }
        let until = format!("until={}h", self.build_after);
        let (rc, said) = merged(
            bounded("30", "600", "docker")
                .args(["builder", "prune", "-f", "--filter", &until])
                .stdin(Stdio::null()),
        );
        match rc {
            0 => {
                self.log(&format!(
                    "docker builder prune: {}",
                    first_line(&said, "docker gave no answer")
                ));
                true
            }
            124 => {
                self.warn("docker builder prune did not finish in 600 seconds");
                false
            }
            _ => {
                self.warn(&format!(
                    "docker builder prune failed ({rc}): {}",
                    first_line(&said, "docker gave no reason")
                ));
                false
            }
        }
    }

    fn remove_scratch(&mut self, f: &Path, would: &str, what: &str) -> bool {
        let shown = f.display().to_string();
        let kb = bytes_of(f);
        if self.dry {
            self.log(&format!("would remove {shown}: {would}, {kb} bytes"));
            self.freed += kb;
            return true;
        }
        if remove(f) {
            self.freed += kb;
            self.log(&format!("removed {shown}: {what}, {kb} bytes"));
            true
        } else {
            self.warn(&format!("could not remove {shown}"));
            false
        }
    }

    /// The plain files directly under <dir> not written within the age whose
    /// names pass <pick>. A <dir> that is a link is not entered.
    fn old_files(&self, dir: &Path, pick: impl Fn(&str) -> bool) -> Vec<PathBuf> {
        if !fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir()) {
            return Vec::new();
        }
        fs::read_dir(dir)
            .map(|d| {
                d.flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        p.file_name().and_then(|n| n.to_str()).is_some_and(&pick)
                            && plain_file(p)
                            && newer(p, self.after) == Some(false)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    // --- 3. host scratch: only these, and nothing elsewhere under the home.
    fn scratch(&mut self) -> bool {
        let tmp = self.home.join("tmp");
        let would = format!("host scratch untouched for {} hours", self.after);
        let mut ok = true;
        let mut files = self.old_files(&tmp.join("dl"), |_| true);
        files.extend(self.old_files(&tmp.join("node-compile-cache"), |_| true));
        if real_dir(&tmp) {
            files.extend(self.old_files(&tmp, |n| n.starts_with("analytics-facts.")));
            // The report a health run writes before its publish, and removes
            // when the publish answers. A run the poll's bound killed inside
            // the publish leaves it: the next clean takes it with the rest of
            // the scratch of that age.
            files.extend(self.old_files(&tmp, |n| n.starts_with("health.")));
        }
        // The copy of the recipe a sandbox build hands docker, and the temporary
        // it renames from, live beside the installed build script. A build takes
        // its own copy back when it ends and replaces a stale one when it starts,
        // so what stands here of an age belongs to a run a signal took off before
        // its trap ran: no other name of that directory is a build's to leave.
        // The shape is the one host/sandbox/build.sh stages with - mktemp's suffix
        // is six characters - and the same one .gitignore carries.
        files.extend(self.old_files(&self.home.join("sandbox"), staged_recipe_shaped));
        for f in files {
            ok &= self.remove_scratch(&f, &would, "host scratch");
        }
        let mut tests: Vec<PathBuf> = fs::read_dir(&tmp)
            .map(|d| {
                d.flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        p.file_name()
                            .and_then(|n| n.to_str())
                            .is_some_and(|n| n.starts_with("tehas-test."))
                            && real_dir(p)
                    })
                    .collect()
            })
            .unwrap_or_default();
        tests.sort();
        for d in tests {
            ok &= self.remove_scratch(
                &d,
                "a test's scratch directory of that age",
                "a test's scratch directory",
            );
        }
        ok
    }

    /// Whether docker dates an image before the age. What it cannot read
    /// keeps the image.
    fn image_old_enough(&self, id: &str) -> bool {
        let t = docker_first(true, &["image", "inspect", "--format", "{{.Created}}", id]);
        if !stamp_shaped(&t) {
            return false;
        }
        let Ok(o) = Command::new("date").args(["-d", &t, "+%s"]).output() else {
            return false;
        };
        let Ok(at) = String::from_utf8_lossy(&o.stdout).trim().parse::<i64>() else {
            return false;
        };
        o.status.success() && at < time::now() - self.after as i64 * 3600
    }

    fn image_size(id: &str) -> u64 {
        docker_first(false, &["image", "inspect", "--format", "{{.Size}}", id])
            .parse()
            .unwrap_or(0)
    }

    /// `docker rmi` of one name: Ok, or docker's reason.
    fn rmi(name: &str) -> Result<(), String> {
        let (rc, said) = merged(
            bounded("30", "600", "docker")
                .args(["rmi", name])
                .stdin(Stdio::null()),
        );
        if rc == 0 {
            Ok(())
        } else {
            Err(first_line(&said, "docker gave no reason"))
        }
    }

    // --- 4. old sandbox images: the dated tags past the age, but the newest.
    fn sandbox_images(&mut self) -> bool {
        let s = self.sandbox.clone();
        let Some(tags) = docker_list(&[
            "image",
            "ls",
            "--no-trunc",
            "--format",
            "{{.Tag}} {{.ID}}",
            &s,
        ]) else {
            self.warn(&format!("docker gave no list of its {s} tags"));
            return false;
        };
        if tags.is_empty() {
            return true;
        }
        let Some(held) = images_held(&tags) else {
            self.warn("docker said nothing about the containers on the host");
            return false;
        };
        let mut lines: Vec<&str> = tags.split('\n').filter(|l| dated(l)).collect();
        lines.sort_by(|a, b| b.as_bytes().cmp(a.as_bytes()));
        let (mut ok, mut undated) = (true, 0);
        for (i, line) in lines.iter().enumerate() {
            let (tag, id) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
            let id = id.trim();
            if i == 0 {
                self.log(&format!("kept {s}:{tag}: the newest dated tag"));
                continue;
            }
            if !self.image_old_enough(id) {
                undated += 1;
                let after = self.after;
                self.log(&format!(
                    "kept {s}:{tag}: docker did not date its image past {after} hours"
                ));
                continue;
            }
            if held.iter().any(|h| h == id) {
                self.log(&format!("kept {s}:{tag}: a container or a uid tag uses it"));
                continue;
            }
            if self.dry {
                // Docker's own figure, and one a dry run does not ask for: it
                // would be the size of the image, not of what the tag's removal
                // frees, since a layer a kept tag shares stays.
                self.log(&format!(
                    "would remove {s}:{tag}: a sandbox image of that age, whose size is not known beforehand"
                ));
                continue;
            }
            let kb = Self::image_size(id);
            match Self::rmi(&format!("{s}:{tag}")) {
                Ok(()) => {
                    self.freed += kb;
                    self.log(&format!(
                        "removed {s}:{tag}: a sandbox image of that age, {kb} bytes"
                    ));
                }
                Err(why) => {
                    self.warn(&format!("could not remove {s}:{tag}: {why}"));
                    ok = false;
                }
            }
        }
        if undated > 0 {
            self.warn(&format!(
                "docker gave no date past {} hours for {undated} image(s), none of them removed",
                self.after
            ));
        }
        ok
    }

    // --- 5. the old base images: `sha-` tags of the base's repository alone.
    fn base_images(&mut self) -> bool {
        if self.pulled.is_empty() {
            self.warn(&format!("kept the base: the host's pins name no base to prune (PI_RUKAS_REPO, PI_RUKAS_FOLLOW or PI_RUKAS_IMAGE_DIGEST in {}/versions.env or the instance's instance.env, or lib/rukas.sh is not sourced beside lib/sandbox.sh)", self.home.display()));
            return true;
        }
        let repo = repo_of(&self.pulled).to_string();
        let s = self.sandbox.clone();
        let format = "{{.Repository}} {{.Tag}} {{.ID}}";
        let Some(listed) = docker_list(&["image", "ls", "--no-trunc", "--format", format, &repo])
        else {
            self.warn(&format!("docker gave no list of its base tags of {repo}"));
            return false;
        };
        let Some(sandbox_listed) =
            docker_list(&["image", "ls", "--no-trunc", "--format", format, &s])
        else {
            self.warn(&format!("docker gave no list of the tags of {s}"));
            return false;
        };
        // Each line says its repository: only the base's may be taken, and
        // the sandbox image's are the proof of what is kept.
        let (mut bases, mut kept_tags) = (Vec::new(), Vec::new());
        for line in listed.split('\n').chain(sandbox_listed.split('\n')) {
            let f: Vec<&str> = line.split_whitespace().collect();
            let field = |i: usize| f.get(i).copied().unwrap_or("").to_string();
            if field(0) == repo {
                bases.push((field(1), field(2)));
            }
            if field(0) == s {
                kept_tags.push((field(1), field(2)));
            }
        }
        if bases.is_empty() {
            self.log(&format!("kept the base: no image of {repo} is on the host"));
            return true;
        }
        let held_lines: String = kept_tags
            .iter()
            .map(|(t, i)| format!("{t} {i}\n"))
            .collect();
        let Some(keep_ids) = images_held(&held_lines) else {
            self.warn("docker said nothing about the containers on the host");
            return false;
        };
        // The releases the host keeps are the `release-` tags it answered for.
        let mut kept = Vec::new();
        for (tag, rid) in &kept_tags {
            if !tag.starts_with("release-") || rid.is_empty() {
                continue;
            }
            if !is_id(rid) {
                self.warn(&format!(
                    "docker named {rid} as a release tag of {s}, which is not an id"
                ));
                return false;
            }
            kept.push(rid.clone());
        }
        kept.extend(keep_ids.iter().cloned());
        let mut cands = Vec::new();
        let mut ok = true;
        for (ltag, lid) in &bases {
            if !is_id(lid) {
                self.warn(&format!(
                    "docker named {lid} as an image of {repo}, which is not an id"
                ));
                ok = false;
                continue;
            }
            let sha = ltag.strip_prefix("sha-").is_some_and(|h| {
                h.len() == 12 && h.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
            });
            if !sha {
                self.log(&format!(
                    "kept {repo}:{ltag}: not a base tag of sha- and twelve characters"
                ));
                continue;
            }
            if keep_ids.contains(lid) {
                self.log(&format!("kept {repo}:{ltag}: a container was made from it"));
                continue;
            }
            if kept.contains(lid) {
                self.log(&format!(
                    "kept {repo}:{ltag}: a kept sandbox tag was built on it"
                ));
                continue;
            }
            if !self.image_old_enough(lid) {
                let after = self.after;
                self.log(&format!(
                    "kept {repo}:{ltag}: docker did not date its image past {after} hours"
                ));
                continue;
            }
            cands.push((format!("{repo}:{ltag}"), Self::image_size(lid)));
        }
        if !ok {
            return false;
        }
        for (name, kb) in cands {
            if self.dry {
                self.log(&format!(
                    "would remove {name}: a base image of that age, whose size is not known beforehand"
                ));
                continue;
            }
            match Self::rmi(&name) {
                Ok(()) => {
                    self.freed += kb;
                    self.log(&format!(
                        "removed {name}: a base image of that age, {kb} bytes"
                    ));
                }
                Err(why) => {
                    self.warn(&format!("could not remove {name}: {why}"));
                    ok = false;
                }
            }
        }
        ok
    }

    // --- 6. old logs and the state store's upkeep.
    fn upkeep(&mut self) -> bool {
        let bin = self.home.join("libexec").join("store");
        let db = self.home.join("state").join("tehas.db");
        let runnable =
            fs::metadata(&bin).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
        if !runnable || !plain_file(&db) {
            self.warn("the state store is not installed: no logs removed, no backup");
            return false;
        }
        let args: &[&str] = if self.dry {
            &["upkeep", "--dry-run"]
        } else {
            &["upkeep"]
        };
        let v = match store::call(&self.home, args) {
            Ok(v) => v,
            Err(no) => {
                self.warn(&format!("store upkeep failed ({}): {}", no.word, no.line));
                return false;
            }
        };
        let act = if self.dry { "would remove" } else { "removed" };
        for n in v["logs"].as_array().into_iter().flatten() {
            let n = n.as_str().map_or_else(|| n.to_string(), String::from);
            // The store answers names, not sizes: a dry run's figure leaves them out.
            let size = if self.dry {
                ", whose size is not known beforehand"
            } else {
                ""
            };
            self.log(&format!("{act} logs/{n}: a log 7 days old{size}"));
        }
        if !self.dry {
            let kept = json!({"backup": v["backup"], "prune": v["prune"]});
            self.log(&format!("store upkeep: {kept}"));
        }
        true
    }

    /// The lock of one clone's manual session, beside the tree of clones and
    /// nowhere among the locks of the cycles: `state/dev` holds what `tehas dev`
    /// takes, `state/cycles` what a cycle takes, so no reader of a cycle's lock
    /// ever asks about one of these. The path is the contract with
    /// `libexec/dev`, which holds it for a session's life, so a test of the rule
    /// is a test of the path. A name of no plain word has no lock of its own and
    /// answers nothing.
    fn clone_lock(&self, name: &str) -> Option<PathBuf> {
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return None;
        }
        Some(
            self.home
                .join("state")
                .join("dev")
                .join(format!("{name}.lock")),
        )
    }

    /// The clone's lock, held while the file lives. A lock not there yet is made:
    /// a session that starts during the removal must find it taken.
    fn hold_clone(&self, name: &str) -> Option<File> {
        let lock = self.clone_lock(name)?;
        fs::create_dir_all(lock.parent()?).ok()?;
        take_clone_lock(&lock).ok()
    }

    /// The names of the host's containers, as docker lists them: one call for
    /// the whole step, as the image steps list the host's images once. The one
    /// form `--format` is asked with, so no step of the cleaner asks docker for
    /// a container in any other way. None when docker would not answer, which
    /// is no proof of no session: a clone whose session cannot be told keeps
    /// its build, as an image no container has been named for keeps itself.
    fn container_names(&self) -> Option<Vec<String>> {
        docker_list(&["ps", "--all", "--format", "{{.Names}}"])
            .map(|listed| listed.split('\n').map(String::from).collect())
    }

    /// The rule of a session: Some(false) only when the clone's lock answers no
    /// session and docker answers no container of the session's name. A lock
    /// that is a link, a directory or a file no one can open, and a docker that
    /// says nothing, all answer the same as a session at work: keep.
    fn session_open(&self, name: &str, containers: Option<&Vec<String>>) -> Option<bool> {
        if let Some(lock) = self.clone_lock(name) {
            match tehas_core::lock::probe(&lock) {
                tehas_core::lock::Probe::Absent | tehas_core::lock::Probe::Free => {}
                _ => return Some(true),
            }
        }
        // A session's container carries the clone's name, which docker knows
        // with or without the lock, so the listing answers a name of no plain
        // word as well as one of a lock that has gone with its session.
        let cname = format!("tehas-dev-{name}");
        containers.map(|listed| listed.contains(&cname))
    }

    // --- 7. the build output of a manual session: <name>.state/target, and
    // nothing else under <name>.state.
    fn clone_targets(&mut self) -> bool {
        // No directory of clones: no person has run `tehas dev` here.
        if !self.home.join("dev").is_dir() {
            return true;
        }
        // Listed once for the whole step: one docker that will not answer keeps
        // every clone's build, as it keeps an image no container has been
        // named for.
        let containers = self.container_names();
        self.clone_targets_among(containers.as_ref())
    }

    fn clone_targets_among(&mut self, containers: Option<&Vec<String>>) -> bool {
        let devs = self.home.join("dev");
        let Ok(entries) = fs::read_dir(&devs) else {
            return true;
        };
        let mut names: Vec<String> = entries
            .flatten()
            .filter_map(|e| e.file_name().to_str().map(String::from))
            .filter(|n| !n.starts_with('.'))
            .collect();
        names.sort();
        let me = the_uid();
        let mut ok = true;
        for name in names {
            let Some(shown_name) = name.strip_suffix(".state") else {
                // Only the tree of a clone's own state is asked about: a bare
                // clone, a `.author` or an `.images` file is no build output.
                continue;
            };
            // Only this run's own user's own plain directory: a target left by
            // hand, by root or by another account is not what this command made
            // and is not taken, and no link is walked, of the state or the target.
            let state = devs.join(&name);
            let target = state.join("target");
            let owned = fs::symlink_metadata(&target).is_ok_and(|m| m.is_dir() && m.uid() == me);
            if !real_dir(&state) || !real_dir(&target) || !owned {
                continue;
            }
            let shown = target.display().to_string();
            let bytes = bytes_of(&target);
            let session = match self.session_open(shown_name, containers) {
                Some(open) => open,
                None => {
                    self.warn(&format!(
                        "docker gave no answer for the session of clone {shown_name}, so its build output was kept, {bytes} bytes"
                    ));
                    continue;
                }
            };
            if session {
                self.log(&format!(
                    "kept the build output of clone {shown_name}: its session is open, {bytes} bytes"
                ));
                continue;
            }
            // Low on disk only, so a healthy host never loses a build, and
            // even then only a build of that age. <after>, not <build_after>:
            // the low disk that zeroes the second is the first rule here.
            if !self.low {
                self.log(&format!(
                    "kept the build output of clone {shown_name}: the disk is not low, {bytes} bytes"
                ));
                continue;
            }
            if newer(&target, self.after) != Some(false) {
                let hours = self.after;
                self.log(&format!(
                    "kept the build output of clone {shown_name}: its target was written within {hours} hours, {bytes} bytes"
                ));
                continue;
            }
            if self.dry {
                self.log(&format!(
                    "would remove {shown}: the build output of clone {shown_name}, {bytes} bytes"
                ));
                self.freed += bytes;
                continue;
            }
            // Held through the removal: a session that starts now waits behind
            // it, and one that started since the question above keeps its build.
            let Some(_held) = self.hold_clone(shown_name) else {
                self.log(&format!(
                    "kept the build output of clone {shown_name}: its session is open, {bytes} bytes"
                ));
                continue;
            };
            if remove(&target) {
                self.freed += bytes;
                self.log(&format!(
                    "removed {shown}: the build output of clone {shown_name}, {bytes} bytes"
                ));
            } else {
                self.warn(&format!("could not remove {shown}"));
                ok = false;
            }
        }
        ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cycles_end_takes_the_build_output_of_every_tree_and_nothing_else() {
        let root = std::env::temp_dir().join(format!("clean-ended-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let tree = root.join("widget-7");
        let retry = root.join("widget-7.before-retry");
        let worktree = tree.join(".runner/issue-7-default");
        for d in [
            tree.join("console/target/debug"),
            tree.join("web/node_modules/x"),
            tree.join("src"),
            retry.join("console/target"),
            worktree.join("console/target/debug"),
            worktree.join("src"),
        ] {
            fs::create_dir_all(d).unwrap();
        }
        fs::write(worktree.join(".git"), "gitdir: /elsewhere\n").unwrap();
        fs::write(tree.join("src/kept.rs"), "x").unwrap();
        let log = root.join("clean.log");
        let args = [
            tree.display().to_string(),
            "target node_modules".to_string(),
            log.display().to_string(),
            root.join("scratch/widget-7").display().to_string(),
        ];
        fs::create_dir_all(root.join("scratch/widget-7/mt")).unwrap();
        assert_eq!(ended(&args), 0);
        assert!(!root.join("scratch/widget-7").exists());
        for gone in [
            tree.join("console/target"),
            tree.join("web/node_modules"),
            retry.join("console/target"),
            worktree.join("console/target"),
        ] {
            assert!(!gone.exists(), "{}", gone.display());
        }
        assert!(tree.join("src/kept.rs").exists());
        assert!(worktree.join("src").exists());
        assert!(fs::read_to_string(&log)
            .unwrap()
            .contains("build output of a cycle that ended"));
        assert_eq!(ended(&args), 0);
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(ended(&args[..2]), 64);
    }

    /// The times of every file and directory under <root>, root included, set
    /// to some hours ago: the age rule reads the tree itself.
    fn age_tree(root: &Path, hours: u64) {
        let at = std::time::SystemTime::now() - std::time::Duration::from_secs(hours * 3600);
        let times = fs::FileTimes::new().set_accessed(at).set_modified(at);
        let mut stack = vec![root.to_path_buf()];
        while let Some(d) = stack.pop() {
            if let Ok(entries) = fs::read_dir(&d) {
                stack.extend(entries.flatten().map(|e| e.path()));
            }
            fs::File::open(&d).unwrap().set_times(times).unwrap();
        }
    }

    /// A workspace of one issue whose build output is a tree of a size this
    /// test knows, with the steps beside it that need docker or the store
    /// absent: the empty names make those steps report and change nothing, as
    /// the whole of a clean run does. The bytes of a directory are what
    /// `bytes_of` counts, so the figure asserted is the sum of the tree named
    /// here.
    fn one_issue_tree(root: &Path) -> (Run, u64) {
        let _ = fs::remove_dir_all(root);
        let works = root.join("work");
        let home = root.join("home");
        let d = works.join("widget-7");
        fs::create_dir_all(home.join("state/cycles")).unwrap();
        fs::create_dir_all(d.join("target")).unwrap();
        fs::create_dir_all(d.join("src")).unwrap();
        fs::write(d.join("src").join("main.rs"), "keep").unwrap();
        fs::write(d.join("target").join("console"), vec![b'x'; 4096]).unwrap();
        fs::write(d.join("target").join("build"), vec![b'y'; 3072]).unwrap();
        // The cycle's lock: a removal is tried only under one that is there.
        fs::write(home.join("state/cycles/7.lock"), "").unwrap();
        // The age rule reads the workspace, its lock and each tree of the
        // issue: all of them are three days old, as the shell fixture dates
        // what it writes.
        age_tree(&works, 72);
        age_tree(&home.join("state/cycles"), 72);
        // No scratch disk; the image names are empty, so neither image step is
        // asked of docker, and no store stands in this home, so step 6 fails
        // and reports itself. A dry run asks docker nothing; the real run below
        // calls only the workspaces' step, so no case prunes the host's docker.
        let run = Run {
            dry: true,
            home,
            works: works.display().to_string(),
            base: "widget".to_string(),
            after: 24,
            build_after: 24,
            low: false,
            dirs: vec!["target".to_string()],
            sandbox: String::new(),
            pulled: String::new(),
            logfile: root.join("clean.log"),
            tmp_disk: String::new(),
            freed: 0,
            rc: 0,
        };
        (run, bytes_of(&d.join("target")))
    }

    #[test]
    fn a_dry_run_names_the_bytes_it_would_free_and_removes_nothing() {
        let root = std::env::temp_dir().join(format!("clean-dry-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (mut run, known) = one_issue_tree(&root);
        let tree = root.join("work/widget-7");
        assert!(known > 0);
        // The closing line is the run's, so the dry figure is read after one.
        assert_eq!(
            run.main(30 * 1024, "30g", ""),
            1,
            "only the store's step failed"
        );
        let said = fs::read_to_string(&run.logfile).unwrap();
        // The bytes on the path it names, and in the closing word of the run.
        assert!(
            said.contains(&format!(
                "would remove {}/target: build output of #7, untouched for 24 hours, {known} bytes",
                tree.display()
            )),
            "{said}"
        );
        assert!(
            said.contains(&format!("would free {known} bytes")),
            "{said}"
        );
        assert!(!said.contains("freed 0 bytes"), "{said}");
        // The figure is not the whole of the run: docker's steps and the
        // store's upkeep are not known beforehand, and the line says so.
        assert!(
            said.contains(
                "which leaves out the build cache, the image tags and the store's upkeep"
            ),
            "{said}"
        );
        assert!(tree.join("target/console").exists(), "a dry run removed");
        // The same tree under a real run frees exactly that figure.
        one_issue_tree(&root);
        run.dry = false;
        run.freed = 0;
        assert!(run.workspaces());
        assert_eq!(
            run.freed,
            known,
            "{}",
            fs::read_to_string(&run.logfile).unwrap()
        );
        assert!(!tree.join("target").exists(), "a real run left it");
        assert!(tree.join("src/main.rs").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_cycles_scratch_is_made_empty_and_for_any_writer() {
        let root = std::env::temp_dir().join(format!("clean-scratch-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let dir = root.display().to_string();
        let args = [dir.clone(), "widget-7".to_string()];
        assert_eq!(scratch_made(&args), 78, "the disk is not there");
        fs::create_dir_all(root.join("widget-7/old")).unwrap();
        assert_eq!(scratch_made(&args), 0);
        let made = root.join("widget-7");
        assert!(!made.join("old").exists());
        let mode = fs::metadata(&made).unwrap().permissions().mode();
        assert_eq!(mode & 0o7777, 0o1777);
        assert_eq!(scratch_made(&["scratch".to_string(), "w".to_string()]), 78);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_clone_keeps_its_build_output_till_the_disk_or_the_age_says_otherwise() {
        let root = std::env::temp_dir().join(format!("clean-clone-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let home = root.join("home");
        let devs = home.join("dev");
        let clone = |name: &str, age: bool| {
            let target = devs.join(format!("{name}.state/target"));
            fs::create_dir_all(&target).unwrap();
            fs::write(target.join("console"), vec![b'x'; 2048]).unwrap();
            fs::create_dir_all(devs.join(format!("{name}.state/sessions"))).unwrap();
            if age {
                age_tree(&target, 72);
            }
            bytes_of(&target)
        };
        clone("alice", true);
        clone("bob", false);
        // What the run frees is read back of the tree before it goes: the
        // figure is bytes_of's own, not a count this case keeps by hand.
        let alice_bytes = bytes_of(&devs.join("alice.state/target"));
        let run = Run {
            dry: false,
            home: home.clone(),
            works: String::new(),
            base: "widget".to_string(),
            after: 24,
            build_after: 24,
            low: false,
            dirs: Vec::new(),
            sandbox: String::new(),
            pulled: String::new(),
            logfile: root.join("clean.log"),
            tmp_disk: String::new(),
            freed: 0,
            rc: 0,
        };
        let mut run = run;
        // No container of the host carries a session's name, so no case here
        // asks a docker.
        let none: Vec<String> = Vec::new();
        // Above the minimum nothing goes, of any age, and the size is logged.
        assert!(run.clone_targets_among(Some(&none)));
        let said = fs::read_to_string(&run.logfile).unwrap();
        assert!(run.home.join("dev/alice.state/target").exists(), "{said}");
        assert!(
            said.contains(&format!(
                "kept the build output of clone alice: the disk is not low, {alice_bytes} bytes"
            )),
            "{said}"
        );
        assert_eq!(run.freed, 0, "{said}");
        // Under it, of an age, so it goes; young, so it stays; and the lines
        // name why. The bytes are what went, read back of the tree itself.
        run.low = true;
        run.build_after = 0;
        fs::remove_file(&run.logfile).unwrap();
        assert!(
            run.clone_targets_among(Some(&none)),
            "{}",
            fs::read_to_string(&run.logfile).unwrap()
        );
        let said = fs::read_to_string(&run.logfile).unwrap();
        assert!(!run.home.join("dev/alice.state/target").exists(), "{said}");
        assert!(run.home.join("dev/alice.state/sessions").exists(), "{said}");
        assert!(run.home.join("dev/bob.state/target").exists(), "{said}");
        assert!(run.home.join("dev/bob.state/sessions").exists());
        assert!(said.contains("removed "), "{said}");
        let young = bytes_of(&devs.join("bob.state/target"));
        assert!(
            said.contains(&format!(
                "kept the build output of clone bob: its target was written within 24 hours, {young} bytes"
            )),
            "{said}"
        );
        assert_eq!(run.freed, alice_bytes, "{said}");
        // A target a hand or an earlier root mount left, and not of this run's
        // account, is nobody's build output to take: a link of that name is
        // left a link and what it points at is not walked.
        let aside = root.join("elsewhere");
        fs::create_dir_all(aside.join("target")).unwrap();
        fs::write(aside.join("target/lib"), vec![b'x'; 4096]).unwrap();
        age_tree(&aside, 72);
        std::os::unix::fs::symlink(&aside, devs.join("dave.state")).unwrap();
        let mut dry = Run {
            dry: true,
            freed: 0,
            ..run
        };
        clone("carol", true);
        fs::remove_file(&dry.logfile).unwrap();
        assert!(dry.clone_targets_among(Some(&none)));
        assert!(dry.home.join("dev/carol.state/target").exists());
        assert!(aside.join("target/lib").exists(), "a link was followed");
        let said = fs::read_to_string(&dry.logfile).unwrap();
        assert!(said.contains("would remove "), "{said}");
        assert!(!said.contains("dave"), "{said}: a link is no build output");
        // A dry run answers as the run would: the young build is not named.
        assert_eq!(
            dry.freed,
            bytes_of(&devs.join("carol.state/target")),
            "{said}"
        );
        // A docker that will not answer keeps every build.
        fs::remove_file(&dry.logfile).unwrap();
        dry.freed = 0;
        assert!(dry.clone_targets_among(None));
        assert_eq!(dry.freed, 0);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn an_open_session_holds_a_clones_build_output() {
        let root = std::env::temp_dir().join(format!("clean-clone-held-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let home = root.join("home");
        for name in ["alice", "bob"] {
            let target = home.join("dev").join(format!("{name}.state/target"));
            fs::create_dir_all(&target).unwrap();
            fs::write(target.join("console"), vec![b'x'; 1024]).unwrap();
            age_tree(&target, 72);
        }
        // The rule of one clone, told by its lock alone: the container question
        // is docker's, and a docker that answers nothing is an open session.
        let run = Run {
            dry: false,
            home,
            works: String::new(),
            base: "widget".to_string(),
            after: 24,
            build_after: 24,
            low: false,
            dirs: Vec::new(),
            sandbox: String::new(),
            pulled: String::new(),
            logfile: root.join("clean.log"),
            tmp_disk: String::new(),
            freed: 0,
            rc: 0,
        };
        fs::create_dir_all(run.clone_lock("alice").unwrap().parent().unwrap()).unwrap();
        // No container of the host carries a session's name, and no lock stands:
        // the ordinary shape of a host whose sessions have all ended.
        let none: Vec<String> = Vec::new();
        assert_eq!(run.session_open("alice", Some(&none)), Some(false));
        fs::write(run.clone_lock("alice").unwrap(), "").unwrap();
        assert_eq!(run.session_open("alice", Some(&none)), Some(false));
        // The container of a session carries the clone's name, and answers with
        // or without the lock: it is the second of the two proofs, the one a
        // session whose lock went with its killed holder still gives.
        let session = vec!["tehas-work-7".to_string(), "tehas-dev-alice".to_string()];
        assert_eq!(run.session_open("alice", Some(&session)), Some(true));
        assert_eq!(run.session_open("carol", Some(&session)), Some(false));
        // A docker that answers nothing is not a host with no session.
        assert_eq!(run.session_open("alice", None), None);
        // A lock another process holds is a session: its target stays whatever
        // its age, and the line says why. The cleaner's step asks the lock
        // again at the instant of the removal, so this is the one rule that must
        // not read a held lock as free.
        let held = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(run.clone_lock("bob").unwrap())
            .unwrap();
        held.try_lock().unwrap();
        assert_eq!(run.session_open("bob", Some(&none)), Some(true));
        // A link, a directory and a lock no one can open are the same answer:
        // a lock is held by descriptor, so a name of its own proves nothing.
        let dir = run.clone_lock("carol").unwrap();
        fs::create_dir(&dir).unwrap();
        assert_eq!(run.session_open("carol", Some(&none)), Some(true));
        std::os::unix::fs::symlink(root.join("nope"), run.clone_lock("dave").unwrap()).unwrap();
        assert_eq!(run.session_open("dave", Some(&none)), Some(true));
        drop(held);
        // The removal's own hold: it makes a lock not there yet, and a session
        // finds it taken for as long as the hold lives.
        let eve = run.clone_lock("eve").unwrap();
        let hold = run.hold_clone("eve").unwrap();
        assert_eq!(tehas_core::lock::probe(&eve), tehas_core::lock::Probe::Held);
        assert!(run.hold_clone("eve").is_none());
        // Unlocked by name: a child another case forks this instant shares the
        // descriptor until it execs, so a drop alone may not free it yet.
        hold.unlock().unwrap();
        assert_eq!(tehas_core::lock::probe(&eve), tehas_core::lock::Probe::Free);
        // A link is not taken, and a name of no plain word has no lock to hold.
        assert!(run.hold_clone("dave").is_none());
        assert!(run.hold_clone("a.b").is_none());
        fs::remove_dir_all(&root).unwrap();
    }

    enum LockShape {
        None,
        File,
        Directory,
        Link,
        Dangling,
    }

    /// What the two lock rules of the run answer for one issue whose lock is
    /// <lock>: whether a cycle is taken as running, and whether the lock counts
    /// as free. The second is what a removal is tried under, so it is the rule
    /// that must not read a link, a directory or an unopenable lock as free.
    fn lock_rules(root: &Path, lock: LockShape) -> (bool, bool) {
        let _ = fs::remove_dir_all(root);
        let home = root.join("home");
        fs::create_dir_all(home.join("state/cycles")).unwrap();
        let path = home.join("state/cycles/7.lock");
        match lock {
            LockShape::None => {}
            LockShape::File => fs::write(&path, "").unwrap(),
            LockShape::Directory => fs::create_dir(&path).unwrap(),
            LockShape::Link => {
                let target = root.join("elsewhere.lock");
                fs::write(&target, "").unwrap();
                std::os::unix::fs::symlink(&target, &path).unwrap();
            }
            LockShape::Dangling => {
                std::os::unix::fs::symlink(root.join("nothing"), &path).unwrap();
            }
        }
        let run = Run {
            dry: false,
            home,
            works: String::new(),
            base: "tehas".to_string(),
            after: 24,
            build_after: 24,
            low: false,
            dirs: Vec::new(),
            sandbox: String::new(),
            pulled: String::new(),
            logfile: root.join("clean.log"),
            tmp_disk: String::new(),
            freed: 0,
            rc: 0,
        };
        (run.cycle_running("7"), run.lock_free("7"))
    }

    #[test]
    fn a_lock_that_is_a_link_is_not_free() {
        let root = std::env::temp_dir().join(format!("clean-link-lock-{}", std::process::id()));
        // A lock is held by descriptor, so a link, a directory, or a lock no one
        // can open proves nothing about the file a cycle locked. Neither rule
        // may call such a lock free: the first keeps the issue's build output,
        // the second is the one a removal is tried under.
        for shape in [LockShape::Directory, LockShape::Link, LockShape::Dangling] {
            let (running, free) = lock_rules(&root, shape);
            assert!(running, "a lock that is no lock file read as no cycle");
            assert!(!free, "a lock that is no lock file read as free");
        }
        // A lock the probe took is free, and so is no lock at all: the second
        // is free of any lock. The run asks whether the lock is there before it
        // removes anything, since taking one that is not there would make it.
        let (running, free) = lock_rules(&root, LockShape::File);
        assert!(!running);
        assert!(free);
        let (running, free) = lock_rules(&root, LockShape::None);
        assert!(!running);
        assert!(free);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn sizes_read_with_one_decimal() {
        assert_eq!(gib(1_610_612_736), "1.5 GiB");
        assert_eq!(gib(0), "0.0 GiB");
        assert_eq!(gib(31 * 1_073_741_824 - 1), "30.9 GiB");
    }

    #[test]
    fn the_first_line_or_the_default() {
        assert_eq!(first_line("", "none"), "none");
        assert_eq!(first_line("\n\n", "none"), "none");
        assert_eq!(first_line("a\nb\n", "none"), "a");
        assert_eq!(first_line("\nb", "none"), "");
    }

    #[test]
    fn docker_stamps_of_the_shapes_it_answers() {
        for t in [
            "2026-10-24T10:00:00Z",
            "2026-10-24T10:00:00.123456789Z",
            "2026-10-24T10:00:00+03:00",
            "2026-10-24T10:00:00-0300",
            "2026-10-24T10:00:00",
        ] {
            assert!(stamp_shaped(t), "{t}");
        }
        for t in [
            "",
            "yesterday",
            "2026-10-24 10:00:00Z",
            "2026-10-24T10:00:00.Z",
            "2026-10-24T10:00:00+3:00",
            "2026-10-24T10:00:00Zjunk",
            "2026-10-24T10:00:00+03:0:0",
        ] {
            assert!(!stamp_shaped(t), "{t}");
        }
    }

    #[test]
    fn an_image_loses_its_tag_and_keeps_a_registry_and_its_port() {
        for (image, repo) in [
            ("tehas/pi-rukas:uid1000", "tehas/pi-rukas"),
            ("tehas/pi-rukas", "tehas/pi-rukas"),
            ("pi-rukas:uid1000", "pi-rukas"),
            ("pi-rukas", "pi-rukas"),
            (
                "registry:5000/tehas/pi-rukas:uid1000",
                "registry:5000/tehas/pi-rukas",
            ),
            ("registry:5000/pi-rukas", "registry:5000/pi-rukas"),
        ] {
            assert_eq!(image_repo(image).as_deref(), Some(repo), "{image}");
        }
    }

    #[test]
    fn a_name_docker_cannot_list_is_refused() {
        for image in [
            "",
            "tehas/",
            "a:",
            ":tag",
            "a:b:c",
            "a b",
            "tehas/-x",
            "tehas/x:-y",
            "tehas/x@sha256:1",
        ] {
            assert_eq!(image_repo(image), None, "{image}");
        }
    }

    #[test]
    fn dated_tags_and_nothing_else() {
        assert!(dated("261009-1000 sha256:1"));
        assert!(dated("261009-1000-product sha256:1"));
        assert!(!dated("uid1000 sha256:1"));
        assert!(!dated("261009-1000-other sha256:1"));
        assert!(!dated("261009-1000"));
        assert!(!dated("release-v0.14.3 sha256:1"));
    }

    #[test]
    fn issue_names_under_the_work_dir() {
        assert_eq!(issue_of("widget-7", "widget").as_deref(), Some("7"));
        assert_eq!(
            issue_of("widget-7.before-retry", "widget").as_deref(),
            Some("7")
        );
        assert_eq!(issue_of("widget-07", "widget"), None);
        assert_eq!(issue_of("widget-7.state", "widget"), None);
        assert_eq!(issue_of("widget-", "widget"), None);
        assert_eq!(issue_of("widget-12345678901", "widget"), None);
        assert_eq!(issue_of("other-7", "widget"), None);
    }

    #[test]
    fn the_two_names_of_a_sandbox_build_and_nothing_else() {
        // The names host/sandbox/build.sh stages, and the shape of its temporary:
        // six characters of mktemp's alphabet, as the build's sweep and .gitignore
        // both read it. A longer suffix, a shorter one, a name that only looks
        // like one, and a suffix of six bytes that is no mktemp name all stay.
        for n in [
            "sandbox-uid.Dockerfile",
            "sandbox-uid.Dockerfile.abcdef",
            "sandbox-uid.Dockerfile.A1b2C3",
        ] {
            assert!(staged_recipe_shaped(n), "{n}");
        }
        for n in [
            "sandbox-uid.Dockerfile.toolong",
            "sandbox-uid.Dockerfile.abcde",
            "sandbox-uid.Dockerfile.DIR",
            "sandbox-uid.Dockerfilex",
            "Dockerfile",
            "build.sh",
            "sandbox-uid.Dockerfile.",
            "sandbox-uid.Dockerfile.a-b_c",
            "sandbox-uid.Dockerfile.\u{e9}\u{e9}\u{e9}",
        ] {
            assert!(!staged_recipe_shaped(n), "{n}");
        }
    }

    #[test]
    fn the_repository_of_an_image_name() {
        assert_eq!(repo_of("ghcr.io/o/r:sha-aaaaaaaaaaaa"), "ghcr.io/o/r");
        assert_eq!(repo_of("ghcr.io/o/r@sha256:abc"), "ghcr.io/o/r");
        assert_eq!(repo_of("ghcr.io/o/r:tag@sha256:abc"), "ghcr.io/o/r");
    }

    #[test]
    fn ids_docker_answers() {
        assert!(is_id(&format!("sha256:{}", "a".repeat(64))));
        assert!(is_id(&"0".repeat(64)));
        assert!(!is_id("sha256:1"));
        assert!(!is_id(&"A".repeat(64)));
    }

    #[test]
    fn the_walk_skips_dot_names_and_links_and_takes_named_dirs() {
        let t = std::env::temp_dir().join(format!("clean-walk-{}", std::process::id()));
        let _ = fs::remove_dir_all(&t);
        for d in [
            "target/sub/target",
            ".git/target",
            "src",
            "node_modules",
            "a/.next",
        ] {
            fs::create_dir_all(t.join(d)).unwrap();
        }
        std::os::unix::fs::symlink(t.join("src"), t.join("link-target")).unwrap();
        std::os::unix::fs::symlink(t.join("src"), t.join("a/target")).unwrap();
        let names = vec![
            "target".to_string(),
            "node_modules".to_string(),
            ".next".to_string(),
        ];
        let mut found: Vec<String> = build_output(&t, &names)
            .iter()
            .map(|p| p.strip_prefix(&t).unwrap().display().to_string())
            .collect();
        found.sort();
        assert_eq!(found, ["node_modules", "target", "target/sub/target"]);
        assert!(bytes_of(&t.join("target")) > 0);
        assert!(remove(&t.join("target")));
        assert!(!t.join("target").exists());
        assert!(remove(&t.join("gone")));
        fs::remove_dir_all(&t).unwrap();
    }
}
