//! The reading of an instance's `state/limit.json`, the limit `tehas limit`
//! sets. Every reader of the file asks here, so the console, the dispatcher and
//! the health page give the same answer for the same bytes. `flags.rs` in the
//! console says what the file holds and the words a reading prints.
//!
//! `state` is `none` when no file is there, `ok`, `expired` once `until` has
//! passed, and `unreadable` when what is there is not a limit of version 1,
//! including an object which names one key twice: a reader could keep either
//! value, so no reader keeps one.

use serde_json::Value;
use std::fs;
use std::path::Path;

/// The longest reason the flag holds.
pub const REASON: usize = 200;

/// A reading of the limit: its state, and its fields in the order they print,
/// each as JSON text.
#[derive(Debug, PartialEq, Clone)]
pub struct Limit {
    pub state: &'static str,
    pub fields: Vec<(String, String)>,
}

impl Limit {
    pub fn of(state: &'static str) -> Limit {
        Limit {
            state,
            fields: Vec::new(),
        }
    }

    /// The reading as one compact object, `state` first: the object
    /// `console flags limit read` prints.
    pub fn json(&self) -> String {
        let mut parts = vec![format!("\"state\":\"{}\"", self.state)];
        parts.extend(
            self.fields
                .iter()
                .map(|(name, value)| format!("\"{name}\":{value}")),
        );
        format!("{{{}}}", parts.join(","))
    }

    /// The reading as a JSON value. The text of `json` is always JSON; were
    /// it not, the reading would be no limit anyone can act on.
    pub fn value(&self) -> Value {
        serde_json::from_str(&self.json())
            .unwrap_or_else(|_| serde_json::json!({"state": "unreadable"}))
    }
}

/// The longest prefix of the text of a value which jq's strtof reads as a
/// number: an optional sign, digits, a point and digits, an exponent, and
/// nothing more. jq stops at the first byte which cannot belong to the number
/// and reads what came before, so `1x` is the number 1 and `1 ` is the
/// number 1 with a space nobody asked about after it.
pub fn leading_number(text: &str) -> Option<&str> {
    let b = text.as_bytes();
    let mut i = 0;
    if matches!(b.first(), Some(b'+') | Some(b'-')) {
        i += 1;
    }
    let mut digits = false;
    while b.get(i).is_some_and(u8::is_ascii_digit) {
        digits = true;
        i += 1;
    }
    if b.get(i) == Some(&b'.') {
        i += 1;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            digits = true;
            i += 1;
        }
    }
    if matches!(b.get(i), Some(b'e') | Some(b'E')) {
        let save = i;
        i += 1;
        if matches!(b.get(i), Some(b'+') | Some(b'-')) {
            i += 1;
        }
        let mut power = false;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            power = true;
            i += 1;
        }
        if !power {
            i = save;
        }
    }
    digits.then(|| &text[..i])
}

/// The limit as every command which honours it reads it.
/// A file which is not there is no limit; anything else which is not a limit
/// of version 1 is unreadable, never none, so a reader holds every start
/// rather than finding no limit where a file is.
pub fn read(file: &Path, now: i64) -> Limit {
    match fs::symlink_metadata(file) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Limit::of("none"),
        Err(_) => return Limit::of("unreadable"),
        Ok(_) => {}
    }
    match fs::read(file) {
        Ok(bytes) => parse(&bytes, now),
        Err(_) => Limit::of("unreadable"),
    }
}

/// The reading of the bytes of a file, unreadable when they are no limit of
/// version 1. The checks are jq's select, one for one, including what an
/// absent key reads as: `.until == null` is true of a document with no until
/// at all. The cycles are plain digits that fit an i64, printed as written.
pub fn parse(bytes: &[u8], now: i64) -> Limit {
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => return Limit::of("unreadable"),
    };
    // The checks of the limit, one jq select at a time, over the document
    // read. What an absent key reads as is part of the check:
    // `.until == null` is true of a document with no until at all.
    let keys = match Skip::new(bytes).object() {
        Some(keys) => keys,
        // The scanner reads a number as jq's reader does, from its front to
        // the first byte which cannot belong to it, so a document which is a
        // document reaches the checks; what is no document at all is no
        // limit. Only `v` is read that forgivingly: the cycles must be plain
        // digits, checked below.
        None => return Limit::of("unreadable"),
    };
    // jq and serde keep the last of a key named twice and a scan for the key
    // finds the first, so such a file is no limit to anyone.
    if keys
        .iter()
        .enumerate()
        .any(|(i, (name, _))| keys[..i].iter().any(|(seen, _)| seen == name))
    {
        return Limit::of("unreadable");
    }
    let held = |want: &str| {
        keys.iter()
            .find(|(name, _)| name == want)
            .map(|(_, at)| &text[at.0..at.1])
    };
    match held("v") {
        Some(value) if leading_number(value).is_some_and(|n| n.parse::<f64>() == Ok(1.0)) => {}
        _ => return Limit::of("unreadable"),
    }
    let cycles = match held("cycles") {
        Some(value) => value,
        None => return Limit::of("unreadable"),
    };
    // Plain digits, no leading zero, and a number the dispatcher can hold:
    // it reads the field as an integer, and `1.0` or `01` would read there
    // as no number, which it would take for 0 cycles while this said ok. A
    // number over what tehas limit writes is still a limit.
    let whole = !cycles.is_empty()
        && cycles.bytes().all(|b| b.is_ascii_digit())
        && (cycles == "0" || !cycles.starts_with('0'))
        && cycles.parse::<i64>().is_ok();
    if !whole {
        return Limit::of("unreadable");
    }
    match held("plan") {
        Some("true") | Some("false") => {}
        _ => return Limit::of("unreadable"),
    }
    let until = match held("until") {
        None | Some("null") => None,
        Some(value) => match quoted(value) {
            Some(s) => Some(s),
            None => return Limit::of("unreadable"),
        },
    };
    let expired = match &until {
        Some(s) => match epoch(s) {
            Some(at) => at <= now,
            None => return Limit::of("unreadable"),
        },
        None => false,
    };
    let reason = match held("reason") {
        None | Some("null") => String::new(),
        Some(value) => match quoted(value) {
            Some(s) => oneline(&s),
            None => return Limit::of("unreadable"),
        },
    };
    let mut out = Limit::of(if expired { "expired" } else { "ok" });
    out.fields.push(("cycles".into(), cycles.to_string()));
    out.fields
        .push(("plan".into(), held("plan").unwrap_or("null").to_string()));
    if let Some(s) = until {
        out.fields
            .push(("until".into(), Value::String(s).to_string()));
    }
    if !reason.is_empty() {
        out.fields
            .push(("reason".into(), Value::String(reason).to_string()));
    }
    out
}

/// The top-level keys of a JSON object and where each value's bytes lie, or
/// nothing when the bytes are no object.
pub fn object(bytes: &[u8]) -> Option<Vec<(String, (usize, usize))>> {
    Skip::new(bytes).object()
}

/// The text of a quoted string, with its escapes as they came. Only a reason
/// and an until are wanted, and both are text.
pub fn quoted(value: &str) -> Option<String> {
    let unescaped = value.strip_prefix('"')?.strip_suffix('"')?;
    // The escapes of a JSON string become the characters the file meant, the
    // same way jq's parser does it, so that the reason of a limit is the
    // reason its author wrote.
    let mut out = String::with_capacity(unescaped.len());
    let mut chars = unescaped.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next()? {
            '"' => out.push('"'),
            '\\' => out.push('\\'),
            '/' => out.push('/'),
            'b' => out.push('\u{8}'),
            'f' => out.push('\u{c}'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            'u' => {
                let mut code = String::new();
                code.extend(chars.by_ref().take(4));
                let at = u32::from_str_radix(&code, 16).ok()?;
                out.push(char::from_u32(at).unwrap_or('\u{fffd}'));
            }
            _ => return None,
        }
    }
    Some(out)
}

/// The hand which walks the bytes of a document. `object` is jq's own first
/// question: is this a document at all, and what does each of its top-level
/// keys hold? The bytes of every value are kept as the document holds them, so
/// a number prints again the way it came in, and a name buried in a reason is
/// never taken for a second reading of the limit.
struct Skip<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Skip<'a> {
    fn new(bytes: &'a [u8]) -> Skip<'a> {
        Skip { bytes, at: 0 }
    }

    fn object(mut self) -> Option<Vec<(String, (usize, usize))>> {
        self.white();
        self.open(b'{')?;
        self.white();
        let mut keys = Vec::new();
        if self.peek() == Some(b'}') {
            return Some(keys);
        }
        loop {
            self.white();
            let name = self.name()?;
            self.white();
            self.open(b':')?;
            self.white();
            let from = self.at;
            self.value()?;
            keys.push((name, (from, self.at)));
            self.white();
            if self.peek() == Some(b',') {
                self.at += 1;
                continue;
            }
            self.open(b'}')?;
            return Some(keys);
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    /// One byte of structure, or nothing for the reader to refuse.
    fn open(&mut self, b: u8) -> Option<()> {
        (self.peek() == Some(b)).then(|| self.at += 1)
    }

    /// jq's whitespace: the space, the tab, the newline and the return.
    fn white(&mut self) {
        while matches!(
            self.peek(),
            Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r')
        ) {
            self.at += 1;
        }
    }

    /// The text of a key, its escapes become what they stand for, so a key
    /// written `\u0063ycles` is the cycles of the file.
    fn name(&mut self) -> Option<String> {
        let from = self.at;
        self.string()?;
        let raw = std::str::from_utf8(&self.bytes[from..self.at]).ok()?;
        quoted(raw)
    }

    /// A string, with its escapes kept as they are: the bytes of a reason are
    /// the bytes of the file, and the escapes are let go only when the text
    /// of the reason is wanted.
    fn string(&mut self) -> Option<()> {
        self.open(b'"')?;
        while let Some(c) = self.peek() {
            match c {
                b'\\' => self.at += 2,
                b'"' => {
                    self.at += 1;
                    return Some(());
                }
                _ => self.at += 1,
            }
        }
        None
    }

    /// The bytes of any value: a literal, a string, a number, an array or an
    /// object, each entered only as far as it reaches.
    fn value(&mut self) -> Option<()> {
        match self.peek()? {
            b'"' => self.string(),
            b'[' | b'{' => {
                let close = if self.peek() == Some(b'[') {
                    b']'
                } else {
                    b'}'
                };
                self.at += 1;
                self.white();
                if self.peek() == Some(close) {
                    self.at += 1;
                    return Some(());
                }
                let nested = close == b'}';
                loop {
                    if nested {
                        self.white();
                        self.string()?;
                        self.white();
                        self.open(b':')?;
                        self.white();
                    }
                    self.value()?;
                    self.white();
                    if self.peek() == Some(b',') {
                        self.at += 1;
                        continue;
                    }
                    return self.open(close);
                }
            }
            b't' => self.lit(b"true"),
            b'f' => self.lit(b"false"),
            b'n' => self.lit(b"null"),
            _ => {
                let from = self.at;
                while matches!(self.peek(), Some(c) if matches!(c, b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'))
                {
                    self.at += 1;
                }
                (self.at > from).then_some(())
            }
        }
    }

    fn lit(&mut self, word: &[u8]) -> Option<()> {
        if self.bytes[self.at..].starts_with(word) {
            self.at += word.len();
            return Some(());
        }
        None
    }
}

/// Whether `tehas_pause_oneline` replaces this character with a space: the C0 and C1
/// controls, U+2028 and U+2029. The space itself is not one of them.
fn replaced(c: char) -> bool {
    matches!(c, '\u{1}'..='\u{1f}' | '\u{7f}'..='\u{9f}' | '\u{2028}' | '\u{2029}')
}

/// A reason as the health file holds it: a run of the characters jq replaces becomes one space, a run of
/// spaces stays as it was written, 200 characters is the most, counted as
/// characters and cut before the ends, and the ends go last. The line
/// `tehas status` prints is one fact, so a reason which carried a newline
/// would otherwise print a line which is not there.
pub fn oneline(s: &str) -> String {
    // Each replaced character becomes a space of its own and a run of spaces
    // which were written keeps every character of itself; 200 characters is
    // the most, cut before the ends go, and only the spaces which touch the
    // two ends of what is left go with them.
    let folded: String = s
        .chars()
        .map(|c| if replaced(c) { ' ' } else { c })
        .collect();
    let cut = folded.chars().take(REASON).collect::<String>();
    cut.trim_matches(edges).to_string()
}

fn edges(c: char) -> bool {
    c == ' ' || replaced(c)
}

/// Seconds since the epoch of an ISO 8601 UTC time, as jq's `fromdateiso8601`
/// reads it: the shape is checked byte for byte, and a day out of range is
/// counted on rather than refused, as jq counts it.
pub fn epoch(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let digits =
        |r: std::ops::Range<usize>| b.get(r).is_some_and(|w| w.iter().all(u8::is_ascii_digit));
    if b.len() != 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || b[19] != b'Z'
        || !digits(0..4)
        || !digits(5..7)
        || !digits(8..10)
        || !digits(11..13)
        || !digits(14..16)
        || !digits(17..19)
    {
        return None;
    }
    let at = |r: std::ops::Range<usize>| s[r].parse::<i64>().ok();
    let (y, m, d) = (at(0..4)?, at(5..7)?, at(8..10)?);
    let (hh, mm, ss) = (at(11..13)?, at(14..16)?, at(17..19)?);
    if !(1..=12).contains(&m) || hh > 23 || mm > 59 || ss > 60 {
        return None;
    }
    Some(crate::time::days_from_civil(y, m, d) * 86_400 + hh * 3600 + mm * 60 + ss)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_792_245_620; // 2026-10-17T14:00:20Z

    fn state(text: &str) -> &'static str {
        parse(text.as_bytes(), NOW).state
    }

    #[test]
    fn a_key_named_twice_is_unreadable() {
        assert_eq!(state(r#"{"v":1,"cycles":1,"plan":true}"#), "ok");
        assert_eq!(
            state(r#"{"v":1,"cycles":1,"plan":true,"cycles":2}"#),
            "unreadable"
        );
        assert_eq!(
            state(r#"{"v":1,"cycles":1,"plan":true,"cycles":1}"#),
            "unreadable"
        );
        assert_eq!(
            state(r#"{"v":1,"cycles":1,"plan":true,"x":1,"x":2}"#),
            "unreadable"
        );
        // A key inside a value is not a key of the limit.
        assert_eq!(
            state(r#"{"v":1,"cycles":1,"plan":true,"reason":"cycles","o":{"v":2}}"#),
            "ok"
        );
    }

    #[test]
    fn the_reading_is_always_json() {
        for cycles in ["0", "1", "12", "9223372036854775807"] {
            let l = parse(
                format!(r#"{{"v":1,"cycles":{cycles},"plan":true}}"#).as_bytes(),
                NOW,
            );
            assert_eq!(l.state, "ok", "{cycles}");
            assert_eq!(l.fields[0], ("cycles".into(), cycles.into()), "{cycles}");
            let read: Value = serde_json::from_str(&l.json()).unwrap();
            assert!(read["cycles"].as_i64().is_some(), "{cycles}");
        }
    }

    #[test]
    fn cycles_are_a_whole_number_every_reader_reads_as_one() {
        // The dispatcher reads the field as an integer: a number it would
        // read as none is no limit, not a limit of 0 cycles.
        for cycles in [
            "01",
            "1.",
            "1.0",
            "-0",
            "+1",
            "1e2",
            "1e999",
            "-1",
            "1.5",
            "9223372036854775808",
        ] {
            assert_eq!(
                state(&format!(r#"{{"v":1,"cycles":{cycles},"plan":true}}"#)),
                "unreadable",
                "{cycles}"
            );
        }
    }

    #[test]
    fn value_is_the_object_json_prints() {
        let l = parse(
            br#"{"v":1,"cycles":2,"plan":false,"until":"2026-10-17T13:00:00Z"}"#,
            NOW,
        );
        assert_eq!(
            l.value(),
            serde_json::json!({"state":"expired","cycles":2,"plan":false,"until":"2026-10-17T13:00:00Z"})
        );
        assert_eq!(
            Limit::of("none").value(),
            serde_json::json!({"state":"none"})
        );
    }
}
