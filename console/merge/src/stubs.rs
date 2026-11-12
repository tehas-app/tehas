//! The stubs of `pi` and `cargo` for the real-repository cases. They are
//! programs the merge runs, so they are written under a directory of the case's
//! own making and named by path: a `pi` on `PATH` is never asked and no case
//! can reach a real model. The value that names them owns that directory, so a
//! case cannot lose the stubs while it still holds the paths to them.

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use tehas_core::testdir::TestDir;

/// The programs of a real repository's cases. The directory of the stubs goes
/// when the value goes, so a case holds this for as long as it runs them.
pub struct Tools {
    pub git: String,
    pub pi: PathBuf,
    pub cargo: PathBuf,
    _stubs: TestDir,
}

impl Tools {
    /// The stubs of one case, in a directory of its own. The merge runs `pi`
    /// inside a tree the case owns, so the stub cannot sit there, and it is
    /// named by path, so a `pi` of the host is never asked. The directory lives
    /// with the value: a case that holds the programs holds the `pi` they name.
    pub fn new(name: &str) -> Tools {
        let dir = TestDir::new(name);
        // A stub `pi`: a real session edits the files its prompt names, so
        // this one does. The prompt is the last argument and it names the
        // files to resolve after `did: `, up to the full stop that ends the
        // list. A file named in `TEHAS_TEST_EXTRA` is written besides them,
        // which is how a case says the model strayed.
        let pi = dir.join("pi");
        write_script(
            &pi,
            "#!/bin/sh\n\
             for a in \"$@\"; do prompt=\"$a\"; done\n\
             list=$(printf '%s\\n' \"$prompt\" | sed -n 's/^.*did: \\(.*\\)\\. Change.*/\\1/p')\n\
             for f in $list; do\n\
             \t[ -n \"$f\" ] || continue\n\
             \tprintf 'the model resolved it\\n' > \"$TEHAS_TEST_TREE/$f\"\n\
             done\n\
             [ -n \"$TEHAS_TEST_EXTRA\" ] && printf 'the model wrote this\\n' > \"$TEHAS_TEST_TREE/$TEHAS_TEST_EXTRA\"\n\
             printf 'resolved\\n'\n",
        );
        // A stub `cargo`: it always answers that it wrote the lockfile again.
        let cargo = dir.join("cargo");
        write_script(&cargo, "#!/bin/sh\nexit 0\n");
        Tools {
            git: "/usr/bin/git".to_string(),
            pi,
            cargo,
            _stubs: dir,
        }
    }
}

/// Write an executable script: the stubs of `pi` and `cargo`.
pub(super) fn write_script(path: &Path, body: &str) {
    std::fs::write(path, body).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}
