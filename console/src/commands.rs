//! The commands of `bin/tehas` and `bin/tehasbot` that are verbs of this
//! program: nothing of them is left in bash but a line of a table.
//!
//!   console tehas status [--json]
//!   console tehas disk [...]
//!   console tehas state pack|unpack ...
//!   console tehas limit [--json] | set ... | clear
//!   console tehas pause [reason]
//!   console tehas analytics report | export <file> [--json]
//!   console tehas top [--once | --json] [--every <seconds>]
//!   console tehas api [--json]
//!   console tehas labels
//!   console tehas resume
//!   console tehas drain [--wait SECONDS] [--json]
//!   console tehas stop <issue> [--keep-workspace]
//!   console tehas|tehasbot mcp serve|key
//!   console tehasbot health
//!   console tehasbot top [--every]
//!
//! status, disk, top and mcp key need no settings. The rest run under
//! `libexec/with-settings`, which first asks `console check` with the same
//! words: an argument that cannot be right is 64 with the usage, whatever the
//! settings hold, and only then are they loaded. `console needs` with those
//! words then names the settings the command cannot run without, for
//! `tehas_require`, so the rules of a setting stay in `lib/config.sh`.
//!
//!   console usage <command>
//!
//! The text of `tehas help <command>`, from src/usage/<command>.txt.
//!
//! Exit: the status of the command; 64 usage.

use crate::{api, disk, flags, health, labels, mcp, state, status, stop, top};
use std::env;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

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

/// `tehas pause` as the words of `console flags`. The reason is one argument;
/// the flag makes it one line.
fn pause_words(given: &[String]) -> Result<Vec<String>, u8> {
    match given {
        [] => Ok(strings(&["pause", "set"], &[])),
        [reason] if reason.is_empty() => Ok(strings(&["pause", "set"], &[])),
        [_] => Ok(strings(&["pause", "set", "--reason"], given)),
        _ => {
            eprintln!("usage: tehas pause [reason]");
            Err(64)
        }
    }
}

fn analytics_words(given: &[String]) -> Result<(), u8> {
    let file_ok = |f: &String| !f.is_empty() && !f.starts_with('-');
    let good = match given {
        [verb] => verb == "report",
        [verb, file] => verb == "export" && file_ok(file),
        [verb, file, flag] => verb == "export" && file_ok(file) && flag == "--json",
        _ => false,
    };
    if !good {
        eprintln!("usage: tehas analytics report | export <file> [--json]");
        return Err(64);
    }
    Ok(())
}

/// The analytics program takes over from here, with nothing on its standard
/// input, and its exit statuses are the command's.
fn analytics(given: &[String]) -> u8 {
    if let Err(rc) = analytics_words(given) {
        return rc;
    }
    if env::var("TEHAS_ANALYTICS").as_deref() != Ok("on") {
        eprintln!(
            "tehas analytics: analytics is off on this host (TEHAS_ANALYTICS=off); nothing is recorded"
        );
        return 69;
    }
    let home = PathBuf::from(env::var_os("TEHAS_HOME").unwrap_or_default());
    let bin = home.join("libexec/analytics");
    let not_installed = || {
        eprintln!(
            "tehas analytics: {} is not installed; install.sh builds it",
            bin.display()
        );
        69
    };
    if !bin
        .metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    {
        return not_installed();
    }
    let _ = Command::new(&bin)
        .args(given)
        .stdin(Stdio::null())
        .env("TEHAS_HOME", &home)
        .exec();
    not_installed()
}

pub fn tehas(given: &[String]) -> u8 {
    let Some((command, rest)) = given.split_first() else {
        eprintln!("console tehas: which command?");
        return 64;
    };
    let flag = |words: Result<Vec<String>, u8>| match words {
        Ok(words) => flags::main(&words),
        Err(rc) => rc,
    };
    match command.as_str() {
        "status" => status::print(rest),
        "disk" => disk::main(rest),
        "state" => state::main(rest),
        "limit" => flag(limit_words(rest)),
        "pause" => flag(pause_words(rest)),
        "analytics" => analytics(rest),
        "top" => top::view(rest),
        "api" => api::main(rest),
        "labels" => labels::main(rest),
        "resume" => flags::resume(rest),
        "drain" => flags::drain(rest),
        "stop" => stop::main(rest),
        "mcp" => mcp::main(rest),
        _ => {
            eprintln!("console tehas: no such command: {command}");
            64
        }
    }
}

pub fn tehasbot(given: &[String]) -> u8 {
    match given.first().map(String::as_str) {
        Some("health") => health::loaded(),
        Some("top") => top::publish(&given[1..]),
        Some("mcp") => mcp::main(&given[1..]),
        _ => {
            eprintln!("console tehasbot: usage: health | top [--every] | mcp");
            64
        }
    }
}

/// What `check` ends with when it was the whole answer, as a help text met
/// among the arguments is: `libexec/with-settings` then ends 0 and loads nothing.
const ANSWERED: u8 = 99;

/// What can be said of the arguments before the settings are loaded. A
/// command with no check here passes.
pub fn check(given: &[String]) -> u8 {
    let words: Vec<&str> = given.iter().take(2).map(String::as_str).collect();
    let rest = given.get(2..).unwrap_or_default();
    let said = match words[..] {
        ["tehas", "limit"] => limit_words(rest).map(|_| ()),
        ["tehas", "pause"] => pause_words(rest).map(|_| ()),
        ["tehas", "analytics"] => analytics_words(rest),
        ["tehas", "api"] => api::words(rest).map(|_| ()),
        ["tehas", "labels"] => labels::words(rest),
        ["tehas", "resume"] => flags::resume_words(rest),
        ["tehas", "drain"] => flags::drain_words(rest).map(|_| ()),
        ["tehas", "stop"] => match stop::parse(rest) {
            Ok(Some(_)) => Ok(()),
            Ok(None) => Err(ANSWERED),
            Err(rc) => Err(rc),
        },
        _ => Ok(()),
    };
    said.err().unwrap_or(0)
}

/// The settings a command cannot run without, on one line.
fn required(given: &[String]) -> &'static [&'static str] {
    let words: Vec<&str> = given.iter().take(2).map(String::as_str).collect();
    match words[..] {
        ["tehas", "api" | "labels"] => &["TEHAS_REPO"],
        ["tehas", "resume"] => &[
            "TEHAS_REPO",
            "TEHAS_CONTROL_ISSUE",
            "TEHAS_ALLOWED_LABELERS",
        ],
        _ => &[],
    }
}

pub fn needs(given: &[String]) -> u8 {
    println!("{}", required(given).join(" "));
    0
}

fn text(command: &str) -> Option<&'static str> {
    Some(match command {
        "analytics" => include_str!("usage/analytics.txt"),
        "api" => include_str!("usage/api.txt"),
        "disk" => include_str!("usage/disk.txt"),
        "drain" => include_str!("usage/drain.txt"),
        "labels" => include_str!("usage/labels.txt"),
        "limit" => include_str!("usage/limit.txt"),
        "mcp" => include_str!("usage/mcp.txt"),
        "pause" => include_str!("usage/pause.txt"),
        "resume" => include_str!("usage/resume.txt"),
        "state" => include_str!("usage/state.txt"),
        "status" => include_str!("usage/status.txt"),
        "stop" => stop::USAGE,
        "top" => include_str!("usage/top.txt"),
        _ => return None,
    })
}

pub fn usage(given: &[String]) -> u8 {
    match given {
        [command] if text(command).is_some() => {
            print!("{}", text(command).unwrap_or_default());
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
    fn pause_takes_one_reason_and_an_empty_one_is_none() {
        assert_eq!(pause_words(&[]), Ok(words(&["pause", "set"])));
        assert_eq!(pause_words(&words(&[""])), Ok(words(&["pause", "set"])));
        assert_eq!(
            pause_words(&words(&["a b"])),
            Ok(words(&["pause", "set", "--reason", "a b"]))
        );
        assert_eq!(pause_words(&words(&["a", "b"])), Err(64));
        assert_eq!(pause_words(&words(&["-h", "x"])), Err(64));
    }

    #[test]
    fn analytics_takes_report_or_export() {
        for good in [
            &["report"][..],
            &["export", "a"],
            &["export", "a", "--json"],
        ] {
            assert_eq!(analytics_words(&words(good)), Ok(()), "{good:?}");
        }
        for bad in [
            &[][..],
            &["report", "--json"],
            &["export"],
            &["export", ""],
            &["export", "--json"],
            &["export", "-o", "x"],
            &["export", "a", "--yaml"],
            &["export", "a", "--json", "now"],
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
        assert_eq!(check(&words(&["tehas", "analytics"])), 64);
        assert_eq!(check(&words(&["tehas", "api", "--jsn"])), 64);
        assert_eq!(check(&words(&["tehas", "api", "--json"])), 0);
        assert_eq!(check(&words(&["tehas", "labels", "--force"])), 64);
        assert_eq!(check(&words(&["tehas", "resume", "7"])), 64);
        assert_eq!(check(&words(&["tehas", "drain", "--wait"])), 64);
        assert_eq!(check(&words(&["tehas", "drain", "--wait", "1"])), 0);
        assert_eq!(check(&words(&["tehas", "limit", "set", "--bad"])), 0);
        assert_eq!(check(&words(&["tehas", "state", "bogus"])), 0);
        assert_eq!(check(&words(&["tehasbot", "health"])), 0);
        assert_eq!(check(&[]), 0);
    }

    #[test]
    fn a_command_names_the_settings_it_cannot_run_without() {
        assert_eq!(required(&words(&["tehas", "api"])), ["TEHAS_REPO"]);
        assert_eq!(required(&words(&["tehas", "labels"])), ["TEHAS_REPO"]);
        assert_eq!(
            required(&words(&["tehas", "resume"])),
            [
                "TEHAS_REPO",
                "TEHAS_CONTROL_ISSUE",
                "TEHAS_ALLOWED_LABELERS"
            ]
        );
        assert!(required(&words(&["tehas", "drain"])).is_empty());
        assert!(required(&words(&["tehasbot", "health"])).is_empty());
    }

    #[test]
    fn every_command_has_a_text_that_names_it() {
        for command in [
            "analytics",
            "api",
            "disk",
            "drain",
            "labels",
            "limit",
            "pause",
            "resume",
            "state",
            "status",
        ] {
            let text = text(command).unwrap();
            assert!(text.contains(&format!("tehas {command}")), "{command}");
            assert!(text.ends_with('\n'), "{command}");
        }
        assert_eq!(text("work"), None);
        assert_eq!(usage(&words(&["work"])), 64);
        assert_eq!(usage(&[]), 64);
    }
}
