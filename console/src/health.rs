//! The factory's health file, live/health.json: pass, warn or fail, the one
//! line that says why, and every check under `checks`. `tehasbot health` is
//! `console health run`, so one run starts about fourteen processes and not
//! twenty-seven (libexec/health is the wrapper of that one command).
//!
//!   console health run <home>
//!     The whole run, in the order libexec/health had it: the store-ready
//!     test, `store plan-show --open`, `store publish`, `systemctl --user
//!     --failed --plain --no-legend 'tehas-*'` under a 10 s bound, `store
//!     backup --show`, `store poll-idle --since 3600 --budget <spec>`, the
//!     report, and `store publish --health <file>` three times a second apart
//!     while the store is busy or timed out. Every store call goes through
//!     store::call; the report file is written here, so the run needs no
//!     mktemp, no jq and no mkdir. The budget is TEHAS_POLL_BUDGET, the value
//!     lib/config.sh exports.
//!     Exit 0 the file was written; 1 it could not be written, or the store
//!     refused one call or did not answer; 78 the store is not installed or
//!     cannot be used, which writes nothing; 2 usage. An `ERROR:` line says
//!     which of them it was, on standard error. The checks that read the
//!     store - the units, the backup and the polls that started nothing - are
//!     left out of the file when their read did not answer, as they were.
//!
//!   console health report <home> [--sessions <json>] [--units <text>]
//!     [--backup <json>] [--poll-over <json> --poll-budget <spec>]
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
use crate::store::{self, bounded, merged, Refusal};
use serde_json::Value;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::thread::sleep;
use std::time::Duration;
use tehas_core::time;

/// A queue snapshot older than this is not news.
const QUEUE_MAX_AGE: i64 = 600;
/// The last poll's slice and demand are not believed after this.
const POLL_MAX_AGE: i64 = 600;
/// The previews' measure of the disk is not believed after this.
const DISK_MAX_AGE: i64 = 1800;
/// A platform this long behind is news.
const BEHIND_FOR: i64 = 86_400;
/// The newest backup of the store is old after this.
const BACKUP_MAX_AGE: f64 = 172_800.0;

#[derive(Default)]
struct Given {
    sessions: Option<String>,
    units: Option<String>,
    backup: Option<String>,
    poll_over: Option<String>,
    poll_budget: Option<String>,
}

/// What one run asks of the store. The backup and the idle polls are checks
/// whose refusal leaves their own entry out of the file; the three that stop
/// the run have an ERROR line and an exit status of their own, named here.
/// The publish of the report is not one of them: it is tried three times and
/// its line is `refused_publish`'s.
#[derive(Clone, Copy, PartialEq)]
enum Step {
    PlanShow,
    Publish,
    Backup,
    PollIdle,
}

/// store::call, with the two tests `tehas_store_ready` makes before the call:
/// the store binary an executable file of this home, and its database a plain
/// file and no link. Either failing is 78 `absent`, which writes nothing.
fn store_ready(home: &Path) -> bool {
    let bin = home.join("libexec").join("store");
    fs::metadata(&bin).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        && fs::symlink_metadata(home.join("state").join("tehas.db"))
            .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
}

/// The line `tehas_store_ready` and `tehas_store` answer with for a store that
/// is not there: the binary's name is what the reader looks for, so it is
/// named the way the shell named it, not the home's.
fn absent(home: &Path) -> Refusal {
    Refusal {
        rc: 78,
        word: "absent".to_string(),
        line: format!(
            "store: absent: {}/libexec/store is not an executable file",
            home.display()
        ),
    }
}

fn store_call(home: &Path, args: &[&str]) -> Result<Value, Refusal> {
    if !store_ready(home) {
        return Err(absent(home));
    }
    store::call(home, args)
}

/// `systemctl --user --failed --plain --no-legend 'tehas-*'` under the same
/// 10 s bound the shell gave it. The name is one the host's PATH answers.
fn systemctl() -> String {
    "systemctl".to_string()
}

/// None is the read that did not answer, which leaves the units out of checks
/// rather than say none failed.
fn failed_units() -> Option<String> {
    failed_units_of(&systemctl())
}

fn failed_units_of(program: &str) -> Option<String> {
    let mut cmd = bounded("2", "10", program);
    cmd.args(["--user", "--failed", "--plain", "--no-legend", "tehas-*"])
        .stdin(Stdio::null());
    // The path the unit's program is looked up on. A test names a systemd of
    // its own with it; a host has none set and PATH answers, as the shell's
    // bare `systemctl` did.
    if let Ok(dir) = std::env::var("TEHAS_SYSTEMCTL_DIR") {
        // The host's PATH is kept, so the stub's own `#!/usr/bin/env bash` is
        // found; the named directory comes first.
        cmd.env(
            "PATH",
            format!("{dir}:{}", std::env::var("PATH").unwrap_or_default()),
        );
    }
    let (rc, said) = merged(&mut cmd);
    (rc == 0).then(|| {
        let said = said.trim_end_matches('\n');
        if said.is_empty() {
            String::new()
        } else {
            said.to_string()
        }
    })
}

/// The file of the report, as the store's publish reads it: a plain file in
/// <home>/tmp, which holds nothing but its own files, made 0700 whatever the
/// umask. The name holds this process and no other: `report` above the
/// console is a sibling process, not another run of it.
fn report_file(home: &Path, text: &str) -> io::Result<PathBuf> {
    let dir = home.join("tmp");
    fs::create_dir_all(&dir)?;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    let path = dir.join(format!("health.{}", std::process::id()));
    let made = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .and_then(|mut f| f.write_all(text.as_bytes()));
    if made.is_err() {
        let _ = fs::remove_file(&path);
        return made.map(|()| path);
    }
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    Ok(path)
}

/// What a refused publish of the report says, in the voice libexec/health had.
/// The store keeps the line to its first and the shell kept it to 300 bytes.
fn refused_publish(no: &Refusal) -> u8 {
    if no.word == "absent" {
        return not_installed();
    }
    let cut = first(&no.line, 300);
    eprintln!("ERROR: the health file could not be published: {cut}");
    // The status the store's own answer carries: 1 for a store that was busy,
    // 78 for one that cannot be used. Nothing else is answered.
    if no.rc == 1 {
        1
    } else {
        78
    }
}

/// `tehas_store_try 3 publish --health <file>`: again while the store is busy
/// or its 20 s went by, 1 s apart.
fn publish_health(home: &Path, file: &Path) -> Result<(), Refusal> {
    let arg = file.display().to_string();
    let args = ["publish", "--health", arg.as_str()];
    let mut tries = 1;
    loop {
        let err = match store_call(home, &args) {
            Ok(_) => return Ok(()),
            Err(no) => no,
        };
        if tries >= 3 || err.rc != 1 || !matches!(err.word.as_str(), "busy" | "timeout") {
            return Err(err);
        }
        tries += 1;
        sleep(Duration::from_secs(1));
    }
}

/// The three reads of the store the report is built from, in the order the
/// shell asked for them: the open planning sessions, the newest backup, and
/// the figures of the idle polls over their budget - that budget left out when
/// the store refused, which is what leaves `poll_cost` out of checks rather
/// than pass it. A backup the store would not say is a null of its own, which
/// is the check that says there is no copy at all.
fn checks(home: &Path, budget: &str) -> Result<Given, u8> {
    // Each ask ends the run with the status of its refusal, which is the
    // status of the step and not of a later one.
    let ask = |step: Step, args: &[&str]| -> Result<Option<Value>, u8> {
        match store_call(home, args) {
            Ok(v) => Ok(Some(v)),
            // A read the report can go without: its own entry is what is left
            // out of the file.
            Err(_) if matches!(step, Step::Backup | Step::PollIdle) => Ok(None),
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
    // given; only a read that refused leaves it out.

    let budget = over.as_ref().map(|_| budget.to_string());
    Ok(Given {
        sessions: Some(sessions),
        units: failed_units(),
        backup,
        poll_over: over,
        poll_budget: budget,
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
/// makes nothing of.
fn stopped(step: Step, no: &Refusal) -> u8 {
    if no.word == "absent" {
        return not_installed();
    }
    let cut = first(&no.line, 300);
    match step {
        Step::PlanShow => {
            eprintln!("ERROR: the planning sessions could not be read from the state store: {cut}");
            // The status the store's own answer carries.
            if no.rc == 1 {
                1
            } else {
                78
            }
        }
        Step::Publish => {
            eprintln!("ERROR: the live view could not be published: {cut}");
            // One try, and only a store that cannot be used at all ends the
            // run: a store that was busy leaves the views a tick old.
            if no.rc == 1 {
                0
            } else {
                78
            }
        }
        // A check the report can go without: its own entry is what is left
        // out, and the run says nothing of it.
        Step::Backup | Step::PollIdle => 0,
    }
}

/// One run, as `tehasbot health` ran it: the store's answers in the order the
/// shell asked for them, then `report`, then the publish of what it made. A
/// read of the store that refused is no reason to write no file; only the
/// sessions, a `publish` the store could not use at all, and the publish of
/// the report stop the run, each with the line and the status the shell gave
/// them.
pub fn run(given: &[String]) -> u8 {
    let [home] = given else {
        eprintln!("console health: usage: run <home>");
        return 2;
    };
    let budget =
        std::env::var("TEHAS_POLL_BUDGET").unwrap_or_else(|_| "cpu=5,wall=30,writes=0".to_string());
    run_with(Path::new(home), budget)
}

/// The run itself, with the budget the host's settings give: the case that
/// names a budget of its own does not reach over the environment of the
/// process the tests of the workspace share.
fn run_with(home: &Path, budget: String) -> u8 {
    let given = match checks(home, &budget) {
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
    let outcome = publish_health(home, &file);
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

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    /// Which of the two `publish` calls a refusal stands in for.
    enum Publish {
        Views,
        Health,
        Any,
    }

    struct Store {
        home: PathBuf,
        real: PathBuf,
        /// The refusals the wrapper answers, one line for each call the case
        /// stands in for. Every call the run makes is written down.
        body: Vec<String>,
        record: bool,
    }

    /// The store of this build, as the store's own cases find it: the harness
    /// is built with the binaries, so the store is in it, or one directory up.
    fn store_of_this_build() -> PathBuf {
        let deps = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let here = deps.join("store");
        if here.is_file() {
            return here;
        }
        deps.parent().unwrap().join("store")
    }

    /// The store of a home: the store of this build, in a wrapper that writes
    /// down every call made to it and answers the refusals a case named before
    /// the call. Every other verb is the store's own answer, so no case here
    /// leans on the store's arithmetic.
    impl Store {
        fn new(name: &str) -> Self {
            let home = std::env::temp_dir()
                .join(format!("tehas-health-run-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&home);
            fs::create_dir_all(home.join("libexec")).unwrap();
            fs::create_dir_all(home.join("state")).unwrap();
            // The store of this build, taken from the command line of whoever
            // built the tests: the store's own cases run in the same build, so
            // one of its processes knows the binary's path. The gate builds the
            // two programs out of two target directories, so neither can be
            // found beside the other.
            let real = store_of_this_build();
            fs::write(home.join(".tehas-home"), format!("{}\n", home.display())).unwrap();
            // The store itself is kept under a name of its own, because the
            // home's `libexec/store` is the wrapper every call is recorded in.
            fs::copy(&real, home.join("store.real")).unwrap();
            let mut s = Store {
                home,
                real,
                body: Vec::new(),
                record: true,
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
                    .env("TEHAS_HOME", &s.home)
                    .stdin(Stdio::null()),
            );
            assert_eq!(rc, 0, "the store's init said: {said}");
            // A host that passes has a copy of the store from today, so the
            // report of these cases is not held back by a missing one.
            let (rc, said) = merged(
                Command::new(s.home.join("store.real"))
                    .args(["backup"])
                    .env("TEHAS_HOME", &s.home)
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
            fs::write(self.home.join("libexec/store"), &wrapper).unwrap();
            fs::set_permissions(
                self.home.join("libexec/store"),
                fs::Permissions::from_mode(0o755),
            )
            .unwrap();
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

        /// The units read of a host that answers: the list systemd gives when
        /// no tehas unit is failed. The case of systemd that does not answer
        /// is the report's own.
        /// The units read of a host whose systemd answers <text>. The case of
        /// a systemd that does not answer is the report's own.
        fn units(&mut self, text: &str) {
            let dir = self.home.join("bin");
            fs::create_dir_all(&dir).unwrap();
            // The empty list is systemd answering that nothing failed: no
            // line, which is what --no-legend prints for no unit.
            let said = if text.is_empty() {
                ":".to_string()
            } else {
                format!("printf '%s\n' '{text}'")
            };
            fs::write(
                dir.join("systemctl"),
                format!("#!/usr/bin/env bash\n{said}\n"),
            )
            .unwrap();
            fs::set_permissions(dir.join("systemctl"), fs::Permissions::from_mode(0o755)).unwrap();
            std::env::set_var("TEHAS_SYSTEMCTL_DIR", &dir);
        }

        fn run_with(&mut self, budget: &str) -> u8 {
            self.units("");
            let rc = run_with(&self.home, budget.to_string());
            std::env::remove_var("TEHAS_SYSTEMCTL_DIR");
            rc
        }

        fn run(&mut self) -> u8 {
            self.run_with("cpu=5,wall=30,writes=0")
        }
    }

    impl Drop for Store {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.home);
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
}

pub fn main(given: &[String]) -> u8 {
    if given.first().is_some_and(|v| v == "run") {
        return run(&given[1..]);
    }
    let usage = || {
        eprintln!(
            "console health: usage: run <home> | report <home> [--sessions <json>] [--units <text>] [--backup <json>] [--poll-over <json> --poll-budget <spec>]"
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
            _ => return usage(),
        }
    }
    println!("{}", report(home, &g, time::now()).pretty());
    0
}

fn read(path: &Path) -> Option<J> {
    J::parse(&fs::read_to_string(path).ok()?)
}

/// A file that is there and not empty, as one JSON value, or null.
fn snapshot(path: &Path) -> J {
    match fs::metadata(path) {
        Ok(m) if m.len() > 0 => read(path).unwrap_or(J::Null),
        _ => J::Null,
    }
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

/// The platform checkout against origin, as tehasbot status last read it.
fn platform(live: &Path) -> J {
    match read(&live.join("platform.json")) {
        Some(p @ J::Obj(_)) if matches!(p.get("behind"), J::Num(_)) => p,
        _ => J::Null,
    }
}

/// The slice as the last poll left it, when that poll is young enough.
fn slice(live: &Path, now: i64) -> J {
    let d = snapshot(&live.join("dispatch.json"));
    let young = age_of(d.get("time"), now).is_some_and(|a| a < POLL_MAX_AGE);
    if !matches!(d, J::Obj(_)) || d.get("why").as_str().is_none() || !young {
        return J::Null;
    }
    let mut f = get_fields(
        &d,
        &[
            "time",
            "why",
            "hold",
            "count",
            "room",
            "committed",
            "max",
            "reserve",
        ],
    );
    for k in ["free", "awaitRoom", "limitRoom", "refreshRoom"] {
        if !d.get(k).is_null() {
            f.push((k.to_string(), d.get(k).clone()));
        }
    }
    // What the two counts are when an instance says `auto`: the poll derived
    // them from the host on this very poll, and the reason says which reading
    // gave them. Left out, as every other optional field is, when the poll
    // wrote none: a factory with numbers for both settings reports nothing
    // new.
    if !d.get("slots").is_null() {
        f.push(("slots".to_string(), d.get("slots").clone()));
    }
    J::Obj(f)
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
    J::Obj(f)
}

/// Whether some process holds the lock: a lock no one holds is taken for a
/// moment and given back when the file closes, as `flock -n` does.
fn held(lock: &Path) -> bool {
    File::open(lock).is_ok_and(|f| f.try_lock().is_err())
}

/// The planning turns in flight, each {issue, turn, startedAt}: the sessions
/// the store has running whose turn holds its lock and has not left its exit
/// status (lib/planning.sh).
fn plans(home: &Path, sessions: &J) -> J {
    let state = home.join("state");
    let mut out = Vec::new();
    for s in sessions.get("sessions").items() {
        if !s.get("running").is_true() || !matches!(s.get("issue"), J::Num(_)) {
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
        let lock = state.join(format!("plan-{n}.json.lock"));
        let rc = state.join(format!("plan-{n}.json.rc"));
        if !lock.is_file() || rc.exists() || !held(&lock) {
            continue;
        }
        let turns = match s.get("turns") {
            t @ J::Num(_) => t.as_f64().unwrap_or(0.0),
            _ => 0.0,
        };
        out.push(obj(vec![
            ("issue", s.get("issue").clone()),
            ("turn", J::num(turns + 1.0)),
            ("startedAt", iso(s.get("started_at"))),
        ]));
    }
    J::Arr(out)
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

fn ended_well(c: &J) -> bool {
    in_list(c.get("status").or(&J::str("")), &["merged", "handoff"])
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
    let sessions = g.sessions.as_deref().and_then(J::parse).unwrap_or(J::Null);
    let plans = plans(home, &sessions);
    let limit =
        J::parse(&flags::read(&home.join("state/limit.json"), now).json()).unwrap_or(J::Null);

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
    let no_start = !slice.is_null() && in_list(slice.get("why"), &["slice", "disk", "unreadable"]);
    let no_resources = !slice.is_null() && slice.get("why").as_str() == Some("resources");
    let old_queue = queue_age.is_some_and(|a| a > QUEUE_MAX_AGE);
    let paused = !flag.is_null() || label || lifted;
    let limit_holds = in_list(limit.get("state"), &["expired", "unreadable"]);
    let over_budget = !poll_cost.is_null() && poll_cost.get("over").len() > 0;
    let old_backup = backup_known
        && (backup.is_null()
            || backup
                .get("ageSeconds")
                .cmp(&J::num(BACKUP_MAX_AGE))
                .is_gt());
    let disk_low = !disk.is_null() && disk.get("low").truthy();
    let has_units = units.as_ref().is_some_and(|u| u.len() > 0);

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
            last.get("status").text()
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
        let hold = slice.get("hold");
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
    } else if old_backup && backup.is_null() {
        "There is no backup of the state store: tehasbot clean makes one a day.".to_string()
    } else if old_backup {
        format!(
            "The newest backup of the state store is {} old.",
            ago(backup.get("ageSeconds"))
        )
    } else if over_budget {
        format!(
            "An idle poll is over its budget on {} (TEHAS_POLL_BUDGET={}).",
            joined(poll_cost.get("over")),
            poll_cost.get("budget").text()
        )
    } else if running.len() > 0 {
        let issues: Vec<String> = running
            .items()
            .iter()
            .map(|c| format!("#{}", c.get("issue").text()))
            .collect();
        format!("Working on {}.", issues.join(", "))
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
    let mut ends: Vec<J> = ends
        .into_iter()
        .filter(|e| matches!(e, J::Obj(_)) && !ended_well(e))
        .collect();
    let key = |e: &J| J::Arr(vec![e.get("issue").clone(), e.get("startedAt").clone()]);
    ends.sort_by(|a, b| key(a).cmp(&key(b)));
    ends.dedup_by(|b, a| key(a).cmp(&key(b)).is_eq());
    ends.sort_by(|a, b| a.get("updatedAt").cmp(b.get("updatedAt")));
    ends.reverse();
    let failed_list = J::Arr(
        ends.iter()
            .map(|e| {
                obj(vec![
                    ("issue", e.get("issue").clone()),
                    ("status", e.get("status").clone()),
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
                ("poll_cost", poll_cost),
            ]),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const NOW: i64 = 1_792_245_620; // 2026-10-17T14:00:20Z

    struct Home(PathBuf);

    impl Drop for Home {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// A home with a fresh queue snapshot and nothing else.
    fn home(name: &str) -> Home {
        let dir = std::env::temp_dir().join(format!("tehas-health-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
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
            r#"[{"issue":7,"turn":2,"startedAt":"2026-10-17T13:50:00Z"}]"#
        );
        h.write("state/plan-7.json.rc", "0");
        assert_eq!(h.report(&g).at(&["checks", "plans"]).len(), 0);
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
