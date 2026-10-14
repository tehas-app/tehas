//! Follows files that grow: each call gives the complete lines added since
//! the last one.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::time::{SystemTime, UNIX_EPOCH};

// The transcripts are written by agents, so their size is not to be trusted:
// this much is read of a file in one pass, and a line longer than the second
// limit is left out. Both are far above what a cycle writes between two
// passes. The third bounds what is held of unfinished lines in all files
// together, however many an agent writes.
const MAX_PASS: u64 = 8 << 20;
const MAX_LINE: usize = 1 << 20;
const MAX_HELD: usize = 16 << 20;
const MAX_ROLE: usize = 24;

// Bytes already consumed, the start of a line whose end is not written yet,
// and whether the line being read is one that was left out for its length.
#[derive(Default)]
struct Position {
    offset: u64,
    carry: Vec<u8>,
    dropping: bool,
}

// The transcripts and the other logs are kept apart: a transcript that is
// gone is forgotten at the next listing.
#[derive(Default)]
pub struct Follower {
    files: HashMap<String, Position>,
    logs: HashMap<String, Position>,
    held: usize,
    skipped: usize,
    unheld: usize,
}

/// A transcript of this cycle, opened where it was found.
pub struct Transcript {
    pub path: String,
    file: File,
}

// A name an agent can write may be a link to a file elsewhere: its last part
// is never followed.
fn open(path: &str, flags: i32) -> Option<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | flags)
        .open(path)
        .ok()
}

impl Follower {
    /// The complete, non-empty lines added to a file since the last call. A
    /// file that is not there, is not a plain file or cannot be read has no
    /// new lines. A file that became shorter is read again from its start. A
    /// line longer than the limit is left out and counted.
    pub fn new_lines(&mut self, path: &str) -> Vec<String> {
        // Not waited for, should the name be a pipe.
        match open(path, libc::O_NONBLOCK) {
            Some(file) => self.read(path, &file, false),
            None => Vec::new(),
        }
    }

    /// The same of a transcript.
    pub fn transcript_lines(&mut self, transcript: &Transcript) -> Vec<String> {
        self.read(&transcript.path, &transcript.file, true)
    }

    fn read(&mut self, path: &str, file: &File, transcript: bool) -> Vec<String> {
        let Some(size) = file
            .metadata()
            .ok()
            .filter(|m| m.is_file())
            .map(|m| m.len())
        else {
            return Vec::new();
        };
        let files = if transcript {
            &mut self.files
        } else {
            &mut self.logs
        };
        let position = files.entry(path.to_string()).or_default();
        self.held -= position.carry.len();
        let lines = Self::advance(position, file, size, &mut self.skipped);
        // What the other files hold leaves this one less room.
        if self.held + position.carry.len() > MAX_HELD {
            position.carry = Vec::new();
            position.dropping = true;
            self.unheld += 1;
        }
        self.held += position.carry.len();
        lines
    }

    fn advance(
        position: &mut Position,
        mut file: &File,
        size: u64,
        skipped: &mut usize,
    ) -> Vec<String> {
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
            *skipped += 1;
        }
        let complete = std::mem::replace(&mut position.carry, rest);
        let mut lines = Vec::new();
        for line in complete.split(|b| *b == b'\n') {
            if line.len() > MAX_LINE {
                *skipped += 1;
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

    /// How many unfinished lines were left out since the last call because
    /// the files together held as much as they may.
    pub fn take_unheld(&mut self) -> usize {
        std::mem::take(&mut self.unheld)
    }

    /// Every agent transcript of this cycle, in name order: the `.json` files
    /// one directory below `runs` that are already followed or were written
    /// at or after `since`. The directory may not exist until the first
    /// agent starts. A followed transcript that is not listed is forgotten.
    pub fn transcripts(&mut self, runs: &str, since: SystemTime) -> Vec<Transcript> {
        let found = self.list(runs, since);
        let listed: HashSet<&str> = found.iter().map(|t| t.path.as_str()).collect();
        let held = &mut self.held;
        self.files.retain(|path, position| {
            let keep = listed.contains(path.as_str());
            if !keep {
                *held -= position.carry.len();
            }
            keep
        });
        found
    }

    fn list(&self, runs: &str, since: SystemTime) -> Vec<Transcript> {
        let mut found = Vec::new();
        let Ok(days) = fs::read_dir(runs) else {
            return found;
        };
        let oldest = day_before(since);
        for day in days.flatten() {
            let day = day.file_name();
            let Some(day) = day.to_str() else {
                continue;
            };
            // The directories are named for days and are never cleared away:
            // one of a day long before the cycle is not read at every pass.
            if is_day(day) && day < oldest.as_str() {
                continue;
            }
            // The directory is opened once and its files are reached through
            // that handle, so an agent that swaps it for a link between the
            // listing and the opening does not send the console elsewhere.
            let Some(dir) = open(&format!("{runs}/{day}"), libc::O_DIRECTORY) else {
                continue;
            };
            let held = format!("/proc/self/fd/{}", dir.as_raw_fd());
            let Ok(files) = fs::read_dir(&held) else {
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
                let path = format!("{runs}/{day}/{name}");
                let fresh = || {
                    file.metadata()
                        .and_then(|m| m.modified())
                        .is_ok_and(|at| at >= since)
                };
                if !self.files.contains_key(&path) && !fresh() {
                    continue;
                }
                if let Some(file) = open(&format!("{held}/{name}"), libc::O_NONBLOCK) {
                    found.push(Transcript { path, file });
                }
            }
        }
        found.sort_by(|a, b| a.path.cmp(&b.path));
        found
    }
}

fn is_day(name: &str) -> bool {
    name.len() == 10
        && name.bytes().enumerate().all(|(i, b)| match i {
            4 | 7 => b == b'-',
            _ => b.is_ascii_digit(),
        })
}

// The day before a time, in UTC, as the directories are named: YYYY-MM-DD.
// A day earlier than the time itself, because the directories are named in
// the host's own time zone, which is within a day of UTC.
fn day_before(time: SystemTime) -> String {
    let seconds = time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    // Days since the epoch to a date of the civil calendar, from the first
    // of March of year 0 in eras of 400 years.
    let days = (seconds / 86400) as i64 - 1 + 719_468;
    let (era, day_of_era) = (days.div_euclid(146_097), days.rem_euclid(146_097));
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month + 2) / 5 + 1;
    let month = if month < 10 { month + 3 } else { month - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// The agent a transcript belongs to. The file is named
/// `<job id in two dash-separated parts>-<role>.json`. Agents write these
/// files, and the name is printed as it is: a role that is anything but
/// letters, digits, dashes and underscores is not taken from the name, and
/// neither is the name of a speaker that is not an agent. A role is cut to
/// the length a workstream may have.
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
        && role != "driver"
        && role != "tehas"
    {
        role[..role.len().min(MAX_ROLE)].to_string()
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
        assert_eq!(follower.logs[&log].carry.capacity(), 0);
        assert_eq!(follower.take_skipped(), 1);
        append(&log, b"tail of it\nlast\n");
        assert_eq!(follower.new_lines(&log), ["last"]);
    }

    #[test]
    fn what_all_transcripts_hold_is_bounded_and_a_gone_one_is_forgotten() {
        let scratch = Scratch::new("held");
        let runs = scratch.path("runs");
        let day = day_before(SystemTime::now() + Duration::from_secs(86400));
        fs::create_dir_all(format!("{runs}/{day}")).unwrap();
        let over = 4;
        let count = MAX_HELD / MAX_LINE + over;
        for n in 0..count {
            append(
                &format!("{runs}/{day}/a-b-r{n}.json"),
                &vec![b'x'; MAX_LINE],
            );
        }
        let mut follower = Follower::default();
        for transcript in follower.transcripts(&runs, UNIX_EPOCH) {
            assert_eq!(follower.transcript_lines(&transcript), [] as [&str; 0]);
        }
        let held: usize = follower.files.values().map(|p| p.carry.len()).sum();
        assert_eq!(follower.files.len(), count);
        assert!(held <= MAX_HELD, "{held}");
        assert_eq!(held, follower.held);
        assert_eq!(follower.take_unheld(), over);
        assert_eq!(follower.take_skipped(), 0);

        fs::remove_dir_all(format!("{runs}/{day}")).unwrap();
        assert_eq!(follower.transcripts(&runs, UNIX_EPOCH).len(), 0);
        assert!(follower.files.is_empty());
        assert_eq!(follower.held, 0);
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
        let mut follower = Follower::default();
        let paths = |follower: &mut Follower, since| -> Vec<String> {
            let found = follower.transcripts(&runs, since);
            found.into_iter().map(|t| t.path).collect()
        };
        assert_eq!(paths(&mut follower, UNIX_EPOCH), [] as [&str; 0]);
        // The days are named from the clock: a day long before the time
        // asked for is not listed, so fixed dates would age out of the test.
        let day = |ahead: u64| day_before(SystemTime::now() + Duration::from_secs(ahead * 86400));
        let (yesterday, today, tomorrow) = (day(0), day(1), day(2));
        fs::create_dir_all(format!("{runs}/{today}")).unwrap();
        fs::create_dir_all(format!("{runs}/{yesterday}/nested.json")).unwrap();
        for name in [
            format!("{today}/b-1-coder.json"),
            format!("{today}/a-1-reviewer.json"),
            format!("{yesterday}/z-1-coder.json"),
            format!("{today}/notes.txt"),
            "top.json".to_string(),
        ] {
            append(&format!("{runs}/{name}"), b"{}\n");
        }
        let all = [
            format!("{runs}/{yesterday}/z-1-coder.json"),
            format!("{runs}/{today}/a-1-reviewer.json"),
            format!("{runs}/{today}/b-1-coder.json"),
        ];
        // A day that is a link to a directory elsewhere is not entered.
        fs::create_dir_all(scratch.path("elsewhere")).unwrap();
        append(&scratch.path("elsewhere/c-1-coder.json"), b"{}\n");
        std::os::unix::fs::symlink(scratch.path("elsewhere"), format!("{runs}/{tomorrow}"))
            .unwrap();
        assert_eq!(paths(&mut follower, UNIX_EPOCH), all);

        // Nothing is as new as a minute from now, except what is followed already.
        let later = SystemTime::now() + Duration::from_secs(60);
        assert_eq!(paths(&mut follower, later), [] as [&str; 0]);
        let found = follower.transcripts(&runs, UNIX_EPOCH);
        follower.transcript_lines(&found[1]);
        assert_eq!(paths(&mut follower, later), all[1..2]);
        let found = follower.transcripts(&runs, later);
        append(&all[1], b"more\n");
        assert_eq!(follower.transcript_lines(&found[0]), ["more"]);
    }

    #[test]
    fn a_directory_of_a_day_long_before_the_cycle_is_not_read() {
        let scratch = Scratch::new("days");
        let runs = scratch.path("runs");
        for day in [
            "2026-10-08",
            "2026-10-09",
            "2026-10-10",
            "2026-10-11",
            "other",
        ] {
            fs::create_dir_all(format!("{runs}/{day}")).unwrap();
            append(&format!("{runs}/{day}/a-1-coder.json"), b"{}\n");
        }
        // Noon of the tenth of October 2026, UTC.
        let since = UNIX_EPOCH + Duration::from_secs(1_791_633_600);
        let found = Follower::default().transcripts(&runs, since);
        let days: Vec<&str> = found
            .iter()
            .filter_map(|t| t.path.rsplit('/').nth(1))
            .collect();
        assert_eq!(days, ["2026-10-09", "2026-10-10", "2026-10-11", "other"]);
    }

    #[test]
    fn the_day_before_a_time_is_a_date() {
        let at = |seconds| day_before(UNIX_EPOCH + Duration::from_secs(seconds));
        assert_eq!(at(0), "1969-12-31");
        assert_eq!(at(86400), "1970-01-01");
        assert_eq!(at(1_709_251_200), "2024-02-29"); // the first of March 2024
        assert_eq!(at(1_791_633_600), "2026-10-09");
        assert_eq!(at(1_798_761_600), "2026-12-31"); // the first of January 2027
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
        // An agent does not speak as the driver or as tehas.
        assert_eq!(role("/runs/2026-10-10/a-b-driver.json"), "agent");
        assert_eq!(role("/runs/2026-10-10/tehas.json"), "agent");
        assert_eq!(role("/runs/2026-10-10/a-b-driver-2.json"), "driver-2");
        let long = format!("/runs/2026-10-10/a-b-{}.json", "r".repeat(300));
        assert_eq!(role(&long), "r".repeat(24));
    }
}
