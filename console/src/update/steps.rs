//! The follow-up steps an update finishes after install.sh has copied files.
//!
//! install.sh prints the steps its changed sources call for, one per line, as
//! "next: <step line>", and records the same lines in state/update-pending as
//! it copies each file. A step line is checked here before anything runs it, so
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

/// `console update steps ids|units|valid <line>`: the tables, one name per
/// line, and whether a line is a step: 0 yes, 1 no.
pub fn verb(given: &[String]) -> u8 {
    let words: Vec<&str> = given.iter().map(String::as_str).collect();
    let table = match words[..] {
        ["ids"] => IDS,
        ["units"] => UNITS,
        ["valid", line] => return u8::from(!valid(line)),
        _ => {
            super::tell("console update steps: usage: ids | units | valid <line>");
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
