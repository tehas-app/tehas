//! The host's JSON files read into types: a field of another type than the
//! one wanted reads as nothing, not as a document that cannot be read, as
//! every reader of this program has always done it. The facts, the runner's
//! result, the progress summary, the live files of the poll and the cycle's
//! stream are read through these, so each reader's rule for a wrong type is
//! in its type and not in a walk over a document.

use serde::de::{DeserializeOwned, Deserializer};
use serde::Deserialize;
use serde_json::Value;

/// A `T`, or None where the JSON holds no value, null or one of another type.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Loose<T>(pub Option<T>);

impl<'de, T: DeserializeOwned> Deserialize<'de> for Loose<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        Ok(Loose(serde_json::from_value(v).ok()))
    }
}

impl<T> Loose<T> {
    pub fn get(&self) -> Option<&T> {
        self.0.as_ref()
    }
}

impl Loose<i64> {
    /// The number when it is one and not negative, as every reader of this
    /// program reads a count: a negative one is no figure.
    pub fn whole(&self) -> Option<i64> {
        self.0.filter(|n| *n >= 0)
    }
}

/// A value that is an object (read as a `T`) or is none of one. `T` reads
/// any object: its own fields are all [`Loose`], so a wrong type in one of
/// them loses that field and not the object.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Node<T> {
    #[default]
    Absent,
    Object(T),
}

impl<'de, T: DeserializeOwned> Deserialize<'de> for Node<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        Ok(match v {
            v @ Value::Object(_) => serde_json::from_value(v).map_or(Node::Absent, Node::Object),
            _ => Node::Absent,
        })
    }
}

impl<T> Node<T> {
    pub fn into_object(self) -> Option<T> {
        match self {
            Node::Object(t) => Some(t),
            Node::Absent => None,
        }
    }

    pub fn object(&self) -> Option<&T> {
        match self {
            Node::Object(t) => Some(t),
            Node::Absent => None,
        }
    }
}

/// The members of an array, each read as a `T`; anything that is not an
/// array, and any member that is not a `T`, holds nothing.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Many<T> {
    #[default]
    Absent,
    List(Vec<T>),
}

impl<'de, T: DeserializeOwned> Deserialize<'de> for Many<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        Ok(match v {
            Value::Array(a) => Many::List(
                a.into_iter()
                    .filter_map(|v| serde_json::from_value(v).ok())
                    .collect(),
            ),
            _ => Many::Absent,
        })
    }
}

impl<T> Many<T> {
    pub fn items(&self) -> &[T] {
        match self {
            Many::List(a) => a,
            Many::Absent => &[],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize, Default, Debug, PartialEq)]
    #[serde(default)]
    struct Doc {
        s: Loose<String>,
        n: Loose<i64>,
        o: Node<Inner>,
        m: Many<Inner>,
    }

    #[derive(Deserialize, Default, Debug, PartialEq)]
    #[serde(default)]
    struct Inner {
        x: Loose<String>,
    }

    #[test]
    fn a_field_of_another_type_is_nothing_and_the_rest_still_reads() {
        let d: Doc = serde_json::from_str(r#"{"s":3,"n":-1,"o":{"x":1},"m":[{"x":"a"},7]}"#)
            .expect("any object reads");
        assert_eq!(d.s, Loose(None));
        assert_eq!(d.n.whole(), None);
        assert_eq!(d.n.0, Some(-1));
        assert_eq!(d.o.object().unwrap(), &Inner { x: Loose(None) });
        assert_eq!(
            d.m.items(),
            [Inner {
                x: Loose(Some("a".into()))
            }]
        );
    }

    #[test]
    fn a_node_is_an_object_only_and_many_an_array_only() {
        for t in [
            r#"{"o":null}"#,
            r#"{"o":false}"#,
            r#"{"o":[1]}"#,
            r#"{"o":"x"}"#,
        ] {
            let d: Doc = serde_json::from_str(t).unwrap();
            assert_eq!(d.o, Node::Absent, "{t}");
        }
        for t in [r#"{"m":null}"#, r#"{"m":{}}"#, r#"{"m":1}"#] {
            let d: Doc = serde_json::from_str(t).unwrap();
            assert!(d.m.items().is_empty(), "{t}");
        }
        let d: Doc = serde_json::from_str(r#"{"n":1}"#).unwrap();
        assert_eq!(d.n.get(), Some(&1));
    }
}
