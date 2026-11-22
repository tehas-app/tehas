//! What the runs of the sandbox image share: the wiki's run
//! (src/wiki/runner.rs) and a planning turn (src/plan/start.rs) ask the
//! image's runner what it can do, bound their docker calls and mask what an
//! agent wrote; tehas export reads git's global name. These were the review
//! loop's, which left core in 2.0. The module keeps its name until
//! src/export.rs takes `git_global` from another path.

use std::env;
use std::fs;
use std::io::{self, Write};
use std::os::unix::process::ExitStatusExt;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant};
use tehas_core::{mask, sandbox};

const TICK: Duration = Duration::from_millis(100);
const RUNNER: &str = "/opt/pi-rukas/bin/pi-rukas-headless";

fn warn(line: &str) {
    let _ = writeln!(io::stderr(), "tehas: {line}");
}

pub fn git_global(key: &str) -> String {
    Command::new("git")
        .args(["config", "--global", key])
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim_end_matches('\n')
                .to_string()
        })
        .unwrap_or_default()
}

// What the image's runner says it can do. Err is the status of the call, so a
// docker that does not answer is said apart from an image with no verb.
pub(crate) fn capabilities(image: &str) -> Result<String, i32> {
    let wait = env::var("TEHAS_CAPS_WAIT").unwrap_or_default();
    let shape = !wait.is_empty()
        && wait.len() <= 4
        && !wait.starts_with('0')
        && wait.bytes().all(|b| b.is_ascii_digit());
    let wait = if shape { wait.as_str() } else { "120" };
    let out = Command::new("timeout")
        .args([
            "-k",
            "10",
            wait,
            "docker",
            "run",
            sandbox::NEVER_PULL,
            "--rm",
            "--init",
        ])
        .args([image, RUNNER, "--capabilities"])
        .stderr(Stdio::null())
        .output();
    match out {
        Ok(o) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).into_owned()),
        Ok(o) => Err(exit_of(o.status)),
        Err(_) => Err(127),
    }
}

/// The directories of a run, removed on every way out of it.
pub(crate) struct Workspace(pub(crate) Vec<String>);

impl Drop for Workspace {
    fn drop(&mut self) {
        for dir in &self.0 {
            let _ = fs::remove_dir_all(dir);
        }
    }
}

/// The exit status of a run a signal ended, as a shell gives it.
pub(crate) fn ended(signalled: &AtomicUsize) -> Option<u8> {
    match signalled.load(Ordering::Relaxed) {
        0 => None,
        signal => Some(128 + signal as u8),
    }
}

/// A docker call that may not answer: None when it was still going after
/// `wait` seconds, and then it is killed.
pub(crate) fn bounded(args: &[&str], wait: u64) -> Option<ExitStatus> {
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

pub(crate) fn wait_for(child: &mut Child, wait: Duration) -> Option<ExitStatus> {
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

pub(crate) fn exit_of(status: ExitStatus) -> i32 {
    status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
}

/// A file an agent wrote, masked in place. An empty mask keeps the file as
/// it was, as tehas export does.
pub(crate) fn mask_file(path: &str) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn a_workspace_goes_on_every_way_out_and_a_missing_directory_is_no_fault() {
        let at = tehas_core::testdir::TestDir::new("run-workspace");
        let (tree, state) = (at.join("tree"), at.join("tree.state"));
        fs::create_dir_all(tree.join("sub")).unwrap();
        fs::create_dir_all(&state).unwrap();
        let name = |p: &Path| p.display().to_string();
        let way_out = |dirs: Vec<String>| -> u8 {
            let _workspace = Workspace(dirs);
            2
        };
        let gone = name(&at.join("never"));
        assert_eq!(way_out(vec![name(&tree), gone, name(&state)]), 2);
        assert!(!tree.exists() && !state.exists());
    }

    #[test]
    fn a_signal_ends_a_run_as_a_shell_says_it() {
        assert_eq!(ended(&AtomicUsize::new(0)), None);
        assert_eq!(ended(&AtomicUsize::new(15)), Some(143));
        assert_eq!(ended(&AtomicUsize::new(2)), Some(130));
    }
}
