//! The items: a directory of files for each item under the home, taken from
//! the links an issue's allowed accounts wrote ([`store`]), and the page that
//! shows them ([`page`]). Everything is static: nothing here runs a file it
//! keeps.
//!
//! Kept for the 2.1 plugins: the lab plugin takes it (host/docs/contract.md,
//! the Kept table).

pub mod page;
pub mod store;
