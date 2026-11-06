//! TEHAS_READ_ONLY (#608): an instance that reads its product on GitHub and
//! writes nothing to it. Every write the host would make asks [`refuse`]
//! first: the gh shim's `console gh` for each gh call, and the host's own git
//! pushes. Under the setting the answer is yes, the write is not made, and one
//! line says what it would have been. Nothing else decides it.
//!
//! The labels that hold the queue on GitHub are never written, so a start
//! leaves a record of its own here, `state/read-only/<issue>.json`, and the
//! poll starts nothing more on an issue that has one.

use std::fs::{self, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

pub const NAME: &str = "TEHAS_READ_ONLY";

/// The setting as the environment holds it: on only at exactly 1, as the load
/// leaves a switch.
pub fn on() -> bool {
    std::env::var(NAME).is_ok_and(|v| v == "1")
}

/// The words a refused write leaves, on standard error and in the log.
pub fn line(what: &str) -> String {
    format!("read-only: would {}", what.replace(['\t', '\n'], " "))
}

/// The log of the writes a read-only host did not make.
pub fn log_path(home: &Path) -> PathBuf {
    home.join("state/read-only.log")
}

/// The directory of the local records of starts.
pub fn records(home: &Path) -> PathBuf {
    home.join("state/read-only")
}

fn home() -> Option<PathBuf> {
    std::env::var_os("TEHAS_HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

/// With the setting on: say that `what` is not done, keep the line in
/// `state/read-only.log` of $TEHAS_HOME, and answer true; the caller then
/// skips the write and goes on as if it was made. With it off, false and
/// nothing said. A log that cannot be written changes nothing of the answer.
pub fn refuse(what: &str) -> bool {
    if !on() {
        return false;
    }
    let l = line(what);
    let mut err = io::stderr();
    let _ = writeln!(err, "{l}");
    if let Some(h) = home() {
        if append(
            &log_path(&h),
            &format!("{}\t{l}\n", crate::time::iso(crate::time::now())),
        )
        .is_err()
        {
            let _ = writeln!(
                err,
                "read-only: the log {} could not be written",
                log_path(&h).display()
            );
        }
    }
    true
}

fn append(log: &Path, text: &str) -> io::Result<()> {
    if let Some(dir) = log.parent() {
        fs::create_dir_all(dir)?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)?
        .write_all(text.as_bytes())
}

/// The record of a start on `issue`: what started (`cycle`, `plan`, ...) and
/// when. Written once; a second start of the same issue keeps the first.
pub fn record(home: &Path, issue: u64, how: &str, at: &str) -> io::Result<()> {
    let dir = records(home);
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{issue}.json"));
    if path.exists() {
        return Ok(());
    }
    let body = serde_json::json!({"issue": issue, "how": how, "at": at});
    fs::write(path, format!("{body}\n"))
}

/// The issues a start is on record for, in order. A file whose name is not a
/// number is not a record.
pub fn recorded(home: &Path) -> Vec<u64> {
    let Ok(rd) = fs::read_dir(records(home)) else {
        return Vec::new();
    };
    let mut v: Vec<u64> = rd
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            name.strip_suffix(".json")?.parse().ok()
        })
        .collect();
    v.sort_unstable();
    v
}

/// The records themselves, for the status: each file that reads as an object.
pub fn read_records(home: &Path) -> Vec<serde_json::Value> {
    recorded(home)
        .into_iter()
        .filter_map(|n| fs::read_to_string(records(home).join(format!("{n}.json"))).ok())
        .filter_map(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .filter(serde_json::Value::is_object)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tehas-ro-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_line_names_the_write_on_one_line() {
        assert_eq!(
            line("issue edit o/r#7"),
            "read-only: would issue edit o/r#7"
        );
        assert_eq!(line("a\tb\nc"), "read-only: would a b c");
    }

    #[test]
    fn a_record_is_kept_once_and_listed_by_number() {
        let h = dir("rec");
        record(&h, 12, "cycle", "2026-11-06T10:00:00Z").unwrap();
        record(&h, 7, "plan", "2026-11-06T10:00:00Z").unwrap();
        record(&h, 12, "cycle", "2026-11-06T11:00:00Z").unwrap();
        fs::write(records(&h).join("notes.json"), "{}").unwrap();
        assert_eq!(recorded(&h), vec![7, 12]);
        let r = read_records(&h);
        assert_eq!(r[1]["at"], "2026-11-06T10:00:00Z");
        assert_eq!(r[0]["how"], "plan");
        let _ = fs::remove_dir_all(&h);
    }

    #[test]
    fn no_directory_is_no_record() {
        assert!(recorded(Path::new("/nonexistent/tehas")).is_empty());
    }
}
