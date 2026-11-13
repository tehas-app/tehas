//! Whether install.sh owes the sandbox build, asked once before any copy is made.
//!
//!   console update sandbox <host versions.env> <checkout versions.env> \
//!     <instance sandbox.Dockerfile> <recorded sha256 file>
//!
//! One line of three digits, each 0 or 1: the pins the image reads moved, the
//! instance's sandbox file changed since the last install, the instance's
//! sandbox file cannot be read. Exit 0, or 64 on a usage error.
//!
//! The host's copy of versions.env is read before the copies begin, so the
//! record and the next: lines come from one reading of the file the host held:
//! that file is what the image now on the host was built from. A pin moved on
//! the host owes the build whether or not this run overwrites it, and a pin
//! only the checkout moves owes it because the copy hands the host pins the
//! image was never built from. A host with no copy has never had a build made
//! from these pins, which is a change too.

use super::{say, tell};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

/// The prefixes of the versions.env lines the sandbox image reads
/// (host/sandbox/build.sh, host/sandbox/Dockerfile). The rest of the file is
/// bootstrap.sh's, so a change there asks for nothing but the copy.
const IMAGE_PINS: &[&str] = &[
    "SHELLCHECK_",
    "JQ_",
    "PLAYWRIGHT_",
    "RUST_",
    "CBM_",
    "PI_RUKAS_",
    "VIPUNE_",
    "OO_",
];

/// The lines of `text` the image reads, in their order.
fn image_lines(text: &[u8]) -> Vec<&[u8]> {
    text.split(|b| *b == b'\n')
        .filter(|line| IMAGE_PINS.iter().any(|p| line.starts_with(p.as_bytes())))
        .collect()
}

/// Do the lines the image reads differ between the host's copy (`None` when
/// the host has none) and the checkout's (`None` when it cannot be read)?
fn versions_owed(host: Option<&[u8]>, checkout: Option<&[u8]>) -> bool {
    let Some(host) = host else {
        return true;
    };
    let current = image_lines(checkout.unwrap_or_default());
    let kept = image_lines(host);
    // A file that names none of them cannot be compared with: the build reads
    // what it reads, so the build is owed rather than assumed already done.
    if current.is_empty() || kept.is_empty() {
        return true;
    }
    // One line among them is the change a person makes on a host: one pin
    // moved, the rest as some checkout wrote it. That line alone is compared,
    // so neither the other lines nor the order of the host's copy change the
    // answer. A release's pins are more than one line, the version and its
    // checksum, a model with its revision and its hashes, so a whole block still
    // reads as a whole block, position and all.
    if let [line] = kept[..] {
        let found: Vec<&[u8]> = current.into_iter().filter(|c| *c == line).collect();
        return found != kept;
    }
    kept != current
}

/// What the instance's sandbox file says: (changed, unreadable). A file that is
/// not there owes nothing. One that cannot be read is said, so the run stops:
/// recording it as seen would owe the build once and then never again.
fn dockerfile(file: &Path, recorded: &Path) -> (bool, bool) {
    if !file.is_file() {
        return (false, false);
    }
    let Ok(bytes) = fs::read(file) else {
        return (false, true);
    };
    let hash: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let seen = fs::read(recorded).unwrap_or_default();
    let seen = String::from_utf8_lossy(&seen);
    (hash != seen.trim_end_matches('\n'), false)
}

fn digit(b: bool) -> char {
    if b {
        '1'
    } else {
        '0'
    }
}

pub fn verb(given: &[String]) -> u8 {
    let [host, checkout, file, recorded] = given else {
        tell("console update sandbox: usage: <host versions.env> <checkout versions.env> <sandbox.Dockerfile> <recorded sha256>");
        return 64;
    };
    let host = Path::new(host);
    let host = host.is_file().then(|| fs::read(host).unwrap_or_default());
    let checkout = fs::read(checkout).ok();
    let versions = versions_owed(host.as_deref(), checkout.as_deref());
    let (changed, unread) = dockerfile(Path::new(file), Path::new(recorded));
    say(&format!(
        "{} {} {}",
        digit(versions),
        digit(changed),
        digit(unread)
    ));
    0
}

#[cfg(test)]
mod tests {
    use super::super::testing::dir;
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    const CHECKOUT: &str = "# pins\nNODE_VERSION=22.1.0\nSHELLCHECK_VERSION=0.11.0\n\
SHELLCHECK_SHA256=bb\nPLAYWRIGHT_VERSION=1.55.2\nVIPUNE_VERSION=0.20.2\n\
VIPUNE_MODEL_REVISION=r1\nVIPUNE_MODEL_SHA256_ONNX=cc\n";

    fn owed(host: Option<&str>, checkout: &str) -> bool {
        versions_owed(host.map(str::as_bytes), Some(checkout.as_bytes()))
    }

    #[test]
    fn no_host_copy_owes_the_build() {
        assert!(owed(None, CHECKOUT));
    }

    #[test]
    fn the_same_copy_owes_nothing() {
        assert!(!owed(Some(CHECKOUT), CHECKOUT));
    }

    #[test]
    fn a_host_copy_with_none_of_the_prefixes_owes_the_build() {
        assert!(owed(
            Some("NODE_VERSION=22.1.0\nGH_VERSION=2.1.0\n"),
            CHECKOUT
        ));
        assert!(owed(Some(""), CHECKOUT));
    }

    #[test]
    fn a_checkout_with_none_of_the_prefixes_or_none_at_all_owes_the_build() {
        assert!(owed(Some(CHECKOUT), "NODE_VERSION=22.1.0\n"));
        assert!(versions_owed(Some(CHECKOUT.as_bytes()), None));
    }

    #[test]
    fn a_host_only_pin_moved_owes_nothing() {
        let host = CHECKOUT.replace("NODE_VERSION=22.1.0", "NODE_VERSION=22.2.0");
        assert!(!owed(Some(&host), CHECKOUT));
    }

    #[test]
    fn one_image_pin_moved_on_the_host_owes_the_build() {
        let host = CHECKOUT.replace("PLAYWRIGHT_VERSION=1.55.2", "PLAYWRIGHT_VERSION=1.55.3");
        assert!(owed(Some(&host), CHECKOUT));
    }

    #[test]
    fn a_lone_image_pin_on_the_host_is_read_out_of_the_checkout() {
        assert!(!owed(Some("PLAYWRIGHT_VERSION=1.55.2\n"), CHECKOUT));
        assert!(owed(Some("PLAYWRIGHT_VERSION=1.55.3\n"), CHECKOUT));
        // The same line twice in the checkout is not the one line the host has.
        let twice = format!("{CHECKOUT}PLAYWRIGHT_VERSION=1.55.2\n");
        assert!(owed(Some("PLAYWRIGHT_VERSION=1.55.2\n"), &twice));
    }

    #[test]
    fn a_multi_line_release_block_compares_as_a_whole() {
        let host = CHECKOUT.replace("VIPUNE_MODEL_REVISION=r1", "VIPUNE_MODEL_REVISION=r2");
        assert!(owed(Some(&host), CHECKOUT));
        let block = "VIPUNE_VERSION=0.20.2\nVIPUNE_MODEL_REVISION=r1\n";
        assert!(owed(Some(block), CHECKOUT));
    }

    #[test]
    fn a_reordered_host_copy_owes_the_build() {
        let host = "PLAYWRIGHT_VERSION=1.55.2\nSHELLCHECK_VERSION=0.11.0\nSHELLCHECK_SHA256=bb\n\
VIPUNE_VERSION=0.20.2\nVIPUNE_MODEL_REVISION=r1\nVIPUNE_MODEL_SHA256_ONNX=cc\n";
        assert!(owed(Some(host), CHECKOUT));
        let mut lines: Vec<&str> = CHECKOUT.lines().collect();
        lines.swap(0, 1);
        assert!(
            !owed(Some(&lines.join("\n")), CHECKOUT),
            "only bootstrap lines moved"
        );
    }

    #[test]
    fn rust_pins_are_read_by_the_image() {
        let checkout = format!("{CHECKOUT}RUST_VERSION=1.90.0\n");
        let host = format!("{CHECKOUT}RUST_VERSION=1.91.0\n");
        assert!(owed(Some(&host), &checkout));
    }

    #[test]
    fn a_line_that_only_contains_a_prefix_is_not_read() {
        let host = format!("{CHECKOUT}#SHELLCHECK_VERSION=0.10.0\nXJQ_VERSION=1\n");
        assert!(!owed(Some(&host), CHECKOUT));
    }

    fn sha(text: &str) -> String {
        Sha256::digest(text.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    #[test]
    fn no_dockerfile_owes_nothing() {
        let d = dir("sandbox-none");
        let recorded = d.file("state/sandbox.Dockerfile.sha256", "x\n");
        assert_eq!(
            dockerfile(&d.0.join("sandbox.Dockerfile"), &recorded),
            (false, false)
        );
    }

    #[test]
    fn a_dockerfile_is_judged_against_the_recorded_hash() {
        let d = dir("sandbox-hash");
        let file = d.file("sandbox.Dockerfile", "FROM x\n");
        let missing = d.0.join("state/none");
        assert_eq!(dockerfile(&file, &missing), (true, false));
        let recorded = d.file(
            "state/sandbox.Dockerfile.sha256",
            &format!("{}\n", sha("FROM x\n")),
        );
        assert_eq!(dockerfile(&file, &recorded), (false, false));
        let other = d.file("state/other", &format!("{}\n", sha("FROM y\n")));
        assert_eq!(dockerfile(&file, &other), (true, false));
    }

    #[test]
    fn an_unreadable_dockerfile_is_said() {
        let d = dir("sandbox-unread");
        let file = d.file("sandbox.Dockerfile", "FROM x\n");
        fs::set_permissions(&file, fs::Permissions::from_mode(0o000)).unwrap();
        if fs::read(&file).is_ok() {
            return; // root reads it anyway
        }
        let recorded = d.0.join("state/none");
        assert_eq!(dockerfile(&file, &recorded), (false, true));
    }

    #[test]
    fn the_verb_takes_four_paths() {
        assert_eq!(verb(&[]), 64);
        assert_eq!(verb(&["a".to_string()]), 64);
    }
}
