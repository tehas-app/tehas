//! The ports of tehas, in tehas's own words: tickets, blueprints and the
//! code host, identity and the joined reads, and the types they share.
//!
//! A `Site` is `Send + Sync`, takes `&self` and makes the handles of one
//! realm of one project. A handle is made per pass, takes `&mut self`, owns
//! what it uses and is `Send`, not `Sync`. The traits are synchronous.
//!
//! No supplier is named here. The crate has std and serde alone and no
//! effects: no file, network, process, environment or clock. It is vendored
//! at a tehas tag, which `VERSION` names.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod account;
mod blueprints;
mod body;
mod bound;
mod code_host;
mod identity;
mod ids;
mod mark;
mod marker;
mod outcome;
mod report;
mod tickets;
mod write;

pub use account::*;
pub use blueprints::*;
pub use body::*;
pub use bound::*;
pub use code_host::*;
pub use identity::*;
pub use ids::*;
pub use mark::*;
pub use marker::*;
pub use outcome::*;
pub use report::*;
pub use tickets::*;
pub use write::*;

/// The tehas tag these files came from. The crate has no version of its own.
pub const VERSION: &str = "v2.0.0";

/// How a port enum that may grow reads a tag this copy does not know.
///
/// Such an enum is written with a tag and its content. Its `Unknown` is the
/// last variant, read when no other matches: a tag this copy does not know,
/// and a tag of its own whose content does not read, are both `Unknown`, so
/// an older reader never fails a record ([`read_any`]). `Marker` reads a tag
/// of its own whose content does not read as `Unreadable` instead, as its
/// text does: it asks [`read`], which refuses its own tags.
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

    /// `Unknown` from any tagged value: a tag this copy does not know, or one
    /// of the enum's own whose content does not read. A value with no tag is
    /// no record of the enum, and stays an error.
    pub(crate) fn read_any<'de, D: Deserializer<'de>>(d: D) -> Result<(), D::Error> {
        #[derive(Deserialize)]
        struct Tag {
            r#type: String,
        }
        Tag::deserialize(d).map(|tag| drop(tag.r#type))
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
