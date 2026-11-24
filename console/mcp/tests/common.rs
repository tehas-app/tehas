//! What every test of a tool's answer shares.

use mcp::tools::{list_scoped, Scope};
use serde_json::Value;
use tehas_ports::fake::{FakeSite, World};
use tehas_ports::{Actor, Blueprints, Caller, Cites, Scope as By, Site};

/// The code host's links of `repo`: the ports' fake, whose links are
/// `https://fake.test/<project>/b/<n>` for an issue and `/c/<n>` for a pull
/// request. Which repository a code host links is its own rule, tested on
/// the adapter.
pub fn fake_links(repo: &str) -> Option<Box<dyn Cites + Send>> {
    let site = FakeSite::new(&World::new().named(repo));
    let handle: Box<dyn Blueprints + Send> = site
        .blueprints(&Actor::Ours(By::Full), Caller::new("test.mcp"))
        .ok()?;
    Some(handle)
}

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
