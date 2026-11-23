//! A small self-describing serde format for the tests: a value becomes a
//! `Node` tree and is read back from one, shaped as a text format would
//! shape it. The crate takes serde alone, so its tests bring their own
//! format rather than a dependency.

use std::fmt;

use serde::de::{self, DeserializeSeed, IntoDeserializer, MapAccess, SeqAccess, Visitor};
use serde::ser::{self, Serialize};
use serde::{Deserialize, Deserializer, Serializer};

/// A value in the tree.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Null,
    Bool(bool),
    Int(i64),
    Uint(u64),
    Float(f64),
    Text(String),
    Seq(Vec<Node>),
    Map(Vec<(Node, Node)>),
}

/// Why a value did not become a tree, or a tree a value.
#[derive(Debug, Clone, PartialEq)]
pub struct Fault(pub String);

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Fault {}

impl ser::Error for Fault {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Fault(msg.to_string())
    }
}

impl de::Error for Fault {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Fault(msg.to_string())
    }
}

/// The tree of a value.
pub fn to_node<T: Serialize + ?Sized>(v: &T) -> Result<Node, Fault> {
    v.serialize(Out)
}

/// A value from a tree.
pub fn from_node<T: for<'de> Deserialize<'de>>(n: Node) -> Result<T, Fault> {
    T::deserialize(n)
}

/// Text as a node.
pub fn text(s: &str) -> Node {
    Node::Text(s.to_string())
}

/// A map of text keys.
pub fn map(entries: Vec<(&str, Node)>) -> Node {
    Node::Map(entries.into_iter().map(|(k, v)| (text(k), v)).collect())
}

struct Out;

fn tagged(variant: &str, inner: Node) -> Node {
    Node::Map(vec![(text(variant), inner)])
}

impl Serializer for Out {
    type Ok = Node;
    type Error = Fault;
    type SerializeSeq = SeqOut;
    type SerializeTuple = SeqOut;
    type SerializeTupleStruct = SeqOut;
    type SerializeTupleVariant = SeqOut;
    type SerializeMap = MapOut;
    type SerializeStruct = MapOut;
    type SerializeStructVariant = MapOut;

    fn serialize_bool(self, v: bool) -> Result<Node, Fault> {
        Ok(Node::Bool(v))
    }
    fn serialize_i8(self, v: i8) -> Result<Node, Fault> {
        Ok(Node::Int(v.into()))
    }
    fn serialize_i16(self, v: i16) -> Result<Node, Fault> {
        Ok(Node::Int(v.into()))
    }
    fn serialize_i32(self, v: i32) -> Result<Node, Fault> {
        Ok(Node::Int(v.into()))
    }
    fn serialize_i64(self, v: i64) -> Result<Node, Fault> {
        Ok(Node::Int(v))
    }
    fn serialize_u8(self, v: u8) -> Result<Node, Fault> {
        Ok(Node::Uint(v.into()))
    }
    fn serialize_u16(self, v: u16) -> Result<Node, Fault> {
        Ok(Node::Uint(v.into()))
    }
    fn serialize_u32(self, v: u32) -> Result<Node, Fault> {
        Ok(Node::Uint(v.into()))
    }
    fn serialize_u64(self, v: u64) -> Result<Node, Fault> {
        Ok(Node::Uint(v))
    }
    fn serialize_f32(self, v: f32) -> Result<Node, Fault> {
        Ok(Node::Float(v.into()))
    }
    fn serialize_f64(self, v: f64) -> Result<Node, Fault> {
        Ok(Node::Float(v))
    }
    fn serialize_char(self, v: char) -> Result<Node, Fault> {
        Ok(Node::Text(v.to_string()))
    }
    fn serialize_str(self, v: &str) -> Result<Node, Fault> {
        Ok(text(v))
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<Node, Fault> {
        Ok(Node::Seq(
            v.iter().map(|b| Node::Uint((*b).into())).collect(),
        ))
    }
    fn serialize_none(self) -> Result<Node, Fault> {
        Ok(Node::Null)
    }
    fn serialize_some<T: ?Sized + Serialize>(self, v: &T) -> Result<Node, Fault> {
        v.serialize(Out)
    }
    fn serialize_unit(self) -> Result<Node, Fault> {
        Ok(Node::Null)
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<Node, Fault> {
        Ok(Node::Null)
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<Node, Fault> {
        Ok(text(variant))
    }
    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        v: &T,
    ) -> Result<Node, Fault> {
        v.serialize(Out)
    }
    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        v: &T,
    ) -> Result<Node, Fault> {
        Ok(tagged(variant, v.serialize(Out)?))
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<SeqOut, Fault> {
        Ok(SeqOut(Vec::new(), None))
    }
    fn serialize_tuple(self, _: usize) -> Result<SeqOut, Fault> {
        Ok(SeqOut(Vec::new(), None))
    }
    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<SeqOut, Fault> {
        Ok(SeqOut(Vec::new(), None))
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<SeqOut, Fault> {
        Ok(SeqOut(Vec::new(), Some(variant)))
    }
    fn serialize_map(self, _: Option<usize>) -> Result<MapOut, Fault> {
        Ok(MapOut(Vec::new(), None, None))
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<MapOut, Fault> {
        Ok(MapOut(Vec::new(), None, None))
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<MapOut, Fault> {
        Ok(MapOut(Vec::new(), None, Some(variant)))
    }
}

struct SeqOut(Vec<Node>, Option<&'static str>);

impl SeqOut {
    fn push<T: ?Sized + Serialize>(&mut self, v: &T) -> Result<(), Fault> {
        self.0.push(v.serialize(Out)?);
        Ok(())
    }
    fn done(self) -> Result<Node, Fault> {
        let seq = Node::Seq(self.0);
        Ok(match self.1 {
            Some(variant) => tagged(variant, seq),
            None => seq,
        })
    }
}

impl ser::SerializeSeq for SeqOut {
    type Ok = Node;
    type Error = Fault;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, v: &T) -> Result<(), Fault> {
        self.push(v)
    }
    fn end(self) -> Result<Node, Fault> {
        self.done()
    }
}

impl ser::SerializeTuple for SeqOut {
    type Ok = Node;
    type Error = Fault;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, v: &T) -> Result<(), Fault> {
        self.push(v)
    }
    fn end(self) -> Result<Node, Fault> {
        self.done()
    }
}

impl ser::SerializeTupleStruct for SeqOut {
    type Ok = Node;
    type Error = Fault;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, v: &T) -> Result<(), Fault> {
        self.push(v)
    }
    fn end(self) -> Result<Node, Fault> {
        self.done()
    }
}

impl ser::SerializeTupleVariant for SeqOut {
    type Ok = Node;
    type Error = Fault;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, v: &T) -> Result<(), Fault> {
        self.push(v)
    }
    fn end(self) -> Result<Node, Fault> {
        self.done()
    }
}

struct MapOut(Vec<(Node, Node)>, Option<Node>, Option<&'static str>);

impl MapOut {
    fn done(self) -> Result<Node, Fault> {
        let m = Node::Map(self.0);
        Ok(match self.2 {
            Some(variant) => tagged(variant, m),
            None => m,
        })
    }
}

impl ser::SerializeMap for MapOut {
    type Ok = Node;
    type Error = Fault;
    fn serialize_key<T: ?Sized + Serialize>(&mut self, k: &T) -> Result<(), Fault> {
        self.1 = Some(k.serialize(Out)?);
        Ok(())
    }
    fn serialize_value<T: ?Sized + Serialize>(&mut self, v: &T) -> Result<(), Fault> {
        let k = self
            .1
            .take()
            .ok_or_else(|| Fault("a value with no key".into()))?;
        self.0.push((k, v.serialize(Out)?));
        Ok(())
    }
    fn end(self) -> Result<Node, Fault> {
        self.done()
    }
}

impl ser::SerializeStruct for MapOut {
    type Ok = Node;
    type Error = Fault;
    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        k: &'static str,
        v: &T,
    ) -> Result<(), Fault> {
        self.0.push((text(k), v.serialize(Out)?));
        Ok(())
    }
    fn end(self) -> Result<Node, Fault> {
        self.done()
    }
}

impl ser::SerializeStructVariant for MapOut {
    type Ok = Node;
    type Error = Fault;
    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        k: &'static str,
        v: &T,
    ) -> Result<(), Fault> {
        self.0.push((text(k), v.serialize(Out)?));
        Ok(())
    }
    fn end(self) -> Result<Node, Fault> {
        self.done()
    }
}

impl<'de> Deserializer<'de> for Node {
    type Error = Fault;

    fn deserialize_any<V: Visitor<'de>>(self, v: V) -> Result<V::Value, Fault> {
        match self {
            Node::Null => v.visit_unit(),
            Node::Bool(b) => v.visit_bool(b),
            Node::Int(i) => v.visit_i64(i),
            Node::Uint(u) => v.visit_u64(u),
            Node::Float(f) => v.visit_f64(f),
            Node::Text(s) => v.visit_string(s),
            Node::Seq(items) => v.visit_seq(SeqIn(items.into_iter())),
            Node::Map(entries) => v.visit_map(MapIn(entries.into_iter(), None)),
        }
    }

    fn deserialize_option<V: Visitor<'de>>(self, v: V) -> Result<V::Value, Fault> {
        match self {
            Node::Null => v.visit_none(),
            other => v.visit_some(other),
        }
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        v: V,
    ) -> Result<V::Value, Fault> {
        v.visit_newtype_struct(self)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        v: V,
    ) -> Result<V::Value, Fault> {
        match self {
            Node::Text(s) => v.visit_enum(s.into_deserializer()),
            Node::Map(mut entries) if entries.len() == 1 => {
                let (k, inner) = entries.remove(0);
                v.visit_enum(EnumIn(k, inner))
            }
            other => Err(Fault(format!("an enum from {other:?}"))),
        }
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf unit unit_struct seq tuple tuple_struct map struct
        identifier ignored_any
    }
}

impl<'de> IntoDeserializer<'de, Fault> for Node {
    type Deserializer = Node;
    fn into_deserializer(self) -> Node {
        self
    }
}

struct SeqIn(std::vec::IntoIter<Node>);

impl<'de> SeqAccess<'de> for SeqIn {
    type Error = Fault;
    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Fault> {
        self.0.next().map(|n| seed.deserialize(n)).transpose()
    }
}

struct MapIn(std::vec::IntoIter<(Node, Node)>, Option<Node>);

impl<'de> MapAccess<'de> for MapIn {
    type Error = Fault;
    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Fault> {
        match self.0.next() {
            Some((k, v)) => {
                self.1 = Some(v);
                seed.deserialize(k).map(Some)
            }
            None => Ok(None),
        }
    }
    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, Fault> {
        let v = self
            .1
            .take()
            .ok_or_else(|| Fault("a key with no value".into()))?;
        seed.deserialize(v)
    }
}

struct EnumIn(Node, Node);

impl<'de> de::EnumAccess<'de> for EnumIn {
    type Error = Fault;
    type Variant = Node;
    fn variant_seed<V: DeserializeSeed<'de>>(self, seed: V) -> Result<(V::Value, Node), Fault> {
        Ok((seed.deserialize(self.0)?, self.1))
    }
}

impl<'de> de::VariantAccess<'de> for Node {
    type Error = Fault;
    fn unit_variant(self) -> Result<(), Fault> {
        match self {
            Node::Null => Ok(()),
            other => Err(Fault(format!("a unit variant from {other:?}"))),
        }
    }
    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, Fault> {
        seed.deserialize(self)
    }
    fn tuple_variant<V: Visitor<'de>>(self, _: usize, v: V) -> Result<V::Value, Fault> {
        self.deserialize_any(v)
    }
    fn struct_variant<V: Visitor<'de>>(
        self,
        _: &'static [&'static str],
        v: V,
    ) -> Result<V::Value, Fault> {
        self.deserialize_any(v)
    }
}
