//! The counts a cycle reads from the files of its container, the memory kills
//! and the refused starts, and what it says of them.

mod common;

use common::{has, lacks, World};
use std::fs;
use std::path::Path;

fn gone(p: &Path) -> bool {
    fs::symlink_metadata(p).is_err()
}

#[test]
fn the_refused_starts_are_read_as_digits_said_in_the_comment_and_the_log_and_the_file_is_removed() {
    let w = World::new("rc-forks");
    w.work(r#"echo 41 >"$TEHAS_HOME/state/cycles/7.forks""#);
    let r = w.run7();
    has(&w.gh(), "Its container reached its limit of processes (`--pids-limit`, which follows `TEHAS_MAX_CYCLES` and `TEHAS_SLICE_CONTAINERS`) and 41 starts of a process were refused: what the cycle ran after that may not have run.");
    has(&r.out, "forks_refused=41");
    assert!(gone(&w.state().join("7.forks")));
}

#[test]
fn the_pids_limit_on_the_second_line_of_the_count_is_named_with_the_settings_it_follows() {
    let w = World::new("rc-pids");
    w.fx("forks", "21\n2048\n");
    w.work(r#"cp "$T/fx/forks" "$TEHAS_HOME/state/cycles/7.forks""#);
    let r = w.run7();
    has(
        &w.gh(),
        "Its container reached its limit of 2048 processes (`--pids-limit`, which follows",
    );
    has(&r.out, "forks_refused=21");
}

#[test]
fn a_count_of_refused_starts_that_is_not_digits_or_is_a_link_counts_as_nothing() {
    for bad in ["many", "4; reboot", "-4", "04", ""] {
        let w = World::new("rc-badforks");
        w.fx("forks", &format!("{bad}\n"));
        w.work(r#"cp "$T/fx/forks" "$TEHAS_HOME/state/cycles/7.forks""#);
        let r = w.run7();
        lacks(&w.gh(), "limit of processes");
        lacks(&w.gh(), "reboot");
        has(&r.out, "forks_refused=0");
    }
    let w = World::new("rc-linkforks");
    w.fx("elsewhere", "4\n");
    w.work(r#"ln -s "$T/fx/elsewhere" "$TEHAS_HOME/state/cycles/7.forks""#);
    w.run7();
    lacks(&w.gh(), "limit of processes");
    assert!(gone(&w.state().join("7.forks")));
}

#[test]
fn a_count_of_refused_starts_left_by_an_earlier_cycle_of_the_issue_is_not_this_cycles() {
    let w = World::new("rc-oldforks");
    fs::create_dir_all(w.state()).unwrap();
    fs::write(w.state().join("7.forks"), "6\n").unwrap();
    let r = w.run7();
    lacks(&w.gh(), "limit of processes");
    has(&r.out, "forks_refused=0");
}

#[test]
fn a_count_file_with_letters_counts_as_nothing() {
    for bad in [
        "three",
        "3 processes",
        "3; reboot",
        "`id`",
        "-3",
        "03",
        "12345678901234567890",
        "",
    ] {
        let w = World::new("rc-badoom");
        w.fx("oom", &format!("{bad}\n"));
        w.work(r#"echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; cp "$T/fx/oom" "$TEHAS_HOME/state/cycles/7.oom"; exit 2"#);
        let r = w.run7();
        lacks(&w.gh(), "killed for memory");
        lacks(&w.gh(), "reboot");
        has(&r.out, "oom_kills=0");
        assert_eq!(
            w.row(&["cycle-show", "7"])["cycle"]["oom_kills"],
            0,
            "{bad}"
        );
        assert!(gone(&w.state().join("7.oom")));
    }
}

#[test]
fn a_count_file_that_is_a_link_is_not_read() {
    let w = World::new("rc-linkoom");
    w.fx("elsewhere", "4\n");
    w.work(r#"ln -s "$T/fx/elsewhere" "$TEHAS_HOME/state/cycles/7.oom""#);
    w.run7();
    lacks(&w.gh(), "killed for memory");
    assert_eq!(
        fs::read_to_string(w.fx.join("elsewhere")).unwrap().trim(),
        "4"
    );
    assert!(gone(&w.state().join("7.oom")));
}

#[test]
fn the_appended_log_is_not_where_the_count_comes_from() {
    let w = World::new("rc-log");
    w.work(
        r#"echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"
echo "tehas work: 5 processes in this cycle's container were killed for memory"; exit 2"#,
    );
    w.run7();
    has(
        &fs::read_to_string(w.home.join("logs/dispatch-work-7.log")).unwrap(),
        "5 processes",
    );
    lacks(&w.gh(), "killed for memory");
    assert_eq!(w.row(&["cycle-show", "7"])["cycle"]["oom_kills"], 0);
}

#[test]
fn a_count_left_by_an_earlier_cycle_of_the_issue_is_not_this_cycles() {
    let w = World::new("rc-oldoom");
    fs::create_dir_all(w.state()).unwrap();
    fs::write(w.state().join("7.oom"), "6\n").unwrap();
    let r = w.run7();
    lacks(&w.gh(), "killed for memory");
    has(&r.out, "oom_kills=0");
}
