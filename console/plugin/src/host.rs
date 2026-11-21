//! What a run asks of the host, behind one seam so the tests answer for
//! docker, git, systemd and the store: [`Real`] runs them. The one-shot
//! commands go through the fetch's own seam, `tehas_ops::plugin::exec`.

use serde_json::Value;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tehas_core::plugin::out;
use tehas_ops::plugin::exec::{self, Exec};

/// How a container's `docker run` ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ended {
    /// The container's exit status, as docker gave it.
    Exit(i32),
    /// It outlived its time and was killed.
    Killed,
    /// docker could not be run, or was ended by a signal: why.
    NotRun(String),
}

pub trait Host {
    fn exec(&mut self) -> &mut dyn Exec;
    /// One call of the state store: its answer, or why not.
    fn store(&mut self, args: &[&str]) -> Result<Value, String>;
    /// `docker run` with these words, waited for at most `limit`; past it,
    /// `docker kill` of the id in `cidfile`.
    fn container(&mut self, argv: &[String], limit: Duration, cidfile: &Path) -> Ended;
    /// `docker cp <from> -` read by the tar reader into `target`.
    fn copy(
        &mut self,
        from: &str,
        target: &Path,
        top: &str,
        limits: out::Limits,
    ) -> Result<out::Taken, String>;
    /// The container's log, both streams, at most the last `max` bytes, into
    /// `to`.
    fn logs(&mut self, container: &str, to: &Path, max: u64) -> Result<(), String>;
}

/// The host's own programs and store.
pub struct Real {
    pub home: std::path::PathBuf,
    pub exec: exec::Host,
}

/// The most of docker's own error kept.
const SAID_MAX: usize = 4096;

/// The first `max` bytes of `s`, cut at a character.
fn cut(s: &str, max: usize) -> String {
    let mut at = s.len().min(max);
    while !s.is_char_boundary(at) {
        at -= 1;
    }
    s[..at].to_string()
}

fn said_of(child: &mut Child) -> String {
    let mut err = Vec::new();
    if let Some(mut e) = child.stderr.take() {
        let _ = e.by_ref().take(64 << 10).read_to_end(&mut err);
    }
    cut(String::from_utf8_lossy(&err).trim(), SAID_MAX)
}

fn kill_by_cidfile(cidfile: &Path) {
    let cid = std::fs::read_to_string(cidfile).unwrap_or_default();
    let cid = cid.trim();
    if !cid.is_empty() && cid.bytes().all(|b| b.is_ascii_hexdigit()) {
        let _ = Command::new("docker")
            .args(["kill", cid])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// Waits for `child` at most `limit`. None: it is still running.
fn wait_for(child: &mut Child, limit: Duration) -> Option<std::process::ExitStatus> {
    let until = Instant::now() + limit;
    loop {
        match child.try_wait() {
            Ok(Some(s)) => return Some(s),
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(250)),
            _ => return None,
        }
    }
}

impl Host for Real {
    fn exec(&mut self) -> &mut dyn Exec {
        &mut self.exec
    }

    fn store(&mut self, args: &[&str]) -> Result<Value, String> {
        tehas_ops::store::try_call(&self.home, 5, args).map_err(|r| r.line)
    }

    fn container(&mut self, argv: &[String], limit: Duration, cidfile: &Path) -> Ended {
        let Some((program, args)) = argv.split_first() else {
            return Ended::NotRun("no command".into());
        };
        // The container's own output goes to its log, which docker logs reads
        // after; the client's error is kept for a run docker refused.
        let mut child = match Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => return Ended::NotRun(format!("{program}: {e}")),
        };
        let err = child.stderr.take();
        let reader = std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut e) = err {
                let _ = e.by_ref().take(1 << 20).read_to_end(&mut buf);
                // Whatever is left is read and dropped, so the client never
                // blocks on a full pipe.
                let _ = std::io::copy(&mut e, &mut std::io::sink());
            }
            buf
        });
        let status = match wait_for(&mut child, limit) {
            Some(s) => s,
            None => {
                kill_by_cidfile(cidfile);
                if wait_for(&mut child, Duration::from_secs(30)).is_none() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                let _ = reader.join();
                return Ended::Killed;
            }
        };
        let err = reader.join().unwrap_or_default();
        match status.code() {
            Some(c) if c == 125 || c == 126 || c == 127 => {
                let said = String::from_utf8_lossy(&err);
                let said = cut(said.trim(), SAID_MAX);
                Ended::NotRun(format!("docker run exited {c}: {said}"))
            }
            Some(c) => Ended::Exit(c),
            None => Ended::NotRun("docker run was ended by a signal".into()),
        }
    }

    fn copy(
        &mut self,
        from: &str,
        target: &Path,
        top: &str,
        limits: out::Limits,
    ) -> Result<out::Taken, String> {
        let mut child = Command::new("docker")
            .args(["cp", from, "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("docker cp: {e}"))?;
        let stdout = child.stdout.take().ok_or("docker cp: no output")?;
        let got = out::read(stdout, target, top, limits);
        // A stream the reader refused is not read to its end.
        let _ = child.kill();
        let status = child.wait();
        let said = said_of(&mut child);
        match got {
            Ok(t) if status.is_ok_and(|s| s.success()) => Ok(t),
            Ok(_) => {
                let _ = std::fs::remove_dir_all(target);
                Err(format!("docker cp {from} failed: {said}"))
            }
            Err(why) => Err(why),
        }
    }

    fn logs(&mut self, container: &str, to: &Path, max: u64) -> Result<(), String> {
        let file = File::create(to).map_err(|e| format!("{}: {e}", to.display()))?;
        let err = file.try_clone().map_err(|e| e.to_string())?;
        let ran = Command::new("docker")
            .args(["logs", container])
            .stdin(Stdio::null())
            .stdout(file)
            .stderr(err)
            .status()
            .map_err(|e| format!("docker logs: {e}"))?;
        keep_last(to, max)?;
        if ran.success() {
            Ok(())
        } else {
            Err(format!("docker logs {container} failed"))
        }
    }
}

/// Cuts the file at `path` to its last `max` bytes.
pub fn keep_last(path: &Path, max: u64) -> Result<(), String> {
    let fail = |e: std::io::Error| format!("{}: {e}", path.display());
    let mut f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(fail)?;
    let len = f.metadata().map_err(fail)?.len();
    if len <= max {
        return Ok(());
    }
    let mut tail = Vec::with_capacity(max as usize);
    f.seek(SeekFrom::Start(len - max)).map_err(fail)?;
    Read::by_ref(&mut f)
        .take(max)
        .read_to_end(&mut tail)
        .map_err(fail)?;
    f.seek(SeekFrom::Start(0)).map_err(fail)?;
    f.write_all(&tail).map_err(fail)?;
    f.set_len(tail.len() as u64).map_err(fail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tehas_core::testdir::TestDir;

    #[test]
    fn a_log_keeps_its_last_bytes() {
        let d = TestDir::new("plugin-keep-last");
        let f = d.join("log");
        std::fs::write(&f, "0123456789").unwrap();
        keep_last(&f, 20).unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "0123456789");
        keep_last(&f, 4).unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "6789");
    }

    #[test]
    fn a_container_that_outlives_its_time_is_killed() {
        let d = TestDir::new("plugin-container");
        let mut h = Real {
            home: d.to_path_buf(),
            exec: exec::Host,
        };
        let cid = d.join("cid");
        let sleep = ["sleep".to_string(), "2".to_string()];
        assert_eq!(
            h.container(&sleep, Duration::from_millis(300), &cid),
            Ended::Killed
        );
        let fails = ["sh".to_string(), "-c".to_string(), "exit 3".to_string()];
        assert_eq!(
            h.container(&fails, Duration::from_secs(5), &cid),
            Ended::Exit(3)
        );
        let refused = [
            "sh".to_string(),
            "-c".to_string(),
            "echo no such image >&2; exit 125".to_string(),
        ];
        assert_eq!(
            h.container(&refused, Duration::from_secs(5), &cid),
            Ended::NotRun("docker run exited 125: no such image".into())
        );
    }
}
