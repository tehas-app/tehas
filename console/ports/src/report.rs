//! The per-pass reports: `Caller`, `PortName`, `Report` and `Reports`. The
//! meter's line, `MeterRow`, is in `site.rs`, beside `Site::meter`; what is
//! shared across processes is the `Ledger`'s.

use serde::{Deserialize, Serialize};

use crate::Outcome;

/// A caller's name for the meter: `"dispatch.poll"`, `"cycle.report"`. A
/// `&'static str`, so it is written out but never read back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct Caller(&'static str);

impl Caller {
    /// The caller of this name.
    pub const fn new(name: &'static str) -> Self {
        Self(name)
    }

    /// The name.
    pub fn as_str(self) -> &'static str {
        self.0
    }
}

/// Which port a report or a meter row is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum PortName {
    /// The tickets port.
    Tickets,
    /// The blueprints port.
    Blueprints,
    /// The code host port.
    CodeHost,
    /// Identity, of any realm.
    Identity,
    /// The joined reads.
    Board,
    /// A port this copy does not know.
    #[serde(other)]
    Unknown,
}

/// One kind of outcome a handle met, and how many calls met it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Report {
    /// The port of the calls.
    pub port: PortName,
    /// What stopped them.
    pub outcome: Outcome,
    /// How many calls met it.
    pub calls: u32,
}

new!(
    Report {
        port: PortName,
        outcome: Outcome
    },
    calls
);

/// What a handle met since the last take, for the pass to log once.
pub trait Reports {
    /// The reports since the last take, each kind once with its count.
    fn take_reports(&mut self) -> Vec<Report>;
}
