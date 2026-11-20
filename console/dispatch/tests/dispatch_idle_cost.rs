//! The cost of a poll that starts nothing, on a host the size of a working
//! one: 500 old lock files, 50 shipped issues and 5 blocked ones. Two figures
//! are pinned: the poll makes no write to GitHub, and it starts fewer
//! processes than PROCS_MAX, counted by the PATH shims of procs_reset(). A
//! change that raises either has to change this test, which is the point of
//! it.

mod common;

use common::{has, root, World};
use serde_json::{json, Value};
use std::fs;
use tehas_core::testing::write_exe;

/// The most processes one idle poll may start, its gh calls included. The
/// figure of #736 was 339, and this ceiling sits a fifth above it. The poll
/// runs the real `tehasbot status` and `console tidy run`, which put it at
/// 386 on 2026-10-27. The ceiling stays where it is, so a poll that pays a gh
/// call or a process per lock file is refused: nothing here may grow with
/// what the host holds. A raise is the signal that a poll has grown.
const PROCS_MAX: usize = 405;

/// The bash of the tests' PATH.
fn bash() -> String {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .map(|d| std::path::Path::new(d).join("bash"))
        .find(|p| p.is_file())
        .expect("bash on PATH")
        .to_string_lossy()
        .into_owned()
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// A host that has been running: `locks` lock files of issues it once ran,
/// `shipped` closed issues with tehas:shipped, and `blocked` open issues the
/// blocked label holds. #7 waits, and the control issue is paused.
fn world(w: &World, locks: u64, shipped: u64, blocked: u64) {
    fs::create_dir_all(w.cycles_dir()).unwrap();
    for n in 1000..1000 + locks {
        fs::write(w.cycles_dir().join(format!("{n}.lock")), "").unwrap();
    }
    let closed: Vec<Value> = (1..=shipped)
        .map(|n| json!({"number": n, "state": "CLOSED", "labels": [{"name": "tehas:shipped"}]}))
        .collect();
    w.fx_json("closed.json", &json!(closed));
    let mut ready = vec![json!({"number": 7, "labels": [{"name": "tehas:ready"}]})];
    ready.extend(
        (100..100 + blocked).map(|n| json!({"number": n, "labels": [{"name": "tehas:blocked"}]})),
    );
    w.fx_json("ready.json", &json!(ready));
    let label =
        |l: &str| json!({"event": "labeled", "label": {"name": l}, "actor": {"login": "alice"}});
    w.fx_json("events-7.json", &json!([label("tehas:ready")]));
    for n in 100..100 + blocked {
        w.fx_json(
            &format!("events-{n}.json"),
            &json!([label("tehas:ready"), label("tehas:blocked")]),
        );
    }
    let _ = fs::remove_file(w.home.join("state/github-writes.log"));
}

/// One poll of that world as a host runs it: the real status and the real
/// tidy, which the other tests stand in for.
fn idle_poll(w: &World) -> common::Run {
    w.fx("control.json", r#"{"labels":[{"name":"tehas:pause"}]}"#);
    w.fx(
        "events-1.json",
        r#"[{"event":"labeled","label":{"name":"tehas:pause"},"actor":{"login":"alice"}}]"#,
    );
    let bash = bash();
    let tidy = w.t().join("real-tidy");
    write_exe(
        &tidy,
        format!(
            "#!{bash}\nexec '{}' tidy run \"$@\"\n",
            w.console().display()
        ),
    )
    .unwrap();
    let status = w.t().join("real-status");
    write_exe(
        &status,
        format!(
            "#!{bash}\nTEHAS_CONSOLE='{}' exec '{}' status\n",
            w.console().display(),
            root().join("bin/tehasbot").display()
        ),
    )
    .unwrap();
    w.env("TEHAS_STATUS_CMD", &status.to_string_lossy());
    w.env("TEHAS_TIDY_CMD", &tidy.to_string_lossy());
    w.procs_reset();
    w.poll()
}

#[test]
fn an_idle_poll_of_a_full_host_makes_no_write_to_github() {
    let w = World::new("idle-cost");
    world(&w, 500, 50, 5);
    let r = idle_poll(&w);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    // The settings reached the console through the poll: it made its search.
    has(
        &w.gh(),
        r#"issue list --repo acme/widget --state closed --search label:"tehas:running""#,
    );
    // Every call is a read. A call starts its line; the lines of a GraphQL
    // query that follow it are not calls.
    let writes = w
        .gh()
        .lines()
        .filter(|l| {
            [
                "issue edit ",
                "issue comment ",
                "api -X ",
                "api POST ",
                "label ",
            ]
            .iter()
            .any(|p| l.starts_with(p))
        })
        .count();
    assert_eq!(writes, 0, "{}", w.gh());
    assert!(!w.home.join("state/github-writes.log").exists());
    // The poll said so to the store too: 0 starts and no writes on its row.
    let r = w.store_at(now(), &["poll-show"]);
    let c: Value = serde_json::from_str(&r.out).unwrap();
    assert_eq!(c["last"]["starts"], json!(0));
    assert!(c["last"]["writes"].is_null(), "{c}");
}

#[test]
fn an_idle_poll_of_a_full_host_starts_fewer_processes_than_the_ceiling() {
    let w = World::new("idle-cost");
    world(&w, 500, 50, 5);
    idle_poll(&w);
    // The gh calls are counted by their stub, not by a PATH shim.
    let procs = w.procs() + w.gh_reads();
    let list = w.procs_list();
    if std::env::var_os("TEHAS_TEST_PROCS").is_some() {
        eprintln!("processes: {procs}\n{:?}", &list[..list.len().min(20)]);
    }
    assert!(
        procs <= PROCS_MAX,
        "{procs} processes, over the ceiling of {PROCS_MAX}: {:?}",
        &list[..list.len().min(5)]
    );
}
