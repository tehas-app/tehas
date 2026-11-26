//! `tehas stop` end to end: what a cycle leaves on the host, what a stop
//! removes, keeps and refuses to touch, and the state store that cannot end,
//! clear or read the cycle's record. systemctl and docker are stubs. The
//! cases that reach no GitHub run with gh unstubbed, so a call to it fails
//! them; the stops that go on to the issue have a stub gh that answers its
//! REST read. What is asked of the issue's running mark is on the ports'
//! fake in console/ops/src/stop.rs.

mod common;

use common::*;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
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

// --- End to end with gh: the stops that go on to the issue ------------------

/// gh: the issue's REST read answers $T/issue.json, the question of the
/// ticket it is a part of answers none, and the label's removal answers
/// nothing.
const GH: &str = r#"case "$*" in *"parent {"*) acc=""
  for n in $(printf %s "$*" | grep -o " n[0-9][0-9]*: issue(number:" | sed "s/^ n\([0-9]*\):.*/\1/"); do
    acc="$acc,\"n$n\":{\"number\":$n,\"parent\":null}"
  done
  echo "{\"data\":{\"repository\":{${acc#,}}}}"; exit 0 ;; esac
case "$1 $2" in
  "api repos/"*) cat "$T/issue.json" ;;
esac"#;
/// systemctl: the unit is active, and its stop ends the process that holds
/// the issue's lock, as a unit that stopped does.
const SYSTEMCTL_ENDS: &str = r#"case "$2" in
  show) printf "%s\n" "${SHOW:-active}" ;;
  stop) kill "$(cat "$T/holder" 2>/dev/null)" 2>/dev/null; : ;;
esac"#;

/// A host as `host` makes it, with gh answering an issue of no labels: a stop
/// that stops nothing on this host then asks GitHub for nothing to change.
fn gh_host(name: &str) -> World {
    let w = host(name);
    w.gh_stub(GH);
    labelled(&w, &[]);
    w
}

/// The label of a running cycle, as host/labels.tsv names it.
fn running_label() -> String {
    let table = fs::read_to_string(root().join("host/labels.tsv")).unwrap();
    table
        .lines()
        .map(|l| l.split('\t').collect::<Vec<_>>())
        .find(|f| f.last() == Some(&"State::Running"))
        .map(|f| f[0].to_string())
        .unwrap()
}

/// The issue gh reports, #5 with these labels.
fn labelled(w: &World, labels: &[&str]) {
    let names: Vec<String> = labels
        .iter()
        .map(|l| format!(r#"{{"name":"{l}"}}"#))
        .collect();
    w.fx(
        "issue.json",
        &format!(
            r#"{{"number":5,"title":"t","state":"open","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","html_url":"https://example.com/5","labels":[{}]}}"#,
            names.join(",")
        ),
    );
}

/// The process that holds the lock of a cycle: the stub systemctl's stop
/// ends it by the pid in $T/holder. Killed when the value drops.
struct Holder(Child);

impl Drop for Holder {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A cycle of `n` that is running: its lock, held by a process the stubbed
/// stop ends, which writes a marker once it has the lock. A holder that has
/// not locked yet is a cycle the stop finds no lock for, so this waits.
fn holder(w: &World, n: u32) -> Holder {
    fs::create_dir_all(state(w)).unwrap();
    let held = note(w, n, "held");
    let child = Command::new("bash")
        .args([
            "-c",
            r#"exec 9>"$1"; flock -n 9 || exit 1; : >"$2"; exec sleep 30"#,
            "holder",
        ])
        .arg(note(w, n, "lock"))
        .arg(&held)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    fs::write(w.t().join("holder"), child.id().to_string()).unwrap();
    let h = Holder(child);
    let until = Instant::now() + Duration::from_secs(10);
    while !held.exists() {
        assert!(
            Instant::now() < until,
            "the holder of #{n} never took its lock"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    h
}

/// The record files of `n` a stop keeps, still on the host.
fn record(w: &World, n: u32) -> Vec<&'static str> {
    KEEP.iter()
        .copied()
        .filter(|e| note(w, n, e).exists())
        .collect()
}

fn marker(w: &World, n: u32) -> bool {
    note(w, n, "stopped").exists()
}

#[test]
fn a_running_cycle_is_stopped_its_leftovers_go_its_label_comes_off_and_the_record_stays() {
    let w = gh_host("running");
    cycle_files(&w, 5);
    let _cycle = holder(&w, 5);
    w.stub("systemctl", SYSTEMCTL_ENDS);
    w.set_env(&["CONTAINERS=tehas-work-5 tehas-work-51"]);
    let label = running_label();
    labelled(&w, &[&label, "bug"]);
    let began = Instant::now();
    let r = stop(&w, &["5"]);
    rc(&r, 0);
    assert!(began.elapsed() < Duration::from_secs(30));
    assert_eq!(leftovers(&w, 5), Vec::<String>::new());
    assert_eq!(record(&w, 5), KEEP);
    assert_eq!(removed_containers(&w), "tehas-work-5 ");
    for line in [
        "stopped tehas-cycle@5.service",
        "removed the container tehas-work-5",
        "removed state/cycles/5.lock",
        "removed state/cycles/5.cid",
        "took the park question of #5 off the record",
        "spent the retry of the park of #5",
        "removed live/console-5.txt",
        "removed ~/.pi/agent/ensemble-runs/issue-5",
        "removed work/widget-5",
        "removed work/widget-5.state",
        "removed work/widget-5.images",
        "removed work/widget-5.before-retry",
    ] {
        has(&r.out, line);
    }
    assert_eq!(
        w.store_ok(&["question-known", "5", "--id", "425"])["recorded"],
        false
    );
    // The issue's half comes after the host's: the last line said.
    let off = format!("removed {label} from #5");
    assert_eq!(r.out.lines().last(), Some(off.as_str()));
    assert_eq!(unit_stops(&w), 1);
    // The issue is read once, with the question of its parent, and its
    // label taken off once: no other label, no comment. What is asked of the port is on its fake in
    // console/ops/src/stop.rs.
    assert_eq!(w.gh().lines().count(), 3, "{}", w.gh());
}

#[test]
fn the_marker_of_a_stop_goes_with_the_leftovers_of_the_cycle_it_ended() {
    // What the stop wrote goes with everything else the cycle left, so a
    // cycle started later on this issue reads none of it as its own.
    let w = gh_host("marker-goes");
    cycle_files(&w, 5);
    let _cycle = holder(&w, 5);
    w.stub("systemctl", SYSTEMCTL_ENDS);
    labelled(&w, &[&running_label()]);
    let r = stop(&w, &["5"]);
    rc(&r, 0);
    has(&r.out, "stopped tehas-cycle@5.service");
    has(&r.out, "removed state/cycles/5.stopped");
    assert!(!marker(&w, 5));
    assert_eq!(leftovers(&w, 5), Vec::<String>::new());
}

#[test]
fn a_container_with_no_unit_and_no_lock_behind_it_goes_and_no_marker_is_written() {
    // The marker says a person ended a cycle; a container whose cycle left
    // this host long ago is no cycle.
    let w = gh_host("orphan");
    cycle_files(&w, 5);
    w.set_env(&["CONTAINERS=tehas-work-5"]);
    labelled(&w, &[&running_label()]);
    let r = stop(&w, &["5"]);
    rc(&r, 0);
    has(&r.out, "removed the container tehas-work-5");
    assert!(!marker(&w, 5));
    assert_eq!(leftovers(&w, 5), Vec::<String>::new());
}

#[test]
fn a_marker_left_by_an_earlier_stop_is_a_leftover_and_goes() {
    let w = gh_host("old-marker");
    cycle_files(&w, 5);
    fs::remove_file(note(&w, 5, "stopped.new")).unwrap();
    w.set_env(&["CONTAINERS=tehas-work-5"]);
    let r = stop(&w, &["5"]);
    rc(&r, 0);
    assert!(!marker(&w, 5));
    has(&r.out, "removed state/cycles/5.stopped");
}

#[test]
fn a_run_that_stops_nothing_writes_no_marker_and_takes_one_that_is_there() {
    let w = gh_host("no-stop");
    write(
        note(&w, 5, "stopped"),
        &format!("{} {}\n", tehas_core::time::now(), whoami()),
    );
    let r = stop(&w, &["5"]);
    rc(&r, 0);
    has(&r.out, "removed state/cycles/5.stopped");
    assert!(!marker(&w, 5));
    assert_eq!(unit_stops(&w), 0);
}

#[test]
fn a_marker_that_cannot_be_written_is_said_and_the_stop_goes_through_all_the_same() {
    let w = gh_host("unwritable-marker");
    cycle_files(&w, 5);
    fs::remove_file(note(&w, 5, "stopped")).unwrap();
    let _cycle = holder(&w, 5);
    w.stub("systemctl", SYSTEMCTL_ENDS);
    // The write goes through the name beside the marker and is moved onto a
    // name that is a directory, which no move can replace.
    fs::create_dir(note(&w, 5, "stopped")).unwrap();
    labelled(&w, &[&running_label()]);
    let r = stop(&w, &["5"]);
    rc(&r, 0);
    has(&r.out, "cycles/5.stopped could not be written");
    has(&r.out, "stopped tehas-cycle@5.service");
    has(&r.out, &format!("removed {} from #5", running_label()));
    assert_eq!(unit_stops(&w), 1);
    // The name that could not be written is taken, and its sibling too: no
    // part of a write is left for a later cycle.
    assert!(!note(&w, 5, "stopped").exists());
    assert!(!note(&w, 5, "stopped.new").exists());
}

#[test]
fn stopping_5_touches_nothing_of_51_or_55_and_asks_gh_of_5_alone() {
    let w = gh_host("siblings");
    for n in [5, 51, 55] {
        cycle_files(&w, n);
    }
    w.set_env(&["CONTAINERS=tehas-work-5 tehas-work-51 tehas-work-55"]);
    labelled(&w, &[&running_label()]);
    let r = stop(&w, &["5"]);
    rc(&r, 0);
    assert_eq!(leftovers(&w, 5), Vec::<String>::new());
    assert_eq!(leftovers(&w, 51).len(), LEFTOVERS_N);
    assert_eq!(leftovers(&w, 55).len(), LEFTOVERS_N);
    assert_eq!(removed_containers(&w), "tehas-work-5 ");
    assert_eq!(w.gh().lines().count(), 3, "{}", w.gh());
    for other in ["51", "55"] {
        lacks(&w.gh(), other);
        lacks(&r.out, other);
    }
}

#[test]
fn keep_workspace_keeps_the_workspace_and_what_is_beside_it_and_takes_the_rest() {
    let w = gh_host("keep");
    cycle_files(&w, 5);
    w.set_env(&["CONTAINERS=tehas-work-5"]);
    let r = stop(&w, &["5", "--keep-workspace"]);
    rc(&r, 0);
    let ws = works(&w);
    there(ws.join("widget-5/README.md"));
    let kept: Vec<String> = [
        "widget-5",
        "widget-5.state",
        "widget-5.before-retry",
        "widget-5.images",
    ]
    .iter()
    .map(|p| ws.join(p).display().to_string())
    .collect();
    assert_eq!(leftovers(&w, 5), kept);
    has(&r.out, "spent the retry of the park of #5");
    has(&r.out, "removed state/cycles/5.result.json");
    lacks(&r.out, "removed work/widget-5");
}

#[test]
fn the_container_of_a_cycle_that_left_no_note_goes_too() {
    let w = gh_host("bare-container");
    fs::create_dir_all(state(&w)).unwrap();
    w.set_env(&["CONTAINERS=tehas-work-9"]);
    let r = stop(&w, &["9"]);
    rc(&r, 0);
    has(&r.out, "removed the container tehas-work-9");
}

#[test]
fn a_second_run_finds_nothing_to_do_and_asks_github_for_nothing() {
    let w = gh_host("again");
    cycle_files(&w, 5);
    w.set_env(&["CONTAINERS=tehas-work-5"]);
    labelled(&w, &[&running_label()]);
    rc(&stop(&w, &["5"]), 0);
    let asked = w.gh();
    let r = stop(&w, &["5"]);
    rc(&r, 0);
    assert_eq!(r.out, "nothing to stop for #5");
    assert_eq!(unit_stops(&w), 0);
    assert_eq!(removed_containers(&w), "tehas-work-5 ");
    // A host with nothing to clear is not sent to GitHub.
    assert_eq!(w.gh(), asked);
}

#[test]
fn no_instance_settings_is_78_with_nothing_removed_and_nothing_called() {
    let w = gh_host("no-instance");
    w.no_instance();
    cycle_files(&w, 5);
    let r = stop(&w, &["5"]);
    rc(&r, 78);
    has(&r.out, "no instance.env");
    assert_eq!(leftovers(&w, 5).len(), LEFTOVERS_N);
    assert_eq!(w.calls("systemctl"), "");
    assert_eq!(w.gh(), "");
}

#[test]
fn help_prints_the_usage_and_does_nothing() {
    let w = gh_host("help");
    cycle_files(&w, 5);
    w.stub("systemctl", SYSTEMCTL_LIVE);
    for flag in ["-h", "--help"] {
        let r = stop(&w, &[flag]);
        rc(&r, 0);
        has(&r.out, "Usage:");
        has(&r.out, "tehas stop <issue> --keep-workspace");
        lacks(&r.out, "set -uo pipefail");
    }
    let r = w.tehas("help", &["stop"]);
    rc(&r, 0);
    has(&r.out, "tehas stop <issue>");
    assert_eq!(leftovers(&w, 5).len(), LEFTOVERS_N);
    assert_eq!(w.calls("systemctl"), "");
}

#[test]
fn help_among_the_arguments_is_help_with_no_settings_to_load() {
    let w = gh_host("help-args");
    w.no_instance();
    let r = stop(&w, &["5", "--help"]);
    rc(&r, 0);
    has(&r.out, "Usage:");
    lacks(&r.out, "no instance.env");
    let r = stop(&w, &["5", "6", "--help"]);
    rc(&r, 64);
    has(&r.out, "one issue number at most");
}

#[test]
fn it_names_only_the_paths_of_the_issue_it_was_given_never_a_glob() {
    let w = gh_host("glob");
    cycle_files(&w, 5);
    let ws = works(&w);
    fs::create_dir_all(ws.join("widget-5x")).unwrap();
    fs::create_dir_all(ws.join("other-5")).unwrap();
    write(state(&w).join("5.5.lock"), "x\n");
    rc(&stop(&w, &["5"]), 0);
    there(ws.join("widget-5x"));
    there(state(&w).join("5.5.lock"));
    assert_eq!(record(&w, 5), KEEP);
}

#[test]
fn a_cycle_killed_outright_has_its_record_ended_as_stopped_before_its_lock_file_goes() {
    // Its start is on record and never reported: no unit, no lock held, and
    // the lock file it left.
    let w = gh_host("killed");
    started(&w, 5);
    let lock = note(&w, 5, "lock");
    write(&lock, "");
    // The cycle-end is seen as it happens: a line with no times left answers
    // nothing, so the real store ends the record, but still counts the call
    // in store-answers.n. That count is a symlink to the lock file, so it is
    // written into the lock file's inode when the lock file is there at the
    // cycle-end. lock-seen is a second name of that inode, kept after the
    // stop removes the lock file.
    w.fx("store-answers", "cycle-end|0|unused|0\n");
    let seen = w.t().join("lock-seen");
    fs::hard_link(&lock, &seen).unwrap();
    std::os::unix::fs::symlink(&lock, w.t().join("store-answers.n")).unwrap();
    let r = stop(&w, &["5"]);
    rc(&r, 0);
    has(&r.out, "ended the record of the cycle of #5 as stopped");
    let cycle = &w.store_ok(&["cycle-show", "5"])["cycle"];
    assert!(!cycle["ended_at"].is_null());
    assert_eq!(cycle["end_reason"], "stopped");
    // The end came first: the lock file was there at the cycle-end, and is gone.
    assert_eq!(fs::read_to_string(&seen).unwrap().trim(), "1");
    assert!(!lock.exists());
    // Its one line of history says it was stopped (T11.3).
    let rows = w.store_ok(&["history-show", "5"])["rows"].clone();
    let status: Vec<&str> = rows
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["status"].as_str().unwrap())
        .collect();
    assert_eq!(status, ["stopped"]);
}

#[test]
fn a_stop_spends_the_retry_a_provider_outage_left_owed() {
    let w = gh_host("provider");
    // Started on an approval, not by hand: a start by hand is owed no retry.
    let sha = "0".repeat(64);
    w.store_ok(&[
        "approval-record",
        "5",
        "--at",
        "1760000000",
        "--by",
        "host",
        "--how",
        "host",
        "--body-sha",
        &sha,
    ]);
    locked(&w, 5, &["cycle-start", "5"]);
    locked(
        &w,
        5,
        &[
            "cycle-end",
            "5",
            "--attempt",
            "1",
            "--status",
            "failed",
            "--provider",
            "--parked",
        ],
    );
    w.store_ok(&["park", "5", "--attempt", "1", "--reason", "provider-outage"]);
    assert_eq!(w.store_ok(&["park-show", "5"])["park"]["automatic"], true);
    let r = stop(&w, &["5"]);
    rc(&r, 0);
    has(&r.out, "spent the retry of the park of #5");
    lacks(&r.out, "park question");
    let park = &w.store_ok(&["park-show", "5"])["park"];
    assert_eq!(
        (&park["automatic"], &park["retries"]),
        (&false.into(), &1.into())
    );
    // The dispatcher starts nothing on its own.
    let auto = w.store_ok(&["park-show", "--automatic"]);
    assert_eq!(auto["parks"].as_array().map(Vec::len), Some(0));
    assert_eq!(stop(&w, &["5"]).out, "nothing to stop for #5");
}
