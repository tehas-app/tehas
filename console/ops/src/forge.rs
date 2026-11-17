//! What the commands that ask GitHub share: the instance's repository, the gh a
//! run calls, and the bound of one call.
//!
//! The repository is `TEHAS_REPO`, `owner/name`, as `lib/config.sh` exports it.
//! The program is the instance's `$TEHAS_HOME/shim/gh` when it is there, so a
//! write is the shim's write, with the bot's token and the line in
//! `state/github-writes.log`; with no shim it is gh found on `PATH`, where
//! `host/env.sh` puts that shim first anyway.

use std::env;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

/// The longest repository path part accepted. Its own bound and not
/// `lib/config.sh`'s rule: that checks the characters of `owner/name` and bounds
/// no length, and the label ceiling of the adapter is no rule about a
/// repository. A longer setting is a setting that cannot name one.
const REPO_PART_MAX: usize = 100;

/// The bound of one call of gh: the thirty seconds the dispatcher's reads give
/// it, so the two readers of one issue wait the same time for an answer. The
/// same number is in `console/dispatch/src/read/run.rs`; both are 30 and
/// neither reads the other's.
pub const CALL_BOUND: Duration = Duration::from_secs(30);

/// Whether a repository is `owner/name`: the shape `lib/config.sh` checks the
/// setting for, under a ceiling of its own for a part. A setting that is
/// no repository is no repository to ask, which is the 78 of a run that cannot
/// reach GitHub and never a request aimed at something else.
pub fn repo_shaped(repo: &str) -> bool {
    tehas_core::settings::rules::is_repo(repo) && repo.split('/').all(|p| p.len() <= REPO_PART_MAX)
}

/// `TEHAS_REPO`, trimmed: the repository of the instance, or None when the name
/// is unset, empty, or nothing but space. Nothing here asks GitHub, runs a
/// program or writes a file.
pub fn repo() -> Option<String> {
    match env::var("TEHAS_REPO") {
        Ok(v) => {
            let v = v.trim();
            (!v.is_empty()).then(|| v.to_string())
        }
        Err(_) => None,
    }
}

/// Whether `file` is something that may be run: a regular file with a bit for
/// someone to execute it. The same test `clean`'s `which` and the dispatcher's
/// readers make of a program.
pub fn runnable(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Where `program` may be run, as `command -v` answers: the first directory of
/// `path` that holds it, or None.
fn which(program: &str, path: &str) -> Option<String> {
    path.split(':').find_map(|dir| {
        let p = Path::new(dir).join(program);
        runnable(&p).then(|| p.display().to_string())
    })
}

/// The program of a run: the instance's shim when it is installed, and gh of
/// `PATH` otherwise. The shim is named rather than inherited from an ambient
/// `PATH`, because it is the program that takes the bot's token and keeps the
/// write log the poll's cost record reads (#649): a run that reached GitHub
/// past it would be unmeasured and still exit 0.
pub fn gh_program(home: Option<&str>, path: &str) -> Option<String> {
    if let Some(home) = home.filter(|h| !h.is_empty()) {
        let shim = Path::new(home).join("shim").join("gh");
        if runnable(&shim) {
            return Some(shim.display().to_string());
        }
    }
    which("gh", path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_must_be_a_file_of_path_that_may_be_run() {
        // The name a host runs is its instance's shim or plain gh: a directory that
        // holds no gh, a name that is a directory, and a file no one may run are
        // all "no gh".
        let dir = scratch("path");
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let gh = bin.join("gh");
        let empty = dir.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        // PATH is a list of directories, so the case builds one that holds the
        // program and one that holds nothing, and asks them in both orders.
        let both = |first: &str, second: &str| {
            format!(
                "{}:{}",
                dir.join(first).display(),
                dir.join(second).display()
            )
        };
        let here = gh.display().to_string();
        // No gh of this case is run: it asks only what the mode makes of it.
        tehas_core::testing::write_exe(&gh, "#!/bin/sh\nexit 0\n").unwrap();
        assert_eq!(
            which("gh", &both("bin", "empty")).as_deref(),
            Some(here.as_str())
        );
        assert_eq!(
            which("gh", &both("empty", "bin")).as_deref(),
            Some(here.as_str()),
            "a gh of a later directory"
        );
        std::fs::remove_file(&gh).unwrap();
        std::fs::create_dir_all(&gh).unwrap();
        assert_eq!(
            which("gh", &both("bin", "empty")),
            None,
            "a directory is no gh"
        );
        std::fs::remove_dir_all(&gh).unwrap();
        std::fs::write(&gh, "not a program\n").unwrap();
        assert_eq!(
            which("gh", &both("bin", "empty")),
            None,
            "a file no one may run is no gh"
        );
        std::fs::set_permissions(&gh, PermissionsExt::from_mode(0o700)).unwrap();
        assert!(
            which("gh", &both("bin", "empty")).is_some(),
            "a gh of its owner alone is gh"
        );
        assert_eq!(which("gh", ""), None, "an empty PATH holds no gh");
        assert_eq!(
            which("gh", &dir.join("nowhere").display().to_string()),
            None
        );
    }

    #[test]
    fn the_instance_shim_is_the_program_when_it_is_installed() {
        // The shim of the instance takes the bot's token and keeps the write log
        // the poll's cost record reads, so a run that reached gh by name alone
        // would write unmeasured and exit 0. Where there is no shim, gh of PATH
        // answers, and where there is neither, the run is the 78 of no program.
        let dir = scratch("shim");
        let shim = dir.join("shim");
        std::fs::create_dir_all(&shim).unwrap();
        let gh = shim.join("gh");
        // No gh of this case is run: it asks only what the mode makes of it.
        tehas_core::testing::write_exe(&gh, "#!/bin/sh\nexit 0\n").unwrap();
        let home = dir.display().to_string();
        let named = gh.display().to_string();
        // A stripped PATH: only the shim can be the program.
        assert_eq!(gh_program(Some(&home), "").as_deref(), Some(named.as_str()));
        // A shim that is not runnable is no shim, and neither is a directory.
        std::fs::set_permissions(&gh, PermissionsExt::from_mode(0o644)).unwrap();
        assert_eq!(gh_program(Some(&home), ""), None, "a shim no one may run");
        std::fs::remove_file(&gh).unwrap();
        std::fs::create_dir_all(&gh).unwrap();
        assert_eq!(gh_program(Some(&home), ""), None, "a directory is no shim");
        std::fs::remove_dir_all(&shim).unwrap();
        // No shim: the program is the gh of PATH, and none when PATH holds no gh.
        assert_eq!(gh_program(Some(&home), ""), None);
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        tehas_core::testing::write_exe(bin.join("gh"), "#!/bin/sh\nexit 0\n").unwrap();
        let path = bin.display().to_string();
        assert_eq!(
            gh_program(Some(&home), &path).as_deref(),
            Some(bin.join("gh").display().to_string().as_str())
        );
        // No home at all is no shim either: PATH decides.
        assert_eq!(
            gh_program(None, &path).as_deref(),
            Some(bin.join("gh").display().to_string().as_str())
        );
    }

    #[test]
    fn a_repository_is_owner_and_name_under_the_ceiling() {
        let wide = format!("acme/{}", "w".repeat(REPO_PART_MAX));
        let wider = format!("acme/{}", "w".repeat(REPO_PART_MAX + 1));
        for (given, ok) in [
            ("acme/widget", true),
            ("acme-widget/example.product", true),
            ("acme/widget/extra", false),
            ("acme/", false),
            ("/widget", false),
            ("widget", false),
            ("acme/wid get", false),
            ("acme/..%2fetc", false),
            ("", false),
            (wide.as_str(), true),
            (wider.as_str(), false),
        ] {
            assert_eq!(repo_shaped(given), ok, "{given:?}");
        }
    }

    fn scratch(name: &str) -> tehas_core::testdir::TestDir {
        tehas_core::testdir::TestDir::new(&format!("forge-{name}"))
    }
}
