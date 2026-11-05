//! Which pi-rukas commit a host follows, and the image upstream publishes for
//! it, for the sandbox build, bootstrap.sh and scripts/tag-release.sh. The
//! rules are tehas_core::rukas.
//!
//!   console rukas target <PI_RUKAS_FOLLOW> <PI_RUKAS_REPO>
//!     "<commit> <what it is>" on one line. Exit 1 with nothing printed when
//!     upstream does not answer, has no such tag or branch, or the value is
//!     none of release, release:<line>, branch:<name> or commit:<40 hex>.
//!   console rukas image <PI_RUKAS_REPO> <commit>
//!     ghcr.io/<owner>/<name>:sha-<first 12>, lower case. Exit 1 with nothing
//!     printed when the repository is not on GitHub or the commit is not 40
//!     hex digits.
//!
//! Exit 64 for any other words.

use crate::store::bounded;
use std::process::Stdio;
use tehas_core::rukas;

const USAGE: &str = "usage: console rukas target <PI_RUKAS_FOLLOW> <PI_RUKAS_REPO>\n       console rukas image <PI_RUKAS_REPO> <commit>";

pub fn main(given: &[String]) -> u8 {
    let said = match given {
        [verb, follow, repo] if verb == "target" => {
            rukas::target(follow, repo, ls_remote).map(|(commit, what)| format!("{commit} {what}"))
        }
        [verb, repo, commit] if verb == "image" => rukas::image(repo, commit),
        _ => {
            eprintln!("{USAGE}");
            return 64;
        }
    };
    match said {
        Some(line) => {
            tehas_core::outln!("{line}");
            0
        }
        None => 1,
    }
}

/// `git ls-remote`, bounded: the cleaner runs unattended on a timer, and no
/// caller may wait on a network that does not answer.
pub fn ls_remote(args: &[&str]) -> Option<String> {
    let said = bounded("5", "30", "git")
        .arg("ls-remote")
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    said.status
        .success()
        .then(|| String::from_utf8_lossy(&said.stdout).into_owned())
}
