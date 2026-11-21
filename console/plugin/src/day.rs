//! The day's model tokens of all plugins together, `TEHAS_PLUGIN_MODEL_DAY`.
//! A day is a UTC day. The socket asks [`Day`]; the state store can keep the
//! total later by implementing it.

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tehas_core::file::{side_file, write_atomic, Temp};

/// The day's total, which a reserve is held against before a request is sent.
pub trait Day: Send + Sync {
    /// Adds `tokens` to today's total when it stays within `limit`. False,
    /// and nothing added, when it would pass it or the total cannot be read.
    fn reserve(&self, tokens: u64, limit: u64) -> bool;
    /// Replaces a reserve of `reserved` with the `used` the answer gave.
    fn settle(&self, reserved: u64, used: u64);
}

/// Days since 1970-01-01, UTC.
pub fn today() -> i64 {
    tehas_core::time::now().div_euclid(86_400)
}

// The total after a reserve, or None when it would pass the limit.
fn reserved(total: u64, tokens: u64, limit: u64) -> Option<u64> {
    total.checked_add(tokens).filter(|t| *t <= limit)
}

fn settled(total: u64, reserved: u64, used: u64) -> u64 {
    total.saturating_sub(reserved).saturating_add(used)
}

/// The total in this process's memory: for one program that runs all the
/// day's requests, and for tests.
pub struct MemoryDay {
    today: fn() -> i64,
    // (day, tokens)
    total: Mutex<(i64, u64)>,
}

impl MemoryDay {
    pub fn new(today: fn() -> i64) -> MemoryDay {
        MemoryDay {
            today,
            total: Mutex::new((today(), 0)),
        }
    }

    /// Today's total.
    pub fn used(&self) -> u64 {
        let mut t = self.total.lock().unwrap_or_else(|e| e.into_inner());
        roll(&mut t, (self.today)());
        t.1
    }
}

fn roll(t: &mut (i64, u64), day: i64) {
    if t.0 != day {
        *t = (day, 0);
    }
}

impl Day for MemoryDay {
    fn reserve(&self, tokens: u64, limit: u64) -> bool {
        let mut t = self.total.lock().unwrap_or_else(|e| e.into_inner());
        roll(&mut t, (self.today)());
        match reserved(t.1, tokens, limit) {
            Some(n) => {
                t.1 = n;
                true
            }
            None => false,
        }
    }

    fn settle(&self, reserved: u64, used: u64) {
        let mut t = self.total.lock().unwrap_or_else(|e| e.into_inner());
        roll(&mut t, (self.today)());
        t.1 = settled(t.1, reserved, used);
    }
}

/// The total in a file, `<day> <tokens>`, shared by the runs of several
/// processes under a lock on `<file>.lock`. A file that is not there is a
/// total of 0; one that cannot be read refuses every reserve.
pub struct FileDay {
    path: PathBuf,
    today: fn() -> i64,
}

impl FileDay {
    pub fn new(path: &Path, today: fn() -> i64) -> FileDay {
        FileDay {
            path: path.to_path_buf(),
            today,
        }
    }

    // Today's total, None when the file cannot be read.
    fn read(&self) -> Option<u64> {
        let text = match fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Some(0),
            Err(_) => return None,
        };
        let (day, tokens) = text.trim_end_matches('\n').split_once(' ')?;
        let (day, tokens) = (day.parse::<i64>().ok()?, tokens.parse::<u64>().ok()?);
        Some(if day == (self.today)() { tokens } else { 0 })
    }

    fn write(&self, tokens: u64) -> bool {
        let line = format!("{} {tokens}\n", (self.today)());
        write_atomic(&self.path, line.as_bytes(), 0o600, &Temp::Pid).is_ok()
    }

    // The total changed by `change` under the lock: false when the lock, the
    // read or the write fails, or `change` refuses.
    fn change(&self, change: impl FnOnce(u64) -> Option<u64>) -> bool {
        let Ok(lock) = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(side_file(&self.path, ".lock"))
        else {
            return false;
        };
        if lock.lock().is_err() {
            return false;
        }
        let done = self
            .read()
            .and_then(change)
            .is_some_and(|tokens| self.write(tokens));
        drop::<File>(lock);
        done
    }

    /// Today's total, None when the file cannot be read.
    pub fn used(&self) -> Option<u64> {
        self.read()
    }
}

impl Day for FileDay {
    fn reserve(&self, tokens: u64, limit: u64) -> bool {
        self.change(|total| reserved(total, tokens, limit))
    }

    fn settle(&self, reserved: u64, used: u64) {
        self.change(|total| Some(settled(total, reserved, used)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI64, Ordering};
    use tehas_core::testdir::TestDir;

    static DAY: AtomicI64 = AtomicI64::new(20_000);
    fn clock() -> i64 {
        DAY.load(Ordering::Relaxed)
    }
    fn fixed() -> i64 {
        20_000
    }

    #[test]
    fn a_reserve_within_the_limit_is_held_and_one_past_it_is_not() {
        let d = MemoryDay::new(fixed);
        assert!(d.reserve(60, 100));
        assert!(!d.reserve(41, 100));
        assert_eq!(d.used(), 60);
        assert!(d.reserve(40, 100));
        assert_eq!(d.used(), 100);
    }

    #[test]
    fn the_answer_s_usage_replaces_the_reserve() {
        let d = MemoryDay::new(fixed);
        assert!(d.reserve(60, 100));
        d.settle(60, 25);
        assert_eq!(d.used(), 25);
        d.settle(0, 0);
        assert_eq!(d.used(), 25);
    }

    #[test]
    fn a_file_keeps_the_total_across_its_readers_and_starts_again_each_day() {
        let dir = TestDir::new("day");
        let path = dir.join("model-day");
        let a = FileDay::new(&path, clock);
        assert_eq!(a.used(), Some(0));
        assert!(a.reserve(70, 100));
        a.settle(70, 50);
        let b = FileDay::new(&path, clock);
        assert_eq!(b.used(), Some(50));
        assert!(!b.reserve(51, 100));
        assert!(b.reserve(50, 100));
        assert_eq!(fs::read_to_string(&path).unwrap(), "20000 100\n");
        DAY.store(20_001, Ordering::Relaxed);
        assert_eq!(a.used(), Some(0));
        assert!(a.reserve(100, 100));
    }

    #[test]
    fn a_file_that_cannot_be_read_refuses_every_reserve() {
        let dir = TestDir::new("day-bad");
        let path = dir.join("model-day");
        fs::write(&path, "not a total\n").unwrap();
        let d = FileDay::new(&path, fixed);
        assert_eq!(d.used(), None);
        assert!(!d.reserve(1, 100));
        assert_eq!(fs::read_to_string(&path).unwrap(), "not a total\n");
    }
}
