//! What every test of a tool's answer shares.

use mcp::tools::{list_scoped, Scope};
use serde_json::Value;

/// The answer, after it is checked against the outputSchema its tool lists,
/// so every answer a test reads is one a client may read by that schema.
pub fn conforms(name: &str, answer: Value) -> Value {
    let catalog = list_scoped(Scope::Full);
    let schema = catalog
        .as_array()
        .and_then(|tools| tools.iter().find(|t| t["name"] == name))
        .map(|t| t["outputSchema"].clone())
        .unwrap_or_else(|| panic!("{name} is not listed"));
    if let Err(why) = mcp::conform::check(&answer, &schema) {
        panic!("{name}: the answer does not match its outputSchema: {why}\n{answer}");
    }
    answer
}
