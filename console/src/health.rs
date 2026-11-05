//! The factory's health file, live/health.json: pass, warn or fail, the one
//! line that says why, and every check under `checks`. `tehasbot health` is
//! `console tehasbot health`, which `console run` runs once the
//! settings are loaded, and which comes to `console health run`.
//!
//!   console health run <home>
//!     The whole run, in the order the shell had it: the store's answers,
//!     the units read, the report, and the publish of the report. Each verb is
//!     documented beside the function that makes it; the flags of `report`
//!     below are the usage `main` prints.
//!     Exit 0 the file was written; 1 it could not be written, or the store
//!     refused one call or did not answer; 78 the store is not installed or
//!     cannot be used, which writes nothing; 2 usage. An `ERROR:` line says
//!     which of them it was, on standard error. A read that did not answer
//!     says so with a `WARN:` line and leaves its own check out of the file,
//!     as the shell's left the units, the backup and the polls out.
//!
//!   console health report <home> [--sessions <json>] [--units <text>]
//!     [--backup <json>] [--poll-over <json> --poll-budget <spec>] [--mcp <json>]
//!     The report alone on standard output, from the answers a shell gathered
//!     and gave as arguments; nothing is written. It is the part `run` calls
//!     after it made those answers itself.
//!
//! It reads what the factory already writes under <home>/live/ - the queue
//! snapshot, the cycles and their endings, the last poll's slice and demand,
//! the previews' disk and the platform - and under <home>/state/ the pause
//! flag, the limit and the locks of the planning turns. What only the store or
//! systemd know comes in as arguments, as the shell got it: the open planning
//! sessions (`store plan-show --open`), the failed tehas units (`systemctl
//! --user --failed --plain --no-legend`, left out when systemd did not answer
//! with a list, empty or not), the store's newest backup (`store
//! backup --show`, left out when the store did not answer) and the idle polls
//! over their budget (`store poll-idle --budget`, left out when it refused).
//! A file that is not there, or not what it should be, is null. The report
//! goes to standard output; nothing is written.
//!
//! Exit: 0 the report was printed; 2 usage.

use crate::flags::{self, epoch};
use crate::jq::{first, obj, J, NULL};
use crate::mcp::{liveness, Liveness};
use crate::store::{self, bounded, merged, Refusal};
use serde_json::Value;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::thread::sleep;
use std::time::{Duration, Instant};
use tehas_core::grammar::an_end;
use tehas_core::hold;
use tehas_core::time;

/// A queue snapshot older than this is not news.
const QUEUE_MAX_AGE: i64 = 600;
/// The last poll's slice and demand are not believed after this.
const POLL_MAX_AGE: i64 = 600;
/// The largest poll file the page reads. The store bounds the same files at
/// the same `READ_BOUND` when it publishes them (`REPORT_BOUND` in
/// console/store/src/live.rs); the read here is the parser's only other door, and our parser is slower on
/// a big object than the store's, so the bound is kept on this side too.
const POLL_FILE_BOUND: u64 = tehas_core::file::READ_BOUND;
/// The last read of the epics is not believed after this: `tehasbot status`
/// writes it at least every five minutes while the polls run.
const EPICS_MAX_AGE: i64 = 600;
/// The most issues the output line names of those labelled `epic` with no
/// sub-issues; `checks.epics` lists them all.
const UNLINKED_SAID: usize = 5;
/// The previews' measure of the disk is not believed after this.
const DISK_MAX_AGE: i64 = 1800;
/// A platform this long behind is news.
const BEHIND_FOR: i64 = 86_400;
/// The newest backup of the store is old after this.
const BACKUP_MAX_AGE: f64 = 172_800.0;
/// The whole publish of the report, three tries of a store that is busy or
/// timed out included. Each try is bounded at 20 s of its own and the poll
/// that runs health bounds the run at 120 s, so the publish must stop well
/// inside that or the run is killed with no file written.
const PUBLISH_BUDGET: Duration = Duration::from_secs(30);
/// The budget of the idle polls when the host's settings did not name one,
/// as `lib/config.sh` exports it.
const POLL_BUDGET: &str = "cpu=5,wall=30,writes=0";

#[derive(Default)]
struct Given {
    sessions: Option<String>,
    units: Option<String>,
    backup: Option<String>,
    poll_over: Option<String>,
    poll_budget: Option<String>,
    mcp: Option<String>,
}

/// Where the run looks for the MCP server: `TEHAS_MCP` and `TEHAS_HOST_ADDR`
/// as the settings gave them, and the port, which a case names for itself.
struct Mcp {
    on: String,
    addr: String,
    port: u16,
}

impl Mcp {
    fn of_settings() -> Self {
        let var = |name| env::var(name).unwrap_or_default();
        Self {
            on: var("TEHAS_MCP"),
            addr: var("TEHAS_HOST_ADDR"),
            port: crate::mcp::PORT,
        }
    }
}

/// The probe of the MCP server is bounded at this, to connect and again for
/// the answer.
const MCP_BOUND: Duration = Duration::from_secs(2);

/// `checks.mcp`: `{on}` of a server that is off, else whether it is
/// installed and answers, and why not when it does not.
fn mcp_check(l: &Liveness) -> J {
    let on = |installed: bool, answers: bool, reason: Option<&str>| {
        let mut o = vec![
            ("on", J::Bool(true)),
            ("installed", J::Bool(installed)),
            ("answers", J::Bool(answers)),
        ];
        if let Some(r) = reason {
            o.push(("reason", J::str(r)));
        }
        obj(o)
    };
    match l {
        Liveness::Off => obj(vec![("on", J::Bool(false))]),
        Liveness::NotInstalled => on(
            false,
            false,
            Some("libexec/tehas-mcp is not there: run install.sh"),
        ),
        Liveness::Answers => on(true, true, None),
        Liveness::Silent(why) => on(true, false, Some(why)),
    }
}

/// What a run asks of the store. `PlanShow` and `Publish` stop the run with
/// an ERROR line and an exit status of their own; `Backup` and `PollIdle` are
/// checks whose refusal leaves their own entry out of the file and says so
/// with a WARN line. The publish of the report is not one of them: it is tried
/// while the store is busy and its line is `refused_publish`'s.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Step {
    PlanShow,
    Publish,
    Backup,
    PollIdle,
}

impl Step {
    /// The reads whose refusal leaves their own check out rather than stop the
    /// run. On the type, so a new step is a decision here and not a silent
    /// fallthrough to the run-ending path.
    fn optional(self) -> bool {
        matches!(self, Self::Backup | Self::PollIdle)
    }

    /// What the read is called in the WARN line of a refusal that leaves it
    /// out: the thing the reader looks for, not the verb.
    fn name(self) -> &'static str {
        match self {
            Self::Backup => "newest backup",
            Self::PollIdle => "idle polls",
            Self::PlanShow | Self::Publish => "read",
        }
    }
}

/// `systemctl --user --failed --plain --no-legend 'tehas-*'` of <program>
/// under the same 10 s bound the shell gave it. The production caller names
/// `systemctl` and the host's PATH answers it; a test names a stub of its own,
/// which is the only seam this read has. Nothing on the read's own answers is
/// trusted beyond the list: each line is one unit name.
fn failed_units_of(program: &Path) -> Option<String> {
    let mut cmd = bounded("2", "10", program);
    cmd.args(["--user", "--failed", "--plain", "--no-legend", "tehas-*"])
        .stdin(Stdio::null());
    let (rc, said) = merged(&mut cmd);
    if rc != 0 {
        // The one outside call of the run that used to fail in silence: a
        // systemd that is not on PATH, a user bus that is down, or one that
        // was silent past the bound, all read as no failed unit unless the
        // run says which of them it was.
        eprintln!("WARN: systemctl gave no answer for the failed units (rc {rc})");
        return None;
    }
    let said = said.trim_end_matches('\n');
    Some(if said.is_empty() {
        String::new()
    } else {
        said.to_string()
    })
}

/// The file of the report, as the store's publish reads it: a plain file,
/// mode 0600, that no other process holds the name of. `<home>/tmp` is made
/// 0700 whatever the umask, and is refused when it is a link: the run writes
/// through it, so it must be a real directory of its own.
fn report_file(home: &Path, text: &str) -> io::Result<PathBuf> {
    let dir = home.join("tmp");
    if let Err(e) = fs::DirBuilder::new().mode(0o700).create(&dir) {
        if e.kind() != io::ErrorKind::AlreadyExists {
            return Err(e);
        }
    }
    // As the poll's own scratch writer: a directory of another type, or one it
    // cannot read the mode of, is no place to write the report.
    let made_dir = fs::symlink_metadata(&dir)
        .ok()
        .is_some_and(|m| m.is_dir() && !m.file_type().is_symlink());
    if !made_dir {
        return Err(io::Error::other(format!(
            "{} is not a directory of its own",
            dir.display()
        )));
    }
    if fs::symlink_metadata(&dir).is_ok_and(|m| m.permissions().mode() & 0o777 != 0o700) {
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    }
    // The name is this process's own, so two runs of the console do not share
    // one file.
    let path = dir.join(format!("health.{}", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    match file.write_all(text.as_bytes()) {
        Ok(()) => Ok(path),
        Err(e) => {
            let _ = fs::remove_file(&path);
            Err(e)
        }
    }
}

/// What a refused publish of the report says, in the voice the shell had.
/// `Refusal::line` is the store's first line, already cut to 300 bytes and
/// freed of control characters by the `refusal` of store.rs, as
/// `tehas_store_said` kept it.
fn refused_publish(no: &Refusal) -> u8 {
    if no.word == "absent" {
        return not_installed();
    }
    eprintln!("ERROR: the health file could not be published: {}", no.line);
    // The status the store's own answer carries: 1 for a store that was busy,
    // 78 for one that cannot be used. Nothing else is answered.
    if no.rc == 1 {
        1
    } else {
        78
    }
}

/// The publish of the report: `tehas_store_try 3 publish --health <file>`,
/// again while the store is busy or timed out, 1 s apart, but only within
/// <budget> of its own. The poll bounds the whole health run at 120 s while it
/// holds state/dispatch.lock, and each try is itself bounded at 20 s, so three
/// slow tries would reach that bound and leave no file at all: the earlier of
/// the three tries and the budget is what answers.
fn publish_health(home: &Path, file: &Path, budget: Duration) -> Result<(), Refusal> {
    let arg = file.display().to_string();
    let args = ["publish", "--health", arg.as_str()];
    let began = Instant::now();
    let mut tries = 1;
    loop {
        let err = match store::call(home, &args) {
            Ok(_) => return Ok(()),
            Err(no) => no,
        };
        if tries >= 3
            || err.rc != 1
            || !matches!(err.word.as_str(), "busy" | "timeout")
            || began.elapsed() >= budget
        {
            return Err(err);
        }
        tries += 1;
        sleep(Duration::from_secs(1));
    }
}

/// The four reads of the store the report is built from, in the order the
/// shell asked for them: the open planning sessions, the `publish` of the
/// views, the newest backup, and the figures of the idle polls over their
/// budget. A backup the store would not say is a null of its own, which is the
/// check that says there is no copy at all; a `poll_cost` the store refused is
/// left out rather than passed. Both say so with a WARN line: the units, the
/// backup and the polls are the checks a run goes without, and a home that
/// passes with three of them missing is a home whose store did not answer.
fn checks(home: &Path, budget: &str, systemctl: &Path, mcp: &Mcp) -> Result<Given, u8> {
    // Each ask ends the run with the status of its refusal, which is the
    // status of the step and not of a later one.
    let ask = |step: Step, args: &[&str]| -> Result<Option<Value>, u8> {
        match store::call(home, args) {
            Ok(v) => Ok(Some(v)),
            // A read the report can go without: its own entry is what is left
            // out, and the line says the read did not answer. Only a store
            // that was busy or timed out is quietly missing; anything else
            // says so at ERROR, because a store that cannot be used is not a
            // check that went without.
            Err(no) if step.optional() => {
                eprintln!(
                    "{}: the {} of the state store could not be read: {}",
                    if no.rc == 1 { "WARN" } else { "ERROR" },
                    step.name(),
                    no.line
                );
                Ok(None)
            }
            // One try, and only a store that cannot be used at all ends the
            // run: a store that was busy leaves the views a tick old, which is
            // what the shell's health accepted.
            Err(no) if step == Step::Publish && no.rc == 1 => Ok(None),
            Err(no) => Err(stopped(step, &no)),
        }
    };
    // The whole answer, which is what `--sessions` of `report` was given by
    // the shell: the report reads the sessions out of it.
    let sessions = ask(Step::PlanShow, &["plan-show", "--open"])?
        .map(|v| v.to_string())
        .unwrap_or_else(|| "null".to_string());
    ask(Step::Publish, &["publish"])?;
    // The whole answer of the read of the newest copy, which is what
    // `--backup` of `report` was given by the shell.
    let backup = ask(Step::Backup, &["backup", "--show"])?.map(|v| v.to_string());
    let over = ask(
        Step::PollIdle,
        &["poll-idle", "--since", "3600", "--budget", budget],
    )?
    .map(|v| v["over"].to_string());
    // The shell's `tehas_store_get .over` of a read that answered is the empty
    // list when nothing is over budget, and the report keeps the entry it was
    // given; only a read that refused leaves it out. The budget goes with it
    // and with nothing else: report renders poll_cost only when both are there.
    let poll_budget = over.as_ref().map(|_| budget.to_string());
    // systemd, bounded at 10 s; None leaves the units out of the file.
    let units = failed_units_of(systemctl);
    let mcp = liveness(home, &mcp.on, &mcp.addr, mcp.port, MCP_BOUND);
    Ok(Given {
        sessions: Some(sessions),
        units,
        backup,
        poll_over: over,
        poll_budget,
        mcp: Some(mcp_check(&mcp).dump()),
    })
}

/// `tehas_store_ready`, whose failure is its own: the shell named no reason
/// then and said the store was not installed.
fn not_installed() -> u8 {
    eprintln!("ERROR: the state store is not installed (run install.sh)");
    78
}

/// The steps of the run whose refusal ends it, with the line and the status
/// the shell gave them: the sessions the report's `plans` check is built from,
/// and the `publish` of the views, which a store that could not be used at all
/// makes nothing of. The optional checks never reach here: `checks`' ask is
/// what decides that, once.
fn stopped(step: Step, no: &Refusal) -> u8 {
    if no.word == "absent" {
        return not_installed();
    }
    match step {
        Step::PlanShow => {
            eprintln!(
                "ERROR: the planning sessions could not be read from the state store: {}",
                no.line
            );
            // The status the store's own answer carries.
            if no.rc == 1 {
                1
            } else {
                78
            }
        }
        Step::Publish => {
            eprintln!("ERROR: the live view could not be published: {}", no.line);
            // One try, and only a store that cannot be used at all ends the
            // run: a store that was busy leaves the views a tick old.
            if no.rc == 1 {
                0
            } else {
                78
            }
        }
        // The two optional checks are answered by ask and cannot reach here;
        // they end the run as a refused read would rather than be passed off
        // as a step that stopped nothing.
        Step::Backup | Step::PollIdle => 78,
    }
}

/// One run, as `tehasbot health` ran it: the store's answers in the order the
/// shell asked for them, then `report`, then the publish of what it made. A
/// read of the store that refused is no reason to write no file; only the
/// sessions, a `publish` the store could not use at all, and the publish of
/// the report stop the run, each with the line and the status the shell gave
/// them. A run asked for no budget of the idle polls, as the cases are, takes
/// the instance's default.
pub fn run(given: &[String]) -> u8 {
    let [home, rest @ ..] = given else {
        eprintln!("console health: usage: run <home> [--budget <spec>]");
        return 2;
    };
    let mut budget: Option<String> = None;
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        match (flag.as_str(), it.next()) {
            ("--budget", Some(v)) => budget = Some(v.clone()),
            _ => {
                eprintln!("console health: usage: run <home> [--budget <spec>]");
                return 2;
            }
        }
    }
    run_with(
        Path::new(home),
        budget.unwrap_or_else(|| POLL_BUDGET.to_string()),
        Path::new("systemctl"),
        &Mcp::of_settings(),
    )
}

/// `tehasbot health`, under settings already loaded: the load is the one place
/// the shape of `TEHAS_POLL_BUDGET` is checked, and it gives the home.
pub fn loaded() -> u8 {
    let Some(home) = env::var_os("TEHAS_HOME") else {
        eprintln!("tehasbot health: TEHAS_HOME is not set");
        return 78;
    };
    let budget = env::var("TEHAS_POLL_BUDGET")
        .ok()
        .filter(|b| !b.is_empty())
        .unwrap_or_else(|| POLL_BUDGET.to_string());
    run_with(
        Path::new(&home),
        budget,
        Path::new("systemctl"),
        &Mcp::of_settings(),
    )
}

/// The run itself, with the budget the host's settings give: the case that
/// names a budget of its own does not reach over the environment of the
/// process the tests of the workspace share. The units read is the read of
/// systemd, which a case of the units names for itself.
fn run_with(home: &Path, budget: String, systemctl: &Path, mcp: &Mcp) -> u8 {
    if !store::ready(home) {
        return not_installed();
    }
    let given = match checks(home, &budget, systemctl, mcp) {
        Ok(gathered) => gathered,
        Err(rc) => return rc,
    };
    let text = format!("{}\n", report(home, &given, time::now()).pretty());
    let file = match report_file(home, &text) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("ERROR: the health file could not be written: {e}");
            return 1;
        }
    };
    let outcome = publish_health(home, &file, PUBLISH_BUDGET);
    let _ = fs::remove_file(&file);
    match outcome {
        Ok(()) => 0,
        Err(no) => refused_publish(&no),
    }
}

/// A run of `console health run` against a home with the store of this build
/// in it. The store answers the verbs itself; a case that wants a refusal
/// wraps it in a script that refuses the one verb and leaves the rest alone,
/// as the shell tests' `t_store_answers` does.
#[cfg(test)]
mod run_tests {
    use super::*;
    use std::process::Command;
    use tehas_core::testdir::TestDir;
    use tehas_core::testing::{copy_exe, write_exe};

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    /// Which of the two `publish` calls a refusal stands in for.
    enum Publish {
        Views,
        Health,
        Any,
    }

    struct Store {
        home: TestDir,
        real: PathBuf,
        /// The refusals the wrapper answers, one line for each call the case
        /// stands in for. Every call the run makes is written down.
        body: Vec<String>,
        record: bool,
        /// Where the run looks for the MCP server: off unless a case turns
        /// it on.
        mcp: Mcp,
    }

    /// The store of this build: looked for beside the test binary and one
    /// directory up, and built once when neither holds it.
    ///
    /// The two directories are the profile under test, `target/<profile>/deps`
    /// and `target/<profile>`, where a run of the workspace puts the store it
    /// built with the binaries. A run of a package of its own builds only that
    /// package, so the sibling `store` is not there at all. An absent store is
    /// built, not skipped: these cases are honest about the program a host runs
    /// only when they run the real one. A cold target directory builds the
    /// store's bundled SQLite, which is the slow part of this path; the gate
    /// builds the workspace first, so it does not pay for it.
    fn store_of_this_build() -> PathBuf {
        let deps = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let dirs = [deps.to_path_buf(), deps.parent().unwrap().to_path_buf()];
        find_store(&dirs).unwrap_or_else(|| {
            build_store();
            find_store(&dirs)
                .unwrap_or_else(|| panic!("the store of this build is beside the test binary"))
        })
    }

    /// The `store` of one of these directories, if it is there.
    fn find_store(dirs: &[PathBuf]) -> Option<PathBuf> {
        dirs.iter()
            .map(|dir| dir.join("store"))
            .find(|here| here.is_file())
    }

    /// One build of the store for the whole harness, into the directory the
    /// harness's own profile uses, and its words when it did not build. A
    /// `OnceLock` rather than a `Once` so every thread of the parallel harness
    /// reads the same outcome, including the ones that lost the race and so ran
    /// no closure of their own. cargo is given `timeout(1)` as every other
    /// outside call of this file is: a cargo that blocks on the lock of a
    /// target directory it does not own must fail with its own words rather
    /// than hold the harness. The build scripts of the crates run here, so the
    /// environment keeps no variable named like a secret, the bot's token among
    /// them, as `build_ready` (install.sh) prunes it before a build.
    fn build_store() {
        static BUILT: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
        let built = BUILT.get_or_init(|| {
            let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
            // cargo names the directory when it was asked for one of its own;
            // the workspace's default is the one this manifest's target dir is.
            let target = std::env::var_os("CARGO_TARGET_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target"));
            let mut cmd = crate::store::bounded("10", "900", "cargo");
            cmd.arg("build")
                .args(["--locked", "--offline", "--quiet", "--manifest-path"])
                .arg(&manifest)
                .args(["--target-dir"])
                .arg(&target)
                .args(["-p", "store"])
                .args((!cfg!(debug_assertions)).then_some("--release"));
            for name in std::env::vars_os().map(|(k, _)| k) {
                let name = name.to_string_lossy();
                let secret = ["TOKEN", "SECRET", "PASSWORD", "API_KEY"]
                    .iter()
                    .any(|part| name.contains(part));
                if secret {
                    cmd.env_remove(name.as_ref());
                }
            }
            match cmd.output() {
                Ok(out) if out.status.success() => Ok(()),
                Ok(out) => Err(String::from_utf8_lossy(&out.stderr).trim().to_string()),
                Err(e) => Err(format!("cargo: {e}")),
            }
        });
        if let Err(words) = built {
            panic!("the store of this build could not be built: {words}");
        }
    }

    /// The store of a home: the store of this build, in a wrapper that writes
    /// down every call made to it and answers the refusals a case named before
    /// the call. Every other verb is the store's own answer, so no case here
    /// leans on the store's arithmetic.
    impl Store {
        fn new(name: &str) -> Self {
            let home = TestDir::new(&format!("health-run-{name}"));
            fs::create_dir_all(home.join("libexec")).unwrap();
            fs::create_dir_all(home.join("state")).unwrap();
            fs::create_dir_all(home.join("bin")).unwrap();
            // The store of this build, taken from the command line of whoever
            // built the tests: the store's own cases run in the same build, so
            // one of its processes knows the binary's path. The gate builds the
            // two programs out of two target directories, so neither can be
            // found beside the other.
            let real = store_of_this_build();
            fs::write(home.join(".tehas-home"), format!("{}\n", home.display())).unwrap();
            // The store itself is kept under a name of its own, because the
            // home's `libexec/store` is the wrapper every call is recorded in.
            copy_exe(&real, home.join("store.real")).unwrap();
            let mut s = Store {
                home,
                real,
                body: Vec::new(),
                record: true,
                mcp: Mcp {
                    on: String::new(),
                    addr: String::new(),
                    port: 0,
                },
            };
            s.install();
            // init and the copy of the day are the store's own bookkeeping,
            // not calls of the run, so they are made before the record begins.
            s.record = false;
            s.install();
            let (rc, said) = merged(
                Command::new(&s.real)
                    .args([
                        "init",
                        "--version",
                        "v0.0.0",
                        "--commit",
                        COMMIT,
                        "--instance",
                        "instance",
                        "--repo",
                        "acme/widget",
                    ])
                    .env("TEHAS_HOME", s.home.path())
                    .stdin(Stdio::null()),
            );
            assert_eq!(rc, 0, "the store's init said: {said}");
            // A host that passes has a copy of the store from today, so the
            // report of these cases is not held back by a missing one.
            let (rc, said) = merged(
                Command::new(s.home.join("store.real"))
                    .args(["backup"])
                    .env("TEHAS_HOME", s.home.path())
                    .stdin(Stdio::null()),
            );
            assert_eq!(rc, 0, "the store made no backup: {said}");
            s.record = true;
            s.install();
            s
        }

        /// The wrapper written as the case left it.
        fn install(&self) {
            let record = if self.record {
                format!(
                    "printf '%s\n' \"$*\" >>'{}'",
                    self.home.join("calls").display()
                )
            } else {
                String::new()
            };
            // The refusals are chained: one `if` and the rest `elif`, so the
            // first whose test the call answers is the store's refusal of it.
            // Each refusal is a block of its own, so one that did not answer
            // leaves the next test intact: the `if` and its `fi` are the two
            // halves of the one line the case wrote.
            let body = self
                .body
                .iter()
                .map(|line| format!("{line}\n"))
                .collect::<String>();
            let wrapper = format!(
                "#!/usr/bin/env bash\n{record}\n{body}\nexec '{real}' \"$@\"\n",
                real = self.home.join("store.real").display(),
            );
            write_exe(self.home.join("libexec/store"), &wrapper).unwrap();
        }

        /// The refusals the wrapper answers, one for each call the case stands
        /// in for: <test> says which call, <rc> and <line> are the store's own
        /// refusal of it.
        fn refuses_of(&mut self, verb: &str, only: Publish, rc: u8, line: &str) {
            // The two publishes are told apart by the argument only the
            // report's has.
            let test = match (&only, verb) {
                (Publish::Views, "publish") => {
                    "[ \"${1:-}\" = publish ] && [ \"${2:-}\" != --health ]"
                }
                (Publish::Health, "publish") => "[ \"${2:-}\" = --health ]",
                (_, verb) => return self.refuse(&format!("[ \"${{1:-}}\" = {verb} ]"), rc, line),
            };
            self.refuse(test, rc, line)
        }

        fn refuse(&mut self, test: &str, rc: u8, line: &str) {
            let said = line.replace('\'', "'\\''");
            self.body.push(format!(
                "if {test}; then printf '%s\n' '{said}' >&2; exit {rc}; fi"
            ));
            self.install()
        }

        /// The store wrapped so one verb refuses with `rc` and `line` while
        /// every other call goes to the store itself.
        fn refuses(&mut self, verb: &str, rc: u8, line: &str) {
            self.refuses_of(verb, Publish::Any, rc, line)
        }

        /// The publish of the views and the publish of the report are one verb
        /// of the store, so a case names which of the two it stands in for.
        fn refuses_publish(&mut self, which: Publish, rc: u8, line: &str) {
            self.refuses_of("publish", which, rc, line)
        }

        /// The home's store put back as install.sh left it: the case that
        /// removes the binary or the database has nothing of the wrapper left
        /// in its way.
        fn plain(&mut self) {
            self.record = false;
            self.install()
        }

        /// How many calls began with <verb>, which is how a case counts the
        /// tries of one call without naming the report's temporary.
        fn asks(&self, verb: &str) -> usize {
            self.calls().iter().filter(|c| c.starts_with(verb)).count()
        }

        fn calls(&self) -> Vec<String> {
            fs::read_to_string(self.home.join("calls"))
                .unwrap_or_default()
                .lines()
                .map(String::from)
                .collect()
        }

        /// A fresh queue snapshot and nothing else: the home a run of this
        /// build passes with.
        fn queue(&self) {
            fs::create_dir_all(self.home.join("live")).unwrap();
            fs::write(
                self.home.join("live/status.json"),
                format!(
                    "{{\"generatedAt\":\"{}\",\"queued\":[1,2],\"running\":[],\"awaiting\":[],\"stopped\":[]}}",
                    tehas_core::time::iso(time::now() - 5)
                ),
            )
            .unwrap();
        }

        fn health(&self) -> Value {
            let text = fs::read_to_string(self.home.join("live/health.json")).unwrap();
            serde_json::from_str(&text).unwrap()
        }

        fn written(&self) -> bool {
            self.home.join("live/health.json").exists()
        }

        /// The units read of a host whose systemd answers <text>. The case of
        /// a systemd that does not answer is the report's own.
        fn units(&mut self, text: &str) {
            // The empty list is systemd answering that nothing failed: no
            // line, which is what --no-legend prints for no unit.
            let said = if text.is_empty() {
                ":".to_string()
            } else {
                format!("printf '%s\n' '{text}'")
            };
            write_exe(self.systemctl(), format!("#!/usr/bin/env bash\n{said}\n")).unwrap();
        }

        /// The stub systemd of this home, named by the run rather than read
        /// off the environment.
        fn systemctl(&self) -> PathBuf {
            self.home.join("bin").join("systemctl")
        }

        fn run_with(&mut self, budget: &str) -> u8 {
            self.units("");
            let program = self.systemctl();
            run_with(&self.home, budget.to_string(), &program, &self.mcp)
        }

        /// The whole of `run` for a home whose settings named <budget> as its
        /// budget, given to the console as the wrapper gives it: one argument,
        /// and no name of the host in the console.
        fn run_budget(&mut self, budget: &str) -> u8 {
            self.units("");
            run(&[
                self.home.to_string_lossy().into_owned(),
                "--budget".to_string(),
                budget.to_string(),
            ])
        }

        /// The run of a home whose systemd does not answer: no stub is left
        /// where PATH could find one.
        fn run_without_systemd(&mut self) -> u8 {
            let _ = fs::remove_file(self.systemctl());
            run_with(
                &self.home,
                POLL_BUDGET.to_string(),
                &self.systemctl(),
                &self.mcp,
            )
        }

        fn run(&mut self) -> u8 {
            self.run_with("cpu=5,wall=30,writes=0")
        }
    }

    #[test]
    fn a_run_asks_the_store_its_verbs_in_order_and_publishes_the_report() {
        let mut s = Store::new("run");
        s.queue();
        assert_eq!(s.run(), 0);
        assert_eq!(
            s.calls(),
            [
                "plan-show --open",
                "publish",
                "backup --show",
                "poll-idle --since 3600 --budget cpu=5,wall=30,writes=0",
                format!(
                    "publish --health {}/tmp/health.{}",
                    s.home.display(),
                    std::process::id()
                )
                .as_str(),
            ]
        );
        let h = s.health();
        assert_eq!(h["status"], "pass");
        assert_eq!(h["output"], "Idle, taking work.");
        assert_eq!(h["checks"]["queue"]["inLine"], 2);
        // The copy the store's own init's day made, read with no jq.
        assert!(h["checks"]["backup"]["ageSeconds"].as_f64().unwrap() < 60.0);
        // The units read answered an empty list, which is a check that ran and
        // found nothing, not the check of a systemd that did not answer.
        assert_eq!(h["checks"]["units"], Value::Array(Vec::new()));
        // Nothing was over budget, which is the answer of a read that ran: the
        // entry is there and its list is empty.
        assert_eq!(h["checks"]["poll_cost"]["over"], Value::Array(Vec::new()));
        // The report the store took is gone, as the shell's trap took its
        // mktemp.
        assert_eq!(fs::read_dir(s.home.join("tmp")).unwrap().count(), 0);
    }

    #[test]
    fn no_store_is_seventy_eight_before_the_first_call() {
        let mut s = Store::new("no-store");
        s.queue();
        s.plain();
        fs::remove_file(s.home.join("libexec/store")).unwrap();
        assert_eq!(s.run(), 78);
        assert!(!s.written());
        assert!(s.calls().is_empty());
    }

    #[test]
    fn a_store_that_is_no_database_is_seventy_eight_too() {
        let mut s = Store::new("no-db");
        s.queue();
        s.plain();
        fs::remove_file(s.home.join("state/tehas.db")).unwrap();
        assert_eq!(s.run(), 78);
        assert!(!s.written());
    }

    #[test]
    fn sessions_the_store_would_not_name_stop_the_run() {
        for (rc, word) in [(1, "busy"), (78, "corrupt")] {
            let mut s = Store::new("plan-show");
            s.queue();
            s.refuses("plan-show", rc, &format!("store: {word}: the read is held"));
            assert_eq!(s.run(), rc);
            assert!(!s.written());
            assert_eq!(s.calls(), ["plan-show --open"]);
        }
    }

    #[test]
    fn a_publish_only_busy_leaves_the_views_a_tick_old_and_writes_anyway() {
        for (rc, want) in [(1, 0), (78, 78)] {
            let mut s = Store::new("publish");
            s.queue();
            s.refuses_publish(Publish::Views, rc, "store: busy #0: the read is held");
            assert_eq!(s.run(), want);
            assert_eq!(s.written(), want == 0);
        }
    }

    #[test]
    fn a_backup_or_a_poll_read_that_refused_is_no_check_and_no_warning() {
        let mut s = Store::new("refused");
        s.queue();
        s.refuses("backup", 1, "store: busy: the lock is held");
        s.refuses("poll-idle", 1, "store: busy: the lock is held");
        assert_eq!(s.run(), 0);
        let h = s.health();
        assert_eq!(h["output"], "Idle, taking work.");
        assert_eq!(h["checks"]["backup"], Value::Null);
        assert_eq!(h["checks"]["poll_cost"], Value::Null);
    }

    #[test]
    fn the_budget_of_the_instance_is_the_one_the_store_is_asked_for() {
        let mut s = Store::new("budget");
        s.queue();
        assert_eq!(s.run_with("cpu=10,wall=10"), 0);
        assert!(s
            .calls()
            .iter()
            .any(|c| c == "poll-idle --since 3600 --budget cpu=10,wall=10"));
        assert_eq!(
            s.health()["checks"]["poll_cost"]["budget"],
            "cpu=10,wall=10"
        );
    }

    #[test]
    fn a_store_busy_for_the_report_is_tried_three_times_1_s_apart() {
        let mut s = Store::new("busy-health");
        s.queue();
        s.refuses_publish(
            Publish::Health,
            1,
            "store: busy #0: state/publish.lock is held",
        );
        assert_eq!(s.run(), 1);
        assert_eq!(s.asks("publish --health"), 3);
        assert!(!s.written());
        assert_eq!(fs::read_dir(s.home.join("tmp")).unwrap().count(), 0);
    }

    #[test]
    fn a_store_that_cannot_be_used_is_tried_once() {
        let mut s = Store::new("unusable-health");
        s.queue();
        s.refuses_publish(
            Publish::Health,
            78,
            "store: corrupt: the file is no database",
        );
        assert_eq!(s.run(), 78);
        // One try of a store that cannot be used, and the report's own file is
        // put away by the run that made it.
        assert_eq!(s.asks("publish --health"), 1);
        assert!(!s.written());
        assert_eq!(fs::read_dir(s.home.join("tmp")).unwrap().count(), 0);
    }

    #[test]
    fn a_run_takes_the_home_and_nothing_else() {
        assert_eq!(run(&[]), 2);
        assert_eq!(run(&["a".into(), "b".into()]), 2);
    }

    #[test]
    fn a_systemd_that_does_not_answer_leaves_the_units_out_and_says_so() {
        let mut s = Store::new("no-systemd");
        s.queue();
        assert_eq!(s.run_without_systemd(), 0);
        // The read did not answer, which is not the same as none having failed:
        // the entry is left out and the home still passes.
        assert_eq!(s.health()["checks"]["units"], Value::Null);
        assert_eq!(s.health()["status"], "pass");
    }

    #[test]
    fn a_stopped_mcp_server_warns_and_the_file_is_still_written() {
        let mut s = Store::new("mcp-stopped");
        s.queue();
        let bin = s.home.join("libexec/tehas-mcp");
        fs::write(&bin, "").unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        // A port nothing listens on: the server is stopped.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        s.mcp = Mcp {
            on: "1".into(),
            addr: "127.0.0.1".into(),
            port,
        };
        assert_eq!(s.run(), 0);
        let h = s.health();
        let mcp = &h["checks"]["mcp"];
        assert_eq!(mcp["on"], true);
        assert_eq!(mcp["installed"], true);
        assert_eq!(mcp["answers"], false);
        let why = mcp["reason"].as_str().unwrap();
        assert!(
            why.starts_with(&format!("POST /mcp on 127.0.0.1:{port}: ")),
            "{why}"
        );
        assert_eq!(h["status"], "warn");
        assert_eq!(
            h["output"],
            format!("The MCP server does not answer: {why}.")
        );
    }

    #[test]
    fn an_mcp_server_that_is_off_is_said_and_passes() {
        let mut s = Store::new("mcp-off");
        s.queue();
        assert_eq!(s.run(), 0);
        assert_eq!(
            s.health()["checks"]["mcp"],
            serde_json::json!({"on": false})
        );
        assert_eq!(s.health()["status"], "pass");
    }

    #[test]
    fn a_budget_the_store_refuses_leaves_the_check_out_and_says_so() {
        // The load of lib/config.sh is what stops the host with 78 on a
        // malformed budget; a run given one reaches the store, whose refusal
        // is what leaves poll_cost out - never a check that passed.
        let mut s = Store::new("budget-shape");
        s.queue();
        assert_eq!(s.run_budget("cpu=05,wall=30,writes=0"), 0);
        assert_eq!(s.health()["checks"]["poll_cost"], Value::Null);
        assert!(s
            .calls()
            .iter()
            .any(|c| c == "poll-idle --since 3600 --budget cpu=05,wall=30,writes=0"));
    }

    #[test]
    fn a_store_that_cannot_be_used_for_a_check_is_said_and_left_out() {
        let mut s = Store::new("unusable-check");
        s.queue();
        s.refuses("backup", 78, "store: corrupt: the file is no database");
        assert_eq!(s.run(), 0);
        // The check is still left out, but an unusable store is not a check
        // that went without: the line says which of them it was.
        assert_eq!(s.health()["checks"]["backup"], Value::Null);
    }
}

pub fn main(given: &[String]) -> u8 {
    if given.first().is_some_and(|v| v == "run") {
        return run(&given[1..]);
    }
    let usage = || {
        eprintln!(
            "console health: usage: run <home> | report <home> [--sessions <json>] [--units <text>] [--backup <json>] [--poll-over <json> --poll-budget <spec>] [--mcp <json>]"
        );
        2
    };
    let (home, rest) = match given {
        [verb, home, rest @ ..] if verb == "report" => (Path::new(home), rest),
        _ => return usage(),
    };
    let mut g = Given::default();
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let Some(v) = it.next().cloned() else {
            return usage();
        };
        match flag.as_str() {
            "--sessions" => g.sessions = Some(v),
            "--units" => g.units = Some(v),
            "--backup" => g.backup = Some(v),
            "--poll-over" => g.poll_over = Some(v),
            "--poll-budget" => g.poll_budget = Some(v),
            "--mcp" => g.mcp = Some(v),
            _ => return usage(),
        }
    }
    tehas_core::outln!("{}", report(home, &g, time::now()).pretty());
    0
}

fn read(path: &Path) -> Option<J> {
    J::parse(&fs::read_to_string(path).ok()?)
}

/// A file that is there, not empty and no bigger than a poll may write, with
/// the text it holds, as one JSON value. A file over the bound is said of and
/// left out rather than parsed: the cost of one is quadratic in its fields.
/// The text comes back with the value because a reader that needs to know what
/// the file wrote - a key written twice, say - cannot learn it from the value.
fn poll_file(path: &Path) -> (J, String) {
    match fs::metadata(path) {
        Ok(m) if m.len() == 0 => (J::Null, String::new()),
        Ok(m) if m.len() > POLL_FILE_BOUND => {
            eprintln!(
                "WARN: the poll file {} is {} bytes, over the {} a poll may write: it is not read",
                path.file_name().unwrap_or_default().to_string_lossy(),
                m.len(),
                POLL_FILE_BOUND
            );
            (J::Null, String::new())
        }
        _ => match fs::read_to_string(path) {
            Ok(text) => (J::parse(&text).unwrap_or(J::Null), text),
            Err(_) => (J::Null, String::new()),
        },
    }
}

/// A file that is there and not empty, as one JSON value, or null.
fn snapshot(path: &Path) -> J {
    poll_file(path).0
}

fn age_of(t: &J, now: i64) -> Option<i64> {
    t.as_str().and_then(epoch).map(|e| now - e)
}

/// `todate` of a number, null for anything else.
fn iso(v: &J) -> J {
    match v.as_f64() {
        Some(n) => J::Str(time::iso(n.floor() as i64)),
        None => J::Null,
    }
}

/// An age in seconds as the output line says it.
fn ago(v: &J) -> String {
    let n = v.as_f64().unwrap_or(0.0);
    if n < 120.0 {
        format!("{} s", v.text())
    } else if n < 7200.0 {
        format!("{} min", J::num((n / 60.0).floor()).text())
    } else {
        format!("{} h", J::num((n / 3600.0).floor()).text())
    }
}

fn get_fields(v: &J, keys: &[&str]) -> Vec<(String, J)> {
    keys.iter()
        .map(|k| (k.to_string(), v.get(k).clone()))
        .collect()
}

/// The free space of the host as tehasbot preview measured it, with the time
/// of the measure.
fn disk(live: &Path, now: i64) -> J {
    let p = read(&live.join("previews.json")).unwrap_or(J::Null);
    let fresh = age_of(p.get("generatedAt"), now).is_some_and(|a| a < DISK_MAX_AGE);
    match p.get("disk") {
        J::Obj(d) if fresh => {
            let mut d = d.clone();
            set(&mut d, "time", p.get("generatedAt").clone());
            J::Obj(d)
        }
        _ => J::Null,
    }
}

/// `. + {key: value}`: a key already there keeps its place.
fn set(o: &mut Vec<(String, J)>, key: &str, v: J) {
    match o.iter_mut().find(|(k, _)| k == key) {
        Some(f) => f.1 = v,
        None => o.push((key.to_string(), v)),
    }
}

/// The open issues labelled `epic` with no sub-issues, as tehasbot status last
/// read them (live/epics.json), with the time of that read. Null when the read
/// is too old or the file is not one it wrote: a read that failed leaves the
/// file as it was, so an old file says nothing about the issues now.
fn epics(live: &Path, now: i64) -> J {
    let e = read(&live.join("epics.json")).unwrap_or(J::Null);
    let fresh = age_of(e.get("generatedAt"), now).is_some_and(|a| a < EPICS_MAX_AGE);
    match e.get("unlinked") {
        J::Arr(a) if fresh && a.iter().all(|n| matches!(n, J::Num(_))) => obj(vec![
            ("time", e.get("generatedAt").clone()),
            ("unlinked", J::Arr(a.clone())),
        ]),
        _ => J::Null,
    }
}

/// The platform checkout against origin, as tehasbot status last read it.
fn platform(live: &Path) -> J {
    match read(&live.join("platform.json")) {
        Some(p @ J::Obj(_)) if matches!(p.get("behind"), J::Num(_)) => p,
        _ => J::Null,
    }
}

/// The slice as the last poll left it, when that poll is young enough. The
/// keys are tehas-core's: the dispatcher writes the file through `hold::Report`,
/// so a key named here is a key that type declares.
fn slice(live: &Path, now: i64) -> J {
    let (d, text) = poll_file(&live.join("dispatch.json"));
    let young = age_of(d.get(hold::KEY_TIME), now).is_some_and(|a| a < POLL_MAX_AGE);
    if !matches!(d, J::Obj(_)) || d.get(hold::KEY_WHY).as_str().is_none() || !young {
        return J::Null;
    }
    let mut f = get_fields(
        &d,
        &[
            hold::KEY_TIME,
            hold::KEY_WHY,
            hold::KEY_HOLD,
            hold::KEY_COUNT,
            hold::KEY_ROOM,
            hold::KEY_COMMITTED,
            hold::KEY_MAX,
            hold::KEY_RESERVE,
        ],
    );
    for k in [
        hold::KEY_FREE,
        hold::KEY_AWAIT_ROOM,
        hold::KEY_LIMIT_ROOM,
        hold::KEY_REFRESH_ROOM,
    ] {
        if !d.get(k).is_null() {
            f.push((k.to_string(), d.get(k).clone()));
        }
    }
    // The counts a poll derived, when it derived any. `status --json` copies
    // this object straight into `room.slots`, and api/status.schema.json closes
    // that object, so a value of another shape - a hand-written file, or one
    // an older release left - would be answered as a schema violation. Such a
    // value is left out, as a poll that wrote none is, and said of.
    let slots = d.get(hold::KEY_SLOTS);
    if !slots.is_null() {
        if slots_shape(slots, Some(&text)) {
            f.push((hold::KEY_SLOTS.to_string(), slots.clone()));
        } else {
            eprintln!(
                "WARN: the poll's slots object is not the shape the api schema closes: the slots line is left out"
            );
        }
    }
    let held = held_of(d.get(hold::KEY_HELD));
    if !held.is_empty() {
        f.push((hold::KEY_HELD.to_string(), J::Arr(held)));
    }
    J::Obj(f)
}

/// The ready issues the poll held one by one, each as `hold::Held` writes it.
/// An entry of another shape is left out: `status --json` copies the list and
/// api/status.schema.json closes its objects.
fn held_of(list: &J) -> Vec<J> {
    list.items()
        .iter()
        .filter(|h| {
            h.get("issue")
                .as_f64()
                .is_some_and(|n| n >= 1.0 && n.fract() == 0.0)
                && ["why", "text", "clear"]
                    .iter()
                    .all(|k| h.get(k).as_str().is_some())
        })
        .map(|h| {
            obj(["issue", "why", "text", "clear"]
                .into_iter()
                .map(|k| (k, h.get(k).clone()))
                .collect())
        })
        .collect()
}

/// The line of a factory with room whose every ready issue the poll held:
/// None when the queue lists none, or one the poll did not hold.
fn all_held(slice: &J, queue: &J) -> Option<String> {
    let held = slice.get(hold::KEY_HELD).items();
    let queued: Vec<String> = queue
        .get("queued")
        .items()
        .iter()
        .map(|i| i.get("number").text())
        .collect();
    let is_held = |n: &String| held.iter().any(|h| &h.get("issue").text() == n);
    if queued.is_empty() || !queued.iter().all(is_held) {
        return None;
    }
    if let [one] = queued.as_slice() {
        let h = held.iter().find(|h| &h.get("issue").text() == one)?;
        let clear = h.get("clear").text_or_empty();
        let mut c = clear.chars();
        let clear = match c.next() {
            Some(f) => f.to_uppercase().chain(c).collect(),
            None => String::new(),
        };
        return Some(format!(
            "#{one} is held: {}. {clear}.",
            h.get("text").text_or_empty()
        ));
    }
    let nums: Vec<String> = queued.iter().map(|n| format!("#{n}")).collect();
    Some(format!(
        "Every ready issue is held: {}. tehas status says why.",
        nums.join(", ")
    ))
}

/// The four keys api/status.schema.json requires of the slots object.
const SLOT_REQUIRED: [&str; 4] = ["containers", "containersHow", "cycles", "cyclesHow"];
/// The two derivations a poll writes only for a setting that said `auto`. They
/// are optional, so `slots_shape` adds them to the required four when the poll
/// wrote them; only the names matter, not their order.
const SLOT_DERIVED: [&str; 2] = ["cycle", "container"];
/// The seven keys one derivation holds. `only` compares the names as a set, so
/// the order here is for reading.
const AUTO_KEYS: [&str; 7] = [
    "bound",
    "ceiling",
    "containerMemory",
    "cpus",
    "cycleCpus",
    "floored",
    "slots",
];
/// The words a setting may ask for.
const HOW_WORDS: [&str; 3] = ["auto", "fixed", "number"];
/// The words for what gave a derived count its bound. The list is closed here
/// as it is in api/status.schema.json, so a release that reads the host another
/// way adds its word to both, not to one of them.
const BOUND_WORDS: [&str; 4] = ["ceiling", "cpu", "host", "memory"];

/// Whether <v> is the slots object api/status.schema.json promises: the four
/// keys, their counts and their words, `cycle` and `container` when the poll
/// derived them, and nothing else. The schema is the contract this enforces;
/// the manual's note that a derivation goes with an `auto` is the producer's
/// own rule (`Slots::report`), not one the schema states or this gate checks.
fn slots_shape(v: &J, text: Option<&str>) -> bool {
    // The whole file is asked once whether it wrote any key twice: a value the
    // parser resolved is no witness, and a file whose writer repeated a key is
    // not the file the dispatcher wrote.
    if text.is_some_and(written_once) {
        return false;
    }

    /// The derivation of one side: the seven keys the schema names, and each
    /// of them its own type. A side that the setting gave no reading for is
    /// not there at all, which the caller allows.
    fn derivation(v: &J) -> bool {
        only(v, &AUTO_KEYS)
            && in_list(v.get("bound"), &BOUND_WORDS)
            && count_of_slots(v.get("slots"))
            && (v.get("cpus").is_null() || count_of_slots(v.get("cpus")))
            && v.get("cycleCpus").as_str().is_some()
            && v.get("containerMemory").as_str().is_some()
            && v.get("ceiling").as_str().is_some()
            && matches!(v.get("floored"), J::Bool(_))
    }

    // The four keys the schema requires, plus each side the poll derived.
    let mut want: Vec<&str> = SLOT_REQUIRED.to_vec();
    for k in SLOT_DERIVED {
        if !v.get(k).is_null() {
            want.push(k);
        }
    }
    only(v, &want)
        && count_of_slots(v.get("cycles"))
        && count_of_slots(v.get("containers"))
        && in_list(v.get("cyclesHow"), &HOW_WORDS)
        && in_list(v.get("containersHow"), &HOW_WORDS)
        && (v.get("cycle").is_null() || derivation(v.get("cycle")))
        && (v.get("container").is_null() || derivation(v.get("container")))
}

/// The demand as the last poll left it, when that poll is young enough.
fn demand(live: &Path, now: i64) -> J {
    let d = snapshot(&live.join("demand.json"));
    let young = age_of(d.get("time"), now).is_some_and(|a| a < POLL_MAX_AGE);
    if !matches!(d, J::Obj(_)) || !matches!(d.get("waiting"), J::Num(_)) || !young {
        return J::Null;
    }
    let mut f = get_fields(&d, &["time", "waiting"]);
    if let J::Str(_) = d.get("oldest") {
        f.push(("oldest".to_string(), d.get("oldest").clone()));
    }
    if let Some(h) = d.get("refreshHold").as_str() {
        f.push(("refreshHold".to_string(), J::Str(first(h, 300))));
    }
    if let Some(alone) = label_alone(d.get("labelAlone")) {
        f.push(("labelAlone".to_string(), alone));
    }
    J::Obj(f)
}

/// The most issue numbers `labelAlone` carries, as the dispatcher bounds it.
const LABEL_ALONE_MAX: usize = 50;

/// The issues `tehas:blocked` holds by the label alone, as the poll listed
/// them: a list of issue numbers, at most `LABEL_ALONE_MAX` of them. A list
/// that holds anything but issue numbers is left out, and so is an empty one.
fn label_alone(v: &J) -> Option<J> {
    let J::Arr(a) = v else { return None };
    let ok = a
        .iter()
        .all(|n| matches!(n, J::Num(t) if t.parse::<u64>().is_ok_and(|n| n > 0)));
    if !ok || a.is_empty() {
        return None;
    }
    Some(J::Arr(a.iter().take(LABEL_ALONE_MAX).cloned().collect()))
}

/// Whether some process holds the lock, or it cannot be told apart from one
/// that is: core's probe answers, and anything but no lock and a lock it took
/// is a turn in flight. `flock -n` fails on a lock it cannot open as it does on
/// a lock another process holds, and a name that is no lock file says nothing
/// about the turn, which is the worse thing to read as an ended one.
fn held(lock: &Path) -> bool {
    matches!(
        tehas_core::lock::probe(lock),
        tehas_core::lock::Probe::Held | tehas_core::lock::Probe::Unknown(_)
    )
}

/// The open planning sessions to tell of, each {issue, turn, startedAt,
/// state}: `running` for a turn in flight, the session the store has running
/// whose turn holds its lock and has not left its exit status
/// (console/dispatch/src/apply/turn.rs); `failed` for one whose last turn
/// ended in error and waits for a person: on record as failed (the dispatcher
/// posted its question), or still running on record beside an exit status
/// other than 0 that the next poll will post. A session that is over is not in
/// `plan-show --open`, and one whose turn finished or was cut off is left out.
fn plans(home: &Path, sessions: &J) -> J {
    let state = home.join("state");
    let mut out = Vec::new();
    for s in sessions.get("sessions").items() {
        if !matches!(s.get("issue"), J::Num(_)) || !s.get("ended_at").is_null() {
            continue;
        }
        let n = s.get("issue").text();
        let plain = n.len() <= 10
            && n.bytes().all(|b| b.is_ascii_digit())
            && !n.starts_with('0')
            && !n.is_empty();
        if !plain {
            continue;
        }
        let turns = match s.get("turns") {
            t @ J::Num(_) => t.as_f64().unwrap_or(0.0),
            _ => 0.0,
        };
        let lock = state.join(format!("plan-{n}.json.lock"));
        let rc = state.join(format!("plan-{n}.json.rc"));
        let (turn, word) = if s.get("running").is_true() {
            match exit_status(&rc) {
                Some(t) if t.trim_end_matches('\n') == "0" => continue,
                Some(_) => (turns + 1.0, "failed"),
                None if rc.exists() || !held(&lock) => continue,
                None => (turns + 1.0, "running"),
            }
        } else if s.get("failed").is_true() && !s.get("lost").is_true() {
            (turns, "failed")
        } else {
            continue;
        };
        out.push(obj(vec![
            ("issue", s.get("issue").clone()),
            ("turn", J::num(turn)),
            ("startedAt", iso(s.get("started_at"))),
            ("state", J::str(word)),
        ]));
    }
    J::Arr(out)
}

/// The exit status a planning turn left, as the dispatcher reads it: the
/// first bytes of a regular file, None for no file. A status that cannot be
/// read is not 0, which is a turn that ended in error.
fn exit_status(rc: &Path) -> Option<String> {
    if !rc.is_file() {
        return None;
    }
    let mut text = Vec::new();
    let _ = fs::File::open(rc)
        .and_then(|f| io::Read::read_to_end(&mut io::Read::take(f, 64), &mut text));
    Some(String::from_utf8_lossy(&text).into_owned())
}

/// The pause flag of this host: null with none, {at, reason} with one, and
/// both null for a file that is no flag, which holds the factory all the same.
fn pause_flag(home: &Path) -> J {
    let r = flags::read_pause(&home.join("state/pause.json"));
    let field = |k: &str| r.json_of(k).and_then(J::parse);
    match r.state() {
        "none" => J::Null,
        "ok" => obj(vec![
            ("at", field("at").unwrap_or(J::Null)),
            ("reason", field("reason").unwrap_or(J::str(""))),
        ]),
        _ => obj(vec![("at", J::Null), ("reason", J::Null)]),
    }
}

/// The failed tehas units, from what systemctl printed. No `--units` is null:
/// systemd did not answer, which is not the same as none having failed.
fn units(text: &str) -> J {
    J::Arr(
        text.split('\n')
            .map(|l| l.split(' ').next().unwrap_or(""))
            .filter(|u| u.starts_with("tehas-"))
            .map(J::str)
            .collect(),
    )
}

fn in_list(v: &J, words: &[&str]) -> bool {
    v.as_str().is_some_and(|s| words.contains(&s))
}

/// The shape of a count in the poll's slots object: the whole numbers a poll
/// can write, and nothing else. The producer bounds every count it writes at
/// `MAX` = 16 (`console/dispatch/src/slots.rs`), so a count far above what a
/// host can hold is a hand-written or corrupt file: text parsed as `u64` is
/// what the schema's `integer` means, and it also refuses the spellings a
/// float would round (`1e16`, `9007199254740993`, `-0`, `6.0`).
fn count_of_slots(v: &J) -> bool {
    match v {
        J::Num(text) => text.parse::<u64>().is_ok(),
        _ => false,
    }
}

/// The whole fields of <v>, in the order they were read. `None` for a value
/// that is not an object, so a text or a number is never mistaken for an
/// object with no keys.
fn fields(v: &J) -> Option<Vec<&str>> {
    match v {
        J::Obj(o) => Some(o.iter().map(|(k, _)| k.as_str()).collect()),
        _ => None,
    }
}

/// The keys of <v> are exactly the words of <want>: a key that is not named is
/// refused and one that is named and missing is refused too, as
/// api/status.schema.json closes the object. A key written twice is refused by
/// the caller, which asks `written_once` of the file's bytes: the parser would
/// have resolved it to its last value and left no trace of it in <v>.
fn only(v: &J, want: &[&str]) -> bool {
    let mut want = want.to_vec();
    want.sort_unstable();
    match fields(v) {
        Some(mut have) => {
            have.sort_unstable();
            have == want
        }
        None => false,
    }
}

/// Whether the poll's file wrote the same key twice, in the object the bytes
/// start with or in any object nested in it. `J::parse` is jq's reader: a key
/// written twice keeps its place and its last value, so the value cannot say a
/// key was written twice and the closed-object check would pass a file that
/// held the same key twice with two values. The bytes are the only witness, and
/// the parse kept them. They are walked in the same shape: a string written
/// before a `:` names a key of the object then open, and any other string is a
/// value and names nothing. A name repeated in two different objects is no
/// repeat - `slots` is a key of the poll's object and of a derivation - so each
/// object's own names are compared on their own.
fn written_once(text: &str) -> bool {
    let b = text.as_bytes();
    let mut open: Vec<Vec<&[u8]>> = Vec::new();
    let mut i = 0usize;
    while i < b.len() {
        match b[i] {
            b'{' | b'[' => open.push(Vec::new()),
            b'}' | b']' => {
                open.pop();
            }
            b'"' => {
                let from = i;
                let mut j = i + 1;
                let mut escaped = false;
                while j < b.len() && b[j] != b'"' {
                    escaped = !escaped && b[j] == b'\\';
                    j += 1;
                }
                if j >= b.len() {
                    return true; // no JSON: refused, as the caller refused it
                }
                let mut k = j + 1;
                while k < b.len() && b[k].is_ascii_whitespace() {
                    k += 1;
                }
                if k < b.len() && b[k] == b':' {
                    if let Some(keys) = open.last_mut() {
                        let key = &b[from..=j];
                        if keys.contains(&key) {
                            return true;
                        }
                        keys.push(key);
                    }
                }
                i = j;
            }
            _ => {}
        }
        i += 1;
    }
    false
}

/// Whether an ending's status is an end that went well: its work shipped, it
/// waits for review, or its issue closed while it ran (rc 2), which usually
/// means the pull request merged first or a person closed the issue by hand.
/// The three words are ends, so an ending that carries one has ended.
fn ended_well(c: &J) -> bool {
    in_list(
        c.get("status").or(&J::str("")),
        &["merged", "handoff", "closed"],
    )
}

/// The status word of an ending whose status names no end, as health shows it
/// in `checks.failed` and in the line it says. The words that say so are
/// `tehas_core::grammar::NOT_ENDS`, read here by `an_end` and mapped by the
/// store's writer of the ten endings (`ending_status` there), so the page and
/// the table never disagree about which words they are.
const NOT_AN_END: &str = tehas_core::grammar::NOT_AN_END;

/// Whether a row of the last ten counts for `checks.failed`: an object whose
/// status is none of the ends that went well. A row that never ended is kept by
/// that branch too, and named by the word `failed_word` gives it.
fn a_failed_ending(e: &J) -> bool {
    matches!(e, J::Obj(_)) && !ended_well(e)
}

/// The word health shows for an ending: the status it reported for one that
/// ended, and `NOT_AN_END` for one whose status says it did not. A row carries
/// no word at all when the key is absent, null, or not text, and a word of
/// `NOT_ENDS` names a cycle still at work: neither is a word an end carries, so
/// the row is named rather than copied into the published file. One rule, read
/// the same in `checks.failed` and in the line `output` says.
fn failed_word(e: &J) -> String {
    match e.get("status") {
        J::Str(w) if an_end(w) => w.clone(),
        _ => NOT_AN_END.to_string(),
    }
}

/// One cycle as the health file lists it.
fn brief(c: &J) -> J {
    obj(vec![
        ("issue", c.get("issue").clone()),
        ("running", c.get("running").or(&J::Bool(false)).clone()),
        ("status", c.get("status").clone()),
        ("step", c.get("step").clone()),
        ("startedAt", iso(c.get("startedAt"))),
        ("time", iso(c.get("updatedAt"))),
    ])
}

fn joined(v: &J) -> String {
    v.items()
        .iter()
        .map(|x| if x.is_null() { String::new() } else { x.text() })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The limit as `flags::read` reads it. A reading the page cannot parse is
/// unreadable, as it is to every other reader, never nothing.
fn limit(home: &Path, now: i64) -> J {
    J::parse(&flags::read(&home.join("state/limit.json"), now).json())
        .unwrap_or_else(|| obj(vec![("state", J::Str("unreadable".to_string()))]))
}

fn report(home: &Path, g: &Given, now: i64) -> J {
    let live = home.join("live");
    let endings = match read(&live.join("endings.json")) {
        Some(e @ J::Arr(_)) => e,
        _ => J::Arr(vec![]),
    };
    let queue = snapshot(&live.join("status.json"));
    let view = snapshot(&live.join("running.json"));
    let disk = disk(&live, now);
    let platform = platform(&live);
    let slice = slice(&live, now);
    let demand = demand(&live, now);
    let epics = epics(&live, now);
    let sessions = g.sessions.as_deref().and_then(J::parse).unwrap_or(J::Null);
    let plans = plans(home, &sessions);
    let limit = limit(home, now);

    let flag = pause_flag(home);
    let bool_or_false = |v: &J| match v {
        J::Bool(b) => *b,
        _ => false,
    };
    let label = match (queue.at(&["pause", "label"]), queue.get("paused")) {
        (J::Bool(b), _) => *b,
        (_, J::Bool(b)) => *b,
        _ => false,
    };
    let lifted = bool_or_false(queue.at(&["pause", "lifted"]));
    let pause = obj(vec![
        ("flag", flag.clone()),
        ("label", J::Bool(label)),
        ("lifted", J::Bool(lifted)),
    ]);

    // systemd did not answer: no list, and no warning raised from one.
    let units = g.units.as_deref().map(units);
    let (backup, backup_known) = match g.backup.as_deref().and_then(J::parse) {
        Some(b) => match b.get("newest") {
            J::Null => (J::Null, true),
            n => (
                obj(vec![
                    ("at", n.get("at").clone()),
                    ("ageSeconds", n.get("age_seconds").clone()),
                ]),
                true,
            ),
        },
        None => (J::Null, false),
    };
    // No probe was made: no entry, and no warning raised from one.
    let mcp = g.mcp.as_deref().and_then(J::parse).unwrap_or(J::Null);
    let mcp_silent = mcp.get("on").is_true() && !mcp.get("answers").is_true();
    // The cleaner's record of its last upkeep, which is what makes the backup.
    let upkeep = match read(&home.join("state/upkeep.json")) {
        Some(u) if matches!(u.get("ok"), J::Bool(_)) => obj(vec![
            ("time", iso(u.get("at"))),
            ("ok", u.get("ok").clone()),
            ("word", u.get("word").clone()),
        ]),
        _ => J::Null,
    };
    let upkeep_failed = matches!(upkeep.get("ok"), J::Bool(false));
    let poll_cost = match (g.poll_over.as_deref().and_then(J::parse), &g.poll_budget) {
        (Some(over), Some(budget)) => obj(vec![
            ("over", over.or(&J::Arr(vec![])).clone()),
            ("budget", J::str(budget.clone())),
        ]),
        _ => J::Null,
    };

    // The view tehas work writes, or the one object an earlier version wrote.
    let (running, last) = match &view {
        J::Obj(o) if o.iter().any(|(k, _)| k == "cycles") => (
            view.get("cycles").or(&J::Arr(vec![])).clone(),
            view.get("last").clone(),
        ),
        J::Obj(_) if view.get("running").is_true() => (J::Arr(vec![view.clone()]), J::Null),
        J::Obj(_) => (J::Arr(vec![]), view.clone()),
        _ => (J::Arr(vec![]), J::Null),
    };
    let zero = J::num(0.0);
    let failed = !last.is_null()
        && !ended_well(&last)
        && !running.items().iter().any(|c| {
            // A cycle of no known start is running now.
            c.get("startedAt").is_null()
                || c.get("startedAt")
                    .cmp(last.get("updatedAt").or(&zero))
                    .is_ge()
        });
    let behind = !platform.is_null()
        && platform.get("behind").above_zero()
        && now
            - platform
                .get("behindSince")
                .as_str()
                .and_then(epoch)
                .unwrap_or(0)
            > BEHIND_FOR;
    let queue_age = (!queue.is_null()).then(|| {
        now - queue
            .get("generatedAt")
            .as_str()
            .and_then(epoch)
            .unwrap_or(0)
    });
    let no_start = !slice.is_null()
        && in_list(
            slice.get(hold::KEY_WHY),
            &[
                hold::Why::Slice.as_str(),
                hold::Why::Disk.as_str(),
                hold::Why::Unreadable.as_str(),
            ],
        );
    let no_resources = !slice.is_null()
        && slice.get(hold::KEY_WHY).as_str() == Some(hold::Why::Resources.as_str());
    let old_queue = queue_age.is_some_and(|a| a > QUEUE_MAX_AGE);
    let paused = !flag.is_null() || label || lifted;
    let limit_holds = in_list(limit.get("state"), &["expired", "unreadable"]);
    let over_budget = !poll_cost.is_null() && !poll_cost.get("over").is_empty();
    let old_backup = backup_known
        && (backup.is_null()
            || backup
                .get("ageSeconds")
                .cmp(&J::num(BACKUP_MAX_AGE))
                .is_gt());
    let disk_low = !disk.is_null() && disk.get("low").truthy();
    let has_units = units.as_ref().is_some_and(|u| !u.is_empty());
    let unlinked = epics.get("unlinked").items();
    let held_line = if running.is_empty() && !old_queue {
        all_held(&slice, &queue)
    } else {
        None
    };

    let status = if queue.is_null() || failed {
        "fail"
    } else if old_queue
        || paused
        || limit_holds
        || no_start
        || no_resources
        || has_units
        || disk_low
        || behind
        || old_backup
        || over_budget
        || !unlinked.is_empty()
        || mcp_silent
        || held_line.is_some()
    {
        "warn"
    } else {
        "pass"
    };

    let output = if queue.is_null() {
        "No queue snapshot yet: the dispatcher has not written one.".to_string()
    } else if failed {
        format!(
            "The last cycle failed: #{} ended as {}.",
            last.get("issue").text(),
            // The word of an ending that never ended is named as it is in
            // `checks.failed`, by the one helper that answers both.
            failed_word(&last)
        )
    } else if old_queue {
        format!(
            "The queue snapshot is {} old: GitHub did not answer the last polls.",
            ago(&J::num(queue_age.unwrap_or(0) as f64))
        )
    } else if paused {
        "The factory is paused.".to_string()
    } else if limit_holds {
        if limit.get("state").as_str() == Some("expired") {
            format!(
                "Nothing starts: the limit expired at {}. Set it again or clear it (tehas limit).",
                limit.get("until").text()
            )
        } else {
            "Nothing starts: the limit cannot be read. Set it again or clear it (tehas limit)."
                .to_string()
        }
    } else if no_start || no_resources {
        let hold = slice.get(hold::KEY_HOLD);
        if no_resources {
            let h = hold.text_or_empty();
            let h = match h.strip_prefix("holding: no resources: ") {
                Some(rest) => format!("No resources: {rest}"),
                None => h,
            };
            format!("{h}.")
        } else {
            hold.or(&J::str("holding")).text()
        }
    } else if has_units {
        format!(
            "systemd gave up on {}: run systemctl --user reset-failed and start it again.",
            joined(units.as_ref().unwrap_or(&J::Arr(vec![])))
        )
    } else if disk_low {
        if disk.get("freeBytes").is_null() {
            "The host's free disk space could not be read: previews are not being built."
                .to_string()
        } else {
            "The host is low on disk: previews are not being built.".to_string()
        }
    } else if behind {
        format!(
            "The platform is {} commits behind {} since {}: run tehas update.",
            platform.get("behind").text(),
            platform.get("upstream").text(),
            first(&platform.get("behindSince").text_or_empty(), 10)
        )
    } else if old_backup {
        let why = if upkeep_failed {
            format!(
                "the last upkeep failed at {}: {}",
                upkeep.get("time").text(),
                upkeep.get("word").or(&J::str("no word")).text()
            )
        } else if backup.is_null() {
            "tehasbot clean makes one a day".to_string()
        } else {
            String::new()
        };
        match (backup.is_null(), why.is_empty()) {
            (true, _) => format!("There is no backup of the state store: {why}."),
            (false, true) => format!(
                "The newest backup of the state store is {} old.",
                ago(backup.get("ageSeconds"))
            ),
            (false, false) => format!(
                "The newest backup of the state store is {} old: {why}.",
                ago(backup.get("ageSeconds"))
            ),
        }
    } else if over_budget {
        format!(
            "An idle poll is over its budget on {} (TEHAS_POLL_BUDGET={}).",
            joined(poll_cost.get("over")),
            poll_cost.get("budget").text()
        )
    } else if mcp_silent {
        format!(
            "The MCP server does not answer: {}.",
            mcp.get("reason").text()
        )
    } else if !unlinked.is_empty() {
        let mut named: Vec<String> = unlinked
            .iter()
            .take(UNLINKED_SAID)
            .map(|n| format!("#{}", n.text()))
            .collect();
        if unlinked.len() > UNLINKED_SAID {
            named.push(format!("{} more", unlinked.len() - UNLINKED_SAID));
        }
        format!(
            "Labelled epic with no sub-issues: {}. Link their parts to them as sub-issues.",
            named.join(", ")
        )
    } else if !running.is_empty() {
        let issues: Vec<String> = running
            .items()
            .iter()
            .map(|c| format!("#{}", c.get("issue").text()))
            .collect();
        format!("Working on {}.", issues.join(", "))
    } else if let Some(line) = held_line {
        line
    } else {
        "Idle, taking work.".to_string()
    };

    let count = |k: &str| J::num(queue.get(k).len() as f64);
    let queue_check = if queue.is_null() {
        J::Null
    } else {
        obj(vec![
            ("time", queue.get("generatedAt").clone()),
            ("paused", J::Bool(paused)),
            ("inLine", count("queued")),
            ("building", count("running")),
            ("review", count("awaiting")),
            ("stopped", count("stopped")),
        ])
    };
    let current = running.items().first().unwrap_or(&NULL).or(&last);
    let cycle = if current.is_null() {
        J::Null
    } else {
        brief(current)
    };
    let cycles = J::Arr(running.items().iter().map(brief).collect());

    // The failed endings of the last ten and the last cycle, newest first, one
    // for each cycle.
    let mut ends: Vec<J> = endings.items().to_vec();
    if !last.is_null() {
        ends.push(last.clone());
    }
    let mut failed: Vec<J> = ends.into_iter().filter(a_failed_ending).collect();
    let key = |e: &J| J::Arr(vec![e.get("issue").clone(), e.get("startedAt").clone()]);
    failed.sort_by(|a, b| key(a).cmp(&key(b)));
    failed.dedup_by(|b, a| key(a).cmp(&key(b)).is_eq());
    failed.sort_by(|a, b| a.get("updatedAt").cmp(b.get("updatedAt")));
    failed.reverse();
    let failed_list = J::Arr(
        failed
            .iter()
            .map(|e| {
                obj(vec![
                    ("issue", e.get("issue").clone()),
                    ("status", J::str(failed_word(e))),
                    ("step", e.get("step").clone()),
                    ("startedAt", iso(e.get("startedAt"))),
                    ("time", iso(e.get("updatedAt"))),
                ])
            })
            .collect(),
    );
    let disk_check = if disk.is_null() {
        J::Null
    } else {
        J::Obj(get_fields(
            &disk,
            &["time", "freeBytes", "minFreeBytes", "low"],
        ))
    };
    let all_hands = if queue.is_null() || old_queue {
        J::Null
    } else {
        queue.get("allHands").clone()
    };
    let backup_check = if backup.is_null() {
        J::Null
    } else {
        obj(vec![
            ("time", iso(backup.get("at"))),
            ("ageSeconds", backup.get("ageSeconds").clone()),
        ])
    };

    obj(vec![
        ("status", J::str(status)),
        ("time", J::Str(time::iso(now))),
        ("output", J::Str(output)),
        (
            "checks",
            obj(vec![
                ("queue", queue_check),
                ("cycle", cycle),
                ("cycles", cycles),
                ("plans", plans),
                ("failed", failed_list),
                ("disk", disk_check),
                ("slice", slice),
                ("demand", demand),
                ("limit", limit),
                ("allHands", all_hands),
                ("pause", pause),
                ("platform", platform),
                ("units", units.unwrap_or(J::Null)),
                ("backup", backup_check),
                ("upkeep", upkeep),
                ("poll_cost", poll_cost),
                ("epics", epics),
                ("mcp", mcp),
            ]),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tehas_core::testdir::TestDir;

    const NOW: i64 = 1_792_245_620; // 2026-10-17T14:00:20Z

    struct Home(TestDir);

    /// A home with a fresh queue snapshot and nothing else.
    fn home(name: &str) -> Home {
        let dir = TestDir::new(&format!("health-{name}"));
        fs::create_dir_all(dir.join("live")).unwrap();
        fs::create_dir_all(dir.join("state")).unwrap();
        let h = Home(dir);
        h.write(
            "live/status.json",
            r#"{"generatedAt":"2026-10-17T14:00:10Z","queued":[1,2],"running":[3],"awaiting":[],"stopped":[],"allHands":false}"#,
        );
        h
    }

    impl Home {
        fn write(&self, path: &str, text: &str) {
            fs::write(self.0.join(path), text).unwrap();
        }

        fn report(&self, g: &Given) -> J {
            report(&self.0, g, NOW)
        }
    }

    fn said(r: &J) -> (String, String) {
        (r.get("status").text(), r.get("output").text())
    }

    #[test]
    fn every_reader_of_the_limit_gives_one_answer() {
        let h = home("limit-readers");
        let unreadable = r#"{"state":"unreadable"}"#;
        let cases = [
            (r#"{"v":1,"cycles":1,"plan":true,"cycles":2}"#, unreadable),
            (r#"{"v":1,"cycles":1,"plan":false,"plan":true}"#, unreadable),
            (r#"{"v":1,"cycles":1,"plan":true,"v":1}"#, unreadable),
            (r#"{"v":1,"cycles":1e999,"plan":true}"#, unreadable),
            (r#"{"v":1,"cycles":1,"plan":true"#, unreadable),
            (
                r#"{"v":1,"cycles":2,"plan":false,"until":"2026-10-17T13:00:00Z","reason":"x"}"#,
                r#"{"state":"expired","cycles":2,"plan":false,"until":"2026-10-17T13:00:00Z","reason":"x"}"#,
            ),
            (
                r#"{"v":1,"cycles":3,"plan":true,"until":null,"reason":" held\nfor alpha "}"#,
                r#"{"state":"ok","cycles":3,"plan":true,"reason":"held for alpha"}"#,
            ),
            (
                r#"{"v":1,"cycles":1,"plan":true,"note":{"cycles":9}}"#,
                r#"{"state":"ok","cycles":1,"plan":true}"#,
            ),
            (
                r#"{"v":1.0,"cycles":01,"plan":true}"#,
                r#"{"state":"ok","cycles":1.0,"plan":true}"#,
            ),
        ];
        let value = |text: &str| serde_json::from_str::<Value>(text).unwrap();
        for (file, want) in cases {
            h.write("state/limit.json", file);
            let want = value(want);
            let console = value(&flags::read(&h.0.join("state/limit.json"), NOW).json());
            let page = value(&h.report(&Given::default()).at(&["checks", "limit"]).dump());
            let dispatch = tehas_dispatch::read::host::limit(&h.0, NOW);
            assert_eq!(console, want, "console: {file}");
            assert_eq!(page, want, "health: {file}");
            assert_eq!(dispatch, want, "dispatcher: {file}");
        }
    }

    #[test]
    fn a_fresh_queue_and_nothing_else_is_idle() {
        let h = home("idle");
        let r = h.report(&Given::default());
        assert_eq!(said(&r), ("pass".into(), "Idle, taking work.".into()));
        assert_eq!(
            r.get("checks").get("queue").dump(),
            r#"{"time":"2026-10-17T14:00:10Z","paused":false,"inLine":2,"building":1,"review":0,"stopped":0}"#
        );
        assert_eq!(r.at(&["checks", "allHands"]).dump(), "false");
        assert_eq!(r.at(&["checks", "limit"]).dump(), r#"{"state":"none"}"#);
    }

    #[test]
    fn an_issue_labelled_epic_with_no_sub_issues_warns_while_the_read_is_young() {
        let h = home("epics");
        let epics = |at: &str, unlinked: &str| {
            format!(
                r#"{{"generatedAt":"{at}","repo":"acme/widget","epics":[],"unlinked":{unlinked}}}"#
            )
        };
        h.write("live/epics.json", &epics("2026-10-17T14:00:00Z", "[12,4]"));
        let r = h.report(&Given::default());
        assert_eq!(
            said(&r),
            (
                "warn".into(),
                "Labelled epic with no sub-issues: #12, #4. Link their parts to them as sub-issues."
                    .into()
            )
        );
        assert_eq!(
            r.at(&["checks", "epics"]).dump(),
            r#"{"time":"2026-10-17T14:00:00Z","unlinked":[12,4]}"#
        );
        h.write(
            "live/epics.json",
            &epics("2026-10-17T14:00:00Z", "[9,8,7,6,5,4,3]"),
        );
        assert_eq!(
            h.report(&Given::default()).get("output").text(),
            "Labelled epic with no sub-issues: #9, #8, #7, #6, #5, 2 more. Link their parts to them as sub-issues."
        );
        // None unlinked, a read too old to believe, a file of another shape and
        // no file at all: no warning, and the check is there only for the first.
        h.write("live/epics.json", &epics("2026-10-17T14:00:00Z", "[]"));
        let r = h.report(&Given::default());
        assert_eq!(said(&r), ("pass".into(), "Idle, taking work.".into()));
        assert_eq!(r.at(&["checks", "epics", "unlinked"]).dump(), "[]");
        for file in [
            epics("2026-10-17T13:40:00Z", "[12]"),
            epics("2026-10-17T14:00:00Z", r#"["12"]"#),
            r#"{"epics":"before"}"#.to_string(),
        ] {
            h.write("live/epics.json", &file);
            let r = h.report(&Given::default());
            assert_eq!(
                said(&r),
                ("pass".into(), "Idle, taking work.".into()),
                "{file}"
            );
            assert_eq!(r.at(&["checks", "epics"]), &J::Null, "{file}");
        }
        fs::remove_file(h.0.join("live/epics.json")).unwrap();
        assert_eq!(h.report(&Given::default()).get("status").text(), "pass");
    }

    #[test]
    fn no_queue_snapshot_fails() {
        let h = home("noqueue");
        fs::remove_file(h.0.join("live/status.json")).unwrap();
        let r = h.report(&Given::default());
        assert_eq!(r.get("status").text(), "fail");
        assert_eq!(r.at(&["checks", "queue"]), &J::Null);
    }

    #[test]
    fn a_last_cycle_that_failed_fails_until_one_starts_after_it() {
        let h = home("failed");
        h.write(
            "live/running.json",
            r#"{"cycles":[],"last":{"issue":7,"running":false,"status":"aborted","step":"develop","startedAt":100,"updatedAt":200}}"#,
        );
        let r = h.report(&Given::default());
        assert_eq!(
            said(&r),
            (
                "fail".into(),
                "The last cycle failed: #7 ended as aborted.".into()
            )
        );
        assert_eq!(r.at(&["checks", "failed"]).len(), 1);
        h.write(
            "live/running.json",
            r#"{"cycles":[{"issue":8,"running":true,"startedAt":300}],"last":{"issue":7,"status":"aborted","startedAt":100,"updatedAt":200}}"#,
        );
        let r = h.report(&Given::default());
        assert_eq!(said(&r), ("pass".into(), "Working on #8.".into()));
        assert_eq!(
            r.at(&["checks", "cycle", "startedAt"]).text(),
            "1970-01-01T00:05:00Z"
        );
    }

    #[test]
    fn a_last_cycle_that_ended_closed_is_not_a_failure() {
        let h = home("closed");
        h.write(
            "live/running.json",
            r#"{"cycles":[],"last":{"issue":1106,"running":false,"status":"closed","step":"review","startedAt":100,"updatedAt":200}}"#,
        );
        let r = h.report(&Given::default());
        assert_eq!(said(&r), ("pass".into(), "Idle, taking work.".into()));
        assert_eq!(r.at(&["checks", "failed"]).len(), 0);
    }

    #[test]
    fn a_running_cycle_of_no_known_start_clears_the_failure() {
        let h = home("nostart");
        let last = r#""last":{"issue":12,"status":"aborted","startedAt":100,"updatedAt":500}"#;
        for (cycle, failed) in [
            (r#"{"issue":14,"running":true,"startedAt":200}"#, true),
            (r#"{"issue":14,"running":true,"startedAt":600}"#, false),
            (r#"{"issue":14,"running":true,"updatedAt":700}"#, false),
            (r#"{"issue":14,"running":true,"startedAt":null}"#, false),
        ] {
            h.write(
                "live/running.json",
                &format!(r#"{{"cycles":[{cycle}],{last}}}"#),
            );
            let r = h.report(&Given::default());
            assert_eq!(r.get("status").text() == "fail", failed, "{cycle}");
        }
    }

    #[test]
    fn an_old_queue_snapshot_warns() {
        let h = home("old");
        h.write(
            "live/status.json",
            r#"{"generatedAt":"2026-10-17T13:40:20Z","allHands":true}"#,
        );
        let r = h.report(&Given::default());
        assert_eq!(
            said(&r),
            (
                "warn".into(),
                "The queue snapshot is 20 min old: GitHub did not answer the last polls.".into()
            )
        );
        assert_eq!(r.at(&["checks", "allHands"]), &J::Null);
    }

    #[test]
    fn a_pause_flag_or_label_warns() {
        let h = home("pause");
        h.write(
            "state/pause.json",
            r#"{"by":"someone","at":"2026-10-17T13:00:00Z","reason":"weekend"}"#,
        );
        let r = h.report(&Given::default());
        assert_eq!(said(&r), ("warn".into(), "The factory is paused.".into()));
        assert_eq!(
            r.at(&["checks", "pause"]).dump(),
            r#"{"flag":{"at":"2026-10-17T13:00:00Z","reason":"weekend"},"label":false,"lifted":false}"#
        );
        h.write("state/pause.json", "not a flag");
        let r = h.report(&Given::default());
        assert_eq!(
            r.at(&["checks", "pause", "flag"]).dump(),
            r#"{"at":null,"reason":null}"#
        );
        fs::remove_file(h.0.join("state/pause.json")).unwrap();
        h.write(
            "live/status.json",
            r#"{"generatedAt":"2026-10-17T14:00:10Z","pause":{"label":false,"lifted":true}}"#,
        );
        assert_eq!(h.report(&Given::default()).get("status").text(), "warn");
    }

    #[test]
    fn an_expired_limit_warns() {
        let h = home("limit");
        h.write(
            "state/limit.json",
            r#"{"v":1,"cycles":2,"plan":true,"until":"2026-10-17T13:00:00Z"}"#,
        );
        let r = h.report(&Given::default());
        assert_eq!(
            said(&r),
            ("warn".into(), "Nothing starts: the limit expired at 2026-10-17T13:00:00Z. Set it again or clear it (tehas limit).".into())
        );
    }

    #[test]
    fn a_slice_that_holds_every_start_warns() {
        let h = home("slice");
        h.write(
            "live/dispatch.json",
            r#"{"time":"2026-10-17T14:00:00Z","why":"resources","hold":"holding: no resources: memory","room":0,"free":0}"#,
        );
        let r = h.report(&Given::default());
        assert_eq!(said(&r), ("warn".into(), "No resources: memory.".into()));
        assert_eq!(
            r.at(&["checks", "slice"]).dump(),
            r#"{"time":"2026-10-17T14:00:00Z","why":"resources","hold":"holding: no resources: memory","count":null,"room":0,"committed":null,"max":null,"reserve":null,"free":0}"#
        );
        h.write(
            "live/dispatch.json",
            r#"{"time":"2026-10-17T14:00:00Z","why":"count","hold":"holding: two running"}"#,
        );
        assert_eq!(said(&h.report(&Given::default())).0, "pass");
        h.write(
            "live/dispatch.json",
            r#"{"time":"2026-10-17T13:00:00Z","why":"disk"}"#,
        );
        assert_eq!(
            h.report(&Given::default()).at(&["checks", "slice"]),
            &J::Null
        );
    }

    #[test]
    fn the_slots_a_poll_derived_are_kept_and_a_number_writes_none() {
        let h = home("slots");
        h.write(
            "live/dispatch.json",
            r#"{"time":"2026-10-17T14:00:00Z","why":"","room":3,"free":2,
                 "slots":{"cycles":6,"containers":6,"cyclesHow":"auto","containersHow":"auto",
                          "cycle":{"bound":"cpu","slots":6,"cpus":24,"cycleCpus":"4","containerMemory":"8g","ceiling":"16","floored":false}}}"#,
        );
        assert_eq!(
            h.report(&Given::default())
                .at(&["checks", "slice", "slots", "cycle", "bound"])
                .as_str(),
            Some("cpu")
        );
        // A poll that sets numbers writes no slots object, and the check keeps
        // the fields it always had: no key at all, not one that is null.
        h.write(
            "live/dispatch.json",
            r#"{"time":"2026-10-17T14:00:00Z","why":"","room":1,"free":1}"#,
        );
        let r = h.report(&Given::default());
        assert!(r.at(&["checks", "slice"]).get("slots").is_null());
        assert_eq!(
            r.at(&["checks", "slice"]).dump(),
            r#"{"time":"2026-10-17T14:00:00Z","why":"","hold":null,"count":null,"room":1,"committed":null,"max":null,"reserve":null,"free":1}"#
        );
    }

    /// The poll's slots object with one key of `cycle` added, so a case says
    /// which shape it refused.
    fn cycle_with(home: &Home, key: &str, value: &str) {
        home.write(
            "live/dispatch.json",
            &format!(
                r#"{{"time":"2026-10-17T14:00:00Z","why":"","room":3,"free":2,
                     "slots":{{"cycles":6,"containers":6,"cyclesHow":"auto","containersHow":"auto",
                              "cycle":{{"bound":"cpu","slots":6,"cpus":24,"cycleCpus":"4","containerMemory":"8g","ceiling":"16","floored":false,"{key}":{value}}}}}}}"#,
            ),
        );
    }

    /// The poll's slots object with one of its own keys replaced by <value>.
    fn slots_with(home: &Home, key: &str, value: &str) {
        home.write(
            "live/dispatch.json",
            &format!(
                r#"{{"time":"2026-10-17T14:00:00Z","why":"","room":3,"free":2,
                     "slots":{{"cycles":6,"containers":6,"cyclesHow":"auto","containersHow":"auto","{key}":{value}}}}}"#,
            ),
        );
    }

    #[test]
    fn a_slots_value_of_another_shape_is_left_out() {
        let h = home("slots-shape");
        // The shape the schema promises is copied; a value that is not it is
        // left out as a poll that wrote none is, and said of with a WARN line.
        // The whole check is compared, so a refusal cannot leave one key of
        // the refused object behind.
        let check = |h: &Home| h.report(&Given::default()).at(&["checks", "slice"]).dump();
        // A file whose slots are the shape, and the same file with none: the
        // two dumps the cases below are told from.
        h.write(
            "live/dispatch.json",
            r#"{"time":"2026-10-17T14:00:00Z","why":"","room":3,"free":2,
                 "slots":{"cycles":6,"containers":6,"cyclesHow":"auto","containersHow":"auto"}}"#,
        );
        let kept = r#"{"time":"2026-10-17T14:00:00Z","why":"","hold":null,"count":null,"room":3,"committed":null,"max":null,"reserve":null,"free":2,"slots":{"cycles":6,"containers":6,"cyclesHow":"auto","containersHow":"auto"}}"#;
        let without = r#"{"time":"2026-10-17T14:00:00Z","why":"","hold":null,"count":null,"room":3,"committed":null,"max":null,"reserve":null,"free":2}"#;
        assert_eq!(check(&h), kept);

        // The four keys are each required, at their own type, and a key of a
        // derivation at the wrong type or one of them too many is refused too.
        for (key, value) in [
            // a text instead of a count
            ("cycles", "\"6\""),
            ("containers", "\"6\""),
            // a count below 0, one with a fraction, one with a sign, and one
            // past what the parser holds exactly: the producer bounds every
            // count at 16, so none of these is a poll's own bytes
            ("cycles", "-1"),
            ("containers", "1.5"),
            ("cycles", "1e16"),
            ("containers", "-0"),
            // a word the schema does not know
            ("cyclesHow", "\"maybe\""),
            ("containersHow", "null"),
            // a key the schema does not allow
            ("why", "\"cpu\""),
        ] {
            slots_with(&h, key, value);
            assert_eq!(check(&h), without, "slots with {key}: {value} was copied");
        }

        // A key written twice is refused: the parser would resolve it to its
        // last value and publish that as though the file had written one.
        h.write(
            "live/dispatch.json",
            r#"{"time":"2026-10-17T14:00:00Z","why":"","room":3,"free":2,
                 "slots":{"cycles":6,"containers":6,"cyclesHow":"auto","containersHow":"auto","cycles":7}}"#,
        );
        assert_eq!(check(&h), without, "a repeated key was copied");
        // Of a derivation too, where the outer object has no key to spare.
        h.write(
            "live/dispatch.json",
            r#"{"time":"2026-10-17T14:00:00Z","why":"","room":3,"free":2,
                 "slots":{"cycles":6,"containers":6,"cyclesHow":"auto","containersHow":"auto",
                          "cycle":{"bound":"cpu","slots":6,"cpus":24,"cycleCpus":"4","containerMemory":"8g","ceiling":"16","floored":false,"floored":true}}}"#,
        );
        assert_eq!(
            check(&h),
            without,
            "a repeated key of a derivation was copied"
        );

        // A derivation of its own, and the keys it holds are not asked for
        // until it is there: the shape of the rest of the object is kept.
        for (key, value) in [
            ("bound", "\"disk\""),
            ("slots", "\"six\""),
            ("cpus", "\"24\""),
            ("floored", "\"false\""),
            ("why", "\"cpu\""),
            ("cycle", "1"),
        ] {
            cycle_with(&h, key, value);
            assert_eq!(check(&h), without, "cycle with {key}: {value} was copied");
        }
    }

    #[test]
    fn the_slots_shape_refuses_a_value_that_is_not_the_schema() {
        // The shape the dispatcher's own writer emits is accepted: both sides,
        // neither side, and a null count for a host that gave no reading.
        for text in [
            r#"{"cycles":6,"containers":6,"cyclesHow":"auto","containersHow":"auto",
                "cycle":{"bound":"cpu","slots":6,"cpus":24,"cycleCpus":"4","containerMemory":"8g","ceiling":"16","floored":false},
                "container":{"bound":"memory","slots":2,"cpus":null,"cycleCpus":"4","containerMemory":"8g","ceiling":"16","floored":true}}"#,
            r#"{"cycles":6,"containers":6,"cyclesHow":"auto","containersHow":"auto",
                "cycle":{"bound":"cpu","slots":6,"cpus":24,"cycleCpus":"4","containerMemory":"8g","ceiling":"16","floored":false}}"#,
            r#"{"cycles":1,"containers":2,"cyclesHow":"number","containersHow":"fixed"}"#,
        ] {
            assert!(slots_shape(&J::parse(text).unwrap(), Some(text)), "{text}");
        }
        // A text, a count, nothing, an object with a key the schema does not
        // name, one that is missing a key it requires, a count written in a
        // spelling the schema's `integer` does not hold exactly, and a key
        // written twice - at the top and inside a derivation, where the file
        // would otherwise be published with whichever value came last.
        for text in [
            r#""auto""#,
            r#"6"#,
            r#"null"#,
            r#"{"cycles":1,"containers":2,"cyclesHow":"number","containersHow":"fixed","extra":1}"#,
            r#"{"cycles":1,"containers":2,"cyclesHow":"number"}"#,
            r#"{"cycles":1e16,"containers":2,"cyclesHow":"number","containersHow":"fixed"}"#,
            r#"{"cycles":1,"containers":2,"cyclesHow":"number","containersHow":"fixed","cycles":2}"#,
            r#"{"cycles":1,"containers":2,"cyclesHow":"number","containersHow":"fixed",
                "cycle":{"bound":"cpu","slots":6,"cpus":24,"cycleCpus":"4","containerMemory":"8g","ceiling":"16","floored":false,"floored":true}}"#,
        ] {
            assert!(!slots_shape(&J::parse(text).unwrap(), Some(text)), "{text}");
        }
    }

    #[test]
    fn the_shape_of_the_polls_own_slots_is_the_one_the_gate_takes() {
        // The gate's contract is the object the dispatcher writes, so the
        // producer is the witness. `slots_of` is this file's own copy of the
        // seven keys of one derivation and the four plus the two of the
        // object; the bytes beside it are what `Slots::report` lays down for
        // the same two sides (console/dispatch/src/slots.rs, and the case
        // `the_slots_line_of_a_poll_that_derived_names_both_sides` of status.rs
        // prints them). When the two spellings come apart, this fails here
        // rather than on a host whose `slots` line has gone quiet.
        let bytes = |both: bool| {
            let mut m = serde_json::Map::new();
            m.insert("cycles".into(), serde_json::json!(6));
            m.insert("containers".into(), serde_json::json!(6));
            m.insert("cyclesHow".into(), serde_json::json!("auto"));
            m.insert("containersHow".into(), serde_json::json!("auto"));
            let side = |bound: &str, cpus: serde_json::Value, floored: bool| {
                serde_json::json!({
                    "bound": bound, "slots": 6, "cpus": cpus, "cycleCpus": "4",
                    "containerMemory": "8g", "ceiling": "16", "floored": floored
                })
            };
            m.insert("cycle".into(), side("cpu", serde_json::json!(24), false));
            if both {
                m.insert(
                    "container".into(),
                    side("memory", serde_json::Value::Null, true),
                );
            }
            serde_json::Value::Object(m).to_string()
        };
        let written = |both: bool| {
            let d =
                "\"cycles\":6,\"containers\":6,\"cyclesHow\":\"auto\",\"containersHow\":\"auto\"";
            let one = |k: &str, bound: &str, cpus: &str, floored: &str| {
                format!(
                    ",\"{k}\":{{\"bound\":\"{bound}\",\"slots\":6,\"cpus\":{cpus},\"cycleCpus\":\"4\",\"containerMemory\":\"8g\",\"ceiling\":\"16\",\"floored\":{floored}}}"
                )
            };
            let mut f = one("cycle", "cpu", "24", "false");
            if both {
                f.push_str(&one("container", "memory", "null", "true"));
            }
            format!("{{{d}{f}}}")
        };
        for both in [true, false] {
            let (producer, own) = (bytes(both), written(both));
            // serde_json sorts the keys of a map and `J` keeps what it was
            // given, so the two are read as values and compared by their own
            // structure; the field order of a writer is its own business.
            let value = |t: &str| serde_json::from_str::<serde_json::Value>(t).unwrap();
            assert_eq!(
                value(&producer),
                value(&own),
                "the writer's bytes and this file's copy of them"
            );
            for text in [producer, own] {
                assert!(
                    slots_shape(&J::parse(&text).unwrap(), Some(&text)),
                    "the writer's own bytes: {text}"
                );
            }
        }
        // A key the writer does not lay down, under another name, is refused:
        // the page keeps the rest of the check and prints no slots line.
        let hand = r#"{"cycles":6,"containers":6,"cyclesHow":"auto","containersHow":"auto","cpu":{"bound":"cpu","slots":6,"cpus":24,"cycleCpus":"4","containerMemory":"8g","ceiling":"16","floored":false}}"#
            .to_string();
        assert!(!slots_shape(&J::parse(&hand).unwrap(), Some(&hand)));
    }

    #[test]
    fn a_poll_file_over_the_bound_is_said_of_and_read_as_nothing() {
        // The store bounds the same files at 1 MiB when it publishes them; this
        // is the parser's other door, and the read that would otherwise spend
        // minutes on a hand-written file refuses it and says so.
        let h = home("big");
        h.write(
            "live/dispatch.json",
            &format!(
                "{{\"time\":\"2026-10-17T14:00:00Z\",\"why\":\"\",\"fill\":\"{}\"}}",
                "x".repeat(tehas_core::file::READ_BOUND_LEN)
            ),
        );
        let r = h.report(&Given::default());
        assert_eq!(r.at(&["checks", "slice"]), &J::Null);
        assert_eq!(said(&r), ("pass".into(), "Idle, taking work.".into()));
        // The demand check is read by the same function and is bounded too.
        h.write(
            "live/demand.json",
            &format!(
                "{{\"waiting\":1,\"fill\":\"{}\"}}",
                "y".repeat(tehas_core::file::READ_BOUND_LEN)
            ),
        );
        assert_eq!(
            h.report(&Given::default()).at(&["checks", "demand"]),
            &J::Null
        );
    }

    #[test]
    fn the_demand_lists_the_issues_held_by_the_label_alone_within_its_bound() {
        assert_eq!(label_alone(&J::parse("[7,9]").unwrap()), J::parse("[7,9]"));
        let many: Vec<String> = (1..=80).map(|n| n.to_string()).collect();
        let got = label_alone(&J::parse(&format!("[{}]", many.join(","))).unwrap()).unwrap();
        assert_eq!(got.items().len(), LABEL_ALONE_MAX);
        assert_eq!(got.items()[0], J::Num("1".into()));
        for bad in ["[]", "[0]", "[\"7\"]", "[7,-1]", "[1.5]", "7", "{}"] {
            assert_eq!(label_alone(&J::parse(bad).unwrap()), None, "{bad}");
        }
    }

    /// The report of a poll, at `at`, as the dispatcher's own type writes it:
    /// the bytes of `Report::to_value` with the poll's time stamped on them,
    /// which is exactly what `libexec/dispatch` publishes as live/dispatch.json.
    fn poll_report(h: &Home, at: &str, r: &hold::Report) {
        let mut v = r.to_value();
        v[hold::KEY_TIME] = serde_json::json!(at);
        h.write("live/dispatch.json", &v.to_string());
    }

    /// A poll with room that held the issues given by the park's rule.
    fn held_poll(h: &Home, issues: &[u64]) {
        let r = hold::Report {
            free: 2,
            held: issues
                .iter()
                .map(|&n| hold::Held {
                    issue: n,
                    why: hold::HELD_PARKED,
                    text: "its last cycle parked on `gate` and the body has not changed since"
                        .into(),
                    clear: "edit the body, or tick Retry in the park comment".into(),
                })
                .collect(),
            ..hold::Report::default()
        };
        poll_report(h, "2026-10-17T14:00:00Z", &r);
    }

    fn queued(h: &Home, issues: &str) {
        h.write(
            "live/status.json",
            &format!(
                r#"{{"generatedAt":"2026-10-17T14:00:10Z","queued":{issues},"running":[],"awaiting":[],"stopped":[]}}"#
            ),
        );
    }

    #[test]
    fn a_factory_whose_one_ready_issue_is_held_names_it() {
        let h = home("held-one");
        queued(&h, r#"[{"number":7}]"#);
        held_poll(&h, &[7]);
        let r = h.report(&Given::default());
        assert_eq!(
            said(&r),
            (
                "warn".into(),
                "#7 is held: its last cycle parked on `gate` and the body has not changed since. Edit the body, or tick Retry in the park comment.".into()
            )
        );
        assert_eq!(
            r.at(&["checks", "slice", "held"]).dump(),
            r#"[{"issue":7,"why":"parked","text":"its last cycle parked on `gate` and the body has not changed since","clear":"edit the body, or tick Retry in the park comment"}]"#
        );
    }

    #[test]
    fn every_ready_issue_held_is_said_and_one_free_is_taking_work() {
        let h = home("held-all");
        queued(&h, r#"[{"number":7},{"number":9}]"#);
        held_poll(&h, &[7, 9]);
        assert_eq!(
            said(&h.report(&Given::default())),
            (
                "warn".into(),
                "Every ready issue is held: #7, #9. tehas status says why.".into()
            )
        );
        held_poll(&h, &[9]);
        let r = h.report(&Given::default());
        assert_eq!(said(&r), ("pass".into(), "Idle, taking work.".into()));
        assert_eq!(r.at(&["checks", "slice", "held"]).len(), 1);
        // Nothing in line: a hold the queue no longer lists says nothing.
        queued(&h, "[]");
        assert_eq!(
            said(&h.report(&Given::default())),
            ("pass".into(), "Idle, taking work.".into())
        );
    }

    #[test]
    fn a_held_entry_of_another_shape_is_left_out() {
        let h = home("held-bad");
        queued(&h, r#"[{"number":7}]"#);
        h.write(
            "live/dispatch.json",
            r#"{"time":"2026-10-17T14:00:00Z","why":"","hold":"","held":[{"issue":"7","why":"parked","text":"x","clear":"y"},{"issue":7,"why":"parked","text":"x"}]}"#,
        );
        let r = h.report(&Given::default());
        assert!(r.at(&["checks", "slice", "held"]).is_null());
        assert_eq!(said(&r), ("pass".into(), "Idle, taking work.".into()));
    }

    #[test]
    fn the_slice_reads_the_report_the_dispatcher_writes() {
        // The one contract between the two programs: a Report the dispatcher
        // builds, taken through to_value, and read back by the page. Every
        // field of the check is there, and the derived counts are found under
        // `slots`, where the report writes them and not at the top level.
        let h = home("written");
        let r = hold::Report {
            why: Some(hold::Why::Slice),
            hold: "holding: tehas.slice has no room for another cycle".into(),
            count: 2,
            room: 0,
            committed: 21_474_836_480,
            max: 21_474_836_480,
            reserve: "2g".into(),
            free: 1,
            await_room: Some(3),
            limit_room: Some(0),
            refresh_room: Some(1),
            endpoint: Some("backup".into()),
            slots: Some(serde_json::json!({
                "cycles": 6, "containers": 6, "cyclesHow": "auto",
                "containersHow": "auto",
                "cycle": {"bound": "cpu", "slots": 6, "cpus": 24, "cycleCpus": "4",
                          "containerMemory": "8g", "ceiling": "16", "floored": false}
            })),
            held: Vec::new(),
        };
        poll_report(&h, "2026-10-17T14:00:00Z", &r);
        let got = h.report(&Given::default());
        let slice = got.at(&["checks", "slice"]);
        for (path, want) in [
            (vec![hold::KEY_TIME], "2026-10-17T14:00:00Z"),
            (vec![hold::KEY_WHY], "slice"),
            (
                vec![hold::KEY_HOLD],
                "holding: tehas.slice has no room for another cycle",
            ),
            (vec![hold::KEY_RESERVE], "2g"),
            (vec![hold::KEY_SLOTS, "cycle", "bound"], "cpu"),
        ] {
            assert_eq!(slice.at(&path).as_str(), Some(want), "{path:?}");
        }
        for (key, want) in [
            (hold::KEY_COUNT, 2.0),
            (hold::KEY_ROOM, 0.0),
            (hold::KEY_COMMITTED, 21_474_836_480.0),
            (hold::KEY_MAX, 21_474_836_480.0),
            (hold::KEY_FREE, 1.0),
            (hold::KEY_AWAIT_ROOM, 3.0),
            (hold::KEY_LIMIT_ROOM, 0.0),
            (hold::KEY_REFRESH_ROOM, 1.0),
        ] {
            assert_eq!(slice.get(key).as_f64(), Some(want), "{key}");
        }
        assert_eq!(
            slice.at(&[hold::KEY_SLOTS, "containersHow"]).as_str(),
            Some("auto")
        );
        // A reason that holds every start warns with the poll's own line.
        assert_eq!(
            said(&got),
            (
                "warn".into(),
                "holding: tehas.slice has no room for another cycle".into()
            )
        );
    }

    #[test]
    fn failed_units_low_disk_and_a_platform_behind_warn() {
        let h = home("units");
        let g = Given {
            units: Some(
                "tehas-dispatch.path loaded failed failed Dispatch\nother.service x\n".into(),
            ),
            ..Given::default()
        };
        assert_eq!(
            said(&h.report(&g)).1,
            "systemd gave up on tehas-dispatch.path: run systemctl --user reset-failed and start it again."
        );
        assert_eq!(
            h.report(&g).at(&["checks", "units"]).dump(),
            r#"["tehas-dispatch.path"]"#
        );
        // An empty text is a read that found nothing: an empty list, no warning.
        let g = Given {
            units: Some(String::new()),
            ..Given::default()
        };
        assert_eq!(h.report(&g).at(&["checks", "units"]).dump(), "[]");
        h.write("live/previews.json", r#"{"generatedAt":"2026-10-17T14:00:00Z","disk":{"freeBytes":1,"minFreeBytes":2,"low":true}}"#);
        assert_eq!(
            said(&h.report(&Given::default())).1,
            "The host is low on disk: previews are not being built."
        );
        fs::remove_file(h.0.join("live/previews.json")).unwrap();
        h.write(
            "live/platform.json",
            r#"{"commit":"abc","behind":3,"upstream":"origin/main","behindSince":"2026-10-15T00:00:00Z"}"#,
        );
        assert_eq!(
            said(&h.report(&Given::default())).1,
            "The platform is 3 commits behind origin/main since 2026-10-15: run tehas update."
        );
    }

    #[test]
    fn units_systemd_did_not_answer_is_no_check_and_no_warning() {
        let h = home("no-units");
        // No `--units` at all: the read failed, so the file says nothing of the
        // units and an otherwise healthy home still passes.
        let r = h.report(&Given::default());
        assert_eq!(r.at(&["checks", "units"]), &J::Null);
        assert_eq!(said(&r), ("pass".into(), "Idle, taking work.".into()));
    }

    #[test]
    fn an_old_or_missing_backup_and_a_poll_over_budget_warn() {
        let h = home("backup");
        let g = Given {
            backup: Some(r#"{"newest":null}"#.into()),
            ..Given::default()
        };
        assert_eq!(
            said(&h.report(&g)).1,
            "There is no backup of the state store: tehasbot clean makes one a day."
        );
        let g = Given {
            backup: Some(r#"{"newest":{"at":1792000000,"age_seconds":200000}}"#.into()),
            ..Given::default()
        };
        assert_eq!(
            said(&h.report(&g)).1,
            "The newest backup of the state store is 55 h old."
        );
        // A failed upkeep is why there is none, and the line says when and
        // what the store said rather than to wait for the cleaner.
        h.write(
            "state/upkeep.json",
            r#"{"at":1792242000,"ok":false,"word":"busy","line":"store: busy: the lock is held"}"#,
        );
        let g = Given {
            backup: Some(r#"{"newest":null}"#.into()),
            ..Given::default()
        };
        let r = h.report(&g);
        assert_eq!(
            said(&r).1,
            "There is no backup of the state store: the last upkeep failed at 2026-10-17T13:00:00Z: busy."
        );
        assert_eq!(
            r.at(&["checks", "upkeep"]).dump(),
            r#"{"time":"2026-10-17T13:00:00Z","ok":false,"word":"busy"}"#
        );
        let g = Given {
            backup: Some(r#"{"newest":{"at":1792000000,"age_seconds":200000}}"#.into()),
            ..Given::default()
        };
        assert_eq!(
            said(&h.report(&g)).1,
            "The newest backup of the state store is 55 h old: the last upkeep failed at 2026-10-17T13:00:00Z: busy."
        );
        // A fresh backup passes whatever the record says; the check still
        // carries the failure.
        let fresh = Given {
            backup: Some(r#"{"newest":{"at":1792240000,"age_seconds":5620}}"#.into()),
            ..Given::default()
        };
        let r = h.report(&fresh);
        assert_eq!(said(&r), ("pass".into(), "Idle, taking work.".into()));
        assert_eq!(r.at(&["checks", "upkeep", "word"]).as_str(), Some("busy"));
        // An upkeep that went well keeps the plain wording.
        h.write("state/upkeep.json", r#"{"at":1792242000,"ok":true}"#);
        assert_eq!(
            said(&h.report(&g)).1,
            "The newest backup of the state store is 55 h old."
        );
        fs::remove_file(h.0.join("state/upkeep.json")).unwrap();
        assert_eq!(h.report(&g).at(&["checks", "upkeep"]), &J::Null);
        let g = Given {
            poll_over: Some(r#"["cpu","wall"]"#.into()),
            poll_budget: Some("cpu=5".into()),
            ..Given::default()
        };
        assert_eq!(
            said(&h.report(&g)).1,
            "An idle poll is over its budget on cpu, wall (TEHAS_POLL_BUDGET=cpu=5)."
        );
        // A store that did not answer is no check, never a passed one.
        assert_eq!(
            h.report(&Given::default()).at(&["checks", "poll_cost"]),
            &J::Null
        );
    }

    #[test]
    fn an_mcp_server_that_is_on_and_not_answering_warns() {
        let h = home("mcp");
        let check = |l: Liveness| Given {
            mcp: Some(mcp_check(&l).dump()),
            ..Given::default()
        };
        let r = h.report(&check(Liveness::Answers));
        assert_eq!(
            r.at(&["checks", "mcp"]).dump(),
            r#"{"on":true,"installed":true,"answers":true}"#
        );
        assert_eq!(r.get("status").text(), "pass");
        let r = h.report(&check(Liveness::Off));
        assert_eq!(r.at(&["checks", "mcp"]).dump(), r#"{"on":false}"#);
        assert_eq!(r.get("status").text(), "pass");
        let r = h.report(&check(Liveness::NotInstalled));
        assert_eq!(
            r.at(&["checks", "mcp"]).dump(),
            r#"{"on":true,"installed":false,"answers":false,"reason":"libexec/tehas-mcp is not there: run install.sh"}"#
        );
        assert_eq!(
            said(&r),
            (
                "warn".into(),
                "The MCP server does not answer: libexec/tehas-mcp is not there: run install.sh."
                    .into()
            )
        );
        let r = h.report(&check(Liveness::Silent(
            "POST /mcp on 127.0.0.1:8082 answered 502".into(),
        )));
        assert_eq!(
            said(&r),
            (
                "warn".into(),
                "The MCP server does not answer: POST /mcp on 127.0.0.1:8082 answered 502.".into()
            )
        );
        // No probe was made: no entry, and no warning.
        let r = h.report(&Given::default());
        assert_eq!(r.at(&["checks", "mcp"]), &J::Null);
        assert_eq!(r.get("status").text(), "pass");
    }

    #[test]
    fn a_planning_turn_counts_while_its_lock_is_held() {
        let h = home("plans");
        h.write("state/plan-7.json.lock", "");
        let g = Given {
            sessions: Some(
                r#"{"sessions":[{"issue":7,"running":true,"turns":1,"started_at":1792245000}]}"#
                    .into(),
            ),
            ..Given::default()
        };
        assert_eq!(h.report(&g).at(&["checks", "plans"]).len(), 0);
        let lock = File::open(h.0.join("state/plan-7.json.lock")).unwrap();
        lock.lock().unwrap();
        assert_eq!(
            h.report(&g).at(&["checks", "plans"]).dump(),
            r#"[{"issue":7,"turn":2,"startedAt":"2026-10-17T13:50:00Z","state":"running"}]"#
        );
        h.write("state/plan-7.json.rc", "0");
        assert_eq!(h.report(&g).at(&["checks", "plans"]).len(), 0);
    }

    #[test]
    fn a_planning_session_whose_last_turn_failed_waits_for_a_person() {
        let h = home("plans-failed");
        let plans = |h: &Home, sessions: &str| {
            let g = Given {
                sessions: Some(sessions.into()),
                ..Given::default()
            };
            h.report(&g).at(&["checks", "plans"]).dump()
        };
        // On record as failed: the turn ended in error, the dispatcher posted
        // its question, and the session waits for an answer with no lock held.
        let failed = r#"{"sessions":[{"issue":7,"running":false,"failed":true,"turns":2,"started_at":1792245000,"ended_at":null}]}"#;
        assert_eq!(
            plans(&h, failed),
            r#"[{"issue":7,"turn":2,"startedAt":"2026-10-17T13:50:00Z","state":"failed"}]"#
        );
        // An exit status left beside a session still running on record: the
        // turn ended in error and the next poll posts it.
        let running = r#"{"sessions":[{"issue":7,"running":true,"failed":false,"turns":1,"started_at":1792245000,"ended_at":null}]}"#;
        h.write("state/plan-7.json.rc", "2\n");
        assert_eq!(
            plans(&h, running),
            r#"[{"issue":7,"turn":2,"startedAt":"2026-10-17T13:50:00Z","state":"failed"}]"#
        );
        h.write("state/plan-7.json.rc", "x");
        assert_eq!(
            plans(&h, running),
            r#"[{"issue":7,"turn":2,"startedAt":"2026-10-17T13:50:00Z","state":"failed"}]"#
        );
        // A turn that finished is no failure, and neither is one whose
        // session is over, lost, or waits on an answer to a turn that finished.
        h.write("state/plan-7.json.rc", "0\n");
        assert_eq!(plans(&h, running), "[]");
        fs::remove_file(h.0.join("state/plan-7.json.rc")).unwrap();
        for s in [
            r#"{"sessions":[{"issue":7,"running":false,"failed":true,"turns":2,"started_at":1792245000,"ended_at":1792246000}]}"#,
            r#"{"sessions":[{"issue":7,"running":false,"failed":true,"lost":true,"turns":2,"started_at":1792245000,"ended_at":null}]}"#,
            r#"{"sessions":[{"issue":7,"running":false,"failed":false,"turns":2,"started_at":1792245000,"ended_at":null}]}"#,
        ] {
            assert_eq!(plans(&h, s), "[]", "{s}");
        }
    }

    #[test]
    fn a_planning_turn_whose_lock_cannot_be_opened_is_in_flight() {
        let g = Given {
            sessions: Some(
                r#"{"sessions":[{"issue":7,"running":true,"turns":1,"started_at":1792245000}]}"#
                    .into(),
            ),
            ..Given::default()
        };
        let in_flight =
            r#"[{"issue":7,"turn":2,"startedAt":"2026-10-17T13:50:00Z","state":"running"}]"#;
        // A lock no one can open is not a turn that ended: the page says the
        // turn is in flight rather than read a probe it could not make as free.
        let h = home("plans-mode");
        h.write("state/plan-7.json.lock", "");
        fs::set_permissions(
            h.0.join("state/plan-7.json.lock"),
            fs::Permissions::from_mode(0o000),
        )
        .unwrap();
        if File::open(h.0.join("state/plan-7.json.lock")).is_ok() {
            // Root, or a mode that binds nobody: the case cannot be made here.
            fs::set_permissions(
                h.0.join("state/plan-7.json.lock"),
                fs::Permissions::from_mode(0o600),
            )
            .unwrap();
        } else {
            assert_eq!(h.report(&g).at(&["checks", "plans"]).dump(), in_flight);
            fs::set_permissions(
                h.0.join("state/plan-7.json.lock"),
                fs::Permissions::from_mode(0o600),
            )
            .unwrap();
            assert_eq!(h.report(&g).at(&["checks", "plans"]).len(), 0);
        }
        // A name at the lock path that is no lock file says nothing of the
        // turn, which is the worse thing to read as an ended one.
        let h = home("plans-shapes");
        fs::create_dir(h.0.join("state/plan-7.json.lock")).unwrap();
        assert_eq!(h.report(&g).at(&["checks", "plans"]).dump(), in_flight);
        fs::remove_dir(h.0.join("state/plan-7.json.lock")).unwrap();
        std::os::unix::fs::symlink(
            h.0.join("state/elsewhere.lock"),
            h.0.join("state/plan-7.json.lock"),
        )
        .unwrap();
        assert_eq!(h.report(&g).at(&["checks", "plans"]).dump(), in_flight);
        // No lock at all is the one shape that is not in flight, and so is the
        // exit status a turn left beside a lock that is held.
        fs::remove_file(h.0.join("state/plan-7.json.lock")).unwrap();
        assert_eq!(h.report(&g).at(&["checks", "plans"]).len(), 0);
        h.write("state/plan-7.json.lock", "");
        let lock = File::open(h.0.join("state/plan-7.json.lock")).unwrap();
        lock.lock().unwrap();
        assert_eq!(h.report(&g).at(&["checks", "plans"]).dump(), in_flight);
        h.write("state/plan-7.json.rc", "0");
        assert_eq!(h.report(&g).at(&["checks", "plans"]).len(), 0);
    }

    #[test]
    fn an_ending_that_never_ended_is_named_as_not_an_end() {
        let h = home("not-an-end");
        // Each row carries the word it must answer with, or none at all: the
        // two words of the set, and the row with no key, are `not an end`.
        h.write(
            "live/endings.json",
            r#"[{"issue":1,"status":"running","startedAt":10,"updatedAt":20},
               {"issue":2,"status":"","startedAt":10,"updatedAt":20},
               {"issue":3,"startedAt":10,"updatedAt":20},
               {"issue":4,"status":"failed","startedAt":10,"updatedAt":20},
               {"issue":5,"status":"aborted","startedAt":10,"updatedAt":20}]"#,
        );
        let got: Vec<(String, String)> = h
            .report(&Given::default())
            .at(&["checks", "failed"])
            .items()
            .iter()
            .map(|e| (e.get("issue").text(), e.get("status").text()))
            .collect();
        assert_eq!(
            got,
            [
                ("5".into(), "aborted".into()),
                ("4".into(), "failed".into()),
                // 3, 2 and 1: no word, the empty word, and `running`.
                ("3".into(), "not an end".into()),
                ("2".into(), "not an end".into()),
                ("1".into(), "not an end".into()),
            ]
        );
    }

    #[test]
    fn a_last_cycle_still_running_is_not_reported_as_ended_as_running() {
        let h = home("last-running");
        h.write(
            "live/running.json",
            r#"{"cycles":[],"last":{"issue":9,"running":false,"status":"running","step":"develop","startedAt":100,"updatedAt":200}}"#,
        );
        let r = h.report(&Given::default());
        let said = said(&r);
        let failed = r.at(&["checks", "failed"]);
        assert_eq!(failed.len(), 1);
        // The line and `checks.failed` name the row the same word, which the
        // one helper answers for both.
        assert_eq!(
            said,
            (
                "fail".into(),
                format!("The last cycle failed: #9 ended as {}.", NOT_AN_END)
            )
        );
        assert_eq!(failed.items()[0].get("status"), &J::str(NOT_AN_END));
    }

    #[test]
    fn a_status_that_is_no_word_is_named_and_never_copied_into_the_file() {
        // The published file answers with a word or with `NOT_AN_END`, never
        // with whatever bytes a hand-written `live/endings.json` left under the
        // key: edge and MCP both copy `checks.failed` out as it is.
        let h = home("status-not-a-word");
        h.write(
            "live/endings.json",
            r#"[{"issue":1,"status":{"secret":"a-very-long-value"},"startedAt":10,"updatedAt":20},
               {"issue":2,"status":null,"startedAt":10,"updatedAt":20},
               {"issue":3,"status":7,"startedAt":10,"updatedAt":20}]"#,
        );
        let got: Vec<String> = h
            .report(&Given::default())
            .at(&["checks", "failed"])
            .items()
            .iter()
            .map(|e| e.get("status").text())
            .collect();
        assert_eq!(got, [NOT_AN_END; 3]);
    }

    #[test]
    fn the_failed_list_has_each_cycle_once_newest_first() {
        let h = home("endings");
        h.write(
            "live/endings.json",
            r#"[{"issue":1,"status":"aborted","startedAt":10,"updatedAt":20},{"issue":2,"status":"merged","startedAt":5,"updatedAt":30},
               {"issue":1,"status":"aborted","startedAt":10,"updatedAt":20},{"issue":3,"status":"failed","startedAt":40,"updatedAt":50}, "x"]"#,
        );
        let issues: Vec<String> = h
            .report(&Given::default())
            .at(&["checks", "failed"])
            .items()
            .iter()
            .map(|e| e.get("issue").text())
            .collect();
        assert_eq!(issues, ["3", "1"]);
    }
}
