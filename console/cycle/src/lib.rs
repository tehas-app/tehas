//! A cycle's supervisor in Rust (#256). So far its report: `report` takes
//! what an ended cycle left and the answers GitHub gives, and returns what
//! libexec/cycle does with them. It reads and writes nothing itself; the
//! bash cycle still reports.

pub mod facts;
pub mod report;
pub mod runner;

pub use facts::Facts;
pub use report::{report, Report};
