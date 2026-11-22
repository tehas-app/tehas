//! What the runs of the sandbox image share: a planning turn
//! (src/plan/start.rs) asks the image's runner what it can do; tehas export
//! reads git's global name. These were the review loop's, which left core in
//! 2.0. The module keeps its name until src/export.rs takes `git_global` from
//! another path.

use std::env;
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, ExitStatus, Stdio};
use tehas_core::sandbox;

const RUNNER: &str = "/opt/pi-rukas/bin/pi-rukas-headless";

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

fn exit_of(status: ExitStatus) -> i32 {
    status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
}
