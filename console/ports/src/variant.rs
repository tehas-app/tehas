//! The name a port enum's own serde writes for a unit variant, `Approve`,
//! read from that serde, so the word has one owner.

use std::fmt;

use serde::ser::{self, Impossible, Serialize, Serializer};

/// The variant's name, or None for a value that is not a unit variant.
pub(crate) fn name<T: Serialize>(t: &T) -> Option<&'static str> {
    t.serialize(Name).ok()
}

#[derive(Debug)]
struct NotUnit;

impl fmt::Display for NotUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not a unit variant")
    }
}

impl std::error::Error for NotUnit {}

impl ser::Error for NotUnit {
    fn custom<M: fmt::Display>(_: M) -> Self {
        NotUnit
    }
}

struct Name;

type No = Impossible<&'static str, NotUnit>;

macro_rules! refuse {
    ($($f:ident($($t:ty),*);)+) => {
        $(fn $f(self $(, _: $t)*) -> Result<&'static str, NotUnit> {
            Err(NotUnit)
        })+
    };
}

impl Serializer for Name {
    type Ok = &'static str;
    type Error = NotUnit;
    type SerializeSeq = No;
    type SerializeTuple = No;
    type SerializeTupleStruct = No;
    type SerializeTupleVariant = No;
    type SerializeMap = No;
    type SerializeStruct = No;
    type SerializeStructVariant = No;

    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<&'static str, NotUnit> {
        Ok(variant)
    }

    refuse! {
        serialize_bool(bool);
        serialize_i8(i8);
        serialize_i16(i16);
        serialize_i32(i32);
        serialize_i64(i64);
        serialize_u8(u8);
        serialize_u16(u16);
        serialize_u32(u32);
        serialize_u64(u64);
        serialize_f32(f32);
        serialize_f64(f64);
        serialize_char(char);
        serialize_str(&str);
        serialize_bytes(&[u8]);
        serialize_none();
        serialize_unit();
        serialize_unit_struct(&'static str);
    }

    fn serialize_some<T: ?Sized + Serialize>(self, _: &T) -> Result<&'static str, NotUnit> {
        Err(NotUnit)
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        _: &T,
    ) -> Result<&'static str, NotUnit> {
        Err(NotUnit)
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &T,
    ) -> Result<&'static str, NotUnit> {
        Err(NotUnit)
    }

    fn serialize_seq(self, _: Option<usize>) -> Result<No, NotUnit> {
        Err(NotUnit)
    }

    fn serialize_tuple(self, _: usize) -> Result<No, NotUnit> {
        Err(NotUnit)
    }

    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<No, NotUnit> {
        Err(NotUnit)
    }

    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<No, NotUnit> {
        Err(NotUnit)
    }

    fn serialize_map(self, _: Option<usize>) -> Result<No, NotUnit> {
        Err(NotUnit)
    }

    fn serialize_struct(self, _: &'static str, _: usize) -> Result<No, NotUnit> {
        Err(NotUnit)
    }

    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<No, NotUnit> {
        Err(NotUnit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Act, TicketKey};

    #[test]
    fn a_unit_variant_is_its_name_and_anything_else_is_none() {
        assert_eq!(name(&Act::Approve), Some("Approve"));
        assert_eq!(name(&Act::Unknown), Some("Unknown"));
        let t = TicketKey::Number(std::num::NonZeroU64::new(12).expect("no zero"));
        assert_eq!(name(&t), None);
        assert_eq!(name(&12u32), None);
    }
}
