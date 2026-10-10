//! Follows files that grow: each call gives the complete lines added since
//! the last one.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::time::SystemTime;

// Bytes already consumed, and the start of a line whose end is not written yet.
#[derive(Default)]
struct Position {
    offset: u64,
    carry: Vec<u8>,
}

#[derive(Default)]
pub struct Follower {
    files: HashMap<String, Position>,
}

impl Follower {
    /// The complete, non-empty lines added to a file since the last call. A
    /// file that is not there, or cannot be read, has no new lines.
    pub fn new_lines(&mut self, path: &str) -> Vec<String> {
        let Ok(size) = fs::metadata(path).map(|m| m.len()) else {
            return Vec::new();
        };
        let position = self.files.entry(path.to_string()).or_default();
        if size <= position.offset {
            return Vec::new();
        }
        let mut added = Vec::new();
        let read = File::open(path).and_then(|mut f| {
            f.seek(SeekFrom::Start(position.offset))?;
            f.take(size - position.offset).read_to_end(&mut added)
        });
        if read.is_err() {
            return Vec::new();
        }
        position.offset += added.len() as u64;
        position.carry.extend_from_slice(&added);
        // Lines are cut as bytes and read as text afterwards, so a character
        // that was only half written is never split.
        let Some(end) = position.carry.iter().rposition(|b| *b == b'\n') else {
            return Vec::new();
        };
        let rest = position.carry.split_off(end + 1);
        let complete = std::mem::replace(&mut position.carry, rest);
        complete
            .split(|b| *b == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| String::from_utf8_lossy(line).into_owned())
            .collect()
    }

    /// Every agent transcript of this cycle, in name order: the `.json` files
    /// one directory below `runs` that are already followed or were written
    /// at or after `since`. The directory may not exist until the first
    /// agent starts.
    pub fn transcripts(&self, runs: &str, since: SystemTime) -> Vec<String> {
        let mut found = Vec::new();
        let Ok(days) = fs::read_dir(runs) else {
            return found;
        };
        for day in days.flatten() {
            if !day.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let Some(dir) = day.file_name().to_str().map(|name| format!("{runs}/{name}")) else {
                continue;
            };
            let Ok(files) = fs::read_dir(&dir) else {
                continue;
            };
            for file in files.flatten() {
                let name = file.file_name();
                let Some(name) = name.to_str().filter(|n| n.ends_with(".json")) else {
                    continue;
                };
                if !file.file_type().is_ok_and(|t| t.is_file()) {
                    continue;
                }
                let path = format!("{dir}/{name}");
                let fresh = || fs::metadata(&path).and_then(|m| m.modified()).is_ok_and(|at| at >= since);
                if self.files.contains_key(&path) || fresh() {
                    found.push(path);
                }
            }
        }
        found.sort();
        found
    }
}

/// The agent a transcript belongs to. The file is named
/// `<job id in two dash-separated parts>-<role>.json`.
pub fn role(path: &str) -> String {
    let file = path.rsplit('/').next().unwrap_or(path);
    let file = file.strip_suffix(".json").unwrap_or(file);
    let role = file.splitn(3, '-').nth(2).unwrap_or("");
    if role.is_empty() { file } else { role }.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::path::PathBuf;
    use std::time::Duration;

    // A scratch directory of this test, removed when the test ends.
    struct Scratch(PathBuf);
    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("tehas-console-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }
        fn path(&self, name: &str) -> String {
            self.0.join(name).to_str().unwrap().to_string()
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn append(path: &str, bytes: &[u8]) {
        let mut f = OpenOptions::new().create(true).append(true).open(path).unwrap();
        f.write_all(bytes).unwrap();
    }

    #[test]
    fn only_new_complete_lines_are_given() {
        let scratch = Scratch::new("lines");
        let log = scratch.path("log");
        let mut follower = Follower::default();
        assert_eq!(follower.new_lines(&log), [] as [&str; 0]);
        append(&log, b"one\n\ntwo\nthr");
        assert_eq!(follower.new_lines(&log), ["one", "two"]);
        assert_eq!(follower.new_lines(&log), [] as [&str; 0]);
        append(&log, b"ee\nfour\n");
        assert_eq!(follower.new_lines(&log), ["three", "four"]);
    }

    #[test]
    fn a_character_written_in_two_parts_is_read_whole() {
        let scratch = Scratch::new("utf8");
        let log = scratch.path("log");
        let mut follower = Follower::default();
        let line = "päivää\n".as_bytes();
        append(&log, &line[..2]);
        assert_eq!(follower.new_lines(&log), [] as [&str; 0]);
        append(&log, &line[2..]);
        assert_eq!(follower.new_lines(&log), ["päivää"]);
    }

    #[test]
    fn transcripts_are_the_fresh_and_the_followed_files_in_name_order() {
        let scratch = Scratch::new("runs");
        let runs = scratch.path("runs");
        let follower = Follower::default();
        assert_eq!(follower.transcripts(&runs, SystemTime::UNIX_EPOCH), [] as [&str; 0]);
        fs::create_dir_all(format!("{runs}/2026-10-10")).unwrap();
        fs::create_dir_all(format!("{runs}/2026-10-09/nested.json")).unwrap();
        for name in ["2026-10-10/b-1-coder.json", "2026-10-10/a-1-reviewer.json", "2026-10-09/z-1-coder.json", "2026-10-10/notes.txt", "top.json"] {
            append(&format!("{runs}/{name}"), b"{}\n");
        }
        let all = [
            format!("{runs}/2026-10-09/z-1-coder.json"),
            format!("{runs}/2026-10-10/a-1-reviewer.json"),
            format!("{runs}/2026-10-10/b-1-coder.json"),
        ];
        assert_eq!(follower.transcripts(&runs, SystemTime::UNIX_EPOCH), all);

        // Nothing is as new as a minute from now, except what is followed already.
        let later = SystemTime::now() + Duration::from_secs(60);
        assert_eq!(follower.transcripts(&runs, later), [] as [&str; 0]);
        let mut follower = follower;
        follower.new_lines(&all[1]);
        assert_eq!(follower.transcripts(&runs, later), all[1..2]);
    }

    #[test]
    fn the_role_is_what_follows_the_job_id() {
        assert_eq!(role("/runs/2026-10-10/ab12-cd34-coder.json"), "coder");
        assert_eq!(role("/runs/2026-10-10/ab12-cd34-code-reviewer.json"), "code-reviewer");
        assert_eq!(role("/runs/2026-10-10/ab12-cd34.json"), "ab12-cd34");
        assert_eq!(role("/runs/2026-10-10/plain.json"), "plain");
    }
}
