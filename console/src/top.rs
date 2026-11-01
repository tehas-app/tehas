//! `tehas top` and the publisher behind `tehasbot top`, and the host section
//! both show: one reading of /proc, and the section drawn from two of them.
//!
//!   console tehas top [--once | --json] [--every <seconds>]
//!     The view of a person, see src/usage/top.txt. Loads no settings.
//!   console tehasbot top [--every]
//!     Writes $TEHAS_HOME/live/top.json, run under libexec/with-settings. The
//!     snapshot of the view, cut to the keys of PUBLISHED and nothing else:
//!     the dashboard can be on the internet, and so is this file. TEHAS_TOP=1
//!     turns it on; anything else removes the file. A file that cannot be
//!     written leaves the old one in place. With --every it runs one child
//!     per 5-second mark; 78 from a child ends the loop, and any other
//!     failure tries again at the next mark. Exit: 0 written or removed; 1
//!     not written; 64 usage.
//!   console top snapshot <lines> [<previous reading>]
//!     One snapshot, as the view takes one, see src/snapshot.rs.
//!
//!   console top reading <dir>
//!     One reading of the tree at <dir> (/proc on a host) as one line of JSON:
//!     the clock in seconds (uptime), the CPUs, the load averages, memory in
//!     KiB, the aggregate cpu line of stat, and one row per process: [pid,
//!     comm, CPU ticks, start ticks, resident pages]. Exit 1 when the tree
//!     cannot be read. The clock and the load averages are the text of the
//!     files, so a reading says what the host said.
//!
//! CPU use in the section is what changed between the two readings, never the
//! average since a process started. A process counts only with the same pid
//! and start time in both, so a reused pid is not one long process. Two
//! readings at the same instant measure nothing: their CPU figures are null.
//! The section is null when either reading is no object.
//!
//! A snapshot given the reading of the frame before takes one reading and
//! waits for nothing. Given none, it takes a second reading a second on.
//!
//!   console top frame <width> <height> <snapshot
//!     The frame drawn from a snapshot, see src/frame.rs.
//!
//! A process is named by its short command name only. The command line, the
//! environment and the user are never opened.

use crate::jq::J;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{IsTerminal, Read, Write};
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tehas_core::time;

// How many processes the section keeps.
const ROWS: usize = 15;

// A live file older than this is not believed, as the health file is not
// (src/status.rs).
const MAX_AGE: i64 = 600;

// What of the snapshot may be published: these keys and nothing else. A key
// added to the snapshot later stays out of live/top.json until someone names
// it here. What is inside a listed key is fixed by the code that builds it.
const PUBLISHED: [&str; 7] = [
    "generatedAt",
    "factory",
    "queue",
    "cycle",
    "host",
    "units",
    "energy",
];

// The seconds between the marks of the publisher.
const PERIOD: i64 = 5;

// How long a read of the terminal waits for a key, in tenths of a second. A
// signal is seen this often. stty's limit is 25.5 s, under the longest --every.
const POLL: &str = "2";

const EVERY: &str = "tehas top: --every wants a whole number from 1 to 64";

pub fn main(given: &[String]) -> u8 {
    match given {
        [verb, dir] if verb == "reading" => match reading(Path::new(dir)) {
            Some(text) => {
                println!("{text}");
                0
            }
            None => 1,
        },
        [verb, lines, was @ ..] if verb == "snapshot" && was.len() < 2 => match lines.parse() {
            Ok(lines) => {
                let was = was.first().map_or("", String::as_str);
                println!("{}", take(lines, was, second).0);
                0
            }
            _ => {
                eprintln!("console top: snapshot: <lines> is a whole number");
                2
            }
        },
        [verb, cols, rows] if verb == "frame" => {
            let mut given = String::new();
            let snapshot = std::io::Read::read_to_string(&mut std::io::stdin(), &mut given)
                .ok()
                .and_then(|_| crate::jq::J::parse(&given));
            match (cols.parse(), rows.parse(), snapshot) {
                (Ok(cols), Ok(rows), Some(snapshot)) => {
                    for line in crate::frame::frame(&snapshot, cols, rows) {
                        println!("{line}");
                    }
                    0
                }
                (Ok(_), Ok(_), None) => {
                    eprintln!("console top: frame: the snapshot is not JSON");
                    1
                }
                _ => {
                    eprintln!("console top: frame: <width> and <height> are whole numbers");
                    2
                }
            }
        }
        _ => {
            eprintln!("console top: {USAGE}");
            2
        }
    }
}

// The verbs `main` takes, so the word a caller is shown and the arms that
// answer them are said once.
const USAGE: &str =
    "console top: usage: reading <dir> | snapshot <lines> [<previous reading>] | frame <width> <height> <snapshot";

fn var(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}

fn home() -> String {
    match var("TEHAS_HOME") {
        h if h.is_empty() => format!("{}/tehas", var("HOME")),
        h => h,
    }
}

fn proc_dir() -> PathBuf {
    match var("TEHAS_PROC_DIR") {
        d if d.is_empty() => PathBuf::from("/proc"),
        d => PathBuf::from(d),
    }
}

// By name, so a test's stub takes the wait.
fn second() {
    let _ = Command::new("sleep").arg("1").status();
}

/// The readings of one snapshot, the later first. A previous reading that is
/// no object counts as none.
fn readings(read: impl Fn() -> Option<String>, wait: impl FnOnce(), was: &str) -> (String, String) {
    let is_reading = serde_json::from_str::<Value>(was).is_ok_and(|w| w.is_object());
    let was = if is_reading { was } else { "" };
    let now = read().unwrap_or_default();
    if now.is_empty() || !was.is_empty() {
        return (now, was.to_string());
    }
    wait();
    match read() {
        Some(later) => (later, now),
        None => (now, String::new()),
    }
}

/// One snapshot, and the reading the next one compares against.
fn take(lines: usize, was: &str, wait: impl FnOnce()) -> (String, String) {
    let dir = proc_dir();
    let (now, earlier) = readings(|| reading(&dir), wait, was);
    let live = Path::new(&home()).join("live");
    let snapshot = crate::snapshot::snapshot(&live, lines, MAX_AGE, &now, &earlier);
    (snapshot, now)
}

/// The part of a snapshot that may be published, one line.
fn published(snapshot: &str) -> Option<String> {
    match J::parse(snapshot)? {
        J::Obj(fields) => {
            let kept = fields
                .into_iter()
                .filter(|(key, _)| PUBLISHED.contains(&key.as_str()))
                .collect();
            Some(J::Obj(kept).dump())
        }
        _ => None,
    }
}

fn write_published(file: &Path, line: &str) -> std::io::Result<()> {
    let new = file.with_extension("json.new");
    let wrote = fs::write(&new, format!("{line}\n")).and_then(|()| fs::rename(&new, file));
    if wrote.is_err() {
        let _ = fs::remove_file(&new);
    }
    wrote
}

fn publish_once(home: &str) -> u8 {
    let live = Path::new(home).join("live");
    let file = live.join("top.json");
    if var("TEHAS_TOP") != "1" {
        let _ = fs::remove_file(&file);
        return 0;
    }
    if fs::create_dir_all(&live).is_err() {
        eprintln!("tehasbot top: could not create {}", live.display());
        return 1;
    }
    let (snapshot, _) = take(0, "", second);
    let wrote = published(&snapshot).map(|line| write_published(&file, &line));
    if !matches!(wrote, Some(Ok(()))) {
        eprintln!("tehasbot top: could not write {}", file.display());
        return 1;
    }
    0
}

// A child per mark, by the installed path, so an update reaches the loop: the
// unit is not restarted on a console change, and current_exe reads "(deleted)"
// once an install has replaced the file. with-settings stands before the
// console so that each mark reads the settings, as each run of the command does.
fn publish_every(home: &str) -> u8 {
    let with_settings = format!("{home}/libexec/with-settings");
    loop {
        let done = Command::new(&with_settings)
            .args(["tehasbot", "top"])
            .status();
        if done.is_ok_and(|s| s.code() == Some(78)) {
            return 78;
        }
        std::thread::sleep(Duration::from_secs(to_mark(time::now())));
    }
}

fn to_mark(now: i64) -> u64 {
    (PERIOD - now.rem_euclid(PERIOD)) as u64
}

pub fn publish(given: &[String]) -> u8 {
    let home = home();
    match given.first().map(String::as_str) {
        None | Some("") => publish_once(&home),
        Some("--every") => publish_every(&home),
        _ => {
            eprintln!("usage: tehasbot top [--every]");
            64
        }
    }
}

#[derive(Debug, PartialEq)]
enum Mode {
    Live,
    Once,
    Json,
}

fn view_words(given: &[String]) -> Result<(Mode, u64), String> {
    let (mut mode, mut every) = (Mode::Live, 2);
    let mut words = given.iter();
    while let Some(word) = words.next() {
        match word.as_str() {
            "--once" => mode = Mode::Once,
            "--json" => mode = Mode::Json,
            "--every" => {
                every = words
                    .next()
                    .and_then(|n| digits(n))
                    .filter(|n| (1..=64).contains(n))
                    .ok_or(EVERY)?;
            }
            _ => return Err(format!("tehas top: unknown argument: {word}")),
        }
    }
    Ok((mode, every))
}

fn clamped(cols: Option<u64>, rows: Option<u64>) -> (usize, usize) {
    (
        cols.unwrap_or(80).clamp(20, 400) as usize,
        rows.unwrap_or(24).clamp(6, 200) as usize,
    )
}

/// What `stty size` prints, the rows first: (rows, columns).
fn stty_size(text: &str) -> Option<(u64, u64)> {
    let (rows, cols) = text.trim_end().split_once(' ')?;
    Some((digits(rows)?, digits(cols)?))
}

// stty answers only for a terminal, and fails for anything else on standard
// output.
fn terminal_size() -> Option<(u64, u64)> {
    let out = std::io::stdout().as_fd().try_clone_to_owned().ok()?;
    let asked = Command::new("stty")
        .arg("size")
        .stdin(Stdio::from(out))
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !asked.status.success() {
        return None;
    }
    stty_size(&String::from_utf8_lossy(&asked.stdout))
}

/// The size to draw for: COLUMNS and LINES when they are given, the terminal
/// on standard output when it can be asked, and a plain 80 by 24 when neither
/// is.
fn size() -> (usize, usize) {
    let (mut cols, mut rows) = (digits(&var("COLUMNS")), digits(&var("LINES")));
    if cols.is_none() || rows.is_none() {
        if let Some((r, c)) = terminal_size() {
            cols = cols.or(Some(c));
            rows = rows.or(Some(r));
        }
    }
    clamped(cols, rows)
}

fn lines(snapshot: &str, cols: usize, rows: usize) -> Option<Vec<String>> {
    Some(crate::frame::frame(&J::parse(snapshot)?, cols, rows))
}

fn put(text: &str) -> bool {
    let mut out = std::io::stdout().lock();
    out.write_all(text.as_bytes()).and(out.flush()).is_ok()
}

fn tty(args: &[&str]) -> Option<String> {
    let done = Command::new("stty")
        .args(args)
        .stdin(File::open("/dev/tty").ok()?)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    done.status
        .success()
        .then(|| String::from_utf8_lossy(&done.stdout).trim().to_string())
}

/// The alternate screen with no cursor, and the terminal in its keys-at-once
/// mode. Dropping it leaves the terminal as it was.
struct Screen {
    saved: Option<String>,
    keys: Option<File>,
}

impl Screen {
    fn enter() -> Screen {
        put("\x1b[?1049h\x1b[?25l");
        let saved = tty(&["-g"]);
        let quiet =
            saved.is_some() && tty(&["-icanon", "-echo", "min", "0", "time", POLL]).is_some();
        let keys = if quiet {
            File::open("/dev/tty").ok()
        } else {
            None
        };
        Screen { saved, keys }
    }

    /// Up to `every` seconds, or until a key or a signal. True to end the view.
    fn wait(&mut self, every: u64, stop: &AtomicBool) -> bool {
        let end = Instant::now() + Duration::from_secs(every);
        while Instant::now() < end {
            if stop.load(Ordering::Relaxed) {
                return true;
            }
            let began = Instant::now();
            let mut key = [0u8];
            match self.keys.as_mut().map(|keys| keys.read(&mut key)) {
                Some(Ok(1)) => return key[0] == b'q',
                Some(Err(e)) if e.kind() != std::io::ErrorKind::Interrupted => self.keys = None,
                _ => {}
            }
            // No terminal to read from, or one that hung up and answers at once.
            if began.elapsed() < Duration::from_millis(100) {
                std::thread::sleep(Duration::from_millis(200));
            }
        }
        false
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        if let Some(saved) = &self.saved {
            tty(&[saved]);
        }
        put("\x1b[?25h\x1b[?1049l");
    }
}

fn live_view(every: u64) -> u8 {
    let stop = Arc::new(AtomicBool::new(false));
    use signal_hook::consts::{SIGHUP, SIGINT, SIGQUIT, SIGTERM};
    for signal in [SIGHUP, SIGINT, SIGQUIT, SIGTERM] {
        let _ = signal_hook::flag::register(signal, Arc::clone(&stop));
    }
    let mut screen = Screen::enter();
    let mut was = String::new();
    loop {
        let (cols, rows) = size();
        let (snapshot, now) = take(rows, &was, second);
        was = now;
        if stop.load(Ordering::Relaxed) {
            return 0;
        }
        let Some(frame) = lines(&snapshot, cols, rows) else {
            return 1;
        };
        let frame = frame.join("\n");
        put(&format!("\x1b[H\x1b[2J{}\n", frame.trim_end_matches('\n')));
        if screen.wait(every, &stop) {
            return 0;
        }
    }
}

pub fn view(given: &[String]) -> u8 {
    let (mode, every) = match view_words(given) {
        Ok(words) => words,
        Err(said) => {
            eprintln!("{said}");
            return 64;
        }
    };
    let live = Path::new(&home()).join("live");
    if !live.is_dir() {
        eprintln!("tehas top: no live directory at {} yet", live.display());
        return 1;
    }
    match mode {
        Mode::Json => {
            let (snapshot, _) = take(size().1, "", second);
            u8::from(!put(&format!("{snapshot}\n")))
        }
        Mode::Live if std::io::stdout().is_terminal() => live_view(every),
        _ => {
            let (cols, rows) = size();
            let (snapshot, _) = take(rows, "", second);
            match lines(&snapshot, cols, rows) {
                Some(frame) => u8::from(!put(&format!("{}\n", frame.join("\n")))),
                None => 1,
            }
        }
    }
}

/// The section from two readings, as text: nothing when either is no object,
/// which is what a caller that has no section prints.
pub fn host_text(later: &str, earlier: &str) -> Option<String> {
    let parse = |s: &str| serde_json::from_str::<Value>(s).unwrap_or(Value::Null);
    host_section(&parse(later), &parse(earlier), || raw_load(later))
}

/// The section from two readings that have been parsed, or nothing when
/// either is no object. The load averages are filled in by `load`, which
/// gives their text as the reading wrote it.
fn host_section(
    later: &Value,
    earlier: &Value,
    load: impl FnOnce() -> Option<String>,
) -> Option<String> {
    if !(later.is_object() && earlier.is_object()) {
        return None;
    }
    let section = host(later, earlier, page_size());
    // A section that is nothing stays nothing: only the load of a section that
    // is there has a sentinel to fill.
    if section.is_null() {
        return None;
    }
    let section = section.to_string();
    // The load averages as the reading wrote them: 0.10 stays 0.10.
    Some(match load() {
        Some(raw) => section.replacen(LOAD, &raw, 1),
        None => section.replacen(LOAD, "null", 1),
    })
}

/// A decimal as the file wrote it, digits and dots only.
fn decimal(s: &str) -> Option<&str> {
    (!s.is_empty()
        && s.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        && s.parse::<f64>().is_ok())
    .then_some(s)
}

fn digits(s: &str) -> Option<u64> {
    (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        .then(|| s.parse().ok())
        .flatten()
}

fn reading(root: &Path) -> Option<String> {
    let uptime = fs::read_to_string(root.join("uptime")).ok()?;
    let clock = decimal(uptime.lines().next()?.split(' ').next()?)?.to_string();

    // The sum over every cpu: user nice system idle iowait irq softirq steal.
    // Idle and iowait are not use.
    let stat = fs::read_to_string(root.join("stat")).ok()?;
    let f: Vec<&str> = stat.lines().next()?.split_whitespace().collect();
    if f.first() != Some(&"cpu") || f.len() < 9 {
        return None;
    }
    let ticks: Vec<u64> = f[1..9].iter().map(|v| digits(v)).collect::<Option<_>>()?;
    let total: u64 = ticks.iter().sum();
    let idle = ticks[3] + ticks[4];

    let mut cpus = fs::read_to_string(root.join("cpuinfo"))
        .map(|s| s.lines().filter(|l| l.starts_with("processor")).count())
        .unwrap_or(0);
    if cpus == 0 {
        cpus = std::thread::available_parallelism().map_or(1, |n| n.get());
    }

    // The three load averages and memory: either can be absent, and is then
    // said as nothing rather than guessed at.
    let load: Vec<String> = fs::read_to_string(root.join("loadavg"))
        .ok()
        .and_then(|s| {
            let words: Vec<&str> = s.lines().next()?.split_whitespace().take(3).collect();
            let load: Option<Vec<String>> = words
                .iter()
                .map(|w| decimal(w).map(str::to_string))
                .collect();
            load.filter(|l| l.len() == 3)
        })
        .unwrap_or_default();
    let (mut total_kb, mut avail_kb) = (None, None);
    if let Ok(meminfo) = fs::read_to_string(root.join("meminfo")) {
        for line in meminfo.lines() {
            let mut w = line.split_whitespace();
            match (w.next(), w.next()) {
                (Some("MemTotal:"), Some(v)) => total_kb = digits(v),
                (Some("MemAvailable:"), Some(v)) => avail_kb = digits(v),
                _ => {}
            }
        }
    }
    let memory = match (total_kb, avail_kb) {
        (Some(t), Some(a)) => format!("{{\"totalKb\":{t},\"availKb\":{a}}}"),
        _ => "null".to_string(),
    };

    let mut pids: Vec<(u64, std::path::PathBuf)> = fs::read_dir(root)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| Some((digits(e.file_name().to_str()?)?, e.path())))
                .collect()
        })
        .unwrap_or_default();
    pids.sort();
    let procs: Vec<Value> = pids
        .iter()
        .filter_map(|(pid, dir)| row(*pid, dir))
        .collect();

    Some(format!(
        "{{\"clock\":{clock},\"cpus\":{cpus},\"load\":[{}],\"memory\":{memory},\"cpuAll\":{total},\"cpuIdle\":{idle},\"procs\":{}}}",
        load.join(","),
        Value::Array(procs)
    ))
}

/// [pid, comm, utime + stime, starttime, rss] from <dir>/stat. The comm is
/// what lies between the first ( and the last ), and may hold either; the
/// fields after it are counted from there. A file that cannot be read, or
/// names too few fields, gives no row.
fn row(pid: u64, dir: &Path) -> Option<Value> {
    let text = fs::read_to_string(dir.join("stat")).ok()?;
    let text: String = text
        .lines()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let (open, close) = (text.find('(')?, text.rfind(')')?);
    if close < open {
        return None;
    }
    let comm = &text[open + 1..close];
    let f: Vec<&str> = text[close + 1..].split_whitespace().collect();
    if f.len() < 22 {
        return None;
    }
    let (utime, stime) = (digits(f[11])?, digits(f[12])?);
    let (start, rss) = (digits(f[19])?, digits(f[21])?);
    Some(json!([pid, comm, utime + stime, start, rss]))
}

fn page_size() -> f64 {
    Command::new("getconf")
        .arg("PAGE_SIZE")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| digits(s.trim()))
        .map_or(4096.0, |p| p as f64)
}

// Where the load averages go in a section, for main to fill in as text.
const LOAD: &str = "\"\\u0001load\"";

/// The three load averages of a reading as its text wrote them.
fn raw_load(reading: &str) -> Option<String> {
    let at = reading.find("\"load\":")? + "\"load\":".len();
    let rest = reading[at..].trim_start().strip_prefix('[')?;
    let words: Vec<&str> = rest[..rest.find(']')?].split(',').map(str::trim).collect();
    (words.len() == 3 && words.iter().all(|w| w.parse::<f64>().is_ok()))
        .then(|| format!("[{}]", words.join(",")))
}

/// A number that is not negative, else nothing.
fn num(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64).filter(|n| *n >= 0.0)
}

/// A figure as jq prints it: a whole number without a fraction.
fn figure(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 9e15 {
        Value::from(n as i64)
    } else {
        Value::from(n)
    }
}

/// Rounded to one decimal, within the bounds.
fn one(n: f64, hi: f64) -> Value {
    figure((n.clamp(0.0, hi) * 10.0).round() / 10.0)
}

fn host(later: &Value, earlier: &Value, page: f64) -> Value {
    let (Some(a), Some(b)) = (later.as_object(), earlier.as_object()) else {
        return Value::Null;
    };
    let (c2, c1) = (num(a.get("clock")), num(b.get("clock")));
    // The window in ticks, a hundredth of a second.
    let dt = match (c2, c1) {
        (Some(c2), Some(c1)) if c2 > c1 => Some(((c2 - c1) * 100.0).round()),
        _ => None,
    };
    let rows = |r: &Map<String, Value>| -> Vec<Vec<Value>> {
        r.get("procs")
            .and_then(Value::as_array)
            .map(|p| {
                p.iter()
                    .filter_map(|row| row.as_array().filter(|r| r.len() == 5).cloned())
                    .collect()
            })
            .unwrap_or_default()
    };
    let key = |v: &Value| match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let prev: HashMap<String, Vec<Value>> = rows(b).into_iter().map(|r| (key(&r[0]), r)).collect();
    let dall = num(a.get("cpuAll"))
        .zip(num(b.get("cpuAll")))
        .map(|(x, y)| x - y);
    let didle = num(a.get("cpuIdle"))
        .zip(num(b.get("cpuIdle")))
        .map(|(x, y)| x - y);
    let total_kb = a.get("memory").and_then(|m| num(m.get("totalKb")));
    let avail_kb = a.get("memory").and_then(|m| num(m.get("availKb")));
    let mem_total = total_kb.filter(|t| *t > 0.0);

    let cpu_percent = match (dt, dall, didle) {
        (Some(_), Some(dall), Some(didle)) if dall > 0.0 => {
            one((dall - didle) / dall * 100.0, 100.0)
        }
        _ => Value::Null,
    };
    let memory = match (mem_total, avail_kb) {
        (Some(t), Some(av)) => json!({
            "totalMb": figure((t / 1024.0).round()),
            "usedMb": figure(((t - av) / 1024.0).round()),
            "percent": one((t - av) / t * 100.0, 100.0),
        }),
        _ => Value::Null,
    };
    let load = if a
        .get("load")
        .and_then(Value::as_array)
        .is_some_and(|l| l.len() == 3)
    {
        json!("\u{1}load")
    } else {
        Value::Null
    };

    let mut processes: Vec<(f64, f64, Value)> = rows(a)
        .into_iter()
        .filter_map(|r| {
            let p = prev.get(&key(&r[0]))?;
            if num(p.get(3)) != num(r.get(3)) {
                return None;
            }
            let cpu = match (dt, num(r.get(2)), num(p.get(2))) {
                (Some(dt), Some(now), Some(was)) if dt > 0.0 => one((now - was) / dt * 100.0, 99999.0),
                _ => Value::Null,
            };
            let mem = match (mem_total, num(r.get(4))) {
                (Some(t), Some(rss)) => one(rss * page / 1024.0 / t * 100.0, 100.0),
                _ => Value::Null,
            };
            let up = match (c2, num(r.get(3))) {
                (Some(c2), Some(start)) => {
                    figure((((c2 * 100.0).round() - start) / 100.0).floor().clamp(0.0, 999_999_999.0))
                }
                _ => Value::Null,
            };
            let order = cpu.as_f64().unwrap_or(-1.0);
            let pid = r[0].as_f64().unwrap_or(0.0);
            let name = if r[1].is_null() { json!("") } else { r[1].clone() };
            Some((
                order,
                pid,
                json!({"pid": r[0], "name": name, "cpuPercent": cpu, "memPercent": mem, "upSeconds": up}),
            ))
        })
        .collect();
    // The most CPU in the window first, then by pid.
    processes.sort_by(|x, y| y.0.total_cmp(&x.0).then(x.1.total_cmp(&y.1)));
    let processes: Vec<Value> = processes.into_iter().take(ROWS).map(|p| p.2).collect();

    json!({
        "cpus": a.get("cpus").and_then(|c| num(Some(c))).map_or(Value::Null, figure),
        "load": load,
        "cpuPercent": cpu_percent,
        "memory": memory,
        "processes": processes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(clock: f64, all: u64, idle: u64, procs: Value) -> Value {
        json!({"clock": clock, "cpus": 2, "load": [0.1, 0.2, 0.3],
               "memory": {"totalKb": 65536, "availKb": 32768},
               "cpuAll": all, "cpuIdle": idle, "procs": procs})
    }

    #[test]
    fn use_is_the_difference_between_two_readings() {
        let was = reading(
            1000.0,
            100_000,
            90_000,
            json!([
                [100, "sleepy", 100000, 100, 400],
                [200, "busy", 100, 200, 400]
            ]),
        );
        let now = reading(
            1100.0,
            200_000,
            180_000,
            json!([
                [100, "sleepy", 100100, 100, 400],
                [200, "busy", 4100, 200, 400],
                [700, "new", 50, 700, 10]
            ]),
        );
        let h = host(&now, &was, 4096.0);
        assert_eq!(h["cpuPercent"], json!(10));
        assert_eq!(
            h["memory"],
            json!({"totalMb": 64, "usedMb": 32, "percent": 50})
        );
        let names: Vec<&str> = h["processes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["busy", "sleepy"]);
        assert_eq!(h["processes"][0]["cpuPercent"], json!(40));
        assert_eq!(h["processes"][1]["cpuPercent"], json!(1));
        assert_eq!(h["processes"][0]["upSeconds"], json!(1098));
        assert_eq!(h["processes"][0]["memPercent"], json!(2.4));
    }

    #[test]
    fn the_same_instant_measures_nothing_and_a_reused_pid_is_not_one_process() {
        let was = reading(1000.0, 1000, 900, json!([[100, "cycle", 5000, 500, 400]]));
        let h = host(&was, &was, 4096.0);
        assert_eq!(h["cpuPercent"], Value::Null);
        assert_eq!(h["processes"][0]["cpuPercent"], Value::Null);
        let now = reading(1010.0, 2000, 1900, json!([[100, "cycle", 50, 50000, 400]]));
        assert_eq!(host(&now, &was, 4096.0)["processes"], json!([]));
        assert_eq!(host(&now, &json!("x"), 4096.0), Value::Null);
    }

    // The three load averages of a reading as its file wrote them. The later
    // reading and the earlier one are asked to say different figures, so a
    // section that carries the earlier ones can be seen.
    const LOAD_LATER: &str = "0.10,0.20,0.30";
    const LOAD_EARLIER: &str = "0.04,0.05,0.06";

    // One reading of the host as text. The clock, the load and the two
    // aggregate figures are written as the files gave them, so a reading keeps
    // the digits its host said.
    fn frame(clock: &str, all: &str, idle: &str, load: &str, procs: &str) -> String {
        format!(
            "{{\"clock\":{clock},\"cpus\":2,\"load\":[{load}],\"memory\":{{\"totalKb\":65536,\"availKb\":32768}},\"cpuAll\":{all},\"cpuIdle\":{idle},\"procs\":[{procs}]}}"
        )
    }

    // A row is [pid, comm, ticks, start ticks, resident pages]. The named
    // rows are the ones a test names, and the fill brings the count to
    // `count`: pid 300 upwards, each one `step` ticks more than the last, all
    // of them started at the same instant.
    fn rows(named: [&str; 3], ticks: u64, step: u64, count: usize) -> String {
        let fill = (0..count).map(|i| {
            format!(
                "[{},\"p{}\",{},{},10]",
                300 + i,
                300 + i,
                ticks + (i as u64) * step,
                500
            )
        });
        named
            .into_iter()
            .map(str::to_string)
            .chain(fill)
            .collect::<Vec<_>>()
            .join(",")
    }

    // Two readings of one host, a hundred seconds apart. The sleepy process
    // has used a thousand seconds of CPU in its life and one hundred ticks in
    // this window; the busy one forty seconds in the window and almost none in
    // its life. Over the host the window is 100000 ticks of which 90000 were
    // idle, so the host reads 10% - the figure the test above owns, kept here
    // only to show the whole section is there. The fill is seventeen processes
    // whose use in the window rises by one hundred ticks each, so the fifteen
    // highest fill the rows and the two that used least, at 1.0 and 2.0, are
    // out beside sleepy.
    fn pair() -> (String, String) {
        let was = frame(
            "1000.00",
            "100000",
            "90000",
            LOAD_EARLIER,
            &rows(
                [
                    "[100,\"sleepy\",100000,100,400]",
                    "[200,\"busy\",100,200,400]",
                    "[600,\"gone\",500,600,10]",
                ],
                1000,
                0,
                17,
            ),
        );
        let now = frame(
            "1100.00",
            "200000",
            "180000",
            LOAD_LATER,
            &rows(
                [
                    "[100,\"sleepy\",100100,100,400]",
                    "[200,\"busy\",4100,200,400]",
                    "[700,\"new\",50,700,10]",
                ],
                1100,
                100,
                17,
            ),
        );
        (now, was)
    }

    #[test]
    fn the_section_shows_the_difference_and_the_load_as_the_reading_wrote_it() {
        // The three load averages of a reading are its text, so they are read
        // back as the array a reading holds them, and they are the later
        // reading's: the earlier one says other figures.
        let (now, was) = pair();
        let text = host_text(&now, &was).unwrap();
        let h: Value = serde_json::from_str(&text).unwrap();
        assert!(text.contains("\"load\":[0.10,0.20,0.30]"), "{text}");
        assert_eq!(h["load"], json!([0.10, 0.20, 0.30]));
        // The host over the window: 100000 ticks of which 90000 were idle. The
        // aggregate figure itself is owned by the test above; here it says the
        // section was built at all.
        assert_eq!(h["cpuPercent"], json!(10));
        assert_eq!(
            h["memory"],
            json!({"totalMb": 64, "usedMb": 32, "percent": 50})
        );
        // The fifteen that used the most CPU in this window: sleepy is out
        // although it has used more than any of them in its life, and so are
        // the two of the fill that used least, p300 at 1.0 and p301 at 2.0,
        // below the fifteenth row at 4.0.
        let names: Vec<&str> = h["processes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "busy", "p316", "p315", "p314", "p313", "p312", "p311", "p310", "p309", "p308",
                "p307", "p306", "p305", "p304", "p303"
            ]
        );
        let mut used: Vec<f64> = h["processes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["cpuPercent"].as_f64().unwrap())
            .collect();
        assert_eq!(used[0], 40.0);
        used.sort_by(|a, b| a.total_cmp(b));
        assert_eq!(
            used,
            [4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0, 40.0]
        );
        assert_eq!(h["processes"][0]["upSeconds"], json!(1098));
        // A process only in the later reading and one only in the earlier are
        // not measured: there is nothing to take a difference between.
        assert!(!names.contains(&"new"));
        assert!(!names.contains(&"gone"));
    }

    #[test]
    fn the_section_is_nothing_when_a_reading_is_no_object() {
        let (now, was) = pair();
        // A reading that is no JSON at all, and a reading of no tree, leave the
        // section out: there is no pair to take a difference from. The
        // unparseable one is asked for on both sides, so the order of the two
        // cannot be what makes the answer.
        assert_eq!(host_text("not json", &was), None);
        assert_eq!(host_text(&now, "not json"), None);
        assert_eq!(host_text(&now, ""), None);
        assert_eq!(host_text("", &now), None);
        // The same through the function that decides it, not only the text that
        // carries the decision: one reading that is no object is enough, on
        // either side of the pair, whatever else the other one holds.
        assert_eq!(
            host(&json!("not json"), &json!({"clock": 1.0}), 4096.0),
            Value::Null
        );
        assert_eq!(
            host(&json!({"clock": 1.0}), &json!(""), 4096.0),
            Value::Null
        );
        // An object that says nothing is still a reading, so the pair is still
        // a pair: it gives a section, and that section says nothing. This is
        // the answer a caller must not read as "no section", and it is the one
        // a caller must not read as "the host is idle".
        let thin = host_text("{\"clock\":1}", &was).unwrap();
        let h: Value = serde_json::from_str(&thin).unwrap();
        assert!(h.is_object());
        assert_eq!(h["load"], Value::Null);
        assert_eq!(h["cpuPercent"], Value::Null);
        assert_eq!(h["processes"], json!([]));
    }

    #[test]
    fn a_reading_with_no_load_leaves_the_load_out() {
        let (now, was) = pair();
        let no_load = {
            let mut v: Value = serde_json::from_str(&now).unwrap();
            v.as_object_mut().unwrap().remove("load");
            v.to_string()
        };
        let h: Value = serde_json::from_str(&host_text(&no_load, &was).unwrap()).unwrap();
        assert_eq!(h["load"], Value::Null);
        // A reading whose load is a shorter array is a reading of its own: it
        // gives a section, and the section says nothing about the load. The
        // two are told apart here and nowhere else.
        let short = now.replace(LOAD_LATER, "0.10,0.20");
        assert_ne!(host_text(&short, &was), None);
        // Three figures are the array a section carries.
        let h: Value = serde_json::from_str(&host_text(&short, &was).unwrap()).unwrap();
        assert_eq!(h["load"], Value::Null);
        assert!(h["processes"].is_array());
    }

    fn words(given: &[&str]) -> Vec<String> {
        given.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn the_size_is_clamped_so_a_frame_has_something_to_fit_in() {
        assert_eq!(clamped(None, None), (80, 24));
        assert_eq!(clamped(Some(0), Some(0)), (20, 6));
        assert_eq!(clamped(Some(19), Some(5)), (20, 6));
        assert_eq!(clamped(Some(20), Some(6)), (20, 6));
        assert_eq!(clamped(Some(400), Some(200)), (400, 200));
        assert_eq!(clamped(Some(401), Some(201)), (400, 200));
    }

    #[test]
    fn stty_says_the_rows_first() {
        assert_eq!(stty_size("50 132\n"), Some((50, 132)));
        for text in ["", "50", "50 x", "x 132", "50  132"] {
            assert_eq!(stty_size(text), None, "{text:?}");
        }
    }

    #[test]
    fn every_takes_a_whole_number_from_1_to_64() {
        assert_eq!(view_words(&[]), Ok((Mode::Live, 2)));
        assert_eq!(view_words(&words(&["--every", "1"])), Ok((Mode::Live, 1)));
        assert_eq!(
            view_words(&words(&["--every", "064"])),
            Ok((Mode::Live, 64))
        );
        let last = words(&["--once", "--every", "5", "--json"]);
        assert_eq!(view_words(&last), Ok((Mode::Json, 5)));
        for bad in ["0", "65", "-1", "abc", "", "5x", "99999999999999999999"] {
            let said = view_words(&words(&["--every", bad]));
            assert_eq!(said, Err(EVERY.to_string()), "{bad:?}");
        }
        assert_eq!(view_words(&words(&["--every"])), Err(EVERY.to_string()));
        assert_eq!(
            view_words(&words(&["--watch"])),
            Err("tehas top: unknown argument: --watch".to_string())
        );
    }

    #[test]
    fn only_the_listed_keys_are_published_in_the_order_they_came() {
        let snapshot = r#"{"token":"ghp_secret","host":{"load":[0.10]},"reading":{"clock":"1"},
            "console":{"lines":["x"]},"generatedAt":"t","factory":null,"queue":1,"cycle":[],
            "units":[],"energy":null,"draft":{"note":"no"}}"#;
        assert_eq!(
            published(snapshot).unwrap(),
            r#"{"host":{"load":[0.10]},"generatedAt":"t","factory":null,"queue":1,"cycle":[],"units":[],"energy":null}"#
        );
    }

    #[test]
    fn a_cut_answers_nothing_for_what_is_no_snapshot() {
        for text in ["", "{not json}", "[]", "1"] {
            assert_eq!(published(text), None, "{text:?}");
        }
    }

    #[test]
    fn a_reading_from_the_frame_before_saves_the_wait() {
        let waits = std::cell::Cell::new(0);
        let wait = || waits.set(waits.get() + 1);
        let taken = std::cell::Cell::new(0);
        let read = || {
            taken.set(taken.get() + 1);
            Some(format!("{{\"n\":{}}}", taken.get()))
        };
        // Nothing to compare with: a pair, the second a wait later.
        assert_eq!(
            readings(read, wait, ""),
            (r#"{"n":2}"#.to_string(), r#"{"n":1}"#.to_string())
        );
        assert_eq!(waits.get(), 1);
        // The next frame hands on what the last one read, and takes one reading.
        assert_eq!(
            readings(read, wait, r#"{"n":2}"#),
            (r#"{"n":3}"#.to_string(), r#"{"n":2}"#.to_string())
        );
        assert_eq!(waits.get(), 1);
        // A previous reading that is no object counts as none.
        readings(read, wait, "[1]");
        assert_eq!(waits.get(), 2);
    }

    #[test]
    fn a_reading_that_fails_is_not_compared_or_waited_for() {
        let mut waited = false;
        assert_eq!(
            readings(|| None, || waited = true, ""),
            (String::new(), String::new())
        );
        assert!(!waited);
        assert_eq!(
            readings(|| None, || {}, r#"{"n":1}"#),
            (String::new(), r#"{"n":1}"#.to_string())
        );
        let first = std::cell::Cell::new(true);
        let once = || first.replace(false).then(|| "{}".to_string());
        assert_eq!(readings(once, || {}, ""), ("{}".to_string(), String::new()));
    }

    #[test]
    fn the_next_mark_is_the_next_multiple_of_the_period() {
        assert_eq!(to_mark(100), 5);
        assert_eq!(to_mark(101), 4);
        assert_eq!(to_mark(104), 1);
    }

    #[test]
    fn the_verbs_of_the_command_are_the_words_of_its_usage() {
        assert_eq!(super::main(&["top".into(), "nope".into()]), 2);
        for verb in ["reading", "snapshot", "frame"] {
            assert!(super::USAGE.contains(verb), "{verb} is not in the usage");
        }
    }

    #[test]
    fn a_tree_is_read_as_the_files_say() {
        let dir = std::env::temp_dir().join(format!("top-reading-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("42")).unwrap();
        fs::create_dir_all(dir.join("notapid")).unwrap();
        fs::write(dir.join("uptime"), "1000.00 5.0\n").unwrap();
        fs::write(dir.join("stat"), "cpu  1 2 3 4 5 6 7 8 9 10\ncpu0 1\n").unwrap();
        fs::write(dir.join("cpuinfo"), "processor\t: 0\nprocessor\t: 1\n").unwrap();
        fs::write(dir.join("loadavg"), "0.10 0.20 0.30 1/2 3\n").unwrap();
        fs::write(
            dir.join("meminfo"),
            "MemTotal: 64 kB\nMemAvailable: 32 kB\n",
        )
        .unwrap();
        fs::write(
            dir.join("42/stat"),
            "42 (a (b) c) S 1 1 1 0 -1 1 0 0 0 0 7 3 0 0 20 0 1 0 900 1048576 10 0\n",
        )
        .unwrap();
        fs::write(
            dir.join("notapid/stat"),
            "7 (x) S 1 1 1 0 -1 1 0 0 0 0 1 1 0 0 20 0 1 0 900 1 1 0\n",
        )
        .unwrap();
        let text = super::reading(&dir).unwrap();
        assert_eq!(
            text,
            r#"{"clock":1000.00,"cpus":2,"load":[0.10,0.20,0.30],"memory":{"totalKb":64,"availKb":32},"cpuAll":36,"cpuIdle":9,"procs":[[42,"a (b) c",10,900,10]]}"#
        );
        fs::remove_file(dir.join("uptime")).unwrap();
        assert!(super::reading(&dir).is_none());
        fs::remove_dir_all(&dir).unwrap();
    }
}
