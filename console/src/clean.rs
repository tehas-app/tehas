//! What `tehasbot clean` removes, for libexec/clean. The shell checks the
//! settings and names the two images; this takes the run's lock and runs the
//! six steps.
//!
//!   console clean run <dry 0|1> <home> <work dir> <after hours>
//!     <min free MiB> <min free as set> <free KiB, or empty> <clean dirs>
//!     <sandbox image> <base image, or empty> <log file>
//!     Exit 0 every step worked; 1 a step failed or the log could not be
//!     written; 3 another run holds the lock; 64 a usage error.
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
//!    tmp/node-compile-cache, tmp/analytics-facts.* files of that age, the
//!    copy of the sandbox recipe (and its temporary) of that age a killed build
//!    left beside the build script, and tmp/tehas-test.* directories.
//! 4. Dated tags of the sandbox image of that age, except the newest and any
//!    a container or a uid tag holds.
//! 5. `sha-` tags of the base image's repository of that age, which no
//!    container was made from and no kept tag (uid, release-) stands on, each
//!    removed by its own name. Never `docker image prune`.
//! 6. `store upkeep`: old logs, a daily backup of the state store, prune.
//!
//! One line per removal and a closing line with the bytes freed go to the log
//! and to standard output; a warning goes to the log and standard error.

use crate::store::{self, bounded, merged};
use serde_json::json;
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};
use tehas_core::time;

const USAGE: &str = "usage: console clean run <dry 0|1> <home> <work dir> <after hours> <min free MiB> <min free> <free KiB> <dirs> <sandbox image> <base image> <log file> <scratch dir>\n       console clean ended <workspace> <dirs> <log file> <scratch of the cycle>\n       console clean scratch <scratch dir> <name>";

pub fn main(given: &[String]) -> u8 {
    if given.first().is_some_and(|v| v == "ended") {
        return ended(&given[1..]);
    }
    if given.first().is_some_and(|v| v == "scratch") {
        return scratch_made(&given[1..]);
    }
    let [verb, dry, home, work, after, min_mb, min_word, free_kb, dirs, sandbox, pulled, logfile, tmp_disk] =
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
        dirs: dirs.split_whitespace().map(String::from).collect(),
        sandbox: sandbox.clone(),
        pulled: pulled.clone(),
        logfile: PathBuf::from(logfile),
        tmp_disk: tmp_disk.clone(),
        freed: 0,
        rc: 0,
    };
    run.main(min_mb, min_word, free_kb)
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

struct Run {
    dry: bool,
    home: PathBuf,
    works: String,
    base: String,
    after: u64,
    build_after: u64,
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
        ];
        if steps.contains(&false) {
            self.rc = 1;
        }
        let freed = self.freed;
        self.log(&format!("freed {freed} bytes"));
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
            if self.dry {
                self.log(&format!("would remove {shown}: a cycle's scratch"));
                continue;
            }
            let size = bytes_of(&p);
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

    /// A cycle holds the issue's lock, or the probe could not open it.
    fn cycle_running(&self, n: &str) -> bool {
        let lock = self.cycle_lock(n);
        lock.exists() && File::open(&lock).map_or(true, |f| f.try_lock().is_err())
    }

    /// Only when the probe can tell the lock is free: no lock, or one it took.
    fn lock_free(&self, n: &str) -> bool {
        let lock = self.cycle_lock(n);
        if !lock.exists() {
            return true;
        }
        plain_file(&lock) && File::open(&lock).is_ok_and(|f| f.try_lock().is_ok())
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
                if self.dry {
                    let hours = self.build_after;
                    self.log(&format!(
                        "would remove {shown}: {what}, untouched for {hours} hours"
                    ));
                    continue;
                }
                if lock_refusal != 0 {
                    refused = true;
                    continue;
                }
                let kb = bytes_of(&found);
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
        if self.dry {
            self.log(&format!("would remove {shown}: {would}"));
            return true;
        }
        let kb = bytes_of(f);
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
                self.log(&format!(
                    "would remove {s}:{tag}: a sandbox image of that age"
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
                self.log(&format!("would remove {name}: a base image of that age"));
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
            self.log(&format!("{act} logs/{n}: a log 7 days old"));
        }
        if !self.dry {
            let kept = json!({"backup": v["backup"], "prune": v["prune"]});
            self.log(&format!("store upkeep: {kept}"));
        }
        true
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
