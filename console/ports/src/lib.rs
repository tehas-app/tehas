//! The ports of tehas, in tehas's own words: tickets, blueprints and the
//! code host, identity and the joined reads, the types they share, and the
//! ledger and health that report a supplier's failures once.
//!
//! A `Site` is `Send + Sync`, takes `&self` and makes the handles of one
//! realm of one project. A handle is made per pass, takes `&mut self`, owns
//! what it uses and is `Send`, not `Sync`. The traits are synchronous.
//!
//! No supplier is named here. The crate has std and serde alone and no
//! effects: no file, network, process, environment or clock. It is vendored
//! at a tehas tag, which `VERSION` names.
//!
//! An implementer builds a port's answer one of two ways, then sets the
//! rest by name: a struct whose every field has an honest empty value
//! derives `Default` (`let mut s = Size::default(); s.files = 3;`); any
//! other has `new`, which takes the fields an answer always has and leaves
//! the rest empty, false or None. Every answer is `#[non_exhaustive]`, so
//! outside this crate a struct literal, `..Default::default()` among them,
//! does not compile, and a pattern on one ends in `..`. So a field added
//! later breaks no adapter. A field added after the freeze carries
//! `#[serde(default)]`, with the same empty value `new` gives it, so a
//! value stored before it still reads. An enum that may grow is `#[non_exhaustive]`: a match on one
//! outside this crate has an arm for the rest, and that arm fails closed.
//! A trait method added after the freeze has a default body that fails
//! closed (`Refused(Unsupported)` for a call, `""` or empty for a name),
//! so it breaks no implementer either.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// `new` for a port's answer: the fields named, every other field its
/// default. Each struct lists its other fields, so one added later must be
/// named here, with its default, or the crate does not build.
macro_rules! new {
    ($ty:ident { $($req:ident: $rt:ty),* $(,)? } $(, $rest:ident)* $(,)?) => {
        impl $ty {
            #[doc = concat!(
                "A `", stringify!($ty), "` of the fields given, every other ",
                "field empty, false or None, for an implementer to set by name."
            )]
            // The arguments are the fields every answer has: no fewer.
            #[allow(clippy::too_many_arguments)]
            pub fn new($($req: $rt),*) -> Self {
                Self { $($req,)* $($rest: Default::default(),)* }
            }
        }
    };
}

mod account;
mod blueprints;
mod board;
mod body;
mod bound;
mod code_host;
pub mod events;
pub mod fake;
mod health;
mod identity;
mod ids;
pub mod laws;
mod ledger;
mod links;
mod mark;
mod marker;
mod outcome;
mod report;
mod site;
mod tickets;
mod variant;
mod write;

pub use account::*;
pub use blueprints::*;
pub use board::*;
pub use body::*;
pub use bound::*;
pub use code_host::*;
pub use health::*;
pub use identity::*;
pub use ids::*;
pub use ledger::*;
pub use links::*;
pub use mark::*;
pub use marker::*;
pub use outcome::*;
pub use report::*;
pub use site::*;
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
    use serde::de::{Error as _, IgnoredAny, MapAccess, Visitor};
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
        read_any_at(d, TAG)
    }

    /// [`read_any`] for an enum whose tag has a key of its own, as the
    /// events' `event`.
    pub(crate) fn read_any_at<'de, D: Deserializer<'de>>(
        d: D,
        tag: &'static str,
    ) -> Result<(), D::Error> {
        struct Tagged(&'static str);
        impl<'de> Visitor<'de> for Tagged {
            type Value = ();
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "a map with `{}`", self.0)
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
                let mut seen = false;
                while let Some(key) = map.next_key::<String>()? {
                    if key == self.0 {
                        if seen {
                            return Err(A::Error::duplicate_field(self.0));
                        }
                        map.next_value::<String>()?;
                        seen = true;
                    } else {
                        map.next_value::<IgnoredAny>()?;
                    }
                }
                if seen {
                    Ok(())
                } else {
                    Err(A::Error::missing_field(self.0))
                }
            }
        }
        d.deserialize_map(Tagged(tag))
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
