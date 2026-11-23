//! `xtask config-check --settings <dir>`: an instance's settings held against
//! this version's load, on instance directories made here from
//! instances/example.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tehas_core::settings::load::{ignored_list, ignored_why};
use tehas_core::testdir::TestDir;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A scratch directory of this test's own, empty, removed when it drops.
fn scratch(name: &str) -> TestDir {
    TestDir::new(&format!("config-check-{name}"))
}

/// instances/example copied into a scratch directory, with `lines` added to
/// its instance.env.
fn instance(name: &str, lines: &[String]) -> TestDir {
    let dir = scratch(name);
    for entry in fs::read_dir(repo().join("instances/example")).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), dir.join(entry.file_name())).unwrap();
    }
    let mut text = fs::read_to_string(dir.join("instance.env")).unwrap();
    for line in lines {
        text.push_str(line);
        text.push('\n');
    }
    fs::write(dir.join("instance.env"), text).unwrap();
    dir
}

struct Run {
    rc: i32,
    out: String,
    err: String,
}

/// The task with `args`, from a home whose credential file holds a token and a
/// renamed setting, either of which stops a load that reads it: a check that
/// read this machine's home would fail.
fn check(args: &[&str]) -> Run {
    let home = scratch("home");
    fs::create_dir_all(home.join(".config/tehas")).unwrap();
    fs::write(
        home.join(".config/tehas/env"),
        "GH_TOKEN=x\nTEHAS_EASY_MERGE=1\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("config-check")
        .args(args)
        .env("HOME", home.path())
        .env_remove("TEHAS_INSTANCE_DIR")
        .output()
        .unwrap();
    Run {
        rc: out.status.code().unwrap_or(-1),
        out: String::from_utf8_lossy(&out.stdout).into_owned(),
        err: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn lines(r: &Run) -> Vec<&str> {
    r.out.lines().collect()
}

#[test]
fn without_settings_it_is_78_and_reads_nothing() {
    let r = check(&[]);
    assert_eq!(r.rc, 78, "{}{}", r.out, r.err);
    assert_eq!(r.out, "");
    assert!(r
        .err
        .contains("usage: xtask config-check --settings <instance dir>"));
}

#[test]
fn a_settings_path_that_is_not_there_is_78_and_reads_nothing() {
    let missing = scratch("missing").join("no-such-instance");
    let r = check(&["--settings", &missing.to_string_lossy()]);
    assert_eq!(r.rc, 78, "{}{}", r.out, r.err);
    assert_eq!(r.out, "");
    assert!(r.err.contains("no instance directory at"), "{}", r.err);
}

#[test]
fn the_example_instance_passes() {
    let r = check(&[
        "--settings",
        &repo().join("instances/example").to_string_lossy(),
    ]);
    assert_eq!(r.rc, 0, "{}{}", r.out, r.err);
    assert_eq!(lines(&r), ["pass"]);
}

#[test]
fn every_ignored_setting_is_dropped_with_the_loads_reason_and_the_instance_passes() {
    let names: Vec<&str> = ignored_list().collect();
    assert!(!names.is_empty());
    let set: Vec<String> = names.iter().map(|n| format!("{n}=1")).collect();
    let dir = instance("ignored", &set);
    let r = check(&["--settings", &dir.to_string_lossy()]);
    assert_eq!(r.rc, 0, "{}{}", r.out, r.err);
    let mut want: Vec<String> = names
        .iter()
        .map(|n| format!("dropped {n}: {}", ignored_why(n).unwrap()))
        .collect();
    want.push("pass".into());
    assert_eq!(lines(&r), want);
}

#[test]
fn a_retired_and_a_renamed_setting_each_fail_and_hide_what_the_load_does_after() {
    let dir = instance(
        "refused",
        &[
            "TEHAS_ENERGY=on".into(),
            "TEHAS_FIXER_LOGIN=bob".into(),
            "TEHAS_EASY_MERGE=1".into(),
        ],
    );
    let r = check(&["--settings", &dir.to_string_lossy()]);
    assert_eq!(r.rc, 78, "{}{}", r.out, r.err);
    let got = lines(&r);
    assert_eq!(got.len(), 3, "{}", r.out);
    for (line, name) in got.iter().zip(["TEHAS_FIXER_LOGIN", "TEHAS_EASY_MERGE"]) {
        assert!(
            line.starts_with(&format!("fail {name}: {name} is no longer read: ")),
            "{line}\n{}",
            r.out
        );
    }
    // The load stops before it warns of TEHAS_ENERGY: run again once fixed.
    assert_eq!(got[2], "fail");
}

#[test]
fn a_required_setting_with_a_bad_value_fails() {
    let dir = instance("bad-required", &["TEHAS_CONTROL_ISSUE=abc".into()]);
    let r = check(&["--settings", &dir.to_string_lossy()]);
    assert_eq!(r.rc, 78, "{}{}", r.out, r.err);
    let got = lines(&r);
    assert_eq!(got.len(), 2, "{}", r.out);
    assert!(
        got[0].starts_with(
            "fail TEHAS_CONTROL_ISSUE: TEHAS_CONTROL_ISSUE is not valid: expected a positive whole number (see "
        ),
        "{}",
        r.out
    );
    assert_eq!(got[1], "fail");
}

#[test]
fn a_required_setting_that_is_not_set_fails() {
    let dir = instance("no-repo", &[]);
    let file = dir.join("instance.env");
    let text = fs::read_to_string(&file).unwrap();
    let kept: String = text
        .lines()
        .filter(|l| !l.starts_with("TEHAS_REPO="))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_ne!(kept, text);
    fs::write(&file, kept).unwrap();
    let r = check(&["--settings", &dir.to_string_lossy()]);
    assert_eq!(r.rc, 78, "{}{}", r.out, r.err);
    let got = lines(&r);
    assert!(
        got[0].starts_with("fail TEHAS_REPO: TEHAS_REPO is not set (see "),
        "{}",
        r.out
    );
    assert!(!got[0].contains("manager"), "{}", r.out);
    assert_eq!(got[1..], ["fail"]);
}

#[test]
fn a_required_model_setting_that_is_not_set_fails_and_names_the_manager_that_renders_it() {
    let dir = instance("no-model", &[]);
    let file = dir.join("instance.env");
    let text = fs::read_to_string(&file).unwrap();
    let kept: String = text
        .lines()
        .filter(|l| !l.starts_with("PI_ENSEMBLE_SUBAGENT_MODEL="))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_ne!(kept, text);
    fs::write(&file, kept).unwrap();
    let r = check(&["--settings", &dir.to_string_lossy()]);
    assert_eq!(r.rc, 78, "{}{}", r.out, r.err);
    let got = lines(&r);
    assert_eq!(got.len(), 2, "{}", r.out);
    assert!(
        got[0].starts_with(
            "fail PI_ENSEMBLE_SUBAGENT_MODEL: PI_ENSEMBLE_SUBAGENT_MODEL is not set (see "
        ),
        "{}",
        r.out
    );
    assert!(
        got[0].ends_with(
            "; a manager that renders instance.env supplies it, so check the instance.env it pushes"
        ),
        "{}",
        r.out
    );
    assert_eq!(got[1], "fail");
}

#[test]
fn a_model_that_models_json_does_not_list_fails() {
    let dir = instance(
        "model",
        &["PI_ENSEMBLE_MODEL_CODE_REVIEW_SPECIALIST=unlisted-model".into()],
    );
    let r = check(&["--settings", &dir.to_string_lossy()]);
    assert_eq!(r.rc, 78, "{}{}", r.out, r.err);
    assert_eq!(
        lines(&r),
        [
            "fail models.json: models.json does not list model unlisted-model under provider local",
            "fail"
        ]
    );
}

#[test]
fn what_the_shell_sets_is_what_is_dropped_not_what_a_line_looks_like() {
    let dir = instance(
        "shell",
        &[
            "declare -x TEHAS_WIKI=1".into(),
            "if false; then TEHAS_ENERGY=on; fi".into(),
        ],
    );
    let r = check(&["--settings", &dir.to_string_lossy()]);
    assert_eq!(r.rc, 0, "{}{}", r.out, r.err);
    assert_eq!(
        lines(&r),
        [
            format!("dropped TEHAS_WIKI: {}", ignored_why("TEHAS_WIKI").unwrap()),
            "pass".to_string()
        ]
    );
}

#[test]
fn a_relative_settings_path_is_read_as_the_directory_it_names() {
    let dir = instance("relative", &[]);
    let home = scratch("home-relative");
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["config-check", "--settings", "."])
        .current_dir(dir.path())
        .env("HOME", home.path())
        .env_remove("TEHAS_INSTANCE_DIR")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "pass\n");
}

/// 2.0 rewrites no setting. A rewrite table of `tehas update`, when one is
/// added, lives in console/ops/src/update/rewrites.rs, and config-check must
/// read it to print its `changed` lines.
#[test]
fn a_rewrite_table_of_the_update_is_read_by_config_check() {
    let table = repo().join("console/ops/src/update/rewrites.rs");
    let check = fs::read_to_string(repo().join("console/ops/src/settings.rs")).unwrap();
    assert!(
        !table.exists() || check.contains("update::rewrites"),
        "{} exists and config_check (console/ops/src/settings.rs) does not read it",
        table.display()
    );
}

/// An instance whose models.json names TEHAS_MODEL_KEY_FLEET as its key, and
/// a host env file beside it with `text`.
fn keyed(name: &str, text: &str) -> (TestDir, TestDir) {
    let dir = instance(name, &[]);
    let file = dir.join("models.json");
    let json = fs::read_to_string(&file).unwrap();
    let named = json.replace(
        "\"apiKey\": \"unused\"",
        "\"apiKey\": \"$TEHAS_MODEL_KEY_FLEET\"",
    );
    assert_ne!(named, json);
    fs::write(&file, named).unwrap();
    let host = scratch(&format!("{name}-env"));
    fs::write(host.join("env"), text).unwrap();
    (dir, host)
}

fn check_with_env(dir: &TestDir, host: &TestDir) -> Run {
    check(&[
        "--settings",
        &dir.to_string_lossy(),
        "--env",
        &host.join("env").to_string_lossy(),
    ])
}

const UNSET: &str = "fail TEHAS_MODEL_KEY_FLEET: models.json names TEHAS_MODEL_KEY_FLEET and it is not set: put it in ~/.config/tehas/env";

#[test]
fn a_models_json_key_unset_without_env_fails() {
    let (dir, _) = keyed("key-unset", "");
    let r = check(&["--settings", &dir.to_string_lossy()]);
    assert_eq!(r.rc, 78, "{}{}", r.out, r.err);
    assert_eq!(lines(&r), [UNSET, "fail"]);
}

#[test]
fn a_models_json_key_held_redacted_in_the_env_file_is_set() {
    let (dir, host) = keyed(
        "key-redacted",
        "# the host's\nexport TEHAS_MODEL_KEY_FLEET=<redacted>\nTEHAS_MCP=<redacted>\nTEHAS_MANAGER_VERSION=v1.0.0\n",
    );
    let r = check_with_env(&dir, &host);
    assert_eq!(r.rc, 0, "{}{}", r.out, r.err);
    assert_eq!(lines(&r), ["pass"]);
}

#[test]
fn a_real_value_in_the_env_file_is_checked_and_not_shown() {
    let (dir, host) = keyed("key-real", "TEHAS_MODEL_KEY_FLEET=sk-good-0123\n");
    let r = check_with_env(&dir, &host);
    assert_eq!(r.rc, 0, "{}{}", r.out, r.err);
    assert!(!r.out.contains("sk-good"), "{}", r.out);

    for (name, line, why) in [
        (
            "TEHAS_MODEL_KEY_FLEET",
            "TEHAS_MODEL_KEY_FLEET='sk-secret\\x'\n",
            "expected one word of printable characters, with no quote, backslash, $ or backtick",
        ),
        (
            "TEHAS_MCP_KEYS",
            "TEHAS_MCP_KEYS=short-secret\n",
            "key 1 is not at least 32 characters of A-Za-z0-9_-",
        ),
    ] {
        let (dir, host) = keyed(&format!("bad-{name}"), line);
        let r = check_with_env(&dir, &host);
        assert_eq!(r.rc, 78, "{}{}", r.out, r.err);
        assert_eq!(
            lines(&r),
            [
                format!(
                    "fail {name}: {name} is not valid: {why} (see {})",
                    host.join("env").display()
                )
                .as_str(),
                "fail"
            ]
        );
        for word in ["secret", "sk-"] {
            assert!(
                !r.out.contains(word) && !r.err.contains(word),
                "{}{}",
                r.out,
                r.err
            );
        }
    }
}

#[test]
fn a_quoted_marker_is_an_ordinary_value() {
    // A token may hold < and >, so '<redacted>' quoted is a key like any other.
    let (dir, host) = keyed("key-quoted", "TEHAS_MODEL_KEY_FLEET='<redacted>'\n");
    let r = check_with_env(&dir, &host);
    assert_eq!(r.rc, 0, "{}{}", r.out, r.err);
    // Quoted where its rule refuses < and >, it is refused as a value.
    let (dir, host) = keyed("mcp-quoted", "TEHAS_MCP='<redacted>'\n");
    let r = check_with_env(&dir, &host);
    assert_eq!(r.rc, 78, "{}{}", r.out, r.err);
    assert!(
        lines(&r)[0].starts_with("fail TEHAS_MCP: TEHAS_MCP is not valid: "),
        "{}",
        r.out
    );
}

#[test]
fn a_malformed_env_file_fails_and_names_the_line_not_its_text() {
    let (dir, host) = keyed(
        "env-malformed",
        "TEHAS_MODEL_KEY_FLEET=<redacted>\nsk-secret-line\n",
    );
    let r = check_with_env(&dir, &host);
    assert_eq!(r.rc, 78, "{}{}", r.out, r.err);
    let file = host.join("env");
    assert_eq!(
        lines(&r),
        [
            format!(
                "fail env: line 2 is not NAME=value (see {})",
                file.display()
            )
            .as_str(),
            "fail"
        ]
    );
    assert!(!r.out.contains("secret"), "{}", r.out);
}

#[test]
fn a_name_the_host_env_file_may_not_set_fails() {
    for (name, line) in [
        ("GH_TOKEN", "GH_TOKEN=ghp-secret\n"),
        ("TEHAS_REPO", "TEHAS_REPO=a/b\n"),
    ] {
        let (dir, host) = keyed(&format!("env-{name}"), line);
        let r = check_with_env(&dir, &host);
        assert_eq!(r.rc, 78, "{}{}", r.out, r.err);
        assert_eq!(
            lines(&r)[0],
            format!(
                "fail {name}: {name} is not a setting of ~/.config/tehas/env (see {})",
                host.join("env").display()
            )
        );
        assert!(!r.out.contains("secret"), "{}", r.out);
    }
}

#[test]
fn an_env_file_that_is_not_there_or_given_twice_is_78_and_reads_nothing() {
    let (dir, host) = keyed("env-missing", "");
    let missing = host.join("no-such-env");
    let r = check(&[
        "--settings",
        &dir.to_string_lossy(),
        "--env",
        &missing.to_string_lossy(),
    ]);
    assert_eq!(r.rc, 78, "{}{}", r.out, r.err);
    assert_eq!(r.out, "");
    assert!(r.err.contains("no env file at"), "{}", r.err);
    let env = host.join("env");
    let env = env.to_string_lossy();
    for args in [
        vec![
            "--settings",
            &dir.to_string_lossy(),
            "--env",
            &env,
            "--env",
            &env,
        ],
        vec!["--env", &env],
        vec!["--settings", &dir.to_string_lossy(), "--env"],
        vec!["--settings", &dir.to_string_lossy(), "--other", "x"],
    ] {
        let r = check(&args);
        assert_eq!(r.rc, 78, "{args:?} {}{}", r.out, r.err);
        assert_eq!(r.out, "");
        assert!(r.err.contains("[--env <file>]"), "{}", r.err);
    }
}
