//! What tehas work says when the read of the conflict check of a refresh
//! cannot be judged, and when the product's install cannot open its preflight
//! log. A refresh that is not judged starts no container, says which part of
//! the answer was missing, and writes no note. A git with no such read at all
//! is the shape of the host, and its start runs.

mod work;

use serde_json::json;
use std::fs;
use work::{body_sha256, epoch, git_out, has, lacks, World};

fn world(name: &str) -> World {
    let w = World::new(name);
    w.stub_github();
    w.caps_continue();
    w
}

/// The workspace and the calls of the case before are gone (caps_fresh).
fn caps_fresh(w: &World) {
    w.clear_calls();
    for suffix in ["", ".state", ".before-retry"] {
        let mut p = w.ws.clone().into_os_string();
        p.push(suffix);
        let _ = fs::remove_dir_all(&p);
    }
}

/// The approval of a refresh of pull request 50 at `head`, as the dispatcher
/// records it.
fn refresh_at(w: &World, head: &str) {
    fs::create_dir_all(w.home.join("state/cycles")).unwrap();
    w.issue_is("", "NONE", "");
    w.store_db();
    let input = w.path("refresh-input.json");
    let v = json!({"links": [], "refresh_of": {"pr": 50, "head_sha": head, "branch": "main"}});
    fs::write(&input, v.to_string()).unwrap();
    w.approve(
        7,
        "alice",
        "refresh",
        epoch("2026-10-01T00:00:00Z"),
        &[
            "--body-sha",
            &body_sha256(""),
            "--input-file",
            &input.to_string_lossy(),
            "--refresh-pr",
            "50",
            "--refresh-head",
            head,
        ],
    );
    w.started(7, &[]);
}

/// A branch of the product off main that writes `line` to `file`; its head.
fn pr_branch(w: &World, branch: &str, file: &str, line: &str) -> String {
    git_out(&w.upstream, &["checkout", "-q", "-b", branch]);
    w.commit_upstream("the pull request", Some((file, line)));
    let head = git_out(&w.upstream, &["rev-parse", "HEAD"]);
    git_out(&w.upstream, &["checkout", "-q", "main"]);
    w.fetched();
    head
}

/// A git whose one call holding `merge-tree` answers `rc`, says `said` and
/// writes `tree`; every other call is the host's own git.
fn git_reads(w: &World, rc: i32, said: &str, tree: &str) {
    let tree_line = if tree.is_empty() {
        String::new()
    } else {
        format!("printf '%s\\0%s\\0' '{tree}' 'one.txt'\n")
    };
    let said_line = if said.is_empty() {
        String::new()
    } else {
        format!("printf '%s\\n' '{said}' >&2\n")
    };
    w.stub(
        "git",
        &format!(
            "case \"$*\" in\n  *merge-tree*)\n{tree_line}{said_line}exit {rc} ;;\n  *) exec \"$(command -pv git)\" \"$@\" ;;\nesac"
        ),
    );
}

fn no_note(w: &World) {
    assert!(!w.home.join("state/cycles/7.refresh-conflict").exists());
}

#[test]
fn a_refresh_whose_conflict_check_named_a_side_it_could_not_merge_starts_no_container_78() {
    let w = world("judge-side");
    caps_fresh(&w);
    let head = pr_branch(&w, "feature/none", "none.txt", "a line of its own");
    refresh_at(&w, &head);
    git_reads(
        &w,
        1,
        &format!("merge-tree: {head} - not something we can merge"),
        "",
    );
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    assert!(w.cycle_args().is_empty());
    has(
        &r.out,
        "whether pull request #50 conflicts with origin/main cannot be judged",
    );
    has(&r.out, "the read of it names one side of the merge");
    has(&r.out, "not something we can merge");
    no_note(&w);
}

#[test]
fn a_refresh_whose_conflict_check_ran_out_of_time_starts_no_container_78_with_what_git_said() {
    let w = world("judge-time");
    caps_fresh(&w);
    let head = pr_branch(&w, "feature/timed", "timed.txt", "a line of its own");
    refresh_at(&w, &head);
    git_reads(&w, 124, "fatal: bad object", "");
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    assert!(w.cycle_args().is_empty());
    has(
        &r.out,
        "whether pull request #50 conflicts with origin/main cannot be judged",
    );
    has(&r.out, "git did not answer within 120s: fatal: bad object");
    no_note(&w);
}

#[test]
fn a_refresh_whose_conflict_check_answered_a_status_that_is_no_answer_starts_no_container_78() {
    let w = world("judge-status");
    caps_fresh(&w);
    let head = pr_branch(&w, "feature/strange", "strange.txt", "a line of its own");
    refresh_at(&w, &head);
    git_reads(&w, 3, "", &"c".repeat(40));
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    assert!(w.cycle_args().is_empty());
    has(&r.out, "cannot be judged");
    has(
        &r.out,
        "git wrote a tree object and answered 3 with 1 file(s) named",
    );
    no_note(&w);
}

#[test]
fn a_refresh_judged_by_a_git_with_no_such_read_runs_with_git_s_words_in_the_log() {
    let w = world("judge-old");
    caps_fresh(&w);
    let head = pr_branch(&w, "feature/old", "old.txt", "a line of its own");
    refresh_at(&w, &head);
    git_reads(
        &w,
        129,
        "usage: git merge-tree [--write-tree] [<options>] <branch1> <branch2>",
        "",
    );
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    has(&w.cycle_args().join(" "), "--continue-pr");
    has(&r.out, "usage: git merge-tree");
    no_note(&w);
}

// The name of a start's log is made of the second it began. The first run is
// an ordinary start whose install writes its lines beside its log; the second
// holds the preflight of its own log's name open as a directory, which nothing
// opens for append. The push check reads that name first, so it is the step
// that meets the fault. The install's half is tried beside the code
// (console/cycle/src/prepare.rs).
#[test]
fn an_install_whose_preflight_log_cannot_be_opened_says_that_with_the_path_and_is_not_127() {
    let w = World::new("judge-preflight");
    w.stub_github();
    caps_fresh(&w);
    fs::create_dir_all(w.upstream.join("extension")).unwrap();
    w.commit_upstream("lock", Some(("extension/bun.lock", "{}")));
    w.fetched();

    caps_fresh(&w);
    w.caps_continue();
    refresh_at(&w, &git_out(&w.upstream, &["rev-parse", "main"]));
    let r = w.work7();
    let log = r
        .out
        .lines()
        .find_map(|l| l.strip_prefix("tehas work: issue 7 started, log "))
        .unwrap_or_else(|| panic!("the run named no log of its own: {}", r.out))
        .to_string();
    let preflight = format!("{log}.preflight");
    let text =
        fs::read_to_string(&preflight).expect("the run left no preflight log beside its log");
    assert!(
        text.lines().any(|l| l.starts_with("install extension/")),
        "the install wrote no line of its own: {text}"
    );

    caps_fresh(&w);
    w.caps_continue();
    refresh_at(&w, &git_out(&w.upstream, &["rev-parse", "main"]));
    fs::remove_file(&preflight).unwrap();
    // Whichever second the start names its log by, its preflight is a
    // directory.
    let now = tehas_core::time::now() as i64;
    let logs = w.home.join("logs");
    for s in 0..=2 {
        let name = second(now + s);
        fs::create_dir_all(logs.join(format!("work-7-{name}.jsonl.preflight"))).unwrap();
    }
    fs::create_dir_all(&preflight).unwrap();
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    assert!(w.cycle_args().is_empty());
    has(
        &r.out,
        "could not be opened for the push check of the sandbox",
    );
    let reported = r
        .out
        .lines()
        .find_map(|l| {
            let (_, rest) = l.split_once("the preflight log ")?;
            let (path, _) = rest.split_once(".preflight could not be opened")?;
            Some(format!("{path}.preflight"))
        })
        .unwrap_or_default();
    assert!(
        fs::metadata(&reported).is_ok_and(|m| m.is_dir()),
        "the fault named no directory the case planted: {reported}"
    );
    lacks(&r.out, "the sandbox cannot push");
    lacks(&r.out, "install failed");
    lacks(&r.out, "(exit 127)");
    assert!(!w.home.join("state/push-fault.json").exists());
}

/// An epoch second as a start's log names it: `%y%m%d-%H%M%S` in UTC.
fn second(at: i64) -> String {
    let out = std::process::Command::new("date")
        .args(["-u", "-d", &format!("@{at}"), "+%y%m%d-%H%M%S"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}
