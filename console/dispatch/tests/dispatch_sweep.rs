//! The workspaces, transcripts and console file a cycle leaves that are
//! removed, and cycles that have gone quiet.
//!
//! Each issue a case leaves something of is open on GitHub, unless the case
//! closes it: what a closed issue left goes at once.

mod common;
mod quiet;

use common::*;
use quiet::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn works(w: &World) -> PathBuf {
    w.home.join("work")
}

/// The agents' transcripts of the cycles and planning sessions.
fn runs(w: &World) -> PathBuf {
    w.t().join("home/.pi/agent/ensemble-runs")
}

fn live(w: &World) -> PathBuf {
    w.home.join("live")
}

fn touch_d(age: &str, paths: &[PathBuf]) {
    let ok = Command::new("touch")
        .args(["-d", age])
        .args(paths)
        .status()
        .unwrap();
    assert!(ok.success(), "touch -d {age} {paths:?}");
}

fn open_issue(w: &World, n: u64) {
    w.fx(
        &format!("work-{n}.json"),
        &format!("{{\"number\":{n},\"state\":\"open\"}}\n"),
    );
}

fn closed_issue(w: &World, n: u64) {
    w.fx(
        &format!("work-{n}.json"),
        &format!("{{\"number\":{n},\"state\":\"closed\"}}\n"),
    );
}

/// Issue `n`'s workspace and those named beside it, in a marked directory of
/// workspaces, with its lock, all of `age`.
fn kept(w: &World, n: u64, age: &str, beside: &[&str]) {
    open_issue(w, n);
    fs::create_dir_all(w.cycles_dir()).unwrap();
    fs::create_dir_all(works(w)).unwrap();
    fs::write(works(w).join(".tehas-workspaces"), "").unwrap();
    let mut paths = vec![works(w).join(format!("widget-{n}"))];
    paths.extend(beside.iter().map(|b| works(w).join(b)));
    for p in &paths {
        fs::create_dir_all(p).unwrap();
    }
    let lock = w.cycles_dir().join(format!("{n}.lock"));
    fs::write(&lock, "").unwrap();
    paths.push(lock);
    touch_d(age, &paths);
}

/// Issue `n`'s workspace and all three beside it, eight days old.
fn old(w: &World, n: u64) {
    kept(
        w,
        n,
        "8 days ago",
        &[
            &format!("widget-{n}.before-retry"),
            &format!("widget-{n}.images"),
            &format!("widget-{n}.state"),
        ],
    );
}

/// What is left in the directory of workspaces, each followed by a space.
fn left(w: &World) -> String {
    w.files(&works(w))
        .into_iter()
        .filter(|f| f != ".tehas-workspaces")
        .map(|f| f + " ")
        .collect()
}

/// What the poll said it took, one issue each.
fn swept(out: &str) -> String {
    let re = regex::Regex::new(r"what #[0-9]* left on this host").unwrap();
    re.find_iter(out)
        .map(|m| format!("{} ", m.as_str()))
        .collect()
}

/// The three a cycle leaves outside its workspace: the transcripts of the
/// cycle and of its planning session, and the console the cycle wrote.
fn three(w: &World, n: u64) -> [PathBuf; 3] {
    [
        runs(w).join(format!("issue-{n}")),
        runs(w).join(format!("plan-{n}")),
        live(w).join(format!("console-{n}.txt")),
    ]
}

/// Issue `n` left those three, of `age`.
fn said(w: &World, n: u64, age: &str) {
    open_issue(w, n);
    let [issue, plan, console] = three(w, n);
    for d in [&issue, &plan] {
        fs::create_dir_all(d).unwrap();
    }
    fs::create_dir_all(live(w)).unwrap();
    fs::write(issue.join("a-1-coder.json"), "transcript\n").unwrap();
    fs::write(plan.join("p-1-planner.json"), "transcript\n").unwrap();
    fs::write(&console, "console line\n").unwrap();
    touch_d(age, &[issue, plan, console]);
}

/// What issue `n` still has of the three, by path.
fn left_said(w: &World, n: u64) -> String {
    three(w, n)
        .iter()
        .filter(|p| p.exists())
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

/// All three of issue `n`, as left_said gives them.
fn all_said(w: &World, n: u64) -> String {
    three(w, n)
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

fn rm_works(w: &World) {
    let _ = fs::remove_dir_all(works(w));
}

/// Issue `n`'s lock is a link to a file a person reads.
fn lock_link(w: &World, n: u64) -> PathBuf {
    fs::create_dir_all(w.cycles_dir()).unwrap();
    let lock = w.cycles_dir().join(format!("{n}.lock"));
    let _ = fs::remove_file(&lock);
    let target = w.t().join("target");
    fs::write(&target, "a person reads this\n").unwrap();
    std::os::unix::fs::symlink(&target, &lock).unwrap();
    target
}

#[test]
fn a_workspace_no_cycle_has_used_for_seven_days_goes_with_what_is_beside_it_its_transcripts_and_its_console_and_that_is_logged(
) {
    let w = World::new("sweep-seven-days");
    rm_works(&w);
    old(&w, 5);
    said(&w, 5, "8 days ago");
    kept(&w, 6, "1 day ago", &["widget-6.before-retry"]);
    said(&w, 6, "8 days ago");
    let other = [works(&w).join("widget-5x"), works(&w).join("other-5")];
    for p in &other {
        fs::create_dir_all(p).unwrap();
    }
    touch_d("8 days ago", &other);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        left(&w),
        "other-5 widget-5x widget-6 widget-6.before-retry "
    );
    assert_eq!(left_said(&w, 5), "");
    assert_eq!(left_said(&w, 6), all_said(&w, 6));
    has(
        &r.out,
        "removed what #5 left on this host: no cycle has run on it for 7 days",
    );
    assert_eq!(swept(&r.out), "what #5 left on this host ");
}

#[test]
fn a_workspace_whose_issue_s_lock_is_held_is_kept_however_old_and_so_are_its_transcripts_and_its_console(
) {
    let w = World::new("sweep-held");
    rm_works(&w);
    old(&w, 5);
    said(&w, 5, "8 days ago");
    w.hold(5);
    touch_d(
        "8 days ago",
        &[works(&w).join("widget-5"), w.cycles_dir().join("5.lock")],
    );
    let r = w.poll();
    w.release();
    assert_eq!(
        left(&w),
        "widget-5 widget-5.before-retry widget-5.images widget-5.state "
    );
    assert_eq!(left_said(&w, 5), all_said(&w, 5));
    assert_eq!(swept(&r.out), "");
}

// An orphan: the workspace is gone, the three beside it are not.
#[test]
fn an_issue_s_transcripts_and_console_with_no_workspace_left_go_on_their_own_and_that_is_logged() {
    let w = World::new("sweep-orphan");
    rm_works(&w);
    kept(&w, 5, "8 days ago", &[]);
    fs::remove_dir_all(works(&w).join("widget-5")).unwrap();
    said(&w, 5, "8 days ago");
    said(&w, 51, "8 days ago");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        swept(&r.out),
        "what #5 left on this host what #51 left on this host "
    );
    assert_eq!(left_said(&w, 5), "");
    assert_eq!(left_said(&w, 51), "");
    assert_eq!(left(&w), "");
    has(
        &r.out,
        "removed what #5 left on this host: no cycle has run on it for 7 days",
    );
}

// The same three, touched too recently: the age rule reads them as any other.
#[test]
fn transcripts_and_a_console_written_within_seven_days_are_kept() {
    let w = World::new("sweep-young-said");
    rm_works(&w);
    said(&w, 5, "8 days ago");
    said(&w, 6, "1 day ago");
    let r = w.poll();
    assert_eq!(left_said(&w, 5), "");
    assert_eq!(left_said(&w, 6), all_said(&w, 6));
    assert_eq!(swept(&r.out), "what #5 left on this host ");
}

#[test]
fn what_a_closed_issue_left_goes_at_once_however_young_and_an_open_one_s_is_kept() {
    let w = World::new("sweep-closed");
    rm_works(&w);
    kept(
        &w,
        5,
        "1 hour ago",
        &["widget-5.before-retry", "widget-5.state"],
    );
    said(&w, 5, "1 hour ago");
    closed_issue(&w, 5);
    kept(&w, 6, "1 hour ago", &[]);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(left(&w), "widget-6 ");
    assert_eq!(left_said(&w, 5), "");
    has(
        &r.out,
        "removed what #5 left on this host: its issue is closed",
    );
    assert_eq!(swept(&r.out), "what #5 left on this host ");
}

#[test]
fn a_closed_issue_whose_lock_is_held_keeps_what_it_left() {
    let w = World::new("sweep-closed-held");
    rm_works(&w);
    kept(&w, 5, "1 hour ago", &[]);
    closed_issue(&w, 5);
    w.hold(5);
    w.poll();
    w.release();
    assert_eq!(left(&w), "widget-5 ");
}

#[test]
fn tehas_workspace_keep_days_sets_the_days_an_open_issue_s_workspace_is_kept() {
    let w = World::new("sweep-keep-days");
    let mut lines = MINIMAL.to_vec();
    lines.extend(["TEHAS_MAX_CYCLES=1", "TEHAS_WORKSPACE_KEEP_DAYS=2"]);
    w.instance(&lines);
    rm_works(&w);
    kept(&w, 5, "3 days ago", &[]);
    kept(&w, 6, "1 day ago", &[]);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(left(&w), "widget-6 ");
    has(
        &r.out,
        "removed what #5 left on this host: no cycle has run on it for 2 days",
    );
}

// The dated directories are the runner's, not tehas's: no rule of the sweep
// names them and a poll leaves them whole.
#[test]
fn a_dated_directory_of_ensemble_runs_is_kept_however_old_and_says_nothing() {
    let w = World::new("sweep-dated");
    rm_works(&w);
    let dated = [runs(&w).join("2026-10-08"), runs(&w).join("2026-10-11")];
    for d in &dated {
        fs::create_dir_all(d).unwrap();
    }
    let handoff = dated[0].join("handoff:ops:8.json");
    fs::write(&handoff, "t\n").unwrap();
    touch_d("30 days ago", &dated);
    said(&w, 5, "8 days ago");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    let kept = w
        .files(&runs(&w))
        .into_iter()
        .filter(|f| f.starts_with("2026-"))
        .count();
    assert_eq!(kept, 2);
    assert_eq!(fs::read_to_string(&handoff).unwrap(), "t\n");
    assert_eq!(left_said(&w, 5), "");
    assert_eq!(swept(&r.out), "what #5 left on this host ");
}

// A poll takes three issues at most, and the orphans count toward that with
// the rest: the four oldest issues leave one of them for the next poll.
#[test]
fn orphans_count_toward_the_three_issues_a_poll_takes() {
    let w = World::new("sweep-orphans-three");
    // The queue is empty and one slot is free, so this case is about the
    // sweep alone.
    w.slots(1, &[]);
    w.fx("ready.json", "[]");
    w.fx("marked.json", "[]");
    rm_works(&w);
    for n in [2, 3, 4, 5] {
        said(&w, n, "8 days ago");
    }
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    for n in [2, 3, 4] {
        assert_eq!(left_said(&w, n), "");
    }
    assert_eq!(left_said(&w, 5), all_said(&w, 5));
    w.poll();
    assert_eq!(left_said(&w, 5), "");
}

// The marker is what keeps a workspace from going; the three beside an issue
// are not workspaces, so an unmarked directory loses them and keeps its
// clones.
#[test]
fn a_directory_tehas_work_did_not_mark_loses_an_issue_s_transcripts_and_console_and_keeps_its_workspaces(
) {
    let w = World::new("sweep-unmarked");
    rm_works(&w);
    old(&w, 5);
    said(&w, 5, "8 days ago");
    fs::remove_file(works(&w).join(".tehas-workspaces")).unwrap();
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        left(&w),
        "widget-5 widget-5.before-retry widget-5.images widget-5.state "
    );
    assert_eq!(left_said(&w, 5), "");
    has(
        &r.out,
        "removed what #5 left on this host: no cycle has run on it for 7 days",
    );
}

#[test]
fn a_workspace_moved_aside_recently_keeps_the_issue_s_workspaces() {
    let w = World::new("sweep-moved-aside");
    rm_works(&w);
    old(&w, 5);
    touch_d("now", &[works(&w).join("widget-5.before-retry")]);
    w.poll();
    assert_eq!(
        left(&w),
        "widget-5 widget-5.before-retry widget-5.images widget-5.state "
    );
}

// A lock that is no lock file says nothing about the issue and taking it would
// write through it. It is flock-able and no cycle holds it, so the poll has
// to notice the type, and it says so rather than removing in silence.
#[test]
fn a_lock_that_is_no_lock_file_keeps_the_whole_issue_and_what_it_points_at_is_not_touched() {
    let w = World::new("sweep-lock-link");
    rm_works(&w);
    old(&w, 5);
    said(&w, 5, "8 days ago");
    let target = lock_link(&w, 5);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "a person reads this\n"
    );
    assert_eq!(
        left(&w),
        "widget-5 widget-5.before-retry widget-5.images widget-5.state "
    );
    assert_eq!(left_said(&w, 5), all_said(&w, 5));
    assert_eq!(swept(&r.out), "");
    has(
        &r.out,
        "the lock of #5 is no lock file, so nothing of it was removed",
    );
}

/// find dates one of `names` within seven days.
fn dates_young(names: &[PathBuf]) -> bool {
    let out = Command::new("find")
        .args(names)
        .args(["-maxdepth", "0", "-newermt", "-7 days"])
        .stderr(std::process::Stdio::null())
        .output()
        .unwrap();
    !out.stdout.is_empty()
}

// A name the poll cannot date is not a name it proved old, so the issue is
// kept for the next poll. The name that cannot be dated sits behind a
// directory of no rights, which prints nothing at all to one find over the
// whole set. The poll runs once while the name cannot be read, and once after.
#[test]
fn an_issue_the_poll_cannot_date_is_kept_and_the_whole_of_it_goes_once_it_can() {
    let w = World::new("sweep-undatable");
    rm_works(&w);
    let wk = works(&w);
    let undatable = wk.join("undatable");
    fs::create_dir_all(&undatable).unwrap();
    fs::create_dir_all(w.cycles_dir()).unwrap();
    fs::write(wk.join(".tehas-workspaces"), "").unwrap();
    let names = [
        "widget-5",
        "widget-5.before-retry",
        "widget-5.images",
        "widget-5.state",
    ];
    for n in names {
        fs::create_dir_all(wk.join(n)).unwrap();
    }
    said(&w, 5, "8 days ago");
    for n in [2, 3, 4] {
        said(&w, n, "8 days ago");
    }
    let lock = w.cycles_dir().join("5.lock");
    fs::write(&lock, "").unwrap();
    touch_d("8 days ago", std::slice::from_ref(&lock));
    for n in names {
        fs::rename(wk.join(n), undatable.join(n)).unwrap();
    }
    let mode = |m: u32| fs::set_permissions(&undatable, fs::Permissions::from_mode(m)).unwrap();
    mode(0o000);
    // The rule needs two things, and the case shows both: the poll could not
    // date the name of its own accord, and one find over all of the issue's
    // names printed nothing, which is the answer that reads as aged out.
    let mut all: Vec<PathBuf> = vec![lock];
    all.extend(names.iter().map(|n| wk.join(n)));
    all.extend(three(&w, 5));
    let one = undatable.join("widget-5");
    let bad = if dates_young(&all) {
        Some("one find over the issue's names dated something, so the case does not show the rule")
    } else if one.exists() || one.is_symlink() {
        Some("the poll can read the directory the test gave it 000, so it cannot be denied a date")
    } else if Command::new("find")
        .arg(&one)
        .args(["-maxdepth", "0", "-newermt", "-7 days"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success()
    {
        Some("find dates a name the test cannot read, so the age rule cannot be shown failing")
    } else {
        None
    };
    if let Some(why) = bad {
        mode(0o755);
        panic!("{why}");
    }
    let r = w.poll();
    mode(0o755);
    assert_eq!(r.rc, 0, "{}", r.out);
    // #5 is kept and costs the poll nothing: the three it took are the others.
    assert_eq!(
        swept(&r.out),
        "what #2 left on this host what #3 left on this host what #4 left on this host "
    );
    assert_eq!(left_said(&w, 5), all_said(&w, 5));
    let r = w.poll();
    assert_eq!(left_said(&w, 5), "");
    has(
        &r.out,
        "removed what #5 left on this host: no cycle has run on it for 7 days",
    );
}

// The age rule reads the names of the issue one by one: one name touched today
// keeps the whole set for another 7 days.
#[test]
fn an_issue_is_kept_while_one_name_of_it_is_young_whatever_the_lock_says() {
    let w = World::new("sweep-one-young");
    rm_works(&w);
    old(&w, 5);
    said(&w, 5, "8 days ago");
    touch_d("now", &[live(&w).join("console-5.txt")]);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        left(&w),
        "widget-5 widget-5.before-retry widget-5.images widget-5.state "
    );
    assert_eq!(left_said(&w, 5), all_said(&w, 5));
    assert_eq!(swept(&r.out), "");
}

// The issue whose lock is no lock file is passed over before the three of a
// poll are spent, so the others of the same poll still go.
#[test]
fn an_issue_whose_lock_is_no_lock_file_does_not_spend_one_of_the_three_a_poll_takes() {
    let w = World::new("sweep-link-three");
    rm_works(&w);
    for n in [2, 3, 4, 5] {
        said(&w, n, "8 days ago");
    }
    let target = lock_link(&w, 5);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        swept(&r.out),
        "what #2 left on this host what #3 left on this host what #4 left on this host "
    );
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "a person reads this\n"
    );
    assert_eq!(left_said(&w, 5), all_said(&w, 5));
    fs::remove_file(w.cycles_dir().join("5.lock")).unwrap();
    assert_eq!(swept(&w.poll().out), "what #5 left on this host ");
}

#[test]
fn a_dry_run_says_what_would_go_and_removes_nothing() {
    let w = World::new("sweep-dry-run");
    rm_works(&w);
    old(&w, 5);
    said(&w, 5, "8 days ago");
    let r = w.dispatch(&["--dry-run"]);
    has(
        &r.out,
        "would remove what #5 left on this host: no cycle has run on it for 7 days",
    );
    assert_eq!(
        left(&w),
        "widget-5 widget-5.before-retry widget-5.images widget-5.state "
    );
    assert_eq!(left_said(&w, 5), all_said(&w, 5));
}

#[test]
fn a_poll_removes_the_workspaces_of_three_issues_at_most() {
    let w = World::new("sweep-three-most");
    rm_works(&w);
    for n in [2, 3, 4, 5] {
        kept(&w, n, "8 days ago", &[]);
    }
    w.poll();
    assert_eq!(left(&w), "widget-5 ");
    w.poll();
    assert_eq!(left(&w), "");
}

#[test]
fn a_cycle_that_holds_its_lock_and_has_not_been_heard_from_for_20_minutes_is_reported_and_stopped_once(
) {
    let w = World::new("sweep-quiet-once");
    stop_stub(&w, r#"[ ! -e "/proc/$$/fd/8" ] || : >"$T/calls/stop.fd8""#);
    quiet_cycle(&w, 1300, 3600);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "WARN: the cycle for #9 holds its lock but has not been heard from since ",
    );
    has(&r.out, "more than 1200s: stopping tehas-cycle@9.service");
    assert_eq!(stops(&w), "9 ");
    assert!(!fd8(&w));
    assert_eq!(notes(&w), "9 ");
    assert_eq!(note_asks(&w), "1");
    lacks(&w.poll().out, "#9 holds its lock");
    assert_eq!(stops(&w), "9 ");
    assert_eq!(notes(&w), "9 ");
    assert_eq!(note_asks(&w), "1");
}

/// The stop was run with the poll's fd 8 open.
fn fd8(w: &World) -> bool {
    Path::new(&w.t().join("calls/stop.fd8")).exists()
}

#[test]
fn a_stop_that_failed_is_said_recorded_and_asked_again_once_the_wait_has_passed() {
    let w = World::new("sweep-stop-failed");
    stop_stub(
        &w,
        r#"[ ! -e "/proc/$$/fd/8" ] || : >"$T/calls/stop.fd8"; exit 1"#,
    );
    quiet_cycle(&w, 1300, 3600);
    seen_long_ago(&w);
    let r = w.poll();
    has(
        &r.out,
        "WARN: could not stop tehas-cycle@9.service (exit 1)",
    );
    has(&r.out, "it holds its slot, and the poll asks again");
    assert!(!fd8(&w));
    assert_eq!(note_asks(&w), "1");
    // The wait for the next ask has not passed: this poll says nothing.
    ago(&w, 590);
    lacks(&w.poll().out, "holds its lock but");
    assert_eq!(stops(&w), "9 ");
    // The wait has passed: the same episode is said and the stop asked again.
    ago(&w, 601);
    has(
        &w.poll().out,
        "the poll asks tehas-cycle@9.service to stop again",
    );
    assert_eq!(stops(&w), "9 9 ");
    assert_eq!(note_asks(&w), "2");
}

#[test]
fn a_stop_that_never_works_is_asked_six_times_10_minutes_apart_then_said_once_an_hour_and_left_to_a_person(
) {
    let w = World::new("sweep-stop-never");
    stop_stub(&w, "exit 1");
    quiet_cycle(&w, 1300, 3600);
    seen_long_ago(&w);
    has(&w.poll().out, "stopping tehas-cycle@9.service");
    for _ in 2..=6 {
        ago(&w, 601);
        has(
            &w.poll().out,
            "the poll asks tehas-cycle@9.service to stop again",
        );
    }
    assert_eq!(stops(&w), "9 9 9 9 9 9 ");
    assert_eq!(note_asks(&w), "6");
    // The sixth ask waits for its 10 minutes like the others, and then the
    // poll says the episode is a person's.
    ago(&w, 590);
    lacks(&w.poll().out, "holds its lock but");
    ago(&w, 601);
    let r = w.poll();
    has(
        &r.out,
        "asked 6 times to stop tehas-cycle@9.service and asks no more",
    );
    has(&r.out, "systemctl --user stop tehas-cycle@9.service");
    assert_eq!(stops(&w), "9 9 9 9 9 9 ");
    // The first line is on record as one more: that tells it from the sixth
    // ask.
    assert_eq!(note_asks(&w), "7");
    // That line is said again once an hour, and the stop is never asked again.
    lacks(&w.poll().out, "holds its lock but");
    ago(&w, 3590);
    lacks(&w.poll().out, "holds its lock but");
    for _ in 0..2 {
        ago(&w, 3601);
        has(
            &w.poll().out,
            "asked 6 times to stop tehas-cycle@9.service and asks no more",
        );
        assert_eq!(stops(&w), "9 9 9 9 9 9 ");
    }
}
