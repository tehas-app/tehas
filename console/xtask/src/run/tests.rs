//! The decisions of the runner over text, and the recorder of one task.
//! test/runner_test.sh runs the script over scratch checkouts with fake test
//! files, a stub lint and a stub cargo.

use super::*;
use tehas_core::testdir::TestDir;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|a| a.to_string()).collect()
}

fn touch(dir: &TestDir, rel: &str) {
    let p = dir.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, "").unwrap();
}

#[test]
fn paths_given_are_taken_under_the_root_and_a_missing_test_file_stops_the_run() {
    let d = TestDir::new("run-given");
    touch(&d, "test/a_test.sh");
    let root = d.path();
    assert!(!given(root, &[]).unwrap().any());
    assert!(given(root, &[]).unwrap().asked(&root.join("anything")));
    let g = given(
        root,
        &s(&["./test/a_test.sh", "lib/x.sh", "host/docs/x.md"]),
    )
    .unwrap();
    assert!(g.any());
    assert!(g.asked(&root.join("test/a_test.sh")));
    assert!(g.asked(&root.join("lib/x.sh")));
    assert!(!g.asked(&root.join("test/b_test.sh")));
    assert_eq!(
        given(root, &s(&["test/gone_test.sh"])).unwrap_err(),
        "test.sh: no test file test/gone_test.sh"
    );
    // Only a test file has to exist.
    assert!(given(root, &s(&["lib/gone.sh"])).is_ok());
}

#[test]
fn the_slowest_files_come_first_then_the_rest_in_name_order() {
    let d = TestDir::new("run-order");
    for f in [
        "test/b_test.sh",
        "test/a_test.sh",
        "test/work_test.sh",
        "test/work_x_test.sh",
        "test/dispatch_y_test.sh",
        "test/lib.sh",
        "test/.hidden_test.sh",
    ] {
        touch(&d, f);
    }
    fs::create_dir_all(d.join("test/dir_test.sh")).unwrap();
    let names: Vec<String> = test_files(d.path(), &Given::default())
        .iter()
        .map(|p| p.strip_prefix(d.path()).unwrap().display().to_string())
        .collect();
    // work_test.sh is not work_*_test.sh: it waits for name order.
    assert_eq!(
        names,
        [
            "test/work_x_test.sh",
            "test/dispatch_y_test.sh",
            "test/a_test.sh",
            "test/b_test.sh",
            "test/work_test.sh",
        ]
    );
    let g = given(d.path(), &s(&["test/b_test.sh"])).unwrap();
    assert_eq!(test_files(d.path(), &g), [d.join("test/b_test.sh")]);
}

#[test]
fn the_lint_reads_the_scripts_and_skips_the_vendored_copy() {
    let d = TestDir::new("run-lint");
    for f in [
        "bin/tehas",
        "libexec/work",
        "lib/config.sh",
        "lib/release.list",
        "scripts/verify.sh",
        "scripts/publish-lib.sh",
        "test/lib.sh",
        "test/a_test.sh",
        "install.sh",
        "host/shim/gh",
        "host/docs/x.md",
    ] {
        touch(&d, f);
    }
    let names: Vec<String> = lint_files(d.path(), &Given::default())
        .iter()
        .map(|p| p.strip_prefix(d.path()).unwrap().display().to_string())
        .collect();
    assert_eq!(
        names,
        [
            "bin/tehas",
            "libexec/work",
            "lib/config.sh",
            "scripts/verify.sh",
            "test/a_test.sh",
            "test/lib.sh",
            "install.sh",
            "host/shim/gh",
        ]
    );
    let g = given(d.path(), &s(&["lib/config.sh", "host/docs/x.md"])).unwrap();
    assert_eq!(lint_files(d.path(), &g), [d.join("lib/config.sh")]);
}

#[test]
fn the_pool_is_the_cpus_the_quota_and_the_memory_allow() {
    assert_eq!(cpus(16, None), 16);
    assert_eq!(cpus(16, Some("max 100000\n")), 16);
    // A quota of 2.5 CPUs is rounded up.
    assert_eq!(cpus(16, Some("250000 100000\n")), 3);
    assert_eq!(cpus(2, Some("800000 100000\n")), 2);
    assert_eq!(cpus(16, Some("0 100000\n")), 16);

    let gib = 1u128 << 30;
    assert_eq!(memory_jobs(8, Some(&format!("{}\n", 2 * gib))), 4);
    assert_eq!(memory_jobs(8, Some(&format!("{}\n", 96 * gib))), 8);
    assert_eq!(memory_jobs(8, Some("max\n")), 8);
    assert_eq!(memory_jobs(8, None), 8);
    assert_eq!(memory_jobs(8, Some(&format!("{}\n", 100 << 20))), 1);

    assert_eq!(jobs(Some("3"), || 8), 3);
    assert_eq!(jobs(None, || 8), 8);
    assert_eq!(jobs(Some(""), || 8), 8);
    assert_eq!(jobs(Some("lots"), || 8), 1);
    assert_eq!(jobs(Some("0"), || 8), 1);
    assert_eq!(jobs(None, || 0), 1);

    assert_eq!(limit(None), 300);
    assert_eq!(limit(Some("1")), 1);
    assert_eq!(limit(Some("soon")), 300);
}

#[test]
fn the_slowest_count_is_the_setting_or_five() {
    assert_eq!(slowest_count(None), 5);
    assert_eq!(slowest_count(Some("")), 5);
    assert_eq!(slowest_count(Some("0")), 5);
    assert_eq!(slowest_count(Some("soon")), 5);
    assert_eq!(slowest_count(Some("3")), 3);
    assert_eq!(slowest_count(Some("007")), 5);
}

#[test]
fn a_host_is_busy_once_its_load_reaches_its_cpus() {
    assert!(!busy(Some("0.00 0.00 0.00 1/100 1\n"), 8));
    assert!(!busy(Some("7.90 7.90 7.90 1/100 1\n"), 8));
    assert!(busy(Some("8.00 8.00 8.00 1/100 1\n"), 8));
    assert!(busy(Some("99999.00 0.00 0.00 1/100 1\n"), 8));
    assert!(!busy(None, 8));
    assert!(!busy(Some("high\n"), 8));
}

#[test]
fn the_cases_are_the_last_count_line_of_a_log() {
    assert_eq!(cases_in(b"a_test.sh: 3 cases, 0 failed\n"), Some(3));
    assert_eq!(
        cases_in(b"x: 1 cases, 0 failed\nran\nb_test.sh: 12 cases, 2 failed"),
        Some(12)
    );
    assert_eq!(cases_in(b"3 cases, 0 failed\n"), None);
    assert_eq!(cases_in(b"x: 3 cases, 0 failed later\n"), None);
    assert_eq!(cases_in(b"x: 3 cases,  failed\n"), None);
    assert_eq!(cases_in(b""), None);
    // t_done names the skipped cases between the two counts.
    assert_eq!(cases_in(b"x: 5 cases, 2 skipped, 0 failed\n"), Some(5));
    assert_eq!(cases_in(b"x: 5 cases, skipped, 0 failed\n"), None);
}

#[test]
fn the_first_fail_line_is_trimmed_and_an_empty_one_is_none() {
    assert_eq!(
        first_fail(b"ran\nFAIL: the first one\nFAIL: the second\n"),
        Some("the first one".into())
    );
    assert_eq!(first_fail(b"FAIL:   padded \t\n"), Some("padded".into()));
    assert_eq!(first_fail(b"NOFAIL: not the marker\n"), None);
    assert_eq!(first_fail(b"FAIL: \n"), None);
    assert_eq!(clean("a\tb\x1b[1m"), "a?b?[1m");
}

#[test]
fn the_last_line_of_a_build_log_is_its_reason() {
    assert_eq!(
        last_line(b"Compiling\nerror: could not compile store\n"),
        "error: could not compile store"
    );
    assert_eq!(last_line(b"one"), "one");
    assert_eq!(last_line(b""), "");
}

#[test]
fn the_slowest_five_are_named_slowest_first_ties_in_run_order() {
    let slow: Vec<(u128, String)> = [
        (100, "a"),
        (8400, "b"),
        (100, "c"),
        (61234, "d"),
        (5, "e"),
        (7, "f"),
    ]
    .iter()
    .map(|(ms, n)| (*ms, n.to_string()))
    .collect();
    assert_eq!(
        slowest(&slow, 5),
        [
            "     61.2 s  d",
            "      8.4 s  b",
            "      0.1 s  a",
            "      0.1 s  c",
            "      0.0 s  f",
        ]
    );
}

#[test]
fn asked_for_seven_over_eight_entries_names_seven_slowest_first() {
    let slow: Vec<(u128, String)> = [
        (100, "a"),
        (8400, "b"),
        (100, "c"),
        (61234, "d"),
        (5, "e"),
        (7, "f"),
        (2500, "g"),
        (0, "h"),
    ]
    .iter()
    .map(|(ms, n)| (*ms, n.to_string()))
    .collect();
    assert_eq!(
        slowest(&slow, 7),
        [
            "     61.2 s  d",
            "      8.4 s  b",
            "      2.5 s  g",
            "      0.1 s  a",
            "      0.1 s  c",
            "      0.0 s  f",
            "      0.0 s  e",
        ]
    );
}

#[test]
fn a_count_too_big_for_usize_names_every_file() {
    let slow: Vec<(u128, String)> = [(100, "a"), (8400, "b"), (5, "c")]
        .iter()
        .map(|(ms, n)| (*ms, n.to_string()))
        .collect();
    // 2^64 is a positive whole number, and narrows to 0 in a usize.
    for count in ["18446744073709551616", "36893488147419103232", "1000"] {
        let asked = positive(count).expect("a positive whole number");
        assert_eq!(
            slowest(&slow, asked),
            ["      8.4 s  b", "      0.1 s  a", "      0.0 s  c"],
            "{count} names every file"
        );
    }
}

#[test]
fn a_record_is_judged_by_its_status_its_log_and_the_timeout() {
    let rc = |v: &str| Some(Some(v.to_string()));
    assert_eq!(judge(true, true, rc("0"), 300), None);
    assert_eq!(judge(true, true, rc("1"), 300), Some(String::new()));
    assert_eq!(
        judge(true, true, rc("124"), 300),
        Some("timed out after 300s".into())
    );
    assert_eq!(
        judge(true, true, rc("137"), 1),
        Some("timed out after 1s".into())
    );
    assert_eq!(
        judge(true, true, None, 300),
        Some("it did not finish, no result recorded".into())
    );
    assert_eq!(
        judge(true, true, Some(None), 300),
        Some("its recorded result could not be read".into())
    );
    assert_eq!(
        judge(true, true, rc("not a number"), 300),
        Some("its recorded result could not be read".into())
    );
    // A passing status does not make up for output the run cannot show.
    assert_eq!(
        judge(false, false, rc("0"), 300),
        Some("its output could not be read".into())
    );
    // Nor for a log with no count line from t_done: the file stopped early.
    assert_eq!(
        judge(true, false, rc("0"), 300),
        Some("ended before t_done".into())
    );
    // A failing status is named as before; the count line is not the reason.
    assert_eq!(judge(true, false, rc("1"), 300), Some(String::new()));
}

#[test]
fn a_missing_shellcheck_fails_in_ci_and_when_asked() {
    assert!(!strict(None, None));
    assert!(!strict(Some(""), Some("")));
    assert!(strict(Some("true"), None));
    assert!(strict(None, Some("1")));
}

fn bad(name: &str, why: &str, first: Option<&str>) -> Bad {
    Bad {
        name: name.into(),
        why: why.into(),
        first: first.map(String::from),
    }
}

#[test]
fn the_block_names_what_failed_and_what_did_not_run() {
    assert!(block(false, &[], &[]).is_empty());
    assert_eq!(
        block(
            true,
            &[
                bad("test/a_test.sh", "", Some("a case\tthat failed")),
                bad("test/b\x01_test.sh", "timed out after 1s", None),
                bad("test/c_test.sh", "", None),
            ],
            &s(&["shellcheck is not installed"]),
        ),
        [
            "",
            "test.sh: failed checks (4), not run (1)",
            "    shellcheck",
            "FAILED: test/a_test.sh: a case?that failed",
            "    test/b?_test.sh: timed out after 1s",
            "    test/c_test.sh",
            "SKIPPED: shellcheck is not installed",
        ]
    );
    let long = "x".repeat(300);
    let lines = block(false, &[bad("test/a_test.sh", "", Some(&long))], &[]);
    assert_eq!(
        lines[2],
        format!("FAILED: test/a_test.sh: {}", "x".repeat(200))
    );
}

#[test]
fn the_block_fits_the_lines_verify_keeps_and_counts_the_rest() {
    let many: Vec<Bad> = (1..=60)
        .map(|i| bad(&format!("test/f{i:02}_test.sh"), "", None))
        .collect();
    let lines = block(true, &many, &s(&["one", "two"]));
    // Two spare lines for anything printed after it.
    assert!(lines.len() - 1 <= KEEP - 2, "{} lines", lines.len());
    assert_eq!(lines[1], "test.sh: failed checks (61), not run (2)");
    assert_eq!(lines[2], "    shellcheck");
    // The last ones that fit are named, the first ones counted.
    assert_eq!(lines[3], "    test/f29_test.sh");
    assert!(lines.contains(&"    test/f60_test.sh".to_string()));
    assert!(lines.contains(&"    and 28 more, in the full log".to_string()));
    assert_eq!(lines.last().unwrap(), "SKIPPED: two");
}

#[test]
fn a_task_records_its_output_status_and_time_when_it_ends() {
    let d = TestDir::new("run-task");
    let key = d.join("test/a_test.sh");
    fs::create_dir_all(key.parent().unwrap()).unwrap();
    let args = |cmd: &str| -> Vec<OsString> {
        [
            key.as_os_str(),
            "300".as_ref(),
            "bash".as_ref(),
            "-c".as_ref(),
            cmd.as_ref(),
        ]
        .iter()
        .map(|a| a.to_os_string())
        .collect()
    };
    assert_eq!(task(&args("echo out; echo err >&2; exit 3")), 0);
    assert_eq!(
        fs::read_to_string(with_suffix(&key, ".log")).unwrap(),
        "out\nerr\n"
    );
    assert_eq!(fs::read_to_string(with_suffix(&key, ".rc")).unwrap(), "3\n");
    let ms = fs::read_to_string(with_suffix(&key, ".ms")).unwrap();
    assert!(digits(ms.trim_end()).is_some(), "{ms}");

    let short: Vec<OsString> = [
        key.as_os_str(),
        "1".as_ref(),
        "sleep".as_ref(),
        "30".as_ref(),
    ]
    .iter()
    .map(|a| a.to_os_string())
    .collect();
    assert_eq!(task(&short), 0);
    assert_eq!(
        fs::read_to_string(with_suffix(&key, ".rc")).unwrap(),
        "124\n"
    );

    assert_eq!(task(&[]), 64);
}

#[test]
fn a_dump_the_kernel_keeps_in_the_working_directory_is_the_name_the_run_asks_for() {
    // The kernel's own default, and the name a host that leaves the pattern
    // alone has in its file: a plain name is a file of the working directory.
    assert_eq!(core_name(Some("core")).as_deref(), Some("core"));
    assert_eq!(core_name(Some("tehas.core")).as_deref(), Some("tehas.core"));
    // An unset or empty pattern is the same default.
    assert_eq!(core_name(None).as_deref(), Some("core"));
    assert_eq!(core_name(Some("")).as_deref(), Some("core"));
    // A program the kernel pipes the dump to, a path it writes under, and a
    // template it names after the crash are none a run finds in the checkout,
    // so such a host is asked for nothing.
    assert_eq!(core_name(Some("|/usr/lib/systemd/coredump %p")), None);
    assert_eq!(core_name(Some("/var/crash/core")), None);
    assert_eq!(core_name(Some("core.%p")), None);
}

#[test]
fn the_core_files_of_a_directory_are_named_in_order_and_nothing_else_is() {
    let d = TestDir::new("run-core-left");
    fs::write(d.join("core"), "a dump").unwrap();
    fs::write(d.join("core.4242"), "another dump").unwrap();
    fs::write(d.join("core.4243"), "a third").unwrap();
    // Neither a file of another name nor a hidden one nor a directory of the
    // right name is a core file of the run.
    fs::write(d.join("notes.md"), "text").unwrap();
    fs::write(d.join(".core"), "hidden").unwrap();
    fs::write(d.join("coredump.txt"), "not a dump").unwrap();
    fs::create_dir_all(d.join("core.9999")).unwrap();
    assert_eq!(left("core", d.path()), ["core", "core.4242", "core.4243"]);
    // The name a host with another pattern uses is asked for in that spelling.
    fs::write(d.join("tehas.core"), "a dump").unwrap();
    assert_eq!(left("tehas.core", d.path()), ["tehas.core"]);
    // A directory that cannot be read names nothing: the run reports what it
    // could not see, not a crash it did not have.
    assert_eq!(left("core", &d.join("nowhere")), Vec::<String>::new());
}

#[test]
fn the_core_files_are_kept_only_when_the_setting_names_them() {
    // Unset and empty are the run of a host that wants no core file; any other
    // value is the case that shows the check works on a host that leaves them.
    assert!(!cores_kept(None));
    assert!(!cores_kept(Some("")));
    assert!(cores_kept(Some("1")));
    assert!(cores_kept(Some("0")));
}
