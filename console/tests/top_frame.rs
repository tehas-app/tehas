//! `tehas top`: the frame --once draws, its sections, its size and what it
//! lets through to the terminal. Run as a host runs it: bin/tehas, this
//! build's console, live files and a tree of host numbers written here,
//! systemctl stubbed. Moved from test/top_frame_test.sh.

mod common;
#[path = "top/mod.rs"]
mod top;

use common::*;
use serde_json::json;
use std::fs;
use tehas_core::time::{iso, now};
use top::*;

/// A host for these cases. The readings are the same files a second apart or
/// not, so the first frame's wait for a second reading takes no time here:
/// sleep is stubbed (top_snapshot covers the wait).
fn frame_host(name: &str) -> World {
    let w = host(name);
    w.stub("sleep", "");
    w
}

/// Every later run reads proc1 and is drawn at this size, until reset().
fn sized(w: &World, columns: u32, lines: u32) {
    w.set_env(&[
        &format!("TEHAS_PROC_DIR={}", proc1(w).display()),
        &format!("COLUMNS={columns}"),
        &format!("LINES={lines}"),
    ]);
}

fn console_lines(w: &World, n: u32) {
    let text: String = (1..=n).map(|i| format!("console line {i}\n")).collect();
    fs::write(w.live_file("console-12.txt"), text).unwrap();
}

/// The lines of a frame, as `wc -l` counts what the shell printed of it.
fn lines(r: &Run) -> usize {
    r.out.split('\n').count()
}

/// The lines of a frame longer than `width` characters.
fn longer(r: &Run, width: usize) -> usize {
    r.out
        .split('\n')
        .filter(|l| l.chars().count() > width)
        .count()
}

/// The C1 controls, U+0080 to U+009F: in UTF-8 the pair 0xc2 and a byte from
/// 0x80 to 0x9f, which the output, read as text, holds as one character each.
fn c1_bytes(text: &str) -> usize {
    text.chars()
        .filter(|c| ('\u{80}'..='\u{9f}').contains(c))
        .count()
}

#[track_caller]
fn rc(r: &Run, want: i32) {
    assert_eq!(r.rc, want, "the exit of a run that printed:\n{}", r.out);
}

#[test]
fn a_frame_with_a_running_cycle_has_all_six_sections() {
    let w = frame_host("six");
    files0(&w);
    let r = top(&w, "--once");
    rc(&r, 0);
    for line in [
        "tehas top",
        "factory   pass: Working on #12.",
        "queue     1 in line, 1 building, 1 in review, 1 stopped",
        "  12     running    Two cycles side by side",
        "cycle     #12 running, step develop, started 10 min ago",
        "host      2 cpus, load 0.10 0.20 0.30",
        "  100  systemd",
        "  400  gh",
        "units     3 units, 2 active",
        "  tehas-dispatch.timer        enabled  active  next 10:00:00",
        "console   #12  console-12.txt  live",
        "[step develop] @developer ran the gate",
    ] {
        has(&r.out, line);
    }
}

#[test]
fn a_cycle_that_ended_is_shown_as_the_last_one_marked_ended() {
    let w = frame_host("ended");
    files0(&w);
    let t = now();
    let running = json!({"cycles": [], "last": {"issue": 12, "running": false,
        "status": "handoff", "step": "review", "console": "console-12.txt",
        "startedAt": t - 900, "updatedAt": t - 120}});
    fs::write(w.live_file("running.json"), running.to_string()).unwrap();
    let r = top(&w, "--once");
    has(&r.out, "cycle     #12 handoff at step review, 2 min ago");
    has(&r.out, "console   #12  console-12.txt  ended");
}

#[test]
fn a_missing_status_json_is_one_line_and_the_rest_of_the_frame_is_there() {
    let w = frame_host("no-status");
    files0(&w);
    fs::remove_file(w.live_file("status.json")).unwrap();
    let r = top(&w, "--once");
    rc(&r, 0);
    has(&r.out, "queue     no snapshot yet");
    lacks(&r.out, "Two cycles side by side");
    has(&r.out, "factory   pass: Working on #12.");
    has(&r.out, "cycle     #12 running");
}

#[test]
fn a_missing_running_json_is_one_line_and_the_rest_of_the_frame_is_there() {
    let w = frame_host("no-running");
    files0(&w);
    fs::remove_file(w.live_file("running.json")).unwrap();
    let r = top(&w, "--once");
    rc(&r, 0);
    has(&r.out, "cycle     no cycle written yet");
    has(&r.out, "console   no console file");
    has(&r.out, "queue     1 in line");
}

#[test]
fn a_missing_console_file_is_one_line_and_the_rest_of_the_frame_is_there() {
    let w = frame_host("no-console");
    files0(&w);
    fs::remove_file(w.live_file("console-12.txt")).unwrap();
    let r = top(&w, "--once");
    rc(&r, 0);
    has(&r.out, "console   no console file");
    has(&r.out, "cycle     #12 running");
}

#[test]
fn a_health_file_older_than_ten_minutes_reads_stale_and_is_still_shown() {
    let w = frame_host("stale-health");
    files0(&w);
    let path = w.live_file("health.json");
    let mut health: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    health["time"] = iso(now() - 700).into();
    fs::write(&path, health.to_string()).unwrap();
    sized(&w, 200, 24);
    let r = top(&w, "--json");
    let snapshot = json_of(&r);
    let stale = snapshot["factory"].as_object().and_then(|o| o.get("stale"));
    assert_eq!(stale, Some(&json!(true)), "{}", r.out);
    let r = top(&w, "--once");
    rc(&r, 0);
    has(
        &r.out,
        "factory   stale: the dispatcher last reported 11 min ago",
    );
    has(&r.out, "cycle     #12 running");
}

#[test]
fn a_width_of_40_and_of_200_columns_gives_no_line_longer_than_the_width() {
    let w = frame_host("widths");
    files0(&w);
    sized(&w, 40, 16);
    let r = top(&w, "--once");
    rc(&r, 0);
    assert_eq!(longer(&r, 40), 0, "{}", r.out);
    assert_eq!(lines(&r), 16, "{}", r.out);
    sized(&w, 200, 16);
    let r = top(&w, "--once");
    assert_eq!(longer(&r, 200), 0, "{}", r.out);
}

// stty size prints the rows first and the columns after them. A terminal that
// is not square shows which way round they were read.
#[test]
fn the_rows_and_the_columns_of_the_terminal_are_not_swapped() {
    let w = frame_host("stty");
    files(&w, &"w".repeat(150));
    w.stub("stty", r#"printf "30 100\n""#);
    console_lines(&w, 40);
    proc_env(&w, &proc1(&w));
    let r = top(&w, "--once");
    rc(&r, 0);
    has(&w.calls("stty"), "size");
    // 30 rows: the frame holds 30 lines, not 100 and not the default 24.
    assert_eq!(lines(&r), 30, "{}", r.out);
    // 100 columns: the longest line is cut at 100, not at 30.
    assert_eq!(longer(&r, 100), 0, "{}", r.out);
    let max = r.out.split('\n').map(|l| l.chars().count()).max();
    assert_eq!(max, Some(100), "{}", r.out);
}

#[test]
fn a_short_terminal_drops_console_lines_not_the_sections() {
    let w = frame_host("short");
    files0(&w);
    // Named lines: a bare number could match the snapshot's clock.
    console_lines(&w, 30);
    sized(&w, 80, 12);
    let r = top(&w, "--once");
    assert_eq!(lines(&r), 12, "{}", r.out);
    for line in [
        "factory   pass",
        "queue     1 in line",
        "cycle     #12 running",
        "units     3 units",
        "console   #12",
    ] {
        has(&r.out, line);
    }
    lacks(&r.out, "console line 30");
}

#[test]
fn a_short_terminal_drops_process_rows_first_then_console_lines_then_unit_rows() {
    let w = frame_host("drop-order");
    files0(&w);
    console_lines(&w, 30);
    // The whole frame at a height that drops nothing.
    sized(&w, 80, 64);
    let r = top(&w, "--once");
    rc(&r, 0);
    let full = lines(&r) as u32;
    // One row short: a process row goes, and no console line or unit row.
    sized(&w, 80, full - 1);
    let r = top(&w, "--once");
    assert_eq!(lines(&r), (full - 1) as usize, "{}", r.out);
    lacks(&r.out, "  400  gh");
    has(&r.out, "  100  systemd");
    has(&r.out, "console line 30");
    has(&r.out, "tehas-dispatch.timer        enabled");
    // Four rows short: every process row goes, and still no console line.
    sized(&w, 80, full - 4);
    let r = top(&w, "--once");
    assert_eq!(lines(&r), (full - 4) as usize, "{}", r.out);
    lacks(&r.out, "  400  gh");
    lacks(&r.out, "  100  systemd");
    has(&r.out, "console line 30");
    has(&r.out, "host      2 cpus, load");
    // Eight rows short: the console lines go before any unit row is touched.
    sized(&w, 80, full - 8);
    let r = top(&w, "--once");
    assert_eq!(lines(&r), (full - 8) as usize, "{}", r.out);
    lacks(&r.out, "console line 30");
    lacks(&r.out, "console line 27");
    has(&r.out, "console line 26");
    has(&r.out, "tehas-dispatch.timer        enabled");
}

#[test]
fn an_escape_sequence_in_a_title_or_a_console_line_does_not_reach_the_output() {
    let w = frame_host("escape");
    files(&w, "\x1b[31mRed title");
    fs::write(
        w.live_file("console-12.txt"),
        "\x1b]0;evil\x07 console line\n",
    )
    .unwrap();
    let r = top(&w, "--once");
    rc(&r, 0);
    assert_eq!(control_bytes(&r.out), 0, "{:?}", r.out);
    has(&r.out, "Red title");
    has(&r.out, "evil console line");
}

// The C1 controls, U+0080 to U+009F, introduce a control sequence on some
// terminals. They come in two shapes: bytes in the file, which the byte filter
// reads, and a \u009b escape, which is only a control once the JSON is read.
// Text that is not ASCII and is not a control still shows.
#[test]
fn a_c1_control_does_not_reach_the_output_from_bytes_in_the_file() {
    let w = frame_host("c1-bytes");
    files(&w, "Red \u{9b}title with \u{e4}\u{f6}");
    // The control is in status.json as its bytes, not as an escape.
    let status = fs::read(w.live_file("status.json")).unwrap();
    assert!(status.windows(2).any(|p| p == [0xc2, 0x9b]));
    fs::write(
        w.live_file("console-12.txt"),
        "console\u{9b} line with \u{e4}\n",
    )
    .unwrap();
    let r = top(&w, "--once");
    rc(&r, 0);
    assert_eq!(c1_bytes(&r.out), 0, "{:?}", r.out);
    has(&r.out, "Red title with \u{e4}\u{f6}");
    has(&r.out, "console line with \u{e4}");
}

#[test]
fn a_c1_control_does_not_reach_the_output_from_a_json_escape() {
    let w = frame_host("c1-escape");
    files0(&w);
    // The escape, not yet a control character.
    let esc = r"Escaped \u009btitle äö";
    fs::write(
        w.live_file("status.json"),
        format!(
            r#"{{"generatedAt":"{}","repo":"acme/widget","paused":false,"running":[{{"number":12,"title":"{esc}"}}],"queued":[],"awaiting":[],"stopped":[]}}"#,
            iso(now() - 210)
        ) + "\n",
    )
    .unwrap();
    let r = top(&w, "--once");
    rc(&r, 0);
    assert_eq!(c1_bytes(&r.out), 0, "{:?}", r.out);
    has(&r.out, "Escaped title \u{e4}\u{f6}");
}

// The byte filter itself is tested beside it (console/src/snapshot.rs).
