//! The front of bin/tehas and bin/tehasbot: which command a word names, the
//! text of it, and what runs it.
//!
//!   console front <libexec dir> <euid> <bash> tehas|tehasbot [words]
//!
//! The two commands are a few lines of bash that find this program and exec
//! it. `<euid>` and `<bash>` are the shell's, as for `console run`. A command
//! is a script in <libexec dir>, run by exec with the remaining words, or a
//! verb of this program: in this process, under the settings through `console
//! run` (src/run.rs) where it needs them. Nothing here loads the settings.
//!
//! Exit: the command's; 64 a command this does not know.

use crate::{commands, energy, run};
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::os::unix::process::CommandExt;
use std::process::Command;

const TEHAS: &str = include_str!("usage/tehas.txt");
const TEHASBOT: &str = include_str!("usage/tehasbot.txt");

/// What a command line comes to.
#[derive(Debug, PartialEq, Eq)]
enum Go {
    /// The usage of the command itself, on stdout.
    Usage,
    /// The text of one command.
    Text(String),
    /// Not a command: 64, with the usage on stderr.
    Unknown(String),
    /// The script of that name in libexec, with the words after the first.
    Script(&'static str),
    /// `console tehas|tehasbot <words>`, with no settings.
    Verb,
    /// `console run`: the words under the settings.
    Run,
    /// `console energy` with the words after the first.
    Energy,
}

fn script(which: &str, command: &str) -> Option<&'static str> {
    let scripts: &[&'static str] = match which {
        "tehas" => &[
            "dispatch",
            "work",
            "plan",
            "dev",
            "index",
            "setup",
            "update",
            "check-product",
        ],
        _ => &["dispatch", "cycle", "improve", "review", "github-token"],
    };
    scripts.iter().copied().find(|s| *s == command)
}

/// The verbs of `tehas`.
const VERBS: [&str; 17] = [
    "status",
    "disk",
    "state",
    "limit",
    "panel",
    "pause",
    "analytics",
    "top",
    "api",
    "labels",
    "resume",
    "drain",
    "history",
    "export",
    "stop",
    "mcp",
    "init-product",
];

fn tehas(words: &[String]) -> Go {
    let word = |i: usize| words.get(i).map(String::as_str);
    match word(0) {
        None | Some("help" | "-h" | "--help") => {
            return match word(1) {
                None | Some("help") => Go::Usage,
                Some(c) => Go::Text(c.to_string()),
            }
        }
        Some(c) if !VERBS.contains(&c) && script("tehas", c).is_none() => {
            return Go::Unknown(c.to_string())
        }
        _ => {}
    }
    let command = words[0].as_str();
    if words.len() == 2 && matches!(word(1), Some("-h" | "--help")) {
        return Go::Text(command.to_string());
    }
    match command {
        "status" | "disk" | "top" => Go::Verb,
        "mcp" if word(1) != Some("serve") => Go::Verb,
        c => script("tehas", c).map_or(Go::Run, Go::Script),
    }
}

fn tehasbot(words: &[String]) -> Go {
    match words.first().map(String::as_str) {
        None | Some("help" | "-h" | "--help") => Go::Usage,
        Some("health" | "status" | "top" | "refresh" | "clean" | "preview" | "protolab") => Go::Run,
        Some("mcp") if words.get(1).map(String::as_str) == Some("serve") => Go::Run,
        Some("mcp") => Go::Verb,
        Some("energy") => Go::Energy,
        Some(c) => script("tehasbot", c).map_or_else(|| Go::Unknown(c.to_string()), Go::Script),
    }
}

/// The text of a script is its header: every line from the second to the
/// first that is no comment, without its "# ".
fn header(text: &str) -> String {
    text.lines()
        .skip(1)
        .take_while(|l| l.starts_with('#'))
        .map(|l| {
            let l = &l[1..];
            format!("{}\n", l.strip_prefix(' ').unwrap_or(l))
        })
        .collect()
}

// A reader that took what it wanted and left is no fault.
fn say(text: &str) {
    let _ = io::stdout().write_all(text.as_bytes());
}

fn unknown(which: &str, command: &str, usage: &str) -> u8 {
    eprint!("{which}: unknown command: {command}\n{usage}");
    64
}

fn text(libexec: &str, command: &str) -> u8 {
    if !VERBS.contains(&command) && script("tehas", command).is_none() {
        return unknown("tehas", command, TEHAS);
    }
    if let Some(t) = commands::text(command) {
        say(t);
        return 0;
    }
    match script("tehas", command).map(|s| fs::read_to_string(format!("{libexec}/{s}"))) {
        Some(Ok(file)) => {
            say(&header(&file));
            0
        }
        Some(Err(e)) => {
            eprintln!("tehas {command}: {libexec}/{command}: {e}");
            1
        }
        None => 64,
    }
}

pub fn main(given: &[OsString]) -> u8 {
    let lossy: Vec<String> = given
        .iter()
        .map(|w| w.to_string_lossy().into_owned())
        .collect();
    let [libexec, _, _, which, words @ ..] = &lossy[..] else {
        eprintln!("usage: console front <libexec dir> <euid> <bash> tehas|tehasbot [words]");
        return 64;
    };
    let (go, usage) = match which.as_str() {
        "tehas" => (tehas(words), TEHAS),
        "tehasbot" => (tehasbot(words), TEHASBOT),
        _ => {
            eprintln!("console front: tehas or tehasbot, not {which}");
            return 64;
        }
    };
    let rest = || words.get(1..).unwrap_or_default();
    match go {
        Go::Usage => {
            say(usage);
            0
        }
        Go::Text(command) => text(libexec, &command),
        Go::Unknown(command) => unknown(which, &command, usage),
        Go::Script(name) => {
            let file = format!("{libexec}/{name}");
            let failed = Command::new(&file).args(&given[5..]).exec();
            eprintln!("{which}: {file}: {failed}");
            126
        }
        Go::Verb if which == "tehas" => commands::tehas(words),
        Go::Verb => commands::tehasbot(words),
        Go::Energy => energy::main(rest()),
        Go::Run => {
            let text = given.iter().all(|w| w.to_str().is_some());
            run::main(&lossy, text)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(w: &[&str]) -> Vec<String> {
        w.iter().map(|w| w.to_string()).collect()
    }

    fn t(w: &[&str]) -> Go {
        tehas(&words(w))
    }

    fn b(w: &[&str]) -> Go {
        tehasbot(&words(w))
    }

    #[test]
    fn tehas_runs_the_script_of_each_command_that_has_one() {
        for c in [
            "dispatch",
            "work",
            "plan",
            "dev",
            "index",
            "setup",
            "update",
            "check-product",
        ] {
            assert_eq!(t(&[c, "7"]), Go::Script(script("tehas", c).unwrap()));
        }
    }

    #[test]
    fn status_disk_top_and_mcp_key_are_verbs_with_no_settings() {
        for w in [
            &["status", "--json"][..],
            &["disk", "--full"],
            &["top", "--once"],
            &["top"],
            &["mcp", "key"],
        ] {
            assert_eq!(t(w), Go::Verb, "{w:?}");
        }
    }

    #[test]
    fn the_other_verbs_run_under_the_settings() {
        for c in [
            "state",
            "limit",
            "panel",
            "pause",
            "analytics",
            "api",
            "labels",
            "resume",
            "drain",
            "history",
            "export",
            "stop",
            "init-product",
        ] {
            assert_eq!(t(&[c]), Go::Run, "{c}");
        }
        assert_eq!(t(&["mcp", "serve"]), Go::Run);
    }

    #[test]
    fn alone_help_and_its_flags_are_the_usage_and_help_names_a_text() {
        for w in [&[][..], &["help"], &["-h"], &["--help"], &["help", "help"]] {
            assert_eq!(t(w), Go::Usage, "{w:?}");
        }
        assert_eq!(t(&["help", "work"]), Go::Text("work".into()));
        assert_eq!(t(&["help", "bogus"]), Go::Text("bogus".into()));
    }

    #[test]
    fn a_command_and_one_help_flag_is_its_text_and_another_word_is_its_own() {
        for f in ["-h", "--help"] {
            assert_eq!(t(&["limit", f]), Go::Text("limit".into()));
            assert_eq!(t(&["work", f]), Go::Text("work".into()));
        }
        assert_eq!(t(&["work", "--help", "7"]), Go::Script("work"));
        assert_eq!(t(&["limit", "--help", "7"]), Go::Run);
    }

    #[test]
    fn what_only_the_units_run_is_no_tehas_command() {
        for c in [
            "health",
            "energy",
            "protolab",
            "preview",
            "improve",
            "summary",
            "console",
            "cycle",
            "bogus",
            "--dry-run",
            "7",
            "",
        ] {
            assert_eq!(t(&[c]), Go::Unknown(c.into()), "{c}");
        }
    }

    #[test]
    fn tehasbot_routes_each_command() {
        for c in ["dispatch", "cycle", "improve", "review", "github-token"] {
            assert_eq!(b(&[c, "7"]), Go::Script(script("tehasbot", c).unwrap()));
        }
        for c in [
            "health", "status", "top", "refresh", "clean", "preview", "protolab",
        ] {
            assert_eq!(b(&[c]), Go::Run, "{c}");
        }
        assert_eq!(b(&["mcp", "serve"]), Go::Run);
        assert_eq!(b(&["mcp", "key"]), Go::Verb);
        assert_eq!(b(&["energy", "--every"]), Go::Energy);
        for w in [&[][..], &["help"], &["-h"], &["--help"]] {
            assert_eq!(b(w), Go::Usage, "{w:?}");
        }
        for c in [
            "work", "plan", "dev", "index", "update", "labels", "summary", "bogus", "",
        ] {
            assert_eq!(b(&[c]), Go::Unknown(c.into()), "{c}");
        }
    }

    #[test]
    fn every_unit_runs_a_command_tehasbot_knows() {
        let units = concat!(env!("CARGO_MANIFEST_DIR"), "/../host/systemd");
        let mut seen = 0;
        for unit in fs::read_dir(units).unwrap() {
            let text = fs::read_to_string(unit.unwrap().path()).unwrap();
            for line in text.lines().filter(|l| l.starts_with("ExecStart=")) {
                let Some((_, after)) = line.split_once("bin/tehasbot ") else {
                    continue;
                };
                let w: Vec<&str> = after.trim_end_matches('"').split_whitespace().collect();
                assert!(!matches!(b(&w), Go::Unknown(_) | Go::Usage), "{line}");
                seen += 1;
            }
        }
        assert!(seen >= 10, "{seen} units run tehasbot");
    }

    #[test]
    fn a_header_is_the_comment_after_the_first_line_without_its_marks() {
        let file =
            "#!/usr/bin/env bash\n# tehas work: one cycle.\n#\n#  indented\nset -u\n# not this\n";
        assert_eq!(header(file), "tehas work: one cycle.\n\n indented\n");
    }

    #[test]
    fn every_command_the_usage_lists_is_one_this_knows() {
        for (which, usage) in [("tehas", TEHAS), ("tehasbot", TEHASBOT)] {
            let prefix = format!("  {which} ");
            for line in usage.lines().filter_map(|l| l.strip_prefix(&prefix)) {
                let c = line.split_whitespace().next().unwrap();
                let go = if which == "tehas" { t(&[c]) } else { b(&[c]) };
                assert!(!matches!(go, Go::Unknown(_)), "{which} {c}");
            }
        }
    }
}
