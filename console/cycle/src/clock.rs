//! The clock a cycle is measured on (#1042). A cycle's limits are counted on
//! the wall clock today, and a wall clock jumps forward by the time a VM was
//! saved: every running cycle is then stopped as stalled or over its limit a
//! moment after the restore. This module counts on a clock that stops while
//! the VM is saved: the seconds since boot, read from the first number of
//! `/proc/uptime`. It reads no wall clock at all.
//!
//! Three verbs sit on it. `clock now` prints those seconds. `clock mark
//! <file>` writes them into a file, and `clock check <file> --limit <s>
//! --stall <s> --since <s>` compares how far the cycle has run and how long
//! its log has stood still against the two bounds, and prints `ok`, `stalled`
//! or `over` with exit 0, 1 or 2. A mark it cannot read or understand prints
//! `unknown` and exits 3, so a caller never stops a cycle on a guess.
//!
//! The clock is read through `boot_seconds`, which the tests replace: a test
//! says what the clock reads, so no test sleeps.

use std::fs;
use std::path::{Path, PathBuf};
use tehas_core::file::read_regular;

/// The first number of a host file that holds the clock. It is a small file
/// of two decimals; the bound is room to spare.
const UPTIME_BYTES: u64 = 64;

/// The bytes of a mark file. A mark is one number.
const MARK_BYTES: u64 = 32;

/// The clock a cycle is measured on: the seconds since boot, read from the
/// first number of /proc/uptime. Nothing when that file is missing, is not a
/// regular file, cannot be read, or does not begin with a decimal.
pub fn boot_seconds() -> Option<u64> {
    let (bytes, _) = read_regular(&PathBuf::from("/proc/uptime"), UPTIME_BYTES).ok()?;
    let text = std::str::from_utf8(&bytes).ok()?;
    decimal(text)
}

/// The digits of one unsigned decimal, or nothing for a field that is not
/// one: empty, negative, signed, with a letter in it, with more than one dot,
/// or with a dot and no digit on either side of it.
fn digits(field: &str) -> Option<u64> {
    let mut seen_dot = false;
    let mut seen_digit = false;
    for b in field.bytes() {
        if b == b'.' {
            if seen_dot || !seen_digit {
                return None;
            }
            seen_dot = true;
            continue;
        }
        if !b.is_ascii_digit() {
            return None;
        }
        seen_digit = true;
    }
    if !seen_digit {
        return None;
    }
    field.split('.').next().unwrap_or(field).parse().ok()
}

/// The first field of `text` as whole seconds.
fn decimal(text: &str) -> Option<u64> {
    digits(text.split_whitespace().next()?)
}

/// The clock a caller of `check` is measured against: `boot_seconds` on a
/// host, and a reading of the test's own in its cases, so no test sleeps.
pub type Now = fn() -> Option<u64>;

/// What `clock check` printed, and the exit status it ends with.
#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Inside both bounds.
    Ok,
    /// No log growth for `stall` seconds or more, inside the limit.
    Stalled,
    /// Past `limit` seconds since its start.
    Over,
    /// A mark that is missing or not a number, or a clock that could not be
    /// read: nothing is decided from it.
    Unknown,
}

impl Verdict {
    /// The word printed for it.
    pub fn word(&self) -> &'static str {
        match self {
            Verdict::Ok => "ok",
            Verdict::Stalled => "stalled",
            Verdict::Over => "over",
            Verdict::Unknown => "unknown",
        }
    }

    /// The exit status of the verb that printed it.
    pub fn exit(&self) -> u8 {
        match self {
            Verdict::Ok => 0,
            Verdict::Stalled => 1,
            Verdict::Over => 2,
            Verdict::Unknown => 3,
        }
    }
}

/// Writes the clock to `path`, replacing what was there. The mark of a start
/// and the mark of the last log growth are both made this way. Nothing is
/// written when the clock could not be read.
pub fn mark(path: &Path, now: Now) -> Result<(), String> {
    let s = now().ok_or("the clock could not be read")?;
    fs::write(path, format!("{s}\n")).map_err(|e| format!("{}: {e}", path.display()))
}

/// Reads a mark: the file's one number, or nothing when it is missing, is not
/// a regular file, holds more than a number, or holds one that is not there.
pub fn read_mark(path: &Path) -> Option<u64> {
    let (bytes, _) = read_regular(path, MARK_BYTES).ok()?;
    let text = std::str::from_utf8(&bytes).ok()?;
    let field = text.trim();
    if field.is_empty() || field.split_whitespace().count() != 1 {
        return None;
    }
    decimal(field)
}

/// The verdict for a cycle whose start is marked in `start_mark`, whose log
/// last grew at `since` (a number `now` printed), and which may run at most
/// `limit` seconds and stand still for at most `stall` seconds. A time that
/// would go backwards is not a verdict: it is `unknown`, so a caller stops
/// nothing on a guess.
pub fn check(start_mark: &Path, limit: u64, stall: u64, since: u64, now: Now) -> Verdict {
    let Some(clock) = now() else {
        return Verdict::Unknown;
    };
    let Some(start) = read_mark(start_mark) else {
        return Verdict::Unknown;
    };
    let (elapsed, idle) = match (clock.checked_sub(start), clock.checked_sub(since)) {
        (Some(e), Some(i)) => (e, i),
        _ => return Verdict::Unknown,
    };
    if elapsed > limit {
        return Verdict::Over;
    }
    if idle >= stall {
        return Verdict::Stalled;
    }
    Verdict::Ok
}

/// Reads the clock and prints its seconds. Nothing when it cannot be read,
/// which is what makes the verb fail rather than print a line of its own.
pub fn print_now(now: Now) -> Option<u64> {
    let s = now()?;
    println!("{s}");
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// A clock that says what the test was given. A clock is a plain function,
    /// so the reading is held in a cell of the test's own thread and the
    /// function only reads it back: no test sleeps, and cases that run at the
    /// same time do not read one another's clock.
    fn clock(seconds: Option<u64>) -> Now {
        CLOCK.with(|c| c.set(seconds));
        read
    }

    thread_local! {
        static CLOCK: Cell<Option<u64>> = const { Cell::new(None) };
    }

    fn read() -> Option<u64> {
        CLOCK.with(|c| c.get())
    }

    /// The clock of a host file written like /proc/uptime, read the way the
    /// real one is read.
    fn uptime_of(text: &str) -> Option<u64> {
        let d = dir("clock");
        let p = d.0.join("uptime");
        fs::write(&p, text).unwrap();
        let (bytes, _) = read_regular(&p, UPTIME_BYTES).ok()?;
        std::str::from_utf8(&bytes).ok().and_then(decimal)
    }

    struct Dir(PathBuf);

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn dir(name: &str) -> Dir {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("tehas-clock-{name}-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        Dir(d)
    }

    fn marked(dir: &Path, name: &str, text: &str) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, text).unwrap();
        p
    }

    #[test]
    fn a_run_under_its_limits_is_ok() {
        let d = dir("ok");
        let start = marked(&d.0, "start", "1000\n");
        assert_eq!(
            check(&start, 3600, 600, 1090, clock(Some(1100))),
            Verdict::Ok
        );
    }

    #[test]
    fn a_run_past_its_limit_is_over() {
        let d = dir("over");
        let start = marked(&d.0, "start", "1000\n");
        // 101 s on the clock against a limit of 100 s.
        assert_eq!(
            check(&start, 100, 600, 1099, clock(Some(1101))),
            Verdict::Over
        );
        // The limit itself is not past it.
        assert_eq!(
            check(&start, 101, 600, 1100, clock(Some(1101))),
            Verdict::Ok
        );
    }

    #[test]
    fn a_log_that_stood_still_is_stalled() {
        let d = dir("stalled");
        let start = marked(&d.0, "start", "1000\n");
        // The last growth was at 500 s of the clock, the clock reads 1100 s:
        // 600 s without a line, against a stall of 600 s.
        assert_eq!(
            check(&start, 3600, 600, 500, clock(Some(1100))),
            Verdict::Stalled
        );
        // One second short of the bound is still ok.
        assert_eq!(
            check(&start, 3600, 600, 501, clock(Some(1100))),
            Verdict::Ok
        );
    }

    #[test]
    fn the_limit_is_counted_before_the_stall() {
        let d = dir("both");
        let start = marked(&d.0, "start", "1000\n");
        assert_eq!(
            check(&start, 100, 600, 100, clock(Some(1101))),
            Verdict::Over
        );
    }

    #[test]
    fn a_wall_clock_that_jumped_an_hour_is_still_ok() {
        // The wall clock moved 3600 s while these numbers moved 10 s. The
        // verb never reads a wall clock, so the cycle is not stalled and not
        // over: an hour of the VM being saved is simply not counted.
        let d = dir("saved");
        let start = marked(&d.0, "start", "1000\n");
        assert_eq!(
            check(&start, 3600, 600, 1005, clock(Some(1010))),
            Verdict::Ok
        );
    }

    #[test]
    fn a_missing_mark_is_unknown() {
        let d = dir("missing");
        assert_eq!(
            check(&d.0.join("start"), 3600, 600, 1090, clock(Some(1100))),
            Verdict::Unknown
        );
    }

    #[test]
    fn a_malformed_mark_is_unknown() {
        let d = dir("malformed");
        for bad in [
            "",
            "\n",
            "abc\n",
            "-10\n",
            "1000 2000\n",
            "1000x\n",
            ".5\n",
            "1.2.3\n",
            "+7\n",
        ] {
            let p = marked(&d.0, "bad", bad);
            assert_eq!(read_mark(&p), None, "{bad:?}");
            assert_eq!(
                check(&p, 3600, 600, 1090, clock(Some(1100))),
                Verdict::Unknown,
                "{bad:?}"
            );
            let _ = fs::remove_file(&p);
        }
        // A directory, a fifo or a link is not a mark either.
        let link = d.0.join("link");
        let real = marked(&d.0, "real", "1000\n");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert_eq!(read_mark(&link), None);
        assert_eq!(read_mark(&d.0), None);
    }

    #[test]
    fn a_clock_that_cannot_be_read_is_unknown() {
        let d = dir("noclock");
        let start = marked(&d.0, "start", "1000\n");
        assert_eq!(
            check(&start, 3600, 600, 1090, clock(None)),
            Verdict::Unknown
        );
        assert_eq!(print_now(clock(None)), None);
    }

    #[test]
    fn a_time_that_would_go_backwards_is_unknown() {
        let d = dir("backwards");
        // A mark from ahead of the clock: the boot was redone, or the file
        // belongs to another cycle. Nothing is decided from it.
        let start = marked(&d.0, "start", "2000\n");
        assert_eq!(
            check(&start, 3600, 600, 1090, clock(Some(1100))),
            Verdict::Unknown
        );
        // A last growth the clock has not reached.
        let start = marked(&d.0, "start", "1000\n");
        assert_eq!(
            check(&start, 3600, 600, 1500, clock(Some(1100))),
            Verdict::Unknown
        );
    }

    #[test]
    fn a_mark_is_the_clock_written_once() {
        let d = dir("write");
        let p = d.0.join("start");
        mark(&p, clock(Some(1234))).unwrap();
        assert_eq!(read_mark(&p), Some(1234));
        // Re-marking the same file leaves one number, not two.
        mark(&p, clock(Some(1299))).unwrap();
        assert_eq!(read_mark(&p), Some(1299));
        // The mark is read back as the whole seconds it was.
        assert_eq!(fs::read_to_string(&p).unwrap(), "1299\n");
    }

    #[test]
    fn a_mark_cannot_be_written_without_the_clock() {
        let d = dir("noclock-write");
        let p = d.0.join("start");
        assert!(mark(&p, clock(None)).is_err());
        assert!(!p.exists());
    }

    #[test]
    fn the_verbosity_of_a_mark_does_not_matter() {
        let d = dir("format");
        // Trailing spaces and a missing newline are still one number.
        assert_eq!(read_mark(&marked(&d.0, "a", "  42  \n")), Some(42));
        assert_eq!(read_mark(&marked(&d.0, "b", "7")), Some(7));
        assert_eq!(read_mark(&marked(&d.0, "c", "0")), Some(0));
    }

    #[test]
    fn the_clock_is_the_first_number_of_uptime() {
        assert_eq!(uptime_of("12345.67 3.00"), Some(12345));
        assert_eq!(uptime_of("0.01 0.00"), Some(0));
        assert_eq!(uptime_of("99\n"), Some(99));
        for bad in ["", "  \n", "abc 0.0", "-5.0 0.0", "5,0 0.0", ".5 0.0"] {
            assert_eq!(uptime_of(bad), None, "{bad:?}");
        }
        // A file that is not there is no clock.
        let d = dir("nofile");
        assert!(read_regular(&d.0.join("uptime"), UPTIME_BYTES).is_err());
    }

    #[test]
    fn a_mark_of_more_than_one_number_is_not_a_mark() {
        let d = dir("toolong");
        let p = marked(&d.0, "many", &format!("{} 9\n", "1".repeat(40)));
        assert_eq!(read_mark(&p), None);
    }

    #[test]
    fn the_words_and_the_exit_statuses() {
        assert_eq!(Verdict::Ok.word(), "ok");
        assert_eq!(Verdict::Stalled.word(), "stalled");
        assert_eq!(Verdict::Over.word(), "over");
        assert_eq!(Verdict::Unknown.word(), "unknown");
        assert_eq!(
            [
                Verdict::Ok.exit(),
                Verdict::Stalled.exit(),
                Verdict::Over.exit(),
                Verdict::Unknown.exit()
            ],
            [0, 1, 2, 3]
        );
    }

    #[test]
    fn a_host_uptime_file_is_read_as_the_clock() {
        // boot_seconds reads a file of its own path; the parser and the
        // regular-file rule are both covered above. What the host's own
        // /proc/uptime must at least give is a number that is not nothing.
        if Path::new("/proc/uptime").is_file() {
            assert!(boot_seconds().is_some());
        }
    }
}
