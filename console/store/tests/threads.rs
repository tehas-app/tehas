//! The store called in one process from many threads at once. SQLite 3.51.0
//! and 3.51.1 could deadlock there: a thread closing the last connection of a WAL database held the file's lock mutex
//! and asked for the VFS mutex, while a thread opening the same file held the
//! VFS mutex and asked for the lock mutex. Every later call of the process
//! then waited on the VFS mutex for ever.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use tehas_core::home::Home;
use tehas_core::testdir::TestDir;

/// Two threads to a store: with more, one mostly keeps a connection open, and
/// the close that deadlocked was of the last one.
const HOMES: usize = 6;
const LOAD: Duration = Duration::from_secs(3);
/// Past this the calls are stuck, however busy the machine.
const STUCK: Duration = Duration::from_secs(60);

fn call(home: &Path, args: &[&str]) -> Result<String, (u8, String)> {
    store::call(home, args, &mut std::io::empty())
}

fn home(n: usize) -> TestDir {
    let dir = TestDir::new(&format!("store-threads-{n}"));
    std::fs::create_dir_all(Home::new(dir.path()).state_dir()).unwrap();
    call(
        &dir,
        &[
            "init",
            "--version",
            "v0.30.0",
            "--commit",
            "cb6912a",
            "--instance",
            "example",
            "--repo",
            "example-owner/example-product",
        ],
    )
    .unwrap();
    dir
}

#[test]
fn threads_that_open_and_close_one_store_at_once_never_deadlock() {
    // Every home first: a deadlock takes the VFS mutex, and a call of this
    // thread would wait on it with the rest.
    let dirs: Vec<TestDir> = (0..HOMES).map(home).collect();
    let homes: Vec<PathBuf> = dirs.iter().map(|d| d.path().to_path_buf()).collect();
    let (tx, rx) = mpsc::channel();
    for home in homes {
        for writes in [false, true] {
            let (home, tx) = (home.clone(), tx.clone());
            thread::spawn(move || {
                let end = Instant::now() + LOAD;
                let mut calls = 0u64;
                while Instant::now() < end {
                    let args: &[&str] = if writes {
                        &["poll-begin"]
                    } else {
                        &["locks-held"]
                    };
                    call(&home, args).unwrap_or_else(|e| panic!("{args:?}: {e:?}"));
                    calls += 1;
                }
                let _ = tx.send(calls);
            });
        }
    }
    drop(tx);
    let until = Instant::now() + STUCK;
    for _ in 0..HOMES * 2 {
        let left = until.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok(calls) => assert!(calls > 0),
            Err(e) => panic!("the store's calls did not end within {STUCK:?}: {e}"),
        }
    }
}
