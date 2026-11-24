//! The GitHub adapter: tehas's terms answered from GitHub through `gh`.
//!
//! It holds the GitHub side of the forge, the `GitHub` type with the forge's
//! terms over an issue's labels and their events, and answers the tickets
//! port, `Tickets` and `TicketWrites` of `tehas-ports`, over the same reads.
//! [`GitHubSite`] is its `Site`, the factory of those handles.
//! [`Curl`] is its one way to GitHub with no credential: the API host
//! alone, for `Actor::Anonymous`'s reads of a public project.
//! [`tehas_core::forge::GhRunner`] and [`tehas_core::forge::Bounded`] stay
//! in core while other crates call them.
//!
//! No library crate depends on this one: a program's wiring does, the one
//! place `architecture.toml`'s `adapter-wiring` check allows.

#![forbid(unsafe_code)]

mod attachments;
mod curl;
mod forge;
pub mod gh;
mod items;
pub mod markers;
mod port;
pub mod table;
mod ticket;
mod tickets;
mod vocabulary;

pub use curl::{Curl, ENV as CURL_ENV};
pub use forge::{read_events, Approval, ForgeError, GitHub, LabelEvent};
pub use port::{project_of, GitHubSite, MakeRunner, PeopleClient};
