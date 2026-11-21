//! The commands of `bin/tehas` and `bin/tehasbot` that are verbs of this
//! program. src/front.rs chooses them.
//!
//!   console tehas status [--json]
//!   console tehas disk [...]
//!   console tehas state pack|unpack ... | check [--json]
//!   console tehas limit [--json] | set ... | clear
//!   console tehas blockers [--json]
//!   console tehas pause [--reason TEXT]
//!   console tehas analytics report | copy --to <file> [--json]
//!   console tehas top [--once | --json] [--every <seconds>]
//!   console tehas api [--json]
//!   console tehas labels
//!   console tehas panel [--json] | set <id> ... | clear <id>
//!   console tehas host-message [list] [--json] | set <id> ... | clear <id>
//!   console tehas resume
//!   console tehas drain [--wait SECONDS] [--json]
//!   console tehas history [--json] [--after CURSOR] [--issue N]
//!   console tehas export [--json] --after CURSOR <dir>
//!   console tehas stop <issue> [--keep-workspace]
//!   console tehas freeze <issue> [--reason TEXT] [--json]
//!   console tehas thaw <issue> [--json]
//!   console tehas wake [--json]
//!   console tehas dev <arguments>
//!   console tehas dev-login "Name <email>" [-- <pi arguments>]
//!   console tehas github writes
//!   console tehas outsource invite|list|status|revoke|drain [...] [--json]
//!   console tehas prototype file ...
//!   console tehas plugin [--json]
//!   console tehas|tehasbot mcp serve|key
//!   console tehasbot health
//!   console tehasbot status
//!   console tehasbot top [--every]
//!   console tehasbot github-token [--cycle | --session] [--check]
//!   console tehasbot github-writes
//!
//! status, disk, top and mcp key need no settings, and dev loads its own. The
//! rest run under `console run` (src/run.rs), which first asks [`check`] with the
//! same words: an argument that cannot be right is 64 with the usage, whatever
//! the settings hold, and only then are they loaded. [`required`] then names the
//! settings the command cannot run without.
//!
//!   console usage <command>
//!
//! The text of `tehas help <command>`, from src/usage/<command>.txt.
//!
//! Exit: the status of the command; 64 usage.

use crate::{
    api, blockers, clean, dev, disk, export, freeze, github, health, history, holds, host_message,
    labels, mcp, order, panel, queue, review, start, state, status, stop, top, wake,
};
use std::env;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tehas_core::exit;
use tehas_core::settings::INSTANCE_SETTINGS;

fn strings(words: &[&str], rest: &[String]) -> Vec<String> {
    words
        .iter()
        .map(|w| w.to_string())
        .chain(rest.iter().cloned())
        .collect()
}

/// `tehas limit` as the words of `console flags`.
fn limit_words(given: &[String]) -> Result<Vec<String>, u8> {
    let first = given.first().map(String::as_str);
    match (first, given.len()) {
        (None | Some(""), _) | (Some("--json"), 1) => Ok(strings(&["limit", "show"], given)),
        (Some("clear"), 1) => Ok(strings(&["limit", "clear"], &[])),
        (Some("set"), _) => Ok(strings(&["limit", "set"], &given[1..])),
        _ => {
            eprintln!("usage: tehas limit [--json] | set --cycles N [--plan on|off] [--for SECONDS | --until TIME] [--reason TEXT] | clear");
            Err(64)
        }
    }
}

/// `tehas pause` as the words of `console flags`. The flag makes the reason
/// one line; an empty one is none.
fn pause_words(given: &[String]) -> Result<Vec<String>, u8> {
    let words: Vec<&str> = given.iter().map(String::as_str).collect();
    match words[..] {
        [] | ["--reason", ""] => Ok(strings(&["pause", "set"], &[])),
        ["--reason", _] => Ok(strings(&["pause", "set"], given)),
        [word] if !word.starts_with('-') => {
            eprintln!("tehas pause: the reason is given as --reason TEXT");
            Err(64)
        }
        _ => {
            eprintln!("usage: tehas pause [--reason TEXT]");
            Err(64)
        }
    }
}

fn analytics_words(given: &[String]) -> Result<(), u8> {
    let words: Vec<&str> = given.iter().map(String::as_str).collect();
    let file_ok = |f: &str| !f.is_empty() && !f.starts_with('-');
    let good = match words[..] {
        ["report"] => true,
        ["copy", "--to", file]
        | ["copy", "--to", file, "--json"]
        | ["copy", "--json", "--to", file] => file_ok(file),
        ["export", ..] => {
            eprintln!(
                "tehas analytics: export is now copy: tehas analytics copy --to <file> [--json]"
            );
            return Err(64);
        }
        _ => false,
    };
    if !good {
        eprintln!("usage: tehas analytics report | copy --to <file> [--json]");
        return Err(64);
    }
    Ok(())
}

/// The analytics program answers, with nothing on its standard input. Its own
/// statuses are internal: the command passes on 0, 64, 78 and 141, and any
/// other is a failure, 1. Analytics switched off is a setting, 78.
fn analytics(given: &[String]) -> u8 {
    if let Err(rc) = analytics_words(given) {
        return rc;
    }
    if env::var("TEHAS_ANALYTICS").as_deref() != Ok("on") {
        eprintln!(
            "tehas analytics: analytics is off on this host (TEHAS_ANALYTICS=off); nothing is recorded"
        );
        return exit::SETTINGS;
    }
    let home = PathBuf::from(env::var_os("TEHAS_HOME").unwrap_or_default());
    let bin = home.join("libexec/analytics");
    let not_installed = || {
        eprintln!(
            "tehas analytics: {} is not installed; install.sh builds it",
            bin.display()
        );
        exit::FAILED
    };
    if !bin
        .metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    {
        return not_installed();
    }
    let ran = Command::new(&bin)
        .args(given)
        .stdin(Stdio::null())
        .env("TEHAS_HOME", &home)
        .status();
    match ran {
        Ok(status) => analytics_status(status.code(), status.signal()),
        Err(_) => not_installed(),
    }
}

/// The command's status for the program's exit code or the signal that ended
/// it.
fn analytics_status(code: Option<i32>, signal: Option<i32>) -> u8 {
    let pipe = i32::from(exit::CLOSED) - 128;
    match (code, signal) {
        (Some(0), _) => exit::OK,
        (Some(64), _) => exit::USAGE,
        (Some(78), _) => exit::SETTINGS,
        (Some(141), _) => exit::CLOSED,
        (None, Some(sig)) if sig == pipe => exit::CLOSED,
        _ => exit::FAILED,
    }
}

pub fn tehas(given: &[String]) -> u8 {
    let Some((command, rest)) = given.split_first() else {
        eprintln!("console tehas: which command?");
        return 64;
    };
    let flag = |words: Result<Vec<String>, u8>| match words {
        Ok(words) => holds::main(&words),
        Err(rc) => rc,
    };
    match command.as_str() {
        "status" => status::print(rest),
        "disk" => disk::main(rest),
        "state" => state::main(rest),
        "limit" => flag(limit_words(rest)),
        "order" => order::main(rest),
        "blockers" => blockers::main(rest),
        "start" => start::main(rest),
        "pause" => flag(pause_words(rest)),
        "analytics" => analytics(rest),
        "top" => top::view(rest),
        "api" => api::main(rest),
        "labels" => labels::main(rest),
        "panel" => panel::main(rest),
        "plugin" => tehas_ops::plugin::main(rest),
        "host-message" => host_message::main(rest),
        "resume" => holds::resume(rest),
        "drain" => holds::drain(rest),
        "history" => history::main(rest),
        "export" => export::main(rest),
        "stop" => stop::main(rest),
        "freeze" => freeze::freeze(rest),
        "thaw" => freeze::thaw(rest),
        "dev" => dev::start(rest),
        "dev-login" => dev::login(rest),
        "mcp" => mcp::main(rest),
        "github" => github_verb(rest),
        "outsource" => crate::outsource::main(rest),
        "prototype" => crate::prototype::main(rest),
        "wake" => wake::main(rest),
        _ => {
            eprintln!("console tehas: no such command: {command}");
            64
        }
    }
}

const GITHUB: &str = include_str!("usage/github.txt");

/// `tehas github writes`. `tehas github check` runs under the settings as
/// `tehasbot github-token --check` (src/front.rs), so it is not met here.
fn github_verb(given: &[String]) -> u8 {
    match given {
        [verb] if verb == "writes" => crate::gh_writes::main(&[]),
        _ => {
            eprint!("{GITHUB}");
            64
        }
    }
}

pub fn tehasbot(given: &[String]) -> u8 {
    match given.first().map(String::as_str) {
        Some("health") => health::loaded(),
        Some("status") => queue::main(),
        Some("top") => top::publish(&given[1..]),
        Some("mcp") => mcp::main(&given[1..]),
        Some("github-token") => github::main(&given[1..]),
        Some("github-writes") => crate::gh_writes::main(&given[1..]),
        _ => {
            eprintln!(
                "console tehasbot: usage: health | status | top [--every] | mcp | github-token [--cycle | --session] [--check] | github-writes"
            );
            64
        }
    }
}

/// What `check` ends with when it was the whole answer, as a help text met
/// among the arguments is: `console run` then ends 0 and loads nothing.
pub const ANSWERED: u8 = 99;

const DISPATCH: &str = include_str!("usage/dispatch.txt");

/// A poll starts only from an argument it knows: a typo such as --dryrun is no
/// poll. At most one argument: a second is refused, never combined with the
/// first, so --dry-run --help cannot be read as two intents at once. An empty
/// argument is an argument too.
fn dispatch_words(given: &[String]) -> Result<(), u8> {
    let first = given.first().map(String::as_str);
    let help = matches!(first, Some("-h" | "--help"));
    let unknown = match (first, given.get(1)) {
        (Some(word), _) if !help && !matches!(word, "--dry-run" | "--soon") => Some(word),
        (_, Some(second)) => Some(second.as_str()),
        _ => None,
    };
    if let Some(word) = unknown {
        eprint!("tehas dispatch: unknown argument: {word}\n{DISPATCH}");
        return Err(64);
    }
    if help {
        say(DISPATCH);
        return Err(ANSWERED);
    }
    Ok(())
}

fn clean_words(given: &[String]) -> Result<(), u8> {
    match given {
        [] => Ok(()),
        [word] if word.is_empty() || word == "--dry-run" => Ok(()),
        _ => Err(match clean::usage(given) {
            0 => ANSWERED,
            rc => rc,
        }),
    }
}

fn product_words(given: &[String], flag: &str, usage: &str) -> Result<(), u8> {
    match given {
        [] => Ok(()),
        [word] if word == flag => Ok(()),
        _ => {
            eprintln!("usage: {usage}");
            Err(64)
        }
    }
}

/// Only the first word counts, as it always has.
fn preview_words(given: &[String]) -> Result<(), u8> {
    match given.first().map(String::as_str) {
        None | Some("" | "--sweep") => Ok(()),
        _ => {
            eprintln!("usage: tehasbot preview [--sweep]");
            Err(64)
        }
    }
}

/// What can be said of the arguments before the settings are loaded. A
/// command with no check here passes.
pub fn check(given: &[String]) -> u8 {
    let words: Vec<&str> = given.iter().take(2).map(String::as_str).collect();
    let rest = given.get(2..).unwrap_or_default();
    let said = match words[..] {
        ["tehas", "limit"] => limit_words(rest).map(|_| ()),
        ["tehas", "order"] => order::words(rest).map(|_| ()),
        ["tehas", "blockers"] => blockers::words(rest).map(|_| ()),
        ["tehas" | "tehasbot", "mcp"] => mcp::words(rest).map(|_| ()),
        ["tehas", "start"] => start::words(rest).map(|_| ()),
        ["tehas", "pause"] => pause_words(rest).map(|_| ()),
        ["tehas", "analytics"] => analytics_words(rest),
        ["tehas", "api"] => api::words(rest).map(|_| ()),
        ["tehas", "wake"] => wake::words(rest).map(|_| ()),
        ["tehas", "labels"] => labels::words(rest),
        ["tehas", "outsource"] => match crate::outsource::words(rest) {
            Ok(None) => {
                say(crate::outsource::USAGE);
                Err(ANSWERED)
            }
            Ok(Some(_)) => Ok(()),
            Err(rc) => Err(rc),
        },
        ["tehas", "panel"] => panel::words(rest).map(|_| ()),
        ["tehas", "plugin"] => tehas_ops::plugin::words(rest).map(|_| ()),
        ["tehas", "host-message"] => host_message::words(rest).map(|_| ()),
        ["tehas", "resume"] => holds::resume_words(rest),
        ["tehas", "drain"] => holds::drain_words(rest).map(|_| ()),
        ["tehas", "history"] => history::words(rest).map(|_| ()),
        ["tehas", "export"] => export::words(rest).map(|_| ()),
        ["tehas", "stop"] => match stop::parse(rest) {
            Ok(Some(_)) => Ok(()),
            Ok(None) => Err(ANSWERED),
            Err(rc) => Err(rc),
        },
        ["tehas", verb @ ("freeze" | "thaw")] => match freeze::words(rest, verb == "freeze") {
            Ok(Some(_)) => Ok(()),
            Ok(None) => Err(ANSWERED),
            Err(rc) => Err(rc),
        },
        ["tehas", "check-product"] => {
            product_words(rest, "--sandbox", "tehas check-product [--sandbox]")
        }
        ["tehas", "init-product"] => {
            product_words(rest, "--dry-run", "tehas init-product [--dry-run]")
        }
        ["tehas", "plan"] => crate::plan::turn_words(rest).map(|_| ()),
        ["tehas", "work"] => issue_words(
            rest.first(),
            "usage: tehas work <issue-number> [--restart] [--driver lean|classic]",
        ),
        ["tehas", "index"] => match rest {
            [] => Ok(()),
            [w] if w == "--before-cycle" => Ok(()),
            _ => {
                eprintln!("usage: tehas index [--before-cycle]");
                Err(64)
            }
        },
        ["tehasbot", "cycle"] => match rest {
            [n] => issue_words(Some(n), "usage: tehasbot cycle <issue>"),
            _ => issue_words(None, "usage: tehasbot cycle <issue>"),
        },
        ["tehasbot", "clean"] => clean_words(rest),
        ["tehasbot", "dispatch"] => dispatch_words(rest),
        ["tehasbot", "github-token"] => github::words(rest).map(|_| ()),
        ["tehasbot", "preview"] => preview_words(rest),
        ["tehasbot", "review"] => review::words(rest, include_str!("usage/review.txt")),
        _ => Ok(()),
    };
    said.err().unwrap_or(0)
}

/// An issue's number, as the factory writes one: no sign, no leading zero.
fn issue_words(word: Option<&String>, usage: &str) -> Result<(), u8> {
    let number = word.is_some_and(|w| {
        !w.is_empty() && !w.starts_with('0') && w.bytes().all(|b| b.is_ascii_digit())
    });
    if number {
        return Ok(());
    }
    eprintln!("{usage}");
    Err(64)
}

/// The settings a command cannot run without.
pub fn required(given: &[String]) -> &'static [&'static str] {
    let words: Vec<&str> = given.iter().take(2).map(String::as_str).collect();
    match words[..] {
        ["tehas", "api" | "check-product" | "export" | "history" | "index" | "init-product" | "labels"]
        | ["tehasbot", "cycle" | "preview"] => &["TEHAS_REPO"],
        // What writes or calls GitHub stops on the three before it does.
        ["tehas", "resume"]
        | ["tehasbot", "status"]
        | ["tehas", "start" | "limit" | "order" | "blockers" | "panel" | "host-message" | "freeze" | "thaw"
        | "pause" | "drain" | "wake"] => &[
            "TEHAS_REPO",
            "TEHAS_CONTROL_ISSUE",
            "TEHAS_ALLOWED_LABELERS",
        ],
        ["tehasbot", "clean" | "dispatch" | "refresh" | "review" | "outsource-proxy" | "plugin-gate"] => {
            &INSTANCE_SETTINGS
        }
        ["tehas", "plan" | "work"] => &[
            "TEHAS_REPO",
            "PI_ENSEMBLE_SUBAGENT_MODEL",
            "PI_ENSEMBLE_SUBAGENT_PROVIDER",
        ],
        ["tehasbot", "protolab"] | ["tehas", "prototype"] => {
            &["TEHAS_REPO", "TEHAS_ALLOWED_LABELERS"]
        }
        _ => &[],
    }
}

/// 141 when the reader has gone, as in every command.
fn say(text: &str) {
    tehas_core::out!("{text}");
}

pub fn text(command: &str) -> Option<&'static str> {
    Some(match command {
        "analytics" => include_str!("usage/analytics.txt"),
        "api" => include_str!("usage/api.txt"),
        "blockers" => include_str!("usage/blockers.txt"),
        "check-product" => include_str!("usage/check-product.txt"),
        "disk" => include_str!("usage/disk.txt"),
        "dispatch" => DISPATCH,
        "drain" => include_str!("usage/drain.txt"),
        "export" => include_str!("usage/export.txt"),
        "freeze" => freeze::FREEZE_USAGE,
        "github" => GITHUB,
        "history" => include_str!("usage/history.txt"),
        "host-message" => include_str!("usage/host-message.txt"),
        "index" => include_str!("usage/index.txt"),
        "init-product" => include_str!("usage/init-product.txt"),
        "labels" => include_str!("usage/labels.txt"),
        "limit" => include_str!("usage/limit.txt"),
        "mcp" => include_str!("usage/mcp.txt"),
        "order" => include_str!("usage/order.txt"),
        "outsource" => crate::outsource::USAGE,
        "start" => include_str!("usage/start.txt"),
        "dev" => dev::USAGE,
        "panel" => include_str!("usage/panel.txt"),
        "plugin" => include_str!("usage/plugin.txt"),
        "pause" => include_str!("usage/pause.txt"),
        "plan" => include_str!("usage/plan.txt"),
        "prototype" => include_str!("usage/prototype.txt"),
        // The dispatcher's own text, which `tehasbot refresh --help` prints.
        "refresh" => include_str!("../dispatch/src/usage/refresh.txt"),
        "review" => include_str!("usage/review.txt"),
        "resume" => include_str!("usage/resume.txt"),
        "setup" => include_str!("usage/setup.txt"),
        "state" => include_str!("usage/state.txt"),
        "status" => include_str!("usage/status.txt"),
        "stop" => stop::USAGE,
        "thaw" => freeze::THAW_USAGE,
        "top" => include_str!("usage/top.txt"),
        "update" => include_str!("usage/update.txt"),
        "wake" => include_str!("usage/wake.txt"),
        "work" => include_str!("usage/work.txt"),
        _ => return None,
    })
}

pub fn usage(given: &[String]) -> u8 {
    match given {
        [command] if text(command).is_some() => {
            say(text(command).unwrap_or_default());
            0
        }
        _ => {
            eprintln!("console usage: usage: <command>");
            64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(given: &[&str]) -> Vec<String> {
        given.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn the_analytics_program_s_own_statuses_are_a_failure() {
        for (code, want) in [
            (0, 0),
            (64, 64),
            (78, 78),
            (141, 141),
            (65, 1),
            (70, 1),
            (73, 1),
        ] {
            assert_eq!(analytics_status(Some(code), None), want, "{code}");
        }
        assert_eq!(analytics_status(None, Some(13)), 141);
        assert_eq!(analytics_status(None, Some(9)), 1);
    }

    #[test]
    fn the_blocked_rule_names_the_millwright_the_reader_accepts() {
        // newest_marker and order_stands accept an allowed account or
        // TEHAS_MILLWRIGHT_LOGIN; the printed rule must name the same two.
        let text = DISPATCH.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(!text.contains("or the fixer"), "{text}");
        assert!(text
            .contains("<!-- tehas:blocked by=12,34 --> by an allowed account or the millwright"));
        assert!(text.contains("is by an allowed account or the millwright."));
    }

    #[test]
    fn limit_keeps_what_follows_its_action() {
        assert_eq!(limit_words(&[]), Ok(words(&["limit", "show"])));
        assert_eq!(
            limit_words(&words(&["--json"])),
            Ok(words(&["limit", "show", "--json"]))
        );
        assert_eq!(
            limit_words(&words(&["clear"])),
            Ok(words(&["limit", "clear"]))
        );
        assert_eq!(
            limit_words(&words(&["set", "--cycles", "1"])),
            Ok(words(&["limit", "set", "--cycles", "1"]))
        );
    }

    #[test]
    fn an_empty_first_word_of_limit_is_the_flags_to_refuse() {
        assert_eq!(
            limit_words(&words(&["", "x"])),
            Ok(words(&["limit", "show", "", "x"]))
        );
    }

    #[test]
    fn limit_refuses_any_other_shape() {
        for bad in [
            &["--json", "x"][..],
            &["clear", "x"],
            &["show"],
            &["read"],
            &["--help", "x"],
        ] {
            assert_eq!(limit_words(&words(bad)), Err(64), "{bad:?}");
        }
    }

    #[test]
    fn pause_takes_a_reason_flag_and_an_empty_one_is_none() {
        assert_eq!(pause_words(&[]), Ok(words(&["pause", "set"])));
        assert_eq!(
            pause_words(&words(&["--reason", ""])),
            Ok(words(&["pause", "set"]))
        );
        assert_eq!(
            pause_words(&words(&["--reason", "a b"])),
            Ok(words(&["pause", "set", "--reason", "a b"]))
        );
        for bad in [
            &["a b"][..],
            &[""],
            &["a", "b"],
            &["--reason"],
            &["--reason", "a", "b"],
            &["--why", "a"],
            &["-h", "x"],
        ] {
            assert_eq!(pause_words(&words(bad)), Err(64), "{bad:?}");
        }
    }

    #[test]
    fn analytics_takes_report_or_copy() {
        for good in [
            &["report"][..],
            &["copy", "--to", "a"],
            &["copy", "--to", "a", "--json"],
            &["copy", "--json", "--to", "a"],
        ] {
            assert_eq!(analytics_words(&words(good)), Ok(()), "{good:?}");
        }
        for bad in [
            &[][..],
            &["report", "--json"],
            &["copy"],
            &["copy", "a"],
            &["copy", "--to"],
            &["copy", "--to", ""],
            &["copy", "--to", "--json"],
            &["copy", "--to", "-o"],
            &["copy", "--to", "a", "--yaml"],
            &["copy", "--to", "a", "--json", "now"],
            &["export", "a"],
            &["export", "a", "--json"],
            &["prune"],
            &["run", "7"],
        ] {
            assert_eq!(analytics_words(&words(bad)), Err(64), "{bad:?}");
        }
    }

    #[test]
    fn check_judges_a_person_s_command_and_passes_any_other() {
        assert_eq!(check(&words(&["tehas", "limit", "show"])), 64);
        assert_eq!(check(&words(&["tehas", "pause", "a", "b"])), 64);
        assert_eq!(check(&words(&["tehas", "pause", "a"])), 64);
        assert_eq!(check(&words(&["tehas", "pause", "--reason", "a"])), 0);
        assert_eq!(check(&words(&["tehas", "analytics"])), 64);
        assert_eq!(check(&words(&["tehas", "api", "--jsn"])), 64);
        assert_eq!(check(&words(&["tehas", "api", "--json"])), 0);
        assert_eq!(check(&words(&["tehas", "labels", "--force"])), 64);
        assert_eq!(check(&words(&["tehas", "panel", "show"])), 64);
        assert_eq!(check(&words(&["tehas", "panel", "--json"])), 0);
        assert_eq!(check(&words(&["tehas", "host-message", "show"])), 64);
        assert_eq!(
            check(&words(&["tehas", "host-message", "list", "--json"])),
            0
        );
        assert_eq!(check(&words(&["tehas", "resume", "7"])), 64);
        assert_eq!(check(&words(&["tehas", "drain", "--wait"])), 64);
        assert_eq!(check(&words(&["tehas", "drain", "--wait", "1"])), 0);
        assert_eq!(check(&words(&["tehas", "history", "--jsn"])), 64);
        assert_eq!(check(&words(&["tehas", "history", "--after", "1-1"])), 0);
        assert_eq!(check(&words(&["tehas", "export", "--after", "1-1"])), 64);
        assert_eq!(
            check(&words(&["tehas", "export", "--after", "1-1", "/a"])),
            0
        );
        assert_eq!(check(&words(&["tehas", "limit", "set", "--bad"])), 0);
        assert_eq!(check(&words(&["tehas", "state", "bogus"])), 0);
        assert_eq!(check(&words(&["tehasbot", "health"])), 0);
        assert_eq!(check(&words(&["tehasbot", "github-token", "--check"])), 0);
        assert_eq!(check(&words(&["tehasbot", "github-token", "--print"])), 64);
        assert_eq!(
            check(&words(&["tehasbot", "github-token", "--cycle", "--check"])),
            0
        );
        assert_eq!(check(&[]), 0);
    }

    #[test]
    fn check_judges_each_command_that_had_a_script_before_the_settings() {
        let says = |given: &[&str]| check(&words(given));
        for good in [
            &["tehas", "check-product"][..],
            &["tehas", "check-product", "--sandbox"],
            &["tehas", "init-product"],
            &["tehas", "init-product", "--dry-run"],
            &["tehasbot", "clean"],
            &["tehasbot", "clean", ""],
            &["tehasbot", "clean", "--dry-run"],
            &["tehasbot", "dispatch"],
            &["tehasbot", "dispatch", "--dry-run"],
            &["tehasbot", "dispatch", "--soon"],
            &["tehasbot", "preview"],
            &["tehasbot", "preview", ""],
            &["tehasbot", "preview", "--sweep"],
            &["tehasbot", "preview", "--sweep", "kept"],
            &["tehasbot", "protolab", "ignored"],
            &["tehasbot", "refresh", "--help"],
        ] {
            assert_eq!(says(good), 0, "{good:?}");
        }
        for bad in [
            &["tehas", "check-product", ""][..],
            &["tehas", "check-product", "--sandbox", "x"],
            &["tehas", "check-product", "--help"],
            &["tehas", "init-product", ""],
            &["tehas", "init-product", "--dry-run", "x"],
            &["tehasbot", "clean", "--bad"],
            &["tehasbot", "clean", "--dry-run", "x"],
            &["tehasbot", "clean", "", "x"],
            &["tehasbot", "dispatch", ""],
            &["tehasbot", "dispatch", "--dryrun"],
            &["tehasbot", "dispatch", "--dry-run", "--help"],
            &["tehasbot", "dispatch", "--help", "--dry-run"],
            &["tehasbot", "dispatch", "--soon", "--dry-run"],
            &["tehasbot", "dispatch", "--dry-run", "--soon"],
            &["tehasbot", "preview", "--help"],
        ] {
            assert_eq!(says(bad), 64, "{bad:?}");
        }
        for help in [
            &["tehasbot", "clean", "-h"][..],
            &["tehasbot", "clean", "--help"],
            &["tehasbot", "dispatch", "-h"],
            &["tehasbot", "dispatch", "--help"],
        ] {
            assert_eq!(says(help), ANSWERED, "{help:?}");
        }
    }

    #[test]
    fn a_command_names_the_settings_it_cannot_run_without() {
        assert_eq!(required(&words(&["tehas", "api"])), ["TEHAS_REPO"]);
        assert_eq!(required(&words(&["tehas", "labels"])), ["TEHAS_REPO"]);
        assert_eq!(required(&words(&["tehas", "history"])), ["TEHAS_REPO"]);
        assert_eq!(required(&words(&["tehas", "export"])), ["TEHAS_REPO"]);
        assert_eq!(
            required(&words(&["tehas", "resume"])),
            [
                "TEHAS_REPO",
                "TEHAS_CONTROL_ISSUE",
                "TEHAS_ALLOWED_LABELERS"
            ]
        );
        assert_eq!(
            required(&words(&["tehasbot", "status"])),
            required(&words(&["tehas", "resume"]))
        );
        for writer in [
            "start",
            "limit",
            "order",
            "panel",
            "host-message",
            "freeze",
            "thaw",
            "pause",
            "drain",
            "wake",
        ] {
            assert_eq!(
                required(&words(&["tehas", writer])),
                required(&words(&["tehas", "resume"])),
                "{writer}"
            );
        }
        assert!(required(&words(&["tehasbot", "health"])).is_empty());
        assert!(required(&words(&["tehasbot", "github-token"])).is_empty());
        for repo in [
            &["tehas", "check-product"][..],
            &["tehas", "init-product"],
            &["tehasbot", "preview"],
        ] {
            assert_eq!(required(&words(repo)), ["TEHAS_REPO"], "{repo:?}");
        }
        for all in [
            "clean",
            "dispatch",
            "refresh",
            "review",
            "outsource-proxy",
            "plugin-gate",
        ] {
            assert_eq!(
                required(&words(&["tehasbot", all])),
                INSTANCE_SETTINGS,
                "{all}"
            );
        }
        assert_eq!(
            required(&words(&["tehasbot", "protolab"])),
            ["TEHAS_REPO", "TEHAS_ALLOWED_LABELERS"]
        );
    }

    #[test]
    fn work_index_and_cycle_refuse_a_bad_word_before_the_settings() {
        for ok in [
            &["tehas", "work", "7"][..],
            &["tehas", "work", "7", "--restart"],
            &["tehas", "index"],
            &["tehas", "index", "--before-cycle"],
            &["tehasbot", "cycle", "12"],
        ] {
            assert_eq!(check(&words(ok)), 0, "{ok:?}");
        }
        for bad in [
            &["tehas", "work"][..],
            &["tehas", "work", "07"],
            &["tehas", "work", "-7"],
            &["tehas", "index", "--now"],
            &["tehas", "index", "--before-cycle", "x"],
            &["tehasbot", "cycle"],
            &["tehasbot", "cycle", ""],
            &["tehasbot", "cycle", "abc"],
            &["tehasbot", "cycle", "07"],
            &["tehasbot", "cycle", "0"],
            &["tehasbot", "cycle", "7", "8"],
            &["tehasbot", "cycle", "7x"],
            &["tehasbot", "cycle", "-7"],
        ] {
            assert_eq!(check(&words(bad)), 64, "{bad:?}");
        }
        assert_eq!(required(&words(&["tehas", "index"])), ["TEHAS_REPO"]);
        assert_eq!(
            required(&words(&["tehasbot", "cycle", "7"])),
            ["TEHAS_REPO"]
        );
        assert_eq!(
            required(&words(&["tehas", "work", "7"])),
            required(&words(&["tehas", "plan"]))
        );
    }

    #[test]
    fn every_command_has_a_text_that_names_it() {
        for command in [
            "analytics",
            "api",
            "check-product",
            "disk",
            "dispatch",
            "drain",
            "export",
            "history",
            "host-message",
            "index",
            "init-product",
            "labels",
            "limit",
            "panel",
            "pause",
            "resume",
            "setup",
            "state",
            "status",
            "update",
            "work",
        ] {
            let text = text(command).unwrap();
            assert!(text.contains(&format!("tehas {command}")), "{command}");
            assert!(text.ends_with('\n'), "{command}");
        }
        assert_eq!(text("bogus"), None);
        assert_eq!(usage(&words(&["bogus"])), 64);
        assert_eq!(usage(&[]), 64);
    }
}
