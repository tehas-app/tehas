//! A year of factory data in SQLite: what each run did and with which
//! versions, for the dashboard's report and for a program that fetches the
//! database. All of tehas's SQL is here; the bash commands only call this.
//!
//!   analytics run <issue> --facts <file> [--image <id>] [--image-built <time>]
//!                 [--rukas-image <commit>] [--oom <n>] [--forks <n>]
//!     Records the run of the issue that ended last: its row in runs, its
//!     steps and its roles, from the facts its caller wrote to <file> (the
//!     history line, the attempt, the approval, the settings and the image's
//!     commit) and what the cycle left on the host (run.rs lists both). Call
//!     it after libexec/cycle wrote the history line. The other options are
//!     what libexec/cycle read and removed before that: the
//!     image the container started from (sha256:<64 hex>), the pi-rukas
//!     commit of its label (7 to 40 hex), and how many processes were killed
//!     for memory and how many forks were refused. A value of another shape
//!     is a usage error. --image-built is when the image was built, as docker
//!     prints .Created; one that cannot be read is left out.
//!   analytics event <issue> <event> [--at <epoch seconds>]
//!     Records an event of the issue, the first time only. The events are
//!     ready, started, ended, awaiting, merged, closed and stopped. --at is
//!     when it happened, now when not given: the dispatcher records ready at
//!     the start, with the time tehas:ready was accepted. A time more than a
//!     minute ahead of now is a usage error.
//!   analytics plan <issue> --rc <rc> --facts <file> [--rukas-image <commit>]
//!                  [--image <id>] [--image-built <time>]
//!     Records the planning turn of the issue that ended: its row in runs,
//!     kind plan, from its record, which its caller wrote to <file>, and the
//!     turn's output (plan.rs lists both), with no usage.
//!     lib/planning.sh calls it as plan_finish begins, before the record is
//!     rewritten. <rc> is how the turn's unit ended: its exit status, or
//!     interrupted, not-started or not-readable. The commit, id and build
//!     time are those of the sandbox image as the dispatcher started the
//!     turn. A turn started before it noted them has the commit of the image
//!     as the turn ended, and no id. A turn recorded again keeps its first
//!     row.
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
//!     live/analytics.json. Among the figures, `gate`: report.rs's `gate`
//!     says what it holds, and libexec/status publishes no key it does not
//!     name, which `gate` is not.
//!   analytics orders
//!     Writes live/orders.json: the runs, seconds and tokens of each issue
//!     live/status.json lists, for the front page's work order (orders.rs
//!     says what is in it). libexec/status asks for it after it writes the
//!     snapshot.
//!   analytics sessions [--out <directory>]
//!     Prints the runs of the last 30 days as JSON Lines, for the MCP server
//!     (sessions.rs lists the keys): a header line, then one session per
//!     line, newest first, each with its steps, roles and log file names.
//!     It writes nothing; a failure cause is as the run stored it, masked.
//!     With --out the lines go to <directory>/sessions.jsonl instead, mode
//!     600 and moved in from a name of this run, which is how libexec/status
//!     asks for the file the MCP server reads.
//!   analytics outcomes [--tehas <version>] [--rukas <version>]
//!                 [--since <day or time>] [--until <day or time>]
//!                 [--by version|week] [--out <directory>]
//!     Prints the counts behind the MCP tool factory_outcomes as JSON Lines
//!     (outcomes.rs lists the keys and what stays out of them): a header
//!     line, then one entry per line, for as far back as the database holds.
//!     A run is counted on its end, a run that never ended on its start.
//!     --tehas and --rukas name a version exactly and group with it, so under
//!     --by week they answer to nothing. --since and --until name a day
//!     (YYYY-MM-DD, its start) or a full UTC time, --by version or week
//!     (the default). One of another shape is a usage error. With --out the
//!     lines go to <directory>/outcomes.jsonl instead, mode 600 and moved in
//!     from a name of this run, which is how libexec/status asks for the file
//!     the MCP server reads.
//!   analytics export <file> [--json]
//!     Writes a consistent copy of the database to <file>, mode 0600, with
//!     VACUUM INTO. <file> must not exist. With --json it then prints what
//!     the copy holds, read from the copy: {"api":1,"schema":N,"from":T,
//!     "to":T,"runs":N}, from and to the oldest start and the newest end of
//!     its runs, both left out when it has none.
//!
//! The database is $TEHAS_HOME/state/analytics.db, mode 0600, in WAL mode,
//! its schema version in PRAGMA user_version (schema.sql). A database of
//! schema 2 is migrated in place. A file that is not a database is moved to
//! analytics.db.broken, a database of another schema version to
//! analytics.db.schema-<version>, and a new one is made, said on standard error. prune deletes a schema-<version> file 365 days
//! after its last write. Any other failure leaves the file
//! as it is. TEHAS_HOME comes from the
//! environment and has no default.
//!
//! A facts file is read as a host file is: a regular file, not through a
//! link, at most 16 MiB of one JSON value. One that is not is no facts.
//!
//! Exit: 0 done; 64 usage; 65 nothing to record: no facts, facts with no
//! history line, or no planning turn in flight; 70 the database could not be opened, written or read; 73 the export file
//! exists or cannot be made; 78 TEHAS_HOME is unset or not a directory.

mod db;
mod orders;
mod outcomes;
mod plan;
mod publish;
mod report;
mod run;
mod sample;
mod sessions;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use tehas_core::grammar::{count, is_hex, is_image};
use tehas_core::time::now;

/// The rule one state ledger is read under, held once for both readers: bytes
/// of the whole file, and bytes of one line. `report` and `sessions` read the
/// ledgers beside the database, and a file one of them reads is the same file
/// the other does. Both writers keep 500 lines of a few small counts --
/// `tehas_size_record` trims `sizes.jsonl` and `plan_refined` trims
/// `refined.jsonl` -- so no ledger of their own making comes near the bound.
pub const LEDGER_MAX_BYTES: u64 = 1 << 20;
pub const LEDGER_MAX_LINE: usize = 100_000;

const USAGE: &str = "usage: analytics run <issue> --facts <file> [--image <id>] [--image-built <time>] [--rukas-image <commit>] [--oom <n>] [--forks <n>] | plan <issue> --rc <rc> --facts <file> [--rukas-image <commit>] [--image <id>] [--image-built <time>] | event <issue> <event> [--at <epoch seconds>] | sample | prune | report [--json] | orders | sessions [--out <directory>] | outcomes [--tehas <version>] [--rukas <version>] [--since <day or time>] [--until <day or time>] [--by version|week] [--out <directory>] | export <file> [--json]";

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
    Orders,
    Sessions(Option<PathBuf>),
    Outcomes(outcomes::Filter, Option<PathBuf>),
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
        [c] if c == "orders" => Some(Cmd::Orders),
        [c, rest @ ..] if c == "sessions" => sessions_command(rest),
        [c, rest @ ..] if c == "outcomes" => outcome_command(rest),
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

// The one option of sessions: `--out <directory>`, which writes the snapshot
// there instead of printing it. Without it, or with any other option, the
// plain command: print.
fn sessions_command(args: &[String]) -> Option<Cmd> {
    match args {
        [] => Some(Cmd::Sessions(None)),
        [flag, dir] if flag == "--out" => facts(dir).map(|d| Cmd::Sessions(Some(d))),
        _ => None,
    }
}

// The options of outcomes: each at most once, all optional, each with its
// value next to it. A value of another shape is no command, so the file never
// holds a bound the caller did not mean. `--out <directory>` writes the file
// there instead of printing it, which is how the host asks for it.
fn outcome_command(args: &[String]) -> Option<Cmd> {
    // The options, named once: the shape of each is read from the pair that
    // carries it, so an option added here needs no second list to keep.
    const OPTIONS: [&str; 6] = ["--tehas", "--rukas", "--since", "--until", "--by", "--out"];
    let mut given: Vec<(&str, &str)> = Vec::new();
    let mut rest = args.iter();
    while let Some(flag) = rest.next() {
        // An unknown option is no command, and a known one needs its value
        // next to it: `--tehas` with nothing after it says nothing of a
        // version, which is not what an option of this command may mean.
        let known = OPTIONS.contains(&flag.as_str());
        known.then_some(())?;
        let value = rest.next()?;
        // A value of another option's shape is no value: `--tehas --rukas`
        // would silently ask for a version called --rukas. The path of --out
        // is read as `--facts` is, and refused the same way.
        (!value.is_empty() && !value.starts_with("--")).then_some(())?;
        // Which of its two values was meant cannot be told, so an option
        // given twice is no command at all.
        if given.iter().any(|(f, _)| *f == flag) {
            return None;
        }
        given.push((flag.as_str(), value.as_str()));
    }
    // None when the option was not given, `Some(None)` when it was given and
    // has no value, `Some(Some(v))` once with its value. The shape of a value
    // is the filter's to decide, and one it cannot read means the caller asked
    // for something the file cannot hold: no command, rather than a file of no
    // counts.
    let value = |name: &str| {
        let asked = given.iter().filter(|(f, _)| *f == name).count();
        match asked {
            0 => None,
            1 => given
                .iter()
                .find(|(f, _)| *f == name)
                .map(|(_, v)| Some(*v)),
            _ => Some(None),
        }
    };
    let f = outcomes::filter(
        value("--tehas"),
        value("--rukas"),
        value("--since"),
        value("--until"),
        value("--by").flatten().unwrap_or("version"),
    )?;
    Some(match value("--out").flatten().and_then(facts) {
        None => Cmd::Outcomes(f, None),
        Some(dir) => Cmd::Outcomes(f, Some(dir)),
    })
}

// Epoch seconds: digits, no leading zero, at most twelve of them.
fn epoch(s: &str) -> Option<i64> {
    (s == "0" || (!s.starts_with('0') && s.len() <= 12 && s.bytes().all(|b| b.is_ascii_digit())))
        .then(|| s.parse::<i64>().ok())
        .flatten()
}

/// How far ahead of now an event's --at may be: clocks differ a little.
const AT_AHEAD: i64 = 60;

// The path of --facts: not empty and not an option.
fn facts(v: &str) -> Option<PathBuf> {
    (!v.is_empty() && !v.starts_with('-')).then(|| PathBuf::from(v))
}

// The options of run, each at most once, --facts required.
fn given(args: &[String]) -> Option<run::Given> {
    let mut g = run::Given::default();
    let mut built_seen = false;
    for pair in args.chunks(2) {
        let [flag, value] = pair else { return None };
        let v = value.as_str();
        match flag.as_str() {
            "--image" if g.image.is_none() => {
                is_image(v).then_some(())?;
                g.image = Some(v.to_string());
            }
            "--rukas-image" if g.rukas_image.is_none() => {
                is_hex(v, 7, 40).then_some(())?;
                g.rukas_image = Some(v.to_string());
            }
            "--image-built" if !built_seen => {
                built_seen = true;
                g.image_built = built(v);
            }
            "--oom" if g.oom.is_none() => g.oom = Some(count(v)?),
            "--forks" if g.forks.is_none() => g.forks = Some(count(v)?),
            "--facts" if g.facts.is_none() => g.facts = Some(facts(v)?),
            _ => return None,
        }
    }
    g.facts.as_ref()?;
    Some(g)
}

// The image's build time as docker prints it (.Created). One that cannot be
// read is left out, not refused: the run is recorded without it.
fn built(v: &str) -> Option<i64> {
    (v.len() <= 40)
        .then(|| run::iso_ms(v))
        .flatten()
        .map(|ms| ms / 1000)
}

// The options of plan: --rc and --facts once and required, the others at most
// once.
fn plan_given(args: &[String]) -> Option<plan::Given> {
    let mut rc: Option<String> = None;
    let mut g = plan::Given::default();
    let mut built_seen = false;
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
                is_hex(v, 7, 40).then_some(())?;
                g.rukas_image = Some(v.to_string());
            }
            "--image" if g.image.is_none() => {
                is_image(v).then_some(())?;
                g.image = Some(v.to_string());
            }
            "--image-built" if !built_seen => {
                built_seen = true;
                g.image_built = built(v);
            }
            "--facts" if g.facts.is_none() => g.facts = Some(facts(v)?),
            _ => return None,
        }
    }
    g.facts.as_ref()?;
    g.rc = rc?;
    Some(g)
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
            let report = report::complete(&conn, home, now)?;
            if matches!(cmd, Cmd::Report) {
                println!("{report}");
            } else {
                print!("{}", report::text(&report));
            }
            Ok(())
        }
        Cmd::Orders => orders::write(&home.join("live"), &conn, now),
        Cmd::Sessions(to) => {
            let lines = sessions::lines(&conn, home, now)
                .map_err(|e| (70, format!("the sessions could not be read: {e}")))?;
            match to {
                None => println!("{}", lines.join("\n")),
                // The host's snapshot: written here, so the host runs no
                // writer of its own and reads the file no second time.
                Some(dir) => sessions::write(dir, &lines).map_err(|e| (73, e))?,
            }
            Ok(())
        }
        Cmd::Outcomes(f, to) => {
            let lines = outcomes::lines(&conn, now, f)
                .map_err(|e| (70, format!("the outcomes could not be read: {e}")))?;
            match to {
                None => println!("{}", lines.join("\n")),
                // The host's file: written here, so what the reader checks is
                // what this wrote, and the host runs no second reader of it.
                Some(dir) => outcomes::write(dir, &conn, now, f).map_err(|e| (73, e))?,
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
