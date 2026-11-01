//! `tehas-cycle watch`: the end of tehas work. It starts the container it is
//! given after `--`, keeps the cycle's console and heartbeat, stops a cycle that
//! stalls or passes its time limit, and leaves the cycle's notes for tehasbot
//! cycle. libexec/work execs it, so it holds the issue's lock as fd 9 and the
//! docker client inherits it. What each file proves is in lib/cycles.sh, and the
//! exit statuses in the header of libexec/work.
//!
//! The two bounds of the loop are counted on [`crate::clock`], not the wall
//! clock, so a VM save and restore stops no cycle. Tests set TEHAS_WATCH_TICK
//! (seconds of one look and of the wait for the console's last lines) and
//! TEHAS_WATCH_CLOCK (a program that answers the `clock` verbs).

use crate::clock;
use crate::{host, progress, runner};
use serde_json::Value;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tehas_core::{call, time};

/// How often a stop is looked for while something else is waited on.
const BEAT: Duration = Duration::from_millis(50);
/// Lines of the driver kept of a stream, and the characters of each.
const DIAG_LINES: usize = 20;
const DIAG_WIDTH: usize = 1000;
/// The runner's own ends.
const ENDS: [&str; 7] = ["0", "10", "11", "20", "21", "30", "40"];

/// What libexec/work gives the watch.
pub struct Given {
    pub issue: u64,
    pub attempt: String,
    pub log: PathBuf,
    pub runs: PathBuf,
    pub name: String,
    pub limit: u64,
    pub stall: u64,
    pub grace: u64,
    pub stop_wait: u64,
    pub console_wait: u64,
    pub console: PathBuf,
    pub models: String,
    pub email: String,
    pub renewer: String,
    /// The whole command that starts the container.
    pub docker: Vec<String>,
}

/// The cycle's clock: the boot's, or the test's program.
pub enum Clock {
    Boot,
    Program(String),
}

impl Clock {
    fn from_env() -> Clock {
        match std::env::var("TEHAS_WATCH_CLOCK") {
            Ok(p) if !p.is_empty() => Clock::Program(p),
            _ => Clock::Boot,
        }
    }

    fn asked(p: &str, words: &[&str]) -> Option<String> {
        let out = Command::new(p)
            .arg("clock")
            .args(words)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    fn now(&self) -> Option<u64> {
        match self {
            Clock::Boot => clock::boot_seconds(),
            Clock::Program(p) => Clock::asked(p, &["now"])?.parse().ok(),
        }
    }

    fn mark(&self, file: &Path) -> bool {
        match self {
            Clock::Boot => clock::mark(file, clock::boot_seconds).is_ok(),
            Clock::Program(p) => {
                let f = file.to_string_lossy();
                Clock::asked(p, &["mark", &f]).is_some()
            }
        }
    }

    /// The word of the check. Only `stalled` and `over` end a cycle.
    fn check(&self, mark: &Path, limit: u64, stall: u64, since: u64) -> String {
        match self {
            Clock::Boot => clock::check(mark, limit, stall, since, clock::boot_seconds)
                .word()
                .to_string(),
            Clock::Program(p) => {
                let (m, l, s, n) = (
                    mark.to_string_lossy(),
                    limit.to_string(),
                    stall.to_string(),
                    since.to_string(),
                );
                let words = ["check", &m, "--limit", &l, "--stall", &s, "--since", &n];
                Clock::asked(p, &words).unwrap_or_default()
            }
        }
    }
}

/// One look of the loop and the wait for the console's last lines.
fn ticks() -> (Duration, Duration) {
    let seam = std::env::var("TEHAS_WATCH_TICK").ok();
    match seam.and_then(|t| t.parse::<f64>().ok()) {
        Some(t) if t > 0.0 && t < 60.0 => {
            let t = Duration::from_secs_f64(t);
            (t, t)
        }
        _ => (Duration::from_secs(10), Duration::from_secs(3)),
    }
}

fn say(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

fn append(file: &Path, line: &[u8]) -> bool {
    let done = OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)
        .and_then(|mut f| f.write_all(line));
    done.is_ok()
}

fn signal(pid: &str, word: &str) {
    let _ = Command::new("kill")
        .args([word, "--", pid])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn exit_code(s: ExitStatus) -> i32 {
    s.code()
        .or_else(|| s.signal().map(|n| 128 + n))
        .unwrap_or(2)
}

/// A child that may have ended, its status kept once it has.
struct Running {
    child: Option<Child>,
    rc: Option<i32>,
}

impl Running {
    fn gone(rc: i32) -> Running {
        Running {
            child: None,
            rc: Some(rc),
        }
    }

    fn alive(&mut self) -> bool {
        if self.rc.is_some() {
            return false;
        }
        match self.child.as_mut().map(Child::try_wait) {
            Some(Ok(None)) => true,
            Some(Ok(Some(s))) => {
                self.rc = Some(exit_code(s));
                false
            }
            _ => {
                self.rc = Some(127);
                false
            }
        }
    }
}

/// `read -r` of a line: its spaces and tabs off both ends.
fn field(line: &str) -> String {
    line.trim_matches([' ', '\t']).to_string()
}

/// The step and the notice from the two lines the console keeps beside its
/// file. A second line counts only after a first that was ended.
fn now_lines(file: &Path) -> (String, String) {
    let Ok(bytes) = fs::read(file) else {
        return (String::new(), String::new());
    };
    let text = String::from_utf8_lossy(&bytes);
    match text.split_once('\n') {
        Some((a, rest)) => (field(a), field(rest.split('\n').next().unwrap_or(""))),
        None => (field(&text), String::new()),
    }
}

/// The newest change to any file under `paths`, in whole seconds of mtime. Links
/// are not followed. Transcripts of an earlier cycle are older than this one's
/// start.
pub fn newest(paths: &[&Path]) -> Option<i64> {
    fn walk(p: &Path, best: &mut Option<i64>) {
        let Ok(m) = fs::symlink_metadata(p) else {
            return;
        };
        if m.is_file() {
            *best = Some(best.map_or(m.mtime(), |b: i64| b.max(m.mtime())));
        } else if m.is_dir() {
            for e in fs::read_dir(p).into_iter().flatten().flatten() {
                walk(&e.path(), best);
            }
        }
    }
    let mut best = None;
    for p in paths {
        walk(p, &mut best);
    }
    best
}

/// One number of a cgroup's event file, as its key says it.
pub fn event(text: &str, key: &str) -> Option<u32> {
    let mut found = text.lines().filter_map(|l| {
        let (k, v) = l.trim().split_once([' ', '\t'])?;
        let v = v.trim();
        (k == key && !v.is_empty() && v.len() < 10 && v.bytes().all(|b| b.is_ascii_digit()))
            .then(|| v.parse().ok())
            .flatten()
    });
    found.next_back()
}

/// The kills of the kernel log named for this container.
pub fn kills_in(log: &str, id: &str) -> u32 {
    let scope = format!("task_memcg=/tehas.slice/docker-{id}.scope,");
    let lines = log.lines();
    lines
        .filter(|l| l.starts_with("oom-kill:") && l.contains(&scope))
        .count() as u32
}

/// The driver's own words for a refusal, as they stand in the cycle's stream:
/// the warning, kill, failure and cancellation lines. The last 20, each cut to
/// 1000 bytes, for a line of a stream can carry an error tail of any size and
/// this goes into the record the analyst reads. Empty when there is none.
pub fn diag(stream: &[u8]) -> Vec<u8> {
    let has = |l: &[u8], w: &[u8]| l.windows(w.len()).any(|x| x == w);
    let lines: Vec<&[u8]> = stream
        .split(|b| *b == b'\n')
        .filter(|l| has(l, b"[pi-rukas]"))
        .filter(|l| {
            let words: [&[u8]; 4] = [b"WARN", b"killed", b"FAILED", b"cancelled"];
            words.iter().any(|w| has(l, w))
        })
        .collect();
    let last = &lines[lines.len().saturating_sub(DIAG_LINES)..];
    let mut out = vec![];
    for l in last {
        out.extend_from_slice(&l[..l.len().min(DIAG_WIDTH)]);
        out.push(b'\n');
    }
    out
}

/// What the watch saw of the container's end.
pub struct Seen<'a> {
    pub rc: i32,
    /// The issue's entry of the result; None when the runner wrote none.
    pub outcome: Option<&'a Value>,
    pub error_only: bool,
    pub interrupted: bool,
    pub stalled: bool,
    pub timed: bool,
    /// This command stopped a container that was still running.
    pub stopped: bool,
    /// The time limit had passed when the container ended.
    pub at_limit: bool,
    /// The container's stream holds nothing.
    pub log_empty: bool,
}

/// What that end means.
#[derive(Debug, PartialEq, Eq)]
pub struct Decided {
    pub status: String,
    /// The runner stopped itself at the time limit and named no end.
    pub ran_out: bool,
    pub killed: bool,
    /// No result and no line of its stream: `.nostart`.
    pub nostart: bool,
}

fn text(v: Option<&Value>, key: &str) -> String {
    let got = v.and_then(|v| v.get(key)).and_then(Value::as_str);
    got.unwrap_or("").to_string()
}

fn stopped_word(status: &str) -> bool {
    matches!(status, "running" | "unknown" | "interrupted")
}

pub fn decide(s: &Seen) -> Decided {
    let mut status = "running".to_string();
    match s.outcome {
        Some(_) => {
            status = text(s.outcome, "status");
            if status.is_empty() {
                status = "unknown".to_string();
            }
            // The runner ends a cycle stopped here as interrupted.
            if s.stalled && stopped_word(&status) {
                status = "stalled".to_string();
            }
            if s.timed && stopped_word(&status) {
                status = "timeout".to_string();
            }
        }
        None => {
            if s.interrupted {
                status = "interrupted".to_string();
            }
            if s.stalled {
                status = "stalled".to_string();
            }
            if s.timed {
                status = "timeout".to_string();
            }
        }
    }
    // The runner stops itself at the --timeout it is given. Its result then
    // names no cap and no end, so the limit ended the cycle as surely as a stop.
    let mut ran_out = false;
    if !s.timed
        && !s.stalled
        && !s.interrupted
        && s.rc != 137
        && s.at_limit
        && text(s.outcome, "cap").is_empty()
        && stopped_word(&status)
    {
        ran_out = true;
        status = "timeout".to_string();
    }
    let have = s.outcome.is_some();
    let quiet = !have && !s.error_only && !s.interrupted && !s.stopped;
    // 137 is this cycle's end only when nobody here stopped it and the runner
    // wrote no result.
    let killed = s.rc == 137 && quiet;
    if killed {
        status = "killed".to_string();
    }
    let nostart = quiet
        && !killed
        && !s.timed
        && !ran_out
        && !s.stalled
        && s.rc != 137
        && (1..=999).contains(&s.rc)
        && s.log_empty;
    Decided {
        status,
        ran_out,
        killed,
        nostart,
    }
}

/// The command's exit status. The kill is what this command saw of the
/// container; a runner that wrote its result says how the cycle ended.
pub fn exit_of(s: &Seen, d: &Decided) -> i32 {
    if d.killed {
        return 137;
    }
    if s.stalled || s.timed {
        return 2;
    }
    let own = s.rc.to_string();
    let mut codes = vec![own];
    if let Some(Value::Number(n)) = s.outcome.and_then(|o| o.get("exitCode")) {
        codes.push(n.to_string());
    }
    if s.outcome.is_none() {
        if !s.error_only {
            return 2;
        }
        codes.truncate(1);
    }
    codes
        .iter()
        .find(|c| ENDS.contains(&c.as_str()))
        .and_then(|c| c.parse().ok())
        .unwrap_or(2)
}

/// The step the result names, or nothing.
fn result_step(outcome: Option<&Value>) -> String {
    let step = text(outcome, "failedStep");
    if step.is_empty() {
        text(outcome, "lastStep")
    } else {
        step
    }
}

/// The step said for a stop: the result's, else the console's first line, as
/// plain characters. "-" when neither names one.
fn inflight(outcome: Option<&Value>, now: &Path) -> String {
    let mut step = result_step(outcome);
    if step.is_empty() {
        if let Ok(bytes) = fs::read(now) {
            let text = String::from_utf8_lossy(&bytes);
            if let Some((first, _)) = text.split_once('\n') {
                step = field(first);
            }
        }
    }
    let keep = |c: char| c.is_ascii_alphanumeric() || "_.:-".contains(c);
    let step = runner::cut(&runner::kept(&step, keep), 40);
    if step.is_empty() {
        "-".to_string()
    } else {
        step
    }
}

struct Files {
    cid: PathBuf,
    oom: PathBuf,
    forks: PathBuf,
    stalled: PathBuf,
    timed: PathBuf,
    result: PathBuf,
    progress: PathBuf,
    runner_exit: PathBuf,
    status: PathBuf,
    nostart: PathBuf,
    clock: PathBuf,
}

impl Files {
    fn of(home: &Path, issue: u64) -> Files {
        let f = |name: &str| home.join(format!("state/cycles/{issue}.{name}"));
        Files {
            cid: f("cid"),
            oom: f("oom"),
            forks: f("forks"),
            stalled: f("stalled"),
            timed: f("timed"),
            result: f("result.json"),
            progress: f("progress.json"),
            runner_exit: f("runner_exit"),
            status: f("status"),
            nostart: f("nostart"),
            clock: f("clock"),
        }
    }
}

/// The name beside `p` with `end` added to it.
fn beside(p: &Path, end: &str) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(end);
    PathBuf::from(s)
}

struct Watch<'a> {
    home: &'a Path,
    g: &'a Given,
    files: Files,
    err: PathBuf,
    console_now: PathBuf,
    stop: Arc<AtomicBool>,
    interrupted: bool,
    console: Option<Child>,
    live_on: bool,
    beat_said: bool,
    publish_said: bool,
    ooms: u32,
    forks: u32,
    oom_id: String,
}

impl Watch<'_> {
    fn stop_container(&self, grace: u64) {
        let name = &self.g.name;
        let bound = Duration::from_secs(self.g.stop_wait);
        let five = Duration::from_secs(5);
        let args: Vec<String> = ["stop", "-t", &grace.to_string(), name]
            .iter()
            .map(|s| s.to_string())
            .collect();
        match call::run_grace("docker", &args, None, bound, five) {
            None | Some(124 | 137) => {}
            _ => return,
        }
        say(&format!(
            "tehas work: docker stop of {name} did not end within {}s, killing it",
            self.g.stop_wait
        ));
        let args = vec!["kill".to_string(), name.clone()];
        if call::run_grace("docker", &args, None, bound, five) != Some(0) {
            say(&format!(
                "tehas work: docker kill of {name} did not work either"
            ));
        }
    }

    /// A TERM or INT that came: the console and the renewer go, the container
    /// is stopped and the cycle said to be interrupted. True when there was one.
    fn signalled(&mut self) -> bool {
        if !self.stop.swap(false, Ordering::SeqCst) {
            return false;
        }
        self.interrupted = true;
        if let Some(c) = &self.console {
            signal(&c.id().to_string(), "-TERM");
        }
        if !self.g.renewer.is_empty() {
            signal(&self.g.renewer, "-TERM");
        }
        self.stop_container(10);
        if self.live_on {
            self.now_file(false, "interrupted");
        }
        true
    }

    /// The step and status on the store, which publishes the live view.
    fn now_file(&mut self, running: bool, status: &str) {
        let (mut step, note) = now_lines(&self.console_now);
        if step.is_empty() {
            step = "starting".to_string();
        }
        let n = self.g.issue.to_string();
        let mut args = vec!["cycle-heartbeat", &n];
        let attempt = &self.g.attempt;
        if attempt_shaped(attempt) {
            args.extend(["--attempt", attempt]);
        }
        args.extend([if running { "--running" } else { "--ended" }]);
        args.extend(["--status", status, "--step", &step, "--text", &note]);
        if let Err(r) = host::store_said(self.home, &args) {
            if !self.beat_said {
                eprintln!(
                    "tehas work: the heartbeat of #{n} is not on record ({}): {}",
                    r.word, r.line
                );
                self.beat_said = true;
            }
        }
        // One try: the next tick publishes again, and a stop must not wait on it.
        if let Err(r) = host::store_said(self.home, &["publish"]) {
            if !self.publish_said {
                eprintln!(
                    "tehas work: the live view is not published ({}): {}",
                    r.word, r.line
                );
                self.publish_said = true;
            }
        }
    }

    /// The memory kills and refused starts in this container, from its cgroup.
    /// The last numbers seen are kept.
    fn oom_look(&mut self) {
        let cid = fs::read_to_string(&self.files.cid).unwrap_or_default();
        let id = field(cid.lines().next().unwrap_or(""));
        if id.len() != 64 || !id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            self.oom_id.clear();
            return;
        }
        let root = std::env::var("TEHAS_CGROUP_ROOT").unwrap_or_else(|_| "/sys/fs/cgroup".into());
        let scope = Path::new(&root).join(format!("tehas.slice/docker-{id}.scope"));
        self.oom_id = id;
        let read = |f: &str| fs::read_to_string(scope.join(f)).unwrap_or_default();
        if let Some(n) = event(&read("memory.events"), "oom_kill") {
            self.ooms = n;
        }
        if let Some(n) = event(&read("pids.events"), "max") {
            self.forks = n;
        }
    }

    /// The kills since the last look went with the cgroup. The kernel log names
    /// the cgroup of each kill but rate-limits those lines, so the larger count
    /// stands.
    fn oom_log(&mut self, started: i64) {
        if self.oom_id.is_empty() {
            return;
        }
        let out = Command::new("timeout")
            .args(["-k", "5", &self.g.stop_wait.to_string(), "journalctl"])
            .args(["-k", "-q", "-o", "cat", "--no-pager"])
            .args(["--since", &format!("@{started}")])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output();
        if let Ok(o) = out {
            let n = kills_in(&String::from_utf8_lossy(&o.stdout), &self.oom_id);
            if o.status.success() && n > self.ooms {
                self.ooms = n;
            }
        }
    }

    /// One file of the protocol, whole or not at all.
    fn write_note(&self, file: &Path, words: &str) -> bool {
        let new = beside(file, ".new");
        let wrote = fs::write(&new, format!("{words}\n")).is_ok();
        if wrote && fs::rename(&new, file).is_ok() {
            return true;
        }
        let _ = fs::remove_file(&new);
        let line = format!("tehas work: could not write {}", file.display());
        eprintln!("{line}");
        if !append(&self.err, format!("{line}\n").as_bytes()) {
            eprintln!(
                "tehas work: could not say of {} that {} was lost",
                self.err.display(),
                file.display()
            );
        }
        false
    }

    fn write_or_say(&self, file: &Path, words: &str) {
        if fs::write(file, words).is_err() {
            eprintln!("tehas work: could not write {}", file.display());
        }
    }

    fn append_or_say(&self, line: &[u8]) {
        if !append(&self.err, line) {
            eprintln!("tehas work: could not write {}", self.err.display());
        }
    }

    /// Waits for the time of one look, a stop ending it early.
    fn tick(&mut self, length: Duration) {
        let began = Instant::now();
        while began.elapsed() < length {
            if self.stop.load(Ordering::SeqCst) {
                return;
            }
            std::thread::sleep(BEAT.min(length));
        }
    }

    /// The console finishes its pass before it stops. A slow one must not hold
    /// the issue's lock and its slot.
    fn end_console(&mut self) {
        let Some(mut c) = self.console.take() else {
            return say_console(self.g, 127);
        };
        signal(&c.id().to_string(), "-TERM");
        let bound = Duration::from_secs(self.g.console_wait);
        let began = Instant::now();
        let mut status = None;
        while began.elapsed() < bound {
            status = c.try_wait().ok().flatten();
            if status.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let rc = match status {
            Some(s) => exit_code(s),
            None => {
                say(&format!(
                    "tehas work: the console did not stop within {}s, killing it",
                    self.g.console_wait
                ));
                let _ = c.kill();
                c.wait().map(exit_code).unwrap_or(137)
            }
        };
        if rc != 0 {
            say_console(self.g, rc);
        }
    }
}

fn say_console(g: &Given, rc: i32) {
    let file = beside(&g.log, ".console");
    say(&format!(
        "tehas work: the console ended with {rc}, see {}",
        file.display()
    ));
}

fn attempt_shaped(a: &str) -> bool {
    (1..=9).contains(&a.len()) && a.bytes().all(|b| b.is_ascii_digit()) && !a.starts_with('0')
}

/// The container, with its result and stream where they go. A command that
/// cannot be run leaves a line in the stream, as a shell does.
fn start_container(g: &Given, files: &Files) -> Running {
    let (Ok(out), Ok(err)) = (File::create(&files.result), File::create(&g.log)) else {
        return Running::gone(1);
    };
    let Some((program, args)) = g.docker.split_first() else {
        return Running::gone(127);
    };
    let keep = err.try_clone();
    let child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .spawn();
    match child {
        Ok(c) => Running {
            child: Some(c),
            rc: None,
        },
        Err(e) => {
            if let Ok(mut f) = keep {
                let _ = writeln!(f, "{program}: {e}");
            }
            Running::gone(127)
        }
    }
}

fn start_console(g: &Given, live: &Path, started: i64, console_now: &Path) -> Option<Child> {
    let out = live.join(format!("console-{}.txt", g.issue));
    let mut args: Vec<String> = vec![
        "--progress".into(),
        g.log.to_string_lossy().into_owned(),
        "--runs".into(),
        g.runs.to_string_lossy().into_owned(),
        "--out".into(),
        out.to_string_lossy().into_owned(),
        "--since".into(),
        (started - 5).to_string(),
        "--now".into(),
        console_now.to_string_lossy().into_owned(),
    ];
    for name in tehas_core::mask::host_names(&g.models, &g.email) {
        args.extend(["--private".to_string(), name]);
    }
    let err = File::create(beside(&g.log, ".console")).ok()?;
    Command::new(&g.console)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(err)
        .spawn()
        .ok()
}

/// A cycle's whole watch, its exit status at the end.
pub fn run(home: &Path, g: &Given) -> i32 {
    let stop = Arc::new(AtomicBool::new(false));
    for sig in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        if signal_hook::flag::register(sig, Arc::clone(&stop)).is_err() {
            eprintln!("tehas work: cannot listen for signal {sig}");
            return 2;
        }
    }
    let live = home.join("live");
    let console_now = live.join(format!("console-{}.txt.now", g.issue));
    let mut w = Watch {
        home,
        g,
        files: Files::of(home, g.issue),
        err: beside(&g.log, ".err"),
        console_now,
        stop,
        interrupted: false,
        console: None,
        live_on: false,
        beat_said: false,
        publish_said: false,
        ooms: 0,
        forks: 0,
        oom_id: String::new(),
    };
    let (tick, settle) = ticks();
    let clock = Clock::from_env();

    let mut docker = start_container(g, &w.files);
    let started = time::now();
    if !clock.mark(&w.files.clock) {
        say(&format!(
            "tehas work: no cycle clock for #{}: its limits wait on the clock",
            g.issue
        ));
    }
    say(&format!(
        "tehas work: issue {} started, log {}",
        g.issue,
        g.log.display()
    ));

    // The console's file and the step it keeps beside it start empty: stale
    // lines of a cycle before this one are no better than none.
    let _ = fs::create_dir_all(&live);
    let _ = File::create(live.join(format!("console-{}.txt", g.issue)));
    let _ = fs::remove_file(&w.console_now);
    w.console = start_console(g, &live, started, &w.console_now);
    w.live_on = true;
    w.now_file(true, "running");

    let (mut stalled, mut timed, mut stopped) = (false, false, false);
    let mut seen = started;
    let mut last = clock.now();
    while !w.interrupted && docker.alive() {
        w.tick(tick);
        w.signalled();
        if w.interrupted {
            break;
        }
        w.oom_look();
        w.now_file(true, "running");
        if let Some(m) = newest(&[&g.log, &g.runs]) {
            if m > seen {
                seen = m;
                last = clock.now();
            }
        }
        // A clock that says nothing, says ok or says unknown stops no cycle.
        let verdict =
            last.map(|since| clock.check(&w.files.clock, g.limit + g.grace, g.stall, since));
        match verdict.as_deref() {
            Some("over") => {
                say(&format!("tehas work: time limit of {}s passed", g.limit));
                timed = true;
                break;
            }
            Some("stalled") => {
                say(&format!(
                    "tehas work: none of the cycle's logs changed for {}s, stopping it as stalled",
                    g.stall
                ));
                stalled = true;
                break;
            }
            _ => {}
        }
    }

    // A stop that came while the container was still being made found nothing
    // to stop.
    if w.interrupted && docker.alive() {
        w.stop_container(10);
    }
    if !w.interrupted && docker.alive() {
        w.oom_look();
        stopped = true;
        w.stop_container(20);
    }
    while docker.alive() {
        w.signalled();
        std::thread::sleep(BEAT);
    }
    w.signalled();
    let rc = docker.rc.unwrap_or(2);
    if !g.renewer.is_empty() {
        signal(&g.renewer, "-TERM");
    }
    w.oom_look();
    let _ = fs::remove_file(&w.files.cid);
    w.oom_log(started);

    let result = fs::read(&w.files.result).ok();
    let result = result.map(|b| String::from_utf8_lossy(&b).into_owned());
    let outcome = result.as_deref().and_then(|t| runner::outcome(t, g.issue));
    let error_only = outcome.is_none() && result.as_deref().is_some_and(progress::error_only);
    // What the runner ended with, whatever this command goes on to exit with:
    // what the report makes of the run after this point may fail, and this word
    // must not go with it.
    w.write_note(&w.files.runner_exit, &rc.to_string());
    let log_empty = fs::metadata(&g.log).map_or(true, |m| m.len() == 0);
    let seen_end = Seen {
        rc,
        outcome: outcome.as_ref(),
        error_only,
        interrupted: w.interrupted,
        stalled,
        timed,
        stopped,
        at_limit: time::now() - started >= g.limit as i64,
        log_empty,
    };
    let decided = decide(&seen_end);
    let status = decided.status.clone();
    let step_in_flight = inflight(outcome.as_ref(), &w.console_now);
    if timed || decided.ran_out {
        // The limit, who stopped the cycle and the step it was in.
        let by = if decided.ran_out { "runner" } else { "tehas" };
        let words = format!("{} {by} {step_in_flight}\n", g.limit);
        w.write_or_say(&w.files.timed, &words);
    }
    if stalled {
        let words = format!("{}\n{step_in_flight}\n", g.stall);
        w.write_or_say(&w.files.stalled, &words);
    }
    if decided.killed {
        // The only account of the end when the runner wrote no result: lost, it
        // is said of twice.
        if !w.write_note(&w.files.status, "killed") {
            say("tehas work: the account of the killed container is lost with it");
        }
        say("tehas work: the container was killed (137) while the cycle was running: out of memory, or stopped from outside");
    }
    if decided.nostart {
        if !w.write_note(&w.files.nostart, &rc.to_string()) {
            say("tehas work: the account of the container that never started is lost with it");
        }
        let keep = |c: char| c.is_ascii_alphanumeric() || "._-".contains(c);
        let name = g.log.file_name().unwrap_or_default().to_string_lossy();
        let name = runner::kept(&name, keep);
        let line = format!(
            "tehas work: issue {} ended with the container's exit {rc} and no line of its stream: the runner wrote no event, so no step started. Its preflight is {name}.preflight.\n",
            g.issue
        );
        w.append_or_say(line.as_bytes());
    }
    if w.ooms > 0 {
        say(&format!(
            "tehas work: {} processes in this cycle's container were killed for memory",
            w.ooms
        ));
        w.write_or_say(&w.files.oom, &format!("{}\n", w.ooms));
    }
    if w.forks > 0 {
        say(&format!(
            "tehas work: this cycle's container reached its limit of processes, {} starts were refused",
            w.forks
        ));
        w.write_or_say(&w.files.forks, &format!("{}\n", w.forks));
    }
    // Let the console catch the last transcript lines.
    std::thread::sleep(settle);
    w.end_console();

    let mut step = result_step(outcome.as_ref());
    if step.is_empty() {
        step = "-".to_string();
    }
    if step == "-" && (timed || stalled || decided.ran_out) {
        step = step_in_flight;
    }
    w.now_file(false, &status);
    let _ = fs::remove_file(&w.console_now);
    let _ = fs::copy(&w.files.result, beside(&g.log, ".result.json"));
    let summary = progress::summary(&g.log, Some((&w.files.result, g.issue)));
    w.write_or_say(&w.files.progress, &format!("{summary}\n"));
    // A stop this command made is its own cap, unless the runner had ended first.
    let mut cap = text(outcome.as_ref(), "cap");
    match status.as_str() {
        "timeout" => cap = "wall-clock".to_string(),
        "stalled" => cap = "stall".to_string(),
        _ => {}
    }
    say(&format!(
        "tehas work: issue={} status={status} step={step} cap={} runner_exit={rc} seconds={}",
        g.issue,
        if cap.is_empty() { "-" } else { &cap },
        time::now() - started
    ));
    let code = exit_of(&seen_end, &decided);
    let empty_err = fs::metadata(&w.err).map_or(true, |m| m.len() == 0);
    if !decided.killed
        && !stalled
        && !timed
        && outcome.is_some()
        && empty_err
        && (!cap.is_empty() || status == "aborted")
    {
        tell_the_end(&w, &cap);
    }
    code
}

/// What reaches err.txt of the cycle's improvement record for a cycle that ended
/// at a cap or was aborted: an empty one there reads as nothing happened.
fn tell_the_end(w: &Watch, cap: &str) {
    let n = w.g.issue;
    let progress = fs::read_to_string(&w.files.progress).ok();
    let reason = runner::progress_fields(progress.as_deref())[1].clone();
    let line = if cap.is_empty() {
        format!("tehas work: issue {n} ended as aborted with no cap\n")
    } else if reason.is_empty() {
        format!("tehas work: issue {n} ended at the cap {cap}\n")
    } else {
        format!("tehas work: issue {n} ended at the cap {cap}: {reason}\n")
    };
    w.append_or_say(line.as_bytes());
    let lines = diag(&fs::read(&w.g.log).unwrap_or_default());
    if !lines.is_empty() && !append(&w.err, &lines) {
        eprintln!("tehas work: could not write {}", w.err.display());
    }
    let name = w.g.log.file_name().unwrap_or_default().to_string_lossy();
    let line = format!("tehas work: the driver's diagnostics are in {name}\n");
    w.append_or_say(line.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn seen(rc: i32) -> Seen<'static> {
        Seen {
            rc,
            outcome: None,
            error_only: false,
            interrupted: false,
            stalled: false,
            timed: false,
            stopped: false,
            at_limit: false,
            log_empty: false,
        }
    }

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tehas-watch-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_runner_that_wrote_its_result_exits_as_it_says() {
        for (rc, want) in [
            (0, 0),
            (10, 10),
            (11, 11),
            (20, 20),
            (21, 21),
            (30, 30),
            (40, 40),
        ] {
            let o = json!({"status": "handoff"});
            let s = Seen {
                outcome: Some(&o),
                ..seen(rc)
            };
            assert_eq!(exit_of(&s, &decide(&s)), want);
        }
    }

    #[test]
    fn an_exit_of_no_end_falls_to_the_results_own_then_to_2() {
        let o = json!({"status": "handoff", "exitCode": 10});
        let s = Seen {
            outcome: Some(&o),
            ..seen(1)
        };
        assert_eq!(exit_of(&s, &decide(&s)), 10);
        let o = json!({"status": "handoff", "exitCode": null});
        let s = Seen {
            outcome: Some(&o),
            ..seen(1)
        };
        assert_eq!(exit_of(&s, &decide(&s)), 2);
    }

    #[test]
    fn no_result_is_2_unless_it_is_an_error_only_result() {
        for rc in [0, 1, 10, 137] {
            let s = seen(rc);
            let want = if rc == 137 { 137 } else { 2 };
            assert_eq!(exit_of(&s, &decide(&s)), want);
        }
        for (rc, want) in [(30, 30), (40, 40), (137, 2), (1, 2)] {
            let s = Seen {
                error_only: true,
                ..seen(rc)
            };
            assert_eq!(exit_of(&s, &decide(&s)), want);
        }
    }

    #[test]
    fn a_stop_of_this_command_is_2_whatever_the_runner_exits() {
        let o = json!({"status": "interrupted", "exitCode": 21});
        for s in [
            Seen {
                outcome: Some(&o),
                stalled: true,
                stopped: true,
                ..seen(21)
            },
            Seen {
                outcome: Some(&o),
                timed: true,
                stopped: true,
                ..seen(21)
            },
            Seen {
                stalled: true,
                stopped: true,
                ..seen(137)
            },
            Seen {
                timed: true,
                stopped: true,
                ..seen(137)
            },
        ] {
            assert_eq!(exit_of(&s, &decide(&s)), 2);
        }
    }

    #[test]
    fn a_stop_names_the_status_unless_the_runner_ended_first() {
        let i = json!({"status": "interrupted"});
        let s = Seen {
            outcome: Some(&i),
            stalled: true,
            ..seen(21)
        };
        assert_eq!(decide(&s).status, "stalled");
        let s = Seen {
            outcome: Some(&i),
            timed: true,
            ..seen(21)
        };
        assert_eq!(decide(&s).status, "timeout");
        let h = json!({"status": "handoff"});
        let s = Seen {
            outcome: Some(&h),
            timed: true,
            ..seen(10)
        };
        assert_eq!(decide(&s).status, "handoff");
        let s = Seen {
            interrupted: true,
            ..seen(2)
        };
        assert_eq!(decide(&s).status, "interrupted");
        assert_eq!(decide(&seen(2)).status, "running");
    }

    #[test]
    fn a_result_with_no_status_is_unknown() {
        let o = json!({"status": ""});
        let s = Seen {
            outcome: Some(&o),
            ..seen(0)
        };
        assert_eq!(decide(&s).status, "unknown");
    }

    #[test]
    fn a_runner_at_the_limit_with_no_end_ran_out() {
        let r = json!({"status": "running"});
        let s = Seen {
            outcome: Some(&r),
            at_limit: true,
            ..seen(30)
        };
        let d = decide(&s);
        assert!(d.ran_out);
        assert_eq!(d.status, "timeout");
        let s = Seen {
            at_limit: true,
            ..seen(30)
        };
        assert!(decide(&s).ran_out);
        // A cap of its own, an end, a kill or a stop here is no running out.
        let c = json!({"status": "running", "cap": "deadline"});
        assert!(
            !decide(&Seen {
                outcome: Some(&c),
                at_limit: true,
                ..seen(30)
            })
            .ran_out
        );
        let e = json!({"status": "merged"});
        assert!(
            !decide(&Seen {
                outcome: Some(&e),
                at_limit: true,
                ..seen(0)
            })
            .ran_out
        );
        assert!(
            !decide(&Seen {
                at_limit: true,
                ..seen(137)
            })
            .ran_out
        );
        assert!(
            !decide(&Seen {
                at_limit: true,
                interrupted: true,
                ..seen(21)
            })
            .ran_out
        );
    }

    #[test]
    fn a_kill_is_this_cycles_end_only_when_nobody_here_stopped_it() {
        let d = decide(&seen(137));
        assert!(d.killed);
        assert_eq!(d.status, "killed");
        assert!(
            !decide(&Seen {
                stopped: true,
                ..seen(137)
            })
            .killed
        );
        assert!(
            !decide(&Seen {
                interrupted: true,
                ..seen(137)
            })
            .killed
        );
        assert!(
            !decide(&Seen {
                error_only: true,
                ..seen(137)
            })
            .killed
        );
        let o = json!({"status": "handoff"});
        assert!(
            !decide(&Seen {
                outcome: Some(&o),
                ..seen(137)
            })
            .killed
        );
    }

    #[test]
    fn nostart_is_a_container_with_no_result_no_line_and_no_stop() {
        let quiet = |rc| Seen {
            log_empty: true,
            ..seen(rc)
        };
        assert!(decide(&quiet(1)).nostart);
        assert!(decide(&quiet(125)).nostart);
        assert!(decide(&quiet(999)).nostart);
        for rc in [0, 137, 1000] {
            assert!(!decide(&quiet(rc)).nostart, "{rc}");
        }
        assert!(!decide(&seen(1)).nostart);
        for s in [
            Seen {
                stopped: true,
                ..quiet(1)
            },
            Seen {
                interrupted: true,
                ..quiet(1)
            },
            Seen {
                timed: true,
                ..quiet(1)
            },
            Seen {
                stalled: true,
                ..quiet(1)
            },
            Seen {
                at_limit: true,
                ..quiet(1)
            },
            Seen {
                error_only: true,
                ..quiet(1)
            },
        ] {
            assert!(!decide(&s).nostart);
        }
    }

    #[test]
    fn the_step_in_flight_is_the_results_then_the_consoles() {
        let d = dir("inflight");
        let now = d.join("now");
        fs::write(&now, "lens review!\nnotice\n").unwrap();
        let o = json!({"failedStep": "", "lastStep": "develop"});
        assert_eq!(inflight(Some(&o), &now), "develop");
        let o = json!({"failedStep": "gate", "lastStep": "develop"});
        assert_eq!(inflight(Some(&o), &now), "gate");
        assert_eq!(inflight(None, &now), "lensreview");
        assert_eq!(inflight(None, &d.join("none")), "-");
        fs::write(&now, "no end of line").unwrap();
        assert_eq!(inflight(None, &now), "-");
        fs::write(&now, format!("{}\n", "a".repeat(60))).unwrap();
        assert_eq!(inflight(None, &now), "a".repeat(40));
    }

    #[test]
    fn the_two_lines_of_the_console() {
        let d = dir("now");
        let now = d.join("now");
        assert_eq!(now_lines(&now), (String::new(), String::new()));
        fs::write(&now, "  develop \n a notice\nmore\n").unwrap();
        assert_eq!(now_lines(&now), ("develop".into(), "a notice".into()));
        fs::write(&now, "develop").unwrap();
        assert_eq!(now_lines(&now), ("develop".into(), String::new()));
    }

    #[test]
    fn the_newest_change_looks_through_directories_and_not_through_links() {
        let d = dir("newest");
        let log = d.join("log");
        fs::write(&log, "x").unwrap();
        let runs = d.join("runs");
        fs::create_dir_all(runs.join("a/b")).unwrap();
        fs::write(runs.join("a/b/t.json"), "x").unwrap();
        let old = Command::new("touch")
            .args(["-d", "2020-01-01T00:00:00Z"])
            .arg(&log)
            .status()
            .unwrap();
        assert!(old.success());
        let inner = fs::metadata(runs.join("a/b/t.json")).unwrap().mtime();
        assert_eq!(newest(&[&log, &runs]), Some(inner));
        assert_eq!(newest(&[&log]), Some(fs::metadata(&log).unwrap().mtime()));
        std::os::unix::fs::symlink(&runs, d.join("link")).unwrap();
        assert_eq!(newest(&[&d.join("link")]), None);
        assert_eq!(newest(&[&d.join("none")]), None);
    }

    #[test]
    fn a_cgroups_events_give_their_numbers() {
        let text = "low 0\nhigh 0\nmax 12\noom 3\noom_kill 3\n";
        assert_eq!(event(text, "oom_kill"), Some(3));
        assert_eq!(event(text, "max"), Some(12));
        assert_eq!(event(text, "none"), None);
        assert_eq!(event("max x\n", "max"), None);
        assert_eq!(event("max 1234567890\n", "max"), None);
        assert_eq!(event("", "max"), None);
    }

    #[test]
    fn the_cgroup_of_a_container_is_read_from_its_cid() {
        let d = dir("cgroup");
        let id = "a".repeat(64);
        let scope = d.join(format!("tehas.slice/docker-{id}.scope"));
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("memory.events"), "oom_kill 4\n").unwrap();
        fs::write(scope.join("pids.events"), "max 2\n").unwrap();
        // Read as the watch reads it, under TEHAS_CGROUP_ROOT's shape.
        let read = |f: &str| fs::read_to_string(scope.join(f)).unwrap();
        assert_eq!(event(&read("memory.events"), "oom_kill"), Some(4));
        assert_eq!(event(&read("pids.events"), "max"), Some(2));
    }

    #[test]
    fn the_kernel_log_counts_this_containers_kills() {
        let a = "a".repeat(64);
        let log = format!(
            "oom-kill:constraint=x,task_memcg=/tehas.slice/docker-{a}.scope,task=node\n\
             oom-kill:constraint=x,task_memcg=/tehas.slice/docker-{b}.scope,task=node\n\
             note oom-kill:constraint=x,task_memcg=/tehas.slice/docker-{a}.scope,task=node\n",
            b = "b".repeat(64)
        );
        assert_eq!(kills_in(&log, &a), 1);
    }

    #[test]
    fn the_diag_keeps_the_last_twenty_lines_cut_to_their_width() {
        let mut stream = vec![];
        for i in 0..25 {
            stream.extend_from_slice(format!("[pi-rukas] WARN {i}\n").as_bytes());
        }
        stream.extend_from_slice(b"WARN not the driver\n[pi-rukas] no word\n");
        let got = String::from_utf8(diag(&stream)).unwrap();
        assert_eq!(got.lines().count(), 20);
        assert!(got.starts_with("[pi-rukas] WARN 5\n"));
        assert!(got.ends_with("[pi-rukas] WARN 24\n"));
        let fat = format!("[pi-rukas] killed {}\n", "e".repeat(5000));
        assert_eq!(diag(fat.as_bytes()).len(), 1001);
        assert!(diag(b"nothing here\n").is_empty());
    }

    #[test]
    fn a_note_is_written_whole_or_not_at_all() {
        let d = dir("note");
        let home = d.join("home");
        fs::create_dir_all(home.join("state/cycles")).unwrap();
        let g = Given {
            issue: 7,
            attempt: String::new(),
            log: d.join("log"),
            runs: d.join("runs"),
            name: "n".into(),
            limit: 1,
            stall: 1,
            grace: 0,
            stop_wait: 1,
            console_wait: 1,
            console: d.join("console"),
            models: String::new(),
            email: String::new(),
            renewer: String::new(),
            docker: vec![],
        };
        let w = Watch {
            home: &home,
            g: &g,
            files: Files::of(&home, 7),
            err: d.join("log.err"),
            console_now: d.join("now"),
            stop: Arc::new(AtomicBool::new(false)),
            interrupted: false,
            console: None,
            live_on: false,
            beat_said: false,
            publish_said: false,
            ooms: 0,
            forks: 0,
            oom_id: String::new(),
        };
        assert!(w.write_note(&w.files.runner_exit, "10"));
        assert_eq!(fs::read_to_string(&w.files.runner_exit).unwrap(), "10\n");
        assert!(!beside(&w.files.runner_exit, ".new").exists());
        // A directory in its place: nothing written, and said in the record.
        fs::create_dir_all(&w.files.status).unwrap();
        assert!(!w.write_note(&w.files.status, "killed"));
        assert!(!beside(&w.files.status, ".new").exists());
        assert!(fs::read_to_string(&w.err)
            .unwrap()
            .contains("could not write"));
    }

    #[test]
    fn a_clock_of_a_program_answers_as_the_verbs_do() {
        let d = dir("clock");
        let p = d.join("clock");
        fs::write(
            &p,
            "#!/bin/sh\ncase \"$2\" in now) echo 42 ;; check) echo stalled ;; mark) : >\"$3\" ;; esac\n",
        )
        .unwrap();
        let chmod = Command::new("chmod").arg("+x").arg(&p).status().unwrap();
        assert!(chmod.success());
        let c = Clock::Program(p.to_string_lossy().into_owned());
        assert_eq!(c.now(), Some(42));
        assert!(c.mark(&d.join("m")));
        assert!(d.join("m").exists());
        assert_eq!(c.check(&d.join("m"), 1, 1, 1), "stalled");
    }
}
