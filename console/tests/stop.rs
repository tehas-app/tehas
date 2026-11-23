//! `tehas stop` where it reaches no GitHub: a cycle left whole while it holds
//! its lock, a lock that is no lock file, a container that will not go, the
//! words it refuses, and the state store that cannot end, clear or read the
//! cycle's record. systemctl and docker are stubs; gh is not, so a call to it
//! fails the case. The cases of the issue's running mark are on the ports'
//! fake in console/ops/src/stop.rs, and the stops that go on to it end to end
//! in test/stop_test.sh.

mod common;

use common::*;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// The notes a stop takes away (GO of the shell).
const GO: &[&str] = &[
    "lock",
    "cid",
    "done",
    "stalled",
    "timed",
    "stopped",
    "stopped.new",
    "oom",
    "forks",
    "nostart",
    "nostart.new",
    "progress.json",
    "result.json",
];
/// The notes a stop keeps as the cycle's record.
const KEEP: &[&str] = &["json", "manifest.json", "runner_exit", "status"];
/// What a cycle leaves that a stop takes: the notes of GO, the park on
/// record, and six paths beside them. Stated, not counted from GO, so a note
/// added to GO has to be named here too.
const LEFTOVERS_N: usize = 20;
const BUSY: &str = "store: busy #5: the database is locked";

/// systemctl: the unit's state is $SHOW (active when unset), and a stop ends
/// nothing, as the stop of a unit whose cycle holds its lock elsewhere.
const SYSTEMCTL_LIVE: &str = r#"case "$2" in show) printf "%s\n" "${SHOW:-active}" ;; esac"#;
const SYSTEMCTL_IDLE: &str = r#"case "$2" in show) printf "inactive\n" ;; esac"#;
/// docker: the containers of $CONTAINERS, less those removed.
const DOCKER: &str = r#"case "$1 $2" in
  "ps -a") for c in ${CONTAINERS:-}; do [ -e "$T/gone/$c" ] || printf "%s\n" "$c"; done ;;
  "rm -f") mkdir -p "$T/gone"; : >"$T/gone/$3"; printf "%s\n" "$3" ;;
esac"#;

/// A host as the shell's reset() leaves it, with gh unstubbed.
fn host(name: &str) -> World {
    let w = World::new(&format!("stop-{name}"));
    fs::remove_file(w.t().join("bin/gh")).unwrap();
    w.stub("systemctl", SYSTEMCTL_IDLE);
    w.stub("docker", DOCKER);
    w
}

fn stop(w: &World, words: &[&str]) -> Run {
    w.tehas("stop", words)
}

fn state(w: &World) -> PathBuf {
    w.at().cycles_dir()
}

fn note(w: &World, n: u32, ext: &str) -> PathBuf {
    w.at().cycle_note(n, ext)
}

fn write(p: impl AsRef<Path>, text: &str) {
    let p = p.as_ref();
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}

fn works(w: &World) -> PathBuf {
    w.home.join("work")
}

fn runs(w: &World) -> PathBuf {
    w.t().join("home/.pi/agent/ensemble-runs")
}

fn whoami() -> String {
    let out = Command::new("id").arg("-un").output().unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// The lock of `n`, held by this process until the value drops: a cycle the
/// stop's systemctl stop does not end.
fn hold(w: &World, n: u32) -> File {
    fs::create_dir_all(state(w)).unwrap();
    let f = OpenOptions::new()
        .append(true)
        .create(true)
        .open(note(w, n, "lock"))
        .unwrap();
    f.lock().unwrap();
    f
}

/// A lock held this way also cuts the stop's ten seconds of naps short.
fn hold_whole(w: &World, n: u32) -> File {
    w.stub("sleep", "command -p sleep 0.05");
    hold(w, n)
}

/// A store verb run under the lock of `n`, as a cycle runs it.
fn locked(w: &World, n: u32, args: &[&str]) {
    let r = w.store_locked(n.into(), args);
    assert_eq!(r.rc, 0, "the store refused {args:?}: {}", r.out);
}

/// t_started: a start on record, by hand.
fn started(w: &World, n: u32) {
    locked(w, n, &["cycle-start", &n.to_string(), "--by-hand"]);
}

/// Every file a cycle of `n` leaves on the host, its workspace, and its last
/// cycle, started by hand, parked with the question 42<n> on record. The
/// shell's started on alice's approval; a stop reads no approval.
fn cycle_files(w: &World, n: u32) {
    let s = state(w);
    fs::create_dir_all(&s).unwrap();
    for ext in GO.iter().chain(KEEP) {
        fs::write(note(w, n, ext), format!("{n} {ext}\n")).unwrap();
    }
    let now = tehas_core::time::now();
    fs::write(note(w, n, "stopped"), format!("{now} {}\n", whoami())).unwrap();
    let issue = n.to_string();
    let a = w.attempt(n.into());
    started(w, n);
    let next = (a + 1).to_string();
    locked(
        w,
        n,
        &[
            "cycle-end",
            &issue,
            "--attempt",
            &next,
            "--status",
            "failed",
            "--parked",
        ],
    );
    w.store_ok(&["park", &issue, "--attempt", &next, "--reason", "failed"]);
    w.store_ok(&[
        "question-asked",
        &issue,
        "--id",
        &format!("42{n}"),
        "--kind",
        "park",
    ]);
    write(w.at().live_console(n.into()), "console line\n");
    let run = runs(w).join(format!("issue-{n}"));
    fs::create_dir_all(&run).unwrap();
    fs::write(run.join("a-1-coder.json"), "transcript\n").unwrap();
    let ws = works(w);
    for d in ["", ".state", ".images"] {
        fs::create_dir_all(ws.join(format!("widget-{n}{d}"))).unwrap();
    }
    fs::write(ws.join(format!("widget-{n}/README.md")), "the work\n").unwrap();
    fs::write(ws.join(format!("widget-{n}.before-retry")), "").unwrap();
    fs::write(ws.join(".tehas-workspaces"), "").unwrap();
}

/// What `n` leaves that a stop should take, still on the host.
fn leftovers(w: &World, n: u32) -> Vec<String> {
    let mut out: Vec<String> = GO
        .iter()
        .map(|ext| note(w, n, ext))
        .filter(|p| p.exists())
        .map(|p| p.display().to_string())
        .collect();
    let park = &w.store_ok(&["park-show", &n.to_string()])["park"];
    if !park.is_null() && (!park["question"].is_null() || park["retries"] == 0) {
        out.push(format!("park-of-{n}"));
    }
    let ws = works(w);
    let beside = [
        w.at().live_console(n.into()),
        runs(w).join(format!("issue-{n}")),
        ws.join(format!("widget-{n}")),
        ws.join(format!("widget-{n}.state")),
        ws.join(format!("widget-{n}.before-retry")),
        ws.join(format!("widget-{n}.images")),
    ];
    out.extend(
        beside
            .iter()
            .filter(|p| p.exists())
            .map(|p| p.display().to_string()),
    );
    out
}

fn removed_containers(w: &World) -> String {
    w.calls("docker")
        .lines()
        .filter_map(|l| l.strip_prefix("rm -f "))
        .map(|c| format!("{c} "))
        .collect()
}

fn unit_stops(w: &World) -> usize {
    count(&w.calls("systemctl"), " stop ")
}

fn count(text: &str, want: &str) -> usize {
    text.lines().filter(|l| l.contains(want)).count()
}

#[track_caller]
fn rc(r: &Run, want: i32) {
    assert_eq!(r.rc, want, "exit {} and not {want}:\n{}", r.rc, r.out);
}

#[track_caller]
fn there(p: impl AsRef<Path>) {
    let p = p.as_ref();
    assert!(p.exists(), "{} should be there", p.display());
}

#[test]
fn a_marker_holds_the_time_and_the_account_and_a_cycle_left_whole_keeps_it() {
    // The cycle ran and has not reported: its lock is held by a process the
    // stop's systemctl stop leaves alone, so the run stops the unit, waits for
    // a lock that will not free, and leaves every leftover where it is. The
    // marker it wrote before it asked is the record of that stop.
    let w = host("marker");
    cycle_files(&w, 5);
    fs::remove_file(note(&w, 5, "stopped")).unwrap();
    w.stub("systemctl", SYSTEMCTL_LIVE);
    let lock = hold_whole(&w, 5);
    let began = tehas_core::time::now();
    let r = stop(&w, &["5"]);
    drop(lock);
    rc(&r, 1);
    has(&r.out, "stopped tehas-cycle@5.service");
    has(&r.out, "the cycle for #5 still holds its lock");
    has(&r.out, "nothing was removed");
    // The two fields a cycle reads: the seconds, then the account.
    let marker = fs::read_to_string(note(&w, 5, "stopped")).expect("the stop left no marker");
    let fields: Vec<&str> = marker.split_whitespace().collect();
    assert_eq!(fields.len(), 2, "the marker is not two fields: {marker:?}");
    let at: i64 = fields[0].parse().expect("the marker's time is seconds");
    assert!(fields[0].len() <= 10);
    assert_eq!(fields[1], whoami());
    assert!(
        (-5..=30).contains(&(at - began)),
        "the marker's time {at} is not the stop's, which began at {began}"
    );
    // The marker went before the ask, and the unit was asked once.
    assert_eq!(unit_stops(&w), 1);
    // Every leftover but the name of the write, moved into the marker.
    assert_eq!(leftovers(&w, 5).len(), LEFTOVERS_N - 1);
}

#[test]
fn a_run_that_finds_nothing_at_all_says_so_and_writes_and_removes_no_marker() {
    let w = host("nothing");
    let r = stop(&w, &["5"]);
    rc(&r, 0);
    assert_eq!(r.out.lines().last(), Some("nothing to stop for #5"));
    assert!(!note(&w, 5, "stopped").exists());
}

#[test]
fn a_cycle_that_still_holds_its_lock_after_the_stop_is_left_whole_with_no_label_changed() {
    let w = host("whole");
    cycle_files(&w, 5);
    w.stub("systemctl", SYSTEMCTL_LIVE);
    let lock = hold_whole(&w, 5);
    // Its container is on this host, and is not taken while the lock is held.
    w.set_env(&["CONTAINERS=tehas-work-5"]);
    let began = Instant::now();
    let r = stop(&w, &["5"]);
    drop(lock);
    rc(&r, 1);
    assert_eq!(leftovers(&w, 5).len(), LEFTOVERS_N - 1);
    has(&r.out, "still holds its lock");
    assert_eq!(removed_containers(&w), "");
    there(works(&w).join("widget-5"));
    // The wait for the lock is bounded.
    assert!(began.elapsed() < Duration::from_secs(30));
    // Nothing was cleared, so the issue was not touched: gh is no stub here.
}

#[test]
fn a_cycle_that_holds_its_lock_with_no_unit_behind_it_is_left_whole_too() {
    let w = host("no-unit");
    cycle_files(&w, 5);
    let lock = hold_whole(&w, 5);
    let r = stop(&w, &["5"]);
    drop(lock);
    rc(&r, 1);
    assert_eq!(leftovers(&w, 5).len(), LEFTOVERS_N - 1);
    has(&r.out, "still holds its lock");
}

#[test]
fn a_lock_that_is_no_lock_file_stops_it_and_what_it_points_at_is_not_touched() {
    let w = host("link");
    cycle_files(&w, 5);
    let target = w.t().join("target");
    fs::write(&target, "a person reads this\n").unwrap();
    fs::remove_file(note(&w, 5, "lock")).unwrap();
    std::os::unix::fs::symlink(&target, note(&w, 5, "lock")).unwrap();
    let r = stop(&w, &["5"]);
    rc(&r, 1);
    has(&r.out, "5.lock is not a lock file");
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "a person reads this\n"
    );
    there(works(&w).join("widget-5"));
}

#[test]
fn a_container_that_will_not_be_removed_is_said_and_the_rest_still_goes_but_the_label_stays() {
    let w = host("container");
    cycle_files(&w, 5);
    w.stub(
        "docker",
        r#"case "$1 $2" in
"ps -a") printf "tehas-work-5\n" ;;
"rm -f") echo "cannot remove container: read-only filesystem" >&2; exit 1 ;;
esac"#,
    );
    let r = stop(&w, &["5"]);
    rc(&r, 1);
    has(&r.out, "could not remove the container tehas-work-5");
    assert_eq!(leftovers(&w, 5), Vec::<String>::new());
    // With a container still at work on the bot's token, tehas:running stays:
    // gh is no stub here.
}

#[test]
fn an_issue_that_is_not_a_number_is_64_before_anything_is_read() {
    let w = host("words");
    cycle_files(&w, 5);
    w.stub("systemctl", SYSTEMCTL_LIVE);
    let words: &[&[&str]] = &[&["0"], &["5a"], &["-5"], &["12345678901"], &[], &[""]];
    for given in words {
        let r = stop(&w, given);
        assert_eq!(r.rc, 64, "{given:?}:\n{}", r.out);
        assert_eq!(leftovers(&w, 5).len(), LEFTOVERS_N);
    }
    assert_eq!(w.calls("systemctl"), "");
    assert_eq!(w.calls("docker"), "");
    let r = stop(&w, &["5", "6"]);
    rc(&r, 64);
    has(&r.out, "one issue number at most");
    let r = stop(&w, &["5", "--keep"]);
    rc(&r, 64);
    has(&r.out, "unknown argument: --keep");
}

#[test]
fn a_second_stop_finds_nothing_to_do_and_asks_the_store_to_end_nothing() {
    let w = host("second");
    // The record of a cycle a stop already ended, and no lock file left.
    started(&w, 5);
    w.store_ok(&["cycle-end", "5", "--attempt", "1", "--stopped"]);
    fs::remove_file(note(&w, 5, "lock")).unwrap();
    w.store_answers("cycle-end", 1, BUSY, 0);
    let r = stop(&w, &["5"]);
    rc(&r, 0);
    assert_eq!(r.out, "nothing to stop for #5");
    assert_eq!(w.store_answered(), 0);
}

#[test]
fn a_record_that_cannot_be_ended_leaves_the_lock_file_and_the_rest_says_so_and_exits_1() {
    let w = host("end-busy");
    cycle_files(&w, 5);
    started(&w, 5);
    fs::remove_file(note(&w, 5, "stopped")).unwrap();
    w.store_answers("cycle-end", 1, BUSY, 0);
    let r = stop(&w, &["5"]);
    rc(&r, 1);
    has(
        &r.out,
        &format!(
            "the record of #5 could not be ended as stopped ({BUSY}), so nothing was removed. Run this again."
        ),
    );
    there(note(&w, 5, "lock"));
    there(works(&w).join("widget-5"));
    assert!(w.store_ok(&["cycle-show", "5"])["cycle"]["ended_at"].is_null());
}

#[test]
fn a_park_that_cannot_be_cleared_leaves_the_lock_file_and_the_rest_says_so_and_exits_1() {
    for verb in ["question-answered", "park-retry"] {
        let w = host(&format!("park-{verb}"));
        cycle_files(&w, 5);
        fs::remove_file(note(&w, 5, "stopped")).unwrap();
        w.store_answers(verb, 1, BUSY, 0);
        let r = stop(&w, &["5"]);
        rc(&r, 1);
        has(&r.out, "could not be");
        has(
            &r.out,
            &format!("({BUSY}), so nothing was removed. Run this again."),
        );
        there(note(&w, 5, "lock"));
        there(works(&w).join("widget-5"));
        assert_eq!(w.store_ok(&["park-show", "5"])["park"]["retries"], 0);
    }
}

#[test]
fn a_park_that_cannot_be_read_changes_nothing_1_when_busy_78_when_the_store_is_unusable() {
    let w = host("park-read");
    cycle_files(&w, 5);
    w.store_answers("park-show", 1, BUSY, 0);
    let r = stop(&w, &["5"]);
    rc(&r, 1);
    has(
        &r.out,
        "the park of #5 could not be read from the state store: store: busy #5",
    );
    assert_eq!(leftovers(&w, 5).len(), LEFTOVERS_N);
    w.store_answers(
        "park-show",
        78,
        "store: corrupt: the file is not a database",
        0,
    );
    rc(&stop(&w, &["5"]), 78);
    assert_eq!(leftovers(&w, 5).len(), LEFTOVERS_N);
    assert_eq!(unit_stops(&w), 0);
}

#[test]
fn a_record_that_cannot_be_read_changes_nothing_1_when_busy_78_when_there_is_no_store() {
    let w = host("record-read");
    cycle_files(&w, 5);
    w.store_answers("cycle-show", 1, BUSY, 0);
    rc(&stop(&w, &["5"]), 1);
    assert_eq!(leftovers(&w, 5).len(), LEFTOVERS_N);
    fs::remove_file(w.home.join("libexec/store")).unwrap();
    let r = stop(&w, &["5"]);
    rc(&r, 78);
    has(
        &r.out,
        "ERROR: the state store is not installed (run install.sh)",
    );
    assert_eq!(leftovers(&w, 5).len(), LEFTOVERS_N);
    assert_eq!(unit_stops(&w), 0);
}
