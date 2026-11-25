//! `tehas analytics`: what it asks of the analytics program, and what it
//! refuses. The program is a stub here; the Rust tests of the program
//! (console/analytics/tests/command.rs) run this command against the real one.

mod common;

use common::*;
use std::fs;
use std::path::Path;

/// The world of a case: the minimal instance, no analytics program, and gh,
/// docker, git and curl that fail, since nothing the command does may reach
/// them.
fn reset(w: &World) {
    w.reset();
    // The shell's reset removed the whole home, so World::reset's store home
    // (libexec/store, state/tehas.db, the account ids, a backup) is not there.
    let _ = fs::remove_dir_all(&w.home);
    for tool in outside_tools() {
        w.stub(tool, "exit 1");
    }
}

/// gh, docker, git and curl.
fn outside_tools() -> impl Iterator<Item = &'static str> {
    tehas_core::testing::UNREACHED.into_iter().chain(["curl"])
}

fn world(name: &str) -> World {
    let w = World::new(&format!("analytics-{name}"));
    reset(&w);
    w
}

/// `tehas analytics <args>`, a shell snippet's words, run in `$T`.
fn analytics(w: &World, args: &str) -> Run {
    w.sh(&format!(
        "cd \"$T\" && {} analytics {args}",
        root().join("bin/tehas").display()
    ))
}

/// What the calls of gh, docker, git and curl say: nothing when none ran.
fn outside(w: &World) -> String {
    outside_tools()
        .map(|tool| w.calls(tool).trim_end().to_string())
        .collect()
}

/// Every path under `$T/home`, sorted.
fn tree(w: &World) -> Vec<String> {
    fn walk(base: &Path, dir: &Path, into: &mut Vec<String>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            into.push(path.strip_prefix(base).unwrap().display().to_string());
            if path.is_dir() && !path.is_symlink() {
                walk(base, &path, into);
            }
        }
    }
    let base = w.t().join("home");
    let mut paths = vec![];
    walk(&base, &base, &mut paths);
    paths.sort();
    paths
}

fn last_line(text: &str) -> &str {
    text.trim_end().lines().last().unwrap_or("")
}

#[test]
fn the_program_runs_with_tehas_home_and_its_output_is_printed() {
    let w = world("home");
    w.stub_analytics(r#"echo "$TEHAS_HOME" >"$T/calls/analytics.home"; echo printed"#);
    let r = analytics(&w, "copy --to copy.db");
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(r.out, "printed");
    assert_eq!(w.calls("analytics").trim_end(), "copy --to copy.db");
    assert_eq!(
        w.calls("analytics.home").trim_end(),
        w.home.display().to_string()
    );
    assert_eq!(outside(&w), "");
}

#[test]
fn copy_passes_to_the_file_and_json_on_and_prints_the_programs_object() {
    let w = world("copy");
    w.stub_analytics(r#"echo "{\"api\":1,\"schema\":1,\"runs\":0}""#);
    let r = analytics(&w, "copy --to \"$T/copy.db\" --json");
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(r.out, r#"{"api":1,"schema":1,"runs":0}"#);
    assert_eq!(
        w.calls("analytics").trim_end(),
        format!("copy --to {}/copy.db --json", w.t().display())
    );
    let r = analytics(&w, "copy --to copy.db");
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(last_line(&w.calls("analytics")), "copy --to copy.db");
    let r = analytics(&w, "copy --json --to copy.db");
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(last_line(&w.calls("analytics")), "copy --json --to copy.db");
    assert_eq!(outside(&w), "");
}

#[test]
fn export_the_name_before_is_64_and_names_copy_to_before_the_settings_are_read() {
    let w = world("export");
    w.no_instance();
    w.stub_analytics("");
    for args in ["export copy.db", "export copy.db --json"] {
        let r = analytics(&w, args);
        assert_eq!(r.rc, 64, "{args}: {}", r.out);
        has(&r.out, "tehas analytics copy --to <file>");
    }
    assert_eq!(w.calls("analytics"), "");
}

#[test]
fn a_failure_of_the_program_is_exit_1_a_file_that_exists_a_database_that_cannot_be_read() {
    let w = world("failure");
    w.stub_analytics(r#"echo "analytics: copy.db exists already" >&2; exit 73"#);
    let r = analytics(&w, "copy --to copy.db --json");
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "exists already");
    w.stub_analytics(r#"echo "analytics: the database could not be read" >&2; exit 70"#);
    let r = analytics(&w, "copy --to copy.db");
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "could not be read");
}

#[test]
fn the_programs_0_64_78_and_141_are_the_commands() {
    let w = World::new("analytics-statuses");
    for rc in [0, 64, 78, 141] {
        reset(&w);
        w.stub_analytics(&format!("exit {rc}"));
        let r = analytics(&w, "copy --to copy.db");
        assert_eq!(r.rc, rc, "{}", r.out);
    }
}

#[test]
fn with_tehas_analytics_off_it_says_so_exits_78_and_asks_the_program_nothing() {
    let w = world("off");
    w.minimal(&["TEHAS_ANALYTICS=off"]);
    w.stub_analytics("");
    let before = tree(&w);
    for args in ["copy --to copy.db", "copy --to copy.db --json"] {
        let r = analytics(&w, args);
        assert_eq!(r.rc, 78, "{args}: {}", r.out);
        has(&r.out, "TEHAS_ANALYTICS=off");
    }
    assert_eq!(w.calls("analytics"), "");
    assert_eq!(tree(&w), before);
    assert!(!w.t().join("copy.db").exists());
    assert_eq!(outside(&w), "");
}

#[test]
fn a_host_without_the_program_is_exit_1_and_says_so() {
    let w = world("missing");
    let r = analytics(&w, "copy --to copy.db");
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "is not installed");
}

#[test]
fn missing_or_malformed_settings_are_exit_78_and_ask_the_program_nothing() {
    let w = world("settings");
    w.stub_analytics("");
    w.minimal(&["TEHAS_ANALYTICS=sometimes"]);
    let r = analytics(&w, "copy --to copy.db");
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "TEHAS_ANALYTICS");
    w.no_instance();
    let r = analytics(&w, "copy --to copy.db --json");
    assert_eq!(r.rc, 78, "{}", r.out);
    assert_eq!(w.calls("analytics"), "");
}

#[test]
fn a_bad_argument_is_exit_64_with_the_usage_before_the_settings_are_read() {
    let w = world("usage");
    w.instance(&[]);
    w.stub_analytics("");
    for args in [
        "",
        "report",
        "report --json",
        "copy",
        "copy a",
        "copy --to",
        "copy --to ''",
        "copy --to --json",
        "copy --to -o",
        "copy --to a --yaml",
        "copy --to a --json now",
        "prune",
        "run 7",
        "sample",
    ] {
        let r = analytics(&w, args);
        assert_eq!(r.rc, 64, "{args:?}: {}", r.out);
        has(&r.out, "usage: tehas analytics");
    }
    assert_eq!(w.calls("analytics"), "");
}

#[test]
fn it_writes_nothing_of_its_own() {
    let w = world("writes");
    w.stub_analytics("");
    let before = tree(&w);
    analytics(&w, "copy --to copy.db");
    assert_eq!(tree(&w), before);
}
