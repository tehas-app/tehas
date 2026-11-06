//! state/update.lock, held for the whole of `console update run`.
//!
//! install.sh of any version takes the lock only as a descriptor it inherited,
//! named by TEHAS_UPDATE_LOCK_FD, so the held descriptor has to reach it across
//! exec. Rust opens every file close-on-exec; rustix's safe `fcntl_setfd` clears
//! that for the spawn of install.sh alone and sets it again after. Any other
//! child is started with the flag set, so one that outlives the run does not
//! keep the lock. The descriptor is 7 or the first free one above it, the number
//! the script that held the lock before used.

use super::tell;
use rustix::io::{fcntl_dupfd_cloexec, fcntl_getfd, fcntl_setfd, FdFlags};
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

/// The descriptor the lock is held on, at least.
const FD: i32 = 7;

const POLL: Duration = Duration::from_millis(50);

pub(super) struct Held(File);

impl Held {
    /// The lock under `home`, waited for up to `wait` seconds. Err is the status:
    /// 1, with the line said.
    pub fn take(home: &Path, wait: u64) -> Result<Held, u8> {
        let file = home.join("state/update.lock");
        let opened = fs::create_dir_all(home.join("state"))
            .and_then(|()| OpenOptions::new().create(true).append(true).open(&file))
            .and_then(|f| fcntl_dupfd_cloexec(&f, FD).map_err(io::Error::from))
            .map(File::from);
        let Ok(lock) = opened else {
            tell(&format!(
                "tehas update: cannot open {}; nothing was changed",
                file.display()
            ));
            return Err(1);
        };
        let until = Instant::now() + Duration::from_secs(wait);
        loop {
            match lock.try_lock() {
                Ok(()) => return Ok(Held(lock)),
                Err(TryLockError::WouldBlock) if Instant::now() < until => sleep(POLL),
                Err(TryLockError::WouldBlock) => {
                    tell(&format!(
                        "tehas update: another update is running ({}), still held after {wait}s; nothing was changed",
                        file.display()
                    ));
                    return Err(1);
                }
                Err(TryLockError::Error(e)) => {
                    tell(&format!(
                        "tehas update: cannot lock {}: {e}; nothing was changed",
                        file.display()
                    ));
                    return Err(1);
                }
            }
        }
    }

    pub fn fd(&self) -> i32 {
        self.0.as_raw_fd()
    }

    /// Whether the next program this process starts is given the descriptor.
    pub fn inherit(&self, yes: bool) -> io::Result<()> {
        let mut flags = fcntl_getfd(&self.0)?;
        flags.set(FdFlags::CLOEXEC, !yes);
        Ok(fcntl_setfd(&self.0, flags)?)
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::dir;
    use super::*;

    fn cloexec(held: &Held) -> bool {
        fcntl_getfd(&held.0).unwrap().contains(FdFlags::CLOEXEC)
    }

    #[test]
    fn the_lock_is_held_at_7_or_above_and_closed_on_exec_until_asked() {
        let d = dir("lock-held");
        let held = Held::take(&d.0, 0).unwrap();
        assert!(held.fd() >= FD);
        assert!(cloexec(&held));
        held.inherit(true).unwrap();
        assert!(!cloexec(&held));
        held.inherit(false).unwrap();
        assert!(cloexec(&held));
    }

    #[test]
    fn a_held_lock_refuses_a_second_taker_after_its_wait() {
        let d = dir("lock-busy");
        let _first = Held::take(&d.0, 0).unwrap();
        let start = Instant::now();
        assert_eq!(Held::take(&d.0, 0).err(), Some(1));
        assert!(start.elapsed() < Duration::from_secs(1));
        assert_eq!(Held::take(&d.0, 1).err(), Some(1));
        assert!(start.elapsed() >= Duration::from_secs(1));
    }

    #[test]
    fn a_lock_let_go_is_taken_again() {
        let d = dir("lock-again");
        drop(Held::take(&d.0, 0).unwrap());
        // A child another test forks shares the dropped lock until it execs,
        // so the second take waits for the lock to come free.
        assert!(Held::take(&d.0, 10).is_ok());
    }

    #[test]
    fn a_home_where_no_state_can_be_made_is_refused() {
        let d = dir("lock-no-state");
        fs::write(d.0.join("state"), "").unwrap();
        assert_eq!(Held::take(&d.0, 0).err(), Some(1));
    }
}
