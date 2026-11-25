//! `tehas top`: the snapshot `--json` gives, its host section and the reading
//! it is computed from, run as a host runs them: bin/tehas, this build's
//! console, live files and a tree of host numbers written here, systemctl
//! stubbed. Moved from test/top_snapshot_test.sh.

mod common;
#[path = "top/mod.rs"]
mod top;

use common::*;
use serde_json::Value;
use std::fs;
use std::time::Instant;
use top::*;

/// The rows of a process set: pids from `first` to `last`, each named after
/// itself, each next one having used `step` ticks more than the last, from
/// `base` for the first.
fn pid_rows(first: u64, last: u64, base: u64, step: u64) -> Vec<String> {
    (first..=last)
        .map(|p| format!("{p}|p{p}|{}|10", base + (p - first) * step))
        .collect()
}

/// The processes of the host section of a snapshot, each as JSON text, in its
/// order.
fn processes(text: &str) -> Vec<String> {
    let v = text.parse::<Value>().unwrap();
    v["host"]["processes"]
        .as_array()
        .unwrap_or_else(|| panic!("no processes in {text}"))
        .iter()
        .map(Value::to_string)
        .collect()
}

/// A field of each process of the host section, as JSON text.
fn of_each(text: &str, key: &str) -> Vec<String> {
    processes(text)
        .iter()
        .map(|p| p.parse::<Value>().unwrap()[key].to_string())
        .collect()
}

/// The names of the processes of the host section, in its order.
fn names(text: &str) -> Vec<String> {
    of_each(text, "name")
        .iter()
        .map(|n| n.parse::<Value>().unwrap().as_str().unwrap().to_string())
        .collect()
}

/// The host's CPU figure and each process's, as JSON text.
fn cpu_figures(text: &str) -> String {
    let host = text.parse::<Value>().unwrap()["host"]["cpuPercent"].to_string();
    let mut all = vec![host];
    all.extend(of_each(text, "cpuPercent"));
    format!("[{}]", all.join(","))
}

/// The end of the JSON value that starts at byte `i` of `s`.
fn value_end(s: &str, i: usize) -> usize {
    let b = s.as_bytes();
    let (mut depth, mut in_str, mut esc) = (0usize, false, false);
    let mut j = i;
    while j < b.len() {
        let c = b[j];
        if in_str {
            match c {
                _ if esc => esc = false,
                b'\\' => esc = true,
                b'"' => {
                    in_str = false;
                    if depth == 0 {
                        return j + 1;
                    }
                }
                _ => {}
            }
        } else {
            match c {
                b'"' => in_str = true,
                b'{' | b'[' => depth += 1,
                b'}' | b']' if depth == 0 => return j,
                b'}' | b']' => {
                    depth -= 1;
                    if depth == 0 {
                        return j + 1;
                    }
                }
                b',' if depth == 0 => return j,
                _ => {}
            }
        }
        j += 1;
    }
    j
}

/// The values of an object's or an array's text, as written, each with its
/// key ("" in an array).
fn parts(text: &str) -> Vec<(&str, &str)> {
    let t = text.trim();
    let inner = &t[1..t.len() - 1];
    let mut out = vec![];
    let mut i = 0;
    while i < inner.len() {
        i += inner[i..].len() - inner[i..].trim_start().len();
        if i >= inner.len() {
            break;
        }
        let mut key = "";
        if t.starts_with('{') {
            let end = value_end(inner, i);
            key = &inner[i + 1..end - 1];
            i = end + inner[end..].find(':').unwrap() + 1;
            i += inner[i..].len() - inner[i..].trim_start().len();
        }
        let end = value_end(inner, i);
        out.push((key, inner[i..end].trim_end()));
        i = end + 1;
    }
    out
}

/// The text of the value at `path` of a JSON text, as the console wrote it: a
/// number read through JSON would lose its digits.
fn at<'a>(text: &'a str, path: &[&str]) -> &'a str {
    path.iter().fold(text.trim(), |t, key| {
        parts(t)
            .into_iter()
            .find(|(k, _)| k == key)
            .unwrap_or_else(|| panic!("no {key} in {t}"))
            .1
    })
}

/// The text of the value of `key` inside the object of `section`.
fn raw(out: &str, section: &str, key: &str) -> String {
    at(out, &[section, key]).to_string()
}

/// The text of the field `key` of each process of the host section.
fn raw_of_each(text: &str, key: &str) -> Vec<String> {
    parts(at(text, &["host", "processes"]))
        .into_iter()
        .map(|(_, p)| at(p, &[key]).to_string())
        .collect()
}

#[test]
fn json_gives_one_object_generatedat_and_one_key_per_section() {
    let w = host("one-object");
    files0(&w);
    let r = top(&w, "--json");
    assert_eq!(r.rc, 0, "{}", r.out);
    let v = json_of(&r);
    assert!(v["generatedAt"].is_string());
    assert_eq!(v.as_object().unwrap().len(), 8);
    for key in ["factory", "queue", "cycle", "host", "console"] {
        assert!(v[key].is_object(), "{key}: {}", v[key]);
    }
    assert!(v["units"].is_array());
    assert_eq!(v["factory"]["status"], "pass");
    assert_eq!(v["queue"]["counts"]["inLine"], 1);
    assert_eq!(v["cycle"]["cycles"][0]["issue"], 12);
    assert_eq!(v["console"]["name"], "console-12.txt");
}

#[test]
fn the_snapshot_reads_the_live_files_and_asks_systemd_nothing_else() {
    let w = host("asks-systemd");
    files0(&w);
    let r = top(&w, "--json");
    assert_eq!(r.rc, 0, "{}", r.out);
    let calls = w.calls("systemctl");
    has(
        &calls,
        "list-units --all --full --plain --no-legend tehas-*",
    );
    has(
        &calls,
        "show -p Id,LoadState,ActiveState,SubState,UnitFileState,NextElapseUSecRealtime",
    );
    // Nothing else: the harness's deny stubs fail the case on any call of
    // docker, curl, ssh or the GitHub CLI.
}

#[test]
fn a_source_that_is_not_there_is_null_and_the_others_are_still_there() {
    let w = host("one-missing");
    files0(&w);
    for f in ["console-12.txt", "running.json"] {
        let _ = fs::remove_file(w.live_file(f));
    }
    let r = top(&w, "--json");
    assert_eq!(r.rc, 0, "{}", r.out);
    let v = json_of(&r);
    let nulls: Vec<bool> = ["factory", "queue", "cycle", "units", "console"]
        .iter()
        .map(|k| v[k].is_null())
        .collect();
    assert_eq!(nulls, [false, false, true, false, true]);
}

#[test]
fn every_source_missing_is_null_for_its_key_and_json_still_exits_0() {
    let w = host("all-missing");
    let r = top(&w, "--json");
    assert_eq!(r.rc, 0, "{}", r.out);
    let v = json_of(&r);
    for k in ["factory", "queue", "cycle", "console"] {
        assert!(v[k].is_null(), "{k}: {}", v[k]);
    }
}

#[test]
fn a_snapshot_fed_to_the_renderer_gives_the_same_frame_as_once() {
    let w = host("renderer");
    files0(&w);
    let r = top(&w, "--json");
    assert_eq!(r.rc, 0, "{}", r.out);
    let r = w.sh(&format!(
        r#"{} | "$TEHAS_CONSOLE" top frame 80 24 >"$T/by-snapshot"; {} >"$T/by-files""#,
        top_cmd("--json"),
        top_cmd("--once")
    ));
    assert_eq!(r.rc, 0, "{}", r.out);
    // The first line names the moment the snapshot was taken, which is later
    // for one of the two runs. The frame below it is the same.
    let below = |f: &str| {
        let text = fs::read_to_string(w.t().join(f)).unwrap();
        text.lines().skip(1).collect::<Vec<_>>().join("\n")
    };
    assert_eq!(below("by-snapshot"), below("by-files"));
}

#[test]
fn the_host_section_names_cpus_load_cpu_use_memory_and_the_processes() {
    let w = host("host-section");
    files0(&w);
    let r = top(&w, "--json");
    assert_eq!(r.rc, 0, "{}", r.out);
    let v = json_of(&r);
    let h = &v["host"];
    assert_eq!(h["cpus"], 2);
    assert_eq!(raw(&r.out, "host", "load"), "[0.10,0.20,0.30]");
    assert_eq!(h["memory"]["totalMb"], 64);
    assert_eq!(h["memory"]["usedMb"], 32);
    let mut n = names(&r.out);
    n.sort();
    assert_eq!(n.join(","), "gh,pi-rukas,systemd,tehas-cycle");
    // Only these five things about a process, whatever the tree held beside
    // them.
    for p in h["processes"].as_array().unwrap() {
        let mut keys: Vec<&String> = p.as_object().unwrap().keys().collect();
        keys.sort();
        assert_eq!(
            keys,
            ["cpuPercent", "memPercent", "name", "pid", "upSeconds"],
            "{p}"
        );
        assert!(p["pid"].is_number() && p["upSeconds"].is_number() && p["memPercent"].is_number());
    }
    // One snapshot reads the same tree twice, and a tree that says the same
    // thing both times measured nothing: its CPU figures are null, not a guess
    // from what a process has used since it started.
    assert_eq!(cpu_figures(&r.out), "[null,null,null,null,null]");
}

#[test]
fn a_host_that_cannot_be_read_is_a_null_section_and_one_line() {
    let w = host("no-proc");
    files0(&w);
    proc_env(&w, &w.t().join("no-such-tree"));
    let r = top(&w, "--json");
    assert_eq!(r.rc, 0, "{}", r.out);
    let v = json_of(&r);
    assert!(v["host"].is_null());
    assert_eq!(v.as_object().unwrap().len(), 8);
    assert!(v["reading"].is_null());
    let r = top(&w, "--once");
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "host      no reading of /proc");
    has(&r.out, "cycle     #12 running");
}

// The snapshot is read and put together by the console beside the commands.
// Without one there is no snapshot, and the command says what to do.
#[test]
fn a_host_with_no_console_has_no_snapshot_and_says_to_install() {
    let w = host("no-console");
    files0(&w);
    let r = w.sh(&format!(
        r#"TEHAS_CONSOLE="$T/no-console" {}"#,
        top_cmd("--json")
    ));
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        &format!(
            "tehas: no console at {}/no-console: run install.sh",
            w.t().display()
        ),
    );
}

#[test]
fn a_process_is_named_by_its_short_name_never_by_its_command_line() {
    let w = host("short-name");
    files0(&w);
    let r = top(&w, "--json");
    assert_eq!(r.rc, 0, "{}", r.out);
    lacks(&r.out, "SecretTokenInACommandLine");
    lacks(&r.out, "--repo");
    has(&r.out, r#""name":"gh""#);
    let r = top(&w, "--once");
    assert_eq!(r.rc, 0, "{}", r.out);
    lacks(&r.out, "SecretTokenInACommandLine");
    has(&r.out, "  400  gh");
}

// The name is what lies between the first ( and the last ) of the stat file,
// so a name that holds brackets or quotes stays one name and reaches the
// snapshot as text. A directory that is not a pid, and a pid with no stat file,
// are not processes: only the numbers name one.
#[test]
fn a_short_name_holding_brackets_or_quotes_is_still_one_name() {
    let w = host("brackets");
    files0(&w);
    let proc3 = w.t().join("proc3");
    proc_files(
        &proc3,
        1000,
        CPU,
        &[
            "100 systemd 150 300",
            r#"500 a(b)c\"d 20 100"#,
            "600 [bracket] 30 100",
        ],
    );
    fs::create_dir_all(proc3.join("notapid")).unwrap();
    fs::create_dir_all(proc3.join("700")).unwrap();
    fs::write(
        proc3.join("notapid/stat"),
        "700 (notapid) S 1 1 1 0 -1 1 0 0 0 0 1 1 0 0 20 0 1 0 900 1048576 10 0\n",
    )
    .unwrap();
    proc_env(&w, &proc3);
    let r = top(&w, "--json");
    assert_eq!(r.rc, 0, "{}", r.out);
    let v = json_of(&r);
    let pids: Vec<String> = v["host"]["processes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["pid"].to_string())
        .collect();
    assert_eq!(pids.join(","), "100,500,600");
    assert_eq!(names(&r.out).join(","), r#"systemd,a(b)c\"d,[bracket]"#);
    let r = top(&w, "--once");
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "a(b)c");
    lacks(&r.out, "notapid");
}

// A host that says nothing about its load or its memory is still a reading:
// the line says ? where a figure is missing, and holds no separator with
// nothing either side of it.
#[test]
fn a_tree_with_no_load_averages_and_no_memory_gives_one_line_of_question_marks() {
    let w = host("question-marks");
    files0(&w);
    let proc4 = w.t().join("proc4");
    proc_files(&proc4, 1000, CPU, &["100 systemd 150 300"]);
    fs::remove_file(proc4.join("loadavg")).unwrap();
    fs::remove_file(proc4.join("meminfo")).unwrap();
    proc_env(&w, &proc4);
    let v = json_of(&top(&w, "--json"));
    for k in ["load", "memory", "cpuPercent"] {
        assert!(v["host"][k].is_null(), "{k}: {}", v["host"][k]);
    }
    let r = top(&w, "--once");
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "host      2 cpus, load ?, cpu ?%, mem ? of ? MB");
}

#[test]
fn the_snapshot_carries_the_reading_its_host_section_was_computed_from() {
    let w = host("carries-reading");
    files0(&w);
    let r = top(&w, "--json");
    assert_eq!(r.rc, 0, "{}", r.out);
    let v = json_of(&r);
    assert!(v["reading"].is_object());
    assert_eq!(v["reading"]["clock"].as_f64(), Some(1000.0));
    assert_eq!(v["reading"]["cpus"], 2);
    assert_eq!(v["reading"]["procs"].as_array().unwrap().len(), 4);
    // A tree that cannot be read gives no reading either: the next call has
    // nothing to compare against and takes a pair.
    proc_env(&w, &w.t().join("no-such-tree"));
    assert!(json_of(&top(&w, "--json"))["reading"].is_null());
}

#[test]
fn a_snapshot_given_the_reading_of_the_frame_before_does_not_wait_for_a_second_one() {
    let w = host("no-second-wait");
    files0(&w);
    let began = Instant::now();
    let r = w.sh(r#""$TEHAS_CONSOLE" top snapshot 16 >"$T/one""#);
    let elapsed = began.elapsed().as_millis();
    assert_eq!(r.rc, 0, "{}", r.out);
    // Nothing to compare with: a pair of readings, and the second one a second
    // on. The wait is on the clock rather than a `sleep` child, so it starts no
    // program and ends about a second after it began: the wall time says a wait
    // happened, with a margin, and the reading it reports is the second of the
    // pair.
    assert!(elapsed >= 900, "no wait: {elapsed}ms");
    let one = fs::read_to_string(w.t().join("one")).unwrap();
    assert_eq!(raw(&one, "reading", "clock"), "1000.00");
    assert_eq!(cpu_figures(&one), "[null,null,null,null,null]");
    // Time passes, as it does between two frames of a view. The reading of the
    // frame before is handed to the next one, and that call takes no wait at
    // all. The same tree five seconds on: tehas-cycle used 5 s of CPU in the
    // window, and the aggregate line moved by 1000 ticks of which 500 were idle
    // - 50% over the host, and 100% for that one process.
    let p1 = proc1(&w);
    proc_files(
        &p1,
        1005,
        "1500 100 500 8500 200 10 5 20",
        &[
            "100 systemd 160 300",
            "200 tehas-cycle 500 800",
            "300 pi-rukas 735 1200",
            "400 gh 12 100",
        ],
    );
    // The clock of the tree is a pipe that one writer fills once: a reading
    // opens it and reads it to its end, and a second reading would wait on a
    // writer that never comes. So the call ends only if it took one reading,
    // and the timeout is no measure of speed: the one reading ends the call
    // however loaded the host is, and only a second reading reaches the
    // timeout.
    fs::remove_file(p1.join("uptime")).unwrap();
    let r = w.sh(&format!(
        r#"mkfifo "$TEHAS_PROC_DIR/uptime" || exit 9
  {{ printf "1005.00 0.00\n" >"$TEHAS_PROC_DIR/uptime"; }} & writer=$!
  timeout 10 "$TEHAS_CONSOLE" top snapshot 16 '{}' >"$T/two"; rc=$?
  kill "$writer" 2>/dev/null; wait "$writer" 2>/dev/null; exit "$rc""#,
        at(&one, &["reading"])
    ));
    assert_eq!(r.rc, 0, "{}", r.out);
    // The reading of the frame before was given, so no second reading is
    // taken and no wait is taken with it: .reading below is the one reading of
    // the pipe.
    let two = fs::read_to_string(w.t().join("two")).unwrap();
    assert_eq!(raw(&two, "reading", "clock"), "1005.00");
    // Measured over the five seconds between the two readings: 50% for the
    // host, and 100% for the process that used them.
    assert_eq!(raw(&two, "host", "cpuPercent"), "50");
    let names = raw_of_each(&two, "name");
    let of_cycle = |key| {
        let all = raw_of_each(&two, key);
        let picked: Vec<String> = names
            .iter()
            .zip(all)
            .filter(|(n, _)| *n == r#""tehas-cycle""#)
            .map(|(_, v)| v)
            .collect();
        format!("[{}]", picked.join(","))
    };
    assert_eq!(of_cycle("cpuPercent"), "[100]");
    assert_eq!(of_cycle("upSeconds"), "[995]");
    // The frame is drawn from the snapshot as it is: the extra key changes
    // nothing that is printed.
    let r = w.sh(r#""$TEHAS_CONSOLE" top frame 80 24 <"$T/two""#);
    has(
        &r.out,
        "host      2 cpus, load 0.10 0.20 0.30, cpu 50%, mem 32 of 64 MB",
    );
    has(&r.out, "tehas-cycle");
}

/// Two trees of one host, a hundred seconds apart, from the same process set:
/// tehas-cycle under 200 and twenty processes from p500 up. In the window
/// tehas-cycle used 4000 ticks of one CPU (40%). The aggregate line of the
/// later tree holds 5000 ticks of which 4000 were idle, against 1000 of which
/// 4000 were idle a hundred seconds before: 4000 ticks of use in the window and
/// all of them that one process, so the host reads 100%. Of the twenty, each
/// next pid used 100 ticks more, so the fifteen that used the most are p515
/// down, and p500 and p501 are out beside the process that worked.
fn every_two(w: &World) {
    let rows = |last, step| {
        let mut r = vec![format!(
            "200 tehas-cycle {} 800",
            if step == 0 { 0 } else { 4000 }
        )];
        r.extend(pid_rows(500, last, 1000, step));
        r
    };
    let earlier = rows(519, 0);
    let later = rows(515, 100);
    proc_files(
        &w.t().join("earlier"),
        1000,
        "1000 0 0 4000 0 0 0 0",
        &earlier,
    );
    proc_files(&w.t().join("later"), 1100, "5000 0 0 4000 0 0 0 0", &later);
}

/// A snapshot of the tree `later`, given as the reading of the frame before
/// what `console top reading <earlier>` prints, a shell word.
fn against_earlier(w: &World, earlier: &str) -> String {
    proc_env(w, &w.t().join("later"));
    let r = w.sh(&format!(
        r#""$TEHAS_CONSOLE" top snapshot 16 "$("$TEHAS_CONSOLE" top reading {earlier})" >"$T/host""#
    ));
    assert_eq!(r.rc, 0, "{}", r.out);
    fs::read_to_string(w.t().join("host")).unwrap()
}

#[test]
fn a_snapshot_given_the_reading_of_the_frame_before_reports_the_difference() {
    let w = host("difference");
    files0(&w);
    // The two trees are read through the <dir> of console top reading, so one
    // case can hold a pair of them: the later is the tree named in
    // TEHAS_PROC_DIR and the earlier is asked for by path, which is how the
    // reading of the frame before reaches the next call. Swapping the two would
    // change the figures. Start ticks come from the pid, so upSeconds of a row
    // is the later clock less the start ticks its pid gives it.
    every_two(&w);
    let text = against_earlier(&w, r#""$T/earlier""#);
    let v = text.parse::<Value>().unwrap();
    // The host over the window: 4000 ticks of use out of the 4000 the aggregate
    // line gained, and none of them idle - so the host reads 100% and all of it
    // is the one process.
    assert_eq!(raw(&text, "host", "cpuPercent"), "100");
    let names = raw_of_each(&text, "name");
    let cpus = raw_of_each(&text, "cpuPercent");
    assert_eq!(
        format!("{} {} {}", names[0].trim_matches('"'), cpus[0], names.len()),
        "tehas-cycle 40 15"
    );
    // The fifteen that used the most CPU in this window: tehas-cycle, then the
    // pids from p515 down. p516 to p519, which are in the earlier reading only,
    // are not measured: there is nothing to take a difference between.
    assert_eq!(
        format!("[{}]", names.join(",")),
        r#"["tehas-cycle","p515","p514","p513","p512","p511","p510","p509","p508","p507","p506","p505","p504","p503","p502"]"#
    );
    let mut unique = cpus.clone();
    unique.sort_by(|a, b| {
        let n = |s: &String| s.parse::<f64>().unwrap();
        n(a).total_cmp(&n(b))
    });
    unique.dedup();
    assert_eq!(
        format!("[{}]", unique.join(",")),
        "[2,3,4,5,6,7,8,9,10,11,12,13,14,15,40]"
    );
    // The first row is tehas-cycle, pid 200: the clock of the later reading,
    // 1100 s, less the start ticks its pid gives it, (900 + 200 - 100) * 100 /
    // 100.
    assert_eq!(raw_of_each(&text, "upSeconds")[0], "1090");
    // Load and memory are the text of the tree: 0.10 stays 0.10.
    assert_eq!(raw(&text, "host", "load"), "[0.10,0.20,0.30]");
    let mut keys: Vec<&String> = v["host"]["memory"].as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(keys, ["percent", "totalMb", "usedMb"]);
}

// A process with no row in the earlier reading has nothing to take a difference
// from: its CPU is nothing, not a figure of the window. tehas-cycle is under
// 5000 here and the earlier tree knows only 200; p501 is under a pid the
// earlier tree does not name at all. The rule that a pid which began again in
// the window is not read as one long process is pinned in console/src/top.rs,
// where the start time of one pid can be given two values.
#[test]
fn a_process_with_no_row_in_the_earlier_reading_is_not_measured() {
    let w = host("not-measured");
    files0(&w);
    proc_files(
        &w.t().join("earlier"),
        1000,
        "1000 0 0 4000 0 0 0 0",
        &["200 tehas-cycle 0 800", "500 p500 1000 10"],
    );
    proc_files(
        &w.t().join("later"),
        1100,
        "5500 0 0 4000 0 0 0 0",
        &[
            "5000 tehas-cycle 4000 800",
            "500 p500 1100 10",
            "501 p501 1200 10",
        ],
    );
    // As the shell case ran: it named the earlier tree by a variable its run
    // did not have, so `top reading` was given an empty path, failed, and the
    // snapshot had no reading of the frame before. With the earlier tree
    // really given, the console leaves out the rows it cannot pair and reads
    // the host at 100%, which is not what the comment above says. Kept as it
    // ran; #2854 decides which is right.
    let text = against_earlier(&w, r#""""#);
    let v = text.parse::<Value>().unwrap();
    let mut n = names(&text);
    n.sort();
    assert_eq!(n, ["p500", "p501", "tehas-cycle"], "{text}");
    // A row with no row behind it is not a measurement, and it is not the
    // host's figure either.
    for p in v["host"]["processes"].as_array().unwrap() {
        if p["name"] != "p500" {
            assert!(p["cpuPercent"].is_null(), "{p}");
        }
    }
    assert!(v["host"]["cpuPercent"].is_null());
}
