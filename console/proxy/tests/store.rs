//! The factory's records as the proxy reads them from a real state store: the
//! contractors that are not revoked, and the state of each job it holds a token
//! of. A store with the feature off reads as nobody.

use std::path::Path;

use tehas_core::testdir::TestDir;
use tehas_proxy::jobs::Issuer;
use tehas_proxy::records::{OfProxy, OfStore, Reader};
use tehas_proxy::request::{Records, State};

const KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn call(home: &Path, args: &[&str]) -> String {
    store::call(home, args, &mut std::io::empty()).unwrap_or_else(|e| panic!("{args:?}: {e:?}"))
}

fn made(name: &str, outsource: bool) -> TestDir {
    let home = TestDir::new(name);
    std::fs::create_dir_all(home.path().join("state")).unwrap();
    let mut args = vec![
        "init",
        "--version",
        "v0.30.0",
        "--commit",
        "cb6912a",
        "--instance",
        "example",
        "--repo",
        "example-owner/example-product",
    ];
    if outsource {
        args.push("--outsource");
    }
    call(home.path(), &args);
    home
}

/// A job row as the store's job machine leaves it. The issue it names has no
/// row of its own here: the proxy reads the job, never the issue.
fn job(home: &Path, id: i64, contractor: &str, state: &str) {
    let c = rusqlite::Connection::open(home.join("state/tehas.db")).unwrap();
    c.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
    c.execute(
        "INSERT INTO jobs (id, issue, contractor, attempt, state, offered_at, expires_at, updated_at)
           VALUES (?1, 41, ?2, 1, ?3, 1, 2, 1)",
        rusqlite::params![id, contractor, state],
    )
    .unwrap();
}

#[test]
fn the_store_says_who_may_work_and_where_each_held_job_is() {
    let home = made("proxy-store-on", true);
    let h = home.path();
    for name in ["gpu1", "gpu2", "gpu3"] {
        call(h, &["contractor-add", "--name", name, "--key", KEY]);
    }
    call(h, &["contractor-revoke", "--name", "gpu2"]);
    call(h, &["contractor-drain", "--name", "gpu3"]);
    job(h, 7, "gpu1", "running");
    job(h, 8, "gpu3", "lost");
    job(h, 9, "gpu1", "running");
    let issuer = Issuer::new();
    issuer.put("token-of-7", 7, 1, "gpu1");
    issuer.put("token-of-8", 8, 1, "gpu3");
    let reader = OfStore::at(h, &issuer);
    let rows = reader.rows();
    // A revoked contractor may not work; a draining one finishes its job.
    assert_eq!(rows.contractors, vec!["gpu1", "gpu3"]);
    // Only the jobs of tokens the proxy holds are read.
    let jobs: Vec<(i64, &str)> = rows
        .jobs
        .iter()
        .map(|j| (j.job, j.state.as_str()))
        .collect();
    assert_eq!(jobs, vec![(7, "running"), (8, "lost")]);
    assert!(rows.jobs.iter().all(|j| j.token.is_empty()));

    let records = OfProxy::of(&reader, &issuer);
    assert_eq!(records.state_of(7), Some(State::Running));
    assert_eq!(records.state_of(8), Some(State::Lost));
    assert_eq!(records.state_of(9), None, "a job of no held token was read");
    assert_eq!(records.holder(7), Some((1, "gpu1".to_string())));
    assert_eq!(records.holder(8), Some((1, "gpu3".to_string())));
    assert!(records.allowed("gpu1"));
    assert!(!records.allowed("gpu2"));
    assert!(!records.allowed("nobody"));
}

#[test]
fn a_store_with_the_feature_off_or_none_at_all_reads_as_nobody() {
    let issuer = Issuer::new();
    issuer.put("token-of-7", 7, 1, "gpu1");
    let off = made("proxy-store-off", false);
    let rows = OfStore::at(off.path(), &issuer).rows();
    assert!(rows.contractors.is_empty());
    assert!(rows.jobs.is_empty());
    let none = TestDir::new("proxy-store-none");
    let rows = OfStore::at(none.path(), &issuer).rows();
    assert!(rows.contractors.is_empty());
    assert!(rows.jobs.is_empty());
}
