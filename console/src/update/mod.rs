//! What `tehas update` decides and does. libexec/update holds the lock, because
//! install.sh of any version takes it as an inherited descriptor, and runs
//! install.sh between the verbs here; everything else of an update is in them.
//!
//!   console update args <arguments>
//!   console update gate <arguments>
//!   console update prepare --plan <file> <arguments>
//!   console update api
//!   console update finish --plan <file> --install-out <file> \
//!     --install-err <file> --install-rc <n>
//!   console update release source|installed|newest|keep
//!   console update release older|stale <version> <version>
//!   console update steps ids|units|valid <line>|for <source>...|record <source>...
//!
//! `args` judges the arguments before any setting is read, and answers --help.
//! `gate` says, with the settings loaded and before the lock, whether this run
//! may go on. `prepare` runs under the lock: it reads the steps an earlier run
//! owes, then moves the code, the checkout in src/update/checkout.rs or a release
//! in src/update/release.rs. It leaves what comes next in the plan file. `api`
//! says which of these this console speaks: install.sh may have replaced it while
//! the update ran, and the one left has to finish it. `finish` reads install.sh's
//! answer and runs the follow-ups, see src/update/finish.rs. `release` answers
//! one question about the releases, and `steps` names the steps and records them
//! for install.sh, see src/update/steps.rs.
//!
//! Every program is spawned by name from PATH, so a host's own tool, or a
//! test's stub, answers. Each call outside goes through `timeout`, and only
//! what the message says it waited for is waited for.
//!
//! Exit: as each verb says; 64 usage, 78 a setting is missing, 98 `gate`: go on
//! with no lock, 99 the run is over and went well, nothing more to do.

mod checkout;
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

/// This verb was the whole of the run, and it went well.
pub(crate) const ANSWERED: u8 = 99;

/// `gate`: go on, with no lock to take: --check changes nothing on the host.
const NO_LOCK: u8 = 98;

const USAGE: &str = include_str!("../usage/update.txt");

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
    /// The steps an earlier run owes, read before install.sh ran.
    pub leftover: Vec<String>,
    /// The lines of that record no step here knows.
    pub unknown: Vec<String>,
}

impl Plan {
    fn text(&self) -> String {
        // The script reads the first two lines.
        let mut text = format!(
            "tree={}\nmode={}\nfrom={}\nfound={}\nreused={}\n",
            self.tree,
            self.mode.word(),
            self.from,
            self.found as u8,
            self.reused as u8
        );
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

/// What a person asked for. The words have been judged before this is made.
#[derive(Debug, Default, PartialEq)]
struct Asked {
    mode: Mode,
    to: Option<String>,
}

/// The arguments of `tehas update`, or the status they end the run with. The
/// pair of --pending and another mode is refused whichever way round it was
/// written and before the settings are read: a program that asks for two modes
/// gets 64, not the answer of the one that happened to be read last.
fn parse(given: &[String]) -> Result<Asked, u8> {
    let joined = given.join(" ");
    let help = format!(" {joined} ");
    if help.contains(" -h ") || help.contains(" --help ") {
        if given.len() != 1 {
            tell(&format!(
                "tehas update: --help takes no other argument, got {joined}"
            ));
            let _ = io::stderr().write_all(USAGE.as_bytes());
            return Err(64);
        }
        let _ = io::stdout().write_all(USAGE.as_bytes());
        return Err(ANSWERED);
    }
    let (mut checked, mut pending) = (false, false);
    let mut to = String::new();
    let mut words = given.iter();
    while let Some(word) = words.next() {
        match word.as_str() {
            "--check" => checked = true,
            "--pending" => pending = true,
            "--to" => match words.next() {
                Some(version) => to = version.clone(),
                None => {
                    tell("tehas update: --to needs a version");
                    return Err(64);
                }
            },
            _ => {
                tell(&format!("tehas update: unknown argument: {word}"));
                let _ = io::stderr().write_all(USAGE.as_bytes());
                return Err(64);
            }
        }
    }
    if pending && (checked || !to.is_empty()) {
        let other = match (checked, to.is_empty()) {
            (true, false) => "--check --to",
            (true, true) => "--check",
            _ => "--to",
        };
        tell(&format!(
            "tehas update: --pending runs the recorded follow-ups, and does not go with {other}"
        ));
        tell("  it takes neither --check nor --to");
        return Err(64);
    }
    if !to.is_empty() && !release::tag_ok(&to) {
        tell(&format!(
            "tehas update: --to takes a version vX.Y.Z, got: {to}"
        ));
        return Err(64);
    }
    let mode = if pending {
        Mode::Pending
    } else if checked {
        Mode::Check
    } else {
        Mode::Update
    };
    Ok(Asked {
        mode,
        to: (!to.is_empty()).then_some(to),
    })
}

/// `console update args <arguments>`: --help answers on a host with no settings
/// yet, and an argument this command does not know reaches neither git nor Docker.
fn args(given: &[String]) -> u8 {
    parse(given).map_or_else(|rc| rc, |_| 0)
}

/// What is decided with the settings loaded, before the lock. --to names its
/// version, so it is not held: the hold keeps a plain update from moving the host
/// on after a person moved it. --pending moves nothing either, so a host rolled
/// back by hand still finishes the follow-ups install.sh recorded for it.
fn pass(host: &Host, asked: &Asked) -> u8 {
    if asked.mode == Mode::Pending {
        return 0;
    }
    let from = match release::source(host) {
        Ok(from) => from,
        Err(rc) => return rc,
    };
    if asked.to.is_some() && from != "release" {
        tell("tehas update: --to installs a release, and this host updates from its platform checkout (TEHAS_UPDATE_FROM=git); nothing was changed");
        return 64;
    }
    let hold = host.home.join("state/update-hold");
    if hold.exists() && asked.to.is_none() {
        if asked.mode == Mode::Update {
            tell(&format!(
                "tehas update: {} exists, the platform is held where it is; nothing was changed. Remove the file to update.",
                hold.display()
            ));
            return 1;
        }
        tell(&format!(
            "tehas update: {} exists; an update will not move the checkout until it is removed",
            hold.display()
        ));
    }
    // .git is a directory in a clone and a file in a worktree or a submodule.
    let checkout = host.checkout();
    if from == "git" && (!checkout.join(".git").exists() || !checkout.join("install.sh").is_file())
    {
        tell(&format!(
            "tehas update: no platform checkout at {}. Clone the platform repository there, or run ./install.sh from wherever you keep it.",
            checkout.display()
        ));
        return 78;
    }
    0
}

/// `console update gate <arguments>`. Exit: 0 take the lock and go on, 98 go on
/// with no lock, else the status of the run.
fn gate(given: &[String]) -> u8 {
    let asked = match parse(given) {
        Ok(asked) => asked,
        Err(rc) => return rc,
    };
    let host = match host_or_refuse() {
        Ok(host) => host,
        Err(rc) => return rc,
    };
    match pass(&host, &asked) {
        0 if asked.mode == Mode::Check => NO_LOCK,
        rc => rc,
    }
}

/// The steps an earlier run was asked for and did not finish are still owed:
/// install.sh's report of what changed is spent by the run that made the copies.
/// A line no step here knows is not owed work this command can do; it is named
/// at the end, kept in the record, and the run ends red.
fn leftover(host: &Host, plan: &mut Plan) -> u8 {
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
    let nothing = known.is_empty() && unknown.is_empty();
    plan.leftover = known;
    plan.unknown = unknown;
    if plan.mode == Mode::Pending && nothing {
        say("update: nothing is owed");
        return ANSWERED;
    }
    0
}

/// `console update prepare --plan <file> <arguments>`, under the lock. Exit: 0
/// run what the plan says, 99 the run is over and went well, else its status.
fn prepare(given: &[String]) -> u8 {
    let [flag, file, rest @ ..] = given else {
        tell("console update prepare: --plan is required");
        return 64;
    };
    if flag != "--plan" {
        tell("console update prepare: --plan is required");
        return 64;
    }
    let asked = match parse(rest) {
        Ok(asked) => asked,
        Err(rc) => return rc,
    };
    let host = match host_or_refuse() {
        Ok(host) => host,
        Err(rc) => return rc,
    };
    let mut plan = Plan {
        mode: asked.mode,
        ..Plan::default()
    };
    let rc = leftover(&host, &mut plan);
    if rc != 0 {
        return rc;
    }
    // --pending reads no checkout and no remote: the record is the whole of the
    // work, so a host whose checkout is at a release tag finishes its install.
    if plan.mode != Mode::Pending {
        plan.from = match release::source(&host) {
            Ok(from) => from.to_string(),
            Err(rc) => return rc,
        };
        let rc = if plan.from == "git" {
            checkout::pull(&host, &mut plan)
        } else {
            release::go(&host, asked.to.as_deref(), &mut plan)
        };
        if rc != 0 {
            return rc;
        }
    }
    if plan.write(Path::new(file)).is_err() {
        tell(&format!(
            "tehas update: could not write {file}; run tehas update again to finish"
        ));
        return 1;
    }
    0
}

pub fn main(given: &[String]) -> u8 {
    match given.split_first() {
        Some((verb, rest)) if verb == "args" => args(rest),
        Some((verb, rest)) if verb == "gate" => gate(rest),
        Some((verb, rest)) if verb == "prepare" => prepare(rest),
        Some((verb, [])) if verb == "api" => {
            say(&API.to_string());
            0
        }
        Some((verb, rest)) if verb == "finish" => finish::main(rest),
        Some((verb, rest)) if verb == "release" => release::verb(rest),
        Some((verb, rest)) if verb == "steps" => steps::verb(rest),
        _ => {
            tell("console update: usage: args | gate | prepare | api | finish | release <question> | steps <question>");
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
            leftover: vec!["docs --force-recreate".into(), "sandbox".into()],
            unknown: vec!["rm -rf something=else".into()],
        };
        assert_eq!(Plan::parse(&plan.text()).unwrap(), plan);
        assert_eq!(
            Plan::parse(&Plan::default().text()).unwrap(),
            Plan::default()
        );
        assert!(plan.text().starts_with("tree=/h/cur=rent\nmode=check\n"));
    }

    #[test]
    fn a_plan_this_console_cannot_read_is_no_plan() {
        assert_eq!(Plan::parse("no equals sign\n"), None);
        assert_eq!(Plan::parse("mode=later\n"), None);
        assert_eq!(Plan::parse("newer=1\n"), None);
    }

    fn words(given: &[&str]) -> Vec<String> {
        given.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn the_arguments_name_a_mode_and_a_version() {
        let asked = |given: &[&str]| parse(&words(given));
        assert_eq!(asked(&[]), Ok(Asked::default()));
        assert_eq!(asked(&["--check"]).unwrap().mode, Mode::Check);
        assert_eq!(asked(&["--pending"]).unwrap().mode, Mode::Pending);
        let to = asked(&["--check", "--to", "v1.2.3"]).unwrap();
        assert_eq!((to.mode, to.to.as_deref()), (Mode::Check, Some("v1.2.3")));
        // An empty version is no version asked for.
        assert_eq!(asked(&["--to", ""]), Ok(Asked::default()));
    }

    #[test]
    fn a_wrong_argument_is_64_and_help_is_the_whole_answer() {
        for bad in [
            &["--dryrun"][..],
            &["--to"],
            &["--to", "latest"],
            &["--to", "1.2.3"],
            &["--check", "--help"],
            &["-h", "--check"],
            &["--to", "--help"],
            &["--pending", "--check"],
            &["--check", "--pending"],
            &["--pending", "--to", "v1.2.3"],
            &["--to", "v1.2.3", "--pending"],
        ] {
            assert_eq!(parse(&words(bad)), Err(64), "{bad:?}");
        }
        assert_eq!(args(&words(&["--help"])), ANSWERED);
        assert_eq!(args(&words(&["-h"])), ANSWERED);
        assert_eq!(args(&words(&["--pending"])), 0);
    }

    #[test]
    fn the_usage_names_every_form() {
        for form in ["--check", "--pending", "--to vX.Y.Z", "-h|--help"] {
            assert!(USAGE.contains(form), "{form}");
        }
    }

    fn home(d: &testing::Dir) -> Host {
        Host {
            home: d.0.clone(),
            image: String::new(),
        }
    }

    #[test]
    fn a_hold_stops_a_plain_update_and_neither_to_nor_pending_nor_check() {
        use std::os::unix::fs::symlink;
        let d = dir("gate-hold");
        d.file("state/update-hold", "");
        d.file("versions/v1.0.0/VERSION", "v1.0.0\n");
        symlink("versions/v1.0.0", d.0.join("current")).unwrap();
        let h = home(&d);
        let asked = |mode, to: Option<&str>| Asked {
            mode,
            to: to.map(str::to_string),
        };
        assert_eq!(pass(&h, &asked(Mode::Update, None)), 1);
        assert_eq!(pass(&h, &asked(Mode::Check, None)), 0);
        assert_eq!(pass(&h, &asked(Mode::Update, Some("v1.0.0"))), 0);
        assert_eq!(pass(&h, &asked(Mode::Pending, None)), 0);
    }

    #[test]
    fn a_host_with_no_checkout_and_no_current_is_not_guessed_at() {
        let d = dir("gate-bare");
        let h = home(&d);
        let run = |mode| pass(&h, &Asked { mode, to: None });
        assert_eq!(run(Mode::Update), 78);
        assert_eq!(run(Mode::Check), 78);
        // --pending asks of the host only that its settings read.
        assert_eq!(run(Mode::Pending), 0);
    }

    #[test]
    fn nothing_owed_ends_pending_and_only_pending() {
        let d = dir("leftover-empty");
        let h = home(&d);
        let run = |mode| {
            let mut plan = Plan {
                mode,
                ..Plan::default()
            };
            leftover(&h, &mut plan)
        };
        assert_eq!(run(Mode::Pending), ANSWERED);
        assert_eq!(run(Mode::Update), 0);
        assert_eq!(run(Mode::Check), 0);
    }

    #[test]
    fn the_record_is_read_into_what_is_owed_and_what_no_step_knows() {
        let d = dir("leftover-lines");
        d.file("state/update-pending", "sandbox\nrenew-dashboard --now\n");
        let h = home(&d);
        let mut plan = Plan {
            mode: Mode::Pending,
            ..Plan::default()
        };
        assert_eq!(leftover(&h, &mut plan), 0);
        assert_eq!(plan.leftover, ["sandbox"]);
        assert_eq!(plan.unknown, ["renew-dashboard --now"]);
    }

    #[test]
    fn a_record_that_cannot_be_read_stops_the_run() {
        let d = dir("leftover-unreadable");
        fs::create_dir_all(d.0.join("state/update-pending")).unwrap();
        let h = home(&d);
        assert_eq!(leftover(&h, &mut Plan::default()), 1);
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
