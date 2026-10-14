//! The console as libexec/work runs it: started beside a cycle, stopped with
//! TERM when the cycle has ended.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

const CONSOLE: &str = env!("CARGO_BIN_EXE_console");

// A directory of this test, under cargo's own place for test files.
fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

// A day as the transcript directories are named, from the clock: the console
// does not list a day long before the cycle, so fixed dates would age out.
fn day(when: &str) -> String {
    let out = Command::new("date")
        .args(["-u", "-d", when, "+%F"])
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

// What the file holds, without the time each line begins with.
fn said(out: &PathBuf) -> Vec<String> {
    let text = fs::read_to_string(out).unwrap_or_default();
    text.lines()
        .map(|l| l.get(9..).unwrap_or("").to_string())
        .collect()
}

fn wait_for(out: &PathBuf, lines: usize) {
    let started = Instant::now();
    while said(out).len() < lines {
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "the console wrote {:?}",
            said(out)
        );
        sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_cycle_becomes_a_console_file_and_a_stop_reads_the_last_lines() {
    let dir = scratch("cycle");
    let (rpc, runs, out) = (
        dir.join("work-7.jsonl"),
        dir.join("runs"),
        dir.join("console-7.txt"),
    );
    fs::write(&out, "").unwrap();
    // A transcript of an earlier cycle: it must not be replayed.
    let (yesterday, today) = (day("yesterday"), day("today"));
    fs::create_dir_all(runs.join(&yesterday)).unwrap();
    let old = runs.join(&yesterday).join("aa-bb-coder.json");
    fs::write(&old, "{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"old\"}]}}\n").unwrap();
    let hour_ago = std::time::SystemTime::now() - Duration::from_secs(3600);
    fs::File::options()
        .write(true)
        .open(&old)
        .unwrap()
        .set_modified(hour_ago)
        .unwrap();
    let since = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 5;

    let mut console = Command::new(CONSOLE)
        .args([
            "--rpc",
            rpc.to_str().unwrap(),
            "--runs",
            runs.to_str().unwrap(),
        ])
        .args([
            "--out",
            out.to_str().unwrap(),
            "--since",
            &since.to_string(),
        ])
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    wait_for(&out, 1);

    fs::write(&rpc, "{\"type\":\"extension_ui_request\",\"method\":\"notify\",\"message\":\"cycle started\"}\nnot json\n").unwrap();
    fs::create_dir_all(runs.join(&today)).unwrap();
    fs::write(
        runs.join(&today).join("aa-bb-coder.json"),
        "{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"toolCall\",\"name\":\"bash\",\"arguments\":{\"command\":\"echo GH_TOKEN=abc123\"}}]}}\n",
    )
    .unwrap();
    wait_for(&out, 3);

    // Written just before the stop: the last pass still reads it.
    let mut log = fs::read(&rpc).unwrap();
    log.extend_from_slice(
        b"{\"type\":\"extension_ui_request\",\"method\":\"notify\",\"message\":\"handoff\"}\n",
    );
    // A line too long to be one: left out, and said.
    log.extend_from_slice(&vec![b'x'; (1 << 20) + 1]);
    log.push(b'\n');
    fs::write(&rpc, log).unwrap();
    let killed = Command::new("kill")
        .args(["-TERM", &console.id().to_string()])
        .status()
        .unwrap();
    assert!(killed.success());
    assert!(console.wait().unwrap().success());

    assert_eq!(
        said(&out),
        [
            "tehas      console attached, waiting for the cycle",
            "driver     cycle started",
            "coder      $ echo GH_TOKEN=***",
            "driver     handoff",
            "tehas      log lines over 1 MiB left out: 1",
            "tehas      cycle ended, console detached",
        ]
    );
}

// A console that a failed test leaves running would hold cargo's output open.
struct Stopped(std::process::Child);
impl Drop for Stopped {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn more_transcripts_than_the_process_may_open_are_all_shown() {
    let dir = scratch("many");
    let (rpc, runs, out) = (
        dir.join("work-8.jsonl"),
        dir.join("runs"),
        dir.join("console-8.txt"),
    );
    let today = runs.join(day("today"));
    fs::create_dir_all(&today).unwrap();
    let count = 100;
    for n in 0..count {
        fs::write(
            today.join(format!("aa-bb-r{n}.json")),
            format!("{{\"type\":\"message\",\"message\":{{\"role\":\"assistant\",\"content\":[{{\"type\":\"text\",\"text\":\"hello {n}\"}}]}}}}\n"),
        )
        .unwrap();
    }
    let since = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 5;
    let mut console = Stopped(
        Command::new("bash")
            .args(["-c", "ulimit -n 30 && exec \"$0\" \"$@\"", CONSOLE])
            .args(["--rpc", rpc.to_str().unwrap()])
            .args(["--runs", runs.to_str().unwrap()])
            .args(["--out", out.to_str().unwrap()])
            .args(["--since", &since.to_string()])
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    wait_for(&out, count + 1);
    let killed = Command::new("kill")
        .args(["-TERM", &console.0.id().to_string()])
        .status()
        .unwrap();
    assert!(killed.success());
    assert!(console.0.wait().unwrap().success());
    let lines = said(&out);
    for n in 0..count {
        assert!(
            lines.iter().any(|l| l.ends_with(&format!(" hello {n}"))),
            "{n}: {lines:?}"
        );
    }
}

#[test]
fn a_missing_argument_is_a_usage_error() {
    let run = |args: &[&str]| Command::new(CONSOLE).args(args).output().unwrap();
    let none = run(&[]);
    assert_eq!(none.status.code(), Some(2));
    assert_eq!(
        String::from_utf8_lossy(&none.stderr),
        "console: --since is required\n"
    );
    let no_out = run(&["--since", "1", "--rpc", "a", "--runs", "b"]);
    assert_eq!(no_out.status.code(), Some(2));
    assert_eq!(
        String::from_utf8_lossy(&no_out.stderr),
        "console: --out is required\n"
    );
    for since in ["soon", "-5", "1.5", "18446744073709551615"] {
        let bad = run(&["--since", since, "--rpc", "a", "--runs", "b", "--out", "c"]);
        assert_eq!(bad.status.code(), Some(2), "{since}");
    }
}

#[test]
fn a_console_file_that_cannot_be_written_ends_it_with_1() {
    let dir = scratch("unwritable");
    let out = dir.join("no-such-directory/console.txt");
    let run = Command::new(CONSOLE)
        .args([
            "--rpc",
            "a",
            "--runs",
            "b",
            "--since",
            "1",
            "--out",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(1));
}

// The disk the console file is on is the one its complaints go to.
#[test]
fn the_exit_status_holds_when_stderr_cannot_be_written() {
    let dir = scratch("full");
    let out = dir.join("no-such-directory/console.txt");
    let run = |args: &[&str]| {
        let full = fs::File::options().write(true).open("/dev/full").unwrap();
        let mut console = Command::new(CONSOLE);
        console.args(args).stderr(full).status().unwrap().code()
    };
    assert_eq!(run(&[]), Some(2));
    let out = out.to_str().unwrap();
    assert_eq!(
        run(&["--rpc", "a", "--runs", "b", "--since", "1", "--out", out]),
        Some(1)
    );
}

#[test]
fn an_argument_that_is_not_text_is_a_usage_error() {
    use std::os::unix::ffi::OsStrExt;
    let odd = std::ffi::OsStr::from_bytes(b"c\xff.txt");
    let run = Command::new(CONSOLE)
        .args(["--rpc", "a", "--runs", "b", "--since", "1", "--out"])
        .arg(odd)
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(2));
    assert_eq!(
        String::from_utf8_lossy(&run.stderr),
        "console: an argument is not UTF-8\n"
    );
}

#[test]
fn the_mask_alone_is_standard_input_to_standard_output() {
    use std::io::Write;
    let mut console = Command::new(CONSOLE)
        .arg("--mask")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    // Built here, not written out: this file is linted too.
    let address = [10, 20, 30, 40].map(|part| part.to_string()).join(".");
    let line = format!("step 3 models at {address}:8086 and 192.0.2.7, GH_TOKEN=abc\n");
    console
        .stdin
        .take()
        .unwrap()
        .write_all(line.as_bytes())
        .unwrap();
    let run = console.wait_with_output().unwrap();
    assert_eq!(run.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "step 3 models at [private address]:8086 and 192.0.2.7, GH_TOKEN=***\n"
    );
}

// libexec/work is tested with a stub for the console, and the console with
// arguments written here: this holds the two to each other.
#[test]
fn work_calls_the_console_by_the_arguments_it_takes() {
    let work = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../libexec/work")).unwrap();
    for call in [
        "\"$here/console\" --rpc \"$log\"",
        "--runs \"$runs\" --out \"$console\" --since \"$((started - 5))\"",
        "\"$here/console\" --mask",
    ] {
        assert!(work.contains(call), "libexec/work does not hold: {call}");
    }
}
