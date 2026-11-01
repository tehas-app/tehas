//! A run: the lock and the sweep, the issue stack, the window, the analyst, and
//! what its file says, filed. The steps are in mod.rs.

use super::body::{self, Page, Said};
use super::check::{self, Finding, Given, Plan, Recheck};
use super::models;
use super::pack::pack;
use crate::flags;
use crate::jq::J;
use std::collections::BTreeMap;
use std::env;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::SystemTime;
use tehas_core::models::host_path;
use tehas_core::sandbox;
use tehas_core::secret::secret;
use tehas_core::{mask, time};

const LABEL: &str = "tehas:improvement";
const CROSS: &str = "cross-project";
const TAG: &str = "tehas-improve-logs";
const CHUNK: u64 = 5 * 1024 * 1024;
// The analyst's bound, in seconds. A window of nine cycles took 27 minutes, so
// 30 cut off half the runs.
const BOUND: u64 = 3600;
const MAX_TAKE: usize = 5;
const OLD: u64 = 7 * 24 * 60 * 60;
const FINDINGS_BYTES: u64 = 1_048_576;

fn say(line: &str) {
    let _ = writeln!(io::stdout(), "{} improve: {line}", time::iso(time::now()));
}

fn remove(path: &Path) {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => {
            let _ = fs::remove_dir_all(path);
        }
        Ok(_) => {
            let _ = fs::remove_file(path);
        }
        Err(_) => {}
    }
}

fn hidden(name: &str) -> bool {
    name.starts_with('.')
}

fn directories(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
        .filter(|(name, path)| !hidden(name) && path.is_dir())
        .collect()
}

/// The records in a directory, by name. Any other directory there is removed.
fn look(dir: &Path) -> Vec<String> {
    let mut recs = Vec::new();
    for (name, path) in directories(dir) {
        if check::is_record(&name) {
            recs.push(name);
        } else {
            remove(&path);
        }
    }
    recs.sort();
    recs
}

/// The records of a run return to the window; the rest of it goes.
fn give_back(run: &Path, window: &Path) {
    let _ = fs::create_dir_all(window);
    for (name, path) in directories(&run.join("in/cycles")) {
        let to = window.join(name);
        remove(&to);
        let _ = fs::rename(path, to);
    }
    remove(run);
}

/// Older than seven days, by the time it was last changed.
fn old(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age.as_secs_f64() > OLD as f64)
}

/// The cap of `state/improve.cap`: a digit from 1 to 5, or five.
fn cap_of(file: &Path) -> usize {
    let mut text = Vec::new();
    if let Ok(f) = File::open(file) {
        let _ = f.take(8).read_to_end(&mut text);
    }
    while text.last() == Some(&b'\n') {
        text.pop();
    }
    match text[..] {
        [d @ b'1'..=b'5'] => usize::from(d - b'0'),
        _ => MAX_TAKE,
    }
}

/// The oldest first, by time and not by issue: they are the nearest to the
/// sweep.
fn oldest(mut recs: Vec<String>, cap: usize) -> Vec<String> {
    let when = |r: &str| r.split_once('-').map_or("", |(_, t)| t).to_string();
    recs.sort_by(|a, b| when(a).cmp(&when(b)).then_with(|| a.cmp(b)));
    recs.truncate(cap);
    recs
}

/// What the next run may take. Half of what this run took, not of the cap:
/// fewer records than the cap can be too many already. A run in time doubles
/// it rather than going back to five at once, which would only hit the bound
/// again.
fn next_cap(exit: i32, cap: usize, took: usize) -> usize {
    let next = match exit {
        0 => cap * 2,
        124 | 137 => took / 2,
        _ => cap,
    };
    next.clamp(1, MAX_TAKE)
}

/// `date -u +%y%m%d-%H%M%S`.
fn stamp_of(t: i64) -> String {
    let (y, m, d) = time::civil_from_days(t.div_euclid(86_400));
    let s = t.rem_euclid(86_400);
    format!(
        "{:02}{m:02}{d:02}-{:02}{:02}{:02}",
        y.rem_euclid(100),
        s / 3600,
        s % 3600 / 60,
        s % 60
    )
}

/// `date -u -d '30 days ago' +%Y-%m-%d`.
fn month_ago(now: i64) -> String {
    let (y, m, d) = time::civil_from_days((now - 30 * 86_400).div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// The log as the console masks it, in `to`. Without a log what is there goes
/// through all the same; only a copy that cannot be written is no copy.
fn masked_copy(raw: &Path, to: &Path) -> Option<String> {
    let mut given = fs::read(raw).unwrap_or_default();
    let mut err = raw.as_os_str().to_owned();
    err.push(".err");
    given.extend(fs::read(err).unwrap_or_default());
    let text = mask::mask(&String::from_utf8_lossy(&given));
    fs::write(to, &text).ok()?;
    Some(text)
}

fn gh_quiet(args: &[&str]) -> bool {
    Command::new("gh")
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

// What it prints on standard error goes to the run's log.
fn gh_says(args: &[&str]) -> Option<String> {
    let out = Command::new("gh")
        .args(args)
        .stderr(Stdio::inherit())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// What the call printed, whether or not it failed. Nothing on standard error.
fn gh_read(args: &[&str]) -> (bool, String) {
    match Command::new("gh").args(args).stderr(Stdio::null()).output() {
        Ok(out) => (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
        ),
        Err(_) => (false, String::new()),
    }
}

/// The commit of the last close, when a commit closed it. Nothing for an issue
/// closed by hand.
fn fix_of(repo: &str, number: &str) -> String {
    let (_, out) = gh_read(&[
        "api",
        "--paginate",
        &format!("repos/{repo}/issues/{number}/events"),
        "--jq",
        r#".[] | select(.event == "closed") | .commit_id // "-""#,
    ]);
    let last = out.lines().last().unwrap_or("");
    let full = last.len() == 40 && last.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    if full {
        last.to_string()
    } else {
        String::new()
    }
}

/// yes, no or unknown. GitHub's compare answers what git merge-base
/// --is-ancestor <fix> <build> would; the host keeps no clone of pi-rukas.
fn in_build(repo: &str, fix: &str, build: &str) -> &'static str {
    let (ok, out) = gh_read(&[
        "api",
        &format!("repos/{repo}/compare/{fix}...{build}"),
        "--jq",
        ".status",
    ]);
    match out.trim_end_matches('\n') {
        "identical" | "ahead" if ok => "yes",
        "behind" | "diverged" if ok => "no",
        _ => "unknown",
    }
}

/// `[.buildCommit, .rukas]`, the first that is a commit, and the image: what a
/// record says the cycle ran. Empty where it says nothing.
fn ran(record: &Path) -> (String, String) {
    let doc = fs::read(record.join("cycle.json"))
        .ok()
        .and_then(|b| J::parse(&String::from_utf8_lossy(&b)))
        .unwrap_or(J::Null);
    let hex = |s: &str| s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    let commit = [doc.get("buildCommit"), doc.get("rukas")]
        .into_iter()
        .filter_map(J::as_str)
        .find(|s| (7..=40).contains(&s.len()) && hex(s))
        .unwrap_or("");
    let image = doc
        .get("image")
        .as_str()
        .and_then(|s| s.strip_prefix("sha256:"))
        .filter(|s| s.len() == 64 && hex(s))
        .map_or(String::new(), |id| format!("sha256:{id}"));
    (commit.to_string(), image)
}

struct Loop {
    prompt: String,
    base: PathBuf,
    window: PathBuf,
    logs: PathBuf,
    state: PathBuf,
    capf: PathBuf,
    limit: PathBuf,
    repo: String,
    rukas: String,
    repos: Vec<String>,
    product: String,
    provider: String,
    model: String,
    image: String,
    hosts: Vec<String>,
    skip_rukas: String,
    max_open: i64,
}

/// Why `prepare` ended a run before it began.
enum Stop {
    Quiet,
    Code(u8),
}

struct Ready {
    _lock: File,
    recs: Vec<String>,
    ask: String,
}

impl Loop {
    /// Fails closed: libexec/improve checked these, and a run without them
    /// does nothing.
    fn from_env(prompt: &str) -> Result<Loop, u8> {
        let need = |name: &str| match env::var(name) {
            Ok(v) if !v.trim_matches(' ').is_empty() => Ok(v),
            _ => {
                eprintln!("console improve: {name} is not set");
                Err(78)
            }
        };
        let home = PathBuf::from(need("TEHAS_HOME")?);
        let repo = need("TEHAS_IMPROVE_REPO")?;
        let max_open = need("TEHAS_IMPROVE_MAX_OPEN")?;
        let Ok(max_open) = max_open.parse::<i64>() else {
            eprintln!("console improve: TEHAS_IMPROVE_MAX_OPEN is not a number");
            return Err(78);
        };
        let rukas = env::var("TEHAS_IMPROVE_RUKAS_REPO")
            .ok()
            .filter(|r| !r.is_empty())
            .unwrap_or_else(|| repo.clone());
        let mut repos = vec![repo.clone()];
        if rukas != repo {
            repos.push(rukas.clone());
        }
        let base = home.join("tmp/improve");
        Ok(Loop {
            prompt: prompt.to_string(),
            window: base.join("window"),
            base,
            logs: home.join("logs"),
            capf: home.join("state/improve.cap"),
            limit: home.join("state/limit.json"),
            state: home.join("state"),
            repo,
            rukas,
            repos,
            product: need("TEHAS_REPO")?,
            provider: need("PI_ENSEMBLE_SUBAGENT_PROVIDER")?,
            model: need("PI_ENSEMBLE_SUBAGENT_MODEL")?,
            image: need("PI_ENSEMBLE_IMAGE")?,
            hosts: sandbox::hosts(&env::var("PI_ENSEMBLE_HOST_ALIASES").unwrap_or_default()),
            skip_rukas: need("TEHAS_IMPROVE_RUKAS_SKIP")?,
            max_open,
        })
    }

    /// A run directory here was left by a run that was killed: this one holds
    /// the lock. Then whatever is over seven days old goes, analysed or not.
    fn sweep(&self) {
        for (name, path) in directories(&self.base) {
            if name.starts_with("run-") {
                give_back(&path, &self.window);
            }
        }
        if let Ok(entries) = fs::read_dir(&self.window) {
            for e in entries.flatten() {
                if old(&e.path()) {
                    remove(&e.path());
                }
            }
        }
        if let Ok(entries) = fs::read_dir(&self.base) {
            for e in entries.flatten() {
                let name = e.file_name();
                if name.to_string_lossy().starts_with(".new.") && old(&e.path()) {
                    remove(&e.path());
                }
            }
        }
    }

    fn prepare(&self) -> Result<Ready, Stop> {
        let Ok(text) = fs::read(&self.prompt) else {
            say(&format!("no prompt at {}", self.prompt));
            return Err(Stop::Code(1));
        };
        for dir in [&self.window, &self.state, &self.logs] {
            if let Err(e) = fs::create_dir_all(dir) {
                eprintln!("console improve: {}: {e}", dir.display());
                return Err(Stop::Code(1));
            }
        }
        // std's files are close-on-exec, so the container is not given the
        // lock: a client that outlives a killed run would hold it, and every
        // later run would stand back until the bound.
        let lock = match OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(self.state.join("improve.lock"))
        {
            Ok(f) => f,
            Err(e) => {
                eprintln!("console improve: state/improve.lock: {e}");
                return Err(Stop::Code(1));
            }
        };
        if matches!(
            lock.try_lock(),
            Err(TryLockError::WouldBlock | TryLockError::Error(_))
        ) {
            say("another run is going, leaving it to that one");
            return Err(Stop::Quiet);
        }
        self.sweep();
        let recs = look(&self.window);
        if recs.is_empty() {
            say("cycles=0, nothing to read");
            return Err(Stop::Quiet);
        }
        // A run is held, as a planning turn is, by a limit with planning off
        // and by one that expired or cannot be read. The records wait.
        let limit = flags::read(&self.limit, time::now());
        let holds = !matches!(limit.state(), "ok" | "none");
        if holds || limit.json_of("plan") == Some("false") {
            let line = limit.said();
            let words = line
                .strip_prefix("limit")
                .unwrap_or(&line)
                .trim_start_matches(' ');
            say(&format!(
                "cycles={}, held by the limit: {words}",
                recs.len()
            ));
            return Err(Stop::Quiet);
        }
        let ask = String::from_utf8_lossy(&text)
            .trim_end_matches('\n')
            .replace("@MINUTES@", &(BOUND / 60).to_string());
        Ok(Ready {
            _lock: lock,
            recs,
            ask,
        })
    }

    /// Its open issues and those closed in 30 days.
    fn stack_of(&self, repo: &str) -> Option<Vec<J>> {
        const FIELDS: &str = "number,title,state,labels,body";
        let open = gh_says(&[
            "issue", "list", "--repo", repo, "--state", "open", "--limit", "500", "--json", FIELDS,
        ])?;
        let closed = gh_says(&[
            "issue",
            "list",
            "--repo",
            repo,
            "--state",
            "closed",
            "--limit",
            "200",
            "--search",
            &format!("closed:>={}", month_ago(time::now())),
            "--json",
            FIELDS,
        ])?;
        check::issues_of(repo, &[&open, &closed])
    }

    /// The records are moved out one by one and the window stays, so a cycle
    /// that ends now writes into it for the next run. Renaming the window
    /// instead would lose a record written between its mkdir and its mv.
    fn take(&self, run: &Path, stack: &[J], recs: &[String]) -> bool {
        let cycles = run.join("in/cycles");
        if fs::create_dir_all(&cycles).is_err() || fs::create_dir_all(run.join("out")).is_err() {
            return false;
        }
        let issues = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(run.join("in/issues.json"))
            .and_then(|mut f| writeln!(f, "{}", J::Arr(stack.to_vec()).pretty()));
        if issues.is_err() {
            return false;
        }
        let mut took = true;
        for r in recs {
            took &= fs::rename(self.window.join(r), cycles.join(r)).is_ok();
        }
        took
    }

    fn analyst(&self, run: &Path, keys: &[String], ask: &str) -> i32 {
        let Ok(log) = File::create(run.join("analyst.log")) else {
            return 1;
        };
        let Ok(err) = log.try_clone() else { return 1 };
        let path = |p: &str| run.join(p).display().to_string();
        let mut c = Command::new("timeout");
        c.args(["-k", "60", &BOUND.to_string()]);
        c.args(["docker", "run", "--rm", "--init", "--name", "tehas-improve"]);
        c.args(["--cgroup-parent", "tehas.slice", "--cpus", "1"]);
        c.args(["--memory", "2g", "--memory-swap", "2g"]);
        c.args(["--pids-limit", "1024", "--cpu-shares", "1024"]);
        c.args(["-e", "HOME=/home/vscode"]);
        for k in keys {
            c.args(["-e", k]);
        }
        c.args(["-w", "/improve"]);
        c.args(["-v", &format!("{}:/improve/in:ro", path("in"))]);
        c.args(["-v", &format!("{}:/improve/out", path("out"))]);
        c.args([
            "-v",
            &format!(
                "{}:/home/vscode/.pi/agent/models.json:ro",
                path("models.json")
            ),
        ]);
        c.args(&self.hosts).arg(&self.image);
        c.args(["pi", "-p", "--no-session", "--no-extensions", "--no-skills"]);
        c.args(["--no-prompt-templates", "--no-context-files"]);
        c.args(["--tools", "read,grep,find,ls,write"]);
        c.args(["--provider", &self.provider, "--model", &self.model]);
        c.arg(ask);
        c.stdin(Stdio::null()).stdout(log).stderr(err);
        match c.status() {
            Ok(s) => s.code().unwrap_or_else(|| 128 + s.signal().unwrap_or(0)),
            Err(_) => 127,
        }
    }

    /// Everything after the check for a slice.
    fn go(&self, ready: Ready) -> u8 {
        let Ready { _lock, recs, ask } = ready;
        let mut got = true;
        let mut stack = Vec::new();
        for r in &self.repos {
            match self.stack_of(r) {
                Some(issues) => stack.extend(issues),
                None => got = false,
            }
        }
        if !got {
            say(&format!(
                "cycles={} failed: GitHub did not give the issue stack, the records wait",
                recs.len()
            ));
            return 1;
        }
        let rooms = check::rooms(&stack, &self.repos, LABEL, self.max_open);
        if rooms.values().sum::<i64>() <= 0 {
            say(&format!(
                "cycles={} waiting: {} open issues carry {LABEL} in {}, the limit of each",
                recs.len(),
                self.max_open,
                self.repos.join(" ")
            ));
            return 0;
        }
        // A cycle may have ended while the stack was fetched: its record comes
        // along.
        let recs = look(&self.window);
        if recs.is_empty() {
            say("cycles=0, nothing to read");
            return 0;
        }
        let stamp = stamp_of(time::now());
        let dir = self.base.join(format!("run-{stamp}"));
        let cap = cap_of(&self.capf);
        let recs = oldest(recs, cap);
        let in_window = fs::read_dir(&self.window).map_or(0, Iterator::count);
        let left = in_window.saturating_sub(recs.len());
        if !self.take(&dir, &stack, &recs) {
            if dir.is_dir() {
                give_back(&dir, &self.window);
            }
            say(&format!(
                "cycles={} failed: could not take the window",
                recs.len()
            ));
            return 1;
        }
        let recs = look(&dir.join("in/cycles"));
        let mut run = Run {
            l: self,
            kept: self.logs.join(format!("improve-analyst-{stamp}.log")),
            dir,
            recs,
            notes: Vec::new(),
            links: BTreeMap::new(),
            filed: Vec::new(),
        };
        run.go(&stack, &rooms, cap, left, &ask)
    }
}

struct Run<'a> {
    l: &'a Loop,
    dir: PathBuf,
    kept: PathBuf,
    recs: Vec<String>,
    notes: Vec<String>,
    /// What an issue says about a cycle's log: each cycle's is uploaded once a
    /// run.
    links: BTreeMap<String, String>,
    filed: Vec<String>,
}

/// One issue of a finding.
struct Issue<'a> {
    to: &'a str,
    does: &'a str,
    half: &'a str,
    other_repo: &'a str,
    other: &'a str,
}

impl Run<'_> {
    fn fail(&self, why: &str) -> u8 {
        give_back(&self.dir, &self.l.window);
        say(&format!("cycles={} failed: {why}", self.recs.len()));
        1
    }

    /// What the analyst printed goes with the run. Of a run that failed on it,
    /// it is kept under the run's stamp until the sweep, so a person can still
    /// read it after later runs.
    fn blame(&self, why: &str) -> u8 {
        if fs::copy(self.dir.join("analyst.log"), &self.kept).is_ok() {
            return self.fail(&format!("{why}, its output is in {}", self.kept.display()));
        }
        self.fail(why)
    }

    fn analyse(&mut self, ask: &str, cap: usize) -> Result<Option<(i32, String)>, u8> {
        let l = self.l;
        // A run that was killed leaves its container running under this name.
        let remove_container = || {
            let _ = Command::new("docker")
                .args(["rm", "-f", "tehas-improve"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        };
        remove_container();
        let Some(keys) = models::write(&self.dir.join("models.json")) else {
            return Err(self.fail(&format!("no usable {}: run install.sh", host_path())));
        };
        let exit = l.analyst(&self.dir, &keys, ask);
        // A client that timeout killed leaves its container working on.
        if exit != 0 {
            remove_container();
        }
        // 124 is the bound, 137 the kill that follows it when the client did
        // not stop.
        let next = next_cap(exit, cap, self.recs.len());
        let _ = fs::write(&l.capf, format!("{next}\n"));
        // An analyst stopped at its bound was asked to keep its file up to
        // date, so a file that passes the check is filed. Without one, it
        // failed like any other.
        if matches!(exit, 124 | 137) {
            let why = format!(
                "the analyst ended with {exit} at its bound of {} minutes; the next run takes at most {next}; ",
                BOUND / 60
            );
            return Ok(Some((exit, why)));
        }
        if exit != 0 {
            return Err(self.blame(&format!("the analyst ended with {exit}")));
        }
        Ok(None)
    }

    /// Of a stopped analyst, a record that a finding or a followup names, valid
    /// or not, was read; the rest may not have been and go back for a later
    /// run.
    fn return_unread(&mut self, doc: &J, exit: i32) {
        let named = check::named(doc);
        let window = &self.l.window;
        let _ = fs::create_dir_all(window);
        let mut back = 0;
        for r in &self.recs {
            if named.contains(r) {
                continue;
            }
            remove(&window.join(r));
            if fs::rename(self.dir.join("in/cycles").join(r), window.join(r)).is_ok() {
                back += 1;
            }
        }
        self.notes.push(format!("stopped={exit}"));
        self.notes.push(format!("returned={back}"));
        if fs::copy(self.dir.join("analyst.log"), &self.kept).is_ok() {
            self.notes.push(format!("output={}", self.kept.display()));
        }
    }

    fn keep_log(&mut self, rec: &str, note: &str, text: &str) {
        self.links
            .insert(rec.to_string(), format!("- `{rec}`: {text}"));
        self.notes.push(format!("{note}={rec}"));
    }

    fn release(&self) -> bool {
        let l = self.l;
        gh_quiet(&["release", "view", TAG, "--repo", &l.repo])
            || gh_quiet(&[
                "release",
                "create",
                TAG,
                "--repo",
                &l.repo,
                "--prerelease",
                "--title",
                "Cycle logs of the improvement loop",
                "--notes",
                &format!("The whole logs of the cycles that issues labelled {LABEL} refer to."),
            ])
    }

    /// Pack and upload the whole log of one cycle, once a run. Err when nothing
    /// can be written under the run.
    fn upload(&mut self, rec: &str) -> Result<(), ()> {
        if self.links.contains_key(rec) {
            return Ok(());
        }
        let l = self.l;
        let dir = self.dir.join("up").join(rec);
        fs::create_dir_all(&dir).map_err(|_| ())?;
        let raw = l.logs.join(format!("work-{rec}.jsonl"));
        if !raw.is_file() {
            self.links.insert(
                rec.to_string(),
                format!("- `{rec}`: the log is no longer on the host"),
            );
            return Ok(());
        }
        let name = format!("ticket-{rec}.log");
        // The release is public: the log goes up as the console masks it, or
        // not at all.
        let copy = dir.join(&name);
        let Some(text) = masked_copy(&raw, &copy) else {
            self.keep_log(
                rec,
                "not-masked",
                "not uploaded, the log could not be masked",
            );
            return Ok(());
        };
        if secret(text.as_bytes()) {
            self.keep_log(
                rec,
                "secret-shape",
                "not uploaded, the log holds something shaped like a token",
            );
            return Ok(());
        }
        let gz = dir.join("gz");
        let Some(names) = pack(&copy, &gz, &name, CHUNK) else {
            self.keep_log(
                rec,
                "not-packed",
                "not uploaded, the log could not be packed",
            );
            return Ok(());
        };
        let files: Vec<String> = names
            .iter()
            .map(|n| gz.join(n).display().to_string())
            .collect();
        let mut upload = vec!["release", "upload", TAG];
        upload.extend(files.iter().map(String::as_str));
        upload.extend(["--repo", &l.repo, "--clobber"]);
        if !self.release() || !gh_quiet(&upload) {
            self.keep_log(rec, "not-uploaded", "the log could not be uploaded");
            return Ok(());
        }
        let links: Vec<String> = names
            .iter()
            .map(|f| {
                format!(
                    "- [{f}](https://github.com/{}/releases/download/{TAG}/{f})",
                    l.repo
                )
            })
            .collect();
        self.links.insert(rec.to_string(), links.join("\n"));
        Ok(())
    }

    /// Open one issue of a finding, and give its url.
    fn open(&mut self, f: &Finding, links: &str, i: &Issue, labels: &[&str]) -> Option<String> {
        let l = self.l;
        let text = body::issue(
            f,
            &Page {
                model: &l.model,
                product: &l.product,
                links,
                does: i.does,
                half: i.half,
                other_repo: i.other_repo,
                other: i.other,
                others: &f.others_than(i.to).join(", "),
            },
        );
        let file = self.dir.join("body.md");
        fs::write(&file, text).ok()?;
        let file = file.display().to_string();
        let mut args = vec![
            "issue",
            "create",
            "--repo",
            i.to,
            "--title",
            &f.title,
            "--body-file",
            &file,
        ];
        for label in labels {
            args.extend(["--label", label]);
        }
        let url = gh_says(&args)?;
        let url = url.trim_end_matches('\n').to_string();
        let number = url.rsplit('/').next().unwrap_or("");
        // A number alone is an issue of TEHAS_IMPROVE_REPO.
        self.filed.push(if i.to == l.repo {
            format!("#{number}")
        } else {
            format!("{}#{number}", i.to)
        });
        Some(url)
    }

    /// One issue a side, each naming its own half; the second links the first
    /// in its body, the first is told of the second in a comment. When the
    /// second is refused, the first says so: its title keeps a later run from
    /// filing the pair again, so a person files the other half.
    fn file_both_sides(&mut self, f: &Finding, links: &str, labels: &[&str]) -> bool {
        let l = self.l;
        let first = Issue {
            to: &l.repo,
            does: &format!(
                "This is the factory's half of a problem at the boundary with pi-rukas. Only that half is fixed in this repository, once a person applies `tehas:ready`. The half for pi-rukas is filed in {} and is not worked around here.",
                l.rukas
            ),
            half: "factory",
            other_repo: &l.rukas,
            other: "",
        };
        let Some(url) = self.open(f, links, &first, labels) else {
            return false;
        };
        let number = url.rsplit('/').next().unwrap_or("").to_string();
        let second = Issue {
            to: &l.rukas,
            does: &format!(
                "This is the half for pi-rukas of a problem at the boundary with the factory that runs it. Only that half is fixed here; the factory's half is filed in {}. Nothing starts until a person applies `tehas:ready`.",
                l.repo
            ),
            half: "rukas",
            other_repo: &l.repo,
            other: &url,
        };
        let comment = |text: &str| {
            gh_quiet(&[
                "issue", "comment", &number, "--repo", &l.repo, "--body", text,
            ])
        };
        let Some(second_url) = self.open(f, links, &second, labels) else {
            if !comment(&format!(
                "GitHub refused the half for pi-rukas, so it is not filed in {}, and the loop will not file it again. Until a person files it there from the text above, this issue stands alone.",
                l.rukas
            )) {
                self.notes.push(format!("not-corrected=#{number}"));
            }
            self.notes.push(format!("half-filed=#{number}"));
            return false;
        };
        if !comment(&format!("The half for pi-rukas is filed as {second_url}.")) {
            self.notes.push(format!("not-linked=#{number}"));
        }
        true
    }

    /// The body is what a cycle is given as its task, so it says whose fix it
    /// is.
    fn file_one(&mut self, f: &Finding) -> Result<bool, u8> {
        let l = self.l;
        let mut cycles: Vec<&String> = f.cycles.iter().collect();
        cycles.sort();
        cycles.dedup();
        let mut links = String::new();
        for rec in cycles {
            if self.upload(rec).is_err() {
                return Err(self.fail(&format!("could not write under {}", self.dir.display())));
            }
            links.push_str(self.links.get(rec).map_or("", String::as_str));
            links.push('\n');
        }
        let boundary = f.category == "boundary";
        let about_rukas = f.category == "pi-rukas";
        // A finding that speaks of a repository besides the one its issue is
        // filed in carries cross-project beside the loop's own label, whether
        // or not its category chose two repositories already. The label is the
        // dispatcher's tell that one cycle here cannot make the change on its
        // own; the body names the repository it has to make it in.
        let to = if about_rukas { &l.rukas } else { &l.repo };
        let mut labels = vec![LABEL];
        if boundary {
            labels.push(CROSS);
        }
        if !f.others_than(to).is_empty() {
            labels.push(CROSS);
        }
        if boundary && l.rukas != l.repo {
            return Ok(self.file_both_sides(f, &links, &labels));
        }
        let (does, half) = if boundary {
            (
                "This is a problem at the boundary between the factory and pi-rukas, and this repository is not pi-rukas. A cycle here fixes only the factory's half, once a person applies `tehas:ready`; for the half for pi-rukas it changes no code and writes down what has to change there.".to_string(),
                "both",
            )
        } else if !about_rukas {
            (
                "This is a problem in the factory. It is fixed in this repository: a cycle builds the fix once a person applies `tehas:ready`.".to_string(),
                "",
            )
        } else if l.rukas != l.repo {
            (
                format!(
                    "This is a problem in pi-rukas. It is fixed here, in pi-rukas, and not worked around in {}. Nothing starts until a person applies `tehas:ready`. When a host runs the fix and one of the issues below is worked on again, the loop says here whether the problem came back.",
                    l.product
                ),
                "",
            )
        } else {
            (
                "This is a problem in pi-rukas, and this repository is not pi-rukas. A cycle here cannot fix it and must not work around it: it changes no code, writes down what has to change in pi-rukas, and hands off.".to_string(),
                "",
            )
        };
        let issue = Issue {
            to,
            does: &does,
            half,
            other_repo: "",
            other: "",
        };
        Ok(self.open(f, &links, &issue, &labels).is_some())
    }

    /// Err when the run ended here.
    fn file(&mut self, plan: &Plan) -> Result<(), u8> {
        let l = self.l;
        if !plan.file.is_empty() {
            // It fails when the label is there already, which is fine.
            for r in &l.repos {
                gh_quiet(&[
                    "label",
                    "create",
                    LABEL,
                    "--repo",
                    r,
                    "--color",
                    "5319e7",
                    "--description",
                    "Tehas: filed by the improvement loop, a proposal",
                ]);
            }
        }
        // The label that says a cycle here cannot make the change on its own is
        // created wherever an issue of this run carries it.
        if plan
            .file
            .iter()
            .any(|f| f.category == "boundary" || !f.others.is_empty())
        {
            for r in &l.repos {
                gh_quiet(&[
                    "label",
                    "create",
                    CROSS,
                    "--repo",
                    r,
                    "--color",
                    "0e8a16",
                    "--description",
                    "Needs a change on both sides of the factory and pi-rukas",
                ]);
            }
        }
        for f in &plan.file {
            if !self.file_one(f)? {
                return Err(self.fail(&format!(
                    "GitHub refused an issue; filed before that: {}",
                    self.issues()
                )));
            }
        }
        Ok(())
    }

    fn issues(&self) -> String {
        if self.filed.is_empty() {
            "none".to_string()
        } else {
            self.filed.join(",")
        }
    }

    /// A comment that GitHub refuses is noted and the run goes on: its findings
    /// are filed, and a second run would file nothing new and lose the records
    /// anyway.
    ///
    /// Before it says "seen again" of a closed issue, the comment says whether
    /// the fix was in the build the cycle ran. The fix is the commit GitHub
    /// closed the issue with; the build is the commit the runner's result named
    /// (buildCommit), else, for a record written before tehas kept it, the
    /// image label's. Only for an issue of TEHAS_IMPROVE_RUKAS_REPO: the record
    /// names no commit of the factory. Without a build, or without an answer,
    /// the comment says the build is unknown rather than claim either.
    fn recheck(&mut self, r: &Recheck, stack: &[J]) -> Result<bool, u8> {
        let l = self.l;
        if self.upload(&r.cycle).is_err() {
            return Err(self.fail(&format!("could not write under {}", self.dir.display())));
        }
        let (rukas_at, image_at) = ran(&self.dir.join("in/cycles").join(&r.cycle));
        let number = r.number.parse::<f64>().ok();
        let state = stack
            .iter()
            .find(|i| i.get("repo").as_str() == Some(&r.repo) && i.get("number").as_f64() == number)
            .and_then(|i| i.get("state").as_str());
        let fix = if state == Some("CLOSED") {
            fix_of(&r.repo, &r.number)
        } else {
            String::new()
        };
        let mut build = "";
        let mut built = "";
        if !fix.is_empty() {
            if l.rukas != l.repo && r.repo == l.rukas {
                build = &rukas_at;
            }
            built = "unknown";
            if !build.is_empty() {
                built = in_build(&r.repo, &fix, build);
            }
        }
        let text = body::comment(
            r,
            &Said {
                model: &l.model,
                product: &l.product,
                links: self.links.get(&r.cycle).map_or("", String::as_str),
                rukas: &rukas_at,
                image: &image_at,
                fix: &fix,
                build,
                in_build: built,
            },
        );
        let file = self.dir.join("body.md");
        let file_name = file.display().to_string();
        Ok(fs::write(&file, text).is_ok()
            && gh_quiet(&[
                "issue",
                "comment",
                &r.number,
                "--repo",
                &r.repo,
                "--body-file",
                &file_name,
            ]))
    }

    fn go(
        &mut self,
        stack: &[J],
        rooms: &BTreeMap<String, i64>,
        cap: usize,
        left: usize,
        ask: &str,
    ) -> u8 {
        let stopped = match self.analyse(ask, cap) {
            Ok(stopped) => stopped,
            Err(rc) => return rc,
        };
        let why = stopped.as_ref().map_or("", |(_, why)| why.as_str());
        // A link would have this program read a file of the analyst's choosing.
        let found = self.dir.join("out/findings.json");
        let meta = fs::symlink_metadata(&found);
        if !meta.as_ref().is_ok_and(|m| m.is_file()) {
            return self.blame(&format!("{why}the analyst left no findings.json"));
        }
        if meta.is_ok_and(|m| m.len() > FINDINGS_BYTES) {
            return self.blame(&format!("{why}findings.json is over 1 MB"));
        }
        let doc = fs::read(&found)
            .ok()
            .and_then(|b| J::parse(&String::from_utf8_lossy(&b)));
        let l = self.l;
        let plan = doc.as_ref().and_then(|doc| {
            check::plan(
                doc,
                &Given {
                    cycles: &self.recs,
                    stack,
                    rooms,
                    repo: &l.repo,
                    rukas: &l.rukas,
                    label: LABEL,
                    skip_rukas: &l.skip_rukas,
                },
            )
        });
        let (Some(doc), Some(plan)) = (doc, plan) else {
            return self.blame(&format!("{why}findings.json is not the shape asked for"));
        };
        if plan.skipped != 0 {
            self.notes.push(format!("skipped={}", plan.skipped));
        }
        if let Some((exit, _)) = &stopped {
            self.return_unread(&doc, *exit);
        }
        if let Err(rc) = self.file(&plan) {
            return rc;
        }
        let mut rechecked = 0;
        for r in &plan.recheck {
            match self.recheck(r, stack) {
                Ok(true) => rechecked += 1,
                Ok(false) => self
                    .notes
                    .push(format!("not-commented={}#{}", r.repo, r.number)),
                Err(rc) => return rc,
            }
        }
        remove(&self.dir);
        if left > 0 {
            self.notes.push(format!("left={left}"));
        }
        let notes = if self.notes.is_empty() {
            String::new()
        } else {
            format!(" {}", self.notes.join(" "))
        };
        say(&format!(
            "cycles={} findings={} invalid={} duplicates={} of={} over={} filed={} issues={} rechecked={rechecked}{notes}",
            self.recs.len(),
            plan.found,
            plan.invalid,
            plan.duplicates,
            if plan.of.is_empty() { "none" } else { &plan.of },
            plan.over,
            self.filed.len(),
            self.issues(),
        ));
        0
    }
}

/// Is there a run to make?
pub fn due(prompt: &str) -> u8 {
    let l = match Loop::from_env(prompt) {
        Ok(l) => l,
        Err(rc) => return rc,
    };
    match l.prepare() {
        Ok(_) => 0,
        Err(Stop::Quiet) => 10,
        Err(Stop::Code(rc)) => rc,
    }
}

pub fn run(prompt: &str) -> u8 {
    let l = match Loop::from_env(prompt) {
        Ok(l) => l,
        Err(rc) => return rc,
    };
    match l.prepare() {
        Ok(ready) => l.go(ready),
        Err(Stop::Quiet) => 0,
        Err(Stop::Code(rc)) => rc,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dir(PathBuf);

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn dir(name: &str) -> Dir {
        let d = env::temp_dir().join(format!("improve-run-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        Dir(d)
    }

    #[test]
    fn a_stamp_is_the_time_a_run_began() {
        // 2026-10-10T20:01:40Z
        let t = 1_791_662_500;
        assert_eq!(time::iso(t), "2026-10-10T20:01:40Z");
        assert_eq!(stamp_of(t), "261010-200140");
        assert_eq!(month_ago(t), "2026-09-10");
    }

    #[test]
    fn the_oldest_go_first_by_time_and_not_by_issue() {
        let recs = [
            "9-261001-000000",
            "1-261003-000000",
            "2-261002-120000",
            "30-261002-000000",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(
            oldest(recs, 3),
            ["9-261001-000000", "30-261002-000000", "2-261002-120000"]
        );
    }

    #[test]
    fn a_cap_is_a_digit_from_one_to_five() {
        let d = dir("cap");
        let f = d.0.join("cap");
        assert_eq!(cap_of(&f), 5);
        for (text, cap) in [
            ("1\n", 1),
            ("4", 4),
            ("3\n\n", 3),
            ("5\n", 5),
            ("0\n", 5),
            ("6\n", 5),
            ("x\n", 5),
            ("\n", 5),
            ("12\n", 5),
            ("3\n4\n", 5),
            ("3; rm -rf /\n", 5),
        ] {
            fs::write(&f, text).unwrap();
            assert_eq!(cap_of(&f), cap, "{text:?}");
        }
    }

    #[test]
    fn the_cap_halves_at_the_bound_and_doubles_in_time() {
        assert_eq!(next_cap(0, 1, 1), 2);
        assert_eq!(next_cap(0, 4, 4), 5);
        assert_eq!(next_cap(124, 5, 5), 2);
        assert_eq!(next_cap(137, 5, 4), 2);
        assert_eq!(next_cap(124, 5, 3), 1);
        assert_eq!(next_cap(124, 1, 1), 1);
        assert_eq!(next_cap(3, 4, 4), 4);
    }

    #[test]
    fn a_record_says_what_the_cycle_ran() {
        let d = dir("ran");
        let commit = "0123456789abcdef0123456789abcdef01234567";
        let image = format!("sha256:{}", "e".repeat(64));
        for (text, want) in [
            (
                format!(r#"{{"buildCommit":"abcdef0","rukas":"{commit}","image":"{image}"}}"#),
                ("abcdef0".to_string(), image.clone()),
            ),
            (
                format!(r#"{{"buildCommit":"nope","rukas":"{commit}","image":"sha256:e"}}"#),
                (commit.to_string(), String::new()),
            ),
            (r#"{"rukas":7}"#.to_string(), (String::new(), String::new())),
            ("[]".to_string(), (String::new(), String::new())),
            ("not json".to_string(), (String::new(), String::new())),
        ] {
            fs::write(d.0.join("cycle.json"), &text).unwrap();
            assert_eq!(ran(&d.0), want, "{text}");
        }
        assert_eq!(ran(&d.0.join("none")), (String::new(), String::new()));
    }

    #[test]
    fn look_keeps_records_and_removes_other_directories_but_not_hidden_ones() {
        let d = dir("look");
        for name in ["7-261010-200140", "odd", ".hidden"] {
            fs::create_dir_all(d.0.join(name)).unwrap();
        }
        fs::write(d.0.join("a-file"), "").unwrap();
        assert_eq!(look(&d.0), ["7-261010-200140"]);
        assert!(!d.0.join("odd").exists());
        assert!(d.0.join(".hidden").exists());
        assert!(d.0.join("a-file").exists());
    }

    #[test]
    fn give_back_returns_records_and_replaces_what_is_there() {
        let d = dir("back");
        let (run, window) = (d.0.join("run-1"), d.0.join("window"));
        let cycle = run.join("in/cycles/7-261010-200140");
        fs::create_dir_all(&cycle).unwrap();
        fs::write(cycle.join("cycle.json"), "new").unwrap();
        fs::create_dir_all(window.join("7-261010-200140")).unwrap();
        fs::write(window.join("7-261010-200140/old"), "").unwrap();
        give_back(&run, &window);
        assert!(!run.exists());
        assert!(!window.join("7-261010-200140/old").exists());
        assert_eq!(
            fs::read_to_string(window.join("7-261010-200140/cycle.json")).unwrap(),
            "new"
        );
    }

    #[test]
    fn a_copy_that_cannot_be_written_is_no_copy() {
        let d = dir("mask");
        let raw = d.0.join("work.jsonl");
        fs::write(&raw, "PASSWORD=hunter2-secret\n").unwrap();
        fs::write(d.0.join("work.jsonl.err"), "stderr line\n").unwrap();
        let to = d.0.join("masked");
        let text = masked_copy(&raw, &to).unwrap();
        assert_eq!(text, "PASSWORD=***\nstderr line\n");
        assert_eq!(fs::read_to_string(&to).unwrap(), text);
        assert_eq!(masked_copy(&raw, &d.0.join("no/such/dir")), None);
    }

    #[test]
    fn a_log_that_is_gone_masks_as_nothing_and_a_missing_err_is_no_matter() {
        let d = dir("nolog");
        assert_eq!(
            masked_copy(&d.0.join("none"), &d.0.join("m")).as_deref(),
            Some("")
        );
    }
}
