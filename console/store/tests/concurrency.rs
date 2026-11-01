//! The store as the host's commands will call it, many processes at once:
//! eight writers and eight readers for ten seconds, and a writer killed with
//! SIGKILL while its transaction is open. The readers are `store check`. The
//! writers are `store test-write`, because no verb of this part of the store
//! writes rows; only a build with debug assertions has it (src/exercise.rs).

// The writer verb exists only in debug builds, so a release test run skips this file.
#![cfg(debug_assertions)]

use rusqlite::{Connection, ErrorCode, OpenFlags};
use std::collections::BTreeSet;
use std::io::{BufRead, BufReader};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

const STORE: &str = env!("CARGO_BIN_EXE_store");
const WRITERS: u32 = 8;
const READERS: u32 = 8;
const LOAD: Duration = Duration::from_secs(2);
/// Calls each process of the load must get through. The load runs past `LOAD`
/// until all have, so a machine that lends it little time cannot fail the case.
const FLOOR: usize = 5;
/// Past this a process still short of `FLOOR` is starved, whatever the machine.
const STARVED: Duration = Duration::from_secs(120);
/// The store's notice of a wait on a lock: one line for the first look at it,
/// on standard error, given to a build with debug assertions that was asked for
/// it by the variable of `src/db.rs`. That line is what says a call sat on the
/// store, and nothing else says it: a call that took a second because the rest
/// of the gate had every CPU busy did not wait for anything, and no time
/// measured around a child process tells the two apart. A call that met no lock
/// prints nothing at all, so the healthy case costs no line and no wait. What
/// `met_lock` counts is these lines.
const RETRY: &str = "store debug: busy retry 0";
/// The waits of one call that the load is allowed to cause. Eight writers
/// opening one database at once do arrive at its lock together, and the one
/// SQLite turns away is asked again a millisecond later: in load after load the
/// most any call met was one such wait, and no wait of it came near the five
/// seconds of a bound -- a hundred polls is one twentieth of that. What the
/// store cannot be asked to sit out is another process's transaction, so the
/// bound is roomy rather than tight: a writer past it, or a reader past
/// `READER_WAITS`, met something the load does not explain, and that is the
/// failure this test is for.
/// `a_call_kept_out_by_a_held_lock_reports_its_wait` is the case that shows a
/// wait held out of the store is reported at all.
const LOAD_WAITS: usize = 100;
/// The waits of one reader's call. Load after load has shown one at most.
const READER_WAITS: usize = 2;
const ROWS_PER_WRITE: u32 = 3;

/// A home with a store `init` made, as install.sh will leave it.
fn home(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("store-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = run(
        &dir,
        &[
            "init",
            "--version",
            "v0.10.2",
            "--commit",
            "cb6912a",
            "--instance",
            "tehas-tehas",
            "--repo",
            "example-owner/example-product",
        ],
    );
    assert!(out.status.success(), "init: {}", said(&out));
    dir
}

fn store(home: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(STORE);
    cmd.args(args)
        .env_clear()
        .env("TEHAS_HOME", home)
        // One line per wait on a lock, counted by `met_lock`. The variable is
        // read by a build with debug assertions only, and this file is not built
        // for a release one, so the two are always a pair.
        .env("TEHAS_STORE_TEST_BUSY_REPORT", "1");
    cmd
}

/// How many times a call of the store was kept waiting for a lock another
/// process held: the store says so itself, and an elapsed time says nothing.
fn met_lock(out: &Output) -> usize {
    String::from_utf8_lossy(&out.stderr)
        .lines()
        .filter(|line| *line == RETRY)
        .count()
}

/// The lock of another connection, held until the returned guard is dropped:
/// the one lock that keeps every other connection out, readers included. A
/// writer's transaction does not: WAL lets a reader go on, so a holder of that
/// alone would be waited for by a writer and not by a reader, and the two cases
/// of this file would need two holders. `busy_timeout` is 0 so the holder itself
/// never waits for anything.
struct Holder {
    conn: Connection,
}

impl Drop for Holder {
    fn drop(&mut self) {
        // The transaction ends before the connection goes, and the -wal with
        // it: a reader of the next case sees the store as it was before.
        let _ = self
            .conn
            .execute_batch("COMMIT; PRAGMA wal_checkpoint(TRUNCATE);");
    }
}

fn held_lock(home: &Path, name: &str) -> Holder {
    let conn = Connection::open(db(home)).unwrap();
    conn.busy_timeout(Duration::ZERO).unwrap();
    conn.execute_batch("PRAGMA locking_mode = EXCLUSIVE; BEGIN EXCLUSIVE;")
        .unwrap();
    conn.execute(
        "INSERT INTO branch_checks (name, head_sha, at, state) VALUES (?1, '0', 1, 'pending')",
        [name],
    )
    .unwrap();
    Holder { conn }
}

/// What the store was kept out of, and by whom: the process that waited, how
/// many waits it named, what it was asked for, and what it answered with. The
/// last two are what tell a wait the load causes and is allowed from one that is
/// the store's own trouble: see `load_allows`.
#[derive(Clone)]
struct Waited {
    who: String,
    waits: usize,
    args: Vec<String>,
    said: String,
}

/// Whether a wait that answered ok is the load's own doing rather than a
/// finding. See `LOAD_WAITS` for the figure and what it leaves room for. WAL
/// lets a reader go on past a writer, so a `check` waits only on SQLite's brief
/// exclusive steps, which this load does cause: each process closes the last
/// connection now and then, and that one checkpoints. See `READ_WAIT` in
/// `src/db.rs`. `READER_WAITS` is that and no more.
fn load_allows(w: &Waited) -> bool {
    let bound = if w.args.first().is_some_and(|verb| verb == "test-write") {
        LOAD_WAITS
    } else {
        READER_WAITS
    };
    w.waits <= bound
}

fn run(home: &Path, args: &[&str]) -> Output {
    store(home, args).output().unwrap()
}

fn said(out: &Output) -> String {
    format!(
        "exit {:?}, stdout {:?}, stderr {:?}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn db(home: &Path) -> PathBuf {
    home.join("state").join("tehas.db")
}

/// The rows of branch_checks by name and head, read by this process. The
/// reader asks for nothing: by now nothing holds the file, and a helper that
/// waited for a lock would hide the very failure a case is checking for.
fn rows(home: &Path) -> BTreeSet<(String, String)> {
    let conn = Connection::open_with_flags(db(home), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    conn.busy_timeout(Duration::ZERO).unwrap();
    let mut q = conn
        .prepare("SELECT name, head_sha FROM branch_checks")
        .unwrap();
    let all = q
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    all
}

fn expected(batch: u32, rows: u32) -> impl Iterator<Item = (String, String)> {
    (0..rows).map(move |row| (format!("test-write-{batch}"), row.to_string()))
}

fn load_goes_on(end: Instant, starved: Instant, short: &AtomicU32) -> bool {
    let now = Instant::now();
    now < starved && (now < end || short.load(Ordering::SeqCst) > 0)
}

#[test]
fn eight_writers_and_eight_readers_meet_no_busy_and_lose_no_write() {
    let home = home("load");
    let end = Instant::now() + LOAD;
    let starved = Instant::now() + STARVED;
    let short: &'static AtomicU32 = Box::leak(Box::new(AtomicU32::new(WRITERS + READERS)));
    // What any of the sixteen processes was kept out of the store for, named so
    // a failure says who and with what answer. The handles are collected before
    // the join, so the closures are alive by the time their lifetimes are
    // checked and a borrow of a local here cannot serve: the vector is leaked and
    // shared by pointer, and dropped with the process.
    let waited: &'static Mutex<Vec<Waited>> = Box::leak(Box::new(Mutex::new(Vec::new())));
    let note = |who: String, waits: usize, args: &[&str], out: &Output| {
        waited.lock().unwrap().push(Waited {
            who,
            waits,
            args: args.iter().map(|a| a.to_string()).collect(),
            said: said(out),
        })
    };
    let writers: Vec<_> = (0..WRITERS)
        .map(|w| {
            let home = home.clone();
            thread::spawn(move || {
                let (mut written, mut failed) = (Vec::new(), Vec::new());
                let mut n = 0;
                while load_goes_on(end, starved, short) {
                    let batch = w * 1_000_000 + n;
                    let asked = Instant::now();
                    let args = [
                        "test-write",
                        "--batch",
                        &batch.to_string(),
                        "--rows",
                        &ROWS_PER_WRITE.to_string(),
                    ];
                    let out = run(&home, &args);
                    if out.status.success() {
                        written.push(batch);
                        if written.len() == FLOOR {
                            short.fetch_sub(1, Ordering::SeqCst);
                        }
                        let waits = met_lock(&out);
                        if waits > 0 {
                            note(format!("writer {w}, batch {batch}"), waits, &args, &out);
                        }
                    } else {
                        failed.push(format!(
                            "writer {w}, batch {batch}, {:?}: {}",
                            asked.elapsed(),
                            said(&out)
                        ));
                    }
                    n += 1;
                }
                (written, failed)
            })
        })
        .collect();
    let readers: Vec<_> = (0..READERS)
        .map(|r| {
            let home = home.clone();
            thread::spawn(move || {
                let (mut reads, mut failed) = (0, Vec::new());
                while load_goes_on(end, starved, short) {
                    let args = ["check"];
                    let out = run(&home, &args);
                    if out.status.success() {
                        reads += 1;
                        if reads == FLOOR {
                            short.fetch_sub(1, Ordering::SeqCst);
                        }
                        let waits = met_lock(&out);
                        if waits > 0 {
                            note(format!("reader {r}"), waits, &args, &out);
                        }
                    } else {
                        failed.push(format!("reader {r}: {}", said(&out)));
                    }
                }
                (reads, failed)
            })
        })
        .collect();

    let mut failed = Vec::new();
    let mut want = BTreeSet::new();
    let mut fewest = (usize::MAX, usize::MAX);
    for t in writers {
        let (written, f) = t.join().unwrap();
        fewest.0 = fewest.0.min(written.len());
        for batch in written {
            want.extend(expected(batch, ROWS_PER_WRITE));
        }
        failed.extend(f);
    }
    for t in readers {
        let (reads, f) = t.join().unwrap();
        fewest.1 = fewest.1.min(reads);
        failed.extend(f);
    }
    // A busy database is exit 1 with the word on stderr, so a call that met
    // SQLITE_BUSY to the end of its bound is a failed call here. Nothing is
    // allowed to have waited either: the store's handler is a five-second wait
    // even in this debug build -- the tests' short pair belongs to the unit
    // tests, not to these child processes -- so a call that had to sit out
    // another's transaction would reach `waited` or `failed`. What it does not
    // need is a clock: the store named every wait itself, so a machine busy with
    // the rest of the gate costs this test nothing it has to be forgiven for.
    assert!(failed.is_empty(), "{} calls failed:\n{}", failed.len(), {
        failed.truncate(10);
        failed.join("\n")
    });
    let all = waited.lock().unwrap().clone();
    let waited: Vec<&Waited> = all.iter().filter(|w| !load_allows(w)).collect();
    assert!(
        waited.is_empty(),
        "{} calls waited on the store:\n{}",
        waited.len(),
        {
            let shown: Vec<String> = waited
                .iter()
                .take(10)
                .map(|w| {
                    format!(
                        "{}: {} waits, asked {:?}, answered {}",
                        w.who, w.waits, w.args, w.said
                    )
                })
                .collect();
            shown.join("\n")
        }
    );
    eprintln!(
        "{} rows written; fewest writes, reads per process: {fewest:?}; {} calls of the load met the lock, at most {} waits in one of them",
        want.len(),
        all.len(),
        all.iter().map(|w| w.waits).max().unwrap_or(0)
    );
    // Every writer and every reader had its turn many times over.
    assert!(
        fewest.0 >= FLOOR && fewest.1 >= FLOOR,
        "fewest writes, reads: {fewest:?}"
    );
    assert_eq!(rows(&home), want, "every write that answered 0 is there");
    let out = run(&home, &["check"]);
    assert!(out.status.success(), "check: {}", said(&out));
    assert!(String::from_utf8_lossy(&out.stdout).contains("\"integrity\":\"ok\""));
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_call_kept_out_by_a_held_lock_reports_its_wait() {
    // The other side of `met_lock`, and the reason the load test above can trust
    // it: a call that really is kept out of the store says so. Without this case
    // a notice that never prints -- a renamed word, a variable nobody reads --
    // would make the load test pass on a store that was nothing but locks, which
    // is the failure it exists to catch. Both kinds of call are asked: the writer
    // for the load, the reader because `check` is what the load runs beside it.
    let held = home("held-lock");
    let out = run(&held, &["test-write", "--batch", "1", "--rows", "1"]);
    assert!(out.status.success(), "{}", said(&out));
    let holder = held_lock(&held, "held");
    // The holder lets go two seconds after it is taken, and both calls below are
    // already waiting by then, so each is a wait that was waited out rather than
    // met to its end: the call writes, or reads, instead of answering the `busy`
    // that says a bound ran out. Two seconds is well inside either bound and
    // well outside the moment the calls take to start.
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(2));
        drop(holder);
    });
    // The reader is asked of a lock of its own, in a home of its own: WAL lets a
    // reader go on while a writer's transaction is open, so only the one lock
    // that keeps the file itself away -- `locking_mode EXCLUSIVE`, which holds it
    // after a write -- is what a `check` can be kept out of, and the writer's own
    // wait would otherwise be over before the reader was asked. The unit case for
    // the same fact is `verbs_tests::a_reader_waits_out_a_short_lock`.
    let read_home = home("held-lock-read");
    assert!(
        run(&read_home, &["test-write", "--batch", "1", "--rows", "1"])
            .status
            .success()
    );
    let read_holder = held_lock(&read_home, "held");
    let reader = std::thread::spawn(move || {
        let out = run(&read_home, &["check"]);
        (read_home, met_lock(&out), said(&out))
    });
    let waits = met_lock(&run(&held, &["test-write", "--batch", "2", "--rows", "1"]));
    let (reader_home, read_waits, read_said) = reader.join().unwrap();
    drop(read_holder);
    assert!(
        waits >= 1,
        "a writer kept out of the store by a held lock said nothing"
    );
    assert!(
        read_waits >= 1,
        "a reader kept out of the store by a held lock said nothing: {read_said}"
    );
    // Both waits were waited out rather than met, so both calls did their work:
    // the rows are there and the store still reads well.
    let out = run(&held, &["test-write", "--batch", "3", "--rows", "1"]);
    assert!(out.status.success(), "{}", said(&out));
    assert_eq!(
        rows(&held),
        ["held", "test-write-1", "test-write-2", "test-write-3"]
            .map(|name| (name.to_string(), "0".to_string()))
            .into_iter()
            .collect(),
        "what the calls that waited wrote"
    );
    let out = run(&held, &["check"]);
    assert!(out.status.success(), "check: {}", said(&out));
    let out = run(&reader_home, &["check"]);
    assert!(out.status.success(), "check: {}", said(&out));
    let _ = std::fs::remove_dir_all(&reader_home);
    let _ = std::fs::remove_dir_all(&held);
}

#[test]
fn a_writer_killed_mid_transaction_leaves_a_whole_store_and_none_of_its_rows() {
    let home = home("killed");
    let out = run(&home, &["test-write", "--batch", "1", "--rows", "10"]);
    assert!(out.status.success(), "{}", said(&out));
    let wal = |home: &Path| {
        std::fs::metadata(PathBuf::from(format!("{}-wal", db(home).display())))
            .map_or(0, |m| m.len())
    };
    let before = wal(&home);

    // Rows of 1 KiB, more of them than SQLite's page cache holds, so pages of
    // the open transaction are in the -wal when it dies.
    let mut child = store(
        &home,
        &["test-write", "--batch", "2", "--rows", "4000", "--hold"],
    )
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert!(line.contains("\"held\":true"), "the writer said {line:?}");

    // Mid-transaction: it holds the write lock, none of its rows is visible,
    // and some of them are on disk.
    let probe = Connection::open(db(&home)).unwrap();
    probe.busy_timeout(Duration::ZERO).unwrap();
    let begin = probe.execute_batch("BEGIN IMMEDIATE").unwrap_err();
    assert_eq!(begin.sqlite_error_code(), Some(ErrorCode::DatabaseBusy));
    let seen: i64 = probe
        .query_row(
            "SELECT count(*) FROM branch_checks WHERE name = 'test-write-2'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(seen, 0);
    drop(probe);
    assert!(
        wal(&home) > before + 1024 * 1024,
        "the -wal grew from {before} to {} bytes",
        wal(&home)
    );

    child.kill().unwrap();
    let status = child.wait().unwrap();
    assert_eq!(status.signal(), Some(9), "{status:?}");

    let out = run(&home, &["check"]);
    assert!(out.status.success(), "check: {}", said(&out));
    let answer = String::from_utf8_lossy(&out.stdout);
    assert!(answer.contains("\"integrity\":\"ok\""), "{answer}");
    assert!(answer.contains("\"branch_checks\":10,"), "{answer}");
    assert_eq!(rows(&home), expected(1, 10).collect());

    // The lock died with the process: the next writer is not kept waiting.
    let out = run(&home, &["test-write", "--batch", "3", "--rows", "1"]);
    assert!(out.status.success(), "{}", said(&out));
    let _ = std::fs::remove_dir_all(&home);
}
