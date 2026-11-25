//! The records on the state store are read and written through the store
//! (libexec/store), not as files. These tests fail when code that is installed
//! or run names the old file of a record that moved. The files that stay
//! files, the carriers, are signals between two processes, a container's
//! mount or its output: each is named only by the code that passes it on.
//! Tests, the manual in host/docs/ and the records in docs/ may name both.
//!
//! What stays a file, and why (the plan's decision 4; the inventory of
//! docs/specs/2026-10-21-state-store.md sections 1.3 and 1.4):
//! - a cycle's .lock and a planning turn's .json.lock: liveness is the flock;
//! - work's .runner_exit .status .oom .forks .stalled .timed .nostart: how the
//!   container ended, from work to the cycle;
//! - .result.json and .progress.json: the container's output, kept to the next
//!   start for tehas export;
//! - the dispatcher's .done and stop's .stopped: a signal to a running cycle;
//! - .cid .manifest.json .images.md .retry-context.md: what a container is
//!   given;
//! - planning's .out and .rc, and the .keep directory beside the workspace;
//! - state/refined.jsonl, the ledger the store does not hold (D9);
//! - state/dispatch.*: the poll's own lock and notes.
//!
//! Every other record moved. A v0.30 install imported the old files, and this
//! release does not: the poll stops at 78 while one is left
//! (console/dispatch/tests/dispatch_guard.rs). The export copy an ended
//! session left, state/plan-<n>.turn, is neither imported nor guarded: nothing
//! reads it.
//!
//! The patterns are extended regular expressions, as grep -E read them in
//! test/store_paths_test.sh, matched one line at a time. Moved from that file.

mod common;

use common::{has, root};
use regex::Regex;
use std::fs;
use std::process::Command;

/// Every tracked file that is installed or run and could name a record.
fn code() -> Vec<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root())
        .args([
            "ls-files",
            "--",
            "bin",
            "libexec",
            "lib",
            "install.sh",
            "console/analytics/src",
            "console/src/export.rs",
            "console/ops/src/stop.rs",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "git ls-files");
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(String::from)
        .collect()
}

/// The lines of a file of the checkout that `re` finds, as `n:line`; none
/// for a file that is not there.
fn hits(file: &str, re: &Regex) -> Vec<String> {
    let Ok(bytes) = fs::read(root().join(file)) else {
        return vec![];
    };
    String::from_utf8_lossy(&bytes)
        .lines()
        .enumerate()
        .filter(|(_, l)| re.is_match(l))
        .map(|(i, l)| format!("{}:{l}", i + 1))
        .collect()
}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap_or_else(|e| panic!("{pattern}: {e}"))
}

/// The name of a cycle's file after its issue, as the shell spells it.
const N: &str = r"\$\{?(issue|n|1)(:\?)?\}?";
/// The analytics program still reads some of these files itself.
const AN: &str = "console/analytics/src";
/// A cycle's file as the code names it: after its issue, after <issue> in a
/// comment, or in a glob. Not after "plan-": those are the planning session's.
const F: &str = r"(^|[^-])(\$\{?(issue|n|1|owner)(:\?)?\}?|<issue>|<n>|\*)";
/// A planning session's file as the code names it: plan- and its issue, as a
/// variable, <issue>, <n> or a glob.
const P: &str = r"plan-(\$\{?(issue|n|1|plan_n)(:\?)?\}?|<issue>|<n>|\*)";

/// The old files of the records that moved, each with a line of code it is
/// there to find. Each group that moves onto the store adds its own.
fn forbidden() -> Vec<(String, &'static str)> {
    vec![
        // The cycle's records: its start, the approval it used, its image and
        // settings, its branch and its history.
        (format!(r"{F}\.taken\b"), r#": >"$cycles/$issue.taken""#),
        (
            format!(r"{F}\.by\b"),
            r#"printf x >"$TEHAS_HOME/state/cycles/$n.by""#,
        ),
        (
            format!(r"{F}\.requeued\b"),
            r#"rm -f "${cycles:?}/${issue:?}.requeued""#,
        ),
        (
            format!(r"{F}\.rukas\b"),
            r#"printf x >"$cycles/$issue.rukas""#,
        ),
        (
            format!(r"{F}\.image\b"),
            r"image_file=${cycles:?}/${issue:?}.image",
        ),
        (
            format!(r"{F}\.settings\b"),
            r"settings_file=$TEHAS_HOME/state/cycles/$issue.settings",
        ),
        (
            format!(r"{F}\.branch\b"),
            r#"read -r b <"${cycles:?}/${issue:?}.branch""#,
        ),
        (
            format!(r"{F}\.approved\.json\b"),
            r#"grep -l x "$TEHAS_HOME"/state/cycles/*.approved.json"#,
        ),
        (
            format!(r"{F}\.history\.jsonl\b"),
            r"# state/cycles/<issue>.history.jsonl, one line per end",
        ),
        (
            r"tehas_cycle_history_add\b".into(),
            r#"tehas_cycle_history_add "$issue" "$line""#,
        ),
        // The park, its question on record and the body the question was
        // posted from.
        (
            format!(r"{F}\.park\b"),
            r"pf=$TEHAS_HOME/state/cycles/$1.park",
        ),
        (
            "state/park-".into(),
            r#"printf x >"$TEHAS_HOME/state/park-$issue.json""#,
        ),
        (r"\bpark_id\b".into(), r#"qid=$(cat "$(park_id "$n")")"#),
        // The quiet-cycle episode: what the poll last asked of a stuck
        // cycle's unit.
        (
            format!(r"{F}\.quiet\b"),
            r"note=$TEHAS_HOME/state/cycles/$n.quiet",
        ),
        (
            r"tehas_cycle_note_files\b".into(),
            r"for f in $(tehas_cycle_note_files quiet); do",
        ),
        // The planning session: its record, read or written whole, its brief,
        // its answer, the copy of it a turn left, the directory it kept in
        // state/, its history and the helpers that read and wrote them. The
        // files a turn's unit passes on keep their names after the record's
        // (.out, .rc, .lock).
        (
            format!(r"{P}\.json($|[^.a-z])"),
            r#"for st in "$TEHAS_HOME"/state/plan-*.json; do"#,
        ),
        (
            r#""\$\{?st\}?(\.new)?""#.into(),
            r#"jq -c '.pending = false' "$st" >"$st.new""#,
        ),
        (
            r"\$\{?st\}?\.(brief|answer|turn|keep)\b|\.json\.(brief|answer|keep)\b".into(),
            r#"printf x >"$st.answer.new""#,
        ),
        (
            format!(r"{P}\.turn\b"),
            r#"copy_in "$TEHAS_HOME/state/plan-$issue.turn" "$out" plan.json"#,
        ),
        (
            format!(r"{P}\.history\.jsonl\b"),
            r#"files=("$TEHAS_HOME/state/cycles/plan-$issue.history.jsonl")"#,
        ),
        (
            r"\bplan_record_number\b".into(),
            r#"plan_record_number "$st" || continue"#,
        ),
        (
            r"\btehas_plan_history_add\b".into(),
            r#"tehas_plan_history_add "$n" "$line""#,
        ),
    ]
}

/// A file that stays a file: its pattern, a line it is there to find, the
/// files that may name it, and why it stays a file. No files: no code these
/// tests read names it. tehas-cycle reads it.
struct Carrier {
    re: String,
    sample: &'static str,
    files: Vec<String>,
    why: &'static str,
}

fn carrier(re: &str, sample: &'static str, files: &[&str], why: &'static str) -> Carrier {
    Carrier {
        re: re.into(),
        sample,
        files: files.iter().map(|f| f.to_string()).collect(),
        why,
    }
}

fn carriers() -> Vec<Carrier> {
    let an = |f: &str| format!("{AN}/{f}");
    vec![
        carrier(
            r"\.runner_exit\b",
            r"runner_exit_file=$TEHAS_HOME/state/cycles/$issue.runner_exit",
            &["console/ops/src/stop.rs"],
            "work tells the cycle how the runner ended",
        ),
        carrier(
            &format!(r"{N}\.status\b"),
            r"status_file=$TEHAS_HOME/state/cycles/$issue.status",
            &[],
            "work tells the cycle the runner's status word",
        ),
        carrier(
            r"\.oom\b",
            r"oom_file=${cycles:?}/${issue:?}.oom",
            &[&an("main.rs"), &an("run.rs")],
            "work counts the container's kills for memory for the cycle",
        ),
        carrier(
            r"\.forks\b",
            r"forks_file=$TEHAS_HOME/state/cycles/$issue.forks",
            &[&an("main.rs"), &an("run.rs")],
            "work counts the container's refused forks for the cycle",
        ),
        carrier(
            r"\.stalled\b",
            r"stall_file=${cycles:?}/${issue:?}.stalled",
            &[],
            "work tells the cycle the container stalled",
        ),
        carrier(
            r"\.timed\b",
            r"timed_file=${cycles:?}/${issue:?}.timed",
            &[],
            "work tells the cycle the container ran out of time",
        ),
        carrier(
            r"\.nostart\b",
            r"nostart_file=$TEHAS_HOME/state/cycles/$issue.nostart",
            &[],
            "work tells the cycle the runner never started",
        ),
        carrier(
            r"\.result\.json\b",
            r"result=$TEHAS_HOME/state/cycles/$issue.result.json",
            &[
                "console/src/export.rs",
                &an("run.rs"),
                &an("tests.rs"),
                "console/core/src/snapshot.rs",
            ],
            "the container's result, kept to the next start for tehas export",
        ),
        carrier(
            r"\.progress\.json\b",
            r"progress=$TEHAS_HOME/state/cycles/$issue.progress.json",
            &["console/src/export.rs", &an("run.rs")],
            "the summary of the container's output, kept to the next start for tehas export",
        ),
        carrier(
            &format!(r"{N}\.done\b"),
            r#"echo "$done_why" >"$TEHAS_HOME/state/cycles/$n.done""#,
            &[],
            "the dispatcher tells a running cycle why it is done",
        ),
        carrier(
            &format!(r"{N}\.stopped\b"),
            r"stopped_marker=$state/$n.stopped",
            &["console/ops/src/stop.rs"],
            "tehas stop tells a running cycle it was stopped",
        ),
        carrier(
            r"\.cid\b",
            r"cid_file=$TEHAS_HOME/state/cycles/$issue.cid",
            &["console/src/dev.rs"],
            "the container's id, for docker",
        ),
        carrier(
            r"\.manifest\.json\b",
            r"kept=$TEHAS_HOME/state/cycles/$issue.manifest.json",
            &["console/src/export.rs", "console/ops/src/stop.rs"],
            "the images a container is given, mounted into it",
        ),
        carrier(
            r"\.images\.md\b",
            r#"cp "$images/x" "$TEHAS_HOME/state/cycles/7.images.md""#,
            &[],
            "the list of images a container is given, mounted into it",
        ),
        carrier(
            r"\.retry-context\.md\b",
            r#"cp "$images/x" "$TEHAS_HOME/state/cycles/7.retry-context.md""#,
            &[],
            "what a reviewer said before a retry, mounted into the container",
        ),
        carrier(
            r"\$\{?st\}?\.rc\b|\.json\.rc\b",
            r#"rc=$(cat "$st.rc" 2>/dev/null)"#,
            &[],
            "a planning turn's exit, from the turn to the poll that collects it",
        ),
        carrier(
            r"\$\{?st\}?\.lock\b|\.json\.lock\b",
            r#"[ -f "$st.lock" ]"#,
            &[],
            "the lock a planning turn holds while it runs; nothing moves its .keep while it is held",
        ),
        carrier(
            r"\$\{?st\}?\.out\b|\.json\.out\b",
            r#"what=$(cat "$st.out")"#,
            &[&an("plan.rs")],
            "a planning container's output",
        ),
        carrier(
            r"refined\.jsonl\b",
            r"ledger=$TEHAS_HOME/state/refined.jsonl",
            &[&an("main.rs"), &an("report.rs"), &an("sessions.rs")],
            "the ledger of refined issues, which the store does not hold",
        ),
    ]
}

#[test]
fn no_code_names_the_old_file_of_a_record_on_the_store() {
    let forbidden: Vec<Regex> = forbidden().iter().map(|(p, _)| re(p)).collect();
    let mut found = String::new();
    for f in code() {
        for r in &forbidden {
            for hit in hits(&f, r) {
                found.push_str(&format!("{f}: {hit}\n"));
            }
        }
    }
    assert_eq!(found, "");
}

#[test]
fn a_carrier_is_named_only_by_the_code_that_passes_it_on() {
    let code = code();
    let mut bad = vec![];
    for c in carriers() {
        let r = re(&c.re);
        for f in &code {
            if !hits(f, &r).is_empty() && !c.files.contains(f) {
                bad.push(format!(
                    "{f} names {}, which only {} may: {}",
                    c.re,
                    c.files.join(" "),
                    c.why
                ));
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn each_carrier_is_still_named_where_the_list_says() {
    let mut bad = vec![];
    for c in carriers() {
        let r = re(&c.re);
        for f in &c.files {
            if hits(f, &r).is_empty() {
                bad.push(format!("{f} does not name {}: take it off the list", c.re));
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn each_pattern_finds_what_it_is_for() {
    let mut bad = vec![];
    let pairs = forbidden()
        .into_iter()
        .chain(carriers().into_iter().map(|c| (c.re, c.sample)));
    for (p, sample) in pairs {
        if !re(&p).is_match(sample) {
            bad.push(format!("the pattern {p} finds nothing in its sample"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn no_forbidden_pattern_takes_a_json_field_a_label_or_a_kept_name_for_a_file() {
    let mut bad = vec![];
    for line in [
        r#"jq -r .by <<<"$base""#,
        r#".branch // """#,
        "tehas.rukas",
        r#""$TEHAS_HOME/state/dispatch.taken""#,
        "$images/.taken",
        ".images.md",
        r#""$st.out""#,
        r#""$st.rc""#,
        r#""$st.lock""#,
        "state/plan-<issue>.json.out",
        r#".planning.answer // """#,
        ".planning.brief",
        "tehas_store_get '.planning.turn'",
        r#""$keep/answer.md""#,
        "q=$(tehas_store_get '.park.question // empty')",
        r#".park // "" | strings"#,
        r#"park_holds "$n""#,
        r#"park-show "$n""#,
        "<!-- tehas:park issue=7 -->",
        "systemd-run --user --quiet --collect",
        ".cycle.quiet",
    ] {
        for (p, _) in forbidden() {
            if re(&p).is_match(line) {
                bad.push(format!("the pattern {p} finds: {line}"));
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn no_carriers_pattern_takes_a_json_field_or_a_structs_for_a_file() {
    let mut bad = vec![];
    for line in [
        "jq -r .status",
        "$last.status",
        ".tick.status",
        "run.status = x",
        ".stopped | length",
        "jq .out",
        "given.rc.parse()",
        ".done // false",
    ] {
        for c in carriers() {
            if re(&c.re).is_match(line) {
                bad.push(format!("the pattern {} finds: {line}", c.re));
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn the_list_covers_the_code_that_is_installed_and_run() {
    let code = code().join("\n");
    has(&code, "libexec/setup");
    has(&code, "libexec/update");
    has(&code, "lib/config.sh");
    has(&code, "console/analytics/src/run.rs");
    has(&code, "install.sh");
}

// The console calls the store in its own process (console/ops/src/store.rs).
// The program stays installed for the shell, the cycle and the dispatcher, so
// the console names its path only where it asks whether it is there: ready
// and the absent refusal through store::program, and clean's upkeep check. The
// update installs the program and never runs it.
#[test]
fn the_console_names_the_store_program_only_to_see_that_it_is_installed() {
    let allowed = [
        r#"console/ops/src/store.rs:    home.join("libexec").join("store")"#,
        r#"console/ops/src/clean.rs:        let bin = self.home.join("libexec").join("store");"#,
    ];
    // The tests of console/ops/src/update/programs.rs make the programs a home
    // holds, the store among them, so the lines of its #[cfg(test)] module are
    // left out: the one module, from its attribute to the closing brace on the
    // file's last line. Its other lines are searched as every file is.
    let programs = "console/ops/src/update/programs.rs";
    let text = fs::read_to_string(root().join(programs)).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    let at: Vec<usize> = (0..lines.len())
        .filter(|&i| lines[i] == "#[cfg(test)]")
        .map(|i| i + 1)
        .collect();
    assert_eq!(at.len(), 1);
    let tests_at = at[0];
    assert_eq!(lines[tests_at], "mod tests {");
    let first_close = lines[tests_at - 1..]
        .iter()
        .position(|l| l.starts_with('}'))
        .map(|i| i + 1);
    assert_eq!(first_close, Some(lines.len() - tests_at + 1));

    let out = Command::new("git")
        .arg("-C")
        .arg(root())
        .args(["ls-files", "--", "console/src/*.rs", "console/ops/src/*.rs"])
        .output()
        .unwrap();
    assert!(out.status.success(), "git ls-files");
    let names = re(r#"libexec/store\b|"libexec"\)\.join\("store"\)"#);
    let mut named = vec![];
    for f in String::from_utf8(out.stdout).unwrap().lines() {
        for hit in hits(f, &names) {
            let (n, line) = hit.split_once(':').unwrap();
            if f == programs && n.parse::<usize>().unwrap() >= tests_at {
                continue;
            }
            named.push(format!("{f}:{line}"));
        }
    }
    for hit in &named {
        assert!(
            allowed.contains(&hit.as_str()),
            "the console names the store program: {hit}"
        );
    }
    let named = named.join("\n");
    for a in allowed {
        has(&named, a);
    }
}
