//! The GitHub adapter: tehas's terms answered from GitHub through `gh`.
//!
//! It holds the GitHub side of the forge, the `GitHub` type with the forge's
//! terms over an issue's labels and their events, and answers the tickets
//! port, `Tickets` and `TicketWrites` of `tehas-ports`, over the same reads.
//! [`GitHubSite`] is its `Site`, the factory of those handles.
//! [`tehas_core::forge::GhRunner`] and [`tehas_core::forge::Bounded`] stay
//! in core while other crates call them.
//!
//! No library crate depends on this one: a program's wiring does, the one
//! place `architecture.toml`'s `adapter-wiring` check allows.

#![forbid(unsafe_code)]

mod attachments;
mod forge;
pub mod gh;
pub mod markers;
mod port;
pub mod table;
mod ticket;
mod tickets;

pub use forge::{read_events, Approval, ForgeError, GitHub, LabelEvent};
pub use port::{GitHubSite, MakeRunner};
