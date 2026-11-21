//! The commands a plugin's fetch and build run, and a call's container,
//! behind one seam so the tests answer for git and docker. [`Host`] runs
//! them; a test's [`Exec`] records them and says what each answered.

use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};
use tehas_core::plugin::run::kill_by_cidfile;

/// How a command's git is given the host's settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Git {
    /// Not git: the environment as it is.
    No,
    /// The fetch: no `GIT_*` of the environment, and no prompt, so only the
    /// host's own config, with its credential helper, speaks for it.
    Fetch,
    /// A read of the repository: no config of the host or the system either,
    /// so no filter or helper of theirs runs.
    Local,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cmd {
    pub program: String,
    pub args: Vec<String>,
    pub git: Git,
    /// `DOCKER_BUILDKIT=1`.
    pub buildkit: bool,
}

impl Cmd {
    pub fn new(program: &str, args: Vec<String>) -> Cmd {
        Cmd {
            program: program.to_string(),
            args,
            git: Git::No,
            buildkit: false,
        }
    }

    pub fn git(git: Git, args: Vec<String>) -> Cmd {
        Cmd {
            git,
            ..Cmd::new("git", args)
        }
    }

    /// The command as one line, for a test or a log.
    pub fn line(&self) -> String {
        let mut words = vec![self.program.clone()];
        words.extend(self.args.iter().cloned());
        words.join(" ")
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Done {
    pub ok: bool,
    pub out: Vec<u8>,
    /// What it said on standard error, cut to its last lines.
    pub err: String,
}

impl Done {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.out).trim_end().to_string()
    }
}

/// How an attached run ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Called {
    /// It exited 0, and this is all it wrote to stdout.
    Out(Vec<u8>),
    /// It wrote more than the most, and was killed.
    Over,
    /// It exited other than 0.
    Exit(i32),
    /// It ran past its time, and was killed.
    Killed,
    /// It could not be run, or docker refused it: why.
    NotRun(String),
}

pub trait Exec {
    fn run(&mut self, cmd: &Cmd) -> Done;
    /// `from | to`: ok when both are. `out` is `to`'s; `err` both.
    fn pipe(&mut self, from: &Cmd, to: &Cmd) -> Done;
    /// `argv` attached, with no stdin and stderr dropped: stdout read up to
    /// `max` bytes, killed past it or at `limit` by the id docker wrote in
    /// `cidfile`.
    fn attached(&mut self, argv: &[String], limit: Duration, max: usize, cidfile: &Path) -> Called;
}

/// How long a killed container's client is given to end before it is
/// killed too.
const GRACE: Duration = Duration::from_secs(2);
const POLL: Duration = Duration::from_millis(20);

/// The container killed by its cid file, then its client after the grace.
fn stop(child: &mut Child, cidfile: &Path) {
    kill_by_cidfile(cidfile);
    let until = Instant::now() + GRACE;
    while Instant::now() < until {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        std::thread::sleep(POLL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// The most of standard error kept.
const ERR_MAX: usize = 4096;

fn last_of(err: &[u8]) -> String {
    let s = String::from_utf8_lossy(err);
    let s = s.trim_end();
    let mut at = s.len().saturating_sub(ERR_MAX);
    while !s.is_char_boundary(at) {
        at += 1;
    }
    s[at..].to_string()
}

fn command(cmd: &Cmd) -> Command {
    let mut c = Command::new(&cmd.program);
    c.args(&cmd.args).stdin(Stdio::null());
    if cmd.git != Git::No {
        for (k, _) in std::env::vars_os() {
            if k.to_string_lossy().starts_with("GIT_") {
                c.env_remove(&k);
            }
        }
        c.env("GIT_TERMINAL_PROMPT", "0");
    }
    if cmd.git == Git::Local {
        c.env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null");
    }
    if cmd.buildkit {
        c.env("DOCKER_BUILDKIT", "1");
    }
    c
}

/// The host's own programs.
pub struct Host;

impl Exec for Host {
    fn run(&mut self, cmd: &Cmd) -> Done {
        match command(cmd).output() {
            Ok(o) => Done {
                ok: o.status.success(),
                out: o.stdout,
                err: last_of(&o.stderr),
            },
            Err(e) => Done {
                ok: false,
                out: Vec::new(),
                err: format!("{}: {e}", cmd.program),
            },
        }
    }

    fn pipe(&mut self, from: &Cmd, to: &Cmd) -> Done {
        let mut first = match command(from)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                return Done {
                    ok: false,
                    out: Vec::new(),
                    err: format!("{}: {e}", from.program),
                }
            }
        };
        let stdin = first.stdout.take().map_or(Stdio::null(), Stdio::from);
        let second = command(to).stdin(stdin).output();
        let mut first_err = Vec::new();
        if let Some(mut e) = first.stderr.take() {
            let _ = e.read_to_end(&mut first_err);
        }
        let first_ok = first.wait().is_ok_and(|s| s.success());
        match second {
            Ok(o) => {
                let mut err = first_err;
                err.extend_from_slice(&o.stderr);
                Done {
                    ok: first_ok && o.status.success(),
                    out: o.stdout,
                    err: last_of(&err),
                }
            }
            Err(e) => Done {
                ok: false,
                out: Vec::new(),
                err: format!("{}: {e}", to.program),
            },
        }
    }

    fn attached(&mut self, argv: &[String], limit: Duration, max: usize, cidfile: &Path) -> Called {
        let Some((program, args)) = argv.split_first() else {
            return Called::NotRun("no command".into());
        };
        let mut child = match Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => return Called::NotRun(format!("{program}: {e}")),
        };
        let until = Instant::now() + limit;
        let (tx, rx) = mpsc::channel();
        let stdout = child.stdout.take();
        // One byte past the most says it is over, and the read stops there.
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(o) = stdout {
                let _ = o.take(max as u64 + 1).read_to_end(&mut buf);
            }
            let _ = tx.send(buf);
        });
        let mut out: Option<Vec<u8>> = None;
        let mut status = None;
        loop {
            if out.is_none() {
                match rx.recv_timeout(POLL) {
                    Ok(b) => out = Some(b),
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => out = Some(Vec::new()),
                }
            } else {
                std::thread::sleep(POLL);
            }
            if out.as_ref().is_some_and(|b| b.len() > max) {
                stop(&mut child, cidfile);
                return Called::Over;
            }
            if status.is_none() {
                match child.try_wait() {
                    Ok(s) => status = s,
                    Err(e) => {
                        stop(&mut child, cidfile);
                        return Called::NotRun(format!("{program}: {e}"));
                    }
                }
            }
            if let (Some(b), Some(s)) = (&out, status) {
                return match s.code() {
                    Some(0) => Called::Out(b.clone()),
                    // docker run's own failures, before the container ran.
                    Some(c @ 125..=127) => Called::NotRun(format!("docker run exited {c}")),
                    Some(c) => Called::Exit(c),
                    None => Called::NotRun("docker run was ended by a signal".into()),
                };
            }
            if Instant::now() >= until {
                stop(&mut child, cidfile);
                return Called::Killed;
            }
        }
    }
}

#[cfg(any(test, feature = "testing"))]
pub mod fake {
    //! An [`Exec`] that answers from a list: the first rule whose words the
    //! command's line holds answers it, and a command no rule names fails.

    use super::*;

    #[derive(Default)]
    pub struct Fake {
        /// What a line holds, the answer, and how many more times it gives
        /// it: None, always.
        pub rules: Vec<(String, Done, Option<u32>)>,
        /// What an attached run's line holds, and how it ended.
        pub calls: Vec<(String, Called)>,
        pub ran: Vec<String>,
    }

    pub fn ok(out: &str) -> Done {
        Done {
            ok: true,
            out: out.as_bytes().to_vec(),
            err: String::new(),
        }
    }

    pub fn fails(err: &str) -> Done {
        Done {
            ok: false,
            out: Vec::new(),
            err: err.to_string(),
        }
    }

    impl Fake {
        pub fn new() -> Fake {
            Fake::default()
        }

        /// A rule for an attached run, first in the list.
        pub fn attach(&mut self, holds: &str, c: Called) -> &mut Fake {
            self.calls.insert(0, (holds.to_string(), c));
            self
        }

        /// A rule first in the list, so it answers before the ones given
        /// earlier.
        pub fn on(&mut self, holds: &str, done: Done) -> &mut Fake {
            self.rules.insert(0, (holds.to_string(), done, None));
            self
        }

        /// [`Fake::on`] for the next `n` lines that hold it only.
        pub fn times(&mut self, holds: &str, done: Done, n: u32) -> &mut Fake {
            self.rules.insert(0, (holds.to_string(), done, Some(n)));
            self
        }

        fn answer(&mut self, line: &str) -> Done {
            let rule = self
                .rules
                .iter_mut()
                .find(|(h, _, n)| line.contains(h.as_str()) && *n != Some(0));
            match rule {
                Some((_, d, n)) => {
                    if let Some(n) = n {
                        *n -= 1;
                    }
                    d.clone()
                }
                None => fails("no rule"),
            }
        }

        pub fn ran_any(&self, holds: &str) -> bool {
            self.ran.iter().any(|l| l.contains(holds))
        }
    }

    impl Exec for Fake {
        fn run(&mut self, cmd: &Cmd) -> Done {
            let line = cmd.line();
            self.ran.push(line.clone());
            self.answer(&line)
        }

        fn pipe(&mut self, from: &Cmd, to: &Cmd) -> Done {
            let line = format!("{} | {}", from.line(), to.line());
            self.ran.push(line.clone());
            self.answer(&line)
        }

        /// The line is the words, then the limit's seconds and the most.
        fn attached(
            &mut self,
            argv: &[String],
            limit: Duration,
            max: usize,
            _cidfile: &Path,
        ) -> Called {
            let line = format!("{} limit={} max={max}", argv.join(" "), limit.as_secs());
            self.ran.push(line.clone());
            self.calls
                .iter()
                .find(|(h, _)| line.contains(h.as_str()))
                .map_or(Called::NotRun("no rule".into()), |(_, c)| c.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_git_command_loses_the_git_variables_and_a_read_the_hosts_config() {
        let fetch = command(&Cmd::git(Git::Fetch, vec!["version".into()]));
        let envs: Vec<_> = fetch.get_envs().collect();
        assert!(envs.contains(&("GIT_TERMINAL_PROMPT".as_ref(), Some("0".as_ref()))));
        assert!(!envs.iter().any(|(k, _)| *k == "GIT_CONFIG_GLOBAL"));
        let read = command(&Cmd::git(Git::Local, vec!["version".into()]));
        let envs: Vec<_> = read.get_envs().collect();
        assert!(envs.contains(&("GIT_CONFIG_GLOBAL".as_ref(), Some("/dev/null".as_ref()))));
        assert!(envs.contains(&("GIT_CONFIG_NOSYSTEM".as_ref(), Some("1".as_ref()))));
        let mut build = Cmd::new("docker", vec![]);
        build.buildkit = true;
        let build = command(&build);
        let envs: Vec<_> = build.get_envs().collect();
        assert_eq!(envs, [("DOCKER_BUILDKIT".as_ref(), Some("1".as_ref()))]);
    }

    #[test]
    fn the_host_runs_a_pipe_and_both_must_succeed() {
        let mut h = Host;
        let echo = Cmd::new("printf", vec!["a\\nb\\n".into()]);
        let wc = Cmd::new("wc", vec!["-l".into()]);
        let d = h.pipe(&echo, &wc);
        assert!(d.ok);
        assert_eq!(d.text().trim(), "2");
        let d = h.pipe(&Cmd::new("false", vec![]), &wc);
        assert!(!d.ok);
        assert!(!h.run(&Cmd::new("/nonexistent/x", vec![])).ok);
    }

    // The generic attach, with plain processes and no docker: the kill at the
    // limit falls through to the client's, as a cid file docker never wrote.
    // Whether stdout streams under `docker run --log-driver none` is for a
    // host to confirm.
    #[test]
    fn the_host_runs_attached_reads_at_most_max_and_kills_at_the_limit() {
        let mut h = Host;
        let cid = Path::new("/nonexistent/cid");
        let sh = |script: &str| vec!["sh".to_string(), "-c".to_string(), script.to_string()];
        let s = Duration::from_secs(5);
        assert_eq!(
            h.attached(&sh(r#"printf '{"a":1}'; echo no >&2"#), s, 16, cid),
            Called::Out(br#"{"a":1}"#.to_vec())
        );
        assert_eq!(
            h.attached(&sh("printf 0123456789abcdef"), s, 16, cid),
            Called::Out(b"0123456789abcdef".to_vec())
        );
        let started = Instant::now();
        assert_eq!(
            h.attached(&sh("printf 0123456789abcdefg; exec sleep 30"), s, 16, cid),
            Called::Over
        );
        assert_eq!(h.attached(&sh("exit 3"), s, 16, cid), Called::Exit(3));
        assert_eq!(
            h.attached(&sh("exec sleep 30"), Duration::from_millis(300), 16, cid),
            Called::Killed
        );
        assert!(started.elapsed() < Duration::from_secs(10));
        assert!(matches!(
            h.attached(&["/nonexistent/x".to_string()], s, 16, cid),
            Called::NotRun(_)
        ));
    }
}
