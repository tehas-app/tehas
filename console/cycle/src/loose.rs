//! JSON read as the cycle's jq read it, into types: a field of another type
//! than the one wanted reads as nothing, not as a document that cannot be
//! read. The runner's result, its stream and the driver's state files are
//! read through these, so each reader's rule for a wrong type is in its
//! type and not in a walk over a `serde_json::Value`.

use serde::de::{DeserializeOwned, Deserializer};
use serde::Deserialize;
use serde_json::{Number, Value};

/// A `T`, or None where the JSON holds no value or one of another type, as
/// `.k | strings` (or `numbers`, `booleans`) reads it.
#[derive(Clone, Debug, PartialEq)]
pub struct Loose<T>(pub Option<T>);

impl<T> Default for Loose<T> {
    fn default() -> Self {
        Loose(None)
    }
}

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

impl Loose<String> {
    /// The string, or "" where there is none.
    pub fn str(&self) -> &str {
        self.0.as_deref().unwrap_or("")
    }
}

/// What a path of jq meets at a key: nothing, null, false, an object (read
/// as `T`), or a value of another type, where `.k.x` stops. A `T` is read
/// from an object only, never from an array.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Node<T> {
    #[default]
    Absent,
    Null,
    False,
    Object(T),
    Other,
}

impl<'de, T: DeserializeOwned> Deserialize<'de> for Node<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match Value::deserialize(d)? {
            Value::Null => Node::Null,
            Value::Bool(false) => Node::False,
            v @ Value::Object(_) => serde_json::from_value(v).map_or(Node::Other, Node::Object),
            _ => Node::Other,
        })
    }
}

impl<T> Node<T> {
    /// The object, when it is one.
    pub fn object(&self) -> Option<&T> {
        match self {
            Node::Object(t) => Some(t),
            _ => None,
        }
    }

    pub fn into_object(self) -> Option<T> {
        match self {
            Node::Object(t) => Some(t),
            _ => None,
        }
    }

    /// Whether `.k.x?` makes no value at all here: neither nothing, null nor
    /// an object.
    pub fn hides(&self) -> bool {
        matches!(self, Node::False | Node::Other)
    }
}

/// `.k[]?`: the members of an array, or the values of an object in its key
/// order; anything else holds none.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Many<T> {
    #[default]
    Absent,
    Null,
    False,
    List(Vec<T>),
    Map(Vec<T>),
    Other,
}

impl<'de, T: DeserializeOwned> Deserialize<'de> for Many<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let each = |v: Value| serde_json::from_value(v).ok();
        Ok(match Value::deserialize(d)? {
            Value::Null => Many::Null,
            Value::Bool(false) => Many::False,
            Value::Array(a) => Many::List(a.into_iter().filter_map(each).collect()),
            Value::Object(o) => Many::Map(o.into_iter().filter_map(|(_, v)| each(v)).collect()),
            _ => Many::Other,
        })
    }
}

impl<T> Many<T> {
    /// The members, of an array or an object; none of anything else.
    pub fn items(&self) -> &[T] {
        match self {
            Many::List(a) | Many::Map(a) => a,
            _ => &[],
        }
    }
}

/// One JSON value where its type matters to the reader: a scalar as it is,
/// and an array or an object as its JSON text.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Field {
    #[default]
    Absent,
    Null,
    Bool(bool),
    Num(Number),
    Str(String),
    /// An array or an object, as its JSON text.
    Many(String),
}

impl<'de> Deserialize<'de> for Field {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match Value::deserialize(d)? {
            Value::Null => Field::Null,
            Value::Bool(b) => Field::Bool(b),
            Value::Number(n) => Field::Num(n),
            Value::String(s) => Field::Str(s),
            v => Field::Many(v.to_string()),
        })
    }
}

impl Field {
    /// Nothing, null or false: what jq's `//` reads as no value.
    pub fn nothing(&self) -> bool {
        matches!(self, Field::Absent | Field::Null | Field::Bool(false))
    }

    pub fn str(&self) -> Option<&str> {
        match self {
            Field::Str(s) => Some(s),
            _ => None,
        }
    }

    /// The JSON text of the value, `null` where there is none.
    pub fn json(&self) -> String {
        match self {
            Field::Absent | Field::Null => "null".to_string(),
            Field::Bool(b) => b.to_string(),
            Field::Num(n) => n.to_string(),
            Field::Str(s) => Value::String(s.clone()).to_string(),
            Field::Many(t) => t.clone(),
        }
    }
}

/// A field that is there, null included, as `Some`: `serde(default,
/// deserialize_with = "present")` on an `Option<T>`.
pub fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

/// Every JSON value of a text, read as `T`, or None when one cannot be
/// parsed or read.
pub fn values<T: DeserializeOwned>(text: &str) -> Option<Vec<T>> {
    serde_json::Deserializer::from_str(text)
        .into_iter::<T>()
        .collect::<Result<Vec<_>, _>>()
        .ok()
}

/// The first JSON value of a text, as `jq -n input` reads it.
pub fn first_value<T: DeserializeOwned>(text: &str) -> Option<T> {
    serde_json::Deserializer::from_str(text)
        .into_iter::<T>()
        .next()
        .and_then(Result::ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize, Default, Debug, PartialEq)]
    struct Doc {
        #[serde(default)]
        s: Loose<String>,
        #[serde(default)]
        n: Node<Inner>,
        #[serde(default)]
        m: Many<Loose<u64>>,
        #[serde(default)]
        f: Field,
        #[serde(default, deserialize_with = "present")]
        p: Option<Field>,
    }

    #[derive(Deserialize, Default, Debug, PartialEq)]
    struct Inner {
        #[serde(default)]
        x: Loose<u64>,
    }

    fn doc(text: &str) -> Doc {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn a_field_of_another_type_is_nothing_and_the_rest_still_reads() {
        let d = doc(r#"{"s":3,"n":{"x":"y"},"m":[1,"a",2],"f":[1, 2]}"#);
        assert_eq!(d.s, Loose(None));
        assert_eq!(d.n, Node::Object(Inner { x: Loose(None) }));
        let m: Vec<_> = d.m.items().iter().map(|l| l.0).collect();
        assert_eq!(m, [Some(1), None, Some(2)]);
        assert_eq!(d.f, Field::Many("[1,2]".into()));
        assert_eq!(d.p, None);
        assert_eq!(doc(r#"{"p":null}"#).p, Some(Field::Null));
    }

    #[test]
    fn a_node_is_an_object_only() {
        for (t, n) in [
            (r#"{}"#, Node::Absent),
            (r#"{"n":null}"#, Node::Null),
            (r#"{"n":false}"#, Node::False),
            (r#"{"n":[7]}"#, Node::Other),
            (r#"{"n":true}"#, Node::Other),
        ] {
            assert_eq!(doc(t).n, n, "{t}");
        }
        assert!(Node::<Inner>::Other.hides() && Node::<Inner>::False.hides());
        assert!(!Node::<Inner>::Null.hides());
    }

    #[test]
    fn many_reads_an_array_or_an_objects_values() {
        let m = |t: &str| -> Vec<Option<u64>> { doc(t).m.items().iter().map(|l| l.0).collect() };
        assert_eq!(m(r#"{"m":{"b":2,"a":1}}"#), [Some(1), Some(2)]);
        assert!(m(r#"{"m":"x"}"#).is_empty());
    }

    #[test]
    fn values_reads_each_value_of_a_text() {
        assert_eq!(values::<u64>("1 2\n3"), Some(vec![1, 2, 3]));
        assert_eq!(values::<u64>("1 x"), None);
        assert_eq!(first_value::<u64>("4 x"), Some(4));
    }
}
