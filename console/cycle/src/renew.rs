//! The GitHub token renewer of a cycle (#1079): `tehas-cycle renew --cmd
//! <path> --every <s> --retry <s>` runs `<path>` for the life of a cycle, so
//! the token a container reads under `state/github` does not run out at the
//! top of its hour.
//!
//! The bash loop of `lib/github-app.sh` it replaces asked every
//! `TEHAS_GITHUB_RENEW_EVERY` seconds and threw away what the token command
//! said, so two failed asks in a row let the token expire and nothing was
//! logged. Here a failed ask is one line on standard error, which the cycle's
//! work log keeps, and the next ask comes after `--retry` seconds instead of
//! waiting the whole of `--every`.
//!
//! The clock is read through `Now`, which the tests replace: a test says what
//! the clock reads, so no test waits for a real second.

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// The clock the waits are counted on: whole seconds. The real one is the
/// wall clock; a test's own says what the case wants it to read.
pub type Now = fn() -> u64;

/// The wall clock, as `date +%s` prints it: what the waits of a real run are
/// counted on. The workspace's clock gives 0 for a clock set before 1970,
/// which only ever makes a wait end at once.
pub fn epoch_seconds() -> u64 {
    tehas_core::time::now().max(0) as u64
}

/// When the next ask is made after one that ended `ok`: the whole of `every`
/// seconds after a success, `retry` after a failure.
pub fn next_wait(ok: bool, every: u64, retry: u64) -> u64 {
    if ok {
        every
    } else {
        retry
    }
}

/// The one line a failed ask writes. It names the wait that follows and
/// nothing else: the command's own output is never part of it, because a
/// token must not reach a log.
fn failure_line(wait: u64) -> String {
    format!("tehas: the GitHub token was not renewed, again in {wait}s\n")
}

/// How a wait is stood still: a step at a time, so a signal is seen soon
/// after it arrives. A step of the wall and a reading of the clock are asked
/// for only while a wait lasts, so the reads of the clock are the standing
/// still of it, and a test's clock that does not move leaves the wait standing
/// still with it.
const STEP: Duration = Duration::from_millis(50);

/// Runs `cmd` once, with its standard input closed and its output discarded;
/// its error stream is the caller's own, so the reason a token command gives
/// reaches the cycle's work log. True when it ended without a status.
fn ask(cmd: &Path) -> bool {
    match Command::new(cmd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
    {
        Ok(mut child) => child.wait().map(|s| s.success()).unwrap_or(false),
        Err(_) => false,
    }
}

/// Asks `cmd` for a token, waits, and asks again, while `stop` is not set:
/// the whole of `every` seconds after a success and `retry` after a failure,
/// with one line on standard error for each failure. It returns as soon as
/// `stop` is set, which is what makes the verb end 0 on TERM.
pub fn renew(cmd: &Path, every: u64, retry: u64, stop: &AtomicBool, now: Now) {
    loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let ok = ask(cmd);
        let wait = next_wait(ok, every, retry);
        if !ok {
            eprint!("{}", failure_line(wait));
        }
        stand_still(wait, stop, now);
    }
}

/// Stands still for `seconds` of the clock, a step at a time, and ends at
/// once when `stop` is set. A wait of nothing asks nothing of the clock.
fn stand_still(seconds: u64, stop: &AtomicBool, now: Now) {
    if seconds == 0 {
        return;
    }
    // One reading of the clock gives the bound, and a reading is asked for
    // once more before the first step and before every step after it, so the
    // reads of the clock are the standing still: a clock that stands still
    // leaves the wait standing still with it, and only the flag a signal sets
    // ends such a wait. A clock already past the bound ends it before a step
    // of the wall.
    let until = now().wrapping_add(seconds);
    let mut step = now();
    while step < until {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        std::thread::sleep(STEP);
        step = now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicUsize;
    use std::sync::{Arc, Mutex};
    use std::thread;

    /// A clock a case winds: the seconds it reads, and how many times it has
    /// been read. The loop reads it and counts its steps on it; the case winds
    /// it, so no test waits for a real second.
    #[derive(Clone, Default)]
    struct Clock {
        seconds: Arc<AtomicUsize>,
        reads: Arc<AtomicUsize>,
    }

    impl Clock {
        fn at(seconds: u64) -> Clock {
            Clock {
                seconds: Arc::new(AtomicUsize::new(seconds as usize)),
                reads: Arc::new(AtomicUsize::new(0)),
            }
        }

        /// What the loop reads: the count grows, and the clock says what it
        /// reads.
        fn read(&self) -> u64 {
            self.reads.fetch_add(1, Ordering::Relaxed);
            self.seconds.load(Ordering::Relaxed) as u64
        }

        /// The case's side: how often the loop has read the clock.
        fn reads(&self) -> usize {
            self.reads.load(Ordering::Relaxed)
        }

        /// The case's side: what the clock says now.
        fn seconds_now(&self) -> u64 {
            self.seconds.load(Ordering::Relaxed) as u64
        }

        /// The case's side: the clock goes on by `seconds`, so a wait counted
        /// on it comes to its bound and the loop asks again. The count of the
        /// reads is begun again with it, so what a case reads back is one
        /// wait's standing still.
        fn advance_by(&self, seconds: u64) {
            self.seconds.fetch_add(seconds as usize, Ordering::Relaxed);
            self.reads.store(0, Ordering::Relaxed);
        }
    }

    /// A command of the tests, alone in a directory of its own, emptied first
    /// so nothing of a past run of the file is read as this case's. It keeps
    /// its own count of its calls in a file there, so a case reads the
    /// command's account and not the loop's, and so does a thread of the case.
    struct Cmd {
        path: PathBuf,
        dir: PathBuf,
    }

    /// The name of the case: Rust names each test's thread after the test,
    /// which no other thread of the run is named as.
    fn case_name() -> String {
        let here = thread::current();
        here.name().unwrap_or("the case").to_string()
    }

    fn command(rc: u8) -> Cmd {
        // A directory of the case's own name with the process in front of it,
        // emptied first so nothing of a past run of the file is read as this
        // case's.
        let dir = std::env::temp_dir().join(format!(
            "renew-{}-{}-{}-cmd",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            case_name().replace([':', ' '], "_")
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        new_command(&dir, "cmd", rc)
    }

    /// A command at `name` in `dir`, of the same shape as the one above, so a
    /// case may put one there and leave another path in the directory empty.
    fn new_command(dir: &Path, name: &str, rc: u8) -> Cmd {
        let path = dir.join(name);
        let mut f = fs::File::create(&path).unwrap();
        // The command counts its calls in a file of its own name, so a path
        // with nothing at it counts nothing, as a command that is not there
        // must.
        writeln!(
            f,
            "#!/bin/sh\nn=$(cat '{record}.count' 2>/dev/null || echo 0)\necho $((n + 1)) >'{record}.count'\nprintf 'ghs_secret\\n'\nexit {rc}",
            record = path.display(),
        )
        .unwrap();
        drop(f);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        Cmd {
            path,
            dir: dir.to_path_buf(),
        }
    }

    impl Cmd {
        /// How often the command has run, from its own record.
        fn asks(&self) -> usize {
            fs::read_to_string(format!("{}.count", self.path.display()))
                .ok()
                .and_then(|t| t.trim().parse().ok())
                .unwrap_or(0)
        }
    }

    /// The number of the command of the case, so no two commands of a run
    /// share a directory, whatever name they are asked under.
    static NEXT: AtomicUsize = AtomicUsize::new(0);

    /// The clocks of the waits that run, by the name of the thread each wait
    /// runs on. The verb takes a plain `fn`, which captures nothing, so a
    /// clock is found by the thread that reads it rather than lent to it, and
    /// a wait of one case never reads the clock of another.
    static CLOCKS: Mutex<Vec<(String, Clock)>> = Mutex::new(Vec::new());

    fn clock_of(name: &str) -> Option<Clock> {
        CLOCKS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, c)| c.clone())
    }

    /// The name a thread of the case's carries while it waits: the case's own
    /// name with the process in front of it, which no thread of another case
    /// or another run of the file is named as.
    fn wait_name(kind: &str) -> String {
        format!("{}-{}-{}", kind, std::process::id(), case_name())
    }

    fn give(name: &str, clock: &Clock) {
        CLOCKS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((name.to_string(), clock.clone()));
    }

    fn take_back(name: &str) {
        CLOCKS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|(n, _)| n != name);
    }

    /// The clock of the wait running on this thread, read as the loop reads it.
    fn read_own_clock() -> u64 {
        own().read()
    }

    /// The clock given to the wait running on this thread.
    fn own() -> Clock {
        let here = thread::current();
        let name = here.name().unwrap_or("").to_string();
        clock_of(&name).expect("this thread waits on no clock")
    }

    /// The loop on a thread of its own, so the case winds its clock and sets
    /// the stop flag the signal handler sets while the loop stands still.
    struct Loop {
        cmd: Cmd,
        clock: Clock,
        stop: Arc<AtomicBool>,
        name: String,
        handle: Option<thread::JoinHandle<()>>,
    }

    fn running(cmd: &Cmd, every: u64, retry: u64, start: u64) -> Loop {
        let clock = Clock::at(start);
        let stop = Arc::new(AtomicBool::new(false));
        let name = wait_name("loop");
        give(&name, &clock);
        let (path, s, thread_name) = (cmd.path.clone(), Arc::clone(&stop), name.clone());
        let handle = thread::Builder::new()
            .name(thread_name.clone())
            .spawn(move || {
                renew(&path, every, retry, &s, read_own_clock);
                take_back(&thread_name);
            })
            .unwrap();
        Loop {
            cmd: Cmd {
                path: cmd.path.clone(),
                dir: cmd.dir.clone(),
            },
            clock,
            stop,
            name,
            handle: Some(handle),
        }
    }

    impl Loop {
        /// Waits for the command to have been asked `wanted` times, and says
        /// whether it got that far. The clock stands where the case left it,
        /// so a wait that is not over does not end, and the asks stop there.
        fn asked(&mut self, wanted: usize) -> bool {
            for _ in 0..100 {
                if self.cmd.asks() >= wanted || self.stopped() {
                    break;
                }
                thread::sleep(STEP);
            }
            self.cmd.asks() >= wanted
        }

        /// Waits for the loop to have asked `wanted` times with the clock going
        /// on the whole while, so a command that answers nothing and a command
        /// that is not there are asked for as often as one that fails.
        fn asked_again(&mut self, wanted: usize) -> bool {
            for _ in 0..100 {
                if self.clock.reads() >= wanted {
                    return true;
                }
                if self.stopped() {
                    break;
                }
                self.clock.advance_by(1);
                thread::sleep(STEP);
            }
            self.clock.reads() >= wanted
        }

        /// Whether the loop has returned, which it does as soon as the stop
        /// flag is set. The thread is joined once and the answer kept, so a
        /// later look does not join a thread that is no more.
        fn stopped(&mut self) -> bool {
            if self.handle.as_ref().is_some_and(|h| h.is_finished()) {
                let handle = self.handle.take().unwrap();
                let _ = handle.join();
            }
            self.handle.is_none()
        }
    }

    impl Drop for Loop {
        fn drop(&mut self) {
            // The loop ends before the case does, and its clock is taken back
            // so no case of a run reads it after its own case has ended.
            self.stop.store(true, Ordering::Relaxed);
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
            take_back(&self.name);
        }
    }

    /// `stand_still` for `seconds` on a clock of the case that reads `at`, on
    /// a thread of its own that carries the clock under a name of its own, and
    /// stopped by the flag the signal handler sets. `holds` is how many steps
    /// the helper stands before it sets the flag, so a wait that cannot end on
    /// its own is ended by the flag and not by the wall.
    ///
    /// The case reads back what the wait left on the clock: its seconds, and
    /// how often the clock was read while the wait lasted.
    fn stood(seconds: u64, at: u64, holds: usize) -> (u64, usize) {
        let clock = Clock::at(at);
        let name = wait_name("stand");
        give(&name, &clock);
        let stop = Arc::new(AtomicBool::new(false));
        let s = Arc::clone(&stop);
        let helper = thread::spawn(move || {
            for _ in 0..holds {
                thread::sleep(STEP);
            }
            s.store(true, Ordering::Relaxed);
        });
        let handle = thread::Builder::new()
            .name(name.clone())
            .spawn(move || stand_still(seconds, &stop, read_own_clock))
            .unwrap();
        handle.join().unwrap();
        helper.join().unwrap();
        take_back(&name);
        (clock.seconds_now(), clock.reads())
    }

    #[test]
    fn a_success_waits_the_whole_of_every_and_a_failure_only_the_retry() {
        assert_eq!(next_wait(true, 300, 30), 300);
        assert_eq!(next_wait(false, 300, 30), 30);
        assert_eq!(next_wait(true, 1, 1), 1);
        assert_eq!(next_wait(false, 1, 1), 1);
    }

    #[test]
    fn the_line_of_a_failure_names_the_wait_and_holds_nothing_else() {
        assert_eq!(
            failure_line(30),
            "tehas: the GitHub token was not renewed, again in 30s\n"
        );
        assert!(!failure_line(30).contains("ghs_"));
    }

    #[test]
    fn a_success_waits_the_whole_of_every_before_it_asks_again() {
        let cmd = command(0);
        let mut run = running(&cmd, 300, 30, 1000);
        // The clock stands at 1000 s, so the wait of 300 s is not over: one
        // ask, and the clock read for every step of standing still.
        assert!(run.asked(1));
        assert_eq!(run.cmd.asks(), 1);
        assert!(
            run.clock.reads() > 1,
            "the wait was not counted on the clock"
        );
        // The clock comes to the end of the wait, and the ask follows.
        run.clock.advance_by(300);
        assert!(run.asked(2));
        // The clock goes on 30 s more, over the retry and short of the whole
        // of `every`: the wait is not over, so no third ask comes.
        run.clock.advance_by(30);
        assert!(!run.asked(3));
    }

    #[test]
    fn a_failure_waits_the_retry_and_not_the_every() {
        let cmd = command(1);
        let mut run = running(&cmd, 300, 30, 1000);
        assert!(run.asked(1));
        // 30 s of the clock on, the retry is over and the loop asks again,
        // where a success would have waited 270 s more.
        run.clock.advance_by(30);
        assert!(run.asked(2));
    }

    #[test]
    fn a_failure_does_not_end_the_loop() {
        let cmd = command(1);
        let mut run = running(&cmd, 300, 0, 1000);
        // Ten asks of a command that fails every time: one line for each on
        // the verb's error stream, and the loop does not give up.
        assert!(run.asked(10));
    }

    #[test]
    fn a_command_that_cannot_be_run_is_a_failure_like_any_other() {
        // A command the loop cannot run: the ask of it fails at the gate, as
        // the ask of a command that is not there does, and it is a failure
        // like any other, so the loop goes on asking with the retry's wait
        // between asks and does not end for want of a command.
        let dir = std::env::temp_dir().join(format!(
            "renew-{}-{}-missing",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        // There of name but not of mode, so the ask of it is refused.
        let cannot = new_command(&dir, "cmd", 0);
        fs::set_permissions(&cannot.path, fs::Permissions::from_mode(0o644)).unwrap();
        let mut run = running(&cannot, 300, 1, 1000);
        // The retry's wait is what the clock is read for, so the reads of the
        // case's clock are the loop's turns at the command it cannot run: the
        // loop is on the retry's path from its first ask and does not stop for
        // want of a command.
        assert!(
            run.asked_again(3),
            "the loop gave up on a command it could not run"
        );
    }

    #[test]
    fn it_ends_at_once_when_a_signal_stopped_it() {
        let cmd = command(0);
        let stop = AtomicBool::new(true);
        renew(&cmd.path, 300, 30, &stop, || 1000);
        assert_eq!(cmd.asks(), 0);
    }

    #[test]
    fn a_wait_of_nothing_asks_nothing_of_the_clock() {
        // A wait of nothing ends before it reads the clock at all.
        let (seconds, reads) = stood(0, 1000, 10);
        assert_eq!((seconds, reads), (1000, 0));
    }

    #[test]
    fn a_wait_stands_still_until_the_clock_reaches_it() {
        // 30 s of the clock have not gone by, so the wait cannot end on its
        // own: the flag the signal handler sets is what ends it, and the reads
        // of the clock are the standing still.
        let (seconds, reads) = stood(30, 1000, 10);
        assert_eq!(seconds, 1000, "the wait ended on the clock, not the flag");
        assert!(reads > 1, "the wait was not counted on the clock");
    }

    #[test]
    fn a_wait_whose_clock_is_past_it_is_over_at_once() {
        let (seconds, reads) = stood(30, 1030, 0);
        assert_eq!(seconds, 1030);
        assert!(reads <= 3, "the wait stood still when it was over: {reads}");
    }

    #[test]
    fn the_clock_of_a_host_reads_the_wall_clock() {
        // A reading of the real clock is a number of this decade, so a wait
        // counted on it is a wait of seconds and not of epochs, and it does
        // not go backwards.
        let a = epoch_seconds();
        assert!(a > 1_700_000_000, "{a}");
        assert!(epoch_seconds() >= a);
    }
}
