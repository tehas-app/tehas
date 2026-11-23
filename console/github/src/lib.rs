//! The GitHub adapter: tehas's terms answered from GitHub through `gh`.
//!
//! It holds the GitHub side of the forge, the `GitHub` type that answers the
//! `Forge` trait of `tehas_core::forge` from an issue's labels and their
//! events, and answers the tickets port's marks and history over the same
//! reads. The trait, [`tehas_core::forge::GhRunner`] and
//! [`tehas_core::forge::Bounded`] stay in core while other crates call them.
//!
//! No library crate depends on this one: a program's wiring does, and the
//! callers of `GitHub::new` that `architecture.toml`'s `adapter-wiring`
//! check names until they move behind the ports.

#![forbid(unsafe_code)]

mod forge;
pub mod gh;
pub mod table;
mod tickets;

pub use forge::{read_events, GitHub};
