//! A cycle's supervisor in Rust (#256). So far its report: `report` takes
//! what an ended cycle left and the answers GitHub gives, and returns what
//! libexec/cycle does with them. It writes nothing itself; the bash cycle
//! still reports. `shadow` decides beside the bash's report on a host and
//! says where the two differ.

pub mod facts;
pub mod host;
pub mod last_attempt;
pub mod report;
pub mod runner;
pub mod shadow;

pub use facts::Facts;
pub use report::{report, Report};
