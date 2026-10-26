//! What libexec/review decides and how it runs the container, for
//! `tehasbot review`. The shell prepares the sandbox; this reads the range,
//! checks what the image is given, and supervises the one run.
//!
//!   console review since <checkout> <branch> <record>
//!     The range to review: prints `<head> <since> <from>`, and what it says
//!     about it on standard error. Exit 0 a range,
//!     3 nothing new since the record, 2 the checkout has no origin/<branch>.
//!   console review refused <name>...  (the image's --capabilities on stdin)
//!     The names of the container's variables the image does not take, one a
//!     line. Exit 0.
//!   console review run [--private <name>]... --branch <b> --head <commit>
//!     --since <commit> --record <file> --limit <s> --stop-wait <s>
//!     --name <container> --doc <file> --log <file> [--remove <dir>]...
//!     -- <docker run ...>
//!     One run, bounded by --limit; its document and log masked; the record
//!     moved on for an answer. Exit the runner's 0, 10 or 20, else 2.

use serde_json::Value;
use std::fs;
use std::io::{self, Write};
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::sleep;
use std::time::{Duration, Instant};
use tehas_core::{mask, time};

// How far back a host that never reviewed anything reads.
const DAYS: u32 = 7;
const TICK: Duration = Duration::from_millis(100);

// The names a review's container may be given.
const WANTED: &[&str] = &[
    "PI_ENSEMBLE_SANDBOX_MODE",
    "PI_ENSEMBLE_WORKSPACE_ROOT",
    "CBM_CACHE_DIR",
    "GIT_CONFIG_COUNT",
    "GIT_CONFIG_KEY_0",
    "GIT_CONFIG_KEY_1",
    "GIT_CONFIG_KEY_2",
    "GIT_CONFIG_KEY_3",
    "GIT_CONFIG_VALUE_0",
    "GIT_CONFIG_VALUE_1",
    "GIT_CONFIG_VALUE_2",
    "GIT_CONFIG_VALUE_3",
    "GIT_TERMINAL_PROMPT",
    "PI_ENSEMBLE_SUBAGENT_MODEL",
    "PI_ENSEMBLE_SUBAGENT_PROVIDER",
];

pub fn main(given: &[String]) -> u8 {
    match given.first().map(String::as_str) {
        Some("since") if given.len() == 4 => since(&given[1], &given[2], &given[3]),
        Some("refused") => {
            let mut caps = String::new();
            let _ = io::Read::read_to_string(&mut io::stdin(), &mut caps);
            for name in refused(&caps, &given[1..]) {
                println!("{name}");
            }
            0
        }
        Some("run") => match RunArgs::parse(&given[1..]) {
            Ok(args) => run(&args),
            Err(e) => {
                eprintln!("console review: {e}");
                2
            }
        },
        _ => {
            eprintln!("console review: usage: since|refused|run, see console/src/review.rs");
            2
        }
    }
}

fn say(line: &str) {
    let _ = writeln!(io::stdout(), "tehas review: {line}");
}

fn warn(line: &str) {
    let _ = writeln!(io::stderr(), "tehas review: {line}");
}

fn short(commit: &str) -> &str {
    &commit[..commit.len().min(12)]
}

/// git's standard output, trimmed; None when git fails. What it said on
/// standard error is added to `said`.
fn git(repo: &str, args: &[&str], said: &mut String) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    said.push_str(&String::from_utf8_lossy(&out.stderr));
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The commit of a record: a full commit id, else nothing.
fn recorded(record: &str) -> Option<String> {
    let doc: Value = serde_json::from_str(&fs::read_to_string(record).ok()?).ok()?;
    let commit = doc.get("commit")?.as_str()?;
    (commit.len() == 40
        && commit
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
    .then(|| commit.to_string())
}

// The start is the recorded commit when the tip still reaches it (a
// force-push makes it no start), else the commit 7 days back, else the root.
fn since(repo: &str, branch: &str, record: &str) -> u8 {
    let mut ignored = String::new();
    let Some(head) = git(
        repo,
        &[
            "rev-parse",
            "-q",
            "--verify",
            &format!("refs/remotes/origin/{branch}^{{commit}}"),
        ],
        &mut ignored,
    ) else {
        warn(&format!(
            "{repo} has no origin/{branch}, nothing was reviewed"
        ));
        return 2;
    };
    let mut start = recorded(record);
    if let Some(commit) = &start {
        if git(
            repo,
            &["merge-base", "--is-ancestor", commit, &head],
            &mut ignored,
        )
        .is_none()
        {
            warn(&format!(
                "{record} names {}, which origin/{branch} no longer reaches: reading the {DAYS} days instead",
                short(commit)
            ));
            start = None;
        }
    }
    let (start, from) = match start {
        Some(commit) => (commit, "record".to_string()),
        None => {
            // A git that fails here is said as itself, not read as a young
            // branch.
            let mut said = String::new();
            let before = format!("--before={DAYS}.days ago");
            let older =
                git(repo, &["rev-list", "-1", &before, &head], &mut said).unwrap_or_default();
            let found = if !older.is_empty() && older != head {
                (older, format!("{DAYS} days"))
            } else {
                let root = git(repo, &["rev-list", "--max-parents=0", &head], &mut said)
                    .and_then(|roots| roots.lines().last().map(str::to_string))
                    .unwrap_or_else(|| head.clone());
                let from = if root == head {
                    "root".to_string()
                } else {
                    format!("{DAYS} days")
                };
                (root, from)
            };
            let said = said.replace('\n', " ");
            if !said.trim().is_empty() {
                warn(&format!(
                    "git said: {}",
                    said.chars().take(200).collect::<String>()
                ));
            }
            found
        }
    };
    if start == head && Path::new(record).is_file() {
        warn(&format!(
            "branch={branch} nothing new since {}",
            short(&head)
        ));
        return 3;
    }
    println!("{head} {start} {from}");
    0
}

/// The variables, by name, that the image does not list as an input a host
/// may set. A name that is neither wanted nor of the PI_ENSEMBLE_* family is
/// not the runner's to refuse; nor are secrets and the inputs of a cycle or a
/// planning turn, which libexec/review unsets before the sandbox is made.
fn refused(caps: &str, names: &[String]) -> Vec<String> {
    let caps = serde_json::from_str::<Value>(caps).unwrap_or(Value::Null);
    let inputs: Vec<&str> = caps
        .get("inputs")
        .and_then(Value::as_array)
        .map(|inputs| {
            inputs
                .iter()
                .filter(|i| {
                    matches!(
                        i.get("scope").and_then(Value::as_str),
                        Some("cycle" | "sandbox")
                    )
                })
                .filter_map(|i| i.get("name").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    names
        .iter()
        .map(|n| n.split('=').next().unwrap_or(""))
        .filter(|n| *n != "-e" && !n.is_empty())
        .filter(|n| !WANTED.contains(n))
        .filter(|n| !(n.contains("TOKEN") || n.contains("SECRET") || n.ends_with("KEY")))
        .filter(|n| {
            !(n.starts_with("PI_ENSEMBLE_PLAN_")
                || n.starts_with("PI_ENSEMBLE_HANDOFF_")
                || matches!(
                    *n,
                    "PI_ENSEMBLE_EXTRA_IMAGE_DIRS"
                        | "PI_ENSEMBLE_CYCLE_DEADLINE"
                        | "PI_ENSEMBLE_VERIFY_CONCURRENCY"
                ))
        })
        .filter(|n| n.starts_with("PI_ENSEMBLE_"))
        .filter(|n| !inputs.contains(n))
        .map(str::to_string)
        .collect()
}

struct RunArgs {
    private: Vec<String>,
    branch: String,
    head: String,
    since: String,
    record: String,
    limit: u64,
    stop_wait: u64,
    name: String,
    doc: String,
    log: String,
    remove: Vec<String>,
    command: Vec<String>,
}

impl RunArgs {
    fn parse(given: &[String]) -> Result<RunArgs, String> {
        let split = given
            .iter()
            .position(|a| a == "--")
            .ok_or("run: no -- before the command")?;
        let (flags, command) = (&given[..split], &given[split + 1..]);
        if command.is_empty() {
            return Err("run: no command after --".to_string());
        }
        if flags.len() % 2 != 0 {
            return Err("run: a flag without its value".to_string());
        }
        let mut pairs: Vec<(&str, &str)> = Vec::new();
        for pair in flags.chunks(2) {
            pairs.push((pair[0].as_str(), pair[1].as_str()));
        }
        let all = |flag: &str| -> Vec<String> {
            pairs
                .iter()
                .filter(|(f, _)| *f == flag)
                .map(|(_, v)| v.to_string())
                .collect()
        };
        let one = |flag: &str| -> Result<String, String> {
            match all(flag).as_slice() {
                [v] if !v.is_empty() => Ok(v.clone()),
                _ => Err(format!("run: {flag} is required once")),
            }
        };
        let seconds = |flag: &str| -> Result<u64, String> {
            one(flag)?
                .parse()
                .map_err(|_| format!("run: {flag} is not a number of seconds"))
        };
        let known = [
            "--private",
            "--branch",
            "--head",
            "--since",
            "--record",
            "--limit",
            "--stop-wait",
            "--name",
            "--doc",
            "--log",
            "--remove",
        ];
        if let Some((flag, _)) = pairs.iter().find(|(f, _)| !known.contains(f)) {
            return Err(format!("run: unknown flag {flag}"));
        }
        Ok(RunArgs {
            private: all("--private"),
            branch: one("--branch")?,
            head: one("--head")?,
            since: one("--since")?,
            record: one("--record")?,
            limit: seconds("--limit")?,
            stop_wait: seconds("--stop-wait")?,
            name: one("--name")?,
            doc: one("--doc")?,
            log: one("--log")?,
            remove: all("--remove"),
            command: command.to_vec(),
        })
    }
}

/// A docker call that may not answer: None when it was still going after
/// `wait` seconds, and then it is killed.
fn bounded(args: &[&str], wait: u64) -> Option<ExitStatus> {
    let mut child = Command::new("docker")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    match wait_for(&mut child, Duration::from_secs(wait)) {
        Some(status) => Some(status),
        None => {
            let _ = child.kill();
            let _ = child.wait();
            None
        }
    }
}

fn wait_for(child: &mut Child, wait: Duration) -> Option<ExitStatus> {
    let until = Instant::now() + wait;
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Some(status);
        }
        if Instant::now() >= until {
            return None;
        }
        sleep(TICK);
    }
}

// docker stop, then docker kill, both bounded.
fn stop(args: &RunArgs, grace: u64) {
    if bounded(
        &["stop", "-t", &grace.to_string(), &args.name],
        args.stop_wait,
    )
    .is_some()
    {
        return;
    }
    warn(&format!(
        "docker stop of {} did not end within {}s, killing it",
        args.name, args.stop_wait
    ));
    if !bounded(&["kill", &args.name], args.stop_wait).is_some_and(|s| s.success()) {
        warn(&format!("docker kill of {} did not work either", args.name));
    }
}

// The client may outlive its container: after the stop's grace it is sent
// TERM, so one that traps it can clean up, then KILL.
fn reap(child: &mut Child, args: &RunArgs) -> ExitStatus {
    if let Some(status) = wait_for(child, Duration::from_secs(args.stop_wait)) {
        return status;
    }
    warn("the docker client of the review would not end, killing it");
    let _ = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status();
    if let Some(status) = wait_for(child, Duration::from_secs(2)) {
        return status;
    }
    let _ = child.kill();
    // A client that cannot be waited for is counted as killed.
    child.wait().unwrap_or_else(|_| ExitStatus::from_raw(9))
}

fn exit_of(status: ExitStatus) -> i32 {
    status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
}

/// A file an agent wrote, masked in place. An empty mask keeps the file as
/// it was, as tehas export does.
fn mask_file(path: &str) {
    let Ok(bytes) = fs::read(path) else { return };
    if bytes.is_empty() {
        return;
    }
    let masked = mask::mask(&String::from_utf8_lossy(&bytes));
    if masked.is_empty() {
        return;
    }
    let to = format!("{path}.masking");
    if fs::write(&to, masked)
        .and_then(|()| fs::rename(&to, path))
        .is_err()
    {
        let _ = fs::remove_file(&to);
        warn(&format!("could not replace {path}"));
    }
}

/// The document's own exitCode, when it has one.
fn doc_exit(doc: &str) -> Option<String> {
    let doc: Value = serde_json::from_str(&fs::read_to_string(doc).ok()?).ok()?;
    doc.as_object()?
        .get("exitCode")
        .filter(|c| c.is_number())
        .map(Value::to_string)
}

fn write_record(args: &RunArgs, rc: i32) -> io::Result<()> {
    if let Some(dir) = Path::new(&args.record).parent() {
        fs::create_dir_all(dir)?;
    }
    let record = serde_json::json!({
        "commit": args.head,
        "at": time::iso(time::now()),
        "exit": rc,
    });
    let new = format!("{}.new", args.record);
    let written =
        fs::write(&new, format!("{record}\n")).and_then(|()| fs::rename(&new, &args.record));
    if written.is_err() {
        let _ = fs::remove_file(&new);
    }
    written
}

fn run(args: &RunArgs) -> u8 {
    if let Err(e) = mask::set_private(&args.private) {
        eprintln!("{e}");
        return 2;
    }
    let signalled = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        if signal_hook::flag::register(signal, Arc::clone(&signalled)).is_err() {
            warn("cannot listen for signals, not starting a review");
            return 2;
        }
    }
    // A container under this name outlived its client: the caller holds the
    // lock, so it is no review's.
    bounded(&["rm", "-f", &args.name], args.stop_wait);
    let opened =
        fs::File::create(&args.doc).and_then(|doc| Ok((doc, fs::File::create(&args.log)?)));
    let spawned = opened.and_then(|(doc, log)| {
        Command::new(&args.command[0])
            .args(&args.command[1..])
            .stdin(Stdio::null())
            .stdout(doc)
            .stderr(log)
            .spawn()
    });
    let mut child = match spawned {
        Ok(child) => child,
        Err(e) => {
            warn(&format!("could not start the review: {e}"));
            remove(args);
            return 2;
        }
    };
    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Ok(Some(status)) = child.try_wait() {
            break status;
        }
        if started.elapsed().as_secs() >= args.limit {
            say(&format!("time limit of {}s passed", args.limit));
            timed_out = true;
            stop(args, 20);
            break reap(&mut child, args);
        }
        if signalled.load(Ordering::Relaxed) {
            stop(args, 10);
            break reap(&mut child, args);
        }
        sleep(TICK);
    };
    let rc = exit_of(status);
    remove(args);
    mask_file(&args.doc);
    mask_file(&args.log);

    let branch = &args.branch;
    if timed_out {
        say(&format!(
            "branch={branch} runner_exit={rc} recorded=no, the run ran out of time, see {}",
            args.log
        ));
        return 2;
    }
    // The document and the container must agree before the range counts as
    // read.
    let doc_rc = doc_exit(&args.doc);
    if let Some(doc_rc) = doc_rc.as_ref().filter(|d| **d != rc.to_string()) {
        say(&format!(
            "branch={branch} runner_exit={rc} document_exit={doc_rc} recorded=no, the two disagree, see {}",
            args.doc
        ));
        return 2;
    }
    if ![0, 10, 20].contains(&rc) {
        say(&format!(
            "branch={branch} runner_exit={} recorded=no, see {}",
            doc_rc.unwrap_or_else(|| rc.to_string()),
            args.log
        ));
        return 2;
    }
    if write_record(args, rc).is_err() {
        warn(&format!(
            "could not write {}, the record of what was reviewed is unchanged",
            args.record
        ));
        return 2;
    }
    say(&format!(
        "branch={branch} from={} to={} runner_exit={rc} recorded=yes seconds={}",
        short(&args.since),
        short(&args.head),
        started.elapsed().as_secs()
    ));
    rc as u8
}

fn remove(args: &RunArgs) {
    let mut failed = None;
    for dir in &args.remove {
        if fs::remove_dir_all(dir).is_err() && Path::new(dir).exists() {
            failed.get_or_insert(dir.clone());
        }
    }
    if let Some(dir) = failed {
        warn(&format!("could not remove the workspace {dir}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    const CAPS: &str = r#"{"inputs":[{"name":"PI_ENSEMBLE_LENS_NOTE","scope":"cycle"},
        {"name":"PI_ENSEMBLE_HOST_ONLY","scope":"host"}]}"#;

    #[test]
    fn a_lever_the_image_does_not_list_is_refused() {
        let given = names(&[
            "-e",
            "PI_ENSEMBLE_POLICY_JUDGE=off",
            "-e",
            "PI_ENSEMBLE_LENS_NOTE=short",
            "-e",
            "PI_ENSEMBLE_HOST_ONLY",
        ]);
        assert_eq!(
            refused(CAPS, &given),
            ["PI_ENSEMBLE_POLICY_JUDGE", "PI_ENSEMBLE_HOST_ONLY"]
        );
    }

    #[test]
    fn the_wanted_names_secrets_and_others_are_not_the_runners_to_refuse() {
        let given = names(&[
            "-e",
            "PI_ENSEMBLE_SANDBOX_MODE=x",
            "-e",
            "PI_ENSEMBLE_MODEL_TOKEN",
            "-e",
            "PI_ENSEMBLE_API_KEY",
            "-e",
            "PI_ENSEMBLE_PLAN_ISSUE=7",
            "-e",
            "PI_ENSEMBLE_EXTRA_IMAGE_DIRS",
            "-e",
            "HOME=/x",
        ]);
        assert!(refused(CAPS, &given).is_empty());
    }

    #[test]
    fn an_answer_that_is_no_object_lists_nothing() {
        let given = names(&["-e", "PI_ENSEMBLE_LENS_NOTE"]);
        assert_eq!(refused("not json", &given), ["PI_ENSEMBLE_LENS_NOTE"]);
        assert_eq!(refused("[]", &given), ["PI_ENSEMBLE_LENS_NOTE"]);
    }

    #[test]
    fn a_record_counts_only_with_a_full_commit() {
        let dir = std::env::temp_dir().join(format!("review-record-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("review.json");
        let path = file.to_str().unwrap();
        let full = "0123456789abcdef0123456789abcdef01234567";
        for (text, want) in [
            (format!(r#"{{"commit":"{full}"}}"#), Some(full.to_string())),
            (r#"{"commit":"abc1"}"#.to_string(), None),
            (r#"{"commit":null}"#.to_string(), None),
            (format!(r#"{{"commit":"{}"}}"#, full.to_uppercase()), None),
            ("not json".to_string(), None),
        ] {
            fs::write(&file, text).unwrap();
            assert_eq!(recorded(path), want);
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_run_takes_its_flags_and_the_command_after_the_dashes() {
        let given = names(&[
            "--branch",
            "main",
            "--head",
            "h",
            "--since",
            "s",
            "--record",
            "r",
            "--limit",
            "5",
            "--stop-wait",
            "1",
            "--name",
            "n",
            "--doc",
            "d",
            "--log",
            "l",
            "--remove",
            "a",
            "--remove",
            "b",
            "--private",
            "p",
            "--",
            "docker",
            "run",
            "--private",
        ]);
        let args = RunArgs::parse(&given).unwrap();
        assert_eq!(args.remove, ["a", "b"]);
        assert_eq!(args.private, ["p"]);
        assert_eq!(args.command, ["docker", "run", "--private"]);
        assert_eq!(args.limit, 5);
        assert!(RunArgs::parse(&given[..given.len() - 4]).is_err());
        assert!(RunArgs::parse(&names(&["--limit", "x", "--", "docker"])).is_err());
    }
}
