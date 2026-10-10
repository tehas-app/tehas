//! The console as libexec/work runs it: started beside a cycle, stopped with
//! TERM when the cycle has ended.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

const CONSOLE: &str = env!("CARGO_BIN_EXE_console");

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tehas-console-it-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

// What the file holds, without the time each line begins with.
fn said(out: &PathBuf) -> Vec<String> {
    let text = fs::read_to_string(out).unwrap_or_default();
    text.lines().map(|l| l.get(9..).unwrap_or("").to_string()).collect()
}

fn wait_for(out: &PathBuf, lines: usize) {
    let started = Instant::now();
    while said(out).len() < lines {
        assert!(started.elapsed() < Duration::from_secs(20), "the console wrote {:?}", said(out));
        sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_cycle_becomes_a_console_file_and_a_stop_reads_the_last_lines() {
    let dir = scratch("cycle");
    let (rpc, runs, out) = (dir.join("work-7.jsonl"), dir.join("runs"), dir.join("console-7.txt"));
    fs::write(&out, "").unwrap();
    // A transcript of an earlier cycle: it must not be replayed.
    fs::create_dir_all(runs.join("2026-10-09")).unwrap();
    let old = runs.join("2026-10-09/aa-bb-coder.json");
    fs::write(&old, "{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"old\"}]}}\n").unwrap();
    let hour_ago = std::time::SystemTime::now() - Duration::from_secs(3600);
    fs::File::options().write(true).open(&old).unwrap().set_modified(hour_ago).unwrap();
    let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() - 5;

    let mut console = Command::new(CONSOLE)
        .args(["--rpc", rpc.to_str().unwrap(), "--runs", runs.to_str().unwrap()])
        .args(["--out", out.to_str().unwrap(), "--since", &since.to_string()])
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    wait_for(&out, 1);

    fs::write(&rpc, "{\"type\":\"extension_ui_request\",\"method\":\"notify\",\"message\":\"cycle started\"}\nnot json\n").unwrap();
    fs::create_dir_all(runs.join("2026-10-10")).unwrap();
    fs::write(
        runs.join("2026-10-10/aa-bb-coder.json"),
        "{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"toolCall\",\"name\":\"bash\",\"arguments\":{\"command\":\"echo GH_TOKEN=abc123\"}}]}}\n",
    )
    .unwrap();
    wait_for(&out, 3);

    // Written just before the stop: the last pass still reads it.
    let mut log = fs::read(&rpc).unwrap();
    log.extend_from_slice(b"{\"type\":\"extension_ui_request\",\"method\":\"notify\",\"message\":\"handoff\"}\n");
    fs::write(&rpc, log).unwrap();
    let killed = Command::new("kill").args(["-TERM", &console.id().to_string()]).status().unwrap();
    assert!(killed.success());
    assert!(console.wait().unwrap().success());

    assert_eq!(
        said(&out),
        [
            "tehas      console attached, waiting for the cycle",
            "driver     cycle started",
            "coder      $ echo GH_TOKEN=***",
            "driver     handoff",
            "tehas      cycle ended, console detached",
        ]
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_missing_argument_is_a_usage_error() {
    let run = |args: &[&str]| Command::new(CONSOLE).args(args).output().unwrap();
    let none = run(&[]);
    assert_eq!(none.status.code(), Some(2));
    assert_eq!(String::from_utf8_lossy(&none.stderr), "console: --since is required\n");
    let no_out = run(&["--since", "1", "--rpc", "a", "--runs", "b"]);
    assert_eq!(no_out.status.code(), Some(2));
    assert_eq!(String::from_utf8_lossy(&no_out.stderr), "console: --out is required\n");
    let bad = run(&["--since", "soon", "--rpc", "a", "--runs", "b", "--out", "c"]);
    assert_eq!(bad.status.code(), Some(2));
}

#[test]
fn a_console_file_that_cannot_be_written_ends_it_with_1() {
    let dir = scratch("unwritable");
    let out = dir.join("no-such-directory/console.txt");
    let run = Command::new(CONSOLE)
        .args(["--rpc", "a", "--runs", "b", "--since", "1", "--out", out.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(1));
    let _ = fs::remove_dir_all(&dir);
}
