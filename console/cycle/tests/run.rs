//! `tehas-cycle run`: the cycle runs from its own copy of the install, or
//! from the install when the copy cannot be made.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

/// A home whose tehasbot prints where it runs from and what it was given.
fn home(name: &str) -> PathBuf {
    let h = std::env::temp_dir().join(format!("tehas-run-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&h);
    for t in ["bin", "libexec", "lib"] {
        fs::create_dir_all(h.join(t)).unwrap();
    }
    let bot = h.join("bin/tehasbot");
    fs::write(&bot, "#!/bin/sh\necho \"$0 $* code=$TEHAS_CODE pid=$$\"\n").unwrap();
    fs::set_permissions(&bot, fs::Permissions::from_mode(0o755)).unwrap();
    h
}

fn run(h: &PathBuf) -> (String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_tehas-cycle"))
        .args(["run", "7"])
        .env("TEHAS_HOME", h)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    (
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

#[test]
fn the_cycle_runs_from_its_copy_named_for_its_pid() {
    let h = home("copy");
    // A copy of a process that is gone: pid 0 is never a process's.
    fs::create_dir_all(h.join("run/cycle-3-0")).unwrap();
    let (out, err) = run(&h);
    let pid = out.trim().rsplit("pid=").next().unwrap();
    let dir = h.join(format!("run/cycle-7-{pid}"));
    assert_eq!(
        out.trim(),
        format!(
            "{}/bin/tehasbot cycle 7 code={} pid={pid}",
            dir.display(),
            dir.display()
        )
    );
    assert!(
        err.contains(&format!("tehas-cycle: #7 runs from {}", dir.display())),
        "{err}"
    );
    assert!(!h.join("run/cycle-3-0").exists());
    fs::remove_dir_all(&h).unwrap();
}

#[test]
fn a_copy_that_cannot_be_made_runs_the_install() {
    let h = home("fallback");
    fs::remove_dir_all(h.join("lib")).unwrap();
    let (out, err) = run(&h);
    assert!(
        out.starts_with(&format!(
            "{}/bin/tehasbot cycle 7 code={} ",
            h.display(),
            h.display()
        )),
        "{out}"
    );
    assert!(
        err.contains("#7 runs from the install: its own copy could not be made"),
        "{err}"
    );
    fs::remove_dir_all(&h).unwrap();
}

#[test]
fn a_home_that_is_not_absolute_is_refused() {
    let out = Command::new(env!("CARGO_BIN_EXE_tehas-cycle"))
        .args(["run", "7"])
        .env("TEHAS_HOME", "tehas")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(78));
}

#[test]
fn a_planning_turn_runs_its_program_from_its_copy() {
    let h = home("plan");
    let plan = h.join("libexec/plan");
    fs::write(&plan, "#!/bin/sh\necho \"$0 $* code=$TEHAS_CODE pid=$$\"\n").unwrap();
    fs::set_permissions(&plan, fs::Permissions::from_mode(0o755)).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_tehas-cycle"))
        .args(["pin", "plan", "7", "plan", "7", "--out", "o"])
        .env("TEHAS_HOME", &h)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let out = String::from_utf8(out.stdout).unwrap();
    let pid = out.trim().rsplit("pid=").next().unwrap();
    let dir = h.join(format!("run/plan-7-{pid}"));
    assert_eq!(
        out.trim(),
        format!(
            "{}/libexec/plan 7 --out o code={} pid={pid}",
            dir.display(),
            dir.display()
        )
    );
    fs::remove_dir_all(&h).unwrap();
}

#[test]
fn a_planning_turn_names_a_program_of_libexec_only() {
    for program in ["../bin/tehasbot", "", ".hidden"] {
        let out = Command::new(env!("CARGO_BIN_EXE_tehas-cycle"))
            .args(["pin", "plan", "7", program])
            .env("TEHAS_HOME", "/nonexistent")
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(64), "{program}");
    }
}
