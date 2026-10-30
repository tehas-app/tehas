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
//! model, pushed, why, pr, issue, calls}. The answer is the library's
//! `Report`, so the line a host reads and the one the cases read are one
//! shape. The lines of the commands that ran go to standard error, so the
//! container's log holds them beside the rest of the run, which is why a
//! host's `calls` is the empty list: the runner of a host keeps none.
//!
//! Exit: 0 the merge commit was pushed; 1 it was not, with `why` naming why;
//! 64 an argument it does not know; 78 an argument it cannot do without.

use std::io::Write as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use tehas_merge::{normalise_commit, report, Plan, Runner};

/// How long one command may take: git's own work is seconds, and the model's
/// turn is the long one. The container is bounded by the unit that started it,
/// so this is the bound of a single call, not of the run.
const BOUND: Duration = Duration::from_secs(900);

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
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(tree)
            // A few settings change what git or cargo do without naming them
            // in the call, and the merge cannot read the tree it is told about
            // through one of them. They are taken from this command's own
            // environment before it starts anything: the container is given
            // what a cycle is given, and a mount or a token is added by none of
            // this. Everything else the caller passed stays, because git reads
            // its credentials from it.
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_CONFIG")
            .env_remove("GIT_CONFIG_GLOBAL")
            .env_remove("GIT_CONFIG_SYSTEM")
            .env_remove("GIT_OBJECT_DIRECTORY")
            .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
            .env_remove("GIT_MERGE_AUTOEDIT")
            .env_remove("GIT_PAGER")
            .env_remove("PAGER")
            .env_remove("CARGO")
            .env_remove("RUSTC_WRAPPER")
            .env_remove("BASH_ENV")
            .env_remove("ENV")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        match cmd.spawn() {
            Err(e) => tehas_merge::Done {
                status: None,
                out: String::new(),
                err: format!("cannot run {program}: {e}"),
            },
            Ok(child) => match wait(child) {
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

/// The lines a command wrote, read with the bound of the command that wrote
/// them: a child that has exited with a full pipe keeps a reader blocked, and
/// a read outside the bound would hold the merge past it with no answer.
fn read_bounded(mut r: impl std::io::Read + Send + 'static) -> String {
    let (tx, rx) = std::sync::mpsc::channel();
    if let Err(e) = thread::Builder::new()
        .name("read-the-answer".to_string())
        .spawn(move || {
            let mut buf = Vec::new();
            // A command's answer is read whole up to 1 MiB: what the merge
            // reads is names and lines, and more than that is no answer.
            let _ = r.read_to_end(&mut buf);
            let kept = String::from_utf8_lossy(&buf[..buf.len().min(1 << 20)]).into_owned();
            let _ = tx.send(kept);
        })
    {
        return format!("the answer could not be read: {e}");
    }
    match rx.recv_timeout(BOUND) {
        Ok(s) => s,
        Err(_) => format!("the answer was not written in {}s", BOUND.as_secs()),
    }
}

/// The two streams of a child that has ended, read at once so a full pipe
/// cannot hold the merge past its bound.
fn drain(mut child: std::process::Child) -> (String, String) {
    let out = child.stdout.take().map(read_bounded);
    let err = child.stderr.take().map(read_bounded);
    (
        out.unwrap_or_default(),
        err.unwrap_or_else(|| "the error answer could not be read".to_string()),
    )
}

/// Wait for a command out to its bound, then read what it wrote. A command
/// that does not end is killed and said so: a merge that hangs is one that
/// did not happen, and the full refresh is the answer to that.
fn wait(mut child: std::process::Child) -> Result<(Option<i32>, String, String), String> {
    let began = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let (out, err) = drain(child);
                return Ok((status.code(), out, err));
            }
            Ok(None) => {
                if began.elapsed() >= BOUND {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("it did not end in {}s", BOUND.as_secs()));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("waiting failed: {e}")),
        }
    }
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
    // The one answer of the library, so the host's line and the one under test
    // cannot drift apart: the same keys, and the same `calls`, which a host's
    // `Real` answers empty because it says the commands to the log instead.
    println!("{}", answer.json());
    if !answer.outcome.ok() {
        std::process::exit(1);
    }
}
