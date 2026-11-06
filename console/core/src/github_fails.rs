//! The GitHub calls that failed lately, kept on the host so health can name an
//! outage: `state/github-failures`, one line `<unix second>\t<line>` for each
//! read the poll, `tehasbot status` or `tehas tidy` could not make. The line is
//! the one the program logged, which holds gh's words of a fixed list and no
//! token.
//!
//! Writing is best effort: a record that cannot be written changes nothing in
//! the program that failed its call. The file is cut back to the lines of the
//! last `WINDOW` once it is over `KEEP_BYTES`.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// The window health counts failures over: ten minutes.
pub const WINDOW: i64 = 600;
/// From this many failures in the window, health warns.
pub const THRESHOLD: usize = 3;
/// The size past which a write cuts the file back to the window.
const KEEP_BYTES: u64 = 64 * 1024;
/// The longest line kept, in characters.
const LINE_MAX: usize = 300;

/// The file under a tehas home.
pub fn path(home: &Path) -> PathBuf {
    home.join("state/github-failures")
}

/// One line of a log made safe for the file: no control character, so no
/// second line, and no more than `LINE_MAX` characters.
fn clean(line: &str) -> String {
    line.chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .take(LINE_MAX)
        .collect()
}

/// Keep one failed call at `now`.
pub fn note(home: &Path, now: i64, line: &str) {
    let file = path(home);
    let entry = format!("{now}\t{}\n", clean(line));
    if fs::metadata(&file).is_ok_and(|m| m.len() > KEEP_BYTES) {
        let kept: String = lines(&file)
            .into_iter()
            .filter(|(at, _)| now - at < WINDOW)
            .map(|(at, l)| format!("{at}\t{l}\n"))
            .collect();
        let _ = fs::write(&file, kept + &entry);
        return;
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&file) {
        let _ = f.write_all(entry.as_bytes());
    }
}

fn lines(file: &Path) -> Vec<(i64, String)> {
    fs::read_to_string(file)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let (at, rest) = l.split_once('\t')?;
            Some((at.parse().ok()?, rest.to_string()))
        })
        .collect()
}

/// The failures of the window that ends at `now`.
#[derive(Debug, Default, PartialEq)]
pub struct Recent {
    pub count: usize,
    /// The newest one: its second and its line.
    pub last: Option<(i64, String)>,
}

impl Recent {
    /// Whether there are enough of them to warn of.
    pub fn outage(&self) -> bool {
        self.count >= THRESHOLD
    }
}

/// What the file holds of the last `WINDOW` seconds before `now`. A line of the
/// future, from a clock set back, is not counted.
pub fn recent(home: &Path, now: i64) -> Recent {
    let mut r = Recent::default();
    for (at, line) in lines(&path(home)) {
        if at > now || now - at >= WINDOW {
            continue;
        }
        r.count += 1;
        if r.last.as_ref().is_none_or(|(t, _)| at >= *t) {
            r.last = Some((at, line));
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdir::TestDir;

    fn home(name: &str) -> TestDir {
        let d = TestDir::new(&format!("github-fails-{name}"));
        fs::create_dir_all(d.join("state")).unwrap();
        d
    }

    #[test]
    fn the_window_counts_the_failures_of_the_last_ten_minutes() {
        let h = home("window");
        note(&h, 1000, "ERROR: gh api graphql failed: HTTP 502");
        note(&h, 1500, "WARN status: gh issue list failed: HTTP 502");
        note(&h, 1590, "WARN: tidy: gh search issues failed: timed out");
        let r = recent(&h, 1600);
        assert_eq!(r.count, 2, "the one of 600 s and more ago is out");
        assert_eq!(
            r.last,
            Some((
                1590,
                "WARN: tidy: gh search issues failed: timed out".into()
            ))
        );
        assert!(!r.outage());
        note(&h, 1595, "ERROR: gh issue list failed: HTTP 503");
        assert!(recent(&h, 1600).outage());
    }

    #[test]
    fn no_file_is_no_failure() {
        let h = home("none");
        assert_eq!(recent(&h, 1000), Recent::default());
    }

    #[test]
    fn a_line_is_one_line_and_a_damaged_line_is_skipped() {
        let h = home("clean");
        note(&h, 1000, "ERROR: x\nforged 999\tline");
        fs::write(
            path(&h),
            fs::read_to_string(path(&h)).unwrap() + "garbage\nnope\tx\n",
        )
        .unwrap();
        let r = recent(&h, 1000);
        assert_eq!(r.count, 1);
        assert_eq!(r.last.unwrap().1, "ERROR: x?forged 999?line");
    }

    #[test]
    fn a_big_file_is_cut_back_to_the_window() {
        let h = home("cut");
        let old = format!("1\t{}\n", "x".repeat(200)).repeat(400);
        fs::write(path(&h), old).unwrap();
        note(&h, 10_000, "ERROR: gh issue list failed");
        let text = fs::read_to_string(path(&h)).unwrap();
        assert_eq!(text, "10000\tERROR: gh issue list failed\n");
    }
}
