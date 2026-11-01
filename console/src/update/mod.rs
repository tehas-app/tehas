//! What `tehas update` decides and does. libexec/update holds the lock and
//! runs install.sh; everything else of an update is here.
//!
//!   console update release source|installed|newest|keep
//!   console update release older|stale <version> <version>
//!   console update move --plan <file> --mode update|check [--to <version>]
//!
//! `release` answers one question about the releases, see src/update/release.rs.
//! `move` chooses a version, downloads and checks it and switches current; it
//! leaves what the script needs next in the plan file.
//!
//! Every program is spawned by name from PATH, so a host's own tool, or a
//! test's stub, answers. Each call outside goes through `timeout`, and only
//! what the message says it waited for is waited for.
//!
//! Exit: as each verb says; 64 usage, 78 a setting is missing.

mod release;

use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

/// What a run of `tehas update` has come to mean by the time install.sh runs.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Plan {
    /// The tree install.sh runs from.
    pub tree: String,
    /// --check found something to update.
    pub found: bool,
    /// A version that ran here before has its image again.
    pub reused: bool,
    /// The run ends here with this status: there is no install.sh to ask.
    pub end: Option<u8>,
}

impl Plan {
    fn text(&self) -> String {
        let mut text = format!(
            "tree={}\nfound={}\nreused={}\n",
            self.tree, self.found as u8, self.reused as u8
        );
        if let Some(end) = self.end {
            text.push_str(&format!("end={end}\n"));
        }
        text
    }

    fn write(&self, file: &Path) -> io::Result<()> {
        fs::write(file, self.text())
    }
}

/// The words a program prints, line by line. A closed standard output does not
/// stop an update halfway, so nothing here panics on one.
pub(crate) fn say(line: &str) {
    let _ = writeln!(io::stdout(), "{line}");
}

pub(crate) fn tell(line: &str) {
    let _ = writeln!(io::stderr(), "{line}");
}

/// What `tehas_err` of lib/config.sh prints.
pub(crate) fn fault(line: &str) {
    tell(&format!("tehas: {line}"));
}

pub(crate) fn code(status: ExitStatus) -> i32 {
    status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
}

/// A program under `timeout <seconds>`.
pub(crate) fn timed(seconds: u32, program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut cmd = Command::new("timeout");
    cmd.arg(seconds.to_string()).arg(program);
    cmd
}

/// What a command printed on standard output, as a `$(...)` of the shell gives
/// it, with its exit status: 127 for one that could not be started.
pub(crate) fn output(cmd: &mut Command) -> (i32, String) {
    match cmd
        .stdout(Stdio::piped())
        .spawn()
        .and_then(|child| child.wait_with_output())
    {
        Ok(ran) => (
            code(ran.status),
            String::from_utf8_lossy(&ran.stdout)
                .trim_end_matches('\n')
                .to_string(),
        ),
        Err(_) => (127, String::new()),
    }
}

/// Whether a command ended well with nothing of it on the terminal.
pub(crate) fn quiet(cmd: &mut Command) -> bool {
    cmd.stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// The first line of a file, or nothing, as `head -n 1` gives it.
pub(crate) fn first_line(file: &Path) -> String {
    let mut text = Vec::new();
    if fs::File::open(file)
        .and_then(|mut f| f.read_to_end(&mut text))
        .is_err()
    {
        return String::new();
    }
    let line = text.split(|b| *b == b'\n').next().unwrap_or_default();
    String::from_utf8_lossy(line).into_owned()
}

/// A new directory of this account's alone, `<parent>/<prefix>XXXXXX`.
pub(crate) fn temp_dir(parent: &Path, prefix: &str) -> io::Result<PathBuf> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let seed = u64::from(std::process::id()) << 32 | u64::from(nanos);
    for n in 0..64u64 {
        let dir = parent.join(format!(
            "{prefix}{:06x}",
            seed.wrapping_add(n).wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 40
        ));
        match fs::DirBuilder::new().mode(0o700).create(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::ErrorKind::AlreadyExists.into())
}

/// The settings an update reads. `TEHAS_HOME` is what the script's `tehas_load`
/// left; without it nothing here knows where it is.
pub(crate) struct Host {
    pub home: PathBuf,
    pub image: String,
}

impl Host {
    pub fn from_env() -> Option<Host> {
        let home = env::var_os("TEHAS_HOME").filter(|h| !h.is_empty())?;
        Some(Host {
            home: PathBuf::from(home),
            image: env::var("PI_ENSEMBLE_IMAGE").unwrap_or_default(),
        })
    }

    pub fn checkout(&self) -> PathBuf {
        self.home.join("platform")
    }

    pub fn current(&self) -> PathBuf {
        self.home.join("current")
    }
}

pub(crate) fn host_or_refuse() -> Result<Host, u8> {
    Host::from_env().ok_or_else(|| {
        tell("console update: TEHAS_HOME is not set");
        78
    })
}

pub fn main(given: &[String]) -> u8 {
    match given.split_first() {
        Some((verb, rest)) if verb == "release" => release::verb(rest),
        Some((verb, rest)) if verb == "move" => release::moved(rest),
        _ => {
            tell("console update: usage: release <question> | move --plan <file> --mode update|check [--to <version>]");
            64
        }
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use std::fs;
    use std::path::{Path, PathBuf};

    /// A directory of a test's own, gone when the test ends.
    pub struct Dir(pub PathBuf);

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    pub fn dir(name: &str) -> Dir {
        let dir = std::env::temp_dir().join(format!("tehas-update-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Dir(dir)
    }

    impl Dir {
        pub fn file(&self, name: &str, text: &str) -> PathBuf {
            let path: PathBuf = self.0.join(name);
            fs::create_dir_all(path.parent().unwrap_or(Path::new("/"))).unwrap();
            fs::write(&path, text).unwrap();
            path
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::dir;
    use super::*;

    #[test]
    fn the_plan_says_each_field_on_its_line() {
        let plan = Plan {
            tree: "/h/current".into(),
            found: true,
            reused: false,
            end: Some(1),
        };
        assert_eq!(plan.text(), "tree=/h/current\nfound=1\nreused=0\nend=1\n");
        assert!(!Plan::default().text().contains("end="));
    }

    #[test]
    fn the_first_line_is_what_head_gives() {
        let d = dir("first-line");
        assert_eq!(first_line(&d.file("a", "v1.2.3\nmore\n")), "v1.2.3");
        assert_eq!(first_line(&d.file("b", "v1.2.3")), "v1.2.3");
        assert_eq!(first_line(&d.0.join("none")), "");
    }

    #[test]
    fn a_temp_dir_is_private_and_new() {
        use std::os::unix::fs::PermissionsExt;
        let d = dir("temp");
        let one = temp_dir(&d.0, "release.").unwrap();
        let two = temp_dir(&d.0, "release.").unwrap();
        assert_ne!(one, two);
        assert_eq!(
            fs::metadata(&one).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert!(temp_dir(&d.0.join("none"), "x").is_err());
    }
}
