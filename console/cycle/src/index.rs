//! `tehas-cycle index`: build the code index that a cycle's agents search, or
//! refresh it. libexec/index holds the lock and starts this.
//!
//! pi-rukas tells its agents to find code with the codebase_memory tools. Those
//! answer from an index that nothing builds on its own: interactive pi-rukas
//! builds it in /start, which a headless cycle never runs. Without an index
//! every search in a cycle fails and the agents fall back to reading files.
//!
//! The index is kept in the host's own directory, $TEHAS_HOME/state/index, under
//! the name of the tree it was made from. No container is given that directory:
//! tehas work and tehas plan copy it into the cache directory of the container
//! they start, so a cycle reads the host's index and changes only its own copy.
//! This runs the indexer in the sandbox image, over a clone mounted at the path a
//! cycle works in, so the index it writes is the one a cycle looks for. The clone
//! is made at the tip the product checkout last fetched, mounted read-only and
//! removed afterwards. The cycle's workspace is not touched.
//!
//! The index is as fresh as its last run. The dispatcher runs this as
//! tehas-index.service when it would start cycles and none is running, and
//! starts them on the poll after it, so those cycles start with an index of the
//! tree they are cloned from. A cycle started while another runs gets the index
//! as it is, which falls behind the branch while cycles overlap. What a cycle
//! changes is never in it.
//!
//! The record names the sandbox image that built the index, by the id Docker
//! gives that image. An index written by another image is not updated: what one
//! indexer wrote is not data the next one can add to. What that image wrote is
//! set aside outside the index, the run builds from scratch, and it says which
//! two images it compared. A run that did not write its index puts them back, so
//! a failed rebuild leaves cycles the index they had instead of nothing, and a
//! run that wrote a database keeps what it wrote instead. A run that was killed
//! never gets that far: the next run takes what it left set aside back into the
//! index, or removes it when the index already holds a database. The id is read
//! from the image, not from the pin in host/versions.env, because a host that
//! pulled a new pin has not necessarily rebuilt the image.
//!
//! Called before a cycle, it is done only when the directory holds index data
//! and its record names the branch's tip. The record alone is not enough: on a
//! host upgraded from the version that kept the index in a cache volume, the
//! record names the tip while this directory is empty, and every cycle would be
//! given nothing to search. Only a database counts as index data, and after a
//! run that wrote one the leftovers of earlier runs are removed, so what a cycle
//! is copied stays the size of the index.
//!
//! One run at a time, under $TEHAS_HOME/state/index.lock, and none while a cycle
//! runs: a cycle copies the index when it starts. A poll does not start a cycle
//! while that lock is held, and a poll that is starting cycles holds the
//! dispatcher's lock, so a run waits for that lock, up to a minute, before it
//! looks at the cycles, and lets it go again. A poll is never held by a run.
//!
//! What the container is given is pinned by test/index_test.sh: the clone, the
//! host's index directory, which is the only thing it may write, and nothing
//! else. No network, no token, no settings. It runs in tehas.slice with a
//! cycle's limits, so the dispatcher counts its memory with the cycles'. Without
//! a usable slice nothing is cloned and nothing starts.

use serde_json::Value;
use std::env;
use std::fs::{self, OpenOptions};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};
use tehas_core::lock::{self, Probe};
use tehas_core::sandbox::{self, CACHE_DIR};
use tehas_core::slice::{self, Load, Reading};
use tehas_core::time;

// How long the indexer may run, in seconds. No cycle starts meanwhile. Indexing
// takes seconds to minutes; this bounds an indexer that hangs.
// tehas-index.service, which the dispatcher starts, stops after 2400 s.
const BOUND: u32 = 1800;
const CONTAINER: &str = "tehas-index";

fn say(line: &str) {
    println!("tehas index: {line}");
}

fn warn(line: &str) {
    eprintln!("tehas index: {line}");
}

fn short(commit: &str) -> &str {
    &commit[..commit.len().min(12)]
}

// The plain files of `$dir/*`, in the order of a glob. A symbolic link to a file
// is a file; the indexer's logs are a directory.
fn files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    found.sort();
    found
}

fn name(file: &Path) -> String {
    file.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

fn is_config(file: &Path) -> bool {
    name(file).ends_with("_config.db")
}

fn is_database(file: &Path) -> bool {
    name(file).ends_with(".db")
}

fn written_since(file: &Path, started: i64) -> bool {
    fs::metadata(file).is_ok_and(|m| m.mtime() >= started)
}

// `mv -f <file> <dir>`, also from one file system to another.
fn move_into(file: &Path, dir: &Path) -> bool {
    let to = dir.join(name(file));
    fs::rename(file, &to).is_ok() || (fs::copy(file, &to).is_ok() && fs::remove_file(file).is_ok())
}

// Move what another sandbox image wrote out of <dir>, so this run finds nothing
// to add to. The files wait outside the index rather than being deleted: the run
// that replaces them can fail, and the index they hold is the only one a cycle
// has until a run succeeds. Nothing is said when the directory holds no file,
// which is what a host that has never been indexed looks like. The indexer's own
// configuration and its logs stay.
fn set_aside(
    dir: &Path,
    tmp: &Path,
    built: &str,
    now: &str,
    stale: &mut Option<PathBuf>,
) -> Result<(), ()> {
    if built == now {
        return Ok(());
    }
    let Some(aside) = temp_dir(tmp, "index.stale") else {
        warn("could not make a place for the index of another sandbox image, nothing was indexed");
        return Err(());
    };
    *stale = Some(aside.clone());
    let mut moved = 0;
    for f in files(dir).iter().filter(|f| !is_config(f)) {
        // A file that cannot be moved would stay and mix two images' data,
        // which is what this is for. The run stops instead of doing it halfway.
        if !move_into(f, &aside) {
            warn(&format!(
                "could not set aside {}, nothing was indexed",
                f.display()
            ));
            return Err(());
        }
        moved += 1;
    }
    if moved > 0 {
        let built = if built.is_empty() { "unknown" } else { built };
        say(&format!(
            "the index was built by sandbox image {built}, this run uses {now}: {moved} file(s) it wrote are set aside and the index is built from scratch"
        ));
    }
    Ok(())
}

// Put back what this run set aside, so a run that did not write its index leaves
// the host the index it found. Does nothing for a run that succeeded or never
// rebuilt. A run whose indexer did write a database keeps what it wrote and
// drops the set-aside instead: the answer can fail this command's check for a
// reason that has nothing to do with the write, and putting the other image's
// files back over them would lose the index this run made and mix two images'
// data. `started` is None when the run never got that far.
fn restore(dir: &Path, started: Option<i64>, stale: &mut Option<PathBuf>) {
    let Some(aside) = stale.take().filter(|d| d.is_dir()) else {
        return;
    };
    let wrote = started.is_some_and(|at| {
        files(dir)
            .iter()
            .any(|f| is_database(f) && !is_config(f) && written_since(f, at))
    });
    if wrote {
        warn("this run wrote index data, what the other sandbox image wrote was dropped");
        let _ = fs::remove_dir_all(&aside);
        return;
    }
    for f in files(&aside) {
        if !move_into(&f, dir) {
            warn(&format!(
                "could not put {} back in {}",
                f.display(),
                dir.display()
            ));
        }
    }
    let _ = fs::remove_dir(&aside);
}

// Take back what a run that was killed set aside: it never got to, so those
// files wait in $TEHAS_HOME/tmp and no later run looks at them. Only this run
// holds the index's lock, so what is set aside there belongs to no run in
// progress. When the index holds no database the cycles have nothing to search,
// so the files go back where a run can use them; when it does, they are what
// this run would only set aside again, and they are removed.
fn reclaim(dir: &Path, tmp: &Path) {
    let empty = !files(dir).iter().any(|f| is_database(f) && !is_config(f));
    let mut asides: Vec<PathBuf> = fs::read_dir(tmp)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| name(p).starts_with("index.stale.") && p.is_dir())
        .collect();
    asides.sort();
    let (mut back, mut gone) = (0, 0);
    for aside in asides {
        for f in files(&aside) {
            if empty {
                back += u32::from(move_into(&f, dir));
            } else {
                gone += u32::from(fs::remove_file(&f).is_ok());
            }
        }
        let _ = fs::remove_dir(&aside);
    }
    if back > 0 || gone > 0 {
        say(&format!(
            "a run that was killed left index data set aside: {back} file(s) put back in {}, {gone} removed",
            dir.display()
        ));
    }
}

// Remove what earlier runs left in <dir>, so the copy a cycle is given stays the
// size of the index and not of everything the indexer ever wrote. A staging file
// is how the indexer writes: a run that was stopped leaves one behind, about half
// the size of the database it was writing, and nothing reads it again. A database
// named for another tree is what a host keeps after the path it indexes changed.
// Only a run that wrote its own database removes the others.
fn prune(dir: &Path, started: i64) {
    let all = files(dir);
    let fresh = all
        .iter()
        .any(|f| is_database(f) && written_since(f, started));
    let mut left = 0;
    for f in &all {
        if is_config(f) || (is_database(f) && (!fresh || written_since(f, started))) {
            continue;
        }
        left += u32::from(fs::remove_file(f).is_ok());
    }
    if left > 0 {
        say(&format!(
            "removed {left} file(s) an earlier run left in {}",
            dir.display()
        ));
    }
}

fn temp_dir(tmp: &Path, prefix: &str) -> Option<PathBuf> {
    let out = Command::new("mktemp")
        .arg("-d")
        .arg(tmp.join(format!("{prefix}.XXXXXX")))
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let path = String::from_utf8_lossy(&out.stdout);
    Some(PathBuf::from(path.trim_end_matches('\n')))
}

// What a running cycle looks like: its lock is held. A lock that cannot be read
// counts as held, since nothing here may guess that no cycle runs.
fn running(cycles: &Path) -> Vec<String> {
    let mut held: Vec<String> = fs::read_dir(cycles)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let issue = name(&path).strip_suffix(".lock")?.to_string();
            (!issue.is_empty() && !matches!(lock::probe(&path), Probe::Free | Probe::Absent))
                .then_some(issue)
        })
        .collect();
    held.sort();
    held
}

// The poll that is starting cycles holds the dispatcher's lock: wait for it, look
// at the cycles, let it go. Err is the exit status.
fn cycles_idle(home: &Path, wait: &str) -> Result<(), u8> {
    let path = home.join("state/dispatch.lock");
    let poll = OpenOptions::new().append(true).create(true).open(path);
    let seconds: f64 = wait.parse().unwrap_or(0.0);
    let until = Instant::now() + Duration::from_secs_f64(seconds.max(0.0));
    let held = poll.ok().filter(|f| loop {
        if f.try_lock().is_ok() {
            break true;
        }
        if Instant::now() >= until {
            break false;
        }
        sleep(Duration::from_millis(100));
    });
    if held.is_none() {
        warn(&format!(
            "a poll held the dispatcher's lock for {wait}s, nothing was indexed; run it again in a moment"
        ));
        return Err(3);
    }
    let issues = running(&home.join("state/cycles"));
    if !issues.is_empty() {
        warn(&format!(
            "a cycle is running (issue {}), nothing was indexed; run it again afterwards",
            issues.join(" ")
        ));
        return Err(3);
    }
    Ok(())
}

// A value as jq -r prints it, with what jq's `// <default>` gives for false,
// null and a missing key.
fn jq_raw(v: Option<&Value>, default: &str) -> String {
    match v {
        None | Some(Value::Null) | Some(Value::Bool(false)) => default.to_string(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

fn record_of(marker: &Path) -> Value {
    fs::read_to_string(marker)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null)
}

#[derive(Debug, PartialEq)]
enum State {
    Current,
    Failed,
    Due,
}

// lib/cycles.sh keeps the same reading for the bash poll: tehas_index_state.
fn state(dir: &Path, marker: &Path, failed: &Path, commit: &str) -> State {
    if jq_raw(record_of(marker).get("commit"), "") == commit
        && files(dir)
            .iter()
            .any(|f| is_database(f) && name(f) != "_config.db")
    {
        return State::Current;
    }
    let at = fs::read(failed).unwrap_or_default();
    let at = String::from_utf8_lossy(&at[..at.len().min(64)]);
    if at.trim_end_matches('\n') == commit {
        State::Failed
    } else {
        State::Due
    }
}

// The indexer answers with one JSON object: the last line that opens one.
fn answer_of(log: &str) -> Value {
    let line = log.lines().rfind(|l| l.starts_with('{')).unwrap_or("");
    serde_json::from_str(line).unwrap_or(Value::Null)
}

// jq -n '{commit: ..., at: ..., indexer_image: ...}', in its order and layout.
fn record(commit: &str, branch: &str, image: &str, nodes: &str, edges: &str, at: i64) -> String {
    let text = |s: &str| Value::from(s).to_string();
    format!(
        "{{\n  \"commit\": {},\n  \"branch\": {},\n  \"nodes\": {nodes},\n  \"edges\": {edges},\n  \"at\": {at},\n  \"indexer_image\": {}\n}}\n",
        text(commit),
        text(branch),
        text(image)
    )
}

struct Run {
    home: PathBuf,
    index: PathBuf,
    tmp: PathBuf,
    marker: PathBuf,
    failed: PathBuf,
    log: PathBuf,
    /// The product's checkout and the path a cycle works in.
    src: String,
    repo: String,
    product: String,
    image: String,
    branch: String,
    base: String,
    /// Where what another image wrote waits; None when nothing is waiting.
    stale: Option<PathBuf>,
    tree: Option<PathBuf>,
    started: Option<i64>,
}

impl Run {
    fn clone_at_base(&self, tree: &Path) -> bool {
        let git = |args: &[&str]| {
            Command::new("git")
                .args(args)
                .status()
                .is_ok_and(|s| s.success())
        };
        let tree = tree.display().to_string();
        let refspec = format!(
            "+refs/remotes/origin/{0}:refs/remotes/origin/{0}",
            self.branch
        );
        git(&["init", "-q", "-b", &self.branch, &tree])
            && git(&["-C", &tree, "fetch", "-q", "--no-tags", &self.src, &refspec])
            && git(&[
                "-C",
                &tree,
                "checkout",
                "-q",
                "-B",
                &self.branch,
                &self.base,
            ])
    }

    // A run that was killed leaves its container under this name, and every
    // later run would fail on the name. timeout stops the docker client, not the
    // container.
    fn remove_container(&self) {
        let _ = Command::new("docker")
            .args(["rm", "-f", CONTAINER])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    // What the container is given, in order. The exit status of the client, 127
    // when it cannot be started.
    fn indexer(&self, limits: &[String], user: &str, tree: &Path) -> i32 {
        let err = self.log.with_extension("log.err");
        let (Ok(out), Ok(err)) = (fs::File::create(&self.log), fs::File::create(err)) else {
            return 1;
        };
        let status = Command::new("timeout")
            .args(["-k", "30", &BOUND.to_string()])
            .args(run_args(self, limits, user, tree))
            .stdout(out)
            .stderr(err)
            .status();
        status.map_or(127, |s| s.code().unwrap_or(1))
    }

    fn go(&mut self, before_cycle: bool, wait: &str) -> u8 {
        if let Err(status) = cycles_idle(&self.home, wait) {
            return status;
        }
        let short = short(&self.base).to_string();
        if before_cycle {
            match state(&self.index, &self.marker, &self.failed, &self.base) {
                State::Current => {
                    say(&format!("{} is already indexed at {short}", self.product));
                    return 0;
                }
                // Without this every idle poll would run the same failing
                // indexer again.
                State::Failed => {
                    warn(&format!(
                        "the indexer failed at {short}, not tried again before the branch moves; run tehas index to try now"
                    ));
                    return 1;
                }
                State::Due => {}
            }
        }

        // No container starts outside the slice, and none with half its limits.
        // Asked here, not at the top: a run that finds the lock held or the
        // index current has no container to start.
        let load = Load::from_env();
        let limits = match slice::usable(&Reading::probe(), &load.reserve, &load.containers) {
            Ok(usable) => slice::limits(&usable, &load),
            Err(why) => {
                warn(&why);
                return 78;
            }
        };
        let Some(limits) = limits else {
            warn("the container limits could not be made, not starting");
            return 78;
        };

        // One call answers both questions: is the image there, and which image
        // is it.
        let inspect = Command::new("docker")
            .args(["image", "inspect", &self.image, "-f", "{{.Id}}"])
            .stderr(Stdio::null())
            .output();
        let Some(id) = inspect.ok().filter(|o| o.status.success()) else {
            warn(&format!(
                "no sandbox image {}, nothing was indexed; build it with {}/sandbox/build.sh",
                self.image,
                self.home.display()
            ));
            return 1;
        };
        let image_id = String::from_utf8_lossy(&id.stdout)
            .trim_end_matches('\n')
            .to_string();
        // The image that built the index, as the run that wrote it recorded
        // one. Empty when none is recorded.
        let built = jq_raw(record_of(&self.marker).get("indexer_image"), "");

        let Some(tree) = temp_dir(&self.tmp, "index") else {
            return 1;
        };
        self.tree = Some(tree.clone());
        if !self.clone_at_base(&tree) {
            warn(&format!(
                "could not clone {}, nothing was indexed",
                self.src
            ));
            return 1;
        }

        // Asked after the clone: a run that could not clone keeps the index it
        // has, and so does one that could not set aside what another image
        // wrote. What a killed run left set aside is dealt with first: it is
        // index data no cycle can search, and this run would only move it out
        // again.
        reclaim(&self.index, &self.tmp);
        if set_aside(&self.index, &self.tmp, &built, &image_id, &mut self.stale).is_err() {
            return 1;
        }

        let started = time::now();
        self.started = Some(started);
        self.remove_container();
        let user = sandbox::user();
        let rc = self.indexer(&limits, &user, &tree);

        let log = fs::read_to_string(&self.log).unwrap_or_default();
        let answer = answer_of(&log);
        if rc != 0 || jq_raw(answer.get("status"), "") != "indexed" {
            let log = self.log.display();
            if rc == 124 {
                warn(&format!(
                    "the indexer did not finish within {BOUND}s, see {log} and {log}.err"
                ));
            } else {
                warn(&format!(
                    "the indexer failed (exit {rc}), see {log} and {log}.err"
                ));
            }
            let _ = fs::write(&self.failed, format!("{}\n", self.base));
            self.remove_container();
            return 1;
        }

        let nodes = jq_raw(answer.get("nodes"), "0");
        let edges = jq_raw(answer.get("edges"), "0");
        let new = self.marker.with_extension("json.new");
        let text = record(
            &self.base,
            &self.branch,
            &image_id,
            &nodes,
            &edges,
            time::now(),
        );
        if serde_json::from_str::<Value>(&text).is_ok() && fs::write(&new, text).is_ok() {
            let _ = fs::rename(&new, &self.marker);
        }
        let _ = fs::remove_file(&self.failed);
        // This run wrote its index, so what the other image wrote is no longer
        // needed.
        if let Some(aside) = self.stale.take() {
            let _ = fs::remove_dir_all(aside);
        }
        // Only a run that wrote its index prunes. A failed one may have left the
        // half written database where a later run can still find it.
        prune(&self.index, started);
        say(&format!(
            "{} indexed at {short} on {}, {nodes} nodes, {edges} edges, {}s",
            self.product,
            self.branch,
            time::now() - started
        ));
        0
    }
}

// The arguments of the indexer's client, after `timeout -k 30 <bound>`.
fn run_args(r: &Run, limits: &[String], user: &str, tree: &Path) -> Vec<String> {
    let mut args: Vec<String> = [
        "docker",
        "run",
        "--rm",
        "--init",
        "--name",
        CONTAINER,
        "--network",
        "none",
    ]
    .map(String::from)
    .to_vec();
    args.extend_from_slice(limits);
    let repo = &r.repo;
    args.extend(
        [
            "--user",
            user,
            "-e",
            "HOME=/home/vscode",
            "-e",
            &format!("CBM_CACHE_DIR={CACHE_DIR}"),
            "-w",
            repo,
            "-v",
            &format!("{}:{repo}:ro", tree.display()),
            "-v",
            &format!("{}:{CACHE_DIR}", r.index.display()),
            "--entrypoint",
            "codebase-memory-mcp",
            &r.image,
            "cli",
            "index_repository",
            "--repo-path",
            repo,
        ]
        .map(String::from),
    );
    args
}

/// This file's row of the table of runs: test/sandbox_test.sh reads it off the
/// arguments the container is started with.
pub fn table() -> String {
    let run = Run {
        home: PathBuf::new(),
        index: PathBuf::new(),
        tmp: PathBuf::new(),
        marker: PathBuf::new(),
        failed: PathBuf::new(),
        log: PathBuf::new(),
        src: String::new(),
        repo: String::new(),
        product: String::new(),
        image: "PI_ENSEMBLE_IMAGE".to_string(),
        branch: String::new(),
        base: String::new(),
        stale: None,
        tree: None,
        started: None,
    };
    let args = run_args(&run, &[], "", Path::new(""));
    let at = args.iter().position(|a| a == "--entrypoint");
    let entry = at.map_or("", |i| &args[i + 1]);
    format!("index\t{}\t{entry}", run.image)
}

/// One run, under the lock libexec/index holds on a descriptor this program and
/// the indexer's docker client inherit: a client that outlives this still
/// writes the index. `base` is the tip of <branch> the product checkout last
/// fetched.
pub fn main(branch: &str, base: &str, before_cycle: bool) -> u8 {
    let var = |k: &str| env::var(k).unwrap_or_default();
    let home = PathBuf::from(var("TEHAS_HOME"));
    for dir in ["logs", "tmp", "state/index"] {
        let _ = fs::create_dir_all(home.join(dir));
    }
    let repo_name = var("TEHAS_REPO");
    let product = repo_name.rsplit('/').next().unwrap_or("").to_string();
    let wait = env::var("TEHAS_INDEX_LOCK_WAIT").unwrap_or_else(|_| "60".into());
    let mut run = Run {
        index: home.join("state/index"),
        tmp: home.join("tmp"),
        marker: home.join("state/index.json"),
        // Apart from the marker, which install.sh reads as "an index was built".
        failed: home.join("state/index.failed"),
        log: home.join(format!("logs/index-{}.log", time::stamp(time::now()))),
        src: var("TEHAS_WORK_REPO"),
        repo: var("TEHAS_WORK_DIR"),
        product,
        image: var("PI_ENSEMBLE_IMAGE"),
        branch: branch.to_string(),
        base: base.to_string(),
        stale: None,
        tree: None,
        started: None,
        home,
    };
    let status = run.go(before_cycle, &wait);
    restore(&run.index, run.started, &mut run.stale);
    if let Some(tree) = run.tree.take() {
        let _ = fs::remove_dir_all(tree);
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    struct Dir(PathBuf);

    impl Dir {
        fn new(name: &str) -> Dir {
            let at = env::temp_dir().join(format!("index-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&at);
            fs::create_dir_all(at.join("index")).unwrap();
            fs::create_dir_all(at.join("tmp")).unwrap();
            Dir(at)
        }
        fn index(&self) -> PathBuf {
            self.0.join("index")
        }
        fn tmp(&self) -> PathBuf {
            self.0.join("tmp")
        }
        fn put(&self, file: &str) -> PathBuf {
            let at = self.index().join(file);
            fs::write(&at, "x").unwrap();
            at
        }
        fn old(&self, file: &str) {
            let at = self.put(file);
            let then = SystemTime::now() - Duration::from_secs(1000);
            let f = fs::File::options().write(true).open(at).unwrap();
            f.set_modified(then).unwrap();
        }
        fn names(&self) -> Vec<String> {
            files(&self.index()).iter().map(|f| name(f)).collect()
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn soon() -> i64 {
        time::now() - 10
    }

    #[test]
    fn the_same_image_sets_nothing_aside() {
        let d = Dir::new("same");
        d.put("widget.db");
        let mut stale = None;
        assert!(set_aside(&d.index(), &d.tmp(), "a", "a", &mut stale).is_ok());
        assert_eq!(stale, None);
        assert_eq!(d.names(), ["widget.db"]);
    }

    #[test]
    fn another_images_files_wait_outside_but_its_configuration_stays() {
        let d = Dir::new("aside");
        d.put("widget.db");
        d.put("_config.db");
        d.put("widget.db.stage.1");
        fs::create_dir(d.index().join("logs")).unwrap();
        let mut stale = None;
        assert!(set_aside(&d.index(), &d.tmp(), "a", "b", &mut stale).is_ok());
        assert_eq!(d.names(), ["_config.db"]);
        let aside = stale.unwrap();
        assert_eq!(files(&aside).len(), 2);
        assert!(d.index().join("logs").is_dir());
    }

    #[test]
    fn a_run_that_wrote_nothing_puts_them_back() {
        let d = Dir::new("back");
        d.put("widget.db");
        let mut stale = None;
        set_aside(&d.index(), &d.tmp(), "", "b", &mut stale).unwrap();
        let aside = stale.clone().unwrap();
        restore(&d.index(), Some(soon()), &mut stale);
        assert_eq!(d.names(), ["widget.db"]);
        assert!(!aside.exists());
        assert_eq!(stale, None);
    }

    #[test]
    fn a_run_that_wrote_a_database_keeps_it_and_drops_the_other() {
        let d = Dir::new("keep");
        d.put("old.db");
        let mut stale = None;
        set_aside(&d.index(), &d.tmp(), "a", "b", &mut stale).unwrap();
        let aside = stale.clone().unwrap();
        d.put("new.db");
        restore(&d.index(), Some(soon()), &mut stale);
        assert_eq!(d.names(), ["new.db"]);
        assert!(!aside.exists());
    }

    #[test]
    fn a_run_that_never_started_the_indexer_puts_them_back_over_nothing() {
        let d = Dir::new("never");
        d.put("old.db");
        let mut stale = None;
        set_aside(&d.index(), &d.tmp(), "a", "b", &mut stale).unwrap();
        d.put("other.db");
        restore(&d.index(), None, &mut stale);
        assert_eq!(d.names(), ["old.db", "other.db"]);
    }

    #[test]
    fn what_a_killed_run_left_goes_back_to_an_empty_index() {
        let d = Dir::new("reclaim");
        let aside = d.tmp().join("index.stale.abc");
        fs::create_dir(&aside).unwrap();
        fs::write(aside.join("widget.db"), "x").unwrap();
        d.put("_config.db");
        reclaim(&d.index(), &d.tmp());
        assert_eq!(d.names(), ["_config.db", "widget.db"]);
        assert!(!aside.exists());
    }

    #[test]
    fn what_a_killed_run_left_is_removed_when_the_index_has_a_database() {
        let d = Dir::new("reclaim2");
        let aside = d.tmp().join("index.stale.abc");
        fs::create_dir(&aside).unwrap();
        fs::write(aside.join("widget.db"), "x").unwrap();
        d.put("kept.db");
        reclaim(&d.index(), &d.tmp());
        assert_eq!(d.names(), ["kept.db"]);
        assert!(!aside.exists());
    }

    #[test]
    fn a_run_that_wrote_its_database_prunes_what_is_older() {
        let d = Dir::new("prune");
        d.put("new.db");
        d.old("other-tree.db");
        d.old("_config.db");
        d.old("new.db.stage.7");
        d.old("something.tmp");
        prune(&d.index(), soon());
        assert_eq!(d.names(), ["_config.db", "new.db"]);
    }

    #[test]
    fn a_run_with_no_fresh_database_keeps_the_databases() {
        let d = Dir::new("prune2");
        d.old("old.db");
        d.old("x.db.stage.1");
        prune(&d.index(), soon());
        assert_eq!(d.names(), ["old.db"]);
    }

    #[test]
    fn the_state_needs_the_record_and_a_database() {
        let d = Dir::new("state");
        let (marker, failed) = (d.0.join("index.json"), d.0.join("index.failed"));
        let state_at = |c: &str| state(&d.index(), &marker, &failed, c);
        assert_eq!(state_at("abc"), State::Due);
        fs::write(&marker, r#"{"commit":"abc"}"#).unwrap();
        assert_eq!(state_at("abc"), State::Due);
        d.put("_config.db");
        assert_eq!(state_at("abc"), State::Due);
        d.put("widget.db");
        assert_eq!(state_at("abc"), State::Current);
        assert_eq!(state_at("def"), State::Due);
        fs::write(&failed, "def\n").unwrap();
        assert_eq!(state_at("def"), State::Failed);
    }

    #[test]
    fn the_answer_is_the_last_line_that_opens_an_object() {
        let a = answer_of(
            "noise\n{\"status\":\"no\"}\nmore\n{\"status\":\"indexed\",\"nodes\":5}\ntail\n",
        );
        assert_eq!(a["status"], "indexed");
        assert_eq!(answer_of("nothing\n"), Value::Null);
        assert_eq!(answer_of("{broken\n"), Value::Null);
    }

    #[test]
    fn the_record_is_as_jq_writes_it() {
        assert_eq!(
            record("c", "main", "sha256:1", "5", "7", 9),
            "{\n  \"commit\": \"c\",\n  \"branch\": \"main\",\n  \"nodes\": 5,\n  \"edges\": 7,\n  \"at\": 9,\n  \"indexer_image\": \"sha256:1\"\n}\n"
        );
    }
}
