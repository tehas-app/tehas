//! tehas-merge: the easy merge of one pull request (#1031). Started by
//! `libexec/easy-merge` inside the sandbox container, where it is given the
//! tree, the two commits and the programs it may run.
//!
//! Usage:
//!   tehas-merge --tree <dir> --head <sha> --base <sha> --branch <name>
//!               --pr <n> --issue <n> --ref <branch>
//!               [--git <path>] [--cargo <path>] [--pi <path>] [--no-model]
//!
//! The tree is the pull request's head already: `libexec/easy-merge` made it
//! and checked the head out. The merge is done in it and its one commit is
//! pushed to `refs/heads/<ref>`; nothing else is pushed, and no rebase, force
//! or second pull request is ever made.
//!
//! Answers one JSON line on standard output: {ok, sha, conflicts, settled,
//! model, pushed, why, pr, issue}. The commands that ran are said to the log.
//!
//! Exit: 0 the merge commit was pushed; 1 it was not, with `why` naming why;
//! 64 an argument it does not know; 78 an argument it cannot do without.

use std::io::{Read, Write as _};
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::thread;
use std::time::{Duration, Instant};
use tehas_merge::{normalise_commit, report, Plan, Runner};

/// How long one command may take: git's own work is seconds, and the model's
/// turn is the long one. The container is bounded by the unit that started it,
/// so this is the bound of a single call, not of the run.
const BOUND: Duration = Duration::from_secs(900);

/// The most of one stream a command may write. What the merge reads is names
/// and lines.
const KEPT: usize = 1 << 20;

const SCRUBBED: [&str; 15] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_CONFIG",
    "GIT_CONFIG_GLOBAL",
    "GIT_CONFIG_SYSTEM",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_MERGE_AUTOEDIT",
    "GIT_PAGER",
    "PAGER",
    "CARGO",
    "RUSTC_WRAPPER",
    "BASH_ENV",
    "ENV",
];

fn usage() -> String {
    include_str!("main.rs")
        .lines()
        .skip(2)
        .take_while(|l| l.starts_with("//!"))
        .map(|l| l.trim_start_matches("//!").strip_prefix(' ').unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

fn refuse(words: &str, rc: i32) -> ! {
    eprintln!("tehas-merge: {words}");
    std::process::exit(rc);
}

/// Where a program is: the path the caller gave, or the name found on PATH.
/// A program the caller named must be there and be runnable; a name taken
/// from PATH is asked for once, here, so a missing one is refused before the
/// merge begins rather than in the middle of it.
fn program(given: &str, what: &str, required: bool) -> String {
    if given.is_empty() {
        if required {
            refuse(&format!("{what} is not named"), 78);
        }
        return String::new();
    }
    let p = PathBuf::from(given);
    let ok = if p.is_absolute() {
        p.is_file() && std::fs::metadata(&p).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
    } else {
        which(given)
    };
    if !ok {
        refuse(&format!("{what} {given} cannot be run"), 78);
    }
    given.to_string()
}

fn which(name: &str) -> bool {
    let paths = match std::env::var_os("PATH") {
        Some(p) => p,
        None => return false,
    };
    std::env::split_paths(&paths).any(|dir| {
        let f = dir.join(name);
        f.is_file() && std::fs::metadata(&f).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
    })
}

/// The runner of a host: each command run in the tree under its own bound,
/// its output kept and its own line said to the log.
struct Real {
    log: PathBuf,
}

impl Real {
    /// One line per command, in the container's log: what ran, in which tree,
    /// and how it ended. The arguments are git's, cargo's and pi's own, so
    /// what is said names no token: git reads its credentials from the
    /// environment the container was given, never from an argument.
    fn say(&self, line: &str) {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log)
        {
            let _ = writeln!(f, "{line}");
        }
    }
}

impl Runner for Real {
    fn run(&mut self, program: &str, args: &[String], tree: &Path) -> tehas_merge::Done {
        if program.is_empty() {
            return tehas_merge::Done {
                status: None,
                out: String::new(),
                err: format!("{program} is not there"),
            };
        }
        self.say(&format!(
            "{} {}",
            program,
            args.iter()
                .map(|a| a.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        ));
        match command(program, args, tree).spawn() {
            Err(e) => tehas_merge::Done {
                status: None,
                out: String::new(),
                err: format!("cannot run {program}: {e}"),
            },
            Ok(child) => match wait(child, BOUND) {
                Ok((status, out, err)) => tehas_merge::Done { status, out, err },
                Err(why) => tehas_merge::Done {
                    status: None,
                    out: String::new(),
                    err: why,
                },
            },
        }
    }
}

/// One command, stripped of the settings that would point git or cargo at
/// another tree or run something else in their place. `GIT_CONFIG_COUNT` and
/// its keys stay: they carry the container's credential helper.
fn command(program: &str, args: &[String], tree: &Path) -> Command {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(tree)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for name in SCRUBBED {
        cmd.env_remove(name);
    }
    cmd
}

/// Reads one stream as it is written, so a child never stops on a full pipe.
/// A stream past `KEPT` is read to its end and answered as an error, as is one
/// that cannot be read: the merge decides from these lines, so it never
/// decides from part of them.
fn reader(
    mut stream: impl Read + Send + 'static,
) -> std::io::Result<Receiver<Result<String, String>>> {
    let (tx, rx) = channel();
    thread::Builder::new().spawn(move || {
        let mut buf = Vec::new();
        let read = (&mut stream)
            .take(KEPT as u64 + 1)
            .read_to_end(&mut buf)
            .and_then(|_| std::io::copy(&mut stream, &mut std::io::sink()));
        let answer = match read {
            Err(e) => Err(format!("its output could not be read: {e}")),
            Ok(_) if buf.len() > KEPT => Err(format!("it wrote more than {KEPT} bytes")),
            Ok(_) => Ok(String::from_utf8_lossy(&buf).into_owned()),
        };
        let _ = tx.send(answer);
    })?;
    Ok(rx)
}

/// Wait for a command out to its bound, reading what it writes meanwhile. A
/// command that does not end is killed and said so: a merge that hangs is one
/// that did not happen, and the full refresh is the answer to that. The bound
/// holds for the reads too: a child that left a process behind on its pipes
/// has ended, but its streams have not.
fn wait(
    mut child: std::process::Child,
    bound: Duration,
) -> Result<(Option<i32>, String, String), String> {
    let deadline = Instant::now() + bound;
    let late = format!("it did not end in {}s", bound.as_secs());
    let give_up = |child: &mut std::process::Child, why: String| {
        let _ = child.kill();
        let _ = child.wait();
        why
    };
    let streams = match (child.stdout.take(), child.stderr.take()) {
        (Some(out), Some(err)) => reader(out).and_then(|o| Ok((o, reader(err)?))),
        _ => Err(std::io::Error::other("a stream was not opened")),
    };
    let (out, err) = match streams {
        Ok(s) => s,
        Err(e) => return Err(give_up(&mut child, format!("cannot read it: {e}"))),
    };
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => return Err(give_up(&mut child, late)),
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(give_up(&mut child, format!("waiting failed: {e}"))),
        }
    };
    let collect = |rx: Receiver<Result<String, String>>| {
        rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|_| late.clone())?
    };
    Ok((status.code(), collect(out)?, collect(err)?))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("{}", usage());
        return;
    }
    let mut v: Vec<(String, String)> = Vec::new();
    let mut flags: Vec<String> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if let Some(f) = a.strip_prefix("--") {
            match f {
                "no-model" => flags.push(f.to_string()),
                "" => refuse("-- alone means nothing here", 64),
                _ => {
                    let value = it
                        .next()
                        .unwrap_or_else(|| refuse(&format!("--{f} needs a value"), 64));
                    if value.starts_with("--") {
                        refuse(&format!("--{f} needs a value"), 64);
                    }
                    v.push((f.to_string(), value.clone()));
                }
            }
        } else {
            refuse(&format!("unexpected argument: {a}"), 64);
        }
    }
    let need = |k: &str| -> String {
        v.iter()
            .find(|(n, _)| n == k)
            .map(|(_, x)| x.clone())
            .unwrap_or_else(|| refuse(&format!("--{k} is required"), 78))
    };
    let take = |k: &str, d: &str| -> String {
        v.iter()
            .find(|(n, _)| n == k)
            .map(|(_, x)| x.clone())
            .unwrap_or_else(|| d.to_string())
    };
    let issue: u64 = need("issue")
        .parse()
        .unwrap_or_else(|_| refuse("--issue is not a number", 64));
    let pr: u64 = need("pr")
        .parse()
        .unwrap_or_else(|_| refuse("--pr is not a number", 64));
    let tree = PathBuf::from(need("tree"));
    if !tree.is_dir() {
        refuse(&format!("{} is not a directory", tree.display()), 78);
    }
    // A commit id is read once, by the rule the merge compares with: an
    // upper-case sha from a person or a tool names the same commit as git's
    // own lower-case spelling, and anything else is no commit at all.
    let head = match normalise_commit(&need("head")) {
        Some(sha) => sha,
        None => refuse("--head is not a commit", 64),
    };
    let base = match normalise_commit(&need("base")) {
        Some(sha) => sha,
        None => refuse("--base is not a commit", 64),
    };
    let branch = need("branch");
    let branch_ref = need("ref");
    if branch_ref.is_empty()
        || !branch_ref
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'/' | b'-'))
    {
        refuse("--ref is not a branch name", 64);
    }
    let log = take("log", "/dev/null");
    let mut plan = Plan {
        issue,
        head: &head,
        base: &base,
        branch: &branch,
        pr,
        tree,
        branch_ref: &branch_ref,
        git: program(&take("git", "git"), "git", true),
        // cargo is needed only to write a lockfile again, and pi only for a
        // conflict the rules cannot settle: a host whose pull request needs
        // neither runs the merge with neither named, so a tool that is not on
        // the PATH holds nothing up. A file whose rule needs it is refused
        // when it is met, not before.
        cargo: program(&take("cargo", "cargo"), "cargo", false),
        pi: program(&take("pi", "pi"), "pi", false),
        model: !flags.contains(&"no-model".to_string()),
    };
    if plan.model && plan.pi.is_empty() {
        refuse(
            "pi is not named, so a conflict in code cannot be resolved",
            78,
        );
    }
    // cargo is the one program without which a rule cannot run, and every
    // pull request of this platform may carry a conflicted lockfile, so a
    // merge that asks the rules at all is refused when cargo is not there:
    // the full refresh is the answer, and it is told now rather than after
    // the merge began. With --no-model the rules are all this merge has.
    if plan.cargo.is_empty() {
        refuse(
            "cargo is not named, so a lockfile cannot be written again",
            78,
        );
    }
    let mut runner = Real {
        log: PathBuf::from(log),
    };
    let answer = report(&mut plan, &mut runner);
    println!("{}", answer.json());
    if !answer.outcome.ok() {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    fn sh(script: &str, bound: Duration) -> Result<(Option<i32>, String, String), String> {
        let args = ["-c".to_string(), script.to_string()];
        let child = command("sh", &args, Path::new("/")).spawn().unwrap();
        wait(child, bound)
    }

    #[test]
    fn a_child_that_fills_its_pipe_still_ends() {
        // The pipe holds 64 KiB, so a parent that reads only after the exit
        // holds this child until the bound kills it.
        let began = Instant::now();
        let (status, out, _) = sh("head -c 200000 /dev/zero", Duration::from_secs(20)).unwrap();
        assert_eq!(status, Some(0));
        assert_eq!(out.len(), 200_000);
        assert!(began.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn an_answer_of_exactly_the_limit_is_kept_and_one_byte_more_is_refused() {
        let at = format!("head -c {KEPT} /dev/zero");
        assert_eq!(sh(&at, BOUND).unwrap().1.len(), KEPT);
        let over = format!("head -c {} /dev/zero", KEPT + 1);
        let why = sh(&over, BOUND).unwrap_err();
        assert!(why.contains("more than"), "{why}");
    }

    #[test]
    fn a_child_that_does_not_end_is_killed_at_the_bound() {
        let why = sh("sleep 5", Duration::from_millis(200)).unwrap_err();
        assert!(why.contains("did not end"), "{why}");
    }

    #[test]
    fn a_process_left_on_the_pipes_cannot_hold_the_merge_past_the_bound() {
        let began = Instant::now();
        let why = sh("sleep 5 & exit 0", Duration::from_millis(300)).unwrap_err();
        assert!(why.contains("did not end"), "{why}");
        assert!(began.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn a_command_loses_the_settings_that_move_it_and_keeps_its_credentials() {
        let cmd = command("git", &[], Path::new("/"));
        let removed: Vec<&OsStr> = cmd
            .get_envs()
            .filter(|(_, v)| v.is_none())
            .map(|(k, _)| k)
            .collect();
        for name in SCRUBBED {
            assert!(removed.contains(&OsStr::new(name)), "{name} stays");
        }
        assert!(!SCRUBBED.iter().any(|n| n.starts_with("GIT_CONFIG_COUNT")
            || n.starts_with("GIT_CONFIG_KEY")
            || n.starts_with("GIT_CONFIG_VALUE")));
    }
}
