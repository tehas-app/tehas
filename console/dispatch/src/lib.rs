//! The dispatcher's decisions in Rust (#253). `decide` takes what a poll read
//! and returns what it does; it reads and writes nothing itself.

pub mod apply;
pub mod decide;
pub mod events;
pub mod poll;
pub mod questions;
pub mod read;
pub mod settings;
pub mod shadow;
pub mod slice;
pub mod slots;
pub mod snapshot;
pub mod text;

pub use decide::{decide, Decision};
pub use settings::Settings;
pub use snapshot::Snapshot;
