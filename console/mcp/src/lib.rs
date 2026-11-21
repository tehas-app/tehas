//! A read-only MCP server over the files a factory publishes under live/,
//! the sessions snapshot, the outcomes counts and the logs, the dispatcher's
//! and refresh's own logs among them.

pub mod conform;
pub mod contractors;
pub mod decisions;
pub mod disk;
pub mod egress;
pub mod events;
pub mod git;
pub mod http;
pub mod keys;
pub mod logs;
pub mod outcomes;
pub mod outcomes_schema;
pub mod park;
pub mod plugins;
pub mod rpc;
pub mod sessions;
pub mod snapshot;
pub mod tickets;
pub mod tools;
