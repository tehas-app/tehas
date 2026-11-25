//! The state store as a library: the verbs of the `store` program, and
//! `call`, which runs one of them as the program does. The program
//! (src/main.rs) and the console both go through `call`; src/main.rs says
//! what each verb does.

mod answer;
mod approval;
mod batch;
mod call;
mod cycle;
mod db;
#[cfg(debug_assertions)]
mod exercise;
mod history;
mod journal;
mod link;
mod live;
mod load;
mod lock;
mod order;
mod park;
mod plan;
mod plugin;
mod poll;
mod record;
mod say;
mod upkeep;
mod verbs;

#[cfg(test)]
mod answer_tests;
#[cfg(test)]
mod approval_tests;
#[cfg(test)]
mod call_tests;
#[cfg(test)]
mod cycle_tests;
#[cfg(test)]
mod history_tests;
#[cfg(test)]
mod journal_tests;
#[cfg(test)]
mod link_tests;
#[cfg(test)]
mod live_tests;
#[cfg(test)]
mod load_tests;
#[cfg(test)]
mod lock_tests;
#[cfg(test)]
mod park_tests;
#[cfg(test)]
mod plan_tests;
#[cfg(test)]
mod poll_tests;
#[cfg(test)]
mod record_tests;
#[cfg(test)]
mod say_tests;
#[cfg(test)]
mod schema_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod upkeep_tests;
#[cfg(test)]
mod verbs_tests;

pub use call::{call, call_with, said, Answer, ANSWERS};
pub use say::strip as strip_say;

#[cfg(test)]
/// The tests call what main() calls: the verbs, the classification of an error,
/// and the line and exit each failure becomes. Nothing about a failure is
/// tested by watching a process, so these are the seams the tests are given:
/// the four classes of failure and the word of each.
pub use db::{classify, classify_io};

/// The home of a database the store named itself: `db::path` joins state/ and
/// the file onto it, so the home is the directory two above the file. `migrate`
/// asks for it rather than the environment again because the verb was given the
/// file by `execute`, and the two must agree about whose store this is.
fn home_of(file: &Path) -> PathBuf {
    file.parent()
        .and_then(Path::parent)
        .unwrap_or(file)
        .to_path_buf()
}

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const USAGE: &str = "usage: store init [--force] [--version <v> --commit <sha> --instance <name> --repo <owner/name>] [--json|--say] | migrate --backup <dir> [--json|--say] | migrate --dry-run [--backup <dir>] [--json] | check [--json] | batch | cycle-start | cycle-image | cycle-end | cycle-show | settings-view | cycle-note | cycle-stop-asked | cycle-heartbeat | publish | park | park-show | park-retry | park-told | question-asked | question-known | question-adopt | question-answered | answer-record | answer-drop | question-show | question-list | approval-record | approval-current | approval-view | plan-begin | plan-answer | plan-end | plan-note | plan-show | plan-view | history-show | backup | copy | prune | sweep-locks | locks-held | upkeep | poll-record | poll-show | poll-idle | poll-begin | link-record";

/// The longest a value the caller passes may be. The four come from an
/// instance's settings and a checkout's git, not from a container.
const MAX_PLAIN: usize = 64;
const MAX_REPO: usize = 200;
const MAX_COMMIT: usize = 40;

/// What a call to the command asked for. `--json` is accepted on every verb
/// and is what the verbs print anyway: one object on stdout, `"api": 1` first,
/// as every JSON answer of tehas is written (host/docs/api.md).
#[derive(Debug, PartialEq, Eq)]
pub enum Verb {
    Init {
        force: bool,
        settings: Settings,
    },
    Migrate {
        backup: PathBuf,
        settings: Settings,
    },
    /// `migrate --dry-run`: whether this store would migrate the database,
    /// read only. install.sh asks it before it copies anything. With
    /// `--backup <dir>` it also opens the database to write, settles `<dir>`
    /// and writes the copies `migrate` would write there, then removes them:
    /// what stops the real migration before its first step stops this too.
    MigrateDry {
        backup: Option<PathBuf>,
    },
    Check,
    #[cfg(debug_assertions)]
    TestWrite(exercise::Write),
    /// The calls of one `store batch`, already read from standard input and
    /// judged by `batch::parse`: built by `batch::Request::of` and by nothing
    /// else, so a caller of `execute` cannot name a verb the batch would run.
    Batch(batch::Request),
    Record(verbs::Call),
}

/// What `init` and `migrate` were given about the installation: the four
/// values of the instance and the checkout, passed down by the command that
/// loaded them. The store checks that each arrived and is shaped as the verb
/// states, and stops with 64 when one did not: the rules of a valid setting
/// stay in lib/config.sh, and `migrate` needs the same four because its only
/// step writes the rows of meta that `init` would have written.
pub use db::Settings;

fn parse(args: &[String]) -> Option<Verb> {
    if let Some(call) = verbs::parse(args) {
        return call.ok().map(Verb::Record);
    }
    match args {
        [verb, rest @ ..] if verb == "check" => {
            flags(rest, &["--json"])?;
            Some(Verb::Check)
        }
        [verb, rest @ ..] if verb == "migrate" && rest.iter().any(|a| a == "--dry-run") => {
            let mut backup: Option<PathBuf> = None;
            let mut dry = false;
            let mut i = 0;
            while i < rest.len() {
                match rest[i].as_str() {
                    "--json" => i += 1,
                    "--dry-run" if !dry => {
                        dry = true;
                        i += 1;
                    }
                    "--backup" if backup.is_none() => {
                        let dir = PathBuf::from(rest.get(i + 1)?);
                        if !dir.is_absolute() {
                            return None;
                        }
                        backup = Some(dir);
                        i += 2;
                    }
                    _ => return None,
                }
            }
            Some(Verb::MigrateDry { backup })
        }
        [verb, rest @ ..] if verb == "migrate" => {
            let mut backup: Option<String> = None;
            let mut given: BTreeMap<&str, &str> = BTreeMap::new();
            let mut i = 0;
            while i < rest.len() {
                match rest[i].as_str() {
                    "--json" => i += 1,
                    "--backup" => {
                        // Twice is two answers, and the verb writes one file.
                        if backup.is_some() {
                            return None;
                        }
                        backup = Some(rest.get(i + 1)?.clone());
                        i += 2;
                    }
                    // A directory is a path and not a setting, so it is not
                    // offered to the shape a setting must have: one that starts
                    // with a dash is not an absolute path either.
                    flag if flag.starts_with("--") => {
                        let value = rest.get(i + 1)?;
                        if value.starts_with('-') || given.insert(flag, value).is_some() {
                            return None;
                        }
                        i += 2;
                    }
                    _ => return None,
                }
            }
            // The one place a directory is turned into a path: the verb was
            // refused at 64 unless it named one, and the store's home is not a
            // relative place.
            let dir = PathBuf::from(backup?);
            if !dir.is_absolute() {
                return None;
            }
            Some(Verb::Migrate {
                backup: dir,
                settings: settings(given)?,
            })
        }
        [verb, rest @ ..] if verb == "init" => {
            let mut force = false;
            let mut given: BTreeMap<&str, &str> = BTreeMap::new();
            let mut i = 0;
            while i < rest.len() {
                match rest[i].as_str() {
                    "--json" => i += 1,
                    "--force" if !force => {
                        force = true;
                        i += 1;
                    }
                    flag if flag.starts_with("--") => {
                        let value = rest.get(i + 1)?;
                        if given.insert(flag, value).is_some() {
                            return None;
                        }
                        i += 2;
                    }
                    _ => return None,
                }
            }
            Some(Verb::Init {
                force,
                settings: settings(given)?,
            })
        }
        #[cfg(debug_assertions)]
        [verb, rest @ ..] if verb == exercise::VERB => exercise::parse(rest).map(Verb::TestWrite),
        _ => None,
    }
}

/// Flags that take no argument, each at most once.
fn flags(args: &[String], allowed: &[&str]) -> Option<()> {
    let mut seen: Vec<&str> = Vec::new();
    for arg in args {
        let flag = allowed.iter().find(|f| *f == arg)?;
        if seen.contains(flag) {
            return None;
        }
        seen.push(flag);
    }
    Some(())
}

/// The four settings `init` names, each once, each in the shape the verb
/// states. A value that is missing or of another shape is a usage error, and
/// nothing was written.
/// `owner/name` as the settings check it, each half at most `max` bytes.
fn repo_within(repo: &str, max: usize) -> bool {
    tehas_core::settings::rules::is_repo(repo) && repo.split('/').all(|p| p.len() <= max)
}

fn settings(given: BTreeMap<&str, &str>) -> Option<Settings> {
    let plain = |value: &str, max: usize| -> Option<String> {
        let ok = !value.is_empty()
            && value.len() <= max
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-+@".contains(&b));
        ok.then(|| value.to_string())
    };
    let commit = given.get("--commit")?;
    let hex = commit.len() == 40 && commit.bytes().all(|b| b.is_ascii_hexdigit());
    let sha = commit.len() >= 7
        && commit.len() <= MAX_COMMIT
        && commit.bytes().all(|b| b.is_ascii_hexdigit());
    if !(hex || sha) {
        return None;
    }
    let repo = given.get("--repo")?;
    if !repo_within(repo, MAX_REPO) {
        return None;
    }
    Some(Settings {
        version: plain(given.get("--version")?, 24)?,
        commit: commit.to_string(),
        instance: plain(given.get("--instance")?, MAX_PLAIN)?,
        repo: repo.to_string(),
    })
}

fn now() -> Result<i64, String> {
    #[cfg(debug_assertions)]
    if let Some(t) = test_clock(std::env::var_os(TEST_NOW).as_deref())? {
        return Ok(t);
    }
    Ok(tehas_core::time::now())
}

/// The shell tests' clock: a build with debug assertions takes the time from
/// this variable when it is set. install.sh builds --release, which never
/// reads it.
#[cfg(debug_assertions)]
pub const TEST_NOW: &str = "TEHAS_STORE_TEST_NOW";

/// The tests' short busy and lock bounds are `db::install_test_bounds`, which
/// only the test binary can call: a build a host runs -- debug or release -- has
/// no code at all that reads a setting for a bound, and waits `db::BUSY_TIMEOUT`,
/// `db::READ_WAIT` and `live::LOCK_WAIT`.
///
/// `store --test-store-bound-check <file>` is the argument that shows it. The
/// store answers with the bound it applied to the write lock, whether it met
/// `busy` waiting for that lock and how long it waited, so a test can run the
/// very program a host runs against a lock it cannot get and compare what it
/// waited with what it was given: the answer names both, so neither side of the
/// comparison is a constant the other has to be kept in step with. A release
/// build -- what install.sh installs -- does not know the argument, as
/// `test-write` is not known by one either.
#[cfg(debug_assertions)]
pub const BOUND_CHECK: &str = "--test-store-bound-check";

#[cfg(debug_assertions)]
pub fn bound_check(file: &Path) -> Result<String, (u8, String)> {
    let asked = std::time::Instant::now();
    // The bound of the wait and the constant it is supposed to come from, both
    // read off this very program: the assertions of
    // `tests::a_store_the_tests_did_not_shorten_waits_the_installed_bounds` are
    // made against them, so a bound that drifted from the constant is seen here
    // rather than read past from the parent's own compilation.
    let (constant, bound) = db::bound_check_answer(db::Purpose::Write);
    // `lock` and not `open`: BEGIN IMMEDIATE is the statement that asks for the
    // write lock, so the wait is the one whose bound is in question.
    let opened = db::lock(file).map(|conn| {
        let _ = db::settle(&conn);
        false
    });
    let waited = asked.elapsed().as_millis();
    let said = |busy: bool| {
        format!(
            "{{\"api\":1,\"verb\":\"store-bound-check\",\"ok\":true,\
             \"busy\":{busy},\"waited_ms\":{waited},\
             \"bound_ms\":{},\"installed_ms\":{}}}",
            bound.as_millis(),
            constant.as_millis()
        )
    };
    match opened {
        Ok(busy) => Ok(said(busy)),
        Err(db::DbError::Busy) => Ok(said(true)),
        Err(e) => Err(line(e)),
    }
}

/// The name of the variable of the busy notice, spelled once in `src/db.rs`.
#[cfg(debug_assertions)]
pub use db::TEST_BUSY_REPORT;

/// The time the variable gives, None when it is not set. A value that is not
/// a Unix time stops the command rather than fall back to the real clock.
#[cfg(debug_assertions)]
pub fn test_clock(given: Option<&std::ffi::OsStr>) -> Result<Option<i64>, String> {
    let Some(given) = given else {
        return Ok(None);
    };
    given
        .to_str()
        .and_then(verbs::unix)
        .map(Some)
        .ok_or_else(|| format!("{TEST_NOW} is not a Unix time"))
}

/// Runs one verb and returns the line it prints. The error is the exit status
/// and the one line for standard error.
pub fn execute(home: &Path, verb: &Verb, now: i64) -> Result<String, (u8, String)> {
    let file = db::path(home);
    match verb {
        Verb::Check => {
            // Reads only: `check` writes no row, so it does not wait on a
            // poll's lock. SQLite may still make the -wal and -shm.
            let conn = db::open_read_only(&file).map_err(line)?;
            let report = db::check(&conn).map_err(line)?;
            if report.integrity != "ok" {
                // A failed integrity check is a damaged file, the same as one
                // whose quick_check fails at the open: 78 and `corrupt` first,
                // so a caller that reads the word stops on the store instead
                // of reading it as one refused request and carrying on.
                return Err(line(db::DbError::Corrupt(format!(
                    "integrity check failed: {}: {}",
                    report.integrity,
                    file.display()
                ))));
            }
            let rows = report
                .rows
                .iter()
                .map(|(name, n)| format!("\"{name}\":{n}"))
                .collect::<Vec<_>>()
                .join(",");
            Ok(format!(
                "{{\"api\":1,\"verb\":\"check\",\"ok\":true,\"schema\":{},\"integrity\":\"ok\",\"rows\":{{{rows}}}}}",
                report.schema
            ))
        }
        Verb::Init { force, settings } => init(&file, *force, settings, now),
        Verb::Migrate { backup, settings } => migrate(&file, backup, settings, now),
        Verb::MigrateDry { backup: Some(dir) } => rehearse(&file, dir, now),
        Verb::MigrateDry { backup: None } => {
            let conn = db::open_read_only(&file).map_err(line)?;
            let from: i64 = conn
                .query_row("PRAGMA user_version", [], |r| r.get(0))
                .map_err(db::classify)
                .map_err(line)?;
            db::migrable(&file, from).map_err(line)?;
            Ok(format!(
                "{{\"api\":1,\"verb\":\"migrate\",\"ok\":true,\"dry_run\":true,\"from\":{from},\"to\":{}}}",
                db::SCHEMA_VERSION
            ))
        }
        #[cfg(debug_assertions)]
        Verb::TestWrite(w) => exercise::write(&file, w, now),
        Verb::Batch(asked) => batch::run(home, asked),
        Verb::Record(call) => verbs::run(home, call, now),
    }
}

/// `migrate --backup <dir>`: the copy first, then the steps forward.
///
/// The order is the deliverable. The database is opened for writing so that a
/// lock another verb holds is met before anything is decided, the backup is
/// written from that connection while nothing else can change the file, and
/// only then do the steps run. So a backup that cannot be written leaves the
/// schema version exactly as it was: the verb has not touched the store, and
/// the failure is the ordinary one of the store's four words.
///
/// The copy written first is the one the answer names, taken under the version
/// the store ends at. A copy taken after the step is not a guard: when it fails
/// the store has already been migrated, and the caller is left with an error and
/// no name to restore from. So the reported name is on disk before the step can
/// commit, and where the two names differ -- a database below this binary's
/// version -- the copy under the version the file came with is written beside it
/// before the step too.
///
/// `dir` is the argument the caller gave, and `backup_dir` below is the only
/// reader of it: the copy is written in `state/` or in `state/backup/`, where the
/// 0700 of the directory and the 0600 of the store keep a copy of the host's
/// approvals as unreadable as the store itself. A directory off the command line
/// is a person's -- it could be a link, it could walk out of the home with a
/// `..`, it could be 0755 -- and the store was never given leave to write a copy
/// of itself there. So the answer a wrong `<dir>` gets is the same refusal
/// whatever it names, and it comes before the copy: the verb cannot be made to
/// put a copy of the store in a place the host did not agree to read.
fn migrate(file: &Path, dir: &Path, settings: &Settings, now: i64) -> Result<String, (u8, String)> {
    // The write lock first, before the version is read: a verb that writes the
    // store meets a lock another verb holds before it has decided anything, and
    // SQLite will not answer `busy` for a read-only open. Then it is given back,
    // because SQLite will not copy a database that a transaction is open on. It
    // is the verb's own lock and nothing else takes it in between: a host's
    // other verbs wait on the busy timeout, and the step ahead takes the lock
    // again in its own transaction.
    db::settle_state_dir(
        file.parent()
            .expect("the store's path has a directory of its own"),
    )
    .map_err(line)?;
    // The file's own mode comes from the store too. `create` is what gives it
    // to a file `init` makes, and the empty file of a migration arrives from a
    // host's `touch` at the umask: a verb that fills it with the store's rows
    // leaves it at 0600, whatever it came with. A file that is not there yet is
    // `open`'s to answer for, with the word `missing`.
    if file.exists() {
        db::set_file_mode(file).map_err(line)?;
    }
    let mut conn = db::lock(file).map_err(line)?;
    // A store this binary does not migrate is refused before any copy of it
    // is written, so it is left exactly as it was.
    let from: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(db::classify)
        .and_then(|from| db::migrable(file, from).map(|()| from))
        .map_err(|e| {
            let _ = db::settle(&conn);
            line(e)
        })?;
    let copies = db::backup_dir(&home_of(file), dir).map_err(|e| {
        let _ = db::settle(&conn);
        line(e)
    })?;
    db::settle(&conn).map_err(line)?;
    // The version of the copy the verb reports. For a database that needs a
    // step it is the one the step takes the store to, known before it runs: a
    // database above the version this binary writes is refused by `lock`'s
    // checks, so this is the ceiling and not a guess. A database already at it
    // has no step, and the name is the one it carries already.
    let target = db::SCHEMA_VERSION;
    // The copy the verb reports is the one the caller will open, so it is
    // written first: before it, the step has not run, and a copy that cannot be
    // made leaves the store exactly as it was -- no schema, no version, no row.
    // It holds the database as the verb found it, under the name the answer
    // gives, which is the copy whose bytes predate whatever the step does next.
    let reported = db::backup(&conn, &copies, target, now).map_err(line)?;
    // The copy under the version the file came with, asked of only when the two
    // names differ. A database at the version this binary writes is copied once:
    // the same bytes under two names would be a directory of backups saying a
    // migration happened when none did.
    if target != from {
        db::backup(&conn, &copies, from, now).map_err(line)?;
    }
    // The step is its own transaction and its own lock: a database already at
    // the version this binary writes has no step to run. Both
    // copies are on disk by now, so a step that fails halfway
    // leaves a store nobody was told to open and a copy a person can restore
    // from, rather than a migrated store behind a name that was never written.
    if from < db::SCHEMA_VERSION {
        db::migrate(&mut conn, file, from, now, settings).map_err(line)?;
    }
    // Read back, not assumed: the answer reports the version the file holds,
    // and a step that left it anywhere else is reported as what it did.
    let to: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(db::classify)
        .map_err(line)?;
    if to != from && to != target {
        return Err(line(db::DbError::Corrupt(format!(
            "the step left schema {to} of {} behind, expected {target}",
            file.display()
        ))));
    }
    let copied = reported;
    let name = copied.to_str().ok_or_else(|| {
        (
            78,
            format!("unreadable the backup: {} is not UTF-8", copied.display()),
        )
    })?;
    Ok(format!(
        "{{\"api\":1,\"verb\":\"migrate\",\"ok\":true,\"from\":{from},\"to\":{to},\"backup\":{}}}",
        verbs::j(name)
    ))
}

/// `migrate --dry-run --backup <dir>`: the steps of `migrate` before its first
/// change, on the real file and the real directory. The write open with its
/// quick_check, the version, the backup directory and the copies, each refused
/// as `migrate` refuses it. The copies are removed again, so the database and
/// the directory are left as they were. install.sh runs it before it copies a
/// file, so no room for the copies or a refused directory installs nothing.
fn rehearse(file: &Path, dir: &Path, now: i64) -> Result<String, (u8, String)> {
    verbs::not_empty(file)?;
    let conn = db::lock(file).map_err(line)?;
    let settled = |e: db::DbError| {
        let _ = db::settle(&conn);
        line(e)
    };
    let from: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(db::classify)
        .and_then(|from| db::migrable(file, from).map(|()| from))
        .map_err(settled)?;
    let copies = db::backup_dir(&home_of(file), dir).map_err(settled)?;
    db::settle(&conn).map_err(line)?;
    let target = db::SCHEMA_VERSION;
    let mut made = Vec::new();
    let mut tried = Ok(());
    // Both names when they differ, as `migrate` writes them.
    let versions: &[i64] = if from == target {
        &[target]
    } else {
        &[target, from]
    };
    for &version in versions {
        match db::backup(&conn, &copies, version, now) {
            Ok(copy) => made.push(copy),
            Err(e) => {
                tried = Err(e);
                break;
            }
        }
    }
    for copy in &made {
        if let Err(e) = std::fs::remove_file(copy) {
            return Err(line(db::DbError::Unreadable(format!(
                "cannot remove the trial backup {}: {e}",
                copy.display()
            ))));
        }
    }
    tried.map_err(line)?;
    let dir = copies.to_str().ok_or_else(|| {
        (
            78,
            format!(
                "unreadable the backup directory: {} is not UTF-8",
                copies.display()
            ),
        )
    })?;
    Ok(format!(
        "{{\"api\":1,\"verb\":\"migrate\",\"ok\":true,\"dry_run\":true,\"from\":{from},\"to\":{target},\"backup\":{}}}",
        verbs::j(dir)
    ))
}

/// `init [--force]`: the schema in a file of the store's own making.
///
/// Three cases, and the difference between the last two is the rule of ADR
/// 0003: a file that is not there is made; a file that is there is refused
/// unless --force; and --force replaces only a file that holds nothing. A
/// database with tables in it is never overwritten, and a file that is not a
/// database is not a file the store was asked to fill -- it stays exactly
/// where a person can read it, which is why the refusal is made on the header
/// bytes and not by handing the file to SQLite, whose answer for one of the
/// two is the same word.
fn init(file: &Path, force: bool, settings: &Settings, now: i64) -> Result<String, (u8, String)> {
    let existed = match std::fs::symlink_metadata(file) {
        Ok(m) if m.is_file() => true,
        Ok(_) => return Err(line(db::guard(file))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(line(db::classify_io(&e))),
    };
    let empty = if existed {
        // A file of no bytes is empty in the sense the verb means: SQLite is
        // about to give it its header and its schema. Anything else with the
        // store's name on it is a database of somebody's, and --force does not
        // replace one that holds rows.
        let blank = std::fs::metadata(file)
            .map(|m| m.len() == 0)
            .map_err(|e| line(db::classify_io(&e)))?;
        if !blank && !db::is_database(file) {
            return Err((
                78,
                format!(
                    "corrupt the database: {} is not a database and is not the store's to replace",
                    file.display()
                ),
            ));
        }
        if !force {
            return Err((
                1,
                "the database exists already; init replaces nothing".to_string(),
            ));
        }
        if blank {
            true
        } else {
            let conn = db::open(file, db::Purpose::Write).map_err(line)?;
            let tables: i64 = conn
                .query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get(0))
                .map_err(db::classify)
                .map_err(line)?;
            drop(conn);
            if tables != 0 {
                return Err((
                    1,
                    format!(
                        "{} holds tables; --force replaces an empty file only",
                        file.display()
                    ),
                ));
            }
            true
        }
    } else {
        false
    };
    let mut conn = db::create(file).map_err(line)?;
    // The -wal and -shm siblings are made by SQLite as the first write goes
    // through them, while the write lock is held and the caller still has the
    // connection: they hold the rows until the next checkpoint, so they carry
    // the store's data and take the file's mode. Before the write of the
    // schema they do not exist yet, which is what the store's own test looks
    // for. Their mode is set here rather than in db::create: SQLite takes it
    // from the umask of the process, which a caller of the library can set.
    let sides = || -> Result<(), (u8, String)> {
        for suffix in ["-wal", "-shm"] {
            let sibling = db::side_file(file, suffix);
            if sibling.exists() {
                std::fs::set_permissions(&sibling, std::fs::Permissions::from_mode(0o600))
                    .map_err(|e| (78, format!("unreadable {}: {e}", sibling.display())))?;
            }
        }
        Ok(())
    };
    let written = db::write_schema(
        &mut conn,
        now,
        &settings.version,
        &settings.commit,
        &settings.instance,
        &settings.repo,
    );
    // Named both before and after the transaction ends: SQLite may make the
    // siblings at the first write or only as the commit closes it.
    let settled = match written {
        Ok(()) => sides(),
        Err(e) => Err(line(e)),
    };
    settled?;
    drop(conn);
    Ok(format!(
        "{{\"api\":1,\"verb\":\"init\",\"ok\":true,\"schema\":{},\"created\":{}}}",
        db::SCHEMA_VERSION,
        !empty && !existed
    ))
}

// SQLite names a database's side files by appending to its name; db::side_file
// is the one place that does, for `init` and for the log a migration leaves.

/// The line for standard error: the class of the failure first, then what
/// SQLite said, so a caller reads the word and a person reads the reason.
pub fn line(e: db::DbError) -> (u8, String) {
    let code = if e == db::DbError::Busy { 1 } else { 78 };
    let msg = match e.detail() {
        Some(detail) => format!("{} {}: {detail}", e.word(), name(&e)),
        None => format!("{} {}", e.word(), name(&e)),
    };
    (code, msg)
}

fn name(e: &db::DbError) -> &'static str {
    match e {
        db::DbError::Missing => tehas_core::home::State::StoreDb.relative(),
        _ => "the database",
    }
}
