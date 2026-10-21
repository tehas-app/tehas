//! A read-only MCP server over the files a factory publishes under live/,
//! the sessions snapshot and the logs.

pub mod http;
pub mod keys;
#[path = "../../src/mask.rs"]
#[allow(dead_code)] // the console's mask; this crate uses mask() only
pub mod mask;
pub mod rpc;
pub mod snapshot;
pub mod tools;
