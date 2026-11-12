//! JSON as jq reads and prints it, for the lines of `tehas status` and the
//! frame of `tehas top`, and for the files a cycle leaves: the keys of an
//! object keep the order they were written in, and a number keeps the text it
//! was written with, so 0.10 is printed 0.10. serde_json does neither, and
//! those lines were written in jq.

use serde_json::Value;
use std::cmp::Ordering;
use std::fmt::Write;

#[derive(Clone, Debug, PartialEq)]
pub enum J {
    Null,
    Bool(bool),
    Num(String),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

pub static NULL: J = J::Null;

/// A value serde_json read, as this module holds it. A scalar becomes the
/// variant of its type, never text, so a null read from a file and a null
/// built here are the same value.
impl From<&Value> for J {
    fn from(v: &Value) -> J {
        match v {
            Value::Null => J::Null,
            Value::Bool(b) => J::Bool(*b),
            Value::Number(n) => J::Num(n.to_string()),
            Value::String(s) => J::Str(s.clone()),
            Value::Array(a) => J::Arr(a.iter().map(J::from).collect()),
            Value::Object(o) => J::Obj(o.iter().map(|(k, x)| (k.clone(), J::from(x))).collect()),
        }
    }
}

impl J {
    /// A whole number, as jq prints one.
    pub fn int(n: i64) -> J {
        J::Num(n.to_string())
    }

    pub fn bool(b: bool) -> J {
        J::Bool(b)
    }

    /// The text, or null for an empty one.
    pub fn text_or_null(s: &str) -> J {
        if s.is_empty() {
            J::Null
        } else {
            J::Str(s.to_string())
        }
    }

    /// `del(.key)`.
    pub fn remove(&mut self, key: &str) {
        if let J::Obj(o) = self {
            o.retain(|(k, _)| k != key);
        }
    }

    /// `walk(if type == "string" then f else . end)`.
    pub fn each_string(&mut self, f: &impl Fn(&mut String)) {
        match self {
            J::Str(s) => f(s),
            J::Arr(a) => a.iter_mut().for_each(|x| x.each_string(f)),
            J::Obj(o) => o.iter_mut().for_each(|(_, x)| x.each_string(f)),
            _ => {}
        }
    }

    pub fn parse(text: &str) -> Option<J> {
        let mut p = Parser {
            b: text.as_bytes(),
            i: 0,
        };
        p.white();
        let v = p.value(0)?;
        p.white();
        (p.i == p.b.len()).then_some(v)
    }

    /// The first value of a text, which `jq | head -n1` stops at: what follows
    /// it is not read.
    pub fn parse_first(text: &str) -> Option<J> {
        let mut p = Parser {
            b: text.as_bytes(),
            i: 0,
        };
        p.white();
        p.value(0)
    }

    /// A number worked out here, printed as jq prints one: whole numbers
    /// without a fraction.
    pub fn num(n: f64) -> J {
        if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e17 {
            J::Num(format!("{}", n as i64))
        } else {
            J::Num(format!("{n}"))
        }
    }

    pub fn str(s: impl Into<String>) -> J {
        J::Str(s.into())
    }

    /// `.key`: null for a key that is not there and for anything but an object.
    pub fn get(&self, key: &str) -> &J {
        match self {
            J::Obj(o) => o.iter().find(|(k, _)| k == key).map_or(&NULL, |(_, v)| v),
            _ => &NULL,
        }
    }

    /// `.a.b.c`
    pub fn at(&self, path: &[&str]) -> &J {
        path.iter().fold(self, |v, k| v.get(k))
    }

    pub fn is_null(&self) -> bool {
        matches!(self, J::Null)
    }

    /// What jq counts as true: everything but null and false.
    pub fn truthy(&self) -> bool {
        !matches!(self, J::Null | J::Bool(false))
    }

    /// `a // b`
    pub fn or<'a>(&'a self, other: &'a J) -> &'a J {
        if self.truthy() {
            self
        } else {
            other
        }
    }

    pub fn is_true(&self) -> bool {
        matches!(self, J::Bool(true))
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            J::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            J::Num(n) => n.parse().ok(),
            _ => None,
        }
    }

    pub fn items(&self) -> &[J] {
        match self {
            J::Arr(a) => a,
            _ => &[],
        }
    }

    /// `length`, for an array or an object.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn len(&self) -> usize {
        match self {
            J::Arr(a) => a.len(),
            J::Obj(o) => o.len(),
            J::Str(s) => s.chars().count(),
            _ => 0,
        }
    }

    /// `. > 0`, in jq's order of types: null and booleans sort before every
    /// number, strings, arrays and objects after them.
    pub fn above_zero(&self) -> bool {
        match self {
            J::Null | J::Bool(_) => false,
            J::Num(_) => self.as_f64().is_some_and(|n| n > 0.0),
            _ => true,
        }
    }

    /// `. // "" | tostring`
    pub fn text_or_empty(&self) -> String {
        if self.truthy() {
            self.text()
        } else {
            String::new()
        }
    }

    /// `tostring`, which is also what `"\(.)"` prints.
    pub fn text(&self) -> String {
        match self {
            J::Str(s) => s.clone(),
            other => other.dump(),
        }
    }

    /// The value as `jq -c` prints it.
    pub fn dump(&self) -> String {
        let mut out = String::new();
        self.dump_into(&mut out);
        out
    }

    fn dump_into(&self, out: &mut String) {
        match self {
            J::Null => out.push_str("null"),
            J::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            J::Num(n) => out.push_str(n),
            J::Str(s) => quote(s, out),
            J::Arr(a) => {
                out.push('[');
                for (i, v) in a.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    v.dump_into(out);
                }
                out.push(']');
            }
            J::Obj(o) => {
                out.push('{');
                for (i, (k, v)) in o.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    quote(k, out);
                    out.push(':');
                    v.dump_into(out);
                }
                out.push('}');
            }
        }
    }
}

impl J {
    fn rank(&self) -> u8 {
        match self {
            J::Null => 0,
            J::Bool(false) => 1,
            J::Bool(true) => 2,
            J::Num(_) => 3,
            J::Str(_) => 4,
            J::Arr(_) => 5,
            J::Obj(_) => 6,
        }
    }

    /// jq's order of values: null, false, true, numbers, strings, arrays,
    /// objects; an object by its sorted keys first, then by their values.
    #[allow(clippy::should_implement_trait)]
    pub fn cmp(&self, other: &J) -> Ordering {
        match (self, other) {
            (J::Num(_), J::Num(_)) => {
                let (a, b) = (self.as_f64().unwrap_or(0.0), other.as_f64().unwrap_or(0.0));
                a.partial_cmp(&b).unwrap_or(Ordering::Equal)
            }
            (J::Str(a), J::Str(b)) => a.cmp(b),
            (J::Arr(a), J::Arr(b)) => a
                .iter()
                .zip(b)
                .map(|(x, y)| x.cmp(y))
                .find(|o| o.is_ne())
                .unwrap_or_else(|| a.len().cmp(&b.len())),
            (J::Obj(a), J::Obj(b)) => {
                let (mut ka, mut kb): (Vec<&String>, Vec<&String>) = (
                    a.iter().map(|(k, _)| k).collect(),
                    b.iter().map(|(k, _)| k).collect(),
                );
                ka.sort();
                kb.sort();
                ka.cmp(&kb).then_with(|| {
                    ka.iter()
                        .map(|k| self.get(k).cmp(other.get(k)))
                        .find(|o| o.is_ne())
                        .unwrap_or(Ordering::Equal)
                })
            }
            _ => self.rank().cmp(&other.rank()),
        }
    }

    /// The value as `jq` prints it without -c: two spaces a level.
    pub fn pretty(&self) -> String {
        let mut out = String::new();
        self.pretty_into(&mut out, 0);
        out
    }

    fn pretty_into(&self, out: &mut String, level: usize) {
        let pad = |out: &mut String, n: usize| out.push_str(&"  ".repeat(n));
        match self {
            J::Arr(a) if !a.is_empty() => {
                out.push_str("[\n");
                for (i, v) in a.iter().enumerate() {
                    pad(out, level + 1);
                    v.pretty_into(out, level + 1);
                    out.push_str(if i + 1 < a.len() { ",\n" } else { "\n" });
                }
                pad(out, level);
                out.push(']');
            }
            J::Obj(o) if !o.is_empty() => {
                out.push_str("{\n");
                for (i, (k, v)) in o.iter().enumerate() {
                    pad(out, level + 1);
                    quote(k, out);
                    out.push_str(": ");
                    v.pretty_into(out, level + 1);
                    out.push_str(if i + 1 < o.len() { ",\n" } else { "\n" });
                }
                pad(out, level);
                out.push('}');
            }
            other => other.dump_into(out),
        }
    }
}

/// An object from its fields, in the order given.
pub fn obj(fields: Vec<(&str, J)>) -> J {
    J::Obj(
        fields
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

/// `.[0:n]` of a string, counted in characters.
pub fn first(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn quote(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

// Deeper than this is not a file the factory wrote.
const DEPTH: usize = 256;

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn white(&mut self) {
        while matches!(self.b.get(self.i), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }

    fn eat(&mut self, word: &[u8]) -> Option<()> {
        self.b[self.i..]
            .starts_with(word)
            .then(|| self.i += word.len())
    }

    fn value(&mut self, depth: usize) -> Option<J> {
        if depth > DEPTH {
            return None;
        }
        match *self.b.get(self.i)? {
            b'n' => self.eat(b"null").map(|_| J::Null),
            b't' => self.eat(b"true").map(|_| J::Bool(true)),
            b'f' => self.eat(b"false").map(|_| J::Bool(false)),
            b'"' => self.string().map(J::Str),
            b'[' => {
                self.i += 1;
                let mut items = Vec::new();
                self.white();
                if self.eat(b"]").is_some() {
                    return Some(J::Arr(items));
                }
                loop {
                    self.white();
                    items.push(self.value(depth + 1)?);
                    self.white();
                    if self.eat(b",").is_none() {
                        self.eat(b"]")?;
                        return Some(J::Arr(items));
                    }
                }
            }
            b'{' => {
                self.i += 1;
                let mut fields: Vec<(String, J)> = Vec::new();
                self.white();
                if self.eat(b"}").is_some() {
                    return Some(J::Obj(fields));
                }
                loop {
                    self.white();
                    let key = self.string()?;
                    self.white();
                    self.eat(b":")?;
                    self.white();
                    let v = self.value(depth + 1)?;
                    // A key written twice keeps its first place and its last
                    // value, as jq keeps it.
                    match fields.iter_mut().find(|(k, _)| *k == key) {
                        Some(field) => field.1 = v,
                        None => fields.push((key, v)),
                    }
                    self.white();
                    if self.eat(b",").is_none() {
                        self.eat(b"}")?;
                        return Some(J::Obj(fields));
                    }
                }
            }
            _ => self.number(),
        }
    }

    fn number(&mut self) -> Option<J> {
        let start = self.i;
        let digits = |p: &mut Self| {
            let from = p.i;
            while p.b.get(p.i).is_some_and(u8::is_ascii_digit) {
                p.i += 1;
            }
            p.i > from
        };
        let _ = self.eat(b"-");
        if self.eat(b"0").is_none() && !digits(self) {
            return None;
        }
        if self.eat(b".").is_some() && !digits(self) {
            return None;
        }
        if matches!(self.b.get(self.i), Some(b'e' | b'E')) {
            self.i += 1;
            if matches!(self.b.get(self.i), Some(b'+' | b'-')) {
                self.i += 1;
            }
            if !digits(self) {
                return None;
            }
        }
        let text = std::str::from_utf8(&self.b[start..self.i]).ok()?;
        Some(J::Num(text.to_string()))
    }

    fn hex4(&mut self) -> Option<u32> {
        let h = std::str::from_utf8(self.b.get(self.i..self.i + 4)?).ok()?;
        self.i += 4;
        u32::from_str_radix(h, 16).ok()
    }

    fn string(&mut self) -> Option<String> {
        self.eat(b"\"")?;
        let mut out = String::new();
        loop {
            let start = self.i;
            while !matches!(self.b.get(self.i), Some(b'"' | b'\\') | None) {
                if self.b[self.i] < 0x20 {
                    return None;
                }
                self.i += 1;
            }
            out.push_str(std::str::from_utf8(&self.b[start..self.i]).ok()?);
            match self.b.get(self.i)? {
                b'"' => {
                    self.i += 1;
                    return Some(out);
                }
                _ => {
                    self.i += 1;
                    let e = *self.b.get(self.i)?;
                    self.i += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => out.push(self.unicode()?),
                        _ => return None,
                    }
                }
            }
        }
    }

    /// One `\u` escape. A high surrogate takes a low half at once; without one
    /// it reads as U+FFFD and the escape that follows is read on its own, as
    /// jq reads it.
    fn unicode(&mut self) -> Option<char> {
        let high = self.hex4()?;
        if !(0xd800..0xdc00).contains(&high) {
            return Some(char::from_u32(high).unwrap_or('\u{fffd}'));
        }
        if self.b.get(self.i..self.i + 2) == Some(b"\\u") {
            let at = self.i;
            self.i += 2;
            let low = self.hex4()?;
            if (0xdc00..0xe000).contains(&low) {
                let c = 0x10000 + ((high - 0xd800) << 10) + (low - 0xdc00);
                return Some(char::from_u32(c).unwrap_or('\u{fffd}'));
            }
            self.i = at;
        }
        Some('\u{fffd}')
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_order_and_number_text() {
        let v = J::parse(r#" {"b": 0.10, "a": [1.00, -2e3, null, true], "c": "x\nyé"} "#).unwrap();
        assert_eq!(
            v.dump(),
            r#"{"b":0.10,"a":[1.00,-2e3,null,true],"c":"x\nyé"}"#
        );
        assert_eq!(v.get("b").text(), "0.10");
        assert_eq!(v.get("c").text(), "x\nyé");
    }

    #[test]
    fn a_key_written_twice_keeps_its_place_and_last_value() {
        let v = J::parse(r#"{"a":1,"b":2,"a":3}"#).unwrap();
        assert_eq!(v.dump(), r#"{"a":3,"b":2}"#);
    }

    #[test]
    fn refuses_what_is_not_json() {
        for bad in [
            "",
            "{",
            "{\"a\":}",
            "01x",
            "[1,]",
            "\"a\u{1}\"",
            "1 2",
            "-",
            "1.",
        ] {
            assert_eq!(J::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn numbers_worked_out_print_as_jq_prints_them() {
        assert_eq!(J::num(4.0).text(), "4");
        assert_eq!(J::num(4.5).text(), "4.5");
        assert_eq!(J::num(0.3).text(), "0.3");
    }

    #[test]
    fn surrogate_pairs_and_control_characters() {
        let v = J::parse(r#""😀\u0001\u007f""#).unwrap();
        assert_eq!(v.text(), "😀\u{1}\u{7f}");
        assert_eq!(v.dump(), r#""😀\u0001\u007f""#);
    }

    #[test]
    fn pretty_as_jq_prints_it() {
        let v = J::parse(r#"{"a":[1,{"b":[]}],"c":{}}"#).unwrap();
        assert_eq!(
            v.pretty(),
            "{\n  \"a\": [\n    1,\n    {\n      \"b\": []\n    }\n  ],\n  \"c\": {}\n}"
        );
    }

    #[test]
    fn jq_sort_order() {
        let v = |t: &str| J::parse(t).unwrap();
        assert!(v("null").cmp(&v("false")).is_lt() && v("true").cmp(&v("0")).is_lt());
        assert!(v("10").cmp(&v("9")).is_gt() && v("\"a\"").cmp(&v("[]")).is_lt());
        assert!(v("[1,2]").cmp(&v("[1,3]")).is_lt() && v("[1]").cmp(&v("[1,0]")).is_lt());
    }

    #[test]
    fn jq_truth_and_order() {
        assert!(!J::Null.truthy() && !J::Bool(false).truthy() && J::num(0.0).truthy());
        assert!(J::str("1").above_zero() && !J::Null.above_zero() && !J::num(0.0).above_zero());
    }

    /// The order and the number text of the cycle's file, where serde_json's
    /// Value sorts its keys.
    #[test]
    fn keys_keep_their_order_and_numbers_their_words() {
        let t = r#"{"b":1.50,"a":2.0,"c":100000000000000000000000,"d":[true,null,{}],"e":-0}"#;
        assert_eq!(J::parse(t).unwrap().dump(), t);
    }

    #[test]
    fn a_key_given_twice_keeps_its_place_and_its_last_value() {
        assert_eq!(
            J::parse(r#"{"a":1,"b":2,"a":3}"#).unwrap().dump(),
            r#"{"a":3,"b":2}"#
        );
    }

    #[test]
    fn what_is_no_json_is_none() {
        for t in [
            "",
            "not json",
            "{",
            "[1,",
            r#"{"a"}"#,
            "01",
            "1.",
            "tru",
            "1 2",
            "\"\t\"",
            r#"{"a":1} {"b":2}"#,
        ] {
            assert_eq!(J::parse(t), None, "{t}");
        }
    }

    #[test]
    fn text_is_escaped_as_jq_escapes_it() {
        let v = J::Str("a\"\\\u{8}\t\n\u{c}\r\u{1}\u{7f}é".into());
        assert_eq!(v.dump(), "\"a\\\"\\\\\\b\\t\\n\\f\\r\\u0001\\u007fé\"");
    }

    #[test]
    fn half_a_surrogate_pair_is_the_replacement_character() {
        assert_eq!(
            J::parse(r#""\ud83d x""#).unwrap(),
            J::Str("\u{fffd} x".into())
        );
        assert_eq!(J::parse(r#""😀""#).unwrap(), J::Str("😀".into()));
    }

    /// A high surrogate followed by a `\u` escape that is no low half: the
    /// high one is U+FFFD and the escape after it is read on its own.
    #[test]
    fn a_high_surrogate_followed_by_a_lone_escape_reads_each_on_its_own() {
        assert_eq!(
            J::parse(r#""\ud83dA""#).unwrap(),
            J::Str("\u{fffd}A".into())
        );
        assert_eq!(
            J::parse(r#""\ud83d\ud83d""#).unwrap(),
            J::Str("\u{fffd}\u{fffd}".into())
        );
    }

    #[test]
    fn pretty_is_jqs() {
        let v = J::parse(r#"{"a":1,"b":{"c":[1,"x"],"d":{}},"e":[]}"#).unwrap();
        assert_eq!(
            v.pretty(),
            "{\n  \"a\": 1,\n  \"b\": {\n    \"c\": [\n      1,\n      \"x\"\n    ],\n    \"d\": {}\n  },\n  \"e\": []\n}"
        );
    }

    #[test]
    fn a_key_of_an_object_can_be_removed() {
        let mut v = J::parse(r#"{"a":1,"id":"x","b":2}"#).unwrap();
        v.remove("id");
        assert_eq!(v.dump(), r#"{"a":1,"b":2}"#);
        // del of a key that is not there, and of anything but an object.
        v.remove("nothing");
        assert_eq!(v.dump(), r#"{"a":1,"b":2}"#);
        let mut n = J::parse("[1,2]").unwrap();
        n.remove("a");
        assert_eq!(n.dump(), "[1,2]");
    }

    #[test]
    fn every_string_of_a_tree_can_be_rewritten() {
        let mut v = J::parse(r#"{"a":"abc","b":["de",["e"],1],"c":null,"d":true}"#).unwrap();
        v.each_string(&|s| s.push('!'));
        assert_eq!(
            v.dump(),
            r#"{"a":"abc!","b":["de!",["e!"],1],"c":null,"d":true}"#
        );
    }

    #[test]
    fn an_empty_text_is_null() {
        assert_eq!(J::text_or_null(""), J::Null);
        assert_eq!(J::text_or_null("x"), J::Str("x".into()));
        assert!(J::text_or_null("").is_null());
    }

    /// A value serde_json read becomes the variant of its own type, so a
    /// scalar read from a file is what one built here is. The keys of a
    /// serde_json map are sorted, which is why the cycle reads its JSON here.
    #[test]
    fn a_value_serde_json_read_becomes_the_value_of_its_type() {
        let from: J =
            J::from(&serde_json::from_str::<Value>(r#"{"a":"x","n":1.50,"t":true}"#).unwrap());
        assert_eq!(from.dump(), r#"{"a":"x","n":1.5,"t":true}"#);
        let text = r#"{"at":"T","yes":true,"no":false,"gone":null,"a":[1,"x",[true]],"o":{"k":null},"big":1.50}"#;
        let from: J = J::from(&serde_json::from_str::<Value>(text).unwrap());
        assert_eq!(from.get("at"), &J::str("T"));
        assert_eq!(from.get("gone"), &J::Null);
        assert!(from.get("gone").is_null());
        assert_eq!(from.get("yes"), &J::Bool(true));
        assert_eq!(from.get("no"), &J::Bool(false));
        assert_eq!(from.get("big"), &J::Num("1.5".into()));
        assert_eq!(from.get("a"), &J::parse("[1,\"x\",[true]]").unwrap());
        assert_eq!(from.get("o").get("k"), &J::Null);
        assert_eq!(J::from(&Value::Null), J::Null);
        assert_eq!(J::from(&serde_json::json!(7)), J::Num("7".into()));
        assert_eq!(J::from(&serde_json::json!(null)), J::Null);
    }

    /// The parser calls itself for every level, so a document nested deeper
    /// than DEPTH is refused rather than run out of the thread's stack: the
    /// array of 0 is the 256th level in one and the 257th in the other.
    #[test]
    fn a_document_is_read_to_its_depth_and_no_further() {
        let deep = |n: usize| {
            let mut t = String::new();
            for _ in 0..n {
                t.push('[');
            }
            t.push('0');
            for _ in 0..n {
                t.push(']');
            }
            t
        };
        assert!(J::parse(&deep(255)).is_some());
        assert!(J::parse(&deep(256)).is_some());
        assert_eq!(J::parse(&deep(257)), None);
    }
}
