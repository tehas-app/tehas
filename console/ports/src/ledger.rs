//! The shared budget and health state, across processes, keyed by host and
//! holder: `Ledger`, `HostId`, `Holder`, `HolderId`, `Budget`,
//! `HealthState`, `Failure`, `LedgerDown` and `Whose`.

use serde::{Deserialize, Serialize};

use crate::ids::opaque;
use crate::{AccountId, At, Outcome, Passing, Pool, SiteId};

opaque!(
    /// One supplier's host, as the adapter names it: every project on it
    /// shares its health, and every holder on it its budget. Not a project:
    /// that is `SiteId`, which keys tokens alone.
    HostId
);

/// Whose budget a call spends. The adapter makes the ids. `Ours` is the
/// factory's account as the site budgets it, which may serve every project
/// its grant covers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Holder {
    /// The factory's account, as the site budgets it.
    Ours(HolderId),
    /// A person, by the supplier's id of their account.
    Person(AccountId),
    /// A caller's address, kept as the adapter's sum of it.
    Anonymous(HolderId),
}

opaque!(
    /// A holder of a budget, as the adapter names it.
    HolderId
);

/// What is left of one holder's pool, and when it is full again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Budget {
    /// How many calls or points are left.
    pub left: u32,
    /// When the pool is full again.
    pub resets: At,
}

new!(Budget {
    left: u32,
    resets: At
});

/// The health of a host, or of one holder on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HealthState {
    /// Calls go through.
    Healthy,
    /// The host is out: its calls fail with a passing failure.
    Failing {
        /// When the first failure was seen.
        since: At,
        /// What failed.
        why: Passing,
    },
    /// The holder's pool is out until then.
    Spent {
        /// The pool that is out.
        pool: Pool,
        /// When it has budget again.
        until: At,
    },
    /// The holder's credential is no longer good.
    SignedOut {
        /// When the site first said so.
        since: At,
    },
}

/// A failed call, for the health check: when, the project it was for, and
/// its outcome, whose fixed line is `outcome.line()`. Read back whole by
/// another process.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Failure {
    /// When the call failed.
    pub at: At,
    /// The project it was for.
    pub site: SiteId,
    /// What stopped it.
    pub outcome: Outcome,
}

new!(Failure {
    at: At,
    site: SiteId,
    outcome: Outcome
});

/// The ledger could not be read or written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LedgerDown;

/// Whose health a state is, and so which states a caller keeps under it:
/// the host's own (an outage: `Healthy` or `Failing`), one holder's
/// credential on that host (`Healthy` or `SignedOut`), or one pool of one
/// holder on that host (`Healthy` or `Spent` of that pool).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Whose {
    /// The host's own health: `Healthy` or `Failing`.
    Host,
    /// One holder's credential on that host: `Healthy` or `SignedOut`.
    Holder(Holder),
    /// One pool of one holder on that host: `Healthy` or `Spent` of that
    /// pool.
    Pool(Holder, Pool),
}

/// The budget and the health state, shared by every process and factory
/// that calls one host. The caller gives it to the adapter at wiring. A
/// ledger that fails answers `LedgerDown`, and the adapter then fails
/// toward reporting.
///
/// It keeps health for the host, and for each holder and each pool of a
/// holder whose state is other than `Healthy`. It may drop a `Healthy`
/// entry, and `health` answers `Healthy` for one it does not keep, so a
/// ledger does not grow with each anonymous caller. A person's `SignedOut`
/// notice goes to the `Notices` sink beside `Keyring::signed_out`, never as
/// a host message.
pub trait Ledger: Send + Sync {
    /// What is left of a holder's pool on a host, or None if unknown.
    fn budget(
        &self,
        host: &HostId,
        holder: &Holder,
        pool: Pool,
    ) -> Result<Option<Budget>, LedgerDown>;
    /// Keep what is left of a holder's pool on a host.
    fn put_budget(
        &self,
        host: &HostId,
        holder: &Holder,
        pool: Pool,
        b: Budget,
    ) -> Result<(), LedgerDown>;
    /// The health of a host, or of one holder or pool on it. `Healthy` if
    /// none is kept.
    fn health(&self, host: &HostId, whose: &Whose) -> Result<HealthState, LedgerDown>;
    /// Write `new` only if the state is still `expect`. True if written.
    fn swap_health(
        &self,
        host: &HostId,
        whose: &Whose,
        expect: &HealthState,
        new: &HealthState,
    ) -> Result<bool, LedgerDown>;
    /// Keep one failed call, with the project it was for. Failures older
    /// than `f.at - keep` are dropped, `keep` in seconds. It keeps what it is
    /// given: the adapter chooses which failures count. It hands over only a
    /// failure of the supplier: `Unavailable` but `Broken`, `Spent`, and the
    /// factory's own `SignedOut`; an odd answer (`Unread`, `Cut` and
    /// `TooLarge` among them) and another holder's sign-out are not handed
    /// over.
    fn failed(&self, host: &HostId, f: Failure, keep: u32) -> Result<(), LedgerDown>;
    /// The failed calls on a host at or after `since`, oldest first.
    fn failures(&self, host: &HostId, since: At) -> Result<Vec<Failure>, LedgerDown>;
}
