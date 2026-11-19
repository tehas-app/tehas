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
//! `tehasbot github-token` is what host/shim/gh and the poll run for the app's
//! token, and `console tehas dev-login` is what the sudo rule of `bootstrap.sh
//! dev` names: neither has a script of its own.
//!
//! `tehas refresh`, `tehas github check` and `tehas github writes` are the
//! public names of `tehasbot refresh`, `tehasbot github-token --check` and
//! `tehasbot github-writes`, which stay as aliases through 1.x.
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

/// The commands of `tehas` that run a script in libexec.
const TEHAS_SCRIPTS: [&str; 2] = ["setup", "update"];

fn script(which: &str, command: &str) -> Option<&'static str> {
    let scripts: &[&'static str] = match which {
        "tehas" => &TEHAS_SCRIPTS,
        _ => &[],
    };
    scripts.iter().copied().find(|s| *s == command)
}

/// The verbs of `tehas`. work and index are tehas-cycle's, which takes their
/// lock (src/run.rs).
const VERBS: [&str; 32] = [
    "work",
    "index",
    "dispatch",
    "plan",
    "dev",
    "check-product",
    "status",
    "disk",
    "state",
    "limit",
    "order",
    "start",
    "panel",
    "host-message",
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
    "freeze",
    "thaw",
    "mcp",
    "init-product",
    "refresh",
    "github",
    "outsource",
    "prototype",
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
        "status" | "disk" | "top" | "dev" => Go::Verb,
        "github" if words[1..] == ["check"] => Go::Run,
        "github" => Go::Verb,
        "mcp" if word(1) != Some("serve") => Go::Verb,
        c => script("tehas", c).map_or(Go::Run, Go::Script),
    }
}

fn tehasbot(words: &[String]) -> Go {
    match words.first().map(String::as_str) {
        None | Some("help" | "-h" | "--help") => Go::Usage,
        Some(
            "health" | "status" | "top" | "refresh" | "clean" | "preview" | "protolab" | "dispatch"
            | "review" | "github-token" | "cycle" | "outsource-proxy",
        ) => Go::Run,
        Some("mcp") if words.get(1).map(String::as_str) == Some("serve") => Go::Run,
        Some("mcp" | "github-writes") => Go::Verb,
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

/// The words `console run` is given. `tehas dispatch` is the poll the units
/// run as `tehasbot dispatch`, and `tehas refresh` and `tehas github check`
/// are `tehasbot refresh` and `tehasbot github-token --check`: one command
/// under two names.
fn as_run(mut lossy: Vec<String>) -> Vec<String> {
    let words: Vec<&str> = lossy.iter().skip(3).take(3).map(String::as_str).collect();
    let (to, upto) = match words[..] {
        ["tehas", "dispatch" | "refresh", ..] => (&["tehasbot"][..], 4),
        ["tehas", "github", "check"] => (&["tehasbot", "github-token", "--check"][..], 6),
        _ => return lossy,
    };
    lossy.splice(3..upto, to.iter().map(|w| w.to_string()));
    lossy
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
            run::main(&as_run(lossy), text)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

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
        for c in ["setup", "update"] {
            assert_eq!(t(&[c, "7"]), Go::Script(script("tehas", c).unwrap()));
        }
    }

    #[test]
    fn the_commands_of_the_old_wrappers_go_straight_to_the_console() {
        for c in ["dispatch", "plan", "check-product"] {
            assert_eq!(t(&[c, "7"]), Go::Run, "{c}");
        }
        assert_eq!(t(&["dev", "--issue", "7"]), Go::Verb);
        for c in ["dispatch", "review", "github-token"] {
            assert_eq!(b(&[c]), Go::Run, "{c}");
        }
        for c in [
            "dev-login",
            "github-token",
            "github-writes",
            "review",
            "improve",
        ] {
            assert_eq!(t(&[c]), Go::Unknown(c.into()), "{c}");
        }
        assert_eq!(b(&["dev-login"]), Go::Unknown("dev-login".into()));
    }

    #[test]
    fn status_disk_top_and_mcp_key_are_verbs_with_no_settings() {
        for w in [
            &["status", "--json"][..],
            &["disk", "--full"],
            &["top", "--once"],
            &["top"],
            &["mcp", "key"],
            &["dev"],
        ] {
            assert_eq!(t(w), Go::Verb, "{w:?}");
        }
    }

    #[test]
    fn the_other_verbs_run_under_the_settings() {
        for c in [
            "state",
            "limit",
            "order",
            "start",
            "panel",
            "host-message",
            "pause",
            "analytics",
            "api",
            "labels",
            "resume",
            "drain",
            "history",
            "export",
            "stop",
            "freeze",
            "thaw",
            "init-product",
            "work",
            "index",
            "refresh",
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
        assert_eq!(t(&["work", "--help", "7"]), Go::Run);
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
        for c in [
            "health",
            "status",
            "top",
            "refresh",
            "clean",
            "preview",
            "protolab",
            "dispatch",
            "review",
            "github-token",
            "cycle",
        ] {
            assert_eq!(b(&[c]), Go::Run, "{c}");
        }
        // The improvement loop is gone, and so is its command.
        assert_eq!(b(&["improve"]), Go::Unknown("improve".into()));
        assert_eq!(b(&["mcp", "serve"]), Go::Run);
        assert_eq!(b(&["outsource-proxy"]), Go::Run);
        assert_eq!(
            t(&["outsource-proxy"]),
            Go::Unknown("outsource-proxy".into())
        );
        assert_eq!(b(&["mcp", "key"]), Go::Verb);
        assert_eq!(b(&["github-writes"]), Go::Verb);
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
        assert!(seen >= 9, "{seen} units run tehasbot");
    }

    #[test]
    fn tehas_dispatch_runs_as_the_poll_of_tehasbot() {
        let run = |w: &[&str]| as_run(words(w));
        assert_eq!(
            run(&["/l", "1", "/bin/bash", "tehas", "dispatch", "--dry-run"]),
            words(&["/l", "1", "/bin/bash", "tehasbot", "dispatch", "--dry-run"])
        );
        let plan = ["/l", "1", "/bin/bash", "tehas", "plan", "7"];
        assert_eq!(run(&plan), words(&plan));
    }

    #[test]
    fn tehas_refresh_and_github_check_run_as_their_tehasbot_names() {
        let run = |w: &[&str]| as_run(words(w));
        assert_eq!(
            run(&["/l", "1", "/bin/bash", "tehas", "refresh", "--pr", "7"]),
            words(&["/l", "1", "/bin/bash", "tehasbot", "refresh", "--pr", "7"])
        );
        assert_eq!(
            run(&["/l", "1", "/bin/bash", "tehas", "github", "check"]),
            words(&[
                "/l",
                "1",
                "/bin/bash",
                "tehasbot",
                "github-token",
                "--check"
            ])
        );
        let mint = [
            "/l",
            "1",
            "/bin/bash",
            "tehasbot",
            "github-token",
            "--check",
        ];
        assert_eq!(run(&mint), words(&mint));
    }

    #[test]
    fn tehas_github_check_runs_under_the_settings_and_the_rest_is_a_verb() {
        assert_eq!(t(&["github", "check"]), Go::Run);
        for w in [
            &["github", "writes"][..],
            &["github"],
            &["github", "check", "--cycle"],
            &["github", "bogus"],
        ] {
            assert_eq!(t(w), Go::Verb, "{w:?}");
        }
        assert_eq!(t(&["github", "--help"]), Go::Text("github".into()));
        assert_eq!(t(&["refresh", "--help"]), Go::Text("refresh".into()));
    }

    #[test]
    fn the_tehasbot_names_stay_as_aliases() {
        assert_eq!(b(&["refresh", "--pr", "7"]), Go::Run);
        assert_eq!(b(&["github-token", "--check"]), Go::Run);
        assert_eq!(b(&["github-writes"]), Go::Verb);
    }

    #[test]
    fn the_old_wrappers_texts_are_the_consoles() {
        for c in ["dispatch", "plan", "dev", "check-product"] {
            assert!(commands::text(c).is_some(), "{c}");
        }
    }

    #[test]
    fn a_header_is_the_comment_after_the_first_line_without_its_marks() {
        let file =
            "#!/usr/bin/env bash\n# tehas work: one cycle.\n#\n#  indented\nset -u\n# not this\n";
        assert_eq!(header(file), "tehas work: one cycle.\n\n indented\n");
    }

    /// The `--flags` of a line. `--help`, which every command takes, is no
    /// flag of one.
    fn flags_in(line: &str) -> BTreeSet<String> {
        let mut flags = BTreeSet::new();
        for (i, _) in line.match_indices("--") {
            let flag: String = line[i + 2..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect();
            if flag.starts_with(|c: char| c.is_ascii_lowercase()) && flag != "help" {
                flags.insert(format!("--{flag}"));
            }
        }
        flags
    }

    /// The subcommands a synopsis names after `tehas <command>`: none for the
    /// command itself, `[list]` both the command and `list`. A word after two
    /// spaces says what the line does, and `[options]` stands for flags.
    fn subs_of(rest: &str) -> Vec<Option<String>> {
        let word = |w: &str| {
            let w = w.strip_suffix(':').unwrap_or(w);
            let ok = w.starts_with(|c: char| c.is_ascii_lowercase())
                && w.chars().all(|c| c.is_ascii_lowercase() || c == '-')
                && w != "options";
            ok.then(|| w.to_string())
        };
        let first = match rest.strip_prefix(' ') {
            Some(r) if !r.starts_with(' ') => r.split_whitespace().next().unwrap_or(""),
            _ => "",
        };
        if let Some(w) = first.strip_prefix('[').and_then(|w| w.strip_suffix(']')) {
            if let Some(sub) = word(w) {
                return vec![None, Some(sub)];
            }
        }
        vec![word(first)]
    }

    /// The rows a command's text names, each with its flags. A line that
    /// starts with `tehas <command>`, alone or after `Usage:`, names the
    /// command or one subcommand, and the lines of the usage block under it
    /// add to that row. A line that starts with a flag and says what it is
    /// after two spaces adds its flag to each row that already names it, or to
    /// the command itself when none does.
    fn usage_rows(command: &str, text: &str) -> BTreeMap<Option<String>, BTreeSet<String>> {
        let head = format!("tehas {command}");
        let mut rows = BTreeMap::from([(None, BTreeSet::new())]);
        let mut options = BTreeSet::new();
        let mut in_usage = false;
        let mut current: Vec<Option<String>> = vec![None];
        for line in text.lines() {
            let l = line.trim_start();
            in_usage = (in_usage || l.starts_with("Usage:")) && !l.is_empty();
            let synopsis = l.trim_start_matches("Usage:").trim_start();
            let rest = synopsis
                .strip_prefix(&head)
                .filter(|r| r.is_empty() || r.starts_with([' ', ':']));
            if let Some(rest) = rest {
                current = subs_of(rest);
            } else if l.starts_with("--") && l.contains("  ") {
                options.extend(flags_in(l));
                continue;
            } else if synopsis.starts_with("tehas") {
                current = Vec::new();
            } else if !in_usage {
                continue;
            }
            for key in &current {
                rows.entry(key.clone()).or_default().extend(flags_in(l));
            }
        }
        for flag in options {
            let mut named = rows.values().filter(|f| f.contains(&flag)).peekable();
            if named.peek().is_none() {
                rows.get_mut(&None).unwrap().insert(flag);
            }
        }
        rows
    }

    /// A row of api/commands.txt.
    #[derive(Debug)]
    struct Row {
        command: String,
        sub: Option<String>,
        flags: BTreeSet<String>,
        building: Option<String>,
        internal: BTreeSet<String>,
    }

    fn pinned() -> Vec<Row> {
        let file = concat!(env!("CARGO_MANIFEST_DIR"), "/../api/commands.txt");
        let text = fs::read_to_string(file).unwrap();
        let lines = text
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'));
        lines
            .map(|l| {
                let mut w = l.split_whitespace().peekable();
                let command = w.next().unwrap().to_string();
                let sub = w
                    .next_if(|s| !s.starts_with('-') && !s.contains('='))
                    .map(str::to_string);
                let mut row = Row {
                    command,
                    sub,
                    flags: BTreeSet::new(),
                    building: None,
                    internal: BTreeSet::new(),
                };
                for word in w {
                    if let Some(f) = word.strip_prefix("building=") {
                        row.building = Some(f.to_string());
                    } else if let Some(list) = word.strip_prefix("internal=") {
                        for f in list.split(',') {
                            assert!(row.internal.insert(f.into()), "{l}");
                        }
                    } else if word.starts_with("--") {
                        assert!(row.flags.insert(word.into()), "{l}");
                    } else {
                        panic!("api/commands.txt: {word} in {l:?} is no flag");
                    }
                }
                assert!(
                    row.internal.is_subset(&row.flags),
                    "api/commands.txt: {l:?} marks a flag internal that it does not list"
                );
                row
            })
            .collect()
    }

    fn row(command: &str, sub: &Option<String>, flags: &BTreeSet<String>) -> String {
        let mut words = vec![command];
        words.extend(sub.as_deref());
        words.extend(flags.iter().map(String::as_str));
        words.join(" ")
    }

    #[test]
    fn the_pinned_command_list_is_every_command_of_tehas() {
        let pinned = pinned();
        let keys: Vec<(&str, Option<&str>)> = pinned
            .iter()
            .map(|r| (r.command.as_str(), r.sub.as_deref()))
            .collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(keys, sorted, "api/commands.txt: one line per row, in order");
        let mut listed: Vec<&str> = keys.iter().map(|(c, _)| *c).collect();
        listed.dedup();
        let mut known: Vec<&str> = VERBS.to_vec();
        known.extend(TEHAS_SCRIPTS);
        known.push("help");
        known.sort_unstable();
        assert_eq!(listed, known, "api/commands.txt against VERBS");
    }

    #[test]
    fn the_pinned_rows_are_those_each_command_text_names() {
        let pinned = pinned();
        let mut commands: Vec<&str> = pinned.iter().map(|r| r.command.as_str()).collect();
        commands.dedup();
        let mut wrong = Vec::new();
        for command in commands.into_iter().filter(|c| *c != "help") {
            let text = commands::text(command).unwrap_or_else(|| panic!("no text: {command}"));
            let named: Vec<String> = usage_rows(command, text)
                .iter()
                .map(|(sub, flags)| row(command, sub, flags))
                .collect();
            let listed: Vec<String> = pinned
                .iter()
                .filter(|r| r.command == command)
                .map(|r| row(&r.command, &r.sub, &r.flags))
                .collect();
            if named != listed {
                wrong.push(format!(
                    "{}\n  (the file: {})",
                    named.join("\n"),
                    listed.join(" | ")
                ));
            }
        }
        assert!(
            wrong.is_empty(),
            "api/commands.txt should say, but for its marks:\n{}",
            wrong.join("\n")
        );
    }

    #[test]
    fn the_usage_names_only_pinned_commands_subcommands_and_flags() {
        let pinned = pinned();
        for line in TEHAS.lines().filter_map(|l| l.strip_prefix("  tehas ")) {
            let line = line.split("  ").next().unwrap();
            let command = line.split_whitespace().next().unwrap();
            let rows: Vec<&Row> = pinned.iter().filter(|r| r.command == command).collect();
            assert!(
                !rows.is_empty(),
                "tehas {command} is not in api/commands.txt"
            );
            for flag in flags_in(line) {
                assert!(
                    rows.iter().any(|r| r.flags.contains(&flag)),
                    "tehas {command} {flag} is not in api/commands.txt"
                );
            }
            let subs: Vec<&str> = rows.iter().filter_map(|r| r.sub.as_deref()).collect();
            if subs.is_empty() {
                continue;
            }
            let words = line.split_whitespace().skip(1);
            let words = words.take_while(|w| !w.trim_start_matches('[').starts_with("--"));
            let words = words.take_while(|w| !w.contains('<'));
            for word in words.flat_map(|w| w.split('|')) {
                let word = word.trim_matches(|c| c == '[' || c == ']' || c == '.');
                if !word.is_empty() && word != "-" {
                    assert!(
                        subs.contains(&word),
                        "tehas {command} {word} is no pinned row"
                    );
                }
            }
        }
    }

    #[test]
    fn a_command_of_a_building_feature_is_marked_and_only_while_it_builds() {
        use tehas_core::features::{Stage, FEATURES};
        let pinned = pinned();
        let mut marked: Vec<(&str, &str)> = Vec::new();
        for r in &pinned {
            let Some(name) = r.building.as_deref() else {
                continue;
            };
            let feature = FEATURES.iter().find(|f| f.name == name);
            let feature = feature.unwrap_or_else(|| panic!("{}: no feature {name}", r.command));
            assert_eq!(
                feature.stage,
                Stage::Building,
                "{}: {name} has shipped",
                r.command
            );
            marked.push((r.command.as_str(), name));
        }
        marked.dedup();
        assert_eq!(
            marked,
            [("outsource", "outsource"), ("prototype", "prototype-file")]
        );
        for (command, name) in marked {
            for r in pinned.iter().filter(|r| r.command == command) {
                assert_eq!(r.building.as_deref(), Some(name), "{r:?}");
            }
        }
    }

    /// Each public `--json` row of api/commands.txt and the schemas in api/ of
    /// the objects it prints.
    const JSON_SCHEMAS: [(&str, Option<&str>, &[&str]); 19] = [
        ("analytics", Some("copy"), &["analytics-copy"]),
        ("api", None, &["api"]),
        ("disk", None, &["disk", "disk-cycles"]),
        ("drain", None, &["drain"]),
        ("export", None, &["export"]),
        ("freeze", None, &["freeze"]),
        ("history", None, &["history"]),
        ("host-message", None, &["host-messages"]),
        ("host-message", Some("list"), &["host-messages"]),
        ("limit", None, &["limit"]),
        ("order", None, &["order"]),
        ("panel", None, &["panel"]),
        ("refresh", None, &["refresh"]),
        ("start", None, &["start"]),
        ("state", Some("check"), &["state-check"]),
        ("state", Some("copy"), &["state-copy"]),
        ("state", Some("load"), &["state-load"]),
        ("status", None, &["status"]),
        ("thaw", None, &["freeze"]),
    ];

    #[test]
    fn every_public_json_row_has_a_schema() {
        let public: Vec<(String, Option<String>)> = pinned()
            .into_iter()
            .filter(|r| {
                r.flags.contains("--json") && !r.internal.contains("--json") && r.building.is_none()
            })
            .map(|r| (r.command, r.sub))
            .collect();
        let table: Vec<(String, Option<String>)> = JSON_SCHEMAS
            .iter()
            .map(|(c, s, _)| (c.to_string(), s.map(str::to_string)))
            .collect();
        assert_eq!(public, table, "api/commands.txt against JSON_SCHEMAS");
        let api = concat!(env!("CARGO_MANIFEST_DIR"), "/../api");
        let mut named: BTreeSet<&str> = JSON_SCHEMAS
            .iter()
            .flat_map(|(_, _, s)| s.iter().copied())
            .collect();
        for name in &named {
            let file = format!("{api}/{name}.schema.json");
            assert!(fs::metadata(&file).is_ok(), "no {file}");
        }
        // Files the docs server serves or the release carries, no command's
        // output.
        named.extend(["host-bridge", "settings-rules"]);
        let mut found = Vec::new();
        for e in fs::read_dir(api).unwrap() {
            let name = e.unwrap().file_name().to_string_lossy().into_owned();
            if let Some(name) = name.strip_suffix(".schema.json") {
                found.push(name.to_string());
            }
        }
        found.sort();
        let named: Vec<String> = named.into_iter().map(str::to_string).collect();
        assert_eq!(found, named, "the schemas in api/ against JSON_SCHEMAS");
    }

    #[test]
    fn the_internal_flags_are_those_of_the_dispatcher_the_cleaner_and_the_screen() {
        let marked: Vec<String> = pinned()
            .iter()
            .flat_map(|r| {
                r.internal
                    .iter()
                    .map(|f| row(&r.command, &r.sub, &[f.clone()].into()))
            })
            .collect();
        let plan = [
            "--answer",
            "--brief",
            "--comment",
            "--keep",
            "--kind",
            "--out",
            "--session",
        ];
        let mut want: Vec<String> = vec!["disk --publish".into(), "index --before-cycle".into()];
        want.extend(plan.iter().map(|f| format!("plan {f}")));
        want.push("top --json".into());
        assert_eq!(marked, want);
        let page = concat!(env!("CARGO_MANIFEST_DIR"), "/../host/docs/contract.md");
        let page = fs::read_to_string(page).unwrap();
        for named in [
            "`tehas plan`",
            "`tehas index --before-cycle`",
            "`tehas disk --publish`",
            "`tehas top --json`",
        ] {
            assert!(
                page.contains(named),
                "host/docs/contract.md lists {named} as internal"
            );
        }
    }

    #[test]
    fn the_rows_of_a_text_are_its_synopsis_lines() {
        let text = "tehas x: what.\n\nUsage: tehas x [--json]\n       tehas x   does x\n       tehas x set <id> --to F\n          [--at T]\n       tehas x clear\n       tehas x -h|--help\n\n  --to   a file\n  --only  alone\n     --skip, prose\nprose names --other.\ntehas y set --far\n";
        let got: Vec<String> = usage_rows("x", text)
            .iter()
            .map(|(sub, flags)| row("x", sub, flags))
            .collect();
        assert_eq!(got, ["x --json --only", "x clear", "x set --at --to"]);
        let got = usage_rows("disk", "tehas disk [--full]: how full\n");
        assert_eq!(got[&None].iter().collect::<Vec<_>>(), ["--full"]);
        assert_eq!(usage_rows("dis", "tehas disk [--full]\n")[&None].len(), 0);
        let got: Vec<String> = usage_rows("m", "Usage: tehas m [list] [--json]\n")
            .iter()
            .map(|(sub, flags)| row("m", sub, flags))
            .collect();
        assert_eq!(got, ["m --json", "m list --json"]);
        let got = usage_rows("p", "tehas p file: a title\ntehas p [options]\n");
        assert_eq!(
            got.keys().collect::<Vec<_>>(),
            [&None, &Some("file".into())]
        );
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
