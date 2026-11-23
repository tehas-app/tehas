//! The code host's `Ledger` as one file under `state/`: the budgets, health
//! states and failed calls every process of this factory shares
//! (`State::Ledger`, `state/ledger.json`). A write is a read, a change and an
//! atomic replace, under `state/ledger.lock`; a read reads the file whole,
//! which a replace keeps whole.
//!
//! A missing `state/` is `LedgerDown`. A missing file in it is an empty ledger,
//! made on the first write. A file that is not whole JSON of this version is
//! `LedgerDown` and is never reset here: [`FileLedger::why_down`] says how a
//! person recovers. `Healthy` states are not kept, and `failed` keeps what it
//! is given: the adapter chooses which failures count.

use crate::file::{write_atomic, Temp};
use crate::home::{Home, State};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, TryLockError};
use std::io::ErrorKind;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tehas_ports::{
    At, Budget, Failure, HealthState, Holder, HostId, Ledger, LedgerDown, Pool, Whose,
};

/// The version of the file's shape.
const VERSION: u32 = 1;
/// How long a write waits for another to finish.
const WAIT: Duration = Duration::from_secs(5);
/// The pause between two tries of the lock.
const STEP: Duration = Duration::from_millis(10);

/// The ledger of one tehas home.
#[derive(Debug, Clone)]
pub struct FileLedger {
    file: PathBuf,
    lock: PathBuf,
    wait: Duration,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Kept {
    version: u32,
    budgets: Vec<BudgetRow>,
    health: Vec<HealthRow>,
    failures: Vec<FailureRow>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BudgetRow {
    host: HostId,
    holder: Holder,
    pool: Pool,
    budget: Budget,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HealthRow {
    host: HostId,
    whose: Whose,
    state: HealthState,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FailureRow {
    host: HostId,
    failure: Failure,
}

/// Why the file could not be read.
enum Unread {
    NoState,
    NotWhole,
    Io(std::io::Error),
}

impl FileLedger {
    /// The ledger of `home`.
    pub fn new(home: &Home) -> FileLedger {
        FileLedger {
            file: home.state(State::Ledger),
            lock: home.state(State::LedgerLock),
            wait: WAIT,
        }
    }

    /// The same ledger with a write's wait for the lock of `wait`.
    pub fn with_wait(self, wait: Duration) -> FileLedger {
        FileLedger { wait, ..self }
    }

    /// None when the ledger can be read, else the line a person reads: what
    /// is wrong and how to recover. The health check shows it.
    pub fn why_down(&self) -> Option<String> {
        let file = self.file.display();
        match self.read() {
            Ok(_) => None,
            Err(Unread::NoState) => Some(format!(
                "{file} cannot be read: its directory is missing; reinstall tehas"
            )),
            Err(Unread::NotWhole) => Some(format!(
                "{file} is not a ledger this tehas reads: remove it; it holds only failed calls and budgets, which build up again"
            )),
            Err(Unread::Io(e)) => Some(format!("{file} cannot be read: {e}")),
        }
    }

    fn read(&self) -> Result<Kept, Unread> {
        match fs::read(&self.file) {
            Ok(bytes) => serde_json::from_slice::<Kept>(&bytes)
                .ok()
                .filter(|k| k.version == VERSION)
                .ok_or(Unread::NotWhole),
            Err(e) if e.kind() == ErrorKind::NotFound => match self.file.parent() {
                Some(dir) if dir.is_dir() => Ok(Kept {
                    version: VERSION,
                    ..Kept::default()
                }),
                _ => Err(Unread::NoState),
            },
            Err(e) => Err(Unread::Io(e)),
        }
    }

    fn kept(&self) -> Result<Kept, LedgerDown> {
        self.read().map_err(|_| LedgerDown)
    }

    /// Change the kept file under its lock, and answer what `change` did.
    fn change<T>(&self, change: impl FnOnce(&mut Kept) -> T) -> Result<T, LedgerDown> {
        let _held = self.locked()?;
        let mut kept = self.kept()?;
        let answer = change(&mut kept);
        let bytes = serde_json::to_vec(&kept).map_err(|_| LedgerDown)?;
        write_atomic(&self.file, &bytes, 0o600, &Temp::Pid).map_err(|_| LedgerDown)?;
        Ok(answer)
    }

    /// The lock, held until the file is dropped; `LedgerDown` past the wait.
    fn locked(&self) -> Result<File, LedgerDown> {
        if !self.lock.parent().is_some_and(|d| d.is_dir()) {
            return Err(LedgerDown);
        }
        let f = File::create(&self.lock).map_err(|_| LedgerDown)?;
        let started = Instant::now();
        loop {
            match f.try_lock() {
                Ok(()) => return Ok(f),
                Err(TryLockError::WouldBlock) if started.elapsed() < self.wait => {
                    std::thread::sleep(STEP);
                }
                Err(_) => return Err(LedgerDown),
            }
        }
    }
}

impl Ledger for FileLedger {
    fn budget(
        &self,
        host: &HostId,
        holder: &Holder,
        pool: Pool,
    ) -> Result<Option<Budget>, LedgerDown> {
        Ok(self
            .kept()?
            .budgets
            .into_iter()
            .find(|r| r.host == *host && r.holder == *holder && r.pool == pool)
            .map(|r| r.budget))
    }

    fn put_budget(
        &self,
        host: &HostId,
        holder: &Holder,
        pool: Pool,
        b: Budget,
    ) -> Result<(), LedgerDown> {
        self.change(|k| {
            match k
                .budgets
                .iter_mut()
                .find(|r| r.host == *host && r.holder == *holder && r.pool == pool)
            {
                Some(r) => r.budget = b,
                None => k.budgets.push(BudgetRow {
                    host: host.clone(),
                    holder: holder.clone(),
                    pool,
                    budget: b,
                }),
            }
        })
    }

    fn health(&self, host: &HostId, whose: &Whose) -> Result<HealthState, LedgerDown> {
        Ok(self
            .kept()?
            .health
            .into_iter()
            .find(|r| r.host == *host && r.whose == *whose)
            .map_or(HealthState::Healthy, |r| r.state))
    }

    fn swap_health(
        &self,
        host: &HostId,
        whose: &Whose,
        expect: &HealthState,
        new: &HealthState,
    ) -> Result<bool, LedgerDown> {
        self.change(|k| {
            let at = k
                .health
                .iter()
                .position(|r| r.host == *host && r.whose == *whose);
            let now = at.map_or(HealthState::Healthy, |i| k.health[i].state);
            if now != *expect {
                return false;
            }
            match (at, new) {
                (Some(i), HealthState::Healthy) => {
                    k.health.remove(i);
                }
                (Some(i), _) => k.health[i].state = *new,
                (None, HealthState::Healthy) => {}
                (None, _) => k.health.push(HealthRow {
                    host: host.clone(),
                    whose: whose.clone(),
                    state: *new,
                }),
            }
            true
        })
    }

    fn failed(&self, host: &HostId, f: Failure, keep: u32) -> Result<(), LedgerDown> {
        self.change(|k| {
            let oldest = f.at.secs() - i64::from(keep);
            k.failures
                .retain(|r| r.host != *host || r.failure.at.secs() >= oldest);
            k.failures.push(FailureRow {
                host: host.clone(),
                failure: f,
            });
        })
    }

    fn failures(&self, host: &HostId, since: At) -> Result<Vec<Failure>, LedgerDown> {
        let mut all: Vec<Failure> = self
            .kept()?
            .failures
            .into_iter()
            .filter(|r| r.host == *host && r.failure.at >= since)
            .map(|r| r.failure)
            .collect();
        all.sort_by_key(|f| f.at);
        Ok(all)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdir::TestDir;
    use tehas_ports::laws;

    fn home(name: &str) -> (TestDir, Home) {
        let d = TestDir::new(name);
        let h = Home::new(d.path());
        fs::create_dir_all(h.state_dir()).unwrap();
        (d, h)
    }

    fn host() -> HostId {
        HostId::new("github.com")
    }

    #[test]
    fn the_file_keeps_the_ledgers_laws() {
        let (_d, h) = home("ledger-laws");
        let l = FileLedger::new(&h);
        if let Err(e) = laws::ledger::all(&l, &host(), At::new(1_000_000)) {
            panic!("{}: {}", e.law, e.what);
        }
    }

    // Another process, another handle: what one wrote the next reads.
    #[test]
    fn a_second_handle_reads_what_the_first_wrote() {
        let (_d, h) = home("ledger-two");
        let failing = HealthState::Failing {
            since: At::new(5),
            why: tehas_ports::Passing::Server,
        };
        let one = FileLedger::new(&h);
        assert_eq!(
            one.swap_health(&host(), &Whose::Host, &HealthState::Healthy, &failing),
            Ok(true)
        );
        let two = FileLedger::new(&h);
        assert_eq!(two.health(&host(), &Whose::Host), Ok(failing));
        assert_eq!(
            two.swap_health(&host(), &Whose::Host, &HealthState::Healthy, &failing),
            Ok(false)
        );
    }

    #[test]
    fn a_missing_file_is_empty_and_made_on_the_first_write() {
        let (_d, h) = home("ledger-new");
        let l = FileLedger::new(&h);
        assert_eq!(l.health(&host(), &Whose::Host), Ok(HealthState::Healthy));
        assert_eq!(l.failures(&host(), At::new(0)), Ok(vec![]));
        assert!(!h.state(State::Ledger).exists(), "a read makes no file");
        assert_eq!(l.why_down(), None);
        let b = Budget {
            left: 1,
            resets: At::new(9),
        };
        let ours = Holder::Ours(tehas_ports::HolderId::new("1"));
        assert_eq!(l.put_budget(&host(), &ours, Pool::Calls, b), Ok(()));
        assert!(h.state(State::Ledger).exists());
        assert_eq!(l.budget(&host(), &ours, Pool::Calls), Ok(Some(b)));
    }

    // A Healthy state is dropped, so the file holds only what is wrong.
    #[test]
    fn a_healthy_state_is_not_kept_in_the_file() {
        let (_d, h) = home("ledger-healthy");
        let l = FileLedger::new(&h);
        let signed_out = HealthState::SignedOut { since: At::new(1) };
        let whose = Whose::Holder(Holder::Anonymous(tehas_ports::HolderId::new("a")));
        assert_eq!(
            l.swap_health(&host(), &whose, &HealthState::Healthy, &signed_out),
            Ok(true)
        );
        assert_eq!(
            l.swap_health(&host(), &whose, &signed_out, &HealthState::Healthy),
            Ok(true)
        );
        let text = fs::read_to_string(h.state(State::Ledger)).unwrap();
        assert!(text.contains(r#""health":[]"#), "{text}");
    }

    #[test]
    fn no_state_directory_is_down() {
        let d = TestDir::new("ledger-nostate");
        let h = Home::new(d.path());
        let l = FileLedger::new(&h);
        assert_eq!(l.health(&host(), &Whose::Host), Err(LedgerDown));
        assert_eq!(
            l.swap_health(
                &host(),
                &Whose::Host,
                &HealthState::Healthy,
                &HealthState::Healthy
            ),
            Err(LedgerDown)
        );
        let why = l.why_down().unwrap();
        let file = h.state(State::Ledger).display().to_string();
        assert_eq!(
            why,
            format!("{file} cannot be read: its directory is missing; reinstall tehas")
        );
        assert!(!h.state_dir().exists(), "nothing is made");
    }

    // A file this tehas cannot read stays as it is, every call is down, and
    // the line names the file and the way out.
    #[test]
    fn a_file_that_is_not_whole_is_down_left_alone_and_says_how_to_recover() {
        for text in [
            "{\"version\":1,\"budgets\":[",
            "not json",
            r#"{"version":2,"budgets":[],"health":[],"failures":[]}"#,
            r#"{"version":1,"budgets":[],"health":[],"failures":[],"more":1}"#,
        ] {
            let (_d, h) = home("ledger-broken");
            fs::write(h.state(State::Ledger), text).unwrap();
            let l = FileLedger::new(&h);
            assert_eq!(l.health(&host(), &Whose::Host), Err(LedgerDown), "{text}");
            assert_eq!(l.failures(&host(), At::new(0)), Err(LedgerDown), "{text}");
            let f = Failure {
                at: At::new(1),
                site: tehas_ports::SiteId::new("s"),
                outcome: tehas_ports::Outcome::NotFound,
            };
            assert_eq!(l.failed(&host(), f, 60), Err(LedgerDown), "{text}");
            assert_eq!(fs::read_to_string(h.state(State::Ledger)).unwrap(), text);
            let file = h.state(State::Ledger).display().to_string();
            assert_eq!(
                l.why_down(),
                Some(format!("{file} is not a ledger this tehas reads: remove it; it holds only failed calls and budgets, which build up again")),
                "{text}"
            );
            fs::remove_file(h.state(State::Ledger)).unwrap();
            assert_eq!(l.why_down(), None, "removed, it is empty again");
        }
    }

    // A write waits for another only so long; a read takes no lock.
    #[test]
    fn a_write_past_the_locks_wait_is_down_and_a_read_goes_on() {
        let (_d, h) = home("ledger-lock");
        let held = File::create(h.state(State::LedgerLock)).unwrap();
        held.lock().unwrap();
        let l = FileLedger::new(&h).with_wait(Duration::ZERO);
        let ours = Holder::Ours(tehas_ports::HolderId::new("1"));
        let b = Budget {
            left: 1,
            resets: At::new(9),
        };
        assert_eq!(
            l.put_budget(&host(), &ours, Pool::Calls, b),
            Err(LedgerDown)
        );
        assert_eq!(l.budget(&host(), &ours, Pool::Calls), Ok(None));
        held.unlock().unwrap();
        assert_eq!(l.put_budget(&host(), &ours, Pool::Calls, b), Ok(()));
    }
}
