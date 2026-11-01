//! What `tehas update` decides and does. libexec/update holds the lock and
//! runs install.sh; everything else of an update is here.
//!
//!   console update api
//!   console update leftover --plan <file> --mode update|check|pending [--from git|release]
//!   console update move --plan <file> [--to <version>]
//!   console update finish --plan <file> [--tree <dir>] [--found 0|1] \
//!     --install-out <file> --install-err <file> --install-rc <n>
//!   console update release source|installed|newest|keep
//!   console update release older|stale <version> <version>
//!   console update steps ids|units|valid <line>|for <source>...|record <source>...
//!
//! `api` says which of these this console speaks: install.sh may have replaced
//! the console while an update ran, and the one left has to finish it.
//! `leftover` reads the steps an earlier run owes. `move` chooses a version,
//! downloads and checks it and switches current, see src/update/release.rs.
//! Each leaves what comes next in the plan file. `finish` reads install.sh's
//! answer and runs the follow-ups, see src/update/finish.rs. `release` answers
//! one question about the releases and `steps` names the steps, see
//! src/update/steps.rs.
//!
//! Every program is spawned by name from PATH, so a host's own tool, or a
//! test's stub, answers. Each call outside goes through `timeout`, and only
//! what the message says it waited for is waited for.
//!
//! Exit: as each verb says; 64 usage, 78 a setting is missing, 99 the run is
//! over and went well, which `leftover` says of --pending with nothing owed.

mod finish;
mod release;
mod steps;

use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

/// What `console update api` answers. libexec/update passes this plan and these
/// arguments to `finish`; a console that answers anything else cannot finish it.
const API: u8 = 1;

/// `leftover` answered the whole run.
const ANSWERED: u8 = 99;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum Mode {
    #[default]
    Update,
    Check,
    Pending,
}

impl Mode {
    fn parse(word: &str) -> Option<Mode> {
        match word {
            "update" => Some(Mode::Update),
            "check" => Some(Mode::Check),
            "pending" => Some(Mode::Pending),
            _ => None,
        }
    }

    fn word(self) -> &'static str {
        match self {
            Mode::Update => "update",
            Mode::Check => "check",
            Mode::Pending => "pending",
        }
    }
}

/// What a run of `tehas update` knows at one step and the next needs. It is a
/// file because the steps are separate programs, and the console that finishes
/// a run may be another one than the console that began it: `API` is what says
/// the two read this file alike.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Plan {
    pub mode: Mode,
    /// git or release; nothing for --pending, which asks neither.
    pub from: String,
    /// The tree install.sh runs from.
    pub tree: String,
    /// --check found something to update.
    pub found: bool,
    /// A version that ran here before has its image again.
    pub reused: bool,
    /// The run ends here with this status: there is no install.sh to ask.
    pub end: Option<u8>,
    /// The steps an earlier run owes, read before install.sh ran.
    pub leftover: Vec<String>,
    /// The lines of that record no step here knows.
    pub unknown: Vec<String>,
}

impl Plan {
    fn text(&self) -> String {
        let mut text = format!(
            "mode={}\nfrom={}\ntree={}\nfound={}\nreused={}\n",
            self.mode.word(),
            self.from,
            self.tree,
            self.found as u8,
            self.reused as u8
        );
        if let Some(end) = self.end {
            text.push_str(&format!("end={end}\n"));
        }
        for line in &self.leftover {
            text.push_str(&format!("leftover={line}\n"));
        }
        for line in &self.unknown {
            text.push_str(&format!("unknown={line}\n"));
        }
        text
    }

    fn parse(text: &str) -> Option<Plan> {
        let mut plan = Plan::default();
        for line in text.lines() {
            let (key, value) = line.split_once('=')?;
            match key {
                "mode" => plan.mode = Mode::parse(value)?,
                "from" => plan.from = value.to_string(),
                "tree" => plan.tree = value.to_string(),
                "found" => plan.found = value != "0",
                "reused" => plan.reused = value != "0",
                "end" => plan.end = Some(value.parse().ok()?),
                "leftover" => plan.leftover.push(value.to_string()),
                "unknown" => plan.unknown.push(value.to_string()),
                _ => return None,
            }
        }
        Some(plan)
    }

    fn read(file: &Path) -> Option<Plan> {
        Plan::parse(&fs::read_to_string(file).ok()?)
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

/// The steps an earlier run was asked for and did not finish are still owed:
/// install.sh's report of what changed is spent by the run that made the copies.
/// A line no step here knows is not owed work this command can do; it is named
/// at the end, kept in the record, and the run ends red.
fn leftover(given: &[String]) -> u8 {
    let (mut file, mut mode, mut from) = (None, None, None);
    let mut words = given.iter();
    while let Some(word) = words.next() {
        let slot = match word.as_str() {
            "--plan" => &mut file,
            "--mode" => &mut mode,
            "--from" => &mut from,
            _ => {
                tell(&format!(
                    "console update leftover: unknown argument: {word}"
                ));
                return 64;
            }
        };
        *slot = words.next().map(String::as_str);
    }
    let (Some(file), Some(mode)) = (file, mode.and_then(Mode::parse)) else {
        tell("console update leftover: --plan and --mode are required");
        return 64;
    };
    let host = match host_or_refuse() {
        Ok(host) => host,
        Err(rc) => return rc,
    };
    let Ok(record) = steps::read_record(&host.home) else {
        tell("update: the follow-up record is there but cannot be read; nothing was run");
        return 1;
    };
    let (known, unknown): (Vec<String>, Vec<String>) =
        record.into_iter().partition(|line| steps::valid(line));
    if !known.is_empty() {
        tell(&format!(
            "update: {} follow-up(s) an earlier run did not finish (see {})",
            known.len(),
            steps::record_file(&host.home).display()
        ));
        for line in &known {
            tell(&format!("  next: {line}"));
        }
    }
    if mode == Mode::Pending && known.is_empty() && unknown.is_empty() {
        say("update: nothing is owed");
        return ANSWERED;
    }
    let plan = Plan {
        mode,
        from: from.unwrap_or("").to_string(),
        leftover: known,
        unknown,
        ..Plan::default()
    };
    if plan.write(Path::new(file)).is_err() {
        tell(&format!(
            "tehas update: could not write {file}; nothing was changed"
        ));
        return 1;
    }
    0
}

pub fn main(given: &[String]) -> u8 {
    match given.split_first() {
        Some((verb, [])) if verb == "api" => {
            say(&API.to_string());
            0
        }
        Some((verb, rest)) if verb == "leftover" => leftover(rest),
        Some((verb, rest)) if verb == "move" => release::moved(rest),
        Some((verb, rest)) if verb == "finish" => finish::main(rest),
        Some((verb, rest)) if verb == "release" => release::verb(rest),
        Some((verb, rest)) if verb == "steps" => steps::verb(rest),
        _ => {
            tell("console update: usage: api | leftover | move | finish | release <question> | steps <question>");
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
    fn the_plan_reads_back_as_it_was_written() {
        let plan = Plan {
            mode: Mode::Check,
            from: "release".into(),
            tree: "/h/cur=rent".into(),
            found: true,
            reused: false,
            end: Some(1),
            leftover: vec!["docs --force-recreate".into(), "sandbox".into()],
            unknown: vec!["rm -rf something=else".into()],
        };
        assert_eq!(Plan::parse(&plan.text()).unwrap(), plan);
        assert_eq!(
            Plan::parse(&Plan::default().text()).unwrap(),
            Plan::default()
        );
        assert!(!Plan::default().text().contains("end="));
    }

    #[test]
    fn a_plan_this_console_cannot_read_is_no_plan() {
        assert_eq!(Plan::parse("no equals sign\n"), None);
        assert_eq!(Plan::parse("mode=later\n"), None);
        assert_eq!(Plan::parse("newer=1\n"), None);
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
