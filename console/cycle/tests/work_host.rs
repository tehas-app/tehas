//! What a cycle of tehas work does on the host: the workspace of an earlier
//! cycle, the issue's lock, the live view, what is kept on record and the
//! container's counts of kills and refused starts.

mod work;

use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use work::{git_out, has, lacks, root, slice_id, Run, World, RUNNER};

fn kept(w: &World) -> PathBuf {
    let mut p = w.ws.clone().into_os_string();
    p.push(".before-retry");
    PathBuf::from(p)
}

fn lockf(w: &World) -> PathBuf {
    w.home.join("state/cycles/7.lock")
}

/// Every path under `dir`, at any depth; none when it is missing.
fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    for e in fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(files_under(&p));
        }
        out.push(p);
    }
    out
}

fn ran(w: &World) -> usize {
    fs::read_dir(w.t())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("ran-"))
        .count()
}

fn docker_runs(w: &World) -> usize {
    fs::read_dir(w.path("calls"))
        .map(|d| {
            d.flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with("docker.args."))
                .count()
        })
        .unwrap_or(0)
}

fn all_docker_args(w: &World) -> String {
    let mut s = String::new();
    for e in fs::read_dir(w.path("calls")).unwrap().flatten() {
        if e.file_name().to_string_lossy().starts_with("docker.args.") {
            s.push_str(&fs::read_to_string(e.path()).unwrap());
        }
    }
    s
}

/// The calls are forgotten, as `rm -f "$T"/calls/docker*` does.
fn clear_docker(w: &World) {
    for e in fs::read_dir(w.path("calls")).unwrap().flatten() {
        if e.file_name().to_string_lossy().starts_with("docker") {
            let _ = fs::remove_file(e.path());
        }
    }
}

fn read(p: impl AsRef<Path>) -> String {
    fs::read_to_string(p)
        .unwrap_or_default()
        .trim_end_matches('\n')
        .to_string()
}

fn json_at(p: impl AsRef<Path>) -> Value {
    serde_json::from_str(&fs::read_to_string(p).unwrap()).unwrap()
}

fn running(w: &World) -> Value {
    json_at(w.live.join("running.json"))
}

/// `flock -n <lock> true` succeeds.
fn lock_free(p: &Path) -> bool {
    Command::new("flock")
        .arg("-n")
        .arg(p)
        .arg("true")
        .status()
        .unwrap()
        .success()
}

// --- The workspace of an earlier cycle ----------------------------------------

/// A first cycle, then everything an agent could leave to be run later
/// planted in its workspace, and a second cycle.
fn planted(name: &str) -> (World, Run) {
    let w = World::new(name);
    w.work7();
    // The state file is under a directory of its own: the product's .pi is
    // a tracked file of the clone, not a leftover.
    let ws = &w.ws;
    fs::create_dir_all(ws.join(".git/hooks")).unwrap();
    fs::create_dir_all(ws.join("agent-state")).unwrap();
    let hook = format!("#!/bin/sh\ntouch \"{}\"\n", w.path("ran-hook").display());
    for h in ["post-checkout", "reference-transaction"] {
        let p = ws.join(".git/hooks").join(h);
        fs::write(&p, &hook).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let t = w.t().display().to_string();
    git_out(
        ws,
        &[
            "config",
            "core.fsmonitor",
            &format!("touch {t}/ran-fsmonitor; false"),
        ],
    );
    git_out(
        ws,
        &["config", "core.sshCommand", &format!("touch {t}/ran-ssh")],
    );
    fs::write(ws.join("left-over.txt"), "left\n").unwrap();
    fs::write(
        ws.join("agent-state/9.json"),
        "{\"pipelineState\":{\"status\":\"merged\"}}\n",
    )
    .unwrap();
    w.clear_calls();
    let r = w.work7();
    (w, r)
}

#[test]
fn what_an_earlier_cycle_left_in_the_workspace_is_gone_and_the_host_runs_none_of_it() {
    let (w, r) = planted("host-left");
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(ran(&w), 0);
    for p in ["left-over.txt", "agent-state", ".git/hooks/post-checkout"] {
        assert!(!w.ws.join(p).exists(), "{p} is there");
    }
    // The gate of the product is in the clone, as in the commit it was made at.
    assert_eq!(read(w.ws.join(".pi/verify-cmd")), "scripts/verify.sh");
    let fsmonitor = Command::new("git")
        .arg("-C")
        .arg(&w.ws)
        .args(["config", "--get", "core.fsmonitor"])
        .output()
        .unwrap();
    assert!(!fsmonitor.status.success(), "core.fsmonitor is set");
    assert_eq!(
        git_out(&w.ws, &["rev-parse", "HEAD"]),
        git_out(&w.upstream, &["rev-parse", "main"])
    );
}

#[test]
fn what_a_stopped_cycle_left_is_kept_beside_the_new_workspace_not_deleted() {
    let (w, _) = planted("host-kept");
    let k = kept(&w);
    assert_eq!(read(k.join("left-over.txt")), "left");
    assert_eq!(
        git_out(&k, &["config", "--get", "core.fsmonitor"]),
        format!("touch {}/ran-fsmonitor; false", w.t().display())
    );
}

#[test]
fn the_kept_workspace_is_given_to_no_container() {
    let (w, _) = planted("host-kept-hidden");
    lacks(&all_docker_args(&w), "before-retry");
}

/// The case before, and a second retry.
fn retried(name: &str) -> World {
    let (w, _) = planted(name);
    fs::write(w.ws.join("second.txt"), "second\n").unwrap();
    w.work7();
    w
}

#[test]
fn a_second_retry_keeps_the_latest_stopped_workspace_only() {
    let w = retried("host-second");
    let k = kept(&w);
    assert_eq!(read(k.join("second.txt")), "second");
    assert!(!k.join("left-over.txt").exists());
    let mut names: Vec<String> = fs::read_dir(w.home.join("work"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("widget-7"))
        .collect();
    names.sort();
    // The two, and <ws>.state.
    assert_eq!(
        names,
        ["widget-7", "widget-7.before-retry", "widget-7.state"]
    );
    assert_eq!(ran(&w), 0);
}

#[test]
fn a_kept_workspace_that_cannot_be_replaced_starts_no_cycle() {
    let w = retried("host-kept-held");
    let k = kept(&w);
    let mode = fs::metadata(&k).unwrap().permissions().mode();
    fs::set_permissions(&k, fs::Permissions::from_mode(mode & !0o222)).unwrap();
    let _ = fs::create_dir_all(k.join("held"));
    clear_docker(&w);
    let root_user = String::from_utf8_lossy(&Command::new("id").arg("-u").output().unwrap().stdout)
        .trim()
        == "0";
    let full = fs::read_dir(&k).unwrap().next().is_some();
    let r = (!root_user && full).then(|| w.work7());
    fs::set_permissions(&k, fs::Permissions::from_mode(mode | 0o200)).unwrap();
    if let Some(r) = r {
        assert_eq!(r.rc, 78, "{}", r.out);
        has(
            &r.out,
            &format!(
                "could not make the workspace {}, not starting a cycle",
                w.ws.display()
            ),
        );
        assert_eq!(docker_runs(&w), 0);
    }
}

#[test]
fn a_commit_the_cycle_made_on_its_own_branch_does_not_reach_the_next_cycle() {
    let w = World::new("host-planted-commit");
    w.work7();
    work::commit(&w.ws, "planted", None);
    w.work7();
    let main = git_out(&w.upstream, &["rev-parse", "main"]);
    assert_eq!(git_out(&w.ws, &["rev-parse", "HEAD"]), main);
    assert_eq!(git_out(&w.checkout, &["rev-parse", "main"]), main);
}

// --- The issue's lock ----------------------------------------------------------

/// Another process holds the issue's lock until it is killed.
fn hold_lock(w: &World) -> Child {
    let lock = lockf(w);
    let child = Command::new("flock")
        .arg(&lock)
        .args(["sleep", "30"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    for _ in 0..50 {
        if !lock_free(&lock) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    child
}

#[test]
fn a_cycle_of_the_same_issue_that_holds_its_lock_is_left_alone_exit_5_nothing_moved_no_container() {
    let w = World::new("host-locked");
    w.work7();
    fs::write(w.ws.join("held.txt"), "mine\n").unwrap();
    clear_docker(&w);
    let mut holder = hold_lock(&w);
    let r = w.work7();
    let _ = holder.kill();
    let _ = holder.wait();
    assert_eq!(r.rc, 5, "{}", r.out);
    has(
        &r.out,
        "tehas work: a cycle for #7 is running, not starting another",
    );
    assert_eq!(read(w.ws.join("held.txt")), "mine");
    assert!(!kept(&w).join("held.txt").exists());
    assert_eq!(docker_runs(&w), 0);
    lacks(&w.calls("docker"), "run ");
}

#[test]
fn under_tehasbot_cycle_the_lock_it_holds_is_handed_down_as_fd_9_and_the_cycle_starts() {
    let w = World::new("host-fd9");
    w.work7();
    let r = w.sh(&format!(
        "exec 9>'{}' && flock -n 9 || exit 99; \"{}\" 7",
        lockf(&w).display(),
        w.work_cmd().display()
    ));
    assert_eq!(r.rc, 2, "{}", r.out);
    has(
        &w.cycle_args().join(" "),
        &format!("{RUNNER} work 7 --timeout 360 --json"),
    );
}

#[test]
fn a_run_by_hand_holds_the_issues_lock_while_its_container_runs_and_lets_go_at_the_end() {
    let w = World::new("host-lock-held");
    w.work7();
    let lock = lockf(&w);
    w.fs(
        "container.sh",
        format!(
            "if flock -n '{}' true; then echo free; else echo held; fi >\"$T/lock.seen\"\n",
            lock.display()
        ),
    );
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(read(w.path("lock.seen")), "held");
    assert!(lock_free(&lock));
}

#[test]
fn another_issues_workspace_and_transcripts_are_left_alone() {
    let w = World::new("host-other-issue");
    let files = [
        "home/tehas/work/widget-9/notes.txt",
        "home/tehas/work/widget-9.state/note.json",
        "home/.pi/agent/ensemble-runs/issue-9/a-1-coder.json",
    ];
    for f in files {
        w.fs(f, "keep\n");
    }
    w.work7();
    for f in files {
        assert_eq!(read(w.path(f)), "keep", "{f}");
    }
    let args = all_docker_args(&w);
    lacks(&args, "widget-9");
    lacks(&args, "issue-9");
}

// --- The live view ------------------------------------------------------------

/// The console of the tree is a stub with this body.
fn console_is(w: &World, body: &str) {
    w.stub("console", body);
    fs::rename(w.path("bin/console"), w.tree.join("libexec/console")).unwrap();
}

#[test]
fn the_cycles_status_line_is_the_drivers_last_notice_published_as_the_consoles_mask_leaves_it() {
    let w = World::new("host-status-line");
    w.fs(
        "container.sh",
        r#"cat >&2 <<'LINES'
{"v":1,"kind":"step-started","event":{"step":"develop"}}
{"v":1,"kind":"log","message":"step 3 of the cycle"}
{"v":1,"kind":"log","unstable":true,"message":"not this"}
LINES
"#,
    );
    // This stub is the console's mask: the page's line is what it left in
    // the now file.
    console_is(
        &w,
        r#"now= prev=
for a in "$@"; do [ "$prev" = --now ] && now=$a; prev=$a; done
[ -n "$now" ] || exit 0
sed "s/cycle/[masked]/" >"$now.new" <<NOTICE
develop
step 3 of the cycle
NOTICE
mv "$now.new" "$now""#,
    );
    // The view is published from the cycle's heartbeats.
    w.under_cycle();
    w.work7();
    let v = running(&w);
    assert_eq!(v["last"]["statusText"], "step 3 of the [masked]");
    assert_eq!(v["last"]["step"], "develop");
}

#[test]
fn no_command_writes_to_pi_through_a_fifo() {
    let hits: Vec<PathBuf> = files_under(&root().join("libexec"))
        .into_iter()
        .filter(|p| {
            p.is_file()
                && fs::read(p)
                    .unwrap()
                    .windows(3)
                    .any(|w| w == b">&4".as_slice())
        })
        .collect();
    assert!(hits.is_empty(), "{hits:?}");
}

#[test]
fn without_a_console_the_command_stops_with_78_and_publishes_nothing_of_the_cycle() {
    let w = World::new("host-no-console");
    w.under_cycle();
    w.work7();
    let console = w.tree.join("libexec/console");
    fs::rename(&console, w.path("console.kept")).unwrap();
    w.under_cycle();
    let before = fs::read_to_string(w.live.join("running.json")).unwrap();
    // Only the console of this tree may answer, as on a host.
    let r = w.sh(&format!(
        "env -u TEHAS_WATCH_TICK -u TEHAS_CONSOLE_CMD -u TEHAS_WATCH_CLOCK \"{}\" 7",
        w.work_cmd().display()
    ));
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        &format!(
            "no console beside this command at {}, run install.sh",
            console.display()
        ),
    );
    assert_eq!(
        fs::read_to_string(w.live.join("running.json")).unwrap(),
        before
    );
}

#[test]
fn a_console_that_writes_no_now_file_leaves_the_status_line_empty() {
    let w = World::new("host-no-now");
    // The driver has a notice and the console a line to give: the status is
    // empty only for want of a now file.
    w.fs(
        "container.sh",
        r#"cat >&2 <<'LINES'
{"v":1,"kind":"step-started","event":{"step":"develop"}}
{"v":1,"kind":"log","message":"step 3 of the cycle"}
{"v":1,"kind":"log","unstable":true,"message":"not this"}
LINES
"#,
    );
    w.fs("now.step", "develop");
    w.fs("now.text", "step 3 of the cycle");
    w.touch("now.off");
    w.under_cycle();
    w.work7();
    let v = running(&w);
    assert_eq!(
        json!([v["last"]["statusText"], v["last"]["status"]]),
        json!(["", "running"])
    );
}

/// Another cycle on the store: `running`, `status`, its last heartbeat at
/// `updated` (epoch), started a minute before it.
fn other(w: &World, issue: u64, running: bool, status: &str, updated: i64) {
    let now = tehas_core::time::now() as i64;
    w.live_cycle(
        issue,
        running,
        status,
        "develop",
        now - updated + 60,
        now - updated,
    );
}

fn now() -> i64 {
    tehas_core::time::now() as i64
}

/// The runs of the live view in the order the cases make them, up to `upto`:
/// 1 alone, 2 with 9 running, 3 with 5 ended long ago, 4 with 11 quiet.
fn views(name: &str, upto: u32) -> World {
    let w = World::new(name);
    w.under_cycle();
    w.work7();
    if upto >= 2 {
        other(&w, 9, true, "running", now());
        w.under_cycle();
        w.work7();
    }
    if upto >= 3 {
        other(&w, 5, false, "handoff", 1000);
        w.under_cycle();
        w.work7();
    }
    if upto >= 4 {
        other(&w, 11, true, "running", now() - 300);
        w.under_cycle();
        // Cycle 11 is lost two minutes after its last heartbeat. The container
        // waits until it sees 11 shown as lost, then for the next second.
        w.container(&format!(
            r#"for _ in $(seq 300); do [ "$(jq -r .status "{}/cycles/11.json" 2>/dev/null)" = lost ] && break; "$nap" 0.1; done
s=$(date +%s); while [ "$(date +%s)" = "$s" ]; do "$nap" 0.1; done"#,
            w.live.display()
        ));
        w.work7();
        w.rm("container.sh");
    }
    w
}

#[test]
fn the_cycles_heartbeat_is_published_as_a_live_file_of_its_own_and_the_view_has_it_as_the_last_one()
{
    let w = views("host-view-own", 1);
    let c = json_at(w.live.join("cycles/7.json"));
    assert_eq!(
        json!([c["issue"], c["running"], c["console"]]),
        json!([7, false, "console-7.txt"])
    );
    let v = running(&w);
    assert_eq!(
        json!([v["cycles"].as_array().unwrap().len(), v["last"]["issue"]]),
        json!([0, 7])
    );
}

#[test]
fn another_cycle_that_is_running_is_in_the_view_beside_it() {
    let w = views("host-view-other", 2);
    let v = running(&w);
    let mut got: Vec<Value> = v["cycles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["issue"].clone())
        .collect();
    got.push(v["last"]["issue"].clone());
    assert_eq!(got, [json!(9), json!(7)]);
    assert_eq!(v["cycles"][0]["console"], "console-9.txt");
}

#[test]
fn a_finished_cycle_that_is_not_the_last_one_leaves_the_view_and_the_disk() {
    let w = views("host-view-finished", 3);
    let mut files: Vec<String> = fs::read_dir(w.live.join("cycles"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".json"))
        .collect();
    files.sort();
    assert_eq!(files, ["7.json", "9.json"]);
    assert_eq!(running(&w)["last"]["issue"], 7);
}

#[test]
fn a_cycle_that_has_been_quiet_for_two_minutes_is_no_longer_shown_as_running() {
    let w = views("host-view-quiet", 4);
    let v = running(&w);
    let issues: Vec<Value> = v["cycles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["issue"].clone())
        .collect();
    // Cycle 9 of the case before is still heard from.
    assert_eq!(json!([issues, v["last"]["issue"]]), json!([[9], 7]));
    let lost: Vec<Value> = json_at(w.live.join("endings.json"))
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["issue"] == 11)
        .map(|e| e["status"].clone())
        .collect();
    assert_eq!(lost, [json!("lost")]);
}

#[test]
fn a_cycle_with_no_start_on_record_runs_and_says_once_that_its_heartbeat_is_not_on_record() {
    let w = World::new("host-no-start");
    w.fresh();
    let r = w.work7();
    assert_eq!(
        r.out
            .lines()
            .filter(|l| l.contains("the heartbeat of #7 is not on record (nocycle)"))
            .count(),
        1,
        "{}",
        r.out
    );
    assert!(!w.live.join("cycles/7.json").exists());
}

#[test]
fn a_cycle_that_is_stopped_is_recorded_as_interrupted() {
    let w = World::new("host-interrupted");
    // The cycle's call stays alive until docker stop is called.
    w.container(r#""$nap" 60 >/dev/null 2>&1 & echo $! >"$T/sleeper.pid"; wait"#);
    w.under_cycle();
    // Every wait is bounded. The live file says running only after the trap is set.
    let r = w.sh(&format!(
        "\"{work}\" 7 >\"$T/work.out\" 2>&1 & pid=$!
for _ in $(seq 100); do [ \"$(jq -r .running \"{live}/cycles/7.json\" 2>/dev/null)\" = true ] && break; sleep 0.1; done
kill -TERM $pid; wait $pid; echo rc=$?",
        work = w.work_cmd().display(),
        live = w.live.display()
    ));
    has(&r.out, "rc=2");
    has(&w.calls("docker"), "stop -t 10 tehas-work-7");
    let c = json_at(w.live.join("cycles/7.json"));
    assert_eq!(
        json!([c["running"], c["status"]]),
        json!([false, "interrupted"])
    );
}

// --- What is kept on record ----------------------------------------------------

const COMMIT40: &str = "0123456789abcdef0123456789abcdef01234567";

fn image_id() -> String {
    format!("sha256:{}", "e".repeat(64))
}

/// `<image> <rukas_commit>` of the cycle on record, null as `null`.
fn image_row(w: &World) -> String {
    let c = &w.row(&["cycle-show", "7"])["cycle"];
    let s = |v: &Value| v.as_str().map_or_else(|| v.to_string(), String::from);
    format!("{} {}", s(&c["image"]), s(&c["rukas_commit"]))
}

#[test]
fn the_images_id_and_the_pi_rukas_commit_of_its_label_are_on_record_read_before_the_container_starts(
) {
    let w = World::new("host-image-record");
    w.fresh();
    w.under_cycle();
    w.fs("image.inspect", format!("{} {COMMIT40}\n", image_id()));
    // What the cycle's record holds as the container runs.
    w.container(r#""$T/store-bin" cycle-show 7 >"$T/calls/at-run.json"; says handoff"#);
    w.work7();
    assert_eq!(image_row(&w), format!("{} {COMMIT40}", image_id()));
    let at_run: Value = serde_json::from_str(&w.calls("at-run.json")).unwrap();
    assert_eq!(at_run["cycle"]["image"], image_id().as_str());
    assert!(at_run["cycle"]["settings"]
        .as_object()
        .is_some_and(|s| s.contains_key("PI_ENSEMBLE_SUBAGENT_MODEL")));
    let docker = w.calls("docker");
    has(
        &docker,
        &format!(
            "image inspect --format {{{{.Id}}}} {{{{index .Config.Labels \"tehas.rukas\"}}}} {}",
            w.image
        ),
    );
    let inspect = docker.lines().position(|l| l.starts_with("image inspect"));
    let run = docker
        .lines()
        .position(|l| l.contains("pi-rukas-headless work "));
    assert!(
        matches!((inspect, run), (Some(a), Some(b)) if a < b),
        "{docker}"
    );
}

#[test]
fn an_image_without_the_label_is_on_record_by_its_id_alone() {
    let w = World::new("host-image-no-label");
    w.fresh();
    w.under_cycle();
    w.fs("image.inspect", format!("{} \n", image_id()));
    w.work7();
    assert_eq!(image_row(&w), format!("{} null", image_id()));
}

#[test]
fn an_answer_that_is_not_an_image_id_puts_none_on_record_nor_a_commit_that_is_no_commit() {
    let w = World::new("host-image-bad");
    w.fresh();
    w.under_cycle();
    w.fs("image.inspect", "not-an-id 0123456 $(touch x)\n");
    w.work7();
    assert!(w.row(&["cycle-show", "7"])["cycle"]["image"].is_null());
    w.fs("image.inspect", format!("{} not-a-commit\n", image_id()));
    w.work7();
    assert_eq!(image_row(&w), format!("{} null", image_id()));
}

#[test]
fn run_on_its_own_tehas_work_puts_nothing_on_a_cycles_record() {
    let w = World::new("host-image-by-hand");
    w.fresh();
    w.fs("image.inspect", format!("{} {COMMIT40}\n", image_id()));
    w.work7();
    assert!(w.row(&["cycle-show", "7"])["cycle"].is_null());
}

#[test]
fn what_the_container_is_given_of_the_settings_is_on_record_before_it_starts_with_the_cycles_running(
) {
    let w = World::new("host-settings-record");
    w.fresh();
    w.under_cycle();
    w.fs("image.inspect", format!("{} {COMMIT40}\n", image_id()));
    w.container("says handoff");
    let r = w.work7();
    assert_eq!(r.rc, 0, "{}", r.out);
    let f = w.row(&["cycle-show", "7"])["cycle"]["settings"].clone();
    let mut keys: Vec<&String> = f
        .as_object()
        .unwrap()
        .keys()
        .filter(|k| k.starts_with("PI_ENSEMBLE_"))
        .collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "PI_ENSEMBLE_CYCLE_DEADLINE",
            "PI_ENSEMBLE_HANDOFF_CHANNEL",
            "PI_ENSEMBLE_HANDOFF_TIMEOUT_MS",
            "PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS",
            "PI_ENSEMBLE_MAX_WORKSTREAMS",
            "PI_ENSEMBLE_SANDBOX_MODE",
            "PI_ENSEMBLE_SPAWN_TIMEOUT_MS",
            "PI_ENSEMBLE_SUBAGENT_MODEL",
            "PI_ENSEMBLE_SUBAGENT_PROVIDER",
            "PI_ENSEMBLE_VERIFY_CONCURRENCY",
            "PI_ENSEMBLE_VERIFY_SUITE_TIMEOUT_MS",
            "PI_ENSEMBLE_VERIFY_TIMEOUT_MS",
            "PI_ENSEMBLE_WORKSPACE_ROOT",
        ]
    );
    let text = |k: &str| f[k].as_str().map_or_else(|| f[k].to_string(), String::from);
    assert_eq!(text("PI_ENSEMBLE_SUBAGENT_MODEL"), "test-model");
    assert_eq!(text("PI_ENSEMBLE_VERIFY_SUITE_TIMEOUT_MS"), "1800000");
    assert_eq!(text("PI_ENSEMBLE_WORKSPACE_ROOT"), "[withheld]");
    // This command holds the issue's lock, so it counts itself.
    assert_eq!(text("concurrentCycles"), "1");
    let f = f.to_string();
    lacks(&f, "GH_TOKEN");
    lacks(&f, "ghs_app");
    // The file it was handed in is gone.
    let left: Vec<PathBuf> = files_under(&w.home.join("tmp"))
        .into_iter()
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("settings."))
        })
        .collect();
    assert!(left.is_empty(), "{left:?}");
}

// --- The container's counts -----------------------------------------------------

fn scope(w: &World, id: &str) -> PathBuf {
    w.path(&format!("cgroup/tehas.slice/docker-{id}.scope"))
}

/// A container that runs a second, then its scope goes with it, and half a
/// second more.
fn scope_goes(w: &World, tail: &str) {
    w.container(&format!(
        r#""$nap" 1; rm -rf '{}'{tail}"#,
        scope(w, &w.cid).display()
    ));
}

#[test]
fn the_containers_oom_kill_count_is_printed_and_written_to_the_state_directory_and_is_not_read_from_the_slice(
) {
    let w = World::new("host-oom");
    w.fresh();
    w.slice_good();
    w.slice_child(&w.cid, 1000, 0, "max", 3);
    w.slice_child(&slice_id('b'), 1000, 0, "max", 5);
    w.fs(
        "cgroup/tehas.slice/memory.events",
        "low 0\nhigh 0\nmax 9\noom 9\noom_kill 9\n",
    );
    // The scope goes with the container: the count is the last one seen.
    scope_goes(&w, r#"; "$nap" 0.5"#);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    has(
        &r.out,
        "tehas work: 3 processes in this cycle's container were killed for memory",
    );
    assert_eq!(read(&w.oom_file), "3");
    assert!(!w.cid_file.exists());
}

fn kill_line(id: &str) -> String {
    format!("oom-kill:constraint=CONSTRAINT_MEMCG,nodemask=(null),oom_memcg=/tehas.slice/docker-{id}.scope,task_memcg=/tehas.slice/docker-{id}.scope,task=pi,pid=9,uid=1001\n")
}

#[test]
fn kills_after_the_last_look_at_the_scope_are_counted_from_the_kernel_log_this_containers_alone() {
    let w = World::new("host-oom-kernel");
    w.fresh();
    w.slice_good();
    w.slice_child(&w.cid, 1000, 0, "max", 1);
    // One line of each kill with its cgroup. The scope goes at once with the
    // kills that ended the cycle, before a tick could see them.
    w.fs(
        "kernel.log",
        format!(
            "{}{}Memory cgroup out of memory: Killed process 9 (pi)\n",
            kill_line(&w.cid),
            kill_line(&slice_id('b'))
        ),
    );
    w.container(&format!(
        r#""$nap" 1; {{ for i in 1 2 3 4; do echo 'oom-kill:constraint=CONSTRAINT_MEMCG,task_memcg=/tehas.slice/docker-{cid}.scope,task=pi'; done; }} >>'{log}'; rm -rf '{scope}'; exit 137"#,
        cid = w.cid,
        log = w.path("kernel.log").display(),
        scope = scope(&w, &w.cid).display()
    ));
    let r = w.work7();
    assert_eq!(r.rc, 137, "{}", r.out);
    let said = "tehas work: 5 processes in this cycle's container were killed for memory (pi)";
    has(&r.out, said);
    assert_eq!(read(&w.oom_file), "5");
    has(&read(w.live.join("console-7.txt")), said);
    let j = w.calls("journalctl");
    has(&j, "-k");
    has(&j, "--since @");
}

#[test]
fn a_kernel_log_that_cannot_be_read_or_says_fewer_leaves_the_count_of_the_scope() {
    let w = World::new("host-oom-fewer");
    w.fresh();
    w.slice_good();
    w.slice_child(&w.cid, 1000, 0, "max", 3);
    w.fs("kernel.log", kill_line(&w.cid));
    scope_goes(&w, "");
    w.work7();
    assert_eq!(read(&w.oom_file), "3");
    w.rm("kernel.log");
    w.fresh();
    w.slice_good();
    w.slice_child(&w.cid, 1000, 0, "max", 3);
    scope_goes(&w, "");
    w.work7();
    assert_eq!(read(&w.oom_file), "3");
}

#[test]
fn the_starts_the_pids_limit_refused_are_printed_and_written_to_the_state_directory() {
    let w = World::new("host-pids");
    w.fresh();
    w.slice_good();
    let b = slice_id('b');
    w.slice_child(&w.cid, 1000, 0, "max", 0);
    w.slice_child(&b, 1000, 0, "max", 0);
    fs::write(scope(&w, &w.cid).join("pids.events"), "max 41\n").unwrap();
    fs::write(scope(&w, &b).join("pids.events"), "max 7\n").unwrap();
    scope_goes(&w, r#"; "$nap" 0.5"#);
    let r = w.work7();
    has(
        &r.out,
        "tehas work: this cycle's container reached its limit of processes, 41 starts were refused",
    );
    assert_eq!(read(&w.forks_file), "41");
    lacks(&r.out, "killed for memory");
}

#[test]
fn the_pids_limit_the_container_reached_goes_on_the_second_line_of_the_count() {
    let w = World::new("host-pids-max");
    w.fresh();
    w.slice_good();
    w.slice_child(&w.cid, 1000, 0, "max", 0);
    fs::write(scope(&w, &w.cid).join("pids.events"), "max 21\n").unwrap();
    fs::write(scope(&w, &w.cid).join("pids.max"), "2048\n").unwrap();
    scope_goes(&w, r#"; "$nap" 0.5"#);
    w.work7();
    assert_eq!(read(&w.forks_file), "21\n2048");
}

#[test]
fn a_container_that_stayed_under_the_pids_limit_leaves_no_count_and_one_left_by_an_earlier_cycle_is_removed(
) {
    let w = World::new("host-pids-none");
    w.fresh();
    w.slice_good();
    fs::create_dir_all(w.forks_file.parent().unwrap()).unwrap();
    fs::write(&w.forks_file, "9\n").unwrap();
    w.slice_child(&w.cid, 1000, 0, "max", 0);
    fs::write(scope(&w, &w.cid).join("pids.events"), "max 0\n").unwrap();
    w.container(r#""$nap" 0.5"#);
    let r = w.work7();
    lacks(&r.out, "limit of processes");
    assert!(!w.forks_file.exists());
}

#[test]
fn a_cid_file_and_a_count_left_by_a_cycle_that_was_killed_outright_are_not_this_cycles() {
    let w = World::new("host-cid-left");
    w.fresh();
    w.slice_good();
    fs::create_dir_all(w.cid_file.parent().unwrap()).unwrap();
    fs::write(&w.cid_file, format!("{}\n", slice_id('b'))).unwrap();
    fs::write(&w.oom_file, "5\n").unwrap();
    w.slice_child(&slice_id('b'), 1000, 0, "max", 5);
    w.container(r#"touch "$T/calls/ran""#);
    let r = w.work7();
    assert!(w.path("calls/ran").exists());
    lacks(&r.out, "killed for memory");
    assert!(!w.cid_file.exists() && !w.oom_file.exists());
}

#[test]
fn a_cid_file_that_does_not_hold_a_container_id_reads_no_cgroup() {
    let w = World::new("host-cid-bad");
    w.fresh();
    w.fs("cid", "../../tehas.slice\n");
    w.fs("cgroup/tehas.slice/memory.events", "oom_kill 9\n");
    w.container(r#""$nap" 0.5"#);
    let r = w.work7();
    lacks(&r.out, "killed for memory");
    assert!(!w.oom_file.exists());
}
