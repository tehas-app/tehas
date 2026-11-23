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
mod mark;
mod marker;
mod outcome;

pub use account::*;
pub use body::*;
pub use bound::*;
pub use ids::*;
pub use mark::*;
pub use marker::*;
pub use outcome::*;

/// The tehas tag these files came from. The crate has no version of its own.
pub const VERSION: &str = "v2.0.0";

/// How a port enum that may grow reads a tag this copy does not know.
///
/// Such an enum is written with a tag and its content. Its `Unknown` is the
/// last variant, read when no other matches: a tag of the enum's own whose
/// content does not read is an error, any other tag is `Unknown`. `Marker`
/// reads such a tag as `Unreadable` instead, as its text does.
mod unknown {
    use serde::de::Error as _;
    use serde::ser::SerializeMap;
    use serde::{Deserialize, Deserializer, Serializer};

    /// The key of the tag, beside the key of the content.
    pub(crate) const TAG: &str = "type";

    /// `Unknown` from a tag outside `known`.
    pub(crate) fn read<'de, D: Deserializer<'de>>(d: D, known: &[&str]) -> Result<(), D::Error> {
        #[derive(Deserialize)]
        struct Tag {
            r#type: String,
        }
        let tag = Tag::deserialize(d)?;
        if known.contains(&tag.r#type.as_str()) {
            Err(D::Error::custom(format!(
                "a {} whose content does not read",
                tag.r#type
            )))
        } else {
            Ok(())
        }
    }

    /// `Unknown` as its tag alone, so it reads back as `Unknown`.
    pub(crate) fn write<S: Serializer>(s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(1))?;
        map.serialize_entry(TAG, "Unknown")?;
        map.end()
    }
}

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
