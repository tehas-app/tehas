//! The state store: `$TEHAS_HOME/state/tehas.db`, the host's own records as
//! rows rather than as about 35 kinds of file, and the only writer of that
//! database. `docs/specs/2026-10-21-state-store.md` is the design; section 3
//! is the schema (src/schema.sql) and section 4 this command.
//!
//!   store init [--force] [--version <v>] [--commit <sha>] [--instance <name>]
//!                [--repo <owner/name>] [--json]
//!     Makes the database: schema v1 in an empty file and PRAGMA
//!     user_version 1, and the row of meta that says when it was made, by
//!     which version and commit of tehas, and for which instance and
//!     repository. The four are required: the command that runs the store has
//!     loaded them, and the store refuses a call that arrived without one (64)
//!     rather than guessing a default. Without --force it refuses a file that
//!     already exists and changes nothing; with --force it replaces a file
//!     that holds nothing and refuses one that holds tables, because a
//!     database with rows in it is not the store's to discard (ADR 0003).
//!     Answers
//!     {"api":1,"verb":"init","ok":true,"schema":1,"created":true}, or
//!     "created":false with --force over a file the store then filled.
//!   store check [--json]
//!     Reads only: the schema version, PRAGMA integrity_check, and the rows of
//!     every table. Answers {"api":1,"verb":"check","ok":true,"schema":1,
//!     "integrity":"ok","rows":{...}}. A database that fails its integrity
//!     check answers with "integrity" naming what failed and exit 1; it is
//!     never moved aside.
//!
//! This part of the store is the schema and the two housekeeping verbs. The
//! verbs of the 13 groups, migrate, state-audit and the journal are later
//! parts of epic #244; an argument this command does not know is exit 64.
//!
//! The database is $TEHAS_HOME/state/tehas.db, mode 0600, made so before
//! SQLite opens it, in WAL mode with synchronous=FULL, a busy timeout of 5000
//! ms and foreign_keys ON on every connection. Its directory, state/, is kept
//! mode 0700: with WAL on, a readable state/ is a readable copy of the store.
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

mod db;

#[cfg(test)]
mod tests;

#[cfg(test)]
/// The tests call what main() calls: the verbs, the classification of an error,
/// and the line and exit each failure becomes. Nothing about a failure is
/// tested by watching a process, so these are the seams the tests are given.
pub use db::{classify, classify_io};

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

const USAGE: &str = "usage: store init [--force] [--version <v> --commit <sha> --instance <name> --repo <owner/name>] [--json] | check [--json]";

/// The longest a value the caller passes may be. The four come from an
/// instance's settings and a checkout's git, not from a container.
const MAX_PLAIN: usize = 64;
const MAX_REPO: usize = 200;
const MAX_COMMIT: usize = 40;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
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
    match execute(&home, &verb, now()) {
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
    Init { force: bool, settings: Settings },
    Check,
}

/// What `init` was given about the installation. The store checks that each
/// arrived and is shaped as the verb states, and stops with 78 when one did
/// not: the rules of a valid setting stay in lib/config.sh.
#[derive(Debug, PartialEq, Eq)]
pub struct Settings {
    pub version: String,
    pub commit: String,
    pub instance: String,
    pub repo: String,
}

fn parse(args: &[String]) -> Option<Verb> {
    match args {
        [verb, rest @ ..] if verb == "check" => {
            flags(rest, &["--json"])?;
            Some(Verb::Check)
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

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Runs one verb and returns the line it prints. The error is the exit status
/// and the one line for standard error.
pub fn execute(home: &Path, verb: &Verb, now: i64) -> Result<String, (u8, String)> {
    let file = db::path(home);
    match verb {
        Verb::Check => {
            // Reads only: `check` writes nothing, so it neither waits on a
            // poll's lock nor leaves a -wal beside a database it is looking at.
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
    }
}

/// `init [--force]`: schema v1 in a file of the store's own making.
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
            let sibling = side_file(file, suffix);
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

// SQLite names a database's side files by appending to its name.
fn side_file(file: &Path, side: &str) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(side);
    PathBuf::from(name)
}

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
