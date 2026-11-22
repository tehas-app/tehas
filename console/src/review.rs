//! What the runs of the sandbox image share: a planning turn
//! (src/plan/start.rs) asks the image's runner what it can do; tehas export
//! reads git's global name. These were the review loop's, which left core in
//! 2.0. The module keeps its name until src/export.rs takes `git_global` from
//! another path; T6's regroup renames it after what it holds.

use std::env;
use std::process::{Command, Stdio};
use tehas_core::sandbox;

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

// What the image's runner says it can do, by the sandbox's own check; empty
// when docker does not answer in time.
pub(crate) fn capabilities(image: &str) -> String {
    let wait = env::var("TEHAS_CAPS_WAIT").unwrap_or_default();
    let shape = !wait.is_empty()
        && wait.len() <= 4
        && !wait.starts_with('0')
        && wait.bytes().all(|b| b.is_ascii_digit());
    let wait = if shape { wait.as_str() } else { "120" };
    Command::new("timeout")
        .args(["-k", "10", wait])
        .args(sandbox::capabilities(image))
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}
