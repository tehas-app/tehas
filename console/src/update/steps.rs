//! The follow-up steps an update finishes after install.sh has copied files.
//!
//! One place names them, because two commands read the list. install.sh asks
//! here which steps its changed sources call for, prints them one per line as
//! "next: <step line>", and records the same lines in state/update-pending
//! before it copies each file. tehas update reads those lines and runs each
//! step as its own command. A step line is checked here before anything runs it, so
//! what a step may be asked for, its arguments included, is fixed by this file
//! and not by whoever wrote the line. The id has to be one this file knows.
//!
//!   units <unit>...   systemd reloads its unit files, then restarts these
//!                     units, and only where one is already active
//!   sandbox           rebuild the sandbox image
//!   docs <flag>       renew the docs container where it runs; the one flag
//!                     accepted is --force-recreate and it is required, because
//!                     Caddy reads its config when the process starts and a
//!                     changed Caddyfile never reaches a container left as it
//!                     was. A bare "docs" would run the weaker action and still
//!                     be marked done
//!   edge <flag>       the same for the edge router. A line recorded as
//!                     "protolab <flag>" by an earlier version means this step
//!   labels            tehas labels makes the labels of this version; it takes
//!                     no argument, and neither does the line
//!
//! An id this file does not define is run by nobody, and it stays in the record
//! so the host keeps reporting it until a command that knows it runs. A step
//! that was asked for and did not run stays owed until it has run.

use super::{fault, say};
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The units install.sh names when a unit file changed: the timers and services
/// tehas runs, not only the changed one, since an update restarts the ones that
/// were already active. A oneshot service whose timer is named here is not
/// restarted, because that would run its job off schedule; the reload reaches it.
pub(crate) const UNITS: &[&str] = &[
    "tehas-dispatch.timer",
    "tehas-energy.service",
    "tehas-top.service",
    "tehas-mcp.service",
    "tehas-preview.timer",
    "tehas-preview-sweep.timer",
    "tehas-protolab.timer",
    "tehas-improve.timer",
    "tehas-refresh.timer",
    "tehas-review.timer",
    "tehas-clean.timer",
];

/// The ids, in the order they run: systemd first, because a unit file is not in
/// effect until it is reloaded and its unit restarted, and the containers last.
pub(crate) const IDS: &[&str] = &["units", "sandbox", "docs", "edge", "labels"];

/// The flag both compose servers take: their service definition changes only
/// when the file they mount does, and docker compose leaves a running container
/// with an unchanged definition exactly as it was.
const RECREATE: &str = "--force-recreate";

/// The id a token means. The Protolab's server became the edge router.
pub(crate) fn step_id(token: &str) -> &str {
    if token == "protolab" {
        "edge"
    } else {
        token
    }
}

/// The words of a step line, as `read -a` splits them.
pub(crate) fn words(line: &str) -> Vec<&str> {
    line.split([' ', '\t']).filter(|w| !w.is_empty()).collect()
}

/// A systemd unit name: one word that ends in a unit suffix. A token that is
/// not one never reaches systemctl.
fn is_unit(word: &str) -> bool {
    if word.starts_with('-') {
        return false;
    }
    let Some((stem, suffix)) = word.rsplit_once('.') else {
        return false;
    };
    matches!(suffix, "service" | "timer" | "socket")
        && !stem.is_empty()
        && stem
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:@+-".contains(&b))
}

/// The steps these changed sources call for, in the order they run.
pub(crate) fn steps_for(sources: &[String]) -> Vec<String> {
    let any = |test: &dyn Fn(&str) -> bool| sources.iter().any(|s| test(s));
    let mut steps = Vec::new();
    // bin/tehas-energy, libexec/energy and libexec/top-publish are loops of
    // before the console: a running one moves on only when restarted. A running
    // service keeps the environment env.sh gave it at its start.
    if any(&|s| {
        s.starts_with("systemd/")
            || matches!(
                s,
                "bin/tehas-energy" | "libexec/energy" | "libexec/top-publish" | "env.sh"
            )
    }) {
        steps.push(format!("units {}", UNITS.join(" ")));
    } else if any(&|s| s.ends_with("/tehas-mcp")) {
        // The MCP server's container mounts its binary: a new one reaches it only
        // when the unit starts the container again.
        steps.push("units tehas-mcp.service".to_string());
    }
    if any(&|s| s.starts_with("sandbox/") || s == "versions.env") {
        steps.push("sandbox".to_string());
    }
    if any(&|s| s.starts_with("edge/docs.")) {
        steps.push(format!("docs {RECREATE}"));
    }
    if any(&|s| s.starts_with("edge/router.") || s.starts_with("edge/protolab.")) {
        steps.push(format!("edge {RECREATE}"));
    }
    // A release that adds a label ends with that label in place on GitHub.
    if any(&|s| s == "labels.tsv") {
        steps.push("labels".to_string());
    }
    steps
}

/// Is this a step this file knows, carrying only arguments that id accepts? A
/// line that fails here is not run: the command reading it says so out loud.
pub(crate) fn valid(line: &str) -> bool {
    let argv = words(line);
    let Some((first, rest)) = argv.split_first() else {
        return false;
    };
    match step_id(first) {
        // And only the units this version installs: a step line is not a way to
        // name some other unit of the host's for an update to restart.
        "units" => !rest.is_empty() && rest.iter().all(|u| is_unit(u) && UNITS.contains(u)),
        "sandbox" | "labels" => rest.is_empty(),
        "docs" | "edge" => rest == [RECREATE],
        _ => false,
    }
}

fn line_id(line: &str) -> &str {
    step_id(line.split(' ').next().unwrap_or(""))
}

/// The same lines in the order they run. Lines whose id this file does not know
/// keep their order at the end: nobody runs them, but they stay visible.
pub(crate) fn sorted(lines: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for id in IDS {
        out.extend(lines.iter().filter(|l| line_id(l) == *id).cloned());
    }
    out.extend(lines.iter().filter(|l| !IDS.contains(&line_id(l))).cloned());
    out
}

/// Where the unfinished steps of a run are kept. install.sh's report of what
/// changed is spent by the run that made the copies, so without this file a run
/// that stopped partway would leave the host half updated with nothing owed.
pub(crate) fn record_file(home: &Path) -> PathBuf {
    home.join("state/update-pending")
}

/// The lines of the record, without blank ones. No record is an empty answer. A
/// record that exists but cannot be read is the other case: it holds steps
/// somebody owed, and answering "nothing is owed" would let the next write
/// replace it, so a failed read is a failure.
pub(crate) fn read_record(home: &Path) -> Result<Vec<String>, ()> {
    let file = record_file(home);
    let mut text = Vec::new();
    match fs::File::open(&file) {
        Ok(mut f) => {
            if f.read_to_end(&mut text).is_err() {
                fault(&format!(
                    "could not read the follow-up record {}",
                    file.display()
                ));
                return Err(());
            }
        }
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(_) => {
            fault(&format!(
                "cannot read the follow-up record {}",
                file.display()
            ));
            return Err(());
        }
    }
    Ok(String::from_utf8_lossy(&text)
        .split('\n')
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
}

fn temp_beside(file: &Path) -> Option<(PathBuf, fs::File)> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    for n in 0..64u32 {
        let mut name = file.as_os_str().to_owned();
        name.push(format!(".{:x}{n:x}", nanos ^ std::process::id()));
        let tmp = PathBuf::from(name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)
        {
            Ok(f) => return Some((tmp, f)),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(_) => return None,
        }
    }
    None
}

/// Replace the record with these lines: what is owed after this run, in the
/// order they run. No lines means no record. It writes through a temporary file
/// and moves that over, because the record is the only memory a half-finished
/// run leaves behind, and a truncation that died halfway would erase it.
pub(crate) fn write_record(home: &Path, lines: &[String]) -> Result<(), ()> {
    let file = record_file(home);
    let dir = file.parent().unwrap_or(home);
    if fs::create_dir_all(dir).is_err() {
        fault(&format!("could not create {}", dir.display()));
        return Err(());
    }
    if lines.is_empty() {
        let _ = fs::remove_file(&file);
        return Ok(());
    }
    let Some((tmp, mut f)) = temp_beside(&file) else {
        fault(&format!(
            "could not make a temporary file beside {}",
            file.display()
        ));
        return Err(());
    };
    let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
    if f.write_all(text.as_bytes()).is_err() || fs::rename(&tmp, &file).is_err() {
        let _ = fs::remove_file(&tmp);
        fault(&format!(
            "could not record the follow-ups owed in {}",
            file.display()
        ));
        return Err(());
    }
    Ok(())
}

fn is_units(line: &str) -> bool {
    line.split(' ').next() == Some("units")
}

fn units_in(line: &str) -> impl Iterator<Item = &str> {
    line.strip_prefix("units ")
        .unwrap_or(line)
        .split_whitespace()
}

fn names_all_of(have: &str, line: &str) -> bool {
    units_in(line).all(|unit| units_in(have).any(|u| u == unit))
}

/// A units step whose units another units step names too is done by that one.
/// Of two that name the same units, the first stays.
fn without_covered(merged: Vec<String>) -> Vec<String> {
    let keep: Vec<bool> = (0..merged.len())
        .map(|i| {
            let line = &merged[i];
            !is_units(line)
                || !merged.iter().enumerate().any(|(j, have)| {
                    is_units(have)
                        && have != line
                        && names_all_of(have, line)
                        && (j < i || !names_all_of(line, have))
                })
        })
        .collect();
    merged
        .into_iter()
        .zip(keep)
        .filter_map(|(line, keep)| keep.then_some(line))
        .collect()
}

/// Add these steps to the record, keeping it free of duplicates and in the order
/// they run. install.sh calls this before each copy, so a run that dies partway
/// still leaves what its copies asked for. What is already there has to be read
/// first: merging into an unreadable record would overwrite it. A record that
/// already holds these lines in this order is left as it is.
pub(crate) fn record_lines(home: &Path, lines: &[String]) -> Result<(), ()> {
    if lines.is_empty() {
        return Ok(());
    }
    let record = read_record(home)?;
    let mut merged = record.clone();
    for line in lines {
        if !line.is_empty() && !merged.contains(line) {
            merged.push(line.clone());
        }
    }
    let owed = sorted(&without_covered(merged));
    if owed == record {
        return Ok(());
    }
    write_record(home, &owed)
}

/// `console update steps ids|units|valid <line>|for <source>...|record <source>...`:
/// the tables, one name per line, whether a line is a step (0 yes, 1 no), the
/// steps changed sources call for, and those steps added to the record. Exit:
/// 1 the record could not be read or written.
pub fn verb(given: &[String]) -> u8 {
    if let Some((word, sources)) = given.split_first() {
        match word.as_str() {
            "for" => {
                for step in steps_for(sources) {
                    say(&step);
                }
                return 0;
            }
            "record" => {
                let host = match super::host_or_refuse() {
                    Ok(host) => host,
                    Err(rc) => return rc,
                };
                return u8::from(record_lines(&host.home, &steps_for(sources)).is_err());
            }
            _ => {}
        }
    }
    let words: Vec<&str> = given.iter().map(String::as_str).collect();
    let table = match words[..] {
        ["ids"] => IDS,
        ["units"] => UNITS,
        ["valid", line] => return u8::from(!valid(line)),
        _ => {
            super::tell(
                "console update steps: usage: ids | units | valid <line> | for <source>... | record <source>...",
            );
            return 64;
        }
    };
    for name in table {
        say(name);
    }
    0
}

#[cfg(test)]
mod tests {
    use super::super::testing::dir;
    use super::*;

    fn lines(given: &[&str]) -> Vec<String> {
        given.iter().map(|l| l.to_string()).collect()
    }

    fn asked(sources: &[&str]) -> Vec<String> {
        steps_for(&lines(sources))
    }

    #[test]
    fn the_changed_sources_ask_for_the_steps_systemd_first() {
        let units = format!("units {}", UNITS.join(" "));
        assert_eq!(
            asked(&[
                "systemd/tehas-dispatch.timer",
                "sandbox/Dockerfile",
                "versions.env",
                "edge/docs.compose.yaml",
                "edge/router.compose.yaml",
                "labels.tsv",
                "libexec/dispatch",
            ]),
            [
                units.as_str(),
                "sandbox",
                "docs --force-recreate",
                "edge --force-recreate",
                "labels"
            ]
        );
    }

    #[test]
    fn a_new_mcp_server_restarts_its_unit_and_only_it() {
        let one = ["units tehas-mcp.service"];
        assert_eq!(
            asked(&["/src/console/target/x86_64-unknown-linux-musl/release/tehas-mcp"]),
            one
        );
        assert_eq!(asked(&["libexec/tehas-mcp"]), one);
        assert_eq!(
            asked(&["systemd/tehas-top.service", "libexec/tehas-mcp"]),
            [format!("units {}", UNITS.join(" "))]
        );
        assert!(asked(&["libexec/dispatch"]).is_empty());
    }

    #[test]
    fn a_change_to_one_server_asks_for_that_server_only() {
        assert_eq!(asked(&["edge/docs.Caddyfile"]), ["docs --force-recreate"]);
        assert_eq!(
            asked(&["edge/router.compose.yaml"]),
            ["edge --force-recreate"]
        );
        // The retired files of the Protolab's server call for the same step, once.
        assert_eq!(
            asked(&["edge/protolab.Caddyfile", "edge/protolab.compose.yaml"]),
            ["edge --force-recreate"]
        );
    }

    #[test]
    fn the_loops_of_before_and_env_sh_ask_for_the_units() {
        for source in [
            "bin/tehas-energy",
            "libexec/energy",
            "libexec/top-publish",
            "env.sh",
        ] {
            assert_eq!(
                asked(&[source]),
                [format!("units {}", UNITS.join(" "))],
                "{source}"
            );
        }
    }

    #[test]
    fn a_change_that_needs_no_follow_up_asks_for_none() {
        assert!(asked(&["libexec/dispatch", "docs/host.md", "lib/config.sh"]).is_empty());
        assert!(asked(&[
            "labels.tsv-test",
            "old-labels.tsv",
            "lib/labels.sh",
            "libexec/console",
            "libexec/labels"
        ])
        .is_empty());
        assert!(asked(&[]).is_empty());
    }

    #[test]
    fn every_step_it_asks_for_is_one_it_accepts() {
        for step in asked(&[
            "systemd/tehas-dispatch.service",
            "sandbox/build.sh",
            "versions.env",
            "edge/docs.compose.yaml",
            "edge/router.Caddyfile",
            "labels.tsv",
            "libexec/tehas-mcp",
        ]) {
            assert!(valid(&step), "{step}");
            assert!(IDS.contains(&step.split(' ').next().unwrap()), "{step}");
        }
    }

    #[test]
    fn recording_keeps_each_step_once_in_the_order_they_run() {
        let d = dir("record-order");
        record_lines(
            &d.0,
            &lines(&["docs --force-recreate", "units tehas-dispatch.timer"]),
        )
        .unwrap();
        record_lines(&d.0, &lines(&["sandbox", "docs --force-recreate"])).unwrap();
        assert_eq!(
            read_record(&d.0).unwrap(),
            lines(&[
                "units tehas-dispatch.timer",
                "sandbox",
                "docs --force-recreate"
            ])
        );
    }

    #[test]
    fn a_record_that_holds_the_steps_is_left_as_it_is_one_that_does_not_is_rewritten() {
        use std::os::unix::fs::MetadataExt;
        let d = dir("record-same");
        let inode = || fs::metadata(record_file(&d.0)).unwrap().ino();
        record_lines(&d.0, &lines(&["units tehas-dispatch.timer", "sandbox"])).unwrap();
        let a = inode();
        record_lines(&d.0, &lines(&["sandbox", "units tehas-dispatch.timer"])).unwrap();
        assert_eq!(a, inode());
        record_lines(&d.0, &lines(&["docs --force-recreate"])).unwrap();
        assert_ne!(a, inode());
        // In another order it is sorted.
        fs::write(record_file(&d.0), "sandbox\nunits tehas-dispatch.timer\n").unwrap();
        let b = inode();
        record_lines(&d.0, &lines(&["sandbox"])).unwrap();
        assert_ne!(b, inode());
        assert_eq!(
            read_record(&d.0).unwrap(),
            lines(&["units tehas-dispatch.timer", "sandbox"])
        );
    }

    #[test]
    fn a_units_step_another_one_covers_is_not_recorded_twice() {
        let d = dir("record-units");
        record_lines(&d.0, &lines(&["units tehas-mcp.service"])).unwrap();
        record_lines(
            &d.0,
            &lines(&["units tehas-dispatch.timer tehas-mcp.service", "sandbox"]),
        )
        .unwrap();
        record_lines(&d.0, &lines(&["units tehas-mcp.service"])).unwrap();
        assert_eq!(
            read_record(&d.0).unwrap(),
            lines(&["units tehas-dispatch.timer tehas-mcp.service", "sandbox"])
        );
        let d = dir("record-units-2");
        let two = lines(&["units tehas-mcp.service", "units tehas-top.service"]);
        record_lines(&d.0, &two).unwrap();
        assert_eq!(read_record(&d.0).unwrap(), two);
    }

    #[test]
    fn two_units_steps_of_the_same_units_leave_one() {
        let d = dir("record-units-same");
        let two = lines(&["units a.timer b.timer", "units b.timer a.timer"]);
        record_lines(&d.0, &two).unwrap();
        assert_eq!(
            read_record(&d.0).unwrap(),
            lines(&["units a.timer b.timer"])
        );
    }

    #[test]
    fn a_recorded_step_no_version_here_knows_stays_recorded_at_the_end() {
        let d = dir("record-unknown");
        record_lines(&d.0, &lines(&["renew-dashboard --now"])).unwrap();
        record_lines(&d.0, &lines(&["sandbox"])).unwrap();
        assert_eq!(
            read_record(&d.0).unwrap(),
            lines(&["sandbox", "renew-dashboard --now"])
        );
    }

    #[test]
    fn recording_makes_the_state_directory() {
        let d = dir("record-dir-made");
        record_lines(&d.0.join("not-yet"), &lines(&["sandbox"])).unwrap();
        assert_eq!(
            fs::read_to_string(record_file(&d.0.join("not-yet"))).unwrap(),
            "sandbox\n"
        );
    }

    #[test]
    fn merging_into_a_record_it_cannot_read_stops_instead_of_replacing_it() {
        let d = dir("record-unreadable");
        fs::create_dir_all(record_file(&d.0)).unwrap();
        assert!(record_lines(&d.0, &lines(&["sandbox"])).is_err());
        assert!(record_file(&d.0).is_dir());
    }

    #[test]
    fn a_line_that_is_not_a_step_this_file_knows_is_refused() {
        for bad in [
            "",
            "rm -rf something",
            "units",
            "units --help tehas-dispatch.timer",
            "units dispatch",
            "units tehas-dispatch.timers",
            "units ../etc/passwd",
            "units gnome-shell.service",
            "units systemd-journald.service",
            "docs",
            "edge",
            "protolab",
            "docs restart",
            "docs up -d",
            "docs --always-recreate",
            "docs --force-recreate up",
            "docs --force-recreate --force-recreate",
            "edge --always-recreate",
            "sandbox --now",
            "labels x",
            "labels --force",
        ] {
            assert!(!valid(bad), "accepted: [{bad}]");
        }
    }

    #[test]
    fn the_lines_install_sh_writes_are_steps() {
        let all = format!("units {}", UNITS.join(" "));
        for good in [
            all.as_str(),
            "units tehas-mcp.service",
            "sandbox",
            "docs --force-recreate",
            "edge --force-recreate",
            "protolab --force-recreate",
            "labels",
        ] {
            assert!(valid(good), "refused: [{good}]");
        }
    }

    #[test]
    fn a_unit_name_is_one_word_with_a_unit_suffix() {
        assert!(is_unit("tehas-energy@1.service"));
        assert!(is_unit("socket-2.socket"));
        assert!(!is_unit("--force-recreate"));
        assert!(!is_unit("-x.service"));
        assert!(!is_unit(".service"));
        assert!(!is_unit("a b.service"));
        assert!(!is_unit("a.path"));
    }

    #[test]
    fn one_function_says_a_protolab_line_means_the_edge_step() {
        assert_eq!(step_id("protolab"), "edge");
        assert_eq!(step_id("edge"), "edge");
        assert_eq!(step_id("docs"), "docs");
        assert_eq!(step_id("a-step-newer-than-this"), "a-step-newer-than-this");
    }

    #[test]
    fn the_steps_run_in_the_order_of_the_ids_whoever_wrote_the_lines() {
        assert_eq!(
            sorted(&lines(&[
                "edge --force-recreate",
                "docs --force-recreate",
                "units tehas-dispatch.timer",
                "sandbox"
            ])),
            lines(&[
                "units tehas-dispatch.timer",
                "sandbox",
                "docs --force-recreate",
                "edge --force-recreate"
            ])
        );
        assert_eq!(
            sorted(&lines(&[
                "protolab --force-recreate",
                "sandbox",
                "docs --force-recreate"
            ])),
            lines(&[
                "sandbox",
                "docs --force-recreate",
                "protolab --force-recreate"
            ])
        );
    }

    #[test]
    fn a_line_no_step_knows_keeps_its_place_at_the_end() {
        assert_eq!(
            sorted(&lines(&[
                "b-newer",
                "docs --force-recreate",
                "a-newer --now"
            ])),
            lines(&["docs --force-recreate", "b-newer", "a-newer --now"])
        );
    }

    #[test]
    fn the_record_reads_back_as_written_and_no_lines_mean_no_record() {
        let d = dir("record");
        assert_eq!(read_record(&d.0).unwrap(), Vec::<String>::new());
        let owed = lines(&["sandbox", "docs --force-recreate"]);
        write_record(&d.0, &owed).unwrap();
        assert_eq!(
            fs::read_to_string(record_file(&d.0)).unwrap(),
            "sandbox\ndocs --force-recreate\n"
        );
        assert_eq!(read_record(&d.0).unwrap(), owed);
        write_record(&d.0, &[]).unwrap();
        assert_eq!(fs::read_dir(d.0.join("state")).unwrap().count(), 0);
    }

    #[test]
    fn a_line_is_read_without_a_closing_newline_and_blank_ones_are_not() {
        let d = dir("record-newline");
        d.file("state/update-pending", "sandbox\n\nlabels");
        assert_eq!(read_record(&d.0).unwrap(), lines(&["sandbox", "labels"]));
    }

    #[test]
    fn a_record_that_cannot_be_read_is_a_failure_not_an_empty_answer() {
        let d = dir("record-dir");
        fs::create_dir_all(record_file(&d.0)).unwrap();
        assert!(read_record(&d.0).is_err());
    }

    #[test]
    fn the_record_is_replaced_whole_and_leaves_no_temporary_file() {
        let d = dir("record-whole");
        write_record(&d.0, &lines(&["sandbox"])).unwrap();
        write_record(&d.0, &lines(&["labels"])).unwrap();
        assert_eq!(fs::read_dir(d.0.join("state")).unwrap().count(), 1);
        assert_eq!(fs::read_to_string(record_file(&d.0)).unwrap(), "labels\n");
    }
}
