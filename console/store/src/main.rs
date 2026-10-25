//! The state store: `$TEHAS_HOME/state/tehas.db`, the host's own records as
//! rows rather than as about 35 kinds of file, and the only writer of that
//! database. `docs/specs/2026-10-21-state-store.md` is the design; section 3
//! is the schema (src/schema.sql) and section 4 this command.
//!
//!   store init [--force] [--version <v>] [--commit <sha>] [--instance <name>]
//!                [--repo <owner/name>] [--json]
//!     Makes the database: schema v2 in an empty file and PRAGMA
//!     user_version 2, and the row of meta that says when it was made, by
//!     which version and commit of tehas, and for which instance and
//!     repository. The four are required: the command that runs the store has
//!     loaded them, and the store refuses a call that arrived without one (64)
//!     rather than guessing a default. Without --force it refuses a file that
//!     already exists and changes nothing; with --force it replaces a file
//!     that holds nothing and refuses one that holds tables, because a
//!     database with rows in it is not the store's to discard (ADR 0003).
//!     Answers
//!     {"api":1,"verb":"init","ok":true,"schema":2,"created":true}, or
//!     "created":false with --force over a file the store then filled.
//!   store migrate --backup <dir> [--json]
//!     Backs the database up and then brings it forward, in that order: a
//!     consistent copy of the store as it stands is written to
//!     <dir>/tehas-<to>-<time>.db with VACUUM INTO before a byte of the store is
//!     changed, and a backup that cannot be made migrates nothing. The copy the
//!     answer names is that one, so no step runs without the copy a caller is
//!     told to open already being on disk; a step that runs also leaves a second
//!     copy under the version the file came with. From 0, an SQLite file with no
//!     tables and user_version 0 (what an empty file is), the schema, the rows of
//!     meta and the version are written in one transaction. From 1, the step to 2
//!     adds columns and a table, also in one transaction, and keeps every row. A
//!     database init made is already at 2, and says "from":2, "to":2 and still
//!     writes its backup, the one copy of the two. <dir> is the
//!     state/ of this instance's home, or state/backup/ below it, which the verb
//!     makes 0700 when it is not there; the copy takes the 0700 of the directory
//!     and the 0600 of the file. Another directory is refused, because the store
//!     was not given leave to write a copy of itself in a person's.
//!     Answers
//!     {"api":1,"verb":"migrate","ok":true,"from":1,"to":2,
//!     "backup":"..."}.
//!   store check [--json]
//!     Reads only: the schema version, PRAGMA integrity_check, and the rows of
//!     every table. Answers {"api":1,"verb":"check","ok":true,"schema":2,
//!     "integrity":"ok","rows":{...}}. A database that fails its integrity
//!     check answers with "integrity" naming what failed and exit 1; it is
//!     never moved aside.
//!
//!   store import [--dry-run] [--json]
//!     The records an older tehas kept as files under state/ (the cycle's,
//!     the park's and the planning session's), made rows, and each file moved
//!     to state/imported-<YYYYMMDD>/ once its rows read back (src/import.rs).
//!     $TEHAS_HOME/.tehas-home must be there, else 78 `unmarked`. A cycle or
//!     planning lock held while there is anything to import is 1 `live`; a
//!     file that is a link, over 1 MiB or does not parse stays where it is,
//!     with its issue's other files, and the verb exits 1 `held` with one
//!     line per file. --dry-run reads the files only. Answers
//!     {"api":1,"verb":"import","ok":true,"imported":{<table>:N},"moved":N,
//!     "held":[],"dry_run":false}.
//!
//! This part of the store is the schema, the two housekeeping verbs, migrate,
//! import, the record verbs of cycles, parks, questions, approvals and
//! planning, and the history (src/verbs.rs and the files it names):
//!
//!   store cycle-start <issue> [--rukas <commit>] [--by-hand]
//!   store cycle-image <issue> --attempt N --image <id> [--rukas <commit>] [--settings <file>]
//!   store cycle-end <issue> --attempt N --status <word> [...] | --refused | --stopped
//!   store cycle-show <issue> | --running
//!   store cycle-note <issue> --kind quiet --said-for <key>
//!   store cycle-stop-asked <issue> [--said-only]
//!   store cycle-heartbeat <issue> [--attempt N] --running|--ended --status <word>
//!                [--step <word>] [--text <notice>]
//!   store publish [--dispatch <file>] [--health <file>]
//!   store park <issue> --attempt N --reason <w> [--step <w>] [--pr N]
//!                [--comment-id <id> --body-sha <sha256>]
//!   store park-show <issue> | --automatic
//!   store park-retry <issue>
//!   store park-told <issue>
//!   store question-asked <issue> --id <id> --kind park|plan [--attempt N] [--body-sha <sha256>]
//!   store question-known <issue> --id <id>
//!   store question-answered <issue> --id <id> --by <account> | --withdraw
//!   store approval-record <issue> --at <unix> --by <account> --how <how> ...
//!                [--input-file <file>] | --undo --at <unix>
//!   store approval-current <issue>
//!   store plan-begin <issue> --by <account> --label <label> --brief-file <file> | --undo
//!   store plan-answer <issue> --turn N --by <account> [--fixer] [--text-file <file>]
//!                | --turn N --undo
//!   store plan-end <issue> --turn N --outcome <o> [...] | --posted | --close
//!   store plan-note <issue> --kind fixer-refused|held --id <id>
//!   store plan-show <issue> | --open
//!   store history-show [<issue>] [--kind cycle|plan] [--how <how>]
//!
//! Each answers one line, {"api":1,"verb":"<verb>","ok":true,...}. A record
//! verb that refuses says `store: usage <verb>: ...` (64) or
//! `store: <word> #<issue>: ...` (1). The rest of the 13 groups, state-audit
//! and the journal are later parts of epic #244; an argument this command does
//! not know is exit 64. A build with debug assertions also has `test-write`,
//! the writer its own tests run (src/exercise.rs), and reads its clock from
//! TEHAS_STORE_TEST_NOW when that is set; a release build has neither.
//!
//! The database is $TEHAS_HOME/state/tehas.db, mode 0600, made so before
//! SQLite opens it, in WAL mode with synchronous=FULL and foreign_keys ON on
//! every connection. A connection that writes waits up to 5000 ms for a lock,
//! asking again every millisecond; one that reads waits up to a second. Its
//! directory, state/, is kept mode 0700: with WAL on, a readable state/ is a
//! readable copy of the store.
//! The path is refused when it is a link or sits under live/ or state/index;
//! no container sees the file (AGENTS.md rule 4). The store never moves a
//! damaged file aside: the four classes of failure stop the command with 78
//! and say which one they are, as ADR 0003 decides.
//!
//! The values meta records about the installation are the caller's: a command
//! that runs the store has itself loaded the instance settings and passes what
//! the verb names. The store does not read the settings of a host itself, so
//! the rules of what a valid setting is stay in one place, lib/config.sh.
//!
//! Exit: 0 the verb did its work; 64 an argument this verb does not know, or
//! settings that did not arrive in the shape the verb states, and nothing was
//! written; 78 the database: `missing`, `corrupt`, `unreadable` or `future`,
//! on standard error as the first word of the line, and nothing was written;
//! 1 a call failed or a precondition was not met, or the database was locked
//! past the busy timeout, which says `busy`.

mod approval;
mod cycle;
mod db;
#[cfg(debug_assertions)]
mod exercise;
mod history;
mod import;
mod live;
mod lock;
mod park;
mod plan;
mod verbs;

#[cfg(test)]
mod approval_tests;
#[cfg(test)]
mod cycle_tests;
#[cfg(test)]
mod history_tests;
#[cfg(test)]
mod import_tests;
#[cfg(test)]
mod live_tests;
#[cfg(test)]
mod lock_tests;
#[cfg(test)]
mod park_tests;
#[cfg(test)]
mod plan_tests;
#[cfg(test)]
mod schema_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod verbs_tests;

#[cfg(test)]
/// The tests call what main() calls: the verbs, the classification of an error,
/// and the line and exit each failure becomes. Nothing about a failure is
/// tested by watching a process, so these are the seams the tests are given.
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
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

const USAGE: &str = "usage: store init [--force] [--version <v> --commit <sha> --instance <name> --repo <owner/name>] [--json] | migrate --backup <dir> [--json] | check [--json] | import [--dry-run] [--json] | cycle-start | cycle-image | cycle-end | cycle-show | cycle-note | cycle-stop-asked | cycle-heartbeat | publish | park | park-show | park-retry | park-told | question-asked | question-known | question-answered | approval-record | approval-current | plan-begin | plan-answer | plan-end | plan-note | plan-show | history-show";

/// The longest a value the caller passes may be. The four come from an
/// instance's settings and a checkout's git, not from a container.
const MAX_PLAIN: usize = 64;
const MAX_REPO: usize = 200;
const MAX_COMMIT: usize = 40;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(Err(why)) = verbs::parse(&args) {
        eprintln!("store: {why}");
        return ExitCode::from(64);
    }
    let Some(verb) = parse(&args) else {
        eprintln!("{USAGE}");
        return ExitCode::from(64);
    };
    let home = match std::env::var_os("TEHAS_HOME").map(PathBuf::from) {
        Some(h) if h.is_absolute() && h.is_dir() => h,
        _ => {
            eprintln!("store: TEHAS_HOME is not set to a directory");
            return ExitCode::from(78);
        }
    };
    let now = match now() {
        Ok(t) => t,
        Err(why) => {
            eprintln!("store: {why}");
            return ExitCode::from(78);
        }
    };
    match execute(&home, &verb, now) {
        Ok(line) => {
            println!("{line}");
            ExitCode::SUCCESS
        }
        Err((code, why)) => {
            eprintln!("store: {why}");
            ExitCode::from(code)
        }
    }
}

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
    Check,
    Import {
        dry_run: bool,
    },
    #[cfg(debug_assertions)]
    TestWrite(exercise::Write),
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
        [verb, rest @ ..] if verb == "import" => {
            flags(rest, &["--json", "--dry-run"])?;
            Some(Verb::Import {
                dry_run: rest.iter().any(|a| a == "--dry-run"),
            })
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
    let (owner, name) = repo.split_once('/')?;
    let part = |s: &str| {
        !s.is_empty()
            && s.len() <= MAX_REPO
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    };
    if !part(owner) || !part(name) || name.is_empty() {
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
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64))
}

/// The shell tests' clock: a build with debug assertions takes the time from
/// this variable when it is set. install.sh builds --release, which never
/// reads it.
#[cfg(debug_assertions)]
pub const TEST_NOW: &str = "TEHAS_STORE_TEST_NOW";

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
                // The word first, as every failure of the store says its own,
                // so a caller that reads only the word knows the database is
                // ill. It is not one of the four: nothing about the file stops
                // the answer, so the exit is 1 and the check names itself.
                return Err((
                    1,
                    format!("integrity {}: {}", report.integrity, file.display()),
                ));
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
        Verb::Import { dry_run } => import::run(home, *dry_run, now),
        Verb::Migrate { backup, settings } => migrate(&file, backup, settings, now),
        #[cfg(debug_assertions)]
        Verb::TestWrite(w) => exercise::write(&file, w, now),
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
    let copies = db::backup_dir(&home_of(file), dir).map_err(|e| {
        let _ = db::settle(&conn);
        line(e)
    })?;
    let from: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(db::classify)
        .map_err(|e| {
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
    // The step is its own transaction and its own lock, and it is the only one:
    // a database already at the version this binary writes has no step to run,
    // so nothing asks for the lock again and the file is not opened for writing
    // at all. Both copies are on disk by now, so a step that fails halfway
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
        "{{\"api\":1,\"verb\":\"migrate\",\"ok\":true,\"from\":{from},\"to\":{to},\"backup\":{name:?}}}"
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
        db::DbError::Missing => "state/tehas.db",
        _ => "the database",
    }
}
