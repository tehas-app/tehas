//! The four shapes the two resilience tests need from outside: `__storm` is a
//! writer of the storm, `__reader` is a reader of it, `__hold` is a writer
//! killed partway through its transaction, and `__ask` is the report the tests
//! read afterwards. Each of them is one process the test spawns, which is what
//! the deliverable asks for: processes of the store, not threads of the test.
//!
//! They are here rather than in `tests/` because cargo gives an integration
//! target no path to the library of the package it belongs to: a `tests/*.rs`
//! can only be given the binary by `CARGO_BIN_EXE_store`, so the binary is what
//! has to know how to be a writer under a storm. They are marked by four words
//! no verb of section 4 is and no shell command of tehas names, they do what the
//! store always does with the file (the pragmas of section 3, the busy timeout,
//! the one transaction of a write), and they write nothing outside the store.
//! `main` answers them before `parse`, so an argument this command does not
//! know is still the 64 it has always been.
//!
//! The four of them name the store with `--home` rather than with the
//! environment, which is the one way they differ from a verb: a storm is sixteen
//! processes of one scratch home, and a test that set the variable sixteen ways
//! is a test of the environment. The home is held to the same rule `main` holds
//! `TEHAS_HOME` to -- an absolute directory or 78 -- and the file under it is
//! `db::path`, with every refusal that guard makes for every other caller.

use crate::db;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

/// The four words, as one list, so that `known` and `run` cannot disagree about
/// which arguments the binary answers.
const WORDS: [&str; 4] = ["__storm", "__reader", "__hold", "__ask"];

/// A writer of the storm's own rate: a supervisor's tick is ten seconds, so a
/// writer that writes ten times a second is the load section 3 designed for and
/// not a load made up to fill a loop.
const TICK: Duration = Duration::from_millis(100);

/// The row the killed writer puts into one table inside its transaction. A table
/// with no rows at all is reached from a page that holds nothing but its own
/// header, so a transaction that changes only such a page is a transaction whose
/// loss nothing on disk would show: the table is seeded with `KILLED_SEEDS` rows
/// of other issues first, each in its own committed transaction, and the killed
/// writer then holds the one row a reader of the file afterwards must not be able
/// to see.
const KILLED_ROWS: i64 = 1;
const KILLED_SEEDS: i64 = 400;

/// The issues the seeds of the killed writer's table hold, all of them below the
/// issue of the killed row so that the two sets do not overlap: `cycle_locks`
/// keys on the issue, and the test asks for the one number afterwards.
pub const KILLED_BASE: i64 = 80_000;

/// Whether `args` is one of the shapes this module answers.
pub fn known(args: &[String]) -> bool {
    args.first()
        .is_some_and(|word| WORDS.contains(&word.as_str()))
}

/// One process of the storm. `--home` and `--issue` are required of all three
/// and `--for` of the storm and the reader: these are the same rules as any verb
/// of the store, so the file is `$TEHAS_HOME/state/tehas.db` and never a path
/// the caller chose, and a word this shape does not know is the 64 an unknown
/// argument of the store has always been.
pub fn run(args: &[String]) -> ExitCode {
    let mut home: Option<PathBuf> = None;
    let mut issue: Option<i64> = None;
    let mut for_seconds: Option<u64> = None;
    let mut i = 1;
    while i + 1 < args.len() {
        let flag = args[i].as_str();
        let value = args[i + 1].as_str();
        // A value that starts with a dash is the next flag and not this one's
        // argument, as `parse` reads the verbs of section 4 too.
        if !flag.starts_with("--") || value.starts_with('-') {
            return ExitCode::from(64);
        }
        match (flag, value.parse::<i64>().ok()) {
            ("--home", _) => {
                let dir = PathBuf::from(value);
                // The rule `main` has for `TEHAS_HOME`, since this is the same
                // home named another way: an absolute directory, or 78.
                if !dir.is_absolute() || !dir.is_dir() {
                    eprintln!("store: --home is not set to a directory");
                    return ExitCode::from(78);
                }
                home = Some(dir);
            }
            ("--issue", Some(n)) if n > 0 => issue = Some(n),
            ("--for", Some(n)) if n > 0 => for_seconds = Some(n as u64),
            _ => return ExitCode::from(64),
        }
        i += 2;
    }
    // Once for each shape that was asked for it: the killed writer runs until it
    // is killed and the report answers at once, so neither names seconds and
    // neither wants any.
    let wanted = if args[0] == "__hold" || args[0] == "__ask" {
        2
    } else {
        3
    };
    let missing = [home.is_some(), issue.is_some(), for_seconds.is_some()]
        .into_iter()
        .take(wanted)
        .any(|given| !given);
    if missing || (wanted == 2 && for_seconds.is_some()) {
        eprintln!(
            "store: usage: store __storm|__reader --home <dir> --issue <n> --for <seconds> \
             | __hold|__ask --home <dir> --issue <n>"
        );
        return ExitCode::from(64);
    }
    let (home, issue) = (home.unwrap(), issue.unwrap());
    let file = db::path(&home);
    let answer = match args[0].as_str() {
        "__storm" => storm(&file, issue, for_seconds.unwrap())
            .map(|n| format!("{{\"api\":1,\"writes\":{n}}}")),
        "__reader" => reader(&file, issue, for_seconds.unwrap())
            .map(|n| format!("{{\"api\":1,\"reads\":{n}}}")),
        // The report the two tests read afterwards, in the shape every answer of
        // the store has.
        "__ask" => db::ask(&file, issue).map_err(crate::line),
        // The killed writer answers with the one line of `hold` and does not
        // return: it is killed inside the body of its transaction, which is the
        // whole point of it. The `Ok` arm is there because the function has a
        // type, and says so rather than pretending to print an answer.
        _ => hold(&file, issue).map(|held| {
            // The caller of `hold` never sees this: the process is killed before
            // it returns. The arm exists because the function has a type, and
            // prints the same answer `hold` already printed rather than a second
            // one, so the two cannot drift apart.
            format!("{{\"api\":1,\"verb\":\"hold\",\"ok\":true,\"held\":{held}}}")
        }),
    };
    match answer {
        Ok(fields) => {
            println!("{fields}");
            ExitCode::SUCCESS
        }
        Err((code, why)) => {
            eprintln!("store: {why}");
            ExitCode::from(code)
        }
    }
}

/// A writer of the storm: a transaction every `TICK` until the seconds are up,
/// each one `db::write`'s -- the lock, the row and the commit, and the wait for
/// a writer that holds the store at the moment. A write refused with anything
/// but the lock is the failure the test is looking for, so it is reported with
/// the word the store reports it with and the exit it exits with. The count
/// printed at the end is the writes that committed; the test counts the rows
/// afterwards and asks that the two agree.
///
/// The issue's two parents are made by the first writer that reaches the store,
/// in its own transaction and once: `foreign_keys` is ON on every connection of
/// the store, so a heartbeat is a child row, and `INSERT OR IGNORE` makes the
/// race between eight writers that all want the same parents a race in which all
/// eight win. They are rows of the store's own schema and nothing outside it.
fn storm(file: &Path, issue: i64, seconds: u64) -> Result<u64, (u8, String)> {
    let until = Instant::now() + Duration::from_secs(seconds);
    let mut writes = 0u64;
    // The parents first, in a transaction of their own and on a connection of its
    // own, by the ordinary way in which a verb of the store is written: the eight
    // writers of the storm all want the same two rows, and the one that gets there
    // first writes them while the other seven wait on the lock for them. They are
    // rows of the store's own schema and nothing outside it.
    {
        let mut seeded = db::open(file, db::Purpose::Write).map_err(crate::line)?;
        let parents = |store: &rusqlite::Connection| db::seed_heartbeat(store, issue, now_ms());
        db::write(&mut seeded, parents).map_err(crate::line)?;
    }
    // The writer's own connection, its transaction opened and closed by `write`
    // once per tick: a burst is one transaction held to its commit, which is what
    // the killed writer asks for and not what a writer that writes all the way to
    // its deadline asks for.
    let mut conn = db::open(file, db::Purpose::Write).map_err(crate::line)?;
    while Instant::now() < until {
        // One transaction per tick, as a supervisor's heartbeat is: the row is
        // written and the lock is given back, so the next writer of the storm
        // takes the store rather than eight of them holding it in turn.
        let body = |store: &rusqlite::Connection| db::heartbeat(store, issue, now_ms());
        db::write(&mut conn, body).map_err(crate::line)?;
        writes += 1;
        let next = Instant::now() + TICK;
        while Instant::now() < next {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    Ok(writes)
}

/// A reader of the storm: the count of one issue's heartbeats, asked over and
/// over until the seconds are up. Read-only as `check` is, so it never waits on
/// a writer's lock and never leaves a -wal beside a file somebody is writing. A
/// read that is refused is the failure the test is looking for too: a reader of
/// WAL mode is told `busy` only by something it should not have met.
fn reader(file: &Path, issue: i64, seconds: u64) -> Result<u64, (u8, String)> {
    let until = Instant::now() + Duration::from_secs(seconds);
    let mut reads = 0u64;
    while Instant::now() < until {
        let conn = db::open_read_only(file).map_err(crate::line)?;
        db::heartbeats(&conn, issue).map_err(crate::line)?;
        reads += 1;
    }
    Ok(reads)
}

/// The killed writer: the seeds of one table committed, the lock, one row of that
/// table inside its open transaction, the one line that says it is holding, and
/// then no word at all. The test sends SIGKILL while this is inside its
/// transaction, so the row below is written and never committed, which is the
/// partial write the database must not be left holding.
///
/// The rows are of `cycle_locks` -- the cache of what the flocks say, one row per
/// issue -- because it is the row of a store being asked whether a cycle is
/// running, and it is the row whose presence or absence a reader can be asked
/// about by number.
fn hold(file: &Path, issue: i64) -> Result<i64, (u8, String)> {
    // The seeds are committed, each in its own transaction, by the store's own
    // write: they are the rows the file has already, which the killed writer's
    // one row joins and does not reach. They are committed here rather than by a
    // connection of the test's because the store is the only writer of its file.
    let mut conn = db::open(file, db::Purpose::Write).map_err(crate::line)?;
    for seed in 0..KILLED_SEEDS {
        let body = |store: &rusqlite::Connection| db::lock_row(store, KILLED_BASE + seed, seed);
        db::write(&mut conn, body).map_err(crate::line)?;
    }
    drop(conn);

    // The killed transaction: the burst's connection and lock, the row, and no
    // commit. `written` is the count the test waits for on standard output, which
    // is what tells it the transaction is open and holding.
    let conn = db::burst(file).map_err(crate::line)?;
    for n in 0..KILLED_ROWS {
        db::lock_row(&conn, issue, n).map_err(crate::line)?;
    }
    let written = KILLED_ROWS;
    // The signal that the kill is asked for on: the lock is held, the rows of
    // the transaction are written, and nothing has been committed. The test
    // waits for this line rather than sleeping, because a kill that arrives
    // before the transaction opens proves nothing about a killed writer. It is
    // one answer of the store, on the one stream the store's answers use, and the
    // `Ok` of the caller that never returns.
    println!("{{\"api\":1,\"verb\":\"hold\",\"ok\":true,\"held\":{written}}}");
    // The rows are in the transaction, the transaction is open, and SQLite holds
    // them in the write-ahead log: the file the killed process leaves behind.
    // Nothing here commits, flushes or closes, and the process does not exit: it
    // is killed here. The sleep is the longest the standard library offers rather
    // than forever: a victim that outlives the test that kills it is a process
    // nobody meant to leave running, and a day is longer than that test by far
    // more than it needs to be.
    std::thread::sleep(Duration::from_secs(86_400));
    Ok(written)
}

/// The instant of a heartbeat's row. The value only has to be an instant and to
/// differ between the rows of one writer: the primary key of `heartbeats` is
/// (issue, at), so a writer that wrote two rows in the same millisecond would be
/// writing one row twice and the storm's count would come back short.
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}
