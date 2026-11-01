//! JSON as jq prints it, for the files a cycle leaves: keys stay in the order
//! they came in, a number stays as it was written, and text is escaped as jq
//! escapes it. serde_json's `Value` sorts its keys here.
//!
//! The parser takes standard JSON, as many levels deep as jq takes. An escape
//! of half a surrogate pair reads as U+FFFD, as in jq. jq writes an exponent
//! out (1e3 prints 1000); this keeps it.

use serde_json::Value;
use std::fmt::Write;

/// jq's own limit on nesting.
const DEPTH: usize = 10_000;

#[derive(Clone, Debug, PartialEq)]
pub enum J {
    /// null, true, false or a number, as written.
    Lit(String),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

impl From<&Value> for J {
    fn from(v: &Value) -> J {
        match v {
            Value::String(s) => J::Str(s.clone()),
            Value::Array(a) => J::Arr(a.iter().map(J::from).collect()),
            Value::Object(o) => J::Obj(o.iter().map(|(k, x)| (k.clone(), J::from(x))).collect()),
            other => J::Lit(other.to_string()),
        }
    }
}

impl J {
    pub fn str(s: &str) -> J {
        J::Str(s.to_string())
    }

    pub fn null() -> J {
        J::Lit("null".into())
    }

    pub fn int(n: i64) -> J {
        J::Lit(n.to_string())
    }

    pub fn bool(b: bool) -> J {
        J::Lit(b.to_string())
    }

    /// An object of these fields, in this order.
    pub fn obj(fields: Vec<(&str, J)>) -> J {
        J::Obj(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }

    /// The text, or null for an empty one.
    pub fn text_or_null(s: &str) -> J {
        if s.is_empty() {
            J::null()
        } else {
            J::str(s)
        }
    }

    /// One object, array or scalar from a text that holds nothing else.
    pub fn parse(text: &str) -> Option<J> {
        let mut p = Parser {
            b: text.as_bytes(),
            i: 0,
        };
        let v = p.value(0)?;
        p.blank();
        (p.i == p.b.len()).then_some(v)
    }

    pub fn get(&self, key: &str) -> Option<&J> {
        match self {
            J::Obj(o) => o.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
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
            J::Lit(_) => {}
        }
    }

    /// As `jq -c` prints it.
    pub fn compact(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, None, 0);
        out
    }

    /// As `jq` prints it, with two spaces and no line end.
    pub fn pretty(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, Some(2), 0);
        out
    }

    fn write(&self, out: &mut String, step: Option<usize>, at: usize) {
        let line = |out: &mut String, n: usize| {
            if step.is_some() {
                out.push('\n');
                out.extend(std::iter::repeat_n(' ', n));
            }
        };
        let width = step.unwrap_or(0);
        match self {
            J::Lit(l) => out.push_str(l),
            J::Str(s) => quote(out, s),
            J::Arr(a) if a.is_empty() => out.push_str("[]"),
            J::Obj(o) if o.is_empty() => out.push_str("{}"),
            J::Arr(a) => {
                out.push('[');
                for (i, x) in a.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    line(out, at + width);
                    x.write(out, step, at + width);
                }
                line(out, at);
                out.push(']');
            }
            J::Obj(o) => {
                out.push('{');
                for (i, (k, x)) in o.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    line(out, at + width);
                    quote(out, k);
                    out.push(':');
                    if step.is_some() {
                        out.push(' ');
                    }
                    x.write(out, step, at + width);
                }
                line(out, at);
                out.push('}');
            }
        }
    }
}

fn quote(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c if c < ' ' || c == '\u{7f}' => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn blank(&mut self) {
        while matches!(self.b.get(self.i), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }

    fn eat(&mut self, c: u8) -> bool {
        self.blank();
        let there = self.b.get(self.i) == Some(&c);
        if there {
            self.i += 1;
        }
        there
    }

    fn value(&mut self, depth: usize) -> Option<J> {
        if depth > DEPTH {
            return None;
        }
        self.blank();
        match *self.b.get(self.i)? {
            b'{' => self.object(depth),
            b'[' => self.array(depth),
            b'"' => self.string().map(J::Str),
            _ => self.literal(),
        }
    }

    fn object(&mut self, depth: usize) -> Option<J> {
        self.i += 1;
        let mut o: Vec<(String, J)> = vec![];
        if self.eat(b'}') {
            return Some(J::Obj(o));
        }
        loop {
            self.blank();
            let k = self.string()?;
            if !self.eat(b':') {
                return None;
            }
            let v = self.value(depth + 1)?;
            // A key given twice: the last value, where the first stood.
            match o.iter_mut().find(|(x, _)| *x == k) {
                Some(slot) => slot.1 = v,
                None => o.push((k, v)),
            }
            if self.eat(b'}') {
                return Some(J::Obj(o));
            }
            if !self.eat(b',') {
                return None;
            }
        }
    }

    fn array(&mut self, depth: usize) -> Option<J> {
        self.i += 1;
        let mut a = vec![];
        if self.eat(b']') {
            return Some(J::Arr(a));
        }
        loop {
            a.push(self.value(depth + 1)?);
            if self.eat(b']') {
                return Some(J::Arr(a));
            }
            if !self.eat(b',') {
                return None;
            }
        }
    }

    fn literal(&mut self) -> Option<J> {
        let start = self.i;
        while matches!(
            self.b.get(self.i),
            Some(b'-' | b'+' | b'.' | b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z')
        ) {
            self.i += 1;
        }
        let t = std::str::from_utf8(&self.b[start..self.i]).ok()?;
        let number = |t: &str| {
            let t = t.strip_prefix('-').unwrap_or(t);
            let (whole, rest) = t.split_at(t.find(['.', 'e', 'E']).unwrap_or(t.len()));
            let plain = |s: &str| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit());
            let exp = |s: &str| plain(s.strip_prefix(['+', '-']).unwrap_or(s));
            let frac_ok = match rest.strip_prefix('.') {
                Some(f) => match f.split_once(['e', 'E']) {
                    Some((m, e)) => plain(m) && exp(e),
                    None => plain(f),
                },
                None => match rest.strip_prefix(['e', 'E']) {
                    Some(e) => exp(e),
                    None => rest.is_empty(),
                },
            };
            plain(whole) && (whole == "0" || !whole.starts_with('0')) && frac_ok
        };
        (matches!(t, "null" | "true" | "false") || number(t)).then(|| J::Lit(t.to_string()))
    }

    fn hex4(&mut self) -> Option<u32> {
        let h = self.b.get(self.i..self.i + 4)?;
        let n = u32::from_str_radix(std::str::from_utf8(h).ok()?, 16).ok()?;
        self.i += 4;
        Some(n)
    }

    fn string(&mut self) -> Option<String> {
        if self.b.get(self.i) != Some(&b'"') {
            return None;
        }
        self.i += 1;
        let mut out: Vec<u8> = vec![];
        loop {
            match *self.b.get(self.i)? {
                b'"' => {
                    self.i += 1;
                    return Some(String::from_utf8_lossy(&out).into_owned());
                }
                b'\\' => {
                    self.i += 1;
                    let c = *self.b.get(self.i)?;
                    self.i += 1;
                    let one = match c {
                        b'"' | b'\\' | b'/' => c,
                        b'b' => 8,
                        b'f' => 12,
                        b'n' => b'\n',
                        b'r' => b'\r',
                        b't' => b'\t',
                        b'u' => {
                            let c = self.unicode()?;
                            let mut tmp = [0u8; 4];
                            out.extend_from_slice(c.encode_utf8(&mut tmp).as_bytes());
                            continue;
                        }
                        _ => return None,
                    };
                    out.push(one);
                }
                c if c < 0x20 => return None,
                c => {
                    out.push(c);
                    self.i += 1;
                }
            }
        }
    }

    fn unicode(&mut self) -> Option<char> {
        let high = self.hex4()?;
        if !(0xD800..0xDC00).contains(&high) {
            return Some(char::from_u32(high).unwrap_or('\u{fffd}'));
        }
        // A low half must follow at once, or the high one stands alone.
        if self.b.get(self.i..self.i + 2) == Some(b"\\u") {
            let at = self.i;
            self.i += 2;
            let low = self.hex4()?;
            if (0xDC00..0xE000).contains(&low) {
                let c = 0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00);
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
    fn keys_keep_their_order_and_numbers_their_words() {
        let t = r#"{"b":1.50,"a":2.0,"c":100000000000000000000000,"d":[true,null,{}],"e":-0}"#;
        assert_eq!(J::parse(t).unwrap().compact(), t);
    }

    #[test]
    fn a_key_given_twice_keeps_its_place_and_its_last_value() {
        assert_eq!(
            J::parse(r#"{"a":1,"b":2,"a":3}"#).unwrap().compact(),
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
        assert_eq!(v.compact(), "\"a\\\"\\\\\\b\\t\\n\\f\\r\\u0001\\u007fé\"");
    }

    #[test]
    fn half_a_surrogate_pair_is_the_replacement_character() {
        assert_eq!(
            J::parse(r#""\ud83d x""#).unwrap(),
            J::Str("\u{fffd} x".into())
        );
        assert_eq!(J::parse(r#""😀""#).unwrap(), J::Str("😀".into()));
    }

    #[test]
    fn pretty_is_jqs() {
        let v = J::parse(r#"{"a":1,"b":{"c":[1,"x"],"d":{}},"e":[]}"#).unwrap();
        assert_eq!(
            v.pretty(),
            "{\n  \"a\": 1,\n  \"b\": {\n    \"c\": [\n      1,\n      \"x\"\n    ],\n    \"d\": {}\n  },\n  \"e\": []\n}"
        );
    }
}
