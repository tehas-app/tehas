//! `tehas export` end to end: what a person reading a cycle is given, masked,
//! and the cursor a program keeps. Which files are copied, and under what
//! names, is the question here; what the mask matches is the console's own
//! tests in console/src/export.rs. Run as a host runs it: bin/tehas, this
//! build's console, the real store. Export asks GitHub for nothing: gh is a
//! stub that answers nothing, and git one that records the read of the email.
//! Moved from test/export_test.sh.

mod common;

use common::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tehas_core::time::iso;

const T0: i64 = 1760700000;
/// A token of the shape the mask knows, and what the mask makes of it.
const TOKEN: &str = "ghp_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const MASKED: &str = "ghp_***";

fn host(name: &str) -> World {
    let w = World::new(&format!("export-{name}"));
    w.gh_stub("");
    w.stub("git", "");
    w
}

/// The directory the cases export into.
fn out(w: &World) -> PathBuf {
    w.t().join("archive")
}

fn logs(w: &World) -> PathBuf {
    w.home.join("logs")
}

fn log(w: &World, name: &str) -> PathBuf {
    logs(w).join(name)
}

/// A file of the export of one cycle.
fn got(w: &World, cursor: &str, rel: &str) -> PathBuf {
    out(w).join("cycles").join(cursor).join(rel)
}

/// `<lo>-<n>`: a cursor just before T0, for issue `n`.
fn before(n: u64) -> String {
    format!("{}-{n}", T0 - 1)
}

/// A cycle that ended, with the files a cycle leaves behind that an export
/// is meant to carry.
fn cycle(w: &World, n: u64, e: i64, fields: &str) {
    w.history_end(n, e, fields);
    fs::create_dir_all(logs(w)).unwrap();
    fs::write(
        w.at().cycle_note(n, "result.json"),
        "{\"issue\":12,\"result\":\"merged\"}\n",
    )
    .unwrap();
    fs::write(
        w.at().cycle_note(n, "progress.json"),
        "{\"step\":\"review\"}\n",
    )
    .unwrap();
    let file = |name: &str, text: String| {
        fs::write(log(w, &name.replace("N", &n.to_string())), text).unwrap()
    };
    file("work-N-1760.jsonl", format!("plain text {TOKEN} here\n"));
    file("work-N-1760.jsonl.err", "an error line\n".into());
    file(
        "work-N-1760.jsonl.console",
        format!("the console said {TOKEN} here\n"),
    );
    file("dispatch-work-N.log", "dispatched\n".into());
    file("work-N-1760.jsonl.preflight", "a preflight notice\n".into());
}

const MERGED: &str = r#"{"kind":"work","rc":0,"status":"merged"}"#;
const PLANNED: &str = r#"{"kind":"plan","rc":0,"status":"finished","turn":1}"#;

fn export(w: &World, words: &[&str]) -> Run {
    w.tehas("export", words)
}

/// `tehas export --json --after <cursor> <out>`.
fn export_json(w: &World, cursor: &str) -> Run {
    let to = out(w);
    export(w, &["--json", "--after", cursor, &to.to_string_lossy()])
}

/// bin/tehas export as a shell snippet.
fn export_cmd() -> String {
    format!("{} export", root().join("bin/tehas").display())
}

/// The files under `dir`, relative to it, in byte order; directories too
/// when `dirs`.
fn walk(dir: &Path, dirs: bool) -> Vec<String> {
    fn go(base: &Path, d: &Path, dirs: bool, acc: &mut Vec<String>) {
        for e in fs::read_dir(d).unwrap() {
            let p = e.unwrap().path();
            let rel = p.strip_prefix(base).unwrap().to_string_lossy().into_owned();
            if p.is_dir() && !p.is_symlink() {
                if dirs {
                    acc.push(rel);
                }
                go(base, &p, dirs, acc);
            } else {
                acc.push(rel);
            }
        }
    }
    let mut acc = vec![];
    if dir.is_dir() {
        go(dir, dir, dirs, &mut acc);
    }
    acc.sort();
    acc
}

/// The files an export wrote for one cycle, relative to its directory.
fn listing(w: &World, cursor: &str) -> String {
    let dir = out(w).join("cycles").join(cursor);
    if !dir.is_dir() {
        return "missing".into();
    }
    walk(&dir, false)
        .into_iter()
        .filter(|f| f != ".stamp")
        .collect::<Vec<_>>()
        .join("\n")
}

fn files_out(w: &World) -> usize {
    walk(&out(w), false).len()
}

fn read(p: &Path) -> String {
    fs::read_to_string(p).unwrap_or_default()
}

fn mode(p: &Path) -> u32 {
    fs::metadata(p).unwrap().permissions().mode() & 0o777
}

/// jq -r of a filter that gives one string, or its JSON otherwise.
fn raw(filter: &str, input: &str) -> String {
    let s = jq(filter, input);
    serde_json::from_str::<String>(&s).unwrap_or(s)
}

/// The control bytes the mask makes nothing of, NUL first when `nul`.
fn controls(nul: bool) -> Vec<u8> {
    let mut b: Vec<u8> = (1..=8).chain([11, 12]).chain(14..=31).collect();
    if nul {
        b.insert(0, 0);
    }
    b
}

#[test]
fn one_cycle_its_directory_its_files_and_the_cursor_a_program_keeps() {
    let w = host("one");
    cycle(&w, 12, T0, MERGED);
    let r = export_json(&w, &before(12));
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        jq("del(.runs)", &r.out),
        format!(
            r#"{{"api":1,"version":1,"next":"1760700000-12-1","to":"{}"}}"#,
            out(&w).display()
        )
    );
    let manifest = out(&w).join("export.json");
    assert_eq!(jq(".", &read(&manifest)), r#"{"version":1}"#);
    assert_eq!(mode(&manifest), 0o644);
    assert_eq!(
        listing(&w, "1760700000-12-1"),
        "history.json
logs/dispatch-work.log
logs/progress.json
logs/result.json
logs/work-12-1760.jsonl
logs/work-12-1760.jsonl.console
logs/work-12-1760.jsonl.err
logs/work-12-1760.jsonl.preflight"
    );
}

#[test]
fn a_log_is_masked_whatever_bytes_it_holds_and_one_stray_nul_does_not_escape_the_mask() {
    let w = host("bytes");
    cycle(&w, 12, T0, MERGED);
    let c = "1760700000-12-1";
    fs::write(
        log(&w, "work-12-1760.jsonl"),
        format!("a line with {TOKEN} in it\n"),
    )
    .unwrap();
    let mut err = controls(true);
    err.extend(format!(" {TOKEN}\n").bytes());
    fs::write(log(&w, "work-12-1760.jsonl.err"), err).unwrap();
    fs::write(
        log(&w, "work-12-1760.jsonl.console"),
        format!("a log with one stray \0 in it and {TOKEN}\n"),
    )
    .unwrap();
    let r = export_json(&w, &before(12));
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        read(&got(&w, c, "logs/work-12-1760.jsonl")),
        format!("a line with {MASKED} in it\n")
    );
    let mut want = controls(true);
    want.extend(format!(" {MASKED}\n").bytes());
    assert_eq!(
        fs::read(got(&w, c, "logs/work-12-1760.jsonl.err")).unwrap(),
        want
    );
    let console: Vec<u8> = fs::read(got(&w, c, "logs/work-12-1760.jsonl.console"))
        .unwrap()
        .into_iter()
        .filter(|b| *b != 0)
        .collect();
    assert_eq!(
        String::from_utf8(console).unwrap(),
        format!("a log with one stray  in it and {MASKED}\n")
    );
    // Bytes that are not UTF-8 in a log are no way past the mask.
    let mut cut = b"cut short \xff\xfe and ".to_vec();
    cut.extend(format!("{TOKEN}\n").bytes());
    fs::write(log(&w, "work-12-1760.jsonl"), cut).unwrap();
    export_json(&w, &before(12));
    let text = String::from_utf8_lossy(&fs::read(got(&w, c, "logs/work-12-1760.jsonl")).unwrap())
        .into_owned();
    has(&text, &format!("and {MASKED}"));
    lacks(&text, TOKEN);
}

#[test]
fn bytes_the_mask_makes_nothing_of_are_copied_as_they_came_and_an_empty_file_stays_empty() {
    let w = host("asis");
    cycle(&w, 12, T0, MERGED);
    fs::write(log(&w, "work-12-1760.jsonl"), controls(false)).unwrap();
    fs::write(log(&w, "work-12-1760.jsonl.err"), "").unwrap();
    let r = export_json(&w, &before(12));
    assert_eq!(r.rc, 0, "{}", r.out);
    let c = "1760700000-12-1";
    assert_eq!(
        fs::read(got(&w, c, "logs/work-12-1760.jsonl")).unwrap(),
        controls(false)
    );
    assert_eq!(
        fs::metadata(got(&w, c, "logs/work-12-1760.jsonl.err"))
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn the_cycle_s_own_line_of_history_is_what_tehas_history_says() {
    let w = host("line");
    cycle(
        &w,
        12,
        T0,
        r#"{"kind":"work","rc":10,"status":"handoff","step":"review","parked":true,"pr":"https://github.com/acme/widget/pull/9"}"#,
    );
    export_json(&w, &before(12));
    assert_eq!(
        jq(".", &read(&got(&w, "1760700000-12-1", "history.json"))),
        format!(
            r#"{{"cursor":"1760700000-12-1","kind":"work","issue":12,"cycle":"12-1","ended":"{}","started":"{}","status":10,"outcome":"handoff","end":"handoff","progress":{{"step":"review"}},"provider":false,"parked":true,"pr":"https://github.com/acme/widget/pull/9","change":9,"how":"hand"}}"#,
            iso(T0),
            iso(T0 - 60)
        )
    );
}

#[test]
fn a_planning_turn_brings_its_session_as_the_store_has_it_and_no_cycle_files() {
    let w = host("plan");
    w.plan_end(
        7,
        T0,
        r#"{"kind":"plan","rc":0,"status":"finished","turn":1,"by":"alice"}"#,
    );
    let r = export_json(&w, &before(7));
    assert_eq!(r.rc, 0, "{}", r.out);
    let c = "1760700000-7-1";
    assert_eq!(listing(&w, c), "history.json\nplan.json");
    assert_eq!(raw(".kind", &read(&got(&w, c, "history.json"))), "plan");
    let plan = read(&got(&w, c, "plan.json"));
    assert_eq!(
        jq(
            "[.issue, .session, .turns, .running, .by, .label, (.ended_at | type)]",
            &plan
        ),
        format!(
            r#"[7,"abc-123",1,false,"alice","{}","null"]"#,
            label_of("Act::Plan")
        )
    );
    assert_eq!(
        raw(".brief", &plan).trim_end_matches('\n'),
        "# Faster search"
    );
}

// A turn that made no session ends it, and the session stays on record until
// the next plan label on the issue: the archive of that turn holds it.
#[test]
fn a_turn_that_left_no_session_brings_the_session_it_ended() {
    let w = host("nosession");
    w.plan_end(
        7,
        T0,
        r#"{"kind":"plan","status":"not-started","turn":1,"by":"alice"}"#,
    );
    let r = export_json(&w, &before(7));
    assert_eq!(r.rc, 0, "{}", r.out);
    let c = "1760700000-7-1";
    assert_eq!(listing(&w, c), "history.json\nplan.json");
    assert_eq!(
        jq(
            "[.session, .turns, .running, (.ended_at | type)]",
            &read(&got(&w, c, "plan.json"))
        ),
        r#"[null,1,false,"number"]"#
    );
}

fn brief_again(w: &World) -> String {
    let p = w.t().join("brief-again.md");
    fs::write(&p, "# Again\n").unwrap();
    p.to_string_lossy().into_owned()
}

#[test]
fn a_turn_whose_session_the_store_no_longer_has_has_no_plan_json_and_is_exported_all_the_same() {
    let w = host("forgotten");
    w.plan_end(7, T0, PLANNED);
    // The history of an issue outlives its sessions: a later plan label
    // replaced this one, and the store forgets the replaced one.
    let brief = brief_again(&w);
    let plan = label_of("Act::Plan");
    w.store_ok(&[
        "plan-begin",
        "7",
        "--by",
        "alice",
        "--label",
        &plan,
        "--brief-file",
        &brief,
    ]);
    w.store_ok(&["plan-begin", "7", "--undo"]);
    let r = export_json(&w, &before(7));
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(listing(&w, "1760700000-7-1"), "history.json");
}

#[test]
fn a_turn_whose_session_a_later_plan_replaced_has_no_plan_json_the_new_session_is_not_its() {
    let w = host("replaced");
    w.plan_end(
        7,
        T0,
        r#"{"kind":"plan","rc":0,"status":"finished","turn":1,"by":"alice"}"#,
    );
    let brief = brief_again(&w);
    let refine = label_of("Act::Refine");
    let r = w.store_at(
        T0 + 600,
        &[
            "plan-begin",
            "7",
            "--by",
            "bob",
            "--label",
            &refine,
            "--brief-file",
            &brief,
        ],
    );
    assert_eq!(r.rc, 0, "{}", r.out);
    let r = export_json(&w, &before(7));
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(listing(&w, "1760700000-7-1"), "history.json");
}

#[test]
fn a_planning_turn_whose_session_the_store_cannot_give_is_not_written_and_the_rest_are() {
    let w = host("plan-busy");
    cycle(&w, 12, T0, MERGED);
    w.plan_end(7, T0 + 60, PLANNED);
    w.store_answers("plan-show", 1, "store: busy #7: the database is locked", 0);
    let r = export_json(&w, &before(1));
    assert_eq!(r.rc, 1, "{}", r.out);
    assert_eq!(w.store_answered(), 1);
    assert_eq!(
        listing(&w, "1760700000-12-1").lines().next(),
        Some("history.json")
    );
    assert!(!got(&w, "1760700060-7-2", "plan.json").exists());
}

fn transcript(w: &World) {
    let dir = w.t().join("home/.pi/agent/ensemble-runs/issue-12");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("abc.jsonl"), "{\"type\":\"assistant\"}\n").unwrap();
}

#[test]
fn the_transcripts_of_the_cycle_are_carried() {
    let w = host("transcripts");
    cycle(&w, 12, T0, MERGED);
    transcript(&w);
    export_json(&w, &before(12));
    let listed = listing(&w, "1760700000-12-1");
    let t: Vec<&str> = listed
        .lines()
        .filter(|l| l.starts_with("transcripts/"))
        .collect();
    assert_eq!(t, ["transcripts/abc.jsonl"]);
}

#[test]
fn a_cycle_with_no_logs_and_no_transcripts_is_exported_all_the_same() {
    let w = host("bare");
    w.history_end(12, T0, r#"{"rc":0,"status":"merged"}"#);
    let r = export_json(&w, &before(12));
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(listing(&w, "1760700000-12-1"), "history.json");
}

#[test]
fn nothing_to_copy_exit_0_the_object_and_no_directory_of_a_cycle() {
    let w = host("nothing");
    let r = export_json(&w, &before(1));
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        jq(".", &r.out),
        format!(
            r#"{{"api":1,"version":1,"runs":[],"next":"{}-1","to":"{}"}}"#,
            T0 - 1,
            out(&w).display()
        )
    );
    assert_eq!(
        jq(".", &read(&out(&w).join("export.json"))),
        r#"{"version":1}"#
    );
}

#[test]
fn without_json_one_line_for_each_cycle_and_one_for_the_directory() {
    let w = host("text");
    cycle(&w, 12, T0, MERGED);
    w.plan_end(7, T0 + 60, PLANNED);
    let to = out(&w);
    let r = export(&w, &["--after", &before(1), &to.to_string_lossy()]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        r.out,
        format!(
            "1760700000-12-1  work  #12
1760700060-7-2  plan  #7
exported  2 cycles to {} (next: 1760700060-7-2)",
            to.display()
        )
    );
}

#[test]
fn a_reader_that_goes_away_is_exit_141_with_nothing_on_standard_error_and_the_copy_is_made() {
    let w = host("reader-gone");
    cycle(&w, 12, T0, MERGED);
    for flag in ["--json", ""] {
        let _ = fs::remove_dir_all(out(&w));
        let r = w.sh(&format!(
            r#"{{ sleep 0.3; {} {flag} --after {} '{}' 2>"$T/err"; }} | true; exit ${{PIPESTATUS[0]}}"#,
            export_cmd(),
            before(12),
            out(&w).display()
        ));
        assert_eq!(r.rc, 141, "{flag}: {}", r.out);
        assert_eq!(read(&w.t().join("err")), "");
        assert_eq!(
            jq(".", &read(&out(&w).join("export.json"))),
            r#"{"version":1}"#
        );
    }
}

#[test]
fn an_old_copy_of_a_cycle_is_replaced_whole() {
    let w = host("replace");
    cycle(&w, 12, T0, MERGED);
    let line = || read(&got(&w, "1760700000-12-1", "history.json"));
    export_json(&w, &before(12));
    assert_eq!(jq(".status", &line()), "0");
    // The same cycle, as it is on record now.
    w.store_db();
    cycle(&w, 12, T0, r#"{"kind":"work","rc":10,"status":"handoff"}"#);
    export_json(&w, &before(12));
    assert_eq!(jq(".status", &line()), "10");
}

#[test]
fn a_file_over_32_mib_is_left_out_so_an_export_is_for_reading() {
    let w = host("big");
    cycle(&w, 12, T0, MERGED);
    let f = fs::File::create(log(&w, "work-12-1760.jsonl")).unwrap();
    f.set_len(33554433).unwrap();
    export_json(&w, &before(12));
    let listed = listing(&w, "1760700000-12-1");
    assert_eq!(
        listed
            .lines()
            .filter(|l| l.ends_with("work-12-1760.jsonl"))
            .count(),
        0
    );
    has(&listed, "logs/result.json");
}

#[test]
fn a_file_that_cannot_be_read_fails_its_cycle_and_the_rest_of_it_and_the_other_cycles_are_copied() {
    let w = host("unreadable");
    cycle(&w, 12, T0, MERGED);
    cycle(&w, 13, T0 + 60, MERGED);
    let err = log(&w, "work-12-1760.jsonl.err");
    fs::set_permissions(&err, fs::Permissions::from_mode(0o000)).unwrap();
    let r = export_json(&w, &before(1));
    fs::set_permissions(&err, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(r.rc, 1, "{}", r.out);
    let said: Vec<&str> = r
        .out
        .lines()
        .filter(|l| l.contains("could not write"))
        .collect();
    assert_eq!(said, ["tehas export: could not write 1760700000-12-1"]);
    let listed = listing(&w, "1760700000-12-1");
    assert_eq!(
        listed.lines().filter(|l| l.ends_with("jsonl.err")).count(),
        0
    );
    has(&listed, "logs/work-12-1760.jsonl.console");
    has(
        &listing(&w, "1760700060-13-2"),
        "logs/work-13-1760.jsonl.err",
    );
}

#[test]
fn a_link_is_not_followed_and_only_this_issue_s_logs_are_taken() {
    let w = host("links");
    cycle(&w, 12, T0, MERGED);
    let outside = w.t().join("outside.jsonl");
    fs::write(&outside, "secret elsewhere\n").unwrap();
    std::os::unix::fs::symlink(&outside, log(&w, "work-12-9999.jsonl")).unwrap();
    fs::write(log(&w, "work-13-1760.jsonl"), "another issue's\n").unwrap();
    export_json(&w, &before(12));
    let listed = listing(&w, "1760700000-12-1");
    assert_eq!(listed.lines().filter(|l| l.contains("9999")).count(), 0);
    assert_eq!(listed.lines().filter(|l| l.contains("13")).count(), 0);
    lacks(
        &read(&got(&w, "1760700000-12-1", "logs/work-12-1760.jsonl")),
        "secret elsewhere",
    );
}

#[test]
fn the_directory_is_made_when_it_is_not_there_and_is_readable_whatever_the_umask() {
    let w = host("umask");
    cycle(&w, 12, T0, MERGED);
    let deeper = out(&w).join("nested/deeper");
    let r = w.sh(&format!(
        "umask 077; {} --json --after {} '{}'",
        export_cmd(),
        before(12),
        deeper.display()
    ));
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(raw(".to", &r.out), deeper.to_string_lossy());
    // The directory export makes, and every level below it: the top of the
    // path a caller names belongs to the caller and keeps the caller's own
    // mode.
    let c = deeper.join("cycles/1760700000-12-1");
    for d in [
        deeper.clone(),
        deeper.join("cycles"),
        c.clone(),
        c.join("logs"),
    ] {
        assert_eq!(mode(&d), 0o755, "{}", d.display());
    }
    assert_eq!(mode(&c.join("history.json")), 0o644);
    assert_eq!(mode(&c.join("logs/result.json")), 0o644);
}

#[test]
fn an_argument_it_does_not_know_is_exit_64_and_copies_nothing() {
    let w = host("args");
    cycle(&w, 12, T0, MERGED);
    let (c, o, h) = (
        before(12),
        out(&w).display().to_string(),
        w.home.display().to_string(),
    );
    let cases = [
        String::new(),
        format!("--after {c}"),
        format!("--after {c} {o} extra"),
        format!("--after {c} --after {c} {o}"),
        format!("--after 12 {o}"),
        format!("--after {}-0 {o}", T0 - 1),
        format!("--after x {o}"),
        o.clone(),
        format!("--json {o}"),
        format!("--after {c} ."),
        format!("--after {c} .."),
        format!("--after {c} {o}/."),
        format!("--after {c} {h}"),
        format!("--after {c} {h}/backup"),
    ];
    for args in &cases {
        let words: Vec<&str> = args.split_whitespace().collect();
        let r = export(&w, &words);
        assert_eq!(r.rc, 64, "{args}: {}", r.out);
    }
    assert_eq!(files_out(&w), 0);
}

#[test]
fn without_settings_it_is_exit_78_and_copies_nothing() {
    let w = host("nosettings");
    cycle(&w, 12, T0, MERGED);
    w.no_instance();
    assert_eq!(export_json(&w, &before(12)).rc, 78);
    assert_eq!(files_out(&w), 0);
}

#[test]
fn without_the_state_store_it_is_exit_78_and_copies_nothing() {
    let w = host("nostore");
    cycle(&w, 12, T0, MERGED);
    fs::remove_file(w.home.join("libexec/store")).unwrap();
    let r = export_json(&w, &before(12));
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "tehas export: ERROR: the state store is not installed (run install.sh)",
    );
    assert_eq!(files_out(&w), 0);
}

#[test]
fn a_busy_store_is_exit_1_and_copies_nothing() {
    let w = host("busy");
    cycle(&w, 12, T0, MERGED);
    w.store_answers("history-show", 1, "store: busy: the database is locked", 0);
    assert_eq!(export_json(&w, &before(12)).rc, 1);
    assert_eq!(files_out(&w), 0);
}

#[test]
fn no_console_means_nothing_is_exported_because_nothing_could_be_masked() {
    let w = host("noconsole");
    cycle(&w, 12, T0, MERGED);
    let r = w.sh(&format!(
        r#"TEHAS_CONSOLE="$T/no-console" {} --json --after {} '{}'"#,
        export_cmd(),
        before(12),
        out(&w).display()
    ));
    assert_eq!(r.rc, 78, "{}", r.out);
    assert_eq!(files_out(&w), 0);
}

#[test]
fn export_changes_nothing_in_the_factory() {
    let w = host("readonly");
    cycle(&w, 12, T0, MERGED);
    w.plan_end(7, T0 + 60, PLANNED);
    transcript(&w);
    // The store's own files of its log (-wal, -shm) are SQLite's, not a write.
    let files = || -> Vec<String> {
        walk(&w.home, true)
            .into_iter()
            .filter(|f| !f.ends_with("tehas.db-wal") && !f.ends_with("tehas.db-shm"))
            .collect()
    };
    let pi = w.t().join("home/.pi");
    let before_home = files();
    let before_runs = walk(&pi, true);
    let r = export_json(&w, &before(1));
    assert_eq!(jq(".runs | length", &r.out), "2", "{}", r.out);
    assert_eq!(files(), before_home);
    assert_eq!(walk(&pi, true), before_runs);
}

#[test]
fn reading_takes_no_lock_a_running_cycle_holds() {
    let w = host("lock");
    cycle(&w, 12, T0, MERGED);
    let lock = w.at().cycle_note(12, "lock");
    fs::write(&lock, "").unwrap();
    let r = w.sh(&format!(
        "flock '{}' {} --json --after {} '{}'",
        lock.display(),
        export_cmd(),
        before(12),
        out(&w).display()
    ));
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(jq(".runs | length", &r.out), "1");
}

fn models(w: &World, text: &str) {
    let dir = w.t().join("home/.pi/agent");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("models.json"), text).unwrap();
}

#[test]
fn every_file_is_masked_with_the_host_s_names_aliases_each_role_s_endpoints_the_email() {
    let w = host("names");
    w.minimal(&[
        "PI_ENSEMBLE_HOST_ALIASES=gpu-one:192.0.2.5,solo-host:192.0.2.6",
        "PI_ENSEMBLE_SUBAGENT_PROVIDER=alpha",
        "PI_ENSEMBLE_SUBAGENT_MODEL=m1",
        "PI_ENSEMBLE_PROVIDER_OPS=beta",
        "PI_ENSEMBLE_MODEL_OPS=m2",
    ]);
    models(
        &w,
        r#"{"providers":{"alpha":{"baseUrl":"https://model-main.lan:8086/v1","backupBaseUrl":"http://model-backup.lan/v1"},"beta":{"baseUrl":"http://role-only.lan/v1"},"unused":{"baseUrl":"http://not-used.lan/v1"}}}
"#,
    );
    w.stub("git", "echo bot@mail.example");
    cycle(&w, 12, T0, MERGED);
    let file = log(&w, "work-12-1760.jsonl");
    fs::write(
        &file,
        "gpu-one SOLO-HOST model-main.lan Model-Backup.lan role-only.lan bot@mail.example\n",
    )
    .unwrap();
    let r = export_json(&w, &before(12));
    assert_eq!(r.rc, 0, "{}", r.out);
    let copy = got(&w, "1760700000-12-1", "logs/work-12-1760.jsonl");
    assert_eq!(
        read(&copy),
        format!("{}\n", ["[private name]"; 6].join(" "))
    );
    has(&w.calls("git"), "config --global user.email");
    fs::write(&file, "http://not-used.lan/v1\n").unwrap();
    export_json(&w, &before(12));
    has(&read(&copy), "not-used.lan");
}

#[test]
fn a_file_of_models_that_is_there_and_cannot_be_read_is_exit_78_and_makes_nothing() {
    let w = host("badmodels");
    cycle(&w, 12, T0, MERGED);
    models(
        &w,
        "{\"providers\":{\"alpha\":{\"baseUrl\":\"http://model-main.lan/v1\"}}}\ntrailing\n",
    );
    let r = export_json(&w, &before(12));
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "models.json");
    assert!(!out(&w).exists());
}

#[test]
fn a_host_with_no_file_of_models_still_exports() {
    let w = host("nomodels");
    cycle(&w, 12, T0, MERGED);
    let _ = fs::remove_file(w.t().join("home/.pi/agent/models.json"));
    assert_eq!(export_json(&w, &before(12)).rc, 0);
}

#[test]
fn a_repository_that_is_not_valid_is_exit_78_and_makes_nothing() {
    let w = host("badrepo");
    cycle(&w, 12, T0, MERGED);
    w.minimal(&["TEHAS_REPO=not-a-repo"]);
    let r = export_json(&w, &before(12));
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "TEHAS_REPO is not valid");
    assert!(!out(&w).exists());
}

#[test]
fn a_link_into_the_home_is_refused_and_makes_no_directory_there() {
    let w = host("tohome");
    cycle(&w, 12, T0, MERGED);
    let link = w.t().join("to-home");
    std::os::unix::fs::symlink(&w.home, &link).unwrap();
    let c = before(12);
    let r = export(&w, &["--after", &c, &link.to_string_lossy()]);
    assert_eq!(r.rc, 64, "{}", r.out);
    let r = export(
        &w,
        &["--after", &c, &link.join("backup/deeper").to_string_lossy()],
    );
    assert_eq!(r.rc, 64, "{}", r.out);
    has(
        &r.out,
        &format!("is in the factory's home ({})", w.home.display()),
    );
    assert!(!w.home.join("backup").exists());
    assert!(!w.home.join("cycles").exists());
}

#[test]
fn a_directory_named_in_bytes_that_are_not_text_is_a_bad_argument_and_a_wide_cursor_one_too() {
    let w = host("nottext");
    cycle(&w, 12, T0, MERGED);
    let r = w.sh(&format!(
        r"{} --after 1-1 $'{}\xff'",
        export_cmd(),
        out(&w).display()
    ));
    assert_eq!(r.rc, 64, "{}", r.out);
    has(&r.out, "usage: tehas export");
    let to = out(&w);
    let r = export(&w, &["--after", "12345678901-1", &to.to_string_lossy()]);
    assert_eq!(r.rc, 64, "{}", r.out);
    has(&r.out, "usage: tehas export");
    assert!(!to.exists());
}
