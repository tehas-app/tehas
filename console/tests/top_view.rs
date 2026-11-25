//! `tehas top`: the console file a frame opens, what a redraw costs, systemd
//! without an answer, the arguments and the size, and a run that changes
//! nothing. Run as a host runs it: bin/tehas, this build's console, live files
//! and a tree of host numbers written here, systemctl stubbed. Moved from
//! test/top_view_test.sh.

mod common;
#[path = "top/mod.rs"]
mod top;

use common::*;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use top::*;

/// A host for these cases. sleep is stubbed, as the shell file had it: the
/// wait between two readings is on the clock and starts no program.
fn view_host(name: &str) -> World {
    let w = host(name);
    w.stub("sleep", "");
    w
}

/// The console file that running.json names for the cycle of #12.
fn console_name(w: &World, name: &str) {
    let path = w.live_file("running.json");
    let mut v: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    v["cycles"][0]["console"] = name.into();
    fs::write(&path, v.to_string()).unwrap();
}

fn console_lines(w: &World, n: u32) {
    let text: String = (1..=n).map(|i| format!("console line {i}\n")).collect();
    fs::write(w.live_file("console-12.txt"), text).unwrap();
}

/// The width and height a frame was drawn for: its longest line by its lines.
fn drawn(r: &Run) -> String {
    let lines: Vec<&str> = r.out.split('\n').collect();
    let w = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    format!("{w}x{}", lines.len())
}

/// Every file of `live/` by its bytes.
fn live_files(w: &World) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(&w.live)
        .unwrap()
        .map(|e| e.unwrap())
        .filter(|e| e.file_type().unwrap().is_file())
        .map(|e| {
            (
                e.file_name().to_string_lossy().into_owned(),
                fs::read(e.path()).unwrap(),
            )
        })
        .collect()
}

// The name of the console file is text read out of running.json, and the file
// it names is opened. A name that is not a plain file name under live/ reads
// nothing at all, whatever else it might point at.
#[test]
fn a_console_name_that_is_not_a_plain_file_name_reads_nothing() {
    let w = view_host("console-name");
    files0(&w);
    fs::write(
        w.home.join("secret.txt"),
        "A file outside live/ that must not appear\n",
    )
    .unwrap();
    for name in [
        "../secret",
        "../../secret",
        "..",
        ".secret",
        "sub/console-12.txt",
        "console 12.txt",
        "con$sole",
    ] {
        console_name(&w, name);
        let r = top(&w, "--once");
        assert_eq!(r.rc, 0, "{name}: {}", r.out);
        has(&r.out, "console   no console file");
        lacks(&r.out, "must not appear");
    }
    // The name a cycle actually writes still reads its file.
    console_name(&w, "console-12.txt");
    let r = top(&w, "--once");
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "console   #12  console-12.txt  live");
    has(&r.out, "[step develop] @developer ran the gate");
}

// A redraw costs no jq process, whatever the frame draws. The wrapper records
// the calls and then runs the real jq.
#[test]
fn a_frame_costs_no_jq_process_whatever_it_holds() {
    let w = view_host("no-jq");
    files0(&w);
    let jq = w.sh("command -v jq");
    assert_eq!(jq.rc, 0, "no jq: {}", jq.out);
    w.stub("jq", &format!("exec {} \"$@\"", jq.out));
    w.env("COLUMNS", "80");
    w.env("LINES", "200");
    console_lines(&w, 2);
    let r = top(&w, "--once");
    assert_eq!(r.rc, 0, "{}", r.out);
    let short = r.out.lines().count();
    console_lines(&w, 60);
    let r = top(&w, "--once");
    assert_eq!(r.rc, 0, "{}", r.out);
    let long = r.out.lines().count();
    // The long frame really is 58 lines longer, and neither called jq.
    assert_eq!(long - short, 58);
    assert_eq!(w.calls("jq"), "");
}

#[test]
fn standard_output_that_is_not_a_terminal_gets_no_escape_codes() {
    let w = view_host("no-escapes");
    files0(&w);
    let r = top(&w, "--once");
    assert_eq!(control_bytes(&r.out), 0);
    let r = top(&w, "");
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(control_bytes(&r.out), 0);
    has(&r.out, "tehas top   snapshot");
}

#[test]
fn systemd_that_gives_no_answer_is_a_null_section_and_one_line() {
    let w = view_host("systemd-down");
    w.stub(
        "systemctl",
        r#"echo "Failed to connect to bus: No such file or directory" >&2; exit 1"#,
    );
    files0(&w);
    assert!(json_of(&top(&w, "--json"))["units"].is_null());
    let r = top(&w, "--once");
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "units     systemctl --user gave no answer");
    has(&r.out, "cycle     #12 running");
}

#[test]
fn systemd_with_no_tehas_units_is_an_empty_section_and_one_line() {
    let w = view_host("no-units");
    w.stub("systemctl", "exit 0");
    files0(&w);
    assert_eq!(json_of(&top(&w, "--json"))["units"].to_string(), "[]");
    let r = top(&w, "--once");
    has(&r.out, "units     no tehas units");
}

#[test]
fn every_takes_a_whole_number_from_1_to_64() {
    let w = view_host("every");
    files0(&w);
    w.env("LINES", "10");
    w.env("COLUMNS", "80");
    let r = top(&w, "--once --every 5");
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "tehas top   snapshot");
    for bad in ["0", "65", "100", "-1", "abc", "", "5x"] {
        let r = top(&w, &format!("--every {bad}"));
        assert_eq!(r.rc, 64, "--every {bad}: {}", r.out);
    }
    let r = top(&w, "--every");
    assert_eq!(r.rc, 64, "{}", r.out);
    has(&r.out, "--every wants a whole number from 1 to 64");
}

// The frame fills the height and is cut to the width, so the two say what size
// it was drawn for. A long console takes the rows and a long title the width.
#[test]
fn the_size_comes_from_columns_and_lines_then_the_terminal_then_80_by_24() {
    let w = view_host("size");
    files(&w, &"long title ".repeat(60));
    console_lines(&w, 300);
    w.stub("stty", r#"echo "30 100""#);
    let sized = |cols: &str, lines: &str| {
        proc_env(&w, &proc1(&w));
        w.env("COLUMNS", cols);
        w.env("LINES", lines);
        drawn(&top(&w, "--once"))
    };
    assert_eq!(sized("60", "40"), "60x40");
    assert_eq!(w.calls("stty"), "");
    // One of the two given: the terminal says the other, rows first.
    assert_eq!(sized("60", ""), "60x30");
    assert_eq!(sized("abc", "40"), "100x40");
    let calls = w.calls("stty");
    let mut asked: Vec<&str> = calls.lines().collect();
    asked.sort();
    asked.dedup();
    assert_eq!(asked, ["size"]);
    // A terminal that cannot be asked: 80 by 24.
    w.stub("stty", "exit 1");
    assert_eq!(sized("", ""), "80x24");
    // Clamped both ways.
    assert_eq!(sized("5", "1000"), "20x200");
}

#[test]
fn an_unknown_argument_is_exit_64_with_one_line() {
    let w = view_host("unknown");
    files0(&w);
    let r = top(&w, "--watch");
    assert_eq!(r.rc, 64, "{}", r.out);
    has(&r.out, "tehas top: unknown argument: --watch");
    assert_eq!(w.calls("systemctl"), "");
}

#[test]
fn no_live_directory_at_all_is_exit_1() {
    let w = view_host("no-live");
    fs::remove_dir_all(&w.home).unwrap();
    let r = top(&w, "--once");
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "no live directory");
}

#[test]
fn a_reader_that_goes_away_is_exit_141_with_nothing_on_standard_error() {
    let w = view_host("reader-gone");
    files0(&w);
    for cmd in [top_cmd("--once"), top_cmd("--json")] {
        let r = w.sh(&format!(
            r#"{{ command -p sleep 0.3; {cmd} 2>"$T/err"; }} | true; exit ${{PIPESTATUS[0]}}"#
        ));
        assert_eq!(r.rc, 141, "{cmd}: {}", r.out);
        assert_eq!(fs::read_to_string(w.t().join("err")).unwrap(), "");
    }
}

#[test]
fn a_run_changes_nothing_under_tehas_home() {
    let w = view_host("changes-nothing");
    files0(&w);
    let before = live_files(&w);
    w.env("COLUMNS", "80");
    w.env("LINES", "20");
    let r = w.sh(&format!(
        "{} && {} >/dev/null",
        top_cmd("--once"),
        top_cmd("--json")
    ));
    assert_eq!(r.rc, 0, "{}", r.out);
    let after = live_files(&w);
    assert!(after == before, "the live files changed");
    assert_eq!(
        after.keys().map(String::as_str).collect::<Vec<_>>(),
        [
            "console-12.txt",
            "health.json",
            "running.json",
            "status.json"
        ]
    );
}

#[test]
fn the_frame_comes_from_the_snapshot_alone_a_snapshot_with_nothing_in_it() {
    let w = view_host("empty-snapshot");
    fs::write(
        w.t().join("snap"),
        r#"{"generatedAt":"2026-01-01T00:00:00Z"}"#,
    )
    .unwrap();
    let r = w.sh(r#""$TEHAS_CONSOLE" top frame 80 24 <"$T/snap""#);
    assert_eq!(r.rc, 0, "{}", r.out);
    for line in [
        "factory   no health file yet",
        "cycle     no cycle written yet",
        "host      no reading of /proc",
        "units     systemctl --user gave no answer",
        "console   no console file",
    ] {
        has(&r.out, line);
    }
}
