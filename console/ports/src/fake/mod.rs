//! The fakes every caller's tests run on, in tehas's words: `World`, an
//! in-memory site with a clock the test moves; `FakeSite`, a `Site` over a
//! `World`; `Faulty`, which wraps any site and answers chosen calls with an
//! outcome; `NoticeLog`, a sink that keeps what it was told; `MemoryLedger`
//! and `MemoryTokens`, a `Ledger` and a `TokenStore` in memory.
//!
//! Compiled always, std only, no effects: the clock is the test's, and no
//! answer depends on the machine it runs on.

mod blueprints;
mod code_host;
mod faulty;
mod ledger;
mod notices;
mod site;
mod spec;
mod state;
mod tickets;
mod tokens;
mod world;

pub use faulty::*;
pub use ledger::*;
pub use notices::*;
pub use site::*;
pub use spec::*;
pub use tokens::*;
pub use world::*;
