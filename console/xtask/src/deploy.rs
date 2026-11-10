//! tag-deploy: deploy the newest release tag to a tehas host.
//! scripts/tag-deploy.sh execs it; its header is the usage and the exit
//! statuses, and `--help` prints it.
//!
//! The steps, their lines on stderr and their statuses are the ones the
//! script had. git and ssh still do the work, behind [`Sys`] so the tests can
//! stand in for them; what moved here is what decides between them.

use crate::tag::{newest, remote_head};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

#[derive(Debug, PartialEq)]
pub struct Args {
    /// `tehas update --check` on the host: report, change nothing.
    pub check: bool,
    pub remote: String,
    pub dest: String,
}

#[derive(Debug, PartialEq)]
pub enum Parsed {
    Help,
    Run(Args),
}

/// The arguments, as the script's loop read them: the first error in their
/// order, or --help wherever it comes before one. An error is said after
/// "tag-deploy: " and ends 64.
pub fn parse(args: &[String]) -> Result<Parsed, String> {
    let mut a = Args {
        check: false,
        remote: "origin".into(),
        dest: String::new(),
    };
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--check" => a.check = true,
            "--remote" => match it.next() {
                Some(v) => a.remote = v.clone(),
                None => return Err("--remote needs a name".into()),
            },
            "-h" | "--help" => return Ok(Parsed::Help),
            s if s.starts_with('-') => return Err(format!("unknown argument: {s}")),
            s => {
                if !a.dest.is_empty() {
                    return Err("one destination only".into());
                }
                a.dest = s.to_string();
            }
        }
    }
    if a.dest.is_empty() {
        return Err("name the host, e.g. scripts/tag-deploy.sh tehas-tehas".into());
    }
    Ok(Parsed::Run(a))
}

/// The ssh command from TEHAS_SSH, split on white space as `read -ra` splits
/// it; unset or blank is plain ssh.
pub fn ssh_command(var: Option<&str>) -> Vec<String> {
    let words: Vec<String> = var
        .unwrap_or("")
        .split_whitespace()
        .map(str::to_string)
        .collect();
    if words.is_empty() {
        vec!["ssh".into()]
    } else {
        words
    }
}

/// The programs the task runs, and where it says what it found.
pub trait Sys {
    /// git in the checkout: its stdout with the trailing newlines taken off,
    /// and whether it ended 0. Its stderr is the task's.
    fn git(&mut self, args: &[&str]) -> (String, bool);
    /// The ssh command with the destination and the remote command: its exit
    /// status, its stdout the task's own.
    fn ssh(&mut self, dest: &str, command: &str) -> i32;
    /// The same, its stdout taken as `git` takes it.
    fn ssh_out(&mut self, dest: &str, command: &str) -> (String, i32);
    /// A line on stderr.
    fn say(&mut self, line: &str);
}

/// `${s:0:7}`.
fn short(s: &str) -> &str {
    s.get(..7).unwrap_or(s)
}

/// The task. Returns the exit status.
pub fn run(a: &Args, sys: &mut dyn Sys) -> u8 {
    let remote = a.remote.as_str();
    let dest = a.dest.as_str();
    if !sys
        .git(&["fetch", "--quiet", "--tags", "--force", remote])
        .1
    {
        sys.say(&format!(
            "tag-deploy: git fetch --tags --force from {remote} failed"
        ));
        return 1;
    }
    let (listing, ok) = sys.git(&["ls-remote", "--symref", remote, "HEAD"]);
    if !ok {
        sys.say(&format!("tag-deploy: cannot read HEAD of {remote}"));
        return 1;
    }
    let (tags, _) = sys.git(&["tag", "--list", "v*", "--sort=-v:refname"]);
    let (Some((branch, head)), Some(tag)) = (remote_head(&listing), newest(&tags)) else {
        sys.say(&format!(
            "tag-deploy: no default branch or no vX.Y.Z tag on {remote}"
        ));
        return 1;
    };
    let rev = format!("{tag}^{{commit}}");
    let (tag_commit, _) = sys.git(&["rev-parse", "--verify", "--quiet", &rev]);
    if tag_commit != head {
        sys.say(&format!(
            "tag-deploy: {branch} at {} is past {tag}; run scripts/tag-release.sh first",
            short(&head)
        ));
        return 1;
    }

    // A login as another account gets past ssh and then fails on a missing
    // file, which reads like a broken update.
    match sys.ssh(dest, "test -r ~/tehas/env.sh") {
        0 => {}
        255 => {
            sys.say(&format!("tag-deploy: cannot reach {dest} over ssh"));
            return 1;
        }
        _ => {
            sys.say(&format!(
                "tag-deploy: {dest} has no ~/tehas/env.sh for the ssh user: log in as the account tehas runs as, through ~/.ssh/config or TEHAS_SSH"
            ));
            return 1;
        }
    }
    let only = if a.check { " (check only)" } else { "" };
    sys.say(&format!("tag-deploy: {tag} to {dest}{only}"));
    let update = if a.check {
        ". ~/tehas/env.sh && tehas update --check"
    } else {
        ". ~/tehas/env.sh && tehas update"
    };
    let rc = sys.ssh(dest, update);
    if a.check || rc != 0 {
        if rc != 0 {
            sys.say(&format!("tag-deploy: tehas update on {dest} exited {rc}"));
        }
        // As bash's `exit "$rc"`: the status modulo 256.
        return rc.rem_euclid(256) as u8;
    }

    let (at, rc) = sys.ssh_out(dest, "git -C ~/tehas/platform rev-parse HEAD");
    if rc != 0 {
        sys.say(&format!("tag-deploy: cannot read the checkout on {dest}"));
        return 1;
    }
    if at != tag_commit {
        sys.say(&format!(
            "tag-deploy: {dest} is at {}, not {tag} ({})",
            short(&at),
            short(&tag_commit)
        ));
        return 1;
    }
    sys.say(&format!("tag-deploy: {dest} is at {tag}"));
    0
}

/// git and ssh as the script ran them: git in the checkout the script lives
/// in, ssh as TEHAS_SSH names it.
pub struct Real {
    pub root: PathBuf,
    pub ssh: Vec<String>,
}

impl Real {
    pub fn new(root: &Path, ssh: Vec<String>) -> Real {
        Real {
            root: root.to_path_buf(),
            ssh,
        }
    }

    fn ssh_cmd(&self, dest: &str, command: &str) -> Command {
        let mut c = Command::new(&self.ssh[0]);
        c.args(&self.ssh[1..]).arg(dest).arg(command);
        c.current_dir(&self.root);
        c
    }
}

/// A status as bash gives it: the code, or 128 and the signal.
fn code(s: ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    s.code().unwrap_or_else(|| 128 + s.signal().unwrap_or(0))
}

/// A program that could not start ends 127, as bash says of one not found.
fn not_started(cmd: &Command, e: &std::io::Error) -> i32 {
    eprintln!("tag-deploy: {}: {e}", cmd.get_program().to_string_lossy());
    127
}

fn output(cmd: &mut Command) -> (String, i32) {
    match cmd
        .stdin(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
    {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout);
            (text.trim_end_matches('\n').to_string(), code(o.status))
        }
        Err(e) => (String::new(), not_started(cmd, &e)),
    }
}

impl Sys for Real {
    fn git(&mut self, args: &[&str]) -> (String, bool) {
        let mut c = Command::new("git");
        c.current_dir(&self.root).args(args);
        // fetch says nothing on stdout; it is the task's, as in the script.
        if args.first() == Some(&"fetch") {
            return match c.status() {
                Ok(s) => (String::new(), s.success()),
                Err(e) => (String::new(), not_started(&c, &e) == 0),
            };
        }
        let (out, rc) = output(&mut c);
        (out, rc == 0)
    }

    fn ssh(&mut self, dest: &str, command: &str) -> i32 {
        let mut c = self.ssh_cmd(dest, command);
        match c.status() {
            Ok(s) => code(s),
            Err(e) => not_started(&c, &e),
        }
    }

    fn ssh_out(&mut self, dest: &str, command: &str) -> (String, i32) {
        output(&mut self.ssh_cmd(dest, command))
    }

    fn say(&mut self, line: &str) {
        eprintln!("{line}");
    }
}

#[cfg(test)]
mod tests;
