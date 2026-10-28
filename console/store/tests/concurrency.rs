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
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

const STORE: &str = env!("CARGO_BIN_EXE_store");
const WRITERS: u32 = 8;
const READERS: u32 = 8;
const LOAD: Duration = Duration::from_secs(2);
/// A call that answered ok and took this long met the busy handler on the way:
/// the store's own five seconds is what it would have waited, and the whole load
/// of eight writers and eight readers costs a few tens of milliseconds. Under the
/// installed bounds a waiter would still be caught, but only after ten seconds of
/// the load, which is why the load is measured against this instead of against
/// `db::BUSY_TIMEOUT`.
const LEAST_BUSY: Duration = Duration::from_millis(1000);
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
    cmd.args(args).env_clear().env("TEHAS_HOME", home);
    cmd
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

/// The rows of branch_checks by name and head, read by this process.
fn rows(home: &Path) -> BTreeSet<(String, String)> {
    let conn = Connection::open_with_flags(db(home), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
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

#[test]
fn eight_writers_and_eight_readers_meet_no_busy_and_lose_no_write() {
    let home = home("load");
    let end = Instant::now() + LOAD;
    // What any of the sixteen processes took long enough to be a wait rather
    // than a call, named so a failure says who met the lock. The handles are
    // collected before the join, so the closures are alive by the time their
    // lifetimes are checked and a borrow of a local here cannot serve: the
    // vector is leaked and shared by pointer, and dropped with the process.
    let slowest: &'static Mutex<Vec<String>> = Box::leak(Box::new(Mutex::new(Vec::new())));
    let writers: Vec<_> = (0..WRITERS)
        .map(|w| {
            let home = home.clone();
            thread::spawn(move || {
                let (mut written, mut failed) = (Vec::new(), Vec::new());
                let mut n = 0;
                while Instant::now() < end {
                    let batch = w * 1_000_000 + n;
                    let asked = Instant::now();
                    let out = run(
                        &home,
                        &[
                            "test-write",
                            "--batch",
                            &batch.to_string(),
                            "--rows",
                            &ROWS_PER_WRITE.to_string(),
                        ],
                    );
                    if out.status.success() {
                        written.push(batch);
                        let took = asked.elapsed();
                        if took >= LEAST_BUSY {
                            slowest
                                .lock()
                                .unwrap()
                                .push(format!("writer {w}, batch {batch}: {took:?}"));
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
                while Instant::now() < end {
                    let asked = Instant::now();
                    let out = run(&home, &["check"]);
                    if out.status.success() {
                        reads += 1;
                        let took = asked.elapsed();
                        if took >= LEAST_BUSY {
                            slowest
                                .lock()
                                .unwrap()
                                .push(format!("reader {r}: {took:?}"));
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
    // tests, not to these child processes -- so a call that had to sit on a lock
    // would reach `waited` or `failed`, and no healthy call comes near a second.
    assert!(failed.is_empty(), "{} calls failed:\n{}", failed.len(), {
        failed.truncate(10);
        failed.join("\n")
    });
    let waited = slowest.lock().unwrap().clone();
    assert!(
        waited.is_empty(),
        "{} calls waited on the store:\n{}",
        waited.len(),
        {
            let mut shown = waited.clone();
            shown.truncate(10);
            shown.join("\n")
        }
    );
    eprintln!(
        "{} rows written; fewest writes, reads per process: {fewest:?}",
        want.len()
    );
    // Every writer and every reader had its turn many times over.
    assert!(
        fewest.0 >= 5 && fewest.1 >= 5,
        "fewest writes, reads: {fewest:?}"
    );
    assert_eq!(rows(&home), want, "every write that answered 0 is there");
    let out = run(&home, &["check"]);
    assert!(out.status.success(), "check: {}", said(&out));
    assert!(String::from_utf8_lossy(&out.stdout).contains("\"integrity\":\"ok\""));
    let _ = std::fs::remove_dir_all(&home);
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
