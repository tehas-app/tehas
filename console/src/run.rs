//! The one way into a command that runs under the instance's settings.
//!
//!   console run <libexec dir> <euid> <bash> tehas|tehasbot <command> [arguments]
//!
//! bin/tehas and bin/tehasbot call it, and the few paths a test or a shim still
//! names. `<euid>` and `<bash>` are the shell's, as for `console settings load`;
//! a command which needs the uid itself asks `id` (src/account.rs). Each run
//! goes in this order:
//!
//!   1. `check` on the words: a bad argument is 64 and a help text 0 whatever the
//!      settings hold, and neither loads them. A command with no check passes.
//!   2. tehas_load on this process's environment, as a shell's load leaves it, so
//!      the verb and whatever it starts have the same names.
//!   3. what the command cannot run without, `commands::required`.
//!   4. the verb, in this process, or by exec when it is another program.
//!
//! So a bad setting is 78 before GitHub, Docker or git and before any write
//! (AGENTS.md, rule 2). A command's own rule sits between 2 and 4:
//!
//!   improve   exit 0 before step 3 when TEHAS_IMPROVE_REPO is unset, the loop is off
//!   refresh   exit 0 before step 3 unless TEHAS_REFRESH_CONFLICTS is 1
//!   review    exit 0 after step 3 unless TEHAS_REVIEW is 1
//!   preview   exit 78 after step 3 unless TEHAS_PREVIEWS is 1
//!
//! The review's gate sits below the check of the settings it needs because an
//! agent runs here on a timer with no person in front of it: a weekly run says 78
//! for an instance that could not review rather than exiting 0 on it.
//!
//! What runs here hands no descriptor to a child, so nothing here needs a
//! close-on-exec cleared. libexec/index does, which is why it stays bash.
//!
//! Exit: the command's; 2 an argument that is not UTF-8; 64 usage; 78 the
//! settings are missing or malformed.

use crate::commands::{self, ANSWERED};
use crate::{clean, disk, github, improve, plan, preview, product, protolab, review, settings};
use std::env;
use std::ffi::OsString;
use std::fs::File;
use std::io::{ErrorKind, Read};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;
use tehas_core::settings as core_settings;
use tehas_core::slice::{self, Host, Load};

/// bash, not an exec of this program: glibc's exec gives a file the kernel
/// refuses to /bin/sh, and a poll that is no program has to end 78.
const POLL: &str = r#"shopt -s execfail; exec "$0" "$@"; echo "tehas: $0 cannot run (run install.sh)" >&2; exit 78"#;

fn var(name: &str) -> String {
    env::var(name).unwrap_or_default()
}

/// tehas-dispatch, or what a test names in its place.
fn dispatcher(libexec: &str) -> String {
    match var("TEHAS_DISPATCH_CMD") {
        named if named.is_empty() => format!("{libexec}/tehas-dispatch"),
        named => named,
    }
}

/// An executable file that starts as a program does. bash would run any other
/// file as a script of its own, and an empty one is a poll that exits 0.
fn a_program(path: &Path) -> bool {
    let runs = path
        .metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
    let mut magic = Vec::new();
    let read = File::open(path).and_then(|f| f.take(4).read_to_end(&mut magic));
    runs && read.is_ok() && (magic == b"\x7fELF" || magic.starts_with(b"#!"))
}

/// tehas_checkout then tehas_branch. A cycle must never start from a guess, nor
/// in a checkout of another repository.
fn branch() -> Result<String, u8> {
    core_settings::checkout()
        .and_then(|()| core_settings::branch())
        .map_err(|line| {
            eprintln!("{line}");
            78
        })
}

fn dispatch(bash: &str, libexec: &str, rest: &[String]) -> Result<u8, u8> {
    branch()?;
    let poll = dispatcher(libexec);
    if !a_program(Path::new(&poll)) {
        eprintln!("tehas: {poll} is not installed (run install.sh)");
        return Ok(78);
    }
    let failed = Command::new(bash)
        .args(["-c", POLL, &poll, "poll"])
        .args(rest)
        .exec();
    eprintln!("tehas: {poll} cannot run (run install.sh): {failed}");
    Ok(78)
}

/// tehas-dispatch's own refresh verb takes over, with the status a shell's exec
/// would have ended with when it cannot. `log` names the file the run's line
/// goes to, which only the settings know: a hand-off before the load, which is
/// what an argument is, has none to give.
fn hand_to_dispatcher(libexec: &str, rest: &[String], log: Option<String>) -> u8 {
    let program = dispatcher(libexec);
    let mut refresh = Command::new(&program);
    refresh.arg("refresh").args(rest);
    if let Some(log) = log {
        refresh.env("TEHAS_REFRESH_LOG", log);
    }
    let failed = refresh.exec();
    eprintln!("tehasbot refresh: {program}: {failed}");
    if failed.kind() == ErrorKind::NotFound {
        127
    } else {
        126
    }
}

/// The GitHub App's token goes in this environment for the pass and its children.
fn protolab_run() -> u8 {
    if !var("TEHAS_GITHUB_APP_ID").is_empty() {
        let Some(token) = github::for_a_container() else {
            eprintln!("tehasbot protolab: no GitHub token for the app");
            return 1;
        };
        env::set_var("GH_TOKEN", token);
    }
    protolab::main(&["run".to_string()])
}

/// A run with nothing to read starts no container and asks no slice; one without
/// a usable slice stops before the console calls GitHub, and its records wait.
fn improve_run(libexec: &str) -> u8 {
    let prompt = format!("{libexec}/../lib/improve-prompt.md");
    let words = |verb: &str| [verb.to_string(), prompt.clone()];
    match improve::main(&words("due")) {
        0 => {}
        10 => return 0,
        rc => return rc,
    }
    let load = Load::from_env();
    if let Err(why) = slice::usable(&Host::here(), &load.reserve, &load.containers) {
        eprintln!("tehas improve: {why}");
        return 78;
    }
    improve::main(&words("run"))
}

/// A free figure that cannot be read changes nothing.
fn clean_run(rest: &[String]) -> Result<u8, u8> {
    let dry = if rest.first().is_some_and(|w| w == "--dry-run") {
        "1"
    } else {
        "0"
    };
    let image = clean::sandbox_image(&var("PI_ENSEMBLE_IMAGE"))?;
    let home = var("TEHAS_HOME");
    let free = disk::free_kb(&home).map_or(String::new(), |kb| kb.to_string());
    Ok(clean::main(&[
        "run".to_string(),
        dry.to_string(),
        home.clone(),
        var("TEHAS_WORK_DIR"),
        var("TEHAS_CLEAN_AFTER"),
        var("TEHAS_CLEAN_MIN_FREE_MB"),
        var("TEHAS_CLEAN_MIN_FREE"),
        free,
        var("TEHAS_CLEAN_DIRS"),
        image,
        format!("{home}/logs/clean.log"),
        var("TEHAS_SCRATCH_DIR"),
    ]))
}

fn preview_run(rest: &[String], settle: OsString) -> u8 {
    if var("TEHAS_PREVIEWS") != "1" {
        eprintln!("tehas: TEHAS_PREVIEWS is not 1: this instance has no previews");
        return 78;
    }
    env::set_var("TEHAS_PREVIEW_SETTLE", settle);
    let mode = if rest.first().is_some_and(|w| w == "--sweep") {
        "sweep"
    } else {
        "run"
    };
    preview::main(&[mode.to_string()])
}

fn init_product(rest: &[String]) -> Result<u8, u8> {
    let words: Vec<String> = std::iter::once(branch()?)
        .chain(rest.iter().cloned())
        .collect();
    Ok(product::init(&words))
}

/// Steps 2 to 4. Err is the status of a refusal.
fn command(libexec: &str, uid: &str, bash: &str, words: &[String]) -> Result<u8, u8> {
    let rest = words.get(2..).unwrap_or_default();
    let name: Vec<&str> = words.iter().take(2).map(String::as_str).collect();
    // Before the settings, so that a refusal needs none. Of the words the
    // check lets through, only --json goes on: it is a run, so it goes over
    // below, after the load, which is where the log it writes is named. An
    // empty argument is an argument, so it is refused rather than run.
    if name == ["tehasbot", "refresh"] && !rest.iter().all(|w| w == "--json") {
        return Ok(hand_to_dispatcher(libexec, rest, None));
    }
    // The load drops every TEHAS_ name it does not keep.
    let settle = env::var_os("TEHAS_PREVIEW_SETTLE").unwrap_or_default();
    let vars = settings::load_here(uid, bash)?;
    match name[..] {
        ["tehasbot", "improve"] if var("TEHAS_IMPROVE_REPO").is_empty() => {
            tehas_core::outln!(
                "tehas improve: TEHAS_IMPROVE_REPO is not set, the improvement loop is off"
            );
            return Ok(0);
        }
        ["tehasbot", "refresh"] if var("TEHAS_REFRESH_CONFLICTS") != "1" => return Ok(0),
        _ => {}
    }
    settings::require(commands::required(words), &vars)?;
    Ok(match name[..] {
        ["tehas", "check-product"] => product::check(rest),
        ["tehas", "init-product"] => init_product(rest)?,
        ["tehas", "plan"] => plan::start(libexec, rest),
        ["tehas", _] => commands::tehas(&words[1..]),
        ["tehasbot", "clean"] => clean_run(rest)?,
        ["tehasbot", "dispatch"] => dispatch(bash, libexec, rest)?,
        ["tehasbot", "improve"] => improve_run(libexec),
        ["tehasbot", "preview"] => preview_run(rest, settle),
        ["tehasbot", "protolab"] => protolab_run(),
        ["tehasbot", "refresh"] => {
            let log = format!("{}/logs/refresh.log", var("TEHAS_HOME"));
            hand_to_dispatcher(libexec, rest, Some(log))
        }
        ["tehasbot", "review"] if var("TEHAS_REVIEW") != "1" => 0,
        ["tehasbot", "review"] => review::main(&["main".to_string()]),
        _ => commands::tehasbot(&words[1..]),
    })
}

/// `text` is whether every argument the program was given is UTF-8. `check` is
/// asked either way: it refuses such a word with the usage of its command.
pub fn main(given: &[String], text: bool) -> u8 {
    let [libexec, uid, bash, words @ ..] = given else {
        eprintln!(
            "usage: console run <libexec dir> <euid> <bash> tehas|tehasbot <command> [arguments]"
        );
        return 64;
    };
    match commands::check(words) {
        0 => {}
        ANSWERED => return 0,
        rc => return rc,
    }
    if !text {
        eprintln!("console: an argument is not UTF-8");
        return 2;
    }
    command(libexec, uid, bash, words).unwrap_or_else(|rc| rc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> tehas_core::testdir::TestDir {
        tehas_core::testdir::TestDir::new(&format!("run-{name}"))
    }

    fn file(dir: &Path, bytes: &[u8], mode: u32) -> std::path::PathBuf {
        let path = dir.join("poll");
        fs::write(&path, bytes).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        path
    }

    #[test]
    fn a_poll_is_an_executable_file_that_starts_as_a_program_does() {
        let dir = scratch("program");
        for bytes in [
            &b"\x7fELF"[..],
            b"\x7fELF\x02\x01\x01",
            b"#!/bin/sh\n",
            b"#!",
        ] {
            assert!(a_program(&file(&dir, bytes, 0o755)), "{bytes:?}");
        }
        for bytes in [
            &b""[..],
            b"\n",
            b"not a program\n",
            b"\x01\x02\x03\x04",
            b"\x7fEL",
        ] {
            assert!(!a_program(&file(&dir, bytes, 0o755)), "{bytes:?}");
        }
        assert!(!a_program(&file(&dir, b"#!/bin/sh\n", 0o644)));
        assert!(!a_program(&dir));
        assert!(!a_program(&dir.join("no-such")));
    }

    #[test]
    fn too_few_words_are_a_usage_error() {
        assert_eq!(main(&["/l".to_string()], true), 64);
    }
}
