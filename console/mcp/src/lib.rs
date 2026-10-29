//! A read-only MCP server over the files a factory publishes under live/,
//! the sessions snapshot, the outcomes counts and the logs, with the
//! dispatcher's own log.

pub mod decisions;
pub mod http;
pub mod keys;
pub mod logs;
pub mod outcomes;
pub mod outcomes_schema;
pub mod rpc;
pub mod sessions;
pub mod snapshot;
pub mod tools;
