//! A read-only MCP server over the files a factory publishes under live/,
//! the sessions snapshot, the outcomes counts and the logs, the dispatcher's
//! and refresh's own logs among them.

pub mod decisions;
pub mod disk;
pub mod events;
pub mod http;
pub mod keys;
pub mod logs;
pub mod outcomes;
pub mod outcomes_schema;
pub mod park;
pub mod rpc;
pub mod sessions;
pub mod snapshot;
pub mod tools;
