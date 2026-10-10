//! Follows files that grow: each call gives the complete lines added since
//! the last one.

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::OpenOptionsExt;
use std::time::SystemTime;

// The transcripts are written by agents, so their size is not to be trusted:
// this much is read of a file in one pass, and a line longer than the second
// limit is left out. Both are far above what a cycle writes between two
// passes.
const MAX_PASS: u64 = 8 << 20;
const MAX_LINE: usize = 1 << 20;

// Bytes already consumed, the start of a line whose end is not written yet,
// and whether the line being read is one that was left out for its length.
#[derive(Default)]
struct Position {
    offset: u64,
    carry: Vec<u8>,
    dropping: bool,
}

#[derive(Default)]
pub struct Follower {
    files: HashMap<String, Position>,
    skipped: usize,
}

impl Follower {
    /// The complete, non-empty lines added to a file since the last call. A
    /// file that is not there, is not a plain file or cannot be read has no
    /// new lines. A file that became shorter is read again from its start. A
    /// line longer than the limit is left out and counted.
    pub fn new_lines(&mut self, path: &str) -> Vec<String> {
        // A name an agent can write may be a link to a file elsewhere, or a
        // pipe that would block: neither is opened and waited for.
        let opened = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path);
        let Ok(mut file) = opened else {
            return Vec::new();
        };
        let Some(size) = file
            .metadata()
            .ok()
            .filter(|m| m.is_file())
            .map(|m| m.len())
        else {
            return Vec::new();
        };
        let position = self.files.entry(path.to_string()).or_default();
        if size < position.offset {
            *position = Position::default();
        }
        let mut added = Vec::new();
        let wanted = (size - position.offset).min(MAX_PASS);
        let read = file
            .seek(SeekFrom::Start(position.offset))
            .and_then(|_| file.take(wanted).read_to_end(&mut added));
        if read.is_err() || added.is_empty() {
            return Vec::new();
        }
        position.offset += added.len() as u64;
        position.carry.extend_from_slice(&added);
        if position.dropping {
            let Some(end) = position.carry.iter().position(|b| *b == b'\n') else {
                position.carry = Vec::new();
                return Vec::new();
            };
            position.carry.drain(..=end);
            position.dropping = false;
        }
        // Lines are cut as bytes and read as text afterwards, so a character
        // that was only half written is never split.
        let newline = position.carry.iter().rposition(|b| *b == b'\n');
        let mut rest = position.carry.split_off(newline.map_or(0, |end| end + 1));
        // What is kept until the next pass is never more than a line may be,
        // however many files an agent writes. A new Vec gives the memory
        // back, which clearing this one would not.
        if rest.len() > MAX_LINE {
            rest = Vec::new();
            position.dropping = true;
            self.skipped += 1;
        }
        let complete = std::mem::replace(&mut position.carry, rest);
        let mut lines = Vec::new();
        for line in complete.split(|b| *b == b'\n') {
            if line.len() > MAX_LINE {
                self.skipped += 1;
            } else if !line.is_empty() {
                lines.push(String::from_utf8_lossy(line).into_owned());
            }
        }
        lines
    }

    /// How many lines were left out for their length since the last call.
    pub fn take_skipped(&mut self) -> usize {
        std::mem::take(&mut self.skipped)
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
            let Some(dir) = day
                .file_name()
                .to_str()
                .map(|name| format!("{runs}/{name}"))
            else {
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
                let fresh = || {
                    fs::metadata(&path)
                        .and_then(|m| m.modified())
                        .is_ok_and(|at| at >= since)
                };
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
/// `<job id in two dash-separated parts>-<role>.json`. Agents write these
/// files, and the name is printed as it is: a role that is anything but
/// letters, digits, dashes and underscores is not taken from the name.
pub fn role(path: &str) -> String {
    let file = path.rsplit('/').next().unwrap_or(path);
    let file = file.strip_suffix(".json").unwrap_or(file);
    let role = file
        .splitn(3, '-')
        .nth(2)
        .filter(|r| !r.is_empty())
        .unwrap_or(file);
    if role
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        && !role.is_empty()
    {
        role.to_string()
    } else {
        "agent".to_string()
    }
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
            let dir =
                std::env::temp_dir().join(format!("tehas-console-{}-{name}", std::process::id()));
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
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
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
    fn a_file_that_became_shorter_is_read_again_from_its_start() {
        let scratch = Scratch::new("shorter");
        let log = scratch.path("log");
        let mut follower = Follower::default();
        append(&log, b"one\ntwo\nthr");
        assert_eq!(follower.new_lines(&log), ["one", "two"]);
        fs::write(&log, b"new\n").unwrap();
        assert_eq!(follower.new_lines(&log), ["new"]);
        append(&log, b"next\n");
        assert_eq!(follower.new_lines(&log), ["next"]);
    }

    #[test]
    fn a_line_that_does_not_end_is_left_out_and_the_next_is_read() {
        let scratch = Scratch::new("long");
        let log = scratch.path("log");
        let mut follower = Follower::default();
        append(&log, b"first\n");
        assert_eq!(follower.new_lines(&log), ["first"]);
        append(&log, &vec![b'x'; MAX_LINE + 1]);
        assert_eq!(follower.new_lines(&log), [] as [&str; 0]);
        assert_eq!(follower.take_skipped(), 1);
        append(&log, &[b'x'; 10]);
        assert_eq!(follower.new_lines(&log), [] as [&str; 0]);
        append(&log, b"tail of it\nlast\n");
        assert_eq!(follower.new_lines(&log), ["last"]);
        assert_eq!(follower.take_skipped(), 0);
    }

    #[test]
    fn what_follows_the_last_line_of_a_pass_is_held_to_the_same_limit() {
        let scratch = Scratch::new("rest");
        let log = scratch.path("log");
        let mut follower = Follower::default();
        append(&log, b"first\n");
        append(&log, &vec![b'x'; MAX_LINE + 1]);
        assert_eq!(follower.new_lines(&log), ["first"]);
        assert_eq!(follower.files[&log].carry.capacity(), 0);
        assert_eq!(follower.take_skipped(), 1);
        append(&log, b"tail of it\nlast\n");
        assert_eq!(follower.new_lines(&log), ["last"]);
    }

    #[test]
    fn a_whole_line_over_the_limit_is_left_out_and_counted() {
        let scratch = Scratch::new("whole");
        let log = scratch.path("log");
        let mut follower = Follower::default();
        append(&log, b"one\n");
        append(&log, &vec![b'x'; MAX_LINE + 1]);
        append(&log, b"\ntwo\n");
        assert_eq!(follower.new_lines(&log), ["one", "two"]);
        assert_eq!(follower.take_skipped(), 1);
        assert_eq!(follower.take_skipped(), 0);
    }

    #[test]
    fn a_link_and_a_pipe_are_not_read() {
        let scratch = Scratch::new("odd");
        let (file, link, pipe) = (
            scratch.path("file"),
            scratch.path("link"),
            scratch.path("pipe"),
        );
        append(&file, b"elsewhere\n");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        assert!(std::process::Command::new("mkfifo")
            .arg(&pipe)
            .status()
            .unwrap()
            .success());
        let mut follower = Follower::default();
        assert_eq!(follower.new_lines(&link), [] as [&str; 0]);
        assert_eq!(follower.new_lines(&pipe), [] as [&str; 0]);
        assert_eq!(follower.new_lines(&file), ["elsewhere"]);
    }

    #[test]
    fn transcripts_are_the_fresh_and_the_followed_files_in_name_order() {
        let scratch = Scratch::new("runs");
        let runs = scratch.path("runs");
        let follower = Follower::default();
        assert_eq!(
            follower.transcripts(&runs, SystemTime::UNIX_EPOCH),
            [] as [&str; 0]
        );
        fs::create_dir_all(format!("{runs}/2026-10-10")).unwrap();
        fs::create_dir_all(format!("{runs}/2026-10-09/nested.json")).unwrap();
        for name in [
            "2026-10-10/b-1-coder.json",
            "2026-10-10/a-1-reviewer.json",
            "2026-10-09/z-1-coder.json",
            "2026-10-10/notes.txt",
            "top.json",
        ] {
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
        assert_eq!(
            role("/runs/2026-10-10/ab12-cd34-code-reviewer.json"),
            "code-reviewer"
        );
        assert_eq!(role("/runs/2026-10-10/ab12-cd34.json"), "ab12-cd34");
        assert_eq!(role("/runs/2026-10-10/plain.json"), "plain");
        // Nothing an agent puts in a file name reaches the console as text.
        assert_eq!(role("/runs/2026-10-10/a-b-GH_TOKEN=abc.json"), "agent");
        assert_eq!(
            role("/runs/2026-10-10/a-b-coder\n12:00:00 driver     forged.json"),
            "agent"
        );
        assert_eq!(role("/runs/2026-10-10/.json"), "agent");
    }
}
