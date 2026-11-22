//! What the tests of `tehas top` share: a host of the World harness, the live
//! files a working factory leaves behind, a tree of host numbers written here
//! in place of /proc (TEHAS_PROC_DIR), and a systemctl stub that answers with
//! two units and a timer. The console is this build's. Nothing here calls
//! GitHub, a model or Docker, and no case reads the machine that runs it.
//! Moved from test/top_lib.sh, which top_frame_test.sh and
//! top_publish_test.sh still source.

use crate::common::*;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use tehas_core::home::Live;
use tehas_core::time::{iso, now};

/// A systemctl that answers: two units, one timer that fires later.
const UNITS: &str = r#"case " $* " in
  *" list-units "*)
    printf "%s\n" "tehas-dispatch.timer loaded active waiting Dispatch the queue" \
                  "tehas-cycle@12.service loaded active running Cycle for 12" ;;
  *" list-unit-files "*)
    printf "%s\n" "tehas-dispatch.timer enabled" "tehas-energy.service enabled" ;;
  *" show "*)
    printf "Id=tehas-cycle@12.service\nLoadState=loaded\nActiveState=active\nSubState=running\nUnitFileState=\nNextElapseUSecRealtime=n/a\n\nId=tehas-dispatch.timer\nLoadState=loaded\nActiveState=active\nSubState=waiting\nUnitFileState=enabled\nNextElapseUSecRealtime=Mon 2026-01-01 10:00:00 UTC n/a\n\nId=tehas-energy.service\nLoadState=loaded\nActiveState=inactive\nSubState=dead\nUnitFileState=enabled\nNextElapseUSecRealtime=n/a\n" ;;
  *) exit 0 ;;
esac"#;

/// The processes of proc1, as rows of `proc_files`. The fourth has a command
/// line with a token in it, which no reading is allowed to open.
pub const PROCS: [&str; 4] = [
    "100 systemd 150 300",
    "200 tehas-cycle 0 800",
    "300 pi-rukas 730 1200",
    "400 gh 10 100",
];

/// The aggregate cpu line of proc1.
pub const CPU: &str = "1000 100 500 8000 200 10 5 20";

/// One host as reset() leaves it.
pub fn host(name: &str) -> World {
    let w = World::new(&format!("top-{name}"));
    reset(&w);
    w
}

/// The tree of host numbers that every run reads, until a case names another.
pub fn proc1(w: &World) -> PathBuf {
    w.t().join("proc1")
}

/// The live files and the trees are gone, systemctl answers, proc1 is written
/// and every run reads it.
pub fn reset(w: &World) {
    w.clear_calls();
    for d in [w.live.clone(), proc1(w), w.t().join("proc2")] {
        let _ = fs::remove_dir_all(d);
    }
    fs::create_dir_all(&w.live).unwrap();
    w.stub("systemctl", UNITS);
    proc_files(&proc1(w), 1000, CPU, &PROCS);
    // A command line is never opened. It is here to be missed.
    fs::write(
        proc1(w).join("400/cmdline"),
        "gh_SecretTokenInACommandLine --repo acme/widget\n",
    )
    .unwrap();
    proc_env(w, &proc1(w));
}

/// Every later run reads the tree at `dir`, until reset().
pub fn proc_env(w: &World, dir: &Path) {
    w.set_env(&[&format!("TEHAS_PROC_DIR={}", dir.display())]);
}

/// The host's own numbers, as files a reading can be taken from: two CPUs, load
/// averages, memory, and one process per row. A row is "<pid> <short name>
/// <cpu ticks> <resident pages>", its fields kept apart by spaces or `|`. The
/// start ticks come from the pid, 900 + pid - 100, so two trees give the same
/// process the same start.
pub fn proc_files(dir: &Path, clock: u64, cpu: &str, rows: &[impl AsRef<str>]) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("uptime"), format!("{clock}.00 0.00\n")).unwrap();
    fs::write(
        dir.join("stat"),
        format!("cpu  {cpu}\ncpu0 1 0 0 0 0 0 0 0 0 0\ncpu1 1 0 0 0 0 0 0 0 0 0\nintr 1\n"),
    )
    .unwrap();
    fs::write(dir.join("loadavg"), "0.10 0.20 0.30 1/200 300\n").unwrap();
    fs::write(
        dir.join("meminfo"),
        "MemTotal:       65536 kB\nMemFree:          10000 kB\nMemAvailable:     32768 kB\n",
    )
    .unwrap();
    fs::write(
        dir.join("cpuinfo"),
        "processor\t: 0\nmodel name\t: a cpu\nprocessor\t: 1\n",
    )
    .unwrap();
    for row in rows {
        let row = row.as_ref();
        let f: Vec<&str> = row.split([' ', '|']).collect();
        let [pid, name, ticks, rss] = f[..] else {
            panic!("a row is four fields: {row}");
        };
        let p: u64 = pid.parse().unwrap();
        fs::create_dir_all(dir.join(pid)).unwrap();
        fs::write(
            dir.join(pid).join("stat"),
            format!(
                "{pid} ({name}) S 1 {pid} {pid} 0 -1 1 0 0 0 0 {ticks} 0 0 0 20 0 1 0 {} 1048576 {rss} 0 0 0 0 0 0 0 0 0 0 0 0 0 0 17 0 0 0 0 0 0\n",
                900 + p - 100
            ),
        )
        .unwrap();
    }
}

/// The files a working factory leaves behind. Times are seconds before now,
/// so the words about age stay in one bucket while a case runs.
pub fn files(w: &World, title: &str) {
    let t = now();
    let at = w.at();
    let item = |n: u64, title: &str| json!({"number": n, "title": title, "url": "u", "updatedAt": "x", "closedAt": null});
    let status = json!({
        "generatedAt": iso(t - 210), "repo": "acme/widget", "paused": false,
        "running": [item(12, title)],
        "queued": [item(42, "tehas top")],
        "awaiting": [item(7, "A preview for every pull request")],
        "stopped": [item(9, "A stopped cycle")],
        "completed": [],
    });
    fs::write(at.live(Live::Status), status.to_string()).unwrap();
    let running = json!({"cycles": [{"issue": 12, "running": true, "status": "running",
        "step": "develop", "console": "console-12.txt", "startedAt": t - 630,
        "updatedAt": t - 10}], "last": null});
    fs::write(at.live(Live::Running), running.to_string()).unwrap();
    let health = json!({"status": "pass", "time": iso(t - 210), "output": "Working on #12.",
        "checks": {"queue": {"time": iso(t - 210), "paused": false, "inLine": 1,
            "building": 1, "review": 1, "stopped": 1}, "cycle": null, "cycles": []}});
    fs::write(at.live(Live::Health), health.to_string()).unwrap();
    fs::write(
        at.live_console(12),
        "[step develop] @developer ran the gate\nok    the shell tests\n",
    )
    .unwrap();
}

/// `files` with the title the shell gave by default.
pub fn files0(w: &World) {
    files(w, "Two cycles side by side");
}

/// An energy sample `age` seconds old.
pub fn energy(w: &World, age: i64) {
    let v = json!({"generatedAt": iso(now() - age), "totals": {"watts": 312.5, "hosts": 4},
        "models": [{"name": "m", "slots": 8, "busy": 4}]});
    fs::write(w.at().live(Live::Energy), v.to_string()).unwrap();
}

/// `tehas top` with these words, as a shell snippet.
pub fn top_cmd(words: &str) -> String {
    format!("{} top {words}", root().join("bin/tehas").display())
}

/// `tehas top` with these words.
pub fn top(w: &World, words: &str) -> Run {
    w.sh(&top_cmd(words))
}

/// Bytes that are not printing characters, tab and newline excepted.
#[allow(dead_code)] // each test file uses a part of this module
pub fn control_bytes(text: &str) -> usize {
    text.bytes()
        .filter(|&b| (b < 0x20 && b != b'\t' && b != b'\n') || b == 0x7f)
        .count()
}
