//! The commands of one `finish` run, and the scratch it makes for itself.
//!
//! A run asks two programs and no other. A host's git and gh are found once,
//! before anything is asked of them, and every call runs under a bound of its
//! own: a fetch, a rebase, a push or a pull request is seconds, and one that has
//! stopped is one that did not happen. A case hands the rules a host of its own,
//! so the rules of `finish` are read without a network and without a token.
//!
//! This is the one place the factory asks git of a tree a cycle's container
//! wrote, so a call of git is asked of that tree's repository under the settings
//! that leave nothing of the tree to be run: see [`PINS`].

use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long one call may take before its process group is signalled.
const BOUND: Duration = Duration::from_secs(600);

/// How long a call that outlived [`BOUND`] gets to leave after TERM.
const GRACE: Duration = Duration::from_secs(10);

/// How much of one stream a call may say: what a run reads is a name, a count
/// and a URL. A stream past this is read to its end and dropped.
const KEPT: u64 = 1 << 16;

/// What every call of git is pinned to. A run is the factory's first host-side
/// ask of git in a tree a cycle's container wrote, and an agent can write
/// anything into the tree it works in, hooks and git configuration included, so
/// a git of the workspace reads the settings of the tree it is pointed at. Two
/// of those name a program for git to run: `core.hooksPath`, which moves the
/// hooks git executes around a checkout, a rebase and a push (and whose default
/// is under the workspace's own `.git`), and `core.fsmonitor`, which a `status`
/// reads as a command. The workspace's hooks are no part of the work: the
/// factory pushes the cycle's commits, not the scripts somebody left beside
/// them, and a script of the tree must not run as the factory on the host or
/// answer for whether its push goes through. So both are pinned off on every
/// call, as the cleaner's one call into a workspace pins them off
/// (`console/src/clean.rs`), and pinned off at the front of the words, where
/// they answer the tree's config.
///
/// The pins are `args` and not the environment, because a host of a case keeps
/// the words it was asked and a case can see that a run gave them.
const PINS: [&str; 4] = [
    "-c",
    "core.hooksPath=/dev/null",
    "-c",
    "core.fsmonitor=false",
];

/// `ETXTBSY`, the kernel's answer to a `posix_spawn` of a file that some process
/// still holds open for writing: a program cannot be execed while a writer of it
/// lives. A run puts one program of its own on disk, the body of the draft, and a
/// case of `finish` stages a stub of `gh`, so the exec of a just-written file is
/// on this command's path.
///
/// The writer is usually not the process that wrote the file. A process forked
/// between the write and the exec holds a copy of every descriptor its parent had
/// then, and one of them can still be the script: the test harness holds the write
/// end of its children's streams this way, which is why a case that stages a
/// program and runs it at once fails now and then and never fails on its own.
///
/// The refusal clears when the last writer closes, so the ask is repeated rather
/// than given back as a call that could not be run. The span is bounded, as
/// everything else of a call is: a program held by a writer that never closes is a
/// hung host, and a run that waited on it would be a step of the poll that stopped
/// reporting.
const BUSY: i32 = 26;
const BUSY_ASKS: u32 = 20;
const BUSY_WAIT: Duration = Duration::from_millis(50);

/// What a command answered: its exit status and what it printed. A status of
/// None is a command that could not be run or did not end in its bound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Done {
    pub status: Option<i32>,
    pub out: String,
    pub err: String,
}

impl Done {
    pub fn ok(&self) -> bool {
        self.status == Some(0)
    }

    /// The command's own words: the first line it said on either stream.
    pub fn said(&self) -> Option<String> {
        self.err
            .lines()
            .chain(self.out.lines())
            .map(str::trim)
            .find(|l| !l.is_empty())
            .map(str::to_string)
    }
}

/// The answer of a call that could not be run at all: nothing to read, and a
/// line that says why.
fn missing(why: &str) -> Done {
    Done {
        status: None,
        out: String::new(),
        err: why.to_string(),
    }
}

/// The programs a run asks, and the way its own tree goes away. A host's asks
/// git and gh of their own; a case's answers from a repository of its own and a
/// stub of `gh`.
pub trait Host {
    /// Ask `program` with `args`. The run gives only the names `git` and `gh`;
    /// a call of git is given the pins of [`PINS`] as the first of its `args`,
    /// because they decide what a run does and not only how it is asked. `dir`
    /// is the directory to ask in: the workspace, or the run's tree.
    fn run(&mut self, program: &str, args: &[String], dir: &Path) -> Done;

    /// Take the run's tree away, the one thing every path does. `workspace` is
    /// the repository the tree belongs to: a tree's registration lives in that
    /// repository, and a run that stopped before its end would otherwise leave
    /// the name claimed, which the next run of the issue cannot use again.
    fn clean(&mut self, workspace: &Path, dir: &Path);

    /// The host of a real host, when this one is it. One call of a run is not a
    /// rule but a tidy-up — git's record of a tree the run has already taken away
    /// — and its words answer nothing. A case that stands in for the host's
    /// programs would answer it with a line of its own and be read as a call the
    /// case never meant, so the ask is made of the real programs or not at all.
    fn as_live(&mut self) -> Option<&mut Live> {
        None
    }
}

/// The programs of a host: git and gh, found once, before anything is asked of
/// them. A program that is not there is refused before the run begins.
pub struct Live {
    git: String,
    gh: String,
}

impl Live {
    /// The programs of the host as it is: `git` and `gh`, found on `PATH`.
    pub fn new() -> Result<Live, String> {
        Ok(Live {
            git: found("git")?,
            gh: found("gh")?,
        })
    }

    /// The same host aimed at two programs of the caller's own. A case needs it:
    /// the rules of a run are decided from what a program printed, and only the
    /// host a run is really given takes that from the streams of a child.
    pub fn aimed_at(git: &Path, gh: &Path) -> Live {
        Live {
            git: git.display().to_string(),
            gh: gh.display().to_string(),
        }
    }

    /// git's record of the run's trees cleared, which asks nothing whose answer
    /// a rule reads. The pins go with it: it is a call of git of the workspace
    /// like every other, and `asked` is the one place a call is built.
    pub fn prune(&self, workspace: &Path) {
        let _ = bounded(
            &self.git,
            &asked(vec!["worktree".to_string(), "prune".to_string()]),
            workspace,
        );
    }
}

impl Host for Live {
    fn run(&mut self, program: &str, args: &[String], dir: &Path) -> Done {
        let program = match program {
            // A call that reached the host without the pins was built around
            // `git` outside `asked`, and is refused rather than run: the pins
            // are part of what git is asked, not a courtesy of the host.
            "git" if !asked_of_git(args) => {
                return missing("a call of git was asked without the pins")
            }
            "git" => self.git.as_str(),
            "gh" => self.gh.as_str(),
            "" => return missing("no program was named"),
            other => other,
        };
        bounded(program, args, dir)
    }

    fn as_live(&mut self) -> Option<&mut Live> {
        Some(self)
    }

    /// git's own removal when git still knows the tree, so its record goes with
    /// it; the tree itself when git does not, which is what a tree the rebase
    /// left mid flight needs.
    fn clean(&mut self, workspace: &Path, dir: &Path) {
        // The tree of a run is named for the process, so its name is not known
        // here. Whatever is under the directory is the run's own, and a tree git
        // still has a registration for is removed through the repository it is a
        // worktree of, so the name is free for the next run of the issue.
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let tree = entry.path();
                if tree
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("tree-"))
                {
                    let _ = self.run(
                        "git",
                        &asked(vec![
                            "-C".into(),
                            workspace.display().to_string(),
                            "worktree".into(),
                            "remove".into(),
                            "--force".into(),
                            tree.display().to_string(),
                        ]),
                        dir,
                    );
                }
            }
        }
        if dir_exists(dir) {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

/// The words of a call of git, the pins of [`PINS`] before everything else: one
/// place builds them, so no call of the workspace is asked without them. They
/// are `args` and not the command's environment, because the tree's own config
/// is what they answer, and `args` is what the host hands the program.
pub fn asked(words: Vec<String>) -> Vec<String> {
    let mut all: Vec<String> = PINS.iter().map(|s| s.to_string()).collect();
    all.extend(words);
    all
}

/// Whether a call's words carry the pins: what a host reads to know that what
/// it was given is a call of git it should answer, and what a case reads to see
/// that a run asked with them.
pub fn asked_of_git(args: &[String]) -> bool {
    args.starts_with(&PINS.map(String::from))
}

/// Whether a path is a directory, a link's own entry and all.
pub fn dir_exists(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir())
}

/// The first `PATH` directory holding `name` as something runnable, as
/// `command -v` answers.
fn found(name: &str) -> Result<String, String> {
    let path = std::env::var("PATH").unwrap_or_default();
    path.split(':')
        .find_map(|dir| {
            let p = Path::new(dir).join(name);
            let m = std::fs::metadata(&p).ok()?;
            (m.is_file() && m.permissions().mode() & 0o111 != 0).then(|| p.display().to_string())
        })
        .ok_or_else(|| format!("{name} is not on PATH"))
}

/// Where one run's scratch lives: beside the kept workspace, so the tree of the
/// run and the repository it is a worktree of are on one filesystem, and under
/// names of the issue's own that no other program writes.
pub struct Scratch {
    parent: PathBuf,
    name: String,
}

impl Scratch {
    /// `<parent>/<name>` is the work directory and `<name>` the issue's own
    /// prefix: `<name>.finish.<pid>` is the run's tree and `<name>.finish.body`
    /// the draft's body. The process number keeps two runs of one issue out of
    /// one another's tree.
    pub fn new(parent: PathBuf, name: String) -> Scratch {
        Scratch { parent, name }
    }

    /// The run's own directory, made now: a run has a tree before it has used
    /// one, and a tree it cannot make is a refusal and not a push.
    pub fn dir(&self) -> Result<PathBuf, String> {
        std::fs::create_dir_all(&self.parent)
            .map_err(|e| format!("{}: {e}", self.parent.display()))?;
        let dir = self
            .parent
            .join(format!("{}.finish.{}", self.name, std::process::id()));
        std::fs::create_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        Ok(dir)
    }

    /// The body of the draft, written for the call and readable by nobody but
    /// the run: the reason is the asker's own sentence, and a sentence of theirs
    /// goes into a file and never into a word of a command line.
    pub fn write_body(&self, text: &str) -> Result<PathBuf, String> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::create_dir_all(&self.parent)
            .map_err(|e| format!("{}: {e}", self.parent.display()))?;
        let file = self.parent.join(format!("{}.finish.body", self.name));
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&file)
            .map_err(|e| format!("{}: {e}", file.display()))?;
        f.write_all(text.as_bytes())
            .map_err(|e| format!("{}: {e}", file.display()))?;
        Ok(file)
    }

    /// The body is the run's own to take back, whether or not the call it was
    /// given answered.
    pub fn drop_body(&self) {
        let _ = std::fs::remove_file(self.parent.join(format!("{}.finish.body", self.name)));
    }
}

/// The names a call of a run is stripped of: nothing here points git or gh at
/// another tree, runs something else in their place, or waits for a person at a
/// prompt. The token is gh's own to find, the one its shim or its store holds: a
/// run given one of a person's would act as that person, and the factory acts as
/// the factory.
const SCRUBBED: [&str; 17] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_CONFIG",
    "GIT_CONFIG_GLOBAL",
    "GIT_CONFIG_SYSTEM",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_TERMINAL_PROMPT",
    "GIT_ASKPASS",
    "SSH_ASKPASS",
    "GIT_PAGER",
    "PAGER",
    "GH_TOKEN",
    "GITHUB_TOKEN",
    "BASH_ENV",
    "ENV",
];

/// One of a command's streams, read on a thread of its own to the bound of
/// [`KEPT`] bytes: what a run reads is a name, a count and a URL, and a child
/// that filled a pipe nobody drained would never reach its end.
fn bounded_stream<S: std::io::Read + Send + 'static>(
    stream: Option<S>,
) -> Option<std::thread::JoinHandle<String>> {
    let s = stream?;
    Some(std::thread::spawn(move || {
        use std::io::Read;
        let mut text = String::new();
        let _ = s.take(KEPT).read_to_string(&mut text);
        text
    }))
}

/// One command in `dir`, its standard input closed, both streams read as they
/// are written, and its process group killed when it outlives [`BOUND`].
fn bounded(program: &str, args: &[String], dir: &Path) -> Done {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        // Both streams are taken from the child and read as they are written, so
        // a child never stops on a full pipe and the run reads the name, the
        // count or the URL it asked for. A stream left to the terminal would be
        // seen by nobody, and every rule of a run would decide from nothing.
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    for name in SCRUBBED {
        cmd.env_remove(name);
    }
    let mut asks = 0;
    let mut child = loop {
        match cmd.spawn() {
            Ok(c) => break c,
            // A program another process still holds for writing is execed a
            // moment later, if it is execed at all: see [`BUSY`].
            Err(e) if e.raw_os_error() == Some(BUSY) && asks < BUSY_ASKS => {
                asks += 1;
                std::thread::sleep(BUSY_WAIT);
            }
            Err(e) => return missing(&format!("{program} could not be run: {e}")),
        }
    };
    // Both streams are read on a thread of their own: a child that filled one
    // while the other was still being read would never reach its end.
    let out = bounded_stream(child.stdout.take());
    let err = bounded_stream(child.stderr.take());
    let pid = child.id().to_string();
    let until = Instant::now() + BOUND;
    // TERM to the group and KILL after the grace, as `timeout -k` does: gh's and
    // git's own helpers go with them.
    let stop = |signal: &str| {
        let _ = Command::new("kill")
            .args([signal, &format!("-{pid}")])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    };
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s.code(),
            Ok(None) if Instant::now() >= until => {
                stop("-TERM");
                std::thread::sleep(GRACE);
                stop("-KILL");
                let _ = child.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return missing(&format!("it could not be waited for: {e}"));
            }
        }
    };
    let given = |r: Option<std::thread::JoinHandle<String>>| {
        r.and_then(|h| h.join().ok()).unwrap_or_default()
    };
    Done {
        status,
        out: given(out),
        err: given(err),
    }
}
