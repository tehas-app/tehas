//! The one-line form of an envelope, as a program's log writes it: `event`
//! and the envelope as compact JSON, the text serde_json writes. The crate
//! takes serde alone, so the JSON is written here. It knows what the ports'
//! types write: no float, and map keys that are strings or integers.

use std::fmt::{self, Write as _};

use serde::ser::{self, Serialize, Serializer};

/// `value` as compact JSON, or why it does not write.
pub(super) fn json<T: Serialize + ?Sized>(value: &T) -> Result<String, NoJson> {
    let mut out = Out(String::new());
    value.serialize(&mut out)?;
    Ok(out.0)
}

/// Why a value does not write as JSON.
#[derive(Debug)]
pub(super) struct NoJson;

impl fmt::Display for NoJson {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("does not write as JSON")
    }
}

impl std::error::Error for NoJson {}

impl ser::Error for NoJson {
    fn custom<M: fmt::Display>(_: M) -> Self {
        NoJson
    }
}

struct Out(String);

impl Out {
    fn text(&mut self, s: &str) {
        self.0.push('"');
        for c in s.chars() {
            match c {
                '"' => self.0.push_str("\\\""),
                '\\' => self.0.push_str("\\\\"),
                '\u{8}' => self.0.push_str("\\b"),
                '\u{c}' => self.0.push_str("\\f"),
                '\n' => self.0.push_str("\\n"),
                '\r' => self.0.push_str("\\r"),
                '\t' => self.0.push_str("\\t"),
                c if c < ' ' => {
                    let _ = write!(self.0, "\\u{:04x}", c as u32);
                }
                c => self.0.push(c),
            }
        }
        self.0.push('"');
    }

    fn number(&mut self, n: impl fmt::Display) {
        let _ = write!(self.0, "{n}");
    }

    fn open(&mut self, bracket: char, close: &'static str) -> Many<'_> {
        self.0.push(bracket);
        Many {
            out: self,
            first: true,
            close,
        }
    }

    // `{"variant":` before a variant's content; its `}` is closed after.
    fn variant(&mut self, variant: &str) {
        self.0.push('{');
        self.text(variant);
        self.0.push(':');
    }
}

// A sequence or a map being written; `close` ends it, with a variant's `}`.
struct Many<'a> {
    out: &'a mut Out,
    first: bool,
    close: &'static str,
}

impl Many<'_> {
    fn comma(&mut self) {
        if !self.first {
            self.out.0.push(',');
        }
        self.first = false;
    }

    fn item<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), NoJson> {
        self.comma();
        v.serialize(&mut *self.out)
    }

    fn field<T: Serialize + ?Sized>(&mut self, key: &str, v: &T) -> Result<(), NoJson> {
        self.comma();
        self.out.text(key);
        self.out.0.push(':');
        v.serialize(&mut *self.out)
    }

    fn end(self) -> Result<(), NoJson> {
        self.out.0.push_str(self.close);
        Ok(())
    }
}

impl<'a> Serializer for &'a mut Out {
    type Ok = ();
    type Error = NoJson;
    type SerializeSeq = Many<'a>;
    type SerializeTuple = Many<'a>;
    type SerializeTupleStruct = Many<'a>;
    type SerializeTupleVariant = Many<'a>;
    type SerializeMap = Many<'a>;
    type SerializeStruct = Many<'a>;
    type SerializeStructVariant = Many<'a>;

    fn serialize_bool(self, v: bool) -> Result<(), NoJson> {
        self.0.push_str(if v { "true" } else { "false" });
        Ok(())
    }

    fn serialize_i8(self, v: i8) -> Result<(), NoJson> {
        self.number(v);
        Ok(())
    }

    fn serialize_i16(self, v: i16) -> Result<(), NoJson> {
        self.number(v);
        Ok(())
    }

    fn serialize_i32(self, v: i32) -> Result<(), NoJson> {
        self.number(v);
        Ok(())
    }

    fn serialize_i64(self, v: i64) -> Result<(), NoJson> {
        self.number(v);
        Ok(())
    }

    fn serialize_u8(self, v: u8) -> Result<(), NoJson> {
        self.number(v);
        Ok(())
    }

    fn serialize_u16(self, v: u16) -> Result<(), NoJson> {
        self.number(v);
        Ok(())
    }

    fn serialize_u32(self, v: u32) -> Result<(), NoJson> {
        self.number(v);
        Ok(())
    }

    fn serialize_u64(self, v: u64) -> Result<(), NoJson> {
        self.number(v);
        Ok(())
    }

    // No type of the ports writes a float, and serde_json's shortest form
    // of one is not std's.
    fn serialize_f32(self, _: f32) -> Result<(), NoJson> {
        Err(NoJson)
    }

    fn serialize_f64(self, _: f64) -> Result<(), NoJson> {
        Err(NoJson)
    }

    fn serialize_char(self, v: char) -> Result<(), NoJson> {
        self.text(v.encode_utf8(&mut [0; 4]));
        Ok(())
    }

    fn serialize_str(self, v: &str) -> Result<(), NoJson> {
        self.text(v);
        Ok(())
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<(), NoJson> {
        let mut seq = self.open('[', "]");
        for b in v {
            seq.item(b)?;
        }
        seq.end()
    }

    fn serialize_none(self) -> Result<(), NoJson> {
        self.serialize_unit()
    }

    fn serialize_some<T: Serialize + ?Sized>(self, v: &T) -> Result<(), NoJson> {
        v.serialize(self)
    }

    fn serialize_unit(self) -> Result<(), NoJson> {
        self.0.push_str("null");
        Ok(())
    }

    fn serialize_unit_struct(self, _: &'static str) -> Result<(), NoJson> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<(), NoJson> {
        self.text(variant);
        Ok(())
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        v: &T,
    ) -> Result<(), NoJson> {
        v.serialize(self)
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        v: &T,
    ) -> Result<(), NoJson> {
        self.variant(variant);
        v.serialize(&mut *self)?;
        self.0.push('}');
        Ok(())
    }

    fn serialize_seq(self, _: Option<usize>) -> Result<Many<'a>, NoJson> {
        Ok(self.open('[', "]"))
    }

    fn serialize_tuple(self, _: usize) -> Result<Many<'a>, NoJson> {
        Ok(self.open('[', "]"))
    }

    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<Many<'a>, NoJson> {
        Ok(self.open('[', "]"))
    }

    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Many<'a>, NoJson> {
        self.variant(variant);
        Ok(self.open('[', "]}"))
    }

    fn serialize_map(self, _: Option<usize>) -> Result<Many<'a>, NoJson> {
        Ok(self.open('{', "}"))
    }

    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Many<'a>, NoJson> {
        Ok(self.open('{', "}"))
    }

    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Many<'a>, NoJson> {
        self.variant(variant);
        Ok(self.open('{', "}}"))
    }
}

impl ser::SerializeSeq for Many<'_> {
    type Ok = ();
    type Error = NoJson;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), NoJson> {
        self.item(v)
    }

    fn end(self) -> Result<(), NoJson> {
        Many::end(self)
    }
}

impl ser::SerializeTuple for Many<'_> {
    type Ok = ();
    type Error = NoJson;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), NoJson> {
        self.item(v)
    }

    fn end(self) -> Result<(), NoJson> {
        Many::end(self)
    }
}

impl ser::SerializeTupleStruct for Many<'_> {
    type Ok = ();
    type Error = NoJson;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), NoJson> {
        self.item(v)
    }

    fn end(self) -> Result<(), NoJson> {
        Many::end(self)
    }
}

impl ser::SerializeTupleVariant for Many<'_> {
    type Ok = ();
    type Error = NoJson;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), NoJson> {
        self.item(v)
    }

    fn end(self) -> Result<(), NoJson> {
        Many::end(self)
    }
}

impl ser::SerializeMap for Many<'_> {
    type Ok = ();
    type Error = NoJson;

    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), NoJson> {
        self.comma();
        let key = key.serialize(Key)?;
        self.out.text(&key);
        self.out.0.push(':');
        Ok(())
    }

    fn serialize_value<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), NoJson> {
        v.serialize(&mut *self.out)
    }

    fn end(self) -> Result<(), NoJson> {
        Many::end(self)
    }
}

impl ser::SerializeStruct for Many<'_> {
    type Ok = ();
    type Error = NoJson;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        v: &T,
    ) -> Result<(), NoJson> {
        self.field(key, v)
    }

    fn end(self) -> Result<(), NoJson> {
        Many::end(self)
    }
}

impl ser::SerializeStructVariant for Many<'_> {
    type Ok = ();
    type Error = NoJson;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        v: &T,
    ) -> Result<(), NoJson> {
        self.field(key, v)
    }

    fn end(self) -> Result<(), NoJson> {
        Many::end(self)
    }
}

// A map's key: a string, or an integer written as one, as serde_json does.
struct Key;

type NoKey = ser::Impossible<String, NoJson>;

macro_rules! integer_keys {
    ($($f:ident($t:ty);)+) => {
        $(fn $f(self, v: $t) -> Result<String, NoJson> {
            Ok(v.to_string())
        })+
    };
}

impl Serializer for Key {
    type Ok = String;
    type Error = NoJson;
    type SerializeSeq = NoKey;
    type SerializeTuple = NoKey;
    type SerializeTupleStruct = NoKey;
    type SerializeTupleVariant = NoKey;
    type SerializeMap = NoKey;
    type SerializeStruct = NoKey;
    type SerializeStructVariant = NoKey;

    integer_keys! {
        serialize_i8(i8);
        serialize_i16(i16);
        serialize_i32(i32);
        serialize_i64(i64);
        serialize_u8(u8);
        serialize_u16(u16);
        serialize_u32(u32);
        serialize_u64(u64);
    }

    fn serialize_str(self, v: &str) -> Result<String, NoJson> {
        Ok(v.to_string())
    }

    fn serialize_char(self, v: char) -> Result<String, NoJson> {
        Ok(v.to_string())
    }

    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<String, NoJson> {
        Ok(variant.to_string())
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        v: &T,
    ) -> Result<String, NoJson> {
        v.serialize(self)
    }

    fn serialize_bool(self, _: bool) -> Result<String, NoJson> {
        Err(NoJson)
    }

    fn serialize_f32(self, _: f32) -> Result<String, NoJson> {
        Err(NoJson)
    }

    fn serialize_f64(self, _: f64) -> Result<String, NoJson> {
        Err(NoJson)
    }

    fn serialize_bytes(self, _: &[u8]) -> Result<String, NoJson> {
        Err(NoJson)
    }

    fn serialize_none(self) -> Result<String, NoJson> {
        Err(NoJson)
    }

    fn serialize_some<T: Serialize + ?Sized>(self, _: &T) -> Result<String, NoJson> {
        Err(NoJson)
    }

    fn serialize_unit(self) -> Result<String, NoJson> {
        Err(NoJson)
    }

    fn serialize_unit_struct(self, _: &'static str) -> Result<String, NoJson> {
        Err(NoJson)
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &T,
    ) -> Result<String, NoJson> {
        Err(NoJson)
    }

    fn serialize_seq(self, _: Option<usize>) -> Result<NoKey, NoJson> {
        Err(NoJson)
    }

    fn serialize_tuple(self, _: usize) -> Result<NoKey, NoJson> {
        Err(NoJson)
    }

    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<NoKey, NoJson> {
        Err(NoJson)
    }

    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<NoKey, NoJson> {
        Err(NoJson)
    }

    fn serialize_map(self, _: Option<usize>) -> Result<NoKey, NoJson> {
        Err(NoJson)
    }

    fn serialize_struct(self, _: &'static str, _: usize) -> Result<NoKey, NoJson> {
        Err(NoJson)
    }

    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<NoKey, NoJson> {
        Err(NoJson)
    }
}
