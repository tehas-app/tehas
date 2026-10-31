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

// One write to the end of a log, as the runner adds its lines. A pass that
// wrote the file again from its start would be read in full, and the follower
// would see bytes the cycle never added.
fn append(path: &PathBuf, bytes: &str) {
    use std::io::Write;
    fs::OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(bytes.as_bytes())
        .unwrap();
}

#[test]
fn a_cycle_becomes_a_console_file_and_a_stop_reads_the_last_lines() {
    let dir = scratch("cycle");
    let (progress, runs, out) = (
        dir.join("work-7.ndjson"),
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
            "--progress",
            progress.to_str().unwrap(),
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

    // A resumed cycle writes an event again under the same key: one entry.
    let step =
        "{\"v\":1,\"kind\":\"step-started\",\"key\":\"7:0:1\",\"event\":{\"step\":\"explore\"}}\n";
    let unstable =
        "{\"v\":1,\"kind\":\"dispatch-failed\",\"unstable\":true,\"event\":{\"role\":\"ops\"}}\n";
    fs::write(
        &progress,
        format!("{{\"v\":1,\"kind\":\"log\",\"message\":\"cycle started\"}}\nnot json\n{step}{unstable}{step}"),
    )
    .unwrap();
    fs::create_dir_all(runs.join(&today)).unwrap();
    fs::write(
        runs.join(&today).join("aa-bb-coder.json"),
        "{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"toolCall\",\"name\":\"bash\",\"arguments\":{\"command\":\"echo GH_TOKEN=abc123\"}}]}}\n",
    )
    .unwrap();
    wait_for(&out, 4);

    // Written just before the stop: the last pass still reads it.
    let mut log = fs::read(&progress).unwrap();
    log.extend_from_slice(b"{\"v\":1,\"kind\":\"log\",\"message\":\"handoff\"}\n");
    // A line too long to be one: left out, and said.
    log.extend_from_slice(&vec![b'x'; (1 << 20) + 1]);
    log.push(b'\n');
    fs::write(&progress, log).unwrap();
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
            "driver     --- explore started (round 1)",
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
    let (progress, runs, out) = (
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
            .args(["--progress", progress.to_str().unwrap()])
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
fn a_removed_console_file_is_made_again() {
    let dir = scratch("removed");
    let (progress, runs, out) = (
        dir.join("work-9.jsonl"),
        dir.join("runs"),
        dir.join("console-9.txt"),
    );
    let mut console = Stopped(
        Command::new(CONSOLE)
            .args(["--progress", progress.to_str().unwrap()])
            .args(["--runs", runs.to_str().unwrap()])
            .args(["--out", out.to_str().unwrap(), "--since", "0"])
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    wait_for(&out, 1);
    fs::remove_file(&out).unwrap();
    let started = Instant::now();
    while !out.exists() {
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "not made again"
        );
        sleep(Duration::from_millis(50));
    }
    fs::write(
        &progress,
        "{\"v\":1,\"kind\":\"log\",\"message\":\"after\"}\n",
    )
    .unwrap();
    wait_for(&out, 1);
    let killed = Command::new("kill")
        .args(["-TERM", &console.0.id().to_string()])
        .status()
        .unwrap();
    assert!(killed.success());
    let started = Instant::now();
    assert!(console.0.wait().unwrap().success());
    assert!(started.elapsed() < Duration::from_secs(10));
    assert_eq!(
        said(&out),
        [
            "driver     after",
            "tehas      cycle ended, console detached"
        ]
    );
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
    let no_out = run(&["--since", "1", "--progress", "a", "--runs", "b"]);
    assert_eq!(no_out.status.code(), Some(2));
    assert_eq!(
        String::from_utf8_lossy(&no_out.stderr),
        "console: --out is required\n"
    );
    for since in ["soon", "-5", "1.5", "18446744073709551615"] {
        let bad = run(&[
            "--since",
            since,
            "--progress",
            "a",
            "--runs",
            "b",
            "--out",
            "c",
        ]);
        assert_eq!(bad.status.code(), Some(2), "{since}");
    }
}

#[test]
fn a_console_file_that_cannot_be_written_ends_it_with_1() {
    let dir = scratch("unwritable");
    let out = dir.join("no-such-directory/console.txt");
    let run = Command::new(CONSOLE)
        .args([
            "--progress",
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
        run(&[
            "--progress",
            "a",
            "--runs",
            "b",
            "--since",
            "1",
            "--out",
            out
        ]),
        Some(1)
    );
}

#[test]
fn an_argument_that_is_not_text_is_a_usage_error() {
    use std::os::unix::ffi::OsStrExt;
    let odd = std::ffi::OsStr::from_bytes(b"c\xff.txt");
    let run = Command::new(CONSOLE)
        .args(["--progress", "a", "--runs", "b", "--since", "1", "--out"])
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
        "\"$here/console\" --progress \"$log\"",
        "--runs \"$runs\" --out \"$console\" --since \"$((started - 5))\" --now \"$console_now\"",
    ] {
        assert!(work.contains(call), "libexec/work does not hold: {call}");
    }
    assert!(
        !work.contains("--mask"),
        "libexec/work still asks for --mask"
    );
}

// The two lines the watch loop of libexec/work reads for the dashboard's
// status, kept by the console and by no one else.
#[test]
fn the_now_file_holds_the_step_and_the_notice_and_is_written_again_when_they_change() {
    let dir = scratch("now");
    let (progress, runs, out, now) = (
        dir.join("work-10.jsonl"),
        dir.join("runs"),
        dir.join("console-10.txt"),
        dir.join("console-10.txt.now"),
    );
    let since = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 5;
    let mut console = Stopped(
        Command::new(CONSOLE)
            .args(["--progress", progress.to_str().unwrap()])
            .args(["--runs", runs.to_str().unwrap()])
            .args(["--out", out.to_str().unwrap()])
            .args(["--since", &since.to_string()])
            .args(["--private", "models.lan"])
            .args(["--now", now.to_str().unwrap()])
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    // Nothing of the cycle yet: the step is empty and the notice too.
    let held = |what: &str| {
        let started = Instant::now();
        loop {
            let got = fs::read_to_string(&now).unwrap_or_default();
            if got == what {
                return;
            }
            assert!(
                started.elapsed() < Duration::from_secs(20),
                "the now file holds {:?}, not {what:?}",
                got
            );
            sleep(Duration::from_millis(50));
        }
    };
    held("\n\n");

    let step = "{\"v\":1,\"kind\":\"step-started\",\"event\":{\"step\":\"develop\"}}\n";
    let notice = "{\"v\":1,\"kind\":\"log\",\"message\":\"on models.lan now\\nsecond line\"}\n";
    fs::write(&progress, format!("{step}{notice}")).unwrap();
    // The masked notice, of one line, and the step of the last step-started.
    held("develop\non [private name] now\n");
    let written = fs::metadata(&now).unwrap().modified().unwrap();

    // A line that changes neither is not written again. The log is appended to
    // as the runner writes it: a pass that rewrote the file from its start
    // would be read again in full, not left behind.
    append(
        &progress,
        &format!("{{\"v\":1,\"kind\":\"log\",\"message\":\"and again\"}}\n"),
    );
    wait_for(&out, 4);
    assert_eq!(fs::metadata(&now).unwrap().modified().unwrap(), written);

    // The step and the notice of the next step are written again.
    append(
        &progress,
        &format!(
            "{{\"v\":1,\"kind\":\"step-started\",\"event\":{{\"step\":\"review\"}}}}\n\
             {{\"v\":1,\"kind\":\"log\",\"message\":\"pi-rukas:driver-event hidden\"}}\n\
             {{\"v\":1,\"kind\":\"log\",\"message\":\"the handoff\"}}\n"
        ),
    );
    held("review\nthe handoff\n");

    let killed = Command::new("kill")
        .args(["-TERM", &console.0.id().to_string()])
        .status()
        .unwrap();
    assert!(killed.success());
    assert!(console.0.wait().unwrap().success());
    assert_eq!(fs::read_to_string(&now).unwrap(), "review\nthe handoff\n");
}
