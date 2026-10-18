//! Ten days of factory data in SQLite: what each run did and with which
//! versions, for the dashboard's report and for a program that fetches the
//! database. All of tehas's SQL is here; the bash commands only call this.
//!
//!   analytics run <issue> [--image <id>] [--rukas-image <commit>]
//!                 [--oom <n>] [--forks <n>]
//!     Records the run of the issue that ended last: its row in runs, its
//!     steps and its roles, from what the cycle left on the host (run.rs
//!     lists the files). Call it after libexec/cycle wrote the history line.
//!     The options are what libexec/cycle read and removed before that: the
//!     image the container started from (sha256:<64 hex>), the pi-rukas
//!     commit of its label (7 to 40 hex), and how many processes were killed
//!     for memory and how many forks were refused. A value of another shape
//!     is a usage error.
//!   analytics event <issue> <event>
//!     Records an event of the issue, the first time only. The events are
//!     ready, started, ended, awaiting, merged, closed and stopped.
//!   analytics prune
//!     Deletes what is older than ten days, then runs PRAGMA optimize. run
//!     and event prune too, after their write. The events of an issue that
//!     started and never ended are kept at most a day longer.
//!   analytics export <file>
//!     Writes a consistent copy of the database to <file>, mode 0600, with
//!     VACUUM INTO. <file> must not exist.
//!
//! The database is $TEHAS_HOME/state/analytics.db, mode 0600, in WAL mode,
//! its schema version in PRAGMA user_version (schema.sql). A file that is
//! not a database is moved to analytics.db.broken, a database of another
//! schema version to analytics.db.schema-<version>, and a new one is made,
//! said on standard error. prune deletes a schema-<version> file ten days
//! after its last write. Any other failure leaves the file
//! as it is. TEHAS_HOME comes from the
//! environment and has no default.
//!
//! Exit: 0 done; 64 usage; 65 nothing to record: the issue has no history
//! line; 70 the database could not be opened or written; 73 the export file
//! exists or cannot be made; 78 TEHAS_HOME is unset or not a directory.

#[path = "../../src/mask.rs"]
#[allow(dead_code)] // the console's mask; this program uses mask() only
mod mask;

mod db;
mod run;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

const USAGE: &str = "usage: analytics run <issue> [--image <id>] [--rukas-image <commit>] [--oom <n>] [--forks <n>] | event <issue> <event> | prune | export <file>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = parse(&args) else {
        eprintln!("{USAGE}");
        return ExitCode::from(64);
    };
    let home = match std::env::var_os("TEHAS_HOME").map(PathBuf::from) {
        Some(h) if h.is_absolute() && h.is_dir() => h,
        _ => {
            eprintln!("analytics: TEHAS_HOME is not set to a directory");
            return ExitCode::from(78);
        }
    };
    match execute(&home, &cmd, now()) {
        Ok(()) => ExitCode::SUCCESS,
        Err((code, why)) => {
            eprintln!("analytics: {why}");
            ExitCode::from(code)
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Cmd {
    Run(i64, run::Given),
    Event(i64, String),
    Prune,
    Export(PathBuf),
}

fn parse(args: &[String]) -> Option<Cmd> {
    let issue = |s: &str| {
        (!s.starts_with('0') && s.len() <= 9 && s.bytes().all(|b| b.is_ascii_digit()))
            .then(|| s.parse::<i64>().ok())
            .flatten()
    };
    match args {
        [c, n, rest @ ..] if c == "run" => Some(Cmd::Run(issue(n)?, given(rest)?)),
        [c, n, e] if c == "event" && db::EVENTS.contains(&e.as_str()) => {
            Some(Cmd::Event(issue(n)?, e.clone()))
        }
        [c] if c == "prune" => Some(Cmd::Prune),
        [c, f] if c == "export" && !f.is_empty() => Some(Cmd::Export(PathBuf::from(f))),
        _ => None,
    }
}

// The options of run, each at most once.
fn given(args: &[String]) -> Option<run::Given> {
    let count = |s: &str| {
        (s == "0" || (!s.starts_with('0') && s.len() <= 9 && s.bytes().all(|b| b.is_ascii_digit())))
            .then(|| s.parse::<i64>().ok())
            .flatten()
    };
    let mut g = run::Given::default();
    for pair in args.chunks(2) {
        let [flag, value] = pair else { return None };
        let v = value.as_str();
        match flag.as_str() {
            "--image" if g.image.is_none() => {
                let hex = v.strip_prefix("sha256:")?;
                run::is_hex(hex, 64, 64).then_some(())?;
                g.image = Some(v.to_string());
            }
            "--rukas-image" if g.rukas_image.is_none() => {
                run::is_hex(v, 7, 40).then_some(())?;
                g.rukas_image = Some(v.to_string());
            }
            "--oom" if g.oom.is_none() => g.oom = Some(count(v)?),
            "--forks" if g.forks.is_none() => g.forks = Some(count(v)?),
            _ => return None,
        }
    }
    Some(g)
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

pub fn execute(home: &Path, cmd: &Cmd, now: i64) -> Result<(), (u8, String)> {
    let db_err = |e: rusqlite::Error| (70, format!("the database could not be written: {e}"));
    // The run is read before the database is touched: nothing to record
    // leaves the file as it was.
    let run = match cmd {
        Cmd::Run(issue, given) => Some(run::read(home, *issue, given).map_err(|e| (65, e))?),
        _ => None,
    };
    if let Cmd::Export(to) = cmd {
        if to.exists() || to.is_symlink() {
            return Err((73, format!("{} exists already", to.display())));
        }
    }
    let file = db::path(home);
    let (conn, moved) = db::open(&file, now).map_err(|e| (70, e))?;
    if let Some(to) = moved {
        eprintln!(
            "analytics: {} was not a database of schema {}; it was moved to {} and a new one made",
            file.display(),
            db::SCHEMA_VERSION,
            to.display()
        );
    }
    match cmd {
        Cmd::Run(..) => {
            if let Some(run) = &run {
                run::insert(&conn, run).map_err(db_err)?;
            }
            db::prune(&conn, now).map_err(db_err)
        }
        Cmd::Event(issue, event) => {
            db::event(&conn, *issue, event, now).map_err(db_err)?;
            db::prune(&conn, now).map_err(db_err)
        }
        Cmd::Prune => {
            db::prune(&conn, now).map_err(db_err)?;
            db::prune_aside(&file, now);
            conn.execute_batch("PRAGMA optimize").map_err(db_err)
        }
        Cmd::Export(to) => db::export(&conn, to).map_err(|e| (73, e)),
    }
}
