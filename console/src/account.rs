//! Who this run is: the account name a flag records as its author, and the uid
//! a check of who owns a file compares against.
//!
//! Both come from `id`. Asking the kernel instead needs libc's `getuid` and
//! `getpwuid`, which are `unsafe`, and AGENTS.md allows no `unsafe`.
//!
//! The uid is the effective one. It used to be the real one, from
//! `libc::getuid`. The rest of the family already hands the console the
//! effective number: `bin/tehas` and `bin/tehasbot` give `$EUID` to
//! `console run` and `lib/config.sh` gives it to `console settings load`, so a
//! check of who owns a file compares against the number the run was started
//! with. No tehas program is setuid (host/docs/host.md), so on a host today the
//! two answers are the same number.

use std::process::{Command, Stdio};
use std::sync::OnceLock;

fn word(flag: &str) -> Option<String> {
    let out = Command::new("id")
        .arg(flag)
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let text = String::from_utf8_lossy(&out.stdout).trim_end().to_string();
    (!text.is_empty()).then_some(text)
}

/// The account which wrote a flag, as `id -un` names it: nothing for an
/// account `id` does not name.
pub(crate) fn name() -> String {
    word("-un").unwrap_or_default()
}

/// The uid of this run, kept after the first ask. None when `id` does not
/// answer: a check of whether a file is ours then says no, rather than
/// guessing an uid that no run has.
pub(crate) fn uid() -> Option<u32> {
    static UID: OnceLock<Option<u32>> = OnceLock::new();
    *UID.get_or_init(|| word("-u").and_then(|w| w.parse().ok()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::MetadataExt;
    use tehas_core::testdir::TestDir;

    #[test]
    fn the_name_is_the_one_id_un_prints() {
        // The name pause.json records as `by` and a stop records in its
        // marker. test/stop_test.sh compares the marker with the account of the
        // run; test/pause_test.sh only asks that `by` is a string.
        let out = Command::new("id").arg("-un").output().expect("id -un");
        assert_eq!(name(), String::from_utf8_lossy(&out.stdout).trim_end());
    }

    #[test]
    fn the_uid_owns_a_file_this_run_made() {
        // The number is used to say whether a file is this run's own, so it is
        // checked against the owner of a file this run wrote, not against the
        // same command again.
        let home = TestDir::new("account-uid");
        let file = home.join("own");
        fs::write(&file, "x").unwrap();
        assert_eq!(uid(), Some(fs::metadata(&file).unwrap().uid()));
    }
}
