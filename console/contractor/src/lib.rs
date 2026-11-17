//! The contractor program of outsource: a host that runs whole cycles for one
//! factory and holds nothing worth stealing between them.
//!
//! The factory never connects to a contractor. The contractor calls the
//! factory's job channel (proxy/src/channel.rs): it says hello, asks for the
//! job in its hand, runs it with the same `tehas work` and the same container
//! a factory runs, checks in on its own timer, renews the cycle's token and
//! relays its issue writes through the factory, and posts the result. A
//! contractor that loses its factory stops its cycle in an orderly way and
//! leaves the host clean ([`job`]).

pub mod doctor;
pub mod egress;
pub mod host;
pub mod http;
pub mod identity;
pub mod job;

#[cfg(test)]
mod job_tests;
