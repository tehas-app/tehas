//! `MemoryLedger`: a `Ledger` in memory, shared by every handle of one
//! process, which a test can make fail.

use std::sync::Mutex;

use super::state::lock;
use crate::{At, Budget, Failure, HealthState, Holder, HostId, Ledger, LedgerDown, Pool, Whose};

/// A `Ledger` in memory. `swap_health` compares and writes under one lock,
/// so it is atomic within the process. Made to fail, every call answers
/// `LedgerDown` and nothing is kept.
#[derive(Debug, Default)]
pub struct MemoryLedger {
    inner: Mutex<Kept>,
}

#[derive(Debug, Default)]
struct Kept {
    budgets: Vec<((HostId, Holder, Pool), Budget)>,
    health: Vec<((HostId, Whose), HealthState)>,
    failures: Vec<(HostId, Failure)>,
    down: bool,
}

impl MemoryLedger {
    /// An empty ledger that works.
    pub fn new() -> Self {
        Self::default()
    }

    /// Make every call answer `LedgerDown`, or work again.
    pub fn set_down(&self, down: bool) {
        lock(&self.inner).down = down;
    }

    /// Every state it keeps, other than `Healthy`, in the order first kept.
    pub fn states(&self) -> Vec<(HostId, Whose, HealthState)> {
        lock(&self.inner)
            .health
            .iter()
            .map(|((h, w), s)| (h.clone(), w.clone(), *s))
            .collect()
    }

    fn up(&self) -> Result<std::sync::MutexGuard<'_, Kept>, LedgerDown> {
        let kept = lock(&self.inner);
        if kept.down {
            Err(LedgerDown)
        } else {
            Ok(kept)
        }
    }
}

impl Ledger for MemoryLedger {
    fn budget(
        &self,
        host: &HostId,
        holder: &Holder,
        pool: Pool,
    ) -> Result<Option<Budget>, LedgerDown> {
        let kept = self.up()?;
        Ok(kept
            .budgets
            .iter()
            .find(|((h, o, p), _)| h == host && o == holder && *p == pool)
            .map(|(_, b)| *b))
    }

    fn put_budget(
        &self,
        host: &HostId,
        holder: &Holder,
        pool: Pool,
        b: Budget,
    ) -> Result<(), LedgerDown> {
        let mut kept = self.up()?;
        match kept
            .budgets
            .iter_mut()
            .find(|((h, o, p), _)| h == host && o == holder && *p == pool)
        {
            Some((_, old)) => *old = b,
            None => kept.budgets.push(((host.clone(), holder.clone(), pool), b)),
        }
        Ok(())
    }

    fn health(&self, host: &HostId, whose: &Whose) -> Result<HealthState, LedgerDown> {
        let kept = self.up()?;
        Ok(kept
            .health
            .iter()
            .find(|((h, w), _)| h == host && w == whose)
            .map_or(HealthState::Healthy, |(_, s)| *s))
    }

    /// A `Healthy` state written is dropped, not kept.
    fn swap_health(
        &self,
        host: &HostId,
        whose: &Whose,
        expect: &HealthState,
        new: &HealthState,
    ) -> Result<bool, LedgerDown> {
        let mut kept = self.up()?;
        let at = kept
            .health
            .iter()
            .position(|((h, w), _)| h == host && w == whose);
        let now = at.map_or(HealthState::Healthy, |i| kept.health[i].1);
        if now != *expect {
            return Ok(false);
        }
        match (at, new) {
            (Some(i), HealthState::Healthy) => {
                kept.health.remove(i);
            }
            (Some(i), _) => kept.health[i].1 = *new,
            (None, HealthState::Healthy) => {}
            (None, _) => kept.health.push(((host.clone(), whose.clone()), *new)),
        }
        Ok(true)
    }

    fn failed(&self, host: &HostId, f: Failure, keep: u32) -> Result<(), LedgerDown> {
        let mut kept = self.up()?;
        let oldest = f.at.secs() - i64::from(keep);
        kept.failures
            .retain(|(h, old)| h != host || old.at.secs() >= oldest);
        kept.failures.push((host.clone(), f));
        Ok(())
    }

    fn failures(&self, host: &HostId, since: At) -> Result<Vec<Failure>, LedgerDown> {
        let kept = self.up()?;
        let mut all: Vec<Failure> = kept
            .failures
            .iter()
            .filter(|(h, f)| h == host && f.at >= since)
            .map(|(_, f)| f.clone())
            .collect();
        all.sort_by_key(|f| f.at);
        Ok(all)
    }
}
