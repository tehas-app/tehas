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
//!   analytics event <issue> <event> [--at <epoch seconds>]
//!     Records an event of the issue, the first time only. The events are
//!     ready, started, ended, awaiting, merged, closed and stopped. --at is
//!     when it happened, now when not given: the dispatcher records ready at
//!     the start, with the time tehas:ready was accepted. A time more than a
//!     minute ahead of now is a usage error.
//!   analytics plan <issue> --rc <rc> [--rukas-image <commit>]
//!     Records the planning turn of the issue that ended: its row in runs,
//!     kind plan, from its record (plan.rs lists the files), with no usage.
//!     lib/planning.sh calls it as plan_finish begins, before the record is
//!     rewritten. <rc> is how the turn's unit ended: its exit status, or
//!     interrupted, not-started or not-readable. The commit is that of the
//!     sandbox image's label. A turn recorded again keeps its first row.
//!   analytics sample
//!     Records one sample of the factory: the issues in line, running,
//!     awaiting a merge and stopped, the cycles allowed, the pause, the hold
//!     and the mesh's draw, from the live files the poll wrote (sample.rs
//!     lists them). The dispatcher calls it once a poll.
//!   analytics prune
//!     Deletes what is older than 365 days, then runs PRAGMA optimize. run,
//!     event and sample prune too, after their write. The events of an
//!     issue that started and never ended are kept at most a day longer.
//!   analytics report [--json]
//!     Prints the ten-day report the dashboard shows, aggregates only
//!     (report.rs says what is in it): as plain text for a person, or with
//!     --json as one JSON object, which libexec/status writes to
//!     live/analytics.json.
//!   analytics export <file> [--json]
//!     Writes a consistent copy of the database to <file>, mode 0600, with
//!     VACUUM INTO. <file> must not exist. With --json it then prints what
//!     the copy holds, read from the copy: {"api":1,"schema":N,"from":T,
//!     "to":T,"runs":N}, from and to the oldest start and the newest end of
//!     its runs, both left out when it has none.
//!
//! The database is $TEHAS_HOME/state/analytics.db, mode 0600, in WAL mode,
//! its schema version in PRAGMA user_version (schema.sql). A file that is
//! not a database is moved to analytics.db.broken, a database of another
//! schema version to analytics.db.schema-<version>, and a new one is made,
//! said on standard error. prune deletes a schema-<version> file 365 days
//! after its last write. Any other failure leaves the file
//! as it is. TEHAS_HOME comes from the
//! environment and has no default.
//!
//! Exit: 0 done; 64 usage; 65 nothing to record: the issue has no history
//! line, or no planning turn in flight; 70 the database could not be opened, written or read; 73 the export file
//! exists or cannot be made; 78 TEHAS_HOME is unset or not a directory.

#[path = "../../src/mask.rs"]
#[allow(dead_code)] // the console's mask; this program uses mask() only
mod mask;

mod db;
mod plan;
mod report;
mod run;
mod sample;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

const USAGE: &str = "usage: analytics run <issue> [--image <id>] [--rukas-image <commit>] [--oom <n>] [--forks <n>] | plan <issue> --rc <rc> [--rukas-image <commit>] | event <issue> <event> [--at <epoch seconds>] | sample | prune | report [--json] | export <file> [--json]";

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
    Plan(i64, plan::Given),
    Event(i64, String, Option<i64>),
    Sample,
    Prune,
    Report,
    ReportText,
    Export(PathBuf, bool),
}

fn parse(args: &[String]) -> Option<Cmd> {
    let issue = |s: &str| {
        (!s.starts_with('0') && s.len() <= 9 && s.bytes().all(|b| b.is_ascii_digit()))
            .then(|| s.parse::<i64>().ok())
            .flatten()
    };
    match args {
        [c, n, rest @ ..] if c == "run" => Some(Cmd::Run(issue(n)?, given(rest)?)),
        [c, n, rest @ ..] if c == "plan" => Some(Cmd::Plan(issue(n)?, plan_given(rest)?)),
        [c, n, e, rest @ ..] if c == "event" && db::EVENTS.contains(&e.as_str()) => {
            let at = match rest {
                [] => None,
                [flag, t] if flag == "--at" => Some(epoch(t)?),
                _ => return None,
            };
            Some(Cmd::Event(issue(n)?, e.clone(), at))
        }
        [c] if c == "sample" => Some(Cmd::Sample),
        [c] if c == "prune" => Some(Cmd::Prune),
        [c, f] if c == "report" && f == "--json" => Some(Cmd::Report),
        [c] if c == "report" => Some(Cmd::ReportText),
        [c, f, rest @ ..] if c == "export" && !f.is_empty() && !f.starts_with('-') => {
            let json = match rest {
                [] => false,
                [j] if j == "--json" => true,
                _ => return None,
            };
            Some(Cmd::Export(PathBuf::from(f), json))
        }
        _ => None,
    }
}

// Epoch seconds: digits, no leading zero, at most twelve of them.
fn epoch(s: &str) -> Option<i64> {
    (s == "0" || (!s.starts_with('0') && s.len() <= 12 && s.bytes().all(|b| b.is_ascii_digit())))
        .then(|| s.parse::<i64>().ok())
        .flatten()
}

/// How far ahead of now an event's --at may be: clocks differ a little.
const AT_AHEAD: i64 = 60;

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

// The options of plan: --rc once and required, --rukas-image at most once.
fn plan_given(args: &[String]) -> Option<plan::Given> {
    let mut rc: Option<String> = None;
    let mut g = plan::Given::default();
    for pair in args.chunks(2) {
        let [flag, value] = pair else { return None };
        let v = value.as_str();
        match flag.as_str() {
            "--rc" if rc.is_none() => {
                let code = v == "0"
                    || (!v.starts_with('0')
                        && v.len() <= 3
                        && v.bytes().all(|b| b.is_ascii_digit()));
                (code || plan::RC_WORDS.contains(&v)).then_some(())?;
                rc = Some(v.to_string());
            }
            "--rukas-image" if g.rukas_image.is_none() => {
                run::is_hex(v, 7, 40).then_some(())?;
                g.rukas_image = Some(v.to_string());
            }
            _ => return None,
        }
    }
    g.rc = rc?;
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
        Cmd::Plan(issue, given) => Some(plan::read(home, *issue, given, now).map_err(|e| (65, e))?),
        _ => None,
    };
    if let Cmd::Export(to, _) = cmd {
        if to.exists() || to.is_symlink() {
            return Err((73, format!("{} exists already", to.display())));
        }
    }
    if let Cmd::Event(_, _, Some(at)) = cmd {
        if *at > now + AT_AHEAD {
            return Err((
                64,
                format!("--at {at} is more than {AT_AHEAD}s ahead of now"),
            ));
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
        Cmd::Plan(..) => {
            if let Some(run) = &run {
                plan::insert(&conn, run).map_err(db_err)?;
            }
            db::prune(&conn, now).map_err(db_err)
        }
        Cmd::Event(issue, event, at) => {
            db::event(&conn, *issue, event, at.unwrap_or(now)).map_err(db_err)?;
            db::prune(&conn, now).map_err(db_err)
        }
        Cmd::Sample => {
            sample::insert(&conn, now, &sample::read(home, now)).map_err(db_err)?;
            db::prune(&conn, now).map_err(db_err)
        }
        Cmd::Prune => {
            db::prune(&conn, now).map_err(db_err)?;
            db::prune_aside(&file, now);
            conn.execute_batch("PRAGMA optimize").map_err(db_err)
        }
        Cmd::Report | Cmd::ReportText => {
            let report =
                report::build(&conn, now).map_err(|e| (70, format!("the report failed: {e}")))?;
            if matches!(cmd, Cmd::Report) {
                println!("{report}");
            } else {
                print!("{}", report::text(&report));
            }
            Ok(())
        }
        Cmd::Export(to, json) => {
            db::export(&conn, to).map_err(|e| (73, e))?;
            if *json {
                println!("{}", db::copy_summary(to).map_err(|e| (70, e))?);
            }
            Ok(())
        }
    }
}
