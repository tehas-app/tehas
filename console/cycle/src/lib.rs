//! A cycle's supervisor in Rust (#256). So far its report: `report` takes
//! what an ended cycle left and the answers GitHub gives, and returns what
//! libexec/cycle does with them. It writes nothing itself; the bash cycle
//! still reports. `shadow` decides beside the bash's report on a host and
//! says where the two differ. `pin` gives a cycle its own copy of the code.
//! `clock` measures a cycle on a clock that stops while the VM is saved.

pub mod clock;
pub mod facts;
pub mod host;
pub mod last_attempt;
pub mod pin;
pub mod report;
pub mod runner;
pub mod shadow;

pub use facts::Facts;
pub use report::{report, Report};
