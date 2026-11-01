//! The measure of an issue body as a command, and the ledger of what approved
//! starts measured. The counts are `tehas_core::size`'s.
//!
//!   console size measure
//!     stdin: one body. stdout: one compact JSON object, one line, the four
//!     counts. A body of nothing is four zeros.
//!
//!   console size record <issue> <ledger>
//!     stdin: the body of an approved start. Appends one line to <ledger>:
//!     {at, issue, files, tests, deliverables, done_when}. `tehas work` is the
//!     only writer; `console analytics report` reads it.
//!
//! A line that cannot be written is one word on stdout and exit 1: `measure`
//! or `ledger`, the step that gave up. The measure is a record of the start,
//! never a reason to stop one.
//!
//! <ledger> is created owner-only and is written only when it is a plain file
//! and no link. It keeps the last 500 lines: it is one file per instance, not
//! one per issue. Both bounds of the measure come from the environment.
//!
//! Exit: 0 done, 1 no line was written or the ledger could not be trimmed, 64
//! a usage error.

use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use tehas_core::size::{measure, Size, MAX_BYTES, MAX_LINES};
use tehas_core::time;

const KEPT: usize = 500;
const USAGE: &str = "usage: console size measure
       console size record <issue> <ledger>";

pub fn main(given: &[String]) -> u8 {
    match given {
        [verb] if verb == "measure" => {
            let size = read_body().map(|b| measured(&b)).unwrap_or_default();
            println!("{{{}}}", size.fields());
            0
        }
        [verb, issue, ledger] if verb == "record" => {
            let line = match (read_body(), issue.parse::<u64>()) {
                (Ok(body), Ok(issue)) => line_of(&time::iso(time::now()), issue, &measured(&body)),
                _ => return gave_up("measure"),
            };
            match record(&line, Path::new(ledger)) {
                Ok(()) => 0,
                Err(()) => gave_up("ledger"),
            }
        }
        _ => {
            eprintln!("{USAGE}");
            64
        }
    }
}

fn gave_up(word: &str) -> u8 {
    println!("{word}");
    1
}

fn bound(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn max_bytes() -> usize {
    bound("TEHAS_SIZE_MAX_BYTES", MAX_BYTES)
}

/// Only what the measure will look at is read of a paste of anything.
fn read_body() -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    io::stdin()
        .take(max_bytes() as u64)
        .read_to_end(&mut body)?;
    Ok(body)
}

fn measured(body: &[u8]) -> Size {
    measure(body, max_bytes(), bound("TEHAS_SIZE_MAX_LINES", MAX_LINES))
}

fn line_of(at: &str, issue: u64, size: &Size) -> String {
    format!(r#"{{"at":"{at}","issue":{issue},{}}}"#, size.fields())
}

fn lines_in(bytes: &[u8]) -> usize {
    bytes.iter().filter(|b| **b == b'\n').count()
}

fn opened(ledger: &Path) -> io::Result<fs::File> {
    OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(ledger)
}

/// Appends and trims. A file this call made is removed when the line did not
/// land: a ledger it could not add to keeps what it holds.
fn record(line: &str, ledger: &Path) -> Result<(), ()> {
    let made = match fs::symlink_metadata(ledger) {
        Ok(m) if m.file_type().is_file() => false,
        Ok(_) => return Err(()),
        Err(_) => {
            opened(ledger).map_err(|_| ())?;
            true
        }
    };
    let before = fs::read(ledger).map_or(0, |b| lines_in(&b));
    let _ = opened(ledger).and_then(|mut f| writeln!(f, "{line}"));
    // Judged by what the file ends with and how many lines it holds, not by
    // what the write reported.
    let held = fs::read(ledger).unwrap_or_default();
    let ours = format!("\n{line}\n");
    let ends = held.ends_with(ours.as_bytes()) || held == ours.as_bytes()[1..];
    if !ends || lines_in(&held) != before + 1 {
        if made {
            let _ = fs::remove_file(ledger);
        }
        return Err(());
    }
    trim(&held, ledger)
}

/// A side file renamed into place. A trim that cannot be made leaves the
/// ledger a line longer, which is no loss of a start.
fn trim(held: &[u8], ledger: &Path) -> Result<(), ()> {
    let lines = lines_in(held);
    if lines <= KEPT {
        return Ok(());
    }
    let from = held
        .iter()
        .enumerate()
        .filter(|(_, b)| **b == b'\n')
        .nth(lines - KEPT - 1)
        .map_or(0, |(i, _)| i + 1);
    let mut side = ledger.as_os_str().to_owned();
    side.push(".new");
    fs::write(&side, &held[from..]).map_err(|_| ())?;
    fs::rename(&side, ledger).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::path::PathBuf;

    struct Dir(PathBuf);

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn dir(name: &str) -> Dir {
        let d = std::env::temp_dir().join(format!("tehas-size-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        Dir(d)
    }

    const LINE: &str = "{\"at\":\"t\"}";

    #[test]
    fn the_line_keeps_its_keys_in_order() {
        let size = Size {
            files: 2,
            tests: 1,
            deliverables: 3,
            done_when: 4,
        };
        assert_eq!(
            line_of("2026-11-01T00:00:00Z", 7, &size),
            r#"{"at":"2026-11-01T00:00:00Z","issue":7,"files":2,"tests":1,"deliverables":3,"done_when":4}"#
        );
    }

    #[test]
    fn a_first_line_makes_the_ledger_owner_only() {
        let d = dir("first");
        let p = d.0.join("sizes.jsonl");
        record(LINE, &p).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), format!("{LINE}\n"));
        let mode = fs::metadata(&p).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn a_ledger_that_is_a_link_or_a_directory_is_not_written() {
        let d = dir("link");
        let target = d.0.join("target");
        fs::write(&target, "kept\n").unwrap();
        let link = d.0.join("link");
        symlink(&target, &link).unwrap();
        assert!(record(LINE, &link).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "kept\n");
        symlink(d.0.join("nothing"), d.0.join("dangling")).unwrap();
        assert!(record(LINE, &d.0.join("dangling")).is_err());
        assert!(!d.0.join("nothing").exists());
        assert!(record(LINE, &d.0).is_err());
    }

    #[test]
    fn a_path_with_no_directory_is_no_ledger() {
        let d = dir("nodir");
        assert!(record(LINE, &d.0.join("no/sizes.jsonl")).is_err());
    }

    #[test]
    fn a_file_with_no_last_newline_takes_the_line_and_says_it_did_not() {
        let d = dir("torn");
        let p = d.0.join("sizes.jsonl");
        fs::write(&p, "torn").unwrap();
        assert!(record(LINE, &p).is_err());
        assert_eq!(fs::read_to_string(&p).unwrap(), format!("torn{LINE}\n"));
    }

    #[test]
    fn the_ledger_keeps_its_last_five_hundred_lines() {
        let d = dir("trim");
        let p = d.0.join("sizes.jsonl");
        let old: String = (1..=500).map(|i| format!("{i}\n")).collect();
        fs::write(&p, old).unwrap();
        record(LINE, &p).unwrap();
        let text = fs::read_to_string(&p).unwrap();
        assert_eq!(lines_in(text.as_bytes()), 500);
        assert!(text.starts_with("2\n3\n"));
        assert!(text.ends_with(&format!("500\n{LINE}\n")));
        assert!(!d.0.join("sizes.jsonl.new").exists());
    }

    #[test]
    fn five_hundred_lines_are_left_as_they_are() {
        let d = dir("exact");
        let p = d.0.join("sizes.jsonl");
        let old: String = (1..=499).map(|i| format!("{i}\n")).collect();
        fs::write(&p, old).unwrap();
        record(LINE, &p).unwrap();
        let text = fs::read_to_string(&p).unwrap();
        assert_eq!(lines_in(text.as_bytes()), 500);
        assert!(text.starts_with("1\n"));
    }

    #[test]
    fn a_trim_that_cannot_be_made_is_said_and_the_line_stays() {
        let d = dir("stuck");
        let p = d.0.join("sizes.jsonl");
        fs::create_dir(d.0.join("sizes.jsonl.new")).unwrap();
        let old: String = (1..=500).map(|i| format!("{i}\n")).collect();
        fs::write(&p, old).unwrap();
        assert!(record(LINE, &p).is_err());
        assert_eq!(lines_in(&fs::read(&p).unwrap()), 501);
    }
}
