//! `tehas stop <issue>`: end a cycle by hand and clear what it left on this
//! host. console/src/usage/stop.txt is the text a person reads; this says what the
//! code must keep.
//!
//! The order is the point: the marker, the unit, the container, the record
//! ended with the lock file there and free, the lock, the park, the removals,
//! the lock file, and last the label. A wrong order loses a cycle's end or
//! removes under a live lock.
//!
//! `state/cycles/$n.stopped` is what tehasbot cycle reads to say a person
//! stopped the cycle: one line, the epoch seconds and the account.
//!
//! What stays is the cycle's record: `.json`, `.manifest.json`, `.runner_exit`
//! and `.status` beside the lock, and the logs.
//!
//! A record this run ended raises its `cycle.ended`: the line has no report,
//! so no cycle raises it. One the store had ended already raises nothing
//! here; its writer raised it. The event's line goes to the factory's log.
//!
//! Every path is the settings and the number, as a string, never a glob. The
//! calls of systemctl and docker keep the bounds of `timeout` they had in
//! bash. The issue's running mark is read and taken off through the tickets
//! port, each call in [`BOUND`]; the console's wiring makes the handle.
//!
//! Exit: see console/src/usage/stop.txt.

use crate::{account, store};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Write as _};
use std::num::NonZeroU64;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;
use tehas_core::cycle_notes;
use tehas_core::home::Home;
use tehas_core::lock::{probe, Held, Probe};
use tehas_core::raise::{self, Writer};
use tehas_core::settings::rules::is_repo;
use tehas_ports::events::Dispatcher;
use tehas_ports::{Mark, Outcome, State, TicketKey, TicketWrites};

pub const USAGE: &str = include_str!("../../src/usage/stop.txt");

/// How long each call of the tickets port may take, the bound of `timeout`
/// the label's calls had in bash.
const BOUND: Duration = Duration::from_secs(10);

/// What the words of a run asked for.
#[derive(Debug, PartialEq, Eq)]
pub struct Asked {
    issue: String,
    keep: bool,
}

pub(crate) fn is_issue(word: &str) -> bool {
    let b = word.as_bytes();
    (1..=10).contains(&b.len()) && (b'1'..=b'9').contains(&b[0]) && b.iter().all(u8::is_ascii_digit)
}

/// The words of a run, read left to right. `Ok(None)` is help, printed here.
pub fn parse(given: &[String]) -> Result<Option<Asked>, u8> {
    let mut issue = "";
    let mut keep = false;
    for word in given {
        match word.as_str() {
            "-h" | "--help" => {
                tehas_core::out!("{USAGE}");
                return Ok(None);
            }
            "--keep-workspace" => keep = true,
            w if w.starts_with('-') => {
                eprintln!("tehas stop: unknown argument: {w}");
                eprint!("{USAGE}");
                return Err(64);
            }
            w => {
                if !issue.is_empty() {
                    eprintln!("tehas stop: one issue number at most");
                    return Err(64);
                }
                issue = w;
            }
        }
    }
    if issue.is_empty() {
        eprintln!("tehas stop: it needs an issue number");
        eprint!("{USAGE}");
        return Err(64);
    }
    if !is_issue(issue) {
        eprintln!("tehas stop: not an issue number: {issue}");
        return Err(64);
    }
    Ok(Some(Asked {
        issue: issue.to_string(),
        keep,
    }))
}

/// Whether the marker reads back as `read -r at by _` reads it: one finished
/// line, the seconds, then one account.
fn marker_reads(text: &str) -> bool {
    let Some((line, _)) = text.split_once('\n') else {
        return false;
    };
    let mut fields = line.split_ascii_whitespace();
    let (Some(at), Some(by)) = (fields.next(), fields.next()) else {
        return false;
    };
    let at_ok = at == "0" || is_issue(at);
    let by_ok = (1..=64).contains(&by.len())
        && by
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'));
    at_ok && by_ok
}

/// The marker goes through a temporary name and is read back: a name the move
/// could not replace keeps what it held.
fn write_marker(marker: &str) -> bool {
    let new = format!("{marker}.new");
    let at = tehas_core::time::now();
    let wrote = fs::write(&new, format!("{at} {}\n", account::name())).is_ok()
        && fs::rename(&new, marker).is_ok()
        && fs::read_to_string(marker).is_ok_and(|text| marker_reads(&text));
    if !wrote {
        let _ = fs::remove_file(&new);
    }
    wrote
}

/// As `rm -rf`: a link goes and what it points at stays, and a name that is
/// not there is gone already.
fn remove(path: &str) -> bool {
    match fs::symlink_metadata(path) {
        Err(e) => e.kind() == ErrorKind::NotFound,
        Ok(m) if m.is_dir() => fs::remove_dir_all(path).is_ok(),
        Ok(_) => fs::remove_file(path).is_ok(),
    }
}

/// A name that is a link or anything else but a plain file says nothing about
/// this issue, and taking it would write through it.
fn plain_or_absent(lock: &str) -> bool {
    fs::symlink_metadata(lock).map_or(true, |m| m.is_file())
}

fn take_lock_file(lock: &str) -> io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(lock)
}

/// What came out of a command that ended well, under `timeout <limit>`.
fn read(limit: &str, program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new("timeout")
        .arg(limit)
        .arg(program)
        .args(args)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Whether a command ended well under `timeout <limit>`. It says nothing.
fn quiet(limit: &str, program: &str, args: &[&str]) -> bool {
    Command::new("timeout")
        .arg(limit)
        .arg(program)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// A closed standard output does not stop a stop half way.
fn say(text: &str) {
    let mut out = io::stdout();
    let _ = out.write_all(text.as_bytes());
    let _ = out.flush();
}

/// What jq -r prints of a field read with `// empty`.
fn field_or_empty(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null | serde_json::Value::Bool(false) => String::new(),
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// The names of one issue on this host, built from the settings as strings so
/// that what is said is what was named.
struct Names {
    home: String,
    user_home: String,
    repo: String,
    work_dir: String,
    n: String,
}

impl Names {
    /// The home joined as text, as the paths this names always were.
    fn layout(&self) -> Home {
        Home::of_text(&self.home)
    }

    fn state(&self) -> String {
        self.layout().cycles_dir().display().to_string()
    }

    fn note(&self, end: &str) -> String {
        format!("{}/{}{end}", self.state(), self.n)
    }

    fn lock(&self) -> String {
        self.note(".lock")
    }

    fn unit(&self) -> String {
        format!("tehas-cycle@{}.service", self.n)
    }

    fn container(&self) -> String {
        format!("tehas-work-{}", self.n)
    }

    /// What is on the host now and may go: the notes, the console file, the
    /// transcripts, and unless it is kept the workspace with what is beside
    /// it, named the way tehas-cycle work names them.
    fn candidates(&self, keep: bool) -> Vec<String> {
        // The notes that say an issue's cycle runs, owes something, or is
        // parked.
        let mut out: Vec<String> = cycle_notes::stopped()
            .map(|name| self.note(&format!(".{name}")))
            .collect();
        // The number passed `is_issue`: ten digits at most, so it parses.
        if let Ok(n) = self.n.parse::<u64>() {
            out.push(self.layout().live_console(n).display().to_string());
        }
        out.push(format!(
            "{}/.pi/agent/ensemble-runs/issue-{}",
            self.user_home, self.n
        ));
        if !keep {
            for end in ["", ".state", ".before-retry", ".images"] {
                out.push(format!("{}-{}{end}", self.work_dir, self.n));
            }
        }
        out
    }

    /// A path in the shape a person types.
    fn short(&self, path: &str) -> String {
        if let Some(rest) = path.strip_prefix(&format!("{}/", self.home)) {
            rest.to_string()
        } else if let Some(rest) = path.strip_prefix(&format!("{}/", self.user_home)) {
            format!("~/{rest}")
        } else {
            path.to_string()
        }
    }

    fn removed(&self, path: &str) {
        say(&format!("removed {}\n", self.short(path)));
    }

    fn running(&self) -> bool {
        probe(Path::new(&self.lock())) == Probe::Held
    }

    /// The attempt of the cycle's record when it is open, empty when it has
    /// ended or there is none. Ends the run when the store could not answer.
    fn open_attempt(&self) -> Result<String, u8> {
        let home = Path::new(&self.home);
        match store::call(home, &["cycle-show", &self.n]) {
            Ok(v) => {
                let cycle = &v["cycle"];
                Ok(if !cycle.is_null() && cycle["ended_at"].is_null() {
                    cycle["attempt"].to_string()
                } else {
                    String::new()
                })
            }
            Err(no) => {
                eprintln!(
                    "tehas stop: the record of #{} could not be read from the state store: {}",
                    self.n, no.line
                );
                Err(if no.rc == 1 { 1 } else { 78 })
            }
        }
    }

    /// The question of the issue's current park, and whether its one retry is
    /// still owed. Ends the run when the store could not answer.
    fn park_owed(&self) -> Result<(String, bool), u8> {
        let home = Path::new(&self.home);
        match store::call(home, &["park-show", &self.n]) {
            Ok(v) => Ok((
                field_or_empty(&v["park"]["question"]),
                v["park"]["retries"] == 0,
            )),
            Err(no) => {
                eprintln!(
                    "tehas stop: the park of #{} could not be read from the state store: {}",
                    self.n, no.line
                );
                Err(if no.rc == 1 { 1 } else { 78 })
            }
        }
    }

    /// The last step, the issue's half: what to say, on standard output when
    /// it went well, or on standard error, with exit 1, when the number is
    /// not an issue, the running mark could not be read or taken off, or a
    /// cycle holds the lock again and owns the mark.
    fn take_label(&self, tickets: &mut dyn TicketWrites) -> Result<Option<String>, String> {
        let n = &self.n;
        let running = Mark::State(State::Running);
        let key = self
            .n
            .parse()
            .ok()
            .and_then(NonZeroU64::new)
            .map(TicketKey::Number);
        let answer = key
            .as_ref()
            .and_then(|k| tickets.several(std::slice::from_ref(k)).pop())
            .map(|(_, d)| d);
        let (key, digest) = match (key, answer) {
            (Some(key), Some(Ok(digest))) => (key, digest),
            // A pull request's number, or no item at all: no read failed.
            (_, Some(Err(Outcome::NotFound))) => {
                return Err(format!("#{n} is not an issue; tehas stops only issues"));
            }
            _ => {
                return Err(format!(
                    "WARN: the labels of #{n} could not be read, so tehas:running stays on the issue"
                ));
            }
        };
        if !digest.marks.contains(&running) {
            return Ok(None);
        }
        if self.running() {
            return Err(format!(
                "WARN: a cycle for #{n} holds its lock again, so tehas:running stays on the issue"
            ));
        }
        match tickets.unmark(&key, &running) {
            Ok(()) => Ok(Some(format!("removed tehas:running from #{n}"))),
            Err(_) => Err(format!("WARN: tehas:running could not be taken off #{n}")),
        }
    }

    fn run(
        &self,
        keep: bool,
        events: &Dispatcher,
        tickets: &mut dyn TicketWrites,
    ) -> Result<u8, u8> {
        let n = &self.n;
        let home = Path::new(&self.home);
        let unit = self.unit();
        let container = self.container();
        let lock = self.lock();
        let state = self.state();

        // What is here, read before anything changes: a run with nothing to
        // do says so and touches nothing, so running this twice is safe.
        let open_at = self.open_attempt()?;
        let (question, retry) = self.park_owed()?;
        let mut present: Vec<String> = self
            .candidates(keep)
            .into_iter()
            .filter(|p| Path::new(p).exists())
            .collect();

        let args = ["--user", "show", "-p", "ActiveState", "--value", &unit];
        let unit_state = read("10", "systemctl", &args).unwrap_or_default();
        let unit_live = matches!(unit_state.trim_end_matches('\n'), "active" | "activating");
        // A whole line: tehas-work-5 is not tehas-work-51.
        let names = read("10", "docker", &["ps", "-a", "--format", "{{.Names}}"]);
        let container_live = names.is_some_and(|all| all.split('\n').any(|c| c == container));
        let lock_held = self.running();

        if !unit_live
            && !container_live
            && !lock_held
            && present.is_empty()
            && open_at.is_empty()
            && question.is_empty()
            && !retry
        {
            say(&format!("nothing to stop for #{n}\n"));
            return Ok(0);
        }

        // Before the unit is asked to stop, since the cycle may report as soon
        // as it stops. A run that only clears leftovers writes none.
        let asked_to_stop = unit_live || lock_held;
        let marker = self.note(".stopped");
        let mut marker_this = true;
        if asked_to_stop {
            let _ = fs::create_dir_all(&state);
            if !write_marker(&marker) {
                eprintln!(
                    "tehas stop: {} could not be written, so nothing says a person stopped this cycle",
                    self.short(&marker)
                );
                marker_this = false;
            }
        }
        // A marker this run did not write, or could not read back, goes with
        // the leftovers.
        if !asked_to_stop || !marker_this {
            for p in [marker.clone(), self.note(".stopped.new")] {
                if Path::new(&p).exists() {
                    present.push(p);
                }
            }
        }

        let mut rc = 0;
        // As the poll does when it stops a cycle whose work is done: the
        // runner reports as it stops.
        if asked_to_stop {
            let (status, told) = store::merged(
                store::bounded("5", "30", "systemctl").args(["--user", "stop", &unit]),
            );
            say(&told);
            if status == 0 {
                say(&format!("stopped {unit}\n"));
            } else {
                eprintln!("tehas stop: could not stop {unit}");
                rc = 1;
            }
            // The program, not a sleep of this process: ten seconds at most.
            for _ in 0..20 {
                if !self.running() {
                    break;
                }
                let _ = Command::new("sleep").arg("0.5").status();
            }
            if self.running() {
                eprintln!(
                    "tehas stop: the cycle for #{n} still holds its lock after the stop, so nothing was removed. It needs a person: stop it with systemctl --user stop {unit} and run this again."
                );
                return Ok(1);
            }
        }

        // The container of a cycle whose client died works on with the bot's
        // token, and the next start under its name would fail.
        if container_live {
            if quiet("30", "docker", &["rm", "-f", &container]) {
                say(&format!("removed the container {container}\n"));
            } else {
                eprintln!("tehas stop: could not remove the container {container}");
                rc = 1;
            }
        }

        if !plain_or_absent(&lock) {
            eprintln!(
                "tehas stop: {} is not a lock file, so nothing was removed.",
                self.short(&lock)
            );
            return Ok(1);
        }

        if fs::create_dir_all(&state).is_err() {
            eprintln!("tehas stop: {state} could not be created");
            return Ok(1);
        }
        // Held for the removals, so no cycle of this issue starts among them,
        // and it goes with them, so a later cycle takes a fresh one. The file
        // is opened first: an open record with no lock file reads as a cycle
        // whose lock cannot be probed.
        let held = take_lock_file(&lock);
        if let Err(e) = &held {
            eprintln!("tehas stop: {}: {e}", self.short(&lock));
        }
        // The cycle reported if it could, so a record still open is one that
        // never will.
        let open_at = self.open_attempt()?;
        if !open_at.is_empty() {
            let end = ["cycle-end", n, "--attempt", &open_at, "--stopped"];
            let ended = store::try_call(home, 3, &end);
            if let Err(no) = &ended {
                if no.word == "running" {
                    eprintln!(
                        "tehas stop: a cycle for #{n} holds its lock, so nothing was removed."
                    );
                } else {
                    eprintln!(
                        "tehas stop: the record of #{n} could not be ended as stopped ({}), so nothing was removed. Run this again.",
                        no.line
                    );
                }
                return Ok(1);
            }
            let answer = ended.unwrap_or_default();
            let already = answer["already"].as_bool().unwrap_or(false);
            let reason = answer.pointer("/cycle/end_reason").and_then(|r| r.as_str());
            say(&ended_line(n, already, reason));
            raise_stopped(home, n, &open_at, &answer.to_string(), events);
        }
        let Some(Ok(held)) = held.ok().map(Held::try_take) else {
            eprintln!("tehas stop: a cycle for #{n} holds its lock, so nothing was removed.");
            return Ok(1);
        };

        // Read again: a cycle that stopped just now may have parked as it
        // reported.
        let (question, retry) = self.park_owed()?;
        if !question.is_empty() {
            let withdraw = ["question-answered", n, "--id", &question, "--withdraw"];
            match store::try_call(home, 3, &withdraw) {
                Err(no) if no.word != "unknown" => {
                    eprintln!(
                        "tehas stop: the park question of #{n} could not be taken off the record ({}), so nothing was removed. Run this again.",
                        no.line
                    );
                    return Ok(1);
                }
                _ => say(&format!("took the park question of #{n} off the record\n")),
            }
        }
        if retry {
            match store::try_call(home, 3, &["park-retry", n]) {
                Err(no) if no.word != "unparked" => {
                    eprintln!(
                        "tehas stop: the retry of the park of #{n} could not be spent ({}), so nothing was removed. Run this again.",
                        no.line
                    );
                    return Ok(1);
                }
                _ => say(&format!("spent the retry of the park of #{n}\n")),
            }
        }

        let mut had_lock = false;
        for p in &present {
            if *p == lock {
                had_lock = true;
            } else if remove(p) {
                self.removed(p);
            } else {
                eprintln!("tehas stop: could not remove {p}");
                rc = 1;
            }
        }
        // Removed whatever it was before, so said only when it was here to
        // begin with.
        match fs::remove_file(&lock) {
            Ok(()) if had_lock => self.removed(&lock),
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => {
                if had_lock {
                    self.removed(&lock);
                }
            }
            Err(_) => {
                eprintln!("tehas stop: could not remove {lock}");
                rc = 1;
            }
        }
        drop(held);

        // Only when nothing failed: the running mark comes off an issue
        // whose cycle really ended.
        if rc == 0 {
            rc = said(self.take_label(tickets));
        }
        Ok(rc)
    }
}

/// The issue's half said, and its exit status: 0, or 1 for a line of
/// standard error.
fn said(half: Result<Option<String>, String>) -> u8 {
    match half {
        Ok(line) => {
            if let Some(line) = line {
                say(&format!("{line}\n"));
            }
            0
        }
        Err(line) => {
            eprintln!("{line}");
            1
        }
    }
}

/// `TEHAS_REPO` as `tehas_require` asks for it: a line that says why not.
fn repo_refusal(repo: &str) -> Option<&'static str> {
    if repo.trim_matches(' ').is_empty() {
        return Some("is not set");
    }
    if is_repo(repo) {
        None
    } else {
        Some("is not valid: expected owner/name")
    }
}

fn setting(name: &str) -> String {
    env::var(name).unwrap_or_default()
}

/// `tickets` makes the handle of the repository it is given, each call in
/// the bound it is given; it is asked once the settings are known good.
pub fn main(given: &[String], tickets: impl FnOnce(&str, Duration) -> Box<dyn TicketWrites>) -> u8 {
    let asked = match parse(given) {
        Ok(Some(asked)) => asked,
        Ok(None) => return 0,
        Err(rc) => return rc,
    };
    let repo = setting("TEHAS_REPO");
    if let Some(why) = repo_refusal(&repo) {
        let dir = env::var("TEHAS_INSTANCE_DIR").unwrap_or_else(|_| "?".to_string());
        eprintln!("tehas: TEHAS_REPO {why} (see {dir}/instance.env)");
        return 78;
    }
    let names = Names {
        home: setting("TEHAS_HOME"),
        user_home: setting("HOME"),
        repo,
        work_dir: setting("TEHAS_WORK_DIR"),
        n: asked.issue,
    };
    if names.home.is_empty() || names.user_home.is_empty() || names.work_dir.is_empty() {
        eprintln!("tehas stop: TEHAS_HOME, HOME and TEHAS_WORK_DIR must be set");
        return 78;
    }
    if !store::ready(Path::new(&names.home)) {
        eprintln!("tehas stop: ERROR: the state store is not installed (run install.sh)");
        return 78;
    }
    let events = wiring(Path::new(&names.home));
    let mut tickets = tickets(&names.repo, BOUND);
    names
        .run(asked.keep, &events, &mut *tickets)
        .unwrap_or_else(|rc| rc)
}

/// The dispatcher of `tehas stop`: the log listener alone, which writes to
/// the factory's log, or says the line when that cannot be written.
fn wiring(home: &Path) -> Dispatcher {
    let log = Home::new(home).factory_log();
    raise::logged(move |line| {
        let at = tehas_core::time::iso(tehas_core::time::now());
        // One write of the whole line, so O_APPEND keeps it whole beside the
        // other writers of the log.
        let whole = format!("{at} {line}\n");
        let wrote = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log)
            .and_then(|mut f| f.write_all(whole.as_bytes()));
        if wrote.is_err() {
            eprintln!("tehas stop: {line}");
        }
    })
    .build()
}

/// The end of attempt `attempt` of issue `n`, raised when `answer`, the
/// store's answer to `cycle-end --stopped`, says this run wrote its line. A
/// record ended before, by a report, the poll or a refused start, is its
/// writer's to raise. A read that fails raises nothing, and says so.
fn raise_stopped(home: &Path, n: &str, attempt: &str, answer: &str, events: &Dispatcher) {
    let wrote = serde_json::from_str::<raise::EndAnswer>(answer).is_ok_and(|a| !a.already);
    let (true, Ok(issue), Ok(attempt)) = (wrote, n.parse(), attempt.parse()) else {
        return;
    };
    let warn = |line: &str| eprintln!("tehas stop: {line}");
    let mut ask = |args: &[&str]| store::line(home, args).map_err(|r| r.word);
    match raise::read_ended(&mut ask, issue, attempt, Writer::Short) {
        Ok(e) => e.raise(events, &warn),
        Err(why) => warn(&format!(
            "WARN: the history of #{n} could not be read, so its end raises no event ({why})"
        )),
    }
}

/// What `tehas stop` says of the record it asked the store to end. One the
/// store had ended already, as the poll ends a quiet cycle as lost in the
/// window between its lock going free and this call, says how it ended.
fn ended_line(n: &str, already: bool, reason: Option<&str>) -> String {
    if already {
        let how = reason.map_or(String::new(), |r| format!(" as {r}"));
        format!("the record of the cycle of #{n} was already ended{how}\n")
    } else {
        format!("ended the record of the cycle of #{n} as stopped\n")
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_record_ended_already_is_said_as_it_ended() {
        use super::ended_line;
        assert_eq!(
            ended_line("5", false, Some("stopped")),
            "ended the record of the cycle of #5 as stopped\n"
        );
        assert_eq!(
            ended_line("5", true, Some("lost")),
            "the record of the cycle of #5 was already ended as lost\n"
        );
        assert_eq!(
            ended_line("5", true, None),
            "the record of the cycle of #5 was already ended\n"
        );
    }

    use super::*;
    use std::os::unix::fs::symlink;
    use tehas_core::testdir::TestDir;

    // A home with a store of its own, made by the store's init.
    fn store_home(name: &str) -> TestDir {
        let home = TestDir::new(name);
        fs::create_dir_all(Home::new(&*home).cycles_dir()).unwrap();
        fs::create_dir_all(home.join("libexec")).unwrap();
        tehas_core::testing::write_exe(store::program(&home), "#!/bin/sh\nexit 1\n").unwrap();
        let init = [
            "init",
            "--version",
            "v0.0.0",
            "--commit",
            "0123456789abcdef0123456789abcdef01234567",
            "--instance",
            "instance",
            "--repo",
            "acme/widget",
        ];
        store::call(&home, &init).unwrap();
        home
    }

    // A start of #5 by hand, under the lock a cycle holds. Unlocked by name:
    // a child another case forks this instant shares the descriptor until it
    // execs, so a drop alone may not free it before the next start takes it.
    fn started(home: &Path) {
        let lock = File::create(Home::new(home).cycle_note(5, "lock")).unwrap();
        lock.try_lock().unwrap();
        store::call(home, &["cycle-start", "5", "--by-hand"]).unwrap();
        lock.unlock().unwrap();
    }

    #[test]
    fn a_record_this_run_ended_raises_its_end_and_one_a_report_ended_raises_none() {
        use std::sync::{Arc, Mutex};
        use tehas_ports::events::{End, Envelope, Event};
        let home = store_home("stop-raise");
        let heard = Arc::new(Mutex::new(Vec::new()));
        let h = Arc::clone(&heard);
        let events = raise::logged(move |l| h.lock().unwrap().push(l.to_string())).build();
        started(&home);
        let stop = |attempt: &str| {
            let end = ["cycle-end", "5", "--attempt", attempt, "--stopped"];
            store::call(&home, &end).unwrap().to_string()
        };
        raise_stopped(&home, "5", "1", &stop("1"), &events);
        let lines = heard.lock().unwrap().clone();
        assert_eq!(lines.len(), 1, "{lines:?}");
        let e: Envelope = serde_json::from_str(lines[0].strip_prefix("event ").unwrap()).unwrap();
        let Event::CycleEnded(p) = &e.event else {
            panic!("{e:?}")
        };
        assert_eq!((e.key.as_str(), p.end), ("5-1", End::Stopped));
        // A cycle that reported first wrote the line, and raised it.
        started(&home);
        let report = ["cycle-end", "5", "--attempt", "2", "--status", "failed"];
        store::call(&home, &report).unwrap();
        raise_stopped(&home, "5", "2", &stop("2"), &events);
        assert_eq!(heard.lock().unwrap().len(), 1);
        // A record a refused start ended has a line no report wrote, as the
        // stop's would be: the store's `already` alone keeps this run from
        // raising it a second time.
        let lock = File::create(Home::new(&*home).cycle_note(5, "lock")).unwrap();
        lock.try_lock().unwrap();
        store::call(&home, &["cycle-start", "5", "--by-hand"]).unwrap();
        let refused = [
            "cycle-end",
            "5",
            "--attempt",
            "3",
            "--refused",
            "--rc",
            "65",
        ];
        store::call(&home, &refused).unwrap();
        lock.unlock().unwrap();
        let rows = store::call(&home, &["history-show", "5"]).unwrap()["rows"].clone();
        assert_eq!(rows[2]["reported"], false);
        let answer = stop("3");
        assert!(answer.contains("\"already\":true"), "{answer}");
        raise_stopped(&home, "5", "3", &answer, &events);
        assert_eq!(heard.lock().unwrap().len(), 1);
        // An answer that does not say who wrote raises nothing.
        raise_stopped(&home, "5", "3", "{}", &events);
        assert_eq!(heard.lock().unwrap().len(), 1);
    }

    fn words(given: &[&str]) -> Vec<String> {
        given.iter().map(|w| w.to_string()).collect()
    }

    fn text(p: std::path::PathBuf) -> String {
        p.display().to_string()
    }

    /// The path of five's note that ends in `end`, `.lock` say.
    fn note(end: &str) -> String {
        let end = end.strip_prefix('.').unwrap();
        text(Home::new("/h/tehas").cycle_note(5, end))
    }

    fn names(n: &str) -> Names {
        Names {
            home: "/h/tehas".to_string(),
            user_home: "/h".to_string(),
            repo: "acme/widget".to_string(),
            work_dir: "/h/tehas/work/widget".to_string(),
            n: n.to_string(),
        }
    }

    fn at(dir: &TestDir, name: &str) -> String {
        dir.join(name).to_string_lossy().into_owned()
    }

    #[test]
    fn an_issue_is_one_to_ten_digits_with_no_leading_zero() {
        for good in ["1", "5", "1234567890"] {
            assert!(is_issue(good), "{good}");
        }
        for bad in ["", "0", "05", "5a", "12345678901", "5\n", " 5", "٥"] {
            assert!(!is_issue(bad), "{bad:?}");
        }
    }

    #[test]
    fn the_words_are_read_left_to_right() {
        let asked = |issue: &str, keep| {
            Ok(Some(Asked {
                issue: issue.to_string(),
                keep,
            }))
        };
        assert_eq!(parse(&words(&["5"])), asked("5", false));
        assert_eq!(parse(&words(&["--keep-workspace", "5"])), asked("5", true));
        // An empty word is no issue yet, so the next one is the first.
        assert_eq!(parse(&words(&["", "5"])), asked("5", false));
        assert_eq!(parse(&words(&["5", "--help"])), Ok(None));
        assert_eq!(parse(&words(&["-h", "x", "y"])), Ok(None));
        for bad in [
            &[][..],
            &[""],
            &["5", "6"],
            &["5", "6", "-h"],
            &["5", ""],
            &["5", "--keep"],
            &["-"],
            &["-5"],
            &["0"],
            &["5a"],
            &["--keep-workspace"],
        ] {
            assert_eq!(parse(&words(bad)), Err(64), "{bad:?}");
        }
    }

    #[test]
    fn the_paths_of_five_are_nothing_of_fifty_one() {
        let five = names("5");
        let fifty_one = names("51");
        let all = five.candidates(false);
        assert_eq!(all.len(), cycle_notes::stopped().count() + 6);
        for path in &all {
            assert!(!path.contains('*'), "{path}");
            assert!(!fifty_one.candidates(false).contains(path), "{path}");
        }
        let home = Home::new("/h/tehas");
        assert!(all.contains(&text(home.cycle_note(5, "lock"))));
        assert!(all.contains(&text(home.live_console(5))));
        assert!(all.contains(&"/h/.pi/agent/ensemble-runs/issue-5".to_string()));
        assert!(all.contains(&"/h/tehas/work/widget-5".to_string()));
        assert!(all.contains(&"/h/tehas/work/widget-5.before-retry".to_string()));
        assert_eq!(fifty_one.unit(), "tehas-cycle@51.service");
        assert_eq!(fifty_one.container(), "tehas-work-51");
    }

    #[test]
    fn a_stop_takes_every_note_but_the_record() {
        let all = names("5").candidates(true);
        for end in [
            ".disk",
            ".comments",
            ".signal",
            ".freeze",
            ".frozen",
            ".status.new",
        ] {
            assert!(all.contains(&note(end)), "{end}");
        }
    }

    #[test]
    fn a_kept_workspace_is_not_a_candidate() {
        let kept = names("5").candidates(true);
        assert_eq!(kept.len(), cycle_notes::stopped().count() + 2);
        assert!(kept.iter().all(|p| !p.contains("/work/")));
    }

    #[test]
    fn the_candidates_keep_the_cycles_record() {
        let all = names("5").candidates(false);
        for end in [".json", ".manifest.json", ".runner_exit", ".status"] {
            assert!(!all.contains(&note(end)));
        }
    }

    #[test]
    fn a_path_is_said_the_way_a_person_types_it() {
        let five = names("5");
        assert_eq!(
            five.short(&text(Home::new("/h/tehas").cycle_note(5, "lock"))),
            text(Home::new("").cycle_note(5, "lock"))
        );
        assert_eq!(five.short("/h/.pi/x"), "~/.pi/x");
        assert_eq!(five.short("/other/x"), "/other/x");
    }

    #[test]
    fn a_marker_is_seconds_and_one_account() {
        assert!(marker_reads("1793000000 alice\n"));
        assert!(marker_reads("0 a.b_c-d\n"));
        assert!(marker_reads("1793000000 alice extra\n"));
        for bad in [
            "",
            "1793000000 alice",
            "1793000000\n",
            "1793000000 \n",
            "01 alice\n",
            "12345678901 alice\n",
            "x alice\n",
            "1 al/ice\n",
        ] {
            assert!(!marker_reads(bad), "{bad:?}");
        }
        assert!(!marker_reads(&format!("1 {}\n", "a".repeat(65))));
    }

    #[test]
    fn a_marker_that_cannot_land_leaves_no_name_of_its_write() {
        let dir = TestDir::new("marker");
        let marker = at(&dir, "5.stopped");
        fs::create_dir(&marker).unwrap();
        assert!(!write_marker(&marker));
        assert!(!Path::new(&format!("{marker}.new")).exists());
        assert!(Path::new(&marker).is_dir());
    }

    #[test]
    fn only_a_plain_file_or_no_file_is_a_lock() {
        let dir = TestDir::new("lock");
        let plain = at(&dir, "1.lock");
        fs::write(&plain, "").unwrap();
        assert!(plain_or_absent(&plain));
        assert!(plain_or_absent(&at(&dir, "2.lock")));
        let linked = at(&dir, "3.lock");
        symlink(&plain, &linked).unwrap();
        assert!(!plain_or_absent(&linked));
        let dangling = at(&dir, "4.lock");
        symlink(at(&dir, "nothing"), &dangling).unwrap();
        assert!(!plain_or_absent(&dangling));
        let directory = at(&dir, "5.lock");
        fs::create_dir(&directory).unwrap();
        assert!(!plain_or_absent(&directory));
    }

    #[test]
    fn a_lock_file_is_never_opened_through_a_link() {
        let dir = TestDir::new("nofollow");
        let target = at(&dir, "target");
        fs::write(&target, "a person reads this").unwrap();
        let link = at(&dir, "6.lock");
        symlink(&target, &link).unwrap();
        assert!(take_lock_file(&link).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "a person reads this");
    }

    #[test]
    fn a_link_is_removed_and_not_followed() {
        let dir = TestDir::new("remove");
        let inside = at(&dir, "target");
        fs::create_dir(&inside).unwrap();
        fs::write(at(&dir, "target/kept"), "x").unwrap();
        let link = at(&dir, "link");
        symlink(&inside, &link).unwrap();
        assert!(remove(&link));
        assert!(fs::symlink_metadata(&link).is_err());
        assert!(Path::new(&at(&dir, "target/kept")).exists());
        assert!(remove(&inside));
        assert!(!Path::new(&inside).exists());
        assert!(remove(&inside), "a name not there is gone already");
    }

    mod label {
        //! The issue's half, on the ports' fake: the running mark comes off
        //! when it is on and no cycle holds the lock again, and what could
        //! not be done is said in stop's own words.

        use super::*;
        use tehas_ports::fake::{FakeSite, Fault, Faulty, Op, TicketsOp, World};
        use tehas_ports::{Act, Actor, Caller, Site, Write};

        /// The running mark's label, as stop's lines name it.
        const LABEL: &str = "tehas:running";

        fn running() -> Mark {
            Mark::State(State::Running)
        }

        /// #5 of a home of its own, so its lock is a file this test holds.
        fn five(dir: &TestDir) -> Names {
            Names {
                home: dir.to_string_lossy().into_owned(),
                ..names("5")
            }
        }

        fn take(names: &Names, site: &impl Site) -> Result<Option<String>, String> {
            let mut t = site
                .ticket_writes(&Actor::Operator, Caller::new("stop"))
                .unwrap();
            names.take_label(&mut *t)
        }

        fn unmarked(n: u64) -> Write {
            Write::Unmark {
                ticket: TicketKey::Number(NonZeroU64::new(n).unwrap()),
                mark: running(),
            }
        }

        /// The lock of #5, held until the value drops: a cycle that started
        /// while the host was cleared.
        fn hold(names: &Names) -> File {
            fs::create_dir_all(names.state()).unwrap();
            let f = File::create(names.lock()).unwrap();
            f.try_lock().unwrap();
            f
        }

        #[test]
        fn the_mark_on_the_issue_comes_off_with_one_read_and_one_write() {
            let dir = TestDir::new("stop-label-on");
            let w = World::new().ticket(5, |t| {
                t.mark(Mark::Project(tehas_ports::MarkName::new("bug").unwrap()))
                    .mark(running())
                    .mark(Mark::Act(Act::Pause))
            });
            assert_eq!(
                take(&five(&dir), &FakeSite::new(&w)),
                Ok(Some(format!("removed {LABEL} from #5")))
            );
            let wrote: Vec<Write> = w.wrote().into_iter().map(|x| x.what).collect();
            assert_eq!(wrote, vec![unmarked(5)]);
            assert_eq!(w.sent(), 2);
        }

        #[test]
        fn a_mark_not_on_the_issue_is_read_and_nothing_is_written_or_said() {
            let dir = TestDir::new("stop-label-off");
            let w = World::new().ticket(5, |t| t.mark(Mark::Act(Act::Pause)));
            assert_eq!(take(&five(&dir), &FakeSite::new(&w)), Ok(None));
            assert_eq!(w.wrote(), vec![]);
            assert_eq!(w.sent(), 1);
        }

        #[test]
        fn of_five_and_fifty_one_only_five_is_asked_about_and_unmarked() {
            let dir = TestDir::new("stop-label-51");
            let w = World::new()
                .ticket(5, |t| t.mark(running()))
                .ticket(51, |t| t.mark(running()));
            let site = FakeSite::new(&w);
            assert_eq!(
                take(&five(&dir), &site),
                Ok(Some(format!("removed {LABEL} from #5")))
            );
            let wrote: Vec<Write> = w.wrote().into_iter().map(|x| x.what).collect();
            assert_eq!(wrote, vec![unmarked(5)]);
            assert_eq!(w.sent(), 2);
            let mut t = site.tickets(&Actor::Operator, Caller::new("stop")).unwrap();
            let key = TicketKey::Number(NonZeroU64::new(51).unwrap());
            let (_, of_51) = t.several(&[key]).pop().unwrap();
            assert!(of_51.unwrap().marks.contains(&running()));
        }

        #[test]
        fn marks_that_cannot_be_read_are_a_warning_and_no_write() {
            let dir = TestDir::new("stop-label-unread");
            let w = World::new().ticket(5, |t| t.mark(running()));
            let slow =
                Faulty::new(FakeSite::new(&w)).on(Op::Tickets(TicketsOp::Several), Fault::Timeout);
            let warn =
                format!("WARN: the labels of #5 could not be read, so {LABEL} stays on the issue");
            assert_eq!(take(&five(&dir), &slow), Err(warn.clone()));
            for fault in [Fault::Unreachable, Fault::Forbidden, Fault::Malformed] {
                let failed =
                    Faulty::new(FakeSite::new(&w)).on(Op::Tickets(TicketsOp::Several), fault);
                assert_eq!(take(&five(&dir), &failed), Err(warn.clone()));
            }
            assert_eq!(w.wrote(), vec![]);
        }

        /// The line of a number that is not an issue of this project.
        fn not_an_issue(n: u64) -> Result<Option<String>, String> {
            Err(format!("#{n} is not an issue; tehas stops only issues"))
        }

        #[test]
        fn a_changes_number_is_not_an_issue_and_nothing_is_written() {
            let dir = TestDir::new("stop-label-change");
            let w = World::new().change(5, "work/5", |c| c);
            let answer = take(&five(&dir), &FakeSite::new(&w));
            assert_eq!(answer, not_an_issue(5));
            assert!(!answer.unwrap_err().contains(LABEL));
            assert_eq!(w.wrote(), vec![]);
            assert_eq!(w.sent(), 1);
        }

        #[test]
        fn a_number_with_no_item_is_not_an_issue_and_nothing_is_written() {
            let dir = TestDir::new("stop-label-none");
            let w = World::new().ticket(51, |t| t.mark(running()));
            assert_eq!(take(&five(&dir), &FakeSite::new(&w)), not_an_issue(5));
            assert_eq!(w.wrote(), vec![]);
            assert_eq!(w.sent(), 1);
        }

        #[test]
        fn a_mark_that_cannot_be_taken_off_is_a_warning() {
            let dir = TestDir::new("stop-label-refused");
            let w = World::new().ticket(5, |t| t.mark(running()));
            let refused =
                Faulty::new(FakeSite::new(&w)).on(Op::Tickets(TicketsOp::Unmark), Fault::Timeout);
            assert_eq!(
                take(&five(&dir), &refused),
                Err(format!("WARN: {LABEL} could not be taken off #5"))
            );
            assert_eq!(w.wrote(), vec![]);
        }

        #[test]
        fn a_cycle_that_holds_the_lock_again_keeps_its_mark_and_is_asked_after_the_read() {
            let dir = TestDir::new("stop-label-again");
            let names = five(&dir);
            let _lock = hold(&names);
            let on = World::new().ticket(5, |t| t.mark(running()));
            assert_eq!(
                take(&names, &FakeSite::new(&on)),
                Err(format!(
                    "WARN: a cycle for #5 holds its lock again, so {LABEL} stays on the issue"
                ))
            );
            assert_eq!(on.wrote(), vec![]);
            assert_eq!(on.sent(), 1);
            // The lock is asked about only for a mark that is on: a read that
            // finds none says nothing of a cycle.
            let off = World::new().ticket(5, |t| t);
            assert_eq!(take(&names, &FakeSite::new(&off)), Ok(None));
        }

        #[test]
        fn a_warning_is_exit_1_and_the_rest_is_0() {
            assert_eq!(said(Ok(None)), 0);
            assert_eq!(said(Ok(Some("removed".to_string()))), 0);
            assert_eq!(said(Err("WARN".to_string())), 1);
        }
    }

    #[test]
    fn the_repository_is_refused_as_the_settings_refuse_it() {
        assert_eq!(repo_refusal("acme/widget"), None);
        assert_eq!(repo_refusal("a.b_c/d-e"), None);
        assert_eq!(repo_refusal(""), Some("is not set"));
        assert_eq!(repo_refusal("   "), Some("is not set"));
        for bad in ["acme", "/x", "x/", "a/b/c", "a b/c"] {
            assert_eq!(repo_refusal(bad), Some("is not valid: expected owner/name"));
        }
    }

    #[test]
    fn a_park_field_reads_as_jq_prints_it() {
        use serde_json::json;
        assert_eq!(field_or_empty(&json!(null)), "");
        assert_eq!(field_or_empty(&json!(false)), "");
        assert_eq!(field_or_empty(&json!(425)), "425");
        assert_eq!(field_or_empty(&json!("425")), "425");
    }
}
