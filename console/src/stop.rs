//! `tehas stop <issue>`: end a cycle by hand and clear what it left on this
//! host. src/usage/stop.txt is the text a person reads; this says what the
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
//! Every path is the settings and the number, as a string, never a glob. The
//! calls of systemctl, docker and gh keep the bounds of `timeout` they had in
//! bash, and gh is the one on PATH.
//!
//! Exit: see src/usage/stop.txt.

use crate::{flags, store};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Write as _};
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};
use tehas_core::lock::{probe, Probe};

pub const USAGE: &str = include_str!("usage/stop.txt");

/// The markers that say an issue's cycle runs, owes something, or is parked,
/// after `state/cycles/<issue>`. `.nostart.new` is there because a note is
/// written beside its name and moved into place: a stop that takes the note
/// takes a write that never finished, as libexec/work's own clear does.
const NOTES: [&str; 13] = [
    ".lock",
    ".cid",
    ".done",
    ".stalled",
    ".timed",
    ".stopped",
    ".oom",
    ".forks",
    ".nostart",
    ".nostart.new",
    ".lean-refused",
    ".progress.json",
    ".result.json",
];

/// What the words of a run asked for.
#[derive(Debug, PartialEq, Eq)]
pub struct Asked {
    issue: String,
    keep: bool,
}

fn is_issue(word: &str) -> bool {
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
                print!("{USAGE}");
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
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let wrote = fs::write(&new, format!("{at} {}\n", flags::account())).is_ok()
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
    fn state(&self) -> String {
        format!("{}/state/cycles", self.home)
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
    /// it, named the way libexec/work names them.
    fn candidates(&self, keep: bool) -> Vec<String> {
        let mut out: Vec<String> = NOTES.iter().map(|end| self.note(end)).collect();
        out.push(format!("{}/live/console-{}.txt", self.home, self.n));
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

    /// The last step, the issue's half: 0, or 1 when the label could not be
    /// read or taken off, or a cycle holds the lock again and owns the label.
    fn take_label(&self) -> u8 {
        let n = &self.n;
        let view = [
            "issue",
            "view",
            n,
            "--repo",
            &self.repo,
            "--json",
            "labels",
            "--jq",
            ".labels[].name",
        ];
        let Some(labels) = read("10", "gh", &view) else {
            eprintln!(
                "WARN: the labels of #{n} could not be read, so tehas:running stays on the issue"
            );
            return 1;
        };
        if !labels.split('\n').any(|l| l == "tehas:running") {
            return 0;
        }
        if self.running() {
            eprintln!(
                "WARN: a cycle for #{n} holds its lock again, so tehas:running stays on the issue"
            );
            return 1;
        }
        let edit = [
            "issue",
            "edit",
            n,
            "--repo",
            &self.repo,
            "--remove-label",
            "tehas:running",
        ];
        if quiet("10", "gh", &edit) {
            say(&format!("removed tehas:running from #{n}\n"));
            0
        } else {
            eprintln!("WARN: tehas:running could not be taken off #{n}");
            1
        }
    }

    fn run(&self, keep: bool) -> Result<u8, u8> {
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
            if let Err(no) = store::try_call(home, 3, &end) {
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
            say(&format!(
                "ended the record of the cycle of #{n} as stopped\n"
            ));
        }
        if !held.as_ref().is_ok_and(|f| f.try_lock().is_ok()) {
            eprintln!("tehas stop: a cycle for #{n} holds its lock, so nothing was removed.");
            return Ok(1);
        }

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

        // Only when nothing failed: tehas:running comes off an issue whose
        // cycle really ended.
        if rc == 0 {
            rc = self.take_label();
        }
        Ok(rc)
    }
}

/// `TEHAS_REPO` as `tehas_require` asks for it: a line that says why not.
fn repo_refusal(repo: &str) -> Option<&'static str> {
    if repo.trim_matches(' ').is_empty() {
        return Some("is not set");
    }
    let part = |p: &str| {
        !p.is_empty()
            && p.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
    };
    match repo.split_once('/') {
        Some((owner, name)) if part(owner) && part(name) => None,
        _ => Some("is not valid: expected owner/name"),
    }
}

fn setting(name: &str) -> String {
    env::var(name).unwrap_or_default()
}

pub fn main(given: &[String]) -> u8 {
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
    names.run(asked.keep).unwrap_or_else(|rc| rc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;

    fn words(given: &[&str]) -> Vec<String> {
        given.iter().map(|w| w.to_string()).collect()
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

    struct Dir(PathBuf);

    impl Dir {
        fn new(name: &str) -> Dir {
            let dir = env::temp_dir().join(format!("stop-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Dir(dir)
        }

        fn path(&self, name: &str) -> String {
            self.0.join(name).to_string_lossy().into_owned()
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
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
        assert_eq!(all.len(), NOTES.len() + 6);
        for path in &all {
            assert!(!path.contains('*'), "{path}");
            assert!(!fifty_one.candidates(false).contains(path), "{path}");
        }
        assert!(all.contains(&"/h/tehas/state/cycles/5.lock".to_string()));
        assert!(all.contains(&"/h/tehas/live/console-5.txt".to_string()));
        assert!(all.contains(&"/h/.pi/agent/ensemble-runs/issue-5".to_string()));
        assert!(all.contains(&"/h/tehas/work/widget-5".to_string()));
        assert!(all.contains(&"/h/tehas/work/widget-5.before-retry".to_string()));
        assert_eq!(fifty_one.unit(), "tehas-cycle@51.service");
        assert_eq!(fifty_one.container(), "tehas-work-51");
    }

    #[test]
    fn a_kept_workspace_is_not_a_candidate() {
        let kept = names("5").candidates(true);
        assert_eq!(kept.len(), NOTES.len() + 2);
        assert!(kept.iter().all(|p| !p.contains("/work/")));
    }

    #[test]
    fn the_candidates_keep_the_cycles_record() {
        let all = names("5").candidates(false);
        for end in [".json", ".manifest.json", ".runner_exit", ".status"] {
            assert!(!all.contains(&format!("/h/tehas/state/cycles/5{end}")));
        }
    }

    #[test]
    fn a_path_is_said_the_way_a_person_types_it() {
        let five = names("5");
        assert_eq!(
            five.short("/h/tehas/state/cycles/5.lock"),
            "state/cycles/5.lock"
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
        let dir = Dir::new("marker");
        let marker = dir.path("5.stopped");
        fs::create_dir(&marker).unwrap();
        assert!(!write_marker(&marker));
        assert!(!Path::new(&format!("{marker}.new")).exists());
        assert!(Path::new(&marker).is_dir());
    }

    #[test]
    fn only_a_plain_file_or_no_file_is_a_lock() {
        let dir = Dir::new("lock");
        let plain = dir.path("1.lock");
        fs::write(&plain, "").unwrap();
        assert!(plain_or_absent(&plain));
        assert!(plain_or_absent(&dir.path("2.lock")));
        let linked = dir.path("3.lock");
        symlink(&plain, &linked).unwrap();
        assert!(!plain_or_absent(&linked));
        let dangling = dir.path("4.lock");
        symlink(dir.path("nothing"), &dangling).unwrap();
        assert!(!plain_or_absent(&dangling));
        let directory = dir.path("5.lock");
        fs::create_dir(&directory).unwrap();
        assert!(!plain_or_absent(&directory));
    }

    #[test]
    fn a_lock_file_is_never_opened_through_a_link() {
        let dir = Dir::new("nofollow");
        let target = dir.path("target");
        fs::write(&target, "a person reads this").unwrap();
        let link = dir.path("6.lock");
        symlink(&target, &link).unwrap();
        assert!(take_lock_file(&link).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "a person reads this");
    }

    #[test]
    fn a_link_is_removed_and_not_followed() {
        let dir = Dir::new("remove");
        let inside = dir.path("target");
        fs::create_dir(&inside).unwrap();
        fs::write(dir.path("target/kept"), "x").unwrap();
        let link = dir.path("link");
        symlink(&inside, &link).unwrap();
        assert!(remove(&link));
        assert!(fs::symlink_metadata(&link).is_err());
        assert!(Path::new(&dir.path("target/kept")).exists());
        assert!(remove(&inside));
        assert!(!Path::new(&inside).exists());
        assert!(remove(&inside), "a name not there is gone already");
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
