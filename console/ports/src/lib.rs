//! The ports of tehas, in tehas's own words: tickets, blueprints and the
//! code host, and the types they share.
//!
//! No supplier is named here. The crate has std and serde alone and no
//! effects: no file, network, process, environment or clock. It is vendored
//! at a tehas tag, which `VERSION` names.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod account;
mod body;
mod bound;
mod ids;
mod outcome;

pub use account::*;
pub use body::*;
pub use bound::*;
pub use ids::*;
pub use outcome::*;

/// The tehas tag these files came from. The crate has no version of its own.
pub const VERSION: &str = "v2.0.0";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_names_a_tag() {
        assert!(VERSION.starts_with('v'));
        let parts: Vec<&str> = VERSION[1..].split('.').collect();
        assert_eq!(parts.len(), 3);
        assert!(parts.iter().all(|p| p.parse::<u32>().is_ok()));
    }
}
