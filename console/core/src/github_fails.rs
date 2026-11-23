//! The GitHub calls that failed lately, kept on the host so health can name an
//! outage: `state/github-failures`, one line `<unix second>\t<line>` for each
//! read the poll, `tehasbot status` or `tehas tidy` could not make. The line is
//! a fixed one in our words for the kind of failure ([`line`]), never gh's
//! text: the program's own log keeps that.
//!
//! Writing is best effort: a record that cannot be written changes nothing in
//! the program that failed its call. The file is cut back to the lines of the
//! last `WINDOW` once it is over `KEEP_BYTES`.

use crate::home::{Home, State};
use crate::retry::Fault;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// The window health counts failures over: ten minutes.
pub const WINDOW: i64 = 600;
/// From this many failures in the window, health warns.
pub const THRESHOLD: usize = 3;
/// The size past which a write cuts the file back to the window.
const KEEP_BYTES: u64 = 64 * 1024;

/// The file under a tehas home.
pub fn path(home: &Path) -> PathBuf {
    Home::new(home).state(State::GithubFailures)
}

/// The line the health check shows for a kind of failure.
pub fn line(fault: Fault) -> &'static str {
    match fault {
        Fault::Limit => "GitHub's rate limit was reached",
        Fault::Server => "GitHub failed with a server error",
        Fault::Refused => "GitHub refused the call",
        Fault::Slow => "GitHub did not answer in time",
        Fault::Down => "GitHub could not be reached",
        Fault::Unread => "GitHub's answer could not be read",
        Fault::Other => "a call to GitHub failed",
    }
}

/// Keep one failed call at `now`, of the kind the logged line `said` names
/// (`retry::fault_of`).
pub fn note(home: &Path, now: i64, said: &str) {
    let file = path(home);
    let entry = format!("{now}\t{}\n", line(crate::retry::fault_of(said)));
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
        fs::create_dir_all(Home::new(d.path()).state_dir()).unwrap();
        d
    }

    #[test]
    fn the_window_counts_the_failures_of_the_last_ten_minutes() {
        let h = home("window");
        note(&h, 1000, "ERROR: gh api graphql failed: no such host");
        note(&h, 1500, "WARN status: gh issue list failed: no such host");
        note(&h, 1590, "WARN: tidy: gh search issues failed: timed out");
        let r = recent(&h, 1600);
        assert_eq!(r.count, 2, "the one of 600 s and more ago is out");
        assert_eq!(r.last, Some((1590, "GitHub did not answer in time".into())));
        assert!(!r.outage());
        note(&h, 1595, "ERROR: gh issue list failed: unexpected EOF");
        assert!(recent(&h, 1600).outage());
    }

    #[test]
    fn no_file_is_no_failure() {
        let h = home("none");
        assert_eq!(recent(&h, 1000), Recent::default());
    }

    #[test]
    fn the_file_keeps_a_fixed_line_per_kind_and_none_of_ghs_words() {
        let h = home("kinds");
        let said = [
            "WARN: tidy: gh search failed: secondary rate limit",
            "ERROR: gh api graphql failed: HTTP 502 (tried 2 times)",
            "ERROR: gh issue list failed: HTTP 404 at api.example.test",
            "WARN status: gh issue list failed: connection reset",
            "ERROR: gh api graphql failed: its answer could not be read",
            "WARN status: gh issue list failed\nforged 999\tline",
        ];
        for (i, l) in said.iter().enumerate() {
            note(&h, 1000 + i as i64, l);
        }
        let text = fs::read_to_string(path(&h)).unwrap();
        let kept: Vec<&str> = text
            .lines()
            .map(|l| l.split_once('\t').unwrap().1)
            .collect();
        assert_eq!(
            kept,
            [
                "GitHub's rate limit was reached",
                "GitHub failed with a server error",
                "GitHub refused the call",
                "GitHub could not be reached",
                "GitHub's answer could not be read",
                "a call to GitHub failed",
            ]
        );
    }

    #[test]
    fn the_line_and_githubs_fault_read_a_failure_alike() {
        use crate::retry::{fault_of, githubs_fault};
        let git = "fatal: unable to access: The requested URL returned error: 502";
        assert_eq!(fault_of(git), Fault::Server);
        assert!(githubs_fault(git));
        let slow = "WARN status: gh issue list failed: time out";
        assert_eq!(fault_of(slow), Fault::Slow);
        assert!(githubs_fault(slow));
        for said in ["ERROR: gh issue list failed", "its answer could not be read"] {
            assert!(!githubs_fault(said), "{said}");
        }
    }

    #[test]
    fn a_damaged_line_is_skipped() {
        let h = home("clean");
        note(&h, 1000, "ERROR: x");
        fs::write(
            path(&h),
            fs::read_to_string(path(&h)).unwrap() + "garbage\nnope\tx\n",
        )
        .unwrap();
        let r = recent(&h, 1000);
        assert_eq!(r.count, 1);
        assert_eq!(r.last.unwrap().1, "a call to GitHub failed");
    }

    #[test]
    fn a_big_file_is_cut_back_to_the_window() {
        let h = home("cut");
        let old = format!("1\t{}\n", "x".repeat(200)).repeat(400);
        fs::write(path(&h), old).unwrap();
        note(&h, 10_000, "ERROR: gh issue list failed");
        let text = fs::read_to_string(path(&h)).unwrap();
        assert_eq!(text, "10000\ta call to GitHub failed\n");
    }
}
