//! What stops a poll before it starts, and what runs around its decision:
//! the repository, the pause, invalid settings, the arguments, tidy, the
//! size hold's label change and the records of an older tehas. The rules
//! are in tests/fixtures/guard.json.

mod common;

use common::{count_lines, has, lacks, root, World, MINIMAL};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use tehas_core::testing::{copy_exe, write_exe};

fn tehasbot(w: &World) -> String {
    w.front()
        .join("bin/tehasbot")
        .to_string_lossy()
        .into_owned()
}

/// `tehasbot dispatch <words>` as a bash snippet, for a case that sends a
/// stream elsewhere.
fn dispatch_sh(w: &World, words: &str) -> common::Run {
    w.sh(&format!("\"{}\" dispatch {words}", tehasbot(w)))
}

fn wake(w: &World, words: &str) -> common::Run {
    w.sh(&format!(
        "\"{}\" wake {words}",
        w.front().join("bin/tehas").display()
    ))
}

fn set_origin(w: &World, url: &str) {
    let out = Command::new("git")
        .arg("-C")
        .arg(w.checkout())
        .args(["remote", "set-url", "origin", url])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
}

/// Neither state/ nor logs/ is in the home.
#[track_caller]
fn no_state_or_logs(w: &World) {
    assert!(!w.home.join("state").exists(), "state/ was made");
    assert!(!w.home.join("logs").exists(), "logs/ was made");
}

fn rm_state_and_logs(w: &World) {
    for d in ["state", "logs"] {
        let _ = fs::remove_dir_all(w.home.join(d));
    }
}

fn instance_with(w: &World, more: &[&str]) {
    let mut lines = MINIMAL.to_vec();
    lines.extend_from_slice(more);
    w.instance(&lines);
}

/// A poll program that says it ran.
fn rust_poll(w: &World) {
    w.stub("rust-poll", "#!/bin/sh\necho \"rust poll: $*\"\n");
    w.env_always(
        "TEHAS_DISPATCH_CMD",
        &w.t().join("bin/rust-poll").to_string_lossy(),
    );
}

/// bin/tehas and bin/tehasbot of this checkout in `tree`, with the console
/// in its libexec and lib/ beside them, as a host has them (t_front).
fn front_tree(w: &World, tree: &Path) {
    fs::create_dir_all(tree.join("bin")).unwrap();
    fs::create_dir_all(tree.join("libexec")).unwrap();
    std::os::unix::fs::symlink(root().join("lib"), tree.join("lib")).unwrap();
    for verb in ["tehas", "tehasbot"] {
        copy_exe(root().join("bin").join(verb), tree.join("bin").join(verb)).unwrap();
    }
    copy_exe(w.console(), tree.join("libexec/console")).unwrap();
}

fn tidy_calls(w: &World) -> usize {
    w.calls("fake-tidy").lines().count()
}

#[test]
fn a_checkout_of_another_repository_stops_everything() {
    let w = World::new("guard");
    set_origin(&w, &w.t().join("git/acme/other").to_string_lossy());
    let r = w.poll();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "is a checkout of");
    has(&r.out, "not of acme/widget");
    assert_eq!(w.gh(), "");
}

#[test]
fn origin_url_is_recognised_as_acme_widget() {
    let w = World::new("guard");
    for url in [
        "https://github.com/acme/widget.git",
        "https://github.com/acme/widget",
        "git@github.com:acme/widget.git",
        "ssh://git@github.com/acme/widget/",
    ] {
        w.reset();
        set_origin(&w, url);
        let r = w.dispatch(&["--dry-run"]);
        assert_eq!(r.rc, 0, "{url}: {}", r.out);
        has(&r.out, "would run a cycle for #7");
    }
}

#[test]
fn origin_ending_in_a_longer_name_is_not_acme_widget() {
    let w = World::new("guard");
    set_origin(&w, "https://github.com/notacme/widget.git");
    let r = w.dispatch(&["--dry-run"]);
    assert_eq!(r.rc, 78, "{}", r.out);
}

#[test]
fn the_poll_is_tehas_dispatch_with_its_arguments() {
    let w = World::new("guard");
    rust_poll(&w);
    let r = w.dispatch(&["--dry-run"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(r.out, "rust poll: poll --dry-run");
    assert_eq!(w.gh(), "");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(r.out, "rust poll: poll");
}

#[test]
fn tehas_dispatcher_rust_is_the_same_poll() {
    let w = World::new("guard");
    rust_poll(&w);
    instance_with(&w, &["TEHAS_DISPATCHER=rust"]);
    let r = w.dispatch(&["--dry-run"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(r.out, "rust poll: poll --dry-run");
}

#[test]
fn tehas_dispatcher_v_stops_the_poll_with_78_before_it_runs() {
    let w = World::new("guard");
    for v in ["bash", "perl"] {
        w.reset();
        rust_poll(&w);
        instance_with(&w, &[&format!("TEHAS_DISPATCHER={v}")]);
        let r = w.dispatch(&["--dry-run"]);
        assert_eq!(r.rc, 78, "{v}: {}", r.out);
        has(
            &r.out,
            "TEHAS_DISPATCHER is set and this version of tehas has no bash poll",
        );
        lacks(&r.out, "rust poll");
        assert_eq!(w.gh(), "");
    }
}

#[test]
fn a_poll_program_that_cannot_run_stops_the_poll_with_78_and_calls_nothing() {
    let w = World::new("guard");
    let bin = w.t().join("bin");
    let plain = bin.join("plain-file");
    fs::write(&plain, "").unwrap();
    fs::set_permissions(&plain, fs::Permissions::from_mode(0o644)).unwrap();
    for program in [bin.join("no-such-poll"), plain.clone(), bin.clone()] {
        w.env_always("TEHAS_DISPATCH_CMD", &program.to_string_lossy());
        let r = w.poll();
        assert_eq!(r.rc, 78, "{}", r.out);
        has(
            &r.out,
            &format!(
                "tehas: {} is not installed (run install.sh)",
                program.display()
            ),
        );
        assert_eq!(w.gh(), "");
    }
    fs::remove_file(&plain).unwrap();
    let not = bin.join("not-a-program");
    let try_bytes = |bytes: &[u8], want: &str| {
        write_exe(&not, bytes).unwrap();
        w.env_always("TEHAS_DISPATCH_CMD", &not.to_string_lossy());
        let r = w.poll();
        assert_eq!(r.rc, 78, "{bytes:?}: {}", r.out);
        has(&r.out, want);
        assert_eq!(w.gh(), "");
    };
    for bytes in [&b""[..], b"\n", b"not a program\n", b"\x01\x02\x03\x04\x05"] {
        try_bytes(bytes, "is not installed (run install.sh)");
    }
    let cannot = format!("tehas: {} cannot run (run install.sh)", not.display());
    for bytes in [&b"\x7fELF\x02\x01\x01"[..], b"#!/no/such/interpreter\n"] {
        try_bytes(bytes, &cannot);
    }
}

#[test]
fn with_no_tehas_dispatch_cmd_the_poll_is_the_program_beside_the_command() {
    let w = World::new("guard");
    let tree = w.t().join("wrapper");
    front_tree(&w, &tree);
    let run = |words: &str| {
        w.sh(&format!(
            "env -u TEHAS_DISPATCH_CMD -u TEHAS_HEALTH_CMD \"{}/bin/tehasbot\" dispatch {words}",
            tree.display()
        ))
    };
    let r = run("");
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        &format!(
            "tehas: {}/libexec/tehas-dispatch is not installed (run install.sh)",
            tree.display()
        ),
    );
    write_exe(
        tree.join("libexec/tehas-dispatch"),
        "#!/bin/sh\necho \"beside it: $*\"\n",
    )
    .unwrap();
    let r = run("--dry-run");
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(r.out, "beside it: poll --dry-run");
}

#[test]
fn a_missing_checkout_stops_before_any_label_changes_even_with_tehas_branch_set() {
    let w = World::new("guard");
    let nowhere = w.t().join("nowhere");
    instance_with(
        &w,
        &[
            "TEHAS_BRANCH=main",
            &format!("TEHAS_WORK_REPO={}", nowhere.display()),
        ],
    );
    let r = w.poll();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        &format!("no product checkout at {}", nowhere.display()),
    );
    assert_eq!(w.gh(), "");
    assert!(!nowhere.exists());
}

#[test]
fn a_renamed_repository_stops_the_poll_and_says_its_new_name() {
    let w = World::new("guard");
    w.fx("repo.json", r#"{"nameWithOwner":"acme/gadget"}"#);
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "ERROR: acme/widget is now acme/gadget on GitHub: set TEHAS_REPO",
    );
    assert_eq!(w.started(), "");
    lacks(&w.gh(), "issue list");
}

#[test]
fn a_failed_call_names_ghs_error_by_words_of_a_fixed_list() {
    let w = World::new("guard");
    w.fx(
        "gh-stderr",
        "HTTP 502: Bad Gateway for query with a sk-secret-looking body\n",
    );
    w.touch("gh-stderr-fails");
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "ERROR: gh api graphql failed: HTTP 502");
    lacks(&r.out, "sk-secret");
}

#[test]
fn an_error_with_no_word_of_the_list_keeps_the_words_the_poll_has_always_had() {
    let w = World::new("guard");
    w.fx("gh-stderr", "something an agent wrote\n");
    w.touch("gh-stderr-fails");
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "ERROR: gh api graphql failed");
    lacks(&r.out, "an agent wrote");
}

/// A question waiting on #8 and #9 blocked: what the rules after the pause
/// read of them, their comments and their blockers' states.
fn asking_and_blocked(w: &World) {
    w.fx(
        "asking.json",
        r#"[{"number":8,"labels":[{"name":"needs-human-attention"},{"name":"tehas:question"}]}]"#,
    );
    w.fx(
        "ready.json",
        r#"[{"number":7,"labels":[{"name":"tehas:ready"}]},{"number":9,"labels":[{"name":"tehas:blocked"}]}]"#,
    );
    w.fx(
        "events-8.json",
        r#"[{"event":"labeled","label":{"name":"tehas:question"},"actor":{"login":"alice"}}]"#,
    );
    w.fx(
        "events-9.json",
        r#"[{"event":"labeled","label":{"name":"tehas:blocked"},"actor":{"login":"alice"}}]"#,
    );
}

#[test]
fn a_paused_poll_reads_no_comments_or_blockers_of_the_issues_it_would_decide_on() {
    let w = World::new("guard");
    asking_and_blocked(&w);
    w.fx("control.json", r#"{"labels":[{"name":"tehas:pause"}]}"#);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    lacks(&w.gh(), "comments");
    lacks(&w.gh(), "issueOrPullRequest");
}

#[test]
fn the_same_poll_unpaused_reads_them() {
    let w = World::new("guard");
    asking_and_blocked(&w);
    w.poll();
    has(&w.gh(), "comments");
}

#[test]
fn the_pause_flag_stops_dispatch_and_deciding_it_asks_github_nothing() {
    let w = World::new("guard");
    w.pause_flag(r#"{"by":"boss","at":"2026-10-15T10:00:00Z","reason":"waiting for the gate"}"#);
    let r = w.dispatch(&["--dry-run"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "paused: the flag tehas pause wrote at 2026-10-15T10:00:00Z holds this host",
    );
    lacks(&r.out, "would run a cycle");
    assert_eq!(w.started(), "");
    // The flag decides on its own: the control issue is not asked about.
    lacks(&w.gh(), "issue view");
    lacks(&w.gh(), "issues/1/events");
}

#[test]
fn an_invalid_instance_makes_no_call_at_all_and_exits_78() {
    let w = World::new("guard");
    w.instance(&[
        "TEHAS_GITHUB_APP_ID=1",
        "TEHAS_CONTROL_ISSUE=1",
        "TEHAS_ALLOWED_LABELERS=alice",
    ]);
    rm_state_and_logs(&w);
    let r = w.poll();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "tehas: TEHAS_REPO is not set");
    assert_eq!(w.gh(), "");
    assert_eq!(w.started(), "");
    no_state_or_logs(&w);
}

#[test]
fn an_empty_labeller_list_does_not_widen_who_may_label() {
    let w = World::new("guard");
    instance_with(&w, &["TEHAS_ALLOWED_LABELERS=''"]);
    let r = w.dispatch(&["--dry-run"]);
    assert_eq!(r.rc, 78, "{}", r.out);
    assert_eq!(w.gh(), "");
}

// The poll's own guard. tehas-dispatch reads its settings from the
// environment, with no shell before it that loaded them, so it names what it
// needs: the settings an instance has to set itself, which `console settings
// required` answers, and those a load always leaves set.

/// The settings a run of tehas-dispatch with no load says are missing.
fn poll_needs(w: &World) -> Vec<String> {
    let r = w.sh(&format!(
        "TEHAS_HOME= \"{}\" poll --dry-run",
        w.dispatcher().display()
    ));
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "not set: load the instance's settings first (lib/config.sh, tehas_load)",
    );
    let needs = r.out.split(" not set:").next().unwrap();
    needs
        .trim_start_matches("tehas-dispatch: ")
        .split_whitespace()
        .map(|s| s.replace(',', ""))
        .collect()
}

#[test]
fn tehas_dispatch_names_every_setting_a_run_with_no_load_is_missing() {
    let w = World::new("guard");
    rm_state_and_logs(&w);
    let needs = poll_needs(&w);
    no_state_or_logs(&w);
    let r = w.sh(&format!("\"{}\" settings required", w.console().display()));
    let instance_needs: Vec<&str> = r.out.split_whitespace().collect();
    assert!(!instance_needs.is_empty(), "{}", r.out);
    for name in instance_needs {
        assert!(needs.iter().any(|n| n == name), "{name} not in {needs:?}");
    }
}

#[test]
fn tehas_dispatch_stops_a_run_with_no_missing_and_names_it() {
    let w = World::new("guard");
    let needs = poll_needs(&w);
    assert!(!needs.is_empty());
    for missing in needs {
        w.reset();
        let r = w.sh(&format!(
            ". \"{}/lib/config.sh\" && tehas_load && unset {missing} && \"{}\" poll --dry-run",
            root().display(),
            w.dispatcher().display()
        ));
        assert_eq!(r.rc, 78, "{missing}: {}", r.out);
        assert_eq!(
            r.out.split(" not set:").next().unwrap(),
            format!("tehas-dispatch: {missing}")
        );
        assert_eq!(w.gh(), "");
        assert_eq!(w.started(), "");
    }
}

#[test]
fn tehas_dispatch_stops_a_run_whose_tehas_issue_types_no_load_would_take() {
    let w = World::new("guard");
    let r = w.sh(&format!(
        ". \"{}/lib/config.sh\" && tehas_load && TEHAS_ISSUE_TYPES=bug=Bug \"{}\" poll --dry-run",
        root().display(),
        w.dispatcher().display()
    ));
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "tehas-dispatch: TEHAS_ISSUE_TYPES is not valid: bug is no kind",
    );
    assert_eq!(w.gh(), "");
    assert_eq!(w.started(), "");
}

// Arguments. A poll starts only from an argument this command knows, and the
// check runs before the instance settings are read.

#[test]
fn help_prints_the_usage_of_the_command_and_calls_nothing() {
    let w = World::new("guard");
    let r = dispatch_sh(&w, "--help 2>/dev/null");
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "Usage:");
    has(&r.out, "tehas dispatch --dry-run");
    lacks(&r.out, "set -uo pipefail");
    assert_eq!(w.gh(), "");
    assert_eq!(w.started(), "");
}

#[test]
fn h_is_the_same_as_help() {
    let w = World::new("guard");
    let r = dispatch_sh(&w, "-h 2>/dev/null");
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "Usage:");
    assert_eq!(w.gh(), "");
}

#[test]
fn help_writes_to_standard_output_and_nothing_to_standard_error() {
    let w = World::new("guard");
    let r = dispatch_sh(&w, "--help 1>/dev/null");
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(r.out, "");
}

#[test]
fn help_answers_on_a_host_with_no_instance_settings_yet() {
    let w = World::new("guard");
    w.instance(&[]);
    rm_state_and_logs(&w);
    let r = w.dispatch(&["--help"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "Usage:");
    lacks(&r.out, "TEHAS_REPO is not set");
    assert_eq!(w.gh(), "");
    no_state_or_logs(&w);
}

#[test]
fn a_mistyped_argument_starts_no_poll_and_creates_nothing() {
    let w = World::new("guard");
    rm_state_and_logs(&w);
    let r = w.dispatch(&["--dryrun"]);
    assert_eq!(r.rc, 64, "{}", r.out);
    has(&r.out, "tehas dispatch: unknown argument: --dryrun");
    has(&r.out, "Usage:");
    lacks(&r.out, "would run a cycle");
    assert_eq!(w.gh(), "");
    assert_eq!(w.started(), "");
    no_state_or_logs(&w);
}

#[test]
fn an_unknown_argument_is_reported_on_standard_error_never_as_a_poll() {
    let w = World::new("guard");
    let r = dispatch_sh(&w, "--dryrun 2>/dev/null");
    assert_eq!(r.rc, 64, "{}", r.out);
    lacks(&r.out, "unknown argument");
    let r = dispatch_sh(&w, "--dryrun 1>/dev/null");
    assert_eq!(r.rc, 64, "{}", r.out);
    has(&r.out, "tehas dispatch: unknown argument: --dryrun");
    has(&r.out, "Usage:");
}

#[test]
fn an_unknown_argument_is_refused_before_the_instance_settings_are_read() {
    let w = World::new("guard");
    rm_state_and_logs(&w);
    w.instance(&[
        "TEHAS_GITHUB_APP_ID=1",
        "TEHAS_CONTROL_ISSUE=1",
        "TEHAS_ALLOWED_LABELERS=alice",
    ]);
    let r = w.dispatch(&["--dryrun"]);
    assert_eq!(r.rc, 64, "{}", r.out);
    has(&r.out, "unknown argument: --dryrun");
    lacks(&r.out, "TEHAS_REPO is not set");
    assert_eq!(w.gh(), "");
}

#[test]
fn an_unknown_option_is_refused_too_whatever_it_looks_like() {
    let w = World::new("guard");
    for arg in ["-x", "--dry-run=1", "7", "--"] {
        let r = w.dispatch(&[arg]);
        assert_eq!(r.rc, 64, "{arg}: {}", r.out);
        has(&r.out, &format!("tehas dispatch: unknown argument: {arg}"));
    }
    assert_eq!(w.gh(), "");
}

#[test]
fn an_empty_argument_is_an_argument_not_a_poll() {
    let w = World::new("guard");
    let r = w.dispatch(&[""]);
    assert_eq!(r.rc, 64, "{}", r.out);
    has(&r.out, "tehas dispatch: unknown argument:");
    assert_eq!(w.gh(), "");
    assert_eq!(w.started(), "");
}

fn again(w: &World) -> PathBuf {
    w.home.join("state/dispatch.again")
}

#[test]
fn soon_writes_the_file_the_path_unit_starts_a_poll_on_and_polls_nothing_itself() {
    let w = World::new("guard");
    fs::create_dir_all(w.home.join("state")).unwrap();
    let _ = fs::remove_file(again(&w));
    let r = w.dispatch(&["--soon"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        &format!(
            "tehas dispatch: a poll is asked for ({})",
            again(&w).display()
        ),
    );
    assert!(again(&w).exists());
    assert_eq!(w.gh(), "");
    assert_eq!(w.started(), "");
    let r = w.dispatch(&["--soon"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert!(again(&w).exists());
}

#[test]
fn soon_with_no_state_directory_says_it_could_not_write_the_file() {
    let w = World::new("guard");
    let _ = fs::remove_dir_all(w.home.join("state"));
    let r = w.dispatch(&["--soon"]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        &format!("tehas dispatch: could not write {}", again(&w).display()),
    );
    assert_eq!(w.gh(), "");
}

#[test]
fn tehas_wake_asks_for_a_poll_as_soon_does_and_polls_nothing_itself() {
    let w = World::new("guard");
    fs::create_dir_all(w.home.join("state")).unwrap();
    let _ = fs::remove_file(again(&w));
    let r = wake(&w, "");
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        &format!("tehas wake: a poll is asked for ({})", again(&w).display()),
    );
    assert!(again(&w).exists());
    assert_eq!(w.gh(), "");
    assert_eq!(w.started(), "");
    let r = wake(&w, "--json");
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(r.out, r#"{"api":1,"asked":true}"#);
    let sample = fs::read_to_string(root().join("api/wake.sample.json")).unwrap();
    assert_eq!(common::jq(".", &sample), r.out);
}

#[test]
fn tehas_wake_is_taken_while_the_factory_is_paused_the_poll_it_asks_for_starts_nothing() {
    let w = World::new("guard");
    w.pause_flag(r#"{"at": "2026-10-17T14:00:00Z", "reason": "test"}"#);
    let r = wake(&w, "--json");
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(r.out, r#"{"api":1,"asked":true}"#);
    assert!(again(&w).exists());
}

#[test]
fn tehas_wake_with_no_state_directory_says_so_and_json_says_asked_false() {
    let w = World::new("guard");
    let _ = fs::remove_dir_all(w.home.join("state"));
    let r = wake(&w, "--json");
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, r#"{"api":1,"asked":false}"#);
    has(
        &r.out,
        &format!("tehas wake: could not write {}", again(&w).display()),
    );
}

#[test]
fn tehas_wake_refuses_an_argument_it_does_not_know_and_writes_nothing() {
    let w = World::new("guard");
    fs::create_dir_all(w.home.join("state")).unwrap();
    let _ = fs::remove_file(again(&w));
    let r = wake(&w, "--soon");
    assert_eq!(r.rc, 64, "{}", r.out);
    has(&r.out, "usage: tehas wake [--json]");
    assert!(!again(&w).exists());
}

#[test]
fn tehas_wake_stops_with_78_on_settings_it_cannot_load_and_writes_nothing() {
    let w = World::new("guard");
    fs::create_dir_all(w.home.join("state")).unwrap();
    let _ = fs::remove_file(again(&w));
    w.instance(&["TEHAS_REPO=acme/widget"]);
    let r = wake(&w, "");
    assert_eq!(r.rc, 78, "{}", r.out);
    assert!(!again(&w).exists());
}

#[test]
fn two_arguments_are_refused_not_combined() {
    let w = World::new("guard");
    rm_state_and_logs(&w);
    let r = w.dispatch(&["--dry-run", "--help"]);
    assert_eq!(r.rc, 64, "{}", r.out);
    has(&r.out, "tehas dispatch: unknown argument: --help");
    lacks(&r.out, "would run a cycle");
    assert_eq!(w.gh(), "");
    no_state_or_logs(&w);
}

#[test]
fn the_same_argument_twice_is_refused_too() {
    let w = World::new("guard");
    let r = w.dispatch(&["--dry-run", "--dry-run"]);
    assert_eq!(r.rc, 64, "{}", r.out);
    has(&r.out, "unknown argument: --dry-run");
    assert_eq!(w.gh(), "");
}

#[test]
fn help_with_another_argument_is_still_an_unknown_invocation() {
    let w = World::new("guard");
    let r = w.dispatch(&["--help", "--dry-run"]);
    assert_eq!(r.rc, 64, "{}", r.out);
    has(&r.out, "tehas dispatch: unknown argument: --dry-run");
    assert_eq!(w.gh(), "");
}

#[test]
fn no_argument_and_dry_run_still_behave_as_before() {
    let w = World::new("guard");
    let r = w.dispatch(&["--dry-run"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "would run a cycle for #7");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&w.calls("fake-start"), "7");
}

#[test]
fn all_hands_on_the_control_issue_starts_nothing_and_holds_nothing() {
    let w = World::new("guard");
    w.fx("control.json", r#"{"labels":[{"name":"tehas:all-hands"}]}"#);
    w.fx(
        "events-1.json",
        r#"[{"event":"labeled","label":{"name":"tehas:all-hands"},"actor":{"login":"alice"}}]"#,
    );
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.calls("fake-start"), "7\n");
}

#[test]
fn a_poll_runs_tidy_once_after_the_cycles_it_starts() {
    let w = World::new("guard");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.calls("fake-start"), "7\n");
    assert_eq!(tidy_calls(&w), 1);
    assert_eq!(w.calls("fake-tidy").trim_end(), "");
    assert!(w.t().join("calls/tidy.after-start").exists());
}

#[test]
fn a_dry_run_runs_tidy_with_dry_run() {
    let w = World::new("guard");
    let r = w.dispatch(&["--dry-run"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.calls("fake-tidy"), "--dry-run\n");
}

#[test]
fn a_paused_poll_still_tidies() {
    let w = World::new("guard");
    w.pause_flag(r#"{"by":"boss","at":"2026-10-15T10:00:00Z","reason":"waiting for the gate"}"#);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(tidy_calls(&w), 1);
}

#[test]
fn a_poll_that_fails_after_the_bots_login_still_tidies_and_keeps_its_exit_status() {
    let w = World::new("guard");
    w.rm_fx("control.json");
    w.poll();
    assert_eq!(tidy_calls(&w), 1);
}

#[test]
fn a_failed_tidy_does_not_change_the_polls_exit_status() {
    let w = World::new("guard");
    w.stub("fake-tidy", "exit 1");
    let r = w.dispatch(&["--dry-run"]);
    assert_eq!(r.rc, 0, "{}", r.out);
}

#[test]
fn a_poll_that_stops_before_the_bots_login_runs_no_tidy() {
    let w = World::new("guard");
    w.fx("repo.json", r#"{"nameWithOwner":"acme/gadget"}"#);
    let r = w.poll();
    assert_eq!(r.rc, 1, "{}", r.out);
    assert_eq!(w.calls("fake-tidy"), "");
}

/// A body the measure reads as `n` files: a path counts when it stands
/// between two ticks.
fn wide_body(n: u32) -> String {
    let mut out = String::from("Wide issue, one cycle.\n");
    for i in 1..=n {
        out.push_str(&format!("Change `lib/aa{i}.sh`.\n"));
    }
    out
}

/// A poll of one issue whose text is a body of `n` files, labelled
/// tehas:ready by alice.
fn wide_issue(w: &World, n: u32) {
    w.reset();
    w.fx_json(
        "work-7.json",
        &serde_json::json!({"number": 7, "state": "open", "title": "Wide",
            "body": wide_body(n), "labels": [{"name": "tehas:ready"}]}),
    );
}

#[test]
fn when_the_labels_cannot_be_changed_nothing_is_said_on_the_issue_and_the_poll_goes_on() {
    // The comment follows the label change: while the change fails the next
    // poll finds the same label and must not say the same once more.
    let w = World::new("guard");
    wide_issue(&w, 6);
    w.slots(1, &["TEHAS_SIZE_MAX_FILES=3"]);
    w.fx("edit-fails-on", "issue edit 7\n");
    w.fx("gh-stderr", "HTTP 403: refused\n");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "WARN: label change on #7 failed: --remove-label tehas:ready --add-label needs-human-attention");
    has(&r.out, "HTTP 403");
    lacks(&w.gh(), "issue comment 7");
    assert_eq!(w.started(), "");
}

// install.sh imports the records of an older tehas into the store. One still
// on disk is an import that failed or a cycle that ran old code across the
// update: the poll would read no record for it and act on that, so it stops
// before GitHub (D2).
const OLD_RECORDS: &[&str] = &[
    "state/cycles/5.approved.json",
    "state/cycles/5.park",
    "state/cycles/5.by",
    "state/cycles/5.requeued",
    "state/cycles/5.taken",
    "state/cycles/5.rukas",
    "state/cycles/5.quiet",
    "state/cycles/5.image",
    "state/cycles/5.settings",
    "state/cycles/5.branch",
    "state/cycles/5.history.jsonl",
    "state/park-5.id",
    "state/park-5.json",
    "state/plan-5.json",
    "state/plan-5.json.brief",
    "state/plan-5.json.answer",
    "state/plan-5.json.turn",
    "state/plan-5.json.keep/",
    "state/cycles/plan-5.history.jsonl",
    "state/plan-5.history.jsonl",
];

fn rm_park_files(w: &World) {
    for f in w.files(&w.home.join("state")) {
        if f.starts_with("park-") {
            let _ = fs::remove_file(w.home.join("state").join(f));
        }
    }
}

#[test]
fn an_old_record_file_stops_the_poll_and_a_dry_run_at_78_before_any_call_to_github() {
    let w = World::new("guard");
    for old in OLD_RECORDS {
        w.reset();
        rm_park_files(&w);
        fs::create_dir_all(w.cycles_dir()).unwrap();
        let path = w.home.join(old.trim_end_matches('/'));
        if old.ends_with('/') {
            fs::create_dir_all(&path).unwrap();
        } else {
            w.home_file(old, "{}\n");
        }
        let r = w.poll();
        assert_eq!(r.rc, 78, "{old}: {}", r.out);
        has(&r.out, &format!("ERROR: records of an older tehas at {}; this release does not import them: move them out of state/", path.display()));
        assert_eq!(w.gh(), "");
        assert_eq!(w.started(), "");
        let r = w.dispatch(&["--dry-run"]);
        assert_eq!(r.rc, 78, "{old}: {}", r.out);
        assert_eq!(w.gh(), "");
        if path.is_dir() {
            fs::remove_dir_all(&path).unwrap();
        } else {
            fs::remove_file(&path).unwrap();
        }
    }
}

#[test]
fn the_polls_own_files_the_carriers_and_the_imported_records_do_not_stop_it() {
    let w = World::new("guard");
    rm_park_files(&w);
    for f in [
        "dispatch.taken",
        "dispatch.stuck",
        "refined.jsonl",
        "plan-5.json.out",
        "plan-5.json.rc",
        "plan-5.json.lock",
        "cycles/5.lock",
        "cycles/5.done",
        "cycles/5.stopped",
        "cycles/5.runner_exit",
        "cycles/5.status",
        "cycles/5.oom",
        "cycles/5.forks",
        "cycles/5.stalled",
        "cycles/5.timed",
        "cycles/5.nostart",
        "cycles/5.result.json",
        "cycles/5.progress.json",
        "cycles/5.cid",
        "cycles/5.manifest.json",
        "cycles/5.images.md",
        "cycles/5.retry-context.md",
        "cycles/5.park.new",
        "cycles/5.approved.json.new",
        "plan-5.json.new",
        "cycles/shadow-5.json",
        "cycles/shadow-5.gh",
        "cycles/shadow-5.park",
        "cycles/shadow-5.end",
        "imported-20261024/cycles/5.taken",
        "imported-20261024/park-5.id",
        "imported-20261024/plan-5.json",
    ] {
        w.home_file(&format!("state/{f}"), "");
    }
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    lacks(&r.out, "older tehas");
    assert_eq!(w.calls("fake-start"), "7\n");
    let state = w.home.join("state");
    fs::remove_dir_all(state.join("imported-20261024")).unwrap();
    let _ = fs::remove_file(state.join("refined.jsonl"));
    for f in w.files(&state) {
        if f.starts_with("plan-5.") {
            let _ = fs::remove_file(state.join(f));
        }
    }
    for f in w.files(&w.cycles_dir()) {
        if (f.starts_with("5.") && f.ends_with(".new")) || f.starts_with("shadow-5.") {
            let _ = fs::remove_file(w.cycles_dir().join(f));
        }
    }
}

/// No TEHAS_TIDY_CMD and a console beside the poll, as on a host.
fn host_poll(w: &World) {
    w.reset();
    let tree = w.t().join("poll-tree");
    fs::create_dir_all(&tree).unwrap();
    copy_exe(w.dispatcher(), tree.join("tehas-dispatch")).unwrap();
    copy_exe(w.console(), tree.join("console")).unwrap();
    w.env_always(
        "TEHAS_DISPATCH_CMD",
        &tree.join("tehas-dispatch").to_string_lossy(),
    );
    w.fx(
        "closed.json",
        r#"[{"number":44,"state":"CLOSED","labels":[{"name":"tehas:awaiting-merge"},{"name":"tehas:shipped"}]}]"#,
    );
}

#[test]
fn the_poll_runs_the_consoles_tidy_on_a_host_and_a_dry_run_edits_nothing() {
    let w = World::new("guard");
    host_poll(&w);
    let r = w.sh(&format!(
        "env -u TEHAS_TIDY_CMD \"{}\" dispatch --dry-run",
        tehasbot(&w)
    ));
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "tidy #44: would remove tehas:awaiting-merge");
    lacks(&r.out, "No such file");
    lacks(&w.gh(), "issue edit");
    host_poll(&w);
    let r = w.sh(&format!(
        "env -u TEHAS_TIDY_CMD \"{}\" dispatch",
        tehasbot(&w)
    ));
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "tidy #44: removed tehas:awaiting-merge");
    lacks(&r.out, "No such file");
    has(&w.gh(), "issue edit 44");
}

#[test]
fn an_issue_over_the_ceiling_is_held_its_label_comes_off_needs_human_attention_goes_on_and_the_issue_says_the_counts_once(
) {
    let w = World::new("guard");
    wide_issue(&w, 6);
    w.slots(1, &["TEHAS_SIZE_MAX_FILES=3"]);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    has(&r.out, "skip #7: its body names 6 files, more than TEHAS_SIZE_MAX_FILES (3), and one cycle does not make that many changes");
    let gh = w.gh();
    has(&gh, "issue edit 7 --repo acme/widget --remove-label tehas:ready --add-label needs-human-attention");
    has(&gh, "issue comment 7 --repo acme/widget --body Tehas took `tehas:ready` off this issue, put `needs-human-attention` on it and started nothing. Its body names 6 files, more than the 3 of `TEHAS_SIZE_MAX_FILES`");
    has(&gh, "apply `tehas:ready` again to start one of them");
    // One label change and one comment: the comment of a start is not there.
    assert_eq!(count_lines(&gh, "issue edit 7 "), 1, "{gh}");
    assert_eq!(count_lines(&gh, "issue comment 7 "), 1, "{gh}");
}
