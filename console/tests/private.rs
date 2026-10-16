//! Names the host knows, given with --private, are masked in what the
//! console writes.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const CONSOLE: &str = env!("CARGO_BIN_EXE_console");

fn masked(args: &[&str], input: &str) -> (bool, String) {
    let mut console = Command::new(CONSOLE)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    console
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = console.wait_with_output().unwrap();
    (out.status.success(), String::from_utf8(out.stdout).unwrap())
}

#[test]
fn the_status_line_is_masked_of_the_names_given() {
    let (ok, out) = masked(
        &[
            "--mask",
            "--private",
            "models.lan",
            "--private",
            "bot@example.com",
        ],
        "asked models.lan as bot@example.com",
    );
    assert!(ok);
    assert_eq!(out, "asked [private name] as [private name]");
}

#[test]
fn the_mask_alone_still_works() {
    let (ok, out) = masked(&["--mask"], "plain");
    assert!(ok);
    assert_eq!(out, "plain");
}

#[test]
fn a_private_without_a_value_or_another_flag_is_a_usage_error() {
    let status = Command::new(CONSOLE)
        .args(["--mask", "--private"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(2));
    let status = Command::new(CONSOLE)
        .args(["--mask", "--other"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(2));
}

#[test]
fn entries_and_the_speaker_are_masked_of_the_names_given() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("private");
    let _ = fs::remove_dir_all(&dir);
    let today = Command::new("date").args(["-u", "+%F"]).output().unwrap();
    let today = String::from_utf8(today.stdout).unwrap();
    let day = dir.join("runs").join(today.trim());
    fs::create_dir_all(&day).unwrap();
    let (progress, out) = (dir.join("work-7.ndjson"), dir.join("console-7.txt"));
    fs::write(
        &progress,
        "{\"v\":1,\"kind\":\"log\",\"message\":\"asked models.lan\"}\n",
    )
    .unwrap();
    fs::write(
        day.join("aa-bb-gpu1box.json"),
        concat!(
            "{\"type\":\"session\",\"cwd\":\"/w\"}\n",
            "{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"hi\"}]}}\n",
        ),
    )
    .unwrap();
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 5;
    let mut console = Command::new(CONSOLE)
        .args(["--progress", progress.to_str().unwrap()])
        .args(["--runs", dir.join("runs").to_str().unwrap()])
        .args([
            "--out",
            out.to_str().unwrap(),
            "--since",
            &since.to_string(),
        ])
        .args(["--private", "models.lan", "--private", "gpu1box"])
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let started = Instant::now();
    let written = loop {
        let text = fs::read_to_string(&out).unwrap_or_default();
        if text.lines().count() >= 3 || started.elapsed() > Duration::from_secs(20) {
            break text;
        }
        sleep(Duration::from_millis(50));
    };
    let _ = console.kill();
    let _ = console.wait();
    assert!(written.contains("asked [private name]"), "{written}");
    assert!(
        written
            .lines()
            .any(|l| l.contains("[private name]") && l.ends_with(" hi")),
        "{written}"
    );
    assert!(
        !written.contains("models.lan") && !written.contains("gpu1box"),
        "{written}"
    );
}
